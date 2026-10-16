//! Prototype retained flight rows. Internal grenade and salvo tickets have no wire row.
use super::*;
use crate::{
    game::Point,
    grid::SpatialGrid,
    match_firing::LiveFlight,
    projectiles::{self as geometry, BodyPosition, Box3, HullPosition, Point3},
};

pub(super) fn validate(state: &Match) -> Result<(), SnapshotError> {
    if !state.firing.flights.is_empty() {
        state
            .routing_fog
            .validate_shape(&state.world.game)
            .map_err(|_| SnapshotError::Rules)?;
        state
            .validate_firing_tick_state()
            .map_err(|_| SnapshotError::Rules)?;
        if state
            .firing
            .flights
            .values()
            .any(|p| state.rules.kinds[p.source_kind].air)
        {
            return Err(SnapshotError::Unsupported {
                unit: 0,
                capability: SnapshotCapability::Air,
            });
        }
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

/// Static extrapolation uses current bodies and only recipients' seen units,
/// matching flightContact with a fresh prior map and fractions (1, 1).
fn contact_fraction(
    state: &Match,
    slot: usize,
    p: &LiveFlight,
    a: Point3,
    b: Point3,
) -> Result<Option<f64>, SnapshotError> {
    let g = &state.world.game;
    let height = Catalog::compiled()
        .map_err(|_| SnapshotError::Rules)?
        .table("CFG")
        .map_err(|_| SnapshotError::Rules)?
        .get("levelHeight")
        .and_then(crate::catalog::Value::number)
        .ok_or(SnapshotError::Rules)?;
    let mut best: Option<f64> = None;
    let mut retain = |fraction: f64| {
        if best.is_none_or(|old| fraction < old) {
            best = Some(fraction);
        }
    };
    for c in geometry::crossed_cells(
        BodyPosition { x: a.x, z: a.z },
        BodyPosition { x: b.x, z: b.z },
        CELL,
        g.w,
        g.h,
    )
    .map_err(|_| SnapshotError::Rules)?
    {
        let ch = g.physical(c);
        if p.sequence == 0
            && p.start_cell < g.flags.len()
            && (c == p.start_cell
                || (ch == b'#'
                    && (c % g.w).abs_diff(p.start_cell % g.w) <= 1
                    && (c / g.w).abs_diff(p.start_cell / g.w) <= 1))
        {
            continue;
        }
        let x = (c % g.w) as f64 * CELL;
        let z = (c / g.w) as f64 * CELL;
        let ground = g.height.as_ref().map_or(0.0, |h| f64::from(h[c])) * height;
        let top = ground
            + match ch {
                b'B' | b'K' => 5.0,
                b'H' => 1.8,
                b'#' | b'Y' => 1.2,
                b'Q' => 1.6,
                b'R' | b'O' => 0.8,
                b'=' => 0.25,
                _ => 0.0,
            };
        if let Some(hit) = geometry::sweep_box(
            a,
            b,
            Box3 {
                x: [x, x + CELL],
                y: [-100.0, top],
                z: [z, z + CELL],
            },
        )
        .map_err(|_| SnapshotError::Rules)?
        {
            retain(hit.fraction);
        }
    }
    let center = ((a.x + b.x) / 2.0, (a.z + b.z) / 2.0);
    let reach = math::hypot(b.x - a.x, b.z - a.z) / 2.0 + 14.0 + if g.naval { 55.0 } else { 0.0 };
    let grid = SpatialGrid::new(&g.units, 16.0);
    for id in grid.in_order(&g.units, center, reach, |_, t| {
        t.hp > 0.0
            && t.air.is_none()
            && !t.riding
            && t.id != p.shooter
            && seen_by(state, slot, t.id)
            && (p.blast_source.is_some() || !g.allied(p.owner, t.owner))
    }) {
        let t = &g.units[&id];
        let def = &state.rules.kinds[t.kind];
        if def.building && t.cells.as_ref().is_some_and(|cells| !cells.is_empty()) {
            continue;
        }
        let ground = f64::from(g.level_at(t.x, t.z)) * height;
        let top = ground
            + if def.structure || def.naval {
                5.0
            } else if def.infantry {
                1.8
            } else {
                2.6
            };
        let radius = def.radius * 0.8;
        let hit = if def.hull != 0.0 {
            let at = HullPosition {
                x: t.x,
                z: t.z,
                rotation: Some(t.rot),
            };
            geometry::sweep_hull(a, b, at, at, radius, def.hull, ground, top)
        } else {
            let at = BodyPosition { x: t.x, z: t.z };
            geometry::sweep_body(a, b, at, at, radius, ground, top)
        }
        .map_err(|_| SnapshotError::Rules)?;
        if let Some(hit) = hit {
            retain(hit.fraction);
        }
    }
    Ok(best)
}

pub(super) fn for_seat(state: &Match, slot: usize) -> Result<Value, SnapshotError> {
    let g = &state.world.game;
    let team = g
        .players
        .get(slot)
        .ok_or(SnapshotError::InvalidRecipient)?
        .team;
    let mut fog = state.routing_fog.clone();
    let visible = &fog.team(g, &state.rules, team).vis;
    let time = g.tick as f64 * 0.05;
    let mut rows = Vec::new();
    for p in state.firing.flights.values() {
        if p.launched > time {
            continue;
        }
        let at = geometry::flight_at(p.solution, time).map_err(|_| SnapshotError::Rules)?;
        if !shots::flight_visible(state, visible, Point { x: at.x, z: at.z }) {
            continue;
        }
        let v = geometry::velocity_at(p.solution, time).map_err(|_| SnapshotError::Rules)?;
        let mut row = json!({"flight":p.id,"kind":state.rules.kinds[p.source_kind].name,"seq":p.sequence,
            "x":at.x,"y":at.y,"z":at.z,"v":[v.x,v.y,v.z],"gravity":p.solution.gravity,
            "time":time,"launched":p.launched,"expires":p.expires});
        let mut until = shots::until(state, visible, &row)?;
        if state.rules.kinds[p.source_kind].weapon && until > time {
            let end = geometry::flight_at(p.solution, until).map_err(|_| SnapshotError::Rules)?;
            if let Some(fraction) = contact_fraction(state, slot, p, at, end)? {
                until = time + (until - time) * fraction;
            }
        }
        row["until"] = json!(until);
        if let Some(effect) = p.effect {
            row["effect"] = json!(effect);
        }
        if seen_by(state, slot, p.shooter) {
            row["f"] = json!(p.shooter);
            row["fo"] = json!(p.owner);
        }
        crate::match_firing::normalize_json_zero(&mut row);
        rows.push(row);
    }
    Ok(json!(rows))
}

#[cfg(test)]
#[path = "projectiles/tests.rs"]
mod tests;
