//! Units moving along their routes, one tick at a time: a port of the movement block of `step()`'s unit loop
//! in `shared/sim.js` at the pin (from "Global waypoints, voluntary traffic steering and swept contact are
//! separate steps" through the stuck count that follows `updateGrid`), with `walkRoute` and `driveVehicle`
//! from `shared/movement.js`, the speed rules (`speedMul`, `groundMul`, `suppMul`, `weatherSpeed`, the
//! slope), local traffic (`traffic.rs`) and the hull's motion (`vehicle.rs`). It also has the parts of the
//! loop around the block that movement reads (which units reach it, the timers it reads) and the infantry
//! progress count after crowd separation (`separation.rs`).
//!
//! Its answers match the reference call for call (`harness/movement-trace.mjs`, `simtest --replay`), as the
//! frozen workload of issue #2 runs it. Left out, and so different from the reference where they apply:
//! - the default entry points preserve the frozen workload's disabled path search. The `with_routes`
//!   entry points call supplied routing at the real recovery/replan point; callers select terrain,
//!   apply routes and handle deferred searches. Full order scheduling is separate;
//! - logistics: fuel limits and fuel use (`consumeDrivingFuel`), convoys, and units forced back by
//!   shortages; the loader refuses units that have reserves or a convoy;
//! - crushing, ground wear and the mud it makes, and mines going off under a unit (the workload turns
//!   crushing and wear off, and asserts the terrain never changes); with wear unchanged, `touch` only
//!   tells clients about a cell whose look changed, and the workload checks none did;
//! - the default entry point uses march factor one; Match supplies the World factor explicitly.
//!   Riders following their carrier, trains and planes remain separate;
//! - the default entry points use the physical map, as in the frozen workload.
//!   Match uses `move_unit_with_knowledge` for remembered traffic decisions.

use crate::game::{Clearance, FORD, Game, MUD, Point, Pose, Unit, WIRE};
use crate::grid::{SpatialGrid, UnitId};
use crate::math;
use crate::route_knowledge::KnowledgeError;
use crate::rules::{Rules, UnitKind};
use crate::terrain::{height_at, speed_mul, truthy, walkable, wet_at};
use crate::traffic::{
    Steer, ghost_pair, remember_position, start_ghost, traffic_step_with_knowledge,
};
use crate::traffic_knowledge::TrafficKnowledge;
use crate::vehicle::{VehicleStep, angle_delta, swept_vehicle_clear, vehicle_step};
use crate::vision::{in_trench, or_one};

fn dist(a: Point, b: Point) -> f64 {
    math::hypot(a.x - b.x, a.z - b.z)
}

fn point(p: Pose) -> Point {
    Point { x: p.x, z: p.z }
}

/// Suppression's effect on speed (`suppMul(u).speed`).
pub fn supp_speed(supp: f64) -> f64 {
    if supp >= 90.0 {
        0.3
    } else if supp >= 50.0 {
        0.6
    } else {
        1.0
    }
}

/// The weather's speed factor for a kind (`weatherSpeed`); planes fly over it.
pub fn weather_speed(g: &Game, rules: &Rules, kind: &UnitKind) -> f64 {
    match g.weather.map(|w| &rules.weather[w]) {
        Some(w) if !kind.air => {
            if kind.infantry {
                w.inf
            } else {
                w.veh
            }
        }
        _ => 1.0,
    }
}

/// The speed and route rules for one unit this tick: what it moved (for the infantry progress count).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Moved {
    pub id: UnitId,
    /// The speed it was allowed, metres per second.
    pub speed: f64,
    pub infantry: bool,
}

/// The movement block for unit `id` (see the module notes): steer through traffic, walk or drive along the
/// route, update the spatial index and the unit's dust, stillness, retreat and stuck count. `supp_speed` is
/// `suppMul(u).speed` taken at the start of the unit's turn, before its suppression wore off this tick.
/// Why a moving unit needs a replacement route.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RouteRequest {
    /// A stalled infantry route, subject to the step's request budget.
    Recovery,
    /// Vehicle stalls and traffic replans search immediately.
    Immediate,
}

pub fn move_unit(
    g: &mut Game,
    rules: &Rules,
    grid: &mut SpatialGrid,
    id: UnitId,
    supp_speed: f64,
) -> Option<Moved> {
    move_unit_with_routes(g, rules, grid, id, supp_speed, &mut |_, _, u, _, _| {
        u.repath = 1.0;
    })
}

