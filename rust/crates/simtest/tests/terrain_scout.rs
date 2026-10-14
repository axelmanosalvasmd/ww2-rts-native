use serde_json::{Value, json};
use simtest::terrain_scout;
#[test]
fn full_source_world_scouting_rows_match_canonical_history() {
    let mut fixture: Value = serde_json::from_str(include_str!(
        "../../../../docs/fixtures/terrain/source-world-scout.json"
    ))
    .unwrap();
    assert_eq!(
        fixture["source"]["pin"],
        "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
    );
    assert_eq!(fixture["source"]["node"], "v24.21.0");
    assert_eq!(fixture["source"]["arch"], "x64");
    let expected = terrain_scout::expand_source_receipts(&fixture).unwrap();
    // Destroy expectations before replay to prove they cannot initialize state.
    fixture["dataPool"] = json!([]);
    fixture["receipts"] = json!([]);
    let actual = terrain_scout::replay(&fixture).unwrap();
    assert_eq!(actual["actorCount"], expected["actorCount"]);
    assert_eq!(actual["actionCount"], expected["actionCount"]);
    let native_receipts = actual["receipts"].as_array().unwrap();
    let source_receipts = expected["receipts"].as_array().unwrap();
    assert_eq!(native_receipts.len(), source_receipts.len());
    for (native, source) in native_receipts.iter().zip(source_receipts) {
        for key in ["name", "slot", "full", "rowCount", "tick", "memoryVersion"] {
            assert!(
                terrain_scout::same_numbers(&native[key], &source[key]),
                "{} metadata {key}",
                source["name"]
            );
        }
        let native_rows = native["rows"].as_array().unwrap();
        let source_rows = source["rows"].as_array().unwrap();
        assert_eq!(native_rows.len(), source_rows.len(), "{}", source["name"]);
        for (index, (native, source_row)) in native_rows.iter().zip(source_rows).enumerate() {
            assert!(
                terrain_scout::same_numbers(native, source_row),
                "{} row{index}: actual={native}, source={source_row}",
                source["name"]
            );
        }
    }
    assert_eq!(actual["actorCount"], 398);
    assert_eq!(actual["actionCount"], 32);
    let receipts = actual["receipts"].as_array().unwrap();
    assert_eq!(receipts.len(), 17);
    let reveal = receipts
        .iter()
        .find(|r| r["name"] == "end-reveal-all-world-rows")
        .unwrap();
    assert_eq!(reveal["rows"].as_array().unwrap().len(), 512 * 512);
    assert!(
        reveal["rows"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r[4].get("section").is_some())
    );
    assert!(
        reveal["rows"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r[4].get("biome").is_some())
    );
    assert!(
        reveal["rows"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r[4].get("station").is_some())
    );
}
