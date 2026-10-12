//! What the ground means for a unit on the move: whether a straight line is walkable, how fast a unit goes
//! over a cell, and how much cover a spot gives. Ports of `clear`, `blocker`, `walkable`, `noCliffs`,
//! `heightAt`, `groundMul`, `speedMul`, `coverRank`, `coverBehind` (without vehicles), `trenchFaces`,
//! `coverQ`, `maxHp` and `inCover` in `shared/sim.js`, and `segmentCliffClear` in `shared/vehicle-motion.js`,
//! at the pin.

use crate::game::{
    CELL, COVER, FORD, Game, MOVE, MUD, Point, ROAD, SIGHT, TRENCH, Unit, WIRE, WOOD,
};
use crate::math;
use crate::rules::Rules;
use crate::vision::{or_one, step_of};

/// Whether a straight line crosses no cell with any of the `mask` bits (`clear`). The first cell never
/// counts; the last only with `with_end`. Off the map stops it; with `SIGHT` in the mask, so does one woods
/// cell more than `wood_sight`.
pub fn clear(
    g: &Game,
    rules: &Rules,
    a: (f64, f64),
    b: (f64, f64),
    mask: u16,
    with_end: bool,
) -> bool {
    let (x0, z0, x1, z1) = (a.0, a.1, b.0, b.1);
    let mut cx = (x0 / CELL).floor();
    let mut cy = (z0 / CELL).floor();
    let ex = (x1 / CELL).floor();
    let ey = (z1 / CELL).floor();
    let (dx, dz) = (x1 - x0, z1 - z0);
    let (sx, sy) = (math::sign(dx), math::sign(dz));
    let tdx = if truthy(dx) {
        CELL / dx.abs()
    } else {
        f64::INFINITY
    };
    let tdy = if truthy(dz) {
        CELL / dz.abs()
    } else {
        f64::INFINITY
    };
    let mut tx = if truthy(dx) {
        (if sx > 0.0 {
            (cx + 1.0) * CELL - x0
        } else {
            x0 - cx * CELL
        }) / dx.abs()
    } else {
        f64::INFINITY
    };
    let mut ty = if truthy(dz) {
        (if sy > 0.0 {
            (cy + 1.0) * CELL - z0
        } else {
            z0 - cy * CELL
        }) / dz.abs()
    } else {
        f64::INFINITY
    };
    let (w, h) = (g.w as f64, g.h as f64);
    let mut wood = 0.0;
    let mut n = w + h;
    while n > 0.0 && !(cx == ex && cy == ey) {
        if cy == ey || (cx != ex && tx < ty) {
            tx += tdx;
            cx += sx;
        } else {
            ty += tdy;
            cy += sy;
        }
        let end = cx == ex && cy == ey;
        if end && !with_end {
            return true;
        }
        if cx < 0.0 || cy < 0.0 || cx >= w || cy >= h {
            return false;
        }
        // a NaN cell reads as no flags, as `g.flags[NaN]` does (positions are never NaN in play)
        let flags = if cx.is_nan() || cy.is_nan() {
            0
        } else {
            g.flags[(cy * w + cx) as usize]
        };
        if flags & mask != 0 {
            return false;
        }
        if mask & SIGHT != 0 && flags & WOOD != 0 {
            wood += 1.0;
            if wood > rules.vision.wood_sight {
                return false;
            }
        }
        n -= 1.0;
    }
    true
}

/// JavaScript truthiness of a number: not 0 and not NaN.
pub(crate) fn truthy(v: f64) -> bool {
    v != 0.0 && !v.is_nan()
}

/// Whether a unit `2 * half` wide can go straight from `a` to `b` (`walkable`): three parallel lines clear
/// of `mask`, so wide units do not clip corners, and no cliff on the way.
pub fn walkable(g: &Game, rules: &Rules, a: Point, b: Point, mask: u16, half: f64) -> bool {
    let d = or_one(math::hypot(b.x - a.x, b.z - a.z));
    let ox = -(b.z - a.z) / d * half;
    let oz = (b.x - a.x) / d * half;
    [-1.0, 0.0, 1.0].iter().all(|&k| {
        clear(
            g,
            rules,
            (a.x + ox * k, a.z + oz * k),
            (b.x + ox * k, b.z + oz * k),
            mask,
            true,
        )
    }) && segment_cliff_clear(g, a, b)
}

