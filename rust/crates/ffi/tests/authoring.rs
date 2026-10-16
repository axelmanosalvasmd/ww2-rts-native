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

fn biased_map(mirror: bool) -> Value {
    let mut source = json!({"name":"Biased authoring test","w":96,"h":96,
        "rows":vec![".".repeat(96);96],
        "spawns":[{"x":4,"y":4},{"x":91,"y":91}],
        "points":[{"x":5,"y":5,"vp":5,"mp":0},{"x":5,"y":8,"vp":5,"mp":0},
                  {"x":8,"y":5,"vp":5,"mp":0}]});
    if mirror {
        for point in source["points"].as_array_mut().unwrap() {
            point["x"] = json!(95 - point["x"].as_u64().unwrap());
            point["y"] = json!(95 - point["y"].as_u64().unwrap());
        }
    }
    source
}

#[test]
fn fairness_finishes_real_ai_matches_and_credits_actual_shuffled_starts() {
    for mirror in [false, true] {
        let source = biased_map(mirror);
        assert_eq!(call(ww2_authoring_validate_map, &source), Value::Null);
        for seed in [7, 77] {
            let input = json!({"map":source,"seed":seed,"max_ticks":2400,
                "options":{"players":2}});
            let result = call(ww2_authoring_play_match, &input);
            assert_eq!(result, call(ww2_authoring_play_match, &input));
            assert_eq!(result["timedOut"], false, "{result}");
            assert!(result["ticks"].as_u64().unwrap() > 80);
            let winner = result["winner"].as_u64().unwrap() as usize;
            assert_eq!(
                result["players"][winner]["spawn"],
                if mirror {
                    json!({"x":183.0,"z":183.0})
                } else {
                    json!({"x":9.0,"z":9.0})
                }
            );
            assert!(result["player_vp"][winner].as_f64().unwrap() >= 1200.0);
            assert_eq!(result["winning_team"], result["players"][winner]["team"]);
        }
    }
}

#[test]
fn preview_exposes_classic_resources_and_horde_roles() {
    let mut source = biased_map(false);
    source["points"] = json!([{"x":48,"y":48,"vp":1,"mp":1}]);
    let classic = call(
        ww2_authoring_create_preview,
        &json!({"map":source,"options":{"mode":"classic","players":2}}),
    );
    assert_eq!(
        classic["units"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|u| u["type"] == "hq")
            .count(),
        2
    );
    let nodes = classic["nodes"].as_array().unwrap();
    assert!(nodes.iter().any(|node| node["fuel"] == true));
    assert!(nodes.iter().any(|node| node["fuel"] == false));
    source["defend"] = json!([0]);
    let horde = call(
        ww2_authoring_create_preview,
        &json!({"map":source,"options":{"mode":"horde","players":2}}),
    );
    assert_eq!(horde["players"].as_array().unwrap().len(), 3);
    assert_eq!(horde["players"][0]["spawn"], horde["players"][1]["spawn"]);
    assert_eq!(horde["horde"]["slot"], 2);
    let gate = &horde["horde"]["gates"][0];
    assert_eq!(horde["horde"]["gates"].as_array().unwrap().len(), 1);
    assert!((gate["x"].as_f64().unwrap() - 171.68629150101523).abs() < 1e-10);
    assert!((gate["z"].as_f64().unwrap() - 171.68629150101523).abs() < 1e-10);
    assert_eq!(horde["horde"]["active"], false);
    assert!(horde["horde"]["timeLeft"].as_f64().unwrap() > 0.0);
    assert_eq!(
        horde["units"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|u| u["type"] == "bunker")
            .count(),
        1
    );
}

