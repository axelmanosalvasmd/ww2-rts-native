use super::*;
use ww2_core::{
    economy::Faction,
    map::AuthoritativeMap,
    match_runtime::{MatchOptions, PlayerConfig},
    recipient_snapshot::DeliveryState,
    world::CellChange,
};

#[test]
fn map104_material_startup_uses_public_projection_then_only_delivers_observed_changes() {
    let visible = 8 * 64 + 10;
    let hidden = 60;
    let mut input = json!({"name":"Material startup","w":64,"h":64,
        "rows":vec![".".repeat(64);64],"spawns":[{"x":8,"y":8},{"x":55,"y":55}],
        "points":[{"x":32,"y":32}],"worldVersion":2,
        "layers":{"ground":vec![".".repeat(64);64],"objects":vec![".".repeat(64);64],"mines":vec![".".repeat(64);64],
            "materials":[{"c":visible,"material":"steel"}],"objectMaterials":[{"c":visible,"material":"steel"}],
            "groundMaterials":[{"c":visible,"material":"road"}],"mineMaterials":[{"c":visible,"material":"steel"}]},
        "triggers":[{"at":600,"say":"private authoring"}]});
    let row = |ch: char| format!("{}{}{}", ".".repeat(10), ch, ".".repeat(53));
    input["rows"][8] = json!(row('N'));
    input["layers"]["ground"][8] = json!(row('D'));
    input["layers"]["objects"][8] = json!(row('N'));
    let authored = AuthoritativeMap::from_json(&serde_json::to_vec(&input).unwrap()).unwrap();
    let mut state = Match::new(
        &authored,
        MatchOptions {
            seed: 701,
            mode: Mode::Conquest,
            players: vec![
                PlayerConfig {
                    name: "A".into(),
                    team: 0,
                    faction: Faction::Usa,
                },
                PlayerConfig {
                    name: "B".into(),
                    team: 1,
                    faction: Faction::Germany,
                },
            ],
            shuffle: false,
            army: ArmySize::Standard,
            weather: WeatherChoice::Map,
            weather_seed: Some(1),
            living_weather: false,
            map_key: Some("batch3-material".into()),
            tech: false,
            supply: false,
            logistics: false,
            sandbox: false,
        },
    )
    .unwrap();
    let startup = NativeStartup::prepare_authored(
        &state,
        StartupSettings {
            map: "batch3-material",
            mode: "conquest",
            army: "standard",
            weather: "map",
            sandbox: false,
        },
        &authored,
    )
    .unwrap();
    let envelope = startup
        .envelope(&state, &["A".into(), "B".into()], 17)
        .unwrap();
    let mut delivery = DeliveryState::new(&state).unwrap();
    let initial = delivery
        .capture_with_lists(&mut state, &[true; 2], &[false; 2], &[])
        .unwrap()
        .recipient(0)
        .unwrap()
        .into_json();
    let start = envelope.message(0, initial);
    assert_eq!(start["map"]["rows"][8], json!(row('D')));
    assert_eq!(start["map"]["layers"]["materials"], json!([]));
    assert_eq!(start["map"]["layers"]["objectMaterials"], json!([]));
    assert_eq!(start["map"]["layers"]["mineMaterials"], json!([]));
    assert_eq!(
        start["map"]["layers"]["groundMaterials"],
        input["layers"]["groundMaterials"]
    );
    assert!(start["map"].get("triggers").is_none());
    assert!(
        !start["cells"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row[0] == hidden)
    );
    state
        .world
        .mutate_cell(
            visible,
            CellChange {
                object: Some(b'R'),
                mine: Some(false),
                material: Some("stone".into()),
                ..Default::default()
            },
        )
        .unwrap();
    state
        .world
        .mutate_cell(
            hidden,
            CellChange {
                ground: Some(b'D'),
                material: Some("road".into()),
                ..Default::default()
            },
        )
        .unwrap();
    let mut delivered = false;
    for _ in 0..6 {
        state.advance_tick().unwrap();
        let snapshot = delivery
            .capture_with_lists(&mut state, &[false; 2], &[false; 2], &[])
            .unwrap()
            .recipient(0)
            .unwrap()
            .into_json();
        let cells = snapshot["cells"].as_array().unwrap();
        delivered |= cells.iter().any(|row| row[0] == visible && row[1] == "R");
        assert!(!cells.iter().any(|row| row[0] == hidden));
    }
    assert!(delivered);
    assert_eq!(authored.source(), &input);
}
