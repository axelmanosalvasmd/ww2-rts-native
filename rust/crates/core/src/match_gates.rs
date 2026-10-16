//! Gates reuse canonical building cells and the existing owner snapshot flag.
use crate::{catalog::Catalog, grid::UnitId, match_runtime::Match, math};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GateControl {
    /// Missing until the first complete-building turn, as in Source.
    pub open: Option<bool>,
    pub remaining: f64,
}

impl Match {
    pub(crate) fn ordinary_gate_mode(&self) -> bool {
        matches!(
            self.options.mode,
            crate::economy::Mode::Default | crate::economy::Mode::Conquest
        ) && self.world.game.mode.is_none()
            && !self.options.logistics
            && self.convoys.is_none()
    }

    pub(crate) fn validate_gate_actor(&self, id: UnitId) -> Result<(), String> {
        let unit = self.world.game.units.get(&id).ok_or("missing gate actor")?;
        let kind = self.rules.kinds.get(unit.kind).ok_or("invalid gate kind")?;
        let gate = self
            .commands
            .units
            .get(&id)
            .ok_or("missing gate control")?
            .gate;
        if (kind.name == "gate") != gate.is_some() {
            return Err("gate control does not match its actor".into());
        }
        if let Some(gate) = gate {
            if !self.ordinary_gate_mode() {
                return Err("Gate runtime requires an ordinary Default or Conquest match".into());
            }
            if !gate.remaining.is_finite() || !(0.0..=0.5).contains(&gate.remaining) {
                return Err("invalid gate check timer".into());
            }
            let cells = unit.cells.as_deref().ok_or("missing gate footprint")?;
            if cells.len() != 1 || cells[0] as usize >= self.world.game.chars.len() {
                return Err("invalid gate footprint".into());
            }
            let cell = cells[0] as usize;
            if self.world.game.cell_of(unit.x, unit.z) != Some(cell)
                || !unit.rot.is_finite()
                || !unit.hp.is_finite()
            {
                return Err("invalid gate pose or health".into());
            }
        }
        Ok(())
    }

    pub(crate) fn validate_gate_tick(&self) -> Result<(), String> {
        for &id in self.world.game.units.keys() {
            self.validate_gate_actor(id)?;
        }
        Ok(())
    }

    /// Source checks at most twice a second, before a building's early continue.
    pub(crate) fn step_gate(&mut self, id: UnitId, dt: f64) -> Result<(), String> {
        self.validate_gate_actor(id)?;
        if !dt.is_finite() || dt < 0.0 {
            return Err("invalid gate tick duration".into());
        }
        let Some(mut gate) = self.commands.units[&id].gate else {
            return Ok(());
        };
        let unit = &self.world.game.units[&id];
        if unit.hp <= 0.0 || unit.built.is_none_or(|built| built < 1.0) {
            return Ok(());
        }
        gate.remaining -= dt;
        if gate.remaining > 0.0 {
            self.commands.units.get_mut(&id).unwrap().gate = Some(gate);
            return Ok(());
        }
        gate.remaining = 0.5;
        let def = Catalog::compiled()
            .map_err(|e| e.to_string())?
            .unit("gate")
            .map_err(|e| e.to_string())?;
        let shut = def
            .get("gate")
            .and_then(|g| g.get("shut"))
            .and_then(|n| n.number())
            .filter(|n| n.is_finite() && *n >= 0.0)
            .ok_or("invalid gate shut radius")?;
        let radius = if gate.open == Some(false) {
            shut + 4.0
        } else {
            shut
        };
        let eligible = |other: &crate::game::Unit| {
            other.hp > 0.0
                && other.air.is_none()
                && !other.riding
                && !self.rules.kinds[other.kind].building
        };
        // Source threat range is a grid-bucket query, not an exact radial test.
        let foe = self.grid.has(
            &self.world.game.units,
            (unit.x, unit.z),
            radius,
            |_, other| eligible(other) && !self.world.game.allied(other.owner, unit.owner),
        );
        let passage = gate.open != Some(false)
            && self
                .grid
                .has(&self.world.game.units, (unit.x, unit.z), 3.0, |_, other| {
                    let distance = math::hypot(other.x - unit.x, other.z - unit.z);
                    eligible(other)
                        && distance < crate::game::CELL * 0.7 + self.rules.kinds[other.kind].radius
                });
        let open = !foe || passage;
        if gate.open != Some(open) {
            let cell = unit.cells.as_ref().unwrap()[0] as usize;
            // Publish terrain first: a failed journal write cannot publish an openness change.
            self.world
                .set_cell(cell, if open { b'.' } else { b'K' })
                .map_err(|e| e.to_string())?;
            gate.open = Some(open);
        }
        self.commands.units.get_mut(&id).unwrap().gate = Some(gate);
        Ok(())
    }
}

#[cfg(test)]
#[path = "match_gate_tests.rs"]
mod tests;
