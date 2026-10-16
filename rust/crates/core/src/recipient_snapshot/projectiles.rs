//! Prototype retained flight rows. Internal grenade and salvo tickets have no wire row.
use super::*;

pub(super) fn validate(state: &Match) -> Result<(), SnapshotError> {
    if !state.firing.flights.is_empty() {
        state
            .routing_fog
            .validate_shape(&state.world.game)
            .map_err(|_| SnapshotError::Rules)?;
    }
    if !state.firing.flights.is_empty() || !state.firing.salvos.is_empty() {
        state
            .validate_firing_tick_state()
            .map_err(|_| SnapshotError::Rules)?;
    }
    if !state.firing.grenades.is_empty() {
        state
            .validate_firing_grenade_step(0.0)
            .map_err(|_| SnapshotError::Rules)?;
    }
    if state.firing.salvos.iter().any(|s| {
        s.left == 0
            || s.left > 4096
            || s.owner >= state.world.game.players.len()
            || ![s.at.x, s.at.z, s.next, s.every, s.spread]
                .iter()
                .all(|v| v.is_finite())
            || s.every <= 0.0
            || s.spread < 0.0
    }) {
        return Err(SnapshotError::Rules);
    }
    Ok(())
}

#[cfg(test)]
pub(super) fn for_seat(state: &Match, slot: usize) -> Result<Value, SnapshotError> {
    super::flights::project(state, slot)
}

#[cfg(test)]
#[path = "projectiles/tests.rs"]
mod tests;
