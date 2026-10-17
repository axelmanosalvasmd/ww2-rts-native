//! Seeded native Conquest runs through delivered observations and real input admission.
//! Purpose-specific native RNG streams are retained; this is not a JS-aligned replay.

use serde_json::{Value, json};
use ww2_core::{
    commander_ai::Observation,
    economy::Mode,
    human_commander::{HumanView, OwnerHurtBatch},
    human_commander_inputs::InputKind,
    human_commander_policy::{self, HumanPolicy},
    human_commander_state::HumanState,
    map::AuthoritativeMap,
    match_commands::Command,
    match_human_observation::{HumanObservationMemory, HumanShot},
    match_runtime::{Match, MatchOptions},
    recipient_snapshot::{DeliveryState, SelectedSeat},
};

/// One actual physical input, including non-command camera and selection work.
#[derive(Clone, Debug, PartialEq)]
pub struct InputRecord {
    pub slot: usize,
    pub tick: u64,
    pub kind: InputKind,
    pub command: Option<Value>,
    pub accepted: Option<bool>,
}
impl InputRecord {
    /// The row shape used by the AI-65 oracle: slot, tick, kind, cmd, accepted.
    pub fn row(&self) -> Value {
        json!([
            self.slot,
            self.tick,
            match self.kind {
                InputKind::Camera => "camera",
                InputKind::Select => "select",
                InputKind::Key => "key",
                InputKind::Click => "click",
            },
            self.command,
            self.accepted,
        ])
    }
}

pub struct Run {
    pub state: Match,
    pub inputs: Vec<InputRecord>,
    /// Last actual recipient frame for every seat, also checked on every send beat.
    pub recipients: Vec<Value>,
    pub snapshot_passes: u64,
}

struct Seat {
    memory: HumanObservationMemory,
    view: HumanView<Observation>,
    human: Option<HumanState>,
    policy: HumanPolicy,
}

