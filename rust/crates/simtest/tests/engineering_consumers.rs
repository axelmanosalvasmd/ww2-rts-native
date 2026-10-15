//! Source-only five-row engineering preparation through actual Match consumers.
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
    Point {
        x: number(&v["x"]),
        z: number(&v["z"]),
    }
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
                assert_eq!(
                    a.keys().collect::<Vec<_>>(),
                    b.keys().collect::<Vec<_>>(),
                    "{label}:{path}"
                );
                for (key, value) in a {
                    let child = if path.is_empty() {
                        key.clone()
                    } else {
                        format!("{path}.{key}")
                    };
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

fn authored_map(input: &Value, terrain: &str, w: usize, h: usize) -> Value {
    let mut map = input["map"].clone();
    map["w"] = json!(w);
    map["h"] = json!(h);
    map["rows"] = json!(
        (0..h)
            .map(|y| {
                if terrain == "road-lanes" {
                    (if (1..=3).contains(&y) { "D" } else { "." }).repeat(w)
                } else if terrain == "mud-gap" {
                    format!(
                        "{}{}{}",
                        ".".repeat(10),
                        if (7..=13).contains(&y) && y != 11 {
                            "M"
                        } else {
                            "."
                        },
                        ".".repeat(w - 11)
                    )
                } else {
                    terrain.repeat(w)
                }
            })
            .collect::<Vec<_>>()
    );
    map
}

fn assert_input_json_bits(actual: &Value, expected: &Value, path: &str) {
    match (actual, expected) {
        (Value::Number(a), Value::Number(b)) => assert_eq!(
            a.as_f64().expect("JSON number").to_bits(),
            b.as_f64().expect("JSON number").to_bits(),
            "{path}: numeric input bits"
        ),
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{path}: input array length");
            for (index, (a, b)) in a.iter().zip(b).enumerate() {
                assert_input_json_bits(a, b, &format!("{path}/{index}"));
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(
                a.keys().collect::<Vec<_>>(),
                b.keys().collect::<Vec<_>>(),
                "{path}: input object keys"
            );
            for (key, value) in a {
                assert_input_json_bits(value, &b[key], &format!("{path}/{key}"));
            }
        }
        _ => assert_eq!(actual, expected, "{path}: input value and type"),
    }
}

fn fixture() -> Value {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let input: Value = serde_json::from_slice(
        &std::fs::read(root.join("tests/fixtures/engineering-consumers-input.json")).unwrap(),
    )
    .unwrap();
    let bytes = std::fs::read(root.join("tests/fixtures/engineering-consumers.json.gz"))
        .expect("authentic Source capture is pending; run only with Root's grant");
    let mut text = String::new();
    GzDecoder::new(bytes.as_slice())
        .read_to_string(&mut text)
        .unwrap();
    let fixture: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        fixture["reference"],
        "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
    );
    assert_eq!(fixture["node"], "v24.21.0");
    assert_eq!(fixture["arch"], "x64");
    assert_input_json_bits(&fixture["input"], &input, "input");
    fixture
}

fn environment(m: &Match) -> Value {
    let living = m.start.living.as_ref().unwrap();
    json!({"tick":m.world.game.tick,
        "weather":{"now":m.start.weather.now.name(),"next":m.start.weather.next.map(WeatherKind::name),"at":m.start.weather.at},
        "wx":{"rain":m.world.game.rain,"raining":living.raining,"wet":m.world.game.wet.unwrap(),"next":living.next},
        "seed":m.start.environment_seed,"wind":{"a":m.start.wind.0,"v":m.start.wind.1}})
}

fn restore_environment(m: &mut Match, setup: &Value) {
    // Initial scene facts only. Recorded subsequent frames never enter native state.
    let e = &setup["environment"]["raw"];
    m.start.environment_seed = e["seed"].as_u64().unwrap() as u32;
    m.start.wind = (number(&e["wind"]["a"]), number(&e["wind"]["v"]));
    m.start.weather = WeatherPlan {
        now: WeatherKind::try_from(e["weather"]["now"].as_str().unwrap()).unwrap(),
        next: e["weather"]["next"]
            .as_str()
            .map(|v| WeatherKind::try_from(v).unwrap()),
        at: e["weather"]["at"].as_u64().unwrap() as u32,
    };
    m.world.game.weather = Some(m.start.weather.now as usize);
    m.start.living = Some(LivingWeather {
        raining: e["wx"]["raining"].as_bool().unwrap(),
        next: number(&e["wx"]["next"]),
    });
    m.world.game.rain = number(&e["wx"]["rain"]);
    m.world.game.wet = Some(number(&e["wx"]["wet"]));
    compare(
        &environment(m),
        &setup["environment"],
        "initial environment",
    );
}

