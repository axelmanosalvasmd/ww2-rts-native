use serde_json::{Value, json};
use ww2_core::{
    game::Point,
    human_commander::{HumanSighting, HumanUnit, HumanView, Hurt, OwnerHurtBatch, TickControl},
    human_commander_inputs::{InputKind, Unit},
    human_commander_state::{Error, HumanState},
};

fn point(v: &Value) -> Point {
    Point {
        x: v["x"].as_f64().unwrap(),
        z: v["z"].as_f64().unwrap(),
    }
}
fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::Number(v) => v.as_f64().unwrap() != 0.0,
        Value::String(v) => !v.is_empty(),
        _ => true,
    }
}
fn view(v: &Value) -> HumanView<()> {
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
fn batch(v: &Value) -> OwnerHurtBatch {
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
fn exact(a: &Value, b: &Value) -> bool {
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

// Replay actual delivered facts and Source-collected policy output through this component.
// Match execution and native planner policy are deliberately not replaced or claimed here.
#[test]
fn ordinary_human_state_matches_actual_source_opening_and_real_hurt_traces() {
    for bytes in [
        include_str!("fixtures/human-commander-runtime.json"),
        include_str!("fixtures/human-commander-alerts.json"),
    ] {
        let f: Value = serde_json::from_str(bytes).unwrap();
        for case in f["cases"].as_array().unwrap() {
            let slot = case["slot"].as_u64().unwrap() as u32;
            let mut human = HumanState::new(
                case["sourceSeed"].as_i64().unwrap() as u32,
                slot,
                point(&case["sourceSpawn"]),
            );
            assert_eq!(human.camera(), None);
            assert!(human.wants_view(0));
            let source_command = case["inputs"]
                .as_array()
                .unwrap()
                .iter()
                .find_map(|i| i.get("cmd"))
                .unwrap()
                .clone();
            let mut delivered = view(&case["deliveries"][0]["viewSnapshot"]);
            let mut actual_inputs = Vec::new();
            let mut actual_alerts = Vec::new();
            let mut answers = Vec::new();
            let mut plans = 0;
            for row in case["batches"].as_array().unwrap() {
                let tick = row["tick"].as_u64().unwrap();
                if let Some(d) = case["deliveries"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|d| d["tick"].as_u64() == Some(tick))
                {
                    delivered = view(&d["viewSnapshot"]);
                }
                let control = &row["control"];
                let result = human
                    .advance(
                        TickControl {
                            now: control["now"].as_u64().unwrap(),
                            live_owner_out: control["live_owner_out"].as_bool().unwrap(),
                            winner: control["winner"].as_bool().unwrap(),
                            level: control["level"].as_str().unwrap(),
                        },
                        Some(&delivered),
                        &batch(row),
                        |screen| {
                            assert_eq!(screen.tick, delivered.tick);
                            assert!(screen.units.iter().any(|u| u.owner == slot));
                            plans += 1;
                            Ok(vec![source_command.clone()])
                        },
                    )
                    .unwrap();
                actual_alerts.extend(result.new_alerts);
                answers.extend(result.answered);
                if let Some(i) = result.input {
                    let mut value = json!({"tick":i.tick,"origin":i.origin,"kind":match i.kind {InputKind::Camera=>"camera",InputKind::Select=>"select",InputKind::Key=>"key",InputKind::Click=>"click"},"x":i.at.x,"z":i.at.z,"camera":{"x":i.camera.x,"z":i.camera.z}});
                    if let Some(ids) = i.ids {
                        value["ids"] = json!(ids);
                    }
                    if let Some(command) = i.command {
                        value["cmd"] = command;
                    }
                    actual_inputs.push(value);
                }
            }
            assert_eq!(plans, 1);
            let mut expected_inputs = case["inputs"].clone();
            for row in expected_inputs.as_array_mut().unwrap() {
                row.as_object_mut().unwrap().remove("accepted");
            }
            assert!(exact(&json!(actual_inputs), &expected_inputs), "{}", case);
            assert_eq!(
                actual_alerts.len(),
                case["alerts"].as_array().unwrap().len()
            );
            for (a, expected) in actual_alerts.iter().zip(case["alerts"].as_array().unwrap()) {
                assert_eq!(a.tick, expected["tick"].as_u64().unwrap());
                assert_eq!(a.due, expected["due"].as_u64().unwrap());
                assert_eq!(a.victim, expected["id"].as_u64().unwrap() as u32);
                assert_eq!(a.at, point(expected));
                assert_eq!(a.on_screen, expected["onScreen"].as_bool().unwrap());
                assert_eq!(
                    answers.iter().find(|r| r.id == a.id).map(|r| r.tick),
                    expected["answered"].as_u64()
                );
            }
            let s = human.snapshot().unwrap();
            assert_eq!(s.camera, point(&case["final"]["state"]["camera"]));
            assert_eq!(
                s.next_input,
                case["final"]["state"]["nextInput"].as_u64().unwrap()
            );
            assert_eq!(
                s.recent.iter().copied().collect::<Vec<_>>(),
                case["final"]["state"]["recent"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_u64().unwrap())
                    .collect::<Vec<_>>()
            );
            assert_eq!(
                human.wants_view(case["final"]["tick"].as_u64().unwrap()),
                case["timeline"].as_array().unwrap().last().unwrap()["needsView"]
                    .as_bool()
                    .unwrap()
            );
        }
    }
}

#[test]
fn malformed_alert_and_live_stop_preserve_dormant_runtime() {
    let mut state = HumanState::new(1, 0, Point { x: 9.0, z: 9.0 });
    let before = state.clone();
    let malformed = OwnerHurtBatch {
        generation: 0,
        through_ordinal: 1,
        events: vec![Hurt {
            ordinal: 0,
            id: 1,
            at: Point {
                x: 9.0,
                z: f64::NAN,
            },
        }],
    };
    let control = TickControl {
        now: 1,
        live_owner_out: false,
        winner: false,
        level: "normal",
    };
    assert_eq!(
        state.advance::<(), _>(control, None, &malformed, |_| Err(Error::Planner(
            "unused".into()
        ))),
        Err(Error::InvalidBatch)
    );
    assert_eq!(state, before);
    assert!(
        state
            .advance::<(), _>(
                TickControl {
                    live_owner_out: true,
                    ..control
                },
                None,
                &malformed,
                |_| panic!("stopped")
            )
            .unwrap()
            .input
            .is_none()
    );
    assert_eq!(state, before);
}
