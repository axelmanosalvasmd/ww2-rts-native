use crate::{
    catalog::Catalog,
    game::{Game, LAND, MOVE, Point, Pose, ROAD},
    grid::SpatialGrid,
    map::AuthoritativeMap,
    movement::move_unit,
    rules::Rules,
    separation::separate_crowds,
    traffic::Steer,
    traffic_knowledge::TrafficKnowledge,
    unit_spawn::initial_unit,
    vehicle::{swept_vehicle_clear, vehicle_position_clear},
    world::World,
};
use serde_json::json;

fn game() -> Game {
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"Movement runtime intent", "w":40, "h":40,
            "rows":vec!["........................................";40],
            "spawns":[{"x":2,"y":2},{"x":37,"y":37}],
            "points":[{"x":20,"y":20}]
        }))
        .unwrap(),
    )
    .unwrap();
    World::from_map(&map).unwrap().game
}

fn add(g: &mut Game, rules: &Rules, name: &str, id: u32, at: Point) {
    let kind = rules.kind(name).unwrap();
    let unit = initial_unit(rules, Catalog::compiled().unwrap(), kind, id, 0, at).unwrap();
    g.units.insert(id, unit);
}

#[test]
fn mov15_rotated_vehicle_hull_blocks_wall_and_map_edge_during_motion() {
    let rules = Rules::compiled().unwrap();
    let mut g = game();
    let kind = &rules.kinds[rules.kind("tank").unwrap()];
    let wall = g.cell_of(21.0, 23.0).unwrap();
    g.flags[wall] |= MOVE;
    let start = Pose {
        x: 20.0,
        z: 20.3,
        rot: 0.0,
    };
    let facing_wall = Pose {
        rot: std::f64::consts::FRAC_PI_2,
        ..start
    };
    assert!(vehicle_position_clear(&g, start, kind, kind.block()));
    assert!(!vehicle_position_clear(&g, facing_wall, kind, kind.block()));
    assert!(!swept_vehicle_clear(
        &g,
        start,
        facing_wall,
        kind,
        kind.block()
    ));
    assert!(!vehicle_position_clear(
        &g,
        Pose {
            x: 0.5,
            z: 20.0,
            rot: 0.0
        },
        kind,
        kind.block()
    ));
    add(
        &mut g,
        &rules,
        "tank",
        1,
        Point {
            x: start.x,
            z: start.z,
        },
    );
    {
        let u = g.units.get_mut(&1).unwrap();
        u.rot = start.rot;
        u.path = vec![Point { x: 20.0, z: 34.0 }];
    }
    let mut grid = SpatialGrid::new(&g.units, 16.0);
    let mut changed = false;
    for _ in 0..80 {
        let before = g.units[&1].pose();
        move_unit(&mut g, &rules, &mut grid, 1, 1.0).unwrap();
        let delivered = g.units[&1].pose();
        assert!(vehicle_position_clear(&g, delivered, kind, kind.block()));
        assert!(swept_vehicle_clear(
            &g,
            before,
            delivered,
            kind,
            kind.block()
        ));
        changed |= delivered != start;
        g.tick += 1;
    }
    assert!(
        changed,
        "the delivered hull must be observed moving and turning"
    );
}

#[test]
fn mov20_reverse_keeps_hull_facing_forward_and_clears_on_new_route() {
    let rules = Rules::compiled().unwrap();
    let mut g = game();
    add(&mut g, &rules, "tank", 1, Point { x: 30.0, z: 30.0 });
    let behind = Point { x: 24.0, z: 30.0 };
    {
        let u = g.units.get_mut(&1).unwrap();
        u.path = vec![behind];
        u.reverse = Some(behind);
        u.rot = 0.0;
    }
    let mut grid = SpatialGrid::new(&g.units, 16.0);
    for _ in 0..12 {
        move_unit(&mut g, &rules, &mut grid, 1, 1.0).unwrap();
    }
    let backed = g.units.get(&1).unwrap();
    assert!(backed.x < 30.0, "vehicle must actually back up");
    assert!(backed.move_speed < 0.0);
    assert!(backed.rot.abs() < 0.1, "hull must keep facing ahead");
    let u = g.units.get_mut(&1).unwrap();
    u.path = vec![Point { x: 40.0, z: 30.0 }];
    u.path_gen += 1;
    move_unit(&mut g, &rules, &mut grid, 1, 1.0).unwrap();
    assert!(g.units[&1].reverse.is_none());
}

#[test]
fn mov20_rear_yield_goal_uses_reverse_without_an_explicit_retreat_order() {
    let rules = Rules::compiled().unwrap();
    let mut g = game();
    add(&mut g, &rules, "tank", 1, Point { x: 30.0, z: 30.0 });
    let grid = SpatialGrid::new(&g.units, 16.0);
    let mut u = g.units.remove(&1).unwrap();
    u.rot = 0.0;
    let before = u.pose();
    let behind = Point { x: 25.0, z: 30.0 };
    let mut route = vec![behind];
    let steer = Steer {
        goal: Some(behind),
        yielding: true,
        ..Steer::default()
    };
    let kind = &rules.kinds[u.kind];
    super::drive_vehicle(
        &g,
        &rules,
        &grid,
        &mut u,
        kind,
        &mut route,
        kind.speed,
        rules.movement.tick,
        false,
        &steer,
        before,
        &TrafficKnowledge::physical(),
    );
    assert!(u.x < before.x);
    assert!(u.move_speed < 0.0);
    assert!(u.rot.abs() < 0.1);
}

