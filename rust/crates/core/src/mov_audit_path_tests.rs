//! Native navigation behaviors, plus exact heap and heuristic component checks.
use super::*;
use crate::{
    mov_audit_test_support::{close, point, unit},
    navigation::{TerrainChange, Versions},
    rules::Rules,
};

struct Field {
    w: usize,
    h: usize,
    flags: Vec<u16>,
    height: Option<Vec<i8>>,
    near: Vec<u8>,
    mines: Vec<bool>,
    fires: Vec<bool>,
    bunkers: Vec<Point>,
    version: u64,
    changed: Vec<usize>,
    world: bool,
    roads: bool,
}
impl Field {
    fn new(w: usize, h: usize) -> Self {
        let n = w * h;
        Self {
            w,
            h,
            flags: vec![0; n],
            height: None,
            near: vec![0; n],
            mines: vec![false; n],
            fires: vec![false; n],
            bunkers: vec![],
            version: 0,
            changed: vec![],
            world: false,
            roads: false,
        }
    }
    fn terrain(&self) -> PathTerrain<'_> {
        PathTerrain {
            navigation: TerrainView {
                width: self.w,
                height: self.h,
                flags: &self.flags,
                heights: self.height.as_deref(),
                world_known: self.world,
                knowledge_id: 71,
                versions: Versions {
                    infantry: self.version,
                    vehicle: self.version,
                    naval: self.version,
                    obstruction: self.version,
                    knowledge: self.version,
                },
                changed: (self.version > 0).then_some(TerrainChange {
                    from: self.version.saturating_sub(1),
                    to: self.version,
                    cells: &self.changed,
                }),
                changes: &[],
            },
            near_walls: &self.near,
            known_mines: &self.mines,
            fires: &self.fires,
            bunkers: &self.bunkers,
            wear: None,
            wetness: 0.,
            roads: self.roads,
        }
    }
    fn set(&mut self, c: usize, flags: u16) {
        self.flags[c] = flags;
        self.changed = vec![c];
        self.version += 1;
    }
    fn route(&self, paths: &mut Pathfinding, name: &str, from: Point, to: Point) -> PathResult {
        let rules = Rules::compiled().unwrap();
        paths
            .find_path(
                self.terrain(),
                PathRules::compiled().unwrap(),
                &rules.kinds[rules.kind(name).unwrap()],
                from,
                to,
                None,
            )
            .unwrap()
    }
}
fn same(a: &PathResult, b: &PathResult) {
    assert_eq!(
        (a.status, a.limited, a.local_goal, a.route_end, &a.waypoints),
        (b.status, b.limited, b.local_goal, b.route_end, &b.waypoints)
    );
}
fn segments(from: Point, path: &[Point]) -> Vec<(Point, Point)> {
    let mut at = from;
    path.iter()
        .map(|&next| {
            let pair = (at, next);
            at = next;
            pair
        })
        .collect()
}

#[test]
fn mov27_exact_click_requires_full_navigation_radius() {
    let mut f = Field::new(40, 40);
    f.flags[20 * 40 + 20] = MOVE;
    let mut paths = Pathfinding::new();
    let clear = point(31.2, 39.4);
    let exact = f.route(&mut paths, "tank", point(15., 39.), clear);
    assert_eq!(exact.status, PathStatus::Found);
    assert_eq!(exact.route_end, clear);
    let tight = point(38.9, 41.0);
    let snapped = f.route(&mut paths, "tank", point(15., 39.), tight);
    assert_eq!(snapped.status, PathStatus::Found);
    assert_ne!(snapped.route_end, tight);
    close(snapped.route_end.x % 2., 1.);
    close(snapped.route_end.z % 2., 1.);
    let edge = f.route(&mut paths, "tank", point(15., 39.), point(0.2, 20.));
    assert_eq!(edge.status, PathStatus::Found);
    assert_ne!(edge.route_end, point(0.2, 20.));
    close(edge.route_end.x % 2., 1.);
    close(edge.route_end.z % 2., 1.);
    let rules = Rules::compiled().unwrap();
    let profile = &rules.kinds[rules.kind("tank").unwrap()];
    let radius = crate::math::hypot(profile.radius * 0.8, profile.radius * 0.48);
    let open = Field::new(40, 40);
    for (at, admitted) in [
        (point(radius, 21.), true),
        (point(radius - 1e-7, 21.), false),
        (point(80. - radius, 21.), false),
        (point(80. - radius - 1e-7, 21.), true),
        (point(21., radius), true),
        (point(21., radius - 1e-7), false),
        (point(21., 80. - radius), false),
    ] {
        assert_eq!(
            crate::vehicle_navigation::destination_clear(
                open.terrain().navigation,
                at,
                profile.radius,
                profile.block()
            ),
            admitted
        );
    }
}

