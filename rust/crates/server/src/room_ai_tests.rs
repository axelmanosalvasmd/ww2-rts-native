use super::*;
use crate::room_runtime::PauseReason;
use ww2_core::{
    match_commands::{Command, Denial},
    recipient_snapshot::SelectedSeat,
};

fn client(id: u64) -> (Connection, mpsc::Receiver<Output>) {
    let (tx, rx) = mpsc::channel(128);
    let (cancel, _) = watch::channel(false);
    (Connection::new(id, tx, cancel), rx)
}

fn hello(hub: &mut Hub, connection: &Connection, token: &str, now: u64) {
    hub.message(
        "ABC",
        connection,
        &mut false,
        json!({"t":"hello", "token":token, "name":token}),
        now,
    );
}

fn send(hub: &mut Hub, connection: &Connection, message: Value, now: u64) {
    hub.message("ABC", connection, &mut true, message, now);
}

fn drain(rx: &mut mpsc::Receiver<Output>) -> Vec<Output> {
    let mut messages = Vec::new();
    while let Ok(message) = rx.try_recv() {
        messages.push(message);
    }
    messages
}

fn humans() -> (
    Hub,
    Connection,
    mpsc::Receiver<Output>,
    Connection,
    mpsc::Receiver<Output>,
) {
    let mut hub = Hub::new(32, String::new());
    let (a, mut ra) = client(1);
    let (b, mut rb) = client(2);
    hello(&mut hub, &a, "a", 0);
    hello(&mut hub, &b, "b", 0);
    drain(&mut ra);
    drain(&mut rb);
    (hub, a, ra, b, rb)
}

fn runtime(hub: &Hub) -> &RoomRuntime {
    &hub.rooms["ABC"].native.as_ref().unwrap().runtime
}

fn runtime_mut(hub: &mut Hub) -> &mut RoomRuntime {
    &mut hub
        .rooms
        .get_mut("ABC")
        .unwrap()
        .native
        .as_mut()
        .unwrap()
        .runtime
}

fn army(state: &Match, slot: usize) -> Vec<(u32, usize, u64, u64, u64)> {
    state
        .world
        .game
        .units
        .values()
        .filter(|u| u.owner == slot)
        .map(|u| (u.id, u.kind, u.x.to_bits(), u.z.to_bits(), u.hp.to_bits()))
        .collect()
}

