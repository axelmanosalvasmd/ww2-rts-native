use super::*;
use crate::{
    catalog::Catalog,
    economy::{Faction, Mode},
    economy_state::ArmySize,
    environment_debris::{DebrisBody, DebrisKind, Vec3},
    game::Point,
    map::AuthoritativeMap,
    match_commands::UnitControl,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    match_world::ResourceNode,
    random::MatchRandom,
    unit_spawn,
    weather::WeatherChoice,
    world::{CellChange, Recipient},
};
fn fixture() -> Value {
    serde_json::from_str(include_str!("../../tests/fixtures/recipient-lists.json")).unwrap()
}
fn index(v: &Value) -> usize {
    v.as_u64().unwrap() as usize
}
fn options(c: &Value, world: bool) -> MatchOptions {
    MatchOptions {
        seed: c["seed"].as_u64().unwrap(),
        mode: if world { Mode::World } else { Mode::Conquest },
        players: c["players"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| PlayerConfig {
                name: p["name"].as_str().unwrap().into(),
                team: p["team"].as_i64().unwrap() as i32,
                faction: Faction::try_from(p["faction"].as_u64().unwrap() as u8).unwrap(),
            })
            .collect(),
        shuffle: c["settings"]["shuffle"].as_bool().unwrap(),
        army: match c["settings"]["army"].as_str().unwrap() {
            "standard" => ArmySize::Standard,
            "large" => ArmySize::Large,
            _ => unreachable!(),
        },
        weather: WeatherChoice::from_reference(c["settings"]["weather"].as_str()),
        weather_seed: c["weatherSeed"].as_u64().map(|v| v as u32),
        living_weather: c["settings"]["living"].as_bool().unwrap(),
        map_key: None,
        tech: c["settings"]["tech"].as_bool().unwrap(),
        supply: world,
        logistics: false,
        sandbox: false,
    }
}
fn load(case: &Value) -> Match {
    let world = case["world"].as_bool().unwrap();
    let starts: Value = serde_json::from_str(include_str!(
        "../../../simtest/tests/fixtures/world-start.json"
    ))
    .unwrap();
    let c = if world {
        &starts["cases"][index(&case["startIndex"])]
    } else {
        &case["start"]
    };
    let mut map = c["map"].clone();
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
    let o = options(c, world);
    let mut m = if world {
        Match::new_world(&map, o.clone(), MatchRandom::new(o.seed)).unwrap()
    } else {
        Match::new(&map, o).unwrap()
    };
    m.world.game.units.clear();
    m.grid.rebuild(&m.world.game.units);
    m.world.game.wrecks.clear();
    m.world.mine_seen.clear();
    m
}
fn eye(m: &mut Match, a: &Value) {
    let id = a["id"].as_u64().unwrap() as u32;
    let owner = a["owner"].as_u64().unwrap_or(0) as usize;
    let u = unit_spawn::initial_unit(
        &m.rules,
        Catalog::compiled().unwrap(),
        m.rules.kind("rifle").unwrap(),
        id,
        owner,
        Point {
            x: a["x"].as_f64().unwrap(),
            z: a["z"].as_f64().unwrap(),
        },
    )
    .unwrap();
    m.commands
        .insert(id, UnitControl::new(&u, &m.rules).unwrap());
    m.combat.initialize(id);
    m.firing.initialize(id);
    m.world.game.units.insert(id, u);
    m.grid.rebuild(&m.world.game.units);
}
fn wreck(m: &mut Match, a: &Value) {
    let id = a["id"].as_u64().unwrap() as u32;
    let x = a["x"].as_f64().unwrap();
    let z = a["z"].as_f64().unwrap();
    let moving = a["moving"].as_bool().unwrap_or(false);
    let hull = Wreck {
        id: Some(id),
        cell: None,
        x,
        z,
        moving,
    };
    if let Some(old) = m.world.game.wrecks.iter_mut().find(|w| w.id == Some(id)) {
        *old = hull
    } else {
        m.world.game.wrecks.push(hull)
    }
    m.environment.wreck_metadata.insert(
        id,
        WreckMetadata {
            kind: m.rules.kind(a["kind"].as_str().unwrap_or("tank")).unwrap(),
            owner: if a["owner"].as_i64() == Some(-1) {
                crate::game::LOCAL_DEFENDER_OWNER
            } else {
                a["owner"].as_u64().unwrap_or(1) as usize
            },
            rotation: a["rot"].as_f64().unwrap_or(-0.26),
            motion: moving.then(|| DebrisBody {
                id: format!("wreck:{id}"),
                kind: DebrisKind::Wreck,
                x,
                y: 0.08,
                z,
                vx: 0.13,
                vy: -0.02,
                vz: 0.27,
                dir: 0.35,
                material: "steel".into(),
                mass: 8500.,
                radius: 1.,
                origin: Vec3 { x, y: 0., z },
                gravity: 9.81,
                drag: 3.2,
                stamp: 1.25,
                born: 0.5,
                contacts: 0,
                settled: false,
                tilt: 0.007,
            }),
        },
    );
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
fn original_recipient_lists_retain_display_privacy_order_and_world_nodes() {
    let f = fixture();
    let mut count = 0;
    for case in f["cases"].as_array().unwrap() {
        let mut m = load(case);
        let world = case["world"].as_bool().unwrap();
        let mut ids = BTreeMap::new();
        for row in case["rows"].as_array().unwrap() {
            let a = &row["action"];
            let mut result = Value::Null;
            match a["op"].as_str().unwrap() {
                "eye" => {
                    eye(&mut m, a);
                    ids.insert(
                        a["alias"].as_str().unwrap(),
                        a["id"].as_u64().unwrap() as u32,
                    );
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
                    m.grid.rebuild(&m.world.game.units);
                }
                "vision" => {
                    m.world.game.tick = a["tick"].as_i64().unwrap();
                    if world {
                        m.refresh_world_visibility().unwrap()
                    } else {
                        crate::vision::update_vision(&mut m.world.game, &m.rules, &m.grid)
                    }
                }
                "wreck" => wreck(&mut m, a),
                "removeWreck" => {
                    let id = a["id"].as_u64().unwrap() as u32;
                    m.world.game.wrecks.retain(|w| w.id != Some(id));
                    m.environment.reconcile_wreck_metadata(&m.world);
                }
                "seen" => {
                    m.world
                        .mine_seen
                        .insert(index(&a["cell"]), a["bits"].as_u64().unwrap() as u32);
                }
                "forget" => {
                    m.world.mine_seen.remove(&index(&a["cell"]));
                }
                "mine" => {
                    m.world
                        .mutate_cell(
                            index(&a["cell"]),
                            CellChange {
                                mine: Some(a["value"].as_bool().unwrap()),
                                ..Default::default()
                            },
                        )
                        .unwrap();
                }
                "reveal" => m.world.game.reveal = a["value"].as_bool().unwrap(),
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
                "observe" => {
                    if world {
                        m.refresh_world_routing(index(&a["slot"])).unwrap()
                    } else {
                        m.refresh_recipient_routing(index(&a["slot"])).unwrap()
                    }
                }
                "read" => {
                    result = m
                        .recipient_snapshot_lists(index(&a["slot"]))
                        .unwrap()
                        .to_json()
                }
                op => panic!("unknown action {op}"),
            }
            assert!(
                equal(&result, &row["result"]),
                "{} action{count} {a}: native {result}, source {}",
                case["name"],
                row["result"]
            );
            count += 1;
        }
    }
    assert_eq!(count, 92);
}
fn ordinary_context(m: &Match) -> crate::match_routing::RecipientContext<'_> {
    crate::match_routing::RecipientContext {
        world: &m.world,
        rules: &m.rules,
        start: &m.start,
        options: &m.options,
        building_cells: &m.building_cells,
        environment: &m.environment,
    }
}
fn world_context(m: &Match) -> crate::match_world_memory::WorldRoutingContext<'_> {
    let runtime = m.world_runtime.as_ref().unwrap();
    crate::match_world_memory::WorldRoutingContext {
        world: &m.world,
        environment: &m.environment,
        rules: &m.rules,
        start: &m.start,
        rails: &runtime.rails,
        regions: &runtime.regions,
        known_buildings: &runtime.known_buildings,
        local_defender: &runtime.local_defender,
    }
}
fn observation_state(m: &Match) -> String {
    let rows = m
        .recipients
        .iter()
        .map(|r| {
            format!(
                "{:?}{:?}{}{:?}",
                r.remembered(),
                r.remembered_wrecks(),
                r.query().navigation().flags.as_ptr() as usize,
                r.query().navigation()
            )
        })
        .collect::<Vec<_>>();
    let world = m.world_runtime.as_ref().map(|w| {
        w.memory
            .teams
            .iter()
            .map(|(team, t)| {
                format!(
                    "{team}{:?}{:?}{:?}{}",
                    t.rows(),
                    t.remembered_wrecks(),
                    t.query().navigation(),
                    t.version()
                )
            })
            .collect::<Vec<_>>()
    });
    format!(
        "{:?}{:?}{:?}{:?}{:?}{:?}{:?}",
        m.world.journal(),
        m.world.journal_base(),
        m.world.pending(Recipient::Player(0)),
        rows,
        world,
        m.routing_fog,
        m.world_runtime.as_ref().map(|w| &w.memory.fog)
    )
}
#[test]
fn malformed_wrecks_and_changed_prepared_facts_do_not_publish_observation() {
    let f = fixture();
    for world in [false, true] {
        let mut m = load(&f["cases"][if world { 2 } else { 0 }]);
        let mut a = json!({"id":777,"alias":"eye","x":41.,"z":41.});
        eye(&mut m, &a);
        m.world.game.tick = 1;
        if world {
            m.refresh_world_visibility().unwrap();
            m.refresh_world_routing(0).unwrap()
        } else {
            crate::vision::update_vision(&mut m.world.game, &m.rules, &m.grid);
            m.refresh_recipient_routing(0).unwrap()
        }
        m.world
            .set_cell(if world { 10260 } else { 820 }, b'T')
            .unwrap();
        m.world.game.tick += 1;
        a = json!({"id":9001,"x":43.,"z":41.,"moving":true});
        wreck(&mut m, &a);
        m.environment.wreck_metadata.get_mut(&9001).unwrap().kind = usize::MAX;
        let before = observation_state(&m);
        assert!(
            if world {
                m.refresh_world_routing(0)
            } else {
                m.refresh_recipient_routing(0)
            }
            .is_err()
        );
        assert_eq!(before, observation_state(&m));
        assert!(m.recipient_snapshot_lists(0).is_err());
        assert_eq!(before, observation_state(&m));
        m.environment.wreck_metadata.get_mut(&9001).unwrap().kind = m.rules.kind("tank").unwrap();
        m.environment
            .wreck_metadata
            .get_mut(&9001)
            .unwrap()
            .rotation = f64::MAX;
        assert!(m.recipient_snapshot_lists(0).is_err());
        assert_eq!(before, observation_state(&m));
        m.environment
            .wreck_metadata
            .get_mut(&9001)
            .unwrap()
            .rotation = -0.26;
        m.environment
            .wreck_metadata
            .get_mut(&9001)
            .unwrap()
            .motion
            .as_mut()
            .unwrap()
            .x = f64::NAN;
        assert!(
            if world {
                m.refresh_world_routing(0)
            } else {
                m.refresh_recipient_routing(0)
            }
            .is_err()
        );
        assert_eq!(before, observation_state(&m));
    }
    let mut world = load(&f["cases"][2]);
    eye(&mut world, &json!({"id":777,"x":41.,"z":41.}));
    world.world.game.tick = 1;
    world.refresh_world_visibility().unwrap();
    wreck(
        &mut world,
        &json!({"id":9001,"x":43.,"z":41.,"moving":false}),
    );
    let mut memory = world.world_runtime.as_ref().unwrap().memory.clone();
    let preview = crate::match_world_memory::prepare_world_routing(
        world_context(&world),
        &mut memory,
        0,
        None,
    )
    .unwrap();
    let before = observation_state(&world);
    world
        .environment
        .wreck_metadata
        .get_mut(&9001)
        .unwrap()
        .rotation = 0.7;
    assert!(
        preview
            .prepare_publication(&mut world.world, &world.environment)
            .is_err()
    );
    assert_eq!(before, observation_state(&world));
    let mut m = load(&f["cases"][0]);
    eye(&mut m, &json!({"id":777,"alias":"eye","x":41.,"z":41.}));
    m.world.game.tick = 1;
    crate::vision::update_vision(&mut m.world.game, &m.rules, &m.grid);
    wreck(&mut m, &json!({"id":9001,"x":43.,"z":41.,"moving":false}));
    let mut state = m.recipients[0].clone();
    let mut fog = m.routing_fog.clone();
    let p = crate::match_routing::prepare_recipient_routing(
        ordinary_context(&m),
        &mut state,
        &mut fog,
        0,
        None,
    )
    .unwrap();
    let before = observation_state(&m);
    m.environment
        .wreck_metadata
        .get_mut(&9001)
        .unwrap()
        .rotation = 0.7;
    assert!(p.prepare_publication(&mut m.world, &m.environment).is_err());
    assert_eq!(before, observation_state(&m));
    m.environment.wreck_metadata.remove(&9001);
    assert!(m.recipient_snapshot_lists(0).is_err());
    assert_eq!(before, observation_state(&m));
}
