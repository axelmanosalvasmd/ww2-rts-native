//! Terrain fire orders share the recorded queue, aircraft missions and owner-bound path service.
use crate::{
    catalog::{Catalog, Value},
    game::{CELL, Point},
    grid::UnitId,
    match_runtime::Match,
};

fn center(width: usize, cell: usize) -> Point {
    Point {
        x: (cell % width) as f64 * CELL + CELL * 0.5,
        z: (cell / width) as f64 * CELL + CELL * 0.5,
    }
}

impl Match {
    fn terrain_fire_weapon(&self, id: UnitId) -> Result<Option<&'static Value>, String> {
        let u = self
            .world
            .game
            .units
            .get(&id)
            .ok_or("missing terrain fire actor")?;
        let kind = self
            .rules
            .kinds
            .get(u.kind)
            .ok_or("unknown terrain fire kind")?;
        Ok(Catalog::compiled()
            .map_err(|e| e.to_string())?
            .unit(&kind.name)
            .map_err(|e| e.to_string())?
            .get("w"))
    }

    fn terrain_fire_cell(&self, cell: usize) -> Result<Point, String> {
        let g = &self.world.game;
        if g.w == 0
            || g.w.checked_mul(g.h) != Some(g.chars.len())
            || g.flags.len() != g.chars.len()
            || g.cell_hp.len() != g.chars.len()
            || cell >= g.chars.len()
            || !g.cell_hp[cell].is_finite()
        {
            return Err("invalid terrain fire cell".into());
        }
        Ok(center(g.w, cell))
    }

    fn terrain_fire_actor(&self, id: UnitId) -> Result<&'static Value, String> {
        let g = &self.world.game;
        let u = g.units.get(&id).ok_or("missing terrain fire actor")?;
        let kind = self
            .rules
            .kinds
            .get(u.kind)
            .ok_or("unknown terrain fire kind")?;
        if u.id != id
            || id == 0
            || u.owner >= g.players.len()
            || u.air.is_some()
            || kind.air
            || kind.structure
            || kind.rail
            || ![u.hp, u.x, u.z, u.repath, u.rot]
                .iter()
                .all(|v| v.is_finite())
            || g.cell_of(u.x, u.z).is_none()
            || !self.commands.units.contains_key(&id)
            || self.firing.get(id).is_none()
            || self.combat.get(id).is_none()
            || self.abilities.get(id).is_none()
            || u.dig
            || self.field_works.is_active(id)
            || u.entrench
            || self.field_works.membership(id).is_some()
        {
            return Err("invalid canonical terrain fire actor".into());
        }
        let weapon = self
            .terrain_fire_weapon(id)?
            .ok_or("terrain fire actor has no weapon")?;
        let salvo = weapon.get("salvo").and_then(Value::boolean) == Some(true);
        let shell = weapon
            .get("shellTerrain")
            .and_then(Value::number)
            .unwrap_or(0.);
        let range = weapon.get("range").and_then(Value::number).unwrap_or(0.);
        let interval = weapon.get("interval").and_then(Value::number).unwrap_or(0.);
        if !shell.is_finite()
            || !range.is_finite()
            || range <= 0.
            || !interval.is_finite()
            || interval <= 0.
            || (!salvo && shell <= 0.)
            || !self.weapon_range(u, weapon)?.is_finite()
        {
            return Err("invalid terrain fire weapon".into());
        }
        Ok(weapon)
    }

    pub(crate) fn validate_terrain_fire_orders(&self) -> Result<(), String> {
        for &id in self.world.game.units.keys() {
            if let Some(cell) = self.commands.units.get(&id).and_then(|c| c.fire_at) {
                self.terrain_fire_cell(cell)?;
                self.terrain_fire_actor(id)?;
                self.world.game.units[&id]
                    .path_gen
                    .checked_add(3)
                    .ok_or("terrain fire path identity exhausted")?;
                self.can_shell_cell(id, cell)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "match_terrain_orders_tests.rs"]
mod tests;
