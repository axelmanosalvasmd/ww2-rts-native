//! Complete cheap MAP component checks using the maintained terrain and movement providers.
//! Inputs come from the retained map corpus and original test.js scenes. No recorded
//! output initializes native state. This module has no whole-match or delivery claim.

use serde_json::{Value, json};
use ww2_core::{
    catalog::Catalog,
    game::{Game, LAND, MOVE, Player, Point, Unit},
    grid::SpatialGrid,
    map::AuthoritativeMap,
    movement::move_unit,
    navigation::{TerrainView, Versions},
    pathfinding::{PathRules, PathTerrain, Pathfinding},
    rules::Rules,
    terrain::{ground_mul, speed_mul},
    unit_spawn::initial_unit,
    vision::los,
    world::World,
};

fn original_map(name: &str) -> Value {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../core/tests/fixtures/authoritative-maps.json"
    ))
    .unwrap();
    assert_eq!(
        fixture["reference"],
        "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
    );
    let cases: Vec<_> = fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|case| case["name"] == name)
        .collect();
    assert_eq!(cases.len(), 1);
    assert!(cases[0]["error"].is_null() && cases[0]["unsupported"].is_null());
    cases[0]["map"].clone()
}

fn world(map: &Value) -> World {
    World::from_map(&AuthoritativeMap::from_json(&serde_json::to_vec(map).unwrap()).unwrap())
        .unwrap()
}

fn actor(rules: &Rules, name: &str, id: u32, x: f64, z: f64) -> Unit {
    initial_unit(
        rules,
        Catalog::compiled().unwrap(),
        rules.kind(name).unwrap(),
        id,
        0,
        Point { x, z },
    )
    .unwrap()
}

fn exact(actual: f64, expected: f64, context: &str) {
    assert_eq!(
        actual.to_bits(),
        expected.to_bits(),
        "{context}: {actual} != {expected}"
    );
}

#[test]
fn map25_map47_every_character_wear_and_wetness_reach_ground_mul() {
    // Source TERRAIN: all 19 letters, including runtime-only K/A. Expected dry
    // values are the pinned constants at wear 0, 0.5 and 1, not a second flag decoder.
    let rows = [
        ('.', [1.0, 0.85, 0.7], [1.0; 3], true),
        ('B', [1.0, 0.85, 0.7], [1.0; 3], true),
        ('H', [1.0, 0.85, 0.7], [1.0; 3], true),
        ('#', [1.0, 0.85, 0.7], [1.0; 3], true),
        ('+', [1.0, 0.85, 0.7], [1.0; 3], true),
        ('T', [1.0, 0.85, 0.7], [1.2; 3], true),
        ('W', [1.0, 0.85, 0.7], [1.0; 3], true),
        ('F', [0.75, 0.55, 0.35], [0.75, 0.55, 0.35], false),
        ('=', [1.35, 1.175, 1.0], [1.0; 3], false),
        ('R', [1.0, 0.85, 0.7], [1.0; 3], true),
        ('K', [1.0, 0.85, 0.7], [1.0; 3], true),
        ('X', [1.0, 0.85, 0.7], [0.35; 3], true),
        ('Y', [1.0, 0.85, 0.7], [1.0; 3], true),
        ('D', [1.35, 1.175, 1.0], [1.0; 3], false),
        (
            'M',
            [0.7, f64::from_bits(0x3fe0_cccc_cccc_cccc), 0.35],
            [1.0; 3],
            true,
        ),
        ('N', [1.0, 0.85, 0.7], [1.0; 3], true),
        ('O', [0.5; 3], [1.0; 3], false),
        ('A', [1.0, 0.85, 0.7], [1.0; 3], true),
        ('Q', [1.0, 0.85, 0.7], [1.0; 3], true),
    ];
    let rules = Rules::compiled().unwrap();
    let input = original_map("boundary:minimum");
    let mut comparisons = 0;
    for (ch, vehicle, infantry, wet_vehicle) in rows {
        let mut w = world(&input);
        let cell = 5 * w.game.w + 5;
        w.set_cell(cell, ch as u8).unwrap();
        if ch == '.' {
            assert_eq!(w.game.flags[cell], LAND, "open ground has only LAND");
        }
        for level in [0, -1] {
            // Authored negative-height cells use the production doubled-wetness path.
            w.game.height = Some(vec![level; w.game.w * w.game.h]);
            for (index, wear) in [0.0, 0.5, 1.0].into_iter().enumerate() {
                w.game.wear.as_mut().unwrap()[cell] = wear;
                for wet in [0.0, 1.0] {
                    w.game.wet = Some(wet);
                    let wet_factor = if wet == 0.0 {
                        1.0
                    } else if level < 0 {
                        0.6
                    } else {
                        0.8
                    };
                    for (is_vehicle, dry) in [(false, infantry[index]), (true, vehicle[index])] {
                        let factor = if ch == 'F' && wet != 0.0 {
                            0.7
                        } else if is_vehicle && wet_vehicle {
                            wet_factor
                        } else {
                            1.0
                        };
                        exact(
                            ground_mul(&w.game, &rules, cell, is_vehicle),
                            dry * factor,
                            &format!(
                                "{ch}, wear={wear}, wet={wet}, level={level}, vehicle={is_vehicle}"
                            ),
                        );
                        comparisons += 1;
                    }
                }
            }
        }
    }
    assert_eq!(comparisons, 456);
}

