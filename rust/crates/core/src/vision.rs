//! The vision pass: which enemy units each team sees. A port of `updateVision` in `shared/sim.js` at the
//! pin, with everything it calls (`visionOf`, `visionRange`, `reachDist`, `los`, `clear`, `hillsClear`,
//! `aimPoint`, `inStrip`, the camouflage and trench rules). Its answers match the JavaScript exactly
//! (`harness/vision-check.mjs`).
//!
//! Not ported yet, because Conquest does not use them: the Horde stragglers and boss shown through the fog,
//! the remembered enemy buildings of Classic and World Conquest (`g.ghosts`), and World Conquest's neutral
//! guards. A game in those modes gets everything else right and lacks those three.

use std::collections::HashMap;

use crate::game::{
    CELL, Game, ModeKind, PlaneState, SIGHT, Strike, StrikeKind, TRENCH, Team, Unit, WOOD,
};
use crate::grid::{SpatialGrid, UnitId};
use crate::math;
use crate::rules::{Rules, UnitKind};

/// Anything this close is seen, whatever stands between.
const CLOSE: f64 = 6.0;
/// An unfinished Scout Tower sees this far.
const UNFINISHED_LOOKOUT: f64 = 12.0;
/// Ticks without firing before a unit counts as quiet (4 s).
const QUIET_TICKS: f64 = 80.0;
/// Seconds standing still before camouflage hides a unit.
const CAMO_STILL: f64 = 3.0;
/// Ticks a vehicle's dust stays up (1 s).
const DUST_TICKS: f64 = 20.0;
/// The longest hull's half length: how much further the search reaches on a naval map (`HULL_MAX`).
const HULL_MAX: f64 = 55.0;
/// Sources share one spatial query per bucket of this many metres.
const NEAR_BUCKET: f64 = 16.0;
/// `-1e9`: a missing `shotAt` or `dust` tick.
const LONG_AGO: f64 = -1e9;

/// How far a unit sees before height and houses (`visionOf`): weather shortens it for everything on the
/// ground, and an unfinished lookout sees only a few metres.
pub fn vision_of(g: &Game, rules: &Rules, u: &Unit) -> f64 {
    let kind = &rules.kinds[u.kind];
    let base = if kind.lookout && u.built.is_some_and(|b| b < 1.0) {
        UNFINISHED_LOOKOUT
    } else {
        kind.vision
    };
    base * if u.air.is_some() {
        1.0
    } else {
        g.sight_mul(rules)
    }
}

/// A ground unit's sight (`visionRange`): high ground and an upper floor see farther, rain sees less.
pub fn vision_range(g: &Game, rules: &Rules, u: &Unit) -> f64 {
    let v = &rules.vision;
    let house = match u.garrison {
        // The reference throws on a house kind outside its table; here it sees as a plain house.
        Some(c) => g
            .house
            .get(c)
            .and_then(|&k| v.house_vision.get(usize::from(k)).copied().flatten())
            .unwrap_or(v.garrison_vision),
        None => 1.0,
    };
    vision_of(g, rules, u)
        * (1.0 + v.high_ground_vision * f64::from(g.level_at(u.x, u.z)))
        * house
        * (1.0 - v.rain_sight * g.rain)
}

/// A plane in the air: on its way out, on station or flying home (`airborne`).
pub fn airborne(u: &Unit) -> bool {
    matches!(
        u.air,
        Some(PlaneState::Out | PlaneState::Station | PlaneState::Home)
    )
}

/// A vehicle that drove over dry ground in the last second trails dust (`dusty`).
pub fn dusty(g: &Game, u: &Unit) -> bool {
    g.tick as f64 - u.dust.unwrap_or(LONG_AGO) < DUST_TICKS
}

/// Infantry standing in a trench cell (`inTrench`).
pub fn in_trench(g: &Game, rules: &Rules, u: &Unit) -> bool {
    rules.kinds[u.kind].infantry && g.flags_at(u.x, u.z) & TRENCH > 0
}

