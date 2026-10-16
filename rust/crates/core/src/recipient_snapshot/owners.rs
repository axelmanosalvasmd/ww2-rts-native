use super::*;
use crate::{game::Point, match_commands::Command, production::JobStatus};

pub(super) fn capture(state: &Match) -> Result<Vec<Map<String, Value>>, SnapshotError> {
    let mut owners = vec![Map::new(); state.world.game.players.len()];
    for owner in &mut owners {
        for name in ["plans", "orders", "queues", "productionJobs"] {
            owner.insert(name.into(), Value::Array(Vec::new()));
        }
    }
    for (slot, owner) in owners.iter_mut().enumerate() {
        owner.insert("movement".into(), super::movement_for(state, slot)?);
        if !state.aviation.planes.is_empty() {
            owner.insert("air".into(), json!([]));
        }
    }
    let catalog = Catalog::compiled().map_err(|_| SnapshotError::Rules)?;
    for u in state.world.game.units.values() {
        let own = &mut owners[u.owner];
        if let Some(air) = u.air {
            push(own, "air", aircraft::row(state, u.id, air)?);
        }
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
            push(own, "queues", json!(row));
            let jobs = building.rows().into_iter().map(|row| {
                finite(&[row.job.charge.mp, row.job.charge.fuel])?;
                Ok(json!({"id":row.job.id,"type":row.job.kind,"mp":row.job.charge.mp,
                    "fuel":row.job.charge.fuel,"status":match row.status { JobStatus::Active=>"active",JobStatus::Waiting=>"waiting"}}))
            }).collect::<Result<Vec<_>, SnapshotError>>()?;
            push(own, "productionJobs", json!([u.id, jobs]));
        }
        let control = state
            .commands
            .units
            .get(&u.id)
            .ok_or(SnapshotError::MissingState {
                unit: u.id,
                owner: "control",
            })?;
        if !state.rules.kinds[u.kind].structure {
            let (kind, point) = plan(state, u)?;
            let mut row = vec![json!(u.id), json!(kind)];
            finite(&[point.x, point.z])?;
            row.extend([json!(r(point.x)), json!(r(point.z))]);
            for p in &u.path {
                finite(&[p.x, p.z])?;
                row.extend([json!(r(p.x)), json!(r(p.z))]);
            }
            push(own, "plans", json!(row));
        }
        if !control.orders.is_empty() {
            let mut row = vec![json!(u.id), json!(control.orders.len())];
            for command in &control.orders {
                let (kind, point) = match command {
                    Command::Move {
                        orders,
                        attack_move,
                        ..
                    } if orders.len() == 1 && orders[0].unit == i64::from(u.id) => {
                        (if *attack_move { 2 } else { 1 }, orders[0].at)
                    }
                    Command::Attack {
                        ids,
                        target,
                        queue: false,
                        at: Some(at),
                    } if ids.as_slice() == [i64::from(u.id)] => {
                        let target = u32::try_from(*target)
                            .ok()
                            .and_then(|id| state.world.game.units.get(&id))
                            .filter(|t| state.world.game.players[u.owner].sees(t.id));
                        (4, target.map_or(*at, Unit::at))
                    }
                    Command::Enter {
                        ids,
                        at,
                        queue: false,
                    } if ids.as_slice() == [i64::from(u.id)] => (9, *at),
                    Command::FireAt {
                        ids,
                        at,
                        queue: false,
                    } if ids.as_slice() == [i64::from(u.id)] => (5, *at),
                    Command::Assist {
                        ids,
                        target,
                        at: Some(at),
                        queue: false,
                    } if ids.as_slice() == [i64::from(u.id)]
                        && (1..=i64::from(u32::MAX)).contains(target) =>
                    {
                        (8, *at)
                    }
                    Command::Cover {
                        ids,
                        at: Some(at),
                        queue: false,
                    } if ids.as_slice() == [i64::from(u.id)] => (1, *at),
                    Command::Dig {
                        ids,
                        at,
                        direction,
                        queue: false,
                        ..
                    } if ids.as_slice() == [i64::from(u.id)]
                        && direction.is_none_or(f64::is_finite) =>
                    {
                        (7, *at)
                    }
                    Command::Entrench {
                        ids,
                        order:
                            crate::field_works::ProjectOrder::Join {
                                project,
                                at: Some(at),
                            },
                        queue: false,
                    } if ids.as_slice() == [i64::from(u.id)]
                        && *project <= 9_007_199_254_740_991 =>
                    {
                        (7, *at)
                    }
                    _ => {
                        return Err(SnapshotError::Unsupported {
                            unit: u.id,
                            capability: SnapshotCapability::Orders,
                        });
                    }
                };
                finite(&[point.x, point.z])?;
                row.extend([json!(kind), json!(r(point.x)), json!(r(point.z))]);
            }
            push(own, "orders", json!(row));
        }
    }
    Ok(owners)
}
pub(super) fn movement_rows(state: &Match, slot: usize) -> Result<Value, SnapshotError> {
    let g = &state.world.game;
    if slot >= g.players.len() {
        return Err(SnapshotError::InvalidRecipient);
    }
    let mut rows = Vec::new();
    for u in g.units.values().filter(|u| u.owner == slot) {
        let control = state
            .commands
            .units
            .get(&u.id)
            .ok_or(SnapshotError::MissingState {
                unit: u.id,
                owner: "control",
            })?;
        if let Some(outcome) = control.outcome {
            let tick = control.outcome_tick.unwrap_or(g.tick);
            if tick < 0 {
                return Err(SnapshotError::InvalidNumber);
            }
            let mut row = vec![json!(u.id), json!(outcome.name()), json!(tick)];
            if let Some(result) = control.move_result {
                let (kind, tick) = result.parts();
                if tick < 0 {
                    return Err(SnapshotError::InvalidNumber);
                }
                row.extend([json!(kind), json!(tick)]);
            }
            rows.push(Value::Array(row));
        }
    }
    Ok(Value::Array(rows))
}
fn push(owner: &mut Map<String, Value>, name: &str, row: Value) {
    owner
        .get_mut(name)
        .unwrap()
        .as_array_mut()
        .unwrap()
        .push(row);
}
fn center(state: &Match, cell: usize) -> Result<Point, SnapshotError> {
    let g = &state.world.game;
    if cell >= g.chars.len() {
        return Err(SnapshotError::Rules);
    }
    Ok(Point {
        x: (cell % g.w) as f64 * crate::game::CELL + crate::game::CELL / 2.,
        z: (cell / g.w) as f64 * crate::game::CELL + crate::game::CELL / 2.,
    })
}
fn plan(state: &Match, u: &Unit) -> Result<(u8, Point), SnapshotError> {
    let g = &state.world.game;
    if u.air.is_some() {
        return Ok(aircraft::plan(state, u));
    }
    let c = &state.commands.units[&u.id];
    if let Some(ride) = g.units.get(&u.board) {
        return Ok((9, ride.at()));
    }
    if u.retreating {
        return Ok((
            3,
            state
                .reaction_home(u.id)
                .map_err(|_| SnapshotError::Rules)?,
        ));
    }
    if let Some(target) = g
        .units
        .get(&c.attack_id)
        .filter(|t| g.players[u.owner].sees(t.id))
    {
        return Ok((4, target.at()));
    }
    if let Some(cell) = c.fire_at {
        return Ok((5, center(state, cell)?));
    }
    if let Some(at) = c.nade {
        return Ok((6, at));
    }
    if let Some(job) = state.field_works.jobs().get(&u.id) {
        return Ok((7, job.at));
    }
    if let Some(at) = state.project_goal(u.id).map_err(|_| SnapshotError::Rules)? {
        return Ok((7, at));
    }
    if let Some(site) = c.build_target.and_then(|id| g.units.get(&id)) {
        return Ok((8, site.at()));
    }
    if let Some(cell) = c.enter {
        return Ok((9, center(state, cell)?));
    }
    if let Some(at) = u.amove {
        return Ok((2, at));
    }
    Ok(u.world_goal
        .or_else(|| u.path.last().copied())
        .map_or((0, Point { x: 0., z: 0. }), |at| (1, at)))
}

