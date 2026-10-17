//! Bounded authored timed announcements and objectives over ordinary match ticks.
use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::map::AuthoritativeMap;

const MAX_TICK: u64 = 9_007_199_254_740_991;
const LAST_AUTHORED_TICK: u64 = 144_000;
const TICK_SECONDS: f64 = 1.0 / 20.0;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TriggerState {
    id: String,
    previous: bool,
    execution: Option<u64>,
    at_tick: Option<u64>,
}

/// Private canonical mission state. Definition arrays and objective insertion order
/// retain authored ordering; no recipient projection contains this definition.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthoredMission {
    definition: Value,
    first_tick: Option<u64>,
    last_tick: Option<u64>,
    triggers: Vec<TriggerState>,
    objectives: Vec<Value>,
    next_execution: u64,
}

fn fail(detail: impl AsRef<str>) -> String {
    format!("authored mission: {}", detail.as_ref())
}

fn list<'a>(definition: &'a Value, key: &str) -> Result<&'a [Value], String> {
    match definition.get(key) {
        None => Ok(&[]),
        Some(Value::Array(rows)) if rows.len() <= 32 => Ok(rows),
        _ => Err(fail(format!("{key} must be an array of at most 32 rows"))),
    }
}

fn stable_id(value: &Value) -> bool {
    value.as_str().is_some_and(|id| {
        (1..=64).contains(&id.len())
            && id.as_bytes()[0].is_ascii_alphanumeric()
            && id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b":_-".contains(&byte))
    })
}

fn text(value: &Value) -> bool {
    value
        .as_str()
        .is_some_and(|value| (1..=280).contains(&value.encode_utf16().count()))
}

fn localized(value: &Value) -> bool {
    text(value)
        || value.as_object().is_some_and(|values| {
            (1..=8).contains(&values.len())
                && values.values().all(text)
                // Map JSON does not retain language-key insertion order. Require
                // English so the prototype's fallback never depends on that order.
                && values.get("en").is_some_and(text)
        })
}

fn side(row: &Value) -> Option<usize> {
    row.get("side")
        .and_then(Value::as_f64)
        .filter(|value| {
            value.is_finite() && *value >= 0.0 && value.fract() == 0.0 && *value <= MAX_TICK as f64
        })
        .and_then(|value| usize::try_from(value as u64).ok())
}

fn policy<'a>(row: &'a Value, fallback: &'a str) -> &'a str {
    row.get("recipients")
        .and_then(Value::as_str)
        .unwrap_or(fallback)
}

fn validate_recipients(row: &Value, player_count: usize) -> Result<(), String> {
    if row
        .get("recipients")
        .is_some_and(|value| !matches!(value.as_str(), Some("public" | "side" | "team")))
        || row
            .get("side")
            .is_some_and(|_| side(row).is_none_or(|slot| slot >= player_count))
    {
        return Err(fail("invalid recipient policy or side"));
    }
    Ok(())
}

