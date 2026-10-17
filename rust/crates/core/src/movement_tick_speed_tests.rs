//! MOV-04: speed selection and the reserve cap through ordinary native ticks.
use crate::{
    grid::SpatialGrid,
    match_commands::Command,
    match_runtime::Match,
    mov_audit_test_support::{close, ground, point, scene, scene_rows, spawn, unit},
    movement,
    recipient_snapshot::{DeliveryState, SelectedSeat},
    rules::Rules,
    traffic_knowledge::TrafficKnowledge,
    weather::{WeatherKind, WeatherPlan},
};

fn quiet(m: &mut Match) {
    for u in m.world.game.units.values_mut() {
        u.hold_pos = true;
    }
    for c in m.commands.units.values_mut() {
        c.hold_fire = true;
        c.auto_cast = false;
        c.auto_retreat = false;
    }
}

fn actor(m: &mut Match, name: &str, x: f64) -> u32 {
    let id = spawn(m, name, 0, point(x, 41.));
    m.recovery.initialize(id);
    quiet(m);
    let u = m.world.game.units.get_mut(&id).unwrap();
    u.hold_pos = false;
    u.rot = 0.;
    u.path = vec![point(x + 30., 41.)];
    u.world_goal = u.path.last().copied();
    u.path_gen += 1;
    u.repath = 1.;
    id
}

fn tick(m: &mut Match, delivery: &mut DeliveryState) {
    let report = m.advance_tick().unwrap();
    let selected = (0..m.start.players.len())
        .map(|slot| SelectedSeat {
            slot,
            full: true,
            omit: false,
        })
        .collect::<Vec<_>>();
    let frame = delivery
        .capture_selected_with_tick_events(m, &selected, &report.events)
        .unwrap()
        .unwrap();
    for slot in 0..selected.len() {
        assert_eq!(frame.recipient(slot).unwrap().into_json()["t"], "s");
    }
    assert!(m.tick_state.fault().is_none());
}

#[test]
fn mov04_ordinary_ticks_select_pace_retreat_ura_and_pre_rally_suppression() {
    // Literal factors: e4e579a sim.js:317-319,3817,5345,5362,5483-5486.
    // At exactly 50 or 90, rally crosses the threshold during this tick.
    // Its faster speed is available only on the following turn.
    for (pace, supp, ura, retreat, first, second, retained_pace) in [
        (0., 0., false, false, 4.6, 4.6, 0.),
        (2., 0., false, false, 2., 2., 2.),
        (9., 0., false, false, 4.6, 4.6, 9.),
        (0., 49.999, false, false, 4.6, 4.6, 0.),
        (0., 50., false, false, 2.76, 4.6, 0.),
        (0., 89.999, false, false, 2.76, 2.76, 0.),
        (0., 90., false, false, 1.38, 2.76, 0.),
        (2., 90., false, false, 0.6, 1.2, 2.),
        (2., 90., true, false, 3.2, 3.2, 2.),
        (2., 90., true, true, 6.9, 6.9, 0.),
        (2., 90., false, true, 6.9, 6.9, 0.),
    ] {
        let mut m = scene();
        let id = actor(&mut m, "conscript", 41.);
        if ura {
            assert_eq!(
                m.execute_command(
                    0,
                    &Command::Ability {
                        ids: vec![i64::from(id)],
                        at: None,
                        queue: false,
                        automatic: false,
                    },
                    false,
                )
                .unwrap(),
                None,
            );
            close(m.world.game.units[&id].sprint, 6.);
        }
        let u = m.world.game.units.get_mut(&id).unwrap();
        u.pace = pace;
        // Incoming suppression while Ura is active must not replace its speed.
        u.supp = supp;
        u.retreating = retreat;
        let mut delivery = DeliveryState::new(&m).unwrap();
        for expected in [first, second] {
            let before = m.world.game.units[&id].at();
            let prior_supp = m.world.game.units[&id].supp;
            tick(&mut m, &mut delivery);
            let u = &m.world.game.units[&id];
            close(u.x - before.x, expected * 0.05);
            close(u.z, before.z);
            close(u.pace, retained_pace);
            close(u.supp, (prior_supp - 0.4).max(0.));
        }
    }
}

