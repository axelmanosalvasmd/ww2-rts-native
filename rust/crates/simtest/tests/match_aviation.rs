use crate::shared::aviation as common;
use common::{
    aircraft_phase_snapshot, assert_json, bits, make, mission, patch, scene, snapshot, state_name,
};
use serde_json::{Value, json};
use std::io::Read;
use ww2_core::{
    game::{PlaneState, Point},
    match_runtime::Match,
    unit_publication::{PublicationParts, StagedUnit, prepare_publication},
};
fn fixture() -> Value {
    let mut text = String::new();
    flate2::read::GzDecoder::new(&include_bytes!("fixtures/match-aviation.json.gz")[..])
        .read_to_string(&mut text)
        .unwrap();
    serde_json::from_str(&text).unwrap()
}
#[test]
fn explicit_aircraft_factory_matches_original_spawn_and_preserves_ordinary_policy() {
    let f = fixture();
    for row in f["constructors"].as_array().unwrap() {
        let mut m = make(&row["map"]);
        m.world.game.units.clear();
        m.commands.units.clear();
        m.recruit_metadata.clear();
        m.next_unit_id = 1000;
        m.grid.rebuild(&m.world.game.units);
        let owner = row["owner"].as_u64().unwrap() as usize;
        let kind = m.rules.kind(row["kind"].as_str().unwrap()).unwrap();
        let index = row["index"].as_u64().unwrap() as u32;
        let spawn = Point {
            x: row["spawn"]["x"].as_f64().unwrap(),
            z: row["spawn"]["z"].as_f64().unwrap(),
        };
        let base = Point {
            x: row["base"]["x"].as_f64().unwrap(),
            z: row["base"]["z"].as_f64().unwrap(),
        };
        assert!(
            StagedUnit::ordinary(
                &m.world.game,
                &m.rules,
                owner,
                kind,
                1000,
                index,
                spawn,
                None
            )
            .is_err()
        );
        let draft = StagedUnit::aircraft(
            &m.world.game,
            &m.rules,
            owner,
            kind,
            1000,
            index,
            spawn,
            base,
        )
        .unwrap();
        let u = &draft.unit;
        let c = &draft.control;
        assert!(c.orders.is_empty());
        let got = json!({"id":u.id,"owner":u.owner,"kind":m.rules.kinds[u.kind].name,"x":bits(u.x),"z":bits(u.z),"rot":bits(u.rot),"hp":bits(u.hp),"supp":bits(u.supp),"air":if u.air==Some(PlaneState::Base){"base"}else{"unexpected"},"still":bits(u.still.unwrap()),"vx":bits(u.vx.unwrap()),"vz":bits(u.vz.unwrap()),"moveSpeed":bits(u.move_speed),"travelDir":bits(u.travel_dir.unwrap()),"path":u.path.iter().map(|p|json!({"x":p.x,"z":p.z})).collect::<Vec<_>>(),"orders":Vec::<Value>::new(),"auto":c.auto_cast,"autoRetreat":c.auto_retreat,"aim":bits(c.aim),"cooldown":bits(0.0),"retarget":bits(0.0),"targetId":0,"hitBy":0,"hitAt":bits(-1e9)});
        assert_eq!(got, row["expected"], "{}", row["kind"]);
        let ids = prepare_publication(
            PublicationParts {
                units: &mut m.world.game.units,
                grid: &mut m.grid,
                controls: &mut m.commands.units,
                recruitment: &mut m.recruit_metadata,
                next_id: &mut m.next_unit_id,
            },
            &m.rules,
            2,
            vec![draft],
        )
        .unwrap()
        .publish_with(|u, _| {
            m.combat.initialize(u.id).hit_by = Some(0);
            m.firing.initialize(u.id);
        });
        assert_eq!(ids, [1000]);
        assert_eq!(m.next_unit_id, 1001);
        assert_eq!(m.world.game.units[&1000].at(), base);
        let firing = m.firing.get(1000).unwrap();
        let combat = m.combat.get(1000).unwrap();
        for (key, actual) in [
            ("cooldown", bits(firing.cooldown)),
            ("retarget", bits(firing.retarget)),
            ("targetId", json!(firing.target_id)),
            ("hitBy", json!(combat.hit_by)),
            ("hitAt", bits(combat.hit_at)),
        ] {
            assert_eq!(actual, row["expected"][key], "published {key}");
        }
    }
    assert_eq!(f["constructors"].as_array().unwrap().len(), 18);
}

