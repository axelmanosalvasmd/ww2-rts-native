//! Replay one pinned Source session-admission trace through the actual TCP router.
use super::*;
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
};

enum Frame {
    Text(Value),
    Close(Value),
}

struct Client(TcpStream);
impl Client {
    fn connect(address: SocketAddr, path: &str) -> Self {
        let mut socket = TcpStream::connect(address).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        socket
            .set_write_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        write!(socket, "GET {path} HTTP/1.1\r\nHost: {address}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n").unwrap();
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

    fn fragment(&mut self, text: &[u8], first: bool, last: bool) {
        let mask = [7_u8, 11, 17, 23];
        let mut frame = vec![(if last { 0x80 } else { 0 }) | u8::from(first)];
        if text.len() < 126 {
            frame.push(0x80 | text.len() as u8);
        } else {
            frame.push(0x80 | 126);
            frame.extend(u16::try_from(text.len()).unwrap().to_be_bytes());
        }
        frame.extend(mask);
        frame.extend(text.iter().enumerate().map(|(i, b)| b ^ mask[i % 4]));
        self.0.write_all(&frame).unwrap();
    }

    fn text(&mut self, text: &str) {
        self.fragment(text.as_bytes(), true, true);
    }

    fn send(&mut self, message: Value) {
        self.text(&message.to_string());
    }

    fn frame(&mut self) -> Frame {
        let mut header = [0; 2];
        self.0.read_exact(&mut header).unwrap();
        assert_eq!(header[1] & 0x80, 0, "unmasked server frame");
        assert_eq!(header[0] & 0x80, 0x80, "complete server frame");
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
            n => usize::from(n),
        };
        assert!(length < 1024 * 1024);
        let mut bytes = vec![0; length];
        self.0.read_exact(&mut bytes).unwrap();
        match header[0] & 15 {
            1 => Frame::Text(serde_json::from_slice(&bytes).unwrap()),
            8 => {
                assert!(bytes.len() >= 2, "close has its actual status code");
                Frame::Close(json!({
                    "code":u16::from_be_bytes([bytes[0],bytes[1]]),
                    "reason":String::from_utf8(bytes[2..].to_vec()).unwrap()
                }))
            }
            opcode => panic!("unexpected websocket opcode {opcode}"),
        }
    }

    fn receive(&mut self) -> Value {
        match self.frame() {
            Frame::Text(value) => value,
            Frame::Close(value) => panic!("expected a message, received close {value}"),
        }
    }

    fn close_frame(&mut self) -> Value {
        match self.frame() {
            Frame::Close(value) => value,
            Frame::Text(value) => panic!("expected close, received message {value}"),
        }
    }

    fn join(address: SocketAddr, code: &str, token: &str) -> (Self, Value) {
        let mut client = Self::connect(address, &format!("/ws?room={code}"));
        client.send(json!({"t":"hello","token":token,"name":token}));
        let initial = lobby(client.receive());
        (client, initial)
    }
}

fn lobby(message: Value) -> Value {
    assert_eq!(message["t"], "lobby");
    let players: Vec<_> = message["players"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| json!({"name":p["name"],"connected":p["connected"],"ai":p["ai"],"team":p["team"],"faction":p["faction"],"level":p["level"]}))
        .collect();
    json!({"code":message["code"],"state":message["state"],"mode":message["mode"],"you":message["you"],"players":players})
}

