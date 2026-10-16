use super::*;
use std::path::PathBuf;
use ww2_core::{game::Point, match_commands::Command, orders::OrderKey, seat_commander::Brain};

struct TestDir(PathBuf);
impl TestDir {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "ww2-horde-save-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn peer(id: u64) -> (Connection, mpsc::Receiver<Output>) {
    let (tx, rx) = mpsc::channel(128);
    let (cancel, _) = watch::channel(false);
    (Connection::new(id, tx, cancel), rx)
}
fn messages(hub: &mut Hub, rx: &mut mpsc::Receiver<Output>) -> Vec<Value> {
    let mut result = Vec::new();
    while let Ok(output) = rx.try_recv() {
        match output {
            Output::Json(value) => result.push(value),
            Output::Snapshot(frame) => {
                result.push(serde_json::from_str(&frame.json).unwrap());
                hub.complete_snapshot(&frame, true, 0);
            }
            Output::Close(code, reason) => panic!("unexpected close {code}: {reason}"),
        }
    }
    result
}
fn command(hub: &mut Hub, peer: &Connection, value: Value, now: u64) {
    hub.message("ABC", peer, &mut true, value, now);
}
fn setup(
    dir: &TestDir,
    sandbox: bool,
) -> (
    Hub,
    Connection,
    mpsc::Receiver<Output>,
    Connection,
    mpsc::Receiver<Output>,
) {
    let mut hub = Hub::new(32, String::new());
    hub.configure_saves(dir.0.join("saves"));
    hub.configure_records(crate::HordeRecords::new(dir.0.join("records.json")));
    let raw = json!({"name":"Saved Horde arena", "w":60,"h":60,"rows":vec![".".repeat(60);60],
        "spawns":[{"x":30,"y":5},{"x":10,"y":54},{"x":50,"y":54}],
        "defend":[0],"points":[{"x":30,"y":30}]});
    std::fs::write(
        dir.0.join("bastogne.json"),
        serde_json::to_vec(&raw).unwrap(),
    )
    .unwrap();
    hub.configure_authored_maps(Some(dir.0.clone()));
    let (host, mut host_rx) = peer(1);
    let (guest, mut guest_rx) = peer(2);
    for (client, token) in [(&host, "host"), (&guest, "guest")] {
        hub.message(
            "ABC",
            client,
            &mut false,
            json!({"t":"hello","name":token,"token":token}),
            0,
        );
    }
    command(&mut hub, &host, json!({"t":"mode","v":"horde"}), 0);
    command(&mut hub, &host, json!({"t":"sandbox","v":sandbox}), 0);
    command(&mut hub, &host, json!({"t":"start"}), 0);
    assert!(hub.rooms["ABC"].native.is_some());
    messages(&mut hub, &mut host_rx);
    messages(&mut hub, &mut guest_rx);
    (hub, host, host_rx, guest, guest_rx)
}
fn snapshot_every_seat(runtime: &mut RoomRuntime) {
    let selected = (0..3)
        .map(|slot| SelectedSeat {
            slot,
            full: true,
            omit: false,
        })
        .collect::<Vec<_>>();
    let frames = runtime
        .collect_frame(&selected, FrameKind::Current)
        .unwrap();
    assert_eq!(frames.len(), 3);
    for (slot, frame) in frames.iter().enumerate() {
        assert_eq!(frame.slot, slot);
        assert_eq!(frame.message["mode"]["kind"], "horde");
    }
    assert!(runtime.fault().is_none());
}

