use super::*;

fn room() -> (
    Hub,
    Connection,
    mpsc::Receiver<Output>,
    Connection,
    mpsc::Receiver<Output>,
) {
    let mut hub = Hub::new(32, String::new());
    hub.configure_trusted_room(Some(crate::TrustedRoomConfig::new("ABC", 3936341).unwrap()));
    let mut make = |id, token| {
        let (tx, rx) = mpsc::channel(64);
        let (cancel, _) = watch::channel(false);
        let c = Connection::new(id, tx, cancel);
        hub.message(
            "ABC",
            &c,
            &mut false,
            json!({"t":"hello","name":token,"token":token}),
            0,
        );
        (c, rx)
    };
    let (a, mut ra) = make(1, "a");
    let (b, mut rb) = make(2, "b");
    while ra.try_recv().is_ok() {}
    while rb.try_recv().is_ok() {}
    (hub, a, ra, b, rb)
}

type AccountSnapshot = (u64, u64, usize, SnapshotDelivery, SeatBackpressure, bool);
type RoomIdentity = (crate::NativeRoomStatus, String, Value, Vec<AccountSnapshot>);

fn identity(hub: &Hub) -> RoomIdentity {
    let native = hub.rooms["ABC"].native.as_ref().unwrap();
    let state = native.runtime.state();
    (
        hub.native_status("ABC").unwrap(),
        format!(
            "{:?}{:?}{:?}",
            state.world.game.units,
            state.commands.units,
            state
                .commands
                .pending_tick(state.world.game.tick as u64 + 1)
                .collect::<Vec<_>>()
        ),
        native.receipt.clone().unwrap(),
        hub.accounts
            .iter()
            .map(|(&id, a)| {
                (
                    id,
                    a.epoch,
                    a.slot,
                    a.delivery.clone(),
                    a.pressure.clone(),
                    a.full_terrain,
                )
            })
            .collect(),
    )
}

#[test]
fn trusted_rejected_and_faulted_restart_preserves_runtime_receipt_accounts_and_epoch() {
    let (mut hub, a, mut ra, _, mut rb) = room();
    assert_eq!(hub.start_trusted("ABC", a.id, false, 0).unwrap(), 1);
    while ra.try_recv().is_ok() {}
    while rb.try_recv().is_ok() {}
    hub.next_epoch = crate::native_startup::MAX_MATCH_ID;
    let before = identity(&hub);
    assert!(
        hub.start_trusted("ABC", a.id, true, 0)
            .unwrap_err()
            .contains("epoch exhausted")
    );
    assert_eq!(identity(&hub), before);
    assert!(ra.try_recv().is_err() && rb.try_recv().is_err());
    hub.next_epoch = 1;
    hub.rooms
        .get_mut("ABC")
        .unwrap()
        .native
        .as_mut()
        .unwrap()
        .runtime
        .fault_stop("internal deterministic latched-fault setup".into());
    let before = identity(&hub);
    assert!(
        hub.start_trusted("ABC", a.id, true, 0)
            .unwrap_err()
            .contains("faulted match restart")
    );
    hub.poll_native(1000, 1_000_000_000);
    assert_eq!(identity(&hub), before);
    assert_eq!(hub.next_epoch, 1);
    assert!(ra.try_recv().is_err() && rb.try_recv().is_err());
}

#[test]
fn trusted_restart_resets_input_and_delivery_but_keeps_old_socket_write_accounting() {
    let (mut hub, a, mut ra, _, mut rb) = room();
    hub.start_trusted("ABC", a.id, false, 0).unwrap();
    while ra.try_recv().is_ok() {}
    while rb.try_recv().is_ok() {}
    hub.poll_native(100, 100_000_000);
    let old = loop {
        if let Output::Snapshot(write) = ra.try_recv().unwrap() {
            break write;
        }
    };
    let pressure = hub.accounts[&a.id].pressure.clone();
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
        .find(|u| {
            u.owner == 0
                && !hub.rooms["ABC"]
                    .native
                    .as_ref()
                    .unwrap()
                    .runtime
                    .state()
                    .rules
                    .kinds[u.kind]
                    .structure
        })
        .unwrap()
        .id;
    hub.message("ABC", &a, &mut true, json!({"t":"stop","ids":[owned]}), 100);
    assert!(
        hub.rooms["ABC"]
            .native
            .as_ref()
            .unwrap()
            .runtime
            .state()
            .commands
            .pending_tick((hub.native_status("ABC").unwrap().tick + 1) as u64)
            .next()
            .is_some()
    );
    assert_eq!(hub.start_trusted("ABC", a.id, true, 100).unwrap(), 2);
    assert_eq!(hub.accounts[&a.id].pressure, pressure);
    let delivery = hub.accounts[&a.id].delivery.clone();
    hub.complete_snapshot(&old, true, 100);
    assert_eq!(hub.accounts[&a.id].epoch, 2);
    assert_eq!(hub.accounts[&a.id].delivery, delivery);
    let state = hub.rooms["ABC"].native.as_ref().unwrap().runtime.state();
    assert_eq!(state.world.game.tick, 0);
    assert!(state.commands.pending_tick(1).next().is_none());
}

