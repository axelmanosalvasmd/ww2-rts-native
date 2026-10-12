//! How a ground vehicle's hull moves: its next step toward a waypoint (braking for bends, turning on the
//! spot or on an arc), and whether its rectangular footprint stays clear of blocked cells while it moves and
//! turns. A port of `vehicleStep`, `sweptVehicleClear`, `vehiclePositionClear`, `vehicleOverlap`,
//! `turnSpeed`, `turnAngle` and `angleDelta` in `shared/vehicle-motion.js` at the pin.

use crate::game::{CELL, Game, Point, Pose, Unit};
use crate::math;
use crate::rules::{Rules, UnitKind, VehicleProfile};
use crate::terrain::segment_cliff_clear;

/// The signed angle from `from` to `to`, in (-pi, pi] (`angleDelta`).
pub fn angle_delta(from: f64, to: f64) -> f64 {
    math::atan2(math::sin(to - from), math::cos(to - from))
}

/// `from` turned toward `to` by at most `amount` radians, wrapped to (-pi, pi] (`turnAngle`).
pub fn turn_angle(from: f64, to: f64, amount: f64) -> f64 {
    let a = from + math::max(-amount, math::min(amount, angle_delta(from, to)));
    math::atan2(math::sin(a), math::cos(a))
}

/// How many blocked cells the hull at `at` overlaps, counting no further than `limit`; infinity when the
/// hull reaches off the map (`vehicleOverlap`).
fn vehicle_overlap(g: &Game, at: Pose, kind: &UnitKind, block: u16, limit: f64) -> f64 {
    let cell = CELL;
    let mut count = 0.0;
    let long = kind.body_radius();
    let wide = kind.radius * 0.48;
    let a = at.rot;
    let (cos, sin) = (math::cos(a), math::sin(a));
    let bx = cos.abs() * long + sin.abs() * wide;
    let bz = sin.abs() * long + cos.abs() * wide;
    if at.x - bx < 0.0
        || at.z - bz < 0.0
        || at.x + bx >= g.w as f64 * cell
        || at.z + bz >= g.h as f64 * cell
    {
        return f64::INFINITY;
    }
    let half = cell / 2.0;
    // whether the blocked-or-not cell (x, y) reaches into the hull
    let overlaps = |x: f64, y: f64| {
        if g.flags[(y * g.w as f64 + x) as usize] & block == 0 {
            return false;
        }
        let dx = (x + 0.5) * cell - at.x;
        let dz = (y + 0.5) * cell - at.z;
        if dx.abs() >= bx + half - 1e-6 || dz.abs() >= bz + half - 1e-6 {
            return false;
        }
        if (dx * cos + dz * sin).abs() >= long + half * (cos.abs() + sin.abs()) - 1e-6 {
            return false;
        }
        (-dx * sin + dz * cos).abs() < wide + half * (cos.abs() + sin.abs()) - 1e-6
    };
    // Loop bounds as the reference's floats: a NaN position overlaps nothing.
    let (y1, x1) = (((at.z + bz) / cell).floor(), ((at.x + bx) / cell).floor());
    let mut y = ((at.z - bz) / cell).floor();
    while y <= y1 {
        let mut x = ((at.x - bx) / cell).floor();
        while x <= x1 {
            if overlaps(x, y) {
                count += 1.0;
                if count >= limit {
                    return count;
                }
            }
            x += 1.0;
        }
        y += 1.0;
    }
    count
}

/// Whether the hull at `at` overlaps no blocked cell and stays on the map (`vehiclePositionClear`).
pub fn vehicle_position_clear(g: &Game, at: Pose, kind: &UnitKind, block: u16) -> bool {
    vehicle_overlap(g, at, kind, block, 1.0) == 0.0
}

/// Whether the hull can move and turn from `from` to `to` (`sweptVehicleClear`): no cliff on the way, and
/// no blocked cell at any of the in-between poses a quarter metre or 0.05 radians apart. A hull already
/// against a wall may move as long as it digs in no deeper.
pub fn swept_vehicle_clear(g: &Game, from: Pose, to: Pose, kind: &UnitKind, block: u16) -> bool {
    if !segment_cliff_clear(
        g,
        Point {
            x: from.x,
            z: from.z,
        },
        Point { x: to.x, z: to.z },
    ) {
        return false;
    }
    let stuck_in = vehicle_overlap(g, from, kind, block, f64::INFINITY);
    if stuck_in != 0.0 && !stuck_in.is_nan() {
        return vehicle_overlap(g, to, kind, block, f64::INFINITY) <= stuck_in;
    }
    let d = math::hypot(to.x - from.x, to.z - from.z);
    let turn = angle_delta(from.rot, to.rot);
    let count = math::max(
        math::max(1.0, (d / 0.25).ceil()),
        (turn.abs() / 0.05).ceil(),
    );
    let mut i = 1.0;
    while i <= count {
        let k = i / count;
        let at = Pose {
            x: from.x + (to.x - from.x) * k,
            z: from.z + (to.z - from.z) * k,
            rot: from.rot + turn * k,
        };
        if !vehicle_position_clear(g, at, kind, block) {
            return false;
        }
        i += 1.0;
    }
    true
}

