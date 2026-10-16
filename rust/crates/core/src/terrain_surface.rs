//! Pure cell-centre presentation samples. These do not replace simulation heights or build meshes.
use crate::map::TerrainState;

pub const WATER_LIFT: f64 = 0.06;
pub const TRENCH_DEPTH: f64 = 0.9;

/// Original nominal heights, retained while the current terrain changes.
#[derive(Clone, Debug)]
pub struct FrozenSurface {
    w: usize,
    h: usize,
    levels: Vec<i8>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SurfaceCentres {
    pub height: Vec<f32>,
    /// Bank cells receive the lowest water line among their eight neighbours.
    pub water_line: Vec<Option<f64>>,
    /// Saturated channel rows: zero on land/bridges, one through three in water.
    pub channel_depth: Vec<u8>,
    /// Bit 0 is a vertical east edge, bit 1 a vertical south edge.
    pub cliffs: Vec<u8>,
}

fn valid(terrain: &TerrainState) -> bool {
    terrain.w > 0
        && terrain.h > 0
        && terrain.w.checked_mul(terrain.h) == Some(terrain.chars.len())
        && terrain.height.as_ref().is_none_or(|levels| {
            levels.len() == terrain.chars.len()
                && levels.iter().all(|level| (-2..=4).contains(level))
        })
}

impl FrozenSurface {
    pub fn new(terrain: &TerrainState) -> Result<Self, &'static str> {
        if !valid(terrain) {
            return Err("invalid surface grid");
        }
        Ok(Self {
            w: terrain.w,
            h: terrain.h,
            levels: terrain
                .height
                .clone()
                .unwrap_or_else(|| vec![0; terrain.chars.len()]),
        })
    }

    /// Sample cell centres only, with the same caller-selected road mask as createRelief.
    /// Cubic ramps, ragged cliff rims, triangle picking and mesh construction remain client work.
    pub fn sample_centres(
        &self,
        terrain: &TerrainState,
        roads: &[u8],
    ) -> Result<SurfaceCentres, &'static str> {
        if !valid(terrain)
            || (terrain.w, terrain.h) != (self.w, self.h)
            || roads.len() != self.levels.len()
            || roads.iter().any(|&road| road > 1)
        {
            return Err("invalid surface grid or road mask");
        }
        let n = self.levels.len();
        let wet: Vec<_> = terrain.chars.iter().map(|ch| b"WF=".contains(ch)).collect();
        let bridge: Vec<_> = terrain.chars.iter().map(|&ch| ch == b'=').collect();
        let mut component = vec![usize::MAX; n];
        let mut water = Vec::new();
        let mut queue = Vec::new();
        for c in 0..n {
            if !wet[c] || component[c] != usize::MAX {
                continue;
            }
            let id = water.len();
            queue.clear();
            queue.push(c);
            component[c] = id;
            let mut head = 0;
            let mut any = i8::MAX;
            let mut open = i8::MAX;
            while head < queue.len() {
                let k = queue[head];
                head += 1;
                any = any.min(self.levels[k]);
                if !bridge[k] {
                    open = open.min(self.levels[k]);
                }
                self.neighbours(k, |next| {
                    if wet[next] && component[next] == usize::MAX {
                        component[next] = id;
                        queue.push(next);
                    }
                });
            }
            water.push(f64::from(if open == i8::MAX { any } else { open }) * 2.5);
        }
        let mut water_line = vec![None; n];
        for c in 0..n {
            if wet[c] {
                water_line[c] = Some(water[component[c]] + WATER_LIFT);
                continue;
            }
            let x = c % self.w;
            let y = c / self.w;
            let mut lowest: Option<f64> = None;
            for ny in y.saturating_sub(1)..=(y + 1).min(self.h - 1) {
                for nx in x.saturating_sub(1)..=(x + 1).min(self.w - 1) {
                    let k = ny * self.w + nx;
                    if wet[k] {
                        lowest = Some(
                            lowest.map_or(water[component[k]], |v| v.min(water[component[k]])),
                        );
                    }
                }
            }
            water_line[c] = lowest.map(|v| v + WATER_LIFT);
        }
        let mut channel_depth = vec![3; n];
        for c in 0..n {
            if !wet[c] || bridge[c] {
                channel_depth[c] = 0;
                continue;
            }
            self.neighbours(c, |next| {
                if !wet[next] {
                    channel_depth[c] = 1;
                }
            });
        }
        for c in 0..n {
            if channel_depth[c] > 1 {
                let mut shallow = false;
                self.neighbours(c, |next| {
                    shallow |= channel_depth[next] <= 1;
                });
                if shallow {
                    channel_depth[c] = 2;
                }
            }
        }
        let level = |c| f64::from(terrain.height.as_ref().map_or(0, |levels| levels[c]));
        let mut height = Vec::with_capacity(n);
        let mut cliffs = vec![0; n];
        for c in 0..n {
            let mut value = level(c) * 2.5 - 0.1 * f64::from(roads[c]);
            if wet[c] && !bridge[c] {
                let depth = if terrain.chars[c] == b'F' {
                    0.15
                } else {
                    0.42 + 0.38 * f64::from(channel_depth[c] - 1)
                };
                value = value.min(water_line[c].unwrap() - depth);
            }
            if terrain.chars[c] == b'T' {
                value = f64::from(value as f32) - TRENCH_DEPTH;
            }
            height.push(value as f32);
            if c % self.w + 1 < self.w && (level(c) - level(c + 1)).abs() >= 2.0 {
                cliffs[c] |= 1;
            }
            if c / self.w + 1 < self.h && (level(c) - level(c + self.w)).abs() >= 2.0 {
                cliffs[c] |= 2;
            }
        }
        Ok(SurfaceCentres {
            height,
            water_line,
            channel_depth,
            cliffs,
        })
    }

    fn neighbours(&self, c: usize, mut visit: impl FnMut(usize)) {
        if c % self.w > 0 {
            visit(c - 1);
        }
        if c % self.w + 1 < self.w {
            visit(c + 1);
        }
        if c >= self.w {
            visit(c - self.w);
        }
        if c + self.w < self.w * self.h {
            visit(c + self.w);
        }
    }
}

#[cfg(test)]
mod tests;
