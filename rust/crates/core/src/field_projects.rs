//! Shared field-work patterns. Paid cells remain in the single FieldWorksRuntime job owner.
use super::{Dig, Error, FieldWorksRuntime, FortKind, Reason, definitions, placement, position};
use crate::{
    economy_state::PlayerEconomy,
    game::{Game, Point},
    math,
    world::World,
};
use std::collections::{BTreeMap, BTreeSet};
use std::f64::consts::PI;

pub type ProjectId = u64;
const SAFE_INTEGER: u64 = 9_007_199_254_740_991;

#[derive(Clone, Debug, PartialEq)]
pub struct Project {
    pub id: ProjectId,
    pub owner: usize,
    pub fort: FortKind,
    pub jobs: Vec<Segment>,
    pub crew: usize,
    pub active: i64,
    listed: bool,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct ProjectState {
    pub(super) sequence: ProjectId,
    pub(super) records: BTreeMap<ProjectId, Project>,
    pub(super) members: BTreeMap<u32, ProjectId>,
}

#[derive(Clone, Copy, Debug)]
pub struct ProjectRequest {
    pub owner: usize,
    pub pattern: Pattern,
    pub fort: FortKind,
    pub from: Point,
    pub to: Point,
    pub back: Point,
}

pub struct PreparedProject {
    sequence: ProjectId,
    project: Project,
}
impl PreparedProject {
    pub fn project(&self) -> &Project {
        &self.project
    }
    /// Refuses a stale preparation before publishing any project identity.
    pub fn commit(self, runtime: &mut FieldWorksRuntime) -> Result<ProjectId, Error> {
        if runtime.projects.sequence != self.sequence
            || runtime.projects.records.contains_key(&self.project.id)
        {
            return Err(Error::InvalidInput);
        }
        let id = self.project.id;
        runtime.projects.records.insert(id, self.project);
        runtime.projects.sequence = id;
        Ok(id)
    }
}

/// A paid segment holds the singular wallet until control/story publication is ready.
pub struct PreparedSegment<'a> {
    wallet: &'a mut PlayerEconomy,
    unit: u32,
    project: ProjectId,
    index: usize,
    job: Dig,
    cost: f64,
}
impl PreparedSegment<'_> {
    pub fn cost(&self) -> f64 {
        self.cost
    }
    pub fn job(&self) -> &Dig {
        &self.job
    }
    pub fn commit(self, runtime: &mut FieldWorksRuntime) -> Result<(), Error> {
        let p = runtime
            .projects
            .records
            .get(&self.project)
            .ok_or(Error::InvalidInput)?;
        if runtime.membership(self.unit) != Some(self.project)
            || runtime.is_active(self.unit)
            || p.active >= SAFE_INTEGER as i64
            || p.jobs.get(self.index).is_none_or(|segment| {
                segment.kind != self.job.kind
                    || segment.at != self.job.at
                    || segment.direction != self.job.direction
                    || Some(segment.front) != self.job.front
            })
        {
            return Err(Error::InvalidInput);
        }
        let project = runtime.projects.records.get_mut(&self.project).unwrap();
        project.jobs.remove(self.index);
        project.active += 1;
        self.wallet.mp -= self.cost;
        runtime.jobs.insert(self.unit, self.job);
        Ok(())
    }
}
pub enum SegmentPreparation<'a> {
    Waiting,
    Discard { project: ProjectId, index: usize },
    Paid(PreparedSegment<'a>),
}

impl FieldWorksRuntime {
    pub fn project_sequence(&self) -> ProjectId {
        self.projects.sequence
    }
    /// Listed records match the source Map. Archived identity remains only while referenced.
    pub fn projects(&self) -> impl Iterator<Item = &Project> {
        self.projects.records.values().filter(|p| p.listed)
    }
    pub fn retained_projects(&self) -> impl Iterator<Item = &Project> {
        self.projects.records.values()
    }
    pub fn project(&self, id: ProjectId) -> Option<&Project> {
        self.projects.records.get(&id).filter(|p| p.listed)
    }
    pub fn membership(&self, unit: u32) -> Option<ProjectId> {
        self.projects.members.get(&unit).copied()
    }
    pub fn member_project(&self, unit: u32) -> Option<&Project> {
        self.membership(unit)
            .and_then(|id| self.projects.records.get(&id))
    }