/// Whether a point lies in a `len` by `width` corridor centred on the strike, long side along its
/// direction (`inStrip`).
pub fn in_strip(s: &Strike, x: f64, z: f64, len: f64, width: f64) -> bool {
    let (cx, cz) = (math::cos(s.dir), math::sin(s.dir));
    let (dx, dz) = (x - s.x, z - s.z);
    (dx * cx + dz * cz).abs() <= len / 2.0 && (-dx * cz + dz * cx).abs() <= width / 2.0
}

fn dist(a: (f64, f64), b: (f64, f64)) -> f64 {
    math::hypot(a.0 - b.0, a.1 - b.1)
}

/// The point of `t` nearest to `a`: a ship is a segment along its heading, anything else a point
/// (`hullPoint`).
fn hull_point(rules: &Rules, a: (f64, f64), t: &Unit) -> (f64, f64) {
    let h = rules.kinds[t.kind].hull;
    if h == 0.0 || h.is_nan() {
        return (t.x, t.z);
    }
    let (cx, cz) = (math::cos(t.rot), math::sin(t.rot));
    let k = math::max(-h, math::min(h, (a.0 - t.x) * cx + (a.1 - t.z) * cz));
    (t.x + cx * k, t.z + cz * k)
}

/// Distance from `a` to the nearest point of `t` (`reachDist`).
fn reach_dist(rules: &Rules, a: &Unit, t: &Unit) -> f64 {
    dist((a.x, a.z), hull_point(rules, (a.x, a.z), t))
}

/// Where `u` looks at a structure: its nearest footprint cell still standing, since its own walls would
/// block a line to the middle (`aimPoint`, for a target with a footprint).
fn aim_point(g: &Game, u: &Unit, t: &Unit, cells: &[u32]) -> (f64, f64) {
    let mut best = (t.x, t.z);
    let mut best_d = f64::INFINITY;
    for &c in cells {
        if g.sections.get(&c).is_some_and(|s| s.failed || s.hp <= 0.0) {
            continue;
        }
        let w = g.w as u32;
        let p = (
            (f64::from(c % w) + 0.5) * CELL,
            (f64::from(c / w) + 0.5) * CELL,
        );
        let d = dist((u.x, u.z), p);
        if d < best_d {
            best_d = d;
            best = p;
        }
    }
    best
}

/// Line of sight from `a` to `b` (`los`): no sight-blocking cell between them (woods only a few cells
/// deep), no smoke cloud across a long enough line, and no hill above the line between the two eyes.
pub fn los(g: &Game, rules: &Rules, a: (f64, f64), b: (f64, f64)) -> bool {
    sight_clear(g, rules, a, b) && !smoked(g, rules, a, b) && hills_clear(g, rules, a, b)
}

/// `clear(g, a.x, a.z, b.x, b.z, SIGHT, false)`: walks the cells the line crosses, leaving out the first
/// and the last. Off the map, a sight-blocking cell, or one woods cell more than `wood_sight` stops it.
fn sight_clear(g: &Game, rules: &Rules, a: (f64, f64), b: (f64, f64)) -> bool {
    let (x0, z0, x1, z1) = (a.0, a.1, b.0, b.1);
    let mut cx = (x0 / CELL).floor() as i64;
    let mut cy = (z0 / CELL).floor() as i64;
    let ex = (x1 / CELL).floor() as i64;
    let ey = (z1 / CELL).floor() as i64;
    let (dx, dz) = (x1 - x0, z1 - z0);
    let (sx, sy) = (step_of(dx), step_of(dz));
    let tdx = if dx != 0.0 {
        CELL / dx.abs()
    } else {
        f64::INFINITY
    };
    let tdy = if dz != 0.0 {
        CELL / dz.abs()
    } else {
        f64::INFINITY
    };
    let mut tx = if dx != 0.0 {
        (if sx > 0 {
            (cx + 1) as f64 * CELL - x0
        } else {
            x0 - cx as f64 * CELL
        }) / dx.abs()
    } else {
        f64::INFINITY
    };
    let mut ty = if dz != 0.0 {
        (if sy > 0 {
            (cy + 1) as f64 * CELL - z0
        } else {
            z0 - cy as f64 * CELL
        }) / dz.abs()
    } else {
        f64::INFINITY
    };
    let (w, h) = (g.w as i64, g.h as i64);
    let mut wood = 0.0;
    let mut n = w + h;
    while n > 0 && !(cx == ex && cy == ey) {
        if cy == ey || (cx != ex && tx < ty) {
            tx += tdx;
            cx += sx;
        } else {
            ty += tdy;
            cy += sy;
        }
        if cx == ex && cy == ey {
            return true;
        }
        if cx < 0 || cy < 0 || cx >= w || cy >= h {
            return false;
        }
        let flags = g.flags[(cy * w + cx) as usize];
        if flags & SIGHT != 0 {
            return false;
        }
        if flags & WOOD != 0 {
            wood += 1.0;
            if wood > rules.vision.wood_sight {
                return false;
            }
        }
        n -= 1;
    }
    true
}

