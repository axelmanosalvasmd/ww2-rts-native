//! NET-37: claim, cancellation, stale completion and rollback through an ordinary result.
use super::*;

fn room() -> (Hub, [Connection; 2], [mpsc::Receiver<Output>; 2]) {
    let mut hub = Hub::new(32, String::new());
    hub.configure_trusted_room(Some(crate::TrustedRoomConfig::new("ABC", 3936341).unwrap()));
    let pairs = [1, 2].map(|id| {
        let (tx, rx) = mpsc::channel(128);
        let (cancel, _) = watch::channel(false);
        let connection = Connection::new(id, tx, cancel);
        hub.message(
            "ABC",
            &connection,
            &mut false,
            json!({"t":"hello", "name":format!("Seat {id}"), "token":format!("token{id}")}),
            0,
        );
        (connection, rx)
    });
    let [(a, ra), (b, rb)] = pairs;
    let mut receivers = [ra, rb];
    drain(&mut hub, &mut receivers, 0);
    (hub, [a, b], receivers)
}

fn drain(hub: &mut Hub, receivers: &mut [mpsc::Receiver<Output>; 2], now: u64) -> [Vec<Value>; 2] {
    receivers.each_mut().map(|receiver| {
        let mut messages = Vec::new();
        while let Ok(output) = receiver.try_recv() {
            match output {
                Output::Json(message) => messages.push(message),
                Output::Snapshot(frame) => {
                    messages.push(serde_json::from_str(&frame.json).unwrap());
                    hub.complete_snapshot(&frame, true, now);
                }
                Output::Watcher(_) => {
                    panic!("unexpected spectator snapshot on a seated connection")
                }
                Output::Close(code, reason) => panic!("unexpected close {code}: {reason}"),
            }
        }
        messages
    })
}

fn finish_ordinary_match(hub: &mut Hub, receivers: &mut [mpsc::Receiver<Output>; 2], mut now: u64) {
    let epoch = hub.native_status("ABC").unwrap().epoch;
    let runtime = &mut hub
        .rooms
        .get_mut("ABC")
        .unwrap()
        .native
        .as_mut()
        .unwrap()
        .runtime;
    // A captured front earns the ordinary Conquest victory. The match itself
    // computes income, victory and all 120 ending callbacks.
    runtime.state_mut().start.win_vp = 1.0;
    for point in &mut runtime.state_mut().start.points {
        point.owner = Some(0);
        point.cut = false;
    }
    let mut frames = [0; 2];
    for _ in 0..600 {
        hub.poll_native(now, now * 1_000_000);
        let status = hub.native_status("ABC").unwrap();
        assert_eq!(status.epoch, epoch);
        assert!(status.fault.is_none(), "{:?}", status.fault);
        if let Some(native) = hub.rooms.get_mut("ABC").unwrap().native.as_mut() {
            let selected = [0, 1].map(|slot| SelectedSeat {
                slot,
                full: true,
                omit: false,
            });
            let snapshots = native
                .runtime
                .collect_frame(&selected, FrameKind::Current)
                .unwrap();
            assert_eq!(snapshots.len(), 2);
            for snapshot in snapshots {
                assert_eq!(snapshot.message["t"], "s");
                assert_eq!(snapshot.message["tick"], status.tick);
                frames[snapshot.slot] += 1;
            }
        }
        let delivered = drain(hub, receivers, now);
        if status.ending_complete {
            assert_eq!(status.held, 120);
            assert!(frames.into_iter().all(|count| count >= 60));
            for (slot, messages) in delivered.into_iter().enumerate() {
                if hub.rooms["ABC"].players[slot].ai {
                    assert!(messages.is_empty(), "AI has no socket delivery");
                    continue;
                }
                let lobby = messages
                    .iter()
                    .find(|message| message["t"] == "lobby")
                    .unwrap();
                assert_eq!(lobby["state"], "lobby");
                assert_eq!(lobby["result"]["winner"], 0);
            }
            return;
        }
        now += 50;
    }
    panic!("ordinary match did not finish");
}

