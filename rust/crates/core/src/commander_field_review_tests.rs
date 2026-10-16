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

fn typed_world(input: &mut crate::commander_ai::Input) {
    use crate::commander_world_policy::{Facts, Region};
    let context = input.runtime.as_mut().unwrap();
    context.mode = Mode::World;
    context.world = Some(Facts {
        regions: vec![
            Region {
                id: 0,
                at: Point { x: 40.0, z: 40.0 },
                team: Some(0),
                locked: false,
                runs: None,
                bounds: Some([0.0, 0.0, 100.0, 80.0]),
            },
            Region {
                id: 1,
                at: Point { x: 140.0, z: 40.0 },
                team: Some(1),
                locked: false,
                runs: None,
                bounds: Some([100.0, 0.0, 160.0, 80.0]),
            },
        ],
        visible_cells: (0..input.flags.len()).collect(),
        ..Facts::default()
    });
}

#[test]
fn intent_ai58_typed_world_preserves_shared_rush_escape_support_and_busy_orders() {
    for level in [Level::Normal, Level::Hard] {
        let mut input = fixture(
            (1..=6)
                .map(|id| unit(id, "rifle", 40.0 + id as f64))
                .collect(),
        );
        input.tick = 4000;
        input.points.clear();
        input.players[1].spawn = Point { x: 140.0, z: 30.0 };
        input.units.extend([
            unit(20, "hq", 60.0),
            unit(21, "engineer", 70.0),
            unit(32, "barracks", 50.0),
        ]);
        for (id, x) in [(30, 90.0), (31, 86.0)] {
            let mut enemy = unit(id, "rifle", x);
            enemy.owner = 1;
            input.units.push(enemy);
        }
        input.units.sort_by_key(|u| u.id);
        typed_world(&mut input);
        let context = input.runtime.as_mut().unwrap();
        for id in [20, 21, 32] {
            context.units.insert(
                id,
                UnitFacts {
                    built: 1.0,
                    ..UnitFacts::default()
                },
            );
        }
        context.build_reserve = 500.0;
        context.fort_reserve = 500.0;
        context.munitions = 2000.0;
        context.support.insert("artillery".into(), 0.0);
        input.manpower = 2000.0;
        let sightings: Vec<_> = [(30, 90.0), (31, 86.0)]
            .into_iter()
            .map(|(id, x)| Sighting {
                id,
                kind: "rifle".into(),
                owner: 1,
                at: Point { x, z: 20.0 },
                time: 199.0,
                worth: 100.0,
            })
            .collect();
        let commands = plan(
            &Observation::new(input).unwrap(),
            &sightings,
            &mut State::default(),
            level,
            &mut MatchRandom::new(51),
        )
        .unwrap();
        assert!(
            commands
                .iter()
                .any(|c| c["t"] == "move" && c["orders"] == json!([[21, 10.0, 20.0]])),
            "{commands:?}"
        );
        assert!(
            commands.iter().any(|c| c["t"] == "support"
                && c["kind"] == "artillery"
                && c["x"] == 88.0
                && c["z"] == 20.0),
            "{commands:?}"
        );
        for id in 1..=6 {
            let goals: Vec<_> = commands
                .iter()
                .filter(|c| c["t"] == "amove")
                .flat_map(|c| c["orders"].as_array().unwrap())
                .filter(|o| o[0] == id)
                .cloned()
                .collect();
            assert_eq!(
                goals,
                vec![json!([id, 88.0, 20.0])],
                "region movement must preserve rush assignment: {commands:?}"
            );
        }
    }
}

#[test]
fn intent_ai58_typed_world_retreat_precedes_region_movement_for_wounded_units() {
    let mut input = fixture(vec![unit(1, "rifle", 80.0), unit(10, "hq", 10.0)]);
    input.points.clear();
    typed_world(&mut input);
    let healthy = run(input.clone(), &mut State::default(), Level::Normal);
    assert!(
        acts(&healthy, "amove", 1),
        "healthy squad advances on region: {healthy:?}"
    );
    input.units[0].hp = 20.0;
    let wounded = run(input, &mut State::default(), Level::Normal);
    assert!(acts(&wounded, "retreat", 1), "{wounded:?}");
    assert!(
        !acts(&wounded, "amove", 1),
        "region order cancels retreat: {wounded:?}"
    );
}
