//! IF-11 source-recorded presentation values, exercised through the exported C functions.
use serde_json::{Value, json};
use std::{ffi::c_char, ptr};
use ww2_ffi::*;

type Query = unsafe extern "C" fn(*const c_char, usize, *mut c_char, usize, *mut usize) -> i32;

fn call(query: Query, input: &Value) -> Value {
    let input = serde_json::to_vec(input).unwrap();
    let mut len = 0;
    // SAFETY: input is readable, count writable, and output explicitly empty and disjoint.
    assert_eq!(
        unsafe {
            query(
                input.as_ptr().cast(),
                input.len(),
                ptr::null_mut(),
                0,
                &mut len,
            )
        },
        WW2_ERR_CAPACITY
    );
    let mut output = vec![0_u8; len];
    // SAFETY: disjoint input/output/count storage with the advertised lengths.
    assert_eq!(
        unsafe {
            query(
                input.as_ptr().cast(),
                input.len(),
                output.as_mut_ptr().cast(),
                output.len(),
                &mut len,
            )
        },
        WW2_OK
    );
    serde_json::from_slice(&output[..len]).unwrap()
}

fn exact(actual: &Value, expected: &Value) {
    if expected.is_number() {
        assert_eq!(actual.as_f64(), expected.as_f64());
    } else if let Some(object) = expected.as_object() {
        assert_eq!(actual.as_object().unwrap().len(), object.len());
        for (key, value) in object {
            exact(&actual[key], value);
        }
    } else {
        assert_eq!(actual, expected);
    }
}

#[test]
fn if11_all_unit_motion_queries_match_recorded_source_tables() {
    let source: Value = serde_json::from_str(include_str!(
        "../../core/tests/fixtures/startup-tables.json"
    ))
    .unwrap();
    assert_eq!(
        source["reference"],
        "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
    );
    let units = source["tables"]["UNITS"].as_object().unwrap();
    assert_eq!(units.len(), 47);
    for (name, definition) in units {
        let input = json!({"type":name});
        exact(
            &call(ww2_movement_profile, &input),
            &source["tables"]["MOVEMENT_PROFILES"][name],
        );
        let ground = ["infantry", "structure", "air", "naval"]
            .iter()
            .all(|key| !definition[key].as_bool().unwrap_or(false));
        assert_eq!(call(ww2_is_ground_vehicle, &input), ground, "{name}");
        assert_eq!(
            call(ww2_body_radius, &input).as_f64().unwrap().to_bits(),
            (definition["radius"].as_f64().unwrap_or(1.0) * 0.8).to_bits(),
            "{name}"
        );
    }
}

#[test]
fn if11_recorded_flights_match_source_bits_through_c_entry_points() {
    let fixture: Value =
        serde_json::from_str(include_str!("../../core/tests/fixtures/projectiles.json")).unwrap();
    assert_eq!(
        fixture["reference"],
        "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
    );
    let mut count = 0;
    for row in fixture["cases"].as_array().unwrap() {
        let query: Query = match row["op"].as_str().unwrap() {
            "flightAt" => ww2_flight_at,
            "velocityAt" => ww2_velocity_at,
            _ => continue,
        };
        let actual = call(
            query,
            &json!({"flight":row["args"][0],"time":row["args"][1]}),
        );
        for axis in ["x", "y", "z"] {
            let expected =
                u64::from_str_radix(row["expected"][axis].as_str().unwrap(), 16).unwrap();
            assert_eq!(actual[axis].as_f64().unwrap().to_bits(), expected, "{row}");
        }
        count += 1;
    }
    assert_eq!(count, 10);
}

#[test]
fn if11_invalid_queries_and_capacity_failures_leave_output_untouched() {
    let valid_flight = json!({"flight":{"x":1.0,"y":2.0,"z":3.0,"vx":4.0,"vy":5.0,
        "vz":6.0,"gravity":9.81,"stamp":2.0},"time":3.0});
    let cases: [(Query, Value); 5] = [
        (ww2_movement_profile, json!({"type":"tank"})),
        (ww2_is_ground_vehicle, json!({"type":"rifle"})),
        (ww2_body_radius, json!({"type":"lorry"})),
        (ww2_flight_at, valid_flight.clone()),
        (ww2_velocity_at, valid_flight.clone()),
    ];
    for (query, valid) in cases {
        let input = serde_json::to_vec(&valid).unwrap();
        let mut len = 0;
        let mut output = [0x5a_u8; 1024];
        // SAFETY: valid disjoint buffers, with the deliberately insufficient advertised capacity.
        unsafe {
            assert_eq!(
                query(
                    input.as_ptr().cast(),
                    input.len(),
                    output.as_mut_ptr().cast(),
                    1,
                    &mut len
                ),
                WW2_ERR_CAPACITY
            );
            assert!(len > 1);
            assert_eq!(output, [0x5a; 1024]);
            assert_eq!(
                query(
                    input.as_ptr().cast(),
                    input.len(),
                    ptr::null_mut(),
                    len,
                    &mut len
                ),
                WW2_ERR_NULL_POINTER
            );
        }
        let mut invalids = vec![json!({}), json!({"type":"unknown"}), json!({"type":false})];
        if valid.get("flight").is_some() {
            let mut missing = valid.clone();
            missing["flight"].as_object_mut().unwrap().remove("stamp");
            let mut null = valid.clone();
            null["flight"]["gravity"] = Value::Null;
            let mut overflow = valid.clone();
            overflow["flight"]["gravity"] = json!(1e308);
            overflow["time"] = json!(1e308);
            invalids.extend([missing, null, overflow]);
        }
        for invalid in invalids {
            let input = serde_json::to_vec(&invalid).unwrap();
            // SAFETY: valid disjoint input, output and count storage.
            assert_eq!(
                unsafe {
                    query(
                        input.as_ptr().cast(),
                        input.len(),
                        output.as_mut_ptr().cast(),
                        1024,
                        &mut len,
                    )
                },
                WW2_ERR_BAD_ARGUMENT,
                "{invalid}"
            );
            assert_eq!(len, 0);
            assert_eq!(output, [0x5a; 1024]);
        }
    }
    // A successful call after rejection establishes that stateless failures retain no poison.
    assert_eq!(call(ww2_velocity_at, &valid_flight)["x"], 4.0);
}
