use crate::math::{atan2, cos, hypot, max, min, round, sin};

use super::{Grid, MinHeap, Region, RegionKind};

const STEP: usize = 2;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RailStation {
    pub region: usize,
    pub x: usize,
    pub y: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RailLine {
    pub a: usize,
    pub b: usize,
    pub cells: Vec<usize>,
    pub regions: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RailNetwork {
    pub stations: Vec<RailStation>,
    pub lines: Vec<RailLine>,
}

fn clear(grid: &Grid, x: usize, y: usize) -> bool {
    let height = grid.heights[y * grid.width + x];
    for dy in -2isize..=2 {
        for dx in -2isize..=2 {
            let xx = x as isize + dx;
            let yy = y as isize + dy;
            if xx < 0 || yy < 0 || xx >= grid.width as isize || yy >= grid.height as isize {
                return false;
            }
            let cell = yy as usize * grid.width + xx as usize;
            if !matches!(grid.chars[cell], b'.' | b'O' | b'D') || grid.heights[cell] != height {
                return false;
            }
        }
    }
    true
}

fn squared_distance(a: &RailStation, b: &RailStation) -> f64 {
    let dx = a.x as f64 - b.x as f64;
    let dy = a.y as f64 - b.y as f64;
    dx * dx + dy * dy
}

fn route(
    grid: &Grid,
    cost: &mut [f32],
    cw: usize,
    ch: usize,
    a: &RailStation,
    b: &RailStation,
) -> Option<Vec<usize>> {
    let start = (a.y / STEP) * cw + a.x / STEP;
    let end = (b.y / STEP) * cw + b.x / STEP;
    let mut scores = vec![f64::INFINITY; cw * ch];
    let mut previous = vec![None; cw * ch];
    let mut done = vec![false; cw * ch];
    let mut heap = MinHeap::new();
    scores[start] = 0.0;
    heap.push(0.0, start);
    let ex = (end % cw) as isize;
    let ey = (end / cw) as isize;
    while let Some((_, cell)) = heap.pop() {
        if done[cell] {
            continue;
        }
        done[cell] = true;
        if cell == end {
            break;
        }
        let x = (cell % cw) as isize;
        let y = (cell / cw) as isize;
        for dy in -1isize..=1 {
            for dx in -1isize..=1 {
                let nx = x + dx;
                let ny = y + dy;
                if (dx == 0 && dy == 0)
                    || nx < 0
                    || ny < 0
                    || nx >= cw as isize
                    || ny >= ch as isize
                {
                    continue;
                }
                let next = ny as usize * cw + nx as usize;
                if !cost[next].is_finite()
                    || (dx != 0
                        && dy != 0
                        && (!cost[y as usize * cw + nx as usize].is_finite()
                            || !cost[ny as usize * cw + x as usize].is_finite()))
                {
                    continue;
                }
                let score =
                    scores[cell] + max(0.3, f64::from(cost[next])) * hypot(dx as f64, dy as f64);
                if score < scores[next] {
                    scores[next] = score;
                    previous[next] = Some(cell);
                    heap.push(
                        score + 0.9 * hypot((nx - ex) as f64, (ny - ey) as f64),
                        next,
                    );
                }
            }
        }
    }
    if !scores[end].is_finite() {
        return None;
    }
    let mut nodes = Vec::new();
    let mut current = Some(end);
    while let Some(cell) = current {
        nodes.push(cell);
        if cell == start {
            break;
        }
        current = previous[cell];
    }
    nodes.reverse();
    for &cell in &nodes {
        cost[cell] = min(f64::from(cost[cell]), 0.6) as f32;
    }
    let mut points = vec![(a.x, a.y)];
    if nodes.len() > 2 {
        points.extend(
            nodes[1..nodes.len() - 1]
                .iter()
                .map(|&cell| ((cell % cw) * STEP, (cell / cw) * STEP)),
        );
    }
    points.push((b.x, b.y));
    let mut cells = Vec::new();
    for (index, pair) in points.windows(2).enumerate() {
        let (px, py) = pair[0];
        let (qx, qy) = pair[1];
        let dx = qx as isize - px as isize;
        let dy = qy as isize - py as isize;
        let steps = dx.abs().max(dy.abs()).max(1);
        for s in if index == 0 { 0 } else { 1 }..=steps {
            let x = round(px as f64 + dx as f64 * s as f64 / steps as f64) as usize;
            let y = round(py as f64 + dy as f64 * s as f64 / steps as f64) as usize;
            let cell = y * grid.width + x;
            if cells.last() != Some(&cell) {
                cells.push(cell);
            }
        }
    }
    Some(cells)
}

struct Builder<'a> {
    grid: &'a Grid,
    region_map: &'a [i16],
    cw: usize,
    ch: usize,
    cost: Vec<f32>,
    stations: Vec<RailStation>,
    lines: Vec<RailLine>,
    edges: Vec<(usize, usize)>,
}

impl Builder<'_> {
    fn link(&mut self, i: usize, j: usize) {
        let edge = (i.min(j), i.max(j));
        if self.edges.contains(&edge) {
            return;
        }
        // A failed route is still a visited edge in the source network.
        self.edges.push(edge);
        if let Some(cells) = route(
            self.grid,
            &mut self.cost,
            self.cw,
            self.ch,
            &self.stations[i],
            &self.stations[j],
        ) {
            let mut regions = Vec::new();
            for &cell in &cells {
                let id = self.region_map[cell];
                if id >= 0 && !regions.contains(&(id as usize)) {
                    regions.push(id as usize);
                }
            }
            self.lines.push(RailLine {
                a: self.stations[i].region,
                b: self.stations[j].region,
                cells,
                regions,
            });
        }
    }
}

