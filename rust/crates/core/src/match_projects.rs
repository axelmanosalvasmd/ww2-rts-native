//! Recorded shared field projects over the sole field-work, command and wallet owners.
use super::{
    Error, FortKind, PreparedProject, ProjectMaterial, ProjectOrder, ProjectRequest, Reason,
    SegmentPreparation, definitions, line_fort,
};
use crate::{
    game::{CELL, MOVE, Point, Unit},
    grid::UnitId,
    match_commands::{Command, Denial, MoveOutcome, UnitControl},
    match_runtime::Match,
    match_story::{StoryChange, StoryKey, prepare_story_updates},
    math,
    navigation::{TerrainView, Versions},
    unit_spawn::SpawnPlacer,
};
use std::collections::{BTreeMap, BTreeSet};

struct JoinedUnit {
    unit: Unit,
    control: UnitControl,
    firing: crate::match_firing::FiringUnit,
    ability: crate::match_abilities::AbilityUnit,
}

#[cfg(test)]
#[path = "match_projects_tests.rs"]
mod tests;
fn refusal(reason: Reason) -> Denial {
    match reason {
        Reason::Blocked => Denial::Blocked,
        Reason::NoBuilders => Denial::NoBuilders,
        Reason::Retreating => Denial::Retreating,
        Reason::Mp => Denial::Manpower,
        Reason::NotVisible => Denial::NotVisible,
        Reason::Max => Denial::Maximum,
    }
}

impl Match {
    fn validate_project_unit(&self, id: UnitId) -> Result<(), String> {
        let u = self
            .world
            .game
            .units
            .get(&id)
            .ok_or("missing project unit")?;
        if id == 0
            || u.id != id
            || u.owner >= self.world.game.players.len()
            || ![u.hp, u.x, u.z, u.repath].iter().all(|v| v.is_finite())
            || u.dig != self.field_works.is_active(id)
            || u.entrench != self.field_works.membership(id).is_some()
            || !self.commands.units.contains_key(&id)
            || self.firing.get(id).is_none()
            || self.abilities.get(id).is_none()
        {
            return Err("invalid canonical shared field-work state".into());
        }
        if let Some(job) = self.field_works.jobs().get(&id)
            && job.project != self.field_works.membership(id)
            && job.project.is_some()
        {
            return Err("paid segment names another project".into());
        }
        Ok(())
    }

    /// Exact planOf project branch. A finished membership does not hold a waiting order.
    pub fn project_goal(&self, id: UnitId) -> Result<Option<Point>, String> {
        let unit = self
            .world
            .game
            .units
            .get(&id)
            .ok_or("missing project plan unit")?;
        if !unit.entrench && self.field_works.membership(id).is_none() {
            return Ok(None);
        }
        self.validate_project_unit(id)?;
        let Some(p) = self.field_works.member_project(id) else {
            return Ok(None);
        };
        Ok(p.jobs
            .first()
            .map(|j| j.at)
            .or_else(|| (p.active != 0).then(|| self.world.game.units[&id].at())))
    }

    /// Early source recount, before waiting orders and per-unit paid-segment allocation.
    pub fn recount_field_projects(&mut self) -> Result<(), String> {
        let mut waiting = Vec::new();
        for (&id, c) in &self.commands.units {
            for order in &c.orders {
                if let Command::Entrench {
                    order: ProjectOrder::Join { project, .. },
                    ..
                } = order
                {
                    waiting.push((id, *project));
                }
            }
        }
        self.field_works
            .recount_projects(&self.world.game, &waiting)
            .map_err(|e| e.to_string())
    }

