//! Pure legacy weather plans and public weather information.
//!
//! The caller supplies one recorded plan seed. The planner keeps the reference's local mulberry32
//! sequence and does not consume match random streams. Rain, wet ground, movement and visibility
//! integration remain with their existing systems; these helpers do not mutate `Game`.
use crate::{
    catalog::{Catalog, CatalogError, Value},
    math,
};
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum WeatherKind {
    Clear,
    Fog,
    Rain,
    Mud,
    Snow,
}
impl WeatherKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Clear => "clear",
            Self::Fog => "fog",
            Self::Rain => "rain",
            Self::Mud => "mud",
            Self::Snow => "snow",
        }
    }
}
impl TryFrom<&str> for WeatherKind {
    type Error = WeatherError;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "clear" => Ok(Self::Clear),
            "fog" => Ok(Self::Fog),
            "rain" => Ok(Self::Rain),
            "mud" => Ok(Self::Mud),
            "snow" => Ok(Self::Snow),
            _ => Err(WeatherError::UnknownKind),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mood {
    Warm,
    Dawn,
    Overcast,
    Snow,
    Dust,
}
impl Mood {
    pub fn name(self) -> &'static str {
        match self {
            Self::Warm => "warm",
            Self::Dawn => "dawn",
            Self::Overcast => "overcast",
            Self::Snow => "snow",
            Self::Dust => "dust",
        }
    }
    fn parse(value: &str) -> Option<Self> {
        match value {
            "warm" => Some(Self::Warm),
            "dawn" => Some(Self::Dawn),
            "overcast" => Some(Self::Overcast),
            "snow" => Some(Self::Snow),
            "dust" => Some(Self::Dust),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, Default)]
pub struct MapWeatherInfo<'a> {
    pub name: Option<&'a str>,
    pub mood: Option<&'a str>,
    pub weather: Option<&'a str>,
}

/// Explicit weather holds all match. Every unknown reference choice falls back to Map.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WeatherChoice {
    Map,
    Fixed(WeatherKind),
    Random,
}
impl WeatherChoice {
    pub fn from_reference(value: Option<&str>) -> Self {
        match value {
            Some("random") => Self::Random,
            Some(value) => WeatherKind::try_from(value)
                .map(Self::Fixed)
                .unwrap_or(Self::Map),
            None => Self::Map,
        }
    }
}

/// `at` is a simulation tick, including zero when there is no planned change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WeatherPlan {
    pub now: WeatherKind,
    pub next: Option<WeatherKind>,
    pub at: u32,
}
impl WeatherPlan {
    pub fn fixed(now: WeatherKind) -> Self {
        Self {
            now,
            next: None,
            at: 0,
        }
    }
    /// Apply at most one change. Calling again after the transition leaves the state unchanged.
    pub fn step(&mut self, tick: u64) -> bool {
        if let Some(next) = self.next
            && tick >= u64::from(self.at)
        {
            *self = Self::fixed(next);
            return true;
        }
        false
    }
}

/// Equivalent to `[now]` or `[now, next, seconds]`, without exposing a JSON contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WeatherRow {
    Current(WeatherKind),
    Changing {
        now: WeatherKind,
        next: WeatherKind,
        seconds: u32,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WeatherError {
    Catalog(CatalogError),
    UnknownKind,
    InvalidMapShape,
}
impl From<CatalogError> for WeatherError {
    fn from(e: CatalogError) -> Self {
        Self::Catalog(e)
    }
}
impl std::fmt::Display for WeatherError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Catalog(e) => e.fmt(f),
            Self::UnknownKind => f.write_str("unknown weather kind"),
            Self::InvalidMapShape => f.write_str(
                "road mask requires nonempty rectangular ASCII rows matching width and height",
            ),
        }
    }
}
impl std::error::Error for WeatherError {}

/// Metadata names are optional strings. Unknown mood/weather names use the reference fallback.
/// ASCII-insensitive substring matching reproduces the reference's ASCII regex alternatives.
pub fn map_mood(map: MapWeatherInfo<'_>, key: Option<&str>) -> Mood {
    if let Some(mood) = map.mood.and_then(Mood::parse) {
        return mood;
    }
    let text = format!("{} {}", map.name.unwrap_or(""), key.unwrap_or("")).to_ascii_lowercase();
    for (mood, words) in [
        (
            Mood::Snow,
            &["ardennes", "bastogne", "bulge", "winter", "snow"][..],
        ),
        (
            Mood::Dust,
            &[
                "kasserine",
                "desert",
                "tobruk",
                "alamein",
                "africa",
                "tunis",
            ][..],
        ),
        (
            Mood::Dawn,
            &["pegasus", "polder", "river", "canal", "marsh"][..],
        ),
        (
            Mood::Overcast,
            &[
                "bocage",
                "cassino",
                "stalingrad",
                "seawall",
                "rain",
                "storm",
            ][..],
        ),
    ] {
        if words.iter().any(|word| text.contains(word)) {
            return mood;
        }
    }
    Mood::Warm
}

