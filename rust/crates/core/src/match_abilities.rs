//! Canonical ground ability state. Health, sprint and the aimed point retain their existing owners.
use crate::grid::UnitId;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AbilityUnit {
    /// Source `cd`, allowed to expire below zero.
    pub cooldown: f64,
    pub buff: f64,
    pub ap: bool,
    /// The aimed point itself is `UnitControl::nade`.
    pub aimed_auto: bool,
    pub aimed_walked: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct AbilityState {
    units: BTreeMap<UnitId, AbilityUnit>,
}
impl AbilityState {
    pub fn initialize(&mut self, id: UnitId) -> &mut AbilityUnit {
        self.units.entry(id).or_default()
    }
    pub fn get(&self, id: UnitId) -> Option<&AbilityUnit> {
        self.units.get(&id)
    }
    pub fn get_mut(&mut self, id: UnitId) -> Option<&mut AbilityUnit> {
        self.units.get_mut(&id)
    }
    pub fn remove(&mut self, id: UnitId) -> Option<AbilityUnit> {
        self.units.remove(&id)
    }
    pub fn iter(&self) -> impl Iterator<Item = (UnitId, &AbilityUnit)> {
        self.units.iter().map(|(id, state)| (*id, state))
    }
}
