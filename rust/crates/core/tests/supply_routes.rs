use serde_json::Value;
use ww2_core::{
    game::Point,
    navigation::{TerrainView, Versions},
    pathfinding::PathTerrain,
    rules::Rules,
    supply_convoys::RouteFailure,
    supply_routes::{Handoff, RouteRequest, SupplyRouting, Threat},
};
fn point(v: &Value) -> Point {
    Point {
        x: v["x"].as_f64().unwrap(),
        z: v["z"].as_f64().unwrap(),
    }
}
fn point_bits(v: &Value) -> Point {
    Point {
        x: f64::from_bits(u64::from_str_radix(v[0].as_str().unwrap(), 16).unwrap()),
        z: f64::from_bits(u64::from_str_radix(v[1].as_str().unwrap(), 16).unwrap()),
    }
}
fn values(v: &Value) -> Vec<i64> {
    v.as_array()
        .unwrap()
        .iter()
        .flat_map(|r| std::iter::repeat_n(r[0].as_i64().unwrap(), r[1].as_u64().unwrap() as usize))
        .collect()
}
struct Input {
    w: usize,
    h: usize,
    flags: Vec<u16>,
    heights: Option<Vec<i8>>,
    near: Vec<u8>,
    mines: Vec<bool>,
    fires: Vec<bool>,
    bunkers: Vec<Point>,
    wear: Option<Vec<f32>>,
    wet: f64,
    roads: bool,
    world_known: bool,
    versions: Versions,
}
impl Input {
    fn read(v: &Value) -> Self {
        let w = v["w"].as_u64().unwrap() as usize;
        let h = v["h"].as_u64().unwrap() as usize;
        let mut mines = vec![false; w * h];
        for c in v["mines"].as_array().unwrap() {
            mines[c.as_u64().unwrap() as usize] = true;
        }
        let mut fires = vec![false; w * h];
        for c in v["fires"].as_array().unwrap() {
            fires[c.as_u64().unwrap() as usize] = true;
        }
        Self {
            w,
            h,
            flags: values(&v["flags"]).into_iter().map(|n| n as u16).collect(),
            heights: (!v["height"].is_null())
                .then(|| values(&v["height"]).into_iter().map(|n| n as i8).collect()),
            near: values(&v["near"]).into_iter().map(|n| n as u8).collect(),
            mines,
            fires,
            bunkers: v["bunkers"]
                .as_array()
                .unwrap()
                .iter()
                .map(point_bits)
                .collect(),
            wear: (!v["wear"].is_null()).then(|| {
                values(&v["wear"])
                    .into_iter()
                    .map(|n| f32::from_bits(n as u32))
                    .collect()
            }),
            wet: v["wet"].as_f64().unwrap(),
            roads: v["roads"].as_bool().unwrap(),
            world_known: v["worldKnown"].as_bool().unwrap(),
            versions: Versions {
                infantry: v["infantry"].as_u64().unwrap(),
                vehicle: v["vehicle"].as_u64().unwrap(),
                naval: v["naval"].as_u64().unwrap(),
                obstruction: v["obstruction"].as_u64().unwrap(),
                knowledge: v["version"].as_u64().unwrap(),
            },
        }
    }
    fn terrain(&self) -> PathTerrain<'_> {
        PathTerrain {
            navigation: TerrainView {
                width: self.w,
                height: self.h,
                flags: &self.flags,
                heights: self.heights.as_deref(),
                world_known: self.world_known,
                knowledge_id: 1,
                versions: self.versions,
                changed: None,
                changes: &[],
            },
            near_walls: &self.near,
            known_mines: &self.mines,
            fires: &self.fires,
            bunkers: &self.bunkers,
            wear: self.wear.as_deref(),
            wetness: self.wet,
            roads: self.roads,
        }
    }
}
#[test]
fn real_safe_routes_continuation_and_physical_handoff_match_original() {
    let f: Value = serde_json::from_str(include_str!("fixtures/supply-routes.json")).unwrap();
    let terrains = f["terrains"]
        .as_array()
        .unwrap()
        .iter()
        .map(Input::read)
        .collect::<Vec<_>>();
    let rules = Rules::compiled().unwrap();
    let truck = rules.kinds.iter().find(|k| k.name == "truck").unwrap();
    let rifle = rules.kinds.iter().find(|k| k.name == "rifle").unwrap();
    for (i, c) in f["cases"].as_array().unwrap().iter().enumerate() {
        let mut routes = SupplyRouting::default();
        for (j, q) in c["queries"].as_array().unwrap().iter().enumerate() {
            let input = &terrains[q["input"].as_u64().unwrap() as usize];
            let threats = q["threats"]
                .as_array()
                .unwrap()
                .iter()
                .map(|t| Threat {
                    at: point(&t["at"]),
                    range: t["range"].as_f64().unwrap(),
                })
                .collect::<Vec<_>>();
            let from = point(&q["from"]);
            let expected = q["path"]
                .as_array()
                .unwrap()
                .iter()
                .map(point_bits)
                .collect::<Vec<_>>();
            let actual = routes
                .route(
                    RouteRequest {
                        owner: 0,
                        tick: q["tick"].as_u64().unwrap(),
                        from,
                        to: point(&q["to"]),
                        safe: q["safe"].as_bool().unwrap(),
                    },
                    input.terrain(),
                    truck,
                    &threats,
                )
                .unwrap();
            assert_eq!(
                actual.path.len(),
                expected.len(),
                "case{i} query{j} path length"
            );
            for (a, b) in actual.path.iter().zip(&expected) {
                assert_eq!(
                    (a.x.to_bits(), a.z.to_bits()),
                    (b.x.to_bits(), b.z.to_bits()),
                    "case{i} query{j} path"
                );
            }
            assert_eq!(
                actual.route_end.map(|p| [p.x.to_bits(), p.z.to_bits()]),
                (!q["routeEnd"].is_null()).then(|| {
                    let p = point_bits(&q["routeEnd"]);
                    [p.x.to_bits(), p.z.to_bits()]
                }),
                "case{i} query{j} route endpoint effect"
            );
            if let Some(reason) = q["failure"].as_str() {
                assert_eq!(
                    actual.failure,
                    Some(match reason {
                        "dangerous" => RouteFailure::Dangerous,
                        "blocked" => RouteFailure::Blocked,
                        _ => RouteFailure::Unreachable,
                    })
                );
            } else if !actual.path.is_empty() {
                assert!(actual.failure.is_none());
            }
            assert_eq!(
                SupplyRouting::continue_route(
                    input.terrain().navigation,
                    truck,
                    from,
                    &expected,
                    0.0,
                    &threats
                ),
                q["continuation"].as_bool().unwrap(),
                "case{i} query{j} continue"
            );
            assert_eq!(
                SupplyRouting::continue_route(
                    input.terrain().navigation,
                    truck,
                    from,
                    &expected,
                    1.0,
                    &threats
                ),
                q["stuck"].as_bool().unwrap()
            );
        }
        for (j, q) in c["handoffs"].as_array().unwrap().iter().enumerate() {
            let input = &terrains[q["input"].as_u64().unwrap() as usize];
            let cells = q["cells"]
                .as_array()
                .unwrap()
                .iter()
                .map(|n| n.as_u64().unwrap() as u32)
                .collect::<Vec<_>>();
            let actual = routes
                .handoff(
                    Handoff {
                        tick: q["tick"].as_u64().unwrap(),
                        start: point(&q["start"]),
                        store: point(&q["store"]),
                        radius: q["radius"].as_f64().unwrap(),
                    },
                    input.terrain(),
                    rifle,
                    &cells,
                )
                .unwrap();
            assert_eq!(actual, q["answer"].as_bool().unwrap(), "case{i} handoff{j}");
        }
    }
}

