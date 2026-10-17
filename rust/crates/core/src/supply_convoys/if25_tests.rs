use super::{intent_tests::*, *};

#[test]
fn if25_convoy_job_comparators_preserve_zero_ties_and_nan_order() {
    let players = [player(0, 0, Point { x: 0.0, z: 0.0 })];
    for (values, expected) in [
        (vec![0.0, -0.0, 0.0, -0.0], vec![9, 8, 7, 6]),
        (
            vec![3.0, f64::NAN, 1.0, -0.0, 0.0, f64::NAN, -1.0],
            vec![9, 8, 3, 6, 5, 7, 4],
        ),
    ] {
        let (mut state, mut reserves, mut world) = setup();
        for (i, provisions) in values.into_iter().enumerate() {
            let id = 9 - i as u32;
            world.units.push(unit(
                id,
                0,
                UnitRole::Other,
                Point {
                    x: 50.0 + i as f64 * 30.0,
                    z: 0.0,
                },
            ));
            reserves.insert(id, reserve(provisions));
        }
        let jobs = state.targets(context(&players, 0), 0, &reserves, &mut world);
        assert_eq!(
            jobs.iter().map(|j| j.unit.unwrap()).collect::<Vec<_>>(),
            expected
        );
        assert!(jobs.iter().all(|j| j.members == vec![j.unit.unwrap()]));
    }
}

#[test]
fn if25_convoy_origins_keep_insertion_ties_and_nan_distance() {
    let players = [player(0, 0, Point { x: 0.0, z: 0.0 })];
    let (mut state, _, _) = setup();
    for (id, x) in [(9, 3.0), (2, f64::NAN), (8, -3.0)] {
        state
            .stores
            .push(store(StoreId::Cache(id), 0, Point { x, z: 0.0 }, true));
    }
    let target = Target {
        id: TargetId::Unit(1),
        unit: Some(1),
        store: None,
        owner: 0,
        at: Point { x: 0.0, z: -0.0 },
        members: vec![1],
        need: Stock::default(),
        priority: Some(0.0),
        relief: None,
    };
    assert_eq!(
        state.origins(context(&players, 0), 0, &target),
        vec![StoreId::Cache(9), StoreId::Cache(2), StoreId::Cache(8)]
    );
}

#[test]
fn if25_convoy_sources_rank_owner_before_preserving_insertion_ties() {
    let players = [
        player(0, 0, Point { x: 0.0, z: 0.0 }),
        player(1, 0, Point { x: 0.0, z: 0.0 }),
        player(2, 1, Point { x: 0.0, z: 0.0 }),
    ];
    let (mut state, _, _) = setup();
    for (id, owner, source, active) in [
        (9, 1, true, true),
        (8, 0, true, true),
        (7, 2, true, true),
        (6, 0, false, true),
        (5, 0, true, false),
        (4, 1, true, true),
        (3, 0, true, true),
    ] {
        let mut row = store(StoreId::Source(id), owner, players[0].spawn, source);
        row.active = active;
        state.stores.push(row);
    }
    assert_eq!(
        state.sources(context(&players, 0), 0),
        [8, 3, 9, 4].map(StoreId::Source)
    );
}

#[test]
fn if25_convoy_local_service_equal_distance_consumes_first_inserted_store() {
    let players = [player(0, 0, Point { x: 0.0, z: 0.0 })];
    for ids in [[9, 2], [2, 9]] {
        let (mut state, mut reserves, mut world) = setup();
        world
            .units
            .push(unit(1, 0, UnitRole::Other, players[0].spawn));
        reserves.insert(1, reserve(0.0));
        for (id, x) in ids.into_iter().zip([-3.0, 3.0]) {
            let mut row = store(StoreId::Cache(id), 0, Point { x, z: -0.0 }, false);
            row.buckets.push((
                0,
                Stock {
                    provisions: 120.0,
                    ..empty()
                },
            ));
            state.stores.push(row);
        }
        state.local_service(context(&players, 0), &mut reserves, &mut world, UNLOAD);
        assert_eq!(reserves.get(1).unwrap().provisions, 120.0);
        assert_eq!(state.stores[0].total().provisions, 0.0);
        assert_eq!(state.stores[1].total().provisions, 120.0);
    }
}

