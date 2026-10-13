use serde_json::{Value, json};
use ww2_core::{
    field_works::{FortKind, Pattern, entrench_plan, segment_cost},
    game::Point,
};
fn number(v: &Value) -> f64 {
    f64::from_bits(u64::from_str_radix(v.as_str().unwrap(), 16).unwrap())
}
fn point(v: &Value) -> Point {
    Point {
        x: number(&v["x"]),
        z: number(&v["z"]),
    }
}
fn bits(value: f64) -> String {
    format!("{:016x}", value.to_bits())
}
#[test]
fn original_shared_pattern_order_front_and_segment_prices() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/mass-entrenchment.json")).unwrap();
    let mut compared = 0;
    for case in fixture["plans"].as_array().unwrap() {
        let input = &case["input"];
        let Ok(fort) = FortKind::parse(input["fort"].as_str().unwrap()) else {
            continue;
        };
        let pattern = Pattern::parse(input["pattern"].as_str().unwrap()).unwrap();
        let jobs = entrench_plan(
            pattern,
            point(&input["a"]),
            point(&input["b"]),
            (!input["back"].is_null()).then(|| point(&input["back"])),
            fort,
        )
        .unwrap();
        let actual:Vec<_>=jobs.iter().map(|j|json!({"kind":j.kind.as_str(),"x":bits(j.at.x),"z":bits(j.at.z),"dir":bits(j.direction),"front":bits(j.front)})).collect();
        assert_eq!(json!(actual), case["jobs"], "{input}");
        compared += 1;
    }
    assert_eq!(compared, 168);
    for case in fixture["costs"].as_array().unwrap() {
        let Ok(fort) = FortKind::parse(case["fort"].as_str().unwrap()) else {
            continue;
        };
        assert_eq!(
            segment_cost(fort, case["cells"].as_u64().unwrap() as usize).unwrap(),
            number(&case["cost"])
        );
    }
}
