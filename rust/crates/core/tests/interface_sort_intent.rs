use ww2_core::{
    commander_ai::{
        self, Input, Level, Observation, PlayerObservation, Point, PointObservation, PolicyError,
        SourceState, UnitObservation,
    },
    match_commands::Command,
};

fn input() -> Input {
    Input {
        width: 40,
        height: 40,
        tick: 0,
        slot: 0,
        players: vec![PlayerObservation {
            team: 0,
            out: false,
            spawn: Point { x: 0.0, z: 0.0 },
        }],
        units: vec![UnitObservation {
            id: 30,
            kind: "rifle".into(),
            owner: 0,
            at: Point { x: 0.0, z: -0.0 },
            hp: 100.0,
            supp: -0.0,
            target_id: 0,
            cover: 0,
            path: vec![],
            attack_id: false,
            retreating: false,
            riding: false,
            build: false,
            dig: false,
            entrench: false,
            garrison: false,
            enter: false,
            fire_at: false,
            nade: false,
        }],
        points: vec![
            PointObservation {
                at: Point { x: 40.0, z: 30.0 },
                vp: 1.0,
                mp: 0.0,
                owner: None,
                cut: false,
                locked: false,
            },
            PointObservation {
                at: Point { x: 30.0, z: 40.0 },
                vp: 1.0,
                mp: -0.0,
                owner: None,
                cut: false,
                locked: false,
            },
        ],
        flags: vec![0; 1600],
        chars: vec![b'.'; 1600],
        manpower: 0.0,
        research: None,
    }
}

#[test]
fn native_ai_equal_scores_keep_first_observed_point_with_signed_zero() {
    let observation = Observation::new(input()).unwrap();
    let mut state = SourceState::default();
    let plan =
        commander_ai::plan_with_fractions(&observation, &mut state, Level::Normal, &[0.0]).unwrap();
    assert_eq!(state.aim.unwrap().i, 0);
    let Command::Move { orders, .. } = &plan.commands[0] else {
        panic!("expected move")
    };
    assert_eq!(orders.len(), 1);
    assert_eq!(orders[0].unit, 30);
    assert_eq!(orders[0].at, Point { x: 44.0, z: 30.0 });
}

#[test]
fn native_ai_rejects_nonfinite_observations_before_policy_selection() {
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut frame = input();
        frame.points[0].vp = value;
        assert_eq!(Observation::new(frame).unwrap_err(), PolicyError::Points);
        let mut frame = input();
        frame.units[0].at.x = value;
        assert_eq!(Observation::new(frame).unwrap_err(), PolicyError::Position);
    }
}

#[test]
fn native_ai_skips_nan_score_produced_by_finite_observation_overflow() {
    let mut frame = input();
    // Both inputs are finite, but the weighted terms become -Infinity + Infinity.
    frame.points[0].vp = f64::MAX;
    frame.points[0].mp = -f64::MAX;
    let observation = Observation::new(frame).unwrap();
    let mut state = SourceState::default();
    let plan =
        commander_ai::plan_with_fractions(&observation, &mut state, Level::Normal, &[0.0]).unwrap();
    assert_eq!(state.aim.unwrap().i, 1);
    let Command::Move { orders, .. } = &plan.commands[0] else {
        panic!("expected move")
    };
    assert_eq!(orders[0].at, Point { x: 34.0, z: 40.0 });
}
