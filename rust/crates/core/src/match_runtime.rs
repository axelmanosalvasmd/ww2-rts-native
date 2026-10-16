//! Native match startup. The setup plan is shared by the authoritative constructor.
pub use crate::objectives::Objective;
#[path = "match_recruitment.rs"]
mod recruitment;
pub use recruitment::{CompletedRecruitUnit, RecruitError, RecruitMetadata, RecruitRouting};
#[path = "match_production.rs"]
mod production_step;
pub use production_step::{CompletedTrainingUnit, ProductionStepError};
use std::collections::BTreeMap;

use crate::{
    catalog::{Catalog, Value},
    deployment::{self, LayoutSelection},
    economy::{Faction, Mode, TechState},
    economy_state::{ArmySize, EconomyState, PlayerEconomy},
    game::{CELL, Game, MOVE, Player, Point},
    grid::SpatialGrid,
    math,
    navigation::{TerrainView, Versions},
    random::{MatchRandom, StreamId},
    rules::Rules,
    unit_spawn::{self, SpawnPlacer},
    weather::{MapWeatherInfo, WeatherChoice, WeatherKind, WeatherPlan, WeatherRules},
    world::World,
};

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct PlayerConfig {
    pub name: String,
    pub team: i32,
    pub faction: Faction,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct MatchOptions {
    pub seed: u64,
    pub mode: Mode,
    pub players: Vec<PlayerConfig>,
    pub shuffle: bool,
    pub army: ArmySize,
    pub weather: WeatherChoice,
    pub weather_seed: Option<u32>,
    pub living_weather: bool,
    pub map_key: Option<String>,
    pub tech: bool,
    pub supply: bool,
    pub logistics: bool,
    pub sandbox: bool,
}

impl MatchOptions {
    /// Resolve headless opt-ins against the prototype mode switches (sim.js:729,746,799).
    /// Authored-map logistics selection belongs to the server, before these mode guards.
    pub fn resolve_mode_features(&mut self) {
        self.logistics &= matches!(
            self.mode,
            Mode::Default | Mode::Conquest | Mode::Classic | Mode::Annihilation | Mode::World
        );
        self.tech &= matches!(
            self.mode,
            Mode::Default
                | Mode::Conquest
                | Mode::Assault
                | Mode::Annihilation
                | Mode::Horde
                | Mode::Classic
                | Mode::World
        );
        self.supply &= !matches!(self.mode, Mode::Horde | Mode::Tutorial | Mode::World);
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct PlayerStart {
    pub config: PlayerConfig,
    pub spawn: Point,
    pub support: BTreeMap<String, f64>,
}

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
pub struct LivingWeather {
    pub raining: bool,
    pub next: f64,
}

/// Materialized startup choices. The RNG records exactly the streams used to make them.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct StartPlan {
    pub dimensions: (usize, usize),
    pub players: Vec<PlayerStart>,
    pub economy: EconomyState,
    pub points: Vec<Objective>,
    pub win_vp: f64,
    pub weather: WeatherPlan,
    pub weather_seed: u32,
    /// Mutable environment RNG state, present even with showers disabled.
    pub environment_seed: u32,
    pub living: Option<LivingWeather>,
    pub wind: (f64, f64),
    pub initial_rain: f64,
    pub initial_wet: Option<f64>,
    pub random: MatchRandom,
    pub starting_kinds: Vec<usize>,
    pub(crate) shared_defenders: Option<usize>,
}

fn field<'a>(v: &'a Value, name: &str) -> Result<&'a Value, String> {
    v.get(name)
        .ok_or_else(|| format!("missing startup rule {name}"))
}
fn number(v: &Value, name: &str) -> Result<f64, String> {
    field(v, name)?
        .number()
        .filter(|n| n.is_finite())
        .ok_or_else(|| format!("invalid startup number {name}"))
}

enum StartupProfile {
    Conquest,
    Elimination,
    Tutorial,
    Horde,
    World(MatchRandom),
    Assault { defender_team: i32 },
}

impl StartPlan {
    pub fn prepare(
        map: &crate::map::AuthoritativeMap,
        options: &MatchOptions,
        rules: &Rules,
    ) -> Result<Self, String> {
        Self::prepare_mode(
            map,
            options,
            rules,
            StartupProfile::Conquest,
            #[cfg(test)]
            None,
        )
    }

    /// Rebuild the bound Classic or Annihilation startup layout.
    pub fn prepare_elimination(
        map: &crate::map::AuthoritativeMap,
        options: &MatchOptions,
        rules: &Rules,
    ) -> Result<Self, String> {
        Self::prepare_mode(
            map,
            options,
            rules,
            StartupProfile::Elimination,
            #[cfg(test)]
            None,
        )
    }

    pub(crate) fn prepare_tutorial(
        map: &crate::map::AuthoritativeMap,
        options: &MatchOptions,
        rules: &Rules,
    ) -> Result<Self, String> {
        Self::prepare_mode(
            map,
            options,
            rules,
            StartupProfile::Tutorial,
            #[cfg(test)]
            None,
        )
    }

    pub(crate) fn prepare_world(
        map: &crate::map::AuthoritativeMap,
        options: &MatchOptions,
        rules: &Rules,
        random: MatchRandom,
    ) -> Result<Self, String> {
        Self::prepare_mode(
            map,
            options,
            rules,
            StartupProfile::World(random),
            #[cfg(test)]
            None,
        )
    }

    /// Rebuild the bound Assault layout with the selected defender team.
    pub fn prepare_assault(
        map: &crate::map::AuthoritativeMap,
        options: &MatchOptions,
        rules: &Rules,
        defender_team: i32,
    ) -> Result<Self, String> {
        Self::prepare_mode(
            map,
            options,
            rules,
            StartupProfile::Assault { defender_team },
            #[cfg(test)]
            None,
        )
    }

