//! Reads values written by `harness/state-codec.mjs`: plain JSON plus tags for what JSON cannot hold
//! (`{"$num": "-0"}`, `{"$undefined": true}`, `{"$map": [[k, v], ...]}`, `{"$set": [...]}` and typed arrays
//! such as `{"$Uint8Array": [...]}` or, in run form, `{"$Uint8Array": {"runs": [value, count, ...]}}`).

use serde_json::Value;

pub type Result<T> = std::result::Result<T, String>;

const TYPED: [&str; 8] = [
    "Float32Array",
    "Float64Array",
    "Int8Array",
    "Uint8Array",
    "Int16Array",
    "Uint16Array",
    "Int32Array",
    "Uint32Array",
];

pub fn is_undefined(v: &Value) -> bool {
    v.as_object().is_some_and(|o| o.contains_key("$undefined"))
}

/// A field that is present and not `undefined`. A `null` field is returned: callers decide what the
/// reference makes of it.
pub fn field<'a>(v: &'a Value, key: &str) -> Option<&'a Value> {
    v.get(key).filter(|x| !is_undefined(x))
}

/// A field that is present, not `undefined` and not `null` (what `??` lets through).
pub fn given<'a>(v: &'a Value, key: &str) -> Option<&'a Value> {
    field(v, key).filter(|x| !x.is_null())
}

/// A number, including the tagged -0, Infinity and NaN.
pub fn number(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::Object(o) => match o.get("$num")?.as_str()? {
            "-0" => Some(-0.0),
            "Infinity" => Some(f64::INFINITY),
            "-Infinity" => Some(f64::NEG_INFINITY),
            "NaN" => Some(f64::NAN),
            _ => None,
        },
        _ => None,
    }
}

/// A required number field.
pub fn num(v: &Value, key: &str) -> Result<f64> {
    given(v, key)
        .and_then(number)
        .ok_or_else(|| format!("{key}: expected a number in {}", short(v)))
}

/// A field `x ?? fallback` would read as a number.
pub fn num_or(v: &Value, key: &str, fallback: f64) -> Result<f64> {
    match given(v, key) {
        None => Ok(fallback),
        Some(x) => number(x).ok_or_else(|| format!("{key}: expected a number")),
    }
}

/// A field as `x < 1` or `x >= 3` would compare it: missing compares false (`None`), `null` as 0.
pub fn compared(v: &Value, key: &str) -> Result<Option<f64>> {
    match field(v, key) {
        None => Ok(None),
        Some(Value::Null) => Ok(Some(0.0)),
        Some(Value::Bool(b)) => Ok(Some(f64::from(u8::from(*b)))),
        Some(x) => number(x)
            .map(Some)
            .ok_or_else(|| format!("{key}: expected a number")),
    }
}

/// JavaScript truthiness of a decoded value.
pub fn truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::String(s)) => !s.is_empty(),
        Some(x) => match number(x) {
            Some(n) => n != 0.0 && !n.is_nan(),
            None => !is_undefined(x),
        },
    }
}

/// The elements of a typed array, in either form, and its type name.
pub fn typed(v: &Value) -> Option<(&'static str, Vec<f64>)> {
    let o = v.as_object()?;
    let name = TYPED
        .iter()
        .find(|name| o.contains_key(&format!("${name}")))?;
    let raw = &o[&format!("${name}")];
    let items = match raw {
        Value::Array(a) => a.iter().map(number).collect::<Option<Vec<f64>>>()?,
        Value::Object(r) => {
            let runs = r.get("runs")?.as_array()?;
            let mut items = Vec::new();
            for pair in runs.chunks(2) {
                let value = number(&pair[0])?;
                let count = pair.get(1)?.as_u64()? as usize;
                items.extend(std::iter::repeat_n(value, count));
            }
            items
        }
        _ => return None,
    };
    Some((name, items))
}

/// A typed array or plain array of numbers.
pub fn numbers(v: &Value) -> Option<Vec<f64>> {
    match v {
        Value::Array(a) => a.iter().map(number).collect(),
        _ => typed(v).map(|(_, items)| items),
    }
}

/// The `[key, value]` pairs of a Map, in its order.
pub fn map_entries(v: &Value) -> Option<Vec<(&Value, &Value)>> {
    v.get("$map")?
        .as_array()?
        .iter()
        .map(|pair| Some((pair.get(0)?, pair.get(1)?)))
        .collect()
}

/// The items of a Set, in its order.
pub fn set_items(v: &Value) -> Option<&Vec<Value>> {
    v.get("$set")?.as_array()
}

/// The start of a value, for error messages.
pub fn short(v: &Value) -> String {
    let s = v.to_string();
    if s.chars().count() > 120 {
        format!("{}...", s.chars().take(120).collect::<String>())
    } else {
        s
    }
}
