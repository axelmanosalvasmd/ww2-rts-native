//! Deterministic seat tactics over detached, delivered facts.
use crate::{
    catalog::{Catalog, Value as Rule},
    commander_ai::{Level, Observation, Point, UnitObservation},
    commander_mind::{Mind, Sighting},
    economy::{self, Faction, Mode, Technology},
    math,
    random::{MatchRandom, StreamId},
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq)]
pub struct Stock {
    pub ammo: Option<f64>,
    pub provisions: f64,
    pub fuel: Option<f64>,
    pub forced: bool,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UnitFacts {
    pub built: f64,
    pub hold_position: bool,
    pub hold_fire: bool,
    pub auto_cast: bool,
    pub cooldown: f64,
    pub attack_target: u32,
    pub queued_orders: usize,
    pub auto_retreat: bool,
    pub convoy: bool,
    pub stock: Option<Stock>,
    pub amove: Option<Point>,
    pub queued_amove: Option<Point>,
    pub airborne: bool,
    pub wall_work: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub struct KnownBuilding {
    pub id: u32,
    pub kind: String,
    pub owner: usize,
    pub at: Point,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Context {
    pub mode: Mode,
    pub naval: bool,
    pub caution: f64,
    pub known_buildings: Vec<KnownBuilding>,
    pub hidden_spawns: BTreeSet<usize>,
    pub site_nodes: Vec<Point>,
    pub horde: bool,
    pub defender: bool,
    pub faction: Faction,
    pub munitions: f64,
    pub fuel: f64,
    pub large_army: bool,
    pub units: BTreeMap<u32, UnitFacts>,
    pub height: Option<Vec<i8>>,
    pub smokes: Vec<crate::game::Smoke>,
    pub sight: f64,
    pub support: BTreeMap<String, f64>,
    /// Resource positions and enemy depot memories already delivered to this seat.
    pub nodes: Vec<Point>,
    pub known_depots: Vec<Point>,
    /// Indices of delivered nodes on owned World ground. Other modes use every node.
    pub owned_nodes: BTreeSet<usize>,
    pub build_reserve: f64,
    pub fort_reserve: f64,
    pub field: Option<crate::commander_field_policy::Facts>,
    pub world: Option<crate::commander_world_policy::Facts>,
    /// Public start terrain and recipient-delivered mine knowledge only.
    pub initial_chars: Option<Vec<u8>>,
    pub initial_height: Option<Vec<i8>>,
    pub mines: Vec<Point>,
    pub found_mines: Vec<Point>,
    /// Caller capability, never inferred from visible actors. Complete live-flight
    /// delivery must be integrated before an ordinary human packet enables it.
    pub ground_flights: bool,
}
impl Default for Context {
    fn default() -> Self {
        Self {
            mode: Mode::Conquest,
            naval: false,
            caution: 1.0,
            known_buildings: vec![],
            hidden_spawns: BTreeSet::new(),
            site_nodes: vec![],
            horde: false,
            defender: false,
            faction: Faction::Usa,
            munitions: 0.0,
            fuel: 0.0,
            large_army: false,
            units: BTreeMap::new(),
            height: None,
            smokes: Vec::new(),
            sight: 1.0,
            support: BTreeMap::new(),
            nodes: Vec::new(),
            known_depots: Vec::new(),
            owned_nodes: BTreeSet::new(),
            build_reserve: 0.0,
            fort_reserve: 0.0,
            field: None,
            world: None,
            initial_chars: None,
            initial_height: None,
            mines: Vec::new(),
            found_mines: Vec::new(),
            ground_flights: false,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Skill {
    pub adaptive: bool,
    pub memory: bool,
    pub sup_reserve: f64,
    pub mun_reserve: f64,
    pub sup_gap: f64,
    pub cluster: usize,
    pub best: bool,
    pub wave: usize,
    pub first_assault: f64,
    pub ratio: f64,
    pub base_army: usize,
    pub retreat: f64,
    pub notice: f64,
    pub hands: usize,
    pub casts: usize,
    pub commit: f64,
    pub focus: bool,
    pub guard: bool,
}
impl Skill {
    pub fn for_level(level: Level) -> Self {
        let mut s = Self {
            adaptive: true,
            memory: false,
            sup_reserve: 100.0,
            mun_reserve: 0.0,
            sup_gap: 0.0,
            cluster: 2,
            best: false,
            wave: 3,
            first_assault: 0.0,
            ratio: 0.0,
            base_army: 6,
            retreat: 0.35,
            notice: 1.25,
            hands: 6,
            casts: 2,
            commit: 10.0,
            focus: false,
            guard: false,
        };
        match level {
            Level::Easy => {
                s.adaptive = false;
                s.sup_reserve = 250.0;
                s.mun_reserve = 40.0;
                s.sup_gap = 45.0;
                s.first_assault = 150.0;
                s.base_army = 10;
                s.notice = 2.5;
                s.hands = 4;
                s.casts = 1;
                s.commit = 16.0;
            }
            Level::Hard => {
                s.memory = true;
                s.cluster = 3;
                s.best = true;
                s.wave = 2;
                s.ratio = 1.2;
                s.retreat = 0.5;
                s.notice = 0.5;
                s.hands = 8;
                s.casts = 3;
                s.commit = 6.0;
                s.focus = true;
                s.guard = true;
            }
            Level::Normal => {}
        }
        s
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SituationKind {
    Defense,
    Wait,
    Push,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Situation {
    pub kind: SituationKind,
    pub until: f64,
    pub point: Option<usize>,
    pub threat: Option<u32>,
    pub at: Option<Point>,
    #[serde(deserialize_with = "crate::save_state::optional_static_string")]
    pub counter: Option<crate::save_state::StaticStr>,
    pub safe: Option<usize>,
    pub failed_at: u8,
}
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct State {
    pub mind: Mind,
    pub raid: Option<crate::commander_classic_rules::Raid>,
    pub assault: crate::commander_assault::AssaultState,
    pub situation: Option<Situation>,
    pub aim: Option<(usize, f64)>,
    pub supply_waiting: BTreeSet<u32>,
    pub released: BTreeSet<u32>,
    pub decision: Option<Decision>,
    pub called: Option<f64>,
    pub cleared_sightings: BTreeMap<u32, f64>,
    pub(crate) fort_steps: BTreeMap<u32, u8>,
    #[serde(default)]
    pub field: crate::commander_field_policy::State,
    #[serde(default)]
    pub world: crate::commander_world_policy::State,
    #[serde(default)]
    pub(crate) engineering: crate::commander_engineering::State,
    #[serde(default)]
    pub(crate) armor_ever_seen: bool,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Decision {
    pub situation: SituationKind,
    pub buy: String,
    pub aim: Option<usize>,
}
// Source point selection uses a strict scan with Infinity as the initial best score.
fn least_score(
    indices: impl Iterator<Item = usize>,
    mut score: impl FnMut(usize) -> f64,
) -> Option<usize> {
    let mut best = None;
    let mut value = f64::INFINITY;
    for i in indices {
        let candidate = score(i);
        if candidate < value {
            best = Some(i);
            value = candidate;
        }
    }
    best
}

fn dist(a: Point, b: Point) -> f64 {
    math::hypot(a.x - b.x, a.z - b.z)
}
fn boolean(d: &Rule, key: &str) -> bool {
    d.get(key).and_then(Rule::boolean).unwrap_or(false)
}
fn num(d: &Rule, key: &str) -> f64 {
    d.get(key).and_then(Rule::number).unwrap_or(0.0)
}
fn armor(k: &str) -> bool {
    matches!(k, "tank" | "medium" | "tiger" | "churchill")
}
fn anti(k: &str) -> bool {
    matches!(
        k,
        "at" | "tank" | "medium" | "tankdestroyer" | "tiger" | "churchill" | "rocket"
    )
}
fn full(d: &Rule) -> f64 {
    num(d, "models") * num(d, "hpPer")
}
fn worth(u: &UnitObservation, d: &Rule) -> f64 {
    num(d, "cost") * u.hp / full(d)
}
fn hurt(u: &UnitObservation, d: &Rule, s: Skill) -> bool {
    let frac = u.hp / full(d);
    frac < s.retreat
        || (num(d, "models") > 1.0 && (u.hp / num(d, "hpPer")).ceil() <= 1.0)
        || (u.supp >= 90.0 && frac < 0.6)
}
fn idle(u: &UnitObservation) -> bool {
    !u.retreating
        && !u.riding
        && !u.build
        && u.path.is_empty()
        && !u.attack_id
        && u.target_id == 0
        && !u.nade
        && !u.dig
        && !u.entrench
        && !u.enter
        && !u.fire_at
}

/// Tier savings protect the next upgrade; armory lines use only spare currency.
pub fn research(
    observation: &Observation,
    context: &Context,
) -> Result<(Vec<Value>, f64, f64), String> {
    let input = observation.input();
    let Some(r) = &input.research else {
        return Ok((vec![], 0.0, 0.0));
    };
    if context.horde {
        return Ok((vec![], 0.0, 0.0));
    }
    let lines = [Technology::Infantry, Technology::Guns, Technology::Armor];
    let now = input.tick as f64 / 20.0;
    let due = match r.tech.tier() {
        1 => now >= 120.0,
        2 => now >= 390.0,
        _ => false,
    };
    let kind = if due && !r.busy.contains(&Technology::Tier) {
        Some(Technology::Tier)
    } else {
        lines
            .into_iter()
            .filter(|k| !r.busy.contains(k) && r.tech.level(*k) < r.tech.tier())
            .min_by_key(|k| r.tech.level(*k))
    };
    let Some(kind) = kind else {
        return Ok((vec![], 0.0, 0.0));
    };
    let Some(cost) = economy::tech_cost(r.tech, kind, context.mode).map_err(|e| e.to_string())?
    else {
        return Ok((vec![], 0.0, 0.0));
    };
    if input.manpower >= cost.mp && context.munitions >= cost.mun {
        let name = match kind {
            Technology::Tier => "tier",
            Technology::Infantry => "inf",
            Technology::Guns => "guns",
            Technology::Armor => "armor",
        };
        Ok((vec![json!({"t":"tech","kind":name})], cost.mp, cost.mun))
    } else {
        Ok((
            vec![],
            if kind == Technology::Tier {
                cost.mp
            } else {
                0.0
            },
            0.0,
        ))
    }
}

/// Plan transactionally. The caller publishes private state with the human motor state.
pub fn plan(
    observation: &Observation,
    sightings: &[Sighting],
    state: &mut State,
    level: Level,
    random: &mut MatchRandom,
) -> Result<Vec<Value>, String> {
    let now = observation.input().tick as f64 / 20.0;
    let mut ids = BTreeSet::new();
    if sightings.iter().any(|s| {
        !ids.insert(s.id)
            || s.id == 0
            || s.owner >= observation.input().players.len()
            || ![s.at.x, s.at.z, s.time, s.worth]
                .iter()
                .all(|v| v.is_finite())
            || s.time > now
            || s.worth < 0.0
            || !Catalog::compiled().is_ok_and(|c| c.unit(&s.kind).is_ok())
    }) {
        return Err("invalid delivered sighting".into());
    }
    let mut next = state.clone();
    let mut rng = random.clone();
    let result = plan_inner(observation, sightings, &mut next, level, &mut rng)?;
    *state = next;
    *random = rng;
    Ok(result)
}
// Each planner stage sees the local effects of earlier queued attempts. The
// original delivered packet stays immutable and actual admission happens later.
pub(crate) fn mirrored(
    observation: &Observation,
    commands: &[Value],
) -> Result<Observation, String> {
    let mut mirror = crate::commander_mirror::Mirror::new(observation.input());
    for command in commands {
        mirror.apply(command, true)?;
    }
    Observation::new(mirror.input().clone()).map_err(|e| e.to_string())
}

fn mirrored_units<'a>(
    observation: &'a Observation,
    units: &[&UnitObservation],
) -> Vec<&'a UnitObservation> {
    units
        .iter()
        .map(|unit| {
            let input = observation.input();
            &input.units[input
                .units
                .binary_search_by_key(&unit.id, |u| u.id)
                .unwrap()]
        })
        .collect()
}

fn plan_inner(
    observation: &Observation,
    sightings: &[Sighting],
    state: &mut State,
    level: Level,
    random: &mut MatchRandom,
) -> Result<Vec<Value>, String> {
    let input = observation.input();
    let context = input
        .runtime
        .as_ref()
        .ok_or("missing delivered runtime context")?;
    let world_policy = context.mode == Mode::World && context.world.is_some();
    if let Some(world) = context.world.as_ref().filter(|_| world_policy) {
        world.validate(observation)?;
    }
    let me = &input.players[input.slot];
    if me.out {
        return Ok(vec![]);
    }
    let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
    let defs: Vec<_> = input
        .units
        .iter()
        .map(|u| catalog.unit(&u.kind).map_err(|e| e.to_string()))
        .collect::<Result<_, _>>()?;
    let definition =
        |u: &UnitObservation| defs[input.units.binary_search_by_key(&u.id, |r| r.id).unwrap()];
    let allied = |owner: Option<usize>| owner.is_some_and(|o| input.players[o].team == me.team);
    let now = input.tick as f64 / 20.0;
    let skill = Skill::for_level(level);
    state
        .cleared_sightings
        .retain(|_, time| now - *time <= 60.0);
    let learned_sightings: Vec<_> = sightings
        .iter()
        .filter(|s| {
            state
                .cleared_sightings
                .get(&s.id)
                .is_none_or(|at| s.time > *at)
        })
        .cloned()
        .collect();
    state
        .mind
        .update(observation, &learned_sightings, !context.horde);
    let failures = state.mind.point_failures();
    let extra = |i: usize| f64::from(failures.get(i).copied().unwrap_or(0).min(3));
    let enemies: Vec<_> = input
        .units
        .iter()
        .filter(|u| u.hp > 0.0 && !allied(Some(u.owner)))
        .collect();
    state.armor_ever_seen |= enemies.iter().any(|unit| {
        let d = definition(unit);
        !boolean(d, "infantry") && !boolean(d, "structure") && !boolean(d, "air")
    });
    let sight_game = sight_game(observation, context);
    let sight_rules = crate::rules::Rules::from_catalog(catalog).map_err(|e| e.to_string())?;
    let sees = |at: Point| {
        if let Some(world) = context.world.as_ref().filter(|_| world_policy) {
            return sight_game
                .cell_of(at.x, at.z)
                .is_some_and(|cell| world.visible_cells.contains(&cell));
        }
        input.units.iter().any(|u| {
            if !allied(Some(u.owner)) {
                return false;
            }
            let d = definition(u);
            let air = boolean(d, "air");
            let reach = if boolean(d, "lookout")
                && context.units.get(&u.id).is_some_and(|f| f.built < 1.0)
            {
                12.0
            } else {
                num(d, "vision")
            };
            dist(u.at, at) <= reach * if air { 1.0 } else { context.sight }
                && if air {
                    context.units.get(&u.id).is_some_and(|f| f.airborne)
                } else {
                    boolean(d, "building")
                        || crate::vision::los(
                            &sight_game,
                            &sight_rules,
                            (u.at.x, u.at.z),
                            (at.x, at.z),
                        )
                }
        })
    };
    for sighting in sightings {
        if enemies.iter().any(|u| u.id == sighting.id) {
            state.cleared_sightings.remove(&sighting.id);
        } else if sees(sighting.at) {
            let time = state
                .cleared_sightings
                .entry(sighting.id)
                .or_insert(sighting.time);
            *time = math::max(*time, sighting.time);
        }
    }
    let mut remembered: Vec<_> = sightings
        .iter()
        .filter(|s| {
            state
                .cleared_sightings
                .get(&s.id)
                .is_none_or(|at| s.time > *at)
                && s.owner < input.players.len()
                && !allied(Some(s.owner))
                && now - s.time <= 60.0
                && (enemies.iter().any(|e| e.id == s.id) || !sees(s.at))
        })
        .collect();
    remembered.sort_by_key(|s| s.id);
    let armor_seen: Vec<_> = remembered
        .iter()
        .copied()
        .filter(|s| armor(&s.kind))
        .collect();
    let owned: Vec<_> = input
        .units
        .iter()
        .filter(|u| {
            u.owner == input.slot
                && u.hp > 0.0
                && !boolean(definition(u), "structure")
                && !boolean(definition(u), "logisticsTruck")
                && !context.units.get(&u.id).is_some_and(|f| f.convoy)
        })
        .collect();
    let alive: BTreeSet<_> = owned.iter().map(|u| u.id).collect();
    state.supply_waiting.retain(|id| alive.contains(id));
    let mut commands = Vec::new();
    if world_policy
        && (!input
            .units
            .iter()
            .any(|u| u.owner == input.slot && u.hp > 0.0 && u.kind == "engineer")
            || !input
                .units
                .iter()
                .any(|u| u.owner == input.slot && u.hp > 0.0 && u.kind == "hq"))
    {
        commands.push(json!({"t":"recover"}));
    }
    let mut mine = Vec::new();
    let mut available_engineers = 0;
    let mut available_units = Vec::new();
    for u in owned {
        if let Some(stock) = context.units.get(&u.id).and_then(|f| f.stock.as_ref()) {
            if stock.forced {
                continue;
            }
            let critical = stock.ammo.is_some_and(|a| a <= 0.0)
                || stock.provisions <= 0.0
                || stock.fuel.is_some_and(|f| f <= 36.0);
            let recovered = stock.ammo.is_none_or(|a| a >= 0.5)
                && stock.provisions >= 60.0
                && stock.fuel.is_none_or(|f| f >= 90.0);
            if recovered {
                state.supply_waiting.remove(&u.id);
            }
            if critical {
                if !u.retreating && state.supply_waiting.insert(u.id) {
                    commands.push(json!({"t":"retreat","ids":[u.id]}));
                }
                continue;
            }
            if state.supply_waiting.contains(&u.id) {
                continue;
            }
        } else {
            state.supply_waiting.remove(&u.id);
        }
        available_units.push(u);
        if u.kind == "engineer" {
            available_engineers += 1;
        }
        if u.kind != "engineer"
            && !boolean(definition(u), "rail")
            && !(context.mode.is_skirmish() && u.build)
        {
            mine.push(u);
        }
    }
    let delivered_recent: Vec<_> = sightings
        .iter()
        .filter(|s| !allied(Some(s.owner)) && now - s.time <= 60.0)
        .collect();
    let strategic_recent = if context.mode.is_construction() && skill.adaptive {
        &delivered_recent
    } else {
        &remembered
    };
    let (research_commands, reserved, _research_mun) = research(observation, context)?;
    let tier_savings = if research_commands.is_empty() {
        reserved
    } else {
        0.0
    };
    commands.extend(research_commands);
    let mut budget = mirrored(observation, &commands)?.input().manpower - tier_savings;
    let mut newly_busy = BTreeSet::new();
    let adaptive = context.mode.is_construction() && skill.adaptive;
    let rushing: Vec<_> = strategic_recent
        .iter()
        .copied()
        .filter(|e| {
            adaptive
                && now < 240.0
                && now - e.time < 5.0
                && input.units.iter().any(|u| {
                    u.owner == input.slot
                        && boolean(definition(u), "building")
                        && dist(u.at, e.at) < 35.0
                })
        })
        .collect();
    let rush = !rushing.is_empty();
    let rush_at = rush.then(|| Point {
        x: rushing.iter().map(|s| s.at.x).sum::<f64>() / rushing.len() as f64,
        z: rushing.iter().map(|s| s.at.z).sum::<f64>() / rushing.len() as f64,
    });
    // Reserve endangered engineers for the shared escape order before creating new work.
    let construction_crew: Vec<_> = available_units
        .iter()
        .copied()
        .filter(|u| {
            !(world_policy
                && u.kind == "engineer"
                && rush_at.is_some_and(|at| dist(u.at, at) < 25.0))
        })
        .collect();
    let construction_view = mirrored(observation, &commands)?;
    let construction = if world_policy {
        crate::commander_world_policy::construction(
            &construction_view,
            &mirrored_units(&construction_view, &construction_crew),
            budget,
        )?
    } else {
        crate::commander_construction::plan(
            &construction_view,
            &mirrored_units(&construction_view, &available_units),
            budget,
            (adaptive || skill.memory) && strategic_recent.iter().any(|s| armor(&s.kind)),
            &sight_game,
            sees,
        )?
    };
    budget = construction.remaining_mp;
    let build_reserve = if rush { 0.0 } else { construction.reserve };
    mine.retain(|u| !construction.builders.contains(&u.id));
    available_units.retain(|u| !construction.builders.contains(&u.id));
    newly_busy.extend(construction.builders);
    commands.extend(construction.commands);
    let mut fort_reserve = 0.0;
    if !rush {
        let fortification_view = mirrored(observation, &commands)?;
        let fortification = crate::commander_fortification::plan_after_builds(
            &fortification_view,
            &mirrored_units(&fortification_view, &available_units),
            budget,
            build_reserve,
            tier_savings,
            &sight_game,
            sees,
            if world_policy {
                &mut state.world.fort_steps
            } else {
                &mut state.fort_steps
            },
            &commands,
        )?;
        budget = fortification.remaining_mp;
        fort_reserve = fortification.reserve;
        available_units.retain(|u| !fortification.builders.contains(&u.id));
        mine.retain(|u| !fortification.builders.contains(&u.id));
        newly_busy.extend(fortification.builders);
        commands.extend(fortification.commands);
    }
    state.field.armor_seen |= !armor_seen.is_empty() || enemies.iter().any(|u| armor(&u.kind));
    let mut field_input = mirrored(observation, &commands)?.input().clone();
    field_input.manpower = budget;
    let field_observation = Observation::new(field_input).map_err(|e| e.to_string())?;
    budget = crate::commander_field_policy::field_work(
        &field_observation,
        if world_policy {
            &mut state.world.field
        } else {
            &mut state.field
        },
        &mut newly_busy,
        |raw| {
            let accepted = crate::commander_field_policy::can_submit(&sight_game, &raw, sees);
            commands.push(raw);
            accepted
        },
    )?;
    let count = |kind: &str| mine.iter().filter(|u| u.kind == kind).count();
    let watched = |id| context.horde || state.mind.watched(id, now, skill.notice);
    let mut sit = Situation {
        kind: SituationKind::Push,
        until: now + skill.commit,
        point: None,
        threat: None,
        at: None,
        counter: None,
        safe: None,
        failed_at: 0,
    };
    if !context.horde {
        if let Some(old) = &state.situation
            && old.kind == SituationKind::Wait
            && now + 1e-9 >= old.until
            && let Some(id) = old.threat
        {
            state.released.insert(id);
        }
        let held = crate::sort::first_by(
            enemies
                .iter()
                .copied()
                .filter(|e| {
                    watched(e.id)
                        && !boolean(definition(e), "air")
                        && !boolean(definition(e), "structure")
                })
                .filter(|e| {
                    dist(e.at, me.spawn) <= 32.0
                        || input.points.iter().any(|p| {
                            allied(p.owner) && dist(e.at, p.at) <= observation.point_radius() + 4.0
                        })
                        || input.units.iter().any(|u| {
                            u.owner == input.slot
                                && ((u.kind == "depot" && dist(e.at, u.at) <= 24.0)
                                    || (u.kind == "hq" && dist(e.at, u.at) <= 32.0))
                        })
                }),
            |a, b| {
                crate::sort::then(
                    dist(a.at, me.spawn) - dist(b.at, me.spawn),
                    f64::from(a.id) - f64::from(b.id),
                )
            },
        );
        let holds = state.situation.as_ref().is_some_and(|op| {
            now + 1e-9 < op.until
                && match op.kind {
                    SituationKind::Wait => op
                        .threat
                        .is_some_and(|id| remembered.iter().any(|s| s.id == id)),
                    SituationKind::Defense => false,
                    SituationKind::Push => op.point.is_none_or(|i| {
                        input.points.get(i).is_some_and(|p| {
                            !(allied(p.owner) && !p.cut)
                                && failures.get(i).copied().unwrap_or(0) <= op.failed_at
                        })
                    }),
                }
        });
        if let Some(e) = held {
            sit.kind = SituationKind::Defense;
            sit.at = Some(e.at);
            sit.threat = Some(e.id);
            if let Some(old) = &state.situation
                && old.kind == sit.kind
                && old.threat == sit.threat
                && now + 1e-9 < old.until
            {
                sit.until = old.until;
            }
        } else if holds {
            sit = state.situation.clone().unwrap();
        } else {
            let answer: f64 = mine
                .iter()
                .filter(|u| anti(&u.kind))
                .map(|u| worth(u, definition(u)))
                .sum();
            let armor_value: f64 = armor_seen.iter().map(|s| s.worth).sum();
            let crowd: Vec<_> = remembered
                .iter()
                .filter(|s| catalog.unit(&s.kind).is_ok_and(|d| boolean(d, "infantry")))
                .collect();
            let foot: f64 = mine
                .iter()
                .filter(|u| boolean(definition(u), "infantry"))
                .map(|u| worth(u, definition(u)))
                .sum();
            if let Some(s) = armor_seen
                .iter()
                .find(|s| !state.released.contains(&s.id))
                .filter(|_| armor_value > answer + 1e-9)
            {
                sit.kind = SituationKind::Wait;
                sit.at = Some(s.at);
                sit.threat = Some(s.id);
                sit.counter = (count("at") < 2).then_some("at");
            } else if crowd.len() >= 4
                && crowd.iter().map(|s| s.worth).sum::<f64>() > foot + 1e-9
                && count("mg") == 0
                && !state.released.contains(&crowd[0].id)
            {
                let s = crowd[0];
                sit.kind = SituationKind::Wait;
                sit.at = Some(s.at);
                sit.threat = Some(s.id);
                sit.counter = Some("mg");
            }
        }
    }
    let point_of = |at: Point| {
        input
            .points
            .iter()
            .position(|p| dist(at, p.at) <= observation.point_radius())
    };
    let mut load = vec![0usize; input.points.len()];
    for u in &mine {
        if let Some(i) = point_of(u.path.last().copied().unwrap_or(u.at)) {
            load[i] += 1;
        }
    }
    if state.aim.is_some_and(|(i, until)| {
        state.mind.lost_at_point(i)
            || now >= until
            || input
                .points
                .get(i)
                .is_none_or(|p| allied(p.owner) && !p.cut)
    }) {
        state.aim = None;
    }
    let point_score = |u: &UnitObservation, i: usize, load: &[usize], loyal: bool| {
        let p = &input.points[i];
        dist(u.at, p.at) + load[i] as f64 * 40.0 - (p.vp - 1.0) * 25.0 - p.mp * 10.0
            + extra(i) * 40.0
            - if loyal { 18.0 } else { 0.0 }
    };
    let available = |i: usize| {
        let p = &input.points[i];
        !(allied(p.owner) && !p.cut)
            && !p.locked
            && !(context.defender && dist(p.at, me.spawn) > 70.0)
    };
    if let Some(scout) = mine
        .iter()
        .copied()
        .find(|u| !boolean(definition(u), "air") && !boolean(definition(u), "medic"))
        .or_else(|| mine.first().copied())
    {
        if sit.kind == SituationKind::Push && sit.point.is_none() {
            sit.point = least_score((0..input.points.len()).filter(|&i| available(i)), |i| {
                point_score(scout, i, &load, false)
            });
            if let Some(i) = sit.point {
                sit.at = Some(input.points[i].at);
                sit.failed_at = failures.get(i).copied().unwrap_or(0);
            }
        }
        if sit.kind == SituationKind::Wait
            && sit.safe.is_none_or(|i| {
                input.points.get(i).is_none_or(|p| {
                    (allied(p.owner) && !p.cut) || sit.at.is_some_and(|at| dist(p.at, at) <= 28.0)
                })
            })
        {
            sit.safe = least_score(
                (0..input.points.len()).filter(|&i| {
                    available(i) && sit.at.is_none_or(|at| dist(at, input.points[i].at) > 28.0)
                }),
                |i| point_score(scout, i, &load, false),
            );
        }
        if state.aim.is_none()
            && let Some(i) = if sit.kind == SituationKind::Wait {
                sit.safe
            } else {
                sit.point
            }
        {
            state.aim = Some((i, sit.until));
        }
    }
    let tactics_view = mirrored(observation, &commands)?;
    let mine = mirrored_units(&tactics_view, &mine);
    let available_units = mirrored_units(&tactics_view, &available_units);
    let adaptive_mine: Vec<_> = mine
        .iter()
        .copied()
        .filter(|u| !newly_busy.contains(&u.id))
        .collect();
    let adaptive_available: Vec<_> = available_units
        .iter()
        .copied()
        .filter(|u| !newly_busy.contains(&u.id))
        .collect();
    let adaptive = crate::commander_classic_rules::plan(
        &tactics_view,
        tactics_view.input().runtime.as_ref().unwrap(),
        &adaptive_mine,
        &adaptive_available,
        strategic_recent,
        &mut state.raid,
        skill,
    )?;
    let mut targets = crate::commander_classic_rules::targets(observation, context)?;
    let my_value: f64 = mine.iter().map(|u| worth(u, definition(u))).sum();
    let their_value: f64 = targets.first().and_then(|t| t.0).map_or(0.0, |owner| {
        strategic_recent
            .iter()
            .filter(|s| {
                now - s.time < 30.0 && input.players[s.owner].team == input.players[owner].team
            })
            .map(|s| s.worth)
            .sum()
    });
    let strong_enough = !context.mode.is_construction()
        || !skill.adaptive
        || my_value > 1.1 * context.caution * their_value;
    let shopping = crate::commander_shopping::plan(
        &tactics_view,
        &mine,
        strategic_recent,
        armor_seen.len(),
        &state.mind,
        sit.counter,
        budget,
        available_engineers,
        if adaptive.rush {
            (0.0, 0.0)
        } else {
            (
                math::max(context.build_reserve, build_reserve),
                math::max(context.fort_reserve, fort_reserve),
            )
        },
        level,
    )?;
    let buy = shopping.buy;
    commands.extend(
        shopping
            .commands
            .into_iter()
            .filter(|raw| !(world_policy && raw["t"] == "recover")),
    );
    budget = shopping.remaining_mp;
    if context.field.is_none() {
        let mut engineering_input = mirrored(observation, &commands)?.input().clone();
        engineering_input.manpower -= tier_savings;
        let engineering_view = Observation::new(engineering_input).map_err(|e| e.to_string())?;
        let repair_workers: BTreeSet<_> = mine.iter().map(|unit| unit.id).collect();
        let engineering = crate::commander_engineering::repairs(
            &engineering_view,
            &mut state.engineering,
            &input
                .units
                .iter()
                .filter(|unit| unit.owner == input.slot && !repair_workers.contains(&unit.id))
                .map(|unit| unit.id)
                .collect(),
            sees,
        )?;
        newly_busy.extend(engineering.busy);
        commands.extend(engineering.commands);
    }
    budget = math::min(
        budget,
        mirrored(observation, &commands)?.input().manpower - tier_savings,
    );
    let support_view = mirrored(observation, &commands)?;
    let support_context = support_view.input().runtime.as_ref().unwrap();
    let mut support = SupportBudget {
        mp: budget,
        mun: support_context.munitions,
        last: state.called,
        spent: false,
    };
    if !context.horde
        && mine.len() >= 6
        && let Some((i, p)) = input
            .points
            .iter()
            .enumerate()
            .find(|(_, p)| p.owner.is_some() && !allied(p.owner) && sees(p.at))
        && state.mind.point_ready(i, now, skill.notice)
        && let Some(command) = support.call("para", p.at, None, &sit, context, skill, now)?
    {
        commands.push(command);
    }
    let mut pending = vec![Vec::<&UnitObservation>::new(); input.points.len()];
    let heading = load.clone();
    let mut retreat = Vec::new();
    let mut moves = Vec::new();
    let mut attacks = adaptive.attacks;
    commands.extend(adaptive.commands);
    let mut armed = adaptive.busy.clone();
    let mut busy = adaptive.busy;
    busy.extend(newly_busy);
    let action_view = mirrored(observation, &commands)?;
    let action_workers: BTreeSet<_> = available_units.iter().map(|unit| unit.id).collect();
    let action = crate::commander_actions::plan(
        &action_view,
        &enemies
            .iter()
            .filter(|enemy| watched(enemy.id))
            .map(|enemy| enemy.id)
            .collect(),
        skill,
        &busy
            .iter()
            .copied()
            .chain(
                input
                    .units
                    .iter()
                    .filter(|unit| unit.owner == input.slot && !action_workers.contains(&unit.id))
                    .map(|unit| unit.id),
            )
            .collect(),
    )?;
    busy.extend(action.busy);
    commands.extend(action.commands);
    let ground_view = mirrored(observation, &commands)?;
    let mine = mirrored_units(&ground_view, &mine);
    support.mp = math::min(support.mp, ground_view.input().manpower - tier_savings);
    support.mun = math::min(
        support.mun,
        ground_view.input().runtime.as_ref().unwrap().munitions,
    );
    if let Some((kind, at)) = adaptive.support
        && let Some(command) = support.call(kind, at, None, &sit, context, skill, now)?
    {
        commands.push(command);
    }
    let mut hands = 0;
    let base = |u: &UnitObservation| {
        if context.mode.is_construction() {
            input.units.iter().any(|b| {
                allied(Some(b.owner))
                    && b.hp > 0.0
                    && boolean(definition(b), "produces")
                    && context.units.get(&b.id).is_some_and(|f| f.built >= 1.0)
                    && dist(u.at, b.at) <= num(catalog.table("CFG").unwrap(), "reinforceRadius")
            })
        } else if context.mode.is_skirmish() {
            input.units.iter().any(|b| {
                b.owner == input.slot
                    && b.kind == "hq"
                    && b.hp > 0.0
                    && context.units.get(&b.id).is_some_and(|f| f.built >= 1.0)
                    && dist(u.at, b.at) <= num(catalog.table("CFG").unwrap(), "reinforceRadius")
            })
        } else {
            dist(u.at, me.spawn) <= num(catalog.table("CFG").unwrap(), "reinforceRadius")
        }
    };
    // Depots are defended at every level; held points only at Hard.
    let guarded: Vec<_> = input
        .units
        .iter()
        .filter(|u| context.mode.is_construction() && u.owner == input.slot && u.kind == "depot")
        .map(|u| u.at)
        .chain(
            input
                .points
                .iter()
                .filter(|p| {
                    skill.guard && !context.mode.is_construction() && p.owner == Some(input.slot)
                })
                .map(|p| p.at),
        )
        .collect();
    for at in guarded {
        let foe_value: f64 = enemies
            .iter()
            .filter(|e| {
                dist(e.at, at) <= 24.0
                    && !boolean(definition(e), "structure")
                    && !boolean(definition(e), "air")
            })
            .map(|e| worth(e, definition(e)))
            .sum();
        if foe_value <= 0.0 {
            continue;
        }
        let mut free: Vec<_> = mine
            .iter()
            .copied()
            .filter(|u| {
                !boolean(definition(u), "air")
                    && !u.retreating
                    && !busy.contains(&u.id)
                    && (u.target_id == 0 || dist(u.at, at) < 30.0)
                    && dist(u.at, at) < 70.0
                    && worth(u, definition(u)) >= num(definition(u), "cost") * 0.4
                    && !hurt(u, definition(u), skill)
            })
            .collect();
        crate::sort::sort_by(&mut free, |a, b| dist(a.at, at) - dist(b.at, at));
        let mut go = Vec::new();
        let mut value = 0.0;
        for u in free {
            if value >= foe_value * 1.3 {
                break;
            }
            go.push(u);
            value += worth(u, definition(u));
        }
        if value < foe_value {
            continue;
        }
        for u in go {
            busy.insert(u.id);
            armed.insert(u.id);
            if dist(u.at, at) > 12.0
                && context
                    .units
                    .get(&u.id)
                    .and_then(|f| f.amove)
                    .is_none_or(|goal| dist(goal, at) > 6.0)
            {
                attacks.push((u.id, at));
            }
        }
    }
    // The prototype's front includes every ground army unit, even a busy one.
    let army: Vec<_> = mine
        .iter()
        .copied()
        .filter(|u| !boolean(definition(u), "air"))
        .collect();
    let front = if army.is_empty() {
        me.spawn
    } else {
        Point {
            x: army.iter().map(|u| u.at.x).sum::<f64>() / army.len() as f64,
            z: army.iter().map(|u| u.at.z).sum::<f64>() / army.len() as f64,
        }
    };
    let seek = catalog
        .table("CFG")
        .map_err(|e| e.to_string())?
        .get("aid")
        .ok_or("missing medic aid rules")?;
    let mut world_free = BTreeSet::new();
    for u in &mine {
        let d = definition(u);
        if boolean(d, "air") || u.riding || u.retreating || busy.contains(&u.id) {
            continue;
        }
        if !context.horde && !base(u) && hurt(u, d, skill) {
            retreat.push(u.id);
            continue;
        }
        if !context.horde && base(u) && u.hp < full(d) && (budget >= 20.0 || hurt(u, d, skill)) {
            continue;
        }
        // Chores precede the general busy check in shared/ai.js:673-681.
        if boolean(d, "medic") {
            if u.path.is_empty() && dist(u.at, front) > num(seek, "seek") * 0.8 {
                moves.push((u.id, front));
            }
            continue;
        }
        if d.get("w").is_some_and(|w| num(w, "shellTerrain") != 0.0)
            && u.target_id == 0
            && !u.fire_at
            && let Some(house) = enemies
                .iter()
                .find(|e| watched(e.id) && e.garrison && dist(u.at, e.at) < 60.0)
        {
            commands.push(json!({"t":"fireat","ids":[u.id],"x":house.at.x,"z":house.at.z}));
            continue;
        }
        if !idle(u) {
            continue;
        }
        if sit.kind == SituationKind::Defense || (!armor_seen.is_empty() && anti(&u.kind)) {
            let at = if sit.kind == SituationKind::Defense {
                sit.at
            } else {
                Some(armor_seen[0].at)
            };
            if hands < skill.hands
                && let Some(at) = at
            {
                hands += 1;
                attacks.push((u.id, at));
                armed.insert(u.id);
            }
            continue;
        }
        if world_policy {
            world_free.insert(u.id);
            continue;
        }
        if let Some(i) = point_of(u.at)
            && boolean(d, "infantry")
        {
            let first = mine
                .iter()
                .filter(|v| {
                    boolean(definition(v), "infantry") && point_of(v.at) == Some(i) && idle(v)
                })
                .map(|v| v.id)
                .min()
                == Some(u.id);
            if allied(input.points[i].owner) && first {
                if context.field.is_some() {
                    let mut holder_input = mirrored(observation, &commands)?.input().clone();
                    holder_input.manpower = budget;
                    let holder_observation =
                        Observation::new(holder_input).map_err(|e| e.to_string())?;
                    crate::commander_field_policy::holder(
                        &holder_observation,
                        &holder_observation.input().units[holder_observation
                            .input()
                            .units
                            .binary_search_by_key(&u.id, |unit| unit.id)
                            .unwrap()],
                        i,
                        state.field.armor_seen,
                        |raw| {
                            let accepted = if raw["t"] == "garrison" {
                                sees(Point {
                                    x: raw["x"].as_f64().unwrap_or(0.0),
                                    z: raw["z"].as_f64().unwrap_or(0.0),
                                })
                            } else {
                                crate::commander_field_policy::can_submit(&sight_game, &raw, sees)
                            };
                            if accepted && raw["t"] == "dig" && raw["kind"] == "mines" {
                                budget -= num(
                                    catalog.table("FORTS").unwrap().get("mines").unwrap(),
                                    "cost",
                                );
                            }
                            commands.push(raw);
                            accepted
                        },
                    )?;
                    busy.insert(u.id);
                } else {
                    let holder_view = mirrored(observation, &commands)?;
                    let holder_unit = &holder_view.input().units[holder_view
                        .input()
                        .units
                        .binary_search_by_key(&u.id, |unit| unit.id)
                        .unwrap()];
                    if let Some(command) = crate::commander_engineering::holder(
                        &holder_view,
                        holder_unit,
                        i,
                        state.armor_ever_seen,
                    )? {
                        commands.push(command);
                        busy.insert(u.id);
                    } else if !u.garrison
                        && !u.dig
                        && !u.entrench
                        && !matches!(u.cover, 1 | 2)
                        && !enemies
                            .iter()
                            .any(|e| dist(e.at, input.points[i].at) < 40.0)
                    {
                        commands.push(json!({"t":"cover","ids":[u.id]}));
                    }
                }
            }
            if !allied(input.points[i].owner) || first {
                continue;
            }
        }
        if mine.len() >= math::round(skill.base_army as f64 * context.caution) as usize
            && strong_enough
            && sit.kind == SituationKind::Push
            && !targets.is_empty()
        {
            if hands < skill.hands {
                crate::sort::sort_by(&mut targets, |a, b| dist(u.at, a.2) - dist(u.at, b.2));
                let target = targets[0];
                hands += 1;
                attacks.push((u.id, target.2));
                armed.insert(u.id);
            }
            continue;
        }
        let best = least_score(
            (0..input.points.len()).filter(|&i| {
                available(i)
                    && !(allied(input.points[i].owner)
                        && dist(u.at, input.points[i].at) <= observation.point_radius() + 16.0)
                    && (sit.kind != SituationKind::Wait
                        || sit.safe == Some(i)
                        || allied(input.points[i].owner) && input.points[i].cut)
            }),
            |i| point_score(u, i, &load, state.aim.is_some_and(|(aim, _)| aim == i)),
        );
        if let Some(i) = best {
            load[i] += 1;
            pending[i].push(u);
        }
    }
    if world_policy {
        commands.extend(crate::commander_world_policy::movement(
            observation,
            &mut state.world,
            level,
            &mine,
            &world_free,
            skill.hands.saturating_sub(hands),
        )?);
    }
    let mut pushed = false;
    for (i, group) in pending.into_iter().enumerate() {
        let p = &input.points[i];
        let enemy = p.owner.is_some() && !allied(p.owner);
        if enemy {
            if group.is_empty()
                || pushed
                || sit.kind == SituationKind::Defense
                || sit.kind == SituationKind::Wait && sit.safe != Some(i)
                || sit.kind == SituationKind::Push && sit.point.is_some_and(|point| point != i)
                || now < skill.first_assault
                || group.len() + heading[i] < skill.wave.min(mine.len())
            {
                continue;
            }
            let selected: BTreeSet<_> = group.iter().map(|u| u.id).collect();
            let there = mine.iter().copied().filter(|u| {
                !boolean(definition(u), "air")
                    && !u.retreating
                    && !selected.contains(&u.id)
                    && (dist(u.at, p.at) <= 25.0
                        || context
                            .units
                            .get(&u.id)
                            .and_then(|f| f.amove)
                            .into_iter()
                            .chain(u.path.last().copied())
                            .any(|at| dist(at, p.at) <= observation.point_radius() + 2.0))
            });
            let ours: f64 = group
                .iter()
                .copied()
                .chain(there)
                .map(|u| worth(u, definition(u)))
                .sum();
            let theirs: f64 = if skill.memory {
                remembered
                    .iter()
                    .filter(|e| now - e.time < 30.0 && dist(e.at, p.at) <= 25.0)
                    .map(|e| e.worth)
                    .sum()
            } else {
                enemies
                    .iter()
                    .filter(|e| dist(e.at, p.at) <= 25.0 && !boolean(definition(e), "structure"))
                    .map(|e| worth(e, definition(e)))
                    .sum()
            };
            if theirs > 1.5 * ours || ours < (skill.ratio + extra(i) * 0.45) * theirs {
                continue;
            }
            let centre = Point {
                x: group.iter().map(|u| u.at.x).sum::<f64>() / group.len() as f64,
                z: group.iter().map(|u| u.at.z).sum::<f64>() / group.len() as f64,
            };
            let smoke = Point {
                x: centre.x + (p.at.x - centre.x) * 0.6,
                z: centre.z + (p.at.z - centre.z) * 0.6,
            };
            let direction =
                math::atan2(p.at.z - centre.z, p.at.x - centre.x) + std::f64::consts::FRAC_PI_2;
            if let Some(command) =
                support.call("smoke", smoke, Some(direction), &sit, context, skill, now)?
            {
                commands.push(command);
            }
            pushed = true;
        }
        for u in group {
            if !enemy && !p.cut {
                if hands >= skill.hands {
                    break;
                }
                hands += 1;
            }
            let f = random.next_fraction(StreamId::Ai(input.slot as u32));
            let at = crate::commander_ai::spot_near(observation, p.at, f);
            if enemy || p.cut {
                attacks.push((u.id, at));
                if enemy {
                    armed.insert(u.id);
                }
            } else {
                moves.push((u.id, at));
            }
        }
    }
    for u in &mine {
        if !boolean(definition(u), "air")
            && !u.retreating
            && (context.horde
                || enemies
                    .iter()
                    .any(|e| watched(e.id) && dist(u.at, e.at) < 40.0))
        {
            armed.insert(u.id);
        }
    }
    armed.retain(|id| !context.units.get(id).is_some_and(|f| f.auto_retreat));
    if !armed.is_empty() {
        commands.push(json!({"t":"stance","ids":armed,"key":"autoRetreat","on":true}));
    }
    let safe = |order: &(u32, Point)| {
        let u = &input.units[input
            .units
            .binary_search_by_key(&order.0, |u| u.id)
            .unwrap()];
        anti(&u.kind) || !armor_seen.iter().any(|s| dist(order.1, s.at) <= 28.0)
    };
    moves.retain(safe);
    attacks.retain(safe);
    let mut operation_situation = sit.clone();
    if sit.kind == SituationKind::Push
        && sit.at.is_none()
        && strong_enough
        && mine.len() >= math::round(skill.base_army as f64 * context.caution) as usize
        && let Some(target) = targets.first()
    {
        operation_situation.at = Some(target.2);
    }
    let recovering = retreat.iter().copied().collect();
    let operation_observation = mirrored(observation, &commands)?;
    let live_sightings: Vec<_> = remembered.iter().map(|s| (*s).clone()).collect();
    let operation = if context.horde || world_policy {
        crate::commander_assault::Plan::default()
    } else {
        crate::commander_assault::plan(
            &operation_observation,
            operation_observation.input().runtime.as_ref().unwrap(),
            &state.mind,
            Some(&operation_situation),
            skill,
            now,
            &mut state.assault,
            crate::commander_assault::Options {
                busy: &busy,
                recovering: &recovering,
                sightings: &live_sightings,
                support_at: support.last,
                handoff: false,
                region: None,
            },
        )?
    };
    let controlled = state.assault.controlled(now);
    moves.retain(|(id, _)| !controlled.contains(id));
    attacks.retain(|(id, _)| !controlled.contains(id));
    retreat.retain(|id| !controlled.contains(id));
    if !retreat.is_empty() {
        commands.push(json!({"t":"retreat","ids":retreat}));
    }
    for (kind, orders) in [("move", moves), ("amove", attacks)] {
        if !orders.is_empty() {
            commands.push(json!({"t":kind,"orders":orders.into_iter().map(|(id,p)|json!([id,p.x,p.z])).collect::<Vec<_>>()}));
        }
    }
    commands.extend(operation.commands);
    state.called = support.last;
    state.decision = Some(Decision {
        situation: sit.kind,
        buy,
        aim: state.aim.map(|(i, _)| i).or(sit.point).or(sit.safe),
    });
    state.situation = Some(sit);
    Ok(commands)
}

struct SupportBudget {
    mp: f64,
    mun: f64,
    last: Option<f64>,
    spent: bool,
}
impl SupportBudget {
    #[allow(clippy::too_many_arguments)]
    fn call(
        &mut self,
        kind: &str,
        at: Point,
        direction: Option<f64>,
        situation: &Situation,
        context: &Context,
        skill: Skill,
        now: f64,
    ) -> Result<Option<Value>, String> {
        if self.spent
            || context.support.get(kind).is_none_or(|cd| *cd > 0.0)
            || self.last.is_some_and(|last| now - last < skill.sup_gap)
            || kind != "cover" && situation.at.is_some_and(|goal| dist(at, goal) > 40.0)
        {
            return Ok(None);
        }
        let price = economy::sup_cost(context.mode, kind).map_err(|e| e.to_string())?;
        let wallet = if price.currency == economy::Currency::Manpower {
            &mut self.mp
        } else {
            &mut self.mun
        };
        let reserve = if price.currency == economy::Currency::Manpower {
            skill.sup_reserve
        } else {
            skill.mun_reserve
        };
        if *wallet < price.cost + reserve {
            return Ok(None);
        }
        *wallet -= price.cost;
        self.last = Some(now);
        self.spent = true;
        let mut cmd = json!({"t":"support","kind":kind,"x":at.x,"z":at.z});
        if let Some(dir) = direction {
            cmd["dir"] = json!(dir);
        }
        Ok(Some(cmd))
    }
}

// This geometry world contains only delivered terrain and public smoke. It holds no live actors.
pub(crate) fn sight_game(observation: &Observation, context: &Context) -> crate::game::Game {
    let i = observation.input();
    crate::game::Game {
        w: i.width,
        h: i.height,
        flags: i.flags.clone(),
        height: context.height.clone(),
        house: vec![0; i.flags.len()],
        chars: i.chars.clone(),
        ground: None,
        objects: None,
        wear: None,
        cell_hp: vec![],
        trench_front: None,
        tough: None,
        wet: None,
        reveal: false,
        wrecks: vec![],
        tick: i.tick,
        vision_tick: None,
        terrain_version: 0,
        mode: None,
        naval: false,
        weather: None,
        rain: 0.0,
        players: vec![],
        units: BTreeMap::new(),
        smokes: context.smokes.clone(),
        strikes: vec![],
        sections: BTreeMap::new(),
    }
}

#[cfg(test)]
#[path = "commander_tactics_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "commander_tactics_strategy_tests.rs"]
mod strategy_tests;

#[cfg(test)]
#[path = "commander_chores_tests.rs"]
mod chores_tests;

#[cfg(test)]
#[path = "commander_batch7_tests.rs"]
mod batch7_tests;

#[cfg(test)]
#[path = "commander_shopping_inputs_tests.rs"]
mod shopping_inputs_tests;

#[cfg(test)]
#[path = "commander_tactics_regression_tests.rs"]
mod regression_tests;

#[cfg(test)]
#[path = "if25_tactics_tests.rs"]
mod if25_tests;

#[cfg(test)]
#[path = "commander_field_assault_tests.rs"]
mod field_assault_tests;

#[cfg(test)]
#[path = "commander_field_policy_tests.rs"]
mod field_policy_tests;

#[cfg(test)]
#[path = "commander_field_review_tests.rs"]
mod field_review_tests;
