use super::*;
use crate::room_runtime::PauseReason;
use ww2_core::{recipient_snapshot::SelectedSeat, seat_commander::Brain};

struct Peer {
    connection: Connection,
    receiver: mpsc::Receiver<Output>,
    joined: bool,
}
impl Peer {
    fn new(id: u64) -> Self {
        let (tx, receiver) = mpsc::channel(128);
        let (cancel, _) = watch::channel(false);
        Self {
            connection: Connection::new(id, tx, cancel),
            receiver,
            joined: false,
        }
    }
    fn send(&mut self, hub: &mut Hub, message: Value, now: u64) {
        hub.message("ABC", &self.connection, &mut self.joined, message, now);
    }
    fn drain(&mut self, hub: &mut Hub, now: u64) -> Vec<Value> {
        let mut messages = Vec::new();
        while let Ok(output) = self.receiver.try_recv() {
            let bytes = output.diagnostic_bytes();
            match output {
                Output::Json(message) => messages.push(message),
                Output::Watcher(message) => messages.push(serde_json::from_str(&message).unwrap()),
                Output::Snapshot(write) => {
                    messages.push(serde_json::from_str(&write.json).unwrap());
                    hub.complete_snapshot(&write, true, now);
                }
                Output::Close(code, reason) => panic!("unexpected close {code}: {reason}"),
            }
            self.connection.complete_bytes(bytes);
        }
        messages
    }
}

fn room(horde: bool, level: &str) -> (Hub, [Peer; 3]) {
    let mut hub = Hub::new(32, String::new());
    hub.configure_trusted_room(Some(crate::TrustedRoomConfig::new("ABC", 3936341).unwrap()));
    let mut host = Peer::new(1);
    host.send(
        &mut hub,
        json!({"t":"hello","token":"host","name":"Host"}),
        0,
    );
    host.send(&mut hub, json!({"t":"addAi"}), 0);
    host.send(&mut hub, json!({"t":"level","slot":1,"v":level}), 0);
    if horde {
        host.send(&mut hub, json!({"t":"mode","v":"horde"}), 0);
    }
    let mut operative = Peer::new(2);
    operative.send(
        &mut hub,
        json!({"t":"hello","token":"op","name":"Op","role":"operative","commander":0}),
        0,
    );
    let mut watcher = Peer::new(3);
    watcher.send(
        &mut hub,
        json!({"t":"hello","token":"watch","name":"Watch","spectate":true}),
        0,
    );
    assert!(host.joined && operative.joined && watcher.joined);
    let mut peers = [host, operative, watcher];
    for peer in &mut peers {
        peer.drain(&mut hub, 0);
    }
    (hub, peers)
}

fn runtime(hub: &Hub) -> &RoomRuntime {
    &hub.rooms["ABC"].native.as_ref().unwrap().runtime
}

fn assert_start(messages: &[Value], role: usize, count: usize, loaded: bool) -> u64 {
    let lobby = messages
        .iter()
        .position(|m| m["t"] == "lobby" && m["state"] == "play")
        .unwrap();
    let start = messages.iter().position(|m| m["t"] == "start").unwrap();
    assert!(lobby < start, "playing lobby precedes every role's start");
    assert!(messages[..start].iter().all(|m| m["t"] != "s"));
    assert_eq!(messages.iter().filter(|m| m["t"] == "start").count(), 1);
    if loaded {
        let pause = messages
            .iter()
            .position(|m| m["t"] == "pause" && m["paused"] == true)
            .unwrap();
        assert!(
            lobby < pause && pause < start,
            "loaded pause precedes every start"
        );
    }
    let message = &messages[start];
    assert_eq!(message["names"].as_array().unwrap().len(), count);
    for key in ["spawns", "teams", "factions"] {
        assert_eq!(message[key].as_array().unwrap().len(), count);
    }
    assert!(message["cells"].is_array());
    match role {
        0 => {
            assert!(message["fog"]["v"].is_string());
            assert!(message["fog"]["e"].is_string());
        }
        1 => {
            assert_eq!(message["role"], "operative");
            assert_eq!(message["you"], 0);
            assert!(message["operative"].is_object());
            assert!(message["fog"]["v"].is_string());
            assert!(message["fog"]["e"].is_string());
        }
        2 => {
            assert_eq!(message["you"], 0);
            assert!(message.get("fog").is_none());
        }
        _ => unreachable!(),
    }
    message["matchId"].as_u64().unwrap()
}

