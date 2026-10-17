//! Native whole-match replay uses the same hosted schedule and recipient policy as rooms.
use super::*;
use crate::room_runtime::{FrameKind, RoomRuntime};
use serde_json::Value;
use ww2_core::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    human_commander_state::InputReceipt,
    match_commands::CommandResult,
    match_runtime::{MatchOptions, PlayerConfig},
    recipient_snapshot::SelectedSeat,
    weather::WeatherChoice,
};

#[derive(Debug, PartialEq)]
struct Replay {
    inputs: Vec<(usize, InputReceipt)>,
    accepted: Vec<Option<bool>>,
    commands: Vec<(OrderKey, Command)>,
    admissions: Vec<CommandResult>,
    brains: BTreeMap<usize, SeatCommander>,
    state: Value,
    finished: bool,
    supplied_births: BTreeSet<u32>,
    frames: usize,
    outcome: Value,
    startup: Value,
}

fn room(mode: Mode, level: Level, enabled: bool, seed: u64) -> RoomRuntime {
    room_with_weather(mode, level, enabled, seed, false)
}

fn room_with_weather(
    mode: Mode,
    level: Level,
    enabled: bool,
    seed: u64,
    living_weather: bool,
) -> RoomRuntime {
    // The default map has no Horde defender spawns and would fall back to Conquest.
    let map_key = if mode == Mode::Horde {
        "bocage"
    } else {
        "default"
    };
    let map = crate::authoritative_maps::load(map_key).unwrap();
    let options = MatchOptions {
        seed,
        mode,
        players: [Faction::Usa, Faction::Germany, Faction::Ussr]
            .into_iter()
            .enumerate()
            .map(|(slot, faction)| PlayerConfig {
                name: format!("AI {slot}"),
                team: slot as i32,
                faction,
            })
            .collect(),
        shuffle: true,
        army: ArmySize::Standard,
        weather: WeatherChoice::Map,
        weather_seed: None,
        living_weather,
        map_key: Some(map_key.into()),
        tech: enabled,
        supply: false,
        logistics: enabled,
        sandbox: false,
    };
    let state = match mode {
        Mode::Conquest => Match::new(map, options),
        Mode::Classic => Match::new_classic(map, options),
        Mode::Assault => Match::new_assault(map, options, 0),
        Mode::Annihilation => Match::new_annihilation(map, options),
        Mode::Horde => Match::new_horde(map, options),
        _ => unreachable!(),
    }
    .unwrap();
    assert_eq!(
        state.options.mode, mode,
        "test must exercise the requested mode"
    );
    assert_eq!(state.horde.is_some(), mode == Mode::Horde);
    assert!(!state.world.game.reveal, "AI must use ordinary fog");
    let mut runtime = RoomRuntime::new(state, 0).unwrap();
    runtime
        .configure_ai(&[(0, level), (1, level), (2, level)])
        .unwrap();
    runtime
}

fn play(mode: Mode, level: Level, enabled: bool, seed: u64, ticks: i64) -> Replay {
    play_room(room(mode, level, enabled, seed), enabled, ticks)
}

