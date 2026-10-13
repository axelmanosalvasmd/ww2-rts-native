//! World command, finish and retirement facts on an actual native World constructor.
//! Component witnesses accompany one quiet turn through the sole tick runner.
use serde_json::{Value, json};
use ww2_core::{
    economy::Mode,
    game::{LOCAL_DEFENDER_OWNER, ModeKind, Point, Unit},
    logistics::Reserves,
    match_commands::{MoveOutcome, MoveResult},
    match_end::{EndReason, MatchFinish, MatchSample},
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

#[test]
fn fresh_world_scene_finish_publishes_one_terminal_sample_and_refuses_reinitialization() {
    fn retained(m: &Match) -> String {
        format!(
            "{:?}",
            (
                m.world.game.tick,
                m.commands.next_tick(),
                m.commands.pending_commands(),
                &m.start.economy,
                m.start.random.snapshot(),
                m.start.environment_seed,
                &m.world.game.units,
                &m.commands.units,
                &m.grid,
                &m.tick_state,
            )
        )
    }
    fn terminal(m: &Match) -> String {
        format!("{:?}", (m.finish, &m.timeline, m.world.game.reveal))
    }

    let mut m = scene();
    assert_eq!(m.world.game.tick, 0);
    assert_eq!(m.commands.next_tick(), Some(1));
    assert_eq!(m.commands.pending_commands(), 0);
    assert!(m.finish.is_none() && !m.world.game.reveal && m.timeline.is_empty());
    let mut structures = vec![0.0; 2];
    for unit in m.world.game.units.values() {
        if unit.hp > 0.0 && m.rules.kinds[unit.kind].structure && unit.owner != LOCAL_DEFENDER_OWNER
        {
            structures[m.start.economy.players[unit.owner].team as usize] += 1.0;
        }
    }
    assert!(structures.iter().all(|n| *n > 0.0));
    let mut points = vec![0.0; 2];
    for point in &m.start.points {
        if let Some(owner) = point.owner {
            points[m.start.economy.players[owner].team as usize] += 1.0;
        }
    }
    let before = retained(&m);
    let at = Point { x: 512.0, z: 512.0 };
    let finish = m
        .initialize_tick_scene_finish(0, EndReason::Nations, Some(at))
        .unwrap();
    assert_eq!(
        finish,
        MatchFinish {
            winner: 0,
            reason: EndReason::Nations,
            end_at: at,
            end_tick: 0
        }
    );
    assert_eq!(m.finish, Some(finish));
    assert!(m.world.game.reveal);
    assert_eq!(
        m.timeline,
        vec![MatchSample::Territories {
            t: 0.0,
            points,
            structures
        }]
    );
    assert_eq!(
        retained(&m),
        before,
        "finishing does not advance or consume the scene"
    );
    let terminal_before = terminal(&m);
    assert!(
        m.initialize_tick_scene_finish(1, EndReason::Nations, Some(at))
            .is_err()
    );
    assert_eq!(retained(&m), before);
    assert_eq!(
        terminal(&m),
        terminal_before,
        "repeat finish cannot append another sample"
    );

    for refusal in ["winner", "position", "queued", "clock"] {
        let mut m = scene();
        let (winner, end_at, reason) = match refusal {
            "winner" => (9, at, "unknown winning team"),
            "position" => (
                0,
                Point {
                    x: f64::NAN,
                    z: at.z,
                },
                "invalid Match end position",
            ),
            "queued" => {
                let id = m
                    .world
                    .game
                    .units
                    .values()
                    .find(|u| u.owner == 0 && m.rules.kinds[u.kind].infantry)
                    .unwrap()
                    .id;
                m.enqueue_command(
                    ww2_core::orders::OrderKey {
                        tick: 1,
                        issuer: 0,
                        sequence: 0,
                    },
                    ww2_core::match_commands::Command::Stop {
                        ids: vec![i64::from(id)],
                    },
                )
                .unwrap();
                assert_eq!(m.commands.pending_commands(), 1);
                (0, at, "fresh unfinished empty command queue")
            }
            "clock" => {
                m.initialize_tick_scene(1, Default::default()).unwrap();
                assert_eq!(m.world.game.tick, 1);
                assert_eq!(m.commands.next_tick(), Some(2));
                (0, at, "fresh unfinished empty command queue")
            }
            _ => unreachable!(),
        };
        let before = retained(&m);
        let terminal_before = terminal(&m);
        let error = m
            .initialize_tick_scene_finish(winner, EndReason::Nations, Some(end_at))
            .unwrap_err();
        assert!(error.contains(reason), "{refusal}: {error}");
        assert_eq!(retained(&m), before, "{refusal} must preserve scene state");
        assert_eq!(
            terminal(&m),
            terminal_before,
            "{refusal} must not publish a finish"
        );
    }
}

#[test]
fn world_tick_retires_a_real_casualty_before_income_and_refuses_scene_finish_after_start() {
    let mut m = scene();
    for (&id, unit) in &mut m.world.game.units {
        unit.hold_pos = true;
        let control = m.commands.units.get_mut(&id).unwrap();
        control.hold_fire = true;
        control.auto_cast = false;
    }
    let id = m
        .world
        .game
        .units
        .values()
        .find(|u| {
            let kind = &m.rules.kinds[u.kind];
            let combat = m.combat.get(u.id).unwrap();
            u.owner == 0
                && u.hp > 0.0
                && kind.infantry
                && !kind.structure
                && !kind.air
                && !kind.rail
                && u.garrison.is_none()
                && !u.riding
                && combat.cargo.is_empty()
                && combat.carrier.is_none()
        })
        .expect("the actual World constructor supplies a commander ground infantry actor")
        .id;
    assert!(!m.world.game.allied(0, 1));
    m.world.game.units.get_mut(&id).unwrap().hp = 0.0;
    m.combat.get_mut(id).unwrap().last_hit = Some(1);
    let casualty = m.world.game.units[&id].clone();
    let catalog = ww2_core::catalog::Catalog::compiled().unwrap();
    let cost = catalog
        .unit(&m.rules.kinds[casualty.kind].name)
        .unwrap()
        .get("cost")
        .unwrap()
        .number()
        .unwrap();
    let bounty = catalog
        .table("CFG")
        .unwrap()
        .get("bounty")
        .unwrap()
        .number()
        .unwrap();
    let wallets = m.start.economy.clone();
    let stories = m.story.clone();
    let rng = m.start.random.snapshot();
    let dt = m.rules.movement.tick;
    let report = m
        .advance_tick()
        .expect("actual quiet World turn must finish all admitted phases");
    assert_eq!(report.tick, 1);
    assert_eq!(m.world.game.tick, 1);
    assert_eq!(m.commands.next_tick(), Some(2));
    assert!(
        report.unit_order.contains(&id),
        "the late captured list contains the actual casualty"
    );
    assert!(!m.world.game.units.contains_key(&id));
    assert_eq!(
        report.retired.len(),
        1,
        "only the authored casualty was retired"
    );
    let retired = &report.retired[0];
    same_unit(&retired.unit, &casualty);
    assert_eq!(retired.last_hit, Some(1));
    assert_eq!(retired.enemy_killer, Some(1));
    assert!(
        report.world_removed.is_empty(),
        "ordinary death is not administrative elimination"
    );
    for (slot, before) in stories.iter().enumerate() {
        let mut expected = *before;
        expected.losses += if slot == 0 { 1.0 } else { 0.0 };
        expected.kills += if slot == 1 { 1.0 } else { 0.0 };
        assert_eq!(
            m.story[slot], expected,
            "exactly one commander loss and enemy kill"
        );
    }
    for (slot, before) in wallets.players.iter().enumerate() {
        let after = &m.start.economy.players[slot];
        let expected = (before.mp + if slot == 1 { cost * bounty } else { 0.0 }) + after.inc * dt;
        assert_eq!(
            after.mp.to_bits(),
            expected.to_bits(),
            "commander {slot}: bounty is paid once before actual World income"
        );
    }
    use ww2_core::match_tick::TickPhase;
    let phase = |wanted| {
        assert_eq!(report.phases.iter().filter(|p| **p == wanted).count(), 1);
        report.phases.iter().position(|p| *p == wanted).unwrap()
    };
    assert!(phase(TickPhase::Death) < phase(TickPhase::WorldIncome));
    assert!(phase(TickPhase::WorldIncome) < phase(TickPhase::Convoys));
    assert!(phase(TickPhase::Convoys) < phase(TickPhase::WorldRegions));
    assert!(m.tick_state.fault().is_none());
    assert_eq!(m.start.random.snapshot(), rng);
    assert!(m.finish.is_none());
    let state = |m: &Match| {
        format!(
            "{:?}",
            (
                (
                    m.world.game.tick,
                    m.commands.next_tick(),
                    m.commands.pending_commands(),
                    m.finish,
                    &m.timeline,
                    m.world.game.reveal,
                    &m.start.economy,
                ),
                (
                    m.start.random.snapshot(),
                    m.start.environment_seed,
                    &m.story,
                    &m.world.game.units,
                    &m.commands.units,
                    &m.grid,
                    &m.tick_state,
                ),
            )
        )
    };
    let before = state(&m);
    assert!(
        m.initialize_tick_scene_finish(0, EndReason::Nations, Some(Point { x: 512.0, z: 512.0 }))
            .unwrap_err()
            .contains("fresh unfinished empty command queue")
    );
    assert_eq!(
        state(&m),
        before,
        "a genuinely started World turn cannot be reinitialized with a scene finish"
    );
}

#[test]
fn world_firing_admission_refuses_before_a_recorded_technology_payment_or_boundary() {
    use ww2_core::{economy::Technology, match_commands::Command, orders::OrderKey};
    let mut m = scene();
    // Declared technology input enables the actual command's existing purchase gate.
    m.options.tech = true;
    for (&id, unit) in &mut m.world.game.units {
        unit.hold_pos = true;
        let control = m.commands.units.get_mut(&id).unwrap();
        control.hold_fire = true;
        control.auto_cast = false;
    }
    let research = m.production.player(0).unwrap();
    assert!(research.jobs().is_empty());
    let cost = ww2_core::economy::tech_cost(research.tech(), Technology::Tier, Mode::World)
        .unwrap()
        .expect("the real constructor has a purchasable next tier");
    assert!(cost.mp > 0.0);
    m.start.economy.players[0].mp = cost.mp + 100.0;
    m.start.economy.players[0].mun = cost.mun + 100.0;
    assert!(!m.start.economy.players[0].out && !m.start.economy.players[0].away);
    assert!(
        m.world.game.units.values().any(|unit| {
            unit.owner == 0
                && unit.hp > 0.0
                && unit.built.is_some_and(|built| built >= 1.0)
                && m.rules.kinds[unit.kind].name == "hq"
        }),
        "the real World constructor supplies the required tier laboratory"
    );
    m.enqueue_command(
        OrderKey {
            tick: 1,
            issuer: 0,
            sequence: 0,
        },
        Command::Technology {
            kind: Technology::Tier,
        },
    )
    .unwrap();
    assert_eq!(m.commands.pending_commands(), 1);
    assert_eq!(m.commands.next_tick(), Some(1));
    let id = m
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 0 && m.rules.kinds[u.kind].infantry)
        .unwrap()
        .id;
    m.firing.get_mut(id).unwrap().cooldown = f64::from_bits(0x7ff8_0000_0000_0123);
    let firing_bits = |m: &Match| {
        m.firing
            .units
            .iter()
            .map(|(&id, row)| {
                (
                    id,
                    row.target_id,
                    row.cooldown.to_bits(),
                    row.retarget.to_bits(),
                )
            })
            .collect::<Vec<_>>()
    };
    let metadata = firing_bits(&m);
    let commands = format!(
        "{:?}",
        (
            m.commands.next_tick(),
            m.commands.pending_commands(),
            &m.commands.units,
            m.commands.path_requests()
        )
    );
    let production = format!("{:?}", m.production);
    let units = format!("{:?}", m.world.game.units);
    let grid = format!("{:?}", m.grid);
    let firing = format!("{:?}", m.firing);
    let tick_state = format!("{:?}", m.tick_state);
    let wallets = m.start.economy.clone();
    let story = m.story.clone();
    let rng = m.start.random.snapshot();
    let seed = m.start.environment_seed;
    assert!(
        m.firing.events.is_empty() && m.support.events.is_empty() && m.aviation.events.is_empty()
    );
    let failure = m
        .advance_tick()
        .expect_err("canonical firing admission must reject the whole boundary");
    assert!(
        failure.reason.contains("invalid canonical firing unit"),
        "{}",
        failure.reason
    );
    assert_eq!(failure.tick, 0);
    assert!(failure.events.is_empty());
    assert_eq!(
        failure.retirement,
        ww2_core::match_combat::RetirementProgress::default()
    );
    assert_eq!(m.world.game.tick, 0);
    assert_eq!(m.commands.next_tick(), Some(1));
    assert_eq!(m.commands.pending_commands(), 1);
    assert_eq!(
        format!(
            "{:?}",
            (
                m.commands.next_tick(),
                m.commands.pending_commands(),
                &m.commands.units,
                m.commands.path_requests()
            )
        ),
        commands,
        "the actual queued Technology order remains pending"
    );
    assert_eq!(format!("{:?}", m.production), production);
    assert_eq!(format!("{:?}", m.world.game.units), units);
    assert_eq!(format!("{:?}", m.grid), grid);
    assert_eq!(format!("{:?}", m.firing), firing);
    assert_eq!(
        firing_bits(&m),
        metadata,
        "the NaN payload and every firing row retain their exact bits"
    );
    assert_eq!(format!("{:?}", m.tick_state), tick_state);
    assert!(m.tick_state.fault().is_none());
    assert_eq!(m.start.economy, wallets);
    assert_eq!(m.story, story);
    assert_eq!(m.start.random.snapshot(), rng);
    assert_eq!(m.start.environment_seed, seed);
    assert!(
        m.firing.events.is_empty() && m.support.events.is_empty() && m.aviation.events.is_empty()
    );
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
        let requests = format!("{:?}", m.commands.path_requests());
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
        assert_eq!(format!("{:?}", m.commands.path_requests()), requests);
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

#[test]
fn world_income_facts_keep_captured_order_and_refuse_missing_or_incomplete_facts() {
    use ww2_core::match_combat::{RetiredUnit, RetirementProgress};
    let m = scene();
    let current: Vec<_> = m.world.game.units.keys().copied().collect();
    assert!(current.len() > 2);
    let outside_capture = *current.last().unwrap();
    // This current actor is outside the earlier membership. Capture order is deliberately
    // distinct from the canonical current order, and must never be reconstructed from it.
    let captured: Vec<_> = current[..current.len() - 1].iter().rev().copied().collect();
    let wallets = m.start.economy.clone();
    let rng = m.start.random.snapshot();
    let environment_seed = m.start.environment_seed;
    let units = format!("{:?}", m.world.game.units);
    let controls = format!("{:?}", m.commands.units);
    let grid = format!("{:?}", m.grid);
    let tick = m.world.game.tick;
    let complete_without_removals = RetirementProgress::default();
    let facts = m
        .world_tick_income_facts(&captured, &complete_without_removals)
        .unwrap();
    assert_eq!(facts.iter().map(|row| row.id).collect::<Vec<_>>(), captured);
    assert_eq!(facts.len(), current.len() - 1);
    assert!(!facts.iter().any(|row| row.id == outside_capture));
    for row in &facts {
        let unit = &m.world.game.units[&row.id];
        assert_eq!((row.owner, row.kind), (unit.owner, unit.kind));
        assert_eq!(row.hp.to_bits(), unit.hp.to_bits());
    }
    let first = captured[0];
    let duplicate = m
        .world_tick_income_facts(&[first, first], &complete_without_removals)
        .unwrap_err();
    assert!(duplicate.contains("duplicate"), "{duplicate}");
    let missing = m.next_unit_id;
    assert!(!m.world.game.units.contains_key(&missing));
    let missing_error = m
        .world_tick_income_facts(&[first, missing], &complete_without_removals)
        .unwrap_err();
    assert!(missing_error.contains("missing final"), "{missing_error}");
    // Deliberately invalid progress only. This live object was never retired, and no
    // completed wreck, bounty or lifecycle prefix is fabricated for a success witness.
    let incomplete = RetirementProgress {
        removed: vec![RetiredUnit {
            unit: m.world.game.units[&first].clone(),
            last_hit: m.combat.get(first).unwrap().last_hit,
            enemy_killer: None,
        }],
        ..Default::default()
    };
    let incomplete_error = m
        .world_tick_income_facts(&captured, &incomplete)
        .unwrap_err();
    assert!(
        incomplete_error.contains("completed ordinary retirement"),
        "{incomplete_error}"
    );
    assert_eq!(incomplete.removed.len(), 1);
    assert_eq!(
        [
            incomplete.completed,
            incomplete.companions_notified,
            incomplete.stories_notified,
            incomplete.routes_retained,
        ],
        [0; 4]
    );
    assert_eq!(m.start.economy, wallets);
    assert_eq!(m.start.random.snapshot(), rng);
    assert_eq!(m.start.environment_seed, environment_seed);
    assert_eq!(format!("{:?}", m.world.game.units), units);
    assert_eq!(format!("{:?}", m.commands.units), controls);
    assert_eq!(format!("{:?}", m.grid), grid);
    assert_eq!(m.world.game.tick, tick);
}

#[test]
fn idle_local_world_reaction_uses_actor_policy_without_routing_or_mutation() {
    let mut m = scene();
    let id = m
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == LOCAL_DEFENDER_OWNER && m.rules.kinds[u.kind].name == "rifle")
        .unwrap()
        .id;
    let unit = m.world.game.units.get_mut(&id).unwrap();
    // Reach the actual idle reaction body rather than the constructor's hold-position exit.
    unit.hold_pos = false;
    assert!(unit.path.is_empty() && unit.world_goal.is_none() && unit.hit_from.is_none());
    assert!(!unit.retreating && !unit.build && !unit.dig && !unit.entrench && !unit.riding);
    let control = m.commands.units.get_mut(&id).unwrap();
    control.react = 2.0;
    assert!(control.orders.is_empty());
    assert_eq!(control.attack_id, 0);
    assert!(control.nade.is_none() && control.enter.is_none() && control.fire_at.is_none());
    assert_eq!(m.firing.get(id).unwrap().target_id, 0);
    let before = m.world.game.units[&id].clone();
    let control = m.commands.units[&id].clone();
    let grid = format!("{:?}", m.grid);
    let requests = format!("{:?}", m.commands.path_requests());
    let wallets = m.start.economy.clone();
    let rng = m.start.random.snapshot();
    let environment_seed = m.start.environment_seed;
    let tick = m.world.game.tick;
    m.react_unit(id).unwrap();
    same_unit(&m.world.game.units[&id], &before);
    assert_eq!(m.commands.units[&id], control);
    assert_eq!(m.firing.get(id).unwrap().target_id, 0);
    assert_eq!(format!("{:?}", m.grid), grid);
    assert_eq!(format!("{:?}", m.commands.path_requests()), requests);
    assert_eq!(m.start.economy, wallets);
    assert_eq!(m.start.random.snapshot(), rng);
    assert_eq!(m.start.environment_seed, environment_seed);
    assert_eq!(m.world.game.tick, tick);
}