/// Runs one movement block with real routing supplied by the caller.
/// The callback runs after traffic steering, at the reference's exact replan point.
/// It may update the unit's path and retry timer or defer a budgeted request. A new
/// path must increment `path_gen`, including an empty replacement. The game does
/// not contain this unit during the callback; its current state is supplied as `u`.
/// Terrain selection and search errors remain the caller's responsibility.
pub fn move_unit_with_routes(
    g: &mut Game,
    rules: &Rules,
    grid: &mut SpatialGrid,
    id: UnitId,
    supp_speed: f64,
    routes: &mut impl FnMut(&Game, &Rules, &mut Unit, Point, RouteRequest),
) -> Option<Moved> {
    move_unit_with_knowledge(
        g,
        rules,
        grid,
        id,
        supp_speed,
        routes,
        &TrafficKnowledge::physical(),
    )
    .expect("physical movement cannot fail")
}

/// Real routing with explicit traffic terrain. A knowledge error is fatal to the
/// tick; the unit is always reinserted, including after an error.
#[allow(clippy::too_many_arguments)]
pub fn move_unit_with_knowledge(
    g: &mut Game,
    rules: &Rules,
    grid: &mut SpatialGrid,
    id: UnitId,
    supp_speed: f64,
    routes: &mut impl FnMut(&Game, &Rules, &mut Unit, Point, RouteRequest),
    knowledge: &TrafficKnowledge<'_>,
) -> Result<Option<Moved>, KnowledgeError> {
    move_unit_with_march(g, rules, grid, id, supp_speed, routes, knowledge, 1.0)
}
/// The caller supplies the validated World march policy at the unit's actual turn.
/// The ordinary entry point keeps its existing multiplier of one.
#[allow(clippy::too_many_arguments)]
pub(crate) fn move_unit_with_march(
    g: &mut Game,
    rules: &Rules,
    grid: &mut SpatialGrid,
    id: UnitId,
    supp_speed: f64,
    routes: &mut impl FnMut(&Game, &Rules, &mut Unit, Point, RouteRequest),
    knowledge: &TrafficKnowledge<'_>,
    march: f64,
) -> Result<Option<Moved>, KnowledgeError> {
    if !march.is_finite() || march < 0.0 {
        return Err(KnowledgeError::InvalidValue);
    }
    // The unit is out of the map while it moves. The reference reads it there only through `waitsOn`, which
    // stops at it either way (its `queuedBehind` is 0 by then), and every query skips it.
    let Some(mut u) = g.units.remove(&id) else {
        return Ok(None);
    };
    let speed = block(g, rules, grid, &mut u, supp_speed, routes, knowledge, march);
    grid.update(id, &u);
    let infantry = rules.kinds[u.kind].infantry;
    g.units.insert(id, u);
    Ok(Some(Moved {
        id,
        speed: speed?,
        infantry,
    }))
}

/// One movement step computed on a copy of the unit. `Game` and the grid are
/// unchanged until `keep_trial_move` applies it, so a caller can discard it.
pub(crate) struct TrialMove {
    id: UnitId,
    unit: Unit,
    speed: Result<f64, KnowledgeError>,
}

/// `move_unit_with_march` split in two: the same `block` call on a copy of the
/// unit, with the original left in place. Keeping the trial then performs the
/// same grid update and reinsertion, so the end state is identical.
#[allow(clippy::too_many_arguments)]
pub(crate) fn trial_move_with_march(
    g: &mut Game,
    rules: &Rules,
    grid: &SpatialGrid,
    id: UnitId,
    supp_speed: f64,
    routes: &mut impl FnMut(&Game, &Rules, &mut Unit, Point, RouteRequest),
    knowledge: &TrafficKnowledge<'_>,
    march: f64,
) -> Result<Option<TrialMove>, KnowledgeError> {
    if !march.is_finite() || march < 0.0 {
        return Err(KnowledgeError::InvalidValue);
    }
    let Some(original) = g.units.remove(&id) else {
        return Ok(None);
    };
    let mut u = original.clone();
    let speed = block(g, rules, grid, &mut u, supp_speed, routes, knowledge, march);
    g.units.insert(id, original);
    Ok(Some(TrialMove { id, unit: u, speed }))
}

impl TrialMove {
    pub(crate) fn succeeded(&self) -> bool {
        self.speed.is_ok()
    }
}

