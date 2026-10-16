//! Detached base attacks and Hard focus fire. All positions come from the seat's delivered view.
use std::collections::{BTreeMap, BTreeSet};

use crate::{
    catalog::{Catalog, Value},
    commander_ai::{Level, Observation, Point, UnitObservation},
    economy::Mode,
    game::CELL,
    match_commands::{Command, MoveOrder},
    math,
    weather::{WeatherKind, WeatherPlan, WeatherRules},
};

#[derive(Clone, Debug, PartialEq)]
pub struct KnownBuilding {
    pub id: u32,
    pub kind: String,
    pub owner: usize,
    pub at: Point,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RecentStrength {
    pub owner: usize,
    pub worth: f64,
    /// Simulation seconds when this strength was observed.
    pub seen_at: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BaseAttackFacts {
    pub mode: Mode,
    pub weather: WeatherKind,
    pub push: bool,
    pub known_buildings: Vec<KnownBuilding>,
    pub recent: Vec<RecentStrength>,
    /// Own actors claimed by another action this look. They still count toward army strength.
    pub unavailable: BTreeSet<u32>,
}

/// Own orders only. The first queued attack-move survives switching focus targets.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct OwnFocusOrders {
    pub attack_target: Option<u32>,
    pub amove: Option<Point>,
    pub queued_amove: Option<Point>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct FocusFacts {
    /// Missing mode uses the ordinary skirmish rules.
    pub mode: Option<Mode>,
    pub owned: BTreeMap<u32, OwnFocusOrders>,
    /// Visible enemy IDs whose notice delay has passed, supplied by the seat's memory.
    pub watched: BTreeSet<u32>,
    /// Clear sight lines from own actors to currently visible enemies only.
    pub clear_lines: BTreeSet<(u32, u32)>,
    pub unavailable: BTreeSet<u32>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FocusState {
    /// Issued focus targets. Enemy IDs may survive a lost view, but positions never do.
    pub targets: BTreeMap<u32, u32>,
}

fn unit(observation: &Observation, id: u32) -> Option<&UnitObservation> {
    let units = &observation.input().units;
    units
        .binary_search_by_key(&id, |u| u.id)
        .ok()
        .map(|i| &units[i])
}
fn own(observation: &Observation, id: u32) -> bool {
    unit(observation, id).is_some_and(|u| u.owner == observation.input().slot)
}
fn enemy(observation: &Observation, owner: usize) -> bool {
    let input = observation.input();
    input
        .players
        .get(owner)
        .is_some_and(|p| p.team != input.players[input.slot].team)
}
fn within(observation: &Observation, at: Point) -> bool {
    let input = observation.input();
    at.x.is_finite()
        && at.z.is_finite()
        && at.x >= 0.0
        && at.z >= 0.0
        && at.x <= input.width as f64 * CELL
        && at.z <= input.height as f64 * CELL
}
fn distance(a: Point, b: Point) -> f64 {
    math::hypot(a.x - b.x, a.z - b.z)
}
fn flag(def: &Value, key: &str) -> bool {
    def.get(key).and_then(Value::boolean).unwrap_or(false)
}
fn truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => *n != 0.0 && !n.is_nan(),
        Some(Value::String(s)) => !s.is_empty(),
        _ => true,
    }
}
fn number(def: &Value, key: &str) -> Result<f64, String> {
    def.get(key)
        .and_then(Value::number)
        .filter(|v| v.is_finite())
        .ok_or_else(|| format!("attack policy: missing catalog number {key}"))
}
fn definition<'a>(catalog: &'a Catalog, u: &UnitObservation) -> Result<&'a Value, String> {
    catalog.unit(&u.kind).map_err(|e| e.to_string())
}
fn army(u: &UnitObservation, def: &Value, slot: usize) -> bool {
    u.owner == slot
        && u.hp > 0.0
        && !flag(def, "structure")
        && !flag(def, "logisticsTruck")
        && !flag(def, "rail")
        && u.kind != "engineer"
}
fn skirmish(mode: Mode) -> bool {
    matches!(
        mode,
        Mode::Default | Mode::Conquest | Mode::Assault | Mode::Annihilation | Mode::Horde
    )
}
fn worth(u: &UnitObservation, def: &Value) -> Result<f64, String> {
    Ok(number(def, "cost")? * u.hp / (number(def, "models")? * number(def, "hpPer")?))
}
fn move_command(orders: Vec<MoveOrder>, queue: bool) -> Command {
    Command::Move {
        orders,
        control_ids: None,
        attack_move: true,
        queue,
        together: false,
        face: None,
    }
}
fn unavailable_valid(observation: &Observation, ids: &BTreeSet<u32>) -> Result<(), String> {
    if ids.iter().any(|id| !own(observation, *id)) {
        return Err("attack policy: unavailable actor is not owned".into());
    }
    Ok(())
}

