use super::*;
use crate::{
    map::AuthoritativeMap,
    weather::{RoadMap, road_mask},
};
use serde_json::Value;

use crate::shipped_map_fixtures as shipped;

fn expand<T>(runs: &Value, read: impl Fn(&Value) -> T) -> Vec<T>
where
    T: Clone,
{
    runs.as_array()
        .unwrap()
        .iter()
        .flat_map(|row| std::iter::repeat_n(read(&row[0]), row[1].as_u64().unwrap() as usize))
        .collect()
}

fn road_cells(terrain: &TerrainState) -> Vec<u8> {
    let rows: Vec<_> = terrain
        .chars
        .chunks(terrain.w)
        .map(|row| std::str::from_utf8(row).unwrap())
        .collect();
    road_mask(&RoadMap::new(terrain.w, terrain.h, &rows).unwrap())
}

#[test]
fn map57_every_shipped_centre_and_frozen_water_line_matches_original_relief() {
    let fixture: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/terrain-surface.json")).unwrap();
    assert_eq!(
        fixture["reference"],
        "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
    );
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), shipped::MAPS.len() + 4);
    for row in cases {
        let name = row["name"].as_str().unwrap();
        let map = if let Some(key) = name.strip_prefix("authored:") {
            let source = shipped::MAPS
                .iter()
                .find(|(name, _)| *name == key)
                .unwrap()
                .1;
            AuthoritativeMap::from_json(source.as_bytes()).unwrap()
        } else {
            AuthoritativeMap::from_json(&serde_json::to_vec(&row["map"]).unwrap()).unwrap()
        };
        let mut terrain = map.initialize_terrain().unwrap();
        let frozen = FrozenSurface::new(&terrain).unwrap();
        if row.get("grid").is_some() {
            terrain.chars = row["grid"]
                .as_array()
                .unwrap()
                .iter()
                .flat_map(|row| row.as_str().unwrap().bytes())
                .collect();
            terrain.height = Some(
                row["liveHeights"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .flat_map(|row| {
                        row.as_str().unwrap().bytes().map(|ch| match ch {
                            b'a' => -1,
                            b'b' => -2,
                            _ => (ch - b'0') as i8,
                        })
                    })
                    .collect(),
            );
        }
        let samples = frozen
            .sample_centres(&terrain, &road_cells(&terrain))
            .unwrap();
        let expected_height = expand(&row["height"], |v| v.as_u64().unwrap() as u32);
        assert_eq!(samples.height.len(), expected_height.len(), "{name}");
        for (c, (actual, expected)) in samples.height.iter().zip(expected_height).enumerate() {
            assert_eq!(actual.to_bits(), expected, "{name}: centre {c}");
        }
        let expected_water = expand(&row["water"], |v| v.as_f64());
        assert_eq!(
            samples.water_line, expected_water,
            "{name}: frozen water lines"
        );
        if name == "synthetic:trenches-roads-cliffs" {
            let mut expected = vec![0; 400];
            expected[220..240].fill(2);
            assert_eq!(
                samples.cliffs, expected,
                "the three-level step has twenty south cliffs"
            );
        }
        if name == "synthetic:channels-fords-bridge" {
            assert_eq!(&samples.channel_depth[4..9], &[0, 1, 2, 3, 3]);
            assert_eq!(
                &samples.channel_depth[185..195],
                &[0; 10],
                "bridge deck remains uncarved"
            );
        }
    }
}

#[test]
fn map57_surface_rejects_mismatched_inputs_before_sampling() {
    let map = AuthoritativeMap::from_json(shipped::MAPS[0].1.as_bytes()).unwrap();
    let mut terrain = map.initialize_terrain().unwrap();
    let frozen = FrozenSurface::new(&terrain).unwrap();
    let roads = road_cells(&terrain);
    assert!(
        frozen
            .sample_centres(&terrain, &roads[..roads.len() - 1])
            .is_err()
    );
    let mut invalid_roads = roads.clone();
    invalid_roads[0] = 2;
    assert!(frozen.sample_centres(&terrain, &invalid_roads).is_err());
    terrain.chars.pop();
    assert!(frozen.sample_centres(&terrain, &roads).is_err());
    assert!(FrozenSurface::new(&terrain).is_err());
}