#[test]
fn held_flights_rearming_and_continuous_aa_match_every_original_call() {
    let f = fixture();
    let mut checkpoints = 0;
    for trace in f["traces"].as_array().unwrap() {
        let mut m = scene(trace);
        assert_json(
            &snapshot(&m, &[]),
            &trace["initial"],
            &format!("{} initial", trace["name"]),
        );
        checkpoints += 1;
        for (index, frame) in trace["frames"].as_array().unwrap().iter().enumerate() {
            let a = &frame["action"];
            let id = a["id"].as_u64().unwrap_or(0) as u32;
            let admitted = match a["op"].as_str().unwrap() {
                "step" => {
                    m.step_aircraft(id, a["dt"].as_f64().unwrap()).unwrap();
                    None
                }
                "aa" => {
                    let ids = a["actors"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|v| v.as_u64().unwrap() as u32)
                        .collect::<Vec<_>>();
                    m.step_aircraft_anti_air(&ids, a["dt"].as_f64().unwrap())
                        .unwrap();
                    None
                }
                "mission" => Some(
                    m.send_aircraft_mission(
                        m.world.game.units[&id].owner,
                        id,
                        mission(&a["mission"]),
                    )
                    .unwrap(),
                ),
                "home" => Some(
                    m.return_aircraft_home(m.world.game.units[&id].owner, id)
                        .unwrap(),
                ),
                "patch" => {
                    patch(&mut m, id, &a["value"]);
                    None
                }
                "remove" => {
                    m.world.game.units.remove(&id);
                    m.grid.rebuild(&m.world.game.units);
                    None
                }
                "visible" => {
                    m.world.game.players[a["owner"].as_u64().unwrap() as usize].visible = a["ids"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|v| v.as_u64().unwrap() as u32)
                        .collect();
                    None
                }
                "rebuild" => {
                    m.grid.rebuild(&m.world.game.units);
                    None
                }
                "reregister" => {
                    m.grid.remove_unit(id);
                    m.grid.update(id, &m.world.game.units[&id]);
                    None
                }
                _ => panic!("unknown fixture action"),
            };
            assert_eq!(
                json!(admitted),
                frame["admitted"],
                "{} action {index}",
                trace["name"]
            );
            let events = m.aviation.take_events();
            assert_json(
                &snapshot(&m, &events),
                &frame["expected"],
                &format!("{} action {index}", trace["name"]),
            );
            assert!(m.firing.events.is_empty());
            checkpoints += 1;
        }
    }
    assert_eq!(f["traces"].as_array().unwrap().len(), 42);
    assert_eq!(checkpoints, f["checkpoints"].as_u64().unwrap());
    assert_eq!(checkpoints, 3410);
}
#[test]
fn invalid_late_aircraft_metadata_refuses_before_mutation() {
    use ww2_core::match_aviation::{AviationError, MissionKind, PlaneMission};
    let f = fixture();
    let mut m = scene(&f["traces"][0]);
    let before = snapshot(&m, &[]);
    let area = PlaneMission {
        kind: MissionKind::Area,
        target: None,
        at: Point { x: 40.0, z: 40.0 },
    };
    assert!(matches!(
        m.send_aircraft_mission(0, 1000, area),
        Err(AviationError::Invalid(_))
    ));
    assert_eq!(snapshot(&m, &[]), before);
    m.commands.units.get_mut(&1000).unwrap().hold_fire = false;
    let patrol = PlaneMission {
        kind: MissionKind::Patrol,
        ..area
    };
    assert!(m.send_aircraft_mission(0, 1000, patrol).unwrap());
    assert_eq!(m.aviation.planes[&1000].mission, Some(patrol));
    assert_eq!(m.world.game.units[&1000].air, Some(PlaneState::Out));
    assert!(!m.commands.units[&1000].hold_fire);
    let ammo = m.aviation.planes[&1000].ammo;
    assert!(m.step_aircraft_turn(1000, 0.05).unwrap());
    assert_eq!(m.aviation.planes[&1000].ammo, ammo);
    assert!(m.firing.events.is_empty());
    let trace = f["traces"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "aa-killed-later-fighter-must-skip")
        .unwrap();
    let mut m = scene(trace);
    m.combat.get_mut(1001).unwrap().xp = -1.0;
    let before = snapshot(&m, &[]);
    assert!(matches!(
        m.step_aircraft_anti_air(&[1000, 1001], 0.05),
        Err(AviationError::Invalid(_))
    ));
    assert_eq!(snapshot(&m, &[]), before);
    m.combat.get_mut(1001).unwrap().xp = 0.0;
    m.aviation.planes.remove(&1001);
    let before = format!("{:?}", m.world.game.units);
    assert!(matches!(
        m.step_aircraft_anti_air(&[1000, 1001], 0.05),
        Err(AviationError::Unsupported(_))
    ));
    assert_eq!(format!("{:?}", m.world.game.units), before);
    assert!(m.aviation.events.is_empty());
}

