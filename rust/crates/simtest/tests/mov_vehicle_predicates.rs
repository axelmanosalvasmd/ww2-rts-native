//! Complete selected lab journeys through admitted ticks and recorded ground orders.
//! Source recording and execution require Root's central slot. No runtime adapters.
use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufWriter, Write};

use serde_json::{Value, json};
use ww2_core::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::{MOVE, Point, Pose, VBLOCK},
    grid::SpatialGrid,
    map::AuthoritativeMap,
    match_commands::Command,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    match_tick::TickPathConfig,
    math,
    orders::OrderKey,
    recipient_snapshot::SnapshotCache,
    terrain,
    unit_publication::{PublicationParts, StagedUnit, prepare_publication},
    vehicle::{angle_delta, vehicle_position_clear},
    weather::WeatherChoice,
    world::CellChange,
};

const TYPES: [&str; 11] = [
    "tank",
    "medium",
    "tankdestroyer",
    "tiger",
    "churchill",
    "armoredcar",
    "halftrack",
    "flaktrack",
    "rocket",
    "rifle",
    "at",
];
const SCENARIOS: [&str; 7] = [
    "clearance",
    "corner",
    "cliff",
    "opposing",
    "parked",
    "road",
    "queue",
];

fn number(v: &Value) -> f64 {
    v.as_f64().expect("finite fixture number")
}
fn bits(n: f64) -> Value {
    json!(format!("{:016x}", n.to_bits()))
}
fn point(p: Option<Point>) -> Value {
    p.map_or(Value::Null, |p| json!([bits(p.x), bits(p.z)]))
}
fn distance(a: Point, b: Point) -> f64 {
    math::hypot(a.x - b.x, a.z - b.z)
}
fn snapshot(m: &Match, extended: bool) -> Value {
    json!({"tick":m.world.game.tick,"units":m.world.game.units.values().map(|u| {
        let c = &m.commands.units[&u.id];
        let mut row = json!({"id":u.id,"x":bits(u.x),"z":bits(u.z),"rot":bits(u.rot),
            "speed":bits(u.move_speed),"path":u.path.iter().map(|p|point(Some(*p))).collect::<Vec<_>>(),
            "goal":point(u.world_goal),"orders":c.orders.len(),"reverse":point(u.reverse),
            "clearance":u.clearance.is_some(),"outcome":c.outcome.map(|o|o.name())});
        if extended {
            row["kind"] = json!(m.rules.kinds[u.kind].name);
            row["hp"] = bits(u.hp);
            row["routeEnd"] = point(c.route_end);
        }
        row
    }).collect::<Vec<_>>()})
}

// Factory inputs from pinned movement-lab-scenarios.js, not recorded terrain.
fn original_lab_map(scenario: &str) -> Value {
    let mut rows = vec![vec![b'.'; 64]; 64];
    match scenario {
        "wall-arrival" => rows[30][30] = b'B',
        "obstacle" | "wreck" => {},
        "closure" => {
            for (y, row) in rows.iter_mut().enumerate() {
                for cell in &mut row[29..=35] {
                    *cell = if (27..=29).contains(&y) { b'=' }
                        else if (42..=46).contains(&y) { b'F' } else { b'W' };
                }
            }
        },
        _ => panic!("not an owned original map: {scenario}"),
    }
    json!({"name":"Movement lab","w":64,"h":64,
        "rows":rows.into_iter().map(|row|String::from_utf8(row).unwrap()).collect::<Vec<_>>(),
        "spawns":[{"x":8,"y":8},{"x":55,"y":55}],
        "points":[{"id":"lab-binding","x":4,"y":59}]})
}

fn original_lab_commands(scenario: &str, subject: u32) -> Vec<Value> {
    let target = match scenario {
        "wall-arrival" => json!([61.3,57.9]),
        "obstacle" | "wreck" => json!([100,50.27]),
        "closure" => json!([92,57]),
        _ => panic!("not an owned original command: {scenario}"),
    };
    vec![json!({"tick":2,"owner":0,"sequence":1,
        "command":{"t":"move","orders":[[subject,target[0],target[1]]]},
        "result":{"absent":true}})]
}

