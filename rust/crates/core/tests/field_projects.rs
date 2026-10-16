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
    let mut trench_patterns = std::collections::BTreeMap::new();
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
        assert!(
            jobs.len() <= 24,
            "patterns stay small enough to order at once: {input}"
        );
        let actual:Vec<_>=jobs.iter().map(|j|json!({"kind":j.kind.as_str(),"x":bits(j.at.x),"z":bits(j.at.z),"dir":bits(j.direction),"front":bits(j.front)})).collect();
        assert_eq!(json!(actual), case["jobs"], "{input}");
        if fort == FortKind::Trench {
            *trench_patterns
                .entry(input["pattern"].as_str().unwrap())
                .or_insert(0) += 1;
        }
        compared += 1;
    }
    assert_eq!(compared, 192);
    for pattern in ["line", "zigzag", "double", "arc", "ring"] {
        assert_eq!(trench_patterns.get(pattern), Some(&8), "{pattern}");
    }
    for case in fixture["costs"].as_array().unwrap() {
        let Ok(fort) = FortKind::parse(case["fort"].as_str().unwrap()) else {
            continue;
        };
        assert_eq!(
            segment_cost(fort, case["cells"].as_u64().unwrap() as usize).unwrap(),
            number(&case["cost"])
        );
    }

    // Pinned test.js's "Mass entrenchment" acceptance examples, alongside the
    // complete ordered bit comparisons above for default, boundary and cap cases.
    let a = Point { x: 20.0, z: 40.0 };
    let b = Point { x: 60.0, z: 40.0 };
    let back = Some(Point { x: 40.0, z: 50.0 });
    let plan = |pattern, to| entrench_plan(pattern, a, to, back, FortKind::Trench).unwrap();
    let line = plan(Pattern::Line, b);
    assert_eq!(line.len(), 5, "a 40 m line is five trench segments");
    assert_eq!(line[0].at, Point { x: 40.0, z: 40.0 });
    assert_eq!(
        plan(Pattern::Strongpoint, b)
            .iter()
            .map(|job| job.kind)
            .collect::<Vec<_>>(),
        vec![
            FortKind::Trench,
            FortKind::Trench,
            FortKind::Trench,
            FortKind::Trench,
            FortKind::Wire,
            FortKind::Wire,
        ],
        "a strongpoint is a trench square with wire in front"
    );
    assert!(
        line.iter()
            .all(|job| job.front.to_bits() == (-std::f64::consts::FRAC_PI_2).to_bits())
    );
    assert_eq!(
        plan(Pattern::Line, a).len(),
        1,
        "a plain click is one segment"
    );

    let double = plan(Pattern::Double, b);
    assert_eq!(double.len(), 10, "a double line is two rows");
    assert!(double.iter().any(|job| job.at.z == 46.0));
    assert!(double.iter().all(|job| job.at.z >= 40.0));

    let zigzag = plan(Pattern::Zigzag, b);
    assert_eq!(zigzag.len(), 6);
    let directions: std::collections::BTreeSet<_> = zigzag
        .iter()
        .map(|job| format!("{:.2}", job.direction))
        .collect();
    assert_eq!(
        directions,
        std::collections::BTreeSet::from(["-0.60".to_owned(), "0.60".to_owned()])
    );

    let ring = plan(Pattern::Ring, Point { x: 30.0, z: 40.0 });
    assert_eq!(ring.len(), 8, "a 10 m ring is eight segments");
    for job in ring {
        let dx = job.at.x - a.x;
        let dz = job.at.z - a.z;
        assert!((ww2_core::math::hypot(dx, dz) - 10.0).abs() < 1e-9);
        let outward = dx * ww2_core::math::cos(job.front) + dz * ww2_core::math::sin(job.front);
        assert!((outward - 10.0).abs() < 1e-9, "the ring faces out");
    }
    assert!(
        plan(Pattern::Arc, Point { x: 20.0, z: 20.0 })
            .iter()
            .all(|job| job.at.z < 40.0),
        "an arc bows toward the second point"
    );
}
