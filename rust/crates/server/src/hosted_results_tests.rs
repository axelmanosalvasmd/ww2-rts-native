//! Hosted completion across records, recipient delivery and the full ending hold.
use super::*;
use crate::horde_records::Run;
use ww2_core::match_story::StoryKey;

fn ready(
    sandbox: bool,
    records: crate::HordeRecords,
) -> (Hub, Connection, Vec<mpsc::Receiver<Output>>, MapDir) {
    let (mut hub, a, ra, _, rb) = lobby();
    let dir = MapDir::new();
    dir.write(&arena());
    hub.configure_authored_maps(Some(dir.0.clone()));
    hub.configure_records(records);
    hub.rooms.get_mut("ABC").unwrap().sandbox = sandbox;
    hub.start_horde("ABC", a.id, false, 0).unwrap();
    let mut receivers = vec![ra, rb];
    for rx in &mut receivers {
        outputs(&mut hub, rx, 0);
    }
    (hub, a, receivers, dir)
}

fn mode(hub: &mut Hub) -> Vec<Value> {
    let native = hub.rooms.get_mut("ABC").unwrap().native.as_mut().unwrap();
    let selected = (0..3)
        .map(|slot| SelectedSeat {
            slot,
            full: false,
            omit: false,
        })
        .collect::<Vec<_>>();
    native
        .runtime
        .collect_frame(&selected, FrameKind::Current)
        .unwrap()
        .into_iter()
        .map(|v| v.message)
        .collect()
}

fn defeat(hub: &mut Hub) {
    let state = hub
        .rooms
        .get_mut("ABC")
        .unwrap()
        .native
        .as_mut()
        .unwrap()
        .runtime
        .state_mut();
    let bunker = state.horde.as_ref().unwrap().bunker;
    let target = state.world.game.units[&bunker].at();
    state.world.game.units.get_mut(&bunker).unwrap().hp = 1.0;
    for unit in state
        .world
        .game
        .units
        .values_mut()
        .filter(|u| !state.rules.kinds[u.kind].structure)
    {
        unit.x = 11.0;
        unit.z = 101.0;
        unit.path.clear();
        unit.amove = None;
    }
    let horde = state.horde.as_mut().unwrap();
    horde.wave = 9;
    horde.time_left = 0.0;
    horde.profile_wave = 0;
    horde.gates = vec![Point {
        x: target.x,
        z: target.z + 14.0,
    }];
    state
        .record_story_tally(Some(0), StoryKey::Kills, 11.0)
        .unwrap();
    state
        .record_story_tally(Some(1), StoryKey::Kills, 7.0)
        .unwrap();
    state.grid.rebuild(&state.world.game.units);
}

