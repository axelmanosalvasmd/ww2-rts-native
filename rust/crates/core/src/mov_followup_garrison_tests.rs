//! Garrison intent at prototype e4e579a, with ordinary-match delivery continuations.
use crate::{
    catalog::Catalog,
    economy::Mode,
    game::{MOVE, Point},
    match_commands::{Command, Denial, MoveOrder},
    match_end::EndReason,
    match_runtime::Match,
    mov_audit_test_support::{point, scene_rows},
    recipient_snapshot::DeliveryState,
    unit_publication::StagedUnit,
};
use serde_json::{Value, json};

struct Arena {
    state: Match,
    delivery: DeliveryState,
}
impl Arena {
    fn new(mode: Mode, rows: Vec<String>) -> Self {
        let mut state = scene_rows(rows);
        state.options.mode = mode;
        state.start.win_vp = 1.;
        state.world.game.reveal = true;
        for unit in state.world.game.units.values_mut() {
            unit.hold_pos = true;
        }
        for control in state.commands.units.values_mut() {
            control.hold_fire = true;
            control.auto_cast = false;
            control.auto_retreat = false;
        }
        let delivery = DeliveryState::new(&state).unwrap();
        let mut arena = Self { state, delivery };
        arena.snapshots();
        arena
    }
    fn house(mode: Mode) -> Self {
        let mut rows = vec![".".repeat(64); 64];
        rows[20].replace_range(20..22, "BB");
        Self::new(mode, rows)
    }
    fn add(&mut self, kind: &str, at: Point) -> u32 {
        let id = self.state.next_unit_id;
        let draft = StagedUnit::ordinary(
            &self.state.world.game,
            &self.state.rules,
            0,
            self.state.rules.kind(kind).unwrap(),
            id,
            self.state.world.game.units.len() as u32,
            self.state.start.players[0].spawn,
            Some(at),
        )
        .unwrap();
        self.state.publish_starting_unit(draft).unwrap();
        let c = self.state.commands.units.get_mut(&id).unwrap();
        c.hold_fire = true;
        c.auto_cast = false;
        c.auto_retreat = false;
        self.state.world.game.units.get_mut(&id).unwrap().hold_pos = true;
        id
    }
    fn enter(&mut self, id: u32, at: Point, queue: bool) -> Option<Denial> {
        self.state
            .execute_command(
                0,
                &Command::Enter {
                    ids: vec![i64::from(id)],
                    at,
                    queue,
                },
                false,
            )
            .unwrap()
    }
    fn snapshots(&mut self) -> [Value; 2] {
        let frame = self
            .delivery
            .capture_with_lists(&mut self.state, &[true; 2], &[false; 2], &[])
            .unwrap();
        [0, 1].map(|slot| frame.recipient(slot).unwrap().into_json())
    }
    fn tick(&mut self) -> [Value; 2] {
        self.state.advance_tick().unwrap();
        assert!(self.state.tick_state.fault().is_none());
        self.snapshots()
    }
    fn wait_inside(&mut self, id: u32) {
        for _ in 0..400 {
            if self.state.world.game.units[&id].garrison.is_some() {
                return;
            }
            self.tick();
        }
        panic!("squad {id} did not enter its reserved cell");
    }
    fn finish(mut self) {
        assert!(self.state.finish.is_none(), "behavior must precede victory");
        self.add("rifle", self.state.start.points[0].at);
        for _ in 0..1600 {
            self.tick();
            if self.state.finish.is_some() {
                break;
            }
        }
        let end = self
            .state
            .finish
            .expect("capture and income must finish the match");
        assert_eq!(end.reason, EndReason::VictoryPoints);
        assert_eq!(end.winner, 0);
        assert!(self.state.start.economy.players[0].vp >= self.state.start.win_vp);
        for _ in 0..8 {
            self.tick();
        }
    }
}

