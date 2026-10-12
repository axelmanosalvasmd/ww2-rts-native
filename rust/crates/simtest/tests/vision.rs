//! The vision and fog passes give the reference JavaScript's answers, on one thread and on one thread per
//! team. `fixtures/vision.json` is the bench match at tick 6,000 cut down to what the passes read, with
//! changes that make each rule decide an answer (smoke, dust, trenches, camouflage, woods, recon flights, a
//! ship's hull, a building's aim point), and the lines `harness/vision-check.mjs` printed for it
//! (`harness/vision-fixture.mjs` writes it).

use simtest::check::check;
use simtest::load::{load_game, load_rules};

#[test]
fn vision_and_fog_match_the_reference() {
    let fixture = simtest::parse(include_str!("fixtures/vision.json")).expect("fixture");
    let rules = load_rules(&fixture["tables"]).expect("tables");
    let expected: Vec<&str> = fixture["expected"]
        .as_array()
        .expect("expected lines")
        .iter()
        .map(|line| line.as_str().expect("a line"))
        .collect();
    for threaded in [false, true] {
        let mut loaded = load_game(&fixture["game"], &rules).expect("game");
        let lines = check(&mut loaded.game, &mut loaded.fog, &rules, threaded);
        assert_eq!(lines, expected, "threaded: {threaded}");
    }
}
