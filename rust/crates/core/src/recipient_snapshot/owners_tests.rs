use super::*;
use crate::{
    economy::Faction,
    economy_state::ArmySize,
    field_works::FortKind,
    map::AuthoritativeMap,
    match_commands::MoveOrder,
    match_runtime::{MatchOptions, PlayerConfig},
    recipient_snapshot::DeliveryState,
    unit_publication::{PublicationParts, StagedUnit, prepare_publication},
    weather::WeatherChoice,
};

fn fixture() -> Match {
    let fixture: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/recipient-snapshot.json")).unwrap();
    super::super::tests::initialize(&fixture["cases"][0])
}

fn assist(target: i64, at: Point) -> Command {
    Command::Assist {
        ids: vec![1],
        target,
        at: Some(at),
        queue: false,
    }
}

fn dig(at: Point) -> Command {
    Command::Dig {
        ids: vec![1],
        kind: FortKind::Trench,
        at,
        direction: Some(0.),
        queue: false,
    }
}

#[test]
fn queued_assist_cover_and_dig_keep_source_kinds_anchors_and_owner_privacy() {
    let mut state = fixture();
    let at = Point { x: 41.34, z: 43.36 };
    state.commands.units.get_mut(&1).unwrap().orders.extend([
        assist(3, at),
        Command::Cover {
            ids: vec![1],
            at: Some(at),
            queue: false,
        },
        dig(at),
    ]);
    let expected = json!([[1, 3, 8, 41.3, 43.4, 1, 41.3, 43.4, 7, 41.3, 43.4]]);
    for visible in [true, false] {
        state.world.game.players[0].visible = if visible { vec![3] } else { vec![] };
        state.world.game.units.get_mut(&3).unwrap().x = 51.;
        let cache = SnapshotCache::new(&state).unwrap();
        assert_eq!(cache.recipient(0).unwrap().fields["orders"], expected);
        for slot in [1, 2] {
            assert_eq!(cache.recipient(slot).unwrap().fields["orders"], json!([]));
        }
    }
    state.world.game.units.remove(&3);
    assert_eq!(capture(&state).unwrap()[0]["orders"], expected);
}

#[test]
fn queued_owner_adapters_refuse_unnormalized_or_unrepresented_orders() {
    let at = Point { x: 41., z: 43. };
    let mut malformed = vec![
        assist(0, at),
        assist(-1, at),
        assist(i64::from(u32::MAX) + 1, at),
        Command::Assist {
            ids: vec![1],
            target: 3,
            at: None,
            queue: false,
        },
        Command::Cover {
            ids: vec![1],
            at: None,
            queue: false,
        },
        Command::Ability {
            ids: vec![1],
            at: None,
            queue: false,
            automatic: false,
        },
        Command::Build {
            ids: vec![1],
            kind: "flakpos".into(),
            at: Some(at),
            direction: None,
            queue: false,
        },
    ];
    for command in [
        assist(3, at),
        Command::Cover {
            ids: vec![1],
            at: Some(at),
            queue: false,
        },
        dig(at),
        Command::Enter {
            ids: vec![1],
            at,
            queue: false,
        },
    ] {
        let mut queued = command.clone();
        let mut wrong_actor = command.clone();
        let mut grouped = command;
        for (candidate, ids_value, queued_value) in [
            (&mut queued, vec![1], true),
            (&mut wrong_actor, vec![2], false),
            (&mut grouped, vec![1, 2], false),
        ] {
            match candidate {
                Command::Assist { ids, queue, .. }
                | Command::Cover { ids, queue, .. }
                | Command::Dig { ids, queue, .. }
                | Command::Enter { ids, queue, .. } => {
                    *ids = ids_value;
                    *queue = queued_value;
                }
                _ => unreachable!(),
            }
        }
        malformed.extend([queued, wrong_actor, grouped]);
    }
    let mut bad_direction = dig(at);
    if let Command::Dig { direction, .. } = &mut bad_direction {
        *direction = Some(f64::INFINITY);
    }
    malformed.push(bad_direction);
    for command in malformed {
        let mut state = fixture();
        state
            .commands
            .units
            .get_mut(&1)
            .unwrap()
            .orders
            .push_back(command.clone());
        assert!(
            matches!(
                capture(&state),
                Err(SnapshotError::Unsupported {
                    unit: 1,
                    capability: SnapshotCapability::Orders,
                })
            ),
            "{command:?}"
        );
    }
    for command in [
        assist(
            3,
            Point {
                x: f64::NAN,
                z: 43.,
            },
        ),
        dig(Point {
            x: 41.,
            z: f64::INFINITY,
        }),
        Command::Enter {
            ids: vec![1],
            at: Point {
                x: f64::NAN,
                z: 43.,
            },
            queue: false,
        },
        Command::Enter {
            ids: vec![1],
            at: Point {
                x: 41.,
                z: f64::INFINITY,
            },
            queue: false,
        },
    ] {
        let mut state = fixture();
        state
            .commands
            .units
            .get_mut(&1)
            .unwrap()
            .orders
            .push_back(command);
        assert!(matches!(capture(&state), Err(SnapshotError::InvalidNumber)));
    }
}