/// March idle actors only during a push, respecting weather, strength and the difficulty's hands.
pub fn base_attack_plan(
    observation: &Observation,
    facts: &BaseAttackFacts,
    level: Level,
) -> Result<Vec<Command>, String> {
    let input = observation.input();
    unavailable_valid(observation, &facts.unavailable)?;
    let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
    let mut previous = 0;
    for b in &facts.known_buildings {
        if b.id <= previous || b.owner >= input.players.len() || !within(observation, b.at) {
            return Err("attack policy: invalid remembered building".into());
        }
        previous = b.id;
        let def = catalog.unit(&b.kind).map_err(|e| e.to_string())?;
        if !flag(def, "building") {
            return Err("attack policy: remembered object is not a building".into());
        }
    }
    let now = input.tick as f64 / 20.0;
    if facts.recent.iter().any(|s| {
        s.owner >= input.players.len()
            || !s.worth.is_finite()
            || s.worth < 0.0
            || !s.seen_at.is_finite()
            || s.seen_at < 0.0
            || s.seen_at > now
    }) {
        return Err("attack policy: invalid strength memory".into());
    }
    if input.players[input.slot].out || !facts.push {
        return Ok(Vec::new());
    }
    // Key order defines ties independently of input container order.
    let mut targets: Vec<(u64, Option<usize>, Point)> = Vec::new();
    match facts.mode {
        Mode::Assault | Mode::Annihilation | Mode::Horde => {
            for u in &input.units {
                let def = definition(catalog, u)?;
                if enemy(observation, u.owner)
                    && u.hp > 0.0
                    && flag(def, "structure")
                    && !flag(def, "building")
                {
                    targets.push((u64::from(u.id), Some(u.owner), u.at));
                }
            }
        }
        Mode::Classic | Mode::World => {
            for b in &facts.known_buildings {
                let def = catalog.unit(&b.kind).map_err(|e| e.to_string())?;
                if enemy(observation, b.owner) && truthy(def.get("produces")) {
                    targets.push((u64::from(b.id), Some(b.owner), b.at));
                }
            }
            if targets.is_empty() && facts.mode == Mode::Classic {
                for (i, p) in input.players.iter().enumerate() {
                    if enemy(observation, i) && !p.out {
                        // A public spawn has no owning target in the source strength gate.
                        targets.push((
                            u64::MAX - input.players.len() as u64 + i as u64,
                            None,
                            p.spawn,
                        ));
                    }
                }
            }
        }
        _ => {}
    }
    if targets.is_empty() {
        return Ok(Vec::new());
    }
    let home = input.players[input.slot].spawn;
    targets.sort_by(|a, b| {
        distance(home, a.2)
            .total_cmp(&distance(home, b.2))
            .then(a.0.cmp(&b.0))
    });
    let mut mine = Vec::new();
    let mut our_worth = 0.0;
    for u in &input.units {
        let def = definition(catalog, u)?;
        if army(u, def, input.slot) && !(skirmish(facts.mode) && u.build) {
            our_worth += worth(u, def)?;
            mine.push((u, def));
        }
    }
    let caution = WeatherRules::compiled()
        .map_err(|e| e.to_string())?
        .ai_caution(Some(&WeatherPlan::fixed(facts.weather)));
    let base_army = if level == Level::Easy { 10.0 } else { 6.0 };
    if mine.len() < math::round(base_army * caution) as usize {
        return Ok(Vec::new());
    }
    if matches!(facts.mode, Mode::Classic | Mode::World) && level != Level::Easy {
        let team = targets[0].1.map(|owner| input.players[owner].team);
        let classic = catalog
            .table("CFG")
            .map_err(|e| e.to_string())?
            .get("classic")
            .ok_or("attack policy: missing Classic rules")?;
        let window = number(classic, "aiSeenWindow")?;
        let ratio = number(classic, "aiAttackRatio")?;
        let their_worth: f64 = facts
            .recent
            .iter()
            .filter(|s| now - s.seen_at < window && team == Some(input.players[s.owner].team))
            .map(|s| s.worth)
            .sum();
        if our_worth <= ratio * caution * their_worth {
            return Ok(Vec::new());
        }
    }
    let hands = match level {
        Level::Easy => 4,
        Level::Normal => 6,
        Level::Hard => 8,
    };
    let mut orders = Vec::new();
    for (u, def) in mine {
        if facts.unavailable.contains(&u.id)
            || flag(def, "air")
            || u.retreating
            || u.riding
            || !u.path.is_empty()
            || u.attack_id
            || u.target_id != 0
            || u.nade
            || u.dig
            || u.enter
            || u.fire_at
            || flag(def, "medic")
        {
            continue;
        }
        let target = targets
            .iter()
            .min_by(|a, b| {
                distance(u.at, a.2)
                    .total_cmp(&distance(u.at, b.2))
                    .then(a.0.cmp(&b.0))
            })
            .unwrap();
        orders.push(MoveOrder {
            unit: i64::from(u.id),
            at: target.2,
        });
        if orders.len() == hands {
            break;
        }
    }
    Ok(if orders.is_empty() {
        Vec::new()
    } else {
        vec![move_command(orders, false)]
    })
}

