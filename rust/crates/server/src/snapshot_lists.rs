//! Shared ordinary-room list versions and recipient-local World list history.
//! Role filtering and snapshot production happen before this delivery layer.

use crate::snapshot_delta::{DeltaError, UnitDeltaLedger};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TrimError {
    InvalidSnapshot,
    InvalidList,
    VersionExhausted,
    Units(DeltaError),
}
impl std::fmt::Display for TrimError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidSnapshot => f.write_str("snapshot needs an object and unit rows"),
            Self::InvalidList => f.write_str("snapshot list is invalid or exceeds limits"),
            Self::VersionExhausted => f.write_str("snapshot list version capacity exhausted"),
            Self::Units(error) => error.fmt(f),
        }
    }
}
impl std::error::Error for TrimError {}
impl From<DeltaError> for TrimError {
    fn from(error: DeltaError) -> Self {
        Self::Units(error)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ListVersions {
    pub wrecks: u64,
    pub nodes: u64,
}

/// One raw-list cache per room. Batch identity denotes one immutable send cache.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RoomListCache {
    batch: Option<u64>,
    values: [Option<Value>; 2],
    versions: ListVersions,
}
impl RoomListCache {
    pub fn capture(
        &mut self,
        batch: u64,
        wrecks: Option<&Value>,
        nodes: Option<&Value>,
    ) -> Result<ListVersions, TrimError> {
        if self.batch == Some(batch) {
            return Ok(self.versions);
        }
        let values = [list_value(wrecks)?, list_value(nodes)?];
        let mut versions = self.versions;
        for (index, version) in [&mut versions.wrecks, &mut versions.nodes]
            .into_iter()
            .enumerate()
        {
            if self.values[index]
                .as_ref()
                .is_none_or(|old| !same_json(old, &values[index]))
            {
                *version = version.checked_add(1).ok_or(TrimError::VersionExhausted)?;
            }
        }
        self.batch = Some(batch);
        self.values = values.map(Some);
        self.versions = versions;
        Ok(versions)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListScope {
    Shared(ListVersions),
    Personal,
}
#[derive(Clone, Debug, PartialEq)]
enum Receipt {
    Version(u64),
    Personal(Value),
}

/// One state per source stream: seats/operatives, shared ordinary watchers, or World watchers.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SnapshotDelivery {
    units: UnitDeltaLedger,
    lists: [Option<Receipt>; 2],
}
impl SnapshotDelivery {
    pub fn reset(&mut self) {
        self.units.reset();
        self.lists = [None, None];
    }
    pub fn request_full(&mut self) {
        self.units.request_full();
    }

