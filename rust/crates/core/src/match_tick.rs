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
    Reactions,
    Movement,
    Firing,
    Construction,
    Separation,
    Projectiles,
    Support,
    Scenario,
    LivingWeather,
    FireFlood,
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
#[derive(Clone, Debug, Default)]
pub struct TickState {
    pub(crate) fault: Option<String>,
}
impl TickState {
    pub fn fault(&self) -> Option<&str> {
        self.fault.as_deref()
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
}

impl crate::match_runtime::Match {
    /// Run the connected phase foundation. The returned unavailable list is part
    /// of the result until every actuator required by a playable tick is wired.
    pub fn advance_tick(&mut self) -> Result<TickReport, String> {
        if let Some(fault) = self.tick_state.fault() {
            return Err(format!("tick runner faulted: {fault}"));
        }
        self.validate_tick_input()?;
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
        if game.tick < 0 || game.tick >= 9_007_199_254_740_991 {
            return Err("invalid tick clock".into());
        }
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
        let mut identities = std::collections::BTreeSet::new();
        for smoke in &game.smokes {
            if !identities.insert(smoke.id) || smoke.remaining.is_none_or(|v| !v.is_finite()) {
                return Err("invalid runtime smoke lifetime or identity".into());
            }
        }
        Ok(())
    }

    fn run_tick(&mut self) -> Result<TickReport, String> {
        use crate::{movement, separation, vision, weather_runtime};
        let dt = self.rules.movement.tick;
        self.grid.rebuild(&self.world.game.units);
        let prior = self
            .world
            .game
            .units
            .iter()
            .map(|(&id, u)| (id, u.pose()))
            .collect();
        self.world.game.tick += 1;
        let tick = self.world.game.tick;
        let mut phases = vec![TickPhase::Clock];
        self.sample_conquest_tick()?;
        phases.push(TickPhase::StorySample);
        self.start.weather.step(tick as u64);
        self.world.game.weather = Some(self.start.weather.now as usize);
        phases.push(TickPhase::WeatherPlan);
        if tick % 4 == 1 {
            vision::update_vision(&mut self.world.game, &self.rules, &self.grid);
            phases.push(TickPhase::Vision);
        }
        let mut route_requests = Vec::new();
        let moved = movement::move_units_with_routes(
            &mut self.world.game,
            &self.rules,
            &mut self.grid,
            &mut |_, _, unit, goal, kind| {
                route_requests.push((unit.id, goal, kind));
                unit.repath = 1.0;
            },
        );
        phases.push(TickPhase::Movement);
        let unit_order: Vec<_> = self.world.game.units.keys().copied().collect();
        separation::separate_crowds(&mut self.world.game, &self.rules, &mut self.grid);
        movement::infantry_progress(&mut self.world.game, &self.rules, &moved);
        phases.push(TickPhase::Separation);

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
        game.smokes.retain(|s| s.remaining.is_some_and(|t| t > 0.0));
        phases.push(TickPhase::LivingWeather);
        self.apply_point_supply(&unit_order)?;
        phases.push(TickPhase::PointSupply);
        // Reinforcement and death adapters run between these two point phases.
        self.apply_point_capture_income(&unit_order)?;
        phases.push(TickPhase::PointCaptureIncome);
        self.apply_conquest_victory()?;
        phases.push(TickPhase::Victory);
        Ok(TickReport {
            tick,
            prior,
            unit_order,
            phases,
            route_requests,
            unavailable: vec![
                TickPhase::Commands,
                TickPhase::StructureSync,
                TickPhase::Debris,
                TickPhase::Research,
                TickPhase::Paths,
                TickPhase::UnitReserves,
                TickPhase::UnitOrders,
                TickPhase::Production,
                TickPhase::Reactions,
                TickPhase::Firing,
                TickPhase::Construction,
                TickPhase::Projectiles,
                TickPhase::Support,
                TickPhase::Scenario,
                TickPhase::FireFlood,
                TickPhase::SupplyNetwork,
                TickPhase::Reinforcement,
                TickPhase::Death,
                TickPhase::Convoys,
            ],
        })
    }
}
