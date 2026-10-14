//! Chunk regions and directed portals from `shared/navigation.js` at `e4e579a`.
//!
//! This is the coarse prerequisite for fine pathfinding, not a full pathfinder.
//! A cache belongs to one authoritative or recipient-owned terrain view. The
//! caller supplies every terrain value and advances versions after mutations.

use crate::math;
use std::collections::BTreeMap;

const BLOCKED: usize = usize::MAX;
const CARDINAL: [(isize, isize); 4] = [(-1, 0), (1, 0), (0, -1), (0, 1)];
const PORTALS: [(isize, isize); 4] = [(1, 0), (0, 1), (1, 1), (-1, 1)];
const ALL: [(isize, isize); 8] = [
    (-1, 0),
    (1, 0),
    (0, -1),
    (0, 1),
    (-1, -1),
    (1, -1),
    (-1, 1),
    (1, 1),
];

/// The version selected by the movement mask, plus shared invalidation counters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Versions {
    pub infantry: u64,
    pub vehicle: u64,
    pub naval: u64,
    pub obstruction: u64,
    pub knowledge: u64,
}

impl Versions {
    fn region(self, block: u16) -> u64 {
        if block & (1 << 10) != 0 {
            self.naval
        } else if block == 1 {
            self.infantry
        } else {
            self.vehicle
        }
    }
}

/// All changed cells between two recipient-knowledge versions.
#[derive(Clone, Copy, Debug)]
pub struct TerrainChange<'a> {
    pub from: u64,
    pub to: u64,
    pub cells: &'a [usize],
}

/// Borrowed terrain. No world or game object is consulted behind this view.
#[derive(Clone, Copy, Debug)]
pub struct TerrainView<'a> {
    pub width: usize,
    pub height: usize,
    pub flags: &'a [u16],
    pub heights: Option<&'a [i8]>,
    pub world_known: bool,
    /// Identity of the recipient's knowledge object; zero can mean no object.
    pub knowledge_id: u64,
    pub versions: Versions,
    /// A direct change set, preferred over the bounded change history.
    /// Only `from` is checked here, matching the reference direct-update input.
    pub changed: Option<TerrainChange<'a>>,
    /// At most the last 32 records are considered, in their supplied order.
    pub changes: &'a [TerrainChange<'a>],
}

impl TerrainView<'_> {
    fn level(self, c: usize) -> i16 {
        self.heights.map_or(0, |h| i16::from(h[c]))
    }
    fn validate(self) -> Result<usize, NavigationError> {
        let n = self
            .width
            .checked_mul(self.height)
            .ok_or(NavigationError::InvalidShape)?;
        if self.width == 0
            || self.height == 0
            || n > i32::MAX as usize
            || self.flags.len() != n
            || self.heights.is_some_and(|h| h.len() != n)
        {
            return Err(NavigationError::InvalidShape);
        }
        Ok(n)
    }
}

/// Positive cell counts. Zero selects the JavaScript defaults; chunks clamp to 2.
#[derive(Clone, Copy, Debug)]
pub struct Options {
    pub chunk_size: usize,
    pub max_leg_cells: usize,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            chunk_size: 16,
            max_leg_cells: 32,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavigationError {
    InvalidShape,
    CellOutOfBounds,
}

/// Coarse result, including the immediate fine-search filter.
///
/// The borrow prevents a terrain-cache update while its region IDs are in use.
#[derive(Debug)]
pub struct NavigationLeg<'a> {
    pub goal: usize,
    pub connected: bool,
    pub limited: bool,
    pub allowed: Vec<usize>,
    labels: &'a [usize],
    unrestricted: bool,
}
impl NavigationLeg<'_> {
    pub fn contains(&self, cell: usize) -> bool {
        self.unrestricted
            || self
                .labels
                .get(cell)
                .is_some_and(|id| self.allowed.contains(id))
    }
}

#[derive(Clone, Debug, Default)]
pub struct Navigation {
    graphs: BTreeMap<(u16, usize), Graph>,
}

impl Navigation {
    pub(crate) fn rebind_terrain(
        &mut self,
        source: TerrainView<'_>,
        prepared: TerrainView<'_>,
        vehicle_buffers: &[usize],
    ) {
        for graph in self.graphs.values_mut() {
            let id = &mut graph.identity;
            if id.width == source.width
                && id.height == source.height
                && id.knowledge_id == source.knowledge_id
                && id.heights == source.heights.map(|v| v.as_ptr() as usize)
                && (id.flags == source.flags.as_ptr() as usize
                    || vehicle_buffers.contains(&id.flags))
            {
                id.heights = prepared.heights.map(|v| v.as_ptr() as usize);
                if id.flags == source.flags.as_ptr() as usize {
                    id.flags = prepared.flags.as_ptr() as usize;
                }
            }
        }
    }

