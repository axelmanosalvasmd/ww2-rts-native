use super::*;
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
};

#[test]
fn intent_net96_shard_hash_matches_pinned_javascript_utf16_values() {
    // Values from e4e579a server/shards.js::shardOf, run with Node on the Vostro.
    for (code, expected) in [
        ("", [0, 1, 1, 2, 5]),
        ("ABC", [0, 1, 0, 0, 43]),
        ("abc", [0, 1, 2, 5, 11]),
        ("room0", [0, 0, 1, 3, 62]),
        ("room1", [0, 1, 0, 1, 17]),
        ("A1b2C3d4E5f6", [0, 1, 0, 5, 63]),
        ("é", [0, 0, 1, 0, 4]),
        ("𝄞", [0, 1, 2, 5, 7]),
        ("a+b", [0, 1, 1, 2, 37]),
    ] {
        for (count, expected) in [1, 2, 3, 7, 64].into_iter().zip(expected) {
            assert_eq!(shard_of(code, count), expected, "{code:?}, {count}");
        }
    }
    for (input, expected) in [
        (None, 1),
        (Some(""), 1),
        (Some("bad"), 1),
        (Some("0"), 1),
        (Some("-2"), 1),
        (Some("2.9"), 2),
        (Some("64"), 64),
        (Some("65"), 64),
        (Some("Infinity"), 64),
        (Some("NaN"), 1),
        (Some("inf"), 1),
        (Some("0x10"), 16),
        (Some("0b10"), 2),
        (Some("0o10"), 8),
        (Some("+0x10"), 1),
        (Some(" 2 "), 2),
        (Some("\u{0085}2"), 1),
        (Some("\u{feff}2"), 2),
        (Some("\u{1680}2"), 2),
        (Some("\u{2007}2"), 2),
        (Some("\u{2028}2"), 2),
    ] {
        assert_eq!(shard_count(input), expected, "{input:?}");
    }
}

struct Rig {
    address: SocketAddr,
    front: Front,
    lifetime: Lifetime,
    serving: tokio::task::JoinHandle<io::Result<()>>,
    supervisors: JoinSet<()>,
}
impl Rig {
    async fn new() -> Self {
        Self::with_trusted(None).await
    }
    async fn with_trusted(trusted: Option<TrustedRoomConfig>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (stop, stopped) = watch::channel(false);
        let (workers, supervisors) = start_workers(
            Config {
                max_rooms: 1,
                ..Config::default()
            },
            trusted,
            2,
            stopped.clone(),
        );
        for worker in &workers {
            worker.sender().await.unwrap();
        }
        let front = Front {
            workers,
            stop: stopped.clone(),
        };
        let mut shutdown = stopped;
        let serving = tokio::spawn(serve_front(listener, front.clone(), async move {
            let _ = shutdown.changed().await;
        }));
        Self {
            address,
            front,
            lifetime: Lifetime { stop },
            serving,
            supervisors,
        }
    }
    async fn control(&self, index: usize) -> NativeRoomControl {
        let (send, receive) = oneshot::channel();
        self.front.workers[index]
            .sender()
            .await
            .unwrap()
            .send(Work::Control(send))
            .await
            .unwrap();
        receive.await.unwrap()
    }
    async fn shutdown(mut self) {
        drop(self.lifetime);
        self.serving.await.unwrap().unwrap();
        while let Some(result) = self.supervisors.join_next().await {
            result.unwrap();
        }
        for worker in self.front.workers {
            assert!(worker.sender().await.is_none());
        }
    }
}

