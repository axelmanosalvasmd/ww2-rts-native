use super::*;

fn public_map() -> PublicMap {
    PublicMap {
        width: 40,
        height: 40,
        players: vec![
            json!({"name":"Own","team":0,"faction":0,"spawn":{"x":10,"z":10}}),
            json!({"name":"Enemy","team":1,"faction":1,"spawn":{"x":70,"z":70}}),
            json!({"name":"Ally","team":0,"faction":2,"spawn":{"x":10,"z":70}}),
        ],
        points: vec![json!({"x":30,"z":40,"vp":2,"mp":3})],
    }
}
fn memory() -> SeatMemory {
    SeatMemory::new(0, public_map()).unwrap()
}
fn unit(uid: u32, kind: &str, owner: usize, x: f64, flags: i64) -> Value {
    json!([
        uid, kind, owner, x, 20.3, 0.4, 0.5, 73, 12, 0, 2, 0.7, flags, 1, 0.8
    ])
}
fn snapshot() -> Value {
    json!({"tick":20,"units":[],"points":[[1,0,0.4,1,0,1]],
        "out":[false,false,false],"vp":[1,2,3],"mp":123,"mun":45.6,"fuel":12.3,
        "tech":{"tier":2},"inc":4.3,"fuelInc":1.2,"upkeep":2.4,"sup":{"smoke":3},"rally":[10.1,20.2],
        "nodes":[],"cells":[],"foundMines":[],"smokes":[],"strikes":[],"covers":[],"ghosts":[],
        "plans":[],"queues":[],"productionJobs":[],"air":[],"orders":[],"movement":[]})
}

// Expected values come from the fifteen-field destructure and flag reads at
// reference e4e579a shared/ai-view.js:9-21. No authoritative unit is consulted.
#[test]
fn intent_ai02_fifteen_fields_flags_precision_and_foreign_air_are_delivered_only() {
    let mut snap = snapshot();
    let all = 1 | 2 | 4 | 8 | 16 | 32 | 128 | 512 | 1024 | 2048 | 4096 | 8192 | 16384;
    snap["units"] = json!([
        unit(1, "rifle", 0, 12.1, all),
        unit(2, "fighter", 1, 13.2, all),
        unit(3, "fighter", 2, 14.3, 0)
    ]);
    let view = memory().decode(&snap).unwrap();
    let u = &view.units[&1];
    assert_eq!(
        u,
        &json!({"id":1,"type":"rifle","owner":0,"x":12.1,"z":20.3,"rot":0.4,"aim":0.5,
        "hp":73,"supp":12,"targetId":0,"cover":2,"cd":0.7,"flags":all,"veterancy":1,"built":0.8,
        "path":[],"orders":[],"attackId":0,"retreating":true,"buff":1,"ap":true,"reinf":1,
        "dig":{},"garrison":0,"amove":null,"build":1,"enter":-1,"fireAt":-1,"nade":null,"entrench":{},
        "holdFire":true,"holdPos":true,"autoRetreat":true,"auto":true})
    );
    assert_eq!(view.units[&2]["air"], json!({"state":"base"}));
    assert_eq!(view.units[&3]["air"], json!({"state":"out"}));
    for uid in [2, 3] {
        for key in ["holdFire", "holdPos", "autoRetreat", "auto"] {
            assert_eq!(view.units[&uid][key], false);
        }
        assert_eq!(view.units[&uid]["cd"], 0);
    }
    snap["units"][0][3] = json!(99.9);
    assert_eq!(view.units[&1]["x"], 12.1, "old decoded frame is detached");
}