#[test]
fn net37_cancelled_generation_cannot_replace_new_start_or_restart_and_match_finishes() {
    let (mut hub, [host, guest], mut receivers) = room();
    let cancelled = hub.claim_trusted_start("ABC", host.id, false).unwrap();
    assert_eq!(
        hub.rooms["ABC"].lobby(host.id, &hub.maps, "")["state"],
        "play"
    );
    assert!(hub.rooms["ABC"].native.is_none());
    assert!(hub.claim_trusted_start("ABC", host.id, false).is_err());
    hub.message("ABC", &guest, &mut true, json!({"t":"end"}), 0);
    assert!(hub.rooms["ABC"].starting.is_some());
    assert_eq!(
        drain(&mut hub, &mut receivers, 0)[1],
        vec![json!({"t":"deny","cmd":"end","reason":"blocked"})]
    );
    hub.message("ABC", &host, &mut true, json!({"t":"end"}), 0);
    for messages in drain(&mut hub, &mut receivers, 0) {
        assert_eq!(
            messages[0],
            json!({"t":"pause", "paused":false, "by":"", "reason":"host", "left":0})
        );
        let lobby = messages
            .iter()
            .find(|message| message["t"] == "lobby")
            .unwrap();
        assert_eq!(lobby["result"], json!({"ended":true,"you":null}));
    }
    let fresh = hub.claim_trusted_start("ABC", host.id, false).unwrap();
    let old_state = cancelled.generate().unwrap();
    assert_eq!(
        hub.complete_trusted_start("ABC", cancelled, Ok(old_state), 0)
            .unwrap(),
        None
    );
    assert!(hub.rooms["ABC"].starting.is_some());
    let state = fresh.generate();
    assert_eq!(
        hub.complete_trusted_start("ABC", fresh, state, 0).unwrap(),
        Some(1)
    );
    drain(&mut hub, &mut receivers, 0);
    for callback in 0..4 {
        hub.poll_native(callback * 50, callback * 50_000_000);
        drain(&mut hub, &mut receivers, callback * 50);
        assert!(hub.native_status("ABC").unwrap().fault.is_none());
    }
    let cancelled = hub.claim_trusted_start("ABC", host.id, true).unwrap();
    let before = hub.native_status("ABC").unwrap();
    hub.poll_native(200, 200_000_000);
    assert_eq!(hub.native_status("ABC").unwrap(), before);
    hub.message("ABC", &host, &mut true, json!({"t":"end"}), 200);
    drain(&mut hub, &mut receivers, 200);
    let fresh = hub.claim_trusted_start("ABC", host.id, false).unwrap();
    assert_eq!(
        hub.complete_trusted_start("ABC", cancelled, Err("obsolete load failure".into()), 200)
            .unwrap(),
        None
    );
    assert!(
        drain(&mut hub, &mut receivers, 200)
            .iter()
            .all(Vec::is_empty)
    );
    let state = fresh.generate();
    assert_eq!(
        hub.complete_trusted_start("ABC", fresh, state, 200)
            .unwrap(),
        Some(2)
    );
    drain(&mut hub, &mut receivers, 200);
    finish_ordinary_match(&mut hub, &mut receivers, 250);
}

#[test]
fn net37_real_load_and_creation_failures_restore_lobby_or_previous_match_and_finish() {
    for missing_map in [true, false] {
        let (mut hub, [host, _], mut receivers) = room();
        for restart in [false, true] {
            if missing_map {
                hub.maps.as_object_mut().unwrap().remove("missing-authored-map");
                hub.rooms.get_mut("ABC").unwrap().map = "missing-authored-map".into();
                assert_eq!(
                    hub.claim_trusted_start("ABC", host.id, restart).err().unwrap(),
                    "start requires one through capacity players and both Assault sides"
                );
                assert!(hub.rooms["ABC"].starting.is_none());
                // A stale catalogue can retain capacity after its raw map disappears.
                hub.maps["missing-authored-map"] = hub.maps["default"].clone();
            } else {
                hub.rooms.get_mut("ABC").unwrap().players[1].team = 6;
            }
            let checkpoint = hub.rooms["ABC"]
                .native
                .as_ref()
                .map(|native| native.runtime.save_json().unwrap());
            let accounts = hub
                .accounts
                .iter()
                .map(|(id, account)| (*id, account.delivery.clone(), account.pressure.clone()))
                .collect::<Vec<_>>();
            let request = hub.claim_trusted_start("ABC", host.id, restart).unwrap();
            hub.message(
                "ABC",
                &host,
                &mut true,
                json!({"t":"map","name":"default"}),
                0,
            );
            assert_eq!(hub.rooms["ABC"].map == "missing-authored-map", missing_map);
            hub.poll_native(0, 0);
            let failed = request.generate();
            let error = failed.as_ref().err().unwrap();
            assert!(
                if missing_map {
                    error.contains("unknown authored map")
                } else {
                    error.starts_with("match startup:")
                },
                "{error}"
            );
            assert!(
                hub.complete_trusted_start("ABC", request, failed, 0)
                    .is_err()
            );
            assert_eq!(
                hub.rooms["ABC"]
                    .native
                    .as_ref()
                    .map(|native| native.runtime.save_json().unwrap()),
                checkpoint
            );
            assert_eq!(
                hub.accounts
                    .iter()
                    .map(|(id, account)| (*id, account.delivery.clone(), account.pressure.clone()))
                    .collect::<Vec<_>>(),
                accounts
            );
            for messages in drain(&mut hub, &mut receivers, 0) {
                assert_eq!(
                    messages[0],
                    json!({"t":"deny","cmd":"start","reason":"scenario"})
                );
                assert_eq!(messages[1]["t"], "lobby");
                assert_eq!(messages[1]["state"], if restart { "play" } else { "lobby" });
                assert_eq!(messages.len(), 2);
            }
            hub.rooms.get_mut("ABC").unwrap().map = "default".into();
            hub.rooms.get_mut("ABC").unwrap().players[1].team = 1;
            if !restart {
                let request = hub.claim_trusted_start("ABC", host.id, false).unwrap();
                let generated = request.generate();
                assert_eq!(
                    hub.complete_trusted_start("ABC", request, generated, 0)
                        .unwrap(),
                    Some(1)
                );
                drain(&mut hub, &mut receivers, 0);
            }
        }
        assert_eq!(
            hub.next_epoch, 1,
            "failed preparations cannot consume an identity or seed"
        );
        finish_ordinary_match(&mut hub, &mut receivers, 50);
    }
}

