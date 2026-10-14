use super::*;
use std::io::{Read, Write};

struct Client(std::net::TcpStream);
impl Client {
    fn open(address: std::net::SocketAddr, token: &str) -> Self {
        let mut socket = std::net::TcpStream::connect(address).unwrap();
        socket
            .set_read_timeout(Some(std::time::Duration::from_secs(10)))
            .unwrap();
        socket
            .set_write_timeout(Some(std::time::Duration::from_secs(10)))
            .unwrap();
        write!(socket, "GET /ws?room=ABC HTTP/1.1\r\nHost: {address}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n").unwrap();
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
        let mut client = Self(socket);
        client.send(json!({"t":"hello","token":token,"name":token}));
        client.until("lobby");
        client
    }
    fn send(&mut self, message: Value) {
        let payload = message.to_string().into_bytes();
        assert!(payload.len() < 126);
        let mask = [7_u8, 11, 17, 23];
        let mut frame = vec![0x81, 0x80 | payload.len() as u8];
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
            assert_eq!(header[1] & 0x80, 0, "server frames are unmasked");
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
                n => n as usize,
            };
            assert!(size < 4 * 1024 * 1024);
            let mut payload = vec![0; size];
            self.0.read_exact(&mut payload).unwrap();
            assert_eq!(header[0] & 0x0f, 1, "expected JSON text frame");
            assert_eq!(header[0] & 0x80, 0x80, "expected complete server frame");
            let value: Value = serde_json::from_slice(&payload).unwrap();
            if value["t"] == kind {
                return value;
            }
        }
        panic!("server did not emit {kind}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn real_socket_maintenance_uses_trusted_runtime_and_public_start_stays_denied() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (stop, stopped) = oneshot::channel();
    let (ready, control) = oneshot::channel();
    let server = tokio::spawn(serve_with_native_control(
        listener,
        Config::default(),
        async move {
            let _ = stopped.await;
        },
        Some(ready),
    ));
    let control = control.await.unwrap();
    let receipt = control.clone();
    let host = tokio::runtime::Handle::current();
    tokio::task::spawn_blocking(move || {
        let mut a = Client::open(address, "a");
        let mut b = Client::open(address, "b");
        let epoch = host
            .block_on(control.attach("ABC", lobby::native_tests::native_match()))
            .unwrap();
        a.send(json!({"t":"start"}));
        assert_eq!(a.until("deny")["cmd"], "start");
        let first = a.until("s");
        let second = b.until("s");
        assert_eq!(first["online"], json!([true, true]));
        assert_eq!(second["online"], json!([true, true]));
        assert_eq!(first["ping"], json!([null, null]));
        assert!(first["units"].is_array());
        let status = host.block_on(control.inspect("ABC")).unwrap();
        assert_eq!(status.epoch, epoch);
        assert!(status.tick > 0);
        assert!(status.fault.is_none());
        // The same-token socket gets a new stream in the unchanged stable slot.
        let mut replacement = Client::open(address, "a");
        let frame = replacement.until("s");
        assert!(frame["units"].is_array());
        assert!(frame.get("all").is_none());
        assert_eq!(
            frame["held"][0].as_u64().unwrap() as usize,
            frame["units"].as_array().unwrap().len()
        );
        // A real socket drop pauses once. Its same-token replacement resumes
        // before maintenance needs to discover the replacement connection.
        drop(replacement);
        assert_eq!(b.until("pause")["paused"], true);
        assert!(host.block_on(control.inspect("ABC")).unwrap().paused);
        let mut reconnected = Client::open(address, "a");
        let resumed = host.block_on(control.inspect("ABC")).unwrap();
        assert!(!resumed.paused);
        assert!(resumed.fault.is_none());
        assert!(reconnected.until("s")["units"].is_array());
    })
    .await
    .unwrap();
    let _ = stop.send(());
    server.await.unwrap().unwrap();
    assert!(receipt.inspect("ABC").await.is_err());
    assert!(
        receipt
            .attach("ABC", lobby::native_tests::native_match())
            .await
            .is_err()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dropped_control_receipt_preserves_existing_serve_lobby() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (stop, stopped) = oneshot::channel();
    let (ready, receiver) = oneshot::channel();
    drop(receiver);
    let server = tokio::spawn(serve_with_native_control(
        listener,
        Config::default(),
        async move {
            let _ = stopped.await;
        },
        Some(ready),
    ));
    tokio::task::spawn_blocking(move || {
        let mut client = Client::open(address, "a");
        client.send(json!({"t":"ping","c":7}));
        assert_eq!(client.until("pong")["c"], 7);
    })
    .await
    .unwrap();
    let _ = stop.send(());
    server.await.unwrap().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn aborted_serve_invalidates_retained_control_on_the_same_hub() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (ready, received) = oneshot::channel();
    let server = tokio::spawn(serve_with_native_control(
        listener,
        Config::default(),
        std::future::pending(),
        Some(ready),
    ));
    let control = received.await.unwrap();
    tokio::task::spawn_blocking(move || {
        let _client = Client::open(address, "a");
    })
    .await
    .unwrap();
    server.abort();
    assert!(server.await.unwrap_err().is_cancelled());
    assert_eq!(control.inspect("ABC").await.unwrap_err(), "server stopped");
    assert_eq!(
        control
            .attach("ABC", lobby::native_tests::native_match())
            .await
            .unwrap_err(),
        "server stopped"
    );
}
