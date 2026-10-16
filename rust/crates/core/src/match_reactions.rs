//! Ground-unit reactions over the Match's canonical control, firing and combat state.
use crate::{
    catalog::{Catalog, Value},
    command_destinations::{self, Context},
    economy::Mode,
    game::{Player, Point, Unit},
    grid::UnitId,
    match_commands::MoveOutcome,
    match_runtime::Match,
    math,
    rules::Rules,
    terrain,
};
use std::collections::{BTreeMap, BTreeSet};

fn number(v: &Value, key: &str) -> Result<f64, String> {
    v.get(key)
        .and_then(Value::number)
        .filter(|n| n.is_finite())
        .ok_or_else(|| format!("invalid reaction rule {key}"))
}
fn distance(a: Point, b: Point) -> f64 {
    math::hypot(a.x - b.x, a.z - b.z)
}

// World has its own region-aware home selector. These modes use local buildings.
fn ground_home(
    mode: Mode,
    unit: &Unit,
    units: &BTreeMap<UnitId, Unit>,
    rules: &Rules,
    spawn: Point,
    players: &[Player],
    horde_slot: Option<usize>,
) -> Result<Point, String> {
    if !mode.is_skirmish() && mode != Mode::Classic {
        return Ok(spawn);
    }
    let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
    let mut home = spawn;
    let mut best = f64::INFINITY;
    for building in units.values() {
        let shared_horde_home = mode == Mode::Horde
            && horde_slot.is_some_and(|slot| unit.owner != slot && building.owner != slot)
            && players
                .get(unit.owner)
                .zip(players.get(building.owner))
                .is_some_and(|(actor, owner)| actor.team == owner.team);
        if (building.owner != unit.owner && !shared_horde_home)
            || building.hp <= 0.0
            || !building.built.is_some_and(|b| b >= 1.0)
        {
            continue;
        }
        let kind = rules
            .kinds
            .get(building.kind)
            .ok_or("unknown retreat building")?;
        let eligible = if mode == Mode::Classic {
            catalog
                .unit(&kind.name)
                .map_err(|e| e.to_string())?
                .get("produces")
                .is_some_and(|p| !matches!(p, Value::Bool(false) | Value::Null))
        } else {
            kind.name == "hq"
        };
        if eligible {
            let d = distance(unit.at(), building.at());
            if d < best {
                best = d;
                home = building.at();
            }
        }
    }
    Ok(home)
}

impl Match {
    /// Decrement in the valid ground-unit timer block, before automatic retreat.
    pub fn decrement_reaction_timer(&mut self, id: UnitId, dt: f64) -> Result<(), String> {
        if !dt.is_finite() || dt < 0.0 {
            return Err("invalid reaction elapsed time".into());
        }
        let control = self
            .commands
            .units
            .get_mut(&id)
            .ok_or("missing reaction control")?;
        let next = control.react - dt;
        if !next.is_finite() {
            return Err("invalid reaction timer".into());
        }
        control.react = next;
        Ok(())
    }

    /// Select the mode's finished home, including shared allied Horde HQs.
    pub fn reaction_home(&self, id: UnitId) -> Result<Point, String> {
        let unit = self
            .world
            .game
            .units
            .get(&id)
            .ok_or("missing retreat unit")?;
        self.reaction_home_for(unit)
    }

    pub(crate) fn reaction_home_for(&self, unit: &Unit) -> Result<Point, String> {
        if self.uses_world_routes() {
            return self.world_ground_home_for(unit);
        }
        let spawn = self
            .start
            .players
            .get(unit.owner)
            .ok_or("missing retreat owner")?
            .spawn;
        ground_home(
            self.options.mode,
            unit,
            &self.world.game.units,
            &self.rules,
            spawn,
            &self.world.game.players,
            self.horde.as_ref().map(|horde| horde.slot),
        )
    }

    /// Source retreat reset. Air, carrier and building exit need their own actuator.
    pub fn retreat_unit(&mut self, id: UnitId) -> Result<(), String> {
        let u = self
            .world
            .game
            .units
            .get(&id)
            .ok_or("missing retreat unit")?
            .clone();
        if u.air.is_some() || u.riding || u.garrison.is_some() {
            return Err("air, carrier or garrison retreat is unsupported".into());
        }
        if u.id != id
            || u.dig != self.field_works.is_active(id)
            || u.entrench != self.field_works.membership(id).is_some()
        {
            return Err("unrepresented or inconsistent retreat field work".into());
        }
        let control = self
            .commands
            .units
            .get(&id)
            .ok_or("missing retreat control")?;
        crate::match_construction::validate_build_activity(&u, control)?;
        u.path_gen
            .checked_add(1)
            .ok_or("reaction path identity exhausted")?;
        let home = self.reaction_home(id)?;
        let route = self.command_route(u.owner, &u, home)?;
        self.apply_retreat_route(u, home, route)?;
        Ok(())
    }

