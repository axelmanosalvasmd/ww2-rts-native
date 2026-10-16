//! Scheduler component checks. World hooks supply route safety and handoff facts.
use super::intent_tests::{context, player, setup, store, unit};
use super::*;

fn p(x: f64, z: f64) -> Point {
    Point { x, z }
}

fn delivery(target: Point) -> Truck {
    Truck {
        state: TruckState::Delivering,
        cargo: Stock {
            provisions: 80.0,
            ..Stock::default()
        },
        target: Some(Target {
            id: TargetId::Unit(900),
            unit: None,
            store: None,
            owner: 0,
            at: target,
            members: vec![],
            need: Stock {
                provisions: 80.0,
                ..Stock::default()
            },
            priority: None,
            relief: None,
        }),
        origin: StoreId::Source(0),
        timer: 0.0,
        manual: false,
        hold: false,
        route: vec![target],
        destination: Some(target),
        route_failure: None,
        retry: None,
    }
}

#[test]
fn intent_log37_automatic_same_cell_failures_back_off_to_thirty_two_seconds_and_keep_cargo() {
    let (mut state, _, mut world) = setup();
    let players = [player(0, 0, p(0.0, 0.0))];
    let u = unit(1, 0, UnitRole::Truck, p(0.0, 0.0));
    world.units.push(u.clone());
    let mut c = delivery(p(100.0, 0.0));
    let cargo = c.cargo;
    let mut tick = 0;
    for (i, wait) in [2.0, 4.0, 8.0, 16.0, 32.0, 32.0].into_iter().enumerate() {
        let failure = [
            RouteFailure::Dangerous,
            RouteFailure::Blocked,
            RouteFailure::Unreachable,
        ][i % 3];
        world.failure = Some(failure);
        assert!(!state.send(
            context(&players, tick),
            &u,
            &mut c,
            p(100.0, 0.0),
            &mut world,
            false
        ));
        let retry = c.retry.clone().unwrap();
        assert_eq!(retry.wait, wait);
        assert_eq!(retry.at, tick + (wait * 20.0) as u64);
        assert_eq!(c.route_failure, Some(failure));
        assert_eq!(c.cargo, cargo);
        let searches = world.routes.len();
        assert!(!state.send(
            context(&players, retry.at - 1),
            &u,
            &mut c,
            p(100.1, 0.0),
            &mut world,
            false
        ));
        assert_eq!(
            world.routes.len(),
            searches,
            "same eight-metre cell retains the deadline"
        );
        tick = retry.at;
    }
    world.failure = None;
    assert!(state.send(
        context(&players, tick),
        &u,
        &mut c,
        p(100.0, 0.0),
        &mut world,
        false
    ));
    assert!(c.retry.is_none());
    assert_eq!(c.route_failure, None);
    assert_eq!(c.cargo, cargo);
    assert!(world.routes.iter().all(|r| r.2));

    world.failure = Some(RouteFailure::Blocked);
    assert!(!state.send(
        context(&players, tick),
        &u,
        &mut c,
        p(200.0, 0.0),
        &mut world,
        false
    ));
    assert_eq!(
        c.retry.as_ref().unwrap().wait,
        2.0,
        "another cell starts a new retry series"
    );
    world.failure = None;
    assert!(state.send(
        context(&players, tick + 1),
        &u,
        &mut c,
        p(200.0, 0.0),
        &mut world,
        true
    ));
    assert!(
        !world.routes.last().unwrap().2,
        "manual orders bypass automatic safety and retry delay"
    );
}

#[test]
fn intent_log38_refresh_reuses_only_nearby_goals_with_a_positive_safety_hook() {
    let players = [player(0, 0, p(0.0, 0.0))];
    for (goal_shift, safe, searches) in [(11.99, true, 0), (12.0, true, 1), (0.0, false, 1)] {
        let (mut state, _, mut world) = setup();
        let mut u = unit(1, 0, UnitRole::Truck, p(0.0, 0.0));
        u.path = vec![p(50.0, 0.0), p(100.0, 0.0)];
        u.world_goal = Some(p(100.0, 0.0));
        let mut c = delivery(p(100.0 + goal_shift, 0.0));
        c.destination = Some(p(100.0, 0.0));
        c.route = u.path.clone();
        world.units.push(u.clone());
        world.keep_route = safe;
        state.trucks.insert(u.id, c);
        state.refresh_queue.push_back(u.id);
        state.refresh(context(&players, 40), &mut world);
        assert_eq!(world.routes.len(), searches);
        if searches == 0 {
            assert_eq!(world.unit(u.id).unwrap().path, u.path);
            assert_eq!(state.routed, 0);
        }
    }
}