fn complete(hub: &mut Hub, a: &Connection, receivers: &mut [mpsc::Receiver<Output>]) -> Value {
    let mut finished = None;
    let mut last_tick = 0;
    let mut stopped_brain = None;
    let mut decision = None;
    let mut hold_frames = [0; 2];
    for callback in 0..600_u64 {
        let before = hub.native_status("ABC").unwrap();
        if let Some((tick, _)) = finished {
            let runtime = &hub.rooms["ABC"].native.as_ref().unwrap().runtime;
            assert_eq!(
                runtime.ai.as_ref().unwrap().commander(2),
                stopped_brain.as_ref()
            );
            let state = hub.rooms["ABC"].native.as_ref().unwrap().runtime.state();
            let pending = state.commands.pending_commands();
            command(hub, a, json!({"t":"stop","ids":[1]}), callback * 50);
            assert_eq!(
                hub.rooms["ABC"]
                    .native
                    .as_ref()
                    .unwrap()
                    .runtime
                    .state()
                    .commands
                    .pending_commands(),
                pending
            );
            assert_eq!(before.tick, tick + i64::from(before.held / 2));
        }
        hub.poll_native(callback * 50, callback * 50_000_000);
        let status = hub.native_status("ABC").unwrap();
        assert!(status.fault.is_none(), "{:?}", status.fault);
        let holding = finished.is_some();
        if holding {
            assert_eq!(status.held, before.held + 1);
            assert_eq!(
                status.tick - before.tick,
                i64::from(status.held.is_multiple_of(2))
            );
            assert_eq!(status.ending_complete, status.held == 120);
            assert_eq!(hub.rooms["ABC"].native.is_some(), status.held < 120);
        }
        for (slot, rx) in receivers.iter_mut().enumerate() {
            let sent = outputs(hub, rx, callback * 50);
            let frames = sent.iter().filter(|v| v["t"] == "s").collect::<Vec<_>>();
            if holding {
                assert_eq!(frames.len(), usize::from(status.held.is_multiple_of(2)));
                hold_frames[slot] += frames.len();
                for frame in &frames {
                    assert_eq!(frame["winner"], 1);
                    assert_eq!(frame["mode"]["kind"], "horde");
                    assert_eq!(frame["out"].as_array().unwrap().len(), 3);
                    assert_eq!(frame["end"]["reason"], "structures");
                }
                if status.ending_complete {
                    let lobby = sent.iter().find(|v| v["t"] == "lobby").unwrap();
                    assert_eq!(lobby["result"]["you"], json!({"team":0,"outcome":"defeat"}));
                } else {
                    assert!(!sent.iter().any(|v| v["t"] == "lobby"));
                }
            }
        }
        last_tick = status.tick;
        if status.ending_complete {
            break;
        }
        if finished.is_none()
            && hub.rooms["ABC"]
                .native
                .as_ref()
                .unwrap()
                .runtime
                .state()
                .finish
                .is_some()
        {
            finished = Some((status.tick, callback));
            decision = hub.rooms["ABC"]
                .native
                .as_ref()
                .unwrap()
                .runtime
                .state()
                .finish;
            stopped_brain = Some(
                hub.rooms["ABC"]
                    .native
                    .as_ref()
                    .unwrap()
                    .runtime
                    .ai
                    .as_ref()
                    .unwrap()
                    .commander(2)
                    .unwrap()
                    .clone(),
            );
        }
        assert_eq!(mode(hub).len(), 3);
    }
    assert_eq!(hold_frames, [60, 60]);
    let report = hub.rooms["ABC"].result.as_ref().unwrap().report.clone();
    assert_eq!(report["mode"], "horde");
    assert_eq!(report["winner"], 1);
    assert_eq!(report["teams"], json!([0, 0, 1]));
    assert_eq!(report["names"], json!(["a", "b", "Horde"]));
    assert_eq!(report["reason"], "structures");
    let decision = decision.expect("natural bunker defeat");
    assert_eq!(
        report["at"],
        json!({"x":decision.end_at.x,"z":decision.end_at.z})
    );
    assert_eq!(report["endTick"], decision.end_tick);
    assert_eq!(
        report.as_object().unwrap().len(),
        if report.get("horde").is_some() {
            11
        } else {
            10
        }
    );
    assert_eq!(report["winVp"], Value::Null);
    assert_eq!(report["story"].as_array().unwrap().len(), 3);
    assert!(
        report["timeline"]
            .as_array()
            .unwrap()
            .iter()
            .all(|v| v.get("points").is_some()
                && v.get("structures").is_some()
                && v.get("vp").is_none())
    );
    if report.get("horde").is_some() {
        let kills: u64 = report["story"].as_array().unwrap()[..2]
            .iter()
            .map(|v| v["kills"].as_f64().unwrap() as u64)
            .sum();
        assert_eq!(report["horde"]["wave"], 10);
        assert_eq!(report["horde"]["kills"], kills);
        assert_eq!(
            report["horde"]["time"],
            ww2_core::math::round(last_tick as f64 * 0.05) as u64
        );
        assert_eq!(report["horde"]["names"], json!(["a", "b"]));
    }
    report
}

