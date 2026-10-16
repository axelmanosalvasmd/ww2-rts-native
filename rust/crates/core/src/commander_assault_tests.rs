use super::*;
use crate::{
    commander_ai::{Input, Level, PlayerObservation, PointObservation},
    commander_tactics::UnitFacts,
    game::{MOVE, SIGHT},
};

fn unit(id: u32, kind: &str, x: f64) -> UnitObservation {
    let d = Catalog::compiled().unwrap().unit(kind).unwrap();
    UnitObservation {
        id,
        kind: kind.into(),
        owner: 0,
        at: Point { x, z: 21.0 },
        hp: num(d, "models") * num(d, "hpPer"),
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
fn fixture() -> Input {
    let units = vec![
        unit(1, "rifle", 21.0),
        unit(2, "rifle", 25.0),
        unit(3, "mg", 19.0),
    ];
    Input {
        width: 80,
        height: 40,
        tick: 4000,
        slot: 0,
        players: vec![
            PlayerObservation {
                team: 0,
                out: false,
                spawn: Point { x: 1.0, z: 21.0 },
            },
            PlayerObservation {
                team: 1,
                out: false,
                spawn: Point { x: 159.0, z: 21.0 },
            },
        ],
        runtime: Some(Context {
            units: units
                .iter()
                .map(|u| {
                    (
                        u.id,
                        UnitFacts {
                            built: 1.0,
                            ..UnitFacts::default()
                        },
                    )
                })
                .collect(),
            ..Context::default()
        }),
        units,
        points: vec![PointObservation {
            at: Point { x: 121.0, z: 21.0 },
            vp: 1.0,
            mp: 0.0,
            owner: Some(1),
            cut: false,
            locked: false,
        }],
        flags: vec![0; 3200],
        chars: vec![b'.'; 3200],
        manpower: 0.0,
        research: None,
    }
}
fn sit(input: &Input) -> Situation {
    Situation {
        kind: SituationKind::Push,
        until: 400.0,
        point: Some(0),
        threat: None,
        at: Some(input.points[0].at),
        counter: None,
        safe: None,
        failed_at: 0,
    }
}
fn look(input: &Input, mind: &Mind, state: &mut AssaultState, now: f64) -> Plan {
    run(input, mind, state, now, Some(&sit(input)), &[], None, false)
}
#[allow(clippy::too_many_arguments)]
fn run(
    input: &Input,
    mind: &Mind,
    state: &mut AssaultState,
    now: f64,
    situation: Option<&Situation>,
    sightings: &[Sighting],
    support_at: Option<f64>,
    handoff: bool,
) -> Plan {
    let obs = Observation::new(input.clone()).unwrap();
    let empty = BTreeSet::new();
    plan(
        &obs,
        input.runtime.as_ref().unwrap(),
        mind,
        situation,
        Skill::for_level(Level::Hard),
        now,
        state,
        Options {
            busy: &empty,
            recovering: &empty,
            sightings,
            support_at,
            handoff,
            region: None,
        },
    )
    .unwrap()
}
fn apply(input: &mut Input, state: &AssaultState) {
    for m in &state.operation.as_ref().unwrap().members {
        if let Some(at) = m.order {
            input.units.iter_mut().find(|u| u.id == m.id).unwrap().path = vec![at];
        }
    }
}
fn settle(input: &mut Input, state: &AssaultState) {
    for m in &state.operation.as_ref().unwrap().members {
        if let Some(at) = m.order {
            let u = input.units.iter_mut().find(|u| u.id == m.id).unwrap();
            u.at = at;
            u.path.clear();
        }
    }
}
fn phase(state: &AssaultState) -> Phase {
    state.operation.as_ref().unwrap().phase
}

#[test]
fn intent_ai45_membership_roles_exact_stage_and_phase_deadlines() {
    assert_eq!(
        (
            ASSEMBLE,
            REGROUP,
            BLOCKED,
            LIFETIME,
            WITHDRAW,
            RESUME,
            COOLDOWN,
            SUPPORT_REAR,
            SEARCH_CELLS
        ),
        (12.0, 8.0, 10.0, 100.0, 1.25, 0.75, 18.0, 8.0, 2048)
    );
    let mut input = fixture();
    let mut mind = Mind::default();
    let mut state = AssaultState::default();
    let first = look(&input, &mind, &mut state, 200.0);
    let op = state.operation.as_ref().unwrap();
    assert_eq!(op.id, 1);
    assert_eq!(op.initial, 3);
    assert_eq!(phase(&state), Phase::Assemble);
    assert_eq!(
        op.members
            .iter()
            .map(|m| (m.id, m.role))
            .collect::<Vec<_>>(),
        vec![(2, Role::Line), (1, Role::Line), (3, Role::Support)]
    );
    assert_eq!(
        op.stage,
        Point {
            x: 39.66666666666667,
            z: 21.0
        }
    );
    assert_eq!(first.claims, BTreeSet::from([1, 2, 3]));
    assert_eq!(
        first.commands,
        vec![
            json!({"t":"move","orders":[[2,39.0,21.0]]}),
            json!({"t":"move","orders":[[1,39.0,21.0]]}),
            json!({"t":"move","orders":[[3,31.0,21.0]]})
        ]
    );
    apply(&mut input, &state);
    assert!(look(&input, &mind, &mut state, 201.0).commands.is_empty());
    assert_eq!(state.operation.as_ref().unwrap().id, 1);
    settle(&mut input, &state);
    look(&input, &mind, &mut state, 202.0);
    assert_eq!(phase(&state), Phase::Advance);
    assert!(
        state
            .operation
            .as_ref()
            .unwrap()
            .members
            .iter()
            .filter(|m| m.role == Role::Line)
            .all(|m| m.order == Some(Point { x: 121.0, z: 21.0 }))
    );
    apply(&mut input, &state);
    settle(&mut input, &state);
    look(&input, &mind, &mut state, 203.0);
    assert_eq!(phase(&state), Phase::Engage);
    // The support gun receives a new position as the group's center reaches the objective.
    apply(&mut input, &state);
    // A weak known contact keeps this an engagement while the line becomes dispersed.
    let mut contact = unit(9, "rifle", 105.0);
    contact.owner = 1;
    input.units.push(contact);
    mind.update(&Observation::new(input.clone()).unwrap(), &[], false);
    input.units[0].at = Point { x: 55.0, z: 21.0 };
    // The lagging rifle retains its accepted route, so this is dispersal rather than Stop.
    input.units[0].path = vec![Point { x: 121.0, z: 21.0 }];
    look(&input, &mind, &mut state, 208.0);
    assert_eq!(phase(&state), Phase::Regroup);
    assert_eq!(state.operation.as_ref().unwrap().state_at, 208.0);
    apply(&mut input, &state);
    for u in input.units.iter_mut().filter(|u| u.owner == 0) {
        u.target_id = 99;
    }
    look(&input, &mind, &mut state, 215.999);
    assert_eq!(phase(&state), Phase::Regroup);
    look(&input, &mind, &mut state, 216.0);
    assert_eq!(phase(&state), Phase::Advance);
    assert!(
        state
            .operation
            .as_ref()
            .unwrap()
            .members
            .iter()
            .all(|m| m.id != 1)
    );
}

#[test]
fn intent_ai45_opening_needs_two_lines_nearby_guns_and_eligible_members() {
    let mut input = fixture();
    let mind = Mind::default();
    input.points[0].owner = None;
    let mut state = AssaultState::default();
    assert!(look(&input, &mind, &mut state, 179.999).claims.is_empty());
    assert!(state.operation.is_none());
    // A neutral but undefended point still belongs to quick capture orders after opening.
    assert!(look(&input, &mind, &mut state, 180.0).claims.is_empty());
    input.points[0].owner = Some(1);
    assert_eq!(look(&input, &mind, &mut state, 180.0).claims.len(), 3);
    for excluded in 0..8 {
        let mut i = fixture();
        match excluded {
            0 => i.units[0].hp *= 0.64,
            1 => i.units[0].retreating = true,
            2 => i.units[0].build = true,
            3 => i.units[0].dig = true,
            4 => i.units[0].entrench = true,
            5 => i.units[0].garrison = true,
            6 => {
                i.runtime
                    .as_mut()
                    .unwrap()
                    .units
                    .get_mut(&1)
                    .unwrap()
                    .hold_position = true
            }
            _ => {
                i.runtime
                    .as_mut()
                    .unwrap()
                    .units
                    .get_mut(&1)
                    .unwrap()
                    .queued_orders = 1
            }
        }
        assert!(
            look(&i, &mind, &mut AssaultState::default(), 200.0)
                .claims
                .is_empty(),
            "exclusion {excluded}"
        );
    }
    input = fixture();
    input.units[2].at.x = 100.0;
    assert!(
        look(&input, &mind, &mut AssaultState::default(), 200.0)
            .claims
            .is_empty()
    );
    input = fixture();
    input.points.push(PointObservation {
        at: input.units[0].at,
        vp: 1.0,
        mp: 0.0,
        owner: Some(0),
        cut: false,
        locked: false,
    });
    assert!(
        look(&input, &mind, &mut AssaultState::default(), 200.0)
            .claims
            .is_empty()
    );
}

#[test]
fn intent_ai45_player_stop_and_changed_goal_release_for_exact_eighteen_seconds() {
    let mut input = fixture();
    let mind = Mind::default();
    let mut state = AssaultState::default();
    look(&input, &mind, &mut state, 200.0);
    apply(&mut input, &state);
    input.units[0].path.clear(); // Ordinary Stop removes the acknowledged route while far from it.
    input.units[1].path = vec![Point { x: 1.0, z: 1.0 }];
    let next = look(&input, &mind, &mut state, 201.0);
    assert_eq!(next.claims, BTreeSet::from([3]));
    assert_eq!(state.released, BTreeMap::from([(1, 219.0), (2, 219.0)]));
    assert_eq!(state.controlled(218.999), BTreeSet::from([1, 2, 3]));
    run(&input, &mind, &mut state, 202.0, None, &[], None, true);
    assert_eq!(phase(&state), Phase::Abandon);
    assert_eq!(state.controlled(218.999), BTreeSet::from([1, 2]));
    assert!(look(&input, &mind, &mut state, 218.999).claims.is_empty());
    assert_eq!(
        look(&input, &mind, &mut state, 219.0).claims,
        BTreeSet::from([1, 2, 3])
    );
    assert!(state.released.is_empty());
}

#[test]
fn intent_ai45_capture_handoff_defense_timeout_and_blocked_members_end_operation() {
    let mind = Mind::default();
    for case in 0..5 {
        let mut input = fixture();
        let mut state = AssaultState::default();
        look(&input, &mind, &mut state, 200.0);
        apply(&mut input, &state);
        let mut situation = sit(&input);
        let mut now = 201.0;
        match case {
            0 => input.points[0].owner = Some(0),
            1 => situation.kind = SituationKind::Defense,
            2 => {
                now = 300.001;
                for u in &mut input.units {
                    u.target_id = 99;
                }
            }
            3 => {
                now = 210.001;
                for u in &mut input.units {
                    u.hp *= 0.5;
                }
            }
            _ => {}
        }
        let result = run(
            &input,
            &mind,
            &mut state,
            now,
            Some(&situation),
            &[],
            None,
            case == 4,
        );
        assert!(result.claims.is_empty());
        assert_eq!(
            phase(&state),
            if case == 0 {
                Phase::Complete
            } else {
                Phase::Abandon
            }
        );
        assert_eq!(
            state.operation.as_ref().unwrap().reason,
            match case {
                0 => None,
                1 => Some("defense"),
                2 => Some("timeout"),
                3 => Some("members-unavailable"),
                _ => Some("handoff"),
            }
        );
    }
}

#[test]
fn intent_ai45_known_armor_withdraw_support_mitigates_and_cooldown_resumes_or_abandons() {
    let mut input = fixture();
    let mut tank = unit(9, "tank", 45.0);
    tank.owner = 1;
    input.units.push(tank);
    let mut mind = Mind::default();
    mind.update(&Observation::new(input.clone()).unwrap(), &[], false);
    let mut state = AssaultState::default();
    look(&input, &mind, &mut state, 200.0);
    assert_eq!(
        phase(&state),
        Phase::Assemble,
        "new contacts wait for notice"
    );
    apply(&mut input, &state);
    look(&input, &mind, &mut state, 200.5);
    assert_eq!(phase(&state), Phase::Withdraw);
    assert_eq!(state.operation.as_ref().unwrap().reason, Some("armor"));
    assert_eq!(state.cooldown, 218.5);
    apply(&mut input, &state);
    settle(&mut input, &state);
    input.units.iter_mut().find(|u| u.id == 9).unwrap().hp *= 0.9;
    let mut supported = AssaultState::default();
    run(
        &input,
        &mind,
        &mut supported,
        201.0,
        Some(&sit(&input)),
        &[],
        Some(200.0),
        false,
    );
    assert_ne!(
        phase(&supported),
        Phase::Withdraw,
        "recent support covers unmatched armor up to 80 percent of own value"
    );
    // Removing contacts and preserving progress allows regroup only at the fixed deadline.
    input.units.retain(|u| u.owner == 0);
    for u in &mut input.units {
        u.target_id = 99;
    }
    look(&input, &mind, &mut state, 218.499);
    assert_eq!(phase(&state), Phase::Withdraw);
    look(&input, &mind, &mut state, 218.5);
    assert_eq!(phase(&state), Phase::Regroup);
    assert_eq!(state.operation.as_ref().unwrap().state_at, 218.5);
    // A continuing threat abandons rather than renewing withdrawal forever.
    let mut blocked = AssaultState::default();
    let mut i = fixture();
    let mut tank = unit(9, "tank", 45.0);
    tank.owner = 1;
    i.units.push(tank);
    look(&i, &mind, &mut blocked, 201.0);
    apply(&mut i, &blocked);
    for u in &mut i.units {
        u.target_id = 99;
    }
    look(&i, &mind, &mut blocked, 219.0);
    assert_eq!(phase(&blocked), Phase::Abandon);
    assert_eq!(blocked.cooldown, 237.0);
}

#[test]
fn intent_ai45_position_exact_cover_unknown_block_height_region_los_and_search_cap() {
    let mut input = fixture();
    input.units = vec![unit(1, "rifle", 21.0)];
    input.flags.fill(MOVE);
    for x in 10..=12 {
        input.flags[10 * 80 + x] = 0;
    }
    let choose =
        |i: &Input, desired: Point, safe: bool, region: Option<[f64; 4]>, target: Option<Point>| {
            let obs = Observation::new(i.clone()).unwrap();
            operation_position(
                &obs,
                i.runtime.as_ref().unwrap(),
                &i.units[0],
                desired,
                &[],
                safe,
                region,
                target,
            )
            .unwrap()
        };
    let desired = Point { x: 25.0, z: 21.0 };
    assert_eq!(choose(&input, desired, false, None, None), Some(desired));
    input.flags[10 * 80 + 11] = COVER;
    assert_eq!(
        choose(&input, desired, false, None, None),
        Some(Point { x: 23.0, z: 21.0 })
    );
    input.chars[10 * 80 + 11] = b'?';
    assert_eq!(
        choose(&input, desired, false, None, None),
        Some(Point { x: 21.0, z: 21.0 })
    );
    input.chars[10 * 80 + 11] = b'.';
    input.flags[10 * 80 + 11] = 0;
    input.runtime.as_mut().unwrap().height = Some(vec![0; 3200]);
    input.runtime.as_mut().unwrap().height.as_mut().unwrap()[10 * 80 + 11] = 2;
    assert_eq!(
        choose(&input, desired, false, None, None),
        Some(Point { x: 21.0, z: 21.0 })
    );
    input.runtime.as_mut().unwrap().height = None;
    assert_eq!(
        choose(&input, desired, false, Some([20.0, 20.0, 24.0, 22.0]), None),
        Some(Point { x: 23.0, z: 21.0 })
    );
    input.units[0] = unit(1, "mg", 21.0);
    assert_eq!(
        choose(
            &input,
            desired,
            false,
            None,
            Some(Point { x: 159.0, z: 21.0 })
        ),
        None,
        "support requires weapon range and LOS"
    );
    input.flags[10 * 80 + 9] = SIGHT | MOVE;
    assert_eq!(
        choose(
            &input,
            desired,
            false,
            None,
            Some(Point { x: 1.0, z: 21.0 })
        ),
        None
    );
    input = fixture();
    input.width = 80;
    input.height = 80;
    input.flags = vec![0; 6400];
    input.chars = vec![b'.'; 6400];
    input.units = vec![unit(1, "rifle", 1.0)];
    input.units[0].at.z = 1.0;
    assert_eq!(
        choose(&input, Point { x: 159.0, z: 159.0 }, false, None, None),
        Some(Point { x: 65.0, z: 63.0 }),
        "2,048 queued cells preserve BFS tie order"
    );
    input.chars[0] = b'?';
    assert_eq!(choose(&input, desired, false, None, None), None);
}

#[test]
fn intent_ai45_threat_pressure_machine_guns_losses_and_safe_retreat_fallback() {
    for case in 0..3 {
        let mut input = fixture();
        let count = if case == 0 { 2 } else { 5 };
        for n in 0..count {
            let mut e = unit(
                10 + n,
                if case == 0 { "mg" } else { "rifle" },
                45.0 + n as f64,
            );
            e.owner = 1;
            input.units.push(e);
        }
        let mut mind = Mind::default();
        mind.update(&Observation::new(input.clone()).unwrap(), &[], false);
        let mut state = AssaultState::default();
        look(&input, &mind, &mut state, 200.0);
        apply(&mut input, &state);
        if case == 1 {
            // Exactly 1.25 pressure holds. The comparison is strict, so any increase withdraws.
            input.units.last_mut().unwrap().hp *= 0.375;
            look(&input, &mind, &mut state, 200.5);
            assert_eq!(phase(&state), Phase::Assemble);
            input.units.last_mut().unwrap().hp += 0.001;
        }
        if case == 2 {
            // A third of the original group lost is enough at pressure above 0.75.
            input.units.retain(|u| u.id != 3 && u.id < 12);
        }
        look(&input, &mind, &mut state, 201.0);
        assert_eq!(phase(&state), Phase::Withdraw);
        assert_eq!(
            state.operation.as_ref().unwrap().reason,
            Some(if case == 0 { "machine-gun" } else { "losses" })
        );
    }
    let mut input = fixture();
    let mut tank = unit(9, "tank", 45.0);
    tank.owner = 1;
    input.units.push(tank);
    input.flags.fill(MOVE);
    let mut mind = Mind::default();
    mind.update(&Observation::new(input.clone()).unwrap(), &[], false);
    let mut state = AssaultState::default();
    assert!(look(&input, &mind, &mut state, 200.0).commands.is_empty());
    let result = look(&input, &mind, &mut state, 200.5);
    assert_eq!(phase(&state), Phase::Withdraw);
    assert_eq!(
        result.commands,
        vec![
            json!({"t":"retreat","ids":[2]}),
            json!({"t":"retreat","ids":[1]}),
            json!({"t":"retreat","ids":[3]})
        ]
    );
}

#[test]
fn intent_ai45_assembly_deadline_remembered_contacts_and_withdrawal_safe_placement() {
    let mut input = fixture();
    let mut state = AssaultState::default();
    let mut mind = Mind::default();
    let mut enemy = unit(9, "tank", 45.0);
    enemy.owner = 1;
    input.units.push(enemy.clone());
    mind.update(&Observation::new(input.clone()).unwrap(), &[], false);
    input.units.pop();
    look(&input, &mind, &mut state, 200.0);
    apply(&mut input, &state);
    for u in &mut input.units {
        u.target_id = 99;
    }
    look(&input, &mind, &mut state, 211.999);
    assert_eq!(phase(&state), Phase::Assemble);
    look(&input, &mind, &mut state, 212.0);
    assert_eq!(phase(&state), Phase::Advance);
    // Deliver the newly issued advance route before the next planning look.
    apply(&mut input, &state);
    let memory = [Sighting {
        id: 9,
        kind: "tank".into(),
        owner: 1,
        at: enemy.at,
        time: 192.0,
        worth: 300.0,
    }];
    run(
        &input,
        &mind,
        &mut state,
        212.0,
        Some(&sit(&input)),
        &memory,
        None,
        false,
    );
    assert_eq!(
        phase(&state),
        Phase::Withdraw,
        "a noticed sighting is used through age 20 seconds"
    );
    let obs = Observation::new(input.clone()).unwrap();
    let threats = [Threat {
        id: 9,
        kind: "rifle".into(),
        at: Point { x: 21.0, z: 21.0 },
        worth: 100.0,
    }];
    let selected = operation_position(
        &obs,
        input.runtime.as_ref().unwrap(),
        &input.units[0],
        input.units[0].at,
        &threats,
        true,
        None,
        None,
    )
    .unwrap()
    .unwrap();
    assert!(dist(selected, threats[0].at) >= 26.0);
}
