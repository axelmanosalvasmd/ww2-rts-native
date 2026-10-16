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

// Native behavior checks use constructed detached facts, without a Source recorder.
fn intent_state(level: &str) -> HumanState {
    let mut state = HumanState::new(71, 0, Point { x: 10.0, z: 10.0 });
    state
        .advance::<(), _>(
            TickControl {
                level,
                ..control(0)
            },
            None,
            &empty_batch(),
            |_| panic!("initialization cannot plan"),
        )
        .unwrap();
    state
}

fn intent_unit(id: u32, owner: u32, x: f64, z: f64) -> HumanUnit {
    HumanUnit {
        motion: inputs::Unit {
            id,
            at: Point { x, z },
            world_goal: None,
            amove: None,
            target_id: 0,
            retreating: false,
        },
        owner,
        kind: "rifle".into(),
        hp: profile("rifle").unwrap().full_hp,
        air: false,
        build: false,
        dig: false,
        entrench: false,
        garrison: false,
    }
}

fn intent_view(tick: u64) -> HumanView<()> {
    HumanView {
        tick,
        spawn: Some(Point { x: 10.0, z: 10.0 }),
        units: vec![
            intent_unit(1, 0, 10.0, 10.0),
            intent_unit(2, 1, 30.0, 10.0),
            intent_unit(3, 1, 200.0, 10.0),
            intent_unit(4, 0, 300.0, 10.0),
        ],
        visible: vec![2, 3],
        sightings: vec![],
        packet: (),
    }
}

#[test]
fn intent_ai19_lazy_opening_and_handover_reset() {
    for level in ["easy", "normal", "hard"] {
        let mut state = HumanState::new(71, 0, Point { x: 10.0, z: 10.0 });
        assert!(state.snapshot().is_none());
        assert!(state.wants_view(0));
        state
            .advance::<(), _>(
                TickControl {
                    level,
                    ..control(7)
                },
                None,
                &empty_batch(),
                |_| panic!("opening must wait"),
            )
            .unwrap();
        let first = state.snapshot().unwrap();
        assert_eq!(first.camera, Point { x: 10.0, z: 10.0 });
        assert_eq!(first.timing.draws(), 2);
        assert!(first.look.as_ref().unwrap().at > 7);
        assert!(first.inputs.is_empty());
        state.reset_with_seed(72);
        assert!(state.snapshot().is_none());
        state
            .advance::<(), _>(
                TickControl {
                    level,
                    ..control(8)
                },
                None,
                &empty_batch(),
                |_| panic!("reset must open again"),
            )
            .unwrap();
        let reset = state.snapshot().unwrap();
        let mut timing = Timing::new(72, 0);
        let delay = timing.opening_delay(Skill::for_level(level)).unwrap();
        assert_eq!(reset.look.unwrap().at, 8 + delay);
        assert_eq!(reset.timing, timing);
    }
}

#[test]
fn intent_ai20_camera_memory_expires_and_own_units_stay_available() {
    let mut state = intent_state("normal");
    let runtime = state.runtime.as_mut().unwrap();
    let mut view = intent_view(1);
    view.sightings = vec![
        crate::human_commander::HumanSighting {
            id: 2,
            payload: json!({}),
        },
        crate::human_commander::HumanSighting {
            id: 3,
            payload: json!({}),
        },
    ];
    runtime.remember(&view);
    assert_eq!(runtime.screen_view(&view).visible, vec![2]);
    assert_eq!(
        runtime
            .screen_view(&view)
            .units
            .iter()
            .map(|u| u.motion.id)
            .collect::<Vec<_>>(),
        vec![1, 2, 4]
    );
    assert_eq!(
        runtime
            .screen_view(&view)
            .sightings
            .iter()
            .map(|s| s.id)
            .collect::<Vec<_>>(),
        vec![2]
    );
    let mut edges = intent_state("normal").snapshot().unwrap();
    let mut boundary = intent_view(1);
    boundary.units[1].motion.at = Point { x: 65.0, z: 50.0 };
    boundary.units[2].motion.at = Point { x: 65.001, z: 50.0 };
    edges.remember(&boundary);
    assert_eq!(edges.screen_view(&boundary).visible, vec![2]);
    assert!(!edges.seen.contains_key(&3));
    runtime.camera = Point { x: 200.0, z: 10.0 };
    view.tick = 601;
    runtime.remember(&view);
    assert_eq!(runtime.screen_view(&view).visible, vec![2, 3]);
    view.tick = 602;
    runtime.remember(&view);
    assert_eq!(runtime.screen_view(&view).visible, vec![3]);
    assert!(
        runtime
            .screen_view(&view)
            .units
            .iter()
            .any(|u| u.motion.id == 4)
    );
}