#[test]
fn queued_enter_then_cover_keeps_house_anchor_and_survives_delivery() {
    let mut arena = Arena::house(Mode::Conquest);
    let id = arena.add("rifle", point(25., 41.));
    assert_eq!(
        arena
            .state
            .execute_command(
                0,
                &Command::Move {
                    orders: vec![MoveOrder {
                        unit: i64::from(id),
                        at: point(33., 41.)
                    }],
                    control_ids: None,
                    attack_move: false,
                    queue: false,
                    together: false,
                    face: None,
                },
                false,
            )
            .unwrap(),
        None
    );
    assert_eq!(arena.enter(id, point(41., 41.), true), None);
    let before = arena.state.world.game.units[&id].path.clone();
    assert_eq!(
        arena
            .state
            .execute_command(
                0,
                &Command::Cover {
                    ids: vec![i64::from(id)],
                    queue: true,
                    at: None
                },
                false,
            )
            .unwrap(),
        None
    );
    assert_eq!(arena.state.world.game.units[&id].path, before);
    assert!(matches!(
        arena.state.commands.units[&id].orders.back(),
        Some(Command::Cover { queue: false, at: Some(at), .. }) if *at == point(41., 41.)
    ));
    arena.snapshots();
    for _ in 0..400 {
        arena.tick();
        if arena.state.commands.units[&id].orders.is_empty()
            && arena.state.world.game.units[&id].garrison.is_some()
        {
            break;
        }
    }
    assert_eq!(
        arena.state.world.game.units[&id].garrison,
        Some(20 * 64 + 20)
    );
    assert!(arena.state.commands.units[&id].orders.is_empty());
    assert!(arena.state.world.game.units[&id].path.is_empty());
    arena.finish();
}

#[test]
fn cover_queues_during_entrenchment_and_immediate_cover_cancels_only_its_worker() {
    let mut rows = vec![".".repeat(64); 64];
    rows[16].replace_range(14..19, "TTTTT");
    let mut arena = Arena::new(Mode::Conquest, rows);
    let id = arena.add("rifle", point(31., 33.));
    let other = arena.add("rifle", point(35., 33.));
    let entrench = Command::from_json(&json!({
        "t":"entrench", "ids":[id,other], "pattern":"line",
        "x":40, "z":49, "x2":56, "z2":49
    }))
    .unwrap();
    assert_eq!(
        arena.state.execute_command(0, &entrench, false).unwrap(),
        None
    );
    let project = arena.state.field_works.membership(id).unwrap();
    arena.tick();
    assert!(arena.state.field_works.is_active(id));
    let work = arena.state.field_works.clone();
    let path = arena.state.world.game.units[&id].path.clone();
    let cover = |queue| Command::Cover {
        ids: vec![i64::from(id)],
        queue,
        at: None,
    };
    assert_eq!(
        arena.state.execute_command(0, &cover(true), false).unwrap(),
        None
    );
    assert_eq!(arena.state.field_works, work);
    assert_eq!(arena.state.world.game.units[&id].path, path);
    assert_eq!(arena.state.commands.units[&id].orders.len(), 1);
    for _ in 0..3 {
        arena.tick();
        assert_eq!(arena.state.commands.units[&id].orders.len(), 1);
        assert_eq!(arena.state.field_works.membership(id), Some(project));
    }
    let wallet = arena.state.start.economy.players[0].mp;
    assert_eq!(
        arena
            .state
            .execute_command(0, &cover(false), false)
            .unwrap(),
        None
    );
    assert_eq!(
        arena.state.start.economy.players[0].mp, wallet,
        "paid work has no refund"
    );
    assert!(!arena.state.world.game.units[&id].entrench);
    assert!(!arena.state.world.game.units[&id].dig);
    assert!(!arena.state.field_works.is_active(id));
    assert_eq!(arena.state.field_works.membership(id), None);
    assert_eq!(arena.state.field_works.membership(other), Some(project));
    assert!(arena.state.field_works.is_active(other));
    assert!(arena.state.commands.units[&id].orders.is_empty());
    for _ in 0..8 {
        arena.tick();
    }
    arena.finish();
}