fn validate_definition(definition: &Value, player_count: usize) -> Result<(), String> {
    if player_count == 0 || !definition.is_object() || definition["version"].as_f64() != Some(1.0) {
        return Err(fail("version 1 and at least one player are required"));
    }
    if !list(definition, "groups")?.is_empty() || !list(definition, "areas")?.is_empty() {
        return Err(fail("authored groups and areas are unsupported"));
    }
    let objectives = list(definition, "objectives")?;
    let mut objective_ids = BTreeSet::new();
    for objective in objectives {
        if !stable_id(&objective["id"])
            || !objective_ids.insert(objective["id"].as_str().unwrap().to_owned())
        {
            return Err(fail("objectives require unique stable IDs"));
        }
        if !localized(&objective["text"]) {
            return Err(fail(
                "objective text requires a string or localized object with en",
            ));
        }
        validate_recipients(objective, player_count)?;
        if policy(objective, "public") != "public" && side(objective).is_none() {
            return Err(fail("private objectives require a side"));
        }
    }
    let mut trigger_ids = BTreeSet::new();
    for trigger in list(definition, "triggers")? {
        if !stable_id(&trigger["id"])
            || !trigger_ids.insert(trigger["id"].as_str().unwrap().to_owned())
        {
            return Err(fail("triggers require unique stable IDs"));
        }
        if trigger["scope"] != "match" {
            return Err(fail("only match-scoped triggers are supported"));
        }
        validate_recipients(trigger, player_count)?;
        if policy(trigger, "public") != "public" && side(trigger).is_none() {
            return Err(fail("private match triggers require a side"));
        }
        if trigger.get("repeat").is_some_and(|repeat| {
            repeat["mode"] != "once" || repeat.as_object().is_none_or(|fields| fields.len() != 1)
        }) {
            return Err(fail(
                "only once repeat without cooldown or max is supported",
            ));
        }
        let condition = &trigger["condition"];
        if condition["kind"] != "time"
            || !condition["seconds"]
                .as_f64()
                .is_some_and(|seconds| seconds.is_finite() && (0.0..=7200.0).contains(&seconds))
        {
            return Err(fail("only bounded time conditions are supported"));
        }
        validate_recipients(condition, player_count)?;
        let Some(actions) = trigger["actions"]
            .as_array()
            .filter(|actions| (1..=8).contains(&actions.len()))
        else {
            return Err(fail("triggers require 1-8 actions"));
        };
        for action in actions {
            if action.get("box").is_some() || action.get("roster").is_some() {
                return Err(fail(
                    "mission message/objective actions cannot carry damage or roster costs",
                ));
            }
            validate_recipients(action, player_count)?;
            match action["kind"].as_str() {
                Some("say") => {
                    if !localized(&action["text"]) {
                        return Err(fail(
                            "Say text requires a string or localized object with en",
                        ));
                    }
                    if policy(action, policy(trigger, "public")) != "public"
                        && side(action).or_else(|| side(trigger)).is_none()
                    {
                        return Err(fail("private Say actions require a side"));
                    }
                }
                Some("objectiveActivate" | "objectiveComplete") => {
                    if !action["objective"]
                        .as_str()
                        .is_some_and(|id| objective_ids.contains(id))
                    {
                        return Err(fail("action references an unknown objective"));
                    }
                }
                _ => {
                    return Err(fail(
                        "only Say, objectiveActivate and objectiveComplete are supported",
                    ));
                }
            }
        }
    }
    Ok(())
}

impl AuthoredMission {
    /// Canonical timed execution used by the single persisted scenario owner.
    pub(crate) fn canonical_checkpoint(
        definition: &Value,
        last_tick: Option<u64>,
        player_count: usize,
    ) -> Result<(Value, Vec<Value>), String> {
        validate_definition(definition, player_count)?;
        if last_tick == Some(0) {
            return Err(fail("ordinary mission execution begins at tick one"));
        }
        let initial = Self::initial(definition.clone());
        let (canonical, shots) = initial.reconstruct(last_tick.map(|_| 1), last_tick)?;
        let mut executions = serde_json::Map::new();
        for trigger in &canonical.triggers {
            executions.insert(
                format!("{}@match", trigger.id),
                json!({
                    "count":u32::from(trigger.execution.is_some()),
                    "next":trigger.at_tick.map_or(0.0, |tick| tick as f64 * TICK_SECONDS),
                    "previous":trigger.previous,
                    "execution":trigger.execution.map(|id| format!("scenario:{id}"))
                }),
            );
        }
        Ok((
            json!({"executions":executions,"objectives":canonical.objectives,
            "next_execution":canonical.next_execution}),
            shots,
        ))
    }

    /// Legacy map triggers retain their existing owner. A present authored
    /// scenario is admitted completely or refused before match publication.
    pub fn from_map(map: &AuthoritativeMap) -> Result<Option<Self>, String> {
        let Some(definition) = map
            .source()
            .get("scenario")
            .filter(|value| !value.is_null())
        else {
            return Ok(None);
        };
        validate_definition(definition, map.spawns().len())?;
        Ok(Some(Self::initial(definition.clone())))
    }

    fn initial(definition: Value) -> Self {
        Self {
            definition,
            first_tick: None,
            last_tick: None,
            triggers: Vec::new(),
            objectives: Vec::new(),
            next_execution: 1,
        }
    }

    pub fn definition(&self) -> &Value {
        &self.definition
    }

    pub fn last_tick(&self) -> Option<u64> {
        self.last_tick
    }

