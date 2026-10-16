//! Prototype v0 retained ground-flight rows, clipped to each seat's view.
use super::*;
use crate::{game::Point, match_firing::LiveFlight, projectiles};

pub(super) fn validate(state: &Match) -> Result<(), SnapshotError> {
    let g = &state.world.game;
    let identity = |id: u64| (1..=9_007_199_254_740_991).contains(&id);
    for (id, p) in &state.firing.flights {
        let kind = state
            .rules
            .kinds
            .get(p.source_kind)
            .ok_or(SnapshotError::Rules)?;
        if kind.air {
            return Err(SnapshotError::Unsupported {
                unit: p.shooter,
                capability: SnapshotCapability::Air,
            });
        }
        if *id != p.id || !identity(*id) || p.shooter == 0 || p.owner >= g.players.len() {
            return Err(SnapshotError::Rules);
        }
        finite(&[
            p.solution.origin.x,
            p.solution.origin.y,
            p.solution.origin.z,
            p.solution.velocity.x,
            p.solution.velocity.y,
            p.solution.velocity.z,
            p.solution.gravity,
            p.solution.stamp,
            p.launched,
            p.expires,
        ])?;
        if p.launched < 0.0 || p.expires < p.launched {
            return Err(SnapshotError::Rules);
        }
    }
    // These physical schedules produce the same flight owner and events. The
    // prototype sends no separate retained salvo or grenade-ticket collection.
    for salvo in &state.firing.salvos {
        finite(&[
            salvo.at.x,
            salvo.at.z,
            salvo.next,
            salvo.every,
            salvo.spread,
        ])?;
        if salvo.owner >= g.players.len() {
            return Err(SnapshotError::Rules);
        }
    }
    for grenade in &state.firing.grenades {
        finite(&[grenade.at.x, grenade.at.z, grenade.remaining])?;
        if grenade.owner >= g.players.len() || !identity(grenade.flight) {
            return Err(SnapshotError::Rules);
        }
    }
    Ok(())
}

fn row(
    state: &Match,
    slot: usize,
    visible: &[u8],
    p: &LiveFlight,
    time: f64,
) -> Result<Option<Value>, SnapshotError> {
    if p.launched > time {
        return Ok(None);
    }
    let at = projectiles::flight_at(p.solution, time).map_err(|_| SnapshotError::Rules)?;
    if !shots::flight_visible(state, visible, Point { x: at.x, z: at.z }) {
        return Ok(None);
    }
    let v = projectiles::velocity_at(p.solution, time).map_err(|_| SnapshotError::Rules)?;
    finite(&[at.x, at.y, at.z, v.x, v.y, v.z])?;
    let mut row = json!({
        "flight":p.id, "kind":state.rules.kinds[p.source_kind].name,
        "seq":p.sequence, "x":at.x, "y":at.y, "z":at.z,
        "v":[v.x,v.y,v.z], "gravity":p.solution.gravity,
        "time":time, "launched":p.launched, "expires":p.expires
    });
    if let Some(effect) = p.effect {
        row["effect"] = json!(effect);
    }
    let mut until = shots::until(state, visible, &row)?;
    if state.rules.kinds[p.source_kind].weapon && until > time {
        let end = projectiles::flight_at(p.solution, until).map_err(|_| SnapshotError::Rules)?;
        if let Some(contact) = state
            .recipient_flight_contact(p, at, end, &|id| seen_by(state, slot, id))
            .map_err(|_| SnapshotError::Rules)?
        {
            until = time + (until - time) * contact.hit.fraction;
        }
    }
    row["until"] = json!(until);
    if seen_by(state, slot, p.shooter) {
        row["f"] = json!(p.shooter);
        row["fo"] = json!(p.owner);
    }
    crate::match_firing::normalize_json_zero(&mut row);
    Ok(Some(row))
}

pub(super) fn capture(state: &Match) -> Result<Vec<Value>, SnapshotError> {
    validate(state)?;
    if state.firing.flights.is_empty() {
        return Ok(vec![json!([]); state.world.game.players.len()]);
    }
    state
        .routing_fog
        .validate_shape(&state.world.game)
        .map_err(|_| SnapshotError::Rules)?;
    // Preview on a clone so validation cannot publish fog or exploration.
    let mut fog = state.routing_fog.clone();
    let g = &state.world.game;
    let time = g.tick as f64 * 0.05;
    let mut result = Vec::with_capacity(g.players.len());
    for (slot, player) in g.players.iter().enumerate() {
        let visible = &fog.team(g, &state.rules, player.team).vis;
        let rows = state
            .firing
            .flights
            .values()
            .map(|p| row(state, slot, visible, p, time))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        result.push(Value::Array(rows));
    }
    Ok(result)
}

#[cfg(test)]
#[path = "flights_tests.rs"]
mod tests;
