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

/// The browser stores received cell state in a byte, including any unrecognized bits.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct CellState(u8);
impl CellState {
    pub const fn from_packed(packed: u32) -> Self {
        Self(packed as u8)
    }
    pub const fn packed(self) -> u8 {
        self.0
    }
    pub const fn wear(self) -> u8 {
        self.0 & 3
    }
    pub const fn burnt(self) -> bool {
        self.0 & 4 != 0
    }
    pub const fn damage(self) -> u8 {
        self.0 >> 3 & 3
    }
    pub const fn scar(self) -> u8 {
        self.0 >> 5 & 3
    }
    /// `packState`, followed by the client's Uint8Array conversion.
    pub fn pack(wear: f64, burnt: bool, hp: f64, max: f64, scar: f64) -> Self {
        let worn = (crate::math::min(3.0, (wear * 4.0).floor()) % 256.0) as i16 as u8;
        Self(
            worn | if burnt { 4 } else { 0 }
                | if max > 1.0 && hp < max * 0.67 {
                    if hp < max * 0.34 { 16 } else { 8 }
                } else {
                    0
                }
                | if scar > 0.6 {
                    96
                } else if scar > 0.3 {
                    64
                } else if scar > 0.02 {
                    32
                } else {
                    0
                },
        )
    }
}

/// `startState` packs double-precision wear, before the server's f32 terrain storage.
pub fn start_state(terrain: char, cell: u32) -> CellState {
    let mut h = cell.wrapping_add(1).wrapping_mul(0x9e37_79b1);
    h ^= h >> 15;
    h = h.wrapping_mul(0x85eb_ca6b);
    let noise = f64::from(h ^ (h >> 13)) / 4_294_967_296.0;
    let wear = match terrain {
        'M' | 'F' => 0.2 + 0.6 * noise,
        '+' => 0.3 + 0.3 * noise,
        _ => 0.0,
    };
    CellState::pack(wear, false, 1.0, 1.0, 0.0)
}

/// A cell composed only from the received map and subsequent cell rows.
/// Layers are unavailable when the start supplied no usable baseline or cell data.
#[derive(Debug, Clone, PartialEq)]
pub struct ClientCell<'a> {
    pub terrain: char,
    pub ground: Option<char>,
    pub object: Option<char>,
    pub mine: bool,
    pub physical: Option<char>,
    pub level: f64,
    pub state: CellState,
    /// Last non-null received data object, not an authoritative map record.
    pub data: Option<&'a Value>,
}

#[derive(Debug, Default, Clone, PartialEq)]
struct CellMemory {
    level: Option<f64>,
    data: Option<Value>,
}

