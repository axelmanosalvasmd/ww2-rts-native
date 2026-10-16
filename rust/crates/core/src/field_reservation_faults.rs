//! Internal malformed-reservation controls over genuinely acquired paid work.
//! No actor, pose, HP, wallet or clock mutation is available through this module.
use super::{FieldWorksRuntime, FortKind, Segment, definitions};

#[derive(Clone, Copy, Debug)]
pub(crate) enum Fault {
    MissingMembership,
    PaidProjectMismatch,
    MissingProjectRecord,
    WrongProjectIdentity,
    CachedActiveOutOfRange,
    CachedActiveBelowRange,
    ProjectOwner,
    ZeroProjectIdentity,
    ProjectBeyondSequence,
    ProjectSequenceOutOfRange,
    ProjectFort,
    CachedCrewOutOfRange,
    SegmentKind,
    SegmentPosition,
    SegmentDirection,
    SegmentFront,
    MissingPaidJob,
    JobPosition,
    JobPositionRange,
    JobDirection,
    JobElapsed,
    JobElapsedNegative,
    JobFront,
    JobReach,
    JobReachNegative,
    JobOrigin,
    JobCellsEmpty,
    JobCellsTooMany,
    JobOriginalCells,
    JobCellIndex,
    JobCellCharacter,
    JobNestCellCharacter,
    JobProjectKind,
}

pub(crate) const ALL: &[Fault] = &[
    Fault::MissingMembership,
    Fault::PaidProjectMismatch,
    Fault::MissingProjectRecord,
    Fault::WrongProjectIdentity,
    Fault::CachedActiveOutOfRange,
    Fault::CachedActiveBelowRange,
    Fault::ProjectOwner,
    Fault::ZeroProjectIdentity,
    Fault::ProjectBeyondSequence,
    Fault::ProjectSequenceOutOfRange,
    Fault::ProjectFort,
    Fault::CachedCrewOutOfRange,
    Fault::SegmentKind,
    Fault::SegmentPosition,
    Fault::SegmentDirection,
    Fault::SegmentFront,
    Fault::MissingPaidJob,
    Fault::JobPosition,
    Fault::JobPositionRange,
    Fault::JobDirection,
    Fault::JobElapsed,
    Fault::JobElapsedNegative,
    Fault::JobFront,
    Fault::JobReach,
    Fault::JobReachNegative,
    Fault::JobOrigin,
    Fault::JobCellsEmpty,
    Fault::JobCellsTooMany,
    Fault::JobOriginalCells,
    Fault::JobCellIndex,
    Fault::JobCellCharacter,
    Fault::JobNestCellCharacter,
    Fault::JobProjectKind,
];

impl Fault {
    pub(crate) fn validator_error(self) -> &'static str {
        match self {
            Self::MissingMembership | Self::PaidProjectMismatch | Self::JobProjectKind => {
                "paid field-work project binding mismatch"
            }
            Self::MissingPaidJob => "field-work activity binding mismatch",
            Self::MissingProjectRecord => "invalid field-work membership identity",
            Self::WrongProjectIdentity => "field-work project identity mismatch",
            Self::ProjectSequenceOutOfRange => "invalid field-work project sequence",
            Self::CachedActiveOutOfRange
            | Self::CachedActiveBelowRange
            | Self::ProjectOwner
            | Self::ProjectFort
            | Self::ZeroProjectIdentity
            | Self::ProjectBeyondSequence
            | Self::CachedCrewOutOfRange
            | Self::SegmentKind
            | Self::SegmentPosition
            | Self::SegmentDirection
            | Self::SegmentFront => "invalid field-work project projection",
            _ => "invalid paid field-work projection",
        }
    }
    pub(crate) fn dispatch_error(self) -> String {
        match self {
            Self::MissingMembership | Self::MissingPaidJob => {
                "invalid canonical ground order actor".into()
            }
            _ => format!("ground retreat reservation: {}", self.validator_error()),
        }
    }
}

