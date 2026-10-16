//! Native reactions from actual damage contacts, with explicit component visibility inputs.
use super::*;
use crate::{
    combat::{Impact, WeaponDamage},
    match_combat::{ContactTarget, FlightSource},
    mov_audit_test_support::{close, point, scene, spawn},
    vehicle::{angle_delta, turn_angle},
};

fn hit(m: &mut Match, target: u32, shooter: u32, scale: f64) {
    let source = &m.world.game.units[&shooter];
    let origin = source.at();
    let kind = source.kind;
    let owner = source.owner as i32;
    let at = m.world.game.units[&target].at();
    let impact = Impact {
        owner,
        shooter,
        weapon: WeaponDamage::from_catalog(&m.rules.kinds[kind].name).unwrap(),
        damage_scale: Some(scale),
        energy: 1.,
        armor_piercing: false,
        ap_naval_only: false,
        ap_multiplier: 1.,
        launch_origin: origin,
        at,
        direction: point(at.x - origin.x, at.z - origin.z),
    };
    let ids = m.world.game.units.keys().copied().collect::<Vec<_>>();
    let (mut damage, firing) = m.firing_damage_view(&ids);
    damage
        .local_contact(
            FlightSource {
                shooter,
                kind,
                owner,
                airborne: false,
            },
            ContactTarget::Unit(target),
            impact,
            "mov-native-contact",
            None,
            firing,
        )
        .unwrap();
}

#[test]
fn mov25_stopped_hull_prioritizes_visible_antitank_then_recent_fire_then_target_then_order() {
    for (danger, visible, recent) in [
        (true, true, true),
        (false, true, true),
        (true, false, true),
        (false, true, false),
    ] {
        let mut m = scene();
        let tank = spawn(&mut m, "tank", 0, point(60., 60.));
        let target = spawn(
            &mut m,
            if danger { "at" } else { "rifle" },
            1,
            point(60., 85.),
        );
        let shooter = spawn(&mut m, "at", 1, point(85., 60.));
        hit(&mut m, tank, shooter, 0.01);
        if !recent {
            m.world.game.tick += 101;
        }
        if visible {
            m.world.game.players[0].visible.push(target);
        }
        m.set_firing_target(tank, target).unwrap();
        let before = m.world.game.units[&tank].rot;
        let dt = m.rules.movement.tick;
        let rate = m.rules.kinds[m.world.game.units[&tank].kind]
            .vehicle
            .as_ref()
            .unwrap()
            .hull_turn;
        let desired = if danger && visible {
            crate::math::atan2(1., 0.)
        } else if recent {
            0.
        } else {
            crate::math::atan2(1., 0.)
        };
        m.apply_unit_firing(tank, false, dt).unwrap();
        close(
            m.world.game.units[&tank].rot,
            turn_angle(before, desired, rate * dt),
        );
    }
    let mut m = scene();
    let tank = spawn(&mut m, "tank", 0, point(60., 60.));
    let dt = m.rules.movement.tick;
    m.commands.units.get_mut(&tank).unwrap().hold_fire = true;
    m.commands.units.get_mut(&tank).unwrap().face = Some(crate::match_commands::Facing {
        angle: 1.,
        at: point(60., 60.),
    });
    m.apply_unit_firing(tank, false, dt).unwrap();
    let rate = m.rules.kinds[m.world.game.units[&tank].kind]
        .vehicle
        .as_ref()
        .unwrap()
        .hull_turn;
    close(m.world.game.units[&tank].rot, rate * dt);
    // A clear initial hull may not turn through an adjacent wall. This input is authored terrain.
    let mut blocked = false;
    for x in 27..34 {
        for y in 27..34 {
            let mut candidate = scene();
            let id = spawn(&mut candidate, "tank", 0, point(60., 60.));
            let kind = candidate.rules.kinds[candidate.world.game.units[&id].kind].clone();
            let u = &candidate.world.game.units[&id];
            let before = u.pose();
            let next = crate::game::Pose {
                rot: turn_angle(before.rot, 1., kind.vehicle.as_ref().unwrap().hull_turn),
                ..before
            };
            candidate.world.game.flags[y * 64 + x] = crate::game::MOVE;
            if crate::vehicle::swept_vehicle_clear(
                &candidate.world.game,
                before,
                before,
                &kind,
                kind.block(),
            ) && !crate::vehicle::swept_vehicle_clear(
                &candidate.world.game,
                before,
                next,
                &kind,
                kind.block(),
            ) {
                candidate.turn_firing_hull(id, 1., 1.).unwrap();
                close(candidate.world.game.units[&id].rot, before.rot);
                blocked = true;
                break;
            }
        }
        if blocked {
            break;
        }
    }
    assert!(blocked, "wall refusal branch must be exercised");
}

