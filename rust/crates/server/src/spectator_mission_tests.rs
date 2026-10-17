use super::*;
use serde_json::json;
use ww2_core::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    map::AuthoritativeMap,
    match_runtime::{MatchOptions, PlayerConfig},
    weather::WeatherChoice,
};

fn authored_map() -> Value {
    let mut actions = Vec::new();
    let mut objectives = Vec::new();
    for (id, policy) in [("public", "public"), ("side", "side"), ("team", "team")] {
        objectives.push(
            json!({"id":id,"text":{"en":format!("{id} objective")},"recipients":policy,"side":0}),
        );
        actions.push(json!({"kind":"objectiveActivate","objective":id}));
        actions.push(json!({"kind":"say","text":{"en":format!("{id} message"),"es":"aviso"},"recipients":policy,"side":0}));
    }
    json!({"name":"Spectator mission", "w":64,"h":64,"rows":vec![".".repeat(64);64],
    "spawns":[{"x":8,"y":8},{"x":8,"y":54},{"x":54,"y":54}],
    "points":[{"x":32,"y":32}],
    "scenario":{"version":1,"areas":[],"groups":[],"objectives":objectives,
        "triggers":[
            {"id":"intro","scope":"match","condition":{"kind":"time","seconds":0.1},"repeat":{"mode":"once"},"actions":actions},
            {"id":"done","scope":"match","condition":{"kind":"time","seconds":0.2},"actions":[{"kind":"objectiveComplete","objective":"public"}]}
        ]}})
}
fn state(raw: &Value) -> Match {
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(raw).unwrap()).unwrap();
    let mut state = Match::new(
        &map,
        MatchOptions {
            seed: 7301,
            mode: Mode::Conquest,
            players: (0..3)
                .map(|slot| PlayerConfig {
                    name: format!("P{slot}"),
                    team: if slot == 2 { 1 } else { 0 },
                    faction: Faction::try_from(slot as u8).unwrap(),
                })
                .collect(),
            shuffle: false,
            army: ArmySize::Standard,
            weather: WeatherChoice::Map,
            weather_seed: Some(1),
            living_weather: false,
            map_key: Some("river-pass".into()),
            tech: false,
            supply: false,
            logistics: false,
            sandbox: false,
        },
    )
    .unwrap();
    for control in state.commands.units.values_mut() {
        control.hold_fire = true;
        control.auto_cast = false;
        control.auto_retreat = false;
    }
    state
}
fn objective(id: &str, policy: &str, completed: bool) -> Value {
    json!({"id":id,"scope":"match","text":{"en":format!("{id} objective")},
        "state":if completed {"complete"} else {"active"},"recipients":policy,"side":0,
        "updated":if completed {0.2} else {0.1},"execution":if completed {"scenario:2"} else {"scenario:1"}})
}
fn say(policy: &str) -> Value {
    json!({"k":"say","text":format!("{policy} message"),"localized":{"en":format!("{policy} message"),"es":"aviso"},
        "scenarioRecipients":policy,"scenarioSide":0,"pub":policy=="public","execution":"scenario:1","atTick":2})
}

