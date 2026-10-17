//! Ability decisions on detached recipient facts and the existing human input schedule.

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    catalog::{Catalog, Value as RuleValue},
    commander_abilities::AbilityFacts,
    commander_ai::{Level, Observation},
    human_commander::{HumanView, OwnerHurtBatch, ScreenView, TickControl},
    human_commander_state::{Error, HumanAdvance, HumanState},
    match_commands::Command,
    match_human_observation::{HumanObservationMemory, human_screen_observation},
    match_runtime::Match,
    math,
};
use serde_json::{Value, json};

/// Own ability state supplements the existing recipient packet. Enemy cooldowns are absent.
#[derive(Clone, Debug, PartialEq)]
pub struct AbilityPacket {
    observation: Observation,
    cooldowns: BTreeMap<u32, f64>,
    attack_moving: BTreeSet<u32>,
    at_base: BTreeSet<u32>,
}

impl AbilityPacket {
    pub fn observation(&self) -> &Observation {
        &self.observation
    }
    pub fn cooldowns(&self) -> &BTreeMap<u32, f64> {
        &self.cooldowns
    }
}

impl Match {
    /// Uses the ordinary recipient producer without widening its supported modes or privacy.
    pub fn human_ability_observation(
        &mut self,
        memory: &mut HumanObservationMemory,
    ) -> Result<HumanView<AbilityPacket>, String> {
        let owner = memory.owner();
        let mut cooldowns = BTreeMap::new();
        let radius = Catalog::compiled()
            .map_err(|e| e.to_string())?
            .table("CFG")
            .map_err(|e| e.to_string())?
            .get("reinforceRadius")
            .and_then(RuleValue::number)
            .filter(|n| n.is_finite())
            .ok_or("invalid reinforcement radius")?;
        let completed_bases: BTreeSet<_> = self
            .world
            .game
            .units
            .values()
            .filter(|u| {
                u.owner == owner
                    && u.hp > 0.0
                    && u.built.is_some_and(|built| built >= 1.0)
                    && self
                        .rules
                        .kinds
                        .get(u.kind)
                        .is_some_and(|kind| kind.name == "hq")
            })
            .map(|u| u.id)
            .collect();
        for u in self.world.game.units.values().filter(|u| u.owner == owner) {
            let kind = self
                .rules
                .kinds
                .get(u.kind)
                .ok_or("invalid ability unit kind")?;
            if kind.structure {
                continue;
            }
            let cooldown = self
                .abilities
                .get(u.id)
                .ok_or("missing own ability state")?
                .cooldown;
            if !cooldown.is_finite() {
                return Err("invalid own ability cooldown".into());
            }
            cooldowns.insert(u.id, math::max(0.0, cooldown.ceil()));
        }
        let view = self.human_commander_observation(memory)?;
        let bases: Vec<_> = view
            .packet
            .input()
            .units
            .iter()
            .filter(|u| completed_bases.contains(&u.id))
            .map(|u| u.at)
            .collect();
        let at_base = view
            .packet
            .input()
            .units
            .iter()
            .filter(|u| {
                cooldowns.contains_key(&u.id)
                    && bases
                        .iter()
                        .any(|at| math::hypot(u.at.x - at.x, u.at.z - at.z) <= radius)
            })
            .map(|u| u.id)
            .collect();
        let attack_moving = view
            .units
            .iter()
            .filter(|u| u.owner as usize == owner && u.motion.amove.is_some())
            .map(|u| u.motion.id)
            .collect();
        Ok(HumanView {
            tick: view.tick,
            spawn: view.spawn,
            units: view.units,
            visible: view.visible,
            sightings: view.sightings,
            packet: AbilityPacket {
                observation: view.packet,
                cooldowns,
                attack_moving,
                at_base,
            },
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
struct Watched {
    first: u64,
    last: u64,
}

/// Per-seat notice history, derived only from the camera-filtered planning callback.
#[derive(Clone, Debug, PartialEq)]
pub struct AbilityPolicy {
    owner: u32,
    tick: Option<u64>,
    noticed: BTreeMap<u32, Watched>,
    look: crate::commander_look::LookState,
}

impl AbilityPolicy {
    pub fn new(owner: u32) -> Self {
        Self {
            owner,
            tick: None,
            noticed: BTreeMap::new(),
            look: crate::commander_look::LookState::default(),
        }
    }

    fn plan(
        &mut self,
        screen: ScreenView<'_, AbilityPacket>,
        level: Level,
    ) -> Result<Vec<Value>, Error> {
        if screen.packet.observation.input().slot != self.owner as usize
            || self.tick.is_some_and(|tick| screen.tick < tick)
        {
            return Err(Error::Planner(
                "ability policy seat or clock mismatch".into(),
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
        self.tick = Some(screen.tick);
        self.noticed
            .retain(|_, seen| screen.tick.saturating_sub(seen.last) <= 1200);
        let notice = match level {
            Level::Easy => 50,
            Level::Normal => 25,
            Level::Hard => 10,
        };
        let team = input.players[input.slot].team;
        let mut watched = BTreeSet::new();
        for u in input
            .units
            .iter()
            .filter(|u| input.players[u.owner].team != team)
        {
            let seen = self.noticed.entry(u.id).or_insert(Watched {
                first: screen.tick,
                last: screen.tick,
            });
            seen.last = screen.tick;
            if screen.tick - seen.first >= notice {
                watched.insert(u.id);
            }
        }
        let commands = crate::commander_abilities::plan(
            &observation,
            &AbilityFacts {
                cooldowns: screen.packet.cooldowns.clone(),
                watched,
                attack_moving: screen.packet.attack_moving.clone(),
                at_base: screen.packet.at_base.clone(),
                unavailable: BTreeSet::new(),
            },
            level,
        )
        .map_err(Error::Planner)?;
        let commands = self
            .look
            .finish(
                input.tick as f64 / 20.0,
                None,
                None,
                None,
                &[],
                crate::commander_look::LookOrders {
                    immediate: commands,
                    ..Default::default()
                },
            )
            .map_err(Error::Planner)?;
        commands.iter().map(raw_ability).collect()
    }
}

fn raw_ability(command: &Command) -> Result<Value, Error> {
    match command {
        Command::Ability {
            ids,
            at,
            queue: false,
            automatic: false,
        } => {
            let mut value = json!({"t":"ability", "ids":ids});
            if let Some(at) = at {
                value["x"] = json!(at.x);
                value["z"] = json!(at.z);
            }
            Ok(value)
        }
        _ => Err(Error::Planner("non-ability in ability policy".into())),
    }
}

/// Publish notice history and motor state together. An input remains an admission attempt.
pub fn advance_abilities(
    state: &mut HumanState,
    policy: &mut AbilityPolicy,
    control: TickControl<'_>,
    view: Option<&HumanView<AbilityPacket>>,
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
#[path = "commander_ability_policy_tests.rs"]
mod tests;
