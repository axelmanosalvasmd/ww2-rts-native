//! Full original Source steps compared with the sole composed Match runner.
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

#[test]
fn actual_ground_support_ticks_match_source_effects_and_lifetimes() {
    let fixture = common::fixture();
    let mut traces = 0;
    let mut ticks = 0;
    let mut checkpoints = 0;
    let mut smoke_traces = 0;
    for trace in fixture["traces"].as_array().unwrap() {
        let name = trace["name"].as_str().unwrap();
        let mut m = common::make(trace);
        let expected_mode = match trace["setup"]["nativeMode"].as_str().unwrap() {
            "Default" => ww2_core::economy::Mode::Default,
            "Conquest" => ww2_core::economy::Mode::Conquest,
            _ => panic!("unsupported source fixture mode"),
        };
        assert_eq!(m.options.mode, expected_mode);
        assert!(m.world.game.mode.is_none());
        let smoke_query = &trace["initial"]["raw"]["smokeLos"];
        let is_smoke = trace["spec"]["kind"] == "smoke" && trace["denial"].is_null();
        let mut arrival_x = None;
        let mut drifted = false;
        if is_smoke {
            assert!(m.world.game.smokes.is_empty());
            common::compare_smoke_los(&m, &trace["initial"], name);
            assert_eq!(smoke_query["withSmoke"], true);
        }
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
            assert!(report.environment().next().is_none() && report.retired.is_empty());
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
            let events = common::events(&report.events);
            assert!(m.support.events.is_empty() && m.aviation.events.is_empty());
            if is_smoke {
                let sight = common::smoke_los(&m, smoke_query);
                assert_eq!(sight["withoutSmoke"], true, "{name}:{tick}: clear control");
                let clouds = &m.world.game.smokes;
                if let Some(first) = clouds.first() {
                    if let Some(x) = arrival_x {
                        drifted |= first.x > x;
                    } else {
                        assert_eq!(clouds.len(), 5, "{name}:{tick}: delivered cloud count");
                        assert!(clouds.iter().all(|s| s.r.to_bits() == 7f64.to_bits()));
                        assert_eq!(
                            sight["withSmoke"], false,
                            "{name}:{tick}: actual barrage LOS"
                        );
                        arrival_x = Some(first.x);
                    }
                } else {
                    assert_eq!(sight["withSmoke"], true, "{name}:{tick}: no-smoke LOS");
                }
            }
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
        if is_smoke {
            assert!(arrival_x.is_some(), "{name}: no actual smoke delivery");
            if trace["spec"]["rain"].is_null() {
                assert!(
                    drifted,
                    "{name}: actual delivered cloud did not drift downwind"
                );
            }
            assert!(
                m.world.game.smokes.is_empty(),
                "{name}: smoke did not retire"
            );
            assert_eq!(common::smoke_los(&m, smoke_query)["withSmoke"], true);
            smoke_traces += 1;
        }
        traces += 1;
    }
    assert_eq!((traces, ticks, checkpoints), (13, 2705, 153));
    assert_eq!(smoke_traces, 2);
}
