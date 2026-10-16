use super::*;
use crate::{
    economy_state::ArmySize, map::AuthoritativeMap, match_commands::Stance, weather::WeatherChoice,
};

fn map() -> AuthoritativeMap {
    AuthoritativeMap::from_json(include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../server/src/authoritative-maps/tutorial.json"
    )))
    .unwrap()
}
fn options(humans: usize) -> MatchOptions {
    MatchOptions {
        seed: 0x7050,
        mode: Mode::Tutorial,
        players: (0..humans)
            .map(|slot| PlayerConfig {
                name: format!("Player {slot}"),
                team: slot as i32 + 2,
                faction: Faction::Usa,
            })
            .collect(),
        shuffle: true,
        army: ArmySize::Standard,
        weather: WeatherChoice::Map,
        weather_seed: Some(1),
        living_weather: false,
        map_key: Some("tutorial".into()),
        tech: true,
        supply: true,
        logistics: true,
        sandbox: false,
    }
}
fn tutorial() -> Match {
    Match::new_tutorial(&map(), options(1)).unwrap()
}
fn command(m: &mut Match, owner: usize, c: Command) {
    assert_eq!(m.execute_command(owner, &c, false).unwrap(), None);
}
fn ours(m: &Match) -> Vec<i64> {
    m.world
        .game
        .units
        .values()
        .filter(|u| m.world.game.players[u.owner].team == 0 && u.hp > 0.0)
        .map(|u| i64::from(u.id))
        .collect()
}
fn go(m: &mut Match, at: Point) {
    let mut by_owner = std::collections::BTreeMap::<usize, Vec<MoveOrder>>::new();
    for id in ours(m) {
        let owner = m.world.game.units[&(id as u32)].owner;
        by_owner
            .entry(owner)
            .or_default()
            .push(MoveOrder { unit: id, at });
    }
    for (owner, orders) in by_owner {
        command(
            m,
            owner,
            Command::Move {
                orders,
                control_ids: None,
                attack_move: false,
                queue: false,
                together: false,
                face: None,
            },
        );
    }
}
fn until(m: &mut Match, condition: impl Fn(&Match) -> bool, max: usize) {
    for _ in 0..max {
        if condition(m) {
            return;
        }
        m.advance_tick().unwrap();
    }
    assert!(
        condition(m),
        "stuck on {:?} at tick {}",
        m.tutorial_state().unwrap().step(),
        m.world.game.tick
    );
}
fn next(m: &mut Match, step: TutorialStep) {
    until(m, |m| m.tutorial_state().unwrap().step() == step, 1600);
}
/// Focused row setup enters a real step, publishing its authored effects.
/// The MODE-60 journey below reaches every step through the tick runner.
fn enter(m: &mut Match, step: TutorialStep) {
    m.tutorial.as_mut().unwrap().index = TutorialStep::ALL.iter().position(|s| *s == step).unwrap();
    m.begin_tutorial_step().unwrap();
}
fn kill_step(m: &mut Match) {
    // Same explicit fight resolution used by prototype test-tutorial.js.
    for id in m.tutorial_state().unwrap().spawned().to_vec() {
        if let Some(unit) = m.world.game.units.get_mut(&id) {
            unit.hp = 0.0;
        }
    }
}
fn collect_until_step(m: &mut Match, said: &mut Vec<TutorialLine>, wanted: TutorialStep) {
    for _ in 0..1600 {
        if m.tutorial_state().unwrap().step() == wanted {
            return;
        }
        said.extend(m.advance_tick().unwrap().tutorial_lines);
    }
    panic!("did not reach {wanted:?}");
}
fn hold_fire(m: &mut Match) {
    let ids = ours(m);
    command(
        m,
        0,
        Command::Stance {
            ids,
            stance: Stance::HoldFire,
            on: true,
        },
    );
}

