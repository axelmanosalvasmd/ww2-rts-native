//! Bounded quiet strategic policy on the ordinary human commander's actual attention schedule.
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
#[derive(Clone, Debug, PartialEq)]
pub struct HumanPolicy {
    owner: u32,
    source: SourceState,
    random: MatchRandom,
}

impl HumanPolicy {
    pub fn new(owner: u32, recorded_match_seed: u64) -> Self {
        Self {
            owner,
            source: SourceState::default(),
            random: MatchRandom::new(recorded_match_seed),
        }
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

fn raw_command(command: &Command) -> Result<Value, Error> {
    match command {
        Command::Move {
            orders,
            control_ids: None,
            attack_move: false,
            queue: false,
            together: false,
            face: None,
        } => Ok(
            json!({"t":"move","orders":orders.iter().map(|order|json!([order.unit,order.at.x,order.at.z])).collect::<Vec<_>>()}),
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
