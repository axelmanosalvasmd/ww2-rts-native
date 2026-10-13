//! Original spawn/placement source plus the independent existing Unit loader.
use serde_json::Value;
use ww2_core::catalog::Catalog;
use ww2_core::game::Point;
use ww2_core::navigation::{TerrainView, Versions};
use ww2_core::rules::Rules;
use ww2_core::unit_spawn::{SpawnError, SpawnPlacer, initial_unit};

fn point(v: &Value) -> Point {
    Point {
        x: v["x"].as_f64().unwrap(),
        z: v["z"].as_f64().unwrap(),
    }
}
fn runs(v: &Value) -> Vec<i64> {
    v.as_array()
        .unwrap()
        .iter()
        .flat_map(|p| std::iter::repeat_n(p[0].as_i64().unwrap(), p[1].as_u64().unwrap() as usize))
        .collect()
}
#[test]
fn spawn_projection_and_region_placement_match_original_source() {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/unit-spawn.json")).unwrap();
    assert_eq!(
        fixture["reference"],
        "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
    );
    assert_eq!(fixture["arch"], "x64");
    assert_eq!(fixture["node"], "v24.21.0");
    let rules = Rules::compiled().unwrap();
    let catalog = Catalog::compiled().unwrap();
    let mut spawn_count = 0;
    let mut query_count = 0;
    let mut map_count = 0;
    for scenario in fixture["scenarios"].as_array().unwrap() {
        let name = scenario["name"].as_str().unwrap();
        map_count += usize::from(name.starts_with("map:"));
        let width = scenario["w"].as_u64().unwrap() as usize;
        let height = scenario["h"].as_u64().unwrap() as usize;
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
        let spawns: Vec<_> = scenario["spawns"]
            .as_array()
            .unwrap()
            .iter()
            .map(point)
            .collect();
        let mut placer = SpawnPlacer::new();
        let mut version = 0;
        for (index, action) in scenario["actions"].as_array().unwrap().iter().enumerate() {
            if action["op"] == "patch" {
                for patch in action["cells"].as_array().unwrap() {
                    let c = patch[0].as_u64().unwrap() as usize;
                    flags[c] = patch[1].as_u64().unwrap() as u16;
                    if let Some(h) = &mut heights {
                        h[c] = patch[2].as_i64().unwrap() as i8;
                    }
                }
                version += 1;
                continue;
            }
            let view = TerrainView {
                width,
                height,
                flags: &flags,
                heights: heights.as_deref(),
                world_known: false,
                knowledge_id: 0,
                versions: Versions {
                    infantry: version,
                    vehicle: version,
                    naval: version,
                    ..Versions::default()
                },
                changed: None,
                changes: &[],
            };
            if action["op"] == "nearest" {
                let actual = placer
                    .nearest_free(
                        view,
                        point(&action["at"]),
                        action["block"].as_u64().unwrap() as u16,
                        (!action["origin"].is_null()).then(|| point(&action["origin"])),
                    )
                    .unwrap();
                assert_eq!(
                    actual,
                    action["cell"].as_u64().unwrap() as usize,
                    "{name}, action {index}"
                );
                query_count += 1;
            } else {
                let type_name = action["type"].as_str().unwrap();
                let kind = rules.kind(type_name).unwrap();
                let owner = action["owner"].as_u64().unwrap() as usize;
                let place = placer
                    .scatter_position(
                        view,
                        &rules.kinds[kind],
                        spawns[owner],
                        action["index"].as_u64().unwrap() as u32,
                    )
                    .unwrap();
                let raw = &action["unit"];
                let mut actual = initial_unit(
                    &rules,
                    catalog,
                    kind,
                    raw["id"].as_u64().unwrap() as u32,
                    owner,
                    place.point,
                )
                .unwrap();
                let mode = match scenario["mode"].as_str() {
                    None => ww2_core::economy::Mode::Default,
                    Some("classic") => ww2_core::economy::Mode::Classic,
                    Some("world") => ww2_core::economy::Mode::World,
                    _ => panic!("unknown fixture mode"),
                };
                let control =
                    ww2_core::match_commands::UnitControl::new_for_mode(&actual, &rules, mode)
                        .unwrap();
                assert_eq!(
                    control.aim.to_bits(),
                    raw["aim"].as_f64().unwrap().to_bits()
                );
                assert_eq!(
                    control.auto_cast,
                    raw["auto"].as_bool().unwrap(),
                    "{name}, action {index}"
                );
                assert_eq!(control.auto_retreat, raw["autoRetreat"].as_bool().unwrap());
                let mut expected =
                    simtest::load::unit(raw, &rules, spawns.len(), width * height).unwrap();
                assert_eq!(
                    place.point.x.to_bits(),
                    expected.x.to_bits(),
                    "{name}, {type_name}, action {index}, x"
                );
                assert_eq!(
                    place.point.z.to_bits(),
                    expected.z.to_bits(),
                    "{name}, {type_name}, action {index}, z"
                );
                assert!(
                    actual.pace.is_nan() && expected.pace.is_nan(),
                    "fresh pace is undefined in source"
                );
                actual.pace = 0.0;
                expected.pace = 0.0;
                assert_eq!(actual.path_gen, 0);
                expected.path_gen = 0; // Native identity is not serialized by JavaScript.
                assert_eq!(actual, expected, "{name}, {type_name}, action {index}");
                spawn_count += 1;
            }
        }
    }
    assert_eq!(
        (
            spawn_count,
            query_count,
            map_count,
            fixture["supported"].as_array().unwrap().len()
        ),
        (775, 513, 33, 44)
    );
}