fn aircraft_command_fixture() -> Value {
    let mut text = String::new();
    flate2::read::GzDecoder::new(&include_bytes!("fixtures/match-aircraft-commands.json.gz")[..])
        .read_to_string(&mut text)
        .unwrap();
    serde_json::from_str(&text).unwrap()
}
fn command_scene(trace: &Value) -> Match {
    use ww2_core::{
        game::Traffic,
        match_commands::{Command, Facing, MoveOutcome},
    };
    let mut m = scene(trace);
    // The source fixture installs stale entrench objects. Ground replacements
    // must still cancel a real native project membership, rather than a bare flag.
    let ground: Vec<_> = m
        .world
        .game
        .units
        .values()
        .filter(|u| !m.rules.kinds[u.kind].air)
        .map(|u| (u.id, u.owner))
        .collect();
    for (id, owner) in ground {
        let project = m
            .field_works
            .prepare_project(
                &m.world,
                &m.start.economy.players[owner],
                ww2_core::field_works::ProjectRequest {
                    owner,
                    pattern: ww2_core::field_works::Pattern::Line,
                    fort: ww2_core::field_works::FortKind::Trench,
                    from: Point { x: 30.0, z: 30.0 },
                    to: Point { x: 36.0, z: 30.0 },
                    back: Point { x: 30.0, z: 36.0 },
                },
                |_| true,
            )
            .unwrap()
            .commit(&mut m.field_works)
            .unwrap();
        m.field_works.join_project(project, &[id]).unwrap();
    }
    for (&id, u) in &mut m.world.game.units {
        u.path = vec![Point { x: 9.0, z: 10.0 }];
        u.traffic = Some(Traffic {
            goal: Point { x: 6.0, z: 7.0 },
            legs: Vec::new(),
            passing: false,
            by: 888,
            until: 20.0,
        });
        u.traffic_wait = 5.0;
        u.world_goal = Some(Point { x: 19.0, z: 20.0 });
        u.entrench = true;
        u.attack = true;
        u.retreating = true;
        let c = m.commands.units.get_mut(&id).unwrap();
        c.follow = 777;
        c.orders
            .push_back(Command::from_json(&json!({"t":"move","orders":[[id,8,8]]})).unwrap());
        c.face = Some(Facing {
            angle: 0.75,
            at: Point { x: 22.0, z: 23.0 },
        });
        c.outcome = Some(MoveOutcome::Moving);
        c.outcome_tick = Some(3);
        c.attack_id = 777;
        m.firing.get_mut(id).unwrap().target_id = 555;
    }
    m
}
fn command_snapshot(m: &Match) -> Value {
    use ww2_core::match_commands::{Command, MoveOutcome};
    let point = |p: Point| json!({"x":bits(p.x),"z":bits(p.z)});
    json!(m.world.game.units.values().map(|u| {
        let c=&m.commands.units[&u.id];
        let f=m.firing.get(u.id).unwrap();
        let a=m.aviation.planes.get(&u.id);
        let orders=c.orders.iter().map(|o|match o {
            Command::Move{orders,control_ids,attack_move,..}=>json!({"t":if *attack_move{"amove"}else{"move"},"orders":orders.iter().map(|o|json!([o.unit,o.at.x,o.at.z])).collect::<Vec<_>>(),"ids":control_ids,"target":null,"x":null,"z":null}),
            _=>panic!("unexpected waiting aircraft order"),
        }).collect::<Vec<_>>();
        let air=a.map(|a|json!({"state":state_name(u.air.unwrap()),"fuel":bits(a.fuel),"ammo":bits(a.ammo),"timer":bits(a.timer),"ang":bits(a.orbit),"mission":a.mission.map(|m|json!({"kind":m.kind.name(),"id":m.target.unwrap_or(0),"x":bits(m.at.x),"z":bits(m.at.z)}))}));
        json!({"id":u.id,"owner":u.owner,"kind":m.rules.kinds[u.kind].name,"at":point(u.at()),"rot":bits(u.rot),"hp":bits(u.hp),"path":u.path.iter().map(|p|json!({"x":p.x,"z":p.z})).collect::<Vec<_>>(),"follow":c.follow,"orders":orders,"face":c.face.map(|f|json!({"a":bits(f.angle),"x":bits(f.at.x),"z":bits(f.at.z)})),"traffic":u.traffic.is_some(),"trafficWait":bits(u.traffic_wait),"worldGoal":u.world_goal.map(point),"entrench":u.entrench,"outcome":c.outcome.map(|o|match o{MoveOutcome::Interrupted=>"interrupted",MoveOutcome::Moving=>"moving",MoveOutcome::Unreachable=>"unreachable",MoveOutcome::Arrived=>"arrived"}),"outcomeTick":c.outcome_tick,"attackId":c.attack_id,"targetId":f.target_id,"retreating":u.retreating,"holdFire":c.hold_fire,"holdPos":u.hold_pos,"autoRetreat":c.auto_retreat,"air":air})
    }).collect::<Vec<_>>())
}
#[test]
fn typed_aircraft_commands_match_original_selection_and_native_withdrawal_stances() {
    use ww2_core::match_commands::Command;
    let mut f = aircraft_command_fixture();
    // LOG-13 intentionally admits the stance rejected by the pinned capture.
    // Every other aircraft selection, cleanup and state assertion stays exact.
    let forced = f["traces"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|trace| trace["name"] == "forced-ground-stance-remains-ineligible")
        .unwrap();
    forced["frames"][0]["denial"] = Value::Null;
    forced["frames"][0]["expected"][0]["holdPos"] = json!(true);
    forced["frames"][0]["expected"][0]["follow"] = json!(0);
    let mut checkpoints = 0;
    for trace in f["traces"].as_array().unwrap() {
        let mut m = command_scene(trace);
        assert_json(
            &command_snapshot(&m),
            &trace["initial"],
            &format!("{} initial", trace["name"]),
        );
        checkpoints += 1;
        let paths = m
            .world
            .game
            .units
            .iter()
            .filter(|(_, u)| u.air.is_some())
            .map(|(id, u)| (*id, u.path_gen))
            .collect::<Vec<_>>();
        for (i, frame) in trace["frames"].as_array().unwrap().iter().enumerate() {
            let denial = match Command::from_json(&frame["command"]) {
                Ok(cmd) => m
                    .execute_command(frame["owner"].as_u64().unwrap() as usize, &cmd, false)
                    .unwrap(),
                Err(denial) => Some(denial),
            };
            assert_eq!(
                json!(denial.map(|d| d.name())),
                frame["denial"],
                "{} action {i}",
                trace["name"]
            );
            assert_json(
                &command_snapshot(&m),
                &frame["expected"],
                &format!("{} action {i}", trace["name"]),
            );
            for &(id, generation) in &paths {
                assert_eq!(m.world.game.units[&id].path_gen, generation);
            }
            assert_eq!(m.commands.path_requests().used(), 0);
            assert_eq!(m.commands.path_requests().queue_len(), 0);
            assert_eq!(m.commands.path_requests().pending_len(), 0);
            assert!(m.aviation.events.is_empty());
            assert!(m.firing.events.is_empty());
            checkpoints += 1;
        }
    }
    assert_eq!(f["traces"].as_array().unwrap().len(), 71);
    assert_eq!(checkpoints, 147);
    assert_eq!(checkpoints, f["checkpoints"].as_u64().unwrap());
}
#[test]
fn mixed_attack_cancels_canonical_project_and_preflights_every_selected_actor() {
    use ww2_core::{
        match_aviation::MissionKind,
        match_commands::{Command, Denial},
    };
    let f = aircraft_command_fixture();
    let trace = json!({
        "map": f["traces"][0]["map"],
        "specs": [
            {"kind":"fighter","owner":0,"x":40,"z":40},
            {"kind":"rifle","owner":1,"x":60,"z":50},
            {"kind":"rifle","owner":0,"x":46,"z":47},
            {"kind":"rifle","owner":0,"x":50,"z":47}
        ],
        "visible": true
    });
    for ids in [[1000, 1002, 1003], [1002, 1003, 1000]] {
        for case in 0..5 {
            let mut m = scene(&trace);
            m.start.economy.players[0].mp = 1000.;
            let entrench = Command::from_json(&json!({
                "t":"entrench","ids":[1002,1003],"pattern":"line",
                "x":44,"z":52,"x2":56,"z2":52
            }))
            .unwrap();
            assert_eq!(m.execute_command(0, &entrench, false).unwrap(), None);
            let project = m.field_works.membership(1002).unwrap();
            assert_eq!(m.field_works.membership(1003), Some(project));
            for id in [1000, 1002, 1003] {
                m.commands.units.get_mut(&id).unwrap().follow = 777;
            }
            let generation = m.world.game.units[&1002].path_gen;
            match case {
                1 => {
                    // A later ground row has a scalar without its admitted membership.
                    m.field_works.cancel(1003);
                }
                2 => m.world.game.units.get_mut(&1003).unwrap().path_gen = u64::MAX,
                3 => {
                    m.commands.units.remove(&1003);
                }
                4 => {
                    m.aviation.planes.remove(&1000);
                }
                _ => {}
            }
            let state = |m: &Match| {
                format!(
                    "{:?}",
                    (
                        &m.world.game.units,
                        &m.commands.units,
                        &m.field_works,
                        &m.aviation,
                        &m.logistics,
                        &m.firing,
                        &m.abilities,
                        &m.grid,
                        m.commands.path_requests()
                    )
                )
            };
            let before = state(&m);
            let attack =
                Command::from_json(&json!({"t":"attack","ids":ids,"target":1001})).unwrap();
            let result = m.execute_command(0, &attack, false);
            if case != 0 {
                if case == 4 {
                    assert_eq!(result.unwrap(), Some(Denial::Unsupported));
                } else {
                    assert_eq!(
                        result.unwrap_err(),
                        match case {
                            1 => "invalid canonical ground order actor",
                            2 => "ground order path identity exhausted",
                            3 => "missing construction control",
                            _ => unreachable!(),
                        }
                    );
                }
                assert_eq!(state(&m), before, "case {case}, selection {ids:?}");
                continue;
            }
            assert_eq!(result.unwrap(), None);
            for id in [1002, 1003] {
                assert_eq!(m.field_works.membership(id), None);
                assert!(!m.world.game.units[&id].entrench);
                assert!(!m.world.game.units[&id].dig);
                assert!(m.world.game.units[&id].attack);
                assert!(m.world.game.units[&id].path.is_empty());
                assert_eq!(m.commands.units[&id].attack_id, 1001);
                assert_eq!(m.commands.units[&id].follow, 0);
            }
            assert_eq!(m.world.game.units[&1002].path_gen, generation + 1);
            let mission = m.aviation.planes[&1000].mission.unwrap();
            assert_eq!(mission.kind, MissionKind::Attack);
            assert_eq!(mission.target, Some(1001));
            assert_eq!(mission.at, m.world.game.units[&1001].at());
            assert_eq!(m.world.game.units[&1000].air, Some(PlaneState::Out));
            assert_eq!(m.commands.units[&1000].follow, 0);
        }
    }
}