#[test]
fn hosted_horde_save_restores_wave_inputs_and_brain_then_resumes_to_full_ending_hold() {
    let dir = TestDir::new();
    let (mut hub, host, mut host_rx, guest, mut guest_rx) = setup(&dir, false);
    let runtime = &mut hub
        .rooms
        .get_mut("ABC")
        .unwrap()
        .native
        .as_mut()
        .unwrap()
        .runtime;
    let state = runtime.state_mut();
    let bunker = state.horde.as_ref().unwrap().bunker;
    let target = state.world.game.units[&bunker].at();
    state.world.game.units.get_mut(&bunker).unwrap().hp = 1.0;
    // Keep the defenders away so the spawned boss ends the match through combat.
    for unit in state
        .world
        .game
        .units
        .values_mut()
        .filter(|u| !state.rules.kinds[u.kind].structure)
    {
        unit.x = 11.0;
        unit.z = 101.0;
        unit.path.clear();
        unit.amove = None;
    }
    let human = state
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 0 && !state.rules.kinds[u.kind].structure)
        .unwrap()
        .id;
    let queued =
        Command::from_json(&json!({"t":"stance","ids":[human],"key":"holdFire","on":true}))
            .unwrap();
    let key = OrderKey {
        tick: 5,
        issuer: 0,
        sequence: 1,
    };
    state.enqueue_command(key, queued.clone()).unwrap();
    let horde = state.horde.as_mut().unwrap();
    horde.wave = 9;
    horde.time_left = 0.0;
    horde.gates = vec![Point {
        x: target.x,
        z: target.z + 14.0,
    }];
    horde.profile_wave = 0;
    state.grid.rebuild(&state.world.game.units);
    runtime.advance_tick(&[false; 3]).unwrap();
    let state = runtime.state_mut();
    let horde_slot = state.horde.as_ref().unwrap().slot;
    // A ready boss kills this 1 HP bunker on loaded tick 2. A finite cooldown
    // leaves time for the rebuilt brain and tick 5 input before natural defeat.
    for unit in state
        .world
        .game
        .units
        .values_mut()
        .filter(|unit| unit.owner == horde_slot)
    {
        state.firing.initialize(unit.id).cooldown = 2.0;
        if unit.air.is_some() {
            // Injury lets ordinary AA down this saved plane before the boss fires.
            unit.hp = 20.0;
        }
    }
    runtime.think(true).unwrap();
    snapshot_every_seat(runtime);
    assert!(runtime.state().finish.is_none());
    let saved_wave = runtime.state().horde.clone();
    assert!(saved_wave.as_ref().unwrap().active);
    assert_eq!(saved_wave.as_ref().unwrap().wave, 10);
    let saved_aircraft = runtime
        .state()
        .world
        .game
        .units
        .values()
        .filter(|unit| unit.owner == horde_slot && unit.air.is_some())
        .map(|unit| u64::from(unit.id))
        .collect::<Vec<_>>();
    assert!(!saved_aircraft.is_empty());
    let saved_random = runtime.state().start.random.snapshot();
    let saved_tick = runtime.state().world.game.tick;
    command(&mut hub, &host, json!({"t":"save"}), 1);
    let saved_messages = messages(&mut hub, &mut host_rx);
    assert_eq!(saved_messages[0]["t"], "saved");
    assert_eq!(saved_messages[1]["t"], "lobby");
    messages(&mut hub, &mut guest_rx);
    let id = saved_messages[0]["id"].as_str().unwrap();
    let saved = hub.saves.read("ABC", id).unwrap();
    assert_eq!(saved["settings"]["mode"], "horde");
    assert_eq!(saved["seats"].as_array().unwrap().len(), 2);
    assert!(
        saved["seats"]
            .as_array()
            .unwrap()
            .iter()
            .all(|seat| seat["ai"] == false)
    );
    assert!(saved["ai"].is_null());
    hub.rooms.get_mut("ABC").unwrap().native = None;
    // Loading uses the checkpoint's private raw map, not a fresh map-file read.
    std::fs::remove_file(dir.0.join("bastogne.json")).unwrap();
    command(&mut hub, &host, json!({"t":"loadSave","id":id}), 2);
    for (slot, rx) in [&mut host_rx, &mut guest_rx].into_iter().enumerate() {
        let received = messages(&mut hub, rx);
        let lobby = received.iter().position(|v| v["t"] == "lobby").unwrap();
        let start = received.iter().position(|v| v["t"] == "start").unwrap();
        let pause = received
            .iter()
            .position(|v| v["t"] == "pause" && v["paused"] == true)
            .unwrap();
        assert!(lobby < start && start < pause);
        assert_eq!(received[start]["you"], slot);
        assert_eq!(received[start]["names"], json!(["host", "guest", "Horde"]));
        assert_eq!(
            received[pause],
            json!({"t":"pause","paused":true,"by":"host","reason":"host","left":0})
        );
    }
    let runtime = &mut hub
        .rooms
        .get_mut("ABC")
        .unwrap()
        .native
        .as_mut()
        .unwrap()
        .runtime;
    assert_eq!(runtime.state().horde, saved_wave);
    assert_eq!(runtime.state().start.random.snapshot(), saved_random);
    assert_eq!(
        runtime.state().commands.pending_tick(5).collect::<Vec<_>>(),
        vec![(&key, &queued)]
    );
    let ai = runtime.ai.as_ref().unwrap();
    assert!(matches!(ai.commander(2).unwrap().brain(), Brain::Horde(_)));
    assert!(ai.commander(0).is_none() && ai.commander(1).is_none());
    assert_eq!(ai.delivered(2).unwrap().tick, saved_tick);
    snapshot_every_seat(runtime);
    command(&mut hub, &guest, json!({"t":"resume"}), 3);
    for callback in 0..5_u64 {
        hub.poll_native(callback * 50, callback * 50_000_000);
        messages(&mut hub, &mut host_rx);
        messages(&mut hub, &mut guest_rx);
    }
    assert_eq!(hub.native_status("ABC").unwrap().tick, saved_tick);
    assert!(hub.native_status("ABC").unwrap().paused);
    command(&mut hub, &host, json!({"t":"resume"}), 250);
    let mut saw_brain_observation = false;
    let mut saw_queued_input = false;
    let mut saw_finish = false;
    let mut saw_aircraft_aa = [false; 2];
    let mut saw_aircraft_down = [false; 2];
    let mut snapshots = [0; 2];
    for callback in 5..405_u64 {
        hub.poll_native(callback * 50, callback * 50_000_000);
        let status = hub.native_status("ABC").unwrap();
        assert!(status.fault.is_none(), "{:?}", status.fault);
        for (slot, rx) in [&mut host_rx, &mut guest_rx].into_iter().enumerate() {
            for message in messages(&mut hub, rx) {
                if message["t"] == "s" {
                    snapshots[slot] += 1;
                    if !saw_finish && message["winner"].is_null() {
                        for shot in message["shots"].as_array().into_iter().flatten() {
                            if shot["k"] == "aa"
                                && shot["to"] == 2
                                && shot["t"].as_u64().is_some_and(|id| saved_aircraft.contains(&id))
                            {
                                assert!(shot["x"].as_f64().is_some_and(|x| x.is_finite()));
                                assert!(shot["z"].as_f64().is_some_and(|z| z.is_finite()));
                                saw_aircraft_aa[slot] = true;
                            }
                            if shot["k"] == "planedown"
                                && shot["to"] == 2
                                && shot["t"].as_u64().is_some_and(|id| saved_aircraft.contains(&id))
                            {
                                assert!(saw_aircraft_aa[slot], "plane down preceded delivered AA");
                                assert!(shot["x"].as_f64().is_some_and(|x| x.is_finite()));
                                assert!(shot["z"].as_f64().is_some_and(|z| z.is_finite()));
                                assert!(shot["dir"].as_f64().is_some_and(|dir| dir.is_finite()));
                                assert_eq!(shot["kill"], true);
                                saw_aircraft_down[slot] = true;
                            }
                        }
                    }
                    saw_finish |= message["winner"] == 1;
                }
            }
        }
        if let Some(native) = hub.rooms.get_mut("ABC").unwrap().native.as_mut() {
            snapshot_every_seat(&mut native.runtime);
            let state = native.runtime.state();
            saw_queued_input |=
                state.world.game.tick >= 5 && state.commands.units[&human].hold_fire;
            let Brain::Horde(brain) = native
                .runtime
                .ai
                .as_ref()
                .unwrap()
                .commander(2)
                .unwrap()
                .brain()
            else {
                panic!("scripted Horde brain missing")
            };
            saw_brain_observation |= brain.delivered().is_some_and(|view| view.tick > saved_tick);
        }
        if status.ending_complete {
            break;
        }
    }
    let final_status = hub.native_status("ABC").unwrap();
    let remaining = hub.rooms["ABC"].native.as_ref().map(|native| {
        let state = native.runtime.state();
        (
            state.world.game.units.get(&bunker).map(|unit| unit.hp),
            state.horde.clone(),
            state.commands.units.get(&human).map(|unit| unit.hold_fire),
            state.commands.pending_commands(),
            state.finish,
        )
    });
    let diagnostic = format!(
        "brain={saw_brain_observation}, queued={saw_queued_input}, aa={saw_aircraft_aa:?}, planedown={saw_aircraft_down:?}, finish={saw_finish}, frames={snapshots:?}, status={final_status:?}, remaining={remaining:?}"
    );
    assert!(
        saw_brain_observation,
        "no resumed brain observation: {diagnostic}"
    );
    assert!(
        saw_queued_input,
        "queued HoldFire was not applied: {diagnostic}"
    );
    assert!(saw_finish, "no delivered Horde victory: {diagnostic}");
    assert!(
        saw_aircraft_aa.iter().all(|seen| *seen),
        "every human must receive AA against a saved aircraft before victory: {diagnostic}"
    );
    assert!(
        saw_aircraft_down.iter().all(|seen| *seen),
        "every human must receive a saved aircraft's natural plane down before victory: {diagnostic}"
    );
    assert!(snapshots.iter().all(|count| *count >= 60));
    assert!(hub.rooms["ABC"].native.is_none());
    assert_eq!(
        hub.rooms["ABC"].result.as_ref().unwrap().report["winner"],
        1
    );
}