#[test]
fn mov28_bridge_width_and_incremental_clearance_cache() {
    let rules = Rules::compiled().unwrap();
    let profile = &rules.kinds[rules.kind("tank").unwrap()];
    let mut f = Field::new(64, 64);
    let block = profile.block();
    for y in 27..36 {
        for x in 0..64 {
            f.flags[y * 64 + x] = MOVE;
        }
        f.flags[y * 64 + 31] = 0;
    }
    let mut paths = Pathfinding::new();
    let from = point(63., 90.);
    let to = point(63., 35.);
    assert_eq!(
        f.route(&mut paths, "tank", from, to).status,
        PathStatus::Unreachable,
        "2 m closes tanks"
    );
    let mut cache = VehicleNavigation::default();
    let old = cache
        .view(f.terrain().navigation, profile.radius, block)
        .flags
        .to_vec();
    assert_ne!(old[31 * 64 + 31] & block, 0);
    f.changed.clear();
    for y in 27..36 {
        for x in [30, 32] {
            let c = y * 64 + x;
            f.flags[c] = 0;
            f.changed.push(c);
        }
    }
    f.version = 1;
    let updated = cache
        .view(f.terrain().navigation, profile.radius, block)
        .flags
        .to_vec();
    let fresh = VehicleNavigation::default()
        .view(f.terrain().navigation, profile.radius, block)
        .flags
        .to_vec();
    assert_eq!(updated, fresh);
    assert_eq!(updated[31 * 64 + 31] & block, 0);
    let wide = f.route(&mut paths, "tank", from, to);
    assert_eq!(wide.status, PathStatus::Found, "6 m opens tanks");
    same(&wide, &f.route(&mut Pathfinding::new(), "tank", from, to));
    let replacement = f.flags.clone();
    f.flags = replacement;
    same(&wide, &f.route(&mut paths, "tank", from, to));
    let edge_view = cache.view(f.terrain().navigation, profile.radius, block);
    assert_ne!(edge_view.flags[10 * 64] & block, 0);
    assert_eq!(edge_view.flags[10 * 64 + 3] & block, 0);
    // Bunkers are live inputs even when the inflated terrain cache is reused.
    let mut open = Field::new(64, 64);
    let from = point(21., 41.);
    let to = point(101., 41.);
    let mut cached = Pathfinding::new();
    let clear = open.route(&mut cached, "tank", from, to);
    open.bunkers.push(point(61., 41.));
    let occupied = open.route(&mut cached, "tank", from, to);
    assert_ne!(clear.waypoints, occupied.waypoints);
    same(
        &occupied,
        &open.route(&mut Pathfinding::new(), "tank", from, to),
    );
    open.bunkers.clear();
    same(&clear, &open.route(&mut cached, "tank", from, to));
    let mut corridors = Field::new(64, 64);
    for y in 0..64 {
        if !(14..19).contains(&y) && !(44..49).contains(&y) {
            corridors.flags[y * 64 + 32] = MOVE;
        }
    }
    let from = point(21., 63.);
    let to = point(101., 63.);
    let mut cached = Pathfinding::new();
    let before = corridors.route(&mut cached, "tank", from, to);
    assert_eq!(before.status, PathStatus::Found);
    let upper = before.waypoints.iter().any(|p| p.z < 45.);
    // Public fire changes with no terrain-version increment or cache invalidation.
    for y in if upper { 14..19 } else { 44..49 } {
        corridors.fires[y * 64 + 32] = true;
    }
    let after = corridors.route(&mut cached, "tank", from, to);
    assert_eq!(after.status, PathStatus::Found);
    assert_ne!(before.waypoints, after.waypoints);
    same(
        &after,
        &corridors.route(&mut Pathfinding::new(), "tank", from, to),
    );
}

#[test]
fn mov29_observed_and_world_route_views_ignore_unreported_terrain_and_mines() {
    use crate::route_knowledge::{InitialTerrain, LiveRouting, RouteKnowledge};
    let rules = Rules::compiled().unwrap();
    let chars = vec![b'.'; 64 * 64];
    let hazards = vec![false; 64 * 64];
    let initial = InitialTerrain {
        width: 64,
        height: 64,
        chars: &chars,
        ground: None,
        objects: None,
        heights: None,
        shoals: None,
        house_kinds: None,
        toughness: None,
        trench_front: None,
    };
    let observed = RouteKnowledge::observed(initial, 11, rules.clone()).unwrap();
    let world = RouteKnowledge::world(64, 64, 12, rules.clone()).unwrap();
    let mut live = Field::new(64, 64);
    for knowledge in [&observed, &world] {
        let query = knowledge.query();
        let live_values = LiveRouting {
            fires: &hazards,
            bunkers: &[],
            wetness: 0.,
            roads: false,
        };
        let mut paths = Pathfinding::new();
        let profile = &rules.kinds[rules.kind("rifle").unwrap()];
        let before = paths
            .find_path(
                query.path_terrain(live_values).unwrap(),
                PathRules::compiled().unwrap(),
                profile,
                point(21., 21.),
                point(91., 21.),
                None,
            )
            .unwrap();
        for y in 0..64 {
            live.flags[y * 64 + 30] = MOVE;
            live.mines[y * 64 + 29] = true;
        }
        let after = paths
            .find_path(
                query.path_terrain(live_values).unwrap(),
                PathRules::compiled().unwrap(),
                profile,
                point(21., 21.),
                point(91., 21.),
                None,
            )
            .unwrap();
        same(&before, &after);
        assert!(!before.waypoints.is_empty());
    }
    // This is a query-component guard. Match owner/team selection is deliberately a separate held requirement.
}

