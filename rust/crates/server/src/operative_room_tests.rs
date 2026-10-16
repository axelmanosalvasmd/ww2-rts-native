use super::*;
use ww2_core::recipient_snapshot::{DeliveryState, OperativeDelivery, SelectedSeat};

fn assert_every_recipient_tick(
    state: &mut Match,
    observer: &mut DeliveryState,
    operatives: &mut [OperativeDelivery; 2],
) {
    let selected = (0..2)
        .map(|slot| SelectedSeat {
            slot,
            // Full reads preserve the production recipient's terrain outbox.
            full: true,
            omit: false,
        })
        .collect::<Vec<_>>();
    let tick = state.world.game.tick;
    let frame = observer
        .capture_selected_with_tick_events(state, &selected, &[])
        .unwrap()
        .unwrap();
    for slot in 0..2 {
        let snapshot = frame.recipient(slot).unwrap().into_json();
        assert_eq!(snapshot["tick"], tick);
        assert!(snapshot["units"].is_array());
    }
    drop(frame);
    for stream in operatives {
        let snapshot = stream.capture(state, false, &[]).unwrap();
        assert_eq!(snapshot["tick"], tick);
        assert!(snapshot["units"].is_array());
    }
}

fn assert_lobbies(output: &[Value], rosters: &[Value]) {
    assert_eq!(output.len(), rosters.len());
    for (message, roster) in output.iter().zip(rosters) {
        assert_eq!(message["t"], "lobby");
        assert_eq!(&message["operatives"], roster);
        for private in ["token", "input", "denies", "net", "view"] {
            assert!(
                message["operatives"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|op| op.get(private).is_none())
            );
        }
    }
}

#[test]
fn net25_operative_live_takeover_preserves_person_and_ignores_old_socket_close() {
    // Prototype server.js:596-603,614,620,719. A live takeover differs from
    // an offline reconnect: closing the replaced socket must not drop its successor.
    for playing in [false, true] {
        let (mut hub, _, mut commanders) = room();
        let (old, mut old_rx) = client(3);
        let (peer, mut peer_rx) = client(4);
        join_operative(&mut hub, &old, "friend", 0);
        join_operative(&mut hub, &peer, "peer", 1);
        if playing {
            hub.attach_native("ABC", native_tests::native_match(), 0)
                .unwrap();
            command(&mut hub, &old, fps(9), 20);
        }
        messages(&mut old_rx);
        messages(&mut peer_rx);
        drain_commanders(&mut hub, &mut commanders, 20);
        let person = &mut hub.rooms.get_mut("ABC").unwrap().operatives[0].person;
        person.denies = vec![1, 2, 3];
        person.rtt = Some(17.);
        let soldier = playing.then(|| state(&hub).companions.get("friend").unwrap().unit_id);
        let (current, mut current_rx) = client(5);
        assert!(hello(
            &mut hub,
            &current,
            json!({
                "t":"hello","token":"friend","name":" <Returned> ",
                "role":"commander","commander":1,"spectate":true
            }),
            21
        ));
        assert!(
            matches!(old_rx.try_recv(), Ok(Output::Json(value)) if value == json!({"t":"replaced"}))
        );
        assert!(matches!(old_rx.try_recv(), Ok(Output::Close(1000, ""))));
        assert!(old_rx.try_recv().is_err());
        let output = messages(&mut current_rx);
        assert_eq!(
            output
                .iter()
                .map(|m| m["t"].as_str().unwrap())
                .collect::<Vec<_>>(),
            if playing {
                vec!["start", "lobby"]
            } else {
                vec!["lobby"]
            }
        );
        if let Some(soldier) = soldier {
            assert_eq!(output[0]["operative"]["id"], soldier);
            assert_eq!(output[0]["operative"]["seq"], -1);
            assert_eq!(
                state(&hub).companions.get("friend").unwrap().unit_id,
                soldier
            );
        }
        let operative = &hub.rooms["ABC"].operatives[0];
        assert_eq!(operative.commander, Some(0));
        assert_eq!(operative.person.name, "Returned");
        assert_eq!(operative.person.denies, [1, 2, 3]);
        assert_eq!(operative.person.rtt, Some(17.));
        assert_eq!(operative.person.connection.as_ref().unwrap().id, current.id);
        assert_eq!(hub.rooms["ABC"].players.len(), 2);
        assert_eq!(hub.rooms["ABC"].operatives.len(), 2);
        assert!(hub.rooms["ABC"].spectators.is_empty());
        messages(&mut peer_rx);
        drain_commanders(&mut hub, &mut commanders, 21);
        for stale in [
            fps(99),
            json!({"t":"name","name":"Stale"}),
            json!({"t":"restart"}),
        ] {
            command(&mut hub, &old, stale, 40);
        }
        hub.disconnected("ABC", old.id, 41);
        assert!(messages(&mut current_rx).is_empty());
        assert!(messages(&mut peer_rx).is_empty());
        assert!(
            drain_commanders(&mut hub, &mut commanders, 41)
                .iter()
                .all(Vec::is_empty)
        );
        assert_eq!(
            lobby(&hub, current.id)["operatives"][0],
            json!({"name":"Returned","commander":0,"online":true})
        );
        if playing {
            assert_eq!(last_seq(&hub, "friend"), -1);
            command(&mut hub, &current, fps(0), 42);
            assert_eq!(last_seq(&hub, "friend"), 0);
            assert!(!hub.native_status("ABC").unwrap().paused);
            assert!(hub.native_status("ABC").unwrap().fault.is_none());
        }
    }
}

