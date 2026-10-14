//! Owned socket input after an actual disconnect and same-seat reconnect.
use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn real_socket_drop_reconnect_resumes_owned_move_in_same_match() {
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
        let mut a = Receiver::connect(address, "a");
        let mut b = Receiver::connect(address, "b");
        let mut state = setup("default");
        // Declared quiet initial controls. Later gameplay changes use the socket.
        for unit in state.world.game.units.values_mut() {
            unit.hold_pos = true;
        }
        let expected = SetupExpected::new(&state);
        let owned = state
            .world
            .game
            .units
            .values()
            .find(|unit| unit.owner == 0 && state.rules.kinds[unit.kind].name == "rifle")
            .unwrap()
            .id;
        let origin = state.world.game.units[&owned].at();
        let goal = Point {
            x: origin.x,
            z: origin.z + 12.,
        };
        let epoch = host.block_on(control.attach("ABC", state)).unwrap();
        expected.check(&a.until(|message| message["t"] == "start"), 0, epoch);
        expected.check(&b.until(|message| message["t"] == "start"), 1, epoch);
        a.until(|message| message["t"] == "s");
        b.until(|message| message["t"] == "s");
        let connection = host.block_on(async {
            let hub = control.server.hub.lock().await;
            hub.rooms["ABC"].players[0].connection.as_ref().unwrap().id
        });

        // EOF on the only socket for a precedes any reconnect attempt.
        drop(a);
        let dropped = b.until(|message| message["t"] == "pause" && message["paused"] == true);
        assert_eq!(dropped["reason"], "drop");
        assert_eq!(dropped["by"], "a");
        let paused = host.block_on(control.inspect("ABC")).unwrap();
        assert_eq!(paused.epoch, epoch);
        assert!(paused.paused && paused.fault.is_none());
        host.block_on(async {
            let hub = control.server.hub.lock().await;
            let room = &hub.rooms["ABC"];
            assert_eq!(room.players.len(), 2);
            assert_eq!(
                room.players[0].token,
                "a".encode_utf16().collect::<Vec<_>>()
            );
            assert!(room.players[0].connection.is_none());
            assert!(room.players[0].disconnected.is_some());
            assert_eq!(
                room.players[1].token,
                "b".encode_utf16().collect::<Vec<_>>()
            );
            assert!(room.players[1].connection.is_some());
        });

        let mut reconnected = Receiver::open(address, "a");
        let start = reconnected.receive();
        expected.check(&start, 0, epoch);
        let pause = reconnected.receive();
        assert_eq!(pause["t"], "pause");
        assert_eq!(pause["paused"], true);
        assert_eq!(pause["reason"], "drop");
        assert_eq!(pause["by"], "a");
        let resume = reconnected.receive();
        assert_eq!(resume["t"], "pause");
        assert_eq!(resume["paused"], false);
        let lobby = reconnected.receive();
        assert_eq!(lobby["t"], "lobby");
        assert_eq!(lobby["state"], "play");
        assert_eq!(lobby["you"], 0);
        assert_eq!(reconnected.view.start(), Some(&start));
        assert_eq!(reconnected.view.pause(), Some(&resume));
        assert_eq!(reconnected.view.unit_count(), 0);
        assert!(!reconnected.view.needs_resync());
        assert_eq!(
            b.until(|message| message["t"] == "pause" && message["paused"] == false),
            resume
        );
        assert_eq!(b.receive()["t"], "lobby");

        let baseline = reconnected.until(|message| message["t"] == "s");
        assert!(baseline.get("all").is_none());
        assert_eq!(baseline["online"], json!([true, true]));
        assert_eq!(
            baseline["units"].as_array().unwrap().len(),
            reconnected.view.unit_count(),
            "fresh socket receives its complete primary unit baseline"
        );
        let before = reconnected.view.unit(owned).unwrap().clone();
        assert_eq!(before.owner, 0);
        let baseline_tick = baseline["tick"].as_u64().unwrap();
        host.block_on(async {
            let hub = control.server.hub.lock().await;
            let room = &hub.rooms["ABC"];
            assert_ne!(room.players[0].connection.as_ref().unwrap().id, connection);
            assert!(room.players[0].disconnected.is_none());
            let native = room.native.as_ref().unwrap();
            assert_eq!(native.epoch, epoch);
            assert!(!native.runtime.paused());
            let unit = &native.runtime.state().world.game.units[&owned];
            assert!(unit.world_goal.is_none() && unit.amove.is_none() && unit.path.is_empty());
        });

        reconnected.send(json!({"t":"move","orders":[[owned,goal.x,goal.z]]}));
        let moved = reconnected.until(|message| {
            message["t"] == "s"
                && message["movement"].as_array().is_some_and(|rows| {
                    rows.iter().any(|row| row[0] == owned && row[1] == "moving")
                })
                && message["units"].as_array().unwrap().iter().any(|row| {
                    row[0] == owned
                        && ((row[3].as_f64().unwrap() - before.x).abs()
                            + (row[4].as_f64().unwrap() - before.z).abs())
                            > 0.1
                })
        });
        assert!(moved["tick"].as_u64().unwrap() > baseline_tick);
        let decoded = reconnected.view.unit(owned).unwrap();
        assert_eq!(decoded.owner, 0);
        assert!((decoded.x - before.x).abs() + (decoded.z - before.z).abs() > 0.1);
        assert_eq!(reconnected.view.state()["movement"], moved["movement"]);
        assert!(reconnected.view.last_denial().is_none());
        assert!(!reconnected.view.needs_resync());
        host.block_on(async {
            let hub = control.server.hub.lock().await;
            let native = hub.rooms["ABC"].native.as_ref().unwrap();
            let state = native.runtime.state();
            let unit = &state.world.game.units[&owned];
            assert_eq!(unit.owner, 0);
            assert_eq!(unit.world_goal, Some(goal));
            assert!(unit.amove.is_none() && !unit.path.is_empty());
            assert!((unit.x - origin.x).abs() + (unit.z - origin.z).abs() > 0.1);
            assert_eq!(
                state.commands.units[&owned].outcome,
                Some(ww2_core::match_commands::MoveOutcome::Moving)
            );
            assert!(state.world.game.tick as u64 >= moved["tick"].as_u64().unwrap());
            assert_eq!(native.epoch, epoch);
            assert!(!native.runtime.paused());
            assert!(native.runtime.fault().is_none());
        });
    })
    .await
    .unwrap();
    let _ = stop.send(());
    server.await.unwrap().unwrap();
}