    /// Rechecks every segment using actual owner sight without allocating or charging yet.
    pub fn prepare_project(
        &self,
        w: &World,
        wallet: &PlayerEconomy,
        request: ProjectRequest,
        mut sees: impl FnMut(Point) -> bool,
    ) -> Result<PreparedProject, Error> {
        if self.projects.sequence >= SAFE_INTEGER
            || !wallet.mp.is_finite()
            || request.owner >= w.game.players.len()
        {
            return Err(Error::InvalidInput);
        }
        let mut reason = Reason::Blocked;
        let mut cheapest = f64::INFINITY;
        let mut jobs = Vec::new();
        for segment in entrench_plan(
            request.pattern,
            request.from,
            request.to,
            Some(request.back),
            request.fort,
        )? {
            match placement(
                w,
                segment.kind,
                segment.at,
                segment.direction,
                Some(wallet.team),
                &mut sees,
            ) {
                Ok(cells) => {
                    cheapest = math::min(cheapest, segment_cost(segment.kind, cells.len())?);
                    jobs.push(segment);
                }
                Err(Error::Refused(Reason::NotVisible)) => reason = Reason::NotVisible,
                Err(Error::Refused(_)) => {}
                Err(error) => return Err(error),
            }
        }
        if jobs.is_empty() {
            return Err(Error::Refused(reason));
        }
        if wallet.mp < cheapest {
            return Err(Error::Refused(Reason::Mp));
        }
        Ok(PreparedProject {
            sequence: self.projects.sequence,
            project: Project {
                id: self.projects.sequence + 1,
                owner: request.owner,
                fort: request.fort,
                jobs,
                crew: 0,
                active: 0,
                listed: true,
            },
        })
    }

    /// Caller preflights control/garrison changes first. Rejoining the same object preserves paid work.
    pub fn join_project(&mut self, project: ProjectId, units: &[u32]) -> Result<(), Error> {
        let p = self
            .project(project)
            .filter(|p| !p.jobs.is_empty())
            .ok_or(Error::Refused(Reason::Blocked))?;
        let mut changes = BTreeSet::new();
        for &id in units {
            if id == 0 {
                return Err(Error::InvalidInput);
            }
            if self.membership(id) != Some(project) {
                changes.insert(id);
            }
        }
        if p.crew
            .checked_add(changes.len())
            .is_none_or(|n| n as u64 > SAFE_INTEGER)
        {
            return Err(Error::InvalidInput);
        }
        for &id in units {
            if self.membership(id) == Some(project) {
                continue;
            }
            self.cancel(id);
            self.projects.members.insert(id, project);
            self.projects.records.get_mut(&project).unwrap().crew += 1;
        }
        Ok(())
    }

    pub fn discard_segment(&mut self, project: ProjectId, index: usize) -> Result<(), Error> {
        let p = self
            .projects
            .records
            .get_mut(&project)
            .ok_or(Error::InvalidInput)?;
        if index >= p.jobs.len() {
            return Err(Error::InvalidInput);
        }
        p.jobs.remove(index);
        Ok(())
    }