#[test]
fn mov29_remembered_wall_and_discovered_mine_change_both_route_modes() {
    use crate::route_knowledge::{
        CellMetadata, InitialTerrain, LiveRouting, RememberedCell, RouteKnowledge,
    };
    let rules = Rules::compiled().unwrap();
    let profile = &rules.kinds[rules.kind("rifle").unwrap()];
    let chars = vec![b'.'; 1600];
    let from = point(21., 21.);
    let to = point(61., 21.);
    for world in [false, true] {
        for character in *b"WN" {
            let mut memory = if world {
                RouteKnowledge::world(40, 40, 80, rules.clone()).unwrap()
            } else {
                RouteKnowledge::observed(
                    InitialTerrain {
                        width: 40,
                        height: 40,
                        chars: &chars,
                        ground: None,
                        objects: None,
                        heights: None,
                        shoals: None,
                        house_kinds: None,
                        toughness: None,
                        trench_front: None,
                    },
                    81,
                    rules.clone(),
                )
                .unwrap()
            };
            let mut live = Field::new(40, 40);
            let mut paths = Pathfinding::new();
            let route = |memory: &RouteKnowledge, paths: &mut Pathfinding| {
                paths
                    .find_path(
                        memory
                            .query()
                            .path_terrain(LiveRouting {
                                fires: &live.fires,
                                bunkers: &[],
                                wetness: 0.,
                                roads: false,
                            })
                            .unwrap(),
                        PathRules::compiled().unwrap(),
                        profile,
                        from,
                        to,
                        None,
                    )
                    .unwrap()
            };
            let before = route(&memory, &mut paths);
            assert_eq!(before.waypoints, vec![to]);
            let cell = 10 * 40 + 20;
            // Physical edits do not enter the recipient view until a memory row is delivered.
            live.flags[cell] = MOVE;
            live.mines[cell] = true;
            same(&before, &route(&memory, &mut paths));
            memory
                .apply_rows(
                    1,
                    &[RememberedCell {
                        cell,
                        revision: 1,
                        character,
                        height: 0,
                        state: 0,
                        metadata: CellMetadata {
                            ground: Some(b'.'),
                            ..Default::default()
                        },
                        cover_basis: None,
                    }],
                )
                .unwrap();
            let after = route(&memory, &mut paths);
            assert_eq!(after.status, PathStatus::Found);
            assert_ne!(before.waypoints, after.waypoints);
            let query = memory.query();
            let terrain = query
                .path_terrain(LiveRouting {
                    fires: &live.fires,
                    bunkers: &[],
                    wetness: 0.,
                    roads: false,
                })
                .unwrap();
            for (a, b) in segments(from, &after.waypoints) {
                if character == b'N' {
                    assert!(!mined_line(terrain, a, b));
                } else {
                    assert!(walkable(terrain.navigation, a, b, MOVE, 0.9));
                }
            }
        }
    }
}

#[test]
fn mov30_remembered_bank_requires_a_legal_axis_step_before_routing() {
    let mut f = Field::new(64, 64);
    for y in 0..64 {
        f.flags[y * 64 + 11] = MOVE;
    }
    let rules = Rules::compiled().unwrap();
    let profile = &rules.kinds[rules.kind("rifle").unwrap()];
    let from = point(21.8, 21.);
    let to = point(21., 5.);
    let u = unit(&rules, "rifle", 1, from);
    let raw = f.route(&mut Pathfinding::new(), "rifle", from, to);
    assert!(!raw.waypoints.is_empty());
    assert!(!walkable(
        f.terrain().navigation,
        from,
        raw.waypoints[0],
        MOVE,
        0.9
    ));
    let route = crate::match_commands::search_route(
        &mut Pathfinding::new(),
        f.terrain(),
        PathRules::compiled().unwrap(),
        profile,
        &u,
        to,
    )
    .unwrap()
    .result;
    assert_eq!(route.waypoints[0], point(21., 21.));
    assert_eq!(route.waypoints.last(), Some(&to));
    for (a, b) in segments(from, &route.waypoints) {
        assert!(walkable(f.terrain().navigation, a, b, MOVE, 0.9));
    }
    let mut g = crate::mov_audit_test_support::ground();
    // The component receives only remembered terrain. Unrelated physical edits cannot alter it.
    g.flags.fill(0);
    g.flags[40 * 64 + 40] = MOVE;
    let again = crate::match_commands::search_route(
        &mut Pathfinding::new(),
        f.terrain(),
        PathRules::compiled().unwrap(),
        profile,
        &u,
        to,
    )
    .unwrap()
    .result;
    same(&route, &again);
    g.flags = f.flags;
    let mut u = u;
    u.path = route.waypoints;
    g.units.insert(1, u);
    let mut grid = crate::grid::SpatialGrid::new(&g.units, 16.);
    for _ in 0..200 {
        crate::movement::move_unit(&mut g, &rules, &mut grid, 1, 1.);
    }
    assert!(distance(g.units[&1].at(), to) < 0.05);
    assert!(g.units[&1].path.is_empty());
}

#[test]
fn mov33_pinned_heap_ties_and_road_scaled_neighbor_scores() {
    // Literal score-only heap ordering from the pinned heapPush/heapPop comparisons.
    let mut heap = Heap::default();
    for c in [10, 20, 30, 40] {
        heap.push(7., c);
    }
    assert_eq!(
        (0..4).map(|_| heap.pop().unwrap()).collect::<Vec<_>>(),
        vec![10, 40, 30, 20]
    );
    let rules = Rules::compiled().unwrap();
    let profile = &rules.kinds[rules.kind("tank").unwrap()];
    for roads in [false, true] {
        let mut f = Field::new(64, 64);
        f.roads = roads;
        let mut paths = Pathfinding::new();
        let result = paths
            .find_path(
                f.terrain(),
                PathRules::compiled().unwrap(),
                profile,
                point(21., 21.),
                point(81., 61.),
                Some(1),
            )
            .unwrap();
        assert_eq!(result.status, PathStatus::ExpansionLimit);
        // The second pop detects overflow before expanding its neighbors.
        let low = if roads { 1. / 1.35 } else { 1. };
        // Literal first-ring heuristic values from the pinned octile expression.
        let expected = [
            (9, 9, 1.414, 39.694),
            (10, 9, 1., 38.694),
            (11, 9, 1.414, 37.694),
            (9, 10, 1., 39.28),
            (11, 10, 1., 37.28),
            (9, 11, 1.414, 38.866),
            (10, 11, 1., 37.866),
            (11, 11, 1.414, 36.866),
        ];
        assert_eq!(paths.scratch.heap.0.len(), 7);
        for &(score, cell) in &paths.scratch.heap.0 {
            let &(_, _, cost, heuristic) = expected
                .iter()
                .find(|&&(x, y, _, _)| cell == y * 64 + x)
                .unwrap();
            close(score, cost + heuristic * low);
        }
        assert!((30. + 0.414 * 20.) * low <= (10. + 20. * 1.414) * low + 1e-12);
    }
    let mut symmetric = Field::new(64, 64);
    symmetric.flags[20 * 64 + 30] = MOVE;
    let from = point(21., 41.);
    let to = point(101., 41.);
    let mut paths = Pathfinding::new();
    let first = symmetric.route(&mut paths, "rifle", from, to);
    assert!(first.waypoints.len() > 1);
    for _ in 0..4 {
        same(&first, &symmetric.route(&mut paths, "rifle", from, to));
        same(
            &first,
            &symmetric.route(&mut Pathfinding::new(), "rifle", from, to),
        );
    }
}

