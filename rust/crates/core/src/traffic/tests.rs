use super::*;
use crate::{
    catalog::Catalog, game::Player, map::AuthoritativeMap, movement, unit_spawn, world::World,
};
use serde_json::json;

fn field() -> (Game, Rules) {
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name": "traffic intent", "w": 48, "h": 24,
            "rows": vec!["................................................"; 24],
            "spawns": [{"x": 0, "y": 0}, {"x": 47, "y": 23}],
            "points": [{"id": "p", "x": 24, "y": 12}]
        }))
        .unwrap(),
    )
    .unwrap();
    let mut game = World::from_map(&map).unwrap().game;
    game.players = vec![
        Player {
            slot: 0,
            team: 0,
            visible: Vec::new(),
        },
        Player {
            slot: 1,
            team: 1,
            visible: Vec::new(),
        },
    ];
    (game, Rules::compiled().unwrap())
}

#[test]
fn intent_mov55_following_a_friend_does_not_age_or_start_a_ghost() {
    let (mut game, rules) = field();
    let destination = Point { x: 60.0, z: 20.0 };
    add(
        &mut game,
        &rules,
        1,
        0,
        "tank",
        Point { x: 17.0, z: 20.0 },
        vec![destination],
    );
    add(
        &mut game,
        &rules,
        2,
        0,
        "tank",
        Point { x: 20.0, z: 20.0 },
        vec![destination],
    );
    let grid = SpatialGrid::new(&game.units, 16.0);
    let mut follower = game.units.remove(&1).unwrap();
    follower.traffic_wait = rules.movement.traffic.ghost + 1.0;
    let prior_wait = follower.traffic_wait;
    let steer = traffic_step(
        &game,
        &rules,
        &grid,
        &mut follower,
        Some(destination),
        rules.movement.tick,
    );
    assert!(
        steer.waiting && steer.blocked,
        "same-route friends should form a queue"
    );
    assert_eq!(follower.queued_behind, 2);
    assert_eq!(follower.traffic_wait, prior_wait);
    assert_eq!(follower.ghost, 0.0);
    for _ in 0..80 {
        let next = traffic_step(
            &game,
            &rules,
            &grid,
            &mut follower,
            Some(destination),
            rules.movement.tick,
        );
        assert!(next.waiting);
    }
    assert_eq!(
        follower.traffic_wait, prior_wait,
        "queue time must not age gridlock priority"
    );
    assert_eq!(follower.ghost, 0.0);
    assert!(follower.traffic.is_none());
}

#[test]
fn intent_mov55_waiting_chain_cannot_close_a_cycle() {
    let (mut game, rules) = field();
    let destination = Point { x: 60.0, z: 20.0 };
    add(
        &mut game,
        &rules,
        1,
        0,
        "tank",
        Point { x: 17.0, z: 20.0 },
        vec![destination],
    );
    add(
        &mut game,
        &rules,
        2,
        0,
        "tank",
        Point { x: 20.0, z: 20.0 },
        vec![destination],
    );
    add(
        &mut game,
        &rules,
        3,
        0,
        "tank",
        Point { x: 45.0, z: 20.0 },
        vec![destination],
    );
    game.units.get_mut(&2).unwrap().queued_behind = 3;
    game.units.get_mut(&3).unwrap().queued_behind = 1;
    let grid = SpatialGrid::new(&game.units, 16.0);
    let mut follower = game.units.remove(&1).unwrap();
    let steer = traffic_step(
        &game,
        &rules,
        &grid,
        &mut follower,
        Some(destination),
        rules.movement.tick,
    );
    assert!(
        !steer.waiting,
        "a follower must ignore a leader already waiting on it"
    );
    assert_eq!(follower.queued_behind, 0);
}

