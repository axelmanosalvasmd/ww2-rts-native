//! Cover queries over recipient terrain and explicitly delivered hulls only.
use super::{KnowledgeError, RouteKnowledge};
use crate::{
    game::{COVER, MOVE, Point, TRENCH, Wreck},
    math,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoverClass {
    None,
    Open,
    Light,
    Heavy,
}

/// A delivered unit's cover-relevant facts. Callers exclude undisclosed units.
#[derive(Clone, Copy, Debug)]
pub struct CoverBody {
    pub at: Point,
    pub hp: f64,
    pub infantry: bool,
    pub air: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CoverPreview {
    pub class: CoverClass,
    pub directional: bool,
}

impl RouteKnowledge {
    /// Classify explored ground, then query the simulation's directional cover rule.
    /// This does not collect visibility or read a Match. Explored bits, bodies and
    /// settled wrecks must come from the recipient's delivered view.
    pub fn cover_preview(
        &self,
        at: Point,
        explored: &[bool],
        garrisons: bool,
        shooter: Option<Point>,
        bodies: &[CoverBody],
        wrecks: &[Wreck],
    ) -> Result<CoverPreview, KnowledgeError> {
        if explored.len() != self.known.len() {
            return Err(KnowledgeError::InvalidShape);
        }
        if shooter.is_some_and(|p| !p.x.is_finite() || !p.z.is_finite())
            || bodies
                .iter()
                .any(|b| !b.at.x.is_finite() || !b.at.z.is_finite() || !b.hp.is_finite())
            || wrecks.iter().any(|w| !w.x.is_finite() || !w.z.is_finite())
        {
            return Err(KnowledgeError::InvalidValue);
        }
        let c = self.cell_of(at).ok_or(KnowledgeError::InvalidCell)?;
        let class = if !explored[c] || !self.known[c] {
            CoverClass::None
        } else if self.physical(c) == b'B' {
            let x = (c % self.width) as isize;
            let y = (c / self.width) as isize;
            let edge = [(1, 0), (-1, 0), (0, 1), (0, -1)]
                .into_iter()
                .any(|(dx, dy)| {
                    let nx = x + dx;
                    let ny = y + dy;
                    if nx < 0 || ny < 0 || nx >= self.width as isize || ny >= self.height as isize {
                        return false;
                    }
                    let n = ny as usize * self.width + nx as usize;
                    self.known[n] && self.physical(n) != b'B' && self.flags[n] & MOVE == 0
                });
            if garrisons && edge {
                CoverClass::Heavy
            } else {
                CoverClass::None
            }
        } else if self.flags[c] & MOVE != 0 {
            CoverClass::None
        } else if self.flags[c] & TRENCH != 0 {
            CoverClass::Heavy
        } else if self.flags[c] & COVER != 0 {
            CoverClass::Light
        } else {
            CoverClass::Open
        };
        let mut directional = false;
        if class == CoverClass::Open
            && let Some(from) = shooter
        {
            directional = self.cover_behind(at, from, wrecks)? > 0.4;
            let angle = math::atan2(from.z - at.z, from.x - at.x);
            let ca = math::cos(angle);
            let sa = math::sin(angle);
            for body in bodies
                .iter()
                .filter(|b| b.hp > 0.0 && !b.infantry && !b.air)
            {
                let dx = body.at.x - at.x;
                let dz = body.at.z - at.z;
                let d = math::hypot(dx, dz);
                if d > 0.0
                    && d < 4.0
                    && (dx * ca + dz * sa) / d > 0.7
                    && math::min(1.0, (4.0 - d) / 1.5) > 0.4
                {
                    directional = true;
                }
            }
        }
        Ok(CoverPreview { class, directional })
    }
}
