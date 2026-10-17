//! A front router with isolated game threads and supervised worker lifetimes.
use super::*;
use std::{io, panic::AssertUnwindSafe, thread};
use tokio::{runtime::Builder, task::JoinSet};

const RESTART_DELAY: Duration = Duration::from_secs(1);

/// The prototype's FNV-1a hash, including JavaScript UTF-16 code units.
pub fn shard_of(code: &str, count: usize) -> usize {
    assert!((1..=64).contains(&count));
    let hash = code.encode_utf16().fold(2_166_136_261_u32, |hash, unit| {
        (hash ^ u32::from(unit)).wrapping_mul(16_777_619)
    });
    hash as usize % count
}

/// Host configuration uses the prototype's default and 1..=64 bounds.
pub fn shard_count(value: Option<&str>) -> usize {
    value
        .and_then(number)
        .filter(|value| !value.is_nan() && *value != 0.0)
        .unwrap_or(1.0)
        .floor()
        .clamp(1.0, 64.0) as usize
}

fn number(value: &str) -> Option<f64> {
    // Number(string) trims ECMAScript whitespace, including BOM but excluding NEL.
    let value = value.trim_matches(|ch| {
        matches!(
            ch,
            '\u{0009}'..='\u{000d}'
                | '\u{0020}'
                | '\u{00a0}'
                | '\u{1680}'
                | '\u{2000}'..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
        )
    });
    match value {
        "Infinity" | "+Infinity" => return Some(f64::INFINITY),
        "-Infinity" => return Some(f64::NEG_INFINITY),
        _ => {}
    }
    for (prefixes, radix) in [(["0x", "0X"], 16), (["0b", "0B"], 2), (["0o", "0O"], 8)] {
        if let Some(digits) = prefixes
            .iter()
            .find_map(|prefix| value.strip_prefix(prefix))
        {
            if digits.is_empty() {
                return None;
            }
            return digits.chars().try_fold(0.0, |number, digit| {
                Some(number * f64::from(radix) + f64::from(digit.to_digit(radix)?))
            });
        }
    }
    value
        .bytes()
        .all(|byte| byte.is_ascii_digit() || b"+-.eE".contains(&byte))
        .then(|| value.parse().ok())
        .flatten()
}

enum Work {
    Socket(Box<WebSocket>, String),
    Directory(oneshot::Sender<Value>),
    #[cfg(test)]
    Crash,
    #[cfg(test)]
    Control(oneshot::Sender<NativeRoomControl>),
}

#[derive(Clone)]
struct Worker {
    ready: watch::Receiver<Option<mpsc::Sender<Work>>>,
}

impl Worker {
    async fn sender(&self) -> Option<mpsc::Sender<Work>> {
        let mut ready = self.ready.clone();
        loop {
            if let Some(sender) = ready.borrow_and_update().clone() {
                return Some(sender);
            }
            ready.changed().await.ok()?;
        }
    }

    async fn directory(&self) -> Vec<Value> {
        let Some(sender) = self.sender().await else {
            return Vec::new();
        };
        let (send, receive) = oneshot::channel();
        if sender.send(Work::Directory(send)).await.is_err() {
            return Vec::new();
        }
        receive
            .await
            .ok()
            .and_then(|value| value["rooms"].as_array().cloned())
            .unwrap_or_default()
    }
}

struct Lifetime {
    stop: watch::Sender<bool>,
}
impl Drop for Lifetime {
    fn drop(&mut self) {
        let _ = self.stop.send(true);
    }
}

#[derive(Clone)]
struct Front {
    map_editor: Option<MapEditor>,
    static_files: Option<static_files::StaticFiles>,
    workers: Vec<Worker>,
    stop: watch::Receiver<bool>,
}

