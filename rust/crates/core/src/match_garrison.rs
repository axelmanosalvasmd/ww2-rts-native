//! House entry through ordinary command admission and the canonical path scheduler.
use crate::{
    catalog::Catalog,
    game::{CELL, MOVE, Point},
    grid::UnitId,
    match_commands::{Command, Denial, MoveOutcome},
    match_runtime::Match,
    math,
    navigation::{TerrainView, Versions},
    path_requests::RequestKind,
    unit_spawn::SpawnPlacer,
};
use std::collections::{BTreeSet, VecDeque};

fn center(width: usize, cell: usize) -> Point {
    Point {
        x: (cell % width) as f64 * CELL + CELL / 2.0,
        z: (cell / width) as f64 * CELL + CELL / 2.0,
    }
}

impl Match {
    fn validate_garrison_unit(&self, id: UnitId) -> Result<(), String> {
        let unit = self
            .world
            .game
            .units
            .get(&id)
            .ok_or("missing garrison unit")?;
        let control = self
            .commands
            .units
            .get(&id)
            .ok_or("missing garrison control")?;
        if unit.id != id
            || unit.kind >= self.rules.kinds.len()
            || ![unit.x, unit.z, unit.hp, unit.repath]
                .iter()
                .all(|v| v.is_finite())
            || control
                .enter
                .is_some_and(|c| c >= self.world.game.chars.len())
            || control.enter.is_some() && unit.path_gen == u64::MAX
            || unit
                .garrison
                .is_some_and(|c| c >= self.world.game.chars.len())
            || control
                .garrison_entry
                .is_some_and(|at| !at.x.is_finite() || !at.z.is_finite())
            || unit.dig != self.field_works.is_active(id)
            || unit.entrench != self.field_works.membership(id).is_some()
            || self.firing.get(id).is_none()
            || self.combat.get(id).is_none()
            || self.abilities.get(id).is_none()
        {
            return Err("invalid canonical garrison unit".into());
        }
        crate::match_construction::validate_build_activity(unit, control)
    }

    pub(crate) fn validate_garrison_tick(&self) -> Result<(), String> {
        for (&id, control) in &self.commands.units {
            if control.enter.is_some() {
                self.validate_garrison_unit(id)?;
            }
        }
        Ok(())
    }

