//! Drawn infantry bodies from the pinned squad-men geometry, in source slot order.
use crate::{game::Point, math};

pub const SLOT_SCALE: f64 = 1.3;

fn block(columns: usize, rows: usize) -> Vec<Point> {
    (0..columns * rows)
        .map(|i| Point {
            x: ((rows as f64 - 1.0) / 2.0 - (i / columns) as f64) * 0.65,
            z: ((i % columns) as f64 - (columns as f64 - 1.0) / 2.0) * 0.65,
        })
        .collect()
}
fn slots(kind: &str) -> Result<Vec<Point>, &'static str> {
    let points: &[(f64, f64)] = match kind {
        "rifle" | "commando" => return Ok(block(5, 3)),
        "engineer" => return Ok(block(3, 2)),
        "ranger" => return Ok(block(6, 2)),
        "conscript" => return Ok(block(7, 3)),
        "mg" => &[(-0.12, 0.0), (-0.05, 0.85), (-0.05, -0.85)],
        "mortar" => &[(-0.45, 0.75), (-0.45, -0.75), (0.2, -1.0)],
        "at" => &[(-0.1, 1.15), (-0.1, -1.15), (-0.85, 1.5), (-0.85, -1.5)],
        "flak" => &[(0.0, 1.15), (0.0, -1.15), (-0.95, 1.2)],
        "howitzer" => &[(-0.35, 1.0), (-0.55, -0.95), (-1.3, 0.55), (-1.5, -0.6)],
        "sniper" => &[(0.4, 0.0), (-0.5, 0.6)],
        "medic" => &[(0.3, 0.4), (-0.3, -0.4)],
        "flamer" => &[(0.45, 0.0), (-0.25, 0.75), (-0.35, -0.7)],
        _ => return Err("unknown drawn infantry slots"),
    };
    Ok(points.iter().map(|&(x, z)| Point { x, z }).collect())
}

#[derive(Clone, Debug, PartialEq)]
pub struct DrawnMen {
    pub count: f64,
    pub positions: Vec<Point>,
}

/// Health determines drawn count, while movement trails every slot by velocity/9.
/// Count can exceed the slot count; positions retain source slice truncation.
pub fn drawn_men(
    kind: &str,
    hp: f64,
    max_hp: f64,
    at: Point,
    rotation: f64,
    velocity: Point,
) -> Result<DrawnMen, &'static str> {
    if ![hp, max_hp, at.x, at.z, rotation, velocity.x, velocity.z]
        .iter()
        .all(|n| n.is_finite())
        || hp < 0.0
        || max_hp <= 0.0
    {
        return Err("invalid drawn infantry geometry");
    }
    let slots = slots(kind)?;
    let count = (hp * slots.len() as f64 / max_hp).ceil();
    if !count.is_finite() || count > usize::MAX as f64 {
        return Err("drawn infantry count exhausted");
    }
    let c = math::cos(rotation);
    let s = math::sin(rotation);
    let lag_x = velocity.x / 9.0;
    let lag_z = velocity.z / 9.0;
    let positions = slots
        .into_iter()
        .take(count as usize)
        .map(|p| Point {
            x: at.x + (p.x * c - p.z * s) * SLOT_SCALE - lag_x,
            z: at.z + (p.x * s + p.z * c) * SLOT_SCALE - lag_z,
        })
        .collect();
    Ok(DrawnMen { count, positions })
}
