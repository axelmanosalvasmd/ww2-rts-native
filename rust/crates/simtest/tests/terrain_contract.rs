//! All authored terrain heights against original JavaScript query witnesses.
//! The source fixture is input/oracle evidence, not a native-produced export.
use serde_json::{Value, json};
use std::{fs, path::PathBuf};

fn equal_bits(actual: &Value, expected: &Value, context: &str) {
    match (actual, expected) {
        (Value::Number(a), Value::Number(e)) => assert_eq!(
            a.as_f64().unwrap().to_bits(),
            e.as_f64().unwrap().to_bits(),
            "{context}: {a} != {e}"
        ),
        (Value::Array(a), Value::Array(e)) => {
            assert_eq!(a.len(), e.len(), "{context}: array length");
            for (index, (a, e)) in a.iter().zip(e).enumerate() {
                equal_bits(a, e, &format!("{context}[{index}]"));
            }
        }
        (Value::Object(a), Value::Object(e)) => {
            assert_eq!(a.len(), e.len(), "{context}: object field count");
            for (key, e) in e {
                equal_bits(&a[key], e, &format!("{context}.{key}"));
            }
        }
        _ => assert_eq!(actual, expected, "{context}"),
    }
}

#[test]
fn all_original_authored_maps_every_centre_and_1000_points() {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../docs/fixtures/terrain/source-authored");
    let mut files = fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|e| e == "json"))
        .collect::<Vec<_>>();
    files.sort();
    assert_eq!(files.len(), 33);
    let mut cells = 0;
    for file in files {
        let source: Value = serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
        assert_eq!(source["schemaVersion"], 1);
        assert_eq!(source["producer"]["kind"], "original-javascript");
        assert_eq!(
            source["producer"]["referenceSha"],
            "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
        );
        assert_eq!(
            source["producer"]["authoringBaseline"],
            "efb677b6b0b2ac043415c47631dcacb0005b925d"
        );
        assert_eq!(source["producer"]["node"], "v24.21.0");
        assert_eq!(source["producer"]["arch"], "x64");
        assert_eq!(source["heightSemantics"]["renderedMesh"], false);
        let key = source["mapKey"].as_str().unwrap();
        assert_eq!(
            file.file_name().unwrap().to_str().unwrap(),
            format!("{key}.json")
        );
        let width = source["map"]["w"].as_u64().unwrap();
        let height = source["map"]["h"].as_u64().unwrap();
        assert_eq!(
            source["centres"].as_array().unwrap().len() as u64,
            width * height
        );
        assert_eq!(source["points"].as_array().unwrap().len(), 1000);
        assert_eq!(source["sampler"]["pointsPerMap"], 1000);
        assert_eq!(source["sampler"]["draws"], 1968);
        // Only raw authoritative map data and independent point coordinates enter
        // the native producer. Recorded heights are used solely in this comparison.
        let inputs = source["points"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| {
                assert_eq!(row.as_array().unwrap().len(), 5);
                (row[0].as_f64().unwrap(), row[1].as_f64().unwrap())
            })
            .collect::<Vec<_>>();
        let actual = simtest::terrain_fixtures::produce(key, &source["map"], &inputs).unwrap();
        let expected = json!({
            "schemaVersion":source["schemaVersion"],
            "mapKey":source["mapKey"],
            "sourceFilename":file.file_name().unwrap().to_str().unwrap(),
            "dimensions":{"widthCells":width,"heightCells":height},
            "cellMetres":source["cellMetres"],
            "metresPerLevel":source["metresPerLevel"],
            "centreColumns":source["centreColumns"],
            "pointColumns":source["pointColumns"],
            "centres":source["centres"],
            "points":source["points"],
        });
        equal_bits(&actual, &expected, key);
        cells += width * height;
    }
    assert_eq!(cells, 661_799);
}