#[test]
fn map47_five_samples_use_radius_passability_and_empty_fallback() {
    let rules = Rules::compiled().unwrap();
    let input = original_map("boundary:minimum");
    let mut w = world(&input);
    w.game.wet = None;
    let tank = actor(&rules, "tank", 1, 19.0, 19.0);
    for (cell, ch) in [(190, b'D'), (188, b'M'), (209, b'F'), (169, b'O')] {
        w.set_cell(cell, ch).unwrap();
    }
    // World mutation seeds natural surface wear. This component's authored
    // input is the same zero-wear matrix supplied to the Source checker.
    w.game.wear.as_mut().unwrap().fill(0.0);
    exact(
        speed_mul(&w.game, &rules, &tank),
        (1.0 + 1.35 + 0.7 + 0.75 + 0.5) / 5.0,
        "five distinct cells in FOOT order",
    );
    w.set_cell(190, b'B').unwrap();
    exact(
        speed_mul(&w.game, &rules, &tank),
        (1.0 + 0.7 + 0.75 + 0.5) / 4.0,
        "a wall beside the hull does not count",
    );
    w.set_cell(188, b'W').unwrap();
    exact(
        speed_mul(&w.game, &rules, &tank),
        (1.0 + 0.75 + 0.5) / 3.0,
        "water beside the hull does not count",
    );
    for cell in [189, 209, 169] {
        w.set_cell(cell, b'B').unwrap();
    }
    exact(
        speed_mul(&w.game, &rules, &tank),
        1.0,
        "no passable samples",
    );
    let boat = actor(&rules, "gunboat", 2, 19.0, 19.0);
    exact(speed_mul(&w.game, &rules, &boat), 1.0, "naval bypass");

    let mut edge = world(&input);
    edge.game.wet = None;
    for (cell, ch) in [(0, b'D'), (1, b'M'), (20, b'O')] {
        edge.set_cell(cell, ch).unwrap();
    }
    edge.game.wear.as_mut().unwrap().fill(0.0);
    let tank = actor(&rules, "tank", 1, 0.5, 0.5);
    exact(
        speed_mul(&edge.game, &rules, &tank),
        (1.35 + 0.7 + 0.5) / 3.0,
        "off-map west and north samples do not count",
    );

    let mut radius = world(&input);
    radius.game.wear.as_mut().unwrap().fill(0.0);
    radius.game.wet = None;
    radius.set_cell(190, b'D').unwrap();
    let tank = actor(&rules, "tank", 1, 19.8, 19.0);
    exact(
        speed_mul(&radius.game, &rules, &tank),
        (1.0 + 1.35 + 1.0 + 1.0 + 1.0) / 5.0,
        "0.6 radius reaches column 10, full radius would reach column 11",
    );

    let mut infantry = world(&input);
    infantry.game.wet = None;
    for (cell, ch) in [(189, b'F'), (190, b'X'), (209, b'T')] {
        infantry.set_cell(cell, ch).unwrap();
    }
    infantry.game.wear.as_mut().unwrap().fill(0.0);
    let rifle = actor(&rules, "rifle", 1, 19.5, 19.5);
    exact(
        speed_mul(&infantry.game, &rules, &rifle),
        (0.75 + 0.35 + 0.75 + 1.2 + 0.75) / 5.0,
        "infantry uses its own ground rules and counts repeated centre-cell samples",
    );

    let mut rubble = world(&input);
    rubble.game.wet = None;
    rubble.set_cell(190, b'R').unwrap();
    rubble.game.wear.as_mut().unwrap().fill(0.0);
    rubble.game.wear.as_mut().unwrap()[190] = 1.0;
    let tank = actor(&rules, "tank", 1, 19.0, 19.0);
    exact(
        speed_mul(&rubble.game, &rules, &tank),
        (1.0 + 0.7 + 1.0 + 1.0 + 1.0) / 5.0,
        "the Source footprint filter excludes MOVE, not the vehicle blocker mask",
    );
}

fn original_scene_input(rows: Vec<String>, heights: Option<Vec<String>>) -> Value {
    // The exact 20x20 blank helper shared by test.js's selected checks. A name
    // supplies only native map admission metadata; no accepted terrain changes.
    let mut map = json!({"name":"Original MAP component", "w":20, "h":20, "rows":rows,
        "spawns":[{"x":1,"y":1},{"x":18,"y":1},{"x":1,"y":18}],
        "points":[{"x":10,"y":10}]});
    if let Some(heights) = heights {
        map["heights"] = json!(heights);
    }
    map
}

fn original_scene(rows: Vec<String>, heights: Option<Vec<String>>) -> World {
    world(&original_scene_input(rows, heights))
}

