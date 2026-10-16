use super::{tests::*, *};
use crate::commander_ai::{PointObservation, PolicyError};

#[test]
fn if25_ai_point_scores_keep_strict_scan_semantics_when_finite_inputs_overflow() {
    let mut input = fixture(vec![unit(1, "rifle", 40.0)]);
    input.points[0].vp = 1e308;
    input.points[0].mp = -1e308;
    // Both inputs are finite, but their weighted sum is NaN.
    assert!(Observation::new(input.clone()).is_ok());
    let mut state = State::default();
    let commands = run(input.clone(), &mut state, Level::Normal);
    assert_eq!(state.aim, None);
    assert!(!has(&commands, "amove"));
    input.points.push(PointObservation {
        at: Point { x: 130.0, z: 20.0 },
        vp: 1.0,
        mp: 1.0,
        owner: None,
        cut: false,
        locked: false,
    });
    let mut state = State::default();
    run(input, &mut state, Level::Normal);
    assert_eq!(state.aim.map(|(i, _)| i), Some(1));
    assert_eq!(least_score([0, 1, 2].into_iter(), |_| -0.0), Some(0));
    assert_eq!(least_score([0, 1].into_iter(), |_| f64::INFINITY), None);
}

#[test]
fn if25_ai_observation_rejects_nan_score_inputs_and_positions() {
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut input = fixture(vec![unit(1, "rifle", 40.0)]);
        input.units[0].at.x = value;
        assert!(matches!(
            Observation::new(input),
            Err(PolicyError::Position)
        ));
        let mut input = fixture(vec![unit(1, "rifle", 40.0)]);
        input.points[0].vp = value;
        assert!(matches!(Observation::new(input), Err(PolicyError::Points)));
        let mut input = fixture(vec![unit(1, "rifle", 40.0)]);
        input
            .runtime
            .as_mut()
            .unwrap()
            .units
            .get_mut(&1)
            .unwrap()
            .stock = Some(Stock {
            ammo: None,
            provisions: value,
            fuel: None,
            forced: false,
        });
        assert!(matches!(
            Observation::new(input),
            Err(PolicyError::RuntimeContext)
        ));
    }
}

#[test]
fn if25_ai_guard_dispatch_keeps_equal_distance_observation_order() {
    let mut foe = unit(9, "rifle", 90.0);
    foe.owner = 1;
    let mut input = fixture(vec![
        unit(1, "rifle", 39.0),
        unit(2, "rifle", 40.0),
        unit(3, "rifle", 40.0),
        foe,
    ]);
    input.points[0].at = Point { x: 90.0, z: 20.0 };
    input.points[0].owner = Some(0);
    let commands = run(input.clone(), &mut State::default(), Level::Hard);
    let orders: Vec<_> = commands
        .iter()
        .filter(|c| c["t"] == "amove")
        .flat_map(|c| c["orders"].as_array().unwrap())
        .map(|row| row[0].as_u64().unwrap())
        .collect();
    assert_eq!(orders, [2, 3]);
    // Suppressing an already-issued order must not change the tied responder
    // budget or cause the farther squad to replace it.
    input
        .runtime
        .as_mut()
        .unwrap()
        .units
        .get_mut(&2)
        .unwrap()
        .amove = Some(Point { x: 90.0, z: 20.0 });
    let commands = run(input, &mut State::default(), Level::Hard);
    let orders: Vec<_> = commands
        .iter()
        .filter(|c| c["t"] == "amove")
        .flat_map(|c| c["orders"].as_array().unwrap())
        .map(|row| row[0].as_u64().unwrap())
        .collect();
    assert_eq!(orders, [3]);
}

#[test]
fn if25_ai_visible_enemy_traversal_preserves_delivered_order_for_guard_value() {
    let mut first = unit(8, "rifle", 90.0);
    first.owner = 1;
    let mut second = unit(9, "rifle", 91.0);
    second.owner = 1;
    let mut far = unit(10, "rifle", 130.0);
    far.owner = 1;
    for (delivered, expected) in [
        ([&first, &far, &second], [8, 9]),
        ([&second, &far, &first], [9, 8]),
    ] {
        assert_eq!(
            enemies_near(&delivered, Point { x: 90.0, z: 20.0 }, 24.0)
                .iter()
                .map(|u| u.id)
                .collect::<Vec<_>>(),
            expected
        );
    }
}

