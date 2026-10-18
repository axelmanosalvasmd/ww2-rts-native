//! Borrowed building placement for previews and command admission. No sites or nodes are mutated.
use crate::{
    catalog::{Catalog, Value},
    economy::Mode,
    game::{CELL, Game, LAND, MOVE, Point},
    math,
    rules::Rules,
};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    Blocked,
    NotVisible,
    Territory,
    Rail,
    Coast,
}

#[derive(Clone, Copy, Debug)]
pub struct Node {
    pub cell: usize,
    pub at: Point,
    pub claimed: bool,
}

/// World territory and rail facts belong to the caller; supplying them admits no World command.
pub struct Context<'a> {
    pub game: &'a Game,
    pub rules: &'a Rules,
    pub mode: Mode,
    pub logistics: bool,
    pub nodes: &'a [Node],
    pub owned_cells: Option<&'a BTreeSet<usize>>,
    pub rail_yard: Option<&'a BTreeSet<usize>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Placement {
    pub reason: Option<Reason>,
    pub at: Point,
    pub cell: Option<usize>,
    pub cells: Option<Vec<usize>>,
    pub node: Option<usize>,
}
impl Placement {
    fn denied(at: Point, reason: Reason) -> Self {
        Self {
            reason: Some(reason),
            at,
            cell: None,
            cells: None,
            node: None,
        }
    }
}

/// The building branch of Source `placementCheck`, including its refusal precedence.
/// Fortifications continue to use `field_works::placement`.
pub fn check(
    context: &Context<'_>,
    kind: &str,
    at: Point,
    sees: impl FnMut(Point) -> Result<bool, String>,
) -> Result<Placement, String> {
    let game = context.game;
    let occupied = game
        .units
        .values()
        .filter(|unit| unit.hp > 0.0 && !unit.riding && unit.x.is_finite() && unit.z.is_finite())
        .filter_map(|unit| match context.rules.kinds.get(unit.kind) {
            Some(kind) if !kind.air && !kind.structure => Some(Ok(unit.at())),
            Some(_) => None,
            None => Some(Err("invalid placement actor kind".to_owned())),
        })
        .collect::<Result<Vec<_>, _>>()?;
    check_parts(
        &Terrain {
            w: game.w,
            h: game.h,
            flags: &game.flags,
            chars: &game.chars,
            height: game.height.as_deref(),
            naval: game.naval,
            occupied: &occupied,
        },
        context.mode,
        context.logistics,
        context.nodes,
        context.owned_cells,
        context.rail_yard,
        kind,
        at,
        sees,
    )
}

