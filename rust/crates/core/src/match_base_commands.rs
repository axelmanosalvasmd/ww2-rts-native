//! Site cancellation and producer rally commands on the canonical ordinary Match.
use std::collections::BTreeSet;

use crate::{
    catalog::Catalog,
    game::{CELL, MOVE, Point},
    match_commands::Denial,
    match_runtime::Match,
    match_story::{StoryChange, StoryKey, prepare_story_updates},
    math,
};

impl Match {
    pub(crate) fn execute_cancel_site(
        &mut self,
        owner: usize,
        raw_id: i64,
    ) -> Result<Option<Denial>, String> {
        // World paid construction and cancellation remain outside its admitted adapters.
        self.validate_construction_tick()?;
        let Some(id) = u32::try_from(raw_id).ok() else {
            return Ok(Some(Denial::Unseen));
        };
        let Some(site) = self.world.game.units.get(&id).filter(|u| {
            u.owner == owner
                && self.rules.kinds[u.kind].building
                && u.built.is_some_and(|b| b < 1.0)
        }) else {
            return Ok(Some(Denial::Unseen));
        };
        let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
        let refund = catalog
            .unit(&self.rules.kinds[site.kind].name)
            .map_err(|e| e.to_string())?
            .get("cost")
            .and_then(|v| v.number())
            .filter(|v| v.is_finite() && *v >= 0.0)
            .ok_or("invalid cancellation cost")?
            * 0.75;
        let mp = self.start.economy.players[owner].mp + refund;
        if !mp.is_finite() {
            return Err("invalid cancellation wallet".into());
        }
        let structure = self
            .world
            .structures
            .iter()
            .find(|s| s.entity_id == Some(id))
            .ok_or("missing cancellation structure")?;
        let cells = structure.cells.clone();
        let structure_id = structure.id.clone();
        let sections = structure
            .section_ids
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        self.world
            .preflight_cell_rows(&cells)
            .map_err(|e| e.to_string())?;
        if cells.is_empty()
            || sections
                .iter()
                .any(|id| self.world.section_by_id(id).is_none())
        {
            return Err("inconsistent cancellation structure".into());
        }
        let mut draft = self.world.begin_transaction().map_err(|e| e.to_string())?;
        draft.world.structures.retain(|s| s.id != structure_id);
        draft
            .world
            .detached_sections
            .retain(|id, _| !sections.contains(id));
        for cell in cells {
            // An admitted replacement may already own a failed corner's current
            // section. Only the canceled graph loses its section records.
            if draft
                .world
                .sections
                .get(&cell)
                .is_some_and(|s| s.structure_id == structure_id)
            {
                draft.world.sections.remove(&cell);
                draft.world.game.sections.remove(&(cell as u32));
            }
            // Use the existing surface/mine clearing rule and preserve crater height.
            let change = draft
                .world
                .set_cell(cell, b'.')
                .map_err(|e| e.to_string())?;
            if change.logged_index.is_none() {
                // A previously failed section may already be open ground. Publish
                // its removal even when the physical character stays the same.
                draft
                    .world
                    .log_cell(cell, false)
                    .map_err(|e| e.to_string())?;
            }
        }
        draft.world.game.units.remove(&id);
        let story = prepare_story_updates(
            &mut self.story,
            false,
            &[StoryChange {
                slot: Some(owner),
                key: StoryKey::MpSpent,
                amount: -refund,
            }],
        )?;
        let publication = draft
            .prepare_publication(&mut self.world)
            .map_err(|e| e.to_string())?;
        publication.commit();
        self.environment.cancel_building_sections(&sections);
        self.grid.remove_unit(id);
        self.commands.units.remove(&id);
        self.abilities.remove(id);
        self.recovery.remove(id);
        self.production.remove_building(id);
        self.building_rallies.remove(&id);
        self.recruit_metadata.remove(&id);
        self.aviation.remove(id);
        self.logistics.remove(id);
        // Keep combat/firing attribution and historical footprint ownership, as on death.
        // Builders notice their missing target at their next actual unit turn.
        self.start.economy.players[owner].mp = mp;
        story.commit();
        Ok(None)
    }

    pub(crate) fn execute_rally(
        &mut self,
        owner: usize,
        ids: &[i64],
        scalar: Option<i64>,
        at: Point,
    ) -> Result<Option<Denial>, String> {
        self.validate_construction_tick()?;
        if !at.x.is_finite() || !at.z.is_finite() {
            return Ok(Some(Denial::Blocked));
        }
        if scalar.is_none() && ids.is_empty() {
            let g = &self.world.game;
            if at.x < 0.0 || at.z < 0.0 || at.x >= g.w as f64 * CELL || at.z >= g.h as f64 * CELL {
                return Ok(Some(Denial::Blocked));
            }
            let cell = (at.z / CELL).floor() as usize * g.w + (at.x / CELL).floor() as usize;
            if g.flags[cell] & MOVE != 0 {
                return Ok(Some(Denial::Blocked));
            }
            self.player_rallies[owner] = Some(at);
            return Ok(None);
        }
        let at = Point {
            x: math::min(self.world.game.w as f64 * CELL, math::max(0.0, at.x)),
            z: math::min(self.world.game.h as f64 * CELL, math::max(0.0, at.z)),
        };
        let list = scalar.as_slice();
        let list = if scalar.is_some() { list } else { ids };
        let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
        let mut selected = Vec::new();
        let mut owned = false;
        for id in list.iter().filter_map(|id| u32::try_from(*id).ok()) {
            let Some(unit) = self.world.game.units.get(&id).filter(|u| u.owner == owner) else {
                continue;
            };
            owned = true;
            if unit.hp > 0.0
                && catalog
                    .unit(&self.rules.kinds[unit.kind].name)
                    .map_err(|e| e.to_string())?
                    .get("makes")
                    .is_some()
            {
                if !self.building_rallies.contains_key(&id) {
                    return Err("missing canonical building rally".into());
                }
                selected.push(id);
            }
        }
        if selected.is_empty() {
            return Ok(Some(if owned { Denial::Needs } else { Denial::Unseen }));
        }
        for id in selected {
            self.building_rallies.insert(id, Some(at));
        }
        Ok(None)
    }
}

#[cfg(test)]
#[path = "bases_runtime_command_tests.rs"]
mod tests;
