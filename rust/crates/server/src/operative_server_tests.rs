use super::*;

#[path = "operative_lifecycle_tests.rs"]
mod lifecycle;

#[path = "operative_room_tests.rs"]
mod room_protocol;

#[path = "operative_supplemental_tests.rs"]
mod supplemental;

#[path = "operative_timing_tests.rs"]
mod timing;

fn client(id: u64) -> (Connection, mpsc::Receiver<Output>) {
    let (tx, rx) = mpsc::channel(256);
    let (cancel, _) = watch::channel(false);
    (Connection::new(id, tx, cancel), rx)
}

fn hello(hub: &mut Hub, connection: &Connection, message: Value, now: u64) -> bool {
    let mut joined = false;
    hub.message("ABC", connection, &mut joined, message, now);
    joined
}

fn command(hub: &mut Hub, connection: &Connection, message: Value, now: u64) {
    hub.message("ABC", connection, &mut true, message, now);
}

fn join_operative(hub: &mut Hub, connection: &Connection, token: &str, commander: usize) {
    assert!(hello(
        hub,
        connection,
        json!({"t":"hello","token":token,"name":token,"role":"operative","commander":commander}),
        0,
    ));
}

fn room() -> (Hub, [Connection; 2], [mpsc::Receiver<Output>; 2]) {
    let mut hub = Hub::new(32, String::new());
    let (a, ra) = client(1);
    let (b, rb) = client(2);
    for (connection, token) in [(&a, "a"), (&b, "b")] {
        assert!(hello(
            &mut hub,
            connection,
            json!({"t":"hello","token":token,"name":token}),
            0,
        ));
    }
    let mut receivers = [ra, rb];
    drain_commanders(&mut hub, &mut receivers, 0);
    (hub, [a, b], receivers)
}

fn messages(receiver: &mut mpsc::Receiver<Output>) -> Vec<Value> {
    let mut result = Vec::new();
    while let Ok(output) = receiver.try_recv() {
        match output {
            Output::Json(value) => result.push(value),
            Output::Watcher(_) => {
                panic!("commander and operative sockets cannot receive watcher frames")
            }
            Output::Snapshot(_) => panic!("operative delivery uses its own JSON stream"),
            Output::Close(_, reason) => panic!("unexpected close: {reason}"),
            Output::Watcher(_) => panic!("unexpected watcher output on operative channel"),
        }
    }
    result
}

fn drain_commanders(
    hub: &mut Hub,
    receivers: &mut [mpsc::Receiver<Output>; 2],
    now: u64,
) -> [Vec<Value>; 2] {
    receivers.each_mut().map(|receiver| {
        let mut result = Vec::new();
        while let Ok(output) = receiver.try_recv() {
            match output {
                Output::Json(value) => result.push(value),
                Output::Watcher(_) => {
                    panic!("commander and operative sockets cannot receive watcher frames")
                }
                Output::Snapshot(write) => {
                    result.push(serde_json::from_str(&write.json).unwrap());
                    hub.complete_snapshot(&write, true, now);
                }
                Output::Close(_, reason) => panic!("unexpected commander close: {reason}"),
                Output::Watcher(_) => panic!("unexpected watcher output on commander channel"),
            }
        }
        result
    })
}

fn state(hub: &Hub) -> &Match {
    hub.rooms["ABC"].native.as_ref().unwrap().runtime.state()
}

fn fps(seq: i64) -> Value {
    json!({"t":"fps","seq":seq,"forward":1,"strafe":0,"yaw":0,"pitch":0})
}

fn last_seq(hub: &Hub, key: &str) -> i64 {
    state(hub).companions.get(key).unwrap().last_seq
}

fn lobby(hub: &Hub, id: u64) -> Value {
    hub.rooms["ABC"].lobby(id, &hub.maps, &hub.public_url)
}

