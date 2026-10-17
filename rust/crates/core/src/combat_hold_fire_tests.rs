//! Hold Fire intent at the pinned prototype's targeting, ability and reaction branches.
use super::{command, p, rebuild, scene, terrain};
use crate::{
    game::{PlaneState, Point, Strike, StrikeKind},
    match_abilities::AutocastTarget,
    match_aviation::{AviationEvent, MissionKind, PlaneMission},
    match_firing::LaunchEvent,
    match_runtime::Match,
    match_support::{SupportEvent, SupportKind, SupportTiming},
    random::{MatchRandom, StreamId},
    recipient_snapshot::{DeliveryState, SelectedSeat},
};
use serde_json::json;

fn hold(m: &mut Match, id: u32, on: bool) {
    assert_eq!(
        command(m, json!({"t":"stance","key":"holdFire","ids":[id],"on":on})),
        None
    );
}

fn plane(m: &mut Match, owner: usize, kind: &str, at: Point, center: Point) -> u32 {
    let id = m.spawn_aircraft(owner, kind).unwrap();
    let u = m.world.game.units.get_mut(&id).unwrap();
    u.x = at.x;
    u.z = at.z;
    u.air = Some(PlaneState::Out);
    m.aviation.planes.get_mut(&id).unwrap().mission = Some(PlaneMission {
        kind: MissionKind::Patrol,
        target: None,
        at: center,
    });
    rebuild(m);
    id
}

#[test]
fn cmb84_ground_hold_clears_automatic_target_but_explicit_attack_launches() {
    let mut m = scene(&[("rifle", 0, 40., 40.), ("rifle", 1, 54., 40.)]);
    m.apply_unit_firing(1, false, 0.05).unwrap();
    assert_eq!(m.firing.get(1).unwrap().target_id, 2);
    assert!(!m.firing.flights.is_empty(), "unheld rifles actually fire");
    let flights = m.firing.flights.len();
    m.firing.take_events();
    m.firing.get_mut(1).unwrap().cooldown = 0.;
    hold(&mut m, 1, true);
    m.apply_unit_firing(1, false, 0.05).unwrap();
    assert_eq!(m.firing.get(1).unwrap().target_id, 0);
    assert_eq!(m.firing.flights.len(), flights);
    assert!(m.firing.take_events().is_empty());
    assert_eq!(
        command(&mut m, json!({"t":"attack","ids":[1],"target":2})),
        None
    );
    m.apply_ground_attack(1).unwrap();
    m.apply_unit_firing(1, false, 0.05).unwrap();
    assert_eq!(m.firing.get(1).unwrap().target_id, 2);
    assert!(m.firing.flights.len() > flights);
    assert!(
        m.firing
            .take_events()
            .iter()
            .any(|event| matches!(event, LaunchEvent::Flight { shooter: 1, .. }))
    );
    assert!(m.commands.units[&1].hold_fire);
}

#[test]
fn cmb84_hold_blocks_grenade_suppress_and_barrage_autocast_with_live_controls() {
    for kind in ["rifle", "mg", "rocket"] {
        let target_x = if kind == "rocket" { 80. } else { 54. };
        let mut m = scene(&[(kind, 0, 40., 40.), ("rifle", 1, target_x, 40.)]);
        if kind == "mg" {
            let enemy = m.world.game.units.get_mut(&2).unwrap();
            enemy.path = vec![p(40., 40.)];
            enemy.rot = std::f64::consts::PI;
        } else {
            terrain(
                &mut m,
                target_x,
                40.,
                if kind == "rocket" { b'T' } else { b'O' },
            );
        }
        m.world.game.tick = 209;
        m.start.economy.players[0].mun = 1000.;
        m.commands.units.get_mut(&1).unwrap().auto_cast = true;
        let expected = if kind == "mg" {
            AutocastTarget::Instant
        } else {
            AutocastTarget::At(p(target_x, 40.))
        };
        assert_eq!(
            m.autocast_target(1).unwrap(),
            Some(expected),
            "{kind} positive judgment"
        );
        hold(&mut m, 1, true);
        let wallet = m.start.economy.players[0].mun;
        assert_eq!(m.autocast_target(1).unwrap(), None, "{kind} held judgment");
        assert_eq!(m.apply_autocast(1).unwrap(), None);
        assert_eq!(m.commands.units[&1].nade, None);
        assert_eq!(m.abilities.get(1).unwrap().buff, 0.);
        assert_eq!(m.abilities.get(1).unwrap().cooldown, 0.);
        assert_eq!(m.start.economy.players[0].mun, wallet);
        hold(&mut m, 1, false);
        assert_eq!(m.apply_autocast(1).unwrap(), None, "{kind} permitted cast");
        if kind == "mg" {
            assert!(m.abilities.get(1).unwrap().buff > 0.);
            assert!(m.abilities.get(1).unwrap().cooldown > 0.);
        } else {
            assert_eq!(m.commands.units[&1].nade, Some(p(target_x, 40.)));
            assert!(m.abilities.get(1).unwrap().aimed_auto);
        }
    }
}

