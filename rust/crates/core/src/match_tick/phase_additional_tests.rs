//! Causal boundaries through ordinary ticks and every recipient view.
use super::phase_dependency_tests::{advance, relocate, scene};
use crate::{
    game::{Point, Smoke},
    match_runtime::{LivingWeather, Match},
    recipient_snapshot::DeliveryState,
    unit_publication::{PublicationParts, StagedUnit, prepare_publication},
};

fn publish(m: &mut Match, name: &str, owner: usize, at: Point) -> u32 {
    let id = m.next_unit_id;
    let staged = StagedUnit::ordinary(
        &m.world.game,
        &m.rules,
        owner,
        m.rules.kind(name).unwrap(),
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
        m.start.players.len(),
        vec![staged],
    )
    .unwrap()
    .publish_with(|unit, _| {
        m.combat.initialize(unit.id);
        m.firing.initialize(unit.id);
        m.abilities.initialize(unit.id);
        m.recovery.initialize(unit.id);
    });
    let control = m.commands.units.get_mut(&id).unwrap();
    control.auto_cast = false;
    control.auto_retreat = false;
    control.hold_fire = true;
    id
}

#[test]
fn cmb158_ready_timer_autocasts_before_movement_but_retreat_prevents_it() {
    // Prototype sim.js:5361-5373 decrements timers, retreats, then autocasts.
    for wounded in [false, true] {
        let (mut m, _, _) = scene();
        let at = Point { x: 75., z: 65. };
        let id = publish(&mut m, "conscript", 0, at);
        let mut delivery = DeliveryState::new(&m).unwrap();
        while (m.world.game.tick + 1 + i64::from(id)) % 10 != 0 {
            advance(&mut m, &mut delivery);
        }
        let dt = m.rules.movement.tick;
        let control = m.commands.units.get_mut(&id).unwrap();
        control.auto_cast = true;
        control.auto_retreat = wounded;
        let u = m.world.game.units.get_mut(&id).unwrap();
        u.path = vec![Point { x: 110., z: at.z }];
        u.supp = 80.;
        if wounded {
            u.hp = 1.;
        }
        m.abilities.get_mut(id).unwrap().cooldown = dt / 2.;
        advance(&mut m, &mut delivery);
        let u = &m.world.game.units[&id];
        assert_ne!(u.at(), at, "the same unit turn must move");
        assert_eq!(u.retreating, wounded);
        if wounded {
            assert!(u.sprint <= 0., "retreat must not create a new sprint");
            assert!(m.abilities.get(id).unwrap().cooldown < 0.);
            assert_ne!(u.path.last().unwrap().x, 110.);
        } else {
            assert_eq!(u.sprint, 6., "autocast follows sprint timer decay");
            assert_eq!(m.abilities.get(id).unwrap().cooldown, 35.);
            assert_eq!(u.path.last().unwrap().x, 110.);
            assert!(
                u.x - at.x > m.rules.kinds[u.kind].speed * dt * crate::movement::supp_speed(80.),
                "movement must consume the sprint created by this turn's autocast"
            );
        }
        for _ in 0..4 {
            advance(&mut m, &mut delivery);
        }
    }
}

#[test]
fn cmb158_vision_refresh_precedes_explicit_attack_turn() {
    // Prototype sim.js:5320 refreshes sight before the attack-order check.
    for visible in [false, true] {
        let (mut m, shooter, target) = scene();
        let mut delivery = DeliveryState::new(&m).unwrap();
        relocate(&mut m, shooter, Point { x: 65., z: 65. });
        relocate(
            &mut m,
            target,
            Point {
                x: if visible { 79. } else { 145. },
                z: 65.,
            },
        );
        m.world.game.players[0].visible = if visible { vec![] } else { vec![target] };
        m.commands.units.get_mut(&shooter).unwrap().attack_id = target;
        m.firing.get_mut(shooter).unwrap().cooldown = 0.;
        advance(&mut m, &mut delivery);
        assert_eq!(m.world.game.players[0].sees(target), visible);
        assert_eq!(
            m.commands.units[&shooter].attack_id,
            if visible { target } else { 0 }
        );
        assert_eq!(
            m.firing.get(shooter).unwrap().target_id,
            if visible { target } else { 0 }
        );
        for _ in 0..4 {
            advance(&mut m, &mut delivery);
        }
    }
}

