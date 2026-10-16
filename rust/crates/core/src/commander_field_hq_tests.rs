//! AI-50: complete HQ policy, using only delivered observations and terrain.
use super::*;
use crate::{
    commander_ai::{Input, PlayerObservation, ResearchObservation},
    commander_tactics::{Context, UnitFacts},
    economy::{Faction, TechState},
    match_commands::Command,
};

fn unit(id: u32, kind: &str, x: f64, z: f64) -> UnitObservation {
    let catalog = Catalog::compiled().unwrap();
    let definition = catalog.unit(kind).unwrap();
    UnitObservation {
        id,
        kind: kind.into(),
        owner: 0,
        at: Point { x, z },
        hp: number(definition, "models") * number(definition, "hpPer"),
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
        unit(1, "rifle", 50.0, 50.0),
        unit(2, "rifle", 56.0, 60.0),
        unit(3, "conscript", 60.0, 55.0),
        unit(10, "hq", 60.0, 60.0),
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
        width: 100,
        height: 100,
        tick: 3000,
        slot: 0,
        players: vec![
            PlayerObservation {
                team: 0,
                out: false,
                spawn: Point { x: 60.0, z: 60.0 },
            },
            PlayerObservation {
                team: 1,
                out: false,
                spawn: Point { x: 180.0, z: 60.0 },
            },
        ],
        units,
        points: vec![],
        flags: vec![0; 10000],
        chars: vec![b'.'; 10000],
        manpower: 1000.0,
        research: None,
        runtime: Some(Context {
            units: facts,
            ..Context::default()
        }),
    }
}
fn propose(
    input: Input,
    steps: &mut BTreeMap<u32, u8>,
    budget: f64,
    reserve: f64,
    saved: f64,
    sees: impl Fn(Point) -> bool,
) -> Plan {
    let observation = Observation::new(input).unwrap();
    let ground = crate::commander_tactics::sight_game(
        &observation,
        observation.input().runtime.as_ref().unwrap(),
    );
    let available: Vec<_> = observation.input().units.iter().collect();
    plan(
        &observation,
        &available,
        budget,
        reserve,
        saved,
        &ground,
        sees,
        steps,
    )
    .unwrap()
}
fn with_tier(input: &mut Input, tier: u8) {
    input.research = Some(ResearchObservation {
        faction: Faction::Usa,
        tech: TechState::new(tier, 0, 0, 0).unwrap(),
        busy: vec![],
        stock_base: false,
        armory_ready: false,
        standard_army: true,
    });
}
fn world(input: &mut Input) {
    use crate::commander_world_policy::{Facts, Region};
    let context = input.runtime.as_mut().unwrap();
    context.mode = Mode::World;
    context.world = Some(Facts {
        regions: vec![
            Region {
                id: 0,
                at: Point { x: 60.0, z: 60.0 },
                team: Some(0),
                locked: false,
                runs: None,
                bounds: Some([0.0, 0.0, 200.0, 200.0]),
            },
            Region {
                id: 1,
                at: Point { x: 60.0, z: 180.0 },
                team: Some(1),
                locked: false,
                runs: None,
                bounds: None,
            },
        ],
        visible_cells: (0..input.width * input.height).collect(),
        ..Facts::default()
    });
}
fn at(command: &Value) -> Point {
    Point {
        x: command["x"].as_f64().unwrap(),
        z: command["z"].as_f64().unwrap(),
    }
}
fn end(command: &Value) -> Point {
    Point {
        x: command["x2"].as_f64().unwrap(),
        z: command["z2"].as_f64().unwrap(),
    }
}
fn line(from: Point, to: Point, queued: bool) -> Value {
    json!({"t":"entrench", "ids":[2,3], "pattern":"line", "fort":"wall",
        "x":from.x, "z":from.z, "x2":to.x, "z2":to.z, "queue":queued})
}

