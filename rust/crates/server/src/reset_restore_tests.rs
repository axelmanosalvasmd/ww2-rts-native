use super::*;
use ww2_core::{
    commander_ai::Level,
    match_human_observation::HumanObservationMemory,
    seat_commander::{Brain, Delivered},
};

fn drain(
    hub: &mut Hub,
    connection: &Connection,
    rx: &mut mpsc::Receiver<Output>,
    now: u64,
) -> Vec<Value> {
    let mut out = Vec::new();
    while let Ok(output) = rx.try_recv() {
        let bytes = output.diagnostic_bytes();
        match output {
            Output::Json(value) => out.push(value),
            Output::Watcher(json) => out.push(serde_json::from_str(&json).unwrap()),
            Output::Snapshot(write) => {
                out.push(serde_json::from_str(&write.json).unwrap());
                hub.complete_snapshot(&write, true, now);
            }
            Output::Close(_, _) => {}
        }
        connection.complete_bytes(bytes);
    }
    out
}
fn warm_human(native: &mut AttachedRuntime, slot: usize) {
    let (seat, memory) = &mut native.ai.pairs[slot];
    let view = native
        .runtime
        .state_mut()
        .human_commander_observation(memory)
        .unwrap();
    let tick = native.runtime.state().world.game.tick as u64;
    native
        .runtime
        .state()
        .advance_seat_commander(seat, Some(Delivered::Human(Box::new(view))), tick, &[])
        .unwrap();
    let Brain::Human {
        state, delivered, ..
    } = seat.brain()
    else {
        panic!()
    };
    assert!(state.snapshot().is_some());
    assert!(delivered.is_some());
}
fn fresh_pair(
    native: &AttachedRuntime,
    slot: usize,
    level: Level,
) -> (
    ww2_core::seat_commander::SeatCommander,
    HumanObservationMemory,
) {
    (
        native
            .runtime
            .state()
            .create_seat_commander(slot, level)
            .unwrap(),
        HumanObservationMemory::new(slot),
    )
}

