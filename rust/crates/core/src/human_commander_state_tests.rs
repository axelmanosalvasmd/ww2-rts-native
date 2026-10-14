//! Constructed original helper seams. These replay motor state, not Match actuators.
use super::*;
use crate::human_commander_test_facts as facts;

fn fixture() -> Value {
    serde_json::from_str(include_str!("../tests/fixtures/human-commander-seams.json")).unwrap()
}
fn empty_batch() -> OwnerHurtBatch {
    facts::batch(&json!({"generation":0,"through_ordinal":0,"events":[]}))
}
fn control(now: u64) -> TickControl<'static> {
    TickControl {
        now,
        live_owner_out: false,
        winner: false,
        level: "normal",
    }
}
fn selection(value: &Value) -> Vec<i64> {
    value
        .as_str()
        .unwrap()
        .split(',')
        .filter(|s| !s.is_empty())
        .map(|s| s.parse().unwrap())
        .collect()
}
fn alert_value(alert: &Alert) -> Value {
    json!({"tick":alert.tick,"id":alert.victim,"x":alert.at.x,"z":alert.at.z,"onScreen":alert.on_screen,"due":alert.due,"answered":alert.answered})
}
fn alert_id(state: &mut HumanRuntimeSnapshot, value: &Value) -> u64 {
    if let Some((&id, _)) = state
        .alerts
        .iter()
        .find(|(_, a)| facts::exact(&alert_value(a), value))
    {
        return id;
    }
    let id = state.next_alert;
    state.next_alert += 1;
    state.alerts.insert(
        id,
        Alert {
            id,
            tick: value["tick"].as_u64().unwrap(),
            victim: value["id"].as_u64().unwrap() as u32,
            at: facts::point(value),
            on_screen: value["onScreen"].as_bool().unwrap(),
            due: value["due"].as_u64().unwrap(),
            answered: value["answered"].as_u64(),
        },
    );
    id
}
fn causes(state: &mut HumanRuntimeSnapshot, value: &Value) -> Vec<u64> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .map(|v| alert_id(state, v))
        .collect()
}
fn queued(state: &mut HumanRuntimeSnapshot, value: &Value) -> QueuedInput {
    let causes = causes(state, &value["causes"]);
    if value["kind"] == "plan" {
        return QueuedInput::Plan {
            ready: value["ready"].as_u64(),
            causes,
            resume: value["resume"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|r| ResumedOrder {
                    command: r["cmd"].clone(),
                    origin: r["origin"].as_u64().unwrap(),
                })
                .collect(),
        };
    }
    QueuedInput::Action {
        input: Input {
            kind: match value["kind"].as_str().unwrap() {
                "camera" => InputKind::Camera,
                "select" => InputKind::Select,
                "key" => InputKind::Key,
                "click" => InputKind::Click,
                _ => panic!("source input kind"),
            },
            at: facts::point(value),
            ids: value["ids"]
                .as_array()
                .map(|rows| rows.iter().map(|n| n.as_i64().unwrap()).collect()),
            minimap: value["minimap"].as_bool(),
            command: value.get("cmd").cloned(),
        },
        origin: value["origin"].as_u64().unwrap(),
        causes,
    }
}
fn initialize(case: &Value) -> HumanState {
    let view = facts::view(&case["viewSnapshot"]);
    let mut human = HumanState::new(
        case["matchSeed"].as_i64().unwrap() as u32,
        case["slot"].as_u64().unwrap() as u32,
        view.spawn.unwrap(),
    );
    human
        .advance::<(), _>(control(view.tick), None, &empty_batch(), |_| {
            panic!("initialization must not plan")
        })
        .unwrap();
    let state = human.runtime.as_mut().unwrap();
    assert_eq!(
        state.timing.draws(),
        case["initialPrivateDraws"].as_u64().unwrap()
    );
    assert_eq!(
        state.look.as_ref().unwrap().at,
        case["stateOfResult"]["look"]["at"].as_u64().unwrap()
    );
    let overlay = &case["componentInitial"];
    state.camera = facts::point(&overlay["camera"]);
    state.selection = selection(&overlay["selection"]);
    state.next_input = overlay["nextInput"].as_u64().unwrap();
    state.recent = overlay["recent"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_u64().unwrap())
        .collect();
    state.last_place =
        (!overlay["lastPlace"].is_null()).then(|| facts::point(&overlay["lastPlace"]));
    state.last_place_tick = overlay["lastPlaceTick"].as_u64();
    state.scan = overlay["scan"].as_u64().unwrap();
    state.pending = causes(state, &overlay["pending"]);
    state.inputs = overlay["inputs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| queued(state, i))
        .collect();
    state.look = if overlay["look"].is_null() {
        None
    } else {
        let look = &overlay["look"];
        Some(Look {
            at: look["at"].as_u64().unwrap(),
            target: (!look["target"].is_null()).then(|| facts::point(&look["target"])),
            urgent: look["urgent"].as_bool().unwrap_or(false),
            causes: causes(state, &look["causes"]),
            resume: Vec::new(),
        })
    };
    human
}
fn queue_value(state: &HumanRuntimeSnapshot) -> Value {
    Value::Array(state.inputs.iter().map(|queued|match queued {
        QueuedInput::Plan {ready,causes,resume}=>{
            let mut row=json!({"kind":"plan","causes":causes.iter().map(|id|alert_value(&state.alerts[id])).collect::<Vec<_>>(),"resume":resume.iter().map(|r|json!({"cmd":r.command,"origin":r.origin})).collect::<Vec<_>>()});
            if let Some(ready)=ready {row["ready"]=json!(ready);}
            row
        }
        QueuedInput::Action {input,origin,causes}=>{
            let mut row=json!({"kind":match input.kind {InputKind::Camera=>"camera",InputKind::Select=>"select",InputKind::Key=>"key",InputKind::Click=>"click"},"x":input.at.x,"z":input.at.z,"origin":origin});
            if let Some(ids)=&input.ids {row["ids"]=json!(ids);}
            if let Some(minimap)=input.minimap {row["minimap"]=json!(minimap);}
            if let Some(command)=&input.command {row["cmd"]=command.clone();}
            if !causes.is_empty() {row["causes"]=json!(causes.iter().map(|id|alert_value(&state.alerts[id])).collect::<Vec<_>>());}
            row
        }
    }).collect())
}
fn motor_value(state: &HumanRuntimeSnapshot) -> Value {
    let selection = state
        .selection
        .iter()
        .map(i64::to_string)
        .collect::<Vec<_>>()
        .join(",");
    json!({"camera":{"x":state.camera.x,"z":state.camera.z},"selection":selection,"inputs":queue_value(state),"nextInput":state.next_input,
        "recent":state.recent,"lastPlace":state.last_place.map(|p|json!({"x":p.x,"z":p.z})),"lastPlaceTick":state.last_place_tick.map(|n|n as i64).unwrap_or(-1_000_000_000),
        "pending":state.pending.iter().map(|id|alert_value(&state.alerts[id])).collect::<Vec<_>>(),"look":state.look.as_ref().map(|l|json!({"at":l.at,"target":l.target.map(|p|json!({"x":p.x,"z":p.z})),"urgent":l.urgent,"causes":l.causes.iter().map(|id|alert_value(&state.alerts[id])).collect::<Vec<_>>()})),"scan":state.scan})
}
fn source_motor(state: &Value) -> Value {
    let keys = [
        "camera",
        "selection",
        "inputs",
        "nextInput",
        "recent",
        "lastPlace",
        "lastPlaceTick",
        "pending",
        "look",
        "scan",
    ];
    Value::Object(
        keys.into_iter()
            .map(|key| (key.into(), state[key].clone()))
            .collect(),
    )
}

#[test]
fn original_constructed_caps_locality_and_urgent_resume_preserve_motor_state() {
    let fixture = fixture();
    assert_eq!(
        fixture["reference"],
        "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
    );
    let mut contexts = 0;
    let mut calls = 0;
    for case in fixture["cases"].as_array().unwrap() {
        let mut human = initialize(case);
        let mut answer = None;
        for (call_index, call) in case["calls"].as_array().unwrap().iter().enumerate() {
            let now = call["now"].as_u64().unwrap();
            let view = facts::view(&call["viewSnapshot"]);
            // Dig comes from the original delivered paid-job projection. Only
            // Source receipts below claim wallet changes or actuator cancellation.
            if case["name"] == "urgent-look-resumes-untouched-paid-digger" && call_index == 0 {
                assert!(view.units.iter().find(|u| u.motion.id == 1).unwrap().dig);
            }
            let before_draws = human.snapshot().unwrap().timing.draws();
            let mut plans = 0;
            let advance = human
                .advance(control(now), Some(&view), &empty_batch(), |screen| {
                    assert_eq!(screen.tick, view.tick);
                    plans += 1;
                    Ok(call["plannedInputCommands"].as_array().unwrap().clone())
                })
                .unwrap();
            assert_eq!(
                plans,
                usize::from(!call["plannedInputCommands"].as_array().unwrap().is_empty())
            );
            let state = human.snapshot().unwrap();
            assert_eq!(
                state.timing.draws() - before_draws,
                call["privateDraws"].as_u64().unwrap()
            );
            assert!(matches!(call["privateDraws"].as_u64().unwrap(), 0 | 2));
            assert!(advance.new_alerts.is_empty());
            for update in &advance.answered {
                assert_eq!(update.id, 0);
                assert!(answer.replace(update.tick).is_none());
            }
            if !call["componentAlert"].is_null() {
                assert_eq!(answer, call["componentAlert"]["answered"].as_u64());
            }
            let actual = advance
                .input
                .as_ref()
                .map(facts::input_receipt)
                .into_iter()
                .collect::<Vec<_>>();
            let mut expected = call["callbacks"].clone();
            for row in expected.as_array_mut().unwrap() {
                row.as_object_mut().unwrap().remove("accepted");
            }
            assert!(
                facts::exact(&json!(actual), &expected),
                "{}:{now}: input receipt",
                case["name"]
            );
            // Full advance also remembers delivered facts and consumes the empty
            // event batch. These are outside the recorded private runInput seam.
            assert!(
                facts::exact(&motor_value(&state), &source_motor(&call["state"])),
                "{}:{now}: motor state",
                case["name"]
            );
            calls += 1;
        }
        if case["name"] == "urgent-look-resumes-untouched-paid-digger" {
            let setup = &case["setupCommands"][0];
            assert!(setup["denial"].is_null());
            assert_eq!(
                (
                    setup["walletBefore"].as_f64(),
                    setup["walletAfter"].as_f64()
                ),
                (Some(150.0), Some(120.0))
            );
            assert!(setup["dig"].is_object());
            let rows = case["calls"].as_array().unwrap();
            assert!(rows[0]["canonicalDigBefore"].is_object());
            assert_eq!(rows[0]["canonicalDigBefore"], rows[0]["canonicalDigAfter"]);
            assert!(rows.last().unwrap()["canonicalDigAfter"].is_null());
            assert_eq!(
                human.snapshot().unwrap().alerts.len(),
                0,
                "finished causes release their internal alert references"
            );
        }
        contexts += 1;
    }
    assert_eq!((contexts, calls), (5, 12));
}
#[test]
fn a_missing_delivered_view_cannot_consume_a_delayed_command() {
    // The detached host must retain its latest view. This validates that API
    // boundary rather than reproducing the source helper's Game fallback.
    let fixture = fixture();
    let case = fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "local-screen-inclusive-no-gap")
        .unwrap();
    let mut human = initialize(case);
    let before = human.clone();
    assert_eq!(
        human
            .advance::<(), _>(control(104), None, &empty_batch(), |_| panic!(
                "a physical input does not invoke the planner"
            ))
            .unwrap_err(),
        Error::InvalidView
    );
    assert_eq!(human, before);
}

#[test]
fn a_failed_due_plan_retains_urgent_causes_resume_origins_and_private_rng() {
    let fixture = fixture();
    let case = fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "urgent-look-resumes-untouched-paid-digger")
        .unwrap();
    let mut human = initialize(case);
    let before = human.clone();
    let view = facts::view(&case["calls"][0]["viewSnapshot"]);
    let mut callbacks = 0;
    let error = human
        .advance(control(100), Some(&view), &empty_batch(), |_| {
            callbacks += 1;
            Err(Error::Planner("recorded due-plan refusal".into()))
        })
        .unwrap_err();
    assert_eq!(error, Error::Planner("recorded due-plan refusal".into()));
    assert_eq!(callbacks, 1);
    assert_eq!(human, before);
}
