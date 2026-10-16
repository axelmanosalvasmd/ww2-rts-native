use super::*;
use crate::{
    game::{Point, StrikeKind},
    match_support::SupportKind,
};

#[test]
fn every_support_kind_projects_canonical_strike_state_for_each_seat() {
    let fixture: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/recipient-snapshot.json")).unwrap();
    let mut state = super::super::tests::initialize(&fixture["cases"][0]);
    state.world.game.strikes.clear();
    for (index, kind) in [
        SupportKind::Recon,
        SupportKind::Strafe,
        SupportKind::Bombing,
        SupportKind::Dive,
        SupportKind::Para,
        SupportKind::Artillery,
        SupportKind::Smoke,
        SupportKind::Cover,
    ]
    .into_iter()
    .enumerate()
    {
        for player in &mut state.start.players {
            for cooldown in player.support.values_mut() {
                *cooldown = 0.;
            }
        }
        let owner = index % state.start.players.len();
        state.start.economy.players[owner].mp = 10000.;
        let at = Point { x: 14.15, z: 16.25 };
        assert_eq!(
            state
                .admit_support(owner, kind.name(), at, Some(-0.15))
                .unwrap(),
            None
        );
        let strike = state.world.game.strikes.last_mut().unwrap();
        // The public row clamps an expired arrival timer to zero seconds.
        strike.runtime.as_mut().unwrap().delay = -0.15;
    }
    let expected = json!([
        ["recon", 14.2, 16.3, -0.1, 0., 0],
        ["strafe", 14.2, 16.3, -0.1, 0., 1],
        ["bombing", 14.2, 16.3, -0.1, 0., 2],
        ["dive", 14.2, 16.3, -0.1, 0., 0],
        ["para", 14.2, 16.3, -0.1, 0., 1],
        ["artillery", 14.2, 16.3, -0.1, 0., 2],
        ["smoke", 14.2, 16.3, -0.1, 0., 0],
        ["cover", 14.2, 16.3, -0.1, 0., 1]
    ]);
    let cache = SnapshotCache::new(&state).unwrap();
    for slot in 0..state.start.players.len() {
        let fields = cache.recipient(slot).unwrap().fields;
        assert_eq!(fields["strikes"], expected);
        assert!(fields["sup"].is_object());
    }
    state.world.game.strikes[0].kind = StrikeKind::Other;
    assert!(matches!(
        SnapshotCache::new(&state),
        Err(SnapshotError::Unsupported {
            capability: SnapshotCapability::TeamEffects,
            ..
        })
    ));
}

#[test]
fn plane_support_lifecycle_and_events_refuse_before_delivery_publication() {
    let fixture: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/recipient-snapshot.json")).unwrap();
    for kind in [
        SupportKind::Recon,
        SupportKind::Strafe,
        SupportKind::Bombing,
        SupportKind::Dive,
        SupportKind::Para,
    ] {
        let mut state = super::super::tests::initialize(&fixture["cases"][0]);
        state.world.game.strikes.clear();
        state.start.economy.players[0].mp = 10000.;
        state.start.players[0]
            .support
            .insert(kind.name().into(), 0.);
        let at = state
            .world
            .game
            .units
            .values()
            .find(|u| u.owner == 0)
            .unwrap()
            .at();
        assert_eq!(
            state.admit_support(0, kind.name(), at, Some(0.)).unwrap(),
            None
        );
        let mut delivery = DeliveryState::new(&state).unwrap();
        let before = delivery.clone();
        let selected = (0..state.start.players.len())
            .map(|slot| SelectedSeat {
                slot,
                full: false,
                omit: false,
            })
            .collect::<Vec<_>>();
        assert!(!SUPPORTED_SUPPORT_KINDS.contains(&kind));
        assert!(matches!(
            delivery.capture_selected_with_tick_events(&mut state, &selected, &[]),
            Err(SnapshotError::Unsupported {
                capability: SnapshotCapability::TeamEffects,
                ..
            })
        ));
        assert_eq!(delivery, before);
        assert!(matches!(
            state.preflight_ordinary_snapshot(),
            Err(SnapshotError::Unsupported {
                capability: SnapshotCapability::TeamEffects,
                ..
            })
        ));
        // Even once a strike has retired, its uncontracted arrival event is
        // retained by the host and must fail without publishing delivery state.
        state.world.game.strikes.clear();
        let event =
            crate::match_tick::TickEvent::Support(crate::match_support::SupportEvent::Plane {
                kind,
                at: Point { x: 1., z: 1. },
                direction: 0.,
                owner: 0,
            });
        assert!(matches!(
            delivery.capture_selected_with_tick_events(&mut state, &selected, &[event]),
            Err(SnapshotError::Unsupported {
                capability: SnapshotCapability::Air,
                ..
            })
        ));
        assert_eq!(delivery, before);
    }
}
