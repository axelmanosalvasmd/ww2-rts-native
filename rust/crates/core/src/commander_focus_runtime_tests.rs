use super::*;
use crate::{
    commander_attack_policy::{BaseAttackFacts, base_attack_plan},
    economy::Mode,
    mov_audit_test_support as scene,
    weather::WeatherKind,
};

fn setup() -> (Match, [u32; 2], [u32; 2]) {
    let mut m = scene::scene();
    let own = [
        scene::spawn(&mut m, "rifle", 0, scene::point(50.0, 50.0)),
        scene::spawn(&mut m, "rifle", 0, scene::point(52.0, 50.0)),
    ];
    let enemy = [
        scene::spawn(&mut m, "rifle", 1, scene::point(60.0, 50.0)),
        scene::spawn(&mut m, "rifle", 1, scene::point(62.0, 50.0)),
    ];
    m.world.game.units.get_mut(&enemy[1]).unwrap().hp = 20.0;
    m.world.game.players[0].visible = enemy.to_vec();
    let onward = Command::Move {
        orders: own
            .iter()
            .map(|id| MoveOrder {
                unit: i64::from(*id),
                at: scene::point(100.0, 50.0),
            })
            .collect(),
        control_ids: None,
        attack_move: true,
        queue: false,
        together: false,
        face: None,
    };
    assert_eq!(m.execute_command(0, &onward, false).unwrap(), None);
    for id in own {
        m.firing.get_mut(id).unwrap().target_id = enemy[0];
    }
    (m, own, enemy)
}
fn screen(view: &HumanView<FocusPacket>) -> ScreenView<'_, FocusPacket> {
    ScreenView {
        tick: view.tick,
        spawn: view.spawn,
        units: view.units.iter().collect(),
        visible: view.visible.clone(),
        sightings: view.sightings.iter().collect(),
        packet: &view.packet,
    }
}
fn clock(m: &mut Match, tick: i64) {
    m.world.game.tick = tick;
    m.world.game.vision_tick = (tick > 0).then(|| tick - (tick - 1) % 4);
}
fn view_clock(view: &mut HumanView<FocusPacket>, tick: u64) {
    view.tick = tick;
    let mut input = view.packet.observation.input().clone();
    input.tick = tick as i64;
    view.packet.observation = Observation::new(input).unwrap();
}
fn admit(m: &mut Match, commands: &[Value]) {
    for raw in commands {
        assert_eq!(
            m.execute_command(0, &Command::from_json(raw).unwrap(), false)
                .unwrap(),
            None
        );
    }
}
fn initial_focus(
    m: &mut Match,
    memory: &mut HumanObservationMemory,
    policy: &mut FocusPolicy,
) -> Vec<Value> {
    let first = m.human_focus_observation(memory).unwrap();
    assert!(policy.plan(screen(&first), Level::Hard).unwrap().is_empty());
    clock(m, 10);
    let view = m.human_focus_observation(memory).unwrap();
    policy.plan(screen(&view), Level::Hard).unwrap()
}

#[test]
fn focus_projection_contains_own_orders_and_only_received_enemy_sight_lines() {
    let (mut m, own, enemy) = setup();
    let onward: BTreeMap<_, _> = own
        .iter()
        .map(|id| (*id, m.world.game.units[id].amove.unwrap()))
        .collect();
    assert_eq!(
        m.execute_command(
            0,
            &Command::Move {
                orders: own
                    .iter()
                    .map(|id| MoveOrder {
                        unit: i64::from(*id),
                        at: scene::point(100.04, 50.04)
                    })
                    .collect(),
                control_ids: None,
                attack_move: true,
                queue: true,
                together: false,
                face: None,
            },
            false
        )
        .unwrap(),
        None
    );
    let hidden = scene::spawn(&mut m, "rifle", 1, scene::point(110.0, 110.0));
    let first = m
        .human_focus_observation(&mut HumanObservationMemory::new(0))
        .unwrap();
    assert!(
        own.iter()
            .all(|id| first.packet.facts.owned[id].queued_amove == Some(scene::point(100.0, 50.0)))
    );
    assert!(
        first
            .packet
            .facts
            .owned
            .keys()
            .all(|id| m.world.game.units[id].owner == 0)
    );
    assert!(
        own.iter()
            .all(|id| first.packet.facts.owned[id].amove == Some(onward[id]))
    );
    assert!(own.iter().all(|id| {
        enemy
            .iter()
            .all(|target| first.packet.facts.clear_lines.contains(&(*id, *target)))
    }));
    assert!(
        first
            .packet
            .observation
            .input()
            .units
            .iter()
            .all(|u| u.id != hidden)
    );
    assert!(
        first
            .packet
            .facts
            .clear_lines
            .iter()
            .all(|(_, target)| enemy.contains(target))
    );
    m.commands.units.get_mut(&enemy[0]).unwrap().attack_id = hidden;
    m.commands
        .units
        .get_mut(&enemy[1])
        .unwrap()
        .orders
        .push_back(Command::Move {
            orders: vec![MoveOrder {
                unit: i64::from(enemy[1]),
                at: scene::point(100.0, 100.0),
            }],
            control_ids: None,
            attack_move: true,
            queue: true,
            together: false,
            face: None,
        });
    m.world.game.units.get_mut(&hidden).unwrap().x += 5.0;
    m.firing.get_mut(hidden).unwrap().target_id = own[0];
    let same = m
        .human_focus_observation(&mut HumanObservationMemory::new(0))
        .unwrap();
    assert_eq!(
        same, first,
        "foreign private orders and hidden actors cannot change focus facts"
    );
}

