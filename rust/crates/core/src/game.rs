//! The game state as plain data: so far the parts the vision and fog passes, the movement block and crowd
//! separation read (`shared/sim.js` at the pin keeps all of it on one game object, `g`). Fields keep the
//! JavaScript's meaning; where a JavaScript field may be missing, the Rust one is an `Option` and the comment
//! says how the reference treats it. A number the reference reads without a fallback holds NaN for a missing
//! value, which compares and adds as `undefined` does.

use std::collections::BTreeMap;

use crate::grid::{Placed, UnitId};
use crate::rules::Rules;

/// Metres per map cell (`CELL`).
pub const CELL: f64 = 2.0;

// Cell flag bits (`shared/sim.js`, the line after `CELL`), the ones the passes read.
pub const MOVE: u16 = 1;
pub const SIGHT: u16 = 2;
pub const COVER: u16 = 4;
pub const TRENCH: u16 = 8;
pub const FORD: u16 = 16;
pub const WIRE: u16 = 32;
pub const VBLOCK: u16 = 64;
pub const ROAD: u16 = 128;
pub const MUD: u16 = 256;
pub const WOOD: u16 = 512;
pub const LAND: u16 = 1024;
pub const SHOAL: u16 = 2048;

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

/// A point on the ground (`{ x, z }`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub x: f64,
    pub z: f64,
}

/// A point and a heading (`{ x, z, rot }`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub x: f64,
    pub z: f64,
    pub rot: f64,
}

/// A unit stepping aside for another (`u.traffic`, `shared/local-traffic.js`).
#[derive(Clone, Debug, PartialEq)]
pub struct Traffic {
    /// Where it is heading now.
    pub goal: Point,
    /// Passing a stopped unit: the legs still to go, the first being `goal`. Empty otherwise.
    pub legs: Vec<Point>,
    pub passing: bool,
    /// The unit it gives way to.
    pub by: UnitId,
    /// When it gives up, in seconds of game time.
    pub until: f64,
}

/// The route's next one-lane stretch, as last looked for (`u.laneScan`).
#[derive(Clone, Debug, PartialEq)]
pub struct LaneScan {
    /// Which route it was looked for on: the unit's `path_gen` then.
    pub path: u64,
    /// The tick it is looked for again.
    pub until: f64,
    /// Where the stretch starts and ends, if there is one.
    pub lane: Option<(Point, Point)>,
}

/// A walled-in vehicle backing or pulling forward to where it can turn (`u.motionClearance`).
#[derive(Clone, Debug, PartialEq)]
pub struct Clearance {
    pub goal: Pose,
    pub reverse: bool,
    /// Which route it was started on: the unit's `path_gen` then.
    pub path: u64,
    /// The tick it gives up.
    pub until: f64,
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

    // Movement (the block in `step()`'s unit loop, `shared/movement.js` and `shared/local-traffic.js`).
    pub hp: f64,
    /// The route's waypoints still to reach.
    pub path: Vec<Point>,
    /// Which route array `path` is. The reference compares routes by identity (`u.laneScan.path !== u.path`);
    /// this goes up whenever the unit gets a new route array, and `LaneScan` and `Clearance` keep the value
    /// they were made with.
    pub path_gen: u64,
    /// The route's end it backs up to on a short move (`u.reverse`).
    pub reverse: Option<Point>,
    /// A slower pace it keeps on its route (`u.pace`; 0 or NaN for none).
    pub pace: f64,
    /// Has an attack order (`u.attackId` is truthy).
    pub attack: bool,
    pub retreating: bool,
    /// Seconds of its sprint ability left (`u.sprint`).
    pub sprint: f64,
    /// Suppression, 0 to 100 (`u.supp`).
    pub supp: f64,
    /// Where the last fire it took came from (`u.hitFrom`).
    pub hit_from: Option<Point>,
    /// Seconds it has made too little progress on its route (`u.stuck`).
    pub stuck: f64,
    /// Seconds before it may ask for a new route (`u.repath`).
    pub repath: f64,
    /// Where a world or move order is taking it (`u.worldGoal`).
    pub world_goal: Option<Point>,
    /// An attack-move's destination (`u.amove`).
    pub amove: Option<Point>,
    /// A vehicle's speed along its heading, negative backing up (`u.moveSpeed ?? 0`).
    pub move_speed: f64,
    /// Which way a vehicle is headed for (`u.travelDir`; missing falls back to `rot`).
    pub travel_dir: Option<f64>,
    /// Velocity over the last movement block, metres per second (`u.vx`, `u.vz`; missing: worked out from
    /// the route by the traffic rules).
    pub vx: Option<f64>,
    pub vz: Option<f64>,
    /// Drifting to a spot of its own, not ordered there (`u.drift` is truthy).
    pub drift: bool,
    /// Where it stood when its movement block began (`u.motionBefore`).
    pub motion_before: Option<Pose>,
    pub clearance: Option<Clearance>,
    pub traffic: Option<Traffic>,
    /// Seconds it has been held up by traffic (`u.trafficWait ?? 0`).
    pub traffic_wait: f64,
    /// Up to 20 places it passed on its route, to back up to (`u.trafficTrail ?? []`).
    pub traffic_trail: Vec<Pose>,
    /// Inside the crossing it is driving through (`u.trafficEntered`).
    pub traffic_entered: bool,
    /// The unit it waits behind, 0 for none (`u.queuedBehind`).
    pub queued_behind: UnitId,
    pub lane_scan: Option<LaneScan>,
    /// The tick until which it passes through friends (`u.ghost ?? 0`).
    pub ghost: f64,
    /// Ordered to hold its position, building, digging or entrenching (each truthy in the reference).
    pub hold_pos: bool,
    pub build: bool,
    pub dig: bool,
    pub entrench: bool,
    /// The carrier it is walking up to board, 0 for none (`u.board`).
    pub board: UnitId,
    /// On rails (`u.rail` is truthy).
    pub rail: bool,
}

