//! World command prelude at the sole caller's early live-unit boundary.
//! These helpers do not advance the clock, schedule paths or admit a World tick.

use crate::{
    catalog::{Catalog, Value},
    economy::Mode,
    game::{ModeKind, Point},
    grid::UnitId,
    match_abilities::AbilityKind,
    match_commands::UnitControl,
    match_runtime::Match,
    math,
    rules::Rules,
};

/// Future births must be supported in their canonical newborn stance, not an
/// authored held-fire stance that only an existing actor can already have.
pub(crate) fn validate_world_future_birth(rules: &Rules, name: &str) -> Result<(), String> {
    let index = rules.kind(name).ok_or("invalid World queued birth kind")?;
    let kind = &rules.kinds[index];
    if kind.air || kind.rail || kind.naval {
        return Err("World queued birth requires an unrepresented actor phase".into());
    }
    let control = UnitControl::new_for_kind(kind, 0.0, Mode::World)?;
    let definition = Catalog::compiled()
        .map_err(|e| e.to_string())?
        .unit(name)
        .map_err(|e| e.to_string())?;
    validate_world_salvo_control(definition, &control)?;
    validate_world_autocast_control(definition, &control)
}

fn validate_world_salvo_control(definition: &Value, control: &UnitControl) -> Result<(), String> {
    if !control.hold_fire
        && definition
            .get("w")
            .and_then(|weapon| weapon.get("salvo"))
            .is_some_and(|salvo| !matches!(salvo, Value::Null | Value::Bool(false)))
    {
        return Err("World automatic salvo tick actuator is not connected".into());
    }
    Ok(())
}

fn validate_world_autocast_control(
    definition: &Value,
    control: &UnitControl,
) -> Result<(), String> {
    if control.auto_cast
        && matches!(
            AbilityKind::from_definition(definition)?,
            AbilityKind::Grenade | AbilityKind::Satchel | AbilityKind::Smoke | AbilityKind::Barrage
        )
    {
        return Err("World aimed autocast routing is not connected".into());
    }
    Ok(())
}

impl Match {
    /// Admit only the World ground actuators represented by this caller composition.
    /// No ordinary-mode component error is treated as an inert World phase.
    pub(crate) fn validate_world_tick_adapters(&self, boundary: u64) -> Result<(), String> {
        self.validate_world_phase_context(self.rules.movement.tick)?;
        self.validate_world_path_service()?;
        self.validate_firing_tick_state()?;
        self.validate_firing_grenade_step(self.rules.movement.tick)?;
        if !self.firing.salvos.is_empty() {
            return Err("World physical salvo tick actuator is not connected".into());
        }
        self.validate_world_support_timers(self.rules.movement.tick)
            .map_err(|error| format!("World support timers: {error}"))?;
        self.validate_building_activity_rows()?;
        self.validate_match_finish_state()?;
        if self.scenario.is_some()
            || !self.start.points.is_empty()
            || !self.companions.records.is_empty()
            || !self.companions.soldiers.is_empty()
            || !self.aviation.planes.is_empty()
            || !self.aviation.anti_air.is_empty()
            || !self.field_works.jobs().is_empty()
            || self.field_works.projects().next().is_some()
        {
            return Err(
                "World tick has unrepresented scenario, operative, aircraft or work state".into(),
            );
        }
        let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
        for (&id, unit) in &self.world.game.units {
            self.validate_world_command_prelude(id)?;
            if self.validate_rail_tick_actor(id)? {
                continue;
            }
            if unit.hp > 0.0 {
                self.validate_firing_weapon_metadata(unit)?;
            }
            let control = &self.commands.units[&id];
            let metadata = self
                .combat
                .get(id)
                .ok_or("missing World tick combat metadata")?;
            if !metadata.cargo.is_empty() || metadata.carrier.is_some() {
                return Err("World carrier tick phases are not connected".into());
            }
            let kind = &self.rules.kinds[unit.kind];
            if kind.naval {
                return Err("World naval tick phase is not connected".into());
            }
            let definition = catalog.unit(&kind.name).map_err(|e| e.to_string())?;
            if kind.building
                && unit.hp > 0.0
                && (unit.built.is_none_or(|built| built < 1.0)
                    || definition
                        .get("gate")
                        .is_some_and(|gate| !matches!(gate, Value::Null | Value::Bool(false))))
            {
                return Err("World construction or gate tick phase is not connected".into());
            }
            validate_world_salvo_control(definition, control)?;
            if !control.react.is_finite()
                || !(control.react - self.rules.movement.tick).is_finite()
                || !metadata.hit_at.is_finite()
                || unit
                    .hit_from
                    .is_some_and(|at| !at.x.is_finite() || !at.z.is_finite())
            {
                return Err("invalid World reaction tick facts".into());
            }
            validate_world_autocast_control(definition, control)?;
            for waiting in &control.orders {
                self.validate_world_tick_command(waiting)?;
            }
        }
        for (_, command) in self.commands.queue.pending_tick(boundary) {
            self.validate_world_tick_command(command)?;
        }
        // The generic producer admits mode-consistent jobs; this caller connects
        // only ground births and must refuse unsupported future actor kinds now.
        for (_, building) in self.production.buildings() {
            for job in building.jobs() {
                validate_world_future_birth(&self.rules, &job.kind)?;
            }
        }
        self.validate_world_tick_region_consequences()?;
        let ids = self.world.game.units.keys().copied().collect::<Vec<_>>();
        self.validate_dead_retirement(&ids, boundary)
            .map_err(|error| format!("World retirement admission: {error:?}"))?;
        self.prepare_recovery(&ids)?;
        Ok(())
    }

