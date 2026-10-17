use crate::ai_schedule::*;
use crate::commander_ai::Level;
use crate::{
    ai_seat_view::{SeatMemory, SeatView},
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::Point,
    map::AuthoritativeMap,
    match_commands::{Command, MoveOrder},
    match_runtime::{Match, MatchOptions, PlayerConfig},
    orders::OrderKey,
    recipient_snapshot::{DeliveryState, SelectedSeat},
    weather::WeatherChoice,
};
use serde_json::json;
use std::cell::RefCell;

#[derive(Default)]
struct Trace {
    tick: usize,
    events: Vec<String>,
    acted: bool,
}
#[test]
fn ai14_observe_all_precedes_act_all_and_undelivered_seats_wait() {
    // Preserve caller seat order, as ai-schedule.js does for both loops.
    let mut schedule = Schedule::default();
    let mut seats = [2, 0].map(|slot| DeliveredSeat {
        seat: Seat {
            slot,
            level: Level::Normal,
            brain: Brain::Commander,
        },
        view: None,
    });
    let c = RefCell::new(Trace::default());
    let run = |schedule: &mut Schedule, seats: &mut [DeliveredSeat<usize>], tick, sent| {
        c.borrow_mut().tick = tick;
        let outputs = RefCell::new(Vec::new());
        schedule
            .tick(
                Beat {
                    tick: tick as i64,
                    sent,
                    finished: false,
                },
                seats,
                |_| true,
                |seat| {
                    let mut c = c.borrow_mut();
                    assert!(
                        !c.acted,
                        "an action ran before the observation loop finished"
                    );
                    let tick = c.tick;
                    c.events.push(format!("observe {} at {tick}", seat.slot));
                    tick
                },
                |seat, view| {
                    let mut c = c.borrow_mut();
                    c.acted = true;
                    c.events.push(format!("act {} from {view}", seat.slot));
                    outputs.borrow_mut().push((seat.slot, *view));
                },
            )
            .unwrap();
        outputs.into_inner()
    };
    assert!(run(&mut schedule, &mut seats, 1, false).is_empty());
    assert!(c.borrow().events.is_empty());
    assert_eq!(run(&mut schedule, &mut seats, 2, true), [(2, 2), (0, 2)]);
    assert_eq!(
        c.borrow().events,
        [
            "observe 2 at 2",
            "observe 0 at 2",
            "act 2 from 2",
            "act 0 from 2"
        ]
    );
    c.borrow_mut().events.clear();
    c.borrow_mut().acted = false;
    assert_eq!(run(&mut schedule, &mut seats, 3, false), [(2, 2), (0, 2)]);
    assert_eq!(c.borrow().events, ["act 2 from 2", "act 0 from 2"]);
    // A fresh AI attachment cannot plan from the previous controller's state.
    let mut fresh = Schedule::default();
    let mut fresh_seats = [DeliveredSeat {
        seat: seats[1].seat,
        view: None,
    }];
    assert!(run(&mut fresh, &mut fresh_seats, 3, false).is_empty());
    c.borrow_mut().acted = false;
    assert_eq!(run(&mut fresh, &mut fresh_seats, 4, true), [(0, 4)]);
    let mut duplicates = [
        DeliveredSeat {
            seat: seats[1].seat,
            view: None::<usize>,
        },
        DeliveredSeat {
            seat: seats[1].seat,
            view: None,
        },
    ];
    assert_eq!(
        Schedule::default().tick(
            Beat {
                tick: 1,
                sent: true,
                finished: false
            },
            &mut duplicates,
            |_| panic!("duplicate seats must refuse before callbacks"),
            |_| panic!("duplicate seats must refuse before callbacks"),
            |_, _| panic!("duplicate seats must refuse before callbacks"),
        ),
        Err(ScheduleError::DuplicateSeat)
    );
}

