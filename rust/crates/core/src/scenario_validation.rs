//! Bounded authored mission validation, independent of match admission and execution.
//!
//! The caller supplies an authoritative map document and the core's static catalog.
//! This validates the complete scenario graph without running triggers or changing a match.
use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};

use crate::catalog::Catalog;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScenarioValidationError(pub String);

impl std::fmt::Display for ScenarioValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "scenario: {}", self.0)
    }
}
impl std::error::Error for ScenarioValidationError {}

type Result<T> = std::result::Result<T, ScenarioValidationError>;

fn fail(detail: impl Into<String>) -> ScenarioValidationError {
    ScenarioValidationError(detail.into())
}

fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::Number(v) => v.as_f64().is_some_and(|v| v != 0.0),
        Value::String(v) => !v.is_empty(),
        _ => true,
    }
}

/// Preserve authored definitions, or migrate the legacy timed map triggers.
pub fn migrate_scenario(map: &Value) -> Option<Value> {
    if let Some(scenario) = map.get("scenario").filter(|s| truthy(s)) {
        return Some(scenario.clone());
    }
    let triggers = map.get("triggers")?.as_array()?;
    if triggers.is_empty() {
        return None;
    }
    Some(json!({"version":1,"areas":[],"groups":[],"objectives":[],
        "triggers":triggers.iter().enumerate().map(|(i,t)| {
            let mut actions = Vec::new();
            if let Some(text) = t.get("say").filter(|v| truthy(v)) {
                actions.push(json!({"kind":"say","text":{"en":text},"recipients":"public"}));
            }
            if let Some(bounds) = t.get("blow").filter(|v| truthy(v)) {
                actions.push(json!({"kind":"destroy","box":bounds}));
            }
            json!({"id":format!("legacy:{i}"),"scope":"match","recipients":"public",
                "condition":{"kind":"time","seconds":t.get("at")},
                "repeat":{"mode":"once"},"actions":actions})
        }).collect::<Vec<_>>() }))
}

fn stable_id(v: &Value) -> bool {
    v.as_str().is_some_and(|s| {
        (1..=64).contains(&s.len())
            && s.as_bytes()[0].is_ascii_alphanumeric()
            && s.bytes()
                .all(|c| c.is_ascii_alphanumeric() || b":_-".contains(&c))
    })
}

fn number(v: &Value, lo: f64, hi: f64) -> bool {
    v.as_f64()
        .is_some_and(|n| n.is_finite() && n >= lo && n <= hi)
}

fn integer(v: &Value, lo: usize, hi: usize) -> bool {
    number(v, lo as f64, hi as f64) && v.as_f64().is_some_and(|n| n.fract() == 0.0)
}

fn localized(v: &Value) -> bool {
    fn text(v: &Value) -> bool {
        v.as_str()
            .is_some_and(|s| (1..=280).contains(&s.encode_utf16().count()))
    }
    if v.is_string() {
        return text(v);
    }
    match v {
        Value::Object(values) => (1..=8).contains(&values.len()) && values.values().all(text),
        Value::Array(values) => (1..=8).contains(&values.len()) && values.iter().all(text),
        _ => false,
    }
}

fn recipients(v: Option<&Value>) -> bool {
    v.is_none_or(|v| matches!(v.as_str(), Some("public" | "side" | "team")))
}

fn policy<'a>(action: &'a Value, trigger: &'a Value) -> &'a str {
    action
        .get("recipients")
        .filter(|v| !v.is_null())
        .or_else(|| trigger.get("recipients").filter(|v| !v.is_null()))
        .and_then(Value::as_str)
        .unwrap_or("public")
}

struct Validator<'a> {
    map: &'a Value,
    catalog: &'a Catalog,
    names: BTreeMap<&'static str, BTreeSet<String>>,
    units: BTreeSet<String>,
    targets: BTreeSet<String>,
}