/// `options.seed` covers creation and simulation. Seat order is ascending slot.
/// Other modes and enabled logistics remain outside the current recipient/tick envelope.
pub fn play_match(
    map: &AuthoritativeMap,
    options: MatchOptions,
    levels: &[&str],
    max_ticks: u64,
) -> Result<Run, String> {
    if !matches!(options.mode, Mode::Default | Mode::Conquest) || options.logistics {
        return Err("AI match runner supports ordinary Conquest without logistics".into());
    }
    if levels.len() != options.players.len()
        || levels
            .iter()
            .any(|level| !matches!(*level, "easy" | "normal" | "hard"))
        || max_ticks > 9_007_199_254_740_991
    {
        return Err("invalid AI runner seats, levels or tick limit".into());
    }
    let seed = options.seed;
    let mut state = Match::new(map, options)?;
    let mut delivery =
        DeliveryState::new(&state).map_err(|e| format!("initial delivery: {e:?}"))?;
    let mut seats = Vec::new();
    // playMatch delivers an opening view to all seats before its first step.
    for slot in 0..levels.len() {
        let mut memory = HumanObservationMemory::new(slot);
        let view = state.human_commander_observation(&mut memory)?;
        seats.push(Seat {
            memory,
            view,
            human: None,
            policy: HumanPolicy::new(slot as u32, seed),
        });
    }
    let mut run = Run {
        state,
        inputs: Vec::new(),
        recipients: Vec::new(),
        snapshot_passes: 0,
    };
    let mut shots = Vec::new();
    let mut events = Vec::new();
    let mut generation = 0;
    while run.state.finish.is_none() && (run.state.world.game.tick as u64) < max_ticks {
        let report = run
            .state
            .advance_tick()
            .map_err(|e| format!("AI runner step: {e}"))?;
        let now = run.state.world.game.tick as u64;
        shots.extend(report.events.iter().map(HumanShot::tick));
        events.extend(report.events);
        let sent = now.is_multiple_of(2) || run.state.finish.is_some();
        // Complete the observation pass before admitting even the first seat's input.
        for seat in &mut seats {
            if sent
                && seat
                    .human
                    .as_ref()
                    .is_none_or(|human| human.wants_view(now))
            {
                seat.view = run.state.human_commander_observation(&mut seat.memory)?;
            }
        }
        for (slot, seat) in seats.iter_mut().enumerate() {
            if seat.human.is_none() {
                seat.human = Some(HumanState::new(
                    run.state.start.environment_seed,
                    slot as u32,
                    seat.view
                        .spawn
                        .ok_or("ordinary delivered seat has no home")?,
                ));
            }
            let human = seat.human.as_mut().unwrap();
            let hurts: OwnerHurtBatch =
                run.state.human_commander_hurts(slot, generation, &shots)?;
            let control = run.state.human_commander_control(slot, levels[slot])?;
            let advanced = human_commander_policy::advance(
                human,
                &mut seat.policy,
                control,
                Some(&seat.view),
                &hurts,
            )
            .map_err(|e| format!("AI runner seat {slot} tick {now}: {e:?}"))?;
            if let Some(input) = advanced.input {
                let accepted = input
                    .command
                    .as_ref()
                    .map(|raw| {
                        let command =
                            Command::from_json(raw).map_err(|e| format!("AI command: {e:?}"))?;
                        run.state
                            .execute_command(slot, &command, run.state.finish.is_some())
                            .map(|denial| denial.is_none())
                    })
                    .transpose()?;
                run.inputs.push(InputRecord {
                    slot,
                    tick: now,
                    kind: input.kind,
                    command: input.command,
                    accepted,
                });
            }
        }
        if sent {
            let selected: Vec<_> = (0..seats.len())
                .map(|slot| SelectedSeat {
                    slot,
                    full: false,
                    omit: false,
                })
                .collect();
            let frame = delivery
                .capture_selected_with_tick_events(&mut run.state, &selected, &events)
                .map_err(|e| format!("AI runner tick {now} recipient pass: {e:?}"))?
                .ok_or("missing AI runner delivery")?;
            run.recipients = (0..seats.len())
                .map(|slot| {
                    frame
                        .recipient(slot)
                        .map(|recipient| recipient.into_json())
                        .map_err(|e| format!("AI runner seat {slot}: {e:?}"))
                })
                .collect::<Result<_, _>>()?;
            run.snapshot_passes += 1;
            shots.clear();
            events.clear();
            generation += 1;
        }
    }
    Ok(run)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ww2_core::{
        economy::Faction, economy_state::ArmySize, match_runtime::PlayerConfig,
        weather::WeatherChoice,
    };

    fn options(seed: u64) -> MatchOptions {
        MatchOptions {
            seed,
            mode: Mode::Conquest,
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
            weather_seed: Some(7),
            living_weather: false,
            map_key: None,
            tech: false,
            supply: false,
            logistics: false,
            sandbox: false,
        }
    }

    #[test]
    fn bounded_seed7_default_map_repeats_actual_inputs_and_all_seat_snapshots_at_every_level() {
        // Byte-for-byte authored default map from the pinned e4e579a source, not a recording.
        let map = AuthoritativeMap::from_json(include_bytes!(
            "../tests/fixtures/ai-runner-default-map.json"
        ))
        .unwrap();
        let mut logs = Vec::new();
        for level in ["easy", "normal", "hard"] {
            let a = play_match(&map, options(7), &[level; 3], 200).unwrap();
            let b = play_match(&map, options(7), &[level; 3], 200).unwrap();
            assert_eq!(a.state.world.game.tick, 200, "{level}");
            assert_eq!(a.snapshot_passes, 100);
            assert_eq!(a.inputs, b.inputs, "{level}");
            assert_eq!(a.recipients, b.recipients, "{level}");
            assert_eq!(
                a.state.start.random.snapshot(),
                b.state.start.random.snapshot()
            );
            assert!(a.state.tick_state.fault().is_none());
            for slot in 0..3 {
                assert!(
                    a.inputs
                        .iter()
                        .any(|i| i.slot == slot && i.accepted == Some(true))
                );
                assert_eq!(a.recipients[slot]["tick"], 200);
            }
            assert!(
                a.inputs
                    .iter()
                    .all(|i| i.command.is_some() == i.accepted.is_some())
            );
            let rows: Vec<_> = a.inputs.iter().map(InputRecord::row).collect();
            assert!(rows.iter().all(|r| r.as_array().unwrap().len() == 5));
            logs.push(rows);
        }
        assert_ne!(logs[0], logs[1]);
        assert_ne!(logs[1], logs[2]);
    }

    #[test]
    fn seed7_long_run_delivers_all_seats_beyond_the_former_projectile_boundary() {
        let map = AuthoritativeMap::from_json(include_bytes!(
            "../tests/fixtures/ai-runner-default-map.json"
        ))
        .unwrap();
        let a = play_match(&map, options(7), &["easy"; 3], 3600).unwrap();
        let b = play_match(&map, options(7), &["easy"; 3], 3600).unwrap();
        let tick = a.state.world.game.tick as u64;
        assert!(
            tick > 310,
            "the former projectile refusal must be traversed"
        );
        assert!(tick == 3600 || a.state.finish.is_some());
        assert_eq!(a.snapshot_passes, tick.div_ceil(2));
        assert_eq!(a.snapshot_passes, b.snapshot_passes);
        assert_eq!(a.inputs, b.inputs);
        assert_eq!(a.recipients, b.recipients);
        assert_eq!(
            a.state.start.random.snapshot(),
            b.state.start.random.snapshot()
        );
        assert!(a.state.tick_state.fault().is_none());
        assert!(b.state.tick_state.fault().is_none());
        assert_eq!(a.recipients.len(), 3);
        for recipient in &a.recipients {
            assert_eq!(recipient["t"], "s");
            assert_eq!(recipient["tick"], tick);
        }
        assert!(
            a.inputs
                .iter()
                .all(|input| { input.command.is_some() == input.accepted.is_some() })
        );
    }

    #[test]
    fn seeded_runner_refuses_modes_and_logistics_outside_the_recipient_envelope() {
        let map = AuthoritativeMap::from_json(include_bytes!(
            "../tests/fixtures/ai-runner-default-map.json"
        ))
        .unwrap();
        for mode in [
            Mode::Classic,
            Mode::Assault,
            Mode::Annihilation,
            Mode::Horde,
            Mode::World,
        ] {
            let mut setup = options(7);
            setup.mode = mode;
            assert!(play_match(&map, setup, &["normal"; 3], 1).is_err());
        }
        let mut setup = options(7);
        setup.logistics = true;
        assert!(play_match(&map, setup, &["normal"; 3], 1).is_err());
        assert!(play_match(&map, options(7), &["unknown"; 3], 1).is_err());
    }
}
