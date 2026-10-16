//! MOV-21: blocked hull turns escape lengthwise or slide when neither escape can turn.
use serde_json::json;
use ww2_core::{
    catalog::Catalog,
    game::{Game, MOVE, Point, Pose},
    grid::SpatialGrid,
    map::AuthoritativeMap,
    movement::move_unit,
    rules::Rules,
    unit_spawn,
    vehicle::{swept_vehicle_clear, vehicle_position_clear, vehicle_step},
    world::World,
};

fn point(x: f64, z: f64) -> Point {
    Point { x, z }
}
fn scene(z: f64, goal_z: f64, walls: &[(usize, usize)]) -> (Game, Rules) {
    let rules = Rules::compiled().unwrap();
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"Hull clearance", "w":32,"h":32,"rows":vec![".".repeat(32);32],
            "spawns":[{"x":2,"y":2},{"x":29,"y":29}],"points":[{"x":16,"y":16}]
        }))
        .unwrap(),
    )
    .unwrap();
    let mut g = World::from_map(&map).unwrap().game;
    let mut u = unit_spawn::initial_unit(
        &rules,
        Catalog::compiled().unwrap(),
        rules.kind("tank").unwrap(),
        1,
        0,
        point(20., z),
    )
    .unwrap();
    u.path = vec![point(20., goal_z)];
    u.rot = 0.;
    u.stuck = 0.35;
    for &(x, y) in walls {
        g.flags[y * g.w + x] |= MOVE;
    }
    g.units.insert(1, u);
    (g, rules)
}

#[test]
fn mov21_sideways_slide_has_blocked_turn_and_both_lengthwise_escapes() {
    for (z, goal_z, wall_y, side) in [(20.7, 32., 11, -1.5), (21.3, 10., 9, 1.5)] {
        let (mut g, rules) = scene(z, goal_z, &[(10, wall_y), (7, 10)]);
        let u = &g.units[&1];
        let kind = &rules.kinds[u.kind];
        let profile = kind.vehicle.as_ref().unwrap();
        let before = u.pose();
        assert!(vehicle_position_clear(&g, before, kind, kind.block()));
        let proposal = vehicle_step(
            u,
            &rules,
            profile,
            &u.path,
            kind.speed,
            rules.movement.tick,
            false,
        );
        assert!(
            !swept_vehicle_clear(
                &g,
                before,
                Pose {
                    rot: proposal.rot,
                    ..before
                },
                kind,
                kind.block()
            ),
            "the proposed turn actually hits the wall"
        );
        for direction in [-1., 1.] {
            let escape = Pose {
                x: before.x + 2. * direction,
                ..before
            };
            let turn = Pose {
                rot: ww2_core::math::atan2(goal_z - escape.z, 20. - escape.x),
                ..escape
            };
            assert!(
                !swept_vehicle_clear(&g, before, escape, kind, kind.block())
                    || !swept_vehicle_clear(&g, escape, turn, kind, kind.block()),
                "neither lengthwise escape permits the required turn"
            );
        }
        let slide = Pose {
            z: before.z + side,
            ..before
        };
        assert!(swept_vehicle_clear(&g, before, slide, kind, kind.block()));
        assert!(swept_vehicle_clear(
            &g,
            slide,
            Pose {
                rot: ww2_core::math::atan2(goal_z - slide.z, 20. - slide.x),
                ..slide
            },
            kind,
            kind.block()
        ));
        let mut grid = SpatialGrid::new(&g.units, 16.);
        let mut control = g.clone();
        control.units.get_mut(&1).unwrap().stuck = 0.3;
        let mut control_grid = SpatialGrid::new(&control.units, 16.);
        move_unit(&mut control, &rules, &mut control_grid, 1, 1.).unwrap();
        assert_eq!(
            control.units[&1].pose(),
            before,
            "the slide requires more than 0.3 seconds stuck"
        );
        move_unit(&mut g, &rules, &mut grid, 1, 1.).unwrap();
        let u = &g.units[&1];
        assert!((u.z - (before.z + side.signum() * 1.2 * rules.movement.tick)).abs() < 1e-9);
        assert_eq!(u.x, before.x);
        assert_eq!(u.rot, before.rot);
        assert_eq!(u.move_speed, 0.);
        assert!(
            u.clearance.is_none(),
            "a lateral slide does not install a lengthwise escape"
        );
        assert_eq!(u.path, vec![point(20., goal_z)]);
        assert!(vehicle_position_clear(&g, u.pose(), kind, kind.block()));
    }
}

#[test]
fn mov21_lengthwise_escape_is_speed_limited_and_expires_or_cancels_with_route() {
    for (reverse, wall_x, goal_z, escape_x) in [(true, 10, 32., 18.), (false, 9, 10., 22.)] {
        for cancel in ["expiry", "replacement", "stop", "arrival"] {
            let (mut g, rules) = scene(20.7, goal_z, &[(wall_x, 11)]);
            let mut grid = SpatialGrid::new(&g.units, 16.);
            move_unit(&mut g, &rules, &mut grid, 1, 1.).unwrap();
            let u = &g.units[&1];
            let escape = u
                .clearance
                .clone()
                .expect("a lengthwise escape permits the turn");
            assert_eq!(escape.reverse, reverse);
            assert_eq!(escape.goal.x, escape_x);
            assert_eq!(escape.goal.z, 20.7);
            assert_eq!(escape.until, g.tick as f64 + 80.);
            assert!(if reverse { u.x < 20. } else { u.x > 20. });
            assert!(u.move_speed.abs() <= 1.2);
            let before = u.at();
            g.tick += 1;
            move_unit(&mut g, &rules, &mut grid, 1, 1.).unwrap();
            let u = &g.units[&1];
            assert!(
                ww2_core::math::hypot(u.x - before.x, u.z - before.z)
                    <= 1.2 * rules.movement.tick + 1e-9
            );
            assert!(u.clearance.is_some());
            let u = g.units.get_mut(&1).unwrap();
            u.stuck = 0.;
            match cancel {
                "expiry" => g.tick = escape.until as i64 + 1,
                "replacement" => {
                    u.path = vec![point(30., 20.7)];
                    u.path_gen += 1;
                }
                "stop" => {
                    u.path.clear();
                    u.path_gen += 1;
                }
                "arrival" => {
                    u.x = escape.goal.x;
                    u.z = escape.goal.z;
                }
                _ => unreachable!(),
            }
            grid.update(1, &g.units[&1]);
            move_unit(&mut g, &rules, &mut grid, 1, 1.).unwrap();
            assert!(
                g.units[&1].clearance.is_none(),
                "{cancel} releases escape control"
            );
        }
    }
}
