//! Authoritative ordinary and generated World maps, plus pure received-map presentation helpers.
//!
//! `AuthoritativeMap` is not a client-map importer or a match constructor. Client projections erase
//! mines, triggers and scenarios irreversibly. Its callers must supply the authoritative document;
//! the format has no provenance marker that could distinguish a stripped projection.
//! Authored scenarios are validated here; their execution remains guarded at match admission.
//! Unknown authoring properties stay in `source()` for later systems.
use std::{collections::BTreeMap, fmt};

use serde_json::Value;

use crate::{catalog::Catalog, game::*, math, rules::Rules};

const BASE: &[u8] = b".D+MWF";
const OBJECT: &[u8] = b".BH#TXY=RNO";
const CELLS: &[u8] = b".BH#+TWF=RXYDMNOQ";
const MATERIALS: [&str; 7] = [
    "soil", "road", "wood", "stone", "concrete", "steel", "water",
];
/// Maximum document size before JSON parsing or terrain allocation.
pub const MAX_MAP_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MapError {
    Invalid { field: String, reason: String },
    Unsupported { feature: &'static str },
    Catalog(String),
}
impl fmt::Display for MapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid { field, .. } if field == "name" => {
                f.write_str("name must be a string up to 40 chars")
            }
            Self::Invalid { field, .. } if field == "w" || field == "h" => {
                f.write_str("size must be 20-1024 cells")
            }
            Self::Invalid { field, .. } if field == "defend" => {
                f.write_str("defend must list some (not all) spawn numbers")
            }
            Self::Invalid { field, .. } if field == "assaultTime" => {
                f.write_str("assaultTime must be 300-3600 seconds")
            }
            Self::Invalid { field, reason }
                if field == "rows" || field == "heights" || field == "world.validation" =>
            {
                f.write_str(reason)
            }
            Self::Invalid { field, reason } if field == "structures" => write!(f, "{reason}"),
            Self::Invalid { reason, .. } if reason == "needs two non-assault spawns" => {
                f.write_str("needs 2+ spawns that every mode can use")
            }
            Self::Invalid { field, reason } => write!(f, "{field}: {reason}"),
            Self::Unsupported { feature } => {
                write!(f, "unsupported authoritative map feature: {feature}")
            }
            Self::Catalog(reason) => write!(f, "map rules: {reason}"),
        }
    }
}
impl std::error::Error for MapError {}
type Result<T> = std::result::Result<T, MapError>;
fn invalid(field: &str, reason: &str) -> MapError {
    MapError::Invalid {
        field: field.into(),
        reason: reason.into(),
    }
}
fn array<'a>(v: &'a Value, field: &str) -> Result<&'a [Value]> {
    v.as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| invalid(field, "expected an array"))
}
fn integer(v: &Value, lo: usize, hi: usize, field: &str) -> Result<usize> {
    v.as_f64()
        .filter(|n| n.is_finite() && n.fract() == 0.0 && *n >= lo as f64 && *n <= hi as f64)
        .map(|n| n as usize)
        .ok_or_else(|| invalid(field, "integer is outside the allowed range"))
}
fn number(v: &Value, lo: f64, hi: f64, field: &str) -> Result<f64> {
    v.as_f64()
        .filter(|n| n.is_finite() && *n >= lo && *n <= hi)
        .ok_or_else(|| invalid(field, "number is outside the allowed range"))
}
fn optional_bool(v: &Value, key: &str) -> Result<bool> {
    match v.get(key) {
        None => Ok(false),
        Some(x) => x
            .as_bool()
            .ok_or_else(|| invalid(key, "expected a boolean")),
    }
}
fn rows(v: &Value, w: usize, h: usize, allowed: &[u8], field: &str) -> Result<Vec<u8>> {
    let error = |shape: bool| {
        let reason = match field {
            "rows" if shape => "row count must equal height".to_owned(),
            "rows" => format!("rows must be {w} valid terrain chars"),
            "heights" => format!("heights must be {h} rows of 0-4 (a/b for depths -1/-2)"),
            _ if shape => "row count must equal height".to_owned(),
            _ => "rows must have the declared width and valid cells".to_owned(),
        };
        invalid(field, &reason)
    };
    let list = array(v, field).map_err(|original| {
        if field == "rows" || field == "heights" {
            error(true)
        } else {
            original
        }
    })?;
    if list.len() != h {
        return Err(error(true));
    }
    let mut out = Vec::with_capacity(w * h);
    for row in list {
        let row = row.as_str().ok_or_else(|| {
            if field == "rows" || field == "heights" {
                error(false)
            } else {
                invalid(field, "expected text rows")
            }
        })?;
        if row.len() != w || !row.bytes().all(|c| allowed.contains(&c)) {
            return Err(error(false));
        }
        out.extend_from_slice(row.as_bytes());
    }
    Ok(out)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Spawn {
    pub x: usize,
    pub y: usize,
    pub assault: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub struct CapturePoint {
    pub x: usize,
    pub y: usize,
    pub vp: f64,
    pub mp: f64,
    pub kind: Option<String>,
    pub needs: Option<usize>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Building {
    pub x: usize,
    pub y: usize,
    pub kind: String,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Trigger {
    pub at: f64,
    pub say: Option<String>,
    pub blow: Option<[usize; 4]>,
}

/// Normalized version-2 layers, including hidden mines and all material overrides.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layers {
    pub ground: Vec<u8>,
    pub objects: Vec<u8>,
    pub mines: Vec<u8>,
    pub materials: BTreeMap<usize, String>,
    pub base_materials: BTreeMap<usize, String>,
    pub object_materials: BTreeMap<usize, String>,
    pub mine_materials: BTreeMap<usize, String>,
}
#[derive(Clone, Debug)]
pub struct AuthoritativeMap {
    source: Value,
    name: String,
    w: usize,
    h: usize,
    chars: Vec<u8>,
    height: Option<Vec<i8>>,
    layers: Layers,
    spawns: Vec<Spawn>,
    points: Vec<CapturePoint>,
    buildings: Vec<Building>,
    triggers: Vec<Trigger>,
    defend: Option<Vec<usize>>,
    assault_time: Option<usize>,
    naval: bool,
    trench_facing: bool,
    structures: Vec<crate::structures::StructureDefinition>,
    geography: Option<crate::world_map::WorldDefinition>,
}
impl AuthoritativeMap {
    /// Validate original server authoring JSON. Never pass a `mapForClient` projection here.
    /// JSON decoding, ordinary-map validation and normalization are all in memory.
    pub fn from_json(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > MAX_MAP_BYTES {
            return Err(invalid("document", "exceeds 16 MiB"));
        }
        let source: Value =
            serde_json::from_slice(bytes).map_err(|e| invalid("document", &e.to_string()))?;
        Self::from_value(source)
    }
    pub(crate) fn from_value(source: Value) -> Result<Self> {
        Self::from_value_mode(source, false)
    }
    pub(crate) fn from_editor_value(source: Value) -> Result<Self> {
        Self::from_value_mode(source, true)
    }
    fn from_value_mode(source: Value, editor: bool) -> Result<Self> {
        if !source.is_object() {
            return Err(invalid("document", "expected a map object"));
        }
        let name = source["name"]
            .as_str()
            .filter(|s| s.encode_utf16().count() <= 40)
            .ok_or_else(|| invalid("name", "expected text up to 40 UTF-16 code units"))?
            .to_owned();
        let w = integer(&source["w"], 20, 1024, "w")?;
        let h = integer(&source["h"], 20, 1024, "h")?;
        let chars = rows(&source["rows"], w, h, CELLS, "rows")?;
        let height = source
            .get("heights")
            .map(|v| {
                rows(v, w, h, b"01234ab", "heights").map(|r| {
                    r.into_iter()
                        .map(|c| level_of(c).expect("validated height character"))
                        .collect()
                })
            })
            .transpose()?;
        let layers = normalize_layers(&source, w, h, &chars, true)?;
        let structures = crate::structures::parse(source.get("structures"), w, h, &layers)?;
        let is_world = if editor {
            source.get("world").is_some_and(crate::map_save::truthy)
        } else {
            source.get("world").is_some()
        };
        let minimum_spawns = if is_world { 1 } else { 2 };
        let pos = |v: &Value, field: &str| -> Result<(usize, usize)> {
            Ok((
                integer(&v["x"], 0, w - 1, field)?,
                integer(&v["y"], 0, h - 1, field)?,
            ))
        };
        let open_pos = |v: &Value, field: &str| -> Result<(usize, usize)> {
            let (x, y) = pos(v, field)?;
            if flags(chars[y * w + x]) & MOVE != 0 {
                return Err(invalid(field, "must be on open ground"));
            }
            Ok((x, y))
        };
        let raw_spawns = array(&source["spawns"], "spawns")?;
        if !(minimum_spawns..=6).contains(&raw_spawns.len()) {
            return Err(invalid(
                "spawns",
                if is_world {
                    "needs 1-6 spawns"
                } else {
                    "needs 2-6 spawns"
                },
            ));
        }
        let positions = raw_spawns
            .iter()
            .map(|spawn| open_pos(spawn, "spawns"))
            .collect::<Result<Vec<_>>>()?;
        let spawns = raw_spawns
            .iter()
            .zip(positions)
            .map(|(spawn, (x, y))| {
                Ok(Spawn {
                    x,
                    y,
                    assault: optional_bool(spawn, "assault")?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        if spawns.iter().filter(|s| !s.assault).count() < minimum_spawns {
            return Err(invalid(
                "spawns",
                if is_world {
                    "needs a non-assault spawn"
                } else {
                    "needs two non-assault spawns"
                },
            ));
        }
        let assault_time = source
            .get("assaultTime")
            .map(|v| integer(v, 300, 3600, "assaultTime"))
            .transpose()?;
        let defend = source
            .get("defend")
            .map(|v| {
                let list = array(v, "defend")?;
                if list.is_empty() || list.len() >= spawns.len() {
                    return Err(invalid("defend", "must list some, not all spawns"));
                }
                let mut out = Vec::new();
                for v in list {
                    let i = integer(v, 0, spawns.len() - 1, "defend")?;
                    if out.contains(&i) {
                        return Err(invalid("defend", "duplicate spawn"));
                    }
                    out.push(i);
                }
                Ok(out)
            })
            .transpose()?;
        let naval = optional_bool(&source, "naval")?;
        let trench_facing = optional_bool(&source, "trenchFacing")?;
        let raw_points = array(&source["points"], "points")?;
        if if is_world {
            !raw_points.is_empty()
        } else {
            !(1..=9).contains(&raw_points.len())
        } {
            return Err(invalid(
                "points",
                if is_world {
                    "World maps need zero capture points"
                } else {
                    "needs 1-9 capture points"
                },
            ));
        }
        // The editor reports all geometry and score failures before point metadata.
        let mut points = raw_points
            .iter()
            .map(|point| {
                let (x, y) = open_pos(point, "points")?;
                let score = |key| match point.get(key) {
                    None | Some(Value::Null) => Ok(1.0),
                    Some(value) => number(value, 0.0, 5.0, key),
                };
                Ok(CapturePoint {
                    x,
                    y,
                    vp: score("vp")?,
                    mp: score("mp")?,
                    kind: None,
                    needs: None,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let sequence = source
            .get("pointSequence")
            .map(|value| integer(value, 0, 1_000_000_000, "pointSequence"))
            .transpose()?;
        for point in raw_points {
            if let (Some(sequence), Some(id)) = (sequence, point["id"].as_str())
                && let Some(digits) = id.strip_prefix("point:")
                && !digits.is_empty()
                && digits.bytes().all(|c| c.is_ascii_digit())
                && digits.parse::<f64>().unwrap_or(f64::INFINITY) >= sequence as f64
            {
                return Err(invalid("pointSequence", "must exceed existing point IDs"));
            }
        }
        for (point, raw) in points.iter_mut().zip(raw_points) {
            point.kind = raw
                .get("kind")
                .map(|kind| {
                    kind.as_str()
                        .filter(|kind| ["radio", "depot"].contains(kind))
                        .map(str::to_owned)
                        .ok_or_else(|| invalid("points.kind", "unknown point kind"))
                })
                .transpose()?;
        }
        for (i, (point, raw)) in points.iter_mut().zip(raw_points).enumerate() {
            point.needs = raw
                .get("needs")
                .map(|value| integer(value, 0, raw_points.len() - 1, "points.needs"))
                .transpose()?;
            if point.needs == Some(i) {
                return Err(invalid("points.needs", "cannot depend on itself"));
            }
        }
        for i in 0..points.len() {
            let mut cursor = i;
            for k in 0..=points.len() + 1 {
                match points[cursor].needs {
                    None => break,
                    Some(j) => cursor = j,
                }
                if k > points.len() {
                    return Err(invalid("points.needs", "dependency cycle"));
                }
            }
        }
        let catalog = Catalog::compiled().map_err(|e| MapError::Catalog(e.to_string()))?;
        let houses = catalog
            .table("CFG")
            .map_err(|e| MapError::Catalog(e.to_string()))?
            .get("houses")
            .and_then(crate::catalog::Value::array)
            .ok_or_else(|| MapError::Catalog("missing houses".into()))?;
        let mut buildings = Vec::new();
        if let Some(v) = source.get("buildings") {
            let list = array(v, "buildings")?;
            if list.len() > 500 {
                return Err(invalid("buildings", "at most 500 entries"));
            }
            for b in list {
                let (x, y) = pos(b, "buildings")?;
                let kind = b["kind"]
                    .as_str()
                    .ok_or_else(|| invalid("buildings.kind", "expected a house kind"))?;
                let valid = if kind == "stone bridge" {
                    chars[y * w + x] == b'='
                } else {
                    chars[y * w + x] == b'B'
                        && houses.iter().any(|v| {
                            v.get("name").and_then(crate::catalog::Value::string) == Some(kind)
                        })
                };
                if !valid {
                    return Err(invalid("buildings", "kind must match house or bridge cell"));
                }
                buildings.push(Building {
                    x,
                    y,
                    kind: kind.into(),
                });
            }
        }
        let mut triggers = Vec::new();
        if let Some(v) = source.get("triggers") {
            let list = array(v, "triggers")?;
            if list.len() > 20 {
                return Err(invalid("triggers", "at most 20 entries"));
            }
            for t in list {
                let at = number(&t["at"], 0.0, 7200.0, "triggers.at")?;
                let say = t
                    .get("say")
                    .map(|v| {
                        v.as_str()
                            .filter(|s| (1..=140).contains(&s.encode_utf16().count()))
                            .map(str::to_owned)
                            .ok_or_else(|| {
                                invalid("triggers.say", "expected 1-140 UTF-16 code units")
                            })
                    })
                    .transpose()?;
                let blow = t
                    .get("blow")
                    .map(|v| {
                        let a = array(v, "triggers.blow")?;
                        if a.len() != 4 {
                            return Err(invalid("triggers.blow", "expected four coordinates"));
                        }
                        let x0 = integer(&a[0], 0, w - 1, "triggers.blow")?;
                        let y0 = integer(&a[1], 0, h - 1, "triggers.blow")?;
                        Ok([
                            x0,
                            y0,
                            integer(&a[2], x0, w - 1, "triggers.blow")?,
                            integer(&a[3], y0, h - 1, "triggers.blow")?,
                        ])
                    })
                    .transpose()?;
                if say.is_none() && blow.is_none() {
                    return Err(invalid("triggers", "must say or blow something"));
                }
                triggers.push(Trigger { at, say, blow });
            }
        }
        crate::scenario_validation::validate_scenario(&source, catalog)
            .map_err(|e| invalid("scenario", &e.0))?;
        // Legacy timed triggers migrate to a scenario in the reference. Protect those
        // scenario limits too, without claiming support for authored scenario graphs.
        if !triggers.is_empty() {
            let mut work = 0;
            let mut ids = std::collections::BTreeSet::new();
            for (i, point) in raw_points.iter().enumerate() {
                let default_id = format!("point:{i}");
                let id = match point.get("id") {
                    None => default_id.as_str(),
                    Some(value) => value
                        .as_str()
                        .ok_or_else(|| invalid("points.id", "expected a stable identifier"))?,
                };
                if id.is_empty()
                    || id.len() > 64
                    || !id.as_bytes()[0].is_ascii_alphanumeric()
                    || !id
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || b":_-".contains(&c))
                    || !ids.insert(id.to_owned())
                {
                    return Err(invalid(
                        "points.id",
                        "scenario capture targets need unique stable IDs",
                    ));
                }
            }
            for trigger in &triggers {
                work += 1 + usize::from(trigger.say.is_some());
                if let Some([x0, y0, x1, y1]) = trigger.blow {
                    let cells = (x1 - x0 + 1) * (y1 - y0 + 1);
                    if cells > 256 {
                        return Err(invalid(
                            "triggers.blow",
                            "scenario boxes contain at most 256 cells",
                        ));
                    }
                    work += cells + 1;
                }
            }
            if work > 4096 {
                return Err(invalid("triggers", "scenario work exceeds 4096"));
            }
        }
        let geography = if editor {
            if is_world {
                crate::world_map::validate_editor(
                    &source["world"],
                    w,
                    h,
                    &chars,
                    height.as_deref(),
                    spawns.len(),
                )?;
            }
            None
        } else {
            source
                .get("world")
                .map(|v| crate::world_map::parse(v, w, h, &chars, height.as_deref(), spawns.len()))
                .transpose()?
        };
        Ok(Self {
            source,
            name,
            w,
            h,
            chars,
            height,
            layers,
            spawns,
            points,
            buildings,
            triggers,
            defend,
            assault_time,
            naval,
            trench_facing,
            structures,
            geography,
        })
    }
    /// Immutable generated geography, in source region order. No ownership or capture state is inferred.
    pub fn geography(&self) -> Option<&crate::world_map::WorldDefinition> {
        self.geography.as_ref()
    }
    /// Authored structural definitions, in source order. Implicit sections are built later.
    pub fn structures(&self) -> &[crate::structures::StructureDefinition] {
        &self.structures
    }
    /// Initialize authoritative structural graphs after the original static terrain phase.
    pub fn initialize_structures(&self) -> Result<crate::structures::StructuredTerrain> {
        let mut terrain = self.initialize_terrain()?;
        let structures = crate::structures::initialize(&self.structures, &mut terrain);
        Ok(crate::structures::StructuredTerrain {
            terrain,
            structures,
        })
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn dimensions(&self) -> (usize, usize) {
        (self.w, self.h)
    }
    pub fn spawns(&self) -> &[Spawn] {
        &self.spawns
    }
    pub fn points(&self) -> &[CapturePoint] {
        &self.points
    }
    pub fn buildings(&self) -> &[Building] {
        &self.buildings
    }
    pub fn triggers(&self) -> &[Trigger] {
        &self.triggers
    }
    pub fn defend(&self) -> Option<&[usize]> {
        self.defend.as_deref()
    }
    pub fn assault_time(&self) -> Option<usize> {
        self.assault_time
    }
    pub fn layers(&self) -> &Layers {
        &self.layers
    }
    /// Original authoring fields, including identifiers and metadata this slice does not interpret.
    pub fn source(&self) -> &Value {
        &self.source
    }
}

/// The prototype's public authored-map projection. Private scripts and mine materials
/// are removed from a copy; the authoritative document is never changed.
pub fn map_for_client(source: &Value) -> Result<Value> {
    let mut result = source.clone();
    let object = result
        .as_object_mut()
        .ok_or_else(|| invalid("document", "expected a map object"))?;
    object.remove("triggers");
    object.remove("scenario");
    // mapForClient projects authored terrain without validating gameplay fields.
    // An editor save may intentionally omit World geography; that must not turn
    // readable public terrain into a failed runtime-import attempt.
    let w = integer(&result["w"], 20, 1024, "w")?;
    let h = integer(&result["h"], 20, 1024, "h")?;
    let chars = rows(&result["rows"], w, h, CELLS, "rows")?;
    let layers = normalize_layers(&result, w, h, &chars, true)?;
    let strings = |cells: &[u8]| -> Value {
        serde_json::json!(
            cells
                .chunks(w)
                .map(|row| String::from_utf8(row.to_vec()).expect("validated ASCII terrain"))
                .collect::<Vec<_>>()
        )
    };
    if source.get("layers").is_none() {
        result["worldVersion"] = serde_json::json!(2);
        result["layers"] = serde_json::json!({"ground":strings(&layers.ground),"objects":strings(&layers.objects)});
    }
    let hidden_rows = source
        .get("layers")
        .map_or(&source["rows"], |layers| &layers["objects"]);
    let hidden: Vec<_> = hidden_rows
        .as_array()
        .expect("validated rows")
        .iter()
        .flat_map(|row| row.as_str().expect("validated row").bytes())
        .map(|ch| ch == b'N')
        .collect();
    let public: Vec<_> = chars
        .iter()
        .enumerate()
        .map(|(cell, &ch)| {
            if ch != b'N' {
                ch
            } else if layers.objects[cell] != b'.' {
                layers.objects[cell]
            } else {
                layers.ground[cell]
            }
        })
        .collect();
    result["rows"] = strings(&public);
    result["layers"]["objects"] = strings(&layers.objects);
    result["layers"]["mines"] = serde_json::json!(vec![".".repeat(w); h]);
    for key in ["materials", "objectMaterials"] {
        if let Some(list) = result["layers"].get_mut(key).and_then(Value::as_array_mut) {
            list.retain(|entry| {
                !hidden[entry["c"].as_f64().expect("validated material cell") as usize]
            });
        }
    }
    if result["layers"].get("mineMaterials").is_some() {
        result["layers"]["mineMaterials"] = serde_json::json!([]);
    }
    Ok(result)
}

fn material_list(source: &Value, key: &str, n: usize) -> Result<BTreeMap<usize, String>> {
    let mut result = BTreeMap::new();
    if let Some(v) = source.get(key) {
        let list = array(v, key)?;
        if list.len() > n {
            return Err(invalid(key, "too many material entries"));
        }
        for item in list {
            let c = integer(&item["c"], 0, n - 1, key)?;
            let material = item["material"]
                .as_str()
                .filter(|s| MATERIALS.contains(s))
                .ok_or_else(|| invalid(key, "unknown material"))?;
            if result.insert(c, material.to_owned()).is_some() {
                return Err(invalid(key, "duplicate material cell"));
            }
        }
    }
    Ok(result)
}
fn normalize_layers(
    source: &Value,
    w: usize,
    h: usize,
    chars: &[u8],
    authoritative: bool,
) -> Result<Layers> {
    let (ground, authored, mines, legacy, explicit_base, explicit_object, explicit_mine) =
        if let Some(layer) = source.get("layers") {
            if source["worldVersion"].as_f64() != Some(2.0) {
                return Err(invalid("worldVersion", "layers require version 2"));
            }
            let ground = rows(&layer["ground"], w, h, BASE, "layers.ground")?;
            let allowed = if authoritative {
                OBJECT
            } else {
                b".BH#TXY=RNAQKO"
            };
            let authored = rows(&layer["objects"], w, h, allowed, "layers.objects")?;
            let mines = layer
                .get("mines")
                .map(|v| rows(v, w, h, b".N", "layers.mines"))
                .transpose()?
                .unwrap_or(vec![b'.'; w * h]);
            // Received runtime layers may differ from the initial rows after known terrain changes.
            for c in (0..chars.len()).filter(|_| authoritative) {
                let object = if authored[c] == b'N' {
                    b'.'
                } else {
                    authored[c]
                };
                let mine = authored[c] == b'N' || mines[c] == b'N';
                let composed = compose_world_cell(ground[c], object, mine);
                if composed != chars[c] {
                    return Err(invalid("layers", "rows must match composed layers"));
                }
                if mine && b"WF".contains(&ground[c]) {
                    return Err(invalid("layers", "mines need dry ground"));
                }
                if object == b'=' && ground[c] != b'W' {
                    return Err(invalid("layers", "bridge sections need water"));
                }
                if object != b'.' && object != b'=' && b"WF".contains(&ground[c]) {
                    return Err(invalid("layers", "land objects need dry ground"));
                }
            }
            (
                ground,
                authored,
                mines,
                material_list(layer, "materials", w * h)?,
                material_list(layer, "groundMaterials", w * h)?,
                material_list(layer, "objectMaterials", w * h)?,
                material_list(layer, "mineMaterials", w * h)?,
            )
        } else {
            if source.get("worldVersion").is_some() {
                return Err(invalid("worldVersion", "version 2 needs world layers"));
            }
            (
                chars
                    .iter()
                    .map(|c| {
                        if BASE.contains(c) {
                            *c
                        } else if *c == b'=' {
                            b'W'
                        } else {
                            b'.'
                        }
                    })
                    .collect(),
                chars
                    .iter()
                    .map(|c| {
                        if BASE.contains(c) || *c == b'N' {
                            b'.'
                        } else {
                            *c
                        }
                    })
                    .collect(),
                chars
                    .iter()
                    .map(|c| if *c == b'N' { b'N' } else { b'.' })
                    .collect(),
                BTreeMap::new(),
                BTreeMap::new(),
                BTreeMap::new(),
                BTreeMap::new(),
            )
        };
    let objects: Vec<_> = authored
        .iter()
        .map(|c| if *c == b'N' { b'.' } else { *c })
        .collect();
    let mines: Vec<_> = authored
        .iter()
        .zip(mines)
        .map(|(o, m)| u8::from(*o == b'N' || m == b'N'))
        .collect();
    let mut base_materials = BTreeMap::new();
    let mut object_materials = BTreeMap::new();
    let mut mine_materials = BTreeMap::new();
    for (c, m) in legacy {
        if objects[c] == b'.' && authored[c] != b'N' {
            base_materials.insert(c, m.clone());
        }
        if objects[c] != b'.' {
            object_materials.insert(c, m.clone());
        }
        if mines[c] != 0 && authored[c] == b'N' {
            mine_materials.insert(c, m);
        }
    }
    base_materials.extend(explicit_base);
    for (c, m) in explicit_object {
        if authored[c] == b'N' {
            mine_materials.insert(c, m);
        } else {
            object_materials.insert(c, m);
        }
    }
    mine_materials.extend(explicit_mine);
    let mut materials: BTreeMap<_, _> = base_materials
        .iter()
        .filter(|(c, _)| objects[**c] == b'.')
        .map(|(c, m)| (*c, m.clone()))
        .collect();
    for (c, m) in &object_materials {
        if objects[*c] != b'.' {
            materials.insert(*c, m.clone());
        }
    }
    for (c, ch) in objects.iter().enumerate() {
        if *ch == b'R' {
            materials.entry(c).or_insert_with(|| "stone".into());
        }
    }
    Ok(Layers {
        ground,
        objects,
        mines,
        materials,
        base_materials,
        object_materials,
        mine_materials,
    })
}
pub(crate) fn flags(ch: u8) -> u16 {
    let bits = match ch {
        b'B' | b'K' => MOVE | SIGHT,
        b'H' => SIGHT | COVER,
        b'#' | b'+' => COVER,
        b'T' => COVER | TRENCH,
        b'W' => MOVE,
        b'F' => FORD,
        b'=' | b'D' => ROAD,
        b'R' | b'Y' | b'Q' => VBLOCK | COVER,
        b'X' => WIRE,
        b'M' => MUD,
        b'O' => COVER | WOOD,
        _ => 0,
    };
    bits | if b"WF".contains(&ch) { 0 } else { LAND }
}

/// Static terrain only. Mines and materials stay here rather than entering a client view.
/// This does not include structural graphs, scenario execution, players, units or match RNG.
#[derive(Clone, Debug)]
pub struct TerrainState {
    pub w: usize,
    pub h: usize,
    pub chars: Vec<u8>,
    pub layers: Layers,
    pub flags: Vec<u16>,
    pub height: Option<Vec<i8>>,
    pub house: Vec<u8>,
    pub cell_hp: Vec<f32>,
    pub tough: Option<Vec<f32>>,
    pub wear: Vec<f32>,
    pub surface_wear: Vec<f32>,
    pub trench_front: Option<Vec<f32>>,
    pub shoal: Option<Vec<u8>>,
    pub roads: bool,
    pub naval: bool,
}
impl TerrainState {
    /// Transfer static terrain into the existing query state. It has no players or units and
    /// cannot start a match. Keep the `TerrainState` or its layers for hidden authoritative data.
    pub fn query_game(&self) -> Game {
        Game {
            w: self.w,
            h: self.h,
            flags: self.flags.clone(),
            height: self.height.clone(),
            house: self.house.clone(),
            chars: self.chars.clone(),
            ground: Some(self.layers.ground.clone()),
            objects: Some(self.layers.objects.clone()),
            wear: Some(self.wear.clone()),
            cell_hp: self.cell_hp.clone(),
            trench_front: self.trench_front.clone(),
            tough: self.tough.clone(),
            wet: None,
            reveal: false,
            wrecks: Vec::new(),
            tick: 0,
            vision_tick: None,
            terrain_version: 0,
            mode: None,
            naval: self.naval,
            weather: None,
            rain: 0.0,
            players: Vec::new(),
            units: BTreeMap::new(),
            smokes: Vec::new(),
            strikes: Vec::new(),
            sections: BTreeMap::new(),
        }
    }
}
fn neighbors(c: usize, w: usize, n: usize) -> impl Iterator<Item = usize> {
    [
        c.checked_sub(1).filter(|_| !c.is_multiple_of(w)),
        (c % w + 1 < w).then_some(c + 1),
        c.checked_sub(w),
        (c + w < n).then_some(c + w),
    ]
    .into_iter()
    .flatten()
}
fn component(chars: &[u8], w: usize, start: usize, seen: &mut [bool]) -> Vec<usize> {
    component_where(|cell| chars[cell] == chars[start], w, start, seen)
}
fn component_where(
    matches: impl Fn(usize) -> bool,
    width: usize,
    start: usize,
    seen: &mut [bool],
) -> Vec<usize> {
    let mut queue = vec![start];
    seen[start] = true;
    let mut i = 0;
    while i < queue.len() {
        let c = queue[i];
        i += 1;
        for next in neighbors(c, width, seen.len()) {
            if !seen[next] && matches(next) {
                seen[next] = true;
                queue.push(next);
            }
        }
    }
    queue
}
pub(crate) fn start_wear(ch: u8, c: usize) -> f32 {
    let mut h = (c as u32).wrapping_add(1).wrapping_mul(0x9e37_79b1);
    h ^= h >> 15;
    h = h.wrapping_mul(0x85eb_ca6b);
    let noise = f64::from(h ^ (h >> 13)) / 4_294_967_296.0;
    (match ch {
        b'M' | b'F' => 0.2 + 0.6 * noise,
        b'+' => 0.3 + 0.3 * noise,
        _ => 0.0,
    }) as f32
}
impl AuthoritativeMap {
    /// Build static terrain from the validated definition and the compiled rules catalog.
    pub fn initialize_terrain(&self) -> Result<TerrainState> {
        let rules = Rules::compiled().map_err(|e| MapError::Catalog(e.to_string()))?;
        let catalog = Catalog::compiled().map_err(|e| MapError::Catalog(e.to_string()))?;
        let cfg = catalog
            .table("CFG")
            .map_err(|e| MapError::Catalog(e.to_string()))?;
        let cfg_num = |key| {
            cfg.get(key)
                .and_then(crate::catalog::Value::number)
                .ok_or_else(|| MapError::Catalog(format!("missing {key}")))
        };
        let physical: Vec<_> = self
            .layers
            .ground
            .iter()
            .zip(&self.layers.objects)
            .map(|(g, o)| if *o == b'.' { *g } else { *o })
            .collect();
        let n = physical.len();
        let mut house = vec![0; n];
        for block in house_kinds_where(|c| physical[c] == b'B', n, self.w, &self.buildings, None)? {
            for c in block.cells {
                house[c] = block.kind;
            }
        }
        let mut tough = None;
        for b in self.buildings.iter().filter(|b| b.kind == "stone bridge") {
            let values = tough.get_or_insert_with(|| vec![1.0; n]);
            let mut seen = vec![false; n];
            for c in component(&self.chars, self.w, b.y * self.w + b.x, &mut seen) {
                values[c] = cfg_num("stoneBridge")? as f32;
            }
        }
        let mut terrain = TerrainState {
            w: self.w,
            h: self.h,
            chars: self.chars.clone(),
            layers: self.layers.clone(),
            flags: physical.iter().map(|c| flags(*c)).collect(),
            height: self.height.clone(),
            house,
            cell_hp: vec![0.0; n],
            tough,
            wear: self
                .layers
                .ground
                .iter()
                .enumerate()
                .map(|(c, ch)| {
                    if self.layers.objects[c] == b'T' {
                        0.0
                    } else {
                        start_wear(*ch, c)
                    }
                })
                .collect(),
            surface_wear: self
                .layers
                .ground
                .iter()
                .enumerate()
                .map(|(c, ch)| start_wear(*ch, c))
                .collect(),
            trench_front: None,
            shoal: None,
            roads: self.layers.ground.contains(&b'D'),
            naval: self.naval,
        };
        for (c, ch) in physical.iter().enumerate() {
            terrain.cell_hp[c] = (if *ch == b'B' {
                rules.movement.house_hp[usize::from(terrain.house[c])]
            } else {
                rules.movement.terrain_hp(*ch).unwrap_or(0.0)
                    * terrain.tough.as_ref().map_or(1.0, |t| f64::from(t[c]))
            }) as f32;
        }
        if self.trench_facing {
            let mut front = vec![f32::NAN; n];
            for (c, ch) in self.chars.iter().enumerate().filter(|(_, ch)| **ch == b'T') {
                let _ = ch;
                let (x, y) = ((c % self.w) as f64, (c / self.w) as f64);
                let mut nearest = &self.spawns[0];
                for spawn in self.spawns.iter().skip(1) {
                    if math::hypot(spawn.x as f64 - x, spawn.y as f64 - y)
                        < math::hypot(nearest.x as f64 - x, nearest.y as f64 - y)
                    {
                        nearest = spawn;
                    }
                }
                front[c] = math::atan2(y - nearest.y as f64, x - nearest.x as f64) as f32;
            }
            terrain.trench_front = Some(front);
        }
        if self.naval {
            let limit = cfg_num("shoal")? as i16;
            let mut distances = vec![-1i16; n];
            let mut queue = Vec::with_capacity(n);
            for (c, ch) in self.chars.iter().enumerate() {
                if *ch != b'W' {
                    distances[c] = 0;
                    queue.push(c);
                }
            }
            let mut i = 0;
            while i < queue.len() {
                let c = queue[i];
                i += 1;
                if distances[c] >= limit {
                    continue;
                }
                for next in neighbors(c, self.w, n) {
                    if distances[next] < 0 {
                        distances[next] = distances[c] + 1;
                        queue.push(next);
                    }
                }
            }
            let shoal: Vec<_> = distances
                .iter()
                .map(|d| u8::from(*d >= 0 && *d <= limit))
                .collect();
            for (c, bit) in shoal.iter().enumerate() {
                if *bit != 0 {
                    terrain.flags[c] |= SHOAL;
                }
            }
            terrain.shoal = Some(shoal);
        }
        Ok(terrain)
    }
}

/// Decode a valid authored height character. Presentation heights use levels -2 through 4.
pub fn level_of(ch: u8) -> Option<i8> {
    match ch {
        b'0'..=b'4' => Some((ch - b'0') as i8),
        b'a' => Some(-1),
        b'b' => Some(-2),
        _ => None,
    }
}

/// Encode a valid authored height level.
pub fn level_char(level: i8) -> Option<u8> {
    match level {
        -2 => Some(b'b'),
        -1 => Some(b'a'),
        0..=4 => Some(b'0' + level as u8),
        _ => None,
    }
}

/// Compose only the layers supplied by the caller. A mine marker takes precedence.
pub fn compose_world_cell(ground: u8, object: u8, mine: bool) -> u8 {
    if mine {
        b'N'
    } else if object != b'.' && object != 0 {
        object
    } else {
        ground
    }
}

/// Normalize a received map without requiring authoritative spawns, objectives or scenarios.
/// This reads only the supplied document and accepts discovered runtime object characters.
pub fn world_layers(source: &Value) -> Result<Layers> {
    let w = integer(&source["w"], 1, 1024, "w")?;
    let h = integer(&source["h"], 1, 1024, "h")?;
    let chars = rows(&source["rows"], w, h, b".BH#+TWF=RXYDMNOQAK", "rows")?;
    normalize_layers(source, w, h, &chars, false)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HouseBlock {
    /// Cells in the source breadth-first traversal order.
    pub cells: Vec<usize>,
    pub kind: u8,
}

/// Connected known houses, visited in seed order or ascending cell order when seeds are absent.
/// Named overrides are resolved in breadth-first order, preserving the last named cell.
pub fn house_kinds_where(
    is_house: impl Fn(usize) -> bool,
    cells: usize,
    width: usize,
    buildings: &[Building],
    seeds: Option<&[usize]>,
) -> Result<Vec<HouseBlock>> {
    if width == 0 || !cells.is_multiple_of(width) {
        return Err(invalid("houses", "invalid rectangular cell count"));
    }
    if seeds.is_some_and(|seeds| seeds.iter().any(|&c| c >= cells)) {
        return Err(invalid("houses", "seed outside map"));
    }
    let catalog = Catalog::compiled().map_err(|e| MapError::Catalog(e.to_string()))?;
    let houses = catalog
        .table("CFG")
        .map_err(|e| MapError::Catalog(e.to_string()))?
        .get("houses")
        .and_then(crate::catalog::Value::array)
        .ok_or_else(|| MapError::Catalog("missing houses".into()))?;
    let named: BTreeMap<_, _> = buildings
        .iter()
        .filter_map(|building| {
            if building.x >= width || building.y >= cells / width {
                return None;
            }
            houses
                .iter()
                .position(|house| {
                    house.get("name").and_then(crate::catalog::Value::string)
                        == Some(building.kind.as_str())
                })
                .map(|kind| (building.y * width + building.x, kind))
        })
        .collect();
    let mut seen = vec![false; cells];
    let mut out = Vec::new();
    let starts: Box<dyn Iterator<Item = usize> + '_> = match seeds {
        Some(seeds) => Box::new(seeds.iter().copied()),
        None => Box::new(0..cells),
    };
    for start in starts {
        if seen[start] || !is_house(start) {
            continue;
        }
        let queue = component_where(&is_house, width, start, &mut seen);
        let mut kind = houses
            .iter()
            .rposition(|house| {
                house
                    .get("min")
                    .and_then(crate::catalog::Value::number)
                    .is_some_and(|minimum| queue.len() as f64 >= minimum)
            })
            .ok_or_else(|| MapError::Catalog("no default house kind".into()))?;
        for c in &queue {
            if let Some(named_kind) = named.get(c) {
                kind = *named_kind;
            }
        }
        out.push(HouseBlock {
            cells: queue,
            kind: kind as u8,
        });
    }
    Ok(out)
}

/// Border segments in source run/side order, in cell coordinates.
pub fn territory_edges(runs: &[[i64; 3]]) -> Vec<[i64; 4]> {
    let mut rows: BTreeMap<i64, Vec<(i64, i64)>> = BTreeMap::new();
    let mut edges = Vec::new();
    for &[y, a, b] in runs {
        rows.entry(y).or_default().push((a, b));
    }
    for &[y, a, b] in runs {
        edges.extend([[a, y, a, y + 1], [b, y, b, y + 1]]);
        for side in [-1, 1] {
            let mut x = a;
            let line = if side < 0 { y } else { y + 1 };
            for &(lo, hi) in rows.get(&(y + side)).into_iter().flatten() {
                if hi <= x || lo >= b {
                    continue;
                }
                if lo > x {
                    edges.push([x, line, lo.min(b), line]);
                }
                x = x.max(hi);
                if x >= b {
                    break;
                }
            }
            if x < b {
                edges.push([x, line, b, line]);
            }
        }
    }
    edges
}

/// Test only known runs when present, including an empty known set. Legacy bounds are in metres.
pub fn inside_known_region(
    runs: Option<&[[usize; 3]]>,
    bounds: Option<[f64; 4]>,
    at: Point,
    cell: f64,
) -> bool {
    if let Some(runs) = runs {
        let x = (at.x / cell).floor();
        let y = (at.z / cell).floor();
        return runs
            .iter()
            .any(|r| y == r[0] as f64 && x >= r[1] as f64 && x < r[2] as f64);
    }
    bounds.is_some_and(|[x0, z0, x1, z1]| at.x >= x0 && at.z >= z0 && at.x < x1 && at.z < z1)
}
