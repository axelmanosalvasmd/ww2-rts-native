use serde_json::{Value, json};
use ww2_core::game::{Point, Wreck};
use ww2_core::pathfinding::{PathRules, PathStatus, Pathfinding};
use ww2_core::route_knowledge::LiveRouting;
use ww2_core::route_knowledge::{
    CellMetadata, CoverBasis, InitialTerrain, KnowledgeError, Material, RememberedCell,
    RememberedSection, RouteKnowledge,
};
use ww2_core::rules::Rules;

fn number(v: &Value) -> f64 {
    v.as_f64().unwrap()
}
fn bytes(v: &Value) -> Vec<u8> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().as_bytes()[0])
        .collect()
}
fn point(v: &Value) -> Point {
    Point {
        x: number(&v["x"]),
        z: number(&v["z"]),
    }
}
fn material(v: &Value) -> Option<Material> {
    match v.as_str() {
        Some("soil") => Some(Material::Soil),
        Some("road") => Some(Material::Road),
        Some("wood") => Some(Material::Wood),
        Some("stone") => Some(Material::Stone),
        Some("concrete") => Some(Material::Concrete),
        Some("steel") => Some(Material::Steel),
        Some("water") => Some(Material::Water),
        None => None,
        v => panic!("unknown material {v:?}"),
    }
}
fn material_name(v: Material) -> &'static str {
    match v {
        Material::Soil => "soil",
        Material::Road => "road",
        Material::Wood => "wood",
        Material::Stone => "stone",
        Material::Concrete => "concrete",
        Material::Steel => "steel",
        Material::Water => "water",
    }
}
fn bits(v: f64) -> String {
    format!("{:016x}", v.to_bits())
}
fn row(v: &Value) -> RememberedCell {
    let m = &v["metadata"];
    let s = &m["section"];
    let b = &v["coverBasis"];
    RememberedCell {
        cell: v["cell"].as_u64().unwrap() as usize,
        revision: v["revision"].as_u64().unwrap(),
        character: v["character"].as_str().unwrap().as_bytes()[0],
        height: v["height"].as_i64().unwrap() as i8,
        state: v["state"].as_u64().unwrap() as u8,
        metadata: CellMetadata {
            ground: m["ground"].as_str().map(|s| s.as_bytes()[0]),
            object: m["object"].as_str().map(|s| s.as_bytes()[0]),
            material: material(&m["material"]),
            section: (!s.is_null()).then(|| RememberedSection {
                hp: number(&s["hp"]),
                max_hp: s["maxHp"].as_f64(),
                failed: s["state"].as_str() == Some("failed"),
                material: material(&s["material"]),
            }),
            trench_front: m["trenchFront"].as_f64(),
            mine_owned: m["mineOwned"].as_bool().unwrap_or(false),
        },
        cover_basis: (!b.is_null()).then(|| CoverBasis {
            house_kind: b["houseKind"].as_u64().map(|v| v as u8),
            toughness: number(&b["toughness"]),
        }),
    }
}
#[test]
fn recipient_projection_matches_original_helpers() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/route-knowledge.json")).unwrap();
    assert_eq!(
        fixture["reference"],
        "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
    );
    assert_eq!(fixture["node"], "v24.21.0");
    assert_eq!(fixture["arch"], "x64");
    let mut count = 0;
    for case in fixture["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let i = &case["initial"];
        let w = i["width"].as_u64().unwrap() as usize;
        let h = i["height"].as_u64().unwrap() as usize;
        let chars = bytes(&i["chars"]);
        let ground = bytes(&i["ground"]);
        let objects = bytes(&i["objects"]);
        let heights: Vec<_> = i["heights"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_i64().unwrap() as i8)
            .collect();
        let shoals: Vec<_> = i["shoals"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_bool().unwrap())
            .collect();
        let houses: Vec<_> = i["houseKinds"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap() as u8)
            .collect();
        let toughness: Vec<_> = i["toughness"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| number(v) as f32)
            .collect();
        let fronts: Vec<_> = i["trenchFront"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_f64().map_or(f32::NAN, |v| v as f32))
            .collect();
        let rules = Rules::compiled().unwrap();
        let mut state = if case["mode"] == "World" {
            RouteKnowledge::world(w, h, 7, rules.clone()).unwrap()
        } else {
            RouteKnowledge::observed(
                InitialTerrain {
                    width: w,
                    height: h,
                    chars: &chars,
                    ground: Some(&ground),
                    objects: Some(&objects),
                    heights: Some(&heights),
                    shoals: Some(&shoals),
                    house_kinds: Some(&houses),
                    toughness: Some(&toughness),
                    trench_front: Some(&fronts),
                },
                7,
                rules.clone(),
            )
            .unwrap()
        };
        let m = &rules.movement;
        let path_rules = PathRules {
            wall_hug: 0.5,
            bunker_radius: rules
                .kinds
                .iter()
                .find(|k| k.name == "bunker")
                .unwrap()
                .radius,
            road_speed: m.road_speed,
            ford_speed: m.ford_speed,
            mud_speed: m.mud_speed,
            wood_speed: m.wood_speed,
            churn: m.churn,
            wet_ford: m.wet_ford,
            wet_ground: m.wet_ground,
        };
        let mut finder = Pathfinding::new();
        for (index, snapshot) in case["snapshots"].as_array().unwrap().iter().enumerate() {
            let rows: Vec<_> = snapshot["rows"]
                .as_array()
                .unwrap()
                .iter()
                .map(row)
                .collect();
            state
                .apply_rows(snapshot["version"].as_u64().unwrap(), &rows)
                .unwrap();
            let cells: Vec<Value> = (0..w * h)
                .map(|c| {
                    let v = state.cell(c).unwrap();
                    json!([
                        v.known,
                        (v.character as char).to_string(),
                        (v.ground as char).to_string(),
                        (v.object as char).to_string(),
                        v.flags,
                        v.height,
                        bits(f64::from(v.wear)),
                        v.near_wall,
                        v.mine_owned,
                        bits(f64::from(v.cell_hp.unwrap())),
                        bits(f64::from(v.trench_front)),
                        material_name(v.material)
                    ])
                })
                .collect();
            let expected: Vec<Value> = snapshot["expected"]["cells"]
                .as_array()
                .unwrap()
                .iter()
                .flat_map(|run| {
                    std::iter::repeat_n(run[0].clone(), run[1].as_u64().unwrap() as usize)
                })
                .collect();
            for (c, (a, b)) in cells.iter().zip(expected.iter()).enumerate() {
                assert_eq!(a, b, "{name} snapshot {index} cell {c}");
            }
            let query = state.query();
            let nav = query.navigation();
            let history: Vec<Value> = nav
                .changes
                .iter()
                .map(|c| json!([c.from, c.to, c.cells]))
                .collect();
            assert_eq!(
                json!(history),
                snapshot["expected"]["changes"],
                "{name} snapshot {index} history"
            );
            assert_eq!(
                json!(nav.versions.knowledge),
                snapshot["expected"]["version"],
                "{name} snapshot {index} version"
            );
            assert_eq!(
                json!(nav.changed.map(|c| json!([c.from, c.to, c.cells]))),
                snapshot["expected"]["direct"],
                "{name} snapshot {index} direct"
            );
            let wrecks: Vec<Wreck> = snapshot["wrecks"]
                .as_array()
                .unwrap()
                .iter()
                .map(|w| Wreck {
                    id: None,
                    cell: None,
                    x: number(&w["x"]),
                    z: number(&w["z"]),
                    moving: w["moving"].as_bool().unwrap(),
                })
                .collect();
            for q in snapshot["queries"].as_array().unwrap() {
                let at = point(&q["at"]);
                let from = point(&q["from"]);
                let c = q["cell"].as_u64().unwrap() as usize;
                assert_eq!(
                    material_name(state.cell(c).unwrap().ground_material),
                    q["expected"]["groundMaterial"],
                    "{name} snapshot {index} ground material"
                );
                assert_eq!(
                    bits(state.cover_q(c).unwrap()),
                    q["expected"]["cover"],
                    "{name} snapshot {index} cover"
                );
                assert_eq!(
                    json!(state.cover_rank(at, Some(from), &wrecks).unwrap()),
                    q["expected"]["rank"],
                    "{name} snapshot {index} rank"
                );
                assert_eq!(
                    bits(state.cover_behind(at, from, &wrecks).unwrap()),
                    q["expected"]["behind"],
                    "{name} snapshot {index} behind"
                );
            }
            let mut fires = vec![false; w * h];
            for c in snapshot["fires"].as_array().unwrap() {
                fires[c.as_u64().unwrap() as usize] = true;
            }
            for path in snapshot["paths"].as_array().unwrap() {
                let kind = rules
                    .kinds
                    .iter()
                    .find(|k| Some(k.name.as_str()) == path["type"].as_str())
                    .unwrap();
                let result = finder
                    .find_path(
                        query
                            .path_terrain(LiveRouting {
                                fires: &fires,
                                bunkers: &[],
                                wetness: 0.2,
                                roads: true,
                            })
                            .unwrap(),
                        path_rules,
                        kind,
                        point(&path["from"]),
                        point(&path["to"]),
                        None,
                    )
                    .unwrap();
                let status = match result.status {
                    PathStatus::Found => "Found",
                    PathStatus::Unreachable => "Unreachable",
                    PathStatus::ExpansionLimit => "ExpansionLimit",
                };
                let output = json!({"points":result.waypoints.iter().map(|p|[bits(p.x),bits(p.z)]).collect::<Vec<_>>(),"routeEnd":[bits(result.route_end.x),bits(result.route_end.z)],"status":status,"expansions":result.expansions});
                assert_eq!(
                    output, path["expected"],
                    "{name} snapshot{index} path {}",
                    kind.name
                );
            }
            count += 1;
        }
    }
    assert_eq!(count, 93);
}