#[test]
fn tutorial_roster_features_and_instant_ground_home_recruitment() {
    let mut m = Match::new_tutorial(&map(), options(2)).unwrap();
    assert_eq!(
        m.start
            .players
            .iter()
            .map(|p| (p.config.name.as_str(), p.config.team, p.config.faction))
            .collect::<Vec<_>>(),
        vec![
            ("Player 0", 0, Faction::Usa),
            ("Player 1", 0, Faction::Usa),
            ("Enemy", 1, Faction::Germany)
        ]
    );
    assert_eq!(m.start.players[0].spawn, Point { x: 11.0, z: 43.0 });
    assert_eq!(m.start.players[1].spawn, m.start.players[0].spawn);
    assert_eq!(m.start.players[2].spawn, Point { x: 137.0, z: 77.0 });
    assert!(m.building_cells.is_empty());
    assert!(!m.options.tech && !m.options.supply && !m.options.logistics);
    for owner in 0..2 {
        assert_eq!(
            m.world
                .game
                .units
                .values()
                .filter(|u| u.owner == owner)
                .map(|u| m.rules.kinds[u.kind].name.as_str())
                .collect::<Vec<_>>(),
            vec!["rifle", "rifle", "mg"]
        );
    }
    assert!(!m.world.game.units.values().any(|u| u.owner == 2));
    let wallet = m.start.economy.players[0].mp;
    let id = m.next_unit_id;
    command(
        &mut m,
        0,
        Command::Buy {
            kind: "rifle".into(),
            from: None,
        },
    );
    assert_eq!(m.world.game.units[&id].owner, 0);
    assert!(m.start.economy.players[0].mp < wallet);
    assert!(
        math::hypot(
            m.world.game.units[&id].x - 11.0,
            m.world.game.units[&id].z - 43.0
        ) < 20.0
    );
    m.advance_tick().unwrap();
    assert_eq!(m.start.economy.players[2].mp, 0.0);
}

#[test]
fn mode59_tutorial_income_uses_attacker_base_points_and_army_scale() {
    for (army, scale) in [
        (ArmySize::Standard, 1.0),
        (ArmySize::Large, 3.0),
        (ArmySize::Massive, 6.0),
        (ArmySize::Endless, 20.0),
    ] {
        let mut config = options(2);
        config.army = army;
        let mut m = Match::new_tutorial(&map(), config).unwrap();
        m.advance_tick().unwrap();
        assert_eq!(m.start.economy.players[0].inc, 5.0 * scale);
        let before = m.start.economy.players[0].mp;
        // Declared allied ownership supplies the point-income operand.
        m.start.points[0].owner = Some(1);
        m.advance_tick().unwrap();
        assert_eq!(m.start.economy.players[0].inc, 5.0 * scale);
        assert_eq!(m.start.economy.players[1].inc, 7.0 * scale);
        assert!(
            (m.start.economy.players[0].mp - before - 5.0 * scale * m.rules.movement.tick).abs()
                < 1e-10
        );
        assert_eq!(m.start.economy.players[2].inc, 0.0);
        assert_eq!(m.start.economy.players[2].mp, 0.0);
    }
}