fn empty() -> Vec<String> {
    vec![".".repeat(20); 20]
}

fn path(g: &Game, rules: &Rules, kind: &str, from: Point, to: Point) -> Vec<Point> {
    let n = g.w * g.h;
    let near = vec![0; n];
    let hazards = vec![false; n];
    Pathfinding::new()
        .find_path(
            PathTerrain {
                navigation: TerrainView {
                    width: g.w,
                    height: g.h,
                    flags: &g.flags,
                    heights: g.height.as_deref(),
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
                wear: g.wear.as_deref(),
                wetness: g.wet.unwrap_or(0.0),
                roads: false,
            },
            PathRules::compiled().unwrap(),
            &rules.kinds[rules.kind(kind).unwrap()],
            from,
            to,
            None,
        )
        .unwrap()
        .waypoints
}

#[test]
fn map31_original_water_sight_assertion_uses_full_los() {
    let rows = (0..20)
        .map(|y| {
            format!(
                "{}{}{}",
                ".".repeat(10),
                if y == 5 {
                    '='
                } else if y == 15 {
                    'F'
                } else {
                    'W'
                },
                ".".repeat(9)
            )
        })
        .collect();
    // The original blank objective at (10, 10) overlaps this authored river.
    // Author a neutral, zero-income point on known open ground before admission.
    let mut input = original_scene_input(rows, None);
    input["points"] = json!([{"x":2,"y":2,"vp":0,"mp":0}]);
    let w = world(&input);
    let rules = Rules::compiled().unwrap();
    assert_eq!(
        w.game.flags[12 * 20 + 10],
        MOVE,
        "water has no LAND or SIGHT"
    );
    assert!(
        los(&w.game, &rules, (5.0, 25.0), (35.0, 25.0)),
        "you can see across water"
    );
}

#[test]
fn map50_all_original_los_ridge_and_depression_assertions() {
    let rules = Rules::compiled().unwrap();
    let mut rows = empty();
    rows[5] = ".....B..............".into();
    rows[7] = ".....#..............".into();
    let g = original_scene(rows, None).game;
    assert!(
        !los(&g, &rules, (1.0, 11.0), (21.0, 11.0)),
        "building blocks LOS"
    );
    assert!(
        los(&g, &rules, (1.0, 15.0), (21.0, 15.0)),
        "wall does not block LOS"
    );
    let ridge = original_scene(empty(), Some(vec!["00000000022000000000".into(); 20])).game;
    assert!(
        !los(&ridge, &rules, (5.0, 20.0), (35.0, 20.0)),
        "ridge between valleys"
    );
    let high = original_scene(empty(), Some(vec!["33330220000000000000".into(); 20])).game;
    assert!(
        los(&high, &rules, (3.0, 20.0), (35.0, 20.0)),
        "higher ground sees over ridge"
    );
    assert!(
        !los(&high, &rules, (9.0, 20.0), (35.0, 20.0)),
        "valley floor cannot"
    );
    let gully = original_scene(empty(), Some(vec!["00000000bbbb00000000".into(); 20])).game;
    assert!(
        !los(&gully, &rules, (5.0, 20.0), (21.0, 20.0)),
        "gully floor hidden beside it"
    );
    assert!(
        los(&gully, &rules, (18.0, 20.0), (22.0, 20.0)),
        "visible from inside"
    );
    assert!(
        path(
            &gully,
            &rules,
            "rifle",
            Point { x: 5.0, z: 5.0 },
            Point { x: 21.0, z: 5.0 },
        )
        .is_empty(),
        "the complete Depressions check also refuses a two-level cliff path"
    );
    // Keep the separate authentic 18-ray fixture as the exact grid traversal,
    // endpoint, woods, smoke, hill and raised-eye gate, rather than replacing it.
}

#[test]
fn map48_complete_original_slope_journey_uses_production_movement() {
    let rules = Rules::compiled().unwrap();
    let mut hill = original_scene(empty(), Some(vec!["00000000001111111111".into(); 20])).game;
    let mut flat = original_scene(empty(), None).game;
    for g in [&mut hill, &mut flat] {
        g.players = vec![
            Player {
                slot: 0,
                team: 0,
                visible: vec![],
            },
            Player {
                slot: 1,
                team: 1,
                visible: vec![],
            },
        ];
        for (id, x, z, target, rotation) in [
            (1, 15.0, 21.0, 37.0, 0.0),
            (2, 25.0, 29.0, 3.0, std::f64::consts::PI),
        ] {
            let mut u = actor(&rules, "tank", id, x, z);
            u.rot = rotation;
            u.path = path(g, &rules, "tank", u.at(), Point { x: target, z });
            assert!(!u.path.is_empty());
            g.units.insert(id, u);
        }
    }
    let mut hill_grid = SpatialGrid::new(&hill.units, 16.0);
    let mut flat_grid = SpatialGrid::new(&flat.units, 16.0);
    let mut calls = 0;
    // The original check runs two seconds on each scene. Native state feeds all
    // 40 steps; no Source snapshots, resets, random values or HP edits intervene.
    for _ in 0..40 {
        for (g, grid) in [(&mut hill, &mut hill_grid), (&mut flat, &mut flat_grid)] {
            for id in [1, 2] {
                move_unit(g, &rules, grid, id, 1.0).expect("original tank remains admitted");
                calls += 1;
            }
            g.tick += 1;
        }
    }
    assert_eq!(calls, 160);
    assert!(
        hill.units[&1].x < flat.units[&1].x - 0.5,
        "a tank climbs slower than it drives on the flat"
    );
    assert!(
        (hill.units[&2].x - flat.units[&2].x).abs() < 0.3,
        "and loses nothing going down"
    );

    // Decisive numeric samples at the same original one-level slope: the
    // bilinear grade is 0.625, 1.25 (clamped to 1), or -1.25 (clamped to 0).
    // Assert the speed returned by the actual block for both unit classes.
    for (name, partial, uphill) in [("rifle", 0.875, 0.8), ("tank", 0.71875, 0.55)] {
        for (x, goal, factor) in [
            (18.5, 37.0, partial),
            (19.0, 37.0, uphill),
            (21.0, 3.0, 1.0),
        ] {
            let mut g = original_scene(empty(), Some(vec!["00000000001111111111".into(); 20])).game;
            g.players = hill.players.clone();
            let mut u = actor(&rules, name, 1, x, 21.0);
            u.path = path(&g, &rules, name, u.at(), Point { x: goal, z: 21.0 });
            assert!(!u.path.is_empty());
            let expected = rules.kinds[u.kind].speed * factor;
            g.units.insert(1, u);
            let mut grid = SpatialGrid::new(&g.units, 16.0);
            let moved = move_unit(&mut g, &rules, &mut grid, 1, 1.0).unwrap();
            exact(moved.speed, expected, &format!("{name} at x={x}"));
        }
    }
}

#[test]
fn map49_complete_original_cliff_and_depression_checks() {
    let rules = Rules::compiled().unwrap();
    let from = Point { x: 5.0, z: 5.0 };
    let across = Point { x: 35.0, z: 5.0 };
    let heights: Vec<String> = (0..20)
        .map(|y| {
            if y < 15 {
                "00000000022000000000".into()
            } else {
                "0".repeat(20)
            }
        })
        .collect();
    let cliff = original_scene(empty(), Some(heights.clone())).game;
    let detour = path(&cliff, &rules, "rifle", from, across);
    assert!(
        !detour.is_empty() && detour.iter().any(|p| p.z > 15.0 * 2.0),
        "test.js:646: a two-level cliff forces the path below the raised wall"
    );
    let slope = original_scene(
        empty(),
        Some(heights.iter().map(|row| row.replace('2', "1")).collect()),
    )
    .game;
    let direct = path(&slope, &rules, "rifle", from, across);
    assert!(
        !direct.is_empty(),
        "the one-level control must find a route"
    );
    assert!(
        direct.iter().all(|p| p.z < 15.0 * 2.0),
        "test.js:646: a one-level slope may cross directly"
    );

    // The complete original Depressions body includes both sight predicates
    // and its path refusal. The earlier MAP50 test remains unchanged.
    let gully = original_scene(empty(), Some(vec!["00000000bbbb00000000".into(); 20])).game;
    assert!(
        !los(&gully, &rules, (5.0, 20.0), (21.0, 20.0)),
        "Depressions: gully floor is out of sight from beside it"
    );
    assert!(
        los(&gully, &rules, (18.0, 20.0), (22.0, 20.0)),
        "Depressions: the floor is visible from inside"
    );
    assert!(
        path(&gully, &rules, "rifle", from, Point { x: 21.0, z: 5.0 }).is_empty(),
        "Depressions: a two-level drop is a cliff"
    );

    // Production noCliffs/segmentCliffClear: level-zero diagonal endpoints
    // require both side cells to be within one level of the starting cell.
    let mut corners = 0;
    for (east, south, expected) in [
        ('0', '0', true),
        ('1', '1', true),
        ('2', '0', false),
        ('0', '2', false),
        ('b', '0', false),
        ('0', 'b', false),
        ('1', 'a', true),
        ('2', '2', false),
    ] {
        let mut heights = vec![vec!['0'; 20]; 20];
        heights[9][10] = east;
        heights[10][9] = south;
        let heights = heights
            .into_iter()
            .map(|row| row.into_iter().collect::<String>())
            .collect();
        let corner = original_scene(empty(), Some(heights)).game;
        assert_eq!(
            ww2_core::terrain::segment_cliff_clear(
                &corner,
                Point { x: 19.0, z: 19.0 },
                Point { x: 21.0, z: 21.0 },
            ),
            expected,
            "diagonal corner levels {east}/{south}"
        );
        corners += 1;
    }
    assert_eq!(corners, 8);
}

// MAP owned feature batch. Behavior tests call the shipping providers directly;
// retained fixtures remain the exact oracle for tables and pure grid queries.
#[test]
fn map01_cell_coordinates_centres_and_seeded_boundary_queries() {
    use ww2_core::game::CELL;
    let input = original_map("authored:default.json");
    let w = world(&input);
    assert_eq!(CELL, 2.0);
    let spawn = &input["spawns"][0];
    let x = spawn["x"].as_u64().unwrap() as f64;
    let y = spawn["y"].as_u64().unwrap() as f64;
    assert_eq!(((x + 0.5) * CELL, (y + 0.5) * CELL), (81.0, 13.0));
    for c in 0..w.game.w * w.game.h {
        assert_eq!(
            w.game.cell_of(
                (c % w.game.w) as f64 * 2.0 + 1.0,
                (c / w.game.w) as f64 * 2.0 + 1.0
            ),
            Some(c)
        );
    }
    // Integer numerators make the independent expected floor exact, including
    // negative subcell coordinates. A fixed LCG never uses wall-clock entropy.
    let mut seed = 0x2922_9094u32;
    for _ in 0..10_000 {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let nx = (seed % ((w.game.w as u32 + 4) * 512)) as i64 - 1024;
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let nz = (seed % ((w.game.h as u32 + 4) * 512)) as i64 - 1024;
        let (cx, cy) = (nx.div_euclid(512), nz.div_euclid(512));
        let expected = if cx < 0 || cy < 0 || cx >= w.game.w as i64 || cy >= w.game.h as i64 {
            -1
        } else {
            cy * w.game.w as i64 + cx
        };
        assert_eq!(
            w.game.cell_index(nx as f64 / 256.0, nz as f64 / 256.0),
            expected
        );
    }
}

#[test]
fn map03_off_map_flags_and_rays_block_all_four_edges() {
    use ww2_core::game::SIGHT;
    let rules = Rules::compiled().unwrap();
    let g = original_scene(empty(), None).game;
    for (x, z) in [(-0.001, 20.0), (40.0, 20.0), (20.0, -0.001), (20.0, 40.0)] {
        assert_eq!(g.cell_index(x, z), -1);
        assert_eq!(g.flags_at(x, z), MOVE | SIGHT);
    }
    for to in [(-5.0, 21.0), (45.0, 21.0), (21.0, -5.0), (21.0, 45.0)] {
        assert!(!los(&g, &rules, (21.0, 21.0), to), "off-map ray {to:?}");
    }
    assert!(los(&g, &rules, (1.0, 1.0), (39.0, 39.0)));
}

fn batch_put(map: &mut Value, field: &str, x: usize, y: usize, ch: u8) {
    let row = map[field][y].as_str().unwrap();
    let mut bytes = row.as_bytes().to_vec();
    bytes[x] = ch;
    map[field][y] = json!(String::from_utf8(bytes).unwrap());
}

#[test]
fn map16_layer_composition_material_round_trip_and_invalid_stacks() {
    let mut input = original_scene_input(empty(), None);
    input["worldVersion"] = json!(2);
    input["layers"] = json!({"ground":empty(), "objects":empty(), "mines":empty(),
        "groundMaterials":[{"c":168,"material":"concrete"}],
        "mineMaterials":[{"c":168,"material":"steel"}]});
    batch_put(&mut input["layers"], "ground", 8, 8, b'D');
    batch_put(&mut input["layers"], "mines", 8, 8, b'N');
    batch_put(&mut input, "rows", 8, 8, b'N');
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&input).unwrap()).unwrap();
    let round = AuthoritativeMap::from_json(&serde_json::to_vec(map.source()).unwrap()).unwrap();
    assert_eq!(map.source(), round.source());
    assert_eq!(map.layers().ground[168], b'D');
    assert_eq!(map.layers().mines[168], 1);
    assert_eq!(map.layers().base_materials[&168], "concrete");
    assert_eq!(map.layers().mine_materials[&168], "steel");
    assert_eq!(map.layers().materials[&168], "concrete");
    assert_eq!(World::from_map(&round).unwrap().game.physical(168), b'D');
    // A mine never replaces the physical material or ground flags.
    let mut hedge = input.clone();
    batch_put(&mut hedge["layers"], "objects", 8, 8, b'H');
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&hedge).unwrap()).unwrap();
    assert!(!map.layers().materials.contains_key(&168));
    assert_eq!(
        World::from_map(&map).unwrap().material_name(168, false),
        Some("wood")
    );
    for (ground, object, mine, composed, valid) in [
        (b'W', b'=', b'.', b'=', true),
        (b'.', b'=', b'.', b'=', false),
        (b'W', b'H', b'.', b'H', false),
        (b'W', b'.', b'N', b'N', false),
        (b'D', b'R', b'N', b'N', true),
        (b'D', b'R', b'N', b'R', false),
    ] {
        let mut m = input.clone();
        batch_put(&mut m["layers"], "ground", 8, 8, ground);
        batch_put(&mut m["layers"], "objects", 8, 8, object);
        batch_put(&mut m["layers"], "mines", 8, 8, mine);
        batch_put(&mut m, "rows", 8, 8, composed);
        assert_eq!(
            AuthoritativeMap::from_json(&serde_json::to_vec(&m).unwrap()).is_ok(),
            valid,
            "stack {ground}/{object}/{mine}/{composed}"
        );
    }
}