#[test]
fn intent_mov54_horizon_margin_and_conflict_order() {
    let (mut game, rules) = field();
    let destination = Point { x: 60.0, z: 20.0 };
    add(
        &mut game,
        &rules,
        4,
        0,
        "tank",
        Point { x: 17.0, z: 20.0 },
        vec![destination],
    );
    let stationary = Point { x: 30.0, z: 20.0 };
    add(&mut game, &rules, 2, 0, "tank", stationary, vec![]);
    let pair = radius_pair(
        &rules.kinds[game.units[&4].kind],
        &rules.kinds[game.units[&2].kind],
    );
    let edge = 17.0
        + rules.kinds[game.units[&4].kind].speed * rules.movement.traffic.horizon
        + pair
        + 0.15;
    game.units.get_mut(&2).unwrap().x = edge + 0.01;
    let grid = SpatialGrid::new(&game.units, 16.0);
    let mut active = game.units.remove(&4).unwrap();
    let clear = traffic_step(
        &game,
        &rules,
        &grid,
        &mut active,
        Some(destination),
        rules.movement.tick,
    );
    assert!(
        !clear.blocked,
        "outside the horizon and pair margin there is no conflict"
    );
    game.units.get_mut(&2).unwrap().x = edge - 0.01;
    let grid = SpatialGrid::new(&game.units, 16.0);
    let blocked = traffic_step(
        &game,
        &rules,
        &grid,
        &mut active,
        Some(destination),
        rules.movement.tick,
    );
    assert!(
        blocked.blocked,
        "inside the horizon and pair margin there is a conflict"
    );

    game.units.get_mut(&2).unwrap().x = 23.0;
    game.units.get_mut(&2).unwrap().vx = Some(rules.kinds[active.kind].speed + 1.0);
    game.units.get_mut(&2).unwrap().vz = Some(0.0);
    let grid = SpatialGrid::new(&game.units, 16.0);
    let receding = traffic_step(
        &game,
        &rules,
        &grid,
        &mut active,
        Some(destination),
        rules.movement.tick,
    );
    assert!(
        !receding.blocked,
        "a blocker already pulling away does not stop the follower"
    );

    game.units.get_mut(&2).unwrap().x = 25.0;
    let west = -rules.kinds[game.units[&2].kind].speed;
    game.units.get_mut(&2).unwrap().vx = Some(west);
    game.units.get_mut(&2).unwrap().vz = Some(0.0);
    game.units.get_mut(&2).unwrap().path = vec![Point { x: 5.0, z: 20.0 }];
    add(
        &mut game,
        &rules,
        3,
        0,
        "tank",
        Point { x: 23.0, z: 20.0 },
        vec![Point { x: 5.0, z: 20.0 }],
    );
    game.units.get_mut(&3).unwrap().vx = Some(west);
    game.units.get_mut(&3).unwrap().vz = Some(0.0);
    active.traffic_wait = rules.movement.traffic.wait;
    active.traffic_entered = false;
    let grid = SpatialGrid::new(&game.units, 16.0);
    let earliest = traffic_step(
        &game,
        &rules,
        &grid,
        &mut active,
        Some(destination),
        rules.movement.tick,
    );
    assert!(earliest.yielding);
    assert_eq!(
        active.traffic.as_ref().unwrap().by,
        3,
        "first predicted conflict wins"
    );
    active.traffic = None;
    game.units.get_mut(&2).unwrap().x = 23.0;
    game.units.get_mut(&2).unwrap().z = 19.0;
    game.units.get_mut(&3).unwrap().z = 21.0;
    let grid = SpatialGrid::new(&game.units, 16.0);
    let tied = traffic_step(
        &game,
        &rules,
        &grid,
        &mut active,
        Some(destination),
        rules.movement.tick,
    );
    assert!(tied.yielding);
    assert_eq!(
        active.traffic.as_ref().unwrap().by,
        2,
        "unit id breaks an equal-time tie"
    );
}

#[test]
fn intent_mov57_rejected_sides_backtrack_to_a_kept_trail_point() {
    use crate::game::{MOVE, VBLOCK};
    let (mut game, rules) = field();
    let destination = Point { x: 60.0, z: 20.0 };
    add(
        &mut game,
        &rules,
        4,
        0,
        "tank",
        Point { x: 17.0, z: 20.0 },
        vec![destination],
    );
    add(
        &mut game,
        &rules,
        2,
        0,
        "tank",
        Point { x: 22.0, z: 20.0 },
        vec![Point { x: 5.0, z: 20.0 }],
    );
    let mut active = game.units.remove(&4).unwrap();
    active.traffic_wait = rules.movement.traffic.wait;
    let distance = radius_pair(&rules.kinds[active.kind], &rules.kinds[game.units[&2].kind]) + 0.7;
    for scale in [1.0, 1.5, 2.0] {
        for sign in [-1.0, 1.0] {
            let at = Point {
                x: active.x - 0.8,
                z: active.z + sign * distance * scale,
            };
            let cell = game.cell_of(at.x, at.z).unwrap();
            game.flags[cell] |= MOVE | VBLOCK;
        }
    }
    let trail = Pose {
        x: 12.0,
        z: 20.0,
        rot: 0.0,
    };
    active.traffic_trail.push(trail);
    let grid = SpatialGrid::new(&game.units, 16.0);
    let steer = traffic_step(
        &game,
        &rules,
        &grid,
        &mut active,
        Some(destination),
        rules.movement.tick,
    );
    assert!(
        steer.yielding,
        "blocked side candidates should leave a usable trail choice"
    );
    assert_eq!(
        steer.goal,
        Some(Point {
            x: trail.x,
            z: trail.z
        })
    );
    assert_eq!(
        active.traffic.as_ref().unwrap().until,
        rules.movement.traffic.max_yield
    );
    active.traffic = None;
    active.traffic_trail.clear();
    for i in 0..26 {
        active.x = 2.0 + i as f64 * 1.5;
        remember_position(&mut active);
    }
    assert_eq!(active.traffic_trail.len(), 20);
    assert_eq!(active.traffic_trail[0].x, 11.0);
    assert_eq!(active.traffic_trail[19].x, 39.5);
}