#[test]
fn intent_ai21_alert_floor_area_throttle_and_urgent_resume() {
    let mut state = intent_state("hard");
    let runtime = state.runtime.as_mut().unwrap();
    let event = |ordinal, x| crate::human_commander::Hurt {
        ordinal,
        id: 1,
        at: Point { x, z: 10.0 },
    };
    let mut result = HumanAdvance::default();
    runtime
        .read_alerts(
            10,
            &OwnerHurtBatch {
                generation: 1,
                through_ordinal: 2,
                events: vec![event(0, 10.0), event(1, 30.0)],
            },
            &mut result,
        )
        .unwrap();
    assert_eq!(result.new_alerts.len(), 1);
    let alert = result.new_alerts[0].clone();
    assert!(alert.on_screen && alert.due >= 14);
    runtime.inputs.push_back(QueuedInput::Action {
        input: Input {
            kind: InputKind::Click,
            at: Point { x: 10.0, z: 10.0 },
            ids: None,
            minimap: None,
            command: Some(json!({"t":"cover","ids":[4]})),
        },
        origin: 3,
        causes: vec![],
    });
    runtime.schedule_alerts().unwrap();
    assert!(runtime.look.as_ref().unwrap().urgent);
    runtime.start_look(alert.due).unwrap();
    assert!(
        matches!(runtime.inputs.front(), Some(QueuedInput::Plan { resume, .. })
        if resume.len() == 1 && resume[0].origin == 3 && resume[0].command["ids"][0] == 4)
    );
    runtime
        .read_alerts(
            410,
            &OwnerHurtBatch {
                generation: 2,
                through_ordinal: 1,
                events: vec![event(0, 10.0)],
            },
            &mut result,
        )
        .unwrap();
    assert_eq!(result.new_alerts.len(), 2);
    let mut noticed = Vec::new();
    for level in ["easy", "hard"] {
        let mut offscreen = intent_state(level).snapshot().unwrap();
        offscreen.inputs.push_back(QueuedInput::Action {
            input: Input {
                kind: InputKind::Key,
                at: offscreen.camera,
                ids: None,
                minimap: None,
                command: Some(json!({"t":"cover","ids":[4]})),
            },
            origin: 3,
            causes: vec![],
        });
        let mut alerts = HumanAdvance::default();
        offscreen
            .read_alerts(
                10,
                &OwnerHurtBatch {
                    generation: 1,
                    through_ordinal: 1,
                    events: vec![event(0, 200.0)],
                },
                &mut alerts,
            )
            .unwrap();
        let alert = &alerts.new_alerts[0];
        assert!(!alert.on_screen && alert.due >= 14);
        noticed.push(alert.due);
        offscreen.schedule_alerts().unwrap();
        offscreen.start_look(alert.due).unwrap();
        assert!(
            matches!(offscreen.inputs.front(), Some(QueuedInput::Action { input, .. })
            if input.kind == InputKind::Camera && input.at == Point { x: 200.0, z: 10.0 })
        );
        assert!(
            matches!(offscreen.inputs.back(), Some(QueuedInput::Plan { resume, .. })
            if resume.len() == 1 && resume[0].origin == 3)
        );
    }
    assert!(
        noticed[1] < noticed[0],
        "Hard notices the same offscreen damage before Easy"
    );
    let mut earliest = intent_state("hard").snapshot().unwrap();
    let mut alerts = HumanAdvance::default();
    earliest
        .read_alerts(
            10,
            &OwnerHurtBatch {
                generation: 1,
                through_ordinal: 2,
                events: vec![event(0, 200.0), event(1, 10.0)],
            },
            &mut alerts,
        )
        .unwrap();
    assert_eq!(alerts.new_alerts.len(), 2);
    earliest.schedule_alerts().unwrap();
    assert_eq!(
        earliest.look.as_ref().unwrap().at,
        alerts
            .new_alerts
            .iter()
            .map(|alert| alert.due)
            .min()
            .unwrap()
    );
}