/// `Math.sign` for a grid walk's step.
pub(crate) fn step_of(d: f64) -> i64 {
    if d > 0.0 {
        1
    } else if d < 0.0 {
        -1
    } else {
        0
    }
}

/// Whether segment a-b passes through, or starts or ends inside, a circle (`segHits`).
pub(crate) fn seg_hits(a: (f64, f64), b: (f64, f64), c: (f64, f64), r: f64) -> bool {
    let (dx, dz) = (b.0 - a.0, b.1 - a.1);
    let l2 = or_one(dx * dx + dz * dz);
    let t = math::max(
        0.0,
        math::min(1.0, ((c.0 - a.0) * dx + (c.1 - a.1) * dz) / l2),
    );
    math::hypot(a.0 + dx * t - c.0, a.1 + dz * t - c.1) < r
}

/// `v || 1`.
pub(crate) fn or_one(v: f64) -> f64 {
    if v == 0.0 || v.is_nan() { 1.0 } else { v }
}

/// A smoke cloud across a line long enough for smoke to matter (`smoked`).
fn smoked(g: &Game, rules: &Rules, a: (f64, f64), b: (f64, f64)) -> bool {
    !g.smokes.is_empty()
        && math::hypot(b.0 - a.0, b.1 - a.1) >= rules.vision.smoke_sight
        && g.smokes.iter().any(|s| seg_hits(a, b, (s.x, s.z), s.r))
}

/// Hills block sight: the ground under the line between two eyes, sampled every half cell, must stay
/// below it (`hillsClear`).
fn hills_clear(g: &Game, rules: &Rules, a: (f64, f64), b: (f64, f64)) -> bool {
    if g.height.is_none() {
        return true;
    }
    let (ax, az, bx, bz) = (a.0, a.1, b.0, b.1);
    let l = rules.vision.level_height;
    let ya = f64::from(g.level_at(ax, az)) * l + rules.vision.eye;
    let yb = f64::from(g.level_at(bx, bz)) * l + rules.vision.eye;
    let n = (math::hypot(bx - ax, bz - az) / (CELL / 2.0)).ceil();
    let mut i = 1.0;
    while i < n {
        let t = i / n;
        let level = g.level_at(ax + (bx - ax) * t, az + (bz - az) * t);
        if f64::from(level) * l > ya + (yb - ya) * t {
            return false;
        }
        i += 1.0;
    }
    true
}

/// One of a team's eyes: a unit that can see, with its sight worked out once.
struct Source<'a> {
    u: &'a Unit,
    kind: &'a UnitKind,
    /// `visionOf`
    base: f64,
    /// `visionRange`
    range: f64,
}

