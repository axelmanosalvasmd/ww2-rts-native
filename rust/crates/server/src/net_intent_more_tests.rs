//! Additional behavior witnesses using the production Hub, decoder and recipient producers.
use super::*;
use ww2_core::client_view::{Applied, ClientView};
use ww2_core::recipient_snapshot::DeliveryState;

fn drain(peer: &mut Peer, hub: &mut Hub, now: u64) -> Vec<Value> {
    let mut messages = Vec::new();
    while let Ok(output) = peer.output.try_recv() {
        match output {
            Output::Json(message) => messages.push(message),
            Output::Snapshot(write) => {
                messages.push(serde_json::from_str(&write.json).unwrap());
                hub.complete_snapshot(&write, true, now);
            }
            other => panic!("unexpected output {other:?}"),
        }
    }
    messages
}

#[test]
fn client_rejects_malformed_or_non_string_tags_without_changing_state() {
    let mut view = ClientView::new();
    for live in [false, true] {
        if live {
            assert_eq!(
                view.apply(r#"{"t":"lobby","players":[]}"#).unwrap(),
                Applied::Lobby
            );
        }
        let before = view.clone();
        for raw in [
            "{",
            "null",
            "[]",
            "false",
            "17",
            r#""text""#,
            "{}",
            r#"{"t":9}"#,
        ] {
            assert!(view.apply(raw).is_err(), "{raw}");
            assert_eq!(view, before);
        }
        assert_eq!(view.apply(r#"{"t":"unknown"}"#).unwrap(), Applied::Ignored);
        assert_eq!(view, before);
    }
}

#[test]
fn directory_counts_reserved_seats_and_hides_empty_or_full_rooms() {
    let mut hub = Hub::new(32, String::new());
    let mut peers = (1..=3).map(Peer::new).collect::<Vec<_>>();
    peers[0].send(
        &mut hub,
        "ABC",
        json!({"t":"hello","token":"a","name":"A","listing":{"public":true}}),
        0,
    );
    peers[1].hello(&mut hub, "ABC", "b", "B", false);
    peers[2].hello(&mut hub, "ABC", "c", "C", false);
    let row = hub.directory()["rooms"][0].clone();
    assert_eq!(row["humans"], 3);
    assert_eq!(row["occupied"], 3);
    assert_eq!(row["capacity"], 3);
    assert_eq!(row["joinable"], false);
    hub.disconnected("ABC", 3, 100);
    let row = hub.directory()["rooms"][0].clone();
    assert_eq!(row["humans"], 2);
    assert_eq!(row["occupied"], 3);
    assert_eq!(row["joinable"], false);
    hub.expire(10_100);
    assert_eq!(hub.directory()["rooms"][0]["joinable"], true);
    hub.disconnected("ABC", 1, 10_101);
    hub.disconnected("ABC", 2, 10_101);
    hub.expire(20_101);
    assert!(hub.rooms["ABC"].players.is_empty());
    assert_eq!(hub.directory(), json!({"rooms":[]}));
}

#[test]
fn replaced_and_freed_connections_cannot_queue_army_orders_or_get_feedback() {
    let (mut hub, mut host, mut guest) = room();
    hub.attach_native("ABC", native_tests::native_match(), 0)
        .unwrap();
    host.messages();
    guest.messages();
    let mut returned = Peer::new(3);
    returned.hello(&mut hub, "ABC", "guest", "Returned", false);
    returned.messages();
    host.messages();
    assert!(
        matches!(guest.output.try_recv(), Ok(Output::Json(message)) if message["t"] == "replaced")
    );
    assert!(matches!(
        guest.output.try_recv(),
        Ok(Output::Close(1000, ""))
    ));
    let owned = hub.rooms["ABC"]
        .native
        .as_ref()
        .unwrap()
        .runtime
        .state()
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 1)
        .unwrap()
        .id;
    let before = hub.rooms["ABC"]
        .native
        .as_ref()
        .unwrap()
        .runtime
        .state()
        .commands
        .units
        .clone();
    for message in [
        json!({"t":"stop","ids":[owned]}),
        json!({"t":"move","orders":[[1,2,"bad"]]}),
    ] {
        guest.send(&mut hub, "ABC", message, 1);
    }
    assert!(guest.messages().is_empty());
    assert!(host.messages().is_empty() && returned.messages().is_empty());
    let state = hub.rooms["ABC"].native.as_ref().unwrap().runtime.state();
    assert_eq!(state.commands.pending_commands(), 0);
    assert_eq!(state.commands.units, before);
    hub.disconnected("ABC", 3, 2);
    host.messages();
    hub.rooms
        .get_mut("ABC")
        .unwrap()
        .native
        .as_mut()
        .unwrap()
        .runtime
        .resume(2);
    assert!(!hub.native_status("ABC").unwrap().paused);
    let mut ghost = Peer::new(4);
    ghost.joined = true;
    ghost.send(
        &mut hub,
        "ABC",
        json!({"t":"move","orders":[[1,2,"bad"]]}),
        3,
    );
    returned.send(&mut hub, "ABC", json!({"t":"stop","ids":[owned]}), 3);
    returned.send(
        &mut hub,
        "ABC",
        json!({"t":"move","orders":[[1,2,"bad"]]}),
        3,
    );
    assert!(ghost.messages().is_empty() && returned.messages().is_empty());
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
    assert!(host.messages().is_empty());
}

#[test]
fn in_play_kick_keeps_an_offline_seat_that_lobby_kick_would_remove() {
    let (mut hub, mut host, mut guest) = room();
    hub.attach_native("ABC", native_tests::native_match(), 0)
        .unwrap();
    host.messages();
    guest.messages();
    hub.disconnected("ABC", 2, 10);
    host.messages();
    let before = roster(&hub);
    host.send(&mut hub, "ABC", json!({"t":"kick","slot":1}), 11);
    assert_eq!(roster(&hub), before);
    assert_eq!(hub.rooms["ABC"].players.len(), 2);
}

#[test]
fn contested_point_requires_two_visible_on_point_teams() {
    let mut state = native_tests::native_match();
    let own = state
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 0)
        .unwrap()
        .id;
    let enemy = state
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 1)
        .unwrap()
        .id;
    let another = state
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 0 && u.id != own)
        .unwrap()
        .id;
    state.start.points[0].contested = true;
    state.start.points[0].on_point = vec![own, enemy];
    state.world.game.players[0].visible.clear();
    let contest = |state: &Match| {
        SnapshotCache::new(state)
            .unwrap()
            .recipient(0)
            .unwrap()
            .fields["points"][0][3]
            .clone()
    };
    assert_eq!(
        contest(&state),
        0,
        "hidden opponent must not leak live contest"
    );
    state.world.game.players[0].visible = vec![enemy];
    assert_eq!(contest(&state), 1);
    state.start.points[0].on_point = vec![own, another];
    assert_eq!(contest(&state), 0, "two allied actors are still one team");
    state.start.points[0].on_point = vec![own, enemy];
    state.start.points[0].contested = false;
    assert_eq!(contest(&state), 0);
}

