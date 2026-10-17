use super::*;
use ww2_core::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    match_runtime::{MatchOptions, PlayerConfig},
    weather::WeatherChoice,
};

pub(crate) fn native_match() -> Match {
    crate::authoritative_maps::create_match(
        "default",
        MatchOptions {
            seed: 0x3c1055,
            mode: Mode::Conquest,
            players: (0..2)
                .map(|slot| PlayerConfig {
                    name: format!("Commander {slot}"),
                    team: slot,
                    faction: Faction::try_from(slot as u8).unwrap(),
                })
                .collect(),
            shuffle: false,
            army: ArmySize::Standard,
            weather: WeatherChoice::Map,
            weather_seed: Some(1),
            living_weather: false,
            map_key: Some("default".into()),
            tech: true,
            supply: false,
            logistics: false,
            sandbox: false,
        },
    )
    .unwrap()
}
fn client(id: u64) -> (Connection, mpsc::Receiver<Output>) {
    let (tx, rx) = mpsc::channel(64);
    let (cancel, _) = watch::channel(false);
    (Connection::new(id, tx, cancel), rx)
}
fn join(h: &mut Hub, c: &Connection, token: &str, now: u64) {
    h.message(
        "ABC",
        c,
        &mut false,
        json!({"t":"hello","token":token,"name":token}),
        now,
    );
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
    while ra.try_recv().is_ok() {}
    while rb.try_recv().is_ok() {}
    (hub, a, ra, b, rb)
}
fn write(rx: &mut mpsc::Receiver<Output>) -> SnapshotWrite {
    loop {
        match rx.try_recv().unwrap() {
            Output::Snapshot(write) => return write,
            Output::Json(_) | Output::Watcher(_) => {}
            Output::Close(_, _) => panic!("unexpected close"),
        }
    }
}
fn initial_start(rx: &mut mpsc::Receiver<Output>) {
    for kind in ["pause", "lobby", "start"] {
        let Output::Json(message) = rx.try_recv().unwrap() else {
            panic!("initial setup precedes snapshots")
        };
        assert_eq!(message["t"], kind);
        match kind {
            "pause" => assert_eq!(message["paused"], false),
            "lobby" => assert_eq!(message["state"], "play"),
            "start" => {
                assert!(message["fog"].is_object());
                assert!(message["cells"].is_array());
            }
            _ => unreachable!(),
        }
    }
}
#[test]
fn native_attachment_admits_before_roster_or_account_publication() {
    let (mut h, _, mut ra, _, mut rb) = room();
    let mut invalid = native_match();
    invalid.world.game.units.values_mut().next().unwrap().kind = usize::MAX;
    assert!(h.attach_native("ABC", invalid, 0).is_err());
    assert!(h.rooms["ABC"].native.is_none());
    assert!(h.accounts.is_empty());
    assert_eq!(h.next_epoch, 0);
    let mut mismatch = native_match();
    mismatch.options.players[1].faction = Faction::Uk;
    assert!(h.attach_native("ABC", mismatch, 0).is_err());
    assert!(h.accounts.is_empty());
    let mut wrong_map = native_match();
    wrong_map.options.map_key = Some("el-alamein".into());
    assert!(h.attach_native("ABC", wrong_map, 0).is_err());
    let mut wrong_layout = native_match();
    wrong_layout.start.players[0].spawn.x += 2.;
    assert!(h.attach_native("ABC", wrong_layout, 0).is_err());
    let mut wrong_settings = native_match();
    wrong_settings.options.army = ArmySize::Large;
    assert!(h.attach_native("ABC", wrong_settings, 0).is_err());
    assert!(h.rooms["ABC"].native.is_none());
    assert!(h.accounts.is_empty());
    assert_eq!(h.next_epoch, 0);
    assert!(ra.try_recv().is_err());
    assert!(rb.try_recv().is_err());
    assert_eq!(h.attach_native("ABC", native_match(), 0).unwrap(), 1);
    assert!(h.attach_native("ABC", native_match(), 0).is_err());
    assert_eq!(h.accounts.len(), 2);
    h.stop();
    let tick = h.rooms["ABC"]
        .native
        .as_ref()
        .unwrap()
        .runtime
        .state()
        .world
        .game
        .tick;
    h.poll_native(10_000, 10_000_000_000);
    assert_eq!(
        h.rooms["ABC"]
            .native
            .as_ref()
            .unwrap()
            .runtime
            .state()
            .world
            .game
            .tick,
        tick
    );
    assert!(h.native_status("ABC").is_err());
}
#[test]
fn startup_identity_exhaustion_does_not_publish_an_unsafe_client_number() {
    let (mut h, _, mut ra, _, mut rb) = room();
    h.next_epoch = crate::native_startup::MAX_MATCH_ID;
    assert!(h.attach_native("ABC", native_match(), 0).is_err());
    assert!(h.rooms["ABC"].native.is_none());
    assert!(h.accounts.is_empty());
    assert_eq!(h.next_epoch, crate::native_startup::MAX_MATCH_ID);
    assert!(ra.try_recv().is_err());
    assert!(rb.try_recv().is_err());
}
#[test]
fn attached_roster_reconnects_in_place_and_stale_writes_only_drain_old_accounts() {
    let (mut h, a, mut ra, _, mut rb) = room();
    h.attach_native("ABC", native_match(), 0).unwrap();
    for command in [
        json!({"t":"spectate"}),
        json!({"t":"addAi"}),
        json!({"t":"team","slot":1,"v":0}),
        json!({"t":"faction","slot":0,"v":3}),
    ] {
        h.message("ABC", &a, &mut true, command, 0);
    }
    let (newcomer, mut newcomer_rx) = client(3);
    join(&mut h, &newcomer, "new", 0);
    assert_eq!(h.rooms["ABC"].players.len(), 2);
    assert_eq!(h.rooms["ABC"].spectators.len(), 1);
    while newcomer_rx.try_recv().is_ok() {}
    assert_eq!(h.rooms["ABC"].players[1].team, 1);
    assert_eq!(h.rooms["ABC"].players[0].faction, 0);
    h.poll_native(0, 0);
    h.poll_native(50, 50_000_000);
    let old = write(&mut ra);
    let other = write(&mut rb);
    assert!(h.accounts[&1].pressure.inflight() > 0);
    let (replacement, mut rr) = client(4);
    join(&mut h, &replacement, "a", 51);
    let new_history = h.accounts[&4].delivery.clone();
    h.complete_snapshot(&old, true, 52);
    assert_eq!(h.accounts[&1].pressure.inflight(), 0);
    assert_eq!(h.accounts[&4].delivery, new_history);
    assert_eq!(h.accounts[&4].pressure.inflight(), 0);
    assert!(!h.accounts[&4].full_terrain);
    assert!(!matches!(rr.try_recv(), Ok(Output::Snapshot(_))));
    h.complete_snapshot(&other, true, 52);
    h.disconnected("ABC", 1, 53);
    assert_eq!(h.rooms["ABC"].players[0].connection.as_ref().unwrap().id, 4);
    h.disconnected("ABC", 4, 54);
    h.expire(10_054);
    assert_eq!(h.rooms["ABC"].players.len(), 2);
    assert!(h.rooms["ABC"].players[0].connection.is_none());
    h.retire_connection(1);
    assert!(!h.accounts.contains_key(&1));
}
#[test]
fn refused_enqueue_drops_one_connection_and_old_epoch_completions_only_drain() {
    let (mut h, a, mut ra, _, mut rb) = room();
    h.attach_native("ABC", native_match(), 0).unwrap();
    initial_start(&mut ra);
    initial_start(&mut rb);
    for _ in 0..64 {
        a.send(json!({"t":"pong"}));
    }
    h.poll_native(0, 0);
    h.poll_native(50, 50_000_000);
    assert!(h.native_status("ABC").unwrap().fault.is_none());
    assert!(h.native_status("ABC").unwrap().paused);
    assert!(h.rooms["ABC"].players[0].connection.is_none());
    assert_eq!(h.accounts[&1].pressure.inflight(), 0);
    while let Ok(output) = ra.try_recv() {
        assert!(!matches!(output, Output::Snapshot(_)));
    }
    let healthy = write(&mut rb);
    h.complete_snapshot(&healthy, true, 51);
    assert_eq!(h.accounts[&2].pressure.inflight(), 0);
    let (replacement, mut replacement_rx) = client(3);
    join(&mut h, &replacement, "a", 52);
    assert!(!h.native_status("ABC").unwrap().paused);
    assert!(!h.accounts[&3].full_terrain);
    h.poll_native(100, 100_000_000);
    h.poll_native(150, 150_000_000);
    let full = write(&mut replacement_rx);
    let payload: Value = serde_json::from_str(&full.json).unwrap();
    assert_eq!(
        payload["held"][0].as_u64().unwrap() as usize,
        payload["units"].as_array().unwrap().len()
    );
    h.complete_snapshot(&full, true, 151);
    assert!(h.native_status("ABC").unwrap().fault.is_none());

    let (mut h, _, mut ra, _, mut rb) = room();
    let epoch = h.attach_native("ABC", native_match(), 0).unwrap();
    h.poll_native(0, 0);
    h.poll_native(50, 50_000_000);
    let old = write(&mut ra);
    let other = write(&mut rb);
    h.rooms.remove("ABC");
    let (new, _) = client(9);
    let (new2, _) = client(10);
    join(&mut h, &new, "new", 60);
    join(&mut h, &new2, "new2", 60);
    assert!(h.attach_native("ABC", native_match(), 60).unwrap() > epoch);
    h.complete_snapshot(&old, false, 61);
    h.complete_snapshot(&other, false, 61);
    assert!(h.native_status("ABC").unwrap().fault.is_none());
    assert_eq!(h.accounts[&1].pressure.inflight(), 0);
    assert_eq!(h.accounts[&2].pressure.inflight(), 0);
    assert_eq!(h.accounts[&9].pressure.inflight(), 0);
}