impl Validator<'_> {
    fn side(&self, v: &Value) -> bool {
        let count = self.map["spawns"].as_array().map_or(0, Vec::len);
        count > 0 && integer(v, 0, count - 1)
    }

    fn at(&self, v: &Value) -> bool {
        let Some(values) = v.as_array().filter(|v| v.len() == 2) else {
            return false;
        };
        let w = self.map["w"].as_f64().unwrap_or(0.0) as usize;
        let h = self.map["h"].as_f64().unwrap_or(0.0) as usize;
        w > 0 && h > 0 && integer(&values[0], 0, w - 1) && integer(&values[1], 0, h - 1)
    }

    fn box_cells(&self, v: &Value) -> Option<usize> {
        let values = v.as_array().filter(|v| v.len() == 4)?;
        if !self.at(&json!([values[0], values[1]])) || !self.at(&json!([values[2], values[3]])) {
            return None;
        }
        let coords: Vec<_> = values.iter().map(|v| v.as_f64().unwrap()).collect();
        if coords[0] > coords[2] || coords[1] > coords[3] {
            return None;
        }
        let cells = (coords[2] - coords[0] + 1.0) * (coords[3] - coords[1] + 1.0);
        (cells <= 256.0).then_some(cells as usize)
    }

    fn unit_type(&self, v: &Value) -> bool {
        v.as_str()
            .and_then(|name| self.catalog.unit(name).ok())
            .is_some_and(|kind| {
                ["structure", "building", "air"]
                    .iter()
                    .all(|key| kind.get(key).and_then(crate::catalog::Value::boolean) != Some(true))
            })
    }

    fn named(&self, list: &str, v: &Value) -> bool {
        v.as_str().is_some_and(|id| self.names[list].contains(id))
    }

    fn condition(
        &self,
        q: &Value,
        trigger: &Value,
        depth: usize,
        count: &mut usize,
        dependencies: &mut Vec<String>,
    ) -> bool {
        *count += 1;
        if *count > 32 || depth > 6 {
            return false;
        }
        let kind = q["kind"].as_str().unwrap_or("");
        if matches!(kind, "all" | "any") {
            return q["conditions"].as_array().is_some_and(|children| {
                (1..=32).contains(&children.len())
                    && children
                        .iter()
                        .all(|child| self.condition(child, trigger, depth + 1, count, dependencies))
            });
        }
        if q.get("side").is_some_and(|v| !self.side(v)) {
            return false;
        }
        match kind {
            "time" => number(&q["seconds"], 0.0, 7200.0),
            "ownership" | "capture" => {
                q["target"]
                    .as_str()
                    .is_some_and(|id| self.targets.contains(id))
                    && (self.side(&q["side"])
                        || trigger["scope"] != "match"
                        || self.side(&trigger["side"]))
            }
            "unitLost" => q["unit"].as_str().is_some_and(|id| self.units.contains(id)),
            "groupLost" => self.named("groups", &q["group"]),
            "groupCount" => {
                self.named("groups", &q["group"])
                    && integer(&q["count"], 0, 128)
                    && matches!(q["op"].as_str(), Some("atMost" | "atLeast" | "equal"))
            }
            "areaEntry" => {
                self.named("areas", &q["area"])
                    && q.get("group").is_none_or(|g| self.named("groups", g))
                    && (q.get("group").is_some()
                        || self.side(&q["side"])
                        || trigger["scope"] != "match"
                        || self.side(&trigger["side"]))
            }
            "objectiveComplete" => {
                dependencies.push(format!("o:{}", q["objective"].as_str().unwrap_or("")));
                self.named("objectives", &q["objective"])
            }
            "triggerComplete" => {
                dependencies.push(format!("t:{}", q["trigger"].as_str().unwrap_or("")));
                self.named("triggers", &q["trigger"])
            }
            _ => false,
        }
    }
}

fn list<'a>(scenario: &'a Value, key: &str) -> Result<&'a [Value]> {
    match scenario.get(key) {
        None | Some(Value::Null) => Ok(&[]),
        Some(Value::Array(values)) if values.len() <= 32 => Ok(values),
        _ => Err(fail(format!("{key} exceeds 32"))),
    }
}

