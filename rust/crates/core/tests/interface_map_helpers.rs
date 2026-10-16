use serde_json::{Value, json};
use std::collections::BTreeMap;
use ww2_core::{
    game::Point,
    map::{
        Building, HouseBlock, Layers, compose_world_cell, house_kinds_where, inside_known_region,
        level_char, level_of, territory_edges, world_layers,
    },
    match_world_rails::{RAIL_DIRS, rail_yard},
    weather::{MapWeatherInfo, RoadMap, WeatherKind, WeatherRules, map_mood, road_mask},
};

mod authored {
    include!("../../server/src/map-data.rs");
}

fn fixture() -> Value {
    let value: Value =
        serde_json::from_str(include_str!("fixtures/interface-map-helpers.json")).unwrap();
    assert_eq!(
        value["reference"],
        "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
    );
    assert_eq!(value["node"], "v26.11.1");
    assert_eq!(value["arch"], "x64");
    value
}

fn runs(values: impl IntoIterator<Item = Value>) -> Value {
    let mut out: Vec<(Value, usize)> = Vec::new();
    for value in values {
        if let Some(last) = out.last_mut().filter(|last| last.0 == value) {
            last.1 += 1;
        } else {
            out.push((value, 1));
        }
    }
    json!(out)
}

fn materials(value: &Value) -> BTreeMap<usize, String> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            (
                row[0].as_u64().unwrap() as usize,
                row[1].as_str().unwrap().into(),
            )
        })
        .collect()
}

fn check_layers(layers: &Layers, expected: &Value) {
    let char_runs = |chars: &[u8]| runs(chars.iter().map(|&ch| json!(char::from(ch).to_string())));
    assert_eq!(char_runs(&layers.ground), expected["ground"]);
    assert_eq!(char_runs(&layers.objects), expected["objects"]);
    assert_eq!(
        runs(layers.mines.iter().map(|&n| json!(n))),
        expected["mineLayer"]
    );
    assert_eq!(layers.materials, materials(&expected["materials"]));
    assert_eq!(layers.base_materials, materials(&expected["baseMaterials"]));
    assert_eq!(
        layers.object_materials,
        materials(&expected["objectMaterials"])
    );
    assert_eq!(layers.mine_materials, materials(&expected["mineMaterials"]));
}

fn buildings(map: &Value) -> Vec<Building> {
    map["buildings"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|b| Building {
            x: b["x"].as_u64().unwrap() as usize,
            y: b["y"].as_u64().unwrap() as usize,
            kind: b["kind"].as_str().unwrap().into(),
        })
        .collect()
}

fn block_json(blocks: Vec<HouseBlock>) -> Value {
    json!(
        blocks
            .into_iter()
            .map(|block| json!({"cells":block.cells,"kind":block.kind}))
            .collect::<Vec<_>>()
    )
}

fn integers<const N: usize>(value: &Value) -> Vec<[i64; N]> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|row| std::array::from_fn(|i| row[i].as_i64().unwrap()))
        .collect()
}
fn cells(value: &Value) -> Vec<usize> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_u64().unwrap() as usize)
        .collect()
}
fn signed_cells(value: &Value) -> Vec<i64> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_i64().unwrap())
        .collect()
}

