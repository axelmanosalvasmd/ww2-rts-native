use serde_json::{Value, json};
use std::collections::BTreeMap;
use ww2_core::{
    catalog::Catalog,
    economy::Mode,
    game::{PlaneState, Point, Unit},
    navigation::{TerrainView, Versions},
    produced_units::{self, Door, ProduceInput, ProducedPlacer, ProductionBuilding, RallyKind},
    rules::Rules,
    unit_spawn::initial_unit,
};
fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/produced-units.json")).unwrap()
}
fn p(v: &Value) -> Point {
    Point {
        x: v["x"].as_f64().unwrap(),
        z: v["z"].as_f64().unwrap(),
    }
}
fn point_opt(v: &Value) -> Option<Point> {
    (!v.is_null()).then(|| p(v))
}
fn bits(v: f64) -> Value {
    json!(format!("{:016x}", v.to_bits()))
}
fn runs(v: &Value) -> Vec<i64> {
    v.as_array()
        .unwrap()
        .iter()
        .flat_map(|r| std::iter::repeat_n(r[0].as_i64().unwrap(), r[1].as_u64().unwrap() as usize))
        .collect()
}
fn kind(rules: &Rules, v: &Value) -> usize {
    rules
        .kinds
        .iter()
        .position(|k| Some(k.name.as_str()) == v.as_str())
        .unwrap()
}
#[test]
fn ground_doors_and_production_match_original_on_maps_and_boundaries() {
    let f = fixture();
    assert_eq!(f["reference"], "e4e579a838aabae3b1d9dadbc17a0a87147bc52d");
    assert_eq!(f["node"], "v24.21.0");
    assert_eq!(f["arch"], "x64");
    let rules = Rules::compiled().unwrap();
    let catalog = Catalog::compiled().unwrap();
    let mut count = 0;
    let mut maps = 0;
    for scenario in f["scenarios"].as_array().unwrap() {
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
        let view = TerrainView {
            width: scenario["w"].as_u64().unwrap() as usize,
            height: scenario["h"].as_u64().unwrap() as usize,
            flags: &flags,
            heights: heights.as_deref(),
            world_known: false,
            knowledge_id: 0,
            versions: Versions::default(),
            changed: None,
            changes: &[],
        };
        let spawn = p(&scenario["spawn"]);
        let mut units = BTreeMap::new();
        for row in scenario["occupants"].as_array().unwrap() {
            let id = row["id"].as_u64().unwrap() as u32;
            let mut unit =
                initial_unit(&rules, catalog, kind(&rules, &row["type"]), id, 0, p(row)).unwrap();
            unit.hp = row["hp"].as_f64().unwrap();
            unit.riding = row["riding"].as_i64().unwrap() != 0;
            if row["air"].as_bool().unwrap() {
                unit.air = Some(PlaneState::Base);
            }
            units.insert(id, unit);
        }
        let mut placer = ProducedPlacer::new();
        for action in scenario["actions"].as_array().unwrap() {
            let k = kind(&rules, &action["type"]);
            let expected = &action["expected"];
            if action["op"] == "step" {
                let d = &action["door"];
                let result = placer
                    .step_out(
                        view,
                        &rules,
                        k,
                        10000,
                        &units,
                        Door {
                            at: p(&d["at"]),
                            radius: d["radius"].as_f64().unwrap(),
                            heading: d["heading"].as_f64().unwrap(),
                            origin: point_opt(&d["origin"]),
                        },
                        37,
                    )
                    .unwrap();
                assert_eq!(
                    json!([
                        bits(result.at.x),
                        bits(result.at.z),
                        bits(result.heading),
                        bits(result.aim),
                        bits(result.ghost_until)
                    ]),
                    *expected,
                    "{} {action}",
                    scenario["name"]
                );
            } else {
                let b = &action["building"];
                let result = placer
                    .produce(
                        &rules,
                        catalog,
                        view,
                        &units,
                        ProduceInput {
                            id: 10000,
                            owner: 0,
                            kind: k,
                            scatter_index: action["scatterIndex"].as_u64().unwrap() as u32,
                            player_spawn: spawn,
                            player_rally: point_opt(&action["playerRally"]),
                            building: ProductionBuilding {
                                at: p(&b["at"]),
                                kind: kind(&rules, &b["type"]),
                                rally: point_opt(&b["rally"]),
                            },
                            mode: Mode::from_kind(action["mode"].as_str()),
                            tick: 37,
                        },
                    )
                    .unwrap();
                assert_eq!(result.unit.id, 10000);
                assert_eq!(bits(result.unit.hp), expected["hp"]);
                assert_eq!(
                    json!([
                        bits(result.unit.x),
                        bits(result.unit.z),
                        bits(result.unit.rot),
                        bits(result.aim),
                        bits(result.unit.ghost)
                    ]),
                    expected["pose"],
                    "{} {action}",
                    scenario["name"]
                );
                assert_eq!(result.unit.world_goal, point_opt(&expected["rally"]));
                assert_eq!(result.unit.drift, expected["drift"].as_bool().unwrap());
                assert!(result.unit.path.is_empty());
                if let Some(rally) = result.rally {
                    assert_eq!(result.unit.world_goal, Some(rally.destination));
                    assert_eq!(
                        rally.kind,
                        if b["rally"].is_null() {
                            RallyKind::Player
                        } else {
                            RallyKind::Building
                        }
                    );
                }
            }
            count += 1;
        }
        maps += usize::from(scenario["name"].as_str().unwrap().starts_with("map:"));
    }
    assert_eq!(maps, 33);
    assert_eq!(count, 934);
}
#[test]
fn unsupported_and_invalid_inputs_do_not_mutate_occupancy() {
    let rules = Rules::compiled().unwrap();
    let catalog = Catalog::compiled().unwrap();
    let flags = [0u16; 25];
    let view = TerrainView {
        width: 5,
        height: 5,
        flags: &flags,
        heights: None,
        world_known: false,
        knowledge_id: 0,
        versions: Versions::default(),
        changed: None,
        changes: &[],
    };
    let units: BTreeMap<u32, Unit> = BTreeMap::new();
    let rifle = rules.kinds.iter().position(|k| k.name == "rifle").unwrap();
    let air = rules.kinds.iter().position(|k| k.air).unwrap();
    let building = rules.kinds.iter().position(|k| k.name == "hq").unwrap();
    let mut placer = ProducedPlacer::new();
    let input = ProduceInput {
        id: 1,
        owner: 0,
        kind: rifle,
        scatter_index: 0,
        player_spawn: Point { x: 3.0, z: 3.0 },
        player_rally: None,
        building: ProductionBuilding {
            at: Point { x: 3.0, z: 3.0 },
            kind: building,
            rally: None,
        },
        mode: Mode::Conquest,
        tick: 1,
    };
    assert!(matches!(
        placer.produce(
            &rules,
            catalog,
            view,
            &units,
            ProduceInput { kind: air, ..input }
        ),
        Err(produced_units::Error::UnsupportedAir)
    ));
    assert!(
        placer
            .produce(
                &rules,
                catalog,
                view,
                &units,
                ProduceInput { id: 0, ..input }
            )
            .is_err()
    );
    assert!(
        placer
            .produce(
                &rules,
                catalog,
                view,
                &units,
                ProduceInput {
                    tick: u64::MAX,
                    ..input
                }
            )
            .is_err()
    );
    let door = Door {
        at: Point { x: 3.0, z: 3.0 },
        radius: 4.0,
        heading: 0.0,
        origin: None,
    };
    for radius in [-1.0, f64::NAN, f64::INFINITY] {
        assert!(
            placer
                .step_out(view, &rules, rifle, 1, &units, Door { radius, ..door }, 1)
                .is_err()
        );
    }
    assert!(
        placer
            .step_out(
                TerrainView { width: 0, ..view },
                &rules,
                rifle,
                1,
                &units,
                door,
                1
            )
            .is_err()
    );
    assert!(units.is_empty());
}