#[test]
fn physical_local_threat_uses_canonical_visibility_and_implicit_lookup_refuses() {
    use std::collections::BTreeMap;
    use ww2_core::command_destinations::{Context, Error, threat_for, threat_for_with_visibility};
    let mut m = scene();
    let id = m
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == LOCAL_DEFENDER_OWNER && m.rules.kinds[u.kind].name == "rifle")
        .unwrap()
        .id;
    let enemy = m
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 0 && m.rules.kinds[u.kind].infantry && m.rules.kinds[u.kind].weapon)
        .expect("actual World constructor publishes an armed commander squad")
        .id;
    let at = m.world.game.units[&id].at();
    assert!(m.world.game.units[&id].hit_from.is_none());
    let enemy_at = Point {
        x: at.x + 3.0,
        z: at.z + 4.0,
    };
    let enemy_unit = m.world.game.units.get_mut(&enemy).unwrap();
    enemy_unit.x = enemy_at.x;
    enemy_unit.z = enemy_at.z;
    m.grid.update(enemy, enemy_unit);
    // Declared canonical actor visibility. No callback manufactures a hit or enemy.
    m.world_runtime.as_mut().unwrap().local_defender.visible = vec![enemy];
    let rng = m.start.random.snapshot();
    let environment_seed = m.start.environment_seed;
    let units = format!("{:?}", m.world.game.units);
    let controls = format!("{:?}", m.commands.units);
    let grid = format!("{:?}", m.grid);
    let wallets = m.start.economy.clone();
    let query = |m: &Match| {
        let hit_at: BTreeMap<_, _> = m.combat.iter().map(|(id, c)| (id, c.hit_at)).collect();
        let points: Vec<_> = m.start.points.iter().map(|p| p.at).collect();
        let context = Context {
            game: &m.world.game,
            rules: &m.rules,
            grid: &m.grid,
            points: &points,
            hit_at: &hit_at,
        };
        let actor = m.actor_policy(LOCAL_DEFENDER_OWNER).unwrap();
        assert_eq!(actor.source_owner, -1);
        assert_eq!(actor.economy_slot, None);
        assert_eq!(threat_for(&context, id, at), Err(Error::UnsupportedWorld));
        threat_for_with_visibility(&context, id, at, actor.visible).unwrap()
    };
    assert_eq!(query(&m), Some(enemy_at));
    m.world_runtime
        .as_mut()
        .unwrap()
        .local_defender
        .visible
        .clear();
    assert_eq!(query(&m), None, "an unseen armed actor is not a threat");
    assert_eq!(format!("{:?}", m.world.game.units), units);
    assert_eq!(format!("{:?}", m.commands.units), controls);
    assert_eq!(format!("{:?}", m.grid), grid);
    assert_eq!(m.start.economy, wallets);
    assert_eq!(m.start.random.snapshot(), rng);
    assert_eq!(m.start.environment_seed, environment_seed);
}

