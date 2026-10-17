use crate::match_firing::LaunchEvent;
use crate::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::Point,
    map::AuthoritativeMap,
    match_commands::{Command, MoveOrder},
    match_runtime::{Match, MatchOptions, PlayerConfig},
    recipient_snapshot::{DeliveryState, SelectedSeat},
    unit_publication::{PublicationParts, StagedUnit, prepare_publication},
    weather::WeatherChoice,
};
use serde_json::json;

fn scene(name: &str) -> (Match, u32, usize) {
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"Terrain fire intent", "w":64,"h":64,
            "rows":vec![".".repeat(64);64],
            "spawns":[{"x":5,"y":5},{"x":58,"y":58}],
            "points":[{"x":32,"y":32}]
        }))
        .unwrap(),
    )
    .unwrap();
    let mut m = Match::new(
        &map,
        MatchOptions {
            seed: 701,
            mode: Mode::Conquest,
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
            logistics: true,
            sandbox: false,
        },
    )
    .unwrap();
    let id = m.next_unit_id;
    let staged = StagedUnit::ordinary(
        &m.world.game,
        &m.rules,
        0,
        m.rules.kind(name).unwrap(),
        id,
        m.world.game.units.len() as u32,
        m.start.players[0].spawn,
        Some(Point { x: 51.0, z: 51.0 }),
    )
    .unwrap();
    prepare_publication(
        PublicationParts {
            units: &mut m.world.game.units,
            grid: &mut m.grid,
            controls: &mut m.commands.units,
            recruitment: &mut m.recruit_metadata,
            next_id: &mut m.next_unit_id,
        },
        &m.rules,
        2,
        vec![staged],
    )
    .unwrap()
    .publish_with(|u, _| {
        m.combat.initialize(u.id);
        m.firing.initialize(u.id);
        m.abilities.initialize(u.id);
        m.recovery.initialize(u.id);
    });
    for control in m.commands.units.values_mut() {
        control.hold_fire = true;
        control.auto_retreat = false;
        control.auto_cast = false;
    }
    let reserves =
        crate::match_logistics::initial_unit_reserves(true, &m.world.game.units[&id], &m.rules)
            .unwrap()
            .unwrap();
    m.logistics.insert(id, reserves);
    let cell = m.world.game.cell_of(81.0, 51.0).unwrap();
    m.world.set_cell(cell, b'H').unwrap();
    (m, id, cell)
}

fn ready(m: &mut Match, id: u32, cell: usize) {
    m.commands.units.get_mut(&id).unwrap().fire_at = Some(cell);
    m.world.game.units.get_mut(&id).unwrap().still = Some(10.0);
}

#[test]
fn log05_terrain_shells_and_salvos_launch_only_funded_whole_rounds() {
    for (kind, rounds) in [("tank", 1), ("rocket", 2)] {
        let (mut m, id, cell) = scene(kind);
        ready(&mut m, id, cell);
        m.logistics.get_mut(id).unwrap().ammo = if rounds == 1 { 1.75 } else { 2.75 };
        let random = m.start.random.snapshot();
        m.apply_unit_firing(id, false, 0.05).unwrap();
        assert_eq!(m.firing.flights.len(), rounds);
        assert_eq!(m.logistics.get(id).unwrap().ammo, 0.75);
        assert_eq!(m.world.game.units[&id].shot_at, Some(0.0));
        if kind == "tank" {
            assert_eq!(
                m.start.random.snapshot(),
                random,
                "aimed shells use no dispersion draws"
            );
            let p = m.firing.flights.values().next().unwrap();
            assert!(p.explosive && p.terrain > 0.0 && p.scar > 0.0);
            assert_eq!(p.intended, None);
            let w = crate::catalog::Catalog::compiled()
                .unwrap()
                .unit(kind)
                .unwrap()
                .get("w")
                .unwrap();
            let profile = crate::projectiles::projectile_profile(
                kind,
                false,
                w.get("veh").unwrap().number().unwrap(),
                true,
            )
            .unwrap();
            let end = crate::projectiles::launch_solution(
                p.launch_origin,
                crate::projectiles::Point3::new(81.0, 0.9, 51.0),
                profile.speed.unwrap(),
                profile.gravity,
                None,
            )
            .unwrap();
            assert_eq!(p.solution.velocity, end.velocity, "aim halfway up hedge");
        } else {
            assert_eq!(m.firing.salvos[0].left, 2);
        }
        m.firing.get_mut(id).unwrap().cooldown = 0.0;
        let first = m.firing.next_flight;
        let random = m.start.random.snapshot();
        m.apply_unit_firing(id, false, 0.05).unwrap();
        assert_eq!(
            m.firing.next_flight, first,
            "fractional reserve cannot launch"
        );
        assert_eq!(m.start.random.snapshot(), random);
        assert_eq!(m.logistics.get(id).unwrap().ammo, 0.75);
    }
}

