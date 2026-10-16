use super::*;
use crate::commander_ai::{Input, PlayerObservation};

fn at(x: f64, z: f64) -> Point {
    Point { x, z }
}
fn actor(id: u32, kind: &str, owner: usize, position: Point) -> UnitObservation {
    let def = Catalog::compiled().unwrap().unit(kind).unwrap();
    UnitObservation {
        id,
        kind: kind.into(),
        owner,
        at: position,
        hp: number(def, "models").unwrap() * number(def, "hpPer").unwrap(),
        supp: 0.0,
        target_id: 0,
        cover: 0,
        path: vec![],
        attack_id: false,
        retreating: false,
        riding: false,
        build: false,
        dig: false,
        entrench: false,
        garrison: false,
        enter: false,
        fire_at: false,
        nade: false,
    }
}
fn observation(mut units: Vec<UnitObservation>) -> Observation {
    units.sort_by_key(|u| u.id);
    Observation::new(Input {
        width: 200,
        height: 200,
        tick: 1200,
        slot: 0,
        players: vec![
            PlayerObservation {
                team: 0,
                out: false,
                spawn: at(10.0, 10.0),
            },
            PlayerObservation {
                team: 1,
                out: false,
                spawn: at(300.0, 300.0),
            },
            PlayerObservation {
                team: 1,
                out: true,
                spawn: at(80.0, 80.0),
            },
        ],
        units,
        points: vec![],
        flags: vec![0; 40000],
        chars: vec![b'.'; 40000],
        manpower: 0.0,
        research: None,
    })
    .unwrap()
}
fn army_units(n: u32) -> Vec<UnitObservation> {
    (1..=n)
        .map(|id| actor(id, "rifle", 0, at(20.0 + f64::from(id), 20.0)))
        .collect()
}
fn base(mode: Mode) -> BaseAttackFacts {
    BaseAttackFacts {
        mode,
        weather: WeatherKind::Clear,
        push: true,
        known_buildings: vec![],
        recent: vec![],
        unavailable: BTreeSet::new(),
    }
}
fn moves(commands: &[Command]) -> Vec<(i64, Point, bool)> {
    commands
        .iter()
        .flat_map(|c| match c {
            Command::Move {
                orders,
                queue,
                attack_move: true,
                ..
            } => orders.iter().map(|o| (o.unit, o.at, *queue)).collect(),
            _ => vec![],
        })
        .collect()
}

#[test]
fn intent_ai42_mode_targets_only_visible_bunkers_and_remembered_production() {
    let mut units = army_units(10);
    units.push(actor(100, "bunker", 1, at(110.0, 100.0)));
    units.push(actor(101, "hq", 1, at(80.0, 80.0)));
    units.push(actor(102, "bunker", 0, at(40.0, 40.0)));
    let obs = observation(units);
    for mode in [Mode::Default, Mode::Conquest, Mode::Tutorial] {
        assert!(
            base_attack_plan(&obs, &base(mode), Level::Hard)
                .unwrap()
                .is_empty()
        );
    }
    for mode in [Mode::Assault, Mode::Annihilation, Mode::Horde] {
        let plan = base_attack_plan(&obs, &base(mode), Level::Hard).unwrap();
        assert_eq!(moves(&plan).len(), 8);
        assert!(
            moves(&plan)
                .iter()
                .all(|(_, p, q)| *p == at(110.0, 100.0) && !q)
        );
    }
    for mode in [Mode::Classic, Mode::World] {
        let mut facts = base(mode);
        facts.known_buildings = vec![KnownBuilding {
            id: 200,
            kind: "barracks".into(),
            owner: 1,
            at: at(150.0, 140.0),
        }];
        let plan = base_attack_plan(&obs, &facts, Level::Hard).unwrap();
        assert!(moves(&plan).iter().all(|(_, p, _)| *p == at(150.0, 140.0)));
        assert_eq!(moves(&plan).len(), 8);
    }
}

