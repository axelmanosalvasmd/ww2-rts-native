//! Prototype live projectile rows (shared/sim.js:5962-5994 at e4e579a).
use super::*;
use crate::{game::Point, projectiles};

pub(super) fn project(state: &Match, slot: usize) -> Result<Value, SnapshotError> {
    project_perspective(state, slot, false)
}

pub(super) fn project_perspective(
    state: &Match,
    slot: usize,
    full_map: bool,
) -> Result<Value, SnapshotError> {
    operative_flights::validate(state)?;
    let g = &state.world.game;
    let time = g.tick as f64 * state.rules.movement.tick;
    finite(&[time])?;
    state
        .routing_fog
        .validate_shape(g)
        .map_err(|_| SnapshotError::Rules)?;
    let mut fog = state.routing_fog.clone();
    let visible = &fog.team(g, &state.rules, g.players[slot].team).vis;
    let mut rows = Vec::new();
    for (id, p) in &state.firing.flights {
        if *id != p.id
            || *id == 0
            || *id > 9_007_199_254_740_991
            || p.owner >= g.players.len()
            || p.source_kind >= state.rules.kinds.len()
        {
            return Err(SnapshotError::Rules);
        }
        finite(&[p.launched, p.expires, p.solution.stamp, p.solution.gravity])?;
        if p.launched > time {
            continue;
        }
        let at = projectiles::flight_at(p.solution, time).map_err(|_| SnapshotError::Rules)?;
        let velocity =
            projectiles::velocity_at(p.solution, time).map_err(|_| SnapshotError::Rules)?;
        if !full_map && !shots::flight_visible(state, visible, Point { x: at.x, z: at.z }) {
            continue;
        }
        let mut row = json!({
            "flight":p.id, "kind":state.rules.kinds[p.source_kind].name,
            "seq":p.sequence, "x":at.x,"y":at.y,"z":at.z,
            "v":[velocity.x,velocity.y,velocity.z],"gravity":p.solution.gravity,
            "time":time,"launched":p.launched,"expires":p.expires
        });
        if let Some(effect) = p.effect {
            row["effect"] = json!(effect);
        }
        let mut until = if full_map {
            math::min(p.expires, time + 0.15)
        } else {
            shots::trajectory_until(state, visible, p.solution, time, p.expires)?
        };
        if state.rules.kinds[p.source_kind].weapon && until > time {
            let end =
                projectiles::flight_at(p.solution, until).map_err(|_| SnapshotError::Rules)?;
            if let Some(hit) = state
                .visible_flight_contact_perspective(p, at, end, slot, full_map)
                .map_err(|_| SnapshotError::Rules)?
            {
                until = time + (until - time) * hit.hit.fraction;
            }
        }
        row["until"] = json!(until);
        if full_map || seen_by(state, slot, p.shooter) {
            row["f"] = json!(p.shooter);
            row["fo"] = json!(p.owner);
        }
        crate::match_firing::normalize_json_zero(&mut row);
        rows.push(row);
    }
    Ok(Value::Array(rows))
}

#[cfg(test)]
#[path = "flights_tests.rs"]
mod tests;
