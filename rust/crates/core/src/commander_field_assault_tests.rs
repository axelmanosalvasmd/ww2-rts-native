//! AI-56: detached Assault and Annihilation targets, home limits and HQ roles.
use super::tests::{fixture, run, unit};
use super::*;
use crate::commander_ai::{Input, PointObservation};

fn enemy(id: u32, kind: &str, x: f64) -> UnitObservation {
    let mut u = unit(id, kind, x);
    u.owner = 1;
    u
}
fn destinations(commands: &[Value], kind: &str) -> Vec<Point> {
    commands
        .iter()
        .filter(|c| c["t"] == kind)
        .flat_map(|c| c["orders"].as_array().unwrap())
        .map(|o| Point {
            x: o[1].as_f64().unwrap(),
            z: o[2].as_f64().unwrap(),
        })
        .collect()
}

#[test]
fn intent_ai56_assault_and_annihilation_use_only_delivered_enemy_bunkers() {
    for mode in [Mode::Assault, Mode::Annihilation] {
        let mut units: Vec<_> = (1..=6).map(|id| unit(id, "rifle", 40.0)).collect();
        units.extend([
            enemy(10, "bunker", 145.0),
            enemy(11, "bunker", 120.0),
            enemy(12, "hq", 70.0),
            enemy(13, "rifle", 100.0),
            unit(14, "bunker", 60.0),
            {
                let mut ally = unit(15, "bunker", 65.0);
                ally.owner = 2;
                ally
            },
            {
                let mut dead = enemy(16, "bunker", 80.0);
                dead.hp = 0.0;
                dead
            },
        ]);
        let mut input = fixture(units);
        input.points.clear();
        input.runtime.as_mut().unwrap().mode = mode;
        // A remembered building and a nearer enemy spawn cannot become bunker targets.
        input.runtime.as_mut().unwrap().known_buildings = vec![KnownBuilding {
            id: 90,
            kind: "hq".into(),
            owner: 1,
            at: Point { x: 55.0, z: 20.0 },
        }];
        let observation = Observation::new(input.clone()).unwrap();
        let targets = crate::commander_classic_rules::targets(
            &observation,
            observation.input().runtime.as_ref().unwrap(),
        )
        .unwrap();
        assert_eq!(targets.iter().map(|t| t.1).collect::<Vec<_>>(), [11, 10]);
        let commands = run(input.clone(), &mut State::default(), Level::Normal);
        assert_eq!(destinations(&commands, "amove"), vec![targets[0].2; 6]);
        input.units.retain(|u| u.id != 11);
        let commands = run(input.clone(), &mut State::default(), Level::Normal);
        assert_eq!(destinations(&commands, "amove"), vec![targets[1].2; 6]);
        input.units.retain(|u| u.id != 10);
        assert!(
            destinations(&run(input, &mut State::default(), Level::Normal), "amove").is_empty()
        );
    }
}

#[test]
fn intent_ai56_assault_defender_includes_seventy_metres_but_excludes_beyond() {
    for (distance, defender, selected) in [
        (70.0, true, true),
        (70.001, true, false),
        (70.001, false, true),
    ] {
        let mut input = fixture(vec![unit(1, "rifle", 40.0)]);
        input.points = vec![PointObservation {
            at: Point {
                x: 10.0 + distance,
                z: 20.0,
            },
            owner: None,
            vp: 1.0,
            mp: 0.0,
            cut: false,
            locked: false,
        }];
        let context = input.runtime.as_mut().unwrap();
        context.mode = Mode::Assault;
        context.defender = defender;
        let mut state = State::default();
        let commands = run(input, &mut state, Level::Normal);
        assert_eq!(state.situation.unwrap().point, selected.then_some(0));
        assert_eq!(!destinations(&commands, "move").is_empty(), selected);
    }
}

fn home_input() -> Input {
    let mut units = vec![unit(1, "rifle", 45.0), unit(2, "rifle", 47.0)];
    for (id, kind, x) in [
        (10, "hq", 40.0),
        (11, "barracks", 85.0),
        (12, "motorpool", 115.0),
        (13, "airfield", 145.0),
        (14, "armory", 180.0),
    ] {
        let mut u = unit(id, kind, x);
        u.at.z = 60.0;
        units.push(u);
    }
    let mut input = fixture(units);
    input.width = 120;
    input.height = 100;
    input.flags = vec![0; 12000];
    input.chars = vec![b'.'; 12000];
    input.players[0].spawn = Point { x: 40.0, z: 60.0 };
    input.players[1].spawn = Point { x: 220.0, z: 60.0 };
    input.players[2].spawn = Point { x: 20.0, z: 180.0 };
    input.points.clear();
    input.tick = 4000;
    input.manpower = 2000.0;
    input.runtime.as_mut().unwrap().mode = Mode::Assault;
    input
}

#[test]
fn intent_ai56_assault_defender_fortifies_home_while_attacker_skips_hq_forts() {
    for (defender, step, expected) in [
        (true, 0, Some("tower")),
        (true, 1, Some("pillbox")),
        (false, 0, None),
        (false, 1, None),
    ] {
        let mut input = home_input();
        input.runtime.as_mut().unwrap().defender = defender;
        let mut state = State {
            fort_steps: BTreeMap::from([(10, step)]),
            ..State::default()
        };
        let commands = run(input, &mut state, Level::Normal);
        assert_eq!(
            commands
                .iter()
                .find(|c| c["t"] == "build")
                .and_then(|c| c["kind"].as_str()),
            expected,
            "defender {defender}, step {step}: {commands:?}"
        );
        assert_eq!(
            state.fort_steps[&10],
            if defender { step + 1 } else { step }
        );
    }
}

#[test]
fn intent_ai56_only_assault_defender_refuses_otherwise_valid_forward_hq() {
    for (mode, defender, permitted) in [
        (Mode::Assault, true, false),
        (Mode::Assault, false, true),
        (Mode::Annihilation, true, true),
    ] {
        let mut input = home_input();
        input.units.retain(|u| u.id >= 10);
        input.units.extend((1..=3).map(|id| {
            let mut u = unit(id, "rifle", 140.0 + id as f64 * 2.0);
            u.at.z = 120.0;
            u
        }));
        input.units.sort_by_key(|u| u.id);
        let context = input.runtime.as_mut().unwrap();
        context.mode = mode;
        context.defender = defender;
        let observation = Observation::new(input).unwrap();
        let ground = sight_game(&observation, observation.input().runtime.as_ref().unwrap());
        let crew: Vec<_> = observation
            .input()
            .units
            .iter()
            .filter(|u| u.id < 10)
            .collect();
        let forward =
            crate::commander_forward_hq::plan(&observation, &crew, 2000.0, &ground, |_| true)
                .unwrap();
        assert_eq!(
            forward.is_some(),
            permitted,
            "{mode:?}, defender {defender}"
        );
        if let Some(forward) = forward {
            assert_eq!(forward.command["kind"], "hq");
            assert!(forward.cost > 0.0);
        }
    }
}
