//! Deterministic Conquest tick scheduling over the canonical Match state.
//!
//! This integration grows by attaching the independently verified phase adapters.
//! Reports identify phases still absent; a clock advance is not playable-match proof.
use std::collections::BTreeMap;

use crate::{game::Pose, grid::UnitId, movement::RouteRequest};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TickPhase {
    Commands,
    StructureSync,
    Clock,
    Debris,
    StorySample,
    WeatherPlan,
    Vision,
    Research,
    Paths,
    UnitReserves,
    UnitOrders,
    Production,
    Operative,
    Abilities,
    FieldWork,
    FieldProjects,
    AirRail,
    Aircraft,
    AntiAir,
    Rail,
    Riding,
    Reactions,
    TrafficKnowledge,
    Movement,
    GroundEffects,
    Firing,
    Construction,
    Separation,
    Projectiles,
    Support,
    SupportTimers,
    Scenario,
    LivingWeather,
    FireFlood,
    Recovery,
    PointSupply,
    SupplyNetwork,
    Reinforcement,
    Death,
    PointCaptureIncome,
    Convoys,
    Victory,
}

/// Persistent simulation state, never wall-clock state. A phase failure poisons
/// the runner because some earlier canonical phases may already have committed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TickPathConfig {
    pub budget: usize,
    pub retry_limit: u32,
}
impl Default for TickPathConfig {
    fn default() -> Self {
        Self {
            budget: 4096,
            retry_limit: 20,
        }
    }
}
impl TickPathConfig {
    fn validate(self) -> Result<(), String> {
        if self.budget as u64 > 9_007_199_254_740_991 {
            return Err("path budget exceeds exact source integer range".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default)]
pub struct TickState {
    pub(crate) fault: Option<String>,
    started: bool,
    paths: TickPathConfig,
}
impl TickState {
    pub fn fault(&self) -> Option<&str> {
        self.fault.as_deref()
    }
    pub fn path_config(&self) -> TickPathConfig {
        self.paths
    }
}

#[derive(Clone, Debug)]
pub struct TickReport {
    pub tick: i64,
    /// Positions from before the increment, used by swept flight contacts.
    pub prior: BTreeMap<UnitId, Pose>,
    /// The late unit list is captured after the unit loop, before crowd separation.
    pub unit_order: Vec<UnitId>,
    pub phases: Vec<TickPhase>,
    pub unavailable: Vec<TickPhase>,
    pub route_requests: Vec<(UnitId, crate::game::Point, RouteRequest)>,
    pub commands: Vec<crate::match_commands::CommandResult>,
    pub paths: crate::path_requests::TickReport,
    pub environment: Vec<crate::environment::Effect>,
    pub retired: Vec<crate::match_combat::RetiredUnit>,
}

enum EnvironmentPhase<'a> {
    StructureSync,
    Debris,
    FireFlood(&'a [UnitId]),
}

impl crate::match_runtime::Match {
    /// Prime a trusted initial scene without executing inputs or simulation phases.
    /// A running/faulted match or any pending recorded input cannot be rewound.
    pub fn initialize_tick_scene(
        &mut self,
        clock: u64,
        paths: TickPathConfig,
    ) -> Result<(), String> {
        if self.world.game.tick != 0
            || self.commands.queue.next_tick() != Some(1)
            || !self.commands.queue.is_empty()
            || self.tick_state.started
            || self.tick_state.fault.is_some()
        {
            return Err("tick scene requires a fresh empty command queue".into());
        }
        if clock >= 9_007_199_254_740_351 {
            return Err("invalid initial tick clock".into());
        }
        paths.validate()?;
        let existing = self.commands.queue.snapshot();
        let queue = crate::orders::OrderQueue::restore(crate::orders::OrderSnapshot {
            version: crate::orders::ORDER_QUEUE_VERSION,
            next_tick: Some(clock + 1),
            capacity: existing.capacity,
            pending: vec![],
        })
        .map_err(|error| format!("initial command boundary: {error:?}"))?;
        self.commands.queue = queue;
        self.world.game.tick = clock as i64;
        self.tick_state.paths = paths;
        Ok(())
    }

    /// Run the connected phase foundation. The returned unavailable list is part
    /// of the result until every actuator required by a playable tick is wired.
    pub fn advance_tick(&mut self) -> Result<TickReport, String> {
        if let Some(fault) = self.tick_state.fault() {
            return Err(format!("tick runner faulted: {fault}"));
        }
        self.validate_tick_input()?;
        self.tick_state.started = true;
        match self.run_tick() {
            Ok(report) => Ok(report),
            Err(error) => {
                self.tick_state.fault = Some(error.clone());
                Err(error)
            }
        }
    }

    fn validate_tick_input(&self) -> Result<(), String> {
        let game = &self.world.game;
        if game.tick < 0 || game.tick >= 9_007_199_254_740_351 {
            return Err("invalid tick clock".into());
        }
        if !matches!(
            self.options.mode,
            crate::economy::Mode::Default | crate::economy::Mode::Conquest
        ) || game.mode.is_some()
        {
            return Err("tick execution currently requires ordinary Conquest".into());
        }
        if self.options.logistics || self.convoys.is_some() {
            return Err("reserve and convoy tick phases are not connected".into());
        }
        self.tick_state.paths.validate()?;
        let boundary = game.tick as u64 + 1;
        self.validate_command_tick(boundary)?;
        if !self.rules.movement.tick.is_finite() || self.rules.movement.tick <= 0.0 {
            return Err("invalid tick duration".into());
        }
        if self.start.players.len() != game.players.len()
            || self.start.economy.players.len() != game.players.len()
            || self.capture_ticks.len() != self.start.points.len()
        {
            return Err("invalid tick player state".into());
        }
        if self.start.living.is_some() != self.options.living_weather
            || self.start.living.is_some() && game.wet.is_none()
        {
            return Err("invalid living weather state".into());
        }
        for u in game.units.values() {
            if u.kind >= self.rules.kinds.len() || u.owner >= game.players.len() {
                return Err("invalid tick unit".into());
            }
        }
        self.validate_abilities()?;
        self.prepare_companion_tick()?;
        self.validate_aircraft_runner_boundary(boundary)
            .map_err(|error| format!("aircraft admission: {error}"))?;
        let current_order = game.units.keys().copied().collect::<Vec<_>>();
        self.validate_support_metadata()
            .map_err(|error| format!("support metadata admission: {error}"))?;
        self.validate_support_tick_phases(&current_order, self.rules.movement.tick, boundary)
            .map_err(|error| format!("support admission: {error}"))?;
        self.validate_carrier_tick()?;
        self.validate_reaction_tick_actors()?;
        self.validate_field_project_tick(&current_order)?;
        if self
            .field_works
            .jobs()
            .keys()
            .any(|id| !game.units.contains_key(id))
            || game
                .units
                .values()
                .any(|unit| unit.dig != self.field_works.is_active(unit.id))
        {
            return Err("invalid tick field-work activity".into());
        }
        let mut identities = std::collections::BTreeSet::new();
        for smoke in &game.smokes {
            if !identities.insert(smoke.id) || smoke.remaining.is_none_or(|v| !v.is_finite()) {
                return Err("invalid runtime smoke lifetime or identity".into());
            }
        }
        Ok(())
    }

    fn validate_reaction_tick_actors(&self) -> Result<(), String> {
        for (&id, unit) in &self.world.game.units {
            if unit.hp <= 0.0 || self.commands.units.get(&id).is_some_and(|c| c.operative) {
                continue;
            }
            let kind = self
                .rules
                .kinds
                .get(unit.kind)
                .ok_or("invalid reaction kind")?;
            if kind.air || unit.air.is_some() {
                continue;
            }
            if kind.rail {
                return Err("rail tick actor is not connected".into());
            }
            // The canonical carrier preflight admits riding; Source skips this
            // ordinary reaction/timer block after the early pose update.
            if unit.riding {
                continue;
            }
            if unit.garrison.is_some() && self.automatic_retreat_candidate(id)? {
                return Err("automatic garrison retreat tick exit is not connected".into());
            }
            if unit.build && self.automatic_retreat_candidate(id)? {
                return Err("construction retreat tick exit is not connected".into());
            }
        }
        Ok(())
    }

    fn run_tick(&mut self) -> Result<TickReport, String> {
        use crate::{movement, separation, vision, weather_runtime};
        let dt = self.rules.movement.tick;
        let winner_decided = self.finish.is_some();
        // Boundary orders carry the previous clock's timestamps and view keys.
        let boundary = self.world.game.tick as u64 + 1;
        let commands = self.apply_commands(boundary, winner_decided)?;
        // Boundary joins are included in prior poses and the same live unit loop.
        self.prepare_companion_tick()?;
        self.validate_aircraft_runner_state(boundary)
            .map_err(|error| format!("aircraft published state: {error}"))?;
        self.validate_carrier_tick()?;
        self.validate_reaction_tick_actors()?;
        let current_order = self.world.game.units.keys().copied().collect::<Vec<_>>();
        self.validate_support_metadata()
            .map_err(|error| format!("support published metadata: {error}"))?;
        self.validate_support_tick_phases(&current_order, dt, boundary)
            .map_err(|error| format!("support published state: {error}"))?;
        self.validate_field_project_tick(&current_order)?;
        let mut environment = self.tick_environment_phase(EnvironmentPhase::StructureSync)?;
        self.grid.rebuild(&self.world.game.units);
        self.capture_firing_prior()?;
        let prior = self
            .world
            .game
            .units
            .iter()
            .map(|(&id, u)| (id, u.pose()))
            .collect();
        self.world.game.tick += 1;
        let tick = self.world.game.tick;
        let mut phases = vec![
            TickPhase::Commands,
            TickPhase::StructureSync,
            TickPhase::Clock,
        ];
        self.respawn_companions(winner_decided)?;
        phases.push(TickPhase::Operative);
        environment.extend(self.tick_environment_phase(EnvironmentPhase::Debris)?);
        phases.push(TickPhase::Debris);
        self.sample_conquest_tick()?;
        phases.push(TickPhase::StorySample);
        self.start.weather.step(tick as u64);
        self.world.game.weather = Some(self.start.weather.now as usize);
        phases.push(TickPhase::WeatherPlan);
        if tick % 4 == 1 {
            vision::update_vision(&mut self.world.game, &self.rules, &self.grid);
            phases.push(TickPhase::Vision);
        }
        // Source paid counts/crew sweep precede the path scheduler and unit queues.
        self.recount_field_projects()?;
        phases.push(TickPhase::FieldProjects);
        let config = self.tick_state.paths;
        let paths = self.begin_path_tick(config.budget, config.retry_limit)?;
        phases.push(TickPhase::Paths);
        let claim_order = self.world.game.units.keys().copied().collect::<Vec<_>>();
        self.begin_firing_tick(&claim_order)?;
        let mut moved = Vec::new();
        let mut last_id = None;
        loop {
            // The source iterates its live Map. Monotonic IDs let births made
            // during a unit's turn enter this same loop without a stale ID list.
            let next = match last_id {
                Some(id) => self
                    .world
                    .game
                    .units
                    .range((std::ops::Bound::Excluded(id), std::ops::Bound::Unbounded))
                    .next(),
                None => self.world.game.units.iter().next(),
            };
            let Some((&id, unit)) = next else { break };
            last_id = Some(id);
            if unit.hp <= 0.0 {
                continue;
            }
            if self
                .commands
                .units
                .get(&id)
                .is_some_and(|control| control.operative)
            {
                self.step_companion_turn(id, winner_decided)?;
                continue;
            }
            let factor = movement::supp_speed(unit.supp);
            // Reserve adapters attach before this pre-timer order phase.
            self.prepare_unit_commands(id, winner_decided)?;
            let unit = self.world.game.units.get(&id).expect("current unit");
            let kind = &self.rules.kinds[unit.kind];
            if kind.building && (!kind.weapon || unit.built.is_some_and(|built| built < 1.0)) {
                continue;
            }
            if self
                .step_aircraft_turn(id, dt)
                .map_err(|error| format!("aircraft turn: {error}"))?
            {
                continue;
            }
            match self.apply_unit_riding(id, dt)? {
                crate::match_carriers::RidingOutcome::Riding => continue,
                crate::match_carriers::RidingOutcome::OnFoot => {}
                crate::match_carriers::RidingOutcome::UnsupportedEjection => {
                    return Err("thrown carrier ejection tick phase is not connected".into());
                }
            }
            let unit = self.world.game.units.get(&id).expect("current unit");
            if !movement::reaches_block(&self.rules, unit) {
                continue;
            }
            let rally = if crate::vision::in_trench(&self.world.game, &self.rules, unit) {
                self.rules.movement.trench_rally
            } else {
                8.0
            };
            let infantry = self.rules.kinds[unit.kind].infantry;
            let unit = self.world.game.units.get_mut(&id).expect("current unit");
            if infantry {
                unit.supp = crate::math::max(0.0, unit.supp - rally * dt);
            }
            unit.repath -= dt;
            unit.sprint -= dt;
            self.advance_firing_timers(id, dt)?;
            self.advance_ability_timers(id, dt)?;
            self.decrement_reaction_timer(id, dt)?;
            self.automatic_retreat_unit(id)?;
            self.apply_autocast(id)?;
            self.apply_project_unit(id)?;
            self.apply_field_work_unit(id)?;
            self.step_aimed_ability(id)?;
            // Entry-building execution remains excluded. Boarding uses its live
            // owner-view goal and the one already-started path scheduler.
            if matches!(
                self.apply_unit_boarding_with_routes(id)?,
                crate::match_carriers::BoardingOutcome::Boarded { .. }
            ) {
                continue;
            }
            self.apply_attack_move(id)?;
            self.react_unit(id)?;
            let before = self.world.game.units[&id].at();
            if let Some(result) = self.move_unit_with_command_routes(id, factor)? {
                moved.push(result);
            }
            let unit = &self.world.game.units[&id];
            let moving = !unit.path.is_empty()
                || crate::math::hypot(unit.x - before.x, unit.z - before.z) > 0.001;
            self.apply_unit_firing(id, moving, dt)?;
        }
        phases.extend([
            TickPhase::UnitOrders,
            TickPhase::Abilities,
            TickPhase::FieldWork,
            TickPhase::Riding,
            TickPhase::Aircraft,
            TickPhase::Reactions,
            TickPhase::Movement,
            TickPhase::Firing,
        ]);
        let unit_order: Vec<_> = self.world.game.units.keys().copied().collect();
        separation::separate_crowds(&mut self.world.game, &self.rules, &mut self.grid);
        movement::infantry_progress(&mut self.world.game, &self.rules, &moved);
        phases.push(TickPhase::Separation);
        self.step_firing_flights()?;
        self.advance_firing_grenades(dt)?;
        self.advance_firing_salvos(dt)?;
        phases.push(TickPhase::Projectiles);
        // Source strikes and AA share captured late shooters; current target/grid
        // facts stay live. Radio is read before the later point update.
        self.step_support_strikes(&unit_order, dt)
            .map_err(|error| format!("support strikes: {error}"))?;
        self.step_aircraft_anti_air(&unit_order, dt)
            .map_err(|error| format!("aircraft anti-air: {error}"))?;
        self.advance_support_timers(dt)
            .map_err(|error| format!("support timers: {error}"))?;
        phases.extend([
            TickPhase::Support,
            TickPhase::AntiAir,
            TickPhase::SupportTimers,
        ]);

        let game = &mut self.world.game;
        let mut smokes: Vec<_> = game
            .smokes
            .iter_mut()
            .map(|cloud| weather_runtime::Smoke {
                x: &mut cloud.x,
                z: &mut cloud.z,
                remaining: cloud.remaining.as_mut().expect("validated smoke lifetime"),
            })
            .collect();
        weather_runtime::advance(
            tick as u64,
            dt,
            Some(self.start.weather.now),
            weather_runtime::Environment {
                seed: &mut self.start.environment_seed,
                wind_angle: &mut self.start.wind.0,
                wind_strength: &mut self.start.wind.1,
                showers: self
                    .start
                    .living
                    .as_mut()
                    .map(|living| weather_runtime::Showers {
                        raining: &mut living.raining,
                        next: &mut living.next,
                        rain: &mut game.rain,
                        wet: game
                            .wet
                            .as_mut()
                            .expect("initialized living weather wetness"),
                    }),
            },
            &mut smokes,
        )
        .map_err(|e| format!("living weather: {e:?}"))?;
        drop(smokes);
        phases.push(TickPhase::LivingWeather);
        // Expiring smoke still blocks ignition during the source fire phase.
        environment.extend(self.tick_environment_phase(EnvironmentPhase::FireFlood(&unit_order))?);
        self.world
            .game
            .smokes
            .retain(|s| s.remaining.is_some_and(|t| t > 0.0));
        phases.push(TickPhase::FireFlood);
        let recovery = self.prepare_recovery(&unit_order)?;
        self.heal_and_sweep(&recovery, dt)?;
        phases.push(TickPhase::Recovery);
        self.apply_point_supply(&unit_order)?;
        phases.push(TickPhase::PointSupply);
        // Network refresh attaches between point supply and prepared reinforcement.
        self.reinforce(&recovery, dt)?;
        phases.push(TickPhase::Reinforcement);
        let death = self.tick_retirement(&unit_order)?;
        environment.extend(death.shots);
        phases.push(TickPhase::Death);
        self.apply_point_capture_income(&unit_order)?;
        phases.push(TickPhase::PointCaptureIncome);
        self.apply_conquest_victory()?;
        phases.push(TickPhase::Victory);
        Ok(TickReport {
            tick,
            prior,
            unit_order,
            phases,
            route_requests: Vec::new(),
            commands,
            paths,
            environment,
            retired: death.retired,
            unavailable: vec![
                TickPhase::Research,
                TickPhase::UnitReserves,
                TickPhase::Production,
                TickPhase::Rail,
                TickPhase::TrafficKnowledge,
                TickPhase::GroundEffects,
                TickPhase::Construction,
                TickPhase::Scenario,
                TickPhase::SupplyNetwork,
                TickPhase::Convoys,
            ],
        })
    }

    fn tick_retirement(
        &mut self,
        unit_order: &[UnitId],
    ) -> Result<crate::match_combat::DeathResult, String> {
        // Building collapse can kill a later captured unit during cleanup, so
        // preflight every potential callback before the first removal.
        let facts = unit_order
            .iter()
            .filter_map(|id| self.world.game.units.get(id))
            .map(|unit| {
                let kind = self
                    .rules
                    .kinds
                    .get(unit.kind)
                    .ok_or("invalid death kind")?;
                let metadata = self.combat.get(unit.id).ok_or("missing death metadata")?;
                Ok((
                    crate::match_story::DeathRecord {
                        owner: Some(unit.owner),
                        kind: kind.name.as_str(),
                        at: unit.at(),
                    },
                    metadata
                        .last_hit
                        .and_then(|owner| usize::try_from(owner).ok()),
                ))
            })
            .collect::<Result<Vec<_>, String>>()?;
        self.validate_story_for_deaths(&facts)?;
        self.prepare_companion_deaths(unit_order)?;
        let result = self
            .retire_dead(unit_order)
            .map_err(|e| format!("death cleanup: {e:?}"))?;
        for record in &result.retired {
            self.companion_died(record.unit.id)?;
            let kind = self.rules.kinds[record.unit.kind].name.clone();
            self.record_death(
                crate::match_story::DeathRecord {
                    owner: Some(record.unit.owner),
                    kind: &kind,
                    at: record.unit.at(),
                },
                record
                    .last_hit
                    .and_then(|owner| usize::try_from(owner).ok()),
            )?;
            self.retain_removed_route_unit(&record.unit)?;
        }
        // Active flights retain their removed source's combat XP metadata.
        Ok(result)
    }

    fn tick_environment_phase(
        &mut self,
        phase: EnvironmentPhase<'_>,
    ) -> Result<Vec<crate::environment::Effect>, String> {
        use crate::environment::Context;
        // Environment admission names every current unit. Late objective and
        // retirement consumers retain their separately captured source list.
        let ids: Vec<_> = self.world.game.units.keys().copied().collect();
        let rain = self.world.game.rain;
        let wet = self.world.game.wet.unwrap_or(0.0);
        let tick = self.world.game.tick as u64;
        let mut context = Context {
            tick,
            seed: &mut self.start.environment_seed,
            next_id: &mut self.next_unit_id,
            unit_order: &ids,
            grid: &self.grid,
            rain,
            wet,
            wind_angle: self.start.wind.0,
            wind_strength: self.start.wind.1,
        };
        let mut effects = Vec::new();
        match phase {
            EnvironmentPhase::StructureSync => {
                for &id in &ids {
                    if self
                        .world
                        .structures
                        .iter()
                        .any(|s| s.entity_id == Some(id))
                    {
                        let owner = self.combat.get(id).and_then(|m| m.last_hit);
                        effects.extend(
                            self.environment
                                .sync_building_sections(&mut self.world, id, owner, &mut context)
                                .map_err(|e| format!("building section sync: {e}"))?,
                        );
                    }
                }
            }
            EnvironmentPhase::Debris => {
                self.environment
                    .step_sections(&mut self.world, self.rules.movement.tick, &mut context)
                    .map_err(|e| format!("section debris: {e}"))?;
                self.environment
                    .step_wrecks(&mut self.world, self.rules.movement.tick, &mut context)
                    .map_err(|e| format!("wreck debris: {e}"))?;
            }
            EnvironmentPhase::FireFlood(actor_order) if tick.is_multiple_of(10) => {
                if !self.world.fires.is_empty() {
                    effects.extend(
                        self.environment
                            .burn_captured(
                                &mut self.world,
                                self.rules.movement.tick * 10.0,
                                &mut context,
                                actor_order,
                            )
                            .map_err(|e| format!("burn: {e}"))?,
                    );
                }
                if !self.world.soak().is_empty() {
                    self.environment
                        .flood(&mut self.world)
                        .map_err(|e| format!("flood: {e}"))?;
                }
            }
            EnvironmentPhase::FireFlood(_) => {}
        }
        let mut damage = crate::match_combat::DamageWorld {
            world: &mut self.world,
            combat: &mut self.combat,
            grid: &mut self.grid,
            rules: &self.rules,
            logistics: &self.logistics,
            production: &self.production,
            mode: self.options.mode,
            tech_enabled: self.options.tech,
            environment: &mut self.environment,
            environment_seed: &mut self.start.environment_seed,
            next_id: &mut self.next_unit_id,
            unit_order: &ids,
            wind_angle: self.start.wind.0,
            wind_strength: self.start.wind.1,
        };
        Ok(damage.environment_effects(effects))
    }
}
