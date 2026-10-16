//! Coarse route behavior. Region labels are compared by membership, never by raw IDs.
use super::*;
use crate::game::MOVE;

fn view<'a>(flags: &'a [u16], heights: Option<&'a [i8]>, version: u64) -> TerrainView<'a> {
    TerrainView {
        width: 160,
        height: 64,
        flags,
        heights,
        world_known: true,
        knowledge_id: 91,
        versions: Versions {
            infantry: version,
            knowledge: version,
            ..Default::default()
        },
        changed: None,
        changes: &[],
    }
}

fn signature(leg: NavigationLeg<'_>) -> (usize, bool, bool, Vec<bool>) {
    (
        leg.goal,
        leg.connected,
        leg.limited,
        (0..160 * 64).map(|c| leg.contains(c)).collect(),
    )
}

#[test]
fn mov37_history_updates_membership_and_bounds_coarse_route_storage() {
    let mut flags = vec![0; 160 * 64];
    let mut nav = Navigation::new();
    let start = 30 * 160 + 10;
    let goal = 30 * 160 + 145;
    let original = signature(
        nav.leg(view(&flags, None, 0), start, goal, MOVE, Options::default())
            .unwrap(),
    );
    assert!(original.1 && original.2);
    let changed = (0..64).map(|y| y * 160 + 32).collect::<Vec<_>>();
    for &c in &changed {
        flags[c] = MOVE;
    }
    let history = [TerrainChange {
        from: 0,
        to: 1,
        cells: &changed,
    }];
    let closed = TerrainView {
        changes: &history,
        ..view(&flags, None, 1)
    };
    assert!(
        !nav.leg(closed, start, goal, MOVE, Options::default())
            .unwrap()
            .connected
    );
    flags[30 * 160 + 32] = 0;
    let opening = [30 * 160 + 32];
    let reopened = TerrainView {
        changed: Some(TerrainChange {
            from: 1,
            to: 2,
            cells: &opening,
        }),
        ..view(&flags, None, 2)
    };
    let actual = signature(
        nav.leg(reopened, start, goal, MOVE, Options::default())
            .unwrap(),
    );
    let fresh = signature(
        Navigation::new()
            .leg(reopened, start, goal, MOVE, Options::default())
            .unwrap(),
    );
    assert_eq!(actual, fresh);
    assert!(actual.1);
    // More than 128 distinct region pairs force route-cache eviction.
    for sy in [2, 18, 34, 50] {
        for sx in (2..160).step_by(16) {
            for gy in [2, 18, 34, 50] {
                for gx in (2..160).step_by(16) {
                    nav.leg(
                        reopened,
                        sy * 160 + sx,
                        gy * 160 + gx,
                        MOVE,
                        Options::default(),
                    )
                    .unwrap();
                    assert!(nav.graphs.values().all(|g| g.routes.len() <= 128));
                }
            }
        }
    }
    assert!(nav.graphs.values().any(|g| g.routes.len() == 128));
    assert_eq!(
        actual,
        signature(
            nav.leg(reopened, start, goal, MOVE, Options::default())
                .unwrap()
        )
    );
    for _ in 0..3 {
        assert_eq!(
            actual,
            signature(
                Navigation::new()
                    .leg(reopened, start, goal, MOVE, Options::default())
                    .unwrap()
            )
        );
    }
}

#[test]
fn mov37_hill_portals_cross_chunks_only_in_the_legal_direction() {
    let mut flags = vec![MOVE; 160 * 64];
    let mut heights = vec![0; flags.len()];
    for (x, y, h) in [(15, 15, 0), (16, 16, 1), (16, 15, 2), (15, 16, 2)] {
        flags[y * 160 + x] = 0;
        heights[y * 160 + x] = h;
    }
    let low = 15 * 160 + 15;
    let high = 16 * 160 + 16;
    let mut nav = Navigation::new();
    let v = view(&flags, Some(&heights), 0);
    assert!(
        nav.leg(v, high, low, MOVE, Options::default())
            .unwrap()
            .connected
    );
    assert!(
        !nav.leg(v, low, high, MOVE, Options::default())
            .unwrap()
            .connected
    );
}