fn validate_focus(
    observation: &Observation,
    facts: &FocusFacts,
    state: &FocusState,
) -> Result<(), String> {
    unavailable_valid(observation, &facts.unavailable)?;
    let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
    for id in facts.owned.keys() {
        if let Some(u) = unit(observation, *id)
            && flag(definition(catalog, u)?, "structure")
        {
            return Err("focus policy: own order keys include a structure".into());
        }
    }
    if facts.owned.iter().any(|(id, orders)| {
        !own(observation, *id)
            || orders.attack_target == Some(0)
            || orders.amove.is_some_and(|p| !within(observation, p))
            || orders.queued_amove.is_some_and(|p| !within(observation, p))
    }) {
        return Err("focus policy: invalid own order facts".into());
    }
    for u in &observation.input().units {
        if u.owner == observation.input().slot
            && !flag(definition(catalog, u)?, "structure")
            && !facts.owned.contains_key(&u.id)
        {
            return Err("focus policy: missing own actor orders".into());
        }
        if u.owner == observation.input().slot
            && !flag(definition(catalog, u)?, "structure")
            && u.attack_id
                != facts
                    .owned
                    .get(&u.id)
                    .is_some_and(|o| o.attack_target.is_some())
        {
            return Err("focus policy: missing or inconsistent own attack order".into());
        }
    }
    let visible_enemy = |id| unit(observation, id).is_some_and(|u| enemy(observation, u.owner));
    if facts.watched.iter().any(|id| !visible_enemy(*id))
        || facts
            .clear_lines
            .iter()
            .any(|(from, to)| !own(observation, *from) || !visible_enemy(*to))
        || state.targets.iter().any(|(id, target)| {
            *id == 0
                || *target == 0
                || unit(observation, *id).is_some_and(|u| u.owner != observation.input().slot)
        })
    {
        return Err("focus policy: invalid ownership or visible target keys".into());
    }
    Ok(())
}

fn hit(weapon: &Value, infantry: bool) -> Result<f64, String> {
    Ok(if infantry {
        number(weapon, "inf")? * number(weapon, "accInf")?
    } else {
        number(weapon, "veh")? * number(weapon, "accVeh")?
    })
}