fn ordinary_scene(cover: bool) -> (Match, u32) {
    let mut rows = vec![".".repeat(60); 60];
    if cover {
        rows[12].replace_range(15..16, "#");
    }
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"Owner queue lifecycle", "w":60,"h":60,"rows":rows,
            "heights":vec!["0".repeat(60);60],
            "spawns":[{"x":4,"y":4},{"x":55,"y":55}],"points":[{"x":30,"y":30}]
        }))
        .unwrap(),
    )
    .unwrap();
    let mut state = Match::new(
        &map,
        MatchOptions {
            seed: 701,
            mode: Mode::Default,
            players: (0..2)
                .map(|slot| PlayerConfig {
                    name: format!("p{slot}"),
                    team: slot,
                    faction: Faction::try_from(slot as u8).unwrap(),
                })
                .collect(),
            shuffle: false,
            army: ArmySize::Standard,
            weather: WeatherChoice::Map,
            weather_seed: Some(1),
            living_weather: false,
            map_key: None,
            tech: false,
            supply: false,
            logistics: false,
            sandbox: false,
        },
    )
    .unwrap();
    state.start.economy.players[0].mp = 5000.;
    let worker = state.next_unit_id;
    let staged = StagedUnit::ordinary(
        &state.world.game,
        &state.rules,
        0,
        state.rules.kind("engineer").unwrap(),
        worker,
        state.world.game.units.len() as u32,
        state.start.players[0].spawn,
        Some(Point { x: 28., z: 24. }),
    )
    .unwrap();
    prepare_publication(
        PublicationParts {
            units: &mut state.world.game.units,
            grid: &mut state.grid,
            controls: &mut state.commands.units,
            recruitment: &mut state.recruit_metadata,
            next_id: &mut state.next_unit_id,
        },
        &state.rules,
        2,
        vec![staged],
    )
    .unwrap()
    .publish_with(|u, _| {
        state.combat.initialize(u.id);
        state.firing.initialize(u.id);
        state.abilities.initialize(u.id);
        state.recovery.initialize(u.id);
    });
    (state, worker)
}

fn deliver(state: &mut Match, delivery: &mut DeliveryState) -> [Value; 2] {
    let frame = delivery
        .capture_with_lists(state, &[true; 2], &[false; 2], &[])
        .unwrap();
    [0, 1].map(|slot| frame.recipient(slot).unwrap().into_json())
}

#[test]
fn queued_paid_build_becomes_assist_and_survives_ordinary_ticks_and_delivery() {
    let (mut state, worker) = ordinary_scene(false);
    let site = state.next_unit_id;
    let at = Point { x: 32., z: 24. };
    assert_eq!(
        state
            .execute_build(0, &[i64::from(worker)], "flakpos", Some(at), None, true)
            .unwrap(),
        None
    );
    assert!(matches!(
        state.commands.units[&worker].orders.front(),
        Some(Command::Assist { target, queue: false, .. }) if *target == i64::from(site)
    ));
    let mut delivery = DeliveryState::new(&state).unwrap();
    let rows = deliver(&mut state, &mut delivery);
    assert_eq!(rows[0]["orders"], json!([[worker, 1, 8, 32., 24.]]));
    assert_eq!(rows[1]["orders"], json!([]));
    for _ in 0..4 {
        let report = state.advance_tick().unwrap();
        assert_eq!(report.environment().count(), 0);
        let rows = deliver(&mut state, &mut delivery);
        assert_eq!(rows[0]["orders"], json!([]));
        assert_eq!(rows[1]["orders"], json!([]));
        assert_eq!(state.commands.units[&worker].build_target, Some(site));
        assert!(state.world.game.units[&worker].build);
    }
}

#[test]
fn queued_cover_retains_move_anchor_and_survives_activation_and_delivery() {
    let (mut state, worker) = ordinary_scene(true);
    let at = state.world.game.units[&worker].at();
    assert_eq!(
        state
            .execute_command(
                0,
                &Command::Move {
                    orders: vec![MoveOrder {
                        unit: i64::from(worker),
                        at
                    }],
                    control_ids: None,
                    attack_move: false,
                    queue: true,
                    together: false,
                    face: Some(0.),
                },
                false
            )
            .unwrap(),
        None
    );
    assert_eq!(
        state
            .execute_command(
                0,
                &Command::Cover {
                    ids: vec![i64::from(worker)],
                    at: None,
                    queue: true,
                },
                false
            )
            .unwrap(),
        None
    );
    let mut delivery = DeliveryState::new(&state).unwrap();
    let rows = deliver(&mut state, &mut delivery);
    assert_eq!(
        rows[0]["orders"],
        json!([[worker, 2, 1, 28., 24., 1, 28., 24.]])
    );
    assert_eq!(rows[1]["orders"], json!([]));
    for _ in 0..4 {
        let report = state.advance_tick().unwrap();
        assert_eq!(report.environment().count(), 0);
        let rows = deliver(&mut state, &mut delivery);
        assert_eq!(rows[1]["orders"], json!([]));
    }
    assert!(state.commands.units[&worker].orders.is_empty());
    assert!(state.world.game.units[&worker].world_goal.is_none());
}

