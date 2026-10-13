//! World pre-timer command facts on an actual native World constructor.
//! These component witnesses do not advance a World tick or service a path scheduler.
use serde_json::{Value, json};
use ww2_core::{
    economy::Mode,
    game::{LOCAL_DEFENDER_OWNER, ModeKind, Point, Unit},
    logistics::Reserves,
    match_commands::{MoveOutcome, MoveResult},
    match_runtime::Match,
};
// This file uses only the shared real constructor, not restored phase scenes.
#[allow(dead_code)]
#[path = "common/world_scene.rs"]
mod world_scene;

fn scene() -> Match {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/world-phases.json")).unwrap();
    let m = world_scene::make_match(&fixture, &[json!(0), json!(1)]);
    assert_eq!(m.options.mode, Mode::World);
    assert_eq!(m.world.game.mode, Some(ModeKind::World));
    assert!(m.world_runtime.is_some());
    m
}

fn same_unit(actual: &Unit, expected: &Unit) {
    assert_eq!(actual.pace.to_bits(), expected.pace.to_bits());
    let mut actual = actual.clone();
    let mut expected = expected.clone();
    // The absent pace sentinel is NaN. Only detached comparison copies are normalized.
    actual.pace = 0.0;
    expected.pace = 0.0;
    assert_eq!(actual, expected);
}

#[test]
fn local_guard_home_snap_preserves_the_existing_grid_and_other_command_facts() {
    for distance in [16.0, 17.0] {
        let mut m = scene();
        let id = m
            .world
            .game
            .units
            .values()
            .find(|u| {
                u.owner == LOCAL_DEFENDER_OWNER
                    && m.rules.kinds[u.kind].name == "rifle"
                    && m.commands.units[&u.id].guard_home.is_some()
            })
            .expect("World constructor publishes a live local rifle guard")
            .id;
        let target = m
            .world
            .game
            .units
            .values()
            .find(|u| u.owner == 0 && m.rules.kinds[u.kind].infantry)
            .expect("World constructor publishes a commander infantry actor")
            .id;
        let home = m.commands.units[&id].guard_home.unwrap();
        let goal = Point {
            x: home.x,
            z: home.z + 9.0,
        };
        let unit = m.world.game.units.get_mut(&id).unwrap();
        assert!(unit.hp > 0.0);
        unit.x = home.x + distance;
        unit.z = home.z;
        unit.path = vec![Point {
            x: unit.x + 1.0,
            z: unit.z,
        }];
        unit.path_gen += 1;
        unit.attack = true;
        unit.world_goal = Some(goal);
        m.grid.update(id, unit);
        let control = m.commands.units.get_mut(&id).unwrap();
        control.attack_id = target;
        control.route_end = Some(goal);
        control.move_result = Some(MoveResult::Unreachable { tick: -1 });
        m.firing.get_mut(id).unwrap().target_id = target;
        let before_unit = m.world.game.units[&id].clone();
        let before_control = m.commands.units[&id].clone();
        let grid = format!("{:?}", m.grid);
        let rng = m.start.random.snapshot();
        let environment_seed = m.start.environment_seed;
        let tick = m.world.game.tick;
        m.validate_world_command_prelude(id).unwrap();
        m.apply_world_guard_home(id).unwrap();
        let mut expected_unit = before_unit;
        let mut expected_control = before_control;
        if distance > 16.0 {
            expected_unit.x = home.x;
            expected_unit.z = home.z;
            expected_unit.path.clear();
            expected_unit.attack = false;
            expected_control.attack_id = 0;
        }
        same_unit(&m.world.game.units[&id], &expected_unit);
        assert_eq!(m.commands.units[&id], expected_control);
        assert_eq!(m.firing.get(id).unwrap().target_id, target);
        assert_eq!(
            format!("{:?}", m.grid),
            grid,
            "Source does not repair the grid here"
        );
        assert_eq!(m.world.game.tick, tick);
        assert_eq!(m.start.random.snapshot(), rng);
        assert_eq!(m.start.environment_seed, environment_seed);
    }
}

