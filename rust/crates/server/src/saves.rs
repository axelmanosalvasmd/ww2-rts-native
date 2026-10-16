//! Private versioned match files. Only summary projections enter a lobby.
use flate2::{Compression, read::GzDecoder, write::GzEncoder};
use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Write},
    path::PathBuf,
};

pub(crate) const VERSION: u64 = 1;
pub(crate) const AUTHORED_VERSION: u64 = 2;

pub(crate) fn version_for(state: &ww2_core::match_runtime::Match) -> u64 {
    if state.authored_scenario.is_some() {
        AUTHORED_VERSION
    } else {
        VERSION
    }
}
const MAX_SAVE_BYTES: u64 = 128 * 1024 * 1024;

#[derive(Clone)]
pub(crate) struct SaveStore {
    dir: PathBuf,
}
impl SaveStore {
    pub(crate) fn new(dir: PathBuf) -> Self {
        Self { dir }
    }
    pub(crate) fn list(&self, room: &str) -> Vec<Value> {
        let Ok(files) = fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        let mut summaries = files
            .filter_map(Result::ok)
            .filter_map(|file| {
                let name = file.file_name();
                let id = name.to_str()?.strip_suffix(".json")?;
                if !belongs_to(id, room) {
                    return None;
                }
                let value: Value = serde_json::from_slice(&fs::read(file.path()).ok()?).ok()?;
                if value["id"] != id
                    || value["kind"] != id.rsplit('-').next()?
                    || !value["savedAt"].is_u64()
                {
                    return None;
                }
                Some(value)
            })
            .collect::<Vec<_>>();
        summaries.sort_by(|a, b| {
            b["savedAt"]
                .as_u64()
                .cmp(&a["savedAt"].as_u64())
                .then_with(|| b["id"].as_str().cmp(&a["id"].as_str()))
        });
        summaries
    }
    pub(crate) fn lobby_list(&self, room: &str) -> Value {
        Value::Array(
            self.list(room)
                .iter()
                .map(|row| {
                    Value::Object(
                        ["id", "kind", "savedAt", "mode", "minutes", "names", "world"]
                            .iter()
                            .filter_map(|key| row.get(key).map(|v| ((*key).into(), v.clone())))
                            .collect(),
                    )
                })
                .collect(),
        )
    }
    pub(crate) fn write(
        &self,
        room: &str,
        kind: &str,
        state: &Value,
        meta: Value,
        saved_at: u64,
    ) -> Result<String, String> {
        if state["version"] != VERSION && state["version"] != AUTHORED_VERSION {
            return Err("unsupported save version".into());
        }
        let bytes = serde_json::to_vec(state).map_err(|e| e.to_string())?;
        if bytes.len() as u64 > MAX_SAVE_BYTES {
            return Err("save too large".into());
        }
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&bytes).map_err(|e| e.to_string())?;
        let body = encoder.finish().map_err(|e| e.to_string())?;
        fs::create_dir_all(&self.dir).map_err(|e| e.to_string())?;
        let mut timestamp = saved_at;
        let (id, mut file) = loop {
            let id = format!("{room}-{timestamp:013}-{kind}");
            if !belongs_to(&id, room) {
                return Err("bad save id".into());
            }
            if !self.dir.join(format!("{id}.save")).exists() {
                match fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(self.dir.join(format!("{id}.tmp")))
                {
                    Ok(file) => break (id, file),
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                    Err(e) => return Err(e.to_string()),
                }
            }
            timestamp = timestamp.checked_add(1).ok_or("save timestamp exhausted")?;
        };
        let temporary = self.dir.join(format!("{id}.tmp"));
        let result = (|| {
            file.write_all(&body)?;
            file.sync_all()?;
            fs::rename(&temporary, self.dir.join(format!("{id}.save")))?;
            let mut summary = meta
                .as_object()
                .cloned()
                .ok_or_else(|| std::io::Error::other("invalid save metadata"))?;
            for (key, value) in [
                ("id", json!(id)),
                ("kind", json!(kind)),
                ("savedAt", json!(timestamp)),
                ("bytes", json!(body.len())),
            ] {
                summary.insert(key.into(), value);
            }
            fs::write(
                self.dir.join(format!("{id}.json")),
                serde_json::to_vec(&summary)?,
            )?;
            Ok::<_, std::io::Error>(())
        })();
        if let Err(e) = result {
            let _ = fs::remove_file(temporary);
            return Err(e.to_string());
        }
        let keep = if kind == "auto" { 3 } else { 10 };
        for old in self
            .list(room)
            .iter()
            .filter(|row| row["kind"] == kind)
            .skip(keep)
        {
            if let Some(id) = old["id"].as_str().filter(|id| belongs_to(id, room)) {
                for ext in ["save", "json"] {
                    let _ = fs::remove_file(self.dir.join(format!("{id}.{ext}")));
                }
            }
        }
        Ok(id)
    }
    pub(crate) fn read(&self, room: &str, id: &str) -> Result<Value, String> {
        if !belongs_to(id, room) {
            return Err("bad save id".into());
        }
        let file =
            fs::File::open(self.dir.join(format!("{id}.save"))).map_err(|e| e.to_string())?;
        if file.metadata().map_err(|e| e.to_string())?.len() > MAX_SAVE_BYTES {
            return Err("save too large".into());
        }
        let mut bytes = Vec::new();
        GzDecoder::new(file)
            .take(MAX_SAVE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() as u64 > MAX_SAVE_BYTES {
            return Err("save too large".into());
        }
        let value: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        if value["version"] != VERSION && value["version"] != AUTHORED_VERSION {
            return Err("unsupported save version".into());
        }
        if !value.is_object()
            || !["g", "guard", "map", "seats", "settings", "ai"]
                .iter()
                .all(|key| value.get(key).is_some())
        {
            return Err("incomplete save".into());
        }
        Ok(value)
    }
}
pub(crate) fn belongs_to(id: &str, room: &str) -> bool {
    let parts = id.split('-').collect::<Vec<_>>();
    parts.len() == 3
        && parts[0] == room
        && (3..=12).contains(&room.len())
        && room.bytes().all(|b| b.is_ascii_alphanumeric())
        && parts[1].len() == 13
        && parts[1].bytes().all(|b| b.is_ascii_digit())
        && matches!(parts[2], "auto" | "manual")
}
pub(crate) fn unix_millis() -> Result<u64, String> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_millis()
        .try_into()
        .map_err(|_| "save timestamp exhausted".into())
}
