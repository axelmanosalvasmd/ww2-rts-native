//! Spatial index of units, a port of `shared/grid.js` at the reference pin.
//!
//! Units sit in 16 m buckets. Queries return the units in the square around a point that pass a
//! test; callers check the exact distance they mean. Ordered queries return units in the order the
//! index first saw them, which is the unit map's order (ascending id), so a choice where the first
//! or best match wins never depends on where units sit in the index. Unordered queries return
//! bucket scan order: buckets row by row, and inside a bucket the order after the last rebuild with
//! units moved since then at the end. That order is reproduced exactly so the core can be compared
//! with the JavaScript; code that needs a stable answer uses the ordered queries.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use crate::math;

pub type UnitId = u32;

/// What the index needs to know about a unit.
pub trait Placed {
    fn x(&self) -> f64;
    fn z(&self) -> f64;
    fn owner(&self) -> i32;
    /// Heavy units (everything but infantry in the game) also sit in buckets of their own.
    fn heavy(&self) -> bool;
}

type Cell = (i64, i64);

#[derive(Clone, Copy)]
struct Record {
    id: UnitId,
    cell: Cell,
    owner: i32,
    /// When the index first saw the unit; ordered queries sort by it.
    order: u64,
}

pub struct SpatialGrid {
    size: f64,
    buckets: HashMap<Cell, Vec<Record>>,
    heavy_buckets: HashMap<Cell, Vec<Record>>,
    records: BTreeMap<UnitId, Record>,
    owners: HashMap<i32, BTreeSet<UnitId>>,
    next_order: u64,
    /// Buckets that gained a unit since the last rebuild, sorted back into order by `rebuild`.
    moved: HashSet<(bool, Cell)>,
}

impl SpatialGrid {
    /// Administrative preparation preserves bucket and encounter order without a rebuild.
    pub(crate) fn clone_for_transaction(&self) -> Self {
        Self {
            size: self.size,
            buckets: self.buckets.clone(),
            heavy_buckets: self.heavy_buckets.clone(),
            records: self.records.clone(),
            owners: self.owners.clone(),
            next_order: self.next_order,
            moved: self.moved.clone(),
        }
    }

    /// An index over `units` with buckets `size` metres square (16 in the game).
    pub fn new<U: Placed>(units: &BTreeMap<UnitId, U>, size: f64) -> Self {
        let mut grid = SpatialGrid {
            size,
            buckets: HashMap::new(),
            heavy_buckets: HashMap::new(),
            records: BTreeMap::new(),
            owners: HashMap::new(),
            next_order: 0,
            moved: HashSet::new(),
        };
        grid.rebuild(units);
        grid
    }

    /// Brings the index up to date with `units`: drops units that are gone, moves units whose
    /// bucket changed, and sorts every bucket that changed back into order.
    pub fn rebuild<U: Placed>(&mut self, units: &BTreeMap<UnitId, U>) {
        let gone: Vec<UnitId> = self
            .records
            .keys()
            .filter(|id| !units.contains_key(id))
            .copied()
            .collect();
        for id in gone {
            self.drop_unit(id);
        }
        for (&id, unit) in units {
            self.update(id, unit);
        }
        for (heavy, cell) in self.moved.drain() {
            let buckets = if heavy {
                &mut self.heavy_buckets
            } else {
                &mut self.buckets
            };
            if let Some(bucket) = buckets.get_mut(&cell) {
                bucket.sort_unstable_by_key(|record| record.order);
            }
        }
    }

    /// Remove a retired unit without rebuilding or reordering surviving buckets.
    pub fn remove_unit(&mut self, id: UnitId) {
        self.drop_unit(id);
    }

    fn drop_unit(&mut self, id: UnitId) {
        if let Some(record) = self.records.remove(&id) {
            self.unlink(&record);
        }
    }

    fn unlink(&mut self, record: &Record) {
        for buckets in [&mut self.buckets, &mut self.heavy_buckets] {
            if let Some(bucket) = buckets.get_mut(&record.cell)
                && let Some(i) = bucket.iter().position(|r| r.id == record.id)
            {
                bucket.remove(i);
            }
        }
        if let Some(own) = self.owners.get_mut(&record.owner) {
            own.remove(&record.id);
        }
    }

    /// Records where a unit is now, after it moved within a tick. A unit that changed bucket goes
    /// to the end of its new bucket until the next rebuild.
    pub fn update<U: Placed>(&mut self, id: UnitId, unit: &U) {
        let cell = (self.bucket_of(unit.z()), self.bucket_of(unit.x()));
        let old = self.records.get(&id).copied();
        if let Some(old) = old {
            if old.cell == cell && old.owner == unit.owner() {
                return;
            }
            self.unlink(&old);
        }
        let order = match old {
            Some(old) => old.order,
            None => {
                self.next_order += 1;
                self.next_order - 1
            }
        };
        let record = Record {
            id,
            cell,
            owner: unit.owner(),
            order,
        };
        self.records.insert(id, record);
        self.buckets.entry(cell).or_default().push(record);
        self.moved.insert((false, cell));
        if unit.heavy() {
            self.heavy_buckets.entry(cell).or_default().push(record);
            self.moved.insert((true, cell));
        }
        self.owners.entry(unit.owner()).or_default().insert(id);
    }

    fn bucket_of(&self, v: f64) -> i64 {
        (v / self.size).floor() as i64
    }

