//! Controlled Hub polling over the real upgrade, socket pump and ClientView.
//! No production clock, transport policy or recipient rule is replaced.
use super::*;
use crate::snapshot_backpressure::SEAT_INFLIGHT_LIMIT;
use std::sync::Arc;
use tokio::sync::{Mutex, watch};

struct Rig {
    host: tokio::runtime::Handle,
    hub: Arc<Mutex<Hub>>,
    address: SocketAddr,
    next_ns: u64,
}
impl Rig {
    fn change<T>(&self, change: impl FnOnce(&mut Hub) -> T) -> T {
        self.host.block_on(async {
            let mut hub = self.hub.lock().await;
            change(&mut hub)
        })
    }
    fn poll(&self, now_ms: u64, now_ns: u64) {
        self.change(|hub| {
            hub.poll_native(now_ms, now_ns);
            let status = hub.native_status("ABC").unwrap();
            assert!(status.fault.is_none(), "{status:?}");
        });
    }
    fn step(&mut self) {
        // Clock deadlines stay explicit. Meter inputs in the cadence test use
        // their separate, future host timestamps and cannot change mid-burst.
        self.poll(0, self.next_ns);
        self.next_ns += 50_000_000;
    }
    fn calls(&self) -> u64 {
        self.change(|hub| {
            hub.rooms["ABC"]
                .native
                .as_ref()
                .unwrap()
                .runtime
                .snapshot_production_calls()
        })
    }
    fn absent(&self) {
        self.host.block_on(async {
            tokio::time::timeout(std::time::Duration::from_secs(10), async {
                loop {
                    let absent = {
                        let hub = self.hub.lock().await;
                        hub.rooms["ABC"]
                            .players
                            .iter()
                            .all(|p| p.connection.is_none())
                    };
                    if absent {
                        break;
                    }
                    tokio::task::yield_now().await;
                }
            })
            .await
            .expect("actual socket loops must report both closes");
        });
    }
}

fn barrier(client: &mut Receiver) -> Vec<Value> {
    let begin = client.messages.len();
    let marker = begin as u64 + 1;
    client.send(json!({"t":"ping","c":marker}));
    client.until(|m| m["t"] == "pong" && m["c"] == marker);
    client.messages[begin..]
        .iter()
        .filter(|m| m["t"] == "s")
        .cloned()
        .collect()
}
fn ticks(frames: &[Value]) -> Vec<i64> {
    frames.iter().map(|m| m["tick"].as_i64().unwrap()).collect()
}
fn begin(clients: &mut [Receiver; 2], command: &str) -> u64 {
    clients[0].send(json!({"t":command}));
    let starts = clients.each_mut().map(|c| c.until(|m| m["t"] == "start"));
    let identity = starts[0]["matchId"].as_u64().unwrap();
    for (slot, (client, start)) in clients.iter_mut().zip(starts).enumerate() {
        assert_eq!(start["matchId"], identity);
        assert_eq!(start["you"], slot);
        assert_eq!(start["teams"], json!([0, 1]));
        assert_eq!(start["sandbox"], false);
        assert_eq!(start.as_object().unwrap().len(), 13);
        assert_eq!(client.view.unit_count(), 0);
        assert!(barrier(client).is_empty());
    }
    identity
}