#[test]
fn hosted_horde_save_keeps_human_and_atomic_load_capability_guards() {
    let dir = TestDir::new();
    let (mut hub, host, mut host_rx, guest, mut guest_rx) = setup(&dir, false);
    assert!(hub.save_room("ABC", guest.id).is_err());
    hub.rooms.get_mut("ABC").unwrap().players[1].ai = true;
    assert!(hub.save_room("ABC", host.id).is_err());
    hub.rooms.get_mut("ABC").unwrap().players[1].ai = false;
    let id = hub.save_room("ABC", host.id).unwrap();
    let saved = hub.saves.read("ABC", &id).unwrap();
    hub.rooms.get_mut("ABC").unwrap().native = None;
    let before_epoch = hub.next_epoch;
    let before_lobby = hub.rooms["ABC"].lobby(host.id, &hub.maps, "");
    let accounts = || {
        hub.accounts
            .iter()
            .map(|(id, account)| (*id, account.epoch, account.slot))
            .collect::<Vec<_>>()
    };
    let before_accounts = accounts();
    for mutate in 0..4 {
        let mut bad = saved.clone();
        match mutate {
            0 => bad["settings"]["mode"] = json!("conquest"),
            1 => bad["seats"][1]["ai"] = json!(true),
            2 => bad["ai"] = json!({"planner":"unsupported"}),
            _ => bad["g"]["held"] = json!(120),
        }
        let bad_id = hub
            .saves
            .write(
                "ABC",
                "manual",
                &bad,
                json!({"mode":"horde"}),
                1_800_000_000_000 + mutate,
            )
            .unwrap();
        assert!(hub.load_room("ABC", host.id, &bad_id, 1).is_err());
        assert_eq!(hub.next_epoch, before_epoch);
        assert!(hub.rooms["ABC"].native.is_none());
        let mut after_lobby = hub.rooms["ABC"].lobby(host.id, &hub.maps, "");
        after_lobby["saves"] = before_lobby["saves"].clone();
        assert_eq!(after_lobby, before_lobby);
        assert_eq!(
            hub.accounts
                .iter()
                .map(|(id, account)| (*id, account.epoch, account.slot))
                .collect::<Vec<_>>(),
            before_accounts
        );
    }
    command(&mut hub, &guest, json!({"t":"loadSave","id":id}), 2);
    assert!(hub.rooms["ABC"].native.is_none());
    assert!(messages(&mut hub, &mut host_rx).is_empty());
    assert_eq!(
        messages(&mut hub, &mut guest_rx),
        vec![json!({"t":"deny","cmd":"loadSave","reason":"save"})]
    );
}

