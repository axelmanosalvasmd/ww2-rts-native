//! Regressions for field assignment precedence and detached placement admission.
use super::tests::{acts, fixture, run, unit};
use super::*;
use crate::{commander_ai::PointObservation, commander_field_policy as field};

fn repair_scene() -> crate::commander_ai::Input {
    let mut i = fixture(vec![unit(1, "rifle", 40.0)]);
    i.manpower = 160.0;
    i.points = vec![PointObservation {
        at: Point { x: 40.0, z: 20.0 },
        owner: Some(0),
        vp: 1.0,
        mp: 0.0,
        cut: false,
        locked: false,
    }];
    i.runtime.as_mut().unwrap().field = Some(field::Facts {
        initial_chars: i.chars.clone(),
        ground: i.chars.clone(),
        ..field::Facts::default()
    });
    i.chars[10 * i.width + 21] = b'F';
    i
}

#[test]
fn intent_ai48_visible_centre_hidden_fill_span_approaches_and_retries_next_look() {
    let mut input = repair_scene();
    // Rifle sight is 36m. At 10%, the centre 3.16m away is visible, the far edge is not.
    input.runtime.as_mut().unwrap().sight = 0.1;
    let mut state = State::default();
    for _ in 0..2 {
        let commands = run(input.clone(), &mut state, Level::Normal);
        assert!(
            commands
                .iter()
                .any(|c| c["t"] == "dig" && c["kind"] == "fill")
        );
        assert!(
            acts(&commands, "move", 1),
            "partial-span refusal needs an approach: {commands:?}"
        );
        let approach = commands.iter().find(|c| c["t"] == "move").unwrap();
        assert_eq!(approach["orders"], json!([[1, 43.0, 21.0]]));
        input.tick += 40;
    }
}

#[test]
fn intent_ai48_classic_raid_cannot_replace_this_looks_paid_field_assignment() {
    let mut input = repair_scene();
    input.units.push(unit(2, "rifle", 30.0));
    let context = input.runtime.as_mut().unwrap();
    context.mode = Mode::Classic;
    context.known_buildings = vec![KnownBuilding {
        id: 90,
        kind: "depot".into(),
        owner: 1,
        at: Point { x: 120.0, z: 20.0 },
    }];
    let mut state = State {
        raid: Some(crate::commander_classic_rules::Raid {
            target: 90,
            ids: vec![1, 2],
            at: Point { x: 120.0, z: 20.0 },
        }),
        ..State::default()
    };
    let commands = run(input, &mut state, Level::Normal);
    assert!(
        commands
            .iter()
            .any(|c| c["t"] == "dig" && c["kind"] == "fill" && c["ids"] == json!([1]))
    );
    assert!(
        !acts(&commands, "amove", 1),
        "raid cancels field work: {commands:?}"
    );
    assert!(
        acts(&commands, "amove", 2),
        "unassigned raider still advances"
    );
}

#[test]
fn intent_ai48_existing_construction_builder_cannot_be_reassigned_to_field_work() {
    let mut input = repair_scene();
    input.units[0].build = true;
    let observation = Observation::new(input).unwrap();
    let mut commands = Vec::new();
    field::field_work(
        &observation,
        &mut field::State::default(),
        &mut BTreeSet::new(),
        |raw| {
            commands.push(raw);
            true
        },
    )
    .unwrap();
    assert!(
        commands.is_empty(),
        "construction must retain its worker: {commands:?}"
    );
}
