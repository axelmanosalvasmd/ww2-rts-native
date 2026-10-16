//! Private scripted-seat scheduling and bounded movement. No room or mode admission.
use std::collections::{BTreeMap, VecDeque};

use crate::{
    commander_ai::Level,
    field_works::Dig,
    game::Point,
    match_commands::{Command, MoveOrder, MoveOutcome},
    match_runtime::Match,
};

pub const ORDER_BATCH: usize = 4;
pub const ORDER_LIMIT: usize = 240;

/// Only the owner's intent, detached at observation time. Position and health
/// changes alone do not supersede a queued order; new routes and Stop do.
#[derive(Clone, Debug, PartialEq)]
pub struct OwnPlan {
    pub path_generation: u64,
    pub goal: Option<Point>,
    pub route_end: Option<Point>,
    pub outcome: Option<MoveOutcome>,
    pub outcome_tick: Option<i64>,
    pub path: Vec<Point>,
    pub amove: Option<Point>,
    pub attack: u32,
    pub target: u32,
    pub retreating: bool,
    pub orders: VecDeque<Command>,
    pub dig: Option<Dig>,
    pub entrench: bool,
    pub build: Option<u32>,
    pub enter: Option<usize>,
    pub fire_at: Option<usize>,
    pub nade: Option<Point>,
    pub hold: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OwnUnit {
    pub id: u32,
    pub owner: usize,
    pub hp: f64,
    pub air: bool,
    pub structure: bool,
    pub plan: OwnPlan,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Wave {
    pub slot: usize,
    pub number: u32,
    pub active: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Live {
    pub tick: i64,
    pub winner: bool,
    pub wave: Option<Wave>,
}

/// A seat-owned packet. Enemy inputs contain only a visible bunker destination.
#[derive(Clone, Debug, PartialEq)]
pub struct View {
    pub tick: i64,
    pub wave: Wave,
    pub units: BTreeMap<u32, OwnUnit>,
    pub bunker: Option<Point>,
}

#[derive(Clone, Debug, PartialEq)]
struct Pending {
    id: u32,
    before: OwnPlan,
    command: Command,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HordeCommander {
    owner: usize,
    delivered: Option<View>,
    wave: Option<u32>,
    pending: VecDeque<Pending>,
    budget_tick: Option<i64>,
    issued: usize,
    last_think: Option<i64>,
}

impl HordeCommander {
    pub fn new(owner: usize) -> Self {
        Self {
            owner,
            delivered: None,
            wave: None,
            pending: VecDeque::new(),
            budget_tick: None,
            issued: 0,
            last_think: None,
        }
    }
    pub fn pending(&self) -> usize {
        self.pending.len()
    }
    pub fn delivered(&self) -> Option<&View> {
        self.delivered.as_ref()
    }
    pub fn due(&self, tick: i64) -> bool {
        Level::Normal.due(tick, self.owner)
    }
    /// The shared planner demand remains useful to non-Horde scripted callers.
    pub fn planner_wants_view(&self, tick: i64) -> bool {
        Level::Normal.wants_view(tick, self.owner)
    }
    /// Horde always observes on the caller's delivered snapshot beat.
    pub fn wants_view(&self, sent: bool) -> bool {
        sent
    }
    fn clear(&mut self) {
        self.pending.clear();
        self.wave = None;
        self.budget_tick = None;
        self.issued = 0;
    }
    fn active(&mut self, live: Live, observed: Wave) -> bool {
        let valid = !live.winner
            && live
                .wave
                .is_some_and(|wave| wave.slot == self.owner && wave.active && wave == observed);
        if !valid {
            self.clear();
            return false;
        }
        if self.wave != Some(observed.number) {
            self.clear();
            self.wave = Some(observed.number);
        }
        true
    }
    fn flush(&mut self, view: &View) -> Vec<Command> {
        if self.budget_tick != Some(view.tick) {
            self.budget_tick = Some(view.tick);
            self.issued = 0;
        }
        let mut commands = Vec::new();
        while let Some(job) = self.pending.front() {
            let valid = view.units.get(&job.id).is_some_and(|unit| {
                unit.owner == self.owner
                    && unit.hp > 0.0
                    && !unit.air
                    && !unit.plan.retreating
                    && unit.plan == job.before
            });
            if !valid {
                self.pending.pop_front();
                continue;
            }
            if self.issued == ORDER_BATCH {
                break;
            }
            commands.push(self.pending.pop_front().expect("front exists").command);
            self.issued += 1;
        }
        commands
    }
    /// Refreshing the observation drains pending work before the next think.
    pub fn observe(&mut self, live: Live, view: View) -> Result<Vec<Command>, String> {
        if view.tick < 0
            || view.tick > live.tick
            || view.wave.slot != self.owner
            || self
                .delivered
                .as_ref()
                .is_some_and(|old| old.tick > view.tick)
        {
            return Err("invalid Horde observation boundary".into());
        }
        let commands = if self.active(live, view.wave) {
            self.flush(&view)
        } else {
            Vec::new()
        };
        self.delivered = Some(view);
        Ok(commands)
    }
    /// Submit scripted movement, splitting into one unit per ordinary command.
    /// This queue belongs to the planner, never to UnitControl.orders.
    pub fn submit(&mut self, live: Live, command: Command) -> Result<Vec<Command>, String> {
        let Some(view) = self.delivered.clone() else {
            return Ok(Vec::new());
        };
        if !self.active(live, view.wave) {
            return Ok(Vec::new());
        }
        let Command::Move {
            orders,
            queue: false,
            ..
        } = &command
        else {
            return Ok(vec![command]);
        };
        let mut direct = Vec::new();
        for order in orders {
            let unit = u32::try_from(order.unit)
                .ok()
                .and_then(|id| view.units.get(&id));
            let Some(unit) = unit.filter(|u| u.owner == self.owner && !u.air && !u.structure)
            else {
                direct.push(order.clone());
                continue;
            };
            let mut single = command.clone();
            if let Command::Move { orders, .. } = &mut single {
                *orders = vec![order.clone()];
            }
            let job = Pending {
                id: unit.id,
                before: unit.plan.clone(),
                command: single,
            };
            if let Some(existing) = self.pending.iter_mut().find(|p| p.id == unit.id) {
                *existing = job;
            } else if self.pending.len() < ORDER_LIMIT {
                self.pending.push_back(job);
            }
        }
        let mut commands = Vec::new();
        if !direct.is_empty() {
            let mut command = command;
            if let Command::Move { orders, .. } = &mut command {
                *orders = direct;
            }
            commands.push(command);
        }
        commands.extend(self.flush(&view));
        Ok(commands)
    }
    /// Minimal wave movement caller. Broader Horde support/ability policy is separate.
    /// The live wave guard runs even between observations and before planning.
    pub fn act(&mut self, live: Live) -> Result<Vec<Command>, String> {
        let Some(wave) = self.delivered.as_ref().map(|view| view.wave) else {
            self.clear();
            return Ok(Vec::new());
        };
        if !self.active(live, wave) || !self.due(live.tick) || self.last_think == Some(live.tick) {
            return Ok(Vec::new());
        }
        self.last_think = Some(live.tick);
        let view = self.delivered.as_ref().expect("delivered wave");
        let Some(at) = view.bunker else {
            return Ok(Vec::new());
        };
        let orders = view
            .units
            .values()
            .filter(|u| {
                let p = &u.plan;
                u.owner == self.owner
                    && u.hp > 0.0
                    && !u.air
                    && !u.structure
                    && !p.retreating
                    && p.path.is_empty()
                    && p.amove.is_none()
                    && p.goal.is_none()
                    && p.attack == 0
                    && p.target == 0
                    && p.dig.is_none()
                    && !p.entrench
                    && p.build.is_none()
                    && p.enter.is_none()
                    && p.fire_at.is_none()
                    && p.nade.is_none()
            })
            .map(|u| MoveOrder {
                unit: i64::from(u.id),
                at,
            })
            .collect::<Vec<_>>();
        self.submit(
            live,
            Command::Move {
                orders,
                control_ids: None,
                attack_move: true,
                queue: false,
                together: false,
                face: None,
            },
        )
    }
}

impl Match {
    pub fn horde_commander_live(&self) -> Live {
        Live {
            tick: self.world.game.tick,
            winner: self.finish.is_some(),
            wave: self
                .horde
                .as_ref()
                .filter(|_| {
                    self.options.mode == crate::economy::Mode::Horde
                        && self.world.game.mode == Some(crate::game::ModeKind::Horde)
                })
                .map(|h| Wave {
                    slot: h.slot,
                    number: h.wave,
                    active: h.active,
                }),
        }
    }
    /// Own intent only. No foreign path, economy, support or hidden target position.
    pub(crate) fn horde_own_unit(&self, id: u32, owner: usize) -> Result<OwnUnit, String> {
        let u = self
            .world
            .game
            .units
            .get(&id)
            .filter(|u| u.owner == owner)
            .ok_or("Horde intent requires an owned unit")?;
        let c = self.commands.units.get(&id).ok_or("missing own control")?;
        let target = self
            .firing
            .units
            .get(&id)
            .ok_or("missing own firing state")?
            .target_id;
        let seen = |target| {
            self.world.game.units.get(&target).is_some_and(|unit| {
                self.world.game.reveal
                    || self.world.game.allied(owner, unit.owner)
                    || (self.world.game.players[owner].sees(target)
                        && (unit.air.is_none() || crate::vision::airborne(unit)))
            })
        };
        Ok(OwnUnit {
            id,
            owner,
            hp: u.hp,
            air: u.air.is_some(),
            structure: self.rules.kinds[u.kind].structure,
            plan: OwnPlan {
                path_generation: u.path_gen,
                goal: u.world_goal,
                route_end: c.route_end,
                outcome: c.outcome,
                outcome_tick: c.outcome_tick,
                path: u.path.clone(),
                amove: u.amove,
                attack: if seen(c.attack_id) { c.attack_id } else { 0 },
                target: if seen(target) { target } else { 0 },
                retreating: u.retreating,
                orders: c.orders.clone(),
                dig: self.field_works.jobs().get(&id).cloned(),
                entrench: u.entrench,
                build: c.build_target,
                enter: c.enter,
                fire_at: c.fire_at,
                nade: c.nade,
                hold: u.hold_pos,
            },
        })
    }
    /// Explicit core component observation; it does not open recipient snapshots.
    pub fn horde_commander_observation(&self, owner: usize) -> Result<View, String> {
        let wave = self
            .horde_commander_live()
            .wave
            .filter(|w| w.slot == owner)
            .ok_or("not the scripted Horde seat")?;
        let g = &self.world.game;
        let spawn = self.start.players[owner].spawn;
        let mut bunkers = g
            .units
            .values()
            .filter(|u| {
                u.hp > 0.0
                    && self.rules.kinds[u.kind].name == "bunker"
                    && !g.allied(owner, u.owner)
                    && g.players[owner].sees(u.id)
            })
            .collect::<Vec<_>>();
        bunkers.sort_by(|a, b| {
            crate::math::hypot(a.x - spawn.x, a.z - spawn.z)
                .total_cmp(&crate::math::hypot(b.x - spawn.x, b.z - spawn.z))
                .then(a.id.cmp(&b.id))
        });
        let round = |v| crate::math::round(v * 10.0) / 10.0;
        Ok(View {
            tick: g.tick,
            wave,
            units: g
                .units
                .values()
                .filter(|u| u.owner == owner)
                .map(|u| self.horde_own_unit(u.id, owner).map(|row| (u.id, row)))
                .collect::<Result<_, _>>()?,
            bunker: bunkers.first().map(|u| Point {
                x: round(u.x),
                z: round(u.z),
            }),
        })
    }
}

#[cfg(test)]
#[path = "horde_commander_tests.rs"]
mod tests;