#[test]
fn net37_async_socket_dispatch_stays_available_for_end_while_worker_is_blocked() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .max_blocking_threads(1)
        .build()
        .unwrap();
    runtime.block_on(async {
        let (hub, [host, _], mut receivers) = room();
        let server = crate::Server {
            hub: Arc::new(tokio::sync::Mutex::new(hub)),
            next: Arc::new(AtomicU64::new(3)),
            clock: Instant::now(),
            test_now_ms: Some(Arc::new(AtomicU64::new(0))),
        };
        let (started, wait_started) = std::sync::mpsc::channel();
        let (release, wait_release) = std::sync::mpsc::channel();
        // A channel holds the sole worker. No sleep or scheduler timing decides
        // whether generation is still outstanding when end reaches the hub.
        let blocker = tokio::task::spawn_blocking(move || {
            started.send(()).unwrap();
            wait_release.recv().unwrap();
        });
        wait_started.recv().unwrap();
        let cancelled = server
            .dispatch("ABC", &host, &mut true, json!({"t":"start"}))
            .await
            .unwrap();
        tokio::task::yield_now().await;
        {
            let hub = server.hub.lock().await;
            assert!(hub.rooms["ABC"].starting.is_some());
            assert!(hub.rooms["ABC"].native.is_none());
        }
        assert!(
            server
                .dispatch("ABC", &host, &mut true, json!({"t":"start"}))
                .await
                .is_none()
        );
        assert!(
            server
                .dispatch("ABC", &host, &mut true, json!({"t":"end"}))
                .await
                .is_none()
        );
        {
            let mut hub = server.hub.lock().await;
            assert!(hub.rooms["ABC"].starting.is_none());
            assert!(hub.rooms["ABC"].native.is_none());
            drain(&mut hub, &mut receivers, 0);
        }
        let fresh = server
            .dispatch("ABC", &host, &mut true, json!({"t":"start"}))
            .await
            .unwrap();
        release.send(()).unwrap();
        blocker.await.unwrap();
        cancelled.await.unwrap();
        fresh.await.unwrap();
        let mut hub = server.hub.lock().await;
        assert_eq!(hub.next_epoch, 1);
        assert!(hub.rooms["ABC"].starting.is_none());
        for messages in drain(&mut hub, &mut receivers, 0) {
            assert_eq!(
                messages
                    .iter()
                    .filter(|message| message["t"] == "start")
                    .count(),
                1
            );
            assert!(!messages.iter().any(|message| message["t"] == "deny"));
        }
        finish_ordinary_match(&mut hub, &mut receivers, 0);
    });
}