#[test]
fn intent_ai50_complete_hq_sequence_waits_150_seconds_then_builds_front_gate_and_open_back() {
    let mut steps = BTreeMap::new();
    let mut early = fixture();
    early.tick = 2999;
    let result = propose(early, &mut steps, 1000.0, 0.0, 0.0, |_| true);
    assert!(result.commands.is_empty());
    assert_eq!(result.reserve, 0.0);
    assert!(steps.is_empty());
    let expected = [
        vec![json!({"t":"build", "kind":"tower", "ids":[2], "x":69.0, "z":65.0, "dir":0.0})],
        vec![json!({"t":"build", "kind":"pillbox", "ids":[2], "x":74.0, "z":60.0, "dir":0.0})],
        vec![
            line(
                Point { x: 84.0, z: 64.0 },
                Point { x: 84.0, z: 74.0 },
                false,
            ),
            line(Point { x: 84.0, z: 56.0 }, Point { x: 84.0, z: 46.0 }, true),
        ],
        vec![
            json!({"t":"dig", "kind":"gate", "ids":[2], "x":84.0, "z":60.0, "dir":std::f64::consts::FRAC_PI_2}),
        ],
        vec![line(
            Point { x: 84.0, z: 74.0 },
            Point { x: 70.0, z: 74.0 },
            false,
        )],
        vec![line(
            Point { x: 84.0, z: 46.0 },
            Point { x: 70.0, z: 46.0 },
            false,
        )],
    ];
    for (step, commands) in expected.into_iter().enumerate() {
        let result = propose(fixture(), &mut steps, 1000.0, 0.0, 0.0, |_| true);
        assert_eq!(result.commands, commands, "step {step}");
        assert_eq!(steps.get(&10), Some(&(step as u8 + 1)));
        assert_eq!(result.reserve, 0.0);
        let cost = [120.0, 200.0, 0.0, 150.0, 0.0, 0.0][step];
        assert_eq!(result.remaining_mp, 1000.0 - cost);
        assert_eq!(
            result.builders,
            if matches!(step, 2 | 4 | 5) {
                vec![2, 3]
            } else {
                vec![2]
            }
        );
        for command in commands {
            Command::from_json(&command).unwrap();
        }
    }
    assert!(
        propose(fixture(), &mut steps, 1000.0, 0.0, 0.0, |_| true)
            .commands
            .is_empty()
    );
    assert_eq!(steps, BTreeMap::from([(10, 6)]));
}

#[test]
fn intent_ai50_each_forward_hq_has_its_own_steps_and_nearest_builder() {
    let mut input = fixture();
    input.units.push(unit(20, "hq", 140.0, 140.0));
    input.runtime.as_mut().unwrap().units.insert(
        20,
        UnitFacts {
            built: 1.0,
            ..UnitFacts::default()
        },
    );
    input.units[0].at = Point { x: 136.0, z: 140.0 };
    let mut steps = BTreeMap::from([(10, 1), (99, 5)]);
    with_tier(&mut input, 1);
    let result = propose(input.clone(), &mut steps, 1000.0, 0.0, 0.0, |_| true);
    assert_eq!(result.commands.len(), 1);
    assert_eq!(result.commands[0]["kind"], "tower");
    assert_eq!(result.builders, vec![1]);
    let aim = Point {
        x: 140.0 + 22.0 / math::hypot(1.0, 2.0),
        z: 140.0 - 14.0 / math::hypot(1.0, 2.0),
    };
    assert!(distance(at(&result.commands[0]), aim) < CELL);
    assert_eq!(steps, BTreeMap::from([(10, 1), (20, 1)]));
    // Low-tier HQs waiting for a Pillbox do not consume the next HQ's tower turn.
    assert!(
        propose(input.clone(), &mut steps, 1000.0, 0.0, 0.0, |_| true)
            .commands
            .is_empty()
    );
    with_tier(&mut input, 2);
    let result = propose(input.clone(), &mut steps, 1000.0, 0.0, 0.0, |_| true);
    assert_eq!(result.commands.len(), 1);
    assert_eq!(result.commands[0]["kind"], "pillbox");
    assert_eq!(steps, BTreeMap::from([(10, 2), (20, 1)]));
    steps.insert(10, 6);
    assert_eq!(
        propose(input.clone(), &mut steps, 1000.0, 0.0, 0.0, |_| true).builders,
        vec![1]
    );
    input.units.retain(|u| u.id != 10);
    propose(input, &mut steps, 1000.0, 0.0, 0.0, |_| true);
    assert!(!steps.contains_key(&10));
}