#[test]
fn absent_and_unready_connections_are_distinct_and_expiry_precedes_callbacks() {
    let (mut h, _, mut ra, _, mut rb) = room();
    h.attach_native("ABC", native_match(), 0).unwrap();
    let before = h.accounts[&1].delivery.clone();
    ra.close();
    rb.close();
    h.poll_native(0, 0);
    h.poll_native(50, 50_000_000);
    let native = &h.rooms["ABC"].native.as_ref().unwrap().runtime;
    assert_eq!(native.state().world.game.tick, 2);
    assert!(
        native
            .state()
            .start
            .economy
            .players
            .iter()
            .all(|player| !player.away)
    );
    assert_eq!(h.accounts[&1].delivery, before);
    assert!(!h.accounts[&1].full_terrain);
    assert_eq!(h.accounts[&1].pressure.inflight(), 0);
    // An expired native room must not run a pause callback or any due tick.
    h.rooms.get_mut("ABC").unwrap().empty_since = Some(0);
    h.poll_native(60_001, 60_001_000_000);
    assert!(!h.rooms.contains_key("ABC"));
    assert!(h.room_order.is_empty());
    assert_eq!(h.accounts[&1].delivery, before);
}

#[test]
fn failed_current_write_drops_and_resumes_only_its_stable_seat_once() {
    let (mut h, _, mut ra, _, mut rb) = room();
    h.attach_native("ABC", native_match(), 0).unwrap();
    h.poll_native(0, 0);
    h.poll_native(50, 50_000_000);
    let failed = write(&mut ra);
    let other = write(&mut rb);
    h.complete_snapshot(&failed, false, 51);
    assert_eq!(h.accounts[&1].pressure.inflight(), 0);
    assert!(h.native_status("ABC").unwrap().paused);
    assert!(h.native_status("ABC").unwrap().fault.is_none());
    assert!(h.rooms["ABC"].players[0].connection.is_none());
    let (new_b, _rb) = client(3);
    join(&mut h, &new_b, "b", 52);
    assert!(h.native_status("ABC").unwrap().paused);
    h.complete_snapshot(&other, true, 53);
    let (new_a, _ra) = client(4);
    join(&mut h, &new_a, "a", 54);
    assert!(!h.native_status("ABC").unwrap().paused);
    assert_eq!(h.native_status("ABC").unwrap().tick, 2);
    h.disconnected("ABC", 4, 55);
    assert!(!h.native_status("ABC").unwrap().paused);
    h.rooms
        .get_mut("ABC")
        .unwrap()
        .native
        .as_mut()
        .unwrap()
        .runtime
        .pause(1, "b", crate::room_runtime::PauseReason::Host, 56)
        .unwrap();
    let (another_a, _ra) = client(5);
    join(&mut h, &another_a, "a", 57);
    assert!(h.native_status("ABC").unwrap().paused);
    assert!(h.native_status("ABC").unwrap().fault.is_none());
}

