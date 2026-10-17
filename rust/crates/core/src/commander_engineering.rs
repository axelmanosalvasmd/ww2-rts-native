//! Engineering proposals from the seat's delivered terrain and contacts.
use crate::{
    catalog::{Catalog, Value as Rule},
    commander_ai::{Observation, Point, UnitObservation},
    game::CELL,
    math,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
struct Job {
    time: f64,
    dug: bool,
}

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct State {
    bridges: Vec<usize>,
    bridge_crew: BTreeMap<u32, Job>,
    fill_crew: BTreeMap<u32, Job>,
    demine_crew: BTreeMap<u32, Job>,
}

pub(crate) struct Plan {
    pub commands: Vec<Value>,
    pub builders: Vec<u32>,
}

fn distance(a: Point, b: Point) -> f64 {
    math::hypot(a.x - b.x, a.z - b.z)
}

fn retain_jobs(jobs: &mut BTreeMap<u32, Job>, squads: &[&UnitObservation], now: f64) {
    jobs.retain(|id, job| {
        let Some(unit) = squads.iter().find(|u| u.id == *id) else {
            return false;
        };
        if job.dug && !unit.dig || now - job.time > 90.0 {
            return false;
        }
        if unit.dig {
            job.dug = true;
        }
        true
    });
}

fn move_to(unit: &UnitObservation, at: Point) -> Value {
    json!({"t":"move","orders":[[unit.id,at.x,at.z]]})
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn plan(
    observation: &Observation,
    squads: &[&UnitObservation],
    enemies: &[&UnitObservation],
    busy: &BTreeSet<u32>,
    state: &mut State,
    sees: impl Fn(Point) -> bool,
) -> Result<Plan, String> {
    let input = observation.input();
    let context = input
        .runtime
        .as_ref()
        .ok_or("missing engineering context")?;
    let now = input.tick as f64 / 20.0;
    if state.bridges.iter().any(|c| *c >= input.chars.len())
        || [&state.bridge_crew, &state.fill_crew, &state.demine_crew]
            .iter()
            .any(|jobs| {
                jobs.iter().any(|(id, job)| {
                    *id == 0 || !job.time.is_finite() || job.time < 0.0 || job.time > now
                })
            })
    {
        return Err("invalid engineering memory".into());
    }
    let mut incoming = context.bridge_cells.clone();
    crate::sort::sort_by(&mut incoming, |a, b| a as f64 - b as f64);
    for cell in incoming {
        if !state.bridges.contains(&cell) {
            state.bridges.push(cell);
        }
    }
    retain_jobs(&mut state.bridge_crew, squads, now);
    retain_jobs(&mut state.fill_crew, squads, now);
    retain_jobs(&mut state.demine_crew, squads, now);
    let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
    let forts = catalog.table("FORTS").map_err(|e| e.to_string())?;
    let fort = |kind: &str, key: &str| {
        forts
            .get(kind)
            .and_then(|f| f.get(key))
            .and_then(Rule::number)
            .ok_or_else(|| format!("missing engineering rule {kind}.{key}"))
    };
    let builders = catalog
        .table("CFG")
        .map_err(|e| e.to_string())?
        .get("fortBuilders")
        .and_then(Rule::array)
        .ok_or("missing engineering builders")?;
    let builder = |u: &&UnitObservation| {
        u.owner == input.slot
            && u.hp > 0.0
            && builders.iter().any(|k| k.string() == Some(u.kind.as_str()))
            && !u.retreating
            && u.target_id == 0
            && !u.dig
            && !u.garrison
    };
    let at = |cell: usize| Point {
        x: (cell % input.width) as f64 * CELL + CELL / 2.0,
        z: (cell / input.width) as f64 * CELL + CELL / 2.0,
    };
    let safe = |p: Point| !enemies.iter().any(|e| distance(e.at, p) < 35.0);
    let mut result = Plan {
        commands: Vec::new(),
        builders: Vec::new(),
    };
    if input.manpower >= fort("bridge", "cost")? + 150.0 && state.bridge_crew.is_empty() {
        let gap = crate::sort::first_by(
            state
                .bridges
                .iter()
                .copied()
                .filter(|c| input.chars[*c] == b'W' && safe(at(*c))),
            |a, b| {
                distance(at(a), input.players[input.slot].spawn)
                    - distance(at(b), input.players[input.slot].spawn)
            },
        );
        if let Some(gap) = gap {
            // The prototype scans linear indices, including the row boundary.
            let run = |step: isize| {
                let water = |off: isize| {
                    gap.checked_add_signed(off * step)
                        .and_then(|c| input.chars.get(c))
                        .is_some_and(|ch| *ch == b'W')
                };
                let (mut lo, mut hi) = (0isize, 0isize);
                while lo > -8 && water(lo - 1) {
                    lo -= 1;
                }
                while hi < 8 && water(hi + 1) {
                    hi += 1;
                }
                (lo, hi)
            };
            let (x0, x1) = run(1);
            let (y0, y1) = run(input.width as isize);
            let along_x = x1 - x0 <= y1 - y0;
            let mut spot = at(gap);
            let offset = (if along_x { x0 + x1 } else { y0 + y1 }) as f64 * CELL / 2.0;
            if along_x {
                spot.x += offset;
            } else {
                spot.z += offset;
            }
            if let Some(unit) =
                crate::sort::first_by(squads.iter().copied().filter(builder), |a, b| {
                    distance(a.at, spot) - distance(b.at, spot)
                })
            {
                result.builders.push(unit.id);
                let direction = if along_x {
                    0.0
                } else {
                    std::f64::consts::FRAC_PI_2
                };
                if span_seen(
                    observation,
                    spot,
                    direction,
                    fort("bridge", "n")? as usize,
                    &sees,
                ) {
                    result.commands.push(json!({"t":"dig","ids":[unit.id],"kind":"bridge","x":spot.x,"z":spot.z,"dir":direction}));
                    state.bridge_crew.insert(
                        unit.id,
                        Job {
                            time: now,
                            dug: false,
                        },
                    );
                } else if unit.path.is_empty() {
                    let distance = distance(unit.at, spot);
                    let k = math::min(
                        1.0,
                        (fort("bridge", "reach")? - 2.0)
                            / if distance == 0.0 { 1.0 } else { distance },
                    );
                    result.commands.push(move_to(
                        unit,
                        Point {
                            x: spot.x + (unit.at.x - spot.x) * k,
                            z: spot.z + (unit.at.z - spot.z) * k,
                        },
                    ));
                }
            }
        }
    }
    let free = |u: &&UnitObservation| {
        builder(u) && !u.entrench && !busy.contains(&u.id) && !result.builders.contains(&u.id)
    };
    let mut idle: Vec<_> = squads.iter().copied().filter(free).collect();
    if input.manpower >= fort("fill", "cost")? + 150.0
        && state.fill_crew.is_empty()
        && !idle.is_empty()
    {
        let team = input.players[input.slot].team;
        let spots = input
            .points
            .iter()
            .filter(|p| p.owner.is_some_and(|o| input.players[o].team == team))
            .map(|p| (p.at, observation.point_radius() + 10.0))
            .chain(
                context
                    .nodes
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| !context.claimed_nodes.contains(i))
                    .map(|(_, p)| (*p, 4.0)),
            );
        'spots: for (spot, radius) in spots {
            let n = (radius / CELL).ceil() as isize;
            let px = (spot.x / CELL).floor() as isize;
            let py = (spot.z / CELL).floor() as isize;
            for y in (py - n).max(0)..=(py + n).min(input.height as isize - 1) {
                for x in (px - n).max(0)..=(px + n).min(input.width as isize - 1) {
                    let cell = y as usize * input.width + x as usize;
                    let target = at(cell);
                    if !context.spoiled_cells.contains(&cell)
                        || distance(target, spot) > radius
                        || !safe(target)
                    {
                        continue;
                    }
                    crate::sort::sort_by(&mut idle, |a, b| {
                        distance(a.at, target) - distance(b.at, target)
                    });
                    let unit = idle[0];
                    if distance(unit.at, target) > 60.0 {
                        continue;
                    }
                    result.builders.push(unit.id);
                    if span_seen(observation, target, 0.0, fort("fill", "n")? as usize, &sees) {
                        result.commands.push(json!({"t":"dig","ids":[unit.id],"kind":"fill","x":target.x,"z":target.z,"dir":0}));
                        state.fill_crew.insert(
                            unit.id,
                            Job {
                                time: now,
                                dug: false,
                            },
                        );
                    } else if unit.path.is_empty() {
                        result.commands.push(move_to(unit, target));
                    }
                    break 'spots;
                }
            }
        }
    }
    idle = squads
        .iter()
        .copied()
        .filter(|u| {
            builder(u) && !u.entrench && !busy.contains(&u.id) && !result.builders.contains(&u.id)
        })
        .collect();
    if input.manpower >= fort("demine", "cost")? && state.demine_crew.is_empty() {
        for &target in &context.found_mines {
            if !safe(target) {
                continue;
            }
            crate::sort::sort_by(&mut idle, |a, b| {
                distance(a.at, target) - distance(b.at, target)
            });
            let Some(unit) = idle.first().copied() else {
                break;
            };
            let distance = distance(unit.at, target);
            if distance > 40.0 {
                continue;
            }
            result.builders.push(unit.id);
            if span_seen(
                observation,
                target,
                0.0,
                fort("demine", "n")? as usize,
                &sees,
            ) {
                result.commands.push(json!({"t":"dig","ids":[unit.id],"kind":"demine","x":target.x,"z":target.z,"dir":0}));
                state.demine_crew.insert(
                    unit.id,
                    Job {
                        time: now,
                        dug: false,
                    },
                );
            } else if unit.path.is_empty() && distance > 10.0 {
                let k = 1.0 - 9.0 / distance;
                result.commands.push(move_to(
                    unit,
                    Point {
                        x: unit.at.x + (target.x - unit.at.x) * k,
                        z: unit.at.z + (target.z - unit.at.z) * k,
                    },
                ));
            }
            break;
        }
    }
    Ok(result)
}

fn span_seen(
    observation: &Observation,
    at: Point,
    direction: f64,
    count: usize,
    sees: &impl Fn(Point) -> bool,
) -> bool {
    let input = observation.input();
    (0..count).all(|i| {
        let along = i as f64 - (count - 1) as f64 / 2.0;
        let x = ((at.x + math::cos(direction) * along * CELL) / CELL).floor();
        let z = ((at.z + math::sin(direction) * along * CELL) / CELL).floor();
        x < 0.0
            || z < 0.0
            || x >= input.width as f64
            || z >= input.height as f64
            || sees(Point {
                x: (x + 0.5) * CELL,
                z: (z + 0.5) * CELL,
            })
    })
}

#[cfg(test)]
#[path = "commander_engineering_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "commander_engineering_lifecycle_tests.rs"]
mod lifecycle_tests;
