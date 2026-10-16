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
