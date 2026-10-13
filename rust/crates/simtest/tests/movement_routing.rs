//! Fine searches execute from inside movement, then every movement field is checked against JS.
use serde_json::Value;
use ww2_core::game::{MOVE, SIGHT};
use ww2_core::movement::RouteRequest;
use ww2_core::navigation::{TerrainView, Versions};
use ww2_core::pathfinding::{PathRules, PathTerrain, Pathfinding};

#[test]
fn live_recovery_matches_original_movement() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/movement-routing.json")).unwrap();
    assert_eq!(
        fixture["reference"],
        "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
    );
    assert_eq!(fixture["node"], "v24.21.0");
    assert_eq!(fixture["arch"], "x64");
    let r = &fixture["rules"];
    let rules = PathRules {
        wall_hug: r["wallHug"].as_f64().unwrap(),
        bunker_radius: r["bunkerRadius"].as_f64().unwrap(),
        road_speed: r["roadSpeed"].as_f64().unwrap(),
        wood_speed: r["woodSpeed"].as_f64().unwrap(),
        ford_speed: serde_json::from_value(r["fordSpeed"].clone()).unwrap(),
        mud_speed: serde_json::from_value(r["mudSpeed"].clone()).unwrap(),
        churn: r["churn"].as_f64().unwrap(),
        wet_ford: r["wetFord"].as_f64().unwrap(),
        wet_ground: r["wetGround"].as_f64().unwrap(),
    };
    let mut calls = [0, 0];
    for case in fixture["cases"].as_array().unwrap() {
        let mut planner = Pathfinding::new();
        let report =
            simtest::replay::replay_with_routes(case, &mut |g, kinds, u, goal, request| {
                calls[usize::from(request == RouteRequest::Immediate)] += 1;
                let mut near = vec![0; g.w * g.h];
                for (c, &flags) in g.flags.iter().enumerate() {
                    if flags & (MOVE | SIGHT) == MOVE | SIGHT {
                        let x = c % g.w;
                        for dy in [-1isize, 0, 1] {
                            for dx in -isize::from(x > 0)..=isize::from(x < g.w - 1) {
                                let n = c as isize + dy * g.w as isize + dx;
                                if n >= 0 && n < near.len() as isize {
                                    near[n as usize] = 1;
                                }
                            }
                        }
                    }
                }
                let empty = vec![false; g.w * g.h];
                let terrain = PathTerrain {
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
                    known_mines: &empty,
                    fires: &empty,
                    bunkers: &[],
                    wear: g.wear.as_deref(),
                    wetness: g.wet.unwrap_or(0.0),
                    roads: g.ground.as_ref().unwrap().contains(&b'D'),
                };
                let result = planner
                    .find_path(terrain, rules, &kinds.kinds[u.kind], u.at(), goal, None)
                    .unwrap();
                if !result.waypoints.is_empty() {
                    u.path = result.waypoints;
                    u.path_gen += 1;
                }
                u.repath = 1.0;
            })
            .unwrap();
        assert!(
            report.mismatch.is_none(),
            "{}: {:?}",
            case["name"],
            report.mismatch
        );
        assert_eq!(report.ticks, 12);
        assert_eq!(report.calls, 12);
    }
    assert_eq!(
        calls,
        [2, 2],
        "both recovery paths must run on passable and blocked maps"
    );
}
