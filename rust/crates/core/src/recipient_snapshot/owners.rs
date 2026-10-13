use super::*;
use crate::{game::Point, match_commands::Command, production::JobStatus};

pub(super) fn capture(state: &Match) -> Result<Vec<Map<String, Value>>, SnapshotError> {
    let mut owners = vec![Map::new(); state.world.game.players.len()];
    for owner in &mut owners {
        for name in ["plans", "orders", "queues", "productionJobs"] {
            owner.insert(name.into(), Value::Array(Vec::new()));
        }
    }
    let catalog = Catalog::compiled().map_err(|_| SnapshotError::Rules)?;
    for u in state.world.game.units.values() {
        let own = &mut owners[u.owner];
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
    // unit_row rejects active work before this cache is built.
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