    pub fn matches_map(&self, map: &AuthoritativeMap) -> bool {
        map.source().get("scenario") == Some(&self.definition)
    }

    /// Rebuild at most 32 executions, regardless of match duration. Comparing the
    /// whole state rejects altered execution IDs, objective rows and clocks on load.
    pub fn validate(&self, player_count: usize) -> Result<(), String> {
        validate_definition(&self.definition, player_count)?;
        let (expected, _) = self.reconstruct(self.first_tick, self.last_tick)?;
        if self != &expected {
            return Err(fail(
                "persisted mission state does not match canonical execution",
            ));
        }
        Ok(())
    }

    /// Tick zero is available to component callers. Ordinary matches start at
    /// tick one and must advance consecutively while the mission is running.
    pub fn step(&mut self, tick: u64, player_count: usize) -> Result<Vec<Value>, String> {
        self.validate(player_count)?;
        if tick > MAX_TICK
            || match self.last_tick {
                Some(last) => last.checked_add(1) != Some(tick),
                None => tick > 1,
            }
        {
            return Err(fail(
                "mission ticks must advance consecutively from zero or one",
            ));
        }
        let (next, shots) = self.reconstruct(self.first_tick.or(Some(tick)), Some(tick))?;
        *self = next;
        Ok(shots
            .into_iter()
            .filter(|shot| shot["atTick"].as_u64() == Some(tick))
            .collect())
    }

    /// The spectator's synthetic identity never grants a side or team view.
    pub fn projection(&self, slot: usize, teams: &[i32], spectator: bool) -> Value {
        let objectives: Vec<_> = self
            .objectives
            .iter()
            .filter(|objective| match objective["recipients"].as_str() {
                Some("public") => true,
                Some("side") if !spectator => side(objective) == Some(slot),
                Some("team") if !spectator => side(objective).is_some_and(|owner| {
                    teams
                        .get(owner)
                        .is_some_and(|team| teams.get(slot) == Some(team))
                }),
                _ => false,
            })
            .cloned()
            .collect();
        json!({"version":1,"objectives":objectives})
    }

    /// Check the complete Say row against this mission's committed execution.
    /// Expected rows are derived transiently; no event history grows with time.
    pub fn accepts_shot(&self, row: &Value) -> bool {
        if validate_definition(&self.definition, usize::MAX).is_err() {
            return false;
        }
        self.reconstruct(self.first_tick, self.last_tick)
            .is_ok_and(|(expected, shots)| {
                self == &expected && shots.iter().any(|shot| shot == row)
            })
    }

