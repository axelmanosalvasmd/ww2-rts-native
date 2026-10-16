//! Paid ordinary field-work placement and timed cell execution from pinned `shared/sim.js`.
//! Control cancellation, garrison exit, routing, story and dispatcher integration belong to callers.
use crate::{
    catalog::{Catalog, Value},
    economy_state::PlayerEconomy,
    game::Point,
    math,
    rules::Rules,
    world::{World, WorldError},
};
#[path = "match_engineering.rs"]
mod match_engineering;
#[path = "match_projects.rs"]
mod match_projects;
#[path = "match_queued_dig.rs"]
mod match_queued_dig;
#[path = "field_project_tick.rs"]
mod project_tick;
#[path = "field_work_snapshot.rs"]
mod snapshot;

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::OnceLock,
};
#[path = "field_projects.rs"]
mod projects;
pub use projects::{
    Pattern, PreparedProject, PreparedSegment, Project, ProjectId, ProjectMaterial, ProjectOrder,
    ProjectRequest, Segment, SegmentPreparation, entrench_plan, line_fort, segment_cost,
};

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum FortKind {
    Trench,
    Sandbags,
    Wire,
    Traps,
    Nest,
    Mines,
    Demine,
    Aid,
    Bridge,
    Fill,
    Wall,
    Gate,
}
impl FortKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Trench => "trench",
            Self::Sandbags => "sandbags",
            Self::Wire => "wire",
            Self::Traps => "traps",
            Self::Nest => "nest",
            Self::Mines => "mines",
            Self::Demine => "demine",
            Self::Aid => "aid",
            Self::Bridge => "bridge",
            Self::Fill => "fill",
            Self::Wall => "wall",
            Self::Gate => "gate",
        }
    }
    pub fn parse(name: &str) -> Result<Self, Error> {
        match name {
            "trench" => Ok(Self::Trench),
            "sandbags" => Ok(Self::Sandbags),
            "wire" => Ok(Self::Wire),
            "traps" => Ok(Self::Traps),
            "nest" => Ok(Self::Nest),
            "mines" => Ok(Self::Mines),
            "demine" => Ok(Self::Demine),
            "aid" => Ok(Self::Aid),
            "bridge" => Ok(Self::Bridge),
            "fill" => Ok(Self::Fill),
            "wall" => Ok(Self::Wall),
            "gate" => Ok(Self::Gate),
            _ => Err(Error::Refused(Reason::Blocked)),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    Blocked,
    NoBuilders,
    Retreating,
    Mp,
    NotVisible,
    Max,
}
impl Reason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Blocked => "blocked",
            Self::NoBuilders => "noBuilders",
            Self::Retreating => "retreating",
            Self::Mp => "mp",
            Self::NotVisible => "notVisible",
            Self::Max => "max",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Refused(Reason),
    UnsupportedKind,
    InvalidInput,
    Rules,
    World(WorldError),
}
impl From<WorldError> for Error {
    fn from(value: WorldError) -> Self {
        Self::World(value)
    }
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WorkCell {
    pub cell: usize,
    pub ch: u8,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Dig {
    pub project: Option<ProjectId>,
    pub kind: FortKind,
    pub at: Point,
    pub direction: f64,
    pub cells: Vec<WorkCell>,
    pub elapsed: f64,
    /// Written on the first in-reach work phase, as in the source's lazy `dig.n`.
    pub original_cells: Option<usize>,
    pub front: Option<f64>,
    /// Long-reach work caches an approach point on the builder's side.
    pub reach: Option<f64>,
    pub from: Option<Point>,
    /// Total occupied-cell retries across the paid concrete line.
    pub waits: u8,
    /// Retain an occupied-cell escape while the deterministic path budget is exhausted.
    pub step_back: Option<Point>,
}
#[derive(Clone, Copy, Debug)]
pub struct StartRequest {
    /// Caller supplies operative/reserve eligibility from its canonical control metadata.
    pub control_eligible: bool,
    pub owner: usize,
    pub unit: u32,
    pub kind: FortKind,
    pub at: Point,
    pub direction: Option<f64>,
}
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FieldWorksRuntime {
    jobs: BTreeMap<u32, Dig>,
    pits: BTreeSet<usize>,
    projects: projects::ProjectState,
}
/// Preparation borrows the canonical wallet and changes nothing until the infallible commit.
/// Caller stages its control/garrison changes, cancels prior work, then publishes this job.
pub struct PreparedStart<'a> {
    wallet: &'a mut PlayerEconomy,
    job: Dig,
    unit: u32,
    cost: f64,
}
impl PreparedStart<'_> {
    pub fn cost(&self) -> f64 {
        self.cost
    }
    pub fn job(&self) -> &Dig {
        &self.job
    }
    pub fn commit(self, runtime: &mut FieldWorksRuntime) {
        self.wallet.mp -= self.cost;
        runtime.jobs.insert(self.unit, self.job);
    }
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StepResult {
    /// Source `tally(owner, 'built')`, including when the last paid cell is skipped.
    pub completed: Option<usize>,
    /// Scheduler must add real Dig eligibility before acting on this request.
    pub route: Option<Point>,
    /// The Match publishes the finished building and its canonical metadata.
    pub concrete: Option<(FortKind, usize, f64)>,
}
struct Definition {
    cost: f64,
    count: usize,
    ch: u8,
    time: f64,
    reach: Option<f64>,
}
struct Definitions {
    forts: BTreeMap<FortKind, Definition>,
    rules: Rules,
    builders: Vec<String>,
}
fn definitions() -> Result<&'static Definitions, Error> {
    static D: OnceLock<Result<Definitions, Error>> = OnceLock::new();
    D.get_or_init(|| {
        let catalog = Catalog::compiled().map_err(|_| Error::Rules)?;
        let cfg = catalog.table("CFG").map_err(|_| Error::Rules)?;
        let table = catalog.table("FORTS").map_err(|_| Error::Rules)?;
        let num = |value: &Value, key: &str| {
            value
                .get(key)
                .and_then(Value::number)
                .filter(|n| n.is_finite())
                .ok_or(Error::Rules)
        };
        let default_time = num(cfg, "digTime")?;
        let builders = cfg
            .get("fortBuilders")
            .and_then(Value::array)
            .ok_or(Error::Rules)?
            .iter()
            .map(|v| v.string().map(str::to_owned).ok_or(Error::Rules))
            .collect::<Result<_, _>>()?;
        let mut forts = BTreeMap::new();
        for kind in [
            FortKind::Trench,
            FortKind::Sandbags,
            FortKind::Wire,
            FortKind::Traps,
            FortKind::Nest,
            FortKind::Mines,
            FortKind::Demine,
            FortKind::Aid,
            FortKind::Bridge,
            FortKind::Fill,
            FortKind::Wall,
            FortKind::Gate,
        ] {
            let source = table.get(kind.as_str()).ok_or(Error::Rules)?;
            let count = if kind == FortKind::Nest {
                6
            } else {
                let n = num(source, "n")?;
                if !(1.0..=6.0).contains(&n) || n.fract() != 0. {
                    return Err(Error::Rules);
                }
                n as usize
            };
            let ch = if matches!(kind, FortKind::Wall | FortKind::Gate) {
                0
            } else if kind == FortKind::Nest {
                b'T'
            } else {
                let ch = source
                    .get("ch")
                    .and_then(Value::string)
                    .ok_or(Error::Rules)?;
                if ch.len() != 1 {
                    return Err(Error::Rules);
                }
                ch.as_bytes()[0]
            };
            let time = if source.get("time").is_some() {
                num(source, "time")?
            } else {
                default_time
            };
            let reach = if source.get("reach").is_some() {
                Some(num(source, "reach")?)
            } else {
                None
            };
            let cost = num(source, "cost")?;
            if time <= 0. || cost < 0. || reach.is_some_and(|r| r <= 2.) {
                return Err(Error::Rules);
            }
            forts.insert(
                kind,
                Definition {
                    cost,
                    count,
                    ch,
                    time,
                    reach,
                },
            );
        }
        Ok(Definitions {
            forts,
            rules: Rules::compiled().map_err(|_| Error::Rules)?,
            builders,
        })
    })
    .as_ref()
    .map_err(Clone::clone)
}
fn position(at: Point) -> bool {
    at.x.is_finite()
        && at.z.is_finite()
        && at.x.abs() <= 1_073_741_824.
        && at.z.abs() <= 1_073_741_824.
}
fn center(w: &World, c: usize) -> Point {
    Point {
        x: (c % w.game.w) as f64 * 2. + 1.,
        z: (c / w.game.w) as f64 * 2. + 1.,
    }
}
pub(crate) fn square_direction(direction: f64) -> f64 {
    math::round(direction / std::f64::consts::FRAC_PI_2) * std::f64::consts::FRAC_PI_2
}
fn offsets(kind: FortKind, count: usize) -> Vec<(f64, f64, u8)> {
    if kind == FortKind::Nest {
        vec![
            (0., 0., b'T'),
            (-1., 1., b'#'),
            (0., 1., b'#'),
            (1., 1., b'#'),
            (-1., 0., b'#'),
            (1., 0., b'#'),
        ]
    } else {
        (0..count)
            .map(|i| (i as f64 - (count as f64 - 1.) / 2., 0., 0))
            .collect()
    }
}
fn mine_known(w: &World, c: usize, team: i32) -> bool {
    let owner = w.mines.get(&c).copied().unwrap_or(-1);
    owner >= 0
        && w.game
            .players
            .get(owner as usize)
            .is_some_and(|p| p.team == team)
        || w.mine_seen.get(&c).copied().unwrap_or(0) >> ((team as u32) & 31) & 1 == 1
}
/// The recipient planner and canonical admission share the same complete visibility footprint.
pub(crate) fn visible_placement(
    game: &crate::game::Game,
    kind: FortKind,
    at: Point,
    direction: f64,
    mut sees: impl FnMut(Point) -> bool,
) -> Result<bool, Error> {
    if !position(at) || !direction.is_finite() {
        return Ok(false);
    }
    let definition = &definitions()?.forts[&kind];
    let direction = if kind == FortKind::Gate {
        square_direction(direction)
    } else {
        direction
    };
    let sx = math::cos(direction);
    let sz = math::sin(direction);
    for (along, forward, _) in offsets(kind, definition.count) {
        if let Some(cell) = game.cell_of(
            at.x + (sx * along + sz * forward) * 2.,
            at.z + (sz * along - sx * forward) * 2.,
        ) && !sees(Point {
            x: (cell % game.w) as f64 * 2. + 1.,
            z: (cell / game.w) as f64 * 2. + 1.,
        }) {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Visibility is checked on every planned in-map cell before reading its buildability.
pub fn placement(
    w: &World,
    kind: FortKind,
    at: Point,
    direction: f64,
    team: Option<i32>,
    mut sees: impl FnMut(Point) -> bool,
) -> Result<Vec<WorkCell>, Error> {
    if !position(at) || !direction.is_finite() {
        return Err(Error::Refused(Reason::Blocked));
    }
    let d = &definitions()?.forts[&kind];
    let direction = if kind == FortKind::Gate {
        square_direction(direction)
    } else {
        direction
    };
    let sx = math::cos(direction);
    let sz = math::sin(direction);
    let plan = offsets(kind, d.count);
    if !visible_placement(&w.game, kind, at, direction, &mut sees)? {
        return Err(Error::Refused(Reason::NotVisible));
    }
    let mut cells = Vec::new();
    for (along, forward, ch) in plan {
        let Some(cell) = w.game.cell_of(
            at.x + (sx * along + sz * forward) * 2.,
            at.z + (sz * along + (-sx) * forward) * 2.,
        ) else {
            continue;
        };
        let valid = if kind == FortKind::Bridge {
            w.game.chars[cell] == b'W'
        } else if kind == FortKind::Fill {
            w.fillable(cell)
        } else if kind == FortKind::Demine {
            w.game.chars[cell] == b'N' && team.is_none_or(|team| mine_known(w, cell, team))
        } else {
            b".+RDM".contains(&w.game.chars[cell])
        };
        if valid && !cells.iter().any(|v: &WorkCell| v.cell == cell) {
            cells.push(WorkCell {
                cell,
                ch: if kind == FortKind::Nest { ch } else { d.ch },
            });
        }
    }
    if kind == FortKind::Bridge {
        let mut accepted = BTreeSet::new();
        let mut pending = cells;
        cells = Vec::new();
        loop {
            let before = pending.len();
            pending.retain(|work| {
                if w.bridge_supported_at(work.cell)
                    || w.neighbors(work.cell)
                        .into_iter()
                        .flatten()
                        .any(|c| accepted.contains(&c))
                {
                    accepted.insert(work.cell);
                    cells.push(*work);
                    false
                } else {
                    true
                }
            });
            if pending.len() == before {
                break;
            }
        }
    }
    if cells.is_empty() {
        Err(Error::Refused(Reason::Blocked))
    } else {
        Ok(cells)
    }
}
impl FieldWorksRuntime {
    pub fn jobs(&self) -> &BTreeMap<u32, Dig> {
        &self.jobs
    }
    pub fn is_active(&self, unit: u32) -> bool {
        self.jobs.contains_key(&unit)
    }
    pub fn pits(&self) -> &BTreeSet<usize> {
        &self.pits
    }
    /// Accepted replacement commands, retreat and retirement call this. Paid work has no refund.
    pub fn cancel(&mut self, unit: u32) -> Option<Dig> {
        self.projects.members.remove(&unit);
        self.drop_paid_job(unit)
    }
    /// A failed Dig route drops its paid segment, retaining the shared project membership.
    /// The source cached active counter changes at the next recount, without a refund.
    pub fn drop_paid_job(&mut self, unit: u32) -> Option<Dig> {
        self.jobs.remove(&unit)
    }
    pub fn prepare_start<'a>(
        &self,
        w: &World,
        wallet: &'a mut PlayerEconomy,
        request: StartRequest,
        sees: impl FnMut(Point) -> bool,
    ) -> Result<PreparedStart<'a>, Error> {
        let defs = definitions()?;
        let d = &defs.forts[&request.kind];
        if !request.at.x.is_finite() || !request.at.z.is_finite() {
            return Err(Error::Refused(Reason::Blocked));
        }
        let unit = w
            .game
            .units
            .get(&request.unit)
            .filter(|u| u.owner == request.owner)
            .ok_or(Error::Refused(Reason::NoBuilders))?;
        let kind = defs.rules.kinds.get(unit.kind).ok_or(Error::InvalidInput)?;
        let builder = if matches!(request.kind, FortKind::Wall | FortKind::Gate)
            && matches!(
                w.game.mode,
                Some(crate::game::ModeKind::Classic | crate::game::ModeKind::World)
            ) {
            kind.name == "engineer"
        } else {
            defs.builders.contains(&kind.name)
        };
        if !request.control_eligible || unit.riding || !builder {
            return Err(Error::Refused(Reason::NoBuilders));
        }
        if unit.retreating {
            return Err(Error::Refused(Reason::Retreating));
        }
        if !position(unit.at()) || !wallet.mp.is_finite() {
            return Err(Error::InvalidInput);
        }
        if wallet.mp < d.cost {
            return Err(Error::Refused(Reason::Mp));
        }
        let at = Point {
            x: math::max(0., math::min(w.game.w as f64 * 2., request.at.x)),
            z: math::max(0., math::min(w.game.h as f64 * 2., request.at.z)),
        };
        let direction = request
            .direction
            .filter(|v| v.is_finite())
            .unwrap_or_else(|| {
                math::atan2(at.z - unit.z, at.x - unit.x)
                    + if request.kind == FortKind::Bridge {
                        0.
                    } else {
                        std::f64::consts::FRAC_PI_2
                    }
            });
        let cells = placement(w, request.kind, at, direction, Some(wallet.team), sees)?;
        if request.kind == FortKind::Aid
            && (w.aid.values().any(|owner| *owner == request.owner)
                || self.jobs.iter().any(|(id, job)| {
                    job.kind == FortKind::Aid
                        && w.game
                            .units
                            .get(id)
                            .is_some_and(|u| u.owner == request.owner)
                }))
        {
            return Err(Error::Refused(Reason::Max));
        }
        Ok(PreparedStart {
            wallet,
            unit: request.unit,
            cost: d.cost,
            job: Dig {
                project: None,
                kind: request.kind,
                at,
                direction,
                cells,
                elapsed: 0.,
                original_cells: None,
                front: None,
                reach: d.reach,
                from: None,
                waits: 0,
                step_back: None,
            },
        })
    }
    pub fn progress(&self, w: &World, unit: u32) -> Result<f64, Error> {
        let Some(job) = self.jobs.get(&unit) else {
            return Ok(0.);
        };
        let count = job.original_cells.unwrap_or(0);
        if count == 0 {
            return Ok(0.);
        }
        let span = self.span(w, unit, job.kind)?;
        Ok(math::min(
            1.,
            (count as f64 - job.cells.len() as f64 + job.elapsed / span) / count as f64,
        ))
    }
    fn span(&self, w: &World, unit: u32, kind: FortKind) -> Result<f64, Error> {
        let defs = definitions()?;
        let u = w.game.units.get(&unit).ok_or(Error::InvalidInput)?;
        let name = &defs
            .rules
            .kinds
            .get(u.kind)
            .ok_or(Error::InvalidInput)?
            .name;
        Ok(defs.forts[&kind].time * if name == "engineer" { 0.5 } else { 1. })
    }
    /// Activity comes from the canonical job record. Accepted cancellation is synchronous.
    pub fn step_unit(&mut self, w: &mut World, unit_id: u32, dt: f64) -> Result<StepResult, Error> {
        if !dt.is_finite() || !(0.0..=7200.0).contains(&dt) {
            return Err(Error::InvalidInput);
        }
        let Some(mut job) = self.jobs.get(&unit_id).cloned() else {
            return Ok(StepResult::default());
        };
        if let Some(id) = job.project {
            let project = self.projects.records.get(&id).ok_or(Error::InvalidInput)?;
            if self.membership(unit_id) != Some(id) || project.active <= -9_007_199_254_740_991 {
                return Err(Error::InvalidInput);
            }
        }
        let unit = w.game.units.get(&unit_id).ok_or(Error::InvalidInput)?;
        if !position(unit.at())
            || !unit.hp.is_finite()
            || !unit.repath.is_finite()
            || !position(job.at)
            || !job.elapsed.is_finite()
            || job.cells.is_empty()
            || job.cells.len() > 6
            || job.cells.iter().any(|c| c.cell >= w.game.chars.len())
        {
            return Err(Error::InvalidInput);
        }
        if unit.hp <= 0. {
            return Ok(StepResult::default());
        }
        if !unit.path.is_empty()
            || job
                .step_back
                .is_some_and(|at| math::hypot(unit.x - at.x, unit.z - at.z) < 1.)
        {
            job.step_back = None;
        }
        let distance = math::hypot(unit.x - job.at.x, unit.z - job.at.z);
        if distance > job.reach.unwrap_or(3.) {
            let route = if unit.repath <= 0. && unit.path.is_empty() {
                if let Some(reach) = job.reach {
                    let from = job.from.unwrap_or_else(|| {
                        let k = (reach - 2.) / if distance == 0. { 1. } else { distance };
                        Point {
                            x: job.at.x + (unit.x - job.at.x) * k,
                            z: job.at.z + (unit.z - job.at.z) * k,
                        }
                    });
                    job.from = Some(from);
                    Some(from)
                } else {
                    Some(job.at)
                }
            } else {
                None
            };
            self.jobs.insert(unit_id, job);
            return Ok(StepResult {
                route,
                ..Default::default()
            });
        }
        let span = self.span(w, unit_id, job.kind)?;
        job.original_cells.get_or_insert(job.cells.len());
        job.elapsed += dt;
        if job.elapsed < span {
            self.jobs.insert(unit_id, job);
            return Ok(StepResult::default());
        }
        w.mutation_capacity(16)?;
        let owner = unit.owner;
        let cell = job.cells[0];
        let defs = definitions()?;
        if w.game
            .units
            .values()
            .any(|v| v.kind >= defs.rules.kinds.len() || !position(v.at()))
        {
            return Err(Error::InvalidInput);
        }
        let under = cell.ch == b'Y'
            && w.game.units.values().any(|v| {
                !defs.rules.kinds[v.kind].infantry && w.game.cell_of(v.x, v.z) == Some(cell.cell)
            });
        let mut result = StepResult::default();
        job.elapsed = 0.;
        job.cells.remove(0);
        if matches!(job.kind, FortKind::Wall | FortKind::Gate) {
            if b".+RDM".contains(&w.game.chars[cell.cell]) {
                let occupied = w.game.flags[cell.cell] & crate::game::MOVE != 0
                    || w.game.units.values().any(|v| {
                        v.hp > 0.
                            && !defs.rules.kinds[v.kind].air
                            && !defs.rules.kinds[v.kind].structure
                            && !v.riding
                            && w.game.cell_of(v.x, v.z) == Some(cell.cell)
                    });
                if !occupied {
                    result.concrete = Some((
                        job.kind,
                        cell.cell,
                        if job.kind == FortKind::Gate {
                            square_direction(job.direction)
                        } else {
                            job.direction
                        },
                    ));
                } else {
                    job.waits += 1;
                    if job.waits < 20 {
                        job.cells.push(cell);
                        let at = center(w, cell.cell);
                        if math::hypot(unit.x - at.x, unit.z - at.z) < 2.5 {
                            let goal = Point {
                                x: at.x - math::sin(job.direction) * 4.,
                                z: at.z + math::cos(job.direction) * 4.,
                            };
                            job.step_back = Some(goal);
                            result.route = Some(goal);
                        }
                    }
                }
            }
        } else if job.kind == FortKind::Demine {
            if w.game.chars[cell.cell] == b'N' {
                w.set_cell(cell.cell, b'.')?;
            }
        } else if job.kind == FortKind::Fill {
            if w.fillable(cell.cell) {
                w.fill_cell(cell.cell)?;
            }
        } else if job.kind == FortKind::Bridge {
            if w.game.chars[cell.cell] == b'W' && w.bridge_supported_at(cell.cell) {
                w.build_bridge_cell(cell.cell)?;
            }
        } else if b".+RDM".contains(&w.game.chars[cell.cell]) && !under {
            w.set_cell(cell.cell, cell.ch)?;
            if cell.ch == b'T' {
                w.game
                    .trench_front
                    .get_or_insert_with(|| vec![f32::NAN; w.game.w * w.game.h])[cell.cell] =
                    job.front.unwrap_or_else(|| {
                        math::atan2(-math::cos(job.direction), math::sin(job.direction))
                    }) as f32;
            }
            if cell.ch == b'T' && job.kind == FortKind::Nest {
                self.pits.insert(cell.cell);
            }
            if cell.ch == b'N' {
                w.mines.insert(cell.cell, owner as i32);
            }
            if cell.ch == b'A' {
                w.aid.insert(cell.cell, owner);
            }
        }
        if job.cells.is_empty() {
            if let Some(project) = job.project {
                let p = self
                    .projects
                    .records
                    .get_mut(&project)
                    .ok_or(Error::InvalidInput)?;
                p.active -= 1;
            }
            self.jobs.remove(&unit_id);
            result.completed = Some(owner);
            Ok(result)
        } else {
            self.jobs.insert(unit_id, job);
            Ok(result)
        }
    }
}