#[test]
fn world_retreat_completion_selects_goal_radius_and_keeps_the_forced_endpoint_rule() {
    // near voluntary, far voluntary with stale near endpoint, near forced while moving,
    // and far forced with its actual empty/stopped endpoint condition satisfied.
    for (goal_distance, forced, moving, completed) in [
        (15.0, false, true, true),
        (30.0, false, false, false),
        (15.0, true, true, false),
        (30.0, true, false, true),
    ] {
        let mut m = scene();
        let id = m
            .world
            .game
            .units
            .values()
            .find(|u| u.owner == 0 && m.rules.kinds[u.kind].infantry)
            .expect("real World commander infantry")
            .id;
        let unit = m.world.game.units.get_mut(&id).unwrap();
        let at = unit.at();
        let goal = Point {
            x: at.x + goal_distance,
            z: at.z,
        };
        let path = if moving { vec![goal] } else { Vec::new() };
        unit.world_goal = Some(goal);
        unit.retreating = true;
        unit.path = path.clone();
        unit.move_speed = if moving { 2.0 } else { 0.0 };
        // A retained far goal must not trigger unrepresented routing in this prelude test.
        unit.repath = 1.0;
        let generation = unit.path_gen;
        let control = m.commands.units.get_mut(&id).unwrap();
        assert!(control.orders.is_empty());
        control.route_end = Some(at);
        control.outcome = Some(MoveOutcome::Interrupted);
        control.outcome_tick = Some(-1);
        control.move_result = Some(MoveResult::Unreachable { tick: -1 });
        if forced {
            m.logistics.insert(
                id,
                Reserves {
                    ammo: 0.0,
                    ammo_max: 0.0,
                    ammo_price: 0.0,
                    ammo_credit: None,
                    provisions: 0.0,
                    fuel: None,
                    emergency: None,
                    warning: None,
                    forced: true,
                    stranded: false,
                    exhausted: false,
                    supply_rate: None,
                    cut_for: None,
                },
            );
        }
        let rng = m.start.random.snapshot();
        let environment_seed = m.start.environment_seed;
        let tick = m.world.game.tick;
        let requests = format!("{:?}", m.commands.requests);
        let grid = format!("{:?}", m.grid);
        m.prepare_unit_commands(id, false).unwrap();
        let unit = &m.world.game.units[&id];
        let control = &m.commands.units[&id];
        assert_eq!(unit.path_gen, generation);
        assert_eq!(control.route_end, Some(at));
        assert_eq!(unit.move_speed, if moving { 2.0 } else { 0.0 });
        if completed {
            assert_eq!(unit.world_goal, None);
            assert!(!unit.retreating);
            assert!(unit.path.is_empty());
            assert_eq!(control.outcome, Some(MoveOutcome::Arrived));
            assert_eq!(control.outcome_tick, Some(tick));
            assert_eq!(control.move_result, Some(MoveResult::Completed { tick }));
        } else {
            assert_eq!(unit.world_goal, Some(goal));
            assert!(unit.retreating);
            assert_eq!(unit.path, path);
            assert_eq!(control.outcome, Some(MoveOutcome::Interrupted));
            assert_eq!(control.outcome_tick, Some(-1));
            assert_eq!(
                control.move_result,
                Some(MoveResult::Unreachable { tick: -1 })
            );
        }
        assert_eq!(format!("{:?}", m.commands.requests), requests);
        assert_eq!(format!("{:?}", m.grid), grid);
        assert_eq!(m.world.game.tick, tick);
        assert_eq!(m.start.random.snapshot(), rng);
        assert_eq!(m.start.environment_seed, environment_seed);
    }
}

fn support_bits(m: &Match) -> Vec<std::collections::BTreeMap<String, u64>> {
    m.start
        .players
        .iter()
        .map(|p| {
            p.support
                .iter()
                .map(|(kind, value)| (kind.clone(), value.to_bits()))
                .collect()
        })
        .collect()
}

fn prime_support_cooldowns(m: &mut Match) {
    for (slot, player) in m.start.players.iter_mut().enumerate() {
        for (index, value) in player.support.values_mut().enumerate() {
            *value = [2.0, 0.0, -1.0][(slot + index) % 3];
        }
    }
    m.start.economy.players[0].out = true;
    m.start.economy.players[1].away = true;
}

