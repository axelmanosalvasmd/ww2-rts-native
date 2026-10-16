//! Convoy room input through recorded orders, ordinary ticks and recipient delivery.
use super::runtime_tests::{arena, place, troop};
use crate::{
    game::Point,
    logistics::Stock,
    match_commands::{Command, Denial},
    match_runtime::Match,
    match_tick::TickReport,
    orders::OrderKey,
    recipient_snapshot::{DeliveryState, SelectedSeat},
    supply_convoys::TruckState,
};
use serde_json::{Value, json};

fn quiet(m: &mut Match) {
    for unit in m.world.game.units.values_mut() {
        unit.hold_pos = true;
    }
    for control in m.commands.units.values_mut() {
        control.auto_cast = false;
        control.auto_retreat = false;
        control.hold_fire = true;
    }
}

fn enqueue(m: &mut Match, wire: Value) {
    m.enqueue_command(
        OrderKey {
            tick: m.world.game.tick as u64 + 1,
            issuer: 0,
            sequence: m.world.game.tick as u64 + 1,
        },
        Command::from_json(&wire).unwrap(),
    )
    .unwrap();
}

fn delivered_tick(m: &mut Match, delivery: &mut DeliveryState) -> (TickReport, Vec<Value>) {
    let report = m.advance_tick().unwrap();
    let recipients = (0..m.start.players.len())
        .map(|slot| SelectedSeat {
            slot,
            full: false,
            omit: false,
        })
        .collect::<Vec<_>>();
    let frame = delivery
        .capture_selected_with_tick_events(m, &recipients, &report.events)
        .unwrap()
        .unwrap();
    let snapshots = recipients
        .iter()
        .map(|seat| frame.recipient(seat.slot).unwrap().into_json())
        .collect();
    drop(frame);
    assert!(m.tick_state.fault().is_none());
    (report, snapshots)
}

#[test]
fn convoy_room_shapes_preserve_selection_order_duplicates_and_uncapped_orders_fallback() {
    let ids = (1..=400).chain([1, 400]).collect::<Vec<i64>>();
    let rows = ids.iter().map(|id| json!([id, 30, 40])).collect::<Vec<_>>();
    assert_eq!(
        Command::from_json(&json!({"t":"supply","orders":rows,"target":17})).unwrap(),
        Command::Supply {
            ids: ids.clone(),
            target: 17,
        }
    );
    assert_eq!(
        Command::from_json(&json!({"t":"logisticsResume","ids":ids})).unwrap(),
        Command::LogisticsResume { ids }
    );
    assert_eq!(
        Command::from_json(&json!({"t":"supply","ids":[],"orders":[[1,30,40]],"target":17}))
            .unwrap(),
        Command::Supply {
            ids: vec![],
            target: 17,
        }
    );
    assert_eq!(
        Command::from_json(&json!({"t":"logisticsResume","orders":[[7,30,40],[7,40,50]]})).unwrap(),
        Command::LogisticsResume { ids: vec![7, 7] }
    );
    assert_eq!(
        Command::from_json(&json!({"t":"supply","ids":[1],"target":"17"})).unwrap(),
        Command::Supply {
            ids: vec![1],
            target: -1,
        }
    );
}

