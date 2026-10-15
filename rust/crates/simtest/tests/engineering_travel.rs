//! Original ENG-59 travel acceptance through actual ordinary Match ticks.
//! The authentic output fixture is generated only after a coordinator slot grant.

use flate2::read::GzDecoder;
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Read, path::Path};
use ww2_core::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::Point,
    grid::SpatialGrid,
    map::AuthoritativeMap,
    match_abilities::AbilityState,
    match_combat::CombatState,
    match_commands::{Command, MatchCommands},
    match_firing::FiringState,
    match_recovery::RecoveryState,
    match_runtime::{LivingWeather, Match, MatchOptions, PlayerConfig},
    match_tick::TickPhase,
    unit_publication::{PublicationParts, StagedUnit, prepare_publication},
    weather::{WeatherChoice, WeatherKind, WeatherPlan},
    world::CellChange,
};

fn number(v: &Value) -> f64 {
    v.as_f64().expect("finite Source input")
}

fn point(v: &Value) -> Point {
    Point { x: number(&v["x"]), z: number(&v["z"]) }
}

fn p(p: Point) -> Value {
    json!({"x":p.x,"z":p.z})
}

fn runs(values: impl IntoIterator<Item = Value>) -> Value {
    let mut rows: Vec<Value> = Vec::new();
    for value in values {
        if rows.last().is_some_and(|row| row[0] == value) {
            let last = rows.last_mut().unwrap();
            last[1] = json!(last[1].as_u64().unwrap() + 1);
        } else {
            rows.push(json!([value, 1]));
        }
    }
    json!(rows)
}

fn compare(actual: &Value, expected: &Value, label: &str) {
    fn visit(actual: &Value, raw: &Value, bits: &Value, path: &str, label: &str) {
        match (actual, raw) {
            (Value::Number(a), Value::Number(_)) => {
                assert_eq!(
                    format!("{:016x}", a.as_f64().unwrap().to_bits()),
                    bits[path].as_str().unwrap(),
                    "{label}:{path}"
                );
            }
            (Value::Array(a), Value::Array(b)) => {
                assert_eq!(a.len(), b.len(), "{label}:{path}");
                for (i, (a, b)) in a.iter().zip(b).enumerate() {
                    visit(a, b, bits, &format!("{path}.{i}"), label);
                }
            }
            (Value::Object(a), Value::Object(b)) => {
                assert_eq!(a.keys().collect::<Vec<_>>(), b.keys().collect::<Vec<_>>(), "{label}:{path}");
                for (key, value) in a {
                    let child = if path.is_empty() { key.clone() } else { format!("{path}.{key}") };
                    visit(value, &b[key], bits, &child, label);
                }
            }
            _ => assert_eq!(actual, raw, "{label}:{path}"),
        }
    }
    visit(actual, &expected["raw"], &expected["floatBits"], "", label);
}

fn projection(m: &Match, id: u32) -> Value {
    let g = &m.world.game;
    assert_eq!(g.units.len(), 1);
    let u = &g.units[&id];
    let control = &m.commands.units[&id];
    let living = m.start.living.as_ref().unwrap();
    json!({
        "tick":g.tick,
        "actor":{
            "id":id,"type":m.rules.kinds[u.kind].name,"owner":u.owner,
            "x":u.x,"z":u.z,"rot":u.rot,"hp":u.hp,"supp":u.supp,
            "moveSpeed":u.move_speed,"travelDir":u.travel_dir,"vx":u.vx.unwrap_or(0.0),"vz":u.vz.unwrap_or(0.0),"drift":u.drift,
            "path":u.path.iter().copied().map(p).collect::<Vec<_>>(),"worldGoal":u.world_goal.map(p),
            "still":u.still,"repath":u.repath,"stuck":u.stuck,"dust":u.dust,"sprint":u.sprint,
            "holdPos":u.hold_pos,"autoRetreat":control.auto_retreat,"holdFire":control.hold_fire,"auto":control.auto_cast
        },
        "weather":{"now":m.start.weather.now.name(),"next":m.start.weather.next.map(WeatherKind::name),"at":m.start.weather.at},
        "wx":{"rain":g.rain,"raining":living.raining,"wet":g.wet.unwrap(),"next":living.next},
        "seed":m.start.environment_seed,"wind":{"a":m.start.wind.0,"v":m.start.wind.1},
        "terrain":{
            "chars":String::from_utf8(g.chars.clone()).unwrap(),
            "ground":String::from_utf8(g.ground.clone().unwrap()).unwrap(),
            "objects":String::from_utf8(g.objects.clone().unwrap()).unwrap(),
            "flags":runs(g.flags.iter().map(|v|json!(v))),
            "height":g.height.as_ref().map(|v|runs(v.iter().map(|v|json!(v)))),
            "wear32":runs(g.wear.as_ref().unwrap().iter().map(|v|json!(v.to_bits()))),
            "hp32":runs(g.cell_hp.iter().map(|v|json!(v.to_bits()))),
            "state":runs(m.world.cell_state.iter().map(|v|json!(v)))
        }
    })
}

