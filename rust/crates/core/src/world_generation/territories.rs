use super::{Grid, MinHeap, Region};

const CARDINAL: [(isize, isize); 4] = [(-1, 0), (1, 0), (0, -1), (0, 1)];

pub(super) fn generate(grid: &Grid, regions: &mut [Region]) -> Vec<i16> {
    let cells = grid.width * grid.height;
    let mut owner = vec![-1i16; cells];
    let mut distances = vec![f64::INFINITY; cells];
    let mut heap = MinHeap::new();
    for region in regions.iter() {
        for dy in -9isize..=9 {
            for dx in -9isize..=9 {
                let x = region.x as isize + dx;
                let y = region.y as isize + dy;
                if x < 0 || y < 0 || x >= grid.width as isize || y >= grid.height as isize {
                    continue;
                }
                let cell = y as usize * grid.width + x as usize;
                if grid.chars[cell] == b'W' {
                    continue;
                }
                distances[cell] = 0.0;
                owner[cell] = region.id as i16;
                heap.push(0.0, (cell, region.id as i16));
            }
        }
    }
    while let Some((distance, (cell, id))) = heap.pop() {
        if distance != distances[cell] || owner[cell] != id {
            continue;
        }
        let x = (cell % grid.width) as isize;
        let y = (cell / grid.width) as isize;
        for (dx, dy) in CARDINAL {
            let nx = x + dx;
            let ny = y + dy;
            if nx < 0 || ny < 0 || nx >= grid.width as isize || ny >= grid.height as isize {
                continue;
            }
            let next = ny as usize * grid.width + nx as usize;
            if grid.chars[next] == b'W' {
                continue;
            }
            let elevation = (grid.heights[next] as i16 - grid.heights[cell] as i16).abs();
            let crossing_cost = if matches!(grid.chars[next], b'F' | b'=') {
                4.0
            } else {
                0.0
            };
            let candidate = distance + 1.0 + elevation as f64 * 8.0 + crossing_cost;
            if candidate < distances[next] {
                distances[next] = candidate;
                owner[next] = id;
                heap.push(candidate, (next, id));
            }
        }
    }
    for region in regions.iter_mut() {
        region.bounds = [grid.width, grid.height, 0, 0];
    }
    for (cell, &id) in owner.iter().enumerate() {
        if id < 0 {
            continue;
        }
        let bounds = &mut regions[id as usize].bounds;
        let x = cell % grid.width;
        let y = cell / grid.width;
        bounds[0] = bounds[0].min(x);
        bounds[1] = bounds[1].min(y);
        bounds[2] = bounds[2].max(x + 1);
        bounds[3] = bounds[3].max(y + 1);
    }
    owner
}
