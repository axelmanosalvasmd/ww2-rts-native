//! Private capacity predicate only. No settings, sockets or native room admission change.
use super::*;

#[test]
fn existing_lobby_spawn_capacity_matches_shared_original_source_batch() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../simtest/tests/fixtures/mode-predicates.json");
    let fixture: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(path)
            .expect("generate the authorized shared mode-predicates fixture first"),
    )
    .unwrap();
    assert_eq!(
        fixture["reference"],
        "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
    );
    let (mut hub, _, _, _, _) = room();
    for c in fixture["spawnCases"].as_array().unwrap() {
        let room = hub.rooms.get_mut("ABC").unwrap();
        room.map = c["name"].as_str().unwrap().to_owned();
        room.mode = c["mode"].as_str().unwrap().to_owned();
        assert_eq!(
            room.capacity(&hub.maps) as u64,
            c["expected"]["capacity"].as_u64().unwrap(),
            "{} {}",
            c["name"],
            c["mode"]
        );
        assert!(room.native.is_none());
    }
    assert_eq!(fixture["spawnCases"].as_array().unwrap().len(), 21);
}
