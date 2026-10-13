//! Ephemeral administrative World transaction. One canonical World remains live.
//! Preserve terrain allocations and ordered history when publishing the prepared draft.
use super::{Recipient, RegionVersions, World, WorldError};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) struct WorldTransaction {
    pub(crate) world: World,
    dimensions: (usize, usize),
    flags_identity: usize,
    versions: RegionVersions,
    tick: i64,
    journal_end: u64,
    journal_base: u64,
    terrain_version: u32,
    pending: BTreeMap<Recipient, BTreeSet<usize>>,
}
#[must_use = "commit after every phase guard succeeds, or drop without changing World"]
pub(crate) struct PreparedWorldPublication<'a> {
    target: &'a mut World,
    draft: World,
}
impl World {
    /// Only administrative batches needing real removal or wrecking create this draft.
    /// It is temporary preparation, not another live terrain or gameplay owner.
    pub(crate) fn begin_transaction(&self) -> Result<WorldTransaction, WorldError> {
        let journal_end = self.journal_end()?;
        let world = Self {
            game: self.game.clone(),
            mine_layer: self.mine_layer.clone(),
            materials: self.materials.clone(),
            base_materials: self.base_materials.clone(),
            object_materials: self.object_materials.clone(),
            mine_materials: self.mine_materials.clone(),
            surface_wear: self.surface_wear.clone(),
            burnt: self.burnt.clone(),
            scar: self.scar.clone(),
            cell_state: self.cell_state.clone(),
            shoal: self.shoal.clone(),
            roads: self.roads,
            mines: self.mines.clone(),
            mine_seen: self.mine_seen.clone(),
            aid: self.aid.clone(),
            fires: self.fires.clone(),
            structures: self.structures.clone(),
            sections: self.sections.clone(),
            detached_sections: self.detached_sections.clone(),
            initial: self.initial.clone(),
            geography: self.geography.clone(),
            versions: self.versions,
            rules: self.rules.clone(),
            log: self.log.clone(),
            log_indexes: self.log_indexes.clone(),
            new_cells: self.new_cells.clone(),
            pending: self.pending.clone(),
            journal: self.journal.clone(),
            journal_base: self.journal_base,
            soak: self.soak.clone(),
            soak_members: self.soak_members.clone(),
        };
        Ok(WorldTransaction {
            world,
            dimensions: (self.game.w, self.game.h),
            flags_identity: self.game.flags.as_ptr() as usize,
            versions: self.versions,
            tick: self.game.tick,
            journal_end,
            journal_base: self.journal_base,
            terrain_version: self.game.terrain_version,
            pending: self.pending.clone(),
        })
    }
}
impl WorldTransaction {
    /// Bind the draft to its original structural context before publication.
    /// The caller holds exclusive source ownership throughout preparation; HP, materials and
    /// recipient writes without structural revisions are outside this binding check.
    pub(crate) fn prepare_publication(
        self,
        target: &mut World,
    ) -> Result<PreparedWorldPublication<'_>, WorldError> {
        let n = target
            .game
            .w
            .checked_mul(target.game.h)
            .ok_or(WorldError::InvalidTerrain)?;
        let g = &self.world.game;
        if self.dimensions != (target.game.w, target.game.h)
            || self.dimensions != (g.w, g.h)
            || self.flags_identity != target.game.flags.as_ptr() as usize
            || self.versions != target.versions
            || self.tick != target.game.tick
            || self.journal_end != target.journal_end()?
            || self.journal_base != target.journal_base
            || self.terrain_version != target.game.terrain_version
            || self.pending != target.pending
            || !terrain_shape(target, n)
            || !terrain_shape(&self.world, n)
            || !preserves_optional(&target.game.height, &g.height)
            || !preserves_optional(&target.game.ground, &g.ground)
            || !preserves_optional(&target.game.objects, &g.objects)
            || !preserves_optional(&target.game.wear, &g.wear)
            || !preserves_optional(&target.game.trench_front, &g.trench_front)
            || !preserves_optional(&target.game.tough, &g.tough)
            || !preserves_optional(&target.shoal, &self.world.shoal)
        {
            return Err(WorldError::InvalidTerrain);
        }
        Ok(PreparedWorldPublication {
            target,
            draft: self.world,
        })
    }
}
fn terrain_shape(w: &World, n: usize) -> bool {
    let g = &w.game;
    [
        g.flags.len(),
        g.house.len(),
        g.chars.len(),
        g.cell_hp.len(),
        w.mine_layer.len(),
        w.surface_wear.len(),
        w.burnt.len(),
        w.scar.len(),
        w.cell_state.len(),
    ]
    .into_iter()
    .all(|len| len == n)
        && g.height.as_ref().is_none_or(|v| v.len() == n)
        && g.ground.as_ref().is_none_or(|v| v.len() == n)
        && g.objects.as_ref().is_none_or(|v| v.len() == n)
        && g.wear.as_ref().is_none_or(|v| v.len() == n)
        && g.trench_front.as_ref().is_none_or(|v| v.len() == n)
        && g.tough.as_ref().is_none_or(|v| v.len() == n)
        && w.shoal.as_ref().is_none_or(|v| v.len() == n)
}
fn preserves_optional<T>(old: &Option<Vec<T>>, new: &Option<Vec<T>>) -> bool {
    old.is_none() || new.is_some()
}
fn retain_allocation<T: Clone>(original: &mut Vec<T>, prepared: &mut Vec<T>) {
    original.clone_from(prepared);
    std::mem::swap(original, prepared);
}
fn retain_optional<T: Clone>(original: &mut Option<Vec<T>>, prepared: &mut Option<Vec<T>>) {
    if let (Some(original), Some(prepared)) = (original, prepared) {
        retain_allocation(original, prepared);
    }
}
impl PreparedWorldPublication<'_> {
    /// Terrain vectors keep their source addresses; revisions and journals keep draft order.
    /// All fallible validation precedes this exclusive publication.
    pub(crate) fn commit(mut self) {
        retain_allocation(&mut self.target.game.flags, &mut self.draft.game.flags);
        retain_allocation(&mut self.target.game.house, &mut self.draft.game.house);
        retain_allocation(&mut self.target.game.chars, &mut self.draft.game.chars);
        retain_allocation(&mut self.target.game.cell_hp, &mut self.draft.game.cell_hp);
        retain_optional(&mut self.target.game.height, &mut self.draft.game.height);
        retain_optional(&mut self.target.game.ground, &mut self.draft.game.ground);
        retain_optional(&mut self.target.game.objects, &mut self.draft.game.objects);
        retain_optional(&mut self.target.game.wear, &mut self.draft.game.wear);
        retain_optional(
            &mut self.target.game.trench_front,
            &mut self.draft.game.trench_front,
        );
        retain_optional(&mut self.target.game.tough, &mut self.draft.game.tough);
        retain_allocation(&mut self.target.mine_layer, &mut self.draft.mine_layer);
        retain_allocation(&mut self.target.surface_wear, &mut self.draft.surface_wear);
        retain_allocation(&mut self.target.burnt, &mut self.draft.burnt);
        retain_allocation(&mut self.target.scar, &mut self.draft.scar);
        retain_allocation(&mut self.target.cell_state, &mut self.draft.cell_state);
        retain_optional(&mut self.target.shoal, &mut self.draft.shoal);
        // The initial snapshot, geography and rules remain the original immutable owners.
        std::mem::swap(&mut self.target.initial, &mut self.draft.initial);
        std::mem::swap(&mut self.target.geography, &mut self.draft.geography);
        std::mem::swap(&mut self.target.rules, &mut self.draft.rules);
        *self.target = self.draft;
    }
}
