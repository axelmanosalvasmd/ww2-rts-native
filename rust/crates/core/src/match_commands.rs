//! Canonical control intent and tick-boundary commands for native Conquest.

use crate::{
    catalog::Catalog,
    economy::{self, AbilityCost, Mode},
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
    pub unit: UnitId,
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
        attack_move: bool,
        queue: bool,
        together: bool,
        face: Option<f64>,
    },
    Stop {
        ids: Vec<UnitId>,
    },
    Stance {
        ids: Vec<UnitId>,
        stance: Stance,
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
        Self::new_for_mode(unit, rules, Mode::Default)
    }
    /// Paid construction-mode abilities start off; free abilities start on.
    pub fn new_for_mode(unit: &Unit, rules: &Rules, mode: Mode) -> Result<Self, String> {
        let kind = rules
            .kinds
            .get(unit.kind)
            .ok_or("unknown control unit kind")?;
        let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
        let definition = catalog.unit(&kind.name).map_err(|e| e.to_string())?;
        let ability = definition.get("ab").ok_or("missing unit ability")?;
        let ability_id = ability
            .get("id")
            .and_then(|v| v.string())
            .ok_or("missing unit ability")?;
        let cost = economy::ab_cost(
            mode,
            AbilityCost {
                id: ability_id,
                munitions: ability.get("mun").and_then(|v| v.number()),
            },
        )
        .map_err(|e| e.to_string())?;
        if !unit.rot.is_finite() {
            return Err("nonfinite unit aim".into());
        }
        Ok(Self {
            aim: unit.rot,
            auto_cast: ability_id != "none" && cost == 0.0,
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
        }
    }
    pub fn next_tick(&self) -> Option<u64> {
        self.queue.next_tick()
    }
    pub fn pending_commands(&self) -> usize {
        self.queue.len()
    }
    pub fn path_requests(&self) -> &PathRequests {
        &self.requests
    }
    /// Publish an already prepared row after the caller commits its spawn transaction.
    pub fn insert(&mut self, id: UnitId, row: UnitControl) {
        self.units.insert(id, row);
    }
}
