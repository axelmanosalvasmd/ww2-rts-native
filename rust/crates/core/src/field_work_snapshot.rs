//! Read-only canonical field-work admission for recipient projections.
use super::{FieldWorksRuntime, definitions, line_fort, position};
use crate::world::World;
use std::collections::BTreeSet;

const SAFE_INTEGER: u64 = 9_007_199_254_740_991;

impl FieldWorksRuntime {
    /// Validate every identity and referenced descriptor without ticking,
    /// recounting cached project counters or reserving future queue capacity.
    pub(crate) fn validate_snapshot(&self, world: &World) -> Result<(), String> {
        let game = &world.game;
        let cells = game
            .w
            .checked_mul(game.h)
            .ok_or("invalid field-work dimensions")?;
        if self.projects.sequence > SAFE_INTEGER {
            return Err("invalid field-work project sequence".into());
        }
        let defs = definitions().map_err(|error| error.to_string())?;
        let references = self
            .projects
            .members
            .values()
            .copied()
            .chain(self.jobs.values().filter_map(|job| job.project))
            .collect::<BTreeSet<_>>();
        for (&id, &project) in &self.projects.members {
            let unit = game.units.get(&id).ok_or("orphan field-work membership")?;
            if id == 0
                || unit.id != id
                || !unit.entrench
                || !self.projects.records.contains_key(&project)
            {
                return Err("invalid field-work membership identity".into());
            }
        }
        for (&id, project) in &self.projects.records {
            if id != project.id {
                return Err("field-work project identity mismatch".into());
            }
            if self.project(id).is_none() && !references.contains(&id) {
                continue;
            }
            if id == 0
                || id > self.projects.sequence
                || project.owner >= game.players.len()
                || !line_fort(project.fort)
                || project.crew as u64 > SAFE_INTEGER
                || !(-(SAFE_INTEGER as i64)..=SAFE_INTEGER as i64).contains(&project.active)
                || project.jobs.iter().any(|segment| {
                    !line_fort(segment.kind)
                        || !position(segment.at)
                        || !segment.direction.is_finite()
                        || !segment.front.is_finite()
                })
            {
                return Err("invalid field-work project projection".into());
            }
        }
        for (&id, job) in &self.jobs {
            let unit = game.units.get(&id).ok_or("orphan paid field-work job")?;
            if id == 0
                || unit.id != id
                || !unit.dig
                || !position(job.at)
                || !job.direction.is_finite()
                || !job.elapsed.is_finite()
                || job.elapsed < 0.0
                || job.waits > 23
                || !matches!(job.kind, super::FortKind::Wall | super::FortKind::Gate)
                    && job.waits != 0
                || job.step_back.is_some_and(|p| {
                    !position(p)
                        || !matches!(job.kind, super::FortKind::Wall | super::FortKind::Gate)
                })
                || job.front.is_some_and(|value| !value.is_finite())
                || job
                    .reach
                    .is_some_and(|value| !value.is_finite() || value < 0.0)
                || job.from.is_some_and(|at| !position(at))
                || job.cells.is_empty()
                || job.cells.len() > 6
                || job
                    .original_cells
                    .is_some_and(|count| count < job.cells.len())
                || job.cells.iter().any(|cell| {
                    cell.cell >= cells
                        || if job.kind == super::FortKind::Nest {
                            !b"T#".contains(&cell.ch)
                        } else {
                            cell.ch != defs.forts[&job.kind].ch
                        }
                })
            {
                return Err("invalid paid field-work projection".into());
            }
            if let Some(project) = job.project
                && (self.membership(id) != Some(project)
                    || !self.projects.records.contains_key(&project)
                    || !line_fort(job.kind))
            {
                return Err("paid field-work project binding mismatch".into());
            }
        }
        for (&id, unit) in &game.units {
            if unit.dig != self.is_active(id) || unit.entrench != self.membership(id).is_some() {
                return Err("field-work activity binding mismatch".into());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::recipient_snapshot::{DeliveryState, SnapshotCapability, SnapshotError};
    use serde_json::Value;

    #[test]
    fn orphan_membership_refuses_before_any_delivery_owner_changes() {
        let fixture: Value =
            serde_json::from_str(include_str!("../tests/fixtures/recipient-delivery.json"))
                .unwrap();
        let mut state = crate::recipient_snapshot::tests::initialize(&fixture["initial"]);
        let mut delivery = DeliveryState::new(&state).unwrap();
        let cell = 6 * state.world.game.w + 6;
        state.world.set_cell(cell, b'T').unwrap();
        // No live unit query can discover this orphan. Provider admission must
        // inspect the complete canonical membership map before publishing.
        state.field_works.projects.members.insert(u32::MAX, 1);
        let world = format!("{:?}", state.world);
        let memories = format!("{:?}", state.recipients);
        let fog = format!("{:?}", state.routing_fog);
        let works = state.field_works.clone();
        let delivery_before = delivery.clone();
        assert!(matches!(
            delivery.capture_with_lists(&mut state, &[false; 3], &[false; 3], &[]),
            Err(SnapshotError::Unsupported {
                capability: SnapshotCapability::Engineering,
                ..
            })
        ));
        assert_eq!(format!("{:?}", state.world), world);
        assert_eq!(format!("{:?}", state.recipients), memories);
        assert_eq!(format!("{:?}", state.routing_fog), fog);
        assert_eq!(state.field_works, works);
        assert_eq!(delivery, delivery_before);
        state.field_works.projects.members.remove(&u32::MAX);
        let frame = delivery
            .capture_with_lists(&mut state, &[false; 3], &[false; 3], &[])
            .unwrap();
        assert!(
            frame.recipient(0).unwrap().into_json()["cells"]
                .as_array()
                .unwrap()
                .iter()
                .any(|row| row[0] == cell)
        );
    }
}
