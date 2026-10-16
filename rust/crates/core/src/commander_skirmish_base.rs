//! Stateless skirmish construction over delivered seat facts.
use crate::{
    catalog::{Catalog, Value},
    commander_ai::{Observation, Point, UnitObservation},
    economy::{self, Mode},
    game::{CELL, Game, LAND, MOVE},
    match_runtime::{find_site, foot_center, footprint},
    math,
};

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct BasePlan {
    pub commands: Vec<serde_json::Value>,
    pub reserve: f64,
    pub spent: f64,
    pub assigned: Option<u32>,
}
fn number(d: &Value, key: &str) -> Result<f64, String> {
    d.get(key)
        .and_then(Value::number)
        .ok_or_else(|| format!("missing skirmish base rule {key}"))
}
fn flag(d: &Value, key: &str) -> bool {
    d.get(key).and_then(Value::boolean).unwrap_or(false)
}
fn distance(a: Point, b: Point) -> f64 {
    math::hypot(a.x - b.x, a.z - b.z)
}

pub(crate) fn plan(
    observation: &Observation,
    squads: &[&UnitObservation],
    budget: f64,
    terrain: &Game,
    sees: impl Fn(Point) -> bool,
) -> Result<BasePlan, String> {
    let input = observation.input();
    let context = input
        .runtime
        .as_ref()
        .ok_or("missing skirmish base context")?;
    let mut result = BasePlan::default();
    if !context.mode.is_skirmish() || context.horde {
        return Ok(result);
    }
    let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
    let definition = |u: &UnitObservation| catalog.unit(&u.kind).map_err(|e| e.to_string());
    let team = input.players[input.slot].team;
    let built = |u: &UnitObservation| context.units.get(&u.id).map_or(0.0, |f| f.built);
    let own: Vec<_> = input
        .units
        .iter()
        .filter(|u| {
            u.hp > 0.0
                && (u.owner == input.slot
                    || context.mode == Mode::Horde && input.players[u.owner].team == team)
                && definition(u).is_ok_and(|d| flag(d, "building"))
        })
        .collect();
    let has = |kind: &str, done: bool| {
        own.iter()
            .any(|u| u.kind == kind && (!done || built(u) >= 1.0))
    };
    let mut next = if !has("hq", false) {
        Some("hq")
    } else if !has("barracks", false) {
        Some("barracks")
    } else if has("barracks", true) && !has("motorpool", false) {
        Some("motorpool")
    } else if has("motorpool", true) && !has("airfield", false) {
        Some("airfield")
    } else if context.naval && !has("shipyard", false) {
        Some("shipyard")
    } else {
        None
    };
    if let Some(research) = &input.research {
        if let Some(kind) = next
            && economy::tier_of(kind).map_err(|e| e.to_string())? > research.tech.tier()
        {
            next = None;
        }
        if next.is_none() && research.tech.tier() >= 2 && !has("armory", false) {
            next = Some("armory");
        }
    }
    result.reserve = next
        .map(|kind| {
            catalog
                .unit(kind)
                .map_err(|e| e.to_string())
                .and_then(|d| number(d, "cost"))
        })
        .transpose()?
        .unwrap_or(0.0);
    let builders = catalog
        .table("CFG")
        .map_err(|e| e.to_string())?
        .get("fortBuilders")
        .and_then(Value::array)
        .ok_or("missing skirmish builders")?;
    let builder = squads
        .iter()
        .copied()
        .filter(|u| {
            u.owner == input.slot
                && u.hp > 0.0
                && builders.iter().any(|v| v.string() == Some(u.kind.as_str()))
                && !u.retreating
                && !u.build
                && u.target_id == 0
                && !u.dig
                && !u.entrench
                && !u.garrison
        })
        .min_by(|a, b| {
            distance(a.at, input.players[input.slot].spawn)
                .total_cmp(&distance(b.at, input.players[input.slot].spawn))
                .then(a.id.cmp(&b.id))
        });
    let Some(builder) = builder else {
        return Ok(result);
    };
    if squads.iter().any(|u| u.build) {
        return Ok(result);
    }
    let fix = own
        .iter()
        .copied()
        .filter(|u| {
            built(u) < 1.0
                || definition(u).is_ok_and(|d| number(d, "hpPer").is_ok_and(|hp| u.hp < hp * 0.8))
        })
        .min_by(|a, b| {
            (built(a) >= 1.0)
                .cmp(&(built(b) >= 1.0))
                .then(distance(a.at, builder.at).total_cmp(&distance(b.at, builder.at)))
                .then(a.id.cmp(&b.id))
        });
    if let Some(fix) = fix {
        result
            .commands
            .push(serde_json::json!({"t":"assist","ids":[builder.id],"id":fix.id}));
        result.assigned = Some(builder.id);
        return Ok(result);
    }
    if let Some(forward) =
        crate::commander_forward_hq::plan(observation, squads, budget, terrain, &sees)?
    {
        result.commands.push(forward.command);
        result.spent = forward.cost;
        result.assigned = Some(forward.unit);
        result.reserve = 0.0;
        return Ok(result);
    }
    let Some(kind) = next else {
        return Ok(result);
    };
    if budget < result.reserve {
        return Ok(result);
    }
    let d = catalog.unit(kind).map_err(|e| e.to_string())?;
    let size = number(d, "size")? as usize;
    let home = own
        .iter()
        .find(|u| u.kind == "hq")
        .map_or(input.players[input.slot].spawn, |u| u.at);
    for radius in [12.0, 20.0, 30.0, 42.0] {
        for k in 0..8 {
            let a = k as f64 * std::f64::consts::FRAC_PI_4;
            let aim = Point {
                x: home.x + math::cos(a) * radius,
                z: home.z + math::sin(a) * radius,
            };
            let Some(at) = site(observation, terrain, aim, size, flag(d, "coast"), &sees)? else {
                continue;
            };
            result.commands.push(
                serde_json::json!({"t":"build","ids":[builder.id],"kind":kind,"x":at.x,"z":at.z}),
            );
            result.spent = result.reserve;
            result.reserve = 0.0;
            result.assigned = Some(builder.id);
            return Ok(result);
        }
    }
    Ok(result)
}