#[test]
fn net25_seated_takeover_resets_planner_observation_and_helper_without_losing_identity_or_orders() {
    let dir = TestDir::new();
    let (mut hub, a, mut ra, b, mut rb) = setup(&dir);
    let native = hub.rooms.get_mut("ABC").unwrap().native.as_mut().unwrap();
    native.runtime.state_mut().start.win_vp = 1000.;
    // Exercise the same private owners before the person reclaims the seat.
    for _ in 0..160 {
        native.runtime.advance_tick(&[false; 2]).unwrap();
        warm_human(native, 0);
        warm_human(native, 1);
    }
    let fresh = fresh_pair(native, 0, Level::Normal);
    let Brain::Human { policy: stale, .. } = native.ai.pairs[0].0.brain() else {
        panic!()
    };
    let Brain::Human { policy: empty, .. } = fresh.0.brain() else {
        panic!()
    };
    assert_ne!(
        stale, empty,
        "the returning person must clear actual policy memory"
    );
    let actor = native
        .runtime
        .state()
        .world
        .game
        .units
        .values()
        .find(|unit| {
            unit.owner == 0 && native.runtime.state().rules.kinds[unit.kind].name == "rifle"
        })
        .unwrap()
        .id;
    native
        .runtime
        .state_mut()
        .enqueue_command(
            ww2_core::orders::OrderKey {
                tick: 500,
                issuer: 0,
                sequence: 4,
            },
            ww2_core::match_commands::Command::Stop {
                ids: vec![i64::from(actor)],
            },
        )
        .unwrap();
    let other = native.ai.pairs[1].clone();
    assert_ne!(native.ai.pairs[0], fresh_pair(native, 0, Level::Normal));
    let commands = native
        .runtime
        .state()
        .commands
        .pending_tick(500)
        .map(|(key, command)| (*key, command.clone()))
        .collect::<Vec<_>>();
    let units = serde_json::to_value(&native.runtime.state().world.game.units).unwrap();
    let random = native.runtime.state().start.random.snapshot();
    let tick_before = native.runtime.state().world.game.tick;
    hub.rooms.get_mut("ABC").unwrap().players[0].denies = vec![1, 2];
    hub.rooms.get_mut("ABC").unwrap().players[0].rtt = Some(7.);
    hub.rooms.get_mut("ABC").unwrap().players[0].disconnected = Some(3);
    let (replacement, mut rr) = peer(3);
    hub.message(
        "ABC",
        &replacement,
        &mut false,
        json!({"t":"hello","token":"host","name":"Returned"}),
        4,
    );
    let room = &hub.rooms["ABC"];
    assert_eq!(room.players.len(), 2);
    assert_eq!(room.players[0].connection.as_ref().unwrap().id, 3);
    assert_eq!(room.players[0].denies, [1, 2]);
    assert_eq!(room.players[0].rtt, Some(7.));
    assert_eq!(room.players[0].disconnected, None);
    let native = room.native.as_ref().unwrap();
    assert_eq!(native.ai.pairs[0], fresh_pair(native, 0, Level::Normal));
    assert_eq!(native.ai.pairs[1], other);
    assert_eq!(
        native
            .runtime
            .state()
            .commands
            .pending_tick(500)
            .map(|(key, command)| (*key, command.clone()))
            .collect::<Vec<_>>(),
        commands
    );
    assert_eq!(
        serde_json::to_value(&native.runtime.state().world.game.units).unwrap(),
        units
    );
    assert_eq!(native.runtime.state().start.random.snapshot(), random);
    assert_eq!(native.runtime.state().world.game.tick, tick_before);
    assert_eq!(drain(&mut hub, &a, &mut ra, 4)[0], json!({"t":"replaced"}));
    assert_eq!(drain(&mut hub, &replacement, &mut rr, 4)[0]["t"], "start");
    hub.disconnected("ABC", a.id, 5);
    assert_eq!(
        hub.rooms["ABC"].players[0].connection.as_ref().unwrap().id,
        3
    );
    let mut joined = true;
    hub.message(
        "ABC",
        &a,
        &mut joined,
        json!({"t":"name","name":"Stale"}),
        6,
    );
    assert_eq!(hub.rooms["ABC"].players[0].name, "Returned");
    drain(&mut hub, &b, &mut rb, 6);
    for tick in 1..=8 {
        hub.poll_native(tick * 50, tick * 50_000_000);
        let runtime = &hub.rooms["ABC"].native.as_ref().unwrap().runtime;
        assert!(
            runtime.fault().is_none(),
            "tick {tick}: {:?}",
            runtime.fault()
        );
        let paused = runtime.paused();
        let due = runtime.should_snapshot(runtime.state().world.game.tick);
        for (c, rx) in [(&replacement, &mut rr), (&b, &mut rb)] {
            let messages = drain(&mut hub, c, rx, tick * 50);
            if due {
                assert!(
                    messages.iter().any(|m| m["t"] == "s"),
                    "tick {tick}, connection {}, paused {paused}: {messages:?}",
                    c.id
                );
            }
        }
        assert!(
            hub.rooms["ABC"]
                .native
                .as_ref()
                .unwrap()
                .runtime
                .fault()
                .is_none()
        );
    }
}

