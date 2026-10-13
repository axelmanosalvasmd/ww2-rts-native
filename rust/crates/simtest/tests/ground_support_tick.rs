//! Full original Source steps compared with the sole composed Match runner.
#![recursion_limit = "256"]
#[path = "common/ground_support_tick.rs"]
mod common;

use serde_json::Value;
use std::collections::BTreeMap;
use ww2_core::{match_commands::Command, match_tick::TickPhase, orders::OrderKey};

fn quiet_state(m: &ww2_core::match_runtime::Match) -> Value {
    serde_json::json!({
        "units":m.world.game.units.values().map(|u|serde_json::json!([u.id,u.x,u.z,u.rot,u.hp])).collect::<Vec<_>>(),
        "nextId":m.next_unit_id,
        "story":m.story.iter().map(|s|serde_json::json!([s.kills,s.losses,s.built,s.captures,s.mp_spent,s.support_calls,s.planes_downed])).collect::<Vec<_>>(),
        "random":m.start.random.snapshot().streams.iter().map(|(k,s)|format!("{k:?}:{s:016x}")).collect::<Vec<_>>()
    })
}

fn detached_units(
    m: &ww2_core::match_runtime::Match,
) -> (BTreeMap<u32, ww2_core::game::Unit>, Vec<(u32, u64)>) {
    let mut units = m.world.game.units.clone();
    let pace = units
        .values_mut()
        .map(|unit| {
            let bits = unit.pace.to_bits();
            // Normalize only the detached comparison value. The live absent-pace
            // sentinel remains unchanged and is checked independently by bits.
            unit.pace = 0.0;
            (unit.id, bits)
        })
        .collect();
    (units, pace)
}

#[test]
fn actual_ground_support_ticks_match_source_effects_and_lifetimes() {
    let fixture = common::fixture();
    let mut traces = 0;
    let mut ticks = 0;
    let mut checkpoints = 0;
    for trace in fixture["traces"].as_array().unwrap() {
        if trace["setup"]["nativeMode"] == "Default" {
            continue;
        }
        let name = trace["name"].as_str().unwrap();
        let mut m = common::make(trace);
        let initial_rng = m.start.random.snapshot();
        let frames: BTreeMap<_, _> = trace["frames"]
            .as_array()
            .unwrap()
            .iter()
            .map(|frame| (frame["tick"].as_u64().unwrap(), &frame["expected"]))
            .collect();
        let command = Command::from_json(&trace["request"]).unwrap();
        m.enqueue_command(
            OrderKey {
                tick: 1,
                issuer: 0,
                sequence: 1,
            },
            command,
        )
        .unwrap();
        let mut signature = common::signature(&m);
        let mut quiet = quiet_state(&m);
        for tick in 1..=trace["spec"]["ticks"].as_u64().unwrap() {
            assert!(
                m.support.events.is_empty(),
                "{name}:{tick}: prior observation not drained"
            );
            let before: Vec<_> = m.world.game.units.keys().copied().collect();
            let report = m
                .advance_tick()
                .unwrap_or_else(|error| panic!("{name}:{tick}: {error}"));
            assert_eq!(report.tick, tick as i64);
            assert_eq!(
                report.prior.keys().copied().collect::<Vec<_>>(),
                before,
                "{name}:{tick}: prior membership"
            );
            // These scenes contain one held rifle, no in-turn unit births, and no
            // projectile retirement. Smoke IDs never enter captured unit membership.
            assert_eq!(
                report.unit_order, before,
                "{name}:{tick}: captured late membership"
            );
            assert_eq!(report.unit_order, [1]);
            assert_eq!(m.grid.owned_by(&m.world.game.units, 0), Vec::<u32>::new());
            assert_eq!(m.grid.owned_by(&m.world.game.units, 1), [1]);
            assert!(report.environment.is_empty() && report.retired.is_empty());
            assert!(
                m.firing.events.is_empty(),
                "{name}:{tick}: unexpected non-support effect"
            );
            let phase = |kind| report.phases.iter().position(|p| *p == kind).unwrap();
            assert!(phase(TickPhase::Projectiles) < phase(TickPhase::Support));
            assert!(phase(TickPhase::Support) < phase(TickPhase::SupportTimers));
            assert!(phase(TickPhase::SupportTimers) < phase(TickPhase::LivingWeather));
            for kind in [TickPhase::Support, TickPhase::SupportTimers] {
                assert_eq!(report.phases.iter().filter(|p| **p == kind).count(), 1);
                assert!(!report.unavailable.contains(&kind));
            }
            if tick == 1 {
                assert_eq!(report.commands.len(), 1);
                assert_eq!(
                    report.commands[0].key,
                    OrderKey {
                        tick: 1,
                        issuer: 0,
                        sequence: 1
                    }
                );
                assert_eq!(
                    report.commands[0].denial.map(|d| d.name()),
                    trace["denial"].as_str()
                );
            } else {
                assert!(report.commands.is_empty());
            }
            let events = common::events(&mut m);
            let next_signature = common::signature(&m);
            let next_quiet = quiet_state(&m);
            if let Some(expected) = frames.get(&tick) {
                common::compare(
                    &m,
                    expected,
                    &events,
                    &initial_rng,
                    &format!("{name}:{tick}"),
                );
                checkpoints += 1;
            } else {
                assert_eq!(
                    events,
                    serde_json::json!([]),
                    "{name}:{tick}: source-silent tick emitted effects"
                );
                assert_eq!(
                    next_signature, signature,
                    "{name}:{tick}: unsampled lifetime/membership transition"
                );
                assert_eq!(
                    next_quiet, quiet,
                    "{name}:{tick}: unexpected quiet-tick health/pose/ID/story/random mutation"
                );
            }
            signature = next_signature;
            quiet = next_quiet;
            ticks += 1;
        }
        assert_eq!(m.commands.pending_commands(), 0);
        assert!(m.tick_state.fault().is_none());
        traces += 1;
    }
    assert_eq!((traces, ticks, checkpoints), (12, 2660, 144));
}

#[test]
fn default_support_case_remains_a_read_only_runner_refusal() {
    let fixture = common::fixture();
    let trace = fixture["traces"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["setup"]["nativeMode"] == "Default")
        .unwrap();
    let mut m = common::make(trace);
    m.enqueue_command(
        OrderKey {
            tick: 1,
            issuer: 0,
            sequence: 1,
        },
        Command::from_json(&trace["request"]).unwrap(),
    )
    .unwrap();
    let before = (
        detached_units(&m),
        m.start.economy.players.clone(),
        m.story.clone(),
        m.start.players[0].support.clone(),
        m.support.clone(),
        m.next_unit_id,
        m.start.random.snapshot(),
        m.commands.next_tick(),
        m.commands.pending_commands(),
    );
    assert!(m.advance_tick().is_err());
    assert_eq!(m.world.game.tick, 0);
    assert_eq!(
        before,
        (
            detached_units(&m),
            m.start.economy.players.clone(),
            m.story.clone(),
            m.start.players[0].support.clone(),
            m.support.clone(),
            m.next_unit_id,
            m.start.random.snapshot(),
            m.commands.next_tick(),
            m.commands.pending_commands()
        )
    );
    assert!(m.tick_state.fault().is_none());
    // The lower-level Default support component has its own original-source
    // activation/timer evidence. Broader tick providers still require Conquest.
    m.validate_ground_support_tick(1).unwrap();
}
