//! Recipient-local unit deltas for the existing JSON snapshot format.
//! Input rows must already be filtered by the authoritative recipient producer.

use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

const MAX_UNITS: usize = ww2_core::client_view::MAX_UNITS;
const MAX_COLUMNS: usize = 128;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeltaError {
    TooManyUnits,
    InvalidRow,
    DuplicateUnit,
}
impl std::fmt::Display for DeltaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::TooManyUnits => "snapshot unit limit exceeded",
            Self::InvalidRow => "snapshot unit row is invalid",
            Self::DuplicateUnit => "snapshot contains duplicate unit ids",
        })
    }
}
impl std::error::Error for DeltaError {}

#[derive(Clone, Debug, PartialEq)]
pub struct UnitDelta {
    pub units: Vec<Value>,
    pub gone: Vec<u32>,
    pub all: bool,
    pub held_count: usize,
    /// JavaScript's bitwise XOR returns a signed 32-bit number.
    pub held_xor: i32,
}
impl UnitDelta {
    /// Omit optional fields exactly as JSON.stringify omits source undefined values.
    pub fn wire(&self) -> Value {
        let mut result = json!({
            "units": self.units,
            "held": [self.held_count, self.held_xor],
        });
        if !self.gone.is_empty() {
            result["gone"] = json!(self.gone);
        }
        if self.all {
            result["all"] = Value::Bool(true);
        }
        result
    }
}

/// One ledger per recipient stream, including a separate stream for each World spectator.
/// Reset on a new start/reconnect; request_full retains the current state until a valid frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UnitDeltaLedger {
    sent: BTreeMap<u32, Vec<Value>>,
    order: Vec<u32>,
    full: bool,
}
impl UnitDeltaLedger {
    pub fn reset(&mut self) {
        self.sent.clear();
        self.order.clear();
        self.full = false;
    }
    pub fn request_full(&mut self) {
        self.full = true;
    }
    pub fn needs_full(&self) -> bool {
        self.full
    }

    /// Validate the whole frame before changing held state or consuming a full request.
    pub fn apply(&mut self, rows: &[Value]) -> Result<UnitDelta, DeltaError> {
        if rows.len() > MAX_UNITS {
            return Err(DeltaError::TooManyUnits);
        }
        let mut seen = BTreeSet::new();
        let mut admitted = Vec::with_capacity(rows.len());
        for value in rows {
            let row = value
                .as_array()
                .filter(|row| (15..=MAX_COLUMNS).contains(&row.len()))
                .ok_or(DeltaError::InvalidRow)?;
            if row.iter().any(|value| match value {
                Value::Null | Value::Bool(_) | Value::String(_) => false,
                Value::Number(n) => !n.as_f64().is_some_and(f64::is_finite),
                Value::Array(_) | Value::Object(_) => true,
            }) || !row[1].is_string()
            {
                return Err(DeltaError::InvalidRow);
            }
            let id = row[0]
                .as_f64()
                .filter(|n| n.is_finite() && *n >= 1.0 && *n <= u32::MAX as f64 && n.fract() == 0.0)
                .ok_or(DeltaError::InvalidRow)? as u32;
            if !seen.insert(id) {
                return Err(DeltaError::DuplicateUnit);
            }
            admitted.push((id, row));
        }

        let gone: Vec<_> = self
            .order
            .iter()
            .copied()
            .filter(|id| !seen.contains(id))
            .collect();
        let mut units = Vec::new();
        for (id, row) in admitted {
            let changed = self.full
                || self.sent.get(&id).is_none_or(|old| {
                    row.iter().enumerate().any(|(index, value)| {
                        old.get(index).is_none_or(|old| !same_scalar(old, value))
                    })
                });
            if changed {
                if !self.sent.contains_key(&id) {
                    self.order.push(id);
                }
                self.sent.insert(id, row.clone());
                units.push(Value::Array(row.clone()));
            }
        }
        for id in &gone {
            self.sent.remove(id);
        }
        self.order.retain(|id| seen.contains(id));
        let result = UnitDelta {
            units,
            gone,
            all: self.full,
            held_count: self.sent.len(),
            held_xor: self.sent.keys().fold(0_u32, |xor, id| xor ^ id) as i32,
        };
        self.full = false;
        Ok(result)
    }
}

fn same_scalar(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => a.as_f64() == b.as_f64(),
        _ => a == b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn original_source_traces_preserve_recipient_delta_history() {
        let fixture: Value =
            serde_json::from_str(include_str!("../tests/fixtures/snapshot-delta.json")).unwrap();
        for trace in fixture["traces"].as_array().unwrap() {
            let mut ledger = UnitDeltaLedger::default();
            for frame in trace["frames"].as_array().unwrap() {
                if frame["reset"] == true {
                    ledger.reset();
                }
                if frame["full"] == true {
                    ledger.request_full();
                }
                let delta = ledger.apply(frame["rows"].as_array().unwrap()).unwrap();
                assert_eq!(delta.wire(), frame["expected"], "{}", trace["label"]);
            }
        }
    }

    #[test]
    fn malformed_frame_does_not_change_held_rows_or_consume_resync() {
        let row = json!([1, "rifle", 0, 1, 2, 0, 0, 100, 0, 0, 0, 0, 0, 0, 1]);
        let mut ledger = UnitDeltaLedger::default();
        ledger.apply(std::slice::from_ref(&row)).unwrap();
        ledger.request_full();
        let saved = ledger.clone();
        let mut changed = row.clone();
        changed[3] = json!(99);
        for rows in [
            vec![changed.clone(), json!([2])],
            vec![changed.clone(), changed.clone()],
            vec![json!([
                2,
                "rifle",
                0,
                {},
                2,
                0,
                0,
                100,
                0,
                0,
                0,
                0,
                0,
                0,
                1
            ])],
        ] {
            assert!(ledger.apply(&rows).is_err());
            assert_eq!(ledger, saved);
        }
        let delta = ledger.apply(&[changed]).unwrap();
        assert!(delta.all);
        assert_eq!(delta.units.len(), 1);
        assert!(!ledger.needs_full());
    }
}
