//! Actual ordinary caller replays of the five authored legacy-trigger maps.
#[path = "common/aviation.rs"]
#[allow(dead_code)]
mod comparisons;
use comparisons::assert_json;
use flate2::read::GzDecoder;
use serde_json::{Value, json};
use std::io::Read;
use ww2_core::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    map::AuthoritativeMap,
    match_commands::{Command, Denial},
    match_runtime::{Match, MatchOptions, PlayerConfig},
    match_tick::{TickEvent, TickPathConfig, TickPhase},
    orders::OrderKey,
    weather::WeatherChoice,
};

fn fixture() -> Value {
    let mut text = String::new();
    GzDecoder::new(include_bytes!("fixtures/legacy-scenario-runtime.json.gz").as_slice())
        .read_to_string(&mut text)
        .unwrap();
    serde_json::from_str(&text).unwrap()
}

fn make(trace: &Value) -> Match {
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&trace["map"]).unwrap()).unwrap();
    let mut state = Match::new(
        &map,
        MatchOptions {
            seed: 701,
            mode: Mode::Conquest,
            players: vec![
                PlayerConfig {
                    name: "A".into(),
                    team: 0,
                    faction: Faction::Usa,
                },
                PlayerConfig {
                    name: "B".into(),
                    team: 1,
                    faction: Faction::Germany,
                },
            ],
            shuffle: false,
            army: ArmySize::Standard,
            weather: WeatherChoice::Map,
            weather_seed: Some(1),
            living_weather: false,
            map_key: Some(trace["name"].as_str().unwrap().into()),
            tech: false,
            supply: false,
            logistics: false,
            sandbox: false,
        },
    )
    .unwrap();
    // Both Source and native replay receive these declared pre-step environmental
    // inputs. They are not constructor results or later expected state. The
    // canonical Match random streams are never replaced.
    state.start.environment_seed = trace["inputs"]["environmentSeed"].as_i64().unwrap() as u32;
    state.start.wind = (
        trace["inputs"]["wind"]["a"].as_f64().unwrap(),
        trace["inputs"]["wind"]["v"].as_f64().unwrap(),
    );
    state
        .initialize_tick_scene(0, TickPathConfig::default())
        .unwrap();
    state
}

fn shots(state: &Match, events: &[TickEvent]) -> Value {
    json!(
        events
            .iter()
            .map(|event| match event {
                TickEvent::Scenario(say) => say.shot_json(),
                TickEvent::Environment(effect) => effect.to_json(),
                TickEvent::Firing(event) => event.to_json(&state.rules),
                TickEvent::Support(_) | TickEvent::Aviation(_) => {
                    panic!("five-map fixture has no active support or aircraft producer")
                }
            })
            .collect::<Vec<_>>()
    )
}

fn sections(state: &Match) -> Value {
    json!(state.world.sections.iter().map(|(&cell, meta)| {
        let health = &state.world.game.sections[&(cell as u32)];
        let mut row = json!({"id":meta.id,"c":cell,"structureId":meta.structure_id,
            "hp":health.hp,"maxHp":health.max_hp.unwrap(),
            "state":state.world.cell_row(cell).unwrap().data.section.unwrap().state,
            "material":meta.material,"anchor":meta.anchor,"supports":meta.supports,"rubble":meta.rubble});
        let fields = row.as_object_mut().unwrap();
        if let Some(at) = meta.failure.failed_at { fields.insert("failedAt".into(), json!(at)); }
        if let Some(direction) = meta.failure.direction {
            fields.insert("direction".into(), json!({"x":direction.x,"z":direction.z}));
        }
        if let Some(impulse) = meta.failure.impulse { fields.insert("impulse".into(), json!(impulse)); }
        if let Some(id) = state.world.structures.iter().find(|s| s.id == meta.structure_id).unwrap().entity_id {
            fields.insert("entityId".into(), json!(id));
        }
        row
    }).collect::<Vec<_>>())
}

fn support_timers(state: &Match) -> Value {
    json!(
        state
            .start
            .players
            .iter()
            .enumerate()
            .map(|(slot, p)| json!({"slot":slot,"support":p.support}))
            .collect::<Vec<_>>()
    )
}

