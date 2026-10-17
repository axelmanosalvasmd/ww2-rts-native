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
}

fn room(mode: Mode, level: Level, enabled: bool, seed: u64) -> RoomRuntime {
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
        living_weather: false,
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
    };
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
    // Resolve every final emitted command through the ordinary next-tick admission.
    // This drain tick emits no new policy inputs.
    if runtime.state().finish.is_none() {
        let report = runtime.advance_tick(&vec![false; seats]).unwrap();
        replay.admissions.extend(report.commands);
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