pub(crate) fn keep_trial_move(
    g: &mut Game,
    rules: &Rules,
    grid: &mut SpatialGrid,
    trial: TrialMove,
) -> Result<Moved, KnowledgeError> {
    let TrialMove { id, unit: u, speed } = trial;
    grid.update(id, &u);
    let infantry = rules.kinds[u.kind].infantry;
    g.units.insert(id, u);
    Ok(Moved {
        id,
        speed: speed?,
        infantry,
    })
}

#[allow(clippy::too_many_arguments)]
fn block(
    g: &Game,
    rules: &Rules,
    grid: &SpatialGrid,
    u: &mut Unit,
    supp_speed: f64,
    routes: &mut impl FnMut(&Game, &Rules, &mut Unit, Point, RouteRequest),
    knowledge: &TrafficKnowledge<'_>,
    march: f64,
) -> Result<f64, KnowledgeError> {
    let m = &rules.movement;
    let dt = m.tick;
    let kind = &rules.kinds[u.kind];
    let before = u.pose();
    let ground_vehicle = kind.ground_vehicle();
    u.motion_before = Some(before);
    let back = match (u.reverse, u.path.last()) {
        (Some(reverse), Some(&end)) => dist(end, reverse) < 1e-6,
        _ => false,
    };
    if !back {
        u.reverse = None;
    }
    let mut grade = 0.0;
    if let Some(heights) = &g.height
        && let Some(&wp) = u.path.first()
    {
        let d = or_one(dist(u.at(), wp));
        grade = height_at(
            g,
            rules,
            heights,
            u.x + (wp.x - u.x) / d,
            u.z + (wp.z - u.z) / d,
        ) - height_at(g, rules, heights, u.x, u.z);
    }
    if truthy(u.pace) && (u.attack || u.retreating || u.path.is_empty()) {
        u.pace = 0.0;
    }
    let top = if truthy(u.pace) {
        math::min(kind.speed, u.pace)
    } else {
        kind.speed
    };
    let order = if u.retreating {
        m.retreat_speed
    } else if u.sprint > 0.0 {
        kind.ability_speed.unwrap_or(f64::NAN)
    } else {
        supp_speed
    };
    // Source applies the caller's World march multiplier last.
    let speed = top
        * order
        * speed_mul(g, rules, u)
        * (1.0 - m.slope[usize::from(!kind.infantry)] * math::min(1.0, math::max(0.0, grade)))
        * weather_speed(g, rules, kind)
        * march;
    remember_position(u);
    let goal = u.path.first().copied();
    let steer = if kind.naval || kind.structure {
        Steer {
            goal,
            ..Steer::default()
        }
    } else {
        traffic_step_with_knowledge(g, rules, grid, u, goal, dt, knowledge)?
    };
    let stalled = (ground_vehicle || kind.infantry && !steer.blocked && !steer.yielding)
        && u.stuck > 0.75
        && !steer.waiting;
    if (steer.replan || stalled) && u.repath <= 0.0 {
        let goal = u.world_goal.or(u.amove).or(u.path.last().copied());
        if let Some(goal) = goal {
            let request = if kind.infantry && !steer.replan {
                RouteRequest::Recovery
            } else {
                RouteRequest::Immediate
            };
            routes(g, rules, u, goal, request);
        }
    }
    // The route followed now: the unit's own route, or a sidestep's single point.
    let route_is_path = steer.goal.is_some() && !steer.yielding;
    let mut sidestep: Vec<Point> = match steer.goal {
        Some(goal) if steer.yielding => vec![goal],
        _ => Vec::new(),
    };
    let mut path = std::mem::take(&mut u.path);
    let route = if route_is_path {
        &mut path
    } else {
        &mut sidestep
    };
    let dropped = if ground_vehicle {
        drive_vehicle(
            g, rules, grid, u, kind, route, speed, dt, back, &steer, before, knowledge,
        );
        false
    } else {
        walk_route(g, rules, u, kind, route, speed, dt, back, &steer, knowledge)
    };
    if dropped {
        // `u.path = []`: a new route array
        u.path_gen += 1;
    } else {
        u.path = path;
    }
    u.vx = Some((u.x - before.x) / dt);
    u.vz = Some((u.z - before.z) / dt);
    let moved = dist(u.at(), point(before));
    let moving = !u.path.is_empty() || moved > 0.001;
    if u.path.is_empty() {
        u.drift = false;
    }
    // (the caller updates the spatial index here: `updateGrid`)
    if moved > 0.0 {
        let c = g.cell_index(u.x, u.z);
        if !kind.infantry && c >= 0 {
            // on dry ground, vehicles raise dust
            let c = c as usize;
            if wet_at(g, c) < 0.3 && g.flags[c] & (MUD | FORD) == 0 {
                u.dust = Some(g.tick as f64);
            }
        }
    }
    u.still = Some(if moving {
        0.0
    } else {
        u.still.map_or(f64::NAN, |s| s + dt)
    });
    if u.retreating && u.path.is_empty() && u.world_goal.is_none() {
        u.retreating = false;
    }
    if kind.infantry {
        // measured after crowd separation, which can undo its step (`infantry_progress`)
        return Ok(speed);
    }
    let turning_in_place = ground_vehicle && angle_delta(before.rot, u.rot).abs() > 1e-4;
    let progress_speed = match &u.clearance {
        Some(c) => {
            math::min(speed, 1.2)
                * if c.reverse {
                    kind.vehicle.as_ref().map_or(f64::NAN, |p| p.reverse_speed)
                } else {
                    1.0
                }
        }
        None => speed,
    };
    if !u.path.is_empty() && moved < progress_speed * dt * 0.3 && !turning_in_place {
        u.stuck += dt;
    } else {
        u.stuck = 0.0;
    }
    Ok(speed)
}

