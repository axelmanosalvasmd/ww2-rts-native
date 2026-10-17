//! Off-map support decisions over current delivered contacts and announcements.
use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn plan(
    observation: &Observation,
    own: &[&UnitObservation],
    mine: &[&UnitObservation],
    enemies: &[&UnitObservation],
    state: &State,
    situation: &Situation,
    skill: Skill,
    budget: &mut SupportBudget,
    sees: impl Fn(Point) -> bool,
) -> Result<Option<Value>, String> {
    let input = observation.input();
    let context = input
        .runtime
        .as_ref()
        .ok_or("missing delivered runtime context")?;
    let now = input.tick as f64 / 20.0;
    let team = input.players[input.slot].team;
    let allied = |owner: usize| input.players[owner].team == team;
    let watched =
        |u: &UnitObservation| context.horde || state.mind.watched(u.id, now, skill.notice);
    let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
    let clear_of_friends = |at: Point| {
        !input.units.iter().any(|u| {
            allied(u.owner)
                && u.hp > 0.0
                && catalog.unit(&u.kind).is_ok_and(|d| !boolean(d, "air"))
                && dist(u.at, at) < 10.0
        })
    };
    let cluster = |radius: f64, infantry: bool| {
        let mut best = None;
        let mut most = skill.cluster - 1;
        for e in enemies.iter().filter(|e| watched(e)) {
            let near: Vec<_> = enemies
                .iter()
                .filter(|u| {
                    watched(u)
                        && dist(u.at, e.at) <= radius
                        && (!infantry
                            || catalog.unit(&u.kind).is_ok_and(|d| boolean(d, "infantry")))
                })
                .collect();
            if near.len() <= most {
                continue;
            }
            let at = Point {
                x: near.iter().map(|u| u.at.x).sum::<f64>() / near.len() as f64,
                z: near.iter().map(|u| u.at.z).sum::<f64>() / near.len() as f64,
            };
            if !skill.best {
                return Some(at);
            }
            if clear_of_friends(at) {
                best = Some(at);
                most = near.len();
            }
        }
        best
    };
    let mut call = |kind: &str, at: Option<Point>| -> Result<Option<Value>, String> {
        if let Some(at) = at {
            budget.call(kind, at, None, situation, context, skill, now)
        } else {
            Ok(None)
        }
    };
    let incoming = context.strikes.iter().find(|s| {
        s.delay > 0.0
            && !allied(s.owner)
            && matches!(s.kind.as_str(), "strafe" | "bombing" | "dive" | "para")
            && own.iter().any(|u| dist(u.at, s.at) < 25.0)
    });
    if let Some(command) = call("cover", incoming.map(|s| s.at))? {
        return Ok(Some(command));
    }
    let heavy = enemies
        .iter()
        .find(|e| (armor(&e.kind) || e.kind == "flaktrack") && watched(e));
    if let Some(command) = call("dive", heavy.map(|u| u.at))? {
        return Ok(Some(command));
    }
    let drop_zone = input
        .points
        .iter()
        .enumerate()
        .find(|(_, p)| p.owner.is_some_and(|owner| !allied(owner)) && sees(p.at));
    if !context.horde
        && mine.len() >= 6
        && let Some((index, point)) = drop_zone
        && state.mind.point_ready(index, now, skill.notice)
        && let Some(command) = call("para", Some(point.at))?
    {
        return Ok(Some(command));
    }
    let bomb_target = enemies
        .iter()
        .find(|e| (e.kind == "tank" || e.garrison) && watched(e));
    // Eligibility selects this branch before aim distance is checked, as in the prototype.
    let tactical = if let Some(target) = bomb_target
        && budget.can("bombing", context, skill, now)?
    {
        budget.call("bombing", target.at, None, situation, context, skill, now)?
    } else if budget.can("artillery", context, skill, now)? {
        let at = cluster(8.0, false).or_else(|| {
            enemies
                .iter()
                .find(|e| {
                    matches!(e.kind.as_str(), "mg" | "at")
                        && watched(e)
                        && context
                            .stationary_since
                            .get(&e.id)
                            .is_some_and(|since| now - since > 3.0)
                        && (!skill.best || clear_of_friends(e.at))
                })
                .map(|e| e.at)
        });
        if let Some(at) = at {
            budget.call("artillery", at, None, situation, context, skill, now)?
        } else {
            None
        }
    } else if budget.can("strafe", context, skill, now)? {
        if let Some(at) = cluster(10.0, true) {
            budget.call("strafe", at, None, situation, context, skill, now)?
        } else {
            None
        }
    } else {
        None
    };
    if tactical.is_some() {
        return Ok(tactical);
    }
    if enemies.is_empty()
        && (context.mode.is_construction() || input.manpower > 250.0)
        && let Some(point) = input
            .points
            .iter()
            .find(|p| p.owner.is_some_and(|owner| !allied(owner)))
    {
        return budget.call("recon", point.at, None, situation, context, skill, now);
    }
    Ok(None)
}
