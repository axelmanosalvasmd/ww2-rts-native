//! Crowd separation after the unit loop: units closer than their sizes allow push each other apart, squads
//! gently, vehicles hard. A port of `separateCrowds` in `shared/sim.js` at the pin, with `nearCrossing` and
//! `shovedFromCover`. It visits units in id order and each pair once, in the order the reference does (the
//! nearer units of each in the units' order, looked up again when a push moves the search square), so the
//! pushes add up to the same positions.

use crate::game::{COVER, FORD, Game, Point, Pose, Unit};
use crate::grid::{SpatialGrid, UnitId};
use crate::math;
use crate::rules::Rules;
use crate::terrain::in_cover;
use crate::traffic::ghost_pair;
use crate::vehicle::swept_vehicle_clear;
use crate::vision::or_one;

/// The separation index's bucket size: fine 4 m cells, since the query is tiny and runs for every unit.
const SEP_BUCKET: f64 = 4.0;

/// Next to a bridge or a ford (`nearCrossing`): squads crossing the same way keep their forward step.
fn near_crossing(g: &Game, u: &Unit) -> bool {
    let (w, h) = (g.w as i64, g.h as i64);
    let c = g.cell_index(u.x, u.z);
    // off the map, the reference looks around cell -1: x = -1, y = -1
    let (x, y) = (c % w, c.div_euclid(w));
    for ny in 0.max(y - 1)..=(h - 1).min(y + 1) {
        for nx in 0.max(x - 1)..=(w - 1).min(x + 1) {
            let n = (ny * w + nx) as usize;
            if g.chars[n] == b'=' || g.flags[n] & FORD != 0 {
                return true;
            }
        }
    }
    false
}

/// A squad in cover that a push would take out of it (`shovedFromCover`).
fn shoved_from_cover(g: &Game, rules: &Rules, u: &Unit, x: f64, z: f64) -> bool {
    in_cover(g, rules, u) && g.flags_at(x, z) & COVER == 0
}

/// Whether `u` may be pushed by (px, pz): not holding (`held`), not into a cell it cannot enter, a vehicle's
/// hull not through a wall, and not up or down a cliff.
fn may_push(g: &Game, rules: &Rules, u: &Unit, px: f64, pz: f64) -> bool {
    let kind = &rules.kinds[u.kind];
    let block = kind.block();
    let (x, z) = (u.x + px, u.z + pz);
    g.flags_at(x, z) & block == 0
        && (!kind.ground_vehicle()
            || swept_vehicle_clear(g, u.pose(), Pose { x, z, rot: u.rot }, kind, block))
        && (i32::from(g.level_at(x, z)) - i32::from(g.level_at(u.x, u.z))).abs() <= 1
}

/// The search square's bucket bounds around a unit.
fn bounds(x: f64, z: f64, radius: f64) -> [f64; 4] {
    [
        ((x - radius) / SEP_BUCKET).floor(),
        ((x + radius) / SEP_BUCKET).floor(),
        ((z - radius) / SEP_BUCKET).floor(),
        ((z + radius) / SEP_BUCKET).floor(),
    ]
}