#[test]
fn final_hold_frame_precedes_result_and_old_tickets_survive_new_attachment() {
    let (mut h, _, mut ra, _, mut rb) = room();
    let mut state = native_match();
    state
        .record_story_tally(Some(0), ww2_core::match_story::StoryKey::Kills, 3.4)
        .unwrap();
    state
        .record_story_tally(Some(1), ww2_core::match_story::StoryKey::Losses, 2.4)
        .unwrap();
    state.start.economy.players[0].vp = state.start.win_vp;
    let victory = state.apply_conquest_victory().unwrap().unwrap();
    let old_epoch = h.attach_native("ABC", state, 0).unwrap();
    for callback in 0..119_u64 {
        h.poll_native(callback * 50, callback * 50_000_000);
        for receiver in [&mut ra, &mut rb] {
            while let Ok(output) = receiver.try_recv() {
                if let Output::Snapshot(write) = output {
                    h.complete_snapshot(&write, true, callback * 50);
                }
            }
        }
    }
    assert!(!h.native_status("ABC").unwrap().ending_complete);
    assert_eq!(h.native_status("ABC").unwrap().held, 119);
    h.poll_native(5950, 5_950_000_000);
    let mut final_writes = Vec::new();
    for (slot, receiver) in [&mut ra, &mut rb].into_iter().enumerate() {
        let Output::Snapshot(frame) = receiver.try_recv().unwrap() else {
            panic!("final frame must precede result")
        };
        assert_eq!(frame.epoch, old_epoch);
        let Output::Json(resume) = receiver.try_recv().unwrap() else {
            panic!("expected resume before lobby")
        };
        assert_eq!(resume["paused"], false);
        let Output::Json(lobby) = receiver.try_recv().unwrap() else {
            panic!("expected result lobby")
        };
        let result = &lobby["result"];
        assert_eq!(lobby["state"], "lobby");
        assert_eq!(result["winner"], 0);
        assert_eq!(result["teams"], json!([0, 1]));
        assert_eq!(result["names"], json!(["a", "b"]));
        assert_eq!(result["reason"], "vp");
        assert_eq!(result["endTick"], victory.end_tick);
        assert_eq!(result["at"]["x"].as_f64(), Some(victory.end_at.x));
        assert_eq!(result["story"][0]["kills"].as_f64(), Some(3.0));
        assert_eq!(result["story"][1]["losses"].as_f64(), Some(2.0));
        assert_eq!(result["you"]["team"], slot);
        assert_eq!(
            result["you"]["outcome"],
            if slot == 0 { "victory" } else { "defeat" }
        );
        assert_eq!(result.as_object().unwrap().len(), 11);
        final_writes.push(frame);
    }
    let completed = h.native_status("ABC").unwrap();
    assert_eq!(completed.tick, 60);
    assert_eq!(completed.held, 120);
    assert!(completed.ending_complete);
    assert!(h.rooms["ABC"].native.is_none());
    let outstanding = h.accounts[&1].pressure.inflight();
    assert!(outstanding > 0);
    let new_epoch = h.attach_native("ABC", native_match(), 6000).unwrap();
    initial_start(&mut ra);
    initial_start(&mut rb);
    assert!(new_epoch > old_epoch);
    assert_eq!(h.accounts[&1].pressure.inflight(), outstanding);
    let history = h.accounts[&1].delivery.clone();
    for frame in &final_writes {
        h.complete_snapshot(frame, true, 6001);
    }
    assert_eq!(h.accounts[&1].pressure.inflight(), 0);
    assert_eq!(h.accounts[&1].delivery, history);
    assert!(!h.accounts[&1].full_terrain);
    assert_eq!(h.native_status("ABC").unwrap().tick, 0);
    assert!(ra.try_recv().is_err());
    h.poll_native(6000, 6_000_000_000);
    h.poll_native(6050, 6_050_000_000);
    let next = write(&mut ra);
    assert_eq!(next.epoch, new_epoch);
    assert!(next.ticket > final_writes[0].ticket);
    h.complete_snapshot(&next, true, 6051);
}

