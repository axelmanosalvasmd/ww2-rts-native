//! Classic construction intents over detached recipient facts only.
use crate::{
    catalog::{Catalog, Value as Rule},
    commander_ai::{Level, Observation, Point, UnitObservation},
    commander_mind::Sighting,
    commander_tactics::Skill,
    economy::{self, Mode},
    game::{CELL, Game, MOVE},
    match_runtime::{find_site, foot_center},
    math,
    random::{MatchRandom, StreamId},
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) struct Economy {
    pub commands: Vec<Value>,
    pub reserve: f64,
    pub remaining_mp: f64,
    pub free_nodes: usize,
    pub rush: bool,
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
    recent: &[&Sighting],
    memory: &mut BTreeMap<u32, usize>,
    level: Level,
    budget: f64,
    terrain: &Game,
    random: &mut MatchRandom,
) -> Result<Economy, String> {
    let input = observation.input();
    let context = input
        .runtime
        .as_ref()
        .ok_or("missing Classic economy context")?;
    let mut result = Economy {
        commands: vec![],
        reserve: 0.0,
        remaining_mp: budget,
        free_nodes: 0,
        rush: false,
    };
    // World uses different ownership and frontier rules and is deliberately excluded.
    if context.mode != Mode::Classic {
        return Ok(result);
    }
    let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
    let def = |kind: &str| catalog.unit(kind).map_err(|e| e.to_string());
    let own: Vec<_> = input
        .units
        .iter()
        .filter(|u| {
            u.owner == input.slot && u.hp > 0.0 && def(&u.kind).is_ok_and(|d| flag(d, "building"))
        })
        .collect();
    let built = |u: &UnitObservation| context.units.get(&u.id).map_or(0.0, |f| f.built);
    let has = |kind: &str, done: bool| {
        own.iter()
            .any(|u| u.kind == kind && (!done || built(u) >= 1.0))
    };
    let hq = own
        .iter()
        .find(|u| u.kind == "hq")
        .map_or(input.players[input.slot].spawn, |u| u.at);
    let depots = own.iter().filter(|u| u.kind == "depot").count();
    let free: Vec<_> = context
        .nodes
        .iter()
        .enumerate()
        .filter(|(i, _)| !context.claimed_nodes.contains(i))
        .collect();
    result.free_nodes = free.len();
    let now = input.tick as f64 / 20.0;
    let adaptive = Skill::for_level(level).adaptive;
    result.rush = adaptive
        && now < 240.0
        && recent
            .iter()
            .any(|s| now - s.time < 5.0 && own.iter().any(|b| distance(b.at, s.at) < 35.0));
    let need_armor = (adaptive || Skill::for_level(level).memory)
        && recent
            .iter()
            .any(|s| matches!(s.kind.as_str(), "tank" | "medium" | "tiger" | "churchill"));
    let team = input.players[input.slot].team;
    let planes = input.units.iter().any(|u| {
        input.players[u.owner].team != team
            && u.hp > 0.0
            && def(&u.kind).is_ok_and(|d| flag(d, "air"))
            && distance(u.at, hq) < 60.0
    });
    let mut next = if depots < 2
        && !free.is_empty()
        && !(need_armor && has("barracks", true) && !has("motorpool", false))
    {
        Some("depot")
    } else if !has("barracks", false) {
        Some("barracks")
    } else if has("barracks", true) && !has("motorpool", false) {
        Some("motorpool")
    } else if planes && own.iter().filter(|u| u.kind == "flakpos").count() < 2 {
        Some("flakpos")
    } else if !free.is_empty() {
        Some("depot")
    } else if has("motorpool", true) && !has("airfield", false) && depots >= 3 {
        Some("airfield")
    } else {
        None
    };
    if let Some(research) = &input.research {
        if let Some(kind) = next
            && economy::tier_of(kind).map_err(|e| e.to_string())? > research.tech.tier()
        {
            next = (!free.is_empty()).then_some("depot");
        }
        if next.is_none() && research.tech.tier() >= 2 && !has("armory", false) {
            next = Some("armory");
        }
    }
    let engineers: Vec<_> = available
        .iter()
        .copied()
        .filter(|u| u.kind == "engineer")
        .collect();
    let ids: BTreeSet<_> = engineers.iter().map(|u| u.id).collect();
    memory.retain(|id, _| ids.contains(id));
    let mut taken: BTreeSet<_> = memory.values().copied().collect();
    for u in engineers {
        if u.retreating || u.build {
            continue;
        }
        let fix = own
            .iter()
            .filter(|b| {
                built(b) < 1.0 || def(&b.kind).is_ok_and(|d| b.hp < number(d, "hpPer") * 0.7)
            })
            .min_by(|a, b| {
                distance(u.at, a.at)
                    .total_cmp(&distance(u.at, b.at))
                    .then(a.id.cmp(&b.id))
            });
        if let Some(fix) = fix
            && distance(u.at, fix.at) < 60.0
        {
            result
                .commands
                .push(json!({"t":"assist","ids":[u.id],"id":fix.id}));
            continue;
        }
        let Some(kind) = next else {
            continue;
        };
        let cost = number(def(kind)?, "cost");
        if kind == "depot" {
            let pick = free
                .iter()
                .filter(|(i, _)| !taken.contains(i) || memory.get(&u.id) == Some(i))
                .min_by(|(ai, a), (bi, b)| {
                    let score = |n: &crate::commander_tactics::NodeObservation| {
                        distance(u.at, n.at) - if n.fuel { 2.5 } else { n.rate } * 20.0
                    };
                    score(a).total_cmp(&score(b)).then(ai.cmp(bi))
                });
            let Some((index, node)) = pick else {
                continue;
            };
            memory.insert(u.id, *index);
            taken.insert(*index);
            if result.remaining_mp >= cost && node.visible {
                result.commands.push(
                    json!({"t":"build","ids":[u.id],"kind":"depot","x":node.at.x,"z":node.at.z}),
                );
                result.remaining_mp -= cost;
            } else if u.path.is_empty() {
                result
                    .commands
                    .push(json!({"t":"move","orders":[[u.id,node.at.x,node.at.z]]}));
            }
        } else {
            // The source object literal evaluates its flak expression for every non-depot next.
            let draw = random.next_fraction(StreamId::Ai(input.slot as u32));
            let offset = match kind {
                "barracks" => 0.9,
                "motorpool" => -0.9,
                "airfield" => std::f64::consts::PI,
                "flakpos" => (draw - 0.5) * 2.0,
                _ => 0.0,
            };
            let angle = math::atan2(
                input.height as f64 * CELL / 2.0 - hq.z,
                input.width as f64 * CELL / 2.0 - hq.x,
            ) + offset;
            let aim = Point {
                x: hq.x + math::cos(angle) * 16.0,
                z: hq.z + math::sin(angle) * 16.0,
            };
            if let Some(at) = site(
                observation,
                terrain,
                aim,
                number(def(kind)?, "size") as usize,
            )? && result.remaining_mp >= cost
            {
                result
                    .commands
                    .push(json!({"t":"build","ids":[u.id],"kind":kind,"x":at.x,"z":at.z}));
                result.remaining_mp -= cost;
            }
        }
    }
    if let Some(kind) = next
        && kind != "depot"
        && !result.rush
    {
        result.reserve = number(def(kind)?, "cost");
    }
    Ok(result)
}

fn site(
    observation: &Observation,
    terrain: &Game,
    aim: Point,
    size: usize,
) -> Result<Option<Point>, String> {
    let input = observation.input();
    let context = input.runtime.as_ref().unwrap();
    let half = size as f64 * CELL / 2.0;
    let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
    let mut avoid: Vec<_> = context.nodes.iter().map(|n| (n.at, 5.0)).collect();
    avoid.extend(
        input
            .points
            .iter()
            .map(|p| (p.at, observation.point_radius() + 2.0)),
    );
    avoid.extend(input.players.iter().map(|p| (p.spawn, 6.0)));
    let mut ground = terrain.clone();
    for u in &input.units {
        if u.hp <= 0.0 {
            continue;
        }
        let d = catalog.unit(&u.kind).map_err(|e| e.to_string())?;
        if flag(d, "structure") {
            avoid.push((u.at, number(d, "radius") + half + 4.0));
        } else if !flag(d, "air")
            && !u.riding
            && let Some(c) = ground.cell_of(u.at.x, u.at.z)
        {
            ground.flags[c] |= MOVE;
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
    .map(|c| foot_center(&ground, c, size)))
}

#[cfg(test)]
#[path = "commander_classic_economy_tests.rs"]
mod tests;
