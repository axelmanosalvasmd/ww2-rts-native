//! Rules data the vision and fog passes read: unit kinds, the vision numbers from `CFG` and `SUPPORT`, and
//! the weather kinds. For now they are loaded from the reference game's own tables (`harness/tables.mjs`);
//! the core will own them later (`docs/parity/interface.md`, IF-01).

/// What the vision and fog passes need to know about a unit kind (`UNITS` in `shared/sim.js`).
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

/// A weather state (`WEATHER` in `shared/weather.js`).
#[derive(Clone, Debug, PartialEq)]
pub struct WeatherKind {
    pub name: String,
    /// Sight multiplier for ground units and buildings.
    pub sight: f64,
    /// Rain is the living ground's own rain: its sight loss comes from the rain level, not from here.
    pub rains: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Rules {
    pub kinds: Vec<UnitKind>,
    pub vision: VisionRules,
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
