//! Complete post-initialization Node export and schema refusal correspondence.
use serde_json::{Value as Json, json};
use ww2_core::{
    catalog::{Catalog, Value},
    startup_tables,
};

fn expected() -> Json {
    serde_json::from_slice(include_bytes!("fixtures/startup-tables.json")).unwrap()
}
fn value_at<'a>(root: &'a Value, path: &str) -> &'a Value {
    path.split('/').skip(1).fold(root, |value, key| {
        let key = key.replace("~1", "/").replace("~0", "~");
        match value {
            Value::Object(values) => &values[&key],
            Value::Array(values) => &values[key.parse::<usize>().unwrap()],
            _ => panic!("invalid recorded path {path}"),
        }
    })
}
#[test]
fn complete_node_exports_keep_every_field_number_and_exact_bytes() {
    let produced = startup_tables::compiled_json().unwrap();
    assert_eq!(produced, include_bytes!("fixtures/startup-tables.json"));
    assert_eq!(
        produced,
        include_bytes!("../../../../docs/contracts/startup-tables/proposed-content.json")
    );
    let native = startup_tables::parse_json(produced).unwrap();
    let numbers: Json =
        serde_json::from_slice(include_bytes!("fixtures/startup-table-values.json")).unwrap();
    assert_eq!(
        numbers["contentSha256"],
        "5de90a7990ec5e8a2903436ac4809302464be29b546d79591a2adc08c68a2efd"
    );
    assert_eq!(numbers["numbers"].as_array().unwrap().len(), 1743);
    for number in numbers["numbers"].as_array().unwrap() {
        let path = number["path"].as_str().unwrap();
        let expected = u64::from_str_radix(number["bits"].as_str().unwrap(), 16).unwrap();
        assert_eq!(
            value_at(&native, path).number().unwrap().to_bits(),
            expected,
            "{path}"
        );
    }
    assert!(std::ptr::eq(
        produced,
        startup_tables::compiled_json().unwrap()
    ));
    assert_eq!(native.get("tables").unwrap().object().unwrap().len(), 64);
}
#[test]
fn schema_refuses_unknown_version_missing_fields_and_malformed_tags_atomically() {
    let mut bad = expected();
    bad["schemaVersion"] = json!(2);
    let error = startup_tables::parse_json(&serde_json::to_vec(&bad).unwrap()).unwrap_err();
    assert_eq!(error.path, "/schemaVersion");
    assert!(error.reason.contains("expected 1"));
    for version in [Json::Bool(true), json!("1")] {
        let mut bad = expected();
        bad["schemaVersion"] = version;
        assert!(startup_tables::parse_json(&serde_json::to_vec(&bad).unwrap()).is_err());
    }
    for bad_tag in [
        json!({"$number":"Infinity","other":0}),
        json!({"$number":"infinity"}),
        json!({"$number":0}),
    ] {
        let mut bad = expected();
        bad["tables"]["CFG"]["houses"][3]["min"] = bad_tag;
        assert!(startup_tables::parse_json(&serde_json::to_vec(&bad).unwrap()).is_err());
    }
    let mut bad = expected();
    bad["tables"]["UNITS"]["rifle"]
        .as_object_mut()
        .unwrap()
        .remove("w");
    assert!(startup_tables::parse_json(&serde_json::to_vec(&bad).unwrap()).is_err());
    assert!(startup_tables::parse_json(b"{invalid").is_err());
    // Failed reads cannot change the sole immutable compiled buffer.
    assert_eq!(
        startup_tables::compiled_json().unwrap(),
        include_bytes!("fixtures/startup-tables.json")
    );
}
#[test]
fn catalog_producer_preserves_tags_absence_null_false_and_zero_without_defaults() {
    let base_text = include_str!("../data/rules.json");
    let extra_text = include_str!("../data/startup-table-additions.json");
    for (tag, bits) in [
        ("NaN", f64::NAN.to_bits()),
        ("Infinity", f64::INFINITY.to_bits()),
        ("-Infinity", f64::NEG_INFINITY.to_bits()),
        ("-0", (-0.0_f64).to_bits()),
    ] {
        let mut base: Json = serde_json::from_str(base_text).unwrap();
        base["tables"]["CFG"]["houses"][3]["min"] = json!({"$number":tag});
        let base = Catalog::from_json(&base.to_string()).unwrap();
        let extra = Catalog::from_json(extra_text).unwrap();
        let produced = startup_tables::from_catalogs(&base, &extra).unwrap();
        let decoded = startup_tables::parse_json(&produced).unwrap();
        assert_eq!(
            value_at(&decoded, "/tables/CFG/houses/3/min")
                .number()
                .unwrap()
                .to_bits(),
            bits
        );
    }
    let decoded = startup_tables::parse_json(startup_tables::compiled_json().unwrap()).unwrap();
    // Complete exact-byte comparison above also pins all missing/null/bool/zero leaves.
    let unit = value_at(&decoded, "/tables/UNITS/rifle");
    assert!(unit.get("lookout").is_none());
    assert_eq!(
        value_at(&decoded, "/tables/MOVEMENT_PROFILES/airfield"),
        &Value::Null
    );
    assert_eq!(
        value_at(&decoded, "/tables/MOVEMENT_PROFILES/armoredcar/tracked").boolean(),
        Some(false)
    );
    assert_eq!(
        value_at(&decoded, "/tables/CFG/houses/0/min").number(),
        Some(0.)
    );
    let base = Catalog::from_json(base_text).unwrap();
    assert!(startup_tables::from_catalogs(&base, &base).is_err());
    let mut invalid: Json = serde_json::from_str(extra_text).unwrap();
    invalid["format"] = json!(2);
    let invalid = Catalog::from_json(&invalid.to_string()).unwrap();
    assert!(startup_tables::from_catalogs(&base, &invalid).is_err());
    let mut invalid: Json = serde_json::from_str(extra_text).unwrap();
    invalid["reference"] = json!("unknown");
    let invalid = Catalog::from_json(&invalid.to_string()).unwrap();
    assert!(startup_tables::from_catalogs(&base, &invalid).is_err());
}
