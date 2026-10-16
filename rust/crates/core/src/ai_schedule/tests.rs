use super::*;
use crate::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    map::AuthoritativeMap,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    recipient_snapshot::{DeliveryState, SelectedSeat},
    weather::WeatherChoice,
};
use serde_json::{Value, json};
use std::cell::RefCell;

fn ordinary_match() -> Match {
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"AI delivery scheduling", "w":80,"h":80,
            "rows":vec![".".repeat(80);80],
            "spawns":[{"x":4,"y":4},{"x":75,"y":75},{"x":4,"y":75}],
            "points":[{"x":40,"y":40}]
        }))
        .unwrap(),
    )
    .unwrap();
    Match::new(
        &map,
        MatchOptions {
            seed: 7301,
            mode: Mode::Conquest,
            players: (0..3)
                .map(|slot| PlayerConfig {
                    name: format!("Seat {slot}"),
                    team: slot,
                    faction: Faction::Usa,
                })
                .collect(),
            shuffle: false,
            army: ArmySize::Standard,
            weather: WeatherChoice::Map,
            weather_seed: Some(1),
            living_weather: false,
            map_key: None,
            tech: false,
            supply: false,
            logistics: false,
            sandbox: false,
        },
    )
    .unwrap()
}

#[test]
fn net73_detached_schedule_retains_real_delivered_frames_across_ordinary_ticks() {
    let mut state = ordinary_match();
    let mut delivery = DeliveryState::new(&state).unwrap();
    let mut schedule = Schedule::default();
    // Horde is a scheduling designation here, not admission of a Horde Match.
    let mut seats = [
        (Brain::Commander, Level::Normal),
        (Brain::Scripted, Level::Hard),
        (Brain::Horde, Level::Normal),
    ]
    .into_iter()
    .enumerate()
    .map(|(slot, (brain, level))| DeliveredSeat::<Value> {
        seat: Seat { slot, level, brain },
        view: None,
    })
    .collect::<Vec<_>>();
    let events = RefCell::new(Vec::<(char, usize, i64)>::new());
    let mut expected_view = [None; 3];
    let mut saw_stale_action = false;
    let mut horde_observations = 0;
    let mut sent_beats = 0;
    for tick in 1..=125_i64 {
        assert_eq!(state.advance_tick().unwrap().tick, tick);
        if tick == 121 {
            state.start.economy.players[0].vp = state.start.win_vp;
            assert!(state.apply_conquest_victory().unwrap().is_some());
        }
        let cadence = match tick {
            1..=40 => 2,
            41..=80 => 3,
            _ => 4,
        };
        let sent = tick > 1 && tick % cadence == 0;
        let finished = state.finish.is_some();
        assert_eq!(finished, tick >= 121);
        let selected = (0..3)
            .map(|slot| SelectedSeat {
                slot,
                full: false,
                omit: false,
            })
            .collect::<Vec<_>>();
        // Collect through the actual recipient producer on every simulation tick,
        // including ticks whose frame the scheduler does not receive.
        let frame = delivery
            .capture_selected_with_lists(&mut state, &selected, &[])
            .unwrap()
            .unwrap();
        let snapshots = (0..3)
            .map(|slot| frame.recipient(slot).unwrap().into_json())
            .collect::<Vec<_>>();
        drop(frame);
        events.borrow_mut().clear();
        schedule
            .tick(
                Beat {
                    tick,
                    sent,
                    finished,
                },
                &mut seats,
                |_| tick != 20,
                |seat| {
                    events.borrow_mut().push(('o', seat.slot, tick));
                    snapshots[seat.slot].clone()
                },
                |seat, view| {
                    events
                        .borrow_mut()
                        .push(('a', seat.slot, view["tick"].as_i64().unwrap()));
                },
            )
            .unwrap();
        if finished {
            assert!(events.borrow().is_empty());
        } else {
            if sent {
                sent_beats += 1;
            }
            let mut expected_events = Vec::new();
            for entry in &seats {
                let seat = entry.seat;
                let wants = match seat.brain {
                    Brain::Commander => tick != 20,
                    // Hard slot one is due at 7,27,47,... and requests the
                    // preceding three ticks as well as its due tick.
                    Brain::Scripted => (4..=7).contains(&(tick % 20)),
                    Brain::Horde => true,
                };
                if sent && wants {
                    expected_view[seat.slot] = Some(tick);
                    expected_events.push(('o', seat.slot, tick));
                    horde_observations += usize::from(seat.brain == Brain::Horde);
                }
            }
            for entry in &seats {
                let seat = entry.seat;
                if let Some(delivered) = expected_view[seat.slot]
                    && (seat.brain == Brain::Commander
                        || (seat.slot == 1 && tick % 20 == 7)
                        || (seat.slot == 2 && tick % 40 == 14))
                {
                    expected_events.push(('a', seat.slot, delivered));
                    saw_stale_action |= delivered < tick;
                }
            }
            assert_eq!(*events.borrow(), expected_events, "tick {tick}");
        }
        for entry in &seats {
            assert_eq!(
                entry
                    .view
                    .as_ref()
                    .map(|view| view["tick"].as_i64().unwrap()),
                expected_view[entry.seat.slot]
            );
        }
        if tick == 20 {
            assert_eq!(expected_view[0], Some(18));
            assert!(events.borrow().contains(&('a', 0, 18)));
            assert!(!events.borrow().contains(&('o', 0, 20)));
        }
        if tick == 22 {
            assert!(events.borrow().contains(&('o', 0, 22)));
            assert!(events.borrow().contains(&('a', 0, 22)));
        }
    }
    assert!(saw_stale_action);
    assert_eq!(horde_observations, sent_beats);
    schedule.reset(&mut seats);
    assert!(seats.iter().all(|entry| entry.view.is_none()));
    schedule
        .tick(
            Beat {
                tick: 0,
                sent: false,
                finished: false,
            },
            &mut seats,
            |_| panic!("no delivery"),
            |_| panic!("no delivery"),
            |_, _| panic!("no view"),
        )
        .unwrap();
}