#[test]
fn enter_caps_raw_selection_before_filtering_for_small_and_large_armies() {
    for (large, queue) in [(false, false), (false, true), (true, false), (true, true)] {
        let mut arena = Arena::house(Mode::Conquest);
        let id = arena.add("rifle", point(31., 41.));
        if large {
            while arena.state.world.game.units.len() < 51 {
                arena.add("rifle", point(91., 91.));
            }
        } else {
            assert!(arena.state.world.game.units.len() < 50);
        }
        let limit = 50.max(arena.state.world.game.units.len());
        let mut ids = vec![-1; limit];
        ids.push(i64::from(id));
        let enter = |ids| Command::Enter {
            ids,
            at: point(41., 41.),
            queue,
        };
        let control = arena.state.commands.units[&id].clone();
        let unit = arena.state.world.game.units[&id].clone();
        assert_eq!(
            arena
                .state
                .execute_command(0, &enter(ids.clone()), false)
                .unwrap(),
            Some(Denial::Needs)
        );
        assert_eq!(arena.state.commands.units[&id], control);
        assert_eq!(
            format!("{:?}", arena.state.world.game.units[&id]),
            format!("{unit:?}")
        );
        ids.remove(0);
        assert_eq!(
            arena.state.execute_command(0, &enter(ids), false).unwrap(),
            None
        );
        if queue {
            assert_eq!(arena.state.commands.units[&id].orders.len(), 1);
        } else {
            assert_eq!(arena.state.commands.units[&id].enter, Some(20 * 64 + 20));
        }
        arena.snapshots();
    }
}

#[test]
fn eng33_cmb142_all_nine_squad_kinds_enter_at_the_door_and_finish() {
    // The eligibility list and 3.2 m door radius are exact prototype values.
    for kind in [
        "rifle",
        "mg",
        "sniper",
        "medic",
        "flamer",
        "ranger",
        "conscript",
        "commando",
        "engineer",
    ] {
        let mut arena = Arena::house(Mode::Conquest);
        assert_eq!(
            Catalog::compiled()
                .unwrap()
                .unit(kind)
                .unwrap()
                .get("garrisons")
                .and_then(|v| v.boolean()),
            Some(true)
        );
        let id = arena.add(kind, point(30., 41.));
        assert_eq!(arena.enter(id, point(41., 41.), false), None, "{kind}");
        assert_eq!(arena.state.commands.units[&id].enter, Some(20 * 64 + 20));
        assert!(arena.state.world.game.units[&id].garrison.is_none());
        arena.tick();
        assert!(
            arena.state.world.game.units[&id].garrison.is_none(),
            "entry must wait for the walk"
        );
        arena.wait_inside(id);
        let u = &arena.state.world.game.units[&id];
        assert_eq!(u.at(), point(41., 41.));
        assert!(u.path.is_empty());
        assert_eq!(arena.state.commands.units[&id].enter, None);
        arena.finish();
    }
}

#[test]
fn eng33_cmb142_nearest_free_cells_reserve_before_arrival_and_exclude_enclosed_cells() {
    let mut arena = Arena::house(Mode::Conquest);
    let left = arena.add("rifle", point(30., 41.));
    let right = arena.add("mg", point(54., 41.));
    let third = arena.add("sniper", point(41., 31.));
    assert_eq!(arena.enter(left, point(43., 41.), false), None);
    assert_eq!(arena.state.commands.units[&left].enter, Some(20 * 64 + 20));
    assert_eq!(arena.enter(right, point(41., 41.), false), None);
    assert_eq!(arena.state.commands.units[&right].enter, Some(20 * 64 + 21));
    assert!(arena.state.world.game.units[&left].garrison.is_none());
    assert!(arena.state.world.game.units[&right].garrison.is_none());
    assert_eq!(
        arena.enter(third, point(41., 41.), false),
        Some(Denial::Maximum)
    );
    arena.wait_inside(left);
    arena.wait_inside(right);
    assert_ne!(
        arena.state.world.game.units[&left].garrison,
        arena.state.world.game.units[&right].garrison
    );
    assert_eq!(
        arena.enter(third, point(43., 41.), false),
        Some(Denial::Maximum)
    );
    arena.finish();

    // The closer left cell has no open four-neighbour; only the right cell qualifies.
    let mut rows = vec![".".repeat(64); 64];
    rows[19].replace_range(20..21, "W");
    rows[20].replace_range(19..22, "WBB");
    rows[21].replace_range(20..21, "W");
    let mut arena = Arena::new(Mode::Conquest, rows);
    let id = arena.add("rifle", point(37., 37.));
    assert_eq!(arena.enter(id, point(41., 41.), false), None);
    assert_eq!(arena.state.commands.units[&id].enter, Some(20 * 64 + 21));
    arena.wait_inside(id);
    arena.finish();
}

