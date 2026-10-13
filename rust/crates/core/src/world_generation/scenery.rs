use super::{Grid, Random, Region, RegionKind};
use crate::{map::Building, math};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Biome {
    Farmland,
    Pine,
    Marsh,
    Highland,
    Orchard,
    Steppe,
}
impl Biome {
    pub fn name(self) -> &'static str {
        match self {
            Self::Farmland => "farmland",
            Self::Pine => "pine",
            Self::Marsh => "marsh",
            Self::Highland => "highland",
            Self::Orchard => "orchard",
            Self::Steppe => "steppe",
        }
    }
}
const BIOMES: [Biome; 6] = [
    Biome::Farmland,
    Biome::Pine,
    Biome::Marsh,
    Biome::Highland,
    Biome::Orchard,
    Biome::Steppe,
];
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scenery {
    pub regions: Vec<Biome>,
    pub cells: Vec<u8>,
    pub buildings: Vec<Building>,
}
#[derive(Clone, Copy, Default)]
struct Stats {
    n: usize,
    height: usize,
    wet: usize,
    wood: usize,
}
fn mix(x: isize, y: isize, salt: u32) -> f64 {
    let mut n = (x as u32 ^ 0x9e3779b9).wrapping_mul(0x85ebca6b)
        ^ (y as u32 ^ salt).wrapping_mul(0xc2b2ae35);
    n ^= n >> 16;
    n = n.wrapping_mul(0x7feb352d);
    n ^= n >> 15;
    f64::from(n) / 4294967296.0
}
#[derive(Clone, Copy)]
struct Lot {
    x: isize,
    y: isize,
    w: isize,
    h: isize,
}
struct Painter<'a> {
    grid: &'a mut Grid,
    region_map: &'a [i16],
    roads: &'a [u8],
    town: Random,
    buildings: Vec<Building>,
}
impl Painter<'_> {
    fn lot(&self, r: &Region, l: Lot) -> bool {
        let (x0, y0, bw, bh) = (l.x, l.y, l.w, l.h);
        let (w, h) = (self.grid.width as isize, self.grid.height as isize);
        if x0 < 3 || y0 < 3 || x0 + bw > w - 3 || y0 + bh > h - 3 {
            return false;
        }
        let (rx, ry) = (r.x as isize, r.y as isize);
        if x0 <= rx + 9 && x0 + bw > rx - 9 && y0 <= ry + 9 && y0 + bh > ry - 9 {
            return false;
        }
        let (mut lo, mut hi) = (9, -1);
        for y in y0 - 1..=y0 + bh {
            for x in x0 - 1..=x0 + bw {
                let c = y as usize * self.grid.width + x as usize;
                let ch = self.grid.chars[c];
                if matches!(ch, b'B' | b'R') {
                    return false;
                }
                if x < x0 || y < y0 || x >= x0 + bw || y >= y0 + bh {
                    continue;
                }
                if !matches!(ch, b'.' | b'O')
                    || self.roads[c] != 0
                    || self.region_map[c] != r.id as i16
                {
                    return false;
                }
                lo = lo.min(self.grid.heights[c]);
                hi = hi.max(self.grid.heights[c]);
            }
        }
        hi - lo <= 1
    }
    fn fronts(&self, l: Lot) -> bool {
        let w = self.grid.width;
        for x in l.x - 1..=l.x + l.w {
            if self.roads[(l.y - 1) as usize * w + x as usize] != 0
                || self.roads[(l.y + l.h) as usize * w + x as usize] != 0
            {
                return true;
            }
        }
        for y in l.y..l.y + l.h {
            if self.roads[y as usize * w + (l.x - 1) as usize] != 0
                || self.roads[y as usize * w + (l.x + l.w) as usize] != 0
            {
                return true;
            }
        }
        false
    }
    fn build(&mut self, l: Lot, kind: Option<&str>) -> Lot {
        for y in l.y..l.y + l.h {
            for x in l.x..l.x + l.w {
                self.grid.chars[y as usize * self.grid.width + x as usize] = b'B';
            }
        }
        if let Some(kind) = kind {
            self.buildings.push(Building {
                x: l.x as usize,
                y: l.y as usize,
                kind: kind.into(),
            });
        }
        l
    }
    fn site(
        &mut self,
        r: &Region,
        size: [isize; 2],
        far: isize,
        kind: Option<&str>,
        tries: usize,
    ) -> Option<Lot> {
        for k in 0..tries {
            let turn = self.town.next() < 0.5;
            let (bw, bh) = if turn {
                (size[1], size[0])
            } else {
                (size[0], size[1])
            };
            let x = r.x as isize + (self.town.next() * (2 * far + 1) as f64).floor() as isize
                - far
                - (bw >> 1);
            let y = r.y as isize + (self.town.next() * (2 * far + 1) as f64).floor() as isize
                - far
                - (bh >> 1);
            let l = Lot { x, y, w: bw, h: bh };
            if self.lot(r, l) && (k as f64 > tries as f64 * 0.6 || self.fronts(l)) {
                return Some(self.build(l, kind));
            }
        }
        None
    }
    fn house(&mut self) -> [isize; 2] {
        [[3, 4], [3, 4], [3, 5], [4, 4], [3, 7], [4, 6], [2, 6]]
            [(self.town.next() * 7.0).floor() as usize]
    }
    fn region(&mut self, r: &Region, biome: Biome, salt: u32) {
        match r.kind {
            RegionKind::City => {
                let mut sides = [0, 1, 2, 3];
                for i in (1..=3).rev() {
                    let j = (self.town.next() * (i + 1) as f64).floor() as usize;
                    sides.swap(i, j);
                }
                let mut church = None;
                'search: for back in [10, 11, 12] {
                    for side in sides {
                        for shift in [0, -2, 2, -4, 4, -6, 6, -8, 8] {
                            let along = side & 1 != 0;
                            let (bw, bh) = if along { (4, 7) } else { (7, 4) };
                            let x = if along {
                                if side == 1 {
                                    r.x as isize + back
                                } else {
                                    r.x as isize - back - 3
                                }
                            } else {
                                r.x as isize - 3 + shift
                            };
                            let y = if along {
                                r.y as isize - 3 + shift
                            } else if side == 0 {
                                r.y as isize - back - 3
                            } else {
                                r.y as isize + back
                            };
                            let l = Lot { x, y, w: bw, h: bh };
                            if self.lot(r, l) {
                                church = Some(self.build(l, Some("church")));
                                break 'search;
                            }
                        }
                    }
                }
                if church.is_none() {
                    self.site(r, [4, 7], 22, Some("church"), 120);
                }
                let n = 10 + (self.town.next() * 5.0).floor() as usize;
                for _ in 0..n {
                    let size = self.house();
                    self.site(r, size, 22, None, 60);
                }
                for _ in 0..3 {
                    let bh = 2 + (self.town.next() * 2.0).floor() as isize;
                    self.site(r, [2, bh], 22, None, 60);
                }
            }
            RegionKind::Industrial => {
                let works = self
                    .site(r, [5, 8], 18, Some("factory"), 90)
                    .or_else(|| self.site(r, [4, 6], 18, Some("factory"), 90));
                if let Some(a) = works {
                    let across = a.w < a.h;
                    for i in 1..=3 {
                        let (bw, bh) = if across { (3, 6) } else { (6, 3) };
                        let x = if across {
                            a.x + a.w + 2 + (i - 1) * (bw + 2)
                        } else {
                            a.x + (self.town.next() * 3.0).floor() as isize
                        };
                        let y = if across {
                            a.y + (self.town.next() * 3.0).floor() as isize
                        } else {
                            a.y + a.h + 2 + (i - 1) * (bh + 2)
                        };
                        let l = Lot { x, y, w: bw, h: bh };
                        if self.lot(r, l) {
                            self.build(l, None);
                        }
                    }
                }
                for _ in 0..2 {
                    self.site(r, [2, 3], 20, None, 60);
                }
            }
            RegionKind::Resource => {
                if let Some(head) = self.site(r, [3, 6], 16, Some("mine head"), 90) {
                    for _ in 0..2 {
                        for _ in 0..30 {
                            let a = self.town.next() * std::f64::consts::PI * 2.0;
                            let d = 6.0 + self.town.next() * 5.0;
                            let cx =
                                math::round(head.x as f64 + head.w as f64 / 2.0 + math::cos(a) * d)
                                    as isize;
                            let cy =
                                math::round(head.y as f64 + head.h as f64 / 2.0 + math::sin(a) * d)
                                    as isize;
                            let mut cells = Vec::new();
                            let mut ok = true;
                            'tip: for dy in -2..=2 {
                                for dx in -2..=2 {
                                    if (dx * dx + dy * dy) as f64
                                        > 3.2 + mix(cx + dx, cy + dy, salt) * 1.6
                                    {
                                        continue;
                                    }
                                    let l = Lot {
                                        x: cx + dx,
                                        y: cy + dy,
                                        w: 1,
                                        h: 1,
                                    };
                                    if !self.lot(r, l) {
                                        ok = false;
                                        break 'tip;
                                    }
                                    cells.push(l);
                                }
                            }
                            if !ok {
                                continue;
                            }
                            for l in cells {
                                self.grid.chars[l.y as usize * self.grid.width + l.x as usize] =
                                    b'R';
                            }
                            break;
                        }
                    }
                }
                for _ in 0..2 {
                    self.site(r, [2, 3], 18, None, 60);
                }
            }
            RegionKind::Rural => {}
        }
        let villages = matches!(biome, Biome::Farmland | Biome::Orchard);
        if r.kind == RegionKind::Rural || villages {
            let n = if villages {
                2 + (self.town.next() * 2.0).floor() as usize
            } else {
                1
            };
            for _ in 0..n {
                self.site(r, [3, 4], 20, None, 60);
            }
            let n = if villages {
                1 + (self.town.next() * 2.0).floor() as usize
            } else {
                usize::from(self.town.next() < 0.6)
            };
            for _ in 0..n {
                self.site(r, [3, 3], 20, None, 60);
            }
            if villages {
                self.site(r, [2, 2], 20, None, 60);
            }
        }
    }
}
pub(super) fn generate(
    seed: u32,
    grid: &mut Grid,
    regions: &[Region],
    region_map: &[i16],
    roads: &[u8],
) -> Scenery {
    let (w, h) = (grid.width, grid.height);
    let mut pick = Random(seed ^ 0x2545f491);
    let phase = pick.next() * std::f64::consts::PI * 2.0;
    let salt = seed ^ 0x1b873593;
    let mut stats = vec![Stats::default(); regions.len()];
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            let c = y * w + x;
            let id = region_map[c];
            if id < 0 {
                continue;
            }
            let st = &mut stats[id as usize];
            st.n += 1;
            st.height += grid.heights[c] as usize;
            st.wood += usize::from(grid.chars[c] == b'O');
            st.wet += usize::from(
                grid.chars[c] == b'F'
                    || [c - w, c + w, c - 1, c + 1]
                        .iter()
                        .any(|&n| grid.chars[n] == b'W'),
            );
        }
    }
    let mut provinces = Vec::new();
    for _ in 0..(math::round(regions.len() as f64 / 10.0) as usize).max(3) {
        let x = pick.next() * w as f64;
        let y = pick.next() * h as f64;
        let radius = 90.0 + pick.next() * 70.0;
        let biome = (pick.next() * 6.0).floor() as usize;
        provinces.push((x, y, radius, biome));
    }
    let mut looks = Vec::new();
    for r in regions {
        let st = stats[r.id];
        let (mean, wet, wood) = if st.n != 0 {
            (
                st.height as f64 / st.n as f64,
                st.wet as f64 / st.n as f64,
                st.wood as f64 / st.n as f64,
            )
        } else {
            (0.0, 0.0, 0.0)
        };
        let flat = (1.0 - mean / 2.0).max(0.0);
        let village = matches!(r.kind, RegionKind::City | RegionKind::Rural);
        let mut weight = [
            1.1 * flat + if village { 0.4 } else { 0.0 },
            0.25 + mean * 0.45 + wood * 4.0,
            if r.home.is_none() {
                wet * 9.0 + if mean < 0.4 { 0.25 } else { 0.0 }
            } else {
                0.0
            },
            (mean - 1.0).max(0.0) * 1.6
                + if r.kind == RegionKind::Resource {
                    0.6
                } else {
                    0.0
                },
            0.7 * flat + if village { 0.3 } else { 0.0 },
            0.7 * flat * (1.0 - wood * 3.0).max(0.0),
        ];
        for &(x, y, radius, b) in &provinces {
            weight[b] *=
                1.0 + 2.5 * (1.0 - math::hypot(x - r.x as f64, y - r.y as f64) / radius).max(0.0);
        }
        let mut total = 0.0;
        for v in weight {
            total += v;
        }
        let mut roll = pick.next() * total;
        let mut biome = Biome::Farmland;
        for (i, v) in weight.into_iter().enumerate() {
            roll -= v;
            if roll < 0.0 {
                biome = BIOMES[i];
                break;
            }
        }
        looks.push(biome);
    }
    let mut painter = Painter {
        grid,
        region_map,
        roads,
        town: Random(seed ^ 0x68e31da4),
        buildings: Vec::new(),
    };
    for r in regions {
        painter.region(r, looks[r.id], salt);
    }
    for r in regions {
        if r.kind == RegionKind::City {
            for dy in -7isize..=7 {
                for dx in -7isize..=7 {
                    let c = (r.y as isize + dy) as usize * w + (r.x as isize + dx) as usize;
                    if dx.abs() + dy.abs() <= 12 && painter.grid.chars[c] == b'.' {
                        painter.grid.chars[c] = b'D';
                    }
                }
            }
        }
    }
    let mut biomes = vec![255; w * h];
    for y in 0..h {
        for x in 0..w {
            let c = y * w + x;
            if region_map[c] < 0 {
                continue;
            }
            let jx = math::round(
                x as f64
                    + 5.0 * math::sin(y as f64 / 9.0 + phase)
                    + 4.0 * (mix(x as isize, y as isize, salt.wrapping_add(1)) - 0.5),
            ) as isize;
            let jy = math::round(
                y as f64
                    + 5.0 * math::cos(x as f64 / 11.0 - phase)
                    + 4.0 * (mix(x as isize, y as isize, salt.wrapping_add(2)) - 0.5),
            ) as isize;
            let id = if jx >= 0
                && jy >= 0
                && jx < w as isize
                && jy < h as isize
                && region_map[jy as usize * w + jx as usize] >= 0
            {
                region_map[jy as usize * w + jx as usize]
            } else {
                region_map[c]
            };
            biomes[c] = looks[id as usize] as u8;
        }
    }
    let fields: Vec<_> = regions
        .iter()
        .map(|_| {
            (
                (pick.next() * 13.0).floor() as isize,
                (pick.next() * 13.0).floor() as isize,
                11 + (pick.next() * 5.0).floor() as isize,
                11 + (pick.next() * 5.0).floor() as isize,
            )
        })
        .collect();
    for y in 3..h - 3 {
        for x in 3..w - 3 {
            let c = y * w + x;
            let id = region_map[c];
            if id < 0
                || painter.grid.chars[c] != b'.'
                || roads[c] != 0
                || near(painter.grid, c, w, b"BR")
            {
                continue;
            }
            let r = &regions[id as usize];
            let keep = if r.kind == RegionKind::City { 24 } else { 13 };
            if x.abs_diff(r.x).max(y.abs_diff(r.y)) <= keep {
                continue;
            }
            let wet = near(painter.grid, c, w, b"WF");
            let (xx, yy) = (x as isize, y as isize);
            match BIOMES[biomes[c] as usize] {
                Biome::Farmland if !wet => {
                    let (ox, oy, sx, sy) = fields[id as usize];
                    let u = xx - r.x as isize + ox + 1300;
                    let v = yy - r.y as isize + oy + 1300;
                    let on_x = v % sy == 0;
                    let on_y = u % sx == 0;
                    if !on_x && !on_y {
                        continue;
                    }
                    let (seg, line, at) = if on_x {
                        (u / sx, v, u % sx)
                    } else {
                        (v / sy, u, v % sy)
                    };
                    if mix(seg, line + if on_x { 0 } else { 7919 }, salt) > 0.55 {
                        continue;
                    }
                    if (5..=6).contains(&at) && mix(seg, line, salt.wrapping_add(3)) < 0.5 {
                        continue;
                    }
                    painter.grid.chars[c] = b'H';
                }
                Biome::Pine if !wet => {
                    let d = math::sin(x as f64 / 13.0 + phase)
                        + math::cos(y as f64 / 17.0 - phase)
                        + 0.6 * math::sin((xx - yy) as f64 / 9.0 + phase);
                    if d > 0.45 && mix(xx, yy, salt.wrapping_add(4)) < 0.8 {
                        painter.grid.chars[c] = b'O';
                    }
                }
                Biome::Marsh if painter.grid.heights[c] == 0 => {
                    let m = math::sin(x as f64 / 7.0 + phase) * math::cos(y as f64 / 9.0 - phase)
                        + 0.5 * math::sin((x + y) as f64 / 5.0 + phase)
                        + if wet { 0.4 } else { 0.0 };
                    if m > 0.55 && mix(xx, yy, salt.wrapping_add(5)) < 0.85 {
                        painter.grid.chars[c] = b'M';
                    }
                }
                _ => {}
            }
        }
    }
    Scenery {
        regions: looks,
        cells: biomes,
        buildings: painter.buildings,
    }
}
fn near(grid: &Grid, c: usize, w: usize, chars: &[u8]) -> bool {
    [c - w, c + w, c - 1, c + 1]
        .into_iter()
        .any(|n| chars.contains(&grid.chars[n]))
}
