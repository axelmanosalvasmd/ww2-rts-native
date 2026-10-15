//! MAP56's finite query boundary matrix, using unchanged original capture rows.
use serde_json::Value;
use std::{collections::BTreeSet, fs, path::PathBuf};

fn decode(bits: &Value) -> f64 {
    let bits = bits.as_str().expect("hexadecimal binary64 input");
    assert_eq!(bits.len(), 16);
    let number = f64::from_bits(u64::from_str_radix(bits, 16).unwrap());
    assert!(number.is_finite());
    number
}

#[test]
fn one_thousand_source_samples_match_existing_native_height_queries() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let fixture: Value = serde_json::from_slice(
        &fs::read(root.join("rust/crates/simtest/tests/fixtures/map-height-selected.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(fixture["schemaVersion"], 1);
    assert_eq!(
        fixture["reference"],
        "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
    );
    assert_eq!(
        fixture["selectionBaseline"],
        "446671b977abfa6bf689fcb8853833d36519faab"
    );
    assert_eq!(fixture["selection"]["totalQueries"], 1000);
    let maps = fixture["maps"].as_array().unwrap();
    assert_eq!(maps.len(), 33);
    let mut count = 0;
    let mut coverage = [0_u64; 9];
    let mut unique = BTreeSet::new();
    for (map_index, source) in maps.iter().enumerate() {
        let key = source["mapKey"].as_str().unwrap();
        if map_index > 0 {
            let previous = maps[map_index - 1]["sourceCapture"]["path"].as_str().unwrap();
            let current = source["sourceCapture"]["path"].as_str().unwrap();
            assert!(previous < current);
        }
        let map: Value = serde_json::from_str(source["mapJson"].as_str().unwrap()).unwrap();
        let queries = source["queries"].as_array().unwrap();
        assert_eq!(queries.len(), if map_index < 10 { 31 } else { 30 });
        let inputs = queries
            .iter()
            .map(|query| {
                let bits = query["inputBits"].as_array().unwrap();
                assert_eq!(bits.len(), 2);
                (decode(&bits[0]), decode(&bits[1]))
            })
            .collect::<Vec<_>>();
        // Native receives only complete authoritative map text and input coordinates.
        // Original query outputs remain comparison data and never initialize the scene.
        let actual = simtest::terrain_fixtures::produce(key, &map, &inputs).unwrap();
        let original: Value = serde_json::from_slice(
            &fs::read(root.join(source["sourceCapture"]["path"].as_str().unwrap())).unwrap(),
        )
        .unwrap();
        assert_eq!(map, original["map"]);
        assert_eq!(source["sourceCapture"]["producer"], original["producer"]);
        assert_eq!(original["producer"]["kind"], "original-javascript");
        assert_eq!(original["producer"]["referenceSha"], fixture["reference"]);
        assert_eq!(actual["cellMetres"], 2.0);
        assert_eq!(actual["metresPerLevel"], 2.5);
        let points = actual["points"].as_array().unwrap();
        assert_eq!(points.len(), queries.len());
        let width = 2.0 * map["w"].as_f64().unwrap();
        let height = 2.0 * map["h"].as_f64().unwrap();
        for (index, (query, point)) in queries.iter().zip(points).enumerate() {
            assert_eq!(query["sourcePointIndex"].as_u64().unwrap(), index as u64);
            assert!(unique.insert((key.to_owned(), index)));
            let expected = query["expectedBits"].as_array().unwrap();
            let old = original["points"][index].as_array().unwrap();
            let row = point.as_array().unwrap();
            assert_eq!(row.len(), 5);
            assert_eq!(expected.len(), 5);
            assert_eq!(old.len(), 5);
            for column in 0..5 {
                let bits = u64::from_str_radix(expected[column].as_str().unwrap(), 16).unwrap();
                assert_eq!(old[column].as_f64().unwrap().to_bits(), bits);
                assert_eq!(
                    row[column].as_f64().unwrap().to_bits(),
                    bits,
                    "{key} source point {index}, column {column}"
                );
            }
            let (x, z) = inputs[index];
            assert_eq!(row[0].as_f64().unwrap().to_bits(), x.to_bits());
            assert_eq!(row[1].as_f64().unwrap().to_bits(), z.to_bits());
            let tags = [
                x < 0.0 || z < 0.0,
                x < 0.0 || z < 0.0 || x >= width || z >= height,
                x == 0.0 || x == width || z == 0.0 || z == height,
                x % 2.0 == 1.0 && z % 2.0 == 1.0,
                x % 2.0 == 0.0 || z % 2.0 == 0.0,
                row[2].as_f64().unwrap() < 0.0,
                row[3].as_f64().unwrap() != row[4].as_f64().unwrap(),
                map.get("heights").is_some(),
                row[3].as_f64().unwrap() != row[4].as_f64().unwrap()
                    && x > 1.0
                    && x < width - 1.0
                    && z > 1.0
                    && z < height - 1.0
                    && x % 2.0 != 1.0
                    && z % 2.0 != 1.0,
            ];
            for (total, tag) in coverage.iter_mut().zip(tags) {
                *total += u64::from(tag);
            }
            count += 1;
        }
    }
    assert_eq!(count, 1000);
    assert_eq!(unique.len(), 1000);
    let names = [
        "negative-coordinate",
        "outside-half-open",
        "exact-map-boundary",
        "cell-centre",
        "cell-edge",
        "negative-level",
        "interpolated-distinct-from-step",
        "height-layer-present",
        "interior-bilinear-slope",
    ];
    assert_eq!(coverage, [66, 296, 132, 67, 453, 40, 97, 940, 13]);
    for (name, count) in names.iter().zip(coverage) {
        assert_eq!(fixture["coverage"][*name], count);
    }
    // Existing external producer rejects these before any raw gameplay query.
    // This is a native harness boundary check, not Source parity for NaN/infinity.
    let first: Value = serde_json::from_str(maps[0]["mapJson"].as_str().unwrap()).unwrap();
    let refused = [
        (f64::NAN, 0.0),
        (f64::INFINITY, 0.0),
        (f64::NEG_INFINITY, 0.0),
        (0.0, f64::NAN),
        (0.0, f64::INFINITY),
        (0.0, f64::NEG_INFINITY),
    ];
    for input in refused {
        let error = simtest::terrain_fixtures::produce("boundary", &first, &[input]).unwrap_err();
        assert_eq!(error, "terrain fixture point must be finite");
    }
}
