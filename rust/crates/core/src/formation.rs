//! Formation spots for a move order: a port of `shared/formation.js` at the reference pin, which the
//! browser's order preview and the sim both call, so a preview and the order agree.
//!
//! Every step matches the JavaScript bit for bit: `cos`, `sin`, `atan2`, `round`, `max` and `min`
//! come from the core's math module, sums run in the same order, and both sorts are V8's
//! (`crate::sort`), because the first comparator is not always consistent. `sqrt`, `floor` and
//! `ceil` are exact or correctly rounded in IEEE 754, so Rust's own are the same everywhere.
//! `tests/formation.rs` checks the port against `harness/formation-fixture.mjs`.

use crate::{math, sort};

/// The shapes the Formation menu offers, in menu order (`SHAPES`).
pub const SHAPES: [&str; 4] = ["line", "block", "column", "wedge"];

/// The smallest slot for infantry, in metres: the sim's own minimum.
pub const INFANTRY_SLOT: f64 = 3.0;
/// The smallest slot for everything else, in metres.
pub const VEHICLE_SLOT: f64 = 5.0;
/// Room left between hulls and squads, added to a unit's diameter.
pub const SLOT_MARGIN: f64 = 1.5;
/// Metres between one rank's deepest slot and the next rank, before spread.
pub const RANK_GAP: f64 = 1.5;
/// A line's natural rank: up to this many side by side.
pub const LINE_RANK: f64 = 10.0;
/// A rank spreads out along a drag to at most this many times its natural width.
pub const MAX_STRETCH: f64 = 3.0;
/// The first sort compares how far forward units stand in steps of 1 / this many metres
/// (centimetres), so units closer than that keep their order.
pub const DEPTH_STEPS: f64 = 100.0;

/// The numbers above, in a fixed order, for the rules hash (`crate::rules::RULES_HASH`).
pub const RULE_NUMBERS: [f64; 7] = [
    INFANTRY_SLOT,
    VEHICLE_SLOT,
    SLOT_MARGIN,
    RANK_GAP,
    LINE_RANK,
    MAX_STRETCH,
    DEPTH_STEPS,
];

/// A formation shape. Names outside `SHAPES` count as `Line` in the JavaScript; here they are not
/// shapes at all, and the C interface rejects them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    /// Ranks of up to ten, or as many as fit the drag.
    Line,
    /// A square, also widened by a drag.
    Block,
    /// Two files deep.
    Column,
    /// An arrowhead, one unit at the tip.
    Wedge,
}

impl Shape {
    /// Every shape, in `SHAPES` order.
    pub const ALL: [Shape; 4] = [Shape::Line, Shape::Block, Shape::Column, Shape::Wedge];

    /// The name the menu and the JavaScript use.
    pub fn name(self) -> &'static str {
        SHAPES[self as usize]
    }

    pub fn from_name(name: &str) -> Option<Shape> {
        Shape::ALL.into_iter().find(|shape| shape.name() == name)
    }
}

/// A unit to place (`{ id, x, z, size, back }`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Item {
    pub id: u32,
    pub x: f64,
    pub z: f64,
    /// Its slot width, usually `slot_size` of its kind.
    pub size: f64,
    /// Takes the rear ranks (mortars, rockets, medics).
    pub back: bool,
}

/// Where one unit should stand (`[id, x, z]`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spot {
    pub id: u32,
    pub x: f64,
    pub z: f64,
}

/// `normalizeFace`: the same direction in (-pi, pi], or `None` for a direction that is not finite.
pub fn normalize_face(a: f64) -> Option<f64> {
    if !a.is_finite() {
        return None;
    }
    let f = math::atan2(math::sin(a), math::cos(a));
    Some(if f == -std::f64::consts::PI {
        std::f64::consts::PI
    } else {
        f
    })
}

/// `slotSize`: a unit's diameter plus room between hulls and squads, at least the sim's minimum.
pub fn slot_size(infantry: bool, radius: f64) -> f64 {
    let least = if infantry {
        INFANTRY_SLOT
    } else {
        VEHICLE_SLOT
    };
    math::max(least, 2.0 * radius + SLOT_MARGIN)
}