fn get_maps(address: SocketAddr) -> Value {
    let mut stream = TcpStream::connect(address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    write!(
        stream,
        "GET /maps HTTP/1.0\r\nHost: {address}\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    let mut response = Vec::new();
    stream.read_to_end(&mut response).unwrap();
    let split = response.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
    assert!(response.starts_with(b"HTTP/1.0 200") || response.starts_with(b"HTTP/1.1 200"));
    serde_json::from_slice(&response[split + 4..]).unwrap()
}

fn replay(address: SocketAddr, fixture: Value) {
    assert_eq!(
        fixture["reference"],
        "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
    );
    assert_eq!(
        fixture["rows"],
        json!(["NET-02", "NET-03", "NET-05", "NET-10", "NET-22"])
    );
    for case in fixture["queries"].as_array().unwrap() {
        let id = case["id"].as_str().unwrap();
        eprintln!("NET-02 {id}: {}", case["path"]);
        let mut client = Client::connect(address, case["path"].as_str().unwrap());
        if case["valid"] == true {
            client.send(json!({"t":"hello","token":id,"name":id}));
            assert_eq!(lobby(client.receive()), case["lobby"], "NET-02 {id}");
        } else {
            assert_eq!(client.close_frame(), case["close"], "NET-02 {id}");
        }
    }
    for (index, case) in fixture["payloads"].as_array().unwrap().iter().enumerate() {
        let id = case["id"].as_str().unwrap();
        let bytes = usize::try_from(case["bytes"].as_u64().unwrap()).unwrap();
        let (mut client, _) = Client::join(address, &format!("Size{index}"), id);
        let mut ping = json!({"t":"ping","c":id,"pad":""});
        ping["pad"] = Value::String("x".repeat(bytes - ping.to_string().len()));
        let text = ping.to_string();
        assert_eq!(text.len(), bytes);
        let parts = case["parts"].as_array().unwrap();
        let mut offset = 0;
        for (i, size) in parts.iter().enumerate() {
            let size = usize::try_from(size.as_u64().unwrap()).unwrap();
            client.fragment(
                &text.as_bytes()[offset..offset + size],
                i == 0,
                i + 1 == parts.len(),
            );
            offset += size;
        }
        assert_eq!(offset, bytes);
        if case.get("pong").is_some() {
            assert_eq!(
                client.receive(),
                case["pong"],
                "NET-03 {id}: boundary message processed"
            );
        } else {
            let actual = client.close_frame();
            eprintln!(
                "NET-03 {id}: Source close {}; native close {actual}",
                case["close"]
            );
            assert_eq!(actual["code"], case["close"]["code"], "NET-03 {id}");
        }
    }
    let malformed = &fixture["malformed"];
    let mut client = Client::connect(address, "/ws?room=Malformed");
    for text in malformed["invalid"].as_array().unwrap() {
        client.text(text.as_str().unwrap());
    }
    client.send(json!({"t":"hello","token":"malformed","name":"malformed"}));
    assert_eq!(lobby(client.receive()), malformed["lobby"]);
    for text in malformed["invalid"].as_array().unwrap() {
        client.text(text.as_str().unwrap());
    }
    client.send(json!({"t":"ping","c":"after-invalid"}));
    assert_eq!(client.receive(), malformed["pong"]);
    client.send(json!({"t":"ping","c":"still-alive"}));
    assert_eq!(client.receive(), malformed["barrier"]);
    assert_eq!(
        get_maps(address),
        fixture["maps"],
        "NET-10 complete sorted file list"
    );

    let expected = &fixture["seats"];
    let mut seats = Vec::new();
    for (i, expected) in expected["joins"].as_array().unwrap().iter().enumerate() {
        let (client, initial) = Client::join(address, "SeatDefaults", &format!("seat-{i}"));
        seats.push(Some(client));
        let actual = if i == 0 {
            initial
        } else {
            lobby(seats[0].as_mut().unwrap().receive())
        };
        assert_eq!(actual, *expected, "NET-22 new seat {i}");
    }
    drop(seats[2].take());
    let host = seats[0].as_mut().unwrap();
    assert_eq!(lobby(host.receive()), expected["disconnected"]);
    host.send(json!({"t":"kick","slot":2}));
    assert_eq!(lobby(host.receive()), expected["afterKick"]);
    let (_replacement, initial) = Client::join(address, "SeatDefaults", "replacement");
    assert_eq!(initial, expected["replacement"]);
    assert_eq!(lobby(host.receive()), expected["afterReplacement"]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn source_recorded_session_admission_batch_uses_real_tcp() {
    let fixture: Value =
        serde_json::from_str(include_str!("../tests/fixtures/net-session-batch.json")).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (stop, stopped) = oneshot::channel();
    let server = tokio::spawn(serve(listener, Config::default(), async move {
        let _ = stopped.await;
    }));
    let result = tokio::task::spawn_blocking(move || replay(address, fixture)).await;
    let _ = stop.send(());
    server.await.unwrap().unwrap();
    result.unwrap();
}
