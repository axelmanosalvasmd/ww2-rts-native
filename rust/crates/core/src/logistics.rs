//! Physical ammunition and timed reserves, keyed by the authoritative unit ID.
//!
//! Unit identity, ownership, movement and combat fields remain with the caller. This module
//! changes only reserves and stock. Forced/stranded flags describe shortages; the caller owns
//! retreat orders, route resets, supply routes, convoys and snapshot delivery.
use crate::{
    catalog::{Catalog, CatalogError, Value},
    grid::UnitId,
    math,
};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Conquest,
    Classic,
    Annihilation,
    World,
    Assault,
    Horde,
    Tutorial,
    Other,
}
impl Mode {
    pub fn from_reference(kind: Option<&str>) -> Self {
        match kind {
            None | Some("conquest") => Self::Conquest,
            Some("classic") => Self::Classic,
            Some("annihilation") => Self::Annihilation,
            Some("world") => Self::World,
            Some("assault") => Self::Assault,
            Some("horde") => Self::Horde,
            Some("tutorial") => Self::Tutorial,
            _ => Self::Other,
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Context {
    pub enabled_override: Option<bool>,
    pub scenario: bool,
    pub mode: Mode,
}
pub fn enabled_for(context: Context) -> bool {
    context.enabled_override.unwrap_or(
        !context.scenario
            && matches!(
                context.mode,
                Mode::Conquest | Mode::Classic | Mode::Annihilation | Mode::World
            ),
    )
}
#[derive(Clone, Copy, Debug)]
pub struct UnitFacts<'a> {
    pub id: UnitId,
    pub kind: &'a str,
    pub owner: Option<usize>,
    pub owner_exists: bool,
    pub guard_home: bool,
    pub riding: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct Weapon {
    pub range: f64,
    pub interval: f64,
    pub infantry_damage: f64,
    pub vehicle_damage: f64,
    pub salvo: bool,
    pub rockets: f64,
    pub per_model: bool,
    pub shell_terrain: bool,
}
#[derive(Clone, Copy, Debug)]
pub struct Definition<'a> {
    pub name: &'a str,
    pub speed: f64,
    pub models: f64,
    pub infantry: bool,
    pub building: bool,
    pub structure: bool,
    pub air: bool,
    pub naval: bool,
    pub logistics_truck: bool,
    pub rail: bool,
    pub weapon: Option<Weapon>,
}
impl Definition<'static> {
    pub fn from_catalog(kind: &str) -> Result<Self, CatalogError> {
        Ok(Self::from_value(Catalog::compiled()?.unit(kind)?))
    }
}
impl<'a> Definition<'a> {
    /// Project trusted numeric metadata, not arbitrary JavaScript value coercions.
    /// Borrow only logistics metadata from an already decoded definition. Missing numbers retain
    /// JavaScript's undefined comparison behavior; nullish rockets use the source default of one.
    pub fn from_value(def: &'a Value) -> Self {
        let num =
            |value: &Value, key: &str| value.get(key).and_then(Value::number).unwrap_or(f64::NAN);
        Self {
            name: def.get("name").and_then(Value::string).unwrap_or(""),
            speed: num(def, "speed"),
            models: num(def, "models"),
            infantry: flag(def, "infantry"),
            building: flag(def, "building"),
            structure: flag(def, "structure"),
            air: flag(def, "air"),
            naval: flag(def, "naval"),
            logistics_truck: flag(def, "logisticsTruck"),
            rail: flag(def, "rail"),
            weapon: def.get("w").filter(|v| truthy(v)).map(|w| Weapon {
                range: num(w, "range"),
                interval: num(w, "interval"),
                infantry_damage: num(w, "inf"),
                vehicle_damage: num(w, "veh"),
                salvo: flag(w, "salvo"),
                rockets: w.get("rockets").and_then(Value::number).unwrap_or(1.0),
                per_model: flag(w, "perModel"),
                shell_terrain: flag(w, "shellTerrain"),
            }),
        }
    }
}
fn flag(value: &Value, key: &str) -> bool {
    value.get(key).is_some_and(truthy)
}
fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::Number(v) => *v != 0.0 && !v.is_nan(),
        Value::String(v) => !v.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

/// Constants from `shared/logistics.js` at reference `e4e579a`, in rounds or seconds.
/// The oracle fixture checks every field against the source `LOGISTICS` export.
/// Convoy and territory-supply tuning belongs to those later subsystems.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReserveRules {
    pub ammo_seconds: f64,
    pub provisions: f64,
    pub fuel: f64,
    pub emergency: f64,
    pub low: f64,
    pub recovery: f64,
    pub warning: f64,
    pub unload_seconds: f64,
    pub small_ammo_price: f64,
    pub shell_ammo_price: f64,
    pub heavy_ammo_price: f64,
}
pub const RESERVE_RULES: ReserveRules = ReserveRules {
    ammo_seconds: 90.0,
    provisions: 120.0,
    fuel: 180.0,
    emergency: 60.0,
    low: 0.25,
    recovery: 0.5,
    warning: 20.0,
    unload_seconds: 8.0,
    small_ammo_price: 0.01,
    shell_ammo_price: 0.05,
    heavy_ammo_price: 0.10,
};
const EPS: f64 = 1e-9;
fn positive(value: f64) -> f64 {
    if value.is_finite() {
        math::max(0.0, value)
    } else {
        0.0
    }
}
fn rounds(value: f64) -> f64 {
    positive(value).floor()
}