/// Units are sorted by id; terrain is sorted by cell index. Raw fields retain wire values.
#[derive(Debug, Clone, PartialEq)]
pub struct ClientView {
    units: BTreeMap<u32, UnitRow>,
    cells: BTreeMap<u32, Value>,
    cell_memory: BTreeMap<u32, CellMemory>,
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
            cell_memory: BTreeMap::new(),
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
    /// Compose current recipient terrain without changing the raw received-row cache.
    /// A dimensions-only start remains valid, but cannot supply an unmentioned cell.
    pub fn cell(&self, index: u32) -> Option<ClientCell<'_>> {
        if u64::from(index) >= u64::from(self.width) * u64::from(self.height) {
            return None;
        }
        let map = self.start.as_ref()?.get("map")?;
        let original = map_character(map.get("rows"), self.width, index);
        let row = self.cells.get(&index).and_then(Value::as_array);
        let terrain = row.and_then(|row| character(row.get(1))).or(original)?;
        let layered = map.get("worldVersion").and_then(Value::as_f64) == Some(2.0)
            && map.get("layers").is_some_and(json_truthy);
        let (mut ground, mut object, mut mine) = if layered {
            let layers = &map["layers"];
            let authored = map_character(layers.get("objects"), self.width, index);
            (
                map_character(layers.get("ground"), self.width, index),
                authored.map(|ch| if ch == 'N' { '.' } else { ch }),
                authored == Some('N')
                    || map_character(layers.get("mines"), self.width, index) == Some('N'),
            )
        } else {
            (
                original.map(|ch| match ch {
                    '.' | 'D' | '+' | 'M' | 'W' | 'F' => ch,
                    '=' => 'W',
                    _ => '.',
                }),
                original.map(|ch| match ch {
                    '.' | 'D' | '+' | 'M' | 'W' | 'F' | 'N' => '.',
                    _ => ch,
                }),
                original == Some('N'),
            )
        };
        let memory = self.cell_memory.get(&index);
        let data = memory.and_then(|memory| memory.data.as_ref());
        if let Some(data) = data {
            ground = character(data.get("ground"));
            object = character(data.get("object"));
        }
        if row.is_some() {
            mine = terrain == 'N';
        }
        let physical = if row.is_some() && !mine {
            Some(terrain)
        } else {
            object.filter(|ch| *ch != '.').or(ground)
        };
        let level = memory.and_then(|memory| memory.level).unwrap_or_else(|| {
            map_character(map.get("heights"), self.width, index)
                .map(|ch| {
                    if ch >= 'a' {
                        96.0 - f64::from(ch as u32)
                    } else {
                        ch.to_digit(10).map_or(0.0, f64::from)
                    }
                })
                .unwrap_or(0.0)
        });
        let state = row.map_or_else(
            || start_state(terrain, index),
            |row| {
                row.get(3)
                    .and_then(Value::as_f64)
                    .map_or(CellState::default(), |state| {
                        CellState::from_packed(state as u32)
                    })
            },
        );
        Some(ClientCell {
            terrain,
            ground,
            object,
            mine,
            physical,
            level,
            state,
            data,
        })
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
    /// Last received lobby message, not a derived match lifecycle state.
    pub fn lobby(&self) -> Option<&Value> {
        self.lobby.as_ref()
    }
    /// Last received pause message. A lobby transition can clear the browser's effective pause
    /// without replacing this raw message; presentation must account for the lobby state.
    pub fn pause(&self) -> Option<&Value> {
        self.pause.as_ref()
    }
    /// Last received denial, retained until a later denial replaces it.
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
                self.merge_cells(patch.cells, value.get("cells"));
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
        next.merge_cells(cells, value.get("cells"));
        next.lobby = self.lobby.take();
        next.pause = self.pause.take();
        next.last_denial = self.last_denial.take();
        next.start = Some(value);
        *self = next;
        Ok(Applied::Start)
    }
    fn merge_cells(&mut self, cells: BTreeMap<u32, Value>, received: Option<&Value>) {
        // Optional height and layers survive short rows, including duplicates in one update.
        // Raw rows still replace the cache exactly as received.
        if let Some(rows) = received.and_then(Value::as_array) {
            for raw in rows {
                let row = raw.as_array().expect("validated cell row");
                let level = row.get(2).and_then(Value::as_f64);
                let data = row.get(4).filter(|v| !v.is_null());
                if level.is_none() && data.is_none() {
                    continue;
                }
                let index = row[0].as_f64().expect("validated cell index") as u32;
                let memory = self.cell_memory.entry(index).or_default();
                if let Some(level) = level {
                    memory.level = Some(level);
                }
                if let Some(data) = data {
                    memory.data = Some(data.clone());
                }
            }
        }
        self.changed.extend(cells.keys());
        self.changed.sort_unstable();
        self.changed.dedup();
        self.cells.extend(cells);
    }
}

fn character(value: Option<&Value>) -> Option<char> {
    let mut chars = value?.as_str()?.chars();
    let ch = chars.next()?;
    chars.next().is_none().then_some(ch)
}
fn map_character(rows: Option<&Value>, width: u32, index: u32) -> Option<char> {
    rows?
        .get((index / width) as usize)?
        .as_str()?
        .chars()
        .nth((index % width) as usize)
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
