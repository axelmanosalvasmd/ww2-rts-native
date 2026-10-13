use super::{CellPoint, Crossing, GenerationError, Grid, Random, River, RiverPoint, Waterways};
use crate::math;

pub(super) fn relief(w: usize, h: usize, seed: u32) -> Vec<i8> {
    let phase = f64::from(seed) / 4_294_967_296.0 * std::f64::consts::PI * 2.0;
    let mut heights = vec![0; w * h];
    for y in 0..h {
        for x in 0..w {
            let ridge = h as f64 * (0.36 + 0.08 * math::sin(x as f64 / 93.0 + phase));
            let d = (y as f64 - ridge).abs();
            let pass = (x as f64 - w as f64 * 0.22)
                .abs()
                .min((x as f64 - w as f64 * 0.71).abs());
            let crest: f64 = if d < 7.0 {
                4.0
            } else if d < 17.0 {
                2.0
            } else if d < 30.0 {
                1.0
            } else {
                0.0
            };
            let hills =
                (math::sin(x as f64 / 47.0 + phase) * math::cos(y as f64 / 53.0 - phase)).max(0.0);
            heights[y * w + x] = (hills * 3.0)
                .floor()
                .max(crest.min(((pass - 10.0) / 5.0).floor().max(0.0)))
                .min(4.0) as i8;
        }
    }
    heights
}
fn valley_path(
    w: usize,
    heights: &[i8],
    targets: &[f64],
    start_y: usize,
    fixed: bool,
) -> Result<Vec<RiverPoint>, GenerationError> {
    let mut previous = vec![usize::MAX; w * targets.len()];
    let mut costs = vec![f64::INFINITY; w];
    for (i, &target) in targets.iter().enumerate() {
        let mut next = vec![f64::INFINITY; w];
        let radius = if fixed && (i == 0 || i == targets.len() - 1) {
            0
        } else {
            16
        };
        let rounded = math::round(target) as isize;
        let begin = (rounded - radius).max(4) as usize;
        let end = (rounded + radius).min(w as isize - 5) as usize;
        for x in begin..=end {
            let local =
                f64::from(heights[(start_y + i) * w + x]) * 0.8 + (x as f64 - target).abs() * 0.08;
            if i == 0 {
                next[x] = local;
                continue;
            }
            for dx in -1isize..=1 {
                let px = x
                    .checked_add_signed(dx)
                    .expect("channel envelope leaves edge cells");
                let candidate = costs[px] + local + dx.unsigned_abs() as f64 * 0.12;
                if candidate < next[x] {
                    next[x] = candidate;
                    previous[i * w + x] = px;
                }
            }
        }
        costs = next;
    }
    let mut x = 0;
    for k in 1..w {
        if costs[k] < costs[x] {
            x = k;
        }
    }
    if !costs[x].is_finite() {
        return Err(GenerationError::NoChannelRoute);
    }
    let mut path = vec![
        RiverPoint {
            x: 0,
            y: 0,
            radius: 0
        };
        targets.len()
    ];
    for i in (0..targets.len()).rev() {
        path[i] = RiverPoint {
            x,
            y: start_y + i,
            radius: 0,
        };
        x = previous[i * w + x];
    }
    Ok(path)
}
fn carve(g: &mut Grid, x: usize, y: usize, radius: usize, kind: u8) {
    let reach = (radius + 6) as isize;
    for dy in -reach..=reach {
        for dx in -reach..=reach {
            let Some(xx) = x.checked_add_signed(dx) else {
                continue;
            };
            let Some(yy) = y.checked_add_signed(dy) else {
                continue;
            };
            if xx >= g.width || yy >= g.height {
                continue;
            }
            let distance = math::hypot(dx as f64, dy as f64);
            if distance > (radius + 6) as f64 {
                continue;
            }
            let c = yy * g.width + xx;
            g.heights[c] =
                g.heights[c].min(((distance - radius as f64) / 3.0).floor().max(0.0) as i8);
            if distance <= radius as f64 && (kind == b'W' || g.chars[c] != b'W') {
                g.chars[c] = kind;
            }
        }
    }
}
pub(super) fn waterways(g: &mut Grid, seed: u32) -> Result<Waterways, GenerationError> {
    let w = g.width;
    let h = g.height;
    let mut random = Random(seed);
    let count = 1 + (random.next() * 3.0).floor() as usize;
    let phase = random.next() * std::f64::consts::PI * 2.0;
    let original = g.heights.clone();
    let mut rivers = Vec::new();
    let mut crossings = Vec::new();
    for id in 0..count {
        let center = w as f64 * (0.12 + 0.76 * (id as f64 + 0.5) / count as f64);
        let angle = phase + id as f64 * 2.1;
        let targets: Vec<_> = (0..h)
            .map(|y| {
                center
                    + math::sin(y as f64 / 77.0 + angle) * 18.0
                    + math::sin(y as f64 / 31.0 + angle) * 5.0
            })
            .collect();
        let mut path = valley_path(w, &original, &targets, 0, false)?;
        for p in &mut path {
            p.radius = 4;
        }
        rivers.push(River {
            id,
            parent: None,
            path,
        });
    }
    let branch_count = (random.next() * 4.0).floor() as usize;
    let first_side = if random.next() < 0.5 { -1.0 } else { 1.0 };
    for i in 0..branch_count {
        let parent = i % count;
        let ordinal = i / count;
        let join_y = (math::round(h as f64 * [0.4, 0.6, 0.79][ordinal]) as usize)
            + (random.next() * 21.0).floor() as usize
            - 10;
        let join_x = rivers[parent].path[join_y].x;
        let side = if i % 2 != 0 { -first_side } else { first_side };
        let reach = (w as f64 * 0.76 / count as f64 * 0.35).min(78.0);
        let length = 96 + (random.next() * 21.0).floor() as usize;
        let start_y = join_y - length;
        let start_x = math::round(join_x as f64 + side * reach);
        let targets: Vec<_> = (0..=length)
            .map(|j| {
                let t = j as f64 / length as f64;
                start_x
                    + (join_x as f64 - start_x) * t
                    + math::sin(t * std::f64::consts::PI * 2.0) * 6.0
            })
            .collect();
        let mut path = valley_path(w, &original, &targets, start_y, true)?;
        for p in &mut path {
            p.radius = 2;
            carve(g, p.x, p.y, p.radius, b'F');
        }
        let parent_id = rivers[parent].id;
        rivers.push(River {
            id: count + i,
            parent: Some(parent_id),
            path,
        });
        for p in &mut rivers[parent].path {
            if p.y >= join_y {
                p.radius = (p.radius + 1).min(6);
            }
        }
    }
    for river in rivers.iter().filter(|r| r.parent.is_none()) {
        for p in &river.path {
            carve(g, p.x, p.y, p.radius, b'W');
        }
    }
    let lake_y = math::round(h as f64 * 0.68) as usize;
    let lake_x = rivers[0].path[lake_y].x;
    for y in lake_y - 28..=lake_y + 28 {
        for x in lake_x - 32..=lake_x + 32 {
            let dx = (x as f64 - lake_x as f64) / 27.0;
            let dy = (y as f64 - lake_y as f64) / 24.0;
            if dx * dx + dy * dy < 1.0 + 0.1 * math::sin(math::atan2(dy, dx) * 5.0 + phase) {
                g.chars[y * w + x] = b'W';
                g.heights[y * w + x] = 0;
            }
        }
    }
    for river in rivers.iter().filter(|r| r.parent.is_none()) {
        for (i, f) in [0.12, 0.29, 0.51, 0.87].into_iter().enumerate() {
            let y = math::round(h as f64 * f) as usize + (random.next() * 9.0).floor() as usize - 4;
            let kind = if i % 2 != 0 { b'F' } else { b'=' };
            crossings.push(Crossing {
                x: river.path[y].x,
                y,
                kind,
                river: river.id,
            });
            for yy in y - 4..=y + 4 {
                let cx = river.path[yy].x;
                for x in cx - 12..=cx + 12 {
                    let c = yy * w + x;
                    g.chars[c] = match g.chars[c] {
                        b'W' => kind,
                        b'F' => b'F',
                        _ => b'D',
                    };
                    g.heights[c] = 0;
                }
            }
        }
    }
    let passes = [0.22, 0.71]
        .into_iter()
        .map(|f| CellPoint {
            x: math::round(w as f64 * f) as usize,
            y: math::round(
                h as f64
                    * (0.36
                        + 0.08
                            * math::sin(
                                w as f64 * f / 93.0
                                    + f64::from(seed) / 4_294_967_296.0
                                        * std::f64::consts::PI
                                        * 2.0,
                            )),
            ) as usize,
        })
        .collect();
    Ok(Waterways {
        rivers,
        crossings,
        lake: CellPoint {
            x: lake_x,
            y: lake_y,
        },
        passes,
    })
}