#[test]
fn operative_join_checks_each_requirement_and_keeps_four_nonseat_members() {
    // Prototype server.js:604-607. Invalid requests never fall back to a spectator.
    for commander in [
        Value::Null,
        json!(-1),
        json!(2),
        json!(0.5),
        json!("0"),
        json!(true),
    ] {
        let (mut hub, _, _) = room();
        let (op, mut receiver) = client(3);
        assert!(!hello(
            &mut hub,
            &op,
            json!({"t":"hello","token":"op","name":"op","role":"operative","commander":commander}),
            0,
        ));
        assert_join_refused(&mut receiver);
        assert!(hub.rooms["ABC"].operatives.is_empty());
        assert!(hub.rooms["ABC"].spectators.is_empty());
    }
    for reason in ["empty-token", "ai", "world", "limit"] {
        let (mut hub, _, _) = room();
        match reason {
            "ai" => hub.rooms.get_mut("ABC").unwrap().players[0].ai = true,
            "world" => hub.rooms.get_mut("ABC").unwrap().mode = "world".into(),
            "limit" => {
                for index in 0..4 {
                    let (op, _receiver) = client(10 + index);
                    join_operative(&mut hub, &op, &format!("op{index}"), 0);
                    hub.disconnected("ABC", op.id, 1);
                }
            }
            _ => {}
        }
        let (op, mut receiver) = client(30);
        assert!(!hello(
            &mut hub,
            &op,
            json!({"t":"hello","token":if reason == "empty-token" { "" } else { "op" },"name":"op","role":"operative","commander":0}),
            2,
        ));
        assert_join_refused(&mut receiver);
        assert_eq!(hub.rooms["ABC"].players.len(), 2);
        assert!(hub.rooms["ABC"].spectators.is_empty());
    }
    let (mut hub, _, _) = room();
    let mut sockets = Vec::new();
    for index in 0..4 {
        let (op, receiver) = client(10 + index);
        join_operative(&mut hub, &op, &format!("op{index}"), index as usize % 2);
        sockets.push((op, receiver));
    }
    assert_eq!(hub.rooms["ABC"].players.len(), 2);
    assert_eq!(hub.rooms["ABC"].operatives.len(), 4);
    assert!(hub.rooms["ABC"].spectators.is_empty());
    for (op, mut receiver) in sockets {
        assert!(!hub.rooms["ABC"].is_host(op.id));
        assert!(
            messages(&mut receiver)
                .iter()
                .any(|message| message["t"] == "lobby")
        );
    }
}

fn assert_join_refused(receiver: &mut mpsc::Receiver<Output>) {
    let Output::Json(deny) = receiver.try_recv().unwrap() else {
        panic!("join refusal precedes close")
    };
    assert_eq!(deny, json!({"t":"deny","cmd":"join","reason":"commander"}));
    assert!(matches!(
        receiver.try_recv().unwrap(),
        Output::Close(1008, _)
    ));
    assert!(receiver.try_recv().is_err());
}

#[test]
fn operative_lobby_tracks_commander_identity_across_nonfinal_seat_removal() {
    let (mut hub, _, _) = room();
    let (third, _third_receiver) = client(3);
    assert!(hello(
        &mut hub,
        &third,
        json!({"t":"hello","token":"c","name":"c"}),
        0
    ));
    let (op, mut receiver) = client(4);
    join_operative(&mut hub, &op, "friend", 2);
    let expected = json!([{"name":"friend","commander":2,"online":true}]);
    for id in 1..=4 {
        assert_eq!(lobby(&hub, id)["operatives"], expected);
    }
    let view = lobby(&hub, op.id);
    assert_eq!(view["role"], "operative");
    assert_eq!(view["commander"], 2);
    assert_eq!(view["you"], -1);
    assert_eq!(view["spectator"], false);
    assert_eq!(view["amHost"], false);
    hub.rooms
        .get_mut("ABC")
        .unwrap()
        .retain_seats(|slot, _| slot != 1);
    assert_eq!(lobby(&hub, op.id)["commander"], 1);
    assert_eq!(lobby(&hub, op.id)["players"][1]["name"], "c");
    hub.rooms
        .get_mut("ABC")
        .unwrap()
        .retain_seats(|slot, _| slot != 1);
    assert_eq!(lobby(&hub, op.id)["commander"], -1);
    // A new seat at the same index must not acquire the departed commander's operative.
    let (replacement, _replacement_receiver) = client(5);
    assert!(hello(
        &mut hub,
        &replacement,
        json!({"t":"hello","token":"d","name":"d"}),
        0
    ));
    assert_eq!(lobby(&hub, op.id)["commander"], -1);
    messages(&mut receiver);
    hub.disconnected("ABC", op.id, 1);
    assert_eq!(
        lobby(&hub, 1)["operatives"],
        json!([{"name":"friend","commander":-1,"online":false}])
    );
}

