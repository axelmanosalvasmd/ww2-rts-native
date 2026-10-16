//! Assault deployment and late victory through the shared ordinary tick phases.
use crate::{
    catalog::{Catalog, Value},
    economy::Mode,
    game::{CELL, Game, MOVE, ModeKind, Point},
    map::AuthoritativeMap,
    match_runtime::{Match, MatchOptions, StartPlan, foot_center, footprint, terrain_view},
    math,
    unit_publication::StagedUnit,
    unit_spawn::SpawnPlacer,
};

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AssaultState {
    pub defender_team: i32,
    /// Source uses -1 when the roster has no attacking player.
    pub attacker_team: i32,
    pub time_left: f64,
    pub total: usize,
}
fn number(v: &Value, key: &str) -> Result<f64, String> {
    v.get(key)
        .and_then(Value::number)
        .filter(|v| v.is_finite())
        .ok_or_else(|| format!("invalid Assault rule {key}"))
}
impl Match {
    /// Construct an Assault deployment for the ordinary tick runner.
    /// The explicit defender argument replaces the source lobby-only option. Team values on
    /// the canonical options/player tables are normalized together before any world is built.
    pub fn new_assault(
        map: &AuthoritativeMap,
        mut options: MatchOptions,
        defender_team: i32,
    ) -> Result<Self, String> {
        if options.mode != Mode::Assault || !(0..6).contains(&defender_team) {
            return Err("Assault startup requires Assault mode and defender team 0-5".into());
        }
        if options
            .players
            .iter()
            .any(|player| !(0..6).contains(&player.team))
        {
            return Err("Assault startup requires original team IDs 0-5".into());
        }
        // Legacy timed triggers use the existing shared shell state. New authored scenarios
        // need their own setup/runtime adapter and must not be silently dropped here.
        if map.source().get("scenario").is_some() {
            return Err("Assault startup does not yet apply authored scenarios".into());
        }
        let attacker_team = options
            .players
            .iter()
            .find(|p| p.team != defender_team)
            .map_or(-1, |p| p.team);
        for player in &mut options.players {
            if player.team != defender_team {
                player.team = attacker_team;
            }
        }
        // Source never enables logistics in Assault, including a true lobby option.
        options.logistics = false;
        let rules = crate::rules::Rules::compiled().map_err(|e| e.to_string())?;
        let start = StartPlan::prepare_assault(map, &options, &rules, defender_team)?;
        let mut result = Self::skirmish_shell(map, options, rules, start)?;
        result.world.game.mode = Some(ModeKind::Assault);
        let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
        let assault = catalog
            .table("CFG")
            .map_err(|e| e.to_string())?
            .get("assault")
            .ok_or("missing Assault rules")?;
        let time_left = map
            .assault_time()
            .map_or_else(|| number(assault, "time"), |v| Ok(v as f64))?;
        for owner in 0..result.start.players.len() {
            if result.start.players[owner].config.team == defender_team {
                result.fortify_assault(owner)?;
            }
        }
        let total = result
            .world
            .game
            .units
            .values()
            .filter(|u| result.rules.kinds[u.kind].name == "bunker")
            .count();
        result.assault = Some(AssaultState {
            defender_team,
            attacker_team,
            time_left,
            total,
        });
        result.complete_skirmish_start()?;
        Ok(result)
    }

    /// Source late Assault phase only. Caller supplies finite elapsed simulation time after
    /// income/convoys. Negative time preserves the source arithmetic; normal ticks use TICK.
    /// The tick runner dispatches this after its clock, death cleanup and income phases.
    pub fn apply_assault_victory(
        &mut self,
        dt: f64,
    ) -> Result<Option<crate::match_end::MatchFinish>, String> {
        if self.options.mode != Mode::Assault || self.world.game.mode != Some(ModeKind::Assault) {
            return Err("Assault victory requires explicit Assault mode".into());
        }
        if self.finish.is_some() {
            return Ok(None);
        }
        self.validate_match_finish_state()?;
        self.validate_sample_units()?;
        let state = self.assault.ok_or("missing Assault state")?;
        let time_left = state.time_left - dt;
        if !dt.is_finite() || !time_left.is_finite() {
            return Err("invalid Assault elapsed time".into());
        }
        let live_bunker = self
            .world
            .game
            .units
            .values()
            .any(|u| self.rules.kinds[u.kind].name == "bunker" && u.hp > 0.0);
        let decision = if !live_bunker {
            Some((
                state.attacker_team,
                crate::match_end::EndReason::Structures,
                self.fallen.bunker,
            ))
        } else if time_left <= 0.0 {
            Some((
                state.defender_team,
                crate::match_end::EndReason::Timer,
                None,
            ))
        } else {
            None
        };
        let prepared = decision
            .map(|(winner, reason, at)| self.prepare_match_finish(winner, reason, at))
            .transpose()?;
        self.assault
            .as_mut()
            .expect("validated Assault state")
            .time_left = time_left;
        Ok(prepared.map(|finish| self.publish_match_finish(finish)))
    }

