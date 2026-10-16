//! Read-only admission for the shared-project phases in the caller's real tick.
use super::{FieldWorksRuntime, FortKind, ProjectOrder, definitions, line_fort, position};
use crate::{game::Game, grid::UnitId, match_commands::Command, match_runtime::Match};
use std::collections::{BTreeMap, BTreeSet};

const SAFE_INTEGER: i64 = 9_007_199_254_740_991;

#[cfg(test)]
#[path = "field_project_tick_tests.rs"]
mod tests;

impl FieldWorksRuntime {
    fn validate_tick_records(&self, game: &Game, dt: f64) -> Result<(), String> {
        if self.project_sequence() > SAFE_INTEGER as u64 {
            return Err("invalid field-project sequence".into());
        }
        let cells = game
            .w
            .checked_mul(game.h)
            .ok_or("invalid field-project terrain dimensions")?;
        let defs = definitions().map_err(|e| e.to_string())?;
        for (&unit, &project) in &self.projects.members {
            if !game.units.contains_key(&unit) || !self.projects.records.contains_key(&project) {
                return Err("field-project membership names an absent identity".into());
            }
        }
        let references = self
            .projects
            .members
            .values()
            .copied()
            .chain(self.jobs.values().filter_map(|job| job.project))
            .collect::<BTreeSet<_>>();
        for (&id, p) in &self.projects.records {
            if id != p.id {
                return Err("field-project record identity mismatch".into());
            }
            if self.project(id).is_none() && !references.contains(&id) {
                continue;
            }
            if p.id == 0
                || p.id > self.project_sequence()
                || p.owner >= game.players.len()
                || !line_fort(p.fort)
                || p.crew as u64 > SAFE_INTEGER as u64
                || !(-SAFE_INTEGER..=SAFE_INTEGER).contains(&p.active)
                || p.jobs.iter().any(|j| {
                    !line_fort(j.kind)
                        || !position(j.at)
                        || !j.direction.is_finite()
                        || !j.front.is_finite()
                })
            {
                return Err("invalid canonical field-project record".into());
            }
        }
        let mut active = BTreeMap::<u64, i64>::new();
        for (&id, job) in &self.jobs {
            let unit = game
                .units
                .get(&id)
                .ok_or("paid field work names a retired unit")?;
            if !position(job.at)
                || !job.direction.is_finite()
                || !job.elapsed.is_finite()
                || job.elapsed < 0.0
                || !(job.elapsed + dt).is_finite()
                || job.front.is_some_and(|n| !n.is_finite())
                || job.reach.is_some_and(|n| !n.is_finite() || n < 0.0)
                || job.from.is_some_and(|p| !position(p))
                || job.cells.is_empty()
                || job.cells.len() > 6
                || job.original_cells.is_some_and(|n| n < job.cells.len())
                || job.cells.iter().any(|c| {
                    c.cell >= cells
                        || if job.kind == FortKind::Nest {
                            !b"T#".contains(&c.ch)
                        } else {
                            c.ch != defs.forts[&job.kind].ch
                        }
                })
            {
                return Err("invalid canonical paid field-work descriptor".into());
            }
            if let Some(project) = job.project {
                if !self.projects.records.contains_key(&project)
                    || self.membership(id) != Some(project)
                    || !line_fort(job.kind)
                {
                    return Err("paid field work names another project".into());
                }
                if unit.hp > 0.0 {
                    let count = active.entry(project).or_default();
                    *count = count
                        .checked_add(1)
                        .ok_or("field-project active count exhausted")?;
                }
            }
        }
        // Source only recounts when at least one project remains in its listed Map.
        // Archived paid references retain their prior active count during that recount.
        if self.projects().next().is_some() {
            for (id, count) in active {
                let p = &self.projects.records[&id];
                let before = if self.project(id).is_some() {
                    0
                } else {
                    p.active
                };
                if before
                    .checked_add(count)
                    .is_none_or(|n| !(-SAFE_INTEGER..=SAFE_INTEGER).contains(&n))
                {
                    return Err("field-project active count exhausted".into());
                }
            }
        }
        Ok(())
    }
}

