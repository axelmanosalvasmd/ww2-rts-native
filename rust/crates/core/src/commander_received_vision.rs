//! Sight geometry rebuilt from recipient terrain and delivered allied rows.
use crate::{
    catalog::Catalog,
    commander_ai::Observation,
    game::{Game, PlaneState, Player, Point, Smoke},
    grid::SpatialGrid,
    match_runtime::Match,
    math, unit_spawn, vision,
};
use std::collections::BTreeMap;

/// This value is only for sight queries. It contains no hidden actors or live terrain.
pub(crate) fn received_game(m: &Match, observation: &Observation) -> Result<Game, String> {
    let input = observation.input();
    let owner = input.slot;
    let routing = m.recipient_routing(owner)?;
    let initial = m.world.initial();
    let cells = input
        .width
        .checked_mul(input.height)
        .ok_or("received sight dimensions overflow")?;
    if input.width != m.world.game.w
        || input.height != m.world.game.h
        || input.players.len() != m.world.game.players.len()
        || initial.chars.len() != cells
        || input
            .players
            .iter()
            .zip(&m.world.game.players)
            .any(|(a, b)| a.team != b.team)
    {
        return Err("received sight packet binding mismatch".into());
    }
    let mut height = initial.height.clone().unwrap_or_else(|| vec![0; cells]);
    if height.len() != cells {
        return Err("invalid public sight heights".into());
    }
    for observed in routing.remembered() {
        let cell = observed.row.cell;
        if cell >= cells {
            return Err("invalid received sight cell".into());
        }
        height[cell] = observed.row.height;
    }
    let team = input.players[owner].team;
    let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
    let mut units = BTreeMap::new();
    for row in input
        .units
        .iter()
        .filter(|u| input.players[u.owner].team == team)
    {
        let kind = m
            .rules
            .kind(&row.kind)
            .ok_or("unknown received sight kind")?;
        let canonical = m
            .world
            .game
            .units
            .get(&row.id)
            .ok_or("missing delivered allied sight actor")?;
        if canonical.owner != row.owner || canonical.kind != kind {
            return Err("received sight actor binding mismatch".into());
        }
        let mut unit = unit_spawn::initial_unit(&m.rules, catalog, kind, row.id, row.owner, row.at)
            .map_err(|e| format!("received sight actor: {e:?}"))?;
        unit.hp = row.hp;
        unit.supp = row.supp;
        unit.built = canonical.built;
        unit.air = canonical.air.map(|_| {
            if vision::airborne(canonical) {
                PlaneState::Out
            } else {
                PlaneState::Base
            }
        });
        units.insert(row.id, unit);
    }
    let canonical = &m.world.game;
    let grid = SpatialGrid::new(&canonical.units, 16.0);
    let mut smokes = Vec::new();
    for smoke in &canonical.smokes {
        if canonical.reveal
            || vision::team_sees(
                canonical,
                &m.rules,
                &grid,
                team,
                Point {
                    x: smoke.x,
                    z: smoke.z,
                },
            )
        {
            if ![smoke.x, smoke.z, smoke.r].iter().all(|n| n.is_finite()) || smoke.r < 0.0 {
                return Err("invalid received smoke".into());
            }
            smokes.push(Smoke {
                id: smoke.id,
                remaining: None,
                x: math::round(smoke.x * 10.0) / 10.0,
                z: math::round(smoke.z * 10.0) / 10.0,
                r: smoke.r,
            });
        }
    }
    Ok(Game {
        w: input.width,
        h: input.height,
        flags: input.flags.clone(),
        height: Some(height),
        house: vec![0; cells],
        chars: input.chars.clone(),
        ground: None,
        objects: None,
        wear: None,
        cell_hp: vec![0.0; cells],
        trench_front: None,
        tough: None,
        wet: None,
        reveal: false,
        wrecks: Vec::new(),
        tick: input.tick,
        vision_tick: None,
        terrain_version: 0,
        mode: None,
        naval: false,
        weather: canonical.weather,
        rain: 0.0,
        players: input
            .players
            .iter()
            .enumerate()
            .map(|(slot, p)| Player {
                slot,
                team: p.team,
                visible: Vec::new(),
            })
            .collect(),
        units,
        smokes,
        strikes: Vec::new(),
        sections: BTreeMap::new(),
    })
}

#[cfg(test)]
#[path = "commander_received_vision_tests.rs"]
mod tests;
