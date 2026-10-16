//! Weapon geometry reads the same physical field works used by the build phase.
use super::*;

impl Match {
    pub(crate) fn weapon_range(&self, unit: &Unit, weapon: &Value) -> Result<f64, String> {
        let pit = matches!(
            self.rules.kinds[unit.kind].name.as_str(),
            "mg" | "at" | "mortar" | "flak" | "howitzer"
        ) && self.world.game.cell_of(unit.x, unit.z).is_some_and(|cell| {
            self.field_works.pits().contains(&cell) && self.world.game.chars[cell] == b'T'
        });
        let multiplier = if pit {
            Catalog::compiled()
                .map_err(|e| e.to_string())?
                .table("FORTS")
                .map_err(|e| e.to_string())?
                .get("nest")
                .and_then(|v| v.get("range"))
                .and_then(Value::number)
                .ok_or("missing gun-pit range")?
        } else {
            1.0
        };
        Ok(n(weapon, "range", 0.0) * multiplier)
    }

    pub(super) fn weapon_start_cell(&self, unit: &Unit) -> Result<usize, String> {
        let g = &self.world.game;
        if unit.air.is_some() && g.w > 0 && g.h > 0 {
            Ok(g.cell_of(unit.x, unit.z).unwrap_or(usize::MAX))
        } else {
            g.cell_of(unit.x, unit.z)
                .ok_or_else(|| "shooter outside battlefield".into())
        }
    }

    pub(super) fn flight_crossed_cells(
        &self,
        a: projectiles::Point3,
        b: projectiles::Point3,
    ) -> Result<Vec<usize>, String> {
        let g = &self.world.game;
        let dx = b.x - a.x;
        let dz = b.z - a.z;
        if ![a.x, a.z, b.x, b.z, dx, dz].iter().all(|v| v.is_finite()) {
            return Err("non-finite flight cell traversal".into());
        }
        let mut enter: f64 = 0.0;
        let mut exit: f64 = 1.0;
        for (origin, delta, extent) in [(a.x, dx, g.w as f64 * CELL), (a.z, dz, g.h as f64 * CELL)]
        {
            if delta == 0.0 {
                if origin < 0.0 || origin >= extent {
                    return Ok(Vec::new());
                }
            } else {
                let first = -origin / delta;
                let last = (extent - origin) / delta;
                enter = math::max(enter, math::min(first, last));
                exit = math::min(exit, math::max(first, last));
                if enter > exit {
                    return Ok(Vec::new());
                }
            }
        }
        // Clip only cell enumeration. Sweeps retain the original segment and fractions.
        projectiles::crossed_cells(
            projectiles::BodyPosition {
                x: if enter == 0.0 { a.x } else { a.x + dx * enter },
                z: if enter == 0.0 { a.z } else { a.z + dz * enter },
            },
            projectiles::BodyPosition {
                x: if exit == 1.0 { b.x } else { a.x + dx * exit },
                z: if exit == 1.0 { b.z } else { a.z + dz * exit },
            },
            CELL,
            g.w,
            g.h,
        )
        .map_err(|e| format!("flight cell traversal: {e:?}"))
    }

    pub(super) fn weapon_launch_height(&self, unit: &Unit) -> f64 {
        if unit.air.is_some() {
            20.0
        } else if unit.garrison.is_some() {
            3.5
        } else {
            1.2
        }
    }
}
