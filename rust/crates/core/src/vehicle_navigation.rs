//! Conservative turning clearance over only the terrain supplied to pathfinding.
use crate::game::{CELL, Point};
use crate::math;
use crate::navigation::{TerrainChange, TerrainView, Versions};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, PartialEq, Eq)]
struct Identity {
    width: usize,
    height: usize,
    flags: usize,
    heights: Option<usize>,
    knowledge_id: u64,
    world_known: bool,
}
impl Identity {
    fn new(v: TerrainView<'_>) -> Self {
        Self {
            width: v.width,
            height: v.height,
            flags: v.flags.as_ptr() as usize,
            heights: v.heights.map(|h| h.as_ptr() as usize),
            knowledge_id: v.knowledge_id,
            world_known: v.world_known,
        }
    }
}
#[derive(Clone)]
struct Entry {
    identity: Identity,
    versions: Versions,
    flags: Vec<u16>,
    changed: Vec<usize>,
    previous: u64,
    incremental: bool,
    serial: u64,
}
#[derive(Clone, Default)]
pub(crate) struct VehicleNavigation {
    classes: BTreeMap<(u16, u64), Entry>,
    serial: u64,
}
impl VehicleNavigation {
    pub(crate) fn buffers_for(&self, source: TerrainView<'_>) -> Vec<usize> {
        let identity = Identity::new(source);
        self.classes
            .values()
            .filter(|e| e.identity == identity)
            .map(|e| e.flags.as_ptr() as usize)
            .collect()
    }

    pub(crate) fn rebind_terrain(&mut self, source: TerrainView<'_>, prepared: TerrainView<'_>) {
        let old = Identity::new(source);
        for entry in self.classes.values_mut() {
            if entry.identity == old {
                entry.identity = Identity::new(prepared);
            }
        }
    }

    pub(crate) fn cloned_buffers(&self, cloned: &Self) -> Vec<(usize, usize)> {
        self.classes
            .iter()
            .map(|(key, entry)| {
                (
                    entry.flags.as_ptr() as usize,
                    cloned.classes[key].flags.as_ptr() as usize,
                )
            })
            .collect()
    }

