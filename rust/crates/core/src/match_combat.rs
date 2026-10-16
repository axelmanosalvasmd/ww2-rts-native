//! Canonical combat metadata and ordered damage/death application for the Match runner.
//! Health, suppression, position, paths and garrison remain in World.game.
use crate::{combat::DeathImpulse, grid::UnitId};
use std::collections::BTreeMap;

/// Metadata absent from Game's movement/query state. No health or position is stored here.
#[derive(Clone, Debug, PartialEq)]
pub struct UnitCombat {
    pub xp: f64,
    pub last_hit: Option<i32>,
    pub death_impulse: Option<DeathImpulse>,
    pub hit_by: Option<UnitId>,
    pub hit_at: f64,
    pub at_hit: Option<f64>,
    pub cargo: Vec<UnitId>,
    /// Carrier identity. Game.riding is kept in sync as its query flag.
    pub carrier: Option<UnitId>,
}
impl Default for UnitCombat {
    fn default() -> Self {
        Self {
            xp: 0.,
            last_hit: None,
            death_impulse: None,
            hit_by: None,
            hit_at: -1e9,
            at_hit: None,
            cargo: Vec::new(),
            carrier: None,
        }
    }
}
/// Dead shooter metadata can remain while a projectile still refers to its source XP.
/// The firing producer explicitly releases retained IDs after its last flight expires.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CombatState {
    units: BTreeMap<UnitId, UnitCombat>,
}
impl CombatState {
    pub fn initialize(&mut self, id: UnitId) -> &mut UnitCombat {
        self.units.entry(id).or_default()
    }
    pub fn get(&self, id: UnitId) -> Option<&UnitCombat> {
        self.units.get(&id)
    }
    pub fn get_mut(&mut self, id: UnitId) -> Option<&mut UnitCombat> {
        self.units.get_mut(&id)
    }
    pub fn remove(&mut self, id: UnitId) -> Option<UnitCombat> {
        self.units.remove(&id)
    }
    pub fn iter(&self) -> impl Iterator<Item = (UnitId, &UnitCombat)> {
        self.units.iter().map(|(id, u)| (*id, u))
    }
}
/// The firing lane owns the timer. A reset is infallible after event preflight and must
/// initialize a missing timer just as the source assigns retarget on an existing unit.
pub trait RetargetSink {
    fn reset_retarget(&mut self, unit: UnitId);
}