/// The vision pass (`updateVision`): sets every player's `visible` to the enemy units its team sees, and
/// `vision_tick` to now. Teammates share one answer. `grid` must index `g.units` as they stand.
pub fn update_vision(g: &mut Game, rules: &Rules, grid: &SpatialGrid) {
    g.vision_tick = Some(g.tick);
    let seen = team_views(g, rules, grid);
    for (p, visible) in g.players.iter_mut().zip(seen) {
        p.visible = visible;
    }
}

/// Like `update_vision`, with each team's view worked out on a thread of its own. A team's view only reads
/// the game, so the views equal those of one team after another; they are handed out in player order.
pub fn update_vision_threaded(g: &mut Game, rules: &Rules, grid: &SpatialGrid) {
    g.vision_tick = Some(g.tick);
    let seen = {
        let g: &Game = g;
        let list: Vec<&Unit> = g.units.values().collect();
        let teams: Vec<Team> = list.iter().map(|u| g.team_of(u)).collect();
        let mut order: Vec<Team> = Vec::new();
        for p in &g.players {
            if !order.contains(&p.team) {
                order.push(p.team);
            }
        }
        let views: Vec<Vec<UnitId>> = std::thread::scope(|scope| {
            let (list, teams) = (&list, &teams);
            let threads: Vec<_> = order
                .iter()
                .map(|&team| scope.spawn(move || team_sees(g, rules, grid, list, teams, team)))
                .collect();
            threads
                .into_iter()
                .map(|t| t.join().expect("a vision thread panicked"))
                .collect()
        });
        g.players
            .iter()
            .map(|p| {
                let i = order.iter().position(|&t| t == p.team).expect("listed");
                views[i].clone()
            })
            .collect::<Vec<_>>()
    };
    for (p, visible) in g.players.iter_mut().zip(seen) {
        p.visible = visible;
    }
}

/// What each player sees, worked out once per team, in player order.
fn team_views(g: &Game, rules: &Rules, grid: &SpatialGrid) -> Vec<Vec<UnitId>> {
    let list: Vec<&Unit> = g.units.values().collect();
    let teams: Vec<Team> = list.iter().map(|u| g.team_of(u)).collect();
    let mut done: Vec<(Team, Vec<UnitId>)> = Vec::new();
    g.players
        .iter()
        .map(|p| {
            if let Some((_, seen)) = done.iter().find(|(team, _)| *team == p.team) {
                return seen.clone();
            }
            let seen = team_sees(g, rules, grid, &list, &teams, p.team);
            done.push((p.team, seen.clone()));
            seen
        })
        .collect()
}

