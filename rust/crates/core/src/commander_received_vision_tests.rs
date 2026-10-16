use super::*;
use crate::{
    game::{CELL, SIGHT},
    mov_audit_test_support as scene,
};

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
    assert_eq!(received_a, received_b);
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
    assert_eq!(before, after);
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
    assert_eq!(before, received_game(&m, &observation).unwrap());
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