use crate::{
    catalog::{Catalog, Value},
    combat::{
        self, CombatContext, CombatEvent, CombatSource, Impact, SuppressionInput, UnitCombatState,
    },
    economy::Mode,
    environment::{self, Blast, DamageContext, EnvironmentRuntime},
    game::{Point, TRENCH},
    grid::SpatialGrid,
    logistics::LogisticsState,
    math,
    navigation::{TerrainView, Versions},
    production::ProductionState,
    rules::Rules,
    unit_spawn::SpawnPlacer,
    world::World,
};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CombatError {
    InvalidInput(&'static str),
    Rules,
    Damage(combat::Error),
    Environment(environment::EnvironmentError),
}
impl std::fmt::Display for CombatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for CombatError {}
impl From<combat::Error> for CombatError {
    fn from(e: combat::Error) -> Self {
        Self::Damage(e)
    }
}
impl From<environment::EnvironmentError> for CombatError {
    fn from(e: environment::EnvironmentError) -> Self {
        Self::Environment(e)
    }
}
/// Immutable projectile source identity. Its XP stays in CombatState after unit removal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FlightSource {
    pub shooter: UnitId,
    pub kind: usize,
    pub owner: i32,
    pub airborne: bool,
}
/// Borrowed canonical fields. Destructure Match to construct this view beside the firing sink.
pub struct DamageWorld<'a> {
    pub world: &'a mut World,
    pub combat: &'a mut CombatState,
    pub grid: &'a mut SpatialGrid,
    pub rules: &'a Rules,
    pub logistics: &'a LogisticsState,
    pub production: &'a ProductionState,
    pub mode: Mode,
    pub tech_enabled: bool,
    pub environment: &'a mut EnvironmentRuntime,
    pub environment_seed: &'a mut u32,
    pub next_id: &'a mut u32,
    pub unit_order: &'a [u32],
    pub wind_angle: f64,
    pub wind_strength: f64,
}
fn definition(kind: usize, rules: &Rules) -> Result<&'static Value, CombatError> {
    let name = &rules
        .kinds
        .get(kind)
        .ok_or(CombatError::InvalidInput("kind"))?
        .name;
    Catalog::compiled()
        .map_err(|_| CombatError::Rules)?
        .unit(name)
        .map_err(|_| CombatError::Rules)
}
fn number(v: &Value, key: &str) -> Result<f64, CombatError> {
    v.get(key)
        .and_then(Value::number)
        .filter(|v| v.is_finite())
        .ok_or(CombatError::Rules)
}
fn flag(v: &Value, key: &str) -> bool {
    v.get(key).and_then(Value::boolean).unwrap_or(false)
}
impl DamageWorld<'_> {
    pub(crate) fn validate(&self) -> Result<(), CombatError> {
        let g = &self.world.game;
        if g.tick < 0
            || g.tick as u64 > 9_007_199_254_740_941
            || crate::match_actor::checked_game_mode(g, self.mode).is_err()
            || self.mode == Mode::World && self.world.geography().is_none()
        {
            return Err(CombatError::InvalidInput("combat mode/tick"));
        }
        if g.units.iter().any(|(id, u)| {
            u.id != *id
                || u.kind >= self.rules.kinds.len()
                || crate::match_actor::checked_game_owner(g, u.owner).is_err()
                || ![u.x, u.z, u.hp, u.supp, u.rot, u.sprint]
                    .iter()
                    .all(|v| v.is_finite())
                || u.supp < 0.
                || u.garrison.is_some_and(|c| c >= g.chars.len())
        }) {
            return Err(CombatError::InvalidInput("unit"));
        }
        let ids: BTreeSet<_> = self.unit_order.iter().copied().collect();
        if ids.len() != self.unit_order.len()
            || ids.len() != g.units.len()
            || ids.iter().any(|id| !g.units.contains_key(id))
        {
            return Err(CombatError::InvalidInput("unit order"));
        }
        for (&id, u) in &g.units {
            let m = self
                .combat
                .get(id)
                .ok_or(CombatError::InvalidInput("unit combat metadata"))?;
            combat::retreat_protected(combat::RetreatState {
                retreating: u.retreating,
                forced: self.logistics.get(id).is_some_and(|r| r.forced),
                stranded: self.logistics.get(id).is_some_and(|r| r.stranded),
                move_speed: u.move_speed,
                vx: u.vx.unwrap_or(0.),
                vz: u.vz.unwrap_or(0.),
            })?;
            if !m.xp.is_finite()
                || m.xp < 0.
                || !m.hit_at.is_finite()
                || m.at_hit.is_some_and(|v| !v.is_finite())
                || m.carrier.is_some() != u.riding
                || m.carrier == Some(0)
            {
                return Err(CombatError::InvalidInput("combat metadata"));
            }
        }
        Ok(())
    }
    fn source(&self, source: FlightSource) -> Result<(), CombatError> {
        if source.kind >= self.rules.kinds.len()
            || source.owner < -1
            || source.owner >= 0 && source.owner as usize >= self.world.game.players.len()
        {
            return Err(CombatError::InvalidInput("source identity"));
        }
        let m = self
            .combat
            .get(source.shooter)
            .ok_or(CombatError::InvalidInput("source metadata"))?;
        if !m.xp.is_finite() || m.xp < 0. {
            return Err(CombatError::InvalidInput("source XP"));
        }
        Ok(())
    }
    fn environment_preflight(&mut self) -> Result<(), CombatError> {
        let ctx = environment::Context {
            tick: self.world.game.tick as u64,
            seed: self.environment_seed,
            next_id: self.next_id,
            unit_order: self.unit_order,
            grid: self.grid,
            rain: self.world.game.rain,
            wet: self.world.game.wet.unwrap_or(0.),
            wind_angle: self.wind_angle,
            wind_strength: self.wind_strength,
        };
        self.environment.validate_context(self.world, &ctx)?;
        Ok(())
    }
    fn alliance(&self, owner: i32, target: usize) -> bool {
        if owner == -1 && self.mode == Mode::World {
            Some(crate::game::LOCAL_DEFENDER_OWNER)
        } else {
            usize::try_from(owner).ok()
        }
        .is_some_and(|owner| self.world.game.allied(owner, target))
    }
    fn context(&self, owner: i32, target: usize) -> CombatContext {
        CombatContext {
            tick: self.world.game.tick as f64,
            mode: self.mode,
            horde_defenders: 0,
            allied: self.alliance(owner, target),
            tech_enabled: self.tech_enabled,
            attacker_tech: usize::try_from(owner)
                .ok()
                .and_then(|o| self.production.player(o).map(|p| p.tech())),
            defender_tech: self.production.player(target).map(|p| p.tech()),
        }
    }
    fn target(&self, id: UnitId) -> Result<UnitCombatState<'_>, CombatError> {
        let g = &self.world.game;
        let u = g
            .units
            .get(&id)
            .ok_or(CombatError::InvalidInput("target"))?;
        let m = self
            .combat
            .get(id)
            .ok_or(CombatError::InvalidInput("target metadata"))?;
        let reserves = self.logistics.get(id);
        Ok(UnitCombatState {
            id,
            owner: crate::match_actor::checked_game_owner(g, u.owner)
                .map_err(|_| CombatError::InvalidInput("target actor"))?,
            kind: &self.rules.kinds[u.kind].name,
            hp: u.hp,
            xp: m.xp,
            at: u.at(),
            rotation: u.rot,
            retreat: combat::RetreatState {
                retreating: u.retreating,
                forced: reserves.is_some_and(|r| r.forced),
                stranded: reserves.is_some_and(|r| r.stranded),
                move_speed: u.move_speed,
                vx: u.vx.unwrap_or(0.),
                vz: u.vz.unwrap_or(0.),
            },
            garrisoned: u.garrison.is_some(),
            in_trench: self.rules.kinds[u.kind].infantry
                && g.cell_of(u.x, u.z)
                    .is_some_and(|c| g.flags[c] & TRENCH != 0),
            // The source timer can expire below zero; only its positive state affects combat.
            sprint: math::max(0., u.sprint),
            suppression: u.supp,
            hit_by: m.hit_by,
            cargo: &m.cargo,
        })
    }
    /// Apply a complete component event batch after resolving every fallible rider placement.
    pub fn apply_events(
        &mut self,
        events: &[CombatEvent],
        retarget: &mut dyn RetargetSink,
        suppressed: &mut BTreeSet<UnitId>,
    ) -> Result<(), CombatError> {
        self.validate()?;
        let mut rider_positions = BTreeMap::new();
        let mut placer = SpawnPlacer::new();
        for event in events {
            let (id, finite) = match event {
                CombatEvent::Health { unit, value } => (*unit, value.is_finite()),
                CombatEvent::Suppression { unit, value } => {
                    (*unit, value.is_finite() && *value >= 0.)
                }
                CombatEvent::LastHit { unit, owner } => (
                    *unit,
                    *owner == -1
                        || *owner >= 0 && (*owner as usize) < self.world.game.players.len(),
                ),
                CombatEvent::ResetRetarget { unit } | CombatEvent::SuppressionRecord { unit } => {
                    (*unit, true)
                }
                CombatEvent::DeathImpulse { unit, value } => (
                    *unit,
                    value.direction.is_finite() && value.magnitude.is_finite(),
                ),
                CombatEvent::HitSource {
                    unit, tick, from, ..
                } => (
                    *unit,
                    tick.is_finite() && from.x.is_finite() && from.z.is_finite(),
                ),
                CombatEvent::AntiTankHit { unit, tick } => (*unit, tick.is_finite()),
                CombatEvent::Experience { shooter, value } => {
                    if !value.is_finite() || *value < 0. || self.combat.get(*shooter).is_none() {
                        return Err(CombatError::InvalidInput("source experience"));
                    }
                    continue;
                }
                CombatEvent::DisembarkRider {
                    carrier,
                    rider,
                    minimum_suppression,
                } => {
                    if !minimum_suppression.is_finite()
                        || *minimum_suppression < 0.
                        || !self.world.game.units.contains_key(carrier)
                    {
                        return Err(CombatError::InvalidInput("rider event"));
                    }
                    if let Some(u) = self.world.game.units.get(rider) {
                        if self.combat.get(*rider).is_none() {
                            return Err(CombatError::InvalidInput("rider metadata"));
                        }
                        let g = &self.world.game;
                        let c = placer
                            .nearest_free(
                                TerrainView {
                                    width: g.w,
                                    height: g.h,
                                    flags: &g.flags,
                                    heights: g.height.as_deref(),
                                    world_known: false,
                                    knowledge_id: 0,
                                    versions: Versions::default(),
                                    changed: None,
                                    changes: &[],
                                },
                                u.at(),
                                self.rules.kinds[u.kind].block(),
                                None,
                            )
                            .map_err(|_| CombatError::InvalidInput("rider placement"))?;
                        rider_positions.insert(
                            *rider,
                            Point {
                                x: (c % g.w) as f64 * 2. + 1.,
                                z: (c / g.w) as f64 * 2. + 1.,
                            },
                        );
                    }
                    continue;
                }
            };
            if !finite || !self.world.game.units.contains_key(&id) || self.combat.get(id).is_none()
            {
                return Err(CombatError::InvalidInput("combat event"));
            }
        }
        for event in events {
            match *event {
                CombatEvent::Health { unit, value } => {
                    self.world.game.units.get_mut(&unit).unwrap().hp = value
                }
                CombatEvent::DisembarkRider {
                    carrier,
                    rider,
                    minimum_suppression,
                } => {
                    if let Some(&at) = rider_positions.get(&rider) {
                        self.combat
                            .get_mut(carrier)
                            .unwrap()
                            .cargo
                            .retain(|id| *id != rider);
                        self.combat.get_mut(rider).unwrap().carrier = None;
                        let u = self.world.game.units.get_mut(&rider).unwrap();
                        u.riding = false;
                        u.x = at.x;
                        u.z = at.z;
                        u.supp = math::max(u.supp, minimum_suppression);
                        self.grid.update(rider, u);
                    }
                }
                CombatEvent::LastHit { unit, owner } => {
                    self.combat.get_mut(unit).unwrap().last_hit = Some(owner)
                }
                CombatEvent::DeathImpulse { unit, value } => {
                    self.combat.get_mut(unit).unwrap().death_impulse = Some(value)
                }
                CombatEvent::ResetRetarget { unit } => retarget.reset_retarget(unit),
                CombatEvent::HitSource {
                    unit,
                    shooter,
                    tick,
                    from,
                } => {
                    let m = self.combat.get_mut(unit).unwrap();
                    m.hit_by = Some(shooter);
                    m.hit_at = tick;
                    self.world.game.units.get_mut(&unit).unwrap().hit_from = Some(from);
                }
                CombatEvent::AntiTankHit { unit, tick } => {
                    self.combat.get_mut(unit).unwrap().at_hit = Some(tick)
                }
                CombatEvent::Experience { shooter, value } => {
                    self.combat.get_mut(shooter).unwrap().xp = value
                }
                CombatEvent::SuppressionRecord { unit } => {
                    suppressed.insert(unit);
                }
                CombatEvent::Suppression { unit, value } => {
                    self.world.game.units.get_mut(&unit).unwrap().supp = value
                }
            }
        }
        Ok(())
    }
    pub fn direct_hit(
        &mut self,
        source: FlightSource,
        target: UnitId,
        impact: Impact,
        retarget: &mut dyn RetargetSink,
    ) -> Result<f64, CombatError> {
        self.validate()?;
        self.source(source)?;
        if source.kind >= self.rules.kinds.len()
            || source.shooter != impact.shooter
            || source.owner != impact.owner
        {
            return Err(CombatError::InvalidInput("flight identity"));
        }
        let xp = self
            .combat
            .get(source.shooter)
            .ok_or(CombatError::InvalidInput("flight source metadata"))?
            .xp;
        let t = self.target(target)?;
        let result = combat::flight_damage(
            self.context(source.owner, self.world.game.units[&target].owner),
            impact,
            CombatSource {
                kind: &self.rules.kinds[source.kind].name,
                xp,
                airborne: source.airborne,
            },
            &t,
        )?;
        self.apply_events(&result.events, retarget, &mut BTreeSet::new())?;
        Ok(result.damage)
    }
    /// Cover values are sampled by the firing/contact producer from canonical terrain.
    pub fn suppress(
        &mut self,
        target: UnitId,
        mut input: SuppressionInput,
        retarget: &mut dyn RetargetSink,
        suppressed: &mut BTreeSet<UnitId>,
    ) -> Result<(), CombatError> {
        self.validate()?;
        input.already_suppressed = suppressed.contains(&target);
        let t = self.target(target)?;
        let events = combat::flight_suppress(
            self.context(input.owner, self.world.game.units[&target].owner),
            input,
            &t,
        )?;
        self.apply_events(&events, retarget, suppressed)
    }
    /// Apply environment LastHit to the same canonical combat owner history; return shot effects.
    pub fn environment_effects(
        &mut self,
        effects: Vec<environment::Effect>,
    ) -> Vec<environment::Effect> {
        effects
            .into_iter()
            .filter_map(|effect| match effect {
                environment::Effect::LastHit { unit, owner } => {
                    self.combat.initialize(unit).last_hit = Some(owner);
                    None
                }
                other => Some(other),
            })
            .collect()
    }
    pub fn terrain_blast(
        &mut self,
        blast: Blast,
        hit: &mut DamageContext<'_>,
    ) -> Result<Vec<environment::Effect>, CombatError> {
        let mut effects = Vec::new();
        self.terrain_blast_emitting(blast, hit, &mut |effect| effects.push(effect))?;
        Ok(effects)
    }
    /// Emit committed terrain effects even if a later mutation fails.
    pub fn terrain_blast_emitting(
        &mut self,
        blast: Blast,
        hit: &mut DamageContext<'_>,
        emit: &mut dyn FnMut(environment::Effect),
    ) -> Result<(), CombatError> {
        self.validate()?;
        let mut ctx = environment::Context {
            tick: self.world.game.tick as u64,
            seed: self.environment_seed,
            next_id: self.next_id,
            unit_order: self.unit_order,
            grid: self.grid,
            rain: self.world.game.rain,
            wet: self.world.game.wet.unwrap_or(0.),
            wind_angle: self.wind_angle,
            wind_strength: self.wind_strength,
        };
        let combat = &mut self.combat;
        let result = self.environment.damage_cells_emitting(
            self.world,
            blast,
            &mut ctx,
            hit,
            &mut |effect| absorb_environment_effect(combat, effect, emit),
        );
        self.environment.reconcile_wreck_metadata(self.world);
        result?;
        Ok(())
    }
    pub fn section_hit(
        &mut self,
        cell: usize,
        damage: f64,
        hit: &DamageContext<'_>,
    ) -> Result<(f64, Vec<environment::Effect>), CombatError> {
        let mut effects = Vec::new();
        let taken =
            self.section_hit_emitting(cell, damage, hit, &mut |effect| effects.push(effect))?;
        Ok((taken, effects))
    }
    pub fn section_hit_emitting(
        &mut self,
        cell: usize,
        damage: f64,
        hit: &DamageContext<'_>,
        emit: &mut dyn FnMut(environment::Effect),
    ) -> Result<f64, CombatError> {
        self.validate()?;
        let mut ctx = environment::Context {
            tick: self.world.game.tick as u64,
            seed: self.environment_seed,
            next_id: self.next_id,
            unit_order: self.unit_order,
            grid: self.grid,
            rain: self.world.game.rain,
            wet: self.world.game.wet.unwrap_or(0.),
            wind_angle: self.wind_angle,
            wind_strength: self.wind_strength,
        };
        let combat = &mut self.combat;
        Ok(self.environment.damage_section_emitting(
            self.world,
            cell,
            damage,
            &mut ctx,
            hit,
            &mut |effect| absorb_environment_effect(combat, effect, emit),
        )?)
    }
}