#[test]
fn log08_terrain_reload_uses_shell_predebit_and_salvo_postdebit_shortage_without_stacking() {
    for kind in ["tank", "rocket"] {
        for (fraction, exhausted, suppression, multiplier) in [
            (1.0, false, 0.0, 1.0),
            (0.25, false, 0.0, if kind == "rocket" { 2.0 } else { 1.0 }),
            (0.24, false, 0.0, 2.0),
            (1.0, true, 0.0, 2.0),
            (0.24, true, 0.0, 2.0),
            (1.0, false, 50.0, if kind == "tank" { 1.5 } else { 1.0 }),
            (0.24, true, 90.0, if kind == "tank" { 6.0 } else { 2.0 }),
        ] {
            let (mut m, id, cell) = scene(kind);
            ready(&mut m, id, cell);
            let reserves = m.logistics.get_mut(id).unwrap();
            reserves.ammo = reserves.ammo_max * fraction;
            reserves.exhausted = exhausted;
            m.world.game.units.get_mut(&id).unwrap().supp = suppression;
            m.apply_unit_firing(id, false, 0.05).unwrap();
            assert!(!m.firing.flights.is_empty(), "{kind}");
            let interval = crate::catalog::Catalog::compiled()
                .unwrap()
                .unit(kind)
                .unwrap()
                .get("w")
                .unwrap()
                .get("interval")
                .unwrap()
                .number()
                .unwrap();
            assert_eq!(
                m.firing.get(id).unwrap().cooldown,
                interval * multiplier,
                "{kind} fraction={fraction} exhausted={exhausted} supp={suppression}"
            );
        }
    }
}

#[test]
fn log05_accepted_explosive_abilities_debit_launches_while_ap_and_suppress_only_gate_ammo() {
    for kind in ["rifle", "ranger", "mortar", "at", "mg"] {
        let (mut m, id, _) = scene(kind);
        let ammo = m.logistics.get(id).unwrap().ammo;
        m.start.economy.players[0].mun = 1000.0;
        assert_eq!(
            m.execute_command(
                0,
                &Command::Ability {
                    ids: vec![i64::from(id)],
                    at: Some(Point { x: 55.0, z: 51.0 }),
                    queue: false,
                    automatic: false,
                },
                false
            )
            .unwrap(),
            None,
            "{kind}"
        );
        m.step_aimed_ability(id).unwrap();
        match kind {
            "rifle" | "ranger" => {
                assert_eq!(
                    m.firing.flights.len(),
                    1,
                    "one explosive represents the squad throw"
                );
                assert_eq!(
                    m.logistics.get(id).unwrap().ammo,
                    ammo - if kind == "ranger" { 6.0 } else { 5.0 },
                    "{kind} pays one round per living model"
                );
            }
            "mortar" => {
                assert!(!m.firing.flights.is_empty());
                assert_eq!(
                    m.logistics.get(id).unwrap().ammo,
                    ammo - m.firing.flights.len() as f64
                );
            }
            "at" => {
                assert!(m.abilities.get(id).unwrap().ap);
                assert_eq!(m.logistics.get(id).unwrap().ammo, ammo);
                assert!(m.firing.flights.is_empty());
            }
            "mg" => {
                assert!(m.abilities.get(id).unwrap().buff > 0.0);
                assert_eq!(m.logistics.get(id).unwrap().ammo, ammo);
                assert!(m.firing.flights.is_empty());
            }
            _ => unreachable!(),
        }
    }
}

fn tick_and_deliver(
    m: &mut Match,
    delivery: &mut DeliveryState,
) -> Vec<crate::match_tick::TickEvent> {
    let report = m.advance_tick().unwrap();
    let selected = (0..2)
        .map(|slot| SelectedSeat {
            slot,
            full: true,
            omit: false,
        })
        .collect::<Vec<_>>();
    let frame = delivery
        .capture_selected_with_tick_events(m, &selected, &report.events)
        .unwrap()
        .unwrap();
    for slot in 0..2 {
        assert_eq!(frame.recipient(slot).unwrap().into_json()["t"], "s");
    }
    drop(frame);
    assert!(m.tick_state.fault().is_none());
    report.events
}

#[test]
fn mov03_aimed_ability_walks_into_range_before_throw_and_continues_every_seat_delivery() {
    for kind in ["rifle", "mortar"] {
        let (mut m, id, _) = scene(kind);
        m.start.economy.players[0].mun = 1000.0;
        let at = Point { x: 121.0, z: 61.0 };
        assert_eq!(
            m.execute_command(
                0,
                &Command::Ability {
                    ids: vec![i64::from(id)],
                    at: Some(at),
                    queue: false,
                    automatic: false,
                },
                false
            )
            .unwrap(),
            None
        );
        let mut delivery = DeliveryState::new(&m).unwrap();
        let mut walking = false;
        let mut launched = false;
        let mut after_launch = 0;
        for _ in 0..500 {
            let events = tick_and_deliver(&mut m, &mut delivery);
            walking |= m.commands.units[&id].nade.is_some()
                && m.world.game.units[&id].x > 51.0
                && !m.world.game.units[&id].path.is_empty();
            launched |= events.iter().any(|e| matches!(e,
                crate::match_tick::TickEvent::Firing(LaunchEvent::Flight { shooter, .. }) if *shooter == id));
            if launched {
                after_launch += 1;
                if after_launch >= 50 {
                    break;
                }
            }
        }
        assert!(walking && launched, "{kind} routes before the aimed launch");
        assert!(m.commands.units[&id].nade.is_none());
        // Territorial service may refill rounds later in the launch tick.
        // The LOG-05 cases check the debit at the actual launch boundary.
        assert!(m.firing.flights.is_empty());
    }
}

