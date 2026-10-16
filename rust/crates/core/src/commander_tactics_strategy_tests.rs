use super::tests::{acts, fixture, has, unit};
use super::*;
use crate::commander_ai::{Input, PointObservation};

fn look(input: Input, sightings: &[Sighting], state: &mut State, level: Level) -> Vec<Value> {
    plan(
        &Observation::new(input).unwrap(),
        sightings,
        state,
        level,
        &mut MatchRandom::new(19),
    )
    .unwrap()
}
fn enemy(id: u32, kind: &str, x: f64) -> UnitObservation {
    let mut u = unit(id, kind, x);
    u.owner = 1;
    u
}
fn sight(u: &UnitObservation, time: f64) -> Sighting {
    let catalog = Catalog::compiled().unwrap();
    let d = catalog.unit(&u.kind).unwrap();
    Sighting {
        id: u.id,
        kind: u.kind.clone(),
        owner: u.owner,
        at: u.at,
        time,
        worth: worth(u, d),
    }
}
fn point(x: f64, owner: Option<usize>) -> PointObservation {
    PointObservation {
        at: Point { x, z: 20.0 },
        owner,
        vp: 1.0,
        mp: 0.0,
        cut: false,
        locked: false,
    }
}
fn orders(commands: &[Value], kind: &str) -> Vec<(u32, Point)> {
    commands
        .iter()
        .filter(|c| c["t"] == kind)
        .flat_map(|c| c["orders"].as_array().unwrap())
        .map(|o| {
            (
                o[0].as_u64().unwrap() as u32,
                Point {
                    x: o[1].as_f64().unwrap(),
                    z: o[2].as_f64().unwrap(),
                },
            )
        })
        .collect()
}

#[test]
fn intent_ai29_watched_home_threat_precedes_committed_wait_and_uses_id_ties() {
    let threats = [enemy(9, "rifle", 32.0), enemy(10, "rifle", 32.0)];
    let mut input = fixture(vec![
        unit(1, "rifle", 60.0),
        threats[0].clone(),
        threats[1].clone(),
    ]);
    let mut state = State::default();
    look(input.clone(), &[], &mut state, Level::Normal);
    assert_eq!(state.situation.as_ref().unwrap().kind, SituationKind::Push);
    input.tick += 40;
    state.situation = Some(Situation {
        kind: SituationKind::Wait,
        until: 100.0,
        point: None,
        threat: Some(11),
        at: Some(Point { x: 120.0, z: 20.0 }),
        counter: Some("at"),
        safe: None,
        failed_at: 0,
    });
    let remembered = [Sighting {
        id: 11,
        kind: "tank".into(),
        owner: 1,
        at: Point { x: 120.0, z: 20.0 },
        time: 10.0,
        worth: 300.0,
    }];
    let commands = look(input, &remembered, &mut state, Level::Normal);
    let sit = state.situation.unwrap();
    assert_eq!(sit.kind, SituationKind::Defense);
    assert_eq!(sit.threat, Some(9));
    assert_eq!(orders(&commands, "amove"), vec![(1, threats[0].at)]);
}

#[test]
fn intent_ai29_wait_keeps_safe_point_until_invalid_then_releases_expired_threat() {
    let tank = enemy(9, "tank", 120.0);
    let seen = [sight(&tank, 10.0)];
    let mut input = fixture(vec![unit(1, "rifle", 40.0), tank]);
    input.points = vec![point(120.0, None), point(80.0, None), point(84.0, None)];
    input.manpower = 300.0;
    input.units.insert(1, unit(2, "motorpool", 10.0));
    input.runtime.as_mut().unwrap().units.insert(
        2,
        UnitFacts {
            built: 1.0,
            ..UnitFacts::default()
        },
    );
    // Keep this threat-memory scene at a completed base, so the scout is not its builder.
    for (id, kind) in [(3, "hq"), (4, "barracks"), (5, "airfield")] {
        input.units.insert(input.units.len()-1, unit(id, kind, 10.0));
        input.runtime.as_mut().unwrap().units.insert(id, UnitFacts { built: 1.0, ..UnitFacts::default() });
    }
    let mut state = State::default();
    let commands = look(input.clone(), &seen, &mut state, Level::Normal);
    assert_eq!(state.situation.as_ref().unwrap().kind, SituationKind::Wait);
    assert_eq!(state.situation.as_ref().unwrap().safe, Some(1));
    assert!(
        commands
            .iter()
            .any(|c| c["t"] == "buy" && c["unit"] == "at")
    );
    input.tick += 20;
    input.units[0].at.x = 86.0;
    look(input.clone(), &seen, &mut state, Level::Normal);
    assert_eq!(state.situation.as_ref().unwrap().safe, Some(1));
    input.points[1].owner = Some(0);
    look(input.clone(), &seen, &mut state, Level::Normal);
    assert_eq!(state.situation.as_ref().unwrap().safe, Some(2));
    input.tick = 400;
    look(input, &seen, &mut state, Level::Normal);
    assert!(state.released.contains(&9));
    assert_eq!(state.situation.unwrap().kind, SituationKind::Push);
}

