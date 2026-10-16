//! Ordinary paid field works over the singular Match terrain, wallet and job owners.
use super::{Error, FortKind, Reason, StartRequest};
use crate::{
    game::{CELL, MOVE, Point},
    grid::UnitId,
    match_commands::{Denial, MoveOutcome},
    match_runtime::Match,
    match_story::{StoryChange, StoryKey, prepare_story_updates},
    math,
    navigation::{TerrainView, Versions},
    path_requests::RequestKind,
    unit_spawn::SpawnPlacer,
};
use std::collections::BTreeMap;

fn denial(reason: Reason) -> Denial {
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
    fn validate_field_work_state(&self) -> Result<(), String> {
        if !crate::match_actor::ordinary_mode(&self.world.game, self.options.mode) {
            return Err("Match field works require a matching ordinary mode".into());
        }
        if self.world.game.tick < 0 || self.world.game.tick > 9_007_199_254_740_991 {
            return Err("invalid field-work clock".into());
        }
        if self
            .field_works
            .jobs
            .keys()
            .any(|id| !self.world.game.units.contains_key(id))
        {
            return Err("field work names a retired unit".into());
        }
        for id in self.world.game.units.keys() {
            self.validate_field_work_unit(*id)?;
        }
        Ok(())
    }

    fn validate_field_work_unit(&self, id: UnitId) -> Result<(), String> {
        let Some(unit) = self.world.game.units.get(&id) else {
            return if self.field_works.is_active(id) {
                Err("field work names a retired unit".into())
            } else {
                Ok(())
            };
        };
        if unit.dig != self.field_works.is_active(id)
            || unit.entrench != self.field_works.membership(id).is_some()
        {
            return Err("unrepresented or inconsistent field-work activity".into());
        }
        if !self.commands.units.contains_key(&id)
            || !self.firing.units.contains_key(&id)
            || self.combat.get(id).is_none()
            || self.abilities.get(id).is_none()
        {
            return Err("field work requires canonical unit metadata".into());
        }
        crate::match_construction::validate_build_activity(unit, &self.commands.units[&id])?;
        Ok(())
    }

    /// Called by the typed dispatcher after its global recorded-input checks.
    /// Only the first selected ID is considered. Queued digs are explicitly unsupported.
    pub fn execute_dig(
        &mut self,
        owner: usize,
        ids: &[i64],
        kind: FortKind,
        at: Point,
        direction: Option<f64>,
        winner_decided: bool,
    ) -> Result<Option<Denial>, String> {
        let Some(player) = self.start.economy.players.get(owner) else {
            return Ok(Some(Denial::Blocked));
        };
        if winner_decided || player.away || player.out {
            return Ok(Some(Denial::Blocked));
        }
        self.validate_field_work_state()?;
        let Some(id) = ids.first().copied().and_then(|id| u32::try_from(id).ok()) else {
            return Ok(Some(Denial::NoBuilders));
        };
        let Some(unit) = self.world.game.units.get(&id) else {
            return Ok(Some(Denial::NoBuilders));
        };
        let profile = self
            .rules
            .kinds
            .get(unit.kind)
            .ok_or("unknown dig unit kind")?;
        let control = &self.commands.units[&id];
        let eligible = unit.owner == owner
            && !profile.structure
            && !profile.rail
            && !unit.rail
            && !unit.riding
            && !control.operative
            && !self.logistics.get(id).is_some_and(|r| r.forced);
        if !eligible {
            return Ok(Some(Denial::NoBuilders));
        }
        crate::match_construction::validate_build_activity(unit, control)?;
        if unit.path_gen == u64::MAX {
            return Err("dig path identity exhausted".into());
        }
        let request = StartRequest {
            control_eligible: eligible,
            owner,
            unit: id,
            kind,
            at,
            direction,
        };
        let mut sight = BTreeMap::<usize, bool>::new();
        // Ask the shared placement helper for its next sight query. A missing query returns
        // false, stopping before buildability reads. At most six source stamp cells are queried.
        loop {
            let mut missing = None;
            let probe = {
                let prepared = self.field_works.prepare_start(
                    &self.world,
                    &mut self.start.economy.players[owner],
                    request,
                    |point| {
                        let cell = self
                            .world
                            .game
                            .cell_of(point.x, point.z)
                            .expect("in-map placement query");
                        sight.get(&cell).copied().unwrap_or_else(|| {
                            missing = Some(point);
                            false
                        })
                    },
                );
                match prepared {
                    Ok(prepared) => {
                        drop(prepared);
                        Ok(())
                    }
                    Err(error) => Err(error),
                }
            };
            match probe {
                Err(Error::Refused(Reason::NotVisible)) if missing.is_some() => {
                    let point = missing.unwrap();
                    let visible = self.recipient_sees(owner, point)?;
                    sight.insert(self.world.game.cell_of(point.x, point.z).unwrap(), visible);
                }
                Err(Error::Refused(reason)) => {
                    // Source mine(ids[0]) clears follow even when this ordinary dig is refused.
                    self.commands.units.get_mut(&id).unwrap().follow = 0;
                    return Ok(Some(denial(reason)));
                }
                Err(error) => return Err(format!("dig admission: {error}")),
                Ok(()) => {
                    let prepared = self
                        .field_works
                        .prepare_start(
                            &self.world,
                            &mut self.start.economy.players[owner],
                            request,
                            |point| sight[&self.world.game.cell_of(point.x, point.z).unwrap()],
                        )
                        .map_err(|error| format!("prepared dig admission changed: {error}"))?;
                    let mut unit = self.world.game.units[&id].clone();
                    let mut control = self.commands.units[&id].clone();
                    let mut firing = self.firing.units[&id];
                    let mut ability = *self.abilities.get(id).unwrap();
                    if unit.garrison.is_some() {
                        let game = &self.world.game;
                        let cell = SpawnPlacer::new()
                            .nearest_free(
                                TerrainView {
                                    width: game.w,
                                    height: game.h,
                                    flags: &game.flags,
                                    heights: game.height.as_deref(),
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
                            .map_err(|error| format!("dig garrison exit: {error:?}"))?;
                        unit.x = (cell % game.w) as f64 * CELL + CELL / 2.;
                        unit.z = (cell / game.w) as f64 * CELL + CELL / 2.;
                        unit.garrison = None;
                    }
                    unit.traffic = None;
                    unit.traffic_wait = 0.;
                    unit.world_goal = None;
                    unit.entrench = false;
                    unit.dig = true;
                    unit.build = false;
                    control.build_target = None;
                    unit.board = 0;
                    unit.amove = None;
                    unit.attack = false;
                    unit.path.clear();
                    unit.path_gen += 1;
                    unit.repath = 0.;
                    control.follow = 0;
                    control.orders.clear();
                    control.face = None;
                    control.enter = None;
                    control.fire_at = None;
                    control.attack_id = 0;
                    control.outcome = Some(MoveOutcome::Interrupted);
                    control.outcome_tick = Some(self.world.game.tick);
                    firing.target_id = 0;
                    ability.aimed_auto = false;
                    ability.aimed_walked = false;
                    control.nade = None;
                    let updates = [StoryChange {
                        slot: Some(owner),
                        key: StoryKey::MpSpent,
                        amount: prepared.cost(),
                    }];
                    let story =
                        prepare_story_updates(&mut self.story, self.finish.is_some(), &updates)?;
                    // Earlier parity validation plus exclusive field borrows make cancellation infallible.
                    crate::match_runtime::cancel_field_work_state(
                        &mut self.field_works,
                        self.world.game.units.get_mut(&id).unwrap(),
                    )
                    .expect("prepared field cancellation");
                    prepared.commit(&mut self.field_works);
                    self.world.game.units.insert(id, unit);
                    self.commands.units.insert(id, control);
                    self.firing.units.insert(id, firing);
                    *self.abilities.get_mut(id).unwrap() = ability;
                    self.grid.update(id, &self.world.game.units[&id]);
                    story.commit();
                    return Ok(None);
                }
            }
        }
    }

    /// Exact source currentPathGoal('dig'), including the first cached distant approach.
    pub fn current_dig_goal(&mut self, id: UnitId) -> Option<Point> {
        let unit = self.world.game.units.get(&id)?;
        let job = self.field_works.jobs.get_mut(&id)?;
        if unit.hp <= 0. || unit.retreating || !unit.path.is_empty() {
            return None;
        }
        let distance = math::hypot(unit.x - job.at.x, unit.z - job.at.z);
        if distance.partial_cmp(&job.reach.unwrap_or(3.)) != Some(std::cmp::Ordering::Greater) {
            return None;
        }
        Some(if let Some(reach) = job.reach {
            *job.from.get_or_insert_with(|| {
                let fraction = (reach - 2.) / if distance == 0. { 1. } else { distance };
                Point {
                    x: job.at.x + (unit.x - job.at.x) * fraction,
                    z: job.at.z + (unit.z - job.at.z) * fraction,
                }
            })
        } else {
            job.at
        })
    }

    /// The ordinary digging phase in one unit's source loop, at the fixed simulation interval.
    pub fn apply_field_work_unit(&mut self, id: UnitId) -> Result<(), String> {
        self.apply_field_work_unit_dt(id, 0.05)
    }

    /// Same source work phase at a supplied component interval. The Match runner uses the fixed wrapper.
    pub fn apply_field_work_unit_dt(&mut self, id: UnitId, dt: f64) -> Result<(), String> {
        if !dt.is_finite() || !(0.0..=7200.0).contains(&dt) {
            return Err("invalid field-work interval".into());
        }
        self.validate_field_work_unit(id)?;
        let Some(job) = self.field_works.jobs.get(&id) else {
            return Ok(());
        };
        let unit = &self.world.game.units[&id];
        if unit.hp <= 0. || unit.riding || self.commands.units[&id].operative {
            return Ok(());
        }
        crate::match_construction::validate_build_activity(unit, &self.commands.units[&id])?;
        let distance = math::hypot(unit.x - job.at.x, unit.z - job.at.z);
        if distance > job.reach.unwrap_or(3.) {
            if unit.repath <= 0. && unit.path.is_empty() {
                // Submit before changing work state. The scheduler owns its current-goal cache
                // and rejects an absent path phase or exhausted path identity before that cache.
                let goal = job.from.unwrap_or_else(|| {
                    if let Some(reach) = job.reach {
                        let fraction = (reach - 2.) / if distance == 0. { 1. } else { distance };
                        Point {
                            x: job.at.x + (unit.x - job.at.x) * fraction,
                            z: job.at.z + (unit.z - job.at.z) * fraction,
                        }
                    } else {
                        job.at
                    }
                });
                self.request_command_path(id, goal, RequestKind::Dig)?;
            }
            return Ok(());
        }
        let completes = job.cells.len() == 1
            && job.elapsed + dt
                >= self
                    .field_works
                    .span(&self.world, id, job.kind)
                    .map_err(|e| e.to_string())?;
        let updates = if completes {
            vec![StoryChange {
                slot: Some(unit.owner),
                key: StoryKey::Built,
                amount: 1.,
            }]
        } else {
            Vec::new()
        };
        let story = prepare_story_updates(&mut self.story, self.finish.is_some(), &updates)?;
        let result = self
            .field_works
            .step_unit(&mut self.world, id, dt)
            .map_err(|e| format!("field work step: {e}"))?;
        if result.completed.is_some() {
            self.world.game.units.get_mut(&id).unwrap().dig = false;
        }
        story.commit();
        debug_assert!(
            result.route.is_none(),
            "in-reach work cannot request a route"
        );
        Ok(())
    }
}

#[cfg(test)]
#[path = "match_engineering_tests.rs"]
mod tests;
