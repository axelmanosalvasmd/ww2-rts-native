//! Movement and crowd separation give the reference JavaScript's answers, call by call. `fixtures/movement.json`
//! is two corners of the frozen 1,000-unit state recorded for three ticks by `harness/movement-fixture.mjs`, with
//! changes that make the rules the frozen workload never reaches decide something (roads, mud, woods, fords,
//! wire, wet ground, cliffs, walled-in vehicles, sidesteps and passing legs, ghosts, a one-lane stretch, pace,
//! sprint, backing up, riders, bridges in separation); its `cases` list them.

use simtest::replay::replay;

#[test]
fn movement_matches_the_reference_call_by_call() {
    let fixture = simtest::parse(include_str!("fixtures/movement.json")).expect("fixture");
    let report = replay(&fixture).expect("replay");
    assert_eq!(report.mismatch, None);
    assert_eq!((report.ticks, report.calls, report.units), (3, 636, 1362));
}