#[test]
fn net28_net29_operative_broadcasts_keep_change_order_and_shifted_commander_identity() {
    // Prototype server.js:194,338-350,635,638,718-725. Assert actual broadcasts,
    // including a non-final freed seat, rather than calling the lobby projector.
    let (mut hub, seats, mut commanders) = room();
    let (third, mut third_rx) = client(3);
    assert!(hello(
        &mut hub,
        &third,
        json!({"t":"hello","token":"c","name":"c"}),
        0
    ));
    let (first, mut first_rx) = client(4);
    let (second, mut second_rx) = client(5);
    join_operative(&mut hub, &first, "first", 2);
    join_operative(&mut hub, &second, "second", 0);
    messages(&mut first_rx);
    messages(&mut second_rx);
    messages(&mut third_rx);
    drain_commanders(&mut hub, &mut commanders, 0);
    let mut roster = json!([
        {"name":"first","commander":2,"online":true},
        {"name":"second","commander":0,"online":true}
    ]);
    let mut expected = Vec::new();
    for (connection, name, cleaned) in [
        (&first, " <>&\"' ", "Soldier"),
        (
            &second,
            "  abcdefghijklmnopqrstuvwxyz  ",
            "abcdefghijklmnop",
        ),
        (&first, " <Final> ", "Final"),
    ] {
        command(&mut hub, connection, json!({"t":"name","name":name}), 1);
        roster[usize::from(connection.id == second.id)]["name"] = json!(cleaned);
        expected.push(roster.clone());
    }
    for bad in [Value::Null, json!(false), json!(123), json!([]), json!({})] {
        command(&mut hub, &first, json!({"t":"name","name":bad}), 2);
    }
    for output in drain_commanders(&mut hub, &mut commanders, 2) {
        assert_lobbies(&output, &expected);
    }
    assert_lobbies(&messages(&mut third_rx), &expected);
    for (connection, receiver) in [(&first, &mut first_rx), (&second, &mut second_rx)] {
        let output = messages(receiver);
        assert_lobbies(&output, &expected);
        for message in output {
            assert_eq!(message["role"], "operative");
            assert_eq!(message["you"], -1);
            assert_eq!(message["spectator"], false);
            assert_eq!(message["amHost"], false);
            assert_eq!(
                message["commander"],
                if connection.id == first.id { 2 } else { 0 }
            );
        }
    }
    hub.disconnected("ABC", seats[1].id, 3);
    command(&mut hub, &seats[0], json!({"t":"kick","slot":1}), 4);
    roster[0]["commander"] = json!(1);
    let shifted = messages(&mut first_rx);
    assert_eq!(shifted.len(), 2);
    assert_lobbies(&shifted[1..], std::slice::from_ref(&roster));
    assert_eq!(shifted[1]["commander"], 1);
    assert_eq!(shifted[1]["players"][1]["name"], "c");
    messages(&mut second_rx);
    hub.disconnected("ABC", third.id, 5);
    command(&mut hub, &seats[0], json!({"t":"kick","slot":1}), 6);
    roster[0]["commander"] = json!(-1);
    let removed = messages(&mut first_rx);
    assert_lobbies(&removed[1..], std::slice::from_ref(&roster));
    assert_eq!(removed[1]["commander"], -1);
    let (replacement, _replacement_rx) = client(6);
    assert!(hello(
        &mut hub,
        &replacement,
        json!({"t":"hello","token":"d","name":"d"}),
        7
    ));
    let output = messages(&mut first_rx);
    assert_lobbies(&output, &[roster]);
    assert_eq!(output[0]["commander"], -1);
    assert_eq!(output[0]["players"][1]["name"], "d");
}

