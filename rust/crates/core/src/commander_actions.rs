//! Ground abilities, house shelling and focus fire from delivered seat facts.
use crate::{
    catalog::{Catalog, Value as Rule},
    commander_ai::{Observation, Point, UnitObservation},
    commander_mirror::Mirror,
    commander_tactics::{Context, Skill},
    economy::{self, AbilityCost, Mode},
    math,
    rules::Rules,
};
use serde_json::{Value, json};
use std::collections::BTreeSet;

pub(crate) struct Plan {
    pub commands: Vec<Value>,
    pub busy: BTreeSet<u32>,
}

fn number(rule: &Rule, key: &str) -> f64 {
    rule.get(key).and_then(Rule::number).unwrap_or(0.0)
}
fn flag(rule: &Rule, key: &str) -> bool {
    rule.get(key).and_then(Rule::boolean).unwrap_or(false)
}
fn distance(a: Point, b: Point) -> f64 {
    math::hypot(a.x - b.x, a.z - b.z)
}
fn local_unit(mirror: &Mirror, id: u32) -> &UnitObservation {
    let units = &mirror.input().units;
    &units[units.binary_search_by_key(&id, |unit| unit.id).unwrap()]
}

fn health(rule: &Rule) -> f64 {
    number(rule, "models") * number(rule, "hpPer")
}
fn hurt(unit: &UnitObservation, rule: &Rule, skill: Skill) -> bool {
    let fraction = unit.hp / health(rule);
    fraction < skill.retreat
        || (number(rule, "models") > 1.0 && math::ceil(unit.hp / number(rule, "hpPer")) <= 1.0)
        || (unit.supp >= 90.0 && fraction < 0.6)
}
fn at_base(
    observation: &Observation,
    context: &Context,
    unit: &UnitObservation,
    catalog: &Catalog,
) -> bool {
    let input = observation.input();
    let radius = number(catalog.table("CFG").unwrap(), "reinforceRadius");
    if context.mode.is_construction() || context.mode.is_skirmish() {
        input.units.iter().any(|base| {
            input.players[base.owner].team == input.players[input.slot].team
                && base.hp > 0.0
                && context.units.get(&base.id).is_some_and(|f| f.built >= 1.0)
                && if context.mode.is_construction() {
                    flag(catalog.unit(&base.kind).unwrap(), "produces")
                } else {
                    base.owner == input.slot && base.kind == "hq"
                }
                && distance(base.at, unit.at) <= radius
        })
    } else {
        distance(input.players[input.slot].spawn, unit.at) <= radius
    }
}