#[test]
fn local_world_spacing_reaction_publishes_the_real_cover_route_endpoint() {
    let mut m = scene();
    let guards: Vec<_> = m
        .world
        .game
        .units
        .values()
        .filter(|u| u.owner == LOCAL_DEFENDER_OWNER && m.rules.kinds[u.kind].name == "rifle")
        .take(2)
        .map(|u| u.id)
        .collect();
    assert_eq!(
        guards.len(),
        2,
        "actual constructor supplies both local actors"
    );
    let (lower, reacting) = (guards[0], guards[1]);
    assert!(lower < reacting);
    let game = &m.world.game;
    let mut patch = None;
    'find: for y in (8..game.h - 8).step_by(8) {
        for x in (8..game.w - 8).step_by(8) {
            let at = Point {
                x: x as f64 * 2.0 + 1.0,
                z: y as f64 * 2.0 + 1.0,
            };
            let open = (y - 5..=y + 5).all(|z| {
                (x - 5..=x + 5).all(|x| {
                    let cell = z * game.w + x;
                    game.chars[cell] == b'.' && game.height.as_ref().is_none_or(|h| h[cell] == 0)
                })
            });
            let clear = game.units.values().all(|u| {
                guards.contains(&u.id) || ww2_core::math::hypot(u.x - at.x, u.z - at.z) > 20.0
            });
            if open && clear {
                patch = Some((at, y * game.w + x + 2));
                break 'find;
            }
        }
    }
    let (at, cover_cell) = patch.expect("real World map has a flat unoccupied open patch");
    let cover_at = Point {
        x: at.x + 4.0,
        z: at.z,
    };
    for (id, x) in [(lower, at.x - 1.0), (reacting, at.x)] {
        let unit = m.world.game.units.get_mut(&id).unwrap();
        unit.x = x;
        unit.z = at.z;
        unit.hold_pos = false;
        assert!(unit.path.is_empty() && unit.hit_from.is_none() && unit.world_goal.is_none());
        assert!(!unit.retreating && !unit.build && !unit.dig && !unit.entrench);
        m.grid.update(id, unit);
        let control = m.commands.units.get_mut(&id).unwrap();
        control.react = 0.0;
        assert!(control.orders.is_empty());
        assert_eq!(control.attack_id, 0);
        assert_eq!(m.firing.get(id).unwrap().target_id, 0);
    }
    m.world.set_cell(cover_cell, b'T').unwrap();
    assert_eq!(
        m.world.game.cell_of(cover_at.x, cover_at.z),
        Some(cover_cell)
    );
    assert_eq!(
        ww2_core::terrain::cover_rank(&m.world.game, &m.rules, at, None),
        3
    );
    assert_eq!(
        ww2_core::terrain::cover_rank(&m.world.game, &m.rules, cover_at, None),
        0
    );
    m.refresh_world_visibility().unwrap();
    assert!(
        m.world_runtime.as_ref().unwrap().memory.team(-1).is_none(),
        "visibility does not invent a routing read"
    );
    let stale_end = at;
    m.commands.units.get_mut(&reacting).unwrap().route_end = Some(stale_end);
    let generation = m.world.game.units[&reacting].path_gen;
    let grid = format!("{:?}", m.grid);
    let requests = format!("{:?}", m.commands.path_requests());
    let rng = m.start.random.snapshot();
    let environment_seed = m.start.environment_seed;
    let wallets = m.start.economy.clone();
    let tick = m.world.game.tick;
    m.react_unit(reacting).unwrap();
    let unit = &m.world.game.units[&reacting];
    assert_eq!(
        unit.at(),
        at,
        "a reaction assigns a route before physical movement"
    );
    assert!(unit.drift);
    assert_eq!(unit.path_gen, generation + 1);
    assert!(
        !unit.path.is_empty(),
        "real World search must succeed for the authored cover"
    );
    assert_eq!(unit.path.last().copied(), Some(cover_at));
    assert_eq!(
        m.commands.units[&reacting].route_end,
        unit.path.last().copied()
    );
    assert_ne!(m.commands.units[&reacting].route_end, Some(stale_end));
    assert_eq!(m.commands.units[&reacting].react, 1.0);
    let routing = m.world_routing(LOCAL_DEFENDER_OWNER).unwrap();
    assert_ne!(
        routing.query().navigation().flags[cover_cell] & ww2_core::game::TRENCH,
        0
    );
    assert!(
        m.world_runtime
            .as_ref()
            .unwrap()
            .memory
            .team(-1)
            .unwrap()
            .contains_cell(cover_cell)
    );
    assert_eq!(format!("{:?}", m.grid), grid);
    assert_eq!(format!("{:?}", m.commands.path_requests()), requests);
    assert_eq!(m.commands.pending_commands(), 0);
    assert_eq!(m.start.economy, wallets);
    assert_eq!(m.start.random.snapshot(), rng);
    assert_eq!(m.start.environment_seed, environment_seed);
    assert_eq!(m.world.game.tick, tick);
}
