//! World startup rail records from `setupRails` and `railYard` at reference `e4e579a`.
//! The caller supplies authoritative metadata and the current terrain before World base placement.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use crate::game::CELL;

pub const STATION_YARD: i64 = 5;
pub const RAIL_DIRS: [(i64, i64); 8] = [
    (1, 0),
    (1, 1),
    (0, 1),
    (-1, 1),
    (-1, 0),
    (-1, -1),
    (0, -1),
    (1, -1),
];

#[derive(Clone, Debug, PartialEq)]
pub struct WorldStation {
    pub region: usize,
    pub cell: usize,
    pub x: f64,
    pub z: f64,
    /// Mutable train departure deadlines by team. Startup has no pending departure.
    pub next: BTreeMap<i32, i64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorldRailLine {
    pub a: usize,
    pub b: usize,
    pub cells: Vec<usize>,
    pub regions: Vec<usize>,
    pub bridges: Vec<usize>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct WorldRails {
    pub stations: Vec<WorldStation>,
    pub lines: Vec<WorldRailLine>,
    /// Source Map insertion order, with repeated track keys updating their existing mask.
    pub rail_at: Vec<(usize, u8)>,
    /// Source Map insertion order, with the last station at a cell supplying its region.
    pub station_at: Vec<(usize, usize)>,
    /// Source Set insertion order. Signed cells deliberately retain off-map and row-wrap values.
    pub yard: Vec<i64>,
}

fn array<'a>(value: &'a Value, field: &str) -> Result<&'a [Value], String> {
    value
        .as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| format!("World rails {field}: expected an array"))
}

fn index(value: &Value, limit: usize, field: &str) -> Result<usize, String> {
    value
        .as_f64()
        .filter(|v| v.is_finite() && v.fract() == 0. && *v >= 0. && *v < limit as f64)
        .map(|v| v as usize)
        .ok_or_else(|| format!("World rails {field}: index outside its source collection"))
}

fn indices(value: &Value, limit: usize, field: &str) -> Result<Vec<usize>, String> {
    array(value, field)?
        .iter()
        .map(|v| index(v, limit, field))
        .collect()
}

/// Preserve Map insertion order while replacing the value of a repeated key.
fn put<T>(
    rows: &mut Vec<(usize, T)>,
    positions: &mut BTreeMap<usize, usize>,
    key: usize,
    value: T,
) {
    if let Some(&position) = positions.get(&key) {
        rows[position].1 = value;
    } else {
        positions.insert(key, rows.len());
        rows.push((key, value));
    }
}

impl WorldRails {
    /// Build startup state atomically from `WorldDefinition.metadata["rails"]`.
    /// Missing or source-falsy metadata leaves rails absent. Malformed records fail before
    /// returning any state. Region references are checked against the explicit geography count.
    pub fn from_metadata(
        rails: Option<&Value>,
        chars: &[u8],
        width: usize,
        height: usize,
        regions: usize,
    ) -> Result<Self, String> {
        let cells = width
            .checked_mul(height)
            .filter(|n| width > 0 && height > 0 && *n <= i32::MAX as usize)
            .ok_or_else(|| "World rails terrain: invalid dimensions".to_owned())?;
        if chars.len() != cells {
            return Err("World rails terrain: incorrect cell count".into());
        }
        let Some(rails) = rails else {
            return Ok(Self::default());
        };
        if rails.is_null()
            || rails == &Value::Bool(false)
            || rails.as_f64() == Some(0.)
            || rails.as_str() == Some("")
        {
            return Ok(Self::default());
        }
        if !rails.is_object() {
            return Err("World rails: expected an object".into());
        }
        let stations = array(&rails["stations"], "stations")?
            .iter()
            .map(|station| {
                let region = index(&station["region"], regions, "station region")?;
                let x = index(&station["x"], width, "station x")?;
                let y = index(&station["y"], height, "station y")?;
                Ok(WorldStation {
                    region,
                    cell: y * width + x,
                    x: (x as f64 + 0.5) * CELL,
                    z: (y as f64 + 0.5) * CELL,
                    next: BTreeMap::new(),
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let lines = array(&rails["lines"], "lines")?
            .iter()
            .map(|line| {
                let a = index(&line["a"], regions, "line a")?;
                let b = index(&line["b"], regions, "line b")?;
                let cells = indices(&line["cells"], cells, "line cells")?;
                let regions = indices(&line["regions"], regions, "line regions")?;
                let bridges = cells
                    .iter()
                    .copied()
                    .filter(|&c| chars[c] == b'=')
                    .collect();
                Ok(WorldRailLine {
                    a,
                    b,
                    cells,
                    regions,
                    bridges,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let mut out = Self {
            stations,
            lines,
            ..Self::default()
        };
        let mut track_positions = BTreeMap::new();
        for line in &out.lines {
            for pair in line.cells.windows(2) {
                let [a, b] = [pair[0], pair[1]];
                let delta = (
                    (b % width) as i64 - (a % width) as i64,
                    (b / width) as i64 - (a / width) as i64,
                );
                let Some(direction) = RAIL_DIRS.iter().position(|&d| d == delta) else {
                    // Source setup skips repeated and nonadjacent hops rather than joining them.
                    continue;
                };
                for (cell, bit) in [(a, direction), (b, (direction + 4) % 8)] {
                    let mask = track_positions
                        .get(&cell)
                        .map_or(0, |&position| out.rail_at[position].1);
                    put(
                        &mut out.rail_at,
                        &mut track_positions,
                        cell,
                        mask | 1 << bit,
                    );
                }
            }
        }
        let mut station_positions = BTreeMap::new();
        for station in &out.stations {
            put(
                &mut out.station_at,
                &mut station_positions,
                station.cell,
                station.region,
            );
        }
        let mut yard_members = BTreeSet::new();
        for &(cell, _) in &out.rail_at {
            let cell = cell as i64;
            if yard_members.insert(cell) {
                out.yard.push(cell);
            }
        }
        for &(cell, _) in &out.station_at {
            for dy in -STATION_YARD..=STATION_YARD {
                for dx in -STATION_YARD..=STATION_YARD {
                    let yard_cell = cell as i64 + dy * width as i64 + dx;
                    if yard_members.insert(yard_cell) {
                        out.yard.push(yard_cell);
                    }
                }
            }
        }
        Ok(out)
    }
}