#[test]
fn intent_mov57_infantry_keeps_cover_when_choosing_a_side() {
    use crate::game::COVER;
    let (mut game, rules) = field();
    let destination = Point { x: 60.0, z: 20.0 };
    add(
        &mut game,
        &rules,
        4,
        0,
        "rifle",
        Point { x: 17.0, z: 20.0 },
        vec![destination],
    );
    add(
        &mut game,
        &rules,
        2,
        0,
        "tank",
        Point { x: 20.0, z: 20.0 },
        vec![Point { x: 5.0, z: 20.0 }],
    );
    let cover = game.cell_of(17.0, 20.0).unwrap();
    game.flags[cover] |= COVER;
    let mut active = game.units.remove(&4).unwrap();
    active.traffic_wait = rules.movement.traffic.wait;
    let grid = SpatialGrid::new(&game.units, 16.0);
    let exposed = traffic_step(
        &game,
        &rules,
        &grid,
        &mut active,
        Some(destination),
        rules.movement.tick,
    );
    assert!(
        exposed.blocked && !exposed.yielding,
        "open ground is worse than retained cover"
    );
    let distance = radius_pair(&rules.kinds[active.kind], &rules.kinds[game.units[&2].kind]) + 0.7;
    let candidate = Point {
        x: active.x - 0.8,
        z: active.z - distance,
    };
    let cover = game.cell_of(candidate.x, candidate.z).unwrap();
    game.flags[cover] |= COVER;
    let selected = traffic_step(
        &game,
        &rules,
        &grid,
        &mut active,
        Some(destination),
        rules.movement.tick,
    );
    assert!(selected.yielding, "an equally covered side is usable");
    assert_eq!(selected.goal, Some(candidate));
    let until = active.traffic.as_ref().unwrap().until;
    game.tick = (until / rules.movement.tick) as i64 - 1;
    let held = traffic_step(
        &game,
        &rules,
        &grid,
        &mut active,
        Some(destination),
        rules.movement.tick,
    );
    assert_eq!(held.goal, Some(candidate));
    assert_eq!(active.traffic.as_ref().unwrap().until, until);
    game.tick += 1;
    let expired = traffic_step(
        &game,
        &rules,
        &grid,
        &mut active,
        Some(destination),
        rules.movement.tick,
    );
    assert!(expired.yielding);
    assert!(
        active.traffic.as_ref().unwrap().until > until,
        "expired yield is reconsidered"
    );
}

#[test]
fn intent_mov53_only_visible_live_ground_blockers_enter_steering() {
    let (mut game, rules) = field();
    let destination = Point { x: 60.0, z: 20.0 };
    add(
        &mut game,
        &rules,
        4,
        0,
        "tank",
        Point { x: 17.0, z: 20.0 },
        vec![destination],
    );
    add(
        &mut game,
        &rules,
        2,
        1,
        "tank",
        Point { x: 22.0, z: 20.0 },
        vec![],
    );
    let mut active = game.units.remove(&4).unwrap();
    let check = |game: &Game, active: &mut Unit| {
        let grid = SpatialGrid::new(&game.units, 16.0);
        traffic_step(
            game,
            &rules,
            &grid,
            active,
            Some(destination),
            rules.movement.tick,
        )
    };
    assert!(
        !check(&game, &mut active).blocked,
        "unseen enemy does not steer the tank"
    );
    game.players[0].visible.push(2);
    assert!(
        check(&game, &mut active).blocked,
        "visible enemy blocks the tank"
    );
    game.units.get_mut(&2).unwrap().hp = 0.0;
    assert!(!check(&game, &mut active).blocked, "dead enemy is excluded");
    game.units.get_mut(&2).unwrap().hp = 1.0;
    game.units.get_mut(&2).unwrap().riding = true;
    assert!(
        !check(&game, &mut active).blocked,
        "riding enemy is excluded"
    );
    game.units.get_mut(&2).unwrap().riding = false;
    game.units.get_mut(&2).unwrap().garrison = Some(0);
    assert!(
        !check(&game, &mut active).blocked,
        "garrisoned enemy is excluded"
    );
    game.units.get_mut(&2).unwrap().garrison = None;
    game.units.get_mut(&2).unwrap().owner = 0;
    active.ghost = game.tick as f64 + 10.0;
    assert!(
        !check(&game, &mut active).blocked,
        "a friendly ghost pair is excluded"
    );
}

