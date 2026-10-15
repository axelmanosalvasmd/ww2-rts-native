//! Movement and crowd separation give the reference JavaScript's answers, call by call. `fixtures/movement.json`
//! is two corners of the frozen 1,000-unit state recorded for three ticks by `harness/movement-fixture.mjs`, with
//! changes that make the rules the frozen workload never reaches decide something (roads, mud, woods, fords,
//! wire, wet ground, cliffs, walled-in vehicles, sidesteps and passing legs, ghosts, a one-lane stretch, pace,
//! sprint, backing up, riders, bridges in separation); its `cases` list them.

use simtest::replay::replay;
use ww2_core::game::Point;
use ww2_core::rules::{Rules, VehicleProfile};
use ww2_core::vehicle::vehicle_step;

#[test]
fn movement_matches_the_reference_call_by_call() {
    let fixture = simtest::parse(include_str!("fixtures/movement.json")).expect("fixture");
    assert_eq!(fixture["reference"], "e4e579a");
    assert_eq!(fixture["node"], "v24.21.0");
    assert_eq!(fixture["arch"], "x64");
    let recorded = simtest::load::load_rules(&fixture["tables"]).unwrap();
    let compiled = Rules::compiled().unwrap();
    let mut unit = simtest::load::unit(
        &fixture["game"]["units"]["$map"][0][1],
        &compiled,
        fixture["game"]["players"].as_array().unwrap().len(),
        fixture["game"]["w"].as_u64().unwrap() as usize
            * fixture["game"]["h"].as_u64().unwrap() as usize,
    )
    .unwrap();
    unit.x = 30.0;
    unit.z = 30.0;
    unit.rot = 0.0;
    unit.move_speed = 0.0;
    unit.travel_dir = None;
    let bits = |p: &VehicleProfile| {
        [
            p.acceleration,
            p.reverse_acceleration,
            p.braking,
            p.reverse_speed,
            p.hull_turn,
        ]
        .map(f64::to_bits)
    };
    let mut profiles = Vec::new();
    for expected in &recorded.kinds {
        let index = compiled.kind(&expected.name).unwrap();
        let kind = &compiled.kinds[index];
        assert_eq!(
            kind.vehicle.as_ref().map(bits),
            expected.vehicle.as_ref().map(bits),
            "{} profile differs from the Source recording",
            kind.name
        );
        let Some(profile) = &kind.vehicle else {
            continue;
        };
        profiles.push(kind.name.as_str());
        unit.kind = index;
        // Original test-movement-lab.js:45-53, including the recorded fallback kinds.
        for reverse in [false, true] {
            let path = [Point {
                x: if reverse { 10.0 } else { 80.0 },
                z: 30.0,
            }];
            let next = vehicle_step(
                &unit,
                &compiled,
                profile,
                &path,
                kind.speed,
                compiled.movement.tick,
                reverse,
            );
            let acceleration = if reverse {
                profile.reverse_acceleration
            } else {
                profile.acceleration
            };
            assert!(
                next.move_speed.abs() <= acceleration * compiled.movement.tick + 1e-9,
                "{} reverse={reverse}: starts within its acceleration limit",
                kind.name
            );
            assert!(
                if reverse {
                    next.move_speed < 0.0
                } else {
                    next.move_speed > 0.0
                },
                "{} reverse={reverse}: starts in the commanded direction",
                kind.name
            );
        }
    }
    profiles.sort_unstable();
    assert_eq!(
        compiled
            .kinds
            .iter()
            .filter(|kind| kind.vehicle.is_some())
            .count(),
        profiles.len()
    );
    assert_eq!(
        profiles,
        [
            "armoredcar",
            "churchill",
            "flaktrack",
            "halftrack",
            "kaiju",
            "lorry",
            "medium",
            "rocket",
            "tank",
            "tankdestroyer",
            "tiger",
            "train",
            "truck",
        ]
    );
    let report = replay(&fixture).expect("replay");
    assert_eq!(report.mismatch, None);
    assert_eq!((report.ticks, report.calls, report.units), (3, 636, 1362));
}
