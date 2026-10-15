//! Original constructor/command replay for the private owner production rows.
use super::*;
use crate::{
    economy::Faction,
    economy_state::ArmySize,
    map::AuthoritativeMap,
    match_commands::Command,
    match_runtime::{MatchOptions, PlayerConfig},
    random::MatchRandom,
    weather::WeatherChoice,
};

fn make(input: &Value) -> Match {
    let mut map = input["map"].clone();
    let w = map["w"].as_u64().unwrap() as usize;
    let h = map["h"].as_u64().unwrap() as usize;
    let row = map["fill"].as_str().unwrap().repeat(w);
    map.as_object_mut().unwrap().remove("fill");
    map["rows"] = json!(vec![row; h]);
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&map).unwrap()).unwrap();
    let options = MatchOptions {
        seed: 701,
        mode: if input["map"]["world"].is_null() { Mode::Conquest } else { Mode::World },
        players: vec![
            PlayerConfig { name: "A".into(), team: 0, faction: Faction::Usa },
            PlayerConfig { name: "B".into(), team: 1, faction: Faction::Germany },
        ],
        shuffle: false,
        army: ArmySize::Standard,
        weather: WeatherChoice::Map,
        weather_seed: Some(217),
        living_weather: false,
        map_key: None,
        tech: true,
        supply: true,
        logistics: false,
        sandbox: false,
    };
    let mut state = if options.mode == Mode::World {
        Match::new_world(&map, options, MatchRandom::new(701))
    } else {
        Match::new(&map, options)
    }.unwrap();
    for p in &mut state.start.economy.players {
        p.mp = 5000.125;
        p.mun = 500.5;
        p.fuel = 500.25;
    }
    assert_eq!(json!(state.world.game.units.keys().collect::<Vec<_>>()), input["initialIds"]);
    state
}

// JSON records integral Source numbers without a decimal suffix. Compare their
// numeric values by f64 bits and keep object keys and array order exact.
fn equal(actual: &Value, expected: &Value) {
    match (actual, expected) {
        (Value::Number(a), Value::Number(b)) => {
            assert_eq!(a.as_f64().unwrap().to_bits(), b.as_f64().unwrap().to_bits());
        }
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len());
            for (a, b) in a.iter().zip(b) { equal(a, b); }
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(a.keys().collect::<Vec<_>>(), b.keys().collect::<Vec<_>>());
            for (key, value) in a { equal(value, &b[key]); }
        }
        _ => assert_eq!(actual, expected),
    }
}
fn check(state: &Match, recipients: &Value) {
    for slot in 0..state.world.game.players.len() {
        let rows = capture(state, slot).unwrap();
        let actual = Value::Object(rows.clone());
        equal(&actual, &recipients[slot]["cached"]);
        equal(&actual, &recipients[slot]["uncached"]);
        for name in ["queues", "productionJobs"] {
            for row in rows[name].as_array().unwrap() {
                let id = row[0].as_u64().unwrap() as u32;
                assert_eq!(state.world.game.units[&id].owner, slot);
            }
        }
        if state.options.mode == Mode::Conquest {
            let cache = SnapshotCache::new(state).unwrap();
            let recipient = cache.recipient(slot).unwrap();
            for name in ["queues", "productionJobs"] {
                equal(&recipient.fields[name], &rows[name]);
            }
        } else {
            assert!(matches!(SnapshotCache::new(state), Err(SnapshotError::UnsupportedMode)));
        }
    }
    assert_eq!(capture(state, state.world.game.players.len()), Err(SnapshotError::InvalidRecipient));
}
fn replay(world: bool) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../simtest/tests/fixtures/base-command-consumers.json");
    let fixture: Value = serde_json::from_str(&std::fs::read_to_string(path)
        .expect("original Source recording remains queued until Root grants its exact slot")).unwrap();
    assert_eq!(fixture["reference"], "e4e579a838aabae3b1d9dadbc17a0a87147bc52d");
    let cases = fixture["cases"].as_array().unwrap().iter()
        .filter(|case| !case["input"]["map"]["world"].is_null() == world).collect::<Vec<_>>();
    assert_eq!(cases.len(), if world { 3 } else { 2 });
    for case in cases {
        let mut state = make(&case["input"]);
        check(&state, &case["initialRecipients"]);
        for record in case["records"].as_array().unwrap() {
            match record["op"].as_str().unwrap() {
                "command" => {
                    let slot = record["slot"].as_u64().unwrap() as usize;
                    let command = Command::from_json(&record["cmd"]).unwrap();
                    let denial = state.execute_command(slot, &command, false).unwrap();
                    assert_eq!(json!(denial.map(|d| d.name())), record["result"]);
                }
                "train" => {
                    state.step_training(record["maker"].as_u64().unwrap() as u32,
                        record["dt"].as_f64().unwrap()).unwrap();
                }
                _ => panic!("unexpected Source action"),
            }
            check(&state, &record["recipients"]);
        }
    }
}
#[test]
fn conquest_actual_buy_and_rally_owner_rows_match_original_cached_and_uncached_source() {
    replay(false);
}
#[test]
fn world_actual_queue_birth_and_cancel_owner_rows_match_original_cached_and_uncached_source() {
    replay(true);
}
