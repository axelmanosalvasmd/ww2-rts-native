//! Bounded price, population and research queries using the compiled catalog.
//!
//! These functions do not buy, queue, validate placement, check a wallet or expose a client view.
//! Unit and support names must exist in the catalog. Research has HQ tiers 1..=3 and armory
//! levels 0..=3. Unknown ability IDs cost zero unless the caller supplies an explicit price,
//! as in the reference. An explicit ability price is returned unchanged, including non-finite values.
//! Factions accept only the four integer game factions. Missing or empty modes use the default
//! skirmish rules; unrecognized modes use the field-building list and non-construction prices.
use crate::{
    catalog::{Catalog, CatalogError, Value},
    math,
};
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Default,
    Classic,
    World,
    Conquest,
    Assault,
    Annihilation,
    Horde,
    Tutorial,
    Other,
}
impl Mode {
    pub fn from_kind(kind: Option<&str>) -> Self {
        match kind {
            None | Some("") => Self::Default,
            Some("classic") => Self::Classic,
            Some("world") => Self::World,
            Some("conquest") => Self::Conquest,
            Some("assault") => Self::Assault,
            Some("annihilation") => Self::Annihilation,
            Some("horde") => Self::Horde,
            Some("tutorial") => Self::Tutorial,
            _ => Self::Other,
        }
    }
    pub fn is_construction(self) -> bool {
        matches!(self, Self::Classic | Self::World)
    }
    pub fn is_skirmish(self) -> bool {
        matches!(
            self,
            Self::Default | Self::Conquest | Self::Assault | Self::Annihilation | Self::Horde
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Faction {
    Usa = 0,
    Germany = 1,
    Ussr = 2,
    Uk = 3,
}
impl TryFrom<u8> for Faction {
    type Error = Error;
    fn try_from(value: u8) -> Result<Self, Error> {
        match value {
            0 => Ok(Self::Usa),
            1 => Ok(Self::Germany),
            2 => Ok(Self::Ussr),
            3 => Ok(Self::Uk),
            _ => Err(Error::InvalidFaction(value)),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Catalog(CatalogError),
    InvalidFaction(u8),
    InvalidTechState,
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
            Self::InvalidFaction(n) => write!(f, "invalid faction: {n}"),
            Self::InvalidTechState => {
                write!(f, "research requires HQ tier 1..=3 and armory levels 0..=3")
            }
        }
    }
}
impl std::error::Error for Error {}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Price {
    pub mp: f64,
    pub fuel: f64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Currency {
    Manpower,
    Munitions,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SupportCost {
    pub currency: Currency,
    pub cost: f64,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResearchCost {
    pub mp: f64,
    pub mun: f64,
    pub time: f64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Technology {
    Tier,
    Infantry,
    Guns,
    Armor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TechState {
    tier: u8,
    infantry: u8,
    guns: u8,
    armor: u8,
}
impl TechState {
    pub fn new(tier: u8, infantry: u8, guns: u8, armor: u8) -> Result<Self, Error> {
        if !(1..=3).contains(&tier) || [infantry, guns, armor].iter().any(|&n| n > 3) {
            return Err(Error::InvalidTechState);
        }
        Ok(Self {
            tier,
            infantry,
            guns,
            armor,
        })
    }
    pub fn tier(self) -> u8 {
        self.tier
    }
    pub fn level(self, technology: Technology) -> u8 {
        match technology {
            Technology::Tier => self.tier,
            Technology::Infantry => self.infantry,
            Technology::Guns => self.guns,
            Technology::Armor => self.armor,
        }
    }
}

/// The optional override represents the reference ability object's own `mun` field.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AbilityCost<'a> {
    pub id: &'a str,
    pub munitions: Option<f64>,
}

fn number(value: &Value, key: &str) -> Result<f64, Error> {
    value
        .get(key)
        .and_then(Value::number)
        .ok_or_else(|| CatalogError(format!("missing numeric rules field: {key}")).into())
}
fn strings(value: &'static Value) -> Result<Vec<&'static str>, Error> {
    value
        .array()
        .ok_or_else(|| CatalogError("expected rules array".into()))?
        .iter()
        .map(|v| {
            v.string()
                .ok_or_else(|| CatalogError("expected rules string".into()).into())
        })
        .collect()
}

/// Recruitment price only. This does not check access, resources, population or placement.
pub fn price_of(mode: Mode, unit: &str) -> Result<Price, Error> {
    let def = Catalog::compiled()?.unit(unit)?;
    let mp = if mode.is_construction() {
        def.get("classicCost")
            .and_then(Value::number)
            .unwrap_or(number(def, "cost")?)
    } else {
        number(def, "cost")?
    };
    Ok(Price {
        mp,
        fuel: if mode.is_construction() {
            def.get("fuel").and_then(Value::number).unwrap_or(0.0)
        } else {
            0.0
        },
    })
}
/// Faction compatibility only, matching `canBuild`, not full production availability.
pub fn can_build(unit: &str, faction: Faction) -> Result<bool, Error> {
    Ok(Catalog::compiled()?
        .unit(unit)?
        .get("faction")
        .and_then(Value::number)
        .is_none_or(|n| n == faction as u8 as f64))
}
/// Population weight of one type; full army counting excludes buildings separately.
pub fn pop_use(unit: &str) -> Result<f64, Error> {
    Ok(Catalog::compiled()?
        .unit(unit)?
        .get("pop")
        .and_then(Value::number)
        .unwrap_or(1.0))
}
/// Population reserved by the units delivered by one support call.
pub fn drop_pop(support: &str) -> Result<f64, Error> {
    let def = Catalog::compiled()?.support(support)?;
    let mut total = 0.0;
    if let Some(units) = def.get("units") {
        for unit in strings(units)? {
            total += pop_use(unit)?;
        }
    }
    Ok(total)
}
/// Building choices for the mode, using the source construction/skirmish/world flags.
pub fn build_kinds(mode: Mode) -> Result<Vec<&'static str>, Error> {
    let table = match mode {
        Mode::World => "WORLD_BUILDS",
        Mode::Classic => "BUILDABLE",
        m if m.is_skirmish() => "SKIRMISH_BUILDS",
        _ => "FIELD_BUILDS",
    };
    strings(Catalog::compiled()?.table(table)?)
}
/// Construction modes require engineers; other modes use the configured fort builders.
pub fn builder_types(mode: Mode) -> Result<Vec<&'static str>, Error> {
    if mode.is_construction() {
        return Ok(vec!["engineer"]);
    }
    let builders = Catalog::compiled()?
        .table("CFG")?
        .get("fortBuilders")
        .ok_or_else(|| CatalogError("missing fortBuilders".into()))?;
    strings(builders)
}
/// Support uses munitions in Classic and World, manpower in every other mode.
pub fn sup_cost(mode: Mode, support: &str) -> Result<SupportCost, Error> {
    let def = Catalog::compiled()?.support(support)?;
    let (currency, field) = if mode.is_construction() {
        (Currency::Munitions, "mun")
    } else {
        (Currency::Manpower, "cost")
    };
    Ok(SupportCost {
        currency,
        cost: number(def, field)?,
    })
}
/// Ability cost only. A missing override uses the ability ID lookup, then zero.
pub fn ab_cost(mode: Mode, ability: AbilityCost<'_>) -> Result<f64, Error> {
    if !mode.is_construction() {
        return Ok(0.0);
    }
    if let Some(price) = ability.munitions {
        return Ok(price);
    }
    Ok(Catalog::compiled()?
        .table("AB_MUN")?
        .get(ability.id)
        .and_then(Value::number)
        .unwrap_or(0.0))
}
/// Required HQ tier of a known unit type, defaulting to tier 1.
pub fn tier_of(unit: &str) -> Result<u8, Error> {
    let catalog = Catalog::compiled()?;
    catalog.unit(unit)?;
    Ok(catalog
        .table("TIER")?
        .get(unit)
        .and_then(Value::number)
        .unwrap_or(1.0) as u8)
}
/// Next research price, or `None` at the highest tier or armory level.
pub fn tech_cost(
    player: TechState,
    technology: Technology,
    mode: Mode,
) -> Result<Option<ResearchCost>, Error> {
    let tech = Catalog::compiled()?.table("TECH")?;
    let cost = if technology == Technology::Tier {
        tech.get("tiers")
            .and_then(|t| t.get(&(player.tier + 1).to_string()))
    } else {
        tech.get("levels")
            .and_then(Value::array)
            .and_then(|a| a.get(player.level(technology) as usize))
    };
    cost.map(|cost| {
        Ok(ResearchCost {
            mp: number(cost, "mp")?,
            mun: if mode.is_construction() {
                number(cost, "mun")?
            } else {
                0.0
            },
            time: number(cost, "time")?,
        })
    })
    .transpose()
}
/// Damage multiplier using the core's deterministic pow. `None` means no armory upgrades.
/// Every provided type name is checked, even when the corresponding player is absent.
pub fn tech_mul(
    attacker: Option<TechState>,
    by: Option<&str>,
    defender: Option<TechState>,
    to: &str,
) -> Result<f64, Error> {
    let catalog = Catalog::compiled()?;
    let by = by.map(|kind| catalog.unit(kind)).transpose()?;
    let to = catalog.unit(to)?;
    let infantry = |def: &Value| {
        def.get("infantry")
            .and_then(Value::boolean)
            .unwrap_or(false)
    };
    let vehicle = |def: &Value| {
        !infantry(def)
            && !def
                .get("structure")
                .and_then(Value::boolean)
                .unwrap_or(false)
    };
    let up = match (attacker, by) {
        (Some(a), Some(by)) if vehicle(by) => a.guns,
        (Some(a), Some(by)) if infantry(by) => a.infantry,
        _ => 0,
    };
    let down = defender.filter(|_| vehicle(to)).map_or(0, |d| d.armor);
    let step = number(catalog.table("TECH")?, "step")?;
    Ok((1.0 + step * f64::from(up)) * math::pow(1.0 - step, f64::from(down)))
}
