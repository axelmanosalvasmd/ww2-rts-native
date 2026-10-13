//! World command prelude at the sole caller's early live-unit boundary.
//! These helpers do not advance the clock, schedule paths or admit a World tick.

use crate::{
    catalog::{Catalog, Value},
    economy::Mode,
    game::{ModeKind, Point},
    grid::UnitId,
    match_runtime::Match,
    math,
};

impl Match {
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