#[test]
fn mode55_mode90_net94_hosted_completion_records_and_sandbox_exclusion() {
    let dir = MapDir::new();
    let records = crate::HordeRecords::new(dir.0.join("records.json"));
    let previous = Run {
        wave: 4,
        kills: 1,
        time: 999,
        names: vec!["Old".into()],
    };
    records.record("bastogne", 2, "standard", previous.clone(), false);
    let (mut hub, a, mut rx, _map) = ready(false, records.clone());
    defeat(&mut hub);
    let report = complete(&mut hub, &a, &mut rx);
    assert_eq!(report["horde"]["record"], true);
    assert_eq!(report["horde"]["best"], previous.json());
    assert_eq!(report["horde"].as_object().unwrap().len(), 6);
    let mut best = report["horde"].clone();
    best.as_object_mut().unwrap().remove("record");
    best.as_object_mut().unwrap().remove("best");
    assert_eq!(
        hub.rooms["ABC"].lobby(a.id, &hub.maps, "")["hordeBest"],
        best
    );
    assert_eq!(
        crate::HordeRecords::new(dir.0.join("records.json")).lookup("bastogne", 2, "standard"),
        Some(best.clone())
    );
    assert_eq!(records.lookup("bastogne", 1, "standard"), None);
    assert_eq!(records.lookup("bastogne", 2, "large"), None);
    assert_eq!(records.lookup("other", 2, "standard"), None);
    let disk = std::fs::read(dir.0.join("records.json")).unwrap();
    let (mut hub, a, mut rx, _map) = ready(true, records);
    defeat(&mut hub);
    let sandbox = complete(&mut hub, &a, &mut rx);
    assert!(sandbox.get("horde").is_none());
    assert_eq!(std::fs::read(dir.0.join("records.json")).unwrap(), disk);
}

#[test]
fn mode96_horde_exact_break_active_boss_and_later_break_rows() {
    let dir = MapDir::new();
    let (mut hub, _, _, _map) = ready(false, crate::HordeRecords::new(dir.0.join("records.json")));
    let expected = json!({"kind":"horde","defenderTeam":0,"attackerTeam":1,"timeLeft":45.0,"suddenDeath":false,
        "wave":0,"left":0.0,"active":false,"slot":2,"nextProfile":"mixed"});
    let mut expected = expected;
    expected["nextProfile"] = json!(
        hub.rooms["ABC"]
            .native
            .as_ref()
            .unwrap()
            .runtime
            .state()
            .horde
            .as_ref()
            .unwrap()
            .next_profile
            .as_str()
    );
    for frame in mode(&mut hub) {
        assert_eq!(frame["mode"], expected);
    }
    {
        let h = hub
            .rooms
            .get_mut("ABC")
            .unwrap()
            .native
            .as_mut()
            .unwrap()
            .runtime
            .state_mut()
            .horde
            .as_mut()
            .unwrap();
        h.wave = 10;
        h.active = true;
        h.time_left = -0.25;
        h.boss = Some([1235.0, 6000.0]);
    }
    expected["timeLeft"] = json!(0.0);
    expected["wave"] = json!(10);
    expected["active"] = json!(true);
    expected["boss"] = json!([1235.0, 6000.0]);
    expected["profile"] = json!(
        hub.rooms["ABC"]
            .native
            .as_ref()
            .unwrap()
            .runtime
            .state()
            .horde
            .as_ref()
            .unwrap()
            .profile
            .as_str()
    );
    expected.as_object_mut().unwrap().remove("nextProfile");
    for frame in mode(&mut hub) {
        assert_eq!(frame["mode"], expected);
    }
    {
        let h = hub
            .rooms
            .get_mut("ABC")
            .unwrap()
            .native
            .as_mut()
            .unwrap()
            .runtime
            .state_mut()
            .horde
            .as_mut()
            .unwrap();
        h.active = false;
        h.time_left = 2.01;
        h.boss = None;
    }
    expected["timeLeft"] = json!(3.0);
    expected["active"] = json!(false);
    expected["boss"] = Value::Null;
    expected["nextProfile"] = json!(
        hub.rooms["ABC"]
            .native
            .as_ref()
            .unwrap()
            .runtime
            .state()
            .horde
            .as_ref()
            .unwrap()
            .next_profile
            .as_str()
    );
    for frame in mode(&mut hub) {
        assert_eq!(frame["mode"], expected);
    }
}

