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

#[cfg(test)]
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
    plan_after_builds(
        observation,
        available,
        budget,
        build_reserve,
        saved,
        terrain,
        sees,
        steps,
        &[],
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn plan_after_builds(
    observation: &Observation,
    available: &[&UnitObservation],
    budget: f64,
    build_reserve: f64,
    saved: f64,
    terrain: &Game,
    sees: impl Fn(Point) -> bool,
    steps: &mut BTreeMap<u32, u8>,
    earlier: &[Value],
) -> Result<Plan, String> {
    let input = observation.input();
    let context = input
        .runtime
        .as_ref()
        .ok_or("missing fortification context")?;
    let team = input.players[input.slot].team;
    let allowed = |at| {
        sees(at)
            && (context.mode != Mode::World
                || context.world.as_ref().is_some_and(|facts| {
                    crate::commander_world_policy::owned_ground(facts, team, at)
                }))
    };
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
                && !u.riding
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
        || input.units.iter().any(|u| {
            u.owner == input.slot
                && u.hp > 0.0
                && (u.entrench || context.units.get(&u.id).is_some_and(|f| f.wall_work))
        })
    {
        return Ok(result);
    }
    let tier = input.research.as_ref().map_or(3, |r| r.tech.tier());
    let spare = math::max(budget, budget + saved - 300.0) - build_reserve;
    for hq in hqs {
        let step = *steps.get(&hq.id).unwrap_or(&0);
        if context.mode == Mode::World && (step >= 6 || step > 0 && tier < 2) {
            continue;
        }
        let center = Point {
            x: input.width as f64 * CELL / 2.0,
            z: input.height as f64 * CELL / 2.0,
        };
        let threat = if context.mode == Mode::World {
            context
                .world
                .as_ref()
                .and_then(|facts| {
                    facts
                        .regions
                        .iter()
                        .filter(|region| region.team != Some(team))
                        .min_by(|a, b| {
                            distance(hq.at, a.at)
                                .total_cmp(&distance(hq.at, b.at))
                                .then(a.id.cmp(&b.id))
                        })
                        .map(|region| region.at)
                })
                .unwrap_or(center)
        } else {
            crate::sort::first_by(
                input.players.iter().enumerate().filter(|(seat, p)| {
                    p.team != team && !p.out && !context.hidden_spawns.contains(seat)
                }),
                |(_, a), (_, b)| distance(hq.at, a.spawn) - distance(hq.at, b.spawn),
            )
            .map_or(center, |(_, p)| p.spawn)
        };
        let angle = math::atan2(threat.z - hq.at.z, threat.x - hq.at.x);
        let fx = math::cos(angle);
        let fz = math::sin(angle);
        crate::sort::sort_by(&mut crew, |a, b| {
            distance(a.at, hq.at) - distance(b.at, hq.at)
        });
        if step >= 6 || step > 0 && tier < 2 {
            continue;
        }
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
            if let Some(at) = site(observation, terrain, kind, aim, allowed, earlier)? {
                result.commands.push(json!({"t":"build","ids":[worker.id],"kind":kind,"x":at.x,"z":at.z,"dir":angle}));
                result.remaining_mp -= cost;
                result.builders.push(worker.id);
            }
        } else {
            // Concrete fronts and their three-cell gate share a cardinal grid direction.
            let square = crate::field_works::square_direction(angle);
            let wx = math::round(math::cos(square));
            let wz = math::round(math::sin(square));
            let mid = Point {
                x: hq.at.x + wx * 24.0,
                z: hq.at.z + wz * 24.0,
            };
            let (px, pz) = (-wz, wx);
            let at_side = |side: f64| Point {
                x: mid.x + px * side,
                z: mid.z + pz * side,
            };
            let ids: Vec<_> = crew.iter().take(2).map(|u| u.id).collect();
            let line = |from: Point, to: Point, queue: bool| {
                json!({"t":"entrench", "ids":ids, "pattern":"line", "fort":"wall",
                    "x":from.x, "z":from.z, "x2":to.x, "z2":to.z, "queue":queue})
            };
            match step {
                2 => {
                    let gap = 3.0 * CELL / 2.0 + 1.0;
                    for (side, queued) in [(1.0, false), (-1.0, true)] {
                        let from = at_side(gap * side);
                        let to = at_side(14.0 * side);
                        if context.mode != Mode::World
                            || world_line_site(terrain, from, to, worker.at, allowed)?
                        {
                            result.commands.push(line(
                                from,
                                to,
                                queued && !result.commands.is_empty(),
                            ));
                        }
                    }
                    // Projects pay for their segments later through the field-work wallet.
                    if !result.commands.is_empty() {
                        result.builders.extend(ids.iter().copied());
                    }
                }
                3 => {
                    let direction = math::atan2(pz, px);
                    if gate_site(terrain, mid, direction, allowed)? {
                        result.commands.push(json!({"t":"dig", "kind":"gate",
                            "ids":[worker.id], "x":mid.x, "z":mid.z, "dir":direction}));
                        result.remaining_mp -= catalog
                            .table("FORTS")
                            .map_err(|e| e.to_string())?
                            .get("gate")
                            .map(|d| number(d, "cost"))
                            .ok_or("missing gate cost")?;
                        result.builders.push(worker.id);
                    }
                }
                4 | 5 => {
                    let end = at_side(if step == 4 { 14.0 } else { -14.0 });
                    let to = Point {
                        x: end.x - wx * 14.0,
                        z: end.z - wz * 14.0,
                    };
                    if context.mode != Mode::World
                        || world_line_site(terrain, end, to, worker.at, allowed)?
                    {
                        result.commands.push(line(end, to, false));
                        result.builders.extend(ids.iter().copied());
                    }
                }
                _ => unreachable!(),
            }
        }
        // Source attempts each placement once, including a visible-site refusal.
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
    earlier: &[Value],
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
    avoid.extend(
        input
            .players
            .iter()
            .enumerate()
            .filter(|(seat, _)| {
                input
                    .runtime
                    .as_ref()
                    .is_none_or(|context| !context.hidden_spawns.contains(seat))
            })
            .map(|(_, p)| (p.spawn, 6.0)),
    );
    for command in earlier.iter().filter(|c| c["t"] == "build") {
        let kind = command["kind"]
            .as_str()
            .ok_or("missing planned building kind")?;
        let at = Point {
            x: command["x"].as_f64().ok_or("missing planned building x")?,
            z: command["z"].as_f64().ok_or("missing planned building z")?,
        };
        let definition = catalog.unit(kind).map_err(|e| e.to_string())?;
        avoid.push((at, number(definition, "radius") + half + 4.0));
    }
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
    .filter(|&cell| {
        input
            .runtime
            .as_ref()
            .is_none_or(|context| context.mode != Mode::World)
            || crate::match_runtime::footprint(&ground, cell, size).is_some_and(|cells| {
                cells.into_iter().all(|cell| {
                    sees(Point {
                        x: (cell % ground.w) as f64 * CELL + CELL / 2.0,
                        z: (cell / ground.w) as f64 * CELL + CELL / 2.0,
                    })
                })
            })
    })
    .map(|cell| foot_center(&ground, cell, size))
    .filter(|at| sees(*at)))
}