#[test]
fn map21_all_authored_moods_defaults_overrides_and_fog_lift() {
    use ww2_core::weather::{MapWeatherInfo, WeatherKind, WeatherRules, map_mood};
    let fixture: Value =
        serde_json::from_str(include_str!("../../core/tests/fixtures/weather.json")).unwrap();
    let rules = WeatherRules::compiled().unwrap();
    let mut checked = 0;
    for row in fixture["authored"].as_array().unwrap() {
        let name = row["name"].as_str().unwrap();
        let input = original_map(&format!("authored:{name}.json"));
        let info = MapWeatherInfo {
            name: input["name"].as_str(),
            mood: input["mood"].as_str(),
            weather: input["weather"].as_str(),
        };
        let expected = fixture["metadata"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["key"] == name)
            .unwrap();
        assert_eq!(
            map_mood(info, Some(name)).name(),
            expected["mood"].as_str().unwrap()
        );
        let mut plan = rules.map_weather(info, Some(name));
        assert_eq!(
            plan.now.name(),
            expected["expected"]["now"].as_str().unwrap()
        );
        if plan.now == WeatherKind::Fog {
            assert!(!plan.step(4_799));
            assert!(plan.step(4_800));
            assert_eq!(plan.now, WeatherKind::Clear);
            assert!(!plan.step(4_801));
        }
        checked += 1;
    }
    assert_eq!(checked, 33);
    let override_map = MapWeatherInfo {
        name: Some("winter"),
        mood: Some("dust"),
        weather: Some("rain"),
    };
    assert_eq!(map_mood(override_map, Some("ardennes")).name(), "dust");
    assert_eq!(rules.map_weather(override_map, None).now.name(), "rain");
}

