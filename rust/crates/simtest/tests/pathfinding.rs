//! Compare the public path, resolved goal and search outcome with original JS.
use serde_json::Value;
use ww2_core::game::Point;
use ww2_core::navigation::{TerrainView, Versions};
use ww2_core::pathfinding::{PathError, PathRules, PathStatus, PathTerrain, Pathfinding};
use ww2_core::rules::UnitKind;

fn usize_field(value: &Value, key: &str) -> usize {
    value[key].as_u64().unwrap() as usize
}
fn point(value: &Value) -> Point {
    Point {
        x: value["x"].as_f64().unwrap(),
        z: value["z"].as_f64().unwrap(),
    }
}
fn bits(p: Point) -> Value {
    serde_json::json!([
        format!("{:016x}", p.x.to_bits()),
        format!("{:016x}", p.z.to_bits())
    ])
}
fn runs(value: &Value) -> Vec<i64> {
    value
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|pair| {
            std::iter::repeat_n(
                pair[0].as_i64().unwrap(),
                pair[1].as_u64().unwrap() as usize,
            )
        })
        .collect()
}
fn profile(name: &str, v: &Value) -> UnitKind {
    UnitKind {
        name: name.to_owned(),
        vision: 0.0,
        lookout: false,
        camo: false,
        structure: false,
        building: false,
        infantry: v["infantry"].as_bool().unwrap(),
        hull: 0.0,
        speed: v["speed"].as_f64().unwrap(),
        radius: v["radius"].as_f64().unwrap(),
        naval: v["naval"].as_bool().unwrap(),
        deep: v["deep"].as_bool().unwrap(),
        air: false,
        rail: false,
        weapon: false,
        ability_speed: None,
        vehicle: None,
    }
}

#[test]
fn infantry_and_naval_paths_match_original_find_path() {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/pathfinding.json")).unwrap();
    assert_eq!(
        fixture["reference"],
        "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
    );
    assert_eq!(fixture["node"], "v24.21.0");
    assert_eq!(fixture["arch"], "x64");
    let rules = PathRules {
        wall_hug: fixture["rules"]["wallHug"].as_f64().unwrap(),
        bunker_radius: fixture["rules"]["bunkerRadius"].as_f64().unwrap(),
    };
    let mut queries = 0;
    let mut found = 0;
    let mut unreachable = 0;
    let mut deferred = 0;
    let mut maps = 0;
    for scenario in fixture["scenarios"].as_array().unwrap() {
        let name = scenario["name"].as_str().unwrap();
        maps += usize::from(name.starts_with("map:"));
        let width = usize_field(scenario, "w");
        let height = usize_field(scenario, "h");
        let flags: Vec<_> = runs(&scenario["flags"])
            .into_iter()
            .map(|v| v as u16)
            .collect();
        let heights: Option<Vec<_>> = (!scenario["height"].is_null()).then(|| {
            runs(&scenario["height"])
                .into_iter()
                .map(|v| v as i8)
                .collect()
        });
        let near: Vec<_> = runs(&scenario["near"])
            .into_iter()
            .map(|v| v as u8)
            .collect();
        let mut mines = vec![false; flags.len()];
        for c in scenario["mines"].as_array().unwrap() {
            mines[c.as_u64().unwrap() as usize] = true;
        }
        let mut fires = vec![false; flags.len()];
        for c in scenario["fires"].as_array().unwrap() {
            fires[c.as_u64().unwrap() as usize] = true;
        }
        let bunkers: Vec<_> = scenario["bunkers"]
            .as_array()
            .unwrap()
            .iter()
            .map(point)
            .collect();
        let navigation = TerrainView {
            width,
            height,
            flags: &flags,
            heights: heights.as_deref(),
            world_known: scenario["worldKnown"].as_bool().unwrap(),
            knowledge_id: 1,
            versions: Versions::default(),
            changed: None,
            changes: &[],
        };
        let terrain = PathTerrain {
            navigation,
            near_walls: &near,
            known_mines: &mines,
            fires: &fires,
            bunkers: &bunkers,
        };
        let mut finder = Pathfinding::new();
        for (i, query) in scenario["queries"].as_array().unwrap().iter().enumerate() {
            let name_type = query["type"].as_str().unwrap();
            let kind = profile(name_type, &fixture["profiles"][name_type]);
            let hazards = if query["hazards"].is_null() {
                scenario
            } else {
                &query["hazards"]
            };
            let mut query_mines = vec![false; flags.len()];
            for c in hazards["mines"].as_array().unwrap() {
                query_mines[c.as_u64().unwrap() as usize] = true;
            }
            let mut query_fires = vec![false; flags.len()];
            for c in hazards["fires"].as_array().unwrap() {
                query_fires[c.as_u64().unwrap() as usize] = true;
            }
            let query_bunkers: Vec<_> = hazards["bunkers"]
                .as_array()
                .unwrap()
                .iter()
                .map(point)
                .collect();
            let query_terrain = PathTerrain {
                known_mines: &query_mines,
                fires: &query_fires,
                bunkers: &query_bunkers,
                ..terrain
            };
            let result = finder
                .find_path(
                    query_terrain,
                    rules,
                    &kind,
                    point(&query["from"]),
                    point(&query["to"]),
                    query["limit"].as_u64().map(|v| v as usize),
                )
                .unwrap();
            let status = match result.status {
                PathStatus::Found => {
                    found += 1;
                    "Found"
                }
                PathStatus::Unreachable => {
                    unreachable += 1;
                    "Unreachable"
                }
                PathStatus::ExpansionLimit => {
                    deferred += 1;
                    "ExpansionLimit"
                }
            };
            let actual = serde_json::json!({ "points": result.waypoints.iter().copied().map(bits).collect::<Vec<_>>(),
                "routeEnd": bits(result.route_end), "expansions": result.expansions, "status": status });
            assert_eq!(
                actual, query["expected"],
                "{name}, query {i}: {name_type}, total {queries}"
            );
            queries += 1;
        }
    }
    assert_eq!(
        (queries, found, unreachable, deferred, maps),
        (1175, 914, 163, 98, 33),
        "reference coverage changed"
    );
}

