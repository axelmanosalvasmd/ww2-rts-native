use super::*;

#[test]
fn operative_start_orders_publication_resets_edges_and_respects_missing_commander() {
    let (mut hub, _, mut commanders) = room();
    let (first, mut receiver) = client(3);
    join_operative(&mut hub, &first, "friend", 0);
    assert!(messages(&mut receiver).iter().all(|m| m["t"] == "lobby"));
    hub.poll_native(50, 50_000_000);
    assert!(messages(&mut receiver).is_empty());
    hub.attach_native("ABC", native_tests::native_match(), 50)
        .unwrap();
    let starts = messages(&mut receiver);
    let lobby_at = starts.iter().position(|m| m["t"] == "lobby").unwrap();
    let start_at = starts.iter().position(|m| m["t"] == "start").unwrap();
    assert!(lobby_at < start_at);
    assert!(starts.iter().all(|m| m["t"] != "s"));
    let commander_starts = drain_commanders(&mut hub, &mut commanders, 50);
    let commander_start = commander_starts[0]
        .iter()
        .find(|m| m["t"] == "start")
        .unwrap();
    let soldier = state(&hub).companions.get("friend").unwrap().unit_id;
    assert_start(&starts[start_at], commander_start, 0, soldier);

    let mut input = fps(9);
    for edge in ["jump", "reload", "nade", "squad"] {
        input[edge] = json!(1);
    }
    command(&mut hub, &first, input.clone(), 70);
    input["seq"] = json!(10);
    for edge in ["jump", "reload", "nade", "squad"] {
        input[edge] = json!(2);
    }
    command(&mut hub, &first, input, 90);
    assert_eq!(
        state(&hub).companions.get("friend").unwrap().want,
        [true; 4]
    );
    hub.rooms
        .get_mut("ABC")
        .unwrap()
        .native
        .as_mut()
        .unwrap()
        .runtime
        .pause(0, "a", crate::room_runtime::PauseReason::Host, 91)
        .unwrap();
    let (replacement, mut returned) = client(4);
    assert!(hello(
        &mut hub,
        &replacement,
        json!({"t":"hello","token":"friend","name":"Returned"}),
        92
    ));
    let output = messages(&mut returned);
    assert_eq!(
        output
            .iter()
            .map(|m| m["t"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["start", "pause", "lobby"]
    );
    assert_start(&output[0], commander_start, 0, soldier);
    assert_eq!(output[0]["operative"]["seq"], -1);
    let record = state(&hub).companions.get("friend").unwrap();
    assert!(record.online && record.input.is_none());
    assert_eq!(record.seen, [None; 4]);
    assert_eq!(record.want, [false; 4]);
    assert_eq!(hub.rooms["ABC"].operatives[0].input_at, 0);
    assert!(hub.native_status("ABC").unwrap().fault.is_none());

    // These guards suppress a start without admitting a running World match.
    for missing in [false, true] {
        let room = hub.rooms.get_mut("ABC").unwrap();
        if missing {
            room.operatives[0].commander = None;
        } else {
            room.mode = "world".into();
        }
        room.start_operative(0, 93).unwrap();
        assert!(room.operatives[0].stream.is_none());
        assert!(messages(&mut returned).is_empty());
        room.mode = "conquest".into();
    }
}

#[test]
fn operative_disconnect_idles_retains_membership_and_waits_offline_past_respawn() {
    let (mut hub, _, mut commanders) = room();
    let (op, mut receiver) = client(3);
    join_operative(&mut hub, &op, "friend", 0);
    hub.attach_native("ABC", native_tests::native_match(), 0)
        .unwrap();
    messages(&mut receiver);
    drain_commanders(&mut hub, &mut commanders, 0);
    let mut input = fps(7);
    input["fire"] = json!(true);
    command(&mut hub, &op, input, 20);
    hub.poll_native(0, 0);
    hub.poll_native(50, 50_000_000);
    messages(&mut receiver);
    drain_commanders(&mut hub, &mut commanders, 50);
    let soldier = state(&hub).companions.get("friend").unwrap().unit_id;
    let shots = state(&hub).companions.get("friend").unwrap().shots;
    hub.disconnected("ABC", op.id, 51);
    let dropped = drain_commanders(&mut hub, &mut commanders, 51);
    for output in dropped {
        assert_eq!(output.len(), 1);
        assert_eq!(output[0]["t"], "lobby");
        assert_eq!(
            output[0]["operatives"],
            json!([{"name":"friend","commander":0,"online":false}])
        );
    }
    command(&mut hub, &op, fps(99), 80);
    assert_eq!(last_seq(&hub, "friend"), 7);
    let mut resting = None;
    let mut alive_frames = 0;
    for callback in 2..=20_u64 {
        let now = callback * 50;
        hub.poll_native(now, now * 1_000_000);
        assert!(!hub.native_status("ABC").unwrap().paused);
        assert!(hub.native_status("ABC").unwrap().fault.is_none());
        let record = state(&hub).companions.get("friend").unwrap();
        assert!(!record.online && record.input.is_none());
        assert_eq!(record.unit_id, soldier);
        assert_eq!(record.shots, shots);
        let unit = &state(&hub).world.game.units[&soldier];
        if callback == 10 {
            resting = Some((unit.x, unit.z));
        }
        if callback > 10 {
            assert_eq!(Some((unit.x, unit.z)), resting);
        }
        alive_frames += drain_commanders(&mut hub, &mut commanders, now)[0]
            .iter()
            .filter(|m| m["t"] == "s")
            .count();
        assert!(messages(&mut receiver).is_empty());
    }
    assert!(alive_frames > 0);
    hub.rooms
        .get_mut("ABC")
        .unwrap()
        .native
        .as_mut()
        .unwrap()
        .runtime
        .state_mut()
        .world
        .game
        .units
        .get_mut(&soldier)
        .unwrap()
        .hp = 0.;
    let mut dead_frames = 0;
    for callback in 21..=190_u64 {
        let now = callback * 50;
        hub.poll_native(now, now * 1_000_000);
        assert!(hub.native_status("ABC").unwrap().fault.is_none());
        assert!(!hub.native_status("ABC").unwrap().paused);
        assert_eq!(state(&hub).companions.get("friend").unwrap().unit_id, 0);
        assert!(state(&hub).start.economy.players[0].mp >= 10.);
        dead_frames += drain_commanders(&mut hub, &mut commanders, now)[0]
            .iter()
            .filter(|m| m["t"] == "s")
            .count();
    }
    assert!(dead_frames > 0);
    assert!(state(&hub).world.game.tick > state(&hub).companions.get("friend").unwrap().respawn_at);
    assert_eq!(hub.rooms["ABC"].operatives.len(), 1);
    let (replacement, mut returned) = client(4);
    assert!(hello(
        &mut hub,
        &replacement,
        json!({"t":"hello","token":"friend","name":"Returned"}),
        9501
    ));
    assert_eq!(
        messages(&mut returned)
            .iter()
            .find(|m| m["t"] == "start")
            .unwrap()["operative"]["id"],
        0
    );
    command(&mut hub, &replacement, fps(0), 9521);
    hub.poll_native(9550, 9_550_000_000);
    let record = state(&hub).companions.get("friend").unwrap();
    assert!(record.online);
    assert_eq!(record.last_seq, 0);
    assert_ne!(record.unit_id, 0);
    assert_ne!(record.unit_id, soldier);
    assert!(hub.native_status("ABC").unwrap().fault.is_none());
}

fn direct_frames(hub: &mut Hub, receiver: &mut mpsc::Receiver<Output>) -> Value {
    hub.rooms
        .get_mut("ABC")
        .unwrap()
        .send_operatives(&[], &[true, true], &[None, None])
        .unwrap();
    let frames = messages(receiver);
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0]["t"], "s");
    frames.into_iter().next().unwrap()
}

