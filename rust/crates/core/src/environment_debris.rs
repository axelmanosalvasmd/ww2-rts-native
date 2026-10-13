//! Bounded debris flight from `shared/debris-motion.js` at reference `e4e579a`.
//! The caller owns world contact queries and applies the settled body's world effects.
use crate::math;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DebrisLimits {
    pub active: usize,
    pub contacts: u32,
    pub lifetime: f64,
    pub max_step: f64,
}
pub const DEBRIS_LIMITS: DebrisLimits = DebrisLimits {
    active: 96,
    contacts: 8,
    lifetime: 2.5,
    max_step: 0.05,
};
pub const SECTION_MASS: [(&str, f64); 4] = [
    ("wood", 350.0),
    ("stone", 1200.0),
    ("concrete", 1600.0),
    ("steel", 1800.0),
];

/// Includes the collapse caller's fallback for an unknown material.
pub fn section_mass(material: &str) -> f64 {
    SECTION_MASS
        .iter()
        .find(|(name, _)| *name == material)
        .map_or(1200.0, |(_, mass)| *mass)
}

pub fn wreck_mass(kind: &str, radius: Option<f64>) -> f64 {
    match kind {
        "tiger" => 57000.0,
        "churchill" => 40000.0,
        "medium" => 30000.0,
        "tankdestroyer" => 28000.0,
        "tank" => 14000.0,
        "armoredcar" => 7800.0,
        "halftrack" => 9500.0,
        "flaktrack" => 10000.0,
        "rocket" => 11000.0,
        _ => {
            let radius = radius.unwrap_or(2.0);
            math::max(5000.0, radius * radius * 2200.0)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DebrisKind {
    Section,
    Wreck,
}
impl DebrisKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Section => "section",
            Self::Wreck => "wreck",
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}
impl Vec3 {
    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }
    fn finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite()
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ContactMaterial {
    pub rebound: Option<f64>,
    pub absorption: Option<f64>,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Contact {
    pub fraction: f64,
    pub normal: Vec3,
    pub material: Option<ContactMaterial>,
}

pub trait Environment {
    fn ground(&mut self, x: f64, z: f64) -> f64;
    fn material(&mut self, _x: f64, _z: f64) -> Option<ContactMaterial> {
        None
    }
    fn contact(&mut self, _from: Vec3, _to: Vec3, _body: &DebrisBody) -> Option<Contact> {
        None
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DebrisInput {
    pub id: String,
    pub kind: DebrisKind,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub dir: f64,
    pub impulse: f64,
    pub mass: f64,
    pub material: String,
    pub radius: f64,
    pub time: f64,
}
impl Default for DebrisInput {
    fn default() -> Self {
        Self {
            id: String::new(),
            kind: DebrisKind::Section,
            x: 0.0,
            y: 0.0,
            z: 0.0,
            dir: 0.0,
            impulse: 1.0,
            mass: 1000.0,
            material: "stone".into(),
            radius: 0.7,
            time: 0.0,
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct DebrisBody {
    pub id: String,
    pub kind: DebrisKind,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub vx: f64,
    pub vy: f64,
    pub vz: f64,
    pub dir: f64,
    pub material: String,
    pub mass: f64,
    pub radius: f64,
    /// Only x and z are used; y is zero.
    pub origin: Vec3,
    pub gravity: f64,
    pub drag: f64,
    pub stamp: f64,
    pub born: f64,
    pub contacts: u32,
    pub settled: bool,
    pub tilt: f64,
}
#[derive(Clone, Debug, PartialEq)]
pub struct DebrisRow {
    pub id: String,
    pub kind: DebrisKind,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub dir: f64,
    pub material: String,
    pub mass: f64,
    pub v: [f64; 3],
    pub gravity: f64,
    pub drag: f64,
    pub time: f64,
    pub born: f64,
    pub tilt: f64,
    pub settled: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DebrisError {
    NonfiniteInput,
    InvalidEnvironment,
    StepOutOfRange,
}
impl std::fmt::Display for DebrisError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::NonfiniteInput => "debris input or result is nonfinite",
            Self::InvalidEnvironment => "invalid debris contact or ground result",
            Self::StepOutOfRange => "debris step exceeds the representable subdivision count",
        })
    }
}
impl std::error::Error for DebrisError {}

impl DebrisBody {
    pub fn new(input: DebrisInput) -> Result<Self, DebrisError> {
        if ![
            input.x,
            input.y,
            input.z,
            input.dir,
            input.impulse,
            input.mass,
            input.radius,
            input.time,
        ]
        .iter()
        .all(|v| v.is_finite())
        {
            return Err(DebrisError::NonfiniteInput);
        }
        let wreck = input.kind == DebrisKind::Wreck;
        let strength = math::max(0.25, math::min(4.0, input.impulse));
        let momentum = strength * if wreck { 16000.0 } else { 800.0 };
        let speed = math::min(
            if wreck { 2.4 } else { 3.2 },
            momentum / math::max(1.0, input.mass),
        );
        Ok(Self {
            id: input.id,
            kind: input.kind,
            x: input.x,
            y: input.y,
            z: input.z,
            vx: math::cos(input.dir) * speed,
            vy: if wreck { 0.15 } else { 0.4 + strength * 0.15 },
            vz: math::sin(input.dir) * speed,
            dir: input.dir,
            material: input.material,
            mass: input.mass,
            radius: input.radius,
            origin: Vec3::new(input.x, 0.0, input.z),
            gravity: 9.81,
            drag: if wreck { 3.2 } else { 2.2 },
            stamp: input.time,
            born: input.time,
            contacts: 0,
            settled: false,
            tilt: 0.0,
        })
    }
    fn finite(&self) -> bool {
        [
            self.x,
            self.y,
            self.z,
            self.vx,
            self.vy,
            self.vz,
            self.dir,
            self.mass,
            self.radius,
            self.gravity,
            self.drag,
            self.stamp,
            self.born,
            self.tilt,
        ]
        .iter()
        .all(|v| v.is_finite())
            && self.origin.finite()
    }
    /// Body changes are atomic on errors. Environment callbacks can still record their queries.
    pub fn step<E: Environment + ?Sized>(
        &mut self,
        dt: f64,
        env: &mut E,
    ) -> Result<(), DebrisError> {
        if !dt.is_finite() || !self.finite() {
            return Err(DebrisError::NonfiniteInput);
        }
        if self.settled || dt <= 0.0 {
            return Ok(());
        }
        let pieces = math::max(1.0, (dt / DEBRIS_LIMITS.max_step).ceil());
        if pieces > f64::from(u32::MAX) {
            return Err(DebrisError::StepOutOfRange);
        }
        let step = dt / pieces;
        let mut body = self.clone();
        // A valid body settles within its lifetime. Bound work even for edited body state.
        let count = pieces as u32;
        for _ in 0..count.min(4096) {
            if body.settled {
                break;
            }
            body.substep(step, env)?;
            if !body.finite() {
                return Err(DebrisError::NonfiniteInput);
            }
        }
        if count > 4096 && !body.settled {
            return Err(DebrisError::StepOutOfRange);
        }
        *self = body;
        Ok(())
    }
    pub fn settle<E: Environment + ?Sized>(&mut self, env: &mut E) -> Result<(), DebrisError> {
        if !self.finite() {
            return Err(DebrisError::NonfiniteInput);
        }
        if self.settled {
            return Ok(());
        }
        let floor = ground(env, self.x, self.z)?;
        self.y = floor;
        self.vx = 0.0;
        self.vy = 0.0;
        self.vz = 0.0;
        self.settled = true;
        self.tilt = if self.kind == DebrisKind::Section {
            std::f64::consts::FRAC_PI_2
        } else {
            0.0
        };
        Ok(())
    }
    pub fn row(&self) -> DebrisRow {
        DebrisRow {
            id: self.id.clone(),
            kind: self.kind,
            x: self.x,
            y: self.y,
            z: self.z,
            dir: self.dir,
            material: self.material.clone(),
            mass: self.mass,
            v: [self.vx, self.vy, self.vz],
            gravity: self.gravity,
            drag: self.drag,
            time: self.stamp,
            born: self.born,
            tilt: self.tilt,
            settled: self.settled,
        }
    }
    fn substep<E: Environment + ?Sized>(
        &mut self,
        step: f64,
        env: &mut E,
    ) -> Result<(), DebrisError> {
        let a = Vec3::new(self.x, self.y, self.z);
        let b = Vec3::new(
            a.x + self.vx * step,
            a.y + self.vy * step - self.gravity * step * step / 2.0,
            a.z + self.vz * step,
        );
        if !b.finite() {
            return Err(DebrisError::NonfiniteInput);
        }
        let hit = env.contact(a, b, self);
        let fraction = hit.as_ref().map_or(1.0, |hit| hit.fraction);
        if let Some(hit) = hit
            && (!hit.fraction.is_finite()
                || !(0.0..=1.0).contains(&hit.fraction)
                || !hit.normal.finite())
        {
            return Err(DebrisError::InvalidEnvironment);
        }
        self.x = a.x + (b.x - a.x) * fraction;
        self.y = a.y + (b.y - a.y) * fraction;
        self.z = a.z + (b.z - a.z) * fraction;
        self.vy -= self.gravity * step * fraction;
        self.stamp += step;
        if let Some(hit) = hit {
            self.add_contact()?;
            self.reflect(hit.normal, hit.material)?;
            self.x += hit.normal.x * 0.002;
            self.y += hit.normal.y * 0.002;
            self.z += hit.normal.z * 0.002;
        }
        let floor = ground(env, self.x, self.z)?;
        if self.y <= floor + 0.015 {
            self.y = floor;
            if self.vy < -0.4 {
                self.add_contact()?;
                let material = env.material(self.x, self.z);
                self.reflect(Vec3::new(0.0, 1.0, 0.0), material)?;
            } else {
                self.vy = 0.0;
            }
            self.vx *= math::exp(-self.drag * 2.5 * step);
            self.vz *= math::exp(-self.drag * 2.5 * step);
        } else {
            self.vx *= math::exp(-self.drag * step);
            self.vz *= math::exp(-self.drag * step);
        }
        let age = self.stamp - self.born;
        let travel = math::hypot(self.x - self.origin.x, self.z - self.origin.z);
        let limit = if self.kind == DebrisKind::Wreck {
            2.0
        } else {
            4.0
        };
        self.tilt = if self.kind == DebrisKind::Section {
            math::min(std::f64::consts::FRAC_PI_2, age * 2.6)
        } else {
            math::min(0.025, self.vy.abs() * 0.01)
        };
        if travel >= limit {
            self.x = self.origin.x + (self.x - self.origin.x) * limit / travel;
            self.z = self.origin.z + (self.z - self.origin.z) * limit / travel;
        }
        if travel >= limit
            || age >= DEBRIS_LIMITS.lifetime
            || self.contacts >= DEBRIS_LIMITS.contacts
            || self.y <= floor + 0.01 && math::hypot3(self.vx, self.vy, self.vz) < 0.12
        {
            self.settle(env)?;
        }
        Ok(())
    }
    fn add_contact(&mut self) -> Result<(), DebrisError> {
        self.contacts = self
            .contacts
            .checked_add(1)
            .ok_or(DebrisError::NonfiniteInput)?;
        Ok(())
    }
    fn reflect(
        &mut self,
        normal: Vec3,
        material: Option<ContactMaterial>,
    ) -> Result<(), DebrisError> {
        let length = math::hypot3(normal.x, normal.y, normal.z);
        let length = if length == 0.0 { 1.0 } else { length };
        let normal = Vec3::new(normal.x / length, normal.y / length, normal.z / length);
        let dot = self.vx * normal.x + self.vy * normal.y + self.vz * normal.z;
        if dot >= 0.0 {
            return Ok(());
        }
        let material = material.unwrap_or_default();
        let rebound = material.rebound.unwrap_or(0.1);
        let absorption = material.absorption.unwrap_or(0.7);
        if !rebound.is_finite() || !absorption.is_finite() {
            return Err(DebrisError::InvalidEnvironment);
        }
        let rebound = math::max(0.0, math::min(0.35, rebound));
        let absorb = math::max(0.2, math::min(0.9, absorption));
        let factor = 1.0 + rebound;
        self.vx = (self.vx - factor * dot * normal.x) * (1.0 - absorb * 0.45);
        self.vy = (self.vy - factor * dot * normal.y) * (1.0 - absorb * 0.45);
        self.vz = (self.vz - factor * dot * normal.z) * (1.0 - absorb * 0.45);
        Ok(())
    }
}
fn ground<E: Environment + ?Sized>(env: &mut E, x: f64, z: f64) -> Result<f64, DebrisError> {
    let floor = env.ground(x, z);
    if floor.is_finite() {
        Ok(floor)
    } else {
        Err(DebrisError::InvalidEnvironment)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_steps_and_environment_leave_the_body_unchanged() {
        struct InvalidFloor;
        impl Environment for InvalidFloor {
            fn ground(&mut self, _: f64, _: f64) -> f64 {
                f64::NAN
            }
        }
        let mut body = DebrisBody::new(DebrisInput::default()).unwrap();
        let original = body.clone();
        assert_eq!(
            body.step(f64::INFINITY, &mut InvalidFloor),
            Err(DebrisError::NonfiniteInput)
        );
        assert_eq!(body, original);
        assert_eq!(
            body.step(0.05, &mut InvalidFloor),
            Err(DebrisError::InvalidEnvironment)
        );
        assert_eq!(body, original);
        assert_eq!(
            body.settle(&mut InvalidFloor),
            Err(DebrisError::InvalidEnvironment)
        );
        assert_eq!(body, original);

        struct Flat;
        impl Environment for Flat {
            fn ground(&mut self, _: f64, _: f64) -> f64 {
                0.0
            }
        }
        body.born = 1e10;
        body.y = 1e10;
        let edited = body.clone();
        assert_eq!(
            body.step(1000.0, &mut Flat),
            Err(DebrisError::StepOutOfRange)
        );
        assert_eq!(body, edited);
    }
}