struct Client(TcpStream);
impl Client {
    fn open(address: SocketAddr, query_code: &str, token: &str, title: Option<&str>) -> Self {
        let mut socket = TcpStream::connect(address).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        socket
            .set_write_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        write!(socket, "GET /ws?room={query_code} HTTP/1.1\r\nHost: {address}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n").unwrap();
        let mut headers = Vec::new();
        while !headers.ends_with(b"\r\n\r\n") {
            let mut byte = [0];
            socket.read_exact(&mut byte).unwrap();
            headers.push(byte[0]);
            assert!(headers.len() < 8192);
        }
        assert!(headers.starts_with(b"HTTP/1.1 101"));
        let mut client = Self(socket);
        let mut hello = json!({"t":"hello", "token":token, "name":token});
        if let Some(title) = title {
            hello["listing"] = json!({"public":true,"title":title});
        }
        client.send(hello);
        client
    }
    fn send(&mut self, value: Value) {
        let payload = value.to_string().into_bytes();
        let mut frame = vec![0x81];
        if payload.len() < 126 {
            frame.push(0x80 | payload.len() as u8);
        } else {
            frame.push(0x80 | 126);
            frame.extend((payload.len() as u16).to_be_bytes());
        }
        let mask = [7, 11, 17, 23];
        frame.extend(mask);
        frame.extend(
            payload
                .iter()
                .enumerate()
                .map(|(i, byte)| byte ^ mask[i % 4]),
        );
        self.0.write_all(&frame).unwrap();
    }
    fn until(&mut self, kind: &str) -> Value {
        for _ in 0..128 {
            let mut header = [0; 2];
            self.0.read_exact(&mut header).unwrap();
            assert_eq!(header[0] & 0x0f, 1);
            assert_eq!(header[1] & 0x80, 0);
            let size = match header[1] & 127 {
                126 => {
                    let mut size = [0; 2];
                    self.0.read_exact(&mut size).unwrap();
                    u16::from_be_bytes(size) as usize
                }
                127 => {
                    let mut size = [0; 8];
                    self.0.read_exact(&mut size).unwrap();
                    usize::try_from(u64::from_be_bytes(size)).unwrap()
                }
                size => size as usize,
            };
            assert!(size < 4 * 1024 * 1024);
            let mut payload = vec![0; size];
            self.0.read_exact(&mut payload).unwrap();
            let value: Value = serde_json::from_slice(&payload).unwrap();
            if value["t"] == kind {
                return value;
            }
        }
        panic!("missing {kind}");
    }
}

