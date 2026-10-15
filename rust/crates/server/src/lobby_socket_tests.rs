//! Original lobby scenarios over actual TCP upgrade, frames and socket retirement.
//! Only the host clock and maintenance schedule are controlled by the test.
use super::*;
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    sync::atomic::AtomicU64,
    time::Duration,
};
use tokio::{
    net::TcpListener,
    sync::{Mutex, oneshot},
};

struct Client(TcpStream);
impl Client {
    fn open(address: SocketAddr, room: &str) -> Self {
        let mut socket = TcpStream::connect(address).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        socket
            .set_write_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        write!(socket, "GET /ws?room={room} HTTP/1.1\r\nHost: {address}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n").unwrap();
        let mut headers = Vec::new();
        while !headers.ends_with(b"\r\n\r\n") {
            let mut byte = [0];
            socket.read_exact(&mut byte).unwrap();
            headers.push(byte[0]);
            assert!(headers.len() < 8192);
        }
        assert!(
            String::from_utf8(headers)
                .unwrap()
                .starts_with("HTTP/1.1 101")
        );
        Self(socket)
    }
    fn send(&mut self, value: Value) {
        let payload = value.to_string().into_bytes();
        let mask = [7_u8, 11, 17, 23];
        let mut frame = vec![0x81];
        if payload.len() < 126 {
            frame.push(0x80 | payload.len() as u8);
        } else {
            frame.push(0x80 | 126);
            frame.extend(u16::try_from(payload.len()).unwrap().to_be_bytes());
        }
        frame.extend(mask);
        frame.extend(payload.iter().enumerate().map(|(i, b)| b ^ mask[i % 4]));
        self.0.write_all(&frame).unwrap();
    }
    fn frame(&mut self) -> (u8, Vec<u8>) {
        let mut header = [0; 2];
        self.0.read_exact(&mut header).unwrap();
        assert_eq!(header[0] & 0x80, 0x80);
        assert_eq!(header[1] & 0x80, 0);
        let length = match header[1] & 127 {
            126 => {
                let mut bytes = [0; 2];
                self.0.read_exact(&mut bytes).unwrap();
                usize::from(u16::from_be_bytes(bytes))
            }
            127 => {
                let mut bytes = [0; 8];
                self.0.read_exact(&mut bytes).unwrap();
                usize::try_from(u64::from_be_bytes(bytes)).unwrap()
            }
            length => usize::from(length),
        };
        assert!(length < 1024 * 1024);
        let mut bytes = vec![0; length];
        self.0.read_exact(&mut bytes).unwrap();
        (header[0] & 15, bytes)
    }
    fn receive(&mut self) -> Value {
        let (opcode, bytes) = self.frame();
        assert_eq!(opcode, 1, "expected actual JSON text frame");
        serde_json::from_slice(&bytes).unwrap()
    }
    fn until(&mut self, predicate: impl Fn(&Value) -> bool) -> Value {
        for _ in 0..128 {
            let value = self.receive();
            if predicate(&value) {
                return value;
            }
        }
        panic!("missing expected socket message");
    }
    fn lobby(&mut self) -> Value {
        self.until(|m| m["t"] == "lobby")
    }
    fn barrier(&mut self) -> Vec<Value> {
        self.send(json!({"t":"ping","c":"lobby-barrier"}));
        let mut messages = Vec::new();
        for _ in 0..128 {
            let value = self.receive();
            if value == json!({"t":"pong","c":"lobby-barrier"}) {
                return messages;
            }
            messages.push(value);
        }
        panic!("missing processing barrier");
    }
    fn closed(&mut self) {
        let (opcode, bytes) = self.frame();
        assert_eq!(opcode, 8);
        assert!(bytes.len() >= 2);
        assert_eq!(u16::from_be_bytes([bytes[0], bytes[1]]), 1000);
    }
}

