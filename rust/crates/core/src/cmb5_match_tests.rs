//! Shared full-match checks for the combat batch's ordinary runtime state.
use crate::{match_end::EndReason, match_runtime::Match, recipient_snapshot::SnapshotCache};

pub(crate) fn prepare_finish(m: &mut Match) {
    assert_eq!(m.world.game.tick, 0);
    assert!(m.finish.is_none());
    // An authored five-VP objective runs for at least 100 ordinary simulation ticks.
    m.start.win_vp = 5.;
    let point = &mut m.start.points[0];
    point.at = m.start.players[0].spawn;
    point.owner = Some(0);
    point.vp = 1.;
    point.mp = 0.;
    point.cut = false;
}

pub(crate) fn through_finish(m: &mut Match, check: impl Fn(&Match, &SnapshotCache<'_>)) {
    let first = m.world.game.tick;
    let mut finish_tick = None;
    let mut deliveries = 0;
    for _ in 0..200 {
        m.advance_tick().expect("ordinary match must keep ticking");
        assert_eq!(m.tick_state.fault(), None);
        let cache = SnapshotCache::new(m).expect("all canonical snapshot state must admit");
        for slot in 0..m.world.game.players.len() {
            cache
                .recipient(slot)
                .expect("every seat receives every tick");
            deliveries += 1;
        }
        check(m, &cache);
        if let Some(finish) = &m.finish {
            assert_eq!(finish.winner, 0);
            assert_eq!(finish.reason, EndReason::VictoryPoints);
            let end = *finish_tick.get_or_insert(m.world.game.tick);
            if m.world.game.tick >= end + 8 {
                break;
            }
        }
    }
    let end = finish_tick.expect("ordinary VP income must finish the match");
    assert!(end >= 100);
    assert!(m.world.game.tick >= end + 8);
    assert_eq!(
        deliveries,
        (m.world.game.tick - first) as usize * m.world.game.players.len()
    );
    eprintln!(
        "finish tick {end}, final tick {}, {deliveries} recipient snapshots",
        m.world.game.tick
    );
}
