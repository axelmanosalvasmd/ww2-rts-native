//! Field work and point defense over this seat's delivered terrain and units.
use crate::{
    catalog::{Catalog, Value as Rule},
    commander_ai::{Observation, Point, UnitObservation},
    game::{CELL, TRENCH},
    math,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

/// Authored public terrain plus recipient-filtered discoveries. No live World is borrowed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Facts {
    pub initial_chars: Vec<u8>,
    pub initial_height: Option<Vec<i8>>,
    pub ground: Vec<u8>,
    pub bridges: BTreeSet<usize>,
    pub mines: Vec<Point>,
    pub found_mines: Vec<Point>,
    pub free_nodes: Vec<Point>,
}
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
struct Sent {
    tick: i64,
    dug: bool,
}
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct State {
    bridges: BTreeSet<usize>,
    bridge_crew: BTreeMap<u32, Sent>,
    fill_crew: BTreeMap<u32, Sent>,
    demine_crew: BTreeMap<u32, Sent>,
    pub armor_seen: bool,
}
fn dist(a: Point, b: Point) -> f64 {
    math::hypot(a.x - b.x, a.z - b.z)
}
fn num(d: &Rule, key: &str) -> f64 {
    d.get(key).and_then(Rule::number).unwrap_or(0.0)
}
fn flag(d: &Rule, key: &str) -> bool {
    d.get(key).and_then(Rule::boolean).unwrap_or(false)
}
fn at(width: usize, cell: usize) -> Point {
    Point {
        x: (cell % width) as f64 * CELL + CELL / 2.0,
        z: (cell / width) as f64 * CELL + CELL / 2.0,
    }
}
fn refresh(crew: &mut BTreeMap<u32, Sent>, squads: &[&UnitObservation], tick: i64) {
    crew.retain(|id, job| {
        let Some(u) = squads.iter().find(|u| u.id == *id) else {
            return false;
        };
        if job.dug && !u.dig || tick - job.tick > 1800 {
            return false;
        }
        job.dug |= u.dig;
        true
    });
}
fn builders<'a>(
    squads: &[&'a UnitObservation],
    busy: &BTreeSet<u32>,
    catalog: &Catalog,
) -> Vec<&'a UnitObservation> {
    let list = catalog
        .table("CFG")
        .unwrap()
        .get("fortBuilders")
        .unwrap()
        .array()
        .unwrap();
    squads
        .iter()
        .copied()
        .filter(|u| {
            list.iter().any(|v| v.string() == Some(u.kind.as_str()))
                && !u.retreating
                && u.target_id == 0
                && !u.build
                && !u.dig
                && !u.entrench
                && !u.garrison
                && !u.riding
                && !busy.contains(&u.id)
        })
        .collect()
}
fn nearest<'a>(squads: &[&'a UnitObservation], goal: Point) -> Option<&'a UnitObservation> {
    squads.iter().copied().min_by(|a, b| {
        dist(a.at, goal)
            .total_cmp(&dist(b.at, goal))
            .then(a.id.cmp(&b.id))
    })
}
fn move_to(u: &UnitObservation, p: Point) -> Value {
    json!({"t":"move","orders":[[u.id,p.x,p.z]]})
}
fn dig(u: &UnitObservation, kind: &str, p: Point, dir: f64) -> Value {
    json!({"t":"dig","ids":[u.id],"kind":kind,"x":p.x,"z":p.z,"dir":dir})
}

