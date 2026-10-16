//! Construct the hot-path rule structs from the catalog. Source order is explicit in its arrays.
use super::{
    LaneRules, MovementRules, Rules, TrafficRules, UnitKind, VehicleProfile, VisionRules,
    WeatherKind,
};
use crate::{
    catalog::{Catalog, CatalogError, Value},
    game,
};

type Result<T> = std::result::Result<T, CatalogError>;

fn field<'a>(value: &'a Value, key: &str) -> Result<&'a Value> {
    value
        .get(key)
        .ok_or_else(|| CatalogError(format!("missing rules field: {key}")))
}
fn number(value: &Value) -> Result<f64> {
    value
        .number()
        .ok_or_else(|| CatalogError("expected a rules number".into()))
}
fn num(value: &Value, key: &str) -> Result<f64> {
    number(field(value, key)?)
}
fn optional_number(value: &Value, key: &str) -> Result<Option<f64>> {
    match value.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => number(value).map(Some),
    }
}
fn array(value: &Value) -> Result<&[Value]> {
    value
        .array()
        .ok_or_else(|| CatalogError("expected a rules array".into()))
}
fn name(value: &Value) -> Result<&str> {
    value
        .string()
        .ok_or_else(|| CatalogError("expected a rules name".into()))
}
fn flag(value: &Value, key: &str) -> bool {
    match value.get(key) {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => *n != 0.0 && !n.is_nan(),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Object(_) | Value::Array(_)) => true,
    }
}
fn pair(value: &Value, key: &str) -> Result<[f64; 2]> {
    let [a, b] = array(field(value, key)?)? else {
        return Err(CatalogError(format!("{key}: expected two rules numbers")));
    };
    Ok([number(a)?, number(b)?])
}
fn profile(value: &Value) -> Result<Option<VehicleProfile>> {
    if matches!(value, Value::Null) {
        return Ok(None);
    }
    Ok(Some(VehicleProfile {
        acceleration: num(value, "acceleration")?,
        reverse_acceleration: num(value, "reverseAcceleration")?,
        braking: num(value, "braking")?,
        reverse_speed: num(value, "reverseSpeed")?,
        hull_turn: num(value, "hullTurn")?,
        tracked: flag(value, "tracked"),
    }))
}

impl Rules {
    /// Construct the movement, vision and fog rules without loading files or caller-provided JSON.
    pub fn compiled() -> Result<Self> {
        Self::from_catalog(Catalog::compiled()?)
    }