    pub(crate) fn view<'a>(
        &'a mut self,
        source: TerrainView<'a>,
        radius: f64,
        block: u16,
    ) -> TerrainView<'a> {
        let radius = math::hypot(radius * 0.8, radius * 0.48);
        let reach = ((radius / CELL).ceil() as usize).min(source.width.max(source.height)) + 1;
        let key = (block, radius.to_bits());
        let identity = Identity::new(source);
        let entry = self.classes.entry(key).or_insert_with(|| Entry {
            identity,
            versions: source.versions,
            flags: Vec::new(),
            changed: Vec::new(),
            previous: 0,
            incremental: false,
            serial: 0,
        });
        let same_storage = entry.identity == identity && entry.flags.len() == source.flags.len();
        let reuse = same_storage
            && entry.versions.vehicle == source.versions.vehicle
            && entry.versions.obstruction == source.versions.obstruction
            && entry.versions.knowledge == source.versions.knowledge;
        if !reuse {
            self.serial = self.serial.wrapping_add(1);
            entry.serial = self.serial;
            entry.previous = entry.versions.knowledge;
            let changes = same_storage
                .then(|| changes_since(source, entry.previous))
                .flatten();
            entry.incremental = changes.is_some();
            entry.changed.clear();
            if let Some(changes) = changes {
                let mut affected = BTreeSet::new();
                for c in changes.into_iter().filter(|&c| c < source.flags.len()) {
                    let x = c % source.width;
                    let y = c / source.width;
                    for ny in y.saturating_sub(reach)..=(y + reach).min(source.height - 1) {
                        for nx in x.saturating_sub(reach)..=(x + reach).min(source.width - 1) {
                            let cell = ny * source.width + nx;
                            if affected.insert(cell) {
                                entry.changed.push(cell);
                            }
                        }
                    }
                }
                for &c in &entry.changed {
                    entry.flags[c] = inflated_cell(source, c, radius, reach, block);
                }
            } else {
                entry.flags.resize(source.flags.len(), 0);
                // Start from source flags, then spread each obstruction's radius.
                // This avoids scanning a neighborhood around every open map cell.
                entry.flags.copy_from_slice(source.flags);
                for c in 0..source.flags.len() {
                    let x = c % source.width;
                    let y = c / source.width;
                    if boundary(source, c, radius) {
                        entry.flags[c] |= block;
                    }
                    if source.flags[c] & block == 0 {
                        continue;
                    }
                    for ny in y.saturating_sub(reach)..=(y + reach).min(source.height - 1) {
                        for nx in x.saturating_sub(reach)..=(x + reach).min(source.width - 1) {
                            if overlaps(nx, ny, x, y, radius) {
                                entry.flags[ny * source.width + nx] |= block;
                            }
                        }
                    }
                }
            }
            entry.identity = identity;
            entry.versions = source.versions;
        }
        TerrainView {
            flags: &entry.flags,
            // A source replacement must invalidate the coarse graph even if
            // source versions restart and the owned flags allocation is reused.
            versions: Versions {
                obstruction: entry.serial,
                ..source.versions
            },
            changed: entry.incremental.then_some(TerrainChange {
                from: entry.previous,
                to: source.versions.knowledge,
                cells: &entry.changed,
            }),
            // The direct expanded change covers this view's last update. A missing
            // history gap deliberately triggers the coarse planner's full rebuild.
            changes: &[],
            ..source
        }
    }
}
fn changes_since(source: TerrainView<'_>, previous: u64) -> Option<Vec<usize>> {
    if let Some(change) = source
        .changed
        .filter(|c| c.from == previous && c.to == source.versions.knowledge)
    {
        return Some(change.cells.to_vec());
    }
    let mut version = previous;
    let mut cells = Vec::new();
    for change in &source.changes[source.changes.len().saturating_sub(32)..] {
        if change.from != version || change.to == version {
            continue;
        }
        cells.extend_from_slice(change.cells);
        version = change.to;
        if version == source.versions.knowledge {
            return Some(cells);
        }
    }
    None
}
fn boundary(v: TerrainView<'_>, c: usize, radius: f64) -> bool {
    let px = (c % v.width) as f64 * CELL + CELL / 2.0;
    let pz = (c / v.width) as f64 * CELL + CELL / 2.0;
    px < radius
        || pz < radius
        || px + radius >= v.width as f64 * CELL
        || pz + radius >= v.height as f64 * CELL
}
fn overlaps(ax: usize, ay: usize, bx: usize, by: usize, radius: f64) -> bool {
    let dx = (ax.abs_diff(bx) as f64 * CELL - CELL / 2.0).max(0.0);
    let dz = (ay.abs_diff(by) as f64 * CELL - CELL / 2.0).max(0.0);
    math::hypot(dx, dz) < radius - 1e-6
}
fn inflated_cell(v: TerrainView<'_>, c: usize, radius: f64, reach: usize, block: u16) -> u16 {
    if boundary(v, c, radius) {
        return v.flags[c] | block;
    }
    let x = c % v.width;
    let y = c / v.width;
    for ny in y.saturating_sub(reach)..=(y + reach).min(v.height - 1) {
        for nx in x.saturating_sub(reach)..=(x + reach).min(v.width - 1) {
            if v.flags[ny * v.width + nx] & block != 0 && overlaps(x, y, nx, ny, radius) {
                return v.flags[c] | block;
            }
        }
    }
    v.flags[c]
}
pub(crate) fn destination_clear(v: TerrainView<'_>, at: Point, radius: f64, block: u16) -> bool {
    let radius = math::hypot(radius * 0.8, radius * 0.48);
    if at.x < radius
        || at.z < radius
        || at.x + radius >= v.width as f64 * CELL
        || at.z + radius >= v.height as f64 * CELL
    {
        return false;
    }
    for y in ((at.z - radius) / CELL).floor() as usize..=((at.z + radius) / CELL).floor() as usize {
        for x in
            ((at.x - radius) / CELL).floor() as usize..=((at.x + radius) / CELL).floor() as usize
        {
            if v.flags[y * v.width + x] & block == 0 {
                continue;
            }
            let dx = (x as f64 * CELL - at.x)
                .max(0.0)
                .max(at.x - (x + 1) as f64 * CELL);
            let dz = (y as f64 * CELL - at.z)
                .max(0.0)
                .max(at.z - (y + 1) as f64 * CELL);
            if math::hypot(dx, dz) < radius - 1e-6 {
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::{MOVE, VBLOCK};

    fn assert_fresh(
        cache: &mut VehicleNavigation,
        source: TerrainView<'_>,
        incremental: bool,
    ) -> u64 {
        let view = cache.view(source, 2.0, MOVE | VBLOCK);
        assert_eq!(view.changed.is_some(), incremental);
        let mut fresh = VehicleNavigation::default();
        assert_eq!(view.flags, fresh.view(source, 2.0, MOVE | VBLOCK).flags);
        view.versions.obstruction
    }

    #[test]
    fn cache_reuse_and_change_paths_match_fresh_clearance() {
        let mut flags = vec![0; 13 * 11];
        let mut cache = VehicleNavigation::default();
        fn source<'a>(
            flags: &'a [u16],
            versions: Versions,
            changed: Option<TerrainChange<'a>>,
            changes: &'a [TerrainChange<'a>],
        ) -> TerrainView<'a> {
            TerrainView {
                width: 13,
                height: 11,
                flags,
                heights: None,
                world_known: false,
                knowledge_id: 1,
                versions,
                changed,
                changes,
            }
        }
        let initial = assert_fresh(
            &mut cache,
            source(&flags, Versions::default(), None, &[]),
            false,
        );
        let reused = assert_fresh(
            &mut cache,
            source(&flags, Versions::default(), None, &[]),
            false,
        );
        assert_eq!(initial, reused);
        flags[71] = MOVE;
        let versions = Versions {
            vehicle: 1,
            knowledge: 1,
            ..Versions::default()
        };
        let direct = TerrainChange {
            from: 0,
            to: 1,
            cells: &[71],
        };
        assert_ne!(
            initial,
            assert_fresh(
                &mut cache,
                source(&flags, versions, Some(direct), &[]),
                true
            )
        );
        {
            let view = cache.view(
                source(&flags, versions, Some(direct), &[]),
                2.0,
                MOVE | VBLOCK,
            );
            assert_eq!(view.changed.unwrap().cells.first(), Some(&43));
        }
        flags[72] = VBLOCK;
        flags[120] = MOVE;
        flags[20] = MOVE;
        let versions = Versions {
            vehicle: 2,
            knowledge: 2,
            ..Versions::default()
        };
        let history = [TerrainChange {
            from: 1,
            to: 2,
            cells: &[120, 20, 72],
        }];
        assert_fresh(&mut cache, source(&flags, versions, None, &history), true);
        {
            let view = cache.view(source(&flags, versions, None, &history), 2.0, MOVE | VBLOCK);
            assert_eq!(view.changed.unwrap().cells.first(), Some(&92));
        }
        // Without a complete change chain, all cells must be recomputed.
        flags[71] = 0;
        let versions = Versions {
            vehicle: 3,
            knowledge: 3,
            ..Versions::default()
        };
        assert_fresh(&mut cache, source(&flags, versions, None, &[]), false);
        // A new recipient can share source storage while restarting its versions.
        flags[72] = 0;
        let mut recipient = source(&flags, Versions::default(), None, &[]);
        recipient.knowledge_id = 2;
        let previous = assert_fresh(&mut cache, recipient, false);
        // Replacing the source at the same versions must also reach the coarse cache.
        let mut replacement = flags.clone();
        replacement[71] = MOVE;
        recipient.flags = &replacement;
        assert_ne!(previous, assert_fresh(&mut cache, recipient, false));
    }
}