fn run_callbacks(hub: &mut Hub, peers: &mut [Peer; 3], first: u64, last: u64) {
    let count = runtime(hub).state().world.game.players.len();
    let mut received = [0; 3];
    for callback in first..=last {
        let now = callback * 50;
        hub.poll_native(now, now * 1_000_000);
        assert_eq!(hub.native_status("ABC").unwrap().fault, None);
        for (role, peer) in peers.iter_mut().enumerate() {
            for message in peer.drain(hub, now) {
                if message["t"] == "s" {
                    assert_eq!(message["online"], json!([true, true]));
                    received[role] += 1;
                }
            }
        }
        let selected = (0..count)
            .map(|slot| SelectedSeat {
                slot,
                full: false,
                omit: false,
            })
            .collect::<Vec<_>>();
        let native = hub.rooms.get_mut("ABC").unwrap().native.as_mut().unwrap();
        assert_eq!(
            native
                .runtime
                .collect_frame(&selected, FrameKind::Current)
                .unwrap()
                .len(),
            count
        );
        assert!(native.runtime.state().tick_state.fault().is_none());
    }
    assert!(received.iter().all(|count| *count > 0));
    assert_eq!(received[0], received[1]);
    assert_eq!(received[0], received[2]);
}

#[test]
fn ai62_net40_hosted_horde_levels_start_all_roles_and_share_recipient_economy_rules() {
    let mut initial = None;
    for level in ["easy", "normal", "hard"] {
        let (mut hub, mut peers) = room(true, level);
        peers[0].send(&mut hub, json!({"t":"start"}), 0);
        assert_eq!(hub.native_status("ABC").unwrap().fault, None);
        assert_eq!(runtime(&hub).state().world.game.tick, 0);
        assert!(!runtime(&hub).paused());
        assert_eq!(runtime(&hub).snapshot_cadence(), 2);
        assert_eq!(runtime(&hub).held(), 0);
        let horde = runtime(&hub).state().horde.as_ref().unwrap().slot;
        assert_eq!(horde, 2);
        {
            let native = hub.rooms.get_mut("ABC").unwrap().native.as_mut().unwrap();
            let map = native.startup.public_map().clone();
            native.runtime.activate_ai(&map, horde, "hard").unwrap();
            native
                .runtime
                .collect_frame(
                    &[SelectedSeat {
                        slot: horde,
                        full: true,
                        omit: false,
                    }],
                    FrameKind::Current,
                )
                .unwrap();
        }
        assert_eq!(
            runtime(&hub).ai_level(1),
            ww2_core::commander_ai::Level::from_key(level)
        );
        assert_eq!(
            runtime(&hub).ai_level(horde),
            Some(ww2_core::commander_ai::Level::Normal)
        );
        assert!(matches!(
            runtime(&hub).ai_commander(1).unwrap().brain(),
            Brain::Human { .. }
        ));
        assert!(matches!(
            runtime(&hub).ai_commander(horde).unwrap().brain(),
            Brain::Horde(_)
        ));
        assert!(
            runtime(&hub)
                .ai_delivered_view(1)
                .unwrap()
                .packet
                .input()
                .runtime
                .as_ref()
                .unwrap()
                .defender
        );
        assert!(
            !runtime(&hub)
                .ai_delivered_view(1)
                .unwrap()
                .packet
                .input()
                .runtime
                .as_ref()
                .unwrap()
                .horde
        );
        for (role, peer) in peers.iter_mut().enumerate() {
            let messages = peer.drain(&mut hub, 0);
            assert_eq!(assert_start(&messages, role, 3, false), 1);
        }
        let snapshot = hub
            .rooms
            .get_mut("ABC")
            .unwrap()
            .native
            .as_mut()
            .unwrap()
            .runtime
            .collect_frame(
                &[SelectedSeat {
                    slot: 1,
                    full: true,
                    omit: false,
                }],
                FrameKind::Current,
            )
            .unwrap()
            .remove(0)
            .message;
        let wallet = &runtime(&hub).state().start.economy.players[1];
        let opening = json!([snapshot, wallet.mp, wallet.vp]);
        if let Some(expected) = &initial {
            assert_eq!(
                &opening, expected,
                "difficulty changes neither initial view nor economy"
            );
        } else {
            initial = Some(opening);
        }
        run_callbacks(&mut hub, &mut peers, 1, 240);
        assert!(runtime(&hub).ai_decoded_view(horde).unwrap().tick > 0);
        assert!(runtime(&hub).ai_delivered_view(1).unwrap().tick > 0);
    }
}