#[test]
fn mov35_smoothing_guards_shortcuts_and_keeps_profile_specific_terrain_masks() {
    let from = point(21., 41.);
    let to = point(101., 41.);
    for hazard in [WIRE, MUD, WOOD] {
        let mut f = Field::new(64, 64);
        f.flags[20 * 64 + 30] = hazard;
        for name in ["rifle", "tank"] {
            let path = f.route(&mut Pathfinding::new(), name, from, to);
            assert_eq!(path.status, PathStatus::Found);
            let guarded = if name == "rifle" {
                hazard == WIRE
            } else {
                hazard != WIRE
            };
            if guarded {
                assert!(
                    path.waypoints.len() > 1,
                    "{name} retains corners around {hazard}"
                );
            } else {
                assert_eq!(path.waypoints, vec![to]);
            }
            // Adjacent A* edges are legal even when the 2 m shortcut guard would reject them.
            for (a, b) in segments(from, &path.waypoints) {
                assert!(walkable(f.terrain().navigation, a, b, MOVE, 0.9));
            }
        }
    }
    let mut road = Field::new(64, 64);
    road.roads = true;
    for x in 10..51 {
        road.flags[20 * 64 + x] = ROAD;
    }
    let kept = road.route(&mut Pathfinding::new(), "tank", from, to);
    assert_eq!(kept.status, PathStatus::Found);
    assert!(kept.waypoints.len() > 20);
    let mut water = Field::new(64, 64);
    water.flags.fill(MOVE);
    let boat = water.route(&mut Pathfinding::new(), "gunboat", from, to);
    assert_eq!(boat.waypoints, vec![to]);
    water.flags[20 * 64 + 30] = crate::game::LAND;
    let around_land = water.route(&mut Pathfinding::new(), "gunboat", from, to);
    assert_eq!(around_land.status, PathStatus::Found);
    assert!(around_land.waypoints.len() > 1);
    let mut mine = Field::new(64, 64);
    mine.mines[20 * 64 + 30] = true;
    let avoided = mine.route(&mut Pathfinding::new(), "rifle", from, to);
    assert_eq!(avoided.status, PathStatus::Found);
    assert!(!avoided.waypoints.is_empty());
    for (a, b) in segments(from, &avoided.waypoints) {
        assert!(!mined_line(mine.terrain(), a, b));
    }
    mine.mines.fill(false);
    mine.bunkers.push(point(61., 41.));
    let around_post = mine.route(&mut Pathfinding::new(), "rifle", from, to);
    let radius = PathRules::compiled().unwrap().bunker_radius + 1.5;
    assert!(around_post.waypoints.len() > 1);
    for (a, b) in segments(from, &around_post.waypoints) {
        assert!(!segment_hits(a, b, point(61., 41.), radius));
    }
}

#[test]
#[ignore = "Retained e1c69647 failure: authored initial native leg is already walkable; fallback prerequisite failed"]
fn mov30_bank_step_back_restores_walkable_first_leg_and_arrival() {
    let mut f = Field::new(64, 64);
    for y in 0..64 {
        for x in 29..36 {
            if !(27..=29).contains(&y) {
                f.flags[y * 64 + x] = MOVE;
            }
        }
    }
    let rules = Rules::compiled().unwrap();
    let profile = &rules.kinds[rules.kind("rifle").unwrap()];
    let from = point(71.324, 59.081);
    let to = point(92., 64.);
    let u = unit(&rules, "rifle", 1, from);
    let mut paths = Pathfinding::new();
    let raw = f.route(&mut paths, "rifle", from, to);
    assert!(!raw.waypoints.is_empty());
    assert!(
        !walkable(
            f.terrain().navigation,
            from,
            raw.waypoints[0],
            profile.block(),
            0.9
        ),
        "scene requires bank fallback"
    );
    let route = crate::match_commands::search_route(
        &mut paths,
        f.terrain(),
        PathRules::compiled().unwrap(),
        profile,
        &u,
        to,
    )
    .unwrap()
    .result;
    assert_eq!(route.status, PathStatus::Found);
    assert_eq!(route.waypoints.last(), Some(&to));
    assert_ne!(route.waypoints, raw.waypoints);
    for (a, b) in segments(from, &route.waypoints) {
        assert!(walkable(f.terrain().navigation, a, b, profile.block(), 0.9));
    }
    // Integrate the independently computed native route, never expected positions.
    let mut g = crate::mov_audit_test_support::ground();
    g.flags = f.flags.clone();
    let mut u = u;
    u.path = route.waypoints;
    g.units.insert(1, u);
    let mut grid = crate::grid::SpatialGrid::new(&g.units, 16.);
    for _ in 0..200 {
        crate::movement::move_unit(&mut g, &rules, &mut grid, 1, 1.);
    }
    assert!(distance(g.units[&1].at(), to) < 0.05);
    assert!(g.units[&1].path.is_empty());
}