impl Unit {
    pub fn at(&self) -> Point {
        Point {
            x: self.x,
            z: self.z,
        }
    }

    pub fn pose(&self) -> Pose {
        Pose {
            x: self.x,
            z: self.z,
            rot: self.rot,
        }
    }
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
    /// The enemy units this player's team sees, in the units' order (`p.visible`), which is ascending id.
    pub visible: Vec<UnitId>,
}

impl Player {
    /// Whether `id` is among the units this player sees (`p.visible.has(id)`).
    pub fn sees(&self, id: UnitId) -> bool {
        self.visible.binary_search(&id).is_ok()
    }
}

/// A smoke cloud. `id` stands for the JavaScript object's identity: the fog pass's kept sources remember
/// which clouds they saw by identity, not by value.
#[derive(Clone, Debug, PartialEq)]
pub struct Smoke {
    pub id: u32,
    /// Remaining lifetime (`t`); absent on vision-only fixture clouds.
    pub remaining: Option<f64>,
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
    /// Full hit points (`maxHp`), the cell's cover strength when whole.
    pub max_hp: Option<f64>,
}

/// A burnt-out vehicle (`g.wrecks`): cover for infantry once it has settled.
#[derive(Clone, Debug, PartialEq)]
pub struct Wreck {
    /// Settled obstruction cell (`w.c`), absent for unregistered or moving wrecks.
    pub cell: Option<usize>,
    pub x: f64,
    pub z: f64,
    /// Still sliding or tumbling (`motion` is truthy): no cover yet.
    pub moving: bool,
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
    /// Terrain letter per cell (`g.chars`).
    pub chars: Vec<u8>,
    /// The ground and object layers' letters (`g.ground`, `g.objects`); without them the ground is `chars`
    /// and there are no objects.
    pub ground: Option<Vec<u8>>,
    pub objects: Option<Vec<u8>>,
    /// Wear per cell, 0 to 1, as 32-bit floats (`g.wear`).
    pub wear: Option<Vec<f32>>,
    /// Hit points per cell, as 32-bit floats (`g.cellHp`).
    pub cell_hp: Vec<f32>,
    /// The way a dug trench faces, radians, NaN for map-drawn trenches (`g.trenchFront`, 32-bit floats).
    pub trench_front: Option<Vec<f32>>,
    /// Hit point multiplier per cell (`g.tough`; missing means 1).
    pub tough: Option<Vec<f64>>,
    /// How wet the ground is, 0 to 1, when the living ground runs (`g.wx?.wet`).
    pub wet: Option<f64>,
    /// The end-of-match reveal: everyone sees everything (`g.reveal`).
    pub reveal: bool,
    pub wrecks: Vec<Wreck>,
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

    /// The cell under a point as the reference indexes it, -1 off the map (`cellOf`).
    pub fn cell_index(&self, x: f64, z: f64) -> i64 {
        self.cell_of(x, z).map_or(-1, |c| c as i64)
    }

    /// Two players on one team; a player is allied with itself (`allied`, for players that exist).
    pub fn allied(&self, a: usize, b: usize) -> bool {
        match (self.players.get(a), self.players.get(b)) {
            (Some(a), Some(b)) => a.team == b.team,
            _ => false,
        }
    }

    /// What a cell physically is: its object if it has one, else its ground (`physicalWorldCell`).
    pub fn physical(&self, c: usize) -> u8 {
        let object = self.objects.as_ref().map_or(b'.', |o| o[c]);
        if object != b'.' {
            object
        } else {
            self.ground.as_ref().map_or(self.chars[c], |g| g[c])
        }
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