#[test]
fn focus_notice_then_attack_and_queued_onward_use_actual_command_admission() {
    let (mut m, own, enemy) = setup();
    let onward: BTreeMap<_, _> = own
        .iter()
        .map(|id| (*id, m.world.game.units[id].amove.unwrap()))
        .collect();
    let mut memory = HumanObservationMemory::new(0);
    let mut policy = FocusPolicy::new(0);
    let commands = initial_focus(&mut m, &mut memory, &mut policy);
    assert_eq!(commands.len(), 2);
    assert_eq!(commands[0]["t"], "attack");
    assert_eq!(commands[0]["target"], enemy[1]);
    assert_eq!(commands[1]["t"], "amove");
    assert_eq!(commands[1]["queue"], true);
    admit(&mut m, &commands);
    for id in own {
        let control = &m.commands.units[&id];
        assert_eq!(control.attack_id, enemy[1]);
        assert!(
            control
                .orders
                .iter()
                .any(|c| matches!(c, Command::Move {attack_move:true,orders,..}
            if orders.iter().any(|o| o.unit == i64::from(id) && o.at == onward[&id])))
        );
    }
    for _ in 0..4 {
        m.advance_tick().unwrap();
        let cache = crate::recipient_snapshot::SnapshotCache::new(&m).unwrap();
        for slot in 0..2 {
            cache.recipient(slot).unwrap();
        }
        assert!(m.tick_state.fault().is_none());
    }
}

#[test]
fn focus_retreat_range_and_hidden_drop_restore_queued_onward_without_target_coordinates() {
    for cause in ["retreat", "range", "hidden"] {
        let (mut m, own, enemy) = setup();
        let onward: BTreeMap<_, _> = own
            .iter()
            .map(|id| (*id, m.world.game.units[id].amove.unwrap()))
            .collect();
        let mut memory = HumanObservationMemory::new(0);
        let mut policy = FocusPolicy::new(0);
        let commands = initial_focus(&mut m, &mut memory, &mut policy);
        admit(&mut m, &commands);
        match cause {
            "retreat" => m.world.game.units.get_mut(&enemy[1]).unwrap().retreating = true,
            "range" => m.world.game.units.get_mut(&enemy[1]).unwrap().x = 100.0,
            _ => {
                m.world.game.players[0].visible = vec![enemy[0]];
                m.world.game.units.get_mut(&enemy[1]).unwrap().x = 112.0;
            }
        }
        clock(&mut m, 12);
        let view = m.human_focus_observation(&mut memory).unwrap();
        let drops = policy.plan(screen(&view), Level::Hard).unwrap();
        assert_eq!(drops.len(), 1, "{cause}");
        assert_eq!(drops[0]["t"], "amove", "{cause}");
        assert_eq!(drops[0]["queue"], false, "{cause}");
        assert!(
            drops[0]["orders"].as_array().unwrap().iter().all(|o| {
                let at = onward[&(o[0].as_u64().unwrap() as u32)];
                o[1] == at.x && o[2] == at.z
            }),
            "{cause}: orders {drops:?}, original onward {onward:?}"
        );
        admit(&mut m, &drops);
        for id in own {
            assert_eq!(m.commands.units[&id].attack_id, 0);
        }
    }
}

#[test]
fn hidden_focus_does_not_override_a_later_manual_stop() {
    let (mut m, own, enemy) = setup();
    let mut memory = HumanObservationMemory::new(0);
    let mut policy = FocusPolicy::new(0);
    let commands = initial_focus(&mut m, &mut memory, &mut policy);
    admit(&mut m, &commands);
    assert_eq!(
        m.execute_command(
            0,
            &Command::Stop {
                ids: own.iter().map(|id| i64::from(*id)).collect()
            },
            false
        )
        .unwrap(),
        None
    );
    m.world.game.players[0].visible.clear();
    m.world.game.units.get_mut(&enemy[1]).unwrap().x = 112.0;
    clock(&mut m, 12);
    let view = m.human_focus_observation(&mut memory).unwrap();
    assert!(policy.plan(screen(&view), Level::Hard).unwrap().is_empty());
    assert!(policy.focus.targets.is_empty());
    for id in own {
        assert!(m.world.game.units[&id].amove.is_none());
    }
}

