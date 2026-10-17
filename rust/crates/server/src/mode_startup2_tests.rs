use super::*;

fn room() -> (Hub, Connection, mpsc::Receiver<Output>) {
    let mut hub = Hub::new(32, String::new());
    hub.configure_trusted_room(Some(crate::TrustedRoomConfig::new("ABC", 3936341).unwrap()));
    let (tx, mut rx) = mpsc::channel(128);
    let (cancel, _) = watch::channel(false);
    let host = Connection::new(1, tx, cancel);
    hub.message(
        "ABC",
        &host,
        &mut false,
        json!({"t":"hello","token":"host","name":"host"}),
        0,
    );
    while rx.try_recv().is_ok() {}
    (hub, host, rx)
}

#[test]
fn mode01_lobby_whitelists_seven_kinds_and_rejects_invalid_or_foreign_selection() {
    let (mut hub, host, mut rx) = room();
    for mode in [
        "conquest",
        "assault",
        "annihilation",
        "classic",
        "horde",
        "tutorial",
        "world",
    ] {
        hub.message("ABC", &host, &mut true, json!({"t":"mode","v":mode}), 0);
        let room = &hub.rooms["ABC"];
        assert_eq!(room.mode, mode);
        assert_eq!(room.lobby(host.id, &hub.maps, "")["mode"], mode);
        while rx.try_recv().is_ok() {}
        for invalid in [
            json!("Conquest"),
            json!("other"),
            json!(""),
            json!(null),
            json!(7),
        ] {
            hub.message("ABC", &host, &mut true, json!({"t":"mode","v":invalid}), 0);
            assert_eq!(hub.rooms["ABC"].mode, mode);
            assert!(rx.try_recv().is_err());
        }
    }
    let (tx, mut foreign_rx) = mpsc::channel(32);
    let (cancel, _) = watch::channel(false);
    let foreign = Connection::new(2, tx, cancel);
    hub.message(
        "ABC",
        &foreign,
        &mut false,
        json!({"t":"hello","token":"guest"}),
        0,
    );
    while foreign_rx.try_recv().is_ok() {}
    hub.message(
        "ABC",
        &foreign,
        &mut true,
        json!({"t":"mode","v":"conquest"}),
        0,
    );
    assert_eq!(hub.rooms["ABC"].mode, "world");
    assert!(foreign_rx.try_recv().is_err());
}

#[test]
fn mode04_map06_non_world_capacity_and_start_readiness_follow_authored_spawns() {
    let (mut hub, host, _) = room();
    let template = hub.rooms["ABC"].players[0].clone();
    let keys = hub
        .maps
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(keys.len(), 33);
    for key in keys {
        let map = crate::authoritative_maps::load(&key).unwrap();
        for mode in [
            "conquest",
            "assault",
            "annihilation",
            "classic",
            "horde",
            "tutorial",
        ] {
            let expected = match mode {
                "horde" | "tutorial" => 5,
                "assault" => map.spawns().len(),
                _ => map.spawns().iter().filter(|spawn| !spawn.assault).count(),
            };
            let room = hub.rooms.get_mut("ABC").unwrap();
            room.map.clone_from(&key);
            room.mode = mode.into();
            room.defender = 4;
            assert_eq!(room.capacity(&hub.maps), expected, "{key} {mode}");
            assert_eq!(room.lobby(host.id, &hub.maps, "")["spawns"], expected);
            for count in 0..=7 {
                room.players = (0..count)
                    .map(|slot| {
                        let mut person = template.clone();
                        person.team = if slot == 0 { 4 } else { 2 };
                        person
                    })
                    .collect();
                let expected_ready =
                    count > 0 && count <= expected && (mode != "assault" || count >= 2);
                assert_eq!(
                    room.start_ready(&hub.maps),
                    expected_ready,
                    "{key} {mode} {count}"
                );
                if mode == "assault" && count > 0 {
                    for team in [4, 2] {
                        for player in &mut room.players {
                            player.team = team;
                        }
                        assert!(
                            !room.start_ready(&hub.maps),
                            "both Assault sides are required"
                        );
                    }
                }
            }
        }
    }
    let room = hub.rooms.get_mut("ABC").unwrap();
    room.mode = "conquest".into();
    room.loaded_spawns = Some(json!([{"assault":true},{},{},{}]));
    assert_eq!(room.capacity(&hub.maps), 3);
    room.mode = "assault".into();
    assert_eq!(room.capacity(&hub.maps), 4);
}

