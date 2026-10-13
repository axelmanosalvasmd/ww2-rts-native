//! Spawn placement and initialization of fields represented by `game::Unit`.
//!
//! This is not the full spawn pipeline. The caller owns IDs, owner validation,
//! unit-table/grid insertion, abilities, logistics and later mode-specific setup.

use crate::catalog::{Catalog, CatalogError, Value};
use crate::game::{CELL, Point, Unit};
use crate::grid::UnitId;
use crate::math;
use crate::navigation::TerrainView;
use crate::rules::{Rules, UnitKind};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SpawnError {
    InvalidShape,
    InvalidPosition,
    InvalidId,
    UnknownKind,
    InvalidDefinition,
    UnsupportedAir,
    Catalog(CatalogError),
}
impl From<CatalogError> for SpawnError {
    fn from(error: CatalogError) -> Self {
        Self::Catalog(error)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpawnPlacement {
    pub cell: usize,
    pub point: Point,
}

#[derive(Default)]
pub struct SpawnPlacer {
    component: Vec<bool>,
    seen: Vec<u32>,
    generation: u32,
    queue: Vec<usize>,
}

impl SpawnPlacer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Breadth-first nearest-free search, optionally restricted to the origin's
    /// undirected movement component. If nothing qualifies, returns the clamped
    /// start cell even when it is blocked, exactly as the reference does.
    pub fn nearest_free(
        &mut self,
        view: TerrainView<'_>,
        at: Point,
        block: u16,
        origin: Option<Point>,
    ) -> Result<usize, SpawnError> {
        validate(view, at)?;
        if let Some(origin) = origin {
            validate(view, origin)?;
            let home = self.search(view, origin, block, false);
            self.fill_component(view, home, block);
        }
        Ok(self.search(view, at, block, origin.is_some()))
    }

    /// `index` is the caller's scatter slot (the reference defaults to unit count).
    /// Naval units may use another water region; all other supported kinds stay
    /// in the player's spawn region. Air-base placement is not implemented here.
    pub fn scatter_position(
        &mut self,
        view: TerrainView<'_>,
        kind: &UnitKind,
        player_spawn: Point,
        index: u32,
    ) -> Result<SpawnPlacement, SpawnError> {
        if kind.air {
            return Err(SpawnError::UnsupportedAir);
        }
        let angle = f64::from(index) * 2.4;
        let at = Point {
            x: player_spawn.x + math::cos(angle) * 4.0,
            z: player_spawn.z + math::sin(angle) * 4.0,
        };
        let cell = self.nearest_free(
            view,
            at,
            kind.block(),
            (!kind.naval).then_some(player_spawn),
        )?;
        Ok(SpawnPlacement {
            cell,
            point: center(view, cell),
        })
    }

    fn generation(&mut self, size: usize) -> u32 {
        self.seen.resize(size, 0);
        self.generation = self.generation.wrapping_add(1);
        if self.generation == 0 {
            self.seen.fill(0);
            self.generation = 1;
        }
        self.generation
    }

    fn search(&mut self, view: TerrainView<'_>, at: Point, block: u16, restrict: bool) -> usize {
        let x = ((at.x / CELL).floor().max(0.0) as usize).min(view.width - 1);
        let y = ((at.z / CELL).floor().max(0.0) as usize).min(view.height - 1);
        let start = y * view.width + x;
        let qualifies = |cell: usize| view.flags[cell] & block == 0;
        if qualifies(start) && (!restrict || self.component[start]) {
            return start;
        }
        let generation = self.generation(view.flags.len());
        self.queue.clear();
        self.queue.push(start);
        self.seen[start] = generation;
        let mut head = 0;
        while head < self.queue.len() {
            let c = self.queue[head];
            head += 1;
            if qualifies(c) && (!restrict || self.component[c]) {
                return c;
            }
            for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let Some(n) = neighbor(view, c, dx, dy) else {
                    continue;
                };
                if self.seen[n] != generation {
                    self.seen[n] = generation;
                    self.queue.push(n);
                }
            }
        }
        start
    }