#[test]
fn intent_ai22_scan_prioritizes_unseen_dots_and_scales_with_input_load() {
    let state = intent_state("normal");
    let mut runtime = state.snapshot().unwrap();
    let mut view = intent_view(100);
    view.units.push(intent_unit(5, 0, 12.0, 10.0));
    let first = runtime.scan_look(0, 100, &view).unwrap();
    assert_eq!(first.target, Some(Point { x: 30.0, z: 10.0 }));
    runtime.seen.insert(2, 100);
    let second = runtime.scan_look(0, 100, &view).unwrap();
    assert_eq!(second.target, Some(Point { x: 200.0, z: 10.0 }));
    runtime.seen.insert(3, 100);
    let mut loaded = runtime.clone();
    loaded.recent = std::iter::repeat_n(99, loaded.skill.peak).collect();
    let quiet = runtime.scan_look(0, 100, &view).unwrap();
    let busy = loaded.scan_look(0, 100, &view).unwrap();
    assert_eq!(quiet.target, Some(Point { x: 11.0, z: 10.0 }));
    assert!((busy.at - 100).abs_diff(4 * (quiet.at - 100)) <= 2);
    assert_eq!(
        runtime.scan_look(0, 100, &view).unwrap().target,
        Some(Point { x: 300.0, z: 10.0 })
    );
    assert_eq!(runtime.scan_look(0, 100, &view).unwrap().target, view.spawn);
}

#[test]
fn intent_ai23_plan_waits_for_delivery_at_reached_tick() {
    let mut state = intent_state("normal");
    let runtime = state.runtime.as_mut().unwrap();
    runtime.look = None;
    runtime.inputs.push_back(QueuedInput::Plan {
        ready: None,
        causes: vec![],
        resume: vec![],
    });
    let calls = std::cell::Cell::new(0);
    let mut planner = |_: ScreenView<'_, ()>| {
        calls.set(calls.get() + 1);
        Ok(vec![])
    };
    let mut result = HumanAdvance::default();
    runtime
        .run_input(
            0,
            control(100),
            Some(&intent_view(99)),
            &mut planner,
            &mut result,
        )
        .unwrap();
    assert!(matches!(
        runtime.inputs.front(),
        Some(QueuedInput::Plan {
            ready: Some(100),
            ..
        })
    ));
    runtime
        .run_input(
            0,
            control(101),
            Some(&intent_view(99)),
            &mut planner,
            &mut result,
        )
        .unwrap();
    assert_eq!(calls.get(), 0);
    runtime
        .run_input(
            0,
            control(102),
            Some(&intent_view(100)),
            &mut planner,
            &mut result,
        )
        .unwrap();
    assert_eq!(calls.get(), 1);
}

#[test]
fn intent_ai24_fight_response_retreats_hurt_and_uses_only_idle_nearby_units() {
    let state = intent_state("normal");
    let mut runtime = state.snapshot().unwrap();
    let mut view = intent_view(1);
    runtime.remember(&view);
    let alert = Alert {
        id: 0,
        tick: 1,
        victim: 1,
        at: view.units[0].motion.at,
        on_screen: true,
        due: 5,
        answered: None,
    };
    view.units[0].hp = 1.0;
    assert_eq!(
        runtime
            .fight_response(0, "normal", &view, &alert, &[])
            .unwrap(),
        Some(json!({"t":"retreat","ids":[1]}))
    );
    view.units[0].hp = profile("rifle").unwrap().full_hp * 0.4;
    assert_eq!(
        runtime
            .fight_response(0, "hard", &view, &alert, &[])
            .unwrap(),
        Some(json!({"t":"retreat","ids":[1]}))
    );
    assert_eq!(
        runtime
            .fight_response(0, "normal", &view, &alert, &[])
            .unwrap(),
        Some(json!({"t":"attack","ids":[1],"target":2}))
    );
    view.units[0].hp = profile("rifle").unwrap().full_hp;
    assert_eq!(
        runtime
            .fight_response(0, "normal", &view, &alert, &[])
            .unwrap(),
        Some(json!({"t":"attack","ids":[1],"target":2}))
    );
    view.units[0].dig = true;
    assert_eq!(
        runtime
            .fight_response(0, "normal", &view, &alert, &[])
            .unwrap(),
        None
    );
    view.units[0].dig = false;
    assert_eq!(
        runtime
            .fight_response(
                0,
                "normal",
                &view,
                &alert,
                &[json!({"t":"cover","ids":[1]})]
            )
            .unwrap(),
        None
    );
}