fn play_room(mut runtime: RoomRuntime, enabled: bool, ticks: i64) -> Replay {
    let mode = runtime.state().options.mode;
    let first_birth = runtime.state().next_unit_id;
    let seats = runtime.state().world.game.players.len();
    let selected = (0..seats)
        .map(|slot| SelectedSeat {
            slot,
            full: true,
            omit: false,
        })
        .collect::<Vec<_>>();
    let mut replay = Replay {
        inputs: Vec::new(),
        accepted: Vec::new(),
        commands: Vec::new(),
        admissions: Vec::new(),
        brains: BTreeMap::new(),
        state: Value::Null,
        finished: false,
        supplied_births: BTreeSet::new(),
        frames: 0,
        outcome: Value::Null,
        startup: startup(&runtime),
    };
    let mut held_at_three_minutes = None;
    while runtime.state().world.game.tick < ticks && runtime.state().finish.is_none() {
        let report = runtime.advance_tick(&vec![false; seats]).unwrap();
        if runtime.state().options.logistics {
            for unit in runtime.state().world.game.units.values() {
                let kind = &runtime.state().rules.kinds[unit.kind];
                if unit.id >= first_birth && kind.infantry && !kind.structure {
                    assert!(
                        runtime.state().logistics.get(unit.id).is_some(),
                        "new ground squad {} has no physical reserves",
                        unit.id
                    );
                    replay.supplied_births.insert(unit.id);
                }
            }
        }
        if report.tick == 3_600 {
            held_at_three_minutes = Some(
                runtime
                    .state()
                    .start
                    .points
                    .iter()
                    .filter(|p| p.owner.is_some())
                    .count(),
            );
        }
        replay.admissions.extend(report.commands);
        let sent = runtime.should_snapshot(report.tick);
        runtime.think(sent).unwrap();
        replay.commands.extend(
            runtime
                .state()
                .commands
                .pending_tick(report.tick as u64 + 1)
                .map(|(key, command)| (*key, command.clone())),
        );
        if sent {
            let frames = runtime
                .collect_frame(&selected, FrameKind::Scheduled)
                .unwrap();
            assert_eq!(frames.len(), seats);
            for frame in frames {
                assert_eq!(frame.message["t"], "s");
                assert_eq!(frame.message["tick"], report.tick);
                if mode == Mode::Classic {
                    let nodes = frame.message["nodes"].as_array().unwrap();
                    assert!(!nodes.is_empty(), "Classic resource nodes were omitted");
                    assert!(
                        nodes
                            .iter()
                            .all(|node| node.as_array().is_some_and(|row| row.len() == 4)
                                && matches!(node[3].as_u64(), Some(0 | 1)))
                    );
                }
                replay.frames += 1;
            }
        }
        assert!(runtime.fault().is_none());
        assert!(runtime.state().tick_state.fault().is_none());
    }
    replay.outcome = outcome(runtime.state(), held_at_three_minutes);
    // Resolve every final emitted command through the ordinary next-tick admission.
    // This drain tick emits no new policy inputs.
    if runtime.state().finish.is_none() {
        let report = runtime.advance_tick(&vec![false; seats]).unwrap();
        replay.admissions.extend(report.commands);
    }
    if runtime.state().finish.is_some() {
        for _ in 0..8 {
            let report = runtime.advance_tick(&vec![false; seats]).unwrap();
            runtime.think(runtime.should_snapshot(report.tick)).unwrap();
            let frames = runtime
                .collect_frame(&selected, FrameKind::Scheduled)
                .unwrap();
            assert_eq!(frames.len(), seats);
            assert!(runtime.fault().is_none());
            replay.frames += frames.len();
        }
    }
    let final_frames = runtime
        .collect_frame(&selected, FrameKind::Scheduled)
        .unwrap();
    assert_eq!(final_frames.len(), seats);
    replay.frames += final_frames.len();
    assert!(runtime.fault().is_none());
    replay.inputs = std::mem::take(&mut runtime.ai.as_mut().unwrap().inputs);
    replay.brains = runtime.ai.as_ref().unwrap().commanders.clone();
    replay.accepted = replay
        .inputs
        .iter()
        .map(|(slot, input)| {
            input.command.as_ref().map(|raw| {
                if Command::from_json(raw).is_err() {
                    return false;
                }
                let result = replay
                    .admissions
                    .iter()
                    .find(|result| {
                        result.key.issuer as usize == *slot && result.key.tick == input.tick + 1
                    })
                    .expect("every typed human input must reach ordinary admission");
                result.denial.is_none()
            })
        })
        .collect();
    replay.finished = runtime.state().finish.is_some();
    // The save owner preserves float bits, non-string keys and normalized storage
    // bindings. Ordinary serde_json would lose NaNs and reject structured RNG keys.
    replay.state = runtime.state().save_json().unwrap();
    assert!(replay.frames >= seats);
    if enabled
        && matches!(mode, Mode::Conquest | Mode::Classic | Mode::Annihilation)
        && ticks >= 3_600
    {
        assert!(
            !replay.supplied_births.is_empty(),
            "logistics replay never recruited or trained a squad"
        );
    }
    if mode == Mode::Horde && ticks >= 3_600 {
        let horde = runtime.state().horde.as_ref().unwrap().slot;
        assert!(
            replay
                .commands
                .iter()
                .any(|(key, _)| key.issuer as usize == horde),
            "real Horde director never issued an order"
        );
        assert!(
            replay
                .admissions
                .iter()
                .any(|result| result.key.issuer as usize == horde && result.denial.is_none()),
            "Horde orders never reached admission"
        );
    }
    for slot in 0..3 {
        let mine = replay
            .inputs
            .iter()
            .filter(|(owner, _)| *owner == slot)
            .map(|(_, input)| input)
            .collect::<Vec<_>>();
        assert!(!mine.is_empty(), "seat {slot} never made an input");
        assert!(mine.windows(2).all(|pair| pair[0].tick < pair[1].tick));
        assert!(mine.iter().any(|input| input.command.is_some()));
        assert!(
            replay
                .admissions
                .iter()
                .any(|result| { result.key.issuer as usize == slot && result.denial.is_none() })
        );
    }
    replay
}