#[test]
fn if25_convoy_return_refresh_and_retreat_share_stable_source_ties() {
    let players = [
        player(0, 0, Point { x: 0.0, z: 0.0 }),
        player(1, 0, Point { x: 0.0, z: 0.0 }),
    ];
    for ids in [[9, 2], [2, 9]] {
        let (mut state, _, mut world) = setup();
        // The allied source precedes ours in storage. Ownership sorting comes
        // first, then equal distance keeps our delivered insertion order.
        state.stores.push(store(
            StoreId::Source(77),
            1,
            Point { x: -30.0, z: 0.0 },
            true,
        ));
        for (id, x) in ids.into_iter().zip([30.0, -30.0]) {
            state
                .stores
                .push(store(StoreId::Source(id), 0, Point { x, z: -0.0 }, true));
        }
        let missing = store(StoreId::Source(99), 0, players[0].spawn, true);
        let truck = state.add_truck(0, &missing, &mut world);
        let u = world.unit(truck).unwrap();
        let mut c = state.trucks.remove(&truck).unwrap();
        state.finish(context(&players, 0), &u, &mut c, &mut world);
        assert_eq!(c.origin, StoreId::Source(ids[0]));
        assert_eq!(c.destination, Some(Point { x: 30.0, z: -0.0 }));
        assert_eq!(c.state, TruckState::Returning);

        c.origin = missing.id;
        c.state = TruckState::WaitingReturn;
        c.destination = None;
        state.trucks.insert(truck, c);
        state.refresh_queue.push_back(truck);
        state.refresh(context(&players, 1), &mut world);
        let c = &state.trucks[&truck];
        assert_eq!(c.origin, StoreId::Source(ids[0]));
        assert_eq!(c.destination, Some(Point { x: 30.0, z: -0.0 }));
        assert_eq!(c.state, TruckState::Returning);

        world.routes.clear();
        assert!(
            state
                .prepare_movement(
                    context(&players, 2),
                    0,
                    &[truck],
                    &MovementCommand::Retreat { queue: false },
                    &mut world,
                )
                .unwrap()
        );
        assert_eq!(world.routes.last().unwrap().1, Point { x: 30.0, z: -0.0 });
        assert_eq!(state.trucks[&truck].state, TruckState::Manual);
    }
}

#[test]
fn if25_convoy_delivery_sorts_members_by_provisions_and_keeps_signed_zero_ties() {
    let players = [player(0, 0, Point { x: 0.0, z: 0.0 })];
    for (members, expected) in [(vec![9, 2], 9), (vec![2, 9], 2)] {
        let (mut state, mut reserves, mut world) = setup();
        for (id, provisions) in [(9, -0.0), (2, 0.0), (1, 10.0)] {
            world
                .units
                .push(unit(id, 0, UnitRole::Other, players[0].spawn));
            reserves.insert(id, reserve(provisions));
        }
        let source = store(StoreId::Source(0), 0, players[0].spawn, true);
        let truck = state.add_truck(0, &source, &mut world);
        let u = world.unit(truck).unwrap();
        let mut c = state.trucks.remove(&truck).unwrap();
        c.cargo.provisions = 5.0;
        c.timer = 10.0;
        c.target = Some(Target {
            id: TargetId::Unit(1),
            unit: Some(1),
            store: None,
            owner: 0,
            at: players[0].spawn,
            members: [vec![1], members].concat(),
            need: empty(),
            priority: Some(0.0),
            relief: None,
        });
        state.deliver(
            context(&players, 0),
            &u,
            &mut c,
            &mut reserves,
            &mut world,
            0.5,
        );
        assert_eq!(reserves.get(expected).unwrap().provisions, 5.0);
        assert_eq!(
            reserves
                .get(if expected == 9 { 2 } else { 9 })
                .unwrap()
                .provisions,
            0.0
        );
        assert_eq!(reserves.get(1).unwrap().provisions, 10.0);
        assert_eq!(c.cargo.provisions, 0.0);
    }
}