pub fn ammo_price(def: Option<&Definition<'_>>) -> f64 {
    let Some(def) = def else {
        return RESERVE_RULES.small_ammo_price;
    };
    let Some(w) = def.weapon else {
        return RESERVE_RULES.small_ammo_price;
    };
    if w.salvo {
        let name = def.name.to_ascii_lowercase();
        return if name.contains("howitzer")
            || name.contains("rocket")
            || w.rockets > 1.0
            || w.range >= 80.0
        {
            RESERVE_RULES.heavy_ammo_price
        } else {
            RESERVE_RULES.shell_ammo_price
        };
    }
    if w.shell_terrain || w.vehicle_damage >= 20.0 {
        RESERVE_RULES.shell_ammo_price
    } else {
        RESERVE_RULES.small_ammo_price
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Reserves {
    /// Whole carried projectiles; capacity and cargo price are fixed at initialization.
    pub ammo: f64,
    pub ammo_max: f64,
    pub ammo_price: f64,
    /// Paid fractional cargo credit. `None` supports records from before this field existed.
    pub ammo_credit: Option<f64>,
    pub provisions: f64,
    pub fuel: Option<f64>,
    pub emergency: Option<f64>,
    pub warning: Option<f64>,
    pub forced: bool,
    pub stranded: bool,
    pub exhausted: bool,
    /// Territory service adds these fields after reserve initialization.
    pub supply_rate: Option<f64>,
    pub cut_for: Option<f64>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Stock {
    /// Ammunition cargo credits, rather than physical projectile count.
    pub ammo: f64,
    pub provisions: f64,
    pub fuel: f64,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Shortage {
    pub low_ammo: bool,
    pub low_provisions: bool,
    pub exhausted: bool,
    pub forced: bool,
    pub stranded: bool,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LogisticsState {
    units: BTreeMap<UnitId, Reserves>,
}
impl LogisticsState {
    pub fn get(&self, id: UnitId) -> Option<&Reserves> {
        self.units.get(&id)
    }
    /// Owner-controlled access, as for the reference unit object. No persistence validation occurs.
    pub fn get_mut(&mut self, id: UnitId) -> Option<&mut Reserves> {
        self.units.get_mut(&id)
    }
    /// Install a trusted caller-owned record. This is not an untrusted-save decoder or validator.
    /// Identity and unit facts stay outside this state.
    pub fn insert(&mut self, id: UnitId, reserves: Reserves) -> Option<Reserves> {
        self.units.insert(id, reserves)
    }
    pub fn remove(&mut self, id: UnitId) -> Option<Reserves> {
        self.units.remove(&id)
    }
    pub fn iter(&self) -> impl Iterator<Item = (UnitId, &Reserves)> {
        self.units.iter().map(|(id, r)| (*id, r))
    }
    /// Eligibility is checked before reusing existing state, exactly as in the reference.
    /// Repeated initialization preserves partial stock and never manufactures a refill.
    pub fn initialize_unit(
        &mut self,
        context: Context,
        u: UnitFacts<'_>,
        def: Option<&Definition<'_>>,
    ) -> Option<&Reserves> {
        let def = def?;
        if !enabled_for(context)
            || u.owner.is_none()
            || !u.owner_exists
            || u.guard_home
            || def.building
            || def.structure
            || def.air
            || def.naval
            || (def.speed.is_nan() || def.speed <= 0.0)
            || def.logistics_truck
            || def.rail
            || u.kind == "supplytruck"
        {
            return None;
        }
        self.units.entry(u.id).or_insert_with(|| {
            let armed = def.weapon.filter(|w| {
                w.range > 0.0
                    && w.interval > 0.0
                    && (w.infantry_damage > 0.0 || w.vehicle_damage > 0.0)
            });
            let ammo_max = armed.map_or(0.0, |w| {
                let volley = if w.salvo {
                    math::max(1.0, rounds(w.rockets))
                } else if w.per_model {
                    math::max(1.0, rounds(def.models))
                } else {
                    1.0
                };
                (RESERVE_RULES.ammo_seconds / w.interval).ceil() * volley
            });
            Reserves {
                ammo: ammo_max,
                ammo_max,
                ammo_price: ammo_price(Some(def)),
                ammo_credit: Some(0.0),
                provisions: RESERVE_RULES.provisions,
                fuel: (!def.infantry).then_some(RESERVE_RULES.fuel),
                emergency: (!def.infantry).then_some(RESERVE_RULES.emergency),
                warning: None,
                forced: false,
                stranded: false,
                exhausted: false,
                supply_rate: None,
                cut_for: None,
            }
        });
        self.units.get(&u.id)
    }
    pub fn consume_ammo(&mut self, id: UnitId, count: f64) -> f64 {
        let requested = rounds(count);
        let Some(r) = self.units.get_mut(&id) else {
            return requested;
        };
        let launched = math::min(requested, rounds(r.ammo));
        r.ammo -= launched;
        launched
    }
    pub fn can_fire(&self, id: UnitId, count: f64) -> bool {
        let requested = rounds(count);
        requested > 0.0 && self.units.get(&id).is_none_or(|r| r.ammo >= requested)
    }
    pub fn recovered(&self, id: UnitId) -> bool {
        self.units.get(&id).is_none_or(recovered)
    }
    pub fn tick_reserves(
        &mut self,
        context: Context,
        u: UnitFacts<'_>,
        dt: f64,
        travel_seconds: f64,
    ) {
        if !enabled_for(context) {
            return;
        }
        let Some(r) = self.units.get_mut(&u.id) else {
            return;
        };
        let seconds = positive(dt);
        release_recovered(r);
        let before = r.provisions;
        r.provisions = math::max(0.0, before - seconds);
        if r.provisions == 0.0 && !r.exhausted {
            r.exhausted = true;
            r.warning = Some(math::max(
                0.0,
                RESERVE_RULES.warning - math::max(0.0, seconds - before),
            ));
        } else if r.exhausted
            && let Some(warning) = r.warning
        {
            r.warning = Some(math::max(0.0, warning - seconds));
        }
        if r.warning == Some(0.0) {
            r.forced = true;
        }
        driving(r, u.riding, math::min(seconds, positive(travel_seconds)));
    }
    /// Travel time is powered motion supplied by the movement solver; riding uses none.
    pub fn consume_driving_fuel(&mut self, u: UnitFacts<'_>, travel_seconds: f64) {
        if let Some(r) = self.units.get_mut(&u.id) {
            driving(r, u.riding, travel_seconds);
        }
    }
    pub fn shortage_state(&self, id: UnitId) -> Shortage {
        self.units
            .get(&id)
            .map_or(Shortage::default(), |r| Shortage {
                low_ammo: r.ammo_max > 0.0 && r.ammo < r.ammo_max * RESERVE_RULES.low,
                low_provisions: r.provisions < RESERVE_RULES.provisions * RESERVE_RULES.low,
                exhausted: r.exhausted,
                forced: r.forced,
                stranded: r.stranded,
            })
    }
    pub fn shortage_rate(&self, id: UnitId) -> f64 {
        if self.units.get(&id).is_some_and(|r| {
            r.exhausted || (r.ammo_max > 0.0 && r.ammo < r.ammo_max * RESERVE_RULES.low)
        }) {
            2.0
        } else {
            1.0
        }
    }
    pub fn recovery_rate(&self, id: UnitId) -> f64 {
        match self.units.get(&id) {
            None => 1.0,
            Some(r) if r.exhausted || r.provisions <= 0.0 => 0.0,
            Some(r) if r.provisions < RESERVE_RULES.provisions * RESERVE_RULES.low => 0.5,
            _ => 1.0,
        }
    }
    /// Ammo is transferred in credits and issued as whole rounds. Each commodity has its own rate.
    pub fn transfer_stock(&mut self, id: UnitId, store: Option<&mut Stock>, seconds: f64) -> Stock {
        let mut moved = Stock::default();
        let fraction = positive(seconds) / RESERVE_RULES.unload_seconds;
        let (Some(r), Some(store)) = (self.units.get_mut(&id), store) else {
            return moved;
        };
        if fraction == 0.0 {
            return moved;
        }
        if r.ammo_max > 0.0 {
            let price = r.ammo_price;
            let pending = r.ammo_credit.unwrap_or(0.0);
            moved.ammo = math::min(
                math::min(
                    positive(store.ammo),
                    math::max(0.0, (r.ammo_max - r.ammo) * price - pending),
                ),
                r.ammo_max * price * fraction,
            );
            let credit = pending + moved.ammo;
            let loaded = math::min(r.ammo_max - r.ammo, ((credit + EPS) / price).floor());
            r.ammo += loaded;
            r.ammo_credit = Some(math::max(0.0, credit - loaded * price));
        }
        moved.provisions = math::min(
            math::min(
                positive(store.provisions),
                math::max(0.0, RESERVE_RULES.provisions - r.provisions),
            ),
            RESERVE_RULES.provisions * fraction,
        );
        r.provisions += moved.provisions;
        if let Some(fuel) = r.fuel {
            let emergency = r.emergency.unwrap_or(0.0);
            moved.fuel = math::min(
                math::min(
                    positive(store.fuel),
                    math::max(0.0, RESERVE_RULES.fuel - fuel)
                        + math::max(0.0, RESERVE_RULES.emergency - emergency),
                ),
                (RESERVE_RULES.fuel + RESERVE_RULES.emergency) * fraction,
            );
            let ordinary = math::min(moved.fuel, RESERVE_RULES.fuel - fuel);
            r.fuel = Some(fuel + ordinary);
            r.emergency = Some(emergency + (moved.fuel - ordinary));
        }
        if moved.ammo > 0.0 {
            store.ammo = math::max(0.0, store.ammo - moved.ammo);
        }
        if moved.provisions > 0.0 {
            store.provisions = math::max(0.0, store.provisions - moved.provisions);
        }
        if moved.fuel > 0.0 {
            store.fuel = math::max(0.0, store.fuel - moved.fuel);
        }
        release_recovered(r);
        moved
    }
}
fn recovered(r: &Reserves) -> bool {
    r.provisions >= RESERVE_RULES.provisions * RESERVE_RULES.recovery
        && (r.ammo_max == 0.0 || r.ammo >= r.ammo_max * RESERVE_RULES.recovery)
        && r.fuel
            .is_none_or(|f| f >= RESERVE_RULES.fuel * RESERVE_RULES.recovery)
}
fn release_recovered(r: &mut Reserves) {
    if recovered(r) {
        r.warning = None;
        r.exhausted = false;
        r.forced = false;
        r.stranded = false;
    }
}
fn driving(r: &mut Reserves, riding: bool, seconds: f64) {
    if let Some(fuel) = r.fuel {
        let mut time = if riding { 0.0 } else { positive(seconds) };
        let ordinary = math::min(fuel, time);
        r.fuel = Some(fuel - ordinary);
        time -= ordinary;
        if r.fuel.is_some_and(|v| v <= 0.0) {
            r.fuel = Some(0.0);
            r.forced = true;
            r.emergency = Some(math::max(0.0, r.emergency.unwrap_or(0.0) - time));
        }
        if r.fuel == Some(0.0) && r.emergency == Some(0.0) {
            r.stranded = true;
        }
    }
}
