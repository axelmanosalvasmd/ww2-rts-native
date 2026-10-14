//! Canonical production queues and player research, separate from authoritative units.
//!
//! Unit identity, ownership, health and construction progress are borrowed each call.
//! Wallets remain in `EconomyState`. Queue types and job statuses are derived from one
//! paid-job list. This module emits completed-unit requests; placement, spawning,
//! rally orders, combat and story/statistics updates belong to the match runner.
use crate::{
    catalog::{Catalog, CatalogError, Value},
    economy::{self, Faction, Mode, Price, TechState, Technology},
    economy_state::{self, ArmySize, EconomyState, ModeContext, PendingDrop, WorldContext},
};
use std::collections::{BTreeMap, BTreeSet};

const MAX_JOB_ID: u64 = 9_007_199_254_740_991;
const IMPORT_QUEUE_LIMIT: usize = 4096;
#[derive(Clone, Copy, Debug)]
pub struct UnitView<'a> {
    pub id: u32,
    pub owner: Option<usize>,
    pub kind: &'a str,
    pub hp: f64,
    pub built: f64,
}
#[derive(Clone, Copy, Debug)]
pub struct Context<'a> {
    pub mode: ModeContext,
    pub army: ArmySize,
    pub tech_enabled: bool,
    pub naval: bool,
    pub sudden_death: bool,
    pub units: &'a [UnitView<'a>],
    pub drops: &'a [PendingDrop<'a>],
    pub world: Option<WorldContext<'a>>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Denial {
    Blocked,
    Needs,
    Tier,
    Mp,
    Mun,
    Fuel,
    Pop,
    Max,
    QueueFull,
    SuddenDeath,
    Unseen,
    NotWaiting,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Catalog(CatalogError),
    Economy(economy::Error),
    Population(economy_state::Error),
    InvalidInput(&'static str),
    Denied(Denial),
}
impl From<CatalogError> for Error {
    fn from(v: CatalogError) -> Self {
        Self::Catalog(v)
    }
}
impl From<economy::Error> for Error {
    fn from(v: economy::Error) -> Self {
        Self::Economy(v)
    }
}
impl From<economy_state::Error> for Error {
    fn from(v: economy_state::Error) -> Self {
        Self::Population(v)
    }
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "production: {self:?}")
    }
}
impl std::error::Error for Error {}
#[derive(Clone, Debug, PartialEq)]
pub struct ProductionJob {
    pub id: u64,
    pub kind: String,
    pub charge: Price,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobStatus {
    Active,
    Waiting,
}
#[derive(Clone, Debug, PartialEq)]
pub struct JobRow {
    pub job: ProductionJob,
    pub status: JobStatus,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BuildingProduction {
    jobs: Vec<ProductionJob>,
    progress: f64,
}
impl BuildingProduction {
    pub fn jobs(&self) -> &[ProductionJob] {
        &self.jobs
    }
    pub fn progress(&self) -> f64 {
        self.progress
    }
    pub fn queue(&self) -> impl Iterator<Item = &str> {
        self.jobs.iter().map(|j| j.kind.as_str())
    }
    pub fn rows(&self) -> Vec<JobRow> {
        self.jobs
            .iter()
            .enumerate()
            .map(|(i, j)| JobRow {
                job: j.clone(),
                status: if i == 0 {
                    JobStatus::Active
                } else {
                    JobStatus::Waiting
                },
            })
            .collect()
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct ResearchJob {
    pub kind: Technology,
    pub progress: f64,
    pub time: f64,
}
#[derive(Clone, Debug, PartialEq)]
pub struct PlayerResearch {
    tech: TechState,
    jobs: Vec<ResearchJob>,
}
impl PlayerResearch {
    pub fn tech(&self) -> TechState {
        self.tech
    }
    pub fn jobs(&self) -> &[ResearchJob] {
        &self.jobs
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct ProductionState {
    buildings: BTreeMap<u32, BuildingProduction>,
    players: Vec<PlayerResearch>,
    next_job: u64,
}
#[derive(Clone, Debug, PartialEq)]
pub struct CompletedUnit {
    pub building: u32,
    pub owner: usize,
    pub kind: String,
    pub job_id: u64,
}
/// A staged single-building progress/completion transaction. It holds only
/// production state, so unit placement and metadata can validate before commit.
#[must_use = "stage publication and commit, or drop unchanged"]
pub struct PreparedBuildingStep<'a> {
    state: &'a mut ProductionState,
    building: u32,
    next: Option<BuildingProduction>,
    completed: Option<CompletedUnit>,
}
impl PreparedBuildingStep<'_> {
    pub fn completed(&self) -> Option<&CompletedUnit> {
        self.completed.as_ref()
    }
    pub fn commit(self) -> Option<CompletedUnit> {
        if let Some(next) = self.next {
            self.state.buildings.insert(self.building, next);
        }
        self.completed
    }
}
/// An admitted Conquest purchase, ready for the caller's staged placement.
#[derive(Clone, Debug, PartialEq)]
pub struct RecruitRequest {
    pub building: u32,
    pub owner: usize,
    pub kind: String,
    pub spend: Price,
}
/// A recruitment purchase whose MP has been paid after successful placement.
#[derive(Clone, Debug, PartialEq)]
pub struct CompletedRecruit {
    pub request: RecruitRequest,
}
/// Preparation reserves exclusive access to the wallet without charging it. Drop
/// this token if placement fails. Commit once after staging the unit successfully,
/// before publishing the caller's transaction. Authoritative facts must stay stable
/// between preparation and commit; the match processes purchases serially.
#[must_use = "place the recruit and commit, or drop without payment"]
pub struct PreparedRecruit<'a, 'facts> {
    request: RecruitRequest,
    wallets: &'a mut EconomyState,
    _production: &'a ProductionState,
    _context: &'a Context<'facts>,
}
impl PreparedRecruit<'_, '_> {
    pub fn request(&self) -> &RecruitRequest {
        &self.request
    }
    /// The caller has already completed placement on its staged match state.
    pub fn commit(self) -> CompletedRecruit {
        self.wallets.players[self.request.owner].mp -= self.request.spend.mp;
        CompletedRecruit {
            request: self.request,
        }
    }
}
/// Research admission holds both canonical queues and the payer wallet without
/// mutation, allowing the match to validate its spending record before commit.
#[must_use = "prepare story spending and commit, or drop unchanged"]
pub struct PreparedResearch<'a, 'facts> {
    state: &'a mut ProductionState,
    wallets: &'a mut EconomyState,
    context: &'a Context<'facts>,
    slot: usize,
    kind: Technology,
    cost: economy::ResearchCost,
}
impl PreparedResearch<'_, '_> {
    pub fn cost(&self) -> economy::ResearchCost {
        self.cost
    }
    pub fn commit(self) -> economy::ResearchCost {
        // The context borrow keeps the validated authoritative inputs stable.
        let _ = self.context;
        let payer = &mut self.wallets.players[self.slot];
        payer.mp -= self.cost.mp;
        if self.cost.mun != 0.0 {
            payer.mun -= self.cost.mun;
        }
        self.state.players[self.slot].jobs.push(ResearchJob {
            kind: self.kind,
            progress: 0.0,
            time: self.cost.time,
        });
        self.cost
    }
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StepEvents {
    pub units: Vec<CompletedUnit>,
    pub research: Vec<(usize, Technology)>,
}
fn denial<T>(d: Denial) -> Result<T, Error> {
    Err(Error::Denied(d))
}
fn flag(v: &Value, key: &str) -> bool {
    v.get(key).and_then(Value::boolean).unwrap_or(false)
}
fn makes(v: &Value, kind: &str) -> bool {
    v.get("makes")
        .and_then(Value::array)
        .is_some_and(|a| a.iter().any(|s| s.string() == Some(kind)))
}
fn train_time(kind: &str) -> Result<f64, Error> {
    Catalog::compiled()?
        .unit(kind)?
        .get("train")
        .and_then(Value::number)
        .filter(|v| v.is_finite() && *v > 0.0)
        .ok_or(Error::InvalidInput("unit has no training time"))
}
fn valid_charge(p: Price) -> bool {
    p.mp.is_finite() && p.fuel.is_finite() && p.mp >= 0.0 && p.fuel >= 0.0
}
impl ProductionState {
    pub fn new(tech: Vec<TechState>) -> Result<Self, Error> {
        if tech.is_empty() || tech.len() > 6 {
            return Err(Error::InvalidInput("player count"));
        }
        Ok(Self {
            buildings: BTreeMap::new(),
            players: tech
                .into_iter()
                .map(|tech| PlayerResearch { tech, jobs: vec![] })
                .collect(),
            next_job: 0,
        })
    }
    /// Register a newly placed building's empty queue and zero progress. Repeated
    /// initialization preserves existing paid jobs and progress.
    pub fn initialize_building(&mut self, id: u32) -> Result<&BuildingProduction, Error> {
        if id == 0 {
            return Err(Error::InvalidInput("building ID"));
        }
        Ok(self.buildings.entry(id).or_default())
    }
    pub fn building(&self, id: u32) -> Option<&BuildingProduction> {
        self.buildings.get(&id)
    }
    pub(crate) fn buildings(&self) -> impl Iterator<Item = (u32, &BuildingProduction)> {
        self.buildings.iter().map(|(&id, row)| (id, row))
    }
    pub fn player(&self, slot: usize) -> Option<&PlayerResearch> {
        self.players.get(slot)
    }
    /// Reference nextProductionJob counter: the last allocated ID, initially zero.
    pub fn next_job_id(&self) -> u64 {
        self.next_job
    }
    pub fn remove_building(&mut self, id: u32) -> Option<BuildingProduction> {
        self.buildings.remove(&id)
    }
    /// The match calls this once on the Classic Sudden Death transition. No refunds.
    pub fn clear_queues(&mut self) {
        for b in self.buildings.values_mut() {
            b.jobs.clear();
            b.progress = 0.0;
        }
    }
    fn allocate(&self) -> Result<u64, Error> {
        self.next_job
            .checked_add(1)
            .filter(|n| *n <= MAX_JOB_ID)
            .ok_or(Error::InvalidInput("job ID exhausted"))
    }
    /// Import the reference's queue-only representation. Matching positions keep paid
    /// identities; changed entries get fresh zero-charge IDs. Progress is preserved.
    pub fn reconcile_queue(&mut self, id: u32, types: &[&str]) -> Result<(), Error> {
        if id == 0 || types.len() > IMPORT_QUEUE_LIMIT {
            return Err(Error::InvalidInput("import queue"));
        }
        for kind in types {
            train_time(kind)?;
        }
        let mut next = self.clone();
        let old = next.buildings.get(&id).cloned().unwrap_or_default();
        let mut jobs = Vec::with_capacity(types.len());
        for (i, kind) in types.iter().enumerate() {
            if let Some(j) = old.jobs.get(i).filter(|j| j.kind == *kind) {
                jobs.push(j.clone());
            } else {
                let n = next.allocate()?;
                next.next_job = n;
                jobs.push(ProductionJob {
                    id: n,
                    kind: (*kind).into(),
                    charge: Price { mp: 0.0, fuel: 0.0 },
                });
            }
        }
        next.buildings.insert(
            id,
            BuildingProduction {
                jobs,
                progress: old.progress,
            },
        );
        *self = next;
        Ok(())
    }
    /// Record a job after another authoritative operation already paid for it, such
    /// as World recovery. This does not debit a wallet or perform purchase admission.
    pub fn record_paid(&mut self, id: u32, kind: &str, charge: Price) -> Result<u64, Error> {
        if id == 0
            || !valid_charge(charge)
            || self
                .buildings
                .get(&id)
                .is_some_and(|b| b.jobs.len() >= IMPORT_QUEUE_LIMIT)
        {
            return Err(Error::InvalidInput("paid job"));
        }
        train_time(kind)?;
        let n = self.allocate()?;
        self.buildings
            .entry(id)
            .or_default()
            .jobs
            .push(ProductionJob {
                id: n,
                kind: kind.into(),
                charge,
            });
        self.next_job = n;
        Ok(n)
    }
    fn validate(&self, c: &Context<'_>, wallets: &EconomyState) -> Result<(), Error> {
        if wallets.players.len() != self.players.len() {
            return Err(Error::InvalidInput("player count mismatch"));
        }
        if c.mode
            .income_excluded_slot
            .is_some_and(|s| s >= self.players.len())
        {
            return Err(Error::InvalidInput("mode slot"));
        }
        let mut ids = BTreeSet::new();
        for u in c.units {
            if u.id == 0
                || !ids.insert(u.id)
                || u.owner.is_some_and(|s| s >= self.players.len())
                || !u.hp.is_finite()
                || !u.built.is_finite()
            {
                return Err(Error::InvalidInput("unit"));
            }
            Catalog::compiled()?.unit(u.kind)?;
        }
        for p in &wallets.players {
            if ![p.mp, p.mun, p.fuel].into_iter().all(f64::is_finite) {
                return Err(Error::InvalidInput("wallet"));
            }
        }
        for d in c.drops {
            if d.owner >= self.players.len() {
                return Err(Error::InvalidInput("drop owner"));
            }
            Catalog::compiled()?.support(d.kind)?;
        }
        if c.mode.kind == Mode::World && c.world.is_none() {
            return Err(Error::InvalidInput("World regions"));
        }
        Ok(())
    }
    fn slot(&self, slot: usize) -> Result<(), Error> {
        if slot < self.players.len() {
            Ok(())
        } else {
            Err(Error::InvalidInput("player slot"))
        }
    }
    fn access(c: &Context<'_>, wallets: &EconomyState, u: &UnitView<'_>, slot: usize) -> bool {
        u.owner == Some(slot)
            || (c.mode.kind == Mode::Horde
                && Some(slot) != c.mode.income_excluded_slot
                && u.owner != c.mode.income_excluded_slot
                && u.owner
                    .is_some_and(|s| wallets.players[s].team == wallets.players[slot].team))
    }
    pub fn production_buildings(
        &self,
        c: &Context<'_>,
        wallets: &EconomyState,
        slot: usize,
        kind: Option<&str>,
    ) -> Result<Vec<u32>, Error> {
        self.validate(c, wallets)?;
        self.slot(slot)?;
        let cat = Catalog::compiled()?;
        if let Some(k) = kind {
            cat.unit(k)?;
        }
        let mut out = vec![];
        for u in c.units {
            let def = cat.unit(u.kind)?;
            if u.hp > 0.0
                && u.built >= 1.0
                && Self::access(c, wallets, u, slot)
                && def.get("makes").and_then(Value::array).is_some()
                && kind.is_none_or(|k| {
                    makes(def, k)
                        || (c.mode.kind.is_skirmish() && u.kind == "barracks" && k == "rifle")
                })
            {
                out.push(u.id);
            }
        }
        Ok(out)
    }
    pub fn tech_labs(
        &self,
        c: &Context<'_>,
        wallets: &EconomyState,
        slot: usize,
        kind: Technology,
    ) -> Result<usize, Error> {
        self.validate(c, wallets)?;
        self.slot(slot)?;
        Ok(Self::labs(c, wallets, slot, kind))
    }
    fn labs(c: &Context<'_>, wallets: &EconomyState, slot: usize, kind: Technology) -> usize {
        c.units
            .iter()
            .filter(|u| {
                u.kind
                    == if kind == Technology::Tier {
                        "hq"
                    } else {
                        "armory"
                    }
                    && u.hp > 0.0
                    && u.built >= 1.0
                    && Self::access(c, wallets, u, slot)
            })
            .count()
    }
    fn population(
        &self,
        c: &Context<'_>,
        wallets: &EconomyState,
        slot: usize,
    ) -> Result<(f64, f64), Error> {
        let queues: Vec<Vec<&str>> = c
            .units
            .iter()
            .map(|u| {
                self.buildings
                    .get(&u.id)
                    .map_or_else(Vec::new, |b| b.queue().collect())
            })
            .collect();
        let units: Vec<_> = c
            .units
            .iter()
            .zip(&queues)
            .map(|(u, q)| economy_state::Unit {
                id: u.id.into(),
                owner: u.owner,
                kind: u.kind,
                hp: u.hp,
                built: u.built,
                queue: q,
            })
            .collect();
        let context = economy_state::Context {
            mode: c.mode,
            army: c.army,
            units: &units,
            drops: c.drops,
            world: c.world,
            points: &[],
            nodes: &[],
        };
        Ok((
            wallets.population(&context, slot)?,
            wallets.population_cap(&context, slot)?,
        ))
    }
    /// Prepare instant Conquest recruitment without changing wallets or queues.
    /// Default represents the absent mode object of ordinary Conquest. Other
    /// modes are outside this API. Winner/away/out/rate admission stays with Match.
    pub fn prepare_recruit<'a, 'facts>(
        &'a self,
        c: &'a Context<'facts>,
        wallets: &'a mut EconomyState,
        slot: usize,
        faction: Faction,
        kind: &str,
        from: Option<u32>,
    ) -> Result<PreparedRecruit<'a, 'facts>, Error> {
        self.validate(c, wallets)?;
        self.slot(slot)?;
        if !matches!(c.mode.kind, Mode::Default | Mode::Conquest) {
            return Err(Error::InvalidInput("recruitment supports Conquest only"));
        }
        let cat = Catalog::compiled()?;
        let Ok(def) = cat.unit(kind) else {
            return denial(Denial::Blocked);
        };
        if kind == "worldbase" || !economy::can_build(kind, faction)? {
            return denial(Denial::Blocked);
        }
        if flag(def, "classic") || (flag(def, "naval") && !c.naval) {
            return denial(Denial::Needs);
        }
        if c.tech_enabled && economy::tier_of(kind)? > self.players[slot].tech.tier() {
            return denial(Denial::Tier);
        }
        let price = economy::price_of(c.mode.kind, kind)?;
        let (pop, cap) = self.population(c, wallets, slot)?;
        let mut have = c
            .units
            .iter()
            .filter(|u| u.owner == Some(slot) && u.kind == kind)
            .count();
        for u in c.units.iter().filter(|u| u.owner == Some(slot)) {
            have += self
                .buildings
                .get(&u.id)
                .map_or(0, |b| b.jobs.iter().filter(|j| j.kind == kind).count());
        }
        if wallets.players[slot].mp < price.mp {
            return denial(Denial::Mp);
        }
        if price.fuel != 0.0 && wallets.players[slot].fuel < price.fuel {
            return denial(Denial::Fuel);
        }
        if pop + economy::pop_use(kind)? > cap {
            return denial(Denial::Pop);
        }
        if def
            .get("max")
            .and_then(Value::number)
            .is_some_and(|max| have as f64 >= max)
        {
            return denial(Denial::Max);
        }
        let makers = self.production_buildings(c, wallets, slot, Some(kind))?;
        let building = if let Some(id) = from {
            makers.into_iter().find(|n| *n == id)
        } else {
            makers.first().copied()
        };
        let Some(building) = building else {
            return denial(Denial::Needs);
        };
        Ok(PreparedRecruit {
            request: RecruitRequest {
                building,
                owner: slot,
                kind: kind.into(),
                spend: price,
            },
            wallets,
            _production: self,
            _context: c,
        })
    }
    /// Construction-mode recruitment with the source denial order. Command-level
    /// winner/away/out and rate-limit admission belongs to the caller.
    pub fn queue_unit(
        &mut self,
        c: &Context<'_>,
        wallets: &mut EconomyState,
        slot: usize,
        faction: Faction,
        kind: &str,
        from: Option<u32>,
    ) -> Result<u64, Error> {
        self.validate(c, wallets)?;
        self.slot(slot)?;
        if !c.mode.kind.is_construction() {
            return denial(Denial::Needs);
        }
        let def = Catalog::compiled()?.unit(kind)?;
        if kind == "worldbase" || !economy::can_build(kind, faction)? {
            return denial(Denial::Blocked);
        }
        if flag(def, "naval") && !c.naval {
            return denial(Denial::Needs);
        }
        if c.tech_enabled && economy::tier_of(kind)? > self.players[slot].tech.tier() {
            return denial(Denial::Tier);
        }
        let price = economy::price_of(c.mode.kind, kind)?;
        let p = &wallets.players[slot];
        let (pop, cap) = self.population(c, wallets, slot)?;
        let mut have = c
            .units
            .iter()
            .filter(|u| u.owner == Some(slot) && u.kind == kind)
            .count();
        for u in c.units.iter().filter(|u| u.owner == Some(slot)) {
            have += self
                .buildings
                .get(&u.id)
                .map_or(0, |b| b.jobs.iter().filter(|j| j.kind == kind).count());
        }
        if p.mp < price.mp {
            return denial(Denial::Mp);
        }
        if price.fuel != 0.0 && p.fuel < price.fuel {
            return denial(Denial::Fuel);
        }
        if pop + economy::pop_use(kind)? > cap {
            return denial(Denial::Pop);
        }
        if def
            .get("max")
            .and_then(Value::number)
            .is_some_and(|max| have as f64 >= max)
        {
            return denial(Denial::Max);
        }
        if c.sudden_death {
            return denial(Denial::SuddenDeath);
        }
        // The reference's construction queue selector does not filter HP here.
        let mut maker = None;
        let mut any = false;
        let mut shortest = usize::MAX;
        for u in c
            .units
            .iter()
            .filter(|u| u.owner == Some(slot) && u.built >= 1.0)
        {
            if !makes(Catalog::compiled()?.unit(u.kind)?, kind) {
                continue;
            }
            any = true;
            let len = self.buildings.get(&u.id).map_or(0, |b| b.jobs.len());
            if len >= 5 {
                continue;
            }
            if from.is_some_and(|id| id == u.id) || (from.is_none() && len < shortest) {
                maker = Some(u.id);
                shortest = len;
            }
        }
        let Some(id) = maker else {
            return denial(if any {
                Denial::QueueFull
            } else {
                Denial::Needs
            });
        };
        let job = self.record_paid(id, kind, price)?;
        let p = &mut wallets.players[slot];
        p.mp -= price.mp;
        p.fuel -= price.fuel;
        Ok(job)
    }
    /// Only waiting jobs on a living, finished, owned producer can be cancelled.
    /// The returned original charge is also the amount the match subtracts from spent statistics.
    pub fn cancel(
        &mut self,
        c: &Context<'_>,
        wallets: &mut EconomyState,
        slot: usize,
        building: u32,
        job: u64,
    ) -> Result<Price, Error> {
        self.validate(c, wallets)?;
        self.slot(slot)?;
        if !c.mode.kind.is_construction() {
            return denial(Denial::Needs);
        }
        if c.sudden_death {
            return denial(Denial::SuddenDeath);
        }
        let Some(u) = c.units.iter().find(|u| u.id == building) else {
            return denial(Denial::Unseen);
        };
        if u.owner != Some(slot)
            || u.hp <= 0.0
            || u.built < 1.0
            || Catalog::compiled()?.unit(u.kind)?.get("makes").is_none()
            || job > MAX_JOB_ID
        {
            return denial(Denial::Unseen);
        }
        let Some(index) = self
            .buildings
            .get(&building)
            .and_then(|b| b.jobs.iter().position(|j| j.id == job))
            .filter(|i| *i > 0)
        else {
            return denial(Denial::NotWaiting);
        };
        let refund = self.buildings.get(&building).unwrap().jobs[index].charge;
        let p = &wallets.players[slot];
        if !(p.mp + refund.mp).is_finite() || !(p.fuel + refund.fuel).is_finite() {
            return Err(Error::InvalidInput("refund overflow"));
        }
        self.buildings
            .get_mut(&building)
            .unwrap()
            .jobs
            .remove(index);
        wallets.players[slot].mp += refund.mp;
        wallets.players[slot].fuel += refund.fuel;
        Ok(refund)
    }
    pub fn prepare_research<'a, 'facts>(
        &'a mut self,
        c: &'a Context<'facts>,
        wallets: &'a mut EconomyState,
        slot: usize,
        kind: Technology,
    ) -> Result<PreparedResearch<'a, 'facts>, Error> {
        self.validate(c, wallets)?;
        self.slot(slot)?;
        if !c.tech_enabled {
            return denial(Denial::Blocked);
        }
        if c.sudden_death {
            return denial(Denial::SuddenDeath);
        }
        let p = &self.players[slot];
        let Some(cost) = economy::tech_cost(p.tech, kind, c.mode.kind)? else {
            return denial(Denial::Max);
        };
        if p.jobs.iter().any(|j| j.kind == kind) {
            return denial(Denial::Max);
        }
        if kind != Technology::Tier && p.tech.level(kind) >= p.tech.tier() {
            return denial(Denial::Tier);
        }
        let labs = Self::labs(c, wallets, slot, kind);
        if labs == 0 {
            return denial(Denial::Needs);
        }
        if kind != Technology::Tier
            && p.jobs.iter().filter(|j| j.kind != Technology::Tier).count() >= labs
        {
            return denial(Denial::QueueFull);
        }
        if wallets.players[slot].mp < cost.mp {
            return denial(Denial::Mp);
        }
        if cost.mun != 0.0 && wallets.players[slot].mun < cost.mun {
            return denial(Denial::Mun);
        }
        Ok(PreparedResearch {
            state: self,
            wallets,
            context: c,
            slot,
            kind,
            cost,
        })
    }
    pub fn request_research(
        &mut self,
        c: &Context<'_>,
        wallets: &mut EconomyState,
        slot: usize,
        kind: Technology,
    ) -> Result<economy::ResearchCost, Error> {
        Ok(self.prepare_research(c, wallets, slot, kind)?.commit())
    }
    fn validate_dt(dt: f64) -> Result<(), Error> {
        if !dt.is_finite() || dt < 0.0 {
            return Err(Error::InvalidInput("dt"));
        }
        Ok(())
    }
    fn research_step(
        players: &mut [PlayerResearch],
        c: &Context<'_>,
        wallets: &EconomyState,
        dt: f64,
        out: &mut Vec<(usize, Technology)>,
    ) -> Result<(), Error> {
        if c.tech_enabled {
            for (slot, player) in players.iter_mut().enumerate() {
                if wallets.players[slot].out {
                    continue;
                }
                let hq = Self::labs(c, wallets, slot, Technology::Tier);
                let mut armories = Self::labs(c, wallets, slot, Technology::Infantry) as i64;
                let mut jobs = vec![];
                let mut tech = player.tech;
                for mut j in player.jobs.clone() {
                    let run = if j.kind == Technology::Tier {
                        hq > 0
                    } else {
                        let n = armories;
                        armories -= 1;
                        n > 0
                    };
                    if run {
                        j.progress += dt;
                        if !j.progress.is_finite() {
                            return Err(Error::InvalidInput("research overflow"));
                        }
                        if j.progress >= j.time {
                            let level = |t| tech.level(t) + u8::from(t == j.kind);
                            tech = TechState::new(
                                tech.tier() + u8::from(j.kind == Technology::Tier),
                                level(Technology::Infantry),
                                level(Technology::Guns),
                                level(Technology::Armor),
                            )?;
                            out.push((slot, j.kind));
                            continue;
                        }
                    }
                    jobs.push(j);
                }
                *player = PlayerResearch { tech, jobs };
            }
        }
        Ok(())
    }
    fn building_step(
        u: &UnitView<'_>,
        sudden_death: bool,
        b: &mut BuildingProduction,
        dt: f64,
    ) -> Result<Option<CompletedUnit>, Error> {
        if sudden_death
            || u.hp <= 0.0
            || u.built < 1.0
            || !flag(Catalog::compiled()?.unit(u.kind)?, "building")
        {
            return Ok(None);
        }
        let Some(j) = b.jobs.first() else {
            return Ok(None);
        };
        let time = train_time(&j.kind)?;
        b.progress += dt;
        if !b.progress.is_finite() {
            return Err(Error::InvalidInput("training overflow"));
        }
        if b.progress >= time {
            let Some(owner) = u.owner else {
                return Err(Error::InvalidInput("neutral production"));
            };
            let job = b.jobs.remove(0);
            b.progress = 0.0;
            return Ok(Some(CompletedUnit {
                building: u.id,
                owner,
                kind: job.kind,
                job_id: job.id,
            }));
        }
        Ok(None)
    }
    /// Run only the source early lab phase. No building training advances here.
    pub fn advance_research(
        &mut self,
        c: &Context<'_>,
        wallets: &EconomyState,
        dt: f64,
    ) -> Result<Vec<(usize, Technology)>, Error> {
        self.validate(c, wallets)?;
        Self::validate_dt(dt)?;
        let mut players = self.players.clone();
        let mut out = Vec::new();
        Self::research_step(&mut players, c, wallets, dt, &mut out)?;
        self.players = players;
        Ok(out)
    }
    /// Read-only admission for the source research and live-building phases.
    /// Only this component's prospective jobs are staged, never a Match or unit registry.
    pub(crate) fn validate_phases(
        &self,
        c: &Context<'_>,
        wallets: &EconomyState,
        dt: f64,
    ) -> Result<(), Error> {
        self.validate(c, wallets)?;
        Self::validate_dt(dt)?;
        let mut players = self.players.clone();
        Self::research_step(&mut players, c, wallets, dt, &mut Vec::new())?;
        for (&id, building) in &self.buildings {
            let unit = c
                .units
                .iter()
                .find(|unit| unit.id == id)
                .ok_or(Error::InvalidInput("production building missing"))?;
            let mut next = building.clone();
            Self::building_step(unit, c.sudden_death, &mut next, dt)?;
        }
        Ok(())
    }
    /// Stage one building at its actual source unit-loop position. Completion
    /// placement and unit publication must validate before consuming this guard.
    pub fn prepare_building_step<'a>(
        &'a mut self,
        c: &Context<'_>,
        wallets: &EconomyState,
        building: u32,
        dt: f64,
    ) -> Result<PreparedBuildingStep<'a>, Error> {
        self.validate(c, wallets)?;
        Self::validate_dt(dt)?;
        let u = c
            .units
            .iter()
            .find(|u| u.id == building)
            .ok_or(Error::InvalidInput("building missing"))?;
        let mut next = self.buildings.get(&building).cloned();
        let completed = if let Some(next) = &mut next {
            Self::building_step(u, c.sudden_death, next, dt)?
        } else {
            None
        };
        Ok(PreparedBuildingStep {
            state: self,
            building,
            next,
            completed,
        })
    }
    /// Component convenience: research first, then training in borrowed unit order.
    /// Match scheduling uses the split phases at their actual source positions.
    pub fn advance(
        &mut self,
        c: &Context<'_>,
        wallets: &EconomyState,
        dt: f64,
    ) -> Result<StepEvents, Error> {
        self.validate(c, wallets)?;
        Self::validate_dt(dt)?;
        let mut next = self.clone();
        let mut out = StepEvents::default();
        Self::research_step(&mut next.players, c, wallets, dt, &mut out.research)?;
        for u in c.units {
            if let Some(b) = next.buildings.get_mut(&u.id)
                && let Some(unit) = Self::building_step(u, c.sudden_death, b, dt)?
            {
                out.units.push(unit);
            }
        }
        *self = next;
        Ok(out)
    }
}