#[test]
fn intent_log38_stuck_trucks_rejoin_thirty_metres_ahead_and_preserve_the_remaining_route() {
    let players = [player(0, 0, p(0.0, 0.0))];
    let (mut state, _, mut world) = setup();
    let mut u = unit(1, 0, UnitRole::Truck, p(0.0, 0.0));
    u.stuck = 0.51;
    u.path = vec![
        p(10.0, 0.0),
        p(29.0, 0.0),
        p(35.0, 5.0),
        p(80.0, 0.0),
        p(100.0, 0.0),
    ];
    u.world_goal = Some(p(100.0, 0.0));
    let mut c = delivery(p(100.0, 0.0));
    c.route = u.path.clone();
    world.units.push(u.clone());
    state.trucks.insert(u.id, c);
    state.refresh_queue.push_back(u.id);
    state.refresh(context(&players, 40), &mut world);
    assert_eq!(world.routes, vec![(u.id, p(35.0, 5.0), true)]);
    assert_eq!(world.unit(u.id).unwrap().path, u.path[2..]);
    assert_eq!(world.unit(u.id).unwrap().stuck, 0.0);
    assert_eq!(state.trucks[&u.id].route, u.path[2..]);
    assert_eq!(state.trucks[&u.id].destination, Some(p(100.0, 0.0)));
    assert_eq!(state.routed, 1);

    // A short remaining route rejoins its final point when none is thirty metres away.
    let mut u = world.unit(1).unwrap();
    u.path = vec![p(10.0, 0.0), p(20.0, 0.0)];
    let mut c = state.trucks.remove(&1).unwrap();
    assert!(state.repair(&u, &mut c, &mut world));
    assert_eq!(world.routes.last().unwrap().1, p(20.0, 0.0));
    assert_eq!(c.route, vec![p(20.0, 0.0)]);
}

#[test]
fn intent_log38_stuck_trucks_within_fifteen_metres_count_as_home_only_with_handoff() {
    let players = [player(0, 0, p(0.0, 0.0))];
    for trip in [TruckState::Collecting, TruckState::Returning] {
        for (distance, stuck, handoff, arrived) in [
            (15.0, 0.51, true, true),
            (15.01, 0.51, true, false),
            (15.0, 0.5, true, false),
            (15.0, 0.51, false, false),
        ] {
            let (mut state, mut reserves, mut world) = setup();
            let source = store(StoreId::Source(0), 0, p(0.0, 0.0), true);
            let id = state.add_truck(0, &source, &mut world);
            state.stores.push(source);
            state.next_schedule = 10000;
            let u = world.units.iter_mut().find(|u| u.id == id).unwrap();
            u.at = p(distance, 0.0);
            u.stuck = stuck;
            u.path = vec![p(0.0, 0.0)];
            u.world_goal = Some(p(0.0, 0.0));
            let c = state.trucks.get_mut(&id).unwrap();
            c.state = trip;
            c.destination = Some(p(0.0, 0.0));
            c.cargo.provisions = if trip == TruckState::Returning {
                5.0
            } else {
                0.0
            };
            world.handoff = handoff;
            state
                .step(context(&players, 1), &mut reserves, &mut world, 0.0)
                .unwrap();
            let expected = if arrived {
                if trip == TruckState::Collecting {
                    TruckState::Loading
                } else {
                    TruckState::Idle
                }
            } else {
                trip
            };
            assert_eq!(
                state.trucks[&id].state, expected,
                "{trip:?}, distance={distance}, stuck={stuck}, handoff={handoff}"
            );
            if arrived {
                assert!(world.unit(id).unwrap().path.is_empty());
                if trip == TruckState::Collecting {
                    assert_eq!(state.trucks[&id].timer, 4.0);
                } else {
                    assert_eq!(state.stores[0].total().provisions, 5.0);
                    assert_eq!(state.trucks[&id].cargo, empty());
                }
            }
        }
    }
}