    pub(crate) fn rebind_flags(&mut self, buffers: &[(usize, usize)]) {
        for graph in self.graphs.values_mut() {
            if let Some((_, new)) = buffers.iter().find(|(old, _)| *old == graph.identity.flags) {
                graph.identity.flags = *new;
            }
        }
    }

    pub fn new() -> Self {
        Self::default()
    }

    /// Plan one local target. Advance the supplied versions when editing terrain.
    pub fn leg<'a>(
        &'a mut self,
        view: TerrainView<'_>,
        start: usize,
        goal: usize,
        block: u16,
        options: Options,
    ) -> Result<NavigationLeg<'a>, NavigationError> {
        let n = view.validate()?;
        if start >= n || goal >= n {
            return Err(NavigationError::CellOutOfBounds);
        }
        let chunk = if options.chunk_size == 0 {
            16
        } else {
            options.chunk_size.max(2)
        };
        let limit = if options.max_leg_cells == 0 {
            32
        } else {
            options.max_leg_cells
        };
        let key = (block, chunk);
        let graph = self
            .graphs
            .entry(key)
            .or_insert_with(|| Graph::build(view, block, chunk));
        let identity = Identity::new(view, block);
        if graph.identity != identity {
            let changed = if graph.identity.same_storage(identity)
                && graph.identity.knowledge != identity.knowledge
            {
                changes_since(view, graph.identity.knowledge)
            } else {
                None
            };
            if let Some(cells) = changed {
                graph.update(view, &cells, block, chunk);
                graph.identity = identity;
            } else {
                *graph = Graph::build(view, block, chunk);
            }
        }
        let (target, connected, limited, allowed, unrestricted) =
            graph.plan(view, start, goal, limit);
        Ok(NavigationLeg {
            goal: target,
            connected,
            limited,
            allowed,
            labels: &graph.labels,
            unrestricted,
        })
    }
}