#[test]
fn net48_leave_preserves_army_clears_identity_and_ignores_departed_socket() {
    let (mut hub, _, mut ra, b, mut rb) = humans();
    hub.attach_native("ABC", native_tests::native_match(), 0)
        .unwrap();
    drain(&mut ra);
    drain(&mut rb);
    let map = crate::maps::public_map("default").unwrap();
    runtime_mut(&mut hub).activate_ai(&map, 1, "hard").unwrap();
    runtime_mut(&mut hub)
        .collect_frame(
            &[SelectedSeat {
                slot: 1,
                full: true,
                omit: true,
            }],
            FrameKind::Current,
        )
        .unwrap();
    assert!(runtime(&hub).ai_delivered_view(1).is_some());
    let before = army(runtime(&hub).state(), 1);
    assert!(!before.is_empty());
    let wallet = runtime(&hub).state().start.economy.players[1].clone();
    send(&mut hub, &b, json!({"t":"leave"}), 1);
    assert_eq!(army(runtime(&hub).state(), 1), before);
    assert_eq!(runtime(&hub).state().start.economy.players[1], wallet);
    let person = &hub.rooms["ABC"].players[1];
    assert!(person.ai);
    assert_eq!(person.name, "b (AI)");
    assert_eq!(person.level, "normal");
    assert!(person.token.is_empty());
    assert!(person.connection.is_none());
    assert!(runtime(&hub).ai_delivered_view(1).is_none());
    let departing = drain(&mut rb);
    assert_eq!(departing.len(), 2);
    assert!(matches!(&departing[0], Output::Json(v) if *v == json!({"t":"left"})));
    assert!(matches!(&departing[1], Output::Close(1000, "")));
    let pending = runtime(&hub).state().commands.pending_commands();
    for raw in [
        json!({"t":"buy", "kind":"rifle"}),
        json!({"t":"name", "name":"stale"}),
        json!({"t":"leave"}),
    ] {
        send(&mut hub, &b, raw, 2);
    }
    assert_eq!(runtime(&hub).state().commands.pending_commands(), pending);
    assert_eq!(hub.rooms["ABC"].players[1].name, "b (AI)");
    assert!(drain(&mut rb).is_empty());
    let (returning, mut rr) = client(3);
    hello(&mut hub, &returning, "b", 3);
    let returned = drain(&mut rr);
    assert!(returned.iter().all(|v| !matches!(v, Output::Close(..))));
    assert!(returned.iter().any(|v| matches!(v, Output::Json(v)
        if v["t"] == "lobby" && v["spectator"] == true && v["you"] == -1)));
    assert_eq!(hub.rooms["ABC"].spectators.len(), 1);
    assert_eq!(
        hub.rooms["ABC"].spectators[0]
            .connection
            .as_ref()
            .unwrap()
            .id,
        returning.id
    );
    send(&mut hub, &returning, json!({"t":"buy", "kind":"rifle"}), 4);
    assert_eq!(runtime(&hub).state().commands.pending_commands(), pending);
    assert_eq!(hub.rooms["ABC"].players.len(), 2);
    assert!(hub.rooms["ABC"].players[1].ai);
    assert!(hub.rooms["ABC"].players[1].connection.is_none());
    assert!(hub.rooms["ABC"].players[1].token.is_empty());
}

#[test]
fn net49_hand_ai_requires_host_offline_integer_seat_and_resumes_its_drop_pause() {
    let (mut hub, a, mut ra, b, mut rb) = humans();
    hub.attach_native("ABC", native_tests::native_match(), 0)
        .unwrap();
    drain(&mut ra);
    drain(&mut rb);
    // A terminal socket is offline even before the disconnect callback removes it.
    b.cancel.send_replace(true);
    send(&mut hub, &b, json!({"t":"handAi", "slot":1}), 1);
    assert!(
        !hub.rooms["ABC"].players[1].ai,
        "a non-host cannot hand over an offline seat"
    );
    b.cancel.send_replace(false);
    send(&mut hub, &a, json!({"t":"handAi", "slot":1}), 2);
    assert!(
        !hub.rooms["ABC"].players[1].ai,
        "an online human keeps their seat"
    );
    hub.disconnected("ABC", b.id, 3);
    assert!(runtime(&hub).paused());
    for slot in [json!("1"), json!(1.5), json!(-1), json!(2), Value::Null] {
        send(&mut hub, &a, json!({"t":"handAi", "slot":slot}), 4);
        assert!(!hub.rooms["ABC"].players[1].ai);
        assert!(runtime(&hub).paused());
    }
    let before = army(runtime(&hub).state(), 1);
    send(&mut hub, &a, json!({"t":"handAi", "slot":1}), 5);
    assert!(hub.rooms["ABC"].players[1].ai);
    assert_eq!(hub.rooms["ABC"].players[1].name, "b (AI)");
    assert_eq!(hub.rooms["ABC"].players[1].level, "normal");
    assert!(hub.rooms["ABC"].players[1].token.is_empty());
    assert_eq!(army(runtime(&hub).state(), 1), before);
    assert!(!runtime(&hub).paused());
    assert!(drain(&mut ra).iter().any(|v| matches!(v, Output::Json(v) if *v == json!({"t":"pause", "paused":false, "by":"", "reason":"host", "left":0}))));
    send(&mut hub, &a, json!({"t":"handAi", "slot":1}), 6);
    assert_eq!(hub.rooms["ABC"].players[1].name, "b (AI)");
}

