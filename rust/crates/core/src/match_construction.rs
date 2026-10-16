//! Paid sites and the source-positioned builder/crew phases. No second job or queue authority.
use crate::{
    catalog::{Catalog, Value},
    economy::Mode,
    game::Unit,
    grid::UnitId,
    match_commands::UnitControl,
    match_runtime::Match,
    math,
};
use std::collections::{BTreeMap, BTreeSet};

/// One tick's worker references, in the source's first-site and unit-turn order.
/// A worker's later movement or health change does not revise this captured membership.
#[derive(Debug)]
pub struct ConstructionFrame {
    tick: i64,
    sites: Vec<(UnitId, Vec<UnitId>)>,
    indexes: BTreeMap<UnitId, usize>,
    workers: BTreeSet<UnitId>,
}

pub(crate) fn validate_build_activity(unit: &Unit, control: &UnitControl) -> Result<(), String> {
    if unit.build != control.build_target.is_some() || control.build_target == Some(0) {
        Err("unrepresented or inconsistent construction activity".into())
    } else {
        Ok(())
    }
}

/// Clear the scalar and its activity projection together, without removing or refunding the site.
pub(crate) fn clear_build_activity(
    unit: &mut Unit,
    control: &mut UnitControl,
) -> Result<(), String> {
    validate_build_activity(unit, control)?;
    control.build_target = None;
    unit.build = false;
    Ok(())
}

fn number(value: &Value, key: &str) -> Result<f64, String> {
    value
        .get(key)
        .and_then(Value::number)
        .filter(|v| v.is_finite())
        .ok_or_else(|| format!("invalid construction definition {key}"))
}

impl Match {
    fn validate_construction_actor(&self, id: UnitId) -> Result<(), String> {
        let unit = self
            .world
            .game
            .units
            .get(&id)
            .ok_or("missing construction actor")?;
        let control = self
            .commands
            .units
            .get(&id)
            .ok_or("missing construction control")?;
        validate_build_activity(unit, control)?;
        if unit.id != id
            || unit.kind >= self.rules.kinds.len()
            || crate::match_actor::checked_game_owner(&self.world.game, unit.owner).is_err()
            || ![unit.hp, unit.x, unit.z].iter().all(|v| v.is_finite())
        {
            return Err("invalid construction actor identity".into());
        }
        if let Some(target) = control.build_target {
            let kind = self
                .rules
                .kinds
                .get(unit.kind)
                .ok_or("invalid construction actor kind")?;
            if kind.structure
                || kind.air
                || kind.rail
                || unit.riding
                || control.operative
                || ![unit.x, unit.z, unit.hp, unit.repath]
                    .iter()
                    .all(|v| v.is_finite())
            {
                return Err("unsupported or invalid construction worker".into());
            }
            if let Some(site) = self.world.game.units.get(&target) {
                let kind = self
                    .rules
                    .kinds
                    .get(site.kind)
                    .ok_or("invalid construction site kind")?;
                if !kind.building
                    || site
                        .built
                        .is_none_or(|v| !v.is_finite() || !(0.0..=1.0).contains(&v))
                    || ![site.hp, site.x, site.z].iter().all(|v| v.is_finite())
                {
                    return Err("invalid construction site".into());
                }
            }
        }
        Ok(())
    }

    pub(crate) fn validate_building_activity_rows(&self) -> Result<(), String> {
        for &id in self.world.game.units.keys() {
            self.validate_construction_actor(id)?;
        }
        Ok(())
    }

    /// Source currentPathGoal(Build) deliberately keeps a dead site's path goal until the worker turn.
    pub(crate) fn current_build_goal(
        &self,
        id: UnitId,
    ) -> Result<Option<crate::game::Point>, String> {
        let Some(unit) = self.world.game.units.get(&id) else {
            return Ok(None);
        };
        self.validate_construction_actor(id)?;
        if unit.hp <= 0.0 || unit.retreating || !unit.path.is_empty() {
            return Ok(None);
        }
        let Some(target) = self.commands.units[&id].build_target else {
            return Ok(None);
        };
        let Some(site) = self.world.game.units.get(&target) else {
            return Ok(None);
        };
        let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
        let def = catalog
            .unit(&self.rules.kinds[site.kind].name)
            .map_err(|e| e.to_string())?;
        let classic = catalog
            .table("CFG")
            .map_err(|e| e.to_string())?
            .get("classic")
            .ok_or("missing construction rules")?;
        Ok(
            (!(site.built.unwrap_or(0.0) >= 1.0 && site.hp >= number(def, "hpPer")?)
                && math::hypot(unit.x - site.x, unit.z - site.z)
                    > number(def, "radius")? + number(classic, "buildReach")?)
            .then_some(site.at()),
        )
    }

