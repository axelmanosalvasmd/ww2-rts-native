//! The game state as plain data: so far the parts the vision and fog passes read (`shared/sim.js` at the
//! pin keeps all of it on one game object, `g`). Fields keep the JavaScript's meaning; where a JavaScript
//! field may be missing, the Rust one is an `Option` and the comment says how the reference treats it.

use std::collections::BTreeMap;

use crate::grid::{Placed, UnitId};
use crate::rules::Rules;

/// Metres per map cell (`CELL`).
pub const CELL: f64 = 2.0;

// Cell flag bits (`shared/sim.js`, the line after `CELL`), the ones the passes read.
pub const MOVE: u16 = 1;
pub const SIGHT: u16 = 2;
pub const TRENCH: u16 = 8;
pub const WOOD: u16 = 512;

pub type Team = i32;

/// `g.mode.kind`. Conquest has no mode object, so it is `None` in `Game::mode`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModeKind {
    Annihilation,
    Assault,
    Classic,
    Horde,
    Tutorial,
    World,
}

/// `u.air.state` for a plane.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaneState {
    Base,
    Rearm,
    Out,
    Station,
    Home,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Unit {
    pub id: UnitId,
    /// Index into `Rules::kinds`.
    pub kind: usize,
    /// The owning player's slot, an index into `Game::players`.
    pub owner: usize,
    pub x: f64,
    pub z: f64,
    /// Heading in radians (`u.rot ?? 0`).
    pub rot: f64,
    /// The house cell a squad holds (`u.garrison`, -1 for none).
    pub garrison: Option<usize>,
    /// Riding inside a carrier (`u.riding` is truthy).
    pub riding: bool,
    /// A plane and its state (`u.air`).
    pub air: Option<PlaneState>,
    /// Seconds standing still (`u.still`; missing compares false).
    pub still: Option<f64>,
    /// Tick of its last shot (`u.shotAt ?? -1e9`).
    pub shot_at: Option<f64>,
    /// Tick it last raised dust (`u.dust ?? -1e9`).
    pub dust: Option<f64>,
    /// How finished a structure is, 0 to 1 (`u.built`; missing compares false).
    pub built: Option<f64>,
    /// A structure's footprint cells (`u.cells`).
    pub cells: Option<Vec<u32>>,
    /// Everything but infantry, for the spatial index's heavy buckets (`setHeavy` in `shared/sim.js`).
    /// Copied from the unit's kind when the unit is made.
    pub heavy: bool,
}

impl Placed for Unit {
    fn x(&self) -> f64 {
        self.x
    }
    fn z(&self) -> f64 {
        self.z
    }
    fn owner(&self) -> i32 {
        self.owner as i32
    }
    fn heavy(&self) -> bool {
        self.heavy
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Player {
    pub slot: usize,
    pub team: Team,
    /// The enemy units this player's team sees, in the units' order (`p.visible`).
    pub visible: Vec<UnitId>,
}

/// A smoke cloud. `id` stands for the JavaScript object's identity: the fog pass's kept sources remember
/// which clouds they saw by identity, not by value.
#[derive(Clone, Debug, PartialEq)]
pub struct Smoke {
    pub id: u32,
    pub x: f64,
    pub z: f64,
    pub r: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StrikeKind {
    Recon,
    /// Every other off-map support: the passes only look at recon flights.
    Other,
}

/// An off-map support strike (`g.strikes`).
#[derive(Clone, Debug, PartialEq)]
pub struct Strike {
    pub live: bool,
    pub kind: StrikeKind,
    pub owner: usize,
    pub x: f64,
    pub z: f64,
    /// Direction of the corridor's long side, radians.
    pub dir: f64,
}

/// One cell of a structure (`g.structuralCells`): a building is aimed at by its standing cells.
#[derive(Clone, Debug, PartialEq)]
pub struct Section {
    /// `state === 'failed'`.
    pub failed: bool,
    pub hp: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Game {
    pub w: usize,
    pub h: usize,
    /// Cell flag bits, row by row.
    pub flags: Vec<u16>,
    /// Height level per cell; `None` on a flat map.
    pub height: Option<Vec<i8>>,
    /// House kind per cell, an index into `VisionRules::house_vision`.
    pub house: Vec<u8>,
    pub tick: i64,
    /// The tick of the last vision pass (`g.visionTick`).
    pub vision_tick: Option<i64>,
    /// Goes up with every terrain change (`g.terrainVersion ?? 0`).
    pub terrain_version: u32,
    pub mode: Option<ModeKind>,
    pub naval: bool,
    /// The weather now, an index into `Rules::weather` (`g.weather?.now`).
    pub weather: Option<usize>,
    /// The living ground's rain, 0 to 1 (`g.wx?.rain ?? 0`).
    pub rain: f64,
    pub players: Vec<Player>,
    /// Every unit by id. The JavaScript keeps them in a Map in insertion order; ids only grow and a unit is
    /// never put back, so ascending id is that order.
    pub units: BTreeMap<UnitId, Unit>,
    pub smokes: Vec<Smoke>,
    pub strikes: Vec<Strike>,
    /// Structure cells by cell index.
    pub sections: BTreeMap<u32, Section>,
}

impl Game {
    pub fn team_of(&self, u: &Unit) -> Team {
        self.players[u.owner].team
    }

    /// The cell under a point, or `None` off the map (`cellOf`).
    pub fn cell_of(&self, x: f64, z: f64) -> Option<usize> {
        let (cx, cy) = ((x / CELL).floor(), (z / CELL).floor());
        if cx < 0.0 || cy < 0.0 || cx >= self.w as f64 || cy >= self.h as f64 {
            return None;
        }
        Some(cy as usize * self.w + cx as usize)
    }

    /// A point's cell flags; off the map counts as solid (`flagsAt`).
    pub fn flags_at(&self, x: f64, z: f64) -> u16 {
        self.cell_of(x, z).map_or(MOVE | SIGHT, |c| self.flags[c])
    }

    /// The height level under a point, 0 off the map or on a flat map (`levelAt`).
    pub fn level_at(&self, x: f64, z: f64) -> i8 {
        match (&self.height, self.cell_of(x, z)) {
            (Some(height), Some(c)) => height[c],
            _ => 0,
        }
    }

    /// Weather's sight multiplier for ground units and buildings (`sightMul` in `shared/weather.js`).
    pub fn sight_mul(&self, rules: &Rules) -> f64 {
        match self.weather.map(|k| &rules.weather[k]) {
            Some(w) if !w.rains => w.sight,
            _ => 1.0,
        }
    }
}