#[test]
fn map27_map28_map35_map40_cover_flags_health_fire_and_destruction() {
    use ww2_core::{
        environment::{Context, EnvironmentRuntime},
        game::{COVER, SIGHT},
    };
    let rules = Rules::compiled().unwrap();
    for (ch, hp, blocks_sight, burn, into) in [
        (b'H', 60.0, true, Some(14.0), b'.'),
        (b'#', 150.0, false, None, b'+'),
        (b'X', 40.0, false, None, b'.'),
        (b'O', 200.0, false, Some(20.0), b'.'),
    ] {
        let mut rows = empty();
        let mut bytes = rows[8].as_bytes().to_vec();
        bytes[8] = ch;
        rows[8] = String::from_utf8(bytes).unwrap();
        let mut w = original_scene(rows, None);
        let c = 168;
        assert_eq!(w.game.cell_hp[c], hp);
        assert_eq!(w.game.flags[c] & SIGHT != 0, blocks_sight);
        assert_eq!(w.game.flags[c] & MOVE, 0);
        if ch != b'X' {
            assert_ne!(w.game.flags[c] & COVER, 0);
        }
        let mut env = EnvironmentRuntime::default();
        let grid = SpatialGrid::new(&w.game.units, 16.0);
        let (mut seed, mut next_id) = (123456789, 1000);
        let mut ctx = Context {
            tick: 0,
            seed: &mut seed,
            next_id: &mut next_id,
            unit_order: &[],
            grid: &grid,
            rain: 0.0,
            wet: 0.0,
            wind_angle: 0.0,
            wind_strength: 0.0,
        };
        if let Some(duration) = burn {
            env.ignite(&mut w, c, &mut ctx).unwrap();
            assert_eq!(w.fires.get(&c).copied(), Some(duration));
        }
        env.wreck_cell(&mut w, c, None, &mut ctx).unwrap();
        assert_eq!(w.game.physical(c), into);
        assert_eq!(w.game.flags[c] & (MOVE | SIGHT), 0);
        assert_eq!(ww2_core::terrain::max_hp(&w.game, &rules, c), 0.0);
    }
}

