use serde_json::{Value, json};
use ww2_core::{
    game::MOVE,
    map::{AuthoritativeMap, MAX_MAP_BYTES, MapError},
    navigation::{Navigation, Options, TerrainView, Versions},
};

fn runs(values: impl IntoIterator<Item = Value>) -> Value {
    let mut out: Vec<[Value; 2]> = Vec::new();
    for value in values {
        if let Some(last) = out.last_mut().filter(|last| last[0] == value) {
            last[1] = json!(last[1].as_u64().unwrap() + 1);
        } else {
            out.push([value, json!(1)]);
        }
    }
    json!(out)
}
fn float_runs(values: &[f32]) -> Value {
    runs(values.iter().map(|v| json!(v.to_bits())))
}
fn materials(values: &std::collections::BTreeMap<usize, String>) -> Value {
    json!(
        values
            .iter()
            .map(|(c, m)| json!([c, m]))
            .collect::<Vec<_>>()
    )
}

#[test]
fn authoritative_maps_match_pinned_validation_and_static_terrain() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/authoritative-maps.json")).unwrap();
    assert_eq!(
        fixture["reference"],
        "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
    );
    assert_eq!(fixture["node"], "v24.21.0");
    assert_eq!(fixture["arch"], "x64");
    let (mut authored, mut terrain_count, mut rejected, mut unsupported) = (0, 0, 0, 0);
    for case in fixture["cases"].as_array().unwrap() {
        let mut case = case.clone();
        if case["encoding"] == "world-runs" {
            fn expand(v: &Value) -> Vec<Value> {
                v.as_array()
                    .unwrap()
                    .iter()
                    .flat_map(|r| {
                        std::iter::repeat_n(r[0].clone(), r[1].as_u64().unwrap() as usize)
                    })
                    .collect()
            }
            let width = case["map"]["w"].as_u64().unwrap() as usize;
            for key in ["rows", "heights"] {
                if !case["map"][key].is_null() {
                    let chars = expand(&case["map"][key]);
                    case["map"][key] = json!(
                        chars
                            .chunks(width)
                            .map(|r| r.iter().map(|v| v.as_str().unwrap()).collect::<String>())
                            .collect::<Vec<_>>()
                    );
                }
            }
            for key in ["regionMap", "biomes"] {
                if !case["map"]["world"][key].is_null() {
                    case["map"]["world"][key] = json!(expand(&case["map"]["world"][key]));
                }
            }
            for key in ["chars", "ground", "objects"] {
                case["terrain"][key] = json!(
                    expand(&case["terrain"][key])
                        .iter()
                        .map(|v| v.as_str().unwrap())
                        .collect::<String>()
                );
            }
        }
        let name = case["name"].as_str().unwrap();
        let result = AuthoritativeMap::from_json(&serde_json::to_vec(&case["map"]).unwrap());
        if let Some(feature) = case["unsupported"].as_str() {
            assert!(
                matches!(result, Err(MapError::Unsupported { feature: f }) if f == feature),
                "{name}: {result:?}"
            );
            unsupported += 1;
            continue;
        }
        if !case["error"].is_null() {
            assert!(
                matches!(result, Err(MapError::Invalid { .. })),
                "{name}: {result:?}"
            );
            rejected += 1;
            continue;
        }
        let definition = result.unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(definition.source(), &case["map"], "{name}: retained source");
        assert_eq!(definition.name(), case["map"]["name"].as_str().unwrap());
        assert_eq!(
            definition.spawns().len(),
            case["map"]["spawns"].as_array().unwrap().len()
        );
        assert_eq!(
            definition.points().len(),
            case["map"]["points"].as_array().unwrap().len()
        );
        assert_eq!(
            definition.dimensions(),
            (
                case["map"]["w"].as_u64().unwrap() as usize,
                case["map"]["h"].as_u64().unwrap() as usize
            )
        );
        for (spawn, source) in definition
            .spawns()
            .iter()
            .zip(case["map"]["spawns"].as_array().unwrap())
        {
            assert_eq!(
                (spawn.x, spawn.y, spawn.assault),
                (
                    source["x"].as_u64().unwrap() as usize,
                    source["y"].as_u64().unwrap() as usize,
                    source["assault"].as_bool().unwrap_or(false)
                )
            );
        }
        for (point, source) in definition
            .points()
            .iter()
            .zip(case["map"]["points"].as_array().unwrap())
        {
            assert_eq!(
                (point.vp, point.mp),
                (
                    source["vp"].as_f64().unwrap_or(1.0),
                    source["mp"].as_f64().unwrap_or(1.0)
                )
            );
            assert_eq!(point.needs, source["needs"].as_u64().map(|v| v as usize));
        }
        if let Some(g) = definition.geography() {
            let source = &case["map"]["world"];
            assert_eq!(g.region_cells, 64);
            assert_eq!(g.regions.len(), source["total"].as_u64().unwrap() as usize);
            for (r, v) in g.regions.iter().zip(source["regions"].as_array().unwrap()) {
                assert_eq!(
                    (r.id, r.x, r.y, r.home),
                    (
                        v["id"].as_i64().unwrap() as i32,
                        v["x"].as_u64().unwrap() as usize,
                        v["y"].as_u64().unwrap() as usize,
                        v["home"].as_u64().map(|v| v as usize)
                    )
                );
                assert_eq!(json!(r.bounds), v["bounds"]);
                assert_eq!(r.biome.as_deref(), v["biome"].as_str());
            }
            if let Some(m) = &g.region_map {
                assert_eq!(json!(m), source["regionMap"]);
            }
            if let Some(b) = &g.biomes {
                assert_eq!(json!(b), source["biomes"]);
            }
            assert_eq!(g.seed, source["seed"].as_u64().map(|v| v as u32));
            assert_eq!(
                g.generator_version,
                source["generatorVersion"].as_u64().map(|v| v as u32)
            );
            for (key, value) in &g.metadata {
                assert_eq!(value, &source[key]);
            }
        }
        let t = definition.initialize_terrain().unwrap();
        let actual = json!({
            "chars": String::from_utf8(t.chars.clone()).unwrap(),
            "ground": String::from_utf8(t.layers.ground.clone()).unwrap(),
            "objects": String::from_utf8(t.layers.objects.clone()).unwrap(),
            "mines": runs(t.layers.mines.iter().map(|v| json!(v))),
            "materials": materials(&t.layers.materials), "baseMaterials": materials(&t.layers.base_materials),
            "objectMaterials": materials(&t.layers.object_materials), "mineMaterials": materials(&t.layers.mine_materials),
            "flags": runs(t.flags.iter().map(|v| json!(v))), "house": runs(t.house.iter().map(|v| json!(v))),
            "height": t.height.as_ref().map(|v| runs(v.iter().map(|v| json!(v)))),
            "cellHp": float_runs(&t.cell_hp), "wear": float_runs(&t.wear), "surfaceWear": float_runs(&t.surface_wear),
            "trenchFront": t.trench_front.as_ref().map(|v| float_runs(v)),
            "tough": t.tough.as_ref().map(|v| runs(v.iter().map(|v| if v.fract() == 0.0 { json!(*v as i64) } else { json!(v) }))),
            "shoal": t.shoal.as_ref().map(|v| runs(v.iter().map(|v| json!(v)))), "roads": t.roads, "naval": t.naval,
        });
        for (field, expected) in case["terrain"].as_object().unwrap() {
            assert!(
                actual[field] == *expected,
                "{name}: {field} differs from the reference"
            );
        }
        if name.starts_with("boundary:world-generated") {
            let loaded = ww2_core::world::World::from_map(&definition).unwrap();
            assert_eq!(loaded.geography(), definition.geography());
            assert_eq!(loaded.game.chars, t.chars);
            assert_eq!(loaded.game.flags, t.flags);
            assert_eq!(loaded.game.height, t.height);
            assert_eq!(loaded.mine_layer, t.layers.mines);
            assert!(loaded.game.players.is_empty() && loaded.game.units.is_empty());
            for structure in &loaded.structures {
                for &cell in &structure.cells {
                    assert!(cell < t.w * t.h);
                    assert!(loaded.game.sections[&(cell as u32)].hp.is_finite());
                }
            }
        }
        let game = t.query_game();
        assert!(game.players.is_empty() && game.units.is_empty() && game.sections.is_empty());
        assert_eq!(game.flags, t.flags);
        assert_eq!(game.height, t.height);
        assert_eq!(game.w * game.h, game.flags.len());
        if name == "boundary:layered-material-precedence" {
            assert_eq!(t.layers.mines[22], 1);
            assert_eq!(game.physical(22), b'T');
            assert_eq!(t.layers.mine_materials[&21], "wood");
            assert_eq!(t.layers.object_materials[&22], "stone");
            let mut navigation = Navigation::new();
            let view = TerrainView {
                width: game.w,
                height: game.h,
                flags: &game.flags,
                heights: game.height.as_deref(),
                world_known: false,
                knowledge_id: 1,
                versions: Versions::default(),
                changed: None,
                changes: &[],
            };
            assert!(
                navigation
                    .leg(view, 0, 399, MOVE, Options::default())
                    .unwrap()
                    .connected
            );
        }
        authored += usize::from(name.starts_with("authored:"));
        terrain_count += 1;
    }
    assert_eq!(
        (authored, terrain_count, rejected, unsupported),
        (33, 58, 79, 1)
    );
}

#[test]
fn malformed_and_oversized_documents_return_errors() {
    for bytes in [b"".as_slice(), b"{", b"{\"w\":NaN}", b"{\"w\":1e999}"] {
        assert!(matches!(
            AuthoritativeMap::from_json(bytes),
            Err(MapError::Invalid { .. })
        ));
    }
    assert!(matches!(
        AuthoritativeMap::from_json(&vec![b' '; MAX_MAP_BYTES + 1]),
        Err(MapError::Invalid { .. })
    ));
}
