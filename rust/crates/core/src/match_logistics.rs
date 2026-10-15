//! Executable ordinary-Conquest supply phases over canonical Match state.
#[cfg(test)]
mod causal_tests;

use crate::{
    catalog::Catalog,
    game::{CELL, Game, MOVE, Point, Pose, Unit, VBLOCK},
    grid::SpatialGrid,
    logistics::{self, Definition, UnitFacts},
    match_abilities::AbilityState,
    match_combat::CombatState,
    match_commands::{MatchCommands, UnitControl},
    match_firing::FiringState,
    match_recovery::RecoveryState,
    match_routing::RecipientRouting,
    match_runtime::Match,
    math,
    navigation::{TerrainView, Versions},
    objectives::{ObjectivePlayer, ObjectiveUnit},
    pathfinding::PathTerrain,
    rules::Rules,
    supply_convoys::{self, ConvoyState, DepotPoint, Player, UnitOperation, UnitRole, UnitView},
    supply_network::{GroundSupplyNetwork, SupplyInput, SupplyUnit},
    supply_routes::{Handoff, RouteRequest, SupplyRouting, Threat},
    unit_spawn::{self, SpawnPlacer},
    vehicle,
    world::World,
};

fn view(g: &Game) -> TerrainView<'_> {
    TerrainView {
        width: g.w,
        height: g.h,
        flags: &g.flags,
        heights: g.height.as_deref(),
        world_known: true,
        knowledge_id: 0,
        versions: Versions {
            ..Versions::default()
        },
        changed: None,
        changes: &[],
    }
}
fn reserve_context(enabled: bool) -> logistics::Context {
    logistics::Context {
        enabled_override: Some(enabled),
        scenario: false,
        mode: logistics::Mode::Conquest,
    }
}
fn facts<'a>(u: &Unit, rules: &'a Rules) -> UnitFacts<'a> {
    UnitFacts {
        id: u.id,
        kind: &rules.kinds[u.kind].name,
        owner: Some(u.owner),
        owner_exists: true,
        guard_home: false,
        riding: u.riding,
    }
}
impl Match {
    /// Initialize after troop/base/scenario setup and metadata registration. The constructor invokes this after base and recipient setup.
    pub fn initialize_logistics(&mut self) -> Result<(), String> {
        if !self.options.logistics {
            return Ok(());
        }
        if self.convoys.is_some() {
            return Err("logistics already initialized".into());
        }
        self.validate_supply()?;
        for owner in 0..self.start.players.len() {
            self.validate_recipient(owner)?;
        }
        let mut initialized = self.logistics.clone();
        for u in self.world.game.units.values() {
            let def = Definition::from_catalog(&self.rules.kinds[u.kind].name)
                .map_err(|e| e.to_string())?;
            initialized.initialize_unit(reserve_context(true), facts(u, &self.rules), Some(&def));
        }
        let ids = self.world.game.units.keys().copied().collect::<Vec<_>>();
        self.recompute_supply_network(&ids)?;
        self.logistics = initialized;
        self.convoys = ConvoyState::setup(true);
        self.step_supply_convoys_with_dt(false, 0.0)
    }
    /// Prepare physical reserves before a new-unit payment/publication transaction commits.
    pub fn prepare_new_unit_reserves(
        &self,
        unit: &Unit,
    ) -> Result<Option<logistics::Reserves>, String> {
        if !matches!(
            self.options.mode,
            crate::economy::Mode::Default | crate::economy::Mode::Conquest
        ) {
            return Err("new reserves require ordinary Conquest".into());
        }
        if unit.id == 0
            || unit.owner >= self.start.players.len()
            || unit.kind >= self.rules.kinds.len()
            || self.world.game.units.contains_key(&unit.id)
            || self.logistics.get(unit.id).is_some()
        {
            return Err("invalid new reserve identity".into());
        }
        let definition = Definition::from_catalog(&self.rules.kinds[unit.kind].name)
            .map_err(|e| e.to_string())?;
        let mut prepared = logistics::LogisticsState::default();
        prepared.initialize_unit(
            reserve_context(self.options.logistics),
            facts(unit, &self.rules),
            Some(&definition),
        );
        Ok(prepared.remove(unit.id))
    }
    /// The source network runs on its own early-phase beat, using the tick's captured ordered list.
    pub fn refresh_supply_network(&mut self, unit_order: &[u32]) -> Result<(), String> {
        if self.convoys.is_none() {
            return Ok(());
        }
        self.recompute_supply_network(unit_order)
    }
    fn recompute_supply_network(&mut self, unit_order: &[u32]) -> Result<(), String> {
        self.validate_supply()?;
        let mut seen = std::collections::BTreeSet::new();
        if unit_order.iter().any(|id| !seen.insert(*id)) {
            return Err("duplicate supply tick unit".into());
        }
        let players = self
            .start
            .players
            .iter()
            .zip(&self.start.economy.players)
            .map(|(p, e)| ObjectivePlayer {
                team: e.team,
                out: e.out,
                spawn: p.spawn,
            })
            .collect::<Vec<_>>();
        let units = unit_order
            .iter()
            .filter_map(|id| self.world.game.units.get(id))
            .map(|u| SupplyUnit {
                fact: ObjectiveUnit::from_unit(
                    u,
                    &self.rules.kinds[u.kind].name,
                    self.logistics.get(u.id).is_some_and(|r| r.forced),
                ),
                built: u.built,
            })
            .collect::<Vec<_>>();
        let initial = self.world.initial();
        self.supply_net = GroundSupplyNetwork::compute(
            SupplyInput {
                view: view(&self.world.game),
                chars: &self.world.game.chars,
                initial_chars: &initial.chars,
                initial_heights: initial.height.as_deref(),
                points: &self.start.points,
                units: &units,
                players: &players,
            },
            &mut SpawnPlacer::new(),
        )?;
        Ok(())
    }
    /// Reserve time is charged before movement. Driving fuel is charged separately from actual powered travel.
    pub fn advance_unit_reserves(&mut self, id: u32) -> Result<(), String> {
        self.validate_supply()?;
        let Some(u) = self.world.game.units.get(&id) else {
            return Ok(());
        };
        self.logistics.tick_reserves(
            reserve_context(self.options.logistics),
            facts(u, &self.rules),
            self.rules.movement.tick,
            0.0,
        );
        Ok(())
    }
    pub fn consume_unit_driving_fuel(
        &mut self,
        id: u32,
        travel_seconds: f64,
    ) -> Result<(), String> {
        if !travel_seconds.is_finite() || travel_seconds < 0.0 {
            return Err("invalid powered travel time".into());
        }
        self.validate_supply()?;
        if let Some(u) = self.world.game.units.get(&id) {
            self.logistics
                .consume_driving_fuel(facts(u, &self.rules), travel_seconds);
        }
        Ok(())
    }
    /// Call once in source dead-unit order before removing each unit.
    pub fn record_convoy_death(&mut self, unit: &Unit) -> Result<(), String> {
        if let Some(state) = self.convoys.as_mut() {
            let kind = self
                .rules
                .kinds
                .get(unit.kind)
                .ok_or("unknown convoy death kind")?;
            if self.world.game.tick < 0 || unit.owner >= self.start.players.len() {
                return Err("invalid convoy death state".into());
            }
            state.retire_unit(
                self.world.game.tick as u64,
                unit.id,
                Some(unit.owner),
                kind.name == "truck",
            );
        }
        Ok(())
    }
    /// Admit an ordered recovery loop before it changes health, stock or wallets.
    /// Canonical store/reserve numbers remain trusted source-owned state.
    pub fn validate_support_provisions(&self) -> Result<(), String> {
        if self.convoys.is_none() {
            return Ok(());
        }
        self.validate_supply()?;
        for owner in 0..self.start.players.len() {
            self.validate_recipient(owner)?;
        }
        Ok(())
    }
    /// Spend from nearby real stores first, then the current unit's own provisions.
    pub fn spend_support_provisions(
        &mut self,
        unit: u32,
        owner: usize,
        amount: f64,
    ) -> Result<bool, String> {
        if !amount.is_finite() || amount < 0.0 || owner >= self.start.players.len() {
            return Err("invalid support provisions request".into());
        }
        if self.convoys.is_none() {
            return Ok(true);
        }
        self.with_supply_world(|ctx, state, reserves, world| {
            state.support_provision(ctx, unit, owner, amount, reserves, world)
        })
    }
    /// Manual supply uses canonical stores, reserves and the owner's remembered routes.
    pub fn supply_convoys(
        &mut self,
        owner: usize,
        ids: &[u32],
        target: u32,
    ) -> Result<bool, String> {
        self.reserve_convoy_path_changes(ids, 2)?;
        if self.convoys.is_none() {
            return Ok(false);
        }
        if owner >= self.start.players.len() {
            return Err("invalid convoy owner".into());
        }
        self.with_supply_world(|ctx, state, reserves, world| {
            state.supply(ctx, owner, ids, target, reserves, world)
        })
    }
    /// Resume retains source duplicate-ID order and clears waiting movement before rerouting.
    pub fn resume_convoys(&mut self, owner: usize, ids: &[u32]) -> Result<bool, String> {
        self.reserve_convoy_path_changes(ids, 3)?;
        if self.convoys.is_none() {
            return Ok(false);
        }
        if owner >= self.start.players.len() {
            return Err("invalid convoy owner".into());
        }
        self.with_supply_world(|ctx, state, _, world| Ok(state.resume(ctx, owner, ids, world)))
    }
    fn reserve_convoy_path_changes(&self, ids: &[u32], changes: u64) -> Result<(), String> {
        let mut needed = std::collections::BTreeMap::<u32, u64>::new();
        for id in ids {
            let count = needed.entry(*id).or_default();
            *count = count
                .checked_add(changes)
                .ok_or("convoy path identity exhausted")?;
        }
        for (id, count) in needed {
            if self
                .world
                .game
                .units
                .get(&id)
                .is_some_and(|u| u.path_gen.checked_add(count).is_none())
            {
                return Err("convoy path identity exhausted".into());
            }
        }
        Ok(())
    }
    pub(crate) fn intercept_convoy_movement(
        &mut self,
        owner: usize,
        command: &crate::match_commands::Command,
        raw_ids: &[i64],
    ) -> Result<bool, String> {
        if self.convoys.is_none() {
            return Ok(true);
        }
        use crate::{match_commands::Command, supply_convoys::MovementCommand};
        let movement = match command {
            Command::Move {
                orders,
                attack_move,
                queue,
                ..
            } => {
                let rows = orders
                    .iter()
                    .filter_map(|r| u32::try_from(r.unit).ok().map(|id| (id, r.at)))
                    .collect();
                if *attack_move {
                    MovementCommand::AttackMove {
                        at: None,
                        orders: rows,
                        queue: *queue,
                    }
                } else {
                    MovementCommand::Move {
                        at: None,
                        orders: rows,
                        queue: *queue,
                    }
                }
            }
            Command::Stop { .. } => MovementCommand::Stop,
            _ => return Ok(true),
        };
        let ids = raw_ids
            .iter()
            .filter_map(|id| u32::try_from(*id).ok())
            .collect::<Vec<_>>();
        self.with_supply_world(|ctx, state, _, world| {
            state.prepare_movement(ctx, owner, &ids, &movement, world)
        })
    }
    /// Late source phase, after capture and income. The caller owns clock advancement and winner state.
    pub fn step_supply_convoys(&mut self, winner_decided: bool) -> Result<(), String> {
        self.step_supply_convoys_with_dt(winner_decided, self.rules.movement.tick)
    }
    pub(crate) fn validate_supply(&self) -> Result<(), String> {
        self.validate_supply_context(false)
    }
    pub(crate) fn validate_withdrawal_supply(&self) -> Result<(), String> {
        self.validate_supply_context(true)
    }
    fn validate_supply_context(&self, garrisons: bool) -> Result<(), String> {
        if !matches!(
            self.options.mode,
            crate::economy::Mode::Default | crate::economy::Mode::Conquest
        ) {
            return Err("supply adapter supports ordinary Conquest only".into());
        }
        let g = &self.world.game;
        let birth_slots = u32::try_from(g.players.len()).map_err(|_| "invalid supply roster")?;
        if g.tick < 0
            || g.tick as u64 > 9_007_199_254_740_351
            || self.start.players.len() != g.players.len()
            || g.players.len() != self.start.economy.players.len()
            || self.next_unit_id.checked_add(birth_slots).is_none()
        {
            return Err("invalid supply Match state".into());
        }
        let n = g.w.checked_mul(g.h).ok_or("invalid supply terrain shape")?;
        if n == 0
            || g.chars.len() != n
            || g.flags.len() != n
            || g.height.as_ref().is_some_and(|v| v.len() != n)
            || g.wear.as_ref().is_some_and(|v| v.len() != n)
            || self.world.initial().chars.len() != n
            || self
                .world
                .initial()
                .height
                .as_ref()
                .is_some_and(|v| v.len() != n)
            || self.recipients.len() != g.players.len()
            || self.next_unit_id == 0
            || g.units
                .last_key_value()
                .is_some_and(|(&id, _)| id >= self.next_unit_id)
        {
            return Err("invalid supply terrain or allocator".into());
        }
        if self
            .world
            .aid
            .iter()
            .any(|(&c, &owner)| c >= n || owner >= g.players.len())
            || self.start.points.iter().any(|p| {
                p.owner.is_some_and(|o| o >= g.players.len())
                    || !p.at.x.is_finite()
                    || !p.at.z.is_finite()
            })
            || self
                .start
                .players
                .iter()
                .any(|p| !p.spawn.x.is_finite() || !p.spawn.z.is_finite())
        {
            return Err("invalid supply sources".into());
        }
        if self.convoys.is_some()
            && (self.supply_net.width != g.w
                || self.supply_net.height != g.h
                || self
                    .supply_net
                    .teams
                    .iter()
                    .any(|t| t.steps.len() != n || t.closed.len() != n))
        {
            return Err("invalid canonical supply network".into());
        }
        // Source fleet growth publishes at most one truck per player in each pass.
        // Admit every possible fresh ID before the first birth.
        for id in self.next_unit_id..self.next_unit_id + birth_slots {
            if self.commands.units.contains_key(&id)
                || self.firing.get(id).is_some()
                || self.combat.get(id).is_some()
                || self.abilities.get(id).is_some()
                || self.recovery.get(id).is_some()
                || self.logistics.get(id).is_some()
            {
                return Err("stale convoy birth metadata".into());
            }
        }
        for (i, p) in g.players.iter().enumerate() {
            if i != p.slot
                || !(0..32).contains(&p.team)
                || p.team != self.start.economy.players[i].team
            {
                return Err("supply player ownership mismatch".into());
            }
        }
        for (&id, u) in &g.units {
            if id != u.id
                || u.kind >= self.rules.kinds.len()
                || u.owner >= g.players.len()
                || !u.x.is_finite()
                || !u.z.is_finite()
                || !u.hp.is_finite()
                || !u.stuck.is_finite()
                || u.path_gen.checked_add(4).is_none()
                || u.built.is_some_and(|v| !v.is_finite())
                || u.still.is_some_and(|v| !v.is_finite())
                || u.path.iter().any(|p| !p.x.is_finite() || !p.z.is_finite())
                || u.world_goal
                    .is_some_and(|p| !p.x.is_finite() || !p.z.is_finite())
                || u.amove
                    .is_some_and(|p| !p.x.is_finite() || !p.z.is_finite())
                || !self.commands.units.contains_key(&u.id)
                || self.firing.get(u.id).is_none()
                || self.combat.get(u.id).is_none()
                || self.abilities.get(u.id).is_none()
                || self.recovery.get(u.id).is_none()
            {
                return Err("invalid canonical supply unit".into());
            }
            Catalog::compiled()
                .map_err(|e| e.to_string())?
                .unit(&self.rules.kinds[u.kind].name)
                .map_err(|e| e.to_string())?;
            if u.garrison.is_some_and(|c| !garrisons || c >= n) {
                return Err("supply garrison entry is not represented".into());
            }
            queued_points(
                &self.commands.units[&u.id],
                u.id,
                self.rules.kinds[u.kind].name != "truck",
            )?;
            let c = self.combat.get(u.id).unwrap();
            if u.riding != c.carrier.is_some()
                || c.carrier.is_some_and(|id| !g.units.contains_key(&id))
            {
                return Err("supply carrier identity mismatch".into());
            }
        }
        Ok(())
    }
    /// Withdrawal exits its house before requesting the existing physical hook.
    pub(crate) fn withdrawal_handoff(
        &mut self,
        id: u32,
        at: Point,
        entity: Option<u32>,
    ) -> Result<bool, String> {
        Ok(physical_handoff(
            &self.world,
            &self.rules,
            &self.combat,
            &mut self.supply_routes,
            PhysicalHandoff {
                unit: id,
                at,
                entity,
                radius: 15.0,
            },
        ))
    }
    fn step_supply_convoys_with_dt(&mut self, winner_decided: bool, dt: f64) -> Result<(), String> {
        if self.convoys.is_none() || winner_decided {
            return Ok(());
        }
        self.with_supply_world(|ctx, state, reserves, world| state.step(ctx, reserves, world, dt))
    }
    fn with_supply_world<R>(
        &mut self,
        action: impl FnOnce(
            supply_convoys::Context<'_>,
            &mut ConvoyState,
            &mut logistics::LogisticsState,
            &mut ConvoyWorld<'_>,
        ) -> Result<R, supply_convoys::ConvoyError>,
    ) -> Result<R, String> {
        self.validate_supply()?;
        for owner in 0..self.start.players.len() {
            self.validate_recipient(owner)?;
        }
        let players = self
            .start
            .players
            .iter()
            .zip(&self.start.economy.players)
            .enumerate()
            .map(|(slot, (p, e))| Player {
                slot,
                team: e.team,
                spawn: p.spawn,
                out: e.out,
                away: e.away,
            })
            .collect::<Vec<_>>();
        let points = self
            .start
            .points
            .iter()
            .map(|p| DepotPoint {
                at: p.at,
                owner: p.owner,
                contested: p.contested,
                depot: p.kind.as_deref() == Some("depot"),
            })
            .collect::<Vec<_>>();
        let aid = self
            .world
            .aid
            .iter()
            .map(|(&c, &o)| (c, o))
            .collect::<Vec<_>>();
        let chars = self.world.game.chars.clone();
        let key = match self.options.army {
            crate::economy_state::ArmySize::Standard => "standard",
            crate::economy_state::ArmySize::Large => "large",
            crate::economy_state::ArmySize::Massive => "massive",
            crate::economy_state::ArmySize::Endless => "endless",
        };
        let pop = Catalog::compiled()
            .map_err(|e| e.to_string())?
            .table("CFG")
            .map_err(|e| e.to_string())?
            .get("armies")
            .and_then(|v| v.get(key))
            .and_then(|v| v.get("pop"))
            .and_then(crate::catalog::Value::number)
            .ok_or("missing army population scale")?;
        let ctx = supply_convoys::Context {
            enabled: true,
            construction: false,
            tick: self.world.game.tick as u64,
            winner: false,
            army_pop: pop,
            players: &players,
            width: self.world.game.w,
            chars: &chars,
            aid: &aid,
            points: &points,
        };
        let mut adapter = ConvoyWorld {
            world: &mut self.world,
            environment: &self.environment,
            grid: &mut self.grid,
            commands: &mut self.commands,
            firing: &mut self.firing,
            combat: &mut self.combat,
            abilities: &mut self.abilities,
            recovery: &mut self.recovery,
            recipients: &mut self.recipients,
            routing_fog: &mut self.routing_fog,
            buildings: &self.building_cells,
            routing: &mut self.supply_routes,
            network: &self.supply_net,
            rules: &self.rules,
            players: &players,
            next_id: &mut self.next_unit_id,
            placer: SpawnPlacer::new(),
            retired: Vec::new(),
        };
        let result = action(
            ctx,
            self.convoys.as_mut().unwrap(),
            &mut self.logistics,
            &mut adapter,
        );
        for id in adapter.retired {
            self.logistics.remove(id);
        }
        result.map_err(|e| format!("Match convoys: {e:?}"))
    }
}
struct ConvoyWorld<'a> {
    world: &'a mut World,
    environment: &'a crate::environment::EnvironmentRuntime,
    grid: &'a mut SpatialGrid,
    commands: &'a mut MatchCommands,
    firing: &'a mut FiringState,
    combat: &'a mut CombatState,
    abilities: &'a mut AbilityState,
    recovery: &'a mut RecoveryState,
    recipients: &'a mut [RecipientRouting],
    routing_fog: &'a mut crate::fog::Fog,
    buildings: &'a std::collections::BTreeMap<usize, crate::match_runtime::BuildingCell>,
    routing: &'a mut SupplyRouting,
    network: &'a GroundSupplyNetwork,
    rules: &'a Rules,
    players: &'a [Player],
    next_id: &'a mut u32,
    placer: SpawnPlacer,
    retired: Vec<u32>,
}
impl ConvoyWorld<'_> {
    fn refresh_route_recipient(&mut self, owner: usize) {
        let receipt = crate::match_routing::refresh_recipient_parts(
            crate::match_routing::RecipientObservation {
                world: self.world,
                rules: self.rules,
                building_cells: self.buildings,
                environment: self.environment,
            },
            self.routing_fog,
            &mut self.recipients[owner],
            owner,
        )
        .expect("validated recipient routing parts");
        receipt
            .commit(self.world)
            .expect("unchanged convoy observation source");
    }
    fn unit_view(&self, u: &Unit) -> UnitView {
        let name = self.rules.kinds[u.kind].name.as_str();
        let c = self.combat.get(u.id).unwrap();
        UnitView {
            projected: false,
            id: u.id,
            owner: Some(u.owner),
            role: match name {
                "hq" => UnitRole::Hq,
                "supplycache" => UnitRole::Cache,
                "halftrack" => UnitRole::Halftrack,
                "truck" => UnitRole::Truck,
                _ => UnitRole::Other,
            },
            hp: u.hp,
            built: u.built.unwrap_or(f64::NAN),
            at: u.at(),
            still: u.still.unwrap_or(f64::NAN),
            cargo_len: c.cargo.len(),
            region_cache: false,
            riding: c.carrier,
            path: u.path.clone(),
            world_goal: u.world_goal,
            orders: queued_points(&self.commands.units[&u.id], u.id, name != "truck")
                .expect("validated queued convoy commands"),
            orders_pending: !self.commands.units[&u.id].orders.is_empty(),
            amove: u.amove,
            stuck: u.stuck,
        }
    }
    fn threats(&self, owner: usize) -> Vec<Threat> {
        let catalog = Catalog::compiled().expect("startup catalog");
        self.world.game.players[owner]
            .visible
            .iter()
            .filter_map(|id| self.world.game.units.get(id))
            .filter(|u| {
                u.hp > 0.0
                    && u.air.is_none()
                    && self.players[u.owner].team != self.players[owner].team
            })
            .filter_map(|u| {
                let w = catalog
                    .unit(&self.rules.kinds[u.kind].name)
                    .ok()?
                    .get("w")?;
                let range = w.get("range")?.number()?;
                (range > 0.0).then_some(Threat { at: u.at(), range })
            })
            .collect()
    }
}
impl supply_convoys::World for ConvoyWorld<'_> {
    fn units(&self) -> Vec<UnitView> {
        self.world
            .game
            .units
            .values()
            .map(|u| self.unit_view(u))
            .collect()
    }
    fn unit(&self, id: u32) -> Option<UnitView> {
        self.world.game.units.get(&id).map(|u| self.unit_view(u))
    }
    fn route(&mut self, u: &UnitView, at: Point, safe: bool) -> supply_convoys::Route {
        let kind = self.world.game.units[&u.id].kind;
        let owner = u.owner.unwrap();
        self.refresh_route_recipient(owner);
        let recipient = &self.recipients[owner];
        let query = recipient.query();
        let known = query
            .path_terrain(recipient.live())
            .expect("validated recipient terrain");
        let threats = self.threats(owner);
        let route = self
            .routing
            .route(
                RouteRequest {
                    owner,
                    tick: self.world.game.tick as u64,
                    from: u.at,
                    to: at,
                    safe,
                },
                known,
                &self.rules.kinds[kind],
                &threats,
            )
            .expect("validated convoy route inputs");
        if !u.projected
            && let Some(end) = route.route_end
        {
            self.commands.units.get_mut(&u.id).unwrap().route_end = Some(end);
            self.commands.requests.clear_failures(u.id);
        }
        route
    }
    fn continue_route(&mut self, u: &UnitView, _at: Point) -> bool {
        let kind = self.world.game.units[&u.id].kind;
        self.refresh_route_recipient(u.owner.unwrap());
        let recipient = &self.recipients[u.owner.unwrap()];
        SupplyRouting::continue_route(
            recipient.query().navigation(),
            &self.rules.kinds[kind],
            u.at,
            &u.path,
            u.stuck,
            &self.threats(u.owner.unwrap()),
        )
    }
    fn handoff(&mut self, u: &UnitView, at: Point, entity: Option<u32>, radius: f64) -> bool {
        physical_handoff(
            self.world,
            self.rules,
            self.combat,
            self.routing,
            PhysicalHandoff {
                unit: u.id,
                at,
                entity,
                radius,
            },
        )
    }
    fn supply_rate(&mut self, owner: usize, at: Point) -> Option<f64> {
        Some(
            self.network
                .rate_at(self.players.get(owner).map(|p| p.team), at)
                .expect("validated supply position"),
        )
    }
    fn spawn(&mut self, owner: usize, at: Point) -> u32 {
        let game = &self.world.game;
        let kind = self
            .rules
            .kinds
            .iter()
            .position(|k| k.name == "truck")
            .unwrap();
        let profile = &self.rules.kinds[kind];
        let nearby = game
            .units
            .values()
            .filter(|u| {
                u.air.is_none()
                    && !u.riding
                    && u.hp > 0.0
                    && math::hypot(u.x - at.x, u.z - at.z) < 30.0
            })
            .map(|u| {
                (
                    u.at(),
                    self.rules.kinds[u.kind].radius,
                    self.rules.kinds[u.kind].name == "truck",
                )
            })
            .collect::<Vec<_>>();
        let n = nearby.iter().filter(|v| v.2).count();
        let placement = self
            .placer
            .scatter_position(
                view(game),
                profile,
                self.players[owner].spawn,
                game.units.len() as u32,
            )
            .expect("validated truck spawn");
        let id = *self.next_id;
        let mut unit = unit_spawn::initial_unit(
            self.rules,
            Catalog::compiled().unwrap(),
            kind,
            id,
            owner,
            placement.point,
        )
        .expect("compiled truck definition");
        for k in 0..40 {
            let angle = (n + k) as f64 * 2.4;
            let radius = 10.0 + (k / 16) as f64 * 5.0;
            let c = self
                .placer
                .nearest_free(
                    view(game),
                    Point {
                        x: at.x + math::cos(angle) * radius,
                        z: at.z + math::sin(angle) * radius,
                    },
                    MOVE | VBLOCK,
                    None,
                )
                .expect("validated truck scatter");
            let p = Point {
                x: (c % game.w) as f64 * CELL + CELL / 2.0,
                z: (c / game.w) as f64 * CELL + CELL / 2.0,
            };
            if vehicle::vehicle_position_clear(
                game,
                Pose {
                    x: p.x,
                    z: p.z,
                    rot: 0.0,
                },
                profile,
                MOVE | VBLOCK,
            ) && nearby.iter().all(|(q, r, _)| {
                math::hypot(q.x - p.x, q.z - p.z) > (r + profile.radius) * 0.8 + 1.0
            }) {
                unit.x = p.x;
                unit.z = p.z;
                break;
            }
        }
        let mut control = UnitControl::new(&unit, self.rules).expect("compiled truck control");
        control.auto_retreat = false;
        self.commands.insert(id, control);
        self.firing.initialize(id);
        self.combat.initialize(id);
        self.abilities.initialize(id);
        self.recovery.initialize(id);
        self.world.game.units.insert(id, unit);
        self.grid.update(id, &self.world.game.units[&id]);
        *self.next_id += 1;
        id
    }
    fn retire(&mut self, id: u32) {
        self.retired.push(id);
        self.world.game.units.remove(&id);
        self.commands.units.remove(&id);
        self.firing.remove(id);
        self.combat.remove(id);
        self.abilities.remove(id);
        self.recovery.remove(id);
        self.grid.rebuild(&self.world.game.units);
    }
    fn apply(&mut self, id: u32, operation: UnitOperation) {
        let u = self.world.game.units.get_mut(&id).unwrap();
        let c = self.commands.units.get_mut(&id).unwrap();
        match operation {
            UnitOperation::Route {
                path,
                world_goal,
                repath,
            } => {
                u.path = path;
                u.path_gen += 1;
                u.world_goal = Some(world_goal);
                u.repath = repath;
                u.retreating = false;
                u.attack = false;
                c.attack_id = 0;
                self.firing.get_mut(id).unwrap().target_id = 0;
            }
            UnitOperation::ClearRoute => {
                u.path_gen += 1;
                u.path.clear();
                u.world_goal = None;
            }
            UnitOperation::ResumeClear => {
                u.path_gen += 1;
                u.path.clear();
                u.world_goal = None;
                c.orders.clear();
            }
            UnitOperation::RepairRoute(path) => {
                u.path = path;
                u.path_gen += 1;
                u.stuck = 0.0;
            }
            UnitOperation::NewTruck => c.auto_retreat = false,
        }
    }
}

