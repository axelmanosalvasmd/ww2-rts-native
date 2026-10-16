//! Real convoy routing and physical handoff over explicit recipient and authoritative terrain.
use crate::{
    game::{CELL, MOVE, Point, VBLOCK},
    math,
    navigation::{TerrainChange, TerrainView, Versions},
    pathfinding::{self, PathRules, PathTerrain, Pathfinding},
    rules::UnitKind,
    supply_convoys::{Route, RouteFailure},
};
use std::collections::BTreeMap;

#[cfg(test)]
mod intent_tests;
#[derive(Clone, Copy, Debug)]
pub struct RouteRequest {
    pub owner: usize,
    pub tick: u64,
    pub from: Point,
    pub to: Point,
    pub safe: bool,
}
#[derive(Clone, Copy, Debug)]
pub struct Handoff {
    pub tick: u64,
    pub start: Point,
    pub store: Point,
    pub radius: f64,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Threat {
    pub at: Point,
    pub range: f64,
}
#[derive(Default)]
struct OwnerRoutes {
    key: Option<(u64, u64)>,
    threats: Vec<Threat>,
    flags: Vec<u16>,
    base: Vec<u16>,
    cleared: Vec<usize>,
    heights: Vec<i8>,
    known_id: Option<(u64, usize, usize)>,
    changed_from: u64,
    changed: Vec<usize>,
    changes: Vec<(u64, u64, Vec<usize>)>,
    version: u64,
    ordinary_paths: Pathfinding,
    danger_paths: Pathfinding,
}
/// Scratch belongs to one Match and its stable recipient/world identities.
/// Construct fresh scratch when replacing a Match or its equal-version terrain.
#[derive(Default)]
pub struct SupplyRouting {
    owners: BTreeMap<usize, OwnerRoutes>,
    handoff_paths: Pathfinding,
    handoff_key: Option<(u64, u64, u64)>,
    handoffs: BTreeMap<[i64; 5], bool>,
}
fn distance(a: Point, b: Point) -> f64 {
    math::hypot(a.x - b.x, a.z - b.z)
}
impl SupplyRouting {
    pub fn route(
        &mut self,
        request: RouteRequest,
        known: PathTerrain<'_>,
        profile: &UnitKind,
        threats: &[Threat],
    ) -> Result<Route, String> {
        let RouteRequest {
            owner,
            tick,
            from,
            to,
            safe,
        } = request;
        if owner >= 32 {
            return Err("invalid routing recipient".into());
        }
        validate(known, from, to)?;
        if profile.air || profile.structure || profile.rail {
            return Err("unsupported convoy profile".into());
        }
        if threats.iter().any(|t| {
            !t.at.x.is_finite() || !t.at.z.is_finite() || !t.range.is_finite() || t.range < 0.0
        }) {
            return Err("invalid threat".into());
        }
        if distance(from, to) <= 16.0
            && pathfinding::walkable(
                known.navigation,
                from,
                to,
                profile.block(),
                profile.body_radius(),
            )
        {
            return Ok(Route {
                route_end: None,
                path: vec![to],
                failure: None,
            });
        }
        let state = self.owners.entry(owner).or_default();
        let key = (tick / 40, known.navigation.versions.knowledge);
        let refreshed = safe && state.key != Some(key);
        if refreshed {
            state.key = Some(key);
            state.threats = threats.to_vec();
        }
        let danger = safe && !state.threats.is_empty();
        if danger
            && distance(from, to) > 4.0
            && state
                .threats
                .iter()
                .any(|t| distance(to, t.at) <= t.range + 3.0)
        {
            return Ok(Route {
                route_end: None,
                path: vec![],
                failure: Some(RouteFailure::Dangerous),
            });
        }
        if danger {
            refresh_danger(state, known.navigation, from, refreshed)?;
        }
        let history = state
            .changes
            .iter()
            .map(|(from, to, cells)| TerrainChange {
                from: *from,
                to: *to,
                cells,
            })
            .collect::<Vec<_>>();
        let terrain = if danger {
            let v = known.navigation;
            PathTerrain {
                navigation: TerrainView {
                    flags: &state.flags,
                    knowledge_id: 0x53555000 + owner as u64,
                    versions: Versions {
                        infantry: state.version,
                        vehicle: state.version,
                        naval: state.version,
                        obstruction: state.version,
                        knowledge: state.version,
                    },
                    changed: Some(TerrainChange {
                        from: state.changed_from,
                        to: state.version,
                        cells: &state.changed,
                    }),
                    changes: &history,
                    ..v
                },
                ..known
            }
        } else {
            known
        };
        let rules = PathRules::compiled().map_err(|e| e.to_string())?;
        let first = if danger {
            state
                .danger_paths
                .find_path(terrain, rules, profile, from, to, None)
                .map_err(|e| format!("convoy path: {e:?}"))?
        } else {
            observed_path(&mut state.ordinary_paths, known, rules, profile, from, to)?
        };
        let mut route_end_effect = first.route_end;
        let mut path = first.waypoints;
        if path.is_empty() {
            let failure = if danger {
                let ordinary =
                    observed_path(&mut state.ordinary_paths, known, rules, profile, from, to)?;
                route_end_effect = ordinary.route_end;
                if ordinary.waypoints.is_empty() {
                    RouteFailure::Unreachable
                } else {
                    RouteFailure::Dangerous
                }
            } else {
                RouteFailure::Unreachable
            };
            return Ok(Route {
                route_end: Some(route_end_effect),
                path,
                failure: Some(failure),
            });
        }
        if distance(*path.last().unwrap(), to) <= 3.0 {
            return Ok(Route {
                route_end: Some(route_end_effect),
                path,
                failure: None,
            });
        }
        let route_end = first.route_end;
        let mut at = *path.last().unwrap();
        for _ in 0..512 {
            let paths = if danger {
                &mut state.danger_paths
            } else {
                &mut state.ordinary_paths
            };
            let leg = paths
                .find_path(terrain, rules, profile, at, to, None)
                .map_err(|e| format!("convoy leg: {e:?}"))?;
            let Some(&end) = leg.waypoints.last() else {
                return Ok(Route {
                    route_end: Some(route_end_effect),
                    path: vec![],
                    failure: Some(RouteFailure::Unreachable),
                });
            };
            if distance(at, end) < 0.05 {
                return Ok(Route {
                    route_end: Some(route_end_effect),
                    path: vec![],
                    failure: Some(RouteFailure::Unreachable),
                });
            }
            path.extend(leg.waypoints);
            if distance(end, to) <= 3.0 || distance(end, route_end) <= 0.25 {
                return Ok(Route {
                    route_end: Some(route_end_effect),
                    path,
                    failure: None,
                });
            }
            at = end;
        }
        Ok(Route {
            route_end: Some(route_end_effect),
            path: vec![],
            failure: Some(RouteFailure::Unreachable),
        })
    }
    pub fn continue_route(
        known: TerrainView<'_>,
        profile: &UnitKind,
        from: Point,
        path: &[Point],
        stuck: f64,
        threats: &[Threat],
    ) -> bool {
        if path.is_empty()
            || stuck > 0.5
            || !stuck.is_finite()
            || validate_navigation(known).is_err()
            || !from.x.is_finite()
            || !from.z.is_finite()
            || path.iter().any(|p| !p.x.is_finite() || !p.z.is_finite())
            || threats.iter().any(|t| {
                !t.at.x.is_finite() || !t.at.z.is_finite() || !t.range.is_finite() || t.range < 0.0
            })
        {
            return false;
        }
        let mut from = from;
        for &to in path {
            if !pathfinding::walkable(known, from, to, profile.block(), profile.body_radius()) {
                return false;
            }
            let dx = to.x - from.x;
            let dz = to.z - from.z;
            let length = dx * dx + dz * dz;
            for t in threats {
                let along = if length != 0.0 {
                    math::max(
                        0.0,
                        math::min(
                            1.0,
                            ((t.at.x - from.x) * dx + (t.at.z - from.z) * dz) / length,
                        ),
                    )
                } else {
                    0.0
                };
                let x = from.x + along * dx - t.at.x;
                let z = from.z + along * dz - t.at.z;
                let radius = t.range + 3.0;
                if x * x + z * z <= radius * radius {
                    return false;
                }
            }
            from = to;
        }
        true
    }
    pub fn handoff(
        &mut self,
        request: Handoff,
        terrain: PathTerrain<'_>,
        rifle: &UnitKind,
        cells: &[u32],
    ) -> Result<bool, String> {
        let Handoff {
            tick,
            start,
            store,
            radius,
        } = request;
        validate(terrain, start, store)?;
        if !radius.is_finite() || radius < 0.0 {
            return Err("invalid handoff radius".into());
        }
        let length = distance(start, store);
        if length > radius {
            return Ok(false);
        }
        let mut edge = store;
        if !cells.is_empty() {
            let divisor = if length != 0.0 { length } else { 1.0 };
            let dx = (start.x - store.x) / divisor;
            let dz = (start.z - store.z) / divisor;
            let mut d = 0.25;
            while d <= length {
                edge = Point {
                    x: store.x + dx * d,
                    z: store.z + dz * d,
                };
                let x = (edge.x / CELL).floor();
                let y = (edge.z / CELL).floor();
                let inside = x >= 0.0
                    && y >= 0.0
                    && x < terrain.navigation.width as f64
                    && y < terrain.navigation.height as f64
                    && cells
                        .contains(&((y as usize * terrain.navigation.width + x as usize) as u32));
                if !inside {
                    break;
                }
                d += 0.25;
            }
        }
        if pathfinding::walkable(terrain.navigation, start, edge, MOVE, 0.15) {
            return Ok(true);
        }
        let cache_key = (
            tick / 40,
            terrain.navigation.versions.infantry,
            terrain.navigation.versions.obstruction,
        );
        if self.handoff_key != Some(cache_key) {
            self.handoff_key = Some(cache_key);
            self.handoffs.clear();
        }
        let path_key =
            [start.x, start.z, edge.x, edge.z, radius].map(|n| math::round(n * 4.0) as i64);
        if let Some(&answer) = self.handoffs.get(&path_key) {
            return Ok(answer);
        }
        let route = self
            .handoff_paths
            .find_path(
                terrain,
                PathRules::compiled().map_err(|e| e.to_string())?,
                rifle,
                start,
                edge,
                None,
            )
            .map_err(|e| format!("supply handoff: {e:?}"))?;
        let mut at = start;
        let mut length = 0.0;
        for p in &route.waypoints {
            length += distance(at, *p);
            at = *p;
        }
        let answer =
            !route.waypoints.is_empty() && distance(at, edge) < 1.0 && length <= radius * 2.0;
        self.handoffs.insert(path_key, answer);
        Ok(answer)
    }
}

// One source danger-view object belongs to one fixed Match recipient. Preserve
// insertion order for changed cells because region graph updates consume it.
fn refresh_danger(
    state: &mut OwnerRoutes,
    v: TerrainView<'_>,
    from: Point,
    refreshed: bool,
) -> Result<(), String> {
    let identity = (v.knowledge_id, v.width, v.height);
    let fresh = state.known_id != Some(identity) || state.flags.len() != v.flags.len();
    if fresh {
        state.known_id = Some(identity);
        state.danger_paths = Pathfinding::new();
        state.base = v.flags.to_vec();
        state.flags = v.flags.to_vec();
        state.heights = v
            .heights
            .map_or_else(|| vec![0; v.flags.len()], <[i8]>::to_vec);
        state.cleared.clear();
        state.changes.clear();
        state.version = 0;
    }
    let mut candidates = state.cleared.clone();
    let mut candidate_set = candidates
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    let mut height_changed = Vec::new();
    if fresh || refreshed {
        let mut next = v.flags.to_vec();
        for t in &state.threats {
            let radius = t.range + 3.0;
            for y in math::max(0.0, ((t.at.z - radius) / CELL).floor()) as usize
                ..math::min(v.height as f64, ((t.at.z + radius) / CELL).ceil()) as usize
            {
                for x in math::max(0.0, ((t.at.x - radius) / CELL).floor()) as usize
                    ..math::min(v.width as f64, ((t.at.x + radius) / CELL).ceil()) as usize
                {
                    if distance(
                        Point {
                            x: (x as f64 + 0.5) * CELL,
                            z: (y as f64 + 0.5) * CELL,
                        },
                        t.at,
                    ) <= radius
                    {
                        next[y * v.width + x] |= MOVE | VBLOCK;
                    }
                }
            }
        }
        for (c, &want) in next.iter().enumerate() {
            if want != state.base[c] {
                state.base[c] = want;
                if candidate_set.insert(c) {
                    candidates.push(c);
                }
            }
            let height = v.heights.map_or(0, |h| h[c]);
            if state.heights[c] != height {
                state.heights[c] = height;
                height_changed.push(c);
            }
        }
    }
    let mut cleared = Vec::new();
    for y in math::max(0.0, ((from.z - 4.0) / CELL).floor()) as usize
        ..math::min(v.height as f64, ((from.z + 4.0) / CELL).ceil()) as usize
    {
        for x in math::max(0.0, ((from.x - 4.0) / CELL).floor()) as usize
            ..math::min(v.width as f64, ((from.x + 4.0) / CELL).ceil()) as usize
        {
            let c = y * v.width + x;
            if distance(
                from,
                Point {
                    x: (x as f64 + 0.5) * CELL,
                    z: (y as f64 + 0.5) * CELL,
                },
            ) <= 4.0
            {
                cleared.push(c);
                if candidate_set.insert(c) {
                    candidates.push(c);
                }
            }
        }
    }
    let open = cleared
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    let mut changed = height_changed;
    let mut changed_set = changed
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    for c in candidates {
        let want = if open.contains(&c) {
            v.flags[c]
        } else {
            state.base[c]
        };
        if state.flags[c] != want {
            state.flags[c] = want;
            if changed_set.insert(c) {
                changed.push(c);
            }
        }
    }
    state.cleared = cleared;
    state.changed_from = state.version;
    if !changed.is_empty() {
        let next = state
            .version
            .checked_add(1)
            .ok_or("danger version exhausted")?;
        state.changes.push((state.version, next, changed.clone()));
        state.version = next;
        if state.changes.len() > 32 {
            state.changes.remove(0);
        }
    }
    state.changed = changed;
    Ok(())
}
fn validate_navigation(v: TerrainView<'_>) -> Result<usize, String> {
    let n = v
        .width
        .checked_mul(v.height)
        .ok_or("invalid routing shape")?;
    if n == 0
        || n > i32::MAX as usize
        || v.flags.len() != n
        || v.heights.is_some_and(|h| h.len() != n)
    {
        return Err("invalid routing shape".into());
    }
    Ok(n)
}
fn validate(t: PathTerrain<'_>, a: Point, b: Point) -> Result<(), String> {
    let n = validate_navigation(t.navigation)?;
    if t.near_walls.len() != n
        || t.known_mines.len() != n
        || t.fires.len() != n
        || t.wear.is_some_and(|w| w.len() != n)
    {
        return Err("invalid routing shape".into());
    }
    if [a.x, a.z, b.x, b.z, t.wetness]
        .iter()
        .any(|v| !v.is_finite())
        || !(0.0..=1.0).contains(&t.wetness)
    {
        return Err("invalid routing position or wetness".into());
    }
    Ok(())
}

fn observed_path(
    paths: &mut Pathfinding,
    terrain: PathTerrain<'_>,
    rules: PathRules,
    profile: &UnitKind,
    from: Point,
    to: Point,
) -> Result<crate::pathfinding::PathResult, String> {
    let initial = paths
        .find_path(terrain, rules, profile, from, to, None)
        .map_err(|e| format!("observed convoy path: {e:?}"))?;
    if !terrain.navigation.world_known
        && initial.waypoints.first().is_some_and(|p| {
            !pathfinding::walkable(terrain.navigation, from, *p, profile.block(), 0.9)
        })
    {
        let center = Point {
            x: (from.x / CELL).floor() * CELL + CELL / 2.0,
            z: (from.z / CELL).floor() * CELL + CELL / 2.0,
        };
        for at in [
            Point {
                x: center.x,
                z: from.z,
            },
            Point {
                x: from.x,
                z: center.z,
            },
            center,
        ] {
            if distance(from, at) < 0.05
                || !pathfinding::walkable(terrain.navigation, from, at, profile.block(), 0.9)
            {
                continue;
            }
            let mut route = paths
                .find_path(terrain, rules, profile, at, to, None)
                .map_err(|e| format!("observed convoy backout: {e:?}"))?;
            if route.waypoints.first().is_some_and(|p| {
                pathfinding::walkable(terrain.navigation, at, *p, profile.block(), 0.9)
            }) {
                route.waypoints.insert(0, at);
                route.route_end = initial.route_end;
                return Ok(route);
            }
        }
    }
    Ok(initial)
}
