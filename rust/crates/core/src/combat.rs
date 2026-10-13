//! Standalone damage, suppression and threat rules from pinned `shared/sim.js`.
//! Inputs describe combat facts without changing `Game`. Results are ordered mutations that
//! an integrator must apply, including rider disembarkation and source experience.

use crate::{
    catalog::{Catalog, CatalogError, Value},
    economy::{self, Mode, TechState},
    game::Point,
    math,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Catalog(CatalogError),
    Economy(economy::Error),
    InvalidInput(&'static str),
}
impl From<CatalogError> for Error {
    fn from(v: CatalogError) -> Self {
        Self::Catalog(v)
    }
}
impl From<economy::Error> for Error {
    fn from(v: economy::Error) -> Self {
        Self::Economy(v)
    }
}
fn finite(v: f64, name: &'static str) -> Result<(), Error> {
    if v.is_finite() {
        Ok(())
    } else {
        Err(Error::InvalidInput(name))
    }
}
fn positive(v: f64, name: &'static str) -> Result<(), Error> {
    finite(v, name)?;
    if v < 0.0 {
        Err(Error::InvalidInput(name))
    } else {
        Ok(())
    }
}
fn point(p: Point) -> Result<(), Error> {
    finite(p.x, "position")?;
    finite(p.z, "position")
}
fn num(v: &Value, key: &str) -> Result<f64, Error> {
    v.get(key)
        .and_then(Value::number)
        .ok_or_else(|| Error::Catalog(CatalogError(format!("missing numeric combat rule {key}"))))
}
fn flag(v: &Value, key: &str) -> bool {
    v.get(key).and_then(Value::boolean).unwrap_or(false)
}
fn cfg() -> Result<&'static Value, Error> {
    Ok(Catalog::compiled()?.table("CFG")?)
}
fn cfg_num(name: &str) -> Result<f64, Error> {
    num(cfg()?, name)
}
fn nested_num(section: &str, key: &str) -> Result<f64, Error> {
    num(
        cfg()?.get(section).ok_or_else(|| {
            Error::Catalog(CatalogError(format!("missing combat section {section}")))
        })?,
        key,
    )
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RetreatState {
    pub retreating: bool,
    pub forced: bool,
    pub stranded: bool,
    pub move_speed: f64,
    pub vx: f64,
    pub vz: f64,
}
/// A forced withdrawal gets protection only while moving and not stranded.
pub fn retreat_protected(state: RetreatState) -> Result<bool, Error> {
    finite(state.move_speed, "move speed")?;
    finite(state.vx, "velocity")?;
    finite(state.vz, "velocity")?;
    let speed = if state.move_speed != 0.0 {
        state.move_speed
    } else {
        math::hypot(state.vx, state.vz)
    };
    finite(speed, "speed")?;
    Ok(state.retreating && (!state.forced || !state.stranded && speed.abs() > 0.05))
}
pub fn veterancy(kind: &str, xp: f64) -> Result<u32, Error> {
    positive(xp, "experience")?;
    let cost = num(Catalog::compiled()?.unit(kind)?, "cost")?;
    let thresholds = cfg()?
        .get("vetXp")
        .and_then(Value::array)
        .ok_or_else(|| Error::Catalog(CatalogError("missing vetXp".into())))?;
    Ok(if cost == 0.0 {
        0
    } else {
        thresholds
            .iter()
            .filter(|v| v.number().is_some_and(|threshold| xp >= threshold * cost))
            .count() as u32
    })
}
pub fn role_mul(attacker: &str, target: &str) -> Result<f64, Error> {
    let c = Catalog::compiled()?;
    c.unit(attacker)?;
    let d = c.unit(target)?;
    Ok(if attacker == "at" || attacker == "tankdestroyer" {
        if flag(d, "infantry") || flag(d, "structure") {
            1.0
        } else {
            3.0
        }
    } else if attacker == "flamer" {
        if flag(d, "infantry") { 2.0 } else { 1.0 }
    } else if attacker == "sniper" {
        if ["mg", "at", "mortar", "flak", "howitzer"].contains(&target) {
            3.0
        } else {
            1.0
        }
    } else if ["mg", "rifle", "ranger", "conscript", "commando", "engineer"].contains(&attacker)
        && flag(d, "infantry")
    {
        2.0
    } else {
        1.0
    })
}
pub fn armor_mul(kind: &str, at: Point, rotation: f64, from: Point) -> Result<f64, Error> {
    point(at)?;
    point(from)?;
    finite(rotation, "rotation")?;
    let d = Catalog::compiled()?.unit(kind)?;
    let dx = from.x - at.x;
    let dz = from.z - at.z;
    finite(dx, "bearing")?;
    finite(dz, "bearing")?;
    let angle = math::atan2(dz, dx) - rotation;
    finite(angle, "bearing")?;
    let c = math::cos(angle);
    let front = d.get("frontArmor").and_then(Value::number).unwrap_or(0.0);
    Ok(if c < -0.5 {
        2.0
    } else if front != 0.0 && c > 0.5 {
        front
    } else {
        1.0
    })
}
pub fn bunker_mul(kind: &str, mode: Mode, defenders: u32) -> Result<f64, Error> {
    Catalog::compiled()?.unit(kind)?;
    if ["bunker", "kaiju"].contains(&kind) && mode == Mode::Horde {
        if defenders == 0 {
            return Err(Error::InvalidInput("horde defenders"));
        }
        Ok(1.0 / f64::from(defenders))
    } else {
        Ok(1.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ThreatState {
    pub id: u32,
    pub hit_by: Option<u32>,
    pub hit_at: f64,
    pub target_id: Option<u32>,
    pub shot_at: Option<f64>,
}
pub fn fired_on_by(tick: f64, unit: ThreatState, other: ThreatState) -> Result<bool, Error> {
    positive(tick, "tick")?;
    finite(unit.hit_at, "hit time")?;
    if let Some(t) = other.shot_at {
        finite(t, "shot time")?;
    }
    let last = if unit.hit_by == Some(other.id) {
        unit.hit_at
    } else if other.target_id == Some(unit.id) {
        other.shot_at.unwrap_or(-1e9)
    } else {
        -1e9
    };
    Ok(tick - last
        <= nested_num("behavior", "threatTime")?
            / Catalog::compiled()?
                .table("TICK")?
                .number()
                .ok_or(Error::InvalidInput("tick duration"))?)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WeaponDamage {
    pub infantry: f64,
    pub vehicle: f64,
    pub flame: Option<f64>,
}
impl WeaponDamage {
    pub fn from_catalog(kind: &str) -> Result<Self, Error> {
        let d = Catalog::compiled()?.unit(kind)?;
        let w = d
            .get("w")
            .ok_or(Error::InvalidInput("unit has no weapon"))?;
        Ok(Self {
            infantry: num(w, "inf")?,
            vehicle: num(w, "veh")?,
            flame: w.get("flame").and_then(Value::number),
        })
    }
    fn checked(self) -> Result<(), Error> {
        positive(self.infantry, "infantry damage")?;
        positive(self.vehicle, "vehicle damage")?;
        if let Some(v) = self.flame {
            positive(v, "flame multiplier")?;
        }
        Ok(())
    }
    fn is_flame(self) -> bool {
        self.flame.is_some_and(|v| v != 0.0)
    }
}
#[derive(Clone, Debug)]
pub struct UnitCombatState<'a> {
    pub id: u32,
    pub owner: i32,
    pub kind: &'a str,
    pub hp: f64,
    pub xp: f64,
    pub at: Point,
    pub rotation: f64,
    pub retreat: RetreatState,
    pub garrisoned: bool,
    pub in_trench: bool,
    pub sprint: f64,
    pub suppression: f64,
    pub hit_by: Option<u32>,
    /// Original cargo order, including missing or repeated IDs, is significant.
    pub cargo: &'a [u32],
}
impl UnitCombatState<'_> {
    fn checked(&self) -> Result<(), Error> {
        Catalog::compiled()?.unit(self.kind)?;
        finite(self.hp, "health")?;
        positive(self.xp, "experience")?;
        point(self.at)?;
        finite(self.rotation, "rotation")?;
        positive(self.sprint, "sprint")?;
        positive(self.suppression, "suppression")?;
        retreat_protected(self.retreat)?;
        Ok(())
    }
}
#[derive(Clone, Copy, Debug)]
pub struct CombatSource<'a> {
    pub kind: &'a str,
    pub xp: f64,
    pub airborne: bool,
}
#[derive(Clone, Copy, Debug)]
pub struct Impact {
    pub owner: i32,
    pub shooter: u32,
    pub weapon: WeaponDamage,
    pub damage_scale: Option<f64>,
    pub energy: f64,
    pub armor_piercing: bool,
    pub ap_naval_only: bool,
    pub ap_multiplier: f64,
    pub launch_origin: Point,
    pub at: Point,
    pub direction: Point,
}
#[derive(Clone, Copy, Debug)]
pub struct CombatContext {
    pub tick: f64,
    pub mode: Mode,
    pub horde_defenders: u32,
    /// Authoritative alliance result for the projectile owner and this target owner.
    pub allied: bool,
    pub tech_enabled: bool,
    pub attacker_tech: Option<TechState>,
    pub defender_tech: Option<TechState>,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DeathImpulse {
    pub direction: f64,
    pub magnitude: f64,
}
/// Ordered mutations. No event is applied by this module.
#[derive(Clone, Debug, PartialEq)]
pub enum CombatEvent {
    Health {
        unit: u32,
        value: f64,
    },
    /// Apply in the original cargo order. For an existing rider remove ALL its occurrences from
    /// carrier cargo, set riding=0, and find nearestFree at its current position with blockOf
    /// its own kind, place at that cell center and update the spatial grid, then raise suppression
    /// to minimum_suppression. A missing rider leaves its numeric cargo ID unchanged, as in the
    /// reference. Normal (not thrown) leaveCarrier does not clear orders or targets,
    /// inflict damage, drown the rider, or emit a shot. Those states must remain unchanged.
    DisembarkRider {
        carrier: u32,
        rider: u32,
        minimum_suppression: f64,
    },
    LastHit {
        unit: u32,
        owner: i32,
    },
    DeathImpulse {
        unit: u32,
        value: DeathImpulse,
    },
    ResetRetarget {
        unit: u32,
    },
    HitSource {
        unit: u32,
        shooter: u32,
        tick: f64,
        from: Point,
    },
    AntiTankHit {
        unit: u32,
        tick: f64,
    },
    Experience {
        shooter: u32,
        value: f64,
    },
    SuppressionRecord {
        unit: u32,
    },
    Suppression {
        unit: u32,
        value: f64,
    },
}
#[derive(Clone, Debug, PartialEq)]
pub struct DamageResult {
    pub damage: f64,
    pub events: Vec<CombatEvent>,
}
fn hit_events(
    events: &mut Vec<CombatEvent>,
    target: &UnitCombatState<'_>,
    shooter: u32,
    tick: f64,
    from: Point,
) {
    if target.hit_by != Some(shooter) {
        events.push(CombatEvent::ResetRetarget { unit: target.id });
    }
    events.push(CombatEvent::HitSource {
        unit: target.id,
        shooter,
        tick,
        from,
    });
}

pub fn flight_damage(
    context: CombatContext,
    p: Impact,
    source: CombatSource<'_>,
    target: &UnitCombatState<'_>,
) -> Result<DamageResult, Error> {
    target.checked()?;
    p.weapon.checked()?;
    point(p.at)?;
    point(p.direction)?;
    point(p.launch_origin)?;
    positive(context.tick, "tick")?;
    positive(p.energy, "energy")?;
    positive(source.xp, "source experience")?;
    positive(p.ap_multiplier, "AP multiplier")?;
    let scale = p.damage_scale.unwrap_or(1.0);
    positive(scale, "damage scale")?;
    let d = Catalog::compiled()?.unit(target.kind)?;
    Catalog::compiled()?.unit(source.kind)?;
    if target.hp <= 0.0 {
        return Ok(DamageResult {
            damage: 0.0,
            events: vec![],
        });
    }
    let inf = flag(d, "infantry");
    let structure = flag(d, "structure");
    let mut damage = (if inf {
        p.weapon.infantry
    } else {
        p.weapon.vehicle
    }) * scale
        * p.energy;
    if p.armor_piercing && !inf && (!p.ap_naval_only || flag(d, "naval")) {
        damage *= p.ap_multiplier;
    }
    if retreat_protected(target.retreat)? {
        damage *= cfg_num("retreatDamage")?;
    }
    if p.weapon.is_flame() && (target.garrisoned || target.in_trench) {
        damage *= p.weapon.flame.unwrap();
    }
    damage *= 1.0 - cfg_num("vetArmor")? * f64::from(veterancy(target.kind, target.xp)?);
    if flag(d, "building") {
        damage = (if p.weapon.vehicle >= 20.0 {
            p.weapon.vehicle
        } else {
            p.weapon.infantry * nested_num("classic", "smallArms")?
        }) * scale
            * p.energy;
    } else if structure {
        damage *= nested_num("assault", "directMul")?
            * bunker_mul(target.kind, context.mode, context.horde_defenders)?;
    } else if !inf {
        damage *= armor_mul(
            target.kind,
            target.at,
            target.rotation,
            Point {
                x: p.at.x - p.direction.x,
                z: p.at.z - p.direction.z,
            },
        )? * bunker_mul(target.kind, context.mode, context.horde_defenders)?;
    }
    if context.tech_enabled {
        damage *= economy::tech_mul(
            context.attacker_tech,
            Some(source.kind),
            context.defender_tech,
            target.kind,
        )?;
    }
    positive(damage, "damage result")?;
    let hp = target.hp - damage;
    finite(hp, "health result")?;
    let mut events = vec![CombatEvent::Health {
        unit: target.id,
        value: hp,
    }];
    if !target.cargo.is_empty() && flag(d, "riders") && p.owner >= 0 && !context.allied {
        for &rider in target.cargo {
            events.push(CombatEvent::DisembarkRider {
                carrier: target.id,
                rider,
                minimum_suppression: cfg_num("riderShock")?,
            });
        }
    }
    if damage > 0.0 {
        events.push(CombatEvent::LastHit {
            unit: target.id,
            owner: p.owner,
        });
        events.push(CombatEvent::DeathImpulse {
            unit: target.id,
            value: DeathImpulse {
                direction: math::atan2(p.direction.z, p.direction.x),
                magnitude: math::max(0.25, math::min(4.0, damage / 100.0)),
            },
        });
    }
    if inf || (p.weapon.vehicle >= 20.0 && !source.airborne) {
        hit_events(
            &mut events,
            target,
            p.shooter,
            context.tick,
            p.launch_origin,
        );
    }
    if !inf && !structure && p.weapon.vehicle >= 20.0 && damage > 0.0 {
        events.push(CombatEvent::AntiTankHit {
            unit: target.id,
            tick: context.tick,
        });
    }
    let xp = source.xp
        + (math::min(math::max(0.0, target.hp), damage)
            + if hp <= 0.0 && target.hp > 0.0 {
                num(d, "cost")? * 0.2
            } else {
                0.0
            });
    finite(xp, "experience result")?;
    events.push(CombatEvent::Experience {
        shooter: p.shooter,
        value: xp,
    });
    Ok(DamageResult { damage, events })
}

#[derive(Clone, Copy, Debug)]
pub struct SuppressionInput {
    pub owner: i32,
    pub shooter: u32,
    pub amount: f64,
    pub scale: Option<f64>,
    pub flame: bool,
    pub launch_origin: Point,
    pub already_suppressed: bool,
    /// Exact outputs of coverMul and coverBehind. Terrain sampling belongs to the integrator.
    pub cover_mul: f64,
    pub cover_behind: f64,
}
pub fn flight_suppress(
    context: CombatContext,
    p: SuppressionInput,
    target: &UnitCombatState<'_>,
) -> Result<Vec<CombatEvent>, Error> {
    target.checked()?;
    positive(context.tick, "tick")?;
    positive(p.amount, "suppression amount")?;
    point(p.launch_origin)?;
    let scale = p.scale.unwrap_or(1.0);
    positive(scale, "suppression scale")?;
    positive(p.cover_mul, "cover multiplier")?;
    positive(p.cover_behind, "cover behind")?;
    if p.cover_behind > 1.0 {
        return Err(Error::InvalidInput("cover behind"));
    }
    if p.amount == 0.0
        || p.already_suppressed
        || target.hp <= 0.0
        || !flag(Catalog::compiled()?.unit(target.kind)?, "infantry")
        || context.allied
        || target.retreat.retreating
        || target.sprint > 0.0
    {
        return Ok(vec![]);
    }
    let cover = if p.flame {
        1.0
    } else {
        math::min(
            p.cover_mul,
            1.0 - (1.0 - cfg_num("coverMul")?) * p.cover_behind,
        )
    };
    let value = math::min(
        100.0,
        target.suppression
            + p.amount
                * scale
                * cover
                * (1.0 - cfg_num("vetSupp")? * f64::from(veterancy(target.kind, target.xp)?)),
    );
    finite(value, "suppression result")?;
    let mut events = vec![
        CombatEvent::SuppressionRecord { unit: target.id },
        CombatEvent::Suppression {
            unit: target.id,
            value,
        },
    ];
    hit_events(
        &mut events,
        target,
        p.shooter,
        context.tick,
        p.launch_origin,
    );
    Ok(events)
}