#[test]
fn mov21_stalled_hull_starts_bounded_escape_when_turn_hits_wall() {
    let rules = Rules::compiled().unwrap();
    let mut g = game();
    add(&mut g, &rules, "tank", 1, Point { x: 20.0, z: 20.2 });
    let wall = g.cell_of(21.0, 23.0).unwrap();
    g.flags[wall] |= MOVE;
    {
        let u = g.units.get_mut(&1).unwrap();
        u.rot = 0.3;
        u.path = vec![Point { x: 20.0, z: 32.0 }];
        u.stuck = 0.35;
    }
    let mut grid = SpatialGrid::new(&g.units, 16.0);
    move_unit(&mut g, &rules, &mut grid, 1, 1.0).unwrap();
    let escape = g.units[&1]
        .clearance
        .as_ref()
        .expect("escape starts after a blocked turn");
    assert!(escape.reverse);
    assert!(escape.goal.x < 20.0);
    assert_eq!(escape.until, g.tick as f64 + 80.0);
    assert!(g.units[&1].move_speed.abs() <= 1.2);
    g.tick += 81;
    move_unit(&mut g, &rules, &mut grid, 1, 1.0).unwrap();
    assert!(
        g.units[&1].clearance.is_none(),
        "expired escape gives control back to the route"
    );
}

#[test]
fn mov105_boat_follows_water_legs_with_weather_and_ignores_road_ground() {
    let rules = Rules::compiled().unwrap();
    let mut g = game();
    g.flags.fill(0);
    add(&mut g, &rules, "gunboat", 1, Point { x: 20.0, z: 20.0 });
    let waypoints = vec![Point { x: 25.0, z: 20.0 }, Point { x: 25.0, z: 30.0 }];
    g.units.get_mut(&1).unwrap().path = waypoints.clone();
    let mut grid = SpatialGrid::new(&g.units, 16.0);
    let clear = move_unit(&mut g, &rules, &mut grid, 1, 1.0).unwrap().speed;
    assert_eq!(clear, rules.kinds[g.units[&1].kind].speed);
    assert!(g.units[&1].traffic.is_none());
    let travelled = g.units[&1].x - 20.0;
    assert!(travelled > 0.0);
    let road = g.cell_of(g.units[&1].x, g.units[&1].z).unwrap();
    g.flags[road] |= ROAD;
    let rain = rules.weather.iter().position(|w| w.veh < 1.0).unwrap();
    g.weather = Some(rain);
    let wet = move_unit(&mut g, &rules, &mut grid, 1, 1.0).unwrap().speed;
    assert_eq!(
        wet,
        rules.kinds[g.units[&1].kind].speed * rules.weather[rain].veh
    );
    assert_eq!(g.units[&1].path, waypoints);
    for _ in 0..18 {
        move_unit(&mut g, &rules, &mut grid, 1, 1.0).unwrap();
    }
    assert_eq!(g.units[&1].path.len(), 1, "boat takes the second water leg");
    assert!(g.units[&1].z > 20.0);
    let land = g.cell_of(25.0, 24.0).unwrap();
    g.flags[land] |= LAND;
    for _ in 0..30 {
        move_unit(&mut g, &rules, &mut grid, 1, 1.0).unwrap();
    }
    assert!(g.units[&1].z < 24.0, "boat must stay in water");
}

#[test]
fn mov105_boats_separate_softly_but_boat_and_vehicle_make_solid_contact() {
    let rules = Rules::compiled().unwrap();
    let mut soft = game();
    soft.flags.fill(0);
    add(&mut soft, &rules, "gunboat", 1, Point { x: 20.0, z: 20.0 });
    add(&mut soft, &rules, "gunboat", 2, Point { x: 21.0, z: 20.0 });
    let mut hard = game();
    hard.flags.fill(0);
    add(&mut hard, &rules, "gunboat", 1, Point { x: 20.0, z: 20.0 });
    add(&mut hard, &rules, "tank", 2, Point { x: 21.0, z: 20.0 });
    let mut soft_grid = SpatialGrid::new(&soft.units, 16.0);
    let mut hard_grid = SpatialGrid::new(&hard.units, 16.0);
    separate_crowds(&mut soft, &rules, &mut soft_grid);
    separate_crowds(&mut hard, &rules, &mut hard_grid);
    let soft_gap = soft.units[&2].x - soft.units[&1].x;
    let hard_gap = hard.units[&2].x - hard.units[&1].x;
    assert!(soft_gap > 1.0);
    assert!(
        hard_gap > soft_gap,
        "a vehicle must make solid contact with a boat"
    );
}