#[test]
fn intent_ai31_point_score_values_load_resources_failures_and_loyalty() {
    for (points, aim, expected) in [
        (vec![point(100.0, None), point(110.0, None)], None, 0),
        (
            {
                let a = point(100.0, None);
                let mut b = point(110.0, None);
                b.vp = 2.0;
                vec![a, b]
            },
            None,
            1,
        ),
        (
            {
                let a = point(100.0, None);
                let mut b = point(110.0, None);
                b.mp = 2.0;
                vec![a, b]
            },
            None,
            1,
        ),
        (
            vec![point(100.0, None), point(110.0, None)],
            Some((1, 100.0)),
            1,
        ),
        (
            {
                let mut a = point(100.0, None);
                a.locked = true;
                vec![a, point(110.0, None)]
            },
            None,
            1,
        ),
        (vec![point(100.0, Some(0)), point(110.0, None)], None, 1),
    ] {
        let mut input = fixture(vec![unit(1, "rifle", 40.0)]);
        input.points = points;
        let mut state = State {
            aim,
            ..State::default()
        };
        let commands = look(input.clone(), &[], &mut state, Level::Normal);
        let at = orders(&commands, "move")[0].1;
        assert!(
            dist(at, input.points[expected].at) <= 4.00001,
            "expected {expected}: {commands:?}"
        );
    }
    let mut marching = unit(2, "rifle", 50.0);
    marching.path = vec![Point { x: 100.0, z: 20.0 }];
    let mut input = fixture(vec![unit(1, "rifle", 40.0), marching]);
    input.points = vec![point(100.0, None), point(110.0, None)];
    let commands = look(input, &[], &mut State::default(), Level::Normal);
    assert!(dist(orders(&commands, "move")[0].1, Point { x: 110.0, z: 20.0 }) < 4.00001);
}

#[test]
fn intent_ai31_defenders_stay_within_seventy_metres_of_spawn() {
    let mut input = fixture(vec![unit(1, "rifle", 40.0)]);
    input.runtime.as_mut().unwrap().defender = true;
    input.points = vec![point(90.0, None), point(80.0, None)];
    let commands = look(input, &[], &mut State::default(), Level::Normal);
    assert!(dist(orders(&commands, "move")[0].1, Point { x: 80.0, z: 20.0 }) < 4.00001);
}

#[test]
fn intent_ai32_loss_changes_actual_purchase_after_enemy_leaves_view() {
    let tank = enemy(9, "tank", 100.0);
    let mut state = State::default();
    let mut input = fixture(vec![unit(1, "rifle", 80.0), tank.clone()]);
    look(
        input.clone(),
        &[sight(&tank, 10.0)],
        &mut state,
        Level::Normal,
    );
    input.tick += 40;
    input.units = vec![
        unit(2, "rifle", 40.0),
        unit(3, "motorpool", 10.0),
        unit(4, "barracks", 10.0),
    ];
    input.runtime.as_mut().unwrap().units.insert(
        3,
        UnitFacts {
            built: 1.0,
            ..UnitFacts::default()
        },
    );
    input.runtime.as_mut().unwrap().units.insert(
        4,
        UnitFacts {
            built: 1.0,
            ..UnitFacts::default()
        },
    );
    input.manpower = 200.0;
    // The old position has left sight, so the loss remains attributable to the tank.
    let commands = look(
        input.clone(),
        &[sight(&tank, 10.0)],
        &mut state,
        Level::Normal,
    );
    assert!(
        commands
            .iter()
            .any(|c| c["t"] == "buy" && c["unit"] == "at")
    );
    assert_eq!(state.mind.point_failures(), vec![0]);
    input.tick = 1600;
    let learned = look(input.clone(), &[], &mut state, Level::Normal);
    let unlearned = look(input, &[], &mut State::default(), Level::Normal);
    assert!(learned.iter().any(|c| c["t"] == "buy" && c["unit"] == "at"));
    assert!(
        unlearned
            .iter()
            .any(|c| c["t"] == "buy" && c["unit"] == "mg")
    );
    assert!(
        !unlearned
            .iter()
            .any(|c| c["t"] == "buy" && c["unit"] == "at")
    );
}

