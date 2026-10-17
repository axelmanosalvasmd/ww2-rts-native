use super::*;
fn order(id: i64) -> MoveOrder {
    MoveOrder {
        unit: id,
        at: Point { x: 40.0, z: 50.0 },
    }
}
fn situation(kind: SituationKind) -> Situation {
    Situation {
        kind,
        until: 20.0,
        point: Some(2),
        threat_id: Some(9),
        at: Some(Point { x: 40.0, z: 50.0 }),
        counter: Some("at".into()),
        safe: None,
        failed_at: 1,
    }
}
#[test]
fn intent_ai47_emission_order_and_complete_decision_survive_the_next_look() {
    let mut state = LookState::default();
    let before = Command::Ability {
        ids: vec![1],
        at: None,
        queue: false,
        automatic: false,
    };
    let operation = Command::Cover {
        ids: vec![4],
        queue: false,
        at: None,
    };
    let result = state
        .finish(
            10.0,
            Some(situation(SituationKind::Push)),
            Some("tank".into()),
            Some(3),
            &[0, 0, 4],
            LookOrders {
                immediate: vec![before.clone()],
                retreat: vec![1],
                moves: vec![order(2)],
                attack_moves: vec![order(3)],
                operation: vec![operation.clone()],
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(result[0], before);
    assert!(matches!(&result[1],Command::Retreat {ids} if ids==&[1]));
    assert!(matches!(&result[2],Command::Move {attack_move:false,orders,..} if orders[0].unit==2));
    assert!(matches!(&result[3],Command::Move {attack_move:true,orders,..} if orders[0].unit==3));
    assert_eq!(result[4], operation);
    assert_eq!(state.operation.as_ref().unwrap().failed_at, 4);
    assert_eq!(
        state.decision,
        Some(Decision {
            situation: Some(SituationKind::Push),
            action: Some(Action::Take),
            buy: Some("tank".into()),
            aim: Some(3)
        })
    );
    let previous = state.operation.clone();
    assert!(
        state
            .finish(11.0, None, None, None, &[], LookOrders::default())
            .unwrap()
            .is_empty()
    );
    assert_eq!(state.operation, previous);
    assert_eq!(
        state.decision,
        Some(Decision {
            situation: None,
            action: None,
            buy: None,
            aim: None
        })
    );
}
#[test]
fn intent_ai47_claims_and_unexpired_manual_releases_filter_ground_orders_only() {
    let mut state = LookState::default();
    let result = state
        .finish(
            10.0,
            None,
            None,
            None,
            &[],
            LookOrders {
                retreat: vec![1, 2, 3],
                moves: vec![order(1), order(2), order(3)],
                attack_moves: vec![order(1), order(2), order(3)],
                claims: BTreeSet::from([1]),
                released_until: BTreeMap::from([(2, 10.01), (3, 10.0)]),
                operation: vec![Command::Stop { ids: vec![1] }],
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(result.len(), 4);
    assert!(matches!(&result[0],Command::Retreat {ids} if ids==&[3]));
    for command in &result[1..3] {
        assert!(matches!(command,Command::Move {orders,..} if orders.len()==1&&orders[0].unit==3));
    }
    assert_eq!(result[3], Command::Stop { ids: vec![1] });
}
#[test]
fn intent_ai47_action_and_aim_follow_situation_without_replacing_an_explicit_aim() {
    for (kind, safe, action, expected) in [
        (SituationKind::Defense, None, Action::Hold, 2),
        (SituationKind::Wait, None, Action::Prepare, 2),
        (SituationKind::Wait, Some(1), Action::TakeOther, 2),
        (SituationKind::Push, None, Action::Take, 2),
    ] {
        let mut sit = situation(kind);
        sit.safe = safe;
        let mut state = LookState::default();
        state
            .finish(1.0, Some(sit), None, None, &[], LookOrders::default())
            .unwrap();
        let decision = state.decision.unwrap();
        assert_eq!(decision.action, Some(action));
        assert_eq!(decision.aim, Some(expected));
    }
    let mut sit = situation(SituationKind::Wait);
    sit.point = None;
    sit.safe = Some(1);
    let mut state = LookState::default();
    state
        .finish(1.0, Some(sit), None, None, &[], LookOrders::default())
        .unwrap();
    assert_eq!(state.decision.unwrap().aim, Some(1));
}
#[test]
fn invalid_completion_cannot_overwrite_the_private_operation_or_decision() {
    let mut state = LookState::default();
    state
        .finish(
            1.0,
            Some(situation(SituationKind::Push)),
            None,
            None,
            &[],
            LookOrders::default(),
        )
        .unwrap();
    let before = state.clone();
    assert!(
        state
            .finish(f64::NAN, None, None, None, &[], LookOrders::default())
            .is_err()
    );
    assert_eq!(state, before);
}