fn replay_matrix(mode: Mode) {
    // Prototype shared/ai-schedule.js:52-72 and test-ai-human.js:13-28:
    // repeat creation, stepping, delivery and all human inputs from one recorded seed.
    // PARITY.md permits independent native streams; this compares native runs only.
    for enabled in [false, true] {
        for level in [Level::Easy, Level::Normal, Level::Hard] {
            let first = play(mode, level, enabled, 7, 3_600);
            let second = play(mode, level, enabled, 7, 3_600);
            assert_eq!(first, second, "{mode:?}, {level:?}, enabled={enabled}");
        }
    }
}

#[test]
fn intent_ai65_conquest_seeded_inputs_replay_all_levels_and_option_profiles() {
    replay_matrix(Mode::Conquest);
}

#[test]
fn intent_ai65_classic_seeded_inputs_replay_all_levels_and_option_profiles() {
    replay_matrix(Mode::Classic);
}

#[test]
fn intent_ai65_assault_seeded_inputs_replay_all_levels_and_option_profiles() {
    replay_matrix(Mode::Assault);
}

#[test]
fn intent_ai65_annihilation_seeded_inputs_replay_all_levels_and_option_profiles() {
    replay_matrix(Mode::Annihilation);
}

#[test]
fn intent_ai65_horde_seeded_inputs_replay_all_levels_and_option_profiles() {
    replay_matrix(Mode::Horde);
}

#[test]
fn intent_ai65_another_seed_changes_commander_inputs_and_canonical_match() {
    let first = play(Mode::Conquest, Level::Normal, true, 7, 600);
    let second = play(Mode::Conquest, Level::Normal, true, 8, 600);
    assert_ne!(first.inputs, second.inputs);
    assert_ne!(first.state, second.state);
}

#[test]
fn intent_ai65_seeded_aircraft_room_keeps_inputs_and_every_recipient_replayable() {
    let create = || {
        let mut runtime = room(Mode::Conquest, Level::Normal, true, 7);
        for slot in 0..3 {
            runtime.state_mut().spawn_aircraft(slot, "fighter").unwrap();
        }
        // Seed the recipient views after the declared scene, as playMatch setup does.
        runtime
            .configure_ai(&[(0, Level::Normal), (1, Level::Normal), (2, Level::Normal)])
            .unwrap();
        runtime
    };
    let first = play_room(create(), true, 600);
    let second = play_room(create(), true, 600);
    assert_eq!(first, second);
}

#[test]
fn intent_ai65_seed617_replays_through_natural_conquest_finish() {
    // Prototype test.js:4809-4835 also requires an AI match to reach its own ending.
    // The native winner and ending tick need not match the prototype's random stream.
    let first = play(Mode::Conquest, Level::Normal, false, 617, 48_000);
    assert!(first.finished, "native AI match never ended");
    let second = play(Mode::Conquest, Level::Normal, false, 617, 48_000);
    assert_eq!(first, second);
}