#[test]
fn mixed_aircraft_selections_preserve_canonical_supply_and_control_boundaries() {
    use ww2_core::{
        match_aviation::MissionKind,
        match_commands::{Command, Denial},
    };
    let f = aircraft_command_fixture();
    let trace = f["traces"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "fighter-base-attack-air")
        .unwrap();
    let commands = [
        json!({"t":"move","orders":[[1002,20,21],[1000,22,23]]}),
        json!({"t":"stop","ids":[1002,1000]}),
        json!({"t":"attack","ids":[1002,1000],"target":1002}),
        json!({"t":"move","queue":true,"orders":[[1002,20,21],[1000,22,23]]}),
        json!({"t":"retreat","ids":[1000,1002]}),
        json!({"t":"stance","ids":[1002,1000],"key":"holdFire","on":false}),
    ];
    for (index, raw) in commands.iter().enumerate() {
        let mut m = command_scene(trace);
        m.world.game.units.get_mut(&1000).unwrap().air = Some(PlaneState::Out);
        if index <= 1 {
            m.commands.units.get_mut(&1000).unwrap().hold_fire = false;
        }
        patch(
            &mut m,
            1002,
            &json!({"logistics":{"ammo":100,"ammoMax":100,"ammoPrice":0.01,"ammoCredit":0,"provisions":120,"fuel":180,"emergency":60,"warning":null,"forced":true,"stranded":false,"exhausted":false}}),
        );
        if index == 2 {
            m.world.game.units.get_mut(&1002).unwrap().owner = 1;
            m.grid.update(1002, &m.world.game.units[&1002]);
        }
        m.commands.units.get_mut(&1002).unwrap().supply_hold = true;
        let before = command_snapshot(&m);
        let prior_mission = m.aviation.planes[&1000].mission;
        let reserves = m.logistics.get(1002).unwrap().clone();
        let command = Command::from_json(raw).unwrap();
        let denial = m.execute_command(0, &command, false).unwrap();
        if index == 3 {
            assert_eq!(denial, None, "queued mixed selection");
            let mut after = command_snapshot(&m);
            for id in [1000, 1002] {
                let control = &m.commands.units[&id];
                assert_eq!(control.orders.len(), 2);
                let Command::Move { orders, queue, .. } = control.orders.back().unwrap() else {
                    panic!("queued mixed move must retain a normalized Move");
                };
                assert!(!queue);
                assert_eq!(orders.len(), 1);
                assert_eq!(orders[0].unit, i64::from(id));
                assert_eq!(
                    orders[0].at,
                    if id == 1000 {
                        Point { x: 22.0, z: 23.0 }
                    } else {
                        Point { x: 20.0, z: 21.0 }
                    }
                );
                assert_eq!(control.follow, 0);
                let row = after
                    .as_array_mut()
                    .unwrap()
                    .iter_mut()
                    .find(|row| row["id"] == id)
                    .unwrap();
                let prior = before
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|row| row["id"] == id)
                    .unwrap();
                row["orders"] = prior["orders"].clone();
                row["follow"] = prior["follow"].clone();
            }
            assert_eq!(after, before);
            assert_eq!(m.logistics.get(1002).unwrap(), &reserves);
            assert!(!m.commands.units[&1002].supply_hold);
            assert!(!m.commands.units[&1002].supply_escape);
            assert!(m.field_works.membership(1002).is_some());
            assert_eq!(m.commands.path_requests().used(), 0);
        } else if matches!(index, 0..=2 | 5) {
            assert_eq!(denial, None, "case {index}");
            assert_eq!(m.world.game.units[&1000].air, Some(PlaneState::Out));
            let mission = m.aviation.planes[&1000].mission.unwrap();
            assert_eq!(
                mission.kind,
                if index == 2 {
                    ww2_core::match_aviation::MissionKind::Attack
                } else {
                    ww2_core::match_aviation::MissionKind::Patrol
                }
            );
            match index {
                0 => {
                    assert_eq!(mission.at, Point { x: 22.0, z: 23.0 });
                    assert!(!m.commands.units[&1002].supply_hold);
                    assert!(m.commands.units[&1002].supply_escape);
                    assert!(!m.world.game.units[&1002].path.is_empty());
                    assert!(!m.logistics.get(1002).unwrap().stranded);
                    assert_eq!(m.logistics.get(1002).unwrap(), &reserves);
                    assert!(!m.world.game.units[&1002].entrench);
                    assert_eq!(m.field_works.membership(1002), None);
                }
                1 => {
                    assert_eq!(mission.at, m.world.game.units[&1000].at());
                    assert!(m.commands.units[&1002].supply_hold);
                    assert!(!m.world.game.units[&1002].entrench);
                    assert_eq!(m.field_works.membership(1002), None);
                    assert!(m.world.game.units[&1002].path.is_empty());
                    assert_eq!(m.logistics.get(1002).unwrap(), &reserves);
                }
                2 => {
                    assert_eq!(mission.target, Some(1002));
                    assert_eq!(mission.at, m.world.game.units[&1002].at());
                    assert!(m.commands.units[&1000].hold_fire);
                    assert_eq!(command_snapshot(&m)[2], before[2]);
                    assert_eq!(m.logistics.get(1002).unwrap(), &reserves);
                    assert!(m.commands.units[&1002].supply_hold);
                    assert!(!m.commands.units[&1002].supply_escape);
                }
                5 => {
                    assert!(!m.commands.units[&1000].hold_fire);
                    assert_eq!(m.logistics.get(1002).unwrap(), &reserves);
                    assert!(m.commands.units[&1002].supply_hold);
                    assert!(!m.commands.units[&1002].supply_escape);
                    assert!(m.field_works.membership(1002).is_some());
                }
                _ => unreachable!(),
            }
        } else if index == 4 {
            assert_eq!(denial, None, "mixed retreat");
            assert_eq!(m.world.game.units[&1000].air, Some(PlaneState::Home));
            assert_eq!(m.aviation.planes[&1000].mission, prior_mission);
            assert_eq!(m.logistics.get(1002).unwrap(), &reserves);
            assert!(!m.commands.units[&1002].supply_hold);
            assert!(!m.commands.units[&1002].supply_escape);
            assert!(!m.world.game.units[&1002].entrench);
            assert_eq!(m.field_works.membership(1002), None);
            assert!(!m.world.game.units[&1002].path.is_empty());
        } else {
            assert_eq!(denial, Some(Denial::Unsupported), "case {index}");
            assert_eq!(command_snapshot(&m), before, "case {index}");
            assert_eq!(m.logistics.get(1002).unwrap(), &reserves);
            assert!(m.commands.units[&1002].supply_hold);
            assert!(!m.commands.units[&1002].supply_escape);
            assert_eq!(m.commands.path_requests().used(), 0);
        }
    }
    // A scalar without its project still fails before either selection can mutate.
    for raw in &commands[..2] {
        let mut m = command_scene(trace);
        m.field_works.cancel(1002);
        let before = format!(
            "{:?}|{:?}|{:?}|{:?}",
            command_snapshot(&m),
            m.field_works,
            m.logistics,
            m.commands.path_requests()
        );
        let error = m
            .execute_command(0, &Command::from_json(raw).unwrap(), false)
            .unwrap_err();
        assert_eq!(error, "unrepresented or inconsistent field-work activity");
        assert_eq!(
            format!(
                "{:?}|{:?}|{:?}|{:?}",
                command_snapshot(&m),
                m.field_works,
                m.logistics,
                m.commands.path_requests()
            ),
            before
        );
    }
    for missing_physical in [false, true] {
        let mut m = command_scene(trace);
        if missing_physical {
            m.world.game.units.get_mut(&1000).unwrap().air = None;
        } else {
            m.aviation.planes.remove(&1000);
        }
        let units = format!("{:?}", m.world.game.units);
        let controls = m.commands.units.clone();
        let raw = json!({"t":"move","orders":[[1002,20,21],[1000,22,23]]});
        let result = m.execute_command(0, &Command::from_json(&raw).unwrap(), false);
        if missing_physical {
            assert!(result.is_err());
        } else {
            assert_eq!(result.unwrap(), Some(Denial::Unsupported));
        }
        assert_eq!(format!("{:?}", m.world.game.units), units);
        assert_eq!(m.commands.units, controls);
        assert_eq!(m.commands.path_requests().used(), 0);
    }
    let mut m = command_scene(trace);
    m.start_queued_orders(1000, false).unwrap();
    assert!(m.commands.units[&1000].orders.is_empty());
    let mission = m.aviation.planes[&1000].mission.unwrap();
    assert_eq!(mission.kind, MissionKind::Patrol);
    assert_eq!(mission.at, Point { x: 8.0, z: 8.0 });
    assert_eq!(m.world.game.units[&1000].air, Some(PlaneState::Out));
    m.commands.units.get_mut(&1000).unwrap().orders.clear();
    m.start_queued_orders(1000, false).unwrap();
    m.prepare_unit_commands(1000, false).unwrap();
}