#[test]
fn map29_crater_start_depth_quality_clamp_and_woods_sight_budget() {
    let rules = Rules::compiled().unwrap();
    let mut rows = empty();
    rows[8] = "+".repeat(20);
    let mut w = original_scene(rows, None);
    for c in 160..180 {
        assert!((0.3..=0.6).contains(&w.game.wear.as_ref().unwrap()[c]));
    }
    for wear in [0.0f32, 0.3, 0.6, 1.0] {
        w.game.wear.as_mut().unwrap()[168] = wear;
        exact(
            ww2_core::terrain::cover_q(&w.game, &rules, 168),
            (0.55 + f64::from(wear)).min(1.4),
            "crater cover quality",
        );
    }
    for (trees, visible) in [(3, true), (4, false)] {
        let mut rows = empty();
        rows[8] = format!("....{}{}", "O".repeat(trees), ".".repeat(16 - trees));
        let w = original_scene(rows, None);
        assert_eq!(los(&w.game, &rules, (1.0, 17.0), (35.0, 17.0)), visible);
        exact(
            ww2_core::terrain::cover_q(&w.game, &rules, 164),
            0.6,
            "woods quality",
        );
    }
}

#[test]
fn map32_map37_map38_authored_depth_road_presence_and_wet_speeds() {
    let rules = Rules::compiled().unwrap();
    for ch in [b'F', b'M', b'D'] {
        let mut rows = empty();
        rows[8] = String::from_utf8(vec![ch; 20]).unwrap();
        let mut w = original_scene(rows, None);
        assert_eq!(w.roads, ch == b'D');
        for c in 160..180 {
            let depth = w.game.wear.as_ref().unwrap()[c];
            if ch != b'D' {
                assert!((0.2..=0.8).contains(&depth));
            }
            for wet in [0.0, 1.0] {
                w.game.wet = Some(wet);
                let expected = match ch {
                    b'F' => (0.75 + (0.35 - 0.75) * f64::from(depth)) * (1.0 - 0.3 * wet),
                    b'M' => (0.7 + (0.35 - 0.7) * f64::from(depth)) * (1.0 - 0.2 * wet),
                    _ => 1.35 + (1.0 - 1.35) * f64::from(depth),
                };
                exact(
                    ground_mul(&w.game, &rules, c, true),
                    expected,
                    "authored ground speed",
                );
            }
        }
    }
}

