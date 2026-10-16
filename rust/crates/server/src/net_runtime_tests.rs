//! Intent tests for live room controls and existing JSON v0 ping messages.
use super::*;

fn client(id: u64) -> (Connection, mpsc::Receiver<Output>) {
    let (tx, rx) = mpsc::channel(256);
    let (cancel, _) = watch::channel(false);
    (Connection::new(id, tx, cancel), rx)
}
fn join(hub: &mut Hub, c: &Connection, token: &str, now: u64) {
    hub.message(
        "ABC",
        c,
        &mut false,
        json!({"t":"hello","token":token,"name":token}),
        now,
    );
}
fn send(hub: &mut Hub, c: &Connection, msg: Value, now: u64) {
    hub.message("ABC", c, &mut true, msg, now);
}
fn drain(rx: &mut mpsc::Receiver<Output>) -> Vec<Value> {
    let mut messages = Vec::new();
    while let Ok(output) = rx.try_recv() {
        match output {
            Output::Json(value) => messages.push(value),
            Output::Snapshot(write) => messages.push(serde_json::from_str(&write.json).unwrap()),
            Output::Close(..) => panic!("unexpected close"),
        }
    }
    messages
}
fn room() -> (
    Hub,
    Connection,
    mpsc::Receiver<Output>,
    Connection,
    mpsc::Receiver<Output>,
) {
    let mut hub = Hub::new(32, String::new());
    let (a, mut ra) = client(1);
    let (b, mut rb) = client(2);
    join(&mut hub, &a, "a", 0);
    join(&mut hub, &b, "b", 0);
    hub.attach_native("ABC", native_tests::native_match(), 0)
        .unwrap();
    drain(&mut ra);
    drain(&mut rb);
    (hub, a, ra, b, rb)
}

#[test]
fn intent_net85_latency_echo_clamp_and_measured_play_telemetry_are_personal() {
    let mut hub = Hub::new(32, String::new());
    let (a, mut ra) = client(1);
    let (b, mut rb) = client(2);
    join(&mut hub, &a, "a", 0);
    join(&mut hub, &b, "b", 0);
    drain(&mut ra);
    drain(&mut rb);
    send(
        &mut hub,
        &a,
        json!({"t":"ping","c":{"echo":17},"rtt":-4}),
        0,
    );
    assert_eq!(drain(&mut ra), vec![json!({"t":"pong","c":{"echo":17}})]);
    assert!(drain(&mut rb).is_empty());
    assert_eq!(hub.rooms["ABC"].players[0].rtt, Some(0.0));
    hub.attach_native("ABC", native_tests::native_match(), 0)
        .unwrap();
    drain(&mut ra);
    drain(&mut rb);
    // Twenty snapshot samples have p50 12.14, p95 41.26 and maximum 99.94 ms.
    // The pong must report the rounded p95 and the production cadence change.
    let runtime = &mut hub
        .rooms
        .get_mut("ABC")
        .unwrap()
        .native
        .as_mut()
        .unwrap()
        .runtime;
    for duration in std::iter::repeat_n(12.14, 18).chain([41.26]) {
        runtime
            .record_timing(
                TickSample {
                    durations: [duration, 0., 0., 0., 0., 0.],
                    sent: true,
                },
                100,
            )
            .unwrap();
    }
    runtime
        .record_timing(
            TickSample {
                durations: [99.94, 0., 0., 0., 0., 0.],
                sent: true,
            },
            1000,
        )
        .unwrap();
    send(&mut hub, &a, json!({"t":"ping","c":23,"rtt":10000.4}), 1001);
    assert_eq!(
        drain(&mut ra),
        vec![json!({"t":"pong","c":23,"srv":[41.3,3]})]
    );
    assert_eq!(hub.rooms["ABC"].players[0].rtt, Some(9999.0));
    assert!(drain(&mut rb).is_empty());
    send(&mut hub, &a, json!({"t":"ping","rtt":12.5}), 1002);
    assert_eq!(hub.rooms["ABC"].players[0].rtt, Some(13.0));
    assert_eq!(drain(&mut ra), vec![json!({"t":"pong","srv":[41.3,3]})]);
    send(&mut hub, &a, json!({"t":"ping","rtt":"bad"}), 1003);
    assert_eq!(hub.rooms["ABC"].players[0].rtt, Some(13.0));
    assert_eq!(drain(&mut ra), vec![json!({"t":"pong","srv":[41.3,3]})]);
}

