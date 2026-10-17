use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use ww2_core::{game::Point, recipient_snapshot::SelectedSeat};

fn connection(id: u64) -> (Connection, mpsc::Receiver<Output>) {
    let (tx, rx) = mpsc::channel(128);
    let (cancel, _) = watch::channel(false);
    (Connection::new(id, tx, cancel), rx)
}
fn command(hub: &mut Hub, c: &Connection, value: Value, now: u64) {
    hub.message("ABC", c, &mut true, value, now);
}
fn outputs(hub: &mut Hub, rx: &mut mpsc::Receiver<Output>, now: u64) -> Vec<Value> {
    let mut result = Vec::new();
    while let Ok(output) = rx.try_recv() {
        match output {
            Output::Json(value) => result.push(value),
            Output::Watcher(json) => result.push(serde_json::from_str(&json).unwrap()),
            Output::Snapshot(frame) => {
                result.push(serde_json::from_str(&frame.json).unwrap());
                hub.complete_snapshot(&frame, true, now);
            }
            Output::Watcher(_) => panic!("unexpected watcher output on Horde commander channel"),
            Output::Close(code, reason) => panic!("unexpected close {code}: {reason}"),
        }
    }
    result
}
fn lobby() -> (
    Hub,
    Connection,
    mpsc::Receiver<Output>,
    Connection,
    mpsc::Receiver<Output>,
) {
    let mut hub = Hub::new(32, String::new());
    let (a, mut ra) = connection(1);
    let (b, mut rb) = connection(2);
    for (c, token) in [(&a, "a"), (&b, "b")] {
        hub.message(
            "ABC",
            c,
            &mut false,
            json!({"t":"hello","name":token,"token":token}),
            0,
        );
    }
    command(&mut hub, &a, json!({"t":"mode","v":"horde"}), 0);
    outputs(&mut hub, &mut ra, 0);
    outputs(&mut hub, &mut rb, 0);
    (hub, a, ra, b, rb)
}
struct MapDir(std::path::PathBuf);
impl MapDir {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "ww2-horde-maps-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&dir).unwrap();
        Self(dir)
    }
    fn write(&self, map: &Value) {
        std::fs::write(
            self.0.join("bastogne.json"),
            serde_json::to_vec(map).unwrap(),
        )
        .unwrap();
    }
}
impl Drop for MapDir {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
fn arena() -> Value {
    json!({"name":"Hosted Horde arena", "w":60,"h":60,"rows":vec![".".repeat(60);60],
        "spawns":[{"x":30,"y":5},{"x":10,"y":54},{"x":50,"y":54}],
        "defend":[0],"points":[{"x":30,"y":30}]})
}
fn identity(hub: &Hub) -> (u64, Option<crate::NativeRoomStatus>, String) {
    (
        hub.next_epoch,
        hub.native_status("ABC").ok(),
        format!(
            "{:?} {:?}",
            hub.rooms["ABC"].native.as_ref().map(|n| (
                &n.runtime.state().world.game.units,
                &n.runtime.state().horde,
                &n.runtime.state().start.economy,
                n.runtime.state().commands.pending_commands()
            )),
            hub.accounts
                .iter()
                .map(|(id, account)| (
                    id,
                    account.epoch,
                    account.slot,
                    &account.delivery,
                    &account.pressure,
                    account.full_terrain
                ))
                .collect::<Vec<_>>()
        ),
    )
}

#[test]
fn mode41_horde_maps_select_preserve_ignore_and_reread_start_restart_atomically() {
    let (mut hub, a, mut ra, _, mut rb) = lobby();
    assert_eq!(hub.rooms["ABC"].map, "bastogne");
    let maps = hub
        .maps
        .as_object()
        .unwrap()
        .iter()
        .filter(|(_, map)| map["horde"] == true)
        .map(|(name, _)| name.clone())
        .collect::<Vec<_>>();
    assert_eq!(maps.len(), 17);
    for name in &maps {
        command(&mut hub, &a, json!({"t":"map","name":name}), 0);
        command(&mut hub, &a, json!({"t":"mode","v":"conquest"}), 0);
        command(&mut hub, &a, json!({"t":"mode","v":"horde"}), 0);
        assert_eq!(&hub.rooms["ABC"].map, name);
        // Every declared Horde map admits the real full tick and every canonical recipient.
        hub.start_horde("ABC", a.id, false, 0).unwrap();
        let native = hub.rooms.get_mut("ABC").unwrap().native.as_mut().unwrap();
        let seats = (0..3)
            .map(|slot| SelectedSeat {
                slot,
                full: true,
                omit: false,
            })
            .collect::<Vec<_>>();
        for _ in 0..4 {
            native.runtime.advance_tick(&[false; 3]).unwrap();
            let sent = native
                .runtime
                .should_snapshot(native.runtime.state().world.game.tick);
            assert_eq!(
                native
                    .runtime
                    .collect_frame(&seats, FrameKind::Scheduled)
                    .unwrap()
                    .len(),
                3
            );
            native.runtime.tick_ai(sent).unwrap();
        }
        hub.rooms.get_mut("ABC").unwrap().native = None;
        outputs(&mut hub, &mut ra, 0);
        outputs(&mut hub, &mut rb, 0);
    }
    command(&mut hub, &a, json!({"t":"map","name":"default"}), 0);
    assert_eq!(hub.rooms["ABC"].map, *maps.last().unwrap());
    command(&mut hub, &a, json!({"t":"mode","v":"conquest"}), 0);
    command(&mut hub, &a, json!({"t":"map","name":"default"}), 0);
    command(&mut hub, &a, json!({"t":"army","v":"endless"}), 0);
    command(&mut hub, &a, json!({"t":"mode","v":"horde"}), 0);
    assert_eq!(hub.rooms["ABC"].map, "bastogne");
    assert_eq!(hub.rooms["ABC"].army, "standard");
    let dir = MapDir::new();
    let good = arena();
    let mut bad = good.clone();
    bad.as_object_mut().unwrap().remove("defend");
    hub.configure_authored_maps(Some(dir.0.clone()));
    dir.write(&bad);
    let before = identity(&hub);
    command(&mut hub, &a, json!({"t":"start"}), 1);
    assert_eq!(identity(&hub), before);
    assert!(
        outputs(&mut hub, &mut ra, 1)
            .iter()
            .all(|v| v["t"] != "start")
    );
    dir.write(&good);
    command(&mut hub, &a, json!({"t":"start"}), 2);
    assert!(hub.rooms["ABC"].native.is_some());
    let start = outputs(&mut hub, &mut ra, 2)
        .into_iter()
        .find(|v| v["t"] == "start")
        .unwrap();
    assert_eq!(start["names"], json!(["a", "b", "Horde"]));
    assert_eq!(start["teams"], json!([0, 0, 1]));
    let epoch = hub.next_epoch;
    dir.write(&bad);
    let before = identity(&hub);
    command(&mut hub, &a, json!({"t":"restart"}), 3);
    assert_eq!(identity(&hub), before);
    assert!(
        outputs(&mut hub, &mut ra, 3)
            .iter()
            .all(|v| v["t"] != "start")
    );
    dir.write(&good);
    command(&mut hub, &a, json!({"t":"restart"}), 4);
    assert_eq!(hub.next_epoch, epoch + 1);
    assert_eq!(hub.native_status("ABC").unwrap().tick, 0);
    assert!(
        outputs(&mut hub, &mut ra, 4)
            .iter()
            .any(|v| v["t"] == "start")
    );
    std::fs::remove_file(dir.0.join("bastogne.json")).unwrap();
    let before = identity(&hub);
    outputs(&mut hub, &mut ra, 4);
    outputs(&mut hub, &mut rb, 4);
    command(&mut hub, &a, json!({"t":"restart"}), 5);
    assert_eq!(identity(&hub), before);
    for rx in [&mut ra, &mut rb] {
        let values = outputs(&mut hub, rx, 5);
        assert_eq!(
            values.first().unwrap(),
            &json!({"t":"deny","cmd":"start","reason":"scenario"})
        );
        assert!(values.iter().all(|v| v["t"] != "start"));
    }
}

#[test]
fn mode44_host_nextwave_ignores_guest_pause_and_active_wave_then_ticks_and_reconnects() {
    let (mut hub, a, mut ra, b, mut rb) = lobby();
    command(&mut hub, &b, json!({"t":"start"}), 0);
    command(&mut hub, &a, json!({"t":"restart"}), 0);
    assert!(hub.rooms["ABC"].native.is_none());
    assert!(ra.try_recv().is_err() && rb.try_recv().is_err());
    command(&mut hub, &a, json!({"t":"start"}), 0);
    let host_start = outputs(&mut hub, &mut ra, 0);
    let guest_start = outputs(&mut hub, &mut rb, 0);
    assert!(
        hub.rooms["ABC"].native.is_some(),
        "initial host Start did not attach: host={host_start:?}, guest={guest_start:?}"
    );
    for (slot, messages) in [(0, &host_start), (1, &guest_start)] {
        let starts = messages
            .iter()
            .filter(|message| message["t"] == "start")
            .collect::<Vec<_>>();
        assert_eq!(starts.len(), 1, "seat {slot}: {messages:?}");
        assert_eq!(starts[0]["you"], slot);
        assert_eq!(starts[0]["names"], json!(["a", "b", "Horde"]));
    }
    let prior = identity(&hub);
    command(&mut hub, &a, json!({"t":"start"}), 0);
    assert_eq!(identity(&hub), prior);
    assert!(ra.try_recv().is_err() && rb.try_recv().is_err());
    let clock = |hub: &Hub| {
        hub.rooms["ABC"]
            .native
            .as_ref()
            .unwrap()
            .runtime
            .state()
            .horde
            .as_ref()
            .unwrap()
            .time_left
    };
    let before = clock(&hub);
    command(&mut hub, &b, json!({"t":"nextwave"}), 0);
    assert_eq!(clock(&hub), before);
    command(&mut hub, &a, json!({"t":"pause"}), 0);
    assert!(hub.native_status("ABC").unwrap().paused);
    command(&mut hub, &a, json!({"t":"nextwave"}), 0);
    assert_eq!(clock(&hub), before);
    command(&mut hub, &b, json!({"t":"resume"}), 0);
    assert!(hub.native_status("ABC").unwrap().paused);
    command(&mut hub, &a, json!({"t":"resume"}), 0);
    command(&mut hub, &a, json!({"t":"nextwave"}), 0);
    assert_eq!(clock(&hub), 0.0);
    for callback in 0..12 {
        hub.poll_native(callback * 50, callback * 50_000_000);
        outputs(&mut hub, &mut ra, callback * 50);
        outputs(&mut hub, &mut rb, callback * 50);
        let runtime = &hub.rooms["ABC"].native.as_ref().unwrap().runtime;
        assert!(runtime.fault().is_none());
        assert_eq!(runtime.state().horde.as_ref().unwrap().wave, 1);
        let active_clock = clock(&hub);
        command(&mut hub, &a, json!({"t":"nextwave"}), callback * 50);
        assert_eq!(clock(&hub), active_clock);
    }
    hub.disconnected("ABC", b.id, 600);
    assert!(hub.native_status("ABC").unwrap().paused);
    let (replacement, mut rx) = connection(3);
    hub.message(
        "ABC",
        &replacement,
        &mut false,
        json!({"t":"hello","token":"b","name":"Back"}),
        601,
    );
    assert!(!hub.native_status("ABC").unwrap().paused);
    let start = outputs(&mut hub, &mut rx, 601)
        .into_iter()
        .find(|v| v["t"] == "start")
        .unwrap();
    assert_eq!(start["names"], json!(["a", "Back", "Horde"]));
    assert!(hub.native_status("ABC").unwrap().fault.is_none());
}

#[test]
fn mode56_hosted_horde_runs_boss_combat_to_result_and_complete_hold_with_all_seat_frames() {
    let (mut hub, _a, mut ra, _, mut rb) = lobby();
    let dir = MapDir::new();
    hub.configure_records(crate::HordeRecords::new(dir.0.join("records.json")));
    dir.write(&arena());
    hub.configure_authored_maps(Some(dir.0.clone()));
    let map = hub.horde_map("bastogne").unwrap();
    let mut state = Match::new_horde(
        &map,
        MatchOptions {
            seed: 701,
            mode: Mode::Horde,
            players: vec![
                PlayerConfig {
                    name: "a".into(),
                    team: 0,
                    faction: Faction::Usa,
                },
                PlayerConfig {
                    name: "b".into(),
                    team: 0,
                    faction: Faction::Germany,
                },
            ],
            shuffle: false,
            army: ArmySize::Standard,
            weather: WeatherChoice::Map,
            weather_seed: Some(1),
            living_weather: true,
            map_key: Some("bastogne".into()),
            tech: true,
            supply: false,
            logistics: false,
            sandbox: false,
        },
    )
    .unwrap();
    let bunker = state.horde.as_ref().unwrap().bunker;
    let target = state.world.game.units[&bunker].at();
    state.world.game.units.get_mut(&bunker).unwrap().hp = 1.0;
    for u in state
        .world
        .game
        .units
        .values_mut()
        .filter(|u| !state.rules.kinds[u.kind].structure)
    {
        u.x = 11.0;
        u.z = 101.0;
        u.path.clear();
        u.amove = None;
    }
    let horde = state.horde.as_mut().unwrap();
    horde.wave = 9;
    horde.time_left = 0.0;
    horde.gates = vec![Point {
        x: target.x,
        z: target.z + 14.0,
    }];
    horde.profile_wave = 0;
    state.grid.rebuild(&state.world.game.units);
    let prepared = hub
        .prepare_native_room(&hub.rooms["ABC"], state, 0, false, Some(map.source()))
        .unwrap();
    hub.publish_native("ABC", prepared);
    outputs(&mut hub, &mut ra, 0);
    outputs(&mut hub, &mut rb, 0);
    let mut saw_boss = false;
    let mut saw_finish = false;
    let mut frames = [0; 2];
    for callback in 0..360_u64 {
        hub.poll_native(callback * 50, callback * 50_000_000);
        let status = hub.native_status("ABC").unwrap();
        assert!(status.fault.is_none(), "{:?}", status.fault);
        for (slot, rx) in [&mut ra, &mut rb].into_iter().enumerate() {
            for v in outputs(&mut hub, rx, callback * 50) {
                if v["t"] == "s" {
                    frames[slot] += 1;
                    assert_eq!(v["mode"]["kind"], "horde");
                    saw_boss |= v["mode"]["boss"].is_array();
                    saw_finish |= v["winner"] == 1;
                    assert_eq!(v["online"].as_array().unwrap().len(), 2);
                }
            }
        }
        if let Some(native) = hub.rooms.get_mut("ABC").unwrap().native.as_mut() {
            let selected = (0..3)
                .map(|slot| SelectedSeat {
                    slot,
                    full: false,
                    omit: false,
                })
                .collect::<Vec<_>>();
            assert_eq!(
                native
                    .runtime
                    .collect_frame(&selected, FrameKind::Current)
                    .unwrap()
                    .len(),
                3
            );
        }
        if status.ending_complete {
            break;
        }
    }
    assert!(saw_boss && saw_finish);
    assert!(frames.iter().all(|n| *n >= 60));
    assert!(hub.rooms["ABC"].native.is_none());
    let result = &hub.rooms["ABC"].result.as_ref().unwrap().report;
    assert_eq!(result["mode"], "horde");
    assert_eq!(result["winner"], 1);
    assert_eq!(result["teams"], json!([0, 0, 1]));
    assert_eq!(result["names"], json!(["a", "b", "Horde"]));
    assert_eq!(result["story"].as_array().unwrap().len(), 3);
    assert!(result["winVp"].is_null());
    assert!(
        result["timeline"]
            .as_array()
            .unwrap()
            .iter()
            .all(|v| v.get("points").is_some()
                && v.get("structures").is_some()
                && v.get("vp").is_none())
    );
}

#[path = "hosted_results_tests.rs"]
mod hosted_results;