#[test]
fn invalid_inputs_and_unsupported_profiles_return_errors() {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/pathfinding.json")).unwrap();
    let mut kind = profile("rifle", &fixture["profiles"]["rifle"]);
    let flags = [0];
    let near = [0];
    let hazards = [false];
    let mut terrain = PathTerrain {
        navigation: TerrainView {
            width: 1,
            height: 1,
            flags: &flags,
            heights: None,
            world_known: true,
            knowledge_id: 1,
            versions: Versions::default(),
            changed: None,
            changes: &[],
        },
        near_walls: &near,
        known_mines: &hazards,
        fires: &hazards,
        bunkers: &[],
    };
    let rules = PathRules {
        wall_hug: 0.5,
        bunker_radius: 2.0,
    };
    let from = Point { x: 1.0, z: 1.0 };
    let mut finder = Pathfinding::new();
    for unsupported in 0..4 {
        kind.infantry = unsupported != 0;
        kind.air = unsupported == 1;
        kind.rail = unsupported == 2;
        kind.structure = unsupported == 3;
        assert_eq!(
            finder
                .find_path(terrain, rules, &kind, from, from, None)
                .unwrap_err(),
            PathError::UnsupportedProfile
        );
    }
    kind = profile("rifle", &fixture["profiles"]["rifle"]);
    assert_eq!(
        finder
            .find_path(
                terrain,
                rules,
                &kind,
                Point {
                    x: f64::NAN,
                    z: 1.0
                },
                from,
                None
            )
            .unwrap_err(),
        PathError::InvalidPosition
    );
    assert_eq!(
        finder
            .find_path(
                terrain,
                rules,
                &kind,
                Point { x: 1e300, z: 1.0 },
                from,
                None
            )
            .unwrap_err(),
        PathError::InvalidPosition
    );
    assert_eq!(
        finder
            .find_path(
                terrain,
                PathRules {
                    wall_hug: f64::NAN,
                    ..rules
                },
                &kind,
                from,
                from,
                None
            )
            .unwrap_err(),
        PathError::InvalidRules
    );
    terrain.near_walls = &[];
    assert_eq!(
        finder
            .find_path(terrain, rules, &kind, from, from, None)
            .unwrap_err(),
        PathError::InvalidShape
    );
}
