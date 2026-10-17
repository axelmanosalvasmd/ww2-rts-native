//! Exercise the ordinary HTTP router over a real port-zero listener.
use super::*;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};

fn request(address: SocketAddr, method: &str, path: &str, extra: &str) -> (u16, String, Vec<u8>) {
    let mut socket = TcpStream::connect(address).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    write!(
        socket,
        "{method} {path} HTTP/1.0\r\nHost: {address}\r\nConnection: close\r\n{extra}\r\n"
    )
    .unwrap();
    let mut response = Vec::new();
    socket.read_to_end(&mut response).unwrap();
    let split = response
        .windows(4)
        .position(|bytes| bytes == b"\r\n\r\n")
        .unwrap();
    let headers = String::from_utf8(response[..split].to_vec()).unwrap();
    let status = headers.split_whitespace().nth(1).unwrap().parse().unwrap();
    (
        status,
        headers.to_ascii_lowercase(),
        response[split + 4..].to_vec(),
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn port_zero_directory_and_public_map_requests_use_the_real_router() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    assert_ne!(address.port(), 0);
    let (stop, stopped) = oneshot::channel();
    let serving = tokio::spawn(serve(listener, Config::default(), async move {
        let _ = stopped.await;
    }));
    let checked = tokio::task::spawn_blocking(move || {
        let (status, headers, body) = request(address, "GET", "/api/rooms", "");
        assert_eq!(status, 200);
        assert!(headers.contains("cache-control: no-store"));
        assert_eq!(
            serde_json::from_slice::<Value>(&body).unwrap(),
            json!({"rooms":[]})
        );
        for name in ["default", "default.json"] {
            let (status, headers, body) = request(address, "GET", &format!("/maps/{name}"), "");
            assert_eq!(status, 200);
            assert!(headers.contains("content-type: application/json"));
            let map: Value = serde_json::from_slice(&body).unwrap();
            assert!(map.get("scenario").is_none() && map.get("triggers").is_none());
            assert!(
                map["layers"]["mines"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|row| row.as_str().unwrap().bytes().all(|byte| byte == b'.'))
            );
            assert_eq!(
                request(
                    address,
                    "GET",
                    &format!("/maps/{name}"),
                    "x-edit-password: wrong\r\n"
                )
                .0,
                403
            );
            for credential in ["", "x-edit-password: wrong\r\n"] {
                let (status, headers, body) = request(
                    address,
                    "POST",
                    &format!("/maps/{name}"),
                    &format!("Content-Length: 0\r\n{credential}"),
                );
                assert_eq!(status, 403);
                assert!(headers.contains("content-type: application/json"));
                assert_eq!(
                    serde_json::from_slice::<Value>(&body).unwrap(),
                    json!({"error":"wrong password"})
                );
            }
            let (status, _, body) = request(address, "PUT", &format!("/maps/{name}"), "");
            assert_eq!(status, 405);
            assert_eq!(body, b"method not allowed");
        }
        assert_eq!(request(address, "GET", "/maps/not-present", "").0, 404);
        assert_eq!(request(address, "GET", "/maps/INVALID", "").0, 400);
        // Browser assets remain outside this native server.
        for path in ["/", "/play", "/?edit", "/client/main.js"] {
            assert_eq!(request(address, "GET", path, "").0, 404);
        }
    })
    .await;
    let _ = stop.send(());
    serving.await.unwrap().unwrap();
    checked.unwrap();
}
