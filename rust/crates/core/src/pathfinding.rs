//! Fine grid A* and path smoothing on explicitly supplied terrain and hazards.
//!
//! Supports infantry, naval and road-vehicle profiles at `e4e579a`.
//! Automatic recipient-view selection and request scheduling are separate work.
//! No game state is read behind the caller's view.

use crate::catalog::{Catalog, CatalogError, Value};
use crate::game::{CELL, FORD, MOVE, MUD, Point, ROAD, WIRE, WOOD};
use crate::math;
use crate::navigation::{Navigation, NavigationError, Options, TerrainView};
use crate::rules::UnitKind;
use crate::vehicle_navigation::VehicleNavigation;

/// Path cost and bunker clearance rules from the compiled catalog.
#[derive(Clone, Copy, Debug)]
pub struct PathRules {
    pub wall_hug: f64,
    pub bunker_radius: f64,
    pub road_speed: f64,
    pub ford_speed: [f64; 2],
    pub mud_speed: [f64; 2],
    pub wood_speed: f64,
    pub churn: f64,
    pub wet_ford: f64,
    pub wet_ground: f64,
}

impl PathRules {
    /// Load canonical path costs without runtime files or caller-provided JSON.
    pub fn compiled() -> Result<Self, CatalogError> {
        Self::from_catalog(Catalog::compiled()?)
    }

    /// Read path costs from the same catalog as the unit and movement rules.
    /// Reject unusable costs before a native match starts.
    pub fn from_catalog(catalog: &Catalog) -> Result<Self, CatalogError> {
        let cfg = catalog.table("CFG")?;
        let weather = cfg
            .get("weather")
            .ok_or_else(|| CatalogError("missing CFG.weather".into()))?;
        let wood = cfg
            .get("wood")
            .ok_or_else(|| CatalogError("missing CFG.wood".into()))?;
        let rules = Self {
            wall_hug: path_number(cfg, "wallHug")?,
            bunker_radius: path_number(catalog.unit("bunker")?, "radius")?,
            road_speed: path_number(cfg, "roadSpeed")?,
            ford_speed: path_pair(cfg, "fordSpeed")?,
            mud_speed: path_pair(cfg, "mudSpeed")?,
            wood_speed: path_number(wood, "speed")?,
            churn: path_number(cfg, "churn")?,
            wet_ford: path_number(weather, "wetFord")?,
            wet_ground: path_number(weather, "wetGround")?,
        };
        if rules.wall_hug < 0.0
            || rules.bunker_radius < 0.0
            || [
                rules.road_speed,
                rules.wood_speed,
                rules.ford_speed[0],
                rules.ford_speed[1],
                rules.mud_speed[0],
                rules.mud_speed[1],
            ]
            .iter()
            .any(|v| *v <= 0.0)
            || [rules.churn, rules.wet_ford, rules.wet_ground]
                .iter()
                .any(|v| !(0.0..1.0).contains(v))
        {
            return Err(CatalogError("invalid path cost or ground speed".into()));
        }
        Ok(rules)
    }
}

fn path_number(value: &Value, key: &str) -> Result<f64, CatalogError> {
    value
        .get(key)
        .and_then(Value::number)
        .filter(|n| n.is_finite())
        .ok_or_else(|| CatalogError(format!("path rule {key} must be a finite number")))
}
fn path_pair(value: &Value, key: &str) -> Result<[f64; 2], CatalogError> {
    let Some([a, b]) = value.get(key).and_then(Value::array) else {
        return Err(CatalogError(format!(
            "path rule {key} must have two numbers"
        )));
    };
    match (a.number(), b.number()) {
        (Some(a), Some(b)) if a.is_finite() && b.is_finite() => Ok([a, b]),
        _ => Err(CatalogError(format!(
            "path rule {key} must have finite numbers"
        ))),
    }
}

