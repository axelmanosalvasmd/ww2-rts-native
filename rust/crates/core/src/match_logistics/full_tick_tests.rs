//! Ordinary runner proof for the connected reserve, network and convoy phases.
use super::runtime_tests::{arena, arena_with_barrier, place, troop};
use crate::{
    economy::Mode,
    game::Point,
    match_commands::{Command, MoveOrder},
    match_tick::TickPhase,
    recipient_snapshot::{DeliveryState, SelectedSeat},
};

fn quiet(m: &mut crate::match_runtime::Match) {
    for u in m.world.game.units.values_mut() {
        u.hold_pos = true;
    }
    for c in m.commands.units.values_mut() {
        c.auto_cast = false;
        c.auto_retreat = false;
        c.hold_fire = true;
    }
}
fn selected(m: &crate::match_runtime::Match) -> Vec<SelectedSeat> {
    (0..m.start.players.len())
        .map(|slot| SelectedSeat {
            slot,
            full: false,
            omit: false,
        })
        .collect()
}

fn ready_motorpool(m: &mut crate::match_runtime::Match) {
    let worker = troop(m);
    place(m, worker, Point { x: 57., z: 61. });
    m.start.economy.players[0].mp = 5000.;
    let site = m.next_unit_id;
    assert_eq!(
        m.execute_command(
            0,
            &Command::Build {
                ids: vec![i64::from(worker)],
                kind: "motorpool".into(),
                at: Some(Point { x: 63., z: 61. }),
                direction: None,
                queue: false,
            },
            false
        )
        .unwrap(),
        None
    );
    // The initial scene has a completed producer; placement and structural
    // ownership still come from the ordinary canonical construction actuator.
    m.world.game.units.get_mut(&site).unwrap().built = Some(1.);
    m.repair_building_sections(site, 1900.).unwrap();
}