// Prototype server.js:359-389 and :688-689: retain settings, replace the game,
// and draw new seeds. The native host records a deterministic seed per match.
#[test]
fn mode94_restart_keeps_settings_uses_fresh_seeds_and_runs_recipient_frames() {
    let (mut hub, a, mut ra, b, mut rb) = room();
    hub.rooms.get_mut("ABC").unwrap().weather = "random".into();
    hub.start_trusted("ABC", a.id, false, 0).unwrap();
    super::native_tests::initial_start(&mut ra);
    super::native_tests::initial_start(&mut rb);
    let old = hub.rooms["ABC"].native.as_ref().unwrap().runtime.state();
    let settings = old.options.clone();
    let seeds = (
        old.options.seed,
        old.start.weather_seed,
        old.start.environment_seed,
    );
    let original_units = old.world.game.units.clone();
    let unit = old
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 0)
        .unwrap()
        .id;
    hub.rooms
        .get_mut("ABC")
        .unwrap()
        .native
        .as_mut()
        .unwrap()
        .runtime
        .state_mut()
        .world
        .game
        .units
        .get_mut(&unit)
        .unwrap()
        .hp = 1.0;
    // A guest cannot replace the game, even in the trusted room.
    hub.message("ABC", &b, &mut true, json!({"t":"restart"}), 1);
    assert_eq!(hub.native_status("ABC").unwrap().epoch, 1);
    assert!(
        matches!(rb.try_recv(), Ok(Output::Json(v)) if v == json!({"t":"deny","cmd":"restart","reason":"scenario"}))
    );
    hub.message("ABC", &a, &mut true, json!({"t":"restart"}), 2);
    super::native_tests::initial_start(&mut ra);
    super::native_tests::initial_start(&mut rb);
    let native = hub.rooms["ABC"].native.as_ref().unwrap();
    let fresh = native.runtime.state();
    assert_eq!(native.epoch, 2);
    assert_eq!(fresh.world.game.tick, 0);
    assert_ne!(fresh.options.seed, seeds.0);
    assert_ne!(fresh.start.weather_seed, seeds.1);
    assert_ne!(fresh.start.environment_seed, seeds.2);
    let mut fresh_options = fresh.options.clone();
    fresh_options.seed = settings.seed;
    assert_eq!(format!("{fresh_options:?}"), format!("{settings:?}"));
    assert_eq!(fresh.world.game.units[&unit].hp, original_units[&unit].hp);
    assert_eq!(
        native.receipt.as_ref().unwrap()["options"]["seed"],
        fresh.options.seed
    );
    let expected =
        crate::authoritative_maps::create_match("default", fresh.options.clone()).unwrap();
    assert_eq!(
        format!("{:?}", fresh.world.game.units),
        format!("{:?}", expected.world.game.units)
    );
    let mut frames = [0; 2];
    for callback in 0..6_u64 {
        let now = 2 + callback * 50;
        hub.poll_native(now, now * 1_000_000);
        for (slot, receiver) in [&mut ra, &mut rb].into_iter().enumerate() {
            while let Ok(output) = receiver.try_recv() {
                let Output::Snapshot(frame) = output else {
                    panic!("only fresh recipient frames expected")
                };
                assert_eq!(frame.epoch, 2);
                let snapshot: Value = serde_json::from_str(&frame.json).unwrap();
                assert!(snapshot["tick"].as_u64().unwrap() > 0);
                hub.complete_snapshot(&frame, true, now);
                frames[slot] += 1;
            }
        }
        assert!(hub.native_status("ABC").unwrap().fault.is_none());
    }
    assert_eq!(hub.native_status("ABC").unwrap().tick, 6);
    assert!(frames.into_iter().all(|count| count > 0));
}