#[test]
fn camera_filtered_contacts_never_enter_focus_scores_or_notice_history() {
    let (mut m, _, enemy) = setup();
    let mut view = m
        .human_focus_observation(&mut HumanObservationMemory::new(0))
        .unwrap();
    let mut policy = FocusPolicy::new(0);
    policy.plan(screen(&view), Level::Hard).unwrap();
    view_clock(&mut view, 10);
    let mut filtered = screen(&view);
    filtered.units.retain(|u| u.motion.id != enemy[1]);
    filtered.visible.retain(|id| *id != enemy[1]);
    filtered.sightings.retain(|s| s.id != enemy[1]);
    assert!(policy.plan(filtered, Level::Hard).unwrap().is_empty());
    assert_eq!(policy.noticed[&enemy[1]].1, 0);
}

#[test]
fn camera_filtered_manual_attack_stays_under_player_control() {
    let (mut m, own, enemy) = setup();
    assert_eq!(
        m.execute_command(
            0,
            &Command::Attack {
                ids: own.iter().map(|id| i64::from(*id)).collect(),
                target: i64::from(enemy[1]),
                queue: false,
                at: None,
            },
            false
        )
        .unwrap(),
        None
    );
    let view = m
        .human_focus_observation(&mut HumanObservationMemory::new(0))
        .unwrap();
    let mut filtered = screen(&view);
    filtered.units.retain(|u| !enemy.contains(&u.motion.id));
    filtered.visible.clear();
    filtered.sightings.clear();
    let mut policy = FocusPolicy::new(0);
    assert!(policy.plan(filtered, Level::Hard).unwrap().is_empty());
    for id in own {
        assert_eq!(m.commands.units[&id].attack_id, enemy[1]);
    }
}

#[test]
fn focus_motor_schedule_repeats_and_invalid_advances_publish_no_state() {
    let run = || {
        let (mut m, _, enemy) = setup();
        let mut view = m
            .human_focus_observation(&mut HumanObservationMemory::new(0))
            .unwrap();
        let mut state = HumanState::new(7301, 0, scene::point(50.0, 50.0));
        let mut policy = FocusPolicy::new(0);
        let hurts = OwnerHurtBatch {
            generation: 0,
            through_ordinal: 0,
            events: vec![],
        };
        let mut attempts = Vec::new();
        for now in 0..1000 {
            view_clock(&mut view, now);
            let result = advance_focus(
                &mut state,
                &mut policy,
                TickControl {
                    now,
                    live_owner_out: false,
                    winner: false,
                    level: "hard",
                },
                Some(&view),
                &hurts,
            )
            .unwrap();
            if let Some(receipt) = result.input
                && let Some(raw) = &receipt.command
            {
                assert!(now > receipt.origin);
                admit(&mut m, std::slice::from_ref(raw));
                attempts.push(receipt);
                if attempts.len() == 2 {
                    break;
                }
            }
        }
        assert_eq!(attempts.len(), 2);
        assert_eq!(attempts[0].command.as_ref().unwrap()["target"], enemy[1]);
        assert_eq!(attempts[1].command.as_ref().unwrap()["queue"], true);
        let before_state = state.clone();
        let before_policy = policy.clone();
        view.units[0].hp = f64::NAN;
        assert!(
            advance_focus(
                &mut state,
                &mut policy,
                TickControl {
                    now: 1001,
                    live_owner_out: false,
                    winner: false,
                    level: "hard"
                },
                Some(&view),
                &hurts
            )
            .is_err()
        );
        assert_eq!(state, before_state);
        assert_eq!(policy, before_policy);
        attempts
    };
    assert_eq!(run(), run());
}

#[test]
fn detached_classic_base_intent_composes_with_ordinary_move_admission() {
    let mut m = scene::scene();
    for x in 0..6 {
        scene::spawn(&mut m, "rifle", 0, scene::point(50.0 + f64::from(x), 50.0));
    }
    let view = m
        .human_focus_observation(&mut HumanObservationMemory::new(0))
        .unwrap();
    let facts = BaseAttackFacts {
        mode: Mode::Classic,
        weather: WeatherKind::Clear,
        push: true,
        known_buildings: vec![],
        recent: vec![],
        unavailable: BTreeSet::new(),
    };
    let commands = base_attack_plan(view.packet.observation(), &facts, Level::Normal).unwrap();
    assert_eq!(commands.len(), 1);
    let ids: Vec<_> = match &commands[0] {
        Command::Move { orders, .. } => orders.iter().map(|o| o.unit as u32).collect(),
        _ => panic!("base planner must emit move"),
    };
    assert_eq!(ids.len(), 6);
    assert_eq!(
        m.options.mode,
        Mode::Conquest,
        "this test admits a move without opening Classic admission"
    );
    for command in &commands {
        assert_eq!(m.execute_command(0, command, false).unwrap(), None);
    }
    assert!(ids.iter().all(|id| m.world.game.units[id].amove.is_some()));
}
