//! Legacy timed map triggers, migrated into the scenario engine's once-only state.
//!
//! Inputs come from `AuthoritativeMap`, which already validates legacy trigger limits.
//! This module emits actions in source order. The caller applies them and stops stepping
//! after a winner, as the reference does. General authored scenarios are not supported.
use crate::map::AuthoritativeMap;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PointOwner<'a> {
    pub id: &'a str,
    pub owner: i32,
}
#[derive(Clone, Debug, PartialEq)]
pub enum ActionDefinition {
    Say { text: String },
    Destroy { bounds: [usize; 4] },
}
#[derive(Clone, Debug, PartialEq)]
pub struct LegacyTrigger {
    pub id: String,
    pub at: f64,
    pub actions: Vec<ActionDefinition>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct TriggerState {
    pub count: u32,
    pub next: f64,
    pub previous: bool,
    pub execution: Option<String>,
}
impl Default for TriggerState {
    fn default() -> Self {
        Self {
            count: 0,
            next: 0.0,
            previous: false,
            execution: None,
        }
    }
}
/// The mutable subset of the original scenario state. All other legacy-only collections
/// (objectives, groups, groupStarted, units and deferred) remain empty.
#[derive(Clone, Debug, PartialEq)]
pub struct LegacyState {
    pub triggers: BTreeMap<String, TriggerState>,
    pub owners: BTreeMap<String, Option<i32>>,
    pub captured: BTreeSet<String>,
    pub next_execution: u64,
}
#[derive(Clone, Debug, PartialEq)]
pub struct SayAction {
    pub text: String,
    pub localized: BTreeMap<String, String>,
    pub execution: String,
    pub at_tick: u64,
    pub side: usize,
}
impl SayAction {
    /// The exact public Source shot row for a migrated legacy Say action.
    pub fn shot_json(&self) -> serde_json::Value {
        use serde_json::json;
        json!({"k":"say","text":self.text,"localized":self.localized,"scenarioRecipients":"public","scenarioSide":self.side,"pub":true,"execution":self.execution,"atTick":self.at_tick})
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DestroyAction {
    pub bounds: [usize; 4],
    pub execution: String,
}
#[derive(Clone, Debug, PartialEq)]
pub enum LegacyAction {
    Say(SayAction),
    Destroy(DestroyAction),
}
#[derive(Debug, PartialEq, Eq)]
pub enum LegacyStepError<E> {
    Scenario(LegacyError),
    Emit(E),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LegacyError {
    TickOutOfRange,
    InvalidState,
}
impl std::fmt::Display for LegacyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::TickOutOfRange => "scenario tick exceeds the exact JavaScript integer range",
            Self::InvalidState => "invalid legacy scenario state",
        })
    }
}
impl std::error::Error for LegacyError {}