#[test]
fn net40_loaded_all_roles_receive_lobby_pause_start_before_resumed_snapshots() {
    struct SaveDir(std::path::PathBuf);
    impl Drop for SaveDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let directory =
        SaveDir(std::env::temp_dir().join(format!("ww2-room-ai3-save-{}", std::process::id())));
    let (mut hub, mut peers) = room(false, "hard");
    hub.configure_saves(directory.0.clone());
    peers[0].send(&mut hub, json!({"t":"start"}), 0);
    for (role, peer) in peers.iter_mut().enumerate() {
        assert_start(&peer.drain(&mut hub, 0), role, 2, false);
    }
    run_callbacks(&mut hub, &mut peers, 1, 8);
    let id = hub.save_room("ABC", peers[0].connection.id).unwrap();
    let tick = runtime(&hub).state().world.game.tick;
    hub.rooms.get_mut("ABC").unwrap().native = None;
    hub.load_room("ABC", peers[0].connection.id, &id, 450)
        .unwrap();
    assert_eq!(runtime(&hub).state().world.game.tick, tick);
    assert!(runtime(&hub).paused());
    assert_eq!(
        runtime(&hub).ai_level(1),
        Some(ww2_core::commander_ai::Level::Hard)
    );
    assert_eq!(runtime(&hub).snapshot_cadence(), 2);
    assert_eq!(hub.rooms["ABC"].spectators.len(), 1);
    assert_eq!(hub.rooms["ABC"].operatives.len(), 1);
    for (role, peer) in peers.iter_mut().enumerate() {
        assert_eq!(assert_start(&peer.drain(&mut hub, 450), role, 2, true), 2);
    }
    peers[0].send(&mut hub, json!({"t":"resume"}), 450);
    run_callbacks(&mut hub, &mut peers, 10, 24);
}

#[test]
fn mode09_horde_departure_activates_normal_ai_clears_token_and_only_its_drop_pause() {
    let (mut hub, mut peers) = room(true, "hard");
    peers[0].send(&mut hub, json!({"t":"start"}), 0);
    for peer in &mut peers {
        peer.drain(&mut hub, 0);
    }
    let native = hub.rooms.get_mut("ABC").unwrap().native.as_mut().unwrap();
    native.runtime.auto_pause(0, "Host", 1).unwrap();
    let before = native.runtime.state().start.economy.players[0].clone();
    let units = format!("{:?}", native.runtime.state().world.game.units);
    peers[0].send(&mut hub, json!({"t":"leave"}), 2);
    assert!(hub.rooms["ABC"].players[0].ai);
    assert_eq!(hub.rooms["ABC"].players[0].level, "normal");
    assert!(hub.rooms["ABC"].players[0].token.is_empty());
    assert!(hub.rooms["ABC"].players[0].connection.is_none());
    assert!(!runtime(&hub).paused());
    assert_eq!(runtime(&hub).state().start.economy.players[0], before);
    assert_eq!(
        format!("{:?}", runtime(&hub).state().world.game.units),
        units
    );
    assert!(matches!(
        runtime(&hub).ai_commander(0).unwrap().brain(),
        Brain::Human { .. }
    ));
    assert_eq!(
        runtime(&hub).ai_level(0),
        Some(ww2_core::commander_ai::Level::Normal)
    );
    // Reusing the former token must not resume an unrelated host pause.
    let native = hub.rooms.get_mut("ABC").unwrap().native.as_mut().unwrap();
    native
        .runtime
        .pause(1, "Other host", PauseReason::Host, 3)
        .unwrap();
    let pause = native.runtime.pause_message(3);
    assert_eq!(runtime(&hub).pause_message(4), pause);
    let mut returning = Peer::new(4);
    returning.send(
        &mut hub,
        json!({"t":"hello","token":"host","name":"Returned"}),
        5,
    );
    assert!(returning.joined);
    assert_eq!(runtime(&hub).pause_message(5), pause);
    assert!(hub.rooms["ABC"].players[0].ai);
    assert!(hub.rooms["ABC"].players[0].token.is_empty());
    assert!(
        hub.rooms["ABC"]
            .spectators
            .iter()
            .any(|p| p.connection.as_ref().is_some_and(|c| c.id == 4))
    );
    returning.drain(&mut hub, 5);
    peers[0] = returning;
    hub.rooms
        .get_mut("ABC")
        .unwrap()
        .native
        .as_mut()
        .unwrap()
        .runtime
        .resume(6);
    run_callbacks(&mut hub, &mut peers, 1, 12);
}

#[path = "net79_pause_tests.rs"]
mod pause;