/// Check every candidate cell of a World line before issuing a shared project.
fn world_line_site(
    terrain: &Game,
    from: Point,
    to: Point,
    back: Point,
    sees: impl Fn(Point) -> bool,
) -> Result<bool, String> {
    use crate::field_works::{FortKind, Pattern, entrench_plan};
    let mut buildable = false;
    for segment in entrench_plan(Pattern::Line, from, to, Some(back), FortKind::Wall)
        .map_err(|e| e.to_string())?
    {
        if !fort_site(terrain, "wall", segment.at, segment.direction, &sees)? {
            return Ok(false);
        }
        buildable = true;
    }
    Ok(buildable)
}

/// The Gate branch of the shared placement check, over delivered terrain only.
/// Every in-map planned cell must be visible, and at least one must be buildable.
fn gate_site(
    terrain: &Game,
    at: Point,
    direction: f64,
    sees: impl Fn(Point) -> bool,
) -> Result<bool, String> {
    fort_site(
        terrain,
        "gate",
        at,
        crate::field_works::square_direction(direction),
        sees,
    )
}

fn fort_site(
    terrain: &Game,
    kind: &str,
    at: Point,
    direction: f64,
    sees: impl Fn(Point) -> bool,
) -> Result<bool, String> {
    let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
    let count = catalog
        .table("FORTS")
        .map_err(|e| e.to_string())?
        .get(kind)
        .map(|d| number(d, "n"))
        .ok_or("missing fortification cells")? as usize;
    let (dx, dz) = (math::cos(direction), math::sin(direction));
    let mut buildable = false;
    for index in 0..count {
        let along = (index as f64 - (count as f64 - 1.0) / 2.0) * CELL;
        let Some(cell) = terrain.cell_of(at.x + dx * along, at.z + dz * along) else {
            continue;
        };
        let center = Point {
            x: (cell % terrain.w) as f64 * CELL + CELL / 2.0,
            z: (cell / terrain.w) as f64 * CELL + CELL / 2.0,
        };
        if !sees(center) {
            return Ok(false);
        }
        buildable |= b".+RDM".contains(&terrain.chars[cell]);
    }
    Ok(buildable)
}

#[cfg(test)]
#[path = "commander_field_hq_tests.rs"]
mod tests;