#[test]
fn cmb158_living_rain_updates_before_burn_and_expired_smoke_suppresses_replacement() {
    // Prototype sim.js:5645-5649 ages smoke, runs weather/burn, then removes smoke.
    for expiring_smoke in [false, true] {
        let (mut m, _, _) = scene();
        let width = m.world.game.w;
        let fire = 30 * width + 35;
        for dy in -1..=1 {
            for dx in -1..=1 {
                m.world
                    .set_cell((fire as isize + dy * width as isize + dx) as usize, b'H')
                    .unwrap();
            }
        }
        m.world.fires.insert(fire, 14.);
        m.start.environment_seed = 35;
        let mut delivery = DeliveryState::new(&m).unwrap();
        for _ in 0..9 {
            advance(&mut m, &mut delivery);
        }
        m.options.living_weather = true;
        m.start.living = Some(LivingWeather {
            raining: true,
            next: 100.,
        });
        m.world.game.wet = Some(0.);
        m.world.game.rain = 0.;
        let dt = m.rules.movement.tick;
        if expiring_smoke {
            m.world.game.smokes.push(Smoke {
                id: m.next_unit_id,
                x: 71.,
                z: 61.,
                r: 30.,
                remaining: Some(dt / 2.),
            });
            m.next_unit_id += 1;
        }
        advance(&mut m, &mut delivery);
        assert!((m.world.game.rain - dt / 20.).abs() < 1e-12);
        let expected = 14. - 10. * dt * (1. + 3. * m.world.game.rain);
        assert!((*m.world.fires.get(&fire).unwrap() - expected).abs() < 1e-12);
        assert!(
            m.world.fires.len() > 1,
            "seeded spread must actually ignite neighbors"
        );
        if expiring_smoke {
            assert!(
                m.world.game.smokes.is_empty(),
                "the expired cloud suppresses replacement before pruning"
            );
        } else {
            assert!(
                !m.world.game.smokes.is_empty(),
                "newly ignited hedges produce smoke without an existing cloud"
            );
        }
        for _ in 0..4 {
            advance(&mut m, &mut delivery);
        }
    }
}

#[test]
fn cmb158_due_strike_uses_expiring_cover_before_support_timer_pruning() {
    use crate::{
        match_support::{FighterCover, SupportEvent, SupportKind},
        match_tick::TickEvent,
    };
    for covered in [false, true] {
        let (mut m, _, _) = scene();
        m.start.economy.players[1].mp = 5000.;
        let at = Point { x: 80., z: 80. };
        assert_eq!(m.admit_support(1, "dive", at, Some(0.)).unwrap(), None);
        let dt = m.rules.movement.tick;
        m.world
            .game
            .strikes
            .last_mut()
            .unwrap()
            .runtime
            .as_mut()
            .unwrap()
            .delay = dt / 2.;
        if covered {
            m.support.covers.push(FighterCover {
                team: 0,
                owner: 0,
                at,
                radius: 30.,
                remaining: dt / 2.,
            });
        }
        let mut delivery = DeliveryState::new(&m).unwrap();
        let report = advance(&mut m, &mut delivery);
        let downed = report.events.iter().any(|event| {
            matches!(
                event,
                TickEvent::Support(SupportEvent::ShotDown {
                    kind: SupportKind::Dive,
                    fighters: true,
                    ..
                })
            )
        });
        let bombed = report
            .events
            .iter()
            .any(|event| matches!(event, TickEvent::Support(SupportEvent::Bomb(_))));
        assert_eq!(downed, covered);
        assert_eq!(bombed, !covered);
        assert!(m.support.covers.is_empty());
        for _ in 0..4 {
            advance(&mut m, &mut delivery);
        }
    }
}

#[test]
fn cmb158_later_units_existing_claims_change_the_first_shooters_choice() {
    // Prototype sim.js:5337 reserves existing targets before any unit takes its turn.
    for claimed in [false, true] {
        let (mut m, shooter, nearby) = scene();
        relocate(&mut m, shooter, Point { x: 60., z: 60. });
        relocate(&mut m, nearby, Point { x: 74., z: 60. });
        m.world.game.units.get_mut(&nearby).unwrap().hp = 1.;
        let alternative = publish(&mut m, "rifle", 1, Point { x: 84., z: 60. });
        let later = publish(&mut m, "rifle", 0, Point { x: 58., z: 65. });
        assert!(later > shooter);
        if claimed {
            m.firing.get_mut(later).unwrap().target_id = nearby;
        }
        m.commands.units.get_mut(&shooter).unwrap().hold_fire = false;
        m.firing.get_mut(shooter).unwrap().cooldown = 100.;
        let mut delivery = DeliveryState::new(&m).unwrap();
        advance(&mut m, &mut delivery);
        assert_eq!(
            m.firing.get(shooter).unwrap().target_id,
            if claimed { alternative } else { nearby }
        );
        // The later unit clears its held target during its own turn. Its old
        // reservation still changed the earlier shooter's decision this tick.
        assert_eq!(m.firing.get(later).unwrap().target_id, 0);
        for _ in 0..4 {
            advance(&mut m, &mut delivery);
        }
    }
}