    /// A region can advance by at most one capture increment in this tick.
    /// Refuse an unrepresented fort transfer before commands, even if the fort dies later.
    fn validate_world_tick_region_consequences(&self) -> Result<(), String> {
        let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
        let span = catalog
            .table("CFG")
            .map_err(|e| e.to_string())?
            .get("captureTime")
            .and_then(Value::number)
            .filter(|time| time.is_finite() && *time > 0.0)
            .ok_or("invalid World capture duration")?;
        let rate = self.rules.movement.tick / span;
        if !rate.is_finite() || !(0.0..1.0).contains(&rate) {
            return Err("World capture interval exceeds the bounded tick admission".into());
        }
        let runtime = self
            .world_runtime
            .as_ref()
            .ok_or("missing World tick runtime")?;
        for (index, region) in runtime.regions.iter().enumerate() {
            if !region.progress.is_finite()
                || !(0.0..=1.0).contains(&region.progress)
                || !(-1..6).contains(&region.team)
                || region
                    .capper
                    .is_some_and(|slot| slot >= self.world.game.players.len())
            {
                return Err("invalid World tick region progress".into());
            }
            // Owned regions only neutralize this step. A new or changed capper resets
            // progress to zero, so only this retained capper can complete at the limit.
            let Some(owner) = region
                .capper
                .filter(|_| region.team < 0 && region.progress + rate >= 1.0)
            else {
                continue;
            };
            for unit in self.world.game.units.values() {
                if unit.hp > 0.0
                    && ["pillbox", "tower", "wall", "gate"]
                        .contains(&self.rules.kinds[unit.kind].name.as_str())
                    && !self.world.game.allied(unit.owner, owner)
                    && self.world_region_index(unit.at())? == Some(index)
                {
                    return Err("World fort capture tick actuator is not connected".into());
                }
            }
        }
        Ok(())
    }

    fn validate_world_tick_command(
        &self,
        command: &crate::match_commands::Command,
    ) -> Result<(), String> {
        use crate::match_commands::Command;
        match command {
            Command::Move { .. }
            | Command::Stop { .. }
            | Command::Stance { .. }
            | Command::Destroy { .. }
            | Command::Rally { .. }
            | Command::Technology { .. }
            | Command::Cancel { .. }
            | Command::CancelProduction { .. } => Ok(()),
            Command::Autocast { ids, on }
                if !*on
                    || ids.iter().all(|raw| {
                        u32::try_from(*raw)
                            .ok()
                            .filter(|id| self.world.game.units.contains_key(id))
                            .is_none_or(|id| {
                                self.ability_kind(id).is_ok_and(|kind| {
                                    !matches!(
                                        kind,
                                        crate::match_abilities::AbilityKind::Grenade
                                            | crate::match_abilities::AbilityKind::Satchel
                                            | crate::match_abilities::AbilityKind::Smoke
                                            | crate::match_abilities::AbilityKind::Barrage
                                    )
                                })
                            })
                    }) =>
            {
                Ok(())
            }
            Command::Ability {
                at: None,
                queue: false,
                ..
            } => Ok(()),
            Command::Buy { kind, .. } => validate_world_future_birth(&self.rules, kind),
            _ => Err("World command requires an unrepresented tick actuator".into()),
        }
    }