/// Each slice belongs to the same authoritative or recipient-owned terrain.
/// `bunkers` contains only live, visible/remembered command bunkers, in stable order.
#[derive(Clone, Copy)]
pub struct PathTerrain<'a> {
    pub navigation: TerrainView<'a>,
    pub near_walls: &'a [u8],
    pub known_mines: &'a [bool],
    pub fires: &'a [bool],
    pub bunkers: &'a [Point],
    /// Current observed wear (zero when absent), public wetness and road presence.
    /// Costs refresh on every search even when the navigation cache is reused.
    pub wear: Option<&'a [f32]>,
    pub wetness: f64,
    pub roads: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathError {
    InvalidShape,
    InvalidPosition,
    InvalidRules,
    UnsupportedProfile,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathStatus {
    Found,
    Unreachable,
    ExpansionLimit,
}

#[derive(Debug)]
pub struct PathResult {
    pub waypoints: Vec<Point>,
    pub route_end: Point,
    pub local_goal: usize,
    pub limited: bool,
    pub status: PathStatus,
    pub expansions: usize,
}

#[derive(Default)]
pub struct Pathfinding {
    navigation: Navigation,
    scratch: Scratch,
    vehicles: VehicleNavigation,
}

impl Clone for Pathfinding {
    /// Fork derived search history for a transaction. External terrain buffers
    /// keep their identities; owned vehicle buffers are rebound to this copy.
    /// Publish this value with the terrain preview on success, or drop on error.
    fn clone(&self) -> Self {
        let vehicles = self.vehicles.clone();
        let mut navigation = self.navigation.clone();
        navigation.rebind_flags(&self.vehicles.cloned_buffers(&vehicles));
        Self {
            navigation,
            scratch: self.scratch.clone(),
            vehicles,
        }
    }
}

impl Pathfinding {
    /// The routing guard validated that these arrays belong to the same
    /// recipient. Keep old versions so the next search applies its change log.
    pub(crate) fn fork_for_terrain(
        &self,
        source: TerrainView<'_>,
        prepared: TerrainView<'_>,
    ) -> Self {
        let mut fork = self.clone();
        fork.rebind_terrain(source, prepared);
        fork
    }
    pub(crate) fn rebind_terrain(&mut self, source: TerrainView<'_>, prepared: TerrainView<'_>) {
        let vehicle_buffers = self.vehicles.buffers_for(source);
        self.navigation
            .rebind_terrain(source, prepared, &vehicle_buffers);
        self.vehicles.rebind_terrain(source, prepared);
    }
}

#[derive(Clone, Default)]
struct Scratch {
    scores: Vec<f32>,
    came: Vec<usize>,
    seen: Vec<u32>,
    closed: Vec<u32>,
    generation: u32,
    heap: Heap,
    queue: Vec<usize>,
}

impl Scratch {
    fn prepare(&mut self, n: usize) {
        self.scores.resize(n, 0.0);
        self.came.resize(n, usize::MAX);
        self.seen.resize(n, 0);
        self.closed.resize(n, 0);
        self.generation = self.generation.wrapping_add(1);
        if self.generation == 0 {
            self.seen.fill(0);
            self.closed.fill(0);
            self.generation = 1;
        }
        self.heap.0.clear();
    }
    fn nearest_free(&mut self, terrain: TerrainView<'_>, at: Point, block: u16) -> usize {
        let x = ((at.x / CELL).floor().max(0.0) as usize).min(terrain.width - 1);
        let y = ((at.z / CELL).floor().max(0.0) as usize).min(terrain.height - 1);
        let start = y * terrain.width + x;
        if terrain.flags[start] & block == 0 {
            return start;
        }
        // A separate local visitation set leaves the A* generation unchanged.
        let mut seen = vec![false; terrain.flags.len()];
        self.queue.clear();
        self.queue.push(start);
        seen[start] = true;
        let mut head = 0;
        while head < self.queue.len() {
            let c = self.queue[head];
            head += 1;
            if terrain.flags[c] & block == 0 {
                return c;
            }
            for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let Some(n) = neighbor(terrain, c, dx, dy) else {
                    continue;
                };
                if !seen[n] {
                    seen[n] = true;
                    self.queue.push(n);
                }
            }
        }
        start
    }
}

impl Pathfinding {
    pub fn new() -> Self {
        Self::default()
    }

