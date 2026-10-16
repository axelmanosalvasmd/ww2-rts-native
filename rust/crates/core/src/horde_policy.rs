//! Scripted Horde decisions from a detached seat packet. Outputs are command
//! attempts; this module neither admits them nor opens Horde runtime capabilities.
use std::collections::BTreeMap;

use crate::{
    catalog::{Catalog, Value},
    commander_ai::Level,
    commander_tactics::Skill,
    economy::{self, Mode},
    game::{PlaneState, Point},
    horde_commander::View,
    match_commands::{Command, MoveOrder, Stance},
    math,
};

/// A decoded delivered unit, including owner-only cooldown/stance facts for
/// owned units. Enemy rows must already have passed the recipient visibility filter.
#[derive(Clone, Debug, PartialEq)]
pub struct Unit {
    pub id: u32,
    pub owner: usize,
    pub kind: String,
    pub at: Point,
    pub hp: f64,
    pub suppression: f64,
    pub plane: Option<PlaneState>,
    pub cooldown: f64,
    pub auto_retreat: bool,
    pub garrisoned: bool,
    pub in_cover: bool,
    pub riding: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PointFact {
    pub at: Point,
    pub owner: Option<usize>,
    pub visible: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Strike {
    pub at: Point,
    pub owner: usize,
    pub kind: String,
    pub delay: f64,
}

/// Rust-private decoded facts, not a wire schema or an authoritative-world view.
/// Coordinates, health, wallets and cooldowns retain delivered precision.
#[derive(Clone, Debug, PartialEq)]
pub struct Facts {
    pub tick: i64,
    pub slot: usize,
    pub spawn: Point,
    pub teams: Vec<i32>,
    pub manpower: f64,
    pub support: BTreeMap<String, f64>,
    pub units: BTreeMap<u32, Unit>,
    pub points: Vec<PointFact>,
    pub strikes: Vec<Strike>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Still {
    at: Point,
    since: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct HordePolicy {
    still: BTreeMap<u32, Still>,
    last_observation: Option<i64>,
}

fn distance(a: Point, b: Point) -> f64 {
    math::hypot(a.x - b.x, a.z - b.z)
}
fn flag(rule: &Value, key: &str) -> bool {
    rule.get(key).and_then(Value::boolean).unwrap_or(false)
}
fn number(rule: &Value, key: &str) -> f64 {
    rule.get(key).and_then(Value::number).unwrap_or(0.0)
}
fn finite(at: Point) -> bool {
    at.x.is_finite() && at.z.is_finite()
}
fn nearest_to<'a>(candidates: impl Iterator<Item = &'a Unit>, at: Point) -> Option<&'a Unit> {
    candidates.min_by(|a, b| {
        distance(a.at, at)
            .total_cmp(&distance(b.at, at))
            .then(a.id.cmp(&b.id))
    })
}
fn movement(orders: Vec<MoveOrder>, attack_move: bool) -> Command {
    Command::Move {
        orders,
        control_ids: None,
        attack_move,
        queue: false,
        together: false,
        face: None,
    }
}
fn attempt(
    command: Command,
    commands: &mut Vec<Command>,
    submit: &mut impl FnMut(&Command) -> bool,
) -> bool {
    let accepted = submit(&command);
    commands.push(command);
    accepted
}

impl Facts {
    pub fn validate(&self, view: &View) -> Result<(), String> {
        let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
        if self.tick != view.tick
            || self.tick < 0
            || self.slot != view.wave.slot
            || self.slot >= self.teams.len()
            || self.teams.len() > 6
            || !finite(self.spawn)
            || !self.manpower.is_finite()
            || self.manpower < 0.0
            || self.units.len() > 65_536
            || self.points.len() > 1024
            || self.strikes.len() > 65_536
            || self
                .support
                .iter()
                .any(|(k, cd)| !cd.is_finite() || *cd < 0.0 || catalog.support(k).is_err())
        {
            return Err("invalid delivered Horde policy boundary".into());
        }
        for (&id, unit) in &self.units {
            let rule = catalog.unit(&unit.kind).map_err(|e| e.to_string())?;
            if id == 0
                || id != unit.id
                || unit.owner >= self.teams.len()
                || !finite(unit.at)
                || !unit.hp.is_finite()
                || unit.hp < 0.0
                || !unit.suppression.is_finite()
                || !unit.cooldown.is_finite()
                || flag(rule, "air") != unit.plane.is_some()
                || (unit.owner == self.slot
                    && view.units.get(&id).is_none_or(|own| {
                        own.owner != self.slot
                            || own.hp != unit.hp
                            || own.air != unit.plane.is_some()
                    }))
            {
                return Err("invalid delivered Horde policy unit".into());
            }
        }
        if view.units.values().any(|u| {
            u.owner != self.slot
                || self
                    .units
                    .get(&u.id)
                    .is_none_or(|fact| fact.owner != self.slot)
        }) || self
            .points
            .iter()
            .any(|p| !finite(p.at) || p.owner.is_some_and(|o| o >= self.teams.len()))
            || self.strikes.iter().any(|s| {
                !finite(s.at)
                    || s.owner >= self.teams.len()
                    || !s.delay.is_finite()
                    || catalog.support(&s.kind).is_err()
            })
        {
            return Err("invalid delivered Horde policy ownership".into());
        }
        Ok(())
    }
}

impl HordePolicy {
    /// Stationary evidence is learned from successive delivered coordinates,
    /// never an enemy's private movement timer.
    pub fn observe(&mut self, view: &View, facts: &Facts) -> Result<(), String> {
        facts.validate(view)?;
        if self.last_observation.is_some_and(|old| old > facts.tick) {
            return Err("Horde policy observation moved backwards".into());
        }
        let now = facts.tick as f64 / 20.0;
        self.still.retain(|id, _| facts.units.contains_key(id));
        for unit in facts.units.values() {
            let still = self.still.entry(unit.id).or_insert(Still {
                at: unit.at,
                since: now,
            });
            if distance(still.at, unit.at) > 0.1 + 1e-9 {
                *still = Still {
                    at: unit.at,
                    since: now,
                };
            }
        }
        self.last_observation = Some(facts.tick);
        Ok(())
    }

    /// Normal Horde has no human notice timer, situation, purchasing, research,
    /// health retreat, paradrop, march limit or ability-count limit.
    pub fn plan(&mut self, view: &View, facts: &Facts) -> Result<Vec<Command>, String> {
        // Propose a complete look using the same optimistic local intent mirror
        // as the human planner. Returned commands still require host admission.
        self.plan_with_submission(view, facts, |_| true)
    }

    /// An immediate core submitter can return actual admission results. A
    /// refusal does not consume support or install an aimed-ability intent.
    /// The returned list records attempts, including refused attempts.
    pub fn plan_with_submission(
        &mut self,
        view: &View,
        facts: &Facts,
        mut submit: impl FnMut(&Command) -> bool,
    ) -> Result<Vec<Command>, String> {
        facts.validate(view)?;
        if self.last_observation != Some(facts.tick) {
            return Err("Horde policy requires its delivered observation".into());
        }
        let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
        let definition = |u: &Unit| catalog.unit(&u.kind).expect("validated unit kind");
        let allied = |owner| facts.teams[owner] == facts.teams[facts.slot];
        let all: Vec<_> = facts
            .units
            .values()
            .filter(|u| {
                u.owner == facts.slot
                    && u.hp > 0.0
                    && !flag(definition(u), "structure")
                    && !flag(definition(u), "logisticsTruck")
            })
            .collect();
        let mine: Vec<_> = all
            .iter()
            .copied()
            .filter(|u| u.kind != "engineer" && !flag(definition(u), "rail"))
            .collect();
        let enemies: Vec<_> = facts
            .units
            .values()
            .filter(|u| !allied(u.owner) && u.hp > 0.0)
            .collect();
        let mut commands = Vec::new();
        let steady: Vec<_> = all
            .iter()
            .filter(|u| u.plane.is_none() && !u.auto_retreat)
            .map(|u| i64::from(u.id))
            .collect();
        if !steady.is_empty() {
            attempt(
                Command::Stance {
                    ids: steady,
                    stance: Stance::AutoRetreat,
                    on: true,
                },
                &mut commands,
                &mut submit,
            );
        }
        let skill = Skill::for_level(Level::Normal);
        let now = facts.tick as f64 / 20.0;
        let can = |kind: &str| -> bool {
            let Ok(price) = economy::sup_cost(Mode::Horde, kind) else {
                return false;
            };
            facts.support.get(kind).is_some_and(|cd| *cd <= 0.0)
                && facts.manpower >= price.cost + skill.sup_reserve
        };
        // Source Normal takes the first qualifying cluster. Stable unit ids
        // replace incidental container order, including equal-distance ties.
        let cluster = |radius: f64, infantry: bool| -> Option<Point> {
            for enemy in &enemies {
                let near: Vec<_> = enemies
                    .iter()
                    .filter(|u| {
                        (!infantry || flag(definition(u), "infantry"))
                            && distance(u.at, enemy.at) <= radius
                    })
                    .collect();
                if near.len() >= skill.cluster {
                    return Some(Point {
                        x: near.iter().map(|u| u.at.x).sum::<f64>() / near.len() as f64,
                        z: near.iter().map(|u| u.at.z).sum::<f64>() / near.len() as f64,
                    });
                }
            }
            None
        };
        let incoming = facts.strikes.iter().find(|s| {
            s.delay > 0.0
                && !allied(s.owner)
                && matches!(s.kind.as_str(), "strafe" | "bombing" | "dive" | "para")
                && all.iter().any(|u| distance(u.at, s.at) < 25.0)
        });
        let heavy = enemies.iter().find(|u| {
            matches!(
                u.kind.as_str(),
                "tank" | "medium" | "tiger" | "churchill" | "flaktrack"
            )
        });
        let bomb = enemies.iter().find(|u| u.kind == "tank" || u.garrisoned);
        let mut support = Vec::new();
        if let Some(incoming) = incoming.filter(|_| can("cover")) {
            support.push(("cover", incoming.at));
        }
        if let Some(heavy) = heavy.filter(|_| can("dive")) {
            support.push(("dive", heavy.at));
        }
        if let Some(bomb) = bomb.filter(|_| can("bombing")) {
            support.push(("bombing", bomb.at));
        } else if can("artillery") {
            if let Some(at) = cluster(8.0, false).or_else(|| {
                enemies
                    .iter()
                    .find(|u| {
                        matches!(u.kind.as_str(), "mg" | "at")
                            && self.still.get(&u.id).is_some_and(|s| now - s.since > 3.0)
                    })
                    .map(|u| u.at)
            }) {
                support.push(("artillery", at));
            }
        } else if can("strafe")
            && let Some(at) = cluster(10.0, true)
        {
            support.push(("strafe", at));
        }
        // Recon is a separate final candidate in Source, including when an
        // affordable artillery call found no target. No enemy contact is needed.
        if enemies.is_empty()
            && facts.manpower > 250.0
            && can("recon")
            && let Some(point) = facts
                .points
                .iter()
                .find(|p| p.owner.is_some_and(|o| !allied(o)))
        {
            support.push(("recon", point.at));
        }
        for (kind, at) in support {
            if attempt(
                Command::Support {
                    kind: kind.into(),
                    at,
                    direction: None,
                },
                &mut commands,
                &mut submit,
            ) {
                break;
            }
        }
        let army: Vec<_> = mine.iter().copied().filter(|u| u.plane.is_none()).collect();
        let front = if army.is_empty() {
            facts.spawn
        } else {
            Point {
                x: army.iter().map(|u| u.at.x).sum::<f64>() / army.len() as f64,
                z: army.iter().map(|u| u.at.z).sum::<f64>() / army.len() as f64,
            }
        };
        for unit in mine.iter().filter(|u| u.plane == Some(PlaneState::Base)) {
            if unit.kind == "attacker" {
                let target = nearest_to(
                    enemies.iter().copied().filter(|e| {
                        e.plane.is_none()
                            && !flag(definition(e), "building")
                            && (!flag(definition(e), "infantry")
                                || matches!(e.kind.as_str(), "at" | "flak" | "mortar" | "howitzer"))
                    }),
                    front,
                );
                if let Some(target) = target.filter(|t| distance(t.at, front) < 120.0) {
                    attempt(
                        Command::Attack {
                            ids: vec![i64::from(unit.id)],
                            target: i64::from(target.id),
                            queue: false,
                            at: None,
                        },
                        &mut commands,
                        &mut submit,
                    );
                }
            } else {
                let at = nearest_to(enemies.iter().copied().filter(|u| u.plane.is_some()), front)
                    .map_or(front, |u| u.at);
                attempt(
                    movement(
                        vec![MoveOrder {
                            unit: i64::from(unit.id),
                            at,
                        }],
                        false,
                    ),
                    &mut commands,
                    &mut submit,
                );
            }
        }
        let bunker = enemies
            .iter()
            .copied()
            .filter(|u| flag(definition(u), "structure") && !flag(definition(u), "building"))
            .min_by(|a, b| {
                distance(a.at, facts.spawn)
                    .total_cmp(&distance(b.at, facts.spawn))
                    .then(a.id.cmp(&b.id))
            });
        let mut move_orders = Vec::new();
        let mut assault = Vec::new();
        let reinforce = catalog.table("CFG").map_err(|e| e.to_string())?;
        let aid = reinforce.get("aid").ok_or("missing aid rules")?;
        for unit in mine.iter().filter(|u| u.plane.is_none() && !u.riding) {
            let own = &view.units[&unit.id];
            let p = &own.plan;
            if p.retreating {
                continue;
            }
            let def = definition(unit);
            let ab = def.get("ab").ok_or("missing ability rules")?;
            let kind = ab
                .get("id")
                .and_then(Value::string)
                .ok_or("missing ability id")?;
            let home = distance(unit.at, facts.spawn) <= number(reinforce, "reinforceRadius");
            let frac = unit.hp / (number(def, "models") * number(def, "hpPer"));
            let target = facts.units.get(&p.target).filter(|u| !allied(u.owner));
            let aimed = |range: f64, test: &dyn Fn(&Unit) -> bool| {
                enemies
                    .iter()
                    .copied()
                    .find(|e| distance(unit.at, e.at) <= range && test(e))
                    .map(|e| e.at)
            };
            let at = if unit.cooldown > 0.0 {
                None
            } else {
                match kind {
                    "grenade" => aimed(number(ab, "range") + 4.0, &|e| {
                        flag(definition(e), "infantry") && (e.kind != "rifle" || e.in_cover)
                    })
                    .map(Some),
                    "suppress" if target.is_some() => Some(None),
                    "ap" if target.is_some_and(|e| {
                        !flag(definition(e), "infantry") && !flag(definition(e), "structure")
                    }) =>
                    {
                        Some(None)
                    }
                    "smoke" if frac < 0.5 && !home => Some(None),
                    "satchel" => {
                        aimed(20.0, &|e| e.garrisoned || !flag(definition(e), "infantry")).map(Some)
                    }
                    "ura"
                        if unit.suppression >= 50.0
                            || (p.amove.is_some()
                                && enemies.iter().any(|e| distance(unit.at, e.at) < 30.0)) =>
                    {
                        Some(None)
                    }
                    "barrage" => aimed(number(ab, "range"), &|e| e.garrisoned).map(Some),
                    _ => None,
                }
            };
            let mut aimed_intent = false;
            if let Some(at) = at {
                let accepted = attempt(
                    Command::Ability {
                        ids: vec![i64::from(unit.id)],
                        at,
                        queue: false,
                        automatic: false,
                    },
                    &mut commands,
                    &mut submit,
                );
                aimed_intent = accepted && matches!(kind, "grenade" | "satchel" | "barrage");
            }
            if flag(def, "medic") {
                if p.path.is_empty() && distance(unit.at, front) > number(aid, "seek") * 0.8 {
                    move_orders.push(MoveOrder {
                        unit: i64::from(unit.id),
                        at: front,
                    });
                }
                continue;
            }
            if p.path.is_empty()
                && p.attack == 0
                && p.target == 0
                && p.nade.is_none()
                && !aimed_intent
                && !p.is_digging()
                && p.enter.is_none()
                && p.fire_at.is_none()
                && let Some(bunker) = bunker
            {
                assault.push(MoveOrder {
                    unit: i64::from(unit.id),
                    at: bunker.at,
                });
            }
        }
        if !move_orders.is_empty() {
            attempt(movement(move_orders, false), &mut commands, &mut submit);
        }
        if !assault.is_empty() {
            attempt(movement(assault, true), &mut commands, &mut submit);
        }
        Ok(commands)
    }
}

#[cfg(test)]
#[path = "horde_policy_tests.rs"]
mod tests;
