//! Bounded World income and live region capture on the canonical Match.
//! The caller maintains the live grid and supplies the tick's ordered unit IDs.
//! This phase does not advance time or run the rest of a World tick.

use crate::{
    catalog::{Catalog, Value},
    game::{CELL, Point},
    match_runtime::Match,
    match_story::{StoryChange, StoryKey, prepare_story_updates},
    match_world_elimination::WorldRegionEffects,
    match_world_end::WorldFinishUnitFacts,
    match_world_memory::WorldRoutingContext,
    math,
    world_route_retention::prepare_world_route_retention,
};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorldCapture {
    pub region_index: usize,
    pub owner: usize,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum ProductionLock {
    Guard,
    Team(i32),
}

fn n(value: &Value, key: &str) -> Result<f64, String> {
    value
        .get(key)
        .and_then(Value::number)
        .filter(|v| v.is_finite())
        .ok_or_else(|| format!("missing World phase rule {key}"))
}
fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::Number(v) => *v != 0.0 && !v.is_nan(),
        Value::String(v) => !v.is_empty(),
        _ => true,
    }
}

impl Match {
    /// Source income precedes region capture. Prepare every consequence before publishing.
    /// Unsupported fort transfer and army elimination leave the whole phase intact. Eligible
    /// source finish is prepared after those preflights, then published on canonical state.
    pub fn apply_world_capture_income(
        &mut self,
        unit_order: &[u32],
        dt: f64,
    ) -> Result<Vec<WorldCapture>, String> {
        Ok(self
            .world_region_phase(Some(unit_order), None, dt)?
            .captures)
    }
    /// Run the bounded phase with actual administrative removals and building wrecks.
    /// The complete current order is distinct from the captured tick upkeep list.
    /// Unsupported fort, logistics and rider consequences leave every owner unchanged.
    pub fn apply_world_capture_income_and_elimination(
        &mut self,
        tick_order: &[u32],
        live_order: &[u32],
        dt: f64,
    ) -> Result<WorldRegionEffects, String> {
        self.world_region_phase(Some(tick_order), Some(live_order), dt)
    }
    /// Run the current region, home, elimination and finish phase without charging income.
    /// Invoke this after the real intervening convoy phase. This bounded API still
    /// refuses World logistics and unsupported consequences; it is not a tick runner.
    pub fn apply_world_regions(
        &mut self,
        live_order: &[u32],
        dt: f64,
    ) -> Result<WorldRegionEffects, String> {
        self.world_region_phase(None, Some(live_order), dt)
    }
    fn world_region_phase(
        &mut self,
        income_order: Option<&[u32]>,
        live_order: Option<&[u32]>,
        dt: f64,
    ) -> Result<WorldRegionEffects, String> {
        if let Some(order) = live_order {
            self.validate_world_live_order(order)?;
        }
        let context = self.validate_world_phase_context(dt)?;
        let mut wallets = if let Some(order) = income_order {
            let captured = self.current_world_income_units(order)?;
            self.prepare_world_income(&captured, dt)?
        } else {
            self.start.economy.clone()
        };
        let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
        let geography = self.world.geography().ok_or("missing World geography")?;
        let game = &self.world.game;
        let players = &self.start.economy.players;
        let runtime = self.world_runtime.as_ref().ok_or("missing World runtime")?;

        let cfg = catalog.table("CFG").map_err(|e| e.to_string())?;
        let radius = n(cfg, "pointRadius")?;
        let capture_time = n(cfg, "captureTime")?;
        if capture_time <= 0.0 || radius < 0.0 {
            return Err("invalid World capture rules".into());
        }
        let rate = dt / capture_time;
        let mut production = vec![BTreeSet::new(); runtime.regions.len()];
        for u in game.units.values().filter(|u| u.hp > 0.0) {
            let name = &self.rules.kinds[u.kind].name;
            let definition = catalog.unit(name).map_err(|e| e.to_string())?;
            if !definition.get("produces").is_some_and(truthy) {
                continue;
            }
            if let Some(index) = self.world_region_index(u.at())? {
                production[index].insert(if name == "worldbase" {
                    ProductionLock::Guard
                } else {
                    ProductionLock::Team(game.team_of(u))
                });
            }
        }
        let mut regions = runtime.regions.clone();
        let mut captured = Vec::new();
        for (index, (r, definition)) in regions.iter_mut().zip(&geography.regions).enumerate() {
            if !r.progress.is_finite()
                || !(0.0..=1.0).contains(&r.progress)
                || !(-1..6).contains(&r.team)
                || r.capper.is_some_and(|s| s >= players.len())
                || r.cap_tick
                    .is_some_and(|t| !(-9_007_199_254_740_991..=9_007_199_254_740_991).contains(&t))
            {
                return Err("invalid World region state".into());
            }
            let at = Point {
                x: (definition.x as f64 + 0.5) * CELL,
                z: (definition.y as f64 + 0.5) * CELL,
            };
            let on = self
                .grid
                .in_order(&game.units, (at.x, at.z), radius, |_, u| {
                    u.hp > 0.0
                        && !u.retreating
                        && !u.riding
                        && self.rules.kinds[u.kind].infantry
                        && !self
                            .logistics
                            .get(u.id)
                            .is_some_and(|reserve| reserve.forced)
                        && math::hypot(u.x - at.x, u.z - at.z) <= radius
                });
            let team = on.first().map_or(-1, |id| game.team_of(&game.units[id]));
            r.contested = on.iter().any(|id| game.team_of(&game.units[id]) != team);
            let locked = production[index].contains(&ProductionLock::Guard)
                || production[index].contains(&ProductionLock::Team(r.team));
            r.locked = Some(locked);
            if team < 0 || on.is_empty() || r.contested || (r.team != team && locked) {
                continue;
            }
            let owner = game.units[&on[0]].owner;
            if r.team == team {
                r.progress = 1.0;
                r.capper = None;
            } else if r.team >= 0 {
                r.progress = math::max(0.0, r.progress - rate);
                if r.progress == 0.0 {
                    r.team = -1;
                    r.capper = Some(owner);
                }
            } else {
                if r.capper.is_none_or(|s| players[s].team != team) {
                    r.capper = Some(owner);
                    r.progress = 0.0;
                }
                r.progress = math::min(1.0, r.progress + rate);
                if r.progress >= 1.0 {
                    r.team = team;
                    r.capper = None;
                    if !context.decided {
                        r.cap_tick = Some(game.tick);
                    }
                    for fort in game.units.values().filter(|u| {
                        u.hp > 0.0
                            && ["pillbox", "tower", "wall", "gate"]
                                .contains(&self.rules.kinds[u.kind].name.as_str())
                    }) {
                        if !game.allied(fort.owner, owner)
                            && self.world_region_index(fort.at())? == Some(index)
                        {
                            return Err(
                                "unsupported World capture: fort ownership and gate state".into()
                            );
                        }
                    }
                    captured.push(WorldCapture {
                        region_index: index,
                        owner,
                    });
                }
            }
        }
        let mut spawns: Vec<_> = self.start.players.iter().map(|p| p.spawn).collect();
        for (slot, wallet) in wallets.players.iter_mut().enumerate() {
            let own: Vec<_> = regions
                .iter()
                .enumerate()
                .filter(|(_, r)| r.team == wallet.team)
                .collect();
            if own.is_empty() {
                if live_order.is_none() && game.units.values().any(|u| u.owner == slot) {
                    return Err(
                        "unsupported World phase: army elimination and terrain wrecking".into(),
                    );
                }
                wallet.out = true;
            } else if self
                .world_region_index(spawns[slot])?
                .is_none_or(|i| regions[i].team != wallet.team)
            {
                let mut best: Option<(Point, f64)> = None;
                for (index, _) in own {
                    let r = &geography.regions[index];
                    let at = Point {
                        x: (r.x as f64 + 0.5) * CELL,
                        z: (r.y as f64 + 0.5) * CELL,
                    };
                    let distance = math::hypot(at.x - spawns[slot].x, at.z - spawns[slot].z);
                    if best.is_none_or(|(_, d)| distance < d) {
                        best = Some((at, distance));
                    }
                }
                spawns[slot] = best.expect("owned region").0;
            }
        }
        let draft = live_order
            .map(|order| self.prepare_world_elimination(order, &regions))
            .transpose()?
            .flatten();
        let finish = if let Some(draft) = &draft {
            let facts = draft
                .survivors
                .iter()
                .map(|id| {
                    let unit = &draft.transaction.world.game.units[id];
                    WorldFinishUnitFacts {
                        id: *id,
                        owner: unit.owner,
                        kind: unit.kind,
                        hp: unit.hp,
                    }
                })
                .collect::<Vec<_>>();
            let removed = draft.removed.iter().map(|u| u.id).collect::<Vec<_>>();
            self.prepare_world_finish_after_removal(&wallets, &regions, &facts, &removed)?
        } else {
            self.prepare_world_finish(&wallets, &regions)?
        };
        let changes: Vec<_> = captured
            .iter()
            .map(|event| StoryChange {
                slot: Some(event.owner),
                key: StoryKey::Captures,
                amount: 1.0,
            })
            .collect();
        let mut report = WorldRegionEffects {
            captures: captured,
            ..Default::default()
        };
        if let Some(draft) = draft {
            let runtime = self.world_runtime.as_mut().expect("checked World runtime");
            let final_units = draft.removed.iter().collect::<Vec<_>>();
            let retention = prepare_world_route_retention(
                WorldRoutingContext {
                    world: &self.world,
                    rules: &self.rules,
                    start: &self.start,
                    rails: &runtime.rails,
                    regions: &runtime.regions,
                    known_buildings: &runtime.known_buildings,
                    local_defender: &runtime.local_defender,
                },
                &mut runtime.memory,
                &final_units,
            )?;
            let publication = draft
                .transaction
                .prepare_publication(&mut self.world)
                .map_err(|e| e.to_string())?;
            let story = prepare_story_updates(&mut self.story, context.decided, &changes)?;
            // Every fallible guard has succeeded. Publish the single canonical owners in place.
            publication.commit();
            self.environment = draft.environment;
            self.grid = draft.grid;
            self.combat = draft.combat;
            self.start.environment_seed = draft.seed;
            self.next_unit_id = draft.next_id;
            runtime.nodes = draft.nodes;
            runtime.regions = regions;
            retention.commit();
            self.start.economy = wallets;
            for (player, at) in self.start.players.iter_mut().zip(spawns) {
                player.spawn = at;
            }
            story.commit();
            self.retire_world_metadata(&draft.removed);
            report.removed = draft.removed;
            report.effects = draft.effects;
        } else {
            let story = prepare_story_updates(&mut self.story, context.decided, &changes)?;
            self.world_runtime
                .as_mut()
                .expect("checked World runtime")
                .regions = regions;
            self.start.economy = wallets;
            for (player, at) in self.start.players.iter_mut().zip(spawns) {
                player.spawn = at;
            }
            story.commit();
        }
        if let Some(finish) = finish {
            self.publish_match_finish(finish);
        }
        Ok(report)
    }
}
