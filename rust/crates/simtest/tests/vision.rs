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

#[path = "vision/rows.rs"]
mod rows;

#[path = "vision/supplemental.rs"]
mod supplemental;

#[test]
fn cmb39_existing_tower_actor_sees_twelve_then_sixty_when_complete() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/vision-rows.json")).unwrap();
    let case = fixture["rangeCases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["name"] == "tower-unfinished")
        .unwrap();
    let rules = ww2_core::rules::Rules::compiled().unwrap();
    let mut loaded = load_game(&case["game"], &rules).unwrap();
    let id = case["unit"].as_u64().unwrap() as u32;
    assert_eq!(rules.kinds[loaded.game.units[&id].kind].name, "tower");
    // Use the recorded actor on clear ground, independent of weather/height.
    loaded.game.weather = None;
    for (built, reach) in [(0.99, 12.0_f64), (1.0, 60.0)] {
        loaded.game.units.get_mut(&id).unwrap().built = Some(built);
        assert_eq!(
            ww2_core::vision::vision_of(&loaded.game, &rules, &loaded.game.units[&id]).to_bits(),
            reach.to_bits(),
            "tower built={built}"
        );
    }
}