fn make(case: &Value) -> Match {
    let authored = case["authoredOriginalLab"] == true;
    let map_input = if authored {
        let input = original_lab_map(case["scenario"].as_str().unwrap());
        assert_eq!(case["map"], input, "complete original terrain, spawns and disclosed objective binding");
        input
    } else { case["map"].clone() };
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&map_input).unwrap())
        .expect("authored map must pass real map admission");
    let mut m = Match::new(
        &map,
        MatchOptions {
            seed: 7301,
            mode: Mode::Conquest,
            players: vec![
                PlayerConfig {
                    name: "Lab".into(),
                    team: 0,
                    faction: Faction::Usa,
                },
                PlayerConfig {
                    name: "Observer".into(),
                    team: 1,
                    faction: Faction::Germany,
                },
            ],
            shuffle: false,
            army: ArmySize::Standard,
            weather: WeatherChoice::Map,
            weather_seed: Some(1),
            living_weather: false,
            map_key: None,
            tech: false,
            supply: false,
            logistics: false,
            sandbox: false,
        },
    )
    .expect("canonical Conquest startup");
    if authored {
        let mut writer = Some(BufWriter::new(std::fs::OpenOptions::new().append(true)
            .open(std::env::var_os("WW2_MOV24_27_REPORT").expect("private report required")).unwrap()));
        record_frame(&mut writer, &format!("{}/{}/constructor", case["type"], case["scenario"]),
            &json!({"units":format!("{:?}",m.world.game.units),"controls":format!("{:?}",m.commands.units),
                "nextId":m.next_unit_id,"environmentSeed":m.start.environment_seed,"wind":m.start.wind}));
    }
    // Keep actual constructor bases and all linked owners. The Source adapter
    // removes only initial mobile forces, then publishes canonical lab births.
    let kinds = &m.rules.kinds;
    m.world
        .game
        .units
        .retain(|_, unit| kinds[unit.kind].structure);
    let retained: BTreeSet<_> = m.world.game.units.keys().copied().collect();
    m.grid = SpatialGrid::new(&m.world.game.units, 16.0);
    m.commands.units.retain(|id, _| retained.contains(id));
    m.recruit_metadata.retain(|id, _| retained.contains(id));
    m.combat = Default::default();
    m.firing = Default::default();
    m.abilities = Default::default();
    m.recovery = Default::default();
    for &id in &retained {
        m.combat.initialize(id);
        m.firing.initialize(id);
        m.abilities.initialize(id);
        m.recovery.initialize(id);
    }
    m.start.points.clear();
    m.capture_ticks.clear();
    if authored {
        // Inputs declared by createMovementCase, never a Source after-image.
        assert_eq!(case["environment"]["seed"], 7301);
        assert_eq!(case["environment"]["wet"], Value::Null);
        m.start.environment_seed = 7301;
        m.world.game.wet = None;
    } else {
        m.start.environment_seed = case["environment"]["seed"].as_u64().unwrap() as u32;
        m.start.wind = (
            number(&case["environment"]["wind"][0]),
            number(&case["environment"]["wind"][1]),
        );
        m.world.game.wet = case["environment"]["wet"].as_f64();
    }
    m.initialize_tick_scene(
        0,
        TickPathConfig {
            budget: if authored { 4096 } else { case["environment"]["pathBudget"].as_u64().unwrap() as usize },
            retry_limit: if authored { 3 } else { case["environment"]["pathRetryLimit"].as_u64().unwrap() as u32 },
        },
    )
    .expect("declared Source path budget and retry inputs");
    let initial = case["initial"].as_array().unwrap();
    if authored {
        assert_eq!(initial.len(), retained.len() + 1, "one original subject and every constructor base");
        assert_eq!(case["environment"]["pathBudget"], 4096);
        assert_eq!(case["environment"]["pathRetryLimit"], 3);
    }
    assert_eq!(
        m.next_unit_id,
        case["subject"].as_u64().unwrap() as u32,
        "Source/native constructor allocators"
    );
    assert_eq!(
        retained.len(),
        initial
            .iter()
            .filter(|row| row["structure"] == true)
            .count()
    );
    let mut staged = Vec::new();
    for row in initial {
        let id = row["id"].as_u64().unwrap() as u32;
        let owner = row["owner"].as_u64().unwrap() as usize;
        let kind = m.rules.kind(row["kind"].as_str().unwrap()).unwrap();
        if row["structure"] == true {
            let u = &m.world.game.units[&id];
            assert_eq!(u.kind, kind);
            assert_eq!(u.owner, owner);
            assert_eq!(bits(u.x), bits(number(&row["x"])));
            assert_eq!(bits(u.z), bits(number(&row["z"])));
            assert_eq!(bits(u.hp), bits(number(&row["hp"])));
            assert_eq!(
                json!(u.cells),
                row["cells"],
                "constructor base footprint identity"
            );
            continue;
        }
        assert_eq!(id, m.next_unit_id + staged.len() as u32);
        let (at, rot) = if authored {
            assert_eq!(owner, 0);
            assert_eq!(row["kind"], case["type"]);
            original_lab_birth(case["scenario"].as_str().unwrap())
        } else {
            (Point { x: number(&row["x"]), z: number(&row["z"]) }, number(&row["rot"]))
        };
        let mut draft = StagedUnit::ordinary(
            &m.world.game,
            &m.rules,
            owner,
            kind,
            id,
            (retained.len() + staged.len()) as u32,
            m.start.players[owner].spawn,
            Some(at),
        )
        .expect("canonical scatter and actor/control initialization");
        draft.unit.rot = rot;
        if authored {
            draft.unit.hold_pos = true;
            draft.control.hold_fire = true;
            draft.control.auto_cast = false;
            draft.control.auto_retreat = false;
            for (actual, expected) in [(draft.unit.x, &row["x"]), (draft.unit.z, &row["z"]),
                (draft.unit.rot, &row["rot"]), (draft.unit.hp, &row["hp"]), (draft.control.aim, &row["aim"])] {
                assert_eq!(bits(actual), bits(number(expected)), "authored birth versus actual Source constructor-derived actor");
            }
            assert_eq!(row["holdPos"], true);
            assert_eq!(row["holdFire"], true);
            assert_eq!(row["auto"], false);
            assert_eq!(row["autoRetreat"], false);
        } else {
            draft.unit.hold_pos = row["holdPos"].as_bool().unwrap();
            draft.control.aim = number(&row["aim"]);
            draft.control.hold_fire = row["holdFire"].as_bool().unwrap();
            draft.control.auto_cast = row["auto"].as_bool().unwrap();
            draft.control.auto_retreat = row["autoRetreat"].as_bool().unwrap();
        }
        staged.push(draft);
    }
    prepare_publication(
        PublicationParts {
            units: &mut m.world.game.units,
            grid: &mut m.grid,
            controls: &mut m.commands.units,
            recruitment: &mut m.recruit_metadata,
            next_id: &mut m.next_unit_id,
        },
        &m.rules,
        m.start.players.len(),
        staged,
    )
    .expect("real identity, allocator, geometry and control admission")
    .publish_with(|unit, _| {
        m.combat.initialize(unit.id);
        m.firing.initialize(unit.id);
        m.abilities.initialize(unit.id);
        m.recovery.initialize(unit.id);
    });
    m.validate_tick_state()
        .expect("complete real tick admission before first step");
    m
}

