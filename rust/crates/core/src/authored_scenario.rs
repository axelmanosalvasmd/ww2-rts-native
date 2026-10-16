//! Ordinary authored objective and announcement state. Reinforcements remain unadmitted.
use std::collections::BTreeMap;

use serde_json::{Value, json};

use crate::{economy::Mode, match_runtime::Match};

const MAX_INTEGER: u64 = 9_007_199_254_740_991;

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ScenarioSay {
    pub text: String,
    pub localized: Value,
    pub recipients: String,
    pub side: usize,
    pub execution: String,
    pub at_tick: u64,
}
impl ScenarioSay {
    pub fn shot_json(&self) -> Value {
        json!({"k":"say", "text":self.text, "localized":self.localized,
            "scenarioRecipients":self.recipients, "scenarioSide":self.side,
            "pub":self.recipients=="public", "execution":self.execution, "atTick":self.at_tick})
    }
    pub(crate) fn validate(&self, state: &Match) -> Result<(), String> {
        if state.authored_scenario.is_none()
            || self.side >= state.world.game.players.len()
            || !policy_valid(&self.recipients)
            || self.at_tick > state.world.game.tick as u64
            || self.at_tick > MAX_INTEGER
            || !execution_valid(&self.execution)
            || localized_text(&self.localized).as_deref() != Some(&self.text)
        {
            return Err("invalid authored scenario announcement".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
struct Objective {
    id: String,
    text: Value,
    state: String,
    recipients: String,
    side: usize,
    updated: f64,
    execution: String,
}
impl Objective {
    fn row(&self) -> Value {
        json!({"id":self.id,"scope":"match","text":self.text,"state":self.state,
            "recipients":self.recipients,"side":self.side,"updated":self.updated,"execution":self.execution})
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AuthoredScenario {
    data: Value,
    executions: BTreeMap<String, String>,
    // Preserve first activation order, as Object.values does in the prototype.
    objectives: Vec<Objective>,
    next_execution: u64,
}

fn policy_valid(policy: &str) -> bool {
    matches!(policy, "public" | "side" | "team")
}
fn execution_valid(execution: &str) -> bool {
    execution
        .strip_prefix("scenario:")
        .and_then(|s| s.parse::<u64>().ok())
        .is_some_and(|n| (1..=MAX_INTEGER).contains(&n) && execution == format!("scenario:{n}"))
}
fn supported_localization(text: &Value) -> bool {
    text.as_object()
        .is_none_or(|m| m.len() == 1 || m.contains_key("en"))
}
fn localized_text(text: &Value) -> Option<String> {
    text.as_str().map(str::to_owned).or_else(|| {
        text.get("en")
            .and_then(Value::as_str)
            .or_else(|| text.as_object()?.values().next()?.as_str())
            .or_else(|| text.as_array()?.first()?.as_str())
            .map(str::to_owned)
    })
}
fn list<'a>(data: &'a Value, key: &str) -> &'a [Value] {
    data[key].as_array().map_or(&[], Vec::as_slice)
}
fn policy<'a>(value: &'a Value, fallback: &'a str) -> &'a str {
    value["recipients"].as_str().unwrap_or(fallback)
}
fn side(value: &Value, fallback: usize) -> usize {
    value["side"].as_u64().map_or(fallback, |n| n as usize)
}

/// Match-only, once-only objective missions. Unsupported actions and modes refuse at startup.
pub(crate) fn validate_definition(data: &Value, count: usize, mode: Mode) -> Result<(), String> {
    let side_exists = |v: &Value| {
        v.get("side")
            .is_none_or(|s| s.as_u64().is_some_and(|n| n < count as u64))
    };
    if !data.is_object()
        || data["version"] != 1
        || !matches!(mode, Mode::Default | Mode::Conquest)
        || !list(data, "groups").is_empty()
        || list(data, "objectives").iter().any(|o| !side_exists(o))
        || list(data, "triggers").iter().any(|t| {
            t["scope"] != "match"
                || !side_exists(t)
                || t["condition"]["kind"] != "time"
                || !side_exists(&t["condition"])
                || t.get("repeat")
                    .is_some_and(|r| !r.is_null() && r["mode"] != "once")
                || list(t, "actions").iter().any(|a| {
                    !side_exists(a)
                        || (a["kind"] == "say" && !supported_localization(&a["text"]))
                        || !matches!(
                            a["kind"].as_str(),
                            Some("say" | "objectiveActivate" | "objectiveComplete")
                        )
                })
        })
    {
        return Err("authored scenarios currently require ordinary match-scoped once-only objective and announcement triggers".into());
    }
    Ok(())
}

impl AuthoredScenario {
    pub(crate) fn new(data: Value) -> Self {
        Self {
            data,
            executions: BTreeMap::new(),
            objectives: Vec::new(),
            next_execution: 1,
        }
    }
    pub(crate) fn recipient(&self, state: &Match, slot: usize) -> Value {
        json!({"version":1,"objectives":self.objectives.iter()
            .filter(|o| permitted(state,&o.recipients,o.side,slot)).map(Objective::row).collect::<Vec<_>>()})
    }
    fn step(&mut self, tick: u64) -> Vec<ScenarioSay> {
        let now = tick as f64 * 0.05;
        let mut budget = 4096;
        let mut shots = Vec::new();
        for trigger in list(&self.data.clone(), "triggers") {
            if budget == 0 {
                break;
            }
            budget -= 1;
            let yes = now
                >= trigger["condition"]["seconds"]
                    .as_f64()
                    .expect("validated time");
            let actions = list(trigger, "actions");
            if budget < actions.len() {
                break;
            }
            let id = trigger["id"].as_str().expect("validated trigger ID");
            if !yes || self.executions.contains_key(id) {
                continue;
            }
            budget -= actions.len();
            let execution = format!("scenario:{}", self.next_execution);
            self.next_execution += 1;
            self.executions.insert(id.into(), execution.clone());
            for action in actions {
                let recipients = policy(action, policy(trigger, "public"));
                let side = side(action, side(trigger, 0));
                if action["kind"] == "say" {
                    shots.push(ScenarioSay {
                        text: localized_text(&action["text"]).expect("validated text"),
                        localized: action["text"].clone(),
                        recipients: recipients.into(),
                        side,
                        execution: execution.clone(),
                        at_tick: tick,
                    });
                } else {
                    let id = action["objective"]
                        .as_str()
                        .expect("validated objective ID");
                    let existing = self.objectives.iter().position(|o| o.id == id);
                    if action["kind"] == "objectiveActivate"
                        && existing.is_some_and(|i| self.objectives[i].state == "complete")
                    {
                        continue;
                    }
                    let def = list(&self.data, "objectives")
                        .iter()
                        .find(|o| o["id"] == id)
                        .expect("validated objective");
                    let objective = Objective {
                        id: id.into(),
                        text: def["text"].clone(),
                        state: if action["kind"] == "objectiveComplete" {
                            "complete"
                        } else {
                            "active"
                        }
                        .into(),
                        recipients: policy(def, recipients).into(),
                        side: side_for_objective(def, side),
                        updated: now,
                        execution: execution.clone(),
                    };
                    if let Some(i) = existing {
                        self.objectives[i] = objective;
                    } else {
                        self.objectives.push(objective);
                    }
                }
            }
        }
        shots
    }
}
fn side_for_objective(value: &Value, fallback: usize) -> usize {
    side(value, fallback)
}

pub(crate) fn permitted(state: &Match, policy: &str, side: usize, slot: usize) -> bool {
    policy == "public"
        || if policy == "side" {
            side == slot
        } else {
            state
                .world
                .game
                .players
                .get(side)
                .zip(state.world.game.players.get(slot))
                .is_some_and(|(a, b)| a.team == b.team)
        }
}
impl Match {
    pub(crate) fn preflight_authored_scenario(&self) -> Result<(), String> {
        let Some(s) = &self.authored_scenario else {
            return Ok(());
        };
        if self.scenario.is_some()
            || self.world.geography().is_some()
            || self.world_runtime.is_some()
        {
            return Err("authored objective scenarios require an ordinary match".into());
        }
        let count = self.world.game.players.len();
        let map = json!({"w":self.world.game.w,"h":self.world.game.h,"spawns":vec![json!({});count],"scenario":s.data});
        crate::scenario_validation::validate_scenario(
            &map,
            crate::catalog::Catalog::compiled().map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        validate_definition(&s.data, count, self.options.mode)?;
        if s.next_execution != s.executions.len() as u64 + 1 || s.next_execution > MAX_INTEGER {
            return Err("invalid authored execution sequence".into());
        }
        let mut used = std::collections::BTreeSet::new();
        for (id, execution) in &s.executions {
            if !list(&s.data, "triggers").iter().any(|t| t["id"] == *id)
                || !execution_valid(execution)
                || !used.insert(execution)
                || execution[9..].parse::<u64>().unwrap() >= s.next_execution
            {
                return Err("invalid authored trigger state".into());
            }
        }
        let mut ids = std::collections::BTreeSet::new();
        for o in &s.objectives {
            if !ids.insert(&o.id)
                || o.side >= count
                || !policy_valid(&o.recipients)
                || !matches!(o.state.as_str(), "active" | "complete")
                || !o.updated.is_finite()
                || o.updated < 0.0
                || !used.contains(&o.execution)
                || !list(&s.data, "objectives")
                    .iter()
                    .any(|d| d["id"] == o.id && d["text"] == o.text)
            {
                return Err("invalid authored objective state".into());
            }
        }
        Ok(())
    }
    pub(crate) fn step_authored_scenario(&mut self, tick: u64) -> Result<Vec<ScenarioSay>, String> {
        self.preflight_authored_scenario()?;
        if tick >= MAX_INTEGER {
            return Err("authored scenario tick exceeds exact integer range".into());
        }
        Ok(self
            .authored_scenario
            .as_mut()
            .map_or_else(Vec::new, |s| s.step(tick)))
    }
}

#[cfg(test)]
#[path = "authored_scenario/tests.rs"]
mod tests;