#[test]
fn cmb84_aircraft_patrol_holds_cached_target_and_attack_order_fires() {
    for kind in ["fighter", "attacker"] {
        let mut m = scene(&[("rifle", 1, 54., 40.)]);
        let id = plane(&mut m, 0, kind, p(40., 40.), p(54., 40.));
        m.world.game.players[0].visible = vec![1];
        m.aviation.planes.get_mut(&id).unwrap().attack_pick = 1;
        m.firing.get_mut(id).unwrap().retarget = 99.;
        m.firing.get_mut(id).unwrap().target_id = 1;
        hold(&mut m, id, true);
        let ammo = m.aviation.planes[&id].ammo;
        m.step_aircraft_turn(id, 0.05).unwrap();
        assert_eq!(
            m.aviation.planes[&id].ammo, ammo,
            "{kind} holds patrol fire"
        );
        assert_eq!(m.firing.get(id).unwrap().target_id, 0);
        assert!(m.firing.flights.is_empty());
        assert_eq!(
            command(&mut m, json!({"t":"attack","ids":[id],"target":1})),
            None
        );
        m.step_aircraft_turn(id, 0.05).unwrap();
        assert_eq!(
            m.aviation.planes[&id].ammo,
            ammo - 1.,
            "{kind} ordered shot"
        );
        assert_eq!(m.firing.get(id).unwrap().target_id, 1);
        assert!(!m.firing.flights.is_empty());
        let flights = m.firing.flights.len();
        assert_eq!(
            command(&mut m, json!({"t":"move","orders":[[id,54.,40.]]})),
            None
        );
        m.firing.get_mut(id).unwrap().cooldown = 0.;
        m.step_aircraft_turn(id, 0.05).unwrap();
        assert_eq!(m.firing.get(id).unwrap().target_id, 0);
        assert_eq!(m.firing.flights.len(), flights);
        hold(&mut m, id, false);
        m.step_aircraft_turn(id, 0.05).unwrap();
        assert_eq!(m.firing.get(id).unwrap().target_id, 1);
        assert_eq!(
            m.aviation.planes[&id].ammo,
            ammo - 2.,
            "{kind} unheld patrol fires"
        );
        assert!(m.firing.flights.len() > flights);
    }
}

#[test]
fn cmb84_hold_blocks_ground_and_fighter_aa_except_the_ordered_plane() {
    for kind in ["mg", "flak", "fighter"] {
        let mut m = if kind == "fighter" {
            scene(&[])
        } else {
            scene(&[(kind, 0, 40., 40.)])
        };
        let gun = if kind == "fighter" {
            plane(&mut m, 0, kind, p(40., 40.), p(40., 40.))
        } else {
            1
        };
        let far = plane(&mut m, 1, "attacker", p(54., 40.), p(54., 40.));
        let near = plane(&mut m, 1, "attacker", p(46., 40.), p(46., 40.));
        m.world.game.players[0].visible = vec![far, near];
        let far_hp = m.world.game.units[&far].hp;
        let near_hp = m.world.game.units[&near].hp;
        hold(&mut m, gun, true);
        m.step_aircraft_anti_air(&[gun], 0.3).unwrap();
        assert_eq!(m.world.game.units[&far].hp, far_hp, "{kind}");
        assert_eq!(m.world.game.units[&near].hp, near_hp, "{kind}");
        assert_eq!(m.combat.get(gun).unwrap().xp, 0.);
        assert!(m.aviation.take_events().is_empty());
        if kind == "fighter" {
            assert_eq!(
                command(&mut m, json!({"t":"attack","ids":[gun],"target":far})),
                None
            );
            m.step_aircraft_anti_air(&[gun], 0.3).unwrap();
            assert!(m.world.game.units[&far].hp < far_hp);
            assert_eq!(
                m.world.game.units[&near].hp, near_hp,
                "a nearer unrequested plane stays untouched"
            );
            assert!(m.aviation.take_events().iter().any(|e| matches!(e,
                AviationEvent::AntiAir { shooter, target, .. } if *shooter == gun && *target == far)));
            assert_eq!(
                command(&mut m, json!({"t":"move","orders":[[gun,40.,40.]]})),
                None
            );
        }
        hold(&mut m, gun, false);
        m.step_aircraft_anti_air(&[gun], 0.3).unwrap();
        assert!(
            m.world.game.units[&near].hp < near_hp,
            "{kind} unheld nearest-plane fire"
        );
        assert!(m.combat.get(gun).unwrap().xp > 0.);
    }
}

