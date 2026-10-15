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
    assert_eq!(fixture["scenarios"].as_array().unwrap().len(), 24);
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
    replay_paths(&fixture, (603, 487, 36, 80, 0));
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

#[test]
fn authored_spawn_reachability_matches_original_selected_check() {
    use ww2_core::{game::CELL, map::AuthoritativeMap, world::World};

    let fixture_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/map-reachability.json");
    let fixture_bytes = std::fs::read(&fixture_path).unwrap_or_else(|error| {
        panic!(
            "MAP-105 requires the authorized original-Source capture at {}: {error}; no fixture was fabricated",
            fixture_path.display()
        )
    });
    let fixture: Value = serde_json::from_slice(&fixture_bytes).unwrap();
    assert_eq!(fixture["schemaVersion"], 1);
    assert_eq!(fixture["reference"], "e4e579a838aabae3b1d9dadbc17a0a87147bc52d");
    assert_eq!(fixture["node"], "v24.21.0");
    assert_eq!(fixture["arch"], "x64");
    assert_eq!(fixture["producer"], "original-createGame-clearFixtureUnits-findPath");
    assert_eq!(fixture["selectedCheck"], "test.js:3881-3891");
    assert_eq!(fixture["expansionLimit"], "Source default min(cells,16384)");
    assert_eq!(fixture["profile"], serde_json::json!({
        "inputFields": ["x", "z"], "definitionAbsent": true,
        "block": ww2_core::game::MOVE, "naval": false, "vehicle": false,
        "pull": ww2_core::game::MOVE | ww2_core::game::WIRE
    }));

    // This catalog fixture independently fixes all shipped map names and inputs.
    let authored: Value = serde_json::from_str(include_str!(
        "../../core/tests/fixtures/authoritative-maps.json"
    )).unwrap();
    assert_eq!(authored["reference"], fixture["reference"]);
    let maps: Vec<_> = authored["cases"].as_array().unwrap().iter()
        .filter(|case| case["name"].as_str().unwrap().starts_with("authored:"))
        .collect();
    assert_eq!(maps.len(), 33);
    assert_eq!(fixture["maps"], serde_json::json!(maps.iter().map(|case| {
        case["name"].as_str().unwrap().strip_prefix("authored:").unwrap()
    }).collect::<Vec<_>>()));
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), maps.len());

    // Source passes plain {x,z}, so UNITS[from.type] is absent. This synthetic
    // profile selects MOVE, non-naval, non-vehicle and MOVE|WIRE string pulling.
    // It is not an owned rifle query and never selects a recipient terrain view.
    let kind = profile("untyped-source-query", &serde_json::json!({
        "infantry": true, "speed": 0.0, "radius": 0.0, "naval": false, "deep": false
    }));
    assert_eq!(kind.block(), ww2_core::game::MOVE);
    assert!(!kind.ground_vehicle());
    let rules = PathRules::compiled().unwrap();
    let center = |x: usize, y: usize| Point {
        x: (x as f64 + 0.5) * CELL,
        z: (y as f64 + 0.5) * CELL,
    };
    let mut query_count = 0;
    for (case, authored) in cases.iter().zip(maps) {
        let filename = case["filename"].as_str().unwrap();
        assert_eq!(format!("authored:{filename}"), authored["name"].as_str().unwrap());
        assert_eq!(case["map"], authored["map"], "{filename}: original authored input");
        assert_eq!(case["initializationRandom"]["constant"], 0.5);
        assert!(case["initializationRandom"]["draws"].as_u64().unwrap() > 0);
        let map = AuthoritativeMap::from_json(&serde_json::to_vec(&case["map"]).unwrap())
            .unwrap_or_else(|error| panic!("{filename}: map validation: {error}"));
        let world = World::from_map(&map).unwrap();
        let game = &world.game;
        let n = game.w * game.h;
        let expected_terrain = &case["terrain"];
        for (key, native) in [
            ("chars", game.chars.as_slice()),
            ("ground", game.ground.as_deref().unwrap()),
            ("objects", game.objects.as_deref().unwrap()),
        ] {
            assert_eq!(native, expected_terrain[key].as_str().unwrap().as_bytes(),
                "{filename}: restored Source {key} differs from native authored World");
        }
        for (key, native) in [
            ("flags", game.flags.iter().map(|v| i64::from(*v)).collect::<Vec<_>>()),
            ("house", game.house.iter().map(|v| i64::from(*v)).collect()),
            ("hp32", game.cell_hp.iter().map(|v| i64::from(v.to_bits())).collect()),
            ("mines", world.mine_layer.iter().map(|v| i64::from(*v)).collect()),
            ("wear32", game.wear.as_ref().unwrap().iter()
                .map(|v| i64::from(v.to_bits())).collect()),
        ] {
            assert_eq!(native, runs(&expected_terrain[key]),
                "{filename}: restored Source {key} differs from native authored World");
        }
        if let Some(native) = &game.height {
            assert_eq!(native.iter().map(|v| i64::from(*v)).collect::<Vec<_>>(),
                runs(&expected_terrain["height"]), "{filename}: authored heights");
        } else {
            // clearFixtureUnits restores each real building cell with height 0
            // on flat maps, allocating Source heights. Native keeps None; both
            // navigation::TerrainView::level and pathfinding::level read 0.
            assert!(case["map"]["heights"].is_null(), "{filename}: absent authored heights");
            assert!(case["removed"].as_array().unwrap().iter().any(|unit| {
                !unit["cells"].as_array().unwrap().is_empty()
            }), "{filename}: actual Source footprint restoration");
            assert_eq!(runs(&expected_terrain["height"]), vec![0; n],
                "{filename}: Source allocated flat heights must be entirely zero");
        }
        assert_eq!(world.roads, expected_terrain["roads"].as_bool().unwrap());
        assert!(game.units.is_empty());
        assert!(world.structures.iter().all(|structure| structure.kind != "building"));
        for key in ["units", "buildingCells", "buildingStructures"] {
            assert_eq!(expected_terrain[key], 0, "{filename}: Source {key}");
        }
        let near = authored_reachability_near_walls(game.w, &game.flags);
        assert_eq!(near.iter().map(|v| i64::from(*v)).collect::<Vec<_>>(),
            runs(&expected_terrain["near"]), "{filename}: near-wall inputs");
        let fires: Vec<_> = (0..n).map(|cell| world.fires.contains_key(&cell)).collect();
        assert_eq!(world.fires.iter().map(|(cell, _)| *cell).collect::<Vec<_>>(),
            expected_terrain["fires"].as_array().unwrap().iter()
                .map(|cell| cell.as_u64().unwrap() as usize).collect::<Vec<_>>());
        // No owner was supplied to Source: authored mines are never known mines.
        let known_mines = vec![false; n];
        // Share the observed Source weather scalar as a component input. This
        // does not compare Match weather or infer World::from_map weather.
        // Both searches use wetness only behind their vehicle-cost guards.
        let wetness = expected_terrain["wetness"].as_f64().unwrap();
        assert!(wetness.is_finite() && (0.0..=1.0).contains(&wetness));
        let terrain = PathTerrain {
            navigation: TerrainView {
                width: game.w, height: game.h, flags: &game.flags,
                heights: game.height.as_deref(), world_known: false, knowledge_id: 0,
                versions: Versions::default(), changed: None, changes: &[],
            },
            near_walls: &near, known_mines: &known_mines, fires: &fires,
            bunkers: &[], wear: game.wear.as_deref(),
            wetness, roads: world.roads,
        };
        // Enumerate actual parsed map starts and points, independently of the
        // recorded query list. Naval starts require an existential per spawn.
        let naval = case["map"]["naval"].as_bool().unwrap_or(false);
        let mut required = Vec::new();
        for (spawn, from) in map.spawns().iter().enumerate() {
            for (target, to) in map.points().iter().enumerate() {
                required.push((spawn, "point", target, center(from.x, from.y), center(to.x, to.y)));
            }
            if !naval {
                for (target, to) in map.spawns().iter().enumerate() {
                    if target != spawn {
                        required.push((spawn, "spawn", target, center(from.x, from.y), center(to.x, to.y)));
                    }
                }
            }
        }
        let recorded = case["queries"].as_array().unwrap();
        assert_eq!(recorded.len(), required.len(), "{filename}: exhaustive query coverage");
        let mut reached = vec![false; map.spawns().len()];
        let mut source_reached = vec![false; map.spawns().len()];
        let mut finder = Pathfinding::new();
        for (query, (spawn, target_kind, target, from, to)) in recorded.iter().zip(required) {
            assert_eq!(usize_field(query, "spawn"), spawn, "{filename}: spawn coverage");
            assert_eq!(query["targetKind"], target_kind);
            assert_eq!(usize_field(query, "target"), target);
            assert_eq!(bits(point(&query["from"])), bits(from));
            assert_eq!(bits(point(&query["to"])), bits(to));
            for key in ["from", "to"] {
                assert_eq!(query[key].as_object().unwrap().keys().map(String::as_str)
                    .collect::<Vec<_>>(), ["x", "z"], "{filename}: plain Source point");
            }
            let result = finder.find_path(terrain, rules, &kind, from, to, None).unwrap();
            let status = match result.status {
                PathStatus::Found => "Found",
                PathStatus::Unreachable => "Unreachable",
                PathStatus::ExpansionLimit => "ExpansionLimit",
            };
            assert_eq!(serde_json::json!({
                "points": result.waypoints.iter().copied().map(bits).collect::<Vec<_>>(),
                "status": status,
            }), query["expected"], "{filename}: spawn {spawn} to {target_kind} {target}");
            let native_found = !result.waypoints.is_empty();
            let source_found = !query["expected"]["points"].as_array().unwrap().is_empty();
            if naval {
                reached[spawn] |= native_found;
                source_reached[spawn] |= source_found;
            } else {
                assert!(native_found && source_found,
                    "{filename}: spawn {spawn} must reach {target_kind} {target}");
            }
            query_count += 1;
        }
        if naval {
            for spawn in 0..map.spawns().len() {
                assert!(reached[spawn] && source_reached[spawn],
                    "{filename}: naval spawn {spawn} must reach some capture point");
            }
        }
    }
    assert_eq!(query_count, usize_field(&fixture, "queryCount"));
}

fn authored_reachability_near_walls(width: usize, flags: &[u16]) -> Vec<u8> {
    let mut near = vec![0; flags.len()];
    for (cell, flag) in flags.iter().enumerate() {
        if flag & 3 != 3 {
            continue;
        }
        let x = cell % width;
        for dy in [-1, 0, 1] {
            for dx in (if x > 0 { -1 } else { 0 })..=(if x + 1 < width { 1 } else { 0 }) {
                let neighbor = cell as isize + dy * width as isize + dx;
                if neighbor >= 0 && neighbor < near.len() as isize {
                    near[neighbor as usize] = 1;
                }
            }
        }
    }
    near
}
