//! Editor calls cross the same bounded JSON buffer interface as the C consumer.
use serde_json::{Value, json};
use std::ffi::c_char;
use ww2_ffi::*;

type Query = unsafe extern "C" fn(*const c_char, usize, *mut c_char, usize, *mut usize) -> i32;

fn call(query: Query, input: &Value) -> Value {
    let input = serde_json::to_vec(input).unwrap();
    let mut output = vec![0_i8; 128 * 1024];
    let mut written = 0;
    // SAFETY: both buffers and the output length are valid for the provided lengths.
    let status = unsafe {
        query(
            input.as_ptr().cast(),
            input.len(),
            output.as_mut_ptr(),
            output.len(),
            &mut written,
        )
    };
    let mut error = [0 as c_char; 512];
    let mut error_len = 0;
    // SAFETY: disjoint writable error buffer and count storage.
    let _ = unsafe { ww2_last_error(error.as_mut_ptr(), error.len(), &mut error_len) };
    let message = error[..error_len.min(error.len())]
        .iter()
        .map(|b| *b as u8)
        .collect::<Vec<_>>();
    assert_eq!(status, WW2_OK, "{}", String::from_utf8_lossy(&message));
    let bytes = output[..written]
        .iter()
        .map(|b| *b as u8)
        .collect::<Vec<_>>();
    serde_json::from_slice(&bytes).unwrap()
}

fn map() -> Value {
    json!({"name":"Editor test","w":32,"h":32,"rows":vec![".".repeat(32);32],
        "spawns":[{"x":4,"y":4},{"x":27,"y":27},{"x":4,"y":27,"assault":true}],
        "points":[{"x":16,"y":16}]})
}

#[test]
fn validation_and_mode_filter_preserve_editor_meaning() {
    let source = map();
    assert_eq!(call(ww2_authoring_validate_map, &source), Value::Null);
    let mut bad = source.clone();
    bad["name"] = json!(false);
    assert_eq!(
        call(ww2_authoring_validate_map, &bad),
        "name must be a string up to 40 chars"
    );
    assert_eq!(
        call(
            ww2_authoring_spawns_for,
            &json!({"map":source,"mode":"conquest"})
        ),
        json!([0, 1])
    );
    assert_eq!(
        call(
            ww2_authoring_spawns_for,
            &json!({"map":source,"mode":"assault"})
        ),
        json!([0, 1, 2])
    );
}

#[test]
fn validation_uses_source_order_and_static_house_metadata_needs_no_map() {
    let mut source = map();
    source["points"][0]["vp"] = json!(99);
    source["pointSequence"] = json!(-1);
    assert_eq!(
        call(ww2_authoring_validate_map, &source),
        "points must be on open ground, vp and mp 0-5"
    );
    source = map();
    source["worldVersion"] = json!(1);
    assert_eq!(
        call(ww2_authoring_validate_map, &source),
        "worldVersion must be 2"
    );
    source = map();
    source["worldVersion"] = json!(2);
    assert_eq!(
        call(ww2_authoring_validate_map, &source),
        "version 2 needs world layers"
    );
    let metadata = call(ww2_authoring_house_kinds, &json!({}));
    assert!(!metadata.as_array().unwrap().is_empty());
    assert!(metadata[0]["name"].is_string());
}

#[test]
fn migrations_retain_private_authored_metadata_and_are_idempotent() {
    let mut source = map();
    source["rows"][0] = json!(format!("N={}B", ".".repeat(28)));
    source["custom"] = json!({"secret":"author only"});
    let migrated = call(ww2_authoring_migrate_map, &source);
    assert_eq!(migrated["custom"], source["custom"]);
    assert!(
        migrated["layers"]["ground"][0]
            .as_str()
            .unwrap()
            .starts_with(".W")
    );
    assert!(
        migrated["layers"]["mines"][0]
            .as_str()
            .unwrap()
            .starts_with('N')
    );
    assert_eq!(call(ww2_authoring_migrate_map, &migrated), migrated);
    let mut version_as_double = migrated.clone();
    version_as_double["worldVersion"] = json!(2.0);
    version_as_double["layers"]["author_note"] = json!("preserve this field");
    assert_eq!(
        call(ww2_authoring_migrate_map, &version_as_double),
        version_as_double
    );
    let scenario = call(
        ww2_authoring_migrate_scenario,
        &json!({"triggers":[{"at":5,"say":"Hello"}]}),
    );
    assert_eq!(scenario["triggers"][0]["actions"][0]["text"]["en"], "Hello");
}

#[test]
fn preview_and_path_use_the_authored_map() {
    let source = map();
    let preview = call(
        ww2_authoring_create_preview,
        &json!({"map":source,"seed":19,"options":{"players":2,"mode":"conquest"}}),
    );
    assert_eq!(preview["players"].as_array().unwrap().len(), 2);
    assert!(!preview["units"].as_array().unwrap().is_empty());
    let route = call(
        ww2_authoring_find_path,
        &json!({"map":source,"from":{"x":9,"z":9},"to":{"x":33,"z":33}}),
    );
    assert!(!route.as_array().unwrap().is_empty());
    assert_eq!(
        route.as_array().unwrap().last().unwrap(),
        &json!({"x":33.0,"z":33.0})
    );
}