/// No step of more than one height level between neighbouring cells along a straight segment, visiting
/// every cell it crosses (`segmentCliffClear`, which `noCliffs` calls). Off the map fails.
pub fn segment_cliff_clear(g: &Game, from: Point, to: Point) -> bool {
    let Some(heights) = &g.height else {
        return true;
    };
    if from.x.is_nan() || from.z.is_nan() || to.x.is_nan() || to.z.is_nan() {
        // the reference walks off into undefined cells and ends false
        return false;
    }
    let cell = CELL;
    let (w, h) = (g.w as i64, g.h as i64);
    let mut x = (from.x / cell).floor() as i64;
    let mut y = (from.z / cell).floor() as i64;
    let ex = (to.x / cell).floor() as i64;
    let ey = (to.z / cell).floor() as i64;
    let (dx, dz) = (to.x - from.x, to.z - from.z);
    let (sx, sy) = (step_of(dx), step_of(dz));
    let step_x = if truthy(dx) {
        cell / dx.abs()
    } else {
        f64::INFINITY
    };
    let step_y = if truthy(dz) {
        cell / dz.abs()
    } else {
        f64::INFINITY
    };
    let mut tx = if truthy(dx) {
        (if sx > 0 {
            (x + 1) as f64 * cell - from.x
        } else {
            from.x - x as f64 * cell
        }) / dx.abs()
    } else {
        f64::INFINITY
    };
    let mut ty = if truthy(dz) {
        (if sy > 0 {
            (y + 1) as f64 * cell - from.z
        } else {
            from.z - y as f64 * cell
        }) / dz.abs()
    } else {
        f64::INFINITY
    };
    let off = |x: i64, y: i64| x < 0 || y < 0 || x >= w || y >= h;
    let at = |x: i64, y: i64| i64::from(heights[(y * w + x) as usize]);
    if off(x, y) {
        return false;
    }
    let mut height = at(x, y);
    let mut n = w + h;
    while n > 0 && (x != ex || y != ey) {
        if x != ex && y != ey && (tx - ty).abs() < 1e-12 {
            let (nx, ny) = (x + sx, y + sy);
            if off(nx, ny) {
                return false;
            }
            if [at(nx, y), at(x, ny), at(nx, ny)]
                .iter()
                .any(|next| (next - height).abs() > 1)
            {
                return false;
            }
            x = nx;
            y = ny;
            tx += step_x;
            ty += step_y;
        } else if y == ey || (x != ex && tx < ty) {
            x += sx;
            tx += step_x;
        } else {
            y += sy;
            ty += step_y;
        }
        if off(x, y) {
            return false;
        }
        let next = at(x, y);
        if (next - height).abs() > 1 {
            return false;
        }
        height = next;
        n -= 1;
    }
    x == ex && y == ey
}