#[test]
fn all_33_received_maps_match_pinned_presentation_helpers() {
    let f = fixture();
    let expected = f["maps"].as_array().unwrap();
    assert_eq!(expected.len(), 33);
    assert_eq!(authored::MAPS.len(), expected.len());
    let weather = WeatherRules::compiled().unwrap();
    for &(key, document) in authored::MAPS {
        let expected = expected.iter().find(|entry| entry["key"] == key).unwrap();
        let map: Value = serde_json::from_str(document).unwrap();
        let width = map["w"].as_u64().unwrap() as usize;
        let height = map["h"].as_u64().unwrap() as usize;
        let rows: Vec<_> = map["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r.as_str().unwrap())
            .collect();
        let chars = rows.join("").into_bytes();
        let layers = world_layers(&map).unwrap();
        check_layers(&layers, &expected["layers"]);
        assert!(
            layers.mines.iter().all(|&mine| mine == 0),
            "{key}: received maps have no hidden mines"
        );
        let composed: Vec<_> = (0..chars.len())
            .map(|c| compose_world_cell(layers.ground[c], layers.objects[c], layers.mines[c] != 0))
            .collect();
        assert_eq!(composed, chars, "{key}: composition");
        let blocks = house_kinds_where(
            |c| chars[c] == b'B',
            chars.len(),
            width,
            &buildings(&map),
            None,
        )
        .unwrap();
        assert_eq!(
            block_json(blocks),
            expected["houses"],
            "{key}: house blocks"
        );
        if let Some(heights) = map["heights"].as_array() {
            let levels = heights
                .iter()
                .flat_map(|row| row.as_str().unwrap().bytes())
                .map(|ch| {
                    let level = level_of(ch).unwrap();
                    assert_eq!(level_char(level), Some(ch));
                    json!(level)
                });
            assert_eq!(runs(levels), expected["levels"], "{key}: levels");
        } else {
            assert!(expected["levels"].is_null());
        }
        let roads = road_mask(&RoadMap::new(width, height, &rows).unwrap());
        assert_eq!(
            runs(roads.into_iter().map(|bit| json!(bit))),
            expected["roads"],
            "{key}: roads"
        );
        let info = MapWeatherInfo {
            name: map["name"].as_str(),
            mood: map["mood"].as_str(),
            weather: map["weather"].as_str(),
        };
        assert_eq!(
            map_mood(info, Some(key)).name(),
            expected["mood"].as_str().unwrap()
        );
        let plan = weather.map_weather(info, Some(key));
        assert_eq!(
            json!({"now":plan.now.name(),"next":plan.next.map(WeatherKind::name),"at":plan.at}),
            expected["weather"]
        );
        assert_eq!(
            json!(weather.weather_effects(Some(plan.now))),
            expected["effects"]
        );
        let yard = rail_yard(
            width as i64,
            signed_cells(&expected["track"]),
            signed_cells(&expected["stations"]),
        );
        assert_eq!(json!(yard), expected["yard"], "{key}: yard");
        let region_runs = integers::<3>(&expected["runs"]);
        assert_eq!(
            json!(territory_edges(&region_runs)),
            expected["edges"],
            "{key}: edges"
        );
        let known: Vec<[usize; 3]> = region_runs.iter().map(|r| r.map(|n| n as usize)).collect();
        for (point, inside) in expected["points"]
            .as_array()
            .unwrap()
            .iter()
            .zip(expected["inside"].as_array().unwrap())
        {
            assert_eq!(
                inside_known_region(
                    Some(&known),
                    None,
                    Point {
                        x: point["x"].as_f64().unwrap(),
                        z: point["z"].as_f64().unwrap()
                    },
                    2.0
                ),
                inside.as_bool().unwrap(),
                "{key}: known region"
            );
        }
    }
    assert_eq!(json!(RAIL_DIRS), f["railDirs"]);
}

#[test]
fn discovered_house_seeds_and_named_overrides_match_pinned_helpers() {
    let f = fixture();
    for case in f["houseCases"].as_array().unwrap() {
        let map = &case["map"];
        let chars = map["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r.as_str().unwrap())
            .collect::<String>()
            .into_bytes();
        let known = case["known"].as_array().map(|_| cells(&case["known"]));
        let seeds = case["seeds"].as_array().map(|_| cells(&case["seeds"]));
        let blocks = house_kinds_where(
            |c| chars[c] == b'B' && known.as_ref().is_none_or(|known| known.contains(&c)),
            chars.len(),
            map["w"].as_u64().unwrap() as usize,
            &buildings(map),
            seeds.as_deref(),
        )
        .unwrap();
        assert_eq!(block_json(blocks), case["expected"]);
    }
}

#[test]
fn layer_material_precedence_height_codec_and_composition_match_pinned_helpers() {
    let f = fixture();
    for case in f["layerCases"].as_array().unwrap() {
        check_layers(&world_layers(&case["map"]).unwrap(), &case["expected"]);
    }
    for case in f["levels"].as_array().unwrap() {
        assert_eq!(
            level_of(case["ch"].as_str().unwrap().as_bytes()[0]),
            Some(case["level"].as_i64().unwrap() as i8)
        );
        assert_eq!(
            level_char(case["level"].as_i64().unwrap() as i8),
            Some(case["encoded"].as_str().unwrap().as_bytes()[0])
        );
    }
    for row in f["compositions"].as_array().unwrap() {
        assert_eq!(
            compose_world_cell(
                row[0].as_str().unwrap().as_bytes()[0],
                row[1].as_str().unwrap().as_bytes()[0],
                row[2].as_bool().unwrap()
            ),
            row[3].as_str().unwrap().as_bytes()[0]
        );
    }
}

#[test]
fn known_region_boundaries_and_border_segment_order_match_pinned_helpers() {
    let f = fixture();
    for case in f["regionCases"].as_array().unwrap() {
        let runs = case["region"]["runs"].as_array().map(|_| {
            integers::<3>(&case["region"]["runs"])
                .into_iter()
                .map(|r| r.map(|n| n as usize))
                .collect::<Vec<_>>()
        });
        let bounds = case["region"]["bounds"]
            .as_array()
            .map(|b| std::array::from_fn(|i| b[i].as_f64().unwrap()));
        let at = Point {
            x: case["at"]["x"].as_f64().unwrap(),
            z: case["at"]["z"].as_f64().unwrap(),
        };
        assert_eq!(
            inside_known_region(runs.as_deref(), bounds, at, case["cell"].as_f64().unwrap()),
            case["expected"].as_bool().unwrap()
        );
    }
    for case in f["edgeCases"].as_array().unwrap() {
        assert_eq!(
            json!(territory_edges(&integers::<3>(&case["runs"]))),
            case["expected"]
        );
    }
    // Yard values retain duplicate suppression, signed off-map cells and source insertion order.
    assert_eq!(rail_yard(3, [8, 8, -1], []), vec![8, -1]);
    let yard = rail_yard(3, [], [0, 0]);
    assert_eq!(yard.first(), Some(&-20));
    assert_eq!(yard.last(), Some(&20));
    assert_eq!(yard.len(), 41);
}
