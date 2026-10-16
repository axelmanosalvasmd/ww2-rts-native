//! Native combat command, squad layout and snapshot checks.
use super::{command, p, scene};
use crate::{game::PlaneState, match_commands::Denial};
use serde_json::json;

#[test]
fn cmb143_escort_assigns_friendly_ground_mission_and_refuses_other_targets() {
    use crate::match_aviation::MissionKind;
    let mut m = scene(&[
        ("rifle", 0, 60., 40.),
        ("rifle", 1, 80., 40.),
        ("bunker", 0, 80., 80.),
    ]);
    // Publish the fighter through the canonical aviation owner.
    let plane = m.spawn_aircraft(0, "fighter").unwrap();
    m.commands.units.get_mut(&plane).unwrap().hold_fire = true;
    assert_eq!(
        command(&mut m, json!({"t":"escort","ids":[plane],"target":1})),
        None
    );
    let mission = m.aviation.planes[&plane].mission.unwrap();
    assert_eq!(mission.kind, MissionKind::Escort);
    assert_eq!(mission.target, Some(1));
    assert_eq!(mission.at, p(60., 40.));
    assert_eq!(m.world.game.units[&plane].air, Some(PlaneState::Out));
    for target in [i64::from(plane), 2, 3, 99] {
        assert_eq!(
            command(&mut m, json!({"t":"escort","ids":[plane],"target":target})),
            Some(Denial::Unseen)
        );
        assert_eq!(m.aviation.planes[&plane].mission, Some(mission));
    }
    assert_eq!(
        command(&mut m, json!({"t":"escort","ids":[1],"target":1})),
        Some(Denial::Needs)
    );
}

#[test]
fn cmb153_all_infantry_slot_counts_health_thinning_and_velocity_lag() {
    use crate::squad_men::drawn_men;
    for (kind, slots) in [
        ("rifle", 15),
        ("engineer", 6),
        ("ranger", 12),
        ("commando", 15),
        ("conscript", 21),
        ("mg", 3),
        ("mortar", 3),
        ("at", 4),
        ("flak", 3),
        ("howitzer", 4),
        ("sniper", 2),
        ("medic", 2),
        ("flamer", 3),
    ] {
        let full = drawn_men(kind, 100., 100., p(40., 40.), 0., p(0., 0.)).unwrap();
        assert_eq!(full.positions.len(), slots, "{kind}");
        assert_eq!(full.count, slots as f64);
        let half = drawn_men(kind, 50., 100., p(40., 40.), 0., p(0., 0.)).unwrap();
        assert_eq!(half.count, (slots as f64 / 2.).ceil());
        assert_eq!(half.positions, full.positions[..half.positions.len()]);
        let moving = drawn_men(kind, 100., 100., p(40., 40.), 0., p(9., 18.)).unwrap();
        for (still, moving) in full.positions.iter().zip(moving.positions) {
            assert_eq!(moving.x, still.x - 1.);
            assert_eq!(moving.z, still.z - 2.);
        }
        assert!(
            drawn_men(kind, 0., 100., p(40., 40.), 0., p(0., 0.))
                .unwrap()
                .positions
                .is_empty()
        );
    }
}

#[test]
fn cmb157_native_unit_row_fields_rounding_combat_flags_and_owner_values() {
    use crate::recipient_snapshot::SnapshotCache;
    let mut m = scene(&[("rifle", 0, 40., 40.), ("rifle", 1, 60., 40.)]);
    let u = m.world.game.units.get_mut(&1).unwrap();
    u.hp = 54.2;
    u.supp = 17.6;
    u.retreating = true;
    u.amove = Some(p(80., 40.));
    let control = m.commands.units.get_mut(&1).unwrap();
    control.aim = 0.26;
    control.hold_fire = true;
    control.auto_cast = true;
    let ability = m.abilities.get_mut(1).unwrap();
    ability.cooldown = 1.1;
    ability.buff = 2.;
    ability.ap = true;
    m.recovery.get_mut(1).unwrap().heal_at = Some(m.world.game.tick);
    m.firing.get_mut(1).unwrap().target_id = 2;
    let capture = SnapshotCache::new(&m).unwrap();
    let own = capture.recipient(0).unwrap();
    let row = own.fields["units"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r[0] == 1)
        .unwrap();
    assert_eq!(row.as_array().unwrap().len(), 15);
    assert_eq!(row[6], 0.3);
    assert_eq!(row[7].as_f64(), Some(55.));
    assert_eq!(row[8].as_f64(), Some(18.));
    assert_eq!(row[9], 2);
    assert_eq!(row[10], 0);
    assert_eq!(row[11].as_f64(), Some(2.));
    let expected = 1 | 2 | 4 | 8 | 64 | 2048 | 16384;
    assert_eq!(row[12].as_u64().unwrap() & expected, expected);
    assert_eq!(row[13], 0);
    assert_eq!(row[14], 1.);
}

