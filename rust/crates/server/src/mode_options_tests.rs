//! MODE-11, LOG-01 and NET-39 switches. World and authored scenario execution stay guarded.
use super::*;
use ww2_core::map::AuthoritativeMap;

fn peers() -> (
    Hub,
    Connection,
    mpsc::Receiver<Output>,
    mpsc::Receiver<Output>,
) {
    let mut hub = Hub::new(1, String::new());
    hub.configure_trusted_room(Some(crate::TrustedRoomConfig::new("ABC", 701).unwrap()));
    let mut host = None;
    let mut receivers = Vec::new();
    for id in 1..=2 {
        let (tx, rx) = mpsc::channel(64);
        let (cancel, _) = watch::channel(false);
        let connection = Connection::new(id, tx, cancel);
        hub.message(
            "ABC",
            &connection,
            &mut false,
            json!({"t":"hello","token":format!("p{id}"),"name":format!("p{id}")}),
            0,
        );
        if id == 1 {
            host = Some(connection);
        }
        receivers.push(rx);
    }
    let mut guest_rx = receivers.pop().unwrap();
    let mut host_rx = receivers.pop().unwrap();
    while host_rx.try_recv().is_ok() {}
    while guest_rx.try_recv().is_ok() {}
    (hub, host.unwrap(), host_rx, guest_rx)
}

fn players() -> Vec<PlayerConfig> {
    vec![
        PlayerConfig {
            name: "Allies".into(),
            team: 0,
            faction: Faction::Usa,
        },
        PlayerConfig {
            name: "Axis".into(),
            team: 1,
            faction: Faction::Germany,
        },
    ]
}

fn map() -> AuthoritativeMap {
    AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"Mode switches", "w":100,"h":100,
            "rows":vec![".".repeat(100);100],
            "spawns":[{"x":8,"y":8},{"x":91,"y":91}],
            "defend":[0],
            "points":[{"id":"first","x":40,"y":40,"mp":1},
                      {"id":"second","x":60,"y":60,"mp":1}]
        }))
        .unwrap(),
    )
    .unwrap()
}

fn tick_every_seat(runtime: &mut RoomRuntime, logistics: bool) {
    let count = runtime.state().world.game.players.len();
    let seats = (0..count)
        .map(|slot| SelectedSeat {
            slot,
            full: true,
            omit: false,
        })
        .collect::<Vec<_>>();
    for _ in 0..4 {
        runtime.advance_tick(&vec![false; count]).unwrap();
        let frames = runtime.collect_frame(&seats, FrameKind::Scheduled).unwrap();
        assert_eq!(frames.len(), count);
        for frame in frames {
            assert_eq!(
                frame.message.get("tech").is_some(),
                runtime.state().options.tech
            );
            if logistics {
                assert_eq!(frame.message["logistics"]["enabled"], true);
            } else {
                assert!(frame.message.get("logistics").is_none());
            }
        }
        assert!(runtime.fault().is_none());
    }
}

#[test]
fn net39_room_options_map_all_modes_armies_weather_sandbox_and_roster() {
    let (mut hub, _, _host_rx, _guest_rx) = peers();
    let room = hub.rooms.get_mut("ABC").unwrap();
    room.map = "selected-authoring-key".into();
    for (name, mode) in [
        ("conquest", Mode::Conquest),
        ("assault", Mode::Assault),
        ("annihilation", Mode::Annihilation),
        ("classic", Mode::Classic),
        ("horde", Mode::Horde),
        ("tutorial", Mode::Tutorial),
        ("world", Mode::World),
    ] {
        room.mode = name.into();
        for (army, expected_army) in [
            ("standard", ArmySize::Standard),
            ("large", ArmySize::Large),
            ("massive", ArmySize::Massive),
            ("endless", ArmySize::Endless),
        ] {
            room.army = army.into();
            for weather in ["map", "clear", "fog", "rain", "mud", "snow", "random"] {
                room.weather = weather.into();
                for sandbox in [false, true] {
                    room.sandbox = sandbox;
                    let options = room
                        .native_match_options(701, players(), &json!({}))
                        .unwrap();
                    assert_eq!(options.mode, mode);
                    assert_eq!(options.army, expected_army);
                    assert_eq!(
                        options.weather,
                        WeatherChoice::from_reference(Some(weather))
                    );
                    assert_eq!(options.seed, 701);
                    assert_eq!(options.weather_seed, None);
                    assert_eq!(options.sandbox, sandbox);
                    assert_eq!(options.map_key.as_deref(), Some("selected-authoring-key"));
                    assert!(
                        options.shuffle && options.living_weather && options.tech && options.supply
                    );
                    assert_eq!(options.players.len(), 2);
                    for (actual, expected) in options.players.iter().zip(players()) {
                        assert_eq!(actual.name, expected.name);
                        assert_eq!(actual.team, expected.team);
                        assert_eq!(actual.faction, expected.faction);
                    }
                    assert_eq!(
                        options.logistics,
                        matches!(name, "conquest" | "classic" | "annihilation" | "world")
                    );
                }
            }
        }
    }
    room.mode = "unknown".into();
    assert!(
        room.native_match_options(701, players(), &json!({}))
            .is_err()
    );
}