/// `rankSizes`: how many stand in each rank, front first. `fit` is how many the drag has room for
/// side by side; it may be NaN or infinite. A NaN `fit` (an average slot times spread that is NaN,
/// or 0 with no drag) gives no ranks at all, as `Array.from({ length: NaN })` does in JavaScript.
fn rank_sizes(shape: Shape, n: usize, fit: f64) -> Vec<usize> {
    if shape == Shape::Wedge {
        let mut ranks = Vec::new();
        let (mut k, mut left) = (1, n);
        while left > 0 {
            ranks.push(k.min(left));
            left = left.saturating_sub(k);
            k += 1;
        }
        return ranks;
    }
    let n = n as f64;
    let base = match shape {
        Shape::Column => math::min(n, 2.0),
        Shape::Block => n.sqrt().ceil(),
        _ => (n / (n / LINE_RANK).ceil()).ceil(),
    };
    let cols = if shape == Shape::Column {
        base
    } else {
        math::max(base, math::min(n, fit))
    };
    // ToLength: NaN and anything below 1 make an empty array.
    let count = (n / cols).ceil();
    let count = if count >= 1.0 { count as usize } else { 0 };
    (0..count)
        .map(|i| math::min(cols, n - i as f64 * cols) as usize)
        .collect()
}

/// JavaScript truthiness for a number: false for 0, -0 and NaN.
fn truthy(x: f64) -> bool {
    !(x == 0.0 || x.is_nan())
}

/// `facingSpots`: one spot per item, rank by rank from the front. The ranks run across `face` (radians;
/// 0 faces +x, pi/2 faces +z), the first through `(at_x, at_z)`, later ones behind it. `reach`, the
/// drag's length, widens a line or block to as many side by side as fit, and spreads a single rank
/// out to at most three times its natural width. `spread` scales every gap. Units marked `back` take
/// the rear ranks; otherwise those already furthest forward stand in front, and each rank keeps its
/// units' left to right order so their paths do not cross.
///
/// The result is empty when `items` is, and also for a line or block whose average slot times
/// `spread` is NaN, or is 0 while `reach` is 0 or not finite (see `rank_sizes`).
pub fn facing_spots(
    items: &[Item],
    at_x: f64,
    at_z: f64,
    face: f64,
    reach: f64,
    shape: Shape,
    spread: f64,
) -> Vec<Spot> {
    if items.is_empty() {
        return Vec::new();
    }
    let dx = math::cos(face);
    let dz = math::sin(face);
    let (ax, az) = (-dz, dx);
    let side = |u: &Item| u.x * ax + u.z * az;
    let avg = items.iter().fold(0.0, |s, u| s + u.size) / items.len() as f64 * spread;
    let fixed = matches!(shape, Shape::Column | Shape::Wedge);
    let far = if reach.is_finite() && !fixed {
        reach
    } else {
        0.0
    };
    let sizes = rank_sizes(shape, items.len(), (far / avg).floor());

    // Rear units last, then the furthest forward first (to the centimetre), then input order.
    let mut order: Vec<usize> = (0..items.len()).collect();
    sort::sort_by(&mut order, |a, b| {
        let (ua, ub) = (&items[a], &items[b]);
        let back = f64::from(u8::from(ua.back)) - f64::from(u8::from(ub.back));
        if truthy(back) {
            return back;
        }
        let ahead =
            math::round((ub.x - ua.x) * dx * DEPTH_STEPS + (ub.z - ua.z) * dz * DEPTH_STEPS);
        if truthy(ahead) {
            return ahead;
        }
        a as f64 - b as f64
    });

    let mut out = Vec::with_capacity(items.len());
    let mut back = 0.0;
    let mut start = 0;
    for k in sizes {
        // Each rank from left to right, then input order.
        let mut rank = order[start..start + k].to_vec();
        start += k;
        sort::sort_by(&mut rank, |a, b| {
            let d = side(&items[a]) - side(&items[b]);
            if truthy(d) { d } else { a as f64 - b as f64 }
        });
        let natural = rank.iter().fold(0.0, |s, &i| s + items[i].size);
        let tight = natural * spread;
        let width = if rank.len() > 1 {
            math::max(tight, math::min(MAX_STRETCH * tight, far))
        } else {
            natural
        };
        let gap = if rank.len() > 1 {
            (width - natural) / (rank.len() - 1) as f64
        } else {
            0.0
        };
        let mut along = -width / 2.0;
        for &i in &rank {
            let u = &items[i];
            along += u.size / 2.0;
            out.push(Spot {
                id: u.id,
                x: at_x + ax * along - dx * back,
                z: at_z + az * along - dz * back,
            });
            along += u.size / 2.0 + gap;
        }
        // A later rank leaves the deepest slot above it clear, with another 1.5 m between ranks.
        let deepest = rank
            .iter()
            .fold(f64::NEG_INFINITY, |m, &i| math::max(m, items[i].size));
        back += (deepest + RANK_GAP) * spread;
    }
    out
}
