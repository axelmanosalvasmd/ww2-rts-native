//! Pure authoring validation, matching shared/sim.js:1139-1162 at e4e579a.
use crate::{
    game::{MOVE, VBLOCK},
    map::flags,
};
use serde_json::Value;
use std::collections::BTreeSet;

fn integer(value: &Value) -> Option<f64> {
    value.as_f64().filter(|n| n.is_finite() && n.fract() == 0.0)
}

fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|v| v != 0.0),
        Value::String(s) => !s.is_empty(),
        _ => true,
    }
}

/// Validate raw World geography against an already validated terrain grid and spawn count.
/// Returns the prototype's error text, in its original check order. This does not construct
/// typed geography: that subsequent import also checks native storage bounds and metadata.
/// The grid must have nonzero dimensions, one character per cell and, if supplied, one height
/// per cell. Malformed grid shapes return `invalid world regions` without indexing them.
pub fn validate_geography(
    world: &Value,
    width: usize,
    height: usize,
    chars: &[u8],
    heights: Option<&[i8]>,
    spawns: usize,
) -> Result<(), &'static str> {
    let count = match world["size"].as_str() {
        Some("huge") => 64,
        Some("massive") => 128,
        _ => return Err("invalid world regions"),
    };
    let regions = world["regions"].as_array().filter(|r| r.len() == count);
    let Some(cells) = width.checked_mul(height).filter(|n| *n > 0) else {
        return Err("invalid world regions");
    };
    if integer(&world["total"]) != Some(count as f64)
        || integer(&world["regionCells"]) != Some(64.0)
        || regions.is_none()
        || chars.len() != cells
        || heights.is_some_and(|h| h.len() != cells)
    {
        return Err("invalid world regions");
    }
    let regions = regions.unwrap();
    let membership = &world["regionMap"];
    if truthy(membership)
        && !membership.as_array().is_some_and(|a| {
            a.len() == cells
                && a.iter()
                    .all(|id| integer(id).is_some_and(|id| id >= -1.0 && id < count as f64))
        })
    {
        return Err("invalid region membership");
    }
    let mut ids = BTreeSet::new();
    let mut homes = BTreeSet::new();
    let mut objectives = Vec::with_capacity(count);
    for region in regions {
        let id = integer(&region["id"]);
        let x = integer(&region["x"]).filter(|x| *x >= 0.0 && *x < width as f64);
        let y = integer(&region["y"]).filter(|y| *y >= 0.0 && *y < height as f64);
        let (Some(id), Some(x), Some(y)) = (id, x, y) else {
            return Err("invalid world region");
        };
        let cell = y as usize * width + x as usize;
        // JavaScript Set treats negative and positive zero as the same ID.
        let key = if id == 0.0 { 0 } else { id.to_bits() };
        if !ids.insert(key)
            || flags(chars[cell]) & MOVE != 0
            || !region["bounds"].as_array().is_some_and(|b| b.len() == 4)
        {
            return Err("invalid world region");
        }
        if let Some(home) = region.get("home") {
            let home = integer(home)
                .filter(|n| *n >= 0.0 && *n < spawns as f64)
                .ok_or("invalid world home")? as usize;
            if !homes.insert(home) {
                return Err("invalid world home");
            }
        }
        objectives.push(cell);
    }
    if homes.len() != spawns {
        return Err("each player needs one home");
    }
    let first = objectives[0];
    let mut seen = vec![false; cells];
    let mut queue = vec![first];
    seen[first] = true;
    let mut i = 0;
    while i < queue.len() {
        let cell = queue[i];
        i += 1;
        let x = cell % width;
        let y = cell / width;
        for next in [
            (x + 1 < width).then_some(cell + 1),
            cell.checked_sub(1).filter(|_| x > 0),
            (y + 1 < height).then_some(cell + width),
            cell.checked_sub(width),
        ]
        .into_iter()
        .flatten()
        {
            if !seen[next]
                && flags(chars[next]) & (MOVE | VBLOCK) == 0
                && heights.is_none_or(|h| (i16::from(h[next]) - i16::from(h[cell])).abs() <= 1)
            {
                seen[next] = true;
                queue.push(next);
            }
        }
    }
    if objectives.iter().all(|c| seen[*c]) {
        Ok(())
    } else {
        Err("world regions need connected ground routes")
    }
}