#[test]
fn missing_world_cover_basis_stays_explicit_and_identity_skips_before_validation() {
    let rules = Rules::compiled().unwrap();
    let mut state = RouteKnowledge::world(3, 3, 2, rules).unwrap();
    assert_eq!(state.cover_q(4), Err(KnowledgeError::UnknownCell));
    let mut input = RememberedCell {
        cell: 4,
        revision: 1,
        character: b'B',
        height: 0,
        state: 0,
        metadata: CellMetadata {
            ground: Some(b'.'),
            object: Some(b'B'),
            ..CellMetadata::default()
        },
        cover_basis: None,
    };
    state.apply_rows(1, &[input]).unwrap();
    assert_eq!(state.cover_q(4), Err(KnowledgeError::MissingCoverBasis));
    input.character = b'?';
    input.metadata.trench_front = Some(f64::NAN);
    state.apply_rows(2, &[input]).unwrap();
    assert_eq!(state.cell(4).unwrap().character, b'B');
    input.revision = 2;
    input.character = b'B';
    input.metadata.trench_front = None;
    input.metadata.section = Some(RememberedSection {
        hp: 50.0,
        max_hp: Some(100.0),
        failed: false,
        material: Some(Material::Wood),
    });
    state.apply_rows(3, &[input]).unwrap();
    assert_eq!(state.cover_q(4), Ok(0.75));
    // A stale World memory version gates the entire pass, as m.pathVersion does.
    input.revision = 3;
    input.character = b'?';
    state.apply_rows(3, &[input]).unwrap();
    assert_eq!(state.cell(4).unwrap().character, b'B');
}