struct Room {
    state: Match,
    delivery: DeliveryState,
    observations: Vec<usize>,
}
struct Walker {
    memory: SeatMemory,
    rifle: u32,
    issued: bool,
}
const OWN_MINE: usize = 8 * 128 + 20;
impl Walker {
    fn observe(&mut self, c: &mut Room, slot: usize) -> Result<SeatView, String> {
        c.observations.push(slot);
        let view = self
            .memory
            .observe_match(&mut c.state, &mut c.delivery)
            .map_err(|e| format!("seat observation: {e:?}"))?;
        assert_eq!(view.own_mines.contains(&OWN_MINE), slot == 0);
        if slot == 0 {
            assert_eq!(view.terrain.chars[OWN_MINE], b'N');
        }
        Ok(view)
    }
    fn act(&mut self, c: &mut Room, slot: usize, view: &SeatView) -> Result<bool, String> {
        assert_eq!(c.observations, [0, 1]);
        if self.issued || c.state.finish.is_some() || view.players[slot].out {
            return Ok(false);
        }
        // This test brain uses only its delivered public home and its own ID.
        let home = view.players[slot].spawn.ok_or("missing delivered home")?;
        c.state
            .enqueue_command(
                OrderKey {
                    tick: c.state.world.game.tick as u64 + 1,
                    issuer: slot as u32,
                    sequence: 0,
                },
                Command::Move {
                    orders: vec![MoveOrder {
                        unit: i64::from(self.rifle),
                        at: Point {
                            x: home.x + 6.,
                            z: home.z + 6.,
                        },
                    }],
                    control_ids: None,
                    attack_move: false,
                    queue: false,
                    together: false,
                    face: None,
                },
            )
            .map_err(|e| format!("canonical admission queue: {e:?}"))?;
        self.issued = true;
        Ok(true)
    }
}

fn ordinary() -> Match {
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"AI room lifetime", "w":128, "h":128,
            "rows":vec![".".repeat(128);128],
            "spawns":[{"x":8,"y":8},{"x":112,"y":112}],
            "points":[{"id":"home","x":8,"y":8,"mp":0,"vp":5}]
        }))
        .unwrap(),
    )
    .unwrap();
    let mut state = Match::new(
        &map,
        MatchOptions {
            seed: 7,
            mode: Mode::Conquest,
            players: (0..2)
                .map(|slot| PlayerConfig {
                    name: format!("Seat {slot}"),
                    team: slot,
                    faction: Faction::Usa,
                })
                .collect(),
            shuffle: false,
            army: ArmySize::Standard,
            weather: WeatherChoice::Map,
            weather_seed: Some(7),
            living_weather: false,
            map_key: None,
            tech: false,
            supply: false,
            logistics: false,
            sandbox: false,
        },
    )
    .unwrap();
    // Use a short victory target while retaining a valid authored point rate.
    state.start.win_vp = 20.0;
    state
}

#[test]
fn ai05_ai_and_browser_both_receive_edits_in_either_delivery_order() {
    for browser_first in [false, true] {
        let mut state = ordinary();
        let mut delivery = DeliveryState::new(&state).unwrap();
        let mut memory = SeatMemory::from_match(&state, 0).unwrap();
        let before = memory.observe_match(&mut state, &mut delivery).unwrap();
        state.advance_tick().unwrap();
        state.world.set_cell(OWN_MINE, b'N').unwrap();
        state.world.mines.insert(OWN_MINE, 0);
        let browser = |state: &mut Match, delivery: &mut DeliveryState| {
            delivery
                .capture_selected_with_lists(
                    state,
                    &[SelectedSeat {
                        slot: 0,
                        full: false,
                        omit: false,
                    }],
                    &[],
                )
                .unwrap()
                .unwrap()
                .recipient(0)
                .unwrap()
                .into_json()
        };
        let (ai, human) = if browser_first {
            let human = browser(&mut state, &mut delivery);
            // A takeover starts after the human has drained its last delta.
            memory = SeatMemory::from_match(&state, 0).unwrap();
            (
                memory.observe_match(&mut state, &mut delivery).unwrap(),
                human,
            )
        } else {
            let ai = memory.observe_match(&mut state, &mut delivery).unwrap();
            (ai, browser(&mut state, &mut delivery))
        };
        assert_eq!(before.terrain.chars[OWN_MINE], b'.');
        assert_eq!(ai.terrain.chars[OWN_MINE], b'N');
        assert!(ai.own_mines.contains(&OWN_MINE));
        let row = human["cells"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row[0] == OWN_MINE)
            .unwrap();
        assert_eq!(row[1], "N");
        assert_eq!(row[4]["mineOwned"], true);
    }
}

