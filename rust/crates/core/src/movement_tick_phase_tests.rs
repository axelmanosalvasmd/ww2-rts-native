//! MOV-01: causal phase boundaries through ordinary canonical ticks.
use crate::{
    game::{PlaneState, Point},
    match_aviation::{AviationEvent, MissionKind, PlaneMission},
    match_commands::Command,
    match_firing::{AbilityBlast, LaunchEvent},
    match_runtime::{LivingWeather, Match},
    match_support::{FighterCover, SupportEvent, SupportKind},
    match_tick::{TickEvent, TickPathConfig, TickPhase, TickReport},
    mov_audit_test_support::{close, point, scene, spawn},
    path_requests::{Disposition, RequestKind},
    recipient_snapshot::{DeliveryState, SelectedSeat},
    weather::{WeatherKind, WeatherPlan},
};

fn quiet(m: &mut Match) {
    for u in m.world.game.units.values_mut() {
        u.hold_pos = true;
    }
    for c in m.commands.units.values_mut() {
        c.hold_fire = true;
        c.auto_cast = false;
        c.auto_retreat = false;
    }
}

fn actor(m: &mut Match, name: &str, owner: usize, at: Point) -> u32 {
    let id = spawn(m, name, owner, at);
    m.recovery.initialize(id);
    id
}

fn plane(m: &mut Match, owner: usize, at: Point) -> u32 {
    let id = m.spawn_aircraft(owner, "attacker").unwrap();
    let u = m.world.game.units.get_mut(&id).unwrap();
    u.x = at.x;
    u.z = at.z;
    u.air = Some(PlaneState::Station);
    m.aviation.planes.get_mut(&id).unwrap().mission = Some(PlaneMission {
        kind: MissionKind::Patrol,
        target: None,
        at,
    });
    m.grid.update(id, &m.world.game.units[&id]);
    id
}

// These causal fixtures also exercise currently undeliverable aviation events.
// They advance ordinary rules state only and make no recipient-delivery claim.
fn advance(m: &mut Match) -> TickReport {
    let report = m.advance_tick().unwrap();
    assert!(m.tick_state.fault().is_none());
    report
}

fn tick(m: &mut Match, delivery: &mut DeliveryState) -> TickReport {
    let report = advance(m);
    let seats = (0..m.start.players.len())
        .map(|slot| SelectedSeat {
            slot,
            full: true,
            omit: false,
        })
        .collect::<Vec<_>>();
    let frame = delivery
        .capture_selected_with_tick_events(m, &seats, &report.events)
        .unwrap()
        .unwrap();
    for slot in 0..seats.len() {
        assert_eq!(frame.recipient(slot).unwrap().into_json()["t"], "s");
    }
    assert!(m.tick_state.fault().is_none());
    report
}

fn before(report: &TickReport, a: TickPhase, b: TickPhase) {
    let index = |p| report.phases.iter().position(|phase| *phase == p).unwrap();
    assert!(index(a) < index(b), "{a:?} must precede {b:?}");
}

#[test]
fn mov01_grid_rebuild_precedes_vision_and_the_first_unit_turn() {
    // sim.js:5308-5319 rebuilds the whole grid before the new tick's vision.
    // Move an unvisited higher-ID target without updating its previous bucket.
    let mut m = scene();
    let shooter = actor(&mut m, "rifle", 0, point(41., 41.));
    let target = actor(&mut m, "rifle", 1, point(101., 101.));
    quiet(&mut m);
    let enemy = m.world.game.units.get_mut(&target).unwrap();
    enemy.x = 51.;
    enemy.z = 41.;
    let control = m.commands.units.get_mut(&shooter).unwrap();
    control.hold_fire = false;
    control.attack_id = target;
    let mut delivery = DeliveryState::new(&m).unwrap();
    let report = tick(&mut m, &mut delivery);
    assert_eq!(report.tick, 1);
    close(report.prior[&target].x, 51.);
    assert!(m.world.game.players[0].sees(target));
    assert!(m.firing.units[&shooter].cooldown > 0.);
    assert!(report.events.iter().any(|e| match e {
        TickEvent::Firing(LaunchEvent::SmallArms(shot)) => shot.shooter == shooter,
        TickEvent::Firing(LaunchEvent::Flight { shooter: id, .. }) => *id == shooter,
        _ => false,
    }));
    for _ in 0..3 {
        tick(&mut m, &mut delivery);
    }
}