#[test]
fn connected_houses_return_group_and_cell_labels() {
    let mut source = map();
    source["rows"][10] = json!(format!("{}BB{}", ".".repeat(10), ".".repeat(20)));
    let houses = call(ww2_authoring_house_kinds, &source);
    assert_eq!(houses["blocks"].as_array().unwrap().len(), 1);
    assert_eq!(houses["blocks"][0]["cells"], json!([330, 331]));
    assert_eq!(houses["kinds"][330], houses["blocks"][0]["kind"]);
}

#[test]
fn fairness_zero_tick_timeout_is_seeded_and_reports_actual_starts() {
    let input = json!({"map":map(),"seed":77,"max_ticks":0,"options":{"players":2}});
    let result = call(ww2_authoring_play_match, &input);
    assert_eq!(result, call(ww2_authoring_play_match, &input));
    assert_eq!(result["winner"], Value::Null);
    assert_eq!(result["ticks"], 0);
    assert_eq!(result["timedOut"], true);
    assert_eq!(result["players"].as_array().unwrap().len(), 2);
}

#[test]
fn fairness_runs_eight_ticks_and_replays_the_same_native_outcome() {
    let input = json!({"map":map(),"seed":77,"max_ticks":8,"options":{"players":2}});
    let result = call(ww2_authoring_play_match, &input);
    assert_eq!(result, call(ww2_authoring_play_match, &input));
    assert_eq!(result["ticks"], 8);
    assert_eq!(result["player_vp"].as_array().unwrap().len(), 2);
    assert_eq!(result["timedOut"], true);
}

#[test]
fn rejected_modes_and_insufficient_buffers_never_publish_an_answer() {
    let source =
        serde_json::to_vec(&json!({"map":map(),"seed":1,"options":{"mode":"world"}})).unwrap();
    let mut output = [99_i8; 8];
    let mut written = 99;
    // SAFETY: buffers and output count are valid.
    let status = unsafe {
        ww2_authoring_create_preview(
            source.as_ptr().cast(),
            source.len(),
            output.as_mut_ptr(),
            output.len(),
            &mut written,
        )
    };
    assert_ne!(status, WW2_OK);
    assert_eq!(output, [99; 8]);
    let source = serde_json::to_vec(&map()).unwrap();
    // SAFETY: the one-byte capacity is within the eight-byte output allocation.
    let status = unsafe {
        ww2_authoring_validate_map(
            source.as_ptr().cast(),
            source.len(),
            output.as_mut_ptr(),
            1,
            &mut written,
        )
    };
    assert_eq!(status, WW2_ERR_CAPACITY);
    assert_eq!(output, [99; 8]);
}

#[path = "../../server/src/authoritative-map-data.rs"]
mod authored_maps;

#[test]
fn every_committed_map_validates_through_the_c_entry_point() {
    assert_eq!(authored_maps::MAPS.len(), 33);
    for &(name, source) in authored_maps::MAPS {
        let value: Value = serde_json::from_str(source).unwrap();
        assert_eq!(
            call(ww2_authoring_validate_map, &value),
            Value::Null,
            "{name}"
        );
    }
}

#[test]
fn spawn_filter_is_pure_and_does_not_require_runtime_map_admission() {
    let map = json!({"spawns":[{}, {"assault":true}, {"assault":false}]});
    assert_eq!(
        call(ww2_authoring_spawns_for, &json!({"map":map,"mode":"world"})),
        json!([0, 2])
    );
    assert_eq!(
        call(
            ww2_authoring_spawns_for,
            &json!({"map":map,"mode":"assault"})
        ),
        json!([0, 1, 2])
    );
}

#[test]
fn authored_path_keeps_the_source_clearance_around_houses() {
    let mut source = map();
    source["points"][0] = json!({"x":10,"y":24});
    source["rows"][16] = json!(format!("{}BB{}", ".".repeat(16), ".".repeat(14)));
    assert_eq!(
        call(
            ww2_authoring_find_path,
            &json!({"map":source,"from":{"x":9,"z":9},"to":{"x":55,"z":55}})
        ),
        json!([{"x":29.0,"z":33.0},{"x":31.0,"z":35.0},{"x":55.0,"z":55.0}])
    );
}

#[test]
fn fairness_crosses_both_commander_beats_with_complete_recipient_delivery() {
    let source = json!({"name":"Distant armies","w":96,"h":96,"rows":vec![".".repeat(96);96],
        "spawns":[{"x":4,"y":4},{"x":91,"y":91}],"points":[{"x":48,"y":48}]});
    let input = json!({"map":source,"seed":77,"max_ticks":80,"options":{"players":2}});
    let result = call(ww2_authoring_play_match, &input);
    assert_eq!(result, call(ww2_authoring_play_match, &input));
    assert_eq!(result["ticks"], 80);
    assert_eq!(result["timedOut"], true);
    assert_eq!(result["player_vp"].as_array().unwrap().len(), 2);
}
