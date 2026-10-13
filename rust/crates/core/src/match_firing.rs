//! Canonical weapon state for the authoritative Match firing phase.
//! Unit ownership, health and position remain in Game; command aim and intent
//! remain in UnitControl, and hit history and experience remain in CombatState.

use std::collections::BTreeMap;

use crate::grid::UnitId;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FiringUnit {
    /// Chosen target, with zero meaning no target.
    pub target_id: UnitId,
    pub cooldown: f64,
    pub retarget: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct FiringState {
    pub units: BTreeMap<UnitId, FiringUnit>,
}

impl FiringState {
    /// Initialize once. Repeated calls preserve the current target and timers.
    pub fn initialize(&mut self, id: UnitId) -> &mut FiringUnit {
        self.units.entry(id).or_default()
    }

    pub fn get(&self, id: UnitId) -> Option<&FiringUnit> {
        self.units.get(&id)
    }

    pub fn get_mut(&mut self, id: UnitId) -> Option<&mut FiringUnit> {
        self.units.get_mut(&id)
    }

    pub fn remove(&mut self, id: UnitId) -> Option<FiringUnit> {
        self.units.remove(&id)
    }
}
