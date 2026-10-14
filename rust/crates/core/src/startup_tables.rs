//! Internal static table producer. Transport, FFI and Unreal consumption remain separate.
//! Encoding v1 uses sorted keys, source-order arrays, compact UTF-8 JSON and one final LF.
//! Numbers use the locked serde_json finite formatter with integral .0 removed; special
//! IEEE754 values use the four agreed tags. Byte correspondence is checked against Node.
use crate::{
    catalog::{self, Catalog, Value},
    formation, rules,
};
use serde_json::Value as Json;
use std::{fmt, sync::OnceLock};

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableError {
    pub path: String,
    pub reason: String,
}
impl fmt::Display for TableError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "startup tables {}: {}", self.path, self.reason)
    }
}
impl std::error::Error for TableError {}
fn error(path: &str, reason: impl Into<String>) -> TableError {
    TableError {
        path: path.into(),
        reason: reason.into(),
    }
}
fn catalog_error(e: catalog::CatalogError) -> TableError {
    error("/", e.to_string())
}
fn schema() -> Result<&'static Json, TableError> {
    static SCHEMA: OnceLock<Result<Json, TableError>> = OnceLock::new();
    SCHEMA
        .get_or_init(|| {
            serde_json::from_str(include_str!(
                "../../../../docs/contracts/startup-tables/proposed-content.schema.json"
            ))
            .map_err(|e| error("/schema", e.to_string()))
        })
        .as_ref()
        .map_err(Clone::clone)
}

/// The only compiled producer, cached as immutable core-owned bytes. No I/O or match state.
pub fn compiled_json() -> Result<&'static [u8], TableError> {
    static BYTES: OnceLock<Result<Vec<u8>, TableError>> = OnceLock::new();
    BYTES
        .get_or_init(|| {
            let base = Catalog::compiled().map_err(catalog_error)?;
            let additions =
                Catalog::from_json(include_str!("../data/startup-table-additions.json"))
                    .map_err(catalog_error)?;
            from_catalogs(base, &additions)
        })
        .as_ref()
        .map(Vec::as_slice)
        .map_err(Clone::clone)
}

/// Build the complete agreed static shape from catalogs, preserving every source field.
/// No defaults are added. Duplicate table names and incompatible catalog metadata refuse.
pub fn from_catalogs(base: &Catalog, additions: &Catalog) -> Result<Vec<u8>, TableError> {
    if base.format() != Some(1.) || additions.format() != Some(1.) {
        return Err(error("/format", "expected catalog format 1"));
    }
    if base.reference() != additions.reference() {
        return Err(error("/reference", "catalog source pins disagree"));
    }
    let mut tables = base.tables().clone();
    for (name, value) in additions.tables() {
        if tables.insert(name.clone(), value.clone()).is_some() {
            return Err(error(&format!("/tables/{name}"), "duplicate catalog table"));
        }
    }
    let object = |pairs: Vec<(&str, Value)>| {
        Value::Object(pairs.into_iter().map(|(k, v)| (k.into(), v)).collect())
    };
    let numbers = object(vec![
        ("infantrySlot", Value::Number(formation::INFANTRY_SLOT)),
        ("vehicleSlot", Value::Number(formation::VEHICLE_SLOT)),
        ("slotMargin", Value::Number(formation::SLOT_MARGIN)),
        ("rankGap", Value::Number(formation::RANK_GAP)),
        ("lineRank", Value::Number(formation::LINE_RANK)),
        ("maxStretch", Value::Number(formation::MAX_STRETCH)),
        ("depthSteps", Value::Number(formation::DEPTH_STEPS)),
    ]);
    let formed = object(vec![
        ("hash", Value::String(format!("{:016x}", rules::RULES_HASH))),
        ("scope", Value::String("formation-only".into())),
        (
            "shapes",
            Value::Array(
                formation::SHAPES
                    .iter()
                    .map(|s| Value::String((*s).into()))
                    .collect(),
            ),
        ),
        ("numbers", numbers),
    ]);
    let root = object(vec![
        ("schemaVersion", Value::Number(f64::from(SCHEMA_VERSION))),
        ("reference", Value::String(base.reference().into())),
        ("formation", formed),
        ("tables", Value::Object(tables)),
    ]);
    let mut output = String::new();
    encode(&root, &mut output)?;
    output.push('\n');
    // Return nothing until the entire agreed schema and tag rules have admitted.
    parse_json(output.as_bytes())?;
    Ok(output.into_bytes())
}

/// Validate and decode atomically. On refusal no partial value/cache is returned.
/// Content hashes and transport negotiation belong to the later interface agreement.
pub fn parse_json(bytes: &[u8]) -> Result<Value, TableError> {
    let json: Json = serde_json::from_slice(bytes).map_err(|e| error("/", e.to_string()))?;
    validate(schema()?, &json, "")?;
    catalog::decode(json).map_err(catalog_error)
}