/// Pushes overlapping units apart (`separateCrowds`). `grid` is the game's spatial index, kept up to date.
pub fn separate_crowds(g: &mut Game, rules: &Rules, grid: &mut SpatialGrid) {
    let ids: Vec<UnitId> = g.units.keys().copied().collect();
    let max_radius = g
        .units
        .values()
        .fold(0.0, |m, u| math::max(m, rules.kinds[u.kind].radius));
    let mut sep = SpatialGrid::new(&g.units, SEP_BUCKET);
    for &a_id in &ids {
        let a = &g.units[&a_id];
        let radius = (rules.kinds[a.kind].radius + max_radius) * 0.8;
        if a.garrison.is_some()
            || a.air.is_some()
            || a.riding
            || a.rail
            || crate::match_gates::open_passage(g, &rules.kinds[a.kind], a)
        {
            continue;
        }
        let mut square = bounds(a.x, a.z, radius);
        // the units after `a` in the units' order (ascending id) near it
        let mut candidates = sep.in_order(&g.units, (a.x, a.z), radius, |id, _| id > a_id);
        let mut k = 0;
        while k < candidates.len() {
            let b_id = candidates[k];
            k += 1;
            let (a, b) = (&g.units[&a_id], &g.units[&b_id]);
            let (ak, bk) = (&rules.kinds[a.kind], &rules.kinds[b.kind]);
            let min = (ak.radius + bk.radius) * 0.8;
            if a.garrison.is_some()
                || b.garrison.is_some()
                || a.air.is_some()
                || b.air.is_some()
                || b.riding
                || b.rail
                || crate::match_gates::open_passage(g, &rules.kinds[b.kind], b)
            {
                continue;
            }
            let (mut dx, mut dz) = (b.x - a.x, b.z - a.z);
            let mut d = math::hypot(dx, dz);
            if d >= min {
                continue;
            }
            if d == 0.0 {
                dx = if a.id < b.id { 1e-6 } else { -1e-6 };
                dz = 0.0;
                d = 1e-6;
            }
            let (sa, sb) = (ak.structure, bk.structure);
            if sa && sb {
                continue;
            }
            // two vehicles push hard, unless they are friends and one of them is a ghost (sim.js's own
            // `ghostPair(g, a, b)`)
            let solid_contact = !ak.infantry
                && !bk.infantry
                && (ak.ground_vehicle() || bk.ground_vehicle())
                && !(g.allied(a.owner, b.owner) && ghost_pair(g, a, b, true));
            let push = (min - d) / 2.0
                * if solid_contact {
                    if sa || sb { 2.0 } else { 1.0 }
                } else if sa || sb {
                    1.0
                } else {
                    0.5
                };
            let (px, pz) = (dx / d * push, dz / d * push);
            let (mut apx, mut apz, mut bpx, mut bpz) = (px, pz, px, pz);
            if ak.infantry
                && bk.infantry
                && g.allied(a.owner, b.owner)
                && !a.path.is_empty()
                && !b.path.is_empty()
                && near_crossing(g, a)
                && near_crossing(g, b)
            {
                let goal_of = |u: &Unit| -> Point {
                    u.world_goal
                        .or(u.amove)
                        .or(u.path.last().copied())
                        .expect("a route has an end")
                };
                let (ga, gb) = (goal_of(a), goal_of(b));
                let (ax, az, bx, bz) = (ga.x - a.x, ga.z - a.z, gb.x - b.x, gb.z - b.z);
                if ax * bx + az * bz > math::hypot(ax, az) * math::hypot(bx, bz) * 0.5 {
                    // Same-way bridge traffic keeps lateral spacing without undoing either squad's forward step.
                    let (avx, avz) = (a.vx.unwrap_or(f64::NAN), a.vz.unwrap_or(f64::NAN));
                    let (bvx, bvz) = (b.vx.unwrap_or(f64::NAN), b.vz.unwrap_or(f64::NAN));
                    let against_a =
                        math::max(0.0, (px * avx + pz * avz) / or_one(avx * avx + avz * avz));
                    let against_b =
                        math::min(0.0, (px * bvx + pz * bvz) / or_one(bvx * bvx + bvz * bvz));
                    apx -= against_a * avx;
                    apz -= against_a * avz;
                    bpx -= against_b * bvx;
                    bpz -= against_b * bvz;
                }
            }
            // a squad holding its cover is not pushed off it, and does not push back at a friend walking past
            let ha = !sa
                && (a.hold_pos
                    || a.build
                    || a.dig
                    || a.entrench
                    || shoved_from_cover(g, rules, a, a.x - apx, a.z - apz));
            let hb = !sb
                && (b.hold_pos
                    || b.build
                    || b.dig
                    || b.entrench
                    || shoved_from_cover(g, rules, b, b.x + bpx, b.z + bpz));
            let move_a =
                !sa && !ha && !(hb && !a.path.is_empty()) && may_push(g, rules, a, -apx, -apz);
            let move_b =
                !sb && !hb && !(ha && !b.path.is_empty()) && may_push(g, rules, b, bpx, bpz);
            for (id, moves, px, pz) in [(a_id, move_a, -apx, -apz), (b_id, move_b, bpx, bpz)] {
                let u = g.units.get_mut(&id).expect("listed");
                if moves {
                    // `a.x -= apx` is `a.x + -apx` exactly
                    u.x += px;
                    u.z += pz;
                }
                if solid_contact {
                    u.move_speed = 0.0;
                    u.vx = Some(0.0);
                    u.vz = Some(0.0);
                }
            }
            for id in [a_id, b_id] {
                grid.update(id, &g.units[&id]);
                sep.update(id, &g.units[&id]);
            }
            // Units not visited yet have not moved this pass, so the candidates stay complete until a push
            // changes the search square.
            let a = &g.units[&a_id];
            let moved_square = bounds(a.x, a.z, radius);
            if moved_square != square {
                square = moved_square;
                candidates = sep.in_order(&g.units, (a.x, a.z), radius, |id, _| id > b_id);
                k = 0;
            }
        }
    }
}
