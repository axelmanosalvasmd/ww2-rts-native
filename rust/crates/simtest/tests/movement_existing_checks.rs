//! Selected full Source scenes. Generate the fixture only in a Root-granted slot.
//! Transport recording is retained for a future complete Match/recipient consumer.
use serde_json::{Value, json};
use std::io::Read;
use ww2_core::{
    catalog::{Catalog, Value as CatalogValue},
    game::Point,
    grid::SpatialGrid,
    navigation::{TerrainView, Versions},
    pathfinding::{PathRules, PathTerrain, Pathfinding},
    rules::Rules,
    separation::separate_crowds,
    world_generation::{self, WorldSize},
};

fn fixture() -> Value {
    // Read at runtime so compilation does not require a recorded fixture.
    // Absence is a hard failure, never a skip or an empty successful replay.
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/movement-existing-checks.json.gz"
    );
    let file = std::fs::File::open(path)
        .expect("Root must grant a recorder slot and generate movement-existing-checks.json.gz");
    let mut text = String::new();
    flate2::read::GzDecoder::new(file)
        .read_to_string(&mut text)
        .unwrap();
    let fixture: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        fixture["reference"],
        "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
    );
    assert_eq!(fixture["node"], "v24.21.0");
    assert_eq!(fixture["arch"], "x64");
    assert_eq!(fixture["cases"].as_array().unwrap().len(), 4);
    assert!(
        fixture["cases"]
            .as_array()
            .unwrap()
            .iter()
            .all(|case| case["assertionsCompleted"] == true)
    );
    fixture
}

fn bits(n: f64) -> Value {
    json!(format!("{:016x}", n.to_bits()))
}

fn point(value: &Value) -> Point {
    Point {
        x: value["x"].as_f64().unwrap(),
        z: value["z"].as_f64().unwrap(),
    }
}

fn typed_numbers(value: &Value, kind: &str) -> Vec<i64> {
    let value = &value[kind];
    if let Some(values) = value.as_array() {
        return values.iter().map(|n| n.as_i64().unwrap()).collect();
    }
    value["runs"]
        .as_array()
        .unwrap()
        .as_chunks::<2>()
        .0
        .iter()
        .flat_map(|pair| {
            std::iter::repeat_n(
                pair[0].as_i64().unwrap(),
                pair[1].as_u64().unwrap() as usize,
            )
        })
        .collect()
}

#[test]
fn editor_without_players_preserves_both_original_reachability_checks() {
    let fixture = fixture();
    let case = &fixture["cases"][0];
    assert_eq!(case["row"], "MOV-43");
    let calls = case["calls"].as_array().unwrap();
    assert_eq!(calls.len(), 2);
    let rules = Rules::compiled().unwrap();
    let kind = &rules.kinds[rules.kind("rifle").unwrap()];
    // Each Source findPath call builds a fresh editor navigation view after edits.
    for (index, call) in calls.iter().enumerate() {
        assert_eq!(call["op"], "findPath");
        let before = &call["before"];
        assert!(before.get("players").is_none() && before.get("mines").is_none());
        assert_eq!(before["w"], 20);
        assert_eq!(before["h"], 20);
        let flags: Vec<u16> = typed_numbers(&before["flags"], "$Uint16Array")
            .into_iter()
            .map(|n| u16::try_from(n).unwrap())
            .collect();
        let heights: Vec<i8> = typed_numbers(&before["height"], "$Int8Array")
            .into_iter()
            .map(|n| i8::try_from(n).unwrap())
            .collect();
        let near = vec![0; 400];
        let hazards = vec![false; 400];
        let terrain = PathTerrain {
            navigation: TerrainView {
                width: 20,
                height: 20,
                flags: &flags,
                heights: Some(&heights),
                world_known: false,
                knowledge_id: 0,
                versions: Versions::default(),
                changed: None,
                changes: &[],
            },
            near_walls: &near,
            known_mines: &hazards,
            fires: &hazards,
            bunkers: &[],
            wear: None,
            wetness: 0.0,
            roads: false,
        };
        let route = Pathfinding::new()
            .find_path(
                terrain,
                PathRules::compiled().unwrap(),
                kind,
                point(&call["input"]["from"]),
                point(&call["input"]["to"]),
                None,
            )
            .unwrap();
        let expected = call["result"].as_array().unwrap();
        assert_eq!(route.waypoints.len(), expected.len(), "editor call {index}");
        for (actual, expected) in route.waypoints.iter().zip(expected) {
            assert_eq!(bits(actual.x), bits(expected["x"].as_f64().unwrap()));
            assert_eq!(bits(actual.z), bits(expected["z"].as_f64().unwrap()));
        }
        assert_eq!(route.waypoints.is_empty(), index == 1);
    }
}

