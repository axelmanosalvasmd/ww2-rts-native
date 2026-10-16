use ww2_core::{
    field_works::{self, FortKind, Pattern},
    game::Point,
    recipient_snapshot::DeliveryState,
};

#[test]
fn if25_sim_entrench_sort_matches_original_ties_and_rejects_nan() {
    let a = Point { x: 20.0, z: 20.0 };
    let b = Point { x: 40.0, z: 20.0 };
    // Original entrenchPlan at e4e579a, evaluated directly in Node 24.21.0.
    let expected = [
        (
            Pattern::Line,
            vec![(30.0, 20.0), (22.0, 20.0), (38.0, 20.0)],
        ),
        (
            Pattern::Double,
            vec![
                (30.0, 20.0),
                (30.0, 26.0),
                (22.0, 20.0),
                (38.0, 20.0),
                (22.0, 26.0),
                (38.0, 26.0),
            ],
        ),
    ];
    for (pattern, expected) in expected {
        let actual = field_works::entrench_plan(pattern, a, b, None, FortKind::Trench).unwrap();
        assert_eq!(
            actual.iter().map(|s| (s.at.x, s.at.z)).collect::<Vec<_>>(),
            expected
        );
        assert!(
            actual
                .iter()
                .all(|s| s.direction.to_bits() == 0.0_f64.to_bits()
                    && s.front.to_bits() == (-std::f64::consts::FRAC_PI_2).to_bits())
        );
    }
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(
            field_works::entrench_plan(
                Pattern::Line,
                Point { x: value, z: 20.0 },
                b,
                None,
                FortKind::Trench
            )
            .is_err()
        );
    }
}

#[test]
fn if25_ordinary_matches_keep_ticks_and_complete_snapshots_admitted() {
    let mut state = super::interface_runtime_batch3::make(2501);
    let mut delivery = DeliveryState::new(&state).unwrap();
    for tick in 1..=8 {
        state.advance_tick().unwrap();
        let captured = delivery
            .capture_quiescent(&mut state, &[true, true], &[false, false], &[])
            .unwrap();
        for slot in 0..2 {
            captured.recipient(slot).unwrap().into_json();
        }
        assert_eq!(state.world.game.tick, tick);
        assert!(state.tick_state.fault().is_none());
    }
    state.options.logistics = true;
    assert!(state.validate_tick_state().unwrap_err().contains("convoy"));
}
