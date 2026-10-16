//! Engineering attempts over public start terrain and detached recipient knowledge.
use crate::{
    catalog::{Catalog, Value as Rule},
    commander_ai::{Observation, Point, UnitObservation},
    game::{CELL, TRENCH},
    math,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct State {
    bridges: BTreeSet<usize>,
    start_seen: bool,
    crew: BTreeMap<(u8, u32), (f64, bool)>,
}
#[derive(Default)]
pub(crate) struct Plan {
    pub commands: Vec<Value>,
    pub busy: BTreeSet<u32>,
}
fn distance(a: Point, b: Point) -> f64 {
    math::hypot(a.x - b.x, a.z - b.z)
}
fn number(rule: &Rule, key: &str) -> f64 {
    rule.get(key).and_then(Rule::number).unwrap_or(0.0)
}
fn allowed(unit: &UnitObservation, builders: &[Rule]) -> bool {
    builders
        .iter()
        .any(|v| v.string() == Some(unit.kind.as_str()))
        && !unit.retreating
        && unit.target_id == 0
        && !unit.dig
        && !unit.entrench
        && !unit.garrison
        && !unit.riding
        && !unit.build
        && !unit.enter
}
fn at(cell: usize, width: usize) -> Point {
    Point {
        x: (cell % width) as f64 * CELL + CELL / 2.0,
        z: (cell / width) as f64 * CELL + CELL / 2.0,
    }
}
fn nearby(observation: &Observation, p: Point, radius: f64, test: impl Fn(usize) -> bool) -> usize {
    let i = observation.input();
    let x0 = ((p.x - radius) / CELL).floor().max(0.0) as usize;
    let y0 = ((p.z - radius) / CELL).floor().max(0.0) as usize;
    let x1 = (((p.x + radius) / CELL).floor() as usize).min(i.width - 1);
    let y1 = (((p.z + radius) / CELL).floor() as usize).min(i.height - 1);
    (y0..=y1)
        .flat_map(|y| (x0..=x1).map(move |x| y * i.width + x))
        .filter(|c| test(*c))
        .count()
}
fn spoiled(observation: &Observation, cell: usize) -> bool {
    let i = observation.input();
    let c = i.runtime.as_ref().unwrap();
    let Some(initial) = &c.initial_chars else {
        return false;
    };
    let ch = i.chars[cell];
    let level = |cell| c.height.as_ref().map_or(0, |v| v[cell]);
    let original = c.initial_height.as_ref().map_or(0, |v| v[cell]);
    if ch == b'R' && initial[cell] == b'D' {
        return true;
    }
    let fillable = ch == b'+'
        || ch == b'R'
        || ch == b'F' && initial[cell] != b'F'
        || ch == b'.' && level(cell) < original;
    if !fillable || !(ch == b'F' || level(cell) < original) {
        return false;
    }
    if ch != b'.' {
        return true;
    }
    let x = cell % i.width;
    let low = [
        cell.checked_sub(i.width),
        cell.checked_add(i.width).filter(|c| *c < i.chars.len()),
        cell.checked_sub(1).filter(|_| x > 0),
        (x + 1 < i.width).then_some(cell + 1),
    ]
    .into_iter()
    .flatten()
    .map(level)
    .min()
    .unwrap_or(original);
    original.min(low.saturating_add(1)) > level(cell)
}

/// Repair one crossing, one damaged cell and one detected mine per look. Pending
/// jobs are remembered until an observed dig completes, the squad dies or 90 s pass.
pub(crate) fn repairs(
    observation: &Observation,
    state: &mut State,
    busy: &BTreeSet<u32>,
    sees: impl Fn(Point) -> bool,
) -> Result<Plan, String> {
    let i = observation.input();
    let c = i.runtime.as_ref().ok_or("missing engineering context")?;
    let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
    let cfg = catalog.table("CFG").map_err(|e| e.to_string())?;
    let builders = cfg
        .get("fortBuilders")
        .and_then(Rule::array)
        .ok_or("missing fort builders")?;
    let forts = catalog.table("FORTS").map_err(|e| e.to_string())?;
    let cost = |name| forts.get(name).map_or(0.0, |r| number(r, "cost"));
    let now = i.tick as f64 / 20.0;
    let own: Vec<_> = i
        .units
        .iter()
        .filter(|u| u.owner == i.slot && u.hp > 0.0)
        .collect();
    state.crew.retain(|(_, id), (sent, dug)| {
        let Some(u) = own.iter().find(|u| u.id == *id) else {
            return false;
        };
        if *dug && !u.dig || now - *sent > 90.0 {
            return false;
        }
        *dug |= u.dig;
        true
    });
    if !state.start_seen
        && let Some(initial) = &c.initial_chars
    {
        state.bridges.extend(
            initial
                .iter()
                .enumerate()
                .filter_map(|(cell, ch)| (*ch == b'=').then_some(cell)),
        );
        state.start_seen = true;
    }
    state.bridges.extend(
        i.chars
            .iter()
            .enumerate()
            .filter_map(|(cell, ch)| (*ch == b'=').then_some(cell)),
    );
    let team = i.players[i.slot].team;
    let safe = |p| {
        !i.units
            .iter()
            .any(|u| u.hp > 0.0 && i.players[u.owner].team != team && distance(u.at, p) < 35.0)
    };
    let mut result = Plan::default();
    let mut wallet = i.manpower;
    for (kind, name, reserve, reach) in [
        (0, "bridge", 150.0, f64::INFINITY),
        (1, "fill", 150.0, 60.0),
        (2, "demine", 0.0, 40.0),
    ] {
        if wallet < cost(name) + reserve || state.crew.keys().any(|(k, _)| *k == kind) {
            continue;
        }
        let mut candidates = Vec::new();
        if kind == 0 {
            candidates.extend(
                state
                    .bridges
                    .iter()
                    .copied()
                    .filter(|cell| i.chars[*cell] == b'W')
                    .map(|cell| at(cell, i.width)),
            );
            candidates.sort_by(|a, b| {
                distance(*a, i.players[i.slot].spawn)
                    .total_cmp(&distance(*b, i.players[i.slot].spawn))
            });
        } else if kind == 1 {
            let mut spots: Vec<_> = i
                .points
                .iter()
                .filter(|p| p.owner.is_some_and(|o| i.players[o].team == team))
                .map(|p| (p.at, observation.point_radius() + 10.0))
                .collect();
            spots.extend(
                c.nodes
                    .iter()
                    .copied()
                    .filter(|p| {
                        !i.units
                            .iter()
                            .any(|u| u.kind == "depot" && distance(u.at, *p) < 8.0)
                            && !c.known_depots.iter().any(|q| distance(*q, *p) < 8.0)
                    })
                    .map(|p| (p, 4.0)),
            );
            for (p, radius) in spots {
                let x0 = ((p.x - radius) / CELL).floor().max(0.0) as usize;
                let z0 = ((p.z - radius) / CELL).floor().max(0.0) as usize;
                let x1 = (((p.x + radius) / CELL).floor() as usize).min(i.width - 1);
                let z1 = (((p.z + radius) / CELL).floor() as usize).min(i.height - 1);
                for z in z0..=z1 {
                    for x in x0..=x1 {
                        let cell = z * i.width + x;
                        let point = at(cell, i.width);
                        if distance(point, p) <= radius && spoiled(observation, cell) {
                            candidates.push(point);
                        }
                    }
                }
            }
        } else {
            candidates.extend(c.found_mines.iter().copied());
        }
        for mut point in candidates {
            if !safe(point) {
                continue;
            }
            let mut direction = 0.0;
            if kind == 0 {
                let cell = (point.z / CELL) as usize * i.width + (point.x / CELL) as usize;
                let run = |dx: isize, dy: isize| {
                    let mut low = 0isize;
                    let mut high = 0isize;
                    let water = |offset: isize| {
                        let x = cell as isize % i.width as isize + offset * dx;
                        let y = cell as isize / i.width as isize + offset * dy;
                        x >= 0
                            && y >= 0
                            && x < i.width as isize
                            && y < i.height as isize
                            && i.chars[y as usize * i.width + x as usize] == b'W'
                    };
                    while low > -8 && water(low - 1) {
                        low -= 1;
                    }
                    while high < 8 && water(high + 1) {
                        high += 1;
                    }
                    (low, high)
                };
                let (x0, x1) = run(1, 0);
                let (z0, z1) = run(0, 1);
                if x1 - x0 <= z1 - z0 {
                    point.x += (x0 + x1) as f64 / 2.0 * CELL;
                } else {
                    point.z += (z0 + z1) as f64 / 2.0 * CELL;
                    direction = std::f64::consts::FRAC_PI_2;
                }
            }
            let Some(u) = own
                .iter()
                .copied()
                .filter(|u| {
                    allowed(u, builders) && !busy.contains(&u.id) && !result.busy.contains(&u.id)
                })
                .min_by(|a, b| {
                    distance(a.at, point)
                        .total_cmp(&distance(b.at, point))
                        .then(a.id.cmp(&b.id))
                })
            else {
                break;
            };
            if distance(u.at, point) > reach {
                continue;
            }
            let mut visible = true;
            // Canonical placement checks every in-map stamp cell, including dry banks.
            let count = forts.get(name).map_or(1.0, |rule| number(rule, "n")) as usize;
            for index in 0..count {
                let along = index as f64 - (count as f64 - 1.0) / 2.0;
                let x = point.x + math::cos(direction) * along * CELL;
                let z = point.z + math::sin(direction) * along * CELL;
                if x >= 0.0 && z >= 0.0 && x < i.width as f64 * CELL && z < i.height as f64 * CELL {
                    visible &= sees(at(
                        (z / CELL).floor() as usize * i.width + (x / CELL).floor() as usize,
                        i.width,
                    ));
                }
            }
            if !visible {
                result.busy.insert(u.id);
                if u.path.is_empty() && (kind != 2 || distance(u.at, point) > 10.0) {
                    let gap = if kind == 0 {
                        forts.get("bridge").map_or(0.0, |r| number(r, "reach")) - 2.0
                    } else if kind == 2 {
                        9.0
                    } else {
                        0.0
                    };
                    let fraction = (gap / distance(u.at, point).max(1.0)).min(1.0);
                    let target = Point {
                        x: point.x + (u.at.x - point.x) * fraction,
                        z: point.z + (u.at.z - point.z) * fraction,
                    };
                    result
                        .commands
                        .push(json!({"t":"move","orders":[[u.id,target.x,target.z]]}));
                }
                break;
            }
            result.commands.push(
                json!({"t":"dig","ids":[u.id],"kind":name,"x":point.x,"z":point.z,"dir":direction}),
            );
            result.busy.insert(u.id);
            state.crew.insert((kind, u.id), (now, false));
            wallet -= cost(name);
            break;
        }
    }
    Ok(result)
}

/// The first infantry holder fortifies a quiet captured objective toward the
/// nearest public enemy home, using only mines and terrain the seat was told about.
pub(crate) fn holder(
    observation: &Observation,
    unit: &UnitObservation,
    point: usize,
    armor_seen: bool,
) -> Result<Option<Value>, String> {
    let i = observation.input();
    let c = i.runtime.as_ref().ok_or("missing holder context")?;
    let p = &i.points[point];
    let team = i.players[i.slot].team;
    if c.horde || p.owner.is_none_or(|o| i.players[o].team != team) {
        return Ok(None);
    }
    let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
    let cfg = catalog.table("CFG").map_err(|e| e.to_string())?;
    let forts = catalog.table("FORTS").map_err(|e| e.to_string())?;
    let cost = |name| forts.get(name).map_or(0.0, |r| number(r, "cost"));
    let builders = cfg
        .get("fortBuilders")
        .and_then(Rule::array)
        .ok_or("missing fort builders")?;
    let Some(foe) = i
        .players
        .iter()
        .filter(|q| q.team != team)
        .min_by(|a, b| distance(a.spawn, p.at).total_cmp(&distance(b.spawn, p.at)))
    else {
        return Ok(None);
    };
    let length = distance(foe.spawn, p.at).max(1.0);
    let fx = (foe.spawn.x - p.at.x) / length;
    let fz = (foe.spawn.z - p.at.z) / length;
    let toward = |k| Point {
        x: p.at.x + fx * k,
        z: p.at.z + fz * k,
    };
    let radius = observation.point_radius();
    let ahead = toward(radius + 7.0);
    let needs = [
        i.manpower >= cost("mines") + 60.0
            && c.mines
                .iter()
                .filter(|m| distance(**m, p.at) <= radius + 10.0)
                .count()
                < 2,
        i.manpower >= number(cfg, "digCost") + 60.0
            && nearby(observation, p.at, radius + 4.0, |cell| {
                i.flags[cell] & TRENCH != 0
            }) < 6,
        i.manpower >= cost("wire") * 2.0 + 60.0
            && nearby(observation, ahead, 6.0, |cell| i.chars[cell] == b'X') < 4,
        armor_seen
            && i.manpower >= cost("traps") * 2.0 + 60.0
            && nearby(observation, ahead, 8.0, |cell| i.chars[cell] == b'Y') < 4,
    ];
    let free = builders
        .iter()
        .any(|v| v.string() == Some(unit.kind.as_str()))
        && !unit.dig
        && !unit.entrench;
    let work = free
        && needs.iter().any(|n| *n)
        && !i
            .units
            .iter()
            .any(|u| i.players[u.owner].team != team && distance(u.at, p.at) < 40.0);
    if !work
        && !unit.garrison
        && catalog
            .unit(&unit.kind)
            .map_err(|e| e.to_string())?
            .get("garrisons")
            .and_then(Rule::boolean)
            .unwrap_or(false)
    {
        let mut houses: Vec<_> = (0..i.chars.len())
            .filter(|cell| i.chars[*cell] == b'B')
            .map(|cell| at(cell, i.width))
            .filter(|at| {
                (at.x - p.at.x).abs() <= radius + 3.0 && (at.z - p.at.z).abs() <= radius + 3.0
            })
            .collect();
        houses.sort_by(|a, b| distance(*a, p.at).total_cmp(&distance(*b, p.at)));
        if let Some(house) = houses.first() {
            return Ok(Some(
                json!({"t":"garrison","ids":[unit.id],"x":house.x,"z":house.z}),
            ));
        }
    }
    if unit.garrison {
        return Ok(work.then(|| json!({"t":"move","orders":[[unit.id,p.at.x,p.at.z]]})));
    }
    if !free {
        return Ok(None);
    }
    let angle = math::atan2(fz, fx) + std::f64::consts::FRAC_PI_2;
    if needs[0] {
        let q = toward(radius + 2.0);
        return Ok(Some(
            json!({"t":"dig","ids":[unit.id],"kind":"mines","x":q.x,"z":q.z,"dir":angle}),
        ));
    }
    if needs[1] {
        let q = toward(6.0);
        return Ok(Some(
            json!({"t":"entrench","ids":[unit.id],"pattern":if i.manpower >= 600.0 {"strongpoint"} else {"arc"},"x":p.at.x,"z":p.at.z,"x2":q.x,"z2":q.z}),
        ));
    }
    for (needed, kind, k, half) in [
        (needs[2], "wire", radius + 5.0, 7.0),
        (needs[3], "traps", radius + 10.0, 8.0),
    ] {
        if needed {
            let q = toward(k);
            return Ok(Some(
                json!({"t":"entrench","ids":[unit.id],"pattern":"line","fort":kind,"x":q.x+fz*half,"z":q.z-fx*half,"x2":q.x-fz*half,"z2":q.z+fx*half}),
            ));
        }
    }
    Ok(None)
}

#[cfg(test)]
#[path = "commander_engineering_tests.rs"]
mod tests;