    /// One synchronous search. `None` uses the reference limit of min(cells, 16384).
    /// An expansion-limit result is distinct from a proven disconnection.
    pub fn find_path(
        &mut self,
        terrain: PathTerrain<'_>,
        rules: PathRules,
        profile: &UnitKind,
        from: Point,
        to: Point,
        expansion_limit: Option<usize>,
    ) -> Result<PathResult, PathError> {
        if profile.air || profile.rail || profile.structure {
            return Err(PathError::UnsupportedProfile);
        }
        let v = terrain.navigation;
        let n = v
            .width
            .checked_mul(v.height)
            .ok_or(PathError::InvalidShape)?;
        if n == 0
            || n > i32::MAX as usize
            || v.flags.len() != n
            || v.heights.is_some_and(|h| h.len() != n)
            || terrain.wear.is_some_and(|w| w.len() != n)
            || terrain.near_walls.len() != n
            || terrain.known_mines.len() != n
            || terrain.fires.len() != n
        {
            return Err(PathError::InvalidShape);
        }
        if !finite(from) || !finite(to) || terrain.bunkers.iter().any(|p| !finite(*p)) {
            return Err(PathError::InvalidPosition);
        }
        if !rules.wall_hug.is_finite()
            || rules.wall_hug < 0.0
            || !rules.bunker_radius.is_finite()
            || rules.bunker_radius < 0.0
        {
            return Err(PathError::InvalidRules);
        }
        let vehicle = profile.ground_vehicle();
        if vehicle
            && (!profile.radius.is_finite()
                || profile.radius <= 0.0
                || !terrain.wetness.is_finite()
                || !(0.0..=1.0).contains(&terrain.wetness)
                || terrain
                    .wear
                    .is_some_and(|w| w.iter().any(|v| !v.is_finite() || !(0.0..=1.0).contains(v)))
                || [
                    rules.road_speed,
                    rules.wood_speed,
                    rules.ford_speed[0],
                    rules.ford_speed[1],
                    rules.mud_speed[0],
                    rules.mud_speed[1],
                ]
                .iter()
                .any(|v| !v.is_finite() || *v <= 0.0)
                || [rules.churn, rules.wet_ford, rules.wet_ground]
                    .iter()
                    .any(|v| !v.is_finite() || !(0.0..1.0).contains(v)))
        {
            return Err(PathError::InvalidRules);
        }
        let block = profile.block();
        let source = v;
        let v = if vehicle {
            self.vehicles.view(v, profile.radius, block)
        } else {
            v
        };
        let pull = if profile.naval {
            block
        } else if vehicle {
            block | MUD | WOOD
        } else {
            MOVE | WIRE
        };
        let roads = vehicle && terrain.roads;
        let low = if roads { 1.0 / rules.road_speed } else { 1.0 };
        self.scratch.prepare(n);
        let full_goal = self.scratch.nearest_free(v, to, block);
        let stand = cell_of(v, from).unwrap_or(0);
        let start = if v.flags[stand] & block != 0 {
            self.scratch.nearest_free(v, from, block)
        } else {
            stand
        };
        let escape = (start != stand).then(|| center(v, start));
        let leg = self
            .navigation
            .leg(v, start, full_goal, block, Options::default())
            .map_err(|e| match e {
                NavigationError::InvalidShape => PathError::InvalidShape,
                NavigationError::CellOutOfBounds => PathError::InvalidPosition,
            })?;
        let goal = leg.goal;
        let route_end = if Some(full_goal) == cell_of(v, to)
            && (!vehicle
                || crate::vehicle_navigation::destination_clear(source, to, profile.radius, block))
        {
            to
        } else {
            center(v, full_goal)
        };
        let mut result = PathResult {
            waypoints: Vec::new(),
            route_end,
            local_goal: goal,
            limited: leg.limited,
            status: PathStatus::Unreachable,
            expansions: 0,
        };
        if !leg.connected {
            return Ok(result);
        }
        // A directed coarse route also proves membership in regionsFor's weaker
        // undirected components. Its additional small-map check cannot reject it.
        let scratch = &mut self.scratch;
        let gen_id = scratch.generation;
        let hq = |c: usize| {
            let dx = (c % v.width).abs_diff(goal % v.width) as f64;
            let dy = (c / v.width).abs_diff(goal / v.width) as f64;
            (dx.max(dy) + 0.414 * dx.min(dy)) * low
        };
        scratch.heap.push(hq(start), start);
        scratch.scores[start] = 0.0;
        scratch.seen[start] = gen_id;
        scratch.came[start] = usize::MAX;
        let limit = expansion_limit.unwrap_or(n.min(16384));
        while let Some(c) = scratch.heap.pop() {
            if c == goal {
                break;
            }
            if scratch.closed[c] == gen_id {
                continue;
            }
            scratch.closed[c] = gen_id;
            result.expansions += 1;
            if result.expansions > limit {
                result.status = PathStatus::ExpansionLimit;
                return Ok(result);
            }
            for dy in -1..=1 {
                for dx in -1..=1 {
                    if dx == 0 && dy == 0 {
                        continue;
                    }
                    let Some(next) = neighbor(v, c, dx, dy) else {
                        continue;
                    };
                    if v.flags[next] & block != 0
                        || scratch.closed[next] == gen_id
                        || !leg.contains(next)
                    {
                        continue;
                    }
                    if dx != 0 && dy != 0 {
                        let a = neighbor(v, c, dx, 0).expect("in-bounds corner");
                        let b = neighbor(v, c, 0, dy).expect("in-bounds corner");
                        if v.flags[a] & block != 0
                            || v.flags[b] & block != 0
                            || terrain.known_mines[a]
                            || terrain.known_mines[b]
                            || (level(v, a) - level(v, c)).abs() > 1
                            || (level(v, b) - level(v, c)).abs() > 1
                        {
                            continue;
                        }
                    }
                    let climb = level(v, next) - level(v, c);
                    if climb.abs() > 1 {
                        continue;
                    }
                    let mut post_penalty = 0.0;
                    let at = center(v, next);
                    for &post in terrain.bunkers {
                        let d = distance(at, post);
                        post_penalty += if d < rules.bunker_radius + 1.0 {
                            30.0
                        } else if d < rules.bunker_radius + 3.0 {
                            rules.wall_hug
                        } else {
                            0.0
                        };
                    }
                    let move_cost = if vehicle {
                        low.max(1.0 / ground_mul(terrain, rules, next))
                    } else {
                        1.0
                    };
                    let cost = f64::from(scratch.scores[c])
                        + (if dx != 0 && dy != 0 { 1.414 } else { 1.0 }) * move_cost
                        + f64::from(climb.max(0)) * 0.5
                        + f64::from(terrain.near_walls[next]) * rules.wall_hug
                        + post_penalty
                        + if !vehicle && v.flags[next] & WIRE != 0 {
                            4.0
                        } else {
                            0.0
                        }
                        + if terrain.fires[next] { 8.0 } else { 0.0 }
                        + if terrain.known_mines[next] { 30.0 } else { 0.0 };
                    if scratch.seen[next] != gen_id || cost < f64::from(scratch.scores[next]) {
                        scratch.scores[next] = cost as f32;
                        scratch.came[next] = c;
                        scratch.seen[next] = gen_id;
                        scratch.heap.push(cost + hq(next), next);
                    }
                }
            }
        }
        if goal != start && (scratch.seen[goal] != gen_id || scratch.came[goal] == usize::MAX) {
            return Ok(result);
        }
        result.status = PathStatus::Found;
        if goal == start {
            if let Some(escape) = escape {
                result.waypoints = vec![escape, route_end];
                return Ok(result);
            }
            if distance(from, route_end) > 0.05 && walkable(v, from, route_end, block, 0.9) {
                result.waypoints.push(route_end);
                return Ok(result);
            }
        }
        let mut points = Vec::new();
        let mut c = goal;
        while c != start && c != usize::MAX {
            points.push(center(v, c));
            c = scratch.came[c];
        }
        points.reverse();
        if !leg.limited
            && goal == full_goal
            && let Some(last) = points.last_mut()
        {
            *last = route_end;
        }
        let mut at = escape.unwrap_or(from);
        if let Some(escape) = escape {
            result.waypoints.push(escape);
        }
        let found_mines = terrain.known_mines.iter().any(|&mine| mine);
        let mut i = 0;
        while i < points.len() {
            let mut j = points.len() - 1;
            if roads {
                for (k, point) in points.iter().enumerate().take(j).skip(i) {
                    if cell_of(v, *point).is_some_and(|c| v.flags[c] & ROAD != 0) {
                        j = k;
                        break;
                    }
                }
            }
            while j > i
                && ((!profile.naval && !walkable(v, at, points[j], block, 0.9))
                    || !walkable(v, at, points[j], pull, 2.0)
                    || (found_mines && mined_line(terrain, at, points[j]))
                    || terrain.bunkers.iter().any(|&p| {
                        distance(at, p) >= rules.bunker_radius + 1.5
                            && distance(points[j], p) >= rules.bunker_radius + 1.5
                            && segment_hits(at, points[j], p, rules.bunker_radius + 1.5)
                    }))
            {
                j -= 1;
            }
            result.waypoints.push(points[j]);
            at = points[j];
            i = j + 1;
        }
        Ok(result)
    }
}