#[test]
fn intent_ai37_arms_only_watched_nearby_fights_and_preserves_existing_stance() {
    let foe = enemy(9, "rifle", 70.0);
    let mut input = fixture(vec![unit(1, "rifle", 40.0), foe]);
    let mut state = State::default();
    assert!(!has(
        &look(input.clone(), &[], &mut state, Level::Normal),
        "stance"
    ));
    input.tick += 25;
    assert!(acts(
        &look(input.clone(), &[], &mut state, Level::Normal),
        "stance",
        1
    ));
    input
        .runtime
        .as_mut()
        .unwrap()
        .units
        .get_mut(&1)
        .unwrap()
        .auto_retreat = true;
    assert!(!has(
        &look(input.clone(), &[], &mut state, Level::Normal),
        "stance"
    ));
    input
        .runtime
        .as_mut()
        .unwrap()
        .units
        .get_mut(&1)
        .unwrap()
        .auto_retreat = false;
    input.units[1].at.x = 80.0;
    assert!(!has(&look(input, &[], &mut state, Level::Normal), "stance"));
}

#[test]
fn intent_ai41_guards_with_enough_value_nearest_first_without_reissuing_amove() {
    let mut input = fixture(vec![
        unit(1, "rifle", 40.0),
        unit(2, "rifle", 42.0),
        unit(3, "rifle", 44.0),
        enemy(9, "rifle", 90.0),
    ]);
    input.points = vec![point(90.0, Some(0))];
    let commands = look(input.clone(), &[], &mut State::default(), Level::Hard);
    assert_eq!(
        orders(&commands, "amove")
            .iter()
            .map(|o| o.0)
            .collect::<Vec<_>>(),
        vec![3, 2]
    );
    assert!(!has(
        &look(input.clone(), &[], &mut State::default(), Level::Normal),
        "amove"
    ));
    for id in [2, 3] {
        input
            .runtime
            .as_mut()
            .unwrap()
            .units
            .get_mut(&id)
            .unwrap()
            .amove = Some(Point { x: 90.0, z: 20.0 });
    }
    assert!(!has(
        &look(input.clone(), &[], &mut State::default(), Level::Hard),
        "amove"
    ));
    input.units.truncate(1);
    input.units.push(enemy(9, "tank", 90.0));
    assert!(!has(
        &look(input, &[], &mut State::default(), Level::Hard),
        "amove"
    ));
}

#[test]
fn intent_ai46_remembered_armor_routes_at_and_filters_soft_orders_until_ground_seen_empty() {
    let mut input = fixture(vec![
        unit(1, "rifle", 40.0),
        unit(2, "at", 42.0),
        unit(3, "at", 44.0),
    ]);
    input.points = vec![point(120.0, None)];
    let tank = enemy(9, "tank", 120.0);
    let sightings = [sight(&tank, 10.0)];
    let commands = look(
        input.clone(),
        &sightings,
        &mut State::default(),
        Level::Normal,
    );
    assert_eq!(orders(&commands, "amove"), vec![(2, tank.at), (3, tank.at)]);
    assert!(!acts(&commands, "move", 1));
    assert!(acts(
        &look(input.clone(), &[], &mut State::default(), Level::Normal),
        "move",
        1
    ));
    input.units[0].at.x = 88.0;
    let commands = look(
        input.clone(),
        &sightings,
        &mut State::default(),
        Level::Normal,
    );
    assert!(acts(&commands, "move", 1));
    assert!(!has(&commands, "amove"));
}