    /// Final upkeep facts retain the captured order even after actual ordinary removal.
    /// An incomplete removal/callback prefix cannot advance the World economy.
    pub fn world_tick_income_facts(
        &self,
        captured: &[UnitId],
        retirement: &crate::match_combat::RetirementProgress,
    ) -> Result<Vec<crate::match_world_phases::WorldIncomeUnit>, String> {
        self.validate_world_phase_context(self.rules.movement.tick)?;
        let count = retirement.removed.len();
        if [
            retirement.completed,
            retirement.companions_notified,
            retirement.stories_notified,
            retirement.routes_retained,
        ]
        .iter()
        .any(|done| *done != count)
        {
            return Err("World income requires completed ordinary retirement callbacks".into());
        }
        let mut seen = std::collections::BTreeSet::new();
        let mut removed = std::collections::BTreeMap::new();
        for record in &retirement.removed {
            let unit = &record.unit;
            if unit.id == 0
                || removed.insert(unit.id, unit).is_some()
                || self.world.game.units.contains_key(&unit.id)
                || !captured.contains(&unit.id)
            {
                return Err("conflicting World retired income identity".into());
            }
        }
        captured
            .iter()
            .map(|id| {
                if *id == 0 || !seen.insert(*id) {
                    return Err("duplicate World captured income identity".into());
                }
                let unit = self
                    .world
                    .game
                    .units
                    .get(id)
                    .or_else(|| removed.get(id).copied())
                    .ok_or("missing final World captured object")?;
                if unit.id != *id || unit.kind >= self.rules.kinds.len() || !unit.hp.is_finite() {
                    return Err("invalid final World captured object".into());
                }
                self.world_actor_view(unit.owner)?;
                Ok(crate::match_world_phases::WorldIncomeUnit::from(unit))
            })
            .collect()
    }

    /// Validate the actual guard pose and completion facts without mutating grid or commands.
    pub fn validate_world_command_prelude(&self, id: UnitId) -> Result<(), String> {
        if self.options.mode != Mode::World || self.world.game.mode != Some(ModeKind::World) {
            return Err("World command prelude requires canonical World mode".into());
        }
        let unit = self
            .world
            .game
            .units
            .get(&id)
            .ok_or("missing World command actor")?;
        let control = self
            .commands
            .units
            .get(&id)
            .ok_or("missing World command control")?;
        if unit.id != id
            || unit.kind >= self.rules.kinds.len()
            || ![unit.hp, unit.x, unit.z].iter().all(|v| v.is_finite())
        {
            return Err("invalid World command actor".into());
        }
        self.world_actor_view(unit.owner)?;
        // This also validates the geography/runtime binding without warming actor memory.
        self.world_region_index(unit.at())?;
        if control
            .guard_home
            .into_iter()
            .chain(unit.world_goal)
            .any(|at| !at.x.is_finite() || !at.z.is_finite())
        {
            return Err("invalid World command goal".into());
        }
        Ok(())
    }

    /// Source snaps a displaced guard before command completion and leaves the grid alone.
    pub fn apply_world_guard_home(&mut self, id: UnitId) -> Result<(), String> {
        self.validate_world_command_prelude(id)?;
        let unit = &self.world.game.units[&id];
        if unit.hp <= 0.0 || self.commands.units[&id].operative {
            return Ok(());
        }
        let Some(home) = self.commands.units[&id].guard_home else {
            return Ok(());
        };
        if math::hypot(unit.x - home.x, unit.z - home.z) > 16.0 {
            let unit = self
                .world
                .game
                .units
                .get_mut(&id)
                .expect("validated World actor");
            unit.x = home.x;
            unit.z = home.z;
            unit.path.clear();
            unit.attack = false;
            self.commands
                .units
                .get_mut(&id)
                .expect("validated World control")
                .attack_id = 0;
        }
        Ok(())
    }

    /// World voluntary retreat selects its own home-radius condition; forced and ordinary
    /// completion keep the existing endpoint/path/speed condition in the command owner.
    pub(crate) fn world_retreat_completion(
        &self,
        id: UnitId,
        goal: Point,
    ) -> Result<Option<bool>, String> {
        let unit = self
            .world
            .game
            .units
            .get(&id)
            .ok_or("missing World completion actor")?;
        if self.world.game.mode != Some(ModeKind::World)
            || !unit.retreating
            || self.logistics.get(id).is_some_and(|row| row.forced)
        {
            return Ok(None);
        }
        self.validate_world_command_prelude(id)?;
        let radius = Catalog::compiled()
            .map_err(|e| e.to_string())?
            .table("CFG")
            .map_err(|e| e.to_string())?
            .get("reinforceRadius")
            .and_then(Value::number)
            .filter(|v| v.is_finite() && *v >= 0.0)
            .ok_or("invalid World reinforce radius")?;
        Ok(Some(
            math::hypot(unit.x - goal.x, unit.z - goal.z) <= radius,
        ))
    }
}