#[test]
fn aircraft_commands_replay_on_the_single_recorded_boundary_queue() {
    use ww2_core::{match_commands::Command, orders::OrderKey};
    let f = aircraft_command_fixture();
    let trace = f["traces"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "recorded-plane-command-sequence")
        .unwrap();
    let frames = trace["frames"].as_array().unwrap();
    for arrival in [[0, 1, 2, 3], [3, 1, 0, 2], [2, 0, 3, 1]] {
        let mut m = command_scene(trace);
        for index in arrival {
            m.enqueue_command(
                OrderKey {
                    tick: 1,
                    issuer: 0,
                    sequence: index as u64 + 1,
                },
                Command::from_json(&frames[index]["command"]).unwrap(),
            )
            .unwrap();
        }
        let results = m.apply_commands(1, false).unwrap();
        assert_eq!(results.len(), 4);
        assert!(results.iter().all(|r| r.denial.is_none()));
        assert_eq!(
            results.iter().map(|r| r.key.sequence).collect::<Vec<_>>(),
            [1, 2, 3, 4]
        );
        assert_json(
            &command_snapshot(&m),
            &frames[3]["expected"],
            "boundary replay",
        );
        assert_eq!(m.commands.pending_commands(), 0);
        assert_eq!(m.commands.path_requests().used(), 0);
    }
}

