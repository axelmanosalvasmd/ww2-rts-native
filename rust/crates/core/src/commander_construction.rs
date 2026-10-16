//! Construction proposals from delivered facts. The caller performs canonical admission.
use crate::{
    catalog::{Catalog, Value as Rule},
    commander_ai::{Observation, Point, UnitObservation},
    economy::{self, Mode},
    game::{CELL, Game, LAND, MOVE},
    match_runtime::{find_site, foot_center, footprint},
    math,
};
use serde_json::{Value, json};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Plan {
    pub commands: Vec<Value>,
    pub remaining_mp: f64,
    pub reserve: f64,
    /// Units receiving construction, assist or node travel orders this look.
    pub builders: Vec<u32>,
}

fn distance(a: Point, b: Point) -> f64 {
    math::hypot(a.x - b.x, a.z - b.z)
}
fn flag(rule: &Rule, key: &str) -> bool {
    rule.get(key).and_then(Rule::boolean).unwrap_or(false)
}
fn number(rule: &Rule, key: &str) -> Result<f64, String> {
    rule.get(key)
        .and_then(Rule::number)
        .ok_or_else(|| format!("missing construction rule {key}"))
}

fn unclaimed_nodes(observation: &Observation) -> Vec<Point> {
    let input = observation.input();
    let Some(context) = &input.runtime else {
        return Vec::new();
    };
    context
        .nodes
        .iter()
        .copied()
        .filter(|&node| {
            !input
                .units
                .iter()
                .any(|u| u.kind == "depot" && u.hp > 0.0 && distance(u.at, node) < 8.0)
                && !context
                    .known_depots
                    .iter()
                    .any(|&at| distance(at, node) < 8.0)
        })
        .collect()
}

/// Unclaimed delivered nodes, without World territory filtering: no regions are supplied.
/// `known_depots` contains delivered known enemy depot positions, never hidden live units.
pub(crate) fn free_nodes(observation: &Observation) -> usize {
    unclaimed_nodes(observation).len()
}

fn tech_build(
    observation: &Observation,
    mut next: Option<&'static str>,
    has: impl Fn(&str, bool) -> bool,
    instead: Option<&'static str>,
) -> Result<Option<&'static str>, String> {
    let Some(research) = &observation.input().research else {
        return Ok(next);
    };
    let tier = research.tech.tier();
    if let Some(kind) = next
        && economy::tier_of(kind).map_err(|e| e.to_string())? > tier
    {
        next = instead;
    }
    Ok(next.or_else(|| (tier >= 2 && !has("armory", false)).then_some("armory")))
}

fn site_near(
    observation: &Observation,
    catalog: &Catalog,
    ground: &Game,
    aim: Point,
    size: usize,
    planned: &[(Point, f64)],
) -> Result<Option<usize>, String> {
    let input = observation.input();
    let context = input
        .runtime
        .as_ref()
        .ok_or("missing construction context")?;
    let half = size as f64 * CELL / 2.0;
    let mut avoid: Vec<_> = context.nodes.iter().copied().map(|n| (n, 5.0)).collect();
    avoid.extend(
        input
            .points
            .iter()
            .map(|p| (p.at, observation.point_radius() + 2.0)),
    );
    avoid.extend(input.players.iter().map(|p| (p.spawn, 6.0)));
    for u in &input.units {
        if u.hp <= 0.0 {
            continue;
        }
        let rule = catalog.unit(&u.kind).map_err(|e| e.to_string())?;
        if flag(rule, "structure") {
            avoid.push((u.at, number(rule, "radius")? + half + 4.0));
        }
    }
    avoid.extend(
        planned
            .iter()
            .map(|&(at, radius)| (at, radius + half + 4.0)),
    );
    Ok(find_site(
        ground,
        Point {
            x: aim.x - half,
            z: aim.z - half,
        },
        size,
        &avoid,
    ))
}

fn coast(ground: &Game, cells: &[usize]) -> bool {
    cells.iter().any(|&cell| {
        (-2..=2).any(|dz| {
            (-2..=2).any(|dx| {
                let x = (cell % ground.w) as isize + dx;
                let z = (cell / ground.w) as isize + dz;
                x >= 0
                    && z >= 0
                    && (x as usize) < ground.w
                    && (z as usize) < ground.h
                    && ground.flags[z as usize * ground.w + x as usize] & LAND == 0
            })
        })
    })
}