#[test]
fn row_batches_validate_atomically_and_skip_repeated_identity_in_order() {
    let initial = [b'.'; 9];
    let mut state = RouteKnowledge::observed(
        InitialTerrain {
            width: 3,
            height: 3,
            chars: &initial,
            ground: None,
            objects: None,
            heights: None,
            shoals: None,
            house_kinds: None,
            toughness: None,
            trench_front: None,
        },
        3,
        Rules::compiled().unwrap(),
    )
    .unwrap();
    let valid = RememberedCell {
        cell: 4,
        revision: 1,
        character: b'#',
        height: 0,
        state: 0,
        metadata: CellMetadata {
            ground: Some(b'.'),
            object: Some(b'#'),
            ..CellMetadata::default()
        },
        cover_basis: None,
    };
    let mut invalid = valid;
    invalid.cell = 5;
    invalid.character = b'?';
    assert_eq!(
        state.apply_rows(1, &[valid, invalid]),
        Err(KnowledgeError::InvalidValue)
    );
    assert_eq!(state.cell(4).unwrap().character, b'.');
    invalid.cell = 4;
    state.apply_rows(1, &[valid, invalid]).unwrap();
    assert_eq!(state.cell(4).unwrap().character, b'#');
    assert_eq!(state.query().navigation().versions.knowledge, 1);
}
