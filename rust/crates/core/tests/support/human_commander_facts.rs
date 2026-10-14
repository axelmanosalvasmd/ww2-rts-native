//! Original delivered facts and callback projection shared by human-state proofs.

use super::core_api::{
    game::Point,
    human_commander::{HumanSighting, HumanUnit, HumanView, Hurt, OwnerHurtBatch},
    human_commander_inputs::{InputKind, Unit},
    human_commander_state::InputReceipt,
};
use serde_json::{Value, json};

pub fn point(v: &Value) -> Point {
    Point {
        x: v["x"].as_f64().unwrap(),
        z: v["z"].as_f64().unwrap(),
    }
}
pub fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::Number(v) => v.as_f64().unwrap() != 0.0,
        Value::String(v) => !v.is_empty(),
        _ => true,
    }
}
pub fn view(v: &Value) -> HumanView<()> {
    HumanView {
        tick: v["tick"].as_u64().unwrap(),
        spawn: (!v["spawn"].is_null()).then(|| point(&v["spawn"])),
        units: v["units"]
            .as_array()
            .unwrap()
            .iter()
            .map(|u| HumanUnit {
                motion: Unit {
                    id: u["id"].as_u64().unwrap() as u32,
                    at: point(u),
                    world_goal: (!u["worldGoal"].is_null()).then(|| point(&u["worldGoal"])),
                    amove: (!u["amove"].is_null()).then(|| point(&u["amove"])),
                    target_id: u["targetId"].as_u64().unwrap_or(0) as u32,
                    retreating: truthy(&u["retreating"]),
                },
                owner: u["owner"].as_u64().unwrap() as u32,
                kind: u["type"].as_str().unwrap().to_owned(),
                hp: u["hp"].as_f64().unwrap(),
                air: truthy(&u["air"]),
                build: truthy(&u["build"]),
                dig: truthy(&u["dig"]),
                entrench: truthy(&u["entrench"]),
                garrison: u["garrison"].as_i64().is_some_and(|n| n >= 0),
            })
            .collect(),
        visible: v["visible"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n.as_u64().unwrap() as u32)
            .collect(),
        sightings: v["sightings"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| HumanSighting {
                id: s["id"].as_u64().unwrap() as u32,
                payload: s.clone(),
            })
            .collect(),
        packet: (),
    }
}
pub fn batch(v: &Value) -> OwnerHurtBatch {
    OwnerHurtBatch {
        generation: v["generation"].as_u64().unwrap(),
        through_ordinal: v["through_ordinal"].as_u64().unwrap(),
        events: v["events"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| Hurt {
                ordinal: e["ordinal"].as_u64().unwrap(),
                id: e["id"].as_u64().unwrap() as u32,
                at: point(&e["at"]),
            })
            .collect(),
    }
}
pub fn exact(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => {
            a.as_f64().unwrap().to_bits() == b.as_f64().unwrap().to_bits()
        }
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| exact(a, b))
        }
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len() && a.iter().all(|(k, a)| b.get(k).is_some_and(|b| exact(a, b)))
        }
        _ => a == b,
    }
}

/// Source onInput exposes no minimap flag; actual acceptance remains a host receipt.
pub fn input_receipt(i: &InputReceipt) -> Value {
    let mut value = json!({"tick":i.tick,"origin":i.origin,"kind":match i.kind {
        InputKind::Camera=>"camera",InputKind::Select=>"select",InputKind::Key=>"key",InputKind::Click=>"click",
    },"x":i.at.x,"z":i.at.z,"camera":{"x":i.camera.x,"z":i.camera.z}});
    if let Some(ids) = &i.ids {
        value["ids"] = json!(ids);
    }
    if let Some(command) = &i.command {
        value["cmd"] = command.clone();
    }
    value
}
