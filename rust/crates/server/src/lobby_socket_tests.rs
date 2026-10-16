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

struct HeldFrame {
    entered: oneshot::Receiver<()>,
    release: oneshot::Sender<()>,
    dispatched: oneshot::Receiver<()>,
}

struct Rig {
    host: tokio::runtime::Handle,
    hub: Arc<Mutex<Hub>>,
    now: Arc<AtomicU64>,
    address: SocketAddr,
    ingress_pause: crate::TestIngressGate,
}
impl Rig {
    fn hold_next_frame(&self, connection: u64, command: &'static str) -> HeldFrame {
        let (entered, waiting) = oneshot::channel();
        let (release, released) = oneshot::channel();
        let (dispatched, complete) = oneshot::channel();
        self.host.block_on(async {
            let mut pending = self.ingress_pause.lock().await;
            assert!(pending.is_none());
            *pending = Some(crate::TestIngressPause {
                connection,
                command,
                entered,
                release: released,
                dispatched,
            });
        });
        HeldFrame {
            entered: waiting,
            release,
            dispatched: complete,
        }
    }
    fn wait_signal(&self, signal: oneshot::Receiver<()>) {
        self.host.block_on(async {
            tokio::time::timeout(Duration::from_secs(10), signal)
                .await
                .expect("actual socket handler must reach the scheduled boundary")
                .expect("scheduled socket handler must stay live");
        });
    }
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
    let ingress_pause = Arc::new(Mutex::new(None));
    let server = crate::Server {
        hub: Arc::new(Mutex::new(Hub::new(32, String::new()))),
        next: Arc::new(AtomicU64::new(1)),
        clock: Instant::now(),
        test_now_ms: Some(now.clone()),
        test_ingress_pause: Some(ingress_pause.clone()),
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
        ingress_pause,
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
        rig.poll(1);
        let (ben, _) = rig.join("ExpiryNext", "ben", "Ben", false);
        drop(ben);
        rig.wait(|hub| hub.rooms["ExpiryNext"].players[0].connection.is_none());
        rig.read(|hub| assert_eq!(hub.rooms["ExpiryNext"].empty_since, Some(1)));
        rig.poll(59_951);
        rig.poll(60_000);
        rig.read(|hub| assert!(hub.rooms.contains_key("Expiry")));
        rig.read(|hub| assert_eq!(hub.clock.due_ns(), 60_001_000_000));
        rig.poll(60_001);
        rig.read(|hub| {
            assert!(!hub.rooms.contains_key("Expiry"));
            assert!(!hub.room_order.iter().any(|code| code == "Expiry"));
            assert_eq!(hub.clock.due_ns(), 60_051_000_000);
            let younger = &hub.rooms["ExpiryNext"];
            assert_eq!(younger.empty_since, Some(1));
            assert_eq!(60_001 - younger.empty_since.unwrap(), 60_000);
            assert!(hub.room_order.iter().any(|code| code == "ExpiryNext"));
        });
        rig.poll(60_051);
        rig.read(|hub| {
            assert_eq!(hub.clock.due_ns(), 60_101_000_000);
            assert!(!hub.rooms.contains_key("ExpiryNext"));
            assert!(!hub.room_order.iter().any(|code| code == "ExpiryNext"));
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn net20_net29_websocket_trim_preserves_original_bom_nel_and_controls() {
    sockets(|rig| {
        let mut host = Client::open(rig.address, "Trim");
        host.send(json!({"t":"hello","token":"host","name":"\u{feff}Name\u{feff}","listing":{"public":true,"title":"\u{feff}trim\u{feff}"}}));
        assert_eq!(host.lobby()["players"][0]["name"], "Name");
        rig.read(|hub| assert_eq!(hub.rooms["Trim"].title, "trim"));
        // Retain the original fixture's positive and negative trim boundaries.
        for (input, expected) in [
            ("\u{feff}Name\u{feff}", "Name"),
            ("\u{0085}Name\u{0085}", "\u{0085}Name\u{0085}"),
            ("N\u{0001}ame", "N\u{0001}ame"),
            (" \t\n\r\u{00a0}Name\u{2028}\u{2029} ", "Name"),
            ("\u{feff}\u{feff}", "Soldier"),
        ] {
            host.send(json!({"t":"name","name":input}));
            assert_eq!(host.lobby()["players"][0]["name"], expected);
        }
        let mut http = TcpStream::connect(rig.address).unwrap();
        http.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        http.set_write_timeout(Some(Duration::from_secs(10))).unwrap();
        write!(http, "GET /api/rooms HTTP/1.0\r\nHost: {}\r\nConnection: close\r\n\r\n", rig.address).unwrap();
        let mut response = Vec::new();
        http.read_to_end(&mut response).unwrap();
        let split = response.windows(4).position(|part| part == b"\r\n\r\n").unwrap();
        assert!(response.starts_with(b"HTTP/1.0 200") || response.starts_with(b"HTTP/1.1 200"));
        let directory: Value = serde_json::from_slice(&response[split + 4..]).unwrap();
        assert_eq!(directory["rooms"][0]["title"], "trim");
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn net20_websocket_token_slice_keeps_utf16_identity_and_empty_tokens_distinct() {
    sockets(|rig| {
        let first = format!("{}\u{10000}", "x".repeat(39));
        let second = format!("{}\u{10400}", "x".repeat(39));
        let (mut a, _) = rig.join("Tokens", &first, "First", false);
        let (b, lobby) = rig.join("Tokens", &second, "Second", false);
        assert_eq!(lobby["players"].as_array().unwrap().len(), 2);
        a.barrier();
        // These scalars differ only in the low surrogate. Slicing Rust chars
        // would retain both complete scalars and incorrectly allocate a seat.
        let same_prefix = format!("{}\u{10001}", "x".repeat(39));
        let (mut shared, lobby) = rig.join("Tokens", &same_prefix, "SameUnits", false);
        assert_eq!(lobby["players"].as_array().unwrap().len(), 2);
        assert_eq!(a.receive(), json!({"t":"replaced"}));
        a.closed();
        shared.barrier();
        let (replacement, lobby) =
            rig.join("Tokens", &(first.clone() + "suffix"), "Returned", false);
        assert_eq!(lobby["players"].as_array().unwrap().len(), 2);
        assert_eq!(lobby["players"][0]["name"], "Returned");
        assert_eq!(lobby["players"][1]["name"], "Second");
        assert_eq!(shared.receive(), json!({"t":"replaced"}));
        shared.closed();
        let paired = format!("{}\u{10000}z", "x".repeat(38));
        let (paired_client, lobby) = rig.join("Tokens", &paired, "Paired", false);
        assert_eq!(lobby["players"].as_array().unwrap().len(), 3);
        let (empty_a, _) = rig.join("Tokens", "", "EmptyA", false);
        let (empty_b, lobby) = rig.join("Tokens", "", "EmptyB", false);
        assert_eq!(lobby["players"].as_array().unwrap().len(), 5);
        rig.read(|hub| {
            let people = &hub.rooms["Tokens"].players;
            assert_eq!(people[0].token.len(), 40);
            assert_eq!(people[0].token[39], 0xd800);
            assert_eq!(people[1].token[39], 0xd801);
            assert_eq!(&people[2].token[38..], &[0xd800, 0xdc00]);
            assert!(people[3].token.is_empty());
            assert!(people[4].token.is_empty());
        });
        drop((replacement, b, paired_client, empty_a, empty_b));
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn net21_websocket_explicit_watch_and_ordinary_sit_keep_admission_priority() {
    sockets(|rig| {
        let (mut host, _) = rig.join("SeatChoice", "host", "Host", false);
        let (mut watcher, lobby) = rig.join("SeatChoice", "watcher", "Watcher", true);
        assert_eq!(lobby["players"].as_array().unwrap().len(), 1);
        assert_eq!(lobby["spectators"], json!(["Watcher"]));
        assert_eq!(lobby["you"], -1);
        assert_eq!(
            lobby["spectator"], true,
            "explicit watch wins even while a seat is free"
        );
        host.barrier();
        watcher.send(json!({"t":"sit"}));
        let lobby = watcher.lobby();
        assert_eq!(lobby["you"], 1);
        assert_eq!(lobby["spectator"], false);
        assert_eq!(lobby["spectators"], json!([]));
        assert_eq!(lobby["players"][1]["team"], 1);
        assert_eq!(lobby["players"][1]["faction"], 1);
        host.barrier();
        watcher.send(json!({"t":"spectate"}));
        let lobby = watcher.lobby();
        assert_eq!(lobby["you"], -1);
        assert_eq!(lobby["players"].as_array().unwrap().len(), 1);
        assert_eq!(lobby["spectators"], json!(["Watcher"]));
        host.until(|m| {
            m["t"] == "lobby"
                && m["players"].as_array().unwrap().len() == 1
                && m["spectators"] == json!(["Watcher"])
        });
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn net25_net29_websocket_spectator_rejoin_retains_person_before_seating_hint() {
    sockets(|rig| {
        let (mut host, _) = rig.join("watchid", "host", "Host", false);
        let (mut old, _) = rig.join("watchid", "watch", "Before", true);
        old.send(json!({"t":"ping","c":"rtt","rtt":37}));
        assert_eq!(old.receive(), json!({"t":"pong","c":"rtt"}));
        let before = rig.read(|hub| {
            let person = &hub.rooms["watchid"].spectators[0];
            (
                person as *const Person as usize,
                person.token.clone(),
                person.rtt,
            )
        });
        // Identity lookup precedes a new caller's request for a seat.
        let (mut current, lobby) = rig.join("watchid", "watch", "Returned", false);
        assert_eq!(lobby["spectator"], true);
        assert_eq!(lobby["you"], -1);
        assert_eq!(lobby["players"].as_array().unwrap().len(), 1);
        assert_eq!(lobby["spectators"], json!(["Returned"]));
        assert_eq!(old.receive(), json!({"t":"replaced"}));
        old.closed();
        current.barrier();
        rig.read(|hub| {
            let person = &hub.rooms["watchid"].spectators[0];
            assert_eq!(
                (
                    person as *const Person as usize,
                    person.token.clone(),
                    person.rtt
                ),
                before
            );
            assert!(person.connection.is_some());
        });
        current.send(json!({"t":"name","name":"<Renamed>"}));
        assert_eq!(current.lobby()["spectators"], json!(["Renamed"]));
        host.until(|m| m["t"] == "lobby" && m["spectators"] == json!(["Renamed"]));
        current.send(json!({"t":"name","name":{"toString":null,"valueOf":null}}));
        assert!(current.barrier().is_empty());
        rig.read(|hub| assert_eq!(hub.rooms["watchid"].spectators[0].name, "Renamed"));
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn net20_net29_websocket_paired_utf16_names_keep_exact_units_and_peer_broadcasts() {
    sockets(|rig| {
        let input = format!("\u{feff}<{}ABtail>\u{feff}", "\u{1f600}".repeat(7));
        let expected = format!("{}AB", "\u{1f600}".repeat(7));
        let (mut host, lobby) = rig.join("PairNames", "host", &input, false);
        assert_eq!(lobby["players"][0]["name"], expected);
        let (mut peer, lobby) = rig.join("PairNames", "peer", "Peer", false);
        assert_eq!(lobby["players"][0]["name"], expected);
        host.barrier();
        for (input, expected) in [
            (
                format!("{}tail", "\u{1f600}".repeat(8)),
                "\u{1f600}".repeat(8),
            ),
            ("\u{feff}<>&\"' \t".into(), "Soldier".into()),
            ("\u{0085}<A>\u{0085}".into(), "\u{0085}A\u{0085}".into()),
        ] {
            peer.barrier();
            host.send(json!({"t":"name","name":input}));
            assert_eq!(host.lobby()["players"][0]["name"], expected);
            let lobby = peer.until(|m| m["t"] == "lobby" && m["players"][0]["name"] == expected);
            assert_eq!(lobby["players"][1]["name"], "Peer");
        }
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn net21_websocket_full_watchers_still_allow_an_ordinary_free_seat() {
    sockets(|rig| {
        let (host, _) = rig.join("WatchLimit", "host", "Host", false);
        let mut watchers = Vec::new();
        for index in 0..8 {
            let (watcher, lobby) = rig.join("WatchLimit", &format!("watch{index}"), "Watch", true);
            assert_eq!(lobby["you"], -1);
            assert_eq!(lobby["players"].as_array().unwrap().len(), 1);
            assert_eq!(lobby["spectators"].as_array().unwrap().len(), index + 1);
            watchers.push(watcher);
        }
        let (mut seated, lobby) = rig.join("WatchLimit", "seat", "Seat", false);
        assert_eq!(lobby["you"], 1, "a free seat is considered before the watcher cap");
        assert_eq!(lobby["spectator"], false);
        assert_eq!(lobby["spectators"].as_array().unwrap().len(), 8);
        let mut rejected = Client::open(rig.address, "WatchLimit");
        rejected.send(json!({"t":"hello","token":"extra","name":"Extra","spectate":true}));
        assert_eq!(rejected.receive(), json!({"t":"full","reason":"seats"}));
        rejected.closed();
        let mut seated_non_booleans = Vec::new();
        for (index, spectate) in [json!("true"), json!(1), json!({}), json!([true])].into_iter().enumerate() {
            let mut client = Client::open(rig.address, "WatchLimit");
            client.send(json!({"t":"hello","token":format!("strict{index}"),"name":"Strict","spectate":spectate}));
            let lobby = client.lobby();
            assert_eq!(lobby["you"], index + 2, "only boolean true requests watch");
            assert_eq!(lobby["spectator"], false);
            assert_eq!(lobby["spectators"].as_array().unwrap().len(), 8);
            seated_non_booleans.push(client);
        }
        seated.barrier();
        rig.read(|hub| {
            assert_eq!(hub.rooms["WatchLimit"].players.len(), 6);
            assert_eq!(hub.rooms["WatchLimit"].spectators.len(), 8);
        });
        drop((host, watchers, seated_non_booleans));
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn net25_websocket_return_cancels_cleanup_across_an_actual_due_callback() {
    sockets(|rig| {
        let (mut host, _) = rig.join("ReturnDue", "host", "Host", false);
        let (mut guest, _) = rig.join("ReturnDue", "guest", "Before", false);
        guest.send(json!({"t":"ping","c":"remember","rtt":37}));
        assert_eq!(guest.receive(), json!({"t":"pong","c":"remember"}));
        host.send(json!({"t":"team","slot":1,"v":4}));
        host.until(|m| m["t"] == "lobby" && m["players"][1]["team"] == 4);
        guest.barrier();
        let mut expected = rig.read(|hub| {
            let room = &hub.rooms["ReturnDue"];
            let p = &room.players[1];
            assert_eq!(p.token, "guest".encode_utf16().collect::<Vec<_>>());
            assert_eq!(p.rtt, Some(37.0));
            assert_eq!(p.team, 4);
            assert_eq!(p.faction, 1);
            assert_eq!(
                room.people()
                    .filter(|p| p.token == room.players[1].token)
                    .count(),
                1
            );
            room_people(room)
        });
        let previous_connection = expected.0[1].connection.unwrap();
        drop(guest);
        host.until(|m| m["t"] == "lobby" && m["players"][1]["connected"] == false);
        expected.0[1].connection = None;
        expected.0[1].disconnected = Some(0);
        rig.read(|hub| {
            assert_eq!(hub.rooms["ReturnDue"].players[1].disconnected, Some(0));
            assert_eq!(room_people(&hub.rooms["ReturnDue"]), expected);
        });
        rig.poll(9_951);
        rig.poll(9_999);
        rig.read(|hub| assert_eq!(room_people(&hub.rooms["ReturnDue"]), expected));
        let (mut returned, lobby) = rig.join("ReturnDue", "guest", "Returned", true);
        assert_eq!(
            lobby["you"], 1,
            "existing identity takes precedence over a new watch hint"
        );
        assert_eq!(lobby["spectator"], false);
        let current_connection = rig.read(|hub| {
            let p = &hub.rooms["ReturnDue"].players[1];
            let connection = p.connection.as_ref().unwrap();
            assert_ne!(connection.id, previous_connection);
            assert!(connection.ready());
            connection.clone()
        });
        expected.0[1].name = "Returned".into();
        expected.0[1].connection = Some(current_connection.id);
        expected.0[1].disconnected = None;
        rig.read(|hub| assert_eq!(room_people(&hub.rooms["ReturnDue"]), expected));
        rig.poll(10_001);
        rig.poll(20_002);
        returned.barrier();
        rig.read(|hub| {
            let room = &hub.rooms["ReturnDue"];
            assert_eq!(room.players.len(), 2);
            assert!(room.spectators.is_empty());
            let p = &room.players[1];
            // Cleanup moves retained values. Token membership and complete state
            // identify the person; channel equality identifies the current socket.
            assert_eq!(room_people(room), expected);
            assert_eq!(
                room.people()
                    .filter(|p| p.token == expected.0[1].token)
                    .count(),
                1
            );
            let connection = p.connection.as_ref().unwrap();
            assert_eq!(connection.id, current_connection.id);
            assert!(connection.tx.same_channel(&current_connection.tx));
            assert!(connection.cancel.same_channel(&current_connection.cancel));
            assert!(connection.ready());
            assert!(!room.people().any(|p| {
                p.connection
                    .as_ref()
                    .is_some_and(|c| c.id == previous_connection)
            }));
            assert_eq!(p.name, "Returned");
            assert_eq!(p.disconnected, None);
            assert!(p.connection.is_some());
        });
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn net27_actual_retired_connection_ids_cannot_reclaim_replaced_or_expired_seats() {
    sockets(|rig| {
        let (mut host, _) = rig.join("RetiredIds", "host", "Host", false);
        let (mut old, _) = rig.join("RetiredIds", "guest", "Before", false);
        let retired = rig.read(|hub| {
            hub.rooms["RetiredIds"].players[1]
                .connection
                .clone()
                .unwrap()
        });
        let (mut current, _) = rig.join("RetiredIds", "guest", "Current", false);
        assert_eq!(old.receive(), json!({"t":"replaced"}));
        old.closed();
        current.barrier();
        // Invoke the ordinary consumer with a real retired transport identity.
        // This is a guard-component check, not proof of a late frame crossing TCP.
        let reject_stale = |connection: &Connection| {
            rig.host.block_on(async {
                let mut hub = rig.hub.lock().await;
                for message in [
                    json!({"t":"name","name":"Stale"}),
                    json!({"t":"team","slot":1,"v":5}),
                    json!({"t":"ping","c":"stale","rtt":999}),
                ] {
                    hub.message("RetiredIds", connection, &mut true, message, 0);
                }
            });
        };
        reject_stale(&retired);
        current.barrier();
        rig.read(|hub| {
            let p = &hub.rooms["RetiredIds"].players[1];
            assert_eq!(p.name, "Current");
            assert_eq!(p.team, 1);
            assert_eq!(p.rtt, None);
        });
        current.send(json!({"t":"name","name":"Live"}));
        current.until(|m| m["t"] == "lobby" && m["players"][1]["name"] == "Live");
        let departed = rig.read(|hub| {
            hub.rooms["RetiredIds"].players[1]
                .connection
                .clone()
                .unwrap()
        });
        drop(current);
        host.until(|m| m["t"] == "lobby" && m["players"][1]["connected"] == false);
        rig.poll(10_001);
        rig.read(|hub| assert_eq!(hub.rooms["RetiredIds"].players.len(), 1));
        let (mut newcomer, _) = rig.join("RetiredIds", "new", "New", false);
        reject_stale(&departed);
        newcomer.barrier();
        rig.read(|hub| {
            let p = &hub.rooms["RetiredIds"].players[1];
            assert_eq!(p.name, "New");
            assert_eq!(p.team, 1);
            assert_eq!(p.rtt, None);
        });
        newcomer.send(json!({"t":"name","name":"LiveNew"}));
        newcomer.until(|m| m["t"] == "lobby" && m["players"][1]["name"] == "LiveNew");
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn net27_websocket_parsed_old_frame_is_ignored_after_same_token_replacement() {
    sockets(|rig| {
        let (mut host, _) = rig.join("LateFrame", "host", "Host", false);
        let (mut old, _) = rig.join("LateFrame", "guest", "Before", false);
        let old_id = rig.read(|hub| {
            hub.rooms["LateFrame"].players[1]
                .connection
                .as_ref()
                .unwrap()
                .id
        });
        let HeldFrame {
            entered,
            release,
            dispatched,
        } = rig.hold_next_frame(old_id, "name");
        old.send(json!({"t":"name","name":"Stale"}));
        rig.wait_signal(entered);
        // The real frame is parsed, but its ordinary Hub consumer has not run.
        let (mut current, lobby) = rig.join("LateFrame", "guest", "Current", false);
        assert_eq!(lobby["players"][1]["name"], "Current");
        let current_id = rig.read(|hub| {
            hub.rooms["LateFrame"].players[1]
                .connection
                .as_ref()
                .unwrap()
                .id
        });
        assert_ne!(old_id, current_id);
        host.barrier();
        release.send(()).unwrap();
        rig.wait_signal(dispatched);
        assert_eq!(old.receive(), json!({"t":"replaced"}));
        old.closed();
        current.barrier();
        rig.read(|hub| {
            let person = &hub.rooms["LateFrame"].players[1];
            assert_eq!(person.name, "Current");
            assert_eq!(person.connection.as_ref().unwrap().id, current_id);
            assert_eq!(person.disconnected, None);
        });
        current.send(json!({"t":"name","name":"Live"}));
        current.until(|m| m["t"] == "lobby" && m["players"][1]["name"] == "Live");
        host.until(|m| m["t"] == "lobby" && m["players"][1]["name"] == "Live");
    })
    .await;
}

fn assert_recipient_rosters(views: [Value; 3], players: Value, spectators: Value, slots: [i64; 3]) {
    let mut common = None;
    for (index, (mut view, slot)) in views.into_iter().zip(slots).enumerate() {
        assert_eq!(view["t"], "lobby");
        assert_eq!(view["state"], "lobby");
        assert_eq!(view["players"], players);
        assert_eq!(view["spectators"], spectators);
        assert_eq!(view["you"], slot);
        assert_eq!(view["spectator"], slot < 0);
        assert_eq!(view["amHost"], index == 0);
        assert_eq!(view["host"], 0);
        assert_eq!(view["role"], "commander");
        assert_eq!(view["operatives"], json!([]));
        for row in view["players"].as_array().unwrap() {
            for private in ["token", "connection", "denies", "rtt", "disconnected"] {
                assert!(
                    row.get(private).is_none(),
                    "private roster field: {private}"
                );
            }
        }
        for key in ["you", "spectator", "amHost"] {
            view.as_object_mut().unwrap().remove(key);
        }
        if let Some(expected) = &common {
            assert_eq!(
                &view, expected,
                "shared fields must agree across recipients"
            );
        } else {
            common = Some(view);
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn net21_net29_websocket_rosters_follow_actual_name_and_seating_transitions() {
    // Source server.js:194,333,341-349,638,704-707 at e4e579a.
    sockets(|rig| {
        let (mut host, _) = rig.join("Recipients", "host", "Host", false);
        let (mut guest, _) = rig.join("Recipients", "guest", "Guest", false);
        let (mut watcher, _) = rig.join("Recipients", "watch", "Watcher", true);
        host.barrier();
        guest.barrier();
        watcher.barrier();
        let row = |name: &str, team: usize, faction: usize| {
            json!({"name":name,"connected":true,"ai":false,"team":team,
                "faction":faction,"level":null})
        };
        host.send(json!({"t":"team","slot":0,"v":4}));
        assert_recipient_rosters(
            [host.lobby(), guest.lobby(), watcher.lobby()],
            json!([row("Host", 4, 0), row("Guest", 1, 1)]),
            json!(["Watcher"]),
            [0, 1, -1],
        );
        guest.send(json!({"t":"faction","slot":1,"v":2}));
        let players = json!([row("Host", 4, 0), row("Guest", 1, 2)]);
        assert_recipient_rosters(
            [host.lobby(), guest.lobby(), watcher.lobby()],
            players.clone(),
            json!(["Watcher"]),
            [0, 1, -1],
        );
        watcher.send(json!({"t":"name","name":"\u{feff}<\u{1f600}Fern>\u{feff}"}));
        assert_recipient_rosters(
            [host.lobby(), guest.lobby(), watcher.lobby()],
            players.clone(),
            json!(["\u{1f600}Fern"]),
            [0, 1, -1],
        );
        // A malformed name must not change either the producer or any recipient.
        for invalid in [Value::Null, json!(false), json!(7), json!([]), json!({})] {
            guest.send(json!({"t":"name","name":invalid.clone()}));
            watcher.send(json!({"t":"name","name":invalid}));
            assert!(guest.barrier().is_empty());
            assert!(watcher.barrier().is_empty());
            assert!(host.barrier().is_empty());
            rig.read(|hub| {
                let room = &hub.rooms["Recipients"];
                assert_eq!(room.players[1].name, "Guest");
                assert_eq!(room.spectators[0].name, "\u{1f600}Fern");
            });
        }
        watcher.send(json!({"t":"ping","c":"remember","rtt":37}));
        assert_eq!(watcher.receive(), json!({"t":"pong","c":"remember"}));
        let identity = rig.read(|hub| {
            let person = &hub.rooms["Recipients"].spectators[0];
            (
                person.token.clone(),
                person.connection.as_ref().unwrap().id,
                person.rtt,
            )
        });
        watcher.send(json!({"t":"sit"}));
        assert_recipient_rosters(
            [host.lobby(), guest.lobby(), watcher.lobby()],
            json!([
                row("Host", 4, 0),
                row("Guest", 1, 2),
                row("\u{1f600}Fern", 0, 2)
            ]),
            json!([]),
            [0, 1, 2],
        );
        rig.read(|hub| {
            let person = &hub.rooms["Recipients"].players[2];
            assert_eq!(
                (
                    person.token.clone(),
                    person.connection.as_ref().unwrap().id,
                    person.rtt
                ),
                identity,
            );
        });
        watcher.send(json!({"t":"spectate"}));
        assert_recipient_rosters(
            [host.lobby(), guest.lobby(), watcher.lobby()],
            players,
            json!(["\u{1f600}Fern"]),
            [0, 1, -1],
        );
        rig.read(|hub| {
            let person = &hub.rooms["Recipients"].spectators[0];
            assert_eq!(
                (
                    person.token.clone(),
                    person.connection.as_ref().unwrap().id,
                    person.rtt
                ),
                identity,
            );
        });
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn net27_websocket_replaced_parsed_command_preserves_current_denial_budget() {
    // Source server.js:596-602,620. Native refusal traffic is a guard control,
    // not the missing successful live-buy/departed-in-play Source scenario.
    sockets(|rig| {
        let (mut host, _) = rig.join("LateBudget", "host", "Host", false);
        let (mut old, _) = rig.join("LateBudget", "guest", "Before", false);
        host.barrier();
        for command in ["save", "buy", "restart"] {
            old.send(json!({"t":command}));
            let denial = old.receive();
            assert_eq!(denial["t"], "deny");
            assert_eq!(denial["cmd"], command);
        }
        let old_id = rig.read(|hub| {
            let person = &hub.rooms["LateBudget"].players[1];
            assert_eq!(person.denies, vec![0, 0, 0]);
            person.connection.as_ref().unwrap().id
        });
        let HeldFrame {
            entered,
            release,
            dispatched,
        } = rig.hold_next_frame(old_id, "buy");
        old.send(json!({"t":"buy","unit":"rifle"}));
        rig.wait_signal(entered);
        let (mut current, _) = rig.join("LateBudget", "guest", "Current", false);
        assert!(current.barrier().is_empty());
        host.barrier();
        let before = rig.read(|hub| {
            let room = &hub.rooms["LateBudget"];
            let person = &room.players[1];
            assert_eq!(person.denies, vec![0, 0, 0]);
            let id = person.connection.as_ref().unwrap().id;
            assert_ne!(id, old_id);
            (
                id,
                person.denies.clone(),
                room.lobby(id, &hub.maps, &hub.public_url),
            )
        });
        release.send(()).unwrap();
        rig.wait_signal(dispatched);
        assert_eq!(old.receive(), json!({"t":"replaced"}));
        old.closed();
        assert!(
            current.barrier().is_empty(),
            "stale input must emit no current output"
        );
        assert!(
            host.barrier().is_empty(),
            "stale input must emit no peer broadcast"
        );
        rig.read(|hub| {
            let room = &hub.rooms["LateBudget"];
            let person = &room.players[1];
            assert_eq!(person.connection.as_ref().unwrap().id, before.0);
            assert_eq!(person.denies, before.1);
            assert_eq!(room.lobby(before.0, &hub.maps, &hub.public_url), before.2);
            assert_eq!(person.disconnected, None);
        });
        // The current socket can still consume the fourth real denial. A stale
        // callback charged to the replacement would suppress this response.
        current.send(json!({"t":"buy","unit":"rifle"}));
        assert_eq!(
            current.receive(),
            json!({"t":"deny","cmd":"buy","reason":"blocked"})
        );
        current.send(json!({"t":"save"}));
        assert!(
            current.barrier().is_empty(),
            "the fifth denial is rate limited"
        );
        assert!(host.barrier().is_empty());
        rig.read(|hub| assert_eq!(hub.rooms["LateBudget"].players[1].denies, vec![0, 0, 0, 0]));
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn net20_net29_websocket_malformed_text_keeps_connection_and_name() {
    sockets(|rig| {
        let mut client = Client::open(rig.address, "Badtext");
        let invalid = json!({"toString":null,"valueOf":null});
        client.send(json!({"t":"addAi"}));
        for field in ["token", "name"] {
            let mut hello = json!({"t":"hello","token":"host","name":"Host"});
            hello[field] = invalid.clone();
            client.send(hello);
        }
        client.send(json!({"t":"hello","token":"host","name":"Host"}));
        let initial = client.receive();
        assert_eq!(
            initial["t"], "lobby",
            "no malformed hello response precedes valid admission"
        );
        assert_eq!(initial["players"].as_array().unwrap().len(), 1);
        client.send(json!({"t":"name","name":invalid}));
        assert!(
            client.barrier().is_empty(),
            "malformed rename emits no lobby and keeps the socket live"
        );
        rig.read(|hub| assert_eq!(hub.rooms["Badtext"].players[0].name, "Host"));
        for (input, expected) in [
            ("  <>&\"'Alice  ", "Alice"),
            ("  ", "Soldier"),
            ("abcdefghijklmnopq", "abcdefghijklmnop"),
        ] {
            client.send(json!({"t":"name","name":input}));
            assert_eq!(client.lobby()["players"][0]["name"], expected);
        }
        let (mut spectator, _) = rig.join("Badtext", "watcher", "Watcher", true);
        spectator.send(json!({"t":"name","name":invalid}));
        assert!(spectator.barrier().is_empty());
        spectator.send(json!({"t":"name","name":"<Fern>"}));
        assert_eq!(spectator.lobby()["spectators"], json!(["Fern"]));
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn net21_websocket_six_seats_eight_watchers_then_full_close() {
    sockets(|rig| {
        let (mut ana, _) = rig.join("Full", "ana", "Ana", false);
        for i in 1..6 {
            ana.send(json!({"t":"addAi"}));
            assert_eq!(ana.lobby()["players"].as_array().unwrap().len(), i + 1);
        }
        let (invite, message) = rig.join("Full", "invite", "Invite", false);
        assert_eq!(message["spectator"], true);
        assert_eq!(message["you"], -1);
        let mut watchers = vec![invite];
        for i in 1..8 {
            let (watcher, lobby) = rig.join("Full", &format!("watch{i}"), "Watch", false);
            assert_eq!(lobby["spectators"].as_array().unwrap().len(), i + 1);
            watchers.push(watcher);
        }
        let mut late = Client::open(rig.address, "Full");
        late.send(json!({"t":"hello","token":"late","name":"Late"}));
        assert_eq!(late.receive(), json!({"t":"full","reason":"seats"}));
        late.closed();
        rig.read(|hub| {
            assert_eq!(hub.rooms["Full"].players.len(), 6);
            assert_eq!(hub.rooms["Full"].spectators.len(), 8);
        });
        drop(watchers);
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn net25_websocket_replacement_retains_seat_budget_and_cancels_cleanup() {
    sockets(|rig| {
        let (mut old, _) = rig.join("Replace", "host", "Old", false);
        old.send(json!({"t":"start"}));
        assert_eq!(old.until(|m| m["t"] == "deny")["cmd"], "start");
        let before = rig.read(|hub| {
            let p = &hub.rooms["Replace"].players[0];
            (p as *const Person as usize, p.denies.clone())
        });
        let (mut current, lobby) = rig.join("Replace", "host", "Current", false);
        assert_eq!(lobby["players"].as_array().unwrap().len(), 1);
        assert_eq!(lobby["players"][0]["name"], "Current");
        assert_eq!(old.receive(), json!({"t":"replaced"}));
        old.closed();
        current.barrier();
        rig.read(|hub| {
            let p = &hub.rooms["Replace"].players[0];
            assert_eq!((p as *const Person as usize, p.denies.clone()), before);
        });
        drop(current);
        rig.wait(|hub| hub.rooms["Replace"].players[0].connection.is_none());
        rig.poll(9_999);
        let (mut returned, _) = rig.join("Replace", "host", "Returned", false);
        rig.poll(10_000);
        returned.barrier();
        rig.read(|hub| {
            let room = &hub.rooms["Replace"];
            assert_eq!(room.players.len(), 1);
            assert_eq!(room.players[0].disconnected, None);
            assert_eq!(room.players[0].name, "Returned");
            assert_eq!(room.players[0].denies, before.1);
        });
    })
    .await;
}

#[derive(Debug, PartialEq)]
struct PersonObservation {
    token: Vec<u16>,
    name: String,
    connection: Option<u64>,
    ai: bool,
    team: usize,
    faction: usize,
    level: &'static str,
    disconnected: Option<u64>,
    denies: Vec<u64>,
    rtt: Option<f64>,
    last_match: Option<Value>,
}

impl PersonObservation {
    fn read(person: &Person) -> Self {
        Self {
            token: person.token.clone(),
            name: person.name.clone(),
            connection: person.connection.as_ref().map(|connection| connection.id),
            ai: person.ai,
            team: person.team,
            faction: person.faction,
            level: person.level,
            disconnected: person.disconnected,
            denies: person.denies.clone(),
            rtt: person.rtt,
            last_match: person.last_match.clone(),
        }
    }
}

fn room_people(room: &Room) -> (Vec<PersonObservation>, Vec<PersonObservation>, Option<u64>) {
    (
        room.players.iter().map(PersonObservation::read).collect(),
        room.spectators
            .iter()
            .map(PersonObservation::read)
            .collect(),
        room.empty_since,
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn net20_net29_websocket_text_forms_require_strings_and_keep_all_recipients() {
    // Source server.js:588-589,595,638 and test.js:5135-5147.
    sockets(|rig| {
        for (form, invalid) in [
            None,
            Some(Value::Null),
            Some(json!(false)),
            Some(json!(7)),
            Some(json!([])),
            Some(json!({})),
            Some(json!({"toString":null,"valueOf":null})),
        ]
        .into_iter()
        .enumerate()
        {
            for (field_index, field) in ["token", "name"].into_iter().enumerate() {
                let code = format!("Forms{form}{field_index}");
                let (mut host, _) = rig.join(&code, "host", "Host", false);
                let mut candidate = Client::open(rig.address, &code);
                candidate.send(json!({"t":"addAi"}));
                let mut hello = json!({"t":"hello","token":"guest","name":"Guest"});
                if let Some(value) = &invalid {
                    hello[field] = value.clone();
                } else {
                    hello.as_object_mut().unwrap().remove(field);
                }
                candidate.send(hello);
                candidate.send(json!({"t":"hello","token":"guest","name":"Guest"}));
                let joined = candidate.receive();
                let players = json!([
                    {"name":"Host","connected":true,"ai":false,"team":0,"faction":0,"level":null},
                    {"name":"Guest","connected":true,"ai":false,"team":1,"faction":1,"level":null}
                ]);
                assert_eq!(joined["players"], players);
                assert_eq!(joined["you"], 1);
                assert!(!joined["spectator"].as_bool().unwrap());
                assert!(candidate.barrier().is_empty());
                let delivered = host.barrier();
                assert_eq!(
                    delivered.len(),
                    1,
                    "invalid hello and pre-hello command emit nothing"
                );
                assert_eq!(delivered[0]["players"], players);
                let (mut watcher, _) = rig.join(&code, "watch", "Watcher", true);
                host.barrier();
                candidate.barrier();
                let before = rig.read(|hub| room_people(&hub.rooms[&code]));
                assert_eq!(before.0.len(), 2);
                assert_eq!(before.1.len(), 1);
                assert_eq!(
                    before.0[1].token,
                    "guest".encode_utf16().collect::<Vec<_>>()
                );
                assert_eq!(before.0[1].name, "Guest");
                assert_eq!((before.0[1].team, before.0[1].faction), (1, 1));
                assert!(before.0[1].connection.is_some());
                for client in [&mut candidate, &mut watcher] {
                    let mut rename = json!({"t":"name"});
                    if let Some(value) = &invalid {
                        rename["name"] = value.clone();
                    }
                    client.send(rename);
                    assert!(client.barrier().is_empty());
                }
                assert!(candidate.barrier().is_empty());
                assert!(watcher.barrier().is_empty());
                assert!(host.barrier().is_empty());
                rig.read(|hub| assert_eq!(room_people(&hub.rooms[&code]), before));
                // A real string remains usable on the same two sending sockets.
                candidate.send(json!({"t":"name","name":"\u{feff}<Live>\u{feff}"}));
                let live_players = json!([
                    {"name":"Host","connected":true,"ai":false,"team":0,"faction":0,"level":null},
                    {"name":"Live","connected":true,"ai":false,"team":1,"faction":1,"level":null}
                ]);
                assert_recipient_rosters(
                    [host.lobby(), candidate.lobby(), watcher.lobby()],
                    live_players.clone(),
                    json!(["Watcher"]),
                    [0, 1, -1],
                );
                watcher.send(json!({"t":"name","name":"\u{feff}<>&\"' \t\u{feff}"}));
                assert_recipient_rosters(
                    [host.lobby(), candidate.lobby(), watcher.lobby()],
                    live_players,
                    json!(["Soldier"]),
                    [0, 1, -1],
                );
                rig.read(|hub| {
                    let mut expected = before;
                    expected.0[1].name = "Live".into();
                    expected.1[0].name = "Soldier".into();
                    assert_eq!(room_people(&hub.rooms[&code]), expected);
                });
            }
        }
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn net21_websocket_saturated_watchers_preserve_roster_until_real_departure() {
    // Source server.js:609-611,704-707. Lobby capacity only, no in-play adoption.
    sockets(|rig| {
        let (mut host, _) = rig.join("WatchBound", "host", "Host", false);
        let mut watchers = Vec::new();
        for index in 0..8 {
            let (mut watcher, lobby) = rig.join(
                "WatchBound",
                &format!("watch{index}"),
                &format!("Watch{index}"),
                true,
            );
            assert_eq!(lobby["players"].as_array().unwrap().len(), 1);
            assert_eq!(lobby["spectators"].as_array().unwrap().len(), index + 1);
            assert_eq!(lobby["you"], -1);
            assert!(watcher.barrier().is_empty());
            watchers.push(watcher);
        }
        host.barrier();
        for watcher in &mut watchers {
            watcher.barrier();
        }
        let before = rig.read(|hub| room_people(&hub.rooms["WatchBound"]));
        assert_eq!(before.0.len(), 1);
        assert_eq!(before.1.len(), 8);
        host.send(json!({"t":"spectate"}));
        assert!(
            host.barrier().is_empty(),
            "a seated player cannot become watcher nine"
        );
        let mut rejected = Client::open(rig.address, "WatchBound");
        rejected.send(json!({"t":"hello","token":"extra","name":"Extra","spectate":true}));
        assert_eq!(rejected.receive(), json!({"t":"full","reason":"seats"}));
        rejected.closed();
        for watcher in &mut watchers {
            assert!(watcher.barrier().is_empty());
        }
        assert!(host.barrier().is_empty());
        rig.read(|hub| assert_eq!(room_people(&hub.rooms["WatchBound"]), before));
        // Actual TCP EOF makes one place available; no synthetic roster edit.
        drop(watchers.pop().unwrap());
        host.until(|m| m["t"] == "lobby" && m["spectators"].as_array().unwrap().len() == 7);
        for watcher in &mut watchers {
            watcher.barrier();
        }
        host.send(json!({"t":"spectate"}));
        let names = json!([
            "Watch0", "Watch1", "Watch2", "Watch3", "Watch4", "Watch5", "Watch6", "Host"
        ]);
        let lobby = host.lobby();
        assert_eq!(lobby["players"], json!([]));
        assert_eq!(lobby["spectators"], names);
        assert_eq!(lobby["you"], -1);
        assert_eq!(lobby["spectator"], true);
        assert_eq!(lobby["amHost"], false);
        for (index, watcher) in watchers.iter_mut().enumerate() {
            let lobby = watcher.lobby();
            assert_eq!(lobby["players"], json!([]));
            assert_eq!(lobby["spectators"], names);
            assert_eq!(lobby["you"], -1);
            assert_eq!(lobby["amHost"], index == 0);
        }
        rig.read(|hub| {
            let mut expected_watchers = before.1;
            expected_watchers.pop();
            expected_watchers.extend(before.0);
            assert_eq!(
                room_people(&hub.rooms["WatchBound"]),
                (vec![], expected_watchers, None)
            );
        });
        host.send(json!({"t":"sit"}));
        let lobby = host.lobby();
        assert_eq!(lobby["you"], 0);
        assert_eq!(lobby["spectator"], false);
        assert_eq!(lobby["amHost"], true);
        assert_eq!(
            lobby["players"],
            json!([
                {"name":"Host","connected":true,"ai":false,"team":0,"faction":0,"level":null}
            ])
        );
        let players = lobby["players"].clone();
        let names = json!([
            "Watch0", "Watch1", "Watch2", "Watch3", "Watch4", "Watch5", "Watch6"
        ]);
        assert_eq!(lobby["spectators"], names);
        for watcher in &mut watchers {
            let view = watcher.lobby();
            assert_eq!(view["players"], players);
            assert_eq!(view["spectators"], names);
            assert_eq!(view["you"], -1);
            assert_eq!(view["spectator"], true);
            assert_eq!(view["host"], 0);
            assert_eq!(view["amHost"], false);
            assert!(watcher.barrier().is_empty());
        }
        assert!(host.barrier().is_empty());
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn net25_websocket_rejoin_cancels_cleanup_while_due_peer_actually_expires() {
    // Source server.js:568-574,596-602,718-733. Two real offline seats distinguish cancellation.
    sockets(|rig| {
        let (mut host, _) = rig.join("CancelDue", "host", "Host", false);
        let (mut returning, _) = rig.join("CancelDue", "return", "Before", false);
        let (expired, _) = rig.join("CancelDue", "expire", "Expires", false);
        host.barrier();
        returning.barrier();
        returning.send(json!({"t":"ping","c":"remember","rtt":37}));
        assert_eq!(returning.receive(), json!({"t":"pong","c":"remember"}));
        host.send(json!({"t":"team","slot":1,"v":4}));
        host.lobby();
        returning.lobby();
        let original = rig.read(|hub| PersonObservation::read(&hub.rooms["CancelDue"].players[1]));
        drop((returning, expired));
        rig.wait(|hub| {
            hub.rooms["CancelDue"].players[1..]
                .iter()
                .all(|p| p.connection.is_none())
        });
        host.barrier();
        rig.read(|hub| {
            let room = &hub.rooms["CancelDue"];
            assert_eq!(room.players[1].disconnected, Some(0));
            assert_eq!(room.players[2].disconnected, Some(0));
        });
        rig.poll(9_951);
        rig.read(|hub| assert_eq!(hub.clock.due_ns(), 10_001_000_000));
        rig.poll(9_999);
        let (mut returned, lobby) = rig.join("CancelDue", "return", "Returned", true);
        assert_eq!(lobby["you"], 1);
        assert_eq!(lobby["spectator"], false);
        let joined_players = json!([
            {"name":"Host","connected":true,"ai":false,"team":0,"faction":0,"level":null},
            {"name":"Returned","connected":true,"ai":false,"team":4,"faction":1,"level":null},
            {"name":"Expires","connected":false,"ai":false,"team":2,"faction":2,"level":null}
        ]);
        for (view, slot) in [(lobby, 1), (host.lobby(), 0)] {
            assert_eq!(view["players"], joined_players);
            assert_eq!(view["spectators"], json!([]));
            assert_eq!(view["you"], slot);
            assert_eq!(view["spectator"], false);
            assert_eq!(view["amHost"], slot == 0);
        }
        assert!(host.barrier().is_empty());
        let expected = rig.read(|hub| {
            let person = PersonObservation::read(&hub.rooms["CancelDue"].players[1]);
            let mut expected = original;
            expected.name = "Returned".into();
            assert_ne!(person.connection, expected.connection);
            expected.connection = person.connection;
            assert_ne!(expected.connection, None);
            assert_eq!(person, expected);
            expected
        });
        rig.poll(10_000);
        rig.read(|hub| {
            assert_eq!(hub.clock.due_ns(), 10_001_000_000);
            assert_eq!(hub.rooms["CancelDue"].players.len(), 3);
        });
        assert!(returned.barrier().is_empty());
        rig.poll(10_001);
        rig.read(|hub| {
            assert_eq!(hub.clock.due_ns(), 10_051_000_000);
            let room = &hub.rooms["CancelDue"];
            assert_eq!(
                room.players.len(),
                2,
                "the due callback must remove the offline control seat"
            );
            assert_eq!(PersonObservation::read(&room.players[1]), expected);
            assert!(room.spectators.is_empty());
        });
        let players = json!([
            {"name":"Host","connected":true,"ai":false,"team":0,"faction":0,"level":null},
            {"name":"Returned","connected":true,"ai":false,"team":4,"faction":1,"level":null}
        ]);
        for lobby in [host.lobby(), returned.lobby()] {
            assert_eq!(lobby["players"], players);
            assert_eq!(lobby["spectators"], json!([]));
        }
        returned.send(json!({"t":"name","name":"StillLive"}));
        let mut renamed = players;
        renamed[1]["name"] = json!("StillLive");
        for (view, slot) in [(host.lobby(), 0), (returned.lobby(), 1)] {
            assert_eq!(view["players"], renamed);
            assert_eq!(view["spectators"], json!([]));
            assert_eq!(view["you"], slot);
            assert_eq!(view["spectator"], false);
            assert_eq!(view["amHost"], slot == 0);
        }
        assert!(host.barrier().is_empty());
        assert!(returned.barrier().is_empty());
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn net27_websocket_stale_parsed_buy_is_silent_before_and_after_due_cleanup() {
    // Source server.js:596-602,620. Refusal-budget control, not live-buy or AI handover proof.
    sockets(|rig| {
        for (case, after_due) in [false, true].into_iter().enumerate() {
            let base = case as u64 * 20_000;
            rig.poll(base);
            let code = format!("LateDue{case}");
            let (mut host, _) = rig.join(&code, "host", "Host", false);
            let (mut old, _) = rig.join(&code, "guest", "Before", false);
            let (expired, _) = rig.join(&code, "expire", "Expires", false);
            old.barrier();
            drop(expired);
            rig.wait(|hub| hub.rooms[&code].players[2].connection.is_none());
            host.barrier();
            old.barrier();
            rig.poll(base + 9_951);
            for command in ["save", "buy", "restart"] {
                old.send(json!({"t":command}));
                let denial = old.receive();
                assert_eq!(denial["t"], "deny");
                assert_eq!(denial["cmd"], command);
            }
            old.send(json!({"t":"ping","c":"remember","rtt":37}));
            assert_eq!(old.receive(), json!({"t":"pong","c":"remember"}));
            let original = rig.read(|hub| {
                let room = &hub.rooms[&code];
                assert_eq!(room.players[2].disconnected, Some(base));
                assert_eq!(hub.clock.due_ns(), (base + 10_001) * 1_000_000);
                let person = PersonObservation::read(&room.players[1]);
                assert_eq!(person.denies, vec![base + 9_951; 3]);
                person
            });
            let old_id = original.connection.unwrap();
            let HeldFrame { entered, release, dispatched } = rig.hold_next_frame(old_id, "buy");
            old.send(json!({"t":"buy","unit":"rifle"}));
            rig.wait_signal(entered);
            rig.poll(base + 9_999);
            let (mut current, lobby) = rig.join(&code, "guest", "Current", true);
            assert_eq!(lobby["you"], 1);
            assert_eq!(lobby["spectator"], false);
            host.barrier();
            assert!(current.barrier().is_empty());
            let expected = rig.read(|hub| {
                let observed = PersonObservation::read(&hub.rooms[&code].players[1]);
                let mut expected = original;
                expected.name = "Current".into();
                assert_ne!(observed.connection, expected.connection);
                assert!(observed.connection.is_some());
                expected.connection = observed.connection;
                assert_eq!(observed, expected);
                expected
            });
            if after_due {
                rig.poll(base + 10_001);
                for (view, slot) in [(host.lobby(), 0), (current.lobby(), 1)] {
                    assert_eq!(view["players"], json!([
                        {"name":"Host","connected":true,"ai":false,"team":0,"faction":0,"level":null},
                        {"name":"Current","connected":true,"ai":false,"team":1,"faction":1,"level":null}
                    ]));
                    assert_eq!(view["spectators"], json!([]));
                    assert_eq!(view["you"], slot);
                    assert_eq!(view["spectator"], false);
                    assert_eq!(view["amHost"], slot == 0);
                }
            }
            let before = rig.read(|hub| {
                let room = &hub.rooms[&code];
                assert_eq!(room.players.len(), if after_due { 2 } else { 3 });
                assert_eq!(PersonObservation::read(&room.players[1]), expected);
                (room_people(room), room.lobby(expected.connection.unwrap(), &hub.maps, &hub.public_url))
            });
            release.send(()).unwrap();
            rig.wait_signal(dispatched);
            assert_eq!(old.receive(), json!({"t":"replaced"}));
            old.closed();
            assert!(current.barrier().is_empty(), "stale parsed input emits no current response");
            assert!(host.barrier().is_empty(), "stale parsed input broadcasts nothing");
            rig.read(|hub| {
                let room = &hub.rooms[&code];
                assert_eq!(room_people(room), before.0);
                assert_eq!(room.lobby(expected.connection.unwrap(), &hub.maps, &hub.public_url), before.1);
            });
            if !after_due {
                rig.poll(base + 10_001);
                for (view, slot) in [(host.lobby(), 0), (current.lobby(), 1)] {
                    assert_eq!(view["players"], json!([
                        {"name":"Host","connected":true,"ai":false,"team":0,"faction":0,"level":null},
                        {"name":"Current","connected":true,"ai":false,"team":1,"faction":1,"level":null}
                    ]));
                    assert_eq!(view["spectators"], json!([]));
                    assert_eq!(view["you"], slot);
                    assert_eq!(view["spectator"], false);
                    assert_eq!(view["amHost"], slot == 0);
                }
            }
            rig.read(|hub| {
                assert_eq!(hub.clock.due_ns(), (base + 10_051) * 1_000_000);
                let room = &hub.rooms[&code];
                assert_eq!(room.players.len(), 2, "an actual due callback removes the offline control");
                assert_eq!(PersonObservation::read(&room.players[1]), expected);
            });
            current.send(json!({"t":"buy","unit":"rifle"}));
            assert_eq!(current.receive(), json!({"t":"deny","cmd":"buy","reason":"blocked"}));
            current.send(json!({"t":"save"}));
            assert!(current.barrier().is_empty(), "only the current fourth denial consumes the budget");
            assert!(host.barrier().is_empty());
            rig.read(|hub| {
                let mut expected = expected;
                expected.denies.push(base + 10_001);
                assert_eq!(PersonObservation::read(&hub.rooms[&code].players[1]), expected);
            });
        }
    })
    .await;
}
