//! CMB-158 dependencies over ordinary ticks and every recipient view.
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

pub(super) fn scene() -> (Match, UnitId, UnitId) {
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

pub(super) fn relocate(m: &mut Match, id: UnitId, at: Point) {
    let u = m.world.game.units.get_mut(&id).unwrap();
    u.x = at.x;
    u.z = at.z;
    u.path.clear();
    u.hold_pos = true;
    m.grid.update(id, u);
}

pub(super) fn advance(m: &mut Match, delivery: &mut DeliveryState) -> TickReport {
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

pub(super) fn add(m: &mut Match, kind: &str, owner: usize, at: Point) -> UnitId {
    use crate::unit_publication::{PublicationParts, StagedUnit, prepare_publication};
    let id = m.next_unit_id;
    let staged = StagedUnit::ordinary(
        &m.world.game,
        &m.rules,
        owner,
        m.rules.kind(kind).unwrap(),
        id,
        m.world.game.units.len() as u32,
        m.start.players[owner].spawn,
        Some(at),
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
    let control = m.commands.units.get_mut(&id).unwrap();
    control.auto_retreat = false;
    control.auto_cast = false;
    control.hold_fire = true;
    id
}

#[test]
fn cmb158_radio_capture_affects_the_next_support_cooldown_pass() {
    // sim.js:5638-5642 reads radio ownership before :5709-5730 captures it.
    let (mut m, _, capper) = scene();
    let at = m.start.points[0].at;
    relocate(&mut m, capper, at);
    let dt = m.rules.movement.tick;
    m.start.points[0].kind = Some("radio".into());
    m.start.points[0].capper = Some(1);
    m.start.points[0].progress = 1.0 - dt / ObjectiveRules::compiled().unwrap().capture_time / 2.;
    m.start.players[1].support.insert("recon".into(), 100.);
    let radio = crate::catalog::Catalog::compiled()
        .unwrap()
        .table("CFG")
        .unwrap()
        .get("radioCd")
        .unwrap()
        .number()
        .unwrap();
    let mut delivery = DeliveryState::new(&m).unwrap();
    advance(&mut m, &mut delivery);
    assert_eq!(m.start.points[0].owner, Some(1));
    assert_eq!(m.start.players[1].support["recon"], 100. - dt);
    advance(&mut m, &mut delivery);
    assert_eq!(
        m.start.players[1].support["recon"],
        (100. - dt) - dt * radio
    );
}

#[test]
fn cmb158_lethal_flight_precedes_medic_healing_reinforcement_and_death() {
    // sim.js:5614 resolves the flight before :5655 healing and :5660 reinforcement.
    // A medic and a ready reinforcement timer must not resurrect a lethal hit.
    for lethal in [false, true] {
        let (mut m, thrower, patient) = scene();
        // Isolate the projectile from the constructor's clustered spawn actors.
        // An owned depot is also a canonical reinforcement home.
        let home = m.start.points[0].at;
        m.start.points[0].kind = Some("depot".into());
        m.start.points[0].owner = Some(1);
        m.start.points[0].progress = 1.;
        relocate(&mut m, patient, home);
        relocate(
            &mut m,
            thrower,
            Point {
                x: home.x - 14.,
                z: home.z,
            },
        );
        add(
            &mut m,
            "medic",
            1,
            Point {
                x: home.x + 3.,
                z: home.z,
            },
        );
        m.world.game.units.get_mut(&patient).unwrap().hp = 1.;
        m.recovery.get_mut(patient).unwrap().reinforcement = 100.;
        m.initialize_tick_scene(9, Default::default()).unwrap();
        if lethal {
            let blast =
                AbilityBlast::from_unit(m.world.game.units[&thrower].kind, &m.rules).unwrap();
            m.launch_ability_flight(thrower, home, blast, Some(m.rules.movement.tick), None)
                .unwrap();
            drop(m.firing.take_events());
        }
        let mut delivery = DeliveryState::new(&m).unwrap();
        let report = advance(&mut m, &mut delivery);
        if lethal {
            assert!(
                !m.world.game.units.contains_key(&patient),
                "patient={:?}, events={:?}",
                m.world.game.units.get(&patient),
                report.events
            );
            assert!(report.retired.iter().any(|u| u.unit.id == patient));
        } else {
            assert!(m.world.game.units[&patient].hp > 1.);
            assert_eq!(m.recovery.get(patient).unwrap().heal_at, Some(10));
            assert_eq!(m.recovery.get(patient).unwrap().reinforcement, 0.);
        }
        for _ in 0..4 {
            advance(&mut m, &mut delivery);
        }
    }
}

#[test]
fn cmb158_construction_commits_after_the_sites_unit_turn() {
    // sim.js:5349 skips unfinished buildings before :5585-5602 commits crew work.
    let (mut m, _, _) = scene();
    let worker = add(&mut m, "engineer", 0, Point { x: 60., z: 60. });
    m.start.economy.players[0].mp = 5000.;
    let site = m.next_unit_id;
    assert_eq!(
        m.execute_command(
            0,
            &Command::Build {
                ids: vec![i64::from(worker)],
                kind: "pillbox".into(),
                at: Some(Point { x: 64., z: 60. }),
                direction: None,
                queue: false,
            },
            false
        )
        .unwrap(),
        None
    );
    m.world.game.units.get_mut(&site).unwrap().built = Some(1. - 1e-8);
    m.firing.get_mut(site).unwrap().cooldown = 1.;
    let mut delivery = DeliveryState::new(&m).unwrap();
    advance(&mut m, &mut delivery);
    assert_eq!(m.world.game.units[&site].built, Some(1.));
    assert_eq!(
        m.firing.get(site).unwrap().cooldown,
        1.,
        "unfinished site skipped its turn"
    );
    advance(&mut m, &mut delivery);
    assert_eq!(
        m.firing.get(site).unwrap().cooldown,
        1. - m.rules.movement.tick
    );
}

#[test]
fn cmb158_physical_salvo_resolves_each_round_once_and_retires_its_timer() {
    // sim.js:5614,5624-5631 resolves physical flights before salvo bookkeeping.
    let (mut m, _, target) = scene();
    let at = m.start.points[0].at;
    relocate(&mut m, target, at);
    let launcher = add(
        &mut m,
        "rocket",
        0,
        Point {
            x: at.x - 20.,
            z: at.z,
        },
    );
    let flights = m.launch_weapon_salvo(launcher, at, Some(3)).unwrap();
    assert_eq!(flights.len(), 3);
    assert_eq!(m.firing.salvos[0].left, 3);
    drop(m.firing.take_events());
    let mut delivery = DeliveryState::new(&m).unwrap();
    let mut terminals = std::collections::BTreeMap::new();
    for _ in 0..400 {
        let report = advance(&mut m, &mut delivery);
        for event in &report.events {
            if let TickEvent::Firing(LaunchEvent::Contact(contact)) = event
                && contact.terminal
                && flights.contains(&contact.flight)
            {
                *terminals.entry(contact.flight).or_insert(0) += 1;
            }
        }
    }
    assert_eq!(terminals.len(), 3);
    assert!(terminals.values().all(|&count| count == 1));
    assert!(m.firing.salvos.is_empty());
    assert!(flights.iter().all(|id| !m.firing.flights.contains_key(id)));
}