impl Match {
    /// Call before advancing the clock, using the complete current unit order.
    /// This reads only project-owned facts. It does not refresh sight, spend, recount or sort turns.
    /// Common terrain, combat and runner admission remain the caller's responsibility.
    pub fn validate_field_project_tick(&self, current_order: &[UnitId]) -> Result<(), String> {
        if !crate::match_actor::ordinary_ground_mode(&self.world.game, self.options.mode) {
            return Err("field-project tick supports ordinary Conquest only".into());
        }
        let game = &self.world.game;
        if !(0..SAFE_INTEGER).contains(&game.tick) {
            return Err("field-project tick clock exhausted or invalid".into());
        }
        let ids = current_order.iter().copied().collect::<BTreeSet<_>>();
        if ids.len() != current_order.len()
            || ids.len() != game.units.len()
            || ids
                .iter()
                .any(|id| *id == 0 || !game.units.contains_key(id))
        {
            return Err("invalid field-project current unit order".into());
        }
        let dt = self.rules.movement.tick;
        if !dt.is_finite() || dt <= 0.0 {
            return Err("invalid field-project tick interval".into());
        }
        self.field_works.validate_tick_records(game, dt)?;
        let mut joining = BTreeMap::<u64, BTreeSet<UnitId>>::new();
        for &id in current_order {
            let u = &game.units[&id];
            let control = self
                .commands
                .units
                .get(&id)
                .ok_or("missing field-project current control")?;
            let queued = control
                .orders
                .iter()
                .any(|o| matches!(o, Command::Entrench { .. }));
            if u.dig
                || u.entrench
                || self.field_works.is_active(id)
                || self.field_works.membership(id).is_some()
                || queued
            {
                self.validate_project_unit(id)?;
                let kind = self
                    .rules
                    .kinds
                    .get(u.kind)
                    .ok_or("unknown field-project unit kind")?;
                let wallet = self
                    .start
                    .economy
                    .players
                    .get(u.owner)
                    .ok_or("missing field-project wallet")?;
                if !wallet.mp.is_finite()
                    || u.build
                    || u.air.is_some()
                    || u.rail
                    || kind.rail
                    || u.riding
                    || control.operative
                {
                    return Err("unsupported or invalid field-project participant".into());
                }
            }
            for order in &control.orders {
                if let Command::Entrench { ids, order, queue } = order {
                    let ProjectOrder::Join {
                        project,
                        at: Some(at),
                    } = order
                    else {
                        return Err("unrepresented field-project waiting order".into());
                    };
                    if *queue
                        || ids.as_slice() != [i64::from(id)]
                        || *project > SAFE_INTEGER as u64
                        || !position(*at)
                    {
                        return Err("invalid field-project waiting order".into());
                    }
                    if (game.tick + 1) % 20 != 0
                        && u.hp > 0.0
                        && self.field_works.membership(id) != Some(*project)
                        && self
                            .field_works
                            .project(*project)
                            .is_some_and(|p| !p.jobs.is_empty())
                    {
                        joining.entry(*project).or_default().insert(id);
                    }
                    // An absent listed project is an ordinary later Blocked denial, not a schema error.
                }
            }
        }
        // Reserve room for distinct potential waiting members. The source's twentieth-tick
        // sweep replaces cached crew counts before any waiting Join executes.
        for (project, ids) in joining {
            if self
                .field_works
                .project(project)
                .unwrap()
                .crew
                .checked_add(ids.len())
                .is_none_or(|n| n as u64 > SAFE_INTEGER as u64)
            {
                return Err("field-project waiting crew capacity exhausted".into());
            }
        }
        Ok(())
    }
}
