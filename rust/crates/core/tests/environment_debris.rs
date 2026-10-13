//! Exact body and public-row comparisons with the original debris helpers.
use serde_json::{Value, json};
use ww2_core::environment_debris::*;
fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/environment-debris.json")).unwrap()
}
fn n(value: &Value) -> f64 {
    if value == "-0" {
        -0.0
    } else {
        value.as_f64().unwrap()
    }
}
fn input(value: &Value) -> DebrisInput {
    let mut input = DebrisInput {
        id: value["id"].as_str().unwrap().into(),
        x: n(&value["x"]),
        y: n(&value["y"]),
        z: n(&value["z"]),
        ..Default::default()
    };
    if let Some(value) = value.get("kind") {
        input.kind = match value.as_str().unwrap() {
            "section" => DebrisKind::Section,
            "wreck" => DebrisKind::Wreck,
            _ => panic!("unknown fixture kind"),
        };
    }
    for (name, field) in [
        ("dir", &mut input.dir),
        ("impulse", &mut input.impulse),
        ("mass", &mut input.mass),
        ("radius", &mut input.radius),
        ("time", &mut input.time),
    ] {
        if let Some(value) = value.get(name) {
            *field = n(value);
        }
    }
    if let Some(value) = value.get("material") {
        input.material = value.as_str().unwrap().into();
    }
    input
}
fn material(value: &Value) -> Option<ContactMaterial> {
    value.as_object().map(|_| ContactMaterial {
        rebound: value.get("rebound").map(n),
        absorption: value.get("absorption").map(n),
    })
}
fn vector(value: &Value) -> Vec3 {
    Vec3::new(n(&value["x"]), n(&value["y"]), n(&value["z"]))
}
struct FixtureEnvironment<'a>(&'a Value);
impl Environment for FixtureEnvironment<'_> {
    fn ground(&mut self, x: f64, _: f64) -> f64 {
        self.0.get("floor").map_or(0.0, n) + x * self.0.get("slope").map_or(0.0, n)
    }
    fn material(&mut self, _: f64, _: f64) -> Option<ContactMaterial> {
        material(&self.0["material"])
    }
    fn contact(&mut self, from: Vec3, to: Vec3, _: &DebrisBody) -> Option<Contact> {
        if let Some(hit) = self.0.get("hit") {
            return Some(Contact {
                fraction: n(&hit["fraction"]),
                normal: vector(&hit["normal"]),
                material: material(&hit["material"]),
            });
        }
        if let Some(wall) = self.0.get("wall").map(n)
            && from.x <= wall
            && to.x > wall
        {
            return Some(Contact {
                fraction: (wall - from.x) / (to.x - from.x),
                normal: Vec3::new(-2.0, 0.0, 0.0),
                material: None,
            });
        }
        None
    }
}
fn row(value: DebrisRow) -> Value {
    json!({"id": value.id, "kind": value.kind.as_str(), "x": value.x, "y": value.y, "z": value.z, "dir": value.dir, "material": value.material, "mass": value.mass, "v": value.v, "gravity": value.gravity, "drag": value.drag, "time": value.time, "born": value.born, "tilt": value.tilt, "settled": value.settled})
}
fn body(value: &DebrisBody) -> Value {
    json!({"id": value.id, "kind": value.kind.as_str(), "x": value.x, "y": value.y, "z": value.z, "vx": value.vx, "vy": value.vy, "vz": value.vz, "dir": value.dir, "material": value.material, "mass": value.mass, "radius": value.radius, "origin": {"x": value.origin.x, "z": value.origin.z}, "gravity": value.gravity, "drag": value.drag, "stamp": value.stamp, "born": value.born, "contacts": value.contacts, "settled": value.settled, "tilt": value.tilt})
}
fn equal(actual: &Value, expected: &Value, label: &str) {
    match actual {
        Value::Number(number) => assert_eq!(
            format!("{:016x}", number.as_f64().unwrap().to_bits()),
            expected.as_str().unwrap(),
            "{label}"
        ),
        Value::Array(items) => {
            assert_eq!(items.len(), expected.as_array().unwrap().len(), "{label}");
            for (i, item) in items.iter().enumerate() {
                equal(item, &expected[i], &format!("{label}[{i}]"));
            }
        }
        Value::Object(fields) => {
            assert_eq!(fields.len(), expected.as_object().unwrap().len(), "{label}");
            for (key, item) in fields {
                equal(item, &expected[key], &format!("{label}.{key}"));
            }
        }
        _ => assert_eq!(actual, expected, "{label}"),
    }
}
fn snapshot(actual: &DebrisBody, expected: &Value, label: &str) {
    equal(&body(actual), &expected["body"], label);
    equal(
        &row(actual.row()),
        &expected["row"],
        &format!("{label}:row"),
    );
}
#[test]
fn constructor_defaults_mass_tables_and_rows_match_original_helpers() {
    let fixture = fixture();
    assert_eq!(
        fixture["reference"],
        "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
    );
    equal(
        &json!({"active": DEBRIS_LIMITS.active, "contacts": DEBRIS_LIMITS.contacts, "lifetime": DEBRIS_LIMITS.lifetime, "maxStep": DEBRIS_LIMITS.max_step}),
        &fixture["limits"],
        "limits",
    );
    for (name, mass) in SECTION_MASS {
        equal(&json!(mass), &fixture["sectionMass"][name], name);
        assert_eq!(section_mass(name), mass);
    }
    assert_eq!(section_mass("unknown"), 1200.0);
    for (i, case) in fixture["masses"].as_array().unwrap().iter().enumerate() {
        equal(
            &json!(wreck_mass(
                case["kind"].as_str().unwrap(),
                case.get("radius").map(n)
            )),
            &case["expected"],
            &format!("mass:{i}"),
        );
    }
    for (i, case) in fixture["constructors"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        snapshot(
            &DebrisBody::new(input(&case["input"])).unwrap(),
            case,
            &format!("constructor:{i}"),
        );
    }
}
#[test]
fn motion_contact_thresholds_and_settlement_match_original_helpers() {
    let fixture = fixture();
    for trace in fixture["traces"].as_array().unwrap() {
        let label = trace["label"].as_str().unwrap();
        let mut body = DebrisBody::new(input(&trace["input"])).unwrap();
        for (name, field) in [
            ("vx", &mut body.vx),
            ("vy", &mut body.vy),
            ("vz", &mut body.vz),
            ("gravity", &mut body.gravity),
            ("drag", &mut body.drag),
            ("born", &mut body.born),
        ] {
            if let Some(value) = trace["patch"].get(name) {
                *field = n(value);
            }
        }
        snapshot(&body, &trace["snapshots"][0], label);
        let mut env = FixtureEnvironment(&trace["env"]);
        for (i, action) in trace["actions"].as_array().unwrap().iter().enumerate() {
            match action["op"].as_str().unwrap() {
                "step" => body.step(n(&action["dt"]), &mut env).unwrap(),
                "settle" => body.settle(&mut env).unwrap(),
                _ => panic!("unknown fixture action"),
            }
            snapshot(&body, &trace["snapshots"][i + 1], &format!("{label}:{i}"));
        }
    }
}