fn aircraft_tick_fixture() -> Value {
    let mut text = String::new();
    flate2::read::GzDecoder::new(&include_bytes!("fixtures/match-aircraft-tick.json.gz")[..])
        .read_to_string(&mut text)
        .unwrap();
    serde_json::from_str(&text).unwrap()
}
#[test]
fn aircraft_tick_hooks_match_observed_original_full_step_phases() {
    use ww2_core::{match_commands::Command, match_story::DeathRecord, orders::OrderKey};
    let f = aircraft_tick_fixture();
    let mut steps = 0;
    let mut calls = 0;
    for trace in f["traces"].as_array().unwrap() {
        let mut m = scene(trace);
        assert_json(
            &aircraft_phase_snapshot(&m),
            &trace["initial"],
            "full-step initial",
        );
        for (index, frame) in trace["frames"].as_array().unwrap().iter().enumerate() {
            let tick = frame["fullStep"]["tick"].as_u64().unwrap();
            assert_eq!(tick, index as u64 + 1);
            let dt = frame["phases"][0]["dt"].as_f64().unwrap();
            m.aviation.take_events();
            // Replay real held inputs on the sole queue. This consumer owns no production clock.
            for (sequence, input) in frame["inputs"].as_array().unwrap().iter().enumerate() {
                m.enqueue_command(
                    OrderKey {
                        tick,
                        issuer: input["owner"].as_u64().unwrap() as u32,
                        sequence: sequence as u64 + 1,
                    },
                    Command::from_json(&input["command"]).unwrap(),
                )
                .unwrap();
            }
            assert!(
                m.apply_commands(tick, false)
                    .unwrap()
                    .iter()
                    .all(|r| r.denial.is_none())
            );
            let ids = m.world.game.units.keys().copied().collect::<Vec<_>>();
            m.validate_aircraft_tick_phases(&ids, dt, tick).unwrap();
            m.grid.rebuild(&m.world.game.units);
            m.capture_firing_prior().unwrap();
            m.world.game.tick = tick as i64;
            for phase in frame["phases"].as_array().unwrap() {
                // Inputs from the already-observed vision/setup owners, never aircraft/AA results.
                for (player, visible) in m
                    .world
                    .game
                    .players
                    .iter_mut()
                    .zip(phase["visible"].as_array().unwrap())
                {
                    player.visible = visible
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|id| id.as_u64().unwrap() as u32)
                        .collect();
                }
                for row in phase["groundInputs"].as_array().unwrap() {
                    let u = m
                        .world
                        .game
                        .units
                        .get_mut(&(row["id"].as_u64().unwrap() as u32))
                        .unwrap();
                    u.still = row["still"].as_f64();
                    u.built = row["built"].as_f64();
                }
                let path = format!("{} tick {tick} {}", trace["name"], phase["op"]);
                assert_json(
                    &aircraft_phase_snapshot(&m),
                    &phase["before"],
                    &format!("{path} before"),
                );
                let actors = phase["actors"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|id| id.as_u64().unwrap() as u32)
                    .collect::<Vec<_>>();
                let dt = phase["dt"].as_f64().unwrap();
                match phase["op"].as_str().unwrap() {
                    "plane" => {
                        let id = actors[0];
                        let u = &m.world.game.units[&id];
                        let general = [u.supp.to_bits(), u.repath.to_bits(), u.sprint.to_bits()];
                        assert!(m.step_aircraft_turn(id, dt).unwrap());
                        let u = &m.world.game.units[&id];
                        assert_eq!(
                            general,
                            [u.supp.to_bits(), u.repath.to_bits(), u.sprint.to_bits()],
                            "AIR must skip ordinary timers"
                        );
                    }
                    "aa" => m.step_aircraft_anti_air(&actors, dt).unwrap(),
                    _ => panic!("unknown aircraft phase"),
                }
                assert_json(
                    &aircraft_phase_snapshot(&m),
                    &phase["after"],
                    &format!("{path} after"),
                );
                calls += 1;
            }
            // Actual canonical removal provider, with its single returned story consumer.
            let deaths = m.retire_dead(&ids).unwrap();
            for death in deaths.retired {
                let kind = m.rules.kinds[death.unit.kind].name.clone();
                m.record_death(
                    DeathRecord {
                        owner: Some(death.unit.owner),
                        kind: &kind,
                        at: death.unit.at(),
                    },
                    death.enemy_killer,
                )
                .unwrap();
            }
            assert_json(
                &aircraft_phase_snapshot(&m),
                &frame["after"],
                "full-step owned final",
            );
            assert_eq!(m.commands.pending_commands(), 0);
            assert_eq!(m.commands.path_requests().used(), 0);
            steps += 1;
        }
    }
    assert_eq!(steps, f["steps"].as_u64().unwrap());
    assert_eq!(calls, f["phaseCalls"].as_u64().unwrap());
}

