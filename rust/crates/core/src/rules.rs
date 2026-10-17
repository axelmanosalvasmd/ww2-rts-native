//! Typed rules used by the movement, vision and fog passes. `Rules::compiled` constructs them
//! from the embedded catalog, with no caller JSON or runtime file access. The independent
//! harness loader remains a fixture oracle.
//!
//! `RULES_HASH` still covers only formation constants. Compiling the catalog and typed rules
//! does not extend that public hash; its scope changes only with a separate interface agreement.

use crate::formation;

#[path = "rules_catalog.rs"]
mod compiled;

/// FNV-1a 64 of the rules data compiled into the core. The C interface returns it from
/// `ww2_rules_hash`, and the Rust server is to send the same number in `lobby.rules`
/// (`docs/contracts/rust-unreal-abi.md`). Today the hashed data consists of the formation constants
/// only: each name in `formation::SHAPES` followed by a zero byte, then the little-endian bits of
/// each number in `formation::RULE_NUMBERS`. The tables below and the compiled catalog are not
/// covered. The catalog and typed rules now have compiled constructors, but remain outside this
/// hash until a separate interface agreement changes its scope.
pub const RULES_HASH: u64 = {
    let mut hash = 0xCBF2_9CE4_8422_2325;
    let mut i = 0;
    while i < formation::SHAPES.len() {
        hash = fnv1a(hash, formation::SHAPES[i].as_bytes());
        hash = fnv1a(hash, &[0]);
        i += 1;
    }
    let mut i = 0;
    while i < formation::RULE_NUMBERS.len() {
        hash = fnv1a(hash, &formation::RULE_NUMBERS[i].to_bits().to_le_bytes());
        i += 1;
    }
    hash
};

const fn fnv1a(mut hash: u64, bytes: &[u8]) -> u64 {
    let mut i = 0;
    while i < bytes.len() {
        hash ^= bytes[i] as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
        i += 1;
    }
    hash
}

/// What the ported passes need to know about a unit kind (`UNITS` in `shared/sim.js`).
#[derive(Clone, Debug, PartialEq)]
pub struct UnitKind {
    pub name: String,
    /// Metres of sight before weather, height and houses.
    pub vision: f64,
    /// A Scout Tower: sees only a few metres until it is finished.
    pub lookout: bool,
    /// Hidden while it stands still and holds fire (snipers, commandos).
    pub camo: bool,
    pub structure: bool,
    pub building: bool,
    pub infantry: bool,
    /// Half a ship's length, 0 for everything else: a ship is seen along its whole hull.
    pub hull: f64,
    /// Top speed on open dry ground, metres per second.
    pub speed: f64,
    /// Size in metres: the hull's half length is 0.8 of it (`bodyRadius`).
    pub radius: f64,
    pub naval: bool,
    /// A ship kept off the shoals.
    pub deep: bool,
    pub air: bool,
    /// Runs on rails (the train).
    pub rail: bool,
    /// Has a weapon (`def.w`): a building without one never reaches the movement block.
    pub weapon: bool,
    /// The speed factor of the kind's sprint ability, if it has one (`def.ab.speed`).
    pub ability_speed: Option<f64>,
    /// How a ground vehicle drives (`movementProfile`); `None` for everything else.
    pub vehicle: Option<VehicleProfile>,
}

impl UnitKind {
    /// A vehicle on the ground: no infantry, structure, plane or ship (`isGroundVehicle`).
    pub fn ground_vehicle(&self) -> bool {
        !self.infantry && !self.structure && !self.air && !self.naval
    }

    /// The hull's half length (`bodyRadius`).
    pub fn body_radius(&self) -> f64 {
        self.radius * 0.8
    }

    /// The terrain bits that stop this kind (`blockOf`): ships keep to water (deep ones off the shoals),
    /// vehicles keep out of water and off tank traps, infantry out of water.
    pub fn block(&self) -> u16 {
        use crate::game::{LAND, MOVE, SHOAL, VBLOCK};
        if self.deep {
            LAND | SHOAL
        } else if self.naval {
            LAND
        } else if !self.infantry {
            MOVE | VBLOCK
        } else {
            MOVE
        }
    }
}

/// A ground vehicle's motion (`VEHICLE_PROFILES` in `shared/vehicle-motion.js`, or its fallback).
#[derive(Clone, Debug, PartialEq)]
pub struct VehicleProfile {
    /// Metres per second squared, forward.
    pub acceleration: f64,
    pub reverse_acceleration: f64,
    pub braking: f64,
    /// Reverse speed as a share of top speed.
    pub reverse_speed: f64,
    /// Radians per second.
    pub hull_turn: f64,
    /// Tracked suspension uses a different visual blend rate from wheeled suspension.
    pub tracked: bool,
}

