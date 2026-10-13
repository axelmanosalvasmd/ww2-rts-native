use super::*;
use crate::{
    economy::Faction,
    economy_state::ArmySize,
    environment::WreckMetadata,
    game::Wreck,
    map::AuthoritativeMap,
    match_commands::UnitControl,
    match_runtime::{MatchOptions, PlayerConfig},
    match_world::ResourceNode,
    match_world_rails::WorldStation,
    random::MatchRandom,
    unit_spawn,
    weather::WeatherChoice,
    world::{CellChange, Recipient},
};
use std::collections::BTreeMap;
fn index(v: &Value) -> usize {
    v.as_u64().unwrap() as usize
}
fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../tests/fixtures/world-recipient-metadata.json"
    ))
    .unwrap()
}
fn load(case: &Value) -> Match {
    let starts: Value = serde_json::from_str(include_str!(
        "../../../simtest/tests/fixtures/world-start.json"
    ))
    .unwrap();
    let mut c = starts["cases"][index(&case["startIndex"])].clone();
    if let Some(teams) = case["teams"].as_array() {
        for (p, team) in c["players"].as_array_mut().unwrap().iter_mut().zip(teams) {
            p["team"] = team.clone();
        }
    }
    let mut map = c["map"].clone();
    if let Some(rails) = case.get("railsOverride") {
        if rails.as_str() == Some("absent") {
            map["world"].as_object_mut().unwrap().remove("rails");
        } else {
            map["world"]["rails"] = rails.clone();
        }
    }
    if let Some(runs) = map["world"]["regionMapRuns"].as_array() {
        let cells: Vec<_> = runs
            .iter()
            .flat_map(|r| std::iter::repeat_n(r[0].clone(), index(&r[1])))
            .collect();
        map["world"]
            .as_object_mut()
            .unwrap()
            .remove("regionMapRuns");
        map["world"]["regionMap"] = json!(cells);
    }
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&map).unwrap()).unwrap();
    let options = MatchOptions {
        seed: c["seed"].as_u64().unwrap(),
        mode: Mode::World,
        players: c["players"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| PlayerConfig {
                name: p["name"].as_str().unwrap().into(),
                team: p["team"].as_i64().unwrap() as i32,
                faction: Faction::try_from(index(&p["faction"]) as u8).unwrap(),
            })
            .collect(),
        shuffle: c["settings"]["shuffle"].as_bool().unwrap(),
        army: match c["settings"]["army"].as_str().unwrap() {
            "standard" => ArmySize::Standard,
            "large" => ArmySize::Large,
            "massive" => ArmySize::Massive,
            "endless" => ArmySize::Endless,
            _ => unreachable!(),
        },
        weather: WeatherChoice::from_reference(c["settings"]["weather"].as_str()),
        weather_seed: c["weatherSeed"].as_u64().map(|v| v as u32),
        living_weather: c["settings"]["living"].as_bool().unwrap(),
        map_key: None,
        tech: c["settings"]["tech"].as_bool().unwrap(),
        supply: true,
        logistics: false,
        sandbox: false,
    };
    Match::new_world(&map, options.clone(), MatchRandom::new(options.seed)).unwrap()
}
fn apply_fields(u: &mut crate::game::Unit, v: &Value) {
    if let Some(n) = v["hp"].as_f64() {
        u.hp = n;
    }
    if let Some(n) = v["built"].as_f64() {
        u.built = Some(n);
    }
}
fn digest(value: &Value) -> String {
    fn feed(h: &mut u64, b: &[u8]) {
        for &v in b {
            *h = (*h ^ u64::from(v)).wrapping_mul(0x100000001b3);
        }
    }
    fn visit(h: &mut u64, v: &Value) {
        match v {
            Value::Null => feed(h, b"n"),
            Value::Bool(v) => feed(h, if *v { b"t" } else { b"f" }),
            Value::Number(v) => {
                feed(h, b"d");
                let n = v.as_f64().unwrap();
                feed(h, &if n == 0.0 { 0.0_f64 } else { n }.to_le_bytes());
            }
            Value::String(v) => {
                feed(h, b"s");
                feed(h, v.as_bytes());
                feed(h, &[0]);
            }
            Value::Array(v) => {
                feed(h, b"a");
                for child in v {
                    visit(h, child);
                }
                feed(h, b"]");
            }
            Value::Object(v) => {
                feed(h, b"o");
                let mut keys: Vec<_> = v.keys().collect();
                keys.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
                for key in keys {
                    visit(h, &json!(key));
                    visit(h, &v[key]);
                }
                feed(h, b"}");
            }
        }
    }
    let mut h = 0xcbf29ce484222325;
    visit(&mut h, value);
    format!("{h:016x}")
}
fn equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => a.as_f64() == b.as_f64(),
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| equal(a, b))
        }
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len() && a.iter().all(|(k, a)| b.get(k).is_some_and(|b| equal(a, b)))
        }
        _ => a == b,
    }
}
#[test]
fn original_world_metadata_readers_preserve_privacy_and_optional_fields() {
    let fixture = fixture();
    assert_eq!(
        fixture["reference"],
        "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
    );
    assert_eq!(fixture["arch"], "x64");
    assert_eq!(fixture["node"], "v24.21.0");
    let mut steps = 0;
    let mut captures = 0;
    for case in fixture["cases"].as_array().unwrap() {
        let mut m = load(case);
        let mut ids = BTreeMap::new();
        for row in case["rows"].as_array().unwrap() {
            let a = &row["action"];
            let mut result = Value::Null;
            match a["op"].as_str().unwrap() {
                "clear" => {
                    m.world.game.units.clear();
                    m.grid.rebuild(&m.world.game.units);
                }
                "spawn" => {
                    let id = index(&a["id"]) as u32;
                    assert_eq!(id, m.next_unit_id);
                    m.next_unit_id += 1;
                    let kind = m.rules.kind(a["kind"].as_str().unwrap_or("rifle")).unwrap();
                    let mut u = unit_spawn::initial_unit(
                        &m.rules,
                        Catalog::compiled().unwrap(),
                        kind,
                        id,
                        a["owner"].as_u64().unwrap_or(0) as usize,
                        Point {
                            x: a["x"].as_f64().unwrap(),
                            z: a["z"].as_f64().unwrap(),
                        },
                    )
                    .unwrap();
                    apply_fields(&mut u, &a["fields"]);
                    m.commands
                        .insert(id, UnitControl::new(&u, &m.rules).unwrap());
                    m.combat.initialize(id);
                    m.firing.initialize(id);
                    m.abilities.initialize(id);
                    m.recovery.initialize(id);
                    m.world.game.units.insert(id, u);
                    m.grid.rebuild(&m.world.game.units);
                    ids.insert(a["alias"].as_str().unwrap().to_owned(), id);
                }
                "move" => {
                    let u = m
                        .world
                        .game
                        .units
                        .get_mut(&ids[a["alias"].as_str().unwrap()])
                        .unwrap();
                    u.x = a["x"].as_f64().unwrap();
                    u.z = a["z"].as_f64().unwrap();
                    apply_fields(u, &a["fields"]);
                    m.grid.rebuild(&m.world.game.units);
                }
                "remove" => {
                    m.world
                        .game
                        .units
                        .remove(&ids[a["alias"].as_str().unwrap()]);
                    m.grid.rebuild(&m.world.game.units);
                }
                "vision" => {
                    m.world.game.tick = a["tick"].as_i64().unwrap();
                    m.refresh_world_visibility().unwrap();
                }
                "reveal" => m.world.game.reveal = a["value"].as_bool().unwrap(),
                "region" => {
                    let r = &mut m.world_runtime.as_mut().unwrap().regions[index(&a["index"])];
                    let f = &a["fields"];
                    if let Some(n) = f["team"].as_i64() {
                        r.team = n as i32;
                    }
                    if let Some(n) = f["progress"].as_f64() {
                        r.progress = n;
                    }
                    if let Some(n) = f["capper"].as_i64() {
                        r.capper = (n >= 0).then_some(n as usize);
                    }
                    if let Some(n) = f["contested"].as_bool() {
                        r.contested = n;
                    }
                }
                "homeTie" => {
                    m.start.players[index(&a["slot"])].spawn = Point {
                        x: a["spawn"]["x"].as_f64().unwrap(),
                        z: a["spawn"]["z"].as_f64().unwrap(),
                    };
                }
                "player" => {
                    let p = &mut m.start.economy.players[index(&a["slot"])];
                    if let Some(v) = a["fields"]["out"].as_bool() {
                        p.out = v;
                    }
                    if let Some(v) = a["fields"]["away"].as_bool() {
                        p.away = v;
                    }
                }
                "mutate" => {
                    let c = &a["change"];
                    m.world
                        .mutate_cell(
                            index(&a["cell"]),
                            CellChange {
                                ground: c["ground"].as_str().map(|s| s.as_bytes()[0]),
                                object: c["object"].as_str().map(|s| s.as_bytes()[0]),
                                height: c["height"].as_i64().map(|v| v as i8),
                                material: c["material"].as_str().map(str::to_owned),
                                mine: c["mine"].as_bool(),
                            },
                        )
                        .unwrap();
                }
                "seen" => {
                    m.world
                        .mine_seen
                        .insert(index(&a["cell"]), index(&a["bits"]) as u32);
                }
                "forget" => {
                    m.world.mine_seen.remove(&index(&a["cell"]));
                }
                "log" => {
                    m.world.log_cell(index(&a["cell"]), true).unwrap();
                }
                "nodes" => {
                    m.world_runtime.as_mut().unwrap().nodes = a["rows"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|n| ResourceNode {
                            cell: index(&n["c"]),
                            at: Point {
                                x: n["x"].as_f64().unwrap(),
                                z: n["z"].as_f64().unwrap(),
                            },
                            depot: None,
                            rate: n["rate"].as_f64().unwrap(),
                            fuel: n["fuel"].as_bool().unwrap(),
                        })
                        .collect()
                }
                "stations" => {
                    m.world_runtime.as_mut().unwrap().rails.stations = a["rows"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|s| WorldStation {
                            cell: index(&s["c"]),
                            region: index(&s["region"]),
                            x: s["x"].as_f64().unwrap(),
                            z: s["z"].as_f64().unwrap(),
                            next: BTreeMap::new(),
                        })
                        .collect()
                }
                "wreck" => {
                    let id = index(&a["id"]) as u32;
                    let w = Wreck {
                        id: Some(id),
                        cell: None,
                        x: a["x"].as_f64().unwrap(),
                        z: a["z"].as_f64().unwrap(),
                        moving: false,
                    };
                    if let Some(old) = m.world.game.wrecks.iter_mut().find(|w| w.id == Some(id)) {
                        *old = w;
                    } else {
                        m.world.game.wrecks.push(w);
                    }
                    m.environment.wreck_metadata.insert(
                        id,
                        WreckMetadata {
                            kind: m.rules.kind(a["kind"].as_str().unwrap_or("tank")).unwrap(),
                            owner: a["owner"].as_u64().unwrap_or(1) as usize,
                            rotation: a["rot"].as_f64().unwrap_or(-0.26),
                            motion: None,
                        },
                    );
                }
                "removeWreck" => {
                    m.world
                        .game
                        .wrecks
                        .retain(|w| w.id != Some(index(&a["id"]) as u32));
                    m.environment.reconcile_wreck_metadata(&m.world);
                }
                "capture" => {
                    captures += 1;
                    result = m
                        .world_recipient_metadata(index(&a["slot"]), a["map"].as_bool().unwrap())
                        .unwrap()
                        .to_json();
                    if a["map"].as_bool() == Some(true) {
                        result["map"] = json!(digest(&result["map"]));
                    }
                }
                other => panic!("{other}"),
            }
            assert!(
                equal(&result, &row["result"]),
                "{} step{steps}: actual {result}, expected {}",
                case["name"],
                row["result"]
            );
            steps += 1;
        }
    }
    assert_eq!((steps, captures), (153, 34));
}
#[test]
fn late_invalid_projection_preserves_memory_fog_journal_pending_and_send_cursor() {
    let case = &fixture()["cases"][7];
    let mut m = load(case);
    m.world.register_recipient(Recipient::Player(0));
    m.world_recipient_metadata(0, false).unwrap();
    let team = m.world.game.players[0].team;
    // Establish a real independent send cursor, then stage actual journal work.
    m.world_terrain(0, false).unwrap();
    let cell = m
        .world_runtime
        .as_ref()
        .unwrap()
        .memory
        .team(team)
        .unwrap()
        .rows()[0]
        .observed
        .row
        .cell;
    m.world.set_cell(cell, b'T').unwrap();
    for _ in 0..65536 {
        m.world.touch(cell).unwrap();
    }
    m.world_runtime.as_mut().unwrap().nodes.push(ResourceNode {
        cell,
        at: Point {
            x: f64::MAX,
            z: 1.0,
        },
        depot: None,
        rate: 1.0,
        fuel: false,
    });
    let prior = m.world_runtime.as_ref().unwrap().memory.clone();
    let journal = m.world.journal().to_vec();
    let base = m.world.journal_base();
    let pending = m.world.pending(Recipient::Player(0)).cloned();
    let changes = m.world.new_cells().to_vec();
    assert!(
        m.world_recipient_metadata(0, true)
            .unwrap_err()
            .contains("rounded")
    );
    let memory = &m.world_runtime.as_ref().unwrap().memory;
    assert_eq!(memory.fog, prior.fog);
    for (team, expected) in &prior.teams {
        let actual = memory.team(*team).unwrap();
        assert_eq!(actual.rows(), expected.rows());
        assert_eq!(actual.version(), expected.version());
        assert_eq!(actual.regions(), expected.regions());
        assert_eq!(actual.remembered_wrecks(), expected.remembered_wrecks());
    }
    let mut actual = memory.clone();
    let mut expected = prior.clone();
    assert_eq!(
        actual.terrain_for(team, 0, false).unwrap(),
        expected.terrain_for(team, 0, false).unwrap()
    );
    assert_eq!(m.world.journal(), journal);
    assert_eq!(m.world.journal_base(), base);
    assert_eq!(m.world.pending(Recipient::Player(0)), pending.as_ref());
    assert_eq!(m.world.new_cells(), changes);
    m.world_runtime.as_mut().unwrap().nodes.pop();
    m.world.game.wrecks.push(Wreck {
        id: Some(9900),
        cell: None,
        x: m.start.players[0].spawn.x,
        z: m.start.players[0].spawn.z,
        moving: false,
    });
    m.world.game.reveal = true;
    assert!(
        m.world_recipient_metadata(0, false)
            .unwrap_err()
            .contains("missing observed")
    );
    assert_eq!(
        m.world_runtime
            .as_ref()
            .unwrap()
            .memory
            .team(team)
            .unwrap()
            .rows(),
        prior.team(team).unwrap().rows()
    );
    assert_eq!(m.world.journal(), journal);
    for mode in [Mode::Default, Mode::Conquest, Mode::Classic] {
        m.options.mode = mode;
        assert!(m.world_recipient_metadata(0, false).is_err());
    }
    m.options.mode = Mode::World;
    assert!(
        m.world_recipient_metadata(LOCAL_DEFENDER_OWNER, false)
            .is_err()
    );
}