#[test]
fn net92_ai_memory_human_reset_and_surplus_watchers_restore_transactionally_through_victory() {
    let dir = TestDir::new();
    let (mut hub, a, mut ra, b, mut rb) = setup(&dir);
    // Use the ordinary attachment path with an AI opponent and a real spectator.
    hub.rooms.get_mut("ABC").unwrap().native = None;
    let old = hub.rooms.get_mut("ABC").unwrap().players.pop().unwrap();
    hub.rooms.get_mut("ABC").unwrap().spectators.push(old);
    let ai = Person {
        token: Vec::new(),
        name: "Bot".into(),
        ai: true,
        connection: None,
        team: 1,
        faction: 1,
        level: "hard",
        disconnected: None,
        denies: Vec::new(),
        rtt: None,
        diagnostics: Default::default(),
        last_match: None,
    };
    hub.rooms.get_mut("ABC").unwrap().players.push(ai);
    let (mut state, raw) = small_match();
    state.start.win_vp = 10.;
    for control in state.commands.units.values_mut() {
        control.hold_fire = false;
    }
    // Use the starting armies without additional purchases.
    for player in &mut state.start.economy.players {
        player.mp = 0.;
        player.fuel = 0.;
        player.mun = 0.;
    }
    let prepared = hub
        .prepare_native_room(&hub.rooms["ABC"], state, 0, false, Some(&raw))
        .unwrap();
    hub.publish_native("ABC", prepared);
    drain(&mut hub, &a, &mut ra, 0);
    drain(&mut hub, &b, &mut rb, 0);
    let fresh_ai = {
        let n = hub.rooms["ABC"].native.as_ref().unwrap();
        n.ai.save(n.runtime.state()).unwrap()
    };
    let mut combat_frames = 0;
    for tick in 1..=160 {
        hub.poll_native(tick * 50, tick * 50_000_000);
        for (c, rx) in [(&a, &mut ra), (&b, &mut rb)] {
            combat_frames += drain(&mut hub, c, rx, tick * 50)
                .iter()
                .filter(|m| m["shots"].as_array().is_some_and(|shots| !shots.is_empty()))
                .count();
        }
        assert!(
            hub.rooms["ABC"]
                .native
                .as_ref()
                .unwrap()
                .runtime
                .fault()
                .is_none()
        );
    }
    let native = hub.rooms.get_mut("ABC").unwrap().native.as_mut().unwrap();
    warm_human(native, 0);
    assert_ne!(native.ai.pairs[0], fresh_pair(native, 0, Level::Normal));
    let Brain::Human {
        policy: warm_policy,
        ..
    } = native.ai.pairs[1].0.brain()
    else {
        panic!()
    };
    let fresh = fresh_pair(native, 1, Level::Hard);
    let Brain::Human {
        policy: fresh_policy,
        ..
    } = fresh.0.brain()
    else {
        panic!()
    };
    assert_ne!(
        warm_policy, fresh_policy,
        "save must contain exercised strategic memory"
    );
    let expected_policy = warm_policy.clone();
    let expected_memory = serde_json::to_value(&native.ai.pairs[1].1).unwrap();
    let id = hub.save_room("ABC", a.id).unwrap();
    let saved = hub.saves.read("ABC", &id).unwrap();
    assert_eq!(saved["version"], 1);
    assert!(saved["ai"][0].is_null());
    assert!(!saved["ai"][1].is_null());
    assert_ne!(saved["ai"], fresh_ai);
    assert_eq!(
        saved["seats"][1],
        json!({"name":"Bot","token":"","ai":true,"level":"hard","team":1,"faction":1})
    );
    let (extra, mut re) = peer(4);
    hub.message(
        "ABC",
        &extra,
        &mut false,
        json!({"t":"hello","token":"extra","name":"Extra","spectate":true}),
        8001,
    );
    drain(&mut hub, &a, &mut ra, 8001);
    drain(&mut hub, &b, &mut rb, 8001);
    drain(&mut hub, &extra, &mut re, 8001);
    hub.rooms.get_mut("ABC").unwrap().native = None;
    // A malformed owner must leave lobby, epoch and every connection intact.
    let epoch = hub.next_epoch;
    let mut bad = saved.clone();
    bad["ai"][1] = json!(["broken"]);
    let bad_id = hub
        .saves
        .write("ABC", "manual", &bad, json!({}), 1_800_000_000_001)
        .unwrap();
    assert!(hub.load_room("ABC", a.id, &bad_id, 8002).is_err());
    assert_eq!(hub.next_epoch, epoch);
    assert!(hub.rooms["ABC"].native.is_none());
    assert_eq!(hub.rooms["ABC"].spectators.len(), 2);
    hub.load_room("ABC", a.id, &id, 8003).unwrap();
    let room = &hub.rooms["ABC"];
    assert_eq!(
        room.spectators
            .iter()
            .map(|p| p.name.as_str())
            .collect::<Vec<_>>(),
        ["guest", "Extra"]
    );
    assert_eq!(
        room.players[0].token,
        "host".encode_utf16().collect::<Vec<_>>()
    );
    assert!(room.players[1].ai);
    assert_eq!(room.players[1].level, "hard");
    let native = room.native.as_ref().unwrap();
    assert!(native.runtime.paused());
    assert_eq!(native.ai.pairs[0], fresh_pair(native, 0, Level::Normal));
    assert_same_json(
        &native.ai.save(native.runtime.state()).unwrap(),
        &saved["ai"],
    );
    let Brain::Human {
        state,
        policy,
        delivered,
    } = native.ai.pairs[1].0.brain()
    else {
        panic!()
    };
    assert!(state.snapshot().is_none());
    assert!(delivered.is_none());
    assert_eq!(policy, &expected_policy);
    assert_eq!(
        serde_json::to_value(&native.ai.pairs[1].1).unwrap(),
        expected_memory
    );
    for (c, rx) in [(&a, &mut ra), (&b, &mut rb), (&extra, &mut re)] {
        let messages = drain(&mut hub, c, rx, 8003);
        let start = messages.iter().position(|m| m["t"] == "start").unwrap();
        assert!(messages[..start].iter().any(|m| m["t"] == "lobby"));
        assert_eq!(
            messages[start + 1],
            json!({"t":"pause","paused":true,"by":"host","reason":"host","left":0})
        );
    }
    hub.message("ABC", &a, &mut true, json!({"t":"resume"}), 8004);
    let mut frames = [0; 3];
    let mut finished = false;
    for tick in 161..=3000 {
        hub.poll_native(tick * 50, tick * 50_000_000);
        for (index, (c, rx)) in [(&a, &mut ra), (&b, &mut rb), (&extra, &mut re)]
            .into_iter()
            .enumerate()
        {
            let messages = drain(&mut hub, c, rx, tick * 50);
            frames[index] += messages.iter().filter(|m| m["t"] == "s").count();
            combat_frames += messages
                .iter()
                .filter(|m| m["shots"].as_array().is_some_and(|shots| !shots.is_empty()))
                .count();
        }
        if let Some(native) = hub.rooms.get_mut("ABC").unwrap().native.as_mut() {
            let ai_frame = native
                .runtime
                .collect_frame(
                    &[SelectedSeat {
                        slot: 1,
                        full: true,
                        omit: false,
                    }],
                    FrameKind::Current,
                )
                .unwrap();
            assert_eq!(ai_frame[0].message["t"], "s");
            assert!(
                native.runtime.fault().is_none(),
                "{:?}",
                native.runtime.fault()
            );
            assert!(!native.runtime.state().start.economy.players[1].away);
        } else {
            finished = true;
            break;
        }
    }
    assert!(
        finished,
        "ordinary capture and VP income must finish the restored room"
    );
    assert!(
        combat_frames > 0,
        "real combat must be delivered without a fault"
    );
    assert!(frames.iter().all(|count| *count > 4));
    assert!(hub.rooms["ABC"].result.is_some());
    assert_eq!(hub.rooms["ABC"].spectators.len(), 2);
}

