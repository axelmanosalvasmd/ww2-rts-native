//! Dedicated Classic Depot command adapter. Ordinary construction tick admission
//! remains separate; this provider does not admit a full Classic runner.
use super::*;
use crate::{game::Point, match_commands::Denial};

// Waiting commands are emitted by the existing Move/Assist consumers. Keep
// their one-actor shape, finite coordinates and nonqueued playback contract.
fn classic_waiting_order(id: UnitId, command: &crate::match_commands::Command) -> bool {
    use crate::match_commands::Command;
    match command {
        Command::Move {
            orders,
            control_ids,
            attack_move,
            queue,
            together,
            face,
        } => {
            control_ids.is_none()
                && !attack_move
                && !queue
                && !together
                && orders.len() == 1
                && orders[0].unit == i64::from(id)
                && orders[0].at.x.is_finite()
                && orders[0].at.z.is_finite()
                && face.is_none_or(|angle| angle.is_finite())
        }
        // Shared construction validation checks the actual actor/site/anchor shape.
        Command::Assist { .. } => true,
        _ => false,
    }
}

impl Match {
    pub(crate) fn validate_classic_depot_components(&self) -> Result<(), String> {
        self.validate_classic_depot_context()?;
        self.validate_construction_orders()?;
        if self.options.logistics || self.convoys.is_some() {
            return Err("Classic Depot components do not execute reserves or convoys".into());
        }
        for (&id, unit) in &self.world.game.units {
            let kind = self
                .rules
                .kinds
                .get(unit.kind)
                .ok_or("invalid Depot component kind")?;
            let control = &self.commands.units[&id];
            let firing = self
                .firing
                .get(id)
                .ok_or("missing Depot component firing metadata")?;
            if unit.hp <= 0.
                || kind.air
                || kind.rail
                || unit.riding
                || unit.garrison.is_some()
                || unit.retreating
                || unit.dig
                || unit.entrench
                || unit.attack
                || unit.amove.is_some()
                || unit.board != 0
                || control.operative
                || control.orders.len() > 8
                || control.attack_id != 0
                || control
                    .orders
                    .iter()
                    .any(|order| !classic_waiting_order(id, order))
                || firing.target_id != 0
                || control.nade.is_some()
                || control.enter.is_some()
                || control.fire_at.is_some()
                || ![
                    unit.hp,
                    unit.x,
                    unit.z,
                    unit.rot,
                    unit.repath,
                    unit.sprint,
                    unit.supp,
                ]
                .iter()
                .all(|n| n.is_finite())
                || unit
                    .path
                    .iter()
                    .any(|p| !p.x.is_finite() || !p.z.is_finite())
                || unit
                    .world_goal
                    .is_some_and(|p| !p.x.is_finite() || !p.z.is_finite())
                || control
                    .route_end
                    .is_some_and(|p| !p.x.is_finite() || !p.z.is_finite())
                || (kind.building
                    && unit.built.is_some_and(|b| b < 1.)
                    && !matches!(kind.name.as_str(), "depot" | "barracks"))
            {
                return Err("unrepresented Classic Depot component activity".into());
            }
        }
        Ok(())
    }