#[test]
fn mov04_ordinary_ticks_multiply_ground_slope_and_weather() {
    for (name, terrain, uphill, snow, expected) in [
        ("rifle", b'.', false, false, 4.5),
        ("rifle", b'X', false, false, 1.575),
        ("rifle", b'.', true, false, 3.6),
        ("rifle", b'.', false, true, 4.05),
        ("rifle", b'X', true, true, 1.134),
        ("tank", b'.', false, false, 6.5),
        ("tank", b'=', false, false, 8.775),
        ("tank", b'.', true, false, 3.575),
        ("tank", b'.', false, true, 5.525),
        ("tank", b'=', true, true, 4.1023125),
    ] {
        let mut m = scene();
        let id = actor(&mut m, name, 41.);
        // All five footprint samples have the same surface. Keep initial bases intact.
        for y in 17..25 {
            for x in 17..25 {
                m.world.set_cell(y * m.world.game.w + x, terrain).unwrap();
            }
        }
        if uphill {
            for y in 17..25 {
                for x in 21..25 {
                    m.world.set_level(y * m.world.game.w + x, 1).unwrap();
                }
            }
        }
        if snow {
            m.start.weather = WeatherPlan::fixed(WeatherKind::Snow);
            m.world.game.weather = Some(WeatherKind::Snow as usize);
        }
        let u = m.world.game.units.get_mut(&id).unwrap();
        u.move_speed = expected;
        u.travel_dir = Some(0.);
        let mut delivery = DeliveryState::new(&m).unwrap();
        let before = m.world.game.units[&id].at();
        tick(&mut m, &mut delivery);
        close(m.world.game.units[&id].x - before.x, expected * 0.05);
        close(m.world.game.units[&id].z, before.z);
        for _ in 0..3 {
            tick(&mut m, &mut delivery);
        }
    }
}

#[test]
fn mov04_detached_march_multiplies_the_selected_component_speed() {
    // World rooms remain closed. The existing original_world_march_home_visibility_cover_and_first_contact
    // test covers eligibility; this tests its 1.5 result at the displacement consumer.
    let rules = Rules::compiled().unwrap();
    for (march, expected) in [(1., 1.134), (1.5, 1.701)] {
        let mut g = ground();
        g.flags.fill(crate::game::WIRE);
        g.height = Some((0..g.w * g.h).map(|c| i8::from(c % g.w >= 21)).collect());
        g.weather = Some(rules.weather.iter().position(|w| w.name == "snow").unwrap());
        let mut u = unit(&rules, "rifle", 1, point(41., 41.));
        u.path = vec![point(71., 41.)];
        g.units.insert(1, u);
        let mut grid = SpatialGrid::new(&g.units, 16.);
        let moved = movement::move_unit_with_march(
            &mut g,
            &rules,
            &mut grid,
            1,
            1.,
            &mut |_, _, _, _, _| panic!("clear straight route must not replan"),
            &TrafficKnowledge::physical(),
            march,
        )
        .unwrap()
        .unwrap();
        close(moved.speed, expected);
        close(g.units[&1].x - 41., expected * 0.05);
        close(g.units[&1].z, 41.);
    }
}

#[test]
fn mov04_ordinary_fuel_cap_limits_momentum_and_uses_emergency_only_when_forced() {
    // The river disconnects the actor from friendly supply. The expected first
    // step is literal, including retreat's 1.5 priority and the final fuel/dt cap.
    for (fuel, emergency, forced, supp, expected) in [
        (1., 1., false, 0., 6.5),
        (0.025, 1., false, 0., 3.25),
        (0.025, 1., false, 90., 0.975),
        (0.0125, 0.0125, false, 0., 1.625),
        (0., 0.025, true, 0., 4.875),
        (0.0125, 0.0125, true, 0., 4.875),
        (0., 1., true, 90., 9.75),
        (0., 0., true, 0., 0.),
    ] {
        let mut row = ".".repeat(64);
        row.replace_range(28..30, "WW");
        let mut m = scene_rows(vec![row; 64]);
        let id = actor(&mut m, "tank", 81.);
        m.options.supply = true;
        m.options.logistics = true;
        m.initialize_logistics().unwrap();
        quiet(&mut m);
        let u = m.world.game.units.get_mut(&id).unwrap();
        u.hold_pos = false;
        // Deliberately retain more momentum than the low-fuel step permits.
        u.move_speed = 20.;
        u.travel_dir = Some(0.);
        u.supp = supp;
        let r = m.logistics.get_mut(id).unwrap();
        r.fuel = Some(fuel);
        r.emergency = Some(emergency);
        r.forced = forced;
        let mut delivery = DeliveryState::new(&m).unwrap();
        let before = m.world.game.units[&id].at();
        tick(&mut m, &mut delivery);
        close(m.world.game.units[&id].x - before.x, expected * 0.05);
        close(m.world.game.units[&id].z, before.z);
        let r = m.logistics.get(id).unwrap();
        if expected > 0. {
            assert!(r.fuel.unwrap() < fuel || r.emergency.unwrap() < emergency);
        } else {
            assert!(r.stranded);
        }
        for _ in 0..3 {
            tick(&mut m, &mut delivery);
        }
    }
}