    fn prepare_project_join(&self, id: UnitId) -> Result<JoinedUnit, String> {
        self.validate_project_unit(id)?;
        let mut unit = self.world.game.units[&id].clone();
        if unit.build {
            return Err("unrepresented construction project".into());
        }
        unit.path_gen = unit
            .path_gen
            .checked_add(1)
            .ok_or("project path identity exhausted")?;
        let mut control = self.commands.units[&id].clone();
        let mut firing = self.firing.units[&id].clone();
        let mut ability = *self.abilities.get(id).unwrap();
        if unit.garrison.is_some() {
            let g = &self.world.game;
            let cell = SpawnPlacer::new()
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
                .map_err(|e| format!("project garrison exit: {e:?}"))?;
            unit.x = (cell % g.w) as f64 * CELL + CELL / 2.;
            unit.z = (cell / g.w) as f64 * CELL + CELL / 2.;
            unit.garrison = None;
        }
        unit.traffic = None;
        unit.traffic_wait = 0.;
        unit.world_goal = None;
        unit.entrench = true;
        unit.dig = false;
        unit.build = false;
        unit.board = 0;
        unit.amove = None;
        unit.attack = false;
        unit.path.clear();
        unit.repath = 0.;
        control.follow = 0;
        control.orders.clear();
        control.face = None;
        control.enter = None;
        control.fire_at = None;
        control.attack_id = 0;
        control.outcome = Some(MoveOutcome::Interrupted);
        control.outcome_tick = Some(self.world.game.tick);
        control.nade = None;
        firing.target_id = 0;
        ability.aimed_auto = false;
        ability.aimed_walked = false;
        Ok(JoinedUnit {
            unit,
            control,
            firing,
            ability,
        })
    }

    fn prepare_visible_project(
        &mut self,
        request: ProjectRequest,
    ) -> Result<PreparedProject, Error> {
        let mut sight = BTreeMap::new();
        loop {
            let mut missing = None;
            let result = self.field_works.prepare_project(
                &self.world,
                &self.start.economy.players[request.owner],
                request,
                |point| {
                    let cell = self
                        .world
                        .game
                        .cell_of(point.x, point.z)
                        .expect("planned in-map cell");
                    sight.get(&cell).copied().unwrap_or_else(|| {
                        missing = Some(point);
                        false
                    })
                },
            );
            if let Some(point) = missing {
                let visible = self
                    .recipient_sees(request.owner, point)
                    .map_err(|_| Error::InvalidInput)?;
                sight.insert(self.world.game.cell_of(point.x, point.z).unwrap(), visible);
            } else {
                return result;
            }
        }
    }