#[test]
fn intent_ai50_reserve_respects_company_tier_build_order_and_three_hundred_mp_bank() {
    for (step, cost) in [
        (0, 120.0),
        (1, 200.0),
        (2, 240.0),
        (3, 240.0),
        (4, 240.0),
        (5, 240.0),
    ] {
        let mut steps = BTreeMap::from([(10, step)]);
        let result = propose(fixture(), &mut steps, cost - 1.0, 0.0, 300.0, |_| true);
        assert_eq!(result.reserve, cost, "step {step}");
        assert!(result.commands.is_empty());
        assert_eq!(steps[&10], step);
        let result = propose(fixture(), &mut steps, cost - 1.0, 0.0, 301.0, |_| true);
        assert!(!result.commands.is_empty(), "step {step}");
        assert_eq!(steps[&10], step + 1);
        let mut steps = BTreeMap::from([(10, step)]);
        let result = propose(fixture(), &mut steps, cost, 1.0, 300.0, |_| true);
        assert_eq!(result.reserve, cost);
        assert_eq!(steps[&10], step);
    }
    let mut input = fixture();
    with_tier(&mut input, 1);
    for step in 1..6 {
        let mut steps = BTreeMap::from([(10, step)]);
        let result = propose(input.clone(), &mut steps, 1000.0, 0.0, 0.0, |_| true);
        assert!(result.commands.is_empty());
        assert_eq!(result.reserve, 0.0);
        assert_eq!(steps[&10], step);
    }
    assert!(
        !propose(input, &mut BTreeMap::new(), 120.0, 0.0, 0.0, |_| true)
            .commands
            .is_empty()
    );
}

#[test]
fn intent_ai50_front_faces_nearest_live_known_enemy_and_snaps_walls_to_grid() {
    for (spawn, forward) in [
        (Point { x: 180.0, z: 60.0 }, Point { x: 1.0, z: 0.0 }),
        (Point { x: 60.0, z: 180.0 }, Point { x: 0.0, z: 1.0 }),
        (Point { x: 0.0, z: 60.0 }, Point { x: -1.0, z: 0.0 }),
        (Point { x: 60.0, z: 0.0 }, Point { x: 0.0, z: -1.0 }),
        (Point { x: 180.0, z: 160.0 }, Point { x: 1.0, z: 0.0 }),
        (Point { x: 160.0, z: 180.0 }, Point { x: 0.0, z: 1.0 }),
    ] {
        let mut input = fixture();
        input.players[1].spawn = spawn;
        let result = propose(
            input,
            &mut BTreeMap::from([(10, 2)]),
            1000.0,
            0.0,
            0.0,
            |_| true,
        );
        let middle = Point {
            x: 60.0 + forward.x * 24.0,
            z: 60.0 + forward.z * 24.0,
        };
        let side = Point {
            x: -forward.z,
            z: forward.x,
        };
        for (index, sign) in [1.0, -1.0].into_iter().enumerate() {
            assert_eq!(
                at(&result.commands[index]),
                Point {
                    x: middle.x + side.x * sign * 4.0,
                    z: middle.z + side.z * sign * 4.0
                }
            );
            assert_eq!(
                end(&result.commands[index]),
                Point {
                    x: middle.x + side.x * sign * 14.0,
                    z: middle.z + side.z * sign * 14.0
                }
            );
        }
        assert_eq!(
            distance(at(&result.commands[0]), at(&result.commands[1])),
            8.0
        );
    }
    let mut input = fixture();
    input.players.push(PlayerObservation {
        team: 1,
        out: false,
        spawn: Point { x: 60.0, z: 100.0 },
    });
    let choose = |input| {
        propose(
            input,
            &mut BTreeMap::from([(10, 2)]),
            1000.0,
            0.0,
            0.0,
            |_| true,
        )
        .commands
    };
    assert_eq!(at(&choose(input.clone())[0]).z, 84.0);
    input.players[2].out = true;
    assert_eq!(at(&choose(input.clone())[0]).x, 84.0);
    input.players[2].out = false;
    input.runtime.as_mut().unwrap().hidden_spawns.insert(2);
    assert_eq!(at(&choose(input.clone())[0]).x, 84.0);
    input.runtime.as_mut().unwrap().hidden_spawns.insert(1);
    // With no delivered enemy home, the map center supplies the direction.
    assert_eq!(at(&choose(input)[0]), Point { x: 56.0, z: 84.0 });
}