// Original authored lab inputs, independently selected from the pinned factory.
fn original_lab_birth(scenario: &str) -> (Point, f64) {
    match scenario {
        "wall-arrival" => (Point { x: 54.0, z: 50.0 }, 0.0),
        "obstacle" | "wreck" => (Point { x: 50.13, z: 50.27 }, 0.15),
        "closure" => (Point { x: 40.0, z: 57.0 }, 0.0),
        _ => panic!("not an authorized original lab input: {scenario}"),
    }
}

fn original_lab_mutations(scenario: &str) -> Vec<Value> {
    match scenario {
        "wall-arrival" => vec![],
        "obstacle" | "wreck" => (22..=28).map(|y| json!({"tick":42,"cell":y*64+36,
            "change":{"object":if scenario == "wreck" { "Q" } else { "Y" }}})).collect(),
        "closure" => (27..=29).flat_map(|y| (29..=35).map(move |x|
            json!({"tick":22,"cell":y*64+x,"change":{"ground":"W","object":"."}}))).collect(),
        _ => panic!("unexpected original mutation input: {scenario}"),
    }
}

fn record_frame(writer: &mut Option<BufWriter<std::fs::File>>, label: &str, frame: &Value) {
    if let Some(writer) = writer {
        serde_json::to_writer(&mut *writer, &json!({"case":label,"actual":frame})).unwrap();
        writer.write_all(b"\n").unwrap();
        writer.flush().unwrap();
    }
}