#[test]
fn queued_dig_then_cover_survives_payment_or_skip_and_complete_delivery() {
    for funded in [true, false] {
        let (mut state, worker) = ordinary_scene(false);
        if !funded {
            state.start.economy.players[0].mp = 0.;
        }
        let wallet = state.start.economy.players[0].mp;
        let at = Point { x: 30., z: 24. };
        assert_eq!(
            state
                .execute_command(
                    0,
                    &Command::Dig {
                        ids: vec![i64::from(worker)],
                        kind: FortKind::Trench,
                        at,
                        direction: Some(0.),
                        queue: true,
                    },
                    false,
                )
                .unwrap(),
            None
        );
        assert_eq!(
            state
                .execute_command(
                    0,
                    &Command::Cover {
                        ids: vec![i64::from(worker)],
                        at: None,
                        queue: true,
                    },
                    false,
                )
                .unwrap(),
            None
        );
        assert_eq!(state.start.economy.players[0].mp, wallet);
        assert!(matches!(
            state.commands.units[&worker].orders.back(),
            Some(Command::Cover { at: Some(anchor), queue: false, .. }) if *anchor == at
        ));
        let mut delivery = DeliveryState::new(&state).unwrap();
        let rows = deliver(&mut state, &mut delivery);
        assert_eq!(
            rows[0]["orders"],
            json!([[worker, 2, 7, 30., 24., 1, 30., 24.]])
        );
        assert_eq!(rows[1]["orders"], json!([]));
        let mut saw_dig = false;
        for _ in 0..if funded { 160 } else { 4 } {
            let report = state.advance_tick().unwrap();
            assert_eq!(report.environment().count(), 0);
            let rows = deliver(&mut state, &mut delivery);
            assert_eq!(rows[1]["orders"], json!([]));
            if state.field_works.is_active(worker) {
                saw_dig = true;
                assert_eq!(rows[0]["orders"], json!([[worker, 1, 1, 30., 24.]]));
            }
        }
        assert_eq!(saw_dig, funded);
        assert!(!state.field_works.is_active(worker));
        assert!(!state.world.game.units[&worker].dig);
        assert!(state.commands.units[&worker].orders.is_empty());
        assert_eq!(state.story[0].mp_spent, if funded { 30. } else { 0. });
        assert_eq!(state.world.game.chars.contains(&b'T'), funded);
    }
}

#[test]
fn queued_enter_keeps_source_kind_house_anchor_and_owner_privacy() {
    let mut state = fixture();
    let at = Point { x: 41.34, z: 43.36 };
    state
        .commands
        .units
        .get_mut(&1)
        .unwrap()
        .orders
        .push_back(Command::Enter {
            ids: vec![1],
            at,
            queue: false,
        });
    let cache = SnapshotCache::new(&state).unwrap();
    assert_eq!(
        cache.recipient(0).unwrap().fields["orders"],
        json!([[1, 1, 9, 41.3, 43.4]])
    );
    // The fixture includes another seat on the owner's team as well as an enemy.
    for slot in [1, 2] {
        assert_eq!(cache.recipient(slot).unwrap().fields["orders"], json!([]));
    }
}