/// Ground height in metres, smooth between cell centres (`heightAt`). The map must have heights.
pub fn height_at(g: &Game, rules: &Rules, heights: &[i8], x: f64, z: f64) -> f64 {
    let (w, h) = (g.w as f64, g.h as f64);
    let fx = math::min(w - 1.0, math::max(0.0, x / CELL - 0.5));
    let fz = math::min(h - 1.0, math::max(0.0, z / CELL - 0.5));
    let (x0, z0) = (fx.floor(), fz.floor());
    let x1 = math::min(w - 1.0, x0 + 1.0);
    let z1 = math::min(h - 1.0, z0 + 1.0);
    let (tx, tz) = (fx - x0, fz - z0);
    let at = |cx: f64, cz: f64| f64::from(heights[(cz * w + cx) as usize]);
    (at(x0, z0) * (1.0 - tx) * (1.0 - tz)
        + at(x1, z0) * tx * (1.0 - tz)
        + at(x0, z1) * (1.0 - tx) * tz
        + at(x1, z1) * tx * tz)
        * rules.vision.level_height
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

/// How wet a cell's ground is: the living ground's wetness, doubled in depressions (0 without weather).
pub fn wet_at(g: &Game, c: usize) -> f64 {
    match g.wet {
        Some(wet) => {
            wet * if g.height.as_ref().is_some_and(|h| h[c] < 0) {
                2.0
            } else {
                1.0
            }
        }
        None => 0.0,
    }
}

/// How fast a unit moves on one cell, 1 being open dry ground (`groundMul`).
pub fn ground_mul(g: &Game, rules: &Rules, c: usize, vehicle: bool) -> f64 {
    let m = &rules.movement;
    let f = g.flags[c];
    let w = g.wear.as_ref().map_or(0.0, |wear| f64::from(wear[c]));
    let wet = wet_at(g, c);
    if f & FORD != 0 {
        return lerp(m.ford_speed[0], m.ford_speed[1], w)
            * (1.0 - m.wet_ford * math::min(1.0, wet));
    }
    if !vehicle {
        return if f & WIRE != 0 {
            m.wire_speed
        } else if f & TRENCH != 0 {
            m.trench_speed
        } else {
            1.0
        };
    }
    if f & WOOD != 0 {
        return m.wood_speed;
    }
    if f & ROAD != 0 {
        return lerp(m.road_speed, 1.0, w);
    }
    if f & MUD != 0 {
        return lerp(m.mud_speed[0], m.mud_speed[1], w) * (1.0 - m.wet_ground * wet);
    }
    (1.0 - m.churn * w) * (1.0 - m.wet_ground * wet)
}

/// The cells a unit's speed is averaged over: its own and the four beside it.
const FOOT: [(f64, f64); 5] = [(0.0, 0.0), (1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)];

/// A unit's ground speed factor: the average over the walkable cells it stands on (`speedMul`).
pub fn speed_mul(g: &Game, rules: &Rules, u: &Unit) -> f64 {
    let kind = &rules.kinds[u.kind];
    let r = kind.radius * 0.6;
    if kind.naval {
        return 1.0;
    }
    let (mut sum, mut n) = (0.0, 0.0);
    for (dx, dz) in FOOT {
        if let Some(c) = g.cell_of(u.x + dx * r, u.z + dz * r)
            && g.flags[c] & MOVE == 0
        {
            sum += ground_mul(g, rules, c, !kind.infantry);
            n += 1.0;
        }
    }
    if n != 0.0 { sum / n } else { 1.0 }
}

/// Infantry standing in a cover cell (`inCover`).
pub fn in_cover(g: &Game, rules: &Rules, u: &Unit) -> bool {
    rules.kinds[u.kind].infantry && g.flags_at(u.x, u.z) & COVER > 0
}

/// A structure cell's full hit points (`maxHp`).
pub fn max_hp(g: &Game, rules: &Rules, c: usize) -> f64 {
    let m = &rules.movement;
    match g.sections.get(&(c as u32)) {
        Some(s) if !s.failed => s.max_hp,
        _ => None,
    }
    .unwrap_or_else(|| {
        let ch = g.physical(c);
        if ch == b'B' {
            m.house_hp
                .get(usize::from(g.house[c]))
                .copied()
                .unwrap_or(f64::NAN)
        } else {
            m.terrain_hp(ch).unwrap_or(0.0) * g.tough.as_ref().map_or(1.0, |t| t[c])
        }
    })
}

/// How much of its protection a cover cell still gives (`coverQ`): woods a fixed share, a crater more the
/// deeper it is, walls and hedges less as they are shot up.
pub fn cover_q(g: &Game, rules: &Rules, c: usize) -> f64 {
    let ch = g.physical(c);
    let max = max_hp(g, rules, c);
    if ch == b'O' {
        return rules.movement.wood_cover;
    }
    if ch == b'+' {
        math::min(
            1.4,
            0.55 + g.wear.as_ref().map_or(0.45, |w| f64::from(w[c])),
        )
    } else if max > 1.0 {
        0.5 + 0.5 * math::max(0.0, f64::from(g.cell_hp[c])) / max
    } else {
        1.0
    }
}

/// Whether the trench at `at` faces fire from `from` (`trenchFaces`): no shooter known, a map-drawn trench,
/// or fire within the trench's arc.
pub fn trench_faces(g: &Game, rules: &Rules, at: Point, from: Option<Point>) -> bool {
    let Some(from) = from else {
        return true;
    };
    let front = g.trench_front.as_ref().and_then(|t| {
        let c = g.cell_index(at.x, at.z);
        if c < 0 {
            None
        } else {
            Some(f64::from(t[c as usize]))
        }
    });
    match front {
        None => true,
        Some(f) if f.is_nan() => true,
        Some(f) => {
            math::cos(math::atan2(from.z - at.z, from.x - at.x) - f) > rules.movement.trench_arc
        }
    }
}

/// Letters of something solid to hide behind (`SOLID`).
fn solid(ch: u8) -> bool {
    matches!(ch, b'B' | b'#' | b'R' | b'H' | b'K' | b'Q')
}

/// House and building walls (`WALLS`).
fn wall(ch: u8) -> bool {
    matches!(ch, b'B' | b'K')
}

const AROUND: [(i64, i64); 8] = [
    (1, 0),
    (-1, 0),
    (0, 1),
    (0, -1),
    (1, 1),
    (1, -1),
    (-1, 1),
    (-1, -1),
];

/// Cover at `t` from something solid between it and fire from `from`, 0 to 1 (`coverBehind` with
/// `units = false`: the ground and settled wrecks, not vehicles).
pub fn cover_behind(g: &Game, rules: &Rules, t: Point, from: Point) -> f64 {
    let a = math::atan2(from.z - t.z, from.x - t.x);
    let (ca, sa) = (math::cos(a), math::sin(a));
    let mut best = 0.0;
    let mut d = 0.8;
    while d < 3.7 {
        if let Some(c) = g.cell_of(t.x + ca * d, t.z + sa * d)
            && solid(g.physical(c))
        {
            best = math::min(1.0, (3.8 - d) / 1.6) * math::min(1.0, cover_q(g, rules, c));
            break;
        }
        d += 0.7;
    }
    // leaning out from a corner: a house wall in a cell next to it, within 60 degrees of the shooter
    let (x, y) = ((t.x / CELL).floor() as i64, (t.z / CELL).floor() as i64);
    let (w, h) = (g.w as i64, g.h as i64);
    for (dx, dy) in AROUND {
        if x + dx < 0 || y + dy < 0 || x + dx >= w || y + dy >= h {
            continue;
        }
        if !wall(g.physical(((y + dy) * w + x + dx) as usize)) {
            continue;
        }
        let (fx, fy) = (dx as f64, dy as f64);
        if (fx * ca + fy * sa) / math::hypot(fx, fy) > 0.5 {
            best = 1.0;
        }
    }
    for v in g.wrecks.iter().filter(|v| !v.moving) {
        let (dx, dz) = (v.x - t.x, v.z - t.z);
        let d = math::hypot(dx, dz);
        if d > 0.0 && d < 4.0 && (dx * ca + dz * sa) / d > 0.7 {
            best = math::max(best, math::min(1.0, (4.0 - d) / 1.5));
        }
    }
    best
}

/// How well a spot shelters infantry from fire from `from` (`coverRank`): 0 a trench facing it, 1 a cover
/// cell, 2 behind something solid on that side, 3 open ground.
pub fn cover_rank(g: &Game, rules: &Rules, at: Point, from: Option<Point>) -> u8 {
    let f = g.flags_at(at.x, at.z);
    if f & TRENCH != 0 && trench_faces(g, rules, at, from) {
        0
    } else if f & COVER != 0 {
        1
    } else if from.is_some_and(|from| cover_behind(g, rules, at, from) > 0.4) {
        2
    } else {
        3
    }
}
