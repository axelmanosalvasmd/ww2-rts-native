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

#[path = "mov_wrapper_batch.rs"]
mod mov_wrapper_batch;
fn bits(n: f64) -> Value {
    json!(format!("{:016x}", n.to_bits()))
}
fn pt(p: Option<Point>) -> Value {
    p.map_or(Value::Null, |p| json!([bits(p.x), bits(p.z)]))
}
fn snapshot(m: &Match) -> Value {
    crate::unit_placement::tests::assert_current(&m.world.game, &m.grid);
    Value::Array(m.world.game.units.values().map(|u| {
 let c=&m.commands.units[&u.id];let f=&m.firing.units[&u.id];
 let mut row=json!({"id":u.id,"x":bits(u.x),"z":bits(u.z),"rot":bits(u.rot),"aim":bits(c.aim),"path":u.path.iter().map(|p|pt(Some(*p))).collect::<Vec<_>>(),"reverse":pt(u.reverse),"pace":bits(u.pace),"worldGoal":pt(u.world_goal),"amove":pt(u.amove),"attackId":c.attack_id,"targetId":f.target_id,"follow":c.follow,"holdFire":c.hold_fire,"holdPos":u.hold_pos,"auto":c.auto_cast,"autoRetreat":c.auto_retreat});
 let jobs=json!({"orders":c.orders.iter().map(|o|match o {Command::Cover {at,..}=>json!({"t":"cover","at":pt(*at),"face":null}),Command::Ability {at,..}=>json!({"t":"ability","at":pt(*at),"face":null}),Command::Move {orders,attack_move,face,..}=>json!({"t":if *attack_move {"amove"}else{"move"},"at":pt(orders.first().map(|o|o.at)),"face":face.map(bits)}),_=>panic!("unexpected waiting order")}).collect::<Vec<_>>(),"face":c.face.map(|f|json!({"angle":bits(f.angle),"at":pt(Some(f.at))})),"retreating":u.retreating,"stuck":bits(u.stuck),"repath":bits(u.repath),"drift":u.drift,"garrison":u.garrison.map_or(-1,|c|c as i64),"nade":pt(c.nade),"enter":c.enter.map_or(-1,|v|v as i64),"fireAt":c.fire_at.map_or(-1,|v|v as i64),"board":u.board,"build":u.build,"dig":u.dig,"entrench":u.entrench});
 let routing=json!({"traffic":u.traffic.is_some(),"trafficWait":bits(u.traffic_wait),"trafficEntered":u.traffic_entered,"trail":u.traffic_trail.len(),"outcome":c.outcome.map(|o|match o {MoveOutcome::Interrupted=>"interrupted",MoveOutcome::Moving=>"moving",MoveOutcome::Unreachable=>"unreachable",MoveOutcome::Arrived=>"completed"}),"outcomeTick":c.outcome_tick,"result":c.move_result.map(|r|{let(k,t)=r.parts();json!([k,t])}),"routeEnd":pt(c.route_end),"supplyHold":c.supply_hold,"supplyEscape":c.supply_escape,"forced":m.logistics.get(u.id).map(|r|json!({"forced":r.forced,"stranded":r.stranded}))});
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
        m.abilities.initialize(id);
        m.recovery.initialize(id);
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
    if c["steps"]
        .as_array()
        .is_some_and(|steps| steps.iter().any(|s| s["work"].is_object()))
    {
        for player in &mut m.start.economy.players {
            player.mp = 150.0;
        }
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
    if let Some(project) = fields.get("entrench") {
        if let Some(at) = project["jobs"]
            .as_array()
            .and_then(|v| v.first())
            .and_then(point)
        {
            let unit = &m.world.game.units[&id];
            let prepared = m
                .field_works
                .prepare_project(
                    &m.world,
                    &m.start.economy.players[unit.owner],
                    crate::field_works::ProjectRequest {
                        owner: unit.owner,
                        pattern: crate::field_works::Pattern::Line,
                        fort: crate::field_works::FortKind::Trench,
                        from: at,
                        to: at,
                        back: unit.at(),
                    },
                    |_| true,
                )
                .expect("canonical fixture project placement");
            let project = prepared.commit(&mut m.field_works).unwrap();
            m.field_works.join_project(project, &[id]).unwrap();
        } else {
            m.field_works.cancel(id);
        }
    }
    if let Some(dig) = fields.get("dig") {
        if let Some(at) = point(dig) {
            let owner = m.world.game.units[&id].owner;
            let mp = m.start.economy.players[owner].mp;
            let prepared = m
                .field_works
                .prepare_start(
                    &m.world,
                    &mut m.start.economy.players[owner],
                    crate::field_works::StartRequest {
                        control_eligible: true,
                        owner,
                        unit: id,
                        kind: crate::field_works::FortKind::Trench,
                        at,
                        direction: Some(0.0),
                    },
                    |_| true,
                )
                .expect("canonical fixture job placement");
            prepared.commit(&mut m.field_works);
            // The source fixture restores job state directly without a purchase.
            m.start.economy.players[owner].mp = mp;
        } else {
            m.field_works.cancel(id);
        }
    }
    let u = m.world.game.units.get_mut(&id).unwrap();
    let c = m.commands.units.get_mut(&id).unwrap();
    for (key, value) in fields.as_object().unwrap() {
        match key.as_str() {
            "orders" => {
                assert!(value.as_array().unwrap().is_empty());
                c.orders.clear();
            }
            "routeEnd" => c.route_end = point(value),
            "moveOutcome" => {
                c.outcome = value.as_str().map(|v| match v {
                    "interrupted" => MoveOutcome::Interrupted,
                    _ => panic!("fixture outcome"),
                })
            }
            "moveOutcomeTick" => c.outcome_tick = value.as_i64(),
            "garrisonEntry" => {} // Source setup provenance; Cover neither reads nor changes this coordinate.
            "face" => {
                c.face = value.as_object().map(|_| Facing {
                    angle: value["a"].as_f64().unwrap(),
                    at: point(value).unwrap(),
                })
            }
            "traffic" => {
                u.traffic = (!value.is_null() && value.as_bool() != Some(false)).then(|| {
                    crate::game::Traffic {
                        goal: u.at(),
                        legs: vec![],
                        passing: false,
                        by: 0,
                        until: 0.0,
                    }
                })
            }
            "trafficWait" => u.traffic_wait = value.as_f64().unwrap(),
            "stuck" => u.stuck = value.as_f64().unwrap(),
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
            "build" => {
                c.build_target = value
                    .as_u64()
                    .filter(|id| *id != 0)
                    .map(|id| u32::try_from(id).unwrap());
                u.build = c.build_target.is_some();
            }
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
fn original_command_and_waiting_order_phases_with_native_withdrawal_stances() {
    let mut fixture: Value =
        serde_json::from_slice(include_bytes!("../../tests/fixtures/match-commands.json")).unwrap();
    // LOG-13 admits withdrawal stances. Keep the recorded Source bytes intact
    // and replace only this deliberately changed control's expected effects.
    let forced = fixture["cases"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|case| case["name"] == "forced-and-permission")
        .unwrap();
    forced["steps"][0]["denial"] = Value::Null;
    for step in forced["steps"].as_array_mut().unwrap() {
        let actor = step["expected"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|actor| actor["id"] == 1)
            .unwrap();
        actor["holdPos"] = json!(true);
        actor["follow"] = json!(0);
    }
    let mut count = 0;
    for case in fixture["cases"].as_array().unwrap() {
        // The native logistics owner deliberately accepts forced stances. The pin's
        // raw "stance" command misses its forced-control alias allowlist. Keep that
        // seven-action case under the explicit native intent test below.
        if case["name"] == "forced-and-permission" {
            continue;
        }
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
            for row in a["set"].as_array().into_iter().flatten() {
                if row[1].get("dig").is_some_and(|v| !v.is_null()) {
                    let id = row[0].as_u64().unwrap() as u32;
                    assert!(
                        !m.field_works.is_active(id),
                        "accepted command cancels fixture job"
                    );
                }
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
            if let Some(id) = a["digPhase"].as_u64() {
                let id = id as u32;
                let result = m
                    .field_works
                    .step_unit(&mut m.world, id, a["dt"].as_f64().unwrap())
                    .unwrap();
                assert!(
                    result.route.is_none(),
                    "fixture only executes genuine in-reach work"
                );
                m.world.game.units.get_mut(&id).unwrap().dig = m.field_works.is_active(id);
            }
            if step["work"].is_object() {
                assert_eq!(
                    work_snapshot(&m),
                    step["work"],
                    "{} step{index} paid work",
                    case["name"]
                );
            }
            assert_eq!(
                snapshot(&m),
                step["expected"],
                "{} step{index}",
                case["name"]
            );
            for slot in 0..m.world.game.players.len() {
                assert_eq!(
                    crate::recipient_snapshot::movement_for(&m, slot).unwrap(),
                    step["movement"][slot],
                    "{} step{index} movement recipient{slot}",
                    case["name"]
                );
            }
            if case["name"].as_str().unwrap().ends_with("result-retention") {
                let cache = crate::recipient_snapshot::SnapshotCache::new(&m).unwrap();
                for slot in 0..m.world.game.players.len() {
                    assert_eq!(
                        cache.recipient(slot).unwrap().fields["movement"],
                        step["movement"][slot]
                    );
                }
            }
            count += 1;
        }
    }
    assert_eq!(count, 337);
}

#[test]
fn native_forced_stance_preserves_escape_controls_and_permission_boundaries() {
    let fixture: Value =
        serde_json::from_slice(include_bytes!("../../tests/fixtures/match-commands.json")).unwrap();
    let case = fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["name"] == "forced-and-permission")
        .unwrap();
    let steps = case["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 7);
    let mut m = initial(case);
    let denials = [
        None,
        Some(Denial::Blocked),
        None,
        None,
        Some(Denial::Blocked),
        Some(Denial::Needs),
        Some(Denial::Needs),
    ];
    for (index, step) in steps.iter().enumerate() {
        let action = &step["action"];
        for row in action["set"].as_array().into_iter().flatten() {
            set(&mut m, row[0].as_u64().unwrap() as u32, &row[1]);
        }
        let command = Command::from_json(&action["command"]).unwrap();
        assert_eq!(
            m.execute_command(0, &command, false).unwrap(),
            denials[index],
            "native forced permission step{index}"
        );
        // Preserve every recorded actor field except the explicit native choice:
        // accepting Hold Position also clears follow. Stop and escape Move keep
        // that switch. These adjusted rows are native expectations, not a new
        // Source capture or a claim of equality with the pin's raw alias bug.
        let mut expected = step["expected"].clone();
        assert_eq!(expected[0]["id"], 1);
        expected[0]["holdPos"] = json!(true);
        expected[0]["follow"] = json!(0);
        assert_eq!(snapshot(&m), expected, "native forced state step{index}");
        for slot in 0..m.world.game.players.len() {
            assert_eq!(
                crate::recipient_snapshot::movement_for(&m, slot).unwrap(),
                step["movement"][slot],
                "native forced movement step{index} recipient{slot}"
            );
        }
    }
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
        Command::Cover {
            ids: vec![1],
            queue: false,
            at: None,
        },
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
        m.execute_command(
            0,
            &Command::Cover {
                ids: vec![1],
                queue: false,
                at: None
            },
            false
        )
        .unwrap_err(),
        "unrepresented or inconsistent cover field work"
    );
    assert_eq!(snapshot(&m), before);
    assert_eq!(
        Command::from_json(&json!({"t":"cover","ids":[1],"queue":true})).unwrap(),
        Command::Cover {
            ids: vec![1],
            queue: true,
            at: None
        }
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
        m.execute_command(
            0,
            &Command::Cover {
                ids: vec![1, 2],
                queue: false,
                at: None
            },
            false
        )
        .unwrap(),
        None
    );
    assert_eq!(probe(&m), before);
    m.observed_wrecks(0).unwrap();
    assert_eq!(probe(&m), before);
    m.recipient_sees(0, Point { x: 27.0, z: 23.0 }).unwrap();
    assert_eq!(probe(&m), before);
    let mut untouched = initial(&case);
    for id in [1, 3] {
        let low = untouched.world.game.units.get_mut(&id).unwrap();
        low.x = 49.0;
        untouched.grid.update(id, low);
        low.x = 27.0;
        untouched.grid.update(id, low);
    }
    untouched.world.game.tick += 1;
    assert_eq!(
        untouched
            .execute_command(
                0,
                &Command::Cover {
                    ids: vec![1, 2],
                    queue: false,
                    at: None
                },
                false
            )
            .unwrap(),
        None
    );
    untouched.observed_wrecks(0).unwrap();
    untouched
        .recipient_sees(0, Point { x: 27.0, z: 23.0 })
        .unwrap();
    // A late route refusal must also preserve a real paid descriptor and its wallet.
    for candidate in [&mut m, &mut untouched] {
        let mp = candidate.start.economy.players[0].mp;
        assert_eq!(
            candidate
                .execute_command(
                    0,
                    &Command::Dig {
                        ids: vec![1],
                        kind: crate::field_works::FortKind::Trench,
                        at: Point { x: 29.0, z: 27.0 },
                        direction: Some(0.0),
                        queue: false,
                    },
                    false
                )
                .unwrap(),
            None
        );
        assert_eq!(candidate.start.economy.players[0].mp, mp - 30.0);
        assert!(candidate.field_works.is_active(1));
    }
    let paid_jobs = m.field_works.clone();
    let paid_wallet = m.start.economy.players[0].mp;
    untouched.world.game.units.get_mut(&2).unwrap().path_gen = u64::MAX;
    untouched.world.game.tick += 1;
    let witness = 11 * 32 + 14;
    untouched.world.set_cell(witness, b'O').unwrap();
    m.world.game.units.get_mut(&2).unwrap().path_gen = u64::MAX;
    let state = snapshot(&m);
    let generation = m.world.game.units[&1].path_gen;
    m.world.game.tick += 1;
    m.world.set_cell(witness, b'O').unwrap();
    let rows = m.recipients[0].remembered().to_vec();
    let query = m.recipients[0].query();
    let navigation = query.navigation();
    let versions = navigation.versions;
    let flags = navigation.flags.to_vec();
    let heights = navigation.heights.unwrap().to_vec();
    let flags_ptr = navigation.flags.as_ptr();
    let pending = m.world.pending(crate::world::Recipient::Player(0)).cloned();
    assert!(!pending.as_ref().unwrap().is_empty());
    let fog = m.routing_fog.clone();
    assert!(
        m.execute_command(
            0,
            &Command::Cover {
                ids: vec![1, 2],
                queue: false,
                at: None
            },
            false
        )
        .is_err()
    );
    assert_eq!(snapshot(&m), state);
    assert_eq!(m.field_works, paid_jobs);
    assert_eq!(m.start.economy.players[0].mp, paid_wallet);
    assert_eq!(m.world.game.units[&1].path_gen, generation);
    assert_eq!(m.world.game.units[&2].path_gen, u64::MAX);
    assert_eq!(probe(&m), before);
    assert_eq!(m.recipients[0].remembered(), rows.as_slice());
    let query = m.recipients[0].query();
    let navigation = query.navigation();
    assert_eq!(navigation.versions, versions);
    assert_eq!(navigation.flags, flags.as_slice());
    assert_eq!(navigation.heights.unwrap(), heights.as_slice());
    assert_eq!(navigation.flags.as_ptr(), flags_ptr);
    assert_eq!(
        m.world.pending(crate::world::Recipient::Player(0)).cloned(),
        pending
    );
    assert_eq!(m.routing_fog, fog);
    let move_order = Command::Move {
        orders: vec![MoveOrder {
            unit: 1,
            at: Point { x: 35.0, z: 27.0 },
        }],
        control_ids: None,
        attack_move: false,
        queue: false,
        together: false,
        face: None,
    };
    assert_eq!(
        m.execute_command(0, &move_order, false).unwrap(),
        untouched.execute_command(0, &move_order, false).unwrap()
    );
    assert_eq!(snapshot(&m), snapshot(&untouched));
    assert_eq!(
        m.recipients[0].remembered(),
        untouched.recipients[0].remembered()
    );
    assert_eq!(
        m.world.pending(crate::world::Recipient::Player(0)),
        untouched.world.pending(crate::world::Recipient::Player(0))
    );
}

#[test]
fn queued_cover_admission_keeps_live_routes_and_observations() {
    let f: Value =
        serde_json::from_slice(include_bytes!("../../tests/fixtures/match-commands.json")).unwrap();
    let case = f["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "cover-two-squads")
        .unwrap();
    let mut m = initial(case);
    m.world.game.units.get_mut(&1).unwrap().path_gen = u64::MAX;
    m.world.game.tick += 1;
    m.world.set_cell(11 * 32 + 14, b'O').unwrap();
    let pending = m.world.pending(Recipient::Player(0)).cloned();
    let rows = m.recipients[0].remembered().to_vec();
    let fog = m.routing_fog.clone();
    let paths: Vec<_> = m
        .world
        .game
        .units
        .values()
        .map(|u| u.path.clone())
        .collect();
    let cover = Command::from_json(&json!({"t":"cover","ids":[1,2],"queue":true})).unwrap();
    assert_eq!(m.execute_command(0, &cover, false).unwrap(), None);
    assert_eq!(m.world.game.units[&1].path_gen, u64::MAX);
    assert_eq!(
        m.world
            .game
            .units
            .values()
            .map(|u| u.path.clone())
            .collect::<Vec<_>>(),
        paths
    );
    assert_eq!(m.recipients[0].remembered(), rows.as_slice());
    assert_eq!(m.world.pending(Recipient::Player(0)).cloned(), pending);
    assert_eq!(m.routing_fog, fog);
    let Some(Command::Cover {
        queue: false,
        at: Some(anchor),
        ..
    }) = m.commands.units[&1].orders.front()
    else {
        panic!("missing normalized Cover");
    };
    assert_eq!((anchor.x, anchor.z), (27.0, 23.0));
    // An unrepresented later HUD anchor must not partly append the first squad.
    m.commands
        .units
        .get_mut(&2)
        .unwrap()
        .orders
        .push_back(Command::Unsupported);
    let before: Vec<_> = m
        .commands
        .units
        .iter()
        .map(|(id, c)| (*id, c.orders.clone(), c.follow))
        .collect();
    assert!(m.execute_command(0, &cover, false).is_err());
    assert_eq!(
        m.commands
            .units
            .iter()
            .map(|(id, c)| (*id, c.orders.clone(), c.follow))
            .collect::<Vec<_>>(),
        before
    );
}

fn source_number(n: f64) -> Value {
    if n.fract() == 0.0 {
        json!(n as i64)
    } else {
        json!(n)
    }
}
fn work_snapshot(m: &Match) -> Value {
    let dig:Vec<_> = m.world.game.units.keys().map(|id| {
        let d = m.field_works.jobs().get(id).map(|d| {
            let mut row=json!({"x":source_number(d.at.x),"z":source_number(d.at.z),"cells":d.cells.iter().map(|c|json!([c.cell,(c.ch as char).to_string()])).collect::<Vec<_>>(),"t":source_number(d.elapsed),"kind":d.kind.as_str(),"dir":source_number(d.direction)});
            let object=row.as_object_mut().unwrap();
            if let Some(n)=d.original_cells {object.insert("n".into(),json!(n));}
            if let Some(reach)=d.reach {object.insert("reach".into(),source_number(reach));}
            if let Some(front)=d.front {object.insert("front".into(),source_number(front));}
            if let Some(from)=d.from {object.insert("from".into(),json!({"x":source_number(from.x),"z":source_number(from.z)}));}
            row
        });
        json!({"id":id,"dig":d})
    }).collect();
    json!({"players":m.start.economy.players.iter().enumerate().map(|(slot,p)|json!({"slot":slot,"mp":bits(p.mp)})).collect::<Vec<_>>(),"dig":dig})
}

fn log07_match() -> Match {
    let mut m = initial(&json!({
        "map": {
            "name": "powered movement", "w": 32, "h": 24,
            "rows": vec!["................................"; 24],
            "spawns": [{"x": 0, "y": 0}, {"x": 31, "y": 23}],
            "points": [{"id": "p", "x": 12, "y": 12}]
        },
        "initial": [
            {"kind": "tank", "x": 25, "z": 23},
            {"kind": "tank", "x": 53, "z": 39}
        ]
    }));
    m.options.logistics = true;
    m.initialize_logistics().unwrap();
    m
}

fn log07_move(m: &mut Match) {
    let command = Command::Move {
        orders: vec![MoveOrder {
            unit: 1,
            at: Point { x: 55.0, z: 23.0 },
        }],
        control_ids: None,
        attack_move: false,
        queue: false,
        together: false,
        face: None,
    };
    assert_eq!(m.execute_command(0, &command, false).unwrap(), None);
    assert!(!m.world.game.units[&1].path.is_empty());
}

fn log07_step(m: &mut Match, suppression_speed: f64) -> crate::movement::Moved {
    m.world.game.tick += 1;
    m.begin_path_tick(4096, 20).unwrap();
    m.move_unit_with_command_routes(1, suppression_speed)
        .unwrap()
        .unwrap()
}

fn log07_close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-10, "{actual} != {expected}");
}

#[test]
fn mov23_reserve_vehicle_caps_residual_motion_at_full_reduced_and_zero_fuel() {
    for fraction in [1.0, 0.25, 0.0] {
        for clearance in [false, true] {
            let mut m = log07_match();
            log07_move(&mut m);
            let dt = m.rules.movement.tick;
            m.logistics.get_mut(1).unwrap().fuel = Some(dt * fraction);
            let unit = m.world.game.units.get_mut(&1).unwrap();
            let before = unit.at();
            unit.move_speed = 50.0;
            unit.travel_dir = Some(0.0);
            if clearance {
                unit.clearance = Some(crate::game::Clearance {
                    goal: crate::game::Pose {
                        x: before.x + 5.0,
                        z: before.z,
                        rot: 0.0,
                    },
                    reverse: false,
                    path: unit.path_gen,
                    until: 100.0,
                });
            }
            let moved = log07_step(&mut m, 1.0);
            let unit = &m.world.game.units[&1];
            let distance = crate::math::hypot(unit.x - before.x, unit.z - before.z);
            log07_close(distance, moved.speed * dt);
            if fraction == 0.0 {
                assert_eq!(unit.at(), before);
                assert_eq!(unit.move_speed, 0.0);
            }
        }
    }
}

#[test]
fn mov23_convoy_without_reserves_is_capped_but_ordinary_momentum_is_preserved() {
    use crate::supply_convoys::{ConvoyState, StoreId, Truck, TruckState};

    for convoy in [false, true] {
        let mut m = initial(&json!({
            "map": {
                "name":"convoy movement cap", "w":32, "h":24,
                "rows":vec!["................................"; 24],
                "spawns":[{"x":0,"y":0},{"x":31,"y":23}],
                "points":[{"id":"p","x":12,"y":12}]
            },
            "initial":[{"kind":"truck","x":25,"z":23}]
        }));
        log07_move(&mut m);
        assert!(m.logistics.get(1).is_none());
        if convoy {
            let mut state = ConvoyState::default();
            state.trucks.insert(
                1,
                Truck {
                    state: TruckState::Idle,
                    cargo: crate::logistics::Stock::default(),
                    target: None,
                    origin: StoreId::Source(0),
                    timer: 0.0,
                    manual: true,
                    hold: false,
                    route: vec![],
                    destination: None,
                    route_failure: None,
                    retry: None,
                },
            );
            m.convoys = Some(state);
        }
        let unit = m.world.game.units.get_mut(&1).unwrap();
        let before = unit.at();
        unit.move_speed = 20.0;
        unit.travel_dir = Some(0.0);
        let moved = log07_step(&mut m, 0.01);
        let unit = &m.world.game.units[&1];
        let distance = crate::math::hypot(unit.x - before.x, unit.z - before.z);
        let budget = moved.speed * m.rules.movement.tick;
        if convoy {
            log07_close(distance, budget);
        } else {
            assert!(
                distance > budget,
                "ordinary braking retains its existing momentum"
            );
        }
        assert!(m.logistics.get(1).is_none());
    }
}

#[test]
fn log07_parked_and_riding_movement_preserve_fuel() {
    let mut m = log07_match();
    let before = m.world.game.units[&1].at();
    let reserves = m.logistics.get(1).unwrap().clone();
    log07_step(&mut m, 1.0);
    assert_eq!(m.world.game.units[&1].at(), before);
    assert_eq!(m.logistics.get(1), Some(&reserves));

    log07_move(&mut m);
    // Authored rider facts also protect direct wrapper calls; the normal tick skips riders.
    m.world.game.units.get_mut(&1).unwrap().riding = true;
    m.combat.get_mut(1).unwrap().carrier = Some(2);
    m.combat.get_mut(2).unwrap().cargo.push(1);
    log07_step(&mut m, 1.0);
    let unit = &m.world.game.units[&1];
    assert!(crate::math::hypot(unit.x - before.x, unit.z - before.z) > 0.0);
    assert_eq!(m.logistics.get(1), Some(&reserves));
}

#[test]
fn log07_acceleration_charges_actual_distance_without_reticking_reserves() {
    let mut m = log07_match();
    log07_move(&mut m);
    m.advance_unit_reserves(1).unwrap();
    let before = m.world.game.units[&1].at();
    let reserves = m.logistics.get(1).unwrap().clone();
    assert_eq!(m.world.game.units[&1].move_speed, 0.0);
    let moved = log07_step(&mut m, 1.0);
    let unit = &m.world.game.units[&1];
    let distance = crate::math::hypot(unit.x - before.x, unit.z - before.z);
    let driving = distance / moved.speed;
    assert!(driving > 0.0 && driving < m.rules.movement.tick);
    let after = m.logistics.get(1).unwrap();
    log07_close(after.fuel.unwrap(), reserves.fuel.unwrap() - driving);
    assert_eq!(after.emergency, reserves.emergency);
    assert_eq!(after.provisions, reserves.provisions);
    assert_eq!(after.warning, reserves.warning);
    assert!(!after.forced && !after.stranded);
}

#[test]
fn log07_ordinary_depletion_caps_travel_and_charges_actual_driving() {
    let mut baseline = log07_match();
    log07_move(&mut baseline);
    let full_speed = log07_step(&mut baseline, 1.0).speed;
    for fraction in [0.5, 1e-5] {
        let mut m = log07_match();
        log07_move(&mut m);
        let dt = m.rules.movement.tick;
        let ordinary = dt * fraction;
        let emergency = dt * 2.0;
        let r = m.logistics.get_mut(1).unwrap();
        r.fuel = Some(ordinary);
        r.emergency = Some(emergency);
        assert!(!r.forced);
        // Existing momentum exercises the displacement cap; tiny powered speed exercises the .001 divisor floor.
        m.world.game.units.get_mut(&1).unwrap().move_speed = if fraction == 0.5 {
            full_speed
        } else {
            full_speed * fraction
        };
        let before = m.world.game.units[&1].at();
        let moved = log07_step(&mut m, 1.0);
        log07_close(moved.speed, full_speed * fraction);
        let unit = &m.world.game.units[&1];
        let distance = crate::math::hypot(unit.x - before.x, unit.z - before.z);
        assert!(distance > 0.0);
        assert!(distance <= moved.speed * dt + 1e-12);
        let driving = crate::math::min(dt, distance / crate::math::max(moved.speed, 0.001));
        assert!(driving > ordinary);
        if fraction == 0.5 {
            log07_close(distance / moved.speed, dt);
            log07_close(driving, dt);
        } else {
            assert!(moved.speed < 0.001 && driving < dt);
        }
        let after = m.logistics.get(1).unwrap();
        assert_eq!(after.fuel, Some(0.0));
        log07_close(after.emergency.unwrap(), emergency - (driving - ordinary));
        assert!(after.forced && !after.stranded);
    }
}

#[test]
fn log07_forced_escape_scales_retreat_speed_and_empty_fuel_strands() {
    let mut baseline = log07_match();
    baseline.logistics.get_mut(1).unwrap().forced = true;
    log07_move(&mut baseline);
    assert!(baseline.world.game.units[&1].retreating);
    let retreat_speed = log07_step(&mut baseline, 0.0).speed;
    assert!(retreat_speed > 0.0);

    let mut m = log07_match();
    let dt = m.rules.movement.tick;
    let r = m.logistics.get_mut(1).unwrap();
    r.fuel = Some(0.0);
    r.emergency = Some(dt / 2.0);
    r.forced = true;
    log07_move(&mut m);
    assert!(m.world.game.units[&1].retreating);
    assert!(m.commands.units[&1].supply_escape);
    m.world.game.units.get_mut(&1).unwrap().move_speed = retreat_speed / 2.0;
    let before = m.world.game.units[&1].at();
    let moved = log07_step(&mut m, 0.0);
    log07_close(moved.speed, retreat_speed / 2.0);
    let unit = &m.world.game.units[&1];
    assert!(crate::math::hypot(unit.x - before.x, unit.z - before.z) > 0.0);
    let r = m.logistics.get(1).unwrap();
    assert_eq!((r.fuel, r.emergency), (Some(0.0), Some(0.0)));
    assert!(r.forced && r.stranded);

    let mut empty = log07_match();
    let r = empty.logistics.get_mut(1).unwrap();
    r.fuel = Some(0.0);
    r.emergency = Some(0.0);
    log07_move(&mut empty);
    let before = empty.world.game.units[&1].at();
    for _ in 0..2 {
        assert_eq!(log07_step(&mut empty, 1.0).speed, 0.0);
        assert_eq!(empty.world.game.units[&1].at(), before);
        let r = empty.logistics.get(1).unwrap();
        assert_eq!((r.fuel, r.emergency), (Some(0.0), Some(0.0)));
        assert!(r.forced && r.stranded);
    }
}