#[test]
fn operative_presence_prevents_room_expiry_and_never_grants_host() {
    let (mut hub, commanders, _) = room();
    let (op, mut receiver) = client(3);
    join_operative(&mut hub, &op, "friend", 0);
    for commander in &commanders {
        hub.disconnected("ABC", commander.id, 1);
    }
    assert_eq!(hub.rooms["ABC"].empty_since, None);
    hub.expire(70_000);
    assert!(hub.rooms.contains_key("ABC"));
    assert!(!hub.rooms["ABC"].is_host(op.id));
    assert_eq!(lobby(&hub, op.id)["commander"], -1);
    messages(&mut receiver);
    hub.disconnected("ABC", op.id, 70_001);
    assert_eq!(hub.rooms["ABC"].empty_since, Some(70_001));
    hub.expire(130_002);
    assert!(!hub.rooms.contains_key("ABC"));

    let (mut hub, commanders, mut receivers) = room();
    hub.attach_native("ABC", native_tests::native_match(), 0)
        .unwrap();
    let (op, mut receiver) = client(3);
    join_operative(&mut hub, &op, "friend", 0);
    messages(&mut receiver);
    drain_commanders(&mut hub, &mut receivers, 0);
    hub.disconnected("ABC", commanders[0].id, 1);
    let broadcast = messages(&mut receiver);
    let pause = broadcast
        .iter()
        .find(|message| message["t"] == "pause")
        .unwrap();
    assert_eq!(pause["paused"], true);
    assert_eq!(pause["reason"], "drop");
    assert_eq!(pause["by"], "a");
    assert!(
        broadcast
            .iter()
            .any(|message| message["t"] == "lobby" && message["role"] == "operative")
    );
}

#[test]
fn operative_permission_whitelist_is_silent_except_name_and_latency_ping() {
    let (mut hub, _, mut receivers) = room();
    let (op, mut receiver) = client(3);
    join_operative(&mut hub, &op, "friend", 0);
    messages(&mut receiver);
    drain_commanders(&mut hub, &mut receivers, 0);
    for playing in [false, true] {
        if playing {
            hub.attach_native("ABC", native_tests::native_match(), 0)
                .unwrap();
            messages(&mut receiver);
            drain_commanders(&mut hub, &mut receivers, 0);
        }
        let before = lobby(&hub, op.id);
        let denies = hub.rooms["ABC"].operatives[0].person.denies.clone();
        let gameplay = || {
            let state = state(&hub);
            format!(
                "{:?}{:?}{:?}{:?}",
                state.world.game.units, state.commands.units, state.start.economy, state.companions
            )
        };
        let gameplay_before = playing.then(gameplay);
        for message in [
            json!({"t":"move","orders":[[1,100,100]]}),
            json!({"t":"stop","ids":[1]}),
            json!({"t":"attack","ids":[1],"target":2}),
            json!({"t":"retreat","ids":[1]}),
            json!({"t":"stance","ids":[1],"holdFire":true}),
            json!({"t":"buy","kind":"rifle"}),
            json!({"t":"support","kind":"bombing","x":100,"z":100}),
            json!({"t":"ability","ids":[1],"key":"nade","x":100,"z":100}),
            json!({"t":"move","orders":"malformed"}),
            json!({"t":"save"}),
            json!({"t":"loadSave","id":"private"}),
            json!({"t":"start"}),
            json!({"t":"restart"}),
            json!({"t":"end"}),
            json!({"t":"pause"}),
            json!({"t":"resume"}),
            json!({"t":"addAi"}),
            json!({"t":"kick","slot":0}),
            json!({"t":"team","slot":0,"v":1}),
            json!({"t":"faction","slot":0,"v":2}),
            json!({"t":"map","map":"el-alamein"}),
            json!({"t":"spectate"}),
            json!({"t":"sit"}),
            json!({"t":"leave"}),
            json!({"t":"handAi","slot":0}),
            json!({"t":"ping","x":10,"z":10,"c":5}),
        ] {
            command(&mut hub, &op, message, 20);
            assert!(messages(&mut receiver).is_empty());
            assert!(
                drain_commanders(&mut hub, &mut receivers, 20)
                    .iter()
                    .all(Vec::is_empty)
            );
        }
        assert_eq!(lobby(&hub, op.id), before);
        assert_eq!(hub.rooms["ABC"].operatives[0].person.denies, denies);
        assert_eq!(hub.rooms["ABC"].operatives.len(), 1);
        assert!(hub.rooms["ABC"].spectators.is_empty());
        assert_eq!(hub.native_status("ABC").is_ok(), playing);
        if playing {
            assert!(!hub.native_status("ABC").unwrap().paused);
            assert_eq!(hub.native_status("ABC").unwrap().tick, 0);
            assert_eq!(state(&hub).commands.pending_commands(), 0);
            let state = state(&hub);
            assert_eq!(
                gameplay_before.unwrap(),
                format!(
                    "{:?}{:?}{:?}{:?}",
                    state.world.game.units,
                    state.commands.units,
                    state.start.economy,
                    state.companions
                )
            );
        }
        command(&mut hub, &op, json!({"t":"ping","c":123,"rtt":18.6}), 21);
        let pong = messages(&mut receiver);
        assert_eq!(pong.len(), 1);
        assert_eq!(pong[0]["t"], "pong");
        assert_eq!(pong[0]["c"], 123);
        assert_eq!(hub.rooms["ABC"].operatives[0].person.rtt, Some(19.0));
        command(&mut hub, &op, json!({"t":"name","name":"<Friend>"}), 22);
        assert_eq!(lobby(&hub, op.id)["operatives"][0]["name"], "Friend");
        assert!(
            messages(&mut receiver)
                .iter()
                .any(|message| message["t"] == "lobby")
        );
        assert!(
            drain_commanders(&mut hub, &mut receivers, 22)
                .iter()
                .all(|messages| messages.iter().any(|message| message["t"] == "lobby"))
        );
    }
}