#[derive(Clone, Copy, Debug)]
struct Effects {
    sight: f64,
    inf: f64,
    veh: f64,
    off_road: f64,
}
#[derive(Debug)]
pub struct WeatherRules {
    order: Vec<WeatherKind>,
    effects: [Effects; 5],
    tps: u32,
    warn_ticks: u32,
}
impl WeatherRules {
    pub fn compiled() -> Result<&'static Self, WeatherError> {
        static RULES: OnceLock<Result<WeatherRules, WeatherError>> = OnceLock::new();
        RULES
            .get_or_init(|| Self::from_catalog(Catalog::compiled()?))
            .as_ref()
            .map_err(Clone::clone)
    }
    fn from_catalog(catalog: &Catalog) -> Result<Self, WeatherError> {
        let number = |value: Option<&Value>| {
            value
                .and_then(Value::number)
                .ok_or_else(|| CatalogError("missing weather number".into()))
        };
        let table = catalog.table("WEATHER")?;
        let mut effects = [Effects {
            sight: 0.0,
            inf: 0.0,
            veh: 0.0,
            off_road: 0.0,
        }; 5];
        let order = catalog
            .table("WEATHER_KINDS")?
            .array()
            .ok_or_else(|| CatalogError("missing weather order".into()))?
            .iter()
            .map(|value| WeatherKind::try_from(value.string().ok_or(WeatherError::UnknownKind)?))
            .collect::<Result<Vec<_>, _>>()?;
        let mut seen = [false; 5];
        for &kind in &order {
            if seen[kind as usize] {
                return Err(CatalogError("duplicate weather kind".into()).into());
            }
            seen[kind as usize] = true;
            let def = table
                .get(kind.name())
                .ok_or_else(|| CatalogError("missing weather definition".into()))?;
            let e = Effects {
                sight: number(def.get("sight"))?,
                inf: number(def.get("inf"))?,
                veh: number(def.get("veh"))?,
                off_road: number(def.get("offRoad"))?,
            };
            if [e.sight, e.inf, e.veh, e.off_road]
                .iter()
                .any(|n| !n.is_finite() || *n <= 0.0)
            {
                return Err(CatalogError("invalid weather multiplier".into()).into());
            }
            effects[kind as usize] = e;
        }
        if !seen.iter().all(|v| *v) {
            return Err(CatalogError("incomplete weather kinds".into()).into());
        }
        let tps = 1.0 / number(Some(catalog.table("TICK")?))?;
        let warn = number(Some(catalog.table("WEATHER_WARN")?))?;
        if !tps.is_finite()
            || tps <= 0.0
            || tps > f64::from(u32::MAX)
            || tps.fract() != 0.0
            || !warn.is_finite()
            || warn < 0.0
            || warn.fract() != 0.0
            || warn > f64::from(u32::MAX)
        {
            return Err(CatalogError("invalid weather timing constants".into()).into());
        }
        let tps = tps as u32;
        let warn_ticks = (warn as u32)
            .checked_mul(tps)
            .ok_or_else(|| CatalogError("weather warning interval overflow".into()))?;
        Ok(Self {
            order,
            effects,
            tps,
            warn_ticks,
        })
    }
    /// Map defaults, converted from the reference's seconds to simulation ticks.
    pub fn map_weather(&self, map: MapWeatherInfo<'_>, key: Option<&str>) -> WeatherPlan {
        if let Some(kind) = map
            .weather
            .and_then(|name| WeatherKind::try_from(name).ok())
        {
            return WeatherPlan::fixed(kind);
        }
        match map_mood(map, key) {
            Mood::Snow => WeatherPlan::fixed(WeatherKind::Snow),
            Mood::Dawn => WeatherPlan {
                now: WeatherKind::Fog,
                next: Some(WeatherKind::Clear),
                at: 240 * self.tps,
            },
            _ => WeatherPlan::fixed(WeatherKind::Clear),
        }
    }
    pub fn plan_weather(
        &self,
        choice: WeatherChoice,
        map: MapWeatherInfo<'_>,
        key: Option<&str>,
        seed: u32,
    ) -> WeatherPlan {
        if let WeatherChoice::Fixed(kind) = choice {
            return WeatherPlan::fixed(kind);
        }
        if choice == WeatherChoice::Map {
            return self.map_weather(map, key);
        }
        let mut random = PlanRandom(seed);
        let now = self.order[(random.next() * self.order.len() as f64) as usize];
        let turn = random.next() < 0.5;
        let when = random.next();
        let (next, seconds) = match (turn, now) {
            (true, WeatherKind::Fog) => (WeatherKind::Clear, 210 + (when * 60.0) as u32),
            (true, WeatherKind::Rain) => (WeatherKind::Mud, 300 + (when * 120.0) as u32),
            _ => return WeatherPlan::fixed(now),
        };
        WeatherPlan {
            now,
            next: Some(next),
            at: seconds * self.tps,
        }
    }
    pub fn weather_row(&self, plan: Option<&WeatherPlan>, tick: u64) -> WeatherRow {
        let Some(plan) = plan else {
            return WeatherRow::Current(WeatherKind::Clear);
        };
        if let Some(next) = plan.next {
            let left = u64::from(plan.at).saturating_sub(tick);
            if left <= u64::from(self.warn_ticks) {
                return WeatherRow::Changing {
                    now: plan.now,
                    next,
                    seconds: left.div_ceil(u64::from(self.tps)) as u32,
                };
            }
        }
        WeatherRow::Current(plan.now)
    }
    pub fn weather_effects(&self, kind: Option<WeatherKind>) -> Vec<String> {
        let e = self.effects[kind.unwrap_or(WeatherKind::Clear) as usize];
        let mut out = Vec::new();
        for (label, value, suffix) in [
            ("sight", e.sight, ""),
            ("infantry", e.inf, ""),
            ("vehicles", e.veh, ""),
            ("vehicles", e.off_road, " off roads"),
        ] {
            if value < 1.0 {
                out.push(format!(
                    "{label} -{}%{suffix}",
                    math::round((1.0 - value) * 100.0) as i64
                ));
            }
        }
        out
    }
    pub fn ai_caution(&self, plan: Option<&WeatherPlan>) -> f64 {
        let e = self.effects[plan.map_or(WeatherKind::Clear, |p| p.now) as usize];
        1.0 / (e.sight * math::pow(math::min(1.0, e.veh * e.off_road), 0.5))
    }
}

