//! Ordinary authored objective and announcement state. Reinforcements remain unadmitted.
use crate::scenario_conditions::{ConditionState, Scope};
use std::collections::{BTreeMap, BTreeSet};

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
    scope: String,
    text: Value,
    state: String,
    recipients: String,
    side: usize,
    updated: f64,
    execution: String,
}
impl Objective {
    fn row(&self) -> Value {
        json!({"id":self.id,"scope":self.scope,"text":self.text,"state":self.state,
            "recipients":self.recipients,"side":self.side,"updated":self.updated,"execution":self.execution})
    }
}

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
struct TriggerState {
    count: u32,
    next: f64,
    previous: bool,
    execution: Option<String>,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
struct ArrivalUnit {
    kind: String,
    authored: Option<String>,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
struct Arrival {
    group: String,
    side: usize,
    units: Vec<ArrivalUnit>,
    at: [usize; 2],
    order: Value,
    expires: f64,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AuthoredScenario {
    data: Value,
    executions: BTreeMap<String, TriggerState>,
    // Preserve first activation order, as Object.values does in the prototype.
    objectives: Vec<Objective>,
    facts: ConditionState,
    owners: BTreeMap<String, i32>,
    deferred: Vec<Arrival>,
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
        let mut facts = ConditionState::default();
        for area in list(&data, "areas") {
            let bounds = area["box"].as_array().unwrap();
            facts.areas.insert(
                area["id"].as_str().unwrap().into(),
                std::array::from_fn(|i| bounds[i].as_f64().unwrap() as usize),
            );
        }
        Self {
            data,
            executions: BTreeMap::new(),
            objectives: Vec::new(),
            facts,
            owners: BTreeMap::new(),
            deferred: Vec::new(),
            next_execution: 1,
        }
    }
    pub(crate) fn recipient(&self, state: &Match, slot: usize) -> Value {
        json!({"version":1,"objectives":self.objectives.iter()
            .filter(|o| permitted(state,&o.recipients,o.side,slot)).map(Objective::row).collect::<Vec<_>>()})
    }
    fn step(&mut self, tick: u64, owners: &[(String, i32)]) -> Vec<ScenarioSay> {
        for (id, owner) in owners {
            if *owner >= 0 && self.owners.get(id) != Some(owner) {
                self.facts.captured.insert((id.clone(), *owner));
            }
            self.owners.insert(id.clone(), *owner);
        }
        let now = tick as f64 * 0.05;
        let mut budget = 4096;
        let mut shots = Vec::new();
        for trigger in list(&self.data.clone(), "triggers") {
            let id = trigger["id"].as_str().expect("validated trigger ID");
            let key = format!("{id}@match");
            self.facts
                .trigger_counts
                .entry((id.into(), Scope::Match))
                .or_insert(0);
            let record = self.executions.entry(key).or_default();
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
            record.previous = yes;
            if !yes || record.count >= 1 || now < record.next {
                continue;
            }
            budget -= actions.len();
            let execution = format!("scenario:{}", self.next_execution);
            self.next_execution += 1;
            record.count += 1;
            record.next = now + trigger["repeat"]["cooldown"].as_f64().unwrap_or(0.0);
            record.execution = Some(execution.clone());
            self.facts
                .trigger_counts
                .insert((id.into(), Scope::Match), record.count);
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
                        scope: "match".into(),
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
                    if objective.state == "complete" {
                        self.facts
                            .completed_objectives
                            .insert((id.into(), Scope::Match));
                    }
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
    pub(crate) fn setup_authored_scenario(&mut self) -> Result<(), String> {
        if let Some(scenario) = self.authored_scenario.as_mut() {
            for point in &self.start.points {
                scenario.owners.insert(
                    point
                        .id
                        .as_str()
                        .ok_or("scenario point identity is not a string")?
                        .into(),
                    point.owner.map_or(-1, |s| s as i32),
                );
            }
        }
        self.preflight_authored_scenario()
    }
    pub(crate) fn preflight_authored_scenario(&self) -> Result<(), String> {
        let Some(s) = &self.authored_scenario else {
            return Ok(());
        };
        if self.scenario.is_some()
            || self.world.geography().is_some()
            || self.world_runtime.is_some()
        {
            return Err("authored scenarios require an ordinary match".into());
        }
        let count = self.world.game.players.len();
        let points: Vec<_> = self
            .start
            .points
            .iter()
            .map(|p| json!({"id":p.id}))
            .collect();
        let map = json!({"w":self.world.game.w,"h":self.world.game.h,"points":points,"spawns":vec![json!({});count],"scenario":s.data});
        crate::scenario_validation::validate_scenario(
            &map,
            crate::catalog::Catalog::compiled().map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        validate_definition(&s.data, count, self.options.mode)?;
        if s.next_execution == 0 || s.next_execution > MAX_INTEGER || s.deferred.len() > 256 {
            return Err("invalid authored execution sequence or arrival queue".into());
        }
        let valid_scope = |scope: &str| -> Option<Scope> {
            if scope == "match" {
                return Some(Scope::Match);
            }
            if let Some(slot) = scope
                .strip_prefix("player:")
                .and_then(|n| n.parse::<usize>().ok())
            {
                return (slot < count && scope == format!("player:{slot}"))
                    .then_some(Scope::Player(slot));
            }
            if let Some(team) = scope
                .strip_prefix("team:")
                .and_then(|n| n.parse::<i32>().ok())
            {
                return (scope == format!("team:{team}")
                    && self.world.game.players.iter().any(|p| p.team == team))
                .then_some(Scope::Team(team));
            }
            None
        };
        let mut used = BTreeSet::new();
        let mut counts = BTreeMap::new();
        for (key, record) in &s.executions {
            let Some((id, scope)) = key.split_once('@') else {
                return Err("invalid authored trigger key".into());
            };
            let Some(def) = list(&s.data, "triggers").iter().find(|t| t["id"] == id) else {
                return Err("unknown authored trigger".into());
            };
            let scope = valid_scope(scope).ok_or("invalid authored trigger scope")?;
            let matches_scope = matches!(
                (def["scope"].as_str(), scope),
                (Some("match"), Scope::Match)
                    | (Some("player"), Scope::Player(_))
                    | (Some("team"), Scope::Team(_))
            );
            let max = if def["repeat"]["mode"].as_str().unwrap_or("once") == "once" {
                1
            } else {
                def["repeat"]["max"].as_f64().unwrap() as u32
            };
            if !matches_scope
                || !record.next.is_finite()
                || record.next < 0.0
                || record.count > max
                || record.execution.as_ref().is_some_and(|e| {
                    !execution_valid(e)
                        || !used.insert(e.clone())
                        || e[9..].parse::<u64>().unwrap() >= s.next_execution
                })
                || (record.count == 0) != record.execution.is_none()
            {
                return Err("invalid authored trigger state".into());
            }
            counts.insert((id.to_owned(), scope), record.count);
        }
        if counts != s.facts.trigger_counts {
            return Err("invalid authored trigger facts".into());
        }
        let groups: BTreeSet<_> = list(&s.data, "groups")
            .iter()
            .map(|g| g["id"].as_str().unwrap().to_owned())
            .collect();
        let authored: BTreeSet<_> = list(&s.data, "groups")
            .iter()
            .flat_map(|g| list(g, "units"))
            .map(|u| u["id"].as_str().unwrap().to_owned())
            .collect();
        let expected_areas: BTreeMap<_, _> = list(&s.data, "areas")
            .iter()
            .map(|a| {
                (
                    a["id"].as_str().unwrap().to_owned(),
                    std::array::from_fn(|i| a["box"][i].as_f64().unwrap() as usize),
                )
            })
            .collect();
        let point_ids: BTreeSet<_> = self
            .start
            .points
            .iter()
            .map(|p| p.id.as_str().unwrap().to_owned())
            .collect();
        let deferred: BTreeSet<_> = s.deferred.iter().map(|d| d.group.clone()).collect();
        let mut group_units = BTreeSet::new();
        if s.facts.groups.keys().cloned().collect::<BTreeSet<_>>() != groups
            || s.facts.areas != expected_areas
            || s.facts.deferred_groups != deferred
            || !s.facts.started_groups.is_subset(&groups)
            || s.facts
                .groups
                .values()
                .flatten()
                .any(|id| *id == 0 || *id >= self.next_unit_id || !group_units.insert(*id))
            || s.facts
                .authored_units
                .iter()
                .any(|(name, id)| !authored.contains(name) || *id == 0 || *id >= self.next_unit_id)
            || s.owners.keys().cloned().collect::<BTreeSet<_>>() != point_ids
            || s.owners
                .values()
                .any(|slot| *slot < -1 || *slot >= count as i32)
            || s.facts.captured.iter().any(|(target, slot)| {
                !point_ids.contains(target) || *slot < 0 || *slot >= count as i32
            })
        {
            return Err("invalid authored group, area or capture facts".into());
        }
        let mut ids = BTreeSet::new();
        for o in &s.objectives {
            if !ids.insert((&o.id, &o.scope))
                || o.scope != "match"
                || o.side >= count
                || !policy_valid(&o.recipients)
                || !matches!(o.state.as_str(), "active" | "complete")
                || !o.updated.is_finite()
                || o.updated < 0.0
                || !execution_valid(&o.execution)
                || o.execution[9..].parse::<u64>().unwrap() >= s.next_execution
                || !list(&s.data, "objectives")
                    .iter()
                    .any(|d| d["id"] == o.id && d["text"] == o.text)
            {
                return Err("invalid authored objective state".into());
            }
        }
        let completed: BTreeSet<_> = s
            .objectives
            .iter()
            .filter(|o| o.state == "complete")
            .map(|o| (o.id.clone(), valid_scope(&o.scope).unwrap()))
            .collect();
        if completed != s.facts.completed_objectives {
            return Err("invalid authored objective facts".into());
        }
        if !s.deferred.is_empty() {
            return Err("authored objective scenarios do not support deferred arrivals".into());
        }
        Ok(())
    }
    pub(crate) fn step_authored_scenario(&mut self, tick: u64) -> Result<Vec<ScenarioSay>, String> {
        self.preflight_authored_scenario()?;
        if tick >= MAX_INTEGER {
            return Err("authored scenario tick exceeds exact integer range".into());
        }
        if self.authored_scenario.is_none() {
            return Ok(Vec::new());
        }
        let owners = self
            .start
            .points
            .iter()
            .map(|point| {
                Ok((
                    point
                        .id
                        .as_str()
                        .ok_or("scenario point identity is not a string")?
                        .into(),
                    point.owner.map_or(-1, |slot| slot as i32),
                ))
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(self
            .authored_scenario
            .as_mut()
            .map_or_else(Vec::new, |s| s.step(tick, &owners)))
    }
}

#[cfg(test)]
#[path = "authored_scenario/tests.rs"]
mod tests;
