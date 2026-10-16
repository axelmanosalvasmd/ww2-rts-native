//! Commands enter through the live Hub and execute on its canonical tick queue.
use super::*;
use ww2_core::{match_commands::Command, orders::OrderKey};

fn client(id: u64) -> (Connection, mpsc::Receiver<Output>) {
    let (tx, rx) = mpsc::channel(256);
    let (cancel, _) = watch::channel(false);
    (Connection::new(id, tx, cancel), rx)
}
fn join(hub: &mut Hub, connection: &Connection, token: &str, now: u64) {
    hub.message(
        "ABC",
        connection,
        &mut false,
        json!({"t":"hello","token":token,"name":token}),
        now,
    );
}
fn send(hub: &mut Hub, connection: &Connection, message: Value, now: u64) {
    hub.message("ABC", connection, &mut true, message, now);
}
fn drain(rx: &mut mpsc::Receiver<Output>) -> Vec<Value> {
    let mut messages = Vec::new();
    while let Ok(output) = rx.try_recv() {
        match output {
            Output::Json(value) => messages.push(value),
            Output::Snapshot(write) => messages.push(serde_json::from_str(&write.json).unwrap()),
            Output::Watcher(_) => panic!("unexpected spectator output for commander"),
            Output::Close(..) => panic!("unexpected close"),
        }
    }
    messages
}
fn room() -> (
    Hub,
    Connection,
    mpsc::Receiver<Output>,
    Connection,
    mpsc::Receiver<Output>,
) {
    let mut hub = Hub::new(32, String::new());
    let (a, mut ra) = client(1);
    let (b, mut rb) = client(2);
    join(&mut hub, &a, "a", 0);
    join(&mut hub, &b, "b", 0);
    hub.attach_native("ABC", native_tests::native_match(), 0)
        .unwrap();
    drain(&mut ra);
    drain(&mut rb);
    (hub, a, ra, b, rb)
}
fn state(hub: &Hub) -> &Match {
    hub.rooms["ABC"].native.as_ref().unwrap().runtime.state()
}
fn rifles(hub: &Hub, owner: usize) -> Vec<u32> {
    let state = state(hub);
    state
        .world
        .game
        .units
        .values()
        .filter(|unit| unit.owner == owner && state.rules.kinds[unit.kind].name == "rifle")
        .map(|unit| unit.id)
        .collect()
}
fn denies(messages: Vec<Value>) -> Vec<Value> {
    messages
        .into_iter()
        .filter(|message| message["t"] == "deny")
        .collect()
}

#[test]
fn intent_net43_buy_uses_host_keys_and_returns_resource_denial_after_real_purchase() {
    let (mut hub, _a, mut ra, b, mut rb) = room();
    let before = [rifles(&hub, 0), rifles(&hub, 1)];
    let wallet = state(&hub).start.economy.players[1].mp;
    let price = ww2_core::economy::price_of(state(&hub).options.mode, "rifle")
        .unwrap()
        .mp;
    // The constructor's ordinary funds permit one rifle but cannot pay for two.
    assert!(wallet >= price && wallet < price * 2.);
    for now in 0..2 {
        send(
            &mut hub,
            &b,
            json!({"t":"buy","unit":"rifle","slot":0,"issuer":0,"tick":999,"sequence":99}),
            now,
        );
    }
    assert_eq!(rifles(&hub, 1), before[1]);
    assert_eq!(state(&hub).start.economy.players[1].mp, wallet);
    let pending = state(&hub)
        .commands
        .pending_tick(1)
        .map(|(key, command)| (*key, command.clone()))
        .collect::<Vec<_>>();
    assert_eq!(
        pending,
        [1, 2]
            .map(|sequence| (
                OrderKey {
                    tick: 1,
                    issuer: 1,
                    sequence,
                },
                Command::Buy {
                    kind: "rifle".into(),
                    from: None,
                },
            ))
            .to_vec()
    );
    hub.poll_native(2, 0);
    let status = hub.native_status("ABC").unwrap();
    assert!(status.fault.is_none(), "{:?}", status.fault);
    assert_eq!(status.tick, 1);
    assert_eq!(state(&hub).commands.pending_commands(), 0);
    assert_eq!(rifles(&hub, 0), before[0]);
    assert_eq!(rifles(&hub, 1).len(), before[1].len() + 1);
    assert!(state(&hub).start.economy.players[1].mp < price);
    assert_eq!(
        denies(drain(&mut rb)),
        vec![json!({"t":"deny","cmd":"buy","reason":"mp"})]
    );
    assert!(denies(drain(&mut ra)).is_empty());
}