    /// Existing movement and crew owners, at their actual early/late positions.
    /// Combat and projects remain outside this caller. Normalized Move and Assist
    /// use the existing sole queue owner before this actor's worker turn.
    pub(crate) fn step_classic_depot_construction(&mut self) -> Result<(), String> {
        self.validate_classic_depot_components()?;
        let dt = self.rules.movement.tick;
        let config = crate::match_tick::TickPathConfig::default();
        self.begin_path_tick(config.budget, config.retry_limit)?;
        let mut frame = ConstructionFrame {
            tick: self.world.game.tick,
            sites: Vec::new(),
            indexes: BTreeMap::new(),
            workers: BTreeSet::new(),
        };
        let ids = self.world.game.units.keys().copied().collect::<Vec<_>>();
        let mut moved = Vec::new();
        for id in ids {
            self.prepare_unit_commands(id, self.finish.is_some())?;
            let unit = &self.world.game.units[&id];
            if !crate::movement::reaches_block(&self.rules, unit) {
                continue;
            }
            let factor = crate::movement::supp_speed(unit.supp);
            let rally = if crate::vision::in_trench(&self.world.game, &self.rules, unit) {
                self.rules.movement.trench_rally
            } else {
                8.
            };
            let infantry = self.rules.kinds[unit.kind].infantry;
            let unit = self.world.game.units.get_mut(&id).unwrap();
            if infantry {
                unit.supp = math::max(0., unit.supp - rally * dt);
            }
            unit.repath -= dt;
            unit.sprint -= dt;
            self.step_construction_worker_in_context(id, &mut frame)?;
            if let Some(result) = self.move_unit_with_command_routes(id, factor)? {
                moved.push(result);
            }
        }
        self.finish_construction_in_context(frame)?;
        crate::separation::separate_crowds(&mut self.world.game, &self.rules, &mut self.grid);
        crate::movement::infantry_progress(&mut self.world.game, &self.rules, &moved);
        Ok(())
    }

    pub(crate) fn validate_classic_depot_context(&self) -> Result<(), String> {
        self.validate_classic_base_state()?;
        let count = self.start.players.len();
        let game = &self.world.game;
        let cells = game
            .w
            .checked_mul(game.h)
            .ok_or("invalid Classic Depot map dimensions")?;
        if !(1..=6).contains(&count)
            || game.w == 0
            || game.h == 0
            || game.flags.len() != cells
            || game.chars.len() != cells
            || game.cell_hp.len() != cells
            || game.house.len() != cells
            || game.height.as_ref().is_some_and(|h| h.len() != cells)
            || !(0..9_007_199_254_740_351).contains(&game.tick)
            || count != self.options.players.len()
            || count != self.player_rallies.len()
            || self.start.dimensions != (self.world.game.w, self.world.game.h)
            || self.world.game.players.iter().enumerate().any(|(slot, p)| {
                p.slot != slot
                    || !(0..6).contains(&p.team)
                    || p.team != self.start.players[slot].config.team
                    || p.team != self.options.players[slot].team
                    || p.team != self.start.economy.players[slot].team
            })
        {
            return Err("invalid Classic Depot commander context".into());
        }
        self.validate_building_activity_rows()?;
        self.validate_routing_context()?;
        let state = self
            .classic_runtime
            .as_ref()
            .ok_or("missing Classic runtime")?;
        let mut cells = BTreeSet::new();
        for node in &state.nodes {
            if !cells.insert(node.cell)
                || crate::match_runtime::footprint(&self.world.game, node.cell, 2).is_none()
                || node.at != crate::match_runtime::foot_center(&self.world.game, node.cell, 2)
            {
                return Err("invalid Classic Depot node identity or geometry".into());
            }
            if let Some(id) = node.depot {
                let site = self
                    .world
                    .game
                    .units
                    .get(&id)
                    .ok_or("stale Classic Depot node binding")?;
                if self
                    .rules
                    .kinds
                    .get(site.kind)
                    .is_none_or(|k| k.name != "depot")
                    || site.at() != node.at
                    || site.owner >= count
                {
                    return Err("invalid Classic Depot live binding".into());
                }
            }
        }
        Ok(())
    }

    /// Source stable nearest-free selection. Equal distances retain original
    /// node order; an occupied node is never overwritten by another paid site.
    pub(super) fn classic_depot_node(
        &mut self,
        owner: usize,
        at: Point,
    ) -> Result<Result<usize, Denial>, String> {
        self.validate_classic_depot_context()?;
        let nodes = &self
            .classic_runtime
            .as_ref()
            .ok_or("missing Classic runtime")?
            .nodes;
        let mut nearest = None;
        let mut best = f64::INFINITY;
        for (index, node) in nodes.iter().enumerate() {
            let d = math::hypot(node.at.x - at.x, node.at.z - at.z);
            if node.depot.is_none() && d <= 8.0 && d < best {
                nearest = Some(index);
                best = d;
            }
        }
        if let Some(node) = nearest {
            return Ok(Ok(node));
        }
        let nearby = nodes
            .iter()
            .filter(|n| math::hypot(n.at.x - at.x, n.at.z - at.z) <= 8.0)
            .map(|n| n.at)
            .collect::<Vec<_>>();
        for center in nearby {
            if !self.recipient_sees(owner, center)? {
                return Ok(Err(Denial::NotVisible));
            }
        }
        Ok(Err(Denial::Blocked))
    }

