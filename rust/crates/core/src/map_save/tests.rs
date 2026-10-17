use super::*;
use crate::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    map_authoring::{LayerSelection, MapAuthoring},
    match_runtime::{Match, MatchOptions, PlayerConfig},
    recipient_snapshot::SnapshotCache,
    weather::WeatherChoice,
};

use crate::shipped_map_fixtures as shipped;

fn document() -> Value {
    json!({"name":"Editor", "w":60,"h":60,"rows":vec![".".repeat(60);60],
        "spawns":[{"x":3,"y":3},{"x":56,"y":56}],
        "points":[{"id":"target","x":30,"y":30}],"pointSequence":0})
}

fn options() -> MatchOptions {
    MatchOptions {
        seed: 701,
        mode: Mode::Conquest,
        players: vec![
            PlayerConfig {
                name: "a".into(),
                team: 0,
                faction: Faction::Usa,
            },
            PlayerConfig {
                name: "b".into(),
                team: 1,
                faction: Faction::Germany,
            },
        ],
        shuffle: false,
        army: ArmySize::Standard,
        weather: WeatherChoice::Map,
        weather_seed: Some(1),
        living_weather: false,
        map_key: None,
        tech: false,
        supply: false,
        logistics: false,
        sandbox: false,
    }
}

fn save(value: &Value) -> Value {
    serde_json::from_slice(&prepare_map_save(&serde_json::to_vec(value).unwrap()).unwrap()).unwrap()
}

#[test]
fn map23_save_projection_keeps_only_authored_fields_and_all_shipped_maps_round_trip() {
    let mut value = document();
    value["unknown"] = json!({"private":"discard"});
    value["spawns"][0]["unknown"] = json!(7);
    value["spawns"][0]["assault"] = json!(false);
    value["spawns"][1]["assault"] = json!(true);
    value["spawns"]
        .as_array_mut()
        .unwrap()
        .push(json!({"x":3,"y":56}));
    value["points"][0]["unknown"] = json!(7);
    value["points"][0]["vp"] = json!(0);
    value["scenario"] = json!({"version":1,"areas":[],"triggers":[]});
    let expected = json!({"name":"Editor","w":60,"h":60,"rows":vec![".".repeat(60);60],
        "spawns":[{"x":3,"y":3},{"x":56,"y":56,"assault":true},{"x":3,"y":56}],
        "points":[{"id":"target","x":30,"y":30,"vp":0,"mp":1}],"pointSequence":0,
        "scenario":{"version":1,"areas":[],"triggers":[]}});
    assert_eq!(save(&value), expected);
    assert_eq!(shipped::MAPS.len(), 33);
    for (name, source) in shipped::MAPS {
        let original = AuthoritativeMap::from_json(source.as_bytes()).unwrap();
        let bytes = prepare_map_save(source.as_bytes()).unwrap_or_else(|e| panic!("{name}: {e}"));
        let loaded = AuthoritativeMap::from_json(&bytes).unwrap();
        let a = original.initialize_terrain().unwrap();
        let b = loaded.initialize_terrain().unwrap();
        assert_eq!(
            (
                &a.chars, &a.layers, &a.flags, &a.height, &a.house, &a.cell_hp, &a.tough
            ),
            (
                &b.chars, &b.layers, &b.flags, &b.height, &b.house, &b.cell_hp, &b.tough
            ),
            "{name}"
        );
        assert_eq!(a.wear, b.wear, "{name}: wear");
        assert_eq!(a.surface_wear, b.surface_wear, "{name}: surface wear");
        // Unfaced cells use NaN. Compare their complete representation too.
        let bits = |values: &Option<Vec<f32>>| {
            values
                .as_ref()
                .map(|v| v.iter().map(|n| n.to_bits()).collect::<Vec<_>>())
        };
        assert_eq!(
            bits(&a.trench_front),
            bits(&b.trench_front),
            "{name}: trench facing"
        );
        assert_eq!(
            (&a.shoal, a.roads, a.naval),
            (&b.shoal, b.roads, b.naval),
            "{name}"
        );
        assert_eq!(original.structures(), loaded.structures(), "{name}");
        assert_eq!(original.spawns(), loaded.spawns(), "{name}");
        assert_eq!(original.points(), loaded.points(), "{name}");
        assert_eq!(original.buildings(), loaded.buildings(), "{name}");
        assert_eq!(original.triggers(), loaded.triggers(), "{name}");
        assert_eq!(
            prepare_map_save(&bytes).unwrap(),
            bytes,
            "stable second save: {name}"
        );
    }
}