#[test]
fn net49_handover_preserves_host_pause_and_other_seats_drop_pause() {
    for reason in [PauseReason::Host, PauseReason::Drop] {
        let (mut hub, a, _ra, b, _rb) = humans();
        hub.attach_native("ABC", native_tests::native_match(), 0)
            .unwrap();
        hub.disconnected("ABC", b.id, 1);
        runtime_mut(&mut hub).resume(2);
        runtime_mut(&mut hub).pause(0, "a", reason, 2).unwrap();
        let pause = runtime(&hub).pause_message(3);
        send(&mut hub, &a, json!({"t":"handAi", "slot":1}), 3);
        assert!(hub.rooms["ABC"].players[1].ai);
        assert_eq!(runtime(&hub).pause_message(3), pause);
    }
}

#[test]
fn ai62_level_picker_accepts_only_host_lobby_ai_seats_and_defaults_to_normal() {
    let (mut hub, a, mut ra, b, mut rb) = humans();
    send(&mut hub, &a, json!({"t":"addAi"}), 1);
    assert_eq!(hub.rooms["ABC"].players[2].level, "normal");
    for level in ["easy", "hard", "normal"] {
        send(&mut hub, &a, json!({"t":"level", "slot":2, "v":level}), 2);
        assert_eq!(hub.rooms["ABC"].players[2].level, level);
    }
    for raw in [
        json!({"t":"level", "slot":2, "v":"impossible"}),
        json!({"t":"level", "slot":2, "v":0}),
        json!({"t":"level", "slot":"2", "v":"hard"}),
        json!({"t":"level", "slot":2.5, "v":"hard"}),
        json!({"t":"level", "slot":0, "v":"hard"}),
    ] {
        send(&mut hub, &a, raw, 3);
        assert_eq!(hub.rooms["ABC"].players[2].level, "normal");
        assert_eq!(hub.rooms["ABC"].players[0].level, "normal");
    }
    send(&mut hub, &b, json!({"t":"level", "slot":2, "v":"hard"}), 4);
    assert_eq!(hub.rooms["ABC"].players[2].level, "normal");
    send(&mut hub, &a, json!({"t":"kick", "slot":2}), 5);
    hub.attach_native("ABC", native_tests::native_match(), 6)
        .unwrap();
    send(&mut hub, &b, json!({"t":"leave"}), 7);
    send(&mut hub, &a, json!({"t":"level", "slot":1, "v":"hard"}), 8);
    assert_eq!(hub.rooms["ABC"].players[1].level, "normal");
    drain(&mut ra);
    drain(&mut rb);
}

#[test]
fn ai62_difficulty_keeps_identical_delivery_wallets_and_canonical_cost_rules() {
    let map = crate::maps::public_map("default").unwrap();
    let mut first = None;
    for level in ["easy", "normal", "hard"] {
        let mut room = RoomRuntime::new(native_tests::native_match(), 0).unwrap();
        room.activate_ai(&map, 0, level).unwrap();
        let wallet = room.state().start.economy.players[0].clone();
        let frame = room
            .collect_frame(
                &[SelectedSeat {
                    slot: 0,
                    full: true,
                    omit: true,
                }],
                FrameKind::Current,
            )
            .unwrap();
        if let Some(first) = &first {
            assert_eq!(
                &frame[0].message, first,
                "difficulty cannot reveal more information"
            );
        } else {
            first = Some(frame[0].message.clone());
        }
        room.tick_ai(true).unwrap();
        assert_eq!(
            room.state().start.economy.players[0],
            wallet,
            "thinking does not credit resources"
        );
        room.state_mut().start.economy.players[0].mp = 0.0;
        let before = army(room.state(), 0);
        assert_eq!(
            room.state_mut().execute_command(
                0,
                &Command::Buy {
                    kind: "rifle".into(),
                    from: None
                },
                false
            ),
            Ok(Some(Denial::Manpower))
        );
        assert_eq!(army(room.state(), 0), before);
        assert_eq!(room.state().start.economy.players[0].mp, 0.0);
    }
}