#[test]
fn log01_scenarios_and_loaded_false_override_mode_selection_before_native_guards() {
    let (mut hub, _, _host_rx, _guest_rx) = peers();
    let room = hub.rooms.get_mut("ABC").unwrap();
    for (name, enabled) in [
        ("conquest", true),
        ("classic", true),
        ("annihilation", true),
        ("world", true),
        ("assault", false),
        ("horde", false),
        ("tutorial", false),
    ] {
        room.mode = name.into();
        for loaded in [None, Some(false), Some(true)] {
            room.loaded_logistics = loaded;
            for scenario in [
                json!({}),
                json!({"scenario":null}),
                json!({"scenario":false}),
                json!({"scenario":{}}),
                json!({"scenario":{"logistics":false}}),
                json!({"scenario":{"logistics":true}}),
            ] {
                let mut options = room
                    .native_match_options(701, players(), &scenario)
                    .unwrap();
                let server_enabled = loaded != Some(false)
                    && if scenario["scenario"].is_object() {
                        scenario["scenario"]["logistics"] == true
                    } else {
                        enabled
                    };
                assert_eq!(
                    options.logistics, server_enabled,
                    "{name}/{loaded:?}/{scenario}"
                );
                options.resolve_mode_features();
                assert_eq!(
                    options.logistics,
                    server_enabled && enabled,
                    "native {name}/{scenario}"
                );
            }
        }
    }
}

#[test]
fn mode11_headless_explicit_opt_ins_resolve_the_complete_mode_switch_table() {
    let (hub, _, _host_rx, _guest_rx) = peers();
    let options = hub.rooms["ABC"]
        .native_match_options(701, players(), &json!({}))
        .unwrap();
    // Expected outputs from sim.js:729,746,799, including an absent headless mode.
    for (mode, logistics, tech, supply) in [
        (Mode::Default, true, true, true),
        (Mode::Conquest, true, true, true),
        (Mode::Classic, true, true, true),
        (Mode::Annihilation, true, true, true),
        (Mode::Assault, false, true, true),
        (Mode::Horde, false, true, false),
        (Mode::Tutorial, false, false, false),
        (Mode::World, true, true, false),
        (Mode::Other, false, false, true),
    ] {
        for input in 0..8 {
            let mut selected = options.clone();
            selected.mode = mode;
            selected.logistics = input & 1 != 0;
            selected.tech = input & 2 != 0;
            selected.supply = input & 4 != 0;
            selected.resolve_mode_features();
            assert_eq!(selected.logistics, logistics && input & 1 != 0);
            assert_eq!(selected.tech, tech && input & 2 != 0);
            assert_eq!(selected.supply, supply && input & 4 != 0);
        }
    }
}

#[test]
fn mode11_existing_native_constructors_keep_switches_bases_and_every_seat_frames() {
    let authored = map();
    let (mut hub, _, _host_rx, _guest_rx) = peers();
    let room = hub.rooms.get_mut("ABC").unwrap();
    for (name, logistics, tech, supply, barracks) in [
        ("conquest", true, true, true, true),
        ("assault", false, true, true, true),
        ("annihilation", true, true, true, true),
        ("classic", true, true, true, false),
        ("tutorial", false, false, false, false),
    ] {
        room.mode = name.into();
        room.defender = 1;
        for loaded_tech in [None, Some(false)] {
            room.loaded_tech = loaded_tech;
            let options = room
                .native_match_options(701, players(), authored.source())
                .unwrap();
            let mut state = room.create_native_match(&authored, options).unwrap();
            assert_eq!(state.options.logistics, logistics, "{name}");
            assert_eq!(
                state.options.tech,
                tech && loaded_tech != Some(false),
                "{name}"
            );
            assert_eq!(state.options.supply, supply, "{name}");
            let has_barracks = state
                .world
                .game
                .units
                .values()
                .any(|unit| state.rules.kinds[unit.kind].name == "barracks");
            assert_eq!(has_barracks, barracks, "{name}");
            let hqs = state
                .world
                .game
                .units
                .values()
                .filter(|unit| state.rules.kinds[unit.kind].name == "hq")
                .count();
            assert_eq!(hqs, if name == "tutorial" { 0 } else { 2 }, "{name}");
            if name == "assault" {
                assert_eq!(state.assault.as_ref().unwrap().defender_team, 1);
            }
            for control in state.commands.units.values_mut() {
                control.hold_fire = true;
                control.auto_cast = false;
            }
            // Assault delivery is admitted by this chain; Tutorial remains separate.
            if name == "tutorial" {
                for _ in 0..4 {
                    state.advance_tick().unwrap();
                }
                assert!(
                    RoomRuntime::new(state, 0).is_err(),
                    "{name} snapshot admission stays guarded"
                );
            } else {
                let mut runtime = RoomRuntime::new(state, 0).unwrap();
                tick_every_seat(&mut runtime, logistics);
            }
        }
    }
}

