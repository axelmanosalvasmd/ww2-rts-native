//! Recorded native base commands. Transport admission stays with the room owner.
use super::{Match, production_step};
use crate::{
    catalog::Catalog,
    economy::Mode,
    game::{CELL, MOVE, Point},
    match_commands::Denial,
    match_story::{StoryChange, StoryKey, prepare_story_updates},
};

impl Match {
    /// The paid job and its spending refund commit together after all preflight.
    pub(crate) fn execute_production_cancel(
        &mut self,
        slot: usize,
        building: Option<u32>,
        job: Option<i64>,
    ) -> Result<Option<Denial>, String> {
        if !self.options.mode.is_construction() {
            return Ok(Some(Denial::Needs));
        }
        if !matches!(self.options.mode, Mode::World | Mode::Classic) {
            return Ok(Some(Denial::Unsupported));
        }
        let count = self.production_step_context().map_err(|e| format!("production cancel context: {e:?}"))?;
        let (Some(building), Some(job)) = (building, job) else {
            return Ok(Some(Denial::Unseen));
        };
        let regions = self.production_regions().map_err(|e| format!("production cancel regions: {e:?}"))?;
        let units = production_step::units(&self.world.game, &self.rules, count)
            .map_err(|e| format!("production cancel actors: {e:?}"))?;
        let context = production_step::context(
            &self.options, self.world.game.naval, &units, regions.as_deref(), self.classic_sudden_death(),
        );
        // A negative safe integer is a valid Source job lookup that cannot match
        // a positive allocated identity. Invalid numbers were retained as None.
        let job = u64::try_from(job).unwrap_or(0);
        let prepared = match self.production.prepare_cancel(
            &context, &mut self.start.economy, slot, building, job,
        ) {
            Ok(prepared) => prepared,
            Err(crate::production::Error::Denied(reason)) => return Ok(Some(reason.into())),
            Err(error) => return Err(format!("production cancel: {error}")),
        };
        let story = prepare_story_updates(&mut self.story, self.finish.is_some(), &[StoryChange {
            slot: Some(slot), key: StoryKey::MpSpent, amount: -prepared.refund().mp,
        }])?;
        prepared.commit();
        story.commit();
        Ok(None)
    }

    /// Both personal and selected-building rally use their canonical stored owner.
    pub(crate) fn execute_rally(
        &mut self,
        slot: usize,
        ids: &[i64],
        scalar: Option<i64>,
        explicit_id: bool,
        at: Option<Point>,
    ) -> Result<Option<Denial>, String> {
        if !matches!(self.options.mode, Mode::Default | Mode::Conquest | Mode::World | Mode::Classic) {
            return Ok(Some(Denial::Unsupported));
        }
        self.production_step_context().map_err(|e| format!("rally context: {e:?}"))?;
        let Some(at) = at.filter(|p| p.x.is_finite() && p.z.is_finite()) else {
            return Ok(Some(Denial::Blocked));
        };
        let game = &self.world.game;
        let selected_building = self.options.mode.is_construction() || explicit_id || !ids.is_empty();
        if !selected_building {
            if !matches!(self.options.mode, Mode::Default | Mode::Conquest) {
                return Ok(Some(Denial::Unsupported));
            }
            if at.x < 0.0 || at.z < 0.0 || at.x >= game.w as f64 * CELL || at.z >= game.h as f64 * CELL {
                return Ok(Some(Denial::Blocked));
            }
            let cell = (at.z / CELL) as usize * game.w + (at.x / CELL) as usize;
            if game.flags.get(cell).ok_or("rally terrain dimensions")? & MOVE != 0 {
                return Ok(Some(Denial::Blocked));
            }
            self.player_rallies[slot] = Some(at);
            return Ok(None);
        }
        let selected = if explicit_id {
            scalar.into_iter().collect::<Vec<_>>()
        } else {
            ids.iter().copied().take(50.max(game.units.len())).collect()
        };
        let mut makers = Vec::new();
        let mut own = false;
        let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
        for id in selected.into_iter().filter_map(|id| u32::try_from(id).ok()) {
            let Some(unit) = game.units.get(&id) else { continue; };
            own |= unit.owner == slot;
            // The supported Conquest/World contexts grant only owner access.
            // Horde access needs its actual context, never an owner relabel.
            if unit.owner != slot || unit.hp <= 0.0 {
                continue;
            }
            let kind = self.rules.kinds.get(unit.kind).ok_or("rally actor kind")?;
            if catalog.unit(&kind.name).map_err(|e| e.to_string())?.get("makes").is_some() {
                if self.production.building(id).is_none() || !self.building_rallies.contains_key(&id) {
                    return Err("rally producer metadata missing".into());
                }
                makers.push(id);
            }
        }
        if makers.is_empty() {
            return Ok(Some(if own { Denial::Needs } else { Denial::Unseen }));
        }
        let at = Point {
            x: crate::math::min(game.w as f64 * CELL, crate::math::max(0.0, at.x)),
            z: crate::math::min(game.h as f64 * CELL, crate::math::max(0.0, at.z)),
        };
        for id in makers {
            self.building_rallies.insert(id, Some(at));
        }
        Ok(None)
    }
}