#[test]
fn cmb158_attack_move_halts_before_displacement_and_stationary_fire() {
    use crate::{match_firing::LaunchEvent, match_tick::TickEvent};
    for attack_move in [false, true] {
        let (mut m, shooter, target) = scene();
        let at = Point { x: 60., z: 60. };
        relocate(&mut m, shooter, at);
        relocate(&mut m, target, Point { x: 74., z: 60. });
        let goal = Point { x: 95., z: 60. };
        let u = m.world.game.units.get_mut(&shooter).unwrap();
        u.path = vec![goal];
        u.amove = attack_move.then_some(goal);
        u.still = Some(10.);
        m.commands.units.get_mut(&shooter).unwrap().hold_fire = false;
        let state = m.firing.get_mut(shooter).unwrap();
        state.target_id = target;
        state.cooldown = m.rules.movement.tick / 2.;
        state.retarget = 100.;
        let mut delivery = DeliveryState::new(&m).unwrap();
        let report = advance(&mut m, &mut delivery);
        let u = &m.world.game.units[&shooter];
        if attack_move {
            assert_eq!(u.at(), at);
            assert!(u.path.is_empty());
            assert!(u.still.unwrap() > 10.);
        } else {
            assert!(u.x > at.x);
            assert!(!u.path.is_empty());
            assert_eq!(u.still, Some(0.));
        }
        assert!(
            report.events.iter().any(|event| matches!(event,
                TickEvent::Firing(LaunchEvent::Flight { shooter: fired, .. }) if *fired == shooter
            )),
            "the ready weapon fires using the movement state established in this turn"
        );
        for _ in 0..4 {
            advance(&mut m, &mut delivery);
        }
    }
}

#[test]
fn cmb158_reaction_timer_opens_a_reverse_route_before_the_same_ticks_movement() {
    for ready in [false, true] {
        let (mut m, _, _) = scene();
        let at = Point { x: 60., z: 60. };
        let tank = publish(&mut m, "tank", 0, at);
        let dt = m.rules.movement.tick;
        let u = m.world.game.units.get_mut(&tank).unwrap();
        u.hp = 1.;
        u.rot = 0.;
        u.hit_from = Some(Point { x: 110., z: 60. });
        u.hold_pos = false;
        m.combat.get_mut(tank).unwrap().hit_at = 0.;
        m.commands.units.get_mut(&tank).unwrap().react = if ready { dt / 2. } else { dt * 2. };
        let mut delivery = DeliveryState::new(&m).unwrap();
        advance(&mut m, &mut delivery);
        let u = &m.world.game.units[&tank];
        if ready {
            assert!(
                u.x < at.x,
                "fresh reaction route moves the tank on this same turn"
            );
            assert!(u.reverse.is_some());
            assert!(m.commands.units[&tank].react > dt);
        } else {
            assert_eq!(u.at(), at);
            assert!(u.path.is_empty());
            assert!(u.reverse.is_none());
        }
        for _ in 0..4 {
            advance(&mut m, &mut delivery);
        }
    }
}

