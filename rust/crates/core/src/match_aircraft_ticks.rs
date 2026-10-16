//! Aircraft unit-phase hooks for the sole Match tick owner, without clock or scheduling ownership.
use crate::{grid::UnitId, match_aviation::AviationError, match_runtime::Match};
impl Match {
    /// Pure complete-current admission for the sole recorded runner boundary.
    /// Commands retain ordinary denial semantics; their AIR branches preflight before selection writes.
    /// After dispatch, use validate_aircraft_runner_state before prior capture or clock mutation.
    pub fn validate_aircraft_runner_boundary(&self, boundary: u64) -> Result<(), AviationError> {
        self.validate_command_tick(boundary)
            .map_err(AviationError::Actuation)?;
        self.validate_aircraft_runner_state(boundary)
    }

    /// Pure complete-current check after boundary dispatch while Game still has the old clock.
    /// The command owner has advanced its queue already; its next boundary is not rechecked here.
    pub fn validate_aircraft_runner_state(
        &self,
        prospective_tick: u64,
    ) -> Result<(), AviationError> {
        if self.aviation.planes.keys().any(|id| {
            self.world
                .game
                .units
                .get(id)
                .is_none_or(|u| u.air.is_none())
        }) || self
            .aviation
            .anti_air
            .keys()
            .any(|id| !self.world.game.units.contains_key(id))
        {
            return Err(AviationError::Invalid("orphan aircraft runtime identity"));
        }
        let actors = self.world.game.units.keys().copied().collect::<Vec<_>>();
        self.validate_aircraft_tick_phases(&actors, self.rules.movement.tick, prospective_tick)
    }

    /// Admit the represented aircraft turns, late AA and retirement before the next clock.
    /// The tick owner still validates other phases and applies queued inputs in their own order.
    pub fn validate_aircraft_tick_phases(
        &self,
        actors: &[UnitId],
        dt: f64,
        prospective_tick: u64,
    ) -> Result<(), AviationError> {
        if u64::try_from(self.world.game.tick)
            .ok()
            .and_then(|tick| tick.checked_add(1))
            != Some(prospective_tick)
        {
            return Err(AviationError::Invalid(
                "aircraft phase requires the next tick",
            ));
        }
        self.validate_aircraft_turns(dt)?;
        self.validate_aircraft_anti_air(actors, dt)?;
        self.validate_dead_retirement(actors, prospective_tick)
            .map_err(|error| AviationError::Actuation(error.to_string()))
    }
    /// Pure before-clock admission for owned aircraft turns. Other phase owners add their own guards.
    pub fn validate_aircraft_turns(&self, dt: f64) -> Result<(), AviationError> {
        if !dt.is_finite() || !(0.0..=3600.0).contains(&dt) {
            return Err(AviationError::Invalid("invalid aircraft turn duration"));
        }
        if !self
            .world
            .game
            .units
            .values()
            .any(|u| u.air.is_some() || self.rules.kinds.get(u.kind).is_some_and(|k| k.air))
        {
            return Ok(());
        }
        self.validate_aviation()?;
        self.validate_aircraft_firing_context()
            .map_err(AviationError::Actuation)?;
        for story in &self.story {
            story.validate().map_err(AviationError::Actuation)?;
        }
        for (&id, u) in &self.world.game.units {
            if id == 0 || id != u.id {
                return Err(AviationError::Invalid("invalid aircraft phase identity"));
            }
            if u.air.is_none() {
                continue;
            }
            // The original unit loop skips dead actors before examining their waiting turn.
            if u.hp <= 0.0 {
                continue;
            }
            let control = &self.commands.units[&id];
            if control.operative || u.riding || self.rules.kinds[u.kind].rail {
                return Err(AviationError::Unsupported(
                    "aircraft turn conflicts with another actor owner",
                ));
            }
            self.validate_aircraft_step(id, dt)?;
        }
        Ok(())
    }
    /// Call after reserves, ordinary early orders and building handling, before ground timers.
    /// True consumes the unit turn; the caller must skip ordinary timers, movement and firing.
    pub fn step_aircraft_turn(&mut self, id: UnitId, dt: f64) -> Result<bool, AviationError> {
        let Some(u) = self.world.game.units.get(&id) else {
            return Ok(false);
        };
        let kind = self
            .rules
            .kinds
            .get(u.kind)
            .ok_or(AviationError::Invalid("invalid aircraft turn kind"))?;
        if !kind.air && u.air.is_none() {
            return Ok(false);
        }
        if !kind.air || u.air.is_none() {
            return Err(AviationError::Invalid("aircraft turn state/kind mismatch"));
        }
        if u.hp <= 0.0 {
            self.step_aircraft(id, dt)?;
            return Ok(true);
        }
        let c = self
            .commands
            .units
            .get(&id)
            .ok_or(AviationError::Invalid("missing aircraft turn control"))?;
        if c.operative || u.riding {
            return Err(AviationError::Unsupported(
                "aircraft turn conflicts with another actor owner",
            ));
        }
        self.step_aircraft(id, dt)?;
        Ok(true)
    }
}
