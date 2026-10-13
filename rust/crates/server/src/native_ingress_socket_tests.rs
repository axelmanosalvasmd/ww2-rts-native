//! Real socket inputs, ordinary decode and canonical runner outcomes.
use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn socket_controls_use_owned_slot_and_survive_same_token_takeover() {
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
        // Declared quiet initial controls only. Every later control change is a socket input.
        for unit in state.world.game.units.values_mut() {
            unit.hold_pos = true;
        }
        let owned = state
            .world
            .game
            .units
            .values()
            .find(|u| u.owner == 0 && state.rules.kinds[u.kind].name == "rifle")
            .unwrap()
            .id;
        let foreign = state
            .world
            .game
            .units
            .values()
            .find(|u| u.owner == 1 && state.rules.kinds[u.kind].name == "rifle")
            .unwrap()
            .id;
        let origin = state.world.game.units[&owned].at();
        let goal = Point {
            x: origin.x,
            z: origin.z + 12.,
        };
        let epoch = host.block_on(control.attach("ABC", state)).unwrap();
        assert_eq!(a.until(|m| m["t"] == "start")["matchId"], epoch);
        b.until(|m| m["t"] == "start");
        a.until(|m| m["t"] == "s");
        a.send(json!({"t":"stance","ids":[owned],"key":"holdFire","on":true,"slot":1}));
        a.until(|m| {
            m["t"] == "s"
                && m["units"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|row| row[0] == owned && row[12].as_u64().unwrap() & 2048 != 0)
        });
        assert_ne!(a.view.unit(owned).unwrap().flags & 2048, 0);
        a.send(
            json!({"t":"amove","orders":[[owned,goal.x,goal.z]],"slot":1,"tick":777,"sequence":0}),
        );
        a.until(|m| {
            m["t"] == "s"
                && m["units"].as_array().unwrap().iter().any(|row| {
                    row[0] == owned
                        && ((row[3].as_f64().unwrap() - origin.x).abs()
                            + (row[4].as_f64().unwrap() - origin.z).abs())
                            > 0.1
                })
        });
        let moving = a.view.unit(owned).unwrap();
        assert!((moving.x - origin.x).abs() + (moving.z - origin.z).abs() > 0.1);
        host.block_on(async {
            let hub = control.server.hub.lock().await;
            let unit = &hub.rooms["ABC"]
                .native
                .as_ref()
                .unwrap()
                .runtime
                .state()
                .world
                .game
                .units[&owned];
            assert!(unit.world_goal.is_none());
            assert!(
                unit.amove.is_some(),
                "the decoded pose follows the actual admitted attack-move intent"
            );
        });
        a.send(json!({"t":"stop","ids":[owned]}));
        a.until(|m| {
            m["t"] == "s"
                && m["movement"].as_array().is_some_and(|rows| {
                    rows.iter()
                        .any(|row| row[0] == owned && row[1] == "interrupted")
                })
        });
        host.block_on(async {
            let hub = control.server.hub.lock().await;
            let native = hub.rooms["ABC"].native.as_ref().unwrap();
            let state = native.runtime.state();
            assert!(state.world.game.units[&owned].path.is_empty());
            assert!(state.world.game.units[&owned].world_goal.is_none());
            assert!(state.commands.units[&owned].hold_fire);
            assert!(native.runtime.fault().is_none());
        });
        // Unit IDs remain source-selected by the actual issuer, ignoring forged keys.
        a.send(json!({"t":"stop","ids":[foreign],"slot":1,"issuer":1,"tick":999}));
        let deny = a.until(|m| m["t"] == "deny");
        assert_eq!(deny, json!({"t":"deny","cmd":"stop","reason":"needs"}));
        a.send(json!({"t":"move","orders":[[owned,goal.x,"bad"]]}));
        assert_eq!(
            a.until(|m| m["t"] == "deny"),
            json!({"t":"deny","cmd":"move","reason":"blocked"})
        );
        a.send(json!({"t":"attack","ids":[owned],"target":foreign}));
        assert_eq!(a.until(|m| m["t"] == "deny")["reason"], "blocked");
        let mut replacement = Receiver::open(address, "a");
        let start = replacement.until(|m| m["t"] == "start");
        assert_eq!(start["matchId"], epoch);
        assert_eq!(start["you"], 0);
        assert_ne!(replacement.view.unit(owned).unwrap().flags & 2048, 0);
        // A replaced socket's last attempted bytes cannot acquire the new seat.
        let stale = json!({"t":"stance","ids":[owned],"key":"holdFire","on":false})
            .to_string()
            .into_bytes();
        let mask = [7_u8, 11, 17, 23];
        let mut frame = vec![0x81, 0x80 | stale.len() as u8];
        frame.extend(mask);
        frame.extend(stale.iter().enumerate().map(|(i, byte)| byte ^ mask[i % 4]));
        let _ = a.socket.write_all(&frame);
        a.until(|m| m["t"] == "replaced");
        replacement.send(json!({"t":"move","orders":[[owned,goal.x,goal.z]],"slot":1}));
        replacement.until(|m| {
            m["t"] == "s"
                && m["movement"].as_array().is_some_and(|rows| {
                    rows.iter().any(|row| row[0] == owned && row[1] == "moving")
                })
        });
        replacement.send(json!({"t":"stop","ids":[owned]}));
        replacement.until(|m| {
            m["t"] == "s"
                && m["movement"].as_array().is_some_and(|rows| {
                    rows.iter()
                        .any(|row| row[0] == owned && row[1] == "interrupted")
                })
        });
        assert!(!replacement.view.needs_resync());
        assert_ne!(replacement.view.unit(owned).unwrap().flags & 2048, 0);
        let status = host.block_on(control.inspect("ABC")).unwrap();
        assert_eq!(status.epoch, epoch);
        assert!(status.fault.is_none());
    })
    .await
    .unwrap();
    let _ = stop.send(());
    server.await.unwrap().unwrap();
}