#[test]
fn eng33_entry_radius_is_inclusive_at_exactly_three_point_two_metres() {
    for at_boundary in [false, true] {
        let mut rows = vec![".".repeat(64); 64];
        rows[20].replace_range(0..1, "B");
        let mut arena = Arena::new(Mode::Conquest, rows);
        let x = if at_boundary {
            4.2_f64
        } else {
            f64::from_bits(4.2_f64.to_bits() + 1)
        };
        let id = arena.add("rifle", point(x, 41.));
        assert_eq!(arena.state.world.game.units[&id].at(), point(x, 41.));
        let distance = crate::math::hypot(x - 1., 0.);
        if at_boundary {
            assert_eq!(distance, 3.2);
        } else {
            assert!(distance > 3.2);
        }
        assert_eq!(arena.enter(id, point(1., 41.), false), None);
        arena.state.apply_unit_entry_with_routes(id).unwrap();
        assert_eq!(
            arena.state.world.game.units[&id].garrison,
            at_boundary.then_some(20 * 64)
        );
        if at_boundary {
            assert_eq!(arena.state.world.game.units[&id].at(), point(1., 41.));
        } else {
            assert_eq!(arena.state.world.game.units[&id].at(), point(x, 41.));
            assert_eq!(arena.state.commands.units[&id].enter, Some(20 * 64));
        }
        arena.wait_inside(id);
        arena.finish();
    }
}

#[test]
fn cmb142_denials_preserve_eligibility_visibility_retreat_and_queue_limit() {
    let mut arena = Arena::house(Mode::Conquest);
    let tank = arena.add("tank", point(31., 41.));
    assert_eq!(
        arena.enter(tank, point(41., 41.), false),
        Some(Denial::Needs)
    );
    let id = arena.add("rifle", point(31., 31.));
    assert_eq!(
        arena.enter(id, point(-1., 41.), false),
        Some(Denial::Blocked)
    );
    assert_eq!(
        arena.enter(id, point(35., 35.), false),
        Some(Denial::Blocked)
    );
    arena.state.world.game.reveal = false;
    assert_eq!(
        arena.enter(id, point(115., 15.), false),
        Some(Denial::NotVisible)
    );
    arena.state.world.game.reveal = true;
    assert_eq!(
        arena
            .state
            .execute_command(
                0,
                &Command::Retreat {
                    ids: vec![i64::from(id)]
                },
                false
            )
            .unwrap(),
        None
    );
    assert_eq!(
        arena.enter(id, point(41., 41.), false),
        Some(Denial::Retreating)
    );
    let queued = arena.add("rifle", point(31., 51.));
    for _ in 0..8 {
        assert_eq!(arena.enter(queued, point(41., 41.), true), None);
    }
    assert_eq!(
        arena.enter(queued, point(41., 41.), true),
        Some(Denial::QueueFull)
    );
    assert_eq!(arena.state.commands.units[&queued].orders.len(), 8);
    assert_eq!(
        arena.state.commands.units[&queued].enter, None,
        "queued entry reserves only on activation"
    );
    arena.wait_inside(queued);
    arena.finish();
}