#[test]
fn original_five_maps_run_early_research_and_ordered_legacy_scenario() {
    let fixture = fixture();
    assert_eq!(
        fixture["reference"],
        "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
    );
    assert_eq!(fixture["callerTraces"].as_array().unwrap().len(), 5);
    let mut checkpoints = 0;
    for trace in fixture["callerTraces"].as_array().unwrap() {
        let name = trace["name"].as_str().unwrap();
        let mut state = make(trace);
        let frames = trace["steps"].as_array().unwrap();
        let last = frames.last().unwrap()["tick"].as_u64().unwrap();
        let mut selected = 0;
        for tick in 1..=last {
            let before_sections =
                (name == "the-great-bridge" && tick == 8400).then(|| sections(&state));
            let report = state
                .advance_tick()
                .unwrap_or_else(|error| panic!("{name}:{tick}: {error}"));
            let phase = |p| report.phases.iter().position(|value| *value == p).unwrap();
            assert!(phase(TickPhase::Research) < phase(TickPhase::FieldProjects));
            assert!(phase(TickPhase::SupportTimers) < phase(TickPhase::Scenario));
            assert!(phase(TickPhase::Scenario) < phase(TickPhase::LivingWeather));
            assert_eq!(
                report
                    .phases
                    .iter()
                    .filter(|p| **p == TickPhase::Research)
                    .count(),
                1
            );
            assert_eq!(
                report
                    .phases
                    .iter()
                    .filter(|p| **p == TickPhase::Scenario)
                    .count(),
                1
            );
            assert!(!report.unavailable.contains(&TickPhase::Research));
            assert!(!report.unavailable.contains(&TickPhase::Scenario));
            assert!(report.unavailable.contains(&TickPhase::Production));
            if frames[selected]["tick"].as_u64().unwrap() != tick {
                continue;
            }
            let expected = &frames[selected];
            assert_json(
                &shots(&state, &report.events),
                &expected["shots"],
                &format!("{name}:{tick}: complete chronological batch"),
            );
            assert_json(
                &state.scenario.as_ref().unwrap().state_json(),
                &expected["scenario"],
                &format!("{name}:{tick}: accumulated trigger state"),
            );
            assert_json(
                &support_timers(&state),
                &expected["phase"]["afterStep"]["supportTimers"],
                &format!("{name}:{tick}: canonical cooldowns"),
            );
            assert_json(
                &json!({"a":state.start.wind.0,"v":state.start.wind.1}),
                &expected["phase"]["afterStep"]["wind"],
                &format!("{name}:{tick}: later weather"),
            );
            assert_json(
                &json!(state.world.game.flags),
                &expected["recipientInputs"]["terrain"]["flags"],
                &format!("{name}:{tick}: actual terrain flags"),
            );
            if let Some(before) = before_sections {
                assert_json(
                    &before,
                    &expected["structureSectionsBefore"],
                    &format!("{name}: sections before the real tick8400"),
                );
                assert_json(
                    &sections(&state),
                    &expected["structureSectionsAfter"],
                    &format!("{name}: sections after the real tick8400"),
                );
                let bridge: Vec<_> = state
                    .world
                    .sections
                    .iter()
                    .filter(|(_, meta)| meta.structure_id == "map:bridge:5573")
                    .collect();
                assert_eq!(bridge.len(), 84);
                assert!(bridge.iter().all(|(cell, _)| {
                    let s = &state.world.game.sections[&(**cell as u32)];
                    s.failed && s.hp == 0.0
                }));
                assert_eq!(report.events.len(), 85);
                assert!(matches!(report.events[0], TickEvent::Scenario(_)));
                assert!(report.events[1..].iter().all(|event| matches!(
                    event,
                    TickEvent::Environment(ww2_core::environment::Effect::Collapse(_))
                )));
            }
            selected += 1;
            checkpoints += 1;
        }
        assert_eq!(selected, frames.len());
    }
    assert_eq!(checkpoints, 21);
}

