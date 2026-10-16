//! The ordered forced-supply withdrawal turn from the pinned simulation.
use crate::{
    game::{MOVE, Point, Unit},
    match_runtime::Match,
    math,
    navigation::{TerrainView, Versions},
    pathfinding::PathRules,
    unit_spawn::SpawnPlacer,
};

fn distance(a: Point, b: Point) -> f64 {
    math::hypot(a.x - b.x, a.z - b.z)
}

impl Match {
    /// Call after this live unit's reserves advance and before its movement turn.
    /// Field work has one detailed owner; construction/project metadata is not yet represented.
    pub fn advance_supply_withdrawal(&mut self, id: u32) -> Result<(), String> {
        let mut unit = self
            .world
            .game
            .units
            .get(&id)
            .ok_or("missing withdrawal unit")?
            .clone();
        let control = self
            .commands
            .units
            .get(&id)
            .ok_or("missing withdrawal control")?;
        let forced = self.logistics.get(id).is_some_and(|r| r.forced);
        if !forced || unit.riding {
            if control.supply_started && !forced {
                unit.path_gen
                    .checked_add(1)
                    .ok_or("withdrawal path identity exhausted")?;
                let control = self.commands.units.get_mut(&id).unwrap();
                control.supply_started = false;
                control.supply_escape = false;
                control.supply_hold = false;
                unit.retreating = false;
                unit.path.clear();
                unit.path_gen += 1;
                unit.world_goal = None;
                self.world.game.units.insert(id, unit);
            }
            return Ok(());
        }

        // Every fallible shape/binding check precedes cancellation or latch changes.
        self.validate_withdrawal_supply()?;
        for owner in 0..self.start.players.len() {
            self.validate_recipient(owner)?;
        }
        PathRules::compiled().map_err(|e| e.to_string())?;
        unit.path_gen
            .checked_add(2)
            .ok_or("withdrawal path identity exhausted")?;
        let control = &self.commands.units[&id];
        if unit.air.is_some()
            || self.rules.kinds[unit.kind].air
            || (unit.garrison.is_some() && control.supply_started)
        {
            return Err("air or reentered-house withdrawal is not represented".into());
        }
        if control
            .supply_retry
            .is_some_and(|v| v > 9_007_199_254_740_991)
            || unit
                .garrison
                .is_some_and(|c| c >= self.world.game.w * self.world.game.h)
        {
            return Err("invalid withdrawal metadata".into());
        }
        if unit.build || unit.entrench || unit.dig != self.field_works.is_active(id) {
            return Err("unrepresented withdrawal construction or project job".into());
        }
        let stores = self.convoys.as_ref().map(|c| &c.stores);
        if stores.is_some_and(|stores| {
            stores.iter().any(|s| {
                s.owner >= self.start.economy.players.len()
                    || !s.at.x.is_finite()
                    || !s.at.z.is_finite()
            })
        }) {
            return Err("invalid withdrawal store".into());
        }
        let exit = if unit.garrison.is_some() {
            let game = &self.world.game;
            let terrain = TerrainView {
                width: game.w,
                height: game.h,
                flags: &game.flags,
                heights: game.height.as_deref(),
                world_known: true,
                knowledge_id: 0,
                versions: Versions::default(),
                changed: None,
                changes: &[],
            };
            let c = SpawnPlacer::new()
                .nearest_free(terrain, unit.at(), MOVE, None)
                .map_err(|e| format!("withdrawal house exit: {e:?}"))?;
            Some(Point {
                x: (c % game.w) as f64 * 2.0 + 1.0,
                z: (c / game.w) as f64 * 2.0 + 1.0,
            })
        } else {
            None
        };
        let origin = if control.supply_started {
            unit.at()
        } else {
            exit.unwrap_or(unit.at())
        };
        if stores.is_some_and(|stores| {
            stores.iter().any(|store| {
                let length = distance(origin, store.at);
                let d = math::max(0.0, length - 10.0);
                let divisor = if length == 0.0 { 1.0 } else { length };
                !length.is_finite()
                    || !(origin.x + (store.at.x - origin.x) / divisor * d).is_finite()
                    || !(origin.z + (store.at.z - origin.z) / divisor * d).is_finite()
            })
        }) {
            return Err("invalid withdrawal route geometry".into());
        }
        let tick = self.world.game.tick as u64;

        if !self.commands.units[&id].supply_started {
            if let Some(at) = exit {
                unit.x = at.x;
                unit.z = at.z;
                unit.garrison = None;
                self.grid.update(id, &unit);
            }
            crate::match_runtime::cancel_field_work_state(&mut self.field_works, &mut unit)?;
            unit.build = false;
            unit.entrench = false;
            unit.board = 0;
            unit.attack = false;
            let c = self.commands.units.get_mut(&id).unwrap();
            c.supply_started = true;
            c.orders.clear();
            c.nade = None;
            c.fire_at = None;
            c.attack_id = 0;
            c.enter = None;
            self.world.game.units.insert(id, unit.clone());
        }
        if self.commands.units[&id].supply_hold {
            return self.finish_supply_withdrawal(unit, vec![], None, true);
        }
        if self.commands.units[&id].supply_escape
            && (!unit.path.is_empty()
                || !self.commands.units[&id].orders.is_empty()
                || unit.world_goal.is_some())
        {
            let stranded = unit.path.is_empty() || unit.stuck > 1.0;
            unit.retreating = !unit.path.is_empty() && !stranded;
            self.logistics.get_mut(id).unwrap().stranded = stranded;
            self.world.game.units.insert(id, unit);
            return Ok(());
        }
        self.commands.units.get_mut(&id).unwrap().supply_escape = false;
        let reserves = self.logistics.get(id).unwrap();
        if reserves
            .fuel
            .is_some_and(|fuel| fuel + reserves.emergency.unwrap_or(0.0) <= 0.0)
        {
            return self.finish_supply_withdrawal(unit, vec![], None, true);
        }
        if !unit.path.is_empty() {
            let stranded = unit.stuck > 1.0;
            unit.retreating = !stranded;
            self.logistics.get_mut(id).unwrap().stranded = stranded;
            self.world.game.units.insert(id, unit);
            return Ok(());
        }
        if tick < self.commands.units[&id].supply_retry.unwrap_or(0) {
            return Ok(());
        }
        self.commands.units.get_mut(&id).unwrap().supply_retry = Some(tick + 40);
        let team = self.start.economy.players[unit.owner].team;
        let mut stores = self.convoys.as_ref().map_or_else(Vec::new, |c| {
            c.stores
                .iter()
                .filter(|s| {
                    s.active
                        && self.start.economy.players[s.owner].team == team
                        && (s.source || s.buckets.iter().any(|(_, stock)| stock.provisions > 0.0))
                })
                .map(|s| (s.at, s.entity))
                .collect::<Vec<_>>()
        });
        // Stable sorting preserves the source Map insertion order for equal distances.
        crate::sort::sort_by(&mut stores, |(a, _), (b, _)| {
            distance(a, unit.at()) - distance(b, unit.at())
        });
        for (at, entity) in stores {
            let length = distance(at, unit.at());
            let d = math::max(0.0, length - 10.0);
            let divisor = if length == 0.0 { 1.0 } else { length };
            let goal = Point {
                x: unit.x + (at.x - unit.x) / divisor * d,
                z: unit.z + (at.z - unit.z) / divisor * d,
            };
            let path = if length <= 15.0 {
                vec![]
            } else {
                let route = self.command_route(unit.owner, &unit, goal)?;
                self.commands.requests.clear_failures(id);
                self.commands.units.get_mut(&id).unwrap().route_end = Some(route.route_end);
                // Native fine search rejects a disconnected coarse leg before returning waypoints.
                route.waypoints
            };
            if (length <= 15.0 && self.withdrawal_handoff(id, at, entity)?) || !path.is_empty() {
                // The source queries the home path before overriding it with the chosen supply path.
                if unit.garrison.is_some() {
                    let at = exit.expect("withdrawal exit preflight");
                    unit.x = at.x;
                    unit.z = at.z;
                    unit.garrison = None;
                    self.world.game.units.insert(id, unit.clone());
                    self.grid.update(id, &unit);
                }
                self.cancel_field_work(id)?;
                self.retreat_unit(id)?;
                let current = self.world.game.units[&id].clone();
                let destination = (!path.is_empty()).then_some(goal);
                return self.finish_supply_withdrawal(current, path, destination, false);
            }
        }
        self.finish_supply_withdrawal(unit, vec![], None, true)
    }

    fn finish_supply_withdrawal(
        &mut self,
        mut unit: Unit,
        path: Vec<Point>,
        goal: Option<Point>,
        stranded: bool,
    ) -> Result<(), String> {
        unit.retreating = !path.is_empty();
        unit.path = path;
        unit.path_gen += 1;
        unit.world_goal = goal;
        self.logistics
            .get_mut(unit.id)
            .expect("forced withdrawal reserve")
            .stranded = stranded;
        self.world.game.units.insert(unit.id, unit);
        Ok(())
    }
}
