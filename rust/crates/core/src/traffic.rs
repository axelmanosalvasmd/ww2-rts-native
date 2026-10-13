//! Local traffic: how a unit about to run into another waits, queues behind a friend going its way, steps
//! aside, drives around a stopped unit, holds short of a one-lane stretch for an oncoming friend, or passes
//! through friends for a moment when boxed in. A port of `shared/local-traffic.js` at the pin (`trafficStep`
//! and what it calls, `rememberTrafficPosition`, `startGhost`, `ghostPair`). It leaves the route as it is,
//! except to skip waypoints already passed when a sidestep ends.

use crate::game::{Game, LaneScan, Point, Pose, Traffic, Unit};
use crate::grid::{SpatialGrid, UnitId};
use crate::math;
use crate::route_knowledge::KnowledgeError;
use crate::rules::{Rules, UnitKind};
use crate::traffic_knowledge::TrafficKnowledge;
use crate::vision::{airborne, or_one};

/// What traffic lets a unit do this tick (`trafficStep`'s result).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Steer {
    /// The point to head for now; `None` stands still.
    pub goal: Option<Point>,
    pub blocked: bool,
    /// Queued behind a friend or holding short of a one-lane stretch: not gridlock.
    pub waiting: bool,
    /// Heading for a sidestep (`goal` is off the route).
    pub yielding: bool,
    /// Blocked long enough to want a new route.
    pub replan: bool,
}

impl Steer {
    fn go(goal: Option<Point>) -> Steer {
        Steer {
            goal,
            ..Steer::default()
        }
    }
}

fn length(a: Point, b: Point) -> f64 {
    math::hypot(a.x - b.x, a.z - b.z)
}

/// Pass through friends until `seconds` from now (`startGhost`).
pub fn start_ghost(g: &Game, u: &mut Unit, seconds: f64, dt: f64) {
    u.ghost = g.tick as f64 + math::round(seconds / dt);
}

/// Two friends either of which is a ghost pass through each other (`ghostPair`).
pub fn ghost_pair(g: &Game, a: &Unit, b: &Unit, friends: bool) -> bool {
    let tick = g.tick as f64;
    (a.ghost > tick || b.ghost > tick) && friends
}

/// Who goes first: retreating units, then units on a route, then idle ones, with priority growing as a
/// unit waits (`priority`).
fn priority(rules: &Rules, u: &Unit) -> f64 {
    let base = if u.retreating {
        30.0
    } else if u.drift {
        0.0
    } else if !u.path.is_empty() {
        10.0
    } else {
        20.0
    };
    base + math::min(
        25.0,
        (u.traffic_wait / rules.movement.traffic.age).floor() * 5.0,
    )
}

/// Whether `a` has the right of way over `b` (`trafficWins`).
pub fn traffic_wins(rules: &Rules, a: &Unit, b: &Unit) -> bool {
    let (pa, pb) = (priority(rules, a), priority(rules, b));
    if pa != pb {
        return pa > pb;
    }
    if a.traffic_entered != b.traffic_entered {
        a.traffic_entered
    } else {
        a.id < b.id
    }
}

/// A unit traffic does not move: holding its position with nothing to do, in a house, riding, or at work
/// (`protectedUnit`).
fn protected(u: &Unit) -> bool {
    (u.hold_pos
        && u.path.is_empty()
        && u.world_goal.is_none()
        && u.amove.is_none()
        && !u.retreating)
        || u.garrison.is_some()
        || u.riding
        || u.build
        || u.dig
        || u.entrench
}

/// How close two units may come (`radiusPair`): squads by their size, anything else by hull length.
fn radius_pair(a: &UnitKind, b: &UnitKind) -> f64 {
    if a.infantry && b.infantry {
        (a.radius + b.radius) * 0.45
    } else {
        a.body_radius() + b.body_radius()
    }
}

/// Which way a unit's route runs over the next 8 m, as a unit vector (`heading`).
fn heading(u: &Unit) -> Option<Point> {
    let (mut x, mut z, mut left) = (u.x, u.z, 8.0);
    for p in &u.path {
        let d = length(Point { x, z }, *p);
        if d >= left {
            x += (p.x - x) * left / d;
            z += (p.z - z) * left / d;
            break;
        }
        left -= d;
        x = p.x;
        z = p.z;
    }
    let d = length(u.at(), Point { x, z });
    if d > 0.1 {
        Some(Point {
            x: (x - u.x) / d,
            z: (z - u.z) / d,
        })
    } else {
        None
    }
}