#[test]
fn net92_legacy_ai_defaults_and_spectator_host_pause_survive_load() {
    let dir = TestDir::new();
    let (mut hub, a, mut ra, b, mut rb) = setup(&dir);
    let id = hub.save_room("ABC", a.id).unwrap();
    let mut saved = hub.saves.read("ABC", &id).unwrap();
    for seat in saved["seats"].as_array_mut().unwrap() {
        seat["ai"] = json!(true);
        seat["token"] = json!("");
        seat.as_object_mut().unwrap().remove("level");
    }
    saved.as_object_mut().unwrap().remove("ai");
    let id = hub
        .saves
        .write("ABC", "manual", &saved, json!({}), 1_800_000_000_002)
        .unwrap();
    hub.rooms.get_mut("ABC").unwrap().native = None;
    hub.load_room("ABC", a.id, &id, 1).unwrap();
    let room = &hub.rooms["ABC"];
    assert!(room.players.iter().all(|p| p.ai && p.level == "normal"));
    assert_eq!(room.spectators.len(), 2);
    assert!(room.is_host(a.id));
    assert_eq!(
        room.native.as_ref().unwrap().runtime.pause_message(1)["by"],
        "host"
    );
    drain(&mut hub, &a, &mut ra, 1);
    drain(&mut hub, &b, &mut rb, 1);
    hub.message("ABC", &a, &mut true, json!({"t":"resume"}), 2);
    for tick in 1..=6 {
        hub.poll_native(tick * 50, tick * 50_000_000);
        drain(&mut hub, &a, &mut ra, tick * 50);
        drain(&mut hub, &b, &mut rb, tick * 50);
        assert!(
            hub.rooms["ABC"]
                .native
                .as_ref()
                .unwrap()
                .runtime
                .fault()
                .is_none()
        );
    }
}