    pub(crate) fn drop_construction_order(&mut self, id: UnitId) -> Result<(), String> {
        self.validate_construction_actor(id)?;
        clear_build_activity(
            self.world.game.units.get_mut(&id).unwrap(),
            self.commands.units.get_mut(&id).unwrap(),
        )
    }

    /// Read-only admission. The sole runner calls this before inputs and again before its clock.
    pub fn validate_construction_tick(&self) -> Result<(), String> {
        if self.validate_actor_mode().is_err()
            || self.options.mode == Mode::World
            || self.world.game.tick < 0
        {
            return Err("construction phases require ordinary Conquest".into());
        }
        self.validate_building_activity_rows()?;
        for (&id, unit) in &self.world.game.units {
            let control = &self.commands.units[&id];
            for waiting in &control.orders {
                match waiting {
                    crate::match_commands::Command::Assist {
                        ids,
                        target,
                        at,
                        queue,
                    } => {
                        if ids.as_slice() != [i64::from(id)]
                            || *queue
                            || *target <= 0
                            || *target > i64::from(u32::MAX)
                            || at.is_none_or(|p| !p.x.is_finite() || !p.z.is_finite())
                        {
                            return Err("unnormalized waiting Assist".into());
                        }
                    }
                    crate::match_commands::Command::Build { .. } => {
                        return Err(
                            "waiting Build must already own a paid site and normalized Assist"
                                .into(),
                        );
                    }
                    _ => {}
                }
            }
            if unit.id != id {
                return Err("invalid construction waiting actor".into());
            }
        }
        Ok(())
    }

    pub fn begin_construction_tick(&self) -> Result<ConstructionFrame, String> {
        self.validate_construction_tick()?;
        Ok(ConstructionFrame {
            tick: self.world.game.tick,
            sites: Vec::new(),
            indexes: BTreeMap::new(),
            workers: BTreeSet::new(),
        })
    }

    /// Source 5375: after timers/autocast and before projects/Dig in this actor's actual turn.
    pub fn step_construction_worker(
        &mut self,
        id: UnitId,
        frame: &mut ConstructionFrame,
    ) -> Result<(), String> {
        if self.validate_actor_mode().is_err() || self.options.mode == Mode::World {
            return Err("construction worker requires ordinary Conquest".into());
        }
        if frame.tick != self.world.game.tick || frame.workers.contains(&id) {
            return Err("stale or repeated construction worker turn".into());
        }
        self.validate_construction_actor(id)?;
        let unit = self
            .world
            .game
            .units
            .get(&id)
            .ok_or("missing construction worker")?;
        let control = self
            .commands
            .units
            .get(&id)
            .ok_or("missing construction control")?;
        validate_build_activity(unit, control)?;
        if unit.hp <= 0.0 || control.build_target.is_none() {
            frame.workers.insert(id);
            return Ok(());
        }
        let target = control.build_target.unwrap();
        let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
        let Some(site) = self.world.game.units.get(&target) else {
            clear_build_activity(
                self.world.game.units.get_mut(&id).unwrap(),
                self.commands.units.get_mut(&id).unwrap(),
            )?;
            frame.workers.insert(id);
            return Ok(());
        };
        let definition = catalog
            .unit(&self.rules.kinds[site.kind].name)
            .map_err(|e| e.to_string())?;
        if site.hp <= 0.0
            || site.built.unwrap_or(0.0) >= 1.0 && site.hp >= number(definition, "hpPer")?
        {
            clear_build_activity(
                self.world.game.units.get_mut(&id).unwrap(),
                self.commands.units.get_mut(&id).unwrap(),
            )?;
            frame.workers.insert(id);
            return Ok(());
        }
        let reach = number(definition, "radius")?
            + number(
                catalog
                    .table("CFG")
                    .map_err(|e| e.to_string())?
                    .get("classic")
                    .ok_or("missing construction rules")?,
                "buildReach",
            )?;
        if math::hypot(unit.x - site.x, unit.z - site.z) <= reach {
            let unit = self.world.game.units.get_mut(&id).unwrap();
            if !unit.path.is_empty() {
                unit.path_gen = unit
                    .path_gen
                    .checked_add(1)
                    .ok_or("construction path identity exhausted")?;
                unit.path.clear();
            }
            let index = match frame.indexes.get(&target) {
                Some(&index) => index,
                None => {
                    let index = frame.sites.len();
                    frame.sites.push((target, Vec::new()));
                    frame.indexes.insert(target, index);
                    index
                }
            };
            frame.sites[index].1.push(id);
        } else if unit.path.is_empty() && unit.repath <= 0.0 {
            self.request_command_path(id, site.at(), crate::path_requests::RequestKind::Build)?;
        }
        frame.workers.insert(id);
        Ok(())
    }