#[test]
fn complete_crowded_army_and_later_pair_scenes_preserve_every_position() {
    let fixture = fixture();
    let rules = Rules::compiled().unwrap();
    for (index, expected_calls) in [(1, 20), (2, 1)] {
        let case = &fixture["cases"][index];
        assert_eq!(case["row"], "MOV-62");
        let calls: Vec<_> = case["calls"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|call| call["op"] == "separateCrowds" || call["op"] == "step")
            .collect();
        assert_eq!(calls.len(), expected_calls);
        // Restore only the authored starting state. Native output feeds every later pass.
        let mut game = simtest::load::load_game(&calls[0]["before"], &rules)
            .unwrap()
            .game;
        let mut grid = SpatialGrid::new(&game.units, 16.0);
        for (pass, call) in calls.iter().enumerate() {
            separate_crowds(&mut game, &rules, &mut grid);
            let actual = json!(
                game.units
                    .values()
                    .map(|u| json!([u.id, bits(u.x), bits(u.z)]))
                    .collect::<Vec<_>>()
            );
            assert_eq!(
                actual,
                call["positions"],
                "{} pass {}",
                case["name"],
                pass + 1
            );
        }
        assert_eq!(game.units.len(), if index == 1 { 300 } else { 50 });
    }
}

fn catalog_json(value: &CatalogValue) -> Value {
    match value {
        CatalogValue::Null => Value::Null,
        CatalogValue::Bool(v) => json!(v),
        CatalogValue::Number(v) => json!(v),
        CatalogValue::String(v) => json!(v),
        CatalogValue::Array(v) => Value::Array(v.iter().map(catalog_json).collect()),
        CatalogValue::Object(v) => Value::Object(
            v.iter()
                .map(|(key, value)| (key.clone(), catalog_json(value)))
                .collect(),
        ),
    }
}

#[test]
fn complete_aircraft_definitions_and_air_configuration_match_source() {
    let fixture = fixture();
    let catalog = Catalog::compiled().unwrap();
    let source = &fixture["aircraft"];
    assert_eq!(source["units"].as_object().unwrap().len(), 3);
    for name in ["fighter", "attacker", "bomber"] {
        crate::shared::aviation::assert_json(
            &catalog_json(catalog.unit(name).unwrap()),
            &source["units"][name],
            name,
        );
    }
    crate::shared::aviation::assert_json(
        &catalog_json(catalog.table("CFG").unwrap().get("air").unwrap()),
        &source["air"],
        "CFG.air",
    );
}

#[test]
fn five_original_world_seeds_preserve_complete_ordered_rail_networks() {
    let fixture = fixture();
    let rows = fixture["rails"].as_array().unwrap();
    assert_eq!(
        rows.iter()
            .map(|row| row["options"]["seed"].as_u64().unwrap())
            .collect::<Vec<_>>(),
        [0, 1, 42, 20261003, 4294967295]
    );
    for row in rows {
        let options = &row["options"];
        assert_eq!(options["size"], "huge");
        assert_eq!(options["players"], 6);
        assert_eq!(options["scenery"], false);
        let teams: Vec<_> = options["teams"]
            .as_array()
            .unwrap()
            .iter()
            .map(|team| i32::try_from(team.as_i64().unwrap()).unwrap())
            .collect();
        let world = world_generation::generate_world_from_terrain_seed(
            u32::try_from(options["seed"].as_u64().unwrap()).unwrap(),
            WorldSize::Huge,
            &teams,
            false,
        )
        .unwrap();
        let actual = json!({
            "stations": world.rails.stations.iter().map(|station|
                json!({"region":station.region,"x":station.x,"y":station.y})).collect::<Vec<_>>(),
            "lines": world.rails.lines.iter().map(|line|
                json!({"a":line.a,"b":line.b,"regions":line.regions,"cells":line.cells})).collect::<Vec<_>>()
        });
        assert_eq!(actual, row["expected"], "rail seed {}", options["seed"]);
    }
}