#[test]
fn ai_mask_omission_keeps_hidden_units_and_changed_terrain_private() {
    let mut browser = native_tests::native_match();
    browser.world.game.players[0].visible.clear();
    let hidden = browser
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 1)
        .unwrap()
        .id;
    let cell = (0..browser.world.game.chars.len())
        .find(|&cell| {
            let at = ww2_core::game::Point {
                x: (cell % browser.world.game.w) as f64 * 2.0 + 1.0,
                z: (cell / browser.world.game.w) as f64 * 2.0 + 1.0,
            };
            browser.world.game.chars[cell] == b'.'
                && browser.world.game.height.as_ref().unwrap()[cell] == 0
                && !browser.recipient_sees(0, at).unwrap()
        })
        .unwrap();
    browser.observed_terrain(0, true).unwrap();
    browser
        .world
        .mutate_cell(
            cell,
            ww2_core::world::CellChange {
                height: Some(1),
                ..Default::default()
            },
        )
        .unwrap();
    // Two independently constructed matches keep their producer identities intact.
    let mut ai = native_tests::native_match();
    ai.world.game.players[0].visible.clear();
    ai.observed_terrain(0, true).unwrap();
    ai.world
        .mutate_cell(
            cell,
            ww2_core::world::CellChange {
                height: Some(1),
                ..Default::default()
            },
        )
        .unwrap();
    let mut browser_delivery = DeliveryState::new(&browser).unwrap();
    let mut ai_delivery = DeliveryState::new(&ai).unwrap();
    let frame = browser_delivery
        .capture(&mut browser, &[true, true], &[false, false])
        .unwrap();
    let mut expected = frame.recipient(0).unwrap().fields;
    assert!(expected.remove("fog").is_some());
    let frame = ai_delivery
        .capture(&mut ai, &[true, true], &[true, false])
        .unwrap();
    let actual = frame.recipient(0).unwrap().fields;
    assert!(actual.get("fog").is_none());
    assert_eq!(actual, expected);
    assert!(
        !actual["units"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row[0] == hidden)
    );
    assert!(
        !actual["cells"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row[0] == cell && row[2] == 1)
    );
}