#[test]
fn mode61_tutorial_hedgerow_waits_for_all_allied_squads() {
    let mut m = Match::new_tutorial(&map(), options(2)).unwrap();
    let first = m
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 0)
        .unwrap()
        .id;
    command(
        &mut m,
        0,
        Command::Move {
            orders: vec![MoveOrder {
                unit: i64::from(first),
                at: Point { x: 31.0, z: 43.0 },
            }],
            control_ids: None,
            attack_move: false,
            queue: false,
            together: false,
            face: None,
        },
    );
    until(&mut m, |m| m.world.game.units[&first].x >= 24.0, 400);
    assert_eq!(m.tutorial_state().unwrap().step(), TutorialStep::Hedgerow);
    go(&mut m, Point { x: 31.0, z: 45.0 });
    next(&mut m, TutorialStep::Orchard);
}
#[test]
fn mode62_tutorial_orchard_spawns_understrength_sentries_and_waits_for_both() {
    let mut m = tutorial();
    go(&mut m, Point { x: 31.0, z: 45.0 });
    next(&mut m, TutorialStep::Orchard);
    let ids = m.tutorial_state().unwrap().spawned().to_vec();
    assert_eq!(ids.len(), 2);
    for (id, at) in ids
        .iter()
        .zip([Point { x: 49.0, z: 29.0 }, Point { x: 49.0, z: 59.0 }])
    {
        let u = &m.world.game.units[id];
        assert_eq!(u.at(), at);
        assert_eq!(u.hp, 60.0);
        assert_eq!(m.rules.kinds[u.kind].name, "rifle");
        assert_eq!(u.owner, m.tutorial_state().unwrap().enemy());
        assert!(!m.commands.units[id].auto_retreat);
    }
    m.world.game.units.get_mut(&ids[0]).unwrap().hp = 0.0;
    m.advance_tick().unwrap();
    assert_eq!(m.tutorial_state().unwrap().step(), TutorialStep::Orchard);
    m.world.game.units.get_mut(&ids[1]).unwrap().hp = 0.0;
    next(&mut m, TutorialStep::Crossroads);
    assert!(ids.iter().all(|id| !m.world.game.units.contains_key(id)));
}
#[test]
fn mode63_tutorial_crossroads_uses_real_point_capture() {
    let mut m = tutorial();
    enter(&mut m, TutorialStep::Crossroads);
    m.advance_tick().unwrap();
    assert_eq!(m.tutorial_state().unwrap().step(), TutorialStep::Crossroads);
    let at = m.start.points[0].at;
    go(&mut m, at);
    next(&mut m, TutorialStep::Recruit);
    assert_eq!(m.start.points[0].owner, Some(0));
    assert_eq!(m.story[0].captures, 1.0);
}
#[test]
fn mode64_tutorial_recruit_applies_wallet_floor_and_waits_for_at_gun() {
    for initial in [12.0, 400.0] {
        let mut m = tutorial();
        m.start.economy.players[0].mp = initial;
        enter(&mut m, TutorialStep::Recruit);
        assert_eq!(m.start.economy.players[0].mp, initial.max(250.0));
        assert_eq!(m.start.economy.players[1].mp, 0.0);
        m.advance_tick().unwrap();
        assert_eq!(m.tutorial_state().unwrap().step(), TutorialStep::Recruit);
        command(
            &mut m,
            0,
            Command::Buy {
                kind: "at".into(),
                from: None,
            },
        );
        next(&mut m, TutorialStep::Tank);
        assert!(
            m.world
                .game
                .units
                .values()
                .any(|u| u.owner == 0 && m.rules.kinds[u.kind].name == "at")
        );
    }
}
#[test]
fn mode65_tutorial_tank_executes_attack_move_and_death_advances() {
    let mut m = tutorial();
    enter(&mut m, TutorialStep::Tank);
    let id = m.tutorial_state().unwrap().spawned()[0];
    let u = &m.world.game.units[&id];
    assert_eq!(u.at(), Point { x: 107.0, z: 43.0 });
    assert_eq!(m.rules.kinds[u.kind].name, "tank");
    assert_eq!(u.amove, Some(Point { x: 83.0, z: 45.0 }));
    assert!(!m.commands.units[&id].auto_retreat);
    let start = u.at();
    until(&mut m, |m| m.world.game.units[&id].x < start.x - 2.0, 200);
    assert_eq!(m.tutorial_state().unwrap().step(), TutorialStep::Tank);
    kill_step(&mut m);
    next(&mut m, TutorialStep::Trench);
}
#[test]
fn mode66_tutorial_farm_squads_occupy_trench_and_both_must_die() {
    let mut m = tutorial();
    enter(&mut m, TutorialStep::Trench);
    let ids = m.tutorial_state().unwrap().spawned().to_vec();
    assert_eq!(ids.len(), 2);
    for (id, at) in ids
        .iter()
        .zip([Point { x: 91.0, z: 35.0 }, Point { x: 91.0, z: 53.0 }])
    {
        let u = &m.world.game.units[id];
        assert_eq!(u.at(), at);
        assert!(crate::vision::in_trench(&m.world.game, &m.rules, u));
        assert!(!m.commands.units[id].auto_retreat);
    }
    m.world.game.units.get_mut(&ids[0]).unwrap().hp = 0.0;
    m.advance_tick().unwrap();
    assert_eq!(m.tutorial_state().unwrap().step(), TutorialStep::Trench);
    kill_step(&mut m);
    next(&mut m, TutorialStep::Retreat);
}
#[test]
fn mode67_tutorial_retreat_completes_from_accepted_order() {
    let mut m = tutorial();
    enter(&mut m, TutorialStep::Retreat);
    go(&mut m, Point { x: 31.0, z: 45.0 });
    until(&mut m, |m| m.world.game.units[&1].x >= 26.0, 400);
    assert_eq!(m.tutorial_state().unwrap().step(), TutorialStep::Retreat);
    command(&mut m, 0, Command::Retreat { ids: vec![1] });
    assert!(m.world.game.units[&1].retreating);
    next(&mut m, TutorialStep::Artillery);
}
#[test]
fn mode68_tutorial_house_garrisons_and_artillery_cooldown_advances() {
    let mut m = tutorial();
    hold_fire(&mut m);
    m.start.economy.players[0].mp = 10.0;
    enter(&mut m, TutorialStep::Artillery);
    assert_eq!(m.start.economy.players[0].mp, 200.0);
    let ids = m.tutorial_state().unwrap().spawned().to_vec();
    assert_eq!(ids.len(), 2);
    assert_eq!(
        ids.iter()
            .map(|id| m.rules.kinds[m.world.game.units[id].kind].name.as_str())
            .collect::<Vec<_>>(),
        vec!["mg", "rifle"]
    );
    until(
        &mut m,
        |m| {
            ids.iter()
                .all(|id| m.world.game.units[id].garrison.is_some())
        },
        400,
    );
    assert_ne!(
        m.world.game.units[&ids[0]].garrison,
        m.world.game.units[&ids[1]].garrison
    );
    assert_eq!(m.tutorial_state().unwrap().step(), TutorialStep::Artillery);
    m.world.game.reveal = true;
    command(
        &mut m,
        0,
        Command::Support {
            kind: "artillery".into(),
            at: Point { x: 127.0, z: 35.0 },
            direction: Some(0.0),
        },
    );
    assert!(m.start.players[0].support["artillery"] > 0.0);
    assert!(!m.world.game.strikes.is_empty());
    next(&mut m, TutorialStep::Bridgehead);
    assert!(ids.iter().all(|id| m.world.game.units[id].hp > 0.0));
}
#[test]
fn mode69_tutorial_bridgehead_capture_finishes_at_second_point() {
    let mut m = tutorial();
    enter(&mut m, TutorialStep::Bridgehead);
    m.advance_tick().unwrap();
    assert!(m.finish.is_none());
    let at = m.start.points[1].at;
    go(&mut m, at);
    until(&mut m, |m| m.finish.is_some(), 1600);
    let finish = m.finish.unwrap();
    assert_eq!(finish.winner, 0);
    assert_eq!(finish.reason, EndReason::Tutorial);
    assert_eq!(finish.end_at, at);
    assert_eq!(finish.end_tick, m.world.game.tick);
    assert!(m.world.game.reveal);
    assert_eq!(m.start.points[1].owner, Some(0));
    let count = m.timeline.len();
    m.advance_tick().unwrap();
    assert_eq!(m.timeline.len(), count);
}
#[test]
fn mode70_tutorial_authored_map_and_retained_objectives() {
    let map = map();
    assert_eq!(map.dimensions(), (72, 44));
    assert_eq!(
        map.spawns().iter().map(|s| (s.x, s.y)).collect::<Vec<_>>(),
        vec![(5, 21), (68, 38)]
    );
    assert_eq!(
        map.points()
            .iter()
            .map(|p| (p.x, p.y, p.vp, p.mp))
            .collect::<Vec<_>>(),
        vec![(34, 22, 0.0, 2.0), (62, 22, 0.0, 2.0)]
    );
    let m = Match::new_tutorial(&map, options(1)).unwrap();
    assert_eq!(
        m.start
            .points
            .iter()
            .map(|p| (p.at, p.vp, p.mp))
            .collect::<Vec<_>>(),
        vec![
            (Point { x: 69.0, z: 45.0 }, 0.0, 2.0),
            (Point { x: 125.0, z: 45.0 }, 0.0, 2.0)
        ]
    );
}
#[test]
fn mode60_tutorial_runner_delays_dialogue_progresses_all_steps_and_wins() {
    let mut m = tutorial();
    let mut said = Vec::new();
    for tick in 1..=40 {
        let report = m.advance_tick().unwrap();
        if tick < 40 {
            assert!(report.tutorial_lines.is_empty());
        }
        said.extend(report.tutorial_lines);
    }
    assert_eq!(said.len(), 2);
    assert_eq!(said[0].at, Point { x: 27.0, z: 45.0 });
    // Collect every emitted line while satisfying each step through its real consumers.
    hold_fire(&mut m);
    go(&mut m, Point { x: 31.0, z: 45.0 });
    collect_until_step(&mut m, &mut said, TutorialStep::Orchard);
    kill_step(&mut m);
    collect_until_step(&mut m, &mut said, TutorialStep::Crossroads);
    let at = m.start.points[0].at;
    go(&mut m, at);
    collect_until_step(&mut m, &mut said, TutorialStep::Recruit);
    command(
        &mut m,
        0,
        Command::Buy {
            kind: "at".into(),
            from: None,
        },
    );
    collect_until_step(&mut m, &mut said, TutorialStep::Tank);
    kill_step(&mut m);
    collect_until_step(&mut m, &mut said, TutorialStep::Trench);
    kill_step(&mut m);
    collect_until_step(&mut m, &mut said, TutorialStep::Retreat);
    command(&mut m, 0, Command::Retreat { ids: vec![1] });
    collect_until_step(&mut m, &mut said, TutorialStep::Artillery);
    let defenders = m.tutorial_state().unwrap().spawned().to_vec();
    for _ in 0..400 {
        if defenders
            .iter()
            .all(|id| m.world.game.units[id].garrison.is_some())
        {
            break;
        }
        said.extend(m.advance_tick().unwrap().tutorial_lines);
    }
    assert!(
        defenders
            .iter()
            .all(|id| m.world.game.units[id].garrison.is_some())
    );
    m.world.game.reveal = true;
    command(
        &mut m,
        0,
        Command::Support {
            kind: "artillery".into(),
            at: Point { x: 127.0, z: 35.0 },
            direction: Some(0.0),
        },
    );
    collect_until_step(&mut m, &mut said, TutorialStep::Bridgehead);
    // Isolate the teaching progression from the ongoing shelling, as the prototype test does.
    for id in defenders {
        m.world.game.units.get_mut(&id).unwrap().hp = 0.0;
    }
    m.world.game.strikes.clear();
    let at = m.start.points[1].at;
    go(&mut m, at);
    for _ in 0..3000 {
        if m.finish.is_some() {
            break;
        }
        said.extend(m.advance_tick().unwrap().tutorial_lines);
    }
    assert_eq!(m.finish.unwrap().reason, EndReason::Tutorial);
    assert_eq!(said.len(), 18);
    assert_eq!(
        said.iter().map(|line| line.text).collect::<Vec<_>>(),
        LINES.into_iter().flatten().collect::<Vec<_>>()
    );
}
#[test]
fn tutorial_guards_reject_missing_runtime_and_preserve_no_defeat_rule() {
    assert!(Match::new(&map(), options(1)).is_err());
    let mut invalid = tutorial();
    invalid.tutorial = None;
    let tick = invalid.world.game.tick;
    assert!(invalid.advance_tick().is_err());
    assert_eq!(invalid.world.game.tick, tick);
    let mut empty = tutorial();
    for unit in empty.world.game.units.values_mut() {
        unit.hp = 0.0;
    }
    for _ in 0..3 {
        empty.advance_tick().unwrap();
    }
    assert!(empty.finish.is_none());
}