/// How fast a hull may drive while turning through `angle`: slow enough that its arc strays at most
/// `bulge` metres off the straight line; a quarter turn or more is a stop and pivot (`turnSpeed`).
pub fn turn_speed(profile: &VehicleProfile, bulge: f64, angle: f64) -> f64 {
    if angle >= std::f64::consts::PI / 2.0 {
        0.0
    } else if angle < 1e-3 {
        f64::INFINITY
    } else {
        profile.hull_turn * bulge / (1.0 - math::cos(angle))
    }
}

/// A hull's proposed next pose and speed (`vehicleStep`'s result).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VehicleStep {
    pub x: f64,
    pub z: f64,
    pub rot: f64,
    pub move_speed: f64,
    pub travel_dir: f64,
}

/// One tick of a hull toward `path[0]` at up to `top_speed` (`vehicleStep`): it brakes ahead of each bend
/// to the speed the bend allows and to a stop at the route's end, turns toward the waypoint at its hull
/// turn rate, drives only on an arc tight enough to pass through the waypoint, and accelerates or brakes
/// toward that speed. Contacts may cut the result down later but never speed it up.
pub fn vehicle_step(
    u: &Unit,
    rules: &Rules,
    profile: &VehicleProfile,
    path: &[Point],
    top_speed: f64,
    dt: f64,
    reverse: bool,
) -> VehicleStep {
    let pi = std::f64::consts::PI;
    let bulge = rules.movement.corner_bulge;
    let wp = path.first().copied();
    let speed = u.move_speed;
    // Leave one discrete step of braking room instead of chasing a point just passed.
    let brake_step = profile.braking * dt / 2.0;
    let reach = |distance: f64, end: f64| {
        let e = end + brake_step;
        // `** 2` is `x * x` in V8 (math::pow)
        (2.0 * profile.braking * distance + e * e).sqrt() - brake_step
    };
    let mut bearing = u.travel_dir.unwrap_or(u.rot);
    let mut arrival_speed = f64::INFINITY;
    if let Some(wp) = wp {
        // Brake for each bend ahead to the speed it can be taken at, and to a stop at the end of the route.
        bearing = math::atan2(wp.z - u.z, wp.x - u.x);
        let mut at = u.at();
        let mut remaining = 0.0;
        let mut i = 0;
        while i < path.len() && arrival_speed > 0.0 {
            let p = path[i];
            let incoming = math::atan2(p.z - at.z, p.x - at.x);
            let next = path.get(i + 1);
            remaining += math::hypot(p.x - at.x, p.z - at.z);
            at = p;
            let bend = match next {
                Some(next) => angle_delta(incoming, math::atan2(next.z - p.z, next.x - p.x)).abs(),
                None => pi,
            };
            if bend > 0.2 {
                arrival_speed = math::min(
                    arrival_speed,
                    reach(remaining, turn_speed(profile, bulge, bend)),
                );
            }
            if remaining > 40.0 {
                break;
            }
            i += 1;
        }
    }
    let hull_goal = if reverse { bearing + pi } else { bearing };
    let error = angle_delta(u.rot, hull_goal).abs();
    let rot = if wp.is_some() {
        turn_angle(u.rot, hull_goal, profile.hull_turn * dt)
    } else {
        u.rot
    };
    let alignment = math::max(0.0, math::cos(error));
    let to_point = wp.map_or(0.0, |wp| math::hypot(wp.x - u.x, wp.z - u.z));
    let mut desired = if wp.is_some() {
        math::min(
            top_speed * if reverse { profile.reverse_speed } else { 1.0 },
            arrival_speed,
        ) * alignment
    } else {
        0.0
    };
    // Drive while turning only on an arc tight enough to stay near the line and still pass through the waypoint.
    desired = math::min(
        math::min(desired, turn_speed(profile, bulge, error)),
        if error > 1e-3 {
            profile.hull_turn * to_point / (2.0 * math::sin(math::min(error, pi / 2.0)))
        } else {
            f64::INFINITY
        },
    );
    if reverse {
        desired = -desired;
    }
    let truthy = |v: f64| v != 0.0 && !v.is_nan();
    let increasing = desired != 0.0
        && (!truthy(speed) || math::sign(desired) == math::sign(speed))
        && desired.abs() > speed.abs();
    let rate = if increasing {
        if reverse {
            profile.reverse_acceleration
        } else {
            profile.acceleration
        }
    } else {
        profile.braking
    };
    let mut next_speed = speed + math::max(-rate * dt, math::min(rate * dt, desired - speed));
    if next_speed.abs() < 1e-6 {
        next_speed = 0.0;
    }
    // A changed direction brakes before accelerating the other way.
    if truthy(speed)
        && truthy(desired)
        && math::sign(speed) != math::sign(desired)
        && math::sign(next_speed) != math::sign(speed)
    {
        next_speed = 0.0;
    }
    let travel = if next_speed < 0.0 { rot + pi } else { rot };
    let distance = next_speed.abs() * dt;
    VehicleStep {
        x: u.x + math::cos(travel) * distance,
        z: u.z + math::sin(travel) * distance,
        rot,
        move_speed: next_speed,
        travel_dir: bearing,
    }
}