pub(super) fn generate(grid: &Grid, regions: &[Region], region_map: &[i16]) -> RailNetwork {
    let cw = grid.width / STEP;
    let ch = grid.height / STEP;
    let mut cost = vec![0.0f32; cw * ch];
    for y in 0..ch {
        for x in 0..cw {
            let gx = x * STEP;
            let gy = y * STEP;
            let cell = gy * grid.width + gx;
            let terrain = grid.chars[cell];
            if matches!(terrain, b'W' | b'F' | b'B') {
                cost[y * cw + x] = f32::INFINITY;
                continue;
            }
            let mut lo = 9;
            let mut hi = 0;
            for dy in -2isize..=2 {
                for dx in -2isize..=2 {
                    let yy = (gy as isize + dy).clamp(0, grid.height as isize - 1) as usize;
                    let xx = (gx as isize + dx).clamp(0, grid.width as isize - 1) as usize;
                    let level = grid.heights[yy * grid.width + xx];
                    lo = lo.min(level);
                    hi = hi.max(level);
                }
            }
            cost[y * cw + x] = (1.0
                + f64::from(hi - lo) * 12.0
                + f64::from(grid.heights[cell]) * 2.0
                + if terrain == b'O' { 1.5 } else { 0.0 }
                + if terrain == b'=' { -0.5 } else { 0.0 }) as f32;
        }
    }
    for region in regions.iter().filter(|region| region.home.is_some()) {
        for y in 0..ch {
            for x in 0..cw {
                if hypot(
                    (x * STEP) as f64 - region.x as f64,
                    (y * STEP) as f64 - region.y as f64,
                ) < 13.0
                {
                    let cell = y * cw + x;
                    cost[cell] = (f64::from(cost[cell]) + 50.0) as f32;
                }
            }
        }
    }
    let mut stations = Vec::new();
    for region in regions {
        if region.home.is_none()
            && !matches!(region.kind, RegionKind::City | RegionKind::Industrial)
        {
            continue;
        }
        let toward = atan2(
            grid.height as f64 / 2.0 - region.y as f64,
            grid.width as f64 / 2.0 - region.x as f64,
        );
        let first_radius = if region.home.is_none() { 9 } else { 16 };
        let mut best = None;
        'search: for radius in first_radius..=first_radius + 7 {
            for k in 0usize..24 {
                let sign = if k % 2 == 1 { 1.0 } else { -1.0 };
                let angle = toward + sign * k.div_ceil(2) as f64 * std::f64::consts::PI / 12.0;
                let x = round(region.x as f64 + cos(angle) * radius as f64) as isize;
                let y = round(region.y as f64 + sin(angle) * radius as f64) as isize;
                if x <= 4 || y <= 4 || x >= grid.width as isize - 5 || y >= grid.height as isize - 5
                {
                    continue;
                }
                let x = x as usize;
                let y = y as usize;
                let node = (y / STEP) * cw + x / STEP;
                if clear(grid, x, y)
                    && region_map[y * grid.width + x] == region.id as i16
                    && cost[node].is_finite()
                {
                    best = Some(RailStation {
                        region: region.id,
                        x,
                        y,
                    });
                    break 'search;
                }
            }
        }
        if let Some(station) = best {
            stations.push(station);
        }
    }
    let mut builder = Builder {
        grid,
        region_map,
        cw,
        ch,
        cost,
        stations,
        lines: Vec::new(),
        edges: Vec::new(),
    };
    let count = builder.stations.len();
    let mut joined = if count == 0 { Vec::new() } else { vec![0] };
    let mut joined_mask = vec![false; count];
    if count != 0 {
        joined_mask[0] = true;
    }
    while joined.len() < count {
        let mut best = None;
        let mut best_distance = f64::INFINITY;
        for &i in &joined {
            for (j, &is_joined) in joined_mask.iter().enumerate() {
                if is_joined {
                    continue;
                }
                let distance = squared_distance(&builder.stations[i], &builder.stations[j]);
                if distance < best_distance {
                    best_distance = distance;
                    best = Some((i, j));
                }
            }
        }
        let (i, j) = best.expect("unjoined station exists");
        builder.link(i, j);
        joined.push(j);
        joined_mask[j] = true;
    }
    for i in 0..count {
        let mut near: Vec<_> = builder
            .stations
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != i)
            .map(|(j, station)| (j, squared_distance(station, &builder.stations[i])))
            .collect();
        near.sort_by(|a, b| a.1.total_cmp(&b.1));
        for &(j, distance) in near.iter().take(2) {
            if distance < 110.0 * 110.0 {
                builder.link(i, j);
            }
        }
    }
    RailNetwork {
        stations: builder.stations,
        lines: builder.lines,
    }
}