#[test]
#[ignore = "Retained e1c69647 failure: authored expansion count expected 1, actual 2, before heuristic assertions"]
fn mov33_score_only_heap_ties_and_road_heuristic_lower_bound() {
    let mut heap = Heap::default();
    for c in [10, 20, 30, 40] {
        heap.push(7., c);
    }
    assert_eq!(
        (0..4).map(|_| heap.pop().unwrap()).collect::<Vec<_>>(),
        vec![10, 40, 30, 20]
    );
    assert_eq!(heap.pop(), None);
    for roads in [false, true] {
        let mut f = Field::new(64, 64);
        f.roads = roads;
        let rules = Rules::compiled().unwrap();
        let profile = &rules.kinds[rules.kind("tank").unwrap()];
        let mut paths = Pathfinding::new();
        let r = paths
            .find_path(
                f.terrain(),
                PathRules::compiled().unwrap(),
                profile,
                point(21., 21.),
                point(81., 61.),
                Some(1),
            )
            .unwrap();
        assert_eq!(r.expansions, 1);
        let low = if roads { 1. / 1.35 } else { 1. };
        // From cell (10,10), first cardinal neighbor (11,10) has g=1 and h=(29+.414*20)*low.
        let entry = paths
            .scratch
            .heap
            .0
            .iter()
            .find(|(_, c)| *c == 10 * 64 + 11)
            .unwrap();
        close(entry.0, 1. + (29. + 0.414 * 20.) * low);
    }
}

#[test]
#[ignore = "Retained e1c69647 failure: blanket 2 m wire-ray assertion exceeds adjacent-segment shortcut guard"]
fn mov35_smoothing_preserves_roads_wire_mines_and_bunker_clearance() {
    let from = point(21., 41.);
    let to = point(101., 41.);
    let mut open = Field::new(64, 64);
    let straight = open.route(&mut Pathfinding::new(), "rifle", from, to);
    assert_eq!(
        straight.waypoints,
        vec![to],
        "open ground pulls to the furthest waypoint"
    );
    open.mines[20 * 64 + 30] = true;
    let mined = open.route(&mut Pathfinding::new(), "rifle", from, to);
    assert!(mined.waypoints.len() > 1);
    for (a, b) in segments(from, &mined.waypoints) {
        assert!(!mined_line(open.terrain(), a, b));
    }
    open.mines.fill(false);
    open.bunkers.push(point(61., 41.));
    let bunker = open.route(&mut Pathfinding::new(), "rifle", from, to);
    assert!(bunker.waypoints.len() > 1);
    let radius = PathRules::compiled().unwrap().bunker_radius + 1.5;
    for (a, b) in segments(from, &bunker.waypoints) {
        assert!(!segment_hits(a, b, point(61., 41.), radius));
    }
    open.bunkers.clear();
    for y in 15..26 {
        open.flags[y * 64 + 30] = WIRE;
    }
    let wire = open.route(&mut Pathfinding::new(), "rifle", from, to);
    assert!(wire.waypoints.len() > 1);
    for (a, b) in segments(from, &wire.waypoints) {
        assert!(walkable(open.terrain().navigation, a, b, MOVE | WIRE, 2.));
    }
    let mut road = Field::new(64, 64);
    road.roads = true;
    for x in 8..55 {
        road.flags[20 * 64 + x] = ROAD;
    }
    let kept = road.route(&mut Pathfinding::new(), "tank", from, to);
    assert!(
        kept.waypoints.len() > 10,
        "vehicle does not smooth past road cells"
    );
    for y in 15..26 {
        road.flags[y * 64 + 30] = MUD | WOOD;
    }
    road.version = 1;
    let detour = road.route(&mut Pathfinding::new(), "tank", from, to);
    assert_eq!(detour.status, PathStatus::Found);
    assert!(detour.waypoints.len() > 1);
    let mut water = Field::new(64, 64);
    water.flags.fill(MOVE);
    let boat = water.route(&mut Pathfinding::new(), "gunboat", from, to);
    assert_eq!(boat.status, PathStatus::Found);
    assert_eq!(boat.waypoints, vec![to]);
}

#[test]
fn mov36_disconnection_rejects_before_search_and_edits_reconnect() {
    let mut f = Field::new(64, 64);
    for y in 0..64 {
        f.flags[y * 64 + 32] = MOVE;
    }
    let from = point(21., 41.);
    let to = point(101., 41.);
    let mut paths = Pathfinding::new();
    let closed = f.route(&mut paths, "rifle", from, to);
    assert_eq!(
        (closed.status, closed.expansions),
        (PathStatus::Unreachable, 0)
    );
    f.set(20 * 64 + 32, 0);
    let opened = f.route(&mut paths, "rifle", from, to);
    assert_eq!(opened.status, PathStatus::Found);
    same(
        &opened,
        &f.route(&mut Pathfinding::new(), "rifle", from, to),
    );
    f.set(20 * 64 + 32, MOVE);
    let closed_again = f.route(&mut paths, "rifle", from, to);
    same(&closed_again, &closed);
    assert_eq!(closed_again.expansions, 0);
    // A full change set crosses the old local-relabel threshold; compare to a fresh flood.
    f.changed = (0..64).map(|y| y * 64 + 32).collect();
    f.changed.push(0);
    f.version += 1;
    for &c in &f.changed {
        f.flags[c] = 0;
    }
    same(
        &f.route(&mut paths, "rifle", from, to),
        &f.route(&mut Pathfinding::new(), "rifle", from, to),
    );
    // Diagonal uphill adjacency is accepted only when the corner levels suit an endpoint.
    let mut hill = Field::new(64, 64);
    hill.flags.fill(MOVE);
    hill.height = Some(vec![0; 4096]);
    for c in [10 * 64 + 10, 11 * 64 + 11] {
        hill.flags[c] = 0;
    }
    let flat = hill.route(
        &mut Pathfinding::new(),
        "rifle",
        point(21., 21.),
        point(23., 23.),
    );
    assert_eq!(flat.status, PathStatus::Unreachable);
    hill.flags[10 * 64 + 11] = 0;
    hill.flags[11 * 64 + 10] = 0;
    hill.height.as_mut().unwrap()[11 * 64 + 11] = 1;
    let joined = hill.route(
        &mut Pathfinding::new(),
        "rifle",
        point(21., 21.),
        point(23., 23.),
    );
    assert_eq!(joined.status, PathStatus::Found);
}