#[derive(Default)]
struct Journey {
    stationary_turn: f64,
    max_stationary_turn: f64,
    visited: usize,
}

// Independent trajectory reader from the selected Source runner. It visits
// geometric grid crossings rather than calling the solver's cliff predicate.
fn crosses_cliff(m: &Match, from: Point, to: Point) -> bool {
    let Some(heights) = &m.world.game.height else {
        return false;
    };
    let mut crossings = vec![0.0, 1.0];
    for (a, b) in [(from.x, to.x), (from.z, to.z)] {
        let delta = b - a;
        if delta == 0.0 {
            continue;
        }
        let mut edge = (math::min(a, b) / 2.0).floor() + 1.0;
        while edge <= (math::max(a, b) / 2.0).floor() {
            let t = (edge * 2.0 - a) / delta;
            if t > 0.0 && t < 1.0 {
                crossings.push(t);
            }
            edge += 1.0;
        }
    }
    crossings.sort_by(f64::total_cmp);
    let height = |t| {
        let x = from.x + (to.x - from.x) * t;
        let z = from.z + (to.z - from.z) * t;
        let cell = m.world.game.cell_of(x, z).expect("journey stays on map");
        i32::from(heights[cell])
    };
    let mut previous = height(0.0);
    for pair in crossings.windows(2) {
        let current = height((pair[0] + pair[1]) / 2.0);
        if (current - previous).abs() > 1 {
            return true;
        }
        previous = current;
    }
    (height(1.0) - previous).abs() > 1
}

fn delivered(m: &Match, ids: &[u32]) -> Value {
    let cache = SnapshotCache::new(m).expect("canonical recipient rows");
    let recipient = cache.recipient(0).expect("real owned recipient");
    // Only the selected actual unit-row fields are used. This does not serialize
    // the projection's explicit missing capabilities as a full wire envelope.
    let rows = recipient.fields["units"].as_array().unwrap();
    json!(
        ids.iter()
            .map(|id| {
                let row = rows
                    .iter()
                    .find(|row| row[0].as_u64() == Some(u64::from(*id)))
                    .unwrap();
                json!([
                    row[0],
                    row[3],
                    row[4],
                    row[5],
                    row.get(15).cloned().unwrap_or(json!(0))
                ])
            })
            .collect::<Vec<_>>()
    )
}

fn delivered_bits(rows: &Value) -> Value {
    // Source has one Number type; JSON prints integral doubles without `.0`.
    // Preserve row shape, integer identity and every represented value's bits.
    Value::Array(
        rows.as_array()
            .unwrap()
            .iter()
            .map(|row| {
                let row = row.as_array().unwrap();
                assert_eq!(row.len(), 5);
                json!([
                    row[0].as_u64().unwrap(),
                    bits(number(&row[1])),
                    bits(number(&row[2])),
                    bits(number(&row[3])),
                    bits(number(&row[4]))
                ])
            })
            .collect(),
    )
}