/// The submit result is admission feedback. A denied job can produce a safe approach order.
/// A host with delayed input may use detached visibility to preflight the attempt.
pub fn field_work(
    observation: &Observation,
    state: &mut State,
    busy: &mut BTreeSet<u32>,
    mut submit: impl FnMut(Value) -> bool,
) -> Result<f64, String> {
    let input = observation.input();
    let Some(facts) = input.runtime.as_ref().and_then(|c| c.field.as_ref()) else {
        return Ok(input.manpower);
    };
    let n = input.chars.len();
    if facts.initial_chars.len() != n
        || facts.ground.len() != n
        || facts.initial_height.as_ref().is_some_and(|h| h.len() != n)
        || facts.bridges.iter().any(|&c| c >= n)
        || facts
            .mines
            .iter()
            .chain(&facts.found_mines)
            .chain(&facts.free_nodes)
            .any(|p| {
                !p.x.is_finite()
                    || !p.z.is_finite()
                    || p.x < 0.0
                    || p.z < 0.0
                    || p.x >= input.width as f64 * CELL
                    || p.z >= input.height as f64 * CELL
            })
    {
        return Err("invalid delivered field terrain".into());
    }
    let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
    let forts = catalog.table("FORTS").map_err(|e| e.to_string())?;
    let cost = |k: &str| num(forts.get(k).unwrap(), "cost");
    let team = input.players[input.slot].team;
    let squads: Vec<_> = input
        .units
        .iter()
        .filter(|u| u.owner == input.slot && u.hp > 0.0 && u.kind != "engineer")
        .collect();
    let quiet = |p| {
        !input
            .units
            .iter()
            .any(|u| u.hp > 0.0 && input.players[u.owner].team != team && dist(u.at, p) < 35.0)
    };
    state.bridges.extend(&facts.bridges);
    refresh(&mut state.bridge_crew, &squads, input.tick);
    refresh(&mut state.fill_crew, &squads, input.tick);
    refresh(&mut state.demine_crew, &squads, input.tick);
    let spawn = input.players[input.slot].spawn;
    let mut wallet = input.manpower;
    if wallet >= cost("bridge") + 150.0
        && !squads
            .iter()
            .any(|u| u.dig && state.bridge_crew.contains_key(&u.id))
    {
        let gap = state
            .bridges
            .iter()
            .copied()
            .filter(|&c| input.chars[c] == b'W' && quiet(at(input.width, c)))
            .min_by(|&a, &b| {
                dist(at(input.width, a), spawn)
                    .total_cmp(&dist(at(input.width, b), spawn))
                    .then(a.cmp(&b))
            });
        if let Some(gap) = gap {
            let run = |dx: isize, dz: isize| {
                let x = (gap % input.width) as isize;
                let z = (gap / input.width) as isize;
                let water = |k: isize| {
                    let xx = x + dx * k;
                    let zz = z + dz * k;
                    xx >= 0
                        && zz >= 0
                        && xx < input.width as isize
                        && zz < input.height as isize
                        && input.chars[zz as usize * input.width + xx as usize] == b'W'
                };
                let mut lo = 0;
                let mut hi = 0;
                while lo > -8 && water(lo - 1) {
                    lo -= 1;
                }
                while hi < 8 && water(hi + 1) {
                    hi += 1;
                }
                (lo, hi)
            };
            let (x0, x1) = run(1, 0);
            let (y0, y1) = run(0, 1);
            let along = x1 - x0 <= y1 - y0;
            let mut spot = at(input.width, gap);
            let off = if along {
                (x0 + x1) as f64
            } else {
                (y0 + y1) as f64
            } * CELL
                / 2.0;
            if along {
                spot.x += off;
            } else {
                spot.z += off;
            }
            if let Some(u) = nearest(&builders(&squads, busy, catalog), spot) {
                busy.insert(u.id);
                if submit(dig(
                    u,
                    "bridge",
                    spot,
                    if along {
                        0.0
                    } else {
                        std::f64::consts::PI / 2.0
                    },
                )) {
                    state.bridge_crew.insert(
                        u.id,
                        Sent {
                            tick: input.tick,
                            dug: false,
                        },
                    );
                    wallet -= cost("bridge");
                } else if u.path.is_empty() {
                    let k = math::min(
                        1.0,
                        (num(forts.get("bridge").unwrap(), "reach") - 2.0)
                            / math::max(1.0, dist(u.at, spot)),
                    );
                    submit(move_to(
                        u,
                        Point {
                            x: spot.x + (u.at.x - spot.x) * k,
                            z: spot.z + (u.at.z - spot.z) * k,
                        },
                    ));
                }
            }
        }
    }
    if wallet >= cost("fill") + 150.0 && state.fill_crew.is_empty() {
        let radius = observation.point_radius() + 10.0;
        let spots = input
            .points
            .iter()
            .filter(|p| p.owner.is_some_and(|o| input.players[o].team == team))
            .map(|p| (p.at, radius))
            .chain(facts.free_nodes.iter().copied().map(|p| (p, 4.0)));
        'spots: for (p, r) in spots {
            let reach = (r / CELL).ceil() as isize;
            let px = (p.x / CELL).floor() as isize;
            let pz = (p.z / CELL).floor() as isize;
            for z in (pz - reach).max(0)..=(pz + reach).min(input.height as isize - 1) {
                for x in (px - reach).max(0)..=(px + reach).min(input.width as isize - 1) {
                    let c = z as usize * input.width + x as usize;
                    let spot = at(input.width, c);
                    if !spoiled(observation, facts, c) || dist(spot, p) > r || !quiet(spot) {
                        continue;
                    }
                    let Some(u) = nearest(&builders(&squads, busy, catalog), spot) else {
                        break 'spots;
                    };
                    if dist(u.at, spot) > 60.0 {
                        continue;
                    }
                    busy.insert(u.id);
                    if submit(dig(u, "fill", spot, 0.0)) {
                        state.fill_crew.insert(
                            u.id,
                            Sent {
                                tick: input.tick,
                                dug: false,
                            },
                        );
                        wallet -= cost("fill");
                    } else if u.path.is_empty() {
                        submit(move_to(u, spot));
                    }
                    break 'spots;
                }
            }
        }
    }
    if wallet >= cost("demine") && state.demine_crew.is_empty() {
        for &spot in &facts.found_mines {
            if !quiet(spot) {
                continue;
            }
            let Some(u) = nearest(&builders(&squads, busy, catalog), spot) else {
                break;
            };
            let distance = dist(u.at, spot);
            if distance > 40.0 {
                continue;
            }
            busy.insert(u.id);
            if submit(dig(u, "demine", spot, 0.0)) {
                state.demine_crew.insert(
                    u.id,
                    Sent {
                        tick: input.tick,
                        dug: false,
                    },
                );
                wallet -= cost("demine");
            } else if u.path.is_empty() && distance > 10.0 {
                let k = 1.0 - 9.0 / distance;
                submit(move_to(
                    u,
                    Point {
                        x: u.at.x + (spot.x - u.at.x) * k,
                        z: u.at.z + (spot.z - u.at.z) * k,
                    },
                ));
            }
            break;
        }
    }
    Ok(wallet)
}
fn spoiled(o: &Observation, f: &Facts, c: usize) -> bool {
    let input = o.input();
    let ch = input.chars[c];
    let level = |c| {
        input
            .runtime
            .as_ref()
            .and_then(|r| r.height.as_ref())
            .map_or(0, |h| h[c])
    };
    let initial = f.initial_height.as_ref().map_or(0, |h| h[c]);
    let fillable = ch == b'+'
        || ch == b'R'
            && !(input
                .runtime
                .as_ref()
                .is_some_and(|r| r.mode == crate::economy::Mode::World)
                && f.initial_chars[c] == b'R')
        || ch == b'F' && f.initial_chars[c] != b'F'
        || ch == b'.' && level(c) < initial;
    let x = c % input.width;
    let low = [
        c.checked_sub(input.width),
        (c + input.width < input.chars.len()).then_some(c + input.width),
        c.checked_sub(1).filter(|_| x > 0),
        (x + 1 < input.width).then_some(c + 1),
    ]
    .into_iter()
    .flatten()
    .map(level)
    .min()
    .unwrap_or(initial);
    ch == b'R' && f.ground[c] == b'D'
        || fillable
            && (ch == b'F' || level(c) < initial)
            && (ch != b'.' || initial.min(low.saturating_add(1)) > level(c))
}
fn near_cells(o: &Observation, p: Point, r: f64, test: impl Fn(usize) -> bool) -> usize {
    let i = o.input();
    let mut n = 0;
    for z in ((p.z - r) / CELL).floor().max(0.0) as usize
        ..=((p.z + r) / CELL).floor().min(i.height as f64 - 1.0) as usize
    {
        for x in ((p.x - r) / CELL).floor().max(0.0) as usize
            ..=((p.x + r) / CELL).floor().min(i.width as f64 - 1.0) as usize
        {
            if test(z * i.width + x) {
                n += 1;
            }
        }
    }
    n
}
/// Only the first infantry at a held point calls this policy; later squads remain free.
pub fn holder(
    o: &Observation,
    u: &UnitObservation,
    index: usize,
    armor_seen: bool,
    mut submit: impl FnMut(Value) -> bool,
) -> Result<(), String> {
    let input = o.input();
    let p = &input.points[index];
    let team = input.players[input.slot].team;
    let Some(foe) = input
        .players
        .iter()
        .enumerate()
        .filter(|(_, q)| q.team != team)
        .min_by(|(ai, a), (bi, b)| {
            dist(a.spawn, p.at)
                .total_cmp(&dist(b.spawn, p.at))
                .then(ai.cmp(bi))
        })
        .map(|(_, p)| p.spawn)
    else {
        return Ok(());
    };
    let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
    let forts = catalog.table("FORTS").map_err(|e| e.to_string())?;
    let cfg = catalog.table("CFG").map_err(|e| e.to_string())?;
    let r = o.point_radius();
    let length = math::max(dist(foe, p.at), 1.0);
    let fx = (foe.x - p.at.x) / length;
    let fz = (foe.z - p.at.z) / length;
    let toward = |k| Point {
        x: p.at.x + fx * k,
        z: p.at.z + fz * k,
    };
    let ahead = toward(r + 7.0);
    let facts = input.runtime.as_ref().and_then(|r| r.field.as_ref());
    let mines = facts.map_or(0, |f| {
        f.mines
            .iter()
            .filter(|&&m| dist(m, p.at) <= r + 10.0)
            .count()
    });
    let cost = |k| num(forts.get(k).unwrap(), "cost");
    let needs = [
        input.manpower >= cost("mines") + 60.0 && mines < 2,
        input.manpower >= num(cfg, "digCost") + 60.0
            && near_cells(o, p.at, r + 4.0, |c| input.flags[c] & TRENCH != 0) < 6,
        input.manpower >= cost("wire") * 2.0 + 60.0
            && near_cells(o, ahead, 6.0, |c| input.chars[c] == b'X') < 4,
        armor_seen
            && input.manpower >= cost("traps") * 2.0 + 60.0
            && near_cells(o, ahead, 8.0, |c| input.chars[c] == b'Y') < 4,
    ];
    let free = facts.is_some()
        && !u.dig
        && !u.entrench
        && cfg
            .get("fortBuilders")
            .unwrap()
            .array()
            .unwrap()
            .iter()
            .any(|k| k.string() == Some(u.kind.as_str()));
    let quiet = !input
        .units
        .iter()
        .any(|e| e.hp > 0.0 && input.players[e.owner].team != team && dist(e.at, p.at) < 40.0);
    let work = free && quiet && needs.iter().any(|b| *b);
    if !work && !u.garrison && flag(catalog.unit(&u.kind).unwrap(), "garrisons") {
        let mut house = None;
        let mut distance = f64::INFINITY;
        for z in ((p.at.z - r - 3.0) / CELL).floor().max(0.0) as usize
            ..=((p.at.z + r + 3.0) / CELL)
                .floor()
                .min(input.height as f64 - 1.0) as usize
        {
            for x in ((p.at.x - r - 3.0) / CELL).floor().max(0.0) as usize
                ..=((p.at.x + r + 3.0) / CELL)
                    .floor()
                    .min(input.width as f64 - 1.0) as usize
            {
                let c = z * input.width + x;
                if input.chars[c] != b'B' {
                    continue;
                }
                let q = at(input.width, c);
                let d = dist(q, p.at);
                if d < distance {
                    house = Some(q);
                    distance = d;
                }
            }
        }
        if let Some(h) = house
            && submit(json!({"t":"garrison","ids":[u.id],"x":h.x,"z":h.z}))
        {
            return Ok(());
        }
    }
    if u.garrison {
        if work {
            submit(move_to(u, p.at));
        }
        return Ok(());
    }
    if work {
        let angle = math::atan2(fz, fx);
        if needs[0]
            && submit(dig(
                u,
                "mines",
                toward(r + 2.0),
                angle + std::f64::consts::PI / 2.0,
            ))
        {
            return Ok(());
        }
        if needs[1] {
            let q = toward(6.0);
            submit(
                json!({"t":"entrench","ids":[u.id],"pattern":if input.manpower>=600.0{"strongpoint"}else{"arc"},"x":p.at.x,"z":p.at.z,"x2":q.x,"z2":q.z}),
            );
            return Ok(());
        }
        for (needed, fort, k, half) in [
            (needs[2], "wire", r + 5.0, 7.0),
            (needs[3], "traps", r + 10.0, 8.0),
        ] {
            if needed {
                let q = toward(k);
                if submit(
                    json!({"t":"entrench","ids":[u.id],"pattern":"line","fort":fort,"x":q.x+fz*half,"z":q.z-fx*half,"x2":q.x-fz*half,"z2":q.z+fx*half}),
                ) {
                    return Ok(());
                }
            }
        }
    }
    if !u.dig && !u.entrench && !matches!(u.cover, 1 | 2) {
        submit(json!({"t":"cover","ids":[u.id]}));
    }
    Ok(())
}

/// Preflight delayed Dig attempts against the complete delivered visibility footprint.
pub(crate) fn can_submit(
    terrain: &crate::game::Game,
    raw: &Value,
    sees: impl Fn(Point) -> bool,
) -> bool {
    if raw["t"] != "dig" {
        return true;
    }
    let Some(kind) = raw["kind"]
        .as_str()
        .and_then(|s| crate::field_works::FortKind::parse(s).ok())
    else {
        return false;
    };
    let Some(x) = raw["x"].as_f64() else {
        return false;
    };
    let Some(z) = raw["z"].as_f64() else {
        return false;
    };
    crate::field_works::visible_placement(
        terrain,
        kind,
        Point { x, z },
        raw["dir"].as_f64().unwrap_or(0.0),
        sees,
    )
    .unwrap_or(false)
}