/// Validate all schema limits, references and dependency cycles at the pinned prototype's values.
/// Map geometry and spawn shape are validated separately by the map importer.
pub fn validate_scenario(map: &Value, catalog: &Catalog) -> Result<()> {
    let Some(scenario) = migrate_scenario(map) else {
        return Ok(());
    };
    if scenario["version"].as_f64() != Some(1.0) {
        return Err(fail("version must be 1"));
    }
    let mut v = Validator {
        map,
        catalog,
        names: BTreeMap::new(),
        units: BTreeSet::new(),
        targets: BTreeSet::new(),
    };
    for key in ["triggers", "objectives", "groups", "areas"] {
        let mut ids = BTreeSet::new();
        for definition in list(&scenario, key)? {
            if !stable_id(&definition["id"])
                || !ids.insert(definition["id"].as_str().unwrap().to_owned())
            {
                return Err(fail(format!("{key} needs unique stable IDs")));
            }
        }
        v.names.insert(key, ids);
    }
    for area in list(&scenario, "areas")? {
        if v.box_cells(&area["box"]).is_none() {
            return Err(fail(format!(
                "area {} needs a valid box of up to 256 cells",
                area["id"].as_str().unwrap()
            )));
        }
    }
    for objective in list(&scenario, "objectives")? {
        if !localized(&objective["text"])
            || !recipients(objective.get("recipients"))
            || objective.get("side").is_some_and(|side| !v.side(side))
            || (objective
                .get("recipients")
                .is_some_and(|policy| policy != "public")
                && !v.side(&objective["side"]))
        {
            return Err(fail(format!(
                "objective {} needs text and valid recipients",
                objective["id"].as_str().unwrap()
            )));
        }
    }
    let mut unit_count = 0;
    for group in list(&scenario, "groups")? {
        let units = group["units"]
            .as_array()
            .ok_or_else(|| fail("too many authored units"))?;
        unit_count += units.len();
        if unit_count > 128 {
            return Err(fail("too many authored units"));
        }
        for unit in units {
            if !stable_id(&unit["id"])
                || !v.units.insert(unit["id"].as_str().unwrap().to_owned())
                || !v.unit_type(&unit["type"])
                || !v.side(&unit["side"])
                || !v.at(&json!([unit["x"], unit["y"]]))
            {
                return Err(fail(format!(
                    "group {} has an invalid unit",
                    group["id"].as_str().unwrap()
                )));
            }
        }
    }
    let points = map["points"].as_array().map_or(&[][..], Vec::as_slice);
    for (index, point) in points.iter().enumerate() {
        if point.get("id").is_some_and(|id| !stable_id(id)) {
            return Err(fail("capture targets need stable IDs"));
        }
        v.targets.insert(
            point["id"]
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| format!("point:{index}")),
        );
    }
    let regions = map["world"]["regions"]
        .as_array()
        .map_or(&[][..], Vec::as_slice);
    for region in regions {
        let id = match &region["id"] {
            Value::String(id) => id.clone(),
            Value::Number(number) => {
                let number = number.as_f64().unwrap();
                if number == 0.0 {
                    "0".into()
                } else {
                    number.to_string()
                }
            }
            id => id.to_string(),
        };
        v.targets.insert(format!("region:{id}"));
    }
    if v.targets.len() != points.len() + regions.len() {
        return Err(fail("capture targets need unique IDs"));
    }
    let mut edges: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut work = 0;
    for trigger in list(&scenario, "triggers")? {
        let id = trigger["id"].as_str().unwrap();
        let scope = trigger["scope"].as_str().unwrap_or("");
        if !matches!(scope, "match" | "player" | "team")
            || !recipients(trigger.get("recipients"))
            || trigger.get("side").is_some_and(|side| !v.side(side))
            || (scope == "match"
                && trigger
                    .get("recipients")
                    .is_some_and(|policy| policy != "public")
                && !v.side(&trigger["side"]))
        {
            return Err(fail(format!(
                "trigger {id} has invalid scope or recipients"
            )));
        }
        let default_repeat = json!({"mode":"once"});
        let repeat = trigger
            .get("repeat")
            .filter(|r| !r.is_null())
            .unwrap_or(&default_repeat);
        if !matches!(
            repeat["mode"].as_str(),
            Some("once" | "risingEdge" | "whileTrue")
        ) || (repeat["mode"] != "once"
            && (!number(&repeat["cooldown"], 0.05, 7200.0) || !integer(&repeat["max"], 1, 32)))
        {
            return Err(fail(format!(
                "trigger {id} needs a positive cooldown and finite execution limit"
            )));
        }
        let mut count = 0;
        let mut dependencies = Vec::new();
        if !v.condition(
            &trigger["condition"],
            trigger,
            0,
            &mut count,
            &mut dependencies,
        ) {
            return Err(fail(format!(
                "trigger {id} has an invalid condition or reference"
            )));
        }
        edges.insert(format!("t:{id}"), dependencies);
        let actions = trigger["actions"]
            .as_array()
            .filter(|actions| (1..=8).contains(&actions.len()))
            .ok_or_else(|| fail(format!("trigger {id} needs 1-8 actions")))?;
        let mut action_work = 0;
        for action in actions {
            if !truthy(action)
                || !recipients(action.get("recipients"))
                || action.get("side").is_some_and(|side| !v.side(side))
            {
                return Err(fail(format!("trigger {id} has invalid action recipients")));
            }
            match action["kind"].as_str().unwrap_or("") {
                "say" => {
                    if !localized(&action["text"])
                        || (policy(action, trigger) != "public"
                            && scope == "match"
                            && !v.side(action.get("side").unwrap_or(&trigger["side"])))
                    {
                        return Err(fail(format!("trigger {id} has invalid announcement")));
                    }
                }
                "objectiveActivate" | "objectiveComplete" => {
                    if !v.named("objectives", &action["objective"]) {
                        return Err(fail(format!(
                            "trigger {id} references an unknown objective"
                        )));
                    }
                    if action["kind"] == "objectiveComplete" {
                        edges
                            .entry(format!("o:{}", action["objective"].as_str().unwrap()))
                            .or_default()
                            .push(format!("t:{id}"));
                    }
                }
                "reinforce" => {
                    let roster = action["roster"].as_array();
                    let total: usize = roster.map_or(0, |roster| {
                        roster
                            .iter()
                            .map(|r| r["count"].as_f64().unwrap_or(0.0).clamp(0.0, 33.0) as usize)
                            .sum()
                    });
                    if !v.named("groups", &action["group"])
                        || !v.side(&action["side"])
                        || !v.at(&action["at"])
                        || !number(&action["expires"], 0.05, 7200.0)
                        || !roster.is_some_and(|roster| {
                            (1..=32).contains(&roster.len())
                                && roster
                                    .iter()
                                    .all(|r| v.unit_type(&r["type"]) && integer(&r["count"], 1, 32))
                        })
                        || total > 32
                        || !matches!(
                            action["order"]["kind"].as_str(),
                            Some("move" | "attackMove" | "hold")
                        )
                        || (action["order"]["kind"] != "hold" && !v.at(&action["order"]["at"]))
                    {
                        return Err(fail(format!("trigger {id} has invalid reinforcements")));
                    }
                    action_work += total;
                }
                "damage" | "destroy" => {
                    let cells = v.box_cells(&action["box"]);
                    if cells.is_none()
                        || (action["kind"] == "damage"
                            && !number(&action["damage"], 0.01, 100000.0))
                    {
                        return Err(fail(format!("trigger {id} has an invalid damage area")));
                    }
                    action_work += cells.unwrap();
                }
                _ => return Err(fail(format!("trigger {id} has an unknown action"))),
            }
            action_work += 1;
        }
        work += (count + action_work)
            * if scope == "match" {
                1
            } else {
                map["spawns"].as_array().map_or(0, Vec::len)
            };
    }
    if work > 4096 {
        return Err(fail("total trigger work exceeds 4096"));
    }
    fn cyclic(
        key: &str,
        edges: &BTreeMap<String, Vec<String>>,
        visited: &mut BTreeSet<String>,
        active: &mut BTreeSet<String>,
    ) -> bool {
        if active.contains(key) {
            return true;
        }
        if !visited.insert(key.to_owned()) {
            return false;
        }
        active.insert(key.to_owned());
        if edges
            .get(key)
            .is_some_and(|next| next.iter().any(|key| cyclic(key, edges, visited, active)))
        {
            return true;
        }
        active.remove(key);
        false
    }
    let mut visited = BTreeSet::new();
    let mut active = BTreeSet::new();
    if edges
        .keys()
        .any(|key| cyclic(key, &edges, &mut visited, &mut active))
    {
        return Err(fail("objective or trigger dependencies form a cycle"));
    }
    Ok(())
}

#[cfg(test)]
#[path = "scenario_validation/tests.rs"]
mod tests;
