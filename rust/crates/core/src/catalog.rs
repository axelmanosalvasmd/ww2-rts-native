//! Static rules captured after the reference modules finish initialization.
//!
//! This catalog is separate from the hot-path `rules::Rules` and from the FFI rules hash.
//! It is compiled into the library and parsed once, without runtime file or network access.
//! Object keys have a fixed order; the explicit `UNIT_TYPES` and other arrays keep source order.
use std::{collections::BTreeMap, fmt, sync::OnceLock};

/// A JSON value with IEEE 754 numbers, including the reference's non-finite constants.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Value>),
    Object(BTreeMap<String, Value>),
}

impl Value {
    pub fn get(&self, key: &str) -> Option<&Self> {
        self.object()?.get(key)
    }
    pub fn object(&self) -> Option<&BTreeMap<String, Self>> {
        if let Self::Object(value) = self {
            Some(value)
        } else {
            None
        }
    }
    pub fn array(&self) -> Option<&[Self]> {
        if let Self::Array(value) = self {
            Some(value)
        } else {
            None
        }
    }
    pub fn number(&self) -> Option<f64> {
        if let Self::Number(value) = self {
            Some(*value)
        } else {
            None
        }
    }
    pub fn string(&self) -> Option<&str> {
        if let Self::String(value) = self {
            Some(value)
        } else {
            None
        }
    }
    pub fn boolean(&self) -> Option<bool> {
        if let Self::Bool(value) = self {
            Some(*value)
        } else {
            None
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CatalogError(pub String);
impl fmt::Display for CatalogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
impl std::error::Error for CatalogError {}

/// All captured static tables. Unknown names are errors, never unchecked indexes.
#[derive(Debug)]
pub struct Catalog {
    root: Value,
}

impl Catalog {
    /// Decode the recorder's format. `$number` is reserved for an exact special-number tag.
    pub fn from_json(text: &str) -> Result<Self, CatalogError> {
        let json = serde_json::from_str(text).map_err(|e| CatalogError(e.to_string()))?;
        let root = decode(json)?;
        if root.get("tables").and_then(Value::object).is_none() {
            return Err(CatalogError("catalog has no tables object".into()));
        }
        Ok(Self { root })
    }

    pub fn compiled() -> Result<&'static Self, CatalogError> {
        static CATALOG: OnceLock<Result<Catalog, CatalogError>> = OnceLock::new();
        CATALOG
            .get_or_init(|| Self::from_json(include_str!("../data/rules.json")))
            .as_ref()
            .map_err(Clone::clone)
    }

    pub fn table(&self, name: &str) -> Result<&Value, CatalogError> {
        self.root
            .get("tables")
            .and_then(|t| t.get(name))
            .ok_or_else(|| CatalogError(format!("unknown rules table: {name}")))
    }

    pub fn unit(&self, name: &str) -> Result<&Value, CatalogError> {
        self.table("UNITS")?
            .get(name)
            .ok_or_else(|| CatalogError(format!("unknown unit type: {name}")))
    }

    pub fn support(&self, name: &str) -> Result<&Value, CatalogError> {
        self.table("SUPPORT")?
            .get(name)
            .ok_or_else(|| CatalogError(format!("unknown support type: {name}")))
    }

    pub fn reference(&self) -> &str {
        self.root
            .get("reference")
            .and_then(Value::string)
            .unwrap_or("")
    }
}

fn decode(json: serde_json::Value) -> Result<Value, CatalogError> {
    use serde_json::Value as J;
    Ok(match json {
        J::Null => Value::Null,
        J::Bool(b) => Value::Bool(b),
        J::Number(n) => Value::Number(
            n.as_f64()
                .ok_or_else(|| CatalogError("invalid number".into()))?,
        ),
        J::String(s) => Value::String(s),
        J::Array(a) => Value::Array(a.into_iter().map(decode).collect::<Result<_, _>>()?),
        J::Object(mut o) if o.contains_key("$number") => {
            if o.len() != 1 {
                return Err(CatalogError("number tag has extra keys".into()));
            }
            let n = match o
                .remove("$number")
                .and_then(|s| s.as_str().map(str::to_owned))
                .as_deref()
            {
                Some("Infinity") => f64::INFINITY,
                Some("-Infinity") => f64::NEG_INFINITY,
                Some("NaN") => f64::NAN,
                Some("-0") => -0.0,
                _ => return Err(CatalogError("invalid special-number tag".into())),
            };
            Value::Number(n)
        }
        J::Object(o) => Value::Object(
            o.into_iter()
                .map(|(k, v)| Ok((k, decode(v)?)))
                .collect::<Result<_, CatalogError>>()?,
        ),
    })
}
