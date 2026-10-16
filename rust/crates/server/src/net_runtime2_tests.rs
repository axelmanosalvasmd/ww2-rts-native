use super::*;

fn peer(id: u64) -> (Connection, mpsc::Receiver<Output>) {
    let (tx, rx) = mpsc::channel(64);
    let (cancel, _) = watch::channel(false);
    (Connection::new(id, tx, cancel), rx)
}

fn messages(rx: &mut mpsc::Receiver<Output>) -> Vec<Value> {
    let mut values = Vec::new();
    while let Ok(output) = rx.try_recv() {
        let Output::Json(value) = output else {
            panic!("expected JSON output");
        };
        values.push(value);
    }
    values
}

#[test]
fn net20_hello_validates_fields_and_sanitizes_names_with_javascript_whitespace() {
    let mut hub = Hub::new(32, String::new());
    let (host, mut rx) = peer(1);
    let mut joined = false;
    for invalid in [
        json!({"t":"name","name":"Before hello"}),
        json!({"t":"hello","token":"valid"}),
        json!({"t":"hello","name":"valid"}),
        json!({"t":false,"token":"valid","name":"valid"}),
    ] {
        hub.message("ABC", &host, &mut joined, invalid, 0);
        assert!(!joined && hub.rooms.is_empty());
    }
    for bad in [Value::Null, json!(false), json!(3), json!([]), json!({})] {
        for field in ["name", "token"] {
            let mut hello = json!({"t":"hello","token":"valid","name":"valid"});
            hello[field] = bad.clone();
            hub.message("ABC", &host, &mut joined, hello, 0);
            assert!(!joined && hub.rooms.is_empty());
        }
    }
    assert!(messages(&mut rx).is_empty());
    hub.message(
        "ABC",
        &host,
        &mut joined,
        json!({"t":"hello","token":"😀".repeat(25),"name":"\u{feff} <>&\"'Alice \u{feff}"}),
        0,
    );
    assert!(joined);
    assert_eq!(
        hub.rooms["ABC"].players[0].token,
        "😀".repeat(20).encode_utf16().collect::<Vec<_>>()
    );
    assert_eq!(messages(&mut rx)[0]["players"][0]["name"], "Alice");
    let whitespace = "\t\n\u{b}\u{c}\r \u{a0}\u{1680}\u{2000}\u{2001}\u{2002}\u{2003}\u{2004}\u{2005}\u{2006}\u{2007}\u{2008}\u{2009}\u{200a}\u{2028}\u{2029}\u{202f}\u{205f}\u{3000}\u{feff}";
    for (name, expected) in [
        (format!("{whitespace}Alice{whitespace}"), "Alice".to_owned()),
        (
            format!("{whitespace}<>&\"'{whitespace}"),
            "Soldier".to_owned(),
        ),
        (
            "\u{85}Alice\u{85}".to_owned(),
            "\u{85}Alice\u{85}".to_owned(),
        ),
        (
            "\u{180e}Alice\u{200b}".to_owned(),
            "\u{180e}Alice\u{200b}".to_owned(),
        ),
        ("😀".repeat(10), "😀".repeat(8)),
        (
            "abcdefghijklmnopq".to_owned(),
            "abcdefghijklmnop".to_owned(),
        ),
    ] {
        hub.message(
            "ABC",
            &host,
            &mut joined,
            json!({"t":"name","name":name}),
            1,
        );
        assert_eq!(messages(&mut rx)[0]["players"][0]["name"], expected);
    }
}

#[test]
fn net32_ai_level_requires_host_lobby_ai_seat_and_integer_slot() {
    let mut hub = Hub::new(32, String::new());
    let (host, mut rx) = peer(1);
    let (guest, mut guest_rx) = peer(2);
    for (connection, token) in [(&host, "host"), (&guest, "guest")] {
        hub.message(
            "ABC",
            connection,
            &mut false,
            json!({"t":"hello","token":token,"name":token}),
            0,
        );
    }
    hub.message("ABC", &host, &mut true, json!({"t":"addAi"}), 0);
    messages(&mut rx);
    messages(&mut guest_rx);
    for level in ["easy", "normal", "hard"] {
        hub.message(
            "ABC",
            &host,
            &mut true,
            json!({"t":"level","slot":2,"v":level}),
            1,
        );
        assert_eq!(messages(&mut rx)[0]["players"][2]["level"], level);
        assert_eq!(messages(&mut guest_rx)[0]["players"][2]["level"], level);
    }
    for bad in [
        json!(null),
        json!(false),
        json!("2"),
        json!(-1),
        json!(2.5),
        json!(99),
    ] {
        hub.message(
            "ABC",
            &host,
            &mut true,
            json!({"t":"level","slot":bad,"v":"easy"}),
            2,
        );
    }
    for bad in [json!(null), json!(false), json!(3), json!("impossible")] {
        hub.message(
            "ABC",
            &host,
            &mut true,
            json!({"t":"level","slot":2,"v":bad}),
            2,
        );
    }
    hub.message(
        "ABC",
        &guest,
        &mut true,
        json!({"t":"level","slot":2,"v":"easy"}),
        2,
    );
    hub.message(
        "ABC",
        &host,
        &mut true,
        json!({"t":"level","slot":1,"v":"easy"}),
        2,
    );
    assert!(messages(&mut rx).is_empty() && messages(&mut guest_rx).is_empty());
    assert_eq!(hub.rooms["ABC"].players[2].level, "hard");
    hub.message("ABC", &host, &mut true, json!({"t":"kick","slot":2}), 3);
    hub.attach_native("ABC", native_tests::native_match(), 3)
        .unwrap();
    messages(&mut rx);
    messages(&mut guest_rx);
    hub.message(
        "ABC",
        &host,
        &mut true,
        json!({"t":"level","slot":1,"v":"easy"}),
        4,
    );
    assert_eq!(
        messages(&mut rx),
        [json!({"t":"deny","cmd":"level","reason":"blocked"})]
    );
    assert!(messages(&mut guest_rx).is_empty());
}