/// Whether `v` already waits on the unit `u_id`, directly or down a line of others (`waitsOn`).
fn waits_on(g: &Game, v: &Unit, u_id: UnitId) -> bool {
    let mut w = Some(v);
    let mut k = 0;
    while let Some(on) = w {
        if on.queued_behind == 0 || k >= 16 {
            break;
        }
        if on.queued_behind == u_id {
            return true;
        }
        w = g.units.get(&on.queued_behind);
        k += 1;
    }
    false
}

/// Open ground at a point for this kind (the traffic context's `open`).
fn open(g: &Game, knowledge: &TrafficKnowledge<'_>, block: u16, x: f64, z: f64) -> bool {
    knowledge.open(g, block, x, z)
}

/// Open ground across the route at (x, z), up to `need` on each side (`laneWidth`).
#[allow(clippy::too_many_arguments)]
fn lane_width(
    g: &Game,
    knowledge: &TrafficKnowledge<'_>,
    block: u16,
    x: f64,
    z: f64,
    nx: f64,
    nz: f64,
    need: f64,
) -> f64 {
    let mut width = 0.0;
    for side in [1.0, -1.0] {
        let mut k = 0.5;
        while k <= need
            && width < need
            && open(g, knowledge, block, x + nx * k * side, z + nz * k * side)
        {
            width += 0.5;
            k += 0.5;
        }
    }
    width
}

/// Where the route ahead first runs through ground too narrow for two of these hulls to pass, and where that
/// ends (`oneLane`).
fn one_lane(
    g: &Game,
    knowledge: &TrafficKnowledge<'_>,
    rules: &Rules,
    u: &Unit,
    kind: &UnitKind,
) -> Option<(Point, Point)> {
    let lane = &rules.movement.lane;
    let block = kind.block();
    let need = 0.96 * kind.radius + 2.0 * kind.body_radius() + lane.margin;
    let mut at = u.at();
    let mut travelled = 0.0;
    let mut entry: Option<Point> = None;
    let mut exit: Option<Point> = None;
    for &p in &u.path {
        let d = length(at, p);
        let nx = -(p.z - at.z) / or_one(d);
        let nz = (p.x - at.x) / or_one(d);
        let mut s = 0.0;
        while s < d {
            let x = at.x + (p.x - at.x) * s / d;
            let z = at.z + (p.z - at.z) * s / d;
            if lane_width(g, knowledge, block, x, z, nx, nz, need) < need {
                entry.get_or_insert(Point { x, z });
                exit = Some(Point { x, z });
            } else if let Some(entry) = entry {
                return Some((entry, exit.expect("set with entry")));
            }
            if travelled + s > lane.ahead {
                return entry.zip(exit);
            }
            s += 1.0;
        }
        travelled += d;
        at = p;
    }
    entry.zip(exit)
}

/// The route's next one-lane stretch, looked for again every half second or on a new route (`laneAhead`).
fn lane_ahead(
    g: &Game,
    knowledge: &TrafficKnowledge<'_>,
    rules: &Rules,
    u: &mut Unit,
    kind: &UnitKind,
) -> Option<(Point, Point)> {
    let tick = g.tick as f64;
    let stale = match &u.lane_scan {
        Some(scan) => scan.path != u.path_gen || tick >= scan.until,
        None => true,
    };
    if stale {
        let lane = one_lane(g, knowledge, rules, u, kind);
        u.lane_scan = Some(LaneScan {
            path: u.path_gen,
            until: tick + 10.0,
            lane,
        });
    }
    u.lane_scan.as_ref().and_then(|scan| scan.lane)
}

/// Whether the reference's `seenBy(g, slot, v.id)` holds for the unit `v`.
pub fn seen_by(g: &Game, slot: usize, v: &Unit) -> bool {
    g.reveal
        || g.allied(v.owner, slot)
        || (g.players[slot].sees(v.id) && (v.air.is_none() || airborne(v)))
}

