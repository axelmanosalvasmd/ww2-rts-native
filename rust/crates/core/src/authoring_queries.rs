//! Queries over the caller's own authored document. No live recipient state is accepted.
use crate::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::Point,
    map::AuthoritativeMap,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    weather::WeatherChoice,
};
use serde_json::{Value, json};

type Result<T> = std::result::Result<T, String>;
fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(value) => value.as_f64() != Some(0.0),
        Value::String(value) => !value.is_empty(),
        _ => true,
    }
}

fn map(value: &Value) -> Result<AuthoritativeMap> {
    AuthoritativeMap::from_json(&serde_json::to_vec(value).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}

mod validation;
/// Returns null on success or the first editor diagnostic.
pub fn validate_map(value: &Value) -> Value {
    match validation::validate(value) {
        Ok(()) => Value::Null,
        Err(message) => json!(message),
    }
}

pub fn migrate_scenario(value: &Value) -> Result<Value> {
    if !value.is_object() {
        return Err("expected map object".into());
    }
    Ok(crate::scenario_validation::migrate_scenario(value).unwrap_or(Value::Null))
}

/// Preserve every field and reproduce the editor's legacy layer split.
pub fn migrate_map(value: &Value) -> Result<Value> {
    if !value.is_object() {
        return Err("expected map object".into());
    }
    if value["worldVersion"].as_f64() == Some(2.0) && value.get("layers").is_some_and(truthy) {
        return Ok(value.clone());
    }
    let rows = value["rows"].as_array().ok_or("rows must be an array")?;
    let mut ground = Vec::new();
    let mut objects = Vec::new();
    let mut mines = Vec::new();
    for row in rows {
        let row = row.as_str().ok_or("rows must contain strings")?;
        ground.push(
            row.chars()
                .map(|c| {
                    if ".D+MWF".contains(c) {
                        c
                    } else if c == '=' {
                        'W'
                    } else {
                        '.'
                    }
                })
                .collect::<String>(),
        );
        objects.push(
            row.chars()
                .map(|c| {
                    if ".D+MWF".contains(c) || c == 'N' {
                        '.'
                    } else {
                        c
                    }
                })
                .collect::<String>(),
        );
        mines.push(
            row.chars()
                .map(|c| if c == 'N' { 'N' } else { '.' })
                .collect::<String>(),
        );
    }
    let mut out = value.clone();
    out["worldVersion"] = json!(2);
    out["layers"] = json!({"ground":ground,"objects":objects,"mines":mines});
    Ok(out)
}

fn mode(value: &Value) -> Result<Mode> {
    let key = value
        .get("mode")
        .or_else(|| value.get("options").and_then(|o| o.get("mode")));
    if key.is_some_and(|k| !k.is_string()) {
        return Err("mode must be a string".into());
    }
    let mode = Mode::from_kind(key.and_then(Value::as_str));
    if matches!(mode, Mode::Other | Mode::World | Mode::Tutorial) {
        return Err("unsupported authoring mode".into());
    }
    Ok(mode)
}

pub fn spawns_for(value: &Value) -> Result<Value> {
    let spawns = value["map"]["spawns"]
        .as_array()
        .ok_or("map.spawns must be an array")?;
    let mode = value
        .get("mode")
        .map(|v| v.as_str().ok_or("mode must be a string"))
        .transpose()?;
    let mut indices = Vec::new();
    for (i, spawn) in spawns.iter().enumerate() {
        if !spawn.is_object() {
            return Err("spawn must be an object".into());
        }
        if mode == Some("assault") || !truthy(&spawn["assault"]) {
            indices.push(i);
        }
    }
    Ok(json!(indices))
}

fn options(value: &Value, source: &AuthoritativeMap, shuffle: bool) -> Result<MatchOptions> {
    let opts = value.get("options").cloned().unwrap_or_else(|| json!({}));
    let object = opts.as_object().ok_or("options must be an object")?;
    let allowed = [
        "mode",
        "players",
        "names",
        "teams",
        "factions",
        "shuffle",
        "army",
        "weather",
        "weatherSeed",
        "livingWeather",
        "tech",
        "supply",
        "logistics",
        "sandbox",
        "defenderTeam",
    ];
    for key in object.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(format!("unsupported authoring option: {key}"));
        }
    }
    let mode = mode(value)?;
    let count = opts
        .get("players")
        .map(|v| v.as_u64().ok_or("players must be an integer"))
        .transpose()?
        .unwrap_or(
            source
                .spawns()
                .iter()
                .filter(|spawn| mode == Mode::Assault || !spawn.assault)
                .count() as u64,
        );
    if !(2..=6).contains(&count) {
        return Err("players must be 2-6".into());
    }
    let defenders = source
        .defend()
        .map_or(1, <[usize]>::len)
        .max(1)
        .min(count as usize - 1);
    for key in ["names", "teams", "factions"] {
        if let Some(rows) = opts.get(key)
            && !rows.as_array().is_some_and(|a| a.len() == count as usize)
        {
            return Err(format!("{key} must contain one value per player"));
        }
    }
    let mut players = Vec::new();
    for i in 0..count as usize {
        let name = opts
            .get("names")
            .map(|v| {
                v.get(i)
                    .and_then(Value::as_str)
                    .ok_or("names must contain one string per player")
            })
            .transpose()?
            .map(str::to_owned)
            .unwrap_or_else(|| format!("P{}", i + 1));
        let default_team = if mode == Mode::Assault {
            i32::from(i >= defenders)
        } else {
            i as i32
        };
        let team = opts
            .get("teams")
            .map(|v| {
                v.get(i)
                    .and_then(Value::as_i64)
                    .and_then(|v| i32::try_from(v).ok())
                    .ok_or("invalid teams")
            })
            .transpose()?
            .unwrap_or(default_team);
        let faction = opts
            .get("factions")
            .map(|v| {
                v.get(i)
                    .and_then(Value::as_u64)
                    .and_then(|v| u8::try_from(v).ok())
                    .ok_or("invalid factions")
            })
            .transpose()?
            .unwrap_or((i % 3) as u8);
        players.push(PlayerConfig {
            name,
            team,
            faction: Faction::try_from(faction).map_err(|e| e.to_string())?,
        });
    }
    let flag = |key: &str, default| -> Result<bool> {
        opts.get(key)
            .map(|v| v.as_bool().ok_or_else(|| format!("{key} must be boolean")))
            .unwrap_or(Ok(default))
    };
    let army = opts
        .get("army")
        .map(|v| v.as_str().ok_or("army must be a string"))
        .transpose()?
        .unwrap_or("standard");
    if !["standard", "large", "massive", "endless"].contains(&army) {
        return Err("unsupported army".into());
    }
    let weather = opts
        .get("weather")
        .map(|v| v.as_str().ok_or("weather must be a string"))
        .transpose()?;
    if weather
        .is_some_and(|v| !["map", "clear", "fog", "rain", "mud", "snow", "random"].contains(&v))
    {
        return Err("unsupported weather".into());
    }
    let seed = value
        .get("seed")
        .map(|v| v.as_u64().ok_or("seed must be an unsigned integer"))
        .transpose()?
        .unwrap_or(0);
    Ok(MatchOptions {
        seed,
        mode,
        players,
        shuffle: flag("shuffle", shuffle)?,
        army: ArmySize::from_key(army),
        weather: WeatherChoice::from_reference(weather),
        weather_seed: opts
            .get("weatherSeed")
            .map(|v| {
                v.as_u64()
                    .and_then(|n| u32::try_from(n).ok())
                    .ok_or("invalid weatherSeed")
            })
            .transpose()?,
        living_weather: flag("livingWeather", true)?,
        map_key: None,
        tech: flag("tech", false)?,
        supply: flag("supply", true)?,
        logistics: flag("logistics", false)?,
        sandbox: flag("sandbox", false)?,
    })
}

