//! Source sampled infantry ray selection, independent of damage and event publication.
use crate::{
    catalog::Catalog,
    companions::{Input, OPERATIVE, Soldier, WEAPONS},
    game::{Game, Point},
    math,
    projectiles::Point3,
    rules::Rules,
    squad_men, terrain, vision,
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RayHit {
    pub unit: u32,
    pub owner: usize,
    pub man: Option<u32>,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RayResult {
    pub end: Point3,
    /// Friendly bodies stop the ray but produce no enemy-hit record.
    pub enemy_hit: Option<RayHit>,
}
struct Body {
    unit: u32,
    at: Point,
    radius: f64,
    top: f64,
    jump: f64,
    man: Option<u32>,
}
fn height(g: &Game, rules: &Rules, at: Point) -> f64 {
    g.height
        .as_ref()
        .map_or(0.0, |h| terrain::height_at(g, rules, h, at.x, at.z))
}

/// The caller supplies exactly the source radius draw followed by its azimuth draw.
/// This pure preparation consumes no randomness or unit state.
#[allow(clippy::too_many_arguments)]
pub fn trace(
    g: &Game,
    rules: &Rules,
    soldiers: &BTreeMap<u32, Soldier>,
    unit_order: &[u32],
    shooter: u32,
    input: &Input,
    moving: f64,
    draws: [f64; 2],
) -> Result<RayResult, &'static str> {
    let cells =
        g.w.checked_mul(g.h)
            .filter(|n| *n > 0 && *n <= 4_000_000)
            .ok_or("invalid ray terrain dimensions")?;
    let ordered: BTreeSet<_> = unit_order.iter().copied().collect();
    if g.flags.len() != cells
        || g.height.as_ref().is_some_and(|h| h.len() != cells)
        || ordered.len() != unit_order.len()
        || ordered.len() != g.units.len()
        || ordered.iter().any(|id| !g.units.contains_key(id))
        || !moving.is_finite()
        || moving < 0.0
        || !input.yaw.is_finite()
        || input.yaw.abs() > 1e4
        || !input.pitch.is_finite()
        || input.pitch.abs() > 1.45
        || draws
            .iter()
            .any(|n| !n.is_finite() || !(0.0..1.0).contains(n))
        || g.smokes
            .iter()
            .any(|s| ![s.x, s.z, s.r].iter().all(|v| v.is_finite()) || s.r < 0.0)
    {
        return Err("invalid ray context");
    }
    let u = g.units.get(&shooter).ok_or("missing ray shooter")?;
    let soldier = soldiers.get(&shooter).ok_or("missing ray soldier")?;
    let w = WEAPONS.get(soldier.weapon).ok_or("invalid ray weapon")?;
    if u.owner >= g.players.len()
        || ![u.x, u.z, soldier.jy].iter().all(|n| n.is_finite())
        || soldier.jy < 0.0
        || g.cell_of(u.x, u.z).is_none()
    {
        return Err("invalid ray shooter pose");
    }
    let eye = height(g, rules, u.at())
        + soldier.jy
        + if soldier.crouch {
            OPERATIVE.crouch_eye
        } else {
            OPERATIVE.eye
        };
    let spread = (if input.ads { w.ads } else { w.hip })
        * (1.0 + math::min(1.5, moving / OPERATIVE.walk))
        * if soldier.jy > 0.0 { 3.0 } else { 1.0 }
        * if soldier.crouch { 0.6 } else { 1.0 };
    let r = spread * math::pow(draws[0], 0.5);
    let angle = draws[1] * std::f64::consts::PI * 2.0;
    let yaw = input.yaw + math::cos(angle) * r;
    let pitch = math::max(-1.5, math::min(1.5, input.pitch + math::sin(angle) * r));
    let hx = math::cos(yaw);
    let hz = math::sin(yaw);
    let cp = math::cos(pitch);
    let dy = math::sin(pitch);
    let mut targets = Vec::new();
    let near = |body: Body, targets: &mut Vec<Body>| {
        let rx = body.at.x - u.x;
        let rz = body.at.z - u.z;
        let along = rx * hx + rz * hz;
        if along > -body.radius
            && along < w.range + body.radius
            && (rx * hz - rz * hx).abs() <= body.radius
        {
            targets.push(body);
        }
    };
    for id in unit_order {
        let target = &g.units[id];
        if *id == shooter
            || target.hp <= 0.0
            || target.air.is_some()
            || target.riding
            || !g.players[u.owner].sees(*id)
        {
            continue;
        }
        let def = rules
            .kinds
            .get(target.kind)
            .ok_or("invalid ray target kind")?;
        if target.owner >= g.players.len()
            || ![
                target.x,
                target.z,
                target.rot,
                target.hp,
                target.vx.unwrap_or(0.0),
                target.vz.unwrap_or(0.0),
                def.radius,
            ]
            .iter()
            .all(|v| v.is_finite())
            || def.radius < 0.0
            || g.cell_of(target.x, target.z).is_none()
        {
            return Err("invalid ray target pose");
        }
        let operative = soldiers.get(id);
        if operative.is_some_and(|s| !s.jy.is_finite() || s.jy < 0.0) {
            return Err("invalid ray target height");
        }
        let top = if let Some(body) = operative {
            if body.crouch { 1.25 } else { 1.85 }
        } else if def.infantry {
            1.85
        } else if def.structure {
            5.0
        } else {
            3.0
        };
        let jump = operative.map_or(0.0, |s| s.jy);
        if def.infantry
            && operative.is_none()
            && target.garrison.is_none()
            && !vision::in_trench(g, rules, target)
        {
            let catalog = Catalog::compiled().map_err(|_| "ray catalog unavailable")?;
            let definition = catalog
                .unit(&def.name)
                .map_err(|_| "ray definition unavailable")?;
            let models = definition
                .get("models")
                .and_then(|v| v.number())
                .ok_or("missing drawn model count")?;
            let hp_per = definition
                .get("hpPer")
                .and_then(|v| v.number())
                .ok_or("missing drawn model health")?;
            let men = squad_men::drawn_men(
                &def.name,
                target.hp,
                models * hp_per,
                target.at(),
                target.rot,
                Point {
                    x: target.vx.unwrap_or(0.0),
                    z: target.vz.unwrap_or(0.0),
                },
            )?;
            for (man, at) in men.positions.into_iter().enumerate() {
                near(
                    Body {
                        unit: *id,
                        at,
                        radius: 0.45,
                        top,
                        jump,
                        man: Some(man as u32),
                    },
                    &mut targets,
                );
            }
        } else {
            let radius = if operative.is_some() {
                0.4
            } else if def.infantry {
                1.2
            } else {
                def.radius
            };
            near(
                Body {
                    unit: *id,
                    at: target.at(),
                    radius,
                    top,
                    jump,
                    man: None,
                },
                &mut targets,
            );
        }
    }
    let mut end = Point3::new(u.x, eye, u.z);
    let mut hit = None;
    for i in 2..=math::round(w.range * 5.0) as usize {
        let d = i as f64 * 0.2;
        end = Point3::new(u.x + hx * cp * d, eye + dy * d, u.z + hz * cp * d);
        let at = Point { x: end.x, z: end.z };
        if g.cell_of(end.x, end.z).is_none() || end.y < height(g, rules, at) {
            break;
        }
        if i % 5 == 0 && !vision::los(g, rules, (u.x, u.z), (end.x, end.z)) {
            break;
        }
        let found = targets.iter().find(|b| {
            let floor = height(g, rules, b.at) + b.jump;
            math::hypot(end.x - b.at.x, end.z - b.at.z) <= b.radius
                && end.y >= floor
                && end.y <= floor + b.top
        });
        if let Some(body) = found {
            if vision::los(g, rules, (u.x, u.z), (end.x, end.z)) {
                let target = &g.units[&body.unit];
                if !g.allied(u.owner, target.owner) {
                    hit = Some(RayHit {
                        unit: body.unit,
                        owner: target.owner,
                        man: body.man,
                    });
                }
            }
            break;
        }
    }
    Ok(RayResult {
        end,
        enemy_hit: hit,
    })
}
