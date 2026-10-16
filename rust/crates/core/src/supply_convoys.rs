//! Supply stores, ownership, automatic jobs and delivery over caller-owned units and reserves.
//!
//! Store positions are the reference's 40-tick synchronization cache, not authoritative unit
//! positions. World supplies current unit facts and applies typed movement/spawn operations.
//! This module does not own wallets, units, terrain, pathfinding or a network contract.
#[cfg(test)]
mod alert_tests;
#[cfg(test)]
mod manual_tests;

use crate::{
    game::Point,
    grid::UnitId,
    logistics::{LogisticsState, Stock},
    math,
};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[cfg(test)]
mod route_intent_tests;

#[cfg(test)]
mod intent_tests;

const KEYS: usize = 3;
const SOURCE_RADIUS: f64 = 15.0;
const TRUCK_RADIUS: f64 = 12.0;
const UNLOAD: f64 = 8.0;
const REFILL: f64 = 40.0;
const GRACE: f64 = 10.0;
const LOAD: f64 = 4.0;
const ROUTES: usize = 3;
fn items(s: Stock) -> [f64; KEYS] {
    [s.ammo, s.provisions, s.fuel]
}
fn stock(v: [f64; KEYS]) -> Stock {
    Stock {
        ammo: v[0],
        provisions: v[1],
        fuel: v[2],
    }
}
fn quantity(s: Stock) -> f64 {
    items(s).into_iter().fold(0.0, |n, v| n + math::max(0.0, v))
}
fn empty() -> Stock {
    Stock::default()
}
fn distance(a: Point, b: Point) -> f64 {
    math::hypot(a.x - b.x, a.z - b.z)
}
fn capacity(pop: f64, equivalents: f64) -> Stock {
    let scale = math::max(0.01, pop);
    Stock {
        ammo: equivalents * 3.0 * scale,
        provisions: equivalents * 120.0 * scale,
        fuel: equivalents * 180.0 * scale,
    }
}
fn moved(from: &mut Stock, to: &mut Stock, limits: Stock) -> f64 {
    let mut a = items(*from);
    let mut b = items(*to);
    let l = items(limits);
    let mut total = 0.0;
    for k in 0..KEYS {
        let q = math::min(math::max(0.0, a[k]), math::max(0.0, l[k]));
        a[k] -= q;
        b[k] += q;
        total += q;
    }
    *from = stock(a);
    *to = stock(b);
    total
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum StoreId {
    Source(u32),
    Cache(UnitId),
    Support(UnitId),
    Hospital(usize),
    Point(usize),
}
impl StoreId {
    pub fn key(&self) -> String {
        match self {
            Self::Source(i) => format!("source:{i}"),
            Self::Cache(i) => format!("cache:{i}"),
            Self::Support(i) => format!("support:{i}"),
            Self::Hospital(i) => format!("hospital:{i}"),
            Self::Point(i) => format!("point:{i}"),
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct Store {
    pub id: StoreId,
    pub owner: usize,
    pub at: Point,
    pub source: bool,
    pub entity: Option<UnitId>,
    pub capacity: Stock,
    pub buckets: Vec<(usize, Stock)>,
    pub donations: Vec<(usize, Vec<(usize, Stock)>)>,
    pub active: bool,
    pub support: bool,
    pub point: Option<usize>,
}
impl Store {
    fn bucket(&mut self, owner: usize) -> &mut Stock {
        if let Some(i) = self.buckets.iter().position(|(p, _)| *p == owner) {
            return &mut self.buckets[i].1;
        }
        self.buckets.push((owner, empty()));
        &mut self.buckets.last_mut().unwrap().1
    }
    fn grant(&mut self, payer: usize, recipient: usize) -> &mut Stock {
        let i = if let Some(i) = self.donations.iter().position(|(p, _)| *p == payer) {
            i
        } else {
            self.donations.push((payer, vec![]));
            self.donations.len() - 1
        };
        let rows = &mut self.donations[i].1;
        let j = if let Some(j) = rows.iter().position(|(p, _)| *p == recipient) {
            j
        } else {
            rows.push((recipient, empty()));
            rows.len() - 1
        };
        &mut rows[j].1
    }
    fn pools(&mut self, owner: usize, relief: bool) -> Vec<(usize, Stock, Option<usize>)> {
        let own = *self.bucket(owner);
        let mut private = items(own);
        if let Some((_, grants)) = self.donations.iter().find(|(p, _)| *p == owner) {
            for (_, grant) in grants {
                for (k, v) in items(*grant).iter().enumerate() {
                    private[k] = math::max(0.0, private[k] - v);
                }
            }
        }
        let mut out = vec![(owner, stock(private), None)];
        if relief {
            let grants: Vec<_> = self
                .donations
                .iter()
                .filter_map(|(payer, rows)| {
                    rows.iter()
                        .find(|(p, _)| *p == owner)
                        .map(|(_, s)| (*payer, *s))
                })
                .collect();
            for (payer, s) in grants {
                self.bucket(payer);
                out.push((payer, s, Some(owner)));
            }
        }
        out
    }
    pub fn available(&mut self, owner: usize, relief: bool) -> Stock {
        let mut out = [0.0; KEYS];
        for (payer, limit, _) in self.pools(owner, relief) {
            let s = items(*self.bucket(payer));
            for (k, l) in items(limit).iter().enumerate() {
                out[k] += math::min(s[k], *l);
            }
        }
        stock(out)
    }
    pub fn consume(&mut self, owner: usize, amounts: Stock, relief: bool) {
        let mut left = items(amounts).map(|v| {
            if v.is_finite() {
                math::max(0.0, v)
            } else {
                0.0
            }
        });
        for (payer, limit, grant) in self.pools(owner, relief) {
            let mut s = items(*self.bucket(payer));
            let mut used = [0.0; KEYS];
            for k in 0..KEYS {
                let q = math::min(math::min(math::max(0.0, left[k]), s[k]), items(limit)[k]);
                s[k] -= q;
                left[k] -= q;
                used[k] = q;
            }
            *self.bucket(payer) = stock(s);
            if let Some(recipient) = grant {
                let mut g = items(*self.grant(payer, recipient));
                for k in 0..KEYS {
                    g[k] -= used[k];
                }
                *self.grant(payer, recipient) = stock(g);
            }
        }
    }
    pub fn total(&self) -> Stock {
        let mut out = [0.0; KEYS];
        for (_, s) in &self.buckets {
            for (k, v) in items(*s).iter().enumerate() {
                out[k] += v;
            }
        }
        stock(out)
    }
    pub fn space(&self) -> Stock {
        let cap = items(self.capacity);
        let used = items(self.total());
        stock(std::array::from_fn(|k| math::max(0.0, cap[k] - used[k])))
    }
    pub fn fill_source(&mut self, owner: usize, need: Stock, limit: Stock) -> Stock {
        let mut b = items(*self.bucket(owner));
        let n = items(need);
        let l = items(limit);
        let out = std::array::from_fn(|k| {
            let q = math::min(math::max(0.0, n[k]), l[k]);
            b[k] -= math::min(b[k], q);
            q
        });
        *self.bucket(owner) = stock(b);
        stock(out)
    }
    pub fn deposit(
        &mut self,
        payer: usize,
        recipient: Option<usize>,
        cargo: &mut Stock,
        limits: Stock,
    ) -> Stock {
        let before = *cargo;
        moved(cargo, self.bucket(payer), limits);
        let a = items(before);
        let b = items(*cargo);
        let unloaded = stock(std::array::from_fn(|k| a[k] - b[k]));
        if let Some(recipient) = recipient {
            let grant = self.grant(payer, recipient);
            let g = items(*grant);
            let u = items(unloaded);
            *grant = stock(std::array::from_fn(|k| g[k] + u[k]));
        }
        unloaded
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitRole {
    Hq,
    Cache,
    Halftrack,
    Truck,
    Other,
}
#[derive(Clone, Debug)]
pub struct UnitView {
    /// A temporary projected pose, rather than the live unit object.
    pub projected: bool,
    pub id: UnitId,
    pub owner: Option<usize>,
    pub role: UnitRole,
    pub hp: f64,
    pub built: f64,
    pub at: Point,
    pub still: f64,
    pub cargo_len: usize,
    pub region_cache: bool,
    pub riding: Option<UnitId>,
    pub path: Vec<Point>,
    pub world_goal: Option<Point>,
    pub orders: Vec<Point>,
    /// Presence includes supported nonmovement orders; destinations above remain exact.
    pub orders_pending: bool,
    pub amove: Option<Point>,
    pub stuck: f64,
}
impl UnitView {
    fn has_orders(&self) -> bool {
        self.orders_pending || !self.orders.is_empty()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Player {
    pub slot: usize,
    pub team: i32,
    pub spawn: Point,
    pub out: bool,
    pub away: bool,
}
#[derive(Clone, Copy, Debug)]
pub struct DepotPoint {
    pub at: Point,
    pub owner: Option<usize>,
    pub contested: bool,
    pub depot: bool,
}
#[derive(Clone, Copy)]
pub struct Context<'a> {
    pub enabled: bool,
    pub construction: bool,
    pub tick: u64,
    pub winner: bool,
    pub army_pop: f64,
    pub players: &'a [Player],
    pub width: usize,
    pub chars: &'a [u8],
    pub aid: &'a [(usize, usize)],
    pub points: &'a [DepotPoint],
}
fn friendly(ctx: Context<'_>, a: usize, b: usize) -> bool {
    ctx.players
        .get(a)
        .zip(ctx.players.get(b))
        .is_some_and(|(a, b)| a.team == b.team)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TruckState {
    Idle,
    Collecting,
    Loading,
    Delivering,
    Unloading,
    Returning,
    Waiting,
    WaitingCollect,
    WaitingReturn,
    Manual,
    Held,
}
impl TruckState {
    pub fn name(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Collecting => "collecting",
            Self::Loading => "loading",
            Self::Delivering => "delivering",
            Self::Unloading => "unloading",
            Self::Returning => "returning",
            Self::Waiting => "waiting",
            Self::WaitingCollect => "waitingCollect",
            Self::WaitingReturn => "waitingReturn",
            Self::Manual => "manual",
            Self::Held => "held",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum TargetId {
    Unit(UnitId),
    Store(StoreId),
}
#[derive(Clone, Debug, PartialEq)]
pub struct Target {
    pub id: TargetId,
    pub unit: Option<UnitId>,
    pub store: Option<StoreId>,
    pub owner: usize,
    pub at: Point,
    pub members: Vec<UnitId>,
    pub need: Stock,
    pub priority: Option<f64>,
    pub relief: Option<bool>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Retry {
    pub key: (i64, i64),
    pub wait: f64,
    pub at: u64,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Truck {
    pub state: TruckState,
    pub cargo: Stock,
    pub target: Option<Target>,
    pub origin: StoreId,
    pub timer: f64,
    pub manual: bool,
    pub hold: bool,
    pub route: Vec<Point>,
    pub destination: Option<Point>,
    pub route_failure: Option<RouteFailure>,
    pub retry: Option<Retry>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RouteFailure {
    Dangerous,
    Blocked,
    Unreachable,
}
#[derive(Clone, Debug)]
pub struct Route {
    /// A findPath effect. None retains the live unit's previous endpoint/history.
    pub route_end: Option<Point>,
    pub path: Vec<Point>,
    pub failure: Option<RouteFailure>,
}
#[derive(Clone, Debug, PartialEq)]
pub enum UnitOperation {
    Route {
        path: Vec<Point>,
        world_goal: Point,
        repath: f64,
    },
    ClearRoute,
    ResumeClear,
    RepairRoute(Vec<Point>),
    NewTruck,
}
/// Hooks inspect only the caller's supplied authoritative/recipient terrain and update caller-owned units.
/// `units` must preserve authoritative insertion order. `spawn`, `retire` and `apply` take effect
/// immediately, so subsequent `units` and `unit` calls in the same tick see those changes.
/// Route operations also clear retreating, attackId and targetId as in sendTruck; NewTruck clears autoRetreat.
pub trait World {
    fn units(&self) -> Vec<UnitView>;
    fn unit(&self, id: UnitId) -> Option<UnitView>;
    fn route(&mut self, u: &UnitView, at: Point, safe: bool) -> Route;
    fn continue_route(&mut self, _u: &UnitView, _target: Point) -> bool {
        false
    }
    fn handoff(&mut self, u: &UnitView, at: Point, entity: Option<UnitId>, radius: f64) -> bool;
    fn supply_rate(&mut self, _owner: usize, _at: Point) -> Option<f64> {
        None
    }
    fn spawn(&mut self, owner: usize, at: Point) -> UnitId;
    fn retire(&mut self, id: UnitId);
    fn apply(&mut self, id: UnitId, operation: UnitOperation);
}
#[derive(Clone, Debug, PartialEq)]
pub struct Event {
    pub id: u64,
    pub owner: usize,
    pub kind: String,
    pub count: usize,
    pub at: Option<Point>,
    pub message: String,
    pub seconds: f64,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Assignment {
    pub targets: Vec<TargetId>,
    pub target: usize,
    pub origins: Option<Vec<StoreId>>,
    pub origin: usize,
    pub blocked: bool,
    pub no_stock: bool,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ConvoyState {
    pub stores: Vec<Store>,
    pub next_spawn: BTreeMap<usize, f64>,
    pub losses: BTreeMap<usize, Vec<f64>>,
    pub events: Vec<Event>,
    pub alerts: BTreeMap<(usize, String), f64>,
    pub next_event: u64,
    pub next_schedule: u64,
    pub refresh_queue: VecDeque<UnitId>,
    pub schedule_queue: VecDeque<usize>,
    pub scheduled: BTreeSet<UnitId>,
    pub assignments: BTreeMap<UnitId, Assignment>,
    pub routed: usize,
    pub jobs: BTreeMap<usize, usize>,
    pub trucks: BTreeMap<UnitId, Truck>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConvoyError {
    InvalidContext,
    InvalidTime,
    InvalidQuantity,
    InvalidStore,
    UnsupportedLegacyRegionStore,
}
impl ConvoyState {
    pub fn setup(enabled: bool) -> Option<Self> {
        enabled.then(|| Self {
            next_event: 1,
            ..Self::default()
        })
    }
    fn store(&self, id: &StoreId) -> Option<&Store> {
        self.stores.iter().find(|s| &s.id == id)
    }
    pub fn store_mut(&mut self, id: &StoreId) -> Option<&mut Store> {
        self.stores.iter_mut().find(|s| &s.id == id)
    }
    #[allow(clippy::too_many_arguments)]
    fn ensure(
        &mut self,
        ctx: Context<'_>,
        id: StoreId,
        owner: usize,
        at: Point,
        source: bool,
        entity: Option<UnitId>,
        equivalents: f64,
    ) -> usize {
        if let Some(i) = self
            .stores
            .iter()
            .position(|s| s.id == id && s.owner != owner)
        {
            self.stores.remove(i);
        }
        let i = if let Some(i) = self.stores.iter().position(|s| s.id == id) {
            i
        } else {
            self.stores.push(Store {
                id,
                owner,
                at,
                source,
                entity,
                capacity: capacity(ctx.army_pop, equivalents),
                buckets: vec![],
                donations: vec![],
                active: true,
                support: false,
                point: None,
            });
            self.stores.len() - 1
        };
        let s = &mut self.stores[i];
        s.owner = owner;
        s.at = at;
        s.source = source;
        s.entity = entity;
        s.active = true;
        i
    }
    /// Synchronize current store identities while preserving source insertion order and captured locations.
    pub fn sync_stores<W: World>(
        &mut self,
        ctx: Context<'_>,
        world: &W,
    ) -> Result<(), ConvoyError> {
        validate(ctx)?;
        let units = world.units();
        for s in &mut self.stores {
            s.active = false;
        }
        if ctx.construction || units.iter().any(|u| u.role == UnitRole::Hq) {
            for u in &units {
                if u.role == UnitRole::Hq
                    && u.hp > 0.0
                    && u.built >= 1.0
                    && u.owner
                        .is_some_and(|o| ctx.players.get(o).is_none_or(|p| !p.out))
                {
                    self.ensure(
                        ctx,
                        StoreId::Source(u.id),
                        u.owner.unwrap(),
                        u.at,
                        true,
                        Some(u.id),
                        32.0,
                    );
                }
            }
        } else {
            for p in ctx.players {
                if !p.out && !p.away {
                    self.ensure(
                        ctx,
                        StoreId::Source(p.slot as u32),
                        p.slot,
                        p.spawn,
                        true,
                        None,
                        32.0,
                    );
                }
            }
        }
        for u in &units {
            let Some(owner) = u.owner else {
                continue;
            };
            if u.hp <= 0.0 {
                continue;
            }
            if u.role == UnitRole::Cache && u.built >= 1.0 && !u.region_cache {
                self.ensure(
                    ctx,
                    StoreId::Cache(u.id),
                    owner,
                    u.at,
                    false,
                    Some(u.id),
                    32.0,
                );
            }
            if u.role == UnitRole::Halftrack {
                let id = StoreId::Support(u.id);
                let fresh = self.store(&id).is_none();
                let i = self.ensure(ctx, id, owner, u.at, false, Some(u.id), 4.0);
                let s = &mut self.stores[i];
                if fresh {
                    let cap = s.capacity;
                    *s.bucket(owner) = Stock {
                        ammo: cap.ammo,
                        provisions: cap.provisions,
                        fuel: 0.0,
                    };
                }
                s.support = true;
                s.active = u.still >= 2.0 || u.cargo_len > 0;
            }
        }
        for &(cell, owner) in ctx.aid {
            if ctx.chars[cell] != b'A' {
                continue;
            }
            let id = StoreId::Hospital(cell);
            let fresh = self.store(&id).is_none();
            let at = Point {
                x: (cell % ctx.width) as f64 * 2.0 + 1.0,
                z: (cell / ctx.width) as f64 * 2.0 + 1.0,
            };
            let i = self.ensure(ctx, id, owner, at, false, None, 4.0);
            let s = &mut self.stores[i];
            if fresh {
                let cap = s.capacity;
                *s.bucket(owner) = Stock {
                    ammo: cap.ammo,
                    provisions: cap.provisions,
                    fuel: 0.0,
                };
            }
            s.support = true;
        }
        for (i, p) in ctx.points.iter().enumerate() {
            if p.depot
                && !p.contested
                && let Some(owner) = p.owner
            {
                let j = self.ensure(ctx, StoreId::Point(i), owner, p.at, false, None, 32.0);
                self.stores[j].point = Some(i);
            }
        }
        self.stores.retain(|s| {
            s.active
                || s.point.is_some_and(|i| {
                    ctx.points
                        .get(i)
                        .is_some_and(|p| p.owner == Some(s.owner) && p.contested)
                })
        });
        Ok(())
    }
    fn sources(&self, ctx: Context<'_>, owner: usize) -> Vec<StoreId> {
        let mut s: Vec<_> = self
            .stores
            .iter()
            .filter(|s| s.active && s.source && friendly(ctx, s.owner, owner))
            .collect();
        s.sort_by_key(|s| s.owner != owner);
        s.into_iter().map(|s| s.id.clone()).collect()
    }
    fn alert(
        &mut self,
        ctx: Context<'_>,
        owner: usize,
        kind: &str,
        message: String,
        at: Option<Point>,
        count: usize,
    ) {
        let now = ctx.tick as f64 / 20.0;
        let key = (owner, kind.to_owned());
        if now - self.alerts.get(&key).copied().unwrap_or(f64::NEG_INFINITY) < 20.0 {
            return;
        }
        self.alerts.insert(key, now);
        self.events.push(Event {
            id: self.next_event,
            owner,
            kind: kind.to_owned(),
            count,
            at,
            message,
            seconds: now,
        });
        self.next_event += 1;
        self.events.retain(|e| now - e.seconds <= 30.0);
        if self.events.len() > 128 {
            self.events.drain(..self.events.len() - 128);
        }
    }
    fn route_alert(&mut self, ctx: Context<'_>, owner: usize, c: &Truck, at: Point) {
        let (kind, message) = match c.route_failure.unwrap_or(RouteFailure::Unreachable) {
            RouteFailure::Dangerous => ("dangerous", "Supply delivery is waiting for a safe route"),
            RouteFailure::Blocked => ("blocked", "Supply route is blocked"),
            RouteFailure::Unreachable => ("unreachable", "No reachable ground route for supplies"),
        };
        self.alert(ctx, owner, kind, message.into(), Some(at), 1);
    }
}
fn validate(ctx: Context<'_>) -> Result<(), ConvoyError> {
    if ctx.tick > 9_007_199_254_740_351
        || ctx.width == 0
        || !ctx.chars.len().is_multiple_of(ctx.width)
        || !ctx.army_pop.is_finite()
        || ctx
            .players
            .iter()
            .enumerate()
            .any(|(i, p)| p.slot != i || !p.spawn.x.is_finite() || !p.spawn.z.is_finite())
        || ctx
            .aid
            .iter()
            .any(|(c, o)| *c >= ctx.chars.len() || *o >= ctx.players.len())
        || ctx.points.iter().any(|p| {
            !p.at.x.is_finite()
                || !p.at.z.is_finite()
                || p.owner.is_some_and(|o| o >= ctx.players.len())
        })
    {
        Err(ConvoyError::InvalidContext)
    } else {
        Ok(())
    }
}
fn missing(reserves: &LogisticsState, id: UnitId) -> Stock {
    let r = reserves.get(id).unwrap();
    Stock {
        ammo: math::max(
            0.0,
            (r.ammo_max - r.ammo) * r.ammo_price - r.ammo_credit.unwrap_or(0.0),
        ),
        provisions: math::max(0.0, 120.0 - r.provisions),
        fuel: r.fuel.map_or(0.0, |f| {
            math::max(0.0, 180.0 - f) + math::max(0.0, 60.0 - r.emergency.unwrap_or(0.0))
        }),
    }
}
impl ConvoyState {
    fn local_service<W: World>(
        &mut self,
        ctx: Context<'_>,
        reserves: &mut LogisticsState,
        world: &mut W,
        dt: f64,
    ) {
        let stores: Vec<_> = self
            .stores
            .iter()
            .filter(|s| s.active)
            .map(|s| s.id.clone())
            .collect();
        for u in world.units() {
            let Some(owner) = u.owner else {
                continue;
            };
            if reserves.get(u.id).is_none() || u.hp <= 0.0 {
                continue;
            }
            let host = u.riding.and_then(|id| world.unit(id)).unwrap_or(u.clone());
            let mut nearby: Vec<_> = stores
                .iter()
                .filter_map(|id| {
                    self.store(id)
                        .filter(|s| {
                            friendly(ctx, s.owner, owner)
                                && distance(s.at, host.at) <= SOURCE_RADIUS
                                && world.handoff(&u, s.at, s.entity, SOURCE_RADIUS)
                        })
                        .map(|s| (s.id.clone(), distance(s.at, host.at)))
                })
                .collect();
            nearby.sort_by(|a, b| a.1.total_cmp(&b.1));
            for (id, _) in nearby {
                let s = self.store_mut(&id).unwrap();
                if s.source {
                    let r = reserves.get(u.id).unwrap();
                    let limit = Stock {
                        ammo: r.ammo_max * r.ammo_price * dt / UNLOAD,
                        provisions: 120.0 * dt / UNLOAD,
                        fuel: 240.0 * dt / UNLOAD,
                    };
                    let need = missing(reserves, u.id);
                    let mut stock = s.fill_source(owner, need, limit);
                    reserves.transfer_stock(u.id, Some(&mut stock), dt);
                    let copy = stock;
                    moved(&mut stock, s.bucket(owner), copy);
                } else {
                    let mut stock = s.available(owner, true);
                    let used = reserves.transfer_stock(u.id, Some(&mut stock), dt);
                    s.consume(owner, used, true);
                }
                if reserves.recovered(u.id) {
                    break;
                }
            }
        }
    }
    fn territory_service<W: World>(
        &mut self,
        ctx: Context<'_>,
        reserves: &mut LogisticsState,
        world: &mut W,
        dt: f64,
    ) {
        let mut cut: Vec<(usize, Vec<UnitView>)> = vec![];
        for u in world.units() {
            let Some(owner) = u.owner else {
                continue;
            };
            if reserves.get(u.id).is_none() || u.hp <= 0.0 {
                continue;
            }
            let host = u.riding.and_then(|id| world.unit(id)).unwrap_or(u.clone());
            let Some(rate) = world.supply_rate(owner, host.at) else {
                continue;
            };
            let r = reserves.get_mut(u.id).unwrap();
            let was_cut = r.cut_for.unwrap_or(0.0) >= GRACE;
            if rate > 0.0 {
                r.supply_rate = Some(rate);
                r.cut_for = Some(0.0);
            } else {
                r.cut_for = Some(r.cut_for.unwrap_or(0.0) + dt);
            }
            let is_cut = r.cut_for.unwrap() >= GRACE;
            if is_cut && !was_cut {
                if let Some((_, units)) = cut.iter_mut().find(|(p, _)| *p == owner) {
                    units.push(u.clone());
                } else {
                    cut.push((owner, vec![u.clone()]));
                }
            }
            if was_cut && !is_cut {
                self.alert(
                    ctx,
                    owner,
                    "restored",
                    "Supply line restored".into(),
                    Some(u.at),
                    1,
                );
            }
            let share = if is_cut {
                0.0
            } else {
                r.supply_rate.unwrap_or(0.0) * dt / REFILL
            };
            let need = missing(reserves, u.id);
            if share <= 0.0 || quantity(need) < 1e-6 {
                continue;
            }
            let r = reserves.get(u.id).unwrap();
            let limits = items(Stock {
                ammo: r.ammo_max * r.ammo_price * share,
                provisions: 120.0 * share,
                fuel: 240.0 * share,
            });
            let n = items(need);
            let mut stock = stock(std::array::from_fn(|k| math::min(n[k], limits[k])));
            reserves.transfer_stock(u.id, Some(&mut stock), dt);
        }
        for (owner, units) in cut {
            let n = units.len();
            self.alert(
                ctx,
                owner,
                "cut",
                format!(
                    "{n} unit{} cut off from supply",
                    if n == 1 { "" } else { "s" }
                ),
                Some(units[0].at),
                n,
            );
        }
        for s in &mut self.stores {
            if !s.active || s.source {
                continue;
            }
            let Some(rate) = world.supply_rate(s.owner, s.at) else {
                continue;
            };
            if rate <= 0.0 {
                continue;
            }
            let space = items(s.space());
            let cap = items(s.capacity);
            let share = rate * dt / REFILL;
            let b = s.bucket(s.owner);
            let old = items(*b);
            *b = stock(std::array::from_fn(|k| {
                old[k] + math::min(space[k], cap[k] * share)
            }));
        }
    }
    fn send<W: World>(
        &mut self,
        ctx: Context<'_>,
        u: &UnitView,
        c: &mut Truck,
        target: Point,
        world: &mut W,
        manual: bool,
    ) -> bool {
        let d = distance(u.at, target);
        let divisor = if d == 0.0 { 1.0 } else { d };
        let gap = math::min(9.0, divisor);
        let at = Point {
            x: target.x + (u.at.x - target.x) / divisor * gap,
            z: target.z + (u.at.z - target.z) / divisor * gap,
        };
        let key = (
            math::round(at.x / 8.0) as i64,
            math::round(at.z / 8.0) as i64,
        );
        if !manual
            && c.retry
                .as_ref()
                .is_some_and(|r| r.key == key && ctx.tick < r.at)
        {
            return false;
        }
        let route = world.route(u, at, !manual);
        self.routed += 1;
        if route.path.is_empty() && distance(u.at, at) > TRUCK_RADIUS {
            c.route_failure = Some(route.failure.unwrap_or(RouteFailure::Unreachable));
            let wait = c
                .retry
                .as_ref()
                .filter(|r| r.key == key)
                .map_or(2.0, |r| math::min(32.0, r.wait * 2.0));
            c.retry = Some(Retry {
                key,
                wait,
                at: ctx.tick + (wait * 20.0) as u64,
            });
            return false;
        }
        c.retry = None;
        c.route_failure = None;
        c.destination = Some(target);
        c.route = route.path.clone();
        world.apply(
            u.id,
            UnitOperation::Route {
                path: route.path,
                world_goal: at,
                repath: 2.0,
            },
        );
        true
    }
    fn targets<W: World>(
        &mut self,
        ctx: Context<'_>,
        owner: usize,
        reserves: &LogisticsState,
        world: &mut W,
    ) -> Vec<Target> {
        let units = world.units();
        let mut occupied = BTreeSet::new();
        for u in &units {
            if u.role == UnitRole::Truck
                && u.owner == Some(owner)
                && u.hp > 0.0
                && let Some(c) = self.trucks.get(&u.id)
                && !matches!(
                    c.state,
                    TruckState::Idle | TruckState::Returning | TruckState::WaitingReturn
                )
                && let Some(target) = &c.target
            {
                occupied.insert(target.id.clone());
                for id in &target.members {
                    occupied.insert(TargetId::Unit(*id));
                }
            }
        }
        let mut troops: Vec<_> = units
            .iter()
            .filter(|u| u.owner == Some(owner) && u.hp > 0.0 && reserves.get(u.id).is_some())
            .collect();
        let mut assigned = BTreeSet::new();
        troops.sort_by(|a, b| {
            reserves
                .get(a.id)
                .unwrap()
                .provisions
                .total_cmp(&reserves.get(b.id).unwrap().provisions)
        });
        let mut targets = vec![];
        for u in troops.clone() {
            if assigned.contains(&u.id) || occupied.contains(&TargetId::Unit(u.id)) {
                continue;
            }
            let r = reserves.get(u.id).unwrap();
            if r.cut_for.unwrap_or(0.0) < GRACE {
                continue;
            }
            if r.provisions > 90.0
                && (r.ammo_max == 0.0 || r.ammo / r.ammo_max > 0.75)
                && r.fuel.is_none_or(|f| f > 135.0)
                && !r.forced
            {
                continue;
            }
            let members: Vec<_> = troops
                .iter()
                .filter(|t| {
                    !assigned.contains(&t.id)
                        && !occupied.contains(&TargetId::Unit(t.id))
                        && distance(t.at, u.at) <= 10.0
                })
                .copied()
                .collect();
            let mut need = [0.0; KEYS];
            for t in &members {
                assigned.insert(t.id);
                for (k, v) in items(missing(reserves, t.id)).iter().enumerate() {
                    need[k] += v;
                }
            }
            targets.push(Target {
                id: TargetId::Unit(u.id),
                unit: Some(u.id),
                store: None,
                owner,
                at: u.at,
                members: members.iter().map(|u| u.id).collect(),
                need: stock(need),
                priority: Some(r.provisions + if r.forced { -200.0 } else { 0.0 }),
                relief: None,
            });
        }
        for s in &mut self.stores {
            if !s.active
                || s.source
                || !friendly(ctx, s.owner, owner)
                || occupied.contains(&TargetId::Store(s.id.clone()))
            {
                continue;
            }
            if world.supply_rate(s.owner, s.at).is_some_and(|r| r > 0.0)
                || !troops.iter().any(|u| distance(u.at, s.at) < 200.0)
            {
                continue;
            }
            let b = items(s.available(owner, true));
            let cap = items(s.capacity);
            let space = items(s.space());
            let need = stock(std::array::from_fn(|k| {
                math::min(space[k], math::max(0.0, cap[k] * 0.5 - b[k]))
            }));
            if quantity(need) > 1.0 {
                targets.push(Target {
                    id: TargetId::Store(s.id.clone()),
                    unit: None,
                    store: Some(s.id.clone()),
                    owner,
                    at: s.at,
                    members: vec![],
                    need,
                    priority: Some(if s.support { 170.0 } else { 180.0 }),
                    relief: None,
                });
            }
        }
        targets.sort_by(|a, b| {
            a.priority
                .unwrap_or(0.0)
                .total_cmp(&b.priority.unwrap_or(0.0))
        });
        troops.clear();
        targets
    }
    fn origins(&mut self, ctx: Context<'_>, owner: usize, target: &Target) -> Vec<StoreId> {
        let roots: Vec<_> = self
            .sources(ctx, owner)
            .iter()
            .filter_map(|id| self.store(id).map(|s| s.at))
            .collect();
        let root_distance = |at: Point| {
            roots
                .iter()
                .fold(f64::INFINITY, |n, r| math::min(n, distance(*r, at)))
        };
        let mut out = vec![];
        for s in &mut self.stores {
            if !s.active
                || !friendly(ctx, s.owner, owner)
                || TargetId::Store(s.id.clone()) == target.id
            {
                continue;
            }
            if !s.source && quantity(s.available(owner, false)) <= 1.0 {
                continue;
            }
            if target.store.is_some()
                && !s.source
                && root_distance(s.at) + 8.0 >= root_distance(target.at)
            {
                continue;
            }
            out.push((s.id.clone(), distance(s.at, target.at)));
        }
        out.sort_by(|a, b| a.1.total_cmp(&b.1));
        out.into_iter().map(|r| r.0).collect()
    }
    #[allow(clippy::too_many_arguments)]
    fn assign<W: World>(
        &mut self,
        ctx: Context<'_>,
        u: &UnitView,
        c: &mut Truck,
        target: Target,
        origin: Store,
        world: &mut W,
        manual: bool,
    ) -> bool {
        let before = c.clone();
        c.target = Some(target.clone());
        c.origin = origin.id;
        c.manual = manual;
        c.hold = false;
        c.timer = LOAD;
        let success = if quantity(c.cargo) > 0.0 {
            if self.send(ctx, u, c, target.at, world, manual) {
                c.state = TruckState::Delivering;
                true
            } else {
                false
            }
        } else if distance(u.at, origin.at) > SOURCE_RADIUS
            || !world.handoff(u, origin.at, origin.entity, SOURCE_RADIUS)
        {
            if self.send(ctx, u, c, origin.at, world, manual) {
                c.state = TruckState::Collecting;
                true
            } else {
                false
            }
        } else if self.send(ctx, u, c, target.at, world, manual) {
            c.state = TruckState::Loading;
            world.apply(u.id, UnitOperation::ClearRoute);
            true
        } else {
            false
        };
        if !success {
            let failure = c.route_failure;
            let retry = c.retry.take();
            *c = before;
            c.route_failure = failure;
            c.retry = retry;
        }
        success
    }
    fn load_job<W: World>(&mut self, ctx: Context<'_>, u: &UnitView, c: &mut Truck, world: &mut W) {
        let Some(origin) = self.store_mut(&c.origin).filter(|s| s.active) else {
            c.state = TruckState::Idle;
            return;
        };
        let Some(target) = c.target.clone() else {
            c.state = TruckState::Idle;
            return;
        };
        let cap = capacity(ctx.army_pop, 8.0);
        let need = target.need;
        if origin.source {
            c.cargo = origin.fill_source(u.owner.unwrap(), need, cap);
        } else {
            c.cargo = empty();
            let mut s = origin.available(u.owner.unwrap(), false);
            let n = items(need);
            let cap = items(cap);
            moved(
                &mut s,
                &mut c.cargo,
                stock(std::array::from_fn(|k| math::min(n[k], cap[k]))),
            );
            origin.consume(u.owner.unwrap(), c.cargo, false);
        }
        if quantity(c.cargo) == 0.0 {
            c.state = TruckState::Idle;
            c.target = None;
            self.alert(
                ctx,
                u.owner.unwrap(),
                "stock",
                "No affordable supply stock is available for loading".into(),
                Some(u.at),
                1,
            );
            return;
        }
        c.state = TruckState::Delivering;
        if !self.send(ctx, u, c, target.at, world, c.manual) {
            c.state = TruckState::Waiting;
            self.route_alert(ctx, u.owner.unwrap(), c, target.at);
        }
    }
    fn finish<W: World>(&mut self, ctx: Context<'_>, u: &UnitView, c: &mut Truck, world: &mut W) {
        c.target = None;
        c.manual = false;
        let owner = u.owner.unwrap();
        let origin = self
            .store(&c.origin)
            .filter(|s| s.active && friendly(ctx, s.owner, owner))
            .cloned()
            .or_else(|| {
                let mut s: Vec<_> = self
                    .sources(ctx, owner)
                    .iter()
                    .filter_map(|id| self.store(id).cloned())
                    .collect();
                s.sort_by(|a, b| distance(a.at, u.at).total_cmp(&distance(b.at, u.at)));
                s.into_iter().next()
            });
        let Some(origin) = origin else {
            c.state = TruckState::WaitingReturn;
            c.hold = false;
            c.route.clear();
            world.apply(u.id, UnitOperation::ClearRoute);
            return;
        };
        c.state = TruckState::Returning;
        c.origin = origin.id;
        if !self.send(ctx, u, c, origin.at, world, false) {
            c.state = TruckState::WaitingReturn;
            world.apply(u.id, UnitOperation::ClearRoute);
        }
    }
    fn deliver<W: World>(
        &mut self,
        ctx: Context<'_>,
        u: &UnitView,
        c: &mut Truck,
        reserves: &mut LogisticsState,
        world: &mut W,
        dt: f64,
    ) {
        let Some(mut target) = c.target.clone() else {
            self.finish(ctx, u, c, world);
            return;
        };
        if let Some(id) = &target.store {
            let Some(s) = self
                .store(id)
                .filter(|s| s.active && friendly(ctx, s.owner, u.owner.unwrap()))
                .cloned()
            else {
                self.finish(ctx, u, c, world);
                return;
            };
            if distance(u.at, s.at) > TRUCK_RADIUS
                || !world.handoff(u, s.at, s.entity, TRUCK_RADIUS)
            {
                c.timer -= dt;
                if c.timer <= 0.0 {
                    target.at = s.at;
                    c.target = Some(target.clone());
                    c.state = if self.send(ctx, u, c, target.at, world, c.manual) {
                        TruckState::Delivering
                    } else {
                        TruckState::Waiting
                    };
                }
                return;
            }
            let cap = items(capacity(ctx.army_pop, 8.0));
            let s = self.store_mut(id).unwrap();
            let space = items(s.space());
            let limit = stock(std::array::from_fn(|k| {
                math::min(space[k], cap[k] * dt / UNLOAD)
            }));
            s.deposit(
                u.owner.unwrap(),
                (c.manual && target.relief.unwrap_or(false) && s.owner != u.owner.unwrap())
                    .then_some(s.owner),
                &mut c.cargo,
                limit,
            );
        } else {
            let truck_owner = u.owner.unwrap();
            let mut members: Vec<_> = target
                .members
                .iter()
                .filter_map(|id| world.unit(*id))
                .filter(|u| {
                    u.hp > 0.0
                        && reserves.get(u.id).is_some()
                        && u.owner.is_some_and(|o| friendly(ctx, o, truck_owner))
                })
                .collect();
            members.sort_by(|a, b| {
                reserves
                    .get(a.id)
                    .unwrap()
                    .provisions
                    .total_cmp(&reserves.get(b.id).unwrap().provisions)
            });
            let mut served = false;
            for troop in &members {
                let at = troop
                    .riding
                    .and_then(|id| world.unit(id))
                    .map_or(troop.at, |u| u.at);
                if distance(u.at, at) > TRUCK_RADIUS
                    || !world.handoff(troop, u.at, Some(u.id), TRUCK_RADIUS)
                {
                    continue;
                }
                reserves.transfer_stock(troop.id, Some(&mut c.cargo), dt);
                served = true;
            }
            if !served {
                c.timer -= dt;
                if let Some(troop) = members.first() {
                    if c.timer <= 0.0 {
                        target.at = troop.at;
                        c.target = Some(target.clone());
                        c.state = TruckState::Delivering;
                        if !self.send(ctx, u, c, target.at, world, c.manual) {
                            c.state = TruckState::Waiting;
                        }
                    }
                } else {
                    self.finish(ctx, u, c, world);
                }
                return;
            }
        }
        c.timer -= dt;
        if c.timer <= 0.0 || quantity(c.cargo) < 0.001 {
            self.finish(ctx, u, c, world);
        }
    }
    pub fn add_truck<W: World>(&mut self, owner: usize, source: &Store, world: &mut W) -> UnitId {
        let id = world.spawn(owner, source.at);
        world.apply(id, UnitOperation::NewTruck);
        self.trucks.insert(
            id,
            Truck {
                state: TruckState::Idle,
                cargo: empty(),
                target: None,
                origin: source.id.clone(),
                timer: 0.0,
                manual: false,
                hold: false,
                route: vec![],
                destination: None,
                route_failure: None,
                retry: None,
            },
        );
        id
    }
    fn fleet<W: World>(&mut self, ctx: Context<'_>, world: &mut W) {
        let now = ctx.tick as f64 / 20.0;
        for p in ctx.players {
            if p.out || p.away {
                continue;
            }
            let mut active: Vec<_> = world
                .units()
                .into_iter()
                .filter(|u| u.owner == Some(p.slot) && u.hp > 0.0 && u.role == UnitRole::Truck)
                .collect();
            let busy = active
                .iter()
                .filter(|u| {
                    self.trucks
                        .get(&u.id)
                        .is_some_and(|c| c.state != TruckState::Idle || c.manual || c.hold)
                })
                .count();
            let desired = (busy + self.jobs.get(&p.slot).copied().unwrap_or(0)).min(4);
            let mut surplus = active.len().saturating_sub(desired);
            for i in (0..active.len()).rev() {
                if surplus == 0 {
                    break;
                }
                let u = &active[i];
                let Some(c) = self.trucks.get(&u.id) else {
                    continue;
                };
                if c.state != TruckState::Idle
                    || c.manual
                    || c.hold
                    || quantity(c.cargo) > 0.0
                    || !u.path.is_empty()
                    || u.world_goal.is_some()
                    || u.has_orders()
                {
                    continue;
                }
                let at_source = self
                    .sources(ctx, p.slot)
                    .iter()
                    .filter_map(|id| self.store(id))
                    .any(|s| {
                        distance(s.at, u.at) <= SOURCE_RADIUS
                            && world.handoff(u, s.at, s.entity, SOURCE_RADIUS)
                    });
                if !at_source {
                    continue;
                }
                world.retire(u.id);
                self.trucks.remove(&u.id);
                active.remove(i);
                surplus -= 1;
            }
            let losses = self.losses.entry(p.slot).or_default();
            losses.retain(|t| *t > now);
            if active.len() + losses.len() >= desired
                || now < self.next_spawn.get(&p.slot).copied().unwrap_or(0.0)
            {
                continue;
            }
            if let Some(source) = self
                .sources(ctx, p.slot)
                .first()
                .and_then(|id| self.store(id))
                .cloned()
            {
                self.add_truck(p.slot, &source, world);
                self.next_spawn.insert(p.slot, now + 1.0);
            }
        }
    }
}
impl ConvoyState {
    fn schedule_player<W: World>(
        &mut self,
        ctx: Context<'_>,
        p: Player,
        reserves: &LogisticsState,
        world: &mut W,
    ) -> bool {
        let idle: Vec<_> = world
            .units()
            .into_iter()
            .filter(|u| {
                u.owner == Some(p.slot)
                    && u.role == UnitRole::Truck
                    && u.hp > 0.0
                    && self
                        .trucks
                        .get(&u.id)
                        .is_some_and(|c| c.state == TruckState::Idle && !c.manual && !c.hold)
                    && !self.scheduled.contains(&u.id)
            })
            .collect();
        let mut targets = self.targets(ctx, p.slot, reserves, world);
        self.jobs.insert(p.slot, targets.len());
        for u in idle {
            if self.routed >= ROUTES {
                return false;
            }
            let mut pass = self
                .assignments
                .remove(&u.id)
                .unwrap_or_else(|| Assignment {
                    targets: targets.iter().map(|t| t.id.clone()).collect(),
                    target: 0,
                    origins: None,
                    origin: 0,
                    blocked: false,
                    no_stock: false,
                });
            let mut c = self.trucks.remove(&u.id).unwrap();
            let mut assigned = false;
            while pass.target < pass.targets.len() && !assigned {
                if let Some(target) = targets
                    .iter()
                    .find(|t| t.id == pass.targets[pass.target])
                    .cloned()
                {
                    if pass.origins.is_none() {
                        let origins = self.origins(ctx, p.slot, &target);
                        pass.no_stock |= origins.is_empty();
                        pass.origins = Some(origins);
                        pass.origin = 0;
                    }
                    let origins = pass.origins.as_ref().unwrap();
                    while pass.origin < origins.len() {
                        if self.routed >= ROUTES {
                            self.trucks.insert(u.id, c);
                            self.assignments.insert(u.id, pass);
                            return false;
                        }
                        let id = &origins[pass.origin];
                        pass.origin += 1;
                        if let Some(origin) = self
                            .store(id)
                            .filter(|s| s.active && friendly(ctx, s.owner, p.slot))
                            .cloned()
                            && self.assign(ctx, &u, &mut c, target.clone(), origin, world, false)
                        {
                            assigned = true;
                            targets.retain(|t| t.id != target.id);
                            self.jobs.insert(p.slot, targets.len());
                            break;
                        }
                    }
                    pass.blocked |= !origins.is_empty() && !assigned;
                }
                pass.target += 1;
                pass.origins = None;
            }
            self.scheduled.insert(u.id);
            if c.state == TruckState::Idle && !targets.is_empty() {
                if pass.blocked {
                    self.route_alert(ctx, p.slot, &c, targets[0].at);
                } else if pass.no_stock {
                    self.alert(
                        ctx,
                        p.slot,
                        "stock",
                        "No available supply stock or source for delivery".into(),
                        Some(targets[0].at),
                        1,
                    );
                }
            }
            self.trucks.insert(u.id, c);
        }
        let own: Vec<_> = world
            .units()
            .into_iter()
            .filter(|u| u.owner == Some(p.slot) && u.hp > 0.0 && reserves.get(u.id).is_some())
            .collect();
        let critical: Vec<_> = own
            .iter()
            .filter(|u| {
                let r = reserves.get(u.id).unwrap();
                r.provisions < 30.0 || (r.ammo_max != 0.0 && r.ammo / r.ammo_max < 0.25)
            })
            .collect();
        for (kind, rows, message) in [
            ("low", critical.clone(), "running low on supplies"),
            (
                "withdrawal",
                own.iter()
                    .filter(|u| reserves.get(u.id).unwrap().forced)
                    .collect(),
                "must withdraw for supplies",
            ),
            (
                "stranded",
                own.iter()
                    .filter(|u| reserves.get(u.id).unwrap().stranded)
                    .collect(),
                "cannot complete supply withdrawal",
            ),
        ] {
            if let Some(u) = rows.first() {
                self.alert(
                    ctx,
                    p.slot,
                    kind,
                    format!(
                        "{} unit{} {message}",
                        rows.len(),
                        if rows.len() == 1 { "" } else { "s" }
                    ),
                    Some(u.at),
                    rows.len(),
                );
            }
        }
        let mut incoming = BTreeSet::new();
        for u in world.units() {
            if u.owner == Some(p.slot)
                && u.hp > 0.0
                && let Some(c) = self.trucks.get(&u.id)
                && matches!(c.state, TruckState::Delivering | TruckState::Unloading)
                && quantity(c.cargo) > 0.0
                && let Some(t) = &c.target
            {
                incoming.extend(t.members.iter().copied());
            }
        }
        let transit: Vec<_> = critical
            .iter()
            .filter(|u| incoming.contains(&u.id))
            .collect();
        if let Some(u) = transit.first() {
            self.alert(
                ctx,
                p.slot,
                "transit",
                format!(
                    "Supplies are in transit to {} unit{}",
                    transit.len(),
                    if transit.len() == 1 { "" } else { "s" }
                ),
                Some(u.at),
                transit.len(),
            );
        }
        true
    }
    fn schedule<W: World>(&mut self, ctx: Context<'_>, reserves: &LogisticsState, world: &mut W) {
        while let Some(&slot) = self.schedule_queue.front() {
            if let Some(p) = ctx.players.get(slot)
                && !p.out
                && !p.away
                && !self.schedule_player(ctx, *p, reserves, world)
            {
                return;
            }
            self.schedule_queue.pop_front();
        }
    }
    fn repair<W: World>(&mut self, u: &UnitView, c: &mut Truck, world: &mut W) -> bool {
        let at = u
            .path
            .iter()
            .position(|p| distance(u.at, *p) >= 30.0)
            .unwrap_or(u.path.len() - 1);
        let rejoin = u.path[at];
        let mut detour = world.route(u, rejoin, true).path;
        self.routed += 1;
        if detour.last().is_none_or(|p| distance(*p, rejoin) > 1.0) {
            return false;
        }
        detour.extend_from_slice(&u.path[at + 1..]);
        c.route = detour.clone();
        world.apply(u.id, UnitOperation::RepairRoute(detour));
        true
    }
    fn refresh<W: World>(&mut self, ctx: Context<'_>, world: &mut W) {
        while !self.refresh_queue.is_empty() && self.routed < ROUTES {
            let id = self.refresh_queue.pop_front().unwrap();
            let Some(u) = world.unit(id) else {
                continue;
            };
            let Some(mut c) = self.trucks.remove(&id) else {
                continue;
            };
            if u.role != UnitRole::Truck || u.hp <= 0.0 || c.manual || c.hold {
                self.trucks.insert(id, c);
                continue;
            }
            let resume = match c.state {
                TruckState::WaitingCollect => TruckState::Collecting,
                TruckState::Waiting => TruckState::Delivering,
                TruckState::WaitingReturn => TruckState::Returning,
                s => s,
            };
            if !matches!(
                resume,
                TruckState::Collecting | TruckState::Delivering | TruckState::Returning
            ) {
                self.trucks.insert(id, c);
                continue;
            }
            let mut target = if resume == TruckState::Delivering {
                c.target.as_ref().map(|t| t.at)
            } else {
                self.store(&c.origin)
                    .filter(|s| s.active && friendly(ctx, s.owner, u.owner.unwrap()))
                    .map(|s| s.at)
            };
            if resume != TruckState::Delivering && target.is_none() {
                let mut sources: Vec<_> = self
                    .sources(ctx, u.owner.unwrap())
                    .iter()
                    .filter_map(|id| self.store(id).cloned())
                    .collect();
                sources.sort_by(|a, b| distance(a.at, u.at).total_cmp(&distance(b.at, u.at)));
                if let Some(source) = sources.first() {
                    target = Some(source.at);
                    c.origin = source.id.clone();
                } else {
                    c.state = if resume == TruckState::Collecting {
                        TruckState::WaitingCollect
                    } else {
                        TruckState::WaitingReturn
                    };
                    c.route.clear();
                    world.apply(id, UnitOperation::ClearRoute);
                    self.alert(
                        ctx,
                        u.owner.unwrap(),
                        "stock",
                        "No available supply source for collection or return".into(),
                        Some(u.at),
                        1,
                    );
                    self.trucks.insert(id, c);
                    continue;
                }
            }
            if target.is_none() {
                self.finish(ctx, &u, &mut c, world);
                self.trucks.insert(id, c);
                continue;
            }
            if resume == TruckState::Delivering
                && let Some(t) = c.target.as_mut()
            {
                let recipient = if let Some(store) = &t.store {
                    self.store(store).map(|s| s.at)
                } else {
                    t.unit.and_then(|id| world.unit(id)).map(|u| u.at)
                };
                if let Some(at) = recipient {
                    t.at = at;
                    target = Some(at);
                }
            }
            let target = target.unwrap();
            let on_course = c.state == resume
                && c.destination
                    .is_some_and(|d| distance(d, target) < TRUCK_RADIUS);
            if on_course && !u.path.is_empty() && u.stuck > 0.5 && self.repair(&u, &mut c, world) {
                self.trucks.insert(id, c);
                continue;
            }
            if on_course
                && (!u.path.is_empty() || u.world_goal.is_some())
                && world.continue_route(&u, target)
            {
                self.trucks.insert(id, c);
                continue;
            }
            if self.send(ctx, &u, &mut c, target, world, false) {
                c.state = resume;
            } else {
                c.state = match resume {
                    TruckState::Collecting => TruckState::WaitingCollect,
                    TruckState::Returning => TruckState::WaitingReturn,
                    _ => TruckState::Waiting,
                };
                c.route.clear();
                world.apply(id, UnitOperation::ClearRoute);
                self.route_alert(ctx, u.owner.unwrap(), &c, target);
            }
            self.trucks.insert(id, c);
        }
    }
    /// One source convoy tick. Numeric context/time validation completes before any mutation.
    /// The caller first updates the supply network, then supplies its rates through World.
    pub fn step<W: World>(
        &mut self,
        ctx: Context<'_>,
        reserves: &mut LogisticsState,
        world: &mut W,
        dt: f64,
    ) -> Result<(), ConvoyError> {
        validate(ctx)?;
        if !dt.is_finite() || dt < 0.0 {
            return Err(ConvoyError::InvalidTime);
        }
        if ctx.winner {
            return Ok(());
        }
        self.routed = 0;
        if ctx.tick >= self.next_schedule {
            self.sync_stores(ctx, world)?;
            if self.refresh_queue.is_empty() {
                self.refresh_queue = world
                    .units()
                    .into_iter()
                    .filter(|u| u.role == UnitRole::Truck && self.trucks.contains_key(&u.id))
                    .map(|u| u.id)
                    .collect();
            }
            if self.schedule_queue.is_empty() {
                self.schedule_queue = ctx.players.iter().map(|p| p.slot).collect();
                self.scheduled.clear();
                self.assignments.clear();
            }
            for p in ctx.players {
                if !p.out && !p.away {
                    let count = self.targets(ctx, p.slot, reserves, world).len();
                    self.jobs.insert(p.slot, count);
                }
            }
            self.next_schedule = ctx.tick + 40;
        }
        self.refresh(ctx, world);
        self.fleet(ctx, world);
        self.schedule(ctx, reserves, world);
        self.local_service(ctx, reserves, world, dt);
        self.territory_service(ctx, reserves, world, dt);
        for u in world.units() {
            if u.role != UnitRole::Truck || u.hp <= 0.0 {
                continue;
            }
            let Some(mut c) = self.trucks.remove(&u.id) else {
                continue;
            };
            if c.hold {
                self.trucks.insert(u.id, c);
                continue;
            }
            if c.state == TruckState::Manual {
                if u.path.is_empty() && u.world_goal.is_none() && !u.has_orders() {
                    c.manual = false;
                    c.state = TruckState::Idle;
                }
                self.trucks.insert(u.id, c);
                continue;
            }
            let arrived = c.destination.is_some_and(|d| {
                distance(u.at, d) <= TRUCK_RADIUS
                    && (u.path.is_empty() || distance(u.at, u.world_goal.unwrap_or(d)) < 1.0)
            });
            let origin = self.store(&c.origin).cloned();
            let at_source = matches!(c.state, TruckState::Collecting | TruckState::Returning)
                && u.stuck > 0.5
                && origin.as_ref().is_some_and(|s| {
                    s.active
                        && distance(u.at, s.at) <= SOURCE_RADIUS
                        && world.handoff(&u, s.at, s.entity, SOURCE_RADIUS)
                });
            if c.state == TruckState::Collecting
                && (arrived || at_source)
                && origin
                    .as_ref()
                    .is_some_and(|s| s.active && world.handoff(&u, s.at, s.entity, SOURCE_RADIUS))
            {
                c.state = TruckState::Loading;
                c.timer = LOAD;
                world.apply(u.id, UnitOperation::ClearRoute);
            }
            if c.state == TruckState::Loading {
                c.timer -= dt;
                if c.timer <= 0.0 {
                    self.load_job(ctx, &u, &mut c, world);
                }
            } else if c.state == TruckState::Delivering && arrived {
                c.state = TruckState::Unloading;
                c.timer = UNLOAD;
                world.apply(u.id, UnitOperation::ClearRoute);
            } else if c.state == TruckState::Unloading {
                self.deliver(ctx, &u, &mut c, reserves, world, dt);
            } else if c.state == TruckState::Returning && (arrived || at_source) {
                if let Some(origin) =
                    origin.filter(|s| s.active && world.handoff(&u, s.at, s.entity, SOURCE_RADIUS))
                {
                    let store = self.store_mut(&origin.id).unwrap();
                    let limit = store.space();
                    moved(&mut c.cargo, store.bucket(u.owner.unwrap()), limit);
                    c.state = TruckState::Idle;
                    c.target = None;
                    c.destination = None;
                    world.apply(u.id, UnitOperation::ClearRoute);
                } else {
                    c.state = TruckState::WaitingReturn;
                }
            } else if matches!(
                c.state,
                TruckState::Waiting | TruckState::WaitingReturn | TruckState::WaitingCollect
            ) && ctx.tick % 40 == u64::from(u.id) % 40
            {
                let waiting = c.state;
                let target = if waiting == TruckState::Waiting {
                    c.target.as_ref().map(|t| t.at)
                } else {
                    self.store(&c.origin).map(|s| s.at)
                };
                let manual = c.manual;
                if let Some(target) = target
                    && self.send(ctx, &u, &mut c, target, world, manual)
                {
                    c.state = match waiting {
                        TruckState::Waiting => TruckState::Delivering,
                        TruckState::WaitingCollect => TruckState::Collecting,
                        _ => TruckState::Returning,
                    };
                }
            }
            self.trucks.insert(u.id, c);
        }
        Ok(())
    }
    pub fn convoy_death(&mut self, tick: u64, u: &UnitView) {
        self.retire_unit(tick, u.id, u.owner, u.role == UnitRole::Truck);
    }
    /// Death needs identity and truck eligibility only; positions and cargo remain caller-owned.
    pub fn retire_unit(&mut self, tick: u64, id: UnitId, owner: Option<usize>, truck: bool) {
        if truck && let Some(owner) = owner {
            self.losses
                .entry(owner)
                .or_default()
                .push(tick as f64 / 20.0 + 30.0);
            self.trucks.remove(&id);
        }
        self.stores.retain(|s| s.entity != Some(id));
    }
    pub fn support_provision<W: World>(
        &mut self,
        ctx: Context<'_>,
        unit: UnitId,
        owner: usize,
        amount: f64,
        reserves: &mut LogisticsState,
        world: &mut W,
    ) -> Result<bool, ConvoyError> {
        validate(ctx)?;
        if !amount.is_finite() || amount < 0.0 {
            return Err(ConvoyError::InvalidQuantity);
        }
        let Some(at) = world.unit(unit) else {
            return Ok(false);
        };
        for s in &mut self.stores {
            if !s.active
                || !friendly(ctx, s.owner, owner)
                || distance(s.at, at.at) > SOURCE_RADIUS
                || !world.handoff(&at, s.at, s.entity, SOURCE_RADIUS)
            {
                continue;
            }
            if s.source {
                return Ok(true);
            }
            if s.available(owner, true).provisions >= amount {
                s.consume(
                    owner,
                    Stock {
                        provisions: amount,
                        ..empty()
                    },
                    true,
                );
                return Ok(true);
            }
        }
        if at.owner == Some(owner)
            && at.hp > 0.0
            && world.unit(at.id).is_some()
            && let Some(r) = reserves.get_mut(at.id)
            && r.provisions >= amount
        {
            r.provisions -= amount;
            return Ok(true);
        }
        Ok(false)
    }
    /// Explicit manual delivery to a known cache or unit. Route edits are emitted through World.
    pub fn supply<W: World>(
        &mut self,
        ctx: Context<'_>,
        slot: usize,
        ids: &[UnitId],
        target_id: UnitId,
        reserves: &LogisticsState,
        world: &mut W,
    ) -> Result<bool, ConvoyError> {
        validate(ctx)?;
        if !ids.iter().any(|id| {
            world.unit(*id).is_some_and(|u| {
                u.role == UnitRole::Truck
                    && u.owner == Some(slot)
                    && u.hp > 0.0
                    && self.trucks.contains_key(id)
            })
        }) {
            return Ok(false);
        }
        let Some(target_unit) = world.unit(target_id) else {
            return Ok(false);
        };
        if !target_unit.owner.is_some_and(|o| friendly(ctx, o, slot))
            || target_unit.hp <= 0.0
            || (reserves.get(target_id).is_none() && target_unit.role != UnitRole::Cache)
            || (target_unit.role == UnitRole::Cache
                && (target_unit.built < 1.0 || target_unit.region_cache))
        {
            return Ok(false);
        }
        let store = self
            .stores
            .iter()
            .find(|s| s.entity == Some(target_id) && s.active && Some(s.owner) == target_unit.owner)
            .cloned()
            .or_else(|| {
                if target_unit.role == UnitRole::Cache {
                    let old_position = self
                        .stores
                        .iter()
                        .position(|s| s.id == StoreId::Cache(target_id));
                    let i = self.ensure(
                        ctx,
                        StoreId::Cache(target_id),
                        target_unit.owner.unwrap(),
                        target_unit.at,
                        false,
                        Some(target_id),
                        32.0,
                    );
                    let record = self.stores[i].clone();
                    if let Some(old) = old_position
                        && old != i
                    {
                        let value = self.stores.remove(i);
                        self.stores.insert(old, value);
                    }
                    Some(record)
                } else {
                    None
                }
            });
        let target = Target {
            id: store
                .as_ref()
                .map_or(TargetId::Unit(target_id), |s| TargetId::Store(s.id.clone())),
            unit: store.is_none().then_some(target_id),
            store: store.as_ref().map(|s| s.id.clone()),
            members: if store.is_none() {
                vec![target_id]
            } else {
                vec![]
            },
            owner: target_unit.owner.unwrap(),
            at: target_unit.at,
            need: store
                .as_ref()
                .map_or_else(|| missing(reserves, target_id), Store::space),
            priority: None,
            relief: Some(target_unit.role == UnitRole::Cache && target_unit.owner != Some(slot)),
        };
        let mut success = false;
        for &id in ids {
            let Some(u) = world
                .unit(id)
                .filter(|u| u.role == UnitRole::Truck && u.owner == Some(slot) && u.hp > 0.0)
            else {
                continue;
            };
            let Some(mut c) = self.trucks.remove(&id) else {
                continue;
            };
            if quantity(c.cargo) > 0.0 {
                if self.send(ctx, &u, &mut c, target.at, world, true) {
                    c.target = Some(target.clone());
                    c.manual = true;
                    c.hold = false;
                    c.state = TruckState::Delivering;
                    success = true;
                }
            } else if let Some(origin) = self
                .origins(ctx, slot, &target)
                .first()
                .and_then(|id| self.store(id))
                .cloned()
            {
                success =
                    self.assign(ctx, &u, &mut c, target.clone(), origin, world, true) || success;
            }
            self.trucks.insert(id, c);
        }
        Ok(success)
    }
    pub fn resume<W: World>(
        &mut self,
        ctx: Context<'_>,
        slot: usize,
        ids: &[UnitId],
        world: &mut W,
    ) -> bool {
        if validate(ctx).is_err() {
            return false;
        }
        let mut found = false;
        for &id in ids {
            let Some(u) = world
                .unit(id)
                .filter(|u| u.role == UnitRole::Truck && u.owner == Some(slot) && u.hp > 0.0)
            else {
                continue;
            };
            let Some(mut c) = self.trucks.remove(&id) else {
                continue;
            };
            found = true;
            c.manual = false;
            c.hold = false;
            world.apply(id, UnitOperation::ResumeClear);
            if quantity(c.cargo) > 0.0 {
                if let Some(target) = c.target.clone() {
                    c.state = if self.send(ctx, &u, &mut c, target.at, world, false) {
                        TruckState::Delivering
                    } else {
                        TruckState::Waiting
                    };
                } else {
                    self.finish(ctx, &u, &mut c, world);
                }
            } else if let Some(target) = c.target.clone() {
                if let Some(origin) = self.store(&c.origin).filter(|s| s.active).cloned() {
                    if !self.assign(ctx, &u, &mut c, target, origin, world, false) {
                        c.state = TruckState::Idle;
                        c.target = None;
                    }
                } else {
                    c.state = TruckState::Idle;
                    c.target = None;
                }
            } else {
                c.state = TruckState::Idle;
            }
            self.trucks.insert(id, c);
        }
        found
    }
}
/// Typed ordinary movement orders. The caller validates the wire format before constructing one.
/// These mark convoy manual/hold state; applying the ordinary unit movement order remains caller-owned.
#[derive(Clone, Debug)]
pub enum MovementCommand {
    Move {
        at: Option<Point>,
        orders: Vec<(UnitId, Point)>,
        queue: bool,
    },
    AttackMove {
        at: Option<Point>,
        orders: Vec<(UnitId, Point)>,
        queue: bool,
    },
    Retreat {
        queue: bool,
    },
    Stop,
}
impl ConvoyState {
    pub fn prepare_movement<W: World>(
        &mut self,
        ctx: Context<'_>,
        slot: usize,
        ids: &[UnitId],
        command: &MovementCommand,
        world: &mut W,
    ) -> Result<bool, ConvoyError> {
        validate(ctx)?;
        let (at, rows, queue, retreat, stop) = match command {
            MovementCommand::Move { at, orders, queue }
            | MovementCommand::AttackMove { at, orders, queue } => {
                (*at, orders.as_slice(), *queue, false, false)
            }
            MovementCommand::Retreat { queue } => (None, &[][..], *queue, true, false),
            MovementCommand::Stop => (None, &[][..], false, false, true),
        };
        if at.is_some_and(|p| !p.x.is_finite() || !p.z.is_finite())
            || rows
                .iter()
                .any(|(_, p)| !p.x.is_finite() || !p.z.is_finite())
        {
            return Err(ConvoyError::InvalidContext);
        }
        let trucks: Vec<_> = ids
            .iter()
            .filter_map(|id| world.unit(*id))
            .filter(|u| {
                u.role == UnitRole::Truck
                    && u.owner == Some(slot)
                    && u.hp > 0.0
                    && self.trucks.contains_key(&u.id)
            })
            .collect();
        if !stop {
            for u in &trucks {
                let target = if retreat {
                    let mut sources: Vec<_> = self
                        .sources(ctx, slot)
                        .iter()
                        .filter_map(|id| self.store(id))
                        .collect();
                    sources.sort_by(|a, b| distance(a.at, u.at).total_cmp(&distance(b.at, u.at)));
                    sources.first().map(|s| s.at)
                } else {
                    rows.iter()
                        .find(|(id, _)| *id == u.id)
                        .map(|(_, p)| *p)
                        .or(at)
                };
                let Some(target) = target else {
                    return Ok(false);
                };
                let mut from = u.clone();
                from.projected = true;
                if queue
                    && (!u.path.is_empty()
                        || u.has_orders()
                        || u.world_goal.is_some()
                        || u.amove.is_some())
                {
                    let goal = u.world_goal.or(u.amove).or_else(|| u.path.last().copied());
                    if let Some(goal) = goal {
                        let route = world.route(u, goal, false).path;
                        if route.is_empty() && distance(u.at, goal) > 1.0 {
                            return Ok(false);
                        }
                        if let Some(at) = route.last() {
                            from.at = *at;
                        }
                    }
                    for &waypoint in &u.orders {
                        if !waypoint.x.is_finite() || !waypoint.z.is_finite() {
                            continue;
                        }
                        if let Some(at) = world.route(&from, waypoint, false).path.last() {
                            from.at = *at;
                        }
                    }
                }
                let route = world.route(&from, target, false).path;
                if route.is_empty() && distance(from.at, target) > 1.0 {
                    return Ok(false);
                }
            }
        }
        for u in trucks {
            let c = self.trucks.get_mut(&u.id).unwrap();
            c.manual = true;
            c.hold = stop;
            if !stop {
                c.target = None;
            }
            c.state = if stop {
                TruckState::Held
            } else {
                TruckState::Manual
            };
        }
        Ok(true)
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct ReserveRow {
    pub id: UnitId,
    pub ammo: Option<f64>,
    pub provisions: f64,
    pub fuel: Option<f64>,
    pub emergency: Option<f64>,
    pub warning: Option<f64>,
    pub forced: bool,
    pub stranded: bool,
    pub supply: f64,
    pub cut: bool,
    pub grace: Option<f64>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct TruckRow {
    pub id: UnitId,
    pub state: TruckState,
    pub manual: bool,
    pub hold: bool,
    pub cargo: Stock,
    pub destination: Option<Point>,
    pub route: Vec<Point>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct StoreRow {
    pub id: StoreId,
    pub owner: usize,
    pub at: Point,
    pub source: bool,
    pub stock: Stock,
    pub capacity: Stock,
}
#[derive(Clone, Debug, PartialEq)]
pub struct RecipientState {
    pub units: Vec<ReserveRow>,
    pub trucks: Vec<TruckRow>,
    pub stores: Vec<StoreRow>,
    pub events: Vec<Event>,
}
fn rounded(n: f64) -> f64 {
    math::round(n * 100.0) / 100.0
}
impl ConvoyState {
    /// Recipient-owned data for a future snapshot encoder. World-region rates remain caller-owned.
    /// This defines no new wire fields and never returns enemy reserves or another owner's events.
    pub fn recipient_state<W: World>(
        &mut self,
        ctx: Context<'_>,
        slot: usize,
        reserves: &LogisticsState,
        world: &W,
    ) -> Result<RecipientState, ConvoyError> {
        validate(ctx)?;
        if slot >= ctx.players.len() {
            return Err(ConvoyError::InvalidContext);
        }
        let mut units = vec![];
        let mut trucks = vec![];
        for u in world.units() {
            if u.owner != Some(slot) {
                continue;
            }
            if let Some(r) = reserves.get(u.id) {
                let cut = r.cut_for.unwrap_or(0.0) >= GRACE;
                units.push(ReserveRow {
                    id: u.id,
                    ammo: (r.ammo_max != 0.0).then(|| rounded(r.ammo / r.ammo_max)),
                    provisions: rounded(r.provisions),
                    fuel: r.fuel.map(rounded),
                    emergency: r.emergency.map(rounded),
                    warning: r.warning.map(rounded),
                    forced: r.forced,
                    stranded: r.stranded,
                    supply: rounded(if cut {
                        0.0
                    } else {
                        r.supply_rate.unwrap_or(0.0)
                    }),
                    cut,
                    grace: r
                        .cut_for
                        .filter(|n| *n > 0.0 && *n < GRACE)
                        .map(|n| (GRACE - n).ceil()),
                });
            }
            if let Some(c) = self.trucks.get(&u.id) {
                let q = items(c.cargo).map(rounded);
                trucks.push(TruckRow {
                    id: u.id,
                    state: c.state,
                    manual: c.manual,
                    hold: c.hold,
                    cargo: stock(q),
                    destination: c.destination,
                    route: u
                        .path
                        .iter()
                        .map(|p| Point {
                            x: rounded(p.x),
                            z: rounded(p.z),
                        })
                        .collect(),
                });
            }
        }
        let mut stores = vec![];
        for s in &mut self.stores {
            if s.active && friendly(ctx, s.owner, slot) {
                let inventory = s.available(slot, true);
                stores.push(StoreRow {
                    id: s.id.clone(),
                    owner: s.owner,
                    at: Point {
                        x: rounded(s.at.x),
                        z: rounded(s.at.z),
                    },
                    source: s.source,
                    stock: inventory,
                    capacity: s.capacity,
                });
            }
        }
        let events = self
            .events
            .iter()
            .filter(|e| e.owner == slot)
            .cloned()
            .collect();
        Ok(RecipientState {
            units,
            trucks,
            stores,
            events,
        })
    }
}
