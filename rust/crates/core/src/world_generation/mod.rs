//! Pure seeded base World geography. Biome dressing, landmarks and rails are later stages.
//! All coordinates and bounds here are cells, never simulation metres.
mod landforms;
mod roads;
mod territories;

use crate::{
    math,
    random::{MatchRandom, StreamId},
};

pub const GENERATOR_VERSION: u32 = 1;
pub const REGION_CELLS: usize = 64;

pub use crate::world_map::WorldSize;
impl WorldSize {
    fn columns(self) -> usize {
        match self {
            Self::Huge => 8,
            Self::Massive => 16,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellPoint {
    pub x: usize,
    pub y: usize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegionKind {
    Rural,
    City,
    Industrial,
    Resource,
}
impl RegionKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Rural => "rural",
            Self::City => "city",
            Self::Industrial => "industrial",
            Self::Resource => "resource",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Region {
    pub id: usize,
    pub kind: RegionKind,
    pub x: usize,
    pub y: usize,
    pub bounds: [usize; 4],
    pub home: Option<usize>,
    pub team: Option<i32>,
    pub name: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RiverPoint {
    pub x: usize,
    pub y: usize,
    pub radius: usize,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct River {
    pub id: usize,
    pub parent: Option<usize>,
    pub path: Vec<RiverPoint>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Crossing {
    pub x: usize,
    pub y: usize,
    pub kind: u8,
    pub river: usize,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Waterways {
    pub rivers: Vec<River>,
    pub crossings: Vec<Crossing>,
    pub lake: CellPoint,
    pub passes: Vec<CellPoint>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Grid {
    pub width: usize,
    pub height: usize,
    pub chars: Vec<u8>,
    pub heights: Vec<i8>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GeneratedGeography {
    pub size: WorldSize,
    pub terrain_seed: u32,
    pub grid: Grid,
    pub regions: Vec<Region>,
    pub spawns: Vec<CellPoint>,
    pub region_map: Vec<i16>,
    pub waterways: Waterways,
}
impl GeneratedGeography {
    /// Transfer bare geography to the agreed loader input without rebuilding terrain from JSON.
    /// Native version 1 leaves biomes, landmark layers and rails for later generation stages.
    pub fn into_world_map_input(self) -> crate::world_map::WorldMapInput {
        use crate::{
            map::Spawn,
            world_map::{WorldDefinition, WorldMapInput, WorldRegion},
        };
        use serde_json::json;
        let rivers = self.waterways.rivers.into_iter().map(|r| json!({
            "id": r.id, "kind": if r.parent.is_some() { "tributary" } else { "main" },
            "parent": r.parent,
            "path": r.path.into_iter().map(|p| json!({"x":p.x,"y":p.y,"radius":p.radius})).collect::<Vec<_>>()
        })).collect::<Vec<_>>();
        let crossings = self
            .waterways
            .crossings
            .into_iter()
            .map(|c| {
                json!({
                    "x":c.x,"y":c.y,"type":char::from(c.kind).to_string(),"river":c.river
                })
            })
            .collect::<Vec<_>>();
        let waterways = json!({"rivers":rivers,"crossings":crossings,
            "lake":{"x":self.waterways.lake.x,"y":self.waterways.lake.y}});
        WorldMapInput {
            name: format!(
                "World Conquest ({})",
                match self.size {
                    WorldSize::Huge => "Huge",
                    WorldSize::Massive => "Massive",
                }
            ),
            width: self.grid.width,
            height: self.grid.height,
            chars: self.grid.chars,
            heights: Some(self.grid.heights),
            layers: None,
            buildings: Vec::new(),
            spawns: self
                .spawns
                .into_iter()
                .map(|p| Spawn {
                    x: p.x,
                    y: p.y,
                    assault: false,
                })
                .collect(),
            geography: WorldDefinition {
                size: self.size,
                region_cells: REGION_CELLS,
                regions: self
                    .regions
                    .into_iter()
                    .map(|r| WorldRegion {
                        id: r.id as i32,
                        x: r.x,
                        y: r.y,
                        bounds: r.bounds,
                        home: r.home,
                        team: r.team,
                        kind: Some(r.kind.name().into()),
                        name: Some(r.name),
                        biome: None,
                    })
                    .collect(),
                region_map: Some(self.region_map),
                biomes: None,
                seed: Some(self.terrain_seed),
                generator_version: Some(GENERATOR_VERSION),
                metadata: std::collections::BTreeMap::from([("waterways".into(), waterways)]),
            },
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GenerationError {
    Players,
    Teams,
    NoChannelRoute,
    NoRegionalSite,
    NoHomeSite,
    NoRoadAccess,
    DisconnectedRoadGraph,
}
impl std::fmt::Display for GenerationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "World generation: {self:?}")
    }
}
impl std::error::Error for GenerationError {}

/// Consume one recorded Terrain-stream draw. No other purpose is touched.
pub fn generate(
    random: &mut MatchRandom,
    size: WorldSize,
    teams: &[i32],
) -> Result<GeneratedGeography, GenerationError> {
    validate(teams)?;
    generate_from_terrain_seed(random.next_u64(StreamId::Terrain) as u32, size, teams)
}
/// Explicit recorded geography seed. Keeps source landscape arithmetic independently testable.
pub fn generate_from_terrain_seed(
    seed: u32,
    size: WorldSize,
    teams: &[i32],
) -> Result<GeneratedGeography, GenerationError> {
    validate(teams)?;
    let cols = size.columns();
    let w = cols * REGION_CELLS;
    let h = 512;
    let mut grid = Grid {
        width: w,
        height: h,
        chars: vec![b'.'; w * h],
        heights: landforms::relief(w, h, seed),
    };
    let waterways = landforms::waterways(&mut grid, seed)?;
    let mut random = Random(seed);
    let mut regions: Vec<Region> = Vec::with_capacity(cols * 8);
    for ry in 0..8 {
        for rx in 0..cols {
            let cx = rx * 64 + 32;
            let cy = ry * 64 + 32;
            let mut best = None;
            let mut score = f64::INFINITY;
            for _ in 0..140 {
                let x = cx + (random.next() * 33.0).floor() as usize - 16;
                let y = cy + (random.next() * 33.0).floor() as usize - 16;
                if !safe_site(&grid, x, y, 10) {
                    continue;
                }
                let candidate = f64::from(grid.heights[y * w + x]) * 30.0
                    + math::hypot(x as f64 - cx as f64, y as f64 - cy as f64) * 0.25;
                if candidate < score {
                    score = candidate;
                    best = Some(CellPoint { x, y });
                }
            }
            if best.is_none() {
                'scan: for y in ry * 64 + 10..(ry + 1) * 64 - 10 {
                    for x in rx * 64 + 10..(rx + 1) * 64 - 10 {
                        if safe_site(&grid, x, y, 8) {
                            best = Some(CellPoint { x, y });
                            break 'scan;
                        }
                    }
                }
            }
            if best.is_none() {
                'rings: for radius in (24..=80).step_by(4) {
                    for a in 0..32 {
                        let angle = a as f64 * std::f64::consts::PI / 16.0;
                        let x = math::round(cx as f64 + math::cos(angle) * radius as f64) as isize;
                        let y = math::round(cy as f64 + math::sin(angle) * radius as f64) as isize;
                        if x < 12 || y < 12 || x >= w as isize - 12 || y >= h as isize - 12 {
                            continue;
                        }
                        let x = x as usize;
                        let y = y as usize;
                        if regions.iter().any(|r| {
                            math::hypot(r.x as f64 - x as f64, r.y as f64 - y as f64) < 26.0
                        }) {
                            continue;
                        }
                        if safe_site(&grid, x, y, 10) {
                            best = Some(CellPoint { x, y });
                            break 'rings;
                        }
                    }
                }
            }
            let at = best.ok_or(GenerationError::NoRegionalSite)?;
            let kind = [
                RegionKind::Rural,
                RegionKind::City,
                RegionKind::Industrial,
                RegionKind::Resource,
            ][(random.next() * 4.0).floor() as usize];
            regions.push(Region {
                id: ry * cols + rx,
                kind,
                x: at.x,
                y: at.y,
                bounds: [rx * 64, ry * 64, (rx + 1) * 64, (ry + 1) * 64],
                home: None,
                team: None,
                name: String::new(),
            });
            clear(&mut grid, at, 8);
        }
    }
    let road = roads::generate(&mut grid, &regions, &waterways)?;
    // The foliage expression uses the source's decimal constant, not TAU.
    #[allow(clippy::approx_constant)]
    let phase = f64::from(seed) / 4_294_967_296.0 * 6.283;
    for y in 2..h - 2 {
        for x in 2..w - 2 {
            let c = y * w + x;
            if grid.chars[c] != b'.' || road[c] != 0 {
                continue;
            }
            let density = math::sin(x as f64 / 19.0 + phase)
                + math::cos(y as f64 / 23.0 - phase)
                + 0.6 * math::sin((x + y) as f64 / 11.0);
            if density > 0.9 && random.next() < 0.72 {
                grid.chars[c] = b'O';
            }
        }
    }
    for r in &regions {
        if r.kind == RegionKind::City {
            for dy in (-17isize..=17).step_by(4) {
                for dx in (-17isize..=17).step_by(4) {
                    if dx.abs() < 10 && dy.abs() < 10 {
                        continue;
                    }
                    let x = r.x as isize + dx;
                    let y = r.y as isize + dy;
                    if x < 3
                        || y < 3
                        || x >= w as isize - 3
                        || y >= h as isize - 3
                        || random.next() > 0.65
                    {
                        continue;
                    }
                    let x = x as usize;
                    let y = y as usize;
                    if (0..2).all(|yy| {
                        (0..2).all(|xx| {
                            matches!(grid.chars[(y + yy) * w + x + xx], b'.' | b'O')
                                && road[(y + yy) * w + x + xx] == 0
                        })
                    }) {
                        for yy in 0..2 {
                            for xx in 0..2 {
                                grid.chars[(y + yy) * w + x + xx] = b'B';
                            }
                        }
                    }
                }
            }
        }
        clear(&mut grid, CellPoint { x: r.x, y: r.y }, 8);
    }
    let rotation = (random.next() * regions.len() as f64).floor() as usize;
    let mut homes: Vec<usize> = Vec::new();
    for (slot, &team) in teams.iter().enumerate() {
        let mut best = None;
        let mut score = -1.0;
        for k in 0..regions.len() {
            let id = (k + rotation) % regions.len();
            let r = &regions[id];
            if homes.contains(&id) {
                continue;
            }
            let expansion = regions
                .iter()
                .filter(|a| a.id != id)
                .map(|a| distance(a, r))
                .fold(f64::INFINITY, f64::min);
            if !(50.0..=70.0).contains(&expansion) {
                continue;
            }
            let d = if homes.is_empty() {
                math::hypot(r.x as f64 - w as f64 / 2.0, r.y as f64 - h as f64 / 2.0)
            } else {
                homes
                    .iter()
                    .map(|&id| distance(&regions[id], r))
                    .fold(f64::INFINITY, f64::min)
            };
            if d > score {
                score = d;
                best = Some(id);
            }
        }
        let id = best.ok_or(GenerationError::NoHomeSite)?;
        homes.push(id);
        regions[id].home = Some(slot);
        regions[id].team = Some(team);
    }
    let mut name_seed = seed ^ 0x9e37_79b9;
    let heads = [
        "Alden", "Bray", "Carn", "Dorn", "Elm", "Falk", "Gram", "Hart", "Kessel", "Lind", "Mar",
        "Nor", "Ost", "Ravel", "Stein", "Wald",
    ];
    let tails = [
        "burg", "court", "dorf", "feld", "ford", "heim", "mont", "ville", "wick", "stadt",
    ];
    let mut pool: Vec<_> = heads
        .iter()
        .flat_map(|a| tails.iter().map(move |b| format!("{a}{b}")))
        .collect();
    for r in &mut regions {
        name_seed = name_seed
            .wrapping_mul(1_664_525)
            .wrapping_add(1_013_904_223);
        let mut name = pool.remove(name_seed as usize % pool.len());
        name.push_str(match r.kind {
            RegionKind::Industrial => " Works",
            RegionKind::Resource => " Mines",
            _ => "",
        });
        r.name = name;
    }
    let region_map = territories::generate(&grid, &mut regions);
    let spawns = homes
        .into_iter()
        .map(|id| CellPoint {
            x: regions[id].x,
            y: regions[id].y,
        })
        .collect();
    Ok(GeneratedGeography {
        size,
        terrain_seed: seed,
        grid,
        regions,
        spawns,
        region_map,
        waterways,
    })
}
fn validate(teams: &[i32]) -> Result<(), GenerationError> {
    if !(1..=6).contains(&teams.len()) {
        return Err(GenerationError::Players);
    }
    if teams.iter().any(|t| !(0..=5).contains(t)) {
        return Err(GenerationError::Teams);
    }
    Ok(())
}
fn safe_site(g: &Grid, x: usize, y: usize, r: usize) -> bool {
    (y - r..=y + r).all(|yy| {
        (x - r..=x + r).all(|xx| !matches!(g.chars[yy * g.width + xx], b'W' | b'F' | b'='))
    })
}
fn clear(g: &mut Grid, at: CellPoint, r: usize) {
    for y in at.y - r..=at.y + r {
        for x in at.x - r..=at.x + r {
            let c = y * g.width + x;
            if g.chars[c] != b'D' {
                g.chars[c] = b'.';
            }
            g.heights[c] = 0;
        }
    }
}
fn distance(a: &Region, b: &Region) -> f64 {
    math::hypot(a.x as f64 - b.x as f64, a.y as f64 - b.y as f64)
}

struct Random(u32);
impl Random {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x6d2b_79f5);
        let mut t = self.0;
        t = (t ^ (t >> 15)).wrapping_mul(t | 1);
        t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 61));
        f64::from(t ^ (t >> 14)) / 4_294_967_296.0
    }
}
// Score-only source heap. Insertion and sift order define ties, not host containers.
pub(super) struct MinHeap<T>(Vec<(f64, T)>);
impl<T: Copy> MinHeap<T> {
    fn new() -> Self {
        Self(Vec::new())
    }
    fn clear(&mut self) {
        self.0.clear();
    }
    fn push(&mut self, score: f64, value: T) {
        let mut i = self.0.len();
        self.0.push((score, value));
        while i > 0 {
            let p = (i - 1) / 2;
            if self.0[p].0 <= score {
                break;
            }
            self.0[i] = self.0[p];
            i = p;
        }
        self.0[i] = (score, value);
    }
    fn pop(&mut self) -> Option<(f64, T)> {
        let first = *self.0.first()?;
        let last = self.0.pop()?;
        if !self.0.is_empty() {
            let mut i = 0;
            while i * 2 + 1 < self.0.len() {
                let mut j = i * 2 + 1;
                if j + 1 < self.0.len() && self.0[j + 1].0 < self.0[j].0 {
                    j += 1;
                }
                if self.0[j].0 >= last.0 {
                    break;
                }
                self.0[i] = self.0[j];
                i = j;
            }
            self.0[i] = last;
        }
        Some(first)
    }
}
