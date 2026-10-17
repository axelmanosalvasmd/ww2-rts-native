use serde_json::Value;
use ww2_core::{
    projectiles::{Flight, Point3, flight_at, velocity_at},
    rules::Rules,
};

#[test]
fn if11_vehicle_presentation_and_delivered_flights_keep_source_fields() {
    // The existing startup fixture contains the complete pinned Node exports.
    let recorded: Value =
        serde_json::from_slice(include_bytes!("fixtures/startup-tables.json")).unwrap();
    let tables = &recorded["tables"];
    let rules = Rules::compiled().unwrap();
    let types = tables["UNIT_TYPES"].as_array().unwrap();
    assert_eq!(rules.kinds.len(), types.len());
    for (kind, name) in rules.kinds.iter().zip(types) {
        let name = name.as_str().unwrap();
        assert_eq!(kind.name, name);
        let def = &tables["UNITS"][name];
        let ground = !["infantry", "structure", "air", "naval"]
            .iter()
            .any(|key| def[*key].as_bool().unwrap_or(false));
        assert_eq!(kind.ground_vehicle(), ground, "{name}");
        let radius = def["radius"].as_f64().unwrap_or(1.0) * 0.8;
        assert_eq!(kind.body_radius().to_bits(), radius.to_bits(), "{name}");
        let expected = &tables["MOVEMENT_PROFILES"][name];
        assert_eq!(kind.vehicle.is_some(), ground, "{name}");
        if let Some(profile) = &kind.vehicle {
            for (actual, field) in [
                (profile.acceleration, "acceleration"),
                (profile.reverse_acceleration, "reverseAcceleration"),
                (profile.braking, "braking"),
                (profile.reverse_speed, "reverseSpeed"),
                (profile.hull_turn, "hullTurn"),
            ] {
                assert_eq!(
                    actual.to_bits(),
                    expected[field].as_f64().unwrap().to_bits(),
                    "{name}.{field}"
                );
            }
            assert_eq!(
                profile.tracked,
                expected["tracked"].as_bool().unwrap(),
                "{name}"
            );
        } else {
            assert!(expected.is_null(), "{name}");
        }
    }
    for name in ["lorry", "train", "kaiju"] {
        let fallback = rules.kinds.iter().find(|kind| kind.name == name).unwrap();
        assert!(fallback.vehicle.as_ref().unwrap().tracked);
        assert_eq!(fallback.vehicle.as_ref().unwrap().acceleration, 4.0);
    }

    // Source client/projectiles.js uses the delivered flight's time, not socket arrival time.
    let delivered = serde_json::json!({
        "x": -4.0, "y": 1.5, "z": 3.0,
        "v": [8.0, 6.0, -2.0], "gravity": 25.0, "time": 12.0
    });
    let flight = Flight {
        origin: Point3::new(
            delivered["x"].as_f64().unwrap(),
            delivered["y"].as_f64().unwrap(),
            delivered["z"].as_f64().unwrap(),
        ),
        velocity: Point3::new(
            delivered["v"][0].as_f64().unwrap(),
            delivered["v"][1].as_f64().unwrap(),
            delivered["v"][2].as_f64().unwrap(),
        ),
        gravity: delivered["gravity"].as_f64().unwrap(),
        stamp: delivered["time"].as_f64().unwrap(),
    };
    // Exact binary-fraction results from pinned flightAt/velocityAt.
    for (time, position, velocity) in [
        (11.0, [-4.0, 1.5, 3.0], [8.0, 6.0, -2.0]),
        (12.0, [-4.0, 1.5, 3.0], [8.0, 6.0, -2.0]),
        (12.125, [-3.0, 2.0546875, 2.75], [8.0, 2.875, -2.0]),
        (12.5, [0.0, 1.375, 2.0], [8.0, -6.5, -2.0]),
    ] {
        let p = flight_at(flight, time).unwrap();
        let v = velocity_at(flight, time).unwrap();
        assert_eq!(
            [p.x, p.y, p.z].map(f64::to_bits),
            position.map(f64::to_bits)
        );
        assert_eq!(
            [v.x, v.y, v.z].map(f64::to_bits),
            velocity.map(f64::to_bits)
        );
    }
}
