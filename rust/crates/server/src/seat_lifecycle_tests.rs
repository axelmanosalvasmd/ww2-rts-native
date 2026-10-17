use super::*;

fn peer(id: u64) -> (Connection, mpsc::Receiver<Output>) {
    let (tx, rx) = mpsc::channel(64);
    let (cancel, _) = watch::channel(false);
    (Connection::new(id, tx, cancel), rx)
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
        assert!(snapshots(&messages).next().is_some());
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

    // AI metadata uses the same production projection in a hosted room.
    let mut lobby = Hub::new(32, String::new());
    let (client, mut rx) = peer(10);
    join(&mut lobby, &client, "human", 0);
    lobby.message("ABC", &client, &mut true, json!({"t":"addAi"}), 1);
    let ai = &mut lobby.rooms.get_mut("ABC").unwrap().players[1];
    ai.rtt = Some(123.0);
    assert!(ai.online());
    assert_eq!(ai.ping(), Some(-1.0));
    lobby
        .attach_native("ABC", native_tests::native_match(), 2)
        .unwrap();
    drain(&mut lobby, &mut rx, 2);
    lobby.poll_native(2, 0);
    lobby.poll_native(52, 50_000_000);
    let delivered = drain(&mut lobby, &mut rx, 52);
    let frame = snapshots(&delivered).next().unwrap();
    assert_eq!(frame["online"], json!([true, true]));
    assert_eq!(frame["ping"][1].as_f64(), Some(-1.0));
    assert!(lobby.native_status("ABC").unwrap().fault.is_none());
}

#[test]
fn net80_human_drop_expiry_reconnect_host_release_and_fresh_match_allowance() {
    let (mut hub, _host, mut host_rx, guest, mut guest_rx) = setup();
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
    // Exercise the existing host API without admitting a client resume message.
    hub.rooms
        .get_mut("ABC")
        .unwrap()
        .native
        .as_mut()
        .unwrap()
        .runtime
        .resume(30_014);
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