fn replay(case: &Value) {
    let label = format!(
        "{}/{}",
        case["type"].as_str().unwrap(),
        case["scenario"].as_str().unwrap()
    );
    let scenario = case["scenario"].as_str().unwrap();
    let authored = case["authoredOriginalLab"] == true;
    let mut writer = authored.then(|| BufWriter::new(std::fs::OpenOptions::new().append(true)
        .open(std::env::var_os("WW2_MOV24_27_REPORT").expect("private report required")).unwrap()));
    let mutations = if authored { original_lab_mutations(scenario) } else { vec![] };
    if authored {
        assert_eq!(case["mutations"], json!(mutations), "retain the complete original timed terrain inputs");
        let seconds = match scenario { "wall-arrival" => 30, "obstacle" | "wreck" => 50, "closure" => 60, _ => unreachable!() };
        assert_eq!(case["ticks"], seconds * 20 + 1);
        assert_eq!(case["frames"].as_array().unwrap().len(), seconds as usize * 20 + 2);
    }
    let mut m = make(case);
    let ids: Vec<_> = m.world.game.units.keys().copied().collect();
    let first = case["subject"].as_u64().unwrap() as u32;
    assert_eq!(
        m.rules.kinds[m.world.game.units[&first].kind].name,
        case["type"].as_str().unwrap(),
        "selected subject archetype"
    );
    let anchors: BTreeMap<_, _> = m
        .world
        .game
        .units
        .values()
        .map(|u| (u.id, u.at()))
        .collect();
    let mut journeys: BTreeMap<u32, Journey> =
        ids.iter().map(|id| (*id, Journey::default())).collect();
    let mut checkpoints = case["checkpoints"].as_array().unwrap().iter().peekable();
    let command_inputs = if authored {
        let inputs = original_lab_commands(scenario, first);
        assert_eq!(case["commands"], json!(inputs), "all original command inputs and actual Source acceptance");
        inputs
    } else { case["commands"].as_array().unwrap().clone() };
    let mut commands = command_inputs.iter().peekable();
    let mut corner_finish = None;
    let mut milestones = BTreeMap::new();
    let initial_frame = snapshot(&m, authored);
    record_frame(&mut writer, &label, &initial_frame);
    if authored { assert_eq!(initial_frame, case["frames"][0], "{label}: initial actual frame"); }
    assert_eq!(
        initial_frame,
        *checkpoints.next().unwrap(),
        "{label}: initial canonical state"
    );
    for tick in 1..=case["ticks"].as_u64().unwrap() {
        for row in mutations.iter().filter(|row| row["tick"].as_u64() == Some(tick)) {
            let character = |key| row["change"][key].as_str().map(|s| {
                assert_eq!(s.len(), 1);
                s.as_bytes()[0]
            });
            m.world.mutate_cell(row["cell"].as_u64().unwrap() as usize, CellChange {
                ground: character("ground"), object: character("object"), ..CellChange::default()
            }).expect("actual authoritative original terrain event");
        }
        while commands
            .peek()
            .is_some_and(|row| row["tick"].as_u64() == Some(tick))
        {
            let row = commands.next().unwrap();
            let command = Command::from_json(&row["command"]).unwrap();
            m.enqueue_command(
                OrderKey {
                    tick,
                    issuer: row["owner"].as_u64().unwrap() as u32,
                    sequence: row["sequence"].as_u64().unwrap(),
                },
                command,
            )
            .unwrap();
        }
        let before: BTreeMap<_, _> = m
            .world
            .game
            .units
            .values()
            .map(|u| {
                (
                    u.id,
                    Pose {
                        x: u.x,
                        z: u.z,
                        rot: u.rot,
                    },
                )
            })
            .collect();
        let report = m
            .advance_tick()
            .unwrap_or_else(|error| panic!("{label}: tick {tick}: {error:?}"));
        let actual_frame = snapshot(&m, authored);
        record_frame(&mut writer, &label, &actual_frame);
        if authored { assert_eq!(actual_frame, case["frames"][tick as usize], "{label}: actual frame at tick {tick}"); }
        assert!(
            report.commands.iter().all(|result| result.denial.is_none()),
            "{label}: every Source-accepted ground command must admit at tick {tick}"
        );
        assert_eq!(
            m.world.game.units.len(),
            ids.len(),
            "{label}: unexpected actor retirement"
        );
        for u in m.world.game.units.values() {
            let prior = before[&u.id];
            let prior_at = Point {
                x: prior.x,
                z: prior.z,
            };
            let moved = distance(prior_at, u.at());
            let turn = angle_delta(prior.rot, u.rot).abs();
            let journey = journeys.get_mut(&u.id).unwrap();
            journey.stationary_turn = if moved < 0.001 {
                journey.stationary_turn + turn
            } else {
                0.0
            };
            journey.max_stationary_turn =
                math::max(journey.max_stationary_turn, journey.stationary_turn);
            let kind = &m.rules.kinds[u.kind];
            if kind.ground_vehicle() {
                assert!(
                    vehicle_position_clear(
                        &m.world.game,
                        Pose {
                            x: u.x,
                            z: u.z,
                            rot: u.rot
                        },
                        kind,
                        MOVE | VBLOCK
                    ),
                    "{label}: hull crossed blocked terrain at tick {tick}, actor {}",
                    u.id
                );
            }
            assert!(
                !crosses_cliff(&m, prior_at, u.at()),
                "{label}: trajectory crossed cliff at tick {tick}"
            );
            if scenario == "parked" && u.id != first {
                assert!(
                    distance(u.at(), anchors[&u.id]) <= 0.01,
                    "{label}: protected blocker displaced"
                );
            }
            if scenario == "queue" && u.id == first {
                let visits = [
                    Point { x: 70.0, z: 50.27 },
                    Point { x: 70.0, z: 75.0 },
                    Point { x: 45.0, z: 75.0 },
                ];
                if let Some(goal) = visits.get(journey.visited)
                    && distance(u.at(), *goal) < 0.25
                    && u.move_speed.abs() < 0.1
                {
                    journey.visited += 1;
                }
            }
        }
        if scenario == "corner" && case["type"] == "tank" && tick > 1 {
            let u = &m.world.game.units[&first];
            if u.path.is_empty()
                && u.world_goal.is_none()
                && u.move_speed.abs() < 0.1
                && distance(u.at(), Point { x: 64.0, z: 48.0 }) < 0.5
            {
                corner_finish.get_or_insert(tick);
            }
        }
        if scenario == "engine-braking" {
            for milestone in case["milestones"].as_array().unwrap() {
                if milestone["tick"].as_u64() == Some(tick) {
                    let engine_ids = case["engineActors"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|id| id.as_u64().unwrap() as u32)
                        .collect::<Vec<_>>();
                    assert_eq!(engine_ids.len(), 2);
                    assert_eq!(engine_ids[0], first);
                    assert_eq!(
                        m.rules.kinds[m.world.game.units[&engine_ids[1]].kind].name,
                        "rifle"
                    );
                    let rows = delivered(&m, &engine_ids);
                    assert_eq!(
                        delivered_bits(&rows),
                        delivered_bits(&milestone["rows"]),
                        "{label}: actual recipient row at tick {tick}"
                    );
                    let recipient = SnapshotCache::new(&m).unwrap().recipient(0).unwrap();
                    let plan = recipient.fields["plans"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|row| row[0].as_u64() == Some(u64::from(first)))
                        .unwrap();
                    assert_eq!(
                        plan[1], milestone["plan"],
                        "{label}: actual owned plan at tick {tick}"
                    );
                    milestones.insert(milestone["label"].as_str().unwrap().to_owned(), rows);
                }
            }
        }
        if checkpoints
            .peek()
            .is_some_and(|row| row["tick"].as_u64() == Some(tick))
        {
            assert_eq!(
                snapshot(&m, authored),
                *checkpoints.next().unwrap(),
                "{label}: Source checkpoint tick {tick}"
            );
        }
    }
    assert!(
        commands.next().is_none() && checkpoints.next().is_none(),
        "{label}: unread fixture work"
    );
    assert!(m.tick_state.fault().is_none(), "{label}: runner fault");
    if scenario == "engine-braking" {
        let accelerated = &milestones["accelerated"];
        assert!(
            number(&accelerated[0][4]) > 0.0
                && number(&accelerated[0][4])
                    < m.rules.kinds[m.world.game.units[&first].kind].speed
        );
        assert!(number(&accelerated[1][1]) - 30.0 > 0.3);
        let before = &milestones["before-stop"];
        let braking = &milestones["braking"];
        let stopped = &milestones["stopped"];
        assert!(number(&braking[0][1]) > number(&before[0][1]));
        assert!(number(&braking[0][4]) > 0.0 && number(&braking[0][4]) < number(&before[0][4]));
        assert_eq!(number(&stopped[0][4]), 0.0);
        let profile = m.rules.kinds[m.world.game.units[&first].kind]
            .vehicle
            .as_ref()
            .unwrap();
        let speed = number(&before[0][4]);
        assert!(
            number(&stopped[0][1]) - number(&braking[0][1])
                <= speed * speed / (2.0 * profile.braking) + 1.0
        );
        let u = &m.world.game.units[&first];
        assert!(
            u.path.is_empty()
                && u.world_goal.is_none()
                && m.commands.units[&first].orders.is_empty()
        );
        return;
    }
    assert_eq!(
        case["verdict"]["failures"],
        json!([]),
        "{label}: Source verdict"
    );
    for u in m.world.game.units.values() {
        let journey = &journeys[&u.id];
        assert!(
            journey.max_stationary_turn <= std::f64::consts::PI * 1.9,
            "{label}: stationary spin"
        );
        if m.rules.kinds[u.kind].structure || scenario == "parked" && u.id != first {
            continue;
        }
        let goal = match scenario {
            "clearance" => Point {
                x: 69.3989857,
                z: 62.6076599,
            },
            "corner" => Point { x: 64.0, z: 48.0 },
            "cliff" => Point { x: 92.0, z: 57.0 },
            "opposing" => Point {
                x: if u.id == first { 80.0 } else { 40.0 },
                z: 57.0,
            },
            "parked" => Point { x: 85.0, z: 57.0 },
            "road" => Point { x: 87.0, z: 73.0 },
            "queue" => Point { x: 45.0, z: 75.0 },
            "wall-arrival" => Point { x: 61.3, z: 57.9 },
            "obstacle" | "wreck" => Point { x: 100.0, z: 50.27 },
            "closure" => Point { x: 92.0, z: 57.0 },
            _ => panic!("unexpected scenario {scenario}"),
        };
        let destination = m.commands.units[&u.id].route_end.unwrap_or(goal);
        assert!(
            distance(u.at(), destination) <= 0.5
                && u.path.is_empty()
                && u.world_goal.is_none()
                && u.move_speed.abs() <= 0.1,
            "{label}: actor {} did not finish selected journey",
            u.id
        );
        if scenario == "queue" {
            assert_eq!(
                journey.visited, 3,
                "{label}: all destinations visited in order"
            );
        }
    }
    if scenario == "corner" && case["type"] == "tank" {
        assert!(
            corner_finish.is_some_and(|tick| tick <= 181),
            "selected light-tank corner must finish within nine seconds"
        );
    }
}