#[test]
fn intent_net86_map_ping_validates_before_budget_and_reaches_only_ready_human_teammates() {
    let mut hub = Hub::new(32, String::new());
    let (a, mut ra) = client(1);
    let (ally, mut r_ally) = client(2);
    let (enemy, mut r_enemy) = client(3);
    join(&mut hub, &a, "a", 0);
    join(&mut hub, &ally, "ally", 0);
    join(&mut hub, &enemy, "enemy", 0);
    send(&mut hub, &a, json!({"t":"team","slot":1,"v":0}), 0);
    drain(&mut ra);
    drain(&mut r_ally);
    drain(&mut r_enemy);
    send(&mut hub, &a, json!({"t":"ping","x":1,"z":2}), 0);
    assert!(
        drain(&mut ra).is_empty()
            && drain(&mut r_ally).is_empty()
            && drain(&mut r_enemy).is_empty()
    );
    assert!(hub.rooms["ABC"].players[0].map_pings.is_empty());
    let mut options = native_tests::native_match().options;
    options.players[1].team = 0;
    options.players.push(PlayerConfig {
        name: "enemy".into(),
        team: 2,
        faction: Faction::try_from(2).unwrap(),
    });
    let state = crate::authoritative_maps::create_match("default", options).unwrap();
    hub.attach_native("ABC", state, 0).unwrap();
    drain(&mut ra);
    drain(&mut r_ally);
    drain(&mut r_enemy);
    let (unseated, mut r_unseated) = client(5);
    send(&mut hub, &unseated, json!({"t":"ping","x":1,"z":2}), 0);
    assert!(drain(&mut r_unseated).is_empty());
    // Native attachment admits only human commanders. Move one admitted person
    // temporarily to exercise the spectator guard without widening admission.
    let spectator = hub.rooms.get_mut("ABC").unwrap().players.pop().unwrap();
    hub.rooms.get_mut("ABC").unwrap().spectators.push(spectator);
    send(&mut hub, &enemy, json!({"t":"ping","x":1,"z":2}), 0);
    assert!(
        drain(&mut ra).is_empty()
            && drain(&mut r_ally).is_empty()
            && drain(&mut r_enemy).is_empty()
    );
    let spectator = hub.rooms.get_mut("ABC").unwrap().spectators.pop().unwrap();
    assert!(spectator.map_pings.is_empty());
    hub.rooms.get_mut("ABC").unwrap().players.push(spectator);
    let game = &hub.rooms["ABC"]
        .native
        .as_ref()
        .unwrap()
        .runtime
        .state()
        .world
        .game;
    let (width, height) = (
        game.w as f64 * ww2_core::game::CELL,
        game.h as f64 * ww2_core::game::CELL,
    );
    for msg in [
        json!({"t":"ping","x":-0.1,"z":0}),
        json!({"t":"ping","x":width+0.1,"z":0}),
        json!({"t":"ping","x":0,"z":height+0.1}),
        json!({"t":"ping","x":"0","z":0}),
        json!({"t":"ping","x":0}),
        json!({"t":"ping","z":0}),
    ] {
        send(&mut hub, &a, msg, 0);
    }
    assert!(
        drain(&mut ra).is_empty()
            && drain(&mut r_ally).is_empty()
            && drain(&mut r_enemy).is_empty()
    );
    for (now, x, z) in [(0, 1.26, 2.34), (1, 0., 0.), (2, width, height)] {
        send(&mut hub, &a, json!({"t":"ping","x":x,"z":z}), now);
        let expected = json!({"t":"ping","from":0,"x":ww2_core::math::round(x*10.)/10.,"z":ww2_core::math::round(z*10.)/10.});
        assert_eq!(drain(&mut ra), vec![expected.clone()]);
        assert_eq!(drain(&mut r_ally), vec![expected]);
        assert!(drain(&mut r_enemy).is_empty());
    }
    for now in [3, 4999] {
        send(&mut hub, &a, json!({"t":"ping","x":1,"z":2}), now);
    }
    assert!(drain(&mut ra).is_empty() && drain(&mut r_ally).is_empty());
    // A replaced socket cannot bypass the person's budget, and a canceled ally receives nothing.
    let (replacement, mut rr) = client(4);
    join(&mut hub, &replacement, "a", 4999);
    // Consume the old socket's replaced envelope and close separately.
    assert!(matches!(ra.try_recv(),Ok(Output::Json(m)) if m["t"]=="replaced"));
    assert!(matches!(ra.try_recv(), Ok(Output::Close(..))));
    drain(&mut rr);
    drain(&mut r_ally);
    drain(&mut r_enemy);
    send(&mut hub, &a, json!({"t":"ping","x":1,"z":2}), 5000);
    assert!(drain(&mut rr).is_empty() && drain(&mut r_ally).is_empty());
    send(
        &mut hub,
        &replacement,
        json!({"t":"ping","x":1,"z":2}),
        4999,
    );
    assert!(drain(&mut rr).is_empty());
    ally.cancel.send_replace(true);
    send(
        &mut hub,
        &replacement,
        json!({"t":"ping","x":1,"z":2}),
        5000,
    );
    assert_eq!(
        drain(&mut rr),
        vec![json!({"t":"ping","from":0,"x":1.0,"z":2.0})]
    );
    assert!(drain(&mut r_ally).is_empty() && drain(&mut r_enemy).is_empty());
    // Exercise the defensive AI filters after human-only native admission.
    // A ready connection ensures the AI recipient check is independently needed.
    ally.cancel.send_replace(false);
    hub.rooms.get_mut("ABC").unwrap().players[1].ai = true;
    send(&mut hub, &ally, json!({"t":"ping","x":1,"z":2}), 5001);
    assert!(
        drain(&mut rr).is_empty()
            && drain(&mut r_ally).is_empty()
            && drain(&mut r_enemy).is_empty()
    );
    assert!(hub.rooms["ABC"].players[1].map_pings.is_empty());
    send(
        &mut hub,
        &replacement,
        json!({"t":"ping","x":1,"z":2}),
        5002,
    );
    assert_eq!(
        drain(&mut rr),
        vec![json!({"t":"ping","from":0,"x":1.0,"z":2.0})]
    );
    assert!(drain(&mut r_ally).is_empty() && drain(&mut r_enemy).is_empty());
    hub.rooms.get_mut("ABC").unwrap().players[1].ai = false;
    send(
        &mut hub,
        &replacement,
        json!({"t":"ping","x":1,"z":2}),
        5003,
    );
    let expected = json!({"t":"ping","from":0,"x":1.0,"z":2.0});
    assert_eq!(drain(&mut rr), vec![expected.clone()]);
    assert_eq!(drain(&mut r_ally), vec![expected]);
    assert!(drain(&mut r_enemy).is_empty());
}