#[test]
fn original_result_roster_rows_follow_people_across_removal_and_refill() {
    let fixture: Value =
        serde_json::from_str(include_str!("../tests/fixtures/room-lobby-result.json")).unwrap();
    assert_eq!(
        fixture["reference"],
        "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
    );
    for trace in fixture["traces"].as_array().unwrap() {
        let (mut hub, _, _, _, _) = room();
        let (third, _receiver) = client(3);
        join(&mut hub, &third, "c", 0);
        let room = hub.rooms.get_mut("ABC").unwrap();
        for (person, initial) in room
            .players
            .iter_mut()
            .zip(trace["initial"]["players"].as_array().unwrap())
        {
            assert_eq!(person.name, initial["name"].as_str().unwrap());
            person.last_match = Some(initial["lastMatch"].clone());
        }
        room.result = (!trace["initial"]["result"].is_null()).then(|| RoomResult {
            report: trace["initial"]["result"].clone(),
            status: crate::NativeRoomStatus {
                epoch: 1,
                tick: 0,
                held: 120,
                ending_complete: true,
                paused: false,
                fault: None,
            },
        });
        let mut removed = BTreeMap::new();
        for action in trace["steps"].as_array().unwrap() {
            match action["op"].as_str().unwrap() {
                "retain" => {
                    let kept = action["names"].as_array().unwrap();
                    for person in room.retain_seats(|_, person| {
                        kept.iter()
                            .any(|name| name.as_str() == Some(person.name.as_str()))
                    }) {
                        removed.insert(person.name.clone(), person);
                    }
                }
                "add" => {
                    let name = action["name"].as_str().unwrap();
                    let person = removed.remove(name).unwrap_or_else(|| Person {
                        token: name.encode_utf16().collect(),
                        name: name.into(),
                        connection: None,
                        ai: false,
                        team: 0,
                        faction: 0,
                        level: "normal",
                        disconnected: None,
                        denies: Vec::new(),
                        rtt: None,
                        map_pings: Vec::new(),
                        diagnostics: crate::diagnostics::PlayerDiagnostics::default(),
                        last_match: Some(
                            json!({"team":name.as_bytes()[0] - b'a',"outcome":"victory"}),
                        ),
                    });
                    room.add_seat(person);
                }
                _ => panic!("unknown original roster action"),
            }
            let actual = json!({"players":room.players.iter().map(|person| json!({"name":person.name,"lastMatch":person.last_match})).collect::<Vec<_>>(),
                "result":room.result.as_ref().map(|result| &result.report)});
            assert_eq!(actual, action["expected"], "{} {action}", trace["name"]);
        }
    }
}