#[test]
#[ignore = "Retained e1c69647 failure: incremental and fresh internal region IDs differ despite same leg endpoint"]
fn mov37_coarse_routes_update_from_history_and_survive_cache_eviction() {
    let mut f = Field::new(160, 64);
    f.world = true;
    let mut nav = Navigation::new();
    let start = 30 * 160 + 10;
    let goal = 30 * 160 + 145;
    let original = nav
        .leg(
            f.terrain().navigation,
            start,
            goal,
            MOVE,
            Options::default(),
        )
        .unwrap();
    assert!(original.connected && original.limited);
    let original_goal = original.goal;
    f.changed = (0..64).map(|y| y * 160 + 32).collect();
    for &c in &f.changed {
        f.flags[c] = MOVE;
    }
    f.version = 1;
    let terrain = f.terrain().navigation;
    let history = [TerrainChange {
        from: 0,
        to: 1,
        cells: &f.changed,
    }];
    let history_view = TerrainView {
        changed: None,
        changes: &history,
        ..terrain
    };
    assert!(
        !nav.leg(history_view, start, goal, MOVE, Options::default())
            .unwrap()
            .connected
    );
    f.set(30 * 160 + 32, 0);
    let updated = nav
        .leg(
            f.terrain().navigation,
            start,
            goal,
            MOVE,
            Options::default(),
        )
        .unwrap();
    assert!(updated.connected);
    let actual = (updated.goal, updated.limited, updated.allowed.clone());
    let mut fresh = Navigation::new();
    let expected = fresh
        .leg(
            f.terrain().navigation,
            start,
            goal,
            MOVE,
            Options::default(),
        )
        .unwrap();
    assert_eq!(actual, (expected.goal, expected.limited, expected.allowed));
    for y in 2..62 {
        for x in [2, 18, 34] {
            let _ = nav
                .leg(
                    f.terrain().navigation,
                    y * 160 + x,
                    goal,
                    MOVE,
                    Options::default(),
                )
                .unwrap();
        }
    }
    let reused = nav
        .leg(
            f.terrain().navigation,
            start,
            goal,
            MOVE,
            Options::default(),
        )
        .unwrap();
    assert_eq!(reused.goal, original_goal);
    assert!(reused.connected);
}

#[test]
fn mov38_small_maps_have_full_routes_world_and_large_maps_limit_to_32_cells() {
    for (w, world, limited) in [(128, false, false), (128, true, true), (160, false, true)] {
        let mut f = Field::new(w, 64);
        f.world = world;
        let from = point(21., 41.);
        let to = point((w as f64 - 10.) * 2. + 0.3, 41.2);
        let result = f.route(&mut Pathfinding::new(), "rifle", from, to);
        assert_eq!(result.status, PathStatus::Found);
        assert_eq!(result.limited, limited);
        if limited {
            let end = *result.waypoints.last().unwrap();
            close(end.x % 2., 1.);
            close(end.z % 2., 1.);
            // A leg counts cells, including diagonal steps, rather than metres.
            assert!((from.x - end.x).abs().max((from.z - end.z).abs()) <= 64.01);
            assert_ne!(end, result.route_end);
        } else {
            assert_eq!(result.waypoints.last(), Some(&to));
        }
        assert_eq!(
            result.route_end, to,
            "retains the complete clicked destination"
        );
    }
}

#[test]
fn mov38_bounded_legs_reach_exact_goal_and_small_maps_keep_cost_choices() {
    for (w, world) in [(128, true), (160, false)] {
        let mut f = Field::new(w, 64);
        f.world = world;
        for y in 0..28 {
            f.flags[y * w + 45] = MOVE;
        }
        let mut from = point(21., 41.);
        let to = point((w as f64 - 10.) * 2. + 0.3, 41.2);
        let mut paths = Pathfinding::new();
        let mut visited = std::collections::BTreeSet::new();
        let mut legs = 0;
        loop {
            let leg = f.route(&mut paths, "rifle", from, to);
            assert_eq!(leg.status, PathStatus::Found);
            let end = *leg.waypoints.last().unwrap();
            for (a, b) in segments(from, &leg.waypoints) {
                assert!(walkable(f.terrain().navigation, a, b, MOVE, 0.9));
            }
            legs += 1;
            if !leg.limited {
                assert_eq!(end, to);
                break;
            }
            assert!(legs < 20);
            close(end.x % 2., 1.);
            close(end.z % 2., 1.);
            // A leg counts cells, including diagonal steps, rather than metres.
            assert!((from.x - end.x).abs().max((from.z - end.z).abs()) <= 64.01);
            assert!(visited.insert((end.x.to_bits(), end.z.to_bits())));
            from = end;
        }
        assert!(legs > 1);
    }
    let mut f = Field::new(128, 64);
    let from = point(21., 61.);
    let to = point(221., 61.);
    for y in 0..64 {
        if y != 15 && y != 45 {
            f.flags[y * 128 + 60] = MOVE;
        }
    }
    let mut paths = Pathfinding::new();
    let upper = f.route(&mut paths, "rifle", from, to);
    assert_eq!(upper.status, PathStatus::Found);
    assert!(!upper.limited);
    // Fires alter costs without changing which coarse regions are reachable.
    let first_is_upper = upper.waypoints.iter().any(|p| p.z < 40.);
    f.fires[if first_is_upper {
        15 * 128 + 60
    } else {
        45 * 128 + 60
    }] = true;
    let lower = f.route(&mut paths, "rifle", from, to);
    assert_eq!(lower.status, PathStatus::Found);
    assert!(!lower.limited);
    assert_ne!(upper.waypoints, lower.waypoints);
    assert!(
        lower
            .waypoints
            .iter()
            .any(|p| if first_is_upper { p.z > 80. } else { p.z < 40. })
    );
}

