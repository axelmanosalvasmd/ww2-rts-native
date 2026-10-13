use super::*;

pub(super) fn capture(state: &Match) -> Result<Vec<Map<String, Value>>, SnapshotError> {
    let g = &state.world.game;
    let mut strikes = Vec::with_capacity(g.strikes.len());
    for strike in &g.strikes {
        let unsupported = || SnapshotError::Unsupported {
            unit: 0,
            capability: SnapshotCapability::TeamEffects,
        };
        let timing = strike.runtime.as_ref().ok_or_else(unsupported)?;
        if !matches!(
            timing.kind,
            crate::match_support::SupportKind::Artillery
                | crate::match_support::SupportKind::Smoke
                | crate::match_support::SupportKind::Cover
        ) {
            return Err(unsupported());
        }
        if strike.kind != crate::game::StrikeKind::Other {
            return Err(unsupported());
        }
        if strike.owner >= g.players.len() {
            return Err(SnapshotError::InvalidRecipient);
        }
        finite(&[
            strike.x,
            strike.z,
            strike.dir,
            timing.delay,
            timing.left,
            timing.next,
        ])?;
        if timing.left < 0. {
            return Err(SnapshotError::Rules);
        }
        strikes.push(json!([
            timing.kind.name(),
            r(strike.x),
            r(strike.z),
            r(strike.dir),
            math::max(0., r(timing.delay)),
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
        state.world.game.strikes.clear();
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
        assert_eq!(state.world.game.strikes.len(), 3);
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
        state.world.game.strikes[0].owner = usize::MAX;
        assert!(matches!(
            SnapshotCache::new(&state),
            Err(SnapshotError::InvalidRecipient)
        ));
        state.world.game.strikes[0].owner = 2;
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
    #[test]
    fn unrepresented_support_rows_reject_before_delivery_mutation() {
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
        let original = state.world.game.strikes[0].clone();
        state.world.game.strikes[0].runtime = None;
        assert!(matches!(
            delivery.capture(&mut state, &[false; 3], &[false; 3]),
            Err(SnapshotError::Unsupported {
                capability: SnapshotCapability::TeamEffects,
                ..
            })
        ));
        assert_eq!(delivery, before);
        state.world.game.strikes[0] = original;
        state.world.game.strikes[0].kind = crate::game::StrikeKind::Recon;
        assert!(matches!(
            delivery.capture(&mut state, &[false; 3], &[false; 3]),
            Err(SnapshotError::Unsupported {
                capability: SnapshotCapability::TeamEffects,
                ..
            })
        ));
        assert_eq!(delivery, before);
        state.world.game.strikes[0].runtime.as_mut().unwrap().kind =
            crate::match_support::SupportKind::Recon;
        assert!(matches!(
            delivery.capture(&mut state, &[false; 3], &[false; 3]),
            Err(SnapshotError::Unsupported {
                capability: SnapshotCapability::TeamEffects,
                ..
            })
        ));
        assert_eq!(delivery, before);
    }
}