#[test]
fn world_support_timers_decay_every_raw_commander_cooldown() {
    let mut m = scene();
    prime_support_cooldowns(&mut m);
    let before = support_bits(&m);
    assert!(before.iter().all(|row| !row.is_empty()));
    let wallets = m.start.economy.clone();
    let rng = m.start.random.snapshot();
    let environment_seed = m.start.environment_seed;
    let tick = m.world.game.tick;
    m.validate_world_support_timers(0.05).unwrap();
    assert_eq!(support_bits(&m), before, "timer admission is read-only");
    m.advance_world_support_timers(0.05).unwrap();
    for (slot, row) in before.iter().enumerate() {
        assert_eq!(m.start.players[slot].support.len(), row.len());
        for (kind, bits) in row {
            assert_eq!(
                m.start.players[slot].support[kind].to_bits(),
                (f64::from_bits(*bits) - 0.05).to_bits(),
                "slot {slot}, {kind}: out/away and negative values still decay",
            );
        }
    }
    assert_eq!(m.start.economy, wallets);
    assert!(m.support.covers.is_empty() && m.world.game.strikes.is_empty());
    assert!(m.support.events.is_empty());
    assert_eq!(m.world.game.tick, tick);
    assert_eq!(m.start.random.snapshot(), rng);
    assert_eq!(m.start.environment_seed, environment_seed);
}

#[test]
fn world_support_timer_refusals_preserve_every_timer_before_publication() {
    use ww2_core::{
        game::{Strike, StrikeKind},
        match_support::{FighterCover, SupportKind, SupportTiming},
    };
    for invalid in [
        "missing key",
        "team",
        "nonfinite",
        "overflow",
        "strike",
        "cover",
    ] {
        let mut m = scene();
        prime_support_cooldowns(&mut m);
        let last = m.start.players.len() - 1;
        let key = m.start.players[last]
            .support
            .keys()
            .next_back()
            .unwrap()
            .clone();
        let mut dt = 0.05;
        match invalid {
            "missing key" => {
                m.start.players[last].support.remove(&key);
            }
            "team" => m.world.game.players[last].team = 5,
            "nonfinite" => {
                m.start.players[last].support.insert(key, f64::NAN);
            }
            "overflow" => {
                // Earlier finite rows can be prepared, but the last subtraction overflows.
                m.start.players[last].support.insert(key, -f64::MAX);
                dt = f64::MAX;
            }
            "strike" => m.world.game.strikes.push(Strike {
                live: true,
                kind: StrikeKind::Other,
                owner: 0,
                x: 20.0,
                z: 20.0,
                dir: 0.0,
                runtime: Some(SupportTiming {
                    kind: SupportKind::Artillery,
                    delay: 1.0,
                    left: 1.0,
                    next: 1.0,
                }),
            }),
            "cover" => m.support.covers.push(FighterCover {
                team: 0,
                owner: 0,
                at: Point { x: 20.0, z: 20.0 },
                radius: 10.0,
                remaining: 2.0,
            }),
            _ => unreachable!(),
        }
        let timers = support_bits(&m);
        let covers = m.support.covers.clone();
        let strikes = m.world.game.strikes.clone();
        let wallets = m.start.economy.clone();
        let rng = m.start.random.snapshot();
        let environment_seed = m.start.environment_seed;
        let tick = m.world.game.tick;
        assert!(m.validate_world_support_timers(dt).is_err(), "{invalid}");
        assert_eq!(support_bits(&m), timers, "{invalid}: pure admission");
        let error = m.advance_world_support_timers(dt).unwrap_err();
        if invalid == "overflow" {
            assert!(error.to_string().contains("overflow"), "{error}");
        }
        assert_eq!(support_bits(&m), timers, "{invalid}: all raw timer bits");
        assert_eq!(m.support.covers, covers, "{invalid}: covers unchanged");
        assert_eq!(
            m.world.game.strikes, strikes,
            "{invalid}: strikes unchanged"
        );
        assert_eq!(m.start.economy, wallets, "{invalid}: wallets unchanged");
        assert!(m.support.events.is_empty());
        assert_eq!(m.world.game.tick, tick);
        assert_eq!(m.start.random.snapshot(), rng);
        assert_eq!(m.start.environment_seed, environment_seed);
    }
}