#[test]
fn base19_tutorial_buy_faces_personal_rally_or_centre_and_walks_without_a_producer() {
    // Prototype e4e579a shared/sim.js:3798-3803: immediate spawn, facing, personal rally.
    for rally in [
        None,
        Some(Point { x: 21.0, z: 57.0 }),
        Some(Point { x: 21.0, z: 29.0 }),
    ] {
        let mut m = tutorial();
        assert!(m.building_cells.is_empty());
        assert!(
            !m.world
                .game
                .units
                .values()
                .any(|u| m.rules.kinds[u.kind].building)
        );
        if let Some(at) = rally {
            command(
                &mut m,
                0,
                Command::Rally {
                    ids: vec![],
                    id: None,
                    at: Some(at),
                },
            );
        }
        let home = m.start.players[0].spawn;
        let facing = rally.unwrap_or(Point {
            x: m.world.game.w as f64 * CELL / 2.0,
            z: m.world.game.h as f64 * CELL / 2.0,
        });
        let id = m.next_unit_id;
        command(
            &mut m,
            0,
            Command::Buy {
                kind: "rifle".into(),
                from: None,
            },
        );
        let u = &m.world.game.units[&id];
        assert_eq!(u.owner, 0);
        assert_eq!(m.rules.kinds[u.kind].name, "rifle");
        assert!(math::hypot(u.x - home.x, u.z - home.z) < 12.0);
        assert_eq!(u.rot, math::atan2(facing.z - home.z, facing.x - home.x));
        assert_eq!(u.world_goal, rally);
        if let Some(at) = rally {
            assert!(m.commands.units[&id].rally_drift);
            assert!(!u.path.is_empty());
            let before = math::hypot(u.x - at.x, u.z - at.z);
            for _ in 0..40 {
                m.advance_tick().unwrap();
            }
            let u = &m.world.game.units[&id];
            assert!(math::hypot(u.x - at.x, u.z - at.z) < before - 1.0);
            assert_eq!(m.tick_state.fault(), None);
        } else {
            assert!(u.path.is_empty());
            assert!(!m.commands.units[&id].rally_drift);
        }
    }
}