#[test]
fn mov03_terrain_shooter_retirement_keeps_its_flights_and_successor_ticks_deliverable() {
    for kind in ["tank", "rocket"] {
        let (mut m, id, cell) = scene(kind);
        let at = Point { x: 81.0, z: 51.0 };
        assert_eq!(
            m.execute_command(
                0,
                &Command::FireAt {
                    ids: vec![i64::from(id)],
                    at,
                    queue: false,
                },
                false
            )
            .unwrap(),
            None
        );
        m.world.game.units.get_mut(&id).unwrap().still = Some(10.0);
        let mut delivery = DeliveryState::new(&m).unwrap();
        tick_and_deliver(&mut m, &mut delivery);
        assert!(!m.firing.flights.is_empty());
        let hp = m.world.game.cell_hp[cell];
        m.world.game.units.get_mut(&id).unwrap().hp = 0.0;
        tick_and_deliver(&mut m, &mut delivery);
        assert!(!m.world.game.units.contains_key(&id));
        assert!(
            !m.firing.flights.is_empty(),
            "{kind} retains its dead shooter's launch"
        );
        for _ in 0..80 {
            tick_and_deliver(&mut m, &mut delivery);
        }
        assert!(m.firing.flights.is_empty());
        assert!(
            m.world.game.cell_hp[cell] < hp,
            "{kind} completes its stored launch"
        );
    }
}

#[test]
fn mov03_terrain_orders_move_stop_launch_damage_and_finish_with_every_seat() {
    for kind in ["tank", "rocket"] {
        let (mut m, id, cell) = scene(kind);
        m.start.points[0].owner = Some(0);
        m.start.points[0].progress = 1.0;
        m.start.points[0].vp = 1.0;
        m.start.win_vp = 20.0;
        let at = Point {
            x: (cell % m.world.game.w) as f64 * 2.0 + 1.0,
            z: (cell / m.world.game.w) as f64 * 2.0 + 1.0,
        };
        let initial_hp = m.world.game.cell_hp[cell];
        let start = m.world.game.units[&id].at();
        let mut delivery = DeliveryState::new(&m).unwrap();
        assert_eq!(
            m.execute_command(
                0,
                &Command::Move {
                    orders: vec![MoveOrder {
                        unit: i64::from(id),
                        at: Point { x: 55.0, z: 51.0 }
                    }],
                    control_ids: None,
                    attack_move: false,
                    queue: false,
                    together: false,
                    face: None
                },
                false
            )
            .unwrap(),
            None
        );
        assert_eq!(
            m.execute_command(
                0,
                &Command::FireAt {
                    ids: vec![i64::from(id)],
                    at,
                    queue: true
                },
                false
            )
            .unwrap(),
            None
        );
        // The waiting FireAt row must be deliverable before it starts.
        let seats = (0..2)
            .map(|slot| SelectedSeat {
                slot,
                full: true,
                omit: false,
            })
            .collect::<Vec<_>>();
        delivery
            .capture_selected_with_tick_events(&mut m, &seats, &[])
            .unwrap()
            .unwrap()
            .recipient(0)
            .unwrap();
        let mut launched = 0;
        let mut damaged = false;
        let mut finished = None;
        let mut restored = false;
        for _ in 0..650 {
            let events = tick_and_deliver(&mut m, &mut delivery);
            launched += events.iter().filter(|event| matches!(event, crate::match_tick::TickEvent::Firing(LaunchEvent::Flight { shooter, .. }) if *shooter == id)).count();
            damaged |= m.world.game.cell_hp[cell] < initial_hp;
            if !restored && !m.firing.flights.is_empty() {
                assert_ne!(m.world.game.units[&id].at(), start);
                assert!(m.world.game.units[&id].path.is_empty());
                let save = m.save_json().unwrap();
                m = Match::from_save_json(save).unwrap();
                delivery = DeliveryState::new(&m).unwrap();
                restored = true;
            }
            if m.finish.is_some() {
                let first = *finished.get_or_insert(m.world.game.tick);
                if m.world.game.tick >= first + 8 {
                    break;
                }
            }
        }
        assert!(
            launched > 0 && restored,
            "{kind} launches must survive delivery and save/load"
        );
        assert!(damaged, "{kind} shells must reach terrain");
        assert!(m.finish.is_some(), "{kind} match finishes");
        assert!(m.world.game.tick >= finished.unwrap() + 8);
    }
}
