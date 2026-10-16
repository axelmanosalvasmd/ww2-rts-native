use super::*;

fn peer(id: u64) -> (Connection, mpsc::Receiver<Output>) {
    let (tx, rx) = mpsc::channel(64);
    let (cancel, _) = watch::channel(false);
    (Connection { id, tx, cancel }, rx)
}

fn join(hub: &mut Hub, connection: &Connection, token: &str, now: u64) {
    hub.message(
        "ABC",
        connection,
        &mut false,
        json!({"t":"hello","token":token,"name":token}),
        now,
    );
}

fn drain(hub: &mut Hub, rx: &mut mpsc::Receiver<Output>, now: u64) -> Vec<Value> {
    let mut messages = Vec::new();
    while let Ok(output) = rx.try_recv() {
        match output {
            Output::Json(message) => messages.push(message),
            Output::Snapshot(write) => {
                messages.push(serde_json::from_str(&write.json).unwrap());
                hub.complete_snapshot(&write, true, now);
            }
            other => panic!("unexpected output {other:?}"),
        }
    }
    messages
}

fn setup() -> (
    Hub,
    Connection,
    mpsc::Receiver<Output>,
    Connection,
    mpsc::Receiver<Output>,
) {
    let mut hub = Hub::new(32, String::new());
    let (host, mut host_rx) = peer(1);
    let (guest, mut guest_rx) = peer(2);
    join(&mut hub, &host, "host", 0);
    join(&mut hub, &guest, "guest", 0);
    hub.attach_native("ABC", native_tests::native_match(), 0)
        .unwrap();
    drain(&mut hub, &mut host_rx, 0);
    drain(&mut hub, &mut guest_rx, 0);
    (hub, host, host_rx, guest, guest_rx)
}

fn snapshots(messages: &[Value]) -> impl Iterator<Item = &Value> {
    messages.iter().filter(|m| m["t"] == "s")
}

#[test]
fn net66_presence_and_ping_survive_scheduled_paused_catchup_and_reconnect_delivery() {
    let (mut hub, host, mut host_rx, guest, mut guest_rx) = setup();
    for beat in 0..4 {
        hub.poll_native(beat * 50, beat * 50_000_000);
    }
    for rx in [&mut host_rx, &mut guest_rx] {
        let messages = drain(&mut hub, rx, 150);
        assert!(!snapshots(&messages).collect::<Vec<_>>().is_empty());
        for frame in snapshots(&messages) {
            assert_eq!(frame["online"], json!([true, true]));
            assert_eq!(frame["ping"], json!([null, null]));
        }
    }
    for (client, rtt) in [(&host, -12.0), (&guest, 12_000.0)] {
        hub.message("ABC", client, &mut true, json!({"t":"ping","rtt":rtt}), 151);
    }
    hub.disconnected("ABC", guest.id, 200);
    hub.poll_native(1200, 200_000_000);
    let paused = drain(&mut hub, &mut host_rx, 1200);
    let frame = snapshots(&paused).next().unwrap();
    assert_eq!(frame["online"], json!([true, false]));
    assert_eq!(frame["ping"], json!([0.0, 9999.0]));
    assert!(hub.native_status("ABC").unwrap().paused);

    // The real completion callback produces the owed current frame.
    let epoch = hub.native_status("ABC").unwrap().epoch;
    let account = hub.accounts.get_mut(&host.id).unwrap();
    let ticket = account
        .pressure
        .begin_write(&"x".repeat(256 * 1024))
        .unwrap();
    assert!(account.pressure.skip_snapshot());
    hub.complete_snapshot(
        &SnapshotWrite {
            room: "ABC".into(),
            epoch,
            slot: 0,
            connection_id: host.id,
            ticket,
            json: String::new(),
        },
        true,
        1201,
    );
    let catchup = drain(&mut hub, &mut host_rx, 1201);
    let frame = snapshots(&catchup).next().unwrap();
    assert_eq!(frame["online"], json!([true, false]));
    assert_eq!(frame["ping"], json!([0.0, 9999.0]));

    let (returned, mut returned_rx) = peer(3);
    join(&mut hub, &returned, "guest", 1202);
    drain(&mut hub, &mut returned_rx, 1202);
    drain(&mut hub, &mut host_rx, 1202);
    for beat in 5..9 {
        hub.poll_native(1202 + beat * 50, beat * 50_000_000);
    }
    for rx in [&mut host_rx, &mut returned_rx] {
        let messages = drain(&mut hub, rx, 1700);
        assert!(snapshots(&messages).next().is_some());
        for frame in snapshots(&messages) {
            assert_eq!(frame["online"], json!([true, true]));
            assert_eq!(frame["ping"], json!([0.0, 9999.0]));
        }
    }
    assert!(hub.native_status("ABC").unwrap().fault.is_none());

    // AI metadata uses the same production projection, without enabling room AI.
    let mut lobby = Hub::new(32, String::new());
    let (client, _rx) = peer(10);
    join(&mut lobby, &client, "human", 0);
    lobby.message("ABC", &client, &mut true, json!({"t":"addAi"}), 1);
    let ai = &mut lobby.rooms.get_mut("ABC").unwrap().players[1];
    ai.rtt = Some(123.0);
    assert!(ai.online());
    assert_eq!(ai.ping(), Some(-1.0));
    assert!(
        lobby
            .attach_native("ABC", native_tests::native_match(), 2)
            .is_err()
    );
}