#[test]
fn intent_ai50_existing_fortification_work_blocks_the_next_job_even_if_builder_unavailable() {
    for kind in ["tower", "pillbox", "wall", "gate"] {
        let mut input = fixture();
        input.units.push(unit(20, kind, 100.0, 100.0));
        input.runtime.as_mut().unwrap().units.insert(
            20,
            UnitFacts {
                built: 0.999,
                ..UnitFacts::default()
            },
        );
        let mut steps = BTreeMap::from([(10, 2)]);
        assert!(
            propose(input.clone(), &mut steps, 1000.0, 0.0, 0.0, |_| true)
                .commands
                .is_empty(),
            "{kind}"
        );
        assert_eq!(steps[&10], 2);
        input
            .runtime
            .as_mut()
            .unwrap()
            .units
            .get_mut(&20)
            .unwrap()
            .built = 1.0;
        assert!(
            !propose(input, &mut steps, 1000.0, 0.0, 0.0, |_| true)
                .commands
                .is_empty(),
            "{kind}"
        );
    }
    for entrench in [false, true] {
        let mut input = fixture();
        input.units[0].entrench = entrench;
        input
            .runtime
            .as_mut()
            .unwrap()
            .units
            .get_mut(&1)
            .unwrap()
            .wall_work = !entrench;
        let observation = Observation::new(input).unwrap();
        let ground = crate::commander_tactics::sight_game(
            &observation,
            observation.input().runtime.as_ref().unwrap(),
        );
        // A prior policy phase may already have removed this active worker from available.
        let available = observation.input().units[1..3].iter().collect::<Vec<_>>();
        let mut steps = BTreeMap::from([(10, 2)]);
        assert!(
            plan(
                &observation,
                &available,
                1000.0,
                0.0,
                0.0,
                &ground,
                |_| true,
                &mut steps
            )
            .unwrap()
            .commands
            .is_empty()
        );
        assert_eq!(steps[&10], 2);
    }
    let mut input = fixture();
    let mut foreign = unit(20, "wall", 100.0, 100.0);
    foreign.owner = 1;
    input.units.push(foreign);
    input.runtime.as_mut().unwrap().units.insert(
        20,
        UnitFacts {
            built: 0.1,
            ..UnitFacts::default()
        },
    );
    assert!(
        !propose(
            input.clone(),
            &mut BTreeMap::new(),
            1000.0,
            0.0,
            0.0,
            |_| true
        )
        .commands
        .is_empty()
    );
    input.units[0].owner = 1;
    input.units[0].entrench = true;
    assert!(
        !propose(input, &mut BTreeMap::new(), 1000.0, 0.0, 0.0, |_| true)
            .commands
            .is_empty()
    );
    let mut input = fixture();
    input
        .runtime
        .as_mut()
        .unwrap()
        .units
        .get_mut(&10)
        .unwrap()
        .built = 0.999;
    assert!(
        propose(input, &mut BTreeMap::new(), 1000.0, 0.0, 0.0, |_| true)
            .commands
            .is_empty()
    );
}