#[test]
fn preview_assault_and_annihilation_preserve_roles_forts_and_point_filtering() {
    let mut source = biased_map(false);
    source["defend"] = json!([1]);
    source["assaultTime"] = json!(600);
    source["points"] = json!([{"x":48,"y":48,"vp":5,"mp":0},{"x":48,"y":50,"vp":1,"mp":1}]);
    for mode in ["assault", "annihilation"] {
        let preview = call(
            ww2_authoring_create_preview,
            &json!({"map":source,"options":{"mode":mode,"players":2}}),
        );
        assert_eq!(preview["points"].as_array().unwrap().len(), 1);
        assert!(
            preview["units"]
                .as_array()
                .unwrap()
                .iter()
                .any(|u| u["type"] == "bunker")
        );
        assert!(
            preview["rows"]
                .as_array()
                .unwrap()
                .iter()
                .any(|r| r.as_str().unwrap().contains('T'))
        );
        if mode == "assault" {
            assert_eq!(preview["players"][0]["spawn"], json!({"x":183.0,"z":183.0}));
            assert_eq!(preview["players"][0]["team"], 0);
            assert_eq!(preview["players"][1]["team"], 1);
            assert_eq!(preview["assault"]["time_left"], 600.0);
        } else {
            assert_eq!(preview["elimination"]["bunkers"], json!([1, 1]));
            assert_eq!(preview["elimination"]["time_left"], Value::Null);
        }
    }
}

#[test]
fn fairness_moves_beyond_home_capture_range_before_winning() {
    let source = json!({"name":"AI must leave home","w":128,"h":128,
        "rows":vec![".".repeat(128);128],
        "spawns":[{"x":4,"y":4},{"x":123,"y":123}],
        "points":[{"x":20,"y":20,"vp":5,"mp":0},{"x":20,"y":23,"vp":5,"mp":0},
                  {"x":23,"y":20,"vp":5,"mp":0}]});
    let input = json!({"map":source,"seed":77,"max_ticks":3600,"options":{"players":2}});
    let early = call(
        ww2_authoring_play_match,
        &json!({"map":source,"seed":77,"max_ticks":80,"options":{"players":2}}),
    );
    assert_eq!(early["player_vp"], json!([0.0, 0.0]));
    let result = call(ww2_authoring_play_match, &input);
    assert_eq!(result, call(ww2_authoring_play_match, &input));
    assert_eq!(result["timedOut"], false, "{result}");
    let winner = result["winner"].as_u64().unwrap() as usize;
    assert_eq!(result["players"][winner]["spawn"], json!({"x":9.0,"z":9.0}));
    assert!(result["player_vp"][winner].as_f64().unwrap() >= 1200.0);
}

#[test]
fn horde_preview_without_defenders_keeps_the_prototype_conquest_fallback() {
    let result = call(
        ww2_authoring_create_preview,
        &json!({"map":map(),"options":{"mode":"horde","players":2,"logistics":true}}),
    );
    assert_eq!(result["horde"], Value::Null);
    assert_eq!(result["players"].as_array().unwrap().len(), 2);
    assert_eq!(result["winVp"], 1200.0);
}

#[test]
fn sixty_seeded_c_matches_reproduce_the_editor_unfair_verdict_by_authored_spawn() {
    let source = biased_map(false);
    let mut wins = [0_u32; 2];
    let mut timeouts = 0;
    let mut winning_seats = std::collections::BTreeSet::new();
    for seed in 7..67 {
        let result = call(ww2_authoring_play_match,
            &json!({"map":source,"seed":seed,"max_ticks":2400,"options":{"players":2}}));
        if result["timedOut"] == true {
            timeouts += 1;
            continue;
        }
        let winner = result["winner"].as_u64().unwrap() as usize;
        winning_seats.insert(winner);
        let spawn = &result["players"][winner]["spawn"];
        let index = if spawn == &json!({"x":9.0,"z":9.0}) { 0 }
            else if spawn == &json!({"x":183.0,"z":183.0}) { 1 }
            else { panic!("winner has no authored spawn: {result}") };
        wins[index] += 1;
    }
    // client/fairness.js credits the shuffled start; editor.js:392-399 uses this precedence.
    let played = 60 - timeouts;
    let percentages = wins.map(|n| if played == 0 { 0 } else { (f64::from(n) / f64::from(played) * 100.0).round() as u32 });
    let verdict = if timeouts > 12 { "too-long" }
        else if played < 60 { "too-few" }
        else if percentages.iter().any(|p| *p < 30 || *p > 70) { "unfair" }
        else { "fair" };
    assert_eq!((wins,timeouts,percentages,verdict),([60,0],0,[100,0],"unfair"));
    assert_eq!(winning_seats.len(),2,"spawn attribution must survive seat shuffling");
}