#[test]
fn operative_resync_and_rename_cover_lobby_play_pause_and_ending() {
    let (mut hub, _, mut commanders) = room();
    let (op, mut receiver) = client(3);
    let (peer, mut peer_receiver) = client(4);
    join_operative(&mut hub, &op, "friend", 0);
    join_operative(&mut hub, &peer, "peer", 0);
    messages(&mut receiver);
    messages(&mut peer_receiver);
    let initial = hub.rooms["ABC"].operatives[0].net.clone();
    command(&mut hub, &op, json!({"t":"resync"}), 0);
    assert_eq!(hub.rooms["ABC"].operatives[0].net, initial);
    hub.attach_native("ABC", native_tests::native_match(), 0)
        .unwrap();
    messages(&mut receiver);
    messages(&mut peer_receiver);
    drain_commanders(&mut hub, &mut commanders, 0);
    assert_eq!(direct_frames(&mut hub, &mut receiver)["all"], true);
    assert_eq!(messages(&mut peer_receiver)[0]["all"], true);

    for phase in ["play", "pause", "ending", "lobby"] {
        let runtime = &mut hub
            .rooms
            .get_mut("ABC")
            .unwrap()
            .native
            .as_mut()
            .unwrap()
            .runtime;
        if phase == "pause" {
            runtime
                .pause(0, "a", crate::room_runtime::PauseReason::Host, 10)
                .unwrap();
        } else if phase == "ending" {
            runtime.resume(11);
            let vp = runtime.state().start.win_vp;
            runtime.state_mut().start.economy.players[0].vp = vp;
            runtime
                .state_mut()
                .apply_conquest_victory()
                .unwrap()
                .unwrap();
        } else if phase == "lobby" {
            hub.poll_native(0, 0);
            for callback in 1..=120_u64 {
                let now = callback * 50;
                hub.poll_native(now, now * 1_000_000);
                assert!(hub.native_status("ABC").unwrap().fault.is_none());
                drain_commanders(&mut hub, &mut commanders, now);
                messages(&mut receiver);
                messages(&mut peer_receiver);
            }
            assert!(hub.rooms["ABC"].native.is_none());
        }
        let peer_before = hub.rooms["ABC"].operatives[1].net.clone();
        let commander_before = hub
            .accounts
            .iter()
            .map(|(id, account)| (*id, account.delivery.clone()))
            .collect::<Vec<_>>();
        command(&mut hub, &op, json!({"t":"resync"}), 12);
        assert_eq!(hub.rooms["ABC"].operatives[1].net, peer_before);
        for (id, delivery) in &commander_before {
            assert_eq!(&hub.accounts[id].delivery, delivery);
        }
        if phase != "lobby" {
            assert_eq!(direct_frames(&mut hub, &mut receiver)["all"], true);
            assert!(messages(&mut peer_receiver)[0].get("all").is_none());
            assert!(direct_frames(&mut hub, &mut receiver).get("all").is_none());
            messages(&mut peer_receiver);
            assert!(hub.native_status("ABC").unwrap().fault.is_none());
        } else {
            let actual = &mut hub.rooms.get_mut("ABC").unwrap().operatives[0].net;
            let scope = crate::snapshot_lists::ListScope::Personal;
            let frame = json!({"units":[]});
            assert_eq!(actual.trim(scope, frame.clone()).unwrap()["all"], true);
        }
        for malformed in [Value::Null, json!(42), json!(false), json!({})] {
            command(&mut hub, &op, json!({"t":"name","name":malformed}), 13);
            assert!(messages(&mut receiver).is_empty());
        }
        command(&mut hub, &op, json!({"t":"name","name":" <>&\"' "}), 14);
        assert_eq!(lobby(&hub, op.id)["operatives"][0]["name"], "Soldier");
        assert_eq!(messages(&mut receiver).last().unwrap()["t"], "lobby");
        assert_eq!(messages(&mut peer_receiver).last().unwrap()["t"], "lobby");
        assert!(
            drain_commanders(&mut hub, &mut commanders, 14)
                .iter()
                .all(|output| output.last().is_some_and(|m| m["t"] == "lobby"))
        );
    }
}
