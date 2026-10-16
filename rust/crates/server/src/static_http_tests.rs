use super::*;
use crate::{Config, serve};
use flate2::read::GzDecoder;
use std::{
    fs,
    net::{SocketAddr, TcpStream},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime},
};
use tokio::{net::TcpListener, sync::oneshot, task::JoinHandle};

static NEXT: AtomicU64 = AtomicU64::new(1);

struct Assets(PathBuf);

impl Assets {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "ww2-static-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        let assets = Self(root);
        assets.write("client/public-lobby.html", b"<html>public lobby</html>");
        assets.write("client/index.html", b"<html>game and editor</html>");
        assets
    }

    fn write(&self, path: &str, bytes: &[u8]) {
        let file = self.0.join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, bytes).unwrap();
    }
}

impl Drop for Assets {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct Http {
    status: u16,
    headers: BTreeMap<String, String>,
    body: Vec<u8>,
}

struct TestServer {
    address: SocketAddr,
    shutdown: Option<oneshot::Sender<()>>,
    task: Option<JoinHandle<std::io::Result<()>>>,
}

impl TestServer {
    async fn start(root: Option<PathBuf>) -> Self {
        Self::start_with_shards(root, 1).await
    }

    async fn start_with_shards(root: Option<PathBuf>, shards: usize) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (shutdown, stopped) = oneshot::channel();
        let config = Config {
            static_root: root,
            ..Config::default()
        };
        let shutdown_wait = async {
            let _ = stopped.await;
        };
        let task = tokio::spawn(async move {
            if shards == 1 {
                serve(listener, config, shutdown_wait).await
            } else {
                crate::shards::serve_sharded(listener, config, shutdown_wait, None, shards).await
            }
        });
        Self {
            address,
            shutdown: Some(shutdown),
            task: Some(task),
        }
    }

    async fn get(&self, path: &str, headers: &[(&str, &str)]) -> Http {
        let address = self.address;
        let path = path.to_owned();
        let headers: Vec<_> = headers
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect();
        tokio::task::spawn_blocking(move || {
            let mut socket = TcpStream::connect(address).unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            socket
                .set_write_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            write!(
                socket,
                "GET {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n"
            )
            .unwrap();
            for (key, value) in headers {
                write!(socket, "{key}: {value}\r\n").unwrap();
            }
            socket.write_all(b"\r\n").unwrap();
            let mut bytes = Vec::new();
            socket.read_to_end(&mut bytes).unwrap();
            let split = bytes
                .windows(4)
                .position(|window| window == b"\r\n\r\n")
                .unwrap();
            let head = std::str::from_utf8(&bytes[..split]).unwrap();
            let mut lines = head.split("\r\n");
            let status = lines
                .next()
                .unwrap()
                .split_whitespace()
                .nth(1)
                .unwrap()
                .parse()
                .unwrap();
            let headers: BTreeMap<_, _> = lines
                .map(|line| {
                    let (key, value) = line.split_once(':').unwrap();
                    (key.to_ascii_lowercase(), value.trim().to_owned())
                })
                .collect();
            assert!(!headers.contains_key("transfer-encoding"));
            Http {
                status,
                headers,
                body: bytes[split + 4..].to_vec(),
            }
        })
        .await
        .unwrap()
    }

    async fn stop(mut self) {
        self.shutdown.take().unwrap().send(()).unwrap();
        self.task.take().unwrap().await.unwrap().unwrap();
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
    }
}