    pub fn prepare_segment<'a>(
        &self,
        w: &World,
        wallet: &'a mut PlayerEconomy,
        unit: u32,
        mut sees: impl FnMut(Point) -> bool,
    ) -> Result<SegmentPreparation<'a>, Error> {
        let id = self.membership(unit).ok_or(Error::InvalidInput)?;
        let p = self.projects.records.get(&id).ok_or(Error::InvalidInput)?;
        let u = w.game.units.get(&unit).ok_or(Error::InvalidInput)?;
        if self.is_active(unit)
            || p.jobs.is_empty()
            || !position(u.at())
            || !wallet.mp.is_finite()
            || p.active >= SAFE_INTEGER as i64
        {
            return Err(Error::InvalidInput);
        }
        let mut index = 0;
        for i in 1..p.jobs.len().min(p.crew) {
            if math::hypot(u.x - p.jobs[i].at.x, u.z - p.jobs[i].at.z)
                < math::hypot(u.x - p.jobs[index].at.x, u.z - p.jobs[index].at.z)
            {
                index = i;
            }
        }
        let segment = p.jobs[index];
        let cells = match placement(
            w,
            segment.kind,
            segment.at,
            segment.direction,
            Some(wallet.team),
            &mut sees,
        ) {
            Ok(cells) => cells,
            Err(Error::Refused(_)) => {
                return Ok(SegmentPreparation::Discard { project: id, index });
            }
            Err(error) => return Err(error),
        };
        let cost = segment_cost(segment.kind, cells.len())?;
        if wallet.mp < cost {
            return Ok(SegmentPreparation::Waiting);
        }
        let reach = definitions()?.forts[&segment.kind].reach;
        Ok(SegmentPreparation::Paid(PreparedSegment {
            wallet,
            unit,
            project: id,
            index,
            cost,
            job: Dig {
                project: Some(id),
                kind: segment.kind,
                at: segment.at,
                direction: segment.direction,
                cells,
                elapsed: 0.,
                original_cells: None,
                front: Some(segment.front),
                reach,
                from: None,
            },
        }))
    }

    /// Source early beat: active recount every tick, listed-project sweep/crew recount each twentieth tick.
    /// Waiting rows are supplied in source unit/order order by the singular command owner.
    pub fn recount_projects(
        &mut self,
        game: &Game,
        waiting: &[(u32, ProjectId)],
    ) -> Result<(), Error> {
        if game.tick < 0 {
            return Err(Error::InvalidInput);
        }
        for (&id, &project) in &self.projects.members {
            let u = game.units.get(&id).ok_or(Error::InvalidInput)?;
            if !u.entrench || !self.projects.records.contains_key(&project) {
                return Err(Error::InvalidInput);
            }
        }
        for (&id, u) in &game.units {
            if u.entrench != self.projects.members.contains_key(&id) || !u.hp.is_finite() {
                return Err(Error::InvalidInput);
            }
        }
        if self.jobs.values().any(|j| {
            j.project
                .is_some_and(|id| !self.projects.records.contains_key(&id))
        }) {
            return Err(Error::InvalidInput);
        }
        for p in self.projects.records.values_mut().filter(|p| p.listed) {
            p.active = 0;
        }
        for (&id, job) in &self.jobs {
            if game.units.get(&id).is_some_and(|u| u.hp > 0.)
                && let Some(project) = job.project
            {
                self.projects.records.get_mut(&project).unwrap().active += 1;
            }
        }
        if game.tick % 20 == 0 {
            let mut crews = BTreeMap::<ProjectId, usize>::new();
            for &project in self.projects.members.values() {
                *crews.entry(project).or_default() += 1;
            }
            for &(unit, project) in waiting {
                if game.units.contains_key(&unit) && self.project(project).is_some() {
                    crews.entry(project).or_default();
                }
            }
            for p in self.projects.records.values_mut().filter(|p| p.listed) {
                if (p.jobs.is_empty() || !crews.contains_key(&p.id)) && p.active == 0 {
                    p.listed = false;
                } else {
                    p.crew = crews.get(&p.id).copied().unwrap_or(0).max(1);
                }
            }
        }
        let references: BTreeSet<_> = self
            .projects
            .members
            .values()
            .copied()
            .chain(self.jobs.values().filter_map(|j| j.project))
            .collect();
        self.projects
            .records
            .retain(|id, p| p.listed || references.contains(id));
        Ok(())
    }
    pub fn finish_empty_project(&mut self, unit: u32) -> Result<bool, Error> {
        let Some(p) = self.member_project(unit) else {
            return Ok(false);
        };
        if p.jobs.is_empty() && p.active == 0 {
            self.projects.members.remove(&unit);
            return Ok(true);
        }
        Ok(false)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pattern {
    Line,
    Zigzag,
    Double,
    Arc,
    Ring,
    Strongpoint,
}
impl Pattern {
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "line" => Some(Self::Line),
            "zigzag" => Some(Self::Zigzag),
            "double" => Some(Self::Double),
            "arc" => Some(Self::Arc),
            "ring" => Some(Self::Ring),
            "strongpoint" => Some(Self::Strongpoint),
            _ => None,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Line => "line",
            Self::Zigzag => "zigzag",
            Self::Double => "double",
            Self::Arc => "arc",
            Self::Ring => "ring",
            Self::Strongpoint => "strongpoint",
        }
    }
    fn linear(self) -> bool {
        matches!(self, Self::Line | Self::Zigzag | Self::Double)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Segment {
    pub kind: FortKind,
    pub at: Point,
    pub direction: f64,
    pub front: f64,
}

/// Exact source price for the buildable part of a segment, without paying yet.
pub fn segment_cost(kind: FortKind, cells: usize) -> Result<f64, Error> {
    if !line_fort(kind) || cells > 6 {
        return Err(Error::InvalidInput);
    }
    let d = &definitions()?.forts[&kind];
    Ok(math::max(
        1.,
        math::round(d.cost * cells as f64 / d.count as f64),
    ))
}

pub fn line_fort(kind: FortKind) -> bool {
    matches!(
        kind,
        FortKind::Trench
            | FortKind::Sandbags
            | FortKind::Wire
            | FortKind::Traps
            | FortKind::Mines
            | FortKind::Demine
    )
}

/// Source preview order is also the order of unpaid segments in a shared project.
pub fn entrench_plan(
    pattern: Pattern,
    a: Point,
    b: Point,
    back: Option<Point>,
    fort: FortKind,
) -> Result<Vec<Segment>, Error> {
    if !position(a)
        || !position(b)
        || back.is_some_and(|p| !position(p))
        || !line_fort(fort)
        || (fort != FortKind::Trench && !pattern.linear())
    {
        return Err(Error::InvalidInput);
    }
    let defs = definitions()?;
    let seg = defs.forts[&fort].count as f64 * 2.;
    let len = math::hypot(b.x - a.x, b.z - a.z);
    let drawn = len >= 2.;
    let away = back
        .filter(|p| math::hypot(a.x - p.x, a.z - p.z) > 0.5)
        .map_or(0., |p| math::atan2(a.z - p.z, a.x - p.x));
    let dir = if drawn {
        math::atan2(b.z - a.z, b.x - a.x)
    } else if pattern.linear() {
        away + PI / 2.
    } else {
        away
    };
    let dx = math::cos(dir);
    let dz = math::sin(dir);
    let clamp = |v, lo, hi| math::max(lo, math::min(hi, v));
    let mut out = Vec::new();
    let mut push = |kind, x, z, direction, front: Option<f64>| {
        out.push(Segment {
            kind,
            at: Point { x, z },
            direction,
            front: front.unwrap_or_else(|| math::atan2(z - a.z, x - a.x)),
        })
    };
    if pattern.linear() {
        let mx = (a.x + b.x) / 2.;
        let mz = (a.z + b.z) / 2.;
        let length = if drawn { len } else { seg };
        let side = if back.is_some_and(|p| (mx - p.x) * dz + (mz - p.z) * -dx < 0.) {
            -1.
        } else {
            1.
        };
        let nx = dz * side;
        let nz = -dx * side;
        let front = math::atan2(nz, nx);
        if pattern == Pattern::Zigzag {
            let w = seg * math::cos(0.6);
            let h = seg * math::sin(0.6);
            let n = clamp(math::round(length / w), 1., 16.) as usize;
            let vertex = |i: usize| Point {
                x: mx
                    + dx * (i as f64 - n as f64 / 2.) * w
                    + nx * (if i % 2 == 1 { h / 2. } else { -h / 2. }),
                z: mz
                    + dz * (i as f64 - n as f64 / 2.) * w
                    + nz * (if i % 2 == 1 { h / 2. } else { -h / 2. }),
            };
            for i in 0..n {
                let p = vertex(i);
                let q = vertex(i + 1);
                push(
                    fort,
                    (p.x + q.x) / 2.,
                    (p.z + q.z) / 2.,
                    math::atan2(q.z - p.z, q.x - p.x),
                    Some(front),
                );
            }
        } else {
            let n = clamp(math::round(length / seg), 1., 12.) as usize;
            for off in if pattern == Pattern::Double {
                vec![0., -3. * 2.]
            } else {
                vec![0.]
            } {
                for i in 0..n {
                    let t = (i as f64 - (n as f64 - 1.) / 2.) * seg;
                    push(
                        fort,
                        mx + dx * t + nx * off,
                        mz + dz * t + nz * off,
                        dir,
                        Some(front),
                    );
                }
            }
        }
    } else if matches!(pattern, Pattern::Arc | Pattern::Ring) {
        let ring = pattern == Pattern::Ring;
        let radius = if drawn {
            clamp(
                len,
                if ring { 6. } else { 8. },
                if ring { 24. } else { 30. },
            )
        } else if ring {
            10.
        } else {
            12.
        };
        let span = if ring { PI * 2. } else { PI * 2. / 3. };
        let n = clamp(
            math::round(radius * span / seg),
            if ring { 4. } else { 2. },
            20.,
        ) as usize;
        let step = if ring { span / n as f64 } else { seg / radius };
        for i in 0..n {
            let t = dir + (i as f64 - (n as f64 - 1.) / 2.) * step;
            push(
                FortKind::Trench,
                a.x + math::cos(t) * radius,
                a.z + math::sin(t) * radius,
                t + PI / 2.,
                None,
            );
        }
    } else {
        let h = seg / 2. - 2. / 2.;
        let px = -dz;
        let pz = dx;
        push(
            FortKind::Trench,
            a.x + dx * h,
            a.z + dz * h,
            dir + PI / 2.,
            None,
        );
        push(
            FortKind::Trench,
            a.x - dx * h,
            a.z - dz * h,
            dir + PI / 2.,
            None,
        );
        push(FortKind::Trench, a.x + px * h, a.z + pz * h, dir, None);
        push(FortKind::Trench, a.x - px * h, a.z - pz * h, dir, None);
        let f = h + 3. * 2.;
        let wl = defs.forts[&FortKind::Wire].count as f64 * 2. / 2.;
        push(
            FortKind::Wire,
            a.x + dx * f + px * wl,
            a.z + dz * f + pz * wl,
            dir + PI / 2.,
            None,
        );
        push(
            FortKind::Wire,
            a.x + dx * f - px * wl,
            a.z + dz * f - pz * wl,
            dir + PI / 2.,
            None,
        );
        return Ok(out);
    }
    let cx = out.iter().fold(0., |s, j| s + j.at.x) / out.len() as f64;
    let cz = out.iter().fold(0., |s, j| s + j.at.z) / out.len() as f64;
    let mut keyed: Vec<_> = out
        .into_iter()
        .enumerate()
        .map(|(i, j)| (math::hypot(j.at.x - cx, j.at.z - cz), i, j))
        .collect();
    keyed.sort_by(|a, b| {
        a.0.partial_cmp(&b.0)
            .expect("finite pattern distance")
            .then(a.1.cmp(&b.1))
    });
    Ok(keyed.into_iter().map(|(_, _, j)| j).collect())
}