#[test]
fn mode11_headless_default_and_conquest_require_explicit_logistics_and_keep_disabled_frames() {
    let (hub, _, _host_rx, _guest_rx) = peers();
    let authored = map();
    for mode in [Mode::Default, Mode::Conquest] {
        for logistics in [false, true] {
            let mut options = hub.rooms["ABC"]
                .native_match_options(701, players(), authored.source())
                .unwrap();
            options.mode = mode;
            options.logistics = logistics;
            let mut state = Match::new(&authored, options).unwrap();
            assert_eq!(state.convoys.is_some(), logistics);
            assert_eq!(state.logistics.iter().next().is_some(), logistics);
            for control in state.commands.units.values_mut() {
                control.hold_fire = true;
                control.auto_cast = false;
            }
            let mut runtime = RoomRuntime::new(state, 0).unwrap();
            tick_every_seat(&mut runtime, logistics);
        }
    }
}

#[test]
fn mode11_horde_switches_and_shared_base_support_healthy_hosted_ticks() {
    let (mut hub, _, _host_rx, _guest_rx) = peers();
    let room = hub.rooms.get_mut("ABC").unwrap();
    room.mode = "horde".into();
    let authored = map();
    let options = room
        .native_match_options(701, players(), authored.source())
        .unwrap();
    let mut state = room.create_native_match(&authored, options).unwrap();
    assert!(!state.options.logistics && !state.options.supply && state.options.tech);
    assert_eq!(state.world.game.players.len(), 3);
    let hqs = state
        .world
        .game
        .units
        .values()
        .filter(|unit| state.rules.kinds[unit.kind].name == "hq")
        .collect::<Vec<_>>();
    assert_eq!(hqs.len(), 1);
    assert_eq!(hqs[0].owner, 0);
    for slot in 0..3 {
        assert_eq!(
            state.production.player(slot).unwrap().tech().tier(),
            if slot == 2 { 3 } else { 2 }
        );
    }
    state.advance_tick().unwrap();
    assert!(state.tick_state.fault().is_none());
    let mut runtime = RoomRuntime::new(state, 0).unwrap();
    runtime.advance_tick(&[false; 3]).unwrap();
    assert!(runtime.fault().is_none());
    room.mode = "world".into();
    let options = room
        .native_match_options(701, players(), authored.source())
        .unwrap();
    assert!(options.logistics && options.tech);
    assert!(
        room.create_native_match(&authored, options)
            .err()
            .unwrap()
            .contains("World room admission is deferred")
    );
}

#[test]
fn log01_loaded_switches_survive_restart_and_socket_messages_cannot_toggle_them() {
    struct SaveDir(std::path::PathBuf);
    impl Drop for SaveDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let directory = SaveDir(std::env::temp_dir().join(format!(
        "ww2-mode-options-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )));
    for logistics in [false, true] {
        for tech in [false, true] {
            let (mut hub, host, mut host_rx, mut guest_rx) = peers();
            hub.configure_saves(directory.0.clone());
            let authored = crate::authoritative_maps::load("default").unwrap();
            let room = &hub.rooms["ABC"];
            let mut options = room
                .native_match_options(701, players(), authored.source())
                .unwrap();
            options.logistics = logistics;
            options.tech = tech;
            let state = room.create_native_match(authored, options).unwrap();
            let structures = state.world.structures.clone();
            assert!(structures.iter().any(|structure| structure.kind == "house"));
            hub.attach_native("ABC", state, 0).unwrap();
            let id = hub.save_room("ABC", host.id).unwrap();
            hub.rooms.get_mut("ABC").unwrap().native = None;
            hub.load_room("ABC", host.id, &id, 1).unwrap();
            assert_eq!(
                hub.rooms["ABC"]
                    .native
                    .as_ref()
                    .unwrap()
                    .runtime
                    .state()
                    .world
                    .structures,
                structures
            );
            assert_eq!(hub.rooms["ABC"].loaded_logistics, Some(logistics));
            assert_eq!(hub.rooms["ABC"].loaded_tech, Some(tech));
            while host_rx.try_recv().is_ok() {}
            while guest_rx.try_recv().is_ok() {}
            for (command, current) in [("logistics", logistics), ("tech", tech)] {
                hub.message(
                    "ABC",
                    &host,
                    &mut true,
                    json!({"t":command,"v":!current}),
                    2,
                );
            }
            assert_eq!(hub.rooms["ABC"].loaded_logistics, Some(logistics));
            assert_eq!(hub.rooms["ABC"].loaded_tech, Some(tech));
            hub.start_trusted("ABC", host.id, true, 3).unwrap();
            let runtime = &mut hub
                .rooms
                .get_mut("ABC")
                .unwrap()
                .native
                .as_mut()
                .unwrap()
                .runtime;
            assert_eq!(runtime.state().options.logistics, logistics);
            assert_eq!(runtime.state().options.tech, tech);
            tick_every_seat(runtime, logistics);
        }
    }
}
