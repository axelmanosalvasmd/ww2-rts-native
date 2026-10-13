use super::*;
use crate::{
    catalog::Catalog,
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::{Player, Point},
    map::AuthoritativeMap,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    unit_spawn,
    weather::WeatherChoice,
    world::{Recipient, World},
};
use serde_json::{Value, json};
fn bits(n: f64) -> Value {
    json!(format!("{:016x}", n.to_bits()))
}
fn pt(p: Option<Point>) -> Value {
    p.map_or(Value::Null, |p| json!([bits(p.x), bits(p.z)]))
}
fn snapshot(m: &Match) -> Value {
    Value::Array(m.world.game.units.values().map(|u| {
 let c=&m.commands.units[&u.id];let f=&m.firing.units[&u.id];
 let mut row=json!({"id":u.id,"x":bits(u.x),"z":bits(u.z),"rot":bits(u.rot),"aim":bits(c.aim),"path":u.path.iter().map(|p|pt(Some(*p))).collect::<Vec<_>>(),"reverse":pt(u.reverse),"pace":bits(u.pace),"worldGoal":pt(u.world_goal),"amove":pt(u.amove),"attackId":c.attack_id,"targetId":f.target_id,"follow":c.follow,"holdFire":c.hold_fire,"holdPos":u.hold_pos,"auto":c.auto_cast,"autoRetreat":c.auto_retreat});
 let jobs=json!({"orders":c.orders.iter().map(|o|match o {Command::Move {orders,attack_move,face,..}=>json!({"t":if *attack_move {"amove"}else{"move"},"at":pt(orders.first().map(|o|o.at)),"face":face.map(bits)}),_=>panic!("unexpected waiting order")}).collect::<Vec<_>>(),"face":c.face.map(|f|json!({"angle":bits(f.angle),"at":pt(Some(f.at))})),"retreating":u.retreating,"stuck":bits(u.stuck),"repath":bits(u.repath),"drift":u.drift,"garrison":u.garrison.map_or(-1,|c|c as i64),"nade":pt(c.nade),"enter":c.enter.map_or(-1,|v|v as i64),"fireAt":c.fire_at.map_or(-1,|v|v as i64),"board":u.board,"build":u.build,"dig":u.dig,"entrench":u.entrench});
 let routing=json!({"traffic":u.traffic.is_some(),"trafficWait":bits(u.traffic_wait),"trafficEntered":u.traffic_entered,"trail":u.traffic_trail.len(),"outcome":c.outcome.map(|o|match o {MoveOutcome::Interrupted=>"interrupted",MoveOutcome::Moving=>"moving",MoveOutcome::Unreachable=>"unreachable",MoveOutcome::Arrived=>"completed"}),"outcomeTick":c.outcome_tick,"routeEnd":pt(c.route_end),"supplyHold":c.supply_hold,"supplyEscape":c.supply_escape,"forced":m.logistics.get(u.id).map(|r|json!({"forced":r.forced,"stranded":r.stranded}))});
 row.as_object_mut().unwrap().extend(jobs.as_object().unwrap().clone());
 row.as_object_mut().unwrap().extend(routing.as_object().unwrap().clone());
 row
 }).collect())
}
fn initial(c: &Value) -> Match {
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&c["map"]).unwrap()).unwrap();
    let mut m = Match::new(
        &map,
        MatchOptions {
            seed: 701,
            mode: Mode::Conquest,
            players: vec![
                PlayerConfig {
                    name: "A".into(),
                    team: 0,
                    faction: Faction::try_from(0).unwrap(),
                },
                PlayerConfig {
                    name: "B".into(),
                    team: 1,
                    faction: Faction::try_from(1).unwrap(),
                },
            ],
            shuffle: false,
            army: ArmySize::Standard,
            weather: WeatherChoice::Map,
            weather_seed: None,
            living_weather: false,
            map_key: None,
            tech: false,
            supply: false,
            logistics: false,
            sandbox: false,
        },
    )
    .unwrap();
    // Isolate command state on authored terrain before runtime bases are stamped.
    m.world = World::from_map(&map).unwrap();
    m.building_cells.clear();
    m.commands = MatchCommands::new(4096);
    m.world.game.players = vec![
        Player {
            slot: 0,
            team: 0,
            visible: Vec::new(),
        },
        Player {
            slot: 1,
            team: 1,
            visible: Vec::new(),
        },
    ];
    let catalog = Catalog::compiled().unwrap();
    for (i, row) in c["initial"].as_array().unwrap().iter().enumerate() {
        let id = i as u32 + 1;
        let owner = row["owner"].as_u64().unwrap_or(0) as usize;
        let u = unit_spawn::initial_unit(
            &m.rules,
            catalog,
            m.rules.kind(row["kind"].as_str().unwrap()).unwrap(),
            id,
            owner,
            Point {
                x: row["x"].as_f64().unwrap(),
                z: row["z"].as_f64().unwrap(),
            },
        )
        .unwrap();
        let control = UnitControl::new(&u, &m.rules).unwrap();
        m.commands.insert(id, control);
        m.firing.initialize(id);
        m.combat.initialize(id);
        m.world.game.units.insert(id, u);
        if row["fields"].is_object() {
            set(&mut m, id, &row["fields"]);
        }
    }
    for owner in 0..2 {
        m.world.register_recipient(Recipient::Player(owner));
        m.world.game.players[owner].visible = m
            .world
            .game
            .units
            .values()
            .filter(|u| u.owner != owner)
            .map(|u| u.id)
            .collect();
    }
    m.world.game.tick = 1;
    m.grid.rebuild(&m.world.game.units);
    m.initialize_recipient_routing().unwrap();
    m
}
fn point(v: &Value) -> Option<Point> {
    v.as_object().map(|_| Point {
        x: v["x"].as_f64().unwrap(),
        z: v["z"].as_f64().unwrap(),
    })
}
fn set(m: &mut Match, id: u32, fields: &Value) {
    let u = m.world.game.units.get_mut(&id).unwrap();
    let c = m.commands.units.get_mut(&id).unwrap();
    for (key, value) in fields.as_object().unwrap() {
        match key.as_str() {
            "retreating" => u.retreating = value.as_bool().unwrap(),
            "holdPos" => u.hold_pos = value.as_bool().unwrap(),
            "x" => u.x = value.as_f64().unwrap(),
            "z" => u.z = value.as_f64().unwrap(),
            "path" => {
                u.path = value
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| point(v).unwrap())
                    .collect()
            }
            "worldGoal" => u.world_goal = point(value),
            "amove" => u.amove = point(value),
            "moveSpeed" => u.move_speed = value.as_f64().unwrap(),
            "repath" => u.repath = value.as_f64().unwrap(),
            "targetId" => {
                m.firing.units.get_mut(&id).unwrap().target_id = value.as_u64().unwrap() as u32
            }
            "hitAt" => m.combat.get_mut(id).unwrap().hit_at = value.as_f64().unwrap(),
            "attackId" => {
                c.attack_id = value.as_u64().unwrap() as u32;
                u.attack = c.attack_id != 0;
            }
            "garrison" => u.garrison = usize::try_from(value.as_i64().unwrap()).ok(),
            "nade" => c.nade = point(value),
            "dig" => u.dig = !value.is_null(),
            "enter" => c.enter = usize::try_from(value.as_i64().unwrap()).ok(),
            "fireAt" => c.fire_at = usize::try_from(value.as_i64().unwrap()).ok(),
            "board" => u.board = value.as_u64().unwrap() as u32,
            "build" => u.build = value.as_u64().unwrap() != 0,
            "entrench" => u.entrench = !value.is_null(),
            "follow" => c.follow = value.as_u64().unwrap() as u32,
            "drift" => {
                u.drift = value.as_str().is_some() || value.as_bool() == Some(true);
                c.rally_drift = value.as_str() == Some("rally");
            }
            "operative" => c.operative = !value.is_null(),
            "riding" => u.riding = value.as_u64().unwrap() != 0,
            "logistics" => {
                m.logistics.insert(
                    id,
                    crate::logistics::Reserves {
                        ammo: 0.0,
                        ammo_max: 0.0,
                        ammo_price: 0.0,
                        ammo_credit: None,
                        provisions: 0.0,
                        fuel: None,
                        emergency: None,
                        warning: None,
                        forced: value["forced"].as_bool().unwrap(),
                        stranded: value["stranded"].as_bool().unwrap(),
                        exhausted: false,
                        supply_rate: None,
                        cut_for: None,
                    },
                );
            }
            _ => panic!("unhandled fixture change {key}"),
        }
    }
    m.grid.rebuild(&m.world.game.units);
}
#[test]
fn original_command_and_waiting_order_phases() {
    let fixture: Value =
        serde_json::from_slice(include_bytes!("../../tests/fixtures/match-commands.json")).unwrap();
    let mut count = 0;
    for case in fixture["cases"].as_array().unwrap() {
        let mut m = initial(case);
        for (index, step) in case["steps"].as_array().unwrap().iter().enumerate() {
            let a = &step["action"];
            m.world.game.tick = a["tick"].as_i64().unwrap_or(m.world.game.tick);
            for row in a["set"].as_array().into_iter().flatten() {
                set(&mut m, row[0].as_u64().unwrap() as u32, &row[1]);
            }
            for row in a["cells"].as_array().into_iter().flatten() {
                m.world
                    .set_cell(
                        row[0].as_u64().unwrap() as usize,
                        row[1].as_str().unwrap().as_bytes()[0],
                    )
                    .unwrap();
            }
            for row in a["players"].as_array().into_iter().flatten() {
                let p = &mut m.start.economy.players[row[0].as_u64().unwrap() as usize];
                if let Some(v) = row[1]["away"].as_bool() {
                    p.away = v;
                }
                if let Some(v) = row[1]["out"].as_bool() {
                    p.out = v;
                }
            }
            let mut denial = None;
            if !a["command"].is_null() {
                denial = match Command::from_json(&a["command"]) {
                    Ok(command) => m
                        .execute_command(
                            a["owner"].as_u64().unwrap_or(0) as usize,
                            &command,
                            a["winner"].as_bool().unwrap_or(false),
                        )
                        .unwrap(),
                    Err(d) => Some(d),
                };
            }
            assert_eq!(
                json!(denial.map(Denial::name)),
                step["denial"],
                "{} step{index} denial",
                case["name"]
            );
            if a["begin"].as_bool() == Some(true) {
                m.begin_path_tick(
                    a["budget"].as_u64().unwrap_or(4096) as usize,
                    a["retryLimit"].as_u64().unwrap_or(20) as u32,
                )
                .unwrap();
            }
            if let Some(id) = a["pre"].as_u64() {
                m.prepare_unit_commands(id as u32, a["winner"].as_bool().unwrap_or(false))
                    .unwrap();
            }
            if let Some(id) = a["amove"].as_u64() {
                m.apply_attack_move(id as u32).unwrap();
            }
            assert_eq!(
                snapshot(&m),
                step["expected"],
                "{} step{index}",
                case["name"]
            );
            count += 1;
        }
    }
    assert_eq!(count, 261);
}
#[test]
fn ordered_match_boundary_and_whole_command_rejection() {
    let fixture: Value =
        serde_json::from_slice(include_bytes!("../../tests/fixtures/match-commands.json")).unwrap();
    let mut left = initial(&fixture["cases"][0]);
    let mut right = initial(&fixture["cases"][0]);
    left.world.game.tick = 0;
    right.world.game.tick = 0;
    let first =
        Command::from_json(&json!({"t":"stance","ids":[1],"key":"holdPos","on":true})).unwrap();
    let second =
        Command::from_json(&json!({"t":"stance","ids":[1],"key":"holdPos","on":false})).unwrap();
    for (m, reverse) in [(&mut left, false), (&mut right, true)] {
        let mut orders = vec![
            (
                crate::orders::OrderKey {
                    tick: 1,
                    issuer: 0,
                    sequence: 1,
                },
                first.clone(),
            ),
            (
                crate::orders::OrderKey {
                    tick: 1,
                    issuer: 0,
                    sequence: 2,
                },
                second.clone(),
            ),
        ];
        if reverse {
            orders.reverse();
        }
        for (key, command) in orders {
            m.enqueue_command(key, command).unwrap();
        }
        let before = m.commands.pending_commands();
        assert!(m.apply_commands(2, false).is_err());
        assert_eq!(m.commands.pending_commands(), before);
        assert_eq!(m.apply_commands(1, false).unwrap().len(), 2);
    }
    assert_eq!(snapshot(&left), snapshot(&right));
    assert!(!left.world.game.units[&1].hold_pos);
    let before = snapshot(&left);
    let mut rows = vec![json!([1, 20, 20]); 51];
    rows.push(json!([1, "invalid", 20]));
    assert_eq!(
        Command::from_json(&json!({"t":"move","orders":rows})),
        Err(Denial::Blocked)
    );
    assert_eq!(snapshot(&left), before);
    left.world.game.units.get_mut(&1).unwrap().path_gen = u64::MAX;
    let exhausted_before = snapshot(&left);
    let command = Command::from_json(&json!({"t":"move","orders":[[1,30,30]]})).unwrap();
    assert!(left.execute_command(0, &command, false).is_err());
    assert_eq!(snapshot(&left), exhausted_before);
    assert_eq!(left.world.game.units[&1].path_gen, u64::MAX);
}

