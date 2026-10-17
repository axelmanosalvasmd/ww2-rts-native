use super::*;
use ww2_core::orders::OrderKey;

#[derive(Clone)]
enum Input {
    Message(usize, Value),
    Poll,
    Disconnect,
    Resume,
    Reconnect,
}

#[test]
fn if28_native_input_log_replays_tick_order_away_pause_and_operative_throttle() {
    // Prototype sim.js:27 and server.js:633,713,784,791-800. The native queue
    // stamps the next tick; the host clock and socket transitions are replay inputs.
    fn replay(reverse_orders: bool) -> (Vec<Value>, Value) {
        let (mut hub, seats, mut receivers) = room();
        let (operative, mut op_rx) = client(3);
        join_operative(&mut hub, &operative, "friend", 0);
        let mut scene = native_tests::native_match();
        for control in scene.commands.units.values_mut() {
            control.hold_fire = true;
            control.auto_cast = false;
            control.auto_retreat = false;
        }
        let unit = scene
            .world
            .game
            .units
            .values()
            .find(|u| u.owner == 0 && scene.rules.kinds[u.kind].name == "rifle")
            .unwrap()
            .id;
        hub.attach_native("ABC", scene, 0).unwrap();
        drain_commanders(&mut hub, &mut receivers, 0);
        messages(&mut op_rx);
        let (returned, mut returned_rx) = client(4);
        let stance = |on| {
            json!({"t":"stance","ids":[unit],"key":"holdPos","on":on,
            "tick":999,"issuer":1,"sequence":999})
        };
        let log = [
            (1, Input::Message(0, stance(!reverse_orders))),
            (2, Input::Message(0, stance(reverse_orders))),
            (19, Input::Message(2, fps(0))),
            (20, Input::Message(2, fps(1))),
            (39, Input::Message(2, fps(2))),
            (40, Input::Message(2, fps(2))),
            (49, Input::Poll),
            (50, Input::Poll),
            (
                51,
                Input::Message(0, json!({"t":"move","orders":[[unit,50.,50.]]})),
            ),
            (52, Input::Message(0, json!({"t":"stop","ids":[unit]}))),
            (60, Input::Disconnect),
            (80, Input::Message(2, fps(3))),
            (100, Input::Poll),
            (110, Input::Resume),
            (150, Input::Poll),
            (170, Input::Reconnect),
            (199, Input::Poll),
            (200, Input::Poll),
            (250, Input::Poll),
        ];
        // Initialize the 20 Hz clock before replaying messages between ticks.
        hub.poll_native(0, 0);
        assert_eq!(state(&hub).world.game.tick, 1);
        let mut checkpoints = Vec::new();
        for (now, input) in &log {
            match input {
                Input::Message(slot, message) => {
                    let connection = [&seats[0], &seats[1], &operative][*slot];
                    command(&mut hub, connection, message.clone(), *now);
                }
                Input::Poll => hub.poll_native(*now, *now * 1_000_000),
                Input::Disconnect => hub.disconnected("ABC", seats[1].id, *now),
                Input::Resume => {
                    hub.rooms
                        .get_mut("ABC")
                        .unwrap()
                        .native
                        .as_mut()
                        .unwrap()
                        .runtime
                        .resume(*now);
                }
                Input::Reconnect => assert!(hello(
                    &mut hub,
                    &returned,
                    json!({"t":"hello","token":"b","name":"b"}),
                    *now
                )),
            }
            match now {
                2 => {
                    assert_eq!(state(&hub).world.game.tick, 1);
                    assert!(!state(&hub).world.game.units[&unit].hold_pos);
                    assert_eq!(
                        state(&hub)
                            .commands
                            .pending_tick(2)
                            .map(|(key, _)| *key)
                            .collect::<Vec<_>>(),
                        [
                            OrderKey {
                                tick: 2,
                                issuer: 0,
                                sequence: 1
                            },
                            OrderKey {
                                tick: 2,
                                issuer: 0,
                                sequence: 2
                            },
                        ]
                    );
                }
                19 => assert_eq!(last_seq(&hub, "friend"), -1),
                20 | 39 => assert_eq!(last_seq(&hub, "friend"), 1),
                40 | 80 => assert_eq!(last_seq(&hub, "friend"), 2),
                49 => assert_eq!(state(&hub).world.game.tick, 1),
                50 => {
                    assert_eq!(state(&hub).world.game.tick, 2);
                    assert_eq!(state(&hub).world.game.units[&unit].hold_pos, reverse_orders);
                    assert_eq!(state(&hub).commands.pending_commands(), 0);
                }
                52 => assert_eq!(
                    state(&hub)
                        .commands
                        .pending_tick(3)
                        .map(|(key, _)| *key)
                        .collect::<Vec<_>>(),
                    [
                        OrderKey {
                            tick: 3,
                            issuer: 0,
                            sequence: 3
                        },
                        OrderKey {
                            tick: 3,
                            issuer: 0,
                            sequence: 4
                        },
                    ]
                ),
                60 | 100 => {
                    assert!(hub.native_status("ABC").unwrap().paused);
                    assert_eq!(state(&hub).world.game.tick, 2);
                    assert_eq!(state(&hub).commands.pending_commands(), 2);
                }
                150 => {
                    assert_eq!(state(&hub).world.game.tick, 3);
                    assert!(state(&hub).start.economy.players[1].away);
                    assert_eq!(state(&hub).commands.pending_commands(), 0);
                    assert!(state(&hub).world.game.units[&unit].path.is_empty());
                }
                170 | 199 => assert!(state(&hub).start.economy.players[1].away),
                200 => {
                    assert_eq!(state(&hub).world.game.tick, 4);
                    assert!(!state(&hub).start.economy.players[1].away);
                }
                250 => assert_eq!(state(&hub).world.game.tick, 5),
                _ => {}
            }
            assert!(hub.native_status("ABC").unwrap().fault.is_none());
            drain_commanders(&mut hub, &mut receivers, *now);
            let mut returned_messages = Vec::new();
            while let Ok(output) = returned_rx.try_recv() {
                match output {
                    Output::Json(message) => returned_messages.push(message),
                    Output::Watcher(_) => {
                        panic!("returning commander cannot receive watcher frames")
                    }
                    Output::Snapshot(write) => {
                        returned_messages.push(serde_json::from_str(&write.json).unwrap());
                        hub.complete_snapshot(&write, true, *now);
                    }
                    Output::Close(_, reason) => panic!("unexpected returning seat close: {reason}"),
                }
            }
            if *now == 170 {
                assert_eq!(returned_messages[0]["t"], "start");
                assert_eq!(returned_messages.last().unwrap()["t"], "lobby");
            }
            messages(&mut op_rx);
            checkpoints.push(state(&hub).save_json().unwrap());
        }
        let final_state = state(&hub).save_json().unwrap();
        (checkpoints, final_state)
    }
    let (recorded, final_state) = replay(false);
    let (replayed, replay_final) = replay(false);
    // Full tagged checkpoints compare all canonical state, queues and RNG bits,
    // rather than accepting a coincidentally equal unit count or winner.
    assert_eq!(recorded, replayed);
    assert_eq!(final_state, replay_final);
    let (_, changed_order) = replay(true);
    assert_ne!(final_state, changed_order);
}