#[test]
fn aircraft_tick_preflight_admits_weapons_and_queue_but_rejects_malformed_state() {
    let f = aircraft_tick_fixture();
    let mut m = scene(&f["traces"][0]);
    let before = aircraft_phase_snapshot(&m);
    m.validate_aircraft_turns(0.05).unwrap();
    m.validate_aircraft_anti_air(&[1000], 0.05).unwrap();
    m.validate_aircraft_tick_phases(&[1000], 0.05, 1).unwrap();
    assert!(m.validate_aircraft_tick_phases(&[1000], 0.05, 2).is_err());
    assert_json(&aircraft_phase_snapshot(&m), &before, "pure admission");
    assert!(m.validate_aircraft_turns(f64::NAN).is_err());
    assert!(m.validate_aircraft_anti_air(&[1000, 1000], 0.05).is_err());
    m.world.game.units.get_mut(&1000).unwrap().air = Some(PlaneState::Out);
    m.aviation.planes.get_mut(&1000).unwrap().mission =
        Some(mission(&json!({"kind":"patrol","id":0,"x":40,"z":40})));
    m.commands.units.get_mut(&1000).unwrap().hold_fire = false;
    let before = aircraft_phase_snapshot(&m);
    m.validate_aircraft_turns(0.05).unwrap();
    assert_json(
        &aircraft_phase_snapshot(&m),
        &before,
        "automatic weapon admission is read-only",
    );
    assert!(m.step_aircraft_turn(1000, 0.05).unwrap());
    assert!(!m.commands.units[&1000].hold_fire);
    m.commands.units.get_mut(&1000).unwrap().hold_fire = true;
    m.commands.units.get_mut(&1000).unwrap().orders.push_back(
        ww2_core::match_commands::Command::from_json(&json!({"t":"move","orders":[[1000,30,30]]}))
            .unwrap(),
    );
    let before = aircraft_phase_snapshot(&m);
    m.validate_aircraft_turns(0.05).unwrap();
    assert_json(
        &aircraft_phase_snapshot(&m),
        &before,
        "queued order admission",
    );
    m.world.game.units.get_mut(&1000).unwrap().hp = 0.0;
    let dead = aircraft_phase_snapshot(&m);
    m.validate_aircraft_turns(0.05).unwrap();
    assert!(m.step_aircraft_turn(1000, 0.05).unwrap());
    assert_json(&aircraft_phase_snapshot(&m), &dead, "dead AIR skipped turn");
    m.world.game.units.get_mut(&1000).unwrap().hp = 200.0;
    m.commands.units.get_mut(&1000).unwrap().orders.clear();
    m.world.game.units.get_mut(&1000).unwrap().x = 1_073_741_825.0;
    assert!(m.validate_aircraft_turns(0.05).is_err());
    m.world.game.units.get_mut(&1000).unwrap().x = 40.0;
    m.aviation.planes.remove(&1000);
    assert!(m.validate_aircraft_turns(0.05).is_err());
}