#[test]
fn operative_input_has_independent_twenty_millisecond_boundaries_and_pause_gate() {
    let (mut hub, _, mut receivers) = room();
    let (a, mut ra) = client(3);
    let (b, mut rb) = client(4);
    join_operative(&mut hub, &a, "first", 0);
    join_operative(&mut hub, &b, "second", 1);
    command(&mut hub, &a, fps(50), 500);
    assert_eq!(hub.rooms["ABC"].operatives[0].input_at, 0);
    hub.attach_native("ABC", native_tests::native_match(), 0)
        .unwrap();
    messages(&mut ra);
    messages(&mut rb);
    drain_commanders(&mut hub, &mut receivers, 0);
    for now in [0, 19] {
        command(&mut hub, &a, fps(0), now);
        assert_eq!(last_seq(&hub, "first"), -1);
    }
    command(&mut hub, &a, fps(0), 20);
    assert_eq!(last_seq(&hub, "first"), 0);
    command(&mut hub, &a, fps(1), 25);
    command(&mut hub, &a, fps(1), 39);
    assert_eq!(last_seq(&hub, "first"), 0);
    command(&mut hub, &b, fps(0), 20);
    assert_eq!(last_seq(&hub, "second"), 0);
    command(&mut hub, &a, fps(2), 40);
    assert_eq!(last_seq(&hub, "first"), 2);
    hub.rooms
        .get_mut("ABC")
        .unwrap()
        .native
        .as_mut()
        .unwrap()
        .runtime
        .pause(0, "a", crate::room_runtime::PauseReason::Host, 41)
        .unwrap();
    command(&mut hub, &a, fps(3), 60);
    assert_eq!(last_seq(&hub, "first"), 2);
    assert_eq!(hub.rooms["ABC"].operatives[0].input_at, 40);
    hub.rooms
        .get_mut("ABC")
        .unwrap()
        .native
        .as_mut()
        .unwrap()
        .runtime
        .resume(61);
    command(&mut hub, &a, fps(3), 61);
    assert_eq!(last_seq(&hub, "first"), 3);
    let mut invalid = fps(4);
    invalid["forward"] = json!("bad");
    command(&mut hub, &a, invalid, 81);
    assert_eq!(last_seq(&hub, "first"), 3);
    assert_eq!(hub.rooms["ABC"].operatives[0].input_at, 81);
    command(&mut hub, &a, fps(4), 100);
    assert_eq!(last_seq(&hub, "first"), 3);
    command(&mut hub, &a, fps(4), 101);
    assert_eq!(last_seq(&hub, "first"), 4);
    assert!(messages(&mut ra).is_empty());
    assert!(messages(&mut rb).is_empty());
}

