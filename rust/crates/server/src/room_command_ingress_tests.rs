//! Room routing binds each new command to the current seated connection.
use super::*;
use ww2_core::{match_commands::Command, orders::OrderKey};

fn drain(hub: &mut Hub, rx: &mut mpsc::Receiver<Output>, now: u64) -> Vec<Value> {
    let mut messages = Vec::new();
    while let Ok(out) = rx.try_recv() {
        match out {
            Output::Json(v) => messages.push(v),
            Output::Snapshot(write) => {
                messages.push(serde_json::from_str(&write.json).unwrap());
                hub.complete_snapshot(&write, true, now);
            }
            Output::Watcher(_) => panic!("watcher output reached a seated commander"),
            Output::Close(..) => panic!("unexpected close"),
        }
    }
    messages
}

#[test]
fn net43_room_routes_fireat_and_convoys_with_guest_keys_and_exact_denials() {
    for (raw, expected) in [
        (json!({"t":"fireat","ids":[],"x":1e99,"z":0}), "blocked"),
        (json!({"t":"supply","ids":[],"target":1}), "blocked"),
        (json!({"t":"logisticsResume","ids":[]}), "blocked"),
    ] {
        let mut hub = Hub::new(32, String::new());
        let mut peers = Vec::new();
        let mut receivers = Vec::new();
        for slot in 0..2 {
            let (tx, rx) = mpsc::channel(64);
            let (cancel, _) = watch::channel(false);
            let peer = Connection::new(slot + 1, tx, cancel);
            hub.message(
                "ABC",
                &peer,
                &mut false,
                json!({"t":"hello","token":format!("seat{slot}"),"name":"seat"}),
                0,
            );
            peers.push(peer);
            receivers.push(rx);
        }
        hub.attach_native("ABC", native_tests::native_match(), 0)
            .unwrap();
        for rx in &mut receivers {
            drain(&mut hub, rx, 0);
        }
        let mut forged = raw.clone();
        forged["issuer"] = json!(0);
        forged["slot"] = json!(0);
        forged["tick"] = json!(999);
        forged["sequence"] = json!(999);
        // Paused rooms ignore gameplay without reserving a queue identity.
        hub.message("ABC", &peers[0], &mut true, json!({"t":"pause"}), 1);
        hub.message("ABC", &peers[1], &mut true, forged.clone(), 2);
        assert_eq!(
            hub.rooms["ABC"]
                .native
                .as_ref()
                .unwrap()
                .runtime
                .state()
                .commands
                .pending_commands(),
            0
        );
        hub.message("ABC", &peers[0], &mut true, json!({"t":"resume"}), 3);
        for rx in &mut receivers {
            drain(&mut hub, rx, 3);
        }
        hub.message("ABC", &peers[1], &mut true, forged, 4);
        let state = hub.rooms["ABC"].native.as_ref().unwrap().runtime.state();
        let queued = state.commands.pending_tick(1).collect::<Vec<_>>();
        assert_eq!(queued.len(), 1);
        assert_eq!(
            *queued[0].0,
            OrderKey {
                tick: 1,
                issuer: 1,
                sequence: 1
            }
        );
        match raw["t"].as_str().unwrap() {
            "fireat" => assert!(matches!(queued[0].1, Command::FireAt { .. })),
            "supply" => assert!(matches!(queued[0].1, Command::Supply { .. })),
            "logisticsResume" => assert!(matches!(queued[0].1, Command::LogisticsResume { .. })),
            _ => unreachable!(),
        }
        for tick in 0..4 {
            hub.poll_native(5 + tick * 50, tick * 50_000_000);
            assert!(hub.native_status("ABC").unwrap().fault.is_none());
        }
        let host = drain(&mut hub, &mut receivers[0], 200);
        assert!(!host.iter().any(|v| v["t"] == "deny"));
        let guest = drain(&mut hub, &mut receivers[1], 200);
        let denials = guest
            .iter()
            .filter(|v| v["t"] == "deny")
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(
            denials,
            vec![json!({"t":"deny","cmd":raw["t"],"reason":expected})]
        );
        assert!(host.iter().any(|v| v["t"] == "s"));
        assert!(guest.iter().any(|v| v["t"] == "s"));
    }
}