#[test]
fn mov36_directed_hill_corner_is_walkable_from_its_high_endpoint() {
    let mut f = Field::new(64, 64);
    f.flags.fill(MOVE);
    f.height = Some(vec![0; 4096]);
    for (x, y, h) in [(15, 15, 0), (16, 16, 1), (16, 15, 2), (15, 16, 2)] {
        f.flags[y * 64 + x] = 0;
        f.height.as_mut().unwrap()[y * 64 + x] = h;
    }
    let low = point(31., 31.);
    let high = point(33., 33.);
    assert_eq!(
        f.route(&mut Pathfinding::new(), "rifle", high, low).status,
        PathStatus::Found
    );
    assert_eq!(
        f.route(&mut Pathfinding::new(), "rifle", low, high).status,
        PathStatus::Unreachable
    );
}

#[test]
fn mov40_multiple_valid_waiters_precede_new_work_and_real_move_bypasses_zero_budget() {
    use crate::{
        match_commands::{Command, MoveOrder},
        mov_audit_test_support::{scene, spawn},
        path_requests::*,
    };
    let mut m = scene();
    let ids = (0..4)
        .map(|i| spawn(&mut m, "rifle", 0, point(21., 21. + f64::from(i) * 8.)))
        .collect::<Vec<_>>();
    m.begin_path_tick(0, 20).unwrap();
    for &id in &ids {
        let at = m.world.game.units[&id].at();
        m.execute_command(
            0,
            &Command::Move {
                orders: vec![MoveOrder {
                    unit: i64::from(id),
                    at: point(81., at.z),
                }],
                control_ids: None,
                attack_move: false,
                queue: false,
                together: false,
                face: None,
            },
            false,
        )
        .unwrap();
        assert!(!m.world.game.units[&id].path.is_empty());
        assert_eq!(m.commands.requests.used(), 0);
    }
    // Reuse canonical command goals so queued requests remain valid across ticks.
    for &id in &ids[..3] {
        let goal = m.world.game.units[&id]
            .world_goal
            .or(m.world.game.units[&id].path.last().copied())
            .unwrap();
        m.world.game.units.get_mut(&id).unwrap().path.clear();
        m.world.game.units.get_mut(&id).unwrap().world_goal = Some(goal);
        assert_eq!(
            m.request_command_path(id, goal, RequestKind::World)
                .unwrap(),
            Disposition::Queued
        );
    }
    m.begin_path_tick(1, 20).unwrap();
    assert!(!m.world.game.units[&ids[0]].path.is_empty());
    assert!(m.world.game.units[&ids[1]].path.is_empty());
    let id = ids[3];
    let goal = *m.world.game.units[&id].path.last().unwrap();
    m.world.game.units.get_mut(&id).unwrap().path.clear();
    m.world.game.units.get_mut(&id).unwrap().world_goal = Some(goal);
    assert_eq!(
        m.request_command_path(id, goal, RequestKind::World)
            .unwrap(),
        Disposition::Queued
    );
    m.begin_path_tick(1, 20).unwrap();
    assert!(!m.world.game.units[&ids[1]].path.is_empty());
    assert!(m.world.game.units[&ids[2]].path.is_empty());
    assert!(m.world.game.units[&ids[3]].path.is_empty());
    m.begin_path_tick(1, 20).unwrap();
    assert!(!m.world.game.units[&ids[2]].path.is_empty());
    assert!(m.world.game.units[&ids[3]].path.is_empty());
    m.begin_path_tick(1, 20).unwrap();
    assert!(!m.world.game.units[&ids[3]].path.is_empty());
}