    /// Source new/join admission. The existing command owner calls this through recorded input.
    pub fn execute_entrench(
        &mut self,
        owner: usize,
        ids: &[i64],
        order: ProjectOrder,
        queue: bool,
        winner: bool,
    ) -> Result<Option<Denial>, String> {
        let Some(player) = self.start.economy.players.get(owner) else {
            return Ok(Some(Denial::Blocked));
        };
        if winner || player.out || player.away {
            return Ok(Some(Denial::Blocked));
        }
        if self.options.mode != crate::economy::Mode::Conquest || self.world.game.mode.is_some() {
            return Ok(Some(Denial::Unsupported));
        }
        let defs = definitions().map_err(|e| e.to_string())?;
        let mut mine = Vec::new();
        let mut crew = Vec::new();
        let mut able = Vec::new();
        for &raw in ids.iter().take(self.world.game.units.len().max(50)) {
            let Ok(id) = u32::try_from(raw) else { continue };
            let Some(u) = self.world.game.units.get(&id) else {
                continue;
            };
            let profile = self
                .rules
                .kinds
                .get(u.kind)
                .ok_or("unknown project unit kind")?;
            let c = self
                .commands
                .units
                .get(&id)
                .ok_or("missing project control")?;
            if u.owner != owner
                || profile.structure
                || profile.rail
                || u.rail
                || u.riding
                || c.operative
                || self.logistics.get(id).is_some_and(|r| r.forced)
            {
                continue;
            }
            self.validate_project_unit(id)?;
            mine.push(id);
            if defs.builders.contains(&profile.name) {
                crew.push(id);
                if !u.retreating {
                    able.push(id);
                }
            }
        }
        if !queue && able.iter().any(|id| self.world.game.units[id].build) {
            return Ok(Some(Denial::Unsupported));
        }
        if let ProjectOrder::New {
            material: ProjectMaterial::Fill | ProjectMaterial::Wall,
            ..
        } = order
        {
            return Ok(Some(Denial::Unsupported));
        }
        let existing = match order {
            ProjectOrder::Join { project, .. } => Some(project),
            _ => None,
        };
        let mut staged = Vec::new();
        let mut seen = BTreeSet::new();
        if !queue {
            for &id in &able {
                if (existing.is_none() || self.field_works.membership(id) != existing)
                    && seen.insert(id)
                {
                    staged.push(self.prepare_project_join(id)?);
                }
            }
        }
        if !queue && let Some(project) = existing.and_then(|id| self.field_works.project(id)) {
            if project
                .crew
                .checked_add(staged.len())
                .is_none_or(|n| n as u64 > 9_007_199_254_740_991)
            {
                return Err("project crew capacity exhausted".into());
            }
        }
        // Ordinary refusals publish source follow clearing; defensive errors publish no controls.
        let request = match order {
            ProjectOrder::Join { project, .. } => {
                let Some(p) = self.field_works.project(project) else {
                    return Ok(self.finish_project_selection(&mine, Some(Denial::Blocked)));
                };
                if p.jobs.is_empty()
                    || self.world.game.players[p.owner].team != self.world.game.players[owner].team
                {
                    return Ok(self.finish_project_selection(&mine, Some(Denial::Blocked)));
                }
                None
            }
            ProjectOrder::New {
                pattern,
                material,
                from,
                to,
            } => {
                let (Some(pattern), ProjectMaterial::Fort(fort), Some(from), Some(to)) =
                    (pattern, material, from, to)
                else {
                    return Ok(self.finish_project_selection(&mine, Some(Denial::Blocked)));
                };
                if ![from.x, from.z, to.x, to.z].iter().all(|v| v.is_finite())
                    || !line_fort(fort)
                    || (fort != FortKind::Trench
                        && !matches!(
                            pattern,
                            super::Pattern::Line | super::Pattern::Zigzag | super::Pattern::Double
                        ))
                {
                    return Ok(self.finish_project_selection(&mine, Some(Denial::Blocked)));
                }
                let clamp = |p: Point| Point {
                    x: math::max(0., math::min(self.world.game.w as f64 * CELL, p.x)),
                    z: math::max(0., math::min(self.world.game.h as f64 * CELL, p.z)),
                };
                Some((pattern, fort, clamp(from), clamp(to)))
            }
        };
        if able.is_empty() {
            return Ok(self.finish_project_selection(
                &mine,
                Some(if crew.is_empty() {
                    Denial::NoBuilders
                } else {
                    Denial::Retreating
                }),
            ));
        }
        if queue
            && able
                .iter()
                .all(|id| self.commands.units[id].orders.len() >= 8)
        {
            return Ok(self.finish_project_selection(&mine, Some(Denial::QueueFull)));
        }
        let project = if let Some((pattern, fort, from, to)) = request {
            let count = able.len() as f64;
            let back = Point {
                x: able
                    .iter()
                    .fold(0., |v, id| v + self.world.game.units[id].x)
                    / count,
                z: able
                    .iter()
                    .fold(0., |v, id| v + self.world.game.units[id].z)
                    / count,
            };
            match self.prepare_visible_project(ProjectRequest {
                owner,
                pattern,
                fort,
                from,
                to,
                back,
            }) {
                Ok(prepared) => prepared
                    .commit(&mut self.field_works)
                    .map_err(|e| e.to_string())?,
                Err(Error::Refused(reason)) => {
                    return Ok(self.finish_project_selection(&mine, Some(refusal(reason))));
                }
                Err(e) => return Err(e.to_string()),
            }
        } else {
            existing.unwrap()
        };
        let first = self.field_works.project(project).unwrap().jobs[0].at;
        if queue {
            for id in able {
                let c = self.commands.units.get_mut(&id).unwrap();
                if c.orders.len() < 8 {
                    c.orders.push_back(Command::Entrench {
                        ids: vec![i64::from(id)],
                        order: ProjectOrder::Join {
                            project,
                            at: Some(first),
                        },
                        queue: false,
                    });
                }
            }
            return Ok(self.finish_project_selection(&mine, None));
        }
        self.field_works
            .join_project(project, &able)
            .map_err(|e| e.to_string())?;
        for row in staged {
            let id = row.unit.id;
            self.world.game.units.insert(id, row.unit);
            self.commands.units.insert(id, row.control);
            self.firing.units.insert(id, row.firing);
            *self.abilities.get_mut(id).unwrap() = row.ability;
            self.grid.update(id, &self.world.game.units[&id]);
        }
        Ok(self.finish_project_selection(&mine, None))
    }

