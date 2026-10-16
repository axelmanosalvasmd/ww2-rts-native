//! Horde record persistence, matching reference server.js:66-86 and :753.
use serde_json::{Map, Value, json};
use std::{
    fs,
    path::PathBuf,
    sync::{Arc, Mutex},
};

#[derive(Clone, Debug)]
pub(crate) struct Run {
    pub(crate) wave: u64,
    pub(crate) kills: u64,
    pub(crate) time: u64,
    pub(crate) names: Vec<String>,
}

impl Run {
    pub(crate) fn json(&self) -> Value {
        json!({"wave":self.wave,"kills":self.kills,"time":self.time,"names":self.names})
    }

    fn beats(&self, best: Option<&Value>) -> bool {
        let Some(best) = best else { return true };
        let Some(wave) = best["wave"].as_f64() else {
            return false;
        };
        self.wave as f64 > wave
            || (self.wave as f64 == wave
                && best["time"]
                    .as_f64()
                    .is_some_and(|time| self.time as f64 > time))
    }
}

struct State {
    path: PathBuf,
    records: Option<Map<String, Value>>,
}

impl State {
    fn records(&mut self) -> &mut Map<String, Value> {
        self.records.get_or_insert_with(|| {
            fs::read(&self.path)
                .ok()
                .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
                .and_then(|value| value.as_object().cloned())
                .unwrap_or_default()
        })
    }
}

/// Clone the store when configuring workers so lookup, comparison and writes
/// share one lock. Independently constructed stores have independent caches.
#[derive(Clone)]
pub struct RecordStore {
    state: Arc<Mutex<State>>,
}

impl RecordStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            state: Arc::new(Mutex::new(State {
                path: path.into(),
                records: None,
            })),
        }
    }

    fn key(map: &str, count: usize, army: &str) -> String {
        format!("{map}|{count}|{army}")
    }

    /// The caller supplies "standard" when the army setting is absent.
    pub(crate) fn lookup(&self, map: &str, count: usize, army: &str) -> Option<Value> {
        self.state
            .lock()
            .expect("Horde record lock")
            .records()
            .get(&Self::key(map, count, army))
            .filter(|value| !value.is_null())
            .cloned()
    }

    /// Sandbox runs have no Horde record result. Failed writes retain the new
    /// in-memory best, as the prototype does, and are reported to stderr.
    pub(crate) fn record(
        &self,
        map: &str,
        count: usize,
        army: &str,
        run: Run,
        sandbox: bool,
    ) -> Option<Value> {
        if sandbox {
            return None;
        }
        let mut state = self.state.lock().expect("Horde record lock");
        let key = Self::key(map, count, army);
        let best = state.records().get(&key).filter(|v| !v.is_null()).cloned();
        let record = run.beats(best.as_ref());
        let mut result = run.json();
        if record {
            state.records().insert(key, result.clone());
            let bytes = serde_json::to_vec_pretty(state.records()).expect("Horde records JSON");
            if let Err(error) = fs::write(&state.path, bytes) {
                eprintln!("horde records not saved: {error}");
            }
        }
        result["record"] = json!(record);
        result["best"] = best.unwrap_or(Value::Null);
        Some(result)
    }
}

impl std::fmt::Debug for RecordStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HordeRecords").finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct File(PathBuf);
    impl File {
        fn new() -> Self {
            Self(std::env::temp_dir().join(format!(
                "ww2-horde-records-{}-{}.json",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            )))
        }
    }
    impl Drop for File {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }
    fn run(wave: u64, time: u64) -> Run {
        Run {
            wave,
            kills: 123,
            time,
            names: vec!["Ana".into(), "Bo".into()],
        }
    }

    #[test]
    fn mode55_records_exact_shape_keys_order_and_durable_reload() {
        let file = File::new();
        let store = RecordStore::new(file.0.clone());
        let first = run(5, 300);
        assert_eq!(
            store.record("bastogne", 2, "standard", first, false),
            Some(
                json!({"wave":5,"kills":123,"time":300,"names":["Ana","Bo"],"record":true,"best":null})
            )
        );
        for candidate in [run(4, 999), run(5, 299), run(5, 300)] {
            let result = store
                .record("bastogne", 2, "standard", candidate, false)
                .unwrap();
            assert_eq!(result["record"], false);
            assert_eq!(result["best"], run(5, 300).json());
        }
        assert_eq!(
            store
                .record("bastogne", 2, "standard", run(5, 301), false)
                .unwrap()["record"],
            true
        );
        assert_eq!(
            store
                .record("bastogne", 2, "standard", run(6, 1), false)
                .unwrap()["record"],
            true
        );
        let disk: Value = serde_json::from_slice(&fs::read(&file.0).unwrap()).unwrap();
        assert_eq!(disk, json!({"bastogne|2|standard":run(6, 1).json()}));
        let reloaded = RecordStore::new(file.0.clone());
        assert_eq!(
            reloaded.lookup("bastogne", 2, "standard"),
            Some(run(6, 1).json())
        );
        assert_eq!(reloaded.lookup("bastogne", 1, "standard"), None);
        assert_eq!(reloaded.lookup("bastogne", 2, "small"), None);
        assert_eq!(reloaded.lookup("other", 2, "standard"), None);
    }

    #[test]
    fn mode55_sandbox_skips_records_and_missing_or_malformed_files_start_empty() {
        let file = File::new();
        let store = RecordStore::new(file.0.clone());
        assert_eq!(store.lookup("map", 1, "standard"), None);
        assert_eq!(store.record("map", 1, "standard", run(99, 99), true), None);
        assert!(!file.0.exists());
        fs::write(&file.0, b"{broken").unwrap();
        let store = RecordStore::new(file.0.clone());
        assert_eq!(store.lookup("map", 1, "standard"), None);
        store.record("map", 1, "standard", run(1, 1), false);
        let before = fs::read(&file.0).unwrap();
        assert_eq!(store.record("map", 1, "standard", run(99, 99), true), None);
        assert_eq!(fs::read(&file.0).unwrap(), before);
        assert_eq!(store.lookup("map", 1, "standard"), Some(run(1, 1).json()));
    }

    #[test]
    fn mode55_cloned_worker_stores_preserve_best_and_other_keys() {
        let file = File::new();
        let store = RecordStore::new(file.0.clone());
        std::thread::scope(|scope| {
            for wave in 1..=12 {
                let worker = store.clone();
                scope.spawn(move || {
                    worker.record("map", 2, "standard", run(wave, 100), false);
                    worker.record("map", 2, &format!("army{wave}"), run(wave, 1), false);
                });
            }
        });
        assert_eq!(
            store.lookup("map", 2, "standard"),
            Some(run(12, 100).json())
        );
        let reloaded = RecordStore::new(file.0.clone());
        assert_eq!(
            reloaded.lookup("map", 2, "standard"),
            Some(run(12, 100).json())
        );
        for wave in 1..=12 {
            assert_eq!(
                reloaded.lookup("map", 2, &format!("army{wave}")),
                Some(run(wave, 1).json())
            );
        }
    }

    #[test]
    fn mode55_write_failure_retains_memory_like_prototype() {
        let file = File::new();
        let store = RecordStore::new(file.0.join("missing-parent").join("records.json"));
        assert_eq!(
            store
                .record("map", 1, "standard", run(3, 4), false)
                .unwrap()["record"],
            true
        );
        assert_eq!(store.lookup("map", 1, "standard"), Some(run(3, 4).json()));
    }
}
