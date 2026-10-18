//! Password-protected authored map persistence, shared by the front and game workers.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
};
use subtle::ConstantTimeEq;

#[derive(Clone)]
pub struct MapEditor {
    inner: Arc<RwLock<Store>>,
    password: [u8; 32],
}
struct Store {
    dir: PathBuf,
    maps: BTreeMap<String, Value>,
    sources: BTreeMap<String, Vec<u8>>,
    invalid: BTreeSet<String>,
    unavailable: BTreeSet<String>,
    revision: u64,
}
impl std::fmt::Debug for MapEditor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MapEditor").finish_non_exhaustive()
    }
}
#[derive(Debug)]
pub(crate) enum SaveError {
    Invalid(ww2_core::map::MapError),
    Io(std::io::Error),
}
impl MapEditor {
    /// Open writable map overrides over the embedded catalogue.
    pub fn open(dir: PathBuf, password: &str) -> std::io::Result<Self> {
        fs::create_dir_all(&dir)?;
        let names: Value = serde_json::from_str(include_str!("map-catalog.json"))?;
        let mut maps = names
            .as_object()
            .unwrap()
            .keys()
            .map(|name| {
                (
                    name.clone(),
                    crate::authoritative_maps::load(name)
                        .unwrap()
                        .source()
                        .clone(),
                )
            })
            .collect::<BTreeMap<_, _>>();
        let mut sources = BTreeMap::new();
        let mut invalid = BTreeSet::new();
        let mut unavailable = BTreeSet::new();
        for file in fs::read_dir(&dir)? {
            let file = file?;
            let name = file.file_name();
            let Some(name) = name
                .to_str()
                .and_then(|n| n.strip_suffix(".json"))
                .filter(|n| valid_name(n))
            else {
                continue;
            };
            // A corrupt optional map must not stop the server. Preserve its bytes
            // for authenticated GET; strict runtime loading still rejects it.
            let Ok(bytes) = fs::read(file.path()) else {
                unavailable.insert(name.to_owned());
                invalid.insert(name.to_owned());
                maps.insert(name.into(), Value::Null);
                continue;
            };
            let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
            if ww2_core::map_save::validate_editor_map(&value).is_err() {
                invalid.insert(name.to_owned());
            }
            maps.insert(name.into(), value);
            sources.insert(name.into(), bytes);
        }
        Ok(Self {
            inner: Arc::new(RwLock::new(Store {
                dir,
                maps,
                sources,
                invalid,
                unavailable,
                revision: 1,
            })),
            password: Sha256::digest(password.as_bytes()).into(),
        })
    }
    /// Match the prototype's EDIT_PASSWORD or persistent six-byte random hex password.
    pub fn from_password_file(
        dir: PathBuf,
        password: Option<&str>,
        file: &Path,
    ) -> std::io::Result<Self> {
        let password = match password.filter(|s| !s.is_empty()) {
            Some(value) => value.to_owned(),
            None => {
                if !file.exists() {
                    let mut bytes = [0; 6];
                    getrandom::fill(&mut bytes).map_err(std::io::Error::other)?;
                    let value = bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
                    let mut options = fs::OpenOptions::new();
                    options.write(true).create_new(true);
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::OpenOptionsExt;
                        options.mode(0o600);
                    }
                    match options.open(file) {
                        Ok(mut file) => file.write_all(value.as_bytes())?,
                        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                        Err(error) => return Err(error),
                    }
                }
                fs::read_to_string(file)?.trim().into()
            }
        };
        Self::open(dir, &password)
    }
    pub(crate) fn authorized(&self, given: &[u8]) -> bool {
        let given: [u8; 32] = Sha256::digest(given).into();
        bool::from(self.password.ct_eq(&given))
    }
    pub(crate) fn raw(&self, name: &str) -> Option<Value> {
        self.inner.read().unwrap().maps.get(name).cloned()
    }
    pub(crate) fn unavailable(&self, name: &str) -> bool {
        self.inner.read().unwrap().unavailable.contains(name)
    }
    pub(crate) fn raw_bytes(&self, name: &str) -> Option<Vec<u8>> {
        let store = self.inner.read().unwrap();
        store.sources.get(name).cloned().or_else(|| {
            store
                .maps
                .get(name)
                .map(|value| serde_json::to_vec(value).unwrap())
        })
    }
    pub(crate) fn catalog(&self) -> (u64, Value) {
        let store = self.inner.read().unwrap();
        let fallback = crate::authoritative_maps::load("default").unwrap();
        let rows = store.maps.iter().map(|(name, raw)| {
            // loadMap(name, false) uses the frozen default on invalid input.
            let raw = if store.invalid.contains(name) { fallback.source() } else { raw };
            let spawns = raw["spawns"].as_array().unwrap();
            (name.clone(), json!({"normal":spawns.iter().filter(|s|s["assault"]!=true).count(),
                "assault":spawns.len(),"horde":raw["defend"].as_array().is_some_and(|v|!v.is_empty())}))
        }).collect::<serde_json::Map<_,_>>();
        (store.revision, Value::Object(rows))
    }
    pub(crate) fn save(&self, name: &str, bytes: &[u8]) -> Result<(), SaveError> {
        if !valid_name(name) {
            return Err(SaveError::Io(std::io::Error::other("bad map name")));
        }
        let clean = ww2_core::map_save::prepare_map_save(bytes).map_err(SaveError::Invalid)?;
        let raw: Value = serde_json::from_slice(&clean).expect("validated map JSON");
        let mut store = self.inner.write().unwrap();
        fs::write(store.dir.join(format!("{name}.json")), &clean).map_err(SaveError::Io)?;
        if ww2_core::map_save::validate_editor_map(&raw).is_err() {
            store.invalid.insert(name.into());
        } else {
            store.invalid.remove(name);
        }
        store.unavailable.remove(name);
        store.sources.insert(name.into(), clean);
        store.maps.insert(name.into(), raw);
        store.revision += 1;
        Ok(())
    }
}
pub(crate) fn valid_name(name: &str) -> bool {
    (1..=32).contains(&name.len())
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}