#[test]
fn intent_ai03_private_plans_queues_jobs_air_orders_economy_and_logistics_stay_own() {
    let mut snap = snapshot();
    snap["units"] = json!([
        unit(1, "fighter", 0, 12.1, 512),
        unit(2, "rifle", 1, 20.0, 0),
        unit(3, "barracks", 0, 15.0, 0)
    ]);
    snap["plans"] = json!([[1, 4, 30.1, 40.2, 20.1, 21.2], [2, 5, 55, 55]]);
    snap["queues"] = json!([
        [3, 0.4, 12.3, 14.5, "rifle", "mg"],
        [2, 0.9, 90, 90, "tank"]
    ]);
    snap["productionJobs"] =
        json!([[3,[{"id":7,"type":"rifle","mp":50,"fuel":0,"status":"active"}]]]);
    snap["air"] = json!([[1, 3, 77.7, 2, 3.4]]);
    snap["orders"] = json!([[1, 2, 1, 20.1, 21.2, 4, 30.1, 40.2], [2, 1, 1, 70, 70]]);
    snap["movement"] = json!([[1, "blocked", 19, "unreachable", 3], [2, "arrived", 18]]);
    snap["logistics"] = json!({"enabled":true,"units":[{"id":1,"ammo":0.52},{"id":2,"ammo":0.99}],"trucks":[{"id":2},{"id":1}],"links":[1]});
    let view = memory().decode(&snap).unwrap();
    let u = &view.units[&1];
    assert_eq!(u["path"], json!([{"x":20.1,"z":21.2}]));
    assert_eq!(u["orderPlan"], json!({"kind":4,"x":30.1,"z":40.2}));
    assert_eq!(
        u["attackId"], 1,
        "source fallback for a delivered attack with no target id"
    );
    assert_eq!(
        u["air"],
        json!({"state":"home","mission":{"kind":"attack","x":30.1,"z":40.2},"fuel":77.7,"ammo":2,"timer":3.4})
    );
    assert_eq!(
        u["orders"],
        json!([{"kind":1,"x":20.1,"z":21.2},{"kind":4,"x":30.1,"z":40.2}])
    );
    assert_eq!(u["moveOutcome"], "blocked");
    assert_eq!(u["moveOutcomeTick"], 19);
    assert_eq!(u["logistics"], json!({"id":1,"ammo":0.52}));
    assert_eq!(view.units[&3]["queue"], json!(["rifle", "mg"]));
    assert_eq!(view.units[&3]["prog"], 0.4);
    assert_eq!(view.units[&3]["rally"], json!({"x":12.3,"z":14.5}));
    assert_eq!(
        view.units[&3]["productionJobs"],
        snap["productionJobs"][0][1]
    );
    assert_eq!(view.units[&2]["path"], json!([]));
    assert_eq!(view.units[&2]["orders"], json!([]));
    for key in ["queue", "moveOutcome", "logistics"] {
        assert!(view.units[&2].get(key).is_none());
    }
    assert_eq!(view.players[0]["mp"], 123);
    assert_eq!(view.players[0]["sup"], json!({"smoke":3}));
    for key in [
        "mp", "mun", "fuel", "tech", "inc", "fuelInc", "upkeep", "sup", "rally",
    ] {
        assert_eq!(
            view.players[0][key],
            if key == "rally" {
                json!({"x":10.1,"z":20.2})
            } else {
                snap[key].clone()
            }
        );
        assert!(view.players[1].get(key).is_none());
    }
    assert_eq!(view.logistics["units"], json!([{"id":1,"ammo":0.52}]));
    assert_eq!(view.logistics["trucks"], json!([{"id":1}]));
}

#[test]
fn intent_ai03_each_plan_kind_and_null_rally_decode_without_invented_private_state() {
    let cases = [
        (1, "worldGoal", json!({"x":10.2,"z":12.4})),
        (2, "amove", json!({"x":10.2,"z":12.4})),
        (5, "fireAt", json!(245)),
        (6, "nade", json!({"x":10.2,"z":12.4})),
        (7, "dig", json!({"x":10.2,"z":12.4})),
        (8, "build", json!(1)),
        (9, "enter", json!(245)),
    ];
    for (kind, key, expected) in cases {
        let mut snap = snapshot();
        snap["units"] = json!([unit(1, "rifle", 0, 12.0, 16)]);
        snap["plans"] = json!([[1, kind, 10.2, 12.4]]);
        snap["queues"] = json!([[1, 0, -1, -1]]);
        let view = memory().decode(&snap).unwrap();
        assert_eq!(view.units[&1][key], expected, "kind {kind}");
        assert_eq!(view.units[&1]["rally"], Value::Null);
    }
    let mut snap = snapshot();
    snap["units"] = json!([unit(1, "rifle", 0, 12.0, 1024)]);
    snap["plans"] = json!([[1, 7, 10.2, 12.4]]);
    assert_eq!(
        memory().decode(&snap).unwrap().units[&1]["dig"],
        Value::Null,
        "an entrenchment segment is not a dig job"
    );
}

