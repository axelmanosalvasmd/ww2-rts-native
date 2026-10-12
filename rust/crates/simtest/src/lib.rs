//! Loads the issue #2 harness's game states into the rules core and checks the core against the reference
//! JavaScript: the vision and fog passes line for line (`harness/vision-check.mjs`), movement and crowd
//! separation call by call (`harness/movement-trace.mjs`), and the Rust side of the frozen workload's timings
//! (`harness/sim-workload.mjs`). Kept out of `ww2-core`, which reads no files and parses no JSON.

pub mod check;
pub mod codec;
pub mod load;
pub mod replay;
pub mod workload;

use serde_json::Value;

/// Reads a state file's text: its `game`, with the fog caches saved alongside, and its `tables` when the
/// file carries them (the committed fixture does; harness states use `harness/out/tables.json`).
pub fn parse(text: &str) -> codec::Result<Value> {
    serde_json::from_str(text).map_err(|e| format!("not JSON: {e}"))
}
