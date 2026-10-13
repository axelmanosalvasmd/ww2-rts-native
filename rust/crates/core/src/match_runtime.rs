//! Native match startup. The setup plan is shared by the authoritative constructor.
pub use crate::objectives::Objective;
#[path = "match_recruitment.rs"]
mod recruitment;
pub use recruitment::{CompletedRecruitUnit, RecruitError, RecruitMetadata, RecruitRouting};
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
    weather_runtime::environment_random,
    world::World,
};

#[derive(Clone, Debug)]
pub struct PlayerConfig {
    pub name: String,
    pub team: i32,
    pub faction: Faction,
}

#[derive(Clone, Debug)]
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

#[derive(Clone, Debug)]
pub struct PlayerStart {
    pub config: PlayerConfig,
    pub spawn: Point,
    pub support: BTreeMap<String, f64>,
}

#[derive(Clone, Copy, Debug)]
pub struct LivingWeather {
    pub raining: bool,
    pub next: f64,
}

/// Materialized startup choices. The RNG records exactly the streams used to make them.
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

impl StartPlan {
    pub fn prepare(
        map: &crate::map::AuthoritativeMap,
        options: &MatchOptions,
        rules: &Rules,
    ) -> Result<Self, String> {
        if map.geography().is_some() {
            return Err("native startup does not yet initialize World matches".into());
        }
        if !matches!(options.mode, Mode::Default | Mode::Conquest) {
            return Err("native startup currently supports Conquest".into());
        }
        if !options.living_weather && options.weather != WeatherChoice::Map {
            return Err("disabled living weather requires the map weather choice".into());
        }
        if options.logistics {
            return Err("native startup does not yet initialize logistics".into());
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
        let distances = deployment::spawn_distances(&terrain, &spawns, &available)
            .map_err(|e| format!("spawn distances: {e:?}"))?;
        let mut random = MatchRandom::new(options.seed);
        let selection = if options.shuffle {
            LayoutSelection::RandomFraction(random.next_fraction(StreamId::Terrain))
        } else {
            LayoutSelection::First
        };
        let slots = deployment::spawn_slots(available.len(), &teams, Some(&distances), selection)
            .map_err(|e| format!("spawn allocation: {e:?}"))?;
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
                let s = &spawns[available[slots[slot]]];
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
        let army_key = match options.army {
            ArmySize::Standard => "standard",
            ArmySize::Large => "large",
            ArmySize::Massive => "massive",
            ArmySize::Endless => "endless",
        };
        let income_scale = number(field(field(cfg, "armies")?, army_key)?, "income")?;
        let initial_mp = number(cfg, "mpStart")? * income_scale;
        let initial_inc = number(cfg, "mpBase")?;
        let economy = EconomyState {
            players: teams
                .iter()
                .map(|&team| PlayerEconomy {
                    team,
                    mp: initial_mp,
                    inc: initial_inc,
                    ..Default::default()
                })
                .collect(),
        };
        let points = map
            .points()
            .iter()
            .enumerate()
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
                needs: p.needs,
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
            environment_random(&mut living_seed) * std::f64::consts::TAU,
            0.3 + environment_random(&mut living_seed) * 0.5,
        );
        let wx = field(cfg, "weather")?;
        let living = if options.living_weather {
            let dry = field(wx, "dry")?
                .array()
                .and_then(|v| v.first())
                .and_then(Value::number)
                .ok_or("invalid dry interval")?;
            let next = number(wx, "firstRain")? + environment_random(&mut living_seed) * dry;
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
        let starting_kinds = field(cfg, "startForce")?
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
        for (owner, player) in self.players.iter().enumerate() {
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
                    .scatter_position(view, &rules.kinds[kind], player.spawn, index as u32)
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
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BuildingCell {
    pub unit: u32,
    pub owner: usize,
    pub at: Point,
}

/// Authoritative Conquest startup state. Tick execution is added independently.
/// Live terrain and units have a single owner in `world.game`.
pub struct Match {
    pub abilities: crate::match_abilities::AbilityState,
    pub recovery: crate::match_recovery::RecoveryState,
    pub combat: crate::match_combat::CombatState,
    pub environment: crate::environment::EnvironmentRuntime,
    pub commands: crate::match_commands::MatchCommands,
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
    produced_placer: crate::produced_units::ProducedPlacer,
    pub scenario: Option<crate::legacy_scenario::LegacyScenario>,
    pub logistics: crate::logistics::LogisticsState,
    pub capture_ticks: Vec<Option<i64>>,
    /// Capture story counts only. Combat and spending counters are not integrated yet.
    pub capture_counts: Vec<f64>,
    pub finish: Option<crate::conquest_end::ConquestFinish>,
    pub timeline: Vec<crate::conquest_end::ConquestSample>,
    pub next_unit_id: u32,
    pub rules: Rules,
}

impl Match {
    pub fn new(map: &crate::map::AuthoritativeMap, options: MatchOptions) -> Result<Self, String> {
        let rules = Rules::compiled().map_err(|e| e.to_string())?;
        let start = StartPlan::prepare(map, &options, &rules)?;
        let mut world = World::from_map(map).map_err(|e| e.to_string())?;
        let next_unit_id = start.spawn_initial_force(&mut world.game, &rules)?;
        for slot in 0..start.players.len() {
            world.register_recipient(crate::world::Recipient::Player(slot));
        }
        let initial_tech =
            TechState::new(1, 0, 0, 0).map_err(|e| format!("initial research: {e:?}"))?;
        let production =
            crate::production::ProductionState::new(vec![initial_tech; start.players.len()])
                .map_err(|e| e.to_string())?;
        let scenario = if map.triggers().is_empty() {
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
        let mut result = Self {
            abilities: crate::match_abilities::AbilityState::default(),
            recovery: crate::match_recovery::RecoveryState::default(),
            combat: crate::match_combat::CombatState::default(),
            environment: crate::environment::EnvironmentRuntime::default(),
            commands: crate::match_commands::MatchCommands::new(4096),
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
            logistics: crate::logistics::LogisticsState::default(),
            capture_ticks: vec![None; map.points().len()],
            capture_counts: vec![0.0; player_count],
            finish: None,
            timeline: Vec::new(),
            next_unit_id,
            rules,
        };
        result.setup_bases()?;
        for unit in result.world.game.units.values() {
            result.combat.initialize(unit.id);
            result.abilities.initialize(unit.id);
            result.recovery.initialize(unit.id);
            let control = crate::match_commands::UnitControl::new(unit, &result.rules)?;
            result.commands.insert(unit.id, control);
        }
        result.grid.rebuild(&result.world.game.units);
        result.initialize_recipient_routing()?;
        Ok(result)
    }

    /// Run the source's late Conquest point-supply, capture and income phases.
    /// `unit_order` is the tick's captured list, so newly spawned units wait until the next tick.
    /// This phase does not advance the clock, apply combat or decide a winner.
    pub fn apply_point_economy(
        &mut self,
        unit_order: &[u32],
        winner_decided: bool,
    ) -> Result<Vec<crate::objectives::Captured>, String> {
        use crate::{economy_state as economy, objectives};
        self.validate_conquest_finish_state()?;
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
        let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
        let cfg = catalog.table("CFG").map_err(|e| e.to_string())?;
        let supply_beat = number(field(cfg, "supply")?, "every")? as i64;
        if self.options.supply && game.tick % supply_beat == 1 {
            objectives::supply_lines(
                terrain_view(game),
                &mut points,
                &units,
                &players,
                &mut placer,
            )
            .map_err(|e| format!("point supply: {e:?}"))?;
        }
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
                    mode: economy::ModeContext::default(),
                    army: self.options.army,
                    units: &income_units,
                    drops: &[],
                    points: &income_points,
                    nodes: &[],
                    world: None,
                },
                dt,
                winner_decided,
            )
            .map_err(|e| e.to_string())?;
        self.start.points = points;
        self.start.economy = wallets;
        if !winner_decided {
            for event in &captured {
                self.capture_ticks[event.objective_index] = Some(game.tick);
                self.capture_counts[event.owner] += 1.0;
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
        let mut placer = SpawnPlacer::new();
        for owner in 0..self.start.players.len() {
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
                let at = self.place_starting_building(owner, name, cell, &mut placer)?;
                avoid.push((at, 12.0));
            }
        }
        let game = &mut self.world.game;
        for unit in game.units.values_mut() {
            let kind = &self.rules.kinds[unit.kind];
            if kind.structure || kind.air {
                continue;
            }
            let x = (unit.x / CELL).floor() as usize;
            let y = (unit.z / CELL).floor() as usize;
            if x < game.w && y < game.h && game.flags[y * game.w + x] & MOVE != 0 {
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
                let c = placer
                    .nearest_free(view, unit.at(), kind.block(), None)
                    .map_err(|e| format!("base clearance: {e:?}"))?;
                unit.x = (c % game.w) as f64 * CELL + CELL / 2.0;
                unit.z = (c / game.w) as f64 * CELL + CELL / 2.0;
            }
        }
        Ok(())
    }

    fn place_starting_building(
        &mut self,
        owner: usize,
        name: &str,
        cell: usize,
        placer: &mut SpawnPlacer,
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
        let at = placer
            .scatter_position(
                terrain_view(&self.world.game),
                &self.rules.kinds[kind],
                self.start.players[owner].spawn,
                self.world.game.units.len() as u32,
            )
            .map_err(|e| format!("building spawn: {e:?}"))?
            .point;
        let id = self.next_unit_id;
        self.next_unit_id = id.checked_add(1).ok_or("unit ID capacity exhausted")?;
        let mut unit = unit_spawn::initial_unit(&self.rules, catalog, kind, id, owner, at)
            .map_err(|e| format!("building unit: {e:?}"))?;
        let at = foot_center(&self.world.game, cell, size);
        unit.x = at.x;
        unit.z = at.z;
        unit.cells = Some(cells.iter().map(|&c| c as u32).collect());
        unit.built = Some(1.0);
        unit.hp = max_hp;
        self.world.game.units.insert(id, unit);
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
}

fn terrain_view(game: &Game) -> TerrainView<'_> {
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
fn footprint(game: &Game, cell: usize, size: usize) -> Option<Vec<usize>> {
    let (x, y) = (cell % game.w, cell / game.w);
    (x + size <= game.w && y + size <= game.h).then(|| {
        (0..size)
            .flat_map(|dy| (0..size).map(move |dx| (y + dy) * game.w + x + dx))
            .collect()
    })
}
fn foot_center(game: &Game, cell: usize, size: usize) -> Point {
    Point {
        x: (cell % game.w) as f64 * CELL + size as f64 * CELL / 2.0,
        z: (cell / game.w) as f64 * CELL + size as f64 * CELL / 2.0,
    }
}
fn find_site(game: &Game, at: Point, size: usize, avoid: &[(Point, f64)]) -> Option<usize> {
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
