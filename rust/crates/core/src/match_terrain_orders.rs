//! Terrain fire orders share the recorded queue, aircraft missions and owner-bound path service.
use crate::{
    catalog::{Catalog, Value},
    game::{CELL, Point, SIGHT, WOOD},
    grid::UnitId,
    match_aircraft_commands::AircraftOrder,
    match_commands::{Command, Denial, MoveOutcome},
    match_runtime::Match,
    math,
    path_requests::RequestKind,
};
use std::collections::BTreeMap;

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

    pub(crate) fn terrain_fire_command(
        &mut self,
        owner: usize,
        ids: &[i64],
        at: Point,
        queue: bool,
    ) -> Result<Option<Denial>, String> {
        if !crate::match_actor::ordinary_ground_mode(&self.world.game, self.options.mode) {
            return Ok(Some(Denial::Unsupported));
        }
        if owner >= self.world.game.players.len() || !at.x.is_finite() || !at.z.is_finite() {
            return Ok(Some(Denial::Blocked));
        }
        let Some(cell) = self.world.game.cell_of(at.x, at.z) else {
            return Ok(Some(Denial::Blocked));
        };
        let at = self.terrain_fire_cell(cell)?;
        let solid = self.world.game.cell_hp[cell] > 0.;
        let command = Command::FireAt {
            ids: ids.to_vec(),
            at,
            queue,
        };
        let selected: Vec<_> = ids
            .iter()
            .take(50.max(self.world.game.units.len()))
            .filter_map(|id| self.eligible_command_unit(*id, owner, &command))
            .collect();
        let mut ground = Vec::new();
        let mut aircraft = Vec::new();
        for &id in &selected {
            let Some(w) = self.terrain_fire_weapon(id)? else {
                continue;
            };
            let capable = w.get("salvo").and_then(Value::boolean) == Some(true)
                || solid
                    && w.get("shellTerrain")
                        .and_then(Value::number)
                        .is_some_and(|n| n > 0.);
            if !capable {
                continue;
            }
            if self.is_aircraft_command_unit(id) {
                aircraft.push(
                    self.prepare_aircraft_order(owner, id, AircraftOrder::Area(at))
                        .map_err(|e| e.to_string())?,
                );
            } else {
                self.terrain_fire_actor(id)?;
                ground.push(id);
            }
        }
        if !queue {
            let mut counts = BTreeMap::<UnitId, u64>::new();
            for &id in &ground {
                *counts.entry(id).or_default() += 1;
            }
            for (id, count) in counts {
                self.world.game.units[&id]
                    .path_gen
                    .checked_add(count)
                    .ok_or("terrain fire path identity exhausted")?;
            }
        }
        // mine() releases follow control even when the selected weapon cannot shell this cell.
        for &id in &selected {
            self.commands.units.get_mut(&id).unwrap().follow = 0;
        }
        if ground.is_empty() && aircraft.is_empty() {
            let seen = crate::vision::team_sees(
                &self.world.game,
                &self.rules,
                &self.grid,
                self.world.game.players[owner].team,
                at,
            );
            return Ok(Some(if !seen {
                Denial::NotVisible
            } else if solid {
                Denial::Needs
            } else {
                Denial::Blocked
            }));
        }
        let mut ordered = !aircraft.is_empty();
        for order in aircraft {
            self.apply_aircraft_order(order);
        }
        for id in ground {
            let c = self.commands.units.get_mut(&id).unwrap();
            if queue {
                if c.orders.len() < 8 {
                    c.orders.push_back(Command::FireAt {
                        ids: vec![i64::from(id)],
                        at,
                        queue: false,
                    });
                    ordered = true;
                }
                continue;
            }
            let u = self.world.game.units.get_mut(&id).unwrap();
            u.traffic = None;
            u.traffic_wait = 0.;
            u.world_goal = None;
            u.entrench = false;
            u.attack = false;
            u.amove = None;
            u.retreating = false;
            u.repath = 0.;
            u.path.clear();
            u.path_gen += 1;
            c.orders.clear();
            c.face = None;
            c.fire_at = Some(cell);
            c.attack_id = 0;
            c.outcome = Some(MoveOutcome::Interrupted);
            c.outcome_tick = Some(self.world.game.tick);
            ordered = true;
        }
        Ok((!ordered).then_some(Denial::QueueFull))
    }

    /// Direct fire may reach a cell through the first wall of the same structure.
    pub(crate) fn can_shell_cell(&self, id: UnitId, cell: usize) -> Result<bool, String> {
        let at = self.terrain_fire_cell(cell)?;
        let u = self
            .world
            .game
            .units
            .get(&id)
            .ok_or("missing terrain fire actor")?;
        if ![u.x, u.z].iter().all(|v| v.is_finite()) || self.world.game.cell_of(u.x, u.z).is_none()
        {
            return Err("invalid terrain fire pose".into());
        }
        if crate::vision::los(&self.world.game, &self.rules, (u.x, u.z), (at.x, at.z)) {
            return Ok(true);
        }
        let Some(section) = self.world.sections.get(&cell) else {
            return Ok(false);
        };
        let Some(blocker) = self.terrain_fire_blocker(u.at(), at) else {
            return Ok(false);
        };
        let same = self
            .world
            .sections
            .get(&blocker)
            .is_some_and(|b| b.structure_id == section.structure_id);
        let wall = center(self.world.game.w, blocker);
        Ok(same && crate::vision::los(&self.world.game, &self.rules, (u.x, u.z), (wall.x, wall.z)))
    }

    /// The prototype blocker walk excludes both endpoints and preserves tied grid crossings.
    fn terrain_fire_blocker(&self, a: Point, b: Point) -> Option<usize> {
        let g = &self.world.game;
        let (mut x, mut z) = ((a.x / CELL).floor() as i64, (a.z / CELL).floor() as i64);
        let (ex, ez) = ((b.x / CELL).floor() as i64, (b.z / CELL).floor() as i64);
        let (dx, dz) = (b.x - a.x, b.z - a.z);
        let (sx, sz) = (crate::vision::step_of(dx), crate::vision::step_of(dz));
        let tdx = if dx != 0. {
            CELL / dx.abs()
        } else {
            f64::INFINITY
        };
        let tdz = if dz != 0. {
            CELL / dz.abs()
        } else {
            f64::INFINITY
        };
        let mut tx = if dx != 0. {
            (if sx > 0 {
                (x + 1) as f64 * CELL - a.x
            } else {
                a.x - x as f64 * CELL
            }) / dx.abs()
        } else {
            f64::INFINITY
        };
        let mut tz = if dz != 0. {
            (if sz > 0 {
                (z + 1) as f64 * CELL - a.z
            } else {
                a.z - z as f64 * CELL
            }) / dz.abs()
        } else {
            f64::INFINITY
        };
        let mut wood = 0.;
        for _ in 0..g.w + g.h {
            if x == ex && z == ez {
                return None;
            }
            if z == ez || (x != ex && tx < tz) {
                tx += tdx;
                x += sx;
            } else {
                tz += tdz;
                z += sz;
            }
            if x == ex && z == ez {
                return None;
            }
            if x < 0 || z < 0 || x >= g.w as i64 || z >= g.h as i64 {
                return None;
            }
            let cell = z as usize * g.w + x as usize;
            let flags = g.flags[cell];
            if flags & SIGHT != 0 {
                return Some(cell);
            }
            if flags & WOOD != 0 {
                wood += 1.;
                if wood > self.rules.vision.wood_sight {
                    return Some(cell);
                }
            }
        }
        None
    }

    pub(crate) fn current_terrain_fire_goal(&self, id: UnitId) -> Result<Option<Point>, String> {
        let Some(u) = self.world.game.units.get(&id) else {
            return Ok(None);
        };
        let Some(cell) = self.commands.units.get(&id).and_then(|c| c.fire_at) else {
            return Ok(None);
        };
        let at = self.terrain_fire_cell(cell)?;
        let w = self.terrain_fire_actor(id)?;
        let salvo = w.get("salvo").and_then(Value::boolean) == Some(true);
        if u.hp <= 0.
            || u.retreating
            || !u.path.is_empty()
            || (!salvo && self.world.game.cell_hp[cell] <= 0.)
        {
            return Ok(None);
        }
        // currentPathGoal uses the base weapon range when servicing deferred requests.
        let range = w
            .get("range")
            .and_then(Value::number)
            .ok_or("missing terrain fire range")?;
        Ok((math::hypot(at.x - u.x, at.z - u.z) > range
            || (!salvo && !self.can_shell_cell(id, cell)?))
        .then_some(at))
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

    pub(crate) fn apply_terrain_fire_order(&mut self, id: UnitId) -> Result<(), String> {
        let Some(cell) = self.commands.units.get(&id).and_then(|c| c.fire_at) else {
            return Ok(());
        };
        let at = self.terrain_fire_cell(cell)?;
        let weapon = self.terrain_fire_actor(id)?;
        let salvo = weapon.get("salvo").and_then(Value::boolean) == Some(true);
        if !salvo && self.world.game.cell_hp[cell] <= 0. {
            self.commands.units.get_mut(&id).unwrap().fire_at = None;
            return Ok(());
        }
        let u = &self.world.game.units[&id];
        if math::hypot(at.x - u.x, at.z - u.z) <= self.weapon_range(u, weapon)?
            && (salvo || self.can_shell_cell(id, cell)?)
        {
            u.path_gen
                .checked_add(1)
                .ok_or("terrain fire path identity exhausted")?;
            let u = self.world.game.units.get_mut(&id).unwrap();
            u.path.clear();
            u.path_gen += 1;
        } else if u.path.is_empty() && u.repath <= 0. {
            self.request_command_path(id, at, RequestKind::FireAt)?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "match_terrain_orders_tests.rs"]
mod tests;
