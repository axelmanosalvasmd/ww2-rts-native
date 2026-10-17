//! Detached commander ability choices from delivered own facts and watched enemies.
use std::collections::{BTreeMap, BTreeSet};

use crate::{
    catalog::{Catalog, Value},
    commander_ai::{Level, Observation},
    game::Point,
    match_commands::Command,
    math,
};

fn distance(a: Point, b: Point) -> f64 {
    math::hypot(a.x - b.x, a.z - b.z)
}
fn flag(definition: &Value, key: &str) -> bool {
    definition
        .get(key)
        .and_then(Value::boolean)
        .unwrap_or(false)
}
fn number(definition: &Value, key: &str) -> Result<f64, String> {
    definition
        .get(key)
        .and_then(Value::number)
        .filter(|n| n.is_finite())
        .ok_or_else(|| format!("missing commander ability rule {key}"))
}

/// Own fields are delivered facts; watched IDs authorize only packet enemies.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AbilityFacts {
    pub cooldowns: BTreeMap<u32, f64>,
    pub watched: BTreeSet<u32>,
    pub attack_moving: BTreeSet<u32>,
    pub at_base: BTreeSet<u32>,
    pub unavailable: BTreeSet<u32>,
}

/// Returned commands still need canonical admission. `unavailable` represents
/// earlier strategic assignments and logistics work, never an enemy private field.
pub fn plan(
    observation: &Observation,
    facts: &AbilityFacts,
    level: Level,
) -> Result<Vec<Command>, String> {
    let AbilityFacts {
        cooldowns: own_cooldowns,
        watched,
        attack_moving,
        at_base,
        unavailable,
    } = facts;
    let input = observation.input();
    let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
    let definitions = input
        .units
        .iter()
        .map(|u| catalog.unit(&u.kind).map_err(|e| e.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    let own: BTreeSet<_> = input
        .units
        .iter()
        .zip(&definitions)
        .filter(|(u, def)| u.owner == input.slot && !flag(def, "structure"))
        .map(|(u, _)| u.id)
        .collect();
    if own_cooldowns.keys().copied().collect::<BTreeSet<_>>() != own
        || own_cooldowns.values().any(|v| !v.is_finite())
        || !attack_moving.is_subset(&own)
        || !at_base.is_subset(&own)
        || !unavailable.is_subset(&own)
    {
        return Err("commander ability facts do not match delivered own units".into());
    }
    let team = input.players[input.slot].team;
    let enemies: Vec<_> = input
        .units
        .iter()
        .zip(&definitions)
        .filter(|(u, _)| input.players[u.owner].team != team)
        .collect();
    let enemy_ids = enemies.iter().map(|(u, _)| u.id).collect::<BTreeSet<_>>();
    if !watched.is_subset(&enemy_ids) {
        return Err("commander ability watch includes an undelivered enemy".into());
    }
    if input.players[input.slot].out {
        return Ok(Vec::new());
    }
    let clear = |at: Point| {
        input.units.iter().zip(&definitions).all(|(u, def)| {
            input.players[u.owner].team != team || flag(def, "air") || distance(u.at, at) >= 10.0
        })
    };
    let cap = match level {
        Level::Easy => 1,
        Level::Normal => 2,
        Level::Hard => 3,
    };
    let mut commands = Vec::new();
    for (u, def) in input.units.iter().zip(&definitions) {
        if commands.len() == cap {
            break;
        }
        if u.owner != input.slot
            || flag(def, "structure")
            || flag(def, "air")
            || flag(def, "rail")
            || flag(def, "logisticsTruck")
            || u.kind == "engineer"
            || u.hp <= 0.0
            || u.build
            || u.riding
            || u.retreating
            || unavailable.contains(&u.id)
            || own_cooldowns.get(&u.id).is_none_or(|cd| *cd > 0.0)
        {
            continue;
        }
        let ability = def
            .get("ab")
            .ok_or("missing commander ability definition")?;
        let kind = ability
            .get("id")
            .and_then(Value::string)
            .ok_or("missing commander ability identity")?;
        let target = enemies
            .iter()
            .find(|(e, _)| e.id == u.target_id && watched.contains(&e.id));
        let mut at = None;
        let cast = match kind {
            "grenade" => {
                let range = number(ability, "range")? + 4.0;
                at = enemies
                    .iter()
                    .find(|(e, ed)| {
                        watched.contains(&e.id)
                            && flag(ed, "infantry")
                            && distance(u.at, e.at) <= range
                            && (e.kind != "rifle" || matches!(e.cover, 1 | 2))
                    })
                    .map(|(e, _)| e.at);
                at.is_some()
            }
            "suppress" => target.is_some(),
            "ap" => target.is_some_and(|(_, ed)| !flag(ed, "infantry") && !flag(ed, "structure")),
            "smoke" => {
                u.hp < number(def, "models")? * number(def, "hpPer")? * 0.5
                    && !at_base.contains(&u.id)
            }
            "satchel" => {
                at = enemies
                    .iter()
                    .find(|(e, ed)| {
                        watched.contains(&e.id)
                            && (e.garrison || !flag(ed, "infantry"))
                            && distance(u.at, e.at) <= 20.0
                    })
                    .map(|(e, _)| e.at);
                at.is_some()
            }
            "ura" => {
                u.supp >= 50.0
                    || (attack_moving.contains(&u.id)
                        && enemies
                            .iter()
                            .any(|(e, _)| watched.contains(&e.id) && distance(u.at, e.at) < 30.0))
            }
            "barrage" => {
                let range = number(ability, "range")?;
                if level == Level::Hard {
                    let mut most = 2;
                    for (anchor, _) in &enemies {
                        if !watched.contains(&anchor.id) {
                            continue;
                        }
                        let near: Vec<_> = enemies
                            .iter()
                            .filter(|(e, _)| {
                                watched.contains(&e.id)
                                    && distance(e.at, anchor.at) <= 8.0
                                    && distance(u.at, e.at) <= range
                            })
                            .collect();
                        if near.len() <= most {
                            continue;
                        }
                        let centre = Point {
                            x: near.iter().map(|(e, _)| e.at.x).sum::<f64>() / near.len() as f64,
                            z: near.iter().map(|(e, _)| e.at.z).sum::<f64>() / near.len() as f64,
                        };
                        if clear(centre) {
                            most = near.len();
                            at = Some(centre);
                        }
                    }
                }
                if at.is_none() {
                    at = enemies
                        .iter()
                        .find(|(e, _)| {
                            watched.contains(&e.id)
                                && e.garrison
                                && distance(u.at, e.at) <= range
                                && (level != Level::Hard || clear(e.at))
                        })
                        .map(|(e, _)| e.at);
                }
                at.is_some()
            }
            _ => false,
        };
        if cast {
            commands.push(Command::Ability {
                ids: vec![i64::from(u.id)],
                at,
                queue: false,
                automatic: false,
            });
        }
    }
    Ok(commands)
}

#[cfg(test)]
#[path = "commander_abilities_tests.rs"]
mod tests;