#[tokio::test]
async fn intent_net06_static_prefixes_mime_and_confined_files_over_http() {
    let assets = Assets::new();
    for (path, mime) in [
        ("client/module.js", "text/javascript"),
        ("shared/types.json", "application/json"),
        (
            "node_modules/three/build/three.module.js",
            "text/javascript",
        ),
        (
            "node_modules/three/examples/jsm/controls/OrbitControls.js",
            "text/javascript",
        ),
        ("client/style.css", "text/css"),
        ("client/icon.png", "image/png"),
        ("client/photo.jpg", "image/jpeg"),
        ("client/photo.webp", "image/webp"),
        ("client/icon.svg", "image/svg+xml"),
        ("client/audio.mp3", "audio/mpeg"),
        ("client/data.bin", "application/octet-stream"),
    ] {
        assets.write(path, mime.as_bytes());
    }
    assets.write("secret.txt", b"private-root-file");
    assets.write("maps/private.json", br#"{"private":"raw authoring data"}"#);
    assets.write("client-sibling/private.txt", b"sibling-prefix-secret");
    let server = TestServer::start(Some(assets.0.clone())).await;
    for (path, mime) in [
        ("/client/module.js", "text/javascript"),
        ("/shared/types.json", "application/json"),
        ("/vendor/three.module.js", "text/javascript"),
        ("/vendor-jsm/controls/OrbitControls.js", "text/javascript"),
        ("/client/style.css", "text/css"),
        ("/client/icon.png", "image/png"),
        ("/client/photo.jpg", "image/jpeg"),
        ("/client/photo.webp", "image/webp"),
        ("/client/icon.svg", "image/svg+xml"),
        ("/client/audio.mp3", "audio/mpeg"),
        ("/client/data.bin", "application/octet-stream"),
    ] {
        let response = server.get(path, &[]).await;
        assert_eq!(response.status, 200, "{path}");
        assert_eq!(response.headers["content-type"], mime);
        assert_eq!(response.body, mime.as_bytes());
    }
    for path in [
        "/client/",
        "/shared/missing.js",
        "/secret.txt",
        "/node_modules/three/build/three.module.js",
        "/client/../secret.txt",
        "/client/%2e%2e/secret.txt",
        "/client/../client-sibling/private.txt",
        "/client/%2f..%2fsecret.txt",
        "/client/%00.js",
        "/client/../../maps/private.json",
        "/maps/private.json",
    ] {
        let response = server.get(path, &[]).await;
        assert_eq!(response.status, 404, "{path}");
        assert!(!String::from_utf8_lossy(&response.body).contains("private"));
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(
            assets.0.join("secret.txt"),
            assets.0.join("client/leak.txt"),
        )
        .unwrap();
        std::os::unix::fs::symlink(
            assets.0.join("client-sibling"),
            assets.0.join("client/leak-dir"),
        )
        .unwrap();
        for path in ["/client/leak.txt", "/client/leak-dir/private.txt"] {
            assert_eq!(server.get(path, &[]).await.status, 404, "{path}");
        }
        fs::remove_dir_all(assets.0.join("shared")).unwrap();
        std::os::unix::fs::symlink(assets.0.join("maps"), assets.0.join("shared")).unwrap();
        assert_eq!(server.get("/shared/private.json", &[]).await.status, 404);
    }
    server.stop().await;
}

#[tokio::test]
async fn intent_net07_static_etag_conditional_gzip_and_file_version_invalidation() {
    let assets = Assets::new();
    assets.write("client/module.js", b"export const version = 1;");
    assets.write("client/icon.png", b"not-text-png");
    assets.write("client/icon.svg", b"<svg>text</svg>");
    assets.write("shared/data.json", b"{\"text\":true}");
    File::options()
        .write(true)
        .open(assets.0.join("client/module.js"))
        .unwrap()
        .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(1))
        .unwrap();
    let server = TestServer::start(Some(assets.0.clone())).await;
    let plain = server.get("/client/module.js", &[]).await;
    assert_eq!(plain.status, 200);
    assert_eq!(plain.headers["cache-control"], "no-cache");
    assert_eq!(plain.headers["vary"], "accept-encoding");
    assert!(!plain.headers.contains_key("content-encoding"));
    let expected = "\"p-rs\"".to_owned();
    assert_eq!(plain.headers["etag"], expected);
    let conditional = server
        .get(
            "/client/module.js",
            &[("If-None-Match", &expected), ("Accept-Encoding", "gzip")],
        )
        .await;
    assert_eq!(conditional.status, 304);
    assert!(conditional.body.is_empty());
    assert_eq!(conditional.headers["etag"], expected);
    assert_eq!(conditional.headers["cache-control"], "no-cache");
    assert_eq!(conditional.headers["vary"], "accept-encoding");
    assert!(!conditional.headers.contains_key("content-encoding"));
    for path in ["/client/module.js", "/client/icon.svg", "/shared/data.json"] {
        let original = server.get(path, &[]).await;
        let packed = server.get(path, &[("Accept-Encoding", "br, gzip")]).await;
        assert_eq!(packed.status, 200);
        assert_eq!(packed.headers["content-encoding"], "gzip");
        assert_eq!(packed.headers["vary"], "accept-encoding");
        assert_eq!(packed.headers["etag"], original.headers["etag"]);
        let mut decoded = Vec::new();
        GzDecoder::new(packed.body.as_slice())
            .read_to_end(&mut decoded)
            .unwrap();
        assert_eq!(decoded, original.body);
        let cached = server.get(path, &[("Accept-Encoding", "gzip")]).await;
        assert_eq!(cached.body, packed.body);
    }
    let old_packed = server
        .get("/client/module.js", &[("Accept-Encoding", "gzip")])
        .await;
    // Equal size and mtime identify the same cache version, even after a write.
    assets.write("client/module.js", b"export const version = 9;");
    File::options()
        .write(true)
        .open(assets.0.join("client/module.js"))
        .unwrap()
        .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(1))
        .unwrap();
    let reused = server
        .get("/client/module.js", &[("Accept-Encoding", "gzip")])
        .await;
    assert_eq!(reused.headers["etag"], expected);
    assert_eq!(reused.body, old_packed.body);
    assert_eq!(
        server.get("/client/module.js", &[]).await.body,
        b"export const version = 9;"
    );
    assets.write("client/module.js", b"export const version = 222;");
    let changed = server
        .get(
            "/client/module.js",
            &[("Accept-Encoding", "gzip"), ("If-None-Match", &expected)],
        )
        .await;
    assert_eq!(changed.status, 200);
    assert_ne!(changed.headers["etag"], expected);
    assert_ne!(changed.body, old_packed.body);
    let mut decoded = Vec::new();
    GzDecoder::new(changed.body.as_slice())
        .read_to_end(&mut decoded)
        .unwrap();
    assert_eq!(decoded, b"export const version = 222;");
    let file = File::options()
        .write(true)
        .open(assets.0.join("client/module.js"))
        .unwrap();
    file.set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(2))
        .unwrap();
    let retimed = server
        .get(
            "/client/module.js",
            &[("If-None-Match", &changed.headers["etag"])],
        )
        .await;
    assert_eq!(retimed.status, 200);
    assert_ne!(retimed.headers["etag"], changed.headers["etag"]);
    let binary = server
        .get("/client/icon.png", &[("Accept-Encoding", "gzip")])
        .await;
    assert_eq!(binary.status, 200);
    assert_eq!(binary.body, b"not-text-png");
    assert!(!binary.headers.contains_key("vary"));
    assert!(!binary.headers.contains_key("content-encoding"));
    let binary_cached = server
        .get(
            "/client/icon.png",
            &[("If-None-Match", &binary.headers["etag"])],
        )
        .await;
    assert_eq!(binary_cached.status, 304);
    assert!(binary_cached.body.is_empty());
    assert!(!binary_cached.headers.contains_key("vary"));
    server.stop().await;
}

