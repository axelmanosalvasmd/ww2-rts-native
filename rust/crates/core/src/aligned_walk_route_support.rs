//! Preserved MOV11 comparison and observation helpers from 2a13bf4e.
use super::super::{Match, Replay};
use crate::game::Point;
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Write};

pub(super) fn number(value: f64) -> Value {
    json!({"bits":format!("{:016x}",value.to_bits()),"value":if value.is_finite(){json!(value)}else{json!(if value.is_nan(){"NaN"}else if value.is_sign_positive(){"Infinity"}else{"-Infinity"})}})
}
pub(super) fn point(value: Point) -> Value {
    json!({"x":number(value.x),"z":number(value.z)})
}
pub(super) fn goal(value: Option<Point>) -> Value {
    value.map_or(Value::Null, point)
}
pub(super) fn optional_number(value: Option<f64>) -> Value {
    value.map_or_else(|| json!({"absent":true}), number)
}
impl Replay {
    pub(crate) fn observe_attack_move(&self, m: &Match, id: u32, event: &str) {
        let u = &m.world.game.units[&id];
        let target = m.firing.units.get(&id).map_or(0, |f| f.target_id);
        self.0.lock().unwrap().scenario_branches.push(json!({"tick":m.world.game.tick,"id":id,"event":event,"targetId":target,
            "x":number(u.x),"z":number(u.z),"hp":number(u.hp),"goal":goal(u.amove),"path":u.path.iter().copied().map(point).collect::<Vec<_>>()}));
    }
    pub(crate) fn observe_moving(&self, id: u32, moving: bool) {
        self.0
            .lock()
            .unwrap()
            .scenario_motion
            .push(json!({"id":id,"moving":moving}));
    }
}
pub(super) fn proposed_registry() -> Value {
    serde_json::from_str(include_str!(
        "../../../../docs/testing/mov11-preserved-defaults.json"
    ))
    .unwrap()
}
// The owner-approved four-currency exception remains a separate result.
pub(super) fn approved_currency_only(snapshot: &Value) -> (Value, Vec<String>) {
    let mut result = snapshot.clone();
    let mut mapped = Vec::new();
    if let Some(players) = result
        .get_mut("players")
        .and_then(Value::as_array_mut)
        .filter(|p| p.len() == 2)
    {
        for (slot, player) in players.iter_mut().enumerate() {
            if player["slot"].as_u64() != Some(slot as u64)
                || player["team"].as_u64() != Some(slot as u64)
            {
                continue;
            }
            if let Some(object) = player.as_object_mut() {
                for field in ["fuel", "mun"] {
                    if object
                        .get(field)
                        .is_none_or(|v| *v == json!({"absent":true}))
                    {
                        object.insert(field.into(), number(0.0));
                        mapped.push(format!("$.players[{slot}].{field}"));
                    }
                }
            }
        }
    }
    (result, mapped)
}
// Proposed per-field registry, diagnostic only. No owner-approved parity verdict.
pub(super) fn proposed_defaults(snapshot: &Value) -> (Value, Vec<String>, Vec<Value>) {
    let (mut result, mapped) = approved_currency_only(snapshot);
    let mut booleans = Vec::new();
    if let Some(units) = result.get_mut("units").and_then(Value::as_array_mut) {
        for (index, unit) in units.iter_mut().enumerate() {
            if let Some(object) = unit.as_object_mut() {
                for field in ["holdFire", "holdPos"] {
                    if object
                        .get(field)
                        .is_none_or(|value| *value == json!({"absent":true}))
                    {
                        booleans.push(json!({"path":format!("$.units[{index}].{field}"),"actor":object.get("id"),"field":field}));
                        object.insert(field.into(), json!(false));
                    }
                }
            }
        }
    }
    (result, mapped, booleans)
}

pub(super) type BooleanCounts = BTreeMap<(String, String, String), usize>;
pub(super) fn count_mappings(
    currencies: Vec<String>,
    booleans: Vec<Value>,
    currency_counts: &mut BTreeMap<String, usize>,
    boolean_counts: &mut BooleanCounts,
) {
    for path in currencies {
        *currency_counts.entry(path).or_default() += 1;
    }
    for row in booleans {
        let key = (
            row["path"].as_str().unwrap().to_owned(),
            row["actor"].to_string(),
            row["field"].as_str().unwrap().to_owned(),
        );
        *boolean_counts.entry(key).or_default() += 1;
    }
}
pub(super) fn boolean_counts(counts: &BooleanCounts) -> Vec<Value> {
    counts
        .iter()
        .map(|((path, actor, field), count)| {
            let actor: Value = serde_json::from_str(actor).unwrap();
            json!({"path":path,"actor":actor,"field":field,"count":count,"default":false})
        })
        .collect()
}

pub(super) fn prefix_alignment(
    source: &Value,
    frame: &Value,
    replay: &Replay,
    checked: &mut usize,
) -> Result<(), Value> {
    let tape = replay.0.lock().unwrap();
    let draws = &tape.used;
    let expected = source["inputReceipt"]["draws"].as_array().unwrap();
    let count = frame["drawCount"].as_u64().unwrap() as usize;
    if draws.len() != count || count < *checked {
        return Err(
            json!({"kind":"drawCount","source":count,"native":draws.len(),"checked":checked}),
        );
    }
    if count > expected.len() {
        return Err(json!({"kind":"sourceDrawsTruncated","source":expected.len(),"native":count}));
    }
    for i in *checked..count {
        let (a, b) = (&expected[i], &draws[i]);
        if a != b {
            return Err(json!({"kind":"draw","index":i,"source":a,"native":b}));
        }
        if a["consumer"][0]
            .as_str()
            .is_none_or(|v| v.starts_with("UNMAPPED"))
        {
            return Err(json!({"kind":"unmapped","index":i,"source":a,"native":b}));
        }
        *checked = i + 1;
    }
    Ok(())
}
pub(super) fn stream_case(
    writer: &mut impl Write,
    metadata: &Value,
    native: &[Value],
    source: &Value,
) -> Result<(), String> {
    // Borrow complete raw frames into the output stream, with no full-frame clones.
    let mut header = serde_json::to_vec(metadata).map_err(|e| e.to_string())?;
    assert_eq!(header.pop(), Some(b'}'));
    writer.write_all(&header).map_err(|e| e.to_string())?;
    writer
        .write_all(b",\"nativeFrames\":")
        .map_err(|e| e.to_string())?;
    serde_json::to_writer(&mut *writer, native).map_err(|e| e.to_string())?;
    writer
        .write_all(b",\"sourceRawFrames\":")
        .map_err(|e| e.to_string())?;
    serde_json::to_writer(&mut *writer, &source["frames"]).map_err(|e| e.to_string())?;
    writer.write_all(b"}").map_err(|e| e.to_string())
}