/// Validate the only public Say payload that migrated legacy triggers can emit.
pub fn validate_public_say(action: &SayAction) -> Result<(), LegacyError> {
    let mut keys = action.localized.keys();
    let Some(language) = keys.next() else {
        return Err(LegacyError::InvalidState);
    };
    if language != "en"
        || keys.next().is_some()
        || action.text.is_empty()
        || action.localized.get("en") != Some(&action.text)
        || action.side != 0
        || action.at_tick > 9_007_199_254_740_991
        || !action.execution.starts_with("scenario:")
        || match action.execution["scenario:".len()..].parse::<u64>() {
            Ok(execution) => execution == 0,
            Err(_) => true,
        }
    {
        return Err(LegacyError::InvalidState);
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq)]
pub struct LegacyScenario {
    triggers: Vec<LegacyTrigger>,
    state: LegacyState,
}
impl LegacyScenario {
    /// Set up migrated definitions and point owners. Initialization never fires a trigger.
    /// Supply the actual match's retained points after mode filtering, in their source order.
    pub fn from_map(map: &AuthoritativeMap, points: &[PointOwner<'_>]) -> Option<Self> {
        if map.triggers().is_empty() {
            return None;
        }
        let triggers = map
            .triggers()
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let mut actions = Vec::new();
                if let Some(text) = &t.say {
                    actions.push(ActionDefinition::Say { text: text.clone() });
                }
                if let Some(bounds) = t.blow {
                    actions.push(ActionDefinition::Destroy { bounds });
                }
                LegacyTrigger {
                    id: format!("legacy:{i}"),
                    at: t.at,
                    actions,
                }
            })
            .collect();
        Some(Self {
            triggers,
            state: LegacyState {
                triggers: BTreeMap::new(),
                owners: points
                    .iter()
                    .map(|p| (p.id.to_owned(), Some(p.owner)))
                    .collect(),
                captured: BTreeSet::new(),
                next_execution: 1,
            },
        })
    }
    pub fn definitions(&self) -> &[LegacyTrigger] {
        &self.triggers
    }
    pub fn state(&self) -> &LegacyState {
        &self.state
    }
    /// Check that persisted mutable state still belongs to this fixed legacy-trigger set.
    pub fn validate_legacy_subset(&self) -> Result<(), LegacyError> {
        if self.triggers.iter().enumerate().any(|(i, trigger)| {
            trigger.id != format!("legacy:{i}")
                || !trigger.at.is_finite()
                || trigger.at < 0.0
                || trigger.actions.is_empty()
                || trigger.actions.iter().any(|action| match action {
                    ActionDefinition::Say { text } => text.is_empty(),
                    ActionDefinition::Destroy { .. } => false,
                })
        }) {
            return Err(LegacyError::InvalidState);
        }
        let mut checked = self.clone();
        checked.restore_state(checked.state.clone())
    }
    /// Restore typed state without accepting arbitrary scenario definitions or action payloads.
    pub fn restore_state(&mut self, state: LegacyState) -> Result<(), LegacyError> {
        let mut executions = BTreeSet::new();
        let mut count = 0u64;
        for (key, value) in &state.triggers {
            let Some(t) = self
                .triggers
                .iter()
                .find(|t| format!("{}@match", t.id) == *key)
            else {
                return Err(LegacyError::InvalidState);
            };
            if value.count > 1 || !value.next.is_finite() || value.next < 0.0 {
                return Err(LegacyError::InvalidState);
            }
            match (value.count, value.execution.as_deref()) {
                (0, None) if value.next == 0.0 => {}
                (1, Some(execution)) if value.next >= t.at => {
                    let Some(number) = execution
                        .strip_prefix("scenario:")
                        .and_then(|s| s.parse::<u64>().ok())
                    else {
                        return Err(LegacyError::InvalidState);
                    };
                    if number == 0
                        || number >= state.next_execution
                        || execution != format!("scenario:{number}")
                        || !executions.insert(number)
                    {
                        return Err(LegacyError::InvalidState);
                    }
                    count += 1;
                }
                _ => return Err(LegacyError::InvalidState),
            }
        }
        if state.next_execution != count + 1 || state.owners.keys().ne(self.state.owners.keys()) {
            return Err(LegacyError::InvalidState);
        }
        for captured in &state.captured {
            let Some((id, owner)) = captured.rsplit_once(':') else {
                return Err(LegacyError::InvalidState);
            };
            if !state.owners.contains_key(id) || owner.parse::<i32>().ok().is_none_or(|o| o < 0) {
                return Err(LegacyError::InvalidState);
            }
        }
        self.state = state;
        Ok(())
    }
    /// Evaluate one undecided match tick. Effects are returned in trigger/action order.
    pub fn step(
        &mut self,
        tick: u64,
        points: &[PointOwner<'_>],
    ) -> Result<Vec<LegacyAction>, LegacyError> {
        let mut actions = Vec::new();
        self.step_emitting(tick, points, |action| {
            actions.push(action);
            Ok::<(), std::convert::Infallible>(())
        })
        .map_err(|error| match error {
            LegacyStepError::Scenario(error) => error,
            LegacyStepError::Emit(never) => match never {},
        })?;
        Ok(actions)
    }

    /// Evaluate triggers in source order and actuate each trigger before considering the next.
    /// If an actuator fails, the already committed state and output prefix remain intact.
    pub fn step_emitting<E>(
        &mut self,
        tick: u64,
        points: &[PointOwner<'_>],
        emit: impl FnMut(LegacyAction) -> Result<(), E>,
    ) -> Result<(), LegacyStepError<E>> {
        if tick > 9_007_199_254_740_991 {
            return Err(LegacyStepError::Scenario(LegacyError::TickOutOfRange));
        }
        let now = tick as f64 * 0.05;
        if !now.is_finite() {
            return Err(LegacyStepError::Scenario(LegacyError::TickOutOfRange));
        }
        for (id, old) in &mut self.state.owners {
            // The source reserves region-prefixed targets for World regions, even if a
            // legacy point happens to use that otherwise-valid ID. World is unsupported.
            let owner = (!id.starts_with("region:"))
                .then(|| points.iter().find(|p| p.id == id).map(|p| p.owner))
                .flatten();
            if let Some(owner) = owner
                && owner >= 0
                && Some(owner) != *old
            {
                self.state.captured.insert(format!("{id}:{owner}"));
            }
            *old = owner;
        }
        let mut emit = emit;
        let mut budget = 4096usize;
        for trigger in &self.triggers {
            let key = format!("{}@match", trigger.id);
            let state = self.state.triggers.entry(key).or_default();
            if budget == 0 {
                break;
            }
            budget -= 1;
            let yes = now >= trigger.at;
            let cost = trigger
                .actions
                .iter()
                .map(|a| match a {
                    ActionDefinition::Say { .. } => 1,
                    ActionDefinition::Destroy {
                        bounds: [x0, y0, x1, y1],
                    } => (x1 - x0 + 1) * (y1 - y0 + 1),
                })
                .sum::<usize>();
            if budget < cost {
                break;
            }
            state.previous = yes;
            if !yes || state.count >= 1 || now < state.next {
                continue;
            }
            let execution = format!("scenario:{}", self.state.next_execution);
            self.state.next_execution += 1;
            budget -= cost;
            state.count += 1;
            state.next = now;
            state.execution = Some(execution.clone());
            for (i, action) in trigger.actions.iter().enumerate() {
                let effect = match action {
                    ActionDefinition::Say { text } => LegacyAction::Say(SayAction {
                        text: text.clone(),
                        localized: BTreeMap::from([("en".into(), text.clone())]),
                        execution: execution.clone(),
                        at_tick: tick,
                        side: 0,
                    }),
                    ActionDefinition::Destroy { bounds } => LegacyAction::Destroy(DestroyAction {
                        bounds: *bounds,
                        execution: format!("{execution}:{i}"),
                    }),
                };
                emit(effect).map_err(LegacyStepError::Emit)?;
            }
        }
        Ok(())
    }
    /// Exact migrated schema for persistence/fixture adapters. This is output only.
    pub fn definition_json(&self) -> serde_json::Value {
        use serde_json::json;
        json!({"version":1,"areas":[],"groups":[],"objectives":[],"triggers":self.triggers.iter().map(|t|json!({
            "id":t.id,"scope":"match","recipients":"public","condition":{"kind":"time","seconds":t.at},"repeat":{"mode":"once"},
            "actions":t.actions.iter().map(|a|match a {ActionDefinition::Say{text}=>json!({"kind":"say","text":{"en":text},"recipients":"public"}),ActionDefinition::Destroy{bounds}=>json!({"kind":"destroy","box":bounds})}).collect::<Vec<_>>()
        })).collect::<Vec<_>>()})
    }
    /// Original scenario-state keys, including the collections unused by legacy triggers.
    pub fn state_json(&self) -> serde_json::Value {
        use serde_json::json;
        let triggers: serde_json::Map<_, _> = self
            .state
            .triggers
            .iter()
            .map(|(key, state)| {
                let mut value =
                    json!({"count":state.count,"next":state.next,"previous":state.previous});
                if let Some(execution) = &state.execution {
                    value["execution"] = json!(execution);
                }
                (key.clone(), value)
            })
            .collect();
        let owners: serde_json::Map<_, _> = self
            .state
            .owners
            .iter()
            .filter_map(|(key, value)| value.map(|v| (key.clone(), json!(v))))
            .collect();
        let captured: serde_json::Map<_, _> = self
            .state
            .captured
            .iter()
            .map(|key| (key.clone(), json!(true)))
            .collect();
        json!({"data":self.definition_json(),"triggers":triggers,"objectives":{},"groups":{},"groupStarted":{},"units":{},"deferred":[],"captured":captured,"owners":owners,"nextExecution":self.state.next_execution})
    }
}
impl LegacyAction {
    /// Source-equivalent emitted row or actuator call, without executing either one.
    pub fn to_json(&self) -> serde_json::Value {
        use serde_json::json;
        match self {
            Self::Say(s) => {
                json!({"kind":"say","row":s.shot_json()})
            }
            Self::Destroy(d) => {
                json!({"kind":"damage","action":{"kind":"destroy","box":d.bounds},"execution":d.execution})
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actuator_error_keeps_committed_trigger_prefix() {
        let mut scenario = LegacyScenario {
            triggers: vec![
                LegacyTrigger {
                    id: "first".into(),
                    at: 0.0,
                    actions: vec![ActionDefinition::Say {
                        text: "first".into(),
                    }],
                },
                LegacyTrigger {
                    id: "second".into(),
                    at: 0.0,
                    actions: vec![ActionDefinition::Say {
                        text: "second".into(),
                    }],
                },
            ],
            state: LegacyState {
                triggers: BTreeMap::new(),
                owners: BTreeMap::new(),
                captured: BTreeSet::new(),
                next_execution: 1,
            },
        };
        let mut emitted = Vec::new();
        let result = scenario.step_emitting(0, &[], |action| {
            emitted.push(action);
            Err::<(), _>("actuator failed")
        });

        assert_eq!(result, Err(LegacyStepError::Emit("actuator failed")));
        assert_eq!(emitted.len(), 1);
        assert_eq!(scenario.state.triggers["first@match"].count, 1);
        assert_eq!(scenario.state.triggers["second@match"].count, 0);
        assert_eq!(scenario.state.next_execution, 2);
    }
}