#[test]
fn net79_host_controls_freeze_ticks_and_orders_with_operative_admission_closed() {
    let (mut hub, host, mut host_rx, guest, mut guest_rx) = setup();
    let id = hub.rooms["ABC"]
        .native
        .as_ref()
        .unwrap()
        .runtime
        .state()
        .world
        .game
        .units
        .values()
        .find(|u| {
            u.owner == 0
                && hub.rooms["ABC"]
                    .native
                    .as_ref()
                    .unwrap()
                    .runtime
                    .state()
                    .rules
                    .kinds[u.kind]
                    .name
                    == "rifle"
        })
        .unwrap()
        .id;
    let original = hub.rooms["ABC"]
        .native
        .as_ref()
        .unwrap()
        .runtime
        .state()
        .commands
        .units[&id]
        .hold_fire;
    hub.message(
        "ABC",
        &host,
        &mut true,
        json!({"t":"stance","ids":[id],"key":"holdFire","on":!original}),
        1,
    );
    hub.message("ABC", &guest, &mut true, json!({"t":"pause"}), 2);
    assert!(!hub.native_status("ABC").unwrap().paused);
    hub.message("ABC", &host, &mut true, json!({"t":"pause"}), 3);
    for rx in [&mut host_rx, &mut guest_rx] {
        assert!(
            drain(&mut hub, rx, 3)
                .iter()
                .any(|m| *m
                    == json!({"t":"pause","paused":true,"by":"host","reason":"host","left":0}))
        );
    }
    hub.message(
        "ABC",
        &host,
        &mut true,
        json!({"t":"stance","ids":[id],"key":"holdFire","on":original}),
        4,
    );
    hub.message("ABC", &guest, &mut true, json!({"t":"resume"}), 5);
    assert!(hub.native_status("ABC").unwrap().paused);
    let random = hub.rooms["ABC"]
        .native
        .as_ref()
        .unwrap()
        .runtime
        .state()
        .start
        .random
        .snapshot();
    for (now, monotonic, expect_frame) in [
        (1002, 0, false),
        (1003, 50_000_000, true),
        (2002, 100_000_000, false),
        (2003, 150_000_000, true),
    ] {
        hub.poll_native(now, monotonic);
        for rx in [&mut host_rx, &mut guest_rx] {
            let messages = drain(&mut hub, rx, now);
            assert_eq!(snapshots(&messages).count(), usize::from(expect_frame));
            assert_eq!(
                messages.iter().filter(|m| m["t"] == "pause").count(),
                usize::from(expect_frame)
            );
        }
        let state = hub.rooms["ABC"].native.as_ref().unwrap().runtime.state();
        assert_eq!(state.world.game.tick, 0);
        assert_eq!(state.start.random.snapshot(), random);
        assert_eq!(state.commands.pending_commands(), 1);
        assert_eq!(state.commands.units[&id].hold_fire, original);
    }
    let (operative, mut operative_rx) = peer(8);
    let mut joined = false;
    hub.message(
        "ABC",
        &operative,
        &mut joined,
        json!({"t":"hello","token":"op","name":"op","role":"operative","commander":0}),
        2004,
    );
    assert!(!joined);
    assert!(matches!(operative_rx.try_recv(), Ok(Output::Json(m)) if m["reason"] == "commander"));
    assert!(matches!(
        operative_rx.try_recv(),
        Ok(Output::Close(1008, _))
    ));
    hub.message(
        "ABC",
        &operative,
        &mut joined,
        json!({"t":"fps","seq":1,"fire":true}),
        2005,
    );
    assert_eq!(hub.rooms["ABC"].players.len(), 2);
    hub.message("ABC", &host, &mut true, json!({"t":"resume"}), 2006);
    assert!(!hub.native_status("ABC").unwrap().paused);
    for rx in [&mut host_rx, &mut guest_rx] {
        assert!(
            drain(&mut hub, rx, 2006)
                .iter()
                .any(|m| m["t"] == "pause" && m["paused"] == false)
        );
    }
    for beat in 4..10 {
        hub.poll_native(2006 + beat * 50, beat * 50_000_000);
        drain(&mut hub, &mut host_rx, 2506);
        drain(&mut hub, &mut guest_rx, 2506);
    }
    let state = hub.rooms["ABC"].native.as_ref().unwrap().runtime.state();
    assert_eq!(state.world.game.tick, 6);
    assert_eq!(state.commands.pending_commands(), 0);
    assert_eq!(state.commands.units[&id].hold_fire, !original);
    assert!(hub.native_status("ABC").unwrap().fault.is_none());

    // Finish by the ordinary Conquest victory rule, then complete the result hold.
    let win_vp = state.start.win_vp;
    hub.rooms
        .get_mut("ABC")
        .unwrap()
        .native
        .as_mut()
        .unwrap()
        .runtime
        .state_mut()
        .start
        .economy
        .players[0]
        .vp = win_vp;
    for beat in 10..=130 {
        hub.poll_native(2006 + beat * 50, beat * 50_000_000);
        drain(&mut hub, &mut host_rx, 2006 + beat * 50);
        drain(&mut hub, &mut guest_rx, 2006 + beat * 50);
        assert!(hub.native_status("ABC").unwrap().fault.is_none());
    }
    assert!(hub.native_status("ABC").unwrap().ending_complete);
    assert!(hub.rooms["ABC"].native.is_none());
}

