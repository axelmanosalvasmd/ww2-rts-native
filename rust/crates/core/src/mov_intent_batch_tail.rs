//! Recon, strafing and naval navigation intent at reference e4e579a.
use super::*;
use crate::match_support::SupportEvent;

fn strikes(m: &mut Match, n: usize) -> Vec<SupportEvent> {
    let mut events = Vec::new();
    for _ in 0..n {
        m.world.game.tick += 1;
        let ids = m.world.game.units.keys().copied().collect::<Vec<_>>();
        m.step_support_strikes(&ids, 0.05).unwrap();
        events.extend(m.support.take_events());
    }
    events
}
fn strip_point(direction: f64, along: f64, side: f64) -> Point {
    p(
        80.0 + math::cos(direction) * along - math::sin(direction) * side,
        80.0 + math::sin(direction) * along + math::cos(direction) * side,
    )
}
fn vision(m: &mut Match) {
    crate::vision::update_vision(&mut m.world.game, &m.rules, &m.grid);
}
fn fog_cells(m: &Match, team: i32) -> Vec<u8> {
    crate::fog::Fog::default()
        .team(&m.world.game, &m.rules, team)
        .vis
        .clone()
}

// MOV-95: shared/sim.js:400,4600-4605,5269,5300.
#[test]
fn mov_95_recon_delayed_rotated_unit_and_terrain_strip_is_shared_and_expires() {
    for direction in [std::f64::consts::FRAC_PI_2, std::f64::consts::FRAC_PI_4] {
        let mut m = scene();
        m.world.game.reveal = false;
        // A teammate without observers must receive the caller's recon view.
        m.world.game.players.push(Player {
            slot: 2,
            team: 0,
            visible: vec![],
        });
        let teammate = m.start.players[0].clone();
        m.start.players.push(teammate);
        let economy = m.start.economy.players[0].clone();
        m.start.economy.players.push(economy);
        m.story.push(Default::default());
        let inside = [(39.0, 0.0), (-39.0, 0.0), (0.0, 14.0), (0.0, -14.0)]
            .map(|(along, side)| add(&mut m, "rifle", 1, strip_point(direction, along, side)));
        let outside = [(40.5, 0.0), (-40.5, 0.0), (0.0, 15.5), (0.0, -15.5)]
            .map(|(along, side)| add(&mut m, "rifle", 1, strip_point(direction, along, side)));
        vision(&mut m);
        assert!(m.world.game.players[0].visible.is_empty());
        assert!(m.world.game.players[2].visible.is_empty());
        let enemy_fog = fog_cells(&m, 1);
        assert!(fog_cells(&m, 0).iter().all(|cell| *cell == 0));
        assert_eq!(
            m.admit_support(0, "recon", p(80.0, 80.0), Some(direction))
                .unwrap(),
            None
        );
        assert!(strikes(&mut m, 59).is_empty());
        assert!(!m.world.game.strikes[0].live);
        vision(&mut m);
        assert!(m.world.game.players[0].visible.is_empty());
        assert!(fog_cells(&m, 0).iter().all(|cell| *cell == 0));

        strikes(&mut m, 2);
        assert!(m.world.game.strikes[0].live);
        vision(&mut m);
        for slot in [0, 2] {
            for id in inside {
                assert!(m.world.game.players[slot].sees(id));
            }
            for id in outside {
                assert!(!m.world.game.players[slot].sees(id));
            }
        }
        assert_eq!(
            m.world.game.players[0].visible,
            m.world.game.players[2].visible
        );
        assert_eq!(fog_cells(&m, 1), enemy_fog);
        let revealed = fog_cells(&m, 0);
        // Compare every terrain-cell centre to the specified 80 by 30 m rectangle.
        for (cell, visible) in revealed.iter().enumerate() {
            let dx = (cell % 80) as f64 * 2.0 + 1.0 - 80.0;
            let dz = (cell / 80) as f64 * 2.0 + 1.0 - 80.0;
            let along = dx * math::cos(direction) + dz * math::sin(direction);
            let side = -dx * math::sin(direction) + dz * math::cos(direction);
            assert_eq!(
                *visible != 0,
                along.abs() <= 40.0 && side.abs() <= 15.0,
                "cell {cell}"
            );
        }
        // The flight remains active just before 15 seconds, then leaves no live vision.
        strikes(&mut m, 297);
        assert!(m.world.game.strikes[0].live);
        vision(&mut m);
        assert_eq!(fog_cells(&m, 0), revealed);
        strikes(&mut m, 3);
        assert!(m.world.game.strikes.is_empty());
        vision(&mut m);
        assert!(m.world.game.players[0].visible.is_empty());
        assert!(m.world.game.players[2].visible.is_empty());
        assert!(fog_cells(&m, 0).iter().all(|cell| *cell == 0));
        assert_eq!(fog_cells(&m, 1), enemy_fog);
    }
}

