//! Wallet and population operations over an explicit, ordered view of the match.
//!
//! Units, queues, drops, points and nodes must keep simulation order. World node regions
//! are resolved by the caller against the authoritative map. This module does not capture
//! points, resolve regions, buy units, complete queues or award wave rewards.
//! `Mode::Default` means no reference mode object; explicit Conquest and Tutorial modes
//! retain the reference's Assault fallback for income. No ABI or wire format is defined here.
//!
//! Inputs must have valid player/region indices, unique unit IDs, known unit/support names
//! and finite numbers. Income dt must also be nonnegative. Unlike unchecked JavaScript
//! objects, malformed context returns an error before changing any wallet. World mode
//! requires explicit regions; a missing depot or a node outside all regions is valid.
//! Starting balances and one-off rewards are supplied by the caller.
use crate::{
    catalog::{Catalog, CatalogError, Value},
    economy::{self, Mode},
    math,
};
use std::{collections::BTreeMap, fmt};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ArmySize {
    #[default]
    Standard,
    Large,
    Massive,
    Endless,
}
impl ArmySize {
    /// Normalize the core's army choice, including the prototype's unknown-key fallback.
    pub fn from_key(key: &str) -> Self {
        match key {
            "large" => Self::Large,
            "massive" => Self::Massive,
            "endless" => Self::Endless,
            _ => Self::Standard,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::Large => "large",
            Self::Massive => "massive",
            Self::Endless => "endless",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModeContext {
    pub kind: Mode,
    pub defender_team: Option<i32>,
    /// The reference mode's `slot`, normally the Horde attacker paid by wave rewards.
    pub income_excluded_slot: Option<usize>,
}
impl Default for ModeContext {
    fn default() -> Self {
        Self {
            kind: Mode::Default,
            defender_team: None,
            income_excluded_slot: None,
        }
    }
}

/// The vector index is the player's slot, matching the reference's player array.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlayerEconomy {
    pub team: i32,
    pub away: bool,
    pub out: bool,
    pub mp: f64,
    pub mun: f64,
    pub fuel: f64,
    pub vp: f64,
    pub inc: f64,
    /// Absent until a construction-mode tick writes the reference field.
    pub fuel_inc: Option<f64>,
    pub upkeep: Option<f64>,
}

#[derive(Clone, Copy, Debug)]
pub struct Unit<'a> {
    pub id: u64,
    /// `None` represents a neutral World unit, which consumes no player's population.
    pub owner: Option<usize>,
    pub kind: &'a str,
    pub hp: f64,
    pub built: f64,
    pub queue: &'a [&'a str],
}
#[derive(Clone, Copy, Debug)]
pub struct PendingDrop<'a> {
    pub owner: usize,
    pub kind: &'a str,
    pub live: bool,
}
#[derive(Clone, Copy, Debug)]
pub struct Point {
    pub owner: Option<usize>,
    pub cut: bool,
    pub mp: f64,
    pub vp: f64,
}
#[derive(Clone, Copy, Debug)]
pub struct Node {
    pub depot: Option<u64>,
    pub rate: f64,
    pub fuel: bool,
    /// Index into `WorldContext::regions`. `None` means outside any region.
    pub region: Option<usize>,
}
/// Current authoritative depot facts, separate from the tick list used for upkeep.
#[derive(Clone, Copy, Debug)]
pub struct DepotFacts {
    pub id: u64,
    pub owner: Option<usize>,
    pub hp: f64,
    pub built: f64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegionKind {
    City,
    Industrial,
    Resource,
    Other,
}
#[derive(Clone, Copy, Debug)]
pub struct Region {
    pub team: i32,
    pub kind: RegionKind,
}
#[derive(Clone, Copy, Debug)]
pub struct WorldContext<'a> {
    pub regions: &'a [Region],
}
#[derive(Clone, Copy, Debug)]
pub struct Context<'a> {
    pub mode: ModeContext,
    pub army: ArmySize,
    pub units: &'a [Unit<'a>],
    pub drops: &'a [PendingDrop<'a>],
    pub points: &'a [Point],
    pub nodes: &'a [Node],
    pub world: Option<WorldContext<'a>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Catalog(CatalogError),
    Economy(economy::Error),
    InvalidSlot(usize),
    DuplicateUnit(u64),
    MissingWorld,
    InvalidRegion(usize),
    InvalidNumber(&'static str),
}
impl From<CatalogError> for Error {
    fn from(e: CatalogError) -> Self {
        Self::Catalog(e)
    }
}
impl From<economy::Error> for Error {
    fn from(e: economy::Error) -> Self {
        Self::Economy(e)
    }
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Catalog(e) => e.fmt(f),
            Self::Economy(e) => e.fmt(f),
            Self::InvalidSlot(s) => write!(f, "unknown economy slot: {s}"),
            Self::DuplicateUnit(id) => write!(f, "duplicate economy unit: {id}"),
            Self::MissingWorld => write!(f, "World economy requires authoritative regions"),
            Self::InvalidRegion(r) => write!(f, "unknown economy region: {r}"),
            Self::InvalidNumber(n) => write!(f, "invalid economy number: {n}"),
        }
    }
}
impl std::error::Error for Error {}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct EconomyState {
    pub players: Vec<PlayerEconomy>,
}
fn number(v: &Value, key: &str) -> Result<f64, Error> {
    v.get(key)
        .and_then(Value::number)
        .ok_or_else(|| CatalogError(format!("missing numeric economy rule: {key}")).into())
}
fn structure(v: &Value) -> bool {
    v.get("structure").and_then(Value::boolean).unwrap_or(false)
}
fn finite(n: f64, name: &'static str) -> Result<(), Error> {
    if n.is_finite() {
        Ok(())
    } else {
        Err(Error::InvalidNumber(name))
    }
}
impl EconomyState {
    /// Resolve a retired actor's bounty. The caller owns death ordering and applies
    /// the returned payment once, after wreck and building removal effects.
    /// Neutral actors have no victim slot. The mode slot (Horde AI) never earns it.
    pub fn kill_bounty(
        &self,
        mode: ModeContext,
        victim: Option<usize>,
        kind: &str,
        last_hit: Option<usize>,
    ) -> Result<Option<(usize, f64)>, Error> {
        if let Some(victim) = victim {
            self.slot(victim)?;
        }
        if let Some(excluded) = mode.income_excluded_slot {
            self.slot(excluded)?;
        }
        let catalog = Catalog::compiled()?;
        let cost = number(catalog.unit(kind)?, "cost")?;
        let Some(killer) = last_hit else {
            return Ok(None);
        };
        self.slot(killer)?;
        let player = &self.players[killer];
        if player.out
            || Some(killer) == mode.income_excluded_slot
            || victim.is_some_and(|victim| self.players[victim].team == player.team)
        {
            return Ok(None);
        }
        finite(player.mp, "bounty wallet")?;
        let amount = cost * number(catalog.table("CFG")?, "bounty")?;
        finite(player.mp + amount, "bounty wallet")?;
        Ok(Some((killer, amount)))
    }