#[test]
fn intent_ai42_classic_spawn_fallback_world_hold_and_enemy_out_filter() {
    let obs = observation(army_units(6));
    let plan = base_attack_plan(&obs, &base(Mode::Classic), Level::Normal).unwrap();
    assert_eq!(moves(&plan).len(), 6);
    assert!(moves(&plan).iter().all(|(_, p, _)| *p == at(300.0, 300.0)));
    assert!(
        base_attack_plan(&obs, &base(Mode::World), Level::Hard)
            .unwrap()
            .is_empty()
    );
    assert!(
        base_attack_plan(&obs, &base(Mode::Assault), Level::Hard)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn intent_ai42_weather_count_push_strength_and_hands_are_independent_gates() {
    let mut units = army_units(8);
    units.push(actor(100, "bunker", 1, at(110.0, 100.0)));
    let obs = observation(units);
    let mut facts = base(Mode::Assault);
    assert_eq!(
        moves(&base_attack_plan(&obs, &facts, Level::Hard).unwrap()).len(),
        8
    );
    facts.weather = WeatherKind::Fog; // round(6 / 0.7) = 9.
    assert!(
        base_attack_plan(&obs, &facts, Level::Hard)
            .unwrap()
            .is_empty()
    );
    facts.weather = WeatherKind::Clear;
    facts.push = false;
    assert!(
        base_attack_plan(&obs, &facts, Level::Hard)
            .unwrap()
            .is_empty()
    );
    facts.push = true;
    facts.mode = Mode::Classic;
    facts.known_buildings = vec![KnownBuilding {
        id: 200,
        kind: "hq".into(),
        owner: 1,
        at: at(120.0, 110.0),
    }];
    facts.recent = vec![RecentStrength {
        owner: 2,
        worth: 800.0,
        seen_at: 50.0,
    }];
    assert!(
        base_attack_plan(&obs, &facts, Level::Hard)
            .unwrap()
            .is_empty()
    );
    facts.recent[0].worth = 700.0;
    assert_eq!(
        moves(&base_attack_plan(&obs, &facts, Level::Hard).unwrap()).len(),
        8
    );
    facts.weather = WeatherKind::Rain;
    assert!(
        base_attack_plan(&obs, &facts, Level::Hard)
            .unwrap()
            .is_empty()
    );
    facts.weather = WeatherKind::Clear;
    facts.recent[0].worth = 9000.0;
    facts.recent[0].seen_at = 30.0; // Exact 30-second boundary expires.
    assert_eq!(
        moves(&base_attack_plan(&obs, &facts, Level::Normal).unwrap()).len(),
        6
    );
    let large = observation(army_units(12));
    assert_eq!(
        moves(&base_attack_plan(&large, &base(Mode::Classic), Level::Easy).unwrap()).len(),
        4
    );
}

#[test]
fn intent_ai42_only_idle_unclaimed_actors_march() {
    let mut units = army_units(8);
    units[0].path.push(at(30.0, 20.0));
    units[1].target_id = 100;
    units[2].retreating = true;
    units[3].dig = true;
    units.push(actor(100, "bunker", 1, at(100.0, 80.0)));
    units.push(actor(101, "bunker", 1, at(100.0, 0.0)));
    let obs = observation(units);
    let mut facts = base(Mode::Assault);
    facts.unavailable.insert(5);
    let plan = base_attack_plan(&obs, &facts, Level::Hard).unwrap();
    assert_eq!(
        moves(&plan)
            .iter()
            .map(|(id, _, _)| *id)
            .collect::<Vec<_>>(),
        vec![6, 7, 8]
    );
    facts.unavailable.insert(100);
    assert!(base_attack_plan(&obs, &facts, Level::Hard).is_err());
}

fn fight() -> (Observation, FocusFacts) {
    let mut units = army_units(2);
    for u in &mut units {
        u.target_id = 100;
    }
    units.push(actor(100, "rifle", 1, at(30.0, 20.0)));
    let mut hurt = actor(101, "rifle", 1, at(32.0, 20.0));
    hurt.hp = 20.0;
    units.push(hurt);
    let obs = observation(units);
    let facts = FocusFacts {
        mode: None,
        owned: [
            (
                1,
                OwnFocusOrders {
                    amove: Some(at(150.0, 150.0)),
                    ..Default::default()
                },
            ),
            (
                2,
                OwnFocusOrders {
                    queued_amove: Some(at(160.0, 150.0)),
                    ..Default::default()
                },
            ),
        ]
        .into(),
        watched: [100, 101].into(),
        clear_lines: [(1, 100), (1, 101), (2, 100), (2, 101)].into(),
        unavailable: BTreeSet::new(),
    };
    (obs, facts)
}
fn attacks(commands: &[Command]) -> Vec<(Vec<i64>, i64)> {
    commands
        .iter()
        .filter_map(|c| match c {
            Command::Attack { ids, target, .. } => Some((ids.clone(), *target)),
            _ => None,
        })
        .collect()
}

#[test]
fn intent_ai44_hard_combines_fire_finishes_hurt_target_and_requeues_onward_orders() {
    let (obs, facts) = fight();
    let mut state = FocusState::default();
    let plan = focus_plan(&obs, &facts, &mut state, Level::Hard).unwrap();
    assert_eq!(attacks(&plan), vec![(vec![1, 2], 101)]);
    assert_eq!(
        moves(&plan),
        vec![(1, at(150.0, 150.0), true), (2, at(160.0, 150.0), true)]
    );
    assert_eq!(state.targets, [(1, 101), (2, 101)].into());
    assert!(
        focus_plan(&obs, &facts, &mut FocusState::default(), Level::Normal)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn intent_ai44_switch_preserves_queued_amove_and_retreat_or_lost_view_drops_focus() {
    let (obs, mut facts) = fight();
    let mut input = obs.input().clone();
    for u in &mut input.units[..2] {
        u.attack_id = true;
    }
    for orders in facts.owned.values_mut() {
        orders.attack_target = Some(100);
        orders.queued_amove = orders.amove.or(orders.queued_amove);
        orders.amove = None;
    }
    let obs = Observation::new(input.clone()).unwrap();
    let plan = focus_plan(&obs, &facts, &mut FocusState::default(), Level::Hard).unwrap();
    assert_eq!(attacks(&plan), vec![(vec![1, 2], 101)]);
    assert!(moves(&plan).iter().all(|(_, _, queued)| *queued));
    input.units[2].retreating = true;
    facts.watched.remove(&101);
    let retreat = Observation::new(input.clone()).unwrap();
    let plan = focus_plan(&retreat, &facts, &mut FocusState::default(), Level::Hard).unwrap();
    assert!(attacks(&plan).is_empty());
    assert_eq!(
        moves(&plan),
        vec![(1, at(150.0, 150.0), false), (2, at(160.0, 150.0), false)]
    );
    input.units.retain(|u| u.owner == 0);
    facts.watched.clear();
    facts.clear_lines.clear();
    let hidden = Observation::new(input).unwrap();
    let mut state = FocusState {
        targets: [(1, 100), (2, 100)].into(),
    };
    assert_eq!(
        moves(&focus_plan(&hidden, &facts, &mut state, Level::Hard).unwrap()).len(),
        2
    );
    assert!(state.targets.is_empty());
}

#[test]
fn intent_ai44_notice_range_los_open_and_two_shooter_thresholds_hold() {
    let (obs, mut facts) = fight();
    facts.watched.clear();
    assert!(
        attacks(&focus_plan(&obs, &facts, &mut FocusState::default(), Level::Hard).unwrap())
            .is_empty()
    );
    facts.watched.insert(101);
    facts.clear_lines.remove(&(2, 101));
    assert!(
        focus_plan(&obs, &facts, &mut FocusState::default(), Level::Hard)
            .unwrap()
            .is_empty()
    );
    facts.clear_lines.insert((2, 101));
    let mut input = obs.input().clone();
    input.units[1].garrison = true;
    let covered = Observation::new(input.clone()).unwrap();
    assert!(
        focus_plan(&covered, &facts, &mut FocusState::default(), Level::Hard)
            .unwrap()
            .is_empty()
    );
    input.units[1].garrison = false;
    input.units[3].at = at(200.0, 200.0);
    let far = Observation::new(input).unwrap();
    assert!(
        focus_plan(&far, &facts, &mut FocusState::default(), Level::Hard)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn intent_ai44_rejects_hidden_watch_and_foreign_order_keys_without_state_mutation() {
    let (obs, mut facts) = fight();
    let original = FocusState {
        targets: [(1, 100)].into(),
    };
    let mut state = original.clone();
    facts.watched.insert(999);
    assert!(focus_plan(&obs, &facts, &mut state, Level::Hard).is_err());
    assert_eq!(state, original);
    facts.watched.remove(&999);
    facts.owned.insert(100, OwnFocusOrders::default());
    assert!(focus_plan(&obs, &facts, &mut state, Level::Hard).is_err());
    facts.owned.remove(&100);
    facts.clear_lines.insert((100, 1));
    assert!(focus_plan(&obs, &facts, &mut state, Level::Hard).is_err());
    assert_eq!(state, original);
}

#[test]
fn intent_ai44_weapon_specialization_and_exact_range_boundary() {
    let mut units = vec![
        actor(1, "at", 0, at(20.0, 20.0)),
        actor(2, "at", 0, at(20.0, 20.0)),
        actor(100, "rifle", 1, at(25.0, 20.0)),
        actor(101, "tank", 1, at(65.0, 20.0)),
    ];
    units[0].target_id = 100;
    units[1].target_id = 100;
    let facts = FocusFacts {
        mode: None,
        owned: [
            (1, OwnFocusOrders::default()),
            (2, OwnFocusOrders::default()),
        ]
        .into(),
        watched: [100, 101].into(),
        clear_lines: [(1, 100), (2, 100), (1, 101), (2, 101)].into(),
        unavailable: BTreeSet::new(),
    };
    let obs = observation(units.clone());
    let commands = focus_plan(&obs, &facts, &mut FocusState::default(), Level::Hard).unwrap();
    assert_eq!(attacks(&commands), vec![(vec![1, 2], 101)]);
    // The guns cannot focus infantry efficiently, and the tank is now outside range.
    units[3].at.x += 0.001;
    assert!(
        focus_plan(
            &observation(units),
            &facts,
            &mut FocusState::default(),
            Level::Hard
        )
        .unwrap()
        .is_empty()
    );
}

#[test]
fn intent_ai44_equal_value_ties_choose_lowest_id_and_only_one_focus_target() {
    let (obs, facts) = fight();
    let mut input = obs.input().clone();
    for u in &mut input.units[..2] {
        u.target_id = 999;
    }
    input.units[3].hp = input.units[2].hp;
    let commands = focus_plan(
        &Observation::new(input).unwrap(),
        &facts,
        &mut FocusState::default(),
        Level::Hard,
    )
    .unwrap();
    assert_eq!(attacks(&commands), vec![(vec![1, 2], 100)]);
}

#[test]
fn intent_ai44_killed_own_actor_is_pruned_without_faulting_the_next_look() {
    let (obs, mut facts) = fight();
    let mut input = obs.input().clone();
    input.units.retain(|u| u.id != 1);
    facts.owned.remove(&1);
    facts.clear_lines.retain(|(from, _)| *from != 1);
    let obs = Observation::new(input).unwrap();
    let mut state = FocusState {
        targets: [(1, 101)].into(),
    };
    assert!(
        focus_plan(&obs, &facts, &mut state, Level::Hard)
            .unwrap()
            .is_empty()
    );
    assert!(state.targets.is_empty());
    // A previous own ID now present as somebody else's actor is rejected.
    let mut foreign = obs.input().clone();
    foreign.units.push(actor(1, "rifle", 1, at(30.0, 20.0)));
    foreign.units.sort_by_key(|u| u.id);
    state.targets.insert(1, 101);
    assert!(
        focus_plan(
            &Observation::new(foreign).unwrap(),
            &facts,
            &mut state,
            Level::Hard
        )
        .is_err()
    );
}

#[test]
fn intent_ai42_building_work_excludes_army_only_in_skirmish_modes() {
    let mut units = army_units(6);
    units[0].build = true;
    units[1].garrison = true;
    units[2].entrench = true;
    units.push(actor(100, "bunker", 1, at(100.0, 80.0)));
    let obs = observation(units);
    assert!(
        base_attack_plan(&obs, &base(Mode::Assault), Level::Hard)
            .unwrap()
            .is_empty()
    );
    let commands = base_attack_plan(&obs, &base(Mode::Classic), Level::Hard).unwrap();
    assert_eq!(moves(&commands).len(), 6);
    assert_eq!(moves(&commands)[0].0, 1);
}

#[test]
fn intent_ai44_classic_builder_can_focus_but_skirmish_builder_cannot() {
    let (obs, mut facts) = fight();
    let mut input = obs.input().clone();
    input.units[0].build = true;
    let obs = Observation::new(input).unwrap();
    assert!(
        focus_plan(&obs, &facts, &mut FocusState::default(), Level::Hard)
            .unwrap()
            .is_empty()
    );
    facts.mode = Some(Mode::Classic);
    assert_eq!(
        attacks(&focus_plan(&obs, &facts, &mut FocusState::default(), Level::Hard).unwrap()),
        vec![(vec![1, 2], 101)]
    );
}