#[test]
fn cmb160_armor_flank_front_and_explicit_launch_end_are_deterministic() {
    use crate::{
        combat::armor_mul,
        projectiles::{Point3, launch_solution},
    };
    let at = p(40., 40.);
    assert_eq!(armor_mul("tiger", at, 0., p(50., 40.)).unwrap(), 0.7);
    assert_eq!(armor_mul("tiger", at, 0., p(30., 40.)).unwrap(), 2.);
    assert_eq!(armor_mul("tiger", at, 0., p(40., 50.)).unwrap(), 1.);
    let from = Point3::new(40., 1.2, 40.);
    let end = Point3::new(50., 0., 40.);
    let launch = launch_solution(from, end, 0., 9.81, Some(1.2)).unwrap();
    assert_eq!(launch.duration, 1.2);
    assert_eq!(
        launch,
        launch_solution(from, end, 0., 9.81, Some(1.2)).unwrap()
    );
    assert!(launch.velocity.x > 0.);
    assert_eq!(launch.velocity.z, 0.);
}

#[test]
fn aircraft_projectile_contact_preserves_vehicle_hit_memory_and_idle_route() {
    use crate::match_firing::LaunchEvent;

    let mut m = scene(&[("tank", 0, 60., 40.), ("rifle", 1, 90., 40.)]);
    let plane = m.spawn_aircraft(1, "attacker").unwrap();
    let aircraft = m.world.game.units.get_mut(&plane).unwrap();
    aircraft.air = Some(PlaneState::Station);
    aircraft.x = 40.;
    aircraft.z = 40.;
    m.grid.rebuild(&m.world.game.units);
    let old_from = Some(p(90., 40.));
    let tank = m.world.game.units.get_mut(&1).unwrap();
    tank.hp = 150.;
    // Face the incoming round so the rear-hit multiplier cannot kill the witness.
    tank.rot = std::f64::consts::PI;
    tank.hit_from = old_from;
    let old_tick = -1e9;
    let memory = m.combat.get_mut(1).unwrap();
    memory.hit_by = Some(2);
    memory.hit_at = old_tick;
    let control = m.commands.units.get_mut(&1).unwrap();
    control.hold_fire = true;
    control.react = 0.;
    assert!(m.world.game.units[&1].path.is_empty());

    // Use the seeded weapon launches and actual swept contact, including misses.
    let mut hit = false;
    for _ in 0..8 {
        m.fire_weapon(plane, 1, false).unwrap();
        assert!(!m.firing.flights.is_empty());
        for _ in 0..80 {
            if m.firing.flights.is_empty() {
                break;
            }
            m.capture_firing_prior().unwrap();
            m.world.game.tick += 1;
            m.step_firing_flights().unwrap();
        }
        assert!(m.firing.flights.is_empty());
        hit = m.firing.take_events().iter().any(|event| {
            matches!(event, LaunchEvent::Contact(contact)
                if contact.shooter == plane && contact.target == Some(1) && contact.hit)
        });
        if hit {
            break;
        }
    }
    assert!(hit, "the aircraft projectile must contact the tank");
    assert!(m.world.game.units[&1].hp > 0.);
    assert!(m.world.game.units[&1].hp < 150.);
    assert_eq!(m.combat.get(1).unwrap().hit_by, Some(2));
    assert_eq!(m.combat.get(1).unwrap().hit_at, old_tick);
    assert_eq!(m.world.game.units[&1].hit_from, old_from);

    m.react_unit(1).unwrap();
    let tank = &m.world.game.units[&1];
    assert!(
        tank.path.is_empty(),
        "aircraft fire must not cause an idle pullback"
    );
    assert_eq!(tank.reverse, None);
    assert!(!tank.retreating);
    assert_eq!(m.commands.units[&1].react, 0.);
}
