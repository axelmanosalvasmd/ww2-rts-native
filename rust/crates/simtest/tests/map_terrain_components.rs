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
    assert!(!direct.is_empty(), "the one-level control must find a route");
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
