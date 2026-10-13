//! Match spawn filtering, walking distances, layout selection and team VP goals.
//!
//! Terrain must come from the raw authoring rows, before building footprints or
//! match-only terrain edits. A caller supplies the shuffle fraction, normally from
//! its recorded match stream. This module consumes no global randomness.
use crate::{
    catalog::{Catalog, CatalogError},
    economy::Mode,
    game::{FORD, MOVE},
    math,
};
use std::{
    collections::{BTreeSet, VecDeque},
    fmt,
};

// The reference supports six players and at most six authored spawns. Keeping this
// bound also limits shared layouts to 5^6 = 15,625. This is not a general allocator.
const MAX_PLAYERS: usize = 6;
const MAX_DIMENSION: usize = 1024;
const FORD_COST: u32 = 6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Spawn {
    pub x: usize,
    pub y: usize,
    pub assault: bool,
}
#[derive(Clone, Copy, Debug)]
pub struct TerrainView<'a> {
    pub width: usize,
    pub height: usize,
    pub flags: &'a [u16],
    /// Authoring levels -2..=4, before any match edits. Missing heights mean zero.
    pub heights: Option<&'a [i8]>,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LayoutSelection {
    First,
    /// Uniform fraction in [0, 1), supplied from a recorded random stream.
    RandomFraction(f64),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Catalog(CatalogError),
    PlayerCount,
    SpawnCount,
    TerrainShape,
    HeightRange,
    SpawnCell(usize),
    SpawnIndex(usize),
    DistanceShape,
    DistanceValue,
    ScoreRange,
    RandomFraction,
}
impl From<CatalogError> for Error {
    fn from(value: CatalogError) -> Self {
        Self::Catalog(value)
    }
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Catalog(e) => e.fmt(f),
            Self::PlayerCount => write!(f, "deployment needs 1..=6 players"),
            Self::SpawnCount => write!(f, "deployment needs 1..=6 spawns"),
            Self::TerrainShape => write!(
                f,
                "deployment terrain must have matching 1..=1024 dimensions and buffers"
            ),
            Self::HeightRange => write!(f, "deployment heights must be in -2..=4"),
            Self::SpawnCell(i) => {
                write!(f, "spawn {i} is outside the terrain or on blocked ground")
            }
            Self::SpawnIndex(i) => write!(f, "unknown spawn index: {i}"),
            Self::DistanceShape => write!(
                f,
                "deployment distance matrix must be square with one row per spawn"
            ),
            Self::DistanceValue => write!(f, "deployment distances must be finite and nonnegative"),
            Self::ScoreRange => write!(f, "deployment score exceeds finite arithmetic"),
            Self::RandomFraction => {
                write!(f, "deployment random fraction must be finite and in [0, 1)")
            }
        }
    }
}
impl std::error::Error for Error {}

fn spawn_count(n: usize) -> Result<(), Error> {
    if (1..=MAX_PLAYERS).contains(&n) {
        Ok(())
    } else {
        Err(Error::SpawnCount)
    }
}
fn player_count(n: usize) -> Result<(), Error> {
    if (1..=MAX_PLAYERS).contains(&n) {
        Ok(())
    } else {
        Err(Error::PlayerCount)
    }
}

/// Indices stay in authoring order; only Assault uses Assault-only spawns.
pub fn spawns_for(spawns: &[Spawn], mode: Mode) -> Result<Vec<usize>, Error> {
    spawn_count(spawns.len())?;
    Ok(spawns
        .iter()
        .enumerate()
        .filter_map(|(i, s)| (mode == Mode::Assault || !s.assault).then_some(i))
        .collect())
}

/// Scale the compiled Conquest VP goal by average team size.
pub fn win_vp(teams: &[i32]) -> Result<f64, Error> {
    player_count(teams.len())?;
    let base = Catalog::compiled()?
        .table("CFG")?
        .get("vpToWin")
        .and_then(|v| v.number())
        .ok_or_else(|| CatalogError("missing numeric CFG.vpToWin".into()))?;
    let distinct = teams.iter().copied().collect::<BTreeSet<_>>().len();
    Ok(base * teams.len() as f64 / distinct as f64)
}

