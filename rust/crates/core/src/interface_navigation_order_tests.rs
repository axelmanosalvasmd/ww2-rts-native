use super::*;
use crate::game::MOVE;

#[test]
fn if22_navigation_edges_reinsert_last_and_route_cache_evicts_oldest_inserted() {
    let flags = vec![0; 260 * 2];
    let view = TerrainView {
        width: 260,
        height: 2,
        flags: &flags,
        heights: None,
        world_known: false,
        knowledge_id: 1,
        versions: Versions::default(),
        changed: None,
        changes: &[],
    };
    let mut graph = Graph::build(view, MOVE, 2);
    assert_eq!(
        graph
            .region(2)
            .edges
            .iter()
            .map(|edge| edge.region)
            .collect::<Vec<_>>(),
        vec![1, 3]
    );
    // A replacement crossing updates its edge in place. A retired edge is appended on return.
    let crossing_count = graph.region(2).edges[0].crossings.len();
    graph.add(2, 1);
    assert_eq!(graph.region(2).edges[0].crossings.len(), crossing_count + 1);
    graph.region_mut(2).edges.retain(|edge| edge.region != 1);
    graph.add(2, 1);
    assert_eq!(
        graph
            .region(2)
            .edges
            .iter()
            .map(|edge| edge.region)
            .collect::<Vec<_>>(),
        vec![3, 1]
    );
    assert_eq!(
        graph
            .region(1)
            .incoming
            .iter()
            .filter(|&&id| id == 2)
            .count(),
        1
    );

    for goal in 2..=129 {
        assert_eq!(
            graph.route(1, goal).unwrap(),
            (1..=goal).collect::<Vec<_>>()
        );
    }
    assert_eq!(graph.routes.len(), 128);
    assert_eq!(graph.route(1, 2), Some(vec![1, 2]));
    assert_eq!(graph.routes[0].goal, 2, "cache hits preserve insertion age");
    assert_eq!(graph.route(1, 130).unwrap(), (1..=130).collect::<Vec<_>>());
    assert_eq!(graph.routes[0].goal, 3);
    assert_eq!(graph.routes.last().unwrap().goal, 130);
    assert_eq!(graph.route(1, 2), Some(vec![1, 2]));
    assert_eq!(graph.routes[0].goal, 4);
    assert_eq!(graph.routes.last().unwrap().goal, 2);
}
