//! Operative turn provider for the single Match tick runner.
use crate::{
    companions::{CombatRequest, RESPAWN_TICKS, SAFE_INTEGER},
    economy::Mode,
    match_runtime::Match,
};

impl Match {
    /// Validate the identity join and next clock before a tick publishes phases.
    /// The empty registry leaves the ordinary runner's existing clock bounds intact.
    pub(crate) fn prepare_companion_tick(&self) -> Result<(), String> {
        if self.companions.records.is_empty() && self.companions.soldiers.is_empty() {
            if self
                .commands
                .units
                .values()
                .any(|control| control.operative)
            {
                return Err("operative control has no companion registry".into());
            }
            return Ok(());
        }
        if !matches!(self.options.mode, Mode::Default | Mode::Conquest)
            || self.world.game.mode.is_some()
            || !(0..SAFE_INTEGER - RESPAWN_TICKS).contains(&self.world.game.tick)
        {
            return Err("invalid operative tick mode or clock".into());
        }
        self.check_companion_lifecycle()?;
        for op in self.companions.records.values() {
            if op.unit_id != 0
                && self
                    .companions
                    .soldiers
                    .get(&op.unit_id)
                    .is_none_or(|body| body.companion != op.id)
            {
                return Err("companion record has no matching soldier".into());
            }
        }
        for unit in self.companions.soldiers.keys() {
            self.check_companion(*unit)?;
        }
        for (id, control) in &self.commands.units {
            if !control.operative {
                continue;
            }
            if *id == 0 || *id >= self.next_unit_id {
                return Err("operative control identity is outside the allocator".into());
            }
            if self.world.game.units.contains_key(id) {
                if self.world.game.units[id].id != *id {
                    return Err("operative unit identity does not match its map key".into());
                }
                if !self.companions.soldiers.contains_key(id) {
                    return Err("operative control has no matching soldier".into());
                }
            } else if self.firing.get(*id).is_none()
                || self.combat.get(*id).is_none()
                || self.companions.soldiers.contains_key(id)
                || self.companions.records.values().any(|op| op.unit_id == *id)
            {
                // Generic retirement retains these rows while flights may refer
                // to their source. A future or unbacked control is not a retiree.
                return Err("operative control has no live or retained identity".into());
            }
        }
        Ok(())
    }

    /// Run instead of the ordinary unit branch, including its combat request now.
    /// Positions, timers, input and press state are mutated by step_companion once;
    /// damage, random draws and flights remain with their canonical effect methods.
    pub(crate) fn step_companion_turn(
        &mut self,
        unit: u32,
        winner_decided: bool,
    ) -> Result<(), String> {
        let winner_decided = winner_decided || self.finish.is_some();
        let result = self.step_companion(unit, winner_decided)?;
        match result.combat {
            Some(CombatRequest::Fire { unit, input }) => {
                self.fire_companion(unit, &input, winner_decided)?;
            }
            Some(CombatRequest::Grenade { unit, input }) => {
                self.throw_companion(unit, &input, winner_decided)?;
            }
            None => {}
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        economy::Faction,
        economy_state::ArmySize,
        map::AuthoritativeMap,
        match_runtime::{MatchOptions, PlayerConfig},
        weather::WeatherChoice,
    };

    #[test]
    fn next_tick_reserves_the_entire_operative_death_delay_before_mutation() {
        let map = AuthoritativeMap::from_json(
            &serde_json::to_vec(&serde_json::json!({
                "name":"Operative clock admission", "w":64, "h":64,
                "rows":vec![".".repeat(64);64],
                "spawns":[{"x":10,"y":32},{"x":54,"y":32}],
                "points":[{"id":"inert","x":32,"y":32}],
            }))
            .unwrap(),
        )
        .unwrap();
        let mut m = Match::new(
            &map,
            MatchOptions {
                seed: 701,
                mode: Mode::Conquest,
                players: vec![
                    PlayerConfig {
                        name: "Commander".into(),
                        team: 0,
                        faction: Faction::Usa,
                    },
                    PlayerConfig {
                        name: "Enemy".into(),
                        team: 1,
                        faction: Faction::Germany,
                    },
                ],
                shuffle: false,
                army: ArmySize::Standard,
                weather: WeatherChoice::Map,
                weather_seed: Some(1),
                living_weather: false,
                map_key: None,
                tech: false,
                supply: false,
                logistics: false,
                sandbox: false,
            },
        )
        .unwrap();
        m.join_companion("clock", 0, false).unwrap();
        m.world.game.tick = SAFE_INTEGER - RESPAWN_TICKS;
        let before = format!(
            "{:?}",
            (
                &m.world,
                &m.companions,
                &m.start.economy,
                m.commands.queue.snapshot(),
                m.start.random.snapshot(),
                m.next_unit_id
            )
        );
        // Exercise this provider's boundary directly. The current full runner
        // already imposes a stricter ordinary clock limit before this guard.
        let error = m.prepare_companion_tick().unwrap_err();
        assert!(error.contains("operative tick mode or clock"), "{error}");
        assert_eq!(
            format!(
                "{:?}",
                (
                    &m.world,
                    &m.companions,
                    &m.start.economy,
                    m.commands.queue.snapshot(),
                    m.start.random.snapshot(),
                    m.next_unit_id
                )
            ),
            before
        );
        assert!(m.tick_state.fault().is_none());
    }
}