fn changes_since(view: TerrainView<'_>, from: u64) -> Option<Vec<usize>> {
    if let Some(change) = view.changed.filter(|c| c.from == from) {
        return Some(change.cells.to_vec());
    }
    let mut version = from;
    let mut cells = Vec::new();
    for change in &view.changes[view.changes.len().saturating_sub(32)..] {
        if change.from != version || change.to == version {
            continue;
        }
        for &cell in change.cells {
            if !cells.contains(&cell) {
                cells.push(cell);
            }
        }
        version = change.to;
        if version == view.versions.knowledge {
            return Some(cells);
        }
    }
    None
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Identity {
    width: usize,
    height: usize,
    flags: usize,
    heights: Option<usize>,
    region: u64,
    obstruction: u64,
    knowledge_id: u64,
    knowledge: u64,
}
impl Identity {
    fn new(v: TerrainView<'_>, block: u16) -> Self {
        Self {
            width: v.width,
            height: v.height,
            flags: v.flags.as_ptr() as usize,
            heights: v.heights.map(|h| h.as_ptr() as usize),
            region: v.versions.region(block),
            obstruction: v.versions.obstruction,
            knowledge_id: v.knowledge_id,
            knowledge: v.versions.knowledge,
        }
    }
    // Slice identity is the reference typed-array identity. A Navigation value
    // stays with one map or recipient; its owner advances versions after edits.
    fn same_storage(self, other: Self) -> bool {
        self.width == other.width
            && self.height == other.height
            && self.flags == other.flags
            && self.heights == other.heights
            && self.knowledge_id == other.knowledge_id
    }
}

#[derive(Clone, Debug)]
struct Edge {
    region: usize,
    crossings: Vec<(usize, usize)>,
}
// Edges and incoming IDs preserve JavaScript Map/Set insertion order. Numeric
// sorting here would change tied routes and incremental-update history.
#[derive(Clone, Debug)]
struct Region {
    x: f64,
    y: f64,
    edges: Vec<Edge>,
    incoming: Vec<usize>,
}
impl Region {
    fn new(x: f64, y: f64) -> Self {
        Self {
            x,
            y,
            edges: Vec::new(),
            incoming: Vec::new(),
        }
    }
}
#[derive(Clone, Debug)]
struct CachedRoute {
    start: usize,
    goal: usize,
    route: Option<Vec<usize>>,
}

#[derive(Clone, Debug)]
struct Graph {
    identity: Identity,
    labels: Vec<usize>,
    regions: Vec<Option<Region>>,
    chunks: Vec<Vec<usize>>,
    free: Vec<usize>,
    routes: Vec<CachedRoute>,
    seen: Vec<u32>,
    generation: u32,
    came: Vec<usize>,
    queue: Vec<(usize, usize)>,
}

impl Graph {
    fn region(&self, id: usize) -> &Region {
        self.regions[id].as_ref().expect("live region")
    }
    fn region_mut(&mut self, id: usize) -> &mut Region {
        self.regions[id].as_mut().expect("live region")
    }
    fn bounds(view: TerrainView<'_>, chunk: usize, id: usize) -> (usize, usize, usize, usize) {
        let cw = view.width.div_ceil(chunk);
        let x = id % cw * chunk;
        let y = id / cw * chunk;
        (
            x,
            y,
            view.width.min(x.saturating_add(chunk)),
            view.height.min(y.saturating_add(chunk)),
        )
    }
    // The reference skips interior portal scans for flat, fully open chunks.
    // Preserve that traversal order as well as the shortcut itself.
    fn build(view: TerrainView<'_>, block: u16, chunk: usize) -> Self {
        let mut graph = Self {
            identity: Identity::new(view, block),
            labels: view
                .flags
                .iter()
                .map(|f| if f & block != 0 { BLOCKED } else { 0 })
                .collect(),
            regions: vec![None],
            chunks: vec![Vec::new(); view.width.div_ceil(chunk) * view.height.div_ceil(chunk)],
            free: Vec::new(),
            routes: Vec::new(),
            seen: vec![0; view.flags.len()],
            generation: 0,
            came: vec![0; view.flags.len()],
            queue: Vec::new(),
        };
        let low = (0..view.flags.len()).map(|c| view.level(c)).min().unwrap();
        let high = (0..view.flags.len()).map(|c| view.level(c)).max().unwrap();
        let no_cliffs = high - low <= 1;
        let mut open_chunks = vec![false; graph.chunks.len()];
        for (id, open_slot) in open_chunks.iter_mut().enumerate() {
            let (cx, cy, xmax, ymax) = Self::bounds(view, chunk, id);
            let mut open = true;
            let mut low = i16::MAX;
            let mut high = i16::MIN;
            'scan: for y in cy..ymax {
                for x in cx..xmax {
                    let c = y * view.width + x;
                    if graph.labels[c] == BLOCKED {
                        open = false;
                        break 'scan;
                    }
                    if !no_cliffs {
                        low = low.min(view.level(c));
                        high = high.max(view.level(c));
                        if high - low > 1 {
                            open = false;
                            break 'scan;
                        }
                    }
                }
            }
            if open {
                *open_slot = true;
                let region = graph.regions.len();
                graph.regions.push(Some(Region::new(
                    (cx + xmax - 1) as f64 / 2.0,
                    (cy + ymax - 1) as f64 / 2.0,
                )));
                graph.chunks[id].push(region);
                for y in cy..ymax {
                    graph.labels[y * view.width + cx..y * view.width + xmax].fill(region);
                }
            } else {
                graph.fill_chunk(view, chunk, id);
            }
        }
        if no_cliffs {
            for y in 0..view.height {
                if y % chunk == chunk - 1 {
                    for x in 0..view.width {
                        graph.connect(view, x, y, &PORTALS, None);
                    }
                } else {
                    for x in (0..view.width).step_by(chunk) {
                        graph.connect(view, x, y, &PORTALS, None);
                        let end = (view.width - 1).min(x.saturating_add(chunk - 1));
                        if end != x {
                            graph.connect(view, end, y, &PORTALS, None);
                        }
                    }
                }
            }
        } else {
            for (id, &open) in open_chunks.iter().enumerate() {
                let (cx, cy, xmax, ymax) = Self::bounds(view, chunk, id);
                for y in cy..ymax {
                    if !open || y == ymax - 1 {
                        for x in cx..xmax {
                            graph.connect(view, x, y, &PORTALS, None);
                        }
                    } else {
                        graph.connect(view, cx, y, &PORTALS, None);
                        if xmax - 1 != cx {
                            graph.connect(view, xmax - 1, y, &PORTALS, None);
                        }
                    }
                }
            }
        }
        graph
    }
    fn fill_chunk(&mut self, view: TerrainView<'_>, chunk: usize, id: usize) {
        let (cx, cy, xmax, ymax) = Self::bounds(view, chunk, id);
        for y in cy..ymax {
            for x in cx..xmax {
                let seed = y * view.width + x;
                if self.labels[seed] != 0 {
                    continue;
                }
                let region = self.free.pop().unwrap_or(self.regions.len());
                if region == self.regions.len() {
                    self.regions.push(None);
                }
                self.regions[region] = Some(Region::new(0.0, 0.0));
                self.chunks[id].push(region);
                self.labels[seed] = region;
                let mut queue = vec![seed];
                let mut sum_x = 0.0;
                let mut sum_y = 0.0;
                let mut head = 0;
                while head < queue.len() {
                    let c = queue[head];
                    head += 1;
                    let px = c % view.width;
                    let py = c / view.width;
                    sum_x += px as f64;
                    sum_y += py as f64;
                    for (dx, dy) in CARDINAL {
                        let Some(nx) = px.checked_add_signed(dx) else {
                            continue;
                        };
                        let Some(ny) = py.checked_add_signed(dy) else {
                            continue;
                        };
                        if nx < cx || nx >= xmax || ny < cy || ny >= ymax {
                            continue;
                        }
                        let n = ny * view.width + nx;
                        if self.labels[n] != 0 || (view.level(n) - view.level(c)).abs() > 1 {
                            continue;
                        }
                        self.labels[n] = region;
                        queue.push(n);
                    }
                }
                let r = self.region_mut(region);
                r.x = sum_x / queue.len() as f64;
                r.y = sum_y / queue.len() as f64;
            }
        }
    }
    fn add(&mut self, from: usize, to: usize) {
        let a = self.labels[from];
        let b = self.labels[to];
        let region = self.region_mut(a);
        if let Some(edge) = region.edges.iter_mut().find(|e| e.region == b) {
            edge.crossings.push((from, to));
        } else {
            region.edges.push(Edge {
                region: b,
                crossings: vec![(from, to)],
            });
            let incoming = &mut self.region_mut(b).incoming;
            if !incoming.contains(&a) {
                incoming.push(a);
            }
        }
    }
    fn connect(
        &mut self,
        view: TerrainView<'_>,
        x: usize,
        y: usize,
        directions: &[(isize, isize)],
        dirty: Option<(&[usize], usize)>,
    ) {
        let c = y * view.width + x;
        if self.labels[c] == BLOCKED {
            return;
        }
        for &(dx, dy) in directions {
            let Some(nx) = x.checked_add_signed(dx) else {
                continue;
            };
            let Some(ny) = y.checked_add_signed(dy) else {
                continue;
            };
            if nx >= view.width || ny >= view.height {
                continue;
            }
            let n = ny * view.width + nx;
            if let Some((dirty, chunk)) = dirty {
                let id = (ny / chunk) * view.width.div_ceil(chunk) + nx / chunk;
                if n < c && dirty.contains(&id) {
                    continue;
                }
            }
            if self.labels[n] == BLOCKED
                || self.labels[n] == self.labels[c]
                || (view.level(n) - view.level(c)).abs() > 1
            {
                continue;
            }
            if dx != 0 && dy != 0 {
                let a = y * view.width + nx;
                let b = ny * view.width + x;
                if self.labels[a] == BLOCKED || self.labels[b] == BLOCKED {
                    continue;
                }
                if (view.level(a) - view.level(c)).abs() <= 1
                    && (view.level(b) - view.level(c)).abs() <= 1
                {
                    self.add(c, n);
                }
                if (view.level(a) - view.level(n)).abs() <= 1
                    && (view.level(b) - view.level(n)).abs() <= 1
                {
                    self.add(n, c);
                }
            } else {
                self.add(c, n);
                self.add(n, c);
            }
        }
    }
    // Adjacent chunks also become dirty: a corner can change a diagonal whose
    // endpoints both lie outside the edited cell's chunk.
    fn update(&mut self, view: TerrainView<'_>, changed: &[usize], block: u16, chunk: usize) {
        let cw = view.width.div_ceil(chunk);
        let ch = view.height.div_ceil(chunk);
        let mut dirty = Vec::new();
        for &c in changed {
            if c >= self.labels.len() {
                continue;
            }
            let x = c % view.width / chunk;
            let y = c / view.width / chunk;
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let Some(nx) = x.checked_add_signed(dx) else {
                        continue;
                    };
                    let Some(ny) = y.checked_add_signed(dy) else {
                        continue;
                    };
                    if nx < cw && ny < ch {
                        let id = ny * cw + nx;
                        if !dirty.contains(&id) {
                            dirty.push(id);
                        }
                    }
                }
            }
        }
        for &id in &dirty {
            for region_id in self.chunks[id].clone() {
                let region = self.regions[region_id].take().expect("live dirty region");
                for from in region.incoming {
                    if let Some(r) = &mut self.regions[from] {
                        r.edges.retain(|e| e.region != region_id);
                    }
                }
                for to in region.edges {
                    if let Some(r) = &mut self.regions[to.region] {
                        r.incoming.retain(|&i| i != region_id);
                    }
                }
                self.free.push(region_id);
            }
        }
        for &id in &dirty {
            let (cx, cy, xmax, ymax) = Self::bounds(view, chunk, id);
            self.chunks[id].clear();
            for y in cy..ymax {
                for x in cx..xmax {
                    let c = y * view.width + x;
                    self.labels[c] = if view.flags[c] & block != 0 {
                        BLOCKED
                    } else {
                        0
                    };
                }
            }
            self.fill_chunk(view, chunk, id);
        }
        for &id in &dirty {
            let (cx, cy, xmax, ymax) = Self::bounds(view, chunk, id);
            for y in cy..ymax {
                for x in cx..xmax {
                    self.connect(view, x, y, &ALL, Some((&dirty, chunk)));
                }
            }
        }
        self.routes.clear();
    }
    fn metric(&self, a: usize, b: usize) -> f64 {
        math::hypot(
            self.region(a).x - self.region(b).x,
            self.region(a).y - self.region(b).y,
        )
    }
    fn route(&mut self, start: usize, goal: usize) -> Option<Vec<usize>> {
        if let Some(cached) = self
            .routes
            .iter()
            .find(|cached| cached.start == start && cached.goal == goal)
        {
            return cached.route.clone();
        }
        let mut came = vec![0; self.regions.len()];
        let mut costs = vec![f64::INFINITY; self.regions.len()];
        let mut heap = Heap(Vec::new());
        came[start] = start;
        costs[start] = 0.0;
        heap.push(Entry {
            region: start,
            cost: 0.0,
            score: self.metric(start, goal),
        });
        let mut reached = false;
        while let Some(entry) = heap.pop() {
            let r = entry.region;
            if entry.cost != costs[r] {
                continue;
            }
            if r == goal {
                reached = true;
                break;
            }
            for edge in &self.region(r).edges {
                let n = edge.region;
                let cost = entry.cost + self.metric(r, n).max(1.0);
                if cost >= costs[n] {
                    continue;
                }
                costs[n] = cost;
                came[n] = r;
                heap.push(Entry {
                    region: n,
                    cost,
                    score: cost + self.metric(n, goal),
                });
            }
        }
        let route = if reached {
            let mut route = vec![goal];
            let mut r = goal;
            while r != start {
                r = came[r];
                route.push(r);
            }
            route.reverse();
            Some(route)
        } else {
            None
        };
        if self.routes.len() >= 128 {
            self.routes.remove(0);
        }
        self.routes.push(CachedRoute {
            start,
            goal,
            route: route.clone(),
        });
        route
    }
    fn local_path(
        &mut self,
        view: TerrainView<'_>,
        from: usize,
        goal: usize,
        next_region: Option<usize>,
    ) -> Option<(Vec<usize>, Option<usize>)> {
        let distance = |a: usize, b: usize| {
            math::hypot(
                (a % view.width) as f64 - (b % view.width) as f64,
                (a / view.width) as f64 - (b / view.width) as f64,
            )
        };
        let mut candidates: BTreeMap<usize, (Option<usize>, f64)> = BTreeMap::new();
        if let Some(next_region) = next_region {
            let edge = self
                .region(self.labels[from])
                .edges
                .iter()
                .find(|edge| edge.region == next_region)
                .expect("route edge");
            for &(cell, next) in &edge.crossings {
                let cost = distance(next, goal);
                if candidates.get(&cell).is_none_or(|&(_, old)| cost < old) {
                    candidates.insert(cell, (Some(next), cost));
                }
            }
        } else {
            candidates.insert(goal, (None, 0.0));
        }
        self.generation = self.generation.wrapping_add(1);
        if self.generation == 0 {
            self.seen.fill(0);
            self.generation = 1;
        }
        self.queue.clear();
        self.queue.push((from, 0));
        self.seen[from] = self.generation;
        let mut head = 0;
        let mut target = None;
        let mut crossing = None;
        let mut best = f64::INFINITY;
        while head < self.queue.len() {
            let (c, depth) = self.queue[head];
            head += 1;
            if depth as f64 > best {
                break;
            }
            if let Some(&(next, cost)) = candidates.get(&c)
                && (depth as f64 + cost) < best
            {
                target = Some(c);
                best = depth as f64 + cost;
                crossing = next;
            }
            let x = c % view.width;
            let y = c / view.width;
            for (dx, dy) in CARDINAL {
                let Some(nx) = x.checked_add_signed(dx) else {
                    continue;
                };
                let Some(ny) = y.checked_add_signed(dy) else {
                    continue;
                };
                if nx >= view.width || ny >= view.height {
                    continue;
                }
                let n = ny * view.width + nx;
                if self.labels[n] != self.labels[from]
                    || self.seen[n] == self.generation
                    || (view.level(n) - view.level(c)).abs() > 1
                {
                    continue;
                }
                self.seen[n] = self.generation;
                self.came[n] = c;
                self.queue.push((n, depth + 1));
            }
        }
        let mut c = target?;
        let mut path = Vec::new();
        while c != from {
            path.push(c);
            c = self.came[c];
        }
        path.reverse();
        Some((path, crossing))
    }
    fn plan(
        &mut self,
        view: TerrainView<'_>,
        start_cell: usize,
        goal_cell: usize,
        limit: usize,
    ) -> (usize, bool, bool, Vec<usize>, bool) {
        let start = self.labels[start_cell];
        let goal = self.labels[goal_cell];
        let mut allowed = Vec::new();
        if start == BLOCKED && goal != BLOCKED {
            return (goal_cell, true, false, allowed, true);
        }
        if start == BLOCKED || goal == BLOCKED {
            return (goal_cell, false, false, allowed, false);
        }
        let Some(route) = self.route(start, goal) else {
            return (goal_cell, false, false, allowed, false);
        };
        allowed.push(start);
        if !view.world_known && view.width <= 128 && view.height <= 128 {
            let mut i = 0;
            while i < allowed.len() {
                for edge in &self.region(allowed[i]).edges {
                    if !allowed.contains(&edge.region) {
                        allowed.push(edge.region);
                    }
                }
                i += 1;
            }
            return (goal_cell, true, false, allowed, false);
        }
        let mut at = start_cell;
        let mut steps = 0;
        for (i, _) in route.iter().enumerate() {
            let next_region = route.get(i + 1).copied();
            let Some((path, crossing)) = self.local_path(view, at, goal_cell, next_region) else {
                return (goal_cell, true, false, allowed, true);
            };
            for cell in path {
                at = cell;
                steps += 1;
                if steps >= limit {
                    return (at, true, at != goal_cell, allowed, false);
                }
            }
            if let Some(cell) = crossing {
                at = cell;
                allowed.push(route[i + 1]);
                steps += 1;
                if steps >= limit {
                    return (at, true, at != goal_cell, allowed, false);
                }
            }
        }
        (goal_cell, true, false, allowed, false)
    }
}