impl TerrainView<'_> {
    fn validate(&self, spawns: &[Spawn], indices: &[usize]) -> Result<usize, Error> {
        spawn_count(spawns.len())?;
        if self.width == 0
            || self.height == 0
            || self.width > MAX_DIMENSION
            || self.height > MAX_DIMENSION
        {
            return Err(Error::TerrainShape);
        }
        let cells = self.width * self.height;
        if self.flags.len() != cells || self.heights.is_some_and(|v| v.len() != cells) {
            return Err(Error::TerrainShape);
        }
        if self
            .heights
            .is_some_and(|v| v.iter().any(|h| !(-2..=4).contains(h)))
        {
            return Err(Error::HeightRange);
        }
        for (i, s) in spawns.iter().enumerate() {
            if s.x >= self.width
                || s.y >= self.height
                || self.flags[s.y * self.width + s.x] & MOVE != 0
            {
                return Err(Error::SpawnCell(i));
            }
        }
        if indices.len() > MAX_PLAYERS {
            return Err(Error::SpawnCount);
        }
        for &i in indices {
            if i >= spawns.len() {
                return Err(Error::SpawnIndex(i));
            }
        }
        Ok(cells)
    }
}

/// Row-major walking-distance matrix for the selected spawn indices. The distance
/// can be asymmetric when one spawn itself is on a ford, since entering it costs six.
/// Tiny maps are allowed for component fixtures; the map loader validates game dimensions.
pub fn spawn_distances(
    terrain: &TerrainView<'_>,
    spawns: &[Spawn],
    indices: &[usize],
) -> Result<Vec<f64>, Error> {
    let cells = terrain.validate(spawns, indices)?;
    let mut result = Vec::with_capacity(indices.len() * indices.len());
    for &i in indices {
        let start = spawns[i];
        let mut distance = vec![u32::MAX; cells];
        // Edge weights are one or six. Seven circular queues preserve the source's
        // distance-bucket insertion order without allocating one queue per distance.
        let mut buckets: [VecDeque<usize>; FORD_COST as usize + 1] =
            std::array::from_fn(|_| VecDeque::new());
        let start_cell = start.y * terrain.width + start.x;
        buckets[0].push_back(start_cell);
        distance[start_cell] = 0;
        let mut pending = 1_usize;
        let mut cost = 0_u32;
        while pending != 0 {
            let bucket = cost as usize % buckets.len();
            while let Some(cell) = buckets[bucket].pop_front() {
                pending -= 1;
                if distance[cell] != cost {
                    continue;
                }
                let x = cell % terrain.width;
                let y = cell / terrain.width;
                let level = terrain.heights.map_or(0, |v| v[cell]);
                let neighbors = [
                    (x + 1 < terrain.width).then_some(cell + 1),
                    (x > 0).then(|| cell - 1),
                    (y + 1 < terrain.height).then_some(cell + terrain.width),
                    (y > 0).then(|| cell - terrain.width),
                ];
                for n in neighbors.into_iter().flatten() {
                    let flags = terrain.flags[n];
                    let next_cost = cost + if flags & FORD != 0 { FORD_COST } else { 1 };
                    let next_level = terrain.heights.map_or(0, |v| v[n]);
                    if flags & MOVE == 0
                        && (i16::from(next_level) - i16::from(level)).abs() <= 1
                        && next_cost < distance[n]
                    {
                        distance[n] = next_cost;
                        let next_bucket = next_cost as usize % buckets.len();
                        buckets[next_bucket].push_back(n);
                        pending += 1;
                    }
                }
            }
            cost += 1;
        }
        for &j in indices {
            let end = spawns[j];
            let d = distance[end.y * terrain.width + end.x];
            result.push(if d == u32::MAX {
                4.0 * math::hypot(end.x as f64 - start.x as f64, end.y as f64 - start.y as f64)
            } else {
                f64::from(d)
            });
        }
    }
    Ok(result)
}