#[test]
fn intent_net43_malformed_full_payload_and_unknown_name_cannot_reserve_input() {
    let (mut hub, a, mut ra, _b, mut rb) = room();
    let before = rifles(&hub, 0);
    let count = 50.max(state(&hub).world.game.units.len()) + 1;
    let mut rows = vec![json!([before[0], 2, 3]); count];
    rows.push(json!([before[0], 2, "bad"]));
    send(
        &mut hub,
        &a,
        json!({"t":"buy","unit":"rifle","orders":rows}),
        0,
    );
    send(&mut hub, &a, json!({"t":"unrecognizedGameplay"}), 1);
    send(&mut hub, &a, json!({"t":"move"}), 1);
    assert_eq!(state(&hub).commands.pending_commands(), 0);
    assert_eq!(rifles(&hub, 0), before);
    assert_eq!(
        denies(drain(&mut ra)),
        vec![
            json!({"t":"deny","cmd":"buy","reason":"blocked"}),
            json!({"t":"deny","cmd":"unrecognizedGameplay","reason":"blocked"}),
            json!({"t":"deny","cmd":"move","reason":"blocked"}),
        ]
    );
    assert!(drain(&mut rb).is_empty());
    send(&mut hub, &a, json!({"t":"buy","unit":"rifle"}), 2);
    assert_eq!(
        state(&hub)
            .commands
            .pending_tick(1)
            .map(|(key, _)| *key)
            .collect::<Vec<_>>(),
        vec![OrderKey {
            tick: 1,
            issuer: 0,
            sequence: 1,
        }]
    );
    hub.poll_native(3, 0);
    assert!(hub.native_status("ABC").unwrap().fault.is_none());
    assert_eq!(rifles(&hub, 0).len(), before.len() + 1);
    assert!(denies(drain(&mut ra)).is_empty());
}

#[test]
fn intent_net43_paused_orders_are_ignored_and_replacement_keeps_stable_issuer_and_feedback() {
    let (mut hub, a, mut ra, b, mut rb) = room();
    send(&mut hub, &a, json!({"t":"pause"}), 0);
    assert!(hub.native_status("ABC").unwrap().paused);
    drain(&mut ra);
    drain(&mut rb);
    for message in [
        json!({"t":"buy","unit":"rifle"}),
        json!({"t":"unrecognizedGameplay"}),
    ] {
        send(&mut hub, &b, message, 1);
    }
    assert_eq!(state(&hub).commands.pending_commands(), 0);
    assert!(drain(&mut rb).is_empty());
    send(&mut hub, &a, json!({"t":"resume"}), 2);
    assert!(!hub.native_status("ABC").unwrap().paused);
    drain(&mut ra);
    drain(&mut rb);
    send(&mut hub, &b, json!({"t":"buy","unit":"rifle"}), 3);
    let (current, mut rr) = client(3);
    join(&mut hub, &current, "b", 4);
    assert!(matches!(rb.try_recv(), Ok(Output::Json(m)) if m["t"] == "replaced"));
    assert!(matches!(rb.try_recv(), Ok(Output::Close(..))));
    drain(&mut rr);
    drain(&mut ra);
    // A stale socket cannot reserve an order or disconnect the replacement.
    send(&mut hub, &b, json!({"t":"buy","unit":"rifle"}), 5);
    hub.disconnected("ABC", b.id, 6);
    assert_eq!(
        hub.rooms["ABC"].players[1].connection.as_ref().unwrap().id,
        current.id
    );
    assert_eq!(state(&hub).commands.pending_commands(), 1);
    send(
        &mut hub,
        &current,
        json!({"t":"buy","unit":"rifle","slot":0,"issuer":0,"tick":88,"sequence":88}),
        7,
    );
    assert_eq!(
        state(&hub)
            .commands
            .pending_tick(1)
            .map(|(key, _)| *key)
            .collect::<Vec<_>>(),
        vec![
            OrderKey {
                tick: 1,
                issuer: 1,
                sequence: 1,
            },
            OrderKey {
                tick: 1,
                issuer: 1,
                sequence: 2,
            },
        ]
    );
    hub.poll_native(8, 0);
    assert!(hub.native_status("ABC").unwrap().fault.is_none());
    assert_eq!(
        denies(drain(&mut rr)),
        vec![json!({"t":"deny","cmd":"buy","reason":"mp"})]
    );
    assert!(rb.try_recv().is_err());
    assert!(denies(drain(&mut ra)).is_empty());
}

