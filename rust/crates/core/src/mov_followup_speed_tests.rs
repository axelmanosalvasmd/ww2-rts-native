//! Speed-component witnesses against the pinned prototype, without logistics admission.
use crate::{
    game::{ROAD, WIRE},
    grid::SpatialGrid,
    mov_audit_test_support::{ground, point, unit},
    movement,
    rules::Rules,
};

#[test]
fn speed_component_pace_and_retreat_sprint_suppression_precedence() {
    let rules = Rules::compiled().unwrap();
    // Literal speeds follow sim.js:317-319, 3817 and 5483-5486 at e4e579a.
    for (pace, suppression, sprint, retreat, attack, routed, expected, retained_pace) in [
        (0., 49.999, 0., false, false, true, 4.6, 0.),
        (0., 50., 0., false, false, true, 4.6 * 0.6, 0.),
        (0., 89.999, 0., false, false, true, 4.6 * 0.6, 0.),
        (0., 90., 0., false, false, true, 4.6 * 0.3, 0.),
        (2., 90., 0., false, false, true, 2. * 0.3, 2.),
        (9., 0., 0., false, false, true, 4.6, 9.),
        (2., 90., 1., false, false, true, 2. * 1.6, 2.),
        (2., 90., 1., true, false, true, 4.6 * 1.5, 0.),
        (2., 0., 0., false, true, true, 4.6, 0.),
        (2., 0., 0., false, false, false, 4.6, 0.),
    ] {
        let mut game = ground();
        let mut actor = unit(&rules, "conscript", 1, point(41., 41.));
        actor.pace = pace;
        actor.supp = suppression;
        actor.sprint = sprint;
        actor.retreating = retreat;
        actor.attack = attack;
        actor.path = if routed {
            vec![point(61., 41.)]
        } else {
            vec![]
        };
        game.units.insert(1, actor);
        let mut grid = SpatialGrid::new(&game.units, 16.);
        let moved = movement::move_unit(
            &mut game,
            &rules,
            &mut grid,
            1,
            movement::supp_speed(suppression),
        )
        .unwrap();
        assert_eq!(moved.speed, expected);
        assert_eq!(game.units[&1].pace, retained_pace);
        assert_eq!(
            game.units[&1].x,
            41. + if routed { expected * 0.05 } else { 0. },
        );
        assert_eq!(game.units[&1].z, 41.);
    }
}

#[test]
fn speed_component_wire_slope_and_snow_multiply_actual_displacement() {
    let rules = Rules::compiled().unwrap();
    // Five footprint samples all stand on wire. The uphill sample rises 1.25 m,
    // exercising the grade cap; descending has no speed bonus. Snow scales infantry by .9.
    for (wire, uphill, downhill, snow, expected) in [
        (false, false, false, false, 4.5),
        (true, false, false, false, 4.5 * 0.35),
        (false, true, false, false, 4.5 * (1. - 0.2)),
        (false, false, true, false, 4.5),
        (false, false, false, true, 4.5 * 0.9),
        (true, true, false, true, 4.5 * 0.35 * (1. - 0.2) * 0.9),
    ] {
        let mut game = ground();
        if wire {
            game.flags.fill(WIRE);
        }
        if uphill || downhill {
            game.height = Some(
                (0..game.w * game.h)
                    .map(|c| i8::from((c % game.w >= 21) == uphill))
                    .collect(),
            );
        }
        if snow {
            game.weather = Some(rules.weather.iter().position(|w| w.name == "snow").unwrap());
        }
        let mut actor = unit(&rules, "rifle", 1, point(41., 41.));
        actor.path = vec![point(61., 41.)];
        game.units.insert(1, actor);
        let mut grid = SpatialGrid::new(&game.units, 16.);
        let moved = movement::move_unit(&mut game, &rules, &mut grid, 1, 1.).unwrap();
        assert_eq!(moved.speed, expected);
        assert_eq!(game.units[&1].x, 41. + expected * 0.05);
        assert_eq!(game.units[&1].z, 41.);
    }
}

#[test]
fn speed_component_vehicle_road_grade_and_snow_keep_their_own_factors() {
    let rules = Rules::compiled().unwrap();
    for (road, uphill, snow, expected) in [
        (false, false, false, 6.5),
        (true, false, false, 6.5 * 1.35),
        (false, true, false, 6.5 * (1. - 0.45)),
        (false, false, true, 6.5 * 0.85),
        (true, true, true, 6.5 * 1.35 * (1. - 0.45) * 0.85),
    ] {
        let mut game = ground();
        if road {
            game.flags.fill(ROAD);
        }
        if uphill {
            game.height = Some(
                (0..game.w * game.h)
                    .map(|c| i8::from(c % game.w >= 21))
                    .collect(),
            );
        }
        if snow {
            game.weather = Some(rules.weather.iter().position(|w| w.name == "snow").unwrap());
        }
        let mut actor = unit(&rules, "tank", 1, point(41., 41.));
        actor.path = vec![point(101., 41.)];
        // Existing forward motion isolates the selected top speed from acceleration.
        actor.move_speed = expected;
        actor.travel_dir = Some(0.);
        game.units.insert(1, actor);
        let mut grid = SpatialGrid::new(&game.units, 16.);
        let moved = movement::move_unit(&mut game, &rules, &mut grid, 1, 1.).unwrap();
        assert_eq!(moved.speed, expected);
        assert_eq!(game.units[&1].x, 41. + expected * 0.05);
        assert_eq!(game.units[&1].z, 41.);
    }
}