#[test]
fn intent_mov53_infantry_steers_around_heavy_bodies_only() {
    let (mut game, rules) = field();
    let destination = Point { x: 60.0, z: 20.0 };
    add(
        &mut game,
        &rules,
        4,
        0,
        "rifle",
        Point { x: 17.0, z: 20.0 },
        vec![destination],
    );
    add(
        &mut game,
        &rules,
        2,
        0,
        "rifle",
        Point { x: 20.0, z: 20.0 },
        vec![],
    );
    let mut active = game.units.remove(&4).unwrap();
    let grid = SpatialGrid::new(&game.units, 16.0);
    assert!(
        !traffic_step(
            &game,
            &rules,
            &grid,
            &mut active,
            Some(destination),
            rules.movement.tick
        )
        .blocked
    );
    game.units.remove(&2);
    add(
        &mut game,
        &rules,
        2,
        0,
        "tank",
        Point { x: 20.0, z: 20.0 },
        vec![],
    );
    let grid = SpatialGrid::new(&game.units, 16.0);
    assert!(
        traffic_step(
            &game,
            &rules,
            &grid,
            &mut active,
            Some(destination),
            rules.movement.tick
        )
        .blocked
    );
}

#[test]
fn intent_mov59_stuck_leader_yields_and_no_candidate_replans() {
    use crate::game::{MOVE, VBLOCK};
    let (mut game, rules) = field();
    let destination = Point { x: 60.0, z: 20.0 };
    add(
        &mut game,
        &rules,
        1,
        0,
        "tank",
        Point { x: 17.0, z: 20.0 },
        vec![destination],
    );
    add(
        &mut game,
        &rules,
        2,
        0,
        "tank",
        Point { x: 20.0, z: 20.0 },
        vec![destination],
    );
    game.units.get_mut(&2).unwrap().stuck = 2.0;
    let mut active = game.units.remove(&1).unwrap();
    active.traffic_wait = rules.movement.traffic.wait;
    let grid = SpatialGrid::new(&game.units, 16.0);
    let give_way = traffic_step(
        &game,
        &rules,
        &grid,
        &mut active,
        Some(destination),
        rules.movement.tick,
    );
    assert!(
        give_way.yielding,
        "a stuck leader cannot keep the following tank in its queue"
    );
    assert_eq!(active.queued_behind, 0);

    active.traffic = None;
    active.traffic_wait = rules.movement.traffic.replan;
    game.units.get_mut(&2).unwrap().owner = 1;
    game.units.get_mut(&2).unwrap().retreating = true;
    game.players[0].visible.push(2);
    game.units.get_mut(&2).unwrap().path = vec![Point { x: 5.0, z: 20.0 }];
    let distance = radius_pair(&rules.kinds[active.kind], &rules.kinds[game.units[&2].kind]) + 0.7;
    for scale in [1.0, 1.5, 2.0] {
        for sign in [-1.0, 1.0] {
            let at = Point {
                x: active.x - 0.8,
                z: active.z + sign * distance * scale,
            };
            let cell = game.cell_of(at.x, at.z).unwrap();
            game.flags[cell] |= MOVE | VBLOCK;
        }
    }
    let grid = SpatialGrid::new(&game.units, 16.0);
    let trapped = traffic_step(
        &game,
        &rules,
        &grid,
        &mut active,
        Some(destination),
        rules.movement.tick,
    );
    assert!(
        trapped.blocked && trapped.replan,
        "without a safe side or trail the route is retried"
    );
    assert!(active.traffic.is_none());
}