#[test]
fn map06_lobby_map_selection_reaches_conquest_layout_and_every_recipient() {
    // These authored maps have an extra Assault-only spawn and ordinary R1-safe terrain.
    for key in ["bastogne", "stalingrad-factory"] {
        let (mut hub, host, mut ra) = room();
        let (tx, mut rb) = mpsc::channel(128);
        let (cancel, _) = watch::channel(false);
        let guest = Connection::new(2, tx, cancel);
        hub.message(
            "ABC",
            &guest,
            &mut false,
            json!({"t":"hello","token":"guest","name":"guest"}),
            0,
        );
        hub.message("ABC", &host, &mut true, json!({"t":"map","name":key}), 0);
        let map = crate::authoritative_maps::load(key).unwrap();
        let usable = map
            .spawns()
            .iter()
            .filter(|s| !s.assault)
            .collect::<Vec<_>>();
        assert!(
            map.spawns().len() > usable.len(),
            "fixture must include Assault-only spawn"
        );
        assert_eq!(hub.rooms["ABC"].capacity(&hub.maps), usable.len());
        while ra.try_recv().is_ok() {}
        while rb.try_recv().is_ok() {}
        hub.message("ABC", &host, &mut true, json!({"t":"start"}), 0);
        let state = hub.rooms["ABC"]
            .native
            .as_ref()
            .expect("selected map must start")
            .runtime
            .state();
        assert_eq!(state.options.map_key.as_deref(), Some(key));
        let spawns = state
            .start
            .players
            .iter()
            .map(|p| json!({"x":p.spawn.x,"z":p.spawn.z}))
            .collect::<Vec<_>>();
        for spawn in &spawns {
            assert!(
                usable
                    .iter()
                    .any(|s| spawn
                        == &json!({"x":(s.x as f64 + 0.5)*2.0,"z":(s.y as f64 + 0.5)*2.0}))
            );
        }
        assert_ne!(spawns[0], spawns[1]);
        let mut assault_options = state.options.clone();
        assault_options.mode = ww2_core::economy::Mode::Assault;
        let assault = Match::new_assault(map, assault_options, 0).unwrap();
        let reserved = &map.spawns()[map.defend().unwrap()[0]];
        assert!(reserved.assault);
        assert_eq!(
            assault.start.players[0].spawn,
            ww2_core::game::Point {
                x: (reserved.x as f64 + 0.5) * 2.0,
                z: (reserved.y as f64 + 0.5) * 2.0,
            }
        );
        for (slot, receiver) in [&mut ra, &mut rb].into_iter().enumerate() {
            let mut starts = 0;
            while let Ok(output) = receiver.try_recv() {
                if let Output::Json(packet) = output
                    && packet["t"] == "start"
                {
                    starts += 1;
                    assert_eq!(packet["you"], slot);
                    assert_eq!(packet["spawns"], json!(spawns));
                    assert_eq!(packet["spawn"], spawns[slot]);
                }
            }
            assert_eq!(starts, 1);
        }
        for now in [50, 150, 250] {
            hub.poll_native(now, now * 1_000_000);
            for (slot, receiver) in [&mut ra, &mut rb].into_iter().enumerate() {
                let mut frames = 0;
                while let Ok(output) = receiver.try_recv() {
                    if let Output::Snapshot(frame) = output {
                        assert_eq!(frame.slot, slot);
                        let packet: Value = serde_json::from_str(&frame.json).unwrap();
                        assert!(packet.get("mode").is_none());
                        hub.complete_snapshot(&frame, true, now);
                        frames += 1;
                    }
                }
                assert_eq!(frames, 1);
            }
            assert!(hub.native_status("ABC").unwrap().fault.is_none());
        }
        assert_eq!(hub.native_status("ABC").unwrap().tick, 6);
    }
}

