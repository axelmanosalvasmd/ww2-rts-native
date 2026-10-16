//! House entry and exit through the canonical unit, order, and grid owners.
use crate::{
    catalog::Catalog,
    game::{CELL, MOVE, Point},
    grid::UnitId,
    match_commands::{Command, Denial, MoveOutcome},
    match_runtime::Match,
    math,
    navigation::TerrainView,
    path_requests::RequestKind,
    unit_spawn::SpawnPlacer,
};
use std::collections::BTreeSet;

fn view(m: &Match) -> TerrainView<'_> {
    let g = &m.world.game;
    TerrainView {
        width: g.w,
        height: g.h,
        flags: &g.flags,
        heights: g.height.as_deref(),
        world_known: false,
        knowledge_id: 0,
        versions: Default::default(),
        changed: None,
        changes: &[],
    }
}
fn center(width: usize, cell: usize) -> Point {
    Point {
        x: (cell % width) as f64 * CELL + CELL * 0.5,
        z: (cell / width) as f64 * CELL + CELL * 0.5,
    }
}
impl Match {
    /// Move setup units trapped by newly placed buildings before the first battle tick.
    pub(crate) fn relocate_trapped_setup_units(&mut self) -> Result<(), String> {
        let ids: Vec<_> = self.world.game.units.keys().copied().collect();
        for id in ids {
            let u = &self.world.game.units[&id];
            let kind = &self.rules.kinds[u.kind];
            if kind.structure || kind.air {
                continue;
            }
            let cell = self
                .world
                .game
                .cell_of(u.x, u.z)
                .ok_or("invalid setup unit pose")?;
            if self.world.game.flags[cell] & MOVE == 0 {
                continue;
            }
            let free = SpawnPlacer::new()
                .nearest_free(view(self), u.at(), kind.block(), None)
                .map_err(|e| format!("setup relocation: {e:?}"))?;
            let at = center(self.world.game.w, free);
            let u = self.world.game.units.get_mut(&id).unwrap();
            crate::unit_placement::place_unit(&mut self.grid, u, |u| {
                u.x = at.x;
                u.z = at.z;
            });
        }
        Ok(())
    }
    /// Leave a house at the closest traversable cell before a move or retreat.
    pub(crate) fn leave_garrison(&mut self, id: UnitId) -> Result<(), String> {
        let Some(unit) = self.world.game.units.get(&id) else {
            return Ok(());
        };
        if unit.garrison.is_none() {
            return Ok(());
        }
        let cell = SpawnPlacer::new()
            .nearest_free(view(self), unit.at(), MOVE, None)
            .map_err(|e| format!("garrison exit: {e:?}"))?;
        let at = center(self.world.game.w, cell);
        let unit = self.world.game.units.get_mut(&id).unwrap();
        crate::unit_placement::place_unit(&mut self.grid, unit, |unit| {
            unit.garrison = None;
            unit.x = at.x;
            unit.z = at.z;
        });
        Ok(())
    }
    fn garrison_entry_cell(
        &self,
        start: usize,
        from: Point,
        taken: &BTreeSet<usize>,
    ) -> Option<usize> {
        let g = &self.world.game;
        let mut queue = vec![start];
        let mut seen = BTreeSet::from([start]);
        let mut candidates = Vec::new();
        let mut head = 0;
        while head < queue.len() {
            let cell = queue[head];
            head += 1;
            let x = cell % g.w;
            let z = cell / g.w;
            let mut open = false;
            for (dx, dz) in [(1isize, 0isize), (-1, 0), (0, 1), (0, -1)] {
                let nx = x as isize + dx;
                let nz = z as isize + dz;
                if nx < 0 || nz < 0 || nx >= g.w as isize || nz >= g.h as isize {
                    continue;
                }
                let next = nz as usize * g.w + nx as usize;
                if g.chars[next] == b'B' {
                    if seen.insert(next) {
                        queue.push(next);
                    }
                } else if g.flags[next] & MOVE == 0 {
                    open = true;
                }
            }
            if open && !taken.contains(&cell) {
                candidates.push(cell);
            }
        }
        candidates.into_iter().min_by(|a, b| {
            let distance = |cell| {
                let p = center(g.w, cell);
                math::hypot(p.x - from.x, p.z - from.z)
            };
            distance(*a).total_cmp(&distance(*b)).then_with(|| a.cmp(b))
        })
    }
    pub fn execute_enter_command(
        &mut self,
        owner: usize,
        ids: &[i64],
        at: Point,
        queue: bool,
    ) -> Result<Option<Denial>, String> {
        if !matches!(
            self.options.mode,
            crate::economy::Mode::Default | crate::economy::Mode::Conquest
        ) || self.world.game.mode.is_some()
        {
            return Ok(Some(Denial::Unsupported));
        }
        let g = &self.world.game;
        let team = g.players.get(owner).ok_or("invalid garrison owner")?.team;
        let Some(start) = g.cell_of(at.x, at.z) else {
            return Ok(Some(Denial::Blocked));
        };
        let seen = crate::vision::team_sees(g, &self.rules, &self.grid, team, center(g.w, start));
        if g.chars[start] != b'B' {
            return Ok(Some(if seen {
                Denial::Blocked
            } else {
                Denial::NotVisible
            }));
        }
        let mut taken: BTreeSet<_> = g.units.values().filter_map(|u| u.garrison).collect();
        taken.extend(self.commands.units.values().filter_map(|c| c.enter));
        let mut entered = false;
        let mut reason = Denial::Needs;
        for &raw in ids.iter().take(50.max(self.world.game.units.len())) {
            let Some(id) = self.eligible_command_unit(raw, owner, &Command::Unsupported) else {
                continue;
            };
            let u = &self.world.game.units[&id];
            let def = Catalog::compiled()
                .map_err(|e| e.to_string())?
                .unit(&self.rules.kinds[u.kind].name)
                .map_err(|e| e.to_string())?;
            if u.retreating {
                reason = Denial::Retreating;
                continue;
            }
            if !def
                .get("garrisons")
                .and_then(|v| v.boolean())
                .unwrap_or(false)
            {
                continue;
            }
            if queue {
                let c = self.commands.units.get_mut(&id).unwrap();
                if c.orders.len() < 8 {
                    c.orders.push_back(Command::Enter {
                        ids: vec![i64::from(id)],
                        at: center(self.world.game.w, start),
                        queue: false,
                    });
                    entered = true;
                } else {
                    reason = Denial::QueueFull;
                }
                continue;
            }
            let Some(cell) = self.garrison_entry_cell(start, u.at(), &taken) else {
                reason = Denial::Maximum;
                break;
            };
            self.leave_garrison(id)?;
            let unit = self.world.game.units[&id].clone();
            let goal = center(self.world.game.w, cell);
            let route = self.command_route(owner, &unit, goal)?;
            taken.insert(cell);
            crate::match_runtime::cancel_owned_work_state(
                &mut self.field_works,
                self.world.game.units.get_mut(&id).unwrap(),
                self.commands.units.get_mut(&id).unwrap(),
            )?;
            let u = self.world.game.units.get_mut(&id).unwrap();
            u.path = route.waypoints;
            u.path_gen = u
                .path_gen
                .checked_add(1)
                .ok_or("garrison path identity exhausted")?;
            u.traffic = None;
            u.traffic_wait = 0.;
            u.world_goal = None;
            u.board = 0;
            u.amove = None;
            u.repath = 0.;
            let c = self.commands.units.get_mut(&id).unwrap();
            c.orders.clear();
            c.face = None;
            c.enter = Some(cell);
            c.attack_id = 0;
            c.nade = None;
            c.fire_at = None;
            c.outcome = Some(MoveOutcome::Interrupted);
            c.outcome_tick = Some(self.world.game.tick);
            entered = true;
        }
        Ok(if entered {
            None
        } else {
            Some(if seen { reason } else { Denial::NotVisible })
        })
    }
    /// At the door, put the squad inside and clear the route. A destroyed door cancels entry.
    pub fn apply_unit_entry_with_routes(&mut self, id: UnitId) -> Result<(), String> {
        let Some(&cell) = self.commands.units.get(&id).and_then(|c| c.enter.as_ref()) else {
            return Ok(());
        };
        let g = &self.world.game;
        if cell >= g.chars.len() || g.chars[cell] != b'B' {
            self.commands.units.get_mut(&id).unwrap().enter = None;
            return Ok(());
        }
        let at = center(g.w, cell);
        let u = g.units.get(&id).ok_or("missing garrison actor")?;
        if math::hypot(u.x - at.x, u.z - at.z) <= CELL * 1.6 {
            let u = self.world.game.units.get_mut(&id).unwrap();
            let path_gen = u
                .path_gen
                .checked_add(1)
                .ok_or("garrison path identity exhausted")?;
            crate::unit_placement::place_unit(&mut self.grid, u, |u| {
                u.garrison = Some(cell);
                u.x = at.x;
                u.z = at.z;
                u.board = 0;
                u.path.clear();
                u.path_gen = path_gen;
            });
            self.commands.units.get_mut(&id).unwrap().enter = None;
        } else if u.path.is_empty() && u.repath <= 0. {
            self.request_command_path(id, at, RequestKind::Enter)?;
        }
        Ok(())
    }
}
