//! Off-map support choices from detached recipient facts. Admission remains canonical.
use std::collections::{BTreeMap, BTreeSet};

use crate::{
    catalog::{Catalog, Value},
    commander_ai::{Level, Observation},
    economy::{self, Currency, Mode},
    game::Point,
    match_commands::Command,
    math,
};

#[derive(Clone, Debug, PartialEq)]
pub struct AnnouncedStrike {
    pub kind: String,
    pub owner: usize,
    pub at: Point,
    pub delay: f64,
}
#[derive(Clone, Debug, PartialEq)]
pub struct SupportFacts {
    pub mode: Mode,
    pub munitions: f64,
    pub cooldowns: BTreeMap<String, f64>,
    pub watched: BTreeSet<u32>,
    /// First unchanged position in the observer's continuous delivered sightings.
    pub stationary_since: BTreeMap<u32, f64>,
    pub visible_points: BTreeSet<usize>,
    pub ready_points: BTreeSet<usize>,
    pub strikes: Vec<AnnouncedStrike>,
    pub situation: Option<Point>,
    pub horde_seat: bool,
    /// Owned actors already withdrawing or waiting for logistics recovery.
    pub unavailable: BTreeSet<u32>,
}
fn distance(a: Point, b: Point) -> f64 {
    math::hypot(a.x - b.x, a.z - b.z)
}
fn flag(def: &Value, key: &str) -> bool {
    def.get(key).and_then(Value::boolean).unwrap_or(false)
}
fn finite(at: Point) -> bool {
    at.x.is_finite() && at.z.is_finite()
}
fn support(kind: &str, at: Point) -> Command {
    Command::Support {
        kind: kind.into(),
        at,
        direction: None,
    }
}