#[test]
fn net25_replacement_clears_stored_ai_view_and_keeps_the_human_identity() {
    let (mut hub, a, mut ra, _, mut rb) = humans();
    hub.attach_native("ABC", native_tests::native_match(), 0)
        .unwrap();
    drain(&mut ra);
    drain(&mut rb);
    let map = crate::maps::public_map("default").unwrap();
    let room = runtime_mut(&mut hub);
    room.activate_ai(&map, 0, "hard").unwrap();
    room.collect_frame(
        &[SelectedSeat {
            slot: 0,
            full: true,
            omit: true,
        }],
        FrameKind::Current,
    )
    .unwrap();
    assert!(room.ai_delivered_view(0).is_some());
    hub.rooms.get_mut("ABC").unwrap().players[0].denies.push(0);
    let (replacement, mut rr) = client(3);
    hello(&mut hub, &replacement, "a", 1);
    assert!(runtime(&hub).ai_delivered_view(0).is_none());
    assert!(
        !runtime(&hub)
            .ai_selected()
            .iter()
            .any(|seat| seat.slot == 0)
    );
    let person = &hub.rooms["ABC"].players[0];
    assert!(!person.ai);
    assert_eq!(person.connection.as_ref().unwrap().id, replacement.id);
    assert_eq!(person.denies, [0]);
    assert!(
        drain(&mut ra)
            .iter()
            .any(|v| matches!(v, Output::Json(v) if *v == json!({"t":"replaced"})))
    );
    assert!(
        drain(&mut rr)
            .iter()
            .any(|v| matches!(v, Output::Json(v) if v["t"] == "start"))
    );
    let before = runtime(&hub).state().commands.pending_commands();
    send(&mut hub, &a, json!({"t":"buy", "kind":"rifle"}), 2);
    assert_eq!(runtime(&hub).state().commands.pending_commands(), before);
}

#[test]
fn net73_hosted_ai_refreshes_only_on_the_shared_snapshot_beat() {
    let mut hub = Hub::new(32, String::new());
    let (a, mut ra) = client(1);
    hello(&mut hub, &a, "a", 0);
    hub.configure_trusted_room(Some(crate::TrustedRoomConfig::new("ABC", 3936341).unwrap()));
    send(&mut hub, &a, json!({"t":"addAi"}), 0);
    send(&mut hub, &a, json!({"t":"level", "slot":1, "v":"hard"}), 0);
    send(&mut hub, &a, json!({"t":"start"}), 0);
    assert!(hub.rooms["ABC"].native.is_some());
    assert_eq!(hub.rooms["ABC"].players[1].level, "hard");
    drain(&mut ra);
    let mut delivered = 0;
    let mut refreshed = 0;
    let mut ordinary_ticks = 0;
    let mut acted = false;
    for step in 0..500u64 {
        let tick = step + 1;
        let cadence = runtime(&hub).snapshot_cadence();
        hub.poll_native(step * 50, step * 50_000_000);
        assert!(
            runtime(&hub).fault().is_none(),
            "{:?}",
            runtime(&hub).fault()
        );
        let current = runtime(&hub).state().world.game.tick as u64;
        assert_eq!(current, tick);
        let now = runtime(&hub).ai_delivered_view(1).unwrap().tick;
        if now != delivered {
            assert_eq!(current % u64::from(cadence), 0);
            assert_eq!(now, current);
            delivered = now;
            refreshed += 1;
        } else if !current.is_multiple_of(u64::from(cadence)) {
            ordinary_ticks += 1;
        }
        acted |= runtime(&hub).state().commands.pending_commands() > 0;
        for output in drain(&mut ra) {
            if let Output::Snapshot(write) = output {
                let snapshot: Value = serde_json::from_str(&write.json).unwrap();
                assert_eq!(snapshot["online"], json!([true, true]));
                assert_eq!(snapshot["ping"], json!([null, -1.0]));
                hub.complete_snapshot(&write, true, step * 50);
            }
        }
    }
    assert!(
        refreshed > 1,
        "the commander must request later observations"
    );
    assert!(
        ordinary_ticks > 100,
        "exercise ticks between snapshot beats"
    );
    assert!(
        acted,
        "the hosted AI must submit real commands between delivery beats"
    );
}

