//! Bit-level oracle results come from calls to the original JavaScript helpers.
use serde_json::Value;
use ww2_core::projectiles::*;
fn n(v: &Value) -> f64 {
    v.as_f64().unwrap()
}
fn point(v: &Value) -> Point3 {
    Point3::new(n(&v["x"]), n(&v["y"]), n(&v["z"]))
}
fn body(v: &Value) -> BodyPosition {
    BodyPosition {
        x: n(&v["x"]),
        z: n(&v["z"]),
    }
}
fn hull(v: &Value) -> HullPosition {
    HullPosition {
        x: n(&v["x"]),
        z: n(&v["z"]),
        rotation: v.get("rot").map(n),
    }
}
fn range(v: &Value) -> [f64; 2] {
    [n(&v[0]), n(&v[1])]
}
fn bits(actual: f64, expected: &Value, label: &str) {
    assert_eq!(
        format!("{:016x}", actual.to_bits()),
        expected.as_str().unwrap(),
        "{label}"
    );
}
fn vector(actual: Point3, expected: &Value, label: &str) {
    bits(actual.x, &expected["x"], label);
    bits(actual.y, &expected["y"], label);
    bits(actual.z, &expected["z"], label);
}
fn contact(actual: Option<ContactHit>, expected: &Value, label: &str) {
    if expected.is_null() {
        assert_eq!(actual, None, "{label}");
    } else {
        let actual = actual.unwrap_or_else(|| panic!("missing contact: {label}"));
        bits(actual.fraction, &expected["fraction"], label);
        vector(actual.normal, &expected["normal"], label);
    }
}
fn profile(actual: ProjectileProfile, expected: &Value, label: &str) {
    if let Some(speed) = actual.speed {
        bits(speed, &expected["speed"], label);
    } else {
        assert!(expected.get("speed").is_none(), "{label}");
    }
    bits(actual.gravity, &expected["gravity"], label);
    bits(
        f64::from(actual.max_contacts),
        &expected["maxContacts"],
        label,
    );
    bits(actual.near, &expected["near"], label);
}
fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/projectiles.json")).unwrap()
}
fn check(operations: &[&str]) {
    let fixture = fixture();
    assert_eq!(
        fixture["reference"],
        "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
    );
    for row in fixture["cases"].as_array().unwrap() {
        let op = row["op"].as_str().unwrap();
        if !operations.contains(&op) {
            continue;
        }
        let a = &row["args"];
        let e = &row["expected"];
        let label = format!("{op} {}", row["label"]);
        match op {
            "projectileProfile" => profile(
                projectile_profile(
                    a[0].as_str().unwrap(),
                    a[1]["flame"].as_bool().unwrap(),
                    n(&a[1]["veh"]),
                    a[1]["shellTerrain"].as_bool().unwrap(),
                )
                .unwrap(),
                e,
                &label,
            ),
            "flightAt" | "velocityAt" => {
                let p = Flight {
                    origin: point(&a[0]),
                    velocity: Point3::new(n(&a[0]["vx"]), n(&a[0]["vy"]), n(&a[0]["vz"])),
                    gravity: n(&a[0]["gravity"]),
                    stamp: n(&a[0]["stamp"]),
                };
                vector(
                    if op == "flightAt" {
                        flight_at(p, n(&a[1]))
                    } else {
                        velocity_at(p, n(&a[1]))
                    }
                    .unwrap(),
                    e,
                    &label,
                );
            }
            "launchSolution" => {
                let p = launch_solution(
                    point(&a[0]),
                    point(&a[1]),
                    n(&a[2]),
                    n(&a[3]),
                    a.get(4).filter(|v| !v.is_null()).map(n),
                )
                .unwrap();
                vector(p.origin, e, &label);
                bits(p.velocity.x, &e["vx"], &label);
                bits(p.velocity.y, &e["vy"], &label);
                bits(p.velocity.z, &e["vz"], &label);
                bits(p.gravity, &e["gravity"], &label);
                bits(p.duration, &e["duration"], &label);
            }
            "sweepBox" => contact(
                sweep_box(
                    point(&a[0]),
                    point(&a[1]),
                    Box3 {
                        x: range(&a[2]["x"]),
                        y: range(&a[2]["y"]),
                        z: range(&a[2]["z"]),
                    },
                )
                .unwrap(),
                e,
                &label,
            ),
            "sweepBody" => contact(
                sweep_body(
                    point(&a[0]),
                    point(&a[1]),
                    body(&a[2]),
                    body(&a[3]),
                    n(&a[4]),
                    n(&a[5]),
                    n(&a[6]),
                )
                .unwrap(),
                e,
                &label,
            ),
            "sweepHull" => contact(
                sweep_hull(
                    point(&a[0]),
                    point(&a[1]),
                    hull(&a[2]),
                    hull(&a[3]),
                    n(&a[4]),
                    n(&a[5]),
                    n(&a[6]),
                    n(&a[7]),
                )
                .unwrap(),
                e,
                &label,
            ),
            "segmentDistance" => bits(
                segment_distance(body(&a[0]), body(&a[1]), body(&a[2])).unwrap(),
                e,
                &label,
            ),
            "crossedCells" => {
                let cells = crossed_cells(
                    body(&a[0]),
                    body(&a[1]),
                    n(&a[2]),
                    a[3].as_u64().unwrap() as usize,
                    a[4].as_u64().unwrap() as usize,
                )
                .unwrap();
                assert_eq!(cells.len(), e.as_array().unwrap().len(), "{label}");
                for (actual, expected) in cells.into_iter().zip(e.as_array().unwrap()) {
                    bits(actual as f64, expected, &label);
                }
            }
            "contactResponse" => {
                let p = ContactState {
                    explosive: a[0]["explosive"].as_bool().unwrap(),
                    sequence: a[0]["sequence"].as_u64().unwrap() as u32,
                    max_contacts: a[0]["maxContacts"].as_u64().unwrap() as u32,
                    energy: n(&a[0]["energy"]),
                };
                let material = ContactMaterial {
                    name: "fixture",
                    hardness: n(&a[3]["hardness"]),
                    absorption: n(&a[3]["absorption"]),
                    rebound: n(&a[3]["rebound"]),
                };
                let result = contact_response(p, point(&a[1]), point(&a[2]), material).unwrap();
                if e.is_null() {
                    assert_eq!(result, None, "{label}");
                } else {
                    let result = result.unwrap();
                    bits(result.energy, &e["energy"], &label);
                    bits(result.velocity.x, &e["vx"], &label);
                    bits(result.velocity.y, &e["vy"], &label);
                    bits(result.velocity.z, &e["vz"], &label);
                }
            }
            _ => panic!("unknown operation {op}"),
        }
    }
}
#[test]
fn profile_selection_and_constants_match_reference() {
    check(&["projectileProfile"]);
    let f = fixture();
    for (name, value) in [
        ("small", SMALL),
        ("sniper", SNIPER),
        ("gun", GUN),
        ("flame", FLAME),
        ("grenade", GRENADE),
        ("artillery", ARTILLERY),
    ] {
        profile(value, &f["profiles"][name], name);
    }
    for value in [SOIL, TIMBER, STONE, STEEL] {
        let e = &f["materials"][value.name];
        assert_eq!(e["name"], value.name);
        bits(value.hardness, &e["hardness"], value.name);
        bits(value.absorption, &e["absorption"], value.name);
        bits(value.rebound, &e["rebound"], value.name);
    }
}
#[test]
fn flight_and_launch_match_reference_at_and_before_stamp() {
    check(&["flightAt", "velocityAt", "launchSolution"]);
}
#[test]
fn swept_contacts_match_reference_boundaries_movement_and_rotation() {
    check(&["sweepBox", "sweepBody", "sweepHull"]);
}
#[test]
fn cells_and_segment_distance_match_reference_corner_order() {
    check(&["crossedCells", "segmentDistance"]);
}
#[test]
fn contact_response_matches_reference_ricochet_thresholds() {
    check(&["contactResponse"]);
}
#[test]
fn invalid_inputs_fail_explicitly_before_unbounded_or_nonfinite_work() {
    let p = Point3::new(0.0, 0.0, 0.0);
    let b = BodyPosition { x: 0.0, z: 0.0 };
    let valid_box = Box3 {
        x: [-1.0, 1.0],
        y: [-1.0, 1.0],
        z: [-1.0, 1.0],
    };
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let q = Point3::new(bad, 0.0, 0.0);
        assert_eq!(sweep_box(q, p, valid_box), Err(GeometryError::NonFinite));
        assert_eq!(
            sweep_body(q, p, b, b, 1.0, -1.0, 1.0),
            Err(GeometryError::NonFinite)
        );
        assert_eq!(
            segment_distance(BodyPosition { x: bad, z: 0.0 }, b, b),
            Err(GeometryError::NonFinite)
        );
        assert_eq!(
            launch_solution(p, q, 100.0, 9.81, None),
            Err(GeometryError::NonFinite)
        );
        assert_eq!(
            projectile_profile("rifle", false, bad, false),
            Err(GeometryError::NonFinite)
        );
    }
    assert_eq!(
        sweep_box(
            p,
            p,
            Box3 {
                x: [2.0, 1.0],
                ..valid_box
            }
        ),
        Err(GeometryError::InvalidDimensions)
    );
    assert_eq!(
        sweep_body(p, p, b, b, -1.0, -1.0, 1.0),
        Err(GeometryError::InvalidDimensions)
    );
    assert_eq!(
        sweep_body(p, p, b, b, 1.0, 1.0, -1.0),
        Err(GeometryError::InvalidDimensions)
    );
    for duration in [0.0, -1.0] {
        assert_eq!(
            launch_solution(p, p, 100.0, 9.81, Some(duration)),
            Err(GeometryError::InvalidDuration)
        );
    }
    assert_eq!(
        launch_solution(p, p, 0.0, 0.0, None),
        Err(GeometryError::InvalidDuration)
    );
    assert_eq!(
        crossed_cells(b, b, 0.0, 4, 4),
        Err(GeometryError::InvalidDimensions)
    );
    assert_eq!(
        crossed_cells(b, b, 1.0, usize::MAX, 4),
        Err(GeometryError::GridTooLarge)
    );
    let huge = HullPosition {
        x: 0.0,
        z: 0.0,
        rotation: Some(1e100),
    };
    let zero = HullPosition {
        rotation: Some(0.0),
        ..huge
    };
    assert_eq!(
        sweep_hull(p, p, zero, huge, 1.0, 4.0, -1.0, 1.0),
        Err(GeometryError::ExcessiveRotation)
    );
    assert_eq!(
        sweep_box(
            Point3::new(f64::MAX, 0.0, 0.0),
            Point3::new(-f64::MAX, 0.0, 0.0),
            valid_box
        ),
        Err(GeometryError::NonFinite)
    );
}