#[test]
fn net37_failed_restart_settles_old_writes_and_restores_replaced_seat_before_finishing() {
    let (mut hub, [host, guest], mut receivers) = room();
    hub.start_trusted("ABC", host.id, false, 0).unwrap();
    drain(&mut hub, &mut receivers, 0);
    hub.poll_native(0, 0);
    hub.poll_native(50, 50_000_000);
    assert!(
        hub.accounts
            .values()
            .all(|account| account.pressure.inflight() > 0)
    );
    hub.rooms.get_mut("ABC").unwrap().map = "missing-authored-map".into();
    let request = hub.claim_trusted_start("ABC", host.id, true).unwrap();
    let frozen = hub.native_status("ABC").unwrap();
    // Writes from before the claim still settle, but cannot generate catch-up
    // frames while the previous runtime is suspended.
    let messages = drain(&mut hub, &mut receivers, 50);
    assert!(
        messages
            .iter()
            .all(|messages| messages.len() == 1 && messages[0]["t"] == "s")
    );
    assert!(
        hub.accounts
            .values()
            .all(|account| account.pressure.inflight() == 0)
    );
    hub.disconnected("ABC", guest.id, 60);
    hub.retire_connection(guest.id);
    let (tx, replacement_rx) = mpsc::channel(128);
    let (cancel, _) = watch::channel(false);
    let replacement = Connection::new(3, tx, cancel);
    hub.message(
        "ABC",
        &replacement,
        &mut false,
        json!({"t":"hello","name":"Seat 2","token":"token2"}),
        70,
    );
    receivers[1] = replacement_rx;
    for messages in drain(&mut hub, &mut receivers, 70) {
        assert!(messages.iter().all(|message| message["t"] == "lobby"));
    }
    hub.poll_native(100, 100_000_000);
    assert_eq!(hub.native_status("ABC").unwrap(), frozen);
    let generated = request.generate();
    assert!(
        hub.complete_trusted_start("ABC", request, generated, 100)
            .is_err()
    );
    let delivered = drain(&mut hub, &mut receivers, 100);
    assert!(!delivered[0].iter().any(|message| message["t"] == "start"));
    let starts = delivered[1]
        .iter()
        .filter(|message| message["t"] == "start")
        .collect::<Vec<_>>();
    assert_eq!(starts.len(), 1);
    assert_eq!(starts[0]["matchId"], 1);
    assert_eq!(starts[0]["you"], 1);
    assert_eq!(hub.accounts[&replacement.id].epoch, 1);
    assert!(!hub.native_status("ABC").unwrap().paused);
    assert!(hub.native_status("ABC").unwrap().fault.is_none());
    hub.rooms.get_mut("ABC").unwrap().map = "default".into();
    finish_ordinary_match(&mut hub, &mut receivers, 150);
}

#[test]
fn net37_failed_restart_with_ai_retains_pause_state_and_finishes_every_seat() {
    let (mut hub, [host, guest], mut receivers) = room();
    hub.disconnected("ABC", guest.id, 0);
    hub.retire_connection(guest.id);
    hub.message("ABC", &host, &mut true, json!({"t":"kick","slot":1}), 0);
    hub.message("ABC", &host, &mut true, json!({"t":"addAi"}), 0);
    hub.message(
        "ABC",
        &host,
        &mut true,
        json!({"t":"team","slot":1,"v":1}),
        0,
    );
    assert!(hub.rooms["ABC"].players[1].ai);
    assert!(hub.rooms["ABC"].players[1].connection.is_none());
    drain(&mut hub, &mut receivers, 0);
    hub.start_trusted("ABC", host.id, false, 0).unwrap();
    drain(&mut hub, &mut receivers, 0);
    for callback in 0..4 {
        hub.poll_native(callback * 50, callback * 50_000_000);
        drain(&mut hub, &mut receivers, callback * 50);
    }
    let before = hub.native_status("ABC").unwrap();
    assert!(!before.paused);
    let checkpoint = hub.rooms["ABC"]
        .native
        .as_ref()
        .unwrap()
        .runtime
        .save_json()
        .unwrap();
    hub.rooms.get_mut("ABC").unwrap().map = "missing-authored-map".into();
    let request = hub.claim_trusted_start("ABC", host.id, true).unwrap();
    hub.poll_native(200, 200_000_000);
    let generated = request.generate();
    assert!(generated.is_err());
    assert!(
        hub.complete_trusted_start("ABC", request, generated, 200)
            .is_err()
    );
    assert_eq!(hub.native_status("ABC").unwrap(), before);
    assert_eq!(
        hub.rooms["ABC"]
            .native
            .as_ref()
            .unwrap()
            .runtime
            .save_json()
            .unwrap(),
        checkpoint
    );
    let [host_messages, ai_messages] = drain(&mut hub, &mut receivers, 200);
    assert!(ai_messages.is_empty());
    assert_eq!(
        host_messages[0],
        json!({"t":"deny","cmd":"start","reason":"scenario"})
    );
    assert!(
        host_messages
            .iter()
            .all(|message| message["t"] != "pause" && message["t"] != "start")
    );
    hub.rooms.get_mut("ABC").unwrap().map = "default".into();
    finish_ordinary_match(&mut hub, &mut receivers, 250);
}
