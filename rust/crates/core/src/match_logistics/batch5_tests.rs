//! Manual convoy adapters stay below the full logistics tick and snapshot guards.
use super::runtime_tests::{arena, arena_with_barrier, place, troop};
use super::*;
use crate::{
    match_commands::{Command, Denial, MoveOrder},
    recipient_snapshot::{DeliveryState, SelectedSeat, SnapshotCapability, SnapshotError},
    supply_convoys::{StoreId, TruckState},
};

fn cargo(m: &mut Match, truck: u32) -> logistics::Stock {
    let stock = logistics::Stock {
        ammo: 3.,
        provisions: 24.,
        fuel: 6.,
    };
    m.convoys
        .as_mut()
        .unwrap()
        .trucks
        .get_mut(&truck)
        .unwrap()
        .cargo = stock;
    stock
}
fn move_to(truck: u32, at: Point, queue: bool) -> Command {
    Command::Move {
        orders: vec![MoveOrder {
            unit: i64::from(truck),
            at,
        }],
        control_ids: None,
        attack_move: false,
        queue,
        together: false,
        face: None,
    }
}
fn assert_guards(m: &mut Match) {
    let before = format!("{:?}", (&m.world.game.units, &m.convoys, &m.commands.units));
    let tick = m.world.game.tick;
    assert_eq!(
        m.advance_tick().unwrap_err().reason,
        "reserve and convoy tick phases are not connected"
    );
    assert_eq!(m.world.game.tick, tick);
    assert!(m.tick_state.fault().is_none());
    assert_eq!(
        format!("{:?}", (&m.world.game.units, &m.convoys, &m.commands.units)),
        before
    );
    assert!(matches!(
        m.preflight_ordinary_snapshot(),
        Err(SnapshotError::Unsupported {
            capability: SnapshotCapability::Logistics,
            ..
        })
    ));
}

#[test]
fn intent_log39_match_supply_hold_resume_queued_moves_and_nearest_source_retreat() {
    let mut m = arena(true);
    let target = troop(&m);
    let truck = *m.convoys.as_ref().unwrap().trucks.keys().next().unwrap();
    place(&mut m, truck, Point { x: 65., z: 81. });
    let stock = cargo(&mut m, truck);
    let destination = Point { x: 67., z: 77. };
    place(&mut m, target, destination);
    assert!(m.supply_convoys(0, &[truck], target).unwrap());
    assert_eq!(
        m.convoys.as_ref().unwrap().trucks[&truck].state,
        TruckState::Delivering
    );
    assert_eq!(
        m.convoys.as_ref().unwrap().trucks[&truck].destination,
        Some(destination)
    );
    let goal = m.world.game.units[&truck].world_goal.unwrap();
    assert!((math::hypot(goal.x - destination.x, goal.z - destination.z) - 9.).abs() < 1e-12);
    let job = m.convoys.as_ref().unwrap().trucks[&truck].target.clone();
    assert_eq!(
        m.execute_command(
            0,
            &Command::Stop {
                ids: vec![i64::from(truck)]
            },
            false
        )
        .unwrap(),
        None
    );
    let held = &m.convoys.as_ref().unwrap().trucks[&truck];
    assert_eq!(held.state, TruckState::Held);
    assert!(held.manual && held.hold);
    assert_eq!(held.cargo, stock);
    assert_eq!(held.target, job);
    assert!(m.resume_convoys(0, &[truck]).unwrap());
    let resumed = &m.convoys.as_ref().unwrap().trucks[&truck];
    assert_eq!(resumed.state, TruckState::Delivering);
    assert!(!resumed.manual && !resumed.hold);
    assert_eq!(resumed.cargo, stock);
    assert_eq!(resumed.target, job);
    let waypoint = Point { x: 65., z: 83. };
    assert_eq!(
        m.execute_command(0, &move_to(truck, waypoint, true), false)
            .unwrap(),
        None
    );
    let manual = &m.convoys.as_ref().unwrap().trucks[&truck];
    assert_eq!(manual.state, TruckState::Manual);
    assert!(manual.manual && !manual.hold);
    assert!(manual.target.is_none());
    assert_eq!(manual.cargo, stock);
    assert!(!m.commands.units[&truck].orders.is_empty());

    let home = Point { x: 61., z: 79. };
    let mut source = m
        .convoys
        .as_ref()
        .unwrap()
        .stores
        .iter()
        .find(|s| s.source && s.owner == 0)
        .unwrap()
        .clone();
    source.id = StoreId::Source(900);
    source.at = home;
    m.convoys.as_mut().unwrap().stores.push(source);
    // The selected convoy beyond the ordinary troop cap must still enter manual retreat.
    let mut ids = vec![i64::from(target); 50.max(m.world.game.units.len())];
    ids.push(i64::from(truck));
    assert_eq!(
        m.execute_command(0, &Command::Retreat { ids }, false)
            .unwrap(),
        None
    );
    assert_eq!(m.world.game.units[&truck].world_goal, Some(home));
    assert!(m.world.game.units[&truck].retreating);
    assert!(m.commands.units[&truck].orders.is_empty());
    let retreat = &m.convoys.as_ref().unwrap().trucks[&truck];
    assert_eq!(retreat.state, TruckState::Manual);
    assert!(retreat.manual && !retreat.hold && retreat.target.is_none());
    assert_eq!(retreat.cargo, stock);
    assert_guards(&mut m);
}

