use super::*;
use crate::{game::Point, match_support::SupportDenial, mov_audit_test_support as scene};

fn setup() -> (Match, u32) {
    let mut m = scene::scene();
    let enemy = scene::spawn(&mut m, "mg", 1, scene::point(70.0, 50.0));
    m.recovery.initialize(enemy);
    m.world.game.players[0].visible = vec![enemy];
    m.start.economy.players[0].mp = 1000.0;
    for (kind, cd) in &mut m.start.players[0].support {
        *cd = if kind == "artillery" { 0.0 } else { 100.0 };
    }
    (m, enemy)
}
fn screen(view: &HumanView<SupportPacket>) -> ScreenView<'_, SupportPacket> {
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
    // Age the authored observation scene through empty canonical input boundaries.
    while m.world.game.tick < tick {
        let next = m.world.game.tick + 1;
        assert!(m.apply_commands(next as u64, false).unwrap().is_empty());
        m.world.game.tick = next;
    }
    m.world.game.vision_tick = (tick > 0).then(|| tick - (tick - 1) % 4);
}

#[test]
fn support_uses_delivered_stationary_history_and_real_admission_receipts() {
    let (mut m, enemy) = setup();
    let mut memory = HumanObservationMemory::new(0);
    let mut policy = SupportPolicy::new(0);
    let first = m.human_support_observation(&mut memory).unwrap();
    assert!(
        policy
            .plan(screen(&first), Level::Normal)
            .unwrap()
            .is_empty()
    );
    // A private stationary timer cannot make a first delivered sighting old.
    m.world.game.units.get_mut(&enemy).unwrap().still = Some(9999.0);
    clock(&mut m, 62);
    let fresh = m
        .human_support_observation(&mut HumanObservationMemory::new(0))
        .unwrap();
    let mut fresh_policy = policy.clone();
    assert!(
        fresh_policy
            .plan(screen(&fresh), Level::Normal)
            .unwrap()
            .is_empty()
    );
    let view = m.human_support_observation(&mut memory).unwrap();
    assert_eq!(view.packet.facts.stationary_since[&enemy], 0.0);
    let random = m.start.random.snapshot();
    let commands = policy.plan(screen(&view), Level::Normal).unwrap();
    assert_eq!(
        commands,
        vec![json!({"t":"support","kind":"artillery","x":70.0,"z":50.0})]
    );
    assert_eq!(m.start.random.snapshot(), random);
    let before = m.start.economy.players[0].mp;
    let command = Command::from_json(&commands[0]).unwrap();
    assert_eq!(m.execute_command(0, &command, false).unwrap(), None);
    assert_eq!(m.start.economy.players[0].mp, before - 150.0);
    assert_eq!(m.world.game.strikes.len(), 1);
    policy.accepted_call(view.tick).unwrap();
    assert_eq!(policy.last_called, 3.1);
    assert_eq!(
        m.admit_support(0, "artillery", Point { x: 70.0, z: 50.0 }, None)
            .unwrap(),
        Some(SupportDenial::Cooldown)
    );
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
fn movement_loss_of_sight_and_opponent_wallets_do_not_forge_old_targets() {
    let (mut m, enemy) = setup();
    let mut memory = HumanObservationMemory::new(0);
    let first = m.human_support_observation(&mut memory).unwrap();
    m.start.economy.players[1].mp += 1234.0;
    for cd in m.start.players[1].support.values_mut() {
        *cd = 42.0;
    }
    let same = m
        .human_support_observation(&mut HumanObservationMemory::new(0))
        .unwrap();
    assert_eq!(
        first, same,
        "opponent economy is outside the detached packet"
    );
    clock(&mut m, 62);
    m.world.game.units.get_mut(&enemy).unwrap().x += 1.0;
    let moved = m.human_support_observation(&mut memory).unwrap();
    assert_eq!(moved.packet.facts.stationary_since[&enemy], 3.1);
    m.world.game.players[0].visible.clear();
    clock(&mut m, 64);
    let hidden = m.human_support_observation(&mut memory).unwrap();
    assert!(!hidden.packet.facts.stationary_since.contains_key(&enemy));
    assert!(
        hidden
            .packet
            .observation
            .input()
            .units
            .iter()
            .all(|u| u.id != enemy)
    );
    m.world.game.players[0].visible = vec![enemy];
    clock(&mut m, 100);
    let rediscovered = m.human_support_observation(&mut memory).unwrap();
    assert_eq!(rediscovered.packet.facts.stationary_since[&enemy], 5.0);
}

#[test]
fn support_attempts_do_not_update_the_accepted_call_clock() {
    let (mut m, _) = setup();
    let mut memory = HumanObservationMemory::new(0);
    let mut policy = SupportPolicy::new(0);
    let first = m.human_support_observation(&mut memory).unwrap();
    policy.plan(screen(&first), Level::Easy).unwrap();
    clock(&mut m, 62);
    let view = m.human_support_observation(&mut memory).unwrap();
    assert_eq!(policy.plan(screen(&view), Level::Easy).unwrap().len(), 1);
    assert_eq!(policy.last_called, -1e9);
    // Motor admission can be newer than the retained strategic packet.
    policy.accepted_call(70).unwrap();
    assert!(policy.plan(screen(&view), Level::Easy).unwrap().is_empty());
    assert!(
        policy
            .plan(screen(&view), Level::Normal)
            .unwrap()
            .is_empty()
    );
    let before = policy.clone();
    assert!(policy.accepted_call(60).is_err());
    assert_eq!(policy, before);
}

#[test]
fn support_motor_receipts_repeat_and_enter_canonical_admission() {
    let run = || {
        let (mut m, _) = setup();
        let mut view = m
            .human_support_observation(&mut HumanObservationMemory::new(0))
            .unwrap();
        let mut state = HumanState::new(7301, 0, Point { x: 50.0, z: 50.0 });
        let mut policy = SupportPolicy::new(0);
        let hurts = OwnerHurtBatch {
            generation: 0,
            through_ordinal: 0,
            events: Vec::new(),
        };
        let mut attempts = Vec::new();
        for now in 0..1000 {
            view.tick = now;
            let mut input = view.packet.observation.input().clone();
            input.tick = now as i64;
            view.packet.observation = Observation::new(input).unwrap();
            let result = advance_support(
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
                assert_eq!(raw["t"], "support");
                assert_eq!(raw["kind"], "artillery");
                assert_eq!(
                    m.execute_command(0, &Command::from_json(raw).unwrap(), false)
                        .unwrap(),
                    None
                );
                policy.accepted_call(now).unwrap();
                attempts.push(receipt);
                break;
            }
        }
        assert_eq!(attempts.len(), 1);
        assert_eq!(m.world.game.strikes.len(), 1);
        attempts
    };
    assert_eq!(run(), run());
}

#[test]
fn preferred_dive_and_artillery_fallback_survive_admission_and_recipient_lifecycles() {
    for (dive_ready, artillery_ready, expected) in [
        (true, true, Some("dive")),
        (true, false, Some("dive")),
        (false, true, Some("artillery")),
        (false, false, None),
    ] {
        let (mut m, enemy) = setup();
        let heavy = scene::spawn(&mut m, "tank", 1, scene::point(72.0, 50.0));
        m.recovery.initialize(heavy);
        m.world.game.players[0].visible = vec![enemy, heavy];
        m.start.economy.players[0].mun = 1000.0;
        for (kind, cd) in &mut m.start.players[0].support {
            *cd = if (kind == "dive" && dive_ready) || (kind == "artillery" && artillery_ready) {
                0.0
            } else {
                100.0
            };
        }
        let mut memory = HumanObservationMemory::new(0);
        let mut policy = SupportPolicy::new(0);
        let first = m.human_support_observation(&mut memory).unwrap();
        assert!(
            policy
                .plan(screen(&first), Level::Normal)
                .unwrap()
                .is_empty()
        );
        clock(&mut m, 62);
        let view = m.human_support_observation(&mut memory).unwrap();
        // Dive takes the watched heavy before artillery; cooldown permits fallback.
        assert_eq!(
            view.packet.facts.cooldowns["dive"],
            if dive_ready { 0.0 } else { 100.0 }
        );
        assert!(
            view.packet
                .observation
                .input()
                .units
                .iter()
                .any(|u| u.id == heavy && u.kind == "tank")
        );
        let commands = policy.plan(screen(&view), Level::Normal).unwrap();
        assert_eq!(commands.len(), usize::from(expected.is_some()));
        let before = m.start.economy.players[0].mp;
        for raw in commands {
            let kind = expected.unwrap();
            assert_eq!(raw["kind"], kind);
            if kind == "dive" {
                assert_eq!(
                    (raw["x"].as_f64(), raw["z"].as_f64()),
                    (Some(72.0), Some(50.0))
                );
            }
            assert_eq!(
                m.execute_command(0, &Command::from_json(&raw).unwrap(), false)
                    .unwrap(),
                None
            );
            policy.accepted_call(view.tick).unwrap();
        }
        let cost = expected.map_or(0.0, |kind| {
            crate::economy::sup_cost(crate::economy::Mode::Conquest, kind)
                .unwrap()
                .cost
        });
        assert_eq!(m.start.economy.players[0].mp, before - cost);
        assert_eq!(m.world.game.strikes.len(), usize::from(expected.is_some()));
        for tick in 0..1260 {
            m.advance_tick().unwrap();
            let cache = crate::recipient_snapshot::SnapshotCache::new(&m).unwrap();
            for seat in 0..m.start.players.len() {
                cache.recipient(seat).unwrap();
            }
            assert!(m.tick_state.fault().is_none());
            if tick >= 7 && m.world.game.strikes.is_empty() {
                break;
            }
        }
        assert!(m.world.game.strikes.is_empty());
        for _ in 0..4 {
            m.advance_tick().unwrap();
            let cache = crate::recipient_snapshot::SnapshotCache::new(&m).unwrap();
            for seat in 0..m.start.players.len() {
                cache.recipient(seat).unwrap();
            }
            assert!(m.tick_state.fault().is_none());
        }
    }
}