async fn sockets(check: impl FnOnce(&mut Rig, [Receiver; 2]) + Send + 'static) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let mut hub = Hub::new(32, String::new());
    hub.configure_trusted_room(Some(crate::TrustedRoomConfig::new("ABC", 3936341).unwrap()));
    let server = crate::Server {
        hub: Arc::new(Mutex::new(hub)),
        next: Arc::new(std::sync::atomic::AtomicU64::new(1)),
        clock: std::time::Instant::now(),
    };
    let (stop, stopped) = watch::channel(false);
    let (finish, finished) = oneshot::channel();
    // Only maintenance is test-driven. HTTP upgrade and the complete socket
    // pump, admission, start construction and write completion are unchanged.
    let app = axum::Router::new()
        .route("/ws", axum::routing::get(crate::upgrade))
        .with_state((server.clone(), stopped));
    let serving = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = finished.await;
            })
            .await
    });
    let hub = server.hub.clone();
    let host = tokio::runtime::Handle::current();
    let checked = tokio::task::spawn_blocking(move || {
        let mut clients = [
            Receiver::connect(address, "a"),
            Receiver::connect(address, "b"),
        ];
        begin(&mut clients, "start");
        let mut rig = Rig {
            host,
            hub,
            address,
            next_ns: 0,
        };
        check(&mut rig, clients);
    })
    .await;
    server.hub.lock().await.stop();
    let _ = stop.send(true);
    let _ = finish.send(());
    serving.await.unwrap().unwrap();
    checked.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn r1_eligibility_skips_production_and_reconnects_real_receivers() {
    sockets(|rig, mut clients| {
        assert_eq!(rig.calls(), 0);
        rig.step();
        rig.step();
        assert_eq!(rig.calls(), 1, "one shared production for two ready seats");
        for client in &mut clients {
            assert_eq!(ticks(&barrier(client)), [2]);
        }
        rig.change(|hub| {
            let room = &hub.rooms["ABC"];
            let connections = room
                .players
                .iter()
                .map(|p| p.connection.clone().unwrap())
                .collect::<Vec<_>>();
            let histories = connections
                .iter()
                .map(|c| hub.accounts[&c.id].delivery.clone())
                .collect::<Vec<_>>();
            for c in &connections {
                c.cancel.send(true).unwrap();
                assert!(!c.ready());
            }
            // Keep the lock until selection/production return, so the actual
            // socket close cannot turn this into the absent-connection case.
            hub.poll_native(100, 100_000_000);
            hub.poll_native(150, 150_000_000);
            let room = &hub.rooms["ABC"];
            assert_eq!(
                room.native
                    .as_ref()
                    .unwrap()
                    .runtime
                    .state()
                    .world
                    .game
                    .tick,
                4
            );
            assert_eq!(
                room.native
                    .as_ref()
                    .unwrap()
                    .runtime
                    .snapshot_production_calls(),
                1
            );
            for (c, history) in connections.iter().zip(histories) {
                assert_eq!(hub.accounts[&c.id].delivery, history);
                assert_eq!(hub.accounts[&c.id].pressure.inflight(), 0);
            }
        });
        drop(clients);
        rig.absent();
        // A paused frame request with zero connected recipients must also
        // avoid the canonical producer. Room expiry is still below 60 s.
        rig.poll(10_000, 10_000_000_000);
        assert_eq!(rig.calls(), 1);
        rig.change(|hub| {
            assert!(
                hub.rooms["ABC"]
                    .players
                    .iter()
                    .all(|p| p.connection.is_none())
            );
        });
        let mut returned = [
            Receiver::connect(rig.address, "a"),
            Receiver::connect(rig.address, "b"),
        ];
        rig.change(|hub| assert!(!hub.native_status("ABC").unwrap().paused));
        rig.poll(10_050, 10_050_000_000);
        rig.poll(10_100, 10_100_000_000);
        assert_eq!(rig.calls(), 2);
        for client in &mut returned {
            let frames = barrier(client);
            assert_eq!(frames.len(), 1);
            assert_eq!(frames[0]["online"], json!([true, true]));
            assert_eq!(
                frames[0]["held"][0].as_u64(),
                Some(client.view.unit_count() as u64)
            );
        }
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn r1_socket_ticks_follow_explicit_clock_boundaries_and_stall_cap() {
    sockets(|rig, mut clients| {
        // Pinned server.js:791-800 and the existing tick-timing fixture supply
        // these exact 50 ms/four-tick/drop boundaries, not elapsed sleeps.
        for (now_ns, count, due, frame_ticks) in [
            (0, 1, 50_000_000, vec![]),
            (49_999_999, 1, 50_000_000, vec![]),
            (50_000_000, 2, 100_000_000, vec![2]),
            (1_000_000_000, 6, 1_050_000_000, vec![4, 6]),
            (1_000_000_001, 6, 1_050_000_000, vec![]),
            (1_049_999_999, 6, 1_050_000_000, vec![]),
            (1_050_000_000, 7, 1_100_000_000, vec![]),
            (1_100_000_000, 8, 1_150_000_000, vec![8]),
        ] {
            rig.poll(now_ns / 1_000_000, now_ns);
            rig.change(|hub| {
                assert_eq!(hub.native_status("ABC").unwrap().tick, count);
                assert_eq!(hub.clock.due_ns(), due);
            });
            for client in &mut clients {
                assert_eq!(ticks(&barrier(client)), frame_ticks);
            }
        }
        assert_eq!(rig.calls(), 4);
    })
    .await;
}

fn sample(rig: &Rig, now: u64, duration: f64, sent: bool, expected: u8) {
    rig.change(|hub| {
        let runtime = &mut hub
            .rooms
            .get_mut("ABC")
            .unwrap()
            .native
            .as_mut()
            .unwrap()
            .runtime;
        let interval = runtime
            .record_timing(
                TickSample {
                    durations: [duration; 6],
                    sent,
                },
                now,
            )
            .unwrap();
        assert_eq!(
            interval, expected,
            "now={now}, duration={duration}, sent={sent}"
        );
    });
}
fn fill(rig: &Rig, now: u64, duration: f64, expected: u8) {
    for _ in 0..50 {
        sample(rig, now, duration, true, expected);
    }
}
fn beat(rig: &mut Rig, clients: &mut [Receiver; 2], interval: i64) {
    let before = rig.change(|hub| hub.native_status("ABC").unwrap().tick);
    for _ in 0..4 {
        rig.step();
    }
    let expected = (before + 1..=before + 4)
        .filter(|tick| tick % interval == 0)
        .collect::<Vec<_>>();
    for client in clients {
        assert_eq!(ticks(&barrier(client)), expected);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn r1_meter_thresholds_drive_real_socket_cadence_and_ending_hold() {
    sockets(|rig, mut clients| {
        // Public RoomRuntime timing inputs model durations. Production still
        // measures its own durations; no test field overrides a live sample.
        // Future meter times keep measured samples at poll time 0 from checking
        // an interval mid-burst. Flush 50 entries before each new p95 condition.
        for _ in 0..9 {
            sample(rig, 1_000_000, 40., true, 2);
        }
        sample(rig, 1_000_000, 90., false, 2); // unsent does not reach ten samples
        sample(rig, 1_000_000, 40., true, 2); // exactly the budget
        sample(rig, 1_000_999, 60., true, 2); // check is not due
        sample(rig, 1_001_000, 60., true, 3);
        beat(rig, &mut clients, 3);
        fill(rig, 1_001_001, 60., 3);
        sample(rig, 1_002_000, 60., true, 4);
        sample(rig, 1_003_000, 60., true, 4); // cap
        beat(rig, &mut clients, 4);
        fill(rig, 1_003_001, 24., 4);
        sample(rig, 1_004_000, 24., true, 4);
        sample(rig, 1_014_000, 24., true, 4); // equality cannot recover
        fill(rig, 1_014_001, 23., 4);
        sample(rig, 1_015_000, 23., true, 4); // start low window
        fill(rig, 1_015_001, 24., 4);
        sample(rig, 1_016_000, 24., true, 4); // interrupt it
        fill(rig, 1_016_001, 23., 4);
        sample(rig, 1_017_000, 23., true, 4);
        sample(rig, 1_026_000, 23., true, 4); // only nine seconds
        sample(rig, 1_027_000, 23., true, 3);
        beat(rig, &mut clients, 3);
        fill(rig, 1_027_001, 23., 3);
        sample(rig, 1_037_000, 23., true, 2);
        fill(rig, 1_037_001, 23., 2);
        sample(rig, 1_047_000, 23., true, 2); // floor
        beat(rig, &mut clients, 2);

        // Canonical terminal setup, as in the existing lifecycle witnesses.
        // The held-match branch emits every second callback independent of
        // the meter. This is not a full-match or AI-idling acceptance claim.
        fill(rig, 1_047_001, 60., 2);
        sample(rig, 1_048_000, 60., true, 3);
        sample(rig, 1_049_000, 60., true, 4);
        let before = rig.change(|hub| {
            let runtime = &mut hub
                .rooms
                .get_mut("ABC")
                .unwrap()
                .native
                .as_mut()
                .unwrap()
                .runtime;
            let state = runtime.state_mut();
            state.start.economy.players[0].vp = state.start.win_vp;
            state.apply_conquest_victory().unwrap().unwrap();
            let tick = runtime.state().world.game.tick;
            assert_ne!((tick + 1) % 4, 0);
            assert!(runtime.should_snapshot(tick + 1));
            tick
        });
        rig.step();
        for client in &mut clients {
            assert!(barrier(client).is_empty());
        }
        rig.step();
        for client in &mut clients {
            let frames = barrier(client);
            assert_eq!(ticks(&frames), [before + 1]);
            assert_eq!(frames[0]["winner"], 0);
        }
    })
    .await;
}

fn reservation(rig: &Rig, slot: usize, text: &str) -> SnapshotWrite {
    rig.change(|hub| {
        let room = &hub.rooms["ABC"];
        let id = room.players[slot].connection.as_ref().unwrap().id;
        let epoch = room.native.as_ref().unwrap().epoch;
        let ticket = hub
            .accounts
            .get_mut(&id)
            .unwrap()
            .pressure
            .begin_write(text)
            .unwrap();
        SnapshotWrite {
            room: "ABC".into(),
            epoch,
            slot,
            connection_id: id,
            ticket,
            json: String::new(),
        }
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn r1_backpressure_preserves_skipped_history_and_drains_one_catchup() {
    sockets(|rig, mut clients| {
        rig.step();
        rig.step();
        for client in &mut clients {
            assert_eq!(ticks(&barrier(client)), [2]);
        }
        // Like Source test-snapshot-backpressure.mjs, account for a controlled
        // pre-existing write. These reservations are never sent as fake frames.
        // All snapshots below still use the actual socket pump and decoder.
        let base = reservation(rig, 0, &"x".repeat(SEAT_INFLIGHT_LIMIT as usize - 1));
        rig.step();
        rig.step();
        for client in &mut clients {
            assert_eq!(
                ticks(&barrier(client)),
                [4],
                "one below the gate is eligible"
            );
        }
        let edge = reservation(rig, 0, "x");
        let (delivery, full) = rig.change(|hub| {
            let account = &hub.accounts[&base.connection_id];
            assert_eq!(account.pressure.inflight(), SEAT_INFLIGHT_LIMIT);
            (account.delivery.clone(), account.full_terrain)
        });
        let calls = rig.calls();
        for _ in 0..4 {
            rig.step();
        }
        assert!(barrier(&mut clients[0]).is_empty());
        assert_eq!(ticks(&barrier(&mut clients[1])), [6, 8]);
        assert_eq!(rig.calls(), calls + 2);
        rig.change(|hub| {
            let account = &hub.accounts[&base.connection_id];
            assert!(account.pressure.owed());
            assert_eq!(account.delivery, delivery);
            assert_eq!(account.full_terrain, full);
        });
        let extra = reservation(rig, 0, "x");
        rig.change(|hub| {
            hub.complete_snapshot(&extra, true, 0); // out of order, still exactly at cap
            assert!(hub.accounts[&base.connection_id].pressure.owed());
            assert_eq!(hub.accounts[&base.connection_id].delivery, delivery);
        });
        assert!(barrier(&mut clients[0]).is_empty());
        assert_eq!(rig.calls(), calls + 2);
        rig.change(|hub| hub.complete_snapshot(&edge, true, 0));
        let catchup = barrier(&mut clients[0]);
        assert_eq!(ticks(&catchup), [8]);
        assert!(catchup[0]["shots"].as_array().unwrap().is_empty());
        assert!(catchup[0]["cells"].as_array().unwrap().is_empty());
        assert!(catchup[0].get("all").is_none());
        assert!(barrier(&mut clients[1]).is_empty());
        assert_eq!(rig.calls(), calls + 3);
        rig.change(|hub| {
            assert!(!hub.accounts[&base.connection_id].pressure.owed());
            hub.complete_snapshot(&base, true, 0);
            assert_eq!(hub.accounts[&base.connection_id].pressure.inflight(), 0);
        });
        assert!(barrier(&mut clients[0]).is_empty());
        assert_eq!(rig.calls(), calls + 3, "one catch-up only");

        // An old-epoch completion on a retained socket only drains its own
        // accounting, even if its previous epoch had an owed frame.
        let old = reservation(rig, 0, &"😀".repeat(SEAT_INFLIGHT_LIMIT as usize / 2));
        for _ in 0..2 {
            rig.step();
        }
        assert!(barrier(&mut clients[0]).is_empty());
        assert_eq!(barrier(&mut clients[1]).len(), 1);
        rig.change(|hub| {
            assert!(hub.accounts[&old.connection_id].pressure.owed());
            assert_eq!(
                hub.accounts[&old.connection_id].pressure.inflight(),
                SEAT_INFLIGHT_LIMIT
            );
        });
        let identity = begin(&mut clients, "restart");
        assert!(identity > old.epoch);
        let new_delivery = rig.change(|hub| hub.accounts[&old.connection_id].delivery.clone());
        rig.change(|hub| {
            hub.complete_snapshot(&old, true, 0);
            let account = &hub.accounts[&old.connection_id];
            assert_eq!(account.pressure.inflight(), 0);
            assert_eq!(account.delivery, new_delivery);
        });
        for client in &mut clients {
            assert!(barrier(client).is_empty());
        }
        assert_eq!(
            rig.calls(),
            0,
            "fresh runtime did not produce a catch-up for an old epoch"
        );

        let stale = reservation(rig, 0, &"x".repeat(SEAT_INFLIGHT_LIMIT as usize));
        rig.step();
        rig.step();
        assert!(barrier(&mut clients[0]).is_empty());
        assert_eq!(ticks(&barrier(&mut clients[1])), [2]);
        let replacement = Receiver::connect(rig.address, "a");
        let mut retired = std::mem::replace(&mut clients[0], replacement);
        retired.until(|m| m["t"] == "replaced");
        drop(retired); // no read or write after the retirement notification
        let calls = rig.calls();
        rig.change(|hub| {
            let current_id = hub.rooms["ABC"].players[0].connection.as_ref().unwrap().id;
            assert_ne!(current_id, stale.connection_id);
            let history = hub.accounts[&current_id].delivery.clone();
            hub.complete_snapshot(&stale, true, 0);
            assert_eq!(hub.accounts[&current_id].delivery, history);
            assert_eq!(hub.accounts[&current_id].pressure.inflight(), 0);
            // The real socket loop may already have retired the old account.
            assert!(
                hub.accounts
                    .get(&stale.connection_id)
                    .is_none_or(|a| a.pressure.inflight() == 0)
            );
            assert_eq!(hub.native_status("ABC").unwrap().epoch, identity);
            assert!(!hub.native_status("ABC").unwrap().paused);
        });
        assert!(barrier(&mut clients[0]).is_empty());
        assert_eq!(
            rig.calls(),
            calls,
            "stale completion cannot schedule the replacement"
        );
        rig.step();
        rig.step();
        for client in &mut clients {
            assert_eq!(ticks(&barrier(client)), [4]);
        }
    })
    .await;
}
