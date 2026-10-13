//! Recovery metadata only. Game owns live health, and LogisticsState owns physical provisions.
use crate::grid::UnitId;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RecoveryUnit {
    pub hp_was: Option<f64>,
    pub wound_at: Option<i64>,
    pub heal_at: Option<i64>,
    pub reinforcement: f64,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RecoveryState {
    units: BTreeMap<UnitId, RecoveryUnit>,
}
impl RecoveryState {
    pub fn initialize(&mut self, id: UnitId) -> &mut RecoveryUnit {
        self.units.entry(id).or_default()
    }
    pub fn get(&self, id: UnitId) -> Option<&RecoveryUnit> {
        self.units.get(&id)
    }
    pub fn get_mut(&mut self, id: UnitId) -> Option<&mut RecoveryUnit> {
        self.units.get_mut(&id)
    }
    pub fn remove(&mut self, id: UnitId) -> Option<RecoveryUnit> {
        self.units.remove(&id)
    }
    pub fn iter(&self) -> impl Iterator<Item = (UnitId, &RecoveryUnit)> {
        self.units.iter().map(|(id, state)| (*id, state))
    }
}