#[test]
fn every_support_kind_keeps_ordinary_r1_scheduled_snapshots_flowing() {
    let catalog = ww2_core::catalog::Catalog::compiled().unwrap();
    for kind in catalog.table("SUPPORT").unwrap().object().unwrap().keys() {
        for trusted in [false, true] {
            let mut hub = Hub::new(32, String::new());
            if trusted {
                hub.configure_trusted_room(Some(
                    crate::TrustedRoomConfig::new("ABC", 3936341).unwrap(),
                ));
            }
            let (a, mut ra) = client(1);
            let (b, mut rb) = client(2);
            join(&mut hub, &a, "a", 0);
            join(&mut hub, &b, "b", 0);
            if trusted {
                hub.start_trusted("ABC", a.id, false, 0).unwrap();
            } else {
                hub.attach_native("ABC", native_tests::native_match(), 0)
                    .unwrap();
            }
            drain(&mut ra);
            drain(&mut rb);
            let native = hub.rooms.get_mut("ABC").unwrap().native.as_mut().unwrap();
            let setup = native.runtime.state_mut();
            *setup.start.players[0].support.get_mut(kind).unwrap() = 0.;
            let cost = ww2_core::economy::sup_cost(setup.options.mode, kind)
                .unwrap()
                .cost;
            let funds = cost.max(200.);
            setup.start.economy.players[0].mp = funds;
            send(
                &mut hub,
                &a,
                json!({"t":"support","kind":kind,"x":1,"z":2}),
                0,
            );
            assert_eq!(state(&hub).start.economy.players[0].mp, funds);
            let pending = state(&hub).commands.pending_commands();
            let replies = drain(&mut ra);
            let mut snapshots = [0; 2];
            for callback in 0..8 {
                hub.poll_native(callback * 50, callback * 50_000_000);
                let status = hub.native_status("ABC").unwrap();
                assert!(
                    status.fault.is_none(),
                    "kind={kind}, trusted={trusted}: {:?}",
                    status.fault
                );
                assert_eq!(status.tick, callback as i64 + 1);
                for (slot, rx) in [&mut ra, &mut rb].into_iter().enumerate() {
                    while let Ok(output) = rx.try_recv() {
                        let Output::Snapshot(write) = output else {
                            panic!("kind={kind}, trusted={trusted}: {output:?}");
                        };
                        let message: Value = serde_json::from_str(&write.json).unwrap();
                        assert_eq!(message["t"], "s");
                        assert!(message["strikes"].is_array());
                        snapshots[slot] += 1;
                        hub.complete_snapshot(&write, true, callback * 50);
                    }
                }
            }
            assert!(snapshots.iter().all(|count| *count >= 2), "{snapshots:?}");
            assert_eq!(state(&hub).commands.pending_commands(), 0);
            // Supported calls must create real state; refusal cannot pass by poverty.
            if matches!(kind.as_str(), "artillery" | "smoke" | "cover") {
                assert_eq!(pending, 1);
                assert!(replies.is_empty());
                assert_eq!(state(&hub).world.game.strikes.len(), 1);
                assert!(state(&hub).start.players[0].support[kind] > 0.);
            } else {
                assert_eq!(pending, 0);
                assert_eq!(
                    replies,
                    vec![json!({"t":"deny","cmd":"support","reason":"unsupported"})]
                );
                assert!(state(&hub).world.game.strikes.is_empty());
                assert!(state(&hub).start.players[0].support[kind] <= 0.);
                let actor = rifles(&hub, 0)[0];
                send(&mut hub, &a, json!({"t":"stop","ids":[actor]}), 400);
                assert_eq!(
                    state(&hub)
                        .commands
                        .pending_tick(9)
                        .next()
                        .unwrap()
                        .0
                        .sequence,
                    1
                );
            }
            assert!(
                hub.accounts
                    .values()
                    .all(|account| account.pressure.inflight() == 0)
            );
        }
    }
}