#[test]
fn convoy_room_supply_stop_resume_survives_pending_save_and_finishes_with_every_seat() {
    let mut m = arena(true);
    let target = troop(&m);
    let truck = m.test_supply_truck(0);
    quiet(&mut m);
    place(&mut m, target, Point { x: 105., z: 81. });
    place(&mut m, truck, Point { x: 65., z: 81. });
    m.world.game.units.get_mut(&truck).unwrap().hold_pos = false;
    m.logistics.get_mut(target).unwrap().ammo = 1.;
    m.logistics.get_mut(target).unwrap().provisions = 20.;
    let cargo = Stock {
        ammo: 12.,
        provisions: 120.,
        fuel: 0.,
    };
    m.convoys
        .as_mut()
        .unwrap()
        .trucks
        .get_mut(&truck)
        .unwrap()
        .cargo = cargo;
    m.start.points[0].owner = Some(0);
    m.start.points[0].progress = 1.;
    m.start.points[0].vp = 1.;
    m.start.win_vp = 30.;
    enqueue(&mut m, json!({"t":"supply","ids":[truck],"target":target}));
    let mut restored = Match::from_save_json(m.save_json().unwrap()).unwrap();
    let mut delivery = DeliveryState::new(&m).unwrap();
    let mut restored_delivery = DeliveryState::new(&restored).unwrap();
    let (report, snapshots) = delivered_tick(&mut m, &mut delivery);
    let (restored_report, restored_snapshots) =
        delivered_tick(&mut restored, &mut restored_delivery);
    assert_eq!(report.commands, restored_report.commands);
    assert_eq!(snapshots, restored_snapshots);
    assert_eq!(m.convoys, restored.convoys);
    assert_eq!(report.commands.len(), 1);
    assert_eq!(report.commands[0].denial, None);
    assert!(m.convoys.as_ref().unwrap().trucks[&truck].manual);
    assert_eq!(
        m.convoys.as_ref().unwrap().trucks[&truck].state,
        TruckState::Delivering
    );

    enqueue(&mut m, json!({"t":"stop","ids":[truck]}));
    let (report, _) = delivered_tick(&mut m, &mut delivery);
    assert_eq!(report.commands[0].denial, None);
    assert!(m.convoys.as_ref().unwrap().trucks[&truck].hold);
    assert_eq!(
        m.convoys.as_ref().unwrap().trucks[&truck].state,
        TruckState::Held
    );
    let held_position = m.world.game.units[&truck].at();
    let held_cargo = m.convoys.as_ref().unwrap().trucks[&truck].cargo;
    for _ in 0..4 {
        delivered_tick(&mut m, &mut delivery);
        assert_eq!(m.world.game.units[&truck].at(), held_position);
        assert_eq!(m.convoys.as_ref().unwrap().trucks[&truck].cargo, held_cargo);
    }
    enqueue(
        &mut m,
        json!({"t":"logisticsResume","orders":[[truck,65,81]]}),
    );
    restored = Match::from_save_json(m.save_json().unwrap()).unwrap();
    // Both adapters begin from the same saved recipient baseline.
    delivery = DeliveryState::new(&m).unwrap();
    restored_delivery = DeliveryState::new(&restored).unwrap();
    for _ in 0..3 {
        let (report, snapshots) = delivered_tick(&mut m, &mut delivery);
        let (restored_report, restored_snapshots) =
            delivered_tick(&mut restored, &mut restored_delivery);
        assert_eq!(report.commands, restored_report.commands);
        assert_eq!(snapshots, restored_snapshots);
        assert_eq!(m.convoys, restored.convoys);
        assert_eq!(m.logistics, restored.logistics);
    }
    assert!(!m.convoys.as_ref().unwrap().trucks[&truck].hold);
    assert!(!m.convoys.as_ref().unwrap().trucks[&truck].manual);
    let mut served = false;
    let mut finish_tick = None;
    for _ in 0..750 {
        let (_, snapshots) = delivered_tick(&mut m, &mut delivery);
        let state = m.convoys.as_ref().unwrap();
        served |= state
            .trucks
            .get(&truck)
            .is_some_and(|c| c.cargo.ammo < cargo.ammo)
            && m.logistics.get(target).unwrap().ammo > 1.;
        assert!(
            snapshots[0]["logistics"]["trucks"]
                .as_array()
                .unwrap()
                .iter()
                .all(|row| {
                    let id = row["id"].as_u64().unwrap() as u32;
                    m.world.game.units[&id].owner == 0
                })
        );
        assert!(
            snapshots[1]["logistics"]["trucks"]
                .as_array()
                .unwrap()
                .iter()
                .all(|row| row["id"] != truck)
        );
        if m.finish.is_some() {
            let first = *finish_tick.get_or_insert(m.world.game.tick);
            if m.world.game.tick >= first + 8 {
                break;
            }
        }
    }
    assert!(
        served,
        "loaded truck must spend cargo and replenish the requested troop"
    );
    assert_eq!(m.finish.as_ref().unwrap().winner, 0);
    assert!(m.world.game.tick >= finish_tick.unwrap() + 8);
}

