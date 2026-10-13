use super::*;

pub(super) fn capture(state: &Match) -> Result<Vec<Map<String, Value>>, SnapshotError> {
    let g = &state.world.game;
    // Coarse Game strike rows represent actor kinds outside the verified ground
    // provider. They cannot substitute for its exact kind and delay metadata.
    if !g.strikes.is_empty() {
        return Err(SnapshotError::Unsupported {
            unit: 0,
            capability: SnapshotCapability::TeamEffects,
        });
    }
    let mut strikes = Vec::with_capacity(state.support.strikes.len());
    for strike in &state.support.strikes {
        if strike.owner >= g.players.len() {
            return Err(SnapshotError::InvalidRecipient);
        }
        finite(&[
            strike.at.x,
            strike.at.z,
            strike.direction,
            strike.delay,
            strike.left,
            strike.next,
        ])?;
        if strike.left < 0. {
            return Err(SnapshotError::Rules);
        }
        strikes.push(json!([
            strike.kind.name(),
            r(strike.at.x),
            r(strike.at.z),
            r(strike.direction),
            math::max(0., r(strike.delay)),
            strike.owner
        ]));
    }
    for cover in &state.support.covers {
        if cover.owner >= g.players.len() || !(0..6).contains(&cover.team) || cover.radius < 0. {
            return Err(SnapshotError::Rules);
        }
        finite(&[cover.at.x, cover.at.z, cover.radius, cover.remaining])?;
    }
    Ok(g.players
        .iter()
        .map(|player| {
            let covers = state
                .support
                .covers
                .iter()
                .filter(|cover| cover.team == player.team)
                .map(|cover| {
                    json!([
                        r(cover.at.x),
                        r(cover.at.z),
                        cover.radius,
                        cover.remaining.ceil()
                    ])
                })
                .collect::<Vec<_>>();
            Map::from_iter([
                ("strikes".into(), json!(strikes)),
                ("covers".into(), json!(covers)),
            ])
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::Point;
    #[test]
    fn actual_support_admission_projects_public_strikes_and_owner_cooldowns() {
        let fixture: Value =
            serde_json::from_str(include_str!("../../tests/fixtures/recipient-snapshot.json"))
                .unwrap();
        let case = fixture["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == "public:support-calls")
            .unwrap();
        let mut state = super::super::tests::initialize(case);
        state.support = Default::default();
        for slot in 0..3 {
            state.start.economy.players[slot].mp = 1000.;
            for cooldown in state.start.players[slot].support.values_mut() {
                *cooldown = 0.;
            }
        }
        for (owner, kind) in [(0, "artillery"), (1, "smoke"), (2, "cover")] {
            assert_eq!(
                state
                    .admit_support(
                        owner,
                        kind,
                        Point {
                            x: 14.15 + owner as f64,
                            z: 16.25
                        },
                        Some(-0.05)
                    )
                    .unwrap(),
                None
            );
        }
        let cache = SnapshotCache::new(&state).unwrap();
        for slot in 0..3 {
            super::super::tests::equal(
                &Value::Object(cache.recipient(slot).unwrap().fields),
                &case["outputs"][slot],
                "actual support admission",
            );
        }
        assert_eq!(state.support.strikes.len(), 3);
        assert!(state.support.events.is_empty());
    }
    #[test]
    fn support_and_finish_errors_reject_before_delivery_mutation() {
        let fixture: Value =
            serde_json::from_str(include_str!("../../tests/fixtures/recipient-snapshot.json"))
                .unwrap();
        let case = fixture["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == "public:support-actors")
            .unwrap();
        let mut state = super::super::tests::initialize(case);
        let mut delivery = DeliveryState::new(&state).unwrap();
        let before = delivery.clone();
        state.support.covers[0].remaining = f64::NAN;
        assert!(matches!(
            delivery.capture(&mut state, &[false; 3], &[false; 3]),
            Err(SnapshotError::InvalidNumber)
        ));
        assert_eq!(delivery, before);
        state.support.covers[0].remaining = 2.01;
        state.support.strikes[0].owner = usize::MAX;
        assert!(matches!(
            SnapshotCache::new(&state),
            Err(SnapshotError::InvalidRecipient)
        ));
        state.support.strikes[0].owner = 2;
        state.finish = Some(crate::match_end::MatchFinish {
            winner: 0,
            reason: crate::match_end::EndReason::VictoryPoints,
            end_at: Point { x: f64::NAN, z: 1. },
            end_tick: 0,
        });
        assert!(matches!(
            delivery.capture(&mut state, &[false; 3], &[false; 3]),
            Err(SnapshotError::InvalidNumber)
        ));
        assert_eq!(delivery, before);
    }
}