    pub fn prepare_horde(
        map: &crate::map::AuthoritativeMap,
        options: &MatchOptions,
        rules: &Rules,
    ) -> Result<Self, String> {
        Self::prepare_mode(
            map,
            options,
            rules,
            StartupProfile::Horde,
            #[cfg(test)]
            None,
        )
    }

    fn prepare_mode(
        map: &crate::map::AuthoritativeMap,
        options: &MatchOptions,
        rules: &Rules,
        profile: StartupProfile,
        #[cfg(test)] replay: Option<crate::aligned_replay::Replay>,
    ) -> Result<Self, String> {
        let elimination_mode = matches!(&profile, StartupProfile::Elimination);
        if elimination_mode && !matches!(options.mode, Mode::Annihilation | Mode::Classic) {
            return Err("elimination startup requires Annihilation or Classic".into());
        }
        let world_mode = matches!(&profile, StartupProfile::World(_));
        let tutorial = matches!(&profile, StartupProfile::Tutorial);
        if let Some(data) = crate::scenario_validation::explicit_scenario(map.source()) {
            crate::authored_scenario::validate_definition(
                data,
                options.players.len(),
                options.mode,
            )?;
        }
        let horde_mode = matches!(&profile, StartupProfile::Horde);
        let defender_team = match &profile {
            StartupProfile::Assault { defender_team } => Some(*defender_team),
            _ => None,
        };
        if map.geography().is_some() != world_mode || (world_mode && options.mode != Mode::World) {
            return Err("native startup does not yet initialize World matches".into());
        }
        if defender_team.is_some() && options.mode != Mode::Assault {
            return Err("Assault startup requires explicit Assault mode".into());
        }
        if tutorial && options.mode != Mode::Tutorial {
            return Err("Tutorial profile requires Tutorial mode".into());
        }
        if !world_mode
            && !elimination_mode
            && !tutorial
            && defender_team.is_none()
            && !horde_mode
            && !matches!(options.mode, Mode::Default | Mode::Conquest)
        {
            return Err("native startup currently supports Conquest".into());
        }
        if horde_mode && options.mode != Mode::Horde {
            return Err("Horde startup requires explicit Horde mode".into());
        }
        if !options.living_weather && options.weather != WeatherChoice::Map {
            return Err("disabled living weather requires the map weather choice".into());
        }
        if !(1..=6).contains(&options.players.len())
            || options.players.iter().any(|p| p.team < 0 || p.team >= 6)
        {
            return Err("startup needs 1-6 players with team IDs 0-5".into());
        }
        let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
        let cfg = catalog.table("CFG").map_err(|e| e.to_string())?;
        let teams: Vec<_> = options.players.iter().map(|p| p.team).collect();
        let spawns: Vec<_> = map
            .spawns()
            .iter()
            .map(|p| deployment::Spawn {
                x: p.x,
                y: p.y,
                assault: p.assault,
            })
            .collect();
        let available = deployment::spawns_for(&spawns, options.mode)
            .map_err(|e| format!("spawn list: {e:?}"))?;
        let (width, height) = map.dimensions();
        let rows = map.source()["rows"]
            .as_array()
            .ok_or("missing authoring rows")?;
        let table = catalog.table("TERRAIN").map_err(|e| e.to_string())?;
        let mut flags = Vec::with_capacity(width * height);
        for row in rows {
            for cell in row.as_str().ok_or("invalid authoring row")?.bytes() {
                flags.push(
                    table
                        .get(&char::from(cell).to_string())
                        .and_then(Value::number)
                        .unwrap_or(0.0) as u16,
                );
            }
        }
        let heights: Option<Vec<i8>> = map.source().get("heights").map(|rows| {
            rows.as_array()
                .expect("validated rows")
                .iter()
                .flat_map(|r| r.as_str().expect("validated row").bytes())
                .map(|c| match c {
                    b'a' => -1,
                    b'b' => -2,
                    _ => (c - b'0') as i8,
                })
                .collect()
        });
        let terrain = deployment::TerrainView {
            width,
            height,
            flags: &flags,
            heights: heights.as_deref(),
        };
        let distances = if world_mode {
            Vec::new()
        } else {
            deployment::spawn_distances(&terrain, &spawns, &available)
                .map_err(|e| format!("spawn distances: {e:?}"))?
        };
        let mut random = match profile {
            StartupProfile::World(random) => random,
            _ => MatchRandom::new(options.seed),
        };
        #[cfg(test)]
        {
            random.replay = replay.clone();
        }
        let selection = if options.shuffle && !world_mode {
            LayoutSelection::RandomFraction(random.next_fraction(StreamId::Terrain))
        } else {
            LayoutSelection::First
        };
        let slots = if tutorial {
            teams.iter().map(|&team| team as usize).collect()
        } else if world_mode {
            if spawns.len() < teams.len() {
                return Err("World startup needs one authored spawn per commander".into());
            }
            (0..teams.len()).collect()
        } else {
            deployment::spawn_slots(available.len(), &teams, Some(&distances), selection)
                .map_err(|e| format!("spawn allocation: {e:?}"))?
        };
        let mut authored_slots: Vec<_> = if world_mode || tutorial {
            slots.clone()
        } else {
            slots.iter().map(|&slot| available[slot]).collect()
        };
        if let (Some(defender), Some(defend)) = (defender_team, map.defend()) {
            let attackers: Vec<_> = (0..spawns.len())
                .filter(|slot| !defend.contains(slot))
                .collect();
            let (mut d, mut a) = (0, 0);
            for (index, team) in teams.iter().enumerate() {
                authored_slots[index] = if *team == defender {
                    let slot = defend[d % defend.len()];
                    d += 1;
                    slot
                } else {
                    let slot = attackers[a % attackers.len()];
                    a += 1;
                    slot
                };
            }
        }
        if horde_mode {
            let defend = map.defend().ok_or("Horde needs defender spawns")?;
            let home = *defend
                .get(defend.len() / 2)
                .ok_or("Horde needs defender spawns")?;
            let gate = (0..spawns.len())
                .find(|slot| !defend.contains(slot))
                .ok_or("Horde needs an attacker spawn")?;
            for (index, team) in teams.iter().enumerate() {
                authored_slots[index] = if *team == 0 { home } else { gate };
            }
        }
        #[cfg(test)]
        if let Some(replay) = &replay {
            // Explicit constructor input, before player positions, placement or IDs exist.
            authored_slots = replay.layout(&authored_slots, &available, options.players.len())?;
        }
        let support = catalog
            .table("SUPPORT_TYPES")
            .map_err(|e| e.to_string())?
            .array()
            .ok_or("invalid support type list")?;
        let players = options
            .players
            .iter()
            .enumerate()
            .map(|(slot, config)| {
                let s = &spawns[if world_mode {
                    slot
                } else {
                    authored_slots[slot]
                }];
                Ok(PlayerStart {
                    config: config.clone(),
                    spawn: Point {
                        x: (s.x as f64 + 0.5) * CELL,
                        z: (s.y as f64 + 0.5) * CELL,
                    },
                    support: support
                        .iter()
                        .map(|s| {
                            s.string()
                                .map(|s| (s.to_owned(), 0.0))
                                .ok_or("invalid support type".to_owned())
                        })
                        .collect::<Result<_, _>>()?,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let army_key = options.army.key();
        let income_scale = number(field(field(cfg, "armies")?, army_key)?, "income")?;
        let initial_mp = number(
            if world_mode || options.mode == Mode::Classic {
                field(cfg, "classic")?
            } else {
                cfg
            },
            "mpStart",
        )? * income_scale;
        let initial_inc = number(cfg, "mpBase")?;
        let economy = EconomyState {
            players: teams
                .iter()
                .map(|&team| {
                    Ok(PlayerEconomy {
                        team,
                        mp: if horde_mode {
                            if team == 0 {
                                number(field(cfg, "assault")?, "defenderMp")? * income_scale
                            } else {
                                0.0
                            }
                        } else if let Some(defender) = defender_team {
                            number(
                                field(cfg, "assault")?,
                                if team == defender {
                                    "defenderMp"
                                } else {
                                    "attackerMp"
                                },
                            )? * income_scale
                        } else {
                            initial_mp
                        },
                        inc: initial_inc,
                        ..Default::default()
                    })
                })
                .collect::<Result<Vec<_>, String>>()?,
        };
        let kept: Vec<_> = map
            .points()
            .iter()
            .enumerate()
            .filter_map(|(i, p)| {
                (!(defender_team.is_some()
                    || options.mode == Mode::Annihilation
                    || tutorial
                    || horde_mode)
                    || p.mp > 0.0)
                    .then_some(i)
            })
            .collect();
        let points = map
            .points()
            .iter()
            .enumerate()
            .filter(|(i, _)| kept.contains(i))
            .map(|(i, p)| Objective {
                id: match map.source()["points"][i].get("id") {
                    Some(id) if !id.is_null() => id.clone(),
                    _ => serde_json::Value::String(format!("point:{i}")),
                },
                at: Point {
                    x: (p.x as f64 + 0.5) * CELL,
                    z: (p.y as f64 + 0.5) * CELL,
                },
                vp: p.vp,
                mp: p.mp,
                kind: p.kind.clone(),
                needs: p
                    .needs
                    .and_then(|needed| kept.iter().position(|i| *i == needed)),
                owner: None,
                capper: None,
                progress: 0.0,
                cut: false,
                on_point: Vec::new(),
                contested: false,
            })
            .collect();
        let mut living_seed = random.next_u64(StreamId::Weather) as u32;
        let wind = (
            aligned_environment_draw!(
                &mut living_seed,
                replay.as_ref(),
                serde_json::json!(["startup.wind.angle"])
            ) * std::f64::consts::TAU,
            0.3 + aligned_environment_draw!(
                &mut living_seed,
                replay.as_ref(),
                serde_json::json!(["startup.wind.strength"])
            ) * 0.5,
        );
        let wx = field(cfg, "weather")?;
        let living = if options.living_weather {
            let dry = field(wx, "dry")?
                .array()
                .and_then(|v| v.first())
                .and_then(Value::number)
                .ok_or("invalid dry interval")?;
            let next = number(wx, "firstRain")?
                + aligned_environment_draw!(
                    &mut living_seed,
                    replay.as_ref(),
                    serde_json::json!(["startup.shower"])
                ) * dry;
            Some(LivingWeather {
                raining: false,
                next,
            })
        } else {
            None
        };
        let weather_seed = options.weather_seed.unwrap_or_else(|| {
            if options.weather == WeatherChoice::Random {
                random
                    .below(StreamId::Weather, 1 << 31)
                    .expect("nonempty seed range") as u32
            } else {
                1
            }
        });
        let weather = WeatherRules::compiled()
            .map_err(|e| e.to_string())?
            .plan_weather(
                options.weather,
                MapWeatherInfo {
                    name: Some(map.name()),
                    mood: map.source()["mood"].as_str(),
                    weather: map.source()["weather"].as_str(),
                },
                options.map_key.as_deref(),
                weather_seed,
            );
        let initial_rain = if options.living_weather && weather.now == WeatherKind::Rain {
            1.0
        } else {
            0.0
        };
        let initial_wet = options.living_weather.then_some(
            if matches!(weather.now, WeatherKind::Rain | WeatherKind::Mud) {
                1.0
            } else {
                0.0
            },
        );
        let living = living.map(|mut l| {
            l.raining = weather.now == WeatherKind::Rain;
            l
        });
        let starting_kinds = field(
            if world_mode || options.mode == Mode::Classic {
                field(cfg, "classic")?
            } else {
                cfg
            },
            "startForce",
        )?
        .array()
        .ok_or("invalid starting force")?
        .iter()
        .map(|t| {
            rules
                .kind(t.string().ok_or("invalid starting type")?)
                .ok_or("unknown starting type".to_owned())
        })
        .collect::<Result<_, _>>()?;
        Ok(Self {
            dimensions: (width, height),
            players,
            economy,
            points,
            win_vp: deployment::win_vp(&teams).map_err(|e| format!("VP goal: {e:?}"))?,
            weather,
            weather_seed,
            environment_seed: living_seed,
            living,
            wind,
            initial_rain,
            initial_wet,
            random,
            starting_kinds,
            shared_defenders: horde_mode.then_some(options.players.len() - 1),
        })
    }

    /// Populate the initial moving units before the starting bases stamp footprints.
    pub fn spawn_initial_force(&self, game: &mut Game, rules: &Rules) -> Result<u32, String> {
        if !game.units.is_empty() {
            return Err("initial force requires an empty unit table".into());
        }
        let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
        let mut placer = SpawnPlacer::new();
        if (game.w, game.h) != self.dimensions {
            return Err("starting terrain does not match planned dimensions".into());
        }
        let mut units = BTreeMap::new();
        let mut next_id = 1;
        let tutorial = game.mode == Some(crate::game::ModeKind::Tutorial);
        for (owner, player) in self.players.iter().enumerate() {
            if (tutorial && player.config.team != 0)
                || self
                    .shared_defenders
                    .is_some_and(|defenders| owner >= defenders)
            {
                continue;
            }
            for (index, &kind) in self.starting_kinds.iter().enumerate() {
                let view = TerrainView {
                    width: game.w,
                    height: game.h,
                    flags: &game.flags,
                    heights: game.height.as_deref(),
                    world_known: false,
                    knowledge_id: 0,
                    versions: Versions::default(),
                    changed: None,
                    changes: &[],
                };
                let at = placer
                    .scatter_position(
                        view,
                        &rules.kinds[kind],
                        player.spawn,
                        (index
                            + if tutorial || self.shared_defenders.is_some() {
                                owner * 3
                            } else {
                                0
                            }) as u32,
                    )
                    .map_err(|e| format!("starting placement: {e:?}"))?
                    .point;
                let unit = unit_spawn::initial_unit(rules, catalog, kind, next_id, owner, at)
                    .map_err(|e| format!("starting unit: {e:?}"))?;
                units.insert(next_id, unit);
                next_id += 1;
            }
        }
        game.units = units;
        game.players = self
            .players
            .iter()
            .enumerate()
            .map(|(slot, p)| Player {
                slot,
                team: p.config.team,
                visible: Vec::new(),
            })
            .collect();
        game.weather = rules
            .weather
            .iter()
            .position(|w| w.name == self.weather.now.name());
        game.rain = self.initial_rain;
        game.wet = self.initial_wet;
        Ok(next_id)
    }
}

/// Owner and position retained for a building footprint after its unit is removed.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct BuildingCell {
    pub unit: u32,
    pub owner: usize,
    pub at: Point,
}

/// Authoritative startup state. Each mode admits only its implemented phases.
/// Live terrain and units have a single owner in `world.game`.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Match {
    pub aviation: crate::match_aviation::AviationState,
    pub support: crate::match_support::SupportState,
    pub field_works: crate::field_works::FieldWorksRuntime,
    pub abilities: crate::match_abilities::AbilityState,
    pub recovery: crate::match_recovery::RecoveryState,
    pub assault: Option<crate::assault_runtime::AssaultState>,
    #[serde(default)]
    pub elimination: Option<crate::elimination_modes::EliminationState>,
    pub horde: Option<crate::match_horde::HordeState>,
    pub commands: crate::match_commands::MatchCommands,
    pub combat: crate::match_combat::CombatState,
    pub environment: crate::environment::EnvironmentRuntime,
    pub companions: crate::companions::Companions,
    #[serde(skip)]
    pub world_runtime: Option<crate::match_world::WorldRuntime>,
    pub rail_runs: BTreeMap<u32, crate::match_world_rails::RailRun>,
    /// Team-shared last observed buildings for ordinary rooms and the explicit Classic core phase.
    pub classic_known_buildings: crate::vision::BuildingMemory,
    pub world: World,
    pub start: StartPlan,
    pub options: MatchOptions,
    pub grid: SpatialGrid,
    pub(crate) recipients: Vec<crate::match_routing::RecipientRouting>,
    pub(crate) routing_fog: crate::fog::Fog,
    pub building_cells: BTreeMap<usize, BuildingCell>,
    pub production: crate::production::ProductionState,
    pub building_rallies: BTreeMap<u32, Option<Point>>,
    pub player_rallies: Vec<Option<Point>>,
    pub recruit_metadata: BTreeMap<u32, RecruitMetadata>,
    pub(crate) produced_placer: crate::produced_units::ProducedPlacer,
    pub scenario: Option<crate::legacy_scenario::LegacyScenario>,
    #[serde(default)]
    pub authored_scenario: Option<crate::authored_scenario::AuthoredScenario>,
    pub logistics: crate::logistics::LogisticsState,
    pub convoys: Option<crate::supply_convoys::ConvoyState>,
    pub supply_net: crate::supply_network::GroundSupplyNetwork,
    pub(crate) supply_routes: crate::supply_routes::SupplyRouting,
    pub firing: crate::match_firing::FiringState,
    pub capture_ticks: Vec<Option<i64>>,
    pub tick_state: crate::match_tick::TickState,
    /// Host admission stays bound to this match across ticks and recorded commands.
    pub(crate) ordinary_snapshot_only: bool,
    /// Canonical source counters. Event owners must record accepted effects exactly once.
    pub story: Vec<crate::match_story::PlayerStory>,
    pub fallen: crate::match_story::FallenLocations,
    pub finish: Option<crate::match_end::MatchFinish>,
    pub(crate) tutorial: Option<crate::match_tutorial::TutorialRuntime>,
    pub timeline: Vec<crate::match_end::MatchSample>,
    pub next_unit_id: u32,
    #[serde(skip, default = "crate::save_state::compiled_rules")]
    pub rules: Rules,
}

/// Shared cancellation for transactions holding disjoint wallet/story guards.
pub(crate) fn cancel_field_work_state(
    runtime: &mut crate::field_works::FieldWorksRuntime,
    unit: &mut crate::game::Unit,
) -> Result<(), String> {
    if unit.dig != runtime.is_active(unit.id)
        || unit.entrench != runtime.membership(unit.id).is_some()
    {
        return Err("unrepresented or inconsistent field-work activity".into());
    }
    runtime.cancel(unit.id);
    unit.dig = false;
    unit.entrench = false;
    Ok(())
}

/// Accepted replacement orders clear the existing field owners and construction scalar together.
pub(crate) fn cancel_owned_work_state(
    runtime: &mut crate::field_works::FieldWorksRuntime,
    unit: &mut crate::game::Unit,
    control: &mut crate::match_commands::UnitControl,
) -> Result<(), String> {
    crate::match_construction::validate_build_activity(unit, control)?;
    cancel_field_work_state(runtime, unit)?;
    crate::match_construction::clear_build_activity(unit, control)
}

impl Match {
    /// Source failed Dig routing clears only paid work. The squad remains on its shared project.
    pub fn drop_field_work_job(&mut self, id: u32) -> Result<(), String> {
        let unit = self
            .world
            .game
            .units
            .get_mut(&id)
            .ok_or("missing dropped field-work unit")?;
        if unit.id != id
            || unit.dig != self.field_works.is_active(id)
            || unit.entrench != self.field_works.membership(id).is_some()
        {
            return Err("unrepresented or inconsistent field-work activity".into());
        }
        self.field_works.drop_paid_job(id);
        unit.dig = false;
        Ok(())
    }
    /// Cancel current paid work without a refund. The descriptor is removed before its activity flag.
    pub fn cancel_field_work(&mut self, id: u32) -> Result<(), String> {
        let control = self
            .commands
            .units
            .get_mut(&id)
            .ok_or("missing field-work control")?;
        let unit = self
            .world
            .game
            .units
            .get_mut(&id)
            .ok_or("missing field-work unit")?;
        if unit.id != id {
            return Err("inconsistent field-work identity".into());
        }
        cancel_owned_work_state(&mut self.field_works, unit, control)
    }
    pub fn new(map: &crate::map::AuthoritativeMap, options: MatchOptions) -> Result<Self, String> {
        let rules = Rules::compiled().map_err(|e| e.to_string())?;
        let start = StartPlan::prepare(map, &options, &rules)?;
        let mut result = Self::skirmish_shell(map, options, rules, start)?;
        result.complete_skirmish_start()?;
        Ok(result)
    }

    #[cfg(test)]
    pub(crate) fn new_aligned(
        map: &crate::map::AuthoritativeMap,
        options: MatchOptions,
        replay: crate::aligned_replay::Replay,
    ) -> Result<Self, String> {
        let rules = Rules::compiled().map_err(|e| e.to_string())?;
        let start = StartPlan::prepare_mode(
            map,
            &options,
            &rules,
            StartupProfile::Conquest,
            Some(replay.clone()),
        )?;
        let mut result = Self::skirmish_shell(map, options, rules, start)?;
        result.environment.replay = Some(replay.clone());
        result.complete_skirmish_start()?;
        replay.startup(&result);
        Ok(result)
    }

    pub(crate) fn skirmish_shell(
        map: &crate::map::AuthoritativeMap,
        mut options: MatchOptions,
        rules: Rules,
        start: StartPlan,
    ) -> Result<Self, String> {
        options.resolve_mode_features();
        let mut world = World::from_map(map).map_err(|e| e.to_string())?;
        if options.mode == Mode::Tutorial {
            world.game.mode = Some(crate::game::ModeKind::Tutorial);
        }
        let next_unit_id = start.spawn_initial_force(&mut world.game, &rules)?;
        for slot in 0..start.players.len() {
            world.register_recipient(crate::world::Recipient::Player(slot));
        }
        let initial_tech =
            TechState::new(1, 0, 0, 0).map_err(|e| format!("initial research: {e:?}"))?;
        let initial_research = (0..start.players.len())
            .map(|slot| {
                if let Some(defenders) = start.shared_defenders {
                    TechState::new(if slot < defenders { 2 } else { 3 }, 0, 0, 0)
                        .map_err(|e| format!("Horde initial research: {e:?}"))
                } else {
                    Ok(initial_tech)
                }
            })
            .collect::<Result<Vec<_>, String>>()?;
        let production =
            crate::production::ProductionState::new(initial_research).map_err(|e| e.to_string())?;
        let authored_scenario = crate::scenario_validation::explicit_scenario(map.source())
            .cloned()
            .map(crate::authored_scenario::AuthoredScenario::new);
        let scenario = if authored_scenario.is_some() || map.triggers().is_empty() {
            None
        } else {
            let owners = start
                .points
                .iter()
                .map(|p| {
                    Ok(crate::legacy_scenario::PointOwner {
                        id: p
                            .id
                            .as_str()
                            .ok_or("legacy scenario needs string point identities")?,
                        owner: -1,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            crate::legacy_scenario::LegacyScenario::from_map(map, &owners)
        };
        let player_count = start.players.len();
        let point_count = start.points.len();
        let result = Self {
            aviation: crate::match_aviation::AviationState::default(),
            support: crate::match_support::SupportState::default(),
            field_works: crate::field_works::FieldWorksRuntime::default(),
            abilities: crate::match_abilities::AbilityState::default(),
            recovery: crate::match_recovery::RecoveryState::default(),
            assault: None,
            elimination: None,
            horde: None,
            commands: crate::match_commands::MatchCommands::new(4096),
            combat: crate::match_combat::CombatState::default(),
            environment: crate::environment::EnvironmentRuntime::default(),
            companions: crate::companions::Companions::default(),
            world_runtime: None,
            rail_runs: BTreeMap::new(),
            classic_known_buildings: Default::default(),
            grid: SpatialGrid::new(&world.game.units, 16.0),
            recipients: Vec::new(),
            routing_fog: crate::fog::Fog::default(),
            world,
            start,
            options,
            building_cells: BTreeMap::new(),
            production,
            building_rallies: BTreeMap::new(),
            player_rallies: vec![None; player_count],
            recruit_metadata: BTreeMap::new(),
            produced_placer: crate::produced_units::ProducedPlacer::new(),
            scenario,
            authored_scenario,
            logistics: crate::logistics::LogisticsState::default(),
            convoys: None,
            supply_net: crate::supply_network::GroundSupplyNetwork::default(),
            supply_routes: crate::supply_routes::SupplyRouting::default(),
            firing: crate::match_firing::FiringState::default(),
            capture_ticks: vec![None; point_count],
            tick_state: crate::match_tick::TickState::default(),
            ordinary_snapshot_only: false,
            story: vec![crate::match_story::PlayerStory::default(); player_count],
            fallen: crate::match_story::FallenLocations::default(),
            finish: None,
            tutorial: None,
            timeline: Vec::new(),
            next_unit_id,
            rules,
        };
        Ok(result)
    }

    pub(crate) fn complete_skirmish_start(&mut self) -> Result<(), String> {
        if !matches!(self.options.mode, Mode::Classic | Mode::Tutorial) {
            self.setup_bases()?;
        }
        for unit in self.world.game.units.values() {
            self.combat.initialize(unit.id);
            self.firing.initialize(unit.id);
            self.abilities.initialize(unit.id);
            self.recovery.initialize(unit.id);
            if !self.commands.units.contains_key(&unit.id) {
                let control = crate::match_commands::UnitControl::new_for_mode(
                    unit,
                    &self.rules,
                    self.options.mode,
                )?;
                self.commands.insert(unit.id, control);
            }
        }
        self.grid.rebuild(&self.world.game.units);
        if crate::match_actor::ordinary_ground_mode(&self.world.game, self.options.mode) {
            self.initialize_recipient_routing()?;
            self.initialize_logistics()?;
        }
        self.setup_authored_scenario()?;
        Ok(())
    }

    /// Run the late ordinary or explicit Horde point-supply, capture and income phases.
    /// `unit_order` is the tick's captured list, so newly spawned units wait until the next tick.
    /// This phase does not advance the clock, apply combat or decide a winner.
    pub fn apply_point_economy(
        &mut self,
        unit_order: &[u32],
        winner_decided: bool,
    ) -> Result<Vec<crate::objectives::Captured>, String> {
        self.staged_point_phase(unit_order, winner_decided, true, true)
    }

    /// Run point connectivity before reinforcement and dead-unit terrain effects.
    /// The tick runner calls capture/income afterward without recomputing this beat.
    pub fn apply_point_supply(&mut self, unit_order: &[u32]) -> Result<(), String> {
        self.staged_point_phase(unit_order, self.finish.is_some(), true, false)?;
        Ok(())
    }

    /// Run capture and income against the cut flags computed in the earlier phase.
    /// Canonical finish state controls VP and capture-story freezing.
    pub fn apply_point_capture_income(
        &mut self,
        unit_order: &[u32],
    ) -> Result<Vec<crate::objectives::Captured>, String> {
        self.staged_point_phase(unit_order, self.finish.is_some(), false, true)
    }

    fn staged_point_phase(
        &mut self,
        unit_order: &[u32],
        winner_decided: bool,
        supply: bool,
        capture_income: bool,
    ) -> Result<Vec<crate::objectives::Captured>, String> {
        use crate::{economy_state as economy, objectives};
        let mode = if self.options.mode == Mode::Horde {
            self.validate_horde_economy()?;
            economy::ModeContext {
                kind: Mode::Horde,
                defender_team: Some(0),
                income_excluded_slot: self.horde.as_ref().map(|h| h.slot),
            }
        } else {
            self.validate_ordinary_finish_state()?;
            economy::ModeContext {
                kind: if matches!(self.options.mode, Mode::Default | Mode::Conquest) {
                    Mode::Default
                } else {
                    self.options.mode
                },
                defender_team: self.assault.map(|state| state.defender_team),
                income_excluded_slot: self.tutorial.as_ref().map(|runtime| runtime.enemy()),
            }
        };
        let winner_decided = winner_decided || self.finish.is_some();
        let dt = self.rules.movement.tick;
        let game = &self.world.game;
        if game.tick < 0
            || game.players.len() != self.start.economy.players.len()
            || self.start.players.len() != self.start.economy.players.len()
            || self.capture_ticks.len() != self.start.points.len()
        {
            return Err("invalid Conquest phase state".into());
        }
        let mut seen = std::collections::BTreeSet::new();
        if unit_order.iter().any(|id| !seen.insert(*id)) {
            return Err("duplicate unit in tick order".into());
        }
        let list: Vec<_> = unit_order
            .iter()
            .filter_map(|id| game.units.get(id))
            .collect();
        if list.iter().any(|u| u.kind >= self.rules.kinds.len()) {
            return Err("unknown kind in tick unit list".into());
        }
        let players: Vec<_> = self
            .start
            .players
            .iter()
            .zip(&self.start.economy.players)
            .map(|(p, e)| objectives::ObjectivePlayer {
                team: e.team,
                out: e.out,
                spawn: p.spawn,
            })
            .collect();
        let units: Vec<_> = list
            .iter()
            .map(|u| {
                objectives::ObjectiveUnit::from_unit(
                    u,
                    &self.rules.kinds[u.kind].name,
                    self.logistics.get(u.id).is_some_and(|r| r.forced),
                )
            })
            .collect();
        let mut points = self.start.points.clone();
        let mut wallets = self.start.economy.clone();
        let mut placer = SpawnPlacer::new();
        if supply
            && objectives::point_supply_due(self.options.mode, self.options.supply, game.tick)
                .map_err(|e| format!("point supply scheduling: {e:?}"))?
        {
            objectives::supply_lines(
                terrain_view(game),
                &mut points,
                &units,
                &players,
                &mut placer,
            )
            .map_err(|e| format!("point supply: {e:?}"))?;
        }
        let captured = if capture_income {
            let captured = objectives::capture_points(&mut points, &units, &players, dt)
                .map_err(|e| format!("point capture: {e:?}"))?;
            let queues: Vec<Vec<&str>> = list
                .iter()
                .map(|u| {
                    self.production
                        .building(u.id)
                        .map_or_else(Vec::new, |p| p.queue().collect())
                })
                .collect();
            let income_units: Vec<_> = list
                .iter()
                .zip(&queues)
                .map(|(u, queue)| economy::Unit {
                    id: u.id as u64,
                    owner: Some(u.owner),
                    kind: &self.rules.kinds[u.kind].name,
                    hp: u.hp,
                    // Missing built never passes the source's >= 1 check. This finite projection
                    // is equivalent for income and leaves the canonical unit field absent.
                    built: u.built.unwrap_or(0.0),
                    queue,
                })
                .collect();
            let income_points: Vec<_> = points
                .iter()
                .map(|p| economy::Point {
                    owner: p.owner,
                    cut: p.cut,
                    mp: p.mp,
                    vp: p.vp,
                })
                .collect();
            wallets
                .advance_income(
                    &economy::Context {
                        mode,
                        army: self.options.army,
                        units: &income_units,
                        drops: &[],
                        points: &income_points,
                        nodes: &self.elimination.as_ref().map_or_else(Vec::new, |state| {
                            state
                                .nodes
                                .iter()
                                .map(|node| economy::Node {
                                    depot: node.depot.map(u64::from),
                                    rate: node.rate,
                                    fuel: node.fuel,
                                    region: None,
                                })
                                .collect::<Vec<_>>()
                        }),
                        world: None,
                    },
                    dt,
                    winner_decided,
                )
                .map_err(|e| e.to_string())?;
            captured
        } else {
            Vec::new()
        };
        let changes: Vec<_> = captured
            .iter()
            .map(|event| crate::match_story::StoryChange {
                slot: Some(event.owner),
                key: crate::match_story::StoryKey::Captures,
                amount: 1.0,
            })
            .collect();
        let story_updates =
            crate::match_story::prepare_story_updates(&mut self.story, winner_decided, &changes)?;
        self.start.points = points;
        self.start.economy = wallets;
        story_updates.commit();
        if !winner_decided {
            for event in &captured {
                self.capture_ticks[event.objective_index] = Some(game.tick);
            }
        }
        Ok(captured)
    }

    fn setup_bases(&mut self) -> Result<(), String> {
        let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
        let point_radius = number(
            catalog.table("CFG").map_err(|e| e.to_string())?,
            "pointRadius",
        )?;
        for owner in 0..self.start.players.len() {
            if self.horde.is_some() && owner != 0 {
                continue;
            }
            let spawn = self.start.players[owner].spawn;
            let mut avoid: Vec<(Point, f64)> = self
                .start
                .points
                .iter()
                .map(|p| (p.at, point_radius + 5.0))
                .collect();
            avoid.extend(
                self.world
                    .game
                    .units
                    .values()
                    .filter(|u| self.rules.kinds[u.kind].structure)
                    .map(|u| (u.at(), self.rules.kinds[u.kind].radius + 9.0)),
            );
            for name in ["hq", "barracks"] {
                let cell = if let Some(cell) = find_site(
                    &self.world.game,
                    Point {
                        x: spawn.x - 3.0,
                        z: spawn.z - 3.0,
                    },
                    3,
                    &avoid,
                ) {
                    cell
                } else {
                    let game = &self.world.game;
                    let x =
                        (spawn.x / CELL).floor() - 1.0 + if name == "barracks" { 4.0 } else { 0.0 };
                    let x = x.min(game.w as f64 - 3.0).max(0.0) as usize;
                    let y = ((spawn.z / CELL).floor() - 1.0)
                        .min(game.h as f64 - 3.0)
                        .max(0.0) as usize;
                    let cell = y * game.w + x;
                    let Some(cells) = footprint(game, cell, 3) else {
                        continue;
                    };
                    if cells.iter().any(|&c| game.chars[c] == b'K') {
                        continue;
                    }
                    for c in cells {
                        self.world.set_cell(c, b'.').map_err(|e| e.to_string())?;
                    }
                    cell
                };
                let at = self.place_starting_building(owner, name, cell)?;
                avoid.push((at, 12.0));
            }
        }
        self.relocate_trapped_setup_units()?;
        Ok(())
    }

    pub(crate) fn place_starting_building(
        &mut self,
        owner: usize,
        name: &str,
        cell: usize,
    ) -> Result<Point, String> {
        let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
        let definition = catalog.unit(name).map_err(|e| e.to_string())?;
        let size = number(definition, "size")? as usize;
        let max_hp = number(definition, "hpPer")?;
        let cells =
            footprint(&self.world.game, cell, size).ok_or("building footprint is outside map")?;
        let level = |c: usize| self.world.game.height.as_ref().map_or(0, |h| h[c]);
        let top = cells
            .iter()
            .map(|&c| level(c))
            .max()
            .ok_or("empty building footprint")?;
        for &c in &cells {
            if self.world.game.height.as_ref().map_or(0, |h| h[c]) < top {
                self.world.set_level(c, top).map_err(|e| e.to_string())?;
            }
            self.world.set_cell(c, b'K').map_err(|e| e.to_string())?;
        }
        let kind = self.rules.kind(name).ok_or("unknown building kind")?;
        let id = self.next_unit_id;
        let at = foot_center(&self.world.game, cell, size);
        let mut draft = crate::unit_publication::StagedUnit::ordinary(
            &self.world.game,
            &self.rules,
            owner,
            kind,
            id,
            self.world.game.units.len() as u32,
            self.start.players[owner].spawn,
            Some(at),
        )
        .map_err(|e| format!("building unit: {e:?}"))?;
        draft.unit.cells = Some(cells.iter().map(|&c| c as u32).collect());
        draft.unit.built = Some(1.0);
        draft.unit.hp = max_hp;
        self.publish_starting_unit(draft)?;
        self.production
            .initialize_building(id)
            .map_err(|e| e.to_string())?;
        self.building_rallies.insert(id, None);
        for &c in &cells {
            self.building_cells.insert(
                c,
                BuildingCell {
                    unit: id,
                    owner,
                    at,
                },
            );
        }
        self.world
            .register_building_structure(id, &cells, max_hp, max_hp)
            .map_err(|e| e.to_string())?;
        Ok(at)
    }

    pub(crate) fn publish_starting_unit(
        &mut self,
        draft: crate::unit_publication::StagedUnit,
    ) -> Result<(), String> {
        crate::unit_publication::prepare_publication(
            crate::unit_publication::PublicationParts {
                units: &mut self.world.game.units,
                grid: &mut self.grid,
                controls: &mut self.commands.units,
                recruitment: &mut self.recruit_metadata,
                next_id: &mut self.next_unit_id,
            },
            &self.rules,
            self.start.players.len(),
            vec![draft],
        )
        .map_err(|e| format!("starting unit publication: {e:?}"))?
        .publish_with(|unit, _| {
            self.combat.initialize(unit.id);
            self.firing.initialize(unit.id);
            self.abilities.initialize(unit.id);
            self.recovery.initialize(unit.id);
        });
        Ok(())
    }
}

pub(crate) fn terrain_view(game: &Game) -> TerrainView<'_> {
    TerrainView {
        width: game.w,
        height: game.h,
        flags: &game.flags,
        heights: game.height.as_deref(),
        world_known: false,
        knowledge_id: 0,
        versions: Versions::default(),
        changed: None,
        changes: &[],
    }
}
pub(crate) fn footprint(game: &Game, cell: usize, size: usize) -> Option<Vec<usize>> {
    let (x, y) = (cell % game.w, cell / game.w);
    (x + size <= game.w && y + size <= game.h).then(|| {
        (0..size)
            .flat_map(|dy| (0..size).map(move |dx| (y + dy) * game.w + x + dx))
            .collect()
    })
}
pub(crate) fn foot_center(game: &Game, cell: usize, size: usize) -> Point {
    Point {
        x: (cell % game.w) as f64 * CELL + size as f64 * CELL / 2.0,
        z: (cell / game.w) as f64 * CELL + size as f64 * CELL / 2.0,
    }
}
pub(crate) fn find_site(
    game: &Game,
    at: Point,
    size: usize,
    avoid: &[(Point, f64)],
) -> Option<usize> {
    let x = at.x.max(0.0).min(game.w as f64 * CELL - 1.0);
    let z = at.z.max(0.0).min(game.h as f64 * CELL - 1.0);
    let start = (z / CELL).floor() as usize * game.w + (x / CELL).floor() as usize;
    let mut seen = vec![false; game.w * game.h];
    seen[start] = true;
    let mut queue = vec![start];
    let mut index = 0;
    while index < queue.len() && index < 4000 {
        let cell = queue[index];
        let at = foot_center(game, cell, size);
        if let Some(cells) = footprint(game, cell, size) {
            let level = |c: usize| game.height.as_ref().map_or(0, |h| h[c]);
            let min = cells.iter().map(|&c| level(c)).min().unwrap();
            let max = cells.iter().map(|&c| level(c)).max().unwrap();
            if cells
                .iter()
                .all(|&c| game.flags[c] & MOVE == 0 && !b"WF=KOAQ".contains(&game.chars[c]))
                && i16::from(max) - i16::from(min) <= 1
                && avoid
                    .iter()
                    .all(|(p, r)| math::hypot(p.x - at.x, p.z - at.z) >= *r)
            {
                return Some(cell);
            }
        }
        let (x, y) = (cell % game.w, cell / game.w);
        for (nx, ny) in [
            (x as isize + 1, y as isize),
            (x as isize - 1, y as isize),
            (x as isize, y as isize + 1),
            (x as isize, y as isize - 1),
        ] {
            if nx < 0 || ny < 0 || nx as usize >= game.w || ny as usize >= game.h {
                continue;
            }
            let c = ny as usize * game.w + nx as usize;
            if !seen[c] {
                seen[c] = true;
                queue.push(c);
            }
        }
        index += 1;
    }
    None
}
