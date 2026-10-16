//! AI-64's Conquest prerequisites. Other real mode snapshots remain separate prerequisites.
use super::*;
use crate::{
    human_commander::ScreenView,
    human_commander_policy::HumanPolicy,
    match_commands::{Command, MoveOrder},
    recipient_snapshot::{DeliveryState, SelectedSeat},
};

fn scene(houses: bool, seed: u64) -> Match {
    let mut case = fixture()["cases"][0].clone();
    case["mode"] = json!("conquest");
    case["tech"] = json!(false);
    if houses {
        case["map"]["rows"][12] = json!(format!("{}B{}", ".".repeat(26), ".".repeat(21)));
    }
    let mut state = instance(&case);
    assert_eq!(state.options.mode, Mode::Conquest);
    assert_eq!(state.world.game.mode, None);
    state.options.seed = seed;
    state.start.random = crate::random::MatchRandom::new(seed);
    let visible = state.world.game.units.get_mut(&5).unwrap();
    visible.x = 35.011;
    visible.z = 20.011;
    visible.hp -= 0.1;
    state.start.weather.next = Some(crate::weather::WeatherKind::Fog);
    state.start.weather.at = 100_000;
    state
}

fn refresh(state: &mut Match, tick: i64) {
    state.grid.rebuild(&state.world.game.units);
    state.world.game.tick = tick - (tick - 1) % 4;
    vision::update_vision(&mut state.world.game, &state.rules, &state.grid);
    state.world.game.tick = tick;
}

fn add(state: &mut Match, kind: &str) {
    let id = state.next_unit_id;
    state.next_unit_id += 1;
    let kind = state.rules.kind(kind).unwrap();
    let unit = unit_spawn::initial_unit(
        &state.rules,
        Catalog::compiled().unwrap(),
        kind,
        id,
        1,
        Point { x: 88.0, z: 88.0 },
    )
    .unwrap();
    state
        .commands
        .units
        .insert(id, UnitControl::new(&unit, &state.rules).unwrap());
    state.firing.initialize(id);
    state.combat.initialize(id);
    state.abilities.initialize(id);
    state.recovery.initialize(id);
    state.world.game.units.insert(id, unit);
}

fn perturb(state: &mut Match, name: &str) {
    match name {
        "enemy_economy" => {
            let p = &mut state.start.economy.players[1];
            p.mp += 12345.67;
            p.mun += 891.2;
            p.fuel += 765.4;
            for cooldown in state.start.players[1].support.values_mut() {
                *cooldown += 97.0;
            }
        }
        "hidden_army" => {
            let u = state.world.game.units.get_mut(&6).unwrap();
            u.x = -10000.0;
            u.z = -10000.0;
            u.hp = 1.0;
            u.still = Some(1000.0);
            u.path = vec![Point { x: 90.0, z: 90.0 }];
            let c = state.commands.units.get_mut(&6).unwrap();
            c.attack_id = 999998;
            state.firing.units.get_mut(&6).unwrap().target_id = 999999;
        }
        "added_tank" => add(state, "tank"),
        "secret_depot" => add(state, "depot"),
        "visible_private" => {
            let u = state.world.game.units.get_mut(&5).unwrap();
            u.path = vec![Point { x: 90.0, z: 90.0 }];
            u.still = Some(1000.0);
            u.hold_pos = !u.hold_pos;
            let c = state.commands.units.get_mut(&5).unwrap();
            c.hold_fire = !c.hold_fire;
            c.auto_retreat = !c.auto_retreat;
            c.orders.push_back(Command::Move {
                orders: vec![MoveOrder {
                    unit: 5,
                    at: Point { x: 88.0, z: 86.0 },
                }],
                control_ids: None,
                attack_move: false,
                queue: false,
                together: false,
                face: None,
            });
            state.abilities.get_mut(5).unwrap().cooldown = 999.0;
        }
        "wire_precision" => {
            let u = state.world.game.units.get_mut(&5).unwrap();
            u.x = 35.039;
            u.z = 20.039;
            u.hp = u.hp.ceil() - 0.45;
        }
        "hidden_footprint" => {
            state.world.set_cell(42 * 48 + 40, b'R').unwrap();
        }
        "future_weather" => {
            state.start.weather.next = Some(crate::weather::WeatherKind::Clear);
            state.start.weather.at = 200_000;
        }
        "hidden_mines" => {
            for (cell, surface) in [
                (10 * 48 + 13, b'.'),
                (14 * 48 + 13, b'M'),
                (16 * 48 + 13, b'D'),
            ] {
                state.world.set_cell(cell, surface).unwrap();
                state.world.set_cell(cell, b'N').unwrap();
                state.world.mines.insert(cell, 1);
            }
        }
        _ => panic!("unknown perturbation {name}"),
    }
}