#[test]
fn cmb158_separation_changes_who_the_expiring_flight_blast_hits() {
    use crate::{catalog::Catalog, match_firing::AbilityBlast};
    for crowded in [false, true] {
        let (mut m, thrower, target) = scene();
        let at = Point { x: 80., z: 60. };
        let kind = m.world.game.units[&thrower].kind;
        let radius = Catalog::compiled()
            .unwrap()
            .unit(&m.rules.kinds[kind].name)
            .unwrap()
            .get("ab")
            .unwrap()
            .get("radius")
            .unwrap()
            .number()
            .unwrap();
        relocate(&mut m, thrower, Point { x: 66., z: 60. });
        let before = Point {
            x: at.x + radius + 0.05,
            z: at.z,
        };
        relocate(&mut m, target, before);
        m.world.game.units.get_mut(&target).unwrap().hold_pos = false;
        m.world.game.units.get_mut(&target).unwrap().hit_from = Some(Point { x: 66., z: 60. });
        m.combat.get_mut(target).unwrap().hit_at = 0.;
        m.commands.units.get_mut(&target).unwrap().react = 100.;
        if crowded {
            let neighbor = publish(
                &mut m,
                "rifle",
                1,
                Point {
                    x: before.x + 0.2,
                    z: at.z,
                },
            );
            m.world.game.units.get_mut(&neighbor).unwrap().hold_pos = true;
        }
        let hp = m.world.game.units[&target].hp;
        let blast = AbilityBlast::from_unit(kind, &m.rules).unwrap();
        m.launch_ability_flight(thrower, at, blast, Some(m.rules.movement.tick), None)
            .unwrap();
        drop(m.firing.take_events());
        let mut delivery = DeliveryState::new(&m).unwrap();
        advance(&mut m, &mut delivery);
        let u = &m.world.game.units[&target];
        if crowded {
            assert!(
                u.x < at.x + radius,
                "crowd separation must cross the blast boundary"
            );
            assert!(u.hp < hp, "flight resolution must use the separated pose");
        } else {
            assert_eq!(u.at(), before);
            assert_eq!(u.hp, hp);
        }
        for _ in 0..4 {
            advance(&mut m, &mut delivery);
        }
    }
}

#[test]
fn cmb158_medic_healing_crosses_a_model_boundary_before_paid_reinforcement() {
    use crate::catalog::Catalog;
    for medic in [false, true] {
        let (mut m, _, patient) = scene();
        let home = m.start.players[1].spawn;
        relocate(
            &mut m,
            patient,
            Point {
                x: home.x - 5.,
                z: home.z,
            },
        );
        if medic {
            publish(
                &mut m,
                "medic",
                1,
                Point {
                    x: home.x - 5.,
                    z: home.z - 3.,
                },
            );
        }
        let kind = m.world.game.units[&patient].kind;
        let hp_per = Catalog::compiled()
            .unwrap()
            .unit(&m.rules.kinds[kind].name)
            .unwrap()
            .get("hpPer")
            .unwrap()
            .number()
            .unwrap();
        m.world.game.units.get_mut(&patient).unwrap().hp = hp_per - 0.001;
        m.recovery.get_mut(patient).unwrap().reinforcement = 100.;
        m.initialize_tick_scene(9, Default::default()).unwrap();
        let mut delivery = DeliveryState::new(&m).unwrap();
        advance(&mut m, &mut delivery);
        let expected = hp_per * if medic { 3. } else { 2. };
        assert!(
            (m.world.game.units[&patient].hp - expected).abs() < 1e-12,
            "reinforcement counts living models after the medic pass"
        );
        assert_eq!(
            m.recovery.get(patient).unwrap().heal_at,
            medic.then_some(10)
        );
        assert_eq!(m.recovery.get(patient).unwrap().reinforcement, 0.);
        for _ in 0..4 {
            advance(&mut m, &mut delivery);
        }
    }
}

#[test]
fn cmb158_stationary_hull_faces_its_live_target_while_moving_fire_keeps_the_route_heading() {
    use crate::{match_firing::LaunchEvent, match_tick::TickEvent};
    for moving in [false, true] {
        let (mut m, _, _) = scene();
        let at = Point { x: 60., z: 60. };
        let tank = publish(&mut m, "tank", 0, at);
        let target = publish(&mut m, "tank", 1, Point { x: 60., z: 85. });
        let u = m.world.game.units.get_mut(&tank).unwrap();
        u.rot = 0.;
        u.still = Some(10.);
        u.hold_pos = true;
        if moving {
            u.path = vec![Point { x: 95., z: 60. }];
        }
        let state = m.firing.get_mut(tank).unwrap();
        state.target_id = target;
        state.cooldown = 0.;
        state.retarget = 100.;
        m.commands.units.get_mut(&tank).unwrap().hold_fire = false;
        let mut delivery = DeliveryState::new(&m).unwrap();
        let report = advance(&mut m, &mut delivery);
        let u = &m.world.game.units[&tank];
        if moving {
            assert!(u.x > at.x);
            assert!(u.rot.abs() < 1e-12);
        } else {
            assert_eq!(u.at(), at);
            assert!(
                u.rot > 0.,
                "the stationary hull turns toward its visible target"
            );
        }
        assert!(report.events.iter().any(|event| matches!(event,
            TickEvent::Firing(LaunchEvent::Flight { shooter, .. }) if *shooter == tank
        )));
        for _ in 0..4 {
            advance(&mut m, &mut delivery);
        }
    }
}

