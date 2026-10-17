use super::*;
use crate::{
    game::{CELL, Point, SIGHT, Smoke},
    grid::SpatialGrid,
    mov_audit_test_support as scene, vision,
};

fn assert_same_geometry(a: &Game, b: &Game) {
    assert_eq!((a.w, a.h, a.weather), (b.w, b.h, b.weather));
    assert_eq!(a.flags, b.flags);
    assert_eq!(a.chars, b.chars);
    assert_eq!(a.height, b.height);
    assert_eq!(a.smokes, b.smokes);
    // Initial Unit fields include an unused NaN pace sentinel. Compare the actual sight inputs.
    let rows = |g: &Game| {
        g.units
            .iter()
            .map(|(id, u)| (*id, u.kind, u.owner, u.x, u.z, u.hp, u.supp, u.built, u.air))
            .collect::<Vec<_>>()
    };
    assert_eq!(rows(a), rows(b));
}

#[test]
fn identical_received_coordinates_keep_point_vision_stable_at_the_live_boundary() {
    let mut m = scene::scene();
    m.world.game.weather = None;
    let own = scene::spawn(&mut m, "rifle", 0, Point { x: 40.0, z: 50.0 });
    let range = vision::vision_of(&m.world.game, &m.rules, &m.world.game.units[&own]);
    let point = Point {
        x: 40.0 + range,
        z: 50.0,
    };
    m.world.game.units.get_mut(&own).unwrap().x = 39.99;
    let a = m.opening_commander_observation(0).unwrap();
    let received_a = received_game(&m, &a).unwrap();
    let grid = SpatialGrid::new(&m.world.game.units, 16.0);
    assert!(!vision::team_sees(&m.world.game, &m.rules, &grid, 0, point));
    m.world.game.units.get_mut(&own).unwrap().x = 40.01;
    let b = m.opening_commander_observation(0).unwrap();
    let received_b = received_game(&m, &b).unwrap();
    assert_eq!(a.input(), b.input());
    let grid = SpatialGrid::new(&m.world.game.units, 16.0);
    assert!(vision::team_sees(&m.world.game, &m.rules, &grid, 0, point));
    assert_same_geometry(&received_a, &received_b);
    let grid = SpatialGrid::new(&received_a.units, 16.0);
    assert!(vision::team_sees(&received_a, &m.rules, &grid, 0, point));
}

#[test]
fn unpublished_physical_flags_and_heights_cannot_change_received_los() {
    let mut m = scene::scene();
    let observation = m.opening_commander_observation(0).unwrap();
    let before = received_game(&m, &observation).unwrap();
    let cell = 25 * m.world.game.w + 30;
    assert!(vision::los(&before, &m.rules, (40.0, 50.0), (80.0, 50.0)));
    m.world.set_cell(cell, b'H').unwrap();
    m.world.set_level(cell, 4).unwrap();
    assert!(m.world.game.flags[cell] & SIGHT != 0);
    assert!(!vision::los(
        &m.world.game,
        &m.rules,
        (40.0, 50.0),
        (80.0, 50.0)
    ));
    let after = received_game(&m, &observation).unwrap();
    assert_same_geometry(&before, &after);
    assert!(vision::los(&after, &m.rules, (40.0, 50.0), (80.0, 50.0)));
    assert_eq!(after.height.as_ref().unwrap()[cell], 0);
}

#[test]
fn hidden_actors_and_private_state_never_enter_received_sight_geometry() {
    let mut m = scene::scene();
    let own = scene::spawn(&mut m, "rifle", 0, Point { x: 40.0, z: 50.0 });
    let enemy = scene::spawn(&mut m, "rifle", 1, Point { x: 100.0, z: 100.0 });
    m.world.game.players[0].visible.clear();
    let observation = m.opening_commander_observation(0).unwrap();
    let before = received_game(&m, &observation).unwrap();
    assert!(!before.units.contains_key(&enemy));
    assert!(before.units.values().all(|u| u.owner == 0));
    m.world.game.units.get_mut(&enemy).unwrap().x = 110.0;
    m.world.game.units.get_mut(&enemy).unwrap().hp = 1.0;
    m.start.economy.players[1].mp = 9999.0;
    m.abilities.get_mut(enemy).unwrap().cooldown = 123.0;
    m.world.game.units.get_mut(&own).unwrap().path = vec![Point { x: 80.0, z: 80.0 }];
    m.commands.units.get_mut(&own).unwrap().attack_id = enemy;
    assert_same_geometry(&before, &received_game(&m, &observation).unwrap());
    assert!(before.units[&own].path.is_empty());
    assert_eq!(before.units[&own].x, 40.0);
    assert_eq!(before.w as f64 * CELL, 128.0);
}

