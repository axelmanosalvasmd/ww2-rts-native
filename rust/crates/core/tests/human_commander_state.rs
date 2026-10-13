use core_api::{
    game::Point,
    human_commander::{Hurt, OwnerHurtBatch, TickControl},
    human_commander_state::{Error, HumanState},
};
use serde_json::{Value, json};
use ww2_core as core_api;
#[path = "support/human_commander_facts.rs"]
mod facts;
use facts::{batch, exact, input_receipt, point, view};

#[test]
fn real_hurt_reset_reseeds_private_timing_without_reusing_subscriber_identity() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/human-commander-reset.json")).unwrap();
    let case = &fixture["cases"][0];
    let reset = &case["resets"][0];
    let restart = &reset["reinitialized"];
    let reset_tick = reset["tick"].as_u64().unwrap();
    let restart_tick = restart["tick"].as_u64().unwrap();
    let source_seed = case["sourceSeed"].as_i64().unwrap();
    let initialization = &case["initializations"][0];
    let initialization_seed = initialization["initializationSeed"].as_i64().unwrap();
    let mut human = HumanState::new(initialization_seed as u32, 0, point(&case["sourceSpawn"]));
    assert_eq!(source_seed as i32, -1_167_756_645);
    assert_eq!(initialization_seed as u32 as i32, -1_799_592_315);
    let mut delivered = view(&case["deliveries"][0]["viewSnapshot"]);
    let mut inputs = Vec::new();
    let mut alerts = Vec::new();
    let mut old_receipt = None;
    let mut plans = 0;
    for row in case["batches"].as_array().unwrap() {
        let tick = row["tick"].as_u64().unwrap();
        if tick == restart_tick {
            // Source initializes on the next active callback, after the intervening step.
            let seed = restart["initializationSeed"].as_i64().unwrap();
            assert_ne!(seed, reset["resetSeed"].as_i64().unwrap());
            human.reset_with_seed(seed as u32);
        }
        if let Some(record) = case["deliveries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|d| d["tick"].as_u64() == Some(tick))
        {
            delivered = view(&record["viewSnapshot"]);
        }
        let control = &row["control"];
        let result = human
            .advance(
                TickControl {
                    now: tick,
                    live_owner_out: control["live_owner_out"].as_bool().unwrap(),
                    winner: control["winner"].as_bool().unwrap(),
                    level: control["level"].as_str().unwrap(),
                },
                Some(&delivered),
                &batch(row),
                |screen| {
                    let plan = case["plans"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|p| p["tick"].as_u64() == Some(tick))
                        .expect("actual original raw planner collection");
                    assert_eq!(screen.tick, plan["viewTick"].as_u64().unwrap());
                    plans += 1;
                    Ok(plan["commands"].as_array().unwrap().clone())
                },
            )
            .unwrap();
        alerts.extend(result.new_alerts);
        for answer in result.answered {
            let alert = alerts.iter_mut().find(|a| a.id == answer.id).unwrap();
            alert.answered = Some(answer.tick);
        }
        if let Some(input) = result.input {
            inputs.push(input_receipt(&input));
        }
        if tick == reset_tick {
            assert_eq!(alerts.len(), 1);
            assert_eq!(alerts[0].answered, Some(reset_tick));
            old_receipt = Some(alerts[0].clone());
            assert_eq!(human.camera(), Some(point(&reset["before"]["camera"])));
            human.reset();
            assert_eq!(human.camera(), None);
            assert!(human.wants_view(tick));
        }
        if tick == initialization["tick"].as_u64().unwrap() {
            let state = human.snapshot().unwrap();
            assert_eq!(state.timing.draws(), 2);
            assert_eq!(
                state.look.unwrap().at,
                initialization["openingAt"].as_u64().unwrap()
            );
        }
        if tick == restart_tick {
            let state = human.snapshot().unwrap();
            assert_eq!(state.timing.draws(), 2);
            assert_eq!(state.look.unwrap().at, 141);
            assert_eq!(state.camera, point(&case["sourceSpawn"]));
            assert!(state.selection.is_empty());
            assert!(state.inputs.is_empty());
            assert!(state.recent.is_empty());
            assert!(state.areas.is_empty());
            assert_eq!(state.next_alert, 1);
        }
        if tick > reset_tick {
            assert_eq!(alerts[0], *old_receipt.as_ref().unwrap());
        }
    }
    assert_eq!(plans, case["plans"].as_array().unwrap().len());
    assert_eq!(alerts.len(), 2);
    assert_ne!(alerts[0].id, alerts[1].id);
    assert_eq!(case["distinctSubscriberObjects"], true);
    for (actual, expected) in alerts.iter().zip(case["alerts"].as_array().unwrap()) {
        assert_eq!(actual.tick, expected["tick"].as_u64().unwrap());
        assert_eq!(actual.due, expected["due"].as_u64().unwrap());
        assert_eq!(actual.answered, expected["answered"].as_u64());
        assert_eq!(actual.at, point(expected));
    }
    let mut expected = case["inputs"].clone();
    for input in expected.as_array_mut().unwrap() {
        input.as_object_mut().unwrap().remove("accepted");
    }
    assert!(exact(&json!(inputs), &expected));
    let state = human.snapshot().unwrap();
    assert_eq!(state.camera, point(&case["final"]["state"]["camera"]));
    assert_eq!(
        state.next_input,
        case["final"]["state"]["nextInput"].as_u64().unwrap()
    );
}

// Replay actual delivered facts and Source-collected policy output through this component.
// The callback boundary supplies recorded Source policy output; native admission stays pending.
#[test]
fn ordinary_human_state_matches_actual_source_opening_hurts_and_denied_input() {
    for bytes in [
        include_str!("fixtures/human-commander-runtime.json"),
        include_str!("fixtures/human-commander-alerts.json"),
        include_str!("fixtures/human-commander-denial.json"),
    ] {
        let f: Value = serde_json::from_str(bytes).unwrap();
        for case in f["cases"].as_array().unwrap() {
            let slot = case["slot"].as_u64().unwrap() as u32;
            let mut human = HumanState::new(
                case["initializationSeed"].as_i64().unwrap() as u32,
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
                    actual_inputs.push(input_receipt(&i));
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
            if case.get("lifecycleOverlays").is_some() {
                assert_eq!(case["inputs"][1]["accepted"], false);
                assert_eq!(case["final"]["away"], true);
                assert_eq!(case["final"]["out"], false);
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
