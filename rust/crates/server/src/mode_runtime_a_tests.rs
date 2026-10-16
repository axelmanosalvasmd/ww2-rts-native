use super::*;

struct Lobby {
    hub: Hub,
    host: Connection,
    guest: Connection,
    host_rx: mpsc::Receiver<Output>,
    guest_rx: mpsc::Receiver<Output>,
}

impl Lobby {
    fn new() -> Self {
        let mut hub = Hub::new(1, String::new());
        let mut join = |id, name| {
            let (tx, rx) = mpsc::channel(64);
            let (cancel, _) = watch::channel(false);
            let connection = Connection {
                id,
                tx,
                cancel,
                buffered: None,
            };
            hub.message(
                "MODE",
                &connection,
                &mut false,
                json!({"t":"hello","token":name,"name":name}),
                0,
            );
            (connection, rx)
        };
        let (host, host_rx) = join(1, "Host");
        let (guest, guest_rx) = join(2, "Guest");
        let mut lobby = Self {
            hub,
            host,
            guest,
            host_rx,
            guest_rx,
        };
        lobby.drain();
        lobby
    }

    fn send(&mut self, host: bool, message: Value) {
        let connection = if host { &self.host } else { &self.guest };
        self.hub
            .message("MODE", connection, &mut true, message, 1_000);
    }

    fn state(&self) -> Value {
        self.hub.rooms["MODE"].lobby(self.host.id, &self.hub.maps, "")
    }

    fn drain(&mut self) {
        while self.host_rx.try_recv().is_ok() {}
        while self.guest_rx.try_recv().is_ok() {}
    }

    fn changed(&mut self) {
        let expected = self.state();
        assert!(matches!(self.host_rx.try_recv(), Ok(Output::Json(value)) if value == expected));
        assert!(
            matches!(self.guest_rx.try_recv(), Ok(Output::Json(value)) if value["t"] == "lobby")
        );
        assert!(self.host_rx.try_recv().is_err());
        assert!(self.guest_rx.try_recv().is_err());
    }

    fn unchanged(&mut self, before: &Value) {
        assert_eq!(&self.state(), before);
        assert!(self.host_rx.try_recv().is_err());
        assert!(self.guest_rx.try_recv().is_err());
    }
}

#[test]
fn mode03_each_host_setting_rejects_guests_then_changes_and_broadcasts() {
    let mut cases = Vec::new();
    for (command, key, values) in [
        (
            "mode",
            "mode",
            vec![
                "conquest",
                "assault",
                "annihilation",
                "classic",
                "horde",
                "tutorial",
                "world",
            ],
        ),
        ("worldSize", "worldSize", vec!["huge", "massive"]),
        (
            "army",
            "army",
            vec!["standard", "large", "massive", "endless"],
        ),
        (
            "weather",
            "weather",
            vec!["map", "clear", "fog", "rain", "mud", "snow", "random"],
        ),
    ] {
        for value in &values {
            let initial = values.iter().find(|other| *other != value).unwrap();
            cases.push((
                json!({"t":command,"v":initial}),
                json!({"t":command,"v":value}),
                format!("/{key}"),
                json!(value),
            ));
        }
    }
    for value in [false, true] {
        cases.push((
            json!({"t":"sandbox","v":!value}),
            json!({"t":"sandbox","v":value}),
            "/sandbox".into(),
            json!(value),
        ));
    }
    for value in 0..6 {
        for (command, key, slot) in [
            ("team", "/players/0/team", Some(0)),
            ("team", "/players/1/team", Some(1)),
            ("team", "/players/2/team", Some(2)),
            ("defender", "/defenderTeam", None),
        ] {
            cases.push((
                json!({"t":command,"slot":slot,"v":(value + 1) % 6}),
                json!({"t":command,"slot":slot,"v":value}),
                key.into(),
                json!(value),
            ));
        }
    }
    for value in 0..4 {
        cases.push((
            json!({"t":"faction","slot":2,"v":(value + 1) % 4}),
            json!({"t":"faction","slot":2,"v":value}),
            "/players/2/faction".into(),
            json!(value),
        ));
    }
    for value in ["easy", "normal", "hard"] {
        cases.push((
            json!({"t":"level","slot":2,"v":if value == "easy" { "hard" } else { "easy" }}),
            json!({"t":"level","slot":2,"v":value}),
            "/players/2/level".into(),
            json!(value),
        ));
    }
    cases.push((
        json!({"t":"map","name":"default"}),
        json!({"t":"map","name":"bastogne"}),
        "/mapName".into(),
        json!("bastogne"),
    ));
    for (initial, command, path, expected) in cases {
        let mut lobby = Lobby::new();
        lobby.send(true, json!({"t":"addAi"}));
        lobby.send(true, initial);
        lobby.drain();
        let before = lobby.state();
        assert_ne!(before.pointer(&path), Some(&expected), "{command}");
        lobby.send(false, command.clone());
        lobby.unchanged(&before);
        lobby.send(true, command.clone());
        assert_eq!(lobby.state().pointer(&path), Some(&expected), "{command}");
        lobby.changed();
    }
}

