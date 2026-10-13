//! Retain source cached-unit object facts after an ordered World removal batch.
//! Terrain, wrecks, visibility, journal cursors and route buffers stay with their owners.
use crate::{
    game::Unit,
    match_world_memory::{WorldRoutingContext, bound, validate_world_context},
    world_memory::{BoundUnit, WorldMemory},
};
use std::collections::BTreeSet;

/// The caller prepares final removal snapshots before changing canonical units.
/// Earlier staged building collapses may change a later unit's HP or position;
/// those final values may differ from its still-live canonical values.
/// Only cached kind, HP and position are retained. This does not delete units,
/// wreck buildings, refresh knowledge or validate the caller's whole transaction.
#[must_use = "commit after validated ordered removal, or drop without changing cached facts"]
pub struct PreparedWorldRouteRetention<'a> {
    target: &'a mut WorldMemory,
    rows: Vec<BoundUnit>,
}
impl PreparedWorldRouteRetention<'_> {
    /// Run after the caller's canonical deletion and wreck publication. Cached
    /// membership stays fixed until its ordinary source refresh key advances.
    pub fn commit(self) {
        for row in self.rows {
            for live in self.target.lives.values_mut() {
                if let Some(old) = live.units.iter_mut().find(|u| u.id == row.id) {
                    *old = row;
                }
            }
        }
    }
}

/// Validate current World/memory bindings and ordered final snapshots without
/// changing either owner. Existing IDs, owners and kinds must still match. The
/// caller's prepared removal plan establishes the final finite HP and pose.
/// The guard borrows only memory, leaving canonical Game and terrain available
/// for the already validated, infallible deletion and wreck publication.
pub fn prepare_world_route_retention<'a>(
    context: WorldRoutingContext<'_>,
    target: &'a mut WorldMemory,
    final_units: &[&Unit],
) -> Result<PreparedWorldRouteRetention<'a>, String> {
    let first_owner = final_units.first().map_or(0, |u| u.owner);
    validate_world_context(context, target, first_owner)?;
    let mut owners = BTreeSet::from([first_owner]);
    let mut ids = BTreeSet::new();
    let mut rows = Vec::with_capacity(final_units.len());
    for unit in final_units {
        let current = context
            .world
            .game
            .units
            .get(&unit.id)
            .ok_or("World route retention needs a current unit")?;
        if unit.id == 0
            || !ids.insert(unit.id)
            || current.owner != unit.owner
            || current.kind != unit.kind
            || !unit.hp.is_finite()
            || !unit.x.is_finite()
            || !unit.z.is_finite()
        {
            return Err("invalid final World route retention facts".into());
        }
        if owners.insert(unit.owner) {
            validate_world_context(context, target, unit.owner)?;
        }
        rows.push(bound(unit));
    }
    Ok(PreparedWorldRouteRetention { target, rows })
}
