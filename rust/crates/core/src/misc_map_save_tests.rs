use crate::{
    map::AuthoritativeMap,
    map_save::{MAX_SAVE_BYTES, prepare_map_save},
};
use serde_json::Value;

#[path = "../../server/src/authoritative-map-data.rs"]
mod shipped;

#[test]
fn map23_existing_save_component_matches_original_projection_for_all_shipped_maps_and_field_matrix() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../tests/fixtures/misc-map-save.json"
    ))
    .unwrap();
    assert_eq!(
        fixture["reference"],
        "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
    );
    assert_eq!(fixture["maxBytes"].as_u64().unwrap() as usize, MAX_SAVE_BYTES);
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(shipped::MAPS.len(), 33);
    assert_eq!(cases.len(), shipped::MAPS.len() + 2);
    for row in cases {
        let name = row["name"].as_str().unwrap();
        let source = if let Some(key) = name.strip_prefix("authored:") {
            let original = shipped::MAPS.iter().find(|(name, _)| *name == key).unwrap().1;
            original.as_bytes().to_vec()
        } else {
            serde_json::to_vec(&row["map"]).unwrap()
        };
        let saved = prepare_map_save(&source).unwrap_or_else(|error| panic!("{name}: {error}"));
        let actual: Value = serde_json::from_slice(&saved).unwrap();
        assert_eq!(actual, row["saved"], "{name}: original server projection");
        AuthoritativeMap::from_json(&saved).unwrap_or_else(|error| panic!("{name}: {error}"));
        assert_eq!(prepare_map_save(&saved).unwrap(), saved, "{name}: stable second save");
    }
}