pub(crate) fn absorb_environment_effect(
    combat: &mut CombatState,
    effect: environment::Effect,
    emit: &mut dyn FnMut(environment::Effect),
) {
    match effect {
        environment::Effect::LastHit { unit, owner } => {
            combat.initialize(unit).last_hit = Some(owner);
        }
        other => emit(other),
    }
}

/// Source explosion values from a catalog weapon/support entry. No dynamic action JSON is accepted.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ExplosionSource {
    pub infantry: f64,
    pub vehicle: f64,
    pub terrain: Option<f64>,
    pub suppression: f64,
    pub scar: f64,
    pub anti_garrison: Option<f64>,
    pub weapon_kind: Option<usize>,
    pub off_map_support: bool,
}
pub(crate) struct PreparedHurt {
    pub events: Vec<CombatEvent>,
    pub hurt: environment::Hurt,
}
impl ExplosionSource {
    pub fn weapon(kind: usize, rules: &Rules) -> Result<Self, CombatError> {
        let v = definition(kind, rules)?
            .get("w")
            .ok_or(CombatError::InvalidInput("weapon"))?;
        Self::from_catalog(v, Some(kind), false)
    }
    pub fn support(name: &str) -> Result<Self, CombatError> {
        let v = Catalog::compiled()
            .map_err(|_| CombatError::Rules)?
            .table("SUPPORT")
            .map_err(|_| CombatError::Rules)?
            .get(name)
            .ok_or(CombatError::InvalidInput("support"))?;
        Self::from_catalog(v, None, true)
    }
    fn from_catalog(
        v: &Value,
        weapon_kind: Option<usize>,
        off_map_support: bool,
    ) -> Result<Self, CombatError> {
        Ok(Self {
            infantry: number(v, "inf")?,
            vehicle: number(v, "veh")?,
            terrain: v.get("terrain").and_then(Value::number),
            suppression: number(v, "supp")?,
            scar: v.get("scar").and_then(Value::number).unwrap_or(0.),
            anti_garrison: v.get("antiGarrison").and_then(Value::number),
            weapon_kind,
            off_map_support,
        })
    }
    fn checked(self) -> Result<(), CombatError> {
        if ![
            self.infantry,
            self.vehicle,
            self.suppression,
            self.scar,
            self.terrain.unwrap_or(0.),
            self.anti_garrison.unwrap_or(1.),
        ]
        .iter()
        .all(|v| v.is_finite() && *v >= 0. && *v <= 100_000.)
        {
            Err(CombatError::InvalidInput("explosion profile"))
        } else {
            Ok(())
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContactTarget {
    Unit(UnitId),
    Cell(usize),
}
impl DamageWorld<'_> {
    /// Resolve the original local-contact damage. Flight motion/selection remains with firing.
    #[allow(clippy::too_many_arguments)]
    pub fn local_contact(
        &mut self,
        source: FlightSource,
        target: ContactTarget,
        impact: Impact,
        contact_id: &str,
        blast_source: Option<ExplosionSource>,
        retarget: &mut dyn RetargetSink,
    ) -> Result<(f64, Vec<environment::Effect>), CombatError> {
        let mut effects = Vec::new();
        let taken = self.local_contact_emitting(
            source,
            target,
            impact,
            contact_id,
            blast_source,
            retarget,
            &mut |effect| effects.push(effect),
        )?;
        Ok((taken, effects))
    }
    #[allow(clippy::too_many_arguments)]
    pub fn local_contact_emitting(
        &mut self,
        source: FlightSource,
        target: ContactTarget,
        impact: Impact,
        contact_id: &str,
        blast_source: Option<ExplosionSource>,
        retarget: &mut dyn RetargetSink,
        emit: &mut dyn FnMut(environment::Effect),
    ) -> Result<f64, CombatError> {
        self.validate()?;
        self.source(source)?;
        if source.owner != impact.owner
            || source.shooter != impact.shooter
            || ![
                impact.energy,
                impact.damage_scale.unwrap_or(1.),
                impact.direction.x,
                impact.direction.z,
            ]
            .iter()
            .all(|v| v.is_finite())
            || impact.energy < 0.
            || impact.damage_scale.is_some_and(|v| v < 0.)
        {
            return Err(CombatError::InvalidInput("contact impact"));
        }

        self.environment_preflight()?;
        if let ContactTarget::Unit(id) = target {
            return Ok(if blast_source.is_some() {
                0.
            } else {
                self.direct_hit(source, id, impact, retarget)?
            });
        }
        let ContactTarget::Cell(c) = target else {
            unreachable!()
        };
        if c >= self.world.game.chars.len() {
            return Err(CombatError::InvalidInput("contact cell"));
        }
        let meta = self.world.sections.get(&c);
        let entity = meta
            .and_then(|m| {
                self.world
                    .structures
                    .iter()
                    .find(|s| s.id == m.structure_id)
                    .and_then(|s| s.entity_id)
            })
            .filter(|id| self.world.game.units.contains_key(id));
        if let Some(id) = entity
            && blast_source.is_none()
            && self.alliance(source.owner, self.world.game.units[&id].owner)
        {
            return Ok(0.);
        }
        if entity.is_none() && self.world.game.cell_hp[c] <= 0. {
            return Ok(0.);
        }
        let cfg = Catalog::compiled()
            .map_err(|_| CombatError::Rules)?
            .table("CFG")
            .map_err(|_| CombatError::Rules)?;
        let weapon = definition(source.kind, self.rules)?
            .get("w")
            .ok_or(CombatError::InvalidInput("weapon"))?;
        let damage = if let Some(id) = entity.filter(|_| blast_source.is_none()) {
            let u = &self.world.game.units[&id];
            let xp = self.combat.get(id).unwrap().xp;
            let def = definition(u.kind, self.rules)?;
            let small = impact.weapon.vehicle < 20.;
            let base = if !small {
                impact.weapon.vehicle
            } else {
                impact.weapon.infantry
                    * number(cfg.get("classic").ok_or(CombatError::Rules)?, "smallArms")?
            };
            let slit = if let Some(s) = def.get("slit") {
                (if small { number(s, "smallArms")? } else { 1. })
                    * if math::cos(u.rot) * impact.direction.x
                        + math::sin(u.rot) * impact.direction.z
                        > 0.
                    {
                        number(s, "rear")?
                    } else {
                        1.
                    }
            } else {
                1.
            };
            base * impact.damage_scale.unwrap_or(1.)
                * impact.energy
                * (1.
                    - number(cfg, "vetArmor")?
                        * f64::from(combat::veterancy(&self.rules.kinds[u.kind].name, xp)?))
                * slit
        } else {
            blast_source
                .and_then(|p| p.terrain)
                .or_else(|| weapon.get("shellTerrain").and_then(Value::number))
                .or_else(|| weapon.get("veh").and_then(Value::number))
                .unwrap_or(0.)
                * impact.energy
                * 0.25
        };
        if !damage.is_finite() || damage < 0. {
            return Err(CombatError::InvalidInput("contact damage"));
        }
        if meta.is_some() {
            let before = entity.map(|id| self.world.game.units[&id].hp);
            let skip = BTreeSet::new();
            let mut hit = DamageContext::plain(&skip);
            hit.owner = Some(source.owner);
            hit.contact_id = Some(contact_id);
            hit.direction = Some(impact.direction);
            let taken = self.section_hit_emitting(c, damage, &hit, emit)?;
            if let Some(id) = entity
                && let Some(before) = before.filter(|v| *v > 0.)
                && !self.alliance(source.owner, self.world.game.units[&id].owner)
            {
                let after = self.world.game.units[&id].hp;
                let cost = number(
                    definition(self.world.game.units[&id].kind, self.rules)?,
                    "cost",
                )?;
                self.combat
                    .get_mut(source.shooter)
                    .ok_or(CombatError::InvalidInput("source metadata"))?
                    .xp += math::max(0., math::min(before, before - after))
                    + if after <= 0. { cost * 0.2 } else { 0. };
            }
            Ok(taken)
        } else {
            let hp = f64::from(self.world.game.cell_hp[c]);
            let at = Point {
                x: (c % self.world.game.w) as f64 * 2. + 1.,
                z: (c / self.world.game.w) as f64 * 2. + 1.,
            };
            let skip = BTreeSet::new();
            let mut hit = DamageContext::plain(&skip);
            hit.owner = Some(source.owner);
            hit.contact_id = Some(contact_id);
            hit.direction = Some(impact.direction);
            if damage > 0. {
                self.terrain_blast_emitting(
                    Blast {
                        at,
                        radius: 0.,
                        damage,
                        scar: 0.,
                    },
                    &mut hit,
                    emit,
                )?;
            }
            Ok(math::max(0., hp - f64::from(self.world.game.cell_hp[c])))
        }
    }
    /// Apply source generic hurt without projectile armor, XP or radial impulse.
    /// The returned Hurt event describes contact before carried units disembark.
    pub fn hurt(
        &mut self,
        id: UnitId,
        profile: ExplosionSource,
        fall: f64,
        owner: i32,
        retarget: &mut dyn RetargetSink,
    ) -> Result<environment::Hurt, CombatError> {
        let prepared = self.prepare_hurt(id, profile, fall, owner)?;
        self.apply_events(&prepared.events, retarget, &mut BTreeSet::new())?;
        Ok(prepared.hurt)
    }
    pub(crate) fn prepare_hurt(
        &self,
        id: UnitId,
        profile: ExplosionSource,
        fall: f64,
        owner: i32,
    ) -> Result<PreparedHurt, CombatError> {
        self.validate()?;
        profile.checked()?;
        if !fall.is_finite()
            || !(0.0..=1.0).contains(&fall)
            || owner < -1
            || owner >= 0 && owner as usize >= self.world.game.players.len()
            || profile
                .weapon_kind
                .is_some_and(|k| k >= self.rules.kinds.len())
        {
            return Err(CombatError::InvalidInput("hurt source/falloff"));
        }
        let u = self
            .world
            .game
            .units
            .get(&id)
            .ok_or(CombatError::InvalidInput("hurt target"))?;
        let def = definition(u.kind, self.rules)?;
        let carrier = self
            .combat
            .get(id)
            .ok_or(CombatError::InvalidInput("hurt metadata"))?
            .carrier;
        let cfg = Catalog::compiled()
            .map_err(|_| CombatError::Rules)?
            .table("CFG")
            .map_err(|_| CombatError::Rules)?;
        let target = self.target(id)?;
        let context = self.context(owner, u.owner);
        let base = if flag(def, "structure") {
            profile.terrain.unwrap_or(profile.vehicle)
        } else if flag(def, "infantry") {
            profile.infantry
        } else {
            profile.vehicle
        };
        let shrug = if self.rules.kinds[u.kind].name == "bunker" && profile.off_map_support {
            number(cfg.get("assault").ok_or(CombatError::Rules)?, "supportMul")?
        } else {
            1.
        };
        let shrug = shrug
            * if self.mode == Mode::Annihilation
                && self.rules.kinds[u.kind].name == "bunker"
                && profile
                    .weapon_kind
                    .is_some_and(|kind| self.rules.kinds[kind].name == "howitzer")
            {
                number(cfg.get("assault").ok_or(CombatError::Rules)?, "howitzerMul")?
            } else {
                1.0
            };
        let cover = if u.garrison.is_some() {
            profile
                .anti_garrison
                .unwrap_or(number(cfg, "trenchBlastMul")?)
        } else if target.in_trench {
            number(cfg, "trenchBlastMul")?
        } else {
            1.
        };
        let vet = combat::veterancy(target.kind, target.xp)?;
        let tech = if self.tech_enabled {
            crate::economy::tech_mul(
                context.attacker_tech,
                profile
                    .weapon_kind
                    .map(|k| self.rules.kinds[k].name.as_str()),
                context.defender_tech,
                target.kind,
            )
            .map_err(|_| CombatError::Rules)?
        } else {
            1.
        };
        let damage = base
            * shrug
            * fall
            * if combat::retreat_protected(target.retreat)? {
                number(cfg, "retreatDamage")?
            } else {
                1.
            }
            * cover
            * (1. - number(cfg, "vetArmor")? * f64::from(vet))
            * tech;
        let mut events = Vec::new();
        if owner >= 0 {
            events.push(CombatEvent::LastHit { unit: id, owner });
        }
        events.push(CombatEvent::Health {
            unit: id,
            value: target.hp - damage,
        });
        if flag(def, "infantry") {
            events.push(CombatEvent::Suppression {
                unit: id,
                value: math::min(
                    100.,
                    target.suppression
                        + profile.suppression * (1. - number(cfg, "vetSupp")? * f64::from(vet)),
                ),
            });
        }
        let jump = carrier.unwrap_or(id);
        if self
            .world
            .game
            .units
            .get(&jump)
            .is_some_and(|c| definition(c.kind, self.rules).is_ok_and(|d| flag(d, "riders")))
            && owner >= 0
            && !self.alliance(owner, self.world.game.units[&jump].owner)
        {
            for &rider in &self.combat.get(jump).unwrap().cargo {
                events.push(CombatEvent::DisembarkRider {
                    carrier: jump,
                    rider,
                    minimum_suppression: number(cfg, "riderShock")?,
                });
            }
        }
        let hurt = environment::Hurt {
            unit: id,
            owner: u.owner,
            at: u.at(),
            killed: target.hp - damage <= 0.0,
            from: Some(owner),
            local: false,
        };
        Ok(PreparedHurt { events, hurt })
    }

    /// Terminal blast damages the supplied source-order live unit collection, then terrain.
    #[allow(clippy::too_many_arguments)]
    pub fn explosion(
        &mut self,
        at: Point,
        radius: f64,
        profile: ExplosionSource,
        owner: i32,
        contact_id: Option<&str>,
        direction: Option<Point>,
        memory: Option<&mut environment::DamageMemory>,
        retarget: &mut dyn RetargetSink,
    ) -> Result<Vec<environment::Effect>, CombatError> {
        self.explosion_membership(
            at,
            radius,
            profile,
            owner,
            contact_id,
            direction,
            memory,
            self.unit_order,
            retarget,
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub fn explosion_emitting(
        &mut self,
        at: Point,
        radius: f64,
        profile: ExplosionSource,
        owner: i32,
        contact_id: Option<&str>,
        direction: Option<Point>,
        memory: Option<&mut environment::DamageMemory>,
        retarget: &mut dyn RetargetSink,
        emit: &mut dyn FnMut(environment::Effect),
    ) -> Result<(), CombatError> {
        self.explosion_membership_emitting(
            at,
            radius,
            profile,
            owner,
            contact_id,
            direction,
            memory,
            self.unit_order,
            retarget,
            emit,
        )
    }
    /// Apply direct unit and terrain-list consequences to captured current-unit membership.
    /// Metadata, grid and environment validation still use the complete canonical unit order.
    #[allow(clippy::too_many_arguments)]
    pub fn explosion_membership(
        &mut self,
        at: Point,
        radius: f64,
        profile: ExplosionSource,
        owner: i32,
        contact_id: Option<&str>,
        direction: Option<Point>,
        memory: Option<&mut environment::DamageMemory>,
        actor_order: &[UnitId],
        retarget: &mut dyn RetargetSink,
    ) -> Result<Vec<environment::Effect>, CombatError> {
        let mut effects = Vec::new();
        self.explosion_membership_emitting(
            at,
            radius,
            profile,
            owner,
            contact_id,
            direction,
            memory,
            actor_order,
            retarget,
            &mut |effect| effects.push(effect),
        )?;
        Ok(effects)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn explosion_membership_emitting(
        &mut self,
        at: Point,
        radius: f64,
        profile: ExplosionSource,
        owner: i32,
        contact_id: Option<&str>,
        direction: Option<Point>,
        memory: Option<&mut environment::DamageMemory>,
        actor_order: &[UnitId],
        retarget: &mut dyn RetargetSink,
        emit: &mut dyn FnMut(environment::Effect),
    ) -> Result<(), CombatError> {
        self.validate()?;
        self.environment_preflight()?;
        let actors: BTreeSet<_> = actor_order.iter().copied().collect();
        if actors.len() != actor_order.len()
            || actor_order
                .iter()
                .any(|id| !self.world.game.units.contains_key(id))
        {
            return Err(CombatError::InvalidInput("blast actor membership"));
        }
        profile.checked()?;
        if direction.is_some_and(|d| !d.x.is_finite() || !d.z.is_finite()) {
            return Err(CombatError::InvalidInput("blast direction"));
        }
        for u in self.world.game.units.values() {
            if u.kind >= self.rules.kinds.len() || !u.rot.is_finite() {
                return Err(CombatError::InvalidInput("blast unit pose"));
            }
        }

        if profile
            .weapon_kind
            .is_some_and(|k| k >= self.rules.kinds.len())
            || owner < -1
            || owner >= 0 && owner as usize >= self.world.game.players.len()
        {
            return Err(CombatError::InvalidInput("blast identity"));
        }
        if ![at.x, at.z, radius].iter().all(|v| v.is_finite()) || radius <= 0. || radius > 4096. {
            return Err(CombatError::InvalidInput("blast geometry"));
        }
        let mut skip = BTreeSet::new();
        let ids = actor_order.to_vec();
        for id in ids {
            let u = &self.world.game.units[&id];
            let distance = math::hypot(u.x - at.x, u.z - at.z);
            let def = definition(u.kind, self.rules)?;
            let carrier = self.combat.get(id).unwrap().carrier;
            let on_deck = carrier
                .and_then(|c| self.world.game.units.get(&c))
                .is_some_and(|c| definition(c.kind, self.rules).is_ok_and(|d| flag(d, "riders")));
            if distance > radius || u.hp <= 0. || u.air.is_some() || u.riding && !on_deck {
                continue;
            }
            let fall = 1. - distance / radius * 0.5;
            if flag(def, "building")
                && self
                    .world
                    .structures
                    .iter()
                    .any(|s| s.entity_id == Some(id))
            {
                if profile.terrain.is_some_and(|d| d != 0.) {
                    continue;
                }
                let structure = self
                    .world
                    .structures
                    .iter()
                    .find(|s| s.entity_id == Some(id))
                    .unwrap();
                let mut cells = structure
                    .cells
                    .iter()
                    .zip(&structure.section_ids)
                    .filter_map(|(c, id)| {
                        self.world
                            .section_by_id(id)
                            .filter(|(_, s)| !s.failed)
                            .map(|_| *c)
                    })
                    .collect::<Vec<_>>();
                cells.sort_by(|a, b| {
                    let pos = |c: usize| Point {
                        x: (c % self.world.game.w) as f64 * 2. + 1.,
                        z: (c / self.world.game.w) as f64 * 2. + 1.,
                    };
                    let a = pos(*a);
                    let b = pos(*b);
                    math::hypot(a.x - at.x, a.z - at.z)
                        .total_cmp(&math::hypot(b.x - at.x, b.z - at.z))
                });
                if let Some(&c) = cells.first() {
                    let skip_cells = BTreeSet::new();
                    let mut hit = DamageContext::plain(&skip_cells);
                    hit.actor_order = Some(actor_order);
                    hit.owner = Some(owner);
                    hit.contact_id = contact_id;
                    hit.direction = Some(Point {
                        x: u.x - at.x,
                        z: u.z - at.z,
                    });
                    self.section_hit_emitting(
                        c,
                        profile.terrain.unwrap_or(profile.vehicle) * fall,
                        &hit,
                        emit,
                    )?;
                }
                skip.insert(id);
                continue;
            }
            let mut hurt = self.hurt(id, profile, fall, owner, retarget)?;
            hurt.local = contact_id.is_some_and(|id| !id.is_empty());
            emit(environment::Effect::Hurt(hurt));
            let u = &self.world.game.units[&id];
            let impulse = CombatEvent::DeathImpulse {
                unit: id,
                value: DeathImpulse {
                    direction: math::atan2(u.z - at.z, u.x - at.x),
                    magnitude: math::max(0.25, math::min(4., profile.vehicle / 100.)),
                },
            };
            self.apply_events(&[impulse], retarget, &mut BTreeSet::new())?;
        }
        if let Some(damage) = profile.terrain.filter(|d| *d != 0.) {
            let mut hit = DamageContext::plain(&skip);
            hit.actor_order = Some(actor_order);
            hit.owner = Some(owner);
            hit.contact_id = contact_id;
            hit.direction = direction;
            hit.memory = memory;
            self.terrain_blast_emitting(
                Blast {
                    at,
                    radius,
                    damage,
                    scar: profile.scar,
                },
                &mut hit,
                emit,
            )?;
        }
        Ok(())
    }
}

/// Deferred story/convoy notification after canonical removal and bounty payment.
/// The caller uses this source snapshot; it must not look the deleted unit up again.
#[derive(Clone, Debug, PartialEq)]
pub struct RetiredUnit {
    pub unit: crate::game::Unit,
    pub last_hit: Option<i32>,
    pub enemy_killer: Option<usize>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DeathResult {
    pub retired: Vec<RetiredUnit>,
    pub shots: Vec<environment::Effect>,
}
/// Ordered removal facts remain available if wreck or lifecycle work fails.
/// Counts distinguish completed generic retirements from completed caller callbacks.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RetirementProgress {
    pub removed: Vec<RetiredUnit>,
    pub completed: usize,
    pub companions_notified: usize,
    pub stories_notified: usize,
    pub routes_retained: usize,
}
impl crate::match_runtime::Match {
    /// Validate every captured potential removal, including living garrisons
    /// that an earlier building retirement can kill in this same ordered pass.
    fn validate_retirement_sidecars(&self, ids: &[UnitId]) -> Result<(), CombatError> {
        for &id in ids {
            let unit = self
                .world
                .game
                .units
                .get(&id)
                .ok_or(CombatError::InvalidInput("death sidecar unit"))?;
            let control = self
                .commands
                .units
                .get(&id)
                .ok_or(CombatError::InvalidInput("death construction control"))?;
            crate::match_construction::validate_build_activity(unit, control)
                .map_err(|_| CombatError::InvalidInput("death construction activity"))?;
            if unit.id != id
                || self.abilities.get(id).is_none()
                || self.recovery.get(id).is_none()
                || unit.dig != self.field_works.is_active(id)
                || unit.entrench != self.field_works.membership(id).is_some()
            {
                return Err(CombatError::InvalidInput("death sidecar state"));
            }
        }
        Ok(())
    }

    /// Drop Unit-owned sidecars at the actual removal boundary. Physical facts
    /// and retained combat metadata stay available to the remaining phases.
    fn retire_unit_sidecars(&mut self, unit: &mut crate::game::Unit) -> Result<(), CombatError> {
        if self.abilities.get(unit.id).is_none()
            || self.recovery.get(unit.id).is_none()
            || unit.dig != self.field_works.is_active(unit.id)
        {
            return Err(CombatError::InvalidInput("death sidecar state"));
        }
        let control = self
            .commands
            .units
            .get_mut(&unit.id)
            .ok_or(CombatError::InvalidInput("death construction control"))?;
        crate::match_runtime::cancel_owned_work_state(&mut self.field_works, unit, control)
            .map_err(|_| CombatError::InvalidInput("death owned work"))?;
        self.abilities.remove(unit.id);
        self.recovery.remove(unit.id);
        Ok(())
    }

    /// Read-only admission for the ordinary death phase at the caller's prospective clock.
    /// Seed and ID copies allow the whole tick to reject before advancing canonical state.
    pub fn validate_dead_retirement(
        &self,
        unit_order: &[UnitId],
        prospective_tick: u64,
    ) -> Result<(), CombatError> {
        if self.world.game.tick < 0 || self.validate_actor_mode().is_err() {
            return Err(CombatError::InvalidInput("death mode/tick"));
        }
        let mut seen = BTreeSet::new();
        if unit_order.iter().any(|id| !seen.insert(*id)) {
            return Err(CombatError::InvalidInput("death order"));
        }
        let ordered = unit_order
            .iter()
            .copied()
            .filter(|id| self.world.game.units.contains_key(id))
            .collect::<Vec<_>>();
        self.validate_retirement_sidecars(&ordered)?;
        let cfg = Catalog::compiled()
            .map_err(|_| CombatError::Rules)?
            .table("CFG")
            .map_err(|_| CombatError::Rules)?;
        let bounty = number(cfg, "bounty")?;
        for id in &ordered {
            let u = &self.world.game.units[id];
            if self.actor_policy(u.owner).is_err() || u.kind >= self.rules.kinds.len() {
                return Err(CombatError::InvalidInput("unsupported dead unit"));
            }
            let m = self
                .combat
                .get(u.id)
                .ok_or(CombatError::InvalidInput("death metadata"))?;
            if !u.hp.is_finite()
                || m.last_hit.is_some_and(|owner| {
                    owner < -1 || owner >= 0 && owner as usize >= self.start.economy.players.len()
                })
            {
                return Err(CombatError::InvalidInput("death attribution"));
            }
            if let Some(impulse) = m.death_impulse
                && (!impulse.direction.is_finite() || !impulse.magnitude.is_finite())
            {
                return Err(CombatError::InvalidInput("death impulse"));
            }
        }
        if self
            .world
            .game
            .units
            .values()
            .any(|u| u.air.is_some() || self.rules.kinds.get(u.kind).is_some_and(|kind| kind.air))
            || !self.aviation.planes.is_empty()
            || !self.aviation.anti_air.is_empty()
        {
            self.validate_aviation()
                .map_err(|_| CombatError::InvalidInput("aircraft retirement registry"))?;
        }
        let mut wallets = self
            .start
            .economy
            .players
            .iter()
            .map(|p| p.mp)
            .collect::<Vec<_>>();
        if wallets.iter().any(|v| !v.is_finite()) {
            return Err(CombatError::InvalidInput("bounty wallet"));
        }
        for id in &ordered {
            let u = &self.world.game.units[id];
            let killer = self
                .combat
                .get(u.id)
                .unwrap()
                .last_hit
                .and_then(|o| usize::try_from(o).ok())
                .filter(|o| {
                    !self.world.game.allied(*o, u.owner) && !self.start.economy.players[*o].out
                });
            if let Some(k) = killer {
                wallets[k] += number(definition(u.kind, &self.rules)?, "cost")? * bounty;
                if !wallets[k].is_finite() {
                    return Err(CombatError::InvalidInput("bounty overflow"));
                }
            }
        }
        self.world
            .mutation_capacity(self.world.game.chars.len() as u64 * 64 + 4096)
            .map_err(environment::EnvironmentError::from)?;
        let all = self.world.game.units.keys().copied().collect::<Vec<_>>();
        let mut seed = self.start.environment_seed;
        let mut next_id = self.next_unit_id;
        let ctx = environment::Context {
            tick: prospective_tick,
            seed: &mut seed,
            next_id: &mut next_id,
            unit_order: &all,
            grid: &self.grid,
            rain: self.world.game.rain,
            wet: self.world.game.wet.unwrap_or(0.),
            wind_angle: self.start.wind.0,
            wind_strength: self.start.wind.1,
        };
        let facts = ordered
            .iter()
            .map(|id| &self.world.game.units[id])
            .collect::<Vec<_>>();
        self.environment
            .validate_retirement(&self.world, &facts, &ctx)?;
        Ok(())
    }

    /// Finish represented ground and registered aircraft deaths in captured source order.
    /// Story/fallen, firing/control retirement and convoy hooks remain with their owners.
    /// Combat and firing metadata stay retained while projectile sources may refer to them.
    pub fn retire_dead(&mut self, unit_order: &[UnitId]) -> Result<DeathResult, CombatError> {
        let mut progress = RetirementProgress::default();
        let mut shots = Vec::new();
        self.retire_dead_emitting(unit_order, &mut progress, &mut |effect| shots.push(effect))?;
        Ok(DeathResult {
            retired: progress.removed,
            shots,
        })
    }
    pub fn retire_dead_emitting(
        &mut self,
        unit_order: &[UnitId],
        progress: &mut RetirementProgress,
        emit: &mut dyn FnMut(environment::Effect),
    ) -> Result<(), CombatError> {
        if !progress.removed.is_empty()
            || progress.completed != 0
            || progress.companions_notified != 0
            || progress.stories_notified != 0
            || progress.routes_retained != 0
        {
            return Err(CombatError::InvalidInput(
                "retirement progress already used",
            ));
        }
        self.validate_dead_retirement(unit_order, self.world.game.tick as u64)?;
        let ordered = unit_order
            .iter()
            .copied()
            .filter(|id| self.world.game.units.contains_key(id))
            .collect::<Vec<_>>();
        let bounty = number(
            Catalog::compiled()
                .map_err(|_| CombatError::Rules)?
                .table("CFG")
                .map_err(|_| CombatError::Rules)?,
            "bounty",
        )?;
        for id in ordered {
            let Some(mut u) = self
                .world
                .game
                .units
                .get(&id)
                .filter(|u| u.hp <= 0.)
                .cloned()
            else {
                continue;
            };
            let metadata = self.combat.get(u.id).unwrap().clone();
            self.retire_unit_sidecars(&mut u)?;
            self.world.game.units.remove(&u.id);
            self.grid.remove_unit(u.id);
            let enemy = metadata
                .last_hit
                .and_then(|owner| usize::try_from(owner).ok())
                .filter(|owner| !self.world.game.allied(*owner, u.owner));
            progress.removed.push(RetiredUnit {
                unit: u,
                last_hit: metadata.last_hit,
                enemy_killer: enemy,
            });
            let u = &progress.removed.last().unwrap().unit;
            let current = self.world.game.units.keys().copied().collect::<Vec<_>>();
            let mut ctx = environment::Context {
                tick: self.world.game.tick as u64,
                seed: &mut self.start.environment_seed,
                next_id: &mut self.next_unit_id,
                unit_order: &current,
                grid: &self.grid,
                rain: self.world.game.rain,
                wet: self.world.game.wet.unwrap_or(0.),
                wind_angle: self.start.wind.0,
                wind_strength: self.start.wind.1,
            };
            let def = definition(u.kind, &self.rules)?;
            let combat = &mut self.combat;
            let mut absorb = |effect| absorb_environment_effect(combat, effect, emit);
            if u.air.is_some() {
                // The source excludes AIR from hull wreck creation. Its AA event already exists.
            } else if flag(def, "building") {
                self.environment.wreck_building_emitting(
                    &mut self.world,
                    u,
                    &mut ctx,
                    &mut absorb,
                )?;
            } else {
                self.environment.spawn_vehicle_wreck_emitting(
                    &mut self.world,
                    u,
                    metadata.death_impulse,
                    &mut ctx,
                    &mut absorb,
                )?;
            }
            self.aviation.remove(u.id);
            self.logistics.remove(u.id);
            self.production.remove_building(u.id);
            self.building_rallies.remove(&u.id);
            // Source buildingCells retains historical ruin ownership/position after retirement.

            if let Some(killer) = enemy.filter(|k| !self.start.economy.players[*k].out) {
                self.start.economy.players[killer].mp += number(def, "cost")? * bounty;
            }
            progress.completed += 1;
        }
        Ok(())
    }
}