    fn house_entry(&self, start: usize, from: Point, taken: &BTreeSet<usize>) -> Option<usize> {
        let g = &self.world.game;
        let mut seen = BTreeSet::from([start]);
        let mut queue = VecDeque::from([start]);
        let mut edges = Vec::new();
        while let Some(cell) = queue.pop_front() {
            let x = cell % g.w;
            let z = cell / g.w;
            let mut open = false;
            for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let nx = x as isize + dx;
                let nz = z as isize + dz;
                if nx < 0 || nz < 0 || nx >= g.w as isize || nz >= g.h as isize {
                    continue;
                }
                let next = nz as usize * g.w + nx as usize;
                if g.chars[next] == b'B' {
                    if seen.insert(next) {
                        queue.push_back(next);
                    }
                } else if g.flags[next] & MOVE == 0 {
                    open = true;
                }
            }
            if open && !taken.contains(&cell) {
                edges.push(cell);
            }
        }
        // Stable sorting preserves the source's neighbour traversal order on ties.
        edges.sort_by(|a, b| {
            let a = center(g.w, *a);
            let b = center(g.w, *b);
            math::hypot(a.x - from.x, a.z - from.z)
                .total_cmp(&math::hypot(b.x - from.x, b.z - from.z))
        });
        edges.first().copied()
    }

    pub(crate) fn execute_garrison(
        &mut self,
        owner: usize,
        ids: &[i64],
        at: Option<Point>,
        queued: bool,
    ) -> Result<Option<Denial>, String> {
        if self.uses_world_routes() {
            return Ok(Some(Denial::Unsupported));
        }
        let Some(cell) = at.and_then(|at| self.world.game.cell_of(at.x, at.z)) else {
            return Ok(Some(Denial::Blocked));
        };
        let point = center(self.world.game.w, cell);
        if self.world.game.chars[cell] != b'B' {
            return Ok(Some(if self.recipient_sees(owner, point)? {
                Denial::Blocked
            } else {
                Denial::NotVisible
            }));
        }
        let command = Command::Garrison {
            ids: ids.to_vec(),
            at,
            queue: queued,
        };
        let selected: Vec<_> = ids
            .iter()
            .take(50.max(self.world.game.units.len()))
            .filter_map(|id| self.eligible_command_unit(*id, owner, &command))
            .collect();
        for &id in &selected {
            self.validate_garrison_unit(id)?;
            if self.world.game.units[&id].path_gen.checked_add(1).is_none() {
                return Err("garrison path identity exhausted".into());
            }
        }
        let mut taken: BTreeSet<_> = self
            .world
            .game
            .units
            .values()
            .filter_map(|u| u.garrison)
            .chain(self.commands.units.values().filter_map(|c| c.enter))
            .collect();
        let mut accepted = false;
        let mut reason = Denial::Needs;
        for id in selected {
            self.commands.units.get_mut(&id).unwrap().follow = 0;
            let unit = &self.world.game.units[&id];
            let allowed = Catalog::compiled()
                .map_err(|e| e.to_string())?
                .unit(&self.rules.kinds[unit.kind].name)
                .map_err(|e| e.to_string())?
                .get("garrisons")
                .and_then(|v| v.boolean())
                .unwrap_or(false);
            if !allowed || unit.retreating {
                reason = if unit.retreating {
                    Denial::Retreating
                } else {
                    Denial::Needs
                };
                continue;
            }
            if queued {
                let orders = &mut self.commands.units.get_mut(&id).unwrap().orders;
                if orders.len() >= 8 {
                    reason = Denial::QueueFull;
                } else {
                    orders.push_back(Command::Garrison {
                        ids: vec![i64::from(id)],
                        at: Some(point),
                        queue: false,
                    });
                    accepted = true;
                }
                continue;
            }
            let Some(entry) = self.house_entry(cell, unit.at(), &taken) else {
                reason = Denial::Maximum;
                break;
            };
            let mut unit = unit.clone();
            if unit.garrison.is_some() {
                let g = &self.world.game;
                let free = SpawnPlacer::new()
                    .nearest_free(
                        TerrainView {
                            width: g.w,
                            height: g.h,
                            flags: &g.flags,
                            heights: g.height.as_deref(),
                            world_known: false,
                            knowledge_id: 0,
                            versions: Versions::default(),
                            changed: None,
                            changes: &[],
                        },
                        unit.at(),
                        MOVE,
                        None,
                    )
                    .map_err(|e| format!("garrison exit: {e:?}"))?;
                let at = center(g.w, free);
                unit.x = at.x;
                unit.z = at.z;
                unit.garrison = None;
            }
            let route = self.command_route(owner, &unit, center(self.world.game.w, entry))?;
            self.cancel_field_work(id)?;
            unit.traffic = None;
            unit.traffic_wait = 0.0;
            unit.world_goal = None;
            unit.entrench = false;
            unit.attack = false;
            unit.dig = false;
            unit.build = false;
            unit.board = 0;
            unit.amove = None;
            unit.drift = false;
            unit.reverse = None;
            unit.repath = 0.0;
            unit.path = route.waypoints;
            unit.path_gen += 1;
            let control = self.commands.units.get_mut(&id).unwrap();
            control.outcome = Some(MoveOutcome::Interrupted);
            control.outcome_tick = Some(self.world.game.tick);
            control.orders.clear();
            control.face = None;
            control.enter = Some(entry);
            control.attack_id = 0;
            control.nade = None;
            control.fire_at = None;
            control.build_target = None;
            control.rally_drift = false;
            control.route_end = Some(route.route_end);
            let ability = self.abilities.get_mut(id).unwrap();
            ability.aimed_auto = false;
            ability.aimed_walked = false;
            self.firing.initialize(id).target_id = 0;
            self.commands.requests.retire_unit(id);
            self.grid.update(id, &unit);
            self.world.game.units.insert(id, unit);
            taken.insert(entry);
            accepted = true;
        }
        Ok(if accepted {
            None
        } else if self.recipient_sees(owner, point)? {
            Some(reason)
        } else {
            Some(Denial::NotVisible)
        })
    }

    pub(crate) fn current_enter_goal(&self, id: UnitId) -> Result<Option<Point>, String> {
        let Some(unit) = self.world.game.units.get(&id) else {
            return Ok(None);
        };
        self.validate_garrison_unit(id)?;
        let Some(cell) = self.commands.units[&id].enter else {
            return Ok(None);
        };
        let at = center(self.world.game.w, cell);
        Ok((unit.hp > 0.0
            && self.world.game.chars[cell] == b'B'
            && unit.path.is_empty()
            && math::hypot(unit.x - at.x, unit.z - at.z) > CELL * 1.6)
            .then_some(at))
    }

    pub(crate) fn apply_unit_enter(&mut self, id: UnitId) -> Result<(), String> {
        let Some(cell) = self.commands.units.get(&id).and_then(|c| c.enter) else {
            return Ok(());
        };
        self.validate_garrison_unit(id)?;
        if self.world.game.chars[cell] != b'B' {
            self.commands.units.get_mut(&id).unwrap().enter = None;
            return Ok(());
        }
        let at = center(self.world.game.w, cell);
        let unit = &self.world.game.units[&id];
        if math::hypot(unit.x - at.x, unit.z - at.z) <= CELL * 1.6 {
            let entry = unit.at();
            let unit = self.world.game.units.get_mut(&id).unwrap();
            unit.x = at.x;
            unit.z = at.z;
            unit.garrison = Some(cell);
            unit.board = 0;
            unit.path.clear();
            unit.path_gen = unit
                .path_gen
                .checked_add(1)
                .ok_or("garrison path identity exhausted")?;
            let control = self.commands.units.get_mut(&id).unwrap();
            control.enter = None;
            control.garrison_entry = Some(entry);
            self.grid.update(id, unit);
            self.commands.requests.retire_unit(id);
        } else if unit.path.is_empty() && unit.repath <= 0.0 {
            self.request_command_path(id, at, RequestKind::Enter)?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "match_garrison_tests.rs"]
mod tests;