#[test]
fn cmb158_flight_damage_disables_flak_before_a_due_support_strike() {
    use crate::{match_firing::AbilityBlast, match_support::SupportEvent, match_tick::TickEvent};
    for lethal in [false, true] {
        let (mut m, thrower, _) = scene();
        m.start.economy.players[1].mp = 5000.;
        let at = Point { x: 80., z: 60. };
        let flak = publish(&mut m, "flak", 0, at);
        let u = m.world.game.units.get_mut(&flak).unwrap();
        u.hp = 1.;
        u.still = Some(10.);
        u.hold_pos = true;
        m.commands.units.get_mut(&flak).unwrap().hold_fire = false;
        assert_eq!(m.admit_support(1, "dive", at, Some(0.)).unwrap(), None);
        let dt = m.rules.movement.tick;
        m.world
            .game
            .strikes
            .last_mut()
            .unwrap()
            .runtime
            .as_mut()
            .unwrap()
            .delay = dt / 2.;
        if lethal {
            relocate(&mut m, thrower, Point { x: 66., z: 60. });
            let blast =
                AbilityBlast::from_unit(m.world.game.units[&thrower].kind, &m.rules).unwrap();
            m.launch_ability_flight(thrower, at, blast, Some(dt), None)
                .unwrap();
            drop(m.firing.take_events());
        }
        let mut delivery = DeliveryState::new(&m).unwrap();
        let report = advance(&mut m, &mut delivery);
        let attempted = report.events.iter().any(|event| {
            matches!(event,
                TickEvent::Support(SupportEvent::Flak { unit, .. }) if *unit == flak
            )
        });
        assert_eq!(
            attempted, !lethal,
            "flak eligibility reads the preceding flight damage"
        );
        if lethal {
            assert!(report.retired.iter().any(|unit| unit.unit.id == flak));
            assert!(
                report
                    .events
                    .iter()
                    .any(|event| matches!(event, TickEvent::Support(SupportEvent::Bomb(_))))
            );
        }
        for _ in 0..4 {
            advance(&mut m, &mut delivery);
        }
    }
}

#[test]
fn cmb158_on_map_anti_air_damages_the_moved_plane_and_delivers_its_event() {
    use crate::{
        game::PlaneState,
        match_aviation::{AviationEvent, MissionKind, PlaneMission},
        match_tick::TickEvent,
    };
    for held in [false, true] {
        let (mut m, _, _) = scene();
        let gun = publish(&mut m, "flak", 0, Point { x: 60., z: 60. });
        let u = m.world.game.units.get_mut(&gun).unwrap();
        u.hold_pos = true;
        u.still = Some(10.);
        m.commands.units.get_mut(&gun).unwrap().hold_fire = held;
        let plane = m.spawn_aircraft(1, "attacker").unwrap();
        let before = Point { x: 80., z: 60. };
        let u = m.world.game.units.get_mut(&plane).unwrap();
        u.x = before.x;
        u.z = before.z;
        u.air = Some(PlaneState::Out);
        let hp = u.hp;
        m.aviation.planes.get_mut(&plane).unwrap().mission = Some(PlaneMission {
            kind: MissionKind::Patrol,
            target: None,
            at: before,
        });
        m.commands.units.get_mut(&plane).unwrap().hold_fire = true;
        m.grid.rebuild(&m.world.game.units);
        let mut delivery = DeliveryState::new(&m).unwrap();
        let report = advance(&mut m, &mut delivery);
        let after = m.world.game.units[&plane].at();
        assert_ne!(
            after, before,
            "aircraft movement must run before the late AA pass"
        );
        let event = report.events.iter().find_map(|event| match event {
            TickEvent::Aviation(AviationEvent::AntiAir {
                shooter,
                target,
                at,
                ..
            }) if *shooter == gun && *target == plane => Some(*at),
            _ => None,
        });
        if held {
            assert_eq!(m.world.game.units[&plane].hp, hp);
            assert_eq!(event, None);
        } else {
            assert!(m.world.game.units[&plane].hp < hp);
            assert_eq!(
                event,
                Some(after),
                "the AA event uses this tick's moved aircraft pose"
            );
            assert!(m.combat.get(gun).unwrap().xp > 0.);
        }
        for _ in 0..4 {
            advance(&mut m, &mut delivery);
        }
    }
}