    /// Source 5585: finish captured crews before the late unit list and crowd separation.
    /// Captured workers are not filtered again by health, pose or current assignment.
    pub fn finish_construction_tick(&mut self, frame: ConstructionFrame) -> Result<(), String> {
        if frame.tick != self.world.game.tick {
            return Err("stale construction crew frame".into());
        }
        self.validate_match_finish_state()?;
        let dt = self.rules.movement.tick;
        if !dt.is_finite() || dt <= 0.0 {
            return Err("invalid construction interval".into());
        }
        let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
        let classic = catalog
            .table("CFG")
            .map_err(|e| e.to_string())?
            .get("classic")
            .ok_or("missing construction rules")?;
        let exponent = number(classic, "crew")?;
        let mut plans = Vec::new();
        for (id, workers) in frame.sites {
            let Some(site) = self.world.game.units.get(&id) else {
                continue;
            };
            if site.hp <= 0.0 {
                continue;
            }
            let definition = catalog
                .unit(&self.rules.kinds[site.kind].name)
                .map_err(|e| e.to_string())?;
            let full = number(definition, "hpPer")?;
            let built = site.built.ok_or("missing construction progress")?;
            let crew = math::pow(workers.len() as f64, exponent);
            let (progress, amount) = if built >= 1.0 {
                let mut recovery = 0.0;
                let mut available = 0.0;
                for &worker in &workers {
                    if !self.world.game.units.contains_key(&worker) {
                        return Err("removed captured construction worker is unsupported".into());
                    }
                    recovery += self.logistics.recovery_rate(worker);
                    if let Some(reserve) = self.logistics.get(worker) {
                        if !reserve.provisions.is_finite() || reserve.provisions < 0.0 {
                            return Err("invalid construction provisions".into());
                        }
                        available += reserve.provisions * 5.0;
                    } else {
                        available = f64::INFINITY;
                    }
                }
                let cap = if self.elimination_sudden_death() {
                    number(classic, "decay")? * 0.6
                } else {
                    f64::INFINITY
                };
                let amount = math::min(
                    available,
                    full * math::min(
                        number(classic, "repair")? * crew * (recovery / workers.len() as f64),
                        cap,
                    ) * dt,
                );

                (built, amount)
            } else {
                if self.elimination_sudden_death() {
                    continue;
                }
                let time = definition
                    .get("buildTime")
                    .and_then(Value::number)
                    .unwrap_or(1.0);
                if !time.is_finite() || time <= 0.0 {
                    return Err("invalid construction duration".into());
                }
                let rate = crew / time * dt;
                (math::min(1.0, built + rate), full * 0.75 * rate)
            };
            if ![progress, amount].iter().all(|v| v.is_finite()) || amount < 0.0 {
                return Err("invalid construction rate".into());
            }
            plans.push((id, workers, built, progress, amount));
        }
        let mut capacity = 0u64;
        for &(id, _, _, _, amount) in &plans {
            capacity = capacity
                .checked_add(self.preflight_building_repair(id, amount)?)
                .ok_or("construction mutation capacity exhausted")?;
        }
        if capacity != 0 {
            self.world
                .mutation_capacity(capacity)
                .map_err(|e| e.to_string())?;
            self.world
                .journal_end()
                .map_err(|e| e.to_string())?
                .checked_add(capacity)
                .ok_or("construction journal capacity exhausted")?;
        }
        let mut story = self.story.clone();
        let changes = plans
            .iter()
            .filter(|p| p.2 < 1.0 && p.3 >= 1.0)
            .map(|p| crate::match_story::StoryChange {
                slot: Some(self.world.game.units[&p.0].owner),
                key: crate::match_story::StoryKey::Built,
                amount: 1.0,
            })
            .collect::<Vec<_>>();
        crate::match_story::prepare_story_updates(&mut story, self.finish.is_some(), &changes)?
            .commit();
        for (id, workers, built, progress, amount) in plans {
            if built < 1.0 {
                self.world.game.units.get_mut(&id).unwrap().built = Some(progress);
                if progress >= 1.0 {
                    self.record_built(self.world.game.units[&id].owner)?;
                }
            }
            let mut repaired = self.repair_building_sections(id, amount)?;
            if built >= 1.0 {
                for worker in workers {
                    let Some(reserve) = self.logistics.get_mut(worker) else {
                        break;
                    };
                    let paid = math::min(repaired, reserve.provisions * 5.0);
                    reserve.provisions -= paid / 5.0;
                    repaired -= paid;
                }
            }
        }
        Ok(())
    }
}