/// The oncoming friend that has the one-lane stretch ahead of `u`, if `u` should hold short of it for one
/// (`laneHolder`).
fn lane_holder(
    g: &Game,
    knowledge: &TrafficKnowledge<'_>,
    rules: &Rules,
    grid: &SpatialGrid,
    u: &mut Unit,
    kind: &UnitKind,
) -> Option<UnitId> {
    let lr = &rules.movement.lane;
    let own = heading(u)?;
    let (entry, exit) = lane_ahead(g, knowledge, rules, u, kind)?;
    let u: &Unit = u;
    // Its nose already in it (or past its start): drive on through. Further than the hold point: carry on.
    let before = (entry.x - u.x) * own.x + (entry.z - u.z) * own.z;
    if before < kind.body_radius() + 0.5
        || length(u.at(), entry) > 2.0 * kind.body_radius() + lr.hold
    {
        return None;
    }
    let oncoming = grid.unordered(&g.units, (u.x, u.z), lr.ahead, |vid, v| {
        vid != u.id
            && v.hp > 0.0
            && v.air.is_none()
            && !v.riding
            && !v.path.is_empty()
            && g.allied(u.owner, v.owner)
            && seen_by(g, u.owner, v)
            && (v.x - u.x) * own.x + (v.z - u.z) * own.z > 0.0
            && heading(v).is_some_and(|h| h.x * own.x + h.z * own.z < -0.3)
    });
    if oncoming.is_empty() {
        return None;
    }
    let (sx, sz) = (exit.x - entry.x, exit.z - entry.z);
    let squared = sx * sx + sz * sz;
    // Who is nearer is measured to the 2 m cell in the middle of the stretch, so both sides agree.
    let mid = Point {
        x: (((entry.x + exit.x) / 4.0).floor() + 0.5) * 2.0,
        z: (((entry.z + exit.z) / 4.0).floor() + 0.5) * 2.0,
    };
    let near = length(u.at(), mid);
    oncoming.into_iter().find(|vid| {
        let v = &g.units[vid];
        let k = math::max(
            0.0,
            math::min(
                1.0,
                ((v.x - entry.x) * sx + (v.z - entry.z) * sz) / or_one(squared),
            ),
        );
        if math::hypot(entry.x + sx * k - v.x, entry.z + sz * k - v.z)
            <= rules.kinds[v.kind].body_radius() + 0.5
        {
            return true;
        }
        // Outside it, only one on its way (not waiting on anyone) and nearer to it has the stretch.
        let far = length(v.at(), mid);
        let h = heading(v).expect("oncoming units have a heading");
        v.queued_behind == 0
            && (mid.x - v.x) * h.x + (mid.z - v.z) * h.z > 0.0
            && (far < near || far == near && v.id < u.id)
    })
}

/// One tick of local traffic for `u` heading for `goal`, its route's next waypoint (`trafficStep`).
/// `u` is out of `g.units` while it moves; `grid` indexes the others where they stand now.
pub fn traffic_step(
    g: &Game,
    rules: &Rules,
    grid: &SpatialGrid,
    u: &mut Unit,
    goal: Option<Point>,
    dt: f64,
) -> Steer {
    traffic_step_with_knowledge(g, rules, grid, u, goal, dt, &TrafficKnowledge::physical())
        .expect("physical traffic cannot fail")
}

