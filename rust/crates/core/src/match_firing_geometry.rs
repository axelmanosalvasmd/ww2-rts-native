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
            let x = (unit.x / CELL).floor().clamp(0.0, (g.w - 1) as f64) as usize;
            let z = (unit.z / CELL).floor().clamp(0.0, (g.h - 1) as f64) as usize;
            Ok(z * g.w + x)
        } else {
            g.cell_of(unit.x, unit.z)
                .ok_or_else(|| "shooter outside battlefield".into())
        }
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
