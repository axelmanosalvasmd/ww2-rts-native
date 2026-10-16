use super::*;

struct Peer {
    c: Connection,
    rx: mpsc::Receiver<Output>,
    joined: bool,
}
impl Peer {
    fn new(id: u64) -> Self {
        let (tx, rx) = mpsc::channel(128);
        let (cancel, _) = watch::channel(false);
        Self {
            c: Connection::new(id, tx, cancel),
            rx,
            joined: false,
        }
    }
    fn send(&mut self, hub: &mut Hub, value: Value, now: u64) {
        hub.message("ABC", &self.c, &mut self.joined, value, now);
    }
    fn hello(&mut self, hub: &mut Hub, watch: bool, now: u64) {
        self.send(hub, json!({"t":"hello","token":format!("p{}",self.c.id),"name":format!("P{}",self.c.id),"spectate":watch}), now);
    }
    fn drain(&mut self, hub: &mut Hub, now: u64) -> Vec<Value> {
        let mut values = Vec::new();
        while let Ok(output) = self.rx.try_recv() {
            let bytes = output.diagnostic_bytes();
            match output {
                Output::Json(value) => values.push(value),
                Output::Watcher(json) => values.push(serde_json::from_str(&json).unwrap()),
                Output::Snapshot(write) => {
                    values.push(serde_json::from_str(&write.json).unwrap());
                    hub.complete_snapshot(&write, true, now);
                }
                Output::Close(code, reason) => panic!("unexpected close {code}: {reason}"),
            }
            self.c.complete_bytes(bytes);
        }
        values
    }
}
fn room() -> (Hub, Peer, Peer) {
    let mut hub = Hub::new(32, String::new());
    let mut a = Peer::new(1);
    let mut b = Peer::new(2);
    a.hello(&mut hub, false, 0);
    b.hello(&mut hub, false, 0);
    a.drain(&mut hub, 0);
    b.drain(&mut hub, 0);
    (hub, a, b)
}

#[test]
fn intent_net21_spectator_admission_and_full_reasons_follow_room_state() {
    for playing in [false, true] {
        let (mut hub, mut a, mut b) = room();
        if playing {
            hub.attach_native("ABC", native_tests::native_match(), 0)
                .unwrap();
        }
        let mut watchers = Vec::new();
        for id in 3..11 {
            let mut peer = Peer::new(id);
            peer.hello(&mut hub, !playing, 0);
            assert!(peer.joined);
            let messages = peer.drain(&mut hub, 0);
            assert!(
                messages
                    .iter()
                    .any(|v| v["t"] == "lobby" && v["spectator"] == true && v["you"] == -1)
            );
            a.drain(&mut hub, 0);
            b.drain(&mut hub, 0);
            for watcher in &mut watchers {
                Peer::drain(watcher, &mut hub, 0);
            }
            watchers.push(peer);
        }
        assert_eq!(hub.rooms["ABC"].players.len(), 2);
        assert_eq!(hub.rooms["ABC"].spectators.len(), 8);
        let mut excess = Peer::new(11);
        excess.hello(&mut hub, !playing, 0);
        assert!(!excess.joined);
        let Output::Json(full) = excess.rx.try_recv().unwrap() else {
            panic!("full message")
        };
        assert_eq!(
            full,
            json!({"t":"full","reason":if playing {"started"} else {"seats"}})
        );
        assert!(matches!(excess.rx.try_recv(), Ok(Output::Close(1000, ""))));
        if !playing {
            hub.attach_native("ABC", native_tests::native_match(), 0)
                .unwrap();
            for watcher in &mut watchers {
                let messages = watcher.drain(&mut hub, 0);
                let starts = messages
                    .iter()
                    .filter(|m| m["t"] == "start")
                    .collect::<Vec<_>>();
                assert_eq!(starts.len(), 1);
                assert!(starts[0].get("fog").is_none());
                assert_eq!(starts[0]["you"], 0);
            }
            assert!(hub.native_status("ABC").unwrap().fault.is_none());
        }
    }
}