#[test]
fn intent_ai27_peak_minute_and_locality_block_without_consuming_inputs() {
    for level in ["easy", "normal", "hard"] {
        let state = intent_state(level);
        let mut runtime = state.snapshot().unwrap();
        runtime.look = None;
        runtime.inputs.push_back(QueuedInput::Action {
            input: Input {
                kind: InputKind::Click,
                at: Point { x: 10.0, z: 10.0 },
                ids: None,
                minimap: None,
                command: Some(json!({"t":"amove","orders":[[1,10,10]]})),
            },
            origin: 0,
            causes: vec![],
        });
        let view = intent_view(1500);
        let mut planner = |_: ScreenView<'_, ()>| panic!("physical input must not plan");
        let mut out = HumanAdvance::default();
        runtime.recent = std::iter::repeat_n(1499, runtime.skill.peak).collect();
        runtime
            .run_input(
                0,
                TickControl {
                    level,
                    ..control(1500)
                },
                Some(&view),
                &mut planner,
                &mut out,
            )
            .unwrap();
        assert!(out.input.is_none() && runtime.inputs.len() == 1);
        runtime.recent = std::iter::repeat_n(1200, runtime.skill.minute).collect();
        runtime
            .run_input(
                0,
                TickControl {
                    level,
                    ..control(1500)
                },
                Some(&view),
                &mut planner,
                &mut out,
            )
            .unwrap();
        assert!(out.input.is_none() && runtime.inputs.len() == 1);
        runtime.recent.clear();
        runtime.last_place = Some(Point { x: 300.0, z: 10.0 });
        runtime.last_place_tick = Some(1496);
        runtime
            .run_input(
                0,
                TickControl {
                    level,
                    ..control(1500)
                },
                Some(&view),
                &mut planner,
                &mut out,
            )
            .unwrap();
        assert!(out.input.is_none() && runtime.inputs.len() == 1);
        runtime
            .run_input(
                0,
                TickControl {
                    level,
                    ..control(1501)
                },
                Some(&view),
                &mut planner,
                &mut out,
            )
            .unwrap();
        assert!(out.input.is_some() && runtime.inputs.is_empty());
        assert!(runtime.next_input > 1501);
        runtime.inputs.push_back(QueuedInput::Action {
            input: Input {
                kind: InputKind::Key,
                at: Point { x: 10.0, z: 10.0 },
                ids: None,
                minimap: None,
                command: None,
            },
            origin: 0,
            causes: vec![],
        });
        let mut same_tick = HumanAdvance::default();
        runtime
            .run_input(
                0,
                TickControl {
                    level,
                    ..control(1501)
                },
                Some(&view),
                &mut planner,
                &mut same_tick,
            )
            .unwrap();
        assert!(same_tick.input.is_none() && runtime.inputs.len() == 1);
    }
}

#[test]
fn intent_ai17_scripted_stagger_and_four_tick_view_window() {
    use crate::commander_ai::Level;
    for level in [Level::Easy, Level::Normal, Level::Hard] {
        for slot in 0..6 {
            for tick in 0..400 {
                let phase = (tick + slot as i64 * 13) % level.every();
                assert_eq!(level.due(tick, slot), phase == 0);
                assert_eq!(
                    level.wants_view(tick, slot),
                    (level.every() - phase) % level.every() < 4
                );
            }
        }
    }
    assert_eq!(Level::Normal.every(), 40);
}