#[test]
fn net41_r1_restart_repeats_settings_resets_pause_and_reconnects_absent_seat() {
    let mut hub = Hub::new(32, String::new());
    hub.configure_trusted_room(Some(crate::TrustedRoomConfig::new("ABC", 3936341).unwrap()));
    let (host, mut rx) = peer(1);
    let (guest, mut guest_rx) = peer(2);
    for (connection, token) in [(&host, "host"), (&guest, "guest")] {
        hub.message(
            "ABC",
            connection,
            &mut false,
            json!({"t":"hello","token":token,"name":token}),
            0,
        );
    }
    hub.message(
        "ABC",
        &host,
        &mut true,
        json!({"t":"weather","v":"rain"}),
        0,
    );
    hub.message("ABC", &host, &mut true, json!({"t":"start"}), 1);
    let first = messages(&mut rx)
        .into_iter()
        .find(|m| m["t"] == "start")
        .unwrap();
    messages(&mut guest_rx);
    let original = hub.rooms["ABC"]
        .native
        .as_ref()
        .unwrap()
        .receipt
        .clone()
        .unwrap();
    hub.rooms
        .get_mut("ABC")
        .unwrap()
        .native
        .as_mut()
        .unwrap()
        .runtime
        .advance_tick(&[false, false])
        .unwrap();
    assert!(hub.native_status("ABC").unwrap().tick > 0);
    hub.rooms
        .get_mut("ABC")
        .unwrap()
        .native
        .as_mut()
        .unwrap()
        .runtime
        .pause(0, "host", crate::room_runtime::PauseReason::Host, 2)
        .unwrap();
    hub.message("ABC", &guest, &mut true, json!({"t":"restart"}), 3);
    assert_eq!(hub.native_status("ABC").unwrap().epoch, 1);
    assert!(hub.native_status("ABC").unwrap().paused);
    messages(&mut guest_rx);
    hub.message("ABC", &host, &mut true, json!({"t":"restart"}), 4);
    let restarted = messages(&mut rx);
    assert_eq!(
        restarted
            .iter()
            .map(|m| m["t"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["pause", "lobby", "start"]
    );
    assert_eq!(restarted[0]["paused"], false);
    assert_eq!(restarted[1]["mode"], "conquest");
    let start = &restarted[2];
    assert_ne!(start["matchId"], first["matchId"]);
    for field in [
        "map", "names", "teams", "factions", "spawn", "spawns", "sandbox",
    ] {
        assert_eq!(start[field], first[field], "{field}");
    }
    let receipt = hub.rooms["ABC"]
        .native
        .as_ref()
        .unwrap()
        .receipt
        .clone()
        .unwrap();
    for recorded in [&original, &receipt] {
        assert!(recorded["options"]["weatherSeed"].as_u64().unwrap() < (1 << 31));
        assert_eq!(
            recorded["options"]["weatherSeed"],
            recorded["derivedWeatherSeed"]
        );
    }
    let mut expected = original.clone();
    expected["matchId"] = start["matchId"].clone();
    expected["options"]["weatherSeed"] = receipt["options"]["weatherSeed"].clone();
    expected["derivedWeatherSeed"] = receipt["options"]["weatherSeed"].clone();
    assert_eq!(receipt, expected);
    assert_eq!(hub.rooms["ABC"].weather, "rain");
    assert_eq!(hub.native_status("ABC").unwrap().tick, 0);
    assert!(!hub.native_status("ABC").unwrap().paused);
    messages(&mut guest_rx);
    hub.disconnected("ABC", guest.id, 5);
    messages(&mut rx);
    hub.message("ABC", &host, &mut true, json!({"t":"restart"}), 6);
    let offline_restart = messages(&mut rx);
    assert_eq!(
        offline_restart.iter().find(|m| m["t"] == "start").unwrap()["matchId"],
        3
    );
    assert!(
        offline_restart
            .iter()
            .any(|m| m["t"] == "pause" && m["paused"] == true && m["reason"] == "drop")
    );
    assert!(hub.native_status("ABC").unwrap().paused);
    let (returned, mut returned_rx) = peer(3);
    hub.message(
        "ABC",
        &returned,
        &mut false,
        json!({"t":"hello","token":"guest","name":"Returned"}),
        7,
    );
    let rejoined = messages(&mut returned_rx);
    assert_eq!(rejoined[0]["t"], "start");
    assert_eq!(rejoined[0]["matchId"], 3);
    assert_eq!(rejoined[0]["you"], 1);
    assert!(!hub.native_status("ABC").unwrap().paused);
    hub.rooms
        .get_mut("ABC")
        .unwrap()
        .native
        .as_mut()
        .unwrap()
        .runtime
        .fault_stop("test fault".into());
    messages(&mut rx);
    hub.message("ABC", &host, &mut true, json!({"t":"restart"}), 8);
    assert_eq!(hub.native_status("ABC").unwrap().epoch, 3);
    assert!(hub.native_status("ABC").unwrap().fault.is_some());
    assert_eq!(
        messages(&mut rx),
        [json!({"t":"deny","cmd":"restart","reason":"scenario"})]
    );
}