    /// Commit a preflighted route through the sole retreat reset authority.
    /// A manual command may supply an already prepared physical building exit.
    pub(crate) fn apply_retreat_route(
        &mut self,
        mut u: Unit,
        home: Point,
        route: crate::pathfinding::PathResult,
    ) -> Result<(), String> {
        let id = u.id;
        crate::match_runtime::cancel_owned_work_state(
            &mut self.field_works,
            &mut u,
            self.commands.units.get_mut(&id).unwrap(),
        )?;
        // The source retreat reset assigns targetId directly; its existing claim
        // remains until the next tick's claim rebuild.
        self.firing.initialize(id).target_id = 0;
        u.traffic = None;
        u.traffic_wait = 0.0;
        u.world_goal = Some(home);
        u.entrench = false;
        u.retreating = true;
        u.attack = false;
        u.dig = false;
        u.stuck = 0.0;
        u.board = 0;
        u.amove = None;
        u.build = false;
        u.traffic_entered = false;
        u.path = route.waypoints;
        u.path_gen = u
            .path_gen
            .checked_add(1)
            .ok_or("reaction path identity exhausted")?;
        let c = self
            .commands
            .units
            .get_mut(&id)
            .expect("retreat control checked");
        c.route_end = Some(route.route_end);
        c.orders.clear();
        c.face = None;
        c.attack_id = 0;
        c.nade = None;
        c.enter = None;
        c.fire_at = None;
        c.outcome = Some(MoveOutcome::Interrupted);
        c.outcome_tick = Some(self.world.game.tick);
        self.world.game.units.insert(id, u);
        self.commands.requests.clear_failures(id);
        Ok(())
    }

    /// Read the same source threshold/home predicate before a tick admits an exit.
    pub fn automatic_retreat_candidate(&self, id: UnitId) -> Result<bool, String> {
        let u = self
            .world
            .game
            .units
            .get(&id)
            .ok_or("missing automatic retreat unit")?;
        let c = self
            .commands
            .units
            .get(&id)
            .ok_or("missing automatic retreat control")?;
        let kind = self
            .rules
            .kinds
            .get(u.kind)
            .ok_or("unknown automatic retreat kind")?;
        let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
        let def = catalog.unit(&kind.name).map_err(|e| e.to_string())?;
        let cfg = catalog.table("CFG").map_err(|e| e.to_string())?;
        if !c.auto_retreat
            || u.retreating
            || kind.speed <= 0.0
            || u.hp >= number(def, "models")? * number(def, "hpPer")? * number(cfg, "autoRetreat")?
            || distance(u.at(), self.reaction_home(id)?) <= number(cfg, "reinforceRadius")?
        {
            return Ok(false);
        }
        Ok(true)
    }

    /// Run after the ground-unit timers and before autocast, as in the source loop.
    pub fn automatic_retreat_unit(&mut self, id: UnitId) -> Result<bool, String> {
        if !self.automatic_retreat_candidate(id)? {
            return Ok(false);
        }
        self.retreat_unit(id)?;
        Ok(true)
    }