#[test]
#[ignore = "requires the private complete MOV24/27 Source capture and Root execution grant"]
fn original_all_eleven_wall_arrivals_and_dynamic_recovery_journeys() {
    let source = std::env::var_os("WW2_MOV24_27_TRACE").expect("private complete Source packet required");
    let fixture: Value = serde_json::from_slice(&std::fs::read(source).unwrap()).unwrap();
    assert_eq!(fixture["format"], "mov24-27-original-lab-v1");
    assert_eq!(fixture["reference"], "e4e579a838aabae3b1d9dadbc17a0a87147bc52d");
    assert_eq!(fixture["node"], "v24.21.0");
    assert_eq!(fixture["arch"], "x64");
    assert_eq!(fixture["failure"], Value::Null);
    assert_eq!(fixture["incomplete"], Value::Null);
    assert_eq!(fixture["exit"], 0);
    assert_eq!(fixture["types"], json!(TYPES));
    let scenarios = ["wall-arrival", "obstacle", "wreck", "closure"];
    assert_eq!(fixture["scenarios"], json!(scenarios));
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 44);
    let observed = cases.iter().map(|case| (case["type"].as_str().unwrap(), case["scenario"].as_str().unwrap())).collect::<BTreeSet<_>>();
    let expected = TYPES.iter().flat_map(|kind| scenarios.iter().map(move |scenario| (*kind,*scenario))).collect::<BTreeSet<_>>();
    assert_eq!(observed, expected, "all eleven original types in every complete original journey");
    let output = std::fs::OpenOptions::new().write(true).create_new(true)
        .open(std::env::var_os("WW2_MOV24_27_REPORT").expect("fresh private native report required")).unwrap();
    let mut writer = BufWriter::new(output);
    serde_json::to_writer(&mut writer, &json!({"format":"mov24-27-native-original-lab-v1",
        "nativeHead":std::env::var("WW2_MOV24_27_NATIVE_HEAD").expect("frozen producing head required"),
        "sourceHead":fixture["sourceHead"],"scope":"existing canonical-birth lab setup; independently authored inputs and actual ordinary command/tick/terrain actuators; no production-roster equivalence"})).unwrap();
    writer.write_all(b"\n").unwrap();
    writer.flush().unwrap();
    for case in cases {
        assert_eq!(case["authoredOriginalLab"], true);
        let scenario = case["scenario"].as_str().unwrap();
        assert_eq!(case["map"], original_lab_map(scenario));
        assert_eq!(case["commands"], json!(original_lab_commands(scenario,
            case["subject"].as_u64().unwrap().try_into().unwrap())),
            "complete unchanged original command input and actual acceptance");
        assert_eq!(case["verdict"]["passed"], true);
        assert_eq!(case["verdict"]["failures"], json!([]));
        replay(case);
    }
    assert_eq!(cases.iter().map(|case| case["ticks"].as_u64().unwrap()).sum::<u64>(), 41_844);
    assert_eq!(cases.iter().map(|case| case["frames"].as_array().unwrap().len()).sum::<usize>(), 41_888);
    println!("44 original wall-arrival/recovery journeys, 41,844 actual admitted ticks, 41,888 complete recorded frame comparisons; original windows and all arrival/hull/cliff/spin checks");
}