#[test]
fn invalid_inputs_and_air_base_placement_are_rejected() {
    let rules = Rules::compiled().unwrap();
    let catalog = Catalog::compiled().unwrap();
    let mut placer = SpawnPlacer::new();
    let at = Point { x: 1.0, z: 1.0 };
    let mut view = TerrainView {
        width: 1,
        height: 1,
        flags: &[0],
        heights: None,
        world_known: false,
        knowledge_id: 0,
        versions: Versions::default(),
        changed: None,
        changes: &[],
    };
    assert_eq!(
        placer.nearest_free(
            view,
            Point {
                x: f64::NAN,
                z: 0.0
            },
            1,
            None
        ),
        Err(SpawnError::InvalidPosition)
    );
    assert_eq!(
        placer.nearest_free(
            view,
            at,
            1,
            Some(Point {
                x: 0.0,
                z: f64::INFINITY
            })
        ),
        Err(SpawnError::InvalidPosition)
    );
    view.width = 2;
    assert_eq!(
        placer.nearest_free(view, at, 1, None),
        Err(SpawnError::InvalidShape)
    );
    view.width = 1;
    view.height = 0;
    assert_eq!(
        placer.nearest_free(view, at, 1, None),
        Err(SpawnError::InvalidShape)
    );
    view.height = 1;
    for (kind, profile) in rules.kinds.iter().enumerate().filter(|(_, p)| p.air) {
        assert_eq!(
            placer.scatter_position(view, profile, at, 0),
            Err(SpawnError::UnsupportedAir)
        );
        assert_eq!(
            initial_unit(&rules, catalog, kind, 1, 0, at),
            Err(SpawnError::UnsupportedAir)
        );
    }
    let malformed = serde_json::json!({"tables": {"UNITS": {rules.kinds[0].name.clone(): {"models": -1, "hpPer": -1}}}});
    let malformed = Catalog::from_json(&malformed.to_string()).unwrap();
    assert_eq!(
        initial_unit(&rules, &malformed, 0, 1, 0, at),
        Err(SpawnError::InvalidDefinition)
    );
    assert_eq!(
        initial_unit(&rules, catalog, usize::MAX, 1, 0, at),
        Err(SpawnError::UnknownKind)
    );
    assert_eq!(
        initial_unit(&rules, catalog, 0, 0, 0, at),
        Err(SpawnError::InvalidId)
    );
    assert_eq!(
        initial_unit(
            &rules,
            catalog,
            0,
            1,
            0,
            Point {
                x: f64::INFINITY,
                z: 0.0
            }
        ),
        Err(SpawnError::InvalidPosition)
    );
}