/// Serve existing JSON v0 messages through N independent game runtimes.
/// Every worker keeps the host's existing private, code-bound R1 policy.
pub async fn serve_sharded(
    listener: TcpListener,
    config: Config,
    shutdown: impl Future<Output = ()> + Send + 'static,
    trusted: Option<TrustedRoomConfig>,
    count: usize,
) -> io::Result<()> {
    if !(1..=64).contains(&count) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "shards must be 1-64",
        ));
    }
    let (stop, stopped) = watch::channel(false);
    let lifetime = Lifetime { stop: stop.clone() };
    let map_editor = config.map_editor.clone();
    let static_files = config
        .static_root
        .clone()
        .map(static_files::StaticFiles::new);
    let (workers, mut supervisors) = start_workers(config, trusted, count, stopped.clone());
    let front = Front {
        map_editor,
        static_files,
        workers,
        stop: stopped,
    };
    for worker in &front.workers {
        if worker.sender().await.is_none() {
            return Err(io::Error::other("game worker failed to start"));
        }
    }
    let result = serve_front(listener, front, async move {
        shutdown.await;
        let _ = stop.send(true);
    })
    .await;
    drop(lifetime);
    while let Some(result) = supervisors.join_next().await {
        if let Err(error) = result {
            return Err(io::Error::other(error));
        }
    }
    result
}

fn start_workers(
    config: Config,
    trusted: Option<TrustedRoomConfig>,
    count: usize,
    stopped: watch::Receiver<bool>,
) -> (Vec<Worker>, JoinSet<()>) {
    let mut workers = Vec::with_capacity(count);
    let mut supervisors = JoinSet::new();
    for index in 0..count {
        let (ready, receive) = watch::channel(None);
        workers.push(Worker { ready: receive });
        supervisors.spawn(supervise(
            index,
            config.clone(),
            trusted.clone(),
            stopped.clone(),
            ready,
        ));
    }
    (workers, supervisors)
}

async fn serve_front(
    listener: TcpListener,
    front: Front,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> io::Result<()> {
    let app = Router::new()
        .route(
            "/health",
            get(|| async { Json(json!({"status":"ok","service":"ww2-server","scope":"lobby"})) }),
        )
        .route("/api/rooms", get(front_directory))
        .route("/maps", any(maps::serve))
        .route("/ws", get(front_upgrade))
        .fallback(front_static_or_maps)
        .layer(axum::Extension(front.map_editor.clone()))
        .with_state(front);
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await
}

async fn front_static_or_maps(
    State(front): State<Front>,
    OriginalUri(uri): OriginalUri,
    method: Method,
    headers: HeaderMap,
    body: Body,
) -> Response<Body> {
    serve_static_or_maps(
        front.static_files.as_ref(),
        front.map_editor.clone(),
        uri,
        method,
        headers,
        body,
    )
    .await
}

async fn front_directory(
    State(front): State<Front>,
) -> ([(header::HeaderName, &'static str); 1], Json<Value>) {
    let mut pending = JoinSet::new();
    let count = front.workers.len();
    for (index, worker) in front.workers.into_iter().enumerate() {
        pending.spawn(async move { (index, worker.directory().await) });
    }
    let mut lists = vec![Vec::new(); count];
    while let Some(Ok((index, rooms))) = pending.join_next().await {
        lists[index] = rooms;
    }
    let rooms: Vec<_> = lists.into_iter().flatten().collect();
    (
        [(header::CACHE_CONTROL, "no-store")],
        Json(json!({"rooms": rooms})),
    )
}

async fn front_upgrade(
    ws: WebSocketUpgrade,
    RawQuery(query): RawQuery,
    State(front): State<Front>,
) -> axum::response::Response {
    let code = query.as_deref().and_then(room_query).unwrap_or_default();
    let worker = front.workers[shard_of(&code, front.workers.len())].clone();
    ws.max_message_size(MAX_MESSAGE)
        .max_frame_size(MAX_MESSAGE)
        .on_upgrade(move |socket| async move {
            let mut stop = front.stop;
            if *stop.borrow() {
                return;
            }
            tokio::select! {
                _ = stop.changed() => {},
                sender = worker.sender() => {
                    if let Some(sender) = sender {
                        let _ = sender.send(Work::Socket(Box::new(socket), code)).await;
                    }
                }
            }
        })
}

async fn supervise(
    index: usize,
    config: Config,
    trusted: Option<TrustedRoomConfig>,
    mut stop: watch::Receiver<bool>,
    ready: watch::Sender<Option<mpsc::Sender<Work>>>,
) {
    loop {
        if *stop.borrow() {
            break;
        }
        let (work, receive) = mpsc::channel(64);
        let (started, starting) = oneshot::channel();
        let (exited, exit) = oneshot::channel();
        let config = config.clone();
        let trusted = trusted.clone();
        let stopped = stop.clone();
        let worker = thread::Builder::new()
            .name(format!("ww2-game-{index}"))
            .spawn(move || {
                let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
                    let runtime = Builder::new_current_thread().enable_all().build()?;
                    runtime.block_on(run_worker(config, trusted, stopped, receive, started))
                }));
                let _ = exited.send(result);
            });
        let Ok(worker) = worker else {
            return;
        };
        let started = matches!(starting.await, Ok(Ok(())));
        if started {
            let _ = ready.send(Some(work));
        }
        let _ = exit.await;
        let _ = ready.send(None);
        let _ = tokio::task::spawn_blocking(move || worker.join()).await;
        if !started {
            return;
        }
        if *stop.borrow() {
            break;
        }
        tokio::select! {
            _ = stop.changed() => break,
            _ = tokio::time::sleep(RESTART_DELAY) => {},
        }
    }
}