pub(crate) fn corrupt(runtime: &mut FieldWorksRuntime, actor: u32, fault: Fault) {
    let project = runtime
        .membership(actor)
        .expect("genuine project membership required");
    assert_eq!(
        runtime.jobs()[&actor].project,
        Some(project),
        "genuine paid segment required"
    );
    let other = runtime.project_sequence().checked_add(1).unwrap();
    // The remaining queue may be empty once both real actors have paid. A
    // malformed queued descriptor may then be based on their actual paid job.
    if matches!(
        fault,
        Fault::SegmentKind | Fault::SegmentPosition | Fault::SegmentDirection | Fault::SegmentFront
    ) && runtime.projects.records[&project].jobs.is_empty()
    {
        let job = &runtime.jobs[&actor];
        let segment = Segment {
            kind: job.kind,
            at: job.at,
            direction: job.direction,
            front: job.front.expect("actual paid project front"),
        };
        runtime
            .projects
            .records
            .get_mut(&project)
            .unwrap()
            .jobs
            .push(segment);
    }
    match fault {
        Fault::MissingMembership => {
            runtime.projects.members.remove(&actor);
        }
        Fault::PaidProjectMismatch => {
            runtime.jobs.get_mut(&actor).unwrap().project = Some(other);
        }
        Fault::MissingProjectRecord => {
            runtime.projects.records.remove(&project);
        }
        Fault::WrongProjectIdentity => {
            runtime.projects.records.get_mut(&project).unwrap().id = other;
        }
        Fault::CachedActiveOutOfRange => {
            runtime.projects.records.get_mut(&project).unwrap().active = i64::MAX;
        }
        Fault::CachedActiveBelowRange => {
            runtime.projects.records.get_mut(&project).unwrap().active = i64::MIN;
        }
        Fault::ProjectOwner => {
            runtime.projects.records.get_mut(&project).unwrap().owner = usize::MAX;
        }
        Fault::ZeroProjectIdentity => {
            let mut record = runtime.projects.records.remove(&project).unwrap();
            record.id = 0;
            runtime.projects.records.insert(0, record);
            runtime.projects.members.insert(actor, 0);
            runtime.jobs.get_mut(&actor).unwrap().project = Some(0);
        }
        Fault::ProjectBeyondSequence => {
            runtime.projects.sequence = 0;
        }
        Fault::ProjectSequenceOutOfRange => {
            runtime.projects.sequence = u64::MAX;
        }
        Fault::ProjectFort => {
            runtime.projects.records.get_mut(&project).unwrap().fort = FortKind::Aid;
        }
        Fault::CachedCrewOutOfRange => {
            runtime.projects.records.get_mut(&project).unwrap().crew = usize::MAX;
        }
        Fault::SegmentKind => {
            runtime.projects.records.get_mut(&project).unwrap().jobs[0].kind = FortKind::Aid;
        }
        Fault::SegmentPosition => {
            runtime.projects.records.get_mut(&project).unwrap().jobs[0]
                .at
                .x = f64::NAN;
        }
        Fault::SegmentDirection => {
            runtime.projects.records.get_mut(&project).unwrap().jobs[0].direction = f64::INFINITY;
        }
        Fault::SegmentFront => {
            runtime.projects.records.get_mut(&project).unwrap().jobs[0].front = f64::NAN;
        }
        Fault::MissingPaidJob => {
            runtime.jobs.remove(&actor);
        }
        Fault::JobPosition => {
            runtime.jobs.get_mut(&actor).unwrap().at.z = f64::NAN;
        }
        Fault::JobPositionRange => {
            runtime.jobs.get_mut(&actor).unwrap().at.x = 1_073_741_825.0;
        }
        Fault::JobDirection => {
            runtime.jobs.get_mut(&actor).unwrap().direction = f64::INFINITY;
        }
        Fault::JobElapsed => {
            runtime.jobs.get_mut(&actor).unwrap().elapsed = f64::NAN;
        }
        Fault::JobElapsedNegative => {
            runtime.jobs.get_mut(&actor).unwrap().elapsed = -1.0;
        }
        Fault::JobFront => {
            runtime.jobs.get_mut(&actor).unwrap().front = Some(f64::NAN);
        }
        Fault::JobReach => {
            runtime.jobs.get_mut(&actor).unwrap().reach = Some(f64::INFINITY);
        }
        Fault::JobReachNegative => {
            runtime.jobs.get_mut(&actor).unwrap().reach = Some(-1.0);
        }
        Fault::JobOrigin => {
            runtime.jobs.get_mut(&actor).unwrap().from = Some(crate::game::Point {
                x: f64::NAN,
                z: 0.0,
            });
        }
        Fault::JobCellsEmpty => {
            runtime.jobs.get_mut(&actor).unwrap().cells.clear();
        }
        Fault::JobCellsTooMany => {
            let job = runtime.jobs.get_mut(&actor).unwrap();
            job.cells.resize(7, job.cells[0]);
        }
        Fault::JobOriginalCells => {
            runtime.jobs.get_mut(&actor).unwrap().original_cells = Some(0);
        }
        Fault::JobCellIndex => {
            runtime.jobs.get_mut(&actor).unwrap().cells[0].cell = usize::MAX;
        }
        Fault::JobCellCharacter => {
            runtime.jobs.get_mut(&actor).unwrap().cells[0].ch = b'?';
        }
        Fault::JobNestCellCharacter => {
            let job = runtime.jobs.get_mut(&actor).unwrap();
            job.kind = FortKind::Nest;
            job.cells[0].ch = b'?';
        }
        Fault::JobProjectKind => {
            let ch = definitions().unwrap().forts[&FortKind::Aid].ch;
            let job = runtime.jobs.get_mut(&actor).unwrap();
            job.kind = FortKind::Aid;
            for cell in &mut job.cells {
                cell.ch = ch;
            }
        }
    }
}
