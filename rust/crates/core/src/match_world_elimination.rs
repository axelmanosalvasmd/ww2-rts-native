//! Administrative World elimination. Prepare actual ordered wrecks without combat death rules.
use crate::{
    environment::{Context, Effect, EnvironmentRuntime},
    game::Unit,
    grid::SpatialGrid,
    match_combat::CombatState,
    match_runtime::Match,
    match_world::{RegionState, ResourceNode},
    match_world_regions::WorldCapture,
    world::WorldTransaction,
};
use std::collections::BTreeSet;

/// Transient phase output. Canonical ownership, health, terrain and metadata stay in Match.
#[derive(Clone, Debug, Default)]
pub struct WorldRegionEffects {
    pub captures: Vec<WorldCapture>,
    /// Final source object values at each ordered deletion boundary.
    pub removed: Vec<Unit>,
    pub effects: Vec<Effect>,
}
pub(crate) struct WorldEliminationDraft {
    pub transaction: WorldTransaction,
    pub environment: EnvironmentRuntime,
    pub grid: SpatialGrid,
    pub combat: CombatState,
    pub seed: u32,
    pub next_id: u32,
    pub nodes: Vec<ResourceNode>,
    pub removed: Vec<Unit>,
    pub effects: Vec<Effect>,
    pub survivors: Vec<u32>,
}
impl Match {
    /// Validate the explicit current source collection, distinct from captured tick upkeep.
    pub(crate) fn validate_world_live_order(&self, live_order: &[u32]) -> Result<(), String> {
        let ids: BTreeSet<_> = live_order.iter().copied().collect();
        if ids.len() != live_order.len()
            || ids.len() != self.world.game.units.len()
            || ids.iter().any(|id| !self.world.game.units.contains_key(id))
        {
            return Err("invalid World current unit order".into());
        }
        Ok(())
    }
    pub(crate) fn prepare_world_elimination(
        &self,
        live_order: &[u32],
        regions: &[RegionState],
    ) -> Result<Option<WorldEliminationDraft>, String> {
        self.validate_world_live_order(live_order)?;
        let mut planned = Vec::new();
        // Source repeats this for already-out players too. It does not transfer to a teammate.
        for (slot, player) in self.start.economy.players.iter().enumerate() {
            if !regions.iter().any(|r| r.team == player.team) {
                planned.extend(
                    live_order
                        .iter()
                        .copied()
                        .filter(|id| self.world.game.units[id].owner == slot),
                );
            }
        }
        if planned.is_empty() {
            return Ok(None);
        }
        for id in &planned {
            let unit = &self.world.game.units[id];
            let combat = self
                .combat
                .get(*id)
                .ok_or("missing World elimination combat metadata")?;
            if self.field_works.is_active(*id) != unit.dig {
                return Err("invalid World elimination paid-work projection".into());
            }
            if unit.riding || combat.carrier.is_some() || !combat.cargo.is_empty() {
                return Err("unsupported World elimination: cargo and rider links".into());
            }
        }
        if planned
            .iter()
            .any(|id| self.rules.kinds[self.world.game.units[id].kind].building)
            && self.world.game.units.values().any(|unit| {
                unit.riding
                    || self
                        .combat
                        .get(unit.id)
                        .is_some_and(|m| m.carrier.is_some() || !m.cargo.is_empty())
            })
        {
            return Err("unsupported World building elimination: current cargo and riders".into());
        }
        let mut transaction = self.world.begin_transaction().map_err(|e| e.to_string())?;
        let mut environment = self.environment.clone();
        let mut grid = self.grid.clone_for_transaction();
        let mut combat = self.combat.clone();
        let mut seed = self.start.environment_seed;
        let mut next_id = self.next_unit_id;
        let mut survivors = live_order.to_vec();
        let mut nodes = self
            .world_runtime
            .as_ref()
            .ok_or("missing World runtime")?
            .nodes
            .clone();
        let row_start = transaction.world.new_cells().len();
        let mut removed = Vec::with_capacity(planned.len());
        let mut effects = Vec::new();
        for id in planned {
            // Earlier wrecks may alter this unit. Capture it only at its actual source deletion.
            let unit = transaction
                .world
                .game
                .units
                .remove(&id)
                .ok_or("missing staged World removal")?;
            grid.remove_unit(id);
            survivors.retain(|other| *other != id);
            if self.rules.kinds[unit.kind].building {
                let world = &mut transaction.world;
                let mut context = Context {
                    tick: world.game.tick as u64,
                    seed: &mut seed,
                    next_id: &mut next_id,
                    unit_order: &survivors,
                    grid: &grid,
                    rain: world.game.rain,
                    wet: world.game.wet.unwrap_or(0.0),
                    wind_angle: self.start.wind.0,
                    wind_strength: self.start.wind.1,
                };
                let produced = environment
                    .wreck_building(world, &unit, &mut context)
                    .map_err(|e| e.to_string())?;
                for effect in produced {
                    if let Effect::LastHit { unit, owner } = effect {
                        if !transaction.world.game.units.contains_key(&unit)
                            || owner < 0
                            || owner as usize >= self.start.economy.players.len()
                        {
                            return Err("invalid staged World collapse attribution".into());
                        }
                        combat.initialize(unit).last_hit = Some(owner);
                    } else {
                        effects.push(effect);
                    }
                }
                for node in &mut nodes {
                    if node.depot == Some(id) {
                        node.depot = None;
                    }
                }
            }
            removed.push(unit);
        }
        // Existing rails remain the sole authority for every row emitted by real wreck work.
        let rails = &self.world_runtime.as_ref().unwrap().rails;
        let fields = transaction.world.new_cells()[row_start..]
            .iter()
            .map(|row| {
                (
                    row.cell,
                    rails
                        .rail_at
                        .iter()
                        .find(|(c, _)| *c == row.cell)
                        .map(|(_, v)| *v),
                    rails
                        .station_at
                        .iter()
                        .find(|(c, _)| *c == row.cell)
                        .map(|(_, v)| *v),
                )
            })
            .collect::<Vec<_>>();
        transaction
            .world
            .annotate_recent_rows(row_start, &fields)
            .map_err(|e| e.to_string())?;
        if transaction
            .world
            .game
            .units
            .values()
            .any(|u| !u.hp.is_finite() || !u.x.is_finite() || !u.z.is_finite())
        {
            return Err("invalid post-wreck World unit facts".into());
        }
        Ok(Some(WorldEliminationDraft {
            transaction,
            environment,
            grid,
            combat,
            seed,
            next_id,
            nodes,
            removed,
            effects,
            survivors,
        }))
    }
    /// All fallible preparation has succeeded. This is metadata removal, not combat death.
    pub(crate) fn retire_world_metadata(&mut self, removed: &[Unit]) {
        for unit in removed {
            self.field_works.cancel(unit.id);
            self.commands.units.remove(&unit.id);
            self.commands.requests.retire_unit(unit.id);
            self.firing.remove(unit.id);
            self.abilities.remove(unit.id);
            self.recovery.remove(unit.id);
            self.production.remove_building(unit.id);
            self.building_rallies.remove(&unit.id);
            self.recruit_metadata.remove(&unit.id);
            self.logistics.remove(unit.id);
            self.world_runtime
                .as_mut()
                .unwrap()
                .guard_bases
                .remove(&unit.id);
            // Combat source rows and every flight store remain for their owning cleanup phase.
            // Historical building-cell rows and player research/rallies remain unchanged.
        }
    }
}