#[test]
fn intent_net51_only_current_host_ends_match_and_frees_offline_seats_without_late_frames() {
    let (mut hub, a, mut ra, b, mut rb) = room();
    let epoch = hub.native_status("ABC").unwrap().epoch;
    send(&mut hub, &b, json!({"t":"end"}), 0);
    assert_eq!(hub.native_status("ABC").unwrap().epoch, epoch);
    assert!(!drain(&mut rb).is_empty());
    let (replacement, mut rr) = client(3);
    join(&mut hub, &replacement, "a", 1);
    assert!(matches!(ra.try_recv(),Ok(Output::Json(m)) if m["t"]=="replaced"));
    assert!(matches!(ra.try_recv(), Ok(Output::Close(..))));
    drain(&mut rr);
    drain(&mut rb);
    send(&mut hub, &a, json!({"t":"end"}), 2);
    assert!(hub.rooms["ABC"].native.is_some());
    hub.disconnected("ABC", b.id, 3);
    drain(&mut rr);
    send(&mut hub, &replacement, json!({"t":"end"}), 4);
    let messages = drain(&mut rr);
    assert_eq!(messages.len(), 2);
    assert_eq!(
        messages[0],
        json!({"t":"pause","paused":false,"by":"","reason":"host","left":0})
    );
    assert_eq!(messages[1]["t"], "lobby");
    assert_eq!(messages[1]["state"], "lobby");
    assert_eq!(messages[1]["result"], json!({"ended":true,"you":null}));
    assert_eq!(hub.rooms["ABC"].players.len(), 1);
    // Socket accounting survives end even when its room no longer runs a match.
    assert!(hub.accounts.contains_key(&replacement.id));
    hub.poll_native(1000, 1_000_000_000);
    assert!(drain(&mut rr).is_empty());
    assert!(hub.rooms["ABC"].native.is_none());
    // A subsequent supported start gets a fresh identity, with no old result or stream.
    let (new_peer, mut rn) = client(4);
    join(&mut hub, &new_peer, "new", 1001);
    drain(&mut rr);
    drain(&mut rn);
    hub.attach_native("ABC", native_tests::native_match(), 1002)
        .unwrap();
    assert!(hub.native_status("ABC").unwrap().epoch > epoch);
    assert!(hub.rooms["ABC"].result.is_none());
    assert!(drain(&mut rr).iter().any(|m| m["t"] == "start"));
}