#[test]
fn if25_ai_classic_targets_recon_depots_and_raiders_keep_stable_ties() {
    let mut input = fixture(vec![
        unit(1, "armoredcar", 40.0),
        unit(2, "armoredcar", 40.0),
        unit(3, "rifle", 40.0),
    ]);
    input.players[1].spawn = Point { x: 100.0, z: 10.0 };
    input.players[2].team = 1;
    input.players[2].spawn = Point { x: 100.0, z: 30.0 };
    input.runtime.as_mut().unwrap().mode = Mode::Classic;
    let observation = Observation::new(input.clone()).unwrap();
    let context = observation.input().runtime.as_ref().unwrap();
    let targets = crate::commander_classic_rules::targets(&observation, context).unwrap();
    assert_eq!(targets.iter().map(|t| t.1).collect::<Vec<_>>(), [1, 2]);
    let mine = observation.input().units.iter().collect::<Vec<_>>();
    let recon = crate::commander_classic_rules::plan(
        &observation,
        context,
        &mine,
        &mine,
        &[],
        &mut None,
        Skill::for_level(Level::Normal),
    )
    .unwrap();
    assert_eq!(recon.support, Some(("recon", input.players[1].spawn)));
    input.runtime.as_mut().unwrap().known_buildings = vec![
        KnownBuilding {
            id: 8,
            kind: "depot".into(),
            owner: 1,
            at: Point { x: 80.0, z: 10.0 },
        },
        KnownBuilding {
            id: 9,
            kind: "depot".into(),
            owner: 2,
            at: Point { x: 80.0, z: 30.0 },
        },
    ];
    let observation = Observation::new(input).unwrap();
    let context = observation.input().runtime.as_ref().unwrap();
    let units = &observation.input().units;
    for order in [[0, 1, 2], [1, 0, 2]] {
        let mine = order.map(|i| &units[i]);
        let mut raid = None;
        crate::commander_classic_rules::plan(
            &observation,
            context,
            &mine,
            &mine,
            &[],
            &mut raid,
            Skill::for_level(Level::Normal),
        )
        .unwrap();
        let raid = raid.unwrap();
        assert_eq!(raid.target, 8);
        assert_eq!(
            raid.ids,
            order[..2].iter().map(|i| units[*i].id).collect::<Vec<_>>()
        );
    }
}

#[test]
fn if25_ai_attack_targets_reuse_previous_units_sorted_tie_order() {
    let mut input = fixture(
        (1..=10)
            .map(|id| unit(id, "rifle", if id == 1 { 110.0 } else { 70.0 }))
            .collect(),
    );
    input.runtime.as_mut().unwrap().mode = Mode::Classic;
    input.runtime.as_mut().unwrap().known_buildings = vec![
        KnownBuilding {
            id: 20,
            kind: "hq".into(),
            owner: 1,
            at: Point { x: 40.0, z: 20.0 },
        },
        KnownBuilding {
            id: 21,
            kind: "hq".into(),
            owner: 1,
            at: Point { x: 100.0, z: 20.0 },
        },
    ];
    input.points.clear();
    let commands = run(input, &mut State::default(), Level::Easy);
    let attacks: Vec<_> = commands
        .iter()
        .filter(|c| c["t"] == "amove")
        .flat_map(|c| c["orders"].as_array().unwrap())
        .collect();
    assert_eq!(attacks.len(), 4);
    assert_eq!(attacks[0], &json!([1, 100.0, 20.0]));
    assert_eq!(attacks[1], &json!([2, 100.0, 20.0]));
}

#[test]
fn if25_ai_research_equal_levels_keep_authored_line_order() {
    use crate::{commander_ai::ResearchObservation, economy::TechState};
    let mut input = fixture(vec![unit(1, "rifle", 40.0)]);
    input.manpower = 1000.0;
    input.runtime.as_mut().unwrap().munitions = 1000.0;
    input.research = Some(ResearchObservation {
        faction: Faction::Usa,
        tech: TechState::new(3, 0, 0, 0).unwrap(),
        busy: vec![],
        stock_base: false,
        armory_ready: true,
        standard_army: true,
    });
    for (busy, expected) in [
        (vec![], "inf"),
        (vec![Technology::Infantry], "guns"),
        (vec![Technology::Infantry, Technology::Guns], "armor"),
    ] {
        input.research.as_mut().unwrap().busy = busy;
        let observation = Observation::new(input.clone()).unwrap();
        let commands = research(&observation, observation.input().runtime.as_ref().unwrap())
            .unwrap()
            .0;
        assert_eq!(commands[0]["kind"], expected);
    }
}

#[test]
fn if25_ai_aircraft_attack_and_intercept_keep_front_distance_ties_and_ready_gate() {
    let mut input = fixture(vec![
        unit(1, "rifle", 60.0),
        unit(2, "attacker", 10.0),
        unit(3, "fighter", 10.0),
        unit(8, "tank", 80.0),
        unit(9, "tank", 40.0),
        unit(10, "fighter", 90.0),
        unit(11, "fighter", 30.0),
    ]);
    for u in &mut input.units[3..] {
        u.owner = 1;
    }
    for id in [2, 3] {
        input
            .runtime
            .as_mut()
            .unwrap()
            .units
            .get_mut(&id)
            .unwrap()
            .air_ready = true;
    }
    let observation = Observation::new(input.clone()).unwrap();
    let context = observation.input().runtime.as_ref().unwrap();
    let units = &observation.input().units;
    for (order, tank, plane) in [([3, 4, 5, 6], 8, 90.0), ([4, 3, 6, 5], 9, 30.0)] {
        let enemies = order.map(|i| &units[i]);
        let commands = aircraft(
            &observation,
            context,
            &units[..3].iter().collect::<Vec<_>>(),
            &enemies,
        )
        .unwrap();
        assert_eq!(
            commands,
            [
                json!({"t":"attack","ids":[2],"target":tank}),
                json!({"t":"move","orders":[[3,plane,20.0]]})
            ]
        );
    }
    input
        .runtime
        .as_mut()
        .unwrap()
        .units
        .get_mut(&2)
        .unwrap()
        .air_ready = false;
    let observation = Observation::new(input).unwrap();
    let context = observation.input().runtime.as_ref().unwrap();
    let units = &observation.input().units;
    assert_eq!(
        aircraft(
            &observation,
            context,
            &units[..3].iter().collect::<Vec<_>>(),
            &[]
        )
        .unwrap(),
        [json!({"t":"move","orders":[[3,60.0,20.0]]})]
    );
}