#[test]
fn convoy_room_denials_distinguish_selection_target_disabled_and_unassignable_job() {
    for reason in [
        "no trucks",
        "enemy truck",
        "missing target",
        "enemy target",
        "incomplete cache",
        "disabled",
        "no source",
        "resume no trucks",
    ] {
        let mut m = arena(reason != "disabled");
        let target = troop(&m);
        let truck = if reason == "disabled" {
            target
        } else {
            m.test_supply_truck(0)
        };
        quiet(&mut m);
        let enemy = m
            .world
            .game
            .units
            .values()
            .find(|u| u.owner == 1)
            .unwrap()
            .id;
        let mut selected = truck;
        let mut destination = i64::from(target);
        let mut kind = "supply";
        match reason {
            "no trucks" => selected = target,
            "resume no trucks" => {
                selected = target;
                kind = "logisticsResume";
            }
            "enemy truck" => selected = m.test_supply_truck(1),
            "missing target" => destination = -1,
            "enemy target" => destination = i64::from(enemy),
            "incomplete cache" => {
                place(&mut m, target, Point { x: 57., z: 81. });
                m.start.economy.players[0].mp = 5000.;
                destination = i64::from(m.next_unit_id);
                assert_eq!(
                    m.execute_command(
                        0,
                        &Command::Build {
                            ids: vec![i64::from(target)],
                            kind: "supplycache".into(),
                            at: Some(Point { x: 63., z: 81. }),
                            direction: None,
                            queue: false,
                        },
                        false
                    )
                    .unwrap(),
                    None
                );
            }
            "no source" => {
                // This empty truck has no collection origin at command time.
                // The later convoy phase may rebuild stores from ordinary sources.
                m.logistics.get_mut(target).unwrap().ammo = 1.;
                let convoys = m.convoys.as_mut().unwrap();
                assert_eq!(convoys.trucks[&truck].cargo, Stock::default());
                convoys.stores.clear();
            }
            _ => {}
        }
        enqueue(
            &mut m,
            json!({"t":kind,"ids":[selected],"target":destination}),
        );
        let mut delivery = DeliveryState::new(&m).unwrap();
        let (report, _) = delivered_tick(&mut m, &mut delivery);
        assert_eq!(report.commands.len(), 1, "{reason}");
        let expected = if matches!(reason, "disabled" | "no source") {
            Denial::Blocked
        } else {
            Denial::Needs
        };
        assert_eq!(report.commands[0].denial, Some(expected), "{reason}");
    }
}

#[test]
fn cmb150_sandbox_spawn_and_convoy_commands_share_ordinary_ticks_and_recipient_delivery() {
    let mut m = arena(true);
    m.options.sandbox = true;
    quiet(&mut m);
    let target = m.next_unit_id;
    enqueue(&mut m, json!({"t":"spawn","unit":"rifle","x":105,"z":81}));
    let mut delivery = DeliveryState::new(&m).unwrap();
    let (report, _) = delivered_tick(&mut m, &mut delivery);
    assert_eq!(report.commands[0].denial, None);
    assert_eq!(m.rules.kinds[m.world.game.units[&target].kind].name, "rifle");
    assert!(m.logistics.get(target).is_some());
    let truck = m.test_supply_truck(0);
    quiet(&mut m);
    place(&mut m, truck, Point { x: 95., z: 81. });
    m.world.game.units.get_mut(&truck).unwrap().hold_pos = false;
    m.logistics.get_mut(target).unwrap().ammo = 1.;
    m.logistics.get_mut(target).unwrap().provisions = 20.;
    m.convoys.as_mut().unwrap().trucks.get_mut(&truck).unwrap().cargo = Stock {
        ammo: 12., provisions: 120., fuel: 0.,
    };
    enqueue(&mut m, json!({"t":"supply","ids":[truck],"target":target}));
    let (report, _) = delivered_tick(&mut m, &mut delivery);
    assert_eq!(report.commands[0].denial, None);
    assert!(m.convoys.as_ref().unwrap().trucks[&truck].manual);
    enqueue(&mut m, json!({"t":"stop","ids":[truck]}));
    assert_eq!(delivered_tick(&mut m, &mut delivery).0.commands[0].denial, None);
    assert!(m.convoys.as_ref().unwrap().trucks[&truck].hold);
    enqueue(&mut m, json!({"t":"logisticsResume","ids":[truck]}));
    assert_eq!(delivered_tick(&mut m, &mut delivery).0.commands[0].denial, None);
    assert!(!m.convoys.as_ref().unwrap().trucks[&truck].manual);
    assert!(!m.convoys.as_ref().unwrap().trucks[&truck].hold);
    for _ in 0..80 {
        delivered_tick(&mut m, &mut delivery);
    }
    assert!(m.logistics.get(target).unwrap().ammo > 1.);
    assert!(m.convoys.as_ref().unwrap().trucks[&truck].cargo.ammo < 12.);
    for command in ["recover", "unknown"] {
        enqueue(&mut m, json!({"t":command}));
        let (report, _) = delivered_tick(&mut m, &mut delivery);
        assert_eq!(report.commands[0].denial, Some(Denial::Blocked));
    }
}