/// Source siteNear plus the visible placement and coastal checks, using only delivered facts.
pub(crate) fn site(
    observation: &Observation,
    terrain: &Game,
    aim: Point,
    size: usize,
    coastal: bool,
    sees: impl Fn(Point) -> bool,
) -> Result<Option<Point>, String> {
    let input = observation.input();
    let context = input.runtime.as_ref().ok_or("missing site context")?;
    let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
    let half = size as f64 * CELL / 2.0;
    let mut ground = terrain.clone();
    let mut avoid: Vec<_> = input
        .points
        .iter()
        .map(|p| (p.at, observation.point_radius() + 2.0))
        .collect();
    avoid.extend(context.site_nodes.iter().map(|p| (*p, 5.0)));
    avoid.extend(input.players.iter().map(|p| (p.spawn, 6.0)));
    for u in &input.units {
        if u.hp <= 0.0 {
            continue;
        }
        let def = catalog.unit(&u.kind).map_err(|e| e.to_string())?;
        if flag(def, "structure") {
            avoid.push((u.at, number(def, "radius")? + half + 4.0));
        } else if !flag(def, "air")
            && !u.riding
            && let Some(c) = ground.cell_of(u.at.x, u.at.z)
        {
            ground.flags[c] |= MOVE;
        }
    }
    let Some(cell) = find_site(
        &ground,
        Point {
            x: aim.x - half,
            z: aim.z - half,
        },
        size,
        &avoid,
    ) else {
        return Ok(None);
    };
    let at = foot_center(&ground, cell, size);
    if !sees(at) {
        return Ok(None);
    }
    let Some(cells) = footprint(&ground, cell, size) else {
        return Ok(None);
    };
    if context.site_nodes.iter().any(|node| {
        let x = math::round(node.x / CELL - 1.0);
        let y = math::round(node.z / CELL - 1.0);
        x >= 0.0
            && y >= 0.0
            && x < ground.w as f64
            && y < ground.h as f64
            && footprint(&ground, y as usize * ground.w + x as usize, 2)
                .is_some_and(|reserved| cells.iter().any(|cell| reserved.contains(cell)))
    }) {
        return Ok(None);
    }
    if coastal
        && (!context.naval
            || !cells.iter().any(|&c| {
                let x = (c % ground.w) as isize;
                let z = (c / ground.w) as isize;
                (-2..=2).any(|dz| {
                    (-2..=2).any(|dx| {
                        let (x, z) = (x + dx, z + dz);
                        x >= 0
                            && z >= 0
                            && x < ground.w as isize
                            && z < ground.h as isize
                            && ground.flags[z as usize * ground.w + x as usize] & LAND == 0
                    })
                })
            }))
    {
        return Ok(None);
    }
    Ok(Some(at))
}

#[cfg(test)]
#[path = "commander_skirmish_base_tests.rs"]
mod tests;
