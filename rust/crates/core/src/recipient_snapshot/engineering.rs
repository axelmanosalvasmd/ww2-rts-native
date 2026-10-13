use super::*;

pub(super) fn validate(state: &Match) -> Result<(), SnapshotError> {
    state
        .field_works
        .validate_snapshot(&state.world)
        .map_err(|_| SnapshotError::Unsupported {
            unit: 0,
            capability: SnapshotCapability::Engineering,
        })
}

/// Listed project segments precede live paid jobs, in their canonical order.
/// Coordinates and direction are deliberately unrounded in Source works rows.
pub(super) fn capture(state: &Match) -> Result<Vec<Value>, SnapshotError> {
    validate(state)?;
    let game = &state.world.game;
    let mut rows = Vec::with_capacity(game.players.len());
    for slot in 0..game.players.len() {
        let mut works = Vec::new();
        for project in state
            .field_works
            .projects()
            .filter(|p| game.allied(p.owner, slot))
        {
            for segment in &project.jobs {
                works.push(json!([
                    project.id,
                    u8::from(segment.kind == crate::field_works::FortKind::Wire),
                    segment.at.x,
                    segment.at.z,
                    segment.direction,
                    segment.kind.as_str()
                ]));
            }
        }
        for unit in game
            .units
            .values()
            .filter(|u| u.hp > 0.0 && game.allied(u.owner, slot))
        {
            let Some(job) = state.field_works.jobs().get(&unit.id) else {
                continue;
            };
            let project = match job.project {
                Some(id) => {
                    let linked = state
                        .field_works
                        .member_project(unit.id)
                        .ok_or(SnapshotError::Rules)?;
                    if linked.id != id {
                        return Err(SnapshotError::Rules);
                    }
                    if linked.jobs.is_empty() {
                        -1
                    } else {
                        id as i64
                    }
                }
                None => -1,
            };
            works.push(json!([
                project,
                u8::from(job.kind == crate::field_works::FortKind::Wire),
                job.at.x,
                job.at.z,
                job.direction,
                job.kind.as_str()
            ]));
        }
        rows.push(Value::Array(works));
    }
    Ok(rows)
}