    fn reconstruct(
        &self,
        first_tick: Option<u64>,
        last_tick: Option<u64>,
    ) -> Result<(Self, Vec<Value>), String> {
        let mut expected = Self::initial(self.definition.clone());
        let (first, last) = match (first_tick, last_tick) {
            (None, None) => return Ok((expected, Vec::new())),
            (Some(first), Some(last)) if first <= 1 && first <= last && last <= MAX_TICK => {
                (first, last)
            }
            _ => return Err(fail("invalid persisted mission clock")),
        };
        expected.first_tick = Some(first);
        expected.last_tick = Some(last);
        let definitions = list(&self.definition, "triggers")?;
        let mut events = Vec::new();
        for (index, trigger) in definitions.iter().enumerate() {
            let seconds = trigger["condition"]["seconds"]
                .as_f64()
                .ok_or_else(|| fail("invalid time condition"))?;
            // Binary search uses the prototype's tick*TICK predicate, avoiding a
            // rounded seconds*20 conversion at authored threshold boundaries.
            let (mut low, mut high) = (first, LAST_AUTHORED_TICK.max(first));
            while low < high {
                let middle = low + (high - low) / 2;
                if middle as f64 * TICK_SECONDS >= seconds {
                    high = middle;
                } else {
                    low = middle + 1;
                }
            }
            let reached = last as f64 * TICK_SECONDS >= seconds;
            expected.triggers.push(TriggerState {
                id: trigger["id"].as_str().unwrap_or_default().to_owned(),
                previous: reached,
                execution: None,
                at_tick: None,
            });
            if reached {
                events.push((low, index));
            }
        }
        events.sort_unstable();
        let mut shots = Vec::new();
        for (tick, index) in events {
            let trigger = &definitions[index];
            let execution = expected.next_execution;
            expected.next_execution += 1;
            expected.triggers[index].execution = Some(execution);
            expected.triggers[index].at_tick = Some(tick);
            let execution = format!("scenario:{execution}");
            for action in trigger["actions"]
                .as_array()
                .ok_or_else(|| fail("invalid actions"))?
            {
                let action_side = side(action).or_else(|| side(trigger)).unwrap_or(0);
                let action_policy = policy(action, policy(trigger, "public"));
                if action["kind"] == "say" {
                    let localized = &action["text"];
                    let text = localized
                        .as_str()
                        .map(Value::from)
                        .unwrap_or_else(|| localized["en"].clone());
                    shots.push(json!({"k":"say","text":text,"localized":localized,
                        "scenarioRecipients":action_policy,"scenarioSide":action_side,
                        "pub":action_policy=="public","execution":execution,"atTick":tick}));
                    continue;
                }
                let objective = list(&self.definition, "objectives")?
                    .iter()
                    .find(|objective| objective["id"] == action["objective"])
                    .ok_or_else(|| fail("unknown objective"))?;
                let position = expected
                    .objectives
                    .iter()
                    .position(|row| row["id"] == objective["id"]);
                if action["kind"] == "objectiveActivate"
                    && position.is_some_and(|position| {
                        expected.objectives[position]["state"] == "complete"
                    })
                {
                    continue;
                }
                let row = json!({"id":objective["id"],"scope":"match","text":objective["text"],
                    "state":if action["kind"]=="objectiveComplete" {"complete"} else {"active"},
                    "recipients":policy(objective,action_policy),"side":side(objective).unwrap_or(action_side),
                    "updated":tick as f64 * TICK_SECONDS,"execution":execution});
                if let Some(position) = position {
                    expected.objectives[position] = row;
                } else {
                    expected.objectives.push(row);
                }
            }
        }
        Ok((expected, shots))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mission() -> AuthoredMission {
        let definition = json!({"version":1,"groups":[],"areas":[],
            "objectives":[
                {"id":"public","text":{"en":"Public","es":"Publico"}},
                {"id":"side","text":"Private","recipients":"side","side":1},
                {"id":"team","text":"Team","recipients":"team","side":1}],
            "triggers":[
                {"id":"later","scope":"match","condition":{"kind":"time","seconds":0.15},
                 "actions":[{"kind":"objectiveActivate","objective":"public"},
                            {"kind":"say","text":"Late"}]},
                {"id":"early","scope":"match","condition":{"kind":"time","seconds":0.05},
                 "actions":[{"kind":"objectiveActivate","objective":"team"},
                            {"kind":"objectiveComplete","objective":"public"},
                            {"kind":"objectiveActivate","objective":"side"},
                            {"kind":"say","text":{"en":"Private","es":"Privado"},"recipients":"side","side":1}]},
                {"id":"same-time","scope":"match","condition":{"kind":"time","seconds":0.05},
                 "actions":[{"kind":"say","text":"Public"}]}]});
        validate_definition(&definition, 3).unwrap();
        AuthoredMission::initial(definition)
    }

    #[test]
    fn authored_mission_say_shape_and_public_side_team_projection() {
        let mut mission = mission();
        let shots = mission.step(1, 3).unwrap();
        assert_eq!(
            shots,
            vec![
                json!({"k":"say","text":"Private","localized":{"en":"Private","es":"Privado"},
                "scenarioRecipients":"side","scenarioSide":1,"pub":false,"execution":"scenario:1","atTick":1}),
                json!({"k":"say","text":"Public","localized":"Public", "scenarioRecipients":"public",
                "scenarioSide":0,"pub":true,"execution":"scenario:2","atTick":1})
            ]
        );
        let ids = |view: Value| {
            view["objectives"]
                .as_array()
                .unwrap()
                .iter()
                .map(|row| row["id"].as_str().unwrap().to_owned())
                .collect::<Vec<_>>()
        };
        assert_eq!(ids(mission.projection(0, &[0, 1, 1], false)), ["public"]);
        assert_eq!(
            ids(mission.projection(1, &[0, 1, 1], false)),
            ["team", "public", "side"]
        );
        assert_eq!(
            ids(mission.projection(2, &[0, 1, 1], false)),
            ["team", "public"]
        );
        assert_eq!(ids(mission.projection(1, &[0, 1, 1], true)), ["public"]);
        assert!(shots.iter().all(|shot| mission.accepts_shot(shot)));
        let mut changed = shots[0].clone();
        changed["scenarioSide"] = json!(0);
        assert!(!mission.accepts_shot(&changed));
        changed = shots[0].clone();
        changed["x"] = json!(0);
        assert!(!mission.accepts_shot(&changed));
    }

    #[test]
    fn authored_mission_uses_prototype_multiplication_at_fractional_tick_boundary() {
        let seconds = 3.0 * TICK_SECONDS;
        assert!(seconds > 3.0 / 20.0);
        let mut mission = AuthoredMission::initial(json!({"version":1,
            "objectives":[{"id":"clock","text":"Clock"}],
            "triggers":[{"id":"clock","scope":"match","condition":{"kind":"time","seconds":seconds},
                "actions":[{"kind":"objectiveActivate","objective":"clock"}]}]}));
        mission.step(1, 1).unwrap();
        mission.step(2, 1).unwrap();
        assert!(
            mission.projection(0, &[0], true)["objectives"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        mission.step(3, 1).unwrap();
        let view = mission.projection(0, &[0], true);
        assert_eq!(view["objectives"][0]["updated"].as_f64(), Some(seconds));
    }

    #[test]
    fn authored_mission_restore_retains_execution_and_completed_objective() {
        let mut mission = mission();
        mission.step(1, 3).unwrap();
        mission.step(2, 3).unwrap();
        let mut restored: AuthoredMission =
            serde_json::from_value(serde_json::to_value(&mission).unwrap()).unwrap();
        restored.validate(3).unwrap();
        assert_eq!(mission.step(3, 3), restored.step(3, 3));
        assert_eq!(mission, restored);
        let public = &mission.projection(0, &[0, 1, 1], false)["objectives"][0];
        assert_eq!(
            public,
            &json!({"id":"public","scope":"match","text":{"en":"Public","es":"Publico"},
            "state":"complete","recipients":"public","side":0,"updated":0.05,"execution":"scenario:1"})
        );
        assert!(mission.step(4, 3).unwrap().is_empty());
        let mut tampered = serde_json::to_value(&mission).unwrap();
        tampered["objectives"][1]["state"] = json!("active");
        let tampered: AuthoredMission = serde_json::from_value(tampered).unwrap();
        assert!(tampered.validate(3).is_err());
    }

    #[test]
    fn authored_mission_refuses_unsupported_and_noncanonical_definitions() {
        let definition = mission().definition;
        for (key, value) in [
            ("scope", json!("player")),
            ("repeat", json!({"mode":"whileTrue","max":1,"cooldown":1})),
            (
                "condition",
                json!({"kind":"ownership","target":"point:0","side":0}),
            ),
            ("actions", json!([{"kind":"destroy","box":[0,0,1,1]}])),
        ] {
            let mut changed = definition.clone();
            changed["triggers"][0][key] = value;
            assert!(validate_definition(&changed, 3).is_err());
        }
        for (field, value) in [
            ("box", json!([0, 0, 63, 63])),
            ("roster", json!([{ "count": 4096 }])),
        ] {
            let mut changed = definition.clone();
            changed["triggers"][0]["actions"][1][field] = value;
            assert!(validate_definition(&changed, 3).is_err());
        }
        let mut changed = definition.clone();
        changed["objectives"][1]["side"] = json!(3);
        assert!(validate_definition(&changed, 3).is_err());
        let mut changed = definition;
        changed["triggers"][0]["actions"][1]["text"] = json!({"es":"Sin ingles"});
        assert!(validate_definition(&changed, 3).is_err());
        let mut mission = mission();
        let original = mission.clone();
        assert!(mission.step(2, 3).is_err());
        assert_eq!(mission, original);
        mission.step(1, 3).unwrap();
        let original = mission.clone();
        assert!(mission.step(1, 3).is_err());
        assert_eq!(mission, original);
    }
}
