//! Current falling-section delivery from the existing Environment owner.
//! Source debrisRow fields are unrounded; this reader owns no motion or memory.
use super::*;
use crate::{
    environment::FallingSection, environment_debris::DebrisKind, game::Point, grid::SpatialGrid,
};

pub(super) fn project(state: &Match, slot: usize) -> Result<Value, SnapshotError> {
    project_perspective(state, slot, false)
}

pub(super) fn project_perspective(
    state: &Match,
    slot: usize,
    full_map: bool,
) -> Result<Value, SnapshotError> {
    let g = &state.world.game;
    let player = g.players.get(slot).ok_or(SnapshotError::InvalidRecipient)?;
    // Source teamSees uses current objects, independent of cached visible IDs
    // and browser fog-mask omission. This local read changes no maintained index.
    let grid = SpatialGrid::new(&g.units, 16.);
    let mut rows = Vec::new();
    for section in state.environment.falling_sections() {
        if section.body.settled {
            continue;
        }
        if ![section.body.x, section.body.z]
            .iter()
            .all(|n| n.is_finite())
        {
            return Err(SnapshotError::InvalidNumber);
        }
        if full_map
            || g.reveal
            || vision::team_sees(
                g,
                &state.rules,
                &grid,
                player.team,
                Point {
                    x: section.body.x,
                    z: section.body.z,
                },
            )
        {
            rows.push(row(section)?);
        }
    }
    Ok(Value::Array(rows))
}

fn row(section: &FallingSection) -> Result<Value, SnapshotError> {
    let body = section.body.row();
    if body.kind != DebrisKind::Section {
        return Err(SnapshotError::Rules);
    }
    if ![
        body.x,
        body.y,
        body.z,
        body.dir,
        body.mass,
        body.v[0],
        body.v[1],
        body.v[2],
        body.gravity,
        body.drag,
        body.time,
        body.born,
        body.tilt,
        section.at,
        section.impulse,
    ]
    .iter()
    .all(|n| n.is_finite())
        || section.until > 9_007_199_254_740_991
    {
        return Err(SnapshotError::InvalidNumber);
    }
    Ok(json!({
        "id": body.id,
        "kind": body.kind.as_str(),
        "x": body.x, "y": body.y, "z": body.z,
        "dir": body.dir, "material": body.material, "mass": body.mass,
        "v": body.v, "gravity": body.gravity, "drag": body.drag,
        "time": body.time, "born": body.born, "tilt": body.tilt,
        "settled": body.settled,
        "section": section.section, "at": section.at,
        "until": section.until, "impulse": section.impulse,
    }))
}

#[cfg(test)]
#[path = "falling_tests.rs"]
mod tests;