#[test]
fn scripted_difficulty_stagger_and_invalid_inputs_do_not_refresh_views() {
    for (level, expected_ticks) in [
        (Level::Easy, vec![107, 227]),
        (Level::Normal, vec![27, 67, 107, 147, 187, 227]),
        (
            Level::Hard,
            vec![7, 27, 47, 67, 87, 107, 127, 147, 167, 187, 207, 227],
        ),
    ] {
        let mut schedule = Schedule::default();
        let mut seats = vec![DeliveredSeat {
            seat: Seat {
                slot: 1,
                level,
                brain: Brain::Scripted,
            },
            view: None,
        }];
        let actions = RefCell::new(Vec::new());
        for tick in 0..=240 {
            schedule
                .tick(
                    Beat {
                        tick,
                        sent: true,
                        finished: false,
                    },
                    &mut seats,
                    |_| panic!("scripted uses shared cadence"),
                    |_| tick,
                    |_, view| actions.borrow_mut().push((tick, *view)),
                )
                .unwrap();
        }
        let expected = expected_ticks
            .into_iter()
            .map(|tick| (tick, tick))
            .collect::<Vec<_>>();
        assert_eq!(*actions.borrow(), expected);
        assert!(!actions.borrow().is_empty());
        assert_eq!(
            schedule.tick(
                Beat {
                    tick: 240,
                    sent: true,
                    finished: false
                },
                &mut seats,
                |_| panic!(),
                |_| panic!(),
                |_, _| panic!()
            ),
            Err(ScheduleError::InvalidTick)
        );
        let prior = seats[0].view;
        seats.push(DeliveredSeat {
            seat: seats[0].seat,
            view: None,
        });
        assert_eq!(
            schedule.tick(
                Beat {
                    tick: 241,
                    sent: true,
                    finished: false
                },
                &mut seats,
                |_| panic!(),
                |_| panic!(),
                |_, _| panic!()
            ),
            Err(ScheduleError::DuplicateSeat)
        );
        assert_eq!(seats[0].view, prior);
    }
}

#[path = "seat_delivery_tests.rs"]
mod seat_delivery;