#[test]
fn ordinary_logistics_match_finishes_with_cache_depot_convoys_fuel_and_every_seat_each_tick() {
    for mode in [Mode::Default, Mode::Conquest] {
        let mut m = arena_with_barrier(true, true);
        m.options.mode = mode;
        ready_motorpool(&mut m);
        let vehicle = m.recorded_buy(0, "armoredcar", None).unwrap().unit;
        let truck = m.test_supply_truck(0);
        let worker = troop(&m);
        quiet(&mut m);
        place(&mut m, worker, Point { x: 57., z: 81. });
        // The river isolates this vehicle from territorial replenishment, so
        // powered travel must consume its carried fuel. Convoys stay on shore.
        place(&mut m, vehicle, Point { x: 100., z: 50. });
        m.world.game.units.get_mut(&vehicle).unwrap().hold_pos = false;
        m.start.points[0].owner = Some(0);
        m.start.points[0].progress = 1.;
        m.start.points[0].kind = Some("depot".into());
        m.start.points[0].vp = 1.;
        m.start.win_vp = 30.;
        let cache = m.next_unit_id;
        assert_eq!(
            m.execute_command(
                0,
                &Command::Build {
                    ids: vec![i64::from(worker)],
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
        assert_eq!(
            m.execute_command(
                0,
                &Command::Move {
                    orders: vec![MoveOrder {
                        unit: i64::from(vehicle),
                        at: Point { x: 125., z: 50. }
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
        let initial_fuel = m.logistics.get(vehicle).unwrap().fuel.unwrap();
        m.logistics.get_mut(worker).unwrap().ammo = 1.;
        m.logistics.get_mut(worker).unwrap().provisions = 20.;
        let cargo = crate::logistics::Stock {
            ammo: 3.,
            provisions: 120.,
            fuel: 180.,
        };
        m.convoys
            .as_mut()
            .unwrap()
            .trucks
            .get_mut(&truck)
            .unwrap()
            .cargo = cargo;
        assert!(m.supply_convoys(0, &[truck], worker).unwrap());
        let mut delivery = DeliveryState::new(&m).unwrap();
        let recipients = selected(&m);
        let mut saw_fuel_use = false;
        let mut saw_ammo_service = false;
        let mut saw_cargo_use = false;
        let mut saw_own_truck = false;
        let mut saw_network = false;
        let mut saw_cache = false;
        let mut saw_depot = false;
        let mut finish_tick = None;
        let mut checkpoint_restored = false;
        for _ in 0..750 {
            let report = m.advance_tick().unwrap();
            assert!(report.phases.contains(&TickPhase::UnitReserves));
            assert!(report.phases.contains(&TickPhase::Convoys));
            assert!(!report.unavailable.contains(&TickPhase::UnitReserves));
            assert!(!report.unavailable.contains(&TickPhase::SupplyNetwork));
            assert!(!report.unavailable.contains(&TickPhase::Convoys));
            saw_network |= report.phases.contains(&TickPhase::SupplyNetwork);
            saw_fuel_use |= m.logistics.get(vehicle).unwrap().fuel.unwrap() < initial_fuel;
            saw_ammo_service |= m.logistics.get(worker).unwrap().ammo > 1.;
            saw_cargo_use |= m
                .convoys
                .as_ref()
                .unwrap()
                .trucks
                .get(&truck)
                .is_some_and(|c| {
                    c.cargo.ammo < cargo.ammo || c.cargo.provisions < cargo.provisions
                });
            saw_cache |= m.world.game.units[&cache].built == Some(1.);
            saw_depot |= m
                .convoys
                .as_ref()
                .unwrap()
                .stores
                .iter()
                .any(|s| s.point == Some(0));
            let frame = delivery
                .capture_selected_with_tick_events(&mut m, &recipients, &report.events)
                .unwrap()
                .unwrap();
            for slot in 0..2 {
                let snapshot = frame.recipient(slot).unwrap().into_json();
                let logistics = &snapshot["logistics"];
                assert_eq!(logistics["enabled"], true);
                for row in logistics["units"].as_array().unwrap() {
                    let id = row["id"].as_u64().unwrap();
                    assert!(
                        snapshot["units"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .any(|u| u[0] == id && u[2] == slot)
                    );
                }
                if slot == 0 {
                    saw_own_truck |= logistics["trucks"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|r| r["id"] == truck);
                }
                if slot == 1 {
                    assert!(
                        !logistics["trucks"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .any(|r| r["id"] == truck)
                    );
                }
            }
            drop(frame);
            assert!(m.tick_state.fault().is_none());
            if m.world.game.tick == 40 {
                // Save while the cache is unfinished and the selected truck is active.
                assert!(m.world.game.units[&cache].built.unwrap() < 1.);
                assert!(m.convoys.as_ref().unwrap().trucks.contains_key(&truck));
                let checkpoint = m.save_json().unwrap();
                let restored = crate::match_runtime::Match::from_save_json(checkpoint).unwrap();
                assert_eq!(restored.logistics, m.logistics);
                assert_eq!(restored.convoys, m.convoys);
                assert_eq!(restored.supply_net, m.supply_net);
                m = restored;
                delivery = DeliveryState::new(&m).unwrap();
                checkpoint_restored = true;
            }
            if m.finish.is_some() {
                let start = *finish_tick.get_or_insert(m.world.game.tick);
                if m.world.game.tick >= start + 8 {
                    break;
                }
            }
        }
        assert!(
            checkpoint_restored,
            "active logistics checkpoint was never restored"
        );
        assert!(saw_network, "network refresh was never observed");
        assert!(saw_fuel_use, "powered fuel use was never observed");
        assert!(saw_cache, "cache completion was never observed");
        assert!(saw_depot, "depot service was never observed");
        assert!(saw_ammo_service && saw_cargo_use && saw_own_truck);
        assert_eq!(m.finish.as_ref().unwrap().winner, 0);
        assert!(m.world.game.tick >= finish_tick.unwrap() + 8);
    }
}

#[test]
fn reserve_exhaustion_cancels_live_construction_then_recovers_through_tick_and_delivery() {
    let mut m = arena(true);
    quiet(&mut m);
    let worker = troop(&m);
    place(&mut m, worker, Point { x: 57., z: 81. });
    m.start.economy.players[0].mp = 500.;
    let cache = m.next_unit_id;
    assert_eq!(
        m.execute_command(
            0,
            &Command::Build {
                ids: vec![i64::from(worker)],
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
    assert!(m.world.game.units[&worker].build);
    let r = m.logistics.get_mut(worker).unwrap();
    r.provisions = 0.;
    r.exhausted = true;
    r.warning = Some(0.);
    let mut delivery = DeliveryState::new(&m).unwrap();
    let recipients = selected(&m);
    let mut saw_forced = false;
    let mut recovered = false;
    for _ in 0..600 {
        let report = m.advance_tick().unwrap();
        let r = m.logistics.get(worker).unwrap();
        saw_forced |= r.forced;
        recovered |= saw_forced && !r.forced && r.provisions >= 60.;
        assert!(!m.world.game.units[&worker].build);
        assert_eq!(m.commands.units[&worker].build_target, None);
        let frame = delivery
            .capture_selected_with_tick_events(&mut m, &recipients, &report.events)
            .unwrap()
            .unwrap();
        for slot in 0..2 {
            frame.recipient(slot).unwrap();
        }
        assert!(m.tick_state.fault().is_none());
        if recovered {
            break;
        }
    }
    assert!(saw_forced && recovered);
    assert!(m.world.game.units[&cache].built.unwrap() < 1.);
}

#[test]
fn killed_convoy_is_notified_once_and_removed_from_recipient_delivery() {
    let mut m = arena(true);
    quiet(&mut m);
    let truck = m.test_supply_truck(0);
    m.world.game.units.get_mut(&truck).unwrap().hp = 0.;
    let mut delivery = DeliveryState::new(&m).unwrap();
    let recipients = selected(&m);
    for _ in 0..3 {
        let report = m.advance_tick().unwrap();
        assert!(!m.world.game.units.contains_key(&truck));
        assert!(!m.convoys.as_ref().unwrap().trucks.contains_key(&truck));
        assert_eq!(m.convoys.as_ref().unwrap().losses[&0], vec![30.05]);
        let frame = delivery
            .capture_selected_with_tick_events(&mut m, &recipients, &report.events)
            .unwrap()
            .unwrap();
        for slot in 0..2 {
            let view = frame.recipient(slot).unwrap().into_json();
            assert!(
                !view["logistics"]["trucks"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|r| r["id"] == truck)
            );
        }
    }
}

#[test]
fn queued_nontruck_orders_survive_logistics_preflight_ticks_and_recipient_delivery() {
    for kind in ["attack", "enter", "dig"] {
        let mut m = arena(true);
        quiet(&mut m);
        let id = troop(&m);
        place(&mut m, id, Point { x: 57., z: 81. });
        let at = Point { x: 65., z: 81. };
        let target = m
            .world
            .game
            .units
            .values()
            .find(|u| u.owner == 1)
            .unwrap()
            .id;
        m.world.game.players[0].visible.push(target);
        m.world.game.players[0].visible.sort_unstable();
        m.world.game.players[0].visible.dedup();
        let command = match kind {
            "attack" => Command::Attack {
                ids: vec![i64::from(id)],
                target: i64::from(target),
                queue: true,
                at: None,
            },
            "enter" => {
                let cell = m.world.game.cell_of(at.x, at.z).unwrap();
                m.world.set_cell(cell, b'B').unwrap();
                Command::Enter {
                    ids: vec![i64::from(id)],
                    at,
                    queue: true,
                }
            }
            _ => Command::Dig {
                ids: vec![i64::from(id)],
                kind: crate::field_works::FortKind::Trench,
                at,
                direction: Some(0.),
                queue: true,
            },
        };
        assert_eq!(
            m.execute_command(0, &command, false).unwrap(),
            None,
            "{kind}"
        );
        // Keep the accepted order waiting while all supply phases read the unit.
        m.world.game.units.get_mut(&id).unwrap().path = vec![Point { x: 95., z: 95. }];
        let mut delivery = DeliveryState::new(&m).unwrap();
        let recipients = selected(&m);
        for _ in 0..3 {
            let report = m.advance_tick().unwrap();
            assert!(!m.commands.units[&id].orders.is_empty(), "{kind}");
            let frame = delivery
                .capture_selected_with_tick_events(&mut m, &recipients, &report.events)
                .unwrap()
                .unwrap();
            for slot in 0..2 {
                frame.recipient(slot).unwrap();
            }
            assert!(m.tick_state.fault().is_none());
        }
    }
}

#[test]
fn automatic_convoys_keep_supply_routes_while_manual_trucks_use_ordinary_routes() {
    for manual in [false, true] {
        let mut m = arena(true);
        let id = m.test_supply_truck(0);
        quiet(&mut m);
        place(&mut m, id, Point { x: 50., z: 50. });
        m.convoys
            .as_mut()
            .unwrap()
            .trucks
            .get_mut(&id)
            .unwrap()
            .manual = manual;
        let goal = Point { x: 70., z: 50. };
        m.world.game.units.get_mut(&id).unwrap().world_goal = Some(goal);
        m.begin_path_tick(4096, 20).unwrap();
        m.prepare_unit_commands(id, false).unwrap();
        assert_eq!(!m.world.game.units[&id].path.is_empty(), manual);
        m.commands.units.get_mut(&id).unwrap().route_end = None;
        let u = m.world.game.units.get_mut(&id).unwrap();
        u.path = vec![goal];
        u.stuck = 2.;
        u.repath = 0.;
        m.move_unit_with_command_routes(id, 1.).unwrap();
        assert_eq!(m.commands.units[&id].route_end.is_some(), manual);
    }
    // A low-health ordinary vehicle pulls back from fresh fire; a convoy does not.
    for convoy in [false, true] {
        let mut m = arena(true);
        let id = if convoy {
            m.test_supply_truck(0)
        } else {
            ready_motorpool(&mut m);
            m.recorded_buy(0, "armoredcar", None).unwrap().unit
        };
        place(&mut m, id, Point { x: 60., z: 70. });
        let u = m.world.game.units.get_mut(&id).unwrap();
        u.hp = 1.;
        u.hold_pos = false;
        u.path.clear();
        u.world_goal = None;
        u.hit_from = Some(Point { x: 70., z: 70. });
        m.commands.units.get_mut(&id).unwrap().hold_fire = true;
        m.combat.get_mut(id).unwrap().hit_at = 0.;
        m.react_unit(id).unwrap();
        assert_eq!(!m.world.game.units[&id].path.is_empty(), !convoy);
    }
}