#[test]
fn net82_operative_offline_identity_survives_lobby_cleanup_deadline() {
    // Prototype server.js:568-575,718-725. Only seats receive a cleanup timer.
    let (mut hub, _, mut commanders) = room();
    let (op, mut receiver) = client(3);
    let (peer, mut peer_rx) = client(4);
    join_operative(&mut hub, &op, "friend", 0);
    join_operative(&mut hub, &peer, "peer", 1);
    messages(&mut receiver);
    messages(&mut peer_rx);
    drain_commanders(&mut hub, &mut commanders, 0);
    hub.disconnected("ABC", op.id, 1);
    let roster = json!([
        {"name":"friend","commander":0,"online":false},
        {"name":"peer","commander":1,"online":true}
    ]);
    assert!(messages(&mut receiver).is_empty());
    assert_lobbies(&messages(&mut peer_rx), std::slice::from_ref(&roster));
    for output in drain_commanders(&mut hub, &mut commanders, 1) {
        assert_lobbies(&output, std::slice::from_ref(&roster));
    }
    hub.expire(10_002);
    assert_eq!(hub.rooms["ABC"].operatives.len(), 2);
    assert_eq!(hub.rooms["ABC"].empty_since, None);
    let (returned, mut returned_rx) = client(5);
    assert!(hello(
        &mut hub,
        &returned,
        json!({"t":"hello","token":"friend","name":"Back"}),
        10_003
    ));
    let mut roster = roster;
    roster[0]["name"] = json!("Back");
    roster[0]["online"] = json!(true);
    assert_lobbies(&messages(&mut returned_rx), &[roster]);
    assert_eq!(hub.rooms["ABC"].operatives.len(), 2);
    assert_eq!(lobby(&hub, returned.id)["role"], "operative");
}

