//! Seat tactics and the original quiet component on the human commander's attention schedule.
//! This publishes private state and delayed attempts; the host owns queue admission and receipts.

use crate::{
    commander_ai::{self, Level, Observation, SourceState},
    economy::Technology,
    human_commander::{HumanView, OwnerHurtBatch, ScreenView, TickControl},
    human_commander_state::{Error, HumanAdvance, HumanState},
    match_commands::Command,
    match_human_observation::human_screen_observation,
    random::MatchRandom,
};
use serde_json::{Value, json};

/// One seat's private strategic state and existing Ai-purpose random stream.
/// Human motor timing is a separate stream owned by HumanState.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct HumanPolicy {
    owner: u32,
    source: SourceState,
    random: MatchRandom,
    tactics: crate::commander_tactics::State,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct PlannerMemory {
    version: u32,
    owner: u32,
    source: SourceState,
    tactics: crate::commander_tactics::State,
}

impl HumanPolicy {
    pub(crate) fn save_owner(&self) -> u32 {
        self.owner
    }

    pub fn new(owner: u32, recorded_match_seed: u64) -> Self {
        Self {
            owner,
            source: SourceState::default(),
            random: MatchRandom::new(recorded_match_seed),
            tactics: crate::commander_tactics::State::default(),
        }
    }

    /// Persist strategic memory separately from motor timing and random streams.
    pub(crate) fn planner_memory_json(&self) -> Result<Value, String> {
        crate::save_codec::to_value(&PlannerMemory {
            version: 1,
            owner: self.owner,
            source: self.source.clone(),
            tactics: self.tactics.clone(),
        })
    }

    pub(crate) fn restore_planner_memory(&mut self, value: &Value) -> Result<(), String> {
        let memory: PlannerMemory = crate::save_codec::from_value(value.clone())?;
        if memory.version != 1 || memory.owner != self.owner {
            return Err("planner memory belongs to another seat or version".into());
        }
        self.source = memory.source;
        self.tactics = memory.tactics;
        Ok(())
    }

    fn plan(
        &mut self,
        screen: ScreenView<'_, Observation>,
        level: Level,
    ) -> Result<Vec<Value>, Error> {
        let observation = human_screen_observation(&screen).map_err(Error::Planner)?;
        if observation.input().slot != self.owner as usize {
            return Err(Error::Planner(
                "human policy seat does not match its delivered packet".into(),
            ));
        }
        if observation.input().runtime.is_some() {
            let sightings = screen
                .sightings
                .iter()
                .map(|s| {
                    let v = &s.payload;
                    Ok(crate::commander_mind::Sighting {
                        id: s.id,
                        kind: v["type"]
                            .as_str()
                            .ok_or_else(|| Error::Planner("invalid sighting kind".into()))?
                            .into(),
                        owner: v["owner"]
                            .as_u64()
                            .ok_or_else(|| Error::Planner("invalid sighting owner".into()))?
                            as usize,
                        at: crate::game::Point {
                            x: v["x"]
                                .as_f64()
                                .ok_or_else(|| Error::Planner("invalid sighting x".into()))?,
                            z: v["z"]
                                .as_f64()
                                .ok_or_else(|| Error::Planner("invalid sighting z".into()))?,
                        },
                        time: v["t"]
                            .as_f64()
                            .ok_or_else(|| Error::Planner("invalid sighting time".into()))?,
                        worth: v["val"]
                            .as_f64()
                            .ok_or_else(|| Error::Planner("invalid sighting worth".into()))?,
                    })
                })
                .collect::<Result<Vec<_>, Error>>()?;
            return crate::commander_tactics::plan(
                &observation,
                &sightings,
                &mut self.tactics,
                level,
                &mut self.random,
            )
            .map_err(Error::Planner);
        }
        let planned =
            commander_ai::plan(&observation, &mut self.source, level, &mut self.random)
                .map_err(|error| Error::Planner(format!("bounded human policy: {error:?}")))?;
        planned.commands.iter().map(raw_command).collect()
    }
}

/// Publish both private providers together only after planning and input processing succeed.
/// This does not reserve issuer keys or admit an emitted attempt to the canonical queue.
/// The host binds both providers to the same seat and supplies staged values for a whole-pass commit.
pub fn advance(
    state: &mut HumanState,
    policy: &mut HumanPolicy,
    control: TickControl<'_>,
    view: Option<&HumanView<Observation>>,
    hurts: &OwnerHurtBatch,
) -> Result<HumanAdvance, Error> {
    let level = Level::from_key(control.level).unwrap_or_default();
    let mut next_state = state.clone();
    let mut next_policy = policy.clone();
    let result = next_state.advance(control, view, hurts, |screen| {
        next_policy.plan(screen, level)
    })?;
    *state = next_state;
    *policy = next_policy;
    Ok(result)
}

fn raw_command(command: &Command) -> Result<Value, Error> {
    match command {
        Command::Move {
            orders,
            control_ids: None,
            attack_move,
            queue: false,
            together: false,
            face: None,
        } => Ok(
            json!({"t":if *attack_move { "amove" } else { "move" },"orders":orders.iter().map(|order|json!([order.unit,order.at.x,order.at.z])).collect::<Vec<_>>()}),
        ),
        Command::Cover {
            ids,
            queue: false,
            at: None,
        } => Ok(json!({"t":"cover","ids":ids})),
        Command::Buy { kind, from: None } => Ok(json!({"t":"buy","unit":kind})),
        Command::Technology { kind } => Ok(json!({"t":"tech","kind":match kind {
            Technology::Tier=>"tier", Technology::Infantry=>"inf", Technology::Guns=>"guns", Technology::Armor=>"armor",
        }})),
        _ => Err(Error::Planner(
            "command outside the bounded human policy output envelope".into(),
        )),
    }
}

#[cfg(test)]
#[path = "human_commander_policy_tests.rs"]
mod tests;
