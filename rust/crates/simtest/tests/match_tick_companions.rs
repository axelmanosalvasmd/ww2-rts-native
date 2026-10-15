//! Actual original steps through the canonical operative tick consumer.
//! The authored cases stop grenade flights before contact and leave unsupported
//! ordinary body geometry, scenarios and other unavailable phases explicit.
use crate::shared::companion_tick as source;

use serde_json::json;
use ww2_core::{
    companions::RESPAWN_TICKS, economy::Mode, match_commands::Command, match_runtime::Match,
    match_tick::TickPhase, orders::OrderKey, random::StreamId,
};

#[test]
fn original_full_steps_consume_operative_turns_and_lifecycle() {
    let f = source::fixture();
    assert_eq!(f["schema"], 1);
    assert_eq!(f["reference"], "e4e579a838aabae3b1d9dadbc17a0a87147bc52d");
    let mut operations = 0;
    let mut ticks = 0;
    let mut combat_draws = 0;
    assert_eq!(f["traces"].as_array().unwrap().len(), 15);
    for trace in f["traces"].as_array().unwrap() {
        let name = trace["name"].as_str().unwrap();
        let mut expected = trace["initial"].clone();
        let mut m = source::make(&f, &source::decoded(&expected), Mode::Conquest);
        let mut projection = source::Projection::new();
        projection.compare(&mut m, &source::decoded(&expected), name);
        for (index, operation) in trace["operations"].as_array().unwrap().iter().enumerate() {
            combat_draws += operation["rng"]["raw"].as_array().unwrap().len();
            let context = format!("{name}/{index}/{}", operation["op"]);
            // Predict from a clone of the actual stream. The real tick consumes
            // its own draws, and the subsequent projection checks its cursor.
            let mut predictor = m.start.random.clone();
            for value in source::decoded(&operation["rng"]).as_array().unwrap() {
                assert_eq!(
                    predictor.next_fraction(StreamId::Combat).to_bits(),
                    value.as_f64().unwrap().to_bits(),
                    "{context}: recorded Combat draw"
                );
            }
            let arguments = source::arguments(operation);
            source::apply_delta(&mut expected, &operation["delta"]);
            let winner = m.finish.is_some();
            match arguments["op"].as_str().unwrap() {
                "join" => {
                    let key = arguments["key"].as_str().unwrap();
                    m.join_companion(key, arguments["owner"].as_f64().unwrap() as usize, winner)
                        .unwrap_or_else(|error| panic!("{context}: join: {error}"));
                    let companion = m.companions.get(key).unwrap();
                    source::exact(
                        &json!({"key":companion.key,"unitId":companion.unit_id}),
                        &source::decoded(&operation["result"]),
                        &context,
                    );
                }
                "input" => assert_eq!(
                    m.companion_input(
                        arguments["key"].as_str().unwrap(),
                        &arguments["input"],
                        winner,
                    ),
                    operation["result"]["raw"].as_bool().unwrap(),
                    "{context}: input admission"
                ),
                "patch_unit" => source::patch_unit(
                    &mut m,
                    arguments["id"].as_f64().unwrap() as u32,
                    &arguments["fields"],
                ),
                "patch_player" => source::patch_player(
                    &mut m,
                    arguments["owner"].as_f64().unwrap() as usize,
                    &arguments["fields"],
                ),
                "patch_game" => source::patch_game(&mut m, &arguments["fields"]),
                "step" => {
                    let report = m
                        .advance_tick()
                        .unwrap_or_else(|error| panic!("{context}: tick: {error}"));
                    assert_eq!(report.tick, m.world.game.tick, "{context}: clock");
                    assert!(report.phases.contains(&TickPhase::Operative));
                    assert!(!report.unavailable.contains(&TickPhase::Operative));
                    // The report continues to disclose the unrelated missing phases.
                    assert!(!report.unavailable.is_empty());
                    projection.record_events(&m, report.events);
                    ticks += 1;
                }
                operation => panic!("{context}: unhandled source operation {operation}"),
            }
            projection.compare(&mut m, &source::decoded(&expected), &context);
            operations += 1;
        }
    }
    assert_eq!(operations, 576);
    assert_eq!(ticks, 506);
    assert_eq!(combat_draws, 10);
}

fn admission_state(m: &Match) -> String {
    // All authored admission failures contain finite numbers. Debug retains
    // dormant state and queue-independent sidecars; numeric bits cover the live
    // body, wallet and clock facts the rejected tick might otherwise change.
    let numeric = m
        .world
        .game
        .units
        .values()
        .map(|unit| {
            (
                unit.id,
                [
                    unit.x,
                    unit.z,
                    unit.rot,
                    unit.hp,
                    unit.supp,
                    unit.move_speed,
                ]
                .map(f64::to_bits),
            )
        })
        .collect::<Vec<_>>();
    format!(
        "{:?}",
        (
            (
                &m.world,
                &m.commands.units,
                &m.companions,
                &m.combat,
                &m.firing,
                &m.abilities,
                &m.recovery,
            ),
            (
                &m.environment,
                &m.start.economy,
                &m.start.players,
                &m.start.points,
                &m.story,
                &m.timeline,
                &m.finish,
                &m.tick_state,
            ),
            (
                m.next_unit_id,
                m.start.random.snapshot(),
                m.start.environment_seed,
                m.start.wind.0.to_bits(),
                m.start.wind.1.to_bits(),
                numeric,
            ),
        )
    )
}