#[test]
fn intent_mov60_ghost_expires_and_never_pairs_with_an_enemy() {
    let (mut game, rules) = field();
    add(
        &mut game,
        &rules,
        1,
        0,
        "tank",
        Point { x: 17.0, z: 20.0 },
        vec![],
    );
    add(
        &mut game,
        &rules,
        2,
        0,
        "tank",
        Point { x: 19.0, z: 20.0 },
        vec![],
    );
    add(
        &mut game,
        &rules,
        3,
        1,
        "tank",
        Point { x: 21.0, z: 20.0 },
        vec![],
    );
    let mut ghost = game.units.remove(&1).unwrap();
    start_ghost(
        &game,
        &mut ghost,
        rules.movement.traffic.ghost_time,
        rules.movement.tick,
    );
    let until = ghost.ghost as i64;
    assert_eq!(
        until,
        math::round(rules.movement.traffic.ghost_time / rules.movement.tick) as i64
    );
    assert!(ghost_pair(
        &game,
        &ghost,
        &game.units[&2],
        game.allied(0, 0)
    ));
    assert!(!ghost_pair(
        &game,
        &ghost,
        &game.units[&3],
        game.allied(0, 1)
    ));
    game.tick = until;
    assert!(!ghost_pair(
        &game,
        &ghost,
        &game.units[&2],
        game.allied(0, 0)
    ));
}

#[test]
fn intent_mov60_ghost_keeps_friendly_vehicle_speed_during_separation() {
    let (mut friend, rules) = field();
    add(
        &mut friend,
        &rules,
        1,
        0,
        "tank",
        Point { x: 17.0, z: 20.0 },
        vec![],
    );
    add(
        &mut friend,
        &rules,
        2,
        0,
        "tank",
        Point { x: 18.0, z: 20.0 },
        vec![],
    );
    friend.units.get_mut(&1).unwrap().ghost = 20.0;
    for unit in friend.units.values_mut() {
        unit.move_speed = 3.0;
    }
    let mut enemy = friend.clone();
    enemy.units.get_mut(&2).unwrap().owner = 1;
    let mut friend_grid = SpatialGrid::new(&friend.units, 16.0);
    let mut enemy_grid = SpatialGrid::new(&enemy.units, 16.0);
    crate::separation::separate_crowds(&mut friend, &rules, &mut friend_grid);
    crate::separation::separate_crowds(&mut enemy, &rules, &mut enemy_grid);
    assert_eq!(friend.units[&1].move_speed, 3.0);
    assert_eq!(friend.units[&2].move_speed, 3.0);
    assert_eq!(enemy.units[&1].move_speed, 0.0);
    assert_eq!(enemy.units[&2].move_speed, 0.0);
}

#[test]
fn intent_mov60_enemy_contact_still_stops_a_ghosted_hull() {
    let (mut game, rules) = field();
    add(
        &mut game,
        &rules,
        1,
        0,
        "tank",
        Point { x: 17.0, z: 20.0 },
        vec![Point { x: 45.0, z: 20.0 }],
    );
    add(
        &mut game,
        &rules,
        2,
        1,
        "tank",
        Point { x: 21.0, z: 20.0 },
        vec![],
    );
    game.units.get_mut(&1).unwrap().ghost = 100.0;
    let mut grid = SpatialGrid::new(&game.units, 16.0);
    for _ in 0..35 {
        step(&mut game, &rules, &mut grid);
    }
    assert!(
        game.units[&1].x < game.units[&2].x,
        "enemy swept contact still stops the ghost"
    );
}

#[test]
fn intent_mov59_mov60_gridlocked_friend_ghosts_through_vehicle_contact() {
    let (mut game, rules) = field();
    let destination = Point { x: 45.0, z: 20.0 };
    add(
        &mut game,
        &rules,
        1,
        0,
        "tank",
        Point { x: 17.0, z: 20.0 },
        vec![destination],
    );
    add(
        &mut game,
        &rules,
        2,
        0,
        "tank",
        Point { x: 21.0, z: 20.0 },
        vec![],
    );
    game.units.get_mut(&1).unwrap().traffic_wait = rules.movement.traffic.ghost;
    let blocker = game.units[&2].at();
    let mut grid = SpatialGrid::new(&game.units, 16.0);
    step(&mut game, &rules, &mut grid);
    assert!(
        game.units[&1].ghost > game.tick as f64,
        "gridlock starts a bounded ghost"
    );
    assert_eq!(game.units[&1].traffic_wait, 0.0);
    let ghost_until = game.units[&1].ghost;
    for _ in 0..35 {
        step(&mut game, &rules, &mut grid);
    }
    assert_eq!(game.units[&2].at(), blocker);
    assert!(
        game.units[&1].x > blocker.x,
        "ghosted tank must cross the friendly blocker"
    );
    assert_eq!(game.units[&1].ghost, ghost_until);
}

