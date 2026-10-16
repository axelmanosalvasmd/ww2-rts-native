//! Bounded CMB-158 dependencies over ordinary ticks, not full-row phase proof.
use crate::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::Point,
    grid::UnitId,
    map::AuthoritativeMap,
    match_commands::Command,
    match_firing::{AbilityBlast, LaunchEvent},
    match_runtime::{Match, MatchOptions, PlayerConfig},
    match_tick::{TickEvent, TickReport},
    objectives::ObjectiveRules,
    recipient_snapshot::{DeliveryState, SelectedSeat},
    weather::WeatherChoice,
};
use serde_json::json;

fn scene() -> (Match, UnitId, UnitId) {
    let map = AuthoritativeMap::from_value(json!({
        "name":"Flight phase dependencies", "w":80,"h":80,
        "rows":vec![".".repeat(80);80],
        "spawns":[{"x":5,"y":5},{"x":74,"y":74}],
        "points":[{"x":40,"y":40}]
    }))
    .unwrap();
    let mut m = Match::new(
        &map,
        MatchOptions {
            seed: 158,
            mode: Mode::Default,
            players: (0..2)
                .map(|slot| PlayerConfig {
                    name: format!("P{slot}"),
                    team: slot,
                    faction: Faction::Usa,
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
    let rifles = (0..2)
        .map(|owner| {
            m.world
                .game
                .units
                .values()
                .find(|u| u.owner == owner && m.rules.kinds[u.kind].name == "rifle")
                .unwrap()
                .id
        })
        .collect::<Vec<_>>();
    for control in m.commands.units.values_mut() {
        control.hold_fire = true;
        control.auto_cast = false;
        control.auto_retreat = false;
    }
    (m, rifles[0], rifles[1])
}

fn relocate(m: &mut Match, id: UnitId, at: Point) {
    let u = m.world.game.units.get_mut(&id).unwrap();
    u.x = at.x;
    u.z = at.z;
    u.path.clear();
    u.hold_pos = true;
    m.grid.update(id, u);
}

fn advance(m: &mut Match, delivery: &mut DeliveryState) -> TickReport {
    let report = m.advance_tick().unwrap();
    assert!(m.tick_state.fault().is_none());
    let seats = (0..m.world.game.players.len())
        .map(|slot| SelectedSeat {
            slot,
            full: false,
            omit: false,
        })
        .collect::<Vec<_>>();
    let frame = delivery
        .capture_selected_with_tick_events(m, &seats, &report.events)
        .unwrap()
        .unwrap();
    for seat in seats {
        assert_eq!(frame.recipient(seat.slot).unwrap().into_json()["t"], "s");
    }
    report
}

#[test]
fn cmb158_grenade_born_in_unit_turn_waits_for_travel_and_blasts_once() {
    // Prototype shared/sim.js:5416-5432 launches during the unit turn;
    // :5614-5623 advances flights before pruning their grenade tickets.
    let (mut m, thrower, _) = scene();
    let at = m.start.points[0].at;
    relocate(
        &mut m,
        thrower,
        Point {
            x: at.x - 10.0,
            z: at.z,
        },
    );
    let command = Command::Ability {
        ids: vec![i64::from(thrower)],
        at: Some(at),
        queue: false,
        automatic: false,
    };
    assert_eq!(m.execute_command(0, &command, false).unwrap(), None);
    let mut delivery = DeliveryState::new(&m).unwrap();
    let birth = advance(&mut m, &mut delivery);
    let id = birth
        .events
        .iter()
        .find_map(|event| match event {
            TickEvent::Firing(LaunchEvent::Flight { id, shooter, .. }) if *shooter == thrower => {
                Some(*id)
            }
            _ => None,
        })
        .expect("the unit turn must launch the admitted ability");
    let flight = &m.firing.flights[&id];
    assert_eq!(flight.last, flight.launched);
    assert_eq!(flight.launched, birth.tick as f64 * m.rules.movement.tick);
    assert!(birth.events.iter().all(|event| !matches!(
        event,
        TickEvent::Firing(LaunchEvent::Contact(contact)) if contact.flight == id
    )));
    assert!(m.firing.grenades.iter().any(|ticket| ticket.flight == id));

    advance(&mut m, &mut delivery);
    assert!(m.firing.flights[&id].last > m.firing.flights[&id].launched);
    let mut blasts = 0;
    for _ in 0..40 {
        let report = advance(&mut m, &mut delivery);
        blasts += report
            .events
            .iter()
            .filter(|event| {
                matches!(
                    event,
                    TickEvent::Firing(LaunchEvent::Contact(contact))
                        if contact.flight == id && contact.explosive && contact.terminal
                )
            })
            .count();
    }
    assert_eq!(blasts, 1, "the linked timer must not cause a second blast");
    assert!(!m.firing.flights.contains_key(&id));
    assert!(m.firing.grenades.iter().all(|ticket| ticket.flight != id));
}

#[test]
fn cmb158_expiring_flight_retires_the_capper_before_capture_and_income() {
    // Prototype shared/sim.js:5614,5672-5730 resolves flights before
    // retirement and capture. A surviving control captures on this same tick.
    for live_grenade in [false, true] {
        let (mut m, thrower, capper) = scene();
        let at = m.start.points[0].at;
        relocate(&mut m, capper, at);
        relocate(
            &mut m,
            thrower,
            Point {
                x: at.x - 14.0,
                z: at.z,
            },
        );
        m.world.game.units.get_mut(&capper).unwrap().hp = 1.0;
        let dt = m.rules.movement.tick;
        let point = &mut m.start.points[0];
        point.capper = Some(1);
        point.progress = 1.0 - dt / ObjectiveRules::compiled().unwrap().capture_time / 2.0;
        let progress = point.progress;
        let vp = m.start.economy.players[1].vp;
        let grenade = if live_grenade {
            let blast =
                AbilityBlast::from_unit(m.world.game.units[&thrower].kind, &m.rules).unwrap();
            let id = m
                .launch_ability_flight(thrower, at, blast, Some(dt), None)
                .unwrap();
            // This scene starts with a previously launched, detached producer output.
            drop(m.firing.take_events());
            Some(id)
        } else {
            None
        };
        let mut delivery = DeliveryState::new(&m).unwrap();
        let report = advance(&mut m, &mut delivery);
        if let Some(grenade) = grenade {
            assert!(report.events.iter().any(|event| matches!(
                event,
                TickEvent::Firing(LaunchEvent::Contact(contact))
                    if contact.flight == grenade && contact.explosive && contact.terminal
            )));
            assert!(report.retired.iter().any(|unit| unit.unit.id == capper));
            assert!(!m.world.game.units.contains_key(&capper));
            assert_eq!(m.start.points[0].owner, None);
            assert_eq!(m.start.points[0].progress, progress);
            assert!(!m.start.points[0].on_point.contains(&capper));
            assert_eq!(m.start.economy.players[1].vp, vp);
            assert!(m.firing.grenades.is_empty());
        } else {
            assert!(m.world.game.units.contains_key(&capper));
            assert_eq!(m.start.points[0].owner, Some(1));
            assert!(m.start.points[0].on_point.contains(&capper));
            assert!(m.start.economy.players[1].vp > vp);
        }
        for _ in 0..4 {
            advance(&mut m, &mut delivery);
        }
    }
}