#[test]
fn recon_refuses_before_reservation_and_keeps_ordinary_r1_snapshots_flowing() {
    for trusted in [false, true] {
        let mut hub = Hub::new(32, String::new());
        if trusted {
            hub.configure_trusted_room(Some(
                crate::TrustedRoomConfig::new("ABC", 3936341).unwrap(),
            ));
        }
        let (a, mut ra) = client(1);
        let (b, mut rb) = client(2);
        join(&mut hub, &a, "a", 0);
        join(&mut hub, &b, "b", 0);
        if trusted {
            hub.start_trusted("ABC", a.id, false, 0).unwrap();
        } else {
            hub.attach_native("ABC", native_tests::native_match(), 0)
                .unwrap();
        }
        drain(&mut ra);
        drain(&mut rb);
        let actor = rifles(&hub, 0)[0];
        let at = state(&hub).world.game.units[&actor].at();
        let wallet = state(&hub).start.economy.players[0].mp;
        send(
            &mut hub,
            &a,
            json!({"t":"support","kind":"recon","x":at.x,"z":at.z}),
            0,
        );
        assert_eq!(state(&hub).commands.pending_commands(), 0);
        assert_eq!(state(&hub).start.economy.players[0].mp, wallet);
        assert!(state(&hub).world.game.strikes.is_empty());
        assert!(state(&hub).commands.units[&actor].orders.is_empty());
        assert_eq!(
            drain(&mut ra),
            vec![json!({"t":"deny","cmd":"support","reason":"unsupported"}),]
        );
        // A supported input still receives the first host sequence after refusal.
        send(&mut hub, &a, json!({"t":"stop","ids":[actor]}), 0);
        assert_eq!(
            state(&hub)
                .commands
                .pending_tick(1)
                .next()
                .unwrap()
                .0
                .sequence,
            1
        );
        let mut snapshots = [0; 2];
        for callback in 0..8 {
            hub.poll_native(callback * 50, callback * 50_000_000);
            for (slot, rx) in [&mut ra, &mut rb].into_iter().enumerate() {
                while let Ok(output) = rx.try_recv() {
                    match output {
                        Output::Snapshot(write) => {
                            let message: Value = serde_json::from_str(&write.json).unwrap();
                            assert_eq!(message["t"], "s");
                            assert!(message["units"].is_array());
                            snapshots[slot] += 1;
                            hub.complete_snapshot(&write, true, callback * 50);
                        }
                        other => panic!("unexpected output: {other:?}"),
                    }
                }
            }
            let status = hub.native_status("ABC").unwrap();
            assert!(
                status.fault.is_none(),
                "trusted={trusted}: {:?}",
                status.fault
            );
            assert_eq!(status.tick, callback as i64 + 1);
        }
        assert!(snapshots.iter().all(|count| *count >= 2), "{snapshots:?}");
        assert_eq!(state(&hub).commands.pending_commands(), 0);
        assert!(
            hub.accounts
                .values()
                .all(|account| account.pressure.inflight() == 0)
        );
    }
}
