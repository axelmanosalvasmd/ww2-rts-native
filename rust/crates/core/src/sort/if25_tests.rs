use super::{sort_by, sort_by_ref};

#[test]
fn if25_numeric_difference_matches_pinned_prototype_for_zero_nan_and_infinity() {
    // e4e579a supply-convoys.js:246 comparator, evaluated with Node 24.21.0.
    for (keys, expected) in [
        (vec![0.0, -0.0, 0.0, -0.0], vec![0, 1, 2, 3]),
        (
            vec![3.0, f64::NAN, 1.0, -0.0, 0.0, f64::NAN, -1.0],
            vec![0, 1, 6, 3, 4, 2, 5],
        ),
        (
            vec![
                f64::INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::NAN,
                1.0,
                -0.0,
                0.0,
            ],
            vec![2, 5, 6, 4, 0, 1, 3],
        ),
    ] {
        let mut ids: Vec<_> = (0..keys.len()).collect();
        sort_by(&mut ids, |a, b| keys[a] - keys[b]);
        assert_eq!(ids, expected);
        let mut owned: Vec<_> = keys
            .iter()
            .enumerate()
            .map(|(i, &v)| (i.to_string(), v))
            .collect();
        sort_by_ref(&mut owned, |a, b| a.1 - b.1);
        assert_eq!(
            owned
                .iter()
                .map(|a| a.0.parse::<usize>().unwrap())
                .collect::<Vec<_>>(),
            expected
        );
        for (row, &id) in owned.iter().zip(&expected) {
            assert_eq!(row.1.to_bits(), keys[id].to_bits());
        }
    }
}

#[test]
fn if25_nan_merge_order_matches_pinned_prototype_above_one_run() {
    let keys: Vec<_> = (0..96)
        .map(|i| {
            if i % 17 == 0 {
                f64::NAN
            } else {
                ((i * 37) % 13) as f64 - 6.0
            }
        })
        .collect();
    let mut ids: Vec<_> = (0..keys.len()).collect();
    sort_by(&mut ids, |a, b| keys[a] - keys[b]);
    assert_eq!(
        ids,
        [
            0, 13, 26, 39, 52, 65, 78, 91, 6, 19, 32, 45, 58, 71, 84, 12, 25, 38, 64, 77, 90, 5,
            18, 31, 44, 57, 70, 83, 11, 24, 37, 50, 63, 76, 89, 4, 30, 43, 56, 69, 82, 95, 10, 23,
            36, 49, 62, 75, 88, 3, 16, 29, 42, 55, 9, 22, 35, 48, 2, 15, 28, 41, 8, 21, 47, 1, 14,
            27, 40, 7, 17, 20, 33, 34, 46, 51, 81, 94, 61, 74, 87, 54, 67, 80, 93, 60, 73, 53, 66,
            59, 68, 86, 79, 92, 72, 85
        ]
    );
    let mut owned: Vec<_> = keys
        .iter()
        .enumerate()
        .map(|(i, &v)| (i.to_string(), v))
        .collect();
    sort_by_ref(&mut owned, |a, b| a.1 - b.1);
    assert_eq!(
        owned
            .iter()
            .map(|v| v.0.parse::<usize>().unwrap())
            .collect::<Vec<_>>(),
        ids
    );
}

#[test]
fn if25_finite_consumer_keys_compare_without_nan_and_keep_ties() {
    let mut rows: [(u32, f64); 5] = [(9, 2.0), (2, -0.0), (8, 0.0), (1, 2.0), (3, -1.0)];
    let mut comparisons = 0;
    sort_by(&mut rows, |a, b| {
        let difference = a.1 - b.1;
        assert!(!difference.is_nan());
        comparisons += 1;
        difference
    });
    assert!(comparisons > 0);
    assert_eq!(rows.map(|v| v.0), [3, 2, 8, 9, 1]);
}

#[test]
fn if25_sorted_first_and_chained_comparators_follow_v8_nan_and_stable_ties() {
    let rows = [(9, 3.0), (2, f64::NAN), (8, 1.0)];
    assert_eq!(super::first_by(rows, |a, b| a.1 - b.1).unwrap().0, 9);
    let rows = [(9, -0.0), (2, 0.0)];
    assert_eq!(super::first_by(rows, |a, b| a.1 - b.1).unwrap().0, 9);
    for first in [0.0, -0.0, f64::NAN] {
        assert_eq!(super::then(first, -7.0), -7.0);
    }
    assert_eq!(super::then(f64::INFINITY, -7.0), f64::INFINITY);
    let mut rows = [(9_u32, f64::NAN), (2, 0.0)];
    sort_by(&mut rows, |a, b| {
        super::then(a.1 - b.1, f64::from(a.0) - f64::from(b.0))
    });
    assert_eq!(rows.map(|r| r.0), [2, 9]);
}
