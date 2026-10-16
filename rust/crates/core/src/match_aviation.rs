//! On-map aircraft runtime. Game owns pose/state, firing owns weapon timers,
//! Combat owns HP attribution/XP and Logistics owns physical ground AA rounds.
use crate::{
    catalog::{Catalog, Value},
    game::{PlaneState, Point},
    grid::UnitId,
    match_firing::FiringUnit,
    match_runtime::Match,
    match_story::{StoryChange, StoryKey, prepare_story_updates},
    math,
};
use std::collections::BTreeMap;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MissionKind {
    Patrol,
    Escort,
    Attack,
    Area,
}
impl MissionKind {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Patrol => "patrol",
            Self::Escort => "escort",
            Self::Attack => "attack",
            Self::Area => "area",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlaneMission {
    pub kind: MissionKind,
    pub target: Option<UnitId>,
    pub at: Point,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AircraftRuntime {
    pub fuel: f64,
    pub ammo: f64,
    pub timer: f64,
    pub orbit: f64,
    pub mission: Option<PlaneMission>,
    /// Cached automatic choice, separate from this step's visible firing target.
    pub attack_pick: UnitId,
    pub downed: bool,
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AntiAirRuntime {
    pub shot: Option<f64>,
    pub reserve: Option<f64>,
}
#[derive(Clone, Debug, PartialEq)]
pub enum AviationEvent {
    AntiAir {
        shooter: UnitId,
        target: UnitId,
        owner: usize,
        target_owner: usize,
        at: Point,
    },
    PlaneDown {
        target: UnitId,
        owner: usize,
        at: Point,
        direction: f64,
    },
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AviationState {
    pub planes: BTreeMap<UnitId, AircraftRuntime>,
    pub anti_air: BTreeMap<UnitId, AntiAirRuntime>,
    pub events: Vec<AviationEvent>,
}
impl AviationState {
    pub fn remove(&mut self, id: UnitId) {
        self.planes.remove(&id);
        self.anti_air.remove(&id);
    }
    pub fn take_events(&mut self) -> Vec<AviationEvent> {
        std::mem::take(&mut self.events)
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AviationError {
    Invalid(&'static str),
    Unsupported(&'static str),
    Actuation(String),
}
impl std::fmt::Display for AviationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(s) | Self::Unsupported(s) => f.write_str(s),
            Self::Actuation(s) => f.write_str(s),
        }
    }
}
impl std::error::Error for AviationError {}
fn catalog() -> Result<&'static Catalog, AviationError> {
    Catalog::compiled().map_err(|e| AviationError::Actuation(e.to_string()))
}
fn number(v: &Value, key: &str) -> Result<f64, AviationError> {
    v.get(key)
        .and_then(Value::number)
        .filter(|v| v.is_finite())
        .ok_or(AviationError::Invalid("invalid aircraft rule"))
}
fn aircraft_rules() -> Result<&'static Value, AviationError> {
    catalog()?
        .table("CFG")
        .map_err(|e| AviationError::Actuation(e.to_string()))?
        .get("air")
        .ok_or(AviationError::Invalid("missing aircraft rules"))
}
fn airborne(state: PlaneState) -> bool {
    matches!(
        state,
        PlaneState::Out | PlaneState::Station | PlaneState::Home
    )
}
fn finite(point: Point) -> bool {
    point.x.is_finite() && point.z.is_finite()
}
struct AircraftStep {
    at: Point,
    rotation: f64,
    hp: f64,
    state: PlaneState,
    runtime: AircraftRuntime,
    firing: FiringUnit,
}
struct AntiAirStep {
    hp: BTreeMap<UnitId, f64>,
    xp: BTreeMap<UnitId, f64>,
    ammo: BTreeMap<UnitId, f64>,
    cadence: BTreeMap<UnitId, AntiAirRuntime>,
    attribution: BTreeMap<UnitId, i32>,
    down: BTreeMap<UnitId, bool>,
    events: Vec<AviationEvent>,
    tallies: Vec<StoryChange>,
}
impl Match {
    pub(crate) fn validate_aviation(&self) -> Result<(), AviationError> {
        self.validate_aviation_context(false)
    }
    /// Ground combat and retirement may observe the planes spawned by an
    /// explicit Horde wave. This does not admit aircraft actuation or full ticks.
    pub(crate) fn validate_aviation_observation(&self) -> Result<(), AviationError> {
        self.validate_aviation_context(true)
    }
    fn validate_aviation_context(&self, horde_spawn: bool) -> Result<(), AviationError> {
        let g = &self.world.game;
        if !(crate::match_actor::ordinary_ground_mode(g, self.options.mode)
            || horde_spawn
                && self.options.mode == crate::economy::Mode::Horde
                && g.mode == Some(crate::game::ModeKind::Horde)
                && self.start.shared_defenders.is_some())
        {
            return Err(AviationError::Unsupported(
                "aviation requires a matching ordinary mode",
            ));
        }
        if g.tick < 0
            || g.tick as u64 > 9_007_199_254_740_941
            || g.players.len() != self.start.players.len()
            || g.players.len() != self.start.economy.players.len()
            || g.players.len() != self.story.len()
        {
            return Err(AviationError::Invalid("invalid aviation match ownership"));
        }
        for (&id, u) in &g.units {
            if id != u.id
                || u.owner >= g.players.len()
                || u.kind >= self.rules.kinds.len()
                || ![u.x, u.z, u.rot, u.hp, u.supp]
                    .iter()
                    .all(|v| v.is_finite())
                || u.still.is_some_and(|v| !v.is_finite())
                || u.built.is_some_and(|v| !v.is_finite())
            {
                return Err(AviationError::Invalid("invalid aviation unit"));
            }
            let c = self
                .combat
                .get(id)
                .ok_or(AviationError::Invalid("missing combat metadata"))?;
            let f = self
                .firing
                .get(id)
                .ok_or(AviationError::Invalid("missing firing metadata"))?;
            if !c.xp.is_finite()
                || c.xp < 0.0
                || ![f.cooldown, f.retarget].iter().all(|v| v.is_finite())
                || !self.commands.units.contains_key(&id)
            {
                return Err(AviationError::Invalid("invalid aviation metadata"));
            }
            if let Some(r) = self.logistics.get(id)
                && (!r.ammo.is_finite()
                    || r.ammo < 0.0
                    || !r.ammo_max.is_finite()
                    || r.ammo_max < 0.0)
            {
                return Err(AviationError::Invalid("invalid AA ammunition"));
            }
            if u.air.is_some() {
                if !self.rules.kinds[u.kind].air {
                    return Err(AviationError::Invalid("aircraft kind/state mismatch"));
                }
                let a = self
                    .aviation
                    .planes
                    .get(&id)
                    .ok_or(AviationError::Unsupported(
                        "query aircraft has no sortie runtime metadata",
                    ))?;
                if ![a.fuel, a.ammo, a.timer, a.orbit]
                    .iter()
                    .all(|v| v.is_finite())
                    || a.ammo < 0.0
                    || a.mission
                        .is_some_and(|m| !finite(m.at) || m.target == Some(0))
                {
                    return Err(AviationError::Invalid("invalid sortie metadata"));
                }
                if a.mission
                    .is_some_and(|mission| mission.kind == MissionKind::Area)
                    && !catalog()?
                        .unit(&self.rules.kinds[u.kind].name)
                        .map_err(|e| AviationError::Actuation(e.to_string()))?
                        .get("w")
                        .and_then(|w| w.get("salvo"))
                        .and_then(Value::boolean)
                        .unwrap_or(false)
                {
                    return Err(AviationError::Invalid(
                        "area mission requires a bomber salvo",
                    ));
                }
            } else if self.rules.kinds[u.kind].air {
                return Err(AviationError::Invalid("missing aircraft physical state"));
            }
            if self.aviation.anti_air.get(&id).is_some_and(|a| {
                a.shot.is_some_and(|v| !v.is_finite())
                    || a.reserve.is_some_and(|v| !v.is_finite() || v < 0.0)
            }) {
                return Err(AviationError::Invalid("invalid AA cadence"));
            }
        }
        Ok(())
    }
    /// Nearest finished own airfield, otherwise the source off-map base past HQ.
    pub fn aircraft_base(&self, owner: usize, at: Point) -> Result<Point, AviationError> {
        if !(crate::match_actor::ordinary_ground_mode(&self.world.game, self.options.mode)
            || self.options.mode == crate::economy::Mode::Horde
                && self.world.game.mode == Some(crate::game::ModeKind::Horde)
                && self.start.shared_defenders.is_some())
        {
            return Err(AviationError::Unsupported(
                "air base requires a matching ordinary mode",
            ));
        }
        if owner >= self.start.players.len() || !finite(at) {
            return Err(AviationError::Invalid("invalid aircraft base owner/point"));
        }
        let mut best = None;
        let mut distance = f64::INFINITY;
        for (&id, u) in &self.world.game.units {
            let kind = self
                .rules
                .kinds
                .get(u.kind)
                .ok_or(AviationError::Invalid("invalid airfield kind"))?;
            if kind.name != "airfield"
                || u.owner != owner
                || !u.built.is_some_and(|v| v >= 1.0)
                || u.hp <= 0.0
            {
                continue;
            }
            if !finite(u.at()) || !u.hp.is_finite() {
                return Err(AviationError::Invalid("invalid airfield geometry"));
            }
            let d = math::hypot(u.x - at.x, u.z - at.z);
            if d < distance || d == distance && best.is_none_or(|(old, _)| id < old) {
                distance = d;
                best = Some((id, u.at()));
            }
        }
        if let Some((_, at)) = best {
            return Ok(at);
        }
        let p = self.start.players[owner].spawn;
        if !finite(p) {
            return Err(AviationError::Invalid("invalid aircraft home"));
        }
        let dx = p.x - self.world.game.w as f64 * crate::game::CELL / 2.0;
        let dz = p.z - self.world.game.h as f64 * crate::game::CELL / 2.0;
        let mut length = math::hypot(dx, dz);
        if length == 0.0 {
            length = 1.0;
        }
        let offset = number(aircraft_rules()?, "offmap")?;
        let base = Point {
            x: p.x + dx / length * offset,
            z: p.z + dz / length * offset,
        };
        if !finite(base) {
            return Err(AviationError::Invalid("invalid air-base result"));
        }
        Ok(base)
    }
    /// Generic spawn primitive. Purchase admission/payment remains with production.
    pub fn spawn_aircraft(&mut self, owner: usize, name: &str) -> Result<UnitId, AviationError> {
        self.validate_aviation_context(true)?;
        let kind = self
            .rules
            .kind(name)
            .ok_or(AviationError::Invalid("unknown aircraft kind"))?;
        if owner >= self.start.players.len() || !self.rules.kinds[kind].air {
            return Err(AviationError::Invalid("invalid aircraft spawn actor/kind"));
        }
        let id = self.next_unit_id;
        if self.aviation.planes.contains_key(&id)
            || self.aviation.anti_air.contains_key(&id)
            || self.combat.get(id).is_some()
            || self.firing.get(id).is_some()
            || self.abilities.get(id).is_some()
            || self.recovery.get(id).is_some()
        {
            return Err(AviationError::Invalid(
                "occupied aircraft metadata identity",
            ));
        }
        let def = catalog()?
            .unit(name)
            .map_err(|e| AviationError::Actuation(e.to_string()))?;
        let runtime = AircraftRuntime {
            fuel: number(aircraft_rules()?, "station")?,
            ammo: number(def, "ammo")?,
            timer: 0.0,
            orbit: 0.0,
            mission: None,
            attack_pick: 0,
            downed: false,
        };
        let mut placer = crate::unit_spawn::SpawnPlacer::new();
        let index = u32::try_from(self.world.game.units.len())
            .map_err(|_| AviationError::Invalid("aircraft scatter index"))?;
        let scatter = placer
            .scatter_aircraft_position(
                crate::match_runtime::terrain_view(&self.world.game),
                &self.rules.kinds[kind],
                self.start.players[owner].spawn,
                index,
            )
            .map_err(|e| AviationError::Actuation(format!("aircraft scatter: {e:?}")))?
            .point;
        let base = self.aircraft_base(owner, scatter)?;
        let draft = crate::unit_publication::StagedUnit::aircraft(
            &self.world.game,
            &self.rules,
            owner,
            kind,
            id,
            index,
            self.start.players[owner].spawn,
            base,
        )
        .map_err(|e| AviationError::Actuation(format!("aircraft draft: {e:?}")))?;
        let reserves = self
            .prepare_new_unit_reserves(&draft.unit)
            .map_err(AviationError::Actuation)?;
        let publication = crate::unit_publication::prepare_publication(
            crate::unit_publication::PublicationParts {
                units: &mut self.world.game.units,
                grid: &mut self.grid,
                controls: &mut self.commands.units,
                recruitment: &mut self.recruit_metadata,
                next_id: &mut self.next_unit_id,
            },
            &self.rules,
            self.start.players.len(),
            vec![draft],
        )
        .map_err(|e| AviationError::Actuation(format!("aircraft publication: {e:?}")))?;
        publication.publish_with(|u, _| {
            self.aviation.planes.insert(u.id, runtime);
            self.combat.initialize(u.id).hit_by = Some(0);
            self.firing.initialize(u.id);
            self.abilities.initialize(u.id);
            self.recovery.initialize(u.id);
            if let Some(r) = reserves.clone() {
                self.logistics.insert(u.id, r);
            }
        });
        Ok(id)
    }
    /// Pure admission for whole-selection command preflight.
    pub(crate) fn validate_aircraft_mission(
        &self,
        owner: usize,
        id: UnitId,
        mission: PlaneMission,
    ) -> Result<bool, AviationError> {
        self.validate_aviation()?;
        if !finite(mission.at) || mission.target == Some(0) {
            return Err(AviationError::Invalid("invalid plane mission"));
        }
        let Some(u) = self.world.game.units.get(&id) else {
            return Ok(false);
        };
        if u.owner != owner
            || u.air.is_none()
            || self.finish.is_some()
            || self.start.economy.players[owner].out
            || self.start.economy.players[owner].away
        {
            return Ok(false);
        }
        let a = self.aviation.planes[&id];
        if u.air == Some(PlaneState::Rearm) || a.fuel <= 0.0 || a.ammo <= 0.0 {
            return Ok(false);
        }
        if mission.kind == MissionKind::Area {
            let def = catalog()?
                .unit(&self.rules.kinds[u.kind].name)
                .map_err(|e| AviationError::Actuation(e.to_string()))?;
            if !def
                .get("w")
                .and_then(|w| w.get("salvo"))
                .and_then(Value::boolean)
                .unwrap_or(false)
            {
                return Err(AviationError::Invalid(
                    "area mission requires a bomber salvo",
                ));
            }
        }
        if mission.kind == MissionKind::Attack {
            let target = mission
                .target
                .and_then(|target| self.world.game.units.get(&target));
            if target.is_none_or(|t| {
                t.hp <= 0.0
                    || self.world.game.allied(owner, t.owner)
                    || !self.world.game.players[owner].sees(t.id)
            }) {
                return Err(AviationError::Invalid("invalid aircraft attack target"));
            }
        }
        if mission.kind == MissionKind::Escort {
            let target = mission
                .target
                .and_then(|target| self.world.game.units.get(&target));
            if target.is_none_or(|t| {
                t.air.is_some()
                    || self.rules.kinds[t.kind].structure
                    || !self.world.game.allied(owner, t.owner)
            }) {
                return Ok(false);
            }
        }
        Ok(true)
    }
    /// The single recorded command dispatcher remains the caller.
    pub fn send_aircraft_mission(
        &mut self,
        owner: usize,
        id: UnitId,
        mission: PlaneMission,
    ) -> Result<bool, AviationError> {
        if !self.validate_aircraft_mission(owner, id, mission)? {
            return Ok(false);
        }
        self.aviation.planes.get_mut(&id).unwrap().mission = Some(mission);
        self.world.game.units.get_mut(&id).unwrap().air = Some(PlaneState::Out);
        Ok(true)
    }
    pub fn return_aircraft_home(
        &mut self,
        owner: usize,
        id: UnitId,
    ) -> Result<bool, AviationError> {
        self.validate_aviation()?;
        let Some(u) = self.world.game.units.get(&id) else {
            return Ok(false);
        };
        if u.owner != owner || u.air.is_none() {
            return Ok(false);
        }
        let active = airborne(u.air.unwrap());
        if active {
            self.world.game.units.get_mut(&id).unwrap().air = Some(PlaneState::Home);
        }
        Ok(active)
    }
    fn preview_aircraft_step(&self, id: UnitId, dt: f64) -> Result<AircraftStep, AviationError> {
        self.validate_aviation()?;
        if !dt.is_finite() || !(0.0..=3600.0).contains(&dt) {
            return Err(AviationError::Invalid("invalid aircraft dt"));
        }
        let u = self
            .world
            .game
            .units
            .get(&id)
            .ok_or(AviationError::Invalid("missing aircraft"))?;
        let mut state = u
            .air
            .ok_or(AviationError::Invalid("unit is not aircraft"))?;
        let mut a = self.aviation.planes[&id];
        let mut firing = *self.firing.get(id).unwrap();
        let mut at = u.at();
        let mut rotation = u.rot;
        let mut hp = u.hp;
        if hp <= 0.0 {
            return Ok(AircraftStep {
                at,
                rotation,
                hp,
                state,
                runtime: a,
                firing,
            });
        }
        let def = catalog()?
            .unit(&self.rules.kinds[u.kind].name)
            .map_err(|e| AviationError::Actuation(e.to_string()))?;
        let rules = aircraft_rules()?;
        let base = self.aircraft_base(u.owner, at)?;
        firing.cooldown -= dt;
        firing.retarget -= dt;
        firing.target_id = 0;
        if matches!(state, PlaneState::Base | PlaneState::Rearm) {
            at = base;
            if state == PlaneState::Rearm {
                hp = math::min(
                    number(def, "hpPer")?,
                    hp + number(def, "hpPer")? * dt / number(rules, "rearm")?,
                );
                a.timer -= dt;
                if a.timer <= 0.0 {
                    state = PlaneState::Base;
                    a.fuel = number(rules, "station")?;
                    a.ammo = number(def, "ammo")?;
                }
            }
        } else {
            if let Some(mut mission) = a.mission {
                if let Some(target) = mission.target {
                    if let Some(t) = self.world.game.units.get(&target).filter(|t| {
                        t.hp > 0.0
                            && (mission.kind == MissionKind::Escort
                                || self.world.game.players[u.owner].sees(t.id))
                    }) {
                        mission.at = t.at();
                    } else {
                        mission.kind = MissionKind::Patrol;
                        mission.target = None;
                    }
                }
                a.mission = Some(mission);
            }
            if state == PlaneState::Station {
                a.fuel -= dt;
            }
            if state != PlaneState::Home
                && (a.fuel <= 0.0
                    || a.ammo <= 0.0
                    || hp < number(def, "hpPer")? * number(rules, "bail")?
                    || a.mission.is_none())
            {
                state = PlaneState::Home;
            }
            let mission_center = a.mission.map_or(base, |m| m.at);
            let mut center = mission_center;
            if def.get("role").and_then(Value::string) == Some("fighter")
                && !self.commands.units[&id].hold_fire
                && state != PlaneState::Home
            {
                let mut nearest = number(rules, "seek")?;
                let mut chosen = None;
                for (&other_id, other) in &self.world.game.units {
                    if !other.air.is_some_and(airborne)
                        || other.hp <= 0.0
                        || self.world.game.allied(u.owner, other.owner)
                        || !self.world.game.players[u.owner].sees(other_id)
                    {
                        continue;
                    }
                    let distance =
                        math::hypot(other.x - mission_center.x, other.z - mission_center.z);
                    if distance < nearest {
                        nearest = distance;
                        chosen = Some(other.at());
                    }
                }
                if let Some(at) = chosen {
                    center = at;
                }
            }
            let radius = if a
                .mission
                .is_some_and(|m| matches!(m.kind, MissionKind::Attack | MissionKind::Area))
                && def.get("role").and_then(Value::string) == Some("attack")
            {
                10.0
            } else {
                number(rules, "orbit")?
            };
            let mut goal = base;
            if state == PlaneState::Out {
                goal = center;
                if math::hypot(at.x - center.x, at.z - center.z) < radius + 6.0 {
                    state = PlaneState::Station;
                }
            }
            if state == PlaneState::Station {
                a.orbit += number(def, "speed")? / radius * dt;
                goal = Point {
                    x: center.x + math::cos(a.orbit) * radius,
                    z: center.z + math::sin(a.orbit) * radius,
                };
            }
            let distance = math::hypot(at.x - goal.x, at.z - goal.z);
            let travel = math::min(distance, number(def, "speed")? * dt);
            if distance > 0.01 {
                rotation = math::atan2(goal.z - at.z, goal.x - at.x);
                at.x += (goal.x - at.x) / distance * travel;
                at.z += (goal.z - at.z) / distance * travel;
            }
            if state == PlaneState::Home && math::hypot(at.x - base.x, at.z - base.z) < 6.0 {
                state = PlaneState::Rearm;
                a.timer = number(rules, "rearm")?;
                a.mission = None;
            }
        }
        if ![
            at.x,
            at.z,
            rotation,
            hp,
            a.fuel,
            a.ammo,
            a.timer,
            a.orbit,
            firing.cooldown,
            firing.retarget,
        ]
        .iter()
        .all(|v| v.is_finite())
        {
            return Err(AviationError::Invalid("non-finite aircraft step"));
        }
        Ok(AircraftStep {
            at,
            rotation,
            hp,
            state,
            runtime: a,
            firing,
        })
    }
    /// Pure preflight is available to the tick owner before admitting a whole phase.
    pub fn validate_aircraft_step(&self, id: UnitId, dt: f64) -> Result<(), AviationError> {
        self.preview_aircraft_step(id, dt).map(|_| ())
    }
    pub fn step_aircraft(&mut self, id: UnitId, dt: f64) -> Result<(), AviationError> {
        let next = self.preview_aircraft_step(id, dt)?;
        let u = self.world.game.units.get_mut(&id).unwrap();
        u.x = next.at.x;
        u.z = next.at.z;
        u.rot = next.rotation;
        u.hp = next.hp;
        u.air = Some(next.state);
        self.aviation.planes.insert(id, next.runtime);
        self.firing.units.insert(id, next.firing);
        self.grid.update(id, &self.world.game.units[&id]);
        self.step_aircraft_weapons(id)?;
        Ok(())
    }
    fn prepare_aircraft_anti_air(
        &self,
        actors: &[UnitId],
        dt: f64,
    ) -> Result<AntiAirStep, AviationError> {
        self.validate_aviation()?;
        if !dt.is_finite()
            || !(0.0..=3600.0).contains(&dt)
            || actors.windows(2).any(|ids| ids[0] >= ids[1])
            || actors
                .iter()
                .any(|id| !self.world.game.units.contains_key(id))
        {
            return Err(AviationError::Invalid("invalid AA actor order/dt"));
        }
        let g = &self.world.game;
        let mut hp = BTreeMap::new();
        let mut xp = BTreeMap::new();
        let mut ammo = BTreeMap::new();
        let mut cadence = BTreeMap::new();
        let mut attribution = BTreeMap::new();
        let mut down = BTreeMap::new();
        let mut events = Vec::new();
        let mut tallies = Vec::new();
        for &id in actors {
            let u = &g.units[&id];
            let def = catalog()?
                .unit(&self.rules.kinds[u.kind].name)
                .map_err(|e| AviationError::Actuation(e.to_string()))?;
            let Some(aa) = def.get("aa") else {
                continue;
            };
            let Some(dps) = aa.get("dps").and_then(Value::number).filter(|v| *v > 0.0) else {
                continue;
            };
            if hp.get(&id).copied().unwrap_or(u.hp) <= 0.0
                || aa.get("setup").and_then(Value::boolean).unwrap_or(false)
                    && u.still.is_some_and(|v| v < 2.0)
                || u.air.is_some_and(|state| !airborne(state))
                || u.built.is_some_and(|v| v < 1.0)
            {
                continue;
            }
            let hold = self.commands.units[&id].hold_fire;
            let mission = self.aviation.planes.get(&id).and_then(|a| a.mission);
            if hold && (u.air.is_none() || !mission.is_some_and(|m| m.kind == MissionKind::Attack))
            {
                continue;
            }
            let range = number(aa, "range")?;
            let mut best = None;
            let mut distance = range;
            for target in self
                .grid
                .in_order(&g.units, (u.x, u.z), range, |target, t| {
                    t.air.is_some_and(airborne)
                        && hp.get(&target).copied().unwrap_or(t.hp) > 0.0
                        && !g.allied(u.owner, t.owner)
                        && g.players[u.owner].sees(target)
                        && (!hold || mission.is_some_and(|m| m.target == Some(target)))
                })
            {
                let t = &g.units[&target];
                let d = math::hypot(u.x - t.x, u.z - t.z);
                if d <= distance {
                    distance = d;
                    best = Some(target);
                }
            }
            let Some(target) = best else {
                continue;
            };
            let reserves = self.logistics.get(id);
            let stock = reserves.map(|r| ammo.get(&id).copied().unwrap_or(r.ammo));
            if stock.is_some_and(|stock| stock < 1.0)
                || reserves.is_some_and(|r| r.forced && !r.stranded)
            {
                continue;
            }
            let rate = 1.0 / self.logistics.shortage_rate(id);
            let base = dps * dt;
            let damage = base * rate;
            let mut a = *self
                .aviation
                .anti_air
                .get(&id)
                .unwrap_or(&AntiAirRuntime::default());
            if let Some(stock) = stock {
                let interval = def
                    .get("w")
                    .ok_or(AviationError::Invalid("AA reserves require weapon"))?;
                let mut credit = a.reserve.unwrap_or(0.0)
                    + dt * rate / math::max(0.1, number(interval, "interval")?);
                if credit >= 1.0 {
                    let spent = math::min(credit.floor(), stock.floor());
                    ammo.insert(id, stock - spent);
                    credit %= 1.0;
                }
                a.reserve = Some(credit);
            }
            let t = &g.units[&target];
            let remaining = hp.get(&target).copied().unwrap_or(t.hp) - damage;
            hp.insert(target, remaining);
            attribution.insert(target, u.owner as i32);
            let experience = xp
                .get(&id)
                .copied()
                .unwrap_or(self.combat.get(id).unwrap().xp)
                + math::min(base, math::max(0.0, remaining + base));
            xp.insert(id, experience);
            let shot = a.shot.unwrap_or(0.0) - dt;
            a.shot = Some(shot);
            if shot <= 0.0 {
                a.shot = Some(0.35);
                events.push(AviationEvent::AntiAir {
                    shooter: id,
                    target,
                    owner: u.owner,
                    target_owner: t.owner,
                    at: t.at(),
                });
            }
            if remaining <= 0.0
                && !down
                    .get(&target)
                    .copied()
                    .unwrap_or(self.aviation.planes[&target].downed)
            {
                down.insert(target, true);
                tallies.push(StoryChange {
                    slot: Some(u.owner),
                    key: StoryKey::PlanesDowned,
                    amount: 1.0,
                });
                events.push(AviationEvent::PlaneDown {
                    target,
                    owner: t.owner,
                    at: t.at(),
                    direction: t.rot,
                });
            }
            if !remaining.is_finite()
                || !experience.is_finite()
                || a.shot.is_some_and(|v| !v.is_finite())
                || a.reserve.is_some_and(|v| !v.is_finite())
            {
                return Err(AviationError::Invalid("non-finite AA result"));
            }
            cadence.insert(id, a);
        }
        Ok(AntiAirStep {
            hp,
            xp,
            ammo,
            cadence,
            attribution,
            down,
            events,
            tallies,
        })
    }
    /// Pure admission of the actual current-grid AA arithmetic, including staged dead shooters.
    pub fn validate_aircraft_anti_air(
        &self,
        actors: &[UnitId],
        dt: f64,
    ) -> Result<(), AviationError> {
        let next = self.prepare_aircraft_anti_air(actors, dt)?;
        let mut story = self.story.clone();
        prepare_story_updates(&mut story, self.finish.is_some(), &next.tallies)
            .map_err(AviationError::Actuation)?;
        Ok(())
    }
    /// Captured shooters choose fresh current airborne targets in maintained grid order.
    pub fn step_aircraft_anti_air(
        &mut self,
        actors: &[UnitId],
        dt: f64,
    ) -> Result<(), AviationError> {
        let AntiAirStep {
            hp,
            xp,
            ammo,
            cadence,
            attribution,
            down,
            events,
            tallies,
        } = self.prepare_aircraft_anti_air(actors, dt)?;
        let story = prepare_story_updates(&mut self.story, self.finish.is_some(), &tallies)
            .map_err(AviationError::Actuation)?;
        for (id, value) in hp {
            self.world.game.units.get_mut(&id).unwrap().hp = value;
        }
        for (id, owner) in attribution {
            self.combat.get_mut(id).unwrap().last_hit = Some(owner);
        }
        for (id, value) in xp {
            self.combat.get_mut(id).unwrap().xp = value;
        }
        for (id, value) in ammo {
            self.logistics.get_mut(id).unwrap().ammo = value;
        }
        for (id, value) in cadence {
            self.aviation.anti_air.insert(id, value);
        }
        for (id, value) in down {
            self.aviation.planes.get_mut(&id).unwrap().downed = value;
        }
        self.aviation.events.extend(events);
        story.commit();
        Ok(())
    }
}

#[cfg(test)]
mod intent_tests {
    use super::*;
    use crate::{
        economy::{Faction, Mode},
        economy_state::ArmySize,
        map::AuthoritativeMap,
        match_commands::Command,
        match_runtime::{MatchOptions, PlayerConfig},
        weather::WeatherChoice,
    };
    use serde_json::json;

    fn scene() -> Match {
        scene_with_seed(318)
    }

    fn scene_with_seed(seed: u64) -> Match {
        let map = AuthoritativeMap::from_json(
            &serde_json::to_vec(&json!({
                "name":"Aircraft intent", "w":64, "h":64,
                "rows":vec![".".repeat(64);64], "heights":vec!["0".repeat(64);64],
                "spawns":[{"x":8,"y":8},{"x":48,"y":48}],
                "points":[{"x":32,"y":32,"vp":0,"mp":0}]
            }))
            .unwrap(),
        )
        .unwrap();
        Match::new(
            &map,
            MatchOptions {
                seed,
                mode: Mode::Conquest,
                players: vec![
                    PlayerConfig {
                        name: "A".into(),
                        team: 0,
                        faction: Faction::Usa,
                    },
                    PlayerConfig {
                        name: "B".into(),
                        team: 1,
                        faction: Faction::Germany,
                    },
                ],
                shuffle: false,
                army: ArmySize::Endless,
                weather: WeatherChoice::Map,
                weather_seed: Some(9),
                living_weather: false,
                map_key: None,
                tech: false,
                supply: true,
                logistics: false,
                sandbox: false,
            },
        )
        .unwrap()
    }

    #[test]
    fn mov85_sortie_flies_stations_returns_and_rearms() {
        let mut m = scene();
        let id = m.spawn_aircraft(0, "fighter").unwrap();
        m.commands.units.get_mut(&id).unwrap().hold_fire = true;
        assert_eq!(m.world.game.units[&id].air, Some(PlaneState::Base));
        let at = Point { x: 76.0, z: 76.0 };
        assert!(
            m.send_aircraft_mission(
                0,
                id,
                PlaneMission {
                    kind: MissionKind::Patrol,
                    target: None,
                    at,
                }
            )
            .unwrap()
        );
        let mut station = false;
        let mut home = false;
        let mut rearm = false;
        for _ in 0..2000 {
            m.step_aircraft(id, 0.05).unwrap();
            match m.world.game.units[&id].air.unwrap() {
                PlaneState::Station => station = true,
                PlaneState::Home => home = true,
                PlaneState::Rearm => rearm = true,
                PlaneState::Base if rearm => break,
                _ => {}
            }
        }
        assert!(station && home && rearm);
        assert_eq!(m.world.game.units[&id].air, Some(PlaneState::Base));
        assert_eq!(m.aviation.planes[&id].mission, None);
    }

    #[test]
    fn mov87_fighter_seeks_nearest_visible_enemy_near_mission() {
        let mut m = scene();
        let fighter = m.spawn_aircraft(0, "fighter").unwrap();
        let near = m.spawn_aircraft(1, "fighter").unwrap();
        let far = m.spawn_aircraft(1, "fighter").unwrap();
        let hidden = m.spawn_aircraft(1, "fighter").unwrap();
        for (id, at) in [
            (fighter, Point { x: 60.0, z: 60.0 }),
            (near, Point { x: 80.0, z: 60.0 }),
            (far, Point { x: 95.0, z: 100.0 }),
            (hidden, Point { x: 80.0, z: 78.0 }),
        ] {
            let u = m.world.game.units.get_mut(&id).unwrap();
            u.x = at.x;
            u.z = at.z;
            u.air = Some(PlaneState::Out);
            m.grid.update(id, u);
        }
        m.world.game.players[0].visible = vec![near, far];
        m.aviation.planes.get_mut(&fighter).unwrap().mission = Some(PlaneMission {
            kind: MissionKind::Patrol,
            target: None,
            at: Point { x: 80.0, z: 80.0 },
        });
        m.commands.units.get_mut(&fighter).unwrap().hold_fire = false;
        m.step_aircraft(fighter, 0.05).unwrap();
        let to_near = math::hypot(
            m.world.game.units[&fighter].x - 80.0,
            m.world.game.units[&fighter].z - 60.0,
        );
        assert!(to_near < 20.0);
        assert!(m.world.game.units[&fighter].z < 60.1);
    }

    #[test]
    fn mov89_visible_aircraft_takes_continuous_anti_air_damage() {
        let mut m = scene();
        let shooter = m.spawn_aircraft(0, "fighter").unwrap();
        let target = m.spawn_aircraft(1, "fighter").unwrap();
        for (id, x) in [(shooter, 70.0), (target, 75.0)] {
            let u = m.world.game.units.get_mut(&id).unwrap();
            u.x = x;
            u.z = 70.0;
            u.air = Some(PlaneState::Station);
            m.grid.update(id, u);
        }
        let hp = m.world.game.units[&target].hp;
        m.step_aircraft_anti_air(&[shooter, target], 0.05).unwrap();
        assert_eq!(m.world.game.units[&target].hp, hp, "unseen plane is safe");
        m.world.game.players[0].visible = vec![target];
        m.commands.units.get_mut(&shooter).unwrap().hold_fire = true;
        m.step_aircraft_anti_air(&[shooter, target], 0.05).unwrap();
        assert_eq!(
            m.world.game.units[&target].hp, hp,
            "hold fire suppresses free AA"
        );
        m.commands.units.get_mut(&shooter).unwrap().hold_fire = false;
        m.step_aircraft_anti_air(&[shooter, target], 0.05).unwrap();
        assert!(m.world.game.units[&target].hp < hp);
        assert!(
            m.aviation.events.iter().any(|event| matches!(event,
            AviationEvent::AntiAir { shooter: s, target: t, .. } if *s == shooter && *t == target))
        );
    }

    #[test]
    fn mov88_attack_plane_fires_at_visible_ground_target_and_spends_sortie_ammo() {
        let mut m = scene();
        let attacker = m.spawn_aircraft(0, "attacker").unwrap();
        let target = m
            .world
            .game
            .units
            .values()
            .find(|u| u.owner == 1 && !m.rules.kinds[u.kind].structure)
            .unwrap()
            .id;
        {
            let u = m.world.game.units.get_mut(&attacker).unwrap();
            u.x = 60.0;
            u.z = 70.0;
            u.air = Some(PlaneState::Station);
            m.grid.update(attacker, u);
        }
        {
            let u = m.world.game.units.get_mut(&target).unwrap();
            u.x = 70.0;
            u.z = 70.0;
            m.grid.update(target, u);
        }
        m.world.game.players[0].visible = vec![target];
        m.aviation.planes.get_mut(&attacker).unwrap().mission = Some(PlaneMission {
            kind: MissionKind::Attack,
            target: Some(target),
            at: Point { x: 70.0, z: 70.0 },
        });
        let ammo = m.aviation.planes[&attacker].ammo;
        m.step_aircraft(attacker, 0.05).unwrap();
        assert!(
            m.firing
                .flights
                .values()
                .any(|flight| flight.shooter == attacker && flight.intended == Some(target))
        );
        assert_eq!(m.aviation.planes[&attacker].ammo, ammo - 1.0);
        let flights = m.firing.flights.len();
        m.step_aircraft(attacker, 0.05).unwrap();
        assert_eq!(
            m.firing.flights.len(),
            flights,
            "weapon cooldown limits launches"
        );
    }

    #[test]
    fn off_map_attack_aircraft_launches_and_hits_through_the_match_runner() {
        use crate::{match_firing::LaunchEvent, match_tick::TickEvent};
        // This seed's first accuracy roll hits, isolating the off-map contact path.
        let mut m = scene_with_seed(5);
        let attacker = m.spawn_aircraft(0, "attacker").unwrap();
        let target = m
            .world
            .game
            .units
            .values()
            .find(|u| u.owner == 1 && !m.rules.kinds[u.kind].structure)
            .unwrap()
            .id;
        for unit in m.world.game.units.values_mut() {
            unit.hold_pos = true;
            let control = m.commands.units.get_mut(&unit.id).unwrap();
            control.hold_fire = true;
            control.auto_cast = false;
            control.auto_retreat = false;
        }
        {
            let u = m.world.game.units.get_mut(&attacker).unwrap();
            u.x = -6.0;
            u.z = 70.0;
            u.air = Some(PlaneState::Out);
        }
        {
            let u = m.world.game.units.get_mut(&target).unwrap();
            u.x = 8.0;
            u.z = 70.0;
        }
        m.grid.rebuild(&m.world.game.units);
        m.world.game.players[0].visible = vec![target];
        m.aviation.planes.get_mut(&attacker).unwrap().mission = Some(PlaneMission {
            kind: MissionKind::Attack,
            target: Some(target),
            at: Point { x: 8.0, z: 70.0 },
        });
        let hp = m.world.game.units[&target].hp;
        let mut off_map_flights = Vec::new();
        let mut hit = false;
        let mut contacts = Vec::new();
        for tick in 1..=30 {
            let report = m
                .advance_tick()
                .unwrap_or_else(|e| panic!("tick {tick}: {e}"));
            for event in report.events {
                match event {
                    TickEvent::Firing(LaunchEvent::Flight {
                        id, shooter, at, ..
                    }) if shooter == attacker && at.x < 0.0 => {
                        off_map_flights.push(id);
                    }
                    TickEvent::Firing(LaunchEvent::Contact(contact))
                        if contact.shooter == attacker =>
                    {
                        hit |= off_map_flights.contains(&contact.flight)
                            && contact.target == Some(target)
                            && contact.hit;
                        contacts.push(contact);
                    }
                    _ => {}
                }
            }
            if hit {
                break;
            }
        }
        assert!(
            !off_map_flights.is_empty(),
            "the launch retains its true off-map origin"
        );
        assert!(
            hit,
            "the off-map projectile reaches the ground target: {contacts:?}"
        );
        assert!(m.world.game.units[&target].hp < hp);
        assert!(m.tick_state.fault().is_none());
    }

    #[test]
    fn mov88_bomber_area_order_launches_one_salvo_stick_on_station() {
        let mut m = scene();
        let bomber = m.spawn_aircraft(0, "bomber").unwrap();
        {
            let u = m.world.game.units.get_mut(&bomber).unwrap();
            u.x = 60.0;
            u.z = 70.0;
            u.air = Some(PlaneState::Station);
            m.grid.update(bomber, u);
        }
        m.aviation.planes.get_mut(&bomber).unwrap().mission = Some(PlaneMission {
            kind: MissionKind::Area,
            target: None,
            at: Point { x: 70.0, z: 70.0 },
        });
        let ammo = m.aviation.planes[&bomber].ammo;
        m.step_aircraft(bomber, 0.05).unwrap();
        assert_eq!(m.aviation.planes[&bomber].ammo, ammo - 1.0);
        assert_eq!(m.firing.salvos.len(), 1);
        assert_eq!(
            m.firing
                .flights
                .values()
                .filter(|f| f.shooter == bomber)
                .count(),
            4
        );
        m.step_aircraft(bomber, 0.05).unwrap();
        assert_eq!(
            m.firing.salvos.len(),
            1,
            "second stick waits for its interval"
        );
    }

    #[test]
    fn mov88_sortie_projectile_damages_ground_target_in_normal_ticks() {
        let mut m = scene();
        let attacker = m.spawn_aircraft(0, "attacker").unwrap();
        let target = m
            .world
            .game
            .units
            .values()
            .find(|u| u.owner == 1 && !m.rules.kinds[u.kind].structure)
            .unwrap()
            .id;
        {
            let u = m.world.game.units.get_mut(&attacker).unwrap();
            u.x = 60.0;
            u.z = 70.0;
            u.air = Some(PlaneState::Station);
            m.grid.update(attacker, u);
        }
        {
            let u = m.world.game.units.get_mut(&target).unwrap();
            u.x = 70.0;
            u.z = 70.0;
            m.grid.update(target, u);
        }
        m.world.game.players[0].visible = vec![target];
        m.aviation.planes.get_mut(&attacker).unwrap().mission = Some(PlaneMission {
            kind: MissionKind::Attack,
            target: Some(target),
            at: Point { x: 70.0, z: 70.0 },
        });
        let hp = m.world.game.units[&target].hp;
        let mut launched = false;
        let mut damaged = false;
        for _ in 0..400 {
            let report = m.advance_tick().unwrap();
            launched |= report.events.iter().any(|event| {
                matches!(event,
                crate::match_tick::TickEvent::Firing(
                    crate::match_firing::LaunchEvent::Flight { shooter, .. }
                ) if *shooter == attacker)
            });
            damaged |= m.world.game.units.get(&target).is_none_or(|u| u.hp < hp);
            if damaged {
                break;
            }
        }
        assert!(
            launched,
            "sortie fires a physical shot during the normal tick"
        );
        assert!(
            damaged,
            "the physical shot reaches and harms its ground target"
        );
    }

    #[test]
    fn mov86_recorded_aircraft_orders_change_missions_and_respect_readiness() {
        let mut m = scene();
        let plane = m.spawn_aircraft(0, "attacker").unwrap();
        let friendly = m
            .world
            .game
            .units
            .values()
            .find(|u| u.owner == 0 && !m.rules.kinds[u.kind].structure)
            .unwrap()
            .id;
        let enemy = m
            .world
            .game
            .units
            .values()
            .find(|u| u.owner == 1 && !m.rules.kinds[u.kind].structure)
            .unwrap()
            .id;
        m.world.game.players[0].visible = vec![enemy];
        fn issue(m: &mut Match, raw: serde_json::Value) {
            let command = Command::from_json(&raw).unwrap();
            assert_eq!(m.execute_command(0, &command, false), Ok(None), "{raw}");
        }
        issue(
            &mut m,
            json!({"t":"stance","ids":[plane],"key":"holdFire","on":false}),
        );
        issue(&mut m, json!({"t":"move","orders":[[plane,74.0,74.0]]}));
        assert_eq!(
            m.aviation.planes[&plane].mission.unwrap().kind,
            MissionKind::Patrol
        );
        issue(&mut m, json!({"t":"attack","ids":[plane],"target":enemy}));
        assert_eq!(
            m.aviation.planes[&plane].mission.unwrap().target,
            Some(enemy)
        );
        issue(
            &mut m,
            json!({"t":"escort","ids":[plane],"target":friendly}),
        );
        assert_eq!(
            m.aviation.planes[&plane].mission.unwrap().kind,
            MissionKind::Escort
        );
        issue(&mut m, json!({"t":"stop","ids":[plane]}));
        assert_eq!(
            m.aviation.planes[&plane].mission.unwrap().at,
            m.world.game.units[&plane].at()
        );
        issue(&mut m, json!({"t":"retreat","ids":[plane]}));
        assert_eq!(m.world.game.units[&plane].air, Some(PlaneState::Home));
        let bomber = m.spawn_aircraft(0, "bomber").unwrap();
        issue(
            &mut m,
            json!({"t":"fireat","ids":[bomber,friendly],"x":76.0,"z":76.0,"queue":true}),
        );
        assert_eq!(
            m.aviation.planes[&bomber].mission.unwrap().kind,
            MissionKind::Area
        );
        assert!(m.commands.units[&bomber].orders.is_empty());
        m.aviation.planes.get_mut(&plane).unwrap().fuel = 0.0;
        let before = m.aviation.planes[&plane].mission;
        assert!(
            !m.send_aircraft_mission(
                0,
                plane,
                PlaneMission {
                    kind: MissionKind::Patrol,
                    target: None,
                    at: Point { x: 80.0, z: 80.0 },
                }
            )
            .unwrap()
        );
        assert_eq!(m.aviation.planes[&plane].mission, before);
    }

    #[test]
    fn mov90_plane_crosses_blocked_ground_and_death_leaves_no_wreck() {
        let mut m = scene();
        let plane = m.spawn_aircraft(0, "fighter").unwrap();
        let start = Point { x: 60.0, z: 60.0 };
        {
            let u = m.world.game.units.get_mut(&plane).unwrap();
            u.x = start.x;
            u.z = start.z;
            u.air = Some(PlaneState::Out);
            m.grid.update(plane, u);
        }
        let wall = m.world.game.cell_of(70.0, 60.0).unwrap();
        m.world.game.chars[wall] = b'#';
        m.world.game.cell_hp[wall] = 500.0;
        m.commands.units.get_mut(&plane).unwrap().hold_fire = true;
        m.aviation.planes.get_mut(&plane).unwrap().mission = Some(PlaneMission {
            kind: MissionKind::Patrol,
            target: None,
            at: Point { x: 90.0, z: 60.0 },
        });
        let hp = m.world.game.units[&plane].hp;
        m.step_aircraft(plane, 1.0).unwrap();
        assert!(m.world.game.units[&plane].x > 70.0);
        assert_eq!(m.world.game.units[&plane].hp, hp);
        assert!(m.world.game.units[&plane].path.is_empty());
        m.world.game.units.get_mut(&plane).unwrap().hp = 0.0;
        let death = m.retire_dead(&[plane]).unwrap();
        assert_eq!(death.retired.len(), 1);
        assert!(
            death.shots.is_empty(),
            "aircraft destruction creates no ground wreck"
        );
    }

    #[test]
    fn mov45_aircraft_waiting_order_starts_only_when_sortie_is_idle_and_ready() {
        let mut m = scene();
        let plane = m.spawn_aircraft(0, "fighter").unwrap();
        assert!(
            m.aircraft_queue_ready(plane).unwrap(),
            "ready aircraft at base"
        );
        m.aviation.planes.get_mut(&plane).unwrap().mission = Some(PlaneMission {
            kind: MissionKind::Patrol,
            target: None,
            at: Point { x: 70.0, z: 70.0 },
        });
        m.world.game.units.get_mut(&plane).unwrap().air = Some(PlaneState::Out);
        assert!(
            !m.aircraft_queue_ready(plane).unwrap(),
            "active sortie owns its turn"
        );
        m.world.game.units.get_mut(&plane).unwrap().air = Some(PlaneState::Rearm);
        m.aviation.planes.get_mut(&plane).unwrap().mission = None;
        assert!(
            !m.aircraft_queue_ready(plane).unwrap(),
            "rearming blocks waiting order"
        );
        m.world.game.units.get_mut(&plane).unwrap().air = Some(PlaneState::Base);
        m.aviation.planes.get_mut(&plane).unwrap().fuel = 0.0;
        assert!(
            !m.aircraft_queue_ready(plane).unwrap(),
            "empty fuel blocks waiting order"
        );
        m.aviation.planes.get_mut(&plane).unwrap().fuel = 1.0;
        m.aviation.planes.get_mut(&plane).unwrap().ammo = 0.0;
        assert!(
            !m.aircraft_queue_ready(plane).unwrap(),
            "empty ammo blocks waiting order"
        );
    }

    #[test]
    fn mov45_recorded_aircraft_queue_waits_starts_and_replaces() {
        let mut m = scene();
        let plane = m.spawn_aircraft(0, "fighter").unwrap();
        m.commands.units.get_mut(&plane).unwrap().hold_fire = true;
        fn order(m: &mut Match, raw: serde_json::Value) -> Option<crate::match_commands::Denial> {
            m.execute_command(0, &Command::from_json(&raw).unwrap(), false)
                .unwrap()
        }
        assert_eq!(
            order(
                &mut m,
                json!({"t":"move","orders":[[plane,50.0,50.0]],"queue":true})
            ),
            None
        );
        assert_eq!(m.commands.units[&plane].orders.len(), 1);
        m.advance_tick().unwrap();
        assert_eq!(m.commands.units[&plane].orders.len(), 0);
        assert_eq!(
            m.aviation.planes[&plane].mission.unwrap().at,
            Point { x: 50.0, z: 50.0 }
        );
        for index in 0..8 {
            assert_eq!(
                order(
                    &mut m,
                    json!({"t":"move","orders":[[plane,70.0 + index as f64,70.0]],"queue":true})
                ),
                None
            );
        }
        assert_eq!(m.commands.units[&plane].orders.len(), 8);
        assert_eq!(
            order(
                &mut m,
                json!({"t":"move","orders":[[plane,99.0,99.0]],"queue":true})
            ),
            Some(crate::match_commands::Denial::QueueFull)
        );
        m.advance_tick().unwrap();
        assert_eq!(
            m.commands.units[&plane].orders.len(),
            8,
            "active mission holds queue"
        );
        m.world.game.units.get_mut(&plane).unwrap().air = Some(PlaneState::Rearm);
        m.aviation.planes.get_mut(&plane).unwrap().mission = None;
        m.aviation.planes.get_mut(&plane).unwrap().timer = 30.0;
        m.advance_tick().unwrap();
        assert_eq!(
            m.commands.units[&plane].orders.len(),
            8,
            "rearm holds queue"
        );
        m.world.game.units.get_mut(&plane).unwrap().air = Some(PlaneState::Base);
        m.aviation.planes.get_mut(&plane).unwrap().fuel = 0.0;
        m.advance_tick().unwrap();
        assert_eq!(
            m.commands.units[&plane].orders.len(),
            8,
            "empty fuel holds queue"
        );
        m.aviation.planes.get_mut(&plane).unwrap().fuel = 1.0;
        m.aviation.planes.get_mut(&plane).unwrap().ammo = 0.0;
        m.advance_tick().unwrap();
        assert_eq!(
            m.commands.units[&plane].orders.len(),
            8,
            "empty ammo holds queue"
        );
        m.aviation.planes.get_mut(&plane).unwrap().ammo = 1.0;
        m.advance_tick().unwrap();
        assert_eq!(m.commands.units[&plane].orders.len(), 7);
        assert_eq!(
            m.aviation.planes[&plane].mission.unwrap().at,
            Point { x: 70.0, z: 70.0 }
        );
        assert_eq!(
            order(&mut m, json!({"t":"move","orders":[[plane,100.0,100.0]]})),
            None
        );
        assert!(
            m.commands.units[&plane].orders.is_empty(),
            "replacement clears waiting orders"
        );
    }
}
