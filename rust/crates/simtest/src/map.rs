//! File loading belongs to the external harness. The core receives only bounded authoritative bytes.
use std::{fs::File, io::Read, path::Path};
use ww2_core::map::{AuthoritativeMap, MAX_MAP_BYTES};
pub fn load(path: &Path) -> Result<AuthoritativeMap, String> {
    let file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut bytes = Vec::new();
    file.take((MAX_MAP_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    AuthoritativeMap::from_json(&bytes).map_err(|e| format!("{}: {e}", path.display()))
}
