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

#[test]
fn conquest_no_mans_land_r1_starts_and_delivers_both_seats() {
    let (mut hub, host, mut host_rx, _, mut peer_rx) = room();
    hub.message(
        "ABC",
        &host,
        &mut true,
        json!({"t":"map","name":"no-mans-land"}),
        0,
    );
    for rx in [&mut host_rx, &mut peer_rx] {
        while rx.try_recv().is_ok() {}
    }
    // Use the same Start dispatch and approved R1 profile as the ordinary CLI.
    hub.message("ABC", &host, &mut true, json!({"t":"start"}), 0);
    for (slot, rx) in [&mut host_rx, &mut peer_rx].into_iter().enumerate() {
        let mut start = None;
        while let Ok(output) = rx.try_recv() {
            let Output::Json(message) = output else {
                panic!("start handshake must precede snapshots");
            };
            assert_ne!(message["t"], "deny", "{message}");
            if message["t"] == "start" {
                start = Some(message);
            }
        }
        let start = start.expect("each commander receives Start");
        assert_eq!(start["you"], slot);
        assert_eq!(start["map"]["w"], 120);
        assert_eq!(start["map"]["h"], 90);
        assert_eq!(start["map"]["trenchFacing"], true);
        assert_eq!(start["spawns"].as_array().unwrap().len(), 2);
        assert!(start["cells"].is_array() && start["fog"].is_object());
    }
    let state = hub.rooms["ABC"].native.as_ref().unwrap().runtime.state();
    assert_eq!(state.start.points.len(), 9);
    assert!(state.scenario.is_none());
    let front = state.world.initial().trench_front.as_ref().unwrap();
    assert!(front.iter().any(|v| v.is_nan()));
    assert!(front.iter().any(|v| v.is_finite()));
    assert_eq!(
        state
            .world
            .initial()
            .chars
            .iter()
            .filter(|&&c| c == b'T')
            .count(),
        675
    );
    let mut delivered = [0; 2];
    for tick in 1..=6 {
        let now = (tick - 1) * 50;
        hub.poll_native(now, now * 1_000_000);
        let status = hub.native_status("ABC").unwrap();
        assert_eq!(status.tick, tick as i64);
        assert_eq!(status.fault, None);
        assert!(!status.paused);
        for (slot, rx) in [&mut host_rx, &mut peer_rx].into_iter().enumerate() {
            while let Ok(output) = rx.try_recv() {
                let Output::Snapshot(write) = output else {
                    panic!("running match must deliver recipient snapshots");
                };
                assert_eq!(write.slot, slot);
                let message: Value = serde_json::from_str(&write.json).unwrap();
                assert_eq!(message["t"], "s");
                delivered[slot] += 1;
                hub.complete_snapshot(&write, true, now);
            }
            assert_eq!(delivered[slot], tick / 2);
        }
    }
    assert_eq!(delivered, [3, 3]);
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
            assert_eq!(state.options.weather_seed, Some(state.start.weather_seed));
            assert!(state.start.weather_seed < (1 << 31));
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

fn weather_replay(receipt: &Value) -> Match {
    let o = &receipt["options"];
    crate::authoritative_maps::create_match(
        o["mapKey"].as_str().unwrap(),
        MatchOptions {
            seed: o["seed"].as_u64().unwrap(),
            mode: Mode::Conquest,
            players: o["players"]
                .as_array()
                .unwrap()
                .iter()
                .map(|p| PlayerConfig {
                    name: p["name"].as_str().unwrap().into(),
                    team: p["team"].as_i64().unwrap() as i32,
                    faction: Faction::try_from(p["faction"].as_u64().unwrap() as u8).unwrap(),
                })
                .collect(),
            shuffle: o["shuffle"].as_bool().unwrap(),
            army: ArmySize::Standard,
            weather: WeatherChoice::from_reference(o["weather"].as_str()),
            weather_seed: Some(o["weatherSeed"].as_u64().unwrap() as u32),
            living_weather: o["livingWeather"].as_bool().unwrap(),
            map_key: Some(o["mapKey"].as_str().unwrap().into()),
            tech: o["tech"].as_bool().unwrap(),
            supply: o["supply"].as_bool().unwrap(),
            logistics: o["logistics"].as_bool().unwrap(),
            sandbox: o["sandbox"].as_bool().unwrap(),
        },
    )
    .unwrap()
}

#[test]
fn mode07_server_start_restart_records_weather_inputs_and_replays_all_seats() {
    for choice in ["map", "clear", "fog", "rain", "mud", "snow", "random"] {
        let (mut hub, a, mut ra, _, mut rb) = room();
        hub.message("ABC", &a, &mut true, json!({"t":"weather","v":choice}), 0);
        for (restart, seed, expected_epoch) in [(false, 0, 1), (true, 71, 2)] {
            let epoch = hub
                .start_trusted_with_weather_source("ABC", a.id, restart, 0, || Ok(seed))
                .unwrap();
            assert_eq!(epoch, expected_epoch);
            let native = hub.rooms.get_mut("ABC").unwrap().native.as_mut().unwrap();
            let receipt = native.receipt.as_ref().unwrap().clone();
            assert_eq!(receipt["options"]["weather"], choice);
            assert_eq!(receipt["options"]["weatherSeed"], seed);
            assert_eq!(receipt["derivedWeatherSeed"], seed);
            let mut replay = RoomRuntime::new(weather_replay(&receipt), 0).unwrap();
            assert_eq!(
                native.runtime.state().start.weather,
                replay.state().start.weather
            );
            assert_eq!(
                native.runtime.state().start.initial_rain,
                replay.state().start.initial_rain
            );
            assert_eq!(
                native.runtime.state().start.initial_wet,
                replay.state().start.initial_wet
            );
            if choice == "rain" {
                assert!(native.runtime.state().start.initial_rain > 0.0);
            }
            if matches!(choice, "rain" | "mud") {
                assert!(native.runtime.state().start.initial_wet.unwrap() > 0.0);
            }
            let settings = crate::native_startup::StartupSettings {
                map: receipt["options"]["mapKey"].as_str().unwrap(),
                mode: "conquest",
                army: "standard",
                weather: choice,
                sandbox: false,
            };
            let startup =
                crate::native_startup::NativeStartup::prepare(replay.state(), settings).unwrap();
            let starts = replay
                .collect_starts(&[0, 1], &startup, &["a".into(), "b".into()], epoch)
                .unwrap();
            let queues = [&mut ra, &mut rb];
            for (slot, queue) in queues.into_iter().enumerate() {
                let mut start = None;
                while let Ok(output) = queue.try_recv() {
                    if let Output::Json(message) = output {
                        assert!(message.get("options").is_none());
                        assert!(message.get("weatherSeed").is_none());
                        if message["t"] == "start" {
                            start = Some(message);
                        }
                    }
                }
                assert_eq!(start.unwrap(), starts[slot].1);
            }
            let seats = [0, 1].map(|slot| SelectedSeat {
                slot,
                full: false,
                omit: false,
            });
            for _ in 0..8 {
                native.runtime.advance_tick(&[false; 2]).unwrap();
                replay.advance_tick(&[false; 2]).unwrap();
                let actual = native
                    .runtime
                    .collect_frame(&seats, FrameKind::Scheduled)
                    .unwrap();
                let expected = replay.collect_frame(&seats, FrameKind::Scheduled).unwrap();
                assert_eq!(actual.len(), 2);
                for (a, b) in actual.iter().zip(&expected) {
                    assert_eq!(a.message, b.message);
                }
                assert!(native.runtime.fault().is_none() && replay.fault().is_none());
            }
        }
    }
}

#[test]
fn mode07_server_weather_lobby_is_host_only_validated_and_locked_during_play() {
    let (mut hub, a, mut ra, b, mut rb) = room();
    for (who, value) in [(&b, "snow"), (&a, "storm")] {
        hub.message("ABC", who, &mut true, json!({"t":"weather","v":value}), 0);
        assert_eq!(hub.rooms["ABC"].weather, "map");
    }
    for choice in ["clear", "fog", "rain", "mud", "snow", "random", "map"] {
        hub.message("ABC", &a, &mut true, json!({"t":"weather","v":choice}), 0);
        assert_eq!(hub.rooms["ABC"].weather, choice);
    }
    hub.message("ABC", &a, &mut true, json!({"t":"start"}), 0);
    assert_eq!(hub.rooms["ABC"].native.as_ref().unwrap().epoch, 1);
    let before = identity(&hub);
    for who in [&a, &b] {
        hub.message("ABC", who, &mut true, json!({"t":"weather","v":"snow"}), 0);
        assert_eq!(hub.rooms["ABC"].weather, "map");
        assert_eq!(identity(&hub), before);
    }
    let mut denied = 0;
    for queue in [&mut ra, &mut rb] {
        while let Ok(output) = queue.try_recv() {
            if let Output::Json(message) = output
                && message["t"] == "deny"
                && message["cmd"] == "weather"
            {
                assert_eq!(message["reason"], "blocked");
                denied += 1;
            }
        }
    }
    assert_eq!(denied, 2);
    hub.message("ABC", &a, &mut true, json!({"t":"restart"}), 0);
    let native = hub.rooms["ABC"].native.as_ref().unwrap();
    assert_eq!(native.epoch, 2);
    assert_eq!(hub.rooms["ABC"].weather, "map");
    assert_eq!(
        native.receipt.as_ref().unwrap()["options"]["weatherSeed"],
        native.runtime.state().start.weather_seed
    );
    assert!(native.runtime.state().start.weather_seed < (1 << 31));
}

#[test]
fn mode07_weather_entropy_failure_preserves_lobby_and_running_match() {
    let (mut hub, a, mut ra, _, mut rb) = room();
    let unavailable = || Err("weather seed unavailable: injected failure".into());
    assert!(
        hub.start_trusted_with_weather_source("ABC", a.id, false, 0, unavailable)
            .is_err()
    );
    assert!(hub.rooms["ABC"].native.is_none() && hub.accounts.is_empty());
    assert_eq!(hub.next_epoch, 0);
    assert!(ra.try_recv().is_err() && rb.try_recv().is_err());
    hub.start_trusted("ABC", a.id, false, 0).unwrap();
    while ra.try_recv().is_ok() {}
    while rb.try_recv().is_ok() {}
    let before = identity(&hub);
    assert!(
        hub.start_trusted_with_weather_source("ABC", a.id, true, 0, unavailable)
            .is_err()
    );
    assert_eq!(identity(&hub), before);
    assert!(ra.try_recv().is_err() && rb.try_recv().is_err());
}

#[test]
fn mode06_trusted_armies_keep_lobby_choice_wallets_receipts_and_both_seat_frames() {
    for (key, income, population) in [
        ("standard", 1.0, 1.0),
        ("large", 3.0, 2.5),
        ("massive", 6.0, 5.0),
        ("endless", 20.0, 5.0),
    ] {
        let (mut hub, a, mut ra, b, mut rb) = room();
        hub.message("ABC", &b, &mut true, json!({"t":"army","v":key}), 0);
        assert_eq!(hub.rooms["ABC"].army, "standard");
        hub.message("ABC", &a, &mut true, json!({"t":"army","v":key}), 0);
        assert_eq!(hub.rooms["ABC"].army, key);
        for value in [
            json!("unknown"),
            json!("toString"),
            json!("__proto__"),
            json!(["massive"]),
            json!({}),
            Value::Null,
        ] {
            hub.message("ABC", &a, &mut true, json!({"t":"army","v":value}), 0);
            assert_eq!(hub.rooms["ABC"].army, key);
        }
        while ra.try_recv().is_ok() {}
        while rb.try_recv().is_ok() {}
        hub.message("ABC", &a, &mut true, json!({"t":"start"}), 0);
        for (slot, rx) in [&mut ra, &mut rb].into_iter().enumerate() {
            let mut starts = 0;
            while let Ok(output) = rx.try_recv() {
                if let Output::Json(value) = output {
                    assert_ne!(value["t"], "deny", "{value}");
                    if value["t"] == "start" {
                        starts += 1;
                        assert_eq!(value["you"], slot);
                        assert!(value["fog"].is_object() && value["cells"].is_array());
                    }
                }
            }
            assert_eq!(starts, 1, "{key} seat {slot}");
        }
        let native = hub.rooms["ABC"].native.as_ref().unwrap();
        let state = native.runtime.state();
        assert_eq!(state.options.army.key(), key);
        assert_eq!(native.receipt.as_ref().unwrap()["options"]["army"], key);
        assert!(
            state
                .start
                .economy
                .players
                .iter()
                .all(|p| p.mp == 150.0 * income)
        );
        // Catch-up includes the callback due at zero: two ticks per poll.
        for now in [50, 150, 250] {
            hub.poll_native(now, now * 1_000_000);
            for (slot, rx) in [&mut ra, &mut rb].into_iter().enumerate() {
                let mut snapshots = 0;
                while let Ok(output) = rx.try_recv() {
                    if let Output::Snapshot(write) = output {
                        assert_eq!(write.slot, slot);
                        let value: Value = serde_json::from_str(&write.json).unwrap();
                        assert_eq!(value["t"], "s");
                        assert_eq!(value["army"], json!({"pop":population,"income":income}));
                        hub.complete_snapshot(&write, true, now);
                        snapshots += 1;
                    }
                }
                assert!(snapshots > 0, "{key} seat {slot} at {now}");
            }
        }
        let native = hub.rooms["ABC"].native.as_ref().unwrap();
        assert!(native.runtime.fault().is_none());
        assert_eq!(native.runtime.state().world.game.tick, 6);
        hub.message("ABC", &a, &mut true, json!({"t":"restart"}), 250);
        let native = hub.rooms["ABC"].native.as_ref().unwrap();
        assert_eq!(native.runtime.state().options.army.key(), key);
        assert_eq!(native.receipt.as_ref().unwrap()["options"]["army"], key);
        assert!(
            native
                .runtime
                .state()
                .start
                .economy
                .players
                .iter()
                .all(|p| p.mp == 150.0 * income)
        );
    }
    let (mut hub, a, _ra, _b, _rb) = room();
    hub.rooms.get_mut("ABC").unwrap().army = "unknown".into();
    assert!(
        hub.start_trusted("ABC", a.id, false, 0)
            .unwrap_err()
            .contains("army")
    );
    assert!(hub.rooms["ABC"].native.is_none());
    assert!(hub.accounts.is_empty());
}