/// Local preparation for an accepted rare placement or assignment. These values
/// are published together or discarded; they never become another live owner.
struct ConstructionPreview {
    physical: crate::world::WorldTransaction,
    grid: crate::grid::SpatialGrid,
    buildings: BTreeMap<usize, crate::match_runtime::BuildingCell>,
    recipients: Vec<crate::match_routing::RecipientRouting>,
    fog: crate::fog::Fog,
    paths: crate::pathfinding::Pathfinding,
    controls: BTreeMap<UnitId, UnitControl>,
    routed: bool,
}
impl Match {
    fn construction_selection(
        &self,
        owner: usize,
        ids: &[i64],
        command: &crate::match_commands::Command,
    ) -> Result<Vec<UnitId>, String> {
        self.validate_construction_tick()?;
        self.validate_match_finish_state()?;
        if owner >= self.world.game.players.len()
            || !self.start.economy.players[owner].mp.is_finite()
        {
            return Err("invalid construction owner/wallet".into());
        }
        let selected = ids
            .iter()
            .take(50.max(self.world.game.units.len()))
            .filter_map(|&id| self.eligible_command_unit(id, owner, command))
            .collect::<Vec<_>>();
        for &id in &selected {
            let u = &self.world.game.units[&id];
            if u.dig != self.field_works.is_active(id)
                || u.entrench != self.field_works.membership(id).is_some()
            {
                return Err("inconsistent construction field work".into());
            }
        }
        Ok(selected)
    }
    fn construction_denial(
        &mut self,
        selected: &[UnitId],
        denial: crate::match_commands::Denial,
    ) -> Result<Option<crate::match_commands::Denial>, String> {
        for &id in selected {
            self.commands.units.get_mut(&id).unwrap().follow = 0;
        }
        Ok(Some(denial))
    }
    fn construction_preview(&self) -> Result<ConstructionPreview, String> {
        self.validate_routing_context()?;
        for owner in 0..self.world.game.players.len() {
            self.validate_recipient(owner)?;
        }
        let recipients = self.recipients.clone();
        let mut paths = self.commands.paths.fork_for_terrain(
            self.recipients[0].query().navigation(),
            recipients[0].query().navigation(),
        );
        for (original, prepared) in self.recipients.iter().zip(&recipients).skip(1) {
            paths.rebind_terrain(original.query().navigation(), prepared.query().navigation());
        }
        Ok(ConstructionPreview {
            physical: self.world.begin_transaction().map_err(|e| e.to_string())?,
            grid: self.grid.clone_for_transaction(),
            buildings: self.building_cells.clone(),
            recipients,
            fog: self.routing_fog.clone(),
            paths,
            controls: self.commands.units.clone(),
            routed: false,
        })
    }
    fn construction_assign(
        &self,
        preview: &mut ConstructionPreview,
        selected: &[UnitId],
        crew: &[UnitId],
        target: UnitId,
        queue: bool,
    ) -> Result<Vec<(Unit, UnitControl)>, String> {
        use crate::match_commands::{Command, MoveOutcome};
        for &id in selected {
            preview.controls.get_mut(&id).unwrap().follow = 0;
        }
        let at = preview
            .physical
            .world
            .game
            .units
            .get(&target)
            .ok_or("missing staged site")?
            .at();
        let mut replaced = Vec::new();
        for &id in crew {
            if queue {
                let c = preview.controls.get_mut(&id).unwrap();
                if c.orders.len() < 8 {
                    c.orders.push_back(Command::Assist {
                        ids: vec![i64::from(id)],
                        target: i64::from(target),
                        at: Some(at),
                        queue: false,
                    });
                }
                continue;
            }
            let mut unit = preview.physical.world.game.units[&id].clone();
            let mut control = preview.controls[&id].clone();
            if unit.air.is_some() || unit.riding {
                return Err("construction exit requires its represented physical actuator".into());
            }
            unit.path_gen = unit
                .path_gen
                .checked_add(1)
                .ok_or("construction path identity exhausted")?;
            // Keep original rows for the infallible sole-owner cancellation tail.
            if !replaced
                .iter()
                .any(|(u, _): &(Unit, UnitControl)| u.id == id)
            {
                replaced.push((
                    self.world.game.units[&id].clone(),
                    self.commands.units[&id].clone(),
                ));
            }
            if unit.garrison.is_some() {
                let game = &preview.physical.world.game;
                let cell = crate::unit_spawn::SpawnPlacer::new()
                    .nearest_free(
                        crate::match_runtime::terrain_view(game),
                        unit.at(),
                        crate::game::MOVE,
                        None,
                    )
                    .map_err(|e| format!("construction garrison exit: {e:?}"))?;
                unit.x = (cell % game.w) as f64 * crate::game::CELL + crate::game::CELL / 2.0;
                unit.z = (cell / game.w) as f64 * crate::game::CELL + crate::game::CELL / 2.0;
                unit.garrison = None;
                preview.physical.world.game.units.insert(id, unit.clone());
                preview.grid.update(id, &unit);
            }
            preview.routed = true;
            let receipt = crate::match_routing::refresh_recipient_parts(
                crate::match_routing::RecipientObservation {
                    world: &preview.physical.world,
                    rules: &self.rules,
                    building_cells: &preview.buildings,
                    environment: &self.environment,
                },
                &mut preview.fog,
                &mut preview.recipients[unit.owner],
                unit.owner,
            )?;
            receipt.commit(&mut preview.physical.world)?;
            let recipient = &preview.recipients[unit.owner];
            let query = recipient.query();
            let terrain = query
                .path_terrain(recipient.live())
                .map_err(|e| format!("construction route: {e:?}"))?;
            let route = crate::match_commands::search_route(
                &mut preview.paths,
                terrain,
                crate::pathfinding::PathRules::compiled().map_err(|e| e.to_string())?,
                &self.rules.kinds[unit.kind],
                &unit,
                at,
            )?
            .result;
            unit.traffic = None;
            unit.traffic_wait = 0.0;
            unit.world_goal = None;
            unit.entrench = false;
            unit.build = true;
            unit.attack = false;
            unit.dig = false;
            unit.board = 0;
            unit.amove = None;
            unit.repath = 0.0;
            unit.path = route.waypoints;
            control.outcome = Some(MoveOutcome::Interrupted);
            control.outcome_tick = Some(self.world.game.tick);
            control.orders.clear();
            control.face = None;
            control.build_target = Some(target);
            control.attack_id = 0;
            control.nade = None;
            control.enter = None;
            control.fire_at = None;
            control.route_end = Some(route.route_end);
            preview.physical.world.game.units.insert(id, unit);
            preview.controls.insert(id, control);
        }
        Ok(replaced)
    }
    fn publish_construction_preview(
        &mut self,
        preview: ConstructionPreview,
        mut replaced: Vec<(Unit, UnitControl)>,
        born: Option<(UnitId, u32, crate::production::ProductionState)>,
        owner: usize,
        charge: f64,
    ) -> Result<(), String> {
        use crate::match_story::{StoryChange, StoryKey, prepare_story_updates};
        let ConstructionPreview {
            physical,
            grid,
            buildings,
            recipients,
            fog,
            paths,
            controls,
            routed,
        } = preview;
        let physical = physical
            .prepare_publication(&mut self.world)
            .map_err(|e| e.to_string())?;
        let story = prepare_story_updates(
            &mut self.story,
            self.finish.is_some(),
            &[StoryChange {
                slot: Some(owner),
                key: StoryKey::MpSpent,
                amount: charge,
            }],
        )?;
        // Every fallible guard has completed. The exclusive tail publishes only prepared state.
        for (unit, control) in &mut replaced {
            self.commands.requests.clear_failures(unit.id);
            crate::match_runtime::cancel_owned_work_state(&mut self.field_works, unit, control)
                .expect("construction replacement was preflighted");
        }
        physical.commit();
        self.grid = grid;
        self.building_cells = buildings;
        if routed {
            self.recipients = recipients;
            self.routing_fog = fog;
            self.commands.paths = paths;
        }
        self.commands.units = controls;
        if let Some((id, next_id, production)) = born {
            self.next_unit_id = next_id;
            self.production = production;
            self.building_rallies.insert(id, None);
            self.combat.initialize(id);
            self.firing.initialize(id);
            self.abilities.initialize(id);
            self.recovery.initialize(id);
        }
        self.start.economy.players[owner].mp -= charge;
        story.commit();
        Ok(())
    }