#[test]
fn eng33_queued_entry_activates_after_move_and_destroyed_house_cancels_it() {
    for destroy in [false, true] {
        let mut arena = Arena::house(Mode::Conquest);
        let id = arena.add("rifle", point(25., 41.));
        let move_order = Command::Move {
            orders: vec![MoveOrder {
                unit: i64::from(id),
                at: point(31., 41.),
            }],
            control_ids: None,
            attack_move: false,
            queue: false,
            together: false,
            face: None,
        };
        assert_eq!(
            arena.state.execute_command(0, &move_order, false).unwrap(),
            None
        );
        assert_eq!(arena.enter(id, point(41., 41.), true), None);
        let mut moving_queued = 0;
        let mut reserved = false;
        for _ in 0..400 {
            let snapshots = arena.tick();
            assert_eq!(snapshots[1]["orders"], serde_json::json!([]));
            if !arena.state.commands.units[&id].orders.is_empty() {
                moving_queued += usize::from(!arena.state.world.game.units[&id].path.is_empty());
                assert_eq!(
                    snapshots[0]["orders"],
                    serde_json::json!([[id, 1, 9, 41., 41.]])
                );
                assert_eq!(arena.state.commands.units[&id].enter, None);
            }
            if arena.state.commands.units[&id].enter.is_some() {
                reserved = true;
                break;
            }
        }
        assert!(moving_queued > 1 && reserved);
        assert!(arena.state.commands.units[&id].orders.is_empty());
        if destroy {
            for cell in [20 * 64 + 20, 20 * 64 + 21] {
                arena.state.world.set_cell(cell, b'.').unwrap();
            }
            arena.tick();
            assert_eq!(arena.state.commands.units[&id].enter, None);
            assert_eq!(arena.state.world.game.units[&id].garrison, None);
        } else {
            arena.wait_inside(id);
        }
        arena.finish();
    }
}

#[test]
fn mov48_eng33_manual_and_wounded_garrisons_exit_then_reach_home_through_finish() {
    for mode in [Mode::Default, Mode::Conquest] {
        for automatic in [false, true] {
            let mut arena = Arena::house(mode);
            let id = arena.add("rifle", point(35., 41.));
            assert_eq!(arena.enter(id, point(41., 41.), false), None);
            arena.wait_inside(id);
            if automatic {
                arena.state.world.game.units.get_mut(&id).unwrap().hp = 20.;
                arena
                    .state
                    .commands
                    .units
                    .get_mut(&id)
                    .unwrap()
                    .auto_retreat = true;
                assert!(arena.state.automatic_retreat_candidate(id).unwrap());
                arena.tick();
            } else {
                assert_eq!(
                    arena
                        .state
                        .execute_command(
                            0,
                            &Command::Retreat {
                                ids: vec![i64::from(id)]
                            },
                            false
                        )
                        .unwrap(),
                    None
                );
                arena.snapshots();
            }
            let unit = &arena.state.world.game.units[&id];
            assert_eq!(unit.garrison, None);
            assert!(unit.retreating && !unit.path.is_empty());
            let cell = arena.state.world.game.cell_of(unit.x, unit.z).unwrap();
            assert_eq!(arena.state.world.game.flags[cell] & MOVE, 0);
            let home = arena.state.reaction_home(id).unwrap();
            assert_eq!(unit.world_goal, Some(home));
            let destination = arena.state.commands.units[&id].route_end.unwrap();
            for _ in 0..1200 {
                arena.tick();
                if !arena.state.world.game.units[&id].retreating {
                    break;
                }
            }
            let unit = &arena.state.world.game.units[&id];
            assert!(!unit.retreating && unit.world_goal.is_none());
            assert!(crate::math::hypot(unit.x - destination.x, unit.z - destination.z) <= 0.25);
            assert!(crate::math::hypot(unit.x - home.x, unit.z - home.z) <= 15.);
            arena.finish();
        }
    }
}