// MOV-97: shared/sim.js:402,5292-5297.
#[test]
fn mov_97_rotated_strafe_arrives_once_with_friendly_damage_and_exclusions() {
    for direction in [std::f64::consts::FRAC_PI_2, std::f64::consts::FRAC_PI_4] {
        let mut m = scene();
        let friend = add(&mut m, "rifle", 0, strip_point(direction, -17.5, -3.5));
        let foe = add(&mut m, "rifle", 1, strip_point(direction, 17.5, 3.5));
        let carrier = add(&mut m, "halftrack", 0, p(80.0, 80.0));
        let vehicle = add(&mut m, "tank", 1, strip_point(direction, 10.0, -3.5));
        let rider = add(&mut m, "rifle", 0, p(80.5, 80.0));
        command(
            &mut m,
            Command::Board {
                ids: vec![rider.into()],
                target: Some(carrier.into()),
            },
        );
        assert_eq!(
            m.apply_unit_boarding(rider).unwrap(),
            crate::match_carriers::BoardingOutcome::Boarded { carrier }
        );
        assert!(m.world.game.units[&rider].riding);
        let air = m.spawn_aircraft(1, "fighter").unwrap();
        m.commands.units.get_mut(&air).unwrap().hold_fire = true;
        let u = m.world.game.units.get_mut(&air).unwrap();
        u.x = 80.0;
        u.z = 80.0;
        u.air = Some(PlaneState::Station);
        m.grid.update(air, u);
        let outside = [(18.1, 0.0), (-18.1, 0.0), (0.0, 4.1), (0.0, -4.1)]
            .map(|(along, side)| add(&mut m, "rifle", 1, strip_point(direction, along, side)));
        let before = m
            .world
            .game
            .units
            .iter()
            .map(|(id, u)| (*id, (u.hp, u.supp)))
            .collect::<std::collections::BTreeMap<_, _>>();
        assert_eq!(
            m.admit_support(0, "strafe", p(80.0, 80.0), Some(direction))
                .unwrap(),
            None
        );
        assert!(strikes(&mut m, 99).is_empty());
        for (id, (hp, supp)) in &before {
            assert_eq!(
                (m.world.game.units[id].hp, m.world.game.units[id].supp),
                (*hp, *supp)
            );
        }
        let events = strikes(&mut m, 2);
        let hit = [friend, foe, carrier, vehicle];
        for id in hit {
            let infantry = m.rules.kinds[m.world.game.units[&id].kind].infantry;
            assert_eq!(
                m.world.game.units[&id].hp,
                before[&id].0 - if infantry { 25.0 } else { 10.0 }
            );
            assert_eq!(
                m.world.game.units[&id].supp,
                if infantry { 80.0 } else { before[&id].1 }
            );
            assert_eq!(events.iter().filter(|event| matches!(event, SupportEvent::Environment(crate::environment::Effect::Hurt(hurt)) if hurt.unit == id)).count(), 1);
        }
        for id in outside.into_iter().chain([air, rider]) {
            assert_eq!(
                (m.world.game.units[&id].hp, m.world.game.units[&id].supp),
                before[&id]
            );
        }
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event, SupportEvent::Plane { .. }))
                .count(),
            1
        );
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(
                    event,
                    SupportEvent::Environment(crate::environment::Effect::Hurt(_))
                ))
                .count(),
            hit.len()
        );
        assert!(m.world.game.strikes.is_empty());
        let after = m
            .world
            .game
            .units
            .iter()
            .map(|(id, u)| (*id, (u.hp, u.supp)))
            .collect::<std::collections::BTreeMap<_, _>>();
        assert!(strikes(&mut m, 20).is_empty());
        for (id, state) in after {
            assert_eq!(
                (m.world.game.units[&id].hp, m.world.game.units[&id].supp),
                state
            );
        }
    }
}