    /// Source cancel is administrative removal, without death, rubble or loss tallies.
    pub(crate) fn execute_classic_site_cancel(
        &mut self,
        owner: usize,
        id: Option<u32>,
    ) -> Result<Option<Denial>, String> {
        // Source looks up cmd.id directly, with no selected-id fallback or mode gate.
        // This is a refusal only; physical teardown still needs its own provider.
        let Some(id) = id else {
            return Ok(Some(Denial::Unseen));
        };
        let Some(site) = self.world.game.units.get(&id) else {
            return Ok(Some(Denial::Unseen));
        };
        let kind = self
            .rules
            .kinds
            .get(site.kind)
            .ok_or("invalid cancellation kind")?;
        if site.owner != owner || !kind.building || site.built.is_some_and(|b| b >= 1.) {
            return Ok(Some(Denial::Unseen));
        }
        if self.options.mode != Mode::Classic {
            return Ok(Some(Denial::Unsupported));
        }
        self.validate_classic_depot_context()?;
        // Only the represented canonical Depot and Barracks lifecycles are admitted here.
        if !matches!(kind.name.as_str(), "depot" | "barracks") {
            return Ok(Some(Denial::Unsupported));
        }
        if site
            .built
            .is_none_or(|b| !b.is_finite() || !(0.0..1.0).contains(&b))
        {
            return Err("invalid cancelled site progress".into());
        }
        let cells = site
            .cells
            .as_ref()
            .ok_or("missing cancelled footprint")?
            .iter()
            .map(|c| *c as usize)
            .collect::<Vec<_>>();
        let definition = Catalog::compiled()
            .map_err(|e| e.to_string())?
            .unit(&kind.name)
            .map_err(|e| e.to_string())?;
        let refund = number(definition, "cost")? * 0.75;
        let wallet = self
            .start
            .economy
            .players
            .get(owner)
            .ok_or("missing cancellation wallet")?
            .mp;
        if !wallet.is_finite()
            || !refund.is_finite()
            || refund < 0.
            || !(wallet + refund).is_finite()
        {
            return Err("invalid cancellation wallet/refund".into());
        }
        let mut physical = self.world.begin_transaction().map_err(|e| e.to_string())?;
        let sections = physical
            .world
            .cancel_building_structure(id, &cells)
            .map_err(|e| e.to_string())?;
        physical.world.game.units.remove(&id);
        let mut grid = self.grid.clone_for_transaction();
        grid.remove_unit(id);
        let physical = physical
            .prepare_publication(&mut self.world)
            .map_err(|e| e.to_string())?;
        let story = crate::match_story::prepare_story_updates(
            &mut self.story,
            self.finish.is_some(),
            &[crate::match_story::StoryChange {
                slot: Some(owner),
                key: crate::match_story::StoryKey::MpSpent,
                amount: -refund,
            }],
        )?;
        // No fallible work follows. Worker targets remain until their real next turn.
        physical.commit();
        self.grid = grid;
        self.environment.forget_building_sections(&sections);
        self.commands.units.remove(&id);
        self.commands.requests.retire_unit(id);
        self.field_works.cancel(id);
        self.firing.remove(id);
        self.abilities.remove(id);
        self.recovery.remove(id);
        self.production.remove_building(id);
        self.building_rallies.remove(&id);
        self.recruit_metadata.remove(&id);
        self.logistics.remove(id);
        for node in &mut self
            .classic_runtime
            .as_mut()
            .expect("validated Classic runtime")
            .nodes
        {
            if node.depot == Some(id) {
                node.depot = None;
            }
        }
        self.start.economy.players[owner].mp = wallet + refund;
        story.commit();
        Ok(None)
    }
}

#[cfg(test)]
#[path = "classic_depot_tests.rs"]
mod tests;
