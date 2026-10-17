//! In-memory preparation of editor saves. File access and HTTP admission belong to the host.
use serde_json::{Value, json};

use crate::map::{AuthoritativeMap, MapError};

pub const MAX_SAVE_BYTES: usize = 8 * 1024 * 1024;

pub(crate) fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::Number(v) => v.as_f64() != Some(0.0),
        Value::String(v) => !v.is_empty(),
        _ => true,
    }
}

fn fields(value: &Value, keys: &[&str]) -> Value {
    Value::Object(
        keys.iter()
            .filter_map(|&key| value.get(key).map(|v| (key.to_owned(), v.clone())))
            .collect(),
    )
}

fn project_list(value: &Value, keys: &[&str]) -> Value {
    Value::Array(
        value
            .as_array()
            .expect("validated map list")
            .iter()
            .map(|v| fields(v, keys))
            .collect(),
    )
}

/// Validate the received document before retaining the prototype's editor save fields.
/// The limit applies to incoming bytes, including properties that projection discards.
/// This does not expose private map data or enable a network editor endpoint.
pub fn prepare_map_save(bytes: &[u8]) -> Result<Vec<u8>, MapError> {
    if bytes.len() > MAX_SAVE_BYTES {
        return Err(MapError::Invalid {
            field: "document".into(),
            reason: "editor save exceeds 8 MiB".into(),
        });
    }
    let source: Value = serde_json::from_slice(bytes).map_err(|e| MapError::Invalid {
        field: "document".into(),
        reason: e.to_string(),
    })?;
    let map = AuthoritativeMap::from_editor_value(source)?;
    let source = map.source();
    let mut clean = fields(source, &["name", "w", "h", "rows"])
        .as_object()
        .unwrap()
        .clone();
    for key in [
        "heights",
        "worldVersion",
        "scenario",
        "defend",
        "naval",
        "trenchFacing",
    ] {
        if let Some(value) = source.get(key).filter(|v| truthy(v)) {
            clean.insert(key.into(), value.clone());
        }
    }
    for key in ["pointSequence", "assaultTime"] {
        if let Some(value) = source.get(key) {
            clean.insert(key.into(), value.clone());
        }
    }
    clean.insert(
        "spawns".into(),
        Value::Array(
            source["spawns"]
                .as_array()
                .unwrap()
                .iter()
                .map(|spawn| {
                    let mut row = fields(spawn, &["x", "y"]);
                    if spawn.get("assault").is_some_and(truthy) {
                        row["assault"] = json!(true);
                    }
                    row
                })
                .collect(),
        ),
    );
    clean.insert(
        "points".into(),
        Value::Array(
            source["points"]
                .as_array()
                .unwrap()
                .iter()
                .map(|point| {
                    let mut row = fields(point, &["x", "y", "needs"]);
                    for key in ["id", "kind"] {
                        if let Some(value) = point.get(key).filter(|v| truthy(v)) {
                            row[key] = value.clone();
                        }
                    }
                    for key in ["vp", "mp"] {
                        row[key] = point
                            .get(key)
                            .filter(|v| !v.is_null())
                            .cloned()
                            .unwrap_or(json!(1));
                    }
                    row
                })
                .collect(),
        ),
    );
    if let Some(layers) = source.get("layers") {
        let mut layer = fields(layers, &["ground", "objects", "mines"]);
        for key in [
            "materials",
            "groundMaterials",
            "objectMaterials",
            "mineMaterials",
        ] {
            if let Some(values) = layers.get(key) {
                layer[key] = project_list(values, &["c", "material"]);
            }
        }
        clean.insert("layers".into(), layer);
    }
    if let Some(structures) = source.get("structures") {
        clean.insert(
            "structures".into(),
            Value::Array(
                structures
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|structure| {
                        let mut row = fields(structure, &["id", "kind"]);
                        row["sections"] = project_list(
                            &structure["sections"],
                            &["id", "c", "hp", "material", "anchor", "supports"],
                        );
                        row
                    })
                    .collect(),
            ),
        );
    }
    for (key, keys) in [
        ("buildings", &["x", "y", "kind"][..]),
        ("triggers", &["at", "say", "blow"][..]),
    ] {
        if let Some(value) = source
            .get(key)
            .filter(|v| v.as_array().is_some_and(|v| !v.is_empty()))
        {
            clean.insert(key.into(), project_list(value, keys));
        }
    }
    let clean = Value::Object(clean);
    // The prototype omits `world` from its whitelist, even for a validated World
    // document. Validate the input only; do not add a second acceptance rule here.
    serde_json::to_vec(&clean).map_err(|e| MapError::Invalid {
        field: "document".into(),
        reason: e.to_string(),
    })
}

/// Validate editor input without requiring the runtime's typed World metadata.
/// This does not admit a World room or construct live World state.
pub fn validate_editor_map(source: &Value) -> Result<(), MapError> {
    AuthoritativeMap::from_editor_value(source.clone()).map(|_| ())
}

#[cfg(test)]
#[path = "map_save/tests.rs"]
mod tests;