/// Steering with explicit remembered terrain. An invalid knowledge query aborts the turn.
#[allow(clippy::too_many_arguments)]
pub fn traffic_step_with_knowledge(
    g: &Game,
    rules: &Rules,
    grid: &SpatialGrid,
    u: &mut Unit,
    goal: Option<Point>,
    dt: f64,
    knowledge: &TrafficKnowledge<'_>,
) -> Result<Steer, KnowledgeError> {
    let tr = &rules.movement.traffic;
    let kind = &rules.kinds[u.kind];
    let tick = g.tick as f64;
    u.queued_behind = 0;
    let goal = match goal {
        Some(goal) if !protected(u) => goal,
        _ => {
            u.traffic = None;
            u.traffic_wait = 0.0;
            return Ok(Steer::go(goal));
        }
    };
    let block = kind.block();
    let half = if kind.ground_vehicle() {
        kind.body_radius()
    } else {
        0.9
    };
    let clear = |a: Point, b: Point| knowledge.clear(g, rules, a, b, block, half);
    let rank = |at: Point, from: Option<Point>| {
        if kind.infantry {
            knowledge.cover_rank(g, rules, at, from)
        } else {
            Ok(3)
        }
    };
    let d = length(u.at(), goal);
    let dx = (goal.x - u.x) / or_one(d);
    let dz = (goal.z - u.z) / or_one(d);
    let holder = if !kind.infantry && u.traffic.is_none() && !u.path.is_empty() {
        lane_holder(g, knowledge, rules, grid, u, kind)
    } else {
        None
    };
    let u_now: &Unit = u;
    let friend = |v: &Unit| g.allied(u_now.owner, v.owner);
    let radius = kind.body_radius() + 7.0;
    let matches = |vid: UnitId, v: &Unit| {
        !(kind.infantry && rules.kinds[v.kind].infantry)
            && vid != u_now.id
            && vid != u_now.board
            && v.hp > 0.0
            && v.air.is_none()
            && !v.riding
            && v.garrison.is_none()
            && !ghost_pair(g, v, u_now, friend(v))
            && seen_by(g, u_now.owner, v)
    };
    // a squad steers around vehicles only
    let nearby = if kind.infantry {
        grid.heavy(&g.units, (u.x, u.z), radius, matches)
    } else {
        grid.unordered(&g.units, (u.x, u.z), radius, matches)
    };
    let velocity = math::min(kind.speed, d / dt);
    let (vx, vz) = (dx * velocity, dz * velocity);
    let mut conflict: Option<&Unit> = None;
    let mut time = f64::INFINITY;
    let mut follow = false;
    let mut own: Option<Point> = None;
    for vid in &nearby {
        let v = &g.units[vid];
        let vd = &rules.kinds[v.kind];
        let wp = v
            .traffic
            .as_ref()
            .map(|t| t.goal)
            .or(v.path.first().copied());
        let vl = wp.map_or(0.0, |wp| length(v.at(), wp));
        let vvx =
            v.vx.unwrap_or_else(|| wp.map_or(0.0, |wp| (wp.x - v.x) / or_one(vl) * vd.speed));
        let vvz =
            v.vz.unwrap_or_else(|| wp.map_or(0.0, |wp| (wp.z - v.z) / or_one(vl) * vd.speed));
        let (rx, rz) = (v.x - u.x, v.z - u.z);
        let (rvx, rvz) = (vvx - vx, vvz - vz);
        let t = math::max(
            0.0,
            math::min(
                tr.horizon,
                -(rx * rvx + rz * rvz) / or_one(rvx * rvx + rvz * rvz),
            ),
        );
        let min = radius_pair(kind, vd) + 0.15;
        if math::hypot(rx + rvx * t, rz + rvz * t) >= min {
            continue;
        }
        // A unit already moving away does not hold up someone behind it.
        if rx * rvx + rz * rvz >= 0.0 && math::hypot(rx, rz) >= min - 0.1 {
            continue;
        }
        // Friends whose routes run the same way queue: one clearly in front is waited for instead of dodged;
        // one behind, or already waiting on this unit, is ignored.
        let a = if friend(v) {
            if own.is_none() {
                own = heading(u);
            }
            own
        } else {
            None
        };
        let b = a.and_then(|_| heading(v));
        let along = (rx * dx + rz * dz) / or_one(math::hypot(rx, rz));
        let same = match (a, b) {
            (Some(a), Some(b)) => a.x * b.x + a.z * b.z > 0.0,
            _ => false,
        };
        let queue = same && along > 0.5;
        if same && (along < 0.0 || waits_on(g, v, u.id)) {
            continue;
        }
        if t < time || t == time && conflict.is_some_and(|c| v.id < c.id) {
            conflict = Some(v);
            time = t;
            follow = queue;
        }
    }
    // Holding short of a one-lane stretch: wait there, unless someone runs into this unit.
    if let Some(holder) = holder
        && (conflict.is_none() || follow)
    {
        u.queued_behind = holder;
        return Ok(Steer {
            blocked: true,
            waiting: true,
            ..Steer::default()
        });
    }
    // The one in front is stuck without waiting on anyone, likely on this unit: give way.
    let give_way = holder.is_some()
        || follow && conflict.is_some_and(|c| c.stuck > 1.0 && c.queued_behind == 0);
    if give_way {
        follow = false;
    }
    // Gridlocked by a friend: pass through friends for a moment.
    if let Some(c) = conflict
        && !follow
        && g.allied(u.owner, c.owner)
        && u.traffic_wait >= tr.ghost
    {
        start_ghost(g, u, tr.ghost_time, dt);
        u.traffic = None;
        u.traffic_wait = 0.0;
        u.traffic_entered = !u.path.is_empty();
        return Ok(Steer::go(Some(u.path.first().copied().unwrap_or(goal))));
    }
    if let Some(by) = u.traffic.as_ref().map(|t| t.by) {
        let v = g.units.get(&by);
        let here = u.at();
        if u.traffic.as_ref().is_some_and(|t| t.passing) && v.is_some_and(|v| v.hp > 0.0) {
            let traffic = u.traffic.as_mut().expect("checked");
            if length(here, traffic.goal) < 0.4 {
                if !traffic.legs.is_empty() {
                    traffic.legs.remove(0);
                }
                if let Some(&next) = traffic.legs.first() {
                    traffic.goal = next;
                    traffic.until = tick * dt + tr.max_yield;
                } else {
                    // Resume only a kept waypoint whose whole segment clears terrain and visible bodies.
                    let mut resume = None;
                    for i in (0..u.path.len()).rev() {
                        let point = u.path[i];
                        let (sx, sz) = (point.x - u.x, point.z - u.z);
                        let squared = sx * sx + sz * sz;
                        if !clear(here, point) {
                            continue;
                        }
                        let free = nearby.iter().all(|oid| {
                            let other = &g.units[oid];
                            let along = math::max(
                                0.0,
                                math::min(
                                    1.0,
                                    ((other.x - u.x) * sx + (other.z - u.z) * sz) / or_one(squared),
                                ),
                            );
                            math::hypot(u.x + sx * along - other.x, u.z + sz * along - other.z)
                                >= radius_pair(kind, &rules.kinds[other.kind]) + 0.15
                        });
                        if free {
                            resume = Some(i);
                            break;
                        }
                    }
                    u.traffic = None;
                    u.traffic_wait = 0.0;
                    u.traffic_entered = false;
                    if let Some(resume) = resume {
                        u.path.drain(..resume);
                        return Ok(Steer::go(u.path.first().copied()));
                    }
                    u.repath = 0.0;
                    return Ok(Steer {
                        goal: u.path.first().copied(),
                        replan: true,
                        ..Steer::default()
                    });
                }
            }
            let traffic = u.traffic.as_ref().expect("still passing");
            if tick * dt < traffic.until {
                return Ok(Steer {
                    goal: Some(traffic.goal),
                    yielding: true,
                    ..Steer::default()
                });
            }
            u.traffic = None;
        }
        let Some(traffic) = &u.traffic else {
            return Ok(Steer {
                goal: u.path.first().copied(),
                replan: true,
                ..Steer::default()
            });
        };
        let arrived = length(here, traffic.goal) < 0.4;
        let safe = match v {
            None => true,
            Some(v) => {
                v.hp <= 0.0 || length(here, v.at()) > radius_pair(kind, &rules.kinds[v.kind]) + 4.0
            }
        };
        if safe || tick * dt >= traffic.until {
            u.traffic = None;
            u.traffic_entered = false;
        } else if !arrived {
            return Ok(Steer {
                goal: Some(traffic.goal),
                yielding: true,
                ..Steer::default()
            });
        } else {
            return Ok(Steer {
                blocked: true,
                yielding: true,
                ..Steer::default()
            });
        }
    }
    let Some(conflict) = conflict else {
        u.traffic_wait = math::max(0.0, u.traffic_wait - dt);
        u.traffic_entered = !u.path.is_empty();
        return Ok(Steer::go(Some(goal)));
    };
    // Waiting in a queue is not gridlock: it neither ages priority, leads to a ghost, nor asks for a new route.
    if follow {
        u.queued_behind = conflict.id;
        return Ok(Steer {
            blocked: true,
            waiting: true,
            ..Steer::default()
        });
    }
    // Stable priority remains for the whole maneuver. The other unit can see the yielded lane.
    let conflict_kind = &rules.kinds[conflict.kind];
    if !give_way
        && (conflict.traffic.as_ref().is_some_and(|t| t.by == u.id)
            || waits_on(g, conflict, u.id)
            || traffic_wins(rules, u, conflict)
                && !conflict_kind.structure
                && !protected(conflict)
                && !conflict.path.is_empty())
    {
        return Ok(Steer::go(Some(goal)));
    }
    u.traffic_wait += dt;
    if u.traffic_wait < tr.wait {
        return Ok(Steer {
            blocked: true,
            ..Steer::default()
        });
    }
    let min_cover = rank(u.at(), u.hit_from)?;
    let side = if u.id < conflict.id { 1.0 } else { -1.0 };
    let distance = radius_pair(kind, conflict_kind) + 0.7;
    let occupied = |at: Point| {
        nearby.iter().any(|vid| {
            let v = &g.units[vid];
            length(at, v.at()) < radius_pair(kind, &rules.kinds[v.kind]) + 0.2
        })
    };
    let mut candidates: Vec<Point> = Vec::new();
    for k in [1.0, 1.5, 2.0] {
        for sign in [side, -side] {
            candidates.push(Point {
                x: u.x - dz * sign * distance * k - dx * 0.8,
                z: u.z + dx * sign * distance * k - dz * 0.8,
            });
        }
    }
    // At a narrow crossing, back to a wider piece of the route instead of alternating sides every tick.
    for at in u.traffic_trail.iter().rev() {
        let p = Point { x: at.x, z: at.z };
        if length(u.at(), p) > distance && length(u.at(), p) < 24.0 {
            candidates.push(p);
        }
    }
    let here = u.at();
    for at in candidates {
        if !clear(here, at) || occupied(at) || rank(at, u.hit_from)? > min_cover {
            continue;
        }
        if conflict.path.is_empty() {
            // A stopped unit cannot clear a yielded lane. Pass around it through checked local legs.
            let lateral = (at.x - u.x) * -dz + (at.z - u.z) * dx;
            let offset = math::sign(lateral) * math::max(distance, lateral.abs());
            let pass = Point {
                x: conflict.x - dz * offset,
                z: conflict.z + dx * offset,
            };
            let corner = Point {
                x: pass.x + dx * distance,
                z: pass.z + dz * distance,
            };
            let exit = Point {
                x: conflict.x + dx * distance,
                z: conflict.z + dz * distance,
            };
            let legs = vec![at, pass, corner, exit];
            let mut previous = here;
            let mut safe = true;
            for &point in &legs {
                safe = clear(previous, point)
                    && !occupied(point)
                    && rank(point, u.hit_from)? <= min_cover;
                previous = point;
                if !safe {
                    break;
                }
            }
            if !safe {
                continue;
            }
            u.traffic = Some(Traffic {
                goal: legs[0],
                legs,
                passing: true,
                by: conflict.id,
                until: tick * dt + tr.max_yield,
            });
            u.traffic_entered = false;
            return Ok(Steer {
                goal: Some(at),
                yielding: true,
                ..Steer::default()
            });
        }
        u.traffic = Some(Traffic {
            goal: at,
            legs: Vec::new(),
            passing: false,
            by: conflict.id,
            until: tick * dt + tr.max_yield,
        });
        u.traffic_entered = false;
        return Ok(Steer {
            goal: Some(at),
            yielding: true,
            ..Steer::default()
        });
    }
    Ok(Steer {
        blocked: true,
        replan: u.traffic_wait >= tr.replan,
        ..Steer::default()
    })
}

/// Remembers where a unit on its route has been, a point every metre and a half, the last 20
/// (`rememberTrafficPosition`): places to back up to at a narrow crossing.
pub fn remember_position(u: &mut Unit) {
    if u.path.is_empty() || u.traffic.is_some() {
        return;
    }
    let (x, z) = (u.x, u.z);
    if u.traffic_trail
        .last()
        .is_none_or(|last| math::hypot(x - last.x, z - last.z) >= 1.5)
    {
        u.traffic_trail.push(Pose {
            x: u.x,
            z: u.z,
            rot: u.rot,
        });
    }
    while u.traffic_trail.len() > 20 {
        u.traffic_trail.remove(0);
    }
}
