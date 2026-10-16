//! Host diagnostics stay outside simulation and recipient state.
use crate::tick_timing::{PHASES, TimingStats};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    fs::{OpenOptions, create_dir_all},
    io::Write,
    path::PathBuf,
    sync::mpsc::{SyncSender, sync_channel},
    time::{SystemTime, UNIX_EPOCH},
};

const EVERY_MS: u64 = 5_000;
const CLIENT_KEYS: [&str; 12] = [
    "fps", "worst", "cpu", "snaps", "gap", "kbs", "ping", "jitter", "loss", "mem", "units", "fx",
];

pub(crate) fn clean_client(value: &Value) -> Value {
    let Some(values) = value.as_array() else {
        return Value::Null;
    };
    Value::Object(
        CLIENT_KEYS
            .into_iter()
            .enumerate()
            .map(|(i, key)| {
                let number = values
                    .get(i)
                    .and_then(Value::as_f64)
                    .filter(|v| v.is_finite());
                (
                    key.into(),
                    number.map_or(Value::Null, |v| json!(v.clamp(0.0, 1e6))),
                )
            })
            .collect(),
    )
}

#[derive(Default)]
pub(crate) struct PlayerDiagnostics {
    pub at: Option<u64>,
    pub client: Value,
    pub buffered: u64,
}

#[derive(Default)]
pub(crate) struct RoomDiagnostics {
    window: Option<(u64, i64)>,
    next_log: Option<u64>,
}
impl RoomDiagnostics {
    pub fn due(&mut self, now: u64, tick: i64) -> Option<f64> {
        let (since, first_tick) = *self.window.get_or_insert((now, tick));
        let elapsed = now.saturating_sub(since);
        if elapsed < EVERY_MS {
            return None;
        }
        self.window = Some((now, tick));
        // Pauses start a fresh measurement window instead of reporting a low tick rate.
        if elapsed > EVERY_MS + EVERY_MS / 2 {
            return None;
        }
        Some((tick - first_tick).max(0) as f64 * 1000.0 / elapsed as f64)
    }
    pub fn phase_due(&mut self, now: u64) -> bool {
        let next = self.next_log.get_or_insert(now.saturating_add(30_000));
        if now < *next {
            return false;
        }
        *next = now.saturating_add(30_000);
        true
    }
}
pub(crate) fn phase_log(code: &str, stats: &TimingStats, cadence: u8) -> String {
    let costs = PHASES
        .iter()
        .zip(stats.phases)
        .map(|(phase, v)| format!("{phase} {:.2}/{:.2} ms", v.p50, v.p95))
        .collect::<Vec<_>>()
        .join(", ");
    format!("[tick {code}] p50/p95: {costs}; snapshots every {cadence} ticks")
}

/// One bounded writer keeps disk latency off the room lock. Saturation drops diagnostics.
enum WriteRequest {
    Line(String, Value),
    #[cfg(test)]
    Flush(std::sync::mpsc::Sender<()>),
}
pub(crate) struct Recorder {
    sender: SyncSender<WriteRequest>,
    last_poll_ns: Option<u64>,
    delay_ms: VecDeque<f64>,
    late_ms: f64,
}
impl Recorder {
    pub fn new(dir: PathBuf) -> std::io::Result<Self> {
        let (sender, receiver) = sync_channel::<WriteRequest>(64);
        std::thread::Builder::new()
            .name("ww2-diagnostics".into())
            .spawn(move || {
                for request in receiver {
                    let (code, line) = match request {
                        WriteRequest::Line(code, line) => (code, line),
                        #[cfg(test)]
                        WriteRequest::Flush(done) => {
                            let _ = done.send(());
                            continue;
                        }
                    };
                    if create_dir_all(&dir).is_err() {
                        continue;
                    }
                    if let Ok(mut file) = OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(dir.join(format!("diag-{code}.jsonl")))
                    {
                        let _ = writeln!(file, "{line}");
                    }
                }
            })?;
        Ok(Self {
            sender,
            last_poll_ns: None,
            delay_ms: VecDeque::with_capacity(500),
            late_ms: 0.0,
        })
    }
    #[cfg(test)]
    pub(crate) fn flush(&self) {
        let (done, wait) = std::sync::mpsc::channel();
        self.sender.send(WriteRequest::Flush(done)).unwrap();
        wait.recv_timeout(std::time::Duration::from_secs(10))
            .unwrap();
    }
    pub fn observe_poll(&mut self, now_ns: u64, lateness_ns: Option<u64>) {
        if let Some(last) = self.last_poll_ns {
            let delay = now_ns.saturating_sub(last).saturating_sub(10_000_000) as f64 / 1e6;
            if self.delay_ms.len() == 500 {
                self.delay_ms.pop_front();
            }
            self.delay_ms.push_back(delay);
        }
        self.last_poll_ns = Some(now_ns);
        self.late_ms = self.late_ms.max(lateness_ns.unwrap_or(0) as f64 / 1e6);
    }
    pub fn record(&mut self, code: &str, mut line: Value) {
        // Room codes arrive sanitized, but keep the file sink confined independently.
        if code.is_empty() || !code.bytes().all(|b| b.is_ascii_alphanumeric()) {
            return;
        }
        let mut delays = self.delay_ms.iter().copied().collect::<Vec<_>>();
        delays.sort_by(f64::total_cmp);
        let p99 = delays
            .get((delays.len() * 99).div_ceil(100).saturating_sub(1))
            .copied()
            .unwrap_or(0.0);
        line["loopP99"] = json!(p99);
        line["loopMax"] = json!(delays.last().copied().unwrap_or(0.0));
        line["lateMax"] = json!(self.late_ms);
        line["t"] = json!(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
                .min(u128::from(u64::MAX)) as u64
        );
        let _ = self.sender.try_send(WriteRequest::Line(code.into(), line));
        self.delay_ms.clear();
        self.late_ms = 0.0;
    }
}

#[cfg(test)]
#[path = "diagnostics_tests.rs"]
mod tests;