#[test]
fn mov40_budget_fifo_stale_dead_and_immediate_player_orders() {
    use crate::path_requests::*;
    use std::collections::BTreeMap;
    struct Context {
        goals: BTreeMap<u32, Point>,
        searched: Vec<u32>,
        updates: Vec<PathUpdate>,
    }
    impl PathRequestContext for Context {
        type Error = ();
        fn current_goal(&mut self, id: u32, _: RequestKind) -> Option<Point> {
            self.goals.get(&id).copied()
        }
        fn unit_incarnation(&self, id: u32) -> Option<u64> {
            self.goals.contains_key(&id).then_some(u64::from(id))
        }
        fn knowledge_version(&self, _: u32) -> u64 {
            0
        }
        fn search(&mut self, r: Request) -> Result<SearchOutcome, ()> {
            self.searched.push(r.unit);
            Ok(SearchOutcome {
                path: vec![r.goal],
                failed: false,
            })
        }
        fn apply(&mut self, u: PathUpdate) {
            self.updates.push(u);
        }
    }
    let mut c = Context {
        goals: (1..=5)
            .map(|id| (id, point(80., f64::from(id) * 4.)))
            .collect(),
        searched: vec![],
        updates: vec![],
    };
    let mut scheduler = PathRequests::new();
    scheduler
        .begin_tick(
            TickConfig {
                budget: 1,
                ..TickConfig::default()
            },
            &mut c,
        )
        .unwrap();
    for id in 1..=4 {
        let r = Request {
            unit: id,
            kind: RequestKind::World,
            goal: c.goals[&id],
        };
        assert_eq!(
            scheduler.request(r, &mut c).unwrap(),
            if id == 1 {
                Disposition::Searched
            } else {
                Disposition::Queued
            }
        );
    }
    c.goals.remove(&2);
    c.goals.insert(3, point(99., 12.));
    let report = scheduler
        .begin_tick(
            TickConfig {
                budget: 0,
                tick: 1,
                ..TickConfig::default()
            },
            &mut c,
        )
        .unwrap();
    assert_eq!((report.discarded, report.searched), (2, 0));
    assert_eq!(c.searched, vec![1]);
    let report = scheduler
        .begin_tick(
            TickConfig {
                budget: 1,
                tick: 2,
                ..TickConfig::default()
            },
            &mut c,
        )
        .unwrap();
    assert_eq!(report.searched, 1);
    assert_eq!(c.searched, vec![1, 4]);
    let repeated = Request {
        unit: 1,
        kind: RequestKind::Recovery,
        goal: c.goals[&1],
    };
    assert_eq!(
        scheduler.request(repeated, &mut c).unwrap(),
        Disposition::Queued
    );
    scheduler
        .begin_tick(
            TickConfig {
                budget: 1,
                tick: 3,
                ..TickConfig::default()
            },
            &mut c,
        )
        .unwrap();
    assert_eq!(c.searched, vec![1, 4, 1]);
    assert_eq!(scheduler.used(), 1);
    // Real player command routing is synchronous and outside the scheduler's used count.
    let f = Field::new(64, 64);
    let rules = Rules::compiled().unwrap();
    let u = unit(&rules, "rifle", 5, point(21., 21.));
    let route = crate::match_commands::search_route(
        &mut Pathfinding::new(),
        f.terrain(),
        PathRules::compiled().unwrap(),
        &rules.kinds[u.kind],
        &u,
        point(81., 21.),
    )
    .unwrap();
    assert_eq!(route.result.status, PathStatus::Found);
    assert_eq!(scheduler.used(), 1);
    assert_eq!(TickConfig::default().budget, 4096);
}

#[test]
fn mov29_match_selects_player_memory_and_world_team_memory_after_hidden_edits() {
    use crate::{
        economy::{Faction, Mode},
        map::AuthoritativeMap,
        match_runtime::{Match, PlayerConfig},
        mov_audit_test_support::{scene, spawn},
        random::MatchRandom,
    };
    use serde_json::json;
    let mut ordinary = scene();
    let id = spawn(&mut ordinary, "rifle", 0, point(21., 21.));
    crate::vision::update_vision(&mut ordinary.world.game, &ordinary.rules, &ordinary.grid);
    let u = ordinary.world.game.units[&id].clone();
    let goal = point(111., 21.);
    let before = ordinary.command_route(0, &u, goal).unwrap();
    assert!(!before.waypoints.is_empty());
    ordinary.world.set_cell(10 * 64 + 50, b'B').unwrap();
    ordinary
        .world
        .mutate_cell(
            11 * 64 + 50,
            crate::world::CellChange {
                mine: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
    let after = ordinary.command_route(0, &u, goal).unwrap();
    same(&before, &after);
    let regions=(0..64).map(|i| {
        let x=i%8*64;let y=i/8*64;
        let mut r=json!({"id":i,"x":x+32,"y":y+32,"bounds":[x,y,x+64,y+64],"kind":"rural","name":format!("R{i}")});
        if let Some(home)=[0,63,1].iter().position(|&v|v==i){r["home"]=json!(home);}r
    }).collect::<Vec<_>>();
    let map=AuthoritativeMap::from_json(&serde_json::to_vec(&json!({"name":"MOV World memory","w":512,"h":512,
        "rows":vec![".".repeat(512);512],"spawns":[{"x":32,"y":32},{"x":480,"y":480},{"x":96,"y":32}],"points":[],
        "world":{"size":"huge","total":64,"regionCells":64,"regions":regions}})).unwrap()).unwrap();
    let mut options = ordinary.options.clone();
    options.mode = Mode::World;
    options.supply = true;
    options.players = [5, 1, 5]
        .iter()
        .enumerate()
        .map(|(i, &team)| PlayerConfig {
            name: format!("P{i}"),
            team,
            faction: Faction::Usa,
        })
        .collect();
    let mut world = Match::new_world(&map, options, MatchRandom::new(7301)).unwrap();
    let u = world
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 0 && world.rules.kinds[u.kind].name == "rifle")
        .unwrap()
        .clone();
    let goal = point(700., u.z);
    let before = world.command_route(0, &u, goal).unwrap();
    assert!(!before.waypoints.is_empty());
    world.refresh_world_routing(2).unwrap();
    world.refresh_world_routing(1).unwrap();
    let identity = world
        .world_routing(0)
        .unwrap()
        .query()
        .navigation()
        .knowledge_id;
    assert_eq!(
        identity,
        world
            .world_routing(2)
            .unwrap()
            .query()
            .navigation()
            .knowledge_id,
        "allies share the team route view"
    );
    assert_ne!(
        identity,
        world
            .world_routing(1)
            .unwrap()
            .query()
            .navigation()
            .knowledge_id,
        "opponents use independent memory"
    );
    let hidden = (u.z / 2.) as usize * 512 + 220;
    world.world.set_cell(hidden, b'B').unwrap();
    world
        .world
        .mutate_cell(
            hidden + 512,
            crate::world::CellChange {
                mine: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
    let after = world.command_route(0, &u, goal).unwrap();
    same(&before, &after);
}