#[test]
fn mov01_fifo_is_served_before_movement_and_ordinary_phase_order_survives_600_ticks() {
    // e4e579a sim.js:5305-5340,5585-5644,5672-5692.
    let mut m = scene();
    m.initialize_tick_scene(
        0,
        TickPathConfig {
            budget: 1,
            retry_limit: 20,
        },
    )
    .unwrap();
    let ids = [41., 51., 61.].map(|z| actor(&mut m, "rifle", 0, point(41., z)));
    quiet(&mut m);
    m.begin_path_tick(0, 20).unwrap();
    for (index, &id) in ids.iter().enumerate() {
        let u = m.world.game.units.get_mut(&id).unwrap();
        u.hold_pos = false;
        u.rot = 0.;
        let goal = point(91., u.z);
        u.world_goal = Some(goal);
        if index < 2 {
            assert_eq!(
                m.request_command_path(id, goal, RequestKind::World)
                    .unwrap(),
                Disposition::Queued,
            );
        }
    }
    let mut delivery = DeliveryState::new(&m).unwrap();
    for expected_tick in 1..=600 {
        let report = tick(&mut m, &mut delivery);
        assert_eq!(report.tick, expected_tick);
        for (a, b) in [
            (TickPhase::StructureSync, TickPhase::Clock),
            (TickPhase::Clock, TickPhase::Paths),
            (TickPhase::Paths, TickPhase::Movement),
            (TickPhase::Movement, TickPhase::Construction),
            (TickPhase::Construction, TickPhase::Separation),
            (TickPhase::Separation, TickPhase::Projectiles),
            (TickPhase::Projectiles, TickPhase::Support),
            (TickPhase::Support, TickPhase::AntiAir),
            (TickPhase::AntiAir, TickPhase::SupportTimers),
            (TickPhase::SupportTimers, TickPhase::LivingWeather),
            (TickPhase::LivingWeather, TickPhase::FireFlood),
            (TickPhase::FireFlood, TickPhase::Death),
        ] {
            before(&report, a, b);
        }
        if expected_tick <= 3 {
            assert_eq!(report.paths.searched, 1);
            for (index, &id) in ids.iter().enumerate() {
                let u = &m.world.game.units[&id];
                assert_eq!(
                    u.x > 41.,
                    index < expected_tick as usize,
                    "prior FIFO waiters move before newly requested work",
                );
                if index + 1 == expected_tick as usize {
                    close(report.prior[&id].x, 41.);
                }
            }
        }
    }
    for id in ids {
        let u = &m.world.game.units[&id];
        assert!(u.x > 85. && u.path.is_empty());
    }
}

#[test]
fn mov01_projectiles_remove_grenade_tickets_before_support_aa_and_vehicle_retirement() {
    // Source late flights/grenades/salvos precede strikes/AA and deaths:
    // sim.js:5613-5632,5672-5692. A living control gun proves the AA branch.
    for hit in [false, true] {
        let mut m = scene();
        let gun = actor(&mut m, "flaktrack", 1, point(61., 61.));
        let thrower = actor(&mut m, "ranger", 0, point(51., 61.));
        let rocket = actor(&mut m, "rocket", 0, point(81., 101.));
        let aircraft = plane(&mut m, 0, point(61., 73.));
        quiet(&mut m);
        m.commands.units.get_mut(&gun).unwrap().hold_fire = false;
        m.world.game.units.get_mut(&gun).unwrap().hp = 1.;
        assert_eq!(
            m.admit_support(0, "recon", point(61., 61.), Some(0.))
                .unwrap(),
            None
        );
        m.world.game.strikes[0].runtime.as_mut().unwrap().delay = 0.01;
        let grenade = if hit {
            let blast =
                AbilityBlast::from_unit(m.world.game.units[&thrower].kind, &m.rules).unwrap();
            Some(
                m.launch_ability_flight(thrower, point(61., 61.), blast, Some(0.04), None)
                    .unwrap(),
            )
        } else {
            None
        };
        m.launch_weapon_salvo(rocket, point(101., 101.), Some(2))
            .unwrap();
        // An authored nearly-due logical salvo still has its physical flights.
        m.firing.salvos[0].next = 0.01;
        m.firing.take_events();
        let plane_hp = m.world.game.units[&aircraft].hp;
        let report = advance(&mut m);
        assert_eq!(m.firing.salvos.len(), 1);
        assert_eq!(m.firing.salvos[0].left, 1);
        close(m.firing.salvos[0].next, m.firing.salvos[0].every);
        let support_flak = report.events.iter().any(|e| {
            matches!(e,
            TickEvent::Support(SupportEvent::Flak { unit, .. }) if *unit == gun)
        });
        let aa = report.events.iter().any(|e| {
            matches!(e,
            TickEvent::Aviation(AviationEvent::AntiAir { shooter, target, .. })
                if *shooter == gun && *target == aircraft)
        });
        assert_eq!(support_flak, !hit);
        assert_eq!(aa, !hit);
        assert_eq!(m.world.game.units[&aircraft].hp < plane_hp, !hit);
        if let Some(flight) = grenade {
            assert!(!m.firing.flights.contains_key(&flight));
            assert!(m.firing.grenades.is_empty());
            assert!(!m.world.game.units.contains_key(&gun));
            assert!(report.retired.iter().any(|r| r.unit.id == gun));
            assert!(m.world.game.wrecks.iter().any(|w| w.id == Some(gun)));
            let contact = report
                .events
                .iter()
                .position(|e| {
                    matches!(e,
                TickEvent::Firing(LaunchEvent::Contact(c)) if c.flight == flight)
                })
                .unwrap();
            let arrival = report
                .events
                .iter()
                .position(|e| {
                    matches!(
                        e,
                        TickEvent::Support(SupportEvent::Plane {
                            kind: SupportKind::Recon,
                            ..
                        })
                    )
                })
                .unwrap();
            assert!(contact < arrival);
        }
        for _ in 0..3 {
            advance(&mut m);
        }
    }
}

