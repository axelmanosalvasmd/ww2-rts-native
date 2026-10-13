use crate::math::{hypot, round};

use super::{CellPoint, GenerationError, Grid, MinHeap, Region, Waterways};

const STEP: usize = 4;
const CARDINAL: [(isize, isize); 4] = [(-1, 0), (1, 0), (0, -1), (0, 1)];
const NEIGHBORS: [(isize, isize); 8] = [
    (-1, 0),
    (1, 0),
    (0, -1),
    (0, 1),
    (-1, -1),
    (1, -1),
    (-1, 1),
    (1, 1),
];

#[derive(Clone, Copy)]
struct Point {
    x: isize,
    y: isize,
}

impl From<CellPoint> for Point {
    fn from(value: CellPoint) -> Self {
        Self {
            x: value.x as isize,
            y: value.y as isize,
        }
    }
}

impl From<&Region> for Point {
    fn from(value: &Region) -> Self {
        Self {
            x: value.x as isize,
            y: value.y as isize,
        }
    }
}

fn distance(a: Point, b: Point) -> f64 {
    hypot((a.x - b.x) as f64, (a.y - b.y) as f64)
}

struct Router<'a> {
    grid: &'a mut Grid,
    cw: usize,
    ch: usize,
    cost: Vec<f32>,
    road: Vec<u8>,
    heap: MinHeap<usize>,
}

impl Router<'_> {
    fn nearest(&self, point: Point) -> Result<usize, GenerationError> {
        let cx = point.x.div_euclid(STEP as isize);
        let cy = point.y.div_euclid(STEP as isize);
        let mut best = None;
        let mut distance = f64::INFINITY;
        for y in (cy - 6).max(0)..(cy + 7).min(self.ch as isize) {
            for x in (cx - 6).max(0)..(cx + 7).min(self.cw as isize) {
                let cell = y as usize * self.cw + x as usize;
                let dx = (x * STEP as isize - point.x) as f64;
                let dy = (y * STEP as isize - point.y) as f64;
                let candidate = dx * dx + dy * dy;
                if self.cost[cell].is_finite() && candidate < distance {
                    best = Some(cell);
                    distance = candidate;
                }
            }
        }
        best.ok_or(GenerationError::NoRoadAccess)
    }

    fn paint(&mut self, x: f64, y: f64) {
        let x = round(x) as isize;
        let y = round(y) as isize;
        for dy in -3isize..=3 {
            for dx in -3isize..=3 {
                let xx = x + dx;
                let yy = y + dy;
                if xx < 0
                    || yy < 0
                    || xx >= self.grid.width as isize
                    || yy >= self.grid.height as isize
                {
                    continue;
                }
                let cell = yy as usize * self.grid.width + xx as usize;
                let terrain = self.grid.chars[cell];
                if terrain == b'W' {
                    continue;
                }
                self.road[cell] = 1;
                if !matches!(terrain, b'F' | b'=') {
                    self.grid.chars[cell] = if (dx.abs() <= 1 && dy.abs() <= 1) || terrain == b'D' {
                        b'D'
                    } else {
                        b'.'
                    };
                }
                self.grid.heights[cell] = 0;
            }
        }
    }

    fn segment(&mut self, a: Point, b: Point) {
        let steps = (a.x - b.x).abs().max((a.y - b.y).abs()).max(1);
        for i in 0..=steps {
            self.paint(
                a.x as f64 + ((b.x - a.x) as f64 * i as f64) / steps as f64,
                a.y as f64 + ((b.y - a.y) as f64 * i as f64) / steps as f64,
            );
        }
    }

    fn route(&mut self, a: Point, b: Point) -> Result<(), GenerationError> {
        let start = self.nearest(a)?;
        let end = self.nearest(b)?;
        let cells = self.cw * self.ch;
        let mut scores = vec![f64::INFINITY; cells];
        let mut previous = vec![None; cells];
        let mut done = vec![false; cells];
        self.heap.clear();
        scores[start] = 0.0;
        self.heap.push(0.0, start);
        while let Some((_, cell)) = self.heap.pop() {
            if done[cell] {
                continue;
            }
            done[cell] = true;
            if cell == end {
                break;
            }
            let x = (cell % self.cw) as isize;
            let y = (cell / self.cw) as isize;
            for (dx, dy) in NEIGHBORS {
                let nx = x + dx;
                let ny = y + dy;
                if nx < 0 || ny < 0 || nx >= self.cw as isize || ny >= self.ch as isize {
                    continue;
                }
                let next = ny as usize * self.cw + nx as usize;
                if dx != 0
                    && dy != 0
                    && (!self.cost[y as usize * self.cw + nx as usize].is_finite()
                        || !self.cost[ny as usize * self.cw + x as usize].is_finite())
                {
                    continue;
                }
                let score = scores[cell] + self.cost[next] as f64 * hypot(dx as f64, dy as f64);
                if score < scores[next] {
                    scores[next] = score;
                    previous[next] = Some(cell);
                    self.heap.push(
                        score
                            + 0.8
                                * hypot(
                                    (nx - (end % self.cw) as isize) as f64,
                                    (ny - (end / self.cw) as isize) as f64,
                                ),
                        next,
                    );
                }
            }
        }
        if !scores[end].is_finite() {
            return Err(GenerationError::DisconnectedRoadGraph);
        }
        let mut last = b;
        let mut current = Some(end);
        while let Some(cell) = current {
            let point = Point {
                x: ((cell % self.cw) * STEP) as isize,
                y: ((cell / self.cw) * STEP) as isize,
            };
            self.segment(last, point);
            self.cost[cell] = 0.8;
            last = point;
            if cell == start {
                break;
            }
            current = previous[cell];
        }
        self.segment(last, a);
        Ok(())
    }

    fn connect(
        &mut self,
        sites: &[Region],
        edges: &mut Vec<(usize, usize)>,
        i: usize,
        j: usize,
    ) -> Result<(), GenerationError> {
        let edge = (i.min(j), i.max(j));
        if !edges.contains(&edge) {
            self.route(Point::from(&sites[i]), Point::from(&sites[j]))?;
            edges.push(edge);
        }
        Ok(())
    }

    fn terrace(&mut self) {
        let mut distances = vec![255u8; self.road.len()];
        let mut queue = Vec::with_capacity(self.road.len());
        for (cell, &road) in self.road.iter().enumerate() {
            if road != 0 {
                distances[cell] = 0;
                queue.push(cell);
            }
        }
        let mut index = 0;
        while index < queue.len() {
            let cell = queue[index];
            index += 1;
            if distances[cell] >= 12 {
                continue;
            }
            let x = (cell % self.grid.width) as isize;
            let y = (cell / self.grid.width) as isize;
            for (dx, dy) in CARDINAL {
                let nx = x + dx;
                let ny = y + dy;
                if nx < 0
                    || ny < 0
                    || nx >= self.grid.width as isize
                    || ny >= self.grid.height as isize
                {
                    continue;
                }
                let next = ny as usize * self.grid.width + nx as usize;
                if distances[next] <= distances[cell] + 1 {
                    continue;
                }
                distances[next] = distances[cell] + 1;
                queue.push(next);
            }
        }
        for (height, distance) in self.grid.heights.iter_mut().zip(distances) {
            *height = (*height).min((distance / 3) as i8);
        }
    }
}