#[test]
fn intent_ai59_ai60_ai61_planner_and_motor_tables_match_pinned_levels() {
    // Ordered columns reproduce all AI_LEVELS fields, including defaults for absent flags.
    let vectors = [
        (
            Level::Easy,
            vec![
                120.0, 0.0, 0.0, 250.0, 40.0, 45.0, 2.0, 0.0, 3.0, 150.0, 0.0, 10.0, 0.35, 2.5,
                4.0, 1.0, 16.0, 0.0, 0.0,
            ],
            "easy",
            vec![0.8, 3.4, 0.62, 3.6, 4.6, 10.0, 34.0],
        ),
        (
            Level::Normal,
            vec![
                40.0, 1.0, 0.0, 100.0, 0.0, 0.0, 2.0, 0.0, 3.0, 0.0, 0.0, 6.0, 0.35, 1.25, 6.0,
                2.0, 10.0, 0.0, 0.0,
            ],
            "normal",
            vec![0.5, 1.7, 0.32, 1.4, 3.2, 20.0, 68.0],
        ),
        (
            Level::Hard,
            vec![
                20.0, 1.0, 1.0, 100.0, 0.0, 0.0, 3.0, 1.0, 2.0, 0.0, 1.2, 6.0, 0.5, 0.5, 8.0, 3.0,
                6.0, 1.0, 1.0,
            ],
            "hard",
            vec![0.26, 1.0, 0.14, 0.45, 2.1, 50.0, 160.0],
        ),
    ];
    for (level, expected, name, motor) in vectors {
        let s = Skill::for_level(level);
        let b = |v: bool| if v { 1.0 } else { 0.0 };
        assert_eq!(
            vec![
                level.every() as f64,
                b(s.adaptive),
                b(s.memory),
                s.sup_reserve,
                s.mun_reserve,
                s.sup_gap,
                s.cluster as f64,
                b(s.best),
                s.wave as f64,
                s.first_assault,
                s.ratio,
                s.base_army as f64,
                s.retreat,
                s.notice,
                s.hands as f64,
                s.casts as f64,
                s.commit,
                b(s.focus),
                b(s.guard)
            ],
            expected
        );
        let m = crate::human_commander_timing::Skill::for_level(name);
        assert_eq!(
            vec![
                m.react,
                m.notice,
                m.input,
                m.scan,
                m.opening,
                m.peak as f64,
                m.minute as f64
            ],
            motor
        );
    }
    assert_eq!(
        crate::human_commander_timing::Skill::for_level("unknown"),
        crate::human_commander_timing::Skill::for_level("normal")
    );
}

#[test]
fn intent_ai30_paradrop_waits_for_enemy_point_notice_and_delivered_visibility() {
    let mut input = fixture(
        (1..=6)
            .map(|id| unit(id, "rifle", 25.0 + f64::from(id)))
            .collect(),
    );
    input.points = vec![point(60.0, Some(1))];
    input.manpower = 1000.0;
    input
        .runtime
        .as_mut()
        .unwrap()
        .support
        .insert("para".into(), 0.0);
    let mut state = State::default();
    assert!(!has(
        &look(input.clone(), &[], &mut state, Level::Normal),
        "support"
    ));
    input.tick += 24;
    assert!(!has(
        &look(input.clone(), &[], &mut state, Level::Normal),
        "support"
    ));
    input.tick += 1;
    let commands = look(input.clone(), &[], &mut state, Level::Normal);
    assert!(
        commands
            .iter()
            .any(|c| c["t"] == "support" && c["kind"] == "para")
    );
    let mut hidden = input;
    hidden.tick += 40;
    for u in &mut hidden.units {
        u.at.x = 5.0;
    }
    assert!(!has(
        &look(hidden, &[], &mut State::default(), Level::Normal),
        "support"
    ));
}

#[test]
fn intent_ai40_assault_respects_wave_opening_value_memory_and_smoke() {
    let mut input = fixture((1..=3).map(|id| unit(id, "rifle", 40.0)).collect());
    input.points = vec![point(100.0, Some(1))];
    let commands = look(input.clone(), &[], &mut State::default(), Level::Normal);
    assert_eq!(orders(&commands, "amove").len(), 3);
    assert!(!has(
        &look(input.clone(), &[], &mut State::default(), Level::Easy),
        "amove"
    ));
    input.tick = 3000;
    assert_eq!(
        orders(
            &look(input.clone(), &[], &mut State::default(), Level::Easy),
            "amove"
        )
        .len(),
        3
    );
    input.units.truncate(2);
    input.units.push(enemy(9, "tiger", 100.0));
    assert!(!has(
        &look(input.clone(), &[], &mut State::default(), Level::Normal),
        "amove"
    ));
    input.units.truncate(2);
    let mut past = sight(&enemy(9, "rifle", 100.0), 150.0);
    past.worth = 300.0;
    let remembered = [past];
    assert!(!has(
        &look(
            input.clone(),
            &remembered,
            &mut State::default(),
            Level::Hard
        ),
        "amove"
    ));
    input.tick = 3600;
    assert_eq!(
        orders(
            &look(
                input.clone(),
                &remembered,
                &mut State::default(),
                Level::Hard
            ),
            "amove"
        )
        .len(),
        2
    );
    input.units.push(unit(3, "rifle", 40.0));
    input.manpower = 150.0;
    input
        .runtime
        .as_mut()
        .unwrap()
        .support
        .insert("smoke".into(), 0.0);
    let commands = look(input, &[], &mut State::default(), Level::Normal);
    let smoke = commands.iter().find(|c| c["t"] == "support").unwrap();
    assert_eq!(smoke["kind"], "smoke");
    assert_eq!(smoke["x"], 76.0);
    assert_eq!(smoke["z"], 20.0);
}