fn queued_points(control: &UnitControl, id: u32, abilities: bool) -> Result<Vec<Point>, String> {
    use crate::match_commands::Command;
    let mut points = Vec::new();
    for command in &control.orders {
        match command {
            Command::Move {
                orders,
                control_ids,
                queue,
                together,
                ..
            } => {
                if orders.len() != 1
                    || control_ids.is_some()
                    || *queue
                    || *together
                    || orders[0].unit != i64::from(id)
                    || !orders[0].at.x.is_finite()
                    || !orders[0].at.z.is_finite()
                {
                    return Err("invalid queued convoy movement projection".into());
                }
                points.push(orders[0].at);
            }
            Command::Ability {
                ids,
                at: Some(at),
                queue: false,
                automatic: false,
            } if abilities
                && ids.as_slice() == [i64::from(id)]
                && at.x.is_finite()
                && at.z.is_finite() => {}
            _ => return Err("unsupported queued convoy command projection".into()),
        }
    }
    Ok(points)
}

struct PhysicalHandoff {
    unit: u32,
    at: Point,
    entity: Option<u32>,
    radius: f64,
}
fn physical_handoff(
    world: &World,
    rules: &Rules,
    combat: &CombatState,
    routing: &mut SupplyRouting,
    request: PhysicalHandoff,
) -> bool {
    let PhysicalHandoff {
        unit: id,
        at,
        entity,
        radius,
    } = request;
    let g = &world.game;
    let native = &g.units[&id];
    let start = combat
        .get(id)
        .and_then(|c| c.carrier)
        .and_then(|id| g.units.get(&id))
        .unwrap_or(native)
        .at();
    let mut near = vec![0u8; g.flags.len()];
    for (c, &f) in g.flags.iter().enumerate() {
        if f & (MOVE | crate::game::SIGHT) == (MOVE | crate::game::SIGHT) {
            let x = c % g.w;
            let y = c / g.w;
            for row in y.saturating_sub(1)..=(y + 1).min(g.h - 1) {
                for col in x.saturating_sub(1)..=(x + 1).min(g.w - 1) {
                    near[row * g.w + col] = 1;
                }
            }
        }
    }
    let fires = (0..g.flags.len())
        .map(|c| world.fires.contains_key(&c))
        .collect::<Vec<_>>();
    let owner = native.owner;
    let team = g.players[owner].team;
    let mines = (0..g.flags.len())
        .map(|c| {
            world.mines.get(&c).is_some_and(|o| *o != team)
                && world
                    .mine_seen
                    .get(&c)
                    .is_some_and(|mask| mask & (1u32 << team) != 0)
        })
        .collect::<Vec<_>>();
    let bunkers = g
        .units
        .values()
        .filter(|t| t.hp > 0.0 && rules.kinds[t.kind].name == "bunker")
        .map(Unit::at)
        .collect::<Vec<_>>();
    let terrain = PathTerrain {
        navigation: TerrainView {
            versions: Versions {
                infantry: world.versions.infantry,
                obstruction: world.versions.obstruction,
                ..Versions::default()
            },
            ..view(g)
        },
        near_walls: &near,
        known_mines: &mines,
        fires: &fires,
        bunkers: &bunkers,
        wear: g.wear.as_deref(),
        wetness: g.wet.unwrap_or(0.0),
        roads: world.roads,
    };
    let cells = entity
        .and_then(|id| g.units.get(&id))
        .and_then(|b| b.cells.as_deref())
        .unwrap_or(&[]);
    let rifle = &rules.kinds[rules.kinds.iter().position(|k| k.name == "rifle").unwrap()];
    routing
        .handoff(
            Handoff {
                tick: g.tick as u64,
                start,
                store: at,
                radius,
            },
            terrain,
            rifle,
            cells,
        )
        .expect("validated physical handoff")
}
