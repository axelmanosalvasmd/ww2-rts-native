//! MOV-13's complete original border-ray matrix through the native LOS consumer.
//! Query outputs are comparison-only. Match construction owns the native terrain.
use serde_json::{Value, json};
use std::io::Read;
use ww2_core::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::SIGHT,
    map::AuthoritativeMap,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    vision::los,
    weather::WeatherChoice,
};

#[test]
fn complete_original_border_rays_both_directions_and_wall_controls() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/mov-border-rays.json.gz");
    let file = std::fs::File::open(path).expect("fresh Root-granted MOV-13 Source capture required");
    let mut text = String::new();
    flate2::read::GzDecoder::new(file).read_to_string(&mut text).unwrap();
    let f: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(f["reference"], "e4e579a838aabae3b1d9dadbc17a0a87147bc52d");
    assert_eq!(f["node"], "v24.21.0");
    assert_eq!(f["arch"], "x64");
    assert_eq!(f["completed"], true);
    assert!(f["failure"].is_null() && f["readbackFailure"].is_null());
    assert_eq!(f["name"], "Sight rays ending on a cell border must stop at the destination, including mixed-sign diagonals");
    assert_eq!(f["bodySha256"], "fb60b3952780fada97f99ec4c6df6ba26a1dd3fdc328c750b0d01524dc689610");
    let constructors = f["constructors"].as_array().unwrap();
    assert_eq!(constructors.len(), 1);
    let original = &constructors[0];
    assert_eq!(original["completed"], true);
    assert_eq!(original["w"], 64);
    assert_eq!(original["h"], 64);
    assert!(original["state"].is_object());
    assert!(original.get("failure").is_none());
    let source_map = json!({"w":64,"h":64,"rows":vec![".".repeat(64);64],
        "spawns":[{"x":1,"y":1},{"x":60,"y":60}],"points":[]});
    assert_eq!(original["input"], json!([source_map, ["a", "b"], false]));
    // A disclosed zero-income point satisfies native authoring admission. It
    // changes no terrain. Full flags/height/smoke equality below must prove that.
    let mut map = source_map;
    map["name"] = json!("MOV-13 original border-ray terrain");
    map["points"] = json!([{"x":31,"y":4,"mp":0,"vp":0}]);
    let authoritative = AuthoritativeMap::from_json(&serde_json::to_vec(&map).unwrap()).unwrap();
    let mut m = Match::new(&authoritative, MatchOptions {
        seed: 7301,
        mode: Mode::Default,
        players: vec![PlayerConfig {name:"a".into(),team:0,faction:Faction::Usa},
            PlayerConfig {name:"b".into(),team:1,faction:Faction::Germany}],
        shuffle: false, army: ArmySize::Standard, weather: WeatherChoice::Map,
        weather_seed: Some(1), living_weather: true, map_key: None,
        tech: false, supply: false, logistics: false, sandbox: false,
    }).expect("canonical native Match, no guard bypass or actor import");
    let queries = f["queries"].as_array().unwrap();
    assert_eq!(queries.len(), 244);
    let point = |v: &Value| (v["x"].as_f64().unwrap(), v["z"].as_f64().unwrap());
    let offsets = [-23, -22, -11, -2, -1, 0, 1, 2, 11, 22, 23];
    let mut expected_inputs = Vec::new();
    for dx in offsets {
        for dz in offsets {
            let start = (62.0 + f64::from(dx), 92.0 + f64::from(dz));
            expected_inputs.push((start, (62.0, 92.0), true));
            expected_inputs.push(((62.0, 92.0), start, true));
        }
    }
    expected_inputs.push(((85.0, 69.0), (62.0, 92.0), false));
    expected_inputs.push(((85.0, 69.0), (62.0, 92.0), true));
    assert_eq!(json!(m.world.game.flags), original["flags"]);
    assert_eq!(json!(m.world.game.height), original["height"]);
    assert!(m.world.game.smokes.is_empty());
    assert_eq!(original["smokes"], json!([]));
    for (index, (query, (a, b, expected))) in queries.iter().zip(expected_inputs).enumerate() {
        if index == 242 { m.world.game.flags[40 * 64 + 37] |= SIGHT; }
        if index == 243 {
            m.world.game.flags[40 * 64 + 37] &= !SIGHT;
            m.world.game.flags[45 * 64 + 30] |= SIGHT;
        }
        assert_eq!(query["completed"], true);
        assert!(query.get("failure").is_none());
        assert_eq!(query["w"], 64);
        assert_eq!(query["h"], 64);
        assert_eq!(point(&query["a"]), a);
        assert_eq!(point(&query["b"]), b);
        assert_eq!(json!(m.world.game.flags), query["flags"]);
        assert_eq!(json!(m.world.game.height), query["height"]);
        assert_eq!(query["smokes"], json!([]));
        assert_eq!(query["expected"], expected);
        assert_eq!(los(&m.world.game, &m.rules, a, b), expected, "original LOS query {index}");
    }
}