/// The `CFG` and `SUPPORT` numbers the passes read.
#[derive(Clone, Debug, PartialEq)]
pub struct VisionRules {
    /// A still, quiet camouflaged unit is seen only this close (`CFG.camoRange`).
    pub camo_range: f64,
    /// A vehicle trailing dust is seen this many times farther (`CFG.dustSeen`).
    pub dust_seen: f64,
    /// How far an airborne plane is seen from the ground (`CFG.air.seeRange`).
    pub air_see_range: f64,
    /// Infantry down in a trench and quiet is seen only this close (`CFG.trench.hide`).
    pub trench_hide: f64,
    /// Smoke blocks sight only on lines at least this long (`CFG.smokeSight`).
    pub smoke_sight: f64,
    /// Metres per height level (`CFG.levelHeight`).
    pub level_height: f64,
    /// Eye height above the ground (`CFG.eye`).
    pub eye: f64,
    /// The lowest height level (`CFG.minLevel`).
    pub min_level: i8,
    /// Sight goes this many woods cells deep and no further (`CFG.wood.sight`).
    pub wood_sight: f64,
    /// Extra sight per height level, as a fraction (`CFG.highGroundVision`).
    pub high_ground_vision: f64,
    /// Sight multiplier for a squad inside a house (`CFG.garrisonVision`).
    pub garrison_vision: f64,
    /// Per house kind, a sight multiplier that replaces `garrison_vision` (`CFG.houses[k].vision`).
    pub house_vision: Vec<Option<f64>>,
    /// Sight lost at full rain, as a fraction (`CFG.weather.sight`).
    pub rain_sight: f64,
    /// A recon flight's corridor (`SUPPORT.recon.len`, `SUPPORT.recon.width`).
    pub recon_len: f64,
    pub recon_width: f64,
}

/// Local traffic between units (`TRAFFIC` in `shared/local-traffic.js`).
#[derive(Clone, Debug, PartialEq)]
pub struct TrafficRules {
    /// Seconds ahead a unit looks for someone it would run into.
    pub horizon: f64,
    /// Seconds blocked before a unit steps aside.
    pub wait: f64,
    /// Seconds blocked before it asks for a new route.
    pub replan: f64,
    /// Seconds of waiting per step of priority.
    pub age: f64,
    /// Seconds a sidestep lasts at most.
    pub max_yield: f64,
    /// Seconds boxed in by a friend before passing through friends.
    pub ghost: f64,
    /// Seconds a unit passes through friends.
    pub ghost_time: f64,
}

/// One-lane stretches (`LANE` in `shared/local-traffic.js`).
#[derive(Clone, Debug, PartialEq)]
pub struct LaneRules {
    /// Metres of route a hull looks along.
    pub ahead: f64,
    /// Room beyond two hulls and the passing gap.
    pub margin: f64,
    /// How far short of a stretch a hull stops, beyond its own length.
    pub hold: f64,
}

/// The `CFG` numbers the movement block and crowd separation read.
#[derive(Clone, Debug, PartialEq)]
pub struct MovementRules {
    /// Seconds per tick (`TICK`).
    pub tick: f64,
    /// Speed factor while retreating (`CFG.retreatSpeed`).
    pub retreat_speed: f64,
    /// Speed lost per unit of grade, infantry and vehicles (`CFG.slope`).
    pub slope: [f64; 2],
    /// A ford's speed, shallow to deep (`CFG.fordSpeed`).
    pub ford_speed: [f64; 2],
    /// Wet ground's effect on fords and on open ground and mud (`CFG.weather.wetFord`, `wetGround`).
    pub wet_ford: f64,
    pub wet_ground: f64,
    /// Infantry speed through wire and along trenches (`CFG.wireSpeed`, `CFG.trench.speed`).
    pub wire_speed: f64,
    pub trench_speed: f64,
    /// Suppression a squad in a trench loses per second (`CFG.trench.rally`).
    pub trench_rally: f64,
    /// A dug trench protects against fire within this cosine of its front (`CFG.trench.arc`).
    pub trench_arc: f64,
    /// Vehicle speed in woods and the woods' share of cover (`CFG.wood.speed`, `CFG.wood.cover`).
    pub wood_speed: f64,
    pub wood_cover: f64,
    /// Vehicle speed on an unbroken road (`CFG.roadSpeed`).
    pub road_speed: f64,
    /// Vehicle speed in mud, shallow to deep (`CFG.mudSpeed`).
    pub mud_speed: [f64; 2],
    /// Speed lost on fully churned open ground (`CFG.churn`).
    pub churn: f64,
    /// Speed factor while backing up (`CFG.behavior.reverseSpeed`).
    pub reverse_speed: f64,
    /// Hit points per house kind (`CFG.houses[k].hp`).
    pub house_hp: Vec<f64>,
    /// Hit points of the other structure letters (`CFG.terrainHp`), by terrain letter.
    pub terrain_hp: Vec<(u8, f64)>,
    /// How far a hull's arc may stray off a straight line while turning (`CORNER_BULGE`).
    pub corner_bulge: f64,
    pub traffic: TrafficRules,
    pub lane: LaneRules,
}

impl MovementRules {
    /// A terrain letter's hit points (`CFG.terrainHp[ch]`).
    pub fn terrain_hp(&self, ch: u8) -> Option<f64> {
        self.terrain_hp
            .iter()
            .find(|(c, _)| *c == ch)
            .map(|&(_, hp)| hp)
    }
}

/// A weather state (`WEATHER` in `shared/weather.js`).
#[derive(Clone, Debug, PartialEq)]
pub struct WeatherKind {
    pub name: String,
    /// Sight multiplier for ground units and buildings.
    pub sight: f64,
    /// Rain is the living ground's own rain: its sight loss comes from the rain level, not from here.
    pub rains: bool,
    /// Speed multipliers for infantry and for vehicles (`weatherSpeed`).
    pub inf: f64,
    pub veh: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Rules {
    pub kinds: Vec<UnitKind>,
    pub vision: VisionRules,
    pub movement: MovementRules,
    pub weather: Vec<WeatherKind>,
}

impl Rules {
    /// The index of a unit kind by its name in `UNITS`.
    pub fn kind(&self, name: &str) -> Option<usize> {
        self.kinds.iter().position(|k| k.name == name)
    }

    /// The index of a weather state by its name in `WEATHER`.
    pub fn weather_kind(&self, name: &str) -> Option<usize> {
        self.weather.iter().position(|w| w.name == name)
    }
}
