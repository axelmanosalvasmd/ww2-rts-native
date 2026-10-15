//! Comparison-only representation mapping for the explicitly validated R1 profile.
use super::original;
use serde_json::{Value, json};
use std::collections::BTreeMap;

const FIELDS: [&str; 2] = ["fuel", "mun"];
const PIN: &str = "e4e579a838aabae3b1d9dadbc17a0a87147bc52d";

fn profile() -> Value {
    json!({"mode":"conquest","army":"standard","weather":"clear","shuffle":true,"tech":true,"supply":true,"livingWeather":true,"logistics":false,"sandbox":false,"weatherSeed":null})
}
fn zero() -> Value {
    json!({"bits":"0000000000000000","value":0.0})
}

struct R1Profile;
impl R1Profile {
    fn validate(trace: &Value) -> Result<Self, String> {
        if trace["profile"] != profile()
            || trace["format"] != "r1-whole-match-differential-v1"
            || trace["reference"] != PIN
            || trace["requestedTicks"] != 600
            || trace["checkpointEvery"] != 10
        {
            return Err("currency comparison requires the unchanged bounded R1 profile".into());
        }
        Ok(Self)
    }
}

// Each side independently receives this same function. Present values are never
// replaced, including negative zero, malformed wrappers, null and nonfinite values.
fn canonical_snapshot(snapshot: &Value, _: &R1Profile) -> (Value, Vec<String>) {
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
                for field in FIELDS {
                    let absent = object
                        .get(field)
                        .is_none_or(|v| *v == json!({"absent":true}));
                    if absent {
                        object.insert(field.into(), zero());
                        mapped.push(format!("$.players[{slot}].{field}"));
                    }
                }
            }
        }
    }
    (result, mapped)
}

pub(super) fn compare_frames(trace: &Value, native: &Value) -> Result<Value, String> {
    let profile = R1Profile::validate(trace)?;
    let source = trace["frames"]
        .as_array()
        .ok_or("missing raw Source frames")?;
    let native = native.as_array().ok_or("missing raw native frames")?;
    if source.len() != 601 || native.len() != 601 {
        return Err("canonical comparison requires all 601 raw frames".into());
    }
    let mut first = None;
    let mut matched = 0;
    let mut ticks_matched = 0;
    let mut source_mapped = BTreeMap::<String, usize>::new();
    let mut native_mapped = BTreeMap::<String, usize>::new();
    for (tick, (source, native)) in source.iter().zip(native).enumerate() {
        if source["tick"].as_u64() != Some(tick as u64)
            || native["tick"].as_u64() != Some(tick as u64)
        {
            return Err("raw frames are out of order".into());
        }
        let (source, source_fields) = canonical_snapshot(source, &profile);
        let (native, native_fields) = canonical_snapshot(native, &profile);
        for field in source_fields {
            *source_mapped.entry(field).or_default() += 1;
        }
        for field in native_fields {
            *native_mapped.entry(field).or_default() += 1;
        }
        if let Some(delta) = original::compare_projection_for_test(&source, &native) {
            if first.is_none() {
                first = Some(json!({"tick":tick,"difference":delta}));
            }
        } else {
            if tick % 10 == 0 {
                matched += 1;
            }
            if tick > 0 {
                ticks_matched += 1;
            }
        }
    }
    Ok(json!({
        "format":"r1-currency-comparison-v1",
        "whitelist":["$.players[0].fuel","$.players[0].mun","$.players[1].fuel","$.players[1].mun"],
        "canonicalDefault":zero(),
        "mapping":"only missing keys or exact {absent:true} markers, independently on each side; every present value retained",
        "sourceMappedFields":source_mapped,"nativeMappedFields":native_mapped,
        "checkpointsCompared":61,"checkpointsMatched":matched,"actualTicksMatched":ticks_matched,
        "differentialVerdict":if first.is_none(){"MATCH"}else{"MISMATCH"},"firstMismatch":first,
        "rowStatusChanged":false
    }))
}

