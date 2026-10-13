//! Stable summaries for comparing raw server-message recordings with the browser view.
use serde_json::Value;
use ww2_core::client_view::ClientView;

const BASIS: u64 = 0xcbf29ce484222325;
fn feed(hash: &mut u64, bytes: &[u8]) {
    for byte in bytes {
        *hash = (*hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3);
    }
}
fn value_hash(hash: &mut u64, value: &Value) {
    match value {
        Value::Null => feed(hash, b"n"),
        Value::Bool(v) => feed(hash, if *v { b"t" } else { b"f" }),
        Value::Number(v) => {
            feed(hash, b"d");
            let n = v.as_f64().expect("JSON number fits f64");
            feed(hash, &if n == 0.0 { 0.0_f64 } else { n }.to_le_bytes());
        }
        Value::String(v) => {
            feed(hash, b"s");
            feed(hash, v.as_bytes());
            feed(hash, &[0]);
        }
        Value::Array(values) => {
            feed(hash, b"a");
            for v in values {
                value_hash(hash, v);
            }
            feed(hash, b"]");
        }
        Value::Object(values) => {
            feed(hash, b"o");
            let mut keys: Vec<_> = values.keys().collect();
            keys.sort_unstable_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
            for key in keys {
                feed(hash, b"s");
                feed(hash, key.as_bytes());
                feed(hash, &[0]);
                value_hash(hash, &values[key]);
            }
            feed(hash, b"}");
        }
    }
}
fn array_hash<'a>(values: impl Iterator<Item = &'a Value>) -> u64 {
    let mut hash = BASIS;
    feed(&mut hash, b"a");
    for value in values {
        value_hash(&mut hash, value);
    }
    feed(&mut hash, b"]");
    hash
}
fn bytes_hash(bytes: &[u8]) -> u64 {
    let mut hash = BASIS;
    feed(&mut hash, bytes);
    hash
}
/// One JSON line per input message, with keys in the shared recorder's order.
pub fn report(view: &ClientView) -> String {
    let ids: Vec<_> = view.units().map(|u| u.id).collect();
    format!(
        "{{\"ids\":{},\"units\":\"{:016x}\",\"seen\":\"{:016x}\",\"explored\":\"{:016x}\",\"cells\":{},\"terrain\":\"{:016x}\",\"resync\":{}}}",
        serde_json::to_string(&ids).expect("ids serialize"),
        array_hash(view.raw_units()),
        bytes_hash(view.seen()),
        bytes_hash(view.explored()),
        view.cells().len(),
        array_hash(view.cells().values()),
        view.needs_resync()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn canonical_hash_normalizes_numbers_and_object_order() {
        let a: Value = serde_json::from_str(r#"[{"x":-0.0,"a":[1,true,null]}]"#).unwrap();
        let b: Value = serde_json::from_str(r#"[{"a":[1.0,true,null],"x":0}]"#).unwrap();
        let mut ha = BASIS;
        let mut hb = BASIS;
        value_hash(&mut ha, &a);
        value_hash(&mut hb, &b);
        assert_eq!(ha, hb);
        assert_eq!(bytes_hash(b""), BASIS);
    }
}