    fn finish_project_selection(
        &mut self,
        ids: &[UnitId],
        denial: Option<Denial>,
    ) -> Option<Denial> {
        for id in ids {
            self.commands
                .units
                .get_mut(id)
                .expect("validated project control")
                .follow = 0;
        }
        denial
    }

    /// Mass-work allocation immediately before the existing timed Dig phase of this source unit turn.
    pub fn apply_project_unit(&mut self, id: UnitId) -> Result<(), String> {
        self.validate_project_unit(id)?;
        let u = &self.world.game.units[&id];
        if u.hp <= 0. || u.riding || self.commands.units[&id].operative || !u.entrench || u.dig {
            return Ok(());
        }
        if self
            .field_works
            .finish_empty_project(id)
            .map_err(|e| e.to_string())?
        {
            self.world.game.units.get_mut(&id).unwrap().entrench = false;
            return Ok(());
        }
        let u = &self.world.game.units[&id];
        let c = &self.commands.units[&id];
        let busy = !u.path.is_empty()
            || u.world_goal.is_some()
            || c.attack_id != 0
            || c.nade.is_some()
            || u.amove.is_some()
            || u.build
            || c.enter.is_some()
            || u.board != 0
            || u.riding
            || c.fire_at.is_some()
            || u.retreating;
        if busy
            || u.garrison.is_some()
            || self.field_works.member_project(id).unwrap().jobs.is_empty()
        {
            return Ok(());
        }
        let owner = u.owner;
        let mut sight = BTreeMap::new();
        loop {
            let mut missing = None;
            let probe = self
                .field_works
                .prepare_segment(
                    &self.world,
                    &mut self.start.economy.players[owner],
                    id,
                    |point| {
                        let cell = self.world.game.cell_of(point.x, point.z).unwrap();
                        sight.get(&cell).copied().unwrap_or_else(|| {
                            missing = Some(point);
                            false
                        })
                    },
                )
                .map_err(|e| e.to_string())?;
            drop(probe);
            if let Some(point) = missing {
                let visible = self.recipient_sees(owner, point)?;
                sight.insert(self.world.game.cell_of(point.x, point.z).unwrap(), visible);
            } else {
                break;
            }
        }
        match self
            .field_works
            .prepare_segment(
                &self.world,
                &mut self.start.economy.players[owner],
                id,
                |point| sight[&self.world.game.cell_of(point.x, point.z).unwrap()],
            )
            .map_err(|e| e.to_string())?
        {
            SegmentPreparation::Waiting => {}
            SegmentPreparation::Discard { project, index } => self
                .field_works
                .discard_segment(project, index)
                .map_err(|e| e.to_string())?,
            SegmentPreparation::Paid(prepared) => {
                let changes = [StoryChange {
                    slot: Some(owner),
                    key: StoryKey::MpSpent,
                    amount: prepared.cost(),
                }];
                let story =
                    prepare_story_updates(&mut self.story, self.finish.is_some(), &changes)?;
                prepared
                    .commit(&mut self.field_works)
                    .map_err(|e| e.to_string())?;
                let u = self.world.game.units.get_mut(&id).unwrap();
                u.dig = true;
                u.repath = 0.;
                story.commit();
            }
        }
        Ok(())
    }
}
