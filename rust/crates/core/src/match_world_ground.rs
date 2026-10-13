//! World ground policy over the existing actor, region and remembered-route owners.
use crate::{
    catalog::{Catalog, Value},
    economy::Mode,
    game::{LOCAL_DEFENDER_OWNER, ModeKind, Point, Unit},
    match_commands::CommandRoute,
    match_runtime::Match,
    math,
};

fn distance(a: Point, b: Point) -> f64 {
    math::hypot(a.x - b.x, a.z - b.z)
}
impl Match {
    pub(crate) fn uses_world_routes(&self) -> bool {
        self.options.mode == Mode::World && self.world.game.mode == Some(ModeKind::World)
    }
    /// Current route admission only. The sole runner admits later effects before its clock.
    fn validate_world_actor_facts(&self, unit: &Unit) -> Result<(), String> {
        if !self.uses_world_routes() || self.options.logistics || self.convoys.is_some() {
            return Err("World ground routing requires disabled logistics".into());
        }
        if self
            .world
            .game
            .units
            .get(&unit.id)
            .is_none_or(|current| current.owner != unit.owner || current.kind != unit.kind)
        {
            return Err("World ground facts do not bind their current unit identity".into());
        }
        self.validate_world_memory_context(unit.owner)?;
        self.world_runtime
            .as_ref()
            .unwrap()
            .memory
            .validate_binding(&self.world)?;
        let actor = self.actor_policy(unit.owner)?;
        if actor.team != crate::match_actor::checked_game_team(&self.world.game, unit.owner)? {
            return Err("World ground actor team differs from canonical Game".into());
        }
        self.rules
            .kinds
            .get(unit.kind)
            .ok_or("unknown World ground kind")?;
        self.commands
            .units
            .get(&unit.id)
            .ok_or("missing World ground control")?;
        if unit.id == 0
            || ![unit.x, unit.z, unit.hp, unit.rot]
                .iter()
                .all(|v| v.is_finite())
        {
            return Err("unsupported or malformed World ground actor".into());
        }
        Ok(())
    }
    pub(crate) fn validate_world_ground_actor(&self, unit: &Unit) -> Result<(), String> {
        self.validate_world_actor_facts(unit)?;
        let kind = &self.rules.kinds[unit.kind];
        if kind.air
            || kind.rail
            || unit.air.is_some()
            || unit.riding
            || self.commands.units[&unit.id].operative
            || unit.garrison.is_some()
            || unit
                .path
                .iter()
                .chain(unit.world_goal.iter())
                .chain(unit.amove.iter())
                .any(|p| !p.x.is_finite() || !p.z.is_finite())
        {
            return Err("unsupported World ground actor".into());
        }
        Ok(())
    }
    pub(crate) fn validate_world_path_service(&self) -> Result<(), String> {
        for unit in self.world.game.units.values() {
            self.validate_world_ground_actor(unit)?;
            let control = &self.commands.units[&unit.id];
            if control.attack_id != 0
                || control.nade.is_some()
                || control.fire_at.is_some()
                || control.enter.is_some()
                || control.build_target.is_some()
                || unit.board != 0
                || unit.dig
                || unit.entrench
                || unit.build
                || self.firing.get(unit.id).is_none()
            {
                return Err("World tactical route branch is not connected".into());
            }
        }
        for request in self.commands.requests.queued_requests() {
            Self::validate_world_request_kind(request.kind)?;
        }
        Ok(())
    }
    pub(crate) fn validate_world_request_kind(
        kind: crate::path_requests::RequestKind,
    ) -> Result<(), String> {
        use crate::path_requests::RequestKind;
        if !matches!(
            kind,
            RequestKind::World | RequestKind::Amove | RequestKind::Recovery
        ) {
            return Err("World request kind is not connected".into());
        }
        Ok(())
    }
    /// Exact World home selection for a supplied current or prepared exit pose.
    /// Enabled convoy source selection remains refused by the actor context.
    pub fn world_ground_home_for(&self, unit: &Unit) -> Result<Point, String> {
        let order = self.world.game.units.keys().copied().collect::<Vec<_>>();
        self.world_ground_home_in_order(unit, &order)
    }
    /// Restore or replay callers supply their actual complete live-object order.
    /// This does not create another unit collection or retain a mutable order owner.
    pub fn world_ground_home_in_order(&self, unit: &Unit, order: &[u32]) -> Result<Point, String> {
        self.validate_world_live_order(order)?;
        self.validate_world_actor_facts(unit)?;
        let actor = self.actor_policy(unit.owner)?;
        if unit.owner == LOCAL_DEFENDER_OWNER {
            return Ok(self.commands.units[&unit.id]
                .guard_home
                .unwrap_or(actor.spawn));
        }
        let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
        let mut home = None;
        let mut best = f64::INFINITY;
        for &id in order {
            let building = &self.world.game.units[&id];
            if building.owner != unit.owner
                || building.hp <= 0.0
                || !building.built.is_some_and(|v| v >= 1.0)
                || self
                    .world_region_at(building.at())?
                    .is_none_or(|r| r.team != actor.team)
            {
                continue;
            }
            let kind = self
                .rules
                .kinds
                .get(building.kind)
                .ok_or("unknown World home kind")?;
            if !catalog
                .unit(&kind.name)
                .map_err(|e| e.to_string())?
                .get("produces")
                .is_some_and(|p| !matches!(p, Value::Bool(false) | Value::Null))
            {
                continue;
            }
            let d = distance(unit.at(), building.at());
            if d < best {
                best = d;
                home = Some(building.at());
            }
        }
        if let Some(home) = home {
            return Ok(home);
        }
        let runtime = self.world_runtime.as_ref().ok_or("missing World runtime")?;
        for (definition, state) in self
            .world
            .geography()
            .ok_or("missing World geography")?
            .regions
            .iter()
            .zip(&runtime.regions)
        {
            if state.team != actor.team {
                continue;
            }
            let at = Point {
                x: (definition.x as f64 + 0.5) * 2.0,
                z: (definition.y as f64 + 0.5) * 2.0,
            };
            let d = distance(unit.at(), at);
            if d < best {
                best = d;
                home = Some(at);
            }
        }
        Ok(home.unwrap_or(unit.at()))
    }
    /// Source marchMul, evaluated at the unit's actual turn before traffic steering.
    pub fn world_march_multiplier(&self, unit: &Unit) -> Result<f64, String> {
        self.validate_world_actor_facts(unit)?;
        let actor = self.actor_policy(unit.owner)?;
        let kind = &self.rules.kinds[unit.kind];
        let control = &self.commands.units[&unit.id];
        let target = self
            .firing
            .get(unit.id)
            .ok_or("missing World march firing")?
            .target_id;
        let hit = self
            .combat
            .get(unit.id)
            .ok_or("missing World march combat")?
            .hit_at;
        if !hit.is_finite() || unit.shot_at.is_some_and(|v| !v.is_finite()) {
            return Err("invalid World march combat times".into());
        }
        let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
        let march = catalog
            .table("CFG")
            .map_err(|e| e.to_string())?
            .get("march")
            .ok_or("missing World march rules")?;
        let calm = march
            .get("calm")
            .and_then(Value::number)
            .ok_or("missing march calm")?
            / self.rules.movement.tick;
        if kind.naval
            || kind.air
            || control.attack_id != 0
            || target != 0
            || self.world.game.tick as f64 - hit <= calm
            || self.world.game.tick as f64 - unit.shot_at.unwrap_or(-1e9) <= calm
            || self
                .world_region_at(unit.at())?
                .is_none_or(|r| r.team != actor.team)
        {
            return Ok(1.0);
        }
        march
            .get("speed")
            .and_then(Value::number)
            .filter(|v| v.is_finite() && *v >= 0.0)
            .ok_or_else(|| "invalid World march speed".into())
    }
    pub(crate) fn route_failure_version(&self, owner: usize) -> u64 {
        if self.uses_world_routes() {
            self.actor_policy(owner)
                .ok()
                .and_then(|actor| {
                    self.world_runtime
                        .as_ref()?
                        .memory
                        .team(actor.team)
                        .map(|m| m.version())
                })
                .unwrap_or(0)
        } else {
            self.recipients
                .get(owner)
                .map(|r| r.query().navigation().versions.knowledge)
                .unwrap_or(0)
        }
    }
    pub(crate) fn search_unit_route(
        &mut self,
        unit: &Unit,
        goal: Point,
    ) -> Result<CommandRoute, String> {
        let rules = crate::pathfinding::PathRules::compiled().map_err(|e| e.to_string())?;
        if self.uses_world_routes() {
            self.validate_world_ground_actor(unit)?;
            self.refresh_world_routing(unit.owner)?;
            let team = self.actor_policy(unit.owner)?.team;
            let memory = &self.world_runtime.as_ref().unwrap().memory;
            let query = memory
                .team(team)
                .ok_or("missing World team memory")?
                .query();
            let live = memory
                .live_for(unit.owner)
                .ok_or("missing World route live")?
                .live();
            let terrain = query
                .path_terrain(live)
                .map_err(|e| format!("World path: {e:?}"))?;
            crate::match_commands::search_route(
                &mut self.commands.paths,
                terrain,
                rules,
                &self.rules.kinds[unit.kind],
                unit,
                goal,
            )
        } else {
            self.refresh_recipient_routing(unit.owner)?;
            let query = self.recipients[unit.owner].query();
            let terrain = query
                .path_terrain(self.recipients[unit.owner].live())
                .map_err(|e| format!("recipient path: {e:?}"))?;
            crate::match_commands::search_route(
                &mut self.commands.paths,
                terrain,
                rules,
                &self.rules.kinds[unit.kind],
                unit,
                goal,
            )
        }
    }
    /// Same canonical movement and scheduler, with deferred World observation publication.
    pub(crate) fn move_world_unit_with_routes(
        &mut self,
        id: u32,
        suppression: f64,
    ) -> Result<Option<crate::movement::Moved>, String> {
        let unit = self
            .world
            .game
            .units
            .get(&id)
            .ok_or("missing World moving unit")?;
        self.validate_world_ground_actor(unit)?;
        let march = self.world_march_multiplier(unit)?;
        let owner = unit.owner;
        let kind = unit.kind;
        let visible = self.actor_policy(owner)?.visible.to_vec();
        let rules = crate::pathfinding::PathRules::compiled().map_err(|e| e.to_string())?;
        let runtime = self.world_runtime.as_mut().ok_or("missing World runtime")?;
        let preview = crate::match_world_memory::prepare_world_routing(
            crate::match_world_memory::WorldRoutingContext {
                world: &self.world,
                environment: &self.environment,
                rules: &self.rules,
                start: &self.start,
                rails: &runtime.rails,
                regions: &runtime.regions,
                known_buildings: &runtime.known_buildings,
                local_defender: &runtime.local_defender,
            },
            &mut runtime.memory,
            owner,
            None,
        )?;
        let mut paths = preview.fork_paths(&self.commands.paths);
        let (game, mut commit) = preview.prepare_publication(&mut self.world, &self.environment)?;
        let read = std::cell::Cell::new(false);
        let query = commit.query();
        let terrain = query
            .path_terrain(commit.live())
            .map_err(|e| format!("World movement path: {e:?}"))?;
        let knowledge = crate::traffic_knowledge::TrafficKnowledge::observed(
            query.navigation(),
            commit.knowledge(),
            commit.wrecks(),
        )
        .map_err(|e| format!("World movement traffic: {e:?}"))?
        .with_actor(owner, &visible)
        .track_reads(&read);
        let version = commit.failure_version();
        let profile = &self.rules.kinds[kind];
        let control = self
            .commands
            .units
            .get_mut(&id)
            .ok_or("missing World moving control")?;
        let mut error = None;
        let moved = crate::movement::move_unit_with_march(
            game,
            &self.rules,
            &mut self.grid,
            id,
            suppression,
            &mut |_, _, u, goal, why| {
                if error.is_some() {
                    return;
                }
                match why {
                    crate::movement::RouteRequest::Recovery => {
                        let mut context = crate::match_commands::MovingRequestContext {
                            unit: u,
                            control,
                            paths: &mut paths,
                            terrain,
                            profile,
                            rules,
                            failure_version: version,
                            terrain_read: Some(&read),
                        };
                        if let Err(e) = self.commands.requests.request(
                            crate::path_requests::Request {
                                unit: id,
                                goal,
                                kind: crate::path_requests::RequestKind::Recovery,
                            },
                            &mut context,
                        ) {
                            error = Some(format!("World movement recovery: {e:?}"));
                        }
                    }
                    crate::movement::RouteRequest::Immediate => {
                        read.set(true);
                        self.commands.requests.clear_failures(id);
                        match crate::match_commands::search_route(
                            &mut paths, terrain, rules, profile, u, goal,
                        ) {
                            Ok(route) => {
                                control.route_end = Some(route.result.route_end);
                                if !route.result.waypoints.is_empty() {
                                    u.path = route.result.waypoints;
                                    u.path_gen += 1;
                                }
                                u.repath = 1.0;
                            }
                            Err(e) => error = Some(e),
                        }
                    }
                }
            },
            &knowledge,
            march,
        )
        .map_err(|e| format!("World movement traffic: {e:?}"));
        drop(query);
        commit.sync_moved_object(
            game.units
                .get(&id)
                .expect("movement reinserts checked unit"),
            &self.rules,
            read.get(),
        );
        if read.get() {
            commit.commit();
            self.commands.paths = paths;
        }
        if let Some(error) = error {
            return Err(error);
        }
        moved
    }
}

#[cfg(test)]
mod tests;