#[test]
fn cmb84_hold_prevents_fighter_hunting_without_canceling_patrol_movement() {
    let ready = || {
        let mut m = scene(&[]);
        let fighter = plane(&mut m, 0, "fighter", p(40., 40.), p(100., 40.));
        let enemy = plane(&mut m, 1, "attacker", p(100., 70.), p(100., 70.));
        m.world.game.players[0].visible = vec![enemy];
        (m, fighter)
    };
    let (mut m, fighter) = ready();
    let (mut hunting, hunting_fighter) = ready();
    assert_eq!(fighter, hunting_fighter);
    hold(&mut m, fighter, true);
    hold(&mut hunting, fighter, false);
    m.step_aircraft_turn(fighter, 0.05).unwrap();
    hunting.step_aircraft_turn(fighter, 0.05).unwrap();
    let held_at = m.world.game.units[&fighter].at();
    let hunting_at = hunting.world.game.units[&fighter].at();
    assert!(held_at.x > 40.);
    assert_eq!(
        held_at.z, 40.,
        "the held fighter follows the ordered patrol"
    );
    assert!(
        hunting_at.z > 40.,
        "the unheld fighter turns toward the visible enemy plane"
    );
    assert_ne!(held_at, hunting_at);
    assert_eq!(
        m.aviation.planes[&fighter].mission,
        hunting.aviation.planes[&fighter].mission
    );
    assert_eq!(
        m.aviation.planes[&fighter].ammo,
        hunting.aviation.planes[&fighter].ammo
    );
}

#[test]
fn cmb84_hold_prevents_support_flak_attempt_and_preserves_its_round_and_roll() {
    let mut m = scene(&[("flak", 0, 40., 40.)]);
    m.logistics
        .initialize_unit(
            crate::logistics::Context {
                enabled_override: Some(true),
                scenario: false,
                mode: crate::logistics::Mode::Conquest,
            },
            crate::logistics::UnitFacts {
                id: 1,
                kind: "flak",
                owner: Some(0),
                owner_exists: true,
                guard_home: false,
                riding: false,
            },
            Some(&crate::logistics::Definition::from_catalog("flak").unwrap()),
        )
        .unwrap();
    m.start.random = MatchRandom::new(1);
    let mut control_roll = m.start.random.clone();
    assert!(
        control_roll.next_fraction(StreamId::OffMap) < 0.35,
        "the permitted attempt downs its target"
    );
    hold(&mut m, 1, true);
    let ammo = m.logistics.get(1).unwrap().ammo;
    let random = m.start.random.clone();
    let strike = Strike {
        kind: StrikeKind::Recon,
        owner: 1,
        x: 40.,
        z: 40.,
        dir: 0.,
        live: false,
        runtime: Some(SupportTiming {
            kind: SupportKind::Recon,
            delay: 0.,
            left: 1.,
            next: 0.,
        }),
    };
    m.world.game.strikes.push(strike.clone());
    m.step_support_strikes(&[1], 0.05).unwrap();
    assert!(
        m.support
            .take_events()
            .iter()
            .all(|e| !matches!(e, SupportEvent::Flak { .. } | SupportEvent::ShotDown { .. }))
    );
    assert_eq!(m.logistics.get(1).unwrap().ammo, ammo);
    assert_eq!(m.start.random, random);
    assert_eq!(m.story[0].planes_downed, 0.);
    m.world.game.strikes.clear();
    m.world.game.strikes.push(strike);
    hold(&mut m, 1, false);
    m.step_support_strikes(&[1], 0.05).unwrap();
    let events = m.support.take_events();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, SupportEvent::Flak { unit: 1, .. }))
    );
    assert!(events.iter().any(|e| matches!(
        e,
        SupportEvent::ShotDown {
            fighters: false,
            ..
        }
    )));
    assert_eq!(m.logistics.get(1).unwrap().ammo, ammo - 1.);
    assert_ne!(m.start.random, random);
    assert_eq!(m.story[0].planes_downed, 1.);
}