fn construct(value: &Value, shuffle: bool) -> Result<Match> {
    let source = map(&value["map"])?;
    if source.geography().is_some() {
        return Err("World authoring runtime is unavailable".into());
    }
    let mut opts = options(value, &source, shuffle)?;
    // createGame treats an unavailable Horde selection as ordinary Conquest.
    if opts.mode == Mode::Horde
        && (source.defend().is_none_or(<[usize]>::is_empty) || opts.players.len() == 6)
    {
        opts.mode = Mode::Conquest;
        opts.logistics = false;
    }
    match opts.mode {
        Mode::Default | Mode::Conquest => Match::new(&source, opts),
        Mode::Classic => Match::new_classic(&source, opts),
        Mode::Annihilation => Match::new_annihilation(&source, opts),
        Mode::Assault => {
            let team = value["options"]
                .get("defenderTeam")
                .map(|v| {
                    v.as_i64()
                        .and_then(|n| i32::try_from(n).ok())
                        .ok_or("invalid defenderTeam")
                })
                .transpose()?
                .unwrap_or(0);
            Match::new_assault(&source, opts, team)
        }
        Mode::Horde => Match::new_horde(&source, opts),
        _ => Err("unsupported authoring mode".into()),
    }
}

pub fn create_preview(value: &Value) -> Result<Value> {
    let state = construct(value, false)?;
    let g = &state.world.game;
    let rows = |bytes: &[u8]| {
        bytes
            .chunks(g.w)
            .map(|r| String::from_utf8_lossy(r).into_owned())
            .collect::<Vec<_>>()
    };
    let materials = |table: &std::collections::BTreeMap<usize, String>| {
        table
            .iter()
            .map(|(c, material)| json!({"c":c,"material":material}))
            .collect::<Vec<_>>()
    };
    let display_objects = g.objects.as_ref().map(|objects| {
        objects
            .iter()
            .map(|c| if *c == b'K' { b'B' } else { *c })
            .collect::<Vec<_>>()
    });
    let display_chars = g
        .chars
        .iter()
        .map(|c| if *c == b'K' { b'B' } else { *c })
        .collect::<Vec<_>>();
    let mines = state
        .world
        .mine_layer
        .iter()
        .map(|v| if *v == 0 { b'.' } else { b'N' })
        .collect::<Vec<_>>();
    let mut shown = value["map"].clone();
    shown["worldVersion"] = json!(2);
    shown["rows"] = json!(rows(&display_chars));
    shown["layers"] = json!({"ground":g.ground.as_deref().map(rows),"objects":display_objects.as_deref().map(rows),"mines":rows(&mines),"groundMaterials":materials(&state.world.base_materials),"objectMaterials":materials(&state.world.object_materials),"mineMaterials":materials(&state.world.mine_materials)});
    shown["points"]=json!(state.start.points.iter().map(|p|json!({"x":(p.at.x/crate::game::CELL).floor(),"y":(p.at.z/crate::game::CELL).floor(),"vp":p.vp,"mp":p.mp})).collect::<Vec<_>>());
    let nodes = state
        .elimination
        .as_ref()
        .map(|mode| {
            mode.nodes
                .iter()
                .map(|node| json!({"x":node.at.x,"z":node.at.z,"fuel":node.fuel}))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let horde = state.horde.as_ref().map(|mode| {
        json!({
            "slot":mode.slot,"defenders":mode.defenders,"bunker":mode.bunker,
            "gates":mode.gates,"wave":mode.wave,"active":mode.active,"timeLeft":mode.time_left
        })
    });
    let units = g.units.values().map(|u| json!({"id":u.id,"type":state.rules.kinds[u.kind].name,"owner":u.owner,"x":u.x,"z":u.z})).collect::<Vec<_>>();
    Ok(
        json!({"map":shown,"w":g.w,"h":g.h,"rows":rows(&g.chars),"ground":g.ground.as_deref().map(rows),"objects":g.objects.as_deref().map(rows),
        "players":state.start.players.iter().map(|p|json!({"name":p.config.name,"team":p.config.team,"spawn":{"x":p.spawn.x,"z":p.spawn.z}})).collect::<Vec<_>>(),
        "points":state.start.points,"winVp":state.start.win_vp,"units":units,"assault":state.assault,"elimination":state.elimination,"horde":horde,"nodes":nodes}),
    )
}

pub fn house_kinds(value: &Value) -> Result<Value> {
    let catalog: Value =
        serde_json::from_str(include_str!("../data/rules.json")).map_err(|e| e.to_string())?;
    let metadata = catalog["tables"]["CFG"]["houses"].clone();
    if value.get("rows").is_none() {
        return Ok(metadata);
    }
    let source = map(value)?;
    let terrain = source.initialize_terrain().map_err(|e| e.to_string())?;
    let blocks = crate::map::house_kinds_where(
        |c| terrain.chars[c] == b'B',
        terrain.chars.len(),
        terrain.w,
        source.buildings(),
        None,
    )
    .map_err(|e| e.to_string())?;
    Ok(
        json!({"metadata":metadata,"kinds":terrain.house,"blocks":blocks.into_iter().map(|b|json!({"cells":b.cells,"kind":b.kind})).collect::<Vec<_>>()}),
    )
}

pub fn find_path(value: &Value) -> Result<Value> {
    use crate::{
        navigation::{TerrainView, Versions},
        pathfinding::{PathRules, PathTerrain, Pathfinding},
    };
    let source = map(&value["map"])?;
    let terrain = source.initialize_terrain().map_err(|e| e.to_string())?;
    let point = |v: &Value| -> Result<Point> {
        Ok(Point {
            x: v["x"].as_f64().ok_or("missing x")?,
            z: v["z"].as_f64().ok_or("missing z")?,
        })
    };
    let rules = crate::rules::Rules::compiled().map_err(|e| e.to_string())?;
    let kind = value["from"]
        .get("type")
        .map(|v| v.as_str().ok_or("type must be a string"))
        .transpose()?
        .unwrap_or("rifle");
    let profile = rules
        .kinds
        .iter()
        .find(|k| k.name == kind)
        .ok_or("unknown path unit type")?;
    let n = terrain.w * terrain.h;
    let mut near_walls = vec![0; n];
    for c in 0..n {
        if terrain.flags[c] & (crate::game::MOVE | crate::game::SIGHT)
            != (crate::game::MOVE | crate::game::SIGHT)
        {
            continue;
        }
        let x = c % terrain.w;
        let y = c / terrain.w;
        for ny in y.saturating_sub(1)..=(y + 1).min(terrain.h - 1) {
            for nx in x.saturating_sub(1)..=(x + 1).min(terrain.w - 1) {
                near_walls[ny * terrain.w + nx] = 1;
            }
        }
    }
    let no = vec![false; n];
    let result = Pathfinding::new()
        .find_path(
            PathTerrain {
                navigation: TerrainView {
                    width: terrain.w,
                    height: terrain.h,
                    flags: &terrain.flags,
                    heights: terrain.height.as_deref(),
                    world_known: false,
                    knowledge_id: 0,
                    versions: Versions::default(),
                    changed: None,
                    changes: &[],
                },
                near_walls: &near_walls,
                known_mines: &no,
                fires: &no,
                bunkers: &[],
                wear: None,
                wetness: 0.0,
                roads: false,
            },
            PathRules::compiled().map_err(|e| e.to_string())?,
            profile,
            point(&value["from"])?,
            point(&value["to"])?,
            None,
        )
        .map_err(|e| format!("{e:?}"))?;
    Ok(json!(
        result
            .waypoints
            .iter()
            .map(|p| json!({"x":p.x,"z":p.z}))
            .collect::<Vec<_>>()
    ))
}

/// Runs detached AI matches on the ordinary simulation and human commander schedule.
pub fn play_match(value: &Value) -> Result<Value> {
    use crate::{
        commander_ai::Level,
        match_commands::Command,
        match_human_observation::{HumanObservationMemory, HumanShot},
        orders::OrderKey,
        recipient_snapshot::{DeliveryState, SelectedSeat},
        seat_commander::{Advance, Delivered},
    };
    value
        .get("seed")
        .and_then(Value::as_u64)
        .ok_or("seed must be an unsigned integer")?;
    let max_ticks = value["max_ticks"]
        .as_u64()
        .filter(|n| *n <= 36_000)
        .ok_or("max_ticks must be an integer from 0 to 36000")?;
    let mut state = construct(value, true)?;
    if !matches!(state.options.mode, Mode::Default | Mode::Conquest) || state.options.logistics {
        return Err("fairness AI supports ordinary Conquest without logistics".into());
    }
    let mut seats = (0..state.start.players.len())
        .map(|i| state.create_seat_commander(i, Level::Normal))
        .collect::<Result<Vec<_>>>()?;
    let mut memories = (0..seats.len())
        .map(HumanObservationMemory::new)
        .collect::<Vec<_>>();
    let mut sequences = vec![0; seats.len()];
    // playMatch observes every seat at creation, before any simulation step or action.
    let mut initial = memories
        .iter_mut()
        .map(|memory| {
            state
                .human_commander_observation(memory)
                .map(|view| Some(Delivered::Human(Box::new(view))))
        })
        .collect::<Result<Vec<_>>>()?;
    let mut delivery = DeliveryState::new(&state).map_err(|e| format!("snapshot: {e:?}"))?;
    let selected = (0..seats.len())
        .map(|slot| SelectedSeat {
            slot,
            full: true,
            omit: false,
        })
        .collect::<Vec<_>>();
    let mut pending_events = Vec::new();
    let mut generation = 0;
    while state.finish.is_none() && (state.world.game.tick as u64) < max_ticks {
        // Match::advance_tick guards active unsupported state. Its unavailable list also
        // names dormant phases (construction training, World rail and disabled logistics),
        // so it is capability metadata rather than a per-tick failure. The live host uses
        // the same runner. Keep this query bounded to that native ordinary-mode behavior.
        let mut report = state.advance_tick().map_err(|e| e.to_string())?;
        use crate::match_tick::TickPhase;
        if report.unavailable.iter().any(|phase| {
            !matches!(
                phase,
                TickPhase::UnitReserves
                    | TickPhase::Production
                    | TickPhase::Rail
                    | TickPhase::GroundEffects
                    | TickPhase::SupplyNetwork
                    | TickPhase::Convoys
            )
        }) || state.convoys.is_some()
            || state.logistics.iter().next().is_some()
            || !state.rail_runs.is_empty()
            || state
                .world
                .game
                .units
                .values()
                .any(|u| state.rules.kinds[u.kind].rail)
            || state
                .production
                .buildings()
                .any(|(_, building)| !building.jobs().is_empty())
        {
            return Err("unsupported active fairness phase".into());
        }
        pending_events.append(&mut report.events);
        let sent = state.world.game.tick % 2 == 0 || state.finish.is_some();
        if sent {
            let frame = delivery
                .capture_selected_with_tick_events(&mut state, &selected, &pending_events)
                .map_err(|e| format!("snapshot: {e:?}"))?
                .ok_or("no fairness delivery frame")?;
            for slot in 0..seats.len() {
                let _ = frame
                    .recipient(slot)
                    .map_err(|e| format!("snapshot: {e:?}"))?
                    .into_json();
            }
            drop(frame);
        }
        if state.finish.is_none() {
            let now = state.world.game.tick;
            // Build every requested observation before any brain can enqueue an order.
            let mut observed = Vec::with_capacity(seats.len());
            for (slot, seat) in seats.iter().enumerate() {
                let view = if seat.wants_view(now, sent) {
                    Some(Delivered::Human(Box::new(
                        state.human_commander_observation(&mut memories[slot])?,
                    )))
                } else {
                    initial[slot].take()
                };
                observed.push(view);
            }
            let shots = pending_events
                .iter()
                .map(HumanShot::tick)
                .collect::<Vec<_>>();
            for (slot, (seat, view)) in seats.iter_mut().zip(observed).enumerate() {
                let Advance::Human(advance) =
                    state.advance_seat_commander(seat, view, generation, &shots)?
                else {
                    return Err("unexpected ordinary commander brain".into());
                };
                if let Some(raw) = advance.input.and_then(|input| input.command) {
                    let command = Command::from_json(&raw)
                        .map_err(|e| format!("commander command: {e:?}"))?;
                    state
                        .enqueue_command(
                            OrderKey {
                                tick: now as u64 + 1,
                                issuer: slot as u32,
                                sequence: sequences[slot],
                            },
                            command,
                        )
                        .map_err(|e| format!("commander queue: {e:?}"))?;
                    sequences[slot] += 1;
                }
            }
        }
        if sent {
            pending_events.clear();
            generation += 1;
        }
    }
    let winning_team = state.finish.map(|f| f.winner);
    let winner = winning_team.and_then(|team| {
        state
            .start
            .players
            .iter()
            .position(|p| p.config.team == team)
    });
    Ok(
        json!({"ticks":state.world.game.tick,"player_vp":state.start.economy.players.iter().map(|p|p.vp).collect::<Vec<_>>(),"winner":winner,"winning_team":winning_team,"timedOut":state.finish.is_none(),"seed":state.options.seed,
        "players":state.start.players.iter().enumerate().map(|(i,p)|json!({"team":p.config.team,"spawn":{"x":p.spawn.x,"z":p.spawn.z},"vp":state.start.economy.players[i].vp})).collect::<Vec<_>>()}),
    )
}