// Exactly the local three-draw sequence in shared/weather.js, with wrapping 32-bit arithmetic.
struct PlanRandom(u32);
impl PlanRandom {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x6d2b79f5);
        let mut t = (self.0 ^ (self.0 >> 15)).wrapping_mul(self.0 | 1);
        t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 61));
        f64::from(t ^ (t >> 14)) / 4_294_967_296.0
    }
}

/// A rectangular ASCII terrain view. Non-ASCII rows and inconsistent dimensions are rejected.
pub struct RoadMap<'a> {
    width: usize,
    height: usize,
    rows: &'a [&'a str],
}
impl<'a> RoadMap<'a> {
    pub fn new(width: usize, height: usize, rows: &'a [&'a str]) -> Result<Self, WeatherError> {
        if width == 0
            || height == 0
            || width.checked_mul(height).is_none()
            || rows.len() != height
            || rows.iter().any(|r| !r.is_ascii() || r.len() != width)
        {
            return Err(WeatherError::InvalidMapShape);
        }
        Ok(Self {
            width,
            height,
            rows,
        })
    }
}
/// Presentation roads: authored roads/bridges and dry open ground beside a house.
pub fn road_mask(map: &RoadMap<'_>) -> Vec<u8> {
    let mut out = vec![0; map.width * map.height];
    for y in 0..map.height {
        for x in 0..map.width {
            let ch = map.rows[y].as_bytes()[x];
            if ch == b'=' || ch == b'D' {
                out[y * map.width + x] = 1;
                continue;
            }
            if ch != b'.' {
                continue;
            }
            let mut town = false;
            let mut wet = false;
            for ny in y.saturating_sub(1)..=(y + 1).min(map.height - 1) {
                for nx in x.saturating_sub(1)..=(x + 1).min(map.width - 1) {
                    match map.rows[ny].as_bytes()[nx] {
                        b'B' => town = true,
                        b'W' | b'F' | b'=' => wet = true,
                        _ => {}
                    }
                }
            }
            if town && !wet {
                out[y * map.width + x] = 1;
            }
        }
    }
    out
}
