//! Compare the public path, resolved goal and search outcome with original JS.
use serde_json::Value;
use ww2_core::game::Point;
use ww2_core::navigation::{TerrainChange, TerrainView, Versions};
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
fn infantry_naval_and_vehicle_paths_match_original_find_path() {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/pathfinding.json")).unwrap();
    replay_paths(&fixture, (1529, 1108, 299, 122, 33));
}

#[test]
fn named_fine_search_branches_match_original_find_path() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/pathfinding-branches.json")).unwrap();
    assert_eq!(fixture["scenarios"].as_array().unwrap().len(), 25);
    let covered: std::collections::BTreeSet<_> = fixture["scenarios"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|case| case["rows"].as_array().unwrap().iter())
        .map(|row| row.as_str().unwrap())
        .collect();
    assert_eq!(
        covered,
        [
            "MOV-31", "MOV-32", "MOV-33", "MOV-34", "MOV-35", "MOV-38", "MOV-39"
        ]
        .into_iter()
        .collect()
    );
    replay_paths(&fixture, (606, 489, 37, 80, 0));
}

fn replay_paths(fixture: &Value, expected_counts: (usize, usize, usize, usize, usize)) {
    assert_eq!(
        fixture["reference"],
        "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
    );
    assert_eq!(fixture["node"], "v24.21.0");
    assert_eq!(fixture["arch"], "x64");
    let rules = PathRules {
        wall_hug: fixture["rules"]["wallHug"].as_f64().unwrap(),
        bunker_radius: fixture["rules"]["bunkerRadius"].as_f64().unwrap(),
        road_speed: fixture["rules"]["roadSpeed"].as_f64().unwrap(),
        ford_speed: serde_json::from_value(fixture["rules"]["fordSpeed"].clone()).unwrap(),
        mud_speed: serde_json::from_value(fixture["rules"]["mudSpeed"].clone()).unwrap(),
        wood_speed: fixture["rules"]["woodSpeed"].as_f64().unwrap(),
        churn: fixture["rules"]["churn"].as_f64().unwrap(),
        wet_ford: fixture["rules"]["wetFord"].as_f64().unwrap(),
        wet_ground: fixture["rules"]["wetGround"].as_f64().unwrap(),
    };
    let compiled = PathRules::compiled().unwrap();
    let values = |r: PathRules| {
        [
            r.wall_hug,
            r.bunker_radius,
            r.road_speed,
            r.wood_speed,
            r.ford_speed[0],
            r.ford_speed[1],
            r.mud_speed[0],
            r.mud_speed[1],
            r.churn,
            r.wet_ford,
            r.wet_ground,
        ]
        .map(f64::to_bits)
    };
    assert_eq!(
        values(compiled),
        values(rules),
        "compiled costs must match the original-JavaScript oracle"
    );
    let rules = compiled;
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
        let mut flags: Vec<_> = runs(&scenario["flags"])
            .into_iter()
            .map(|v| v as u16)
            .collect();
        let mut heights: Option<Vec<_>> = (!scenario["height"].is_null()).then(|| {
            runs(&scenario["height"])
                .into_iter()
                .map(|v| v as i8)
                .collect()
        });
        let mut near: Vec<_> = runs(&scenario["near"])
            .into_iter()
            .map(|v| v as u8)
            .collect();
        let mut wear: Option<Vec<f32>> = (!scenario["wear"].is_null()).then(|| {
            scenario["wear"]
                .as_array()
                .unwrap()
                .iter()
                .flat_map(|r| {
                    std::iter::repeat_n(
                        r[0].as_f64().unwrap() as f32,
                        r[1].as_u64().unwrap() as usize,
                    )
                })
                .collect()
        });
        let mut wetness = scenario["wetness"].as_f64().unwrap();
        let mut version = 0;
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
            let update = &query["update"];
            if update["replace"].as_bool() == Some(true) {
                flags = flags.clone();
            }
            for edit in update["flags"].as_array().into_iter().flatten() {
                flags[edit[0].as_u64().unwrap() as usize] = edit[1].as_u64().unwrap() as u16;
            }
            for edit in update["heights"].as_array().into_iter().flatten() {
                heights.as_mut().unwrap()[edit[0].as_u64().unwrap() as usize] =
                    edit[1].as_i64().unwrap() as i8;
            }
            for edit in update["wear"].as_array().into_iter().flatten() {
                wear.as_mut().unwrap()[edit[0].as_u64().unwrap() as usize] =
                    edit[1].as_f64().unwrap() as f32;
            }
            if let Some(v) = update["wetness"].as_f64() {
                wetness = v;
            }
            if let Some(v) = update["version"].as_u64() {
                version = v;
            }
            if !update.is_null() {
                near.fill(0);
                for (c, flag) in flags.iter().enumerate() {
                    if flag & 3 != 3 {
                        continue;
                    }
                    let x = c % width;
                    for dy in [-1, 0, 1] {
                        for dx in (if x > 0 { -1 } else { 0 })..=(if x < width - 1 { 1 } else { 0 })
                        {
                            let n = c as isize + dy * width as isize + dx;
                            if n >= 0 && n < near.len() as isize {
                                near[n as usize] = 1;
                            }
                        }
                    }
                }
            }
            let changed: Vec<usize> = update["changed"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|v| v.as_u64().unwrap() as usize)
                .collect();
            let query_terrain = PathTerrain {
                navigation: TerrainView {
                    width,
                    height,
                    flags: &flags,
                    heights: heights.as_deref(),
                    world_known: scenario["worldKnown"].as_bool().unwrap(),
                    knowledge_id: 1,
                    versions: Versions {
                        vehicle: version,
                        knowledge: version,
                        ..Versions::default()
                    },
                    changed: update["from"].as_u64().map(|from| TerrainChange {
                        from,
                        to: version,
                        cells: &changed,
                    }),
                    changes: &[],
                },
                near_walls: &near,
                known_mines: &query_mines,
                fires: &query_fires,
                bunkers: &query_bunkers,
                wear: wear.as_deref(),
                wetness,
                roads: scenario["roads"].as_bool().unwrap(),
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
            let mut actual = serde_json::json!({ "points": result.waypoints.iter().copied().map(bits).collect::<Vec<_>>(),
                "routeEnd": bits(result.route_end), "expansions": result.expansions, "status": status });
            if query["expected"].get("localGoal").is_some() {
                actual["localGoal"] = serde_json::json!(result.local_goal);
                actual["limited"] = serde_json::json!(result.limited);
            }
            assert_eq!(
                actual, query["expected"],
                "{name}, query {i}: {name_type}, total {queries}"
            );
            queries += 1;
        }
    }
    assert_eq!(
        (queries, found, unreachable, deferred, maps),
        expected_counts,
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
        wear: None,
        wetness: 0.0,
        roads: false,
    };
    let rules = PathRules {
        wall_hug: 0.5,
        bunker_radius: 2.0,
        road_speed: 1.5,
        ford_speed: [0.8, 0.5],
        mud_speed: [0.5, 0.3],
        wood_speed: 0.4,
        churn: 0.3,
        wet_ford: 0.3,
        wet_ground: 0.3,
    };
    let from = Point { x: 1.0, z: 1.0 };
    let mut finder = Pathfinding::new();
    for unsupported in 1..4 {
        kind.infantry = true;
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
