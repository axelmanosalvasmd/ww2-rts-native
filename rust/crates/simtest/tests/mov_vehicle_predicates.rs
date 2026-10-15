//! Complete selected lab journeys through admitted ticks and recorded ground orders.
//! Source recording and execution require Root's central slot. No runtime adapters.
use std::collections::{BTreeMap, BTreeSet};

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
fn snapshot(m: &Match) -> Value {
    json!({"tick":m.world.game.tick,"units":m.world.game.units.values().map(|u| {
        let c = &m.commands.units[&u.id];
        json!({"id":u.id,"x":bits(u.x),"z":bits(u.z),"rot":bits(u.rot),
            "speed":bits(u.move_speed),"path":u.path.iter().map(|p|point(Some(*p))).collect::<Vec<_>>(),
            "goal":point(u.world_goal),"orders":c.orders.len(),"reverse":point(u.reverse),
            "clearance":u.clearance.is_some(),"outcome":c.outcome.map(|o|o.name())})
    }).collect::<Vec<_>>()})
}

fn make(case: &Value) -> Match {
    let bridge: Option<Value> = (case["scenario"] == "bridge").then(||
        serde_json::from_str(include_str!("../../../../harness/mov-bridge-inputs.json")).unwrap());
    let map_input = bridge.as_ref().map_or(&case["map"], |input| &input["map"]);
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(map_input).unwrap())
        .expect("authored map must pass real map admission");
    let mut m = Match::new(
        &map,
        MatchOptions {
            seed: bridge.as_ref().map_or(7301, |input| input["nativeSeed"].as_u64().unwrap()),
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
    if let Some(input) = &bridge {
        // The original lab explicitly declares this seed after construction.
        // Keep native constructor wind/wetness, never copy expected RNG state.
        // Source and native wind are unused in this dry, no-firing component.
        m.start.environment_seed = input["environmentSeed"].as_u64().unwrap() as u32;
        assert_eq!(json!(m.world.game.wet), case["environment"]["wet"]);
        m.initialize_tick_scene(0, TickPathConfig {
            budget: input["pathBudget"].as_u64().unwrap() as usize,
            retry_limit: input["pathRetryLimit"].as_u64().unwrap() as u32,
        }).unwrap();
    } else {
        m.start.environment_seed = case["environment"]["seed"].as_u64().unwrap() as u32;
        m.start.wind = (
            number(&case["environment"]["wind"][0]),
            number(&case["environment"]["wind"][1]),
        );
        m.world.game.wet = case["environment"]["wet"].as_f64();
        m.initialize_tick_scene(
            0,
            TickPathConfig {
                budget: case["environment"]["pathBudget"].as_u64().unwrap() as usize,
                retry_limit: case["environment"]["pathRetryLimit"].as_u64().unwrap() as u32,
            },
        )
        .expect("declared Source path budget and retry inputs");
    }
    let initial = case["initial"].as_array().unwrap();
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
        let authored = bridge.as_ref().map(|input| &input["actors"][staged.len()]);
        let (owner, kind, at, rot, aim, hold_fire, hold_pos, auto, retreat) = if let Some(input) = authored {
            let name = if input["kind"] == "$type" { case["type"].as_str().unwrap() }
                else { input["kind"].as_str().unwrap() };
            let controls = &bridge.as_ref().unwrap()["controls"];
            (input["owner"].as_u64().unwrap() as usize, m.rules.kind(name).unwrap(),
                Point { x: number(&input["at"][0]), z: number(&input["at"][1]) },
                number(&input["rot"]), 0.0,
                controls["holdFire"].as_bool().unwrap(), controls["holdPos"].as_bool().unwrap(),
                controls["auto"].as_bool().unwrap(), controls["autoRetreat"].as_bool().unwrap())
        } else {
            (owner, kind, Point { x: number(&row["x"]), z: number(&row["z"]) },
                number(&row["rot"]), number(&row["aim"]), row["holdFire"].as_bool().unwrap(),
                row["holdPos"].as_bool().unwrap(), row["auto"].as_bool().unwrap(),
                row["autoRetreat"].as_bool().unwrap())
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
        draft.unit.hold_pos = hold_pos;
        draft.control.aim = aim;
        draft.control.hold_fire = hold_fire;
        draft.control.auto_cast = auto;
        draft.control.auto_retreat = retreat;
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
    let mut commands = case["commands"].as_array().unwrap().iter().peekable();
    let mut corner_finish = None;
    let mut milestones = BTreeMap::new();
    assert_eq!(
        snapshot(&m),
        *checkpoints.next().unwrap(),
        "{label}: initial canonical state"
    );
    for tick in 1..=case["ticks"].as_u64().unwrap() {
        while commands
            .peek()
            .is_some_and(|row| row["tick"].as_u64() == Some(tick))
        {
            let row = commands.next().unwrap();
            let command_input = if scenario == "bridge" {
                let bridge_input: Value = serde_json::from_str(include_str!("../../../../harness/mov-bridge-inputs.json")).unwrap();
                let i = row["sequence"].as_u64().unwrap() as usize - 1;
                let actor = &bridge_input["actors"][i];
                let authored = json!({"t":"move", "orders":[[first + i as u32, actor["goal"][0], actor["goal"][1]]]});
                assert_eq!(row["tick"], bridge_input["commandTick"]);
                assert_eq!(row["owner"], actor["owner"]);
                assert_eq!(row["command"], authored);
                authored
            } else { row["command"].clone() };
            let command = Command::from_json(&command_input).unwrap();
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
                snapshot(&m),
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
            "bridge" => match u.id - first {
                0 => Point { x: 92.0, z: 57.0 },
                1 => Point { x: 40.0, z: 57.0 },
                2 => Point { x: 92.0, z: 53.0 },
                3 => Point { x: 40.0, z: 61.0 },
                _ => panic!("bridge lost its four original moving actors"),
            },
            "parked" => Point { x: 85.0, z: 57.0 },
            "road" => Point { x: 87.0, z: 73.0 },
            "queue" => Point { x: 45.0, z: 75.0 },
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


#[test]
fn original_bridge_all_types_preserve_the_complete_two_way_verdict() {
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/mov-bridge-predicates.json"
    )).expect("Root must grant and freeze the separate authentic bridge capture first");
    let fixture: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(fixture["reference"], "e4e579a838aabae3b1d9dadbc17a0a87147bc52d");
    assert_eq!(fixture["node"], "v24.21.0");
    assert_eq!(fixture["arch"], "x64");
    assert_eq!(fixture["types"], json!(TYPES));
    let inputs: Value = serde_json::from_str(include_str!("../../../../harness/mov-bridge-inputs.json")).unwrap();
    assert_eq!(fixture["authoredInputs"], inputs);
    assert_eq!(fixture["sourceBindings"], inputs["sourceBindings"]);
    assert_eq!(inputs["types"], json!(TYPES));
    assert_eq!(fixture["scenarios"], json!(["bridge"]));
    assert_eq!(fixture["cliffs"], json!([]));
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), TYPES.len());
    let actual: BTreeSet<_> = cases.iter().map(|c| c["type"].as_str().unwrap()).collect();
    assert_eq!(actual, TYPES.into_iter().collect::<BTreeSet<_>>());
    for case in cases {
        assert_eq!(case["scenario"], "bridge");
        assert_eq!(case["map"], inputs["map"], "complete shared authored bridge terrain");
        assert_eq!(case["verdict"]["passed"], true);
        assert_eq!(case["verdict"]["failures"], json!([]));
        assert_eq!(case["verdict"]["seconds"], 120);
        assert_eq!(case["ticks"], 2401, "initial tick plus all 120 seconds");
        let first = case["subject"].as_u64().unwrap();
        let movers: Vec<_> = case["initial"].as_array().unwrap().iter()
            .filter(|u| u["structure"] == false).collect();
        assert_eq!(movers.len(), 4, "subject, opposing AT, and two crossing infantry");
        for (i, (kind, x, z)) in [
            (case["type"].as_str().unwrap(), 40.0, 57.0),
            ("at", 86.0, 57.0), ("rifle", 40.0, 51.0), ("rifle", 92.0, 63.0),
        ].into_iter().enumerate() {
            assert_eq!(movers[i]["id"].as_u64(), Some(first + i as u64));
            assert_eq!(movers[i]["kind"], kind);
            assert_eq!(number(&movers[i]["x"]), x);
            assert_eq!(number(&movers[i]["z"]), z);
        }
        // A passing open-ground scene is not the selected narrow-crossing lab.
        assert_eq!(case["map"]["w"], 64);
        assert_eq!(case["map"]["h"], 64);
        for (y, row) in case["map"]["rows"].as_array().unwrap().iter().enumerate() {
            let row = row.as_str().unwrap().as_bytes();
            assert_eq!(row.len(), 64);
            for (x, ch) in row.iter().enumerate() {
                let expected = if (29..=35).contains(&x) {
                    if (27..=29).contains(&y) { b'=' } else { b'W' }
                } else { b'.' };
                assert_eq!(*ch, expected, "complete original bridge terrain");
            }
        }
        let commands = case["commands"].as_array().unwrap();
        assert_eq!(commands.len(), 4);
        for (i, (x, z)) in [(92, 57), (40, 57), (92, 53), (40, 61)].into_iter().enumerate() {
            assert_eq!(commands[i]["owner"], 0);
            assert_eq!(commands[i]["command"]["t"], "move");
            assert_eq!(commands[i]["command"]["orders"], json!([[first + i as u64, x, z]]));
        }
        // Reuse every original continuous hull/cliff/spin, final journey,
        // actual queue/admission and full checkpoint assertion from the old corpus.
        replay(case);
    }
}