#[test]
fn intent_ai06_objectives_decode_delivered_capture_cut_lock_and_world_team_mapping() {
    let mut snap = snapshot();
    let ordinary = memory().decode(&snap).unwrap();
    assert_eq!(ordinary.points,json!([{"x":30,"z":40,"vp":2,"mp":3,"owner":1,"capper":0,"progress":0.4,"contested":true,"cut":false,"locked":true}]).as_array().unwrap().clone());
    snap["mode"] = json!({"kind":"world"});
    snap["home"] = json!([11, 13]);
    snap["world"] = json!({"regions":[{"id":"r0","x":21,"z":22,"team":0,"capper":1,"progress":0.8,"contested":1,"locked":false},
        {"id":"neutral","x":31,"z":32,"team":-1,"capper":-1,"progress":0,"contested":0,"locked":true}]});
    let world = memory().decode(&snap).unwrap();
    assert_eq!(world.points,json!([{"id":"r0","x":21,"z":22,"vp":1,"mp":1,"owner":0,"capper":1,"progress":0.8,"contested":true,"locked":false,"cut":false},
        {"id":"neutral","x":31,"z":32,"vp":1,"mp":1,"owner":-1,"capper":-1,"progress":0,"contested":false,"locked":true,"cut":false}]).as_array().unwrap().clone());
    assert_eq!(world.players[0]["spawn"], json!({"x":11.0,"z":13.0}));
    assert_eq!(world.players[1]["spawn"], Value::Null);
    assert_eq!(world.players[2]["spawn"], json!({"x":10,"z":70}));
}

#[test]
fn intent_ai07_effects_weather_nodes_and_mines_use_only_delivered_rows() {
    let mut snap = snapshot();
    snap["weather"] = json!(["rain", "clear", 17.2]);
    snap["mode"] = json!({"kind":"assault","defender":1});
    snap["army"] = json!({"pop":2.5,"income":3});
    snap["nodes"] = json!([[10, 20, 2.5, 1], [30, 40, 1, 0]]);
    snap["smokes"] = json!([[1.1, 2.2, 3, 91]]);
    snap["strikes"] = json!([["bombing", 4.4, 5.5, 0.3, 0, 1], ["smoke", 6, 7, 0, 0.1, 0]]);
    snap["covers"] = json!([[8.1, 9.2, 10, 11]]);
    snap["cells"] = json!([[81,"N",0,0,{"mineOwned":true}],[82,"N",0,0,{"mineOwned":false}],[83,"N",0,0,{"mineOwned":true}]]);
    snap["foundMines"] = json!([82, 83]);
    let mut memory = memory();
    let view = memory.decode(&snap).unwrap();
    assert_eq!(
        view.weather,
        json!({"now":"rain","next":"clear","left":17.2})
    );
    assert_eq!(view.mode, snap["mode"]);
    assert_eq!(view.army, snap["army"]);
    assert_eq!(
        view.nodes,
        json!([{"x":10,"z":20,"rate":2.5,"fuel":true},{"x":30,"z":40,"rate":1,"fuel":false}])
            .as_array()
            .unwrap()
            .clone()
    );
    assert_eq!(
        view.smokes,
        json!([{"x":1.1,"z":2.2,"r":3}]).as_array().unwrap().clone()
    );
    assert_eq!(
        view.strikes,
        json!([{"kind":"bombing","x":4.4,"z":5.5,"dir":0.3,"t":0,"owner":1,"live":true},
        {"kind":"smoke","x":6,"z":7,"dir":0,"t":0.1,"owner":0,"live":false}])
        .as_array()
        .unwrap()
        .clone()
    );
    assert_eq!(
        view.covers,
        json!([{"x":8.1,"z":9.2,"r":10,"t":11}])
            .as_array()
            .unwrap()
            .clone()
    );
    assert_eq!(view.mines, vec![json!({"x":3.0,"z":5.0})]);
    assert_eq!(
        view.found_mines,
        vec![json!({"x":5.0,"z":5.0}), json!({"x":7.0,"z":5.0})]
    );
    snap["tick"] = json!(22);
    snap["cells"] = json!([[81, ".", 0, 0, {}]]);
    snap["foundMines"] = json!([]);
    let next = memory.decode(&snap).unwrap();
    assert_eq!(next.mines, vec![json!({"x":7.0,"z":5.0})]);
    assert_eq!(
        view.mines,
        vec![json!({"x":3.0,"z":5.0})],
        "later deliveries cannot mutate old views"
    );
}