#[test]
fn cmb65_mg_flak_and_fighter_hold_then_resume_permitted_anti_air() {
    use ww2_core::{match_aviation::MissionKind, match_commands::Command};
    let fixture = fixture();
    for kind in ["mg", "flak", "fighter"] {
        let name = if kind == "fighter" {
            "held-fighter-attack-air-target-loss".to_owned()
        } else {
            format!("{kind}-continuous-aa")
        };
        let trace = fixture["traces"]
            .as_array()
            .unwrap()
            .iter()
            .find(|trace| trace["name"] == name)
            .unwrap();
        let mut m = scene(trace);
        assert_json(
            &snapshot(&m, &[]),
            &trace["initial"],
            "complete captured initial state",
        );
        let shooter = 1000;
        let target = 1001;
        let hold =
            Command::from_json(&json!({"t":"stance","ids":[shooter],"key":"holdFire","on":true}))
                .unwrap();
        assert_eq!(m.execute_command(0, &hold, false).unwrap(), None);
        if kind == "fighter" {
            assert_eq!(m.world.game.units[&shooter].air, Some(PlaneState::Base));
            assert!(m.aviation.planes[&shooter].ammo > 0.0);
            assert!(m.aviation.planes[&shooter].fuel > 0.0);
            assert!(m.world.game.players[0].sees(target));
            let at = m.world.game.units[&target].at();
            let patrol =
                Command::from_json(&json!({"t":"move","orders":[[shooter,at.x,at.z]]})).unwrap();
            assert_eq!(m.execute_command(0, &patrol, false).unwrap(), None);
            let sent = m.aviation.planes[&shooter].mission.unwrap();
            assert_eq!(sent.kind, MissionKind::Patrol);
            assert_eq!(sent.target, None);
            assert_eq!(m.world.game.units[&shooter].air, Some(PlaneState::Out));
            m.step_aircraft(shooter, 0.05).unwrap();
            assert!(matches!(
                m.world.game.units[&shooter].air,
                Some(PlaneState::Out | PlaneState::Station)
            ));
            assert!(m.commands.units[&shooter].hold_fire);
            let caster = &m.world.game.units[&shooter];
            let victim = &m.world.game.units[&target];
            assert!(victim.hp > 0.0);
            assert!(matches!(
                victim.air,
                Some(PlaneState::Out | PlaneState::Station | PlaneState::Home)
            ));
            assert!(m.world.game.players[0].sees(target));
            assert!(ww2_core::math::hypot(caster.x - victim.x, caster.z - victim.z) < 30.0);
        }
        let hp = m.world.game.units[&target].hp;
        let xp = m.combat.get(shooter).unwrap().xp;
        for _ in 0..6 {
            m.step_aircraft_anti_air(&[shooter], 0.05).unwrap();
            assert_eq!(m.world.game.units[&target].hp.to_bits(), hp.to_bits());
            assert_eq!(m.combat.get(shooter).unwrap().xp.to_bits(), xp.to_bits());
            assert!(m.aviation.take_events().is_empty());
        }
        if kind == "fighter" {
            let ammo = m.aviation.planes[&shooter].ammo;
            let attack =
                Command::from_json(&json!({"t":"attack","ids":[shooter],"target":target})).unwrap();
            assert_eq!(m.execute_command(0, &attack, false).unwrap(), None);
            let admitted = m.aviation.planes[&shooter].mission.unwrap();
            assert_eq!(admitted.kind, MissionKind::Attack);
            assert_eq!(admitted.target, Some(target));
            assert_eq!(m.world.game.units[&shooter].air, Some(PlaneState::Out));
            assert_eq!(m.aviation.planes[&shooter].ammo.to_bits(), ammo.to_bits());
            assert!(m.commands.units[&shooter].hold_fire);
        } else {
            let fire = Command::from_json(
                &json!({"t":"stance","ids":[shooter],"key":"holdFire","on":false}),
            )
            .unwrap();
            assert_eq!(m.execute_command(0, &fire, false).unwrap(), None);
        }
        for _ in 0..6 {
            m.step_aircraft_anti_air(&[shooter], 0.05).unwrap();
        }
        assert!(m.world.game.units[&target].hp < hp, "{kind} resumes AA");
        assert!(m.combat.get(shooter).unwrap().xp > xp, "{kind} earns XP");
        assert!(
            m.aviation.take_events().iter().any(|event| matches!(
                event,
                ww2_core::match_aviation::AviationEvent::AntiAir { .. }
            )),
            "{kind} emits the actual AA event"
        );
    }
}
