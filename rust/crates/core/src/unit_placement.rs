//! Commit a relocation's unit fields and spatial membership together.
use crate::{game::Unit, grid::SpatialGrid};

/// Call after the caller has admitted the placement. Field preparation and
/// metadata stay with their existing owners; ordinary movers update directly.
pub(crate) fn place_unit(grid: &mut SpatialGrid, unit: &mut Unit, fields: impl FnOnce(&mut Unit)) {
    fields(unit);
    grid.update(unit.id, unit);
}

#[cfg(test)]
pub(crate) mod tests;