pub(super) fn generate(
    grid: &mut Grid,
    sites: &[Region],
    features: &Waterways,
) -> Result<Vec<u8>, GenerationError> {
    if sites.is_empty() {
        return Err(GenerationError::NoRoadAccess);
    }
    let cw = grid.width / STEP;
    let ch = grid.height / STEP;
    let mut cost = vec![0.0f32; cw * ch];
    for y in 0..ch {
        for x in 0..cw {
            let mut blocked = false;
            let mut level = 0;
            for dy in -2isize..=2 {
                for dx in -2isize..=2 {
                    let xx = (x as isize * STEP as isize + dx).clamp(0, grid.width as isize - 1)
                        as usize;
                    let yy = (y as isize * STEP as isize + dy).clamp(0, grid.height as isize - 1)
                        as usize;
                    let cell = yy * grid.width + xx;
                    blocked |= grid.chars[cell] == b'W';
                    level = level.max(grid.heights[cell]);
                }
            }
            cost[y * cw + x] = if blocked {
                f32::INFINITY
            } else {
                1.0 + level as f32 * 7.0
            };
        }
    }
    let road = vec![0; grid.width * grid.height];
    let mut router = Router {
        grid,
        cw,
        ch,
        cost,
        road,
        heap: MinHeap::new(),
    };
    // Insertion order resolves equally distant backbone candidates.
    let mut linked = vec![0];
    let mut linked_mask = vec![false; sites.len()];
    linked_mask[0] = true;
    let mut edges = Vec::new();
    while linked.len() < sites.len() {
        let mut best = None;
        let mut distance = f64::INFINITY;
        for &i in &linked {
            for j in 0..sites.len() {
                if linked_mask[j] {
                    continue;
                }
                let dx = sites[i].x as f64 - sites[j].x as f64;
                let dy = sites[i].y as f64 - sites[j].y as f64;
                let candidate = dx * dx + dy * dy;
                if candidate < distance {
                    distance = candidate;
                    best = Some((i, j));
                }
            }
        }
        let (i, j) = best.ok_or(GenerationError::DisconnectedRoadGraph)?;
        router.connect(sites, &mut edges, i, j)?;
        linked.push(j);
        linked_mask[j] = true;
    }
    for i in 0..sites.len() {
        let mut near: Vec<_> = sites
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != i)
            .map(|(j, site)| (j, distance(Point::from(site), Point::from(&sites[i]))))
            .collect();
        near.sort_by(|a, b| a.1.total_cmp(&b.1));
        for &(j, _) in near.iter().take(2) {
            router.connect(sites, &mut edges, i, j)?;
        }
    }
    let crossing_points = features.crossings.iter().flat_map(|crossing| {
        [
            Point {
                x: crossing.x as isize - 18,
                y: crossing.y as isize,
            },
            Point {
                x: crossing.x as isize + 18,
                y: crossing.y as isize,
            },
        ]
    });
    for point in crossing_points.chain(features.passes.iter().copied().map(Point::from)) {
        // The source reducer chooses the later site on equal distances.
        let site = sites.iter().reduce(|a, b| {
            if distance(Point::from(a), point) < distance(Point::from(b), point) {
                a
            } else {
                b
            }
        });
        router.route(
            point,
            Point::from(site.ok_or(GenerationError::NoRoadAccess)?),
        )?;
    }
    router.terrace();
    Ok(router.road)
}