#[test]
fn currency_defaults_are_symmetric_and_preserve_real_differences() {
    let context = json!({"format":"r1-whole-match-differential-v1","reference":PIN,"requestedTicks":600,"checkpointEvery":10,"profile":profile()});
    let validated = R1Profile::validate(&context).unwrap();
    let number = |bits: &str, value: Value| json!({"bits":bits,"value":value});
    let frame = |field: &str, value: Option<Value>| {
        let mut frame = json!({"players":[{"slot":0,"team":0,"fuel":zero(),"mun":zero()},{"slot":1,"team":1,"fuel":zero(),"mun":zero()}],"tick":0});
        let object = frame["players"][0].as_object_mut().unwrap();
        if let Some(value) = value {
            object.insert(field.into(), value);
        } else {
            object.remove(field);
        }
        frame
    };
    let equal = |a: &Value, b: &Value| {
        let (a, _) = canonical_snapshot(a, &validated);
        let (b, _) = canonical_snapshot(b, &validated);
        original::compare_projection_for_test(&a, &b).is_none()
    };
    for field in FIELDS {
        let positive = frame(field, Some(zero()));
        let one = frame(field, Some(number("3ff0000000000000", json!(1.0))));
        assert!(!equal(&positive, &one));
        assert!(!equal(&one, &positive));
        assert_eq!(canonical_snapshot(&positive, &validated).0, positive);
        for absent in [Some(json!({"absent":true})), None] {
            let absent = frame(field, absent);
            let untouched = absent.clone();
            assert!(original::compare_projection_for_test(&absent, &positive).is_some());
            assert!(equal(&absent, &positive));
            assert!(equal(&positive, &absent));
            assert!(!equal(&absent, &one));
            assert!(!equal(&one, &absent));
            assert_eq!(absent, untouched);
            for nondefault in [
                number("8000000000000000", json!(-0.0)),
                number("7ff0000000000000", json!("Infinity")),
                number("7ff8000000000000", json!("NaN")),
                Value::Null,
                json!(false),
                json!([]),
                json!({}),
                json!("0"),
                json!(0),
                json!({"absent":false}),
                json!({"absent":true,"extra":1}),
            ] {
                let present = frame(field, Some(nondefault));
                assert_eq!(canonical_snapshot(&present, &validated).0, present);
                assert!(!equal(&absent, &present));
                assert!(!equal(&present, &absent));
            }
        }
        let negative = frame(field, Some(number("8000000000000000", json!(-0.0))));
        assert!(!equal(&negative, &positive));
        assert!(!equal(&positive, &negative));
    }
    for field in ["mp", "fuelInc", "unknown"] {
        for absent in [Some(json!({"absent":true})), None] {
            let absent = frame(field, absent);
            let present = frame(field, Some(zero()));
            assert!(!equal(&absent, &present));
            assert!(!equal(&present, &absent));
        }
    }
    let a = json!({"points":[{"fuel":{"absent":true}}]});
    let b = json!({"points":[{"fuel":zero()}]});
    assert!(!equal(&a, &b));
    assert!(!equal(&b, &a));
    let missing_players = json!({"tick":0});
    let null_players = json!({"tick":0,"players":null});
    assert!(!equal(&missing_players, &null_players));
    assert!(!equal(&null_players, &missing_players));
    let absent_both = json!({"players":[{"slot":0,"team":0},{"slot":1,"team":1}],"tick":0});
    let (all_defaults, mapped) = canonical_snapshot(&absent_both, &validated);
    assert_eq!(
        mapped,
        vec![
            "$.players[0].fuel",
            "$.players[0].mun",
            "$.players[1].fuel",
            "$.players[1].mun"
        ]
    );
    assert_eq!(all_defaults, frame("fuel", Some(zero())));
    for (key, value) in [
        ("logistics", json!(true)),
        ("weatherSeed", json!(0)),
        ("mode", json!("classic")),
        ("army", json!("large")),
    ] {
        let mut changed = context.clone();
        changed["profile"][key] = value;
        assert!(R1Profile::validate(&changed).is_err());
    }
}