/// Watched IDs express notice delay, never visibility. Targets must also be delivered now.
/// World rail, boarding and unloading belong to the deferred World planner.
pub(crate) fn plan(
    observation: &Observation,
    watched: &BTreeSet<u32>,
    skill: Skill,
    unavailable: &BTreeSet<u32>,
) -> Result<Plan, String> {
    let input = observation.input();
    let context = input
        .runtime
        .as_ref()
        .ok_or("missing ground action context")?;
    let mut result = Plan {
        commands: vec![],
        busy: BTreeSet::new(),
    };
    if context.mode == Mode::World || input.players[input.slot].out {
        return Ok(result);
    }
    let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
    let definition = |unit: &UnitObservation| catalog.unit(&unit.kind).unwrap();
    let team = input.players[input.slot].team;
    let enemies: Vec<_> = input
        .units
        .iter()
        .filter(|u| input.players[u.owner].team != team && watched.contains(&u.id) && u.hp > 0.0)
        .collect();
    let mine: Vec<_> = input
        .units
        .iter()
        .filter(|u| {
            let d = definition(u);
            u.owner == input.slot
                && u.hp > 0.0
                && !flag(d, "structure")
                && !flag(d, "air")
                && !flag(d, "logisticsTruck")
                && !u.retreating
                && !u.riding
                && !unavailable.contains(&u.id)
        })
        .collect();
    let clear = |at: Point| {
        !input.units.iter().any(|u| {
            input.players[u.owner].team == team
                && !flag(definition(u), "air")
                && distance(at, u.at) < 10.0
        })
    };
    let mut mirror = Mirror::new(input);
    let mut casts = 0;
    for &unit in &mine {
        let d = definition(unit);
        let Some(ability) = d.get("ab") else { continue };
        let id = ability.get("id").and_then(Rule::string).unwrap_or("none");
        let cooldown = context.units.get(&unit.id).map_or(0.0, |f| f.cooldown);
        let home = at_base(observation, context, unit, catalog);
        let target = enemies.iter().copied().find(|e| e.id == unit.target_id);
        if cooldown <= 0.0 && casts < skill.casts && !unit.nade {
            let near = |radius: f64| {
                enemies
                    .iter()
                    .copied()
                    .filter(move |e| distance(unit.at, e.at) <= radius)
            };
            let mut aim = None;
            let cast = match id {
                "grenade" => {
                    aim = near(number(ability, "range") + 4.0)
                        .find(|e| {
                            flag(definition(e), "infantry")
                                && (e.kind != "rifle" || matches!(e.cover, 1 | 2))
                        })
                        .map(|e| e.at);
                    aim.is_some()
                }
                "suppress" => target.is_some(),
                "ap" => target.is_some_and(|e| {
                    !flag(definition(e), "infantry") && !flag(definition(e), "structure")
                }),
                "smoke" => unit.hp / health(d) < 0.5 && !home,
                "satchel" => {
                    aim = near(20.0)
                        .find(|e| e.garrison || !flag(definition(e), "infantry"))
                        .map(|e| e.at);
                    aim.is_some()
                }
                "ura" => {
                    unit.supp >= 50.0
                        || (context
                            .units
                            .get(&unit.id)
                            .is_some_and(|f| f.amove.is_some())
                            && near(30.0).any(|e| distance(unit.at, e.at) < 30.0))
                }
                "barrage" => {
                    if skill.best {
                        let mut most = skill.cluster.saturating_sub(1);
                        for center in &enemies {
                            let group: Vec<_> = near(number(ability, "range"))
                                .filter(|e| distance(center.at, e.at) <= 8.0)
                                .collect();
                            if group.len() <= most {
                                continue;
                            }
                            let at = Point {
                                x: group.iter().map(|e| e.at.x).sum::<f64>() / group.len() as f64,
                                z: group.iter().map(|e| e.at.z).sum::<f64>() / group.len() as f64,
                            };
                            if clear(at) {
                                aim = Some(at);
                                most = group.len();
                            }
                        }
                    }
                    if aim.is_none() {
                        aim = near(number(ability, "range"))
                            .find(|e| e.garrison && (!skill.best || clear(e.at)))
                            .map(|e| e.at);
                    }
                    aim.is_some()
                }
                _ => false,
            };
            let cost = economy::ab_cost(
                context.mode,
                AbilityCost {
                    id,
                    munitions: ability.get("mun").and_then(Rule::number),
                },
            )
            .map_err(|e| e.to_string())?;
            if cast
                && (aim.is_none() || context.ground_flights)
                && mirror.input().runtime.as_ref().unwrap().munitions >= cost
            {
                let command = if let Some(at) = aim {
                    json!({"t":"ability", "ids":[unit.id], "x":at.x, "z":at.z})
                } else {
                    json!({"t":"ability", "ids":[unit.id]})
                };
                mirror.apply(&command, true)?;
                result.commands.push(command);
                casts += 1;
            }
        }
        // Retreat and reinforcement take precedence over house shelling.
        let local = local_unit(&mirror, unit.id);
        if !context.horde
            && ((!home && hurt(local, d, skill))
                || (home
                    && unit.hp < health(d)
                    && (input.manpower >= 20.0 || hurt(local, d, skill))))
        {
            continue;
        }
        if context.ground_flights
            && d.get("w").is_some_and(|w| number(w, "shellTerrain") > 0.0)
            && unit.target_id == 0
            && !unit.fire_at
            && let Some(house) = enemies
                .iter()
                .find(|e| e.garrison && distance(unit.at, e.at) < 60.0)
        {
            result
                .commands
                .push(json!({"t":"fireat", "ids":[unit.id], "x":house.at.x, "z":house.at.z}));
            result.busy.insert(unit.id);
        }
    }
    if !skill.focus {
        return Ok(result);
    }
    let rules = Rules::from_catalog(catalog).map_err(|e| e.to_string())?;
    let terrain = crate::commander_tactics::sight_game(observation, context);
    let los = |a: Point, b: Point| crate::vision::los(&terrain, &rules, (a.x, a.z), (b.x, b.z));
    let mut stop = Vec::new();
    for &unit in &mine {
        let Some(facts) = context.units.get(&unit.id) else {
            continue;
        };
        let local = local_unit(&mirror, unit.id);
        if facts.attack_target == 0
            || (!context.horde
                && !at_base(observation, context, unit, catalog)
                && hurt(local, definition(unit), skill))
        {
            continue;
        }
        // A hidden target provides no fresh reason to cancel a delivered order.
        let Some(enemy) = input
            .units
            .iter()
            .find(|e| e.id == facts.attack_target && input.players[e.owner].team != team)
        else {
            continue;
        };
        let range = definition(unit)
            .get("w")
            .map_or(0.0, |w| number(w, "range"));
        if enemy.retreating || distance(unit.at, enemy.at) > range || !los(unit.at, enemy.at) {
            let resume = facts.queued_amove.unwrap_or(unit.at);
            stop.push(json!([unit.id, resume.x, resume.z]));
            result.busy.insert(unit.id);
        }
    }
    if !stop.is_empty() {
        result.commands.push(json!({"t":"amove", "orders":stop}));
    }
    let shooters: Vec<_> = mine
        .into_iter()
        .filter(|u| {
            let local = local_unit(&mirror, u.id);
            u.target_id != 0
                && !u.garrison
                && !local.nade
                && !u.dig
                && !u.fire_at
                && !result.busy.contains(&u.id)
                && !definition(u).get("w").is_some_and(|w| flag(w, "salvo"))
                && (context.horde
                    || at_base(observation, context, u, catalog)
                    || !hurt(local, definition(u), skill))
        })
        .collect();
    let hit = |u: &UnitObservation, infantry: bool| {
        let w = definition(u).get("w").unwrap();
        if infantry {
            number(w, "inf") * number(w, "accInf")
        } else {
            number(w, "veh") * number(w, "accVeh")
        }
    };
    let mut used = BTreeSet::new();
    // The ordinary seat policy is mindful, so one focus group is chosen per look.
    let mut best = None;
    let mut best_score = 0.0;
    let mut group = Vec::new();
    for &enemy in &enemies {
        let d = definition(enemy);
        if flag(d, "air") || flag(d, "structure") || enemy.retreating {
            continue;
        }
        let infantry = flag(d, "infantry");
        let ids: Vec<_> = shooters
            .iter()
            .copied()
            .filter(|u| {
                let w = definition(u).get("w").unwrap();
                distance(u.at, enemy.at) <= number(w, "range")
                    && hit(u, infantry) >= 0.5 * math::max(hit(u, true), hit(u, false))
                    && los(u.at, enemy.at)
            })
            .collect();
        if ids.len() < 2 {
            continue;
        }
        let dps: f64 = ids
            .iter()
            .map(|u| {
                let own = definition(u);
                let w = own.get("w").unwrap();
                let models = if flag(w, "perModel") {
                    math::min(
                        math::ceil(u.hp / number(own, "hpPer")),
                        number(own, "models"),
                    )
                } else {
                    1.0
                };
                hit(u, infantry) * models / number(w, "interval")
            })
            .sum();
        let score = dps * number(d, "cost") / health(d) * (2.0 - enemy.hp / health(d));
        if score > best_score {
            best = Some(enemy);
            best_score = score;
            group = ids;
        }
    }
    if let Some(enemy) = best {
        let go: Vec<_> = group
            .into_iter()
            .filter(|u| {
                u.target_id != enemy.id
                    && context
                        .units
                        .get(&u.id)
                        .is_none_or(|f| f.attack_target != enemy.id)
            })
            .collect();
        if !go.is_empty() {
            let resume: Vec<_> = go
                .iter()
                .filter_map(|u| {
                    context
                        .units
                        .get(&u.id)
                        .and_then(|f| f.amove.or(f.queued_amove))
                        .map(|at| json!([u.id, at.x, at.z]))
                })
                .collect();
            used.extend(go.iter().map(|u| u.id));
            result
                .commands
                .push(json!({"t":"attack", "ids":used, "target":enemy.id}));
            if !resume.is_empty() {
                result
                    .commands
                    .push(json!({"t":"amove", "orders":resume, "queue":true}));
            }
        }
    }
    result.busy.extend(used);
    Ok(result)
}

#[cfg(test)]
#[path = "commander_actions_tests.rs"]
mod tests;