    /// Assign a source-legal builder to an existing allied living site. The
    /// normalized waiting order retains its actual site anchor and numeric identity.
    pub fn execute_assist(
        &mut self,
        owner: usize,
        ids: &[i64],
        target: i64,
        queue: bool,
    ) -> Result<Option<crate::match_commands::Denial>, String> {
        use crate::match_commands::{Command, Denial};
        let command = Command::Assist {
            ids: ids.to_vec(),
            target,
            at: None,
            queue,
        };
        let selected = self.construction_selection(owner, ids, &command)?;
        let Some(target) = u32::try_from(target)
            .ok()
            .filter(|id| self.world.game.units.contains_key(id))
        else {
            return Ok(Some(Denial::Unseen));
        };
        let site = &self.world.game.units[&target];
        let definition = Catalog::compiled()
            .map_err(|e| e.to_string())?
            .unit(&self.rules.kinds[site.kind].name)
            .map_err(|e| e.to_string())?;
        if site.hp <= 0.0
            || !self.rules.kinds[site.kind].building
            || !self.world.game.allied(site.owner, owner)
            || (site.built.unwrap_or(0.0) >= 1.0 && site.hp >= number(definition, "hpPer")?)
        {
            return Ok(Some(Denial::Unseen));
        }
        if site
            .built
            .is_none_or(|v| !v.is_finite() || !(0.0..=1.0).contains(&v))
        {
            return Err("unrepresented construction site progress".into());
        }
        let types = crate::economy::builder_types(self.options.mode).map_err(|e| e.to_string())?;
        let builders = selected
            .iter()
            .copied()
            .filter(|id| {
                types.contains(
                    &self.rules.kinds[self.world.game.units[id].kind]
                        .name
                        .as_str(),
                )
            })
            .collect::<Vec<_>>();
        let crew = builders
            .iter()
            .copied()
            .filter(|id| {
                !self.world.game.units[id].retreating
                    && (!queue || self.commands.units[id].orders.len() < 8)
            })
            .collect::<Vec<_>>();
        if crew.is_empty() {
            let denial = if builders
                .iter()
                .any(|id| !self.world.game.units[id].retreating)
            {
                Denial::QueueFull
            } else if !builders.is_empty() {
                Denial::Retreating
            } else {
                Denial::NoBuilders
            };
            return self.construction_denial(&selected, denial);
        }
        let mut preview = self.construction_preview()?;
        let replaced = self.construction_assign(&mut preview, &selected, &crew, target, queue)?;
        self.publish_construction_preview(preview, replaced, None, owner, 0.0)?;
        Ok(None)
    }
}