#[test]
fn mode94_map_and_match_creation_failures_preserve_running_game_and_next_seed() {
    for missing_map in [true, false] {
        let (mut hub, a, mut ra, _, mut rb) = room();
        hub.start_trusted("ABC", a.id, false, 0).unwrap();
        super::native_tests::initial_start(&mut ra);
        super::native_tests::initial_start(&mut rb);
        hub.poll_native(0, 0);
        let checkpoint = hub.rooms["ABC"]
            .native
            .as_ref()
            .unwrap()
            .runtime
            .save_json()
            .unwrap();
        // Exercise both a real catalog load failure and Match::new rejecting
        // the loaded map's setup, rather than an injected result or fault.
        if missing_map {
            hub.rooms.get_mut("ABC").unwrap().map = "missing-authored-map".into();
        } else {
            hub.rooms.get_mut("ABC").unwrap().players[1].team = 6;
        }
        let before = identity(&hub);
        let lobby = hub.rooms["ABC"].lobby(a.id, &hub.maps, &hub.public_url);
        let error = hub.start_trusted("ABC", a.id, true, 1).unwrap_err();
        assert!(
            if missing_map {
                error.contains("unknown authored map")
            } else {
                error.starts_with("match startup:")
            },
            "{error}"
        );
        assert_eq!(identity(&hub), before);
        assert_eq!(
            hub.rooms["ABC"]
                .native
                .as_ref()
                .unwrap()
                .runtime
                .save_json()
                .unwrap(),
            checkpoint
        );
        assert_eq!(
            hub.rooms["ABC"].lobby(a.id, &hub.maps, &hub.public_url),
            lobby
        );
        assert_eq!(hub.next_epoch, 1);
        assert!(ra.try_recv().is_err() && rb.try_recv().is_err());
        hub.message("ABC", &a, &mut true, json!({"t":"restart"}), 2);
        assert!(
            matches!(ra.try_recv(), Ok(Output::Json(v)) if v == json!({"t":"deny","cmd":"restart","reason":"scenario"}))
        );
        assert!(rb.try_recv().is_err());
        assert_eq!(identity(&hub), before);
        let room = hub.rooms.get_mut("ABC").unwrap();
        room.map = "default".into();
        room.players[1].team = 1;
        hub.poll_native(50, 50_000_000);
        assert_eq!(hub.native_status("ABC").unwrap().tick, 2);
        for receiver in [&mut ra, &mut rb] {
            let Output::Snapshot(frame) = receiver.try_recv().unwrap() else {
                panic!("old game must still produce a recipient frame")
            };
            assert_eq!(frame.epoch, 1);
            hub.complete_snapshot(&frame, true, 50);
        }
        assert!(hub.native_status("ABC").unwrap().fault.is_none());
        hub.start_trusted("ABC", a.id, true, 51).unwrap();
        let fresh = hub.rooms["ABC"].native.as_ref().unwrap().runtime.state();
        assert_eq!(fresh.options.seed, 3936342);
        assert_eq!(hub.next_epoch, 2);
    }
}

#[test]
fn trusted_canonical_map_weather_matrix_records_options_before_publication() {
    let maps: Value = serde_json::from_str(include_str!("map-catalog.json")).unwrap();
    let mut admitted = 0;
    let mut refused = Vec::new();
    for map in maps.as_object().unwrap().keys() {
        for weather in ["map", "clear", "fog", "rain", "mud", "snow", "random"] {
            let (mut hub, a, _ra, _b, _rb) = room();
            let room = hub.rooms.get_mut("ABC").unwrap();
            room.map = map.clone();
            room.weather = weather.into();
            let result = hub.start_trusted("ABC", a.id, false, 0);
            if let Err(error) = result {
                assert_eq!(
                    error, "native startup authoring map binding changed",
                    "{map}/{weather}"
                );
                assert!(hub.rooms["ABC"].native.is_none() && hub.accounts.is_empty());
                assert_eq!(hub.next_epoch, 0);
                refused.push(format!("{map}/{weather}: {error}"));
                continue;
            }
            let native = hub.rooms["ABC"].native.as_ref().unwrap();
            let state = native.runtime.state();
            assert!(
                !state.options.logistics
                    && state.convoys.is_none()
                    && state.logistics.iter().next().is_none()
            );
            assert!(
                state.options.shuffle
                    && state.options.tech
                    && state.options.supply
                    && state.options.living_weather
            );
            assert_eq!(state.options.weather_seed, None);
            assert_eq!(
                native.receipt.as_ref().unwrap()["derivedWeatherSeed"],
                state.start.weather_seed
            );
            admitted += 1;
        }
    }
    eprintln!(
        "R1_CANONICAL_MATRIX admitted={admitted} refused={} gaps={refused:?}",
        refused.len()
    );
    assert_eq!(admitted + refused.len(), 231);
    assert_eq!(admitted, 231);
    assert!(refused.is_empty());
}