/// Emit at most one attempt. The caller commits its private call clock separately.
pub fn plan(
    observation: &Observation,
    facts: &SupportFacts,
    level: Level,
    last_called: f64,
) -> Result<Option<Command>, String> {
    let input = observation.input();
    let now = input.tick as f64 / 20.0;
    let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
    let kinds = catalog
        .table("SUPPORT")
        .map_err(|e| e.to_string())?
        .object()
        .ok_or("invalid support rules")?;
    if facts.cooldowns.keys().collect::<BTreeSet<_>>() != kinds.keys().collect::<BTreeSet<_>>()
        || facts.cooldowns.values().any(|v| !v.is_finite())
        || !facts.munitions.is_finite()
        || !last_called.is_finite()
        || facts.situation.is_some_and(|at| !finite(at))
        || facts
            .visible_points
            .iter()
            .chain(&facts.ready_points)
            .any(|i| *i >= input.points.len())
        || facts.strikes.iter().any(|s| {
            !kinds.contains_key(&s.kind)
                || s.owner >= input.players.len()
                || !finite(s.at)
                || !s.delay.is_finite()
        })
        || (facts.horde_seat && facts.mode != Mode::Horde)
    {
        return Err("invalid detached commander support facts".into());
    }
    let team = input.players[input.slot].team;
    let definitions = input
        .units
        .iter()
        .map(|u| catalog.unit(&u.kind).map_err(|e| e.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    let enemies: Vec<_> = input
        .units
        .iter()
        .zip(&definitions)
        .filter(|(u, _)| input.players[u.owner].team != team)
        .collect();
    let enemy_ids: BTreeSet<_> = enemies.iter().map(|(u, _)| u.id).collect();
    let own_ids: BTreeSet<_> = input
        .units
        .iter()
        .filter(|u| u.owner == input.slot)
        .map(|u| u.id)
        .collect();
    if !facts.watched.is_subset(&enemy_ids)
        || !facts.unavailable.is_subset(&own_ids)
        || facts
            .stationary_since
            .iter()
            .any(|(id, since)| !enemy_ids.contains(id) || !since.is_finite() || *since > now)
    {
        return Err(
            "commander support watch includes an undelivered enemy or invalid clock".into(),
        );
    }
    if input.players[input.slot].out {
        return Ok(None);
    }
    let watched = |id: u32| facts.horde_seat || facts.watched.contains(&id);
    let can = |kind: &str| -> Result<bool, String> {
        if !crate::recipient_snapshot::SUPPORTED_SUPPORT_KINDS
            .iter()
            .any(|supported| supported.name() == kind)
        {
            return Ok(false);
        }
        let price = economy::sup_cost(facts.mode, kind).map_err(|e| e.to_string())?;
        let (wallet, reserve) = match price.currency {
            Currency::Manpower => (
                input.manpower,
                if level == Level::Easy { 250.0 } else { 100.0 },
            ),
            Currency::Munitions => (
                facts.munitions,
                if level == Level::Easy { 40.0 } else { 0.0 },
            ),
        };
        Ok(facts.cooldowns[kind] <= 0.0
            && wallet >= price.cost + reserve
            && now - last_called >= if level == Level::Easy { 45.0 } else { 0.0 })
    };
    let in_situation =
        |at: Point| facts.horde_seat || facts.situation.is_none_or(|sit| distance(at, sit) <= 40.0);
    let clear = |at: Point| {
        input.units.iter().zip(&definitions).all(|(u, def)| {
            input.players[u.owner].team != team || flag(def, "air") || distance(u.at, at) >= 10.0
        })
    };
    let cluster = |radius: f64, infantry_only: bool| {
        let mut best = None;
        let mut most = if level == Level::Hard { 2 } else { 1 };
        for (anchor, _) in &enemies {
            if !watched(anchor.id) {
                continue;
            }
            let near: Vec<_> = enemies
                .iter()
                .filter(|(u, def)| {
                    watched(u.id)
                        && distance(u.at, anchor.at) <= radius
                        && (!infantry_only || flag(def, "infantry"))
                })
                .collect();
            if near.len() <= most {
                continue;
            }
            let centre = Point {
                x: near.iter().map(|(u, _)| u.at.x).sum::<f64>() / near.len() as f64,
                z: near.iter().map(|(u, _)| u.at.z).sum::<f64>() / near.len() as f64,
            };
            if level != Level::Hard {
                return Some(centre);
            }
            if clear(centre) {
                best = Some(centre);
                most = near.len();
            }
        }
        best
    };
    if can("cover")? {
        let incoming = facts.strikes.iter().find(|s| {
            s.delay > 0.0
                && input.players[s.owner].team != team
                && matches!(s.kind.as_str(), "strafe" | "bombing" | "dive" | "para")
                && input.units.iter().zip(&definitions).any(|(u, def)| {
                    u.owner == input.slot
                        && !facts.unavailable.contains(&u.id)
                        && !flag(def, "structure")
                        && !flag(def, "logisticsTruck")
                        && distance(u.at, s.at) < 25.0
                })
        });
        if let Some(s) = incoming {
            return Ok(Some(support("cover", s.at)));
        }
    }
    if let Some((heavy, _)) = enemies.iter().find(|(u, _)| {
        watched(u.id)
            && matches!(
                u.kind.as_str(),
                "tank" | "medium" | "tiger" | "churchill" | "flaktrack"
            )
    }) && can("dive")?
        && in_situation(heavy.at)
    {
        return Ok(Some(support("dive", heavy.at)));
    }
    let army = input
        .units
        .iter()
        .zip(&definitions)
        .filter(|(u, def)| {
            u.owner == input.slot
                && !facts.unavailable.contains(&u.id)
                && !flag(def, "structure")
                && !flag(def, "logisticsTruck")
                && !flag(def, "rail")
                && u.kind != "engineer"
                && !(facts.mode.is_skirmish() && u.build)
        })
        .count();
    if let Some((index, point)) = input.points.iter().enumerate().find(|(i, p)| {
        p.owner
            .is_some_and(|owner| input.players[owner].team != team)
            && facts.visible_points.contains(i)
    }) && !facts.horde_seat
        && army >= 6
        && facts.ready_points.contains(&index)
        && can("para")?
        && in_situation(point.at)
    {
        return Ok(Some(support("para", point.at)));
    }
    let bomb = enemies
        .iter()
        .find(|(u, _)| watched(u.id) && (u.kind == "tank" || u.garrison));
    let bombing_ready = can("bombing")?;
    if let Some((target, _)) = bomb.filter(|_| bombing_ready) {
        if in_situation(target.at) {
            return Ok(Some(support("bombing", target.at)));
        }
    } else if can("artillery")? {
        let target = cluster(8.0, false).or_else(|| {
            enemies
                .iter()
                .find(|(u, _)| {
                    watched(u.id)
                        && matches!(u.kind.as_str(), "mg" | "at")
                        && facts
                            .stationary_since
                            .get(&u.id)
                            .is_some_and(|since| now - since > 3.0)
                        && (level != Level::Hard || clear(u.at))
                })
                .map(|(u, _)| u.at)
        });
        if let Some(at) = target.filter(|at| in_situation(*at)) {
            return Ok(Some(support("artillery", at)));
        }
    } else if can("strafe")?
        && let Some(at) = cluster(10.0, true).filter(|at| in_situation(*at))
    {
        return Ok(Some(support("strafe", at)));
    }
    if can("recon")?
        && enemies.is_empty()
        && (facts.mode.is_construction() || input.manpower > 250.0)
        && let Some(point) = input.points.iter().find(|p| {
            p.owner
                .is_some_and(|owner| input.players[owner].team != team)
        })
        && in_situation(point.at)
    {
        return Ok(Some(support("recon", point.at)));
    }
    Ok(None)
}

#[cfg(test)]
#[path = "commander_support_policy_tests.rs"]
mod tests;