#[test]
fn cmb84_rally_walker_keeps_its_route_under_fire_until_hold_is_released() {
    let mut m = scene(&[("rifle", 0, 40., 40.), ("rifle", 1, 54., 40.)]);
    let route = vec![p(60., 40.), p(80., 40.)];
    let unit = m.world.game.units.get_mut(&1).unwrap();
    unit.path = route.clone();
    unit.drift = true;
    unit.hit_from = Some(p(54., 40.));
    m.commands.units.get_mut(&1).unwrap().rally_drift = true;
    m.combat.get_mut(1).unwrap().hit_by = Some(2);
    m.combat.get_mut(1).unwrap().hit_at = m.world.game.tick as f64;
    hold(&mut m, 1, true);
    m.react_unit(1).unwrap();
    assert_eq!(m.world.game.units[&1].path, route);
    assert!(m.world.game.units[&1].drift);
    assert!(m.commands.units[&1].rally_drift);
    assert_eq!(m.world.game.units[&1].amove, None);
    assert_eq!(m.firing.get(1).unwrap().target_id, 0);
    hold(&mut m, 1, false);
    m.react_unit(1).unwrap();
    assert!(m.world.game.units[&1].path.is_empty());
    assert!(!m.world.game.units[&1].drift);
    assert!(!m.commands.units[&1].rally_drift);
    assert_eq!(m.world.game.units[&1].amove, route.last().copied());
    assert_eq!(m.firing.get(1).unwrap().target_id, 2);
}

#[test]
fn cmb84_ordinary_ticks_deliver_held_patrol_and_ordered_air_fire_through_finish() {
    let map = crate::map::AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"Hold Fire match", "w":96, "h":96,
            "rows":vec![".".repeat(96);96],
            "spawns":[{"x":3,"y":3},{"x":92,"y":92}],
            "points":[{"id":"p","x":80,"y":80}]
        }))
        .unwrap(),
    )
    .unwrap();
    let mut m = Match::new(&map, scene(&[]).options).unwrap();
    crate::cmb5_match_tests::prepare_finish(&mut m);
    for c in m.commands.units.values_mut() {
        c.hold_fire = true;
        c.auto_cast = false;
        c.auto_retreat = false;
    }
    let target = m
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 1 && m.rules.kinds[u.kind].name == "rifle")
        .unwrap()
        .id;
    let enemy = m.world.game.units.get_mut(&target).unwrap();
    enemy.x = 54.;
    enemy.z = 40.;
    let attacker = plane(&mut m, 0, "attacker", p(40., 40.), p(54., 40.));
    hold(&mut m, attacker, true);
    crate::vision::update_vision(&mut m.world.game, &m.rules, &m.grid);
    let ammo = m.aviation.planes[&attacker].ammo;
    let mut delivery = DeliveryState::new(&m).unwrap();
    let selected = (0..m.world.game.players.len())
        .map(|slot| SelectedSeat {
            slot,
            full: false,
            omit: false,
        })
        .collect::<Vec<_>>();
    let mut fired = false;
    let mut retained = false;
    let mut delivered_flight = false;
    let mut finish_tick = None;
    let mut deliveries = 0;
    for turn in 0..200 {
        if turn == 3 {
            assert_eq!(
                command(
                    &mut m,
                    json!({"t":"attack","ids":[attacker],"target":target})
                ),
                None
            );
        }
        let report = m.advance_tick().unwrap();
        if turn < 3 {
            assert_eq!(m.aviation.planes[&attacker].ammo, ammo);
            assert_eq!(m.firing.get(attacker).unwrap().target_id, 0);
            assert!(m.firing.flights.is_empty());
        }
        fired |= report.events.iter().any(|event| matches!(event,
            crate::match_tick::TickEvent::Firing(LaunchEvent::Flight { shooter, .. }) if *shooter == attacker));
        retained |= m
            .firing
            .flights
            .values()
            .any(|flight| flight.shooter == attacker);
        // Full composition carries retained projectiles as well as this tick's events.
        let frame = delivery
            .capture_selected_with_tick_events(&mut m, &selected, &report.events)
            .unwrap()
            .unwrap();
        for seat in &selected {
            let snapshot = frame.recipient(seat.slot).unwrap().into_json();
            assert_eq!(snapshot["t"], "s");
            if seat.slot == 0 {
                delivered_flight |= snapshot["flights"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|row| row["f"] == attacker);
            }
            deliveries += 1;
        }
        assert_eq!(m.tick_state.fault(), None);
        assert!(m.commands.units[&attacker].hold_fire);
        if let Some(finish) = &m.finish {
            assert_eq!(finish.winner, 0);
            assert_eq!(finish.reason, crate::match_end::EndReason::VictoryPoints);
            let end = *finish_tick.get_or_insert(m.world.game.tick);
            if m.world.game.tick >= end + 8 {
                break;
            }
        }
    }
    assert!(fired, "explicit attack fires while Hold Fire remains on");
    assert!(retained, "the match retains the aircraft's live projectile");
    assert!(
        delivered_flight,
        "the owner receives the retained aircraft flight"
    );
    assert!(m.aviation.planes[&attacker].ammo < ammo);
    let end = finish_tick.expect("ordinary objective income finishes the match");
    assert!(end >= 100);
    assert!(m.world.game.tick >= end + 8);
    assert_eq!(deliveries, m.world.game.tick as usize * selected.len());
    assert_eq!(m.tick_state.fault(), None);
}