fn scene(input: &Value, spec: &Value, trace: &Value) -> (Match, u32) {
    let setup = &trace["setup"];
    let mut authored = input["map"].clone();
    let w = authored["w"].as_u64().unwrap() as usize;
    let h = authored["h"].as_u64().unwrap() as usize;
    authored["rows"] = json!(vec![spec["surface"].as_str().unwrap().repeat(w); h]);
    assert_eq!(authored, trace["map"], "shared authored map");
    // Source's blank() omits its name. The native authoring validator requires
    // text; an empty name keeps the same default mood and Clear weather.
    assert!(authored.get("name").is_none());
    authored["name"] = json!("");
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&authored).unwrap()).unwrap();
    let mut m = Match::new(&map, MatchOptions {
        seed: input["nativeSeed"].as_u64().unwrap(), mode: Mode::Default,
        players: input["names"].as_array().unwrap().iter().enumerate().map(|(slot, name)| PlayerConfig {
            name: name.as_str().unwrap().into(), team: setup["teams"][slot].as_i64().unwrap() as i32,
            faction: Faction::try_from(setup["factions"][slot].as_u64().unwrap() as u8).unwrap(),
        }).collect(),
        shuffle: false, army: ArmySize::Standard, weather: WeatherChoice::Map,
        weather_seed: Some(1), living_weather: true, map_key: None,
        tech: false, supply: true, logistics: false, sandbox: false,
    }).unwrap();

    // Match the original clearFixtureUnits setup without replacing World or its
    // immutable initial-terrain identity, which the canonical routing owner binds.
    let footprint: Vec<_> = m.building_cells.keys().copied().collect();
    for &cell in &footprint {
        m.world.sections.remove(&cell);
        m.world.game.sections.remove(&(cell as u32));
    }
    m.world.structures.retain(|s| s.kind != "building");
    m.world.detached_sections.retain(|_, s| !s.metadata.structure_id.starts_with("building:"));
    for cell in footprint {
        let i = m.world.initial();
        let change = CellChange {
            ground: Some(i.ground[cell]), object: Some(i.objects[cell]),
            mine: Some(i.mines[cell] != 0), height: Some(i.height.as_ref().map_or(0, |h| h[cell])),
            ..Default::default()
        };
        m.world.mutate_cell(cell, change).unwrap();
    }
    for id in m.world.game.units.keys().copied().collect::<Vec<_>>() {
        m.production.remove_building(id);
    }
    m.world.game.units.clear();
    m.building_cells.clear();
    m.building_rallies.clear();
    m.recruit_metadata.clear();
    m.commands = MatchCommands::new(4096);
    m.firing = FiringState::default();
    m.combat = CombatState::default();
    m.abilities = AbilityState::default();
    m.recovery = RecoveryState::default();
    m.grid = SpatialGrid::new(&m.world.game.units, 16.0);
    for (slot, player) in m.start.players.iter_mut().enumerate() {
        player.spawn = point(&input["spawnSentinel"]);
        m.start.economy.players[slot].mp = number(&setup["players"][slot]["mp"]);
    }
    if !spec["depth"].is_null() {
        // Original travel() writes wear only. It does not touch state or surface wear.
        m.world.game.wear.as_mut().unwrap().fill(number(&spec["depth"]) as f32);
    }
    m.start.environment_seed = setup["seed"].as_u64().unwrap() as u32;
    m.start.wind = (number(&setup["wind"]["a"]), number(&setup["wind"]["v"]));
    m.start.weather = WeatherPlan::fixed(WeatherKind::Clear);
    assert_eq!(setup["weather"]["now"], "clear");
    m.start.living = Some(LivingWeather {
        raining: setup["wx"]["raining"].as_bool().unwrap(), next: number(&setup["wx"]["next"]),
    });
    m.world.game.rain = number(&setup["wx"]["rain"]);
    m.world.game.wet = Some(number(&setup["wx"]["wet"]));
    let id = setup["id"].as_u64().unwrap() as u32;
    m.next_unit_id = id;
    let mut staged = StagedUnit::ordinary(
        &m.world.game, &m.rules, 0, m.rules.kind(spec["type"].as_str().unwrap()).unwrap(),
        id, 0, point(&input["spawnSentinel"]),
        Some(Point { x: number(&input["x"]), z: number(&spec["z"]) }),
    ).unwrap();
    staged.unit.rot = 0.0;
    staged.control.aim = 0.0;
    staged.control.auto_retreat = false;
    prepare_publication(PublicationParts {
        units: &mut m.world.game.units, grid: &mut m.grid, controls: &mut m.commands.units,
        recruitment: &mut m.recruit_metadata, next_id: &mut m.next_unit_id,
    }, &m.rules, 2, vec![staged]).unwrap().publish_with(|u, _| {
        m.combat.initialize(u.id);
        m.firing.initialize(u.id);
        m.abilities.initialize(u.id);
        m.recovery.initialize(u.id);
    });
    assert_eq!(m.next_unit_id, setup["nextId"].as_u64().unwrap() as u32);
    compare(&projection(&m, id), &setup["beforeMove"], spec["name"].as_str().unwrap());
    (m, id)
}