/// At most one target per look. Failure leaves the seat's focus state unchanged.
pub fn focus_plan(
    observation: &Observation,
    facts: &FocusFacts,
    state: &mut FocusState,
    level: Level,
) -> Result<Vec<Command>, String> {
    validate_focus(observation, facts, state)?;
    if level != Level::Hard || observation.input().players[observation.input().slot].out {
        return Ok(Vec::new());
    }
    let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
    let slot = observation.input().slot;
    let mut next = state.clone();
    next.targets.retain(|id, target| {
        facts
            .owned
            .get(id)
            .is_some_and(|o| o.attack_target == Some(*target))
    });
    let mut stop = Vec::new();
    let mut shooters = Vec::new();
    for u in &observation.input().units {
        let def = definition(catalog, u)?;
        if !army(u, def, slot)
            || (skirmish(facts.mode.unwrap_or(Mode::Default)) && u.build)
            || flag(def, "air")
            || facts.unavailable.contains(&u.id)
        {
            continue;
        }
        let orders = facts.owned.get(&u.id).copied().unwrap_or_default();
        let weapon = def.get("w").filter(|w| w.object().is_some());
        let old = orders.attack_target;
        if let Some(id) = old {
            let drop = match unit(observation, id) {
                None => true,
                Some(t) => {
                    t.hp <= 0.0
                        || t.retreating
                        || !enemy(observation, t.owner)
                        || weapon.is_none_or(|w| {
                            number(w, "range").map_or(true, |range| distance(u.at, t.at) > range)
                        })
                        || !facts.clear_lines.contains(&(u.id, id))
                }
            };
            if drop {
                stop.push(MoveOrder {
                    unit: i64::from(u.id),
                    at: orders.queued_amove.unwrap_or(u.at),
                });
                next.targets.remove(&u.id);
            }
        }
        if !u.retreating
            && !u.riding
            && u.target_id != 0
            && !u.garrison
            && !u.nade
            && !u.dig
            && !u.fire_at
            && let Some(w) = weapon
            && !truthy(w.get("salvo"))
        {
            shooters.push((u, def, w));
        }
    }
    let mut commands = Vec::new();
    if !stop.is_empty() {
        commands.push(move_command(stop, false));
    }
    let mut best_score = 0.0;
    let mut best = None;
    let mut team = Vec::new();
    for target in &observation.input().units {
        let def = definition(catalog, target)?;
        if !enemy(observation, target.owner)
            || target.hp <= 0.0
            || target.retreating
            || flag(def, "air")
            || flag(def, "structure")
            || !facts.watched.contains(&target.id)
        {
            continue;
        }
        let inf = flag(def, "infantry");
        let mut ids = Vec::new();
        let mut dps = 0.0;
        for (u, own_def, w) in &shooters {
            let h = hit(w, inf)?;
            if distance(u.at, target.at) > number(w, "range")?
                || h < 0.5 * math::max(hit(w, true)?, hit(w, false)?)
                || !facts.clear_lines.contains(&(u.id, target.id))
            {
                continue;
            }
            let models = if flag(w, "perModel") {
                (u.hp / number(own_def, "hpPer")?).ceil()
            } else {
                1.0
            };
            dps += h * models / number(w, "interval")?;
            ids.push(*u);
        }
        if ids.len() < 2 {
            continue;
        }
        let full = number(def, "models")? * number(def, "hpPer")?;
        let score = dps * number(def, "cost")? / full * (2.0 - target.hp / full);
        if score > best_score {
            best_score = score;
            best = Some(target.id);
            team = ids;
        }
    }
    if let Some(target) = best {
        let mut go = Vec::new();
        let mut resume = Vec::new();
        for u in team {
            let orders = facts.owned.get(&u.id).copied().unwrap_or_default();
            if orders.attack_target != Some(target) && u.target_id != target {
                go.push(i64::from(u.id));
                if let Some(at) = orders.amove.or(orders.queued_amove) {
                    resume.push(MoveOrder {
                        unit: i64::from(u.id),
                        at,
                    });
                }
                next.targets.insert(u.id, target);
            }
        }
        if !go.is_empty() {
            commands.push(Command::Attack {
                ids: go,
                target: i64::from(target),
                queue: false,
                at: None,
            });
            if !resume.is_empty() {
                commands.push(move_command(resume, true));
            }
        }
    }
    *state = next;
    Ok(commands)
}

#[cfg(test)]
#[path = "commander_attack_policy_tests.rs"]
mod tests;