#[test]
fn operative_room_members_deliver_every_tick_through_natural_finish_lobby_and_restart() {
    let (mut hub, seats, mut commanders) = room();
    let (first, mut first_rx) = client(3);
    let (second, mut second_rx) = client(4);
    join_operative(&mut hub, &first, "first", 0);
    join_operative(&mut hub, &second, "second", 1);
    let mut scene = native_tests::native_match();
    for control in scene.commands.units.values_mut() {
        control.hold_fire = true;
        control.auto_cast = false;
        control.auto_retreat = false;
    }
    let epoch = hub.attach_native("ABC", scene, 0).unwrap();
    // After the authored startup has been checked, arrange one held objective
    // to earn victory through the ordinary VP-income phase.
    let scene = hub
        .rooms
        .get_mut("ABC")
        .unwrap()
        .native
        .as_mut()
        .unwrap()
        .runtime
        .state_mut();
    for (index, point) in scene.start.points.iter_mut().enumerate() {
        point.owner = Some(0);
        point.progress = 1.;
        point.vp = if index == 0 { 1. } else { 0. };
    }
    scene.start.win_vp = 12.;
    messages(&mut first_rx);
    messages(&mut second_rx);
    drain_commanders(&mut hub, &mut commanders, 0);
    let first_id = state(&hub).companions.get("first").unwrap().unit_id;
    let second_id = state(&hub).companions.get("second").unwrap().unit_id;
    let mut observer = DeliveryState::new(state(&hub)).unwrap();
    let mut op_observers = [
        OperativeDelivery::new(state(&hub), 0).unwrap(),
        OperativeDelivery::new(state(&hub), 1).unwrap(),
    ];
    let (returned, mut returned_rx) = client(5);
    let mut last_tick = 0;
    let mut finish_tick = None;
    let mut snapshot_ticks = 0;
    let mut ending_frames = 0;
    let mut final_lobbies = 0;
    let mut last_now = 0;
    for callback in 0..=1000_u64 {
        let now = callback * 50;
        last_now = now;
        if callback == 3 {
            command(&mut hub, &first, fps(8), now);
            hub.disconnected("ABC", first.id, now);
            assert!(!state(&hub).companions.get("first").unwrap().online);
            assert!(state(&hub).companions.get("first").unwrap().input.is_none());
        }
        if callback == 8 {
            assert!(hello(
                &mut hub,
                &returned,
                json!({"t":"hello","token":"first","name":"Returned"}),
                now
            ));
            assert_eq!(
                state(&hub).companions.get("first").unwrap().unit_id,
                first_id
            );
            assert_eq!(last_seq(&hub, "first"), -1);
            let output = messages(&mut returned_rx);
            assert_eq!(output[0]["t"], "start");
            assert_eq!(output[1]["t"], "lobby");
        }
        hub.poll_native(now, now * 1_000_000);
        let status = hub.native_status("ABC").unwrap();
        assert!(status.fault.is_none());
        assert!(!status.paused);
        if let Some(native) = hub.rooms.get_mut("ABC").unwrap().native.as_mut() {
            let state = native.runtime.state_mut();
            if state.world.game.tick != last_tick {
                assert_eq!(state.world.game.tick, last_tick + 1);
                last_tick = state.world.game.tick;
                assert_every_recipient_tick(state, &mut observer, &mut op_observers);
                assert_eq!(state.companion_status("first")["id"], first_id);
                assert_eq!(state.companion_status("second")["id"], second_id);
                snapshot_ticks += 1;
            }
            if let Some(finish) = &state.finish {
                assert_eq!(finish.winner, 0);
                finish_tick.get_or_insert(last_tick);
            }
        }
        let seat_output = drain_commanders(&mut hub, &mut commanders, now);
        let old_output = messages(&mut first_rx);
        if callback >= 3 {
            assert!(old_output.is_empty());
        }
        let first_output = if callback >= 8 {
            messages(&mut returned_rx)
        } else {
            old_output
        };
        let second_output = messages(&mut second_rx);
        let scheduled = seat_output[0].iter().filter(|m| m["t"] == "s").count();
        assert_eq!(
            seat_output[1].iter().filter(|m| m["t"] == "s").count(),
            scheduled
        );
        assert_eq!(
            second_output.iter().filter(|m| m["t"] == "s").count(),
            scheduled
        );
        assert_eq!(
            first_output.iter().filter(|m| m["t"] == "s").count(),
            if (3..8).contains(&callback) {
                0
            } else {
                scheduled
            }
        );
        if finish_tick.is_some() {
            ending_frames += scheduled;
        }
        for (output, commander) in [(&first_output, 0), (&second_output, 1)] {
            for message in output
                .iter()
                .filter(|m| m["t"] == "lobby" && m["state"] == "lobby")
            {
                final_lobbies += 1;
                assert_eq!(message["role"], "operative");
                assert_eq!(message["commander"], commander);
                assert_eq!(message["you"], -1);
                assert_eq!(message["result"]["you"], Value::Null);
                assert_eq!(
                    message["operatives"],
                    json!([
                        {"name":"Returned","commander":0,"online":true},
                        {"name":"second","commander":1,"online":true}
                    ])
                );
            }
        }
        if hub.rooms["ABC"].native.is_none() {
            break;
        }
    }
    assert!(snapshot_ticks >= 240);
    assert!(hub.native_status("ABC").unwrap().tick >= finish_tick.unwrap() + 60);
    assert!(ending_frames >= 60);
    assert_eq!(final_lobbies, 2);
    assert!(hub.rooms["ABC"].native.is_none());
    assert_eq!(hub.native_status("ABC").unwrap().held, 120);
    hub.configure_trusted_room(Some(crate::TrustedRoomConfig::new("ABC", 3936341).unwrap()));
    for restart in [false, true] {
        last_now += 50;
        let next = hub
            .start_trusted("ABC", seats[0].id, restart, last_now)
            .unwrap();
        assert!(next > epoch);
        let seat_starts = drain_commanders(&mut hub, &mut commanders, last_now);
        for (slot, output) in [messages(&mut returned_rx), messages(&mut second_rx)]
            .iter()
            .enumerate()
        {
            let lobby_at = output.iter().position(|m| m["t"] == "lobby").unwrap();
            let start_at = output.iter().position(|m| m["t"] == "start").unwrap();
            assert!(lobby_at < start_at);
            assert_eq!(output[lobby_at]["result"], Value::Null);
            let seat_start = seat_starts[slot]
                .iter()
                .find(|m| m["t"] == "start")
                .unwrap();
            let key = if slot == 0 { "first" } else { "second" };
            assert_start(
                &output[start_at],
                seat_start,
                slot,
                state(&hub).companions.get(key).unwrap().unit_id,
            );
            assert_eq!(output[start_at]["matchId"], next);
        }
        let mut observer = DeliveryState::new(state(&hub)).unwrap();
        let mut op_observers = [
            OperativeDelivery::new(state(&hub), 0).unwrap(),
            OperativeDelivery::new(state(&hub), 1).unwrap(),
        ];
        for offset in 0..=4 {
            let now = last_now + offset * 50;
            hub.poll_native(now, now * 1_000_000);
            let state = hub
                .rooms
                .get_mut("ABC")
                .unwrap()
                .native
                .as_mut()
                .unwrap()
                .runtime
                .state_mut();
            assert_every_recipient_tick(state, &mut observer, &mut op_observers);
            drain_commanders(&mut hub, &mut commanders, now);
            messages(&mut returned_rx);
            messages(&mut second_rx);
            assert!(hub.native_status("ABC").unwrap().fault.is_none());
        }
        last_now += 200;
    }
}