#[test]
fn smoke_projection_uses_public_visibility_and_received_coordinates() {
    let mut m = scene::scene();
    scene::spawn(&mut m, "rifle", 0, Point { x: 40.0, z: 50.0 });
    m.world.game.smokes = vec![
        Smoke {
            id: 1,
            x: 40.04,
            z: 50.04,
            r: 4.0,
            remaining: Some(9.0),
        },
        Smoke {
            id: 2,
            x: 120.0,
            z: 120.0,
            r: 4.0,
            remaining: Some(9.0),
        },
    ];
    let observation = m.opening_commander_observation(0).unwrap();
    let received = received_game(&m, &observation).unwrap();
    assert_eq!(received.smokes.len(), 1);
    assert_eq!(
        received.smokes[0],
        Smoke {
            id: 1,
            x: 40.0,
            z: 50.0,
            r: 4.0,
            remaining: None
        }
    );
}

#[test]
fn delivered_allied_aircraft_use_the_public_airborne_state() {
    let mut m = scene::scene();
    let id = m.spawn_aircraft(0, "fighter").unwrap();
    let u = m.world.game.units.get_mut(&id).unwrap();
    u.x = 40.0;
    u.z = 50.0;
    u.air = Some(PlaneState::Out);
    m.grid.update(id, &m.world.game.units[&id]);
    let observation = m.opening_commander_observation(0).unwrap();
    let received = received_game(&m, &observation).unwrap();
    assert_eq!(received.units[&id].air, Some(PlaneState::Out));
    assert_eq!((received.units[&id].x, received.units[&id].z), (40.0, 50.0));
}

#[test]
fn live_sight_changes_cannot_rewrite_a_delivered_observation() {
    let mut m = scene::scene();
    scene::spawn(&mut m, "rifle", 0, Point { x: 40.0, z: 50.0 });
    let own = m
        .world
        .game
        .units
        .values()
        .find(|unit| unit.owner == 0 && m.rules.kinds[unit.kind].name == "hq")
        .unwrap()
        .id;
    let plane = m.spawn_aircraft(0, "fighter").unwrap();
    let aircraft = m.world.game.units.get_mut(&plane).unwrap();
    aircraft.x = 40.0;
    aircraft.z = 50.0;
    aircraft.air = Some(PlaneState::Out);
    m.grid.update(plane, &m.world.game.units[&plane]);
    let observation = m.opening_commander_observation(0).unwrap();
    let before = received_game(&m, &observation).unwrap();
    m.world.game.units.get_mut(&own).unwrap().built = Some(0.25);
    m.world.game.units.get_mut(&plane).unwrap().air = Some(PlaneState::Base);
    m.world.game.weather = m.rules.weather.iter().position(|weather| {
        !weather.rains && weather.sight != observation.input().runtime.as_ref().unwrap().sight
    });
    m.world.game.smokes.push(Smoke {
        id: 99,
        x: 40.0,
        z: 50.0,
        r: 12.0,
        remaining: Some(10.0),
    });
    assert_same_geometry(&before, &received_game(&m, &observation).unwrap());
    let fresh = m.opening_commander_observation(0).unwrap();
    let after = received_game(&m, &fresh).unwrap();
    assert_eq!(after.units[&own].built, Some(0.25));
    assert_eq!(after.units[&plane].air, Some(PlaneState::Base));
    assert_ne!(before.weather, after.weather);
    assert_ne!(before.smokes, after.smokes);
}
