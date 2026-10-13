//! Canonical control intent and tick-boundary commands for native Conquest.

use crate::{
    catalog::Catalog,
    game::{Point, Unit},
    grid::UnitId,
    orders::OrderQueue,
    path_requests::PathRequests,
    pathfinding::Pathfinding,
    rules::Rules,
};
use std::collections::{BTreeMap, VecDeque};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Facing {
    pub angle: f64,
    pub at: Point,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoveOutcome {
    Interrupted,
    Moving,
    Unreachable,
    Arrived,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MoveOrder {
    pub unit: i64,
    pub at: Point,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stance {
    HoldFire,
    HoldPosition,
    AutoRetreat,
}

/// Internal recorded input. This is not a WebSocket or C interface.
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    Move {
        orders: Vec<MoveOrder>,
        control_ids: Option<Vec<i64>>,
        attack_move: bool,
        queue: bool,
        together: bool,
        face: Option<f64>,
    },
    Stop {
        ids: Vec<i64>,
    },
    Stance {
        ids: Vec<i64>,
        stance: Stance,
        on: bool,
    },
    Ability {
        ids: Vec<i64>,
        at: Option<Point>,
        queue: bool,
        automatic: bool,
    },
    Autocast {
        ids: Vec<i64>,
        on: bool,
    },
    Unsupported,
}

/// Only fields absent from Unit live here. Routes, positions and hold_pos remain in Unit.
#[derive(Clone, Debug, PartialEq)]
pub struct UnitControl {
    pub aim: f64,
    pub auto_cast: bool,
    pub auto_retreat: bool,
    pub attack_id: UnitId,
    pub follow: UnitId,
    pub hold_fire: bool,
    pub face: Option<Facing>,
    pub orders: VecDeque<Command>,
    pub outcome: Option<MoveOutcome>,
    pub outcome_tick: Option<i64>,
    pub route_end: Option<Point>,
    pub supply_hold: bool,
    pub supply_escape: bool,
    pub enter: Option<usize>,
    pub fire_at: Option<usize>,
    pub nade: Option<Point>,
    /// Recruitment marks a rally walk so queued orders do not interrupt it.
    pub rally_drift: bool,
    /// Operatives reject ordinary squad orders.
    pub operative: bool,
    /// World local guards return here in the guard phase, owned by tick scheduling.
    pub guard_home: Option<Point>,
}
impl UnitControl {
    /// Prepare before publishing a spawned unit or charging a wallet.
    pub fn new(unit: &Unit, rules: &Rules) -> Result<Self, String> {
        let kind = rules
            .kinds
            .get(unit.kind)
            .ok_or("unknown control unit kind")?;
        let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
        let definition = catalog.unit(&kind.name).map_err(|e| e.to_string())?;
        let ability = definition
            .get("ab")
            .and_then(|a| a.get("id"))
            .and_then(|v| v.string())
            .ok_or("missing unit ability")?;
        if !unit.rot.is_finite() {
            return Err("nonfinite unit aim".into());
        }
        Ok(Self {
            aim: unit.rot,
            auto_cast: ability != "none",
            auto_retreat: !kind.air,
            attack_id: 0,
            follow: 0,
            hold_fire: false,
            face: None,
            orders: VecDeque::new(),
            outcome: None,
            outcome_tick: None,
            route_end: None,
            supply_hold: false,
            supply_escape: false,
            enter: None,
            fire_at: None,
            nade: None,
            rally_drift: false,
            operative: false,
            guard_home: None,
        })
    }
}

pub struct MatchCommands {
    pub units: BTreeMap<UnitId, UnitControl>,
    pub(crate) queue: OrderQueue<Command>,
    pub(crate) paths: Pathfinding,
    pub(crate) requests: PathRequests,
    path_tick: Option<i64>,
}
impl MatchCommands {
    pub fn new(capacity: usize) -> Self {
        let mut queue = OrderQueue::new(capacity);
        // Match startup is tick zero; commands first execute at tick one.
        queue.advance().expect("initial empty tick");
        Self {
            units: BTreeMap::new(),
            queue,
            paths: Pathfinding::new(),
            requests: PathRequests::new(),
            path_tick: None,
        }
    }
    /// Publish an already prepared row after the caller commits its spawn transaction.
    pub fn insert(&mut self, id: UnitId, row: UnitControl) {
        self.units.insert(id, row);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Denial {
    Blocked,
    Needs,
    QueueFull,
    Unsupported,
    Ammo,
    Cooldown,
    Retreating,
    Munitions,
}
impl Denial {
    pub fn name(self) -> &'static str {
        match self {
            Self::Blocked => "blocked",
            Self::Needs => "needs",
            Self::QueueFull => "queueFull",
            Self::Unsupported => "unsupported",
            Self::Ammo => "ammo",
            Self::Cooldown => "cooldown",
            Self::Retreating => "retreating",
            Self::Munitions => "mun",
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct CommandResult {
    pub key: crate::orders::OrderKey,
    pub denial: Option<Denial>,
}

impl Command {
    /// Validate every row before unit selection or the source's row cap.
    pub fn from_json(value: &serde_json::Value) -> Result<Self, Denial> {
        let object = value.as_object().ok_or(Denial::Blocked)?;
        let safe_id = |value: &serde_json::Value| {
            value
                .as_f64()
                .filter(|n| n.is_finite() && n.fract() == 0.0 && n.abs() <= 9007199254740991.0)
                .map(|n| n as i64)
        };
        let mut orders = Vec::new();
        if let Some(raw) = object.get("orders") {
            for row in raw.as_array().ok_or(Denial::Blocked)? {
                let row = row
                    .as_array()
                    .filter(|r| r.len() >= 3)
                    .ok_or(Denial::Blocked)?;
                orders.push(MoveOrder {
                    unit: safe_id(&row[0]).ok_or(Denial::Blocked)?,
                    at: Point {
                        x: row[1]
                            .as_f64()
                            .filter(|n| n.is_finite())
                            .ok_or(Denial::Blocked)?,
                        z: row[2]
                            .as_f64()
                            .filter(|n| n.is_finite())
                            .ok_or(Denial::Blocked)?,
                    },
                });
            }
        }
        let ids = object
            .get("ids")
            .and_then(|v| v.as_array())
            .map_or_else(Vec::new, |rows| {
                rows.iter().map(|v| safe_id(v).unwrap_or(-1)).collect()
            });
        match object.get("t").and_then(|v| v.as_str()) {
            Some("move" | "amove") if object.contains_key("orders") => {
                let face = object
                    .get("face")
                    .map(|v| {
                        v.as_f64()
                            .and_then(crate::formation::normalize_face)
                            .ok_or(Denial::Blocked)
                    })
                    .transpose()?;
                Ok(Self::Move {
                    orders,
                    control_ids: object.get("ids").and_then(|v| v.as_array()).map(|_| ids),
                    attack_move: object["t"] == "amove",
                    queue: object.get("queue").and_then(|v| v.as_bool()) == Some(true),
                    together: object.get("together").and_then(|v| v.as_bool()) == Some(true),
                    face,
                })
            }
            Some("ability") => {
                let at = object
                    .get("x")
                    .and_then(|x| x.as_f64())
                    .filter(|x| x.is_finite())
                    .zip(
                        object
                            .get("z")
                            .and_then(|z| z.as_f64())
                            .filter(|z| z.is_finite()),
                    )
                    .map(|(x, z)| Point { x, z });
                Ok(Self::Ability {
                    ids,
                    at,
                    queue: object.get("queue").and_then(|v| v.as_bool()) == Some(true),
                    automatic: false,
                })
            }
            Some("autocast") => Ok(Self::Autocast {
                ids,
                on: object
                    .get("on")
                    .and_then(|v| v.as_bool())
                    .ok_or(Denial::Blocked)?,
            }),
            Some("stop") => Ok(Self::Stop { ids }),
            Some("stance") => {
                let stance = match object.get("key").and_then(|v| v.as_str()) {
                    Some("holdFire") => Stance::HoldFire,
                    Some("holdPos") => Stance::HoldPosition,
                    Some("autoRetreat") => Stance::AutoRetreat,
                    _ => return Err(Denial::Blocked),
                };
                Ok(Self::Stance {
                    ids,
                    stance,
                    on: object
                        .get("on")
                        .and_then(|v| v.as_bool())
                        .ok_or(Denial::Blocked)?,
                })
            }
            _ => Ok(Self::Unsupported),
        }
    }
    fn valid(&self) -> bool {
        match self {
            Self::Move { orders, face, .. } => {
                orders.iter().all(|o| {
                    o.unit.unsigned_abs() <= 9007199254740991
                        && o.at.x.is_finite()
                        && o.at.z.is_finite()
                }) && face.is_none_or(|a| a.is_finite())
            }
            _ => true,
        }
    }
}

impl crate::match_runtime::Match {
    pub fn enqueue_command(
        &mut self,
        key: crate::orders::OrderKey,
        command: Command,
    ) -> Result<(), crate::orders::QueueError> {
        self.commands.queue.enqueue(key, command)
    }
    pub fn validate_command_tick(&self, tick: u64) -> Result<(), String> {
        if self.commands.queue.next_tick() != Some(tick) {
            return Err("command tick is not the next queued tick".into());
        }
        Ok(())
    }
    /// Apply recorded boundary input before the Match runner advances its clock.
    pub fn apply_commands(
        &mut self,
        tick: u64,
        winner_decided: bool,
    ) -> Result<Vec<CommandResult>, String> {
        self.validate_command_tick(tick)?;
        if self.world.game.tick < 0 || (self.world.game.tick as u64).checked_add(1) != Some(tick) {
            return Err("command tick differs from Match clock".into());
        }
        let batch = self
            .commands
            .queue
            .advance()
            .map_err(|e| format!("command queue: {e:?}"))?;
        let mut results = Vec::with_capacity(batch.orders.len());
        for order in batch.orders {
            let denial =
                self.execute_command(order.key.issuer as usize, &order.value, winner_decided)?;
            results.push(CommandResult {
                key: order.key,
                denial,
            });
        }
        Ok(results)
    }
    fn eligible_command_unit(&self, id: i64, owner: usize, command: &Command) -> Option<UnitId> {
        let id = u32::try_from(id).ok()?;
        let u = self.world.game.units.get(&id)?;
        let kind = self.rules.kinds.get(u.kind)?;
        let control = self.commands.units.get(&id)?;
        let forced = self.logistics.get(id).is_some_and(|r| r.forced);
        let allowed_forced = self.logistics.get(id).is_some_and(|r| r.stranded)
            && matches!(command, Command::Ability { .. })
            || matches!(
                command,
                Command::Move {
                    attack_move: false,
                    ..
                } | Command::Stop { .. }
            );
        (u.owner == owner
            && !kind.structure
            && !kind.rail
            && !u.rail
            && !u.riding
            && !control.operative
            && (!forced || allowed_forced))
            .then_some(id)
    }
    pub fn execute_command(
        &mut self,
        owner: usize,
        command: &Command,
        winner_decided: bool,
    ) -> Result<Option<Denial>, String> {
        let Some(player) = self.start.economy.players.get(owner) else {
            return Ok(Some(Denial::Blocked));
        };
        if winner_decided || player.out || player.away || !command.valid() {
            return Ok(Some(Denial::Blocked));
        }
        if matches!(command, Command::Unsupported) {
            return Ok(Some(Denial::Unsupported));
        }
        if matches!(command, Command::Ability { .. } | Command::Autocast { .. }) {
            self.validate_abilities()?;
        }
        let limit = 50.max(self.world.game.units.len());
        let controlled: Vec<_> = match command {
            Command::Move {
                orders,
                control_ids,
                ..
            } => control_ids
                .clone()
                .unwrap_or_else(|| orders.iter().map(|o| o.unit).collect()),
            Command::Stop { ids }
            | Command::Stance { ids, .. }
            | Command::Ability { ids, .. }
            | Command::Autocast { ids, .. } => ids.clone(),
            Command::Unsupported => unreachable!(),
        };
        if matches!(
            command,
            Command::Stop { .. } | Command::Move { queue: false, .. }
        ) {
            let rows: Vec<_> = match command {
                Command::Move { orders, .. } => orders.iter().take(limit).map(|o| o.unit).collect(),
                _ => controlled.iter().take(limit).copied().collect(),
            };
            let mut advances: BTreeMap<UnitId, u64> = BTreeMap::new();
            for raw in rows {
                if let Some(id) = self.eligible_command_unit(raw, owner, command) {
                    *advances.entry(id).or_default() += 1;
                }
            }
            for (id, count) in advances {
                if self.world.game.units[&id]
                    .path_gen
                    .checked_add(count)
                    .is_none()
                {
                    return Err("unit path identity exhausted".into());
                }
            }
        }
        if !self.intercept_convoy_movement(owner, command, &controlled)? {
            return Ok(Some(Denial::Blocked));
        }
        // The source applies forced supply holds before ordinary ownership filtering.
        for raw in &controlled {
            if let Ok(id) = u32::try_from(*raw)
                && self
                    .world
                    .game
                    .units
                    .get(&id)
                    .is_some_and(|u| u.owner == owner)
                && self.logistics.get(id).is_some_and(|r| r.forced)
                && let Some(control) = self.commands.units.get_mut(&id)
            {
                match command {
                    Command::Stop { .. } => control.supply_hold = true,
                    Command::Move {
                        attack_move: false, ..
                    } => control.supply_hold = false,
                    _ => {}
                }
            }
        }

        let selected_ids: Vec<_> = match command {
            Command::Move { orders, .. } => orders.iter().map(|o| o.unit).collect(),
            _ => controlled.clone(),
        };
        let selected: Vec<_> = selected_ids
            .iter()
            .take(limit)
            .filter_map(|&id| self.eligible_command_unit(id, owner, command))
            .collect();
        if selected
            .iter()
            .any(|id| self.world.game.units[id].air.is_some())
        {
            return Ok(Some(Denial::Unsupported));
        }
        if let Command::Ability {
            at,
            queue,
            automatic,
            ..
        } = command
        {
            if self.unrepresented_ability_work(&selected, *at, *queue, *automatic)? {
                return Ok(Some(Denial::Unsupported));
            }
            self.preflight_ability_selection(&selected, *at, *queue, *automatic)?;
        }
        for id in &selected {
            self.commands
                .units
                .get_mut(id)
                .expect("selected control")
                .follow = 0;
        }
        match command {
            Command::Stop { .. } => {
                if selected.is_empty() {
                    return Ok(Some(Denial::Needs));
                }
                for id in selected {
                    self.stop_command_unit(id);
                }
            }
            Command::Stance { stance, on, .. } => {
                if selected.is_empty() {
                    return Ok(Some(Denial::Needs));
                }
                for id in selected {
                    let c = self.commands.units.get_mut(&id).expect("control");
                    match stance {
                        Stance::HoldFire => c.hold_fire = *on,
                        Stance::AutoRetreat => c.auto_retreat = *on,
                        Stance::HoldPosition => {
                            self.world.game.units.get_mut(&id).expect("unit").hold_pos = *on
                        }
                    }
                }
            }
            Command::Move {
                orders,
                attack_move,
                queue,
                together,
                face,
                ..
            } => {
                return self.move_command(owner, orders, *attack_move, *queue, *together, *face);
            }
            Command::Ability {
                ids,
                at,
                queue,
                automatic,
            } => {
                let ordered = ids
                    .iter()
                    .take(limit)
                    .map(|id| self.eligible_command_unit(*id, owner, command))
                    .collect::<Vec<_>>();
                return self.apply_ability_selection(owner, &ordered, *at, *queue, *automatic);
            }
            Command::Autocast { on, .. } => {
                let ready: Vec<_> = selected
                    .into_iter()
                    .filter(|id| {
                        self.ability_kind(*id)
                            .is_ok_and(|kind| kind != crate::match_abilities::AbilityKind::None)
                    })
                    .collect();
                if ready.is_empty() {
                    return Ok(Some(Denial::Needs));
                }
                for id in ready {
                    self.commands.units.get_mut(&id).unwrap().auto_cast = *on;
                }
            }
            Command::Unsupported => unreachable!(),
        }
        Ok(None)
    }
    fn stop_command_unit(&mut self, id: UnitId) {
        let u = self.world.game.units.get_mut(&id).expect("selected unit");
        let c = self.commands.units.get_mut(&id).expect("selected control");
        u.traffic = None;
        u.traffic_wait = 0.0;
        u.world_goal = None;
        u.entrench = false;
        u.path.clear();
        u.path_gen += 1;
        u.attack = false;
        u.retreating = false;
        u.dig = false;
        u.board = 0;
        u.amove = None;
        u.build = false;
        c.orders.clear();
        c.face = None;
        c.attack_id = 0;
        c.nade = None;
        c.enter = None;
        c.fire_at = None;
        c.outcome = Some(MoveOutcome::Interrupted);
        c.outcome_tick = Some(self.world.game.tick);
    }
}

impl MatchCommands {
    pub fn next_tick(&self) -> Option<u64> {
        self.queue.next_tick()
    }
    pub fn pending_commands(&self) -> usize {
        self.queue.len()
    }
    pub fn path_requests(&self) -> &PathRequests {
        &self.requests
    }
}
impl crate::match_runtime::Match {
    fn move_command(
        &mut self,
        owner: usize,
        orders: &[MoveOrder],
        attack_move: bool,
        queued: bool,
        together: bool,
        face: Option<f64>,
    ) -> Result<Option<Denial>, String> {
        use crate::command_destinations::{Context, Request};
        let face = face.and_then(crate::formation::normalize_face);
        let command = Command::Move {
            control_ids: None,
            orders: orders.to_vec(),
            attack_move,
            queue: queued,
            together,
            face,
        };
        let mut requests: Vec<_> = orders
            .iter()
            .take(50.max(self.world.game.units.len()))
            .filter_map(|o| {
                self.eligible_command_unit(o.unit, owner, &command)
                    .map(|id| Request {
                        id,
                        at: Point {
                            x: crate::math::max(
                                0.0,
                                crate::math::min(
                                    self.world.game.w as f64 * crate::game::CELL,
                                    o.at.x,
                                ),
                            ),
                            z: crate::math::max(
                                0.0,
                                crate::math::min(
                                    self.world.game.h as f64 * crate::game::CELL,
                                    o.at.z,
                                ),
                            ),
                        },
                    })
            })
            .collect();
        if requests.is_empty() {
            return Ok(Some(Denial::Blocked));
        }
        let points: Vec<_> = self.start.points.iter().map(|p| p.at).collect();
        let hit_at = self.combat.iter().map(|(id, c)| (id, c.hit_at)).collect();
        let context = Context {
            game: &self.world.game,
            rules: &self.rules,
            grid: &self.grid,
            points: &points,
            hit_at: &hit_at,
        };
        if queued {
            if let Some(face) = face {
                requests =
                    crate::command_destinations::spread_queued_facing(&context, &requests, face)
                        .map_err(|e| format!("queued facing: {e:?}"))?;
            }
            let mut any = false;
            for r in requests {
                let control = self.commands.units.get_mut(&r.id).expect("control");
                if control.orders.len() < 8 {
                    control.orders.push_back(Command::Move {
                        control_ids: None,
                        orders: vec![MoveOrder {
                            unit: i64::from(r.id),
                            at: r.at,
                        }],
                        attack_move,
                        queue: false,
                        together: false,
                        face,
                    });
                    any = true;
                }
            }
            return Ok((!any).then_some(Denial::QueueFull));
        }
        let destinations = crate::command_destinations::end_spots(&context, &requests, face)
            .map_err(|e| format!("move destination: {e:?}"))?;
        let pace = if together && requests.len() > 1 {
            requests
                .iter()
                .map(|r| self.rules.kinds[self.world.game.units[&r.id].kind].speed)
                .fold(f64::INFINITY, crate::math::min)
        } else {
            0.0
        };
        let behavior = Catalog::compiled()
            .map_err(|e| e.to_string())?
            .table("CFG")
            .map_err(|e| e.to_string())?
            .get("behavior")
            .ok_or("missing command behavior")?;
        let threat = behavior
            .get("threatTime")
            .and_then(|v| v.number())
            .ok_or("missing threat time")?
            / self.rules.movement.tick;
        let reverse_max = behavior
            .get("reverseMax")
            .and_then(|v| v.number())
            .ok_or("missing reverse distance")?;
        for r in destinations {
            let mut u = self.world.game.units[&r.id].clone();
            let engaged = self
                .firing
                .units
                .get(&r.id)
                .is_some_and(|s| s.target_id != 0)
                || self.world.game.tick as f64 - self.combat.get(r.id).map_or(-1e9, |s| s.hit_at)
                    <= threat;
            if u.garrison.is_some() {
                // Leaving a house is physical placement; route searches use the owner view.
                let nav = physical_navigation(&self.world.game);
                let c = crate::unit_spawn::SpawnPlacer::new()
                    .nearest_free(nav, u.at(), crate::game::MOVE, None)
                    .map_err(|e| format!("garrison exit: {e:?}"))?;
                u.x = (c % nav.width) as f64 * 2.0 + 1.0;
                u.z = (c / nav.width) as f64 * 2.0 + 1.0;
                u.garrison = None;
                self.world.game.units.insert(r.id, u.clone());
                self.grid.update(r.id, &u);
            }
            let result = self.command_route(owner, &u, r.at)?;
            self.commands.requests.clear_failures(r.id);
            u.pace = pace;
            u.traffic = None;
            u.traffic_wait = 0.0;
            u.traffic_entered = false;
            u.traffic_trail.clear();
            u.entrench = false;
            u.attack = false;
            u.stuck = 0.0;
            u.retreating = false;
            u.dig = false;
            u.board = 0;
            u.build = false;
            u.amove = attack_move.then_some(r.at);
            u.drift = false;
            u.world_goal = (!attack_move).then_some(r.at);
            u.path = result.waypoints;
            u.path_gen += 1;
            u.reverse = None;
            if !self.rules.kinds[u.kind].infantry
                && engaged
                && let Some(end) = u.path.last().copied()
                && crate::math::hypot(end.x - u.x, end.z - u.z) <= reverse_max
                && crate::math::cos(crate::math::atan2(end.z - u.z, end.x - u.x) - u.rot) < -0.5
            {
                u.reverse = Some(end);
            }

            if let Some(reserve) = self.logistics.get_mut(r.id).filter(|r| r.forced) {
                u.retreating = !u.path.is_empty();
                reserve.stranded = u.path.is_empty();
            }
            let c = self.commands.units.get_mut(&r.id).expect("control");
            c.orders.clear();
            c.face = face.map(|angle| Facing { angle, at: r.at });
            c.attack_id = 0;
            c.nade = None;
            c.enter = None;
            c.fire_at = None;
            c.rally_drift = false;
            c.route_end = Some(result.route_end);
            c.outcome = Some(MoveOutcome::Moving);
            c.outcome_tick = Some(self.world.game.tick);
            if self.logistics.get(r.id).is_some_and(|s| s.forced) {
                c.supply_escape = true;
            }
            if let Some(firing) = self.firing.units.get_mut(&r.id) {
                firing.target_id = 0;
            }
            self.world.game.units.insert(r.id, u);
            self.grid.update(r.id, &self.world.game.units[&r.id]);
        }
        Ok(None)
    }
    pub(crate) fn command_route(
        &mut self,
        owner: usize,
        u: &Unit,
        to: Point,
    ) -> Result<crate::pathfinding::PathResult, String> {
        self.refresh_recipient_routing(owner)?;
        let query = self.recipients[owner].query();
        let terrain = query
            .path_terrain(self.recipients[owner].live())
            .map_err(|e| format!("recipient path: {e:?}"))?;
        let rules = crate::pathfinding::PathRules::compiled().map_err(|e| e.to_string())?;
        let kind = &self.rules.kinds[u.kind];
        search_route(&mut self.commands.paths, terrain, rules, kind, u, to).map(|r| r.result)
    }
}
struct CommandRoute {
    result: crate::pathfinding::PathResult,
    failed: bool,
}
fn search_route(
    paths: &mut Pathfinding,
    terrain: crate::pathfinding::PathTerrain<'_>,
    rules: crate::pathfinding::PathRules,
    kind: &crate::rules::UnitKind,
    u: &Unit,
    to: Point,
) -> Result<CommandRoute, String> {
    let mut result = paths
        .find_path(terrain, rules, kind, u.at(), to, None)
        .map_err(|e| format!("command path: {e:?}"))?;
    let mut failed = result.status == crate::pathfinding::PathStatus::Unreachable;
    if result.waypoints.first().is_some_and(|p| {
        !crate::pathfinding::walkable(terrain.navigation, u.at(), *p, kind.block(), 0.9)
    }) {
        let nav = terrain.navigation;
        let c = cell_at(nav, u.at()).unwrap_or(0);
        let center = Point {
            x: (c % nav.width) as f64 * 2.0 + 1.0,
            z: (c / nav.width) as f64 * 2.0 + 1.0,
        };
        for at in [
            Point {
                x: center.x,
                z: u.z,
            },
            Point {
                x: u.x,
                z: center.z,
            },
            center,
        ] {
            if crate::math::hypot(u.x - at.x, u.z - at.z) < 0.05
                || !crate::pathfinding::walkable(nav, u.at(), at, kind.block(), 0.9)
            {
                continue;
            }
            let next = paths
                .find_path(terrain, rules, kind, at, to, None)
                .map_err(|e| format!("bank path: {e:?}"))?;
            failed |= next.status == crate::pathfinding::PathStatus::Unreachable;
            if next
                .waypoints
                .first()
                .is_some_and(|p| crate::pathfinding::walkable(nav, at, *p, kind.block(), 0.9))
            {
                result = next;
                result.waypoints.insert(0, at);
                break;
            }
        }
    }
    Ok(CommandRoute { result, failed })
}
fn physical_navigation(game: &crate::game::Game) -> crate::navigation::TerrainView<'_> {
    crate::navigation::TerrainView {
        width: game.w,
        height: game.h,
        flags: &game.flags,
        heights: game.height.as_deref(),
        world_known: false,
        knowledge_id: 0,
        versions: crate::navigation::Versions::default(),
        changed: None,
        changes: &[],
    }
}

fn cell_at(v: crate::navigation::TerrainView<'_>, at: Point) -> Option<usize> {
    let x = (at.x / 2.0).floor();
    let y = (at.z / 2.0).floor();
    (x >= 0.0 && y >= 0.0 && x < v.width as f64 && y < v.height as f64)
        .then(|| y as usize * v.width + x as usize)
}

struct RequestContext<'a> {
    match_state: &'a mut crate::match_runtime::Match,
    shootable: BTreeMap<UnitId, bool>,
}
impl crate::path_requests::PathRequestContext for RequestContext<'_> {
    type Error = String;
    fn current_goal(
        &mut self,
        id: UnitId,
        kind: crate::path_requests::RequestKind,
    ) -> Option<Point> {
        use crate::path_requests::RequestKind;
        let u = self.match_state.world.game.units.get(&id)?;
        if u.hp <= 0.0 {
            return None;
        }
        match kind {
            RequestKind::Recovery => (!u.path.is_empty() && u.stuck > 0.75 && u.traffic.is_none())
                .then(|| u.world_goal.or(u.amove).or_else(|| u.path.last().copied()))
                .flatten(),
            RequestKind::World => u.world_goal.filter(|at| {
                u.path.is_empty() && crate::math::hypot(at.x - u.x, at.z - u.z) > 0.25
            }),
            RequestKind::Nade => self
                .match_state
                .commands
                .units
                .get(&id)
                .and_then(|c| c.nade),
            RequestKind::Amove if !u.retreating => u.amove.filter(|at| {
                u.path.is_empty()
                    && crate::math::hypot(at.x - u.x, at.z - u.z) >= 2.5
                    && !self.shootable.get(&id).copied().unwrap_or(false)
            }),
            _ => None,
        }
    }
    fn unit_incarnation(&self, id: UnitId) -> Option<u64> {
        self.match_state
            .world
            .game
            .units
            .contains_key(&id)
            .then_some(u64::from(id))
    }
    fn knowledge_version(&self, id: UnitId) -> u64 {
        self.match_state.world.game.units.get(&id).map_or(0, |u| {
            self.match_state.recipients[u.owner]
                .query()
                .navigation()
                .versions
                .knowledge
        })
    }
    fn search(
        &mut self,
        r: crate::path_requests::Request,
    ) -> Result<crate::path_requests::SearchOutcome, String> {
        let u = self.match_state.world.game.units[&r.unit].clone();
        self.match_state.refresh_recipient_routing(u.owner)?;
        let query = self.match_state.recipients[u.owner].query();
        let terrain = query
            .path_terrain(self.match_state.recipients[u.owner].live())
            .map_err(|e| format!("recipient path: {e:?}"))?;
        let path_rules = crate::pathfinding::PathRules::compiled().map_err(|e| e.to_string())?;
        let outcome = search_route(
            &mut self.match_state.commands.paths,
            terrain,
            path_rules,
            &self.match_state.rules.kinds[u.kind],
            &u,
            r.goal,
        )?;
        let route = outcome.result;
        self.match_state
            .commands
            .units
            .get_mut(&r.unit)
            .ok_or("missing route control")?
            .route_end = Some(route.route_end);
        Ok(crate::path_requests::SearchOutcome {
            failed: outcome.failed,
            path: route.waypoints,
        })
    }
    fn apply(&mut self, update: crate::path_requests::PathUpdate) {
        let u = self
            .match_state
            .world
            .game
            .units
            .get_mut(&update.request.unit)
            .expect("searched unit");
        u.repath = update.repath;
        if let Some(path) = update.path {
            u.path = path;
            u.path_gen += 1;
        }
        if let Some(drop) = update.dropped {
            match update.request.kind {
                crate::path_requests::RequestKind::World => u.world_goal = None,
                crate::path_requests::RequestKind::Amove => u.amove = None,
                crate::path_requests::RequestKind::Nade => {
                    self.match_state.commands.units.get_mut(&u.id).unwrap().nade = None;
                    let ability = self.match_state.abilities.get_mut(u.id).unwrap();
                    ability.aimed_auto = false;
                    ability.aimed_walked = false;
                }
                _ => {}
            }
            u.stuck = 0.0;
            if let Some(c) = self.match_state.commands.units.get_mut(&u.id) {
                c.outcome = Some(MoveOutcome::Unreachable);
                c.outcome_tick = Some(drop.tick as i64);
            }
        }
    }
}
impl crate::match_runtime::Match {
    fn command_shootable(&self) -> Result<BTreeMap<UnitId, bool>, String> {
        self.world
            .game
            .units
            .keys()
            .map(|&id| {
                let target = self.firing.units.get(&id).map_or(0, |f| f.target_id);
                Ok((id, target != 0 && self.can_shoot_target(id, target)?))
            })
            .collect()
    }
    pub fn begin_path_tick(
        &mut self,
        budget: usize,
        retry_limit: u32,
    ) -> Result<crate::path_requests::TickReport, String> {
        if self.world.game.tick < 0 {
            return Err("negative path tick".into());
        }
        if self
            .world
            .game
            .units
            .values()
            .any(|u| u.path_gen == u64::MAX)
        {
            return Err("unit path identity exhausted".into());
        }
        for owner in 0..self.start.players.len() {
            self.refresh_recipient_routing(owner)?;
        }
        let shootable = self.command_shootable()?;
        let config = crate::path_requests::TickConfig {
            budget,
            retry_limit,
            tick: self.world.game.tick as u64,
            tick_seconds: self.rules.movement.tick,
        };
        let mut requests = std::mem::take(&mut self.commands.requests);
        let result = requests.begin_tick(
            config,
            &mut RequestContext {
                match_state: self,
                shootable,
            },
        );
        self.commands.requests = requests;
        if result.is_ok() {
            self.commands.path_tick = Some(self.world.game.tick);
        }
        result.map_err(|e| format!("path tick: {e:?}"))
    }
    /// Source pre-timer world completion/retry and per-unit waiting-order phase.
    pub fn prepare_unit_commands(
        &mut self,
        id: UnitId,
        winner_decided: bool,
    ) -> Result<(), String> {
        let Some(u) = self.world.game.units.get(&id) else {
            return Ok(());
        };
        if u.hp <= 0.0
            || self.rules.kinds[u.kind].structure
            || self.commands.units.get(&id).is_some_and(|c| c.operative)
        {
            return Ok(());
        }
        if let Some(goal) = u.world_goal {
            let end = self
                .commands
                .units
                .get(&id)
                .and_then(|c| c.route_end)
                .unwrap_or(goal);
            if crate::math::hypot(u.x - end.x, u.z - end.z) <= 0.25
                && u.path.is_empty()
                && u.move_speed.abs() < 0.1
            {
                let u = self.world.game.units.get_mut(&id).expect("unit");
                u.world_goal = None;
                if u.retreating {
                    u.path.clear();
                    u.retreating = false;
                }
                let c = self
                    .commands
                    .units
                    .get_mut(&id)
                    .ok_or("missing completion control")?;
                c.outcome = Some(MoveOutcome::Arrived);
                c.outcome_tick = Some(self.world.game.tick);
            } else if u.path.is_empty()
                && crate::math::hypot(u.x - goal.x, u.z - goal.z) > 0.25
                && u.repath <= 0.0
            {
                self.request_command_path(id, goal, crate::path_requests::RequestKind::World)?;
            }
        }
        self.start_queued_orders(id, winner_decided)
    }
    pub fn start_queued_orders(&mut self, id: UnitId, winner_decided: bool) -> Result<(), String> {
        let Some(u) = self.world.game.units.get(&id) else {
            return Ok(());
        };
        if u.air.is_some() {
            return Err("queued aircraft commands are unsupported".into());
        }
        let control = self
            .commands
            .units
            .get(&id)
            .ok_or("missing queued control")?;
        if !control.orders.is_empty() && u.drift && !control.rally_drift {
            if u.path_gen.checked_add(2).is_none() {
                return Err("unit path identity exhausted".into());
            }
            let u = self.world.game.units.get_mut(&id).expect("unit");
            u.drift = false;
            u.path.clear();
            u.path_gen += 1;
            u.reverse = None;
        }
        loop {
            let u = &self.world.game.units[&id];
            let c = &self.commands.units[&id];
            let attack = c.attack_id != 0
                && self.world.game.players[u.owner].sees(c.attack_id)
                && self.world.game.units.contains_key(&c.attack_id);
            if u.retreating
                || attack
                || c.fire_at.is_some()
                || c.nade.is_some()
                || u.dig
                || u.entrench
                || u.build
                || c.enter.is_some()
                || u.board != 0
                || u.amove.is_some()
                || u.world_goal.is_some()
                || !u.path.is_empty()
                || c.orders.is_empty()
            {
                break;
            }
            let owner = u.owner;
            let c = self.commands.units.get_mut(&id).expect("control");
            let order = c.orders.pop_front().expect("nonempty");
            let remaining = std::mem::take(&mut c.orders);
            let result = self.execute_command(owner, &order, winner_decided);
            self.commands.units.get_mut(&id).expect("control").orders = remaining;
            result?;
        }
        Ok(())
    }
    /// Source attack-move branch after timer decrement, before physical movement.
    pub fn apply_attack_move(&mut self, id: UnitId) -> Result<(), String> {
        let Some(u) = self.world.game.units.get(&id) else {
            return Ok(());
        };
        let Some(goal) = u.amove else {
            return Ok(());
        };
        if crate::math::hypot(u.x - goal.x, u.z - goal.z) < 2.5 {
            self.world.game.units.get_mut(&id).expect("unit").amove = None;
            return Ok(());
        }
        let target = self.firing.units.get(&id).map_or(0, |f| f.target_id);
        if target != 0 && self.can_shoot_target(id, target)? {
            if u.path_gen == u64::MAX {
                return Err("unit path identity exhausted".into());
            }
            let u = self.world.game.units.get_mut(&id).expect("unit");
            u.path.clear();
            u.path_gen += 1;
        } else if u.path.is_empty() && u.repath <= 0.0 {
            self.request_command_path(id, goal, crate::path_requests::RequestKind::Amove)?;
        }
        Ok(())
    }
    pub fn request_command_path(
        &mut self,
        id: UnitId,
        goal: Point,
        kind: crate::path_requests::RequestKind,
    ) -> Result<crate::path_requests::Disposition, String> {
        if !matches!(
            kind,
            crate::path_requests::RequestKind::Recovery
                | crate::path_requests::RequestKind::World
                | crate::path_requests::RequestKind::Amove
                | crate::path_requests::RequestKind::Nade
        ) {
            return Err("unsupported command path request".into());
        }
        if self.commands.path_tick != Some(self.world.game.tick) {
            return Err("begin_path_tick must run before path requests".into());
        }
        if self
            .world
            .game
            .units
            .get(&id)
            .is_some_and(|u| u.path_gen == u64::MAX)
        {
            return Err("unit path identity exhausted".into());
        }
        let shootable = self.command_shootable()?;
        let mut requests = std::mem::take(&mut self.commands.requests);
        let result = requests.request(
            crate::path_requests::Request {
                unit: id,
                goal,
                kind,
            },
            &mut RequestContext {
                match_state: self,
                shootable,
            },
        );
        self.commands.requests = requests;
        result.map_err(|e| format!("command path request: {e:?}"))
    }
}

#[cfg(test)]
mod tests;

struct MovingRequestContext<'a, 'terrain> {
    unit: &'a mut Unit,
    control: &'a mut UnitControl,
    paths: &'a mut Pathfinding,
    terrain: crate::pathfinding::PathTerrain<'terrain>,
    profile: &'a crate::rules::UnitKind,
    rules: crate::pathfinding::PathRules,
}
impl crate::path_requests::PathRequestContext for MovingRequestContext<'_, '_> {
    type Error = String;
    fn current_goal(
        &mut self,
        id: UnitId,
        kind: crate::path_requests::RequestKind,
    ) -> Option<Point> {
        let u = &self.unit;
        (id == u.id
            && u.hp > 0.0
            && kind == crate::path_requests::RequestKind::Recovery
            && !u.path.is_empty()
            && u.stuck > 0.75
            && u.traffic.is_none())
        .then(|| u.world_goal.or(u.amove).or_else(|| u.path.last().copied()))
        .flatten()
    }
    fn unit_incarnation(&self, id: UnitId) -> Option<u64> {
        (id == self.unit.id).then_some(u64::from(id))
    }
    fn knowledge_version(&self, _id: UnitId) -> u64 {
        self.terrain.navigation.versions.knowledge
    }
    fn search(
        &mut self,
        request: crate::path_requests::Request,
    ) -> Result<crate::path_requests::SearchOutcome, String> {
        let route = search_route(
            self.paths,
            self.terrain,
            self.rules,
            self.profile,
            self.unit,
            request.goal,
        )?;
        self.control.route_end = Some(route.result.route_end);
        Ok(crate::path_requests::SearchOutcome {
            path: route.result.waypoints,
            failed: route.failed,
        })
    }
    fn apply(&mut self, update: crate::path_requests::PathUpdate) {
        self.unit.repath = update.repath;
        if let Some(path) = update.path {
            self.unit.path = path;
            self.unit.path_gen += 1;
        }
    }
}
impl crate::match_runtime::Match {
    /// Execute one movement block after begin_path_tick on this tick.
    /// A search error aborts the tick; movement may already have changed the unit.
    pub fn move_unit_with_command_routes(
        &mut self,
        id: UnitId,
        suppression_speed: f64,
    ) -> Result<Option<crate::movement::Moved>, String> {
        if self.commands.path_tick != Some(self.world.game.tick) {
            return Err("begin_path_tick must run before movement".into());
        }
        if !suppression_speed.is_finite() || suppression_speed < 0.0 {
            return Err("invalid suppression speed".into());
        }
        let Some(unit) = self.world.game.units.get(&id) else {
            return Ok(None);
        };
        if unit.path_gen.checked_add(2).is_none() {
            return Err("unit path identity exhausted".into());
        }
        let owner = unit.owner;
        let kind = unit.kind;
        self.refresh_recipient_routing(owner)?;
        let query = self.recipients[owner].query();
        let terrain = query
            .path_terrain(self.recipients[owner].live())
            .map_err(|e| format!("movement recipient path: {e:?}"))?;
        let path_rules = crate::pathfinding::PathRules::compiled().map_err(|e| e.to_string())?;
        let profile = self.rules.kinds.get(kind).ok_or("unknown moving kind")?;
        let control = self
            .commands
            .units
            .get_mut(&id)
            .ok_or("missing moving control")?;
        let mut error = None;
        let moved = crate::movement::move_unit_with_routes(
            &mut self.world.game,
            &self.rules,
            &mut self.grid,
            id,
            suppression_speed,
            &mut |_, _, u, to, why| {
                if error.is_some() {
                    return;
                }
                match why {
                    crate::movement::RouteRequest::Recovery => {
                        let mut context = MovingRequestContext {
                            unit: u,
                            control,
                            paths: &mut self.commands.paths,
                            terrain,
                            profile,
                            rules: path_rules,
                        };
                        if let Err(e) = self.commands.requests.request(
                            crate::path_requests::Request {
                                unit: id,
                                goal: to,
                                kind: crate::path_requests::RequestKind::Recovery,
                            },
                            &mut context,
                        ) {
                            error = Some(format!("movement recovery: {e:?}"));
                        }
                    }
                    crate::movement::RouteRequest::Immediate => {
                        self.commands.requests.clear_failures(id);
                        match search_route(
                            &mut self.commands.paths,
                            terrain,
                            path_rules,
                            profile,
                            u,
                            to,
                        ) {
                            Ok(route) => {
                                control.route_end = Some(route.result.route_end);
                                if !route.result.waypoints.is_empty() {
                                    u.path = route.result.waypoints;
                                    u.path_gen += 1;
                                }
                                u.repath = 1.0;
                            }
                            Err(e) => error = Some(e),
                        }
                    }
                }
            },
        );
        if let Some(error) = error {
            return Err(error);
        }
        Ok(moved)
    }
}
