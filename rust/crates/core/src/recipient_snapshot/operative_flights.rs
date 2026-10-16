use super::*;

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
    for (id, flight) in state.firing.flights.iter().filter(|(_, flight)| {
        state.commands.units.get(&flight.shooter).is_some_and(|c| c.operative)
    }) {
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
    for ticket in state.firing.grenades.iter().filter(|ticket| {
        state.firing.flights.get(&ticket.flight).is_some_and(|flight| {
            state.commands.units.get(&flight.shooter).is_some_and(|c| c.operative)
        })
    }) {
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