/// One tick of infantry or a boat along `route` (`walkRoute`): straight legs at full speed, the next
/// waypoint taken on reaching it. A leg that runs into a blocked cell drops the route and asks for a new one
/// shortly; returns whether it dropped the route (`u.path = []`).
#[allow(clippy::too_many_arguments)]
fn walk_route(
    g: &Game,
    rules: &Rules,
    u: &mut Unit,
    kind: &UnitKind,
    route: &mut Vec<Point>,
    speed: f64,
    dt: f64,
    back: bool,
    steer: &Steer,
    knowledge: &TrafficKnowledge<'_>,
) -> bool {
    let block = kind.block();
    if kind.infantry && !steer.yielding {
        // Crowded squads can pass beside a waypoint. Keep required terrain corners and the exact final spot.
        while route.len() > 1
            && dist(u.at(), route[0]) < 1.0
            && knowledge.clear(g, rules, u.at(), route[1], block | WIRE, 0.9)
        {
            route.remove(0);
        }
    }
    let mut budget = speed
        * dt
        * if back {
            rules.movement.reverse_speed
        } else {
            1.0
        };
    while budget > 0.0 && !route.is_empty() {
        let wp = route[0];
        let d = dist(u.at(), wp);
        let advance = math::min(d, budget);
        let next = Point {
            x: u.x + (wp.x - u.x) / or_one(d) * advance,
            z: u.z + (wp.z - u.z) / or_one(d) * advance,
        };
        if !walkable(g, rules, u.at(), next, block, 0.9) {
            u.repath = 0.2;
            return !steer.yielding;
        }
        if !back {
            u.rot = math::atan2(wp.z - u.z, wp.x - u.x);
        }
        u.x = next.x;
        u.z = next.z;
        budget -= advance;
        if d <= advance + 1e-9 {
            if steer.yielding {
                break;
            }
            route.remove(0);
        } else {
            break;
        }
    }
    false
}