#[derive(Clone, Copy)]
struct Candidate {
    score: f64,
    near: f64,
}
fn score(
    slots: &[usize],
    teams: &[i32],
    n: usize,
    distances: Option<&[f64]>,
) -> Result<Candidate, Error> {
    let mut score = 0.0;
    let mut near = f64::INFINITY;
    for a in 0..teams.len() {
        for b in a + 1..teams.len() {
            let (sa, sb) = (slots[a], slots[b]);
            let d = distances.map_or_else(
                || {
                    let delta = sa.abs_diff(sb);
                    delta.min(n - delta) as f64
                },
                |v| v[sa * n + sb],
            );
            let same = teams[a] == teams[b];
            score += if same { -d } else { d };
            if !score.is_finite() {
                return Err(Error::ScoreRange);
            }
            if !same {
                near = math::min(near, d);
            }
        }
    }
    Ok(Candidate { score, near })
}

// `true` stops enumeration. Slot order is exactly the source recursion order.
fn layouts(
    n: usize,
    teams: usize,
    mut visit: impl FnMut(&[usize]) -> Result<bool, Error>,
) -> Result<(), Error> {
    fn place(
        n: usize,
        cur: &mut [usize],
        used: &mut [bool],
        p: usize,
        visit: &mut impl FnMut(&[usize]) -> Result<bool, Error>,
    ) -> Result<bool, Error> {
        if p == cur.len() {
            return visit(cur);
        }
        for s in 0..n {
            if !used[s] || cur.len() > n {
                used[s] = true;
                cur[p] = s;
                if place(n, cur, used, p + 1, visit)? {
                    return Ok(true);
                }
                used[s] = false;
            }
        }
        Ok(false)
    }
    place(n, &mut vec![0; teams], &mut vec![false; n], 0, &mut visit)?;
    Ok(())
}

/// Assign each player a relative spawn index. `None` distances use the source ring metric.
/// Four bounded scans preserve score/filter/enumeration order without retaining every
/// layout. Finite nonnegative matrices whose sums overflow are rejected.
pub fn spawn_slots(
    n_spawns: usize,
    teams: &[i32],
    distances: Option<&[f64]>,
    selection: LayoutSelection,
) -> Result<Vec<usize>, Error> {
    spawn_count(n_spawns)?;
    player_count(teams.len())?;
    if let Some(dist) = distances {
        if dist.len() != n_spawns * n_spawns {
            return Err(Error::DistanceShape);
        }
        if dist.iter().any(|v| !v.is_finite() || *v < 0.0) {
            return Err(Error::DistanceValue);
        }
    }
    let fraction = match selection {
        LayoutSelection::First => 0.0,
        LayoutSelection::RandomFraction(f) if f.is_finite() && (0.0..1.0).contains(&f) => f,
        LayoutSelection::RandomFraction(_) => return Err(Error::RandomFraction),
    };
    let mut best = f64::NEG_INFINITY;
    layouts(n_spawns, teams.len(), |slots| {
        best = math::max(best, score(slots, teams, n_spawns, distances)?.score);
        Ok(false)
    })?;
    let close = best - 0.03 * best.abs() - 1e-9;
    let mut nearest = f64::NEG_INFINITY;
    layouts(n_spawns, teams.len(), |slots| {
        let c = score(slots, teams, n_spawns, distances)?;
        if c.score >= close {
            nearest = math::max(nearest, c.near);
        }
        Ok(false)
    })?;
    let good = nearest * 0.97 - 1e-9;
    let mut count = 0;
    layouts(n_spawns, teams.len(), |slots| {
        let c = score(slots, teams, n_spawns, distances)?;
        if c.score >= close && c.near >= good {
            count += 1;
        }
        Ok(false)
    })?;
    // The validated product is nonnegative, so truncation equals Math.floor.
    let index = (fraction * count as f64) as usize;
    let mut found = None;
    let mut seen = 0;
    layouts(n_spawns, teams.len(), |slots| {
        let c = score(slots, teams, n_spawns, distances)?;
        if c.score >= close && c.near >= good {
            if seen == index {
                found = Some(slots.to_vec());
                return Ok(true);
            }
            seen += 1;
        }
        Ok(false)
    })?;
    found.ok_or(Error::ScoreRange)
}
