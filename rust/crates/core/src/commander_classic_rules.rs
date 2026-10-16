//! Adaptive Classic tactics from delivered building memory and sightings.
use crate::{
    catalog::{Catalog, Value as Rule},
    commander_ai::{Observation, Point, UnitObservation},
    commander_mind::Sighting,
    commander_tactics::{Context, KnownBuilding, Skill},
    economy::Mode,
    math,
};
use serde_json::{Value, json};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Raid {
    pub target: u32,
    pub ids: Vec<u32>,
    pub at: Point,
}
#[derive(Default)]
pub(crate) struct Adaptive {
    pub commands: Vec<Value>,
    pub attacks: Vec<(u32, Point)>,
    pub busy: BTreeSet<u32>,
    pub rush: bool,
    pub support: Option<(&'static str, Point)>,
}
fn distance(a: Point, b: Point) -> f64 {
    math::hypot(a.x - b.x, a.z - b.z)
}
fn flag(d: &Rule, key: &str) -> bool {
    d.get(key).and_then(Rule::boolean).unwrap_or(false)
}
fn number(d: &Rule, key: &str) -> f64 {
    d.get(key).and_then(Rule::number).unwrap_or(0.0)
}
/// The delivered rush centroid used by construction eligibility and adaptive defense.
pub(crate) fn rush_at(
    observation: &Observation,
    recent: &[&Sighting],
    skill: Skill,
) -> Result<Option<Point>, String> {
    let input = observation.input();
    if !input
        .runtime
        .as_ref()
        .is_some_and(|c| c.mode.is_construction())
        || !skill.adaptive
    {
        return Ok(None);
    }
    let now = input.tick as f64 / 20.0;
    let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
    let definition = |u: &UnitObservation| catalog.unit(&u.kind).map_err(|e| e.to_string());
    let own: Vec<_> = input
        .units
        .iter()
        .filter(|u| {
            u.owner == input.slot && u.hp > 0.0 && definition(u).is_ok_and(|d| flag(d, "building"))
        })
        .collect();
    let rush: Vec<_> = recent
        .iter()
        .copied()
        .filter(|s| {
            now < 240.0 && now - s.time < 5.0 && own.iter().any(|b| distance(b.at, s.at) < 35.0)
        })
        .collect();
    Ok((!rush.is_empty()).then(|| Point {
        x: rush.iter().map(|s| s.at.x).sum::<f64>() / rush.len() as f64,
        z: rush.iter().map(|s| s.at.z).sum::<f64>() / rush.len() as f64,
    }))
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn plan(
    observation: &Observation,
    context: &Context,
    mine: &[&UnitObservation],
    available: &[&UnitObservation],
    recent: &[&Sighting],
    raid: &mut Option<Raid>,
    skill: Skill,
) -> Result<Adaptive, String> {
    let mut result = Adaptive::default();
    if !context.mode.is_construction() || !skill.adaptive {
        return Ok(result);
    }
    let input = observation.input();
    let me = &input.players[input.slot];
    let now = input.tick as f64 / 20.0;
    let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
    let definition = |u: &UnitObservation| catalog.unit(&u.kind).map_err(|e| e.to_string());
    let rush = rush_at(observation, recent, skill)?;
    result.rush = rush.is_some();
    let destination = if let Some(at) = rush {
        for u in available
            .iter()
            .filter(|u| u.kind == "engineer" && distance(u.at, at) < 25.0)
        {
            result
                .commands
                .push(json!({"t":"move","orders":[[u.id,me.spawn.x,me.spawn.z]]}));
        }
        result.support = Some(("artillery", at));
        Some((mine.to_vec(), at))
    } else {
        let enemy = |b: &&KnownBuilding| input.players[b.owner].team != me.team;
        let known: Vec<_> = context.known_buildings.iter().filter(enemy).collect();
        if !known
            .iter()
            .any(|b| catalog.unit(&b.kind).is_ok_and(|d| flag(d, "produces")))
            && let Some((_, p)) = input
                .players
                .iter()
                .enumerate()
                .filter(|(i, p)| p.team != me.team && !p.out && !context.hidden_spawns.contains(i))
                .min_by(|(ai, a), (bi, b)| {
                    distance(a.spawn, me.spawn)
                        .total_cmp(&distance(b.spawn, me.spawn))
                        .then(ai.cmp(bi))
                })
        {
            result.support = Some(("recon", p.spawn));
        }
        if raid.as_ref().is_some_and(|r| {
            !known.iter().any(|b| b.id == r.target)
                || !r.ids.iter().any(|id| mine.iter().any(|u| u.id == *id))
        }) {
            *raid = None;
        }
        if raid.is_none()
            && let Some(depot) = known
                .iter()
                .filter(|b| {
                    b.kind == "depot"
                        && !recent
                            .iter()
                            .any(|s| now - s.time < 30.0 && distance(s.at, b.at) < 25.0)
                })
                .min_by(|a, b| {
                    distance(a.at, me.spawn)
                        .total_cmp(&distance(b.at, me.spawn))
                        .then(a.id.cmp(&b.id))
                })
        {
            let mut free: Vec<_> = mine
                .iter()
                .copied()
                .filter(|u| {
                    !u.retreating
                        && u.target_id == 0
                        && !matches!(
                            u.kind.as_str(),
                            "mg" | "at" | "mortar" | "sniper" | "howitzer"
                        )
                })
                .collect();
            free.sort_by(|a, b| {
                (a.kind != "armoredcar")
                    .cmp(&(b.kind != "armoredcar"))
                    .then(distance(a.at, depot.at).total_cmp(&distance(b.at, depot.at)))
                    .then(a.id.cmp(&b.id))
            });
            if free.len() >= 2 {
                *raid = Some(Raid {
                    target: depot.id,
                    ids: free[..2].iter().map(|u| u.id).collect(),
                    at: depot.at,
                });
            }
        }
        raid.as_ref().map(|r| {
            (
                mine.iter()
                    .copied()
                    .filter(|u| r.ids.contains(&u.id))
                    .collect(),
                r.at,
            )
        })
    };
    if let Some((units, at)) = destination {
        for u in units {
            if u.retreating {
                continue;
            }
            let d = definition(u)?;
            let full = number(d, "models") * number(d, "hpPer");
            let fraction = u.hp / full;
            let hurt = fraction < skill.retreat
                || (number(d, "models") > 1.0 && (u.hp / number(d, "hpPer")).ceil() <= 1.0)
                || u.supp >= 90.0 && fraction < 0.6;
            if fraction < 0.4 || hurt {
                continue;
            }
            result.busy.insert(u.id);
            if context
                .units
                .get(&u.id)
                .and_then(|f| f.amove)
                .is_none_or(|goal| distance(goal, at) > 6.0)
            {
                result.attacks.push((u.id, at));
            }
        }
    }
    Ok(result)
}

pub(crate) fn targets(
    observation: &Observation,
    context: &Context,
) -> Result<Vec<(Option<usize>, u32, Point)>, String> {
    let input = observation.input();
    let me = &input.players[input.slot];
    let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
    let mut targets = if matches!(
        context.mode,
        Mode::Assault | Mode::Annihilation | Mode::Horde
    ) {
        input
            .units
            .iter()
            .filter(|u| {
                u.hp > 0.0
                    && input.players[u.owner].team != me.team
                    && catalog
                        .unit(&u.kind)
                        .is_ok_and(|d| flag(d, "structure") && !flag(d, "building"))
            })
            .map(|u| (Some(u.owner), u.id, u.at))
            .collect::<Vec<_>>()
    } else if context.mode.is_construction() {
        context
            .known_buildings
            .iter()
            .filter(|b| {
                input.players[b.owner].team != me.team
                    && catalog.unit(&b.kind).is_ok_and(|d| flag(d, "produces"))
            })
            .map(|b| (Some(b.owner), b.id, b.at))
            .collect()
    } else {
        vec![]
    };
    if targets.is_empty() && context.mode == Mode::Classic {
        targets.extend(
            input
                .players
                .iter()
                .enumerate()
                .filter(|(i, p)| p.team != me.team && !p.out && !context.hidden_spawns.contains(i))
                .map(|(i, p)| (None, i as u32, p.spawn)),
        );
    }
    targets.sort_by(|a, b| {
        distance(a.2, me.spawn)
            .total_cmp(&distance(b.2, me.spawn))
            .then(a.1.cmp(&b.1))
    });
    Ok(targets)
}