#[test]
fn intent_ai19_detached_resets_survive_ordinary_ticks_and_recipient_snapshots() {
    use crate::{
        economy::{Faction, Mode},
        economy_state::ArmySize,
        map::AuthoritativeMap,
        match_runtime::{Match, MatchOptions, PlayerConfig},
        recipient_snapshot::DeliveryState,
        weather::WeatherChoice,
    };

    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name": "Detached commander lifecycle", "w": 64, "h": 64,
            "rows": vec![".".repeat(64); 64],
            "spawns": [{"x": 4, "y": 4}, {"x": 59, "y": 59}],
            "points": [{"x": 27, "y": 10}, {"x": 13, "y": 27}, {"x": 32, "y": 32}]
        }))
        .unwrap(),
    )
    .unwrap();
    let mut game = Match::new(
        &map,
        MatchOptions {
            seed: 71,
            mode: Mode::Conquest,
            players: vec![
                PlayerConfig {
                    name: "A".into(),
                    team: 0,
                    faction: Faction::Usa,
                },
                PlayerConfig {
                    name: "B".into(),
                    team: 1,
                    faction: Faction::try_from(1).unwrap(),
                },
            ],
            shuffle: false,
            army: ArmySize::Standard,
            weather: WeatherChoice::Map,
            weather_seed: None,
            living_weather: false,
            map_key: None,
            tech: false,
            supply: false,
            logistics: false,
            sandbox: false,
        },
    )
    .unwrap();
    let spawn = game.start.players[0].spawn;
    // The host owns these detached states. This test does not activate room AI.
    let mut commander = HumanState::new(71, 0, spawn);
    let mut other = HumanState::new(71, 1, game.start.players[1].spawn);
    let mut delivery = DeliveryState::new(&game).unwrap();
    assert!(commander.snapshot().is_none());
    assert_eq!(commander.camera(), None);
    for tick in 1..=8 {
        game.advance_tick().unwrap();
        assert_eq!(game.world.game.tick, tick);
        assert!(game.tick_state.fault().is_none());
        let simulation_rng = game.start.random.snapshot();
        let environment_seed = game.start.environment_seed;

        if tick == 3 {
            // A human takes control. Private attention is discarded immediately.
            commander.reset();
        }
        if tick == 5 {
            // The human leaves. The host records a fresh seed before resuming AI.
            commander.reset_with_seed(72);
            assert!(commander.snapshot().is_none());
        }
        if (3..5).contains(&tick) {
            assert!(commander.snapshot().is_none());
            assert_eq!(commander.camera(), None);
        } else {
            let advance = commander
                .advance::<(), _>(
                    game.human_commander_control(0, "normal").unwrap(),
                    None,
                    &empty_batch(),
                    |_| panic!("opening must wait before planning"),
                )
                .unwrap();
            assert_eq!(advance, HumanAdvance::default());
            let runtime = commander.snapshot().unwrap();
            assert_eq!(runtime.camera, spawn);
            assert!(runtime.inputs.is_empty());
            assert!(runtime.selection.is_empty());
            let mut expected = Timing::new(if tick < 5 { 71 } else { 72 }, 0);
            let delay = expected.opening_delay(Skill::for_level("normal")).unwrap();
            let opening = runtime.look.as_ref().unwrap().at;
            let initialized_at = if tick < 5 { 1 } else { 5 };
            assert_eq!(opening, initialized_at + delay);
            assert!(opening > tick as u64);
            assert_eq!(runtime.timing, expected);
            let before_other = commander.snapshot();
            other
                .advance::<(), _>(
                    game.human_commander_control(1, "normal").unwrap(),
                    None,
                    &empty_batch(),
                    |_| panic!("other seat opening must wait"),
                )
                .unwrap();
            assert_eq!(commander.snapshot(), before_other);
            assert_ne!(runtime.timing, other.snapshot().unwrap().timing);
        }
        assert_eq!(game.start.random.snapshot(), simulation_rng);
        assert_eq!(game.start.environment_seed, environment_seed);
        if tick % 2 == 0 {
            let frame = delivery
                .capture_with_lists(&mut game, &[false; 2], &[false; 2], &[])
                .unwrap();
            for slot in 0..2 {
                let packet = frame.recipient(slot).unwrap().into_json();
                assert_eq!(packet["tick"], tick);
                assert!(!packet["units"].as_array().unwrap().is_empty());
            }
        }
    }
}