fn snapshot(state: &mut Match) -> Value {
    DeliveryState::new(state)
        .unwrap()
        .capture_selected_with_lists(
            state,
            &[SelectedSeat {
                slot: 0,
                full: true,
                omit: true,
            }],
            &[],
        )
        .unwrap()
        .unwrap()
        .recipient(0)
        .unwrap()
        .into_json()
}

// AI-64 drives the production planner on a scripted look. The human camera and
// motor schedule require fresh later deliveries and belong to separate rows.
fn commands(view: &HumanView<Observation>, level: &str, seed: u64) -> Vec<Value> {
    let screen = ScreenView {
        tick: view.tick,
        spawn: view.spawn,
        units: view.units.iter().collect(),
        visible: view.visible.clone(),
        sightings: view.sightings.iter().collect(),
        packet: &view.packet,
    };
    HumanPolicy::new(0, seed)
        .plan(screen, crate::commander_ai::Level::from_key(level).unwrap())
        .unwrap()
}

#[test]
fn ai64_conquest_hidden_perturbations_preserve_real_snapshots_and_human_policy_commands() {
    let perturbations = [
        "enemy_economy",
        "hidden_army",
        "added_tank",
        "secret_depot",
        "visible_private",
        "wire_precision",
        "hidden_footprint",
        "future_weather",
        "hidden_mines",
    ];
    let mut comparisons = 0;
    let mut emitted = 0;
    for houses in [true, false] {
        for seed in [617, 902] {
            for tick in [40, 120] {
                for level in ["easy", "normal", "hard"] {
                    let mut original = scene(houses, seed);
                    // Make the same underlying surface public in both mine variants.
                    for (cell, ch) in [(14 * 48 + 13, b'M'), (16 * 48 + 13, b'D')] {
                        original.world.set_cell(cell, ch).unwrap();
                    }
                    refresh(&mut original, tick);
                    let expected_snapshot = snapshot(&mut original);
                    assert!(
                        expected_snapshot["units"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .any(|r| r[0] == 5)
                    );
                    assert!(
                        !expected_snapshot["units"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .any(|r| r[0] == 6)
                    );
                    let view = original
                        .human_commander_observation(&mut HumanObservationMemory::new(0))
                        .unwrap();
                    let expected = commands(&view, level, seed);
                    assert!(
                        !expected.is_empty(),
                        "house={houses} seed={seed} tick={tick} {level}"
                    );
                    emitted += expected.len();
                    for name in perturbations {
                        let mut changed = scene(houses, seed);
                        for (cell, ch) in [(14 * 48 + 13, b'M'), (16 * 48 + 13, b'D')] {
                            changed.world.set_cell(cell, ch).unwrap();
                        }
                        refresh(&mut changed, tick);
                        perturb(&mut changed, name);
                        changed.grid.rebuild(&changed.world.game.units);
                        assert_eq!(
                            snapshot(&mut changed),
                            expected_snapshot,
                            "snapshot {name} house={houses}"
                        );
                        let altered = changed
                            .human_commander_observation(&mut HumanObservationMemory::new(0))
                            .unwrap();
                        assert_eq!(view, altered, "observation {name} house={houses}");
                        assert_eq!(
                            commands(&altered, level, seed),
                            expected,
                            "commands {name} {level}"
                        );
                        comparisons += 1;
                    }
                }
            }
        }
    }
    assert_eq!(comparisons, 216);
    assert!(
        emitted >= 24,
        "actual planner commands must exercise the proof"
    );
}

#[test]
fn ai64_visible_health_and_own_economy_change_real_human_commands() {
    for level in ["easy", "normal", "hard"] {
        let mut original = scene(false, 617);
        original.start.economy.players[0].mp = 1000.0;
        refresh(&mut original, 40);
        let normal = original
            .human_commander_observation(&mut HumanObservationMemory::new(0))
            .unwrap();
        let healthy = commands(&normal, level, 617);
        assert!(healthy.iter().any(|c| c["t"] == "build"), "{level}");
        original.start.economy.players[0].mp = 0.0;
        let poor = original
            .human_commander_observation(&mut HumanObservationMemory::new(0))
            .unwrap();
        assert_ne!(normal, poor);
        let unfunded = commands(&poor, level, 617);
        assert!(!unfunded.iter().any(|c| c["t"] == "build"), "{level}");
        assert_ne!(healthy, unfunded);
        original.start.economy.players[0].mp = 1000.0;
        let actor = original.world.game.units.get_mut(&2).unwrap();
        actor.hp = 10.0;
        actor.x = 40.0;
        actor.z = 40.0;
        let hurt = original
            .human_commander_observation(&mut HumanObservationMemory::new(0))
            .unwrap();
        assert_ne!(normal, hurt);
        let wounded = commands(&hurt, level, 617);
        assert_ne!(healthy, wounded, "{level}");
        assert!(
            wounded.iter().any(|c| c["t"] == "retreat"
                && c["ids"]
                    .as_array()
                    .is_some_and(|ids| ids.contains(&json!(2)))),
            "{level}: {wounded:?}"
        );
    }
}

#[test]
fn ai64_own_motion_and_context_decode_the_recipient_rows() {
    let mut state = scene(false, 617);
    let u = state.world.game.units.get_mut(&1).unwrap();
    u.world_goal = Some(Point {
        x: 42.049,
        z: 43.051,
    });
    u.path = vec![Point {
        x: 30.049,
        z: 31.051,
    }];
    for (cell, owner) in [(10 * 48 + 10, 0), (11 * 48 + 10, 1)] {
        state.world.set_cell(cell, b'N').unwrap();
        state.world.mines.insert(cell, owner);
    }
    state.world.mine_seen.insert(11 * 48 + 10, 1);
    let control = state.commands.units.get_mut(&2).unwrap();
    control.attack_id = 5;
    control.orders.push_back(Command::Move {
        orders: vec![MoveOrder {
            unit: 2,
            at: Point {
                x: 45.049,
                z: 46.051,
            },
        }],
        control_ids: None,
        attack_move: true,
        queue: false,
        together: false,
        face: None,
    });
    refresh(&mut state, 40);
    let delivered = snapshot(&mut state);
    let decoded = state
        .ordinary_ai_view_memory(0)
        .unwrap()
        .decode(&delivered)
        .unwrap();
    let view = state
        .human_commander_observation(&mut HumanObservationMemory::new(0))
        .unwrap();
    let unit = view.units.iter().find(|u| u.motion.id == 1).unwrap();
    let plan = delivered["plans"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r[0] == 1)
        .unwrap();
    assert_eq!(
        unit.motion.world_goal,
        Some(Point {
            x: plan[2].as_f64().unwrap(),
            z: plan[3].as_f64().unwrap()
        })
    );
    assert_eq!(
        view.packet
            .input()
            .units
            .iter()
            .find(|u| u.id == 1)
            .unwrap()
            .path,
        vec![Point {
            x: plan[4].as_f64().unwrap(),
            z: plan[5].as_f64().unwrap()
        }]
    );
    let context = view.packet.input().runtime.as_ref().unwrap();
    assert!(context.initial_chars.is_some());
    assert_eq!(
        context.mines,
        decoded.mines.iter().map(point).collect::<Vec<_>>()
    );
    assert_eq!(
        context.found_mines,
        decoded.found_mines.iter().map(point).collect::<Vec<_>>()
    );
    assert!(!context.mines.is_empty());
    assert!(!context.found_mines.is_empty());
    assert_eq!(
        context.units[&2].attack_target,
        decoded.units[&2]["attackId"].as_u64().unwrap() as u32
    );
    assert_eq!(
        context.units[&2].queued_amove,
        Some(point(&decoded.units[&2]["orders"][0]))
    );
}

#[test]
fn ai64_recipient_packet_and_human_commands_survive_ordinary_ticks_for_every_seat() {
    let mut state = crate::mov_audit_test_support::scene();
    state.start.economy.players[0].mp = 1000.0;
    for control in state.commands.units.values_mut() {
        control.hold_fire = true;
        control.auto_cast = false;
    }
    let seats = state.world.game.players.len();
    let mut memory = HumanObservationMemory::new(0);
    let mut delivery = DeliveryState::new(&state).unwrap();
    let view = state.human_commander_observation(&mut memory).unwrap();
    let attempts = commands(&view, "normal", 617);
    assert!(!attempts.is_empty());
    let count = attempts.len();
    for (sequence, raw) in attempts.into_iter().enumerate() {
        state
            .enqueue_command(
                crate::orders::OrderKey {
                    tick: 1,
                    issuer: 0,
                    sequence: sequence as u64,
                },
                Command::from_json(&raw).unwrap(),
            )
            .unwrap();
    }
    for index in 0..8 {
        let report = state.advance_tick().unwrap();
        if index == 0 {
            assert_eq!(report.commands.len(), count);
            assert!(
                report.commands.iter().all(|result| result.denial.is_none()),
                "{:?}",
                report.commands
            );
        }
        state.human_commander_observation(&mut memory).unwrap();
        let frame = delivery
            .capture_with_lists(&mut state, &vec![true; seats], &vec![false; seats], &[])
            .unwrap();
        for slot in 0..seats {
            assert!(
                frame.recipient(slot).unwrap().into_json()["tick"]
                    .as_u64()
                    .is_some()
            );
        }
    }
    assert!(state.tick_state.fault().is_none());
}