#[test]
fn intent_ai50_builders_follow_mode_and_assault_attackers_never_fortify() {
    for mode in [
        Mode::Classic,
        Mode::World,
        Mode::Default,
        Mode::Conquest,
        Mode::Annihilation,
        Mode::Assault,
    ] {
        for kind in ["rifle", "conscript", "engineer", "mg"] {
            let mut input = fixture();
            input.units.retain(|u| u.id == 1 || u.id == 10);
            input.units[0] = unit(1, kind, 56.0, 60.0);
            if mode == Mode::World {
                world(&mut input);
            }
            let context = input.runtime.as_mut().unwrap();
            context.mode = mode;
            context.defender = true;
            let eligible = if mode.is_construction() {
                kind == "engineer"
            } else {
                kind != "mg"
            };
            for step in [0, 2, 3] {
                let result = propose(
                    input.clone(),
                    &mut BTreeMap::from([(10, step)]),
                    1000.0,
                    0.0,
                    0.0,
                    |_| true,
                );
                assert_eq!(
                    !result.commands.is_empty(),
                    eligible,
                    "{mode:?}/{kind}/step {step}"
                );
                assert_eq!(result.builders, if eligible { vec![1] } else { vec![] });
            }
        }
    }
    for mode in [Mode::Assault, Mode::Horde, Mode::Tutorial, Mode::Other] {
        let mut input = fixture();
        input.runtime.as_mut().unwrap().mode = mode;
        input.runtime.as_mut().unwrap().horde = mode == Mode::Horde;
        let mut steps = BTreeMap::new();
        assert!(
            propose(input, &mut steps, 1000.0, 0.0, 0.0, |_| true)
                .commands
                .is_empty(),
            "{mode:?}"
        );
        assert!(steps.is_empty());
    }
    for condition in 0..9 {
        let mut input = fixture();
        input.units.retain(|u| u.id == 1 || u.id == 10);
        let u = &mut input.units[0];
        match condition {
            0 => u.retreating = true,
            1 => u.build = true,
            2 => u.target_id = 90,
            3 => u.dig = true,
            4 => u.entrench = true,
            5 => u.garrison = true,
            6 => u.riding = true,
            7 => u.hp = 0.0,
            _ => u.owner = 1,
        }
        assert!(
            propose(input, &mut BTreeMap::new(), 1000.0, 0.0, 0.0, |_| true)
                .commands
                .is_empty(),
            "condition {condition}"
        );
    }
}

#[test]
fn intent_ai50_gate_checks_every_planned_cell_sight_and_buildable_delivered_terrain() {
    for terrain in *b".+RDMH" {
        let mut input = fixture();
        input.chars.fill(terrain);
        let mut steps = BTreeMap::from([(10, 3)]);
        let result = propose(input, &mut steps, 1000.0, 0.0, 0.0, |_| true);
        assert_eq!(!result.commands.is_empty(), terrain != b'H');
        assert_eq!(
            result.remaining_mp,
            if terrain == b'H' { 1000.0 } else { 850.0 }
        );
        // As in the prototype, a refused placement still consumes this HQ's attempt.
        assert_eq!(steps[&10], 4);
    }
    let input = fixture();
    let observation = Observation::new(input).unwrap();
    let ground = crate::commander_tactics::sight_game(
        &observation,
        observation.input().runtime.as_ref().unwrap(),
    );
    for z in [59.0, 61.0, 63.0] {
        let mut steps = BTreeMap::from([(10, 3)]);
        let result = propose(fixture(), &mut steps, 1000.0, 0.0, 0.0, |p| {
            p != (Point { x: 85.0, z })
        });
        assert!(result.commands.is_empty(), "hidden cell {z}");
    }
    // The shared checker accepts a partially buildable line, preserving the canonical actuator.
    let mut ground = ground;
    ground.chars.fill(b'H');
    let cell = ground.cell_of(84.0, 60.0).unwrap();
    ground.chars[cell] = b'.';
    assert!(
        gate_site(
            &ground,
            Point { x: 84.0, z: 60.0 },
            std::f64::consts::FRAC_PI_2,
            |_| true
        )
        .unwrap()
    );
}

