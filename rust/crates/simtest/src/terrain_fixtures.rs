//! Gameplay terrain query producer in the external harness. This module receives
//! original authoritative map bytes and query inputs, never recorded query outputs.
//! Raw levels and heights in metres are distinct. No rendered mesh is sampled.
use serde_json::{Value, json};
use ww2_core::{game::CELL, map::AuthoritativeMap, rules::Rules, terrain};

pub const SCHEMA_VERSION: u32 = 1;
pub const CENTRE_COLUMNS: [&str; 6] = [
    "cell",
    "x",
    "z",
    "rawLevel",
    "levelMetres",
    "heightAtMetres",
];
pub const POINT_COLUMNS: [&str; 5] = ["x", "z", "rawLevel", "levelMetres", "heightAtMetres"];

/// Compute every cell centre and each supplied point with the maintained native
/// gameplay queries. Provenance for an exported native executable belongs to its
/// caller, which knows the actual compiled source/artifact identity.
pub fn produce(
    map_key: &str,
    raw_map: &Value,
    point_inputs: &[(f64, f64)],
) -> Result<Value, String> {
    if map_key.is_empty() {
        return Err("terrain fixture map key is empty".into());
    }
    if point_inputs
        .iter()
        .any(|(x, z)| !x.is_finite() || !z.is_finite())
    {
        return Err("terrain fixture point must be finite".into());
    }
    let bytes = serde_json::to_vec(raw_map).map_err(|e| e.to_string())?;
    let map = AuthoritativeMap::from_json(&bytes).map_err(|e| e.to_string())?;
    let initial = map.initialize_terrain().map_err(|e| e.to_string())?;
    let game = initial.query_game();
    let rules = Rules::compiled().map_err(|e| e.to_string())?;
    let query = |x: f64, z: f64| {
        let raw = f64::from(game.level_at(x, z));
        // The original gameplay callers guard the private heightAt helper on
        // flat maps. Preserve that branch without manufacturing height cells.
        let interpolated = game.height.as_ref().map_or(0.0, |heights| {
            terrain::height_at(&game, &rules, heights, x, z)
        });
        [x, z, raw, raw * rules.vision.level_height, interpolated]
    };
    let centres = (0..game.w * game.h)
        .map(|cell| {
            let row = query(
                (cell % game.w) as f64 * CELL + CELL / 2.0,
                (cell / game.w) as f64 * CELL + CELL / 2.0,
            );
            [cell as f64, row[0], row[1], row[2], row[3], row[4]]
        })
        .collect::<Vec<_>>();
    let points = point_inputs
        .iter()
        .map(|&(x, z)| query(x, z))
        .collect::<Vec<_>>();
    Ok(json!({
        "schemaVersion":SCHEMA_VERSION,
        "mapKey":map_key,
        "sourceFilename":format!("{map_key}.json"),
        "dimensions":{"widthCells":game.w,"heightCells":game.h},
        "cellMetres":CELL,
        "metresPerLevel":rules.vision.level_height,
        "centreColumns":CENTRE_COLUMNS,
        "pointColumns":POINT_COLUMNS,
        "centres":centres,
        "points":points,
    }))
}
