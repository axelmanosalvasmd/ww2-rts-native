//! Lobby transport and trusted native room attachment. Public match start remains unavailable.
pub mod authoritative_maps;
mod commander_ingress;
mod diagnostics;
mod editor_errors;
mod lobby;
mod map_editor;
mod maps;
pub use map_editor::MapEditor;
mod horde_records;
pub use horde_records::RecordStore as HordeRecords;
mod native_startup;
mod saves;
mod trusted_room;
pub use trusted_room::TrustedRoomConfig;
pub mod room_runtime;
pub mod shards;
pub mod snapshot_backpressure;
pub mod snapshot_delta;
pub mod snapshot_lists;
pub mod tick_timing;

use axum::{
    Json, Router,
    extract::{
        RawQuery, State, WebSocketUpgrade,
        ws::{CloseFrame, Message, WebSocket},
    },
    http::header,
    routing::{any, get},
};
use lobby::{Connection, Hub, Output};
use serde_json::{Value, json};
use std::{
    future::Future,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Instant,
};
use tokio::{
    net::TcpListener,
    sync::{Mutex, mpsc, oneshot, watch},
    time::{Duration, MissedTickBehavior},
};
use ww2_core::match_runtime::Match;

const MAX_MESSAGE: usize = 16 * 1024;

/// Host-only configuration. Identity remains the client's room token in protocol v0.
#[derive(Clone, Debug)]
pub struct Config {
    pub max_rooms: usize,
    pub public_url: String,
    /// Opt-in host JSONL output. No diagnostic fields enter recipient messages.
    pub diagnostic_dir: Option<std::path::PathBuf>,
    pub tick_log: bool,
    /// Private match files, shared by game workers.
    pub saves_dir: std::path::PathBuf,
    /// Optional authenticated authored-map editor, shared with workers.
    pub map_editor: Option<MapEditor>,
    /// Best Horde runs, shared with workers. Hosting admission remains separate.
    pub horde_records: HordeRecords,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            max_rooms: 32,
            public_url: String::new(),
            diagnostic_dir: None,
            tick_log: false,
            saves_dir: "saves".into(),
            map_editor: None,
            horde_records: HordeRecords::new("horde-records.json"),
        }
    }
}
struct ServeLifetime(Arc<AtomicBool>);
impl Drop for ServeLifetime {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

#[derive(Clone)]
struct Server {
    hub: Arc<Mutex<Hub>>,
    next: Arc<AtomicU64>,
    clock: Instant,
    #[cfg(test)]
    test_now_ms: Option<Arc<AtomicU64>>,
}
impl Server {
    fn now(&self) -> u64 {
        #[cfg(test)]
        if let Some(now) = &self.test_now_ms {
            return now.load(Ordering::Acquire);
        }
        self.clock.elapsed().as_millis().min(u128::from(u64::MAX)) as u64
    }
}

/// Trusted host access to the same Hub used by HTTP, sockets and maintenance.
/// Attachment validates the lobby setup and sends its ordinary commander start handshake.
#[derive(Clone)]
pub struct NativeRoomControl {
    server: Server,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeRoomStatus {
    pub epoch: u64,
    pub tick: i64,
    pub held: u16,
    pub ending_complete: bool,
    pub paused: bool,
    pub fault: Option<String>,
}
impl NativeRoomControl {
    pub async fn attach(&self, code: &str, state: Match) -> Result<u64, String> {
        self.server
            .hub
            .lock()
            .await
            .attach_native(code, state, self.server.now())
    }
    pub async fn inspect(&self, code: &str) -> Result<NativeRoomStatus, String> {
        self.server.hub.lock().await.native_status(code)
    }
}

/// Serve a bound listener; callers can bind port zero and inspect its actual local address.
/// Graceful shutdown also stops the expiration task and closes upgraded sockets.
pub async fn serve(
    listener: TcpListener,
    config: Config,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> std::io::Result<()> {
    serve_with_native_control(listener, config, shutdown, None).await
}

/// Serve with an optional host control receipt. Dropping the receipt receiver
/// leaves the ordinary server running. Shutdown invalidates the shared control.
pub async fn serve_with_native_control(
    listener: TcpListener,
    config: Config,
    shutdown: impl Future<Output = ()> + Send + 'static,
    ready: Option<oneshot::Sender<NativeRoomControl>>,
) -> std::io::Result<()> {
    serve_inner(listener, config, shutdown, ready, None).await
}

/// Serve the approved private R1 profile through ordinary JSON v0 start/restart.
pub async fn serve_trusted_room(
    listener: TcpListener,
    config: Config,
    shutdown: impl Future<Output = ()> + Send + 'static,
    trusted: TrustedRoomConfig,
) -> std::io::Result<()> {
    serve_inner(listener, config, shutdown, None, Some(trusted)).await
}

async fn serve_inner(
    listener: TcpListener,
    config: Config,
    shutdown: impl Future<Output = ()> + Send + 'static,
    ready: Option<oneshot::Sender<NativeRoomControl>>,
    trusted: Option<TrustedRoomConfig>,
) -> std::io::Result<()> {
    let mut hub = Hub::new(config.max_rooms.clamp(1, 256), config.public_url);
    hub.configure_trusted_room(trusted);
    hub.configure_saves(config.saves_dir);
    hub.configure_maps(config.map_editor.clone());
    hub.configure_records(config.horde_records);
    hub.configure_diagnostics(config.diagnostic_dir, config.tick_log)?;
    // Dropping or aborting this future invalidates the same stopped authority
    // used under the Hub lock, even when graceful shutdown never runs.
    let _lifetime = ServeLifetime(hub.stop_authority());
    let server = Server {
        hub: Arc::new(Mutex::new(hub)),
        next: Arc::new(AtomicU64::new(1)),
        clock: Instant::now(),
        #[cfg(test)]
        test_now_ms: None,
    };
    if let Some(ready) = ready {
        let _ = ready.send(NativeRoomControl {
            server: server.clone(),
        });
    }
    let shutdown_server = server.clone();
    let final_server = server.clone();
    let (stop, mut expired) = watch::channel(false);
    let maintenance = server.clone();
    let task = tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(10));
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    let now_ns = maintenance.clock.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64;
                    let mut hub = maintenance.hub.lock().await;
                    hub.poll_native(maintenance.now(), now_ns);
                },
                _ = expired.changed() => break,
            }
        }
    });
    let app = Router::new()
        .route(
            "/health",
            get(|| async { Json(json!({"status":"ok","service":"ww2-server","scope":"lobby"})) }),
        )
        .route("/api/rooms", get(directory))
        .route("/maps", any(maps::serve))
        .route("/ws", get(upgrade))
        .fallback(maps::serve)
        .layer(axum::Extension(config.map_editor))
        .with_state((server, stop.subscribe()));
    let result = axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            shutdown.await;
            shutdown_server.hub.lock().await.stop();
            let _ = stop.send(true);
        })
        .await;
    task.abort();
    maintenance_stop(&final_server).await;
    result
}