#[cfg(test)]
#[path = "owners_tests.rs"]
mod queued_tests;

#[cfg(test)]
mod movement_tests {
    use super::*;
    use crate::match_commands::{MoveOutcome, MoveResult};
    #[test]
    fn owner_rows_preserve_absence_fallback_and_recorded_result_tail() {
        let fixture: Value =
            serde_json::from_str(include_str!("../../tests/fixtures/recipient-snapshot.json"))
                .unwrap();
        let mut state = super::super::tests::initialize(&fixture["cases"][0]);
        assert_eq!(movement_rows(&state, 0).unwrap(), json!([]));
        let id = state
            .world
            .game
            .units
            .values()
            .find(|u| u.owner == 0)
            .unwrap()
            .id;
        let c = state.commands.units.get_mut(&id).unwrap();
        c.outcome = Some(MoveOutcome::Moving);
        c.outcome_tick = None;
        c.move_result = Some(MoveResult::Unreachable { tick: 3 });
        assert_eq!(
            movement_rows(&state, 0).unwrap(),
            json!([[id, "moving", state.world.game.tick, "unreachable", 3]])
        );
        assert_eq!(movement_rows(&state, 1).unwrap(), json!([]));
        state.commands.units.get_mut(&id).unwrap().move_result =
            Some(MoveResult::Completed { tick: -1 });
        assert!(matches!(
            movement_rows(&state, 0),
            Err(SnapshotError::InvalidNumber)
        ));
        assert!(matches!(
            movement_rows(&state, 3),
            Err(SnapshotError::InvalidRecipient)
        ));
    }

    #[test]
    fn queued_attack_uses_visible_pose_then_retains_admission_anchor() {
        let fixture: Value =
            serde_json::from_str(include_str!("../../tests/fixtures/recipient-snapshot.json"))
                .unwrap();
        let mut state = super::super::tests::initialize(&fixture["cases"][0]);
        let anchor = Point { x: 41., z: 43. };
        state
            .commands
            .units
            .get_mut(&1)
            .unwrap()
            .orders
            .push_back(Command::Attack {
                ids: vec![1],
                target: 3,
                queue: false,
                at: Some(anchor),
            });
        state.world.game.players[0].visible = vec![3];
        assert_eq!(
            capture(&state).unwrap()[0]["orders"],
            json!([[1, 1, 4, 35., 35.]])
        );
        state.world.game.units.get_mut(&3).unwrap().x = 37.;
        assert_eq!(
            capture(&state).unwrap()[0]["orders"],
            json!([[1, 1, 4, 37., 35.]])
        );
        state.world.game.players[0].visible.clear();
        assert_eq!(
            capture(&state).unwrap()[0]["orders"],
            json!([[1, 1, 4, 41., 43.]])
        );
        state.world.game.units.remove(&3);
        assert_eq!(
            capture(&state).unwrap()[0]["orders"],
            json!([[1, 1, 4, 41., 43.]])
        );
        if let Command::Attack { at, .. } = state
            .commands
            .units
            .get_mut(&1)
            .unwrap()
            .orders
            .front_mut()
            .unwrap()
        {
            *at = None;
        }
        assert!(matches!(
            capture(&state),
            Err(SnapshotError::Unsupported {
                unit: 1,
                capability: SnapshotCapability::Orders,
            })
        ));
    }
}
