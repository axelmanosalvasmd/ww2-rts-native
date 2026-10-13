//! Real convoy routing and physical handoff over explicit recipient and authoritative terrain.
use crate::{
    game::{CELL, MOVE, Point, VBLOCK},
    math,
    navigation::{TerrainView, Versions},
    pathfinding::{self, PathRules, PathTerrain, Pathfinding},
    rules::UnitKind,
    supply_convoys::{Route, RouteFailure},
};
use std::collections::BTreeMap;
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
    heights: Vec<i8>,
    version: u64,
    paths: Pathfinding,
}
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
                path: vec![to],
                failure: None,
            });
        }
        let state = self.owners.entry(owner).or_default();
        let key = (tick / 40, known.navigation.versions.knowledge);
        if safe && state.key != Some(key) {
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
                path: vec![],
                failure: Some(RouteFailure::Dangerous),
            });
        }
        let terrain = if danger {
            let v = known.navigation;
            let mut flags = v.flags.to_vec();
            for t in &state.threats {
                let radius = t.range + 3.0;
                for y in math::max(0.0, ((t.at.z - radius) / CELL).floor()) as usize
                    ..math::min(v.height as f64, ((t.at.z + radius) / CELL).ceil()) as usize
                {
                    for x in math::max(0.0, ((t.at.x - radius) / CELL).floor()) as usize
                        ..math::min(v.width as f64, ((t.at.x + radius) / CELL).ceil()) as usize
                    {
                        let p = Point {
                            x: (x as f64 + 0.5) * CELL,
                            z: (y as f64 + 0.5) * CELL,
                        };
                        if distance(p, t.at) <= radius {
                            flags[y * v.width + x] |= MOVE | VBLOCK;
                        }
                    }
                }
            }
            for y in math::max(0.0, ((from.z - 4.0) / CELL).floor()) as usize
                ..math::min(v.height as f64, ((from.z + 4.0) / CELL).ceil()) as usize
            {
                for x in math::max(0.0, ((from.x - 4.0) / CELL).floor()) as usize
                    ..math::min(v.width as f64, ((from.x + 4.0) / CELL).ceil()) as usize
                {
                    if distance(
                        from,
                        Point {
                            x: (x as f64 + 0.5) * CELL,
                            z: (y as f64 + 0.5) * CELL,
                        },
                    ) <= 4.0
                    {
                        flags[y * v.width + x] = v.flags[y * v.width + x];
                    }
                }
            }
            let heights = v
                .heights
                .map_or_else(|| vec![0; v.flags.len()], <[i8]>::to_vec);
            if state.flags != flags || state.heights != heights {
                state.heights = heights;
                state.version = state.version.wrapping_add(1);
                state.flags = flags;
            }
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
                    changed: None,
                    changes: &[],
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
                .paths
                .find_path(terrain, rules, profile, from, to, None)
                .map_err(|e| format!("convoy path: {e:?}"))?
        } else {
            observed_path(&mut state.paths, known, rules, profile, from, to)?
        };
        let mut path = first.waypoints;
        if path.is_empty() {
            let failure = if danger {
                let ordinary = observed_path(&mut state.paths, known, rules, profile, from, to)?;
                if ordinary.waypoints.is_empty() {
                    RouteFailure::Unreachable
                } else {
                    RouteFailure::Dangerous
                }
            } else {
                RouteFailure::Unreachable
            };
            return Ok(Route {
                path,
                failure: Some(failure),
            });
        }
        if distance(*path.last().unwrap(), to) <= 3.0 {
            return Ok(Route {
                path,
                failure: None,
            });
        }
        let route_end = first.route_end;
        let mut at = *path.last().unwrap();
        for _ in 0..512 {
            let leg = state
                .paths
                .find_path(terrain, rules, profile, at, to, None)
                .map_err(|e| format!("convoy leg: {e:?}"))?;
            let Some(&end) = leg.waypoints.last() else {
                return Ok(Route {
                    path: vec![],
                    failure: Some(RouteFailure::Unreachable),
                });
            };
            if distance(at, end) < 0.05 {
                return Ok(Route {
                    path: vec![],
                    failure: Some(RouteFailure::Unreachable),
                });
            }
            path.extend(leg.waypoints);
            if distance(end, to) <= 3.0 || distance(end, route_end) <= 0.25 {
                return Ok(Route {
                    path,
                    failure: None,
                });
            }
            at = end;
        }
        Ok(Route {
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
        if path.is_empty() || stuck > 0.5 {
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

fn validate(t: PathTerrain<'_>, a: Point, b: Point) -> Result<(), String> {
    let v = t.navigation;
    let n = v
        .width
        .checked_mul(v.height)
        .ok_or("invalid routing shape")?;
    if n == 0
        || n > i32::MAX as usize
        || v.flags.len() != n
        || v.heights.is_some_and(|h| h.len() != n)
        || t.near_walls.len() != n
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
                return Ok(route);
            }
        }
    }
    Ok(initial)
}