#[test]
fn intent_ai09_only_known_depots_claim_nodes_with_last_seen_time_and_strict_eight_metres() {
    let mut snap = snapshot();
    snap["nodes"] = json!([
        [10, 20.3, 1, 0],
        [40, 20.3, 1, 0],
        [60, 20.3, 1, 0],
        [70, 20.3, 1, 0]
    ]);
    snap["units"] = json!([
        unit(1, "depot", 2, 10.0, 0),
        unit(2, "barracks", 0, 40.0, 0),
        unit(3, "depot", 1, 67.9, 0)
    ]);
    snap["ghosts"] = json!([[4, "depot", 1, 40, 20.3, 1], [3, "depot", 1, 75, 20.3, 1]]);
    let mut memory = memory();
    let seen = memory.decode(&snap).unwrap();
    assert_eq!(seen.claimed_nodes, BTreeSet::from([0, 1, 2, 3]));
    let b = seen.known_buildings.iter().find(|b| b["id"] == 3).unwrap();
    assert_eq!(b["x"], 67.9);
    assert_eq!(b["t"], 1.0);
    assert_eq!(
        seen.known_buildings.iter().find(|b| b["id"] == 4).unwrap()["t"],
        Value::Null,
        "human handover ghost has unknown last-seen time"
    );
    snap["tick"] = json!(40);
    snap["units"] = json!([unit(2, "barracks", 0, 40.0, 0)]);
    snap["ghosts"] = json!([[3, "depot", 1, 68, 20.3, 1]]);
    let hidden = memory.decode(&snap).unwrap();
    assert_eq!(
        hidden.claimed_nodes,
        BTreeSet::from([3]),
        "exactly eight metres does not claim a node; barracks never claims"
    );
    assert_eq!(hidden.known_buildings[0]["t"], 1.0);
    snap["tick"] = json!(60);
    snap["ghosts"] = json!([]);
    assert!(
        memory.decode(&snap).unwrap().claimed_nodes.is_empty(),
        "undelivered enemy depots do not claim nodes"
    );
    snap["tick"] = json!(80);
    snap["ghosts"] = json!([[3, "depot", 1, 68, 20.3, 1]]);
    assert_eq!(
        memory.decode(&snap).unwrap().known_buildings[0]["t"],
        Value::Null,
        "cleared ghosts also clear timestamps"
    );
}

#[test]
fn invalid_delivery_does_not_publish_new_mine_or_building_memory() {
    let mut memory = memory();
    let mut snap = snapshot();
    memory.decode(&snap).unwrap();
    let saved = memory.clone();
    snap["cells"] = json!([[81,"N",0,0,{"mineOwned":true}]]);
    snap["ghosts"] = json!([[0, "depot", 1, 20, 20, 1]]);
    assert!(memory.decode(&snap).is_err());
    assert_eq!(memory, saved);
    snap["ghosts"] = json!([]);
    snap["tick"] = json!(19);
    assert!(memory.decode(&snap).is_err());
    assert_eq!(memory, saved);
}