#[test]
fn map06_ordinary_validation_requires_two_universal_open_spawns_and_at_most_six() {
    use ww2_core::map::AuthoritativeMap;
    let valid =
        |source: &Value| AuthoritativeMap::from_json(&serde_json::to_vec(source).unwrap()).is_ok();
    let mut source = crate::authoritative_maps::load("six-fronts")
        .unwrap()
        .source()
        .clone();
    let all = source["spawns"].as_array().unwrap().clone();
    assert_eq!(all.len(), 6);
    for count in 0..=7 {
        source["spawns"] = json!((0..count).map(|i| all[i % 6].clone()).collect::<Vec<_>>());
        assert_eq!(valid(&source), (2..=6).contains(&count), "{count} spawns");
    }
    source["spawns"] = json!(all);
    for universal in 0..=6 {
        for (i, spawn) in source["spawns"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .enumerate()
        {
            spawn["assault"] = json!(i >= universal);
        }
        assert_eq!(
            valid(&source),
            universal >= 2,
            "{universal} universal spawns"
        );
        if universal < 2 {
            let bytes = serde_json::to_vec(&source).unwrap();
            let error = AuthoritativeMap::from_json(&bytes).unwrap_err();
            assert_eq!(error.to_string(), "needs 2+ spawns that every mode can use");
        }
    }
    source["spawns"][0]["assault"] = json!("true");
    assert!(!valid(&source));
    source["spawns"][0]["assault"] = json!(false);
    let spawn = &source["spawns"][0];
    let x = spawn["x"].as_u64().unwrap() as usize;
    let y = spawn["y"].as_u64().unwrap() as usize;
    let mut row = source["rows"][y].as_str().unwrap().as_bytes().to_vec();
    row[x] = b'W';
    source["rows"][y] = json!(String::from_utf8(row).unwrap());
    assert!(!valid(&source));
}

#[test]
fn mode04_trusted_start_checks_capacity_and_assault_sides_before_r1_policy() {
    let (mut hub, host, _receiver) = room();
    let template = hub.rooms["ABC"].players[0].clone();
    let room = hub.rooms.get_mut("ABC").unwrap();
    room.players = vec![template.clone(); 4];
    assert_eq!(room.capacity(&hub.maps), 3);
    let denial = hub.start_trusted("ABC", host.id, false, 0).unwrap_err();
    assert!(denial.contains("capacity"), "{denial}");
    let room = hub.rooms.get_mut("ABC").unwrap();
    room.mode = "assault".into();
    room.players.truncate(2);
    room.defender = 4;
    for team in [4, 2] {
        for player in &mut hub.rooms.get_mut("ABC").unwrap().players {
            player.team = team;
        }
        let denial = hub.start_trusted("ABC", host.id, false, 0).unwrap_err();
        assert!(denial.contains("both Assault sides"), "{denial}");
    }
    hub.rooms.get_mut("ABC").unwrap().players[0].team = 4;
    assert!(hub.rooms["ABC"].start_ready(&hub.maps));
    // Readiness is necessary; the separate R1 hosting capability still applies.
    let denial = hub.start_trusted("ABC", host.id, false, 0).unwrap_err();
    assert!(denial.contains("private Conquest"), "{denial}");
    assert!(hub.rooms["ABC"].native.is_none());
    assert_eq!(hub.next_epoch, 0);
    assert!(hub.accounts.is_empty());
}
