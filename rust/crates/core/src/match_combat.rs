//! Canonical combat metadata and ordered damage/death application for the Match runner.
//! Health, suppression, position, paths and garrison remain in World.game.
use crate::{combat::DeathImpulse, grid::UnitId};
use std::collections::BTreeMap;

/// Metadata absent from Game's movement/query state. No health or position is stored here.
#[derive(Clone, Debug, PartialEq)]
pub struct UnitCombat {
    pub xp: f64,
    pub last_hit: Option<i32>,
    pub death_impulse: Option<DeathImpulse>,
    pub hit_by: Option<UnitId>,
    pub hit_at: f64,
    pub at_hit: Option<f64>,
    pub cargo: Vec<UnitId>,
    /// Carrier identity. Game.riding is kept in sync as its query flag.
    pub carrier: Option<UnitId>,
}
impl Default for UnitCombat {
    fn default() -> Self {
        Self {
            xp: 0.,
            last_hit: None,
            death_impulse: None,
            hit_by: None,
            hit_at: -1e9,
            at_hit: None,
            cargo: Vec::new(),
            carrier: None,
        }
    }
}
/// Dead shooter metadata can remain while a projectile still refers to its source XP.
/// The firing producer explicitly releases retained IDs after its last flight expires.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CombatState {
    units: BTreeMap<UnitId, UnitCombat>,
}
impl CombatState {
    pub fn initialize(&mut self, id: UnitId) -> &mut UnitCombat {
        self.units.entry(id).or_default()
    }
    pub fn get(&self, id: UnitId) -> Option<&UnitCombat> {
        self.units.get(&id)
    }
    pub fn get_mut(&mut self, id: UnitId) -> Option<&mut UnitCombat> {
        self.units.get_mut(&id)
    }
    pub fn remove(&mut self, id: UnitId) -> Option<UnitCombat> {
        self.units.remove(&id)
    }
    pub fn iter(&self) -> impl Iterator<Item = (UnitId, &UnitCombat)> {
        self.units.iter().map(|(id, u)| (*id, u))
    }
}
/// The firing lane owns the timer. A reset is infallible after event preflight and must
/// initialize a missing timer just as the source assigns retarget on an existing unit.
pub trait RetargetSink {
    fn reset_retarget(&mut self, unit: UnitId);
}
