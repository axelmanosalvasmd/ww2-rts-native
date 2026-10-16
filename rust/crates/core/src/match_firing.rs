//! Canonical weapon state for the authoritative Match firing phase.
//! Unit ownership, health and position remain in Game; command aim and intent
//! remain in UnitControl, and hit history and experience remain in CombatState.

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    catalog::{Catalog, Value},
    game::{CELL, COVER, Point, Unit},
    grid::UnitId,
    match_combat::{ExplosionSource, RetargetSink},
    match_runtime::Match,
    math, projectiles, vision,
};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FiringUnit {
    /// Chosen target, with zero meaning no target.
    pub target_id: UnitId,
    pub cooldown: f64,
    pub retarget: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FiringState {
    pub units: BTreeMap<UnitId, FiringUnit>,
    pub flights: BTreeMap<u64, LiveFlight>,
    pub next_flight: u64,
    pub suppressed: BTreeMap<u64, BTreeSet<UnitId>>,
    pub events: Vec<LaunchEvent>,
    pub salvos: Vec<LiveSalvo>,
    pub grenades: Vec<GrenadeTicket>,
    claims: BTreeMap<(UnitId, i32), f64>,
    prior: BTreeMap<UnitId, crate::game::Pose>,
    prior_tick: Option<i64>,
}

impl Default for FiringState {
    fn default() -> Self {
        Self {
            units: BTreeMap::new(),
            flights: BTreeMap::new(),
            next_flight: 1,
            suppressed: BTreeMap::new(),
            claims: BTreeMap::new(),
            prior: BTreeMap::new(),
            prior_tick: None,
            events: Vec::new(),
            salvos: Vec::new(),
            grenades: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LiveSalvo {
    pub at: Point,
    pub owner: usize,
    pub left: u32,
    pub next: f64,
    pub every: f64,
    pub spread: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum LaunchEvent {
    Flight {
        id: u64,
        kind: usize,
        effect: Option<&'static str>,
        shooter: UnitId,
        owner: usize,
        at: projectiles::Point3,
        velocity: projectiles::Point3,
        gravity: f64,
        time: f64,
        expires: f64,
    },
    Salvo {
        shooter: UnitId,
        owner: usize,
        at: Point,
        count: u32,
        spread: f64,
    },
    Throw {
        shooter: UnitId,
        owner: usize,
        at: Point,
    },
    SmallArms(SmallArmsEvent),
    Environment(crate::environment::Effect),
    Contact(ContactEvent),
    Expiry {
        id: u64,
        sequence: u32,
        time: f64,
        at: projectiles::Point3,
    },
}

/// Existing ordinary shot fields used by the operative producer.
#[derive(Clone, Debug, PartialEq)]
pub struct SmallArmsEvent {
    pub effect: &'static str,
    pub shooter: UnitId,
    pub owner: usize,
    pub at: projectiles::Point3,
    pub hit: Option<SmallArmsHit>,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SmallArmsHit {
    pub target: UnitId,
    pub owner: usize,
    pub man: Option<u32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ContactEvent {
    pub kind: usize,
    pub flight: u64,
    pub sequence: u32,
    pub contact_id: String,
    pub time: f64,
    pub at: projectiles::Point3,
    pub shooter: UnitId,
    pub owner: usize,
    pub target: Option<UnitId>,
    pub target_owner: Option<usize>,
    pub hit: bool,
    pub killed: bool,
    pub terminal: bool,
    pub explosive: bool,
    pub effect: Option<&'static str>,
    pub blast: Option<f64>,
    pub normal: projectiles::Point3,
    pub direction: projectiles::Point3,
    pub impulse: f64,
    pub material: &'static str,
}
// JSON.stringify renders both zero signs as 0; keep trajectory math untouched.
pub(crate) fn normalize_json_zero(value: &mut serde_json::Value) {
    use serde_json::Value;
    match value {
        Value::Number(n) if n.as_f64() == Some(0.0) => *value = Value::from(0),
        Value::Array(a) => {
            for v in a {
                normalize_json_zero(v);
            }
        }
        Value::Object(o) => {
            for v in o.values_mut() {
                normalize_json_zero(v);
            }
        }
        _ => {}
    }
}
impl LaunchEvent {
    /// Internal reference-event projection for oracle/producer use, not a new network contract.
    pub fn to_json(&self, rules: &crate::rules::Rules) -> serde_json::Value {
        use serde_json::json;
        let mut value = match self {
            Self::Flight {
                id,
                kind,
                effect,
                shooter,
                owner,
                at,
                velocity,
                gravity,
                time,
                expires,
            } => {
                let mut v = json!({"k":"flight","kind":rules.kinds[*kind].name,"flight":id,"seq":0,"f":shooter,"fo":crate::game::source_owner_json(*owner),"x":at.x,"y":at.y,"z":at.z,"v":[velocity.x,velocity.y,velocity.z],"gravity":gravity,"time":time,"expires":expires,"local":true});
                if let Some(effect) = effect {
                    v["effect"] = json!(effect);
                }
                v
            }
            Self::Salvo {
                shooter,
                owner,
                at,
                count,
                spread,
            } => {
                json!({"f":shooter,"fo":crate::game::source_owner_json(*owner),"x":at.x,"z":at.z,"k":"salvo","n":count,"r":spread,"physical":true,"local":true})
            }
            Self::Throw { shooter, owner, at } => {
                json!({"f":shooter,"fo":crate::game::source_owner_json(*owner),"x":at.x,"z":at.z,"k":"throw","physical":true,"local":true})
            }
            Self::SmallArms(e) => {
                let mut value = json!({"k":e.effect,"f":e.shooter,"fo":crate::game::source_owner_json(e.owner),"x":e.at.x,"y":e.at.y,"z":e.at.z});
                if let Some(hit) = e.hit {
                    value["t"] = json!(hit.target);
                    value["to"] = crate::game::source_owner_json(hit.owner);
                    value["hit"] = json!(true);
                    if let Some(man) = hit.man {
                        value["m"] = json!(man);
                    }
                }
                value
            }
            Self::Environment(e) => e.to_json(),
            Self::Expiry {
                id,
                sequence,
                time,
                at,
            } => {
                json!({"k":"expiry","flight":id,"seq":sequence,"time":time,"x":at.x,"y":at.y,"z":at.z,"local":true})
            }
            Self::Contact(e) => {
                let mut v = json!({"k":"contact","kind":rules.kinds[e.kind].name,"flight":e.flight,"seq":e.sequence,"contactId":e.contact_id,"time":e.time,"x":e.at.x,"y":e.at.y,"z":e.at.z,"f":e.shooter,"fo":crate::game::source_owner_json(e.owner),"hit":e.hit,"kill":e.killed,"terminal":e.terminal,"explosive":e.explosive,"normal":[e.normal.x,e.normal.y,e.normal.z],"direction":[e.direction.x,e.direction.y,e.direction.z],"impulse":e.impulse,"material":e.material,"local":true});
                if let Some(id) = e.target {
                    v["t"] = json!(id);
                }
                if let Some(owner) = e.target_owner {
                    v["to"] = crate::game::source_owner_json(owner);
                }
                if let Some(effect) = e.effect {
                    v["effect"] = json!(effect);
                }
                if let Some(blast) = e.blast {
                    v["blast"] = json!(blast);
                }
                v
            }
        };
        normalize_json_zero(&mut value);
        value
    }
    fn from_flight(p: &LiveFlight) -> Self {
        Self::Flight {
            id: p.id,
            kind: p.source_kind,
            effect: p.effect,
            shooter: p.shooter,
            owner: p.owner,
            at: p.solution.origin,
            velocity: p.solution.velocity,
            gravity: p.solution.gravity,
            time: p.launched,
            expires: p.expires,
        }
    }
}

/// A round retains its launch solution when its shooter or intended target moves.
/// Shooter XP and hit metadata stay in CombatState while this ID remains referenced.
#[derive(Clone, Debug, PartialEq)]
pub struct LiveFlight {
    pub id: u64,
    pub owner: usize,
    pub shooter: UnitId,
    pub source_kind: usize,
    pub solution: projectiles::Flight,
    pub launch_origin: projectiles::Point3,
    pub launched: f64,
    pub last: f64,
    pub expires: f64,
    pub sent: bool,
    pub energy: f64,
    pub sequence: u32,
    pub max_contacts: u32,
    pub near: f64,
    pub start_cell: usize,
    pub intended: Option<UnitId>,
    pub damage_scale: f64,
    pub armor_piercing: bool,
    pub ap_multiplier: f64,
    pub ap_naval: bool,
    pub suppression: f64,
    pub suppression_scale: f64,
    pub volley_group: u64,
    pub explosive: bool,
    pub terrain: f64,
    pub scar: f64,
    pub blast_radius: Option<f64>,
    pub blast_source: Option<ExplosionSource>,
    pub effect: Option<&'static str>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FlightSurface {
    Unit(UnitId),
    Cell(usize),
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FlightContact {
    pub hit: projectiles::ContactHit,
    pub surface: FlightSurface,
}

/// Terrain uses world-layers MATERIALS. Unit contacts use the projectile steel profile.
fn terrain_contact_material(name: &str) -> projectiles::ContactMaterial {
    let (name, hardness, absorption, rebound) = match name {
        "road" => ("road", 0.45, 0.65, 0.12),
        "wood" => ("wood", 0.3, 0.8, 0.05),
        "stone" => ("stone", 0.85, 0.3, 0.55),
        "concrete" => ("concrete", 0.95, 0.25, 0.65),
        "steel" => ("steel", 1.0, 0.15, 0.75),
        "water" => ("water", 0.0, 1.0, 0.0),
        _ => ("soil", 0.12, 0.9, 0.02),
    };
    projectiles::ContactMaterial {
        name,
        hardness,
        absorption,
        rebound,
    }
}

impl RetargetSink for FiringState {
    fn reset_retarget(&mut self, unit: UnitId) {
        self.initialize(unit).retarget = 0.0;
    }
}

fn n(v: &Value, key: &str, fallback: f64) -> f64 {
    v.get(key).and_then(Value::number).unwrap_or(fallback)
}
fn flag(v: &Value, key: &str) -> bool {
    v.get(key).is_some_and(|v| match v {
        Value::Bool(b) => *b,
        Value::Number(n) => *n != 0.0 && !n.is_nan(),
        _ => false,
    })
}
fn dist(a: Point, b: Point) -> f64 {
    math::hypot(a.x - b.x, a.z - b.z)
}

impl Match {
    fn supported_firing_mode(&self) -> Result<(), String> {
        if !matches!(
            (self.options.mode, self.world.game.mode),
            (
                crate::economy::Mode::Default | crate::economy::Mode::Conquest,
                None
            ) | (
                crate::economy::Mode::World,
                Some(crate::game::ModeKind::World)
            )
        ) {
            return Err("weapon execution requires ordinary Conquest mode".into());
        }
        self.validate_actor_mode()
    }
    /// Capture poses before the tick advances and before weather or environment movement.
    pub fn capture_firing_prior(&mut self) -> Result<(), String> {
        self.supported_firing_mode()?;
        if !(0..9_007_199_254_740_351).contains(&self.world.game.tick) {
            return Err("invalid prior-pose tick".into());
        }
        self.validate_firing_units()?;
        self.firing.prior = self
            .world
            .game
            .units
            .iter()
            .map(|(&id, u)| (id, u.pose()))
            .collect();
        self.firing.prior_tick = Some(self.world.game.tick + 1);
        Ok(())
    }
    /// Validate aircraft through their canonical sortie owner before ground or air firing.
    pub(crate) fn validate_aircraft_firing_context(&self) -> Result<(), String> {
        if self
            .world
            .game
            .units
            .values()
            .any(|u| u.air.is_some() || self.rules.kinds.get(u.kind).is_some_and(|kind| kind.air))
        {
            self.validate_aviation()
                .map_err(|error| error.to_string())?;
            if self.world.game.units.values().any(|u| {
                u.air.is_some() && (u.x.abs() > 1_073_741_824.0 || u.z.abs() > 1_073_741_824.0)
            }) {
                return Err("aircraft firing context exceeds environment position bounds".into());
            }
        }
        Ok(())
    }
    fn validate_firing_units(&self) -> Result<(), String> {
        self.validate_aircraft_firing_context()?;
        for (&id, u) in &self.world.game.units {
            if u.id != id
                || u.kind >= self.rules.kinds.len()
                || crate::match_actor::checked_game_owner(&self.world.game, u.owner).is_err()
                || ![u.x, u.z, u.hp, u.rot, u.supp]
                    .iter()
                    .all(|n| n.is_finite())
                || (self.world.game.cell_of(u.x, u.z).is_none()
                    && !(u.air.is_some() && self.rules.kinds[u.kind].air))
                || u.cells
                    .as_ref()
                    .is_some_and(|c| c.iter().any(|&c| c as usize >= self.world.game.flags.len()))
                || u.garrison.is_some_and(|c| c >= self.world.game.flags.len())
                || self
                    .firing
                    .get(u.id)
                    .is_some_and(|s| !s.cooldown.is_finite() || !s.retarget.is_finite())
            {
                return Err("invalid canonical firing unit".into());
            }
        }
        Ok(())
    }
    fn firing_damage_view<'a>(
        &'a mut self,
        unit_order: &'a [UnitId],
    ) -> (crate::match_combat::DamageWorld<'a>, &'a mut FiringState) {
        let (wind_angle, wind_strength) = self.start.wind;
        (
            crate::match_combat::DamageWorld {
                world: &mut self.world,
                combat: &mut self.combat,
                grid: &mut self.grid,
                rules: &self.rules,
                logistics: &self.logistics,
                production: &self.production,
                mode: self.options.mode,
                tech_enabled: self.options.tech,
                environment: &mut self.environment,
                environment_seed: &mut self.start.environment_seed,
                next_id: &mut self.next_unit_id,
                unit_order,
                wind_angle,
                wind_strength,
            },
            &mut self.firing,
        )
    }
    fn suppress_flight_segment(
        &mut self,
        p: &LiveFlight,
        a: projectiles::Point3,
        b: projectiles::Point3,
    ) -> Result<(), String> {
        if p.near == 0.0 || p.suppression == 0.0 {
            return Ok(());
        }
        let g = &self.world.game;
        let center = ((a.x + b.x) / 2.0, (a.z + b.z) / 2.0);
        let reach = math::hypot(b.x - a.x, b.z - a.z) / 2.0 + p.near;
        let candidates = self.grid.in_order(&g.units, center, reach, |_, _| true);
        let from = Point {
            x: p.launch_origin.x,
            z: p.launch_origin.z,
        };
        let definition = Catalog::compiled()
            .map_err(|e| e.to_string())?
            .unit(&self.rules.kinds[p.source_kind].name)
            .map_err(|e| e.to_string())?;
        let flame = definition.get("w").is_some_and(|w| flag(w, "flame"));
        let unit_order: Vec<_> = g.units.keys().copied().collect();
        let mut suppressed = self
            .firing
            .suppressed
            .remove(&p.volley_group)
            .unwrap_or_default();
        let result = (|| {
            for target in candidates {
                let t = &self.world.game.units[&target];
                if suppressed.contains(&target)
                    || t.hp <= 0.0
                    || !self.rules.kinds[t.kind].infantry
                    || self.world.game.allied(p.owner, t.owner)
                    || t.retreating
                    || t.sprint > 0.0
                {
                    continue;
                }
                let distance = projectiles::segment_distance(
                    projectiles::BodyPosition { x: a.x, z: a.z },
                    projectiles::BodyPosition { x: b.x, z: b.z },
                    projectiles::BodyPosition { x: t.x, z: t.z },
                )
                .map_err(|e| format!("flight suppression distance: {e:?}"))?;
                if distance > p.near {
                    continue;
                }
                let input = crate::combat::SuppressionInput {
                    owner: p.owner as i32,
                    shooter: p.shooter,
                    amount: p.suppression,
                    scale: Some(p.suppression_scale),
                    flame,
                    launch_origin: from,
                    already_suppressed: suppressed.contains(&target),
                    cover_mul: self.cover_mul(t, from)?,
                    cover_behind: self.cover_behind(t, from)?,
                };
                let (mut damage, retarget) = self.firing_damage_view(&unit_order);
                damage
                    .suppress(target, input, retarget, &mut suppressed)
                    .map_err(|e| e.to_string())?;
            }
            Ok(())
        })();
        self.firing.suppressed.insert(p.volley_group, suppressed);
        result
    }
    fn entity_at_cell(&self, cell: usize) -> Option<UnitId> {
        let meta = self.world.sections.get(&cell)?;
        self.world
            .structures
            .iter()
            .find(|s| s.id == meta.structure_id)?
            .entity_id
            .filter(|id| self.world.game.units.contains_key(id))
    }
    fn blast_experience(
        &mut self,
        shooter: UnitId,
        owner: usize,
        before: &[(UnitId, f64)],
    ) -> Result<(), String> {
        for &(id, hp) in before {
            let Some(t) = self.world.game.units.get(&id) else {
                continue;
            };
            if hp <= 0.0 || self.world.game.allied(owner, t.owner) {
                continue;
            }
            let cost = n(self.definition(t)?, "cost", 0.0);
            let amount = math::max(0.0, math::min(hp, hp - t.hp))
                + if t.hp <= 0.0 { cost * 0.2 } else { 0.0 };
            self.combat.initialize(shooter).xp += amount;
        }
        Ok(())
    }
    fn resolve_live_flight(
        &mut self,
        p: &mut LiveFlight,
        contact: Option<FlightContact>,
        at: projectiles::Point3,
        time: f64,
        velocity: projectiles::Point3,
    ) -> Result<bool, String> {
        use crate::{
            combat::{Impact, WeaponDamage},
            match_combat::{ContactTarget, FlightSource},
        };
        p.sequence = p
            .sequence
            .checked_add(1)
            .ok_or("flight contact sequence exhausted")?;
        let contact_id = format!("{}:{}", p.id, p.sequence);
        let direction = Point {
            x: velocity.x,
            z: velocity.z,
        };
        let normal = contact.map_or(projectiles::Point3::new(0.0, 1.0, 0.0), |c| c.hit.normal);
        let material = if let Some(FlightContact {
            surface: FlightSurface::Cell(cell),
            ..
        }) = contact
        {
            terrain_contact_material(self.world.material_name(cell, false).unwrap_or("soil"))
        } else {
            projectiles::STEEL
        };
        let bounce = if contact.is_some() {
            projectiles::contact_response(
                projectiles::ContactState {
                    explosive: p.explosive,
                    sequence: p.sequence,
                    max_contacts: p.max_contacts,
                    energy: p.energy,
                },
                velocity,
                normal,
                material,
            )
            .map_err(|e| format!("flight contact response: {e:?}"))?
        } else {
            None
        };
        let kind = &self.rules.kinds[p.source_kind].name;
        let weapon = WeaponDamage::from_catalog(kind).map_err(|e| format!("{e:?}"))?;
        let source = FlightSource {
            shooter: p.shooter,
            kind: p.source_kind,
            owner: p.owner as i32,
            airborne: self.rules.kinds[p.source_kind].air,
        };
        let impact = Impact {
            owner: p.owner as i32,
            shooter: p.shooter,
            weapon,
            damage_scale: Some(p.damage_scale),
            energy: p.energy,
            armor_piercing: p.armor_piercing,
            ap_naval_only: p.ap_naval,
            ap_multiplier: p.ap_multiplier,
            launch_origin: Point {
                x: p.launch_origin.x,
                z: p.launch_origin.z,
            },
            at: Point { x: at.x, z: at.z },
            direction,
        };
        let blast_source = p.blast_source;
        let unit_order: Vec<_> = self.world.game.units.keys().copied().collect();
        let mut damage_amount = 0.0;
        if let Some(contact) = contact {
            let target = match contact.surface {
                FlightSurface::Unit(id) => ContactTarget::Unit(id),
                FlightSurface::Cell(cell) => ContactTarget::Cell(cell),
            };
            let (mut damage, retarget) = self.firing_damage_view(&unit_order);
            let mut effects = Vec::new();
            let result = damage.local_contact_emitting(
                source,
                target,
                impact,
                &contact_id,
                blast_source,
                retarget,
                &mut |effect| effects.push(effect),
            );
            self.firing
                .events
                .extend(effects.into_iter().map(LaunchEvent::Environment));
            damage_amount = result.map_err(|e| e.to_string())?;
        }
        let target = contact.and_then(|c| {
            if let FlightSurface::Unit(id) = c.surface {
                Some(id)
            } else {
                None
            }
        });
        let target_unit = target.and_then(|id| self.world.game.units.get(&id));
        let impulse_base = if p.terrain != 0.0 {
            p.terrain
        } else if weapon.vehicle != 0.0 {
            weapon.vehicle
        } else {
            weapon.infantry
        };
        self.firing.events.push(LaunchEvent::Contact(ContactEvent {
            kind: p.source_kind,
            flight: p.id,
            sequence: p.sequence,
            contact_id: contact_id.clone(),
            time,
            at,
            shooter: p.shooter,
            owner: p.owner,
            target,
            target_owner: target_unit.map(|t| t.owner),
            hit: damage_amount > 0.0,
            killed: target_unit.is_some_and(|t| t.hp <= 0.0),
            terminal: bounce.is_none(),
            explosive: bounce.is_none() && p.explosive,
            effect: p.effect,
            blast: p.blast_radius,
            normal,
            direction: velocity,
            impulse: math::max(0.25, math::min(4.0, p.energy * impulse_base / 100.0)),
            material: material.name,
        }));
        if let Some(bounce) = bounce {
            p.energy = bounce.energy;
            p.solution.velocity = bounce.velocity;
            p.solution.origin = projectiles::Point3::new(
                at.x + normal.x * 0.005,
                at.y + normal.y * 0.005,
                at.z + normal.z * 0.005,
            );
            p.solution.stamp = time;
            p.last = time;
            return Ok(false);
        }
        // The driver removes this round before local stepping. Never insert it during terminal effects.
        let blast_id = format!("{contact_id}:blast");
        if let Some(profile) = blast_source {
            let before: Vec<_> = self
                .world
                .game
                .units
                .values()
                .map(|t| (t.id, math::max(0.0, t.hp)))
                .collect();
            let (mut damage, retarget) = self.firing_damage_view(&unit_order);
            let mut effects = Vec::new();
            let result = damage.explosion_emitting(
                Point { x: at.x, z: at.z },
                p.blast_radius.ok_or("missing blast radius")?,
                profile,
                p.owner as i32,
                Some(&blast_id),
                Some(direction),
                None,
                retarget,
                &mut |effect| effects.push(effect),
            );
            self.firing
                .events
                .extend(effects.into_iter().map(LaunchEvent::Environment));
            result.map_err(|e| e.to_string())?;
            self.blast_experience(p.shooter, p.owner, &before)?;
        } else if p.terrain != 0.0 {
            let g = &self.world.game;
            let cx = (at.x / CELL).floor() as isize;
            let cz = (at.z / CELL).floor() as isize;
            let mut ids = BTreeSet::new();
            let mut before = Vec::new();
            for z in cz - 1..=cz + 1 {
                for x in cx - 1..=cx + 1 {
                    if x < 0 || z < 0 || x >= g.w as isize || z >= g.h as isize {
                        continue;
                    }
                    if let Some(id) = self.entity_at_cell(z as usize * g.w + x as usize)
                        && ids.insert(id)
                    {
                        before.push((id, math::max(0.0, g.units[&id].hp)));
                    }
                }
            }
            let skip = BTreeSet::new();
            let mut context = crate::environment::DamageContext::plain(&skip);
            context.owner = Some(p.owner as i32);
            context.contact_id = Some(&blast_id);
            context.direction = Some(direction);
            let (mut damage, _) = self.firing_damage_view(&unit_order);
            let mut effects = Vec::new();
            let result = damage.terrain_blast_emitting(
                crate::environment::Blast {
                    at: Point { x: at.x, z: at.z },
                    radius: 1.5,
                    damage: p.terrain * p.energy,
                    scar: p.scar,
                },
                &mut context,
                &mut |effect| effects.push(effect),
            );
            self.firing
                .events
                .extend(effects.into_iter().map(LaunchEvent::Environment));
            result.map_err(|e| e.to_string())?;
            self.blast_experience(p.shooter, p.owner, &before)?;
        }
        Ok(true)
    }
    /// Read-only unit and active-flight admission shared by the caller and actuator.
    pub(crate) fn validate_firing_tick_state(&self) -> Result<(), String> {
        self.supported_firing_mode()?;
        if !(0..=9_007_199_254_740_351).contains(&self.world.game.tick) {
            return Err("invalid flight tick".into());
        }
        self.validate_firing_units()?;
        for (&id, p) in &self.firing.flights {
            if id != p.id
                || id == 0
                || id > 9_007_199_254_740_991
                || p.source_kind >= self.rules.kinds.len()
                || crate::match_actor::checked_game_owner(&self.world.game, p.owner).is_err()
                || self.combat.get(p.shooter).is_none()
                || p.start_cell >= self.world.game.flags.len()
                || p.max_contacts > 16
                || p.sequence > p.max_contacts
                || ![
                    p.solution.origin.x,
                    p.solution.origin.y,
                    p.solution.origin.z,
                    p.solution.velocity.x,
                    p.solution.velocity.y,
                    p.solution.velocity.z,
                    p.solution.gravity,
                    p.solution.stamp,
                    p.launch_origin.x,
                    p.launch_origin.y,
                    p.launch_origin.z,
                    p.launched,
                    p.last,
                    p.expires,
                    p.energy,
                    p.near,
                    p.damage_scale,
                    p.ap_multiplier,
                    p.suppression,
                    p.suppression_scale,
                    p.terrain,
                    p.scar,
                ]
                .iter()
                .all(|v| v.is_finite())
                || p.expires < p.launched
                || p.energy < 0.0
                || !(0.0..=1024.0).contains(&p.near)
                || p.damage_scale < 0.0
                || p.ap_multiplier < 0.0
                || p.suppression < 0.0
                || p.suppression_scale < 0.0
                || p.blast_radius
                    .is_some_and(|r| !r.is_finite() || r <= 0.0 || r > 4096.0)
                || p.blast_source.is_some() && p.blast_radius.is_none()
            {
                return Err("invalid live flight payload".into());
            }
        }
        for p in self.firing.flights.values() {
            if let Some(source) = p.blast_source {
                validate_blast(source)?;
                if source
                    .weapon_kind
                    .is_some_and(|k| k >= self.rules.kinds.len())
                {
                    return Err("invalid flight blast identity".into());
                }
            }
        }
        Ok(())
    }

    /// Advance every live round after the source unit loop and crowd separation.
    /// New rounds born at this tick have no traveled interval until the next tick.
    pub fn step_firing_flights(&mut self) -> Result<(), String> {
        self.validate_firing_tick_state()?;
        let now = self.world.game.tick as f64 * 0.05;
        let tick_start = now - 0.05;
        let ids: Vec<_> = self.firing.flights.keys().copied().collect();
        for id in ids {
            let mut p = self
                .firing
                .flights
                .remove(&id)
                .ok_or("missing live flight")?;
            if p.launched > now {
                self.firing.flights.insert(id, p);
                continue;
            }
            let result = (|| {
                if !p.sent {
                    p.sent = true;
                    self.firing.events.push(LaunchEvent::from_flight(&p));
                }
                let mut begin = math::max(p.last, tick_start);
                let end = math::min(now, p.expires);
                let mut terminal = false;
                for _ in 0..=p.max_contacts {
                    if begin >= end - 1e-9 {
                        break;
                    }
                    let a = projectiles::flight_at(p.solution, begin)
                        .map_err(|e| format!("flight position: {e:?}"))?;
                    let b = projectiles::flight_at(p.solution, end)
                        .map_err(|e| format!("flight position: {e:?}"))?;
                    let contact = self.flight_contact(
                        &p,
                        a,
                        b,
                        (begin - tick_start) / 0.05,
                        (end - tick_start) / 0.05,
                    )?;
                    let stop = begin + (end - begin) * contact.map_or(1.0, |c| c.hit.fraction);
                    let at = projectiles::flight_at(p.solution, stop)
                        .map_err(|e| format!("flight position: {e:?}"))?;
                    self.suppress_flight_segment(&p, a, at)?;
                    p.last = stop;
                    if contact.is_none() {
                        break;
                    }
                    let velocity = projectiles::velocity_at(p.solution, stop)
                        .map_err(|e| format!("flight velocity: {e:?}"))?;
                    if self.resolve_live_flight(&mut p, contact, at, stop, velocity)? {
                        terminal = true;
                        break;
                    }
                    begin = stop + 1e-7;
                }
                if !terminal && now >= p.expires {
                    let at = projectiles::flight_at(p.solution, p.expires)
                        .map_err(|e| format!("flight expiry position: {e:?}"))?;
                    if p.blast_source.is_some() {
                        let expires = p.expires;
                        let velocity = projectiles::velocity_at(p.solution, expires)
                            .map_err(|e| format!("flight expiry velocity: {e:?}"))?;
                        self.resolve_live_flight(&mut p, None, at, expires, velocity)?;
                    } else {
                        self.firing.events.push(LaunchEvent::Expiry {
                            id: p.id,
                            sequence: p.sequence + 1,
                            time: p.expires,
                            at,
                        });
                    }
                    terminal = true;
                }
                Ok(terminal)
            })();
            match result {
                Ok(false) => {
                    self.firing.flights.insert(id, p);
                }
                Ok(true) => {}
                Err(e) => {
                    self.firing.flights.insert(id, p);
                    return Err(e);
                }
            }
        }
        let groups: BTreeSet<_> = self
            .firing
            .flights
            .values()
            .map(|p| p.volley_group)
            .collect();
        self.firing.suppressed.retain(|id, _| groups.contains(id));
        let retained: BTreeSet<_> = self.firing.flights.values().map(|p| p.shooter).collect();
        let released: Vec<_> = self
            .combat
            .iter()
            .filter_map(|(id, _)| {
                (!self.world.game.units.contains_key(&id) && !retained.contains(&id)).then_some(id)
            })
            .collect();
        for id in released {
            // Operative admission retains this identity only while its source
            // metadata backs a live flight. Release the other retained owners
            // together once the same flight predicate releases Combat.
            if self.commands.units.get(&id).is_some_and(|c| c.operative) {
                self.commands.requests.retire_unit(id);
                self.commands.units.remove(&id);
                self.firing.remove(id);
            }
            self.combat.remove(id);
        }
        Ok(())
    }
    fn flight_contact(
        &self,
        p: &LiveFlight,
        a: projectiles::Point3,
        b: projectiles::Point3,
        start_fraction: f64,
        end_fraction: f64,
    ) -> Result<Option<FlightContact>, String> {
        let g = &self.world.game;
        let level_height = n(self.firing_config()?, "levelHeight", 2.5);
        let mut best: Option<FlightContact> = None;
        for c in projectiles::crossed_cells(
            projectiles::BodyPosition { x: a.x, z: a.z },
            projectiles::BodyPosition { x: b.x, z: b.z },
            CELL,
            g.w,
            g.h,
        )
        .map_err(|e| format!("flight cell traversal: {e:?}"))?
        {
            let ch = g.physical(c);
            if p.sequence == 0
                && (c == p.start_cell
                    || (ch == b'#'
                        && (c % g.w).abs_diff(p.start_cell % g.w) <= 1
                        && (c / g.w).abs_diff(p.start_cell / g.w) <= 1))
            {
                continue;
            }
            let x = (c % g.w) as f64 * CELL;
            let z = (c / g.w) as f64 * CELL;
            let ground = g.height.as_ref().map_or(0.0, |h| f64::from(h[c])) * level_height;
            let obstacle = match ch {
                b'B' | b'K' => 5.0,
                b'H' => 1.8,
                b'#' | b'Y' => 1.2,
                b'Q' => 1.6,
                b'R' | b'O' => 0.8,
                b'=' => 0.25,
                _ => 0.0,
            };
            if let Some(hit) = projectiles::sweep_box(
                a,
                b,
                projectiles::Box3 {
                    x: [x, x + CELL],
                    y: [-100.0, ground + obstacle],
                    z: [z, z + CELL],
                },
            )
            .map_err(|e| format!("flight terrain sweep: {e:?}"))?
                && best.is_none_or(|old| hit.fraction < old.hit.fraction)
            {
                best = Some(FlightContact {
                    hit,
                    surface: FlightSurface::Cell(c),
                });
            }
        }
        let center = ((a.x + b.x) / 2.0, (a.z + b.z) / 2.0);
        let reach =
            math::hypot(b.x - a.x, b.z - a.z) / 2.0 + 14.0 + if g.naval { 55.0 } else { 0.0 };
        // Exact unit-contact ties use numeric IDs instead of bucket or registration history.
        let mut candidates = self.grid.in_order(&g.units, center, reach, |_, t| {
            t.hp > 0.0
                && t.air.is_none()
                && !t.riding
                && t.id != p.shooter
                && (p.blast_source.is_some() || !g.allied(p.owner, t.owner))
        });
        candidates.sort_unstable();
        for id in candidates {
            let t = &g.units[&id];
            let def = &self.rules.kinds[t.kind];
            if def.building && t.cells.as_ref().is_some_and(|c| !c.is_empty()) {
                continue;
            }
            let old = self
                .firing
                .prior
                .get(&id)
                .copied()
                .unwrap_or_else(|| t.pose());
            let before = projectiles::BodyPosition {
                x: old.x + (t.x - old.x) * start_fraction,
                z: old.z + (t.z - old.z) * start_fraction,
            };
            let after = projectiles::BodyPosition {
                x: old.x + (t.x - old.x) * end_fraction,
                z: old.z + (t.z - old.z) * end_fraction,
            };
            let ground = f64::from(g.level_at(t.x, t.z)) * level_height;
            let radius = n(self.definition(t)?, "radius", 1.5) * 0.8;
            let top = ground
                + if def.structure || def.naval {
                    5.0
                } else if def.infantry {
                    1.8
                } else {
                    2.6
                };
            let hit = if def.hull != 0.0 {
                projectiles::sweep_hull(
                    a,
                    b,
                    projectiles::HullPosition {
                        x: before.x,
                        z: before.z,
                        rotation: Some(old.rot),
                    },
                    projectiles::HullPosition {
                        x: after.x,
                        z: after.z,
                        rotation: Some(t.rot),
                    },
                    radius,
                    def.hull,
                    ground,
                    top,
                )
            } else {
                projectiles::sweep_body(a, b, before, after, radius, ground, top)
            }
            .map_err(|e| format!("flight body sweep: {e:?}"))?;
            if let Some(hit) = hit
                && best.is_none_or(|old| hit.fraction < old.hit.fraction)
            {
                best = Some(FlightContact {
                    hit,
                    surface: FlightSurface::Unit(id),
                });
            }
        }
        Ok(best)
    }
    pub(crate) fn fire_weapon(
        &mut self,
        id: UnitId,
        target: UnitId,
        moving: bool,
    ) -> Result<(), String> {
        let g = &self.world.game;
        let u = &g.units[&id];
        let t = &g.units[&target];
        let w = self.weapon(u)?.ok_or("unarmed firing")?;
        let definition = self.definition(u)?;
        let target_definition = self.definition(t)?;
        let salvo = flag(w, "salvo");
        if salvo {
            let at = t.at();
            let interval = n(w, "interval", 0.0);
            let supply_rate = self.logistics.shortage_rate(id);
            self.launch_weapon_salvo(id, at, None)?;
            self.firing.initialize(id).cooldown = interval * supply_rate;
            return Ok(());
        }
        let per_model = flag(w, "perModel");
        let requested = if per_model {
            (u.hp / n(definition, "hpPer", 1.0)).ceil()
        } else {
            1.0
        };
        if !requested.is_finite() || !(0.0..=4096.0).contains(&requested) {
            return Err("invalid volley size".into());
        }
        let requested = math::max(0.0, requested).floor();
        let count = self.logistics.get(id).map_or(requested, |r| {
            math::min(requested, math::max(0.0, r.ammo).floor())
        }) as u32;
        let supply_rate = self.logistics.shortage_rate(id);
        let end_id = self
            .firing
            .next_flight
            .checked_add(u64::from(count))
            .filter(|n| *n <= 9_007_199_254_740_991)
            .ok_or("flight ID exhausted")?;
        let cfg = self.firing_config()?;
        let level_height = n(cfg, "levelHeight", 2.5);
        let suppression_accuracy = if u.supp >= 90.0 {
            0.5
        } else if u.supp >= 50.0 {
            0.7
        } else {
            1.0
        };
        let mut rate = if u.supp >= 90.0 {
            3.0
        } else if u.supp >= 50.0 {
            1.5
        } else {
            1.0
        };
        let inf = self.rules.kinds[t.kind].infantry;
        let cover = if flag(w, "flame") {
            1.0
        } else {
            math::min(
                self.cover_mul(t, u.at())?,
                if inf {
                    1.0 - (1.0 - n(cfg, "coverMul", 1.0)) * self.cover_behind(t, u.at())?
                } else {
                    1.0
                },
            )
        };
        let hg = math::min(
            1.45,
            math::max(
                0.7,
                1.0 + n(cfg, "highGroundAcc", 0.15)
                    * f64::from(g.level_at(u.x, u.z) - g.level_at(t.x, t.z)),
            ),
        );
        let xp = self.combat.get(id).map_or(0.0, |m| m.xp);
        let veterancy = crate::combat::veterancy(&self.rules.kinds[u.kind].name, xp)
            .map_err(|e| format!("{e:?}"))?;
        let mut accuracy = n(w, if inf { "accInf" } else { "accVeh" }, 0.0)
            * suppression_accuracy
            * if moving {
                n(w, "moveFire", f64::NAN)
            } else {
                1.0
            }
            * cover
            * hg
            * (1.0 + n(cfg, "vetAcc", 0.1) * f64::from(veterancy));
        let ability = self
            .abilities
            .get(id)
            .ok_or("missing firing ability state")?;
        if !ability.buff.is_finite() {
            return Err("invalid firing ability buff".into());
        }
        let ab = definition.get("ab").ok_or("missing ability definition")?;
        let damage_scale = if ability.buff > 0.0 { 0.5 } else { 1.0 };
        let mut suppression = n(w, "supp", 0.0);
        if ability.buff > 0.0 {
            suppression *= 2.5;
            rate *= 0.5;
        }
        let ap = ability.ap && !inf && (!flag(ab, "naval") || self.rules.kinds[t.kind].naval);
        if ap {
            accuracy = 1.0;
        }
        let aim = self.aim_point(u, t);
        let bearing = math::atan2(aim.z - u.z, aim.x - u.x);
        let from = projectiles::Point3::new(
            u.x,
            f64::from(g.level_at(u.x, u.z)) * level_height + self.weapon_launch_height(u),
            u.z,
        );
        let profile = projectiles::projectile_profile(
            &self.rules.kinds[u.kind].name,
            flag(w, "flame"),
            n(w, "veh", 0.0),
            flag(w, "shellTerrain"),
        )
        .map_err(|e| format!("projectile profile: {e:?}"))?;
        let mut random = self.start.random.clone();
        let mut flights = Vec::with_capacity(count as usize);
        let first_id = self.firing.next_flight;
        for i in 0..count {
            #[cfg(test)]
            if let Some(replay) = &random.replay {
                replay.shot(g.tick, id, t.id, i);
            }
            let (x, z, height) = {
                let accurate = random.next_fraction(crate::random::StreamId::Combat) < accuracy;
                let off = if accurate {
                    0.0
                } else {
                    (n(target_definition, "radius", 1.5) * 0.95
                        + 0.2
                        + random.next_fraction(crate::random::StreamId::Combat) * 0.7)
                        * if random.next_fraction(crate::random::StreamId::Combat) < 0.5 {
                            -1.0
                        } else {
                            1.0
                        }
                };
                (
                    aim.x - math::sin(bearing) * off,
                    aim.z + math::cos(bearing) * off,
                    if accurate {
                        if inf { 0.8 } else { 1.2 }
                    } else {
                        0.08
                    },
                )
            };
            let at =
                projectiles::Point3::new(x, f64::from(g.level_at(x, z)) * level_height + height, z);
            let solution = projectiles::launch_solution(
                from,
                at,
                profile.speed.unwrap_or(1.0),
                profile.gravity,
                None,
            )
            .map_err(|e| format!("launch solution: {e:?}"))?;
            let delay = 0.0;
            let born = g.tick as f64 * 0.05 + delay;
            let expires = born
                + math::max(
                    solution.duration + 0.15,
                    n(w, "range", 20.0) / profile.speed.unwrap_or(1.0),
                );
            if !born.is_finite() || !expires.is_finite() {
                return Err("invalid flight lifetime".into());
            }
            let flight_id = first_id + u64::from(i);
            flights.push(LiveFlight {
                id: flight_id,
                owner: u.owner,
                shooter: id,
                source_kind: u.kind,
                solution: projectiles::Flight {
                    origin: solution.origin,
                    velocity: solution.velocity,
                    gravity: solution.gravity,
                    stamp: born,
                },
                launch_origin: from,
                launched: born,
                last: born,
                expires,
                sent: delay == 0.0,
                energy: 1.0,
                sequence: 0,
                max_contacts: profile.max_contacts,
                near: profile.near,
                start_cell: self.weapon_start_cell(u)?,
                intended: Some(target),
                damage_scale,
                armor_piercing: ap,
                ap_multiplier: n(
                    definition.get("ab").ok_or("missing ability definition")?,
                    "mult",
                    1.5,
                ),
                ap_naval: definition.get("ab").is_some_and(|a| flag(a, "naval")),
                suppression,
                suppression_scale: if per_model {
                    f64::from(count) / n(definition, "models", 1.0)
                } else {
                    1.0
                },
                volley_group: first_id,
                explosive: flag(w, "shellTerrain"),
                terrain: n(w, "shellTerrain", 0.0),
                scar: if flag(w, "shellTerrain") {
                    n(cfg.get("scar").ok_or("missing scar rules")?, "tank", 0.25)
                } else {
                    0.0
                },
                blast_radius: None,
                blast_source: None,
                effect: None,
            });
        }
        self.logistics.consume_ammo(id, requested);
        // Health/profile geometry and ID capacity have all passed. AP is spent on
        // a committed qualifying launch, keeping refusal paths unchanged.
        if ap && count > 0 {
            self.abilities.get_mut(id).unwrap().ap = false;
        }
        self.start.random = random;
        self.firing.next_flight = end_id;
        for flight in flights {
            self.firing
                .suppressed
                .entry(flight.volley_group)
                .or_default();
            if flight.sent {
                self.firing.events.push(LaunchEvent::from_flight(&flight));
            }
            self.firing.flights.insert(flight.id, flight);
        }
        self.world.game.units.get_mut(&id).unwrap().shot_at = Some(self.world.game.tick as f64);
        self.firing.initialize(id).cooldown = n(w, "interval", 0.0) * rate * supply_rate;
        Ok(())
    }

    /// Physical salvos retain the reference timer's two draws for each emitted segment.
    /// The projectile already carries the physical payload, so this timer emits no second blast.
    pub fn advance_firing_salvos(&mut self, dt: f64) -> Result<(), String> {
        self.supported_firing_mode()?;
        if !dt.is_finite() || dt < 0.0 {
            return Err("invalid salvo timer step".into());
        }
        if self.firing.salvos.iter().any(|s| {
            s.left == 0
                || s.left > 4096
                || crate::match_actor::checked_game_owner(&self.world.game, s.owner).is_err()
                || ![s.at.x, s.at.z, s.next, s.every, s.spread]
                    .iter()
                    .all(|v| v.is_finite())
                || s.every <= 0.0
                || s.spread < 0.0
        }) {
            return Err("invalid physical salvo".into());
        }
        for s in &mut self.firing.salvos {
            s.next -= dt;
            if s.next > 0.0 {
                continue;
            }
            self.start
                .random
                .next_fraction(crate::random::StreamId::Combat);
            self.start
                .random
                .next_fraction(crate::random::StreamId::Combat);
            s.left -= 1;
            s.next = s.every;
        }
        self.firing.salvos.retain(|s| s.left > 0);
        Ok(())
    }
    fn turn_firing_hull(&mut self, id: UnitId, to: f64, dt: f64) -> Result<(), String> {
        let g = &self.world.game;
        let u = &g.units[&id];
        if u.kind >= self.rules.kinds.len()
            || crate::match_actor::checked_game_owner(&self.world.game, u.owner).is_err()
            || ![u.x, u.z, u.hp, u.rot, u.supp]
                .iter()
                .all(|v| v.is_finite())
        {
            return Err("invalid canonical weapon unit".into());
        }
        let kind = &self.rules.kinds[u.kind];
        let d = math::atan2(math::sin(to - u.rot), math::cos(to - u.rot));
        let max = kind.vehicle.as_ref().map_or(
            n(
                self.firing_config()?
                    .get("behavior")
                    .ok_or("missing behavior rules")?,
                "hullTurn",
                1.5,
            ),
            |p| p.hull_turn,
        ) * dt;
        let r = u.rot + math::max(-max, math::min(max, d));
        let next = math::atan2(math::sin(r), math::cos(r));
        if !kind.ground_vehicle()
            || crate::vehicle::swept_vehicle_clear(
                g,
                u.pose(),
                crate::game::Pose {
                    rot: next,
                    ..u.pose()
                },
                kind,
                kind.block(),
            )
        {
            self.world.game.units.get_mut(&id).unwrap().rot = next;
        }
        Ok(())
    }
    /// Run one unit's source targeting/firing block after its movement and grid update.
    /// Timer decay is caller-owned through advance_firing_timers before movement.
    /// The same metadata guard used at the real weapon turn, without any mutation.
    pub(crate) fn validate_firing_weapon_metadata(&self, u: &Unit) -> Result<(), String> {
        if self
            .firing
            .get(u.id)
            .is_some_and(|r| !r.cooldown.is_finite() || !r.retarget.is_finite())
            || self
                .combat
                .get(u.id)
                .is_some_and(|r| !r.xp.is_finite() || r.xp < 0.0 || !r.hit_at.is_finite())
            || u.hit_from
                .is_some_and(|p| !p.x.is_finite() || !p.z.is_finite())
        {
            return Err("invalid canonical weapon metadata".into());
        }
        Ok(())
    }

    pub fn apply_unit_firing(&mut self, id: UnitId, moving: bool, dt: f64) -> Result<(), String> {
        self.supported_firing_mode()?;
        if !dt.is_finite() || dt < 0.0 {
            return Err("invalid firing step".into());
        }
        let Some(u) = self.world.game.units.get(&id) else {
            return Ok(());
        };
        if u.hp <= 0.0 {
            return Ok(());
        }
        if u.kind >= self.rules.kinds.len()
            || crate::match_actor::checked_game_owner(&self.world.game, u.owner).is_err()
            || ![u.x, u.z, u.hp, u.rot, u.supp]
                .iter()
                .all(|v| v.is_finite())
        {
            return Err("invalid canonical weapon unit".into());
        }
        self.validate_firing_weapon_metadata(u)?;
        let kind = &self.rules.kinds[u.kind];
        let control = self
            .commands
            .units
            .get(&id)
            .ok_or("missing canonical unit control")?;
        if kind.air
            || kind.rail
            || control.operative
            || control.fire_at.is_some()
            || control.nade.is_some()
        {
            return Err("air, rail, operative and ability/terrain firing are unsupported".into());
        }
        if u.riding {
            return Ok(());
        }
        if kind.building && (!kind.weapon || u.built.is_none_or(|v| v < 1.0)) {
            return Ok(());
        }
        let attack = control.attack_id;
        let hold_fire = control.hold_fire;
        let Some(w) = self.weapon(u)? else {
            self.firing.initialize(id).target_id = 0;
            self.commands.units.get_mut(&id).unwrap().aim = u.rot;
            return Ok(());
        };
        if flag(w, "beam") {
            return Err("beam firing is unsupported".into());
        }
        let hull = !kind.infantry && !kind.structure && !moving;
        if hull {
            let actor = self.actor_policy(u.owner)?;
            let target = self
                .firing
                .get(id)
                .and_then(|s| self.world.game.units.get(&s.target_id))
                .filter(|t| t.hp > 0.0 && actor.sees(t.id));
            let seen = target.map(Unit::at);
            let danger = target.is_some_and(|t| {
                self.weapon(t)
                    .ok()
                    .flatten()
                    .is_some_and(|w| n(w, "veh", 0.0) >= 20.0)
            });
            let recent = self
                .combat
                .get(id)
                .is_some_and(|m| self.world.game.tick as f64 - m.hit_at <= 100.0);
            let at = if danger {
                seen
            } else if recent && u.hit_from.is_some() {
                u.hit_from
            } else {
                seen
            };
            if let Some(at) = at {
                self.turn_firing_hull(id, math::atan2(at.z - u.z, at.x - u.x), dt)?;
            }
        }
        let u = &self.world.game.units[&id];
        let at = u.at();
        let rot = u.rot;
        let retreating = u.retreating;
        let still = u.still;
        let amove = u.amove;
        let hit_from = u.hit_from;
        let infantry = self.rules.kinds[u.kind].infantry;
        if n(w, "range", 0.0) == 0.0 || !self.logistics.can_fire(id, 1.0) || retreating {
            self.firing.initialize(id).target_id = 0;
            self.commands.units.get_mut(&id).unwrap().aim = rot;
            return Ok(());
        }
        let current = self.firing.get(id).copied().unwrap_or_default();
        if hold_fire && attack == 0 {
            self.set_firing_target(id, 0)?;
        } else if attack == 0
            && (current.retarget <= 0.0
                || (current.target_id != 0 && !self.can_shoot_target(id, current.target_id)?))
        {
            let target = self.pick_firing_target(id)?;
            self.set_firing_target(id, target)?;
            self.firing.initialize(id).retarget = 0.5;
        }
        let target = self.firing.get(id).unwrap().target_id;
        if target != 0 && self.can_shoot_target(id, target)? {
            let t = &self.world.game.units[&target];
            let aim = math::atan2(t.z - at.z, t.x - at.x);
            self.commands.units.get_mut(&id).unwrap().aim = aim;
            if infantry && !moving {
                self.world.game.units.get_mut(&id).unwrap().rot = aim;
            }
            let ready = if moving {
                w.get("moveFire").is_some()
            } else {
                still.is_some_and(|s| s >= n(w, "setup", 0.0))
            };
            if ready
                && self.firing.get(id).unwrap().cooldown <= 0.0
                && (!flag(w, "area") || attack == target)
            {
                self.fire_weapon(id, target, moving)?;
            }
        } else {
            self.set_firing_target(id, 0)?;
            if let Some(face) = self.commands.units[&id].face
                && !moving
                && amove.is_none()
                && (!hull
                    || hit_from.is_none()
                    || self
                        .combat
                        .get(id)
                        .is_none_or(|m| self.world.game.tick as f64 - m.hit_at > 100.0))
            {
                if hull {
                    self.turn_firing_hull(id, face.angle, dt)?;
                } else if infantry {
                    self.world.game.units.get_mut(&id).unwrap().rot = face.angle;
                }
            }
            self.commands.units.get_mut(&id).unwrap().aim = self.world.game.units[&id].rot;
        }
        Ok(())
    }
    fn firing_config(&self) -> Result<&'static Value, String> {
        Catalog::compiled()
            .map_err(|e| format!("{e:?}"))?
            .table("CFG")
            .map_err(|e| e.to_string())
    }
    fn cell_max_hp(&self, cell: usize) -> f64 {
        let g = &self.world.game;
        if let Some(s) = g.sections.get(&(cell as u32))
            && !s.failed
            && let Some(hp) = s.max_hp
        {
            return hp;
        }
        let ch = g.physical(cell);
        if ch == b'B' {
            self.rules.movement.house_hp[usize::from(g.house[cell])]
        } else {
            self.rules.movement.terrain_hp(ch).unwrap_or(0.0)
                * g.tough.as_ref().map_or(1.0, |t| t[cell])
        }
    }
    fn cover_quality(&self, cell: usize) -> Result<f64, String> {
        let g = &self.world.game;
        let cfg = self.firing_config()?;
        let ch = g.physical(cell);
        let max = self.cell_max_hp(cell);
        Ok(if ch == b'O' {
            n(cfg.get("wood").ok_or("missing wood rules")?, "cover", 0.6)
        } else if ch == b'+' {
            math::min(
                1.4,
                0.55 + g.wear.as_ref().map_or(0.45, |w| f64::from(w[cell])),
            )
        } else if max > 1.0 {
            0.5 + 0.5 * math::max(0.0, f64::from(g.cell_hp[cell])) / max
        } else {
            1.0
        })
    }
    pub(crate) fn cover_mul(&self, t: &Unit, from: Point) -> Result<f64, String> {
        let g = &self.world.game;
        let cfg = self.firing_config()?;
        if let Some(c) = t.garrison {
            let houses = cfg
                .get("houses")
                .and_then(Value::array)
                .ok_or("missing house rules")?;
            let house = houses
                .get(usize::from(g.house[c]))
                .ok_or("invalid house kind")?;
            let worn = n(house, "worn", 1.0);
            return Ok(worn
                + (n(house, "mul", 1.0) - worn) * math::max(0.0, f64::from(g.cell_hp[c]))
                    / n(house, "hp", 1.0));
        }
        let c = g.cell_of(t.x, t.z).ok_or("unit outside battlefield")?;
        let trench = cfg.get("trench").ok_or("missing trench rules")?;
        let front = g.trench_front.as_ref().map(|f| f64::from(f[c]));
        let faces = front.is_none_or(|f| {
            f.is_nan()
                || math::cos(math::atan2(from.z - t.z, from.x - t.x) - f) > n(trench, "arc", 0.0)
        });
        if vision::in_trench(g, &self.rules, t) && faces {
            let fresh = n(trench, "fresh", 1.0);
            let dug = n(trench, "dug", 1.0);
            return Ok(fresh
                + (dug - fresh)
                    * math::min(1.0, t.still.unwrap_or(f64::NAN) / n(trench, "settle", 1.0)));
        }
        Ok(
            if self.rules.kinds[t.kind].infantry && g.flags[c] & COVER != 0 {
                1.0 - (1.0 - n(cfg, "coverMul", 1.0)) * self.cover_quality(c)?
            } else {
                1.0
            },
        )
    }
    pub(crate) fn cover_behind(&self, t: &Unit, from: Point) -> Result<f64, String> {
        let g = &self.world.game;
        let a = math::atan2(from.z - t.z, from.x - t.x);
        let ca = math::cos(a);
        let sa = math::sin(a);
        let mut best = 0.0;
        let mut d = 0.8;
        while d < 3.7 {
            if let Some(c) = g.cell_of(t.x + ca * d, t.z + sa * d)
                && b"B#RHKQ".contains(&g.physical(c))
            {
                best = math::min(1.0, (3.8 - d) / 1.6) * math::min(1.0, self.cover_quality(c)?);
                break;
            }
            d += 0.7;
        }
        let x = (t.x / CELL).floor() as isize;
        let z = (t.z / CELL).floor() as isize;
        for (dx, dz) in [
            (1, 0),
            (-1, 0),
            (0, 1),
            (0, -1),
            (1, 1),
            (1, -1),
            (-1, 1),
            (-1, -1),
        ] {
            let nx = x + dx;
            let nz = z + dz;
            if nx < 0 || nz < 0 || nx >= g.w as isize || nz >= g.h as isize {
                continue;
            }
            if b"BK".contains(&g.physical(nz as usize * g.w + nx as usize))
                && (dx as f64 * ca + dz as f64 * sa) / math::hypot(dx as f64, dz as f64) > 0.5
            {
                best = 1.0;
            }
        }
        let mut hull = |at: Point| {
            let dx = at.x - t.x;
            let dz = at.z - t.z;
            let d = math::hypot(dx, dz);
            if d > 0.0 && d < 4.0 && (dx * ca + dz * sa) / d > 0.7 {
                best = math::max(best, math::min(1.0, (4.0 - d) / 1.5));
            }
        };
        for v in g.units.values() {
            if v.id != t.id && v.air.is_none() && !self.rules.kinds[v.kind].infantry && v.hp > 0.0 {
                hull(v.at());
            }
        }
        for v in &g.wrecks {
            if !v.moving {
                hull(Point { x: v.x, z: v.z });
            }
        }
        Ok(best)
    }
    fn volley(&self, u: &Unit, t: &Unit) -> Result<f64, String> {
        let Some(w) = self.weapon(u)? else {
            return Ok(0.0);
        };
        if flag(w, "salvo") {
            return Ok(0.0);
        }
        let inf = self.rules.kinds[t.kind].infantry;
        let count = if flag(w, "perModel") {
            (u.hp / n(self.definition(u)?, "hpPer", 1.0)).ceil()
        } else {
            1.0
        };
        Ok(n(w, if inf { "inf" } else { "veh" }, 0.0)
            * n(w, if inf { "accInf" } else { "accVeh" }, 0.0)
            * count)
    }
    pub(crate) fn set_firing_target(&mut self, id: UnitId, next: UnitId) -> Result<(), String> {
        let u = self.world.game.units.get(&id).ok_or("missing shooter")?;
        let previous = self.firing.get(id).map_or(0, |s| s.target_id);
        if previous != next {
            let team = self.actor_policy(u.owner)?.team;
            for (target, sign) in [(previous, -1.0), (next, 1.0)] {
                if let Some(t) = self.world.game.units.get(&target) {
                    let amount = self.volley(u, t)?;
                    *self.firing.claims.entry((target, team)).or_default() += sign * amount;
                }
            }
        }
        self.firing.initialize(id).target_id = next;
        Ok(())
    }
    /// Build shared target claims before the ordered per-unit timer/movement/firing loop.
    pub fn begin_firing_tick(&mut self, unit_order: &[UnitId]) -> Result<(), String> {
        self.supported_firing_mode()?;
        if unit_order.windows(2).any(|w| w[0] >= w[1]) {
            return Err("firing unit IDs must be strictly increasing".into());
        }
        if self.world.game.tick < 0 || self.world.game.tick > 9_007_199_254_740_351 {
            return Err("invalid firing tick".into());
        }
        self.validate_firing_units()?;
        let mut claims = BTreeMap::new();
        for u in self.world.game.units.values() {
            if u.hp <= 0.0 {
                continue;
            }
            let target = self.firing.get(u.id).map_or(0, |s| s.target_id);
            if let Some(t) = self.world.game.units.get(&target) {
                *claims
                    .entry((target, self.actor_policy(u.owner)?.team))
                    .or_default() += self.volley(u, t)?;
            }
        }
        self.firing.claims = claims;
        if self.firing.prior_tick != Some(self.world.game.tick) {
            self.firing.prior = self
                .world
                .game
                .units
                .iter()
                .map(|(&id, u)| (id, u.pose()))
                .collect();
            self.firing.prior_tick = Some(self.world.game.tick);
        }
        Ok(())
    }
    /// The tick integrator calls this once before this unit's movement, not at firing time.
    pub fn advance_firing_timers(&mut self, id: UnitId, dt: f64) -> Result<(), String> {
        self.supported_firing_mode()?;
        if !dt.is_finite() || dt < 0.0 {
            return Err("invalid weapon timer step".into());
        }
        let Some(u) = self.world.game.units.get(&id) else {
            return Ok(());
        };
        if u.hp <= 0.0 || u.riding {
            return Ok(());
        }
        if u.kind >= self.rules.kinds.len()
            || crate::match_actor::checked_game_owner(&self.world.game, u.owner).is_err()
            || ![u.x, u.z, u.hp, u.rot, u.supp]
                .iter()
                .all(|v| v.is_finite())
        {
            return Err("invalid canonical weapon unit".into());
        }
        let kind = &self.rules.kinds[u.kind];
        if kind.air || kind.rail {
            return Err("air and rail firing phases are unsupported".into());
        }
        if kind.building && (!kind.weapon || u.built.is_none_or(|n| n < 1.0)) {
            return Ok(());
        }
        if self
            .firing
            .get(id)
            .is_some_and(|r| !r.cooldown.is_finite() || !r.retarget.is_finite())
        {
            return Err("invalid canonical weapon timers".into());
        }
        let row = self.firing.initialize(id);
        row.cooldown -= dt;
        row.retarget -= dt;
        Ok(())
    }
    pub(crate) fn pick_firing_target(&self, id: UnitId) -> Result<UnitId, String> {
        let g = &self.world.game;
        let u = &g.units[&id];
        let w = self.weapon(u)?.ok_or("unarmed target selection")?;
        let current = self.firing.get(id).map_or(0, |s| s.target_id);
        let actor = self.actor_policy(u.owner)?;
        let team = actor.team;
        let salvo = flag(w, "salvo");
        let pool = if salvo {
            self.grid.unordered(
                &g.units,
                (u.x, u.z),
                self.weapon_range(u, w)? + 6.0,
                |_, t| !t.riding && !g.allied(t.owner, u.owner) && actor.sees(t.id),
            )
        } else {
            vec![]
        };
        let targets = self.grid.in_order(
            &g.units,
            (u.x, u.z),
            self.weapon_range(u, w)? + if g.naval { 55.0 } else { 0.0 },
            |_, t| t.hp > 0.0 && t.air.is_none() && !g.allied(t.owner, u.owner) && actor.sees(t.id),
        );
        let mut best = 0;
        let mut best_score = f64::INFINITY;
        let mut current_score = f64::INFINITY;
        for target in targets {
            if !self.can_shoot_target(id, target)? {
                continue;
            }
            let t = &g.units[&target];
            let definition = self.definition(t)?;
            let infantry = self.rules.kinds[t.kind].infantry;
            let trench = vision::in_trench(g, &self.rules, t);
            let covered = infantry && g.flags_at(t.x, t.z) & COVER != 0;
            let dug = if t.garrison.is_some() {
                3.0
            } else if trench {
                2.0
            } else if covered {
                1.5
            } else {
                1.0
            };
            let value = if salvo {
                dug * (1 + pool
                    .iter()
                    .filter(|&&other| {
                        let o = &g.units[&other];
                        o.owner == t.owner && dist(o.at(), t.at()) < 6.0
                    })
                    .count()) as f64
            } else {
                let factor = if infantry {
                    if flag(w, "flame") {
                        if t.garrison.is_some() || trench {
                            n(w, "flame", 1.0)
                        } else {
                            1.0
                        }
                    } else {
                        self.cover_mul(t, u.at())?
                    }
                } else if self.rules.kinds[t.kind].structure {
                    1.0
                } else {
                    crate::combat::armor_mul(&self.rules.kinds[t.kind].name, t.at(), t.rot, u.at())
                        .map_err(|e| format!("{e:?}"))?
                };
                n(w, if infantry { "inf" } else { "veh" }, 0.0)
                    * n(w, if infantry { "accInf" } else { "accVeh" }, 0.0)
                    * factor
                    * crate::combat::role_mul(
                        &self.rules.kinds[u.kind].name,
                        &self.rules.kinds[t.kind].name,
                    )
                    .map_err(|e| format!("{e:?}"))?
            };
            let meta = self.combat.get(id);
            let target_state = self.firing.get(t.id);
            let incoming = if meta.is_some_and(|m| m.hit_by == Some(t.id)) {
                meta.unwrap().hit_at
            } else if target_state.is_some_and(|s| s.target_id == id) {
                t.shot_at.unwrap_or(-1e9)
            } else {
                -1e9
            };
            let cfg = self.firing_config()?;
            let threat_time = n(
                cfg.get("behavior").ok_or("missing behavior rules")?,
                "threatTime",
                5.0,
            ) / 0.05;
            let threat = if g.tick as f64 - incoming <= threat_time {
                2.5
            } else if target_state.is_some_and(|s| s.target_id == id) {
                1.5
            } else {
                1.0
            };
            let others = if salvo {
                0.0
            } else {
                self.firing
                    .claims
                    .get(&(t.id, team))
                    .copied()
                    .unwrap_or(0.0)
                    - if t.id == current {
                        self.volley(u, t)?
                    } else {
                        0.0
                    }
            };
            let score = dist(u.at(), t.at()) / (value * threat)
                * if flag(definition, "structure") {
                    5.0
                } else {
                    1.0
                }
                * math::max(1.0, others / t.hp);
            if t.id == current {
                current_score = score;
            }
            if score < best_score {
                best_score = score;
                best = t.id;
            }
        }
        let gain = n(
            self.firing_config()?
                .get("behavior")
                .ok_or("missing behavior rules")?,
            "switchGain",
            0.7,
        );
        Ok(
            if current_score < f64::INFINITY && best_score > current_score * gain {
                current
            } else {
                best
            },
        )
    }
    fn definition(&self, u: &Unit) -> Result<&'static Value, String> {
        let kind = self.rules.kinds.get(u.kind).ok_or("unknown firing kind")?;
        Catalog::compiled()
            .map_err(|e| format!("{e:?}"))?
            .unit(&kind.name)
            .map_err(|e| e.to_string())
    }
    fn weapon(&self, u: &Unit) -> Result<Option<&'static Value>, String> {
        Ok(self.definition(u)?.get("w"))
    }
    fn hull_point(&self, from: Point, target: &Unit) -> Point {
        let h = self.rules.kinds[target.kind].hull;
        if h == 0.0 {
            return target.at();
        }
        let cx = math::cos(target.rot);
        let cz = math::sin(target.rot);
        let k = math::max(
            -h,
            math::min(h, (from.x - target.x) * cx + (from.z - target.z) * cz),
        );
        Point {
            x: target.x + cx * k,
            z: target.z + cz * k,
        }
    }
    pub(crate) fn aim_point(&self, shooter: &Unit, target: &Unit) -> Point {
        let Some(cells) = &target.cells else {
            return self.hull_point(shooter.at(), target);
        };
        let mut best = target.at();
        let mut distance = f64::INFINITY;
        for &cell in cells {
            if self
                .world
                .game
                .sections
                .get(&cell)
                .is_some_and(|s| s.failed || s.hp <= 0.0)
            {
                continue;
            }
            let at = Point {
                x: (cell as usize % self.world.game.w) as f64 * CELL + CELL / 2.0,
                z: (cell as usize / self.world.game.w) as f64 * CELL + CELL / 2.0,
            };
            let d = dist(shooter.at(), at);
            if d < distance {
                distance = d;
                best = at;
            }
        }
        best
    }
    /// Current authoritative visibility, alliance, hull range and line-of-sight query.
    /// Salvos require spotting and minimum range instead of a direct sight line.
    pub fn can_shoot_target(&self, shooter: UnitId, target: UnitId) -> Result<bool, String> {
        self.supported_firing_mode()?;
        let g = &self.world.game;
        let Some(u) = g.units.get(&shooter) else {
            return Ok(false);
        };
        let Some(t) = g.units.get(&target) else {
            return Ok(false);
        };
        if g.w == 0
            || g.h == 0
            || u.kind >= self.rules.kinds.len()
            || t.kind >= self.rules.kinds.len()
            || crate::match_actor::checked_game_owner(g, u.owner).is_err()
            || crate::match_actor::checked_game_owner(g, t.owner).is_err()
            || ![u.x, u.z, u.rot, t.x, t.z, t.rot, t.hp]
                .iter()
                .all(|v| v.is_finite())
            || t.cells
                .as_ref()
                .is_some_and(|cells| cells.iter().any(|&c| c as usize >= g.flags.len()))
        {
            return Err("invalid canonical targeting input".into());
        }
        let Some(w) = self.weapon(u)? else {
            return Ok(false);
        };
        let range = self.weapon_range(u, w)?;
        if range == 0.0
            || !self.logistics.can_fire(u.id, 1.0)
            || t.hp <= 0.0
            || t.air.is_some()
            || t.riding
            || g.allied(t.owner, u.owner)
            || dist(u.at(), self.hull_point(u.at(), t)) > range
            || !self.actor_policy(u.owner)?.sees(t.id)
        {
            return Ok(false);
        }
        if u.air.is_some() {
            return Ok(true);
        }
        Ok(if flag(w, "salvo") {
            dist(u.at(), self.hull_point(u.at(), t)) >= n(w, "minRange", f64::NAN)
        } else {
            let at = self.aim_point(u, t);
            vision::los(g, &self.rules, (u.x, u.z), (at.x, at.z))
        })
    }
}

impl FiringState {
    pub fn target_claims(&self) -> impl Iterator<Item = ((UnitId, i32), f64)> + '_ {
        self.claims.iter().map(|(key, value)| (*key, *value))
    }

    pub fn take_events(&mut self) -> Vec<LaunchEvent> {
        std::mem::take(&mut self.events)
    }
    /// Initialize once. Repeated calls preserve the current target and timers.
    pub fn initialize(&mut self, id: UnitId) -> &mut FiringUnit {
        self.units.entry(id).or_default()
    }

    pub fn get(&self, id: UnitId) -> Option<&FiringUnit> {
        self.units.get(&id)
    }

    pub fn get_mut(&mut self, id: UnitId) -> Option<&mut FiringUnit> {
        self.units.get_mut(&id)
    }

    pub fn remove(&mut self, id: UnitId) -> Option<FiringUnit> {
        self.units.remove(&id)
    }
}

/// Physical grenade bookkeeping from the source late phase. This ticket holds
/// no trajectory or blast payload; those belong to its one canonical flight.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GrenadeTicket {
    pub at: Point,
    pub remaining: f64,
    pub owner: usize,
    pub flight: u64,
}

/// A grenade/satchel profile read from the actual unit ability catalog.
/// Keeping construction private prevents a caller from substituting a weapon profile.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AbilityBlast {
    source: ExplosionSource,
    radius: f64,
    fuse: f64,
}
impl AbilityBlast {
    pub fn from_unit(kind: usize, rules: &crate::rules::Rules) -> Result<Self, String> {
        let name = &rules.kinds.get(kind).ok_or("unknown ability kind")?.name;
        let definition = Catalog::compiled()
            .map_err(|e| e.to_string())?
            .unit(name)
            .map_err(|e| e.to_string())?;
        let ab = definition.get("ab").ok_or("missing blast ability")?;
        if !matches!(
            ab.get("id").and_then(Value::string),
            Some("grenade" | "satchel")
        ) {
            return Err("ability has no physical grenade/satchel profile".into());
        }
        let blast = Self {
            source: ExplosionSource {
                infantry: n(ab, "inf", f64::NAN),
                vehicle: n(ab, "veh", f64::NAN),
                terrain: ab.get("terrain").and_then(Value::number),
                suppression: n(ab, "supp", f64::NAN),
                scar: n(ab, "scar", 0.0),
                anti_garrison: ab.get("antiGarrison").and_then(Value::number),
                weapon_kind: None,
                off_map_support: false,
            },
            radius: n(ab, "radius", f64::NAN),
            fuse: n(ab, "fuse", f64::NAN),
        };
        validate_blast(blast.source)?;
        if !blast.radius.is_finite()
            || !(0.0..=4096.0).contains(&blast.radius)
            || blast.radius == 0.0
            || !blast.fuse.is_finite()
            || !(0.0..=640.0).contains(&blast.fuse)
            || blast.fuse == 0.0
        {
            return Err("invalid ability blast geometry".into());
        }
        Ok(blast)
    }
}
fn validate_blast(source: ExplosionSource) -> Result<(), String> {
    if ![
        source.infantry,
        source.vehicle,
        source.suppression,
        source.scar,
        source.terrain.unwrap_or(0.0),
        source.anti_garrison.unwrap_or(1.0),
    ]
    .iter()
    .all(|v| v.is_finite() && (0.0..=100_000.0).contains(v))
    {
        return Err("invalid flight blast profile".into());
    }
    Ok(())
}

impl Match {
    fn launch_admission(&self, id: UnitId, at: Point, count: u32) -> Result<u64, String> {
        self.supported_firing_mode()?;
        self.validate_firing_units()?;
        let u = self
            .world
            .game
            .units
            .get(&id)
            .ok_or("missing flight shooter")?;
        if u.hp <= 0.0
            || self.combat.get(id).is_none()
            || self.firing.get(id).is_none()
            || self.logistics.get(id).is_some_and(|r| !r.ammo.is_finite())
            || (self.rules.kinds[u.kind].air && !vision::airborne(u))
            || u.riding
            || ![at.x, at.z].iter().all(|v| v.is_finite())
            || !(0.0..=self.world.game.w as f64 * CELL).contains(&at.x)
            || !(0.0..=self.world.game.h as f64 * CELL).contains(&at.z)
            || !(0..9_007_199_254_740_351).contains(&self.world.game.tick)
            || count == 0
            || count > 4096
        {
            return Err("invalid physical flight launch".into());
        }
        let first = self.firing.next_flight;
        let end = first
            .checked_add(u64::from(count))
            .filter(|&n| first > 0 && n <= 9_007_199_254_740_991)
            .ok_or("flight ID exhausted")?;
        if self.firing.flights.range(first..end).next().is_some()
            || self.firing.suppressed.range(first..end).next().is_some()
            || self
                .firing
                .grenades
                .iter()
                .any(|n| (first..end).contains(&n.flight))
        {
            return Err("flight ID already held".into());
        }
        Ok(end)
    }
    #[allow(clippy::too_many_arguments)]
    fn prepare_physical_flight(
        &self,
        id: UnitId,
        at: Point,
        flight_id: u64,
        duration: f64,
        delay: f64,
        profile: projectiles::ProjectileProfile,
        source: ExplosionSource,
        radius: f64,
        effect: &'static str,
    ) -> Result<LiveFlight, String> {
        if !duration.is_finite()
            || !(0.0..=640.0).contains(&duration)
            || duration == 0.0
            || !delay.is_finite()
            || !(0.0..=640.0).contains(&delay)
        {
            return Err("invalid physical flight duration".into());
        }
        validate_blast(source)?;
        if !radius.is_finite() || radius <= 0.0 || radius > 4096.0 {
            return Err("invalid physical blast radius".into());
        }
        let g = &self.world.game;
        let u = &g.units[&id];
        let height = n(self.firing_config()?, "levelHeight", 2.5);
        let from = projectiles::Point3::new(
            u.x,
            f64::from(g.level_at(u.x, u.z)) * height + self.weapon_launch_height(u),
            u.z,
        );
        let end = projectiles::Point3::new(
            at.x,
            f64::from(g.level_at(at.x, at.z)) * height + 0.02,
            at.z,
        );
        let solution = projectiles::launch_solution(
            from,
            end,
            profile.speed.unwrap_or(1.0),
            profile.gravity,
            Some(duration),
        )
        .map_err(|e| format!("physical launch: {e:?}"))?;
        let born = g.tick as f64 * 0.05 + delay;
        Ok(LiveFlight {
            id: flight_id,
            owner: u.owner,
            shooter: id,
            source_kind: u.kind,
            solution: projectiles::Flight {
                origin: solution.origin,
                velocity: solution.velocity,
                gravity: solution.gravity,
                stamp: born,
            },
            launch_origin: from,
            launched: born,
            last: born,
            expires: born + duration,
            sent: delay == 0.0,
            energy: 1.0,
            sequence: 0,
            max_contacts: profile.max_contacts,
            near: profile.near,
            start_cell: self.weapon_start_cell(u)?,
            intended: None,
            damage_scale: 1.0,
            armor_piercing: false,
            ap_multiplier: 1.5,
            ap_naval: false,
            suppression: 0.0,
            suppression_scale: 1.0,
            volley_group: flight_id,
            explosive: true,
            terrain: 0.0,
            scar: 0.0,
            blast_radius: Some(radius),
            blast_source: Some(source),
            effect: Some(effect),
        })
    }
    fn commit_physical_flight(&mut self, flight: LiveFlight) {
        self.firing
            .suppressed
            .entry(flight.volley_group)
            .or_default();
        if flight.sent {
            self.firing.events.push(LaunchEvent::from_flight(&flight));
        }
        self.firing.flights.insert(flight.id, flight);
    }
    /// Launch one real grenade/satchel flight. `None` duration uses the ability fuse;
    /// FPS supplies its source duration and `None` ammo debit. Caller owns admission,
    /// payment, cooldowns and operative counters, which follow this successful result.
    pub fn launch_ability_flight(
        &mut self,
        id: UnitId,
        at: Point,
        blast: AbilityBlast,
        duration: Option<f64>,
        ammo_debit: Option<u32>,
    ) -> Result<u64, String> {
        let end = self.launch_admission(id, at, 1)?;
        if ammo_debit
            .is_some_and(|n| n == 0 || n > 4096 || !self.logistics.can_fire(id, f64::from(n)))
        {
            return Err("insufficient ability ammunition".into());
        }
        let first = self.firing.next_flight;
        let duration = duration.unwrap_or(blast.fuse);
        let flight = self.prepare_physical_flight(
            id,
            at,
            first,
            duration,
            0.0,
            projectiles::GRENADE,
            blast.source,
            blast.radius,
            "boom",
        )?;
        let owner = flight.owner;
        if let Some(n) = ammo_debit {
            self.logistics.consume_ammo(id, f64::from(n));
        }
        self.firing.next_flight = end;
        self.commit_physical_flight(flight);
        self.firing.grenades.push(GrenadeTicket {
            at,
            remaining: duration,
            owner,
            flight: first,
        });
        self.firing.events.push(LaunchEvent::Throw {
            shooter: id,
            owner,
            at,
        });
        Ok(first)
    }
    /// Source launchSalvo, including partial-ammo clamping and delayed rounds.
    /// `None` uses weapon rockets, including the rocket barrage's omitted shell count.
    /// Caller owns the firing/ability cooldown. No IDs, draws or ammo change on error.
    pub fn launch_weapon_salvo(
        &mut self,
        id: UnitId,
        at: Point,
        requested: Option<u32>,
    ) -> Result<Vec<u64>, String> {
        self.launch_admission(id, at, 1)?;
        let u = &self.world.game.units[&id];
        let w = self.weapon(u)?.ok_or("unarmed salvo")?;
        if !flag(w, "salvo") {
            return Err("weapon has no physical salvo".into());
        }
        let requested = requested
            .map(f64::from)
            .unwrap_or_else(|| n(w, "rockets", f64::NAN));
        if !requested.is_finite() || requested <= 0.0 || requested > 4096.0 {
            return Err("invalid salvo size".into());
        }
        let count = self.logistics.get(id).map_or(requested, |r| {
            math::min(requested, math::max(0.0, r.ammo).floor())
        }) as u32;
        let end = self.launch_admission(id, at, count)?;
        let spread = if let Some(far) = w.get("spreadFar").and_then(Value::number) {
            let a = n(w, "spread", 0.0);
            a + (far - a)
                * math::min(
                    1.0,
                    math::max(
                        0.0,
                        (dist(u.at(), at) - n(w, "minRange", 0.0))
                            / (n(w, "range", 0.0) - n(w, "minRange", 0.0)),
                    ),
                )
        } else {
            n(w, "spread", 0.0)
        };
        if !spread.is_finite() || spread < 0.0 || spread > 4096.0 {
            return Err("invalid salvo spread".into());
        }
        let source = ExplosionSource::weapon(u.kind, &self.rules).map_err(|e| e.to_string())?;
        let duration = n(w, "flight", f64::NAN);
        let every = n(w, "every", 0.0);
        let owner = u.owner;
        let first = self.firing.next_flight;
        let mut random = self.start.random.clone();
        let mut flights = Vec::with_capacity(count as usize);
        for i in 0..count {
            let angle =
                random.next_fraction(crate::random::StreamId::Combat) * std::f64::consts::TAU;
            let radius = random.next_fraction(crate::random::StreamId::Combat).sqrt() * spread;
            let end_at = Point {
                x: at.x + math::cos(angle) * radius,
                z: at.z + math::sin(angle) * radius,
            };
            flights.push(self.prepare_physical_flight(
                id,
                end_at,
                first + u64::from(i),
                duration,
                f64::from(i) * every,
                projectiles::ARTILLERY,
                source,
                n(w, "blast", 0.0),
                "rocket",
            )?);
        }
        self.logistics.consume_ammo(id, requested);
        self.start.random = random;
        self.firing.next_flight = end;
        for flight in flights {
            self.commit_physical_flight(flight);
        }
        self.firing.salvos.push(LiveSalvo {
            at,
            owner,
            left: count,
            next: duration,
            every,
            spread,
        });
        self.world.game.units.get_mut(&id).unwrap().shot_at = Some(self.world.game.tick as f64);
        self.firing.events.push(LaunchEvent::Salvo {
            shooter: id,
            owner,
            at,
            count,
            spread,
        });
        Ok((first..end).collect())
    }
    /// Read-only linked-ticket admission without a timer write or effect emission.
    pub(crate) fn validate_firing_grenade_step(&self, dt: f64) -> Result<(), String> {
        self.supported_firing_mode()?;
        if !dt.is_finite()
            || !(0.0..=640.0).contains(&dt)
            || self.firing.grenades.iter().any(|n| {
                !n.remaining.is_finite()
                    || !(0.0..=640.0).contains(&n.remaining)
                    || ![n.at.x, n.at.z].iter().all(|v| v.is_finite())
                    || crate::match_actor::checked_game_owner(&self.world.game, n.owner).is_err()
                    || n.flight == 0
            })
        {
            return Err("invalid grenade timer step".into());
        }
        Ok(())
    }

    /// Run after step_firing_flights, before the salvo timer phase. Linked tickets
    /// expire/prune with source timing and never apply a second grenade blast.
    pub fn advance_firing_grenades(&mut self, dt: f64) -> Result<(), String> {
        self.validate_firing_grenade_step(dt)?;
        for n in &mut self.firing.grenades {
            n.remaining -= dt;
        }
        self.firing
            .grenades
            .retain(|n| n.remaining > 0.0 && self.firing.flights.contains_key(&n.flight));
        Ok(())
    }
}

#[cfg(test)]
#[path = "world_ground_actor_tests.rs"]
mod world_ground_actor_tests;

#[cfg(test)]
#[path = "mov_audit_reaction_tests.rs"]
mod mov_audit;

#[path = "match_firing_geometry.rs"]
mod geometry;

#[cfg(test)]
#[path = "runtime_combat_batch3.rs"]
mod runtime_batch3;

#[cfg(test)]
#[path = "audit_combat_damage.rs"]
mod audit_damage;
