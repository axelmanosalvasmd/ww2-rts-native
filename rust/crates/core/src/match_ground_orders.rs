//! Ground Attack/Retreat over the existing recorded queue and canonical owners.
use crate::{
    catalog::Catalog,
    existing_observers::{ExistingObservers, PreparedGroundExit},
    game::{Point, Unit},
    grid::UnitId,
    match_commands::{Command, MoveOutcome, search_route},
    match_routing::{RecipientContext, prepare_recipient_exit_routing},
    match_runtime::Match,
    path_requests::RequestKind,
    pathfinding::PathRules,
};
use std::collections::BTreeMap;

impl Match {
    fn ground_orders_open(&self) -> Result<(), String> {
        if !crate::match_actor::ordinary_ground_mode(&self.world.game, self.options.mode) {
            return Err("ground orders require ordinary Default or Conquest".into());
        }
        Ok(())
    }
    fn ground_order_identity(&self, id: UnitId) -> Result<(), String> {
        let g = &self.world.game;
        let u = g.units.get(&id).ok_or("missing ground order actor")?;
        let kind = self
            .rules
            .kinds
            .get(u.kind)
            .ok_or("unknown ground order kind")?;
        if id == 0
            || id != u.id
            || u.owner >= g.players.len()
            || ![u.x, u.z, u.rot, u.hp, u.repath, u.supp]
                .iter()
                .all(|v| v.is_finite())
            || g.cell_of(u.x, u.z).is_none()
            || u.garrison.is_some_and(|c| c >= g.chars.len())
            || !self.commands.units.contains_key(&id)
            || self.firing.get(id).is_none()
            || self.combat.get(id).is_none()
            || self.abilities.get(id).is_none()
            || u.dig != self.field_works.is_active(id)
            || u.entrench != self.field_works.membership(id).is_some()
        {
            return Err("invalid canonical ground order actor".into());
        }
        Catalog::compiled()
            .map_err(|e| e.to_string())?
            .unit(&kind.name)
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    fn attack_target_identity(&self, id: UnitId) -> Result<(), String> {
        let g = &self.world.game;
        let t = g.units.get(&id).ok_or("missing attack target")?;
        let kind = self
            .rules
            .kinds
            .get(t.kind)
            .ok_or("unknown attack target kind")?;
        if id == 0
            || id != t.id
            || t.owner >= g.players.len()
            || ![t.x, t.z, t.rot, t.hp].iter().all(|v| v.is_finite())
            || t.cells
                .as_ref()
                .is_some_and(|cs| cs.iter().any(|c| *c as usize >= g.chars.len()))
        {
            return Err("invalid canonical attack target".into());
        }
        Catalog::compiled()
            .map_err(|e| e.to_string())?
            .unit(&kind.name)
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    fn ground_order_rows(
        &self,
        owner: usize,
        ids: &[i64],
        command: &Command,
    ) -> Result<Vec<UnitId>, String> {
        for raw in ids.iter().take(50.max(self.world.game.units.len())) {
            let Some(u) = u32::try_from(*raw)
                .ok()
                .and_then(|id| self.world.game.units.get(&id))
            else {
                continue;
            };
            if u.owner != owner || u.riding || u.rail {
                continue;
            }
            let kind = self
                .rules
                .kinds
                .get(u.kind)
                .ok_or("unknown selected ground kind")?;
            if !kind.structure && !kind.rail && !self.commands.units.contains_key(&u.id) {
                return Err("missing selected ground control".into());
            }
        }
        Ok(ids
            .iter()
            .take(50.max(self.world.game.units.len()))
            .filter_map(|raw| self.eligible_command_unit(*raw, owner, command))
            .collect())
    }
    fn ground_order_capacity(&self, ids: &[UnitId]) -> Result<(), String> {
        let mut counts = BTreeMap::<UnitId, u64>::new();
        for id in ids {
            *counts.entry(*id).or_default() += 1;
        }
        for (id, count) in counts {
            self.world.game.units[&id]
                .path_gen
                .checked_add(count)
                .ok_or("ground order path identity exhausted")?;
        }
        Ok(())
    }
    fn ground_weapon_range(&self, id: UnitId) -> Result<f64, String> {
        let u = &self.world.game.units[&id];
        let def = Catalog::compiled()
            .map_err(|e| e.to_string())?
            .unit(&self.rules.kinds[u.kind].name)
            .map_err(|e| e.to_string())?;
        Ok(def
            .get("w")
            .and_then(|w| w.get("range"))
            .and_then(|v| v.number())
            .unwrap_or(0.))
    }
    fn physical_ground_exit(&self, id: UnitId) -> Result<Unit, String> {
        PreparedGroundExit::prepare(&self.world.game, &self.rules, id)
            .map(PreparedGroundExit::into_unit)
    }
    pub(crate) fn ground_attack_command(
        &mut self,
        owner: usize,
        ids: &[i64],
        target: i64,
        queue: bool,
    ) -> Result<Option<crate::match_commands::Denial>, String> {
        use crate::match_commands::Denial;
        if self.ground_orders_open().is_err() {
            return Ok(Some(Denial::Unsupported));
        }
        let Some(target) = u32::try_from(target)
            .ok()
            .filter(|id| self.world.game.units.contains_key(id))
        else {
            return Ok(Some(Denial::Unseen));
        };
        let t = &self.world.game.units[&target];
        if self.world.game.allied(owner, t.owner) || !self.world.game.players[owner].sees(target) {
            return Ok(Some(Denial::Unseen));
        }
        self.attack_target_identity(target)?;
        let at = t.at();
        let air = t.air.is_some();
        let command = Command::Attack {
            ids: ids.to_vec(),
            target: i64::from(target),
            queue,
            at: None,
        };
        let selected = self.ground_order_rows(owner, ids, &command)?;
        for &id in &selected {
            self.ground_order_identity(id)?;
        }
        let candidates = selected
            .iter()
            .copied()
            .filter(|_| !air)
            .collect::<Vec<_>>();
        let mut prepared = Vec::new();
        for &id in &candidates {
            if self.ground_weapon_range(id)? == 0. {
                continue;
            }
            if !queue {
                if self.world.game.units[&id].build {
                    return Ok(Some(Denial::Unsupported));
                }
                let u = self.world.game.units[&id].clone();
                let u = if !self.can_shoot_target(id, target)? {
                    self.physical_ground_exit(id)?
                } else {
                    u
                };
                prepared.push(u);
            }
        }
        self.ground_order_capacity(&prepared.iter().map(|u| u.id).collect::<Vec<_>>())?;
        // Source mine() runs even for unarmed actors and the ground-to-AIR Needs path.
        for id in &selected {
            self.commands.units.get_mut(id).unwrap().follow = 0;
        }
        if candidates.is_empty() {
            return Ok(Some(Denial::Needs));
        }
        if queue {
            let mut ordered = false;
            for id in candidates {
                if self.ground_weapon_range(id)? == 0. {
                    continue;
                }
                let c = self.commands.units.get_mut(&id).unwrap();
                if c.orders.len() < 8 {
                    c.orders.push_back(Command::Attack {
                        ids: vec![i64::from(id)],
                        target: i64::from(target),
                        queue: false,
                        at: Some(at),
                    });
                    ordered = true;
                }
            }
            return Ok((!ordered).then_some(Denial::QueueFull));
        }
        if prepared.is_empty() {
            return Ok(Some(Denial::QueueFull));
        }
        for mut u in prepared {
            let id = u.id;
            // Repeated selected rows observe the preceding accepted replacement.
            u.path_gen = self.world.game.units[&id].path_gen;
            u.dig = self.world.game.units[&id].dig;
            u.entrench = self.world.game.units[&id].entrench;
            crate::match_runtime::cancel_field_work_state(&mut self.field_works, &mut u)
                .expect("all selected field-work projections admitted");
            u.traffic = None;
            u.traffic_wait = 0.;
            u.world_goal = None;
            u.retreating = false;
            u.attack = true;
            u.board = 0;
            u.amove = None;
            u.build = false;
            u.repath = 0.;
            u.path.clear();
            u.path_gen += 1;
            let c = self.commands.units.get_mut(&id).unwrap();
            c.orders.clear();
            c.face = None;
            c.attack_id = target;
            c.nade = None;
            c.enter = None;
            c.fire_at = None;
            c.outcome = Some(MoveOutcome::Interrupted);
            c.outcome_tick = Some(self.world.game.tick);
            let ability = self.abilities.get_mut(id).unwrap();
            ability.aimed_auto = false;
            ability.aimed_walked = false;
            self.grid.update(id, &u);
            self.world.game.units.insert(id, u);
        }
        Ok(None)
    }
    pub(crate) fn ground_retreat_command(
        &mut self,
        owner: usize,
        ids: &[i64],
    ) -> Result<Option<crate::match_commands::Denial>, String> {
        use crate::match_commands::Denial;
        if self.ground_orders_open().is_err() {
            return Ok(Some(Denial::Unsupported));
        }
        let command = Command::Retreat { ids: ids.to_vec() };
        let mut selected = self.ground_order_rows(owner, ids, &command)?;
        // Convoy commands inspect every truck before the ordinary troop selection cap.
        if let Some(state) = &self.convoys {
            for &raw in ids {
                if let Some(id) = self.eligible_command_unit(raw, owner, &command)
                    && state.trucks.contains_key(&id)
                    && !selected.contains(&id)
                {
                    selected.push(id);
                }
            }
        }
        for &id in &selected {
            self.ground_order_identity(id)?;
            if self.world.game.units[&id].build {
                return Ok(Some(Denial::Unsupported));
            }
        }
        self.ground_order_capacity(&selected)?;
        let mut forced = Vec::new();
        for raw in ids {
            if let Ok(id) = u32::try_from(*raw)
                && self
                    .world
                    .game
                    .units
                    .get(&id)
                    .is_some_and(|u| u.owner == owner)
                && self.logistics.get(id).is_some_and(|r| r.forced)
            {
                if !self.commands.units.contains_key(&id) {
                    return Err("missing forced retreat control".into());
                }
                forced.push(id);
            }
        }
        if selected.is_empty() {
            for id in forced {
                self.commands.units.get_mut(&id).unwrap().supply_hold = false;
            }
            return Ok(Some(Denial::Needs));
        }
        let mut exits = Vec::new();
        let mut unique = std::collections::BTreeSet::new();
        let mut prefixes = Vec::new();
        for &id in &selected {
            if unique.insert(id) {
                exits.push(PreparedGroundExit::prepare(
                    &self.world.game,
                    &self.rules,
                    id,
                )?);
            }
            prefixes.push(exits.len());
        }
        let mut homes = Vec::with_capacity(selected.len());
        for id in &selected {
            let unit = exits.iter().find(|exit| exit.id() == *id).unwrap().unit();
            if let Some(state) = self.convoys.as_ref().filter(|s| s.trucks.contains_key(id)) {
                let home = state
                    .stores
                    .iter()
                    .filter(|s| s.active && s.source && self.world.game.allied(owner, s.owner))
                    .min_by(|a, b| {
                        crate::math::hypot(a.at.x - unit.x, a.at.z - unit.z)
                            .total_cmp(&crate::math::hypot(b.at.x - unit.x, b.at.z - unit.z))
                    })
                    .map(|s| s.at);
                let Some(home) = home else {
                    return Ok(Some(Denial::Blocked));
                };
                homes.push(home);
            } else {
                homes.push(self.reaction_home_for(unit)?);
            }
        }
        let context = RecipientContext {
            world: &self.world,
            rules: &self.rules,
            building_cells: &self.building_cells,
            start: &self.start,
            options: &self.options,
            environment: &self.environment,
        };
        let mut preview = prepare_recipient_exit_routing(
            context,
            self.recipients
                .get_mut(owner)
                .ok_or("missing ground retreat recipient")?,
            &mut self.routing_fog,
            owner,
            ExistingObservers::new(&exits[..prefixes[0]])?,
        )?;
        let mut paths = preview.fork_paths(&self.commands.paths);
        let rules = PathRules::compiled().map_err(|e| e.to_string())?;
        let mut routes = Vec::new();
        for ((&id, &home), &prefix) in selected.iter().zip(&homes).zip(&prefixes) {
            preview.refresh_existing_exit_prefix(
                context,
                owner,
                ExistingObservers::new(&exits[..prefix])?,
            )?;
            let query = preview.query();
            let terrain = query
                .path_terrain(preview.live())
                .map_err(|e| format!("ground retreat terrain: {e:?}"))?;
            let unit = exits.iter().find(|exit| exit.id() == id).unwrap().unit();
            routes.push(
                search_route(
                    &mut paths,
                    terrain,
                    rules,
                    &self.rules.kinds[unit.kind],
                    unit,
                    home,
                )?
                .result,
            );
        }
        for ((&id, &home), route) in selected.iter().zip(&homes).zip(&routes) {
            if self
                .convoys
                .as_ref()
                .is_some_and(|s| s.trucks.contains_key(&id))
                && route.waypoints.is_empty()
                && crate::math::hypot(
                    self.world.game.units[&id].x - home.x,
                    self.world.game.units[&id].z - home.z,
                ) > 1.0
            {
                return Ok(Some(Denial::Blocked));
            }
        }
        let drafts = exits
            .into_iter()
            .map(|exit| (exit.id(), exit.into_unit()))
            .collect::<BTreeMap<_, _>>();
        let (_, receipt) = preview.prepare_publication(&mut self.world, &self.environment)?;
        receipt.commit();
        self.commands.paths = paths;
        for id in forced {
            self.commands.units.get_mut(&id).unwrap().supply_hold = false;
        }
        for id in &selected {
            self.commands.units.get_mut(id).unwrap().follow = 0;
        }
        for ((id, home), route) in selected.into_iter().zip(homes).zip(routes) {
            if let Some(truck) = self.convoys.as_mut().and_then(|s| s.trucks.get_mut(&id)) {
                truck.manual = true;
                truck.hold = false;
                truck.target = None;
                truck.state = crate::supply_convoys::TruckState::Manual;
            }
            let mut u = self.world.game.units[&id].clone();
            let draft = &drafts[&id];
            crate::unit_placement::place_unit(&mut self.grid, &mut u, |u| {
                u.x = draft.x;
                u.z = draft.z;
                u.garrison = draft.garrison;
            });
            self.apply_retreat_route(u, home, route)
                .expect("complete retreat batch admitted");
            let ability = self.abilities.get_mut(id).unwrap();
            ability.aimed_auto = false;
            ability.aimed_walked = false;
        }
        Ok(None)
    }
    pub fn current_attack_goal(&self, id: UnitId) -> Result<Option<Point>, String> {
        let Some(u) = self.world.game.units.get(&id) else {
            return Ok(None);
        };
        self.ground_order_identity(id)?;
        if u.hp <= 0. || u.retreating {
            return Ok(None);
        }
        let target = self.commands.units[&id].attack_id;
        let Some(t) = self.world.game.units.get(&target) else {
            return Ok(None);
        };
        if !self.world.game.players[u.owner].sees(target) {
            return Ok(None);
        }
        self.attack_target_identity(target)?;
        Ok((!self.can_shoot_target(id, target)?).then_some(t.at()))
    }
    /// Read-only admission for the explicit attack block reached by living ground actors.
    pub fn validate_ground_attacks(&self) -> Result<(), String> {
        self.ground_orders_open()?;
        for u in self.world.game.units.values() {
            let kind = self.rules.kinds.get(u.kind).ok_or("unknown attack kind")?;
            let c = self
                .commands
                .units
                .get(&u.id)
                .ok_or("missing attack control")?;
            if u.hp <= 0.
                || u.air.is_some()
                || kind.structure
                || kind.rail
                || c.operative
                || u.riding
                || c.attack_id == 0
                || c.nade.is_some()
            {
                continue;
            }
            self.ground_order_identity(u.id)?;
            if !self.world.game.players[u.owner].sees(c.attack_id)
                || !self.world.game.units.contains_key(&c.attack_id)
            {
                continue;
            }
            u.path_gen
                .checked_add(3)
                .ok_or("ground attack path identity exhausted")?;
            for target in [c.attack_id, self.firing.get(u.id).unwrap().target_id] {
                if self.world.game.units.contains_key(&target) {
                    self.attack_target_identity(target)?;
                }
            }
            self.can_shoot_target(u.id, c.attack_id)?;
        }
        Ok(())
    }
    /// Source explicit attack after aimed/entry/attack-move and before reactions.
    pub fn apply_ground_attack(&mut self, id: UnitId) -> Result<(), String> {
        let Some(u) = self.world.game.units.get(&id) else {
            return Ok(());
        };
        self.ground_order_identity(id)?;
        let c = &self.commands.units[&id];
        let target = c.attack_id;
        if target == 0 || c.nade.is_some() {
            self.world.game.units.get_mut(&id).unwrap().attack = target != 0;
            return Ok(());
        }
        let Some(t) = self.world.game.units.get(&target) else {
            self.commands.units.get_mut(&id).unwrap().attack_id = 0;
            self.world.game.units.get_mut(&id).unwrap().attack = false;
            return Ok(());
        };
        if !self.world.game.players[u.owner].sees(target) {
            self.commands.units.get_mut(&id).unwrap().attack_id = 0;
            self.world.game.units.get_mut(&id).unwrap().attack = false;
            return Ok(());
        }
        self.attack_target_identity(t.id)?;
        if self.can_shoot_target(id, target)? {
            self.world.game.units[&id]
                .path_gen
                .checked_add(1)
                .ok_or("ground attack path identity exhausted")?;
            let prior = self
                .firing
                .get(id)
                .ok_or("missing attack firing row")?
                .target_id;
            if self.world.game.units.contains_key(&prior) {
                self.attack_target_identity(prior)?;
            }
            self.set_firing_target(id, target)?;
            let u = self.world.game.units.get_mut(&id).unwrap();
            u.attack = true;
            u.path.clear();
            u.path_gen += 1;
        } else if !self.logistics.get(id).is_some_and(|r| r.forced) && u.repath <= 0. {
            let at = t.at();
            self.request_command_path(id, at, RequestKind::AttackId)?;
        }
        self.world.game.units.get_mut(&id).unwrap().attack =
            self.commands.units[&id].attack_id != 0;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn scene() -> Match {
        let fixture: Value =
            serde_json::from_str(include_str!("../tests/fixtures/recipient-snapshot.json"))
                .unwrap();
        let mut m = crate::recipient_snapshot::tests::initialize(&fixture["cases"][0]);
        m.world.game.players[0].visible = vec![3];
        m
    }

    #[test]
    fn mixed_ground_air_attack_orders_both_groups_immediately() {
        mixed_ground_air_attack(false);
    }

    #[test]
    fn mixed_ground_air_attack_queues_both_groups() {
        mixed_ground_air_attack(true);
    }

    fn mixed_ground_air_attack(queue: bool) {
        let mut m = scene();
        let plane = m.spawn_aircraft(0, "attacker").unwrap();
        let command = Command::Attack {
            ids: vec![1, i64::from(plane)],
            target: 3,
            queue,
            at: None,
        };
        assert_eq!(m.execute_command(0, &command, false), Ok(None));
        if queue {
            for id in [1, plane] {
                assert!(
                    matches!(
                        m.commands.units[&id].orders.front(),
                        Some(Command::Attack {
                            target: 3,
                            queue: false,
                            ..
                        })
                    ),
                    "selected actor {id} must retain its attack order"
                );
            }
        } else {
            assert_eq!(m.commands.units[&1].attack_id, 3);
            assert_eq!(m.aviation.planes[&plane].mission.unwrap().target, Some(3));
        }
    }

    #[test]
    fn attack_batch_preflights_metadata_and_repeated_path_capacity_before_follow() {
        let mut m = scene();
        let mut second = m.world.game.units[&1].clone();
        second.id = 4;
        let control = m.commands.units[&1].clone();
        m.world.game.units.insert(4, second);
        m.commands.units.insert(4, control);
        m.combat.initialize(4);
        m.abilities.initialize(4);
        for id in [1, 4] {
            m.commands.units.get_mut(&id).unwrap().follow = 77;
        }
        let before = format!("{:?}", (&m.world.game.units, &m.commands.units));
        let attack = Command::from_json(&json!({"t":"attack","ids":[1,4],"target":3})).unwrap();
        // The snapshot scene retains startup metadata after replacing its units.
        m.firing.remove(4);
        assert!(m.firing.get(4).is_none());
        assert!(m.execute_command(0, &attack, false).is_err());
        assert_eq!(
            format!("{:?}", (&m.world.game.units, &m.commands.units)),
            before
        );
        let second = m.commands.units.remove(&4).unwrap();
        let before = format!("{:?}", (&m.world.game.units, &m.commands.units));
        assert!(m.execute_command(0, &attack, false).is_err());
        assert_eq!(
            format!("{:?}", (&m.world.game.units, &m.commands.units)),
            before
        );
        m.commands.units.insert(4, second);
        m.firing.initialize(4);
        m.world.game.units.get_mut(&1).unwrap().path_gen = u64::MAX - 1;
        let before = format!("{:?}", (&m.world.game.units, &m.commands.units));
        let repeated =
            Command::from_json(&json!({"t":"attack","ids":[1,1],"target":3,"x":999,"z":999}))
                .unwrap();
        assert!(matches!(repeated, Command::Attack { at: None, .. }));
        assert!(m.execute_command(0, &repeated, false).is_err());
        assert_eq!(
            format!("{:?}", (&m.world.game.units, &m.commands.units)),
            before
        );
    }

    #[test]
    fn explicit_attack_refuses_malformed_visible_target_before_flag_or_claim_writes() {
        let mut m = scene();
        m.commands.units.get_mut(&1).unwrap().attack_id = 3;
        m.world.game.units.get_mut(&3).unwrap().x = f64::NAN;
        let before = format!("{:?}", (&m.world.game.units, &m.commands.units));
        let claims = m.firing.target_claims().collect::<Vec<_>>();
        assert!(m.apply_ground_attack(1).is_err());
        assert_eq!(
            format!("{:?}", (&m.world.game.units, &m.commands.units)),
            before
        );
        assert_eq!(m.firing.target_claims().collect::<Vec<_>>(), claims);
    }
    #[test]
    fn convoy_retreat_outside_troop_cap_without_source_refuses_before_follow_or_routing() {
        use crate::supply_convoys::{ConvoyState, StoreId, Truck, TruckState};
        let mut m = scene();
        let truck = crate::unit_spawn::initial_unit(
            &m.rules,
            Catalog::compiled().unwrap(),
            m.rules.kind("lorry").unwrap(),
            4,
            0,
            Point { x: 8., z: 8. },
        )
        .unwrap();
        m.commands.insert(
            4,
            crate::match_commands::UnitControl::new(&truck, &m.rules).unwrap(),
        );
        m.world.game.units.insert(4, truck);
        m.combat.initialize(4);
        m.firing.initialize(4);
        m.abilities.initialize(4);
        m.recovery.initialize(4);
        let mut convoys = ConvoyState::default();
        convoys.trucks.insert(
            4,
            Truck {
                state: TruckState::Idle,
                cargo: crate::logistics::Stock::default(),
                target: None,
                origin: StoreId::Source(0),
                timer: 0.,
                manual: false,
                hold: false,
                route: vec![],
                destination: None,
                route_failure: None,
                retry: None,
            },
        );
        m.convoys = Some(convoys);
        m.commands.units.get_mut(&1).unwrap().follow = 77;
        let before = format!(
            "{:?}",
            (
                &m.world.game.units,
                &m.commands.units,
                &m.commands.requests,
                &m.routing_fog,
                &m.convoys
            )
        );
        let mut ids = vec![1; 50];
        ids.push(4);
        assert_eq!(
            m.execute_command(0, &Command::Retreat { ids }, false)
                .unwrap(),
            Some(crate::match_commands::Denial::Blocked)
        );
        assert_eq!(
            format!(
                "{:?}",
                (
                    &m.world.game.units,
                    &m.commands.units,
                    &m.commands.requests,
                    &m.routing_fog,
                    &m.convoys
                )
            ),
            before
        );
    }
}