fn assert_start(message: &Value, commander_start: &Value, owner: usize, soldier: u32) {
    let mut keys = message
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect::<Vec<_>>();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "cells",
            "factions",
            "fog",
            "map",
            "matchId",
            "names",
            "operative",
            "role",
            "spawn",
            "spawns",
            "t",
            "teams",
            "weather",
            "you"
        ]
    );
    assert_eq!(message["role"], "operative");
    assert_eq!(message["you"], owner);
    assert_eq!(message["operative"]["id"], soldier);
    assert_eq!(message["operative"]["seq"], -1);
    assert!(message["cells"].is_array());
    assert!(message["fog"].is_object());
    for key in [
        "map", "matchId", "names", "teams", "factions", "weather", "spawns", "cells",
    ] {
        assert_eq!(message[key], commander_start[key], "operative start {key}");
    }
    assert_eq!(message["spawn"], commander_start["spawns"][owner]);
}

fn assert_operative_frame(message: &Value, expected_status: &Value) {
    assert_eq!(message["operative"], *expected_status);
    assert_eq!(message["online"], json!([true, true]));
    assert_eq!(message["ping"], json!([null, null]));
    assert!(message["units"].is_array());
    assert!(
        message["units"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row[2] != 1)
    );
    for key in [
        "movement",
        "queues",
        "productionJobs",
        "plans",
        "orders",
        "air",
        "rally",
        "works",
        "covers",
        "sup",
    ] {
        assert!(message.get(key).is_none(), "operative leaked {key}");
    }
}

