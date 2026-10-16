//! Aircraft unit-phase hooks for the sole Match tick owner, without clock or scheduling ownership.
use crate::{
    catalog::{Catalog, Value},
    game::PlaneState,
    grid::UnitId,
    match_aviation::{AviationError, MissionKind},
    match_runtime::Match,
    math,
};
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

impl Match {
    /// Aircraft fire after moving. Their sortie ammunition counts volleys, not rounds.
    pub fn step_aircraft_weapons(&mut self, id: UnitId) -> Result<(), AviationError> {
        self.validate_aviation()?;
        let u = self
            .world
            .game
            .units
            .get(&id)
            .ok_or(AviationError::Invalid("missing weapon aircraft"))?;
        if u.hp <= 0.0 || !matches!(u.air, Some(PlaneState::Out | PlaneState::Station)) {
            return Ok(());
        }
        let a = self.aviation.planes[&id];
        if a.ammo <= 0.0 {
            return Ok(());
        }
        let def = Catalog::compiled()
            .map_err(|e| AviationError::Actuation(e.to_string()))?
            .unit(&self.rules.kinds[u.kind].name)
            .map_err(|e| AviationError::Actuation(e.to_string()))?;
        let Some(w) = def.get("w").filter(|w| {
            w.get("range")
                .and_then(Value::number)
                .is_some_and(|r| r > 0.0)
        }) else {
            return Ok(());
        };
        let salvo = w.get("salvo").and_then(Value::boolean).unwrap_or(false);
        if let Some(mission) = a.mission.filter(|m| m.kind == MissionKind::Area && salvo) {
            if u.air == Some(PlaneState::Station) && self.firing.get(id).unwrap().cooldown <= 0.0 {
                self.launch_weapon_salvo(id, mission.at, None)
                    .map_err(AviationError::Actuation)?;
                self.firing.get_mut(id).unwrap().cooldown = w
                    .get("interval")
                    .and_then(Value::number)
                    .ok_or(AviationError::Invalid("missing aircraft interval"))?;
                self.aviation.planes.get_mut(&id).unwrap().ammo -= 1.0;
            }
            return Ok(());
        }
        let ordered = a
            .mission
            .filter(|m| m.kind == MissionKind::Attack)
            .and_then(|m| m.target);
        let ordered = match ordered {
            Some(target)
                if self
                    .can_shoot_target(id, target)
                    .map_err(AviationError::Actuation)? =>
            {
                Some(target)
            }
            _ => None,
        };
        if self.commands.units[&id].hold_fire && ordered.is_none() {
            self.firing.get_mut(id).unwrap().target_id = 0;
            return Ok(());
        }
        if ordered.is_none() && self.firing.get(id).unwrap().retarget <= 0.0 {
            let picked = self
                .pick_firing_target(id)
                .map_err(AviationError::Actuation)?;
            self.aviation.planes.get_mut(&id).unwrap().attack_pick = picked;
            self.firing.get_mut(id).unwrap().retarget = 0.5;
        }
        let target = ordered.unwrap_or(self.aviation.planes[&id].attack_pick);
        if target != 0
            && self
                .can_shoot_target(id, target)
                .map_err(AviationError::Actuation)?
        {
            let u = &self.world.game.units[&id];
            let t = &self.world.game.units[&target];
            self.commands.units.get_mut(&id).unwrap().aim = math::atan2(t.z - u.z, t.x - u.x);
            self.firing.get_mut(id).unwrap().target_id = target;
            if self.firing.get(id).unwrap().cooldown <= 0.0 {
                self.fire_weapon(id, target, true)
                    .map_err(AviationError::Actuation)?;
                self.aviation.planes.get_mut(&id).unwrap().ammo -= 1.0;
            }
        } else {
            self.firing.get_mut(id).unwrap().target_id = 0;
        }
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod tests;
