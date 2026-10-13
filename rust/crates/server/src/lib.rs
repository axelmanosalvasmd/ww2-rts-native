//! Lobby transport and private native match construction. Room simulation is not enabled.
pub mod authoritative_maps;
pub mod snapshot_delta;
pub mod snapshot_lists;
mod lobby;
mod maps;

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
        atomic::{AtomicU64, Ordering},
    },
    time::Instant,
};
use tokio::{
    net::TcpListener,
    sync::{Mutex, mpsc, watch},
    time::{Duration, MissedTickBehavior},
};

const MAX_MESSAGE: usize = 16 * 1024;

/// Host-only configuration. Identity remains the client's room token in protocol v0.
#[derive(Clone, Debug)]
pub struct Config {
    pub max_rooms: usize,
    pub public_url: String,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            max_rooms: 32,
            public_url: String::new(),
        }
    }
}
#[derive(Clone)]
struct Server {
    hub: Arc<Mutex<Hub>>,
    next: Arc<AtomicU64>,
    clock: Instant,
}
impl Server {
    fn now(&self) -> u64 {
        self.clock.elapsed().as_millis().min(u128::from(u64::MAX)) as u64
    }
}

/// Serve a bound listener; callers can bind port zero and inspect its actual local address.
/// Graceful shutdown also stops the expiration task and closes upgraded sockets.
pub async fn serve(
    listener: TcpListener,
    config: Config,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> std::io::Result<()> {
    let server = Server {
        hub: Arc::new(Mutex::new(Hub::new(
            config.max_rooms.clamp(1, 256),
            config.public_url,
        ))),
        next: Arc::new(AtomicU64::new(1)),
        clock: Instant::now(),
    };
    let (stop, mut expired) = watch::channel(false);
    let maintenance = server.clone();
    let task = tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(100));
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = interval.tick() => maintenance.hub.lock().await.expire(maintenance.now()),
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
        .with_state((server, stop.subscribe()));
    let result = axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            shutdown.await;
            let _ = stop.send(true);
        })
        .await;
    task.abort();
    result
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
        if decode_query(key)? == "room" {
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
    if !(3..=12).contains(&code.len()) || !code.bytes().all(|b| b.is_ascii_alphanumeric()) {
        close(&mut socket, 1008, "bad room").await;
        return;
    }
    let (tx, mut rx) = mpsc::channel(64);
    let (cancel, mut cancelled) = watch::channel(false);
    let connection = Connection {
        id: server.next.fetch_add(1, Ordering::Relaxed),
        tx,
        cancel,
    };
    let mut joined = false;
    loop {
        tokio::select! {
            biased;
            _ = stop.changed() => { close(&mut socket,1001,"server shutdown").await; break; }
            _ = cancelled.changed() => { close(&mut socket,1013,"too far behind").await; break; }
            Some(output) = rx.recv() => {
                match output {
                    Output::Json(v) => if socket.send(Message::Text(v.to_string().into())).await.is_err() { break; },
                    Output::Close(c,r) => { close(&mut socket,c,r).await; break; }
                }
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
    server
        .hub
        .lock()
        .await
        .disconnected(&code, connection.id, server.now());
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