#[test]
fn operative_running_join_moves_and_delivers_through_pause_victory_and_next_match() {
    let (mut hub, _, mut receivers) = room();
    let epoch = hub
        .attach_native("ABC", native_tests::native_match(), 0)
        .unwrap();
    let starts = drain_commanders(&mut hub, &mut receivers, 0);
    let commander_start = starts[0]
        .iter()
        .find(|message| message["t"] == "start")
        .unwrap();
    let (op, mut receiver) = client(3);
    join_operative(&mut hub, &op, "friend", 0);
    let soldier = state(&hub).companions.get("friend").unwrap().unit_id;
    let joined = messages(&mut receiver);
    assert_start(
        joined
            .iter()
            .find(|message| message["t"] == "start")
            .unwrap(),
        commander_start,
        0,
        soldier,
    );
    assert!(!hub.accounts.contains_key(&op.id));
    assert_eq!(hub.rooms["ABC"].players.len(), 2);
    let initial = &state(&hub).world.game.units[&soldier];
    let before = (initial.x, initial.z);
    let mut input = fps(0);
    input["fire"] = json!(true);
    input["nade"] = json!(0);
    command(&mut hub, &op, input.clone(), 20);
    hub.poll_native(0, 0);
    let mut frames = 0;
    let mut saw_fire = false;
    let mut saw_throw = false;
    let mut saw_flight = false;
    for callback in 1..=32_u64 {
        let now = callback * 50;
        if callback == 2 {
            input["seq"] = json!(1);
            input["nade"] = json!(1);
            command(&mut hub, &op, input.clone(), now);
        }
        hub.poll_native(now, now * 1_000_000);
        assert!(hub.native_status("ABC").unwrap().fault.is_none());
        let commanders = drain_commanders(&mut hub, &mut receivers, now);
        let operative = messages(&mut receiver);
        let scheduled = commanders[0]
            .iter()
            .filter(|message| message["t"] == "s")
            .count();
        let delivered = operative
            .iter()
            .filter(|message| message["t"] == "s")
            .collect::<Vec<_>>();
        assert_eq!(delivered.len(), scheduled);
        for message in delivered {
            assert_operative_frame(message, &state(&hub).companion_status("friend"));
            for event in message["shots"].as_array().unwrap() {
                if event["f"] == soldier {
                    saw_throw |= event["k"] == "throw";
                    saw_fire |= event["k"] != "throw";
                }
            }
            saw_flight |= message["flights"]
                .as_array()
                .is_some_and(|rows| !rows.is_empty());
            if frames == 0 {
                assert_eq!(message["all"], true);
            }
            frames += 1;
        }
    }
    assert!(frames > 0);
    assert!(saw_fire && saw_throw && saw_flight);
    assert!(state(&hub).companions.get("friend").unwrap().shots > 0);
    assert_eq!(state(&hub).companions.soldiers[&soldier].nades, 2);
    assert!(state(&hub).firing.flights.is_empty());
    let moved = &state(&hub).world.game.units[&soldier];
    assert!((moved.x - before.0).abs() + (moved.z - before.1).abs() > 0.01);
    let paused_tick = hub.native_status("ABC").unwrap().tick;
    hub.rooms
        .get_mut("ABC")
        .unwrap()
        .native
        .as_mut()
        .unwrap()
        .runtime
        .pause(0, "a", crate::room_runtime::PauseReason::Host, 1600)
        .unwrap();
    let mut pause_frames = 0;
    for callback in 33..=52_u64 {
        let now = callback * 50;
        hub.poll_native(now, now * 1_000_000);
        let commanders = drain_commanders(&mut hub, &mut receivers, now);
        let operative = messages(&mut receiver);
        let scheduled = commanders[0]
            .iter()
            .filter(|message| message["t"] == "s")
            .count();
        let delivered = operative
            .iter()
            .filter(|message| message["t"] == "s")
            .collect::<Vec<_>>();
        assert_eq!(delivered.len(), scheduled);
        for message in delivered {
            assert_operative_frame(message, &state(&hub).companion_status("friend"));
            pause_frames += 1;
        }
    }
    assert_eq!(pause_frames, 1);
    assert_eq!(hub.native_status("ABC").unwrap().tick, paused_tick);
    let runtime = &mut hub
        .rooms
        .get_mut("ABC")
        .unwrap()
        .native
        .as_mut()
        .unwrap()
        .runtime;
    runtime.resume(2600);
    let win_vp = runtime.state().start.win_vp;
    runtime.state_mut().start.economy.players[0].vp = win_vp;
    runtime
        .state_mut()
        .apply_conquest_victory()
        .unwrap()
        .unwrap();
    let mut ending_frames = 0;
    for callback in 53..=172_u64 {
        let now = callback * 50;
        hub.poll_native(now, now * 1_000_000);
        assert!(hub.native_status("ABC").unwrap().fault.is_none());
        let commanders = drain_commanders(&mut hub, &mut receivers, now);
        let operative = messages(&mut receiver);
        let scheduled = commanders[0]
            .iter()
            .filter(|message| message["t"] == "s")
            .count();
        let delivered = operative
            .iter()
            .filter(|message| message["t"] == "s")
            .collect::<Vec<_>>();
        assert_eq!(delivered.len(), scheduled);
        for message in delivered {
            assert_eq!(message["operative"]["id"], soldier);
            for key in [
                "movement",
                "queues",
                "productionJobs",
                "plans",
                "orders",
                "air",
                "rally",
                "works",
                "covers",
                "sup",
            ] {
                assert!(message.get(key).is_none(), "ending operative leaked {key}");
            }
            ending_frames += 1;
        }
        if callback == 172 {
            let frame_index = operative
                .iter()
                .rposition(|message| message["t"] == "s")
                .unwrap();
            let lobby_index = operative
                .iter()
                .position(|message| message["t"] == "lobby")
                .unwrap();
            assert!(frame_index < lobby_index);
            assert_eq!(operative[lobby_index]["role"], "operative");
            assert_eq!(operative[lobby_index]["result"]["you"], Value::Null);
        }
    }
    assert_eq!(ending_frames, 60);
    assert!(hub.rooms["ABC"].native.is_none());
    assert_eq!(hub.native_status("ABC").unwrap().held, 120);
    assert_eq!(lobby(&hub, op.id)["role"], "operative");
    let next = hub
        .attach_native("ABC", native_tests::native_match(), 8650)
        .unwrap();
    assert!(next > epoch);
    let starts = drain_commanders(&mut hub, &mut receivers, 8650);
    let commander_start = starts[0]
        .iter()
        .find(|message| message["t"] == "start")
        .unwrap();
    let joined = messages(&mut receiver);
    let start = joined
        .iter()
        .find(|message| message["t"] == "start")
        .unwrap();
    assert_start(
        start,
        commander_start,
        0,
        state(&hub).companions.get("friend").unwrap().unit_id,
    );
    assert_eq!(start["matchId"], next);
}

