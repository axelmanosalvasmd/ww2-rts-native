//! Fortification spending and shopping reserves from this seat's delivered facts.
use crate::{
    catalog::{Catalog, Value as Rule},
    commander_ai::{Observation, Point, UnitObservation},
    economy::Mode,
    game::{CELL, Game, MOVE},
    match_runtime::{find_site, foot_center},
    math,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub(crate) struct Plan {
    pub commands: Vec<Value>,
    pub remaining_mp: f64,
    pub reserve: f64,
    pub builders: Vec<u32>,
}
fn distance(a: Point, b: Point) -> f64 {
    math::hypot(a.x - b.x, a.z - b.z)
}
fn number(d: &Rule, key: &str) -> f64 {
    d.get(key).and_then(Rule::number).unwrap_or(0.0)
}
fn flag(d: &Rule, key: &str) -> bool {
    d.get(key).and_then(Rule::boolean).unwrap_or(false)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn plan(
    observation: &Observation,
    available: &[&UnitObservation],
    budget: f64,
    build_reserve: f64,
    saved: f64,
    terrain: &Game,
    sees: impl Fn(Point) -> bool,
    steps: &mut BTreeMap<u32, u8>,
) -> Result<Plan, String> {
    let input = observation.input();
    let context = input
        .runtime
        .as_ref()
        .ok_or("missing fortification context")?;
    let mut result = Plan {
        commands: vec![],
        remaining_mp: budget,
        reserve: 0.0,
        builders: vec![],
    };
    if !(context.mode.is_construction() || context.mode.is_skirmish())
        || context.horde
        || input.tick < 3000
        || context.mode == Mode::Assault && !context.defender
    {
        return Ok(result);
    }
    let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
    let def = |kind: &str| catalog.unit(kind).map_err(|e| e.to_string());
    let builders = catalog
        .table("CFG")
        .map_err(|e| e.to_string())?
        .get("fortBuilders")
        .and_then(Rule::array)
        .ok_or("missing fort builders")?;
    let mut crew: Vec<_> = available
        .iter()
        .copied()
        .filter(|u| {
            u.owner == input.slot
                && u.hp > 0.0
                && (if context.mode.is_construction() {
                    u.kind == "engineer"
                } else {
                    builders.iter().any(|k| k.string() == Some(u.kind.as_str()))
                })
                && !u.retreating
                && !u.build
                && u.target_id == 0
                && !u.dig
                && !u.entrench
                && !u.garrison
        })
        .collect();
    let own: Vec<_> = input
        .units
        .iter()
        .filter(|u| {
            u.owner == input.slot && u.hp > 0.0 && def(&u.kind).is_ok_and(|d| flag(d, "building"))
        })
        .collect();
    let built = |u: &UnitObservation| context.units.get(&u.id).map_or(0.0, |f| f.built);
    let hqs: Vec<_> = own
        .iter()
        .copied()
        .filter(|u| u.kind == "hq" && built(u) >= 1.0)
        .collect();
    steps.retain(|id, _| hqs.iter().any(|u| u.id == *id));
    if crew.is_empty()
        || own.iter().any(|u| {
            matches!(u.kind.as_str(), "tower" | "pillbox" | "wall" | "gate") && built(u) < 1.0
        })
        || available.iter().any(|u| u.entrench || u.dig)
    {
        return Ok(result);
    }
    let tier = input.research.as_ref().map_or(3, |r| r.tech.tier());
    let spare = math::max(budget, budget + saved - 300.0) - build_reserve;
    for hq in hqs {
        let step = *steps.get(&hq.id).unwrap_or(&0);
        if step >= 6 || step > 0 && tier < 2 {
            continue;
        }
        let threat = input
            .players
            .iter()
            .enumerate()
            .filter(|(_, p)| p.team != input.players[input.slot].team && !p.out)
            .min_by(|(ai, a), (bi, b)| {
                distance(hq.at, a.spawn)
                    .total_cmp(&distance(hq.at, b.spawn))
                    .then(ai.cmp(bi))
            })
            .map_or(
                Point {
                    x: input.width as f64 * CELL / 2.0,
                    z: input.height as f64 * CELL / 2.0,
                },
                |(_, p)| p.spawn,
            );
        let angle = math::atan2(threat.z - hq.at.z, threat.x - hq.at.x);
        let fx = math::cos(angle);
        let fz = math::sin(angle);
        crew.sort_by(|a, b| {
            distance(a.at, hq.at)
                .total_cmp(&distance(b.at, hq.at))
                .then(a.id.cmp(&b.id))
        });
        let worker = crew[0];
        let wall_cost = catalog
            .table("FORTS")
            .map_err(|e| e.to_string())?
            .get("wall")
            .map(|d| number(d, "cost"))
            .ok_or("missing wall cost")?;
        let cost = match step {
            0 => number(def("tower")?, "cost"),
            1 => number(def("pillbox")?, "cost"),
            _ => wall_cost * 2.0,
        };
        if spare < cost {
            result.reserve = cost;
            return Ok(result);
        }
        if step <= 1 {
            let (kind, forward, side) = if step == 0 {
                ("tower", 10.0, 6.0)
            } else {
                ("pillbox", 14.0, 0.0)
            };
            let aim = Point {
                x: hq.at.x + fx * forward - fz * side,
                z: hq.at.z + fz * forward + fx * side,
            };
            if let Some(at) = site(observation, terrain, kind, aim, &sees)? {
                result.commands.push(json!({"t":"build","ids":[worker.id],"kind":kind,"x":at.x,"z":at.z,"dir":angle}));
                result.remaining_mp -= cost;
                result.builders.push(worker.id);
            }
        }
        // Wall projects and gate Dig are refused by the current canonical callers.
        // Their unspent funds remain available to shopping; never charge a denied job.
        steps.insert(hq.id, step + 1);
        return Ok(result);
    }
    Ok(result)
}

fn site(
    observation: &Observation,
    terrain: &Game,
    kind: &str,
    aim: Point,
    sees: impl Fn(Point) -> bool,
) -> Result<Option<Point>, String> {
    let input = observation.input();
    let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
    let size = number(catalog.unit(kind).map_err(|e| e.to_string())?, "size") as usize;
    let half = size as f64 * CELL / 2.0;
    let mut ground = terrain.clone();
    let mut avoid: Vec<_> = input
        .points
        .iter()
        .map(|p| (p.at, observation.point_radius() + 2.0))
        .collect();
    avoid.extend(input.players.iter().map(|p| (p.spawn, 6.0)));
    for u in &input.units {
        if u.hp <= 0.0 {
            continue;
        }
        let d = catalog.unit(&u.kind).map_err(|e| e.to_string())?;
        if flag(d, "structure") {
            avoid.push((u.at, number(d, "radius") + half + 4.0));
        } else if !flag(d, "air")
            && !u.riding
            && let Some(cell) = ground.cell_of(u.at.x, u.at.z)
        {
            ground.flags[cell] |= MOVE;
        }
    }
    Ok(find_site(
        &ground,
        Point {
            x: aim.x - half,
            z: aim.z - half,
        },
        size,
        &avoid,
    )
    .map(|cell| foot_center(&ground, cell, size))
    .filter(|at| sees(*at)))
}
