//! Complete original selected checks over actual pathfinding and hull queries.
//! Canonical Source startup metadata is retained; native evidence uses explicit routing views.
use serde_json::{Value, json};
use ww2_core::{
    game::{Game, Point, Pose},
    map::AuthoritativeMap,
    math,
    navigation::{Navigation, Options, TerrainView, Versions},
    pathfinding::{PathRules, PathStatus, PathTerrain, Pathfinding},
    rules::{Rules, UnitKind},
    vehicle::vehicle_position_clear,
};

fn fixture() -> Value {
    let value: Value =
        serde_json::from_str(include_str!("../fixtures/pathfinding-selected.json")).unwrap();
    assert_eq!(
        value["reference"],
        "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
    );
    assert_eq!(value["node"], "v24.21.0");
    assert_eq!(value["arch"], "x64");
    assert_eq!(value["scenes"].as_array().unwrap().len(), 12);
    assert_eq!(
        value["scenes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["queries"].as_array().unwrap().len())
            .sum::<usize>(),
        88
    );
    value
}
fn n(v: &Value) -> f64 {
    v.as_f64().unwrap()
}
fn point(v: &Value) -> Point {
    Point {
        x: n(&v["x"]),
        z: n(&v["z"]),
    }
}
fn bits(p: Point) -> Value {
    json!([
        format!("{:016x}", p.x.to_bits()),
        format!("{:016x}", p.z.to_bits())
    ])
}
fn runs(v: &Value) -> Vec<f64> {
    v.as_array()
        .unwrap()
        .iter()
        .flat_map(|r| std::iter::repeat_n(n(&r[0]), r[1].as_u64().unwrap() as usize))
        .collect()
}
fn chars(v: &Value) -> Vec<u8> {
    v.as_array()
        .unwrap()
        .iter()
        .flat_map(|r| {
            std::iter::repeat_n(
                r[0].as_str().unwrap().as_bytes()[0],
                r[1].as_u64().unwrap() as usize,
            )
        })
        .collect()
}
fn oracle(name: &str) -> &'static str {
    match name {
        "house" => "603d83d3454052aeaa916ffcc52c55f0e68c9d97c4467121382ebb8cffd06321",
        "mines" => "595d74b9da8cd4806c442ce462b48c635c935ad6e1d0d643af779e4a5be172f0",
        "roadMud" => "f3214194663892841cb096763ea9d9732a72149237fbeeef7f38df7a7c819dba",
        "naval" => "f6dfa6f44a61afe78340cb771aee1b8acb537171b3202b70ac5f80f54f0229e0",
        "elevation" => "2c7348829bdd69db854f3ff21874901818b9540c92b4a43bcb1dff9e763a208e",
        "bunkerBase" => "ca40f1b77e8940d39a59c4d3227e14af5c409fcd293c061eb7191d0b0e415b59",
        "bunkerNear" => "9f197446e69527df75469b8a595ddbb2b546f1fd0bc4d186c8970599c5fcd170",
        "freshRoad" => "357795d0df4e3151ec226ac54e3740cea85d12d51403df91c6c67446bbd98151",
        "wornRoad" => "ebc84399dc4d07d167c45b883dabfb8d941990c9729e34f5272a997b92bf54f7",
        other => panic!("unknown original route oracle {other}"),
    }
}
// The selected hash oracles have integral metre coordinates. Prove the exact
// JSON.stringify input bytes as well as every f64 bit, without adding a hash dependency.
fn original_route_json(points: &[Point]) -> String {
    let integer = |v: f64| {
        assert!(v.is_finite() && v.fract() == 0.0 && v.abs() <= 9_007_199_254_740_991.0);
        format!("{}", v as i64)
    };
    format!(
        "[{}]",
        points
            .iter()
            .map(|p| format!("[{},{}]", integer(p.x), integer(p.z)))
            .collect::<Vec<_>>()
            .join(",")
    )
}
fn versions(t: &Value) -> Versions {
    let v = &t["versions"];
    Versions {
        infantry: v["infantry"].as_u64().unwrap(),
        vehicle: v["vehicle"].as_u64().unwrap(),
        naval: v["naval"].as_u64().unwrap(),
        obstruction: v["obstruction"].as_u64().unwrap(),
        knowledge: v["knowledge"].as_u64().unwrap(),
    }
}
fn hull_clear(g: &Game, kind: &UnitKind, from: Point, points: &[Point]) {
    assert!(!points.is_empty());
    let mut at = from;
    for p in points {
        let d = math::hypot(p.x - at.x, p.z - at.z);
        let rot = math::atan2(p.z - at.z, p.x - at.x);
        let count = (d / 0.25).ceil().max(1.0) as usize;
        for i in 0..=count {
            let pose = Pose {
                x: at.x + (p.x - at.x) * i as f64 / count as f64,
                z: at.z + (p.z - at.z) * i as f64 / count as f64,
                rot,
            };
            assert!(
                vehicle_position_clear(g, pose, kind, kind.block()),
                "original continuous hull-clearance sample"
            );
        }
        at = *p;
    }
}
fn run_scene(f: &Value, name: &str, count: usize) -> Vec<Vec<Point>> {
    let s = f["scenes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == name)
        .unwrap();
    let queries = s["queries"].as_array().unwrap();
    assert_eq!(
        queries.len(),
        count,
        "{name}: original sequential query count"
    );
    let first = &queries[0]["terrain"];
    let cells = runs(&first["flags"]).len();
    let (w, h) = if name == "large512" {
        (512, 512)
    } else {
        (
            s["provenance"]["map"]["w"].as_u64().unwrap() as usize,
            s["provenance"]["map"]["h"].as_u64().unwrap() as usize,
        )
    };
    assert_eq!(cells, w * h);
    // This Game supplies physical terrain only to the public hull query. Native
    // pathfinding receives the complete recorded Source routing view separately.
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&json!({
        "name":"Selected route hull query","w":w,"h":h,"rows":vec![".".repeat(w);h],"spawns":[{"x":1,"y":1},{"x":w-2,"y":h-2}],"points":[{"x":w/2,"y":h/2}]
    })).unwrap()).unwrap();
    let mut physical = map.initialize_terrain().unwrap().query_game();
    let rules = Rules::compiled().unwrap();
    let path_rules = PathRules::compiled().unwrap();
    let mut finder = Pathfinding::new();
    let mut navigation = Navigation::new();
    let mut flags = vec![0u16; cells];
    let mut navigation_flags = vec![0u16; cells];
    let mut near = vec![0u8; cells];
    let mut wear = vec![0f32; cells];
    let mut heights = vec![0i8; cells];
    let mut navigation_heights = vec![0i8; cells];
    let mut mines = vec![false; cells];
    let mut fires = vec![false; cells];
    let mut results: Vec<Vec<Point>> = Vec::new();
    for (i, query) in queries.iter().enumerate() {
        let t = &query["terrain"];
        // Keep the same backing buffers and actual versions across every scene's
        // sequence, including cost-only updates with unchanged topology.
        for (to, v) in flags.iter_mut().zip(runs(&t["flags"])) {
            *to = v as u16;
        }
        for (to, v) in navigation_flags.iter_mut().zip(runs(&t["navigationFlags"])) {
            *to = v as u16;
        }
        for (to, v) in near.iter_mut().zip(runs(&t["near"])) {
            *to = v as u8;
        }
        if !t["wear"].is_null() {
            for (to, v) in wear.iter_mut().zip(runs(&t["wear"])) {
                *to = v as f32;
            }
        }
        if !t["height"].is_null() {
            for (to, v) in heights.iter_mut().zip(runs(&t["height"])) {
                *to = v as i8;
            }
        }
        if !t["navigationHeight"].is_null() {
            for (to, v) in navigation_heights
                .iter_mut()
                .zip(runs(&t["navigationHeight"]))
            {
                *to = v as i8;
            }
        }
        mines.fill(false);
        fires.fill(false);
        for c in t["mines"].as_array().unwrap() {
            mines[c.as_u64().unwrap() as usize] = true;
        }
        for c in t["fires"].as_array().unwrap() {
            fires[c.as_u64().unwrap() as usize] = true;
        }
        let bunkers: Vec<_> = t["bunkers"].as_array().unwrap().iter().map(point).collect();
        let from = point(&query["from"]);
        let to = point(&query["to"]);
        let kind = &rules.kinds[rules.kind(query["type"].as_str().unwrap()).unwrap()];
        let view = TerrainView {
            width: w,
            height: h,
            flags: &flags,
            heights: (!t["height"].is_null()).then_some(heights.as_slice()),
            world_known: t["worldKnown"].as_bool().unwrap(),
            knowledge_id: 1,
            versions: versions(t),
            changed: None,
            changes: &[],
        };
        let terrain = PathTerrain {
            navigation: view,
            near_walls: &near,
            known_mines: &mines,
            fires: &fires,
            bunkers: &bunkers,
            wear: (!t["wear"].is_null()).then_some(wear.as_slice()),
            wetness: n(&t["wetness"]),
            roads: t["roads"].as_bool().unwrap(),
        };
        let actual = finder
            .find_path(terrain, path_rules, kind, from, to, None)
            .unwrap();
        let expected = &query["expected"];
        let status = match actual.status {
            PathStatus::Found => "Found",
            PathStatus::Unreachable => "Unreachable",
            PathStatus::ExpansionLimit => "ExpansionLimit",
        };
        assert_eq!(
            json!(
                actual
                    .waypoints
                    .iter()
                    .copied()
                    .map(bits)
                    .collect::<Vec<_>>()
            ),
            expected["pointBits"],
            "{name}:{i}: exact original path bits"
        );
        assert_eq!(
            bits(actual.route_end),
            expected["routeEnd"],
            "{name}:{i}: routeEnd"
        );
        assert_eq!(
            actual.expansions as u64,
            expected["expansions"].as_u64().unwrap(),
            "{name}:{i}: expansions"
        );
        assert_eq!(
            status,
            expected["status"].as_str().unwrap(),
            "{name}:{i}: status"
        );
        assert_eq!(
            actual.local_goal as u64,
            expected["localGoal"].as_u64().unwrap(),
            "{name}:{i}: local goal"
        );
        assert_eq!(
            actual.limited,
            expected["limited"].as_bool().unwrap(),
            "{name}:{i}: limited"
        );
        // Independently invoke actual coarse navigation on the captured Source
        // footprint view. Fine search above independently constructs its native view.
        let leg = navigation
            .leg(
                TerrainView {
                    flags: &navigation_flags,
                    heights: (!t["navigationHeight"].is_null())
                        .then_some(navigation_heights.as_slice()),
                    ..view
                },
                expected["start"].as_u64().unwrap() as usize,
                expected["fullGoal"].as_u64().unwrap() as usize,
                kind.block(),
                Options::default(),
            )
            .unwrap();
        assert_eq!(
            leg.connected,
            expected["connected"].as_bool().unwrap(),
            "{name}:{i}: connected"
        );
        assert_eq!(leg.goal as u64, expected["localGoal"].as_u64().unwrap());
        assert_eq!(leg.limited, expected["limited"].as_bool().unwrap());
        let checks = &query["checks"];
        if let Some(label) = checks["hash"].as_str() {
            assert_eq!(f["oracle"][label], oracle(label));
            assert_eq!(
                original_route_json(&actual.waypoints),
                expected["routeJson"].as_str().unwrap(),
                "{name}:{i}: exact original SHA256 input bytes"
            );
        }
        if checks["hull"] == true {
            physical.flags.copy_from_slice(&flags);
            hull_clear(&physical, kind, from, &actual.waypoints);
        }
        if let Some(c) = checks["clearance"].as_object() {
            assert!(!actual.waypoints.is_empty());
            let mut closest = f64::INFINITY;
            let mut at = from;
            for p in &actual.waypoints {
                for k in 0..=40 {
                    let x = at.x + (p.x - at.x) * k as f64 / 40.0;
                    let z = at.z + (p.z - at.z) * k as f64 / 40.0;
                    closest = closest.min(math::hypot(
                        ((x - n(&c["x"])).abs() - n(&c["half"])).max(0.0),
                        ((z - n(&c["z"])).abs() - n(&c["half"])).max(0.0),
                    ));
                }
                at = *p;
            }
            assert!(
                closest >= n(&c["minimum"]),
                "{name}:{i}: original footprint clearance {closest}"
            );
        }
        if checks["nonempty"] == true {
            assert!(!actual.waypoints.is_empty());
        }
        if let Some(type_name) = checks["mud"].as_str() {
            if type_name == "tank" {
                assert!(actual.waypoints.len() > 1);
                let letters = chars(&t["chars"]);
                assert!(actual.waypoints.iter().all(|p| letters
                    [(p.z / 2.0).floor() as usize * w + (p.x / 2.0).floor() as usize]
                    != b'M'));
            } else {
                assert_eq!(actual.waypoints.len(), 1);
            }
        }
        if let Some(type_name) = checks["road"].as_str() {
            let count = actual
                .waypoints
                .iter()
                .filter(|p| (1.0..=3.0).contains(&(p.z / 2.0).floor()))
                .count();
            if type_name == "tank" {
                assert!(count > 5);
            } else {
                assert_eq!(count, 0);
            }
        }
        if checks["large"] == true {
            assert!(!actual.waypoints.is_empty());
            let last = actual.waypoints.last().unwrap();
            assert!(math::hypot(last.x - from.x, last.z - from.z) < 120.0);
            assert!(actual.expansions <= 4096);
        }
        if let Some(prior) = checks["differentFrom"].as_u64() {
            assert_ne!(
                actual.waypoints, results[prior as usize],
                "wet costs refresh with unchanged topology"
            );
        }
        results.push(actual.waypoints);
    }
    results
}
#[test]
fn selected_house_route_has_original_hash_input() {
    run_scene(&fixture(), "house", 1);
}
#[test]
fn selected_elevation_route_has_original_hash_input() {
    run_scene(&fixture(), "elevation", 1);
}
#[test]
fn selected_naval_route_has_original_hash_input() {
    run_scene(&fixture(), "naval", 1);
}
#[test]
fn selected_classic_hq_keeps_both_profiles_clear() {
    let f = fixture();
    let scene = f["scenes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "classic-hq")
        .unwrap();
    assert_eq!(scene["provenance"]["options"]["mode"], "classic");
    let actors = scene["provenance"]["actors"].as_array().unwrap();
    let hq = &actors
        .iter()
        .find(|row| row[1]["owner"] == 0 && row[1]["type"] == "hq")
        .unwrap()[1];
    let structure_id = &hq["structureId"];
    assert!(!structure_id.is_null());
    assert!(
        scene["provenance"]["structures"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| &row[0] == structure_id)
    );
    let cells = hq["cells"].as_array().unwrap();
    assert!(!cells.is_empty());
    for cell in cells {
        for key in ["buildingCells", "structuralCells"] {
            assert!(
                scene["provenance"][key]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|row| &row[0] == cell)
            );
        }
    }
    run_scene(&f, "classic-hq", 2);
}
#[test]
fn selected_annihilation_bunker_avoids_and_accepts_targets() {
    run_scene(&fixture(), "annihilation-bunker", 4);
}
#[test]
fn selected_cost_cache_oracles_preserve_original_sequence() {
    let f = fixture();
    for (name, count) in [
        ("mines", 1),
        ("road-mud", 1),
        ("wear-wetness", 4),
        ("live-bunker-cache", 7),
    ] {
        run_scene(&f, name, count);
    }
}
#[test]
fn selected_roads_and_mud_keep_all_four_original_path_assertions() {
    let f = fixture();
    run_scene(&f, "mud-selection", 2);
    run_scene(&f, "road-selection", 2);
}
#[test]
fn selected_512_grid_keeps_every_original_leg_and_expansion_bound() {
    let f = fixture();
    let s = f["scenes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "large512")
        .unwrap();
    let queries = s["queries"].as_array().unwrap();
    assert!(queries[..31].iter().all(|q| q["type"] == "rifle"));
    assert!(queries[31..].iter().all(|q| q["type"] == "tank"));
    run_scene(&f, "large512", 62);
}