    // Only component membership matters to nearestFree. Rebuild the origin's
    // component from supplied cells rather than retaining global numeric labels.
    fn fill_component(&mut self, view: TerrainView<'_>, home: usize, block: u16) {
        self.component.resize(view.flags.len(), false);
        self.component.fill(false);
        self.queue.clear();
        if view.flags[home] & block != 0 {
            return;
        }
        self.component[home] = true;
        self.queue.push(home);
        let directions = [
            (-1, 0),
            (1, 0),
            (0, -1),
            (0, 1),
            (-1, -1),
            (1, -1),
            (-1, 1),
            (1, 1),
        ];
        let count = if view.heights.is_some() { 8 } else { 4 };
        let mut head = 0;
        while head < self.queue.len() {
            let c = self.queue[head];
            head += 1;
            for &(dx, dy) in &directions[..count] {
                let Some(n) = neighbor(view, c, dx, dy) else {
                    continue;
                };
                if self.component[n]
                    || view.flags[n] & block != 0
                    || (level(view, n) - level(view, c)).abs() > 1
                {
                    continue;
                }
                if dx != 0 && dy != 0 {
                    let a = neighbor(view, c, dx, 0).expect("in-bounds corner");
                    let b = neighbor(view, c, 0, dy).expect("in-bounds corner");
                    if view.flags[a] & block != 0 || view.flags[b] & block != 0 {
                        continue;
                    }
                    let near = |from| {
                        (level(view, a) - level(view, from)).abs() <= 1
                            && (level(view, b) - level(view, from)).abs() <= 1
                    };
                    if !near(c) && !near(n) {
                        continue;
                    }
                }
                self.component[n] = true;
                self.queue.push(n);
            }
        }
    }
}

/// Project the reference's fresh unit into every field the current core carries.
/// No logistics/ability/combat fields are invented. Owner validity, registration
/// and spatial-index insertion remain the caller's responsibility. `path_gen=0`
/// is the native identity of this fresh empty route, not a reference field.
pub fn initial_unit(
    rules: &Rules,
    catalog: &Catalog,
    kind: usize,
    id: UnitId,
    owner: usize,
    at: Point,
) -> Result<Unit, SpawnError> {
    if id == 0 {
        return Err(SpawnError::InvalidId);
    }
    if !finite(at) {
        return Err(SpawnError::InvalidPosition);
    }
    let profile = rules.kinds.get(kind).ok_or(SpawnError::UnknownKind)?;
    if profile.air {
        return Err(SpawnError::UnsupportedAir);
    }
    let definition = catalog.unit(&profile.name)?;
    let models = definition
        .get("models")
        .and_then(Value::number)
        .ok_or(SpawnError::InvalidDefinition)?;
    let hp_per = definition
        .get("hpPer")
        .and_then(Value::number)
        .ok_or(SpawnError::InvalidDefinition)?;
    let hp = models * hp_per;
    if !models.is_finite() || models < 0.0 || !hp_per.is_finite() || hp_per < 0.0 || !hp.is_finite()
    {
        return Err(SpawnError::InvalidDefinition);
    }
    Ok(Unit {
        id,
        kind,
        owner,
        x: at.x,
        z: at.z,
        rot: 0.0,
        garrison: None,
        riding: false,
        air: None,
        still: Some(0.0),
        shot_at: None,
        dust: None,
        built: None,
        cells: None,
        heavy: !profile.infantry,
        hp,
        path: Vec::new(),
        path_gen: 0,
        reverse: None,
        pace: f64::NAN,
        attack: false,
        retreating: false,
        sprint: 0.0,
        supp: 0.0,
        hit_from: None,
        stuck: 0.0,
        repath: 0.0,
        world_goal: None,
        amove: None,
        move_speed: 0.0,
        travel_dir: Some(0.0),
        vx: Some(0.0),
        vz: Some(0.0),
        drift: false,
        motion_before: None,
        clearance: None,
        traffic: None,
        traffic_wait: 0.0,
        traffic_trail: Vec::new(),
        traffic_entered: false,
        queued_behind: 0,
        lane_scan: None,
        ghost: 0.0,
        hold_pos: false,
        build: false,
        dig: false,
        entrench: false,
        board: 0,
        rail: false,
    })
}
fn finite(p: Point) -> bool {
    p.x.is_finite() && p.z.is_finite()
}
fn validate(view: TerrainView<'_>, at: Point) -> Result<(), SpawnError> {
    let n = view
        .width
        .checked_mul(view.height)
        .ok_or(SpawnError::InvalidShape)?;
    if n == 0
        || n > i32::MAX as usize
        || view.flags.len() != n
        || view.heights.is_some_and(|h| h.len() != n)
    {
        return Err(SpawnError::InvalidShape);
    }
    if !finite(at) {
        return Err(SpawnError::InvalidPosition);
    }
    Ok(())
}
fn neighbor(view: TerrainView<'_>, cell: usize, dx: isize, dy: isize) -> Option<usize> {
    let x = (cell % view.width).checked_add_signed(dx)?;
    let y = (cell / view.width).checked_add_signed(dy)?;
    (x < view.width && y < view.height).then_some(y * view.width + x)
}
fn level(view: TerrainView<'_>, c: usize) -> i16 {
    view.heights.map_or(0, |h| i16::from(h[c]))
}
fn center(view: TerrainView<'_>, c: usize) -> Point {
    Point {
        x: (c % view.width) as f64 * CELL + CELL / 2.0,
        z: (c / view.width) as f64 * CELL + CELL / 2.0,
    }
}
