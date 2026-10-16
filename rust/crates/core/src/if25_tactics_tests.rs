use super::{tests::*, *};
use crate::commander_ai::{PointObservation, PolicyError};

#[test]
fn if25_ai_point_scores_keep_strict_scan_semantics_when_finite_inputs_overflow() {
    let mut input = fixture(vec![unit(1, "rifle", 40.0)]);
    input.points[0].vp = 1e308;
    input.points[0].mp = -1e308;
    // Both inputs are finite, but their weighted sum is NaN.
    assert!(Observation::new(input.clone()).is_ok());
    let mut state = State::default();
    let commands = run(input.clone(), &mut state, Level::Normal);
    assert_eq!(state.aim, None);
    assert!(!has(&commands, "amove"));
    input.points.push(PointObservation {
        at: Point { x: 130.0, z: 20.0 },
        vp: 1.0,
        mp: 1.0,
        owner: None,
        cut: false,
        locked: false,
    });
    let mut state = State::default();
    run(input, &mut state, Level::Normal);
    assert_eq!(state.aim.map(|(i, _)| i), Some(1));
    assert_eq!(least_score([0, 1, 2].into_iter(), |_| -0.0), Some(0));
    assert_eq!(least_score([0, 1].into_iter(), |_| f64::INFINITY), None);
}

#[test]
fn if25_ai_observation_rejects_nan_score_inputs_and_positions() {
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut input = fixture(vec![unit(1, "rifle", 40.0)]);
        input.units[0].at.x = value;
        assert!(matches!(
            Observation::new(input),
            Err(PolicyError::Position)
        ));
        let mut input = fixture(vec![unit(1, "rifle", 40.0)]);
        input.points[0].vp = value;
        assert!(matches!(Observation::new(input), Err(PolicyError::Points)));
        let mut input = fixture(vec![unit(1, "rifle", 40.0)]);
        input
            .runtime
            .as_mut()
            .unwrap()
            .units
            .get_mut(&1)
            .unwrap()
            .stock = Some(Stock {
            ammo: None,
            provisions: value,
            fuel: None,
            forced: false,
        });
        assert!(matches!(
            Observation::new(input),
            Err(PolicyError::RuntimeContext)
        ));
    }
}