impl Match {
    /// Pay and stamp an admitted static site before computing any crew route.
    /// Depot nodes, coastal producers, caches and construction modes require their
    /// canonical providers and are refused explicitly by this ordinary caller.
    pub fn execute_build(
        &mut self,
        owner: usize,
        ids: &[i64],
        name: &str,
        at: Option<crate::game::Point>,
        direction: Option<f64>,
        queue: bool,
    ) -> Result<Option<crate::match_commands::Denial>, String> {
        use crate::{
            game::{CELL, MOVE, Point},
            match_commands::{Command, Denial},
            match_runtime::{foot_center, footprint},
            unit_publication::StagedUnit,
        };
        let command = Command::Build {
            ids: ids.to_vec(),
            kind: name.into(),
            at,
            direction,
            queue,
        };
        let selected = self.construction_selection(owner, ids, &command)?;
        if self.elimination_sudden_death() {
            return Ok(Some(Denial::SuddenDeath));
        }
        if !crate::economy::build_kinds(self.options.mode)
            .map_err(|e| e.to_string())?
            .contains(&name)
        {
            return Ok(Some(Denial::Blocked));
        }
        if matches!(name, "depot" | "shipyard" | "supplycache") {
            return Ok(Some(Denial::Unsupported));
        }
        let types = crate::economy::builder_types(self.options.mode).map_err(|e| e.to_string())?;
        let builders = selected
            .iter()
            .copied()
            .filter(|id| {
                types.contains(
                    &self.rules.kinds[self.world.game.units[id].kind]
                        .name
                        .as_str(),
                )
            })
            .collect::<Vec<_>>();
        let engineers = builders
            .iter()
            .copied()
            .filter(|id| !self.world.game.units[id].retreating)
            .collect::<Vec<_>>();
        let crew = engineers
            .iter()
            .copied()
            .filter(|id| !queue || self.commands.units[id].orders.len() < 8)
            .collect::<Vec<_>>();
        if crew.is_empty() {
            let denial = if !engineers.is_empty() {
                Denial::QueueFull
            } else if !builders.is_empty() {
                Denial::Retreating
            } else {
                Denial::NoBuilders
            };
            return self.construction_denial(&selected, denial);
        }
        let Some(at) = at.filter(|p| p.x.is_finite() && p.z.is_finite()) else {
            return self.construction_denial(&selected, Denial::Blocked);
        };
        let at = Point {
            x: math::min(self.world.game.w as f64 * CELL, math::max(0.0, at.x)),
            z: math::min(self.world.game.h as f64 * CELL, math::max(0.0, at.z)),
        };
        let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
        let definition = catalog.unit(name).map_err(|e| e.to_string())?;
        let cost = number(definition, "cost")?;
        let wallet = self
            .start
            .economy
            .players
            .get(owner)
            .ok_or("invalid construction owner")?;
        if !wallet.mp.is_finite() || cost < 0.0 {
            return Err("invalid construction wallet".into());
        }
        if wallet.mp < cost {
            return self.construction_denial(&selected, Denial::Manpower);
        }
        if let Some(needs) = definition.get("needs").and_then(Value::string)
            && !self.world.game.units.values().any(|u| {
                u.owner == owner
                    && self.rules.kinds[u.kind].name == needs
                    && u.hp > 0.0
                    && u.built.is_some_and(|b| b >= 1.0)
            })
        {
            return self.construction_denial(&selected, Denial::Needs);
        }
        if name == "armory" && !self.options.tech {
            return self.construction_denial(&selected, Denial::Blocked);
        }
        if self.options.tech
            && crate::economy::tier_of(name).map_err(|e| e.to_string())?
                > self
                    .production
                    .player(owner)
                    .ok_or("missing construction research")?
                    .tech()
                    .tier()
        {
            return self.construction_denial(&selected, Denial::Tier);
        }
        let size = number(definition, "size")? as usize;
        let x = math::round(at.x / CELL - size as f64 / 2.0);
        let z = math::round(at.z / CELL - size as f64 / 2.0);
        let g = &self.world.game;
        if x < 0.0 || z < 0.0 || x >= g.w as f64 || z >= g.h as f64 {
            return self.construction_denial(&selected, Denial::Blocked);
        }
        let cell = z as usize * g.w + x as usize;
        let Some(cells) = footprint(g, cell, size) else {
            return self.construction_denial(&selected, Denial::Blocked);
        };
        let center = foot_center(g, cell, size);
        if !self.recipient_sees(owner, center)? {
            return self.construction_denial(&selected, Denial::NotVisible);
        }
        self.world
            .preflight_cell_rows(&cells)
            .map_err(|e| e.to_string())?;
        let g = &self.world.game;
        let min = cells
            .iter()
            .map(|&c| g.height.as_ref().map_or(0, |h| h[c]))
            .min()
            .unwrap();
        let top = cells
            .iter()
            .map(|&c| g.height.as_ref().map_or(0, |h| h[c]))
            .max()
            .unwrap();
        let occupied = g.units.values().any(|u| {
            u.hp > 0.0
                && !self.rules.kinds[u.kind].air
                && !u.riding
                && !self.rules.kinds[u.kind].structure
                && u.x >= 0.0
                && u.z >= 0.0
                && u.x < g.w as f64 * CELL
                && u.z < g.h as f64 * CELL
                && cells.contains(
                    &((u.z / CELL).floor() as usize * g.w + (u.x / CELL).floor() as usize),
                )
        });
        if occupied
            || i16::from(top) - i16::from(min) > 1
            || cells
                .iter()
                .any(|&c| g.flags[c] & MOVE != 0 || b"WF=KOAQ".contains(&g.chars[c]))
        {
            return self.construction_denial(&selected, Denial::Blocked);
        }
        let id = self.next_unit_id;
        let next_id = id
            .checked_add(1)
            .filter(|_| id != 0)
            .ok_or("construction unit identity exhausted")?;
        if self.world.game.units.contains_key(&id)
            || self.commands.units.contains_key(&id)
            || self.combat.get(id).is_some()
            || self.firing.get(id).is_some()
            || self.abilities.get(id).is_some()
            || self.recovery.get(id).is_some()
            || self.recruit_metadata.contains_key(&id)
            || self.production.building(id).is_some()
            || self.building_rallies.contains_key(&id)
            || self.aviation.planes.contains_key(&id)
            || self.aviation.anti_air.contains_key(&id)
        {
            return Err("stale construction newborn identity".into());
        }
        self.world
            .preflight_building_stamp(id, &cells)
            .map_err(|e| e.to_string())?;
        let mut production = self.production.clone();
        production
            .initialize_building(id)
            .map_err(|e| e.to_string())?;
        let mut preview = self.construction_preview()?;
        for &c in &cells {
            if preview
                .physical
                .world
                .game
                .height
                .as_ref()
                .map_or(0, |h| h[c])
                < top
            {
                preview
                    .physical
                    .world
                    .set_level(c, top)
                    .map_err(|e| e.to_string())?;
            }
            preview
                .physical
                .world
                .set_cell(c, b'K')
                .map_err(|e| e.to_string())?;
        }
        let kind = self
            .rules
            .kind(name)
            .ok_or("unknown construction definition")?;
        let mut staged = StagedUnit::ordinary(
            &preview.physical.world.game,
            &self.rules,
            owner,
            kind,
            id,
            u32::try_from(preview.physical.world.game.units.len())
                .map_err(|_| "construction scatter capacity exhausted")?,
            self.start.players[owner].spawn,
            Some(center),
        )
        .map_err(|e| format!("construction spawn: {e:?}"))?;
        let full = number(definition, "hpPer")?;
        staged.unit.cells = Some(cells.iter().map(|&c| c as u32).collect());
        staged.unit.built = Some(0.0);
        staged.unit.hp = full * 0.25;
        if definition.get("slit").is_some_and(|v| match v {
            Value::Null => false,
            Value::Bool(v) => *v,
            Value::Number(v) => *v != 0.0 && !v.is_nan(),
            Value::String(v) => !v.is_empty(),
            _ => true,
        }) {
            staged.unit.rot = direction.filter(|v| v.is_finite()).unwrap_or_else(|| {
                math::atan2(
                    center.z - self.start.players[owner].spawn.z,
                    center.x - self.start.players[owner].spawn.x,
                )
            });
        }
        let mut empty_metadata = BTreeMap::new();
        let mut allocator = id;
        crate::unit_publication::prepare_publication(
            crate::unit_publication::PublicationParts {
                units: &mut preview.physical.world.game.units,
                grid: &mut preview.grid,
                controls: &mut preview.controls,
                recruitment: &mut empty_metadata,
                next_id: &mut allocator,
            },
            &self.rules,
            self.world.game.players.len(),
            vec![staged],
        )
        .map_err(|e| format!("construction publication: {e:?}"))?
        .publish();
        for &c in &cells {
            preview.buildings.insert(
                c,
                crate::match_runtime::BuildingCell {
                    unit: id,
                    owner,
                    at: center,
                },
            );
        }
        preview
            .physical
            .world
            .register_building_structure(id, &cells, full * 0.25, full)
            .map_err(|e| e.to_string())?;
        let replaced = self.construction_assign(&mut preview, &selected, &crew, id, queue)?;
        self.publish_construction_preview(
            preview,
            replaced,
            Some((id, next_id, production)),
            owner,
            cost,
        )?;
        Ok(None)
    }
}

#[cfg(test)]
#[path = "match_construction_tests.rs"]
mod tests;