#[test]
fn intent_net26_spectator_start_precedes_lobby_and_pause_without_changing_seat_memory() {
    let (mut hub, mut a, mut b) = room();
    hub.attach_native("ABC", native_tests::native_match(), 0)
        .unwrap();
    a.drain(&mut hub, 0);
    b.drain(&mut hub, 0);
    hub.rooms
        .get_mut("ABC")
        .unwrap()
        .native
        .as_mut()
        .unwrap()
        .runtime
        .pause(0, "P1", crate::room_runtime::PauseReason::Host, 0)
        .unwrap();
    let before = format!(
        "{:?}",
        hub.rooms["ABC"]
            .native
            .as_ref()
            .unwrap()
            .runtime
            .state()
            .recipient_routing(0)
            .unwrap()
    );
    let mut watcher = Peer::new(3);
    watcher.hello(&mut hub, false, 1);
    let messages = watcher.drain(&mut hub, 1);
    assert_eq!(
        messages
            .iter()
            .map(|v| v["t"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["start", "pause", "lobby"]
    );
    let start = &messages[0];
    let expected_keys = [
        "cells", "factions", "map", "matchId", "names", "sandbox", "spawn", "spawns", "t", "teams",
        "weather", "you",
    ];
    assert_eq!(
        start
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        expected_keys
    );
    assert_eq!(start["you"], 0);
    assert_eq!(start["matchId"], 1);
    assert!(start.get("fog").is_none());
    assert_eq!(start["names"], json!(["P1", "P2"]));
    assert_eq!(start["teams"], json!([0, 1]));
    assert_eq!(start["factions"], json!([0, 1]));
    assert_eq!(
        messages[1],
        json!({"t":"pause","paused":true,"by":"P1","reason":"host","left":0})
    );
    assert_eq!(
        before,
        format!(
            "{:?}",
            hub.rooms["ABC"]
                .native
                .as_ref()
                .unwrap()
                .runtime
                .state()
                .recipient_routing(0)
                .unwrap()
        )
    );
    assert!(!hub.accounts.contains_key(&3));
    let mut replacement = Peer::new(4);
    replacement.send(
        &mut hub,
        json!({"t":"hello","token":"p3","name":"Returning"}),
        2,
    );
    assert_eq!(replacement.drain(&mut hub, 2)[0]["t"], "start");
    assert!(matches!(watcher.rx.try_recv(), Ok(Output::Json(v)) if v == json!({"t":"replaced"})));
    assert!(matches!(watcher.rx.try_recv(), Ok(Output::Close(1000, ""))));
    hub.disconnected("ABC", 3, 3);
    assert_eq!(
        hub.rooms["ABC"].spectators[0]
            .connection
            .as_ref()
            .unwrap()
            .id,
        4
    );
}

#[test]
fn intent_net46_spectator_army_commands_are_silent_and_never_queued() {
    let (mut hub, mut a, mut b) = room();
    hub.attach_native("ABC", native_tests::native_match(), 0)
        .unwrap();
    let mut watcher = Peer::new(3);
    watcher.hello(&mut hub, true, 0);
    a.drain(&mut hub, 0);
    b.drain(&mut hub, 0);
    watcher.drain(&mut hub, 0);
    let state = hub.rooms["ABC"].native.as_ref().unwrap().runtime.state();
    let before = format!("{:?}", state.commands.units);
    assert_eq!(state.commands.pending_tick(1).count(), 0);
    let actor = *state.world.game.units.keys().next().unwrap();
    for t in [
        "move", "amove", "stop", "stance", "attack", "recruit", "dig", "build", "support", "pause",
        "resume", "restart",
    ] {
        watcher.send(
            &mut hub,
            json!({"t":t,"ids":[actor],"x":12,"z":12,"kind":"rifle","v":1}),
            0,
        );
        assert!(
            watcher.rx.try_recv().is_err(),
            "spectator got a reply to {t}"
        );
    }
    assert_eq!(
        before,
        format!(
            "{:?}",
            hub.rooms["ABC"]
                .native
                .as_ref()
                .unwrap()
                .runtime
                .state()
                .commands
                .units
        )
    );
    assert_eq!(
        hub.rooms["ABC"]
            .native
            .as_ref()
            .unwrap()
            .runtime
            .state()
            .commands
            .pending_tick(1)
            .count(),
        0
    );
    watcher.send(&mut hub, json!({"t":"ping","c":123}), 0);
    assert_eq!(
        watcher.drain(&mut hub, 0),
        vec![json!({"t":"pong","c":123})]
    );
}

#[test]
fn intent_net75_spectator_over_four_mib_closes_1013_but_boundary_receives() {
    let (mut hub, mut a, mut b) = room();
    hub.attach_native("ABC", native_tests::native_match(), 0)
        .unwrap();
    let mut exact = Peer::new(3);
    exact.hello(&mut hub, true, 0);
    let mut over = Peer::new(4);
    over.hello(&mut hub, true, 0);
    a.drain(&mut hub, 0);
    b.drain(&mut hub, 0);
    exact.drain(&mut hub, 0);
    over.drain(&mut hub, 0);
    exact
        .c
        .buffered
        .as_ref()
        .unwrap()
        .store(4 * 1024 * 1024, Ordering::Relaxed);
    over.c
        .buffered
        .as_ref()
        .unwrap()
        .store(4 * 1024 * 1024 + 1, Ordering::Relaxed);
    hub.poll_native(0, 0);
    hub.poll_native(50, 50_000_000);
    assert!(matches!(
        over.rx.try_recv(),
        Ok(Output::Close(1013, "too far behind"))
    ));
    assert!(
        !hub.rooms["ABC"]
            .spectators
            .iter()
            .any(|p| p.connection.as_ref().is_some_and(|c| c.id == 4))
    );
    assert!(matches!(exact.rx.try_recv(), Ok(Output::Watcher(_))));
    assert!(hub.native_status("ABC").unwrap().fault.is_none());
}

#[test]
fn spectator_shared_current_full_map_stream_survives_ordinary_victory_and_ending_hold() {
    fn send_tick_frame(hub: &mut Hub) {
        let room = hub.rooms.get_mut("ABC").unwrap();
        let selected = Hub::select_native(room, &mut hub.accounts, false).unwrap();
        assert_eq!(selected.len(), 2);
        let mut dropped = Vec::new();
        assert!(
            Hub::send_native_frame(
                room,
                &mut hub.accounts,
                &selected,
                FrameKind::Scheduled,
                &mut dropped,
            )
            .is_some()
        );
        assert!(dropped.is_empty());
    }
    fn assert_seat_tick(peer: &mut Peer, hub: &mut Hub, now: u64, tick: i64, advanced: bool) {
        let messages = peer.drain(hub, now);
        let snapshots = messages
            .iter()
            .filter(|message| message["t"] == "s")
            .collect::<Vec<_>>();
        assert_eq!(
            snapshots.len(),
            usize::from(advanced),
            "seat {} tick {tick}",
            peer.c.id
        );
        for snapshot in snapshots {
            assert_eq!(snapshot["tick"], tick);
            if hub.native_status("ABC").unwrap().held > 0 {
                assert_eq!(snapshot["winner"], 0);
            }
        }
    }
    let (mut hub, mut a, mut b) = room();
    let mut state = native_tests::native_match();
    state.start.economy.players[0].vp = state.start.win_vp - 1.;
    for point in &mut state.start.points {
        point.owner = Some(0);
    }
    hub.attach_native("ABC", state, 0).unwrap();
    a.drain(&mut hub, 0);
    b.drain(&mut hub, 0);
    // Join during play after ordinary ticking has begun.
    let initial_tick = hub.native_status("ABC").unwrap().tick;
    hub.poll_native(0, 0);
    let first = hub.native_status("ABC").unwrap();
    assert_eq!(first.tick, initial_tick + 1);
    assert!(first.fault.is_none(), "{:?}", first.fault);
    if a.rx.is_empty() {
        send_tick_frame(&mut hub);
    }
    assert_seat_tick(&mut a, &mut hub, 0, first.tick, true);
    assert_seat_tick(&mut b, &mut hub, 0, first.tick, true);
    assert!(hub.native_status("ABC").unwrap().fault.is_none());
    let mut w1 = Peer::new(3);
    w1.hello(&mut hub, false, 1);
    let mut w2 = Peer::new(4);
    w2.hello(&mut hub, false, 1);
    a.drain(&mut hub, 1);
    b.drain(&mut hub, 1);
    w1.drain(&mut hub, 1);
    w2.drain(&mut hub, 1);
    let mut frames = 0;
    let mut saw_finish = false;
    let mut forced_full = false;
    let mut joined_after_stream = false;
    let mut previous_tick = first.tick;
    let mut ending_callbacks = 0;
    let mut ending_ticks = 0;
    for callback in 1..=500u64 {
        let now = callback * 50;
        hub.poll_native(now, now * 1_000_000);
        let status = hub.native_status("ABC").unwrap();
        assert!(status.fault.is_none(), "{:?}", status.fault);
        let advanced = status.tick != previous_tick;
        assert_eq!(status.tick, previous_tick + i64::from(advanced));
        if status.held > 0 {
            ending_callbacks += 1;
            ending_ticks += usize::from(advanced);
            assert_eq!(status.held, ending_callbacks);
            assert_eq!(advanced, status.held.is_multiple_of(2));
        } else {
            assert!(advanced, "ordinary callback must advance exactly one tick");
        }
        // Production cadence skips ordinary ticks. Force the existing shared
        // delivery path on those ticks so every new state reaches every peer.
        if advanced && !status.ending_complete && w1.rx.is_empty() {
            send_tick_frame(&mut hub);
        }
        let mut watchers = Vec::new();
        for peer in [&mut w1, &mut w2] {
            let mut stream = Vec::new();
            while let Ok(output) = peer.rx.try_recv() {
                let bytes = output.diagnostic_bytes();
                match output {
                    Output::Watcher(json) => stream.push(json),
                    Output::Json(value) => {
                        assert!(matches!(value["t"].as_str(), Some("lobby" | "pause")));
                    }
                    other => panic!("unexpected watcher output {other:?}"),
                }
                peer.c.complete_bytes(bytes);
            }
            watchers.push(stream);
        }
        assert_eq!(
            watchers[0].len(),
            usize::from(advanced),
            "watcher 1 tick {}",
            status.tick
        );
        assert_eq!(
            watchers[1].len(),
            usize::from(advanced),
            "watcher 2 tick {}",
            status.tick
        );
        for (left, right) in watchers[0].iter().zip(&watchers[1]) {
            assert!(
                Arc::ptr_eq(left, right),
                "watchers must share the one encoded snapshot"
            );
            let snapshot: Value = serde_json::from_str(left).unwrap();
            assert_eq!(snapshot["tick"], status.tick);
            assert!(snapshot.get("fog").is_none());
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
                assert!(snapshot.get(key).is_none(), "private {key}");
            }
            assert!(
                snapshot.get("tech").is_some(),
                "prototype keeps seat 0 tech"
            );
            if frames == 0 {
                let native = hub.rooms["ABC"].native.as_ref().unwrap();
                assert_eq!(
                    snapshot["units"].as_array().unwrap().len(),
                    native.runtime.state().world.game.units.len()
                );
                assert_eq!(
                    snapshot["held"][0],
                    snapshot["units"].as_array().unwrap().len()
                );
                assert!(
                    snapshot["units"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|row| row[2] == 1)
                );
            }
            if frames == 1 {
                assert_eq!(snapshot["all"], true);
                forced_full = true;
            }
            if status.held > 0 {
                assert_eq!(snapshot["winner"], 0);
            }
            if status.held == 0 && snapshot["winner"].is_number() {
                assert_eq!(snapshot["winner"], 0);
                saw_finish = true;
            }
            frames += 1;
        }
        assert_seat_tick(&mut a, &mut hub, now, status.tick, advanced);
        assert_seat_tick(&mut b, &mut hub, now, status.tick, advanced);
        assert!(hub.native_status("ABC").unwrap().fault.is_none());
        previous_tick = status.tick;
        if frames == 1 && !joined_after_stream {
            let mut replacement = Peer::new(5);
            replacement.send(
                &mut hub,
                json!({"t":"hello","token":"p3","name":"Returning"}),
                now,
            );
            let start = replacement.drain(&mut hub, now);
            assert_eq!(start[0]["t"], "start");
            assert!(start[0].get("fog").is_none());
            assert!(
                matches!(w1.rx.try_recv(), Ok(Output::Json(value)) if value["t"] == "replaced")
            );
            assert!(matches!(w1.rx.try_recv(), Ok(Output::Close(1000, ""))));
            w1 = replacement;
            joined_after_stream = true;
        }
        if status.ending_complete {
            break;
        }
    }
    assert!(saw_finish && forced_full && frames >= 60);
    assert_eq!(ending_callbacks, 120);
    assert_eq!(ending_ticks, 60);
    assert!(hub.native_status("ABC").unwrap().ending_complete);
    assert_eq!(hub.rooms["ABC"].spectators.len(), 2);
}