fn naval_world(rows: Vec<String>, enabled: bool) -> World {
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"Native naval MOV", "w":80, "h":80, "rows":rows, "naval":enabled,
            "spawns":[{"x":2,"y":2},{"x":77,"y":77}], "points":[{"x":2,"y":2}]
        }))
        .unwrap(),
    )
    .unwrap();
    World::from_map(&map).unwrap()
}
// MOV-103: shared/sim.js:171-180,740,2665-2678.
#[test]
fn mov_103_naval_shoals_are_ten_cardinal_cells_fixed_at_start_and_route_deep_ships() {
    use crate::game::{LAND, SHOAL};
    let mut cells = vec![vec![b'W'; 80]; 80];
    for (x, z) in [(2, 2), (77, 77), (40, 40)] {
        cells[z][x] = b'.';
    }
    let rows = cells
        .iter()
        .map(|row| String::from_utf8(row.clone()).unwrap())
        .collect::<Vec<_>>();
    let ordinary = naval_world(rows.clone(), false);
    assert!(!ordinary.game.naval);
    assert!(ordinary.shoal.is_none());
    assert!(ordinary.game.flags.iter().all(|flags| flags & SHOAL == 0));
    let mut world = naval_world(rows, true);
    assert!(world.game.naval);
    assert!(world.shoal.is_some());
    assert_ne!(world.game.flags[40 * 80 + 40] & LAND, 0);
    for dz in -11_i32..=11 {
        for dx in -11_i32..=11 {
            let cell = ((40 + dz) * 80 + 40 + dx) as usize;
            assert_eq!(
                world.game.flags[cell] & SHOAL != 0,
                dx.abs() + dz.abs() <= 10,
                "offset {dx},{dz}"
            );
            if dx != 0 || dz != 0 {
                assert_eq!(world.game.flags[cell] & LAND, 0);
            }
        }
    }
    let old_shoals = world.shoal.clone();
    let old_flags = world
        .game
        .flags
        .iter()
        .map(|flags| flags & SHOAL)
        .collect::<Vec<_>>();
    // Move the only central shore far enough to reverse both witness cells in a fresh map.
    world.set_cell(40 * 80 + 40, b'W').unwrap();
    world.set_cell(40 * 80 + 55, b'.').unwrap();
    cells[40][40] = b'W';
    cells[40][55] = b'.';
    let fresh = naval_world(
        cells
            .iter()
            .map(|row| String::from_utf8(row.clone()).unwrap())
            .collect(),
        true,
    );
    assert_eq!(world.shoal, old_shoals);
    assert_eq!(
        world
            .game
            .flags
            .iter()
            .map(|flags| flags & SHOAL)
            .collect::<Vec<_>>(),
        old_flags
    );
    for (cell, retained, recalculated) in [(40 * 80 + 39, true, false), (40 * 80 + 56, false, true)]
    {
        assert_eq!(world.game.flags[cell] & SHOAL != 0, retained);
        assert_eq!(fresh.game.flags[cell] & SHOAL != 0, recalculated);
    }

    // Two deep basins have only a four-cell-wide water connection.
    let mut cells = vec![vec![b'.'; 80]; 80];
    for row in &mut cells[10..70] {
        row[10..70].fill(b'W');
    }
    for row in &mut cells[38..42] {
        row[10..38].fill(b'.');
        row[42..70].fill(b'.');
    }
    let world = naval_world(
        cells
            .iter()
            .map(|row| String::from_utf8(row.clone()).unwrap())
            .collect(),
        true,
    );
    for cell in [25 * 80 + 40, 55 * 80 + 40] {
        assert_eq!(world.game.flags[cell] & (LAND | SHOAL), 0);
    }
    assert_ne!(world.game.flags[40 * 80 + 40] & SHOAL, 0);
    let n = 80 * 80;
    let walls = vec![0; n];
    let absent = vec![false; n];
    let rules = crate::rules::Rules::compiled().unwrap();
    for (name, mask, status) in [
        ("gunboat", LAND, crate::pathfinding::PathStatus::Found),
        (
            "destroyer",
            LAND | SHOAL,
            crate::pathfinding::PathStatus::Unreachable,
        ),
    ] {
        let profile = &rules.kinds[rules.kind(name).unwrap()];
        assert_eq!(profile.block(), mask);
        let result = crate::pathfinding::Pathfinding::new()
            .find_path(
                crate::pathfinding::PathTerrain {
                    navigation: crate::match_runtime::terrain_view(&world.game),
                    near_walls: &walls,
                    known_mines: &absent,
                    fires: &absent,
                    bunkers: &[],
                    wear: world.game.wear.as_deref(),
                    wetness: 0.0,
                    roads: false,
                },
                crate::pathfinding::PathRules::compiled().unwrap(),
                profile,
                p(81.0, 51.0),
                p(81.0, 111.0),
                None,
            )
            .unwrap();
        assert_eq!(result.status, status, "{name}");
        if status == crate::pathfinding::PathStatus::Found {
            assert_eq!(result.waypoints.last(), Some(&p(81.0, 111.0)));
        } else {
            assert!(result.waypoints.is_empty());
        }
    }
}
