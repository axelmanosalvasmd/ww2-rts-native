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