#[path = "native_mode_predicates.rs"]
mod mode_predicates;

#[test]
fn manual_end_restart_old_completion_cannot_consume_current_snapshot_ticket() {
    let (mut h, a, mut ra, _, mut rb) = room();
    let old_epoch = h.attach_native("ABC", native_match(), 0).unwrap();
    initial_start(&mut ra);
    initial_start(&mut rb);
    h.poll_native(0, 0);
    h.poll_native(50, 50_000_000);
    let old = write(&mut ra);
    let old_peer = write(&mut rb);
    let old_bytes = h.accounts[&a.id].pressure.inflight();
    assert!(old_bytes > 0);

    h.message("ABC", &a, &mut true, json!({"t":"end"}), 51);
    assert!(h.rooms["ABC"].native.is_none());
    for receiver in [&mut ra, &mut rb] {
        while let Ok(output) = receiver.try_recv() {
            assert!(matches!(output, Output::Json(_)));
        }
    }
    let new_epoch = h.attach_native("ABC", native_match(), 52).unwrap();
    assert!(new_epoch > old_epoch);
    initial_start(&mut ra);
    initial_start(&mut rb);
    h.poll_native(100, 100_000_000);
    h.poll_native(150, 150_000_000);
    let current = write(&mut ra);
    let current_peer = write(&mut rb);
    assert_eq!(current.connection_id, old.connection_id);
    assert_eq!(current.epoch, new_epoch);
    assert!(current.ticket > old.ticket);
    let current_bytes = current.json.encode_utf16().count() as u64;
    assert_eq!(
        h.accounts[&a.id].pressure.inflight(),
        old_bytes + current_bytes
    );
    let history = h.accounts[&a.id].delivery.clone();

    // The old callback arrives after the new write has already been reserved.
    h.complete_snapshot(&old, true, 151);
    assert_eq!(h.accounts[&a.id].pressure.inflight(), current_bytes);
    assert_eq!(h.accounts[&a.id].delivery, history);
    assert!(ra.try_recv().is_err());
    h.complete_snapshot(&current, true, 152);
    h.complete_snapshot(&old_peer, true, 152);
    h.complete_snapshot(&current_peer, true, 152);
    assert_eq!(h.accounts[&a.id].pressure.inflight(), 0);
    assert!(h.native_status("ABC").unwrap().fault.is_none());

    h.poll_native(200, 200_000_000);
    h.poll_native(250, 250_000_000);
    for receiver in [&mut ra, &mut rb] {
        let next = write(receiver);
        assert_eq!(next.epoch, new_epoch);
        h.complete_snapshot(&next, true, 251);
    }
    assert!(h.native_status("ABC").unwrap().fault.is_none());
    assert!(
        h.accounts
            .values()
            .all(|account| account.pressure.inflight() == 0)
    );
}
#[path = "mode_server_lifecycle_tests.rs"]
mod mode_server_lifecycle;
