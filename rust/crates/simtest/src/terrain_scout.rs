//! Recorded World scouting through canonical Match memory and terrain delivery.
//! Scene maps are private test inputs. Replay output contains only delivered rows.
use serde_json::{Value, json};
use ww2_core::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    grid::SpatialGrid,
    map::AuthoritativeMap,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    random::MatchRandom,
    weather::WeatherChoice,
    world::CellChange,
    world_memory::WorldRow,
};

/// Render the actual borrowed canonical row, retaining optional key omission.
/// This is a fixture producer, not an agreed wire schema or another memory owner.
pub fn render_world_row(r: &WorldRow) -> Value {
    let row = &r.observed.row;
    let d = &row.data;
    let mut data = json!({
        "ground": char::from(d.ground).to_string(),
        "object": char::from(d.object).to_string(),
        "material": d.material,
        "groundMaterial": d.ground_material,
        "groundWear": f64::from(d.ground_wear),
        "initial": [char::from(r.initial.0).to_string(),r.initial.1],
    });
    if let Some(v) = d.trench_front {
        data["trenchFront"] = json!(f64::from(v));
    }
    if d.mine {
        data["mine"] = json!(true);
        if let Some(v) = &d.mine_material {
            data["mineMaterial"] = json!(v);
        }
    }
    if let Some(v) = r.observed.mine_owned {
        data["mineOwned"] = json!(v);
    }
    if let Some(v) = d.rail {
        data["rail"] = json!(v);
    }
    if let Some(v) = d.station {
        data["station"] = json!(v);
    }
    if let Some(v) = r.biome {
        data["biome"] = json!(v);
    }
    if let Some(v) = &r.landmark {
        data["landmark"] = json!(v);
    }
    if let Some(s) = &d.section {
        data["section"] = json!({"id":s.id,"structureId":s.structure_id,"state":s.state,"hp":s.hp,"maxHp":s.max_hp,"material":s.material,"anchor":s.anchor});
    }
    json!([
        row.cell,
        char::from(row.ch).to_string(),
        row.height,
        row.state,
        data
    ])
}
fn integer(v: &Value) -> Result<usize, String> {
    v.as_u64()
        .and_then(|n| usize::try_from(n).ok())
        .ok_or_else(|| "missing unsigned scene integer".into())
}
fn text(v: &Value) -> Result<&str, String> {
    v.as_str().ok_or_else(|| "missing scene text".into())
}
fn number(v: &Value) -> Result<f64, String> {
    v.as_f64()
        .filter(|n| n.is_finite())
        .ok_or_else(|| "invalid scene number".into())
}
fn boolean(v: &Value) -> Result<bool, String> {
    v.as_bool().ok_or_else(|| "missing scene boolean".into())
}
fn character(v: &Value) -> Result<Option<u8>, String> {
    if v.is_null() {
        return Ok(None);
    }
    let bytes = text(v)?.as_bytes();
    if bytes.len() != 1 {
        return Err("scene terrain character is not one byte".into());
    }
    Ok(Some(bytes[0]))
}
fn initialize(scene: &Value) -> Result<Match, String> {
    let mut map = scene["map"].clone();
    let runs = map["world"]["regionMapRuns"]
        .as_array()
        .ok_or("missing authoring region labels")?;
    let mut labels = Vec::new();
    for run in runs {
        labels.extend(std::iter::repeat_n(run[0].clone(), integer(&run[1])?));
    }
    map["world"]
        .as_object_mut()
        .ok_or("missing World authoring geography")?
        .remove("regionMapRuns");
    map["world"]["regionMap"] = json!(labels);
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&map).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let s = &scene["settings"];
    let seed = s["seed"].as_u64().ok_or("missing match seed")?;
    if text(&s["army"])? != "standard" {
        return Err("scouting scene requires the recorded standard army".into());
    }
    let players = s["players"]
        .as_array()
        .ok_or("missing scene players")?
        .iter()
        .map(|p| {
            Ok(PlayerConfig {
                name: text(&p["name"])?.into(),
                team: i32::try_from(p["team"].as_i64().ok_or("missing team")?)
                    .map_err(|e| e.to_string())?,
                faction: Faction::try_from(
                    u8::try_from(integer(&p["faction"])?).map_err(|e| e.to_string())?,
                )
                .map_err(|e| e.to_string())?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let state = Match::new_world(
        &map,
        MatchOptions {
            seed,
            mode: Mode::World,
            players,
            shuffle: boolean(&s["shuffle"])?,
            army: ArmySize::Standard,
            weather: WeatherChoice::Map,
            weather_seed: Some(
                u32::try_from(integer(&s["weatherSeed"])?).map_err(|e| e.to_string())?,
            ),
            living_weather: boolean(&s["living"])?,
            map_key: None,
            tech: boolean(&s["tech"])?,
            supply: boolean(&s["supply"])?,
            logistics: boolean(&s["logistics"])?,
            sandbox: false,
        },
        MatchRandom::new(seed),
    )?;
    // The recorder preserves the complete original constructor inventory.
    // No expected terrain row, filtered actor list or post-state initializes Match.
    let actual = state
        .world
        .game
        .units
        .values()
        .map(|u| {
            json!({
                "id":u.id,"type":state.rules.kinds[u.kind].name,
                "owner":if u.owner==ww2_core::game::LOCAL_DEFENDER_OWNER {-1}else{u.owner as i64},
                "x":u.x,"z":u.z,"hp":u.hp,
            })
        })
        .collect::<Vec<_>>();
    let expected = scene["actors"]
        .as_array()
        .ok_or("missing original actor inventory")?;
    if actual.len() != expected.len()
        || actual
            .iter()
            .zip(expected)
            .any(|(a, b)| !same_numbers(a, b))
    {
        return Err("World constructor differs from the recorded original actor inventory".into());
    }
    Ok(state)
}
/// Exact value comparison with IEEE number bits and strict omission semantics.
pub fn same_numbers(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => {
            a.as_f64().map(f64::to_bits) == b.as_f64().map(f64::to_bits)
        }
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| same_numbers(a, b))
        }
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len()
                && a.iter()
                    .all(|(k, a)| b.get(k).is_some_and(|b| same_numbers(a, b)))
        }
        _ => a == b,
    }
}
/// Run declared scene actions through existing World owners. Expected receipts
/// and dataPool are never read by this producer. Every returned row is full JSON.
pub fn replay(input: &Value) -> Result<Value, String> {
    let mut state = initialize(&input["scene"])?;
    let random = state.start.random.snapshot();
    let environment_seed = state.start.environment_seed;
    let initial_actor_count = state.world.game.units.len();
    let mut receipts = Vec::new();
    for action in input["actions"].as_array().ok_or("missing scene actions")? {
        match text(&action["op"])? {
            "vision" => {
                state.world.game.tick = action["tick"].as_i64().ok_or("missing scene clock")?;
                state.grid = SpatialGrid::new(&state.world.game.units, 16.);
                state.refresh_world_visibility()?;
            }
            "move" => {
                let id = u32::try_from(integer(&action["id"])?).map_err(|e| e.to_string())?;
                let u = state
                    .world
                    .game
                    .units
                    .get_mut(&id)
                    .ok_or("missing declared scene actor")?;
                u.x = number(&action["x"])?;
                u.z = number(&action["z"])?;
                state.grid = SpatialGrid::new(&state.world.game.units, 16.);
            }
            "mutate" => {
                let change = &action["change"];
                state
                    .world
                    .mutate_cell(
                        integer(&action["c"])?,
                        CellChange {
                            ground: character(&change["ground"])?,
                            object: character(&change["object"])?,
                            mine: change["mine"].as_bool(),
                            material: change["material"].as_str().map(str::to_owned),
                            height: change["height"]
                                .as_i64()
                                .map(i8::try_from)
                                .transpose()
                                .map_err(|e| e.to_string())?,
                        },
                    )
                    .map_err(|e| e.to_string())?;
            }
            "mine" => {
                let c = integer(&action["c"])?;
                state.world.mines.insert(
                    c,
                    i32::try_from(action["owner"].as_i64().ok_or("missing mine owner")?)
                        .map_err(|e| e.to_string())?,
                );
                state.world.mine_seen.insert(
                    c,
                    u32::try_from(integer(&action["seen"])?).map_err(|e| e.to_string())?,
                );
            }
            "log" => {
                state
                    .world
                    .log_cell(integer(&action["c"])?, boolean(&action["look"])?)
                    .map_err(|e| e.to_string())?;
            }
            "reveal" => {
                state.world.game.reveal = boolean(&action["value"])?;
            }
            "read" => {
                let slot = integer(&action["slot"])?;
                let full = boolean(&action["full"])?;
                let rows = state
                    .world_terrain(slot, full)?
                    .iter()
                    .map(render_world_row)
                    .collect::<Vec<_>>();
                let team = state.world.game.players[slot].team;
                let memory_version = state
                    .world_runtime
                    .as_ref()
                    .ok_or("missing World runtime")?
                    .memory
                    .team(team)
                    .ok_or("missing observed team memory")?
                    .version();
                receipts.push(json!({"name":text(&action["name"])?,"slot":slot,"full":full,"rowCount":rows.len(),"tick":state.world.game.tick,"rows":rows,"memoryVersion":memory_version}));
            }
            _ => {
                return Err("unsupported scouting scene action".into());
            }
        }
        if state.start.random.snapshot() != random
            || state.start.environment_seed != environment_seed
            || state.world.game.units.len() != initial_actor_count
        {
            return Err(
                "scouting component changed constructor randomness or actor membership".into(),
            );
        }
    }
    Ok(
        json!({"actorCount":initial_actor_count,"actionCount":input["actions"].as_array().unwrap().len(),"receipts":receipts}),
    )
}
/// Expand the separately recorded Source expectations for artifact comparison.
/// This does not feed the native replay or publish the private input scene.
pub fn expand_source_receipts(input: &Value) -> Result<Value, String> {
    let pool = input["dataPool"]
        .as_array()
        .ok_or("missing Source fixture data pool")?;
    let mut receipts = input["receipts"]
        .as_array()
        .ok_or("missing Source receipts")?
        .clone();
    for receipt in &mut receipts {
        for row in receipt["rows"]
            .as_array_mut()
            .ok_or("missing Source rows")?
        {
            let index = integer(&row[4])?;
            row[4] = pool
                .get(index)
                .ok_or("Source row data index is outside pool")?
                .clone();
        }
    }
    Ok(
        json!({"actorCount":input["scene"]["actors"].as_array().ok_or("missing original actors")?.len(),"actionCount":input["actions"].as_array().ok_or("missing original actions")?.len(),"receipts":receipts}),
    )
}