#[tokio::test]
async fn intent_net08_browser_page_routes_and_explicit_static_root_opt_in() {
    let assets = Assets::new();
    let server = TestServer::start(Some(assets.0.clone())).await;
    for path in ["/", "/?other=value"] {
        let response = server.get(path, &[]).await;
        assert_eq!(response.status, 200);
        assert_eq!(response.headers["content-type"], "text/html");
        assert_eq!(response.body, b"<html>public lobby</html>");
    }
    for path in ["/play", "/?edit", "/?edit=1", "/?other=value&%65dit"] {
        let response = server.get(path, &[]).await;
        assert_eq!(response.status, 200, "{path}");
        assert_eq!(response.headers["content-type"], "text/html");
        assert_eq!(response.body, b"<html>game and editor</html>");
    }
    server.stop().await;
    let disabled = TestServer::start(None).await;
    for path in ["/", "/play", "/?edit", "/client/index.html"] {
        assert_eq!(disabled.get(path, &[]).await.status, 404);
    }
    disabled.stop().await;
}

#[tokio::test]
async fn static_assets_survive_sharded_front_with_maps_and_confinement() {
    let assets = Assets::new();
    assets.write("shared/game.js", b"export const native = true;");
    let server = TestServer::start_with_shards(Some(assets.0.clone()), 2).await;
    let page = server.get("/play", &[]).await;
    assert_eq!(page.status, 200);
    assert_eq!(page.body, b"<html>game and editor</html>");
    let script = server
        .get("/shared/game.js", &[("Accept-Encoding", "gzip")])
        .await;
    assert_eq!(script.status, 200);
    assert_eq!(script.headers["content-encoding"], "gzip");
    let mut decoded = Vec::new();
    GzDecoder::new(&script.body[..])
        .read_to_end(&mut decoded)
        .unwrap();
    assert_eq!(decoded, b"export const native = true;");
    assert_eq!(
        server
            .get("/shared/%2e%2e/client/index.html", &[])
            .await
            .status,
        404
    );
    assert_eq!(server.get("/maps", &[]).await.status, 200);
    assert_eq!(server.get("/maps/no-map.json", &[]).await.status, 404);
    server.stop().await;
}