    pub(crate) fn fortify_assault(&mut self, owner: usize) -> Result<(), String> {
        let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
        let assault = catalog
            .table("CFG")
            .map_err(|e| e.to_string())?
            .get("assault")
            .ok_or("missing Assault rules")?;
        let spawn = self.start.players[owner].spawn;
        let toward = math::atan2(
            self.world.game.h as f64 * CELL / 2.0 - spawn.z,
            self.world.game.w as f64 * CELL / 2.0 - spawn.x,
        );
        let radius = number(assault, "fortRadius")?;
        let mut a: f64 = -0.9;
        while a <= 0.9 {
            for (rr, ch, gap) in [(radius, b'T', 0.35), (radius + 2.0, b'#', 0.25)] {
                if ((a / gap) % 2.0).abs() > 1.6 {
                    continue;
                }
                let at = Point {
                    x: spawn.x + math::cos(toward + a) * rr * CELL,
                    z: spawn.z + math::sin(toward + a) * rr * CELL,
                };
                if let Some(c) = self.world.game.cell_of(at.x, at.z)
                    && self.world.game.chars[c] == b'.'
                {
                    self.world.set_cell(c, ch).map_err(|e| e.to_string())?;
                }
            }
            a += 0.04;
        }
        let mut placer = SpawnPlacer::new();
        let kind = self.rules.kind("bunker").ok_or("missing bunker kind")?;
        // Source first scatters the bunker, then overrides its position and heading.
        let mut bunker = StagedUnit::ordinary(
            &self.world.game,
            &self.rules,
            owner,
            kind,
            self.next_unit_id,
            self.world.game.units.len() as u32,
            spawn,
            None,
        )
        .map_err(|e| format!("Assault bunker: {e:?}"))?;
        let c = placer
            .nearest_free(
                terrain_view(&self.world.game),
                Point {
                    x: spawn.x + math::cos(toward) * 8.0 * CELL,
                    z: spawn.z + math::sin(toward) * 8.0 * CELL,
                },
                MOVE,
                None,
            )
            .map_err(|e| format!("Assault bunker placement: {e:?}"))?;
        let bunker_at = Point {
            x: (c % self.world.game.w) as f64 * CELL + CELL / 2.0,
            z: (c / self.world.game.w) as f64 * CELL + CELL / 2.0,
        };
        bunker.unit.x = bunker_at.x;
        bunker.unit.z = bunker_at.z;
        bunker.unit.rot = toward;
        bunker.control.aim = toward;
        self.publish_starting_unit(bunker)?;
        let size = number(catalog.unit("flakpos").map_err(|e| e.to_string())?, "size")? as usize;
        let bunker_radius = self.rules.kinds[kind].radius;
        for i in 0..number(assault, "baseFlak")? as usize {
            let side = if i % 2 != 0 { -1.0 } else { 1.0 };
            let corner = toward + side * ([0.9, 0.9, 2.6, 2.6][i % 4] + (i / 4) as f64 * 0.35);
            'place: for rr in [10.0, 11.0, 9.0, 12.0, 8.0, 7.0, 13.0, 6.0] {
                for swing in [0.0, 0.3, -0.3, 0.6, 1.0, 1.5] {
                    let a = corner - side * swing;
                    let Some(c) = self.world.game.cell_of(
                        spawn.x + math::cos(a) * rr * CELL - size as f64 * CELL / 2.0,
                        spawn.z + math::sin(a) * rr * CELL - size as f64 * CELL / 2.0,
                    ) else {
                        continue;
                    };
                    let Some(cells) = footprint(&self.world.game, c, size) else {
                        continue;
                    };
                    if !can_stamp_start(&self.world.game, &cells) {
                        continue;
                    }
                    if cells.iter().any(|&cell| {
                        let at = Point {
                            x: (cell % self.world.game.w) as f64 * CELL + CELL / 2.0,
                            z: (cell / self.world.game.w) as f64 * CELL + CELL / 2.0,
                        };
                        math::hypot(at.x - bunker_at.x, at.z - bunker_at.z)
                            < bunker_radius + if rr > 8.0 { 8.0 } else { 1.0 }
                    }) {
                        continue;
                    }
                    let at = foot_center(&self.world.game, c, size);
                    if self.world.game.units.values().any(|u| {
                        self.rules.kinds[u.kind].name == "flakpos"
                            && math::hypot(u.x - at.x, u.z - at.z)
                                < if rr > 8.0 { 14.0 } else { 8.0 }
                    }) {
                        continue;
                    }
                    self.place_starting_building(owner, "flakpos", c)?;
                    break 'place;
                }
            }
        }
        Ok(())
    }
}
fn can_stamp_start(game: &Game, cells: &[usize]) -> bool {
    let level = |c: usize| game.height.as_ref().map_or(0, |height| height[c]);
    cells
        .iter()
        .all(|&c| game.flags[c] & MOVE == 0 && !b"WF=KOAQ".contains(&game.chars[c]))
        && i16::from(cells.iter().map(|&c| level(c)).max().unwrap_or(0))
            - i16::from(cells.iter().map(|&c| level(c)).min().unwrap_or(0))
            <= 1
}

#[cfg(test)]
mod tests;