struct Rig {
    host: tokio::runtime::Handle,
    hub: Arc<Mutex<Hub>>,
    now: Arc<AtomicU64>,
    address: SocketAddr,
}
impl Rig {
    fn read<T>(&self, read: impl FnOnce(&Hub) -> T) -> T {
        self.host.block_on(async { read(&*self.hub.lock().await) })
    }
    fn poll(&self, now: u64) {
        self.now.store(now, Ordering::Release);
        self.host.block_on(async {
            self.hub.lock().await.poll_native(now, now * 1_000_000);
        });
    }
    fn wait(&self, predicate: impl Fn(&Hub) -> bool) {
        self.host.block_on(async {
            tokio::time::timeout(Duration::from_secs(10), async {
                loop {
                    if predicate(&*self.hub.lock().await) {
                        break;
                    }
                    tokio::task::yield_now().await;
                }
            })
            .await
            .expect("real socket retirement must reach the Hub");
        });
    }
    fn join(&self, room: &str, token: &str, name: &str, spectate: bool) -> (Client, Value) {
        let mut client = Client::open(self.address, room);
        client.send(json!({"t":"hello","token":token,"name":name,"spectate":spectate}));
        let lobby = client.lobby();
        (client, lobby)
    }
}

async fn sockets(check: impl FnOnce(Rig) + Send + 'static) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let now = Arc::new(AtomicU64::new(0));
    let server = crate::Server {
        hub: Arc::new(Mutex::new(Hub::new(32, String::new()))),
        next: Arc::new(AtomicU64::new(1)),
        clock: Instant::now(),
        test_now_ms: Some(now.clone()),
    };
    let (stop, stopped) = watch::channel(false);
    let (finish, finished) = oneshot::channel();
    let app = axum::Router::new()
        .route("/ws", axum::routing::get(crate::upgrade))
        .route("/api/rooms", axum::routing::get(crate::directory))
        .with_state((server.clone(), stopped));
    let serving = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = finished.await;
            })
            .await
    });
    let rig = Rig {
        host: tokio::runtime::Handle::current(),
        hub: server.hub.clone(),
        now,
        address,
    };
    let checked = tokio::task::spawn_blocking(move || check(rig)).await;
    server.hub.lock().await.stop();
    let _ = stop.send(true);
    let _ = finish.send(());
    serving.await.unwrap().unwrap();
    checked.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn net19_websocket_close_at_zero_expires_only_after_sixty_seconds() {
    sockets(|rig| {
        let (ana, _) = rig.join("Expiry", "ana", "Ana", false);
        drop(ana);
        rig.wait(|hub| hub.rooms["Expiry"].players[0].connection.is_none());
        rig.read(|hub| assert_eq!(hub.rooms["Expiry"].empty_since, Some(0)));
        rig.poll(59_951);
        rig.poll(60_000);
        rig.read(|hub| assert!(hub.rooms.contains_key("Expiry")));
        rig.poll(60_001);
        rig.read(|hub| {
            assert!(!hub.rooms.contains_key("Expiry"));
            assert!(!hub.room_order.iter().any(|code| code == "Expiry"));
        });
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn net22_websocket_lowest_unused_team_is_independent_of_seat_and_faction_cycle() {
    sockets(|rig| {
        let (mut ana, _) = rig.join("Defaults", "ana", "Ana", false);
        let (ben, _) = rig.join("Defaults", "ben", "Ben", false);
        let (cy, initial) = rig.join("Defaults", "cy", "Cy", false);
        let defaults: Vec<_> = initial["players"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| (p["team"].as_u64().unwrap(), p["faction"].as_u64().unwrap()))
            .collect();
        assert_eq!(defaults, vec![(0, 0), (1, 1), (2, 2)]);
        ana.send(json!({"t":"team","slot":0,"v":4}));
        ana.until(|m| m["t"] == "lobby" && m["players"][0]["team"] == 4);
        let (dana, lobby) = rig.join("Defaults", "dana", "Dana", false);
        assert_eq!(
            lobby["players"][3]["team"], 0,
            "lowest unused team, not seat index3"
        );
        assert_eq!(lobby["players"][3]["faction"], 0, "seat count modulo3");
        drop(ben);
        ana.until(|m| m["t"] == "lobby" && m["players"][1]["connected"] == false);
        ana.send(json!({"t":"kick","slot":1}));
        ana.until(|m| m["t"] == "lobby" && m["players"].as_array().unwrap().len() == 3);
        let (eli, lobby) = rig.join("Defaults", "eli", "Eli", false);
        let defaults: Vec<_> = lobby["players"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| (p["team"].as_u64().unwrap(), p["faction"].as_u64().unwrap()))
            .collect();
        assert_eq!(defaults, vec![(4, 0), (2, 2), (0, 0), (1, 0)]);
        drop((cy, dana, eli));
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn net23_websocket_ai_numbering_and_defaults_follow_actual_remaining_ai() {
    sockets(|rig| {
        let (mut host, _) = rig.join("AiDefaults", "host", "Host", false);
        let (guest, _) = rig.join("AiDefaults", "guest", "Guest", false);
        for (count, name) in [(3, "AI 1"), (4, "AI 2")] {
            host.send(json!({"t":"addAi"}));
            let lobby = host
                .until(|m| m["t"] == "lobby" && m["players"].as_array().unwrap().len() == count);
            let ai = &lobby["players"][count - 1];
            assert_eq!(ai["name"], name);
            assert_eq!(ai["level"], "normal");
            assert_eq!(ai["ai"], true);
            assert_eq!(ai["connected"], true);
        }
        host.send(json!({"t":"level","slot":3,"v":"hard"}));
        host.until(|m| m["t"] == "lobby" && m["players"][3]["level"] == "hard");
        host.send(json!({"t":"kick","slot":2}));
        host.until(|m| m["t"] == "lobby" && m["players"].as_array().unwrap().len() == 3);
        host.send(json!({"t":"addAi"}));
        let lobby = host.lobby();
        assert_eq!(lobby["players"][2]["name"], "AI 2");
        assert_eq!(lobby["players"][2]["level"], "hard");
        assert_eq!(
            lobby["players"][3]["name"], "AI 2",
            "n counts current AI, not a global counter"
        );
        assert_eq!(lobby["players"][3]["level"], "normal");
        rig.read(|hub| {
            let room = &hub.rooms["AiDefaults"];
            assert!(
                room.players[2..]
                    .iter()
                    .all(|p| p.ai && p.token.is_empty() && p.connection.is_none())
            );
            assert!(
                room.players[..2]
                    .iter()
                    .all(|p| !p.ai && p.connection.is_some())
            );
        });
        drop(guest);
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn net24_websocket_all_authored_mode_capacities_bound_join_ai_and_sit() {
    // Read original authoring rows, rather than deriving expected seats from
    // the Hub catalogue or calling the capacity function under test.
    let catalogue: Value = serde_json::from_str(include_str!("map-catalog.json")).unwrap();
    let mut cases = Vec::new();
    for key in catalogue.as_object().unwrap().keys() {
        let map = crate::authoritative_maps::load(key).unwrap();
        let spawns = map.source()["spawns"].as_array().unwrap();
        for mode in ["conquest", "annihilation", "classic", "assault"] {
            let count = spawns
                .iter()
                .filter(|spawn| mode == "assault" || spawn["assault"] != true)
                .count();
            cases.push((mode.to_owned(), key.clone(), count));
        }
    }
    cases.extend([
        ("world".into(), "default".into(), 6),
        ("horde".into(), "bastogne".into(), 5),
        ("tutorial".into(), "tutorial".into(), 5),
    ]);
    assert_eq!(
        cases.len(),
        135,
        "33 authored maps across four modes plus three fixed capacities"
    );
    sockets(move |rig| {
        let mut now = 0;
        for (mode, map, capacity) in cases {
            eprintln!("NET-24 {mode}/{map}: {capacity} seats");
            assert!((1..=6).contains(&capacity));
            let mut host = Client::open(rig.address, "Capacity");
            host.send(json!({"t":"hello","token":"host","name":"Host","listing":{"public":true,"title":"Saturday skirmish"}}));
            host.lobby();
            host.send(json!({"t":"map","name":map}));
            host.lobby();
            if mode == "horde" {
                host.send(json!({"t":"army","v":"endless"}));
                host.lobby();
            }
            host.send(json!({"t":"mode","v":mode}));
            let lobby = host.lobby();
            assert_eq!(lobby["spawns"], capacity, "{mode}/{map}");
            assert_eq!(lobby["mode"], mode);
            if mode == "horde" {
                assert_eq!(lobby["army"], "standard");
                assert!(lobby["hordeMaps"].as_array().unwrap().contains(&lobby["mapName"]));
                assert_eq!(lobby["hordeBest"], Value::Null);
            }
            let mut humans = Vec::new();
            for i in 1..capacity {
                let (client, lobby) = rig.join("Capacity", &format!("human{i}"), "Human", false);
                assert_eq!(lobby["spectator"], false);
                assert_eq!(lobby["players"].as_array().unwrap().len(), i + 1);
                humans.push(client);
            }
            let (mut late, lobby) = rig.join("Capacity", "late", "Late", false);
            assert_eq!(lobby["spectator"], true);
            assert_eq!(lobby["you"], -1);
            assert_eq!(lobby["players"].as_array().unwrap().len(), capacity);
            host.send(json!({"t":"addAi"}));
            host.barrier();
            late.send(json!({"t":"sit"}));
            late.barrier();
            rig.read(|hub| {
                let room = &hub.rooms["Capacity"];
                assert_eq!(room.players.len(), capacity);
                assert!(room.players.iter().all(|p| !p.ai));
                assert_eq!(room.spectators.len(), 1);
            });
            drop(host);
            drop(humans);
            drop(late);
            rig.wait(|hub| hub.rooms["Capacity"].people().all(|p| p.connection.is_none()));
            now += 60_001;
            rig.poll(now);
            rig.read(|hub| assert!(!hub.rooms.contains_key("Capacity")));
        }
    }).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn net30_websocket_host_passes_to_connected_human_with_kick_guards() {
    sockets(|rig| {
        let (ana, _) = rig.join("Host", "ana", "Ana", false);
        let (mut ben, _) = rig.join("Host", "ben", "Ben", false);
        let (mut cy, _) = rig.join("Host", "cy", "Cy", false);
        drop(ana);
        let lobby = ben.until(|m| {
            m["t"] == "lobby" && m["host"] == 1 && m["players"][0]["connected"] == false
        });
        assert_eq!(lobby["amHost"], true);
        rig.poll(9_999);
        rig.read(|hub| assert_eq!(hub.rooms["Host"].players.len(), 3));
        cy.send(json!({"t":"kick","slot":0}));
        cy.barrier();
        rig.read(|hub| assert_eq!(hub.rooms["Host"].players.len(), 3));
        ben.send(json!({"t":"kick","slot":0}));
        let lobby = ben.until(|m| m["t"] == "lobby" && m["you"] == 0 && m["host"] == 0);
        assert_eq!(lobby["players"].as_array().unwrap().len(), 2);
        ben.send(json!({"t":"kick","slot":1}));
        ben.barrier();
        rig.read(|hub| assert_eq!(hub.rooms["Host"].players.len(), 2));
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn net30_websocket_host_recomputes_across_humans_offline_fallback_and_watchers() {
    sockets(|rig| {
        let (mut ana, _) = rig.join("Priority", "ana", "Ana", false);
        let (mut ben, _) = rig.join("Priority", "ben", "Ben", false);
        ana.send(json!({"t":"addAi"}));
        ana.until(|m| m["t"] == "lobby" && m["players"].as_array().unwrap().len() == 3);
        let (mut fern, _) = rig.join("Priority", "fern", "Fern", true);
        let (mut gus, _) = rig.join("Priority", "gus", "Gus", true);
        drop(ana);
        let lobby = ben.until(|m| m["t"] == "lobby" && m["players"][0]["connected"] == false);
        assert_eq!(lobby["host"], 1);
        assert_eq!(lobby["amHost"], true);
        drop(ben);
        let lobby = fern.until(|m| m["t"] == "lobby" && m["players"][1]["connected"] == false);
        assert_eq!(
            lobby["host"], 0,
            "first offline human remains ahead of watchers"
        );
        assert_eq!(lobby["amHost"], false);
        rig.poll(10_000);
        let lobby =
            fern.until(|m| m["t"] == "lobby" && m["players"].as_array().unwrap().len() == 1);
        assert_eq!(lobby["host"], -1);
        assert_eq!(lobby["amHost"], true);
        let lobby = gus.until(|m| m["t"] == "lobby" && m["players"].as_array().unwrap().len() == 1);
        assert_eq!(lobby["amHost"], false);
        gus.send(json!({"t":"addAi"}));
        gus.barrier();
        rig.read(|hub| assert_eq!(hub.rooms["Priority"].players.len(), 1));
        fern.send(json!({"t":"addAi"}));
        let lobby = fern.lobby();
        assert_eq!(lobby["players"].as_array().unwrap().len(), 2);
        assert_eq!(lobby["amHost"], true);
        drop(fern);
        let lobby =
            gus.until(|m| m["t"] == "lobby" && m["spectators"].as_array().unwrap().len() == 1);
        assert_eq!(lobby["amHost"], true);
        gus.send(json!({"t":"addAi"}));
        assert_eq!(gus.lobby()["players"].as_array().unwrap().len(), 3);
    })
    .await;
}