#[test]
fn invalid_routing_shapes_and_nonfinite_continuation_return_denials() {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/supply-routes.json")).unwrap();
    let mut input = Input::read(&fixture["terrains"][0]);
    let rules = Rules::compiled().unwrap();
    let truck = rules.kinds.iter().find(|k| k.name == "truck").unwrap();
    let rifle = rules.kinds.iter().find(|k| k.name == "rifle").unwrap();
    let at = Point { x: 1.0, z: 1.0 };
    let end = Point { x: 9.0, z: 1.0 };
    let path = [end];
    assert!(!SupplyRouting::continue_route(
        input.terrain().navigation,
        truck,
        at,
        &path,
        f64::NAN,
        &[]
    ));
    assert!(!SupplyRouting::continue_route(
        input.terrain().navigation,
        truck,
        at,
        &[Point {
            x: f64::NAN,
            z: 1.0
        }],
        0.0,
        &[]
    ));
    let mut state = SupplyRouting::default();
    assert!(
        state
            .route(
                RouteRequest {
                    owner: 32,
                    tick: 0,
                    from: at,
                    to: end,
                    safe: true
                },
                input.terrain(),
                truck,
                &[]
            )
            .is_err()
    );
    input.flags.pop();
    assert!(!SupplyRouting::continue_route(
        input.terrain().navigation,
        truck,
        at,
        &path,
        0.0,
        &[]
    ));
    assert!(
        state
            .route(
                RouteRequest {
                    owner: 0,
                    tick: 0,
                    from: at,
                    to: end,
                    safe: true
                },
                input.terrain(),
                truck,
                &[]
            )
            .is_err()
    );
    assert!(
        state
            .handoff(
                Handoff {
                    tick: 0,
                    start: at,
                    store: end,
                    radius: 15.0
                },
                input.terrain(),
                rifle,
                &[]
            )
            .is_err()
    );
}