    /// The units of one owner, in order.
    pub fn owned_by<U: Placed>(&self, units: &BTreeMap<UnitId, U>, owner: i32) -> Vec<UnitId> {
        let Some(own) = self.owners.get(&owner) else {
            return Vec::new();
        };
        let mut found: Vec<Record> = own
            .iter()
            .filter(|id| units.contains_key(id))
            .map(|id| self.records[id])
            .collect();
        found.sort_unstable_by_key(|record| record.order);
        found.into_iter().map(|record| record.id).collect()
    }

    /// Units in the square of `radius` around `at` that pass `matches`, in the units' order: for
    /// choices where the first or best match wins.
    pub fn in_order<U: Placed>(
        &self,
        units: &BTreeMap<UnitId, U>,
        at: (f64, f64),
        radius: f64,
        matches: impl FnMut(UnitId, &U) -> bool,
    ) -> Vec<UnitId> {
        self.candidates(units, at, radius, true, matches, false)
    }

    /// Like `in_order`, in bucket scan order: a little cheaper, for sums and effects on every unit.
    pub fn unordered<U: Placed>(
        &self,
        units: &BTreeMap<UnitId, U>,
        at: (f64, f64),
        radius: f64,
        matches: impl FnMut(UnitId, &U) -> bool,
    ) -> Vec<UnitId> {
        self.candidates(units, at, radius, false, matches, false)
    }

    /// Heavy units only, in bucket scan order.
    pub fn heavy<U: Placed>(
        &self,
        units: &BTreeMap<UnitId, U>,
        at: (f64, f64),
        radius: f64,
        matches: impl FnMut(UnitId, &U) -> bool,
    ) -> Vec<UnitId> {
        self.candidates(units, at, radius, false, matches, true)
    }

    /// Whether any unit in the square passes `matches`; stops at the first.
    pub fn has<U: Placed>(
        &self,
        units: &BTreeMap<UnitId, U>,
        at: (f64, f64),
        radius: f64,
        matches: impl FnMut(UnitId, &U) -> bool,
    ) -> bool {
        self.first(units, at, radius, matches, false)
    }

    /// Whether any heavy unit in the square passes `matches`; stops at the first.
    pub fn has_heavy<U: Placed>(
        &self,
        units: &BTreeMap<UnitId, U>,
        at: (f64, f64),
        radius: f64,
        matches: impl FnMut(UnitId, &U) -> bool,
    ) -> bool {
        self.first(units, at, radius, matches, true)
    }

    /// Units within `radius` of `at` (the circle, not the square) that pass `matches`, in order.
    pub fn radius<U: Placed>(
        &self,
        units: &BTreeMap<UnitId, U>,
        at: (f64, f64),
        radius: f64,
        mut matches: impl FnMut(UnitId, &U) -> bool,
    ) -> Vec<UnitId> {
        self.candidates(
            units,
            at,
            radius,
            true,
            |id, u| math::hypot(u.x() - at.0, u.z() - at.1) <= radius && matches(id, u),
            false,
        )
    }

    /// The nearest unit within `radius` of `at` that passes `matches`; the first in order wins a tie.
    pub fn nearest<U: Placed>(
        &self,
        units: &BTreeMap<UnitId, U>,
        at: (f64, f64),
        radius: f64,
        mut matches: impl FnMut(UnitId, &U) -> bool,
    ) -> Option<UnitId> {
        let mut best = None;
        let mut distance = radius;
        for id in self.radius(units, at, radius, |_, _| true) {
            let u = &units[&id];
            let d = math::hypot(u.x() - at.0, u.z() - at.1);
            if matches(id, u) && (best.is_none() || d < distance) {
                best = Some(id);
                distance = d;
            }
        }
        best
    }

    fn cells(&self, at: (f64, f64), radius: f64) -> impl Iterator<Item = Cell> + use<> {
        let (x0, x1) = (self.bucket_of(at.0 - radius), self.bucket_of(at.0 + radius));
        let (z0, z1) = (self.bucket_of(at.1 - radius), self.bucket_of(at.1 + radius));
        (z0..=z1).flat_map(move |z| (x0..=x1).map(move |x| (z, x)))
    }

    fn first<U: Placed>(
        &self,
        units: &BTreeMap<UnitId, U>,
        at: (f64, f64),
        radius: f64,
        mut matches: impl FnMut(UnitId, &U) -> bool,
        heavy: bool,
    ) -> bool {
        let buckets = if heavy {
            &self.heavy_buckets
        } else {
            &self.buckets
        };
        self.cells(at, radius).any(|cell| {
            buckets.get(&cell).is_some_and(|bucket| {
                bucket
                    .iter()
                    .any(|r| units.get(&r.id).is_some_and(|u| matches(r.id, u)))
            })
        })
    }

    fn candidates<U: Placed>(
        &self,
        units: &BTreeMap<UnitId, U>,
        at: (f64, f64),
        radius: f64,
        ordered: bool,
        mut matches: impl FnMut(UnitId, &U) -> bool,
        heavy: bool,
    ) -> Vec<UnitId> {
        let buckets = if heavy {
            &self.heavy_buckets
        } else {
            &self.buckets
        };
        let mut found: Vec<Record> = Vec::new();
        for cell in self.cells(at, radius) {
            let Some(bucket) = buckets.get(&cell) else {
                continue;
            };
            for record in bucket {
                if units.get(&record.id).is_some_and(|u| matches(record.id, u)) {
                    found.push(*record);
                }
            }
        }
        // grid.js merges each bucket's run by order, or sorts when a unit moved since the last
        // rebuild; orders are unique, so both come to the same thing as sorting by order.
        if ordered {
            found.sort_unstable_by_key(|record| record.order);
        }
        found.into_iter().map(|record| record.id).collect()
    }
}