#[test]
fn intent_mov56_oncoming_lane_holder_releases_when_route_changes() {
    let (mut game, rules) = field();
    let destination = Point { x: 45.0, z: 20.0 };
    add(
        &mut game,
        &rules,
        1,
        0,
        "tank",
        Point { x: 16.0, z: 20.0 },
        vec![destination],
    );
    add(
        &mut game,
        &rules,
        2,
        0,
        "tank",
        Point { x: 29.0, z: 20.0 },
        vec![Point { x: 7.0, z: 20.0 }],
    );
    let mut active = game.units.remove(&1).unwrap();
    active.lane_scan = Some(LaneScan {
        path: active.path_gen,
        until: 100.0,
        lane: Some((Point { x: 20.0, z: 20.0 }, Point { x: 30.0, z: 20.0 })),
    });
    let mut grid = SpatialGrid::new(&game.units, 16.0);
    let waiting = traffic_step(
        &game,
        &rules,
        &grid,
        &mut active,
        Some(destination),
        rules.movement.tick,
    );
    assert!(
        waiting.blocked && waiting.waiting,
        "oncoming friend has the narrow lane"
    );
    assert_eq!(active.queued_behind, 2);
    game.units.get_mut(&2).unwrap().x = 33.0;
    grid.rebuild(&game.units);
    let nearer = traffic_step(
        &game,
        &rules,
        &grid,
        &mut active,
        Some(destination),
        rules.movement.tick,
    );
    assert!(
        nearer.waiting,
        "the friend nearer the lane middle still goes first"
    );
    game.units.get_mut(&2).unwrap().x = 38.0;
    grid.rebuild(&game.units);
    let farther = traffic_step(
        &game,
        &rules,
        &grid,
        &mut active,
        Some(destination),
        rules.movement.tick,
    );
    assert!(!farther.waiting, "a farther friend does not hold this side");
    active.path_gen += 1;
    let changed = traffic_step(
        &game,
        &rules,
        &grid,
        &mut active,
        Some(destination),
        rules.movement.tick,
    );
    assert_eq!(active.lane_scan.as_ref().unwrap().path, active.path_gen);
    assert!(
        !changed.waiting,
        "a new route invalidates the old lane hold"
    );
}

fn add(
    game: &mut Game,
    rules: &Rules,
    id: UnitId,
    owner: usize,
    kind: &str,
    at: Point,
    path: Vec<Point>,
) {
    let mut unit = unit_spawn::initial_unit(
        rules,
        Catalog::compiled().unwrap(),
        rules.kind(kind).unwrap(),
        id,
        owner,
        at,
    )
    .unwrap();
    unit.path = path;
    game.units.insert(id, unit);
}

fn step(game: &mut Game, rules: &Rules, grid: &mut SpatialGrid) {
    movement::move_units_with_routes(game, rules, grid, &mut |_, _, unit, _, _| {
        unit.repath = 1.0;
    });
    game.tick += 1;
}

#[test]
fn intent_mov53_mov58_vehicle_passes_a_protected_squad_and_reaches_its_route_end() {
    let (mut game, rules) = field();
    let destination = Point { x: 48.0, z: 20.0 };
    add(
        &mut game,
        &rules,
        1,
        0,
        "tank",
        Point { x: 14.0, z: 20.0 },
        vec![destination],
    );
    add(
        &mut game,
        &rules,
        2,
        0,
        "rifle",
        Point { x: 22.0, z: 20.0 },
        vec![],
    );
    game.units.get_mut(&2).unwrap().hold_pos = true;
    let guard = game.units[&2].at();
    let mut grid = SpatialGrid::new(&game.units, 16.0);
    let mut passing = false;
    for _ in 0..500 {
        step(&mut game, &rules, &mut grid);
        passing |= game.units[&1].traffic.as_ref().is_some_and(|t| t.passing);
        if game.units[&1].path.is_empty() {
            break;
        }
    }
    assert!(passing, "the moving tank must use checked passing legs");
    assert_eq!(
        game.units[&2].at(),
        guard,
        "protected squad must remain fixed"
    );
    assert!(
        game.units[&1].path.is_empty(),
        "tank must finish its retained route"
    );
}
