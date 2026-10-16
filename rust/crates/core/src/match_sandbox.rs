//! Prototype sandbox spawning uses ordinary actors, visibility and lifecycle owners.
use crate::{
    game::{CELL, Point},
    match_commands::Denial,
    match_runtime::{Match, terrain_view},
    unit_publication::{PublicationParts, StagedUnit, prepare_publication},
    unit_spawn::SpawnPlacer,
};

impl Match {
    /// Free test-tool spawn. The canonical command caller owns player admission.
    pub(crate) fn sandbox_spawn(
        &mut self,
        slot: usize,
        name: &str,
        at: Point,
        enemy: bool,
    ) -> Result<Option<Denial>, String> {
        let Some(kind) = self.rules.kind(name) else {
            return Ok(Some(Denial::Blocked));
        };
        if !self.options.sandbox
            || self.rules.kinds[kind].structure
            || !at.x.is_finite()
            || !at.z.is_finite()
            || self.world.game.units.len() > 2000
        {
            return Ok(Some(Denial::Blocked));
        }
        let owner = if !enemy {
            Some(slot)
        } else if self.options.mode == crate::economy::Mode::Horde {
            self.horde.as_ref().map(|h| h.slot)
        } else {
            self.world
                .game
                .players
                .iter()
                .position(|p| !self.world.game.allied(p.slot, slot))
        };
        let Some(owner) = owner else {
            return Ok(Some(Denial::Blocked));
        };
        let air = self.rules.kinds[kind].air;
        if air {
            self.spawn_aircraft(owner, name)
                .map_err(|e| format!("sandbox aircraft: {e}"))?;
        } else {
            let cell = SpawnPlacer::new()
                .nearest_free(
                    terrain_view(&self.world.game),
                    at,
                    self.rules.kinds[kind].block(),
                    None,
                )
                .map_err(|e| format!("sandbox placement: {e:?}"))?;
            let pose = Point {
                x: (cell % self.world.game.w) as f64 * CELL + CELL / 2.0,
                z: (cell / self.world.game.w) as f64 * CELL + CELL / 2.0,
            };
            let id = self.next_unit_id;
            let mut draft = StagedUnit::ordinary(
                &self.world.game,
                &self.rules,
                owner,
                kind,
                id,
                u32::try_from(self.world.game.units.len()).map_err(|_| "sandbox actor count")?,
                self.start.players[owner].spawn,
                Some(pose),
            )
            .map_err(|e| format!("sandbox actor: {e:?}"))?;
            if let Some(horde) = self.horde.as_ref()
                && owner == horde.slot
                && !self.rules.kinds[kind].rail
                && let Some(target) = self.world.game.units.get(&horde.bunker).map(|u| u.at())
            {
                draft.unit.amove = Some(target);
                draft.unit.path = self.horde_path(name, pose, target)?;
                draft.control.outcome = Some(crate::match_commands::MoveOutcome::Moving);
                draft.control.outcome_tick = Some(self.world.game.tick);
            }
            let reserves = self.prepare_new_unit_reserves(&draft.unit)?;
            prepare_publication(
                PublicationParts {
                    units: &mut self.world.game.units,
                    grid: &mut self.grid,
                    controls: &mut self.commands.units,
                    recruitment: &mut self.recruit_metadata,
                    next_id: &mut self.next_unit_id,
                },
                &self.rules,
                self.start.players.len(),
                vec![draft],
            )
            .map_err(|e| format!("sandbox publication: {e:?}"))?
            .publish_with(|unit, _| {
                self.combat.initialize(unit.id).hit_by = Some(0);
                self.firing.initialize(unit.id);
                self.abilities.initialize(unit.id);
                self.recovery.initialize(unit.id);
                if let Some(reserves) = reserves.clone() {
                    self.logistics.insert(unit.id, reserves);
                }
            });
        }
        if let Some(horde) = self.horde.as_ref()
            && owner == horde.slot
        {
            let horde = self.horde.as_mut().expect("existing Horde state");
            if !horde.active {
                horde.active = true;
                horde.time_left = 0.0;
            }
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::match_commands::Command;
    use crate::{
        economy::{Faction, Mode},
        economy_state::ArmySize,
        map::AuthoritativeMap,
        match_runtime::{MatchOptions, PlayerConfig},
        weather::WeatherChoice,
    };
    use serde_json::json;

    fn scene(horde: bool) -> Match {
        let map = AuthoritativeMap::from_json(
            &serde_json::to_vec(&json!({
                "name":"Sandbox intent", "w":60, "h":60,
                "rows":vec![".".repeat(60);60],
                "spawns":[{"x":30,"y":5},{"x":10,"y":54},{"x":50,"y":54}],
                "defend":[0], "points":[{"x":30,"y":30},{"x":35,"y":30,"mp":0}]
            }))
            .unwrap(),
        )
        .unwrap();
        let options = MatchOptions {
            seed: 701,
            mode: if horde { Mode::Horde } else { Mode::Conquest },
            players: (0..if horde { 2 } else { 3 })
                .map(|slot| PlayerConfig {
                    name: format!("P{slot}"),
                    team: if !horde && slot == 2 { 0 } else { slot },
                    faction: Faction::Usa,
                })
                .collect(),
            shuffle: false,
            army: ArmySize::Standard,
            weather: WeatherChoice::Map,
            weather_seed: Some(1),
            living_weather: false,
            map_key: None,
            tech: false,
            supply: false,
            logistics: false,
            sandbox: true,
        };
        if horde {
            Match::new_horde(&map, options)
        } else {
            Match::new(&map, options)
        }
        .unwrap()
    }

    #[test]
    fn sandbox_enemy_uses_first_nonallied_roster_seat_without_payment() {
        let mut m = scene(false);
        let before = m.start.economy.clone();
        let id = m.next_unit_id;
        let command =
            Command::from_json(&json!({"t":"spawn", "unit":"tiger", "x":41, "z":41, "enemy":[]}))
                .unwrap();
        assert_eq!(
            command,
            Command::Spawn {
                kind: "tiger".into(),
                at: Point { x: 41.0, z: 41.0 },
                enemy: true,
            }
        );
        assert_eq!(m.execute_command(2, &command, false).unwrap(), None);
        let u = &m.world.game.units[&id];
        assert_eq!(u.owner, 1, "skip the earlier allied seat");
        assert_eq!(m.rules.kinds[u.kind].name, "tiger");
        assert_eq!(u.at(), Point { x: 41.0, z: 41.0 });
        assert_eq!(m.start.economy, before);
        assert!(m.commands.units.contains_key(&id));
        assert!(m.combat.get(id).is_some());
        assert!(m.firing.get(id).is_some());
        assert!(m.abilities.get(id).is_some());
        assert!(m.recovery.get(id).is_some());
        let guest_id = m.next_unit_id;
        assert_eq!(
            m.sandbox_spawn(2, "rifle", Point { x: 41.0, z: 41.0 }, false)
                .unwrap(),
            None
        );
        assert_eq!(m.world.game.units[&guest_id].owner, 2);
        for p in &mut m.world.game.players {
            p.team = 9;
        }
        let next_id = m.next_unit_id;
        assert_eq!(
            m.sandbox_spawn(2, "rifle", Point { x: 41.0, z: 41.0 }, true)
                .unwrap(),
            Some(Denial::Blocked)
        );
        assert_eq!(m.next_unit_id, next_id);
    }

    #[test]
    fn sandbox_flood_guard_allows_spawn_at_2000_then_blocks_at_2001() {
        let mut m = scene(false);
        while m.world.game.units.len() < 2000 {
            assert_eq!(
                m.sandbox_spawn(0, "rifle", Point { x: 41.0, z: 41.0 }, false)
                    .unwrap(),
                None
            );
        }
        assert_eq!(
            m.sandbox_spawn(0, "rifle", Point { x: 41.0, z: 41.0 }, false)
                .unwrap(),
            None
        );
        assert_eq!(m.world.game.units.len(), 2001);
        let next_id = m.next_unit_id;
        assert_eq!(
            m.sandbox_spawn(0, "rifle", Point { x: 41.0, z: 41.0 }, false)
                .unwrap(),
            Some(Denial::Blocked)
        );
        assert_eq!(m.next_unit_id, next_id);
    }

    #[test]
    fn sandbox_horde_enemy_activates_wave_zero_and_attacks_bunker() {
        let mut m = scene(true);
        let id = m.next_unit_id;
        let horde = m.horde.as_ref().unwrap();
        assert_eq!(horde.wave, 0);
        assert!(!horde.active);
        let slot = horde.slot;
        let target = m.world.game.units[&horde.bunker].at();
        assert_eq!(
            m.execute_command(
                1,
                &Command::Spawn {
                    kind: "kaiju".into(),
                    at: Point { x: 41.0, z: 41.0 },
                    enemy: true,
                },
                false
            )
            .unwrap(),
            None
        );
        assert_eq!(m.world.game.units[&id].owner, slot);
        assert_eq!(m.world.game.units[&id].amove, Some(target));
        assert!(m.horde.as_ref().unwrap().active);
        assert_eq!(m.horde.as_ref().unwrap().time_left, 0.0);
        m.step_horde(0.05).unwrap();
        assert_eq!(m.horde.as_ref().unwrap().wave, 0);
        assert!(m.horde.as_ref().unwrap().active);
        let mode = crate::recipient_snapshot::horde_mode_row(&m).unwrap();
        assert_eq!(mode["active"], true);
        assert!(mode.get("boss").is_none());
        assert!(mode.get("profile").is_none());
        assert_eq!(m.horde_projection().unwrap().boss, Some([6000.0, 6000.0]));
        m.world.game.units.get_mut(&id).unwrap().hp = 0.0;
        m.step_horde(0.05).unwrap();
        assert!(!m.horde.as_ref().unwrap().active);
        let bunker = m.horde.as_ref().unwrap().bunker;
        m.world.game.units.get_mut(&bunker).unwrap().hp = 0.0;
        m.step_horde(0.05).unwrap();
        assert!(m.match_result().unwrap().is_some());
        assert!(m.horde_result().unwrap().is_none());
    }
}