    /// Own the outgoing object without cloning all unchanged unit rows.
    /// Lists and unit rows are preflighted before changing recipient history.
    pub fn trim(&mut self, scope: ListScope, message: Value) -> Result<Value, TrimError> {
        let Value::Object(mut message) = message else {
            return Err(TrimError::InvalidSnapshot);
        };
        let rows = message
            .remove("units")
            .and_then(|value| match value {
                Value::Array(rows) => Some(rows),
                _ => None,
            })
            .ok_or(TrimError::InvalidSnapshot)?;
        let values = [
            list_value(message.get("wrecks"))?,
            list_value(message.get("nodes"))?,
        ];
        let receipts = match scope {
            ListScope::Shared(versions) => [
                Receipt::Version(versions.wrecks),
                Receipt::Version(versions.nodes),
            ],
            ListScope::Personal => values.map(Receipt::Personal),
        };
        let full = self.units.needs_full();
        let keep: [bool; 2] = std::array::from_fn(|index| {
            full || self.lists[index]
                .as_ref()
                .is_none_or(|old| !same_receipt(old, &receipts[index]))
        });
        let delta = self.units.apply(&rows)?;
        let Value::Object(delta) = delta.wire() else {
            unreachable!("unit delta wire is an object");
        };
        // Source Object.assign overwrites these with undefined when they are quiet.
        message.remove("gone");
        message.remove("all");
        message.extend(delta);
        for (index, key) in ["wrecks", "nodes"].into_iter().enumerate() {
            if !keep[index] {
                message.remove(key);
            }
        }
        self.lists = receipts.map(Some);
        Ok(Value::Object(message))
    }
}

fn same_receipt(a: &Receipt, b: &Receipt) -> bool {
    match (a, b) {
        (Receipt::Version(a), Receipt::Version(b)) => a == b,
        (Receipt::Personal(a), Receipt::Personal(b)) => same_json(a, b),
        _ => false,
    }
}
fn same_json(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => a.as_f64() == b.as_f64(),
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| same_json(a, b))
        }
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len()
                && a.iter()
                    .all(|(key, a)| b.get(key).is_some_and(|b| same_json(a, b)))
        }
        _ => a == b,
    }
}
fn list_value(value: Option<&Value>) -> Result<Value, TrimError> {
    let Some(value) = value else {
        return Ok(Value::Null);
    };
    if !value.is_null()
        && value
            .as_array()
            .is_none_or(|rows| rows.len() > ww2_core::client_view::MAX_UNITS)
    {
        return Err(TrimError::InvalidList);
    }
    let mut stack = vec![(value, 0_usize)];
    let mut visited = 0;
    while let Some((value, depth)) = stack.pop() {
        visited += 1;
        if depth > 128 || visited > ww2_core::client_view::MAX_CELLS {
            return Err(TrimError::InvalidList);
        }
        match value {
            Value::Array(values) => stack.extend(values.iter().map(|v| (v, depth + 1))),
            Value::Object(values) => stack.extend(values.values().map(|v| (v, depth + 1))),
            Value::Number(n) if !n.as_f64().is_some_and(f64::is_finite) => {
                return Err(TrimError::InvalidList);
            }
            _ => {}
        }
    }
    Ok(value.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::BTreeMap;

    #[test]
    fn original_trimmed_traces_preserve_source_stream_history() {
        let fixture: Value =
            serde_json::from_str(include_str!("../tests/fixtures/snapshot-lists.json")).unwrap();
        for trace in fixture["traces"].as_array().unwrap() {
            let mut room = RoomListCache::default();
            let mut recipients = BTreeMap::<String, SnapshotDelivery>::new();
            for step in trace["steps"].as_array().unwrap() {
                let recipient = recipients
                    .entry(step["recipient"].as_str().unwrap().into())
                    .or_default();
                if step["reset"] == true {
                    recipient.reset();
                }
                if step["full"] == true {
                    recipient.request_full();
                }
                let scope = if step["mode"] == "world" {
                    ListScope::Personal
                } else {
                    ListScope::Shared(
                        room.capture(
                            step["cacheId"].as_u64().unwrap(),
                            step["cache"].get("wrecks"),
                            step["cache"].get("nodes"),
                        )
                        .unwrap(),
                    )
                };
                let output = recipient.trim(scope, step["message"].clone()).unwrap();
                assert_eq!(output, step["expected"], "{}", trace["name"]);
            }
        }
    }

    #[test]
    fn invalid_frame_preserves_lists_units_and_full_request() {
        let mut delivery = SnapshotDelivery::default();
        let row = json!([1, "rifle", 0, 1, 2, 0, 0, 100, 0, 0, 0, 0, 0, 0, 1]);
        let frame = json!({"t":"s", "units":[row], "wrecks":[], "nodes":[]});
        delivery.trim(ListScope::Personal, frame.clone()).unwrap();
        delivery.request_full();
        let before = delivery.clone();
        for bad in [
            json!({"units":[row], "wrecks":{}}),
            json!({"units":[row, [2]], "nodes":[]}),
        ] {
            assert!(delivery.trim(ListScope::Personal, bad).is_err());
            assert_eq!(delivery, before);
        }
        let resent = delivery.trim(ListScope::Personal, frame).unwrap();
        assert_eq!(resent["all"], true);
        assert!(resent.get("wrecks").is_some() && resent.get("nodes").is_some());
    }

    #[test]
    fn version_overflow_is_atomic_and_content_equality_ignores_number_spelling() {
        let mut room = RoomListCache::default();
        let values = json!([[1, 0]]);
        let versions = room.capture(1, Some(&values), None).unwrap();
        let equivalent: Value = serde_json::from_str("[[1.0,-0.0]]").unwrap();
        assert_eq!(room.capture(2, Some(&equivalent), None).unwrap(), versions);
        room.versions.nodes = u64::MAX;
        let before = room.clone();
        assert_eq!(
            room.capture(3, Some(&json!([[2]])), Some(&json!([]))),
            Err(TrimError::VersionExhausted)
        );
        assert_eq!(room, before);
    }
}