    /// Run after ordered jobs and before movement. No random draw is needed.
    pub fn react_unit(&mut self, id: UnitId) -> Result<(), String> {
        let u = self
            .world
            .game
            .units
            .get(&id)
            .ok_or("missing reacting unit")?
            .clone();
        let c = self
            .commands
            .units
            .get(&id)
            .ok_or("missing reaction control")?;
        let kind = self
            .rules
            .kinds
            .get(u.kind)
            .ok_or("unknown reaction kind")?;
        if ![u.x, u.z, u.hp, u.rot, c.react]
            .iter()
            .all(|v| v.is_finite())
            || (self.options.mode != crate::economy::Mode::World
                && u.owner >= self.world.game.players.len())
            || u.hit_from
                .is_some_and(|p| !p.x.is_finite() || !p.z.is_finite())
            || self.combat.get(id).is_some_and(|h| !h.hit_at.is_finite())
        {
            return Err("invalid canonical reaction state".into());
        }
        if self.options.mode == crate::economy::Mode::World {
            self.actor_policy(u.owner)?;
        }
        if kind.structure || self.logistics.get(id).is_some_and(|r| r.forced) {
            return Ok(());
        }
        if u.air.is_some() || u.riding || kind.rail {
            return Err("air, carrier or rail reaction phase is unsupported".into());
        }
        if u.hold_pos
            || !c.orders.is_empty()
            || u.entrench
            || u.retreating
            || u.garrison.is_some()
            || c.attack_id != 0
            || u.amove.is_some()
            || c.nade.is_some()
            || u.dig
            || u.build
            || c.enter.is_some()
            || u.board != 0
            || c.fire_at.is_some()
            || u.world_goal.is_some() && u.path.is_empty()
        {
            return Ok(());
        }
        self.validate_reaction_neighbors(&u)?;
        u.path_gen
            .checked_add(1)
            .ok_or("reaction path identity exhausted")?;
        let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
        let cfg = catalog.table("CFG").map_err(|e| e.to_string())?;
        let behavior = cfg.get("behavior").ok_or("missing reaction behavior")?;
        let hit = self.combat.get(id);
        let fresh = self.world.game.tick as f64 - hit.map_or(-1e9, |h| h.hit_at)
            <= number(behavior, "threatTime")? / self.rules.movement.tick
            && u.hit_from.is_some();
        if !fresh {
            if u.path.is_empty() && kind.infantry {
                self.keep_unit_spacing(id)?;
            }
            return Ok(());
        }
        let shooter = hit.and_then(|h| h.hit_by);
        let answer = !c.hold_fire
            && shooter.is_some_and(|s| self.world.game.units.contains_key(&s))
            && self.can_shoot_target(id, shooter.unwrap_or(0))?;
        if !u.path.is_empty() {
            if u.drift && c.rally_drift && answer {
                self.set_firing_target(id, shooter.expect("answer has shooter"))?;
                let live = self.world.game.units.get_mut(&id).expect("reaction unit");
                live.amove = u.path.last().copied();
                live.path.clear();
                live.path_gen = live
                    .path_gen
                    .checked_add(1)
                    .expect("reaction path preflight");
                live.drift = false;
                self.commands
                    .units
                    .get_mut(&id)
                    .expect("reaction control")
                    .rally_drift = false;
            }
            return Ok(());
        }
        if c.react > 0.0 {
            return Ok(());
        }
        let mut next_react = 2.0;
        let from = u.hit_from.expect("fresh fire origin");
        if kind.infantry {
            let def = catalog.unit(&kind.name).map_err(|e| e.to_string())?;
            let setup = def
                .get("w")
                .and_then(|w| w.get("setup"))
                .and_then(Value::number)
                .is_some_and(|v| v != 0.0);
            let rank = terrain::cover_rank(&self.world.game, &self.rules, u.at(), Some(from));
            if setup || rank == 0 {
                self.commands
                    .units
                    .get_mut(&id)
                    .expect("reaction control")
                    .react = next_react;
                return Ok(());
            }
            let radius = if answer {
                number(behavior, "coverShift")?
            } else {
                number(cfg, "coverSeek")?
            };
            if let Some(spot) = self.reaction_spot(&u, radius, Some(from), rank, false)? {
                self.assign_reaction_route(u, spot, false)?;
            }
        } else {
            let def = catalog.unit(&kind.name).map_err(|e| e.to_string())?;
            let point_radius = number(cfg, "pointRadius")?;
            if !answer
                && u.hp < number(def, "models")? * number(def, "hpPer")? * 0.5
                && !self
                    .start
                    .points
                    .iter()
                    .any(|p| distance(p.at, u.at()) <= point_radius)
            {
                let angle = math::atan2(u.z - from.z, u.x - from.x);
                let back = number(behavior, "pullBack")?;
                let spot = Point {
                    x: u.x + math::cos(angle) * back,
                    z: u.z + math::sin(angle) * back,
                };
                self.assign_reaction_route(u, spot, true)?;
                next_react = 10.0;
            }
        }
        self.commands
            .units
            .get_mut(&id)
            .expect("reaction control")
            .react = next_react;
        Ok(())
    }

    fn validate_reaction_neighbors(&self, u: &Unit) -> Result<(), String> {
        let hit_at: BTreeMap<_, _> = self.combat.iter().map(|(id, c)| (id, c.hit_at)).collect();
        let points: Vec<_> = self.start.points.iter().map(|p| p.at).collect();
        let context = Context {
            game: &self.world.game,
            rules: &self.rules,
            grid: &self.grid,
            points: &points,
            hit_at: &hit_at,
        };
        command_destinations::held_destinations(&context, &BTreeSet::new(), u.at(), 0.0, u.owner)
            .map(|_| ())
            .map_err(|e| format!("reaction state: {e:?}"))
    }