#[test]
fn operative_reconnect_keeps_soldier_resets_sequence_and_resyncs_only_its_stream() {
    let (mut hub, commanders, mut receivers) = room();
    let (first, mut first_receiver) = client(3);
    let (other, mut other_receiver) = client(4);
    join_operative(&mut hub, &first, "friend", 0);
    join_operative(&mut hub, &other, "other", 0);
    hub.attach_native("ABC", native_tests::native_match(), 0)
        .unwrap();
    messages(&mut first_receiver);
    messages(&mut other_receiver);
    drain_commanders(&mut hub, &mut receivers, 0);
    let soldier = state(&hub).companions.get("friend").unwrap().unit_id;
    command(&mut hub, &first, fps(9), 20);
    assert_eq!(last_seq(&hub, "friend"), 9);
    hub.poll_native(0, 0);
    for callback in 1..=4_u64 {
        let now = callback * 50;
        hub.poll_native(now, now * 1_000_000);
        drain_commanders(&mut hub, &mut receivers, now);
        messages(&mut first_receiver);
        messages(&mut other_receiver);
    }
    hub.disconnected("ABC", first.id, 201);
    assert!(!hub.native_status("ABC").unwrap().paused);
    assert!(!state(&hub).companions.get("friend").unwrap().online);
    assert!(
        state(&hub)
            .companions
            .get("friend")
            .unwrap()
            .input
            .is_none()
    );
    let (replacement, mut replacement_receiver) = client(5);
    // A returning token wins over a requested commander role and a different owner.
    assert!(hello(
        &mut hub,
        &replacement,
        json!({"t":"hello","token":"friend","name":"Returned","role":"commander","commander":1}),
        202
    ));
    assert_eq!(hub.rooms["ABC"].players.len(), 2);
    assert_eq!(hub.rooms["ABC"].operatives.len(), 2);
    assert_eq!(hub.rooms["ABC"].operatives[0].commander, Some(0));
    let returned = state(&hub).companions.get("friend").unwrap();
    assert_eq!(returned.unit_id, soldier);
    assert!(returned.online);
    assert_eq!(returned.last_seq, -1);
    assert!(returned.input.is_none());
    assert_eq!(hub.rooms["ABC"].operatives[0].input_at, 0);
    let returned = messages(&mut replacement_receiver);
    let start = returned
        .iter()
        .find(|message| message["t"] == "start")
        .unwrap();
    assert_eq!(start["operative"]["id"], soldier);
    assert_eq!(start["operative"]["seq"], -1);
    command(&mut hub, &first, fps(99), 220);
    assert_eq!(last_seq(&hub, "friend"), -1);
    command(&mut hub, &replacement, fps(0), 220);
    assert_eq!(last_seq(&hub, "friend"), 0);
    let mut reconnect_full = false;
    for callback in 5..=8_u64 {
        let now = callback * 50;
        hub.poll_native(now, now * 1_000_000);
        drain_commanders(&mut hub, &mut receivers, now);
        let frames = messages(&mut replacement_receiver);
        if let Some(frame) = frames.iter().find(|message| message["t"] == "s")
            && !reconnect_full
        {
            assert_eq!(frame["all"], true);
            reconnect_full = true;
        }
        messages(&mut other_receiver);
    }
    assert!(reconnect_full);
    let commander_history = hub.accounts[&commanders[0].id].delivery.clone();
    let other_history = hub.rooms["ABC"].operatives[1].net.clone();
    command(&mut hub, &replacement, json!({"t":"resync"}), 401);
    assert_eq!(hub.accounts[&commanders[0].id].delivery, commander_history);
    assert_eq!(hub.rooms["ABC"].operatives[1].net, other_history);
    let mut resynced = false;
    for callback in 9..=12_u64 {
        let now = callback * 50;
        hub.poll_native(now, now * 1_000_000);
        assert!(hub.native_status("ABC").unwrap().fault.is_none());
        let commanders = drain_commanders(&mut hub, &mut receivers, now);
        let first = messages(&mut replacement_receiver);
        let other = messages(&mut other_receiver);
        for frame in first.iter().filter(|message| message["t"] == "s") {
            if !resynced {
                assert_eq!(frame["all"], true);
                assert_eq!(
                    other
                        .iter()
                        .find(|message| message["t"] == "s")
                        .unwrap()
                        .get("all"),
                    None
                );
                assert_eq!(
                    commanders[0]
                        .iter()
                        .find(|message| message["t"] == "s")
                        .unwrap()
                        .get("all"),
                    None
                );
                resynced = true;
            }
        }
    }
    assert!(resynced);
}

