use super::*;
use crate::{
    commander_ai::Observation,
    human_commander::HumanView,
    match_commands::{Command, CommandResult},
    match_human_observation::HumanObservationMemory,
    orders::OrderKey,
    recipient_snapshot::{DeliveryState, SelectedSeat},
    seat_commander::{Advance, Delivered},
};
use serde_json::{Value, json};
use std::cell::RefCell;

#[derive(Debug, PartialEq)]
struct Trace {
    deliveries: Vec<(i64, usize)>,
    views: Vec<HumanView<Observation>>,
    commands: Vec<(i64, usize, Value)>,
    results: Vec<CommandResult>,
    snapshots: Vec<Value>,
}

// The host controls delivery. The policy only receives detached observations;
// it never gets access to the Match used by this harness to execute its inputs.
fn play(beats: &[Beat]) -> Trace {
    let state = RefCell::new(super::tests::ordinary_match());
    let mut delivery = DeliveryState::new(&state.borrow()).unwrap();
    let levels = [Level::Easy, Level::Normal, Level::Hard];
    let commanders = RefCell::new(
        levels
            .iter()
            .enumerate()
            .map(|(slot, level)| state.borrow().create_seat_commander(slot, *level).unwrap())
            .collect::<Vec<_>>(),
    );
    let memories = RefCell::new((0..3).map(HumanObservationMemory::new).collect::<Vec<_>>());
    let mut seats = levels
        .into_iter()
        .enumerate()
        .map(|(slot, level)| DeliveredSeat::<HumanView<Observation>> {
            seat: Seat {
                slot,
                level,
                brain: Brain::Commander,
            },
            view: None,
        })
        .collect::<Vec<_>>();
    let mut schedule = Schedule::default();
    let mut trace = Trace {
        deliveries: vec![],
        views: vec![],
        commands: vec![],
        results: vec![],
        snapshots: vec![],
    };
    for beat in beats {
        let report = state.borrow_mut().advance_tick().unwrap();
        assert_eq!(report.tick, beat.tick);
        trace.results.extend(report.commands);
        let emitted = RefCell::new(Vec::new());
        schedule
            .tick(
                *beat,
                &mut seats,
                |seat| commanders.borrow()[seat.slot].wants_view(beat.tick, beat.sent),
                |seat| {
                    trace.deliveries.push((beat.tick, seat.slot));
                    let view = state
                        .borrow_mut()
                        .human_commander_observation(&mut memories.borrow_mut()[seat.slot])
                        .unwrap();
                    trace.views.push(view.clone());
                    view
                },
                |seat, view| {
                    let result = state
                        .borrow()
                        .advance_seat_commander(
                            &mut commanders.borrow_mut()[seat.slot],
                            Some(Delivered::Human(Box::new(view.clone()))),
                            0,
                            &[],
                        )
                        .unwrap();
                    let Advance::Human(result) = result else {
                        panic!("ordinary commander")
                    };
                    if let Some(command) = result.input.and_then(|input| input.command) {
                        emitted.borrow_mut().push((beat.tick, seat.slot, command));
                    }
                },
            )
            .unwrap();
        for (tick, slot, command) in emitted.into_inner() {
            state
                .borrow_mut()
                .enqueue_command(
                    OrderKey {
                        tick: tick as u64 + 1,
                        issuer: slot as u32,
                        sequence: 0,
                    },
                    Command::from_json(&command).unwrap(),
                )
                .unwrap();
            trace.commands.push((tick, slot, command));
        }
        let mut state = state.borrow_mut();
        let selected = (0..3)
            .map(|slot| SelectedSeat {
                slot,
                full: true,
                omit: false,
            })
            .collect::<Vec<_>>();
        let frame = delivery
            .capture_selected_with_tick_events(&mut state, &selected, &report.events)
            .unwrap()
            .unwrap();
        for slot in 0..3 {
            trace
                .snapshots
                .push(frame.recipient(slot).unwrap().into_json());
        }
        assert!(state.tick_state.fault().is_none());
    }
    // Resolve the last callback's delayed commands without scheduling new AI work.
    let mut state = state.borrow_mut();
    let report = state.advance_tick().unwrap();
    trace.results.extend(report.commands);
    assert!(state.commands.queue.is_empty());
    let selected = (0..3)
        .map(|slot| SelectedSeat {
            slot,
            full: true,
            omit: false,
        })
        .collect::<Vec<_>>();
    let frame = delivery
        .capture_selected_with_tick_events(&mut state, &selected, &report.events)
        .unwrap()
        .unwrap();
    for slot in 0..3 {
        trace
            .snapshots
            .push(frame.recipient(slot).unwrap().into_json());
    }
    assert!(state.tick_state.fault().is_none());
    assert_eq!(trace.results.len(), trace.commands.len());
    trace
}

#[test]
fn if29_recorded_adaptive_beats_replay_commander_commands_and_recipient_frames() {
    // Prototype room-tick.js:14-21 selects sent before aiTick. Its meter's
    // next cadence affects the next callback, never the beat already selected.
    let beats = (1..=320)
        .map(|tick| {
            let cadence = match tick {
                1..=80 => 3,
                81..=160 => 2,
                _ => 4,
            };
            Beat {
                tick,
                sent: tick % cadence == 0,
                finished: false,
            }
        })
        .collect::<Vec<_>>();
    let log = serde_json::to_string(&beats).unwrap();
    let replayed: Vec<Beat> = serde_json::from_str(&log).unwrap();
    assert_eq!(replayed, beats);
    assert_eq!(
        serde_json::to_value(beats[2]).unwrap(),
        json!({"tick":3,"sent":true,"finished":false})
    );
    let original = play(&beats);
    assert!(!original.commands.is_empty());
    for slot in 0..3 {
        assert!(original.commands.iter().any(|(_, owner, _)| *owner == slot));
        assert!(
            original
                .deliveries
                .iter()
                .any(|(tick, owner)| tick % 2 == 1 && *owner == slot)
        );
    }
    assert_eq!(play(&replayed), original);
    // Discarding the recorded beat and assuming today's default is observable.
    let fixed = beats
        .iter()
        .map(|beat| Beat {
            sent: beat.tick % 2 == 0,
            ..*beat
        })
        .collect::<Vec<_>>();
    let changed = play(&fixed);
    assert_ne!(changed.deliveries, original.deliveries);
    assert_ne!(changed.commands, original.commands);
}