#[test]
fn mode94_horde_editor_restart_refusal_is_atomic_and_success_uses_fresh_seeds() {
    let (mut hub, a, mut ra, _, mut rb) = lobby();
    let dir = MapDir::new();
    let editor = crate::MapEditor::open(dir.0.clone(), "test").unwrap();
    editor
        .save("private-horde", &serde_json::to_vec(&arena()).unwrap())
        .unwrap();
    hub.configure_maps(Some(editor.clone()));
    command(&mut hub, &a, json!({"t":"map","name":"private-horde"}), 0);
    hub.configure_trusted_room(Some(crate::TrustedRoomConfig::new("ABC", 701).unwrap()));
    hub.start_horde("ABC", a.id, false, 0).unwrap();
    let seed = hub.rooms["ABC"]
        .native
        .as_ref()
        .unwrap()
        .runtime
        .state()
        .options
        .seed;
    let weather = hub.rooms["ABC"]
        .native
        .as_ref()
        .unwrap()
        .runtime
        .state()
        .start
        .weather_seed;
    outputs(&mut hub, &mut ra, 0);
    outputs(&mut hub, &mut rb, 0);
    let before = identity(&hub);
    let mut changed = arena();
    changed.as_object_mut().unwrap().remove("defend");
    editor
        .save("private-horde", &serde_json::to_vec(&changed).unwrap())
        .unwrap();
    command(&mut hub, &a, json!({"t":"restart"}), 1);
    assert_eq!(identity(&hub), before);
    assert!(
        !outputs(&mut hub, &mut ra, 1)
            .iter()
            .any(|v| v["t"] == "start")
    );
    editor
        .save("private-horde", &serde_json::to_vec(&arena()).unwrap())
        .unwrap();
    hub.start_horde("ABC", a.id, true, 2).unwrap();
    let native = hub.rooms["ABC"].native.as_ref().unwrap();
    assert_ne!(native.runtime.state().options.seed, seed);
    assert_ne!(native.runtime.state().start.weather_seed, weather);
    assert_eq!(
        native.runtime.state().options.map_key.as_deref(),
        Some("private-horde")
    );
    for callback in 0..8 {
        hub.poll_native(callback * 50, callback * 50_000_000);
        outputs(&mut hub, &mut ra, callback * 50);
        outputs(&mut hub, &mut rb, callback * 50);
        assert_eq!(mode(&mut hub).len(), 3);
    }
    assert!(hub.native_status("ABC").unwrap().fault.is_none());
}

#[test]
fn net94_isolated_game_workers_publish_to_one_record_owner_and_keep_better_run() {
    let dir = MapDir::new();
    let records = crate::HordeRecords::new(dir.0.join("records.json"));
    let worker = |records: crate::HordeRecords| {
        std::thread::spawn(move || {
            let (mut hub, a, mut rx, _map) = ready(false, records);
            defeat(&mut hub);
            let report = complete(&mut hub, &a, &mut rx);
            let best = hub.rooms["ABC"].lobby(a.id, &hub.maps, "")["hordeBest"].clone();
            (report["horde"].clone(), best)
        })
    };
    let (first, first_best) = worker(records.clone()).join().unwrap();
    assert_eq!(first["record"], true);
    let (later, later_best) = worker(records.clone()).join().unwrap();
    assert_eq!(later["record"], false);
    assert_eq!(later["best"], first_best);
    assert_eq!(later_best, first_best);
    assert_eq!(
        records.lookup("bastogne", 2, "standard"),
        Some(first_best.clone())
    );
    assert_eq!(
        crate::HordeRecords::new(dir.0.join("records.json")).lookup("bastogne", 2, "standard"),
        Some(first_best)
    );
}

