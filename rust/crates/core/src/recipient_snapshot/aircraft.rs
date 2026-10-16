//! Existing v0 owner aircraft rows and mission plans, without aviation events.
use super::*;
use crate::{
    game::{PlaneState, Point},
    match_aviation::MissionKind,
};

pub(super) fn validate(state: &Match) -> Result<(), SnapshotError> {
    let units = &state.world.game.units;
    if state.aviation.planes.is_empty()
        && !units
            .values()
            .any(|u| u.air.is_some() || state.rules.kinds[u.kind].air)
    {
        return Ok(());
    }
    state
        .validate_aviation()
        .map_err(|_| SnapshotError::Rules)?;
    if state
        .aviation
        .planes
        .keys()
        .any(|id| units.get(id).is_none_or(|u| u.air.is_none()))
    {
        return Err(SnapshotError::Rules);
    }
    Ok(())
}

pub(super) fn row(state: &Match, id: u32, air: PlaneState) -> Result<Value, SnapshotError> {
    let runtime = state
        .aviation
        .planes
        .get(&id)
        .ok_or(SnapshotError::MissingState {
            unit: id,
            owner: "aviation",
        })?;
    finite(&[runtime.fuel, runtime.ammo, runtime.timer])?;
    // This wire order is explicit in snapshotCache at the pin. PlaneState's
    // Rust declaration has a different order and must never be cast to a number.
    let code = match air {
        PlaneState::Base => 0,
        PlaneState::Out => 1,
        PlaneState::Station => 2,
        PlaneState::Home => 3,
        PlaneState::Rearm => 4,
    };
    Ok(json!([
        id,
        code,
        runtime.fuel.ceil(),
        runtime.ammo,
        runtime.timer.ceil()
    ]))
}

pub(super) fn plan(state: &Match, u: &Unit) -> (u8, Point) {
    if vision::airborne(u)
        && let Some(mission) = state.aviation.planes[&u.id].mission
    {
        let code = match mission.kind {
            MissionKind::Attack => 4,
            MissionKind::Area => 5,
            MissionKind::Patrol | MissionKind::Escort => 1,
        };
        (code, mission.at)
    } else {
        (0, Point { x: 0., z: 0. })
    }
}

#[cfg(test)]
#[path = "aircraft_tests.rs"]
mod tests;