    fn reaction_spot(
        &self,
        u: &Unit,
        radius: f64,
        from: Option<Point>,
        beat: u8,
        open_fallback: bool,
    ) -> Result<Option<Point>, String> {
        let hit_at: BTreeMap<_, _> = self.combat.iter().map(|(id, c)| (id, c.hit_at)).collect();
        let points: Vec<_> = self.start.points.iter().map(|p| p.at).collect();
        let context = Context {
            game: &self.world.game,
            rules: &self.rules,
            grid: &self.grid,
            points: &points,
            hit_at: &hit_at,
        };
        let held = command_destinations::held_destinations(
            &context,
            &BTreeSet::from([u.id]),
            u.at(),
            radius,
            u.owner,
        )
        .map_err(|e| format!("reaction held spots: {e:?}"))?;
        let spot = command_destinations::spot_near(
            &context,
            u.id,
            u.at(),
            radius,
            &held,
            from,
            beat,
            None,
        )
        .map_err(|e| format!("reaction spot: {e:?}"))?;
        if spot.is_some() || !open_fallback {
            return Ok(spot);
        }
        command_destinations::spot_near(&context, u.id, u.at(), radius, &held, None, 4, None)
            .map_err(|e| format!("reaction open spot: {e:?}"))
    }

    fn keep_unit_spacing(&mut self, id: UnitId) -> Result<(), String> {
        let u = self.world.game.units[&id].clone();
        if self.commands.units[&id].react > 0.0
            || self.firing.get(id).is_some_and(|f| f.target_id != 0)
        {
            return Ok(());
        }
        let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
        let behavior = catalog
            .table("CFG")
            .map_err(|e| e.to_string())?
            .get("behavior")
            .ok_or("missing spacing behavior")?;
        let space = number(behavior, "spacing")?;
        let close = self.world.game.units.values().any(|v| {
            v.id < u.id
                && v.owner == u.owner
                && v.hp > 0.0
                && self.rules.kinds[v.kind].infantry
                && v.garrison.is_none()
                && v.path.is_empty()
                && distance(u.at(), v.at()) < space - 0.25
        });
        if !close {
            self.commands
                .units
                .get_mut(&id)
                .expect("spacing control")
                .react = 1.0;
            return Ok(());
        }
        let hit_at: BTreeMap<_, _> = self.combat.iter().map(|(id, c)| (id, c.hit_at)).collect();
        let points: Vec<_> = self.start.points.iter().map(|p| p.at).collect();
        let context = Context {
            game: &self.world.game,
            rules: &self.rules,
            grid: &self.grid,
            points: &points,
            hit_at: &hit_at,
        };
        let from = if self.options.mode == crate::economy::Mode::World {
            let actor = self.actor_policy(u.owner)?;
            command_destinations::threat_for_with_visibility(&context, id, u.at(), actor.visible)
        } else {
            command_destinations::threat_for(&context, id, u.at())
        }
        .map_err(|e| format!("spacing threat: {e:?}"))?;
        let rank = terrain::cover_rank(&self.world.game, &self.rules, u.at(), from);
        let radius = number(behavior, "coverSeek")?;
        if let Some(spot) = self.reaction_spot(&u, radius, from, (rank + 1).min(3), rank >= 3)? {
            self.assign_reaction_route(u, spot, false)?;
        }
        self.commands
            .units
            .get_mut(&id)
            .expect("spacing control")
            .react = 1.0;
        Ok(())
    }

    fn assign_reaction_route(&mut self, mut u: Unit, to: Point, back: bool) -> Result<(), String> {
        if !self.commands.units.contains_key(&u.id) {
            return Err("missing reaction route control".into());
        }
        let route = self.command_route(u.owner, &u, to)?;
        let route_end = route.route_end;
        u.path = route.waypoints;
        u.path_gen = u
            .path_gen
            .checked_add(1)
            .ok_or("reaction path identity exhausted")?;
        u.drift = true;
        if back {
            u.reverse = None;
            if let Some(end) = u.path.last().copied() {
                let cfg = Catalog::compiled()
                    .map_err(|e| e.to_string())?
                    .table("CFG")
                    .map_err(|e| e.to_string())?;
                let max = number(
                    cfg.get("behavior").ok_or("missing reverse behavior")?,
                    "reverseMax",
                )?;
                if distance(u.at(), end) <= max
                    && math::cos(math::atan2(end.z - u.z, end.x - u.x) - u.rot) < -0.5
                {
                    u.reverse = Some(end);
                }
            }
        }
        let control = self
            .commands
            .units
            .get_mut(&u.id)
            .expect("admitted reaction control");
        control.route_end = Some(route_end);
        control.rally_drift = false;
        self.world.game.units.insert(u.id, u);
        Ok(())
    }
}

#[cfg(test)]
mod tests;