async fn maintenance_stop(server: &Server) {
    server.hub.lock().await.stop();
}

type App = (Server, watch::Receiver<bool>);
async fn directory(
    State((server, _)): State<App>,
) -> ([(header::HeaderName, &'static str); 1], Json<Value>) {
    (
        [(header::CACHE_CONTROL, "no-store")],
        Json(server.hub.lock().await.directory()),
    )
}
async fn upgrade(
    ws: WebSocketUpgrade,
    RawQuery(query): RawQuery,
    State((server, stop)): State<App>,
) -> axum::response::Response {
    // Decode with the same URL query rules as the prototype, including %xx and '+' encoding.
    let code = query.as_deref().and_then(room_query).unwrap_or_default();
    ws.max_message_size(MAX_MESSAGE)
        .max_frame_size(MAX_MESSAGE)
        .on_upgrade(move |socket| socket_loop(socket, code, server, stop))
}
fn room_query(query: &str) -> Option<String> {
    for part in query.split('&') {
        let (key, value) = part.split_once('=').unwrap_or((part, ""));
        if decode_query(key).as_deref() == Some("room") {
            return decode_query(value);
        }
    }
    None
}
fn decode_query(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut result = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = s.get(i + 1..i + 3)?;
            result.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            result.push(if bytes[i] == b'+' { b' ' } else { bytes[i] });
            i += 1;
        }
    }
    String::from_utf8(result).ok()
}
async fn close(socket: &mut WebSocket, code: u16, reason: &'static str) {
    let _ = socket
        .send(Message::Close(Some(CloseFrame {
            code,
            reason: reason.into(),
        })))
        .await;
}
async fn socket_loop(
    mut socket: WebSocket,
    code: String,
    server: Server,
    mut stop: watch::Receiver<bool>,
) {
    if *stop.borrow() {
        close(&mut socket, 1001, "server shutdown").await;
        return;
    }
    if !(3..=12).contains(&code.len()) || !code.bytes().all(|b| b.is_ascii_alphanumeric()) {
        close(&mut socket, 1008, "bad room").await;
        return;
    }
    let (tx, mut rx) = mpsc::channel(64);
    let (cancel, mut cancelled) = watch::channel(false);
    let Ok(id) = server
        .next
        .try_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
    else {
        close(&mut socket, 1013, "connection identity exhausted").await;
        return;
    };
    let mut connection = Connection::new(id, tx, cancel);
    let diagnostics = server.hub.lock().await.diagnostics_enabled();
    if diagnostics {
        connection.enable_diagnostics();
    }
    let mut joined = false;
    loop {
        tokio::select! {
            biased;
            _ = stop.changed() => { close(&mut socket,1001,"server shutdown").await; break; }
            _ = cancelled.changed() => { close(&mut socket,1013,"too far behind").await; break; }
            Some(output) = rx.recv() => {
                let diagnostic_bytes = output.diagnostic_bytes();
                match output {
                    Output::Json(v) => if socket.send(Message::Text(v.to_string().into())).await.is_err() { break; },
                    Output::Watcher(json) => {
                        let result = tokio::select! {
                            result = socket.send(Message::Text(json.to_string().into())) => result.is_ok(),
                            _ = stop.changed() => false,
                            _ = cancelled.changed() => false,
                        };
                        if !result { break; }
                    },
                    Output::Snapshot(mut write) => {
                        let message = Message::Text(std::mem::take(&mut write.json).into());
                        let result = tokio::select! {
                            result = socket.send(message) => Some(result.is_ok()),
                            _ = stop.changed() => None,
                            _ = cancelled.changed() => None,
                        };
                        let mut hub = server.hub.lock().await;
                        if result.is_none() {
                            hub.disconnected(&code, connection.id, server.now());
                        }
                        hub.complete_snapshot(&write, result == Some(true), server.now());
                        if result != Some(true) { break; }
                    }
                    Output::Close(c,r) => { close(&mut socket,c,r).await; break; }
                }
                connection.complete_bytes(diagnostic_bytes);
            }
            message = socket.recv() => {
                let bytes = match message {
                    Some(Ok(Message::Text(s))) => s.as_bytes().to_vec(),
                    Some(Ok(Message::Binary(b))) => b.to_vec(),
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(Message::Ping(_) | Message::Pong(_))) => continue,
                    Some(Err(error)) => {
                        // Tungstenite reports capacity errors before yielding an oversized message.
                        if error.to_string().contains("capacity") || error.to_string().contains("too long") || error.to_string().contains("too large") {
                            close(&mut socket,1009,"message too large").await;
                        }
                        break;
                    }
                };
                if let Ok(value) = serde_json::from_slice(&bytes) {
                    server.hub.lock().await.message(&code,&connection,&mut joined,value,server.now());
                }
            }
        }
    }
    // Close admission before draining reservations. No fresh catch-up may be
    // produced by terminal cleanup, including an old replaced connection.
    rx.close();
    let mut hub = server.hub.lock().await;
    hub.disconnected(&code, connection.id, server.now());
    while let Ok(output) = rx.try_recv() {
        if let Output::Snapshot(write) = output {
            hub.complete_snapshot(&write, false, server.now());
        }
    }
    hub.retire_connection(connection.id);
    connection.clear_bytes();
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn room_query_preserves_case_and_decodes_url_values() {
        assert_eq!(room_query("x=1&room=%41bC&room=other"), Some("AbC".into()));
        assert_eq!(room_query("room=a+b"), Some("a b".into()));
        assert_eq!(room_query("room=%zz"), None);
    }
}

#[cfg(test)]
#[path = "native_socket_tests.rs"]
mod native_socket_tests;

#[cfg(test)]
#[path = "session_batch_socket_tests.rs"]
mod session_batch_socket_tests;

#[cfg(test)]
#[path = "net_batch50_http_tests.rs"]
mod net_batch50_http;

#[cfg(test)]
mod map_editor_tests;
