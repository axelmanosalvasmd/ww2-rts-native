//! Public R1 inputs over real sockets, with explicit maintenance deadlines.
use super::*;
use ww2_core::recipient_snapshot::SnapshotCache;

fn flush(client: &mut Receiver) -> Vec<Value> {
    let from = client.messages.len();
    barrier(client);
    client.messages[from..].to_vec()
}
fn submit(rig: &mut Rig, client: &mut Receiver, command: Value) -> Vec<Value> {
    // Denial rate limits use an explicit host clock, never a wall-clock sleep.
    rig.now_ms
        .fetch_add(1_001, std::sync::atomic::Ordering::SeqCst);
    client.send(command);
    let mut messages = flush(client);
    rig.step();
    rig.step();
    messages.extend(flush(client));
    rig.change(|hub| {
        let state = hub.rooms["ABC"].native.as_ref().unwrap().runtime.state();
        let cache = SnapshotCache::new(state).unwrap();
        for slot in 0..state.world.game.players.len() {
            cache.recipient(slot).unwrap();
        }
    });
    messages
}
fn accepted(messages: &[Value]) {
    assert!(!messages.iter().any(|m| m["t"] == "deny"), "{messages:?}");
}
fn denied(messages: &[Value], command: &str, reason: &str) {
    let actual = messages
        .iter()
        .filter(|m| m["t"] == "deny")
        .collect::<Vec<_>>();
    assert_eq!(actual, [&json!({"t":"deny","cmd":command,"reason":reason})]);
}
fn owned(rig: &Rig, slot: usize, kind: &str) -> u32 {
    rig.change(|hub| {
        let state = hub.rooms["ABC"].native.as_ref().unwrap().runtime.state();
        state
            .world
            .game
            .units
            .values()
            .find(|u| u.owner == slot && state.rules.kinds[u.kind].name == kind)
            .unwrap()
            .id
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn r1_host_pause_freezes_commands_and_ticks_and_only_host_resumes_or_ends() {
    sockets(|rig, mut clients| {
        let rifle = owned(rig, 0, "rifle");
        denied(
            &submit(rig, &mut clients[1], json!({"t":"pause"})),
            "pause",
            "blocked",
        );
        clients[0].send(json!({"t":"pause"}));
        let paused = json!({"t":"pause","paused":true,"by":"a","reason":"host","left":0});
        for client in &mut clients {
            assert_eq!(client.until(|m| m["t"] == "pause"), paused);
        }
        let before = rig.change(|hub| hub.native_status("ABC").unwrap().tick);
        let paused_at = rig.now_ms.load(std::sync::atomic::Ordering::SeqCst);
        for (index, command) in [
            json!({"t":"pause"}),
            json!({"t":"stop","ids":[rifle]}),
            json!({"t":"ability","ids":[rifle]}),
        ]
        .into_iter()
        .enumerate()
        {
            clients[0].send(command);
            clients[1].send(json!({"t":"resume"}));
            for client in &mut clients {
                assert!(
                    !flush(client)
                        .iter()
                        .any(|m| m["t"] == "deny" || m["t"] == "pause")
                );
            }
            rig.poll(paused_at + (index as u64 + 1) * 1_000, rig.next_ns);
            rig.next_ns += 50_000_000;
            let status = rig.change(|hub| hub.native_status("ABC").unwrap());
            assert!(status.paused && status.fault.is_none());
            assert_eq!(status.tick, before);
            for client in &mut clients {
                assert_eq!(client.until(|m| m["t"] == "pause"), paused);
                assert_eq!(client.until(|m| m["t"] == "s")["tick"], before);
            }
        }
        rig.change(|hub| {
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
            )
        });
        clients[0].send(json!({"t":"resume"}));
        for client in &mut clients {
            assert_eq!(
                client.until(|m| m["t"] == "pause"),
                json!({"t":"pause","paused":false,"by":"","reason":"host","left":0})
            );
        }
        denied(
            &submit(rig, &mut clients[1], json!({"t":"resume"})),
            "resume",
            "blocked",
        );
        denied(
            &submit(rig, &mut clients[1], json!({"t":"end"})),
            "end",
            "blocked",
        );
        accepted(&submit(
            rig,
            &mut clients[0],
            json!({"t":"stop","ids":[rifle]}),
        ));
        assert!(rig.change(|hub| hub.native_status("ABC").unwrap().tick) > before);
        // End remains available while paused, and carries no previous story/outcome.
        clients[0].send(json!({"t":"pause"}));
        clients[0].until(|m| m["t"] == "pause");
        clients[0].send(json!({"t":"end"}));
        for client in &mut clients {
            let lobby = client.until(|m| m["t"] == "lobby" && m["state"] == "lobby");
            assert_eq!(lobby["result"], json!({"ended":true,"you":null}));
        }
        rig.change(|hub| {
            assert!(hub.rooms["ABC"].native.is_none());
            assert!(hub.native_status("ABC").unwrap().fault.is_none());
        });
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn r1_safe_core_commands_reach_their_prototype_denials_in_an_ordinary_room() {
    sockets(|rig, mut clients| {
        // Invalid selections and out-of-map targets make exact denial branches stable.
        let cases = [
            (json!({"t":"nonsense"}), "blocked"),
            (json!({"t":"move","orders":[]}), "blocked"),
            (json!({"t":"amove","orders":[]}), "blocked"),
            (
                json!({"t":"garrison","ids":[],"x":1000000000,"z":1000000000}),
                "blocked",
            ),
            (json!({"t":"dig","ids":[],"x":1,"z":1}), "noBuilders"),
            (
                json!({"t":"entrench","ids":[],"x":1,"z":1,"pattern":"line"}),
                "noBuilders",
            ),
            (json!({"t":"cover","ids":[]}), "needs"),
            (json!({"t":"assist","ids":[],"id":-1}), "unseen"),
            (json!({"t":"retreat","ids":[]}), "needs"),
            (json!({"t":"rally","x":-1,"z":-1}), "blocked"),
            (json!({"t":"tech","kind":"not-a-tech"}), "blocked"),
            (json!({"t":"cancel","id":-1}), "unseen"),
            (json!({"t":"cancelProduction","id":-1,"job":1}), "needs"),
            (json!({"t":"destroy","ids":[]}), "unseen"),
        ];
        for (command, reason) in cases {
            let name = command["t"].as_str().unwrap().to_owned();
            denied(&submit(rig, &mut clients[0], command), &name, reason);
            flush(&mut clients[1]);
        }
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn r1_resync_is_recipient_local_and_replaces_corrupted_rows() {
    sockets(|rig, mut clients| {
        for _ in 0..3 {
            rig.step();
        }
        for client in &mut clients {
            flush(client);
        }
        // The real server keeps authoritative history. Corrupt only one consumer.
        let id = owned(rig, 0, "rifle");
        let tick = clients[0].view.state()["tick"].clone();
        clients[0]
            .view
            .apply(&json!({"t":"s","tick":tick,"gone":[id],"units":[],"held":[0,0]}).to_string())
            .unwrap();
        assert!(clients[0].view.needs_resync());
        clients[0].send(json!({"t":"resync"}));
        flush(&mut clients[0]);
        for _ in 0..2 {
            rig.step();
        }
        let full = clients[0].until(|m| m["t"] == "s" && m["all"] == true);
        assert!(!clients[0].view.needs_resync());
        assert!(clients[0].view.unit(id).is_some());
        let peer = clients[1].until(|m| m["t"] == "s" && m["tick"] == full["tick"]);
        assert!(peer.get("all").is_none());
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn r1_public_commands_finish_an_ordinary_match_with_every_seat_snapshot_each_tick() {
    sockets(|rig, mut clients| {
        rig.capture_each_tick = true;
        let rifle = owned(rig, 0, "rifle");
        accepted(&submit(
            rig,
            &mut clients[0],
            json!({"t":"stance","ids":[rifle],"key":"holdFire","on":true}),
        ));
        flush(&mut clients[1]);
        accepted(&submit(
            rig,
            &mut clients[0],
            json!({"t":"stance","ids":[rifle],"key":"holdPos","on":false}),
        ));
        flush(&mut clients[1]);
        let destination = rig.change(|hub| {
            let state = hub.rooms["ABC"].native.as_ref().unwrap().runtime.state();
            let u = &state.world.game.units[&rifle];
            Point {
                x: u.x,
                z: u.z + 8.,
            }
        });
        accepted(&submit(
            rig,
            &mut clients[0],
            json!({"t":"move","orders":[[rifle,destination.x,destination.z]]}),
        ));
        flush(&mut clients[1]);
        accepted(&submit(
            rig,
            &mut clients[0],
            json!({"t":"stop","ids":[rifle]}),
        ));
        flush(&mut clients[1]);
        let ids = rig.change(|hub| {
            hub.rooms["ABC"]
                .native
                .as_ref()
                .unwrap()
                .runtime
                .state()
                .world
                .game
                .units
                .values()
                .filter(|u| u.owner == 1)
                .map(|u| u.id)
                .collect::<Vec<_>>()
        });
        clients[1].send(json!({"t":"destroy","ids":ids}));
        flush(&mut clients[1]);
        // Scuttling clears the opposition; Conquest still requires point capture and VP.
        // All later positions and ownership changes follow these public move orders.
        let orders = rig.change(|hub| {
            let state = hub.rooms["ABC"].native.as_ref().unwrap().runtime.state();
            let infantry = state
                .world
                .game
                .units
                .values()
                .filter(|u| u.owner == 0 && state.rules.kinds[u.kind].infantry)
                .map(|u| u.id)
                .collect::<Vec<_>>();
            state
                .start
                .points
                .iter()
                .zip(infantry)
                .map(|(point, id)| (id, point.at))
                .collect::<Vec<_>>()
        });
        assert!(!orders.is_empty());
        for (id, point) in orders {
            accepted(&submit(
                rig,
                &mut clients[0],
                json!({"t":"move","orders":[[id,point.x,point.z]]}),
            ));
            flush(&mut clients[1]);
        }
        for client in &clients {
            client.socket.set_nodelay(true).unwrap();
        }
        let mut result = false;
        for iteration in 0..40_000 {
            rig.step();
            // Drain normal socket traffic in bounded batches. Per-tick recipient
            // projection above stays independent of delivery acknowledgements.
            let finished = rig.change(|hub| hub.rooms["ABC"].native.is_none());
            if iteration % 20 != 19 && !finished {
                continue;
            }
            for client in &mut clients {
                let messages = flush(client);
                accepted(&messages);
                if let Some(lobby) = messages
                    .iter()
                    .find(|m| m["t"] == "lobby" && m["state"] == "lobby")
                {
                    let report = &lobby["result"];
                    assert_eq!(report["winner"], 0);
                    assert_eq!(report["teams"], json!([0, 1]));
                    assert_eq!(report["names"], json!(["a", "b"]));
                    assert_eq!(report["mode"], "conquest");
                    assert_eq!(report["reason"], "vp");
                    assert!(report["at"]["x"].is_number() && report["at"]["z"].is_number());
                    assert!(report["endTick"].is_number());
                    assert_eq!(report["story"].as_array().unwrap().len(), 2);
                    assert!(report["timeline"].is_array() && report["winVp"].is_number());
                    let you = client.view.start().unwrap()["you"].as_u64().unwrap();
                    assert_eq!(
                        report["you"],
                        json!({"team":you,"outcome":if you==0 {"victory"} else {"defeat"}})
                    );
                    result = true;
                }
            }
            if result {
                break;
            }
        }
        let snapshots = rig.captured_seats;
        assert!(
            result && snapshots >= 120,
            "ordinary public match did not complete: result={result}, snapshots={snapshots}"
        );
        rig.change(|hub| {
            let status = hub.native_status("ABC").unwrap();
            assert!(status.ending_complete && status.fault.is_none());
            eprintln!(
                "R1_PUBLIC_MATCH tick={} recipient_snapshots={snapshots}",
                status.tick
            );
        });
        // A later host end cannot reuse the previous match's per-seat outcome.
        clients[0].send(json!({"t":"start"}));
        for client in &mut clients {
            client.until(|m| m["t"] == "start");
        }
        clients[0].send(json!({"t":"end"}));
        for client in &mut clients {
            assert_eq!(
                client.until(|m| m["t"] == "lobby" && m["state"] == "lobby")["result"],
                json!({"ended":true,"you":null})
            );
        }
    })
    .await;
}