#[test]
fn room_ai_hidden_opponent_changes_leave_observations_and_plans_identical() {
    let map = crate::maps::public_map("default").unwrap();
    let mut baseline = native_tests::native_match();
    baseline.world.game.players[0].visible.clear();
    let mut hidden = native_tests::native_match();
    hidden.world.game.players[0].visible.clear();
    let opponent = hidden
        .world
        .game
        .units
        .values_mut()
        .find(|u| u.owner == 1)
        .unwrap();
    opponent.hp *= 0.5;
    hidden.start.economy.players[1].mp += 10_000.0;
    hidden.start.economy.players[1].mun += 10_000.0;
    hidden.start.economy.players[1].fuel += 10_000.0;
    let mut rooms = [
        RoomRuntime::new(baseline, 0).unwrap(),
        RoomRuntime::new(hidden, 0).unwrap(),
    ];
    let mut frames = Vec::new();
    for room in &mut rooms {
        room.activate_ai(&map, 0, "normal").unwrap();
        frames.push(
            room.collect_frame(
                &[SelectedSeat {
                    slot: 0,
                    full: true,
                    omit: true,
                }],
                FrameKind::Current,
            )
            .unwrap()
            .remove(0)
            .message,
        );
    }
    assert_eq!(frames[0], frames[1]);
    assert_eq!(rooms[0].ai_delivered_view(0), rooms[1].ai_delivered_view(0));
    let mut planned = 0;
    for _ in 0..500 {
        for room in &mut rooms {
            room.advance_tick(&[false, false]).unwrap();
        }
        let tick = rooms[0].state().world.game.tick;
        let sent = tick % 2 == 0;
        if sent {
            let mut snapshots = Vec::new();
            for room in &mut rooms {
                snapshots.push(
                    room.collect_frame(
                        &[SelectedSeat {
                            slot: 0,
                            full: false,
                            omit: true,
                        }],
                        FrameKind::Scheduled,
                    )
                    .unwrap()
                    .remove(0)
                    .message,
                );
            }
            assert_eq!(
                snapshots[0], snapshots[1],
                "hidden opponent changed recipient delivery at {tick}"
            );
            assert_eq!(rooms[0].ai_delivered_view(0), rooms[1].ai_delivered_view(0));
        }
        for room in &mut rooms {
            room.tick_ai(sent).unwrap();
        }
        let plans = rooms
            .iter()
            .map(|room| {
                room.state()
                    .commands
                    .pending_tick(tick as u64 + 1)
                    .map(|(key, command)| (*key, command.clone()))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        assert_eq!(
            plans[0], plans[1],
            "hidden state changed plans at tick {tick}"
        );
        planned += plans[0].len();
        if planned > 0 {
            break;
        }
    }
    assert!(
        planned > 0,
        "the privacy comparison must exercise real admitted AI orders"
    );
}

#[test]
fn room_ai_collects_all_requested_seats_before_planning_and_survives_natural_finish() {
    let (mut hub, a, _ra, b, _rb) = humans();
    hub.attach_native("ABC", native_tests::native_match(), 0)
        .unwrap();
    send(&mut hub, &a, json!({"t":"leave"}), 0);
    send(&mut hub, &b, json!({"t":"leave"}), 0);
    assert!(hub.rooms["ABC"].players.iter().all(|person| person.ai));
    let room = runtime_mut(&mut hub);
    let mut deliveries = 0;
    let mut commands = 0;
    let mut finished_at = None;
    for _ in 0..30_000 {
        let report = room.advance_tick(&[false, false]).unwrap();
        commands += report.commands.len();
        let pending = room.state().commands.pending_commands();
        let seats = (0..2)
            .map(|slot| SelectedSeat {
                slot,
                full: deliveries == 0,
                omit: true,
            })
            .collect::<Vec<_>>();
        let captured = room.collect_frame(&seats, FrameKind::Scheduled).unwrap_or_else(|error|panic!("tick {}: {error}; flights={}, salvos={}, grenades={}, firing_events={}, support_events={}",report.tick,room.state().firing.flights.len(),room.state().firing.salvos.len(),room.state().firing.grenades.len(),room.state().firing.events.len(),room.state().support.events.len()));
        assert_eq!(captured.len(), 2);
        assert_eq!(captured[0].batch, captured[1].batch);
        for (slot, captured) in captured.iter().enumerate() {
            assert_eq!(captured.slot, slot);
            assert_eq!(captured.message["tick"], report.tick);
            assert!(room.ai_delivered_view(slot).unwrap().tick <= report.tick as u64);
        }
        assert_eq!(
            room.state().commands.pending_commands(),
            pending,
            "every requested snapshot is collected before commands are admitted"
        );
        deliveries += 2;
        room.tick_ai(true).unwrap();
        assert!(room.fault().is_none());
        assert!(room.tick_failure().is_none());
        if room.state().finish.is_some() {
            let at = *finished_at.get_or_insert(report.tick);
            if report.tick >= at + 8 {
                break;
            }
        }
    }
    assert!(
        room.state().finish.is_some(),
        "ordinary AI match must reach its natural victory target"
    );
    assert!(
        commands > 10,
        "the full match must exercise AI command admission"
    );
    assert!(deliveries > 100);
    assert!(room.state().world.game.tick >= finished_at.unwrap() + 8);
    for step in 0..120u64 {
        hub.poll_native(step * 50, step * 50_000_000);
    }
    assert!(hub.rooms["ABC"].native.is_none());
    assert!(hub.rooms["ABC"].result.is_some());
    assert_eq!(hub.rooms["ABC"].players.len(), 2);
    assert!(
        hub.rooms["ABC"]
            .players
            .iter()
            .all(|person| person.ai && person.token.is_empty())
    );
}

#[test]
fn net49_new_online_host_can_handover_before_old_socket_disconnect_callback() {
    let (mut hub, a, mut ra, b, mut rb) = humans();
    hub.attach_native("ABC", native_tests::native_match(), 0)
        .unwrap();
    drain(&mut ra);
    drain(&mut rb);
    a.cancel.send_replace(true);
    assert_eq!(hub.rooms["ABC"].host(), Some(1));
    send(&mut hub, &b, json!({"t":"handAi", "slot":0}), 1);
    assert!(hub.rooms["ABC"].players[0].ai);
    assert!(hub.rooms["ABC"].players[0].connection.is_none());
    assert!(runtime(&hub).fault().is_none());
}

#[test]
fn ai63_hosted_save_restores_planners_resets_humans_and_restarts_commanders() {
    struct SaveDir(std::path::PathBuf);
    impl Drop for SaveDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let directory =
        SaveDir(std::env::temp_dir().join(format!("ww2-room-ai-save-{}", std::process::id())));
    let mut hub = Hub::new(32, String::new());
    hub.configure_saves(directory.0.clone());
    let (host, mut rh) = client(1);
    hello(&mut hub, &host, "a", 0);
    send(&mut hub, &host, json!({"t":"addAi"}), 0);
    send(&mut hub, &host, json!({"t":"level","slot":1,"v":"hard"}), 0);
    hub.attach_native("ABC", native_tests::native_match(), 0)
        .unwrap();
    drain(&mut rh);
    let map = crate::maps::public_map("default").unwrap();
    // Human helper memory may exist at capture, but loading must discard it.
    runtime_mut(&mut hub).activate_ai(&map, 0, "hard").unwrap();
    let selected = (0..2)
        .map(|slot| SelectedSeat {
            slot,
            full: true,
            omit: true,
        })
        .collect::<Vec<_>>();
    runtime_mut(&mut hub)
        .collect_frame(&selected, FrameKind::Current)
        .unwrap();
    let opening = runtime(&hub).save_ai_memories();
    for _ in 0..500 {
        let room = runtime_mut(&mut hub);
        room.advance_tick(&[false, false]).unwrap();
        let sent = room.state().world.game.tick % 2 == 0;
        if sent {
            room.collect_frame(&selected, FrameKind::Scheduled).unwrap();
        }
        room.tick_ai(sent).unwrap();
    }
    let memories = runtime(&hub).save_ai_memories();
    assert_eq!(
        runtime(&hub).ai_clone_failure(),
        Value::Null,
        "a failure in the second seat must discard the first seat's memory too"
    );
    let before_load = runtime(&hub).ai_commander(1).unwrap().clone();
    assert!(!memories[0].is_null());
    assert_ne!(
        memories[1]["planner"], opening[1]["planner"],
        "exercise learned planner state before saving"
    );
    let id = hub.save_room("ABC", host.id).unwrap();
    let saved = hub.saves.read("ABC", &id).unwrap();
    assert_eq!(saved["ai"], memories);
    assert_eq!(saved["seats"][1]["ai"], true);
    assert_eq!(saved["seats"][1]["name"], "AI 1");
    let checkpoint_tick = runtime(&hub).state().world.game.tick;
    let canonical = runtime(&hub).state().save_json().unwrap();
    let mut restored_memory =
        RoomRuntime::new(Match::from_save_json(canonical.clone()).unwrap(), 0).unwrap();
    restored_memory.activate_ai(&map, 1, "hard").unwrap();
    restored_memory.restore_ai_memories(&memories).unwrap();
    assert_eq!(
        restored_memory.save_ai_memories()[1],
        memories[1],
        "every planner, recipient, sighting and pending-terrain field restores byte-equal before the opening look"
    );
    assert!(restored_memory.save_ai_memories()[0].is_null());
    assert!(
        restored_memory.state().save_json().unwrap() == canonical,
        "the complete checkpoint must round-trip before new observations"
    );
    let mut fresh_delivery =
        ww2_core::recipient_snapshot::DeliveryState::new(restored_memory.state()).unwrap();
    let start_selected = selected
        .iter()
        .map(|seat| SelectedSeat {
            omit: false,
            ..*seat
        })
        .collect::<Vec<_>>();
    assert!(
        fresh_delivery
            .capture_selected_with_lists(restored_memory.state_mut(), &start_selected, &[])
            .unwrap()
            .is_some()
    );
    let after_start = restored_memory.state().save_json().unwrap();
    hub.rooms.get_mut("ABC").unwrap().native = None;
    drain(&mut rh);
    hub.load_room("ABC", host.id, &id, 1).unwrap();
    assert!(runtime(&hub).paused());
    assert_eq!(runtime(&hub).state().world.game.tick, checkpoint_tick);
    assert!(
        runtime(&hub).state().save_json().unwrap() == after_start,
        "loaded starts must retain the checkpoint with its fresh recipient routing observations"
    );
    let restored = runtime(&hub).save_ai_memories();
    assert!(restored[0].is_null(), "human planner memory must be reset");
    assert_eq!(restored[1]["planner"], memories[1]["planner"]);
    use ww2_core::seat_commander::Brain;
    let Brain::Human {
        state: before_attention,
        ..
    } = before_load.brain()
    else {
        panic!("ordinary commander");
    };
    let fresh = runtime(&hub)
        .state()
        .create_seat_commander(1, ww2_core::commander_ai::Level::Hard)
        .unwrap();
    let Brain::Human {
        state: fresh_attention,
        ..
    } = fresh.brain()
    else {
        panic!("ordinary commander");
    };
    let Brain::Human {
        state: loaded_attention,
        delivered,
        ..
    } = runtime(&hub).ai_commander(1).unwrap().brain()
    else {
        panic!("ordinary commander");
    };
    assert_ne!(
        before_attention, fresh_attention,
        "exercise a used commander's attention state"
    );
    assert_eq!(loaded_attention, fresh_attention);
    assert!(delivered.is_none());
    assert!(loaded_attention.wants_view(checkpoint_tick as u64));
    assert_eq!(hub.rooms["ABC"].players[1].level, "hard");
    assert!(hub.rooms["ABC"].players[1].ai);
    assert!(hub.rooms["ABC"].players[1].token.is_empty());
    let messages = drain(&mut rh)
        .into_iter()
        .filter_map(|o| match o {
            Output::Json(v) => Some(v),
            _ => None,
        })
        .collect::<Vec<_>>();
    let lobby = messages.iter().position(|v| v["t"] == "lobby").unwrap();
    let pause = messages
        .iter()
        .position(|v| v["t"] == "pause" && v["paused"] == true)
        .unwrap();
    let start = messages.iter().position(|v| v["t"] == "start").unwrap();
    assert!(lobby < pause && pause < start);
    send(&mut hub, &host, json!({"t":"resume"}), 2);
    for step in 0..20 {
        hub.poll_native(2 + step * 50, step * 50_000_000);
        drain(&mut rh);
        let room = runtime_mut(&mut hub);
        room.collect_frame(&selected, FrameKind::Current).unwrap();
        assert!(room.fault().is_none());
        assert!(room.tick_failure().is_none());
    }
    assert!(runtime(&hub).state().world.game.tick > checkpoint_tick);
}

#[test]
fn room_ai_host_callbacks_survive_natural_finish_and_final_snapshots() {
    let (mut hub, _host, mut rh, guest, _rg) = humans();
    hub.attach_native("ABC", native_tests::native_match(), 0)
        .unwrap();
    drain(&mut rh);
    send(&mut hub, &guest, json!({"t":"leave"}), 0);
    let selected = (0..2)
        .map(|slot| SelectedSeat {
            slot,
            full: true,
            omit: true,
        })
        .collect::<Vec<_>>();
    let mut finished_at = None;
    let mut terminal_tick = 0;
    let mut orders = 0;
    let mut deliveries = 0;
    for step in 0..30_000u64 {
        hub.poll_native(step * 50, step * 50_000_000);
        for output in drain(&mut rh) {
            if let Output::Snapshot(write) = output {
                hub.complete_snapshot(&write, true, step * 50);
            }
        }
        if hub.rooms["ABC"].native.is_none() {
            break;
        }
        let room = runtime_mut(&mut hub);
        assert!(room.fault().is_none(), "{:?}", room.fault());
        assert!(room.tick_failure().is_none());
        orders += room.state().commands.pending_commands();
        let tick = room.state().world.game.tick;
        if room.state().finish.is_some() {
            finished_at.get_or_insert(tick);
            terminal_tick = tick;
            // Once victory is fixed, check final recipient frames on every callback.
            // During play, only Hub's real snapshot cadence supplies observations.
            let captured = room.collect_frame(&selected, FrameKind::Current).unwrap();
            for (slot, frame) in captured.iter().enumerate() {
                assert_eq!(frame.slot, slot);
                assert_eq!(frame.message["tick"], tick);
                deliveries += 1;
            }
        } else if tick >= 2 {
            assert!(room.ai_delivered_view(1).is_some());
        }
    }
    assert!(
        hub.rooms["ABC"].native.is_none(),
        "host callbacks must complete the match"
    );
    assert!(hub.rooms["ABC"].result.is_some());
    assert!(
        terminal_tick > finished_at.unwrap(),
        "collect snapshots on advancing terminal ticks"
    );
    assert!(orders > 10, "actual hosted scheduling must admit orders");
    assert!(deliveries > 100);
    assert_eq!(hub.rooms["ABC"].players.len(), 2);
    assert!(!hub.rooms["ABC"].players[0].ai);
    assert!(hub.rooms["ABC"].players[1].ai);
    assert!(hub.rooms["ABC"].players[1].token.is_empty());
}