#[derive(Clone, Copy, Debug)]
struct Entry {
    region: usize,
    cost: f64,
    score: f64,
}
impl Entry {
    fn before(self, other: Self) -> bool {
        self.score < other.score || self.score == other.score && self.region < other.region
    }
}
#[derive(Clone, Debug)]
struct Heap(Vec<Entry>);
impl Heap {
    fn push(&mut self, entry: Entry) {
        let mut i = self.0.len();
        self.0.push(entry);
        while i > 0 {
            let p = (i - 1) / 2;
            if !entry.before(self.0[p]) {
                break;
            }
            self.0[i] = self.0[p];
            i = p;
        }
        self.0[i] = entry;
    }
    fn pop(&mut self) -> Option<Entry> {
        let first = *self.0.first()?;
        let last = self.0.pop()?;
        if !self.0.is_empty() {
            let mut i = 0;
            loop {
                let left = i * 2 + 1;
                let right = left + 1;
                if left >= self.0.len() {
                    break;
                }
                let child = if right < self.0.len() && self.0[right].before(self.0[left]) {
                    right
                } else {
                    left
                };
                if !self.0[child].before(last) {
                    break;
                }
                self.0[i] = self.0[child];
                i = child;
            }
            self.0[i] = last;
        }
        Some(first)
    }
}