pub(crate) fn plan(
    observation: &Observation,
    available: &[&UnitObservation],
    budget: f64,
    need_armor: bool,
    terrain: &Game,
    sees: impl Fn(Point) -> bool,
) -> Result<Plan, String> {
    if !budget.is_finite() {
        return Err("non-finite construction budget".into());
    }
    let input = observation.input();
    let context = input
        .runtime
        .as_ref()
        .ok_or("missing construction context")?;
    let mut plan = Plan {
        commands: Vec::new(),
        remaining_mp: budget,
        reserve: 0.0,
        builders: Vec::new(),
    };
    let classic = context.mode.is_construction();
    let skirmish = context.mode.is_skirmish() && !context.horde;
    if !classic && !skirmish {
        return Ok(plan);
    }
    let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
    let rule = |kind: &str| catalog.unit(kind).map_err(|e| e.to_string());
    let team = input.players[input.slot].team;
    let built = |u: &UnitObservation| context.units.get(&u.id).map_or(0.0, |f| f.built);
    let mut own = Vec::new();
    for u in &input.units {
        if u.hp > 0.0
            && (u.owner == input.slot
                || skirmish && context.mode == Mode::Horde && input.players[u.owner].team == team)
            && flag(rule(&u.kind)?, "building")
        {
            own.push(u);
        }
    }
    let has = |kind: &str, done: bool| {
        own.iter()
            .any(|u| u.kind == kind && (!done || built(u) >= 1.0))
    };
    let home = own
        .iter()
        .find(|u| u.kind == "hq")
        .map_or(input.players[input.slot].spawn, |u| u.at);
    let free: Vec<_> = unclaimed_nodes(observation)
        .into_iter()
        .filter(|at| {
            context.mode != Mode::World
                || context
                    .nodes
                    .iter()
                    .enumerate()
                    .any(|(i, p)| p == at && context.owned_nodes.contains(&i))
        })
        .collect();
    let depots = own.iter().filter(|u| u.kind == "depot").count();
    let mut next = if skirmish {
        if !has("hq", false) {
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
        }
    } else {
        let mut planes_near = false;
        for u in &input.units {
            if u.hp > 0.0
                && input.players[u.owner].team != team
                && distance(u.at, home) < 60.0
                && flag(rule(&u.kind)?, "air")
            {
                planes_near = true;
            }
        }
        if context.mode == Mode::World && !has("hq", false) {
            Some("hq")
        } else if depots < 2
            && !free.is_empty()
            && !(need_armor && has("barracks", true) && !has("motorpool", false))
        {
            Some("depot")
        } else if !has("barracks", false) {
            Some("barracks")
        } else if has("barracks", true) && !has("motorpool", false) {
            Some("motorpool")
        } else if planes_near && own.iter().filter(|u| u.kind == "flakpos").count() < 2 {
            Some("flakpos")
        } else if !free.is_empty() {
            Some("depot")
        } else if has("motorpool", true) && !has("airfield", false) && depots >= 3 {
            Some("airfield")
        } else {
            None
        }
    };
    next = tech_build(
        observation,
        next,
        has,
        (classic && !free.is_empty()).then_some("depot"),
    )?;
    let cost = next
        .map(|k| number(rule(k)?, "cost"))
        .transpose()?
        .unwrap_or(0.0);
    plan.reserve = if classic && next == Some("depot") {
        0.0
    } else {
        cost
    };
    // Construction-mode actions require their separate mode providers. Shopping still
    // receives the next non-depot cost from the delivered construction decision inputs.
    if classic {
        return Ok(plan);
    }
    let builders = catalog
        .table("CFG")
        .map_err(|e| e.to_string())?
        .get("fortBuilders")
        .and_then(Rule::array)
        .ok_or("missing construction builders")?;
    let worker = available
        .iter()
        .copied()
        .filter(|u| {
            u.owner == input.slot
                && u.hp > 0.0
                && !u.retreating
                && !u.riding
                && !u.build
                && u.target_id == 0
                && !u.dig
                && !u.entrench
                && !u.garrison
                && builders.iter().any(|k| k.string() == Some(u.kind.as_str()))
        })
        .min_by(|a, b| {
            distance(a.at, input.players[input.slot].spawn)
                .total_cmp(&distance(b.at, input.players[input.slot].spawn))
                .then(a.id.cmp(&b.id))
        });
    let Some(worker) = worker else {
        return Ok(plan);
    };
    if available.iter().any(|u| u.build) {
        return Ok(plan);
    }
    let mut repairs = Vec::new();
    for &u in &own {
        if built(u) < 1.0 || u.hp < number(rule(&u.kind)?, "hpPer")? * 0.8 {
            repairs.push(u);
        }
    }
    if let Some(fix) = repairs.into_iter().min_by(|a, b| {
        (built(a) >= 1.0)
            .cmp(&(built(b) >= 1.0))
            .then(distance(worker.at, a.at).total_cmp(&distance(worker.at, b.at)))
            .then(a.id.cmp(&b.id))
    }) {
        plan.commands
            .push(json!({"t":"assist","ids":[worker.id],"id":fix.id}));
        plan.builders.push(worker.id);
        return Ok(plan);
    }
    let Some(kind) = next else { return Ok(plan) };
    if budget < cost {
        return Ok(plan);
    }
    let definition = rule(kind)?;
    let size = number(definition, "size")? as usize;
    let mut ground = terrain.clone();
    for u in &input.units {
        if u.hp > 0.0
            && !flag(rule(&u.kind)?, "structure")
            && !flag(rule(&u.kind)?, "air")
            && !u.riding
            && let Some(cell) = ground.cell_of(u.at.x, u.at.z)
        {
            ground.flags[cell] |= MOVE;
        }
    }
    for radius in [12.0, 20.0, 30.0, 42.0] {
        for index in 0..8 {
            let angle = index as f64 * std::f64::consts::FRAC_PI_4;
            let aim = Point {
                x: home.x + math::cos(angle) * radius,
                z: home.z + math::sin(angle) * radius,
            };
            let Some(cell) = site_near(observation, catalog, &ground, aim, size, &[])? else {
                continue;
            };
            let at = foot_center(&ground, cell, size);
            if !sees(at)
                || flag(definition, "coast")
                    && !(context.naval
                        && footprint(&ground, cell, size)
                            .is_some_and(|cells| coast(&ground, &cells)))
            {
                continue;
            }
            plan.commands
                .push(json!({"t":"build","ids":[worker.id],"kind":kind,"x":at.x,"z":at.z}));
            plan.builders.push(worker.id);
            plan.remaining_mp -= cost;
            plan.reserve = 0.0;
            return Ok(plan);
        }
    }
    Ok(plan)
}