#[test]
fn net80_human_drop_expiry_reconnect_host_release_and_fresh_match_allowance() {
    let (mut hub, host, mut host_rx, guest, mut guest_rx) = setup();
    hub.disconnected("ABC", guest.id, 10);
    assert!(hub.native_status("ABC").unwrap().paused);
    hub.poll_native(30_009, 0);
    assert!(hub.native_status("ABC").unwrap().paused);
    assert_eq!(hub.native_status("ABC").unwrap().tick, 0);
    drain(&mut hub, &mut host_rx, 30_009);
    hub.poll_native(30_010, 50_000_000);
    assert!(!hub.native_status("ABC").unwrap().paused);
    assert_eq!(hub.native_status("ABC").unwrap().tick, 1);
    drain(&mut hub, &mut host_rx, 30_010);
    let (returned, mut returned_rx) = peer(3);
    join(&mut hub, &returned, "guest", 30_011);
    drain(&mut hub, &mut returned_rx, 30_011);
    hub.disconnected("ABC", returned.id, 30_012);
    assert!(!hub.native_status("ABC").unwrap().paused);

    // A new match restores the allowance; neither reconnect nor host release
    // restores a used allowance within that match.
    let prepared = hub
        .prepare_native("ABC", native_tests::native_match(), 30_013, true)
        .unwrap();
    hub.publish_native("ABC", prepared);
    drain(&mut hub, &mut host_rx, 30_013);
    assert!(hub.native_status("ABC").unwrap().paused);
    hub.message("ABC", &host, &mut true, json!({"t":"resume"}), 30_014);
    assert!(!hub.native_status("ABC").unwrap().paused);
    let (returned, mut rx) = peer(4);
    join(&mut hub, &returned, "guest", 30_015);
    drain(&mut hub, &mut rx, 30_015);
    hub.disconnected("ABC", returned.id, 30_016);
    assert!(!hub.native_status("ABC").unwrap().paused);
    let prepared = hub
        .prepare_native("ABC", native_tests::native_match(), 30_017, true)
        .unwrap();
    hub.publish_native("ABC", prepared);
    assert!(hub.native_status("ABC").unwrap().paused);
    let (returned, mut rx) = peer(5);
    join(&mut hub, &returned, "guest", 30_018);
    drain(&mut hub, &mut rx, 30_018);
    assert!(!hub.native_status("ABC").unwrap().paused);
    hub.disconnected("ABC", returned.id, 30_019);
    assert!(!hub.native_status("ABC").unwrap().paused);
    for beat in 2..7 {
        hub.poll_native(30_019 + beat * 50, beat * 50_000_000);
        drain(&mut hub, &mut host_rx, 30_400);
    }
    assert!(hub.native_status("ABC").unwrap().fault.is_none());
    drain(&mut hub, &mut guest_rx, 30_400);
}