#[test]
fn host_pause_wire_control_freezes_clock_and_orders_until_current_host_resumes() {
    let (mut hub, a, mut ra, b, mut rb) = room();
    send(&mut hub, &b, json!({"t":"pause"}), 0);
    assert!(!hub.native_status("ABC").unwrap().paused);
    drain(&mut rb);
    send(&mut hub, &a, json!({"t":"pause"}), 1);
    let paused = json!({"t":"pause","paused":true,"by":"a","reason":"host","left":0});
    assert_eq!(drain(&mut ra), vec![paused.clone()]);
    assert_eq!(drain(&mut rb), vec![paused.clone()]);
    send(&mut hub, &a, json!({"t":"pause"}), 2);
    assert!(drain(&mut ra).is_empty());
    send(&mut hub, &b, json!({"t":"stop","ids":[1]}), 3);
    hub.poll_native(1001, 50_000_000);
    assert_eq!(hub.native_status("ABC").unwrap().tick, 0);
    assert_eq!(
        hub.rooms["ABC"]
            .native
            .as_ref()
            .unwrap()
            .runtime
            .state()
            .commands
            .pending_commands(),
        0
    );
    let messages = drain(&mut ra);
    assert_eq!(messages.iter().filter(|m| m["t"] == "pause").count(), 1);
    assert_eq!(messages.iter().filter(|m| m["t"] == "s").count(), 1);
    drain(&mut rb);
    send(&mut hub, &b, json!({"t":"resume"}), 1002);
    assert!(hub.native_status("ABC").unwrap().paused);
    drain(&mut rb);
    send(&mut hub, &a, json!({"t":"resume"}), 1003);
    assert_eq!(drain(&mut ra)[0]["paused"], false);
    drain(&mut rb);
    hub.poll_native(1050, 100_000_000);
    assert_eq!(hub.native_status("ABC").unwrap().tick, 1);
}

#[test]
fn manual_end_keeps_faulted_runtime_and_existing_recovery_guard() {
    let (mut hub, a, mut ra, _, _) = room();
    hub.rooms
        .get_mut("ABC")
        .unwrap()
        .native
        .as_mut()
        .unwrap()
        .runtime
        .fault_stop("fixture fault".into());
    let before = hub.native_status("ABC").unwrap();
    let accounts = hub.accounts.len();
    send(&mut hub, &a, json!({"t":"end"}), 1);
    assert_eq!(hub.native_status("ABC").unwrap(), before);
    assert_eq!(hub.accounts.len(), accounts);
    assert!(hub.rooms["ABC"].result.is_none());
    assert_eq!(
        drain(&mut ra),
        vec![json!({"t":"deny","cmd":"end","reason":"blocked"})]
    );
}