#[test]
fn net25_restart_recreates_ai_and_human_helpers_and_keeps_spectator_delivery() {
    let dir = TestDir::new();
    let mut hub = Hub::new(32, String::new());
    hub.configure_saves(dir.0.clone());
    hub.configure_trusted_room(Some(crate::TrustedRoomConfig::new("ABC", 123).unwrap()));
    let (a, mut ra) = peer(1);
    let (watcher, mut rw) = peer(2);
    hub.message(
        "ABC",
        &a,
        &mut false,
        json!({"t":"hello","token":"host","name":"host"}),
        0,
    );
    hub.message("ABC", &a, &mut true, json!({"t":"addAi"}), 0);
    hub.message("ABC", &a, &mut true, json!({"t":"team","slot":1,"v":1}), 0);
    hub.message(
        "ABC",
        &a,
        &mut true,
        json!({"t":"level","slot":1,"v":"hard"}),
        0,
    );
    assert!(hub.rooms["ABC"].players[1].ai);
    assert_eq!(hub.rooms["ABC"].players[1].level, "hard");
    hub.message(
        "ABC",
        &watcher,
        &mut false,
        json!({"t":"hello","token":"watcher","name":"watcher","spectate":true}),
        0,
    );
    hub.message("ABC", &a, &mut true, json!({"t":"start"}), 0);
    drain(&mut hub, &a, &mut ra, 0);
    drain(&mut hub, &watcher, &mut rw, 0);
    let epoch = hub.rooms["ABC"].native.as_ref().unwrap().epoch;
    let save_id = hub.save_room("ABC", a.id).unwrap();
    let body = hub.saves.read("ABC", &save_id).unwrap();
    Match::from_save_json(body["g"]["state"].clone()).unwrap();
    for tick in 1..=4 {
        hub.poll_native(tick * 50, tick * 50_000_000);
        drain(&mut hub, &a, &mut ra, tick * 50);
        drain(&mut hub, &watcher, &mut rw, tick * 50);
    }
    let native = hub.rooms.get_mut("ABC").unwrap().native.as_mut().unwrap();
    warm_human(native, 0);
    assert_ne!(native.ai.pairs[0], fresh_pair(native, 0, Level::Normal));
    assert_ne!(native.ai.pairs[1], fresh_pair(native, 1, Level::Hard));
    hub.message("ABC", &a, &mut true, json!({"t":"restart"}), 201);
    let native = hub.rooms["ABC"].native.as_ref().unwrap();
    assert_eq!(native.epoch, epoch + 1);
    assert_eq!(native.runtime.state().world.game.tick, 0);
    assert_eq!(native.ai.pairs[0], fresh_pair(native, 0, Level::Normal));
    assert_eq!(native.ai.pairs[1], fresh_pair(native, 1, Level::Hard));
    for (c, rx) in [(&a, &mut ra), (&watcher, &mut rw)] {
        assert!(
            drain(&mut hub, c, rx, 201)
                .iter()
                .any(|m| m["t"] == "start" && m["matchId"] == epoch + 1)
        );
    }
    for tick in 5..=12 {
        hub.poll_native(tick * 50, tick * 50_000_000);
        for (c, rx) in [(&a, &mut ra), (&watcher, &mut rw)] {
            let messages = drain(&mut hub, c, rx, tick * 50);
            if tick % 2 == 0 {
                assert!(messages.iter().any(|m| m["t"] == "s"));
            }
        }
        assert!(
            hub.rooms["ABC"]
                .native
                .as_ref()
                .unwrap()
                .runtime
                .fault()
                .is_none()
        );
    }
}