#[test]
fn detached_recovery_and_empty_immediate_replan_preserve_routes() {
    let fixture: Value =
        serde_json::from_slice(include_bytes!("../../tests/fixtures/match-commands.json")).unwrap();
    for case in fixture["movement"].as_array().unwrap() {
        let setup = json!({"map":case["map"],"initial":[{"kind":case["kind"],"x":9,"z":23}]});
        let mut m = initial(&setup);
        let u = m.world.game.units.get_mut(&1).unwrap();
        u.path = vec![Point { x: 55.0, z: 23.0 }];
        u.stuck = 1.0;
        u.repath = 0.0;
        u.hold_pos = true;
        let before = snapshot(&m);
        assert!(m.move_unit_with_command_routes(1, 1.0).is_err());
        assert_eq!(
            snapshot(&m),
            before,
            "missing path boundary must reject before movement"
        );
        let generation = m.world.game.units[&1].path_gen;
        m.begin_path_tick(4096, 20).unwrap();
        m.world.game.units.get_mut(&1).unwrap().path_gen = u64::MAX - 1;
        let exhausted = snapshot(&m);
        assert!(m.move_unit_with_command_routes(1, 1.0).is_err());
        assert_eq!(snapshot(&m), exhausted);
        assert_eq!(m.world.game.units[&1].path_gen, u64::MAX - 1);
        m.world.game.units.get_mut(&1).unwrap().path_gen = generation;
        m.move_unit_with_command_routes(1, 1.0).unwrap();
        let u = &m.world.game.units[&1];
        let c = &m.commands.units[&1];
        assert_eq!(
            json!({"path":u.path.iter().map(|p|pt(Some(*p))).collect::<Vec<_>>(),"repath":bits(u.repath),"routeEnd":pt(c.route_end),"replaced":u.path_gen!=generation}),
            case["expected"],
            "{} empty replan",
            case["kind"]
        );
    }
}

