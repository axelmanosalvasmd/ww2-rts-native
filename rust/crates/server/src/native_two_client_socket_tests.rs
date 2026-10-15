//! Both seated sockets control owned units through the actual native runner.
use super::*;
use ww2_core::match_commands::MoveOutcome;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn both_seated_sockets_move_and_stop_owned_units_in_one_match() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (stop, stopped) = oneshot::channel();
    let (ready, receipt) = oneshot::channel();
    let server = tokio::spawn(serve_with_native_control(
        listener,
        Config::default(),
        async move {
            let _ = stopped.await;
        },
        Some(ready),
    ));
    let control = receipt.await.unwrap();
    let host = tokio::runtime::Handle::current();
    tokio::task::spawn_blocking(move || {
        let mut clients = [
            Receiver::connect(address, "a"),
            Receiver::connect(address, "b"),
        ];
        let mut state = setup("default");
        // Only declared initial controls are authored. Later controls enter through sockets.
        for unit in state.world.game.units.values_mut() {
            unit.hold_pos = true;
        }
        let owners = state
            .world
            .game
            .units
            .values()
            .map(|unit| (unit.id, unit.owner))
            .collect::<std::collections::BTreeMap<_, _>>();
        let selected = [0, 1].map(|slot| {
            let unit = state
                .world
                .game
                .units
                .values()
                .find(|unit| unit.owner == slot && state.rules.kinds[unit.kind].name == "rifle")
                .unwrap();
            let origin = unit.at();
            let goal = Point {
                x: origin.x,
                z: origin.z + if slot == 0 { 12. } else { -12. },
            };
            (unit.id, origin, goal)
        });
        let mut expected = SetupExpected::new(&state);
        expected.public_map = serde_json::from_str(include_str!("maps/default.json")).unwrap();
        let epoch = host.block_on(control.attach("ABC", state)).unwrap();
        for (slot, client) in clients.iter_mut().enumerate() {
            let start = client.until(|message| message["t"] == "start");
            expected.check(&start, slot, epoch);
            assert_eq!(client.view.unit_count(), 0);
            client.until(|message| message["t"] == "s");
            assert_eq!(
                client.view.unit(selected[slot].0).unwrap().owner,
                slot as i32
            );
        }
        let before_move = host.block_on(control.inspect("ABC")).unwrap();
        assert_eq!(before_move.epoch, epoch);
        assert!(!before_move.paused && before_move.fault.is_none());
        for (client, &(id, _, goal)) in clients.iter_mut().zip(&selected) {
            client.send(json!({"t":"move","orders":[[id,goal.x,goal.z]]}));
        }
        let moving_ticks = std::array::from_fn::<_, 2, _>(|slot| {
            let (id, origin, _) = selected[slot];
            let client = &mut clients[slot];
            let snapshot = client.until(|message| {
                message["t"] == "s"
                    && message["movement"].as_array().is_some_and(|rows| {
                        rows.iter().any(|row| row[0] == id && row[1] == "moving")
                    })
                    && message["units"].as_array().unwrap().iter().any(|row| {
                        row[0] == id
                            && ((row[3].as_f64().unwrap() - origin.x).abs()
                                + (row[4].as_f64().unwrap() - origin.z).abs())
                                > 0.1
                    })
            });
            let decoded = client.view.unit(id).unwrap();
            assert!((decoded.x - origin.x).abs() + (decoded.z - origin.z).abs() > 0.1);
            assert_eq!(client.view.state()["movement"], snapshot["movement"]);
            let outcome = snapshot["movement"]
                .as_array()
                .unwrap()
                .iter()
                .find(|row| row[0] == id)
                .unwrap();
            let tick = outcome[2].as_i64().unwrap();
            // Boundary controls retain the preceding clock, as in the pinned Source.
            assert!(tick >= before_move.tick);
            assert!(snapshot["tick"].as_i64().unwrap() > tick);
            tick
        });
        host.block_on(async {
            let hub = control.server.hub.lock().await;
            let native = hub.rooms["ABC"].native.as_ref().unwrap();
            let state = native.runtime.state();
            for (slot, &(id, origin, goal)) in selected.iter().enumerate() {
                let unit = &state.world.game.units[&id];
                assert_eq!(unit.owner, slot);
                assert_eq!(unit.world_goal, Some(goal));
                assert!(unit.amove.is_none());
                assert!(!unit.path.is_empty());
                assert!((unit.x - origin.x).abs() + (unit.z - origin.z).abs() > 0.1);
                assert_eq!(state.commands.units[&id].outcome, Some(MoveOutcome::Moving));
                assert_eq!(
                    state.commands.units[&id].outcome_tick,
                    Some(moving_ticks[slot])
                );
                assert!(state.world.game.tick > moving_ticks[slot]);
            }
            assert!(native.runtime.fault().is_none());
        });
        for (client, &(id, _, _)) in clients.iter_mut().zip(&selected) {
            client.send(json!({"t":"stop","ids":[id]}));
        }
        let stopped_ticks = std::array::from_fn::<_, 2, _>(|slot| {
            let id = selected[slot].0;
            let client = &mut clients[slot];
            let snapshot = client.until(|message| {
                message["t"] == "s"
                    && message["movement"].as_array().is_some_and(|rows| {
                        rows.iter()
                            .any(|row| row[0] == id && row[1] == "interrupted")
                    })
            });
            assert_eq!(client.view.state()["movement"], snapshot["movement"]);
            assert!(client.view.unit(id).is_some());
            let outcome = snapshot["movement"]
                .as_array()
                .unwrap()
                .iter()
                .find(|row| row[0] == id)
                .unwrap();
            let tick = outcome[2].as_i64().unwrap();
            assert!(tick > moving_ticks[slot]);
            assert!(snapshot["tick"].as_i64().unwrap() > tick);
            tick
        });
        host.block_on(async {
            let hub = control.server.hub.lock().await;
            let native = hub.rooms["ABC"].native.as_ref().unwrap();
            let state = native.runtime.state();
            for (slot, &(id, _, _)) in selected.iter().enumerate() {
                let unit = &state.world.game.units[&id];
                assert!(unit.path.is_empty());
                assert!(unit.world_goal.is_none() && unit.amove.is_none());
                let control = &state.commands.units[&id];
                assert!(control.orders.is_empty());
                assert_eq!(control.outcome, Some(MoveOutcome::Interrupted));
                assert_eq!(control.outcome_tick, Some(stopped_ticks[slot]));
                assert!(state.world.game.tick > stopped_ticks[slot]);
            }
            assert!(native.runtime.fault().is_none());
        });
        clients[0].send(json!({"t":"resync"}));
        let resync = clients[0].until(|message| message["t"] == "s" && message["all"] == true);
        let resync_tick = resync["tick"].as_u64().unwrap();
        let peer = clients[1].until(|message| {
            message["t"] == "s"
                && message["tick"]
                    .as_u64()
                    .is_some_and(|tick| tick >= resync_tick)
        });
        assert!(
            peer.get("all").is_none(),
            "resync must affect only the requesting seat"
        );
        for (slot, client) in clients.iter().enumerate() {
            assert_eq!(client.view.start().unwrap()["matchId"], epoch);
            assert!(!client.view.needs_resync());
            assert!(client.view.last_denial().is_none());
            for message in client.messages.iter().filter(|message| message["t"] == "s") {
                for row in message["movement"].as_array().unwrap() {
                    let id = u32::try_from(row[0].as_u64().unwrap()).unwrap();
                    assert_eq!(owners[&id], slot, "movement feedback is recipient-owned");
                }
            }
            let id = selected[slot].0;
            assert!(
                client.view.state()["movement"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|row| {
                        row[0] == id && row[1] == "interrupted" && row[2] == stopped_ticks[slot]
                    })
            );
        }
        let status = host.block_on(control.inspect("ABC")).unwrap();
        assert_eq!(status.epoch, epoch);
        assert!(status.tick > before_move.tick);
        assert!(!status.paused && status.fault.is_none());
    })
    .await
    .unwrap();
    let _ = stop.send(());
    server.await.unwrap().unwrap();
}