#[test]
fn ordinary_delivery_and_new_ai_memory_survive_a_full_match_and_final_snapshots() {
    use crate::{
        economy::{Faction, Mode},
        economy_state::ArmySize,
        map::AuthoritativeMap,
        match_runtime::{Match, MatchOptions, PlayerConfig},
        recipient_snapshot::DeliveryState,
        weather::WeatherChoice,
    };
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"AI delivered full match","w":64,"h":40,"rows":vec![".".repeat(64);40],
            "heights":vec!["0".repeat(64);40],"spawns":[{"x":8,"y":10},{"x":56,"y":32}],
            "points":[{"x":8,"y":10,"vp":5,"mp":0}]
        }))
        .unwrap(),
    )
    .unwrap();
    let create = || {
        Match::new(
            &map,
            MatchOptions {
                seed: 713,
                mode: Mode::Conquest,
                players: vec![
                    PlayerConfig {
                        name: "A".into(),
                        team: 0,
                        faction: Faction::Usa,
                    },
                    PlayerConfig {
                        name: "B".into(),
                        team: 1,
                        faction: Faction::Germany,
                    },
                ],
                shuffle: false,
                army: ArmySize::Standard,
                weather: WeatherChoice::Map,
                weather_seed: Some(1),
                living_weather: false,
                map_key: None,
                tech: true,
                supply: false,
                logistics: false,
                sandbox: false,
            },
        )
        .unwrap()
    };
    let mut game = create();
    game.world.game.reveal = true;
    // Activate both new persistent memories using canonical scene state.
    let mine = 13 * game.world.game.w + 13;
    game.world.set_cell(mine, b'N').unwrap();
    game.world.mines.insert(mine, 0);
    let buy = crate::match_commands::Command::Buy {
        kind: "rifle".into(),
        from: None,
    };
    assert_eq!(game.execute_command(0, &buy, false), Ok(None));
    let actor = game
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 0 && !game.rules.kinds[u.kind].structure)
        .unwrap();
    let raw = json!({"t":"move","orders":[[actor.id,actor.x + 2.0,actor.z]]});
    let movement = crate::match_commands::Command::from_json(&raw).unwrap();
    assert_eq!(game.execute_command(0, &movement, false), Ok(None));
    let other = create();
    let mut wrong = other.ordinary_ai_view_memory(0).unwrap();
    let mut delivery = DeliveryState::new(&game).unwrap();
    assert!(
        game.ordinary_ai_view(&mut wrong, &mut delivery, true)
            .is_err()
    );
    let mut memories = [
        game.ordinary_ai_view_memory(0).unwrap(),
        game.ordinary_ai_view_memory(1).unwrap(),
    ];
    let random_before = game.start.random.snapshot();
    let first = game
        .ordinary_ai_view(&mut memories[0], &mut delivery, true)
        .unwrap();
    assert_eq!(
        game.start.random.snapshot(),
        random_before,
        "observing cannot consume simulation randomness"
    );
    assert!(!first.units.is_empty());
    assert!(!first.mines.is_empty());
    assert!(!first.known_buildings.is_empty());
    // A short authored victory target keeps this a full match with a real capture and VP finish.
    game.start.win_vp = 20.0;
    let mut final_tick = None;
    let mut deliveries = 0;
    let mut move_result_seen = false;
    for _ in 0..2000 {
        game.advance_tick()
            .expect("ordinary tick must not fault with AI consumer memory");
        move_result_seen |= game
            .commands
            .units
            .values()
            .any(|c| c.move_result.is_some());
        if game.world.game.tick % 2 == 0 || game.finish.is_some() {
            for memory in &mut memories {
                let view = game
                    .ordinary_ai_view(memory, &mut delivery, false)
                    .expect("recipient snapshot and AI decode must succeed");
                assert_eq!(view.tick, game.world.game.tick);
                assert!(!view.known_buildings.is_empty());
                if memory.slot == 0 {
                    assert!(!view.mines.is_empty());
                }
                assert_eq!(
                    view.players[memory.slot]["out"],
                    game.start.economy.players[memory.slot].out
                );
                deliveries += 1;
            }
        }
        if game.finish.is_some() {
            let end = *final_tick.get_or_insert(game.world.game.tick);
            if game.world.game.tick >= end + 8 {
                break;
            }
        }
    }
    let end = game.finish.expect("ordinary match must reach a VP winner");
    assert_eq!(end.winner, 0);
    assert!(deliveries > 100);
    assert!(
        move_result_seen,
        "the ordinary match must exercise extended movement rows"
    );
    assert!(game.world.game.tick >= final_tick.unwrap() + 8);
    assert!(game.tick_state.fault().is_none());
    assert_eq!(
        first.tick, 0,
        "later deliveries leave earlier decoded frames detached"
    );
    let human = delivery
        .capture_with_lists(&mut game, &[true; 2], &[false; 2], &[])
        .unwrap();
    for slot in 0..2 {
        assert_eq!(human.recipient(slot).unwrap().into_json()["winner"], 0);
    }
}

#[test]
fn intent_ai02_vehicle_and_dig_row_tails_do_not_break_the_fifteen_field_decode() {
    let mut snap = snapshot();
    let mut vehicle = unit(1, "tank", 0, 12.1, 0);
    vehicle
        .as_array_mut()
        .unwrap()
        .extend([json!(2.3), json!(0.1), json!(0.2), json!(0.4)]);
    let mut digger = unit(2, "rifle", 0, 14.3, 16);
    digger.as_array_mut().unwrap().push(json!(0.32));
    snap["units"] = json!([vehicle, digger]);
    let view = memory().decode(&snap).unwrap();
    assert_eq!(view.units[&1]["type"], "tank");
    assert_eq!(view.units[&1]["x"], 12.1);
    assert_eq!(view.units[&2]["dig"], json!({}));
}
