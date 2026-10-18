//! Complete ordinary delivery for the existing mode and room-option matrix.
use super::mode_options_tests::{map, peers, players, tick_every_seat};
use super::*;

struct TestDir(std::path::PathBuf);
impl TestDir {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        Self(std::env::temp_dir().join(format!(
            "ww2-logistics3-{}-{}",
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

fn drain(hub: &mut Hub, rx: &mut mpsc::Receiver<Output>, now: u64) -> Vec<Value> {
    let mut messages = Vec::new();
    while let Ok(output) = rx.try_recv() {
        match output {
            Output::Watcher(_) => panic!("watcher output on seated connection"),
            Output::Json(message) => messages.push(message),
            Output::Snapshot(write) => {
                messages.push(serde_json::from_str(&write.json).unwrap());
                hub.complete_snapshot(&write, true, now);
            }
            Output::Close(code, reason) => panic!("unexpected close {code}: {reason}"),
        }
    }
    messages
}

fn install_map(hub: &mut Hub, dir: &TestDir, raw: &Value) {
    let editor = crate::MapEditor::open(dir.0.join("maps"), "test-password").unwrap();
    editor
        .save("default", &serde_json::to_vec(raw).unwrap())
        .unwrap();
    hub.configure_maps(Some(editor));
}

fn scenario_map(logistics: Option<bool>) -> Value {
    let mut raw = map().source().clone();
    raw["scenario"] = json!({"version":1,"groups":[],"areas":[],
        "objectives":[{"id":"supply","text":"Hold the supply line"}],
        "triggers":[{"id":"supply-order","scope":"match",
            "condition":{"kind":"time","seconds":0.05},
            "actions":[{"kind":"objectiveActivate","objective":"supply"}]}]});
    if let Some(enabled) = logistics {
        raw["scenario"]["logistics"] = json!(enabled);
    }
    raw
}

#[test]
fn mode11_net39_ordinary_mode_army_weather_sandbox_matrix_delivers_every_seat() {
    let authored = map();
    let (mut hub, _, _host_rx, _guest_rx) = peers();
    let room = hub.rooms.get_mut("ABC").unwrap();
    for mode in ["conquest", "assault", "annihilation", "classic"] {
        room.mode = mode.into();
        room.defender = 1;
        for army in ["standard", "large", "massive", "endless"] {
            room.army = army.into();
            for weather in ["map", "clear", "fog", "rain", "mud", "snow", "random"] {
                room.weather = weather.into();
                for sandbox in [false, true] {
                    room.sandbox = sandbox;
                    let options = room
                        .native_match_options(701, 391, players(), authored.source())
                        .unwrap();
                    let mut state = room.create_native_match(&authored, options).unwrap();
                    assert_eq!(
                        state.options.mode,
                        crate::native_startup::ordinary_mode(mode).unwrap()
                    );
                    assert_eq!(state.options.army.key(), army);
                    assert_eq!(
                        state.options.weather,
                        WeatherChoice::from_reference(Some(weather))
                    );
                    assert_eq!(state.options.sandbox, sandbox);
                    assert_eq!(state.options.map_key.as_deref(), Some("default"));
                    assert!(
                        state.options.shuffle
                            && state.options.tech
                            && state.options.supply
                            && state.options.living_weather
                    );
                    assert_eq!(state.options.weather_seed, Some(391));
                    assert_eq!(state.start.weather_seed, 391);
                    assert_eq!(state.options.logistics, mode != "assault");
                    if mode == "assault" {
                        assert_eq!(state.assault.as_ref().unwrap().defender_team, 1);
                    }
                    for control in state.commands.units.values_mut() {
                        control.hold_fire = true;
                        control.auto_cast = false;
                    }
                    let mut runtime = RoomRuntime::new(state, 0).unwrap();
                    tick_every_seat(&mut runtime, mode != "assault");
                    assert!(runtime.state().finish.is_none());
                }
            }
        }
    }
}

#[test]
fn log01_scenario_opt_in_runs_all_ordinary_modes_and_every_seat_objectives() {
    let dir = TestDir::new();
    for mode in ["conquest", "assault", "annihilation", "classic"] {
        for selected in [None, Some(false), Some(true)] {
            let (mut hub, host, mut host_rx, mut guest_rx) = peers();
            install_map(&mut hub, &dir, &scenario_map(selected));
            let room = hub.rooms.get_mut("ABC").unwrap();
            room.mode = mode.into();
            room.defender = 1;
            let expected = selected == Some(true) && mode != "assault";
            hub.message("ABC", &host, &mut true, json!({"t":"start"}), 0);
            for (slot, rx) in [&mut host_rx, &mut guest_rx].into_iter().enumerate() {
                let messages = drain(&mut hub, rx, 0);
                assert!(
                    messages.iter().all(|m| m["t"] != "deny"),
                    "{mode}/{selected:?}: {messages:?}"
                );
                let start = messages.iter().find(|m| m["t"] == "start").unwrap();
                assert_eq!(start["you"], slot);
                assert!(start["map"].get("scenario").is_none());
                assert!(start.get("logistics").is_none());
            }
            for now in [50, 150, 250] {
                hub.poll_native(now, now * 1_000_000);
                for rx in [&mut host_rx, &mut guest_rx] {
                    let messages = drain(&mut hub, rx, now);
                    let snapshot = messages.iter().find(|m| m["t"] == "s").unwrap();
                    assert_eq!(snapshot.get("logistics").is_some(), expected);
                    let objectives = snapshot["scenario"]["objectives"].as_array().unwrap();
                    assert_eq!(objectives.len(), 1);
                    assert_eq!(objectives[0]["id"], "supply");
                    assert_eq!(objectives[0]["state"], "active");
                }
                assert!(hub.native_status("ABC").unwrap().fault.is_none());
            }
            let state = hub.rooms["ABC"].native.as_ref().unwrap().runtime.state();
            assert_eq!(state.world.game.tick, 6);
            assert_eq!(state.options.logistics, expected);
            assert_eq!(state.convoys.is_some(), expected);
            assert_eq!(state.logistics.iter().next().is_some(), expected);
        }
    }
}

#[test]
fn log01_net39_loaded_false_restart_preserves_overrides_in_all_ordinary_modes_and_scenarios() {
    let dir = TestDir::new();
    for mode in ["conquest", "assault", "annihilation", "classic"] {
        for scenario in [false, true] {
            for tech in [false, true] {
                let (mut hub, host, mut host_rx, mut guest_rx) = peers();
                hub.configure_saves(dir.0.join("saves"));
                let raw = if scenario {
                    scenario_map(Some(true))
                } else {
                    map().source().clone()
                };
                install_map(&mut hub, &dir, &raw);
                let room = hub.rooms.get_mut("ABC").unwrap();
                room.mode = mode.into();
                room.defender = 1;
                // A checkpoint can carry false even in modes/maps that normally enable logistics.
                room.loaded_logistics = Some(false);
                room.loaded_tech = Some(tech);
                hub.start_trusted("ABC", host.id, false, 0).unwrap();
                for rx in [&mut host_rx, &mut guest_rx] {
                    drain(&mut hub, rx, 0);
                }
                let id = hub.save_room("ABC", host.id).unwrap();
                let save = hub.saves.read("ABC", &id).unwrap();
                assert_eq!(save["settings"]["logistics"], false);
                assert_eq!(save["version"], if scenario { 2 } else { 1 });
                hub.rooms.get_mut("ABC").unwrap().native = None;
                // Loading, rather than prior in-memory room settings, must recover the override.
                hub.rooms.get_mut("ABC").unwrap().loaded_logistics = None;
                hub.rooms.get_mut("ABC").unwrap().loaded_tech = None;
                hub.load_room("ABC", host.id, &id, 1).unwrap();
                assert_eq!(hub.rooms["ABC"].loaded_logistics, Some(false));
                assert_eq!(hub.rooms["ABC"].loaded_tech, Some(tech));
                for rx in [&mut host_rx, &mut guest_rx] {
                    drain(&mut hub, rx, 1);
                }
                for command in ["logistics", "tech"] {
                    hub.message("ABC", &host, &mut true, json!({"t":command,"v":true}), 2);
                }
                assert_eq!(hub.rooms["ABC"].loaded_logistics, Some(false));
                assert_eq!(hub.rooms["ABC"].loaded_tech, Some(tech));
                hub.message("ABC", &host, &mut true, json!({"t":"restart"}), 3);
                for (slot, rx) in [&mut host_rx, &mut guest_rx].into_iter().enumerate() {
                    let messages = drain(&mut hub, rx, 3);
                    let start = messages.iter().find(|m| m["t"] == "start").unwrap();
                    assert_eq!(start["you"], slot);
                    assert!(start.get("logistics").is_none());
                    assert!(start.get("tech").is_none());
                }
                let runtime = &mut hub
                    .rooms
                    .get_mut("ABC")
                    .unwrap()
                    .native
                    .as_mut()
                    .unwrap()
                    .runtime;
                assert!(!runtime.state().options.logistics);
                assert_eq!(runtime.state().options.tech, tech);
                assert!(runtime.state().convoys.is_none());
                tick_every_seat(runtime, false);
                if scenario {
                    assert!(runtime.state().authored_scenario.is_some());
                }
            }
        }
    }
}