fn outcome(state: &Match, held_at_three_minutes: Option<usize>) -> Value {
    let teams = state
        .start
        .players
        .iter()
        .map(|p| p.config.team)
        .collect::<Vec<_>>();
    let units = (0..teams.len())
        .map(|slot| {
            state
                .world
                .game
                .units
                .values()
                .filter(|u| u.owner == slot)
                .count()
        })
        .collect::<Vec<_>>();
    serde_json::json!({
        "tick": state.world.game.tick,
        "winner": state.finish.map(|f| f.winner),
        "reason": state.finish.map(|f| f.reason.as_str()),
        "teams": teams,
        "units": units,
        "vp": state.start.economy.players.iter().map(|p| p.vp).collect::<Vec<_>>(),
        "points": state.start.points.len(),
        "heldAtThreeMinutes": held_at_three_minutes,
        "losses": state.story.iter().map(|s| s.losses).sum::<f64>(),
        "kills": state.story.iter().map(|s| s.kills).sum::<f64>(),
        "spent": state.story.iter().map(|s| s.mp_spent).collect::<Vec<_>>(),
        "fighterCaptured": state.story.iter().any(|s| s.kills > 0. && s.captures > 0.),
        "samples": state.timeline.len(),
    })
}

fn js_outcome(mode: Mode, level: Level, enabled: bool, seed: u64, ticks: i64) -> Value {
    let reference = std::env::var_os("WW2_AI65_REFERENCE")
        .expect("set WW2_AI65_REFERENCE to a clean e4e579a prototype checkout");
    let node = std::env::var_os("WW2_AI65_NODE").unwrap_or_else(|| "node".into());
    let script =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../harness/ai65-outcomes.mjs");
    let output = std::process::Command::new(node)
        .arg(script)
        .arg(reference)
        .arg(mode_name(mode))
        .arg(level_name(level))
        .arg(if enabled { "on" } else { "off" })
        .arg(seed.to_string())
        .arg(ticks.to_string())
        .output()
        .expect("start the live pinned JavaScript outcome comparator");
    assert!(
        output.status.success(),
        "prototype failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("prototype outcome JSON")
}

// These are fixed before qualification, following test.js:4809-4826. They compare
// match goals and win/loss cardinality, without requiring the same winning seat.
fn outcome_signature(outcome: &Value, full: bool) -> Value {
    let tick = outcome["tick"].as_u64().unwrap();
    let teams = outcome["teams"].as_array().unwrap();
    let winner = outcome["winner"].as_i64();
    let mut win_loss = teams
        .iter()
        .map(|t| winner.map(|w| w == t.as_i64().unwrap()))
        .collect::<Vec<_>>();
    win_loss.sort();
    let units = outcome["units"].as_array().unwrap();
    let score = outcome["vp"].as_array().unwrap();
    assert!(units.iter().any(|n| n.as_u64().unwrap() > 0));
    assert!(
        score
            .iter()
            .all(|v| v.as_f64().is_some_and(|v| v.is_finite() && v >= 0.))
    );
    assert!(
        outcome["spent"].as_array().unwrap()[..3]
            .iter()
            .all(|v| v.as_f64().unwrap() > 0.)
    );
    if full {
        assert!(
            tick <= 48_000 && winner.is_some(),
            "match must finish within 40 minutes: {outcome}"
        );
        assert!(teams.iter().any(|t| t.as_i64() == winner));
        assert!(
            outcome["heldAtThreeMinutes"].as_u64().unwrap()
                >= outcome["points"].as_u64().unwrap().div_ceil(2)
        );
        assert!(outcome["losses"].as_f64().unwrap() >= 5.);
        assert_eq!(outcome["fighterCaptured"], true);
    } else {
        assert!(tick == 3_600 || winner.is_some());
    }
    assert!(outcome["samples"].as_u64().unwrap() >= tick / 200);
    serde_json::json!({
        "teams": teams, "points": outcome["points"],
        "winLoss": win_loss, "reason": outcome["reason"],
        "scoreStarted": score.iter().any(|v| v.as_f64().unwrap() > 0.),
    })
}

fn compare_outcomes(mode: Mode, level: Level, enabled: bool, seed: u64, full: bool) {
    let ticks = if full { 48_000 } else { 3_600 };
    let source = js_outcome(mode, level, enabled, seed, ticks);
    let native = play_room(
        room_with_weather(mode, level, enabled, seed, true),
        enabled,
        ticks,
    );
    let repeated = play_room(
        room_with_weather(mode, level, enabled, seed, true),
        enabled,
        ticks,
    );
    assert_eq!(
        native, repeated,
        "the compared native profile must replay exactly"
    );
    assert_eq!(
        source["reference"],
        "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
    );
    assert_eq!(source["mode"], mode_name(mode));
    assert_eq!(source["level"], level_name(level));
    assert_eq!(source["seed"], seed);
    assert_eq!(
        source["startup"], native.startup,
        "effective startup profiles differ"
    );
    let js = &source["outcome"];
    println!(
        "AI65 mode={} level={} options={enabled} seed={seed} JS={js} Rust={}",
        mode_name(mode),
        level_name(level),
        native.outcome
    );
    assert_eq!(
        outcome_signature(&native.outcome, full),
        outcome_signature(js, full)
    );
}

macro_rules! outcome_matrix {
    ($name:ident, $mode:expr) => {
        #[test]
        #[ignore = "live comparison requires WW2_AI65_REFERENCE and Node; run explicitly"]
        fn $name() {
            for enabled in [false, true] {
                for level in [Level::Easy, Level::Normal, Level::Hard] {
                    compare_outcomes($mode, level, enabled, 7, false);
                }
            }
        }
    };
}
outcome_matrix!(
    ai65_conquest_outcomes_against_pinned_javascript,
    Mode::Conquest
);
outcome_matrix!(
    ai65_classic_outcomes_against_pinned_javascript,
    Mode::Classic
);
outcome_matrix!(
    ai65_assault_outcomes_against_pinned_javascript,
    Mode::Assault
);
outcome_matrix!(
    ai65_annihilation_outcomes_against_pinned_javascript,
    Mode::Annihilation
);
outcome_matrix!(ai65_horde_outcomes_against_pinned_javascript, Mode::Horde);

#[test]
#[ignore = "live comparison requires WW2_AI65_REFERENCE and Node; run explicitly"]
fn ai65_seed617_natural_outcome_against_pinned_javascript() {
    compare_outcomes(Mode::Conquest, Level::Normal, false, 617, true);
}

fn mode_name(mode: Mode) -> &'static str {
    match mode {
        Mode::Conquest => "conquest",
        Mode::Classic => "classic",
        Mode::Assault => "assault",
        Mode::Annihilation => "annihilation",
        Mode::Horde => "horde",
        _ => unreachable!(),
    }
}
fn level_name(level: Level) -> &'static str {
    match level {
        Level::Easy => "easy",
        Level::Normal => "normal",
        Level::Hard => "hard",
    }
}

fn startup(runtime: &RoomRuntime) -> Value {
    let state = runtime.state();
    let map = crate::authoritative_maps::load(state.options.map_key.as_deref().unwrap()).unwrap();
    let ais = runtime.ai.as_ref().unwrap().seats.iter().map(|entry| {
        serde_json::json!({ "slot": entry.seat.slot, "level": level_name(entry.seat.level),
            "brain": if entry.seat.brain == ww2_core::ai_schedule::Brain::Horde { "horde" } else { "commander" } })
    }).collect::<Vec<_>>();
    serde_json::json!({
        "map": map.source(),
        "mode": mode_name(state.options.mode),
        "teams": state.start.players.iter().map(|p| p.config.team).collect::<Vec<_>>(),
        "factions": state.start.players.iter().map(|p| p.config.faction as u8).collect::<Vec<_>>(),
        "army": state.options.army.key(), "tech": state.options.tech,
        "logistics": state.options.logistics, "supply": state.options.supply,
        "sandbox": state.options.sandbox, "weather": state.start.weather.now.name(),
        "livingWeather": state.start.living.is_some(),
        "defenderTeam": state.assault.map(|a| a.defender_team)
            .or_else(|| state.horde.as_ref().map(|_| 0)),
        "ais": ais,
    })
}