fn ground_mul(terrain: PathTerrain<'_>, rules: PathRules, c: usize) -> f64 {
    let f = terrain.navigation.flags[c];
    let wear = terrain.wear.map_or(0.0, |w| f64::from(w[c]));
    let wet = terrain.wetness
        * if level(terrain.navigation, c) < 0 {
            2.0
        } else {
            1.0
        };
    let lerp = |a: f64, b: f64| a + (b - a) * wear;
    if f & FORD != 0 {
        lerp(rules.ford_speed[0], rules.ford_speed[1]) * (1.0 - rules.wet_ford * wet.min(1.0))
    } else if f & WOOD != 0 {
        rules.wood_speed
    } else if f & ROAD != 0 {
        lerp(rules.road_speed, 1.0)
    } else if f & MUD != 0 {
        lerp(rules.mud_speed[0], rules.mud_speed[1]) * (1.0 - rules.wet_ground * wet)
    } else {
        (1.0 - rules.churn * wear) * (1.0 - rules.wet_ground * wet)
    }
}

fn finite(p: Point) -> bool {
    // Keep grid stepping representable even for out-of-map destinations.
    let bound = f64::from(i32::MAX) * CELL;
    p.x.is_finite() && p.z.is_finite() && p.x.abs() <= bound && p.z.abs() <= bound
}
fn distance(a: Point, b: Point) -> f64 {
    math::hypot(b.x - a.x, b.z - a.z)
}
fn center(v: TerrainView<'_>, c: usize) -> Point {
    Point {
        x: (c % v.width) as f64 * CELL + CELL / 2.0,
        z: (c / v.width) as f64 * CELL + CELL / 2.0,
    }
}
fn cell_of(v: TerrainView<'_>, p: Point) -> Option<usize> {
    let x = (p.x / CELL).floor();
    let y = (p.z / CELL).floor();
    if x < 0.0 || y < 0.0 || x >= v.width as f64 || y >= v.height as f64 {
        None
    } else {
        Some(y as usize * v.width + x as usize)
    }
}
fn level(v: TerrainView<'_>, c: usize) -> i16 {
    v.heights.map_or(0, |h| i16::from(h[c]))
}
fn neighbor(v: TerrainView<'_>, c: usize, dx: isize, dy: isize) -> Option<usize> {
    let x = (c % v.width).checked_add_signed(dx)?;
    let y = (c / v.width).checked_add_signed(dy)?;
    (x < v.width && y < v.height).then_some(y * v.width + x)
}
fn segment_hits(a: Point, b: Point, p: Point, radius: f64) -> bool {
    let dx = b.x - a.x;
    let dz = b.z - a.z;
    let l2 = dx * dx + dz * dz;
    let t =
        (((p.x - a.x) * dx + (p.z - a.z) * dz) / if l2 != 0.0 { l2 } else { 1.0 }).clamp(0.0, 1.0);
    distance(
        Point {
            x: a.x + dx * t,
            z: a.z + dz * t,
        },
        p,
    ) < radius
}
fn mined_line(terrain: PathTerrain<'_>, a: Point, b: Point) -> bool {
    let d = distance(a, b);
    let d = if d != 0.0 { d } else { 1.0 };
    let n = (d * 4.0).ceil() as usize;
    let ox = -(b.z - a.z) / d * 0.5;
    let oz = (b.x - a.x) / d * 0.5;
    for k in 0..=n {
        for s in [-1.0, 0.0, 1.0] {
            let p = Point {
                x: a.x + (b.x - a.x) * k as f64 / n as f64 + ox * s,
                z: a.z + (b.z - a.z) * k as f64 / n as f64 + oz * s,
            };
            if cell_of(terrain.navigation, p).is_some_and(|c| terrain.known_mines[c]) {
                return true;
            }
        }
    }
    false
}
pub(crate) fn walkable(v: TerrainView<'_>, a: Point, b: Point, mask: u16, half: f64) -> bool {
    let d = distance(a, b);
    let d = if d != 0.0 { d } else { 1.0 };
    let ox = -(b.z - a.z) / d * half;
    let oz = (b.x - a.x) / d * half;
    [-1.0, 0.0, 1.0].iter().all(|&k| {
        clear(
            v,
            Point {
                x: a.x + ox * k,
                z: a.z + oz * k,
            },
            Point {
                x: b.x + ox * k,
                z: b.z + oz * k,
            },
            mask,
        )
    }) && cliff_clear(v, a, b)
}
fn clear(v: TerrainView<'_>, a: Point, b: Point, mask: u16) -> bool {
    let mut x = (a.x / CELL).floor() as i64;
    let mut y = (a.z / CELL).floor() as i64;
    let ex = (b.x / CELL).floor() as i64;
    let ey = (b.z / CELL).floor() as i64;
    let dx = b.x - a.x;
    let dz = b.z - a.z;
    let sx = step(dx);
    let sy = step(dz);
    let tx_step = if dx != 0.0 {
        CELL / dx.abs()
    } else {
        f64::INFINITY
    };
    let ty_step = if dz != 0.0 {
        CELL / dz.abs()
    } else {
        f64::INFINITY
    };
    let mut tx = first_cross(a.x, x, dx, sx);
    let mut ty = first_cross(a.z, y, dz, sy);
    for _ in 0..v.width + v.height {
        if x == ex && y == ey {
            return true;
        }
        if y == ey || (x != ex && tx < ty) {
            tx += tx_step;
            x += sx;
        } else {
            ty += ty_step;
            y += sy;
        }
        if x < 0 || y < 0 || x >= v.width as i64 || y >= v.height as i64 {
            return false;
        }
        if v.flags[y as usize * v.width + x as usize] & mask != 0 {
            return false;
        }
    }
    true
}
fn step(x: f64) -> i64 {
    if x < 0.0 {
        -1
    } else if x > 0.0 {
        1
    } else {
        0
    }
}
fn first_cross(p: f64, cell: i64, delta: f64, direction: i64) -> f64 {
    if delta == 0.0 {
        f64::INFINITY
    } else {
        (if direction > 0 {
            (cell + 1) as f64 * CELL - p
        } else {
            p - cell as f64 * CELL
        }) / delta.abs()
    }
}
fn cliff_clear(v: TerrainView<'_>, a: Point, b: Point) -> bool {
    if v.heights.is_none() {
        return true;
    }
    let mut x = (a.x / CELL).floor() as i64;
    let mut y = (a.z / CELL).floor() as i64;
    let ex = (b.x / CELL).floor() as i64;
    let ey = (b.z / CELL).floor() as i64;
    let dx = b.x - a.x;
    let dz = b.z - a.z;
    let sx = step(dx);
    let sy = step(dz);
    let tx_step = if dx != 0.0 {
        CELL / dx.abs()
    } else {
        f64::INFINITY
    };
    let ty_step = if dz != 0.0 {
        CELL / dz.abs()
    } else {
        f64::INFINITY
    };
    let mut tx = first_cross(a.x, x, dx, sx);
    let mut ty = first_cross(a.z, y, dz, sy);
    let off = |x: i64, y: i64| x < 0 || y < 0 || x >= v.width as i64 || y >= v.height as i64;
    let at = |x: i64, y: i64| level(v, y as usize * v.width + x as usize);
    if off(x, y) {
        return false;
    }
    let mut height = at(x, y);
    for _ in 0..v.width + v.height {
        if x == ex && y == ey {
            return true;
        }
        if x != ex && y != ey && (tx - ty).abs() < 1e-12 {
            let nx = x + sx;
            let ny = y + sy;
            if off(nx, ny) {
                return false;
            }
            if [at(nx, y), at(x, ny), at(nx, ny)]
                .iter()
                .any(|h| (h - height).abs() > 1)
            {
                return false;
            }
            x = nx;
            y = ny;
            tx += tx_step;
            ty += ty_step;
        } else if y == ey || (x != ex && tx < ty) {
            x += sx;
            tx += tx_step;
        } else {
            y += sy;
            ty += ty_step;
        }
        if off(x, y) {
            return false;
        }
        let next = at(x, y);
        if (next - height).abs() > 1 {
            return false;
        }
        height = next;
    }
    x == ex && y == ey
}

#[derive(Clone, Default)]
struct Heap(Vec<(f64, usize)>);
impl Heap {
    fn push(&mut self, score: f64, cell: usize) {
        let mut i = self.0.len();
        self.0.push((score, cell));
        while i > 0 {
            let p = (i - 1) / 2;
            if self.0[p].0 <= score {
                break;
            }
            self.0[i] = self.0[p];
            i = p;
        }
        self.0[i] = (score, cell);
    }
    fn pop(&mut self) -> Option<usize> {
        let top = self.0.first()?.1;
        let last = self.0.pop()?;
        if !self.0.is_empty() {
            self.0[0] = last;
            let mut i = 0;
            loop {
                let l = 2 * i + 1;
                let r = l + 1;
                let mut m = i;
                if l < self.0.len() && self.0[l].0 < self.0[m].0 {
                    m = l;
                }
                if r < self.0.len() && self.0[r].0 < self.0[m].0 {
                    m = r;
                }
                if m == i {
                    break;
                }
                self.0.swap(i, m);
                i = m;
            }
        }
        Some(top)
    }
}