#[test]
fn immediate_cover_uses_the_recorded_queue_and_refuses_unrepresented_jobs() {
    let f: Value =
        serde_json::from_slice(include_bytes!("../../tests/fixtures/match-commands.json")).unwrap();
    let case = f["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "cover-run-to-trench")
        .unwrap();
    let mut m = initial(case);
    m.world.game.tick = 0;
    m.enqueue_command(
        crate::orders::OrderKey {
            tick: 1,
            issuer: 0,
            sequence: 1,
        },
        Command::Cover { ids: vec![1] },
    )
    .unwrap();
    let results = m.apply_commands(1, false).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].denial, None);
    assert!(!m.world.game.units[&1].path.is_empty());
    let mut m = initial(case);
    m.world.game.units.get_mut(&1).unwrap().dig = true;
    let before = snapshot(&m);
    assert_eq!(
        m.execute_command(0, &Command::Cover { ids: vec![1] }, false)
            .unwrap(),
        Some(Denial::Unsupported)
    );
    assert_eq!(snapshot(&m), before);
    assert_eq!(
        Command::from_json(&json!({"t":"cover","ids":[1],"queue":true})).unwrap(),
        Command::Unsupported
    );
}

#[test]
fn cover_refresh_preserves_moved_gameplay_bucket_order() {
    let f: Value =
        serde_json::from_slice(include_bytes!("../../tests/fixtures/match-commands.json")).unwrap();
    let mut case = f["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "cover-two-squads")
        .unwrap()
        .clone();
    case["initial"].as_array_mut().unwrap().extend([
        json!({"kind":"tank","x":27,"z":24}),
        json!({"kind":"tank","x":27,"z":26}),
    ]);
    let mut m = initial(&case);
    for id in [1, 3] {
        let low = m.world.game.units.get_mut(&id).unwrap();
        low.x = 49.0;
        m.grid.update(id, low);
        low.x = 27.0;
        m.grid.update(id, low);
    }
    let probe = |m: &Match| {
        (
            m.grid
                .unordered(&m.world.game.units, (27.0, 23.0), 4.0, |_, _| true),
            m.grid
                .heavy(&m.world.game.units, (27.0, 23.0), 4.0, |_, _| true),
        )
    };
    let before = probe(&m);
    assert_eq!(before, (vec![2, 4, 1, 3], vec![4, 3]));
    m.world.game.tick += 1;
    assert_eq!(
        m.execute_command(0, &Command::Cover { ids: vec![1, 2] }, false)
            .unwrap(),
        None
    );
    assert_eq!(probe(&m), before);
    m.observed_wrecks(0).unwrap();
    assert_eq!(probe(&m), before);
    m.recipient_sees(0, Point { x: 27.0, z: 23.0 }).unwrap();
    assert_eq!(probe(&m), before);
    m.world.game.units.get_mut(&2).unwrap().path_gen = u64::MAX;
    let state = snapshot(&m);
    let generation = m.world.game.units[&1].path_gen;
    m.world.game.tick += 1;
    assert!(
        m.execute_command(0, &Command::Cover { ids: vec![1, 2] }, false)
            .is_err()
    );
    assert_eq!(snapshot(&m), state);
    assert_eq!(m.world.game.units[&1].path_gen, generation);
    assert_eq!(m.world.game.units[&2].path_gen, u64::MAX);
    assert_eq!(probe(&m), before);
}