    fn slot(&self, slot: usize) -> Result<(), Error> {
        if slot < self.players.len() {
            Ok(())
        } else {
            Err(Error::InvalidSlot(slot))
        }
    }
    /// Reject incoherent slots, duplicate units, unknown catalog kinds and non-finite input.
    /// Missing depots do not pay. Nodes outside a region do not pay in World mode.
    fn validate<'a>(&self, c: &Context<'a>) -> Result<BTreeMap<u64, &'a Unit<'a>>, Error> {
        let cat = Catalog::compiled()?;
        if c.mode.kind == Mode::World && c.world.is_none() {
            return Err(Error::MissingWorld);
        }
        if let Some(slot) = c.mode.income_excluded_slot {
            self.slot(slot)?;
        }
        for p in &self.players {
            for n in [p.mp, p.mun, p.fuel, p.vp, p.inc] {
                finite(n, "player")?;
            }
            for n in [p.fuel_inc, p.upkeep].into_iter().flatten() {
                finite(n, "player")?;
            }
        }
        let mut units = BTreeMap::new();
        for u in c.units {
            if let Some(s) = u.owner {
                self.slot(s)?;
            }
            cat.unit(u.kind)?;
            for kind in u.queue {
                cat.unit(kind)?;
            }
            finite(u.hp, "unit hp")?;
            finite(u.built, "unit built")?;
            if units.insert(u.id, u).is_some() {
                return Err(Error::DuplicateUnit(u.id));
            }
        }
        for d in c.drops {
            self.slot(d.owner)?;
            cat.support(d.kind)?;
        }
        for p in c.points {
            if let Some(s) = p.owner {
                self.slot(s)?;
            }
            finite(p.mp, "point mp")?;
            finite(p.vp, "point vp")?;
        }
        for n in c.nodes {
            finite(n.rate, "node rate")?;
            if let (Some(r), Some(w)) = (n.region, c.world)
                && r >= w.regions.len()
            {
                return Err(Error::InvalidRegion(r));
            }
        }
        Ok(units)
    }

    /// Includes queues on structures, all owned field units (even dead ones still present),
    /// and support drops that have not become live. Does not count structures themselves.
    pub fn population(&self, c: &Context<'_>, slot: usize) -> Result<f64, Error> {
        self.slot(slot)?;
        self.validate(c)?;
        let cat = Catalog::compiled()?;
        let mut total = 0.0;
        for u in c.units.iter().filter(|u| u.owner == Some(slot)) {
            if structure(cat.unit(u.kind)?) {
                let mut queued = 0.0;
                for kind in u.queue {
                    queued += economy::pop_use(kind)?;
                }
                total += queued;
            } else {
                total += economy::pop_use(u.kind)?;
            }
        }
        let mut drops = 0.0;
        for d in c.drops.iter().filter(|d| d.owner == slot && !d.live) {
            drops += economy::drop_pop(d.kind)?;
        }
        Ok(total + drops)
    }

    pub fn population_cap(&self, c: &Context<'_>, slot: usize) -> Result<f64, Error> {
        self.slot(slot)?;
        self.validate(c)?;
        let cat = Catalog::compiled()?;
        let cfg = cat.table("CFG")?;
        let mut cap = if c.mode.kind.is_construction() {
            number(cfg.get("classic").unwrap(), "popCap")?
        } else {
            number(cfg, "popCap")?
        };
        if c.mode.kind == Mode::World {
            let regions = c.world.ok_or(Error::MissingWorld)?.regions;
            let team = self.players[slot].team;
            let seats = self.players.iter().filter(|p| p.team == team).count();
            let active = self
                .players
                .iter()
                .filter(|p| p.team == team && !p.out)
                .count()
                .max(1);
            let gained = regions
                .iter()
                .filter(|r| r.team == team)
                .count()
                .saturating_sub(seats);
            let w = cat.table("WORLD_TUNING")?;
            cap = math::min(
                number(w, "capMax")?,
                cap + gained as f64 * number(w, "regionCap")? / active as f64,
            );
        }
        let army = cfg.get("armies").and_then(|v| v.get(c.army.key())).unwrap();
        Ok(math::round(cap * number(army, "pop")?))
    }

    /// Run income at the mode's source phase. World pays before capture/elimination.
    /// The caller supplies the current winner
    /// state and dt; points, units and ownership are not changed. Errors leave every wallet intact.
    pub fn advance_income(
        &mut self,
        c: &Context<'_>,
        dt: f64,
        winner_decided: bool,
    ) -> Result<(), Error> {
        let depots: Vec<_> = c
            .units
            .iter()
            .map(|u| DepotFacts {
                id: u.id,
                owner: u.owner,
                hp: u.hp,
                built: u.built,
            })
            .collect();
        self.advance_income_with_depots(c, &depots, dt, winner_decided)
    }

    /// Node payments consult these current facts. Upkeep still uses only `c.units`.
    /// Validate both views before changing any canonical wallet.
    pub fn advance_income_with_depots(
        &mut self,
        c: &Context<'_>,
        current_depots: &[DepotFacts],
        dt: f64,
        winner_decided: bool,
    ) -> Result<(), Error> {
        finite(dt, "dt")?;
        if dt < 0.0 {
            return Err(Error::InvalidNumber("dt"));
        }
        self.validate(c)?;
        let mut depots = BTreeMap::new();
        for depot in current_depots {
            if let Some(owner) = depot.owner {
                self.slot(owner)?;
            }
            finite(depot.hp, "depot hp")?;
            finite(depot.built, "depot built")?;
            if depots.insert(depot.id, depot).is_some() {
                return Err(Error::DuplicateUnit(depot.id));
            }
        }
        let cat = Catalog::compiled()?;
        let cfg = cat.table("CFG")?;
        let classic = cfg.get("classic").unwrap();
        let army = cfg.get("armies").and_then(|v| v.get(c.army.key())).unwrap();
        let income = number(army, "income")?;
        let team_vp = |players: &[PlayerEconomy], team| {
            players
                .iter()
                .filter(|p| p.team == team)
                .fold(0.0, |sum, p| sum + p.vp)
        };
        let lead = self.players.iter().fold(f64::NEG_INFINITY, |lead, p| {
            math::max(lead, team_vp(&self.players, p.team))
        });
        let mut next = self.players.clone();
        for slot in 0..next.len() {
            let team = next[slot].team;
            let held_vp = c
                .points
                .iter()
                .filter(|p| p.owner == Some(slot) && !p.cut)
                .fold(0.0, |a, p| a + p.vp);
            if c.mode.kind == Mode::Default && !winner_decided {
                next[slot].vp += held_vp * dt * if next[slot].away { 0.0 } else { 1.0 };
            }
            let active = !next[slot].away && !next[slot].out;
            if c.mode.kind.is_construction() {
                let world = if c.mode.kind == Mode::World {
                    Some(c.world.ok_or(Error::MissingWorld)?.regions)
                } else {
                    None
                };
                let paying: Vec<_> = c
                    .nodes
                    .iter()
                    .filter(|n| {
                        let Some(b) = n.depot.and_then(|id| depots.get(&id)) else {
                            return false;
                        };
                        b.owner == Some(slot)
                            && b.built >= 1.0
                            && b.hp > 0.0
                            && world.is_none_or(|r| n.region.is_some_and(|i| r[i].team == team))
                    })
                    .collect();
                let mp_nodes = paying
                    .iter()
                    .fold(0.0, |a, n| a + if n.fuel { 0.0 } else { n.rate });
                let fuel_nodes = paying
                    .iter()
                    .fold(0.0, |a, n| a + if n.fuel { n.rate } else { 0.0 });
                let mut upkeep = 0.0;
                for u in c
                    .units
                    .iter()
                    .filter(|u| u.owner == Some(slot) && u.hp > 0.0)
                {
                    let def = cat.unit(u.kind)?;
                    upkeep += if structure(def) {
                        0.0
                    } else {
                        number(def, "cost")? * number(classic, "upkeep")?
                    };
                }
                let mut mp_base = number(classic, "trickle")?;
                let mut fuel_base = number(classic, "hqFuel")?;
                let mun_rate;
                if let Some(regions) = world {
                    let tuning = cat.table("WORLD_TUNING")?;
                    let mates = next
                        .iter()
                        .filter(|p| p.team == team && !p.out)
                        .count()
                        .max(1) as f64;
                    let own: Vec<_> = regions.iter().filter(|r| r.team == team).collect();
                    let mut region_mp = 0.0;
                    for r in &own {
                        // Preserve the source's left-to-right reduction, including the zero.
                        region_mp += number(tuning, "regionMP")?;
                        region_mp += if r.kind == RegionKind::City {
                            number(tuning, "cityMP")?
                        } else {
                            0.0
                        };
                    }
                    mp_base += region_mp / mates;
                    fuel_base += own
                        .iter()
                        .filter(|r| r.kind == RegionKind::Industrial)
                        .count() as f64
                        * number(tuning, "industrialFuel")?
                        / mates;
                    // The reference uses this literal in worldIncome, outside WORLD_TUNING.
                    mun_rate = (0.2 * own.len() as f64
                        + own
                            .iter()
                            .filter(|r| r.kind == RegionKind::Resource)
                            .count() as f64
                            * number(tuning, "resourceMun")?)
                        / mates
                        * income;
                } else {
                    let vp = c.points.iter().fold(0.0, |a, p| {
                        a + if !p.cut && p.owner.is_some_and(|s| next[s].team == team) {
                            p.vp
                        } else {
                            0.0
                        }
                    });
                    mun_rate =
                        (number(classic, "hqMun")? + vp * number(classic, "munPerVp")?) * income;
                }
                let p = &mut next[slot];
                p.upkeep = Some(upkeep);
                p.inc = if active {
                    math::max(
                        number(classic, "minInc")?,
                        (mp_base + mp_nodes) * income - upkeep,
                    )
                } else {
                    0.0
                };
                p.fuel_inc = Some(if active {
                    (fuel_base + fuel_nodes) * income
                } else {
                    0.0
                });
                p.mp += p.inc * dt;
                p.fuel += p.fuel_inc.unwrap() * dt;
                if active {
                    p.mun += mun_rate * dt;
                }
            } else if c.mode.income_excluded_slot == Some(slot) {
                next[slot].inc = 0.0;
            } else {
                let assault = cfg.get("assault").unwrap();
                let base = if c.mode.kind == Mode::Default {
                    number(cfg, "mpBase")?
                } else if c.mode.kind == Mode::Annihilation {
                    number(assault, "annihilationBase")?
                } else if c.mode.defender_team == Some(team) {
                    number(assault, "defenderBase")?
                } else {
                    number(assault, "attackerBase")?
                };
                let held_mp = c
                    .points
                    .iter()
                    .filter(|p| p.owner == Some(slot) && !p.cut)
                    .fold(0.0, |a, p| a + p.mp);
                let catchup = math::min(
                    number(cfg, "catchupMax")?,
                    (lead - team_vp(&next, team)) / number(cfg, "catchupPer")?,
                );
                let p = &mut next[slot];
                p.inc = if active {
                    (base + held_mp + catchup) * income
                } else {
                    0.0
                };
                p.mp += p.inc * dt;
            }
        }
        self.players = next;
        Ok(())
    }
}

/// A recipient World snapshot without a player list already carries its final cap.
/// The reference returns this value unchanged, without army scaling or rounding.
pub fn world_cap_preview(cap: f64) -> Result<f64, Error> {
    finite(cap, "World cap preview")?;
    Ok(cap)
}
