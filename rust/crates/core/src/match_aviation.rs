//! On-map aircraft runtime. Game owns pose/state, firing owns weapon timers,
//! Combat owns HP attribution/XP and Logistics owns physical ground AA rounds.
use crate::{
    catalog::{Catalog, Value},
    economy::Mode,
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
    pub(crate) attack_picks: BTreeMap<UnitId, UnitId>,
    pub events: Vec<AviationEvent>,
}
impl AviationState {
    pub fn remove(&mut self, id: UnitId) {
        self.planes.remove(&id);
        self.anti_air.remove(&id);
        self.attack_picks.remove(&id);
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
    fn validate_aviation_context(&self, horde_spawn: bool) -> Result<(), AviationError> {
        let g = &self.world.game;
        if !(matches!(self.options.mode, Mode::Default | Mode::Conquest) && g.mode.is_none()
            || horde_spawn
                && self.options.mode == Mode::Horde
                && g.mode == Some(crate::game::ModeKind::Horde)
                && self.start.shared_defenders.is_some())
        {
            return Err(AviationError::Unsupported(
                "aviation currently requires ordinary Conquest",
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
        if !(matches!(self.options.mode, Mode::Default | Mode::Conquest)
            && self.world.game.mode.is_none()
            || self.options.mode == Mode::Horde
                && self.world.game.mode == Some(crate::game::ModeKind::Horde)
                && self.start.shared_defenders.is_some())
        {
            return Err(AviationError::Unsupported(
                "air base currently requires ordinary Conquest",
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
            let center = a.mission.map_or(base, |m| m.at);
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