#[test]
fn original_vehicle_selected_journeys_through_real_admission_and_ground_orders() {
    // Read at runtime so a source-only checkpoint can compile before Root runs
    // the recorder. Missing evidence fails explicitly; it is never a skipped pass.
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/mov-vehicle-predicates.json"
    ))
    .expect("Root must generate the pinned Source packet before this focused replay");
    let fixture: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        fixture["reference"],
        "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
    );
    assert_eq!(fixture["node"], "v24.21.0");
    assert_eq!(fixture["arch"], "x64");
    assert_eq!(fixture["types"], json!(TYPES));
    assert_eq!(fixture["scenarios"], json!(SCENARIOS));
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 78, "77 lab journeys and one engine sequence");
    let observed: BTreeSet<_> = cases
        .iter()
        .map(|c| (c["type"].as_str().unwrap(), c["scenario"].as_str().unwrap()))
        .collect();
    let expected: BTreeSet<_> = TYPES
        .iter()
        .flat_map(|kind| SCENARIOS.iter().map(move |scenario| (*kind, *scenario)))
        .chain([("tank", "engine-braking")])
        .collect();
    assert_eq!(
        observed, expected,
        "complete selected matrix, no duplicate partial scenes"
    );
    for case in cases {
        replay(case);
    }
    let cliffs = fixture["cliffs"].as_array().unwrap();
    assert_eq!(cliffs.len(), 6);
    for (index, case) in cliffs.iter().enumerate() {
        assert_eq!(
            case["heights"],
            json!([[0, 2, 0, 0], [0, 0, 2, 0], [0, 1, 1, 2]][index / 2]),
            "selected height layout"
        );
        let (from, to) = if index % 2 == 0 {
            (json!({"x":1,"z":1}), json!({"x":3,"z":3}))
        } else {
            (json!({"x":3,"z":3}), json!({"x":1,"z":1}))
        };
        assert_eq!(case["from"], from);
        assert_eq!(case["to"], to);
        let map =
            AuthoritativeMap::from_json(&serde_json::to_vec(&cases[0]["map"]).unwrap()).unwrap();
        let mut world = ww2_core::world::World::from_map(&map).unwrap();
        // Pure 2-by-2 component input from the selected diagonal check. No tick
        // admission or route journey is attributed to this tiny component.
        world.game.w = 2;
        world.game.h = 2;
        world.game.height = Some(
            case["heights"]
                .as_array()
                .unwrap()
                .iter()
                .map(|n| n.as_i64().unwrap() as i8)
                .collect(),
        );
        let at = |v: &Value| Point {
            x: number(&v["x"]),
            z: number(&v["z"]),
        };
        assert!(
            !terrain::segment_cliff_clear(&world.game, at(&case["from"]), at(&case["to"])),
            "diagonal {index}"
        );
        assert_eq!(case["clear"], false);
    }
}