    /// Construct typed rules from a catalog, preserving its explicit unit and weather order.
    /// Vehicle profiles include the source helper's fallback resolved when the catalog is recorded.
    /// Cell size and terrain bits must still match the core's constants.
    pub fn from_catalog(catalog: &Catalog) -> Result<Self> {
        if number(catalog.table("CELL")?)? != game::CELL {
            return Err(CatalogError("catalog CELL differs from core CELL".into()));
        }
        for (key, bits) in [
            ("MOVE", game::MOVE),
            ("SIGHT", game::SIGHT),
            ("COVER", game::COVER),
            ("TRENCH", game::TRENCH),
            ("FORD", game::FORD),
            ("WIRE", game::WIRE),
            ("VBLOCK", game::VBLOCK),
            ("ROAD", game::ROAD),
            ("MUD", game::MUD),
            ("WOOD", game::WOOD),
            ("LAND", game::LAND),
            ("SHOAL", game::SHOAL),
        ] {
            if number(catalog.table(key)?)? != f64::from(bits) {
                return Err(CatalogError(format!(
                    "catalog {key} differs from core terrain bit"
                )));
            }
        }
        let profiles = catalog.table("MOVEMENT_PROFILES")?;
        let kinds = array(catalog.table("UNIT_TYPES")?)?
            .iter()
            .map(|entry| {
                let name = name(entry)?;
                let def = catalog.unit(name)?;
                Ok(UnitKind {
                    name: name.to_owned(),
                    vision: num(def, "vision")?,
                    lookout: flag(def, "lookout"),
                    camo: flag(def, "camo"),
                    structure: flag(def, "structure"),
                    building: flag(def, "building"),
                    infantry: flag(def, "infantry"),
                    hull: optional_number(def, "hull")?.unwrap_or(0.0),
                    speed: num(def, "speed")?,
                    radius: num(def, "radius")?,
                    naval: flag(def, "naval"),
                    deep: flag(def, "deep"),
                    air: flag(def, "air"),
                    rail: flag(def, "rail"),
                    weapon: flag(def, "w"),
                    ability_speed: def
                        .get("ab")
                        .map(|ab| optional_number(ab, "speed"))
                        .transpose()?
                        .flatten(),
                    vehicle: profile(field(profiles, name)?)?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let cfg = catalog.table("CFG")?;
        let houses = array(field(cfg, "houses")?)?;
        let wood = field(cfg, "wood")?;
        let trench = field(cfg, "trench")?;
        let weather = field(cfg, "weather")?;
        let recon = catalog.support("recon")?;
        let min_level = num(cfg, "minLevel")?;
        if min_level.fract() != 0.0 || !(-128.0..=127.0).contains(&min_level) {
            return Err(CatalogError("minLevel is not an i8".into()));
        }
        let vision = VisionRules {
            camo_range: num(cfg, "camoRange")?,
            dust_seen: num(cfg, "dustSeen")?,
            air_see_range: num(field(cfg, "air")?, "seeRange")?,
            trench_hide: num(trench, "hide")?,
            smoke_sight: num(cfg, "smokeSight")?,
            level_height: num(cfg, "levelHeight")?,
            eye: num(cfg, "eye")?,
            min_level: min_level as i8,
            wood_sight: num(wood, "sight")?,
            high_ground_vision: num(cfg, "highGroundVision")?,
            garrison_vision: num(cfg, "garrisonVision")?,
            house_vision: houses
                .iter()
                .map(|house| optional_number(house, "vision"))
                .collect::<Result<_>>()?,
            rain_sight: num(weather, "sight")?,
            recon_len: num(recon, "len")?,
            recon_width: num(recon, "width")?,
        };
        let traffic = catalog.table("TRAFFIC")?;
        let lane = catalog.table("LANE")?;
        let terrain_hp = field(cfg, "terrainHp")?
            .object()
            .ok_or_else(|| CatalogError("terrainHp must be an object".into()))?
            .iter()
            .map(|(letter, hp)| {
                let [ch] = letter.as_bytes() else {
                    return Err(CatalogError(format!("invalid terrainHp letter: {letter}")));
                };
                Ok((*ch, number(hp)?))
            })
            .collect::<Result<_>>()?;
        let movement = MovementRules {
            tick: number(catalog.table("TICK")?)?,
            retreat_speed: num(cfg, "retreatSpeed")?,
            slope: pair(cfg, "slope")?,
            ford_speed: pair(cfg, "fordSpeed")?,
            wet_ford: num(weather, "wetFord")?,
            wet_ground: num(weather, "wetGround")?,
            wire_speed: num(cfg, "wireSpeed")?,
            trench_speed: num(trench, "speed")?,
            trench_rally: num(trench, "rally")?,
            trench_arc: num(trench, "arc")?,
            wood_speed: num(wood, "speed")?,
            wood_cover: num(wood, "cover")?,
            road_speed: num(cfg, "roadSpeed")?,
            mud_speed: pair(cfg, "mudSpeed")?,
            churn: num(cfg, "churn")?,
            reverse_speed: num(field(cfg, "behavior")?, "reverseSpeed")?,
            house_hp: houses
                .iter()
                .map(|house| num(house, "hp"))
                .collect::<Result<_>>()?,
            terrain_hp,
            corner_bulge: number(catalog.table("CORNER_BULGE")?)?,
            traffic: TrafficRules {
                horizon: num(traffic, "horizon")?,
                wait: num(traffic, "wait")?,
                replan: num(traffic, "replan")?,
                age: num(traffic, "age")?,
                max_yield: num(traffic, "maxYield")?,
                ghost: num(traffic, "ghost")?,
                ghost_time: num(traffic, "ghostTime")?,
            },
            lane: LaneRules {
                ahead: num(lane, "ahead")?,
                margin: num(lane, "margin")?,
                hold: num(lane, "hold")?,
            },
        };
        let weather = array(catalog.table("WEATHER_KINDS")?)?
            .iter()
            .map(|entry| {
                let name = name(entry)?;
                let def = field(catalog.table("WEATHER")?, name)?;
                Ok(WeatherKind {
                    name: name.to_owned(),
                    sight: num(def, "sight")?,
                    rains: flag(def, "rains"),
                    inf: num(def, "inf")?,
                    veh: num(def, "veh")?,
                })
            })
            .collect::<Result<_>>()?;
        Ok(Self {
            kinds,
            vision,
            movement,
            weather,
        })
    }
}