async fn run_worker(
    config: Config,
    trusted: Option<TrustedRoomConfig>,
    mut stop: watch::Receiver<bool>,
    mut work: mpsc::Receiver<Work>,
    started: oneshot::Sender<io::Result<()>>,
) -> io::Result<()> {
    let mut hub = Hub::new(config.max_rooms.clamp(1, 256), config.public_url);
    hub.configure_trusted_room(trusted);
    hub.configure_saves(config.saves_dir);
    hub.configure_authored_maps(config.authored_maps_dir);
    hub.configure_maps(config.map_editor);
    hub.configure_records(config.horde_records);
    if let Err(error) = hub.configure_diagnostics(config.diagnostic_dir, config.tick_log) {
        let _ = started.send(Err(error));
        return Ok(());
    }
    let _lifetime = ServeLifetime(hub.stop_authority());
    let server = Server {
        hub: Arc::new(Mutex::new(hub)),
        next: Arc::new(AtomicU64::new(1)),
        clock: Instant::now(),
        static_files: None,
        #[cfg(test)]
        test_now_ms: None,
    };
    let mut sockets = JoinSet::new();
    let mut interval = tokio::time::interval(Duration::from_millis(10));
    interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let _ = started.send(Ok(()));
    loop {
        if *stop.borrow() {
            break;
        }
        // Fair polling keeps requests serviceable when maintenance stays due.
        tokio::select! {
            _ = stop.changed() => break,
            Some(result) = sockets.join_next(), if !sockets.is_empty() => {
                if result.is_err() { panic!("game socket task failed"); }
            },
            _ = interval.tick() => {
                let now_ns = server.clock.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64;
                server.hub.lock().await.poll_native(server.now(), now_ns);
            },
            message = work.recv() => match message {
                Some(Work::Socket(socket, code)) => {
                    sockets.spawn(socket_loop(*socket, code, server.clone(), stop.clone()));
                },
                Some(Work::Directory(send)) => {
                    let _ = send.send(server.hub.lock().await.directory());
                },
                #[cfg(test)]
                Some(Work::Crash) => panic!("test worker crash"),
                #[cfg(test)]
                Some(Work::Control(send)) => {
                    let _ = send.send(NativeRoomControl { server: server.clone() });
                },
                None => break,
            }
        }
    }
    server.hub.lock().await.stop();
    sockets.abort_all();
    while sockets.join_next().await.is_some() {}
    Ok(())
}

#[cfg(test)]
mod tests;