#[test]
fn mov01_finishing_build_crews_enable_aa_in_the_same_late_phase() {
    // sim.js:5585-5603 finishes captured crews before the late actor list,
    // separation and antiAir at 5632. An unfinished control cannot shoot.
    for finishes in [false, true] {
        let mut m = scene();
        let worker = actor(&mut m, "engineer", 0, point(58., 62.));
        m.start.economy.players[0].mp = 5000.;
        let site = m.next_unit_id;
        assert_eq!(
            m.execute_command(
                0,
                &Command::Build {
                    ids: vec![i64::from(worker)],
                    kind: "flakpos".into(),
                    at: Some(point(61., 61.)),
                    direction: None,
                    queue: false,
                },
                false
            )
            .unwrap(),
            None
        );
        // Placement snaps this two-cell site to (62, 62). The crew starts
        // four metres away, inside radius 2 + buildReach 2.5, and works now.
        close(m.world.game.units[&site].x, 62.);
        close(m.world.game.units[&site].z, 62.);
        assert_eq!(m.commands.units[&worker].build_target, Some(site));
        m.world.game.units.get_mut(&site).unwrap().built = Some(if finishes { 0.999 } else { 0.5 });
        let aircraft = plane(&mut m, 1, point(61., 73.));
        quiet(&mut m);
        m.commands.units.get_mut(&site).unwrap().hold_fire = false;
        let hp = m.world.game.units[&aircraft].hp;
        let report = advance(&mut m);
        assert_eq!(m.world.game.units[&site].built == Some(1.), finishes);
        assert_eq!(m.world.game.units[&aircraft].hp < hp, finishes);
        assert_eq!(
            report.events.iter().any(|e| matches!(e,
            TickEvent::Aviation(AviationEvent::AntiAir { shooter, target, .. })
                if *shooter == site && *target == aircraft)),
            finishes
        );
        before(&report, TickPhase::Construction, TickPhase::AntiAir);
        for _ in 0..3 {
            advance(&mut m);
        }
    }
}

#[test]
fn mov01_cover_intercepts_before_decay_and_new_smoke_drifts_after_old_rain_decay() {
    // sim.js:5631-5644; weather():4858-4870; cover interception:4694-4711.
    let mut m = scene();
    quiet(&mut m);
    m.start.economy.players[0].mp = 5000.;
    m.options.living_weather = true;
    m.start.living = Some(LivingWeather {
        raining: true,
        next: 100.,
    });
    m.start.weather = WeatherPlan::fixed(WeatherKind::Rain);
    m.world.game.weather = Some(WeatherKind::Rain as usize);
    m.world.game.rain = 0.4;
    m.world.game.wet = Some(0.2);
    m.start.wind = (0., 0.6);
    m.start.points[0].kind = Some("radio".into());
    m.start.points[0].owner = Some(0);
    m.start.points[0].cut = false;
    m.support.covers.push(FighterCover {
        team: 1,
        owner: 1,
        at: point(41., 41.),
        radius: 40.,
        remaining: 0.01,
    });
    for (name, at) in [
        ("strafe", point(41., 41.)),
        ("cover", point(101., 41.)),
        ("smoke", point(61., 81.)),
    ] {
        assert_eq!(m.admit_support(0, name, at, Some(0.)).unwrap(), None);
        m.world
            .game
            .strikes
            .last_mut()
            .unwrap()
            .runtime
            .as_mut()
            .unwrap()
            .delay = 0.01;
    }
    let cooldowns = m.start.players[0].support.clone();
    let report = advance(&mut m);
    assert!(report.events.iter().any(|e| matches!(
        e,
        TickEvent::Support(SupportEvent::ShotDown {
            fighters: true,
            kind: SupportKind::Strafe,
            ..
        })
    )));
    assert_eq!(m.support.covers.len(), 1);
    assert_eq!(m.support.covers[0].owner, 0);
    close(m.support.covers[0].remaining, 60. - 0.05);
    for (kind, old) in cooldowns {
        close(m.start.players[0].support[&kind], old - 0.05 * 1.5);
    }
    close(m.world.game.rain, 0.4025);
    assert_eq!(m.world.game.smokes.len(), 5);
    let dx = crate::math::cos(m.start.wind.0) * m.start.wind.1 * 1.2 * 0.05;
    let dz = crate::math::sin(m.start.wind.0) * m.start.wind.1 * 1.2 * 0.05;
    assert!(dx > 0.);
    for (index, smoke) in m.world.game.smokes.iter().enumerate() {
        close(smoke.remaining.unwrap(), 20. - 0.05 * (1. + 0.5 * 0.4));
        close(smoke.x, 61. + (index as f64 / 4. - 0.5) * 29. + dx);
        assert!((smoke.z - 81. - dz).abs() <= 1.);
    }
    for _ in 0..3 {
        advance(&mut m);
    }
}