#[test]
fn map23_editor_limit_counts_incoming_bytes_before_parsing_or_projection() {
    let mut bytes = serde_json::to_vec(&document()).unwrap();
    bytes.resize(MAX_SAVE_BYTES, b' ');
    assert!(
        prepare_map_save(&bytes).is_ok(),
        "exactly 8 MiB is accepted"
    );
    bytes.push(b' ');
    assert!(
        matches!(prepare_map_save(&bytes), Err(MapError::Invalid {field,reason})
        if field == "document" && reason.contains("8 MiB"))
    );
    assert!(prepare_map_save(b"{").is_err());
    let mut invalid = document();
    invalid["spawns"] = json!([]);
    assert!(prepare_map_save(&serde_json::to_vec(&invalid).unwrap()).is_err());
}

#[test]
fn map15_map_loading_uses_the_existing_complete_scenario_validator() {
    let mut valid = document();
    valid["scenario"] = json!({"version":1,
        "groups":[{"id":"squad","units":[{"id":"leader","type":"rifle","side":0,"x":9,"y":9}]}],
        "triggers":[{"id":"intro","scope":"match","condition":{"kind":"time","seconds":0},
            "actions":[{"kind":"say","text":{"en":"Ready"},"recipients":"public"}]}]});
    let loaded = AuthoritativeMap::from_value(valid.clone()).unwrap();
    assert_eq!(loaded.source()["scenario"], valid["scenario"]);
    assert_eq!(save(&valid)["scenario"], valid["scenario"]);
    for (pointer, bad) in [
        ("/scenario/version", json!(2)),
        ("/scenario/groups/0/units/0/type", json!("missing")),
        ("/scenario/groups/0/units/0/side", json!(2)),
        (
            "/scenario/triggers/0/actions/0/recipients",
            json!("missing"),
        ),
        (
            "/scenario/triggers/0/condition",
            json!({"kind":"captured","target":"missing","side":0}),
        ),
    ] {
        let mut bad_map = valid.clone();
        *bad_map.pointer_mut(pointer).unwrap() = bad;
        let error = AuthoritativeMap::from_value(bad_map).unwrap_err();
        assert!(
            matches!(error, MapError::Invalid {field,..} if field == "scenario"),
            "{pointer}"
        );
    }
    let error = Match::new(&loaded, options()).err().unwrap();
    assert_eq!(
        error,
        "authored mission: authored groups and areas are unsupported"
    );

    // The timed-message subset is admitted, while the complete loader still
    // retains and validates the unsupported group definition above.
    valid["scenario"]["groups"] = json!([]);
    let timed = AuthoritativeMap::from_value(valid).unwrap();
    let mut state = Match::new(&timed, options()).unwrap();
    assert!(state.authored_scenario.is_some());
    let report = state.advance_tick().unwrap();
    assert!(report.events.iter().any(|event| matches!(
        event,
        crate::match_tick::TickEvent::AuthoredScenario(shot)
            if shot.shot_json()["k"] == "say" && shot.shot_json()["text"] == "Ready" && shot.shot_json()["pub"] == true
    )));
    let cache = SnapshotCache::new(&state).unwrap();
    for slot in 0..2 {
        cache.recipient(slot).unwrap();
    }
}

#[test]
fn edited_saved_map_ticks_and_collects_every_recipient_without_new_match_state() {
    let mut value = document();
    value["rows"][25] = json!(format!("{}B{}", ".".repeat(25), ".".repeat(34)));
    value["structures"] = json!([{"id":"house","kind":"house","sections":[
        {"id":"anchor","c":1525,"hp":321,"material":"steel","anchor":true,"supports":[]}]}]);
    let map = AuthoritativeMap::from_value(value).unwrap();
    let mut draft = MapAuthoring::from_map(&map);
    assert!(draft.move_selection(
        &mut LayerSelection {
            ch: b'B',
            cells: vec![[25, 25]]
        },
        2,
        0
    ));
    let mut edited = draft.document();
    edited["structures"][0]["extra"] = json!(true);
    edited["structures"][0]["sections"][0]["extra"] = json!(true);
    edited["layers"]["groundMaterials"] = json!([{"c":1525,"material":"soil","extra":true}]);
    let bytes = prepare_map_save(&serde_json::to_vec(&edited).unwrap()).unwrap();
    let loaded = AuthoritativeMap::from_json(&bytes).unwrap();
    assert!(loaded.source()["structures"][0].get("extra").is_none());
    assert!(
        loaded.source()["structures"][0]["sections"][0]
            .get("extra")
            .is_none()
    );
    assert_eq!(
        loaded.source()["layers"]["groundMaterials"],
        json!([{"c":1525,"material":"soil"}])
    );
    assert_eq!(loaded.structures()[0].sections[0].cell, 1527);
    let mut state = Match::new(&loaded, options()).unwrap();
    for _ in 0..5 {
        state.advance_tick().unwrap();
        let cache = SnapshotCache::new(&state).unwrap();
        for slot in 0..2 {
            cache.recipient(slot).unwrap();
        }
        state.preflight_ordinary_snapshot().unwrap();
    }
}