#[test]
fn queued_enter_while_moving_survives_every_seat_delivery_through_ordinary_finish() {
    use crate::{
        match_end::EndReason,
        mov_audit_test_support::{point, scene_rows},
    };
    let mut rows = vec![".".repeat(64); 64];
    rows[12].replace_range(18..19, "B");
    let mut state = scene_rows(rows);
    state.start.win_vp = 5.;
    for control in state.commands.units.values_mut() {
        control.hold_fire = true;
        control.auto_cast = false;
    }
    for unit in state.world.game.units.values_mut() {
        unit.hold_pos = true;
    }
    let id = state.next_unit_id;
    let staged = StagedUnit::ordinary(
        &state.world.game,
        &state.rules,
        0,
        state.rules.kind("rifle").unwrap(),
        id,
        state.world.game.units.len() as u32,
        state.start.players[0].spawn,
        Some(point(25., 25.)),
    )
    .unwrap();
    state.publish_starting_unit(staged).unwrap();
    state.commands.units.get_mut(&id).unwrap().hold_fire = true;
    let mut delivery = DeliveryState::new(&state).unwrap();
    deliver(&mut state, &mut delivery);
    assert_eq!(
        state
            .execute_command(
                0,
                &Command::Move {
                    orders: vec![MoveOrder {
                        unit: i64::from(id),
                        at: point(31., 25.)
                    }],
                    control_ids: None,
                    attack_move: false,
                    queue: false,
                    together: false,
                    face: None,
                },
                false
            )
            .unwrap(),
        None
    );
    let move_goal = state.world.game.units[&id].world_goal.unwrap();
    state.advance_tick().unwrap();
    deliver(&mut state, &mut delivery);
    assert!(!state.world.game.units[&id].path.is_empty());
    assert_eq!(
        state
            .execute_command(
                0,
                &Command::Enter {
                    ids: vec![i64::from(id)],
                    at: point(37., 25.),
                    queue: true,
                },
                false
            )
            .unwrap(),
        None
    );
    let snapshots = deliver(&mut state, &mut delivery);
    assert_eq!(snapshots[0]["orders"], json!([[id, 1, 9, 37., 25.]]));
    assert_eq!(snapshots[1]["orders"], json!([]));
    let mut queued_moving_ticks = 0;
    let mut garrisoned = false;
    for _ in 0..1600 {
        state.advance_tick().unwrap();
        let snapshots = deliver(&mut state, &mut delivery);
        assert_eq!(snapshots[1]["orders"], json!([]));
        if !state.commands.units[&id].orders.is_empty() {
            queued_moving_ticks += usize::from(!state.world.game.units[&id].path.is_empty());
            assert_eq!(snapshots[0]["orders"], json!([[id, 1, 9, 37., 25.]]));
            if state.world.game.units[&id].path.is_empty() {
                // Orders are consumed before movement, so the final route tick still queues Enter.
                assert_eq!(state.world.game.units[&id].at(), move_goal);
            }
        }
        if !garrisoned && state.world.game.units[&id].garrison.is_some() {
            garrisoned = true;
            assert!(state.commands.units[&id].orders.is_empty());
            assert_eq!(state.world.game.units[&id].garrison, Some(12 * 64 + 18));
            // A second squad captures the neutral objective and earns the winning VP.
            let capturer = state.next_unit_id;
            let staged = StagedUnit::ordinary(
                &state.world.game,
                &state.rules,
                0,
                state.rules.kind("rifle").unwrap(),
                capturer,
                state.world.game.units.len() as u32,
                state.start.players[0].spawn,
                Some(state.start.points[0].at),
            )
            .unwrap();
            state.publish_starting_unit(staged).unwrap();
            state.commands.units.get_mut(&capturer).unwrap().hold_fire = true;
        }
        if state.finish.is_some() {
            break;
        }
    }
    assert!(
        queued_moving_ticks > 1,
        "entry must remain queued behind the moving route"
    );
    assert!(
        garrisoned,
        "the queued entry must activate and reach the house"
    );
    let finish = state
        .finish
        .expect("ordinary match must finish from objective income");
    assert_eq!(finish.winner, 0);
    assert_eq!(finish.reason, EndReason::VictoryPoints);
    assert!(state.start.economy.players[0].vp >= state.start.win_vp);
    for _ in 0..10 {
        state.advance_tick().unwrap();
        deliver(&mut state, &mut delivery);
    }
    assert!(state.tick_state.fault().is_none());
}

#[test]
fn queued_ability_uses_prototype_kind_six_rounded_anchor_and_owner_privacy() {
    let mut state = fixture();
    state
        .commands
        .units
        .get_mut(&1)
        .unwrap()
        .orders
        .push_back(Command::Ability {
            ids: vec![1],
            at: Some(Point { x: 41.34, z: 43.36 }),
            queue: false,
            automatic: false,
        });
    let cache = SnapshotCache::new(&state).unwrap();
    assert_eq!(
        cache.recipient(0).unwrap().fields["orders"],
        json!([[1, 1, 6, 41.3, 43.4]])
    );
    for slot in [1, 2] {
        assert_eq!(cache.recipient(slot).unwrap().fields["orders"], json!([]));
    }
    for command in [
        Command::Ability {
            ids: vec![1, 2],
            at: Some(Point { x: 41., z: 43. }),
            queue: false,
            automatic: false,
        },
        Command::Ability {
            ids: vec![1],
            at: Some(Point { x: 41., z: 43. }),
            queue: true,
            automatic: false,
        },
        Command::Ability {
            ids: vec![1],
            at: Some(Point { x: 41., z: 43. }),
            queue: false,
            automatic: true,
        },
    ] {
        state.commands.units.get_mut(&1).unwrap().orders = [command].into();
        assert!(SnapshotCache::new(&state).is_err());
    }
}
