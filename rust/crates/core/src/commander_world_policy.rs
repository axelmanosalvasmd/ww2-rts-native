//! World policy over delivered observations. This component does not admit World rooms.
use crate::{
    catalog::{Catalog, Value as Rule},
    commander_ai::{Level, Observation, Point, UnitObservation},
    commander_tactics::{Context, Skill},
    economy::Mode,
    game::{CELL, MOVE},
    math,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq)]
pub struct Region {
    pub id: u32,
    pub at: Point,
    pub team: Option<i32>,
    pub locked: bool,
    /// Delivered half-open cell intervals [row, first_column, end_column].
    pub runs: Option<Vec<[usize; 3]>>,
    /// Delivered half-open metre bounds [left, top, right, bottom].
    pub bounds: Option<[f64; 4]>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Station {
    pub region: u32,
    pub at: Point,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UnitFacts {
    pub cargo: usize,
    /// Destination of a delivered own boarding plan (prototype orderPlan kind 9).
    pub board_at: Option<Point>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Facts {
    pub regions: Vec<Region>,
    pub stations: Vec<Station>,
    pub units: BTreeMap<u32, UnitFacts>,
    /// Current visibility cells delivered to this seat, not explored-cell history.
    pub visible_cells: BTreeSet<usize>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct State {
    pub rail_at: BTreeMap<u32, i64>,
    pub fort_steps: BTreeMap<u32, u8>,
    pub field: crate::commander_field_policy::State,
}
fn distance(a: Point, b: Point) -> f64 {
    math::hypot(a.x - b.x, a.z - b.z)
}
fn flag(rule: &Rule, name: &str) -> bool {
    rule.get(name).and_then(Rule::boolean).unwrap_or(false)
}
fn number(rule: &Rule, name: &str) -> f64 {
    rule.get(name).and_then(Rule::number).unwrap_or(0.0)
}
fn inside(region: &Region, at: Point) -> bool {
    if let Some(runs) = &region.runs {
        let x = (at.x / CELL).floor();
        let z = (at.z / CELL).floor();
        return runs
            .iter()
            .any(|r| z == r[0] as f64 && x >= r[1] as f64 && x < r[2] as f64);
    }
    region.bounds.is_some_and(|[x, z, right, bottom]| {
        at.x >= x && at.z >= z && at.x < right && at.z < bottom
    })
}
pub fn owned_ground(facts: &Facts, team: i32, at: Point) -> bool {
    facts
        .regions
        .iter()
        .any(|r| r.team == Some(team) && inside(r, at))
}
fn idle(u: &UnitObservation, definition: &Rule) -> bool {
    !flag(definition, "air")
        && !flag(definition, "medic")
        && !u.retreating
        && !u.riding
        && !u.build
        && u.path.is_empty()
        && !u.attack_id
        && u.target_id == 0
        && !u.dig
        && !u.entrench
        && !u.enter
        && !u.fire_at
        && !u.nade
}

impl Facts {
    pub fn validate(&self, observation: &Observation) -> Result<(), String> {
        let input = observation.input();
        let within = |p: Point| {
            p.x.is_finite()
                && p.z.is_finite()
                && p.x >= 0.0
                && p.z >= 0.0
                && p.x <= input.width as f64 * CELL
                && p.z <= input.height as f64 * CELL
        };
        let mut regions = BTreeSet::new();
        if self.regions.iter().any(|r| {
            !regions.insert(r.id)
                || !within(r.at)
                || r.team.is_some_and(|t| !(0..=5).contains(&t))
                || r.runs.as_ref().is_some_and(|runs| {
                    runs.iter()
                        .any(|[z, x, end]| *z >= input.height || x >= end || *end > input.width)
                })
                || r.bounds.is_some_and(|[x, z, right, bottom]| {
                    !within(Point { x, z })
                        || !within(Point {
                            x: right,
                            z: bottom,
                        })
                        || x >= right
                        || z >= bottom
                })
        }) || self
            .stations
            .iter()
            .any(|s| !regions.contains(&s.region) || !within(s.at))
            || self
                .visible_cells
                .iter()
                .any(|c| *c >= input.flags.len() || input.chars[*c] == b'?')
        {
            return Err("invalid delivered World geography".into());
        }
        let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
        for (id, facts) in &self.units {
            let u = input
                .units
                .iter()
                .find(|u| u.id == *id && u.owner == input.slot)
                .ok_or("World private facts require an own delivered unit")?;
            let d = catalog.unit(&u.kind).map_err(|e| e.to_string())?;
            if facts.cargo > number(d, "carries") as usize
                || facts.board_at.is_some_and(|p| !within(p))
            {
                return Err("invalid delivered World carrier facts".into());
            }
        }
        Ok(())
    }
}

fn frontier(observation: &Observation, from: Point) -> Option<Point> {
    let input = observation.input();
    let mut best = None;
    let mut nearest = f64::INFINITY;
    for z in (1..input.height - 1).step_by(2) {
        for x in (1..input.width - 1).step_by(2) {
            let cell = z * input.width + x;
            if input.chars[cell] == b'?'
                || input.flags[cell] & MOVE != 0
                || ![cell - 1, cell + 1, cell - input.width, cell + input.width]
                    .iter()
                    .any(|c| input.chars[*c] == b'?')
            {
                continue;
            }
            let at = Point {
                x: (x as f64 + 0.5) * CELL,
                z: (z as f64 + 0.5) * CELL,
            };
            let d = distance(from, at);
            if d >= 3.0 && d < nearest {
                best = Some(at);
                nearest = d;
            }
        }
    }
    best
}

struct Economy {
    commands: Vec<Value>,
    builders: BTreeSet<u32>,
    budget: f64,
    reserve: f64,
}
fn economy(
    observation: &Observation,
    context: &Context,
    facts: &Facts,
    available: &[&UnitObservation],
    budget: f64,
) -> Result<Economy, String> {
    let input = observation.input();
    let team = input.players[input.slot].team;
    let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
    let definition = |kind: &str| catalog.unit(kind).map_err(|e| e.to_string());
    let own: Vec<_> = input
        .units
        .iter()
        .filter(|u| u.owner == input.slot && u.hp > 0.0)
        .collect();
    let built = |u: &UnitObservation| context.units.get(&u.id).map_or(0.0, |f| f.built);
    let has = |kind: &str, complete: bool| {
        own.iter()
            .any(|u| u.kind == kind && (!complete || built(u) >= 1.0))
    };
    let hq = own
        .iter()
        .find(|u| u.kind == "hq")
        .map_or(input.players[input.slot].spawn, |u| u.at);
    let mut nodes: Vec<_> = context
        .nodes
        .iter()
        .copied()
        .enumerate()
        .filter(|(_, at)| {
            owned_ground(facts, team, *at)
                && !input
                    .units
                    .iter()
                    .any(|u| u.hp > 0.0 && u.kind == "depot" && distance(u.at, *at) < 8.0)
                && !context.known_depots.iter().any(|p| distance(*p, *at) < 8.0)
        })
        .collect();
    nodes.sort_by(|(i, a), (j, b)| distance(hq, *a).total_cmp(&distance(hq, *b)).then(i.cmp(j)));
    let depots = own.iter().filter(|u| u.kind == "depot").count();
    let planes_near = input.units.iter().any(|u| {
        u.hp > 0.0
            && input.players[u.owner].team != team
            && distance(u.at, hq) < 60.0
            && definition(&u.kind).is_ok_and(|d| flag(d, "air"))
    });
    let mut next = if !has("hq", false) {
        Some("hq")
    } else if depots < 2 && !nodes.is_empty() {
        Some("depot")
    } else if !has("barracks", false) {
        Some("barracks")
    } else if has("barracks", true) && !has("motorpool", false) {
        Some("motorpool")
    } else if planes_near && own.iter().filter(|u| u.kind == "flakpos").count() < 2 {
        Some("flakpos")
    } else if !nodes.is_empty() {
        Some("depot")
    } else if has("motorpool", true) && !has("airfield", false) && depots >= 3 {
        Some("airfield")
    } else {
        None
    };
    if let Some(research) = &input.research {
        if next
            .is_some_and(|k| crate::economy::tier_of(k).unwrap_or(u8::MAX) > research.tech.tier())
        {
            next = (!nodes.is_empty()).then_some("depot");
        }
        if next.is_none() && research.tech.tier() >= 2 && !has("armory", false) {
            next = Some("armory");
        }
    }
    let cost = next
        .map(|k| definition(k).map(|d| number(d, "cost")))
        .transpose()?
        .unwrap_or(0.0);
    let mut plan = Economy {
        commands: vec![],
        builders: BTreeSet::new(),
        budget,
        reserve: if next == Some("depot") { 0.0 } else { cost },
    };
    let terrain = crate::commander_tactics::sight_game(observation, context);
    let sees = |at: Point| {
        terrain
            .cell_of(at.x, at.z)
            .is_some_and(|c| facts.visible_cells.contains(&c))
    };
    let mut claimed = BTreeSet::new();
    for worker in own.iter().copied().filter(|u| {
        u.kind == "engineer"
            && !u.retreating
            && !u.riding
            && !u.build
            && available.iter().any(|worker| worker.id == u.id)
    }) {
        let fix = own
            .iter()
            .copied()
            .filter(|u| {
                definition(&u.kind).is_ok_and(|d| {
                    flag(d, "building") && (built(u) < 1.0 || u.hp < number(d, "hpPer") * 0.7)
                }) && distance(worker.at, u.at) < 60.0
            })
            .min_by(|a, b| {
                distance(worker.at, a.at)
                    .total_cmp(&distance(worker.at, b.at))
                    .then(a.id.cmp(&b.id))
            });
        if let Some(fix) = fix {
            plan.commands
                .push(json!({"t":"assist","ids":[worker.id],"id":fix.id}));
            plan.builders.insert(worker.id);
            continue;
        }
        let Some(kind) = next else { continue };
        if kind == "depot" {
            let pick =
                nodes
                    .iter()
                    .filter(|(i, _)| !claimed.contains(i))
                    .min_by(|(i, a), (j, b)| {
                        distance(worker.at, *a)
                            .total_cmp(&distance(worker.at, *b))
                            .then(i.cmp(j))
                    });
            let Some((index, at)) = pick else { continue };
            claimed.insert(*index);
            if plan.budget >= cost && sees(*at) {
                plan.commands
                    .push(json!({"t":"build","ids":[worker.id],"kind":kind,"x":at.x,"z":at.z}));
                plan.budget -= cost;
            } else if worker.path.is_empty() {
                plan.commands
                    .push(json!({"t":"move","orders":[[worker.id,at.x,at.z]]}));
            } else {
                continue;
            }
            plan.builders.insert(worker.id);
        } else if plan.budget >= cost {
            let d = definition(kind)?;
            let angle = math::atan2(
                input.height as f64 * CELL / 2.0 - hq.z,
                input.width as f64 * CELL / 2.0 - hq.x,
            ) + match kind {
                "barracks" => 0.9,
                "motorpool" => -0.9,
                "airfield" => std::f64::consts::PI,
                _ => 0.0,
            };
            let aim = Point {
                x: hq.x + math::cos(angle) * 16.0,
                z: hq.z + math::sin(angle) * 16.0,
            };
            let at = crate::commander_construction::site(
                observation,
                &terrain,
                aim,
                number(d, "size") as usize,
                flag(d, "coast"),
                sees,
            )?;
            if let Some(at) = at.filter(|at| owned_ground(facts, team, *at)) {
                plan.commands
                    .push(json!({"t":"build","ids":[worker.id],"kind":kind,"x":at.x,"z":at.z}));
                plan.builders.insert(worker.id);
                plan.budget -= cost;
                // One accepted site per building kind in a look; it is not yet in the observation.
                break;
            }
        }
    }
    Ok(plan)
}

/// Plan from the same detached context used by the ordinary commander. No World caller is admitted.
pub fn plan(
    observation: &Observation,
    state: &mut State,
    level: Level,
) -> Result<Vec<Value>, String> {
    let input = observation.input();
    let context = input.runtime.as_ref().ok_or("missing World context")?;
    if context.mode != Mode::World {
        return Err("World policy requires World context".into());
    }
    let facts = context
        .world
        .as_ref()
        .ok_or("missing delivered World facts")?;
    facts.validate(observation)?;
    if input.players[input.slot].out {
        return Ok(vec![]);
    }
    let mut shared = crate::commander_tactics::State {
        world: state.clone(),
        ..crate::commander_tactics::State::default()
    };
    let result = crate::commander_tactics::plan(
        observation,
        &[],
        &mut shared,
        level,
        &mut crate::random::MatchRandom::new(0),
    )?;
    *state = shared.world;
    Ok(result)
}

/// World construction replaces only the construction stage of the shared commander.
pub(crate) fn construction(
    observation: &Observation,
    available: &[&UnitObservation],
    budget: f64,
) -> Result<crate::commander_construction::Plan, String> {
    let context = observation
        .input()
        .runtime
        .as_ref()
        .ok_or("missing World context")?;
    let facts = context
        .world
        .as_ref()
        .ok_or("missing delivered World facts")?;
    facts.validate(observation)?;
    let result = economy(observation, context, facts, available, budget)?;
    Ok(crate::commander_construction::Plan {
        commands: result.commands,
        remaining_mp: result.budget,
        reserve: result.reserve,
        builders: result.builders.into_iter().collect(),
    })
}

/// Run after shared adaptive, defensive and wounded-unit decisions have claimed their actors.
pub(crate) fn movement(
    observation: &Observation,
    state: &mut State,
    level: Level,
    mine: &[&UnitObservation],
    available: &BTreeSet<u32>,
    mut hands: usize,
) -> Result<Vec<Value>, String> {
    let input = observation.input();
    let context = input.runtime.as_ref().ok_or("missing World context")?;
    let facts = context
        .world
        .as_ref()
        .ok_or("missing delivered World facts")?;
    facts.validate(observation)?;
    let team = input.players[input.slot].team;
    let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
    let definition = |u: &UnitObservation| {
        catalog
            .unit(&u.kind)
            .expect("observation validated unit definition")
    };
    let skill = Skill::for_level(level);
    let mut commands = Vec::new();
    let regions: Vec<_> = facts
        .regions
        .iter()
        .filter(|r| r.team != Some(team))
        .collect();
    let mut stations: Vec<_> = facts
        .stations
        .iter()
        .filter(|s| {
            facts
                .regions
                .iter()
                .any(|r| r.id == s.region && r.team == Some(team))
        })
        .collect();
    stations.sort_by(|a, b| {
        a.region
            .cmp(&b.region)
            .then(a.at.z.total_cmp(&b.at.z))
            .then(a.at.x.total_cmp(&b.at.x))
    });
    let mut railed = BTreeSet::new();
    let mut boarding: BTreeMap<u32, usize> = BTreeMap::new();
    let cargo = |u: &UnitObservation| facts.units.get(&u.id).map_or(0, |f| f.cargo);
    for u in mine {
        let d = definition(u);
        if !idle(u, d) || !available.contains(&u.id) || railed.contains(&u.id) {
            continue;
        }
        let target = regions
            .iter()
            .copied()
            .filter(|r| !r.locked || mine.len() >= skill.wave)
            .min_by(|a, b| {
                distance(u.at, a.at)
                    .total_cmp(&distance(u.at, b.at))
                    .then(a.id.cmp(&b.id))
            });
        if let Some(to) = target
            && !flag(d, "naval")
            && distance(u.at, to.at) >= 250.0
            && let Some(at) = stations
                .iter()
                .copied()
                .find(|s| distance(s.at, u.at) <= 30.0)
            && let Some(end) = stations
                .iter()
                .copied()
                .filter(|s| !std::ptr::eq(*s, at))
                .min_by(|a, b| {
                    distance(a.at, to.at)
                        .total_cmp(&distance(b.at, to.at))
                        .then(a.region.cmp(&b.region))
                })
            && distance(end.at, to.at) <= distance(u.at, to.at) - 200.0
            && state
                .rail_at
                .get(&at.region)
                .is_none_or(|last| *last <= input.tick - 400)
            && hands > 0
        {
            let ids: Vec<_> = mine
                .iter()
                .filter(|v| {
                    idle(v, definition(v))
                        && available.contains(&v.id)
                        && !railed.contains(&v.id)
                        && distance(v.at, at.at) <= 30.0
                })
                .take(8)
                .map(|v| v.id)
                .collect();
            railed.extend(ids.iter().copied());
            state.rail_at.insert(at.region, input.tick);
            hands -= 1;
            commands.push(json!({"t":"rail","ids":ids,"x":end.at.x,"z":end.at.z}));
            continue;
        }
        let goal = target.map(|r| r.at).or_else(|| frontier(observation, u.at));
        let capacity = number(d, "carries") as usize;
        let seats = capacity.saturating_sub(cargo(u) + boarding.get(&u.id).copied().unwrap_or(0));
        let awaited = mine.iter().any(|v| {
            facts
                .units
                .get(&v.id)
                .and_then(|f| f.board_at)
                .is_some_and(|at| distance(at, u.at) < 3.0)
        });
        let far = |v: &UnitObservation| {
            regions.iter().any(|r| distance(v.at, r.at) > 220.0)
                && !regions.iter().any(|r| distance(v.at, r.at) <= 220.0)
        };
        if capacity > 0
            && seats > 0
            && (boarding.contains_key(&u.id)
                || awaited
                || mine.iter().any(|v| {
                    v.id != u.id
                        && flag(definition(v), "infantry")
                        && idle(v, definition(v))
                        && available.contains(&v.id)
                        && distance(v.at, u.at) < 30.0
                        && far(v)
                }))
        {
            continue;
        }
        let mut command = None;
        if capacity > 0 && cargo(u) > 0 {
            if goal.is_none_or(|at| distance(u.at, at) < 60.0)
                || input.units.iter().any(|e| {
                    e.hp > 0.0
                        && input.players[e.owner].team != team
                        && distance(e.at, u.at) <= 45.0
                })
            {
                command = Some(json!({"t":"unload","ids":[u.id]}));
            } else if let Some(at) = goal {
                command = Some(json!({"t":"amove","orders":[[u.id,at.x,at.z]]}));
            }
        } else if u.kind == "lorry" {
            continue;
        } else {
            let ride = goal
                .filter(|at| flag(d, "infantry") && distance(u.at, *at) > 220.0)
                .and_then(|to| {
                    mine.iter().copied().find(|c| {
                        let cd = definition(c);
                        c.id != u.id
                            && number(cd, "carries") as usize
                                > cargo(c) + boarding.get(&c.id).copied().unwrap_or(0)
                            && !flag(cd, "naval")
                            && available.contains(&c.id)
                            && !railed.contains(&c.id)
                            && distance(c.at, u.at) < 30.0
                            && c.path.is_empty()
                            && c.target_id == 0
                            && distance(c.at, to) < distance(u.at, to) + 40.0
                    })
                });
            if let Some(ride) = ride {
                if hands > 0 {
                    *boarding.entry(ride.id).or_default() += 1;
                }
                command = Some(json!({"t":"board","ids":[u.id],"target":ride.id}));
            } else if let Some(target) = target.filter(|r| r.locked || flag(d, "infantry")) {
                let base = target
                    .locked
                    .then(|| {
                        context
                            .known_buildings
                            .iter()
                            .filter(|b| {
                                input.players[b.owner].team != team
                                    && inside(target, b.at)
                                    && catalog.unit(&b.kind).is_ok_and(|d| flag(d, "produces"))
                            })
                            .min_by(|a, b| {
                                distance(u.at, a.at)
                                    .total_cmp(&distance(u.at, b.at))
                                    .then(a.id.cmp(&b.id))
                            })
                    })
                    .flatten();
                if let Some(base) = base {
                    command = Some(
                        if input.units.iter().any(|e| e.id == base.id && e.hp > 0.0) {
                            json!({"t":"attack","ids":[u.id],"target":base.id})
                        } else {
                            json!({"t":"amove","orders":[[u.id,base.at.x,base.at.z]]})
                        },
                    );
                } else if distance(u.at, target.at) > observation.point_radius() - 2.0 {
                    command = Some(json!({"t":"amove","orders":[[u.id,target.at.x,target.at.z]]}));
                }
            } else if let Some(at) = frontier(observation, u.at) {
                command = Some(json!({"t":"amove","orders":[[u.id,at.x,at.z]]}));
            }
        }
        if let Some(command) = command
            && hands > 0
        {
            hands -= 1;
            commands.push(command);
        }
    }
    Ok(commands)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        commander_ai::{Input, PlayerObservation},
        commander_tactics::{KnownBuilding, UnitFacts as RuntimeFacts},
    };
    fn unit(id: u32, kind: &str, x: f64, z: f64) -> UnitObservation {
        let d = Catalog::compiled().unwrap().unit(kind).unwrap();
        UnitObservation {
            id,
            kind: kind.into(),
            owner: 0,
            at: Point { x, z },
            hp: number(d, "models") * number(d, "hpPer"),
            supp: 0.0,
            target_id: 0,
            cover: 0,
            path: vec![],
            attack_id: false,
            retreating: false,
            riding: false,
            build: false,
            dig: false,
            entrench: false,
            garrison: false,
            enter: false,
            fire_at: false,
            nade: false,
        }
    }
    fn region(id: u32, team: Option<i32>, x: f64, locked: bool, bounds: [f64; 4]) -> Region {
        Region {
            id,
            team,
            at: Point { x, z: 80.0 },
            locked,
            runs: None,
            bounds: Some(bounds),
        }
    }
    fn fixture(units: Vec<UnitObservation>) -> Input {
        let facts = Facts {
            regions: vec![
                region(1, Some(0), 50.0, false, [0.0, 0.0, 100.0, 400.0]),
                region(2, Some(1), 600.0, false, [500.0, 0.0, 800.0, 400.0]),
            ],
            visible_cells: (0..80_000).collect(),
            ..Facts::default()
        };
        Input {
            width: 400,
            height: 200,
            tick: 1000,
            slot: 0,
            players: vec![
                PlayerObservation {
                    team: 0,
                    out: false,
                    spawn: Point { x: 20.0, z: 80.0 },
                },
                PlayerObservation {
                    team: 1,
                    out: false,
                    spawn: Point { x: 760.0, z: 80.0 },
                },
            ],
            units,
            points: vec![],
            flags: vec![0; 80_000],
            chars: vec![b'.'; 80_000],
            manpower: 0.0,
            research: None,
            runtime: Some(Context {
                mode: Mode::World,
                world: Some(facts),
                ..Context::default()
            }),
        }
    }
    fn world(input: &mut Input) -> &mut Facts {
        input.runtime.as_mut().unwrap().world.as_mut().unwrap()
    }
    fn propose(input: Input, state: &mut State) -> Vec<Value> {
        plan(&Observation::new(input).unwrap(), state, Level::Normal).unwrap()
    }
    fn orders(commands: &[Value], kind: &str) -> Vec<Value> {
        commands
            .iter()
            .filter(|c| c["t"] == kind)
            .cloned()
            .collect()
    }
    fn motion(commands: &[Value]) -> Vec<Value> {
        commands
            .iter()
            .filter(|c| {
                matches!(
                    c["t"].as_str(),
                    Some("rail" | "board" | "unload" | "attack" | "amove")
                )
            })
            .cloned()
            .collect()
    }
    fn amove(id: u32, x: f64, z: f64) -> Value {
        json!({"t":"amove","orders":[[id,x,z]]})
    }

    #[test]
    fn intent_ai58_recovery_repeats_until_both_engineer_and_hq_are_delivered() {
        for missing in ["hq", "engineer", "both"] {
            let mut units = vec![unit(1, "rifle", 40.0, 80.0)];
            if missing != "engineer" && missing != "both" {
                units.push(unit(2, "engineer", 30.0, 80.0));
            }
            if missing != "hq" && missing != "both" {
                units.push(unit(3, "hq", 20.0, 80.0));
            }
            let input = fixture(units);
            let mut state = State::default();
            for _ in 0..2 {
                assert_eq!(
                    orders(&propose(input.clone(), &mut state), "recover"),
                    vec![json!({"t":"recover"})],
                    "{missing}"
                );
            }
        }
        let input = fixture(vec![
            unit(1, "engineer", 30.0, 80.0),
            unit(2, "hq", 20.0, 80.0),
        ]);
        assert!(orders(&propose(input, &mut State::default()), "recover").is_empty());
    }

    #[test]
    fn intent_ai58_hq_precedes_depot_and_builds_only_on_visible_owned_ground() {
        let mut input = fixture(vec![unit(1, "engineer", 20.0, 80.0)]);
        input.manpower = 500.0;
        input
            .runtime
            .as_mut()
            .unwrap()
            .nodes
            .push(Point { x: 60.0, z: 100.0 });
        let commands = propose(input.clone(), &mut State::default());
        let builds = orders(&commands, "build");
        assert_eq!(builds.len(), 1);
        assert_eq!(builds[0]["kind"], "hq");
        let at = Point {
            x: builds[0]["x"].as_f64().unwrap(),
            z: builds[0]["z"].as_f64().unwrap(),
        };
        assert!(owned_ground(world(&mut input), 0, at));
        world(&mut input).regions[0].team = Some(1);
        assert!(orders(&propose(input.clone(), &mut State::default()), "build").is_empty());
        world(&mut input).regions[0].team = Some(0);
        world(&mut input).visible_cells.clear();
        assert!(orders(&propose(input, &mut State::default()), "build").is_empty());
    }

    #[test]
    fn intent_ai58_region_runs_override_bounds_and_later_sites_remain_owned() {
        let mut input = fixture(vec![
            unit(1, "engineer", 25.0, 80.0),
            unit(2, "hq", 20.0, 80.0),
        ]);
        input.manpower = 500.0;
        input.runtime.as_mut().unwrap().units.insert(
            2,
            RuntimeFacts {
                built: 1.0,
                ..RuntimeFacts::default()
            },
        );
        let builds = orders(&propose(input.clone(), &mut State::default()), "build");
        assert_eq!(builds[0]["kind"], "barracks");
        let at = Point {
            x: builds[0]["x"].as_f64().unwrap(),
            z: builds[0]["z"].as_f64().unwrap(),
        };
        assert!(owned_ground(world(&mut input), 0, at));
        world(&mut input).regions[0].runs = Some(vec![[0, 0, 1]]);
        assert!(!owned_ground(world(&mut input), 0, at));
        assert!(orders(&propose(input, &mut State::default()), "build").is_empty());
    }

    #[test]
    fn intent_ai58_nearest_regions_require_wave_for_locked_targets_and_use_known_bases() {
        let mut input = fixture(vec![unit(1, "rifle", 50.0, 80.0)]);
        world(&mut input)
            .regions
            .push(region(3, Some(1), 150.0, true, [100.0, 0.0, 200.0, 400.0]));
        assert_eq!(
            motion(&propose(input.clone(), &mut State::default())),
            vec![amove(1, 600.0, 80.0)]
        );
        input
            .units
            .extend([unit(2, "rifle", 50.0, 85.0), unit(3, "rifle", 50.0, 90.0)]);
        let commands = propose(input.clone(), &mut State::default());
        assert_eq!(motion(&commands)[0], amove(1, 150.0, 80.0));
        input.runtime.as_mut().unwrap().known_buildings = vec![KnownBuilding {
            id: 10,
            kind: "worldbase".into(),
            owner: 1,
            at: Point { x: 170.0, z: 80.0 },
        }];
        assert_eq!(
            motion(&propose(input.clone(), &mut State::default()))[0],
            amove(1, 170.0, 80.0)
        );
        let mut visible = unit(10, "worldbase", 170.0, 80.0);
        visible.owner = 1;
        input.units.push(visible);
        assert_eq!(
            motion(&propose(input, &mut State::default()))[0],
            json!({"t":"attack","ids":[1],"target":10})
        );
    }

    #[test]
    fn intent_ai58_frontier_is_explored_passable_and_adjacent_to_unknown_ground() {
        let mut input = fixture(vec![unit(1, "rifle", 10.0, 10.0)]);
        world(&mut input).regions.retain(|r| r.team == Some(0));
        let (x, z) = (15, 15);
        input.chars[z * input.width + x + 1] = b'?';
        world(&mut input).visible_cells.remove(&(z * 400 + x + 1));
        assert_eq!(
            motion(&propose(input.clone(), &mut State::default())),
            vec![amove(1, 31.0, 31.0)]
        );
        input.flags[z * input.width + x] = MOVE;
        input.flags[z * input.width + x + 2] = MOVE;
        assert!(motion(&propose(input, &mut State::default())).is_empty());
    }

    fn rails(input: &mut Input) {
        world(input)
            .regions
            .push(region(3, Some(0), 400.0, false, [300.0, 0.0, 500.0, 400.0]));
        world(input).stations = vec![
            Station {
                region: 3,
                at: Point { x: 400.0, z: 80.0 },
            },
            Station {
                region: 1,
                at: Point { x: 50.0, z: 80.0 },
            },
        ];
    }
    #[test]
    fn intent_ai58_rail_carries_eight_idle_units_and_obeys_twenty_second_station_cooldown() {
        let mut input = fixture((1..=10).map(|id| unit(id, "rifle", 50.0, 80.0)).collect());
        rails(&mut input);
        let mut state = State::default();
        assert_eq!(
            orders(&propose(input.clone(), &mut state), "rail"),
            vec![json!({"t":"rail","ids":[1,2,3,4,5,6,7,8],"x":400.0,"z":80.0})]
        );
        input.tick += 399;
        assert!(orders(&propose(input.clone(), &mut state), "rail").is_empty());
        input.tick += 1;
        assert_eq!(orders(&propose(input, &mut state), "rail").len(), 1);
    }

    #[test]
    fn intent_ai58_rail_rejects_foreign_stations_short_goals_and_small_distance_savings() {
        for case in 0..5 {
            let mut input = fixture(vec![unit(1, "rifle", 50.0, 80.0)]);
            rails(&mut input);
            match case {
                0 => world(&mut input).regions[2].team = Some(1),
                1 => world(&mut input).regions[1].at.x = 299.99,
                2 => world(&mut input).stations[0].at.x = 249.99,
                3 => input.units[0].at.z = 110.001,
                _ => {
                    input.units[0] = unit(1, "gunboat", 50.0, 80.0);
                }
            }
            assert!(
                orders(&propose(input, &mut State::default()), "rail").is_empty(),
                "case {case}"
            );
        }
    }

    #[test]
    fn intent_ai58_boarding_reserves_seats_and_carrier_waits_for_nearby_or_inbound_squads() {
        let mut input = fixture(vec![
            unit(1, "tank", 50.0, 80.0),
            unit(2, "rifle", 50.0, 80.0),
            unit(3, "rifle", 50.0, 80.0),
            unit(4, "rifle", 50.0, 80.0),
        ]);
        let commands = propose(input.clone(), &mut State::default());
        assert_eq!(
            orders(&commands, "board"),
            vec![
                json!({"t":"board","ids":[2],"target":1}),
                json!({"t":"board","ids":[3],"target":1})
            ]
        );
        assert!(!motion(&commands).contains(&amove(1, 600.0, 80.0)));
        assert!(motion(&commands).contains(&amove(4, 600.0, 80.0)));
        input.units.truncate(2);
        input.units[1].path = vec![Point { x: 51.0, z: 80.0 }];
        world(&mut input).units.insert(
            1,
            UnitFacts {
                cargo: 1,
                board_at: None,
            },
        );
        world(&mut input).units.insert(
            2,
            UnitFacts {
                cargo: 0,
                board_at: Some(Point { x: 52.9, z: 80.0 }),
            },
        );
        assert!(motion(&propose(input.clone(), &mut State::default())).is_empty());
        world(&mut input).units.get_mut(&2).unwrap().board_at = None;
        assert_eq!(
            motion(&propose(input, &mut State::default())),
            vec![amove(1, 600.0, 80.0)]
        );
    }

    #[test]
    fn intent_ai58_loaded_carriers_march_then_unload_near_goal_or_visible_enemy() {
        let mut input = fixture(vec![unit(1, "lorry", 50.0, 80.0)]);
        input.chars[15 * 400 + 16] = b'?';
        world(&mut input).visible_cells.remove(&(15 * 400 + 16));
        assert!(motion(&propose(input.clone(), &mut State::default())).is_empty());
        world(&mut input).units.insert(
            1,
            UnitFacts {
                cargo: 1,
                board_at: None,
            },
        );
        assert_eq!(
            motion(&propose(input.clone(), &mut State::default())),
            vec![amove(1, 600.0, 80.0)]
        );
        input.units[0].at.x = 540.001;
        assert_eq!(
            orders(&propose(input.clone(), &mut State::default()), "unload"),
            vec![json!({"t":"unload","ids":[1]})]
        );
        input.units[0].at.x = 50.0;
        let mut enemy = unit(2, "rifle", 50.0, 125.0);
        enemy.owner = 1;
        input.units.push(enemy);
        assert_eq!(
            orders(&propose(input.clone(), &mut State::default()), "unload").len(),
            1
        );
        input.units[1].at.z = 125.001;
        assert!(orders(&propose(input.clone(), &mut State::default()), "unload").is_empty());
        input.units.pop();
        input.chars[15 * 400 + 16] = b'.';
        world(&mut input).regions.retain(|r| r.team == Some(0));
        assert_eq!(
            orders(&propose(input, &mut State::default()), "unload").len(),
            1
        );
    }

    #[test]
    fn intent_ai58_short_goal_and_busy_foreign_or_full_carriers_do_not_take_squads() {
        for case in 0..7 {
            let mut input = fixture(vec![
                unit(1, "tank", 50.0, 80.0),
                unit(2, "rifle", 50.0, 80.0),
            ]);
            match case {
                0 => world(&mut input).regions[1].at.x = 270.0,
                1 => input.units[0].path.push(Point { x: 51.0, z: 80.0 }),
                2 => input.units[0].target_id = 100,
                3 => input.units[0].owner = 1,
                4 => {
                    world(&mut input).units.insert(
                        1,
                        UnitFacts {
                            cargo: 2,
                            board_at: None,
                        },
                    );
                }
                5 => input.units[0].at.z = 110.0,
                _ => input.units[1].riding = true,
            }
            assert!(
                orders(&propose(input, &mut State::default()), "board").is_empty(),
                "case {case}"
            );
        }
    }

    #[test]
    fn intent_ai58_world_policy_keeps_lorry_purchase_priority_over_faction_infantry() {
        let kinds = [
            "engineer",
            "engineer",
            "hq",
            "barracks",
            "motorpool",
            "tank",
            "tank",
            "mortar",
            "howitzer",
            "medic",
            "armoredcar",
            "rifle",
            "rifle",
            "rifle",
            "mg",
            "mg",
            "mg",
        ];
        let mut input = fixture(
            kinds
                .iter()
                .enumerate()
                .map(|(i, k)| unit(i as u32 + 1, k, 50.0, 80.0))
                .collect(),
        );
        input.manpower = 2000.0;
        let context = input.runtime.as_mut().unwrap();
        context.fuel = 2000.0;
        context.units = input
            .units
            .iter()
            .map(|u| {
                (
                    u.id,
                    RuntimeFacts {
                        built: 1.0,
                        ..RuntimeFacts::default()
                    },
                )
            })
            .collect();
        assert_eq!(
            orders(&propose(input, &mut State::default()), "buy"),
            vec![json!({"t":"buy","unit":"lorry"})]
        );
    }

    #[test]
    fn intent_ai50_world_hq_fortification_is_reached_through_world_commander() {
        let mut input = fixture(vec![
            unit(1, "engineer", 55.0, 70.0),
            unit(10, "hq", 60.0, 60.0),
            unit(11, "barracks", 20.0, 20.0),
            unit(12, "motorpool", 60.0, 20.0),
        ]);
        input.tick = 3000;
        input.manpower = 1000.0;
        input.runtime.as_mut().unwrap().units = input
            .units
            .iter()
            .map(|u| {
                (
                    u.id,
                    RuntimeFacts {
                        built: 1.0,
                        ..RuntimeFacts::default()
                    },
                )
            })
            .collect();
        let observation = Observation::new(input).unwrap();
        let mut state = crate::commander_tactics::State::default();
        let commands = crate::commander_tactics::plan(
            &observation,
            &[],
            &mut state,
            Level::Normal,
            &mut crate::random::MatchRandom::new(7),
        )
        .unwrap();
        assert!(
            orders(&commands, "build")
                .iter()
                .any(|c| c["kind"] == "tower")
        );
        assert_eq!(state.world.fort_steps.get(&10), Some(&1));
    }

    #[test]
    fn intent_ai48_world_field_jobs_use_delivered_neutral_ground_and_keep_their_workers() {
        let mut input = fixture(vec![unit(1, "rifle", 100.0, 80.0)]);
        input.manpower = 500.0;
        let spot = Point { x: 120.0, z: 80.0 };
        assert!(!owned_ground(world(&mut input), 0, spot));
        input.runtime.as_mut().unwrap().field = Some(crate::commander_field_policy::Facts {
            initial_chars: input.chars.clone(),
            ground: vec![b'.'; input.chars.len()],
            found_mines: vec![spot],
            ..crate::commander_field_policy::Facts::default()
        });
        let observation = Observation::new(input.clone()).unwrap();
        let mut state = crate::commander_tactics::State::default();
        let commands = crate::commander_tactics::plan(
            &observation,
            &[],
            &mut state,
            Level::Normal,
            &mut crate::random::MatchRandom::new(7),
        )
        .unwrap();
        assert_eq!(
            orders(&commands, "dig"),
            vec![json!({
                "t": "dig", "ids": [1], "kind": "demine",
                "x": 120.0, "z": 80.0, "dir": 0.0
            })]
        );
        assert!(motion(&commands).is_empty());
        assert_ne!(
            state.world.field,
            crate::commander_field_policy::State::default()
        );
        world(&mut input).visible_cells.remove(&(40 * 400 + 60));
        let mut hidden_state = State::default();
        let denied = propose(input, &mut hidden_state);
        assert_eq!(
            hidden_state.field,
            crate::commander_field_policy::State::default()
        );
        assert_eq!(
            orders(&denied, "move"),
            vec![json!({"t":"move", "orders":[[1,111.0,80.0]]})]
        );
        assert!(motion(&denied).is_empty());
    }

    #[test]
    fn intent_ai48_world_trains_leave_the_new_field_worker_at_its_job() {
        let mut input = fixture(vec![
            unit(1, "rifle", 50.0, 80.0),
            unit(2, "rifle", 50.0, 80.0),
        ]);
        input.manpower = 500.0;
        rails(&mut input);
        input.runtime.as_mut().unwrap().field = Some(crate::commander_field_policy::Facts {
            initial_chars: input.chars.clone(),
            ground: vec![b'.'; input.chars.len()],
            found_mines: vec![Point { x: 70.0, z: 80.0 }],
            ..crate::commander_field_policy::Facts::default()
        });
        let commands = propose(input, &mut State::default());
        assert_eq!(orders(&commands, "dig")[0]["ids"], json!([1]));
        assert_eq!(
            orders(&commands, "rail"),
            vec![json!({"t":"rail", "ids":[2], "x":400.0, "z":80.0})]
        );
    }

    #[test]
    fn intent_ai58_invalid_private_facts_fail_without_changing_station_memory() {
        let mut input = fixture(vec![unit(1, "rifle", 50.0, 80.0)]);
        rails(&mut input);
        world(&mut input).units.insert(2, UnitFacts::default());
        let observation = Observation::new(input).unwrap();
        let mut state = State {
            rail_at: BTreeMap::from([(1, 100)]),
            ..State::default()
        };
        let before = state.clone();
        assert!(plan(&observation, &mut state, Level::Normal).is_err());
        assert_eq!(state, before);
    }

    #[test]
    fn intent_ai58_world_context_is_callable_through_commander_without_admission() {
        let observation = Observation::new(fixture(vec![unit(1, "rifle", 50.0, 80.0)])).unwrap();
        let mut state = crate::commander_tactics::State::default();
        let commands = crate::commander_tactics::plan(
            &observation,
            &[],
            &mut state,
            Level::Normal,
            &mut crate::random::MatchRandom::new(7),
        )
        .unwrap();
        assert_eq!(motion(&commands), vec![amove(1, 600.0, 80.0)]);
    }
}