#[test]
fn map90_rails_eight_direction_masks_bridge_cells_and_station_yards() {
    use ww2_core::match_world_rails::WorldRails;
    let mut chars = vec![b'.'; 400];
    chars[210] = b'=';
    // Ordered loop exercises all eight directions and both bit contributions.
    let cells = [210, 211, 232, 252, 271, 270, 249, 229, 210];
    let metadata = json!({"stations":[{"region":0,"x":10,"y":10}],
        "lines":[{"a":0,"b":1,"cells":cells,"regions":[0,1]}]});
    let rails = WorldRails::from_metadata(Some(&metadata), &chars, 20, 20, 64).unwrap();
    assert_eq!(rails.lines[0].bridges, [210, 210]);
    assert_eq!(rails.station_at, [(210, 0)]);
    assert_eq!((rails.stations[0].x, rails.stations[0].z), (21.0, 21.0));
    let expected = [
        (210, 9),
        (211, 18),
        (232, 36),
        (252, 72),
        (271, 144),
        (270, 33),
        (249, 66),
        (229, 132),
    ];
    assert_eq!(rails.rail_at, expected);
    assert_eq!(rails.yard.len(), 121);
    for y in 5..=15 {
        for x in 5..=15 {
            assert!(rails.yard.contains(&(y * 20 + x)));
        }
    }
    assert!(!rails.yard.contains(&(10 * 20 + 16)));
}

#[test]
fn map24_public_authored_maps_remove_mines_and_private_scripts() {
    mod public {
        include!("../../server/src/map-data.rs");
    }
    assert_eq!(public::MAPS.len(), 33);
    let mut private_mines = 0;
    for (name, text) in public::MAPS {
        let private = original_map(&format!("authored:{name}.json"));
        let public: Value = serde_json::from_str(text).unwrap();
        let authoritative =
            AuthoritativeMap::from_json(&serde_json::to_vec(&private).unwrap()).unwrap();
        assert!(public.get("triggers").is_none() && public.get("scenario").is_none());
        assert!(
            public["layers"]["mines"]
                .as_array()
                .unwrap()
                .iter()
                .all(|r| r.as_str().unwrap().bytes().all(|c| c == b'.'))
        );
        assert!(
            public["layers"]["mineMaterials"]
                .as_array()
                .is_none_or(Vec::is_empty)
        );
        for (y, row) in public["rows"].as_array().unwrap().iter().enumerate() {
            for (x, ch) in row.as_str().unwrap().bytes().enumerate() {
                let c = y * authoritative.dimensions().0 + x;
                assert_ne!(ch, b'N');
                if authoritative.layers().mines[c] != 0 {
                    let object = authoritative.layers().objects[c];
                    let beneath = if object == b'.' {
                        authoritative.layers().ground[c]
                    } else {
                        object
                    };
                    assert_eq!(ch, beneath);
                    private_mines += 1;
                }
            }
        }
    }
    assert!(private_mines > 0);
}

fn batch_match(input: &Value) -> ww2_core::match_runtime::Match {
    use ww2_core::{
        economy::{Faction, Mode},
        economy_state::ArmySize,
        match_runtime::{Match, MatchOptions, PlayerConfig},
        weather::WeatherChoice,
    };
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(input).unwrap()).unwrap();
    let mut m = Match::new(
        &map,
        MatchOptions {
            seed: 701,
            mode: Mode::Conquest,
            players: (0..2)
                .map(|team| PlayerConfig {
                    name: format!("p{team}"),
                    team,
                    faction: Faction::try_from(team as u8).unwrap(),
                })
                .collect(),
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
        },
    )
    .unwrap();
    m.world.game.units.clear();
    m.commands.units.clear();
    m.grid.rebuild(&m.world.game.units);
    for p in &mut m.start.economy.players {
        p.mp = 5000.0;
    }
    m
}