#[test]
fn malformed_operative_identity_rejects_before_clock_or_respawn_publication() {
    let f = source::fixture();
    let initial = source::decoded(&f["traces"][0]["initial"]);
    for failure in [
        "zero identity",
        "future identity",
        "map key mismatch",
        "missing body",
        "unbacked historical control",
    ] {
        let mut m = source::make(&f, &initial, Mode::Conquest);
        m.join_companion("guard", 0, false).unwrap();
        let companion = m.companions.get("guard").unwrap().id;
        let id = m.companions.records[&companion].unit_id;
        match failure {
            "zero identity" => {
                let mut unit = m.world.game.units.remove(&id).unwrap();
                unit.id = 0;
                m.world.game.units.insert(0, unit);
                let control = m.commands.units.remove(&id).unwrap();
                m.commands.units.insert(0, control);
                let body = m.companions.soldiers.remove(&id).unwrap();
                m.companions.soldiers.insert(0, body);
                m.companions.records.get_mut(&companion).unwrap().unit_id = 0;
                m.firing.remove(id);
                m.combat.remove(id);
                m.abilities.remove(id);
                m.recovery.remove(id);
                m.firing.initialize(0);
                m.combat.initialize(0);
                m.abilities.initialize(0);
                m.recovery.initialize(0);
            }
            "future identity" => m.next_unit_id = id,
            "map key mismatch" => m.world.game.units.get_mut(&id).unwrap().id = id + 1,
            "missing body" => {
                m.companions.soldiers.remove(&id);
            }
            "unbacked historical control" => {
                // Constructor ID 1 was removed by the authored actor reset.
                // It is historical, so only backing metadata can reject it.
                assert!(!m.world.game.units.contains_key(&1));
                assert!(1 < m.next_unit_id);
                assert!(m.firing.get(1).is_none() && m.combat.get(1).is_none());
                m.commands.units.insert(1, m.commands.units[&id].clone());
            }
            _ => unreachable!(),
        }
        let before = admission_state(&m);
        let refused = m.advance_tick().unwrap_err();
        assert!(
            refused.events.is_empty(),
            "{failure}: admission emits no events"
        );
        assert_eq!(refused.tick, m.world.game.tick, "{failure}: refusal clock");
        assert_eq!(admission_state(&m), before, "{failure}: untouched state");
        assert!(
            m.tick_state.fault().is_none(),
            "{failure}: preflight does not poison"
        );
    }
}

#[test]
fn retired_operative_metadata_is_collected_before_later_ticks() {
    let f = source::fixture();
    let initial = source::decoded(&f["traces"][0]["initial"]);
    let mut m = source::make(&f, &initial, Mode::Conquest);
    m.join_companion("history", 0, false).unwrap();
    let companion = m.companions.get("history").unwrap().id;
    let id = m.companions.records[&companion].unit_id;
    let future = OrderKey {
        tick: 5,
        issuer: 0,
        sequence: 29,
    };
    m.enqueue_command(future, Command::Unsupported).unwrap();
    assert!(m.firing.flights.is_empty());
    m.world.game.units.get_mut(&id).unwrap().hp = 0.0;
    let report = m.advance_tick().unwrap();
    assert!(report.retired.iter().any(|retired| retired.unit.id == id));
    assert!(!m.world.game.units.contains_key(&id));
    assert!(!m.companions.soldiers.contains_key(&id));
    assert_eq!(m.companions.records[&companion].unit_id, 0);
    assert_eq!(
        m.companions.records[&companion].respawn_at,
        report.tick + RESPAWN_TICKS
    );
    assert!(m.commands.units[&id].operative);
    assert!(m.firing.get(id).is_some());
    assert!(m.combat.get(id).is_some());
    // Collection follows the next flight phase. It must release the three
    // retained owners together while leaving the companion's respawn due.
    assert_eq!(m.advance_tick().unwrap().tick, 2);
    assert!(!m.commands.units.contains_key(&id));
    assert!(m.firing.get(id).is_none());
    assert!(m.combat.get(id).is_none());
    assert_eq!(m.companions.records[&companion].unit_id, 0);
    assert_eq!(
        m.companions.records[&companion].respawn_at,
        report.tick + RESPAWN_TICKS
    );
    // This was the first subsequent admission failure when Combat alone was
    // collected. The global input queue must also survive per-unit cleanup.
    assert_eq!(m.advance_tick().unwrap().tick, 3);
    assert!(m.tick_state.fault().is_none());
    assert_eq!(m.advance_tick().unwrap().tick, 4);
    let boundary = m.advance_tick().unwrap();
    assert_eq!(boundary.tick, 5);
    assert_eq!(boundary.commands.len(), 1);
    assert_eq!(boundary.commands[0].key, future);
}

#[test]
fn rejected_operative_admission_retains_the_recorded_boundary_order() {
    let f = source::fixture();
    let initial = source::decoded(&f["traces"][0]["initial"]);
    let mut m = source::make(&f, &initial, Mode::Conquest);
    m.join_companion("queued", 0, false).unwrap();
    let id = m.companions.get("queued").unwrap().unit_id;
    let order = OrderKey {
        tick: 1,
        issuer: 0,
        sequence: 19,
    };
    m.enqueue_command(order, Command::Unsupported).unwrap();
    let body = m.companions.soldiers.remove(&id).unwrap();
    let before = admission_state(&m);
    let refused = m.advance_tick().unwrap_err();
    assert!(refused.events.is_empty(), "admission emits no events");
    assert_eq!(refused.tick, m.world.game.tick, "refusal clock");
    assert_eq!(admission_state(&m), before);
    m.companions.soldiers.insert(id, body);
    let report = m.advance_tick().unwrap();
    assert_eq!(report.tick, 1);
    assert_eq!(report.commands.len(), 1);
    assert_eq!(report.commands[0].key, order);
}
