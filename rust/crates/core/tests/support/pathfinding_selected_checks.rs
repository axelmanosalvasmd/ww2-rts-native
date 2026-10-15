//! Selected Source failure signals through the actual fine-search and command caller.
use super::*;
use crate::{catalog::Catalog, match_commands::search_route, rules::Rules, unit_spawn};
use serde_json::{Value, json};

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

fn point(value: &Value) -> Point {
    Point {
        x: value["x"].as_f64().unwrap(),
        z: value["z"].as_f64().unwrap(),
    }
}

fn bits(p: Point) -> Value {
    json!([
        format!("{:016x}", p.x.to_bits()),
        format!("{:016x}", p.z.to_bits())
    ])
}

#[test]
fn selected_goal_start_failure_signals_match_source() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../simtest/tests/fixtures/pathfinding-branches.json"
    ))
    .unwrap();
    assert_eq!(
        fixture["reference"],
        "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
    );
    let catalog = Catalog::compiled().unwrap();
    let rules = Rules::compiled().unwrap();
    let path_rules = PathRules::compiled().unwrap();
    let mut checks = 0;
    let mut rejections = 0;
    let mut same_cells = 0;
    for scenario in fixture["scenarios"].as_array().unwrap() {
        if !matches!(
            scenario["name"].as_str().unwrap(),
            "goal-start-selected" | "diagonal-corners-mines-0"
        ) {
            continue;
        }
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
        let hazards = vec![false; flags.len()];
        let terrain = PathTerrain {
            navigation: TerrainView {
                width: scenario["w"].as_u64().unwrap() as usize,
                height: scenario["h"].as_u64().unwrap() as usize,
                flags: &flags,
                heights: heights.as_deref(),
                world_known: scenario["worldKnown"].as_bool().unwrap(),
                knowledge_id: 1,
                versions: crate::navigation::Versions::default(),
                changed: None,
                changes: &[],
            },
            near_walls: &near,
            known_mines: &hazards,
            fires: &hazards,
            bunkers: &[],
            wear: None,
            wetness: scenario["wetness"].as_f64().unwrap(),
            roads: scenario["roads"].as_bool().unwrap(),
        };
        let mut paths = Pathfinding::new();
        for query in scenario["queries"].as_array().unwrap() {
            let kind = rules.kind(query["type"].as_str().unwrap()).unwrap();
            let profile = &rules.kinds[kind];
            assert!(profile.infantry);
            let from = point(&query["from"]);
            let to = point(&query["to"]);
            let unit = unit_spawn::initial_unit(&rules, catalog, kind, 999, 0, from).unwrap();
            let diagnostic = &query["diagnostics"];
            let block = profile.block();
            paths.scratch.prepare(flags.len());
            let full_goal = paths.scratch.nearest_free(terrain.navigation, to, block);
            let stand = cell_of(terrain.navigation, from).unwrap();
            let start = if flags[stand] & block != 0 {
                paths.scratch.nearest_free(terrain.navigation, from, block)
            } else {
                stand
            };
            assert_eq!(start as u64, diagnostic["start"].as_u64().unwrap());
            assert_eq!(full_goal as u64, diagnostic["fullGoal"].as_u64().unwrap());
            let connected = paths
                .navigation
                .leg(
                    terrain.navigation,
                    start,
                    full_goal,
                    block,
                    Options::default(),
                )
                .unwrap()
                .connected;
            assert_eq!(connected, diagnostic["legConnected"].as_bool().unwrap());
            let route = search_route(&mut paths, terrain, path_rules, profile, &unit, to).unwrap();
            let failed = diagnostic["after"]["failed"].as_u64().unwrap()
                - diagnostic["before"]["failed"].as_u64().unwrap();
            let rejected = diagnostic["after"]["regionRejected"].as_u64().unwrap()
                - diagnostic["before"]["regionRejected"].as_u64().unwrap();
            assert_eq!(
                u64::from(route.failed),
                failed,
                "actual command failure signal"
            );
            assert_eq!(
                u64::from(!connected),
                rejected,
                "actual disconnected coarse leg"
            );
            if !connected {
                assert_eq!(route.result.expansions, 0);
                assert!(route.failed && route.result.waypoints.is_empty());
                rejections += 1;
            }
            if query["from"] == query["to"] {
                assert!(!route.failed && route.result.waypoints.is_empty());
                assert!(diagnostic["before"]["failed"].as_u64().unwrap() > 0);
                same_cells += 1;
            }
            let result = route.result;
            let status = match result.status {
                PathStatus::Found => "Found",
                PathStatus::Unreachable => "Unreachable",
                PathStatus::ExpansionLimit => "ExpansionLimit",
            };
            assert_eq!(
                json!({"points":result.waypoints.into_iter().map(bits).collect::<Vec<_>>(),
                    "routeEnd":bits(result.route_end),"expansions":result.expansions,"status":status,
                    "localGoal":result.local_goal,"limited":result.limited}),
                query["expected"]
            );
            checks += 1;
        }
    }
    assert_eq!((checks, rejections, same_cells), (13, 6, 1));
}