#[test]
fn intent_ai50_world_hqs_face_delivered_nonown_regions_instead_of_player_homes() {
    use crate::commander_world_policy::Region;
    let mut input = fixture();
    input.units[0].kind = "engineer".into();
    world(&mut input);
    let fronts = |input| {
        propose(
            input,
            &mut BTreeMap::from([(10, 2)]),
            1000.0,
            0.0,
            0.0,
            |_| true,
        )
        .commands
    };
    // Enemy player home is east, but the nearest nonowned delivered region is north.
    let commands = fronts(input.clone());
    assert_eq!(commands.len(), 2);
    assert_eq!(at(&commands[0]), Point { x: 56.0, z: 84.0 });
    let facts = input.runtime.as_mut().unwrap().world.as_mut().unwrap();
    facts.regions.push(Region {
        id: 2,
        at: Point { x: 20.0, z: 60.0 },
        team: None,
        locked: false,
        runs: None,
        bounds: None,
    });
    // Neutral territory also qualifies. Ownership is the only threat-region filter.
    assert_eq!(at(&fronts(input.clone())[0]), Point { x: 36.0, z: 56.0 });
    input
        .runtime
        .as_mut()
        .unwrap()
        .world
        .as_mut()
        .unwrap()
        .regions[2]
        .team = Some(0);
    assert_eq!(at(&fronts(input.clone())[0]), Point { x: 56.0, z: 84.0 });
    input
        .runtime
        .as_mut()
        .unwrap()
        .world
        .as_mut()
        .unwrap()
        .regions[1]
        .team = Some(0);
    // With only owned regions, aim at the map center, regardless of visible player homes.
    assert_eq!(at(&fronts(input)[0]), Point { x: 56.0, z: 84.0 });
}

#[test]
fn intent_ai50_world_placements_require_owned_visible_cells_for_buildings_gate_and_wall_segments() {
    let mut input = fixture();
    input.units[0].kind = "engineer".into();
    world(&mut input);
    let commands = |input, step| {
        propose(
            input,
            &mut BTreeMap::from([(10, step)]),
            1000.0,
            0.0,
            0.0,
            |_| true,
        )
        .commands
    };
    assert_eq!(commands(input.clone(), 1)[0]["kind"], "pillbox");
    assert_eq!(commands(input.clone(), 3)[0]["kind"], "gate");
    assert_eq!(commands(input.clone(), 2).len(), 2);
    // Centers remain owned while the left cells of the Pillbox and Gate do not.
    input
        .runtime
        .as_mut()
        .unwrap()
        .world
        .as_mut()
        .unwrap()
        .regions[0]
        .bounds = Some([60.0, 0.0, 200.0, 200.0]);
    assert!(commands(input.clone(), 1).is_empty());
    assert!(commands(input.clone(), 3).is_empty());
    let front = commands(input.clone(), 2);
    assert_eq!(front.len(), 1);
    assert_eq!(front[0]["queue"], false);
    assert_eq!(at(&front[0]), Point { x: 64.0, z: 84.0 });
    input
        .runtime
        .as_mut()
        .unwrap()
        .world
        .as_mut()
        .unwrap()
        .regions[0]
        .bounds = Some([0.0, 75.0, 200.0, 200.0]);
    assert!(commands(input.clone(), 4).is_empty());
    assert!(commands(input.clone(), 5).is_empty());
    input
        .runtime
        .as_mut()
        .unwrap()
        .world
        .as_mut()
        .unwrap()
        .regions[0]
        .bounds = Some([0.0, 0.0, 200.0, 200.0]);
    for step in [0, 1, 2, 3, 4, 5] {
        let result = propose(
            input.clone(),
            &mut BTreeMap::from([(10, step)]),
            1000.0,
            0.0,
            0.0,
            |_| false,
        );
        assert!(result.commands.is_empty(), "invisible World step {step}");
        assert_eq!(result.remaining_mp, 1000.0);
        assert!(result.builders.is_empty());
    }
    // No delivered ownership fact grants permission to place a World fortification.
    input.runtime.as_mut().unwrap().world = None;
    for step in [0, 1, 2, 3, 4, 5] {
        assert!(
            commands(input.clone(), step).is_empty(),
            "missing World facts at step {step}"
        );
    }
}