fn same_json(a: &Json, b: &Json) -> bool {
    match (a, b) {
        (Json::Number(a), Json::Number(b)) => a.as_f64() == b.as_f64(),
        (Json::Array(a), Json::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| same_json(a, b))
        }
        (Json::Object(a), Json::Object(b)) => {
            a.len() == b.len()
                && a.iter()
                    .all(|(k, v)| b.get(k).is_some_and(|b| same_json(v, b)))
        }
        _ => a == b,
    }
}
fn pointer(path: &str, key: &str) -> String {
    format!("{path}/{}", key.replace('~', "~0").replace('/', "~1"))
}
// This implements every keyword emitted by the pinned schema, and rejects any
// future unknown constraint instead of treating it as successfully validated.
fn validate(schema: &Json, value: &Json, path: &str) -> Result<(), TableError> {
    let shape = schema
        .as_object()
        .ok_or_else(|| error(path, "invalid internal schema"))?;
    const KEYS: &[&str] = &[
        "$schema",
        "$id",
        "title",
        "const",
        "enum",
        "anyOf",
        "type",
        "required",
        "additionalProperties",
        "properties",
        "items",
    ];
    if shape.keys().any(|k| !KEYS.contains(&k.as_str())) {
        return Err(error(path, "unsupported internal schema keyword"));
    }
    if let Some(expected) = shape.get("const")
        && !same_json(expected, value)
    {
        return Err(error(
            path,
            format!("expected {expected}, received {value}"),
        ));
    }
    if let Some(options) = shape.get("enum").and_then(Json::as_array)
        && !options.iter().any(|option| same_json(option, value))
    {
        return Err(error(path, "value outside the agreed enum"));
    }
    if let Some(options) = shape.get("anyOf").and_then(Json::as_array)
        && !options
            .iter()
            .any(|option| validate(option, value, path).is_ok())
    {
        return Err(error(path, "no agreed schema alternative matches"));
    }
    if let Some(kind) = shape.get("type").and_then(Json::as_str) {
        let matches = match kind {
            "null" => value.is_null(),
            "boolean" => value.is_boolean(),
            "number" => value.is_number(),
            "string" => value.is_string(),
            "array" => value.is_array(),
            "object" => value.is_object(),
            _ => false,
        };
        if !matches {
            return Err(error(path, format!("expected {kind}")));
        }
    }
    if let Some(values) = value.as_object() {
        if let Some(required) = shape.get("required").and_then(Json::as_array) {
            for key in required {
                let key = key
                    .as_str()
                    .ok_or_else(|| error(path, "invalid schema required key"))?;
                if !values.contains_key(key) {
                    return Err(error(&pointer(path, key), "required field absent"));
                }
            }
        }
        let empty = serde_json::Map::new();
        let properties = shape
            .get("properties")
            .and_then(Json::as_object)
            .unwrap_or(&empty);
        for (key, item) in values {
            match properties.get(key) {
                Some(child) => validate(child, item, &pointer(path, key))?,
                None if shape.get("additionalProperties") == Some(&Json::Bool(false)) => {
                    return Err(error(&pointer(path, key), "unknown field"));
                }
                None => {}
            }
        }
    }
    if let Some(values) = value.as_array()
        && let Some(items) = shape.get("items")
    {
        for (i, value) in values.iter().enumerate() {
            validate(items, value, &pointer(path, &i.to_string()))?;
        }
    }
    Ok(())
}

fn encode(value: &Value, out: &mut String) -> Result<(), TableError> {
    let quote = |text: &str| serde_json::to_string(text).map_err(|e| error("/", e.to_string()));
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(v) => out.push_str(if *v { "true" } else { "false" }),
        Value::String(v) => out.push_str(&quote(v)?),
        Value::Number(v) => {
            let tag = if v.is_nan() {
                Some("NaN")
            } else if *v == f64::INFINITY {
                Some("Infinity")
            } else if *v == f64::NEG_INFINITY {
                Some("-Infinity")
            } else if *v == 0. && v.is_sign_negative() {
                Some("-0")
            } else {
                None
            };
            if let Some(tag) = tag {
                out.push_str(&format!("{{\"$number\":\"{tag}\"}}"));
            } else {
                let number = serde_json::Number::from_f64(*v)
                    .ok_or_else(|| error("/", "invalid finite number"))?
                    .to_string();
                out.push_str(number.strip_suffix(".0").unwrap_or(&number));
            }
        }
        Value::Array(values) => {
            out.push('[');
            for (i, v) in values.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                encode(v, out)?;
            }
            out.push(']');
        }
        Value::Object(values) => {
            out.push('{');
            for (i, (k, v)) in values.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&quote(k)?);
                out.push(':');
                encode(v, out)?;
            }
            out.push('}');
        }
    }
    Ok(())
}