/// The enemy units one team sees, in the units' order.
fn team_sees(
    g: &Game,
    rules: &Rules,
    grid: &SpatialGrid,
    list: &[&Unit],
    teams: &[Team],
    team: Team,
) -> Vec<UnitId> {
    let v = &rules.vision;
    // Planes on the ground see nothing; airborne planes see across terrain.
    let sources: Vec<Source> = list
        .iter()
        .zip(teams)
        .filter(|(u, t)| **t == team && (u.air.is_none() || airborne(u)))
        .map(|(u, _)| Source {
            u,
            kind: &rules.kinds[u.kind],
            base: vision_of(g, rules, u),
            range: vision_range(g, rules, u),
        })
        .collect();
    // `list` is in ascending id order, so a unit's place in it is a binary search away.
    let place = |id: UnitId| {
        list.binary_search_by_key(&id, |u| u.id)
            .expect("indexed unit")
    };
    let air_list: Vec<usize> = (0..list.len()).filter(|&i| list[i].air.is_some()).collect();
    // Gather observers once, in the sources' order; each target's check then visits only the sources near
    // it. One query per 16 m bucket and reach, shared by every source standing there; aircraft are
    // appended whole. On a naval map it reaches a ship's half length further.
    let mut watchers: Vec<Vec<usize>> = vec![Vec::new(); list.len()];
    let mut near_cache: HashMap<(i64, i64, u64), Vec<usize>> = HashMap::new();
    for (si, s) in sources.iter().enumerate() {
        let u = s.u;
        let reach = math::max(
            math::max(math::max(CLOSE, v.camo_range), s.kind.vision),
            s.range,
        ) * v.dust_seen;
        let bx = (u.x / NEAR_BUCKET).floor() as i64;
        let bz = (u.z / NEAR_BUCKET).floor() as i64;
        let near = near_cache
            .entry((bx, bz, key_bits(reach)))
            .or_insert_with(|| {
                let at = (
                    (bx as f64 + 0.5) * NEAR_BUCKET,
                    (bz as f64 + 0.5) * NEAR_BUCKET,
                );
                let radius = reach + 8.0 + if g.naval { HULL_MAX } else { 0.0 };
                let mut near: Vec<usize> = grid
                    .unordered(&g.units, at, radius, |_, t| {
                        t.air.is_none() && g.team_of(t) != team
                    })
                    .into_iter()
                    .map(place)
                    .collect();
                near.extend_from_slice(&air_list);
                near
            });
        for &ti in near.iter() {
            if teams[ti] == team {
                continue;
            }
            let t = list[ti];
            let bound = if t.air.is_some() {
                v.air_see_range
            } else {
                reach + rules.kinds[t.kind].hull
            };
            if (t.x - u.x).abs() > bound || (t.z - u.z).abs() > bound {
                continue;
            }
            watchers[ti].push(si);
        }
    }
    let recon: Vec<&Strike> = g
        .strikes
        .iter()
        .filter(|s| s.live && s.kind == StrikeKind::Recon && g.players[s.owner].team == team)
        .collect();
    let mut seen_ids = Vec::new();
    for (ti, t) in list.iter().enumerate() {
        // a squad inside a carrier is not seen
        if teams[ti] == team || t.riding {
            continue;
        }
        let tk = &rules.kinds[t.kind];
        if g.mode != Some(ModeKind::World) && tk.structure && !tk.building {
            seen_ids.push(t.id);
            continue;
        }
        if t.air.is_some() {
            if airborne(t)
                && watchers[ti].iter().any(|&si| {
                    dist((sources[si].u.x, sources[si].u.z), (t.x, t.z)) <= v.air_see_range
                })
            {
                seen_ids.push(t.id);
            }
            continue;
        }
        // camouflage: after 3 s still and 4 s without firing, only seen within camo_range (planes and
        // recon flights still spot it)
        let quiet = g.tick as f64 - t.shot_at.unwrap_or(LONG_AGO) >= QUIET_TICKS;
        let hidden = tk.camo && t.still.is_some_and(|s| s >= CAMO_STILL) && quiet;
        // down in a trench and not firing: seen only up close (planes and recon still see it)
        let sunk = !hidden && quiet && in_trench(g, rules, t);
        let seen = watchers[ti].iter().any(|&si| {
            let s = &sources[si];
            let u = s.u;
            let d = reach_dist(rules, u, t);
            if u.air.is_some() {
                d <= s.kind.vision && (!hidden || d < v.camo_range * 2.0)
            } else if hidden {
                d < v.camo_range
            } else if sunk {
                d < v.trench_hide && los(g, rules, (u.x, u.z), (t.x, t.z))
            } else {
                d < CLOSE
                    || (s.kind.building && d <= s.base)
                    || (d <= s.range * if dusty(g, t) { v.dust_seen } else { 1.0 }
                        && los(
                            g,
                            rules,
                            (u.x, u.z),
                            match &t.cells {
                                Some(cells) => aim_point(g, u, t, cells),
                                None => (t.x, t.z),
                            },
                        ))
            }
        });
        if seen
            || recon
                .iter()
                .any(|s| in_strip(s, t.x, t.z, v.recon_len, v.recon_width))
        {
            seen_ids.push(t.id);
        }
    }
    seen_ids
}

/// A number as a cache key: equal numbers give equal keys, as the reference's string keys do.
fn key_bits(v: f64) -> u64 {
    if v.is_nan() {
        f64::NAN.to_bits()
    } else if v == 0.0 {
        0
    } else {
        v.to_bits()
    }
}