#[test]
fn intent_log39_match_refuses_missing_source_and_malformed_orders_atomically() {
    let mut m = arena(true);
    let truck = *m.convoys.as_ref().unwrap().trucks.keys().next().unwrap();
    cargo(&mut m, truck);
    let troop = troop(&m);
    m.commands.units.get_mut(&troop).unwrap().follow = 99;
    // No source can be reached, so the complete mixed selection remains untouched.
    m.convoys.as_mut().unwrap().stores.retain(|s| !s.source);
    let before = format!(
        "{:?}",
        (
            &m.world.game.units,
            &m.commands.units,
            &m.convoys,
            &m.routing_fog
        )
    );
    assert_eq!(
        m.execute_command(
            0,
            &Command::Retreat {
                ids: vec![i64::from(troop), i64::from(truck)]
            },
            false
        )
        .unwrap(),
        Some(Denial::Blocked)
    );
    assert_eq!(
        format!(
            "{:?}",
            (
                &m.world.game.units,
                &m.commands.units,
                &m.convoys,
                &m.routing_fog
            )
        ),
        before
    );
    for orders in [
        serde_json::json!({}),
        serde_json::json!([[truck]]),
        serde_json::json!([[truck, "x", 2]]),
        serde_json::json!([[truck, 2, null]]),
    ] {
        assert!(matches!(
            Command::from_json(&serde_json::json!({"t":"move","orders":orders})),
            Err(Denial::Blocked)
        ));
    }
    assert_guards(&mut m);
}

#[test]
fn intent_log39_match_unreachable_existing_source_refuses_before_route_publication() {
    let mut m = arena_with_barrier(true, true);
    let truck = *m.convoys.as_ref().unwrap().trucks.keys().next().unwrap();
    let troop = troop(&m);
    place(&mut m, truck, Point { x: 65., z: 81. });
    cargo(&mut m, truck);
    let sources = &mut m.convoys.as_mut().unwrap().stores;
    sources.retain(|s| s.source && s.owner == 0);
    assert_eq!(sources.len(), 1);
    sources[0].at = Point { x: 121., z: 81. };
    m.commands.units.get_mut(&troop).unwrap().follow = 99;
    let before = format!(
        "{:?}",
        (
            &m.world.game.units,
            &m.commands.units,
            &m.commands.requests,
            &m.convoys,
            &m.routing_fog,
            &m.recipients
        )
    );
    assert_eq!(
        m.execute_command(
            0,
            &Command::Retreat {
                ids: vec![i64::from(troop), i64::from(truck)]
            },
            false
        )
        .unwrap(),
        Some(Denial::Blocked)
    );
    assert_eq!(
        format!(
            "{:?}",
            (
                &m.world.game.units,
                &m.commands.units,
                &m.commands.requests,
                &m.convoys,
                &m.routing_fog,
                &m.recipients
            )
        ),
        before
    );
    assert_guards(&mut m);
}

#[test]
fn intent_log43_match_scheduler_owner_alerts_stay_below_full_snapshot_guard() {
    let mut m = arena(true);
    let id = troop(&m);
    let reserves = m.logistics.get_mut(id).unwrap();
    reserves.provisions = 29.;
    reserves.forced = true;
    reserves.stranded = true;
    m.world.game.tick = 40;
    m.step_supply_convoys(false).unwrap();
    let owned = m
        .with_supply_world(|ctx, state, reserves, world| {
            state.recipient_state(ctx, 0, reserves, world)
        })
        .unwrap();
    for kind in ["low", "withdrawal", "stranded"] {
        assert!(
            owned
                .events
                .iter()
                .any(|e| e.kind == kind && e.owner == 0 && e.count >= 1),
            "{kind}"
        );
    }
    let enemy = m
        .with_supply_world(|ctx, state, reserves, world| {
            state.recipient_state(ctx, 1, reserves, world)
        })
        .unwrap();
    assert!(enemy.events.iter().all(|e| e.owner == 1));
    let events = m.convoys.as_ref().unwrap().events.clone();
    m.step_supply_convoys(false).unwrap();
    assert_eq!(m.convoys.as_ref().unwrap().events, events);
    assert_guards(&mut m);
}

#[test]
fn intent_log39_ordinary_match_retreat_and_snapshots_survive_without_convoy_state() {
    let mut m = arena(false);
    let id = troop(&m);
    assert_eq!(
        m.execute_command(
            0,
            &Command::Retreat {
                ids: vec![i64::from(id)]
            },
            false
        )
        .unwrap(),
        None
    );
    let mut delivery = DeliveryState::new(&m).unwrap();
    let selected = (0..2)
        .map(|slot| SelectedSeat {
            slot,
            full: false,
            omit: false,
        })
        .collect::<Vec<_>>();
    for _ in 0..5 {
        let report = m.advance_tick().unwrap();
        assert!(m.tick_state.fault().is_none());
        let frame = delivery
            .capture_selected_with_tick_events(&mut m, &selected, &report.events)
            .unwrap()
            .unwrap();
        for owner in 0..2 {
            assert_eq!(frame.recipient(owner).unwrap().into_json()["t"], "s");
        }
    }
    assert!(m.convoys.is_none());
}
