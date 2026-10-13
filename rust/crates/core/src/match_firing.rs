//! Canonical weapon state for the authoritative Match firing phase.
//! Unit ownership, health and position remain in Game; command aim and intent
//! remain in UnitControl, and hit history and experience remain in CombatState.

use std::collections::BTreeMap;

use crate::{
    catalog::{Catalog, Value},
    game::{CELL, Point, Unit},
    grid::UnitId,
    match_runtime::Match,
    math, vision,
};

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

fn n(v: &Value, key: &str, fallback: f64) -> f64 {
    v.get(key).and_then(Value::number).unwrap_or(fallback)
}
fn flag(v: &Value, key: &str) -> bool {
    v.get(key).is_some_and(|v| match v {
        Value::Bool(b) => *b,
        Value::Number(n) => *n != 0.0 && !n.is_nan(),
        _ => false,
    })
}
fn dist(a: Point, b: Point) -> f64 {
    math::hypot(a.x - b.x, a.z - b.z)
}

impl Match {
    fn definition(&self, u: &Unit) -> Result<&'static Value, String> {
        let kind = self.rules.kinds.get(u.kind).ok_or("unknown firing kind")?;
        Catalog::compiled()
            .map_err(|e| e.to_string())?
            .unit(&kind.name)
            .map_err(|e| e.to_string())
    }
    fn weapon(&self, u: &Unit) -> Result<Option<&'static Value>, String> {
        Ok(self.definition(u)?.get("w"))
    }
    fn hull_point(&self, from: Point, target: &Unit) -> Point {
        let h = self.rules.kinds[target.kind].hull;
        if h == 0.0 {
            return target.at();
        }
        let cx = math::cos(target.rot);
        let cz = math::sin(target.rot);
        let k = math::max(
            -h,
            math::min(h, (from.x - target.x) * cx + (from.z - target.z) * cz),
        );
        Point {
            x: target.x + cx * k,
            z: target.z + cz * k,
        }
    }
    pub(crate) fn aim_point(&self, shooter: &Unit, target: &Unit) -> Point {
        let Some(cells) = &target.cells else {
            return self.hull_point(shooter.at(), target);
        };
        let mut best = target.at();
        let mut distance = f64::INFINITY;
        for &cell in cells {
            if self
                .world
                .game
                .sections
                .get(&cell)
                .is_some_and(|s| s.failed || s.hp <= 0.0)
            {
                continue;
            }
            let at = Point {
                x: (cell as usize % self.world.game.w) as f64 * CELL + CELL / 2.0,
                z: (cell as usize / self.world.game.w) as f64 * CELL + CELL / 2.0,
            };
            let d = dist(shooter.at(), at);
            if d < distance {
                distance = d;
                best = at;
            }
        }
        best
    }
    /// Current authoritative visibility, alliance, hull range and line-of-sight query.
    /// Salvos require spotting and minimum range instead of a direct sight line.
    pub fn can_shoot_target(&self, shooter: UnitId, target: UnitId) -> Result<bool, String> {
        let g = &self.world.game;
        let Some(u) = g.units.get(&shooter) else {
            return Ok(false);
        };
        let Some(t) = g.units.get(&target) else {
            return Ok(false);
        };
        if g.w == 0
            || g.h == 0
            || u.kind >= self.rules.kinds.len()
            || t.kind >= self.rules.kinds.len()
            || u.owner >= g.players.len()
            || t.owner >= g.players.len()
            || ![u.x, u.z, u.rot, t.x, t.z, t.rot, t.hp]
                .iter()
                .all(|v| v.is_finite())
            || t.cells
                .as_ref()
                .is_some_and(|cells| cells.iter().any(|&c| c as usize >= g.flags.len()))
        {
            return Err("invalid canonical targeting input".into());
        }
        let Some(w) = self.weapon(u)? else {
            return Ok(false);
        };
        let range = n(w, "range", 0.0);
        if range == 0.0
            || !self.logistics.can_fire(u.id, 1.0)
            || t.hp <= 0.0
            || t.air.is_some()
            || t.riding
            || g.allied(t.owner, u.owner)
            || dist(u.at(), self.hull_point(u.at(), t)) > range
            || !g.players.get(u.owner).is_some_and(|p| p.sees(t.id))
        {
            return Ok(false);
        }
        if u.air.is_some() {
            return Err("air weapon firing is unsupported".into());
        }
        Ok(if flag(w, "salvo") {
            dist(u.at(), self.hull_point(u.at(), t)) >= n(w, "minRange", f64::NAN)
        } else {
            let at = self.aim_point(u, t);
            vision::los(g, &self.rules, (u.x, u.z), (at.x, at.z))
        })
    }
}
