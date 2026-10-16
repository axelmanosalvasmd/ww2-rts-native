//! Pure projectile flight and contact geometry from `shared/projectiles.js` at reference
//! `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`. Coordinates are metres, time is seconds,
//! and rotations are radians. These helpers do not advance combat or apply damage.
//!
//! Finite, valid inputs retain the source calculation, axis and tie order. Invalid dimensions,
//! nonfinite input or unrepresentable calculations return an error. Hull rotation permits at most 4,096
//! full-turn reductions, so malformed angles cannot keep a simulation tick in an unbounded loop.

use crate::math;

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Point3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}
impl Point3 {
    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }
    fn checked(self) -> Result<Self, GeometryError> {
        finite(&[self.x, self.y, self.z])?;
        Ok(self)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BodyPosition {
    pub x: f64,
    pub z: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HullPosition {
    pub x: f64,
    pub z: f64,
    /// Missing rotation follows the reference's fallback to the other endpoint, then zero.
    pub rotation: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Flight {
    pub origin: Point3,
    pub velocity: Point3,
    pub gravity: f64,
    pub stamp: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Launch {
    pub origin: Point3,
    pub velocity: Point3,
    pub gravity: f64,
    pub duration: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Box3 {
    pub x: [f64; 2],
    pub y: [f64; 2],
    pub z: [f64; 2],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContactHit {
    pub fraction: f64,
    pub normal: Point3,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProjectileProfile {
    /// Grenade and artillery shots use an explicit duration instead of a fixed speed.
    pub speed: Option<f64>,
    pub gravity: f64,
    pub max_contacts: u32,
    pub near: f64,
}
pub const SMALL: ProjectileProfile = ProjectileProfile {
    speed: Some(230.0),
    gravity: 0.0,
    max_contacts: 2,
    near: 3.5,
};
pub const SNIPER: ProjectileProfile = ProjectileProfile {
    speed: Some(420.0),
    ..SMALL
};
pub const GUN: ProjectileProfile = ProjectileProfile {
    speed: Some(150.0),
    gravity: 9.81,
    ..SMALL
};
pub const FLAME: ProjectileProfile = ProjectileProfile {
    speed: Some(40.0),
    gravity: 0.0,
    max_contacts: 1,
    near: 1.5,
};
pub const GRENADE: ProjectileProfile = ProjectileProfile {
    speed: None,
    gravity: 25.0,
    max_contacts: 1,
    near: 0.0,
};
pub const ARTILLERY: ProjectileProfile = GRENADE;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContactMaterial {
    pub name: &'static str,
    pub hardness: f64,
    pub absorption: f64,
    pub rebound: f64,
}
pub const SOIL: ContactMaterial = ContactMaterial {
    name: "soil",
    hardness: 0.15,
    absorption: 0.9,
    rebound: 0.1,
};
pub const TIMBER: ContactMaterial = ContactMaterial {
    name: "timber",
    hardness: 0.3,
    absorption: 0.8,
    rebound: 0.2,
};
pub const STONE: ContactMaterial = ContactMaterial {
    name: "stone",
    hardness: 0.85,
    absorption: 0.6,
    rebound: 0.4,
};
pub const STEEL: ContactMaterial = ContactMaterial {
    name: "steel",
    hardness: 1.0,
    absorption: 0.55,
    rebound: 0.45,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContactState {
    pub explosive: bool,
    pub sequence: u32,
    pub max_contacts: u32,
    pub energy: f64,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContactResponse {
    pub energy: f64,
    pub velocity: Point3,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GeometryError {
    NonFinite,
    InvalidDimensions,
    InvalidDuration,
    ExcessiveRotation,
    GridTooLarge,
}

fn finite(values: &[f64]) -> Result<(), GeometryError> {
    if values.iter().all(|v| v.is_finite()) {
        Ok(())
    } else {
        Err(GeometryError::NonFinite)
    }
}
fn vertical(bottom: f64, top: f64) -> Result<(), GeometryError> {
    finite(&[bottom, top])?;
    if bottom <= top {
        Ok(())
    } else {
        Err(GeometryError::InvalidDimensions)
    }
}
fn nonnegative(value: f64) -> Result<(), GeometryError> {
    finite(&[value])?;
    if value >= 0.0 {
        Ok(())
    } else {
        Err(GeometryError::InvalidDimensions)
    }
}
fn hit(fraction: f64, normal: Point3) -> Result<Option<ContactHit>, GeometryError> {
    finite(&[fraction])?;
    normal.checked()?;
    Ok(Some(ContactHit { fraction, normal }))
}

/// Flame takes precedence over sniper, which takes precedence over the vehicle/terrain gun test.
pub fn projectile_profile(
    kind: &str,
    flame: bool,
    vehicle_damage: f64,
    shell_terrain: bool,
) -> Result<ProjectileProfile, GeometryError> {
    finite(&[vehicle_damage])?;
    Ok(if flame {
        FLAME
    } else if kind == "sniper" {
        SNIPER
    } else if vehicle_damage >= 20.0 || shell_terrain {
        GUN
    } else {
        SMALL
    })
}

pub fn flight_at(p: Flight, time: f64) -> Result<Point3, GeometryError> {
    p.origin.checked()?;
    p.velocity.checked()?;
    finite(&[p.gravity, p.stamp, time])?;
    let t = math::max(0.0, time - p.stamp);
    finite(&[t])?;
    Point3::new(
        p.origin.x + p.velocity.x * t,
        p.origin.y + p.velocity.y * t - 0.5 * p.gravity * t * t,
        p.origin.z + p.velocity.z * t,
    )
    .checked()
}
pub fn velocity_at(p: Flight, time: f64) -> Result<Point3, GeometryError> {
    p.origin.checked()?;
    p.velocity.checked()?;
    finite(&[p.gravity, p.stamp, time])?;
    let t = math::max(0.0, time - p.stamp);
    finite(&[t])?;
    Point3::new(p.velocity.x, p.velocity.y - p.gravity * t, p.velocity.z).checked()
}
pub fn launch_solution(
    from: Point3,
    at: Point3,
    speed: f64,
    gravity: f64,
    duration: Option<f64>,
) -> Result<Launch, GeometryError> {
    from.checked()?;
    at.checked()?;
    finite(&[speed, gravity])?;
    if let Some(t) = duration {
        finite(&[t])?;
        if t <= 0.0 {
            return Err(GeometryError::InvalidDuration);
        }
    }
    if duration.is_none() && speed <= 0.0 {
        return Err(GeometryError::InvalidDuration);
    }
    let dx = at.x - from.x;
    let dz = at.z - from.z;
    let d = math::hypot3(dx, dz, at.y - from.y);
    finite(&[dx, dz, d])?;
    let t = duration.unwrap_or_else(|| math::max(0.025, d / speed));
    finite(&[t])?;
    let velocity =
        Point3::new(dx / t, (at.y - from.y + 0.5 * gravity * t * t) / t, dz / t).checked()?;
    Ok(Launch {
        origin: from,
        velocity,
        gravity,
        duration: t,
    })
}

/// Earliest box entry. Equal entry fractions retain the earlier axis (x, then y, then z).
pub fn sweep_box(a: Point3, b: Point3, bounds: Box3) -> Result<Option<ContactHit>, GeometryError> {
    a.checked()?;
    b.checked()?;
    for range in [bounds.x, bounds.y, bounds.z] {
        vertical(range[0], range[1])?;
    }
    let mut lo = 0.0;
    let mut hi = 1.0;
    let mut normal = Point3::new(0.0, 1.0, 0.0);
    for (axis, (start, end, range)) in [
        (a.x, b.x, bounds.x),
        (a.y, b.y, bounds.y),
        (a.z, b.z, bounds.z),
    ]
    .into_iter()
    .enumerate()
    {
        let d = end - start;
        finite(&[d])?;
        if d.abs() < 1e-12 {
            if start < range[0] || start > range[1] {
                return Ok(None);
            }
            continue;
        }
        let mut enter = (range[0] - start) / d;
        let mut exit = (range[1] - start) / d;
        finite(&[enter, exit])?;
        let side = if d > 0.0 { -1.0 } else { 1.0 };
        if enter > exit {
            std::mem::swap(&mut enter, &mut exit);
        }
        if enter > lo {
            lo = enter;
            normal = Point3::new(0.0, 0.0, 0.0);
            match axis {
                0 => normal.x = side,
                1 => normal.y = side,
                _ => normal.z = side,
            }
        }
        hi = math::min(hi, exit);
        if lo > hi {
            return Ok(None);
        }
    }
    if hi < 0.0 || lo > 1.0 {
        Ok(None)
    } else {
        hit(math::max(0.0, lo), normal)
    }
}

/// Swept moving circle clipped to a vertical interval, using relative horizontal translation.
pub fn sweep_body(
    a: Point3,
    b: Point3,
    before: BodyPosition,
    after: BodyPosition,
    radius: f64,
    bottom: f64,
    top: f64,
) -> Result<Option<ContactHit>, GeometryError> {
    a.checked()?;
    b.checked()?;
    finite(&[before.x, before.z, after.x, after.z])?;
    nonnegative(radius)?;
    vertical(bottom, top)?;
    let dx = b.x - a.x - (after.x - before.x);
    let dz = b.z - a.z - (after.z - before.z);
    let ox = a.x - before.x;
    let oz = a.z - before.z;
    let aa = dx * dx + dz * dz;
    let bb = 2.0 * (ox * dx + oz * dz);
    let cc = ox * ox + oz * oz - radius * radius;
    finite(&[aa, bb, cc])?;
    let mut lo = 0.0;
    let mut hi = 1.0;
    if aa < 1e-12 {
        if cc > 0.0 {
            return Ok(None);
        }
    } else {
        let disc = bb * bb - 4.0 * aa * cc;
        finite(&[disc])?;
        if disc < 0.0 {
            return Ok(None);
        }
        lo = math::max(0.0, (-bb - disc.sqrt()) / (2.0 * aa));
        hi = math::min(1.0, (-bb + disc.sqrt()) / (2.0 * aa));
    }
    let dy = b.y - a.y;
    finite(&[dy])?;
    if dy.abs() < 1e-12 {
        if a.y < bottom || a.y > top {
            return Ok(None);
        }
    } else {
        let t0 = (bottom - a.y) / dy;
        let t1 = (top - a.y) / dy;
        finite(&[t0, t1])?;
        lo = math::max(lo, math::min(t0, t1));
        hi = math::min(hi, math::max(t0, t1));
    }
    if lo > hi {
        return Ok(None);
    }
    let nx = ox + dx * lo;
    let nz = oz + dz * lo;
    let length = math::hypot(nx, nz);
    let length = if length != 0.0 { length } else { 1.0 };
    hit(lo, Point3::new(nx / length, 0.0, nz / length))
}

pub fn segment_distance(
    a: BodyPosition,
    b: BodyPosition,
    at: BodyPosition,
) -> Result<f64, GeometryError> {
    finite(&[a.x, a.z, b.x, b.z, at.x, at.z])?;
    let dx = b.x - a.x;
    let dz = b.z - a.z;
    let d2 = dx * dx + dz * dz;
    finite(&[d2])?;
    let t = if d2 != 0.0 {
        let projection = ((at.x - a.x) * dx + (at.z - a.z) * dz) / d2;
        finite(&[projection])?;
        math::max(0.0, math::min(1.0, projection))
    } else {
        0.0
    };
    finite(&[t])?;
    let result = math::hypot(at.x - a.x - dx * t, at.z - a.z - dz * t);
    finite(&[result])?;
    Ok(result)
}

/// Crossed cell indices in source order, including both adjoining surfaces at a corner touch.
/// Duplicates are retained. Each grid side is at most 2^26 cells, and the area must fit usize.
/// The caller controls dimensions and the resulting allocation.
pub fn crossed_cells(
    a: BodyPosition,
    b: BodyPosition,
    cell: f64,
    w: usize,
    h: usize,
) -> Result<Vec<usize>, GeometryError> {
    finite(&[a.x, a.z, b.x, b.z, cell])?;
    if cell <= 0.0 {
        return Err(GeometryError::InvalidDimensions);
    }
    let limit = w
        .checked_add(h)
        .and_then(|n| n.checked_add(4))
        .ok_or(GeometryError::GridTooLarge)?;
    if w.checked_mul(h).is_none() || w > (1usize << 26) || h > (1usize << 26) {
        return Err(GeometryError::GridTooLarge);
    }
    let dx = b.x - a.x;
    let dz = b.z - a.z;
    finite(&[dx, dz, a.x / cell, a.z / cell])?;
    let sx = math::sign(dx);
    let sz = math::sign(dz);
    let mut x = (a.x / cell).floor();
    let mut z = (a.z / cell).floor();
    let mut enter = 0.0;
    let tx_step = if dx != 0.0 {
        cell / dx.abs()
    } else {
        f64::INFINITY
    };
    let tz_step = if dz != 0.0 {
        cell / dz.abs()
    } else {
        f64::INFINITY
    };
    let mut tx = if dx != 0.0 {
        ((if sx > 0.0 { (x + 1.0) * cell } else { x * cell }) - a.x) / dx
    } else {
        f64::INFINITY
    };
    let mut tz = if dz != 0.0 {
        ((if sz > 0.0 { (z + 1.0) * cell } else { z * cell }) - a.z) / dz
    } else {
        f64::INFINITY
    };
    if dx != 0.0 {
        finite(&[tx_step, tx])?;
    }
    if dz != 0.0 {
        finite(&[tz_step, tz])?;
    }
    let mut result = Vec::new();
    let push = |result: &mut Vec<usize>, x: f64, z: f64| {
        if x >= 0.0 && z >= 0.0 && x < w as f64 && z < h as f64 {
            result.push((z * w as f64 + x) as usize);
        }
    };
    for _ in 0..limit {
        if enter > 1.0 {
            break;
        }
        push(&mut result, x, z);
        if math::min(tx, tz) > 1.0 {
            break;
        }
        if (tx - tz).abs() < 1e-10 {
            push(&mut result, x + sx, z);
            push(&mut result, x, z + sz);
            enter = tx;
            x += sx;
            z += sz;
            tx += tx_step;
            tz += tz_step;
        } else if tx < tz {
            enter = tx;
            x += sx;
            tx += tx_step;
        } else {
            enter = tz;
            z += sz;
            tz += tz_step;
        }
    }
    Ok(result)
}

pub fn contact_response(
    p: ContactState,
    velocity: Point3,
    normal: Point3,
    material: ContactMaterial,
) -> Result<Option<ContactResponse>, GeometryError> {
    velocity.checked()?;
    normal.checked()?;
    finite(&[
        p.energy,
        material.hardness,
        material.absorption,
        material.rebound,
    ])?;
    let speed = math::hypot3(velocity.x, velocity.y, velocity.z);
    let dot = velocity.x * normal.x + velocity.y * normal.y + velocity.z * normal.z;
    finite(&[speed, dot])?;
    let shallow = speed > 1.0 && dot < 0.0 && -dot / speed < 0.28;
    if p.explosive
        || !shallow
        || material.hardness < 0.7
        || p.sequence >= p.max_contacts
        || p.energy < 0.3
    {
        return Ok(None);
    }
    let retain = math::max(0.15, math::min(0.7, 1.0 - material.absorption));
    let k = retain.sqrt();
    let rebound = math::max(0.1, math::min(1.0, material.rebound));
    let velocity = Point3::new(
        (velocity.x - (1.0 + rebound) * dot * normal.x) * k,
        (velocity.y - (1.0 + rebound) * dot * normal.y) * k,
        (velocity.z - (1.0 + rebound) * dot * normal.z) * k,
    )
    .checked()?;
    let energy = p.energy
        * (velocity.x * velocity.x + velocity.y * velocity.y + velocity.z * velocity.z)
        / (speed * speed);
    finite(&[energy])?;
    Ok(Some(ContactResponse { energy, velocity }))
}

/// Long naval capsule footprint, sampled in bounded angular pieces. Box wins exact cap ties.
#[allow(clippy::too_many_arguments)]
pub fn sweep_hull(
    a: Point3,
    b: Point3,
    before: HullPosition,
    after: HullPosition,
    radius: f64,
    length: f64,
    bottom: f64,
    top: f64,
) -> Result<Option<ContactHit>, GeometryError> {
    a.checked()?;
    b.checked()?;
    finite(&[before.x, before.z, after.x, after.z])?;
    nonnegative(radius)?;
    nonnegative(length)?;
    vertical(bottom, top)?;
    let initial = before.rotation.or(after.rotation).unwrap_or(0.0);
    let mut turn = after.rotation.unwrap_or(0.0) - initial;
    finite(&[initial, turn])?;
    let pi = std::f64::consts::PI;
    let mut reductions = 0;
    while turn > pi || turn < -pi {
        if reductions >= 4096 {
            return Err(GeometryError::ExcessiveRotation);
        }
        if turn > pi {
            turn -= pi * 2.0;
        } else {
            turn += pi * 2.0;
        }
        reductions += 1;
    }
    let pieces = math::max(1.0, math::min(12.0, (turn.abs() / 0.12).ceil())) as u32;
    for i in 0..pieces {
        let lo = f64::from(i) / f64::from(pieces);
        let hi = f64::from(i + 1) / f64::from(pieces);
        let angle = initial + turn * (lo + hi) / 2.0;
        let cx = math::cos(angle);
        let cz = math::sin(angle);
        let rotate = |at: Point3, center: BodyPosition| {
            Point3::new(
                (at.x - center.x) * cx + (at.z - center.z) * cz,
                at.y,
                -(at.x - center.x) * cz + (at.z - center.z) * cx,
            )
        };
        let at = |t| {
            Point3::new(
                a.x + (b.x - a.x) * t,
                a.y + (b.y - a.y) * t,
                a.z + (b.z - a.z) * t,
            )
        };
        let center = |t| BodyPosition {
            x: before.x + (after.x - before.x) * t,
            z: before.z + (after.z - before.z) * t,
        };
        let q0 = rotate(at(lo), center(lo));
        let q1 = rotate(at(hi), center(hi));
        let mut best = sweep_box(
            q0,
            q1,
            Box3 {
                x: [-length, length],
                y: [bottom, top],
                z: [-radius, radius],
            },
        )?;
        for side in [-length, length] {
            let center = BodyPosition { x: side, z: 0.0 };
            if let Some(candidate) = sweep_body(q0, q1, center, center, radius, bottom, top)?
                && best.is_none_or(|best| candidate.fraction < best.fraction)
            {
                best = Some(candidate);
            }
        }
        if let Some(best) = best {
            let n = best.normal;
            return hit(
                lo + (hi - lo) * best.fraction,
                Point3::new(n.x * cx - n.z * cz, n.y, n.x * cz + n.z * cx),
            );
        }
    }
    Ok(None)
}
