//! Full Source observations for the five owned vision/fog predicates.
//! The fixture is generated only after the coordinator grants a recorder slot.

use serde_json::{Value, json};
use simtest::load::load_game;
use ww2_core::fog::Fog;
use ww2_core::grid::SpatialGrid;
use ww2_core::rules::Rules;
use ww2_core::vision::{los, update_vision, update_vision_threaded, vision_of, vision_range};

fn fixture() -> Value {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/vision-rows.json");
    let bytes = std::fs::read(path).expect("generate vision-rows.json with a Root recorder slot");
    let value: Value = serde_json::from_slice(&bytes).expect("vision row fixture JSON");
    assert_eq!(value["pin"], "e4e579a838aabae3b1d9dadbc17a0a87147bc52d");
    value
}

#[test]
fn cmb124_sight_ranges_match_original_source() {
    let fixture = fixture();
    let rules = Rules::compiled().unwrap();
    let cases = fixture["rangeCases"].as_array().unwrap();
    assert_eq!(cases.len(), 18);
    for case in cases {
        let loaded = load_game(&case["game"], &rules).unwrap();
        let unit = &loaded.game.units[&(case["unit"].as_u64().unwrap() as u32)];
        let actual = [
            vision_of(&loaded.game, &rules, unit),
            vision_range(&loaded.game, &rules, unit),
        ]
        .map(|value| format!("{:016x}", value.to_bits()));
        assert_eq!(json!(actual), case["expected"], "{}", case["name"]);
    }
}

#[test]
fn cmb125_ground_visibility_matches_original_source() {
    let fixture = fixture();
    let rules = Rules::compiled().unwrap();
    let cases = fixture["visibleCases"].as_array().unwrap();
    assert_eq!(cases.len(), 30);
    for case in cases {
        for threaded in [false, true] {
            let mut loaded = load_game(&case["game"], &rules).unwrap();
            let grid = SpatialGrid::new(&loaded.game.units, 16.0);
            if threaded {
                update_vision_threaded(&mut loaded.game, &rules, &grid);
            } else {
                update_vision(&mut loaded.game, &rules, &grid);
            }
            let visible: Vec<_> = loaded.game.players.iter().map(|p| &p.visible).collect();
            assert_eq!(
                json!(visible),
                case["expected"],
                "{} threaded={threaded}",
                case["name"]
            );
        }
    }
}

#[test]
fn cmb128_line_of_sight_matches_original_source() {
    let fixture = fixture();
    let rules = Rules::compiled().unwrap();
    let cases = fixture["losCases"].as_array().unwrap();
    assert_eq!(cases.len(), 18);
    for case in cases {
        let loaded = load_game(&case["game"], &rules).unwrap();
        let point = |key: &str| {
            (
                case[key]["x"].as_f64().unwrap(),
                case[key]["z"].as_f64().unwrap(),
            )
        };
        assert_eq!(
            los(&loaded.game, &rules, point("a"), point("b")),
            case["expected"].as_bool().unwrap(),
            "{}",
            case["name"]
        );
    }
}

#[test]
fn cmb131_full_fog_masks_and_cache_transitions_match_original_source() {
    let fixture = fixture();
    let rules = Rules::compiled().unwrap();
    let frames = fixture["fogFrames"].as_array().unwrap();
    assert_eq!(frames.len(), 20);
    for threaded in [false, true] {
        let mut fog = Fog::default();
        for frame in frames {
            let mut loaded = load_game(&frame["game"], &rules).unwrap();
            let grid = SpatialGrid::new(&loaded.game.units, 16.0);
            update_vision(&mut loaded.game, &rules, &grid);
            let visible: Vec<_> = loaded.game.players.iter().map(|p| &p.visible).collect();
            assert_eq!(json!(visible), frame["visible"], "{}", frame["name"]);
            if threaded {
                fog.teams_threaded(&loaded.game, &rules, &[0, 1]);
            }
            let mut cold = Fog::default();
            for expected in frame["expected"].as_array().unwrap() {
                let team = expected["team"].as_i64().unwrap() as i32;
                let actual = fog.team(&loaded.game, &rules, team);
                assert_eq!(
                    json!({"team":team,"vis":actual.vis,"explored":actual.explored,
                        "version":actual.version,"deltas":actual.deltas}),
                    *expected,
                    "{} threaded={threaded}",
                    frame["name"]
                );
                assert_eq!(
                    actual.vis,
                    cold.team(&loaded.game, &rules, team).vis,
                    "{} cold recomputation",
                    frame["name"]
                );
            }
        }
    }
}