#[test]
fn map33_bridge_artillery_and_timed_whole_span_collapse_with_occupants() {
    use ww2_core::{
        match_commands::Command,
        match_tick::{TickEvent, TickPathConfig},
        orders::OrderKey,
        unit_publication::{PublicationParts, StagedUnit, prepare_publication},
    };
    fn put(m: &mut ww2_core::match_runtime::Match, at: Point) -> u32 {
        let staged = StagedUnit::ordinary(
            &m.world.game,
            &m.rules,
            1,
            m.rules.kind("rifle").unwrap(),
            m.next_unit_id,
            m.world.game.units.len() as u32,
            m.start.players[1].spawn,
            Some(at),
        )
        .unwrap();
        let id = staged.unit.id;
        prepare_publication(
            PublicationParts {
                units: &mut m.world.game.units,
                grid: &mut m.grid,
                controls: &mut m.commands.units,
                recruitment: &mut m.recruit_metadata,
                next_id: &mut m.next_unit_id,
            },
            &m.rules,
            m.world.game.players.len(),
            vec![staged],
        )
        .unwrap()
        .publish();
        m.combat.initialize(id);
        m.firing.initialize(id);
        m.abilities.initialize(id);
        m.recovery.initialize(id);
        let control = m.commands.units.get_mut(&id).unwrap();
        control.auto_cast = false;
        control.auto_retreat = false;
        control.hold_fire = true;
        m.world.game.units.get_mut(&id).unwrap().hold_pos = true;
        id
    }
    let rows = (0..20)
        .map(|y| {
            format!(
                "{}{}{}",
                ".".repeat(10),
                if y == 5 {
                    '='
                } else if y == 15 {
                    'F'
                } else {
                    'W'
                },
                ".".repeat(9)
            )
        })
        .collect();
    let mut input = original_scene_input(rows, None);
    input["points"] = json!([{"x":2,"y":2}]);
    let mut m = batch_match(&input);
    let from = Point { x: 5.0, z: 3.0 };
    let to = Point { x: 35.0, z: 3.0 };
    let route = path(&m.world.game, &m.rules, "rifle", from, to);
    assert!(
        !route.is_empty()
            && route
                .iter()
                .all(|p| (p.z - 11.0).abs() < 6.0 || p.x < 20.0 || p.x > 22.0)
    );
    assert!(los(&m.world.game, &m.rules, (5.0, 25.0), (35.0, 25.0)));
    let victim = put(&mut m, Point { x: 21.0, z: 11.0 });
    m.world.game.reveal = true;
    m.initialize_tick_scene(0, TickPathConfig::default())
        .unwrap();
    m.enqueue_command(
        OrderKey {
            tick: 1,
            issuer: 0,
            sequence: 1,
        },
        Command::Support {
            kind: "artillery".into(),
            at: Point { x: 21.0, z: 11.0 },
            direction: Some(0.0),
        },
    )
    .unwrap();
    for tick in 1..=300 {
        let report = m.advance_tick().unwrap();
        if tick == 1 {
            assert_eq!(report.commands.len(), 1);
            assert_eq!(report.commands[0].denial, None);
        }
    }
    assert_eq!(m.world.game.physical(110), b'W');
    assert!(!m.world.game.units.contains_key(&victim));
    let detour = path(&m.world.game, &m.rules, "rifle", from, to);
    assert!(!detour.is_empty() && detour.iter().any(|p| p.z > 25.0));

    let mut rows = empty();
    rows[7] = ".....WWWW==WWWW.....".into();
    let mut input = original_scene_input(rows, None);
    input["buildings"] = json!([{"x":9,"y":7,"kind":"stone bridge"}]);
    input["triggers"] = json!([{"at":1,"say":"The bridge is going up!"},
        {"at":2,"blow":[9,7,10,7],"say":"Bridge down"}]);
    let mut m = batch_match(&input);
    assert_eq!(m.world.game.cell_hp[149], 2400.0);
    assert_eq!(m.world.game.cell_hp[150], 2400.0);
    let victim = put(&mut m, Point { x: 19.0, z: 15.0 });
    m.initialize_tick_scene(0, TickPathConfig::default())
        .unwrap();
    let mut said = Vec::new();
    for _ in 0..50 {
        let report = m.advance_tick().unwrap();
        for event in report.events {
            if let TickEvent::Scenario(say) = event {
                said.push(say.shot_json()["text"].as_str().unwrap().to_owned());
            }
        }
    }
    assert_eq!(said, ["The bridge is going up!", "Bridge down"]);
    assert_eq!(m.world.game.physical(149), b'W');
    assert_eq!(m.world.game.physical(150), b'W');
    assert!(!m.world.game.units.contains_key(&victim));
}
