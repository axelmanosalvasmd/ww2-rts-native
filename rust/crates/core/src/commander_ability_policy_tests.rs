use super::*;
use crate::{game::Point, match_commands::Denial, mov_audit_test_support as scene};

fn setup() -> (Match, u32, u32) {
    let mut m = scene::scene();
    let own = scene::spawn(&mut m, "mg", 0, scene::point(50.0, 50.0));
    let enemy = scene::spawn(&mut m, "rifle", 1, scene::point(58.0, 50.0));
    m.recovery.initialize(own);
    m.recovery.initialize(enemy);
    m.world.game.players[0].visible = vec![enemy];
    m.firing.get_mut(own).unwrap().target_id = enemy;
    for (id, ability) in m
        .abilities
        .iter()
        .map(|(id, a)| (id, *a))
        .collect::<Vec<_>>()
    {
        if id != own && m.world.game.units[&id].owner == 0 {
            m.abilities.get_mut(id).unwrap().cooldown = ability.cooldown + 100.0;
        }
    }
    (m, own, enemy)
}

fn screen(view: &HumanView<AbilityPacket>) -> ScreenView<'_, AbilityPacket> {
    ScreenView {
        tick: view.tick,
        spawn: view.spawn,
        units: view.units.iter().collect(),
        visible: view.visible.clone(),
        sightings: view.sightings.iter().collect(),
        packet: &view.packet,
    }
}

fn at_tick(view: &mut HumanView<AbilityPacket>, tick: u64) {
    view.tick = tick;
    let mut input = view.packet.observation.input().clone();
    input.tick = tick as i64;
    view.packet.observation = Observation::new(input).unwrap();
}

#[test]
fn ability_projection_reads_only_owned_cooldowns_and_completed_own_bases() {
    let (mut m, own, enemy) = setup();
    m.abilities.get_mut(own).unwrap().cooldown = -0.2;
    let view = m
        .human_ability_observation(&mut HumanObservationMemory::new(0))
        .unwrap();
    assert_eq!(view.packet.cooldowns[&own], 0.0);
    assert!(!view.packet.cooldowns.contains_key(&enemy));
    assert!(
        view.packet
            .at_base
            .iter()
            .all(|id| m.world.game.units[id].owner == 0)
    );
    m.abilities.get_mut(enemy).unwrap().cooldown = 12345.0;
    let other = m
        .human_ability_observation(&mut HumanObservationMemory::new(0))
        .unwrap();
    assert_eq!(
        other, view,
        "enemy private ability state cannot change the packet"
    );
    // Recipient rows reject invalid canonical numbers before owner masking.
    m.abilities.get_mut(enemy).unwrap().cooldown = f64::NAN;
    let mut invalid_memory = HumanObservationMemory::new(0);
    let invalid_before = invalid_memory.clone();
    assert!(m.human_ability_observation(&mut invalid_memory).is_err());
    assert_eq!(invalid_memory, invalid_before);
    m.abilities.get_mut(enemy).unwrap().cooldown = 12345.0;
    m.abilities.get_mut(own).unwrap().cooldown = f64::NAN;
    let mut memory = HumanObservationMemory::new(0);
    let before = memory.clone();
    assert!(m.human_ability_observation(&mut memory).is_err());
    assert_eq!(memory, before);
}

#[test]
fn abilities_wait_for_observed_notice_then_use_canonical_admission() {
    for (level, delay) in [(Level::Easy, 50), (Level::Normal, 25), (Level::Hard, 10)] {
        let (mut m, own, _) = setup();
        let mut view = m
            .human_ability_observation(&mut HumanObservationMemory::new(0))
            .unwrap();
        let mut policy = AbilityPolicy::new(0);
        assert!(policy.plan(screen(&view), level).unwrap().is_empty());
        at_tick(&mut view, delay - 1);
        assert!(policy.plan(screen(&view), level).unwrap().is_empty());
        at_tick(&mut view, delay);
        let commands = policy.plan(screen(&view), level).unwrap();
        assert_eq!(commands, vec![json!({"t":"ability","ids":[own]})]);
        let command = Command::from_json(&commands[0]).unwrap();
        assert_eq!(m.execute_command(0, &command, false).unwrap(), None);
        assert!(m.abilities.get(own).unwrap().cooldown > 0.0);
        assert!(m.abilities.get(own).unwrap().buff > 0.0);
        assert_eq!(
            m.execute_command(0, &command, false).unwrap(),
            Some(Denial::Cooldown)
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
}

#[test]
fn human_schedule_delays_ability_attempts_and_repeats_from_the_same_seed() {
    let run = || {
        let (mut m, own, enemy) = setup();
        let mut view = m
            .human_ability_observation(&mut HumanObservationMemory::new(0))
            .unwrap();
        let mut state = HumanState::new(7301, 0, Point { x: 50.0, z: 50.0 });
        let mut policy = AbilityPolicy::new(0);
        let hurts = OwnerHurtBatch {
            generation: 0,
            through_ordinal: 0,
            events: Vec::new(),
        };
        let mut attempts = Vec::new();
        for now in 0..500 {
            at_tick(&mut view, now);
            let result = advance_abilities(
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
                && let Some(command) = &receipt.command
            {
                assert!(
                    now > receipt.origin,
                    "motor input must follow its planning look"
                );
                let command = Command::from_json(command).unwrap();
                assert_eq!(m.execute_command(0, &command, false).unwrap(), None);
                assert!(m.abilities.get(own).unwrap().buff > 0.0);
                attempts.push(receipt);
                break;
            }
        }
        assert_eq!(
            attempts.len(),
            1,
            "visible watched target {enemy} must produce an ability"
        );
        attempts
    };
    assert_eq!(run(), run());
}

#[test]
fn filtered_targets_clock_errors_and_invalid_frames_publish_no_policy_state() {
    let (mut m, own, enemy) = setup();
    let mut view = m
        .human_ability_observation(&mut HumanObservationMemory::new(0))
        .unwrap();
    let mut policy = AbilityPolicy::new(0);
    policy.plan(screen(&view), Level::Normal).unwrap();
    at_tick(&mut view, 30);
    let mut filtered = screen(&view);
    filtered.units.retain(|u| u.motion.id != enemy);
    filtered.visible.clear();
    assert!(policy.plan(filtered, Level::Normal).unwrap().is_empty());
    let mut state = HumanState::new(7301, 0, scene::point(50.0, 50.0));
    let before_state = state.clone();
    let before_policy = policy.clone();
    // HumanState validates the strategic row before either provider is published.
    view.units
        .iter_mut()
        .find(|u| u.motion.id == own)
        .unwrap()
        .hp = f64::NAN;
    let hurts = OwnerHurtBatch {
        generation: 0,
        through_ordinal: 0,
        events: Vec::new(),
    };
    assert!(
        advance_abilities(
            &mut state,
            &mut policy,
            TickControl {
                now: 30,
                live_owner_out: false,
                winner: false,
                level: "normal",
            },
            Some(&view),
            &hurts
        )
        .is_err()
    );
    assert_eq!(state, before_state);
    assert_eq!(policy, before_policy);
}