/// One tick of a ground vehicle along `route` (`driveVehicle`): the hull's own motion, backing or sliding
/// out when walled in, the swept check against walls and units, ghosting past a friend that boxes it in,
/// and taking the next leg. `before` is where it stood when the block began.
#[allow(clippy::too_many_arguments)]
fn drive_vehicle(
    g: &Game,
    rules: &Rules,
    grid: &SpatialGrid,
    u: &mut Unit,
    kind: &UnitKind,
    route: &mut Vec<Point>,
    speed: f64,
    dt: f64,
    back: bool,
    steer: &Steer,
    before: Pose,
    knowledge: &TrafficKnowledge<'_>,
) {
    use std::f64::consts::PI;
    let profile = kind
        .vehicle
        .as_ref()
        .expect("ground vehicles have a motion profile");
    let tr = &rules.movement.traffic;
    let block = kind.block();
    let tick = g.tick as f64;
    let swept = |from: Pose, to: Pose| swept_vehicle_clear(g, from, to, kind, block);
    let reverse = back
        || steer.yielding
            && steer.goal.is_some_and(|goal| {
                math::cos(math::atan2(goal.z - u.z, goal.x - u.x) - u.rot) < -0.5
            });
    if u.clearance.as_ref().is_some_and(|c| {
        c.path != u.path_gen || tick > c.until || dist(u.at(), point(c.goal)) < 0.3
    }) {
        u.clearance = None;
    }
    let mut next = match &u.clearance {
        Some(c) => vehicle_step(
            u,
            rules,
            profile,
            &[point(c.goal)],
            math::min(speed, 1.2),
            dt,
            c.reverse,
        ),
        None => vehicle_step(u, rules, profile, route, speed, dt, reverse),
    };
    if u.clearance.is_none()
        && !route.is_empty()
        && u.stuck > 0.3
        && !swept(
            before,
            Pose {
                rot: next.rot,
                ..before
            },
        )
    {
        for direction in [-1.0, 1.0] {
            let goal = Pose {
                x: before.x + math::cos(before.rot) * 2.0 * direction,
                z: before.z + math::sin(before.rot) * 2.0 * direction,
                rot: before.rot,
            };
            if !swept(before, goal) {
                continue;
            }
            let bearing = math::atan2(route[0].z - goal.z, route[0].x - goal.x)
                + if reverse { PI } else { 0.0 };
            if !swept(
                goal,
                Pose {
                    rot: bearing,
                    ..goal
                },
            ) {
                continue;
            }
            u.clearance = Some(Clearance {
                goal,
                reverse: direction < 0.0,
                path: u.path_gen,
                until: tick + 80.0,
            });
            next = vehicle_step(
                u,
                rules,
                profile,
                &[point(goal)],
                math::min(speed, 1.2),
                dt,
                direction < 0.0,
            );
            break;
        }
        // walled in lengthwise too: slide over, up to 1.5 m at 1.2 m/s, to where the turn fits
        if u.clearance.is_none() {
            for side in [0.5, -0.5, 1.0, -1.0, 1.5, -1.5] {
                let at = Pose {
                    x: before.x - math::sin(before.rot) * side,
                    z: before.z + math::cos(before.rot) * side,
                    rot: before.rot,
                };
                let bearing = math::atan2(route[0].z - at.z, route[0].x - at.x)
                    + if reverse { PI } else { 0.0 };
                if !swept(before, at) || !swept(at, Pose { rot: bearing, ..at }) {
                    continue;
                }
                let k = math::min(1.0, 1.2 * dt / side.abs());
                next = VehicleStep {
                    x: before.x + (at.x - before.x) * k,
                    z: before.z + (at.z - before.z) * k,
                    rot: before.rot,
                    move_speed: 0.0,
                    ..next
                };
                break;
            }
        }
    }
    // Never let a hull rotate or travel through a cell during this tick.
    let mut clear_motion = swept(
        before,
        Pose {
            x: next.x,
            z: next.z,
            rot: next.rot,
        },
    );
    if clear_motion {
        let me: &Unit = u;
        let nearby = grid.unordered(
            &g.units,
            (me.x, me.z),
            kind.body_radius() + 9.0,
            |vid, v| {
                vid != me.id
                    && v.hp > 0.0
                    && v.air.is_none()
                    && !v.riding
                    && v.garrison.is_none()
                    && !(g.allied(me.owner, v.owner) && ghost_pair(g, me, v, true))
            },
        );
        let blocker = nearby.iter().map(|vid| &g.units[vid]).find(|v| {
            let min = kind.body_radius() + rules.kinds[v.kind].body_radius();
            let (dx, dz) = (next.x - before.x, next.z - before.z);
            let t = math::max(
                0.0,
                math::min(
                    1.0,
                    ((v.x - before.x) * dx + (v.z - before.z) * dz) / or_one(dx * dx + dz * dz),
                ),
            );
            let closest = math::hypot(before.x + dx * t - v.x, before.z + dz * t - v.z);
            let from = dist(point(before), v.at());
            !(closest >= min - 1e-6
                || from < min
                    && dist(
                        Point {
                            x: next.x,
                            z: next.z,
                        },
                        v.at(),
                    ) > from)
        });
        clear_motion = blocker.is_none();
        // boxed in by a friend for a while (a crowded gate or street): pass through friends for a moment
        if let Some(blocker) = blocker
            && g.allied(blocker.owner, u.owner)
            && u.stuck > tr.ghost
        {
            start_ghost(g, u, tr.ghost_time, dt);
        }
    }
    if clear_motion {
        u.x = next.x;
        u.z = next.z;
        u.rot = next.rot;
        u.move_speed = next.move_speed;
    } else {
        u.move_speed = 0.0;
        if swept(
            before,
            Pose {
                rot: next.rot,
                ..before
            },
        ) {
            u.rot = next.rot;
        }
        u.traffic_wait += dt;
    }
    u.travel_dir = Some(next.travel_dir);
    // A bend is taken at speed: turn onto the next leg a little early, or just past the waypoint, where the
    // hull has a clear straight line to the next one; otherwise on reaching the waypoint.
    let next_leg = route.get(1).copied();
    let near = route.first().map_or(f64::INFINITY, |&wp| dist(u.at(), wp));
    let pace = u.move_speed.abs();
    let bend = match next_leg {
        Some(leg) => angle_delta(
            next.travel_dir,
            math::atan2(leg.z - route[0].z, leg.x - route[0].x),
        )
        .abs(),
        None => PI,
    };
    let lead = if bend < PI / 2.0 {
        pace / profile.hull_turn * math::tan(bend / 2.0)
    } else {
        0.0
    };
    let passed = near < 2.0
        && math::cos(math::atan2(u.z - route[0].z, u.x - route[0].x) - next.travel_dir) > 0.0;
    let reached = near < math::max(0.12, pace * dt * 1.5)
        || next_leg.is_some_and(|leg| {
            (near < lead || passed)
                && knowledge.clear(g, rules, u.at(), leg, block, kind.body_radius())
        });
    if reached && !steer.yielding && u.clearance.is_none() {
        route.remove(0);
    }
}

