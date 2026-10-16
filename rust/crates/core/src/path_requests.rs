//! FIFO path requests and failure retries from `shared/sim.js` at `e4e579a`.
//!
//! The caller owns live-order eligibility, actual path searches and unit updates.
//! This scheduler does not choose terrain or implement combat/order conditions.

use std::collections::{BTreeMap, VecDeque};

use crate::game::Point;
use crate::grid::UnitId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum RequestKind {
    Recovery,
    World,
    Build,
    Dig,
    Nade,
    Enter,
    Board,
    Amove,
    FireAt,
    AttackId,
}

impl RequestKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Recovery => "recovery",
            Self::World => "world",
            Self::Build => "build",
            Self::Dig => "dig",
            Self::Nade => "nade",
            Self::Enter => "enter",
            Self::Board => "board",
            Self::Amove => "amove",
            Self::FireAt => "fireAt",
            Self::AttackId => "attackId",
        }
    }
    pub fn order_field(self) -> &'static str {
        if self == Self::World {
            "worldGoal"
        } else {
            self.name()
        }
    }
    fn cleared_value(self) -> ClearedValue {
        match self {
            Self::Build | Self::AttackId | Self::Board => ClearedValue::Zero,
            Self::Enter | Self::FireAt => ClearedValue::MinusOne,
            _ => ClearedValue::Null,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Request {
    pub unit: UnitId,
    pub kind: RequestKind,
    pub goal: Point,
}

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
pub struct TickConfig {
    pub budget: usize,
    pub retry_limit: u32,
    pub tick: u64,
    pub tick_seconds: f64,
}
impl Default for TickConfig {
    fn default() -> Self {
        Self {
            budget: 4096,
            retry_limit: 20,
            tick: 0,
            tick_seconds: 0.05,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClearedValue {
    Zero,
    MinusOne,
    Null,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DropOrder {
    pub value: ClearedValue,
    pub tick: u64,
}

/// `None` retains the old route after an empty recovery result. All other
/// searches replace the route, including successful or deferred empty results.
#[derive(Clone, Debug)]
pub struct PathUpdate {
    pub request: Request,
    pub path: Option<Vec<Point>>,
    pub repath: f64,
    /// Clear the kind's order, route and stuck count; publish "unreachable" at tick.
    pub dropped: Option<DropOrder>,
}

#[derive(Clone, Debug)]
pub struct SearchOutcome {
    pub path: Vec<Point>,
    /// True when reference `stats.failed` advances, even if a later fallback
    /// returns a nonempty path. Fine-search budget exhaustion alone is deferred
    /// and must leave this false.
    pub failed: bool,
}

/// Adapter supplied by movement/order integration. `current_goal` must apply
/// the reference's live-unit and per-kind eligibility rules, including dead,
/// superseded, retreating or completed orders. It is queried again at drain time.
/// Searches are synchronous and must not replace the active unit object.
pub trait PathRequestContext {
    type Error;
    fn current_goal(&mut self, unit: UnitId, kind: RequestKind) -> Option<Point>;
    /// Distinguishes a replacement unit object reusing an ID from the prior one.
    fn unit_incarnation(&self, unit: UnitId) -> Option<u64>;
    fn knowledge_version(&self, unit: UnitId) -> u64;
    fn search(&mut self, request: Request) -> Result<SearchOutcome, Self::Error>;
    fn apply(&mut self, update: PathUpdate);
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SchedulerError<E> {
    NotStarted,
    InvalidGoal,
    InvalidTickDuration,
    MissingUnit,
    InvalidSearchResult,
    TokenExhausted,
    Search(E),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Disposition {
    Searched,
    Queued,
    AlreadyQueued,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SchedulerStats {
    /// Requests queued because the tick budget was exhausted. Search-internal
    /// deferrals belong to the caller's pathfinder statistics.
    pub deferred: u64,
    /// Orders cancelled after reaching their failure retry limit.
    pub dropped: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TickReport {
    pub searched: usize,
    pub discarded: usize,
}

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
struct Pending {
    token: u64,
    request: Request,
}
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
struct Failure {
    incarnation: u64,
    request: Request,
    version: u64,
    count: u32,
}

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct PathRequests {
    queue: VecDeque<Pending>,
    pending: BTreeMap<UnitId, Pending>,
    failures: BTreeMap<UnitId, Failure>,
    next_token: u64,
    config: Option<TickConfig>,
    used: usize,
    stats: SchedulerStats,
}

impl PathRequests {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn used(&self) -> usize {
        self.used
    }
    /// Includes stale entries, which are removed when they reach the front.
    pub fn queue_len(&self) -> usize {
        self.queue.len()
    }
    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }
    /// Borrow FIFO requests, including stale entries, for adapter preflight.
    /// This does not acknowledge pending work or change search budget.
    pub(crate) fn queued_requests(&self) -> impl Iterator<Item = &Request> {
        self.queue.iter().map(|pending| &pending.request)
    }
    pub fn stats(&self) -> SchedulerStats {
        self.stats
    }
    /// Match direct `findPath` calls, which reset earlier retry failures before
    /// searching. Step searches preserve their prior count internally first.
    pub fn clear_failures(&mut self, unit: UnitId) {
        self.failures.remove(&unit);
    }
    /// Explicitly cancel pending work and release failure state for a retired unit.
    /// Tombstones do not consume budget.
    pub fn retire_unit(&mut self, unit: UnitId) {
        self.pending.remove(&unit);
        self.failures.remove(&unit);
    }

    pub fn begin_tick<C: PathRequestContext>(
        &mut self,
        config: TickConfig,
        context: &mut C,
    ) -> Result<TickReport, SchedulerError<C::Error>> {
        if !config.tick_seconds.is_finite() || config.tick_seconds < 0.0 {
            return Err(SchedulerError::InvalidTickDuration);
        }
        self.config = Some(config);
        self.used = 0;
        let mut report = TickReport::default();
        while let Some(front) = self.queue.front().copied() {
            let current = context.current_goal(front.request.unit, front.request.kind);
            if current.is_some_and(|p| !valid_goal(p)) {
                return Err(SchedulerError::InvalidGoal);
            }
            let active = self
                .pending
                .get(&front.request.unit)
                .is_some_and(|p| p.token == front.token);
            if !active || current != Some(front.request.goal) {
                self.queue.pop_front();
                if active {
                    self.pending.remove(&front.request.unit);
                }
                report.discarded += 1;
                continue;
            }
            // Invalid, dead and superseded entries above are discarded even at zero budget.
            if self.used >= config.budget {
                break;
            }
            self.queue.pop_front();
            self.pending.remove(&front.request.unit);
            self.perform(front.request, true, context)?;
            report.searched += 1;
        }
        Ok(report)
    }

    pub fn request<C: PathRequestContext>(
        &mut self,
        request: Request,
        context: &mut C,
    ) -> Result<Disposition, SchedulerError<C::Error>> {
        let config = self.config.ok_or(SchedulerError::NotStarted)?;
        if !valid_goal(request.goal) {
            return Err(SchedulerError::InvalidGoal);
        }
        if context.unit_incarnation(request.unit).is_none() {
            return Err(SchedulerError::MissingUnit);
        }
        if self
            .pending
            .get(&request.unit)
            .is_some_and(|pending| pending.request == request)
        {
            return Ok(Disposition::AlreadyQueued);
        }
        if self.used < config.budget {
            self.perform(request, false, context)?;
            return Ok(Disposition::Searched);
        }
        self.next_token = self
            .next_token
            .checked_add(1)
            .ok_or(SchedulerError::TokenExhausted)?;
        let pending = Pending {
            token: self.next_token,
            request,
        };
        self.pending.insert(request.unit, pending);
        self.queue.push_back(pending);
        self.stats.deferred += 1;
        Ok(Disposition::Queued)
    }

    fn perform<C: PathRequestContext>(
        &mut self,
        request: Request,
        queued: bool,
        context: &mut C,
    ) -> Result<(), SchedulerError<C::Error>> {
        let config = self.config.expect("started before search");
        let incarnation = context
            .unit_incarnation(request.unit)
            .ok_or(SchedulerError::MissingUnit)?;
        let previous = self.failures.remove(&request.unit);
        self.used += 1;
        let result = context.search(request).map_err(SchedulerError::Search)?;
        if result.path.iter().any(|&p| !valid_goal(p)) {
            return Err(SchedulerError::InvalidSearchResult);
        }
        let mut update = PathUpdate {
            request,
            path: Some(result.path),
            repath: 1.0 + if queued { config.tick_seconds } else { 0.0 },
            dropped: None,
        };
        if request.kind == RequestKind::Recovery {
            if update.path.as_ref().is_some_and(Vec::is_empty) {
                update.path = None;
            }
            context.apply(update);
            return Ok(());
        }
        if result.failed {
            let version = context.knowledge_version(request.unit);
            let count = previous
                .filter(|old| {
                    old.incarnation == incarnation
                        && old.request == request
                        && old.version == version
                })
                .map_or(1, |old| old.count + 1);
            if count < config.retry_limit {
                self.failures.insert(
                    request.unit,
                    Failure {
                        incarnation,
                        request,
                        version,
                        count,
                    },
                );
            } else {
                update.path = Some(Vec::new());
                update.dropped = Some(DropOrder {
                    value: request.kind.cleared_value(),
                    tick: config.tick,
                });
                self.stats.dropped += 1;
            }
        }
        context.apply(update);
        Ok(())
    }
}

fn valid_goal(p: Point) -> bool {
    p.x.is_finite() && p.z.is_finite()
}