fn http(address: SocketAddr, path: &str, upgrade: bool) -> (String, Value) {
    let mut socket = TcpStream::connect(address).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let upgrade = if upgrade {
        "Upgrade: websocket\r\n"
    } else {
        ""
    };
    write!(
        socket,
        "GET {path} HTTP/1.0\r\nHost: {address}\r\nConnection: close\r\n{upgrade}\r\n"
    )
    .unwrap();
    let mut response = Vec::new();
    socket.read_to_end(&mut response).unwrap();
    let split = response
        .windows(4)
        .position(|bytes| bytes == b"\r\n\r\n")
        .unwrap();
    (
        String::from_utf8(response[..split].to_vec()).unwrap(),
        serde_json::from_slice(&response[split + 4..]).unwrap_or(Value::Null),
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn intent_net96_front_routes_lists_and_runs_native_matches_in_separate_workers() {
    let rig = Rig::new().await;
    let address = rig.address;
    // These room codes are on different pinned FNV shards. max_rooms=1 applies per worker.
    assert_eq!(shard_of("room0", 2), 0);
    assert_eq!(shard_of("room1", 2), 1);
    let (mut host, mut friend, mut other) = tokio::task::spawn_blocking(move || {
        let mut host = Client::open(address, "%72oom0", "host", Some("First"));
        assert_eq!(host.until("lobby")["players"].as_array().unwrap().len(), 1);
        let mut friend = Client::open(address, "room0", "friend", None);
        assert_eq!(
            friend.until("lobby")["players"].as_array().unwrap().len(),
            2
        );
        let mut other = Client::open(address, "room1", "other", Some("Second"));
        assert_eq!(other.until("lobby")["players"].as_array().unwrap().len(), 1);
        let (headers, listed) = http(address, "/api/rooms", false);
        assert!(headers.starts_with("HTTP/1.0 200"));
        assert!(
            headers
                .to_ascii_lowercase()
                .contains("cache-control: no-store")
        );
        let titles: Vec<_> = listed["rooms"]
            .as_array()
            .unwrap()
            .iter()
            .map(|room| room["title"].as_str().unwrap())
            .collect();
        assert_eq!(titles, ["First", "Second"]);
        host.send(json!({"t":"start"}));
        let denied = host.until("deny");
        assert_eq!(denied["cmd"], "start");
        assert_eq!(denied["reason"], "scenario");
        let (headers, map) = http(address, "/maps/default", false);
        assert!(headers.starts_with("HTTP/1.0 200"));
        assert!(map.get("scenario").is_none());
        assert!(!http(address, "/other", true).0.contains("101"));
        (host, friend, other)
    })
    .await
    .unwrap();
    let control = rig.control(0).await;
    control
        .attach("room0", lobby::native_tests::native_match())
        .await
        .unwrap();
    tokio::task::spawn_blocking(move || {
        assert!(host.until("s")["units"].is_array());
        assert!(friend.until("s")["units"].is_array());
        other.send(json!({"t":"ping", "c":42}));
        assert_eq!(other.until("pong")["c"], 42);
    })
    .await
    .unwrap();
    assert!(control.inspect("room0").await.unwrap().fault.is_none());
    rig.shutdown().await;
    assert_eq!(
        control.inspect("room0").await.unwrap_err(),
        "server stopped"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn intent_net96_crash_restarts_only_failed_worker_and_front_shutdown_stops_workers() {
    let rig = Rig::new().await;
    let address = rig.address;
    let (mut lost, mut healthy) = tokio::task::spawn_blocking(move || {
        let mut lost = Client::open(address, "room0", "lost", Some("Lost"));
        lost.until("lobby");
        let mut healthy = Client::open(address, "room1", "healthy", Some("Healthy"));
        healthy.until("lobby");
        (lost, healthy)
    })
    .await
    .unwrap();
    let old = rig.control(0).await;
    let healthy_control = rig.control(1).await;
    let mut ready = rig.front.workers[0].ready.clone();
    ready.borrow_and_update();
    rig.front.workers[0]
        .sender()
        .await
        .unwrap()
        .send(Work::Crash)
        .await
        .unwrap();
    tokio::time::timeout(
        Duration::from_secs(10),
        ready.wait_for(|sender| sender.is_none()),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(old.inspect("room0").await.unwrap_err(), "server stopped");
    healthy = tokio::task::spawn_blocking(move || {
        healthy.send(json!({"t":"ping", "c":73}));
        assert_eq!(healthy.until("pong")["c"], 73);
        let mut byte = [0];
        match lost.0.read(&mut byte) {
            Ok(0) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::ConnectionReset | io::ErrorKind::ConnectionAborted
                ) => {}
            result => panic!("crashed worker kept socket open: {result:?}"),
        }
        healthy
    })
    .await
    .unwrap();
    tokio::time::timeout(
        Duration::from_secs(10),
        ready.wait_for(|sender| sender.is_some()),
    )
    .await
    .unwrap()
    .unwrap();
    let replacement = rig.control(0).await;
    assert!(!Arc::ptr_eq(&old.server.hub, &replacement.server.hub));
    let still_healthy = rig.control(1).await;
    assert!(Arc::ptr_eq(
        &healthy_control.server.hub,
        &still_healthy.server.hub
    ));
    tokio::task::spawn_blocking(move || {
        let mut fresh = Client::open(address, "room0", "fresh", Some("Fresh"));
        assert_eq!(fresh.until("lobby")["players"].as_array().unwrap().len(), 1);
        healthy.send(json!({"t":"ping", "c":74}));
        assert_eq!(healthy.until("pong")["c"], 74);
        let (_, listed) = http(address, "/api/rooms", false);
        let titles: Vec<_> = listed["rooms"]
            .as_array()
            .unwrap()
            .iter()
            .map(|room| room["title"].as_str().unwrap())
            .collect();
        assert_eq!(titles, ["Fresh", "Healthy"]);
    })
    .await
    .unwrap();
    rig.shutdown().await;
    assert_eq!(
        replacement.inspect("room0").await.unwrap_err(),
        "server stopped"
    );
    assert_eq!(
        healthy_control.inspect("room1").await.unwrap_err(),
        "server stopped"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn intent_net96_trusted_front_preserves_private_code_bound_ingress_on_every_worker() {
    let rig = Rig::with_trusted(Some(TrustedRoomConfig::new("room0", 0x3c1055).unwrap())).await;
    let address = rig.address;
    let (mut host, mut friend) = tokio::task::spawn_blocking(move || {
        let mut off_shard = Client::open(address, "room1", "off", None);
        let deny = off_shard.until("deny");
        assert_eq!(deny["cmd"], "join");
        let mut host = Client::open(address, "room0", "host", None);
        host.until("lobby");
        let mut friend = Client::open(address, "room0", "friend", None);
        assert_eq!(
            friend.until("lobby")["players"].as_array().unwrap().len(),
            2
        );
        host.send(json!({"t":"start"}));
        assert!(host.until("s")["units"].is_array());
        assert!(friend.until("s")["units"].is_array());
        let (_, directory) = http(address, "/api/rooms", false);
        assert_eq!(directory["rooms"], json!([]));
        (host, friend)
    })
    .await
    .unwrap();
    let control = rig.control(0).await;
    assert!(control.inspect("room0").await.unwrap().fault.is_none());
    tokio::task::spawn_blocking(move || {
        host.send(json!({"t":"ping", "c":91}));
        assert_eq!(host.until("pong")["c"], 91);
        friend.send(json!({"t":"ping", "c":92}));
        assert_eq!(friend.until("pong")["c"], 92);
    })
    .await
    .unwrap();
    rig.shutdown().await;
}