#[test]
fn scenario_winner_gate_keeps_existing_state_and_does_not_claim_execution() {
    let fixture = fixture();
    let trace = &fixture["callerTraces"][0];
    let mut state = make(trace);
    for _ in 0..59 {
        state.advance_tick().unwrap();
    }
    let before = state.scenario.as_ref().unwrap().state().clone();
    state.start.economy.players[0].vp = state.start.win_vp;
    assert_eq!(state.apply_conquest_victory().unwrap().unwrap().winner, 0);
    assert!(state.world.game.reveal);
    let report = state.advance_tick().unwrap();
    assert!(!report.phases.contains(&TickPhase::Scenario));
    assert!(
        !report
            .events
            .iter()
            .any(|event| matches!(event, TickEvent::Scenario(_)))
    );
    assert_eq!(state.scenario.as_ref().unwrap().state(), &before);
    assert!(report.phases.contains(&TickPhase::Research));
    assert!(!report.unavailable.contains(&TickPhase::Scenario));
}

#[test]
fn committed_scenario_prefix_survives_later_objective_refusal_and_faults_once() {
    let fixture = fixture();
    let trace = fixture["callerTraces"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "the-great-bridge")
        .unwrap();
    let mut state = make(trace);
    for _ in 0..8399 {
        state.advance_tick().unwrap();
    }
    state.start.points[0].progress = f64::NAN;
    let failure = state
        .advance_tick()
        .expect_err("late objective validation must refuse the malformed progress");
    assert_eq!(failure.tick, 8400);
    assert!(
        failure.reason.contains("point capture:"),
        "{}",
        failure.reason
    );
    let expected = trace["steps"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["tick"] == 8400)
        .unwrap();
    assert_json(
        &shots(&state, &failure.events),
        &expected["shots"],
        "failure retains complete real scenario output",
    );
    assert_json(
        &sections(&state),
        &expected["structureSectionsAfter"],
        "failure retains committed bridge sections",
    );
    assert_json(
        &state.scenario.as_ref().unwrap().state_json(),
        &expected["scenario"],
        "failure retains accumulated scenario state",
    );
    assert_eq!(state.tick_state.fault(), Some(failure.reason.as_str()));
    let retained = shots(&state, &failure.events);
    let scenario = state.scenario.as_ref().unwrap().state().clone();
    let refused = state
        .advance_tick()
        .expect_err("faulted runner cannot emit a second copy");
    assert!(refused.events.is_empty());
    assert_eq!(shots(&state, &failure.events), retained);
    assert_eq!(state.scenario.as_ref().unwrap().state(), &scenario);
}

#[test]
fn malformed_legacy_owner_refuses_before_clock_or_input_publication() {
    let fixture = fixture();
    for restored in [false, true] {
        let mut state = make(&fixture["callerTraces"][0]);
        let accepted = state.scenario.as_ref().unwrap().state().clone();
        let owner = state.start.points[0].owner;
        let key = OrderKey {
            tick: 1,
            issuer: 0,
            sequence: 1,
        };
        state.enqueue_command(key, Command::Unsupported).unwrap();
        if restored {
            let mut malformed = accepted.clone();
            let point = malformed.owners.keys().next().unwrap().clone();
            malformed.captured.insert(format!("{point}:99"));
            state
                .scenario
                .as_mut()
                .unwrap()
                .restore_state(malformed)
                .unwrap();
        } else {
            state.start.points[0].owner = Some(state.world.game.players.len());
        }
        let pending = state.commands.pending_commands();
        let boundary = state.commands.next_tick();
        let random = state.start.random.snapshot();
        let flags = state.world.game.flags.clone();
        let failure = state
            .advance_tick()
            .expect_err("foreign capture owner must refuse before clock");
        assert!(failure.reason.contains("legacy scenario point state"));
        assert_eq!(failure.tick, 0);
        assert!(failure.events.is_empty());
        assert_eq!(failure.retirement, Default::default());
        assert!(state.tick_state.fault().is_none());
        assert_eq!(state.commands.pending_commands(), pending);
        assert_eq!(state.commands.next_tick(), boundary);
        assert_eq!(state.start.random.snapshot(), random);
        assert_eq!(state.world.game.flags, flags);
        state
            .scenario
            .as_mut()
            .unwrap()
            .restore_state(accepted)
            .unwrap();
        state.start.points[0].owner = owner;
        let report = state.advance_tick().unwrap();
        assert_eq!(report.tick, 1);
        assert_eq!(report.commands.len(), 1);
        assert_eq!(report.commands[0].key, key);
        assert_eq!(report.commands[0].denial, Some(Denial::Unsupported));
        assert_eq!(state.commands.pending_commands(), 0);
    }
}