#[test]
fn hosted_horde_save_restores_sandbox_flag_and_resumes_every_seat() {
    let dir = TestDir::new();
    let (mut hub, host, mut host_rx, _, mut guest_rx) = setup(&dir, true);
    let runtime = &mut hub
        .rooms
        .get_mut("ABC")
        .unwrap()
        .native
        .as_mut()
        .unwrap()
        .runtime;
    assert!(runtime.state().options.sandbox);
    for _ in 0..3 {
        runtime.advance_tick(&[false; 3]).unwrap();
        runtime.think(true).unwrap();
        snapshot_every_seat(runtime);
    }
    let saved_tick = runtime.state().world.game.tick;
    let saved_wave = runtime.state().horde.clone();
    let id = hub.save_room("ABC", host.id).unwrap();
    hub.rooms.get_mut("ABC").unwrap().native = None;
    hub.load_room("ABC", host.id, &id, 1).unwrap();
    assert!(hub.rooms["ABC"].sandbox);
    for rx in [&mut host_rx, &mut guest_rx] {
        let start = messages(&mut hub, rx)
            .into_iter()
            .find(|message| message["t"] == "start")
            .unwrap();
        assert_eq!(start["sandbox"], true);
    }
    let runtime = &hub.rooms["ABC"].native.as_ref().unwrap().runtime;
    assert!(runtime.paused());
    assert!(runtime.state().options.sandbox);
    assert_eq!(runtime.state().world.game.tick, saved_tick);
    assert_eq!(runtime.state().horde, saved_wave);
    command(&mut hub, &host, json!({"t":"resume"}), 2);
    for callback in 0..10_u64 {
        hub.poll_native(callback * 50, callback * 50_000_000);
        messages(&mut hub, &mut host_rx);
        messages(&mut hub, &mut guest_rx);
        snapshot_every_seat(
            &mut hub
                .rooms
                .get_mut("ABC")
                .unwrap()
                .native
                .as_mut()
                .unwrap()
                .runtime,
        );
    }
    assert!(hub.native_status("ABC").unwrap().tick > saved_tick);
    assert!(!dir.0.join("records.json").exists());
}