#[test]
fn reconnect_start_clears_client_units_and_replaces_existing_delta_history() {
    let (mut hub, mut host, mut guest) = room();
    hub.attach_native("ABC", native_tests::native_match(), 0)
        .unwrap();
    let start = host
        .messages()
        .into_iter()
        .find(|m| m["t"] == "start")
        .unwrap();
    guest.messages();
    let mut view = ClientView::new();
    view.apply(&start.to_string()).unwrap();
    for step in 0..4 {
        hub.poll_native(step * 50, step * 50_000_000);
        for message in drain(&mut host, &mut hub, step * 50) {
            view.apply(&message.to_string()).unwrap();
        }
        drain(&mut guest, &mut hub, step * 50);
    }
    assert!(view.unit_count() > 0);
    let mut replacement = Peer::new(3);
    replacement.send(
        &mut hub,
        "ABC",
        json!({"t":"hello","token":"host","name":"Host returned"}),
        101,
    );
    let start = replacement
        .messages()
        .into_iter()
        .find(|m| m["t"] == "start")
        .unwrap();
    assert_eq!(view.apply(&start.to_string()).unwrap(), Applied::Start);
    assert_eq!(
        view.unit_count(),
        0,
        "start clears the previous receiver's rows"
    );
    let mut snapshots = 0;
    for step in 4..6 {
        hub.poll_native(step * 50, step * 50_000_000);
        for message in drain(&mut replacement, &mut hub, step * 50) {
            if message["t"] == "s" {
                let expected =
                    SnapshotCache::new(hub.rooms["ABC"].native.as_ref().unwrap().runtime.state())
                        .unwrap()
                        .recipient(0)
                        .unwrap()
                        .fields["units"]
                        .clone();
                if snapshots == 0 {
                    assert_eq!(
                        message["units"], expected,
                        "a fresh ledger sends every required row"
                    );
                }
                snapshots += 1;
                view.apply(&message.to_string()).unwrap();
                assert_eq!(view.unit_count(), expected.as_array().unwrap().len());
                for row in expected.as_array().unwrap() {
                    assert_eq!(
                        view.unit(row[0].as_u64().unwrap() as u32).unwrap().raw(),
                        row
                    );
                }
                assert!(!view.needs_resync());
            }
        }
        drain(&mut guest, &mut hub, step * 50);
    }
    assert!(snapshots > 0);
    assert!(view.unit_count() > 0);
}

#[test]
fn pause_fields_round_deadlines_and_survive_paused_join_and_periodic_delivery() {
    let (mut hub, mut host, mut guest) = room();
    hub.attach_native("ABC", native_tests::native_match(), 0)
        .unwrap();
    host.messages();
    guest.messages();
    hub.disconnected("ABC", 2, 100);
    assert_eq!(
        host.messages()[0],
        json!({"t":"pause","paused":true,"by":"Guest","reason":"drop","left":30})
    );
    let runtime = &hub.rooms["ABC"].native.as_ref().unwrap().runtime;
    for (now, left) in [(100, 30), (101, 30), (1100, 29), (29_101, 1), (30_100, 0)] {
        assert_eq!(runtime.pause_message(now)["left"], left);
    }
    let mut replacement = Peer::new(3);
    replacement.send(
        &mut hub,
        "ABC",
        json!({"t":"hello","token":"host","name":"Host returned"}),
        101,
    );
    let messages = replacement.messages();
    assert_eq!(messages[0]["t"], "start");
    assert_eq!(
        messages[1],
        json!({"t":"pause","paused":true,"by":"Guest","reason":"drop","left":30})
    );
    hub.poll_native(1099, 0);
    assert!(drain(&mut replacement, &mut hub, 1099).is_empty());
    hub.poll_native(1100, 50_000_000);
    let messages = drain(&mut replacement, &mut hub, 1100);
    assert_eq!(
        messages[0],
        json!({"t":"pause","paused":true,"by":"Guest","reason":"drop","left":29})
    );
    assert_eq!(messages[1]["t"], "s");
    assert_eq!(hub.native_status("ABC").unwrap().tick, 0);
    let mut returned = Peer::new(4);
    returned.send(
        &mut hub,
        "ABC",
        json!({"t":"hello","token":"guest","name":"Guest returned"}),
        1101,
    );
    let messages = returned.messages();
    assert_eq!(messages[1]["left"], 29);
    assert_eq!(
        messages[2],
        json!({"t":"pause","paused":false,"by":"","reason":"host","left":0})
    );
    let runtime = &mut hub
        .rooms
        .get_mut("ABC")
        .unwrap()
        .native
        .as_mut()
        .unwrap()
        .runtime;
    let message = runtime
        .pause(
            0,
            "Host returned",
            crate::room_runtime::PauseReason::Host,
            1200,
        )
        .unwrap()
        .unwrap();
    assert_eq!(
        message,
        json!({"t":"pause","paused":true,"by":"Host returned","reason":"host","left":0})
    );
    assert_eq!(runtime.pause_message(1_000_000), message);
    assert_eq!(
        runtime.resume(1201),
        json!({"t":"pause","paused":false,"by":"","reason":"host","left":0})
    );
}