#[test]
fn mode09_hosted_horde_levels_and_normal_handover_preserve_army_and_deliver_views() {
    let (mut hub, a, mut ra, b, mut rb) = lobby();
    let dir = MapDir::new();
    dir.write(&arena());
    hub.configure_authored_maps(Some(dir.0.clone()));
    hub.configure_records(crate::HordeRecords::new(dir.0.join("records.json")));
    command(&mut hub, &a, json!({"t":"addAi"}), 0);
    assert_eq!(hub.rooms["ABC"].players[2].level, "normal");
    for level in ["easy", "normal", "hard"] {
        command(&mut hub, &a, json!({"t":"level","slot":2,"v":level}), 0);
        assert_eq!(hub.rooms["ABC"].players[2].level, level);
    }
    command(&mut hub, &a, json!({"t":"level","slot":0,"v":"hard"}), 0);
    assert_eq!(hub.rooms["ABC"].players[0].level, "normal");
    command(&mut hub, &b, json!({"t":"level","slot":2,"v":"easy"}), 0);
    assert_eq!(hub.rooms["ABC"].players[2].level, "hard");
    hub.start_horde("ABC", a.id, false, 0).unwrap();
    outputs(&mut hub, &mut ra, 0);
    outputs(&mut hub, &mut rb, 0);
    let army = |hub: &Hub| {
        hub.rooms["ABC"]
            .native
            .as_ref()
            .unwrap()
            .runtime
            .state()
            .world
            .game
            .units
            .values()
            .filter(|u| u.owner == 1)
            .map(|u| (u.id, u.kind, u.hp))
            .collect::<Vec<_>>()
    };
    let before = army(&hub);
    hub.disconnected("ABC", b.id, 1);
    assert!(hub.native_status("ABC").unwrap().paused);
    command(&mut hub, &a, json!({"t":"handAi","slot":1}), 2);
    assert_eq!(army(&hub), before);
    assert!(hub.rooms["ABC"].players[1].ai);
    assert_eq!(hub.rooms["ABC"].players[1].level, "normal");
    assert_eq!(hub.rooms["ABC"].players[2].level, "hard");
    assert!(!hub.native_status("ABC").unwrap().paused);
    let selected = (0..4)
        .map(|slot| SelectedSeat {
            slot,
            full: false,
            omit: false,
        })
        .collect::<Vec<_>>();
    for callback in 0..40_u64 {
        hub.poll_native(callback * 50, callback * 50_000_000);
        outputs(&mut hub, &mut ra, callback * 50);
        let runtime = &mut hub
            .rooms
            .get_mut("ABC")
            .unwrap()
            .native
            .as_mut()
            .unwrap()
            .runtime;
        assert!(runtime.fault().is_none(), "{:?}", runtime.fault());
        assert_eq!(
            runtime
                .collect_frame(&selected, FrameKind::Current)
                .unwrap()
                .len(),
            4
        );
        assert!(runtime.ai_commander(1).is_some() && runtime.ai_commander(2).is_some());
    }
}

