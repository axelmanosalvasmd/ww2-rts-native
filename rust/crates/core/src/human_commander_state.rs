//! Detached ordinary human commander state. The host owns delivery, policy RNG and admission.
//! An emitted command is a delayed attempt, never an assertion that the actuator accepted it.

use crate::{
    catalog::{Catalog, Value as CatalogValue},
    game::Point,
    human_commander::{HumanUnit, HumanView, OwnerHurtBatch, ScreenView, TickControl},
    human_commander_inputs::{self as inputs, Cursor, Input, InputKind},
    human_commander_timing::{self as timing, Skill, Timing},
    math,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

const SAFE_TICK: u64 = 9_007_199_254_740_991;

#[derive(Clone, Debug, PartialEq)]
pub enum Error {
    InvalidClock,
    InvalidView,
    InvalidBatch,
    InvalidState,
    InvalidProfile,
    Exhausted,
    Inputs(inputs::Error),
    Timing(timing::TimingError),
    Planner(String),
}
impl From<inputs::Error> for Error {
    fn from(error: inputs::Error) -> Self {
        Self::Inputs(error)
    }
}
impl From<timing::TimingError> for Error {
    fn from(error: timing::TimingError) -> Self {
        Self::Timing(error)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Alert {
    pub id: u64,
    pub tick: u64,
    pub victim: u32,
    pub at: Point,
    pub on_screen: bool,
    pub due: u64,
    pub answered: Option<u64>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnsweredAlert {
    pub id: u64,
    pub tick: u64,
}
#[derive(Clone, Debug, PartialEq)]
pub struct InputReceipt {
    pub tick: u64,
    pub origin: u64,
    pub kind: InputKind,
    pub at: Point,
    pub ids: Option<Vec<i64>>,
    pub minimap: Option<bool>,
    pub command: Option<Value>,
    pub camera: Point,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HumanAdvance {
    pub new_alerts: Vec<Alert>,
    pub answered: Vec<AnsweredAlert>,
    pub input: Option<InputReceipt>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ResumedOrder {
    pub command: Value,
    pub origin: u64,
}
#[derive(Clone, Debug, PartialEq)]
pub enum QueuedInput {
    Plan {
        ready: Option<u64>,
        causes: Vec<u64>,
        resume: Vec<ResumedOrder>,
    },
    Action {
        input: Input,
        origin: u64,
        causes: Vec<u64>,
    },
}
#[derive(Clone, Debug, PartialEq)]
pub struct Look {
    pub at: u64,
    pub target: Option<Point>,
    pub urgent: bool,
    pub causes: Vec<u64>,
    pub resume: Vec<ResumedOrder>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct AlertArea {
    pub at: Point,
    pub tick: u64,
}
#[derive(Clone, Debug, PartialEq)]
pub struct HumanRuntimeSnapshot {
    pub timing: Timing,
    pub skill: Skill,
    pub camera: Point,
    pub selection: Vec<i64>,
    pub inputs: VecDeque<QueuedInput>,
    pub next_input: u64,
    pub recent: VecDeque<u64>,
    pub seen: BTreeMap<u32, u64>,
    pub areas: Vec<AlertArea>,
    pub alerts: BTreeMap<u64, Alert>,
    pub pending: Vec<u64>,
    pub batch_generation: Option<u64>,
    pub batch_read: u64,
    pub next_alert: u64,
    pub last_place: Option<Point>,
    pub last_place_tick: Option<u64>,
    pub scan: u64,
    pub look: Option<Look>,
    pub view_tick: Option<u64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HumanState {
    recorded_seed: u32,
    owner: u32,
    initial_spawn: Point,
    // Subscriber identities remain distinct when the seat's private state resets.
    alert_allocator: u64,
    runtime: Option<HumanRuntimeSnapshot>,
}

fn valid_point(at: Point) -> bool {
    at.x.is_finite()
        && at.z.is_finite()
        && at.x.abs() <= SAFE_TICK as f64
        && at.z.abs() <= SAFE_TICK as f64
}
fn at_tick(now: u64, delay: u64) -> Result<u64, Error> {
    now.checked_add(delay)
        .filter(|n| *n <= SAFE_TICK)
        .ok_or(Error::Exhausted)
}
fn distance(a: Point, b: Point) -> f64 {
    math::hypot(a.x - b.x, a.z - b.z)
}

struct Profile {
    structure: bool,
    weapon: bool,
    full_hp: f64,
}
fn profile(kind: &str) -> Result<Profile, Error> {
    let definition = Catalog::compiled()
        .map_err(|_| Error::InvalidProfile)?
        .unit(kind)
        .map_err(|_| Error::InvalidProfile)?;
    let number = |key| {
        definition
            .get(key)
            .and_then(CatalogValue::number)
            .ok_or(Error::InvalidProfile)
    };
    let full_hp = number("models")? * number("hpPer")?;
    if !full_hp.is_finite() || full_hp <= 0.0 {
        return Err(Error::InvalidProfile);
    }
    let weapon = definition.get("w").is_some_and(|w| match w {
        CatalogValue::Null => false,
        CatalogValue::Bool(v) => *v,
        CatalogValue::Number(v) => *v != 0.0 && !v.is_nan(),
        CatalogValue::String(v) => !v.is_empty(),
        CatalogValue::Array(_) | CatalogValue::Object(_) => true,
    });
    Ok(Profile {
        structure: definition
            .get("structure")
            .and_then(CatalogValue::boolean)
            .unwrap_or(false),
        weapon,
        full_hp,
    })
}

fn validate<P>(now: u64, view: Option<&HumanView<P>>, batch: &OwnerHurtBatch) -> Result<(), Error> {
    if now > SAFE_TICK {
        return Err(Error::InvalidClock);
    }
    if batch.through_ordinal > SAFE_TICK
        || batch
            .events
            .windows(2)
            .any(|w| w[0].ordinal >= w[1].ordinal)
        || batch
            .events
            .iter()
            .any(|e| e.ordinal >= batch.through_ordinal || e.id == 0 || !valid_point(e.at))
    {
        return Err(Error::InvalidBatch);
    }
    if let Some(view) = view {
        if view.tick > now || view.spawn.is_some_and(|p| !valid_point(p)) {
            return Err(Error::InvalidView);
        }
        let mut ids = BTreeSet::new();
        for u in &view.units {
            if !ids.insert(u.motion.id)
                || u.motion.id == 0
                || !valid_point(u.motion.at)
                || u.motion.world_goal.is_some_and(|p| !valid_point(p))
                || u.motion.amove.is_some_and(|p| !valid_point(p))
                || !u.hp.is_finite()
                || u.hp < 0.0
            {
                return Err(Error::InvalidView);
            }
            profile(&u.kind)?;
        }
        let mut visible = BTreeSet::new();
        if view
            .visible
            .iter()
            .any(|id| !ids.contains(id) || !visible.insert(*id))
        {
            return Err(Error::InvalidView);
        }
        let mut sightings = BTreeSet::new();
        if view
            .sightings
            .iter()
            .any(|s| s.id == 0 || !sightings.insert(s.id))
        {
            return Err(Error::InvalidView);
        }
    }
    Ok(())
}

impl HumanState {
    pub fn new(recorded_seed: u32, owner: u32, initial_spawn: Point) -> Self {
        Self {
            recorded_seed,
            owner,
            initial_spawn,
            alert_allocator: 0,
            runtime: None,
        }
    }
    pub fn camera(&self) -> Option<Point> {
        self.runtime.as_ref().map(|s| s.camera)
    }
    pub fn snapshot(&self) -> Option<HumanRuntimeSnapshot> {
        self.runtime.clone()
    }
    pub fn wants_view(&self, now: u64) -> bool {
        timing::needs_view(
            self.runtime.is_some(),
            self.runtime
                .as_ref()
                .and_then(|s| s.look.as_ref().map(|l| l.at)),
            self.runtime
                .as_ref()
                .is_some_and(|s| matches!(s.inputs.front(), Some(QueuedInput::Plan { .. }))),
            now,
        )
    }
    pub fn reset(&mut self) {
        self.runtime = None;
    }

    /// The host stages callback-owned planner state/RNG. This function never retries a callback.
    /// Rebuild planner input from ScreenView's filtered rows, not packet-carried unit lists.
    pub fn advance<P, F>(
        &mut self,
        control: TickControl<'_>,
        view: Option<&HumanView<P>>,
        batch: &OwnerHurtBatch,
        mut planner: F,
    ) -> Result<HumanAdvance, Error>
    where
        F: FnMut(ScreenView<'_, P>) -> Result<Vec<Value>, Error>,
    {
        // The original live human stop precedes initialization, observation and alert reads.
        if control.live_owner_out || control.winner {
            return Ok(HumanAdvance::default());
        }
        validate(control.now, view, batch)?;
        if !valid_point(self.initial_spawn) {
            return Err(Error::InvalidState);
        }
        let mut staged = self.clone();
        if staged.runtime.is_none() {
            let skill = Skill::for_level(control.level);
            let mut stream = Timing::new(self.recorded_seed, self.owner);
            let at = at_tick(control.now, stream.opening_delay(skill)?)?;
            staged.runtime = Some(HumanRuntimeSnapshot {
                timing: stream,
                skill,
                camera: self.initial_spawn,
                selection: Vec::new(),
                inputs: VecDeque::new(),
                next_input: 0,
                recent: VecDeque::new(),
                seen: BTreeMap::new(),
                areas: Vec::new(),
                alerts: BTreeMap::new(),
                pending: Vec::new(),
                batch_generation: None,
                batch_read: 0,
                next_alert: staged.alert_allocator,
                last_place: None,
                last_place_tick: None,
                scan: 0,
                look: Some(Look {
                    at,
                    target: None,
                    urgent: false,
                    causes: Vec::new(),
                    resume: Vec::new(),
                }),
                view_tick: None,
            });
        }
        let state = staged.runtime.as_mut().expect("initialized");
        state.skill = Skill::for_level(control.level);
        let mut result = HumanAdvance::default();
        if let Some(view) = view
            && state.view_tick != Some(view.tick)
        {
            state.remember(view);
        }
        state.read_alerts(control.now, batch, &mut result)?;
        state.schedule_alerts()?;
        if state.inputs.is_empty()
            && state.look.is_none()
            && let Some(view) = view
        {
            state.look = Some(state.scan_look(self.owner, control.now, view)?);
        }
        if state
            .look
            .as_ref()
            .is_some_and(|look| control.now >= look.at)
        {
            state.start_look(control.now)?;
        }
        state.run_input(self.owner, control, view, &mut planner, &mut result)?;
        state.release_unreferenced_alerts();
        staged.alert_allocator = state.next_alert;
        *self = staged;
        Ok(result)
    }
}

impl HumanRuntimeSnapshot {
    fn release_unreferenced_alerts(&mut self) {
        let mut retained: BTreeSet<_> = self.pending.iter().copied().collect();
        if let Some(look) = &self.look {
            retained.extend(look.causes.iter().copied());
        }
        for input in &self.inputs {
            let causes = match input {
                QueuedInput::Plan { causes, .. } | QueuedInput::Action { causes, .. } => causes,
            };
            retained.extend(causes.iter().copied());
        }
        // Receipts own their records. With no internal reference, the source never
        // updates this alert again; the monotonic ID counter still prevents reuse.
        self.alerts.retain(|id, _| retained.contains(id));
    }

    fn remember<P>(&mut self, view: &HumanView<P>) {
        self.view_tick = Some(view.tick);
        for id in &view.visible {
            let u = view
                .units
                .iter()
                .find(|u| u.motion.id == *id)
                .expect("validated visible ID");
            if inputs::on_screen(self.camera, u.motion.at) {
                self.seen.insert(*id, view.tick);
            }
        }
        self.seen
            .retain(|_, at| view.tick.saturating_sub(*at) <= 600);
    }
    fn read_alerts(
        &mut self,
        now: u64,
        batch: &OwnerHurtBatch,
        out: &mut HumanAdvance,
    ) -> Result<(), Error> {
        if self.batch_generation != Some(batch.generation) {
            self.batch_generation = Some(batch.generation);
            self.batch_read = 0;
        }
        if batch.through_ordinal < self.batch_read {
            return Err(Error::InvalidBatch);
        }
        for event in batch
            .events
            .iter()
            .filter(|event| event.ordinal >= self.batch_read)
        {
            self.areas.retain(|a| now.saturating_sub(a.tick) < 400);
            if self.areas.iter().any(|a| distance(a.at, event.at) <= 30.0) {
                continue;
            }
            self.areas.push(AlertArea {
                at: event.at,
                tick: now,
            });
            let on_screen = inputs::on_screen(self.camera, event.at);
            let due = at_tick(now, self.timing.reaction_delay(self.skill, on_screen)?)?;
            let id = self.next_alert;
            self.next_alert = id.checked_add(1).ok_or(Error::Exhausted)?;
            let alert = Alert {
                id,
                tick: now,
                victim: event.id,
                at: event.at,
                on_screen,
                due,
                answered: None,
            };
            self.pending.push(id);
            self.alerts.insert(id, alert.clone());
            out.new_alerts.push(alert);
        }
        self.batch_read = batch.through_ordinal;
        Ok(())
    }
    fn schedule_alerts(&mut self) -> Result<(), Error> {
        let mut first: Option<&Alert> = None;
        for id in &self.pending {
            let alert = self.alerts.get(id).ok_or(Error::InvalidState)?;
            if first.is_none_or(|first| alert.due < first.due) {
                first = Some(alert);
            }
        }
        let Some(first) = first else {
            return Ok(());
        };
        if self
            .look
            .as_ref()
            .is_some_and(|look| look.urgent && look.at <= first.due)
        {
            return Ok(());
        }
        self.look = Some(Look {
            at: first.due,
            target: (!inputs::on_screen(self.camera, first.at)).then_some(first.at),
            urgent: true,
            causes: Vec::new(),
            resume: Vec::new(),
        });
        Ok(())
    }
    fn scan_look<P>(&mut self, owner: u32, now: u64, view: &HumanView<P>) -> Result<Look, Error> {
        let own: Vec<_> = view
            .units
            .iter()
            .filter(|u| u.owner == owner && !u.air && u.hp > 0.0)
            .collect();
        let near = |u: &HumanUnit| {
            own.iter()
                .map(|o| {
                    (o.motion.at.x - u.motion.at.x).abs() + (o.motion.at.z - u.motion.at.z).abs()
                })
                .fold(f64::INFINITY, math::min)
        };
        let mut dots: Vec<_> = view
            .visible
            .iter()
            .filter(|id| !self.seen.contains_key(id))
            .map(|id| {
                view.units
                    .iter()
                    .find(|u| u.motion.id == *id)
                    .expect("validated visible ID")
            })
            .collect();
        dots.sort_by(|a, b| near(a).total_cmp(&near(b)));
        let target = if let Some(dot) = dots.first() {
            Some(dot.motion.at)
        } else {
            let mut groups: Vec<((i64, i64), Point, usize)> = Vec::new();
            for u in own {
                let key = (
                    (u.motion.at.x / 110.0).floor() as i64,
                    (u.motion.at.z / 80.0).floor() as i64,
                );
                if let Some((_, sum, n)) = groups.iter_mut().find(|(k, _, _)| *k == key) {
                    sum.x += u.motion.at.x;
                    sum.z += u.motion.at.z;
                    *n += 1;
                } else {
                    groups.push((key, u.motion.at, 1));
                }
            }
            groups.sort_by(|a, b| b.2.cmp(&a.2));
            let mut places: Vec<_> = groups
                .into_iter()
                .map(|(_, sum, n)| Point {
                    x: sum.x / n as f64,
                    z: sum.z / n as f64,
                })
                .collect();
            if let Some(spawn) = view.spawn {
                places.push(spawn);
            }
            let index = self.scan % (places.len().max(1) as u64);
            self.scan = self
                .scan
                .checked_add(1)
                .filter(|n| *n <= SAFE_TICK)
                .ok_or(Error::Exhausted)?;
            places.get(index as usize).copied()
        };
        let load = self
            .recent
            .iter()
            .filter(|tick| now.saturating_sub(**tick) < 200)
            .count() as f64
            / self.skill.peak as f64;
        let at = at_tick(now, self.timing.scan_delay(self.skill, load)?)?;
        Ok(Look {
            at,
            target,
            urgent: false,
            causes: Vec::new(),
            resume: Vec::new(),
        })
    }
    fn start_look(&mut self, now: u64) -> Result<(), Error> {
        let mut look = self.look.take().ok_or(Error::InvalidState)?;
        if look.urgent {
            let mut pending = Vec::new();
            for id in self.pending.drain(..) {
                if self.alerts.get(&id).ok_or(Error::InvalidState)?.due <= now {
                    look.causes.push(id);
                } else {
                    pending.push(id);
                }
            }
            self.pending = pending;
            for queued in &self.inputs {
                let causes = match queued {
                    QueuedInput::Plan { causes, .. } | QueuedInput::Action { causes, .. } => causes,
                };
                for id in causes {
                    if self
                        .alerts
                        .get(id)
                        .ok_or(Error::InvalidState)?
                        .answered
                        .is_none()
                        && !look.causes.contains(id)
                    {
                        look.causes.push(*id);
                    }
                }
            }
            look.resume = self
                .inputs
                .iter()
                .filter_map(|queued| match queued {
                    QueuedInput::Action { input, origin, .. } => {
                        input.command.as_ref().map(|command| ResumedOrder {
                            command: command.clone(),
                            origin: *origin,
                        })
                    }
                    QueuedInput::Plan { .. } => None,
                })
                .collect();
            self.inputs.clear();
        }
        if let Some(at) = look.target
            && !inputs::on_screen(self.camera, at)
        {
            self.inputs.push_back(QueuedInput::Action {
                input: Input {
                    kind: InputKind::Camera,
                    at,
                    ids: None,
                    minimap: None,
                    command: None,
                },
                origin: now,
                causes: look.causes.clone(),
            });
        }
        self.inputs.push_back(QueuedInput::Plan {
            ready: None,
            causes: look.causes,
            resume: look.resume,
        });
        Ok(())
    }
    fn screen_view<'a, P>(&self, view: &'a HumanView<P>) -> ScreenView<'a, P> {
        ScreenView {
            tick: view.tick,
            spawn: view.spawn,
            units: view
                .units
                .iter()
                .filter(|u| {
                    !view.visible.contains(&u.motion.id) || self.seen.contains_key(&u.motion.id)
                })
                .collect(),
            visible: view
                .visible
                .iter()
                .filter(|id| self.seen.contains_key(id))
                .copied()
                .collect(),
            sightings: view
                .sightings
                .iter()
                .filter(|s| self.seen.contains_key(&s.id))
                .collect(),
            packet: &view.packet,
        }
    }
    fn fight_response<P>(
        &self,
        owner: u32,
        level: &str,
        view: &HumanView<P>,
        alert: &Alert,
        orders: &[Value],
    ) -> Result<Option<Value>, Error> {
        let screen = self.screen_view(view);
        let ordered: BTreeSet<_> = orders
            .iter()
            .map(inputs::units_of)
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flatten()
            .collect();
        let Some(u) = screen.units.iter().find(|u| u.motion.id == alert.victim) else {
            return Ok(None);
        };
        if u.owner != owner || u.hp <= 0.0 {
            return Ok(None);
        }
        let def = profile(&u.kind)?;
        let retreat = if level == "hard" { 0.5 } else { 0.35 };
        if !def.structure
            && !u.air
            && !u.motion.retreating
            && !ordered.contains(&i64::from(u.motion.id))
            && u.hp / def.full_hp < retreat
        {
            return Ok(Some(json!({"t":"retreat","ids":[u.motion.id]})));
        }
        let mut foes: Vec<_> = screen
            .visible
            .iter()
            .filter_map(|id| screen.units.iter().find(|u| u.motion.id == *id).copied())
            .filter(|e| e.hp > 0.0 && !e.air && distance(e.motion.at, u.motion.at) <= 60.0)
            .collect();
        foes.sort_by(|a, b| {
            distance(a.motion.at, u.motion.at).total_cmp(&distance(b.motion.at, u.motion.at))
        });
        let Some(foe) = foes.first() else {
            return Ok(None);
        };
        let mut idle = Vec::new();
        for o in screen.units {
            let def = profile(&o.kind)?;
            if o.owner == owner
                && o.hp > 0.0
                && def.weapon
                && !def.structure
                && !o.air
                && !o.motion.retreating
                && o.motion.target_id == 0
                && o.motion.amove.is_none()
                && !o.build
                && !o.dig
                && !o.entrench
                && !o.garrison
                && !ordered.contains(&i64::from(o.motion.id))
                && distance(o.motion.at, u.motion.at) <= 55.0
            {
                idle.push(o.motion.id);
            }
        }
        Ok((!idle.is_empty()).then(|| json!({"t":"attack","ids":idle,"target":foe.motion.id})))
    }
    fn plan_now<P, F>(
        &mut self,
        owner: u32,
        control: TickControl<'_>,
        view: &HumanView<P>,
        causes: Vec<u64>,
        resume: Vec<ResumedOrder>,
        planner: &mut F,
    ) -> Result<(), Error>
    where
        F: FnMut(ScreenView<'_, P>) -> Result<Vec<Value>, Error>,
    {
        let mut orders = planner(self.screen_view(view))?;
        for command in &orders {
            inputs::units_of(command)?;
        }
        for id in &causes {
            let alert = self.alerts.get(id).ok_or(Error::InvalidState)?;
            if inputs::on_screen(self.camera, alert.at)
                && let Some(answer) =
                    self.fight_response(owner, control.level, view, alert, &orders)?
            {
                orders.push(answer);
            }
        }
        let motion = inputs::View {
            units: view.units.iter().map(|u| u.motion.clone()).collect(),
        };
        let mut cursor = Cursor {
            camera: self.camera,
            selection: self.selection.clone(),
        };
        let start = self.inputs.len();
        let mut filtered = Vec::new();
        for command in &orders {
            if let Some(command) = inputs::news(command, &motion)? {
                filtered.push(command);
            }
        }
        for command in inputs::formation_moves(filtered, &motion)? {
            for input in inputs::inputs_for(&mut cursor, &command, &motion)? {
                self.inputs.push_back(QueuedInput::Action {
                    input,
                    origin: control.now,
                    causes: Vec::new(),
                });
            }
        }
        if let Some(QueuedInput::Action {
            causes: input_causes,
            ..
        }) = self.inputs.get_mut(start)
        {
            *input_causes = causes;
        }
        let ordered: BTreeSet<_> = orders
            .iter()
            .map(inputs::units_of)
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flatten()
            .collect();
        for resumed in resume {
            if !inputs::units_of(&resumed.command)?
                .iter()
                .any(|id| ordered.contains(id))
            {
                for input in inputs::inputs_for(&mut cursor, &resumed.command, &motion)? {
                    self.inputs.push_back(QueuedInput::Action {
                        input,
                        origin: resumed.origin,
                        causes: Vec::new(),
                    });
                }
            }
        }
        Ok(())
    }
    fn run_input<P, F>(
        &mut self,
        owner: u32,
        control: TickControl<'_>,
        view: Option<&HumanView<P>>,
        planner: &mut F,
        out: &mut HumanAdvance,
    ) -> Result<(), Error>
    where
        F: FnMut(ScreenView<'_, P>) -> Result<Vec<Value>, Error>,
    {
        let now = control.now;
        while let Some(QueuedInput::Plan { ready, .. }) = self.inputs.front_mut() {
            let Some(view) = view else { return Ok(()) };
            let threshold = *ready.get_or_insert(now);
            if view.tick < threshold {
                return Ok(());
            }
            let Some(QueuedInput::Plan { causes, resume, .. }) = self.inputs.pop_front() else {
                return Err(Error::InvalidState);
            };
            self.plan_now(owner, control, view, causes, resume, planner)?;
        }
        if self.inputs.is_empty() || now < self.next_input {
            return Ok(());
        }
        while self
            .recent
            .front()
            .is_some_and(|tick| now.saturating_sub(*tick) >= 1200)
        {
            self.recent.pop_front();
        }
        let peak = self
            .recent
            .iter()
            .rev()
            .take_while(|tick| now.saturating_sub(**tick) < 200)
            .count();
        if peak >= self.skill.peak || self.recent.len() >= self.skill.minute {
            return Ok(());
        }
        let Some(QueuedInput::Action { input, .. }) = self.inputs.front() else {
            return Err(Error::InvalidState);
        };
        let place = if let Some(command) = &input.command {
            let view = view.ok_or(Error::InvalidView)?;
            let motion = inputs::View {
                units: view.units.iter().map(|u| u.motion.clone()).collect(),
            };
            inputs::place_of(command, &motion)?
        } else {
            None
        };
        if let (Some(place), Some(last), Some(at)) = (place, self.last_place, self.last_place_tick)
            && !inputs::on_screen(last, place)
            && now.saturating_sub(at) < 5
        {
            return Ok(());
        }
        let Some(QueuedInput::Action {
            input,
            origin,
            causes,
        }) = self.inputs.pop_front()
        else {
            return Err(Error::InvalidState);
        };
        match input.kind {
            InputKind::Camera => self.camera = input.at,
            InputKind::Select => {
                let mut ids = input.ids.clone().ok_or(Error::InvalidState)?;
                ids.sort_unstable();
                self.selection = ids;
            }
            InputKind::Key | InputKind::Click => {}
        }
        if input.command.is_some()
            && let Some(place) = place
        {
            self.last_place = Some(place);
            self.last_place_tick = Some(now);
        }
        for id in causes {
            let alert = self.alerts.get_mut(&id).ok_or(Error::InvalidState)?;
            if origin >= alert.due && alert.answered.is_none() {
                alert.answered = Some(now);
                out.answered.push(AnsweredAlert { id, tick: now });
            }
        }
        self.recent.push_back(now);
        self.next_input = at_tick(now, self.timing.input_delay(self.skill)?)?;
        out.input = Some(InputReceipt {
            tick: now,
            origin,
            kind: input.kind,
            at: input.at,
            ids: input.ids,
            minimap: input.minimap,
            command: input.command,
            camera: self.camera,
        });
        Ok(())
    }
}