#[test]
fn mov04_ground_factor_branches_footprint_average_and_naval_bypass_move_the_unit() {
    use crate::game::{FORD, MOVE, MUD, ROAD, TRENCH, WIRE, WOOD};

    let rules = Rules::compiled().unwrap();
    let step = |mut g: crate::game::Game, name: &str, expected: f64, distance: f64| {
        let mut u = unit(&rules, name, 1, point(41., 41.));
        u.path = vec![point(81., 41.)];
        u.rot = 0.;
        u.move_speed = expected;
        u.travel_dir = Some(0.);
        g.units.insert(1, u);
        let mut grid = SpatialGrid::new(&g.units, 16.);
        let moved = movement::move_unit(&mut g, &rules, &mut grid, 1, 1.).unwrap();
        close(moved.speed, expected);
        close(g.units[&1].x - 41., distance);
        close(g.units[&1].z, 41.);
    };

    // Literal speeds use sim.js:60,81-91,101,2363-2375 at e4e579a.
    // Each constant surface covers all five samples; a flat depression changes
    // wetness without adding an uphill slope. Roads and woods ignore wetness.
    for (name, flags, wear, wet, depression, expected) in [
        ("rifle", TRENCH, 0., 0., false, 5.4),
        ("rifle", FORD, 0., 0., false, 3.375),
        ("rifle", FORD, 1., 0., false, 1.575),
        ("rifle", FORD, 0.5, 0.5, false, 2.10375),
        ("rifle", FORD, 0.5, 0.75, true, 1.7325),
        ("rifle", 0, 1., 1., true, 4.5),
        ("rifle", WIRE | TRENCH, 0., 0., false, 1.575),
        ("tank", WOOD, 1., 1., true, 3.25),
        ("tank", WOOD | ROAD | MUD, 1., 1., true, 3.25),
        ("tank", ROAD, 0., 1., true, 8.775),
        ("tank", ROAD, 0.5, 1., true, 7.6375),
        ("tank", ROAD | MUD, 1., 1., true, 6.5),
        ("tank", MUD, 0., 0., false, 4.55),
        ("tank", MUD, 1., 0., false, 2.275),
        ("tank", MUD, 0.5, 0.5, false, 3.07125),
        ("tank", 0, 0., 0., false, 6.5),
        ("tank", 0, 1., 0., false, 4.55),
        ("tank", 0, 0.5, 0.5, false, 4.9725),
        ("tank", 0, 0.5, 0.5, true, 4.42),
        ("tank", 0, 0.5, 1., true, 3.315),
        ("tank", FORD | WOOD | ROAD | MUD, 0.5, 0.5, false, 3.03875),
        (
            "gunboat",
            FORD | WIRE | WOOD | ROAD | MUD,
            1.,
            1.,
            true,
            10.,
        ),
    ] {
        let mut g = ground();
        g.flags.fill(flags);
        g.wear.as_mut().unwrap().fill(wear);
        g.wet = Some(wet);
        g.height = Some(vec![if depression { -1 } else { 0 }; g.w * g.h]);
        g.weather = None;
        step(g, name, expected, expected * 0.05);
    }

    // Tank samples at (41,41), (42.5,41), (39.5,41), (41,42.5),
    // and (41,39.5) fall in five distinct cells. Excluding a blocked sample
    // changes the denominator as well as the sum.
    for (blocked, expected) in [(false, 7.41), (true, 7.6375)] {
        let mut g = ground();
        g.flags[20 * g.w + 20] = ROAD;
        g.flags[20 * g.w + 21] = ROAD;
        if blocked {
            g.flags[20 * g.w + 19] = MOVE | MUD;
        }
        step(g, "tank", expected, expected * 0.05);
    }

    // With no admissible samples, speedMul falls back to one. This short step
    // stays within the starting cell, which clear() excludes (sim.js:2780-2800).
    let mut g = ground();
    g.flags.fill(MOVE | FORD);
    step(g, "rifle", 4.5, 4.5 * 0.05);
}
