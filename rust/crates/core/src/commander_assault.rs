//! Persistent assault membership and placement over one seat's delivered facts.
use crate::{
    catalog::{Catalog, Value as Rule},
    commander_ai::{Observation, Point, UnitObservation},
    commander_mind::{Mind, Sighting},
    commander_tactics::{Context, Situation, SituationKind, Skill},
    economy::Mode,
    game::{CELL, COVER},
    math,
    rules::Rules,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub const ASSEMBLE: f64 = 12.0;
pub const REGROUP: f64 = 8.0;
pub const BLOCKED: f64 = 10.0;
pub const LIFETIME: f64 = 100.0;
pub const WITHDRAW: f64 = 1.25;
pub const RESUME: f64 = 0.75;
pub const COOLDOWN: f64 = 18.0;
pub const SUPPORT_REAR: f64 = 8.0;
pub const SEARCH_CELLS: usize = 2048;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Phase {
    Assemble,
    Advance,
    Engage,
    Regroup,
    Withdraw,
    Complete,
    Abandon,
}
impl Phase {
    fn active(self) -> bool {
        !matches!(self, Self::Complete | Self::Abandon)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Role {
    Line,
    Support,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Member {
    pub id: u32,
    pub role: Role,
    pub last: Point,
    pub progress_at: f64,
    pub order: Option<Point>,
    pub placement: Option<Point>,
    pub acknowledged: bool,
}
// The custom decoder returns compiled names instead of borrowing the save buffer.
type SavedReason = &'static str;

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Operation {
    pub id: u64,
    pub phase: Phase,
    pub started: f64,
    pub state_at: f64,
    pub point: Option<usize>,
    pub at: Point,
    pub stage: Point,
    pub initial: usize,
    pub members: Vec<Member>,
    #[serde(with = "saved_reason")]
    pub reason: Option<SavedReason>,
}
impl Operation {
    fn change(&mut self, phase: Phase, now: f64) {
        self.phase = phase;
        self.state_at = now;
        for member in &mut self.members {
            member.progress_at = now;
        }
    }
    fn clear(&mut self, reason: &'static str) {
        self.phase = Phase::Abandon;
        self.reason = Some(reason);
        self.members.clear();
    }
}
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AssaultState {
    pub operation: Option<Operation>,
    pub released: BTreeMap<u32, f64>,
    pub cooldown: f64,
    pub serial: u64,
}
impl AssaultState {
    /// Both operation members and recently released player orders exclude quick orders.
    pub fn controlled(&self, now: f64) -> BTreeSet<u32> {
        self.operation
            .iter()
            .filter(|op| op.phase.active())
            .flat_map(|op| op.members.iter().map(|m| m.id))
            .chain(
                self.released
                    .iter()
                    .filter(|(_, until)| now < **until)
                    .map(|(id, _)| *id),
            )
            .collect()
    }
    pub fn clear(&mut self, reason: &'static str) {
        if let Some(op) = &mut self.operation {
            op.clear(reason);
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Options<'a> {
    pub busy: &'a BTreeSet<u32>,
    pub recovering: &'a BTreeSet<u32>,
    pub sightings: &'a [Sighting],
    pub support_at: Option<f64>,
    pub handoff: bool,
    pub region: Option<[f64; 4]>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Plan {
    pub commands: Vec<Value>,
    pub claims: BTreeSet<u32>,
}
fn dist(a: Point, b: Point) -> f64 {
    math::hypot(a.x - b.x, a.z - b.z)
}
fn num(d: &Rule, key: &str) -> f64 {
    d.get(key).and_then(Rule::number).unwrap_or(0.0)
}
fn boolean(d: &Rule, key: &str) -> bool {
    d.get(key).and_then(Rule::boolean).unwrap_or(false)
}
fn value(u: &UnitObservation, catalog: &Catalog) -> f64 {
    let d = catalog.unit(&u.kind).unwrap();
    num(d, "cost") * u.hp / (num(d, "models") * num(d, "hpPer"))
}
fn support(kind: &str) -> bool {
    matches!(
        kind,
        "mg" | "at" | "mortar" | "howitzer" | "rocket" | "tankdestroyer"
    )
}
fn armor(kind: &str) -> bool {
    matches!(kind, "tank" | "medium" | "tiger" | "churchill")
}
fn anti(kind: &str) -> bool {
    matches!(
        kind,
        "at" | "tank" | "medium" | "tankdestroyer" | "tiger" | "churchill" | "rocket"
    )
}
fn mean<'a>(units: impl Iterator<Item = &'a UnitObservation>) -> Point {
    let (mut at, mut n) = (Point { x: 0.0, z: 0.0 }, 0.0);
    for u in units {
        at.x += u.at.x;
        at.z += u.at.z;
        n += 1.0;
    }
    Point {
        x: at.x / n,
        z: at.z / n,
    }
}
fn back_from(at: Point, home: Point, metres: f64) -> Point {
    let range = dist(at, home);
    let range = if range == 0.0 { 1.0 } else { range };
    let k = math::min(range, metres) / range;
    Point {
        x: at.x + (home.x - at.x) * k,
        z: at.z + (home.z - at.z) * k,
    }
}
fn region_has(region: Option<[f64; 4]>, at: Point) -> bool {
    region.is_none_or(|b| at.x >= b[0] && at.z >= b[1] && at.x < b[2] && at.z < b[3])
}
#[derive(Clone, Debug)]
pub struct Threat {
    pub id: u32,
    pub kind: String,
    pub at: Point,
    pub worth: f64,
}
fn weapon_range(catalog: &Catalog, kind: &str) -> Option<f64> {
    catalog.unit(kind).ok()?.get("w")?.get("range")?.number()
}

/// Breadth-first search keeps the prototype's left, right, up, down tie order and 2,048-cell cap.
#[allow(clippy::too_many_arguments)]
pub fn operation_position(
    observation: &Observation,
    context: &Context,
    unit: &UnitObservation,
    desired: Point,
    threats: &[Threat],
    safe: bool,
    region: Option<[f64; 4]>,
    support_target: Option<Point>,
) -> Result<Option<Point>, String> {
    let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
    let rules = Rules::from_catalog(catalog).map_err(|e| e.to_string())?;
    let game = crate::commander_tactics::sight_game(observation, context);
    position(
        observation,
        context,
        unit,
        desired,
        threats,
        safe,
        region,
        support_target,
        catalog,
        &rules,
        &game,
    )
}
#[allow(clippy::too_many_arguments)]
fn position(
    observation: &Observation,
    context: &Context,
    unit: &UnitObservation,
    desired: Point,
    threats: &[Threat],
    safe: bool,
    region: Option<[f64; 4]>,
    support_target: Option<Point>,
    catalog: &Catalog,
    rules: &Rules,
    game: &crate::game::Game,
) -> Result<Option<Point>, String> {
    let input = observation.input();
    let x = (unit.at.x / CELL).floor() as usize;
    let z = (unit.at.z / CELL).floor() as usize;
    if x >= input.width || z >= input.height {
        return Ok(None);
    }
    let start = z * input.width + x;
    if input.chars[start] == b'?' {
        return Ok(None);
    }
    let mask = rules.kinds[rules.kind(&unit.kind).ok_or("unknown assault member")?].block();
    let mut queue = Vec::with_capacity(SEARCH_CELLS);
    queue.push(start);
    let mut seen = BTreeSet::from([start]);
    let mut best = None;
    let mut score = f64::INFINITY;
    let height = |c: usize| context.height.as_ref().map_or(0, |h| i16::from(h[c]));
    let mut i = 0;
    while i < queue.len() && i < SEARCH_CELLS {
        let c = queue[i];
        let (x, z) = (c % input.width, c / input.width);
        let at = Point {
            x: (x as f64 + 0.5) * CELL,
            z: (z as f64 + 0.5) * CELL,
        };
        let danger = threats.iter().any(|e| {
            dist(at, e.at) < math::min(weapon_range(catalog, &e.kind).unwrap_or(20.0), 26.0)
        });
        if region_has(region, at)
            && input.flags[c] & mask == 0
            && (!safe || !danger)
            && support_target.is_none_or(|target| {
                dist(at, target) <= weapon_range(catalog, &unit.kind).unwrap_or(0.0) * 0.9
                    && crate::vision::los(game, rules, (at.x, at.z), (target.x, target.z))
            })
        {
            let candidate = dist(at, desired) + dist(at, unit.at) * 0.08
                - if input.flags[c] & COVER != 0 {
                    5.0
                } else {
                    0.0
                }
                + if danger { 8.0 } else { 0.0 };
            if candidate < score {
                best = Some(at);
                score = candidate;
            }
        }
        let neighbors = [
            x.checked_sub(1).map(|nx| (nx, z)),
            (x + 1 < input.width).then_some((x + 1, z)),
            z.checked_sub(1).map(|nz| (x, nz)),
            (z + 1 < input.height).then_some((x, z + 1)),
        ];
        for (nx, nz) in neighbors.into_iter().flatten() {
            let n = nz * input.width + nx;
            let at = Point {
                x: (nx as f64 + 0.5) * CELL,
                z: (nz as f64 + 0.5) * CELL,
            };
            if seen.contains(&n)
                || input.chars[n] == b'?'
                || input.flags[n] & mask != 0
                || !region_has(region, at)
                || (height(n) - height(c)).abs() > 1
            {
                continue;
            }
            seen.insert(n);
            if queue.len() < SEARCH_CELLS {
                queue.push(n);
            }
        }
        i += 1;
    }
    Ok(best)
}

#[allow(clippy::too_many_arguments)]
pub fn plan(
    observation: &Observation,
    context: &Context,
    mind: &Mind,
    situation: Option<&Situation>,
    skill: Skill,
    now: f64,
    state: &mut AssaultState,
    options: Options<'_>,
) -> Result<Plan, String> {
    let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
    let input = observation.input();
    let me = &input.players[input.slot];
    let unit = |id: u32| {
        input
            .units
            .binary_search_by_key(&id, |u| u.id)
            .ok()
            .map(|i| &input.units[i])
    };
    let allied = |owner: Option<usize>| {
        owner
            .and_then(|o| input.players.get(o))
            .is_some_and(|p| p.team == me.team)
    };
    let watched: Vec<_> = input
        .units
        .iter()
        .filter(|u| {
            u.hp > 0.0
                && !allied(Some(u.owner))
                && !boolean(catalog.unit(&u.kind).unwrap(), "air")
                && !boolean(catalog.unit(&u.kind).unwrap(), "structure")
                && mind.watched(u.id, now, skill.notice)
        })
        .collect();
    state.released.retain(|_, until| now < *until);
    let mut result = Plan::default();
    if options.handoff {
        state.clear("handoff");
        return Ok(result);
    }
    if let Some(op) = &mut state.operation
        && op.phase.active()
    {
        op.members.retain_mut(|m| {
            let Some(u) = unit(m.id) else {
                return false;
            };
            let f = context.units.get(&u.id);
            if u.owner != input.slot
                || u.hp <= 0.0
                || u.retreating
                || options.recovering.contains(&u.id)
                || f.is_some_and(|f| f.hold_position || f.queued_orders > 0)
                || u.dig
                || u.entrench
                || u.build
                || u.garrison
            {
                return false;
            }
            let goal = f.and_then(|f| f.amove).or_else(|| u.path.last().copied());
            if let (Some(goal), Some(order)) = (goal, m.order) {
                if dist(goal, order) > 8.0 {
                    state.released.insert(u.id, now + COOLDOWN);
                    return false;
                }
                m.acknowledged = true;
            } else if m.acknowledged
                && m.order.is_some_and(|order| dist(u.at, order) > 6.0)
                && u.target_id == 0
            {
                state.released.insert(u.id, now + COOLDOWN);
                return false;
            }
            if dist(u.at, m.last) > 1.0
                || u.target_id != 0
                || m.order.is_some_and(|order| dist(u.at, order) <= 6.0)
            {
                m.progress_at = now;
            }
            m.last = u.at;
            now - m.progress_at <= BLOCKED
        });
        if op.members.is_empty() {
            op.phase = Phase::Abandon;
            op.reason = Some("members-unavailable");
        } else if now - op.started > LIFETIME {
            op.clear("timeout");
        } else if op
            .point
            .and_then(|i| input.points.get(i))
            .is_some_and(|p| allied(p.owner) && !p.cut)
        {
            op.phase = Phase::Complete;
            op.members.clear();
        } else if situation.is_some_and(|s| s.kind == SituationKind::Defense)
            && op.phase != Phase::Withdraw
        {
            op.clear("defense");
        }
    }
    let active = state.operation.as_ref().is_some_and(|op| op.phase.active());
    let objective = situation
        .and_then(|s| s.point)
        .and_then(|i| input.points.get(i));
    let taken = objective.is_some_and(|p| allied(p.owner) && !p.cut);
    let defended = objective
        .is_none_or(|p| p.owner.is_some() || watched.iter().any(|e| dist(e.at, p.at) < 32.0));
    let opening = matches!(context.mode, Mode::Default | Mode::Conquest)
        && now < 180.0
        && input.points.iter().any(|p| p.owner.is_none());
    if !active
        && now >= state.cooldown
        && !opening
        && !taken
        && defended
        && now >= skill.first_assault
        && let Some(sit) = situation.filter(|s| s.kind == SituationKind::Push)
        && let Some(at) = sit.at
    {
        let mut candidates: Vec<_> = input
            .units
            .iter()
            .filter(|u| {
                let d = catalog.unit(&u.kind).unwrap();
                let f = context.units.get(&u.id);
                u.owner == input.slot
                    && !boolean(d, "structure")
                    && !boolean(d, "air")
                    && !boolean(d, "naval")
                    && u.kind != "engineer"
                    && !boolean(d, "medic")
                    && !u.retreating
                    && !u.dig
                    && !u.entrench
                    && !u.build
                    && !u.garrison
                    && !f.is_some_and(|f| f.hold_position || f.queued_orders > 0)
                    && u.target_id == 0
                    && value(u, catalog) >= num(d, "cost") * 0.65
                    && !options.busy.contains(&u.id)
                    && !options.recovering.contains(&u.id)
                    && !state.released.contains_key(&u.id)
                    && !input.points.iter().any(|p| {
                        allied(p.owner) && !p.cut && dist(u.at, p.at) < observation.point_radius()
                    })
            })
            .collect();
        crate::sort::sort_by(&mut candidates, |a, b| dist(a.at, at) - dist(b.at, at));
        let line: Vec<_> = candidates
            .iter()
            .copied()
            .filter(|u| !support(&u.kind))
            .take(skill.wave.max(2))
            .collect();
        let guns: Vec<_> = candidates
            .iter()
            .copied()
            .filter(|u| {
                support(&u.kind)
                    && !line.is_empty()
                    && dist(u.at, mean(line.iter().copied())) <= 60.0
            })
            .take(2)
            .collect();
        if line.len() >= 2 && !guns.is_empty() {
            let picked: Vec<_> = line.into_iter().chain(guns).collect();
            let center = mean(picked.iter().copied());
            let range = dist(center, at);
            let k = math::min(18.0, math::max(0.0, range - 22.0))
                / if range == 0.0 { 1.0 } else { range };
            let stage = Point {
                x: center.x + (at.x - center.x) * k,
                z: center.z + (at.z - center.z) * k,
            };
            state.serial += 1;
            state.operation = Some(Operation {
                id: state.serial,
                phase: Phase::Assemble,
                started: now,
                state_at: now,
                point: sit.point,
                at,
                stage,
                initial: picked.len(),
                reason: None,
                members: picked
                    .iter()
                    .map(|u| Member {
                        id: u.id,
                        role: if support(&u.kind) {
                            Role::Support
                        } else {
                            Role::Line
                        },
                        last: u.at,
                        progress_at: now,
                        order: None,
                        placement: None,
                        acknowledged: false,
                    })
                    .collect(),
            });
        }
    }
    let Some(op) = &mut state.operation else {
        return Ok(result);
    };
    if !op.phase.active() {
        return Ok(result);
    }
    let mut center = mean(op.members.iter().filter_map(|m| unit(m.id)));
    let mut contacts: Vec<_> = watched
        .iter()
        .map(|u| Threat {
            id: u.id,
            kind: u.kind.clone(),
            at: u.at,
            worth: value(u, catalog),
        })
        .collect();
    for s in options.sightings {
        if !contacts.iter().any(|e| e.id == s.id)
            && now - s.time <= 20.0
            && mind.watched(s.id, now, skill.notice)
        {
            contacts.push(Threat {
                id: s.id,
                kind: s.kind.clone(),
                at: s.at,
                worth: s.worth,
            });
        }
    }
    let threats: Vec<_> = contacts
        .into_iter()
        .filter(|e| dist(e.at, center) < 48.0 || dist(e.at, op.at) < 32.0)
        .collect();
    let own: f64 = op
        .members
        .iter()
        .filter_map(|m| unit(m.id))
        .map(|u| value(u, catalog))
        .sum();
    let enemy: f64 = threats.iter().map(|e| e.worth).sum();
    let anti_value: f64 = op
        .members
        .iter()
        .filter_map(|m| unit(m.id))
        .filter(|u| anti(&u.kind))
        .map(|u| value(u, catalog))
        .sum();
    let armor_value: f64 = threats
        .iter()
        .filter(|e| armor(&e.kind))
        .map(|e| e.worth)
        .sum();
    let recent_support = options.support_at.is_some_and(|at| now - at < 8.0);
    let mg = threats.iter().any(|e| e.kind == "mg");
    let answer = recent_support
        || op.members.iter().filter_map(|m| unit(m.id)).any(|u| {
            armor(&u.kind) || matches!(u.kind.as_str(), "mortar" | "howitzer" | "rocket" | "sniper")
        });
    let pressure = enemy / math::max(1.0, own) * if recent_support { 0.7 } else { 1.0 };
    let losses = mind.losses_near(center, now, 12.0, 40.0);
    let lost = op.initial - op.members.len();
    let untenable = !threats.is_empty()
        && (pressure > WITHDRAW
            || armor_value > anti_value * 1.4 + if recent_support { own * 0.8 } else { 0.0 } + 1.0
            || mg && !answer && enemy > own * 0.65
            || lost + losses >= op.initial.div_ceil(3) && pressure > 0.75);
    if op.phase != Phase::Withdraw && untenable {
        op.change(Phase::Withdraw, now);
        op.reason = Some(if armor_value > anti_value * 1.4 + 1.0 {
            "armor"
        } else if mg && !answer {
            "machine-gun"
        } else {
            "losses"
        });
        state.cooldown = now + COOLDOWN;
    }
    if op.phase == Phase::Assemble
        && (now - op.state_at >= ASSEMBLE
            || op.members.iter().all(|m| {
                m.order
                    .is_some_and(|at| dist(unit(m.id).unwrap().at, at) < 6.0)
            }))
    {
        op.change(Phase::Advance, now);
    }
    if op.phase == Phase::Advance
        && (threats.iter().any(|e| dist(e.at, center) < 36.0) || dist(center, op.at) < 18.0)
    {
        op.change(Phase::Engage, now);
    }
    if op.phase == Phase::Engage && threats.is_empty() && dist(center, op.at) > 18.0 {
        op.change(Phase::Advance, now);
    }
    if op.phase == Phase::Engage
        && op
            .members
            .iter()
            .filter_map(|m| unit(m.id))
            .any(|u| dist(u.at, center) > 22.0)
        && now - op.state_at > 4.0
    {
        op.stage = back_from(center, me.spawn, 12.0);
        op.change(Phase::Regroup, now);
    }
    if op.phase == Phase::Regroup && now - op.state_at >= REGROUP {
        op.members
            .retain(|m| dist(unit(m.id).unwrap().at, op.stage) < 22.0);
        if op.members.is_empty() {
            op.clear("regroup-blocked");
            return Ok(result);
        }
        op.change(Phase::Advance, now);
        center = mean(op.members.iter().filter_map(|m| unit(m.id)));
    }
    if op.phase == Phase::Withdraw && now >= state.cooldown {
        if pressure < RESUME && !untenable {
            op.stage = center;
            op.change(Phase::Regroup, now);
        } else {
            op.clear("withdrawn");
            state.cooldown = now + COOLDOWN;
            return Ok(result);
        }
    }
    let rules = Rules::from_catalog(catalog).map_err(|e| e.to_string())?;
    let game = crate::commander_tactics::sight_game(observation, context);
    for member in &mut op.members {
        let u = unit(member.id).unwrap();
        result.claims.insert(u.id);
        let withdrawing = op.phase == Phase::Withdraw;
        let staging = matches!(op.phase, Phase::Assemble | Phase::Regroup);
        let desired = if withdrawing {
            back_from(center, me.spawn, 24.0)
        } else if staging {
            if member.role == Role::Support {
                back_from(op.stage, me.spawn, SUPPORT_REAR)
            } else {
                op.stage
            }
        } else if member.role == Role::Support {
            back_from(
                op.at,
                center,
                math::max(
                    SUPPORT_REAR,
                    math::min(24.0, weapon_range(catalog, &u.kind).unwrap_or(0.0) * 0.55),
                ),
            )
        } else {
            op.at
        };
        let target = (!withdrawing && !staging && member.role == Role::Support)
            .then(|| threats.first().map(|e| e.at))
            .flatten();
        let Some(at) = position(
            observation,
            context,
            u,
            desired,
            &threats,
            withdrawing,
            options.region,
            target,
            catalog,
            &rules,
            &game,
        )?
        else {
            if withdrawing {
                result.commands.push(json!({"t":"retreat", "ids":[u.id]}));
            }
            continue;
        };
        if dist(u.at, at) <= 3.0
            || member.order.is_some_and(|old| dist(old, at) < 3.0)
                && (!u.path.is_empty()
                    || context.units.get(&u.id).is_some_and(|f| f.amove.is_some()))
        {
            continue;
        }
        member.order = Some(at);
        member.placement = Some(at);
        member.acknowledged = true;
        let kind = if withdrawing || staging || member.role == Role::Support {
            "move"
        } else {
            "amove"
        };
        result
            .commands
            .push(json!({"t":kind,"orders":[[u.id,at.x,at.z]]}));
    }
    Ok(result)
}

#[cfg(test)]
#[path = "commander_assault_tests.rs"]
mod tests;

mod saved_reason {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    fn known(value: &str) -> Option<&'static str> {
        match value {
            "handoff" => Some("handoff"),
            "timeout" => Some("timeout"),
            "defense" => Some("defense"),
            "regroup-blocked" => Some("regroup-blocked"),
            "withdrawn" => Some("withdrawn"),
            "members-unavailable" => Some("members-unavailable"),
            "armor" => Some("armor"),
            "machine-gun" => Some("machine-gun"),
            "losses" => Some("losses"),
            _ => None,
        }
    }
    pub fn serialize<S: Serializer>(
        value: &Option<&'static str>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        if value.is_some_and(|v| known(v).is_none()) {
            return Err(serde::ser::Error::custom("unknown assault reason"));
        }
        value.serialize(serializer)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<&'static str>, D::Error> {
        Option::<String>::deserialize(deserializer)?
            .map(|value| {
                known(&value).ok_or_else(|| serde::de::Error::custom("unknown assault reason"))
            })
            .transpose()
    }
}
