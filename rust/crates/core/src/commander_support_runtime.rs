//! Recipient support planning on the human schedule, with explicit admission receipts.
use crate::{
    commander_ai::{Level, Observation},
    commander_support_policy::{AnnouncedStrike, SupportFacts},
    human_commander::{HumanView, OwnerHurtBatch, ScreenView, TickControl},
    human_commander_state::{Error, HumanAdvance, HumanState},
    match_commands::Command,
    match_human_observation::{HumanObservationMemory, human_screen_observation},
    match_runtime::Match,
    math, vision,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq)]
pub struct SupportPacket {
    observation: Observation,
    facts: SupportFacts,
}
impl SupportPacket {
    pub fn observation(&self) -> &Observation {
        &self.observation
    }
    pub fn facts(&self) -> &SupportFacts {
        &self.facts
    }
}
impl Match {
    /// All strike announcements are public; wallets and cooldowns belong to this seat only.
    pub fn human_support_observation(
        &mut self,
        memory: &mut HumanObservationMemory,
    ) -> Result<HumanView<SupportPacket>, String> {
        let owner = memory.owner();
        self.validate_opening_projection(owner)?;
        let mut strikes = Vec::new();
        for strike in &self.world.game.strikes {
            let timing = strike
                .runtime
                .as_ref()
                .ok_or("missing public strike timing")?;
            if strike.owner >= self.world.game.players.len()
                || ![strike.x, strike.z, timing.delay]
                    .iter()
                    .all(|v| v.is_finite())
            {
                return Err("invalid public strike announcement".into());
            }
            strikes.push(AnnouncedStrike {
                kind: timing.kind.name().into(),
                owner: strike.owner,
                at: crate::game::Point {
                    x: math::round(strike.x * 10.0) / 10.0,
                    z: math::round(strike.z * 10.0) / 10.0,
                },
                delay: math::max(0.0, math::round(timing.delay * 10.0) / 10.0),
            });
        }
        let cooldowns: BTreeMap<_, _> = self.start.players[owner]
            .support
            .iter()
            .map(|(kind, cd)| (kind.clone(), math::max(0.0, cd.ceil())))
            .collect();
        let munitions = self.start.economy.players[owner].mun.floor();
        if !munitions.is_finite() || cooldowns.values().any(|cd| !cd.is_finite()) {
            return Err("invalid own support wallet or cooldown".into());
        }
        let view = self.human_commander_observation(memory)?;
        let geometry = crate::commander_received_vision::received_game(self, &view.packet)?;
        let grid = crate::grid::SpatialGrid::new(&geometry.units, 16.0);
        let team = view.packet.input().players[owner].team;
        let visible_points = view.packet.input().points.iter().enumerate()
            .filter(|(_, p)| vision::team_sees(&geometry, &self.rules, &grid, team, p.at))
            .map(|(i, _)| i).collect();
        let facts = SupportFacts {
            mode: self.options.mode,
            munitions,
            cooldowns,
            watched: BTreeSet::new(),
            stationary_since: memory.stationary_times(),
            visible_points,
            ready_points: BTreeSet::new(),
            strikes,
            situation: None,
            horde_seat: false,
            unavailable: BTreeSet::new(),
        };
        Ok(HumanView {
            tick: view.tick,
            spawn: view.spawn,
            units: view.units,
            visible: view.visible,
            sightings: view.sightings,
            packet: SupportPacket {
                observation: view.packet,
                facts,
            },
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SupportPolicy {
    owner: u32,
    tick: Option<u64>,
    noticed: BTreeMap<u32, (u64, u64)>,
    points: BTreeMap<usize, u64>,
    last_called: f64,
    look: crate::commander_look::LookState,
}
impl SupportPolicy {
    pub fn new(owner: u32) -> Self {
        Self {
            owner,
            tick: None,
            noticed: BTreeMap::new(),
            points: BTreeMap::new(),
            last_called: -1e9,
            look: crate::commander_look::LookState::default(),
        }
    }
    /// Call only after canonical admission accepts the emitted support attempt.
    pub fn accepted_call(&mut self, tick: u64) -> Result<(), String> {
        let now = tick as f64 / 20.0;
        if tick > 9_007_199_254_740_991 || now < self.last_called {
            return Err("invalid accepted support clock".into());
        }
        self.last_called = now;
        Ok(())
    }
    fn plan(
        &mut self,
        screen: ScreenView<'_, SupportPacket>,
        level: Level,
    ) -> Result<Vec<Value>, Error> {
        if screen.packet.observation.input().slot != self.owner as usize
            || self.tick.is_some_and(|tick| screen.tick < tick)
        {
            return Err(Error::Planner(
                "support policy seat or clock mismatch".into(),
            ));
        }
        let observation = human_screen_observation(&ScreenView {
            tick: screen.tick,
            spawn: screen.spawn,
            units: screen.units,
            visible: screen.visible,
            sightings: screen.sightings,
            packet: &screen.packet.observation,
        })
        .map_err(Error::Planner)?;
        let input = observation.input();
        let team = input.players[input.slot].team;
        let notice = match level {
            Level::Easy => 50,
            Level::Normal => 25,
            Level::Hard => 10,
        };
        let mut facts = screen.packet.facts.clone();
        facts.watched.clear();
        facts.ready_points.clear();
        self.noticed
            .retain(|_, (_, last)| screen.tick.saturating_sub(*last) <= 1200);
        let enemies: BTreeSet<_> = input
            .units
            .iter()
            .filter(|u| input.players[u.owner].team != team)
            .map(|u| u.id)
            .collect();
        facts.stationary_since.retain(|id, _| enemies.contains(id));
        for id in enemies {
            let seen = self.noticed.entry(id).or_insert((screen.tick, screen.tick));
            seen.1 = screen.tick;
            if screen.tick - seen.0 >= notice {
                facts.watched.insert(id);
            }
        }
        for (i, p) in input.points.iter().enumerate() {
            if p.owner
                .is_some_and(|owner| input.players[owner].team != team)
            {
                let first = self.points.entry(i).or_insert(screen.tick);
                if screen.tick - *first >= notice {
                    facts.ready_points.insert(i);
                }
            }
        }
        self.tick = Some(screen.tick);
        let immediate =
            crate::commander_support_policy::plan(&observation, &facts, level, self.last_called)
                .map_err(Error::Planner)?
                .into_iter()
                .collect();
        Ok(self
            .look
            .finish(
                input.tick as f64 / 20.0,
                None,
                None,
                None,
                &[],
                crate::commander_look::LookOrders {
                    immediate,
                    ..Default::default()
                },
            )
            .map_err(Error::Planner)?
            .into_iter()
            .map(|command| match command {
                Command::Support {
                    kind,
                    at,
                    direction: _,
                } => json!({"t":"support","kind":kind,"x":at.x,"z":at.z}),
                _ => unreachable!("support planner emits only support"),
            })
            .collect())
    }
}
pub fn advance_support(
    state: &mut HumanState,
    policy: &mut SupportPolicy,
    control: TickControl<'_>,
    view: Option<&HumanView<SupportPacket>>,
    hurts: &OwnerHurtBatch,
) -> Result<HumanAdvance, Error> {
    let level = match control.level {
        "easy" => Level::Easy,
        "hard" => Level::Hard,
        _ => Level::Normal,
    };
    let mut next_state = state.clone();
    let mut next_policy = policy.clone();
    let result = next_state.advance(control, view, hurts, |screen| {
        next_policy.plan(screen, level)
    })?;
    *state = next_state;
    *policy = next_policy;
    Ok(result)
}

#[cfg(test)]
#[path = "commander_support_runtime_tests.rs"]
mod tests;
