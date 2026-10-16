use super::*;
use crate::{
    commander_ai::{Input, PlayerObservation},
    commander_tactics::{Context, NodeObservation, State, UnitFacts},
};

fn unit(id: u32, kind: &str, x: f64, z: f64) -> UnitObservation {
    let catalog = Catalog::compiled().unwrap();
    let d = catalog.unit(kind).unwrap();
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
fn fixture(buildings: &[&str]) -> Input {
    let mut units = vec![
        unit(1, "engineer", 40.0, 40.0),
        unit(2, "engineer", 44.0, 40.0),
        unit(10, "hq", 40.0, 40.0),
    ];
    for (i, kind) in buildings.iter().enumerate() {
        units.push(unit(11 + i as u32, kind, 80.0 + i as f64 * 20.0, 40.0));
    }
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
        width: 128,
        height: 128,
        tick: 200,
        slot: 0,
        players: vec![
            PlayerObservation {
                team: 0,
                out: false,
                spawn: Point { x: 40.0, z: 40.0 },
            },
            PlayerObservation {
                team: 1,
                out: false,
                spawn: Point { x: 220.0, z: 220.0 },
            },
        ],
        units,
        points: vec![],
        flags: vec![0; 16384],
        chars: vec![b'.'; 16384],
        manpower: 2000.0,
        research: None,
        runtime: Some(Context {
            mode: Mode::Classic,
            units: facts,
            ..Context::default()
        }),
    }
}
fn run(
    input: Input,
    recent: &[Sighting],
    memory: &mut BTreeMap<u32, usize>,
    level: Level,
) -> (Economy, MatchRandom) {
    let observation = Observation::new(input).unwrap();
    let ground = terrain(observation.input());
    let available: Vec<_> = observation
        .input()
        .units
        .iter()
        .filter(|u| u.owner == 0)
        .collect();
    let mut random = MatchRandom::new(617);
    let recent: Vec<_> = recent.iter().collect();
    let result = plan(
        &observation,
        &available,
        &recent,
        memory,
        level,
        observation.input().manpower,
        &ground,
        &mut random,
    )
    .unwrap();
    (result, random)
}
fn draws(random: &MatchRandom, n: usize) {
    let mut expected = MatchRandom::new(617);
    for _ in 0..n {
        expected.next_fraction(StreamId::Ai(0));
    }
    assert_eq!(random.snapshot(), expected.snapshot());
}
fn node(x: f64, rate: f64, fuel: bool, visible: bool) -> NodeObservation {
    NodeObservation {
        at: Point { x, z: 100.0 },
        rate,
        fuel,
        visible,
    }
}

#[test]
fn intent_ai54_classic_depots_score_rich_nodes_and_keep_distinct_engineer_memory() {
    let mut input = fixture(&[]);
    input.runtime.as_mut().unwrap().nodes = vec![
        node(40.0, 1.0, false, true),
        node(70.0, 4.0, false, true),
        node(90.0, 0.0, true, false),
    ];
    let mut memory = BTreeMap::from([(99, 0)]);
    let (result, random) = run(input.clone(), &[], &mut memory, Level::Normal);
    assert_eq!(result.free_nodes, 3);
    assert_eq!(memory, BTreeMap::from([(1, 1), (2, 2)]));
    assert_eq!(result.commands[0]["kind"], "depot");
    assert_eq!(result.commands[0]["x"], 70.0);
    assert_eq!(
        result.commands[1],
        json!({"t":"move","orders":[[2,90.0,100.0]]})
    );
    assert_eq!(result.reserve, 0.0);
    draws(&random, 0);
    let (again, _) = run(input, &[], &mut memory, Level::Normal);
    assert_eq!(again.commands, result.commands);
}

