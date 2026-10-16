//! Forward reinforcement bases planned from delivered terrain and unit observations.
use crate::{
    catalog::{Catalog, Value},
    commander_ai::{Observation, Point, UnitObservation},
    economy::Mode,
    game::{CELL, Game, MOVE},
    match_runtime::{find_site, foot_center, footprint},
    math,
};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Forward {
    pub command: serde_json::Value,
    pub unit: u32,
    pub cost: f64,
}

fn distance(a: Point, b: Point) -> f64 {
    math::hypot(a.x - b.x, a.z - b.z)
}
fn number(definition: &Value, key: &str) -> Result<f64, String> {
    definition
        .get(key)
        .and_then(Value::number)
        .ok_or_else(|| format!("missing forward HQ rule {key}"))
}
fn flag(definition: &Value, key: &str) -> bool {
    definition
        .get(key)
        .and_then(Value::boolean)
        .unwrap_or(false)
}

pub(crate) fn plan(
    observation: &Observation,
    available: &[&UnitObservation],
    budget: f64,
    terrain: &Game,
    sees: impl Fn(Point) -> bool,
) -> Result<Option<Forward>, String> {
    let input = observation.input();
    let context = input.runtime.as_ref().ok_or("missing forward HQ context")?;
    if !context.mode.is_skirmish()
        || context.horde
        || context.mode == Mode::Assault && context.defender
    {
        return Ok(None);
    }
    let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
    let hq = catalog.unit("hq").map_err(|e| e.to_string())?;
    let cost = number(hq, "cost")?;
    if budget < cost {
        return Ok(None);
    }
    let team = input.players[input.slot].team;
    let allied = |u: &UnitObservation| input.players[u.owner].team == team;
    let definition = |u: &UnitObservation| catalog.unit(&u.kind).map_err(|e| e.to_string());
    let built = |u: &UnitObservation| context.units.get(&u.id).map_or(0.0, |f| f.built);
    let mut own = Vec::new();
    for u in &input.units {
        if u.hp > 0.0
            && (u.owner == input.slot || context.mode == Mode::Horde && allied(u))
            && flag(definition(u)?, "building")
        {
            own.push(u);
        }
    }
    let hqs: Vec<_> = own.iter().copied().filter(|u| u.kind == "hq").collect();
    if hqs.is_empty() || hqs.len() >= 3 || hqs.iter().any(|u| built(u) < 1.0) {
        return Ok(None);
    }
    // The caller repairs and finishes buildings before starting another site.
    if available.iter().any(|u| u.build) {
        return Ok(None);
    }
    for u in &own {
        if built(u) < 1.0 || u.hp < number(definition(u)?, "hpPer")? * 0.8 {
            return Ok(None);
        }
    }
    let builders = catalog
        .table("CFG")
        .map_err(|e| e.to_string())?
        .get("fortBuilders")
        .and_then(Value::array)
        .ok_or("missing forward HQ builders")?;
    let mut army = Vec::new();
    let mut crew = Vec::new();
    for &u in available {
        if u.owner != input.slot || u.hp <= 0.0 {
            continue;
        }
        let d = definition(u)?;
        if !flag(d, "air") && !flag(d, "structure") && !u.retreating {
            army.push(u);
        }
        if builders
            .iter()
            .any(|kind| kind.string() == Some(u.kind.as_str()))
            && !u.retreating
            && !u.riding
            && !u.build
            && u.target_id == 0
            && !u.dig
            && !u.entrench
            && !u.garrison
        {
            crew.push(u);
        }
    }
    if army.len() < 3 || crew.is_empty() {
        return Ok(None);
    }
    army.sort_by_key(|u| u.id);
    let front = Point {
        x: army.iter().map(|u| u.at.x).sum::<f64>() / army.len() as f64,
        z: army.iter().map(|u| u.at.z).sum::<f64>() / army.len() as f64,
    };
    let near = hqs
        .into_iter()
        .min_by(|a, b| {
            distance(a.at, front)
                .total_cmp(&distance(b.at, front))
                .then(a.id.cmp(&b.id))
        })
        .unwrap();
    let far = distance(near.at, front);
    if far < 70.0 {
        return Ok(None);
    }
    let aim = Point {
        x: front.x + (near.at.x - front.x) * (20.0 / far),
        z: front.z + (near.at.z - front.z) * (20.0 / far),
    };
    let size = number(hq, "size")? as usize;
    let half = size as f64 * CELL / 2.0;
    let mut avoid: Vec<_> = input
        .points
        .iter()
        .map(|p| (p.at, observation.point_radius() + 2.0))
        .collect();
    avoid.extend(input.players.iter().map(|p| (p.spawn, 6.0)));
    // Mark delivered troop cells locally so find_site searches past occupied footprints.
    let mut ground = terrain.clone();
    for u in &input.units {
        if u.hp <= 0.0 {
            continue;
        }
        let d = definition(u)?;
        if flag(d, "structure") {
            avoid.push((u.at, number(d, "radius")? + half + 4.0));
        } else if !flag(d, "air")
            && !u.riding
            && let Some(cell) = ground.cell_of(u.at.x, u.at.z)
        {
            ground.flags[cell] |= MOVE;
        }
    }
    for radius in [0.0, 8.0, 16.0] {
        for index in 0..if radius == 0.0 { 1 } else { 8 } {
            let angle = index as f64 * std::f64::consts::FRAC_PI_4;
            let seed = Point {
                x: aim.x + math::cos(angle) * radius - half,
                z: aim.z + math::sin(angle) * radius - half,
            };
            let Some(cell) = find_site(&ground, seed, size, &avoid) else {
                continue;
            };
            let at = foot_center(&ground, cell, size);
            if !sees(at)
                || input
                    .units
                    .iter()
                    .any(|u| u.hp > 0.0 && !allied(u) && distance(u.at, at) < 30.0)
            {
                continue;
            }
            if footprint(&ground, cell, size).is_none() {
                continue;
            }
            let builder = crew
                .iter()
                .min_by(|a, b| {
                    distance(a.at, at)
                        .total_cmp(&distance(b.at, at))
                        .then(a.id.cmp(&b.id))
                })
                .unwrap();
            return Ok(Some(Forward {
                command: serde_json::json!({"t":"build", "ids":[builder.id], "kind":"hq", "x":at.x, "z":at.z}),
                unit: builder.id,
                cost,
            }));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        commander_ai::{Input, PlayerObservation, PointObservation},
        commander_tactics::{Context, UnitFacts},
    };
    use std::collections::BTreeMap;

    fn unit(id: u32, kind: &str, x: f64, z: f64) -> UnitObservation {
        let catalog = Catalog::compiled().unwrap();
        let d = catalog.unit(kind).unwrap();
        UnitObservation {
            id,
            kind: kind.into(),
            owner: 0,
            at: Point { x, z },
            hp: number(d, "models").unwrap() * number(d, "hpPer").unwrap(),
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
    fn fixture() -> Input {
        let units = vec![
            unit(1, "rifle", 110.0, 80.0),
            unit(2, "rifle", 110.0, 80.0),
            unit(3, "rifle", 110.0, 80.0),
            unit(10, "hq", 20.0, 80.0),
        ];
        let facts = units
            .iter()
            .map(|u| {
                (
                    u.id,
                    UnitFacts {
                        built: 1.0,
                        ..UnitFacts::default()
                    },
                )
            })
            .collect();
        Input {
            width: 120,
            height: 100,
            tick: 4000,
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
                    spawn: Point { x: 220.0, z: 180.0 },
                },
                PlayerObservation {
                    team: 0,
                    out: false,
                    spawn: Point { x: 20.0, z: 180.0 },
                },
            ],
            units,
            points: vec![],
            flags: vec![0; 12000],
            chars: vec![b'.'; 12000],
            manpower: 1000.0,
            research: None,
            runtime: Some(Context {
                units: facts,
                ..Context::default()
            }),
        }
    }
    fn terrain(input: &Input) -> Game {
        Game {
            w: input.width,
            h: input.height,
            flags: input.flags.clone(),
            height: None,
            house: vec![0; input.flags.len()],
            chars: input.chars.clone(),
            ground: None,
            objects: None,
            wear: None,
            cell_hp: vec![],
            trench_front: None,
            tough: None,
            wet: None,
            reveal: false,
            wrecks: vec![],
            tick: input.tick,
            vision_tick: None,
            terrain_version: 0,
            mode: None,
            naval: false,
            weather: None,
            rain: 0.0,
            players: vec![],
            units: BTreeMap::new(),
            smokes: vec![],
            strikes: vec![],
            sections: BTreeMap::new(),
        }
    }
    fn propose(input: Input, budget: f64, sees: impl Fn(Point) -> bool) -> Option<Forward> {
        let ground = terrain(&input);
        let observation = Observation::new(input).unwrap();
        let available: Vec<_> = observation
            .input()
            .units
            .iter()
            .filter(|u| u.owner == 0 && u.id < 10)
            .collect();
        plan(&observation, &available, budget, &ground, sees).unwrap()
    }
    fn location(forward: &Forward) -> Point {
        Point {
            x: forward.command["x"].as_f64().unwrap(),
            z: forward.command["z"].as_f64().unwrap(),
        }
    }

    #[test]
    fn intent_ai53_forward_hq_is_twenty_metres_behind_army_with_stable_walking_crew() {
        let mut input = fixture();
        input.units[0].path.push(Point { x: 120.0, z: 80.0 });
        let forward = propose(input, 200.0, |_| true).unwrap();
        assert_eq!(forward.unit, 1);
        assert_eq!(forward.cost, 200.0);
        assert_eq!(forward.command["t"], "build");
        assert_eq!(forward.command["kind"], "hq");
        assert_eq!(forward.command["ids"], serde_json::json!([1]));
        assert_eq!(location(&forward), Point { x: 89.0, z: 79.0 });
        assert!(distance(location(&forward), Point { x: 90.0, z: 80.0 }) <= CELL);
        let mut input = fixture();
        for (u, x) in input.units.iter_mut().take(3).zip([116.0, 112.0, 102.0]) {
            u.at.x = x;
        }
        assert_eq!(propose(input, 200.0, |_| true).unwrap().unit, 3);
    }

    #[test]
    fn intent_ai53_forward_hq_requires_far_army_budget_and_finished_one_or_two_hqs() {
        for case in 0..10 {
            let mut input = fixture();
            let mut budget = 200.0;
            match case {
                0 => {
                    input.units.remove(2);
                }
                1 => {
                    for u in input.units.iter_mut().take(3) {
                        u.at.x = 89.0;
                    }
                }
                2 => {
                    input
                        .runtime
                        .as_mut()
                        .unwrap()
                        .units
                        .get_mut(&10)
                        .unwrap()
                        .built = 0.999;
                }
                3 => {
                    for id in [11, 12] {
                        input.units.push(unit(id, "hq", 20.0, id as f64 * 10.0));
                        input.runtime.as_mut().unwrap().units.insert(
                            id,
                            UnitFacts {
                                built: 1.0,
                                ..UnitFacts::default()
                            },
                        );
                    }
                }
                4 => {
                    input.units.pop();
                }
                5 => {
                    budget = 199.999;
                }
                6 => {
                    input.runtime.as_mut().unwrap().mode = Mode::Assault;
                    input.runtime.as_mut().unwrap().defender = true;
                }
                7 => {
                    input.runtime.as_mut().unwrap().mode = Mode::Horde;
                    input.runtime.as_mut().unwrap().horde = true;
                }
                8 => {
                    input.units[2].retreating = true;
                }
                _ => {
                    input.units[2] = unit(3, "fighter", 110.0, 80.0);
                }
            }
            assert!(propose(input, budget, |_| true).is_none(), "case {case}");
        }
        let mut input = fixture();
        for u in input.units.iter_mut().take(3) {
            u.at.x = 90.0;
        }
        assert!(propose(input, 200.0, |_| true).is_some());
        let mut input = fixture();
        input.units.push(unit(11, "hq", 20.0, 140.0));
        input.runtime.as_mut().unwrap().units.insert(
            11,
            UnitFacts {
                built: 1.0,
                ..UnitFacts::default()
            },
        );
        assert!(propose(input, 200.0, |_| true).is_some());
    }

    #[test]
    fn intent_ai53_forward_hq_obeys_existing_work_and_building_repairs() {
        for case in 0..4 {
            let mut input = fixture();
            match case {
                0 => {
                    input.units[1].build = true;
                }
                1 => {
                    input.units[3].hp *= 0.799;
                }
                2 => {
                    input.units.push(unit(11, "barracks", 20.0, 120.0));
                    input.runtime.as_mut().unwrap().units.insert(
                        11,
                        UnitFacts {
                            built: 0.5,
                            ..UnitFacts::default()
                        },
                    );
                }
                _ => {
                    for u in input.units.iter_mut().take(3) {
                        u.dig = true;
                    }
                }
            }
            assert!(propose(input, 200.0, |_| true).is_none(), "case {case}");
        }
        let mut input = fixture();
        input.units[3].hp *= 0.8;
        assert!(propose(input, 200.0, |_| true).is_some());
    }

    #[test]
    fn intent_ai53_forward_hq_requires_visible_clear_ground_and_avoids_delivered_troops() {
        assert!(propose(fixture(), 200.0, |_| false).is_none());
        for x in [97.0, 105.0] {
            let expected = Point { x, z: 79.0 };
            let forward = propose(fixture(), 200.0, |at| at == expected).unwrap();
            assert_eq!(location(&forward), expected);
        }
        let mut input = fixture();
        let mut enemy = unit(20, "rifle", 90.0, 80.0);
        enemy.owner = 1;
        input.units.push(enemy);
        assert!(propose(input, 200.0, |_| true).is_none());
        let mut input = fixture();
        let mut boundary_enemy = unit(20, "rifle", 59.0, 79.0);
        boundary_enemy.owner = 1;
        input.units.push(boundary_enemy);
        assert_eq!(
            location(&propose(input, 200.0, |_| true).unwrap()),
            Point { x: 89.0, z: 79.0 }
        );
        let mut input = fixture();
        let mut ally = unit(20, "rifle", 89.0, 79.0);
        ally.owner = 2;
        input.units.push(ally);
        let occupied = terrain(&input).cell_of(89.0, 79.0).unwrap();
        let ground = terrain(&input);
        let forward = propose(input, 200.0, |_| true).unwrap();
        let at = location(&forward);
        let top_left = ground.cell_of(at.x - 3.0, at.z - 3.0).unwrap();
        assert!(!footprint(&ground, top_left, 3).unwrap().contains(&occupied));
        let mut blocked = fixture();
        blocked.flags.fill(MOVE);
        assert!(propose(blocked, 200.0, |_| true).is_none());
    }

    #[test]
    fn intent_ai53_forward_hq_preserves_lanes_points_and_spawns() {
        for obstacle in 0..3 {
            let mut input = fixture();
            let at = Point { x: 89.0, z: 79.0 };
            let radius = match obstacle {
                0 => {
                    input.points.push(PointObservation {
                        at,
                        vp: 1.0,
                        mp: 1.0,
                        owner: None,
                        cut: false,
                        locked: false,
                    });
                    10.0
                }
                1 => {
                    input.players[2].spawn = at;
                    6.0
                }
                _ => {
                    let mut building = unit(20, "hq", at.x, at.z);
                    building.owner = 2;
                    input.units.push(building);
                    10.0
                }
            };
            let forward = propose(input, 200.0, |_| true).unwrap();
            assert!(
                distance(location(&forward), at) >= radius,
                "obstacle {obstacle}"
            );
        }
    }
}