/// The authoritative and recipient adapters supply the same borrowed placement facts.
struct Terrain<'a> {
    w: usize,
    h: usize,
    flags: &'a [u16],
    chars: &'a [u8],
    height: Option<&'a [i8]>,
    naval: bool,
    occupied: &'a [Point],
}
impl Terrain<'_> {
    fn cell_of(&self, at: Point) -> Option<usize> {
        if !at.x.is_finite()
            || !at.z.is_finite()
            || at.x < 0.0
            || at.z < 0.0
            || at.x >= self.w as f64 * CELL
            || at.z >= self.h as f64 * CELL
        {
            return None;
        }
        Some((at.z / CELL) as usize * self.w + (at.x / CELL) as usize)
    }
}
fn foot_center(game: &Terrain<'_>, cell: usize, size: usize) -> Point {
    Point {
        x: (cell % game.w) as f64 * CELL + size as f64 * CELL / 2.0,
        z: (cell / game.w) as f64 * CELL + size as f64 * CELL / 2.0,
    }
}
fn footprint(game: &Terrain<'_>, cell: usize, size: usize) -> Option<Vec<usize>> {
    let (x, z) = (cell % game.w, cell / game.w);
    if x + size > game.w || z + size > game.h {
        return None;
    }
    Some(
        (z..z + size)
            .flat_map(|z| (x..x + size).map(move |x| z * game.w + x))
            .collect(),
    )
}
#[allow(clippy::too_many_arguments)]
fn check_parts(
    game: &Terrain<'_>,
    mode: Mode,
    logistics: bool,
    nodes: &[Node],
    owned_cells: Option<&BTreeSet<usize>>,
    rail_yard: Option<&BTreeSet<usize>>,
    kind: &str,
    at: Point,
    mut sees: impl FnMut(Point) -> Result<bool, String>,
) -> Result<Placement, String> {
    let fail = |reason| Placement::denied(at, reason);
    if !at.x.is_finite() || !at.z.is_finite() {
        return Ok(Placement::denied(
            Point {
                x: if at.x.is_finite() { at.x } else { 0.0 },
                z: if at.z.is_finite() { at.z } else { 0.0 },
            },
            Reason::Blocked,
        ));
    }
    let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
    let buildable = catalog
        .table("BUILDABLE")
        .map_err(|e| e.to_string())?
        .array()
        .ok_or("invalid building list")?
        .iter()
        .any(|name| name.string() == Some(kind));
    if !(buildable || kind == "hq" && (mode.is_skirmish() || mode == Mode::World))
        || kind == "supplycache" && !logistics
    {
        return Ok(fail(Reason::Blocked));
    }
    let world = mode == Mode::World;
    if world && !sees(at)? {
        return Ok(fail(Reason::NotVisible));
    }
    let count = game.w.checked_mul(game.h).ok_or("placement dimensions")?;
    if count == 0
        || game.flags.len() != count
        || game.chars.len() != count
        || game.height.as_ref().is_some_and(|rows| rows.len() != count)
    {
        return Err("invalid placement terrain".into());
    }
    let definition = catalog.unit(kind).map_err(|e| e.to_string())?;
    let size = definition
        .get("size")
        .and_then(Value::number)
        .filter(|size| size.is_finite() && *size >= 1.0 && *size <= count as f64)
        .ok_or("invalid building size")? as usize;
    let node = if kind == "depot" {
        let mut nearest = None;
        let mut distance = f64::INFINITY;
        // Strict comparison keeps Source's stable input order for equal distances.
        for (index, node) in nodes.iter().enumerate() {
            let candidate = math::hypot(node.at.x - at.x, node.at.z - at.z);
            if !node.claimed && candidate <= 8.0 && candidate < distance {
                nearest = Some(index);
                distance = candidate;
            }
        }
        if nearest.is_none() {
            for node in nodes {
                if math::hypot(node.at.x - at.x, node.at.z - at.z) <= 8.0
                    && node.cell < count
                    && !sees(foot_center(game, node.cell, size))?
                {
                    return Ok(fail(Reason::NotVisible));
                }
            }
            return Ok(fail(Reason::Blocked));
        }
        nearest
    } else {
        None
    };
    let cell = if let Some(index) = node {
        nodes[index].cell
    } else {
        let x = math::round(at.x / CELL - size as f64 / 2.0);
        let z = math::round(at.z / CELL - size as f64 / 2.0);
        if x < 0.0 || z < 0.0 || x >= game.w as f64 || z >= game.h as f64 {
            return Ok(fail(Reason::Blocked));
        }
        z as usize * game.w + x as usize
    };
    if cell >= count {
        return Err("invalid placement node cell".into());
    }
    let cells = footprint(game, cell, size);
    let mut result = Placement {
        reason: Some(Reason::Blocked),
        at: foot_center(game, cell, size),
        cell: Some(cell),
        cells,
        node,
    };
    let Some(cells) = result.cells.as_ref() else {
        return Ok(result);
    };
    if world {
        for &cell in cells {
            if !sees(Point {
                x: (cell % game.w) as f64 * CELL + CELL / 2.0,
                z: (cell / game.w) as f64 * CELL + CELL / 2.0,
            })? {
                result.reason = Some(Reason::NotVisible);
                return Ok(result);
            }
        }
    } else if !sees(result.at)? {
        result.reason = Some(Reason::NotVisible);
        return Ok(result);
    }
    let reason = if world
        && !cells
            .iter()
            .all(|cell| owned_cells.is_some_and(|owned| owned.contains(cell)))
    {
        Some(Reason::Territory)
    } else if cells
        .iter()
        .any(|cell| rail_yard.is_some_and(|yard| yard.contains(cell)))
    {
        Some(Reason::Rail)
    } else if kind != "depot"
        && nodes.iter().any(|node| {
            node.cell < count
                && footprint(game, node.cell, 2)
                    .is_some_and(|footprint| footprint.iter().any(|cell| cells.contains(cell)))
        })
    {
        Some(Reason::Blocked)
    } else if definition.get("coast").and_then(Value::boolean) == Some(true)
        && !(game.naval && on_coast(game, cells))
    {
        Some(if game.naval {
            Reason::Coast
        } else {
            Reason::Blocked
        })
    } else if !can_stamp(game, cells) {
        Some(Reason::Blocked)
    } else {
        None
    };
    result.reason = reason;
    Ok(result)
}

fn on_coast(game: &Terrain<'_>, cells: &[usize]) -> bool {
    cells.iter().any(|cell| {
        let (x, z) = ((cell % game.w) as isize, (cell / game.w) as isize);
        (-2..=2).any(|dz| {
            (-2..=2).any(|dx| {
                let (x, z) = (x + dx, z + dz);
                x >= 0
                    && z >= 0
                    && x < game.w as isize
                    && z < game.h as isize
                    && game.flags[z as usize * game.w + x as usize] & LAND == 0
            })
        })
    })
}

fn can_stamp(game: &Terrain<'_>, cells: &[usize]) -> bool {
    let (mut low, mut high) = (i8::MAX, i8::MIN);
    for &cell in cells {
        if game.flags[cell] & MOVE != 0 || b"WF=KOAQ".contains(&game.chars[cell]) {
            return false;
        }
        let height = game.height.map_or(0, |height| height[cell]);
        low = low.min(height);
        high = high.max(height);
    }
    i16::from(high) - i16::from(low) <= 1
        && !game
            .occupied
            .iter()
            .any(|at| game.cell_of(*at).is_some_and(|cell| cells.contains(&cell)))
}

mod recipient;
pub use recipient::check_recipient;

#[cfg(test)]
mod tests;