#[test]
fn mode03_factions_belong_to_their_human_and_levels_only_to_ai_seats() {
    let mut lobby = Lobby::new();
    lobby.send(true, json!({"t":"addAi"}));
    lobby.drain();
    for (host, slot) in [(true, 0), (false, 1)] {
        for value in 0..4 {
            let initial = (value + 1) % 4;
            lobby.send(host, json!({"t":"faction","slot":slot,"v":initial}));
            lobby.drain();
            lobby.send(host, json!({"t":"faction","slot":slot,"v":value}));
            assert_eq!(lobby.state()["players"][slot]["faction"], value);
            lobby.changed();
        }
    }
    for (host, command) in [
        (true, json!({"t":"faction","slot":1,"v":0})),
        (false, json!({"t":"faction","slot":0,"v":0})),
        (true, json!({"t":"level","slot":0,"v":"hard"})),
        (true, json!({"t":"level","slot":1,"v":"hard"})),
    ] {
        let before = lobby.state();
        lobby.send(host, command);
        lobby.unchanged(&before);
        assert!(
            lobby.hub.rooms["MODE"].players[..2]
                .iter()
                .all(|person| person.level == "normal")
        );
    }
    assert_eq!(lobby.state()["players"][0]["level"], Value::Null);
    assert_eq!(lobby.state()["players"][1]["level"], Value::Null);
    assert_eq!(lobby.state()["players"][2]["level"], "normal");
}

#[test]
fn mode03_invalid_settings_and_mode_specific_restrictions_preserve_the_lobby() {
    let mut lobby = Lobby::new();
    lobby.send(true, json!({"t":"addAi"}));
    lobby.drain();
    for command in [
        json!({"t":"mode","v":"unknown"}),
        json!({"t":"map","name":"unknown"}),
        json!({"t":"worldSize","v":"small"}),
        json!({"t":"army","v":"small"}),
        json!({"t":"weather","v":"storm"}),
        json!({"t":"level","slot":2,"v":"expert"}),
        json!({"t":"level","slot":99,"v":"hard"}),
    ] {
        let before = lobby.state();
        lobby.send(true, command);
        lobby.unchanged(&before);
    }
    for (command, slot, limit) in [("team", 1, 6), ("faction", 0, 4), ("defender", 0, 6)] {
        for value in [json!(-1), json!(limit), json!(1.5), json!("1"), Value::Null] {
            let before = lobby.state();
            lobby.send(true, json!({"t":command,"slot":slot,"v":value}));
            lobby.unchanged(&before);
        }
    }
    lobby.send(true, json!({"t":"worldSize","size":"massive"}));
    assert_eq!(lobby.state()["worldSize"], "massive");
    lobby.changed();
    lobby.send(true, json!({"t":"mode","v":"tutorial"}));
    assert_eq!(lobby.state()["mapName"], "tutorial");
    lobby.drain();
    let before = lobby.state();
    lobby.send(true, json!({"t":"map","name":"default"}));
    lobby.unchanged(&before);
    lobby.send(true, json!({"t":"army","v":"endless"}));
    lobby.send(true, json!({"t":"mode","v":"horde"}));
    assert_eq!(lobby.state()["army"], "standard");
    assert_eq!(lobby.hub.maps[&lobby.hub.rooms["MODE"].map]["horde"], true);
    lobby.drain();
    for command in [
        json!({"t":"army","v":"endless"}),
        json!({"t":"map","name":"default"}),
    ] {
        let before = lobby.state();
        lobby.send(true, command);
        lobby.unchanged(&before);
    }
}

#[test]
fn mode03_attached_match_locks_every_setting_for_host_and_guest() {
    let mut lobby = Lobby::new();
    lobby
        .hub
        .attach_native("MODE", super::native_tests::native_match(), 0)
        .unwrap();
    lobby.drain();
    let before = lobby.state();
    assert_eq!(before["state"], "play");
    let epoch = lobby.hub.native_status("MODE").unwrap().epoch;
    for (index, command) in [
        json!({"t":"mode","v":"assault"}),
        json!({"t":"map","name":"bastogne"}),
        json!({"t":"worldSize","v":"massive"}),
        json!({"t":"army","v":"large"}),
        json!({"t":"sandbox","v":true}),
        json!({"t":"weather","v":"rain"}),
        json!({"t":"defender","v":5}),
        json!({"t":"team","slot":1,"v":5}),
        json!({"t":"faction","slot":0,"v":3}),
        json!({"t":"faction","slot":1,"v":3}),
        json!({"t":"level","slot":1,"v":"hard"}),
    ]
    .into_iter()
    .enumerate()
    {
        for host in [true, false] {
            let connection = if host { &lobby.host } else { &lobby.guest };
            lobby.hub.message(
                "MODE",
                connection,
                &mut true,
                command.clone(),
                (index as u64 + 1) * 1_001,
            );
            let receiver = if host {
                &mut lobby.host_rx
            } else {
                &mut lobby.guest_rx
            };
            assert!(matches!(receiver.try_recv(), Ok(Output::Json(value))
                if value == json!({"t":"deny","cmd":command["t"],"reason":"blocked"})));
            lobby.unchanged(&before);
            assert_eq!(lobby.hub.native_status("MODE").unwrap().epoch, epoch);
        }
    }
}