/// Which units reach the movement block this tick, in the order the reference's unit loop visits them
/// (ascending id): alive, not a building without a working weapon, not a plane or a train, not riding.
pub fn reaches_block(rules: &Rules, u: &Unit) -> bool {
    let kind = &rules.kinds[u.kind];
    !(u.hp <= 0.0
        || kind.building && (!kind.weapon || u.built.is_some_and(|b| b < 1.0))
        || kind.air
        || kind.rail
        || u.riding)
}

/// The unit loop of `step()` as far as movement goes: for each unit that reaches the block, in id order,
/// its suppression speed factor, then suppression wearing off and the route and sprint timers running down,
/// then the movement block. Returns what moved, for the infantry progress count after separation.
pub fn move_units(g: &mut Game, rules: &Rules, grid: &mut SpatialGrid) -> Vec<Moved> {
    move_units_with_routes(g, rules, grid, &mut |_, _, u, _, _| {
        u.repath = 1.0;
    })
}

/// Runs the movement loop with the same route callback as `move_unit_with_routes`.
pub fn move_units_with_routes(
    g: &mut Game,
    rules: &Rules,
    grid: &mut SpatialGrid,
    routes: &mut impl FnMut(&Game, &Rules, &mut Unit, Point, RouteRequest),
) -> Vec<Moved> {
    let dt = rules.movement.tick;
    let ids: Vec<UnitId> = g.units.keys().copied().collect();
    let mut moved = Vec::with_capacity(ids.len());
    for id in ids {
        let Some(u) = g.units.get(&id) else { continue };
        if !reaches_block(rules, u) {
            continue;
        }
        let factor = supp_speed(u.supp);
        let rally = if in_trench(g, rules, u) {
            rules.movement.trench_rally
        } else {
            8.0
        };
        let infantry = rules.kinds[u.kind].infantry;
        let u = g.units.get_mut(&id).expect("listed");
        if infantry {
            u.supp = math::max(0.0, u.supp - rally * dt);
        }
        u.repath -= dt;
        u.sprint -= dt;
        if let Some(m) = move_unit_with_routes(g, rules, grid, id, factor, routes) {
            moved.push(m);
        }
    }
    moved
}

/// Infantry progress, measured after crowd separation, which can undo a squad's step: a squad on its route
/// that covered less than 30% of a tick at its speed counts as stuck a little longer.
pub fn infantry_progress(g: &mut Game, rules: &Rules, moved: &[Moved]) {
    let dt = rules.movement.tick;
    for m in moved.iter().filter(|m| m.infantry) {
        let Some(u) = g.units.get_mut(&m.id) else {
            continue;
        };
        if !(u.hp > 0.0 && !u.riding && u.garrison.is_none()) {
            continue;
        }
        let from = u.motion_before.map_or(
            Point {
                x: f64::NAN,
                z: f64::NAN,
            },
            point,
        );
        if !u.path.is_empty() && dist(u.at(), from) < m.speed * dt * 0.3 {
            u.stuck += dt;
        } else {
            u.stuck = 0.0;
        }
    }
}

#[cfg(test)]
#[path = "mov_audit_movement_tests.rs"]
mod mov_audit;
