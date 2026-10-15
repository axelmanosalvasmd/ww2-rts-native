//! Private owner production rows. This is not full World snapshot admission.
use super::*;
use crate::{game::Point, production::JobStatus};

pub(super) fn capture(state: &Match, slot: usize) -> Result<Map<String, Value>, SnapshotError> {
    let g = &state.world.game;
    if slot >= g.players.len() || slot >= state.start.economy.players.len() {
        return Err(SnapshotError::InvalidRecipient);
    }
    let supported = match state.options.mode {
        Mode::Default | Mode::Conquest => g.mode.is_none() && state.world_runtime.is_none(),
        Mode::Classic => g.mode == Some(crate::game::ModeKind::Classic) && state.classic_runtime.is_some(),
        Mode::World => g.mode == Some(crate::game::ModeKind::World) && state.world_runtime.is_some(),
        _ => false,
    };
    if !supported {
        return Err(SnapshotError::UnsupportedMode);
    }
    let mut rows = Map::new();
    for name in ["queues", "productionJobs"] {
        rows.insert(name.into(), Value::Array(Vec::new()));
    }
    let catalog = Catalog::compiled().map_err(|_| SnapshotError::Rules)?;
    for u in g.units.values().filter(|u| u.owner == slot) {
        if let Some(building) = state.production.building(u.id) {
            let mut row = vec![json!(u.id)];
            let progress = if let Some(head) = building.jobs().first() {
                let train = catalog
                    .unit(&head.kind)
                    .map_err(|_| SnapshotError::Rules)?
                    .get("train")
                    .and_then(crate::catalog::Value::number)
                    .filter(|n| n.is_finite() && *n > 0.)
                    .ok_or(SnapshotError::Rules)?;
                building.progress() / train
            } else {
                0.
            };
            finite(&[progress])?;
            row.push(json!(r(progress)));
            let rally = state.building_rallies.get(&u.id).copied().flatten();
            let point = rally.unwrap_or(Point { x: -1., z: -1. });
            finite(&[point.x, point.z])?;
            row.extend([json!(r(point.x)), json!(r(point.z))]);
            row.extend(building.queue().map(|kind| json!(kind)));
            push(&mut rows, "queues", json!(row));
            let jobs = building.rows().into_iter().map(|row| {
                finite(&[row.job.charge.mp, row.job.charge.fuel])?;
                Ok(json!({"id":row.job.id,"type":row.job.kind,"mp":row.job.charge.mp,
                    "fuel":row.job.charge.fuel,"status":match row.status { JobStatus::Active=>"active",JobStatus::Waiting=>"waiting"}}))
            }).collect::<Result<Vec<_>, SnapshotError>>()?;
            push(&mut rows, "productionJobs", json!([u.id, jobs]));
        }
    }
    Ok(rows)
}
fn push(owner: &mut Map<String, Value>, name: &str, row: Value) {
    owner.get_mut(name).unwrap().as_array_mut().unwrap().push(row);
}

#[cfg(test)]
#[path = "production_tests.rs"]
mod tests;
