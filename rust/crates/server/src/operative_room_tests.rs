use super::*;
use ww2_core::recipient_snapshot::{DeliveryState, OperativeDelivery, SelectedSeat};

fn assert_every_recipient_tick(
    state: &mut Match,
    observer: &mut DeliveryState,
    operatives: &mut [OperativeDelivery; 2],
) {
    let selected = (0..2)
        .map(|slot| SelectedSeat {
            slot,
            // Full reads preserve the production recipient's terrain outbox.
            full: true,
            omit: false,
        })
        .collect::<Vec<_>>();
    let tick = state.world.game.tick;
    let frame = observer
        .capture_selected_with_tick_events(state, &selected, &[])
        .unwrap()
        .unwrap();
    for slot in 0..2 {
        let snapshot = frame.recipient(slot).unwrap().into_json();
        assert_eq!(snapshot["tick"], tick);
        assert!(snapshot["units"].is_array());
    }
    drop(frame);
    for stream in operatives {
        let snapshot = stream.capture(state, false, &[]).unwrap();
        assert_eq!(snapshot["tick"], tick);
        assert!(snapshot["units"].is_array());
    }
}

#[test]
fn operative_load_merge_retains_membership_and_remaps_the_commander_person() {
    // Prototype server.js:436-450,411. Loading replaces the seat array, while
    // each operative keeps the same commander person and receives a fresh start.
    struct SaveDir(std::path::PathBuf);
    impl Drop for SaveDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let dir =
        SaveDir(std::env::temp_dir().join(format!("ww2-operative-load-{}", std::process::id())));
    let (mut hub, seats, mut commanders) = room();
    hub.configure_saves(dir.0.clone());
    let (op, mut receiver) = client(3);
    let (peer, mut peer_rx) = client(4);
    join_operative(&mut hub, &op, "friend", 0);
    join_operative(&mut hub, &peer, "peer", 1);
    hub.attach_native("ABC", native_tests::native_match(), 0)
        .unwrap();
    let soldier = state(&hub).companions.get("friend").unwrap().unit_id;
    command(&mut hub, &op, fps(8), 20);
    let saved = hub.save_room("ABC", seats[0].id).unwrap();
    messages(&mut receiver);
    messages(&mut peer_rx);
    drain_commanders(&mut hub, &mut commanders, 20);
    {
        let room = hub.rooms.get_mut("ABC").unwrap();
        room.native = None;
        // Model a lobby whose present people differ from saved-seat order.
        room.players.swap(0, 1);
        room.operatives[0].commander = Some(1);
        room.operatives[1].commander = Some(0);
    }
    assert!(hub.load_room("ABC", seats[1].id, "missing", 49).is_err());
    assert_eq!(hub.rooms["ABC"].operatives[0].commander, Some(1));
    assert_eq!(hub.rooms["ABC"].operatives[1].commander, Some(0));
    assert!(messages(&mut receiver).is_empty());
    assert!(messages(&mut peer_rx).is_empty());
    let epoch = hub.load_room("ABC", seats[1].id, &saved, 50).unwrap();
    assert!(hub.native_status("ABC").unwrap().paused);
    let seat_output = drain_commanders(&mut hub, &mut commanders, 50);
    for (slot, output) in [messages(&mut receiver), messages(&mut peer_rx)]
        .iter()
        .enumerate()
    {
        let lobby = output.iter().find(|m| m["t"] == "lobby").unwrap();
        let start = output.iter().find(|m| m["t"] == "start").unwrap();
        assert_eq!(lobby["commander"], slot);
        assert_eq!(lobby["role"], "operative");
        assert_eq!(start["matchId"], epoch);
        let commander_start = seat_output[slot]
            .iter()
            .find(|m| m["t"] == "start")
            .unwrap();
        let key = if slot == 0 { "friend" } else { "peer" };
        assert_start(
            start,
            commander_start,
            slot,
            state(&hub).companions.get(key).unwrap().unit_id,
        );
        assert_eq!(output.last().unwrap()["t"], "pause");
        assert_eq!(output.last().unwrap()["paused"], true);
    }
    assert_eq!(
        state(&hub).companions.get("friend").unwrap().unit_id,
        soldier
    );
    assert_eq!(last_seq(&hub, "friend"), -1);
    assert_eq!(hub.rooms["ABC"].operatives.len(), 2);
    // Operatives cannot use the loaded-save host-resume handler.
    command(&mut hub, &op, json!({"t":"resume"}), 51);
    assert!(messages(&mut receiver).is_empty());
    assert!(hub.native_status("ABC").unwrap().paused);
    command(&mut hub, &seats[0], json!({"t":"resume"}), 52);
    assert!(!hub.native_status("ABC").unwrap().paused);
    let mut observer = DeliveryState::new(state(&hub)).unwrap();
    let mut op_observers = [
        OperativeDelivery::new(state(&hub), 0).unwrap(),
        OperativeDelivery::new(state(&hub), 1).unwrap(),
    ];
    for callback in 0..=8_u64 {
        let now = 100 + callback * 50;
        hub.poll_native(now, now * 1_000_000);
        let state = hub
            .rooms
            .get_mut("ABC")
            .unwrap()
            .native
            .as_mut()
            .unwrap()
            .runtime
            .state_mut();
        assert_every_recipient_tick(state, &mut observer, &mut op_observers);
        drain_commanders(&mut hub, &mut commanders, now);
        messages(&mut receiver);
        messages(&mut peer_rx);
        assert!(hub.native_status("ABC").unwrap().fault.is_none());
    }
    assert!(hub.native_status("ABC").unwrap().tick >= 8);
}
