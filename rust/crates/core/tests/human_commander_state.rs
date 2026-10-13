use serde_json::{Value, json};
use ww2_core as core_api;
use core_api::{
    game::Point,
    human_commander::{Hurt, OwnerHurtBatch, TickControl},
    human_commander_state::{Error, HumanState},
};
#[path = "support/human_commander_facts.rs"]
mod facts;
use facts::{point, view, batch, exact, input_receipt};

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