#[test]
fn original_roads_and_mud_selected_travel_uses_full_match_ticks() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let input: Value = serde_json::from_slice(&std::fs::read(root.join("tests/fixtures/engineering-travel-input.json")).unwrap()).unwrap();
    let bytes = std::fs::read(root.join("tests/fixtures/engineering-travel.json.gz"))
        .expect("authentic Source capture is pending; generate only with Root's slot grant");
    let mut text = String::new();
    GzDecoder::new(bytes.as_slice()).read_to_string(&mut text).unwrap();
    let fixture: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(fixture["reference"], "e4e579a838aabae3b1d9dadbc17a0a87147bc52d");
    assert_eq!(fixture["node"], "v24.21.0");
    assert_eq!(fixture["arch"], "x64");
    assert_eq!(fixture["input"], input);
    assert_eq!(input["options"], json!({}));
    assert_eq!((input["warmupTicks"].as_u64().unwrap(), input["measuredTicks"].as_u64().unwrap()), (40, 40));
    let traces = fixture["traces"].as_array().unwrap();
    let cases = input["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 5);
    assert_eq!(traces.len(), 5);
    let mut distances = BTreeMap::new();
    let mut ticks = 0;
    let mut frames = 0;
    for (spec, trace) in cases.iter().zip(traces) {
        assert_eq!(spec["name"], trace["name"]);
        let name = spec["name"].as_str().unwrap();
        let (mut m, id) = scene(&input, spec, trace);
        let command = Command::from_json(&trace["request"]).unwrap();
        m.begin_path_tick(4096, 20).unwrap();
        assert!(m.execute_command(0, &command, false).unwrap().is_none());
        let expected = trace["frames"].as_array().unwrap();
        assert_eq!(expected.len(), 81);
        compare(&projection(&m, id), &expected[0], &format!("{name}:0"));
        frames += 1;
        let mut before = None;
        for tick in 1..=80 {
            let report = m.advance_tick().unwrap_or_else(|e| panic!("{name}:{tick}: {e}"));
            assert_eq!(report.tick, tick);
            assert_eq!(report.unit_order, [id]);
            assert_eq!(report.prior.len(), 1);
            assert!(report.phases.contains(&TickPhase::Movement));
            assert!(report.phases.contains(&TickPhase::LivingWeather));
            assert!(!report.unavailable.contains(&TickPhase::Movement));
            assert!(!report.unavailable.contains(&TickPhase::LivingWeather));
            assert_eq!(m.grid.owned_by(&m.world.game.units, 0), [id]);
            assert!(report.events.is_empty() && report.retired.is_empty());
            assert!(m.firing.events.is_empty() && m.support.events.is_empty() && m.aviation.events.is_empty());
            assert!(m.finish.is_none());
            compare(&projection(&m, id), &expected[tick as usize], &format!("{name}:{tick}"));
            if tick == 40 { before = Some(m.world.game.units[&id].x); }
            frames += 1;
            ticks += 1;
        }
        let travel = m.world.game.units[&id].x - before.unwrap();
        assert_eq!(travel.to_bits(), number(&trace["travel"]).to_bits(), "{name}: actual measured travel");
        distances.insert(name, travel);
    }
    let grass = distances["tank-grass"];
    assert!(grass > 0.0);
    assert!((distances["tank-shallow-mud"] / grass - 0.7).abs() < 0.1, "a tank in shallow mud");
    assert!((distances["tank-deep-mud"] / grass - 0.35).abs() < 0.1, "a tank in deep mud");
    assert!((distances["rifle-shallow-mud"] - distances["rifle-grass"]).abs() < 0.2, "infantry walk through mud at full speed");
    assert_eq!((ticks, frames), (400, 405));
}