fn make(input: &Value, authored: &Value, setup: &Value, opts: &Value) -> Match {
    let mut map = authored.clone();
    if map.get("name").is_none() {
        map["name"] = json!("");
    }
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&map).unwrap()).unwrap();
    Match::new(
        &map,
        MatchOptions {
            seed: input["nativeSeed"].as_u64().unwrap(),
            mode: Mode::Default,
            players: input["names"]
                .as_array()
                .unwrap()
                .iter()
                .enumerate()
                .map(|(slot, name)| PlayerConfig {
                    name: name.as_str().unwrap().into(),
                    team: setup["teams"][slot].as_i64().unwrap() as i32,
                    faction: Faction::try_from(setup["factions"][slot].as_u64().unwrap() as u8)
                        .unwrap(),
                })
                .collect(),
            shuffle: false,
            army: ArmySize::Standard,
            weather: WeatherChoice::from_reference(opts["weather"].as_str()),
            weather_seed: Some(opts["weatherSeed"].as_u64().unwrap_or(1) as u32),
            living_weather: true,
            map_key: opts["mapKey"].as_str().map(str::to_owned),
            tech: false,
            supply: true,
            logistics: false,
            sandbox: false,
        },
    )
    .unwrap()
}

fn clear(m: &mut Match) {
    // Retain the canonical World and its initial-terrain identity for routing.
    let footprint: Vec<_> = m.building_cells.keys().copied().collect();
    for &cell in &footprint {
        m.world.sections.remove(&cell);
        m.world.game.sections.remove(&(cell as u32));
    }
    m.world.structures.retain(|s| s.kind != "building");
    m.world
        .detached_sections
        .retain(|_, s| !s.metadata.structure_id.starts_with("building:"));
    for cell in footprint {
        let i = m.world.initial();
        let change = CellChange {
            ground: Some(i.ground[cell]),
            object: Some(i.objects[cell]),
            mine: Some(i.mines[cell] != 0),
            height: Some(i.height.as_ref().map_or(0, |h| h[cell])),
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
}

fn publish(m: &mut Match, input: &Value, setup: &Value) {
    if setup["actors"].as_array().unwrap().is_empty() {
        m.next_unit_id = setup["nextId"].as_u64().unwrap() as u32;
    }
    for spec in setup["actors"].as_array().unwrap() {
        let id = spec["id"].as_u64().unwrap() as u32;
        let owner = spec["owner"].as_u64().unwrap() as usize;
        let name = spec["type"].as_str().unwrap();
        m.next_unit_id = id;
        if spec["air"].is_null() {
            assert!(
                spec["cells"].as_array().unwrap().is_empty(),
                "only canonical ordinary actors are republished"
            );
            let mut staged = StagedUnit::ordinary(
                &m.world.game,
                &m.rules,
                owner,
                m.rules.kind(name).unwrap(),
                id,
                m.world.game.units.len() as u32,
                point(&input["spawnSentinel"]),
                Some(point(spec)),
            )
            .unwrap();
            staged.unit.rot = number(&spec["rot"]);
            staged.control.aim = number(&spec["aim"]);
            staged.control.auto_retreat = spec["autoRetreat"].as_bool().unwrap();
            prepare_publication(
                PublicationParts {
                    units: &mut m.world.game.units,
                    grid: &mut m.grid,
                    controls: &mut m.commands.units,
                    recruitment: &mut m.recruit_metadata,
                    next_id: &mut m.next_unit_id,
                },
                &m.rules,
                2,
                vec![staged],
            )
            .unwrap()
            .publish_with(|u, _| {
                m.combat.initialize(u.id);
                m.firing.initialize(u.id);
                m.abilities.initialize(u.id);
                m.recovery.initialize(u.id);
            });
        } else {
            assert_eq!(m.spawn_aircraft(owner, name).unwrap(), id);
            let patch = json!({"x":spec["x"],"z":spec["z"],"rot":spec["rot"],"autoRetreat":spec["autoRetreat"],"air":spec["air"]});
            crate::shared::aviation::patch(m, id, &patch);
            m.commands.units.get_mut(&id).unwrap().aim = number(&spec["aim"]);
        }
    }
    assert_eq!(m.next_unit_id, setup["nextId"].as_u64().unwrap() as u32);
    m.grid.rebuild(&m.world.game.units);
}

fn scene(input: &Value, trace: &Value, spec: Option<&Value>) -> Match {
    let setup = &trace["setup"];
    let mut m = make(input, &trace["map"], setup, &json!({}));
    clear(&mut m);
    for (slot, p) in m.start.players.iter_mut().enumerate() {
        assert_eq!(setup["players"][slot]["spawn"], input["spawnSentinel"]);
        p.spawn = point(&input["spawnSentinel"]);
        m.start.economy.players[slot].mp = number(&setup["players"][slot]["mp"]);
    }
    if let Some(spec) = spec {
        if let Some(depth) = spec["depth"].as_f64() {
            m.world.game.wear.as_mut().unwrap().fill(depth as f32);
        }
    }
    restore_environment(&mut m, setup);
    publish(&mut m, input, setup);
    m
}

fn tick(m: &mut Match, label: &str) {
    let report = m.advance_tick().unwrap_or_else(|e| panic!("{label}: {e}"));
    for phase in [TickPhase::Movement, TickPhase::LivingWeather] {
        assert!(report.phases.contains(&phase), "{label}: actual {phase:?}");
        assert!(
            !report.unavailable.contains(&phase),
            "{label}: unavailable {phase:?}"
        );
    }
    assert!(m.finish.is_none(), "{label}: scene ended early");
}

fn travels(group: &str) -> BTreeMap<String, f64> {
    let f = fixture();
    let input = &f["input"];
    let specs = input["travel"].as_array().unwrap();
    let traces = f["traces"].as_array().unwrap();
    assert_eq!((specs.len(), traces.len()), (22, 22));
    let mut distances = BTreeMap::new();
    let mut total = 0;
    for (spec, trace) in specs
        .iter()
        .zip(traces)
        .filter(|(spec, _)| spec["group"] == group)
    {
        assert_eq!(spec["name"], trace["name"]);
        let name = spec["name"].as_str().unwrap();
        assert_eq!(
            trace["map"],
            authored_map(
                input,
                spec["terrain"].as_str().unwrap(),
                spec["w"].as_u64().unwrap() as usize,
                spec["h"].as_u64().unwrap() as usize
            )
        );
        let actor = &trace["setup"]["actors"][0];
        assert_eq!(trace["setup"]["actors"].as_array().unwrap().len(), 1);
        assert_eq!(
            (
                actor["type"].clone(),
                actor["owner"].clone(),
                actor["x"].clone(),
                actor["z"].clone()
            ),
            (
                spec["type"].clone(),
                json!(0),
                spec["x"].clone(),
                spec["z"].clone()
            )
        );
        assert_eq!(actor["rot"].as_f64(), Some(0.0));
        let mut m = scene(input, trace, Some(spec));
        let id = trace["setup"]["actors"][0]["id"].as_u64().unwrap() as u32;
        compare(&projection(&m, id), &trace["beforeMove"], name);
        let request = Command::from_json(&trace["request"]).unwrap();
        assert_eq!(
            trace["request"],
            json!({"t":"move","orders":[[id,spec["targetX"],spec["z"]]]})
        );
        m.begin_path_tick(4096, 20).unwrap();
        assert!(m.execute_command(0, &request, false).unwrap().is_none());
        if spec["straight"] == true {
            m.world.game.units.get_mut(&id).unwrap().path = vec![Point {
                x: number(&spec["targetX"]),
                z: number(&spec["z"]),
            }];
        }
        let frames = trace["frames"].as_array().unwrap();
        let count = spec["ticks"].as_u64().unwrap();
        assert_eq!(frames.len(), count as usize + 1);
        compare(&projection(&m, id), &frames[0], name);
        let mut before = m.world.game.units[&id].x;
        for n in 1..=count {
            tick(&mut m, name);
            compare(
                &projection(&m, id),
                &frames[n as usize],
                &format!("{name}:{n}"),
            );
            if n == spec["warmup"].as_u64().unwrap() {
                before = m.world.game.units[&id].x;
            }
        }
        let distance = m.world.game.units[&id].x - before;
        assert_eq!(
            distance.to_bits(),
            number(&trace["travel"]).to_bits(),
            "{name}: actual displacement"
        );
        distances.insert(name.to_owned(), distance);
        total += count;
    }
    assert_eq!(
        (distances.len(), total),
        if group == "roads" {
            (7, 560)
        } else {
            (15, 1420)
        }
    );
    distances
}

#[test]
fn complete_roads_and_mud_travel_uses_actual_match_movement() {
    let d = travels("roads");
    let grass = d["tank-grass"];
    let road_speed = ww2_core::pathfinding::PathRules::compiled()
        .unwrap()
        .road_speed;
    assert!(
        (d["tank-road"] / grass - road_speed).abs() < 0.1,
        "a tank on a road drives roadSpeed times faster"
    );
    let edge = d["tank-edge"] / grass;
    assert!(
        edge > 1.03 && edge < road_speed - 0.1,
        "a tank half on the road gets part of the bonus"
    );
    assert!(
        (d["tank-shallow-mud"] / grass - 0.7).abs() < 0.1,
        "a tank in shallow mud"
    );
    assert!(
        (d["tank-deep-mud"] / grass - 0.35).abs() < 0.1,
        "a tank in deep mud"
    );
    assert!(
        (d["rifle-shallow-mud"] - d["rifle-grass"]).abs() < 0.2,
        "infantry walk through mud at full speed"
    );
}

#[test]
fn all_original_weather_travel_ratios_use_actual_match_movement() {
    let d = travels("weather");
    assert!(
        d["clear-tank"] > 5.0 && d["clear-rifle"] > 5.0,
        "units move in clear weather"
    );
    for (name, baseline, target, tolerance) in [
        ("rain-tank", "clear-tank", 0.8, 0.06),
        ("rain-tank-road", "clear-tank-road", 1.0, 0.03),
        ("rain-rifle", "clear-rifle", 1.0, 0.03),
        ("mud-tank", "clear-tank", 0.8, 0.06),
        ("mud-rifle", "clear-rifle", 0.9, 0.03),
        ("snow-tank", "clear-tank", 0.85, 0.03),
        ("mud-tank-road", "clear-tank-road", 1.0, 0.03),
        ("sink-mud", "sink-clear", 1.0, 0.03),
        ("snow-tank-road", "clear-tank-road", 0.85, 0.03),
        ("snow-rifle", "clear-rifle", 0.9, 0.03),
        ("fog-tank", "clear-tank", 1.0, 0.03),
    ] {
        assert!(
            (d[name] / d[baseline] - target).abs() < tolerance,
            "{name}: original travel bound"
        );
    }
}

#[test]
fn original_road_and_mud_route_predicates_use_actual_pathfinder() {
    use ww2_core::{
        navigation::{TerrainView, Versions},
        pathfinding::{PathRules, PathTerrain, Pathfinding},
    };
    let f = fixture();
    let traces = f["routes"].as_array().unwrap();
    assert_eq!(traces.len(), 4);
    for (spec, trace) in f["input"]["routes"].as_array().unwrap().iter().zip(traces) {
        assert_eq!(
            trace["map"],
            authored_map(
                &f["input"],
                spec["terrain"].as_str().unwrap(),
                if spec["terrain"] == "road-lanes" {
                    80
                } else {
                    20
                },
                20
            )
        );
        let mut m = scene(&f["input"], trace, None);
        for c in 0..m.world.game.chars.len() {
            if m.world.game.chars[c] == b'M' {
                m.world.game.wear.as_mut().unwrap()[c] = 1.0;
            }
        }
        let g = &m.world.game;
        assert!(
            g.flags.iter().all(|flag| flag & 3 != 3),
            "authored open/mud/road scene has no walls"
        );
        let near = vec![0; g.flags.len()];
        let hazards = vec![false; g.flags.len()];
        let terrain = PathTerrain {
            navigation: TerrainView {
                width: g.w,
                height: g.h,
                flags: &g.flags,
                heights: g.height.as_deref(),
                world_known: false,
                knowledge_id: 0,
                versions: Versions::default(),
                changed: None,
                changes: &[],
            },
            near_walls: &near,
            known_mines: &hazards,
            fires: &hazards,
            bunkers: &[],
            wear: g.wear.as_deref(),
            wetness: g.wet.unwrap(),
            roads: m.world.roads,
        };
        let result = Pathfinding::new()
            .find_path(
                terrain,
                PathRules::compiled().unwrap(),
                &m.rules.kinds[m.rules.kind(spec["type"].as_str().unwrap()).unwrap()],
                point(&spec["from_"]),
                point(&spec["to"]),
                None,
            )
            .unwrap();
        let path = result.waypoints;
        compare(
            &json!(path.iter().copied().map(p).collect::<Vec<_>>()),
            &trace["path"],
            spec["name"].as_str().unwrap(),
        );
        let wet = path.iter().any(|p| {
            g.chars[(p.z / 2.0).floor() as usize * g.w + (p.x / 2.0).floor() as usize] == b'M'
        });
        let on_road = path
            .iter()
            .filter(|p| (1..=3).contains(&((p.z / 2.0).floor() as i32)))
            .count();
        match spec["name"].as_str().unwrap() {
            "mud-gap-tank" => assert!(
                path.len() > 1 && !wet,
                "a vehicle routes around mud when dry ground is near"
            ),
            "mud-gap-rifle" => assert_eq!(path.len(), 1, "infantry walk straight through mud"),
            "adjacent-road-tank" => assert!(on_road > 5, "a vehicle takes the road beside it"),
            "adjacent-road-rifle" => assert_eq!(on_road, 0, "infantry ignore the road"),
            _ => panic!("unexpected authored query"),
        }
    }
}

fn apply_event(m: &mut Match, event: &Value) {
    if let Some(kind) = event["weather"].as_str() {
        m.start.weather = WeatherPlan::fixed(WeatherKind::try_from(kind).unwrap());
        m.world.game.weather = Some(m.start.weather.now as usize);
    }
    if let Some(next) = event["next"].as_f64() {
        m.start.living.as_mut().unwrap().next = next;
    }
    if let Some(raining) = event["raining"].as_bool() {
        m.start.living.as_mut().unwrap().raining = raining;
    }
}

#[test]
fn original_live_showers_run_through_actual_match_weather_phase() {
    let f = fixture();
    let traces = f["showers"].as_array().unwrap();
    assert_eq!(traces.len(), 4);
    let mut total = 0;
    for (spec, trace) in f["input"]["showers"].as_array().unwrap().iter().zip(traces) {
        assert_eq!(trace["map"], authored_map(&f["input"], ".", 20, 20));
        let mut m = make(
            &f["input"],
            &trace["map"],
            &trace["setup"],
            &json!({"weather":spec["weather"]}),
        );
        // Original createGame actors and footprints remain on both sides. Their
        // native seeded layout is not claimed to equal Source's global RNG layout.
        assert!(!m.world.game.units.is_empty());
        assert_eq!(
            m.start.weather.now.name(),
            spec["weather"].as_str().unwrap()
        );
        restore_environment(&mut m, &trace["setup"]);
        let count = spec["ticks"].as_u64().unwrap();
        let frames = trace["frames"].as_array().unwrap();
        assert_eq!(frames.len(), count as usize + 1);
        for n in 0..=count {
            if n > 0 {
                tick(&mut m, spec["name"].as_str().unwrap());
            }
            compare(
                &environment(&m),
                &frames[n as usize],
                &format!("{}:{n}", spec["name"]),
            );
            for event in spec["events"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|event| event["tick"].as_u64() == Some(n))
            {
                apply_event(&mut m, event);
            }
        }
        let wx = m.start.living.as_ref().unwrap();
        match spec["name"].as_str().unwrap() {
            "mud-held" => assert_eq!(
                m.world.game.wet,
                Some(1.0),
                "Mud stays soaked after a shower"
            ),
            "rain-to-mud" => assert!(
                !wx.raining && m.world.game.rain == 0.0 && m.world.game.wet == Some(1.0),
                "Rain turns to mud"
            ),
            "snow-no-rain" => assert!(
                !wx.raining && m.world.game.rain == 0.0,
                "no showers in snow"
            ),
            "clear-shower" => assert!(wx.raining && m.world.game.rain > 0.0, "Clear can shower"),
            _ => panic!("unexpected weather scene"),
        }
        total += count;
    }
    assert_eq!(total, 2000);
}

fn snapshot_weather(m: &Match) -> Vec<Value> {
    let cache = ww2_core::recipient_snapshot::SnapshotCache::new(m).unwrap();
    (0..m.start.players.len())
        .map(|slot| {
            let packet = cache.recipient(slot).unwrap();
            // The existing partial snapshot explicitly reports its missing families.
            assert!(!packet.missing.is_empty());
            packet.fields["weather"].clone()
        })
        .collect()
}

#[test]
fn warning_boundaries_and_single_change_use_actual_public_snapshots() {
    let f = fixture();
    let trace = &f["announcement"];
    let spec = &f["input"]["announcement"];
    assert_eq!(trace["map"], authored_map(&f["input"], ".", 20, 20));
    let mut initially_clear = make(&f["input"], &trace["map"], &trace["setup"], &json!({}));
    clear(&mut initially_clear);
    assert_eq!(
        snapshot_weather(&initially_clear),
        vec![json!(["clear"]); 2]
    );
    assert_eq!(
        trace["setup"]["environment"]["raw"]["weather"],
        json!({"now":"fog","next":"clear","at":600})
    );
    let mut m = scene(&f["input"], trace, None);
    let frames = trace["frames"].as_array().unwrap();
    assert_eq!(frames.len(), 621);
    assert_eq!(trace["clearRow"], json!(["clear"]));
    let mut transitions = 0;
    let mut previous = m.start.weather.now;
    for n in 0..=spec["ticks"].as_u64().unwrap() {
        if n > 0 {
            tick(&mut m, "announcement");
        }
        if previous != m.start.weather.now {
            transitions += 1;
            previous = m.start.weather.now;
        }
        compare(
            &environment(&m),
            &frames[n as usize]["environment"],
            &format!("announcement:{n}"),
        );
        let rows = snapshot_weather(&m);
        assert_eq!(json!(rows), frames[n as usize]["rows"]);
        assert_eq!(rows[0], rows[1], "the warning is public");
        let expected = if n < 400 {
            json!(["fog"])
        } else if n < 600 {
            json!(["fog", "clear", (600 - n).div_ceil(20)])
        } else {
            json!(["clear"])
        };
        assert_eq!(rows[0], expected, "warning tick {n}");
    }
    assert_eq!(transitions, 1);
    assert_eq!(m.start.weather, WeatherPlan::fixed(WeatherKind::Clear));
}

#[test]
fn original_sight_distances_use_actual_vision_and_recipient_rows() {
    let f = fixture();
    let spec = &f["input"]["sight"];
    let traces = f["sights"].as_array().unwrap();
    assert_eq!(traces.len(), 4);
    let mut counts = Vec::new();
    let mut total = 0;
    for (index, trace) in traces.iter().enumerate() {
        assert_eq!(trace["map"], authored_map(&f["input"], ".", 20, 20));
        let actors = trace["setup"]["actors"].as_array().unwrap();
        assert_eq!(actors.len(), 5);
        for (i, actor) in actors.iter().enumerate() {
            assert_eq!(actor["type"], "rifle");
            assert_eq!(actor["owner"], if i == 0 { 0 } else { 1 });
            assert_eq!(
                actor["x"].as_f64(),
                Some(
                    4.0 + if i == 0 {
                        0.0
                    } else {
                        number(&spec["distances"][i - 1])
                    }
                )
            );
            assert_eq!(actor["z"].as_f64(), Some(20.0));
        }
        let mut m = scene(&f["input"], trace, None);
        assert_eq!(m.world.game.units.len(), 5);
        // Keep the original firing and auto-retreat conditions. No hold-fire
        // override makes these five actors into an easier synthetic sight scene.
        assert!(
            m.commands
                .units
                .values()
                .all(|c| !c.hold_fire && !c.auto_retreat)
        );
        compare(&environment(&m), &trace["frames"][0], "sight:0");
        for n in 1..=spec["ticks"].as_u64().unwrap() {
            tick(&mut m, trace["name"].as_str().unwrap());
            compare(
                &environment(&m),
                &trace["frames"][n as usize],
                "sight environment",
            );
            total += 1;
        }
        let cache = ww2_core::recipient_snapshot::SnapshotCache::new(&m).unwrap();
        let packet = cache.recipient(0).unwrap();
        let rows = packet.fields["units"].as_array().unwrap();
        let visible: Vec<_> = trace["foes"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|id| {
                let id = id.as_u64().unwrap() as u32;
                rows.iter()
                    .any(|row| row[0].as_u64() == Some(id as u64))
                    .then(|| ww2_core::math::round(m.world.game.units[&id].x - 4.0) as i64)
            })
            .collect();
        assert_eq!(json!(visible), spec["expected"][index]);
        assert_eq!(json!(visible), trace["visible"]);
        assert_eq!(rows.len(), trace["rows"].as_u64().unwrap() as usize);
        counts.push(rows.len());
    }
    assert!(
        counts[1] < counts[2] && counts[2] < counts[0],
        "shorter sight sends fewer actual snapshot rows"
    );
    assert_eq!(total, 40);
}

fn plane_frame(m: &Match, id: u32, probe: Point) -> Value {
    use crate::shared::aviation::state_name;
    let u = &m.world.game.units[&id];
    let a = &m.aviation.planes[&id];
    json!({"environment":environment(m),"id":id,"x":u.x,"z":u.z,"rot":u.rot,
        "air":{"state":state_name(u.air.unwrap()),"fuel":a.fuel,"ammo":a.ammo,"timer":a.timer,"ang":a.orbit,
            "mission":a.mission.map(|mission| json!({"kind":mission.kind.name(),"x":mission.at.x,"z":mission.at.z,"id":mission.target.unwrap_or(0)}))},
        "sees":ww2_core::vision::team_sees(&m.world.game,&m.rules,&m.grid,0,probe)})
}

#[test]
fn flying_planes_travel_and_see_through_all_five_weathers() {
    let f = fixture();
    let spec = &f["input"]["planes"];
    let traces = f["planes"].as_array().unwrap();
    assert_eq!(traces.len(), 5);
    let mut baseline: Option<(u64, Vec<bool>)> = None;
    let mut total = 0;
    for trace in traces {
        assert_eq!(trace["map"], authored_map(&f["input"], ".", 80, 80));
        let actor = &trace["setup"]["actors"][0];
        assert_eq!(actor["type"], "fighter");
        assert_eq!(actor["air"]["state"], "out");
        assert_eq!(actor["air"]["mission"], spec["mission"]);
        assert_eq!(actor["x"], spec["actor"]["x"]);
        assert_eq!(actor["z"], spec["actor"]["z"]);
        let mut m = scene(&f["input"], trace, None);
        let id = trace["setup"]["actors"][0]["id"].as_u64().unwrap() as u32;
        let count = spec["ticks"].as_u64().unwrap();
        let mut seen = Vec::new();
        assert_eq!(trace["frames"].as_array().unwrap().len(), 41);
        for n in 0..=count {
            if n > 0 {
                tick(&mut m, "flying plane");
                total += 1;
            }
            let actual = plane_frame(&m, id, point(&spec["sightProbe"]));
            compare(&actual, &trace["frames"][n as usize], "plane actual tick");
            seen.push(actual["sees"].as_bool().unwrap());
        }
        let distance = m.world.game.units[&id].x - number(&spec["actor"]["x"]);
        assert!(distance > 0.0, "actual aircraft moved");
        assert_eq!(distance.to_bits(), number(&trace["travel"]).to_bits());
        if let Some((bits, sight)) = &baseline {
            assert_eq!(*bits, distance.to_bits());
            assert_eq!(*sight, seen);
        } else {
            baseline = Some((distance.to_bits(), seen));
        }
        // Air rows remain refused by the current public snapshot provider. This
        // test uses actual aircraft and point sight, and makes no air-row claim.
        assert!(
            matches!(ww2_core::recipient_snapshot::SnapshotCache::new(&m),
            Err(ww2_core::recipient_snapshot::SnapshotError::Unsupported {
                unit, capability: ww2_core::recipient_snapshot::SnapshotCapability::Air
            }) if unit == id)
        );
    }
    assert_eq!(total, 200);
}

#[test]
fn actual_match_constructors_consume_all_authored_weather_plans() {
    let f = fixture();
    let traces = f["plans"].as_array().unwrap();
    assert_eq!(traces.len(), 313);
    let mut kinds = std::collections::BTreeSet::new();
    let mut changed = 0;
    for (index, (spec, trace)) in f["input"]["plans"]
        .as_array()
        .unwrap()
        .iter()
        .zip(traces)
        .enumerate()
    {
        assert_eq!(spec["name"], trace["name"]);
        let mut map = authored_map(&f["input"], ".", 20, 20);
        for (key, value) in spec["meta"].as_object().unwrap() {
            map[key] = value.clone();
        }
        assert_eq!(trace["map"], map);
        assert_eq!(
            trace["opts"],
            json!({"weather":spec["choice"],"weatherSeed":spec["seed"],"mapKey":spec["key"]})
        );
        let m = make(&f["input"], &trace["map"], &trace["setup"], &trace["opts"]);
        assert_eq!(m.start.weather_seed, spec["seed"].as_u64().unwrap() as u32);
        let living = m.start.living.as_ref().unwrap();
        assert_eq!(living.raining, m.start.weather.now == WeatherKind::Rain);
        assert_eq!(m.world.game.rain, if living.raining { 1.0 } else { 0.0 });
        assert_eq!(
            m.world.game.wet,
            Some(
                if matches!(m.start.weather.now, WeatherKind::Rain | WeatherKind::Mud) {
                    1.0
                } else {
                    0.0
                }
            )
        );
        let plan = json!({"now":m.start.weather.now.name(),"next":m.start.weather.next.map(WeatherKind::name),"at":m.start.weather.at});
        assert_eq!(plan, trace["plan"], "{}: actual constructor", spec["name"]);
        if index < 50 {
            let repeat = make(&f["input"], &trace["map"], &trace["setup"], &trace["opts"]);
            assert_eq!(
                repeat.start.weather, m.start.weather,
                "same seed gives same actual constructor plan"
            );
            assert_eq!(trace["repeatPlan"], plan);
        } else {
            assert!(trace["repeatPlan"].is_null());
        }
        match spec["name"].as_str().unwrap() {
            "host-snow" | "ardennes" | "mood-snow" => {
                assert_eq!(m.start.weather, WeatherPlan::fixed(WeatherKind::Snow))
            }
            "pegasus" | "mood-dawn" | "key-dawn" => assert_eq!(
                m.start.weather,
                WeatherPlan {
                    now: WeatherKind::Fog,
                    next: Some(WeatherKind::Clear),
                    at: 4800
                }
            ),
            "explicit-rain" => assert_eq!(m.start.weather, WeatherPlan::fixed(WeatherKind::Rain)),
            "plain" | "unknown-choice" | "unknown-map-weather" | "host-clear-over-snow" => {
                assert_eq!(m.start.weather, WeatherPlan::fixed(WeatherKind::Clear))
            }
            _ => (),
        }
        if index < 300 {
            kinds.insert(m.start.weather.now.name());
            if let Some(next) = m.start.weather.next {
                changed += 1;
                assert!(
                    (m.start.weather.now == WeatherKind::Fog
                        && next == WeatherKind::Clear
                        && (4200..=5400).contains(&m.start.weather.at))
                        || (m.start.weather.now == WeatherKind::Rain
                            && next == WeatherKind::Mud
                            && (6000..=8400).contains(&m.start.weather.at))
                );
            }
        }
    }
    assert_eq!(
        kinds,
        ["clear", "fog", "rain", "mud", "snow"]
            .into_iter()
            .collect()
    );
    assert!(changed > 0 && changed < 300);
    assert_eq!(traces[300]["plan"], traces[301]["plan"]);
    let draws = &f["drawCounts"];
    assert_eq!(draws["base"], draws["map"]);
    assert_eq!(draws["base"], draws["fog"]);
    assert_eq!(
        draws["random"].as_u64().unwrap(),
        draws["base"].as_u64().unwrap() + 1
    );
    // These are Source draw observations. Native purpose streams intentionally
    // differ, so this does not claim a shared global RNG or whole-match replay.
}