#[test]
fn net78_hosted_horde_human_ai_and_director_idle_for_all_120_callbacks() {
    let (mut hub, a, mut ra, _, mut rb) = lobby();
    let dir = MapDir::new();
    dir.write(&arena());
    hub.configure_authored_maps(Some(dir.0.clone()));
    hub.configure_records(crate::HordeRecords::new(dir.0.join("records.json")));
    hub.configure_saves(dir.0.join("saves"));
    command(&mut hub, &a, json!({"t":"addAi"}), 0);
    hub.start_horde("ABC", a.id, false, 0).unwrap();
    outputs(&mut hub, &mut ra, 0);
    outputs(&mut hub, &mut rb, 0);
    for callback in 0..40_u64 {
        hub.poll_native(callback * 50, callback * 50_000_000);
        outputs(&mut hub, &mut ra, callback * 50);
        outputs(&mut hub, &mut rb, callback * 50);
        assert!(hub.native_status("ABC").unwrap().fault.is_none());
    }
    let runtime = &hub.rooms["ABC"].native.as_ref().unwrap().runtime;
    assert!(runtime.ai_delivered_view(2).is_some());
    let id = hub.save_room("ABC", a.id).unwrap();
    let saved = hub.saves.read("ABC", &id).unwrap();
    assert!(saved["ai"][2].is_object());
    hub.rooms.get_mut("ABC").unwrap().native = None;
    hub.load_room("ABC", a.id, &id, 2000).unwrap();
    assert!(hub.native_status("ABC").unwrap().paused);
    assert!(hub.rooms["ABC"].players[2].ai);
    assert_eq!(hub.rooms["ABC"].players[2].level, "normal");
    command(&mut hub, &a, json!({"t":"resume"}), 2000);
    outputs(&mut hub, &mut ra, 2000);
    outputs(&mut hub, &mut rb, 2000);
    defeat(&mut hub);
    let mut frozen = None;
    let mut frames = [0; 2];
    for callback in 40..640_u64 {
        let before = hub.native_status("ABC").unwrap();
        if let Some((human, director)) = &frozen {
            let runtime = &hub.rooms["ABC"].native.as_ref().unwrap().runtime;
            assert_eq!(runtime.ai_commander(2), Some(human));
            assert_eq!(runtime.ai.as_ref().unwrap().commander(3), Some(director));
            let pending = runtime.state().commands.pending_commands();
            command(&mut hub, &a, json!({"t":"stop","ids":[1]}), callback * 50);
            assert_eq!(
                hub.rooms["ABC"]
                    .native
                    .as_ref()
                    .unwrap()
                    .runtime
                    .state()
                    .commands
                    .pending_commands(),
                pending
            );
        }
        hub.poll_native(callback * 50, callback * 50_000_000);
        let status = hub.native_status("ABC").unwrap();
        assert!(status.fault.is_none(), "{:?}", status.fault);
        for (slot, rx) in [&mut ra, &mut rb].into_iter().enumerate() {
            let messages = outputs(&mut hub, rx, callback * 50);
            if frozen.is_some() {
                let count = messages.iter().filter(|v| v["t"] == "s").count();
                assert_eq!(count, usize::from(status.held.is_multiple_of(2)));
                frames[slot] += count;
                if status.held == 120 {
                    let lobby = messages.iter().find(|v| v["t"] == "lobby").unwrap();
                    assert_eq!(lobby["result"]["you"], json!({"team":0,"outcome":"defeat"}));
                }
            }
        }
        if frozen.is_some() {
            assert_eq!(status.held, before.held + 1);
            assert_eq!(
                status.tick - before.tick,
                i64::from(status.held.is_multiple_of(2))
            );
        }
        if status.ending_complete {
            break;
        }
        let runtime = &mut hub
            .rooms
            .get_mut("ABC")
            .unwrap()
            .native
            .as_mut()
            .unwrap()
            .runtime;
        if frozen.is_none() && runtime.state().finish.is_some() {
            frozen = Some((
                runtime.ai_commander(2).unwrap().clone(),
                runtime.ai.as_ref().unwrap().commander(3).unwrap().clone(),
            ));
        }
        let selected = (0..4)
            .map(|slot| SelectedSeat {
                slot,
                full: false,
                omit: false,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            runtime
                .collect_frame(&selected, FrameKind::Current)
                .unwrap()
                .len(),
            4
        );
    }
    assert_eq!(frames, [60, 60]);
    assert!(hub.rooms["ABC"].native.is_none());
    let result = &hub.rooms["ABC"].result.as_ref().unwrap().report;
    assert_eq!(result["teams"], json!([0, 0, 0, 1]));
    assert_eq!(result["names"], json!(["a", "b", "AI 1", "Horde"]));
    assert_eq!(result["winner"], 1);
}