#[test]
fn operative_trusted_room_admission_preserves_refusal_and_live_token_takeover() {
    let (mut hub, commanders, mut receivers) = room();
    hub.configure_trusted_room(Some(crate::TrustedRoomConfig::new("ABC", 3936341).unwrap()));
    hub.start_trusted("ABC", commanders[0].id, false, 0)
        .unwrap();
    drain_commanders(&mut hub, &mut receivers, 0);
    let (invalid, mut invalid_receiver) = client(3);
    assert!(!hello(
        &mut hub,
        &invalid,
        json!({
            "t":"hello", "token":"", "name":"Invalid", "role":"operative", "commander":0
        }),
        0
    ));
    assert_join_refused(&mut invalid_receiver);
    let (old, mut old_receiver) = client(4);
    join_operative(&mut hub, &old, "friend", 0);
    let soldier = state(&hub).companions.get("friend").unwrap().unit_id;
    messages(&mut old_receiver);
    let (current, mut current_receiver) = client(5);
    assert!(hello(
        &mut hub,
        &current,
        json!({
            "t":"hello", "token":"friend", "name":"Returned", "role":"ignored", "commander":1
        }),
        1
    ));
    assert!(
        matches!(old_receiver.try_recv(), Ok(Output::Json(value)) if value == json!({"t":"replaced"}))
    );
    assert!(matches!(
        old_receiver.try_recv(),
        Ok(Output::Close(1000, ""))
    ));
    assert_eq!(
        state(&hub).companions.get("friend").unwrap().unit_id,
        soldier
    );
    assert_eq!(lobby(&hub, current.id)["role"], "operative");
    assert_eq!(lobby(&hub, current.id)["commander"], 0);
    assert!(hub.native_status("ABC").unwrap().fault.is_none());
    messages(&mut current_receiver);
    command(&mut hub, &current, json!({"t":"restart"}), 2);
    assert!(messages(&mut current_receiver).is_empty());
}

#[test]
fn operative_truncated_utf16_tokens_keep_distinct_soldiers_and_input() {
    let (mut hub, _, mut commanders) = room();
    hub.attach_native("ABC", native_tests::native_match(), 0)
        .unwrap();
    drain_commanders(&mut hub, &mut commanders, 0);
    let prefix = "x".repeat(39);
    let tokens = [format!("{prefix}😀"), format!("{prefix}�")];
    let (first, mut first_receiver) = client(3);
    let (second, mut second_receiver) = client(4);
    for (owner, (connection, token)) in [(&first, &tokens[0]), (&second, &tokens[1])]
        .into_iter()
        .enumerate()
    {
        assert!(hello(
            &mut hub,
            connection,
            json!({
                "t":"hello", "token":token, "name":"Friend", "role":"operative", "commander":owner
            }),
            0
        ));
    }
    let first_start = messages(&mut first_receiver)
        .into_iter()
        .find(|m| m["t"] == "start")
        .unwrap();
    let second_start = messages(&mut second_receiver)
        .into_iter()
        .find(|m| m["t"] == "start")
        .unwrap();
    assert_ne!(
        first_start["operative"]["id"],
        second_start["operative"]["id"]
    );
    assert_eq!(state(&hub).companions.records.len(), 2);
    command(&mut hub, &first, fps(0), 20);
    command(&mut hub, &second, fps(7), 20);
    for (owner, seq) in [(0, 0), (1, 7)] {
        let record = state(&hub)
            .companions
            .records
            .values()
            .find(|op| op.owner == owner)
            .unwrap();
        assert_eq!(record.last_seq, seq);
        assert_eq!(state(&hub).world.game.units[&record.unit_id].owner, owner);
    }
}