#[test]
fn if25_ai_holder_spawn_ties_drive_mines_trenches_wire_traps_and_housing() {
    let mut input = fixture(vec![unit(1, "rifle", 80.0)]);
    input.points[0].at = Point { x: 80.0, z: 20.0 };
    input.points[0].owner = Some(0);
    input.players[1].spawn = Point { x: 120.0, z: 20.0 };
    input.players[2].team = 1;
    input.players[2].spawn = Point { x: 40.0, z: 20.0 };
    let propose = |mut input: crate::commander_ai::Input, armor| {
        let observation = Observation::new(input.clone()).unwrap();
        let fallback = point_holder::plan(
            &observation,
            observation.input().runtime.as_ref().unwrap(),
            &observation.input().units[0],
            &observation.input().points[0],
            &[],
            armor,
            1000.0,
        )
        .unwrap()
        .unwrap();
        input.manpower = 1000.0;
        let mines = input.runtime.as_ref().unwrap().mines.clone();
        input.runtime.as_mut().unwrap().field = Some(crate::commander_field_policy::Facts {
            initial_chars: input.chars.clone(),
            ground: input.chars.clone(),
            mines,
            ..Default::default()
        });
        let observation = Observation::new(input).unwrap();
        let mut active = Vec::new();
        crate::commander_field_policy::holder(
            &observation,
            &observation.input().units[0],
            0,
            armor,
            |command| {
                active.push(command);
                true
            },
        )
        .unwrap();
        assert_eq!(active.as_slice(), std::slice::from_ref(&fallback));
        fallback
    };
    let right = propose(input.clone(), false);
    assert_eq!(right["kind"], "mines");
    assert!(right["x"].as_f64().unwrap() > 80.0);
    input.players.swap(1, 2);
    assert!(propose(input.clone(), false)["x"].as_f64().unwrap() < 80.0);
    input.runtime.as_mut().unwrap().mines = vec![input.points[0].at; 2];
    assert_eq!(propose(input.clone(), false)["pattern"], "strongpoint");
    input.flags.fill(crate::game::TRENCH);
    assert_eq!(propose(input.clone(), false)["fort"], "wire");
    input.chars.fill(b'X');
    assert_eq!(propose(input.clone(), true)["fort"], "traps");
    input.chars.fill(b'Y');
    input.units[0].garrison = true;
    assert_eq!(propose(input.clone(), true)["t"], "move");
    input.units[0].garrison = false;
    input.chars.fill(b'.');
    input.chars[10 * input.width + 40] = b'B';
    input.units[0].kind = "mg".into();
    let house = propose(input, false);
    assert_eq!(house["t"], "garrison");
}

#[test]
fn if25_ai_hq_threat_and_later_crew_ties_keep_repeated_sort_history() {
    use crate::commander_ai::ResearchObservation;
    use crate::economy::TechState;
    let mut input = fixture(vec![
        unit(1, "rifle", 50.0),
        unit(2, "rifle", 30.0),
        unit(10, "hq", 20.0),
        unit(11, "hq", 40.0),
    ]);
    input.tick = 4000;
    input.height = 60;
    input.flags.resize(input.width * input.height, 0);
    input.chars.resize(input.width * input.height, b'.');
    input.players[1].spawn = Point { x: 120.0, z: 20.0 };
    input.players[2].team = 1;
    input.players[2].spawn = Point { x: 40.0, z: 100.0 };
    input.research = Some(ResearchObservation {
        faction: Faction::Usa,
        tech: TechState::new(1, 0, 0, 0).unwrap(),
        busy: vec![],
        stock_base: false,
        armory_ready: false,
        standard_army: true,
    });
    let observation = Observation::new(input).unwrap();
    let context = observation.input().runtime.as_ref().unwrap();
    let ground = sight_game(&observation, context);
    let available = observation.input().units[..2].iter().collect::<Vec<_>>();
    let result = crate::commander_fortification::plan(
        &observation,
        &available,
        1000.0,
        0.0,
        0.0,
        &ground,
        |_| true,
        &mut BTreeMap::from([(10, 1)]),
    )
    .unwrap();
    assert_eq!(result.builders, [2]);
    assert_eq!(result.commands[0]["dir"], 0.0);
}
