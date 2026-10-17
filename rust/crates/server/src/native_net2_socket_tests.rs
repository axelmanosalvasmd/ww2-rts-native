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
    now_ms: Arc<std::sync::atomic::AtomicU64>,
    capture_each_tick: bool,
    captured_seats: u64,
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
        self.poll(
            self.now_ms.load(std::sync::atomic::Ordering::SeqCst),
            self.next_ns,
        );
        if self.capture_each_tick {
            self.captured_seats += self.change(|hub| {
                let Some(native) = &hub.rooms["ABC"].native else {
                    return 0;
                };
                let state = native.runtime.state();
                state.preflight_ordinary_snapshot().unwrap();
                let cache = ww2_core::recipient_snapshot::SnapshotCache::new(state).unwrap();
                for slot in 0..state.world.game.players.len() {
                    let frame = cache.recipient(slot).unwrap();
                    assert_eq!(frame.fields["tick"], state.world.game.tick);
                }
                state.world.game.players.len() as u64
            });
        }
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
    let now_ms = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let server = crate::Server {
        hub: Arc::new(Mutex::new(hub)),
        next: Arc::new(std::sync::atomic::AtomicU64::new(1)),
        clock: std::time::Instant::now(),
        static_files: None,