#[test]
fn fps39_loaded_operatives_keep_identity_through_finish_start_and_restart() {
    // Prototype server.js:436-450,411. Loading replaces the seat array, while
    // each operative keeps the same commander person and receives a fresh start.
    struct SaveDir(std::path::PathBuf);
    impl Drop for SaveDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let dir =
        SaveDir(std::env::temp_dir().join(format!("ww2-operative-load-{}", std::process::id())));
    let (mut hub, seats, mut commanders) = room();
    hub.configure_saves(dir.0.clone());
    let (op, mut receiver) = client(3);
    let (peer, mut peer_rx) = client(4);
    join_operative(&mut hub, &op, "friend", 0);
    join_operative(&mut hub, &peer, "peer", 1);
    hub.attach_native("ABC", native_tests::native_match(), 0)
        .unwrap();
    let soldier = state(&hub).companions.get("friend").unwrap().unit_id;
    {
        let state = hub
            .rooms
            .get_mut("ABC")
            .unwrap()
            .native
            .as_mut()
            .unwrap()
            .runtime
            .state_mut();
        for control in state.commands.units.values_mut() {
            control.hold_fire = true;
            control.auto_cast = false;
            control.auto_retreat = false;
        }
    }
    command(&mut hub, &op, fps(8), 20);
    let saved = hub.save_room("ABC", seats[0].id).unwrap();
    messages(&mut receiver);
    messages(&mut peer_rx);
    drain_commanders(&mut hub, &mut commanders, 20);
    {
        let room = hub.rooms.get_mut("ABC").unwrap();
        room.native = None;
        // Model a lobby whose present people differ from saved-seat order.
        room.players.swap(0, 1);
        room.operatives[0].commander = Some(1);
        room.operatives[1].commander = Some(0);
    }
    assert!(hub.load_room("ABC", seats[1].id, "missing", 49).is_err());
    assert_eq!(hub.rooms["ABC"].operatives[0].commander, Some(1));
    assert_eq!(hub.rooms["ABC"].operatives[1].commander, Some(0));
    assert!(messages(&mut receiver).is_empty());
    assert!(messages(&mut peer_rx).is_empty());
    let epoch = hub.load_room("ABC", seats[1].id, &saved, 50).unwrap();
    assert!(hub.native_status("ABC").unwrap().paused);
    let seat_output = drain_commanders(&mut hub, &mut commanders, 50);
    for (slot, output) in [messages(&mut receiver), messages(&mut peer_rx)]
        .iter()
        .enumerate()
    {
        let lobby = output.iter().find(|m| m["t"] == "lobby").unwrap();
        let start = output.iter().find(|m| m["t"] == "start").unwrap();
        assert_eq!(lobby["commander"], slot);
        assert_eq!(lobby["role"], "operative");
        assert_eq!(start["matchId"], epoch);
        let commander_start = seat_output[slot]
            .iter()
            .find(|m| m["t"] == "start")
            .unwrap();
        let key = if slot == 0 { "friend" } else { "peer" };
        assert_start(
            start,
            commander_start,
            slot,
            state(&hub).companions.get(key).unwrap().unit_id,
        );
        assert_eq!(output.last().unwrap()["t"], "pause");
        assert_eq!(output.last().unwrap()["paused"], true);
    }
    assert_eq!(
        state(&hub).companions.get("friend").unwrap().unit_id,
        soldier
    );
    assert_eq!(last_seq(&hub, "friend"), -1);
    assert_eq!(hub.rooms["ABC"].operatives.len(), 2);
    // Operatives cannot use the loaded-save host-resume handler.
    command(&mut hub, &op, json!({"t":"resume"}), 51);
    assert!(messages(&mut receiver).is_empty());
    assert!(hub.native_status("ABC").unwrap().paused);
    command(&mut hub, &seats[0], json!({"t":"resume"}), 52);
    assert!(!hub.native_status("ABC").unwrap().paused);
    {
        // Preserve the authored save layout through load admission, then arrange
        // ordinary VP income to exercise the complete loaded-room lifecycle.
        let state = hub
            .rooms
            .get_mut("ABC")
            .unwrap()
            .native
            .as_mut()
            .unwrap()
            .runtime
            .state_mut();
        for (index, point) in state.start.points.iter_mut().enumerate() {
            point.owner = Some(0);
            point.progress = 1.;
            point.vp = if index == 0 { 1. } else { 0. };
        }
        state.start.win_vp = 1.;
    }
    let mut observer = DeliveryState::new(state(&hub)).unwrap();
    let mut op_observers = [
        OperativeDelivery::new(state(&hub), 0).unwrap(),
        OperativeDelivery::new(state(&hub), 1).unwrap(),
    ];
    let mut finish_tick = None;
    let mut final_lobbies = 0;
    let mut last_tick = -1;
    let mut last_now = 100;
    for callback in 0..=1000_u64 {
        let now = 100 + callback * 50;
        last_now = now;
        hub.poll_native(now, (now - 100) * 1_000_000);
        if let Some(native) = hub.rooms.get_mut("ABC").unwrap().native.as_mut() {
            let state = native.runtime.state_mut();
            if state.world.game.tick != last_tick {
                if last_tick >= 0 {
                    assert_eq!(state.world.game.tick, last_tick + 1);
                }
                assert_every_recipient_tick(state, &mut observer, &mut op_observers);
                last_tick = state.world.game.tick;
            }
            if let Some(finish) = &state.finish {
                assert_eq!(finish.winner, 0);
                finish_tick.get_or_insert(last_tick);
            }
        }
        drain_commanders(&mut hub, &mut commanders, now);
        for (slot, output) in [messages(&mut receiver), messages(&mut peer_rx)]
            .iter()
            .enumerate()
        {
            for lobby in output
                .iter()
                .filter(|m| m["t"] == "lobby" && m["state"] == "lobby")
            {
                final_lobbies += 1;
                assert_eq!(lobby["role"], "operative");
                assert_eq!(lobby["commander"], slot);
                assert_eq!(lobby["spectator"], false);
                assert_eq!(lobby["result"]["you"], Value::Null);
            }
        }
        assert!(hub.native_status("ABC").unwrap().fault.is_none());
        assert_eq!(hub.rooms["ABC"].operatives.len(), 2);
        if hub.rooms["ABC"].native.is_none() {
            break;
        }
    }
    assert_eq!(final_lobbies, 2);
    assert_eq!(hub.native_status("ABC").unwrap().held, 120);
    assert!(last_tick >= finish_tick.unwrap() + 59);
    assert!(hub.rooms["ABC"].native.is_none());
    hub.configure_trusted_room(Some(crate::TrustedRoomConfig::new("ABC", 3936341).unwrap()));
    let mut previous_epoch = epoch;
    for restart in [false, true] {
        last_now += 50;
        let next = hub
            .start_trusted("ABC", seats[0].id, restart, last_now)
            .unwrap();
        assert!(next > previous_epoch);
        previous_epoch = next;
        let seat_starts = drain_commanders(&mut hub, &mut commanders, last_now);
        for (slot, output) in [messages(&mut receiver), messages(&mut peer_rx)]
            .iter()
            .enumerate()
        {
            let lobby_at = output.iter().position(|m| m["t"] == "lobby").unwrap();
            let start_at = output.iter().position(|m| m["t"] == "start").unwrap();
            assert!(lobby_at < start_at);
            assert_eq!(output[lobby_at]["role"], "operative");
            assert_eq!(output[lobby_at]["result"], Value::Null);
            let seat_start = seat_starts[slot]
                .iter()
                .find(|m| m["t"] == "start")
                .unwrap();
            let key = if slot == 0 { "friend" } else { "peer" };
            assert_start(
                &output[start_at],
                seat_start,
                slot,
                state(&hub).companions.get(key).unwrap().unit_id,
            );
            assert_eq!(output[start_at]["matchId"], next);
            assert_eq!(output[start_at]["operative"]["seq"], -1);
        }
        let mut observer = DeliveryState::new(state(&hub)).unwrap();
        let mut op_observers = [
            OperativeDelivery::new(state(&hub), 0).unwrap(),
            OperativeDelivery::new(state(&hub), 1).unwrap(),
        ];
        for offset in 0..=4 {
            let now = last_now + offset * 50;
            hub.poll_native(now, (now - 100) * 1_000_000);
            let state = hub
                .rooms
                .get_mut("ABC")
                .unwrap()
                .native
                .as_mut()
                .unwrap()
                .runtime
                .state_mut();
            assert_every_recipient_tick(state, &mut observer, &mut op_observers);
            drain_commanders(&mut hub, &mut commanders, now);
            messages(&mut receiver);
            messages(&mut peer_rx);
            assert!(hub.native_status("ABC").unwrap().fault.is_none());
        }
        last_now += 200;
    }
}
