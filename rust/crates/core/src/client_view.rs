//! Recipient-owned state decoded from the pinned server's JSON messages.
//!
//! The caller owns transport and resync timing. No hidden game state is available here.

use std::collections::BTreeMap;
use std::fmt;

use serde_json::Value;

/// Resource limits apply before state changes. Server snapshots can exceed client command limits.
pub const MAX_MESSAGE_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_CELLS: usize = 4 * 1024 * 1024;
pub const MAX_UNITS: usize = 100_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViewError {
    Parse(String),
    Invalid(String),
    WrongState(&'static str),
    Limit(&'static str),
}
impl fmt::Display for ViewError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(message) | Self::Invalid(message) => message.fmt(f),
            Self::WrongState(message) | Self::Limit(message) => message.fmt(f),
        }
    }
}
impl std::error::Error for ViewError {}
type Result<T> = std::result::Result<T, ViewError>;
fn error(message: impl Into<String>) -> ViewError {
    ViewError::Invalid(message.into())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Applied {
    Start,
    Snapshot,
    Lobby,
    Pause,
    Denial,
    Ignored,
}

/// Fixed columns from UNIT_ROW. The complete received row, including its tail, stays available.
#[derive(Debug, Clone, PartialEq)]
pub struct UnitRow {
    pub id: u32,
    pub kind: String,
    pub owner: i32,
    pub x: f64,
    pub z: f64,
    pub rot: f64,
    pub aim: f64,
    pub hp: f64,
    pub supp: f64,
    pub target: u32,
    pub cover: u32,
    pub cd: f64,
    pub flags: u32,
    pub stars: u32,
    pub built: f64,
    raw: Value,
}
impl UnitRow {
    pub fn raw(&self) -> &Value {
        &self.raw
    }
    fn parse(raw: &Value) -> Result<Self> {
        let a = raw
            .as_array()
            .filter(|a| a.len() >= 15)
            .ok_or_else(|| error("unit row needs at least 15 columns"))?;
        let number = |i: usize| {
            a[i].as_f64()
                .filter(|n| n.is_finite())
                .ok_or_else(|| error(format!("unit column {i} must be a finite number")))
        };
        Ok(Self {
            id: uint(&a[0], "unit id")?,
            kind: a[1]
                .as_str()
                .ok_or_else(|| error("unit kind must be a string"))?
                .into(),
            owner: a[2]
                .as_f64()
                .filter(|n| n.fract() == 0.0 && *n >= i32::MIN as f64 && *n <= i32::MAX as f64)
                .map(|n| n as i32)
                .ok_or_else(|| error("unit owner must fit i32"))?,
            x: number(3)?,
            z: number(4)?,
            rot: number(5)?,
            aim: number(6)?,
            hp: number(7)?,
            supp: number(8)?,
            target: uint(&a[9], "unit target")?,
            cover: uint(&a[10], "unit cover")?,
            cd: number(11)?,
            flags: uint(&a[12], "unit flags")?,
            stars: uint(&a[13], "unit stars")?,
            built: number(14)?,
            raw: raw.clone(),
        })
    }
}

/// Units are sorted by id; terrain is sorted by cell index. Raw fields retain wire values.
#[derive(Debug, Clone, PartialEq)]
pub struct ClientView {
    units: BTreeMap<u32, UnitRow>,
    cells: BTreeMap<u32, Value>,
    changed: Vec<u32>,
    seen: Vec<u8>,
    explored: Vec<u8>,
    width: u32,
    height: u32,
    start: Option<Value>,
    state: Value,
    lobby: Option<Value>,
    pause: Option<Value>,
    last_denial: Option<Value>,
    resync: bool,
}
impl Default for ClientView {
    fn default() -> Self {
        Self {
            units: BTreeMap::new(),
            cells: BTreeMap::new(),
            changed: Vec::new(),
            seen: Vec::new(),
            explored: Vec::new(),
            width: 0,
            height: 0,
            start: None,
            state: Value::Null,
            lobby: None,
            pause: None,
            last_denial: None,
            resync: false,
        }
    }
}
impl ClientView {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn units(&self) -> impl ExactSizeIterator<Item = &UnitRow> {
        self.units.values()
    }
    pub fn raw_units(&self) -> impl ExactSizeIterator<Item = &Value> {
        self.units.values().map(UnitRow::raw)
    }
    pub fn unit(&self, id: u32) -> Option<&UnitRow> {
        self.units.get(&id)
    }
    pub fn unit_count(&self) -> usize {
        self.units.len()
    }
    pub fn seen(&self) -> &[u8] {
        &self.seen
    }
    pub fn explored(&self) -> &[u8] {
        &self.explored
    }
    pub fn width(&self) -> u32 {
        self.width
    }
    pub fn height(&self) -> u32 {
        self.height
    }
    pub fn cells(&self) -> &BTreeMap<u32, Value> {
        &self.cells
    }
    /// Sorted, unique cells received since the last drain, including the start's full memory.
    pub fn changed_cells(&self) -> &[u32] {
        &self.changed
    }
    pub fn take_changed_cells(&mut self) -> Vec<u32> {
        std::mem::take(&mut self.changed)
    }
    pub fn start(&self) -> Option<&Value> {
        self.start.as_ref()
    }
    pub fn lobby(&self) -> Option<&Value> {
        self.lobby.as_ref()
    }
    pub fn pause(&self) -> Option<&Value> {
        self.pause.as_ref()
    }
    pub fn last_denial(&self) -> Option<&Value> {
        self.last_denial.as_ref()
    }
    /// Whether the start supplied a fog mask. Raw zero masks do not imply hidden ground.
    pub fn has_fog_mask(&self) -> bool {
        self.start
            .as_ref()
            .and_then(|v| v.get("fog"))
            .is_some_and(|v| !v.is_null())
    }
    /// Initial missing-mask behavior: ordinary-map fog is lifted, World maps stay opaque.
    /// Renderer-only lifting at match end is not tracked here; the raw masks stay unchanged.
    pub fn fog_lifted(&self) -> bool {
        !self.has_fog_mask()
            && !self
                .start
                .as_ref()
                .and_then(|v| v.get("map"))
                .and_then(|v| v.get("world"))
                .is_some_and(json_truthy)
    }
    /// Last snapshot, completed with all held unit rows and carried wrecks and nodes.
    pub fn state(&self) -> &Value {
        &self.state
    }
    /// The latest snapshot's held-set comparison. Only the transport decides when to request resync.
    pub fn needs_resync(&self) -> bool {
        self.resync
    }

    /// Parse and validate the entire update before changing any state.
    /// Unknown message types leave the view and pending changed cells untouched.
    pub fn apply(&mut self, message: &str) -> Result<Applied> {
        if message.len() > MAX_MESSAGE_BYTES {
            return Err(ViewError::Limit("message exceeds 16 MiB"));
        }
        let mut value: Value = serde_json::from_str(message)
            .map_err(|e| ViewError::Parse(format!("invalid JSON: {e}")))?;
        let object = value
            .as_object()
            .ok_or_else(|| error("message must be an object"))?;
        match object
            .get("t")
            .and_then(Value::as_str)
            .ok_or_else(|| error("message t must be a string"))?
        {
            "start" => self.apply_start(value),
            "s" => {
                if self.start.is_none() {
                    return Err(ViewError::WrongState("snapshot requires start"));
                }
                let patch = Snapshot::parse(&value, self.seen.len())?;
                let clear = patch.all || patch.held.is_none();
                let retained = if clear {
                    0
                } else {
                    self.units
                        .keys()
                        .filter(|id| {
                            patch.gone.binary_search(id).is_err() && !patch.units.contains_key(id)
                        })
                        .count()
                };
                if retained + patch.units.len() > MAX_UNITS {
                    return Err(ViewError::Limit("held units exceed limit"));
                }
                // All remaining operations are infallible after parsing the patch.
                if clear {
                    self.units.clear();
                }
                for id in patch.gone {
                    self.units.remove(&id);
                }
                self.units.extend(patch.units);
                self.resync = patch.held.is_some_and(|(count, xor)| {
                    self.units.len() != count
                        || self.units.keys().fold(0_u32, |a, id| a ^ id) as i32 != xor
                });
                for (start, end) in patch.fog {
                    for c in start..end {
                        self.seen[c] ^= 1;
                        self.explored[c] |= self.seen[c];
                    }
                }
                self.merge_cells(patch.cells);
                let object = value.as_object_mut().expect("validated object");
                for key in ["wrecks", "nodes"] {
                    if object.get(key).is_none_or(Value::is_null)
                        && let Some(previous) = self.state.get(key)
                    {
                        object.insert(key.into(), previous.clone());
                    }
                }
                // Keep the wire delta too, as the browser's world-state does.
                let sent = object
                    .insert(
                        "units".into(),
                        Value::Array(self.raw_units().cloned().collect()),
                    )
                    .expect("validated units");
                object.insert("sent".into(), sent);
                self.state = value;
                Ok(Applied::Snapshot)
            }
            "lobby" => {
                self.lobby = Some(value);
                Ok(Applied::Lobby)
            }
            "pause" => {
                self.pause = Some(value);
                Ok(Applied::Pause)
            }
            "deny" => {
                self.last_denial = Some(value);
                Ok(Applied::Denial)
            }
            _ => Ok(Applied::Ignored),
        }
    }
    fn apply_start(&mut self, value: Value) -> Result<Applied> {
        let map = value
            .get("map")
            .and_then(Value::as_object)
            .ok_or_else(|| error("start needs map"))?;
        let width = uint(map.get("w").unwrap_or(&Value::Null), "map w")?;
        let height = uint(map.get("h").unwrap_or(&Value::Null), "map h")?;
        let n = u64::from(width) * u64::from(height);
        if width == 0 || height == 0 || n > MAX_CELLS as u64 {
            return Err(ViewError::Limit("map dimensions exceed bounds"));
        }
        let n = n as usize;
        let cells = parse_cells(value.get("cells"), n)?;
        let mut next = Self {
            width,
            height,
            seen: vec![0; n],
            explored: vec![0; n],
            ..Self::default()
        };
        if let Some(fog) = value.get("fog").filter(|v| !v.is_null()) {
            let fog = fog
                .as_object()
                .ok_or_else(|| error("start fog must be an object"))?;
            for (name, mask) in [("v", &mut next.seen), ("e", &mut next.explored)] {
                let runs = fog
                    .get(name)
                    .and_then(Value::as_str)
                    .ok_or_else(|| error(format!("start fog {name} must be a string")))?;
                for (start, end) in parse_runs(runs, n)? {
                    mask[start..end].fill(1);
                }
            }
        }
        next.merge_cells(cells);
        next.lobby = self.lobby.take();
        next.pause = self.pause.take();
        next.last_denial = self.last_denial.take();
        next.start = Some(value);
        *self = next;
        Ok(Applied::Start)
    }
    fn merge_cells(&mut self, cells: BTreeMap<u32, Value>) {
        self.changed.extend(cells.keys());
        self.changed.sort_unstable();
        self.changed.dedup();
        self.cells.extend(cells);
    }
}

struct Snapshot {
    units: BTreeMap<u32, UnitRow>,
    gone: Vec<u32>,
    held: Option<(usize, i32)>,
    all: bool,
    cells: BTreeMap<u32, Value>,
    fog: Vec<(usize, usize)>,
}
impl Snapshot {
    fn parse(value: &Value, n: usize) -> Result<Self> {
        let rows = value
            .get("units")
            .and_then(Value::as_array)
            .ok_or_else(|| error("snapshot units must be an array"))?;
        if rows.len() > MAX_UNITS {
            return Err(ViewError::Limit("unit rows exceed limit"));
        }
        let mut units = BTreeMap::new();
        for raw in rows {
            let row = UnitRow::parse(raw)?;
            units.insert(row.id, row);
        }
        let gone = match value.get("gone").filter(|v| !v.is_null()) {
            None => Vec::new(),
            Some(v) => {
                let a = v
                    .as_array()
                    .filter(|a| a.len() <= MAX_UNITS)
                    .ok_or_else(|| error("gone must be a bounded array"))?;
                let mut ids = a
                    .iter()
                    .map(|v| uint(v, "gone id"))
                    .collect::<Result<Vec<_>>>()?;
                ids.sort_unstable();
                ids.dedup();
                ids
            }
        };
        let held = match value.get("held").filter(|v| !v.is_null()) {
            None => None,
            Some(v) => {
                let a = v
                    .as_array()
                    .filter(|a| a.len() == 2)
                    .ok_or_else(|| error("held needs count and xor"))?;
                let count = uint(&a[0], "held count")? as usize;
                if count > MAX_UNITS {
                    return Err(ViewError::Limit("held count exceeds limit"));
                }
                let xor = a[1]
                    .as_f64()
                    .filter(|n| n.fract() == 0.0 && *n >= i32::MIN as f64 && *n <= i32::MAX as f64)
                    .map(|n| n as i32)
                    .ok_or_else(|| error("held xor must fit i32"))?;
                Some((count, xor))
            }
        };
        let all = match value.get("all") {
            None | Some(Value::Null) => false,
            Some(Value::Bool(v)) => *v,
            _ => return Err(error("all must be boolean")),
        };
        let fog = match value.get("fog") {
            None | Some(Value::Null) => Vec::new(),
            Some(Value::String(s)) => parse_runs(s, n)?,
            _ => return Err(error("snapshot fog must be a string")),
        };
        Ok(Self {
            units,
            gone,
            held,
            all,
            cells: parse_cells(value.get("cells"), n)?,
            fog,
        })
    }
}
fn uint(value: &Value, name: &str) -> Result<u32> {
    value
        .as_f64()
        .filter(|n| n.fract() == 0.0 && *n >= 0.0 && *n <= u32::MAX as f64)
        .map(|n| n as u32)
        .ok_or_else(|| error(format!("{name} must fit u32")))
}
fn parse_cells(value: Option<&Value>, n: usize) -> Result<BTreeMap<u32, Value>> {
    let Some(value) = value.filter(|v| !v.is_null()) else {
        return Ok(BTreeMap::new());
    };
    let rows = value
        .as_array()
        .filter(|a| a.len() <= MAX_CELLS)
        .ok_or_else(|| error("cells must be a bounded array"))?;
    let mut cells = BTreeMap::new();
    for raw in rows {
        let a = raw
            .as_array()
            .filter(|a| a.len() >= 2)
            .ok_or_else(|| error("cell row needs index and terrain"))?;
        let c = uint(&a[0], "cell index")?;
        if c as usize >= n {
            return Err(error("cell index outside map"));
        }
        if a[1].as_str().is_none_or(|s| s.chars().count() != 1) {
            return Err(error("terrain must be one character"));
        }
        if a.get(2)
            .is_some_and(|v| !v.is_null() && v.as_f64().is_none())
        {
            return Err(error("cell level must be numeric"));
        }
        if let Some(v) = a.get(3).filter(|v| !v.is_null()) {
            uint(v, "cell state")?;
        }
        if a.get(4).is_some_and(|v| !v.is_null() && !v.is_object()) {
            return Err(error("cell data must be an object"));
        }
        cells.insert(c, raw.clone());
    }
    Ok(cells)
}
/// Decode bounded base-32 lengths, alternating skipped and set runs from cell zero.
fn parse_runs(text: &str, n: usize) -> Result<Vec<(usize, usize)>> {
    const DIGITS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = Vec::new();
    let (mut c, mut value, mut shift, mut on) = (0_usize, 0_usize, 0_u32, false);
    for b in text.bytes() {
        let d = DIGITS
            .iter()
            .position(|v| *v == b)
            .ok_or_else(|| error("invalid fog digit"))?;
        if shift > 20 {
            return Err(error("fog run exceeds bounds"));
        }
        value = value
            .checked_add((d & 31) << shift)
            .ok_or_else(|| error("fog run overflow"))?;
        if value > n {
            return Err(error("fog run outside map"));
        }
        if d & 32 != 0 {
            shift += 5;
            continue;
        }
        let end = c
            .checked_add(value)
            .filter(|end| *end <= n)
            .ok_or_else(|| error("fog run outside map"))?;
        if on && c < end {
            out.push((c, end));
        }
        c = end;
        value = 0;
        shift = 0;
        on = !on;
    }
    if shift != 0 {
        return Err(error("truncated fog run"));
    }
    Ok(out)
}

fn json_truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::Number(v) => v.as_f64().is_some_and(|n| n != 0.0),
        Value::String(v) => !v.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}