#[test]
fn intent_ai54_classic_build_order_counter_flak_rest_airfield_and_world_exclusion() {
    let cases: &[(&[&str], Option<&str>)] = &[
        (&["depot", "depot"], Some("barracks")),
        (&["depot", "depot", "barracks"], Some("motorpool")),
        (
            &["depot", "depot", "depot", "barracks", "motorpool"],
            Some("airfield"),
        ),
        (
            &[
                "depot",
                "depot",
                "depot",
                "barracks",
                "motorpool",
                "airfield",
            ],
            None,
        ),
    ];
    for &(buildings, kind) in cases {
        let (result, random) = run(fixture(buildings), &[], &mut BTreeMap::new(), Level::Normal);
        if let Some(kind) = kind {
            assert_eq!(result.commands[0]["kind"], kind);
            assert!(result.reserve > 0.0);
            draws(&random, 2);
        } else {
            assert!(result.commands.is_empty());
            draws(&random, 0);
        }
    }
    let mut input = fixture(&["barracks"]);
    input.runtime.as_mut().unwrap().nodes = vec![node(90.0, 1.0, false, true)];
    let armor = Sighting {
        id: 100,
        kind: "tank".into(),
        owner: 1,
        at: Point { x: 200.0, z: 200.0 },
        time: 10.0,
        worth: 100.0,
    };
    let (result, _) = run(input.clone(), &[armor], &mut BTreeMap::new(), Level::Normal);
    assert_eq!(result.commands[0]["kind"], "motorpool");
    let (result, random) = run(input, &[], &mut BTreeMap::new(), Level::Easy);
    assert_eq!(result.commands[0]["kind"], "depot");
    draws(&random, 0);
    let mut input = fixture(&["depot", "depot", "barracks", "motorpool"]);
    let mut plane = unit(100, "fighter", 45.0, 40.0);
    plane.owner = 1;
    input.units.push(plane);
    let (result, random) = run(input.clone(), &[], &mut BTreeMap::new(), Level::Hard);
    assert_eq!(result.commands[0]["kind"], "flakpos");
    draws(&random, 2);
    input.units.retain(|u| u.id != 100);
    input.runtime.as_mut().unwrap().nodes = vec![node(90.0, 1.0, false, true)];
    let (result, random) = run(input, &[], &mut BTreeMap::new(), Level::Normal);
    assert_eq!(result.commands[0]["kind"], "depot");
    draws(&random, 0);
    for mode in [Mode::Conquest, Mode::World] {
        let mut input = fixture(&[]);
        input.runtime.as_mut().unwrap().mode = mode;
        let (result, random) = run(input, &[], &mut BTreeMap::new(), Level::Normal);
        assert!(result.commands.is_empty());
        assert_eq!(result.reserve, 0.0);
        draws(&random, 0);
    }
}

#[test]
fn intent_ai54_classic_repairs_help_and_rush_reserve_precede_construction() {
    for unfinished in [false, true] {
        let mut input = fixture(&[]);
        if unfinished {
            input
                .runtime
                .as_mut()
                .unwrap()
                .units
                .get_mut(&10)
                .unwrap()
                .built = 0.5;
        } else {
            input.units.iter_mut().find(|u| u.id == 10).unwrap().hp *= 0.69;
        }
        let (result, random) = run(input, &[], &mut BTreeMap::new(), Level::Normal);
        assert_eq!(result.commands.len(), 2);
        assert!(
            result
                .commands
                .iter()
                .all(|c| c["t"] == "assist" && c["id"] == 10)
        );
        draws(&random, 0);
    }
    let input = fixture(&[]);
    let threat = Sighting {
        id: 100,
        kind: "rifle".into(),
        owner: 1,
        at: Point { x: 45.0, z: 40.0 },
        time: 10.0,
        worth: 100.0,
    };
    let (result, random) = run(
        input.clone(),
        std::slice::from_ref(&threat),
        &mut BTreeMap::new(),
        Level::Normal,
    );
    assert!(result.rush);
    assert_eq!(result.reserve, 0.0);
    draws(&random, 2);
    let (result, _) = run(input, &[threat], &mut BTreeMap::new(), Level::Easy);
    assert!(!result.rush);
    assert!(result.reserve > 0.0);
}

#[test]
fn intent_ai66_classic_non_depot_site_draws_include_unaffordable_and_missing_sites_only() {
    for missing_site in [false, true] {
        let mut input = fixture(&[]);
        input.manpower = 0.0;
        if missing_site {
            input.flags.fill(MOVE);
        }
        let (result, random) = run(input, &[], &mut BTreeMap::new(), Level::Normal);
        assert!(result.commands.is_empty());
        draws(&random, 2);
    }
    let mut input = fixture(&[]);
    input.units[0].retreating = true;
    input.units[1].build = true;
    let (result, random) = run(input, &[], &mut BTreeMap::new(), Level::Normal);
    assert!(result.commands.is_empty());
    draws(&random, 0);
}

