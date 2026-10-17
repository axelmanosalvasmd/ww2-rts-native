//! Ordinary authored scenario scheduling over canonical match facts and effect owners.
use crate::match_tick::TickEvent;
use crate::scenario_conditions::{self, ConditionState, Context, Scope, WorkBudget};
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
fn localized_text(text: &Value) -> Option<String> {
    text.as_str().map(str::to_owned).or_else(|| {
        text.get("en")
            .and_then(Value::as_str)
            .or_else(|| {
                let object = text.as_object()?;
                // JavaScript enumerates canonical array-index keys before other keys.
                object
                    .iter()
                    .filter_map(|(key, value)| {
                        let index = key.parse::<u32>().ok()?;
                        (index < u32::MAX && key == &index.to_string()).then_some((index, value))
                    })
                    .min_by_key(|(index, _)| *index)
                    .map(|(_, value)| value)
                    .or_else(|| object.values().next())?
                    .as_str()
            })
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
    value["side"].as_f64().map_or(fallback, |n| n as usize)
}

/// Ordinary rooms carry the complete authored engine. Other mode admission stays closed.
pub(crate) fn validate_definition(data: &Value, count: usize, mode: Mode) -> Result<(), String> {
    fn side_exists(value: &Value, count: usize) -> bool {
        value.get("side").is_none_or(|v| {
            v.as_f64()
                .is_some_and(|n| n.is_finite() && n >= 0.0 && n.fract() == 0.0 && n < count as f64)
        })
    }
    fn condition_sides_exist(value: &Value, count: usize) -> bool {
        if matches!(value["kind"].as_str(), Some("all" | "any")) {
            list(value, "conditions")
                .iter()
                .all(|q| condition_sides_exist(q, count))
        } else {
            side_exists(value, count)
                && !(matches!(value["kind"].as_str(), Some("ownership" | "capture"))
                    && value["target"]
                        .as_str()
                        .is_some_and(|target| target.starts_with("region:")))
        }
    }
    if !data.is_object()
        || data["version"].as_f64() != Some(1.0)
        || !matches!(
            mode,
            Mode::Default
                | Mode::Conquest
                | Mode::Assault
                | Mode::Annihilation
                | Mode::Classic
                | Mode::Horde
        )
        || list(data, "objectives")
            .iter()
            .any(|o| !side_exists(o, count))
        || list(data, "groups")
            .iter()
            .flat_map(|g| list(g, "units"))
            .any(|u| !side_exists(u, count))
        || list(data, "triggers").iter().any(|t| {
            !side_exists(t, count)
                || !condition_sides_exist(&t["condition"], count)
                || list(t, "actions").iter().any(|a| !side_exists(a, count))
        })
    {
        return Err(
            "authored scenarios require non-World rooms, present sides and ordinary targets".into(),
        );
    }
    Ok(())
}
fn context_key(scope: Scope) -> String {
    match scope {
        Scope::Match => "match".into(),
        Scope::Player(slot) => format!("player:{slot}"),
        Scope::Team(team) => format!("team:{team}"),
    }
}
fn contexts(state: &Match, trigger: &Value) -> Vec<Context> {
    let trigger_side = trigger["side"].as_f64().map(|n| n as usize);
    let context = |scope, slot| Context {
        scope,
        slot,
        trigger_side,
    };
    match trigger["scope"].as_str().unwrap() {
        "match" => vec![context(Scope::Match, trigger_side.unwrap_or(0))],
        "player" => state
            .start
            .economy
            .players
            .iter()
            .enumerate()
            .filter(|(slot, p)| !p.out && trigger_side.is_none_or(|s| s == *slot))
            .map(|(slot, _)| context(Scope::Player(slot), slot))
            .collect(),
        "team" => {
            let mut seen = BTreeSet::new();
            state
                .start
                .economy
                .players
                .iter()
                .enumerate()
                .filter(|(_, p)| {
                    !p.out
                        && trigger_side.is_none_or(|s| state.world.game.players[s].team == p.team)
                })
                .filter(|(_, p)| seen.insert(p.team))
                .map(|(slot, p)| context(Scope::Team(p.team), slot))
                .collect()
        }
        _ => unreachable!("validated scope"),
    }
}
fn coords(value: &Value) -> [usize; 2] {
    [
        value[0].as_f64().unwrap() as usize,
        value[1].as_f64().unwrap() as usize,
    ]
}
impl AuthoredScenario {
    pub(crate) fn new(data: Value) -> Self {
        let mut facts = ConditionState::default();
        let mut deferred = Vec::new();
        for area in list(&data, "areas") {
            let bounds = area["box"].as_array().unwrap();
            facts.areas.insert(
                area["id"].as_str().unwrap().into(),
                std::array::from_fn(|i| bounds[i].as_f64().unwrap() as usize),
            );
        }
        for group in list(&data, "groups") {
            let id = group["id"].as_str().unwrap().to_owned();
            facts.groups.insert(id.clone(), Vec::new());
            for unit in list(group, "units") {
                deferred.push(Arrival {
                    group: id.clone(),
                    side: unit["side"].as_f64().unwrap() as usize,
                    units: vec![ArrivalUnit {
                        kind: unit["type"].as_str().unwrap().into(),
                        authored: Some(unit["id"].as_str().unwrap().into()),
                    }],
                    at: [
                        unit["x"].as_f64().unwrap() as usize,
                        unit["y"].as_f64().unwrap() as usize,
                    ],
                    order: json!({"kind":"hold"}),
                    expires: 7200.0,
                });
            }
        }
        facts.deferred_groups = deferred.iter().map(|d| d.group.clone()).collect();
        Self {
            data,
            executions: BTreeMap::new(),
            objectives: Vec::new(),
            facts,
            owners: BTreeMap::new(),
            deferred,
            next_execution: 1,
        }
    }
    pub(crate) fn matches_map(&self, map: &crate::map::AuthoritativeMap) -> bool {
        crate::scenario_validation::explicit_scenario(map.source()) == Some(&self.data)
    }

    pub(crate) fn recipient(&self, state: &Match, slot: usize) -> Value {
        json!({"version":1,"objectives":self.objectives.iter()
            .filter(|o| permitted(state,&o.recipients,o.side,slot)).map(Objective::row).collect::<Vec<_>>()})
    }
    fn refresh_deferred_facts(&mut self) {
        self.facts.deferred_groups = self.deferred.iter().map(|d| d.group.clone()).collect();
    }
    fn run_deferred(&mut self, state: &mut Match, now: f64) -> Result<(), String> {
        let mut work = 0;
        let mut retained = Vec::new();
        for mut arrival in std::mem::take(&mut self.deferred) {
            if now >= arrival.expires || state.start.economy.players[arrival.side].out {
                continue;
            }
            while !arrival.units.is_empty() && work < 256 {
                work += 1;
                let spec = &arrival.units[0];
                let Some(id) =
                    state.scenario_arrive(arrival.side, &spec.kind, arrival.at, &arrival.order)?
                else {
                    break;
                };
                self.facts.started_groups.insert(arrival.group.clone());
                self.facts.groups.get_mut(&arrival.group).unwrap().push(id);
                if let Some(authored) = &spec.authored {
                    self.facts.authored_units.insert(authored.clone(), id);
                }
                arrival.units.remove(0);
            }
            if !arrival.units.is_empty() {
                retained.push(arrival);
            }
        }
        self.deferred = retained;
        self.refresh_deferred_facts();
        Ok(())
    }
    fn step(
        &mut self,
        state: &mut Match,
        tick: u64,
        events: &mut Vec<TickEvent>,
    ) -> Result<(), String> {
        let now = tick as f64 * 0.05;
        for point in &state.start.points {
            let id = point
                .id
                .as_str()
                .ok_or("scenario point identity is not a string")?;
            let owner = point.owner.map_or(-1, |s| s as i32);
            if owner >= 0 && self.owners.get(id) != Some(&owner) {
                self.facts.captured.insert((id.into(), owner));
            }
            self.owners.insert(id.into(), owner);
        }
        for ids in self.facts.groups.values_mut() {
            ids.retain(|id| state.world.game.units.get(id).is_some_and(|u| u.hp > 0.0));
        }
        self.run_deferred(state, now)?;
        let mut budget = WorkBudget { left: 4096 };
        let definitions = self.data.clone();
        for trigger in list(&definitions, "triggers") {
            let condition = scenario_conditions::from_definition(&trigger["condition"]);
            for context in contexts(state, trigger) {
                let id = trigger["id"].as_str().unwrap();
                let key = format!("{id}@{}", context_key(context.scope));
                self.facts
                    .trigger_counts
                    .entry((id.into(), context.scope))
                    .or_insert(0);
                let record = self.executions.entry(key.clone()).or_default();
                let yes = match state.evaluate_scenario_condition(
                    &condition,
                    &self.facts,
                    context,
                    &mut budget,
                ) {
                    Ok(yes) => yes,
                    Err(scenario_conditions::ConditionError::WorkExhausted) => return Ok(()),
                    Err(error) => return Err(format!("scenario condition: {error:?}")),
                };
                let actions = list(trigger, "actions");
                let cost: usize = actions
                    .iter()
                    .map(|a| {
                        if let Some(b) = a["box"].as_array() {
                            ((b[2].as_f64().unwrap() - b[0].as_f64().unwrap() + 1.0)
                                * (b[3].as_f64().unwrap() - b[1].as_f64().unwrap() + 1.0))
                                as usize
                        } else if let Some(roster) = a["roster"].as_array() {
                            roster
                                .iter()
                                .map(|r| r["count"].as_f64().unwrap() as usize)
                                .sum()
                        } else {
                            1
                        }
                    })
                    .sum();
                if budget.left < cost {
                    return Ok(());
                }
                let edge = yes && !record.previous;
                record.previous = yes;
                let repeat = &trigger["repeat"];
                let mode = repeat["mode"].as_str().unwrap_or("once");
                let max = if mode == "once" {
                    1
                } else {
                    repeat["max"].as_f64().unwrap() as u32
                };
                if !yes
                    || record.count >= max
                    || now < record.next
                    || (mode == "risingEdge" && !edge)
                {
                    continue;
                }
                if self.deferred.len() + actions.iter().filter(|a| a["kind"] == "reinforce").count()
                    > 256
                {
                    continue;
                }
                let execution = format!("scenario:{}", self.next_execution);
                self.next_execution += 1;
                if actions.iter().any(|a| {
                    !state.scenario_action_allowed(a, side(a, side(trigger, context.slot)))
                }) {
                    continue;
                }
                budget.left -= cost;
                record.count += 1;
                record.next = now + repeat["cooldown"].as_f64().unwrap_or(0.0);
                record.execution = Some(execution.clone());
                self.facts
                    .trigger_counts
                    .insert((id.into(), context.scope), record.count);
                for (index, action) in actions.iter().enumerate() {
                    let recipients = policy(action, policy(trigger, "public"));
                    let side = side(action, side(trigger, context.slot));
                    match action["kind"].as_str().unwrap() {
                        "say" => events.push(TickEvent::AuthoredScenario(ScenarioSay {
                            text: localized_text(&action["text"]).unwrap(),
                            localized: action["text"].clone(),
                            recipients: recipients.into(),
                            side,
                            execution: execution.clone(),
                            at_tick: tick,
                        })),
                        "objectiveActivate" | "objectiveComplete" => {
                            let id = action["objective"].as_str().unwrap();
                            let scope = context_key(context.scope);
                            let existing = self
                                .objectives
                                .iter()
                                .position(|o| o.id == id && o.scope == scope);
                            if action["kind"] == "objectiveActivate"
                                && existing.is_some_and(|i| self.objectives[i].state == "complete")
                            {
                                continue;
                            }
                            let def = list(&self.data, "objectives")
                                .iter()
                                .find(|o| o["id"] == id)
                                .unwrap();
                            let complete = action["kind"] == "objectiveComplete";
                            let objective = Objective {
                                id: id.into(),
                                scope,
                                text: def["text"].clone(),
                                state: if complete { "complete" } else { "active" }.into(),
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
                            if complete {
                                self.facts
                                    .completed_objectives
                                    .insert((id.into(), context.scope));
                            }
                        }
                        "reinforce" => {
                            let mut units = Vec::new();
                            for r in list(action, "roster") {
                                for _ in 0..r["count"].as_f64().unwrap() as usize {
                                    units.push(ArrivalUnit {
                                        kind: r["type"].as_str().unwrap().into(),
                                        authored: None,
                                    });
                                }
                            }
                            self.deferred.push(Arrival {
                                group: action["group"].as_str().unwrap().into(),
                                side,
                                units,
                                at: coords(&action["at"]),
                                order: action["order"].clone(),
                                expires: now + action["expires"].as_f64().unwrap(),
                            });
                            self.refresh_deferred_facts();
                        }
                        "damage" | "destroy" => state.scenario_damage_emitting(
                            action,
                            &format!("{execution}:{index}"),
                            &mut |effect| events.push(TickEvent::Environment(effect)),
                        )?,
                        _ => unreachable!("validated action"),
                    }
                }
            }
        }
        self.run_deferred(state, now)
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
        self.preflight_authored_scenario()?;
        let Some(mut scenario) = self.authored_scenario.take() else {
            return Ok(());
        };
        let result = (|| {
            for group in list(&scenario.data, "groups") {
                for unit in list(group, "units") {
                    if !self.scenario_authored_unit_allowed(
                        unit["side"].as_f64().unwrap() as usize,
                        unit["type"].as_str().unwrap(),
                        [
                            unit["x"].as_f64().unwrap() as usize,
                            unit["y"].as_f64().unwrap() as usize,
                        ],
                    ) {
                        return Err("scenario: authored unit unavailable in this match".into());
                    }
                }
            }
            for action in list(&scenario.data, "triggers")
                .iter()
                .flat_map(|t| list(t, "actions"))
            {
                if action["kind"] == "reinforce"
                    && !self
                        .scenario_action_allowed(action, action["side"].as_f64().unwrap() as usize)
                {
                    return Err("scenario: reinforcement unavailable in this match".into());
                }
            }
            scenario.run_deferred(self, 0.0)
        })();
        self.authored_scenario = Some(scenario);
        result
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
                || valid_scope(&o.scope).is_none()
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
        for d in &s.deferred {
            if d.side >= count
                || !d.expires.is_finite()
                || d.expires < 0.0
                || d.at[0] >= self.world.game.w
                || d.at[1] >= self.world.game.h
                || d.units.is_empty()
                || d.units.len() > 32
                || !s.facts.groups.contains_key(&d.group)
                || d.units.iter().any(|u| {
                    !self.scenario_type_allowed(d.side, &u.kind)
                        || u.authored
                            .as_ref()
                            .is_some_and(|name| !authored.contains(name))
                })
                || !matches!(
                    d.order["kind"].as_str(),
                    Some("hold" | "move" | "attackMove")
                )
                || (d.order["kind"] != "hold"
                    && (d.order["at"][0].as_f64().is_none_or(|x| {
                        !x.is_finite()
                            || x < 0.0
                            || x.fract() != 0.0
                            || x >= self.world.game.w as f64
                    }) || d.order["at"][1].as_f64().is_none_or(|y| {
                        !y.is_finite()
                            || y < 0.0
                            || y.fract() != 0.0
                            || y >= self.world.game.h as f64
                    })))
            {
                return Err("invalid authored deferred arrival".into());
            }
        }
        Ok(())
    }
    pub(crate) fn step_authored_scenario(
        &mut self,
        tick: u64,
        events: &mut Vec<TickEvent>,
    ) -> Result<(), String> {
        self.preflight_authored_scenario()?;
        if tick >= MAX_INTEGER {
            return Err("authored scenario tick exceeds exact integer range".into());
        }
        let Some(mut scenario) = self.authored_scenario.take() else {
            return Ok(());
        };
        let result = scenario.step(self, tick, events);
        self.authored_scenario = Some(scenario);
        result
    }
}

#[cfg(test)]
#[path = "authored_scenario/engine_tests.rs"]
mod engine_tests;
#[cfg(test)]
#[path = "authored_scenario/tests.rs"]
mod tests;