#[test]
fn end_hold_reveals_hidden_units_refuses_orders_and_preserves_personal_results() {
    for winner in [0, 1, -1] {
        let (mut hub, mut host, mut guest) = room();
        let mut state = native_tests::native_match();
        for player in &mut state.world.game.players {
            player.visible.clear();
        }
        for slot in 0..2 {
            let rows = SnapshotCache::new(&state)
                .unwrap()
                .recipient(slot)
                .unwrap()
                .fields["units"]
                .clone();
            assert!(rows.as_array().unwrap().len() < state.world.game.units.len());
        }
        state
            .record_story_tally(Some(0), ww2_core::match_story::StoryKey::Kills, 7.0)
            .unwrap();
        state
            .record_story_tally(Some(1), ww2_core::match_story::StoryKey::Losses, 9.0)
            .unwrap();
        state
            .initialize_tick_scene_finish(
                winner,
                ww2_core::match_end::EndReason::Headquarters,
                None,
            )
            .unwrap();
        let unit_count = state.world.game.units.len();
        hub.attach_native("ABC", state, 0).unwrap();
        host.messages();
        guest.messages();
        let mut snapshots = [0; 2];
        let mut outcomes = [0; 2];
        for step in 0..120 {
            if step == 10 {
                // Retain an offline human through the hold; completion cleans that seat.
                if winner == 0 {
                    hub.disconnected("ABC", 2, step * 50);
                    hub.rooms
                        .get_mut("ABC")
                        .unwrap()
                        .native
                        .as_mut()
                        .unwrap()
                        .runtime
                        .resume(step * 50);
                    host.messages();
                }
            }
            host.send(&mut hub, "ABC", json!({"t":"stop","ids":[]}), step * 50);
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
            hub.poll_native(step * 50, step * 50_000_000);
            for (slot, peer) in [&mut host, &mut guest].into_iter().enumerate() {
                for message in drain(peer, &mut hub, step * 50) {
                    if message["t"] == "s" {
                        if snapshots[slot] == 0 {
                            assert_eq!(message["units"].as_array().unwrap().len(), unit_count);
                        }
                        assert_eq!(message["held"][0], unit_count);
                        snapshots[slot] += 1;
                    }
                    if message["t"] == "lobby" && message["state"] == "lobby" {
                        outcomes[slot] += 1;
                        assert_eq!(
                            message["result"]["you"]["outcome"],
                            if winner == -1 {
                                "draw"
                            } else if winner == slot as i32 {
                                "victory"
                            } else {
                                "defeat"
                            }
                        );
                    }
                }
            }
        }
        assert_eq!(snapshots, [60, if winner == 0 { 5 } else { 60 }]);
        assert_eq!(outcomes, [1, if winner == 0 { 0 } else { 1 }]);
        assert!(hub.rooms["ABC"].native.is_none());
        assert_eq!(hub.native_status("ABC").unwrap().held, 120);
        assert_eq!(hub.native_status("ABC").unwrap().tick, 60);
        let result = &hub.rooms["ABC"].result.as_ref().unwrap().report;
        assert_eq!(result["names"], json!(["Host", "Guest"]));
        assert_eq!(result["teams"], json!([0, 1]));
        assert_eq!(result["story"][0]["kills"].as_f64(), Some(7.0));
        assert_eq!(result["story"][1]["losses"].as_f64(), Some(9.0));
        if winner == 0 {
            assert_eq!(hub.rooms["ABC"].players.len(), 1);
        }
        let outcome = hub.rooms["ABC"].players[0].last_match.clone().unwrap();
        host.send(&mut hub, "ABC", json!({"t":"addAi"}), 7000);
        assert_eq!(host.last_lobby()["result"]["you"], outcome);
        let report = &hub.rooms["ABC"].result.as_ref().unwrap().report;
        if winner == 0 {
            assert_eq!(report["names"], json!(["Host", "", "Guest"]));
            assert_eq!(report["teams"], json!([0, null, 1]));
            assert_eq!(report["story"][2]["losses"].as_f64(), Some(9.0));
        }
        let last = hub.rooms["ABC"].players.len() - 1;
        host.send(&mut hub, "ABC", json!({"t":"kick","slot":last}), 7001);
        assert_eq!(host.last_lobby()["result"]["you"], outcome);
    }
}