#[test]
fn intent_ai31_loss_penalty_changes_the_next_point_after_threat_memory_is_gone() {
    let mut first = fixture(vec![unit(1, "rifle", 100.0)]);
    first.points = vec![point(100.0, None), point(110.0, None)];
    let mut learned = State::default();
    look(first.clone(), &[], &mut learned, Level::Normal);
    let mut next = first;
    next.tick = 600;
    next.units = vec![unit(2, "rifle", 40.0)];
    let a = look(next.clone(), &[], &mut learned, Level::Normal);
    let b = look(next, &[], &mut State::default(), Level::Normal);
    assert_eq!(learned.mind.point_failures(), vec![1, 0]);
    assert!(dist(orders(&a, "move")[0].1, Point { x: 110.0, z: 20.0 }) < 4.00001);
    assert!(dist(orders(&b, "move")[0].1, Point { x: 100.0, z: 20.0 }) < 4.00001);
}

#[test]
fn intent_ai46_empty_ground_stays_cleared_after_the_scout_leaves() {
    let mut input = fixture(vec![unit(1, "rifle", 40.0), unit(2, "at", 42.0)]);
    input.points = vec![point(120.0, None)];
    let sightings = [sight(&enemy(9, "tank", 120.0), 10.0)];
    let mut state = State::default();
    assert!(has(
        &look(input.clone(), &sightings, &mut state, Level::Normal),
        "amove"
    ));
    input.units[0].at.x = 88.0;
    input.tick += 20;
    assert!(!has(
        &look(input.clone(), &sightings, &mut state, Level::Normal),
        "amove"
    ));
    input.units[0].at.x = 40.0;
    input.tick += 20;
    let commands = look(input.clone(), &sightings, &mut state, Level::Normal);
    assert!(!has(&commands, "amove"));
    assert!(acts(&commands, "move", 1));
    input.units.push(enemy(9, "tank", 120.0));
    input.tick += 20;
    let new = [sight(input.units.last().unwrap(), input.tick as f64 / 20.0)];
    assert!(has(&look(input, &new, &mut state, Level::Normal), "amove"));
}
#[test]
fn runtime_keeps_one_holder_and_sends_extra_infantry_onward() {
    let mut input = fixture(vec![unit(1, "rifle", 40.0), unit(2, "rifle", 40.0)]);
    input.points = vec![point(40.0, Some(0)), point(100.0, None)];
    let commands = look(input, &[], &mut State::default(), Level::Normal);
    assert!(acts(&commands, "cover", 1));
    assert!(!acts(&commands, "move", 1));
    assert!(acts(&commands, "move", 2));
}

#[test]
fn intent_ai53_runtime_build_spends_once_and_removes_builder_from_army_orders() {
    let mut input = fixture(vec![
        unit(1, "rifle", 100.0),
        unit(2, "rifle", 100.0),
        unit(3, "rifle", 100.0),
        unit(4, "hq", 10.0),
    ]);
    input.manpower = 200.0;
    input.points = vec![];
    let commands = look(input, &[], &mut State::default(), Level::Normal);
    assert!(
        commands
            .iter()
            .any(|c| c["t"] == "build" && c["kind"] == "hq")
    );
    assert!(!has(&commands, "buy"));
    assert!(!acts(&commands, "move", 1));
    assert!(!acts(&commands, "amove", 1));
}
#[test]
fn runtime_repeats_seeded_decisions_and_keeps_other_random_purposes_independent() {
    let mut a = State::default();
    let mut b = a.clone();
    let mut ra = MatchRandom::new(9001);
    let mut rb = ra.clone();
    let mut different = false;
    for turn in 0..40 {
        let mut input = fixture(vec![unit(1, "rifle", 40.0), unit(2, "rifle", 42.0)]);
        input.tick = 200 + turn * 40;
        let observation = Observation::new(input).unwrap();
        rb.next_fraction(StreamId::Combat);
        rb.next_fraction(StreamId::Weather);
        rb.next_fraction(StreamId::Ai(1));
        let left = plan(&observation, &[], &mut a, Level::Normal, &mut ra).unwrap();
        let right = plan(&observation, &[], &mut b, Level::Normal, &mut rb).unwrap();
        assert_eq!(left, right);
        assert_eq!(a, b);
        let other = plan(
            &observation,
            &[],
            &mut State::default(),
            Level::Normal,
            &mut MatchRandom::new(9002),
        )
        .unwrap();
        different |= left != other;
    }
    assert!(different);
}
