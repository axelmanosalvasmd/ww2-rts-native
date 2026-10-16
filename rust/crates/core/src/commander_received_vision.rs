//! Sight geometry rebuilt from recipient terrain and delivered allied rows.
use crate::{
    catalog::Catalog,
    commander_ai::Observation,
    game::{Game, PlaneState, Player},
    match_runtime::Match,
    unit_spawn,
};
use std::collections::BTreeMap;

/// This value is only for sight queries. It contains no hidden actors or live terrain.
pub(crate) fn received_game(m: &Match, observation: &Observation) -> Result<Game, String> {
    let input = observation.input();
    let owner = input.slot;
    m.validate_recipient(owner)?;
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
    let context = input
        .runtime
        .as_ref()
        .ok_or("missing delivered sight facts")?;
    let height = context.height.clone().unwrap_or_else(|| vec![0; cells]);
    if height.len() != cells {
        return Err("invalid public sight heights".into());
    }
    // Sight geometry only needs the delivered multiplier, not the live weather state.
    let weather = if context.sight == 1.0 {
        None
    } else {
        Some(
            m.rules
                .weather
                .iter()
                .position(|weather| !weather.rains && weather.sight == context.sight)
                .ok_or("unrepresented delivered sight multiplier")?,
        )
    };
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
        let mut unit = if m.rules.kinds[kind].air {
            unit_spawn::initial_aircraft(&m.rules, catalog, kind, row.id, row.owner, row.at)
        } else {
            unit_spawn::initial_unit(&m.rules, catalog, kind, row.id, row.owner, row.at)
        }
        .map_err(|e| format!("received sight actor: {e:?}"))?;
        unit.hp = row.hp;
        unit.supp = row.supp;
        let facts = context
            .units
            .get(&row.id)
            .ok_or("missing delivered allied sight facts")?;
        unit.built = Some(facts.built);
        unit.air = m.rules.kinds[kind].air.then_some(if facts.airborne {
            PlaneState::Out
        } else {
            PlaneState::Base
        });
        units.insert(row.id, unit);
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
        weather,
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
        smokes: context.smokes.clone(),
        strikes: Vec::new(),
        sections: BTreeMap::new(),
    })
}

#[cfg(test)]
#[path = "commander_received_vision_tests.rs"]
mod tests;