#[test]
fn intent_ai54_classic_tactics_runs_economy_before_shopping_from_detached_input() {
    let observation = Observation::new(fixture(&[])).unwrap();
    let before = observation.clone();
    let mut state = State::default();
    let mut random = MatchRandom::new(617);
    let commands =
        crate::commander_tactics::plan(&observation, &[], &mut state, Level::Normal, &mut random)
            .unwrap();
    assert!(
        commands
            .iter()
            .any(|c| c["t"] == "build" && c["kind"] == "barracks")
    );
    assert_eq!(observation, before);
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

#[test]
fn intent_ai54_classic_flak_cap_airfield_threshold_and_engineer_replacement() {
    let mut input = fixture(&[
        "depot",
        "depot",
        "barracks",
        "motorpool",
        "flakpos",
        "flakpos",
    ]);
    let mut plane = unit(100, "fighter", 45.0, 40.0);
    plane.owner = 1;
    input.units.push(plane);
    let (result, random) = run(input, &[], &mut BTreeMap::new(), Level::Normal);
    assert!(
        result.commands.is_empty(),
        "two depots cannot unlock an Airfield or a third flak position"
    );
    draws(&random, 0);

    for claimed in [false, true] {
        let mut input = fixture(&[]);
        input.units.retain(|u| u.id != 2);
        input.runtime.as_mut().unwrap().nodes = vec![node(90.0, 1.0, false, true)];
        if claimed {
            input.runtime.as_mut().unwrap().claimed_nodes.insert(0);
        }
        // Keep the engineer from spending on a depot so buying tests the same wallet.
        input.units[0].build = true;
        let observation = Observation::new(input).unwrap();
        let commands = crate::commander_tactics::plan(
            &observation,
            &[],
            &mut State::default(),
            Level::Normal,
            &mut MatchRandom::new(617),
        )
        .unwrap();
        assert_eq!(
            commands
                .iter()
                .any(|c| c["t"] == "buy" && c["unit"] == "engineer"),
            !claimed
        );
    }
}

#[test]
fn intent_ai54_classic_repairs_use_strict_health_and_distance_limits() {
    for (health, distance, repairs) in [
        (0.699, 59.0, true),
        (0.7, 59.0, false),
        (0.699, 60.0, false),
    ] {
        let mut input = fixture(&[]);
        input.units[0].at = Point {
            x: 40.0 + distance,
            z: 40.0,
        };
        input.units[1].retreating = true;
        input.units.iter_mut().find(|u| u.id == 10).unwrap().hp *= health;
        let (result, _) = run(input, &[], &mut BTreeMap::new(), Level::Normal);
        assert_eq!(result.commands.iter().any(|c| c["t"] == "assist"), repairs);
    }
}

#[test]
fn classic_construction_excludes_mounted_and_busy_engineers_before_site_draws() {
    for busy in 0..7 {
        let mut input = fixture(&[]);
        input.units[1].retreating = true;
        match busy {
            0 => input.units[0].riding = true,
            1 => input.units[0].build = true,
            2 => input.units[0].dig = true,
            3 => input.units[0].entrench = true,
            4 => input.units[0].garrison = true,
            5 => input.units[0].target_id = 10,
            _ => input.units[0].retreating = true,
        }
        let (result, random) = run(input.clone(), &[], &mut BTreeMap::new(), Level::Normal);
        assert!(result.commands.is_empty());
        assert!(result.builders.is_empty());
        draws(&random, 0);
        input.units[1].retreating = false;
        let (result, random) = run(input, &[], &mut BTreeMap::new(), Level::Normal);
        assert_eq!(result.builders, vec![2]);
        assert_eq!(result.commands.len(), 1);
        assert_eq!(result.commands[0]["ids"], json!([2]));
        draws(&random, 1);
    }
}

#[test]
fn classic_consolidated_construction_assignments_precede_fortification_and_rush() {
    for job in ["build", "assist", "move"] {
        let mut input = fixture(if job == "assist" { &["barracks"] } else { &[] });
        input.tick = 4000;
        if job == "assist" {
            input
                .runtime
                .as_mut()
                .unwrap()
                .units
                .get_mut(&11)
                .unwrap()
                .built = 0.5;
        } else if job == "move" {
            input.runtime.as_mut().unwrap().nodes = vec![
                node(90.0, 1.0, false, false),
                node(120.0, 2.0, false, false),
            ];
        }
        for under_rush in [false, true] {
            let recent = if under_rush {
                vec![Sighting {
                    id: 100,
                    kind: "rifle".into(),
                    owner: 1,
                    at: Point { x: 70.0, z: 40.0 },
                    time: 200.0,
                    worth: 100.0,
                }]
            } else {
                vec![]
            };
            let (economy, _) = run(input.clone(), &recent, &mut BTreeMap::new(), Level::Normal);
            assert_eq!(economy.builders, vec![1, 2]);
            assert!(economy.commands.iter().all(|c| c["t"] == job));
            let observation = Observation::new(input.clone()).unwrap();
            if !under_rush {
                let available: Vec<_> = observation
                    .input()
                    .units
                    .iter()
                    .filter(|u| u.kind == "engineer")
                    .collect();
                let control = crate::commander_fortification::plan_after_builds(
                    &observation,
                    &available,
                    economy.remaining_mp,
                    economy.reserve,
                    0.0,
                    &terrain(observation.input()),
                    |_| true,
                    &mut BTreeMap::new(),
                    &economy.commands,
                )
                .unwrap();
                assert!(
                    control.commands.iter().any(|c| c["kind"] == "tower"),
                    "{job}: idle crew must have fortification work"
                );
            }
            let mut random = MatchRandom::new(617);
            let commands = crate::commander_tactics::plan(
                &observation,
                &recent,
                &mut State::default(),
                Level::Normal,
                &mut random,
            )
            .unwrap();
            let jobs: Vec<_> = commands
                .iter()
                .filter(|c| {
                    matches!(
                        c["t"].as_str(),
                        Some("build" | "assist" | "move" | "dig" | "entrench" | "retreat")
                    )
                })
                .cloned()
                .collect();
            assert_eq!(jobs, economy.commands, "{job}, rush={under_rush}");
            draws(&random, if job == "build" { 2 } else { 0 });
        }
    }
}

#[test]
fn classic_consolidated_wallet_spends_build_costs_and_reserves_once_before_shopping() {
    let catalog = Catalog::compiled().unwrap();
    let cost = number(catalog.unit("motorpool").unwrap(), "cost");
    let rifle = economy::price_of(Mode::Classic, "rifle").unwrap().mp;
    for extra in [-1.0, 0.0] {
        let mut input = fixture(&["depot", "depot", "barracks"]);
        for id in 50..53 {
            input.units.push(unit(id, "mg", 60.0, 80.0));
        }
        input.runtime.as_mut().unwrap().faction = economy::Faction::Germany;
        input.manpower = cost * 3.0 + rifle + extra;
        let observation = Observation::new(input.clone()).unwrap();
        let commands = crate::commander_tactics::plan(
            &observation,
            &[],
            &mut State::default(),
            Level::Normal,
            &mut MatchRandom::new(617),
        )
        .unwrap();
        let builds: Vec<_> = commands.iter().filter(|c| c["t"] == "build").collect();
        assert_eq!(builds.len(), 2);
        assert!(builds.iter().all(|c| c["kind"] == "motorpool"));
        let buys: Vec<_> = commands.iter().filter(|c| c["t"] == "buy").collect();
        assert_eq!(buys.len(), usize::from(extra == 0.0));
        assert!(buys.iter().all(|c| c["unit"] == "rifle"));
        assert_eq!(
            input.manpower - cost * builds.len() as f64 - rifle * buys.len() as f64,
            cost + if extra < 0.0 { rifle - 1.0 } else { 0.0 }
        );
    }
}

#[test]
fn classic_rush_frees_endangered_engineer_while_safe_engineer_builds_once() {
    let mut input = fixture(&[]);
    input.tick = 4000;
    input.units[0].at.x = 75.0;
    let recent = [Sighting {
        id: 100,
        kind: "rifle".into(),
        owner: 1,
        at: Point { x: 70.0, z: 40.0 },
        time: 200.0,
        worth: 100.0,
    }];
    let observation = Observation::new(input).unwrap();
    let mut random = MatchRandom::new(617);
    let commands = crate::commander_tactics::plan(
        &observation,
        &recent,
        &mut State::default(),
        Level::Normal,
        &mut random,
    )
    .unwrap();
    let builds: Vec<_> = commands.iter().filter(|c| c["t"] == "build").collect();
    assert_eq!(builds.len(), 1);
    assert_eq!(builds[0]["kind"], "barracks");
    assert_eq!(builds[0]["ids"], json!([2]));
    let moves: Vec<_> = commands.iter().filter(|c| c["t"] == "move").collect();
    assert_eq!(moves.len(), 1);
    assert_eq!(moves[0]["orders"], json!([[1, 40.0, 40.0]]));
    draws(&random, 1);
}
