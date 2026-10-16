use super::*;
use crate::{game::Point, projectiles};

fn refused() -> SnapshotError {
    SnapshotError::Unsupported {
        unit: 0,
        capability: SnapshotCapability::ShotsAndFlights,
    }
}

pub(super) fn shooter(state: &Match, id: u32, owner: usize) -> Result<(), SnapshotError> {
    if id == 0
        || owner >= state.world.game.players.len()
        || state.commands.units.get(&id).is_none_or(|c| !c.operative)
        || state.combat.get(id).is_none()
        || state
            .world
            .game
            .units
            .get(&id)
            .is_some_and(|u| u.owner != owner)
    {
        return Err(refused());
    }
    Ok(())
}

pub(super) fn validate(state: &Match) -> Result<(), SnapshotError> {
    for (id, flight) in &state.firing.flights {
        shooter(state, flight.shooter, flight.owner)?;
        if *id == 0
            || *id != flight.id
            || *id > 9_007_199_254_740_991
            || state
                .rules
                .kinds
                .get(flight.source_kind)
                .is_none_or(|k| k.name != "rifle")
            || flight.blast_source.is_none()
            || !flight.explosive
            || flight.effect != Some("boom")
            || ![
                flight.launched,
                flight.expires,
                flight.solution.stamp,
                flight.solution.origin.x,
                flight.solution.origin.y,
                flight.solution.origin.z,
                flight.solution.velocity.x,
                flight.solution.velocity.y,
                flight.solution.velocity.z,
                flight.solution.gravity,
            ]
            .iter()
            .all(|n| n.is_finite())
            || flight.expires < flight.launched
        {
            return Err(refused());
        }
    }
    for ticket in &state.firing.grenades {
        if ![ticket.at.x, ticket.at.z, ticket.remaining]
            .iter()
            .all(|n| n.is_finite())
            || ticket.remaining < 0.0
            || state
                .firing
                .flights
                .get(&ticket.flight)
                .is_none_or(|f| f.owner != ticket.owner)
        {
            return Err(refused());
        }
    }
    Ok(())
}

pub(super) fn project(state: &Match, slot: usize) -> Result<Value, SnapshotError> {
    validate(state)?;
    let g = &state.world.game;
    let mut fog = state.routing_fog.clone();
    let visible = &fog.team(g, &state.rules, g.players[slot].team).vis;
    let time = g.tick as f64 * 0.05;
    let mut rows = Vec::new();
    for flight in state.firing.flights.values() {
        if flight.launched > time {
            continue;
        }
        let at = projectiles::flight_at(flight.solution, time).map_err(|_| SnapshotError::Rules)?;
        if !shots::flight_visible(state, visible, Point { x: at.x, z: at.z }) {
            continue;
        }
        let velocity =
            projectiles::velocity_at(flight.solution, time).map_err(|_| SnapshotError::Rules)?;
        let mut row = json!({
            "flight":flight.id,"kind":state.rules.kinds[flight.source_kind].name,
            "effect":flight.effect,"seq":flight.sequence,"x":at.x,"y":at.y,"z":at.z,
            "v":[velocity.x,velocity.y,velocity.z],"gravity":flight.solution.gravity,
            "time":time,"launched":flight.launched,"expires":flight.expires,
        });
        let mut until = shots::until(state, visible, &row)?;
        if until > time {
            let end =
                projectiles::flight_at(flight.solution, until).map_err(|_| SnapshotError::Rules)?;
            if let Some(contact) = state
                .recipient_flight_contact(flight, at, end, |id| seen_by(state, slot, id))
                .map_err(|_| SnapshotError::Rules)?
            {
                until = time + (until - time) * contact.hit.fraction;
            }
        }
        row["until"] = json!(until);
        if seen_by(state, slot, flight.shooter) {
            row["f"] = json!(flight.shooter);
            row["fo"] = json!(flight.owner);
        }
        crate::match_firing::normalize_json_zero(&mut row);
        rows.push(row);
    }
    Ok(json!(rows))
}
