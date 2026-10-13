use serde_json::{Value, json};
use ww2_core::{
    legacy_scenario::{LegacyAction, LegacyError, LegacyScenario, PointOwner},
    map::{AuthoritativeMap, MapError},
};

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/legacy-scenarios.json")).unwrap()
}
fn points(value: &Value) -> Vec<PointOwner<'_>> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|p| PointOwner {
            id: p["id"].as_str().unwrap(),
            owner: p["owner"].as_i64().unwrap() as i32,
        })
        .collect()
}
// serde_json distinguishes integers and floats; the reference uses IEEE 754 for both.
fn equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => {
            a.as_f64().unwrap().to_bits() == b.as_f64().unwrap().to_bits()
        }
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| equal(a, b))
        }
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len()
                && a.iter()
                    .all(|(key, a)| b.get(key).is_some_and(|b| equal(a, b)))
        }
        _ => a == b,
    }
}
#[test]
fn legacy_setup_and_steps_match_original_scenario_engine() {
    let fixture = fixture();
    assert_eq!(
        fixture["reference"],
        "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
    );
    assert_eq!(fixture["node"], "v24.21.0");
    assert_eq!(fixture["arch"], "x64");
    let (mut authored, mut valid, mut invalid, mut steps) = (0, 0, 0, 0);
    for case in fixture["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let map = AuthoritativeMap::from_json(&serde_json::to_vec(&case["map"]).unwrap());
        if !case["error"].is_null() {
            assert!(
                matches!(map, Err(MapError::Invalid { .. })),
                "{name}: {map:?}"
            );
            invalid += 1;
            continue;
        }
        valid += 1;
        authored += usize::from(name.starts_with("authored:"));
        let map = map.unwrap();
        let mut owners = &case["points"];
        let mut scenario = LegacyScenario::from_map(&map, &points(owners));
        let initial = scenario
            .as_ref()
            .map_or(Value::Null, LegacyScenario::state_json);
        assert!(
            equal(&initial, &case["initial"]),
            "{name} initial\nactual:{initial}\nexpected:{}",
            case["initial"]
        );
        for step in case["steps"].as_array().unwrap() {
            steps += 1;
            if !step["points"].is_null() {
                owners = &step["points"];
            }
            let events = scenario.as_mut().map_or_else(Vec::new, |s| {
                s.step(step["tick"].as_u64().unwrap(), &points(owners))
                    .unwrap()
            });
            let actual = scenario
                .as_ref()
                .map_or(Value::Null, LegacyScenario::state_json);
            assert!(
                equal(&actual, &step["expected"]),
                "{name} tick {}\nactual:{actual}\nexpected:{}",
                step["tick"],
                step["expected"]
            );
            assert!(
                equal(
                    &json!(events.iter().map(LegacyAction::to_json).collect::<Vec<_>>()),
                    &step["events"]
                ),
                "{name} events at {}",
                step["tick"]
            );
            if let Some(scenario) = &mut scenario {
                // Every source-produced snapshot must be accepted by the typed restore API.
                scenario.restore_state(scenario.state().clone()).unwrap();
            }
        }
    }
    assert_eq!((authored, valid, invalid, steps), (5, 14, 8, 100));
}

#[test]
fn restore_replays_ordered_actions_and_rejects_bad_state_atomically() {
    let fixture = fixture();
    let case = fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "valid:zero-fractional-and-simultaneous")
        .unwrap();
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&case["map"]).unwrap()).unwrap();
    let owners = points(&case["points"]);
    let mut scenario = LegacyScenario::from_map(&map, &owners).unwrap();
    assert!(scenario.state().triggers.is_empty());
    scenario.step(0, &owners).unwrap();
    let snapshot = scenario.state().clone();
    let expected = scenario.step(20, &owners).unwrap();
    scenario.restore_state(snapshot.clone()).unwrap();
    assert_eq!(scenario.step(20, &owners).unwrap(), expected);
    let state = scenario.state().clone();
    assert_eq!(
        scenario.step(u64::MAX, &owners),
        Err(LegacyError::TickOutOfRange)
    );
    assert_eq!(scenario.state(), &state);
    let mut invalid = Vec::new();
    let mut s = state.clone();
    s.next_execution = u64::MAX;
    invalid.push(s);
    let mut s = state.clone();
    s.triggers.values_mut().next().unwrap().count = 2;
    invalid.push(s);
    let mut s = state.clone();
    s.triggers.values_mut().next().unwrap().next = f64::NAN;
    invalid.push(s);
    let mut s = state.clone();
    s.triggers.values_mut().next().unwrap().execution = Some("scenario:01".into());
    invalid.push(s);
    let mut s = state.clone();
    s.triggers
        .insert("unknown@match".into(), Default::default());
    invalid.push(s);
    let mut s = state.clone();
    s.owners.clear();
    invalid.push(s);
    let mut s = state.clone();
    s.captured.insert("unknown:1".into());
    invalid.push(s);
    for invalid in invalid {
        assert_eq!(
            scenario.restore_state(invalid),
            Err(LegacyError::InvalidState)
        );
        assert_eq!(scenario.state(), &state);
    }
    scenario.restore_state(snapshot).unwrap();
}
