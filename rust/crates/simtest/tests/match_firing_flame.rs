use crate::shared::firing as common;
use serde_json::{Value, json};
use std::io::Read;
use ww2_core::{economy::Mode, random::StreamId};

const PIN: &str = "e4e579a838aabae3b1d9dadbc17a0a87147bc52d";

fn inputs() -> Value {
    serde_json::from_str(include_str!(
        "../../../../harness/match-firing-flame-inputs.json"
    ))
    .unwrap()
}

fn fixture() -> Value {
    // Source owns these observations. Never create a fallback oracle in the consumer.
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/match-firing-flame.json.gz");
    let bytes = std::fs::read(path).expect(
        "authentic flame fixture absent: Root must grant the pinned Source recorder before replay",
    );
    let mut text = String::new();
    flate2::read::GzDecoder::new(bytes.as_slice())
        .read_to_string(&mut text)
        .unwrap();
    serde_json::from_str(&text).unwrap()
}

fn replay(group: &str) -> Vec<(String, Value)> {
    let spec = inputs();
    let oracle = fixture();
    assert_eq!(spec["reference"], PIN);
    assert_eq!(oracle["reference"], PIN);
    assert_eq!(oracle["seed"], 701);
    assert_eq!(oracle["node"], "v24.21.0");
    assert_eq!(oracle["arch"], "x64");
    assert_eq!(oracle["map"], spec["map"]);
    let authored = spec["cases"].as_array().unwrap();
    let recorded = oracle["cases"].as_array().unwrap();
    assert_eq!(authored.len(), 16);
    assert_eq!(recorded.len(), authored.len());
    let mut observations = Vec::new();
    for (input, output) in authored.iter().zip(recorded) {
        assert_eq!(output["input"], *input);
        assert_eq!(output["name"], input["name"]);
        if input["group"] != group {
            continue;
        }
        let name = input["name"].as_str().unwrap();
        let mut setup = input.clone();
        setup["map"] = spec["map"].clone();
        let steps = output["steps"].as_array().unwrap();
        assert_eq!(steps.len() as u64, input["volleys"].as_u64().unwrap());
        for mode in [Mode::Default, Mode::Conquest] {
            let mut m = common::make_with_mode(&setup, mode);
            assert!(m.world.game.mode.is_none());
            for v in input["input"].as_array().unwrap() {
                if let Some(cell) = v["garrison"].as_u64() {
                    assert_eq!(m.world.game.chars[cell as usize], b'B');
                    m.world
                        .game
                        .units
                        .get_mut(&(v["id"].as_u64().unwrap() as u32))
                        .unwrap()
                        .garrison = Some(cell as usize);
                }
            }
            let order = m.world.game.units.keys().copied().collect::<Vec<_>>();
            let mut last = Value::Null;
            for (index, step) in steps.iter().enumerate() {
                let tick = index as i64 + 1;
                assert_eq!(step["tick"], tick);
                m.world.game.tick = tick;
                // Readiness pulses are authored inputs, never copied from Source observations.
                if group == "launch" {
                    m.firing.initialize(1).cooldown = 0.0;
                }
                m.begin_firing_tick(&order).unwrap();
                m.advance_firing_timers(1, 0.05).unwrap();
                m.apply_unit_firing(1, false, 0.05).unwrap();
                last = common::snapshot(&mut m);
                common::compare(
                    &last,
                    &step["expected"],
                    &format!("{name} {mode:?} tick {tick}"),
                );
                let state = m
                    .start
                    .random
                    .snapshot()
                    .streams
                    .into_iter()
                    .find(|(id, _)| *id == StreamId::Combat)
                    .map(|(_, state)| format!("{state:016x}"));
                assert_eq!(
                    state.as_deref(),
                    step["random"].as_str(),
                    "{name} tick {tick}"
                );
                if group == "launch" {
                    let rounds = if input["shooter"] == "flamer" { 1 } else { 5 };
                    let launched = (index + 1) * rounds;
                    assert_eq!(m.firing.flights.len(), launched, "{name} tick {tick}");
                    let draws = step["draws"].as_u64().unwrap() as usize;
                    assert!(
                        (launched..=3 * launched).contains(&draws),
                        "{name} tick {tick}"
                    );
                } else {
                    assert!(m.firing.flights.is_empty());
                    assert_eq!(step["draws"], 0);
                }
            }
            observations.push((format!("{name}-{mode:?}"), last));
        }
    }
    observations
}

fn named<'a>(observations: &'a [(String, Value)], name: &str, mode: Mode) -> &'a Value {
    &observations
        .iter()
        .find(|(key, _)| *key == format!("{name}-{mode:?}"))
        .unwrap()
        .1
}

fn velocities(snapshot: &Value) -> Value {
    json!(
        snapshot["flights"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["velocity"].clone())
            .collect::<Vec<_>>()
    )
}

#[test]
fn cmb61_flame_launch_accuracy_ignores_trench_and_garrison() {
    let observations = replay("launch");
    assert_eq!(observations.len(), 6 * 2);
    for mode in [Mode::Default, Mode::Conquest] {
        let flame = velocities(named(&observations, "launch-flamer-open", mode));
        let rifle = velocities(named(&observations, "launch-rifle-open", mode));
        for cover in ["trench", "garrison"] {
            assert_eq!(
                flame,
                velocities(named(&observations, &format!("launch-flamer-{cover}"), mode))
            );
            assert_ne!(
                rifle,
                velocities(named(&observations, &format!("launch-rifle-{cover}"), mode))
            );
        }
    }
}

#[test]
fn cmb61_flame_target_value_has_decisive_one_point_five_preference() {
    let observations = replay("choice");
    assert_eq!(observations.len(), 10 * 2);
    for (name, snapshot) in &observations {
        let preferred = name.starts_with("choice-flamer-")
            && !name.starts_with("choice-flamer-open-")
            && name.contains("-below-");
        assert_eq!(snapshot["units"][0][1], if preferred { 3 } else { 2 }, "{name}");
        assert!(snapshot["flights"].as_array().unwrap().is_empty());
    }
}