#[test]
fn intent_net77_authored_objectives_and_private_messages_filter_spectators_through_ticks_and_restore()
 {
    let raw = authored_map();
    let mut runtime = RoomRuntime::new(state(&raw), 0).unwrap();
    let settings = crate::native_startup::StartupSettings {
        map: "river-pass",
        mode: "conquest",
        army: "standard",
        weather: "map",
        sandbox: false,
    };
    let startup =
        crate::native_startup::NativeStartup::prepare_loaded(runtime.state(), settings, &raw)
            .unwrap();
    let names = vec!["P0".into(), "P1".into(), "P2".into()];
    let starts = runtime
        .collect_starts(&[0, 1, 2], &startup, &names, 1)
        .unwrap();
    let watcher_start = runtime.collect_watcher_start(&startup, &names, 1).unwrap();
    assert_eq!(watcher_start["you"], 0);
    assert_eq!(watcher_start["spawn"], starts[0].1["spawn"]);
    assert!(watcher_start.get("fog").is_none());
    assert!(watcher_start["map"].get("scenario").is_none());
    assert!(watcher_start["map"].get("triggers").is_none());
    let seats = (0..3)
        .map(|slot| SelectedSeat {
            slot,
            full: false,
            omit: false,
        })
        .collect::<Vec<_>>();
    let mut restored = None;
    for tick in 1..=8 {
        runtime.advance_tick(&[false; 3]).unwrap();
        // The shared watcher reads before the scheduled seat batch resets events.
        let spectator = runtime.collect_watcher_frame().unwrap();
        if tick == 2 {
            let saved = runtime.save_json().unwrap();
            let mut pending =
                RoomRuntime::new(Match::from_save_json(saved["state"].clone()).unwrap(), 100)
                    .unwrap();
            pending
                .restore_saved_events(saved["pendingEvents"].clone())
                .unwrap();
            assert_eq!(
                pending.collect_watcher_frame().unwrap()["shots"],
                json!([say("public")])
            );
            let replayed = pending.collect_frame(&seats, FrameKind::Scheduled).unwrap();
            assert_eq!(
                replayed[0].message["shots"],
                json!([say("public"), say("side"), say("team")])
            );
            assert_eq!(
                replayed[1].message["shots"],
                json!([say("public"), say("team")])
            );
            assert_eq!(replayed[2].message["shots"], json!([say("public")]));
        }
        let frames = runtime.collect_frame(&seats, FrameKind::Scheduled).unwrap();
        assert_eq!(frames.len(), 3);
        let complete = tick >= 4;
        let public = if tick >= 2 {
            vec![objective("public", "public", complete)]
        } else {
            vec![]
        };
        assert_eq!(
            spectator["scenario"],
            json!({"version":1,"objectives":public})
        );
        assert_eq!(
            spectator["shots"],
            if tick == 2 {
                json!([say("public")])
            } else {
                json!([])
            }
        );
        assert!(spectator.get("fog").is_none());
        for key in [
            "movement",
            "queues",
            "productionJobs",
            "plans",
            "orders",
            "air",
            "mp",
            "inc",
            "mun",
            "fuel",
            "fuelInc",
            "upkeep",
            "sup",
            "rally",
            "home",
            "works",
            "covers",
            "logistics",
        ] {
            assert!(spectator.get(key).is_none(), "private {key}");
        }
        for frame in &frames {
            assert_eq!(frame.message["tick"], tick);
            let mut expected = public.clone();
            if tick >= 2 {
                if frame.slot == 0 {
                    expected.push(objective("side", "side", false));
                }
                if frame.slot < 2 {
                    expected.push(objective("team", "team", false));
                }
            }
            assert_eq!(
                frame.message["scenario"],
                json!({"version":1,"objectives":expected}),
                "seat {}",
                frame.slot
            );
            let expected_shots = if tick == 2 {
                match frame.slot {
                    0 => json!([say("public"), say("side"), say("team")]),
                    1 => json!([say("public"), say("team")]),
                    _ => json!([say("public")]),
                }
            } else {
                json!([])
            };
            assert_eq!(
                frame.message["shots"], expected_shots,
                "seat {}",
                frame.slot
            );
        }
        assert!(runtime.fault().is_none());
        if let Some(other) = restored.as_mut() {
            let other: &mut RoomRuntime = other;
            other.advance_tick(&[false; 3]).unwrap();
            let watcher = other.collect_watcher_frame().unwrap();
            let seats = other.collect_frame(&seats, FrameKind::Scheduled).unwrap();
            for (left, right) in frames.iter().zip(seats) {
                assert_eq!(left.message["scenario"], right.message["scenario"]);
                assert_eq!(left.message["shots"], right.message["shots"]);
            }
            assert_eq!(watcher["scenario"], spectator["scenario"]);
            assert_eq!(watcher["shots"], spectator["shots"]);
            assert!(other.fault().is_none());
        }
        if tick == 3 {
            let loaded = Match::from_save_json(runtime.state().save_json().unwrap()).unwrap();
            assert_eq!(loaded.authored_scenario, runtime.state().authored_scenario);
            restored = Some(RoomRuntime::new(loaded, 150).unwrap());
        }
    }
}

#[test]
fn authored_mission_admission_refuses_unimplemented_actions_and_changed_map_bindings() {
    let raw = authored_map();
    let original = state(&raw);
    // Reject a valid mission history attached to a later unfinished match clock.
    let mut stale = state(&raw);
    stale.world.game.tick = 1;
    assert!(stale.save_json().is_err());
    // A null authored definition retains migrated legacy timed triggers.
    let mut legacy = raw.clone();
    legacy["scenario"] = Value::Null;
    legacy["triggers"] = json!([{"at":0,"say":"Legacy public"}]);
    let legacy = state(&legacy);
    assert!(legacy.scenario.is_some());
    assert!(legacy.authored_scenario.is_none());
    let mut unsupported = raw.clone();
    unsupported["scenario"]["triggers"][0]["actions"] =
        json!([{"kind":"destroy","box":[10,10,11,11]}]);
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&unsupported).unwrap()).unwrap();
    assert!(Match::new(&map, original.options.clone()).is_err());
    let mut different = raw.clone();
    different["scenario"]["objectives"][0]["text"] = json!({"en":"Changed mission"});
    let settings = crate::native_startup::StartupSettings {
        map: "river-pass",
        mode: "conquest",
        army: "standard",
        weather: "map",
        sandbox: false,
    };
    assert!(
        crate::native_startup::NativeStartup::prepare_loaded(&original, settings, &different)
            .is_err()
    );
}
