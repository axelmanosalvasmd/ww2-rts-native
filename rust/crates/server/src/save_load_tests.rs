use super::*;
use crate::saves::SaveStore;
use std::path::PathBuf;

fn assert_same_json(a: &Value, b: &Value) {
    fn difference(a: &Value, b: &Value, path: &str) -> Option<String> {
        if a == b {
            return None;
        }
        match (a, b) {
            (Value::Array(a), Value::Array(b)) if a.len() == b.len() => a
                .iter()
                .zip(b)
                .enumerate()
                .find_map(|(i, (a, b))| difference(a, b, &format!("{path}[{i}]"))),
            (Value::Object(a), Value::Object(b)) if a.len() == b.len() => {
                a.iter().find_map(|(key, a)| {
                    b.get(key).map_or_else(
                        || Some(format!("{path}: missing {key}")),
                        |b| difference(a, b, &format!("{path}.{key}")),
                    )
                })
            }
            _ => Some(format!(
                "{path}: {} != {}",
                a.to_string().chars().take(100).collect::<String>(),
                b.to_string().chars().take(100).collect::<String>()
            )),
        }
    }
    assert!(a == b, "{}", difference(a, b, "$").unwrap_or_default());
}

struct TestDir(PathBuf);
impl TestDir {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        Self(std::env::temp_dir().join(format!(
            "ww2-save-load-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )))
    }
}
impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn peer(id: u64) -> (Connection, mpsc::Receiver<Output>) {
    let (tx, rx) = mpsc::channel(64);
    let (cancel, _) = watch::channel(false);
    (Connection::new(id, tx, cancel), rx)
}
fn messages(hub: &mut Hub, rx: &mut mpsc::Receiver<Output>) -> Vec<Value> {
    let mut out = Vec::new();
    while let Ok(output) = rx.try_recv() {
        match output {
            Output::Json(value) => out.push(value),
            Output::Snapshot(write) => {
                out.push(serde_json::from_str(&write.json).unwrap());
                hub.complete_snapshot(&write, true, 0);
            }
            Output::Watcher(_) => panic!("unexpected spectator snapshot on a seated connection"),
            Output::Close(code, reason) => panic!("unexpected close {code}: {reason}"),
        }
    }
    out
}
fn small_match() -> (Match, Value) {
    let raw = json!({"name":"Save match","w":24,"h":24,"trenchFacing":true,"rows":vec![".".repeat(24);24],
        "spawns":[{"x":3,"y":3},{"x":20,"y":20}],"points":[{"id":"front","x":12,"y":12,"vp":1,"mp":1}]});
    let map =
        ww2_core::map::AuthoritativeMap::from_json(&serde_json::to_vec(&raw).unwrap()).unwrap();
    let mut options = native_tests::native_match().options;
    options.logistics = true;
    options.living_weather = true;
    let mut state = Match::new(&map, options).unwrap();
    state.start.win_vp = 1.;
    for control in state.commands.units.values_mut() {
        control.hold_fire = true;
        control.auto_cast = false;
        control.auto_retreat = false;
    }
    state.world.set_cell(0, b'T').unwrap();
    let unit = state
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 0 && state.rules.kinds[u.kind].name == "rifle")
        .unwrap()
        .id;
    state
        .enqueue_command(
            ww2_core::orders::OrderKey {
                tick: 1,
                issuer: 0,
                sequence: 1,
            },
            ww2_core::match_commands::Command::from_json(
                &json!({"t":"move","orders":[[unit,24.,24.]]}),
            )
            .unwrap(),
        )
        .unwrap();
    state
        .enqueue_command(
            ww2_core::orders::OrderKey {
                tick: 60,
                issuer: 0,
                sequence: 2,
            },
            ww2_core::match_commands::Command::from_json(
                &json!({"t":"stance","ids":[unit],"key":"autoRetreat","on":true}),
            )
            .unwrap(),
        )
        .unwrap();
    state
        .enqueue_command(
            ww2_core::orders::OrderKey {
                tick: 61,
                issuer: 0,
                sequence: 3,
            },
            ww2_core::match_commands::Command::from_json(
                &json!({"t":"stance","ids":[unit],"key":"autoRetreat","on":false}),
            )
            .unwrap(),
        )
        .unwrap();
    (state, raw)
}
fn setup(
    dir: &TestDir,
) -> (
    Hub,
    Connection,
    mpsc::Receiver<Output>,
    Connection,
    mpsc::Receiver<Output>,
) {
    let mut hub = Hub::new(32, String::new());
    hub.configure_saves(dir.0.clone());
    let (a, mut ra) = peer(1);
    let (b, mut rb) = peer(2);
    for (client, token) in [(&a, "host"), (&b, "guest")] {
        hub.message(
            "ABC",
            client,
            &mut false,
            json!({"t":"hello","token":token,"name":token}),
            0,
        );
    }
    let (state, raw) = small_match();
    let prepared = hub
        .prepare_native_room(&hub.rooms["ABC"], state, 0, false, Some(&raw))
        .unwrap();
    hub.publish_native("ABC", prepared);
    messages(&mut hub, &mut ra);
    messages(&mut hub, &mut rb);
    (hub, a, ra, b, rb)
}
fn envelope() -> Value {
    json!({"version":1,"g":{"tick":17,"units":[[1,"rifle",-1]]},"guard":{"team":-1},
    "map":{"rows":["N"],"triggers":[{"at":4,"say":"private"}]},"seats":[],"settings":{},"ai":[{"memory":{"id":7}}]})
}
#[test]
fn net91_save_storage_round_trip_retains_content_and_per_room_per_kind_limits() {
    let dir = TestDir::new();
    let store = SaveStore::new(dir.0.clone());
    let state = envelope();
    let mut newest = String::new();
    for n in 0..12 {
        newest = store
            .write(
                "ABC",
                "manual",
                &state,
                json!({"mode":"conquest","minutes":1,"names":["A"]}),
                1_800_000_000_000 + n,
            )
            .unwrap();
    }
    for n in 0..5 {
        store
            .write(
                "ABC",
                "auto",
                &state,
                json!({"mode":"world","minutes":2,"names":["A"],"world":{"total":5,"owned":[2]}}),
                1_800_000_001_000 + n,
            )
            .unwrap();
    }
    store
        .write(
            "XYZ",
            "manual",
            &state,
            json!({"mode":"conquest","minutes":0,"names":["B"]}),
            1_800_000_002_000,
        )
        .unwrap();
    assert_eq!(store.read("ABC", &newest).unwrap(), state);
    assert_eq!(
        store
            .list("ABC")
            .iter()
            .filter(|row| row["kind"] == "manual")
            .count(),
        10
    );
    assert_eq!(
        store
            .list("ABC")
            .iter()
            .filter(|row| row["kind"] == "auto")
            .count(),
        3
    );
    assert_eq!(store.list("XYZ").len(), 1);
    assert!(!dir.0.join("ABC-1800000000000-manual.save").exists());
    assert!(!dir.0.join("ABC-1800000000000-manual.json").exists());
    assert!(std::fs::read_dir(&dir.0).unwrap().all(|entry| {
        !entry
            .unwrap()
            .path()
            .extension()
            .is_some_and(|s| s == "tmp")
    }));
    assert!(store.read("XYZ", &newest).is_err());
    for id in [
        "../ABC-1800000000000-manual",
        "ABC-1-manual",
        "ABC-1800000000000-manual/..",
        "AB-1800000000000-auto",
    ] {
        assert!(store.read("ABC", id).is_err());
    }
}
#[test]
fn net28_net93_lobby_save_list_is_synchronous_private_and_newest_first() {
    let dir = TestDir::new();
    let (mut hub, a, mut ra, _, mut rb) = setup(&dir);
    let store = hub.saves.clone();
    let first = store
        .write(
            "ABC",
            "manual",
            &envelope(),
            json!({"mode":"conquest","minutes":1,"names":["First"]}),
            1_800_000_000_000,
        )
        .unwrap();
    hub.rooms["ABC"].broadcast(&hub.maps, &hub.public_url);
    let second=store.write("ABC","auto",&envelope(),json!({"mode":"world","minutes":2,"names":["Second"],"world":{"total":8,"owned":[3,2]}}),1_800_000_000_001).unwrap();
    store
        .write(
            "XYZ",
            "manual",
            &envelope(),
            json!({"mode":"conquest","minutes":0,"names":["Other"]}),
            1_800_000_000_002,
        )
        .unwrap();
    std::fs::write(dir.0.join("ABC-1800000000003-manual.json"), b"{broken").unwrap();
    hub.rooms["ABC"].broadcast(&hub.maps, &hub.public_url);
    for receiver in [&mut ra, &mut rb] {
        let received = messages(&mut hub, receiver);
        assert_eq!(received.len(), 2);
        assert_eq!(
            received[0]["saves"],
            json!([{"id":first,"kind":"manual","savedAt":1_800_000_000_000u64,"mode":"conquest","minutes":1,"names":["First"]}])
        );
        assert_eq!(
            received[1]["saves"],
            json!([
            {"id":second,"kind":"auto","savedAt":1_800_000_000_001u64,"mode":"world","minutes":2,"names":["Second"],"world":{"total":8,"owned":[3,2]}},
            {"id":first,"kind":"manual","savedAt":1_800_000_000_000u64,"mode":"conquest","minutes":1,"names":["First"]}])
        );
        let text = received[1]["saves"].to_string();
        for secret in [
            "guard", "triggers", "private", "memory", "bytes", "token", "XYZ",
        ] {
            assert!(!text.contains(secret));
        }
    }
    assert_eq!(
        hub.rooms["ABC"].lobby(a.id, &hub.maps, &hub.public_url)["saves"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}
#[test]
fn net89_manual_save_requires_running_host_and_replies_before_lobby() {
    let dir = TestDir::new();
    let (mut hub, host, mut host_rx, guest, mut guest_rx) = setup(&dir);
    hub.message("ABC", &guest, &mut true, json!({"t":"save"}), 1);
    assert_eq!(
        messages(&mut hub, &mut guest_rx),
        vec![json!({"t":"deny","cmd":"save","reason":"save"})]
    );
    assert!(hub.saves.list("ABC").is_empty());
    hub.message("ABC", &host, &mut true, json!({"t":"save"}), 2);
    let received = messages(&mut hub, &mut host_rx);
    assert_eq!(received.len(), 2);
    assert_eq!(received[0]["t"], "saved");
    assert_eq!(received[1]["t"], "lobby");
    assert_eq!(received[1]["saves"][0]["id"], received[0]["id"]);
    assert_eq!(
        messages(&mut hub, &mut guest_rx)[0]["saves"],
        received[1]["saves"]
    );
    let saved = hub
        .saves
        .read("ABC", received[0]["id"].as_str().unwrap())
        .unwrap();
    assert_eq!(
        saved
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        vec!["ai", "g", "guard", "map", "seats", "settings", "version"]
    );
    assert_eq!(
        saved["settings"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        vec![
            "army",
            "defenderTeam",
            "logistics",
            "mapName",
            "mapSpawns",
            "mode",
            "tech",
            "weather",
            "worldSize"
        ]
    );
    assert_eq!(saved["map"]["name"], "Save match");
    assert_eq!(saved["seats"][0]["token"], "host");
    hub.rooms.get_mut("ABC").unwrap().native = None;
    hub.message("ABC", &host, &mut true, json!({"t":"save"}), 3);
    assert_eq!(
        messages(&mut hub, &mut host_rx),
        vec![json!({"t":"deny","cmd":"save","reason":"save"})]
    );
}
#[test]
fn save_load_running_match_restores_rng_orders_terrain_and_every_seat_through_victory() {
    let dir = TestDir::new();
    let (mut hub, host, mut host_rx, _, mut guest_rx) = setup(&dir);
    let selected = (0..2)
        .map(|slot| SelectedSeat {
            slot,
            full: true,
            omit: false,
        })
        .collect::<Vec<_>>();
    let runtime = &mut hub
        .rooms
        .get_mut("ABC")
        .unwrap()
        .native
        .as_mut()
        .unwrap()
        .runtime;
    for _ in 0..40 {
        runtime.advance_tick(&[false; 2]).unwrap();
        assert_eq!(
            runtime
                .collect_frame(&selected, FrameKind::Scheduled)
                .unwrap()
                .len(),
            2
        );
    }
    assert!(runtime.state().finish.is_none());
    assert_eq!(runtime.state().world.game.chars[0], b'T');
    assert_eq!(runtime.state().commands.pending_tick(60).count(), 1);
    let id = hub.save_room("ABC", host.id).unwrap();
    let saved = hub.saves.read("ABC", &id).unwrap();
    let restored = Match::from_save_json(saved["g"]["state"].clone()).unwrap();
    assert_same_json(&restored.save_json().unwrap(), &saved["g"]["state"]);
    let mut original = hub
        .rooms
        .get_mut("ABC")
        .unwrap()
        .native
        .take()
        .unwrap()
        .runtime;
    hub.load_room("ABC", host.id, &id, 5000).unwrap();
    messages(&mut hub, &mut host_rx);
    messages(&mut hub, &mut guest_rx);
    assert!(hub.native_status("ABC").unwrap().paused);
    hub.message("ABC", &host, &mut true, json!({"t":"resume"}), 5001);
    assert!(!hub.native_status("ABC").unwrap().paused);
    messages(&mut hub, &mut host_rx);
    messages(&mut hub, &mut guest_rx);
    let mut loaded = hub
        .rooms
        .get_mut("ABC")
        .unwrap()
        .native
        .take()
        .unwrap()
        .runtime;
    let mut finished = false;
    for _ in 0..3000 {
        original.advance_tick(&[false; 2]).unwrap();
        loaded.advance_tick(&[false; 2]).unwrap();
        let expected = original
            .collect_frame(&selected, FrameKind::Scheduled)
            .unwrap();
        let actual = loaded
            .collect_frame(&selected, FrameKind::Scheduled)
            .unwrap();
        assert_eq!(expected.len(), 2);
        assert_eq!(actual.len(), 2);
        for (a, b) in expected.iter().zip(&actual) {
            assert_same_json(&a.message, &b.message);
        }
        assert_eq!(original.state().start.random, loaded.state().start.random);
        let tick = loaded.state().world.game.tick;
        if matches!(tick, 60 | 61) {
            let actor = loaded
                .state()
                .world
                .game
                .units
                .values()
                .find(|u| u.owner == 0 && loaded.state().rules.kinds[u.kind].name == "rifle")
                .unwrap()
                .id;
            assert_eq!(
                loaded.state().commands.units[&actor].auto_retreat,
                tick == 60
            );
            assert_eq!(loaded.state().commands.pending_tick(60).count(), 0);
        }
        assert!(original.fault().is_none() && loaded.fault().is_none());
        if original.state().finish.is_some() {
            finished = true;
            break;
        }
    }
    assert!(
        finished,
        "capture and ordinary VP income must decide the match"
    );
    assert_eq!(original.state().finish.as_ref().unwrap().winner, 0);
    assert_eq!(
        original.state().finish.as_ref().unwrap().reason,
        ww2_core::match_end::EndReason::VictoryPoints
    );
    for _ in 0..7 {
        if loaded.hold_callback().unwrap().unwrap().advance {
            loaded.advance_tick(&[false; 2]).unwrap();
            assert_eq!(
                loaded
                    .collect_frame(&selected, FrameKind::Scheduled)
                    .unwrap()
                    .len(),
                2
            );
        }
    }
    let saved_ending = loaded.save_json().unwrap();
    assert_eq!(saved_ending["held"], 7);
    let mut ending = RoomRuntime::new(
        Match::from_save_json(saved_ending["state"].clone()).unwrap(),
        0,
    )
    .unwrap();
    ending.restore_saved_hold(&saved_ending["held"]).unwrap();
    ending
        .restore_saved_events(saved_ending["pendingEvents"].clone())
        .unwrap();
    for held in 8..=120 {
        let a = loaded.hold_callback().unwrap().unwrap();
        let b = ending.hold_callback().unwrap().unwrap();
        assert_eq!(a.advance, b.advance);
        assert_eq!(a.complete, b.complete);
        assert_eq!(ending.held(), held);
        if a.advance {
            loaded.advance_tick(&[false; 2]).unwrap();
            ending.advance_tick(&[false; 2]).unwrap();
            let expected = loaded
                .collect_frame(&selected, FrameKind::Scheduled)
                .unwrap();
            let actual = ending
                .collect_frame(&selected, FrameKind::Scheduled)
                .unwrap();
            for (a, b) in expected.iter().zip(&actual) {
                assert_same_json(&a.message, &b.message);
            }
        }
    }
    assert!(loaded.tick_failure().is_none());
    assert!(ending.tick_failure().is_none());
}
#[test]
fn net80_loaded_save_stays_host_paused_until_host_resume_and_refuses_versions_atomically() {
    let dir = TestDir::new();
    let (mut hub, host, mut ra, guest, mut rb) = setup(&dir);
    let id = hub.save_room("ABC", host.id).unwrap();
    hub.rooms.get_mut("ABC").unwrap().native = None;
    let mut invalid = hub.saves.read("ABC", &id).unwrap();
    invalid["version"] = json!(2);
    let mut zipper = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    zipper
        .write_all(&serde_json::to_vec(&invalid).unwrap())
        .unwrap();
    let bad = "ABC-1800000000000-manual";
    std::fs::write(dir.0.join(format!("{bad}.save")), zipper.finish().unwrap()).unwrap();
    let epoch = hub.next_epoch;
    for (sender, load) in [
        (&guest, id.as_str()),
        (&host, bad),
        (&host, "XYZ-1800000000000-manual"),
        (&host, "../escape"),
    ] {
        hub.message(
            "ABC",
            sender,
            &mut true,
            json!({"t":"loadSave","id":load}),
            1,
        );
        assert!(hub.rooms["ABC"].native.is_none());
        assert_eq!(hub.next_epoch, epoch);
    }
    assert_eq!(
        messages(&mut hub, &mut ra),
        vec![json!({"t":"deny","cmd":"loadSave","reason":"save"}); 3]
    );
    assert_eq!(
        messages(&mut hub, &mut rb),
        vec![json!({"t":"deny","cmd":"loadSave","reason":"save"})]
    );
    hub.message("ABC", &host, &mut true, json!({"t":"loadSave","id":id}), 2);
    for receiver in [&mut ra, &mut rb] {
        let rows = messages(&mut hub, receiver);
        assert!(
            rows.iter().position(|m| m["t"] == "lobby").unwrap()
                < rows.iter().position(|m| m["t"] == "start").unwrap()
        );
        assert_eq!(
            rows.last().unwrap(),
            &json!({"t":"pause","paused":true,"by":"host","reason":"host","left":0})
        );
    }
    let tick = hub.native_status("ABC").unwrap().tick;
    for beat in 0..=10 {
        hub.poll_native(beat * 60_000, beat * 50_000_000);
        messages(&mut hub, &mut ra);
        messages(&mut hub, &mut rb);
    }
    assert_eq!(hub.native_status("ABC").unwrap().tick, tick);
    assert!(hub.native_status("ABC").unwrap().paused);
    hub.message("ABC", &guest, &mut true, json!({"t":"resume"}), 600_001);
    assert!(hub.native_status("ABC").unwrap().paused);
    hub.message("ABC", &host, &mut true, json!({"t":"resume"}), 600_002);
    assert!(!hub.native_status("ABC").unwrap().paused);
    for beat in 11..=14 {
        hub.poll_native(600_002 + beat * 50, beat * 50_000_000);
        messages(&mut hub, &mut ra);
        messages(&mut hub, &mut rb);
    }
    let status = hub.native_status("ABC").unwrap();
    assert!(status.tick > tick);
    assert!(status.fault.is_none());
}
#[test]
fn load_token_matching_precedes_connected_fallback_and_preserves_current_names() {
    let dir = TestDir::new();
    let (mut hub, host, _host_rx, guest, _guest_rx) = setup(&dir);
    let id = hub.save_room("ABC", host.id).unwrap();
    hub.rooms.get_mut("ABC").unwrap().native = None;
    // The current host matches saved seat 1. Seat 0 needs the connected fallback.
    let room = hub.rooms.get_mut("ABC").unwrap();
    room.players[0].token = "guest".encode_utf16().collect();
    room.players[0].name = "Returning host".into();
    room.players[1].token = "new".encode_utf16().collect();
    room.players[1].name = "Replacement".into();
    hub.load_room("ABC", host.id, &id, 1).unwrap();
    assert_eq!(
        hub.rooms["ABC"]
            .players
            .iter()
            .map(|p| p.name.as_str())
            .collect::<Vec<_>>(),
        vec!["Replacement", "Returning host"]
    );
    assert_eq!(
        hub.rooms["ABC"].players[0].connection.as_ref().unwrap().id,
        guest.id
    );
    assert_eq!(
        hub.rooms["ABC"].players[1].connection.as_ref().unwrap().id,
        host.id
    );
    assert_eq!(hub.rooms["ABC"].players[0].team, 0);
    assert_eq!(hub.rooms["ABC"].players[1].team, 1);
}

#[test]
fn save_between_frames_restores_unsent_events_once_without_consuming_live_terrain() {
    let dir = TestDir::new();
    let (mut hub, host, mut ra, _, mut rb) = setup(&dir);
    let runtime = &mut hub
        .rooms
        .get_mut("ABC")
        .unwrap()
        .native
        .as_mut()
        .unwrap()
        .runtime;
    runtime.advance_tick(&[false; 2]).unwrap();
    let actor = runtime
        .state()
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 0)
        .unwrap();
    let event = ww2_core::match_tick::TickEvent::Environment(ww2_core::environment::Effect::Hurt(
        ww2_core::environment::Hurt {
            unit: actor.id,
            owner: 0,
            at: actor.at(),
            killed: false,
            from: None,
            local: false,
        },
    ));
    let before = runtime.state().save_json().unwrap();
    runtime
        .restore_saved_events(Match::save_events_json(&[event]).unwrap())
        .unwrap();
    assert_same_json(&runtime.state().save_json().unwrap(), &before);
    let id = hub.save_room("ABC", host.id).unwrap();
    let mut original = hub
        .rooms
        .get_mut("ABC")
        .unwrap()
        .native
        .take()
        .unwrap()
        .runtime;
    hub.load_room("ABC", host.id, &id, 1).unwrap();
    messages(&mut hub, &mut ra);
    messages(&mut hub, &mut rb);
    let mut loaded = hub
        .rooms
        .get_mut("ABC")
        .unwrap()
        .native
        .take()
        .unwrap()
        .runtime;
    let selected = (0..2)
        .map(|slot| SelectedSeat {
            slot,
            full: true,
            omit: false,
        })
        .collect::<Vec<_>>();
    let expected = original
        .collect_frame(&selected, FrameKind::Scheduled)
        .unwrap();
    let actual = loaded
        .collect_frame(&selected, FrameKind::Scheduled)
        .unwrap();
    for (a, b) in expected.iter().zip(&actual) {
        assert_same_json(&a.message, &b.message);
    }
    assert!(
        actual[0].message["shots"]
            .as_array()
            .unwrap()
            .iter()
            .any(|event| event["k"] == "hurt")
    );
    let next = loaded
        .collect_frame(&selected, FrameKind::Scheduled)
        .unwrap();
    assert!(
        next[0].message["shots"]
            .as_array()
            .is_none_or(|shots| shots.iter().all(|event| event["k"] != "hurt"))
    );
}