#[test]
fn mov26_hurt_out_of_range_vehicle_backs_front_first_except_at_capture_point() {
    for capture in [false, true] {
        let mut m = scene();
        let at = if capture {
            m.start.points[0].at
        } else {
            point(35., 40.)
        };
        let tank = spawn(&mut m, "tank", 0, at);
        let shooter = spawn(&mut m, "at", 1, point(119., at.z));
        let full = m.world.game.units[&tank].hp;
        for _ in 0..100 {
            if m.world.game.units[&tank].hp < full * 0.5 {
                break;
            }
            hit(&mut m, tank, shooter, 0.1);
        }
        assert!(m.world.game.units[&tank].hp > 0. && m.world.game.units[&tank].hp < full * 0.5);
        assert_eq!(m.world.game.units[&tank].hit_from, Some(point(119., at.z)));
        m.react_unit(tank).unwrap();
        let u = &m.world.game.units[&tank];
        if capture {
            assert!(u.path.is_empty());
            assert!(u.reverse.is_none());
            continue;
        }
        let end = *u.path.last().expect("back-off has a native route");
        close(crate::math::hypot(end.x - at.x, end.z - at.z), 10.);
        assert!(end.x < at.x);
        assert_eq!(u.reverse, Some(end));
        close(m.commands.units[&tank].react, 10.);
        let original = u.rot;
        let before = u.at();
        m.begin_path_tick(4096, 20).unwrap();
        for _ in 0..20 {
            m.move_unit_with_command_routes(tank, 1.).unwrap();
        }
        let u = &m.world.game.units[&tank];
        assert!(u.x < before.x);
        assert!(angle_delta(original, u.rot).abs() < 0.01);
        let generation = u.path_gen;
        m.decrement_reaction_timer(tank, 1.).unwrap();
        m.react_unit(tank).unwrap();
        assert_eq!(m.world.game.units[&tank].path_gen, generation);
        close(m.commands.units[&tank].react, 9.);
        // Keep the unit idle so the timer, rather than an active path, prevents another reaction.
        let u = m.world.game.units.get_mut(&tank).unwrap();
        u.path.clear();
        u.world_goal = None;
        u.reverse = None;
        hit(&mut m, tank, shooter, 0.001);
        m.react_unit(tank).unwrap();
        assert!(m.world.game.units[&tank].path.is_empty());
        m.decrement_reaction_timer(tank, 9.).unwrap();
        m.react_unit(tank).unwrap();
        assert!(!m.world.game.units[&tank].path.is_empty());
        close(m.commands.units[&tank].react, 10.);
    }
    for (hurt, can_answer) in [(false, false), (true, true)] {
        let mut m = scene();
        let tank = spawn(&mut m, "tank", 0, point(35., 40.));
        let shooter = spawn(
            &mut m,
            "at",
            1,
            point(if can_answer { 55. } else { 119. }, 40.),
        );
        let full = m.world.game.units[&tank].hp;
        hit(&mut m, tank, shooter, 0.01);
        if hurt {
            m.world.game.units.get_mut(&tank).unwrap().hp = full * 0.4;
        }
        if can_answer {
            m.world.game.players[0].visible.push(shooter);
            assert!(m.can_shoot_target(tank, shooter).unwrap());
        } else {
            assert!(m.world.game.units[&tank].hp >= full * 0.5);
        }
        m.react_unit(tank).unwrap();
        assert!(m.world.game.units[&tank].path.is_empty());
        assert!(m.world.game.units[&tank].reverse.is_none());
    }
}