#[test]
fn ai14_private_views_and_queued_orders_survive_a_full_match_and_final_snapshots() {
    let mut state = ordinary();
    state.world.set_cell(OWN_MINE, b'N').unwrap();
    state.world.mines.insert(OWN_MINE, 0);
    let walkers = (0..2)
        .map(|slot| {
            let rifle = state
                .world
                .game
                .units
                .values()
                .find(|u| u.owner == slot && state.rules.kinds[u.kind].name == "rifle")
                .unwrap()
                .id;
            Walker {
                memory: SeatMemory::from_match(&state, slot).unwrap(),
                rifle,
                issued: false,
            }
        })
        .collect::<Vec<_>>();
    let delivery = DeliveryState::new(&state).unwrap();
    let mut room = Room {
        state,
        delivery,
        observations: Vec::new(),
    };
    let walkers = RefCell::new(walkers);
    let mut schedule = Schedule::default();
    let mut seats = [0, 1].map(|slot| DeliveredSeat {
        seat: Seat {
            slot,
            level: Level::Normal,
            brain: Brain::Commander,
        },
        view: None,
    });
    let mut admitted = 0;
    let mut sent_orders = 0;
    let mut finished_at = None;
    for _ in 0..2000 {
        let report = room.state.advance_tick().unwrap();
        for result in report.commands {
            assert!(result.denial.is_none(), "{}", result.denial.unwrap().name());
            admitted += 1;
        }
        let now = room.state.world.game.tick;
        let sent = now % 2 == 0 || room.state.finish.is_some();
        if sent {
            room.observations.clear();
        }
        let finished = room.state.finish.is_some();
        let callbacks = RefCell::new(Vec::new());
        {
            let room = RefCell::new(&mut room);
            schedule
                .tick(
                    Beat {
                        tick: now,
                        sent,
                        finished,
                    },
                    &mut seats,
                    |_| true,
                    |seat| {
                        callbacks.borrow_mut().push(('o', seat.slot));
                        walkers.borrow_mut()[seat.slot]
                            .observe(&mut room.borrow_mut(), seat.slot)
                            .unwrap()
                    },
                    |seat, view| {
                        callbacks.borrow_mut().push(('a', seat.slot));
                        sent_orders += usize::from(
                            walkers.borrow_mut()[seat.slot]
                                .act(&mut room.borrow_mut(), seat.slot, view)
                                .unwrap(),
                        );
                    },
                )
                .unwrap();
        }
        if finished {
            assert!(
                callbacks.borrow().is_empty(),
                "winner must idle every AI callback"
            );
        }
        if sent {
            let selected = [0, 1].map(|slot| SelectedSeat {
                slot,
                full: false,
                omit: false,
            });
            let frame = room
                .delivery
                .capture_selected_with_tick_events(&mut room.state, &selected, &report.events)
                .unwrap()
                .unwrap();
            for slot in 0..2 {
                assert_eq!(frame.recipient(slot).unwrap().into_json()["t"], "s");
            }
        }
        assert!(room.state.tick_state.fault().is_none());
        if room.state.finish.is_some() {
            let first = *finished_at.get_or_insert(now);
            if now >= first + 8 {
                break;
            }
        }
    }
    assert!(
        finished_at.is_some(),
        "authored ordinary match must reach VP victory"
    );
    assert_eq!((sent_orders, admitted), (2, 2));
    assert_eq!(room.state.finish.as_ref().unwrap().winner, 0);
}

#[test]
fn classic_node_memory_stays_dormant_through_ordinary_ticks_and_every_seat_snapshot() {
    use crate::{
        commander_ai::Level, commander_tactics, match_human_observation::HumanObservationMemory,
        random::MatchRandom,
    };
    let mut state = ordinary();
    let mut delivery = DeliveryState::new(&state).unwrap();
    let mut memories = [
        HumanObservationMemory::new(0),
        HumanObservationMemory::new(1),
    ];
    let mut tactics = [
        commander_tactics::State::default(),
        commander_tactics::State::default(),
    ];
    let mut random = MatchRandom::new(7);
    for _ in 0..8 {
        let report = state.advance_tick().unwrap();
        if state.world.game.tick % 2 != 0 {
            continue;
        }
        for slot in 0..2 {
            let view = state
                .human_commander_observation(&mut memories[slot])
                .unwrap();
            let context = view.packet.input().runtime.as_ref().unwrap();
            assert_eq!(context.mode, Mode::Conquest);
            assert!(context.nodes.is_empty() && context.claimed_nodes.is_empty());
            let commands = commander_tactics::plan(
                &view.packet,
                &[],
                &mut tactics[slot],
                Level::Normal,
                &mut random,
            )
            .unwrap();
            assert!(tactics[slot].node.is_empty());
            assert!(
                !commands
                    .iter()
                    .any(|c| c["t"] == "assist" || c["kind"] == "depot")
            );
        }
        let selected = [0, 1].map(|slot| SelectedSeat {
            slot,
            full: false,
            omit: false,
        });
        let frame = delivery
            .capture_selected_with_tick_events(&mut state, &selected, &report.events)
            .unwrap()
            .unwrap();
        for slot in 0..2 {
            assert_eq!(frame.recipient(slot).unwrap().into_json()["t"], "s");
        }
    }
    assert!(state.tick_state.fault().is_none());
}
