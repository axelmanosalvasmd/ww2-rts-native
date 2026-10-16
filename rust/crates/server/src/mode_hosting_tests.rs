//! Ordinary modes must survive public R1 Start, save/load and result delivery.
use super::*;

fn drain(hub: &mut Hub, rx: &mut mpsc::Receiver<Output>, now: u64) -> Vec<Value> {
    let mut messages = Vec::new();
    while let Ok(output) = rx.try_recv() {
        match output {
            Output::Json(message) => messages.push(message),
            Output::Snapshot(frame) => {
                messages.push(serde_json::from_str(&frame.json).unwrap());
                hub.complete_snapshot(&frame, true, now);
            }
            Output::Close(_, _) => panic!("ordinary recipient closed"),
        }
    }
    messages
}

fn lifecycle(mode: &str, count: usize, defender_timeout: bool) {
    let default_alias = mode == "default";
    let mode = if default_alias { "conquest" } else { mode };
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "ww2-modehost-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let mut hub = Hub::new(32, String::new());
    hub.configure_saves(path.clone());
    hub.configure_trusted_room(Some(crate::TrustedRoomConfig::new("ABC", 3936341).unwrap()));
    let mut peers = Vec::new();
    for slot in 0..count {
        let (tx, rx) = mpsc::channel(256);
        let (cancel, _) = watch::channel(false);
        let connection = Connection::new(slot as u64 + 1, tx, cancel);
        hub.message(
            "ABC",
            &connection,
            &mut false,
            json!({"t":"hello","token":format!("seat{slot}"),"name":format!("seat{slot}")}),
            0,
        );
        peers.push((connection, rx));
    }
    let host = peers[0].0.clone();
    hub.message("ABC", &host, &mut true, json!({"t":"mode","v":mode}), 0);
    if mode == "assault" {
        // Defender is not seat zero; the selected defender is used for deployment.
        hub.message("ABC", &host, &mut true, json!({"t":"defender","v":1}), 0);
    }
    for (_, rx) in &mut peers {
        drain(&mut hub, rx, 0);
    }
    // A guest cannot start any admitted mode.
    if count > 1 {
        hub.message("ABC", &peers[1].0, &mut true, json!({"t":"start"}), 0);
        assert!(hub.rooms["ABC"].native.is_none());
        for (_, rx) in &mut peers {
            drain(&mut hub, rx, 0);
        }
    }
    hub.message("ABC", &host, &mut true, json!({"t":"start"}), 0);
    let native = hub.rooms["ABC"]
        .native
        .as_ref()
        .expect("ordinary Start must publish");
    assert_eq!(native.receipt.as_ref().unwrap()["options"]["mode"], mode);
    assert_eq!(native.runtime.state().options.players.len(), count);
    for (slot, (_, rx)) in peers.iter_mut().enumerate() {
        let messages = drain(&mut hub, rx, 0);
        let starts = messages
            .iter()
            .filter(|m| m["t"] == "start")
            .collect::<Vec<_>>();
        assert_eq!(starts.len(), 1);
        assert_eq!(starts[0]["you"], slot);
        assert_eq!(starts[0]["spawns"].as_array().unwrap().len(), count);
        if mode == "assault" {
            assert_eq!(starts[0]["teams"], json!([0, 1]));
        }
    }
    for now in [50, 150, 250] {
        hub.poll_native(now, now * 1_000_000);
        for (_, rx) in &mut peers {
            let frames = drain(&mut hub, rx, now);
            assert_eq!(frames.len(), 1);
            let frame = &frames[0];
            if mode == "conquest" {
                assert!(frame.get("mode").is_none());
            } else {
                assert_eq!(frame["mode"]["kind"], mode);
            }
        }
        assert!(hub.native_status("ABC").unwrap().fault.is_none());
    }
    assert_eq!(hub.native_status("ABC").unwrap().tick, 6);
    if default_alias {
        // Default is the native no-mode alias. Restoring it exercises the same room Start envelope.
        hub.rooms
            .get_mut("ABC")
            .unwrap()
            .native
            .as_mut()
            .unwrap()
            .runtime
            .state_mut()
            .options
            .mode = Mode::Default;
    }
    let houses = hub.rooms["ABC"]
        .native
        .as_ref()
        .unwrap()
        .runtime
        .state()
        .world
        .structures
        .iter()
        .filter(|structure| structure.kind == "house")
        .cloned()
        .collect::<Vec<_>>();
    assert!(!houses.is_empty(), "save/load must cover authored houses");
    hub.message("ABC", &host, &mut true, json!({"t":"save"}), 250);
    let saved = drain(&mut hub, &mut peers[0].1, 250);
    let id = saved
        .iter()
        .find(|m| m["t"] == "saved")
        .expect("ordinary save")["id"]
        .as_str()
        .unwrap()
        .to_owned();
    for (_, rx) in &mut peers {
        drain(&mut hub, rx, 250);
    }
    let state = hub
        .rooms
        .get_mut("ABC")
        .unwrap()
        .native
        .as_mut()
        .unwrap()
        .runtime
        .state_mut();
    match mode {
        "conquest" => {
            state.start.economy.players[0].vp = state.start.win_vp;
        }
        "assault" if defender_timeout => {
            state.assault.as_mut().unwrap().time_left = 0.01;
        }
        "assault" | "annihilation" => {
            for unit in state.world.game.units.values_mut() {
                if state.rules.kinds[unit.kind].name == "bunker"
                    && (mode == "assault" || unit.owner != 0)
                {
                    unit.hp = 0.0;
                }
            }
        }
        "classic" => {
            // Enter sudden death and remove the other production HQs through real death cleanup.
            state.elimination.as_mut().unwrap().time_left = Some(0.01);
            for unit in state.world.game.units.values_mut() {
                if unit.owner != 0 && state.rules.kinds[unit.kind].building {
                    unit.hp = 0.0;
                }
            }
        }
        _ => unreachable!(),
    }
    let mut results = vec![None; count];
    let mut terminal_frames = vec![0; count];
    for callback in 0..130 {
        let now = 300 + callback * 50;
        hub.poll_native(now, now * 1_000_000);
        for (slot, (_, rx)) in peers.iter_mut().enumerate() {
            for packet in drain(&mut hub, rx, now) {
                assert_ne!(packet["t"], "deny");
                if packet["winner"].is_number() {
                    terminal_frames[slot] += 1;
                }
                if packet["t"] == "lobby" && !packet["result"].is_null() {
                    results[slot] = Some(packet["result"].clone());
                }
            }
        }
        assert!(hub.native_status("ABC").unwrap().fault.is_none());
        if hub.rooms["ABC"].native.is_none() {
            break;
        }
    }
    assert!(hub.native_status("ABC").unwrap().ending_complete);
    for (slot, result) in results.into_iter().enumerate() {
        let result = result.expect("every recipient gets result lobby");
        assert_eq!(result["mode"], mode);
        assert_eq!(result["winner"], if defender_timeout { 1 } else { 0 });
        assert_eq!(result["teams"].as_array().unwrap().len(), count);
        let samples = result["timeline"].as_array().unwrap();
        assert!(!samples.is_empty());
        for sample in samples {
            let mut keys = sample
                .as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>();
            keys.sort();
            assert_eq!(
                keys,
                if mode == "conquest" {
                    vec!["t", "vp"]
                } else {
                    vec!["points", "structures", "t"]
                }
            );
        }
        assert_eq!(result["winVp"].is_null(), mode != "conquest");
        assert!(terminal_frames[slot] >= 60);
    }
    assert!(hub.rooms["ABC"].native.is_none());
    hub.message(
        "ABC",
        &host,
        &mut true,
        json!({"t":"loadSave","id":id}),
        7000,
    );
    // Consume the lobby-to-load host-clock gap while the restored room is paused.
    hub.poll_native(7000, 7_000_000_000);
    assert_eq!(hub.native_status("ABC").unwrap().tick, 6);
    assert!(hub.native_status("ABC").unwrap().paused);
    let restored_houses = hub.rooms["ABC"]
        .native
        .as_ref()
        .unwrap()
        .runtime
        .state()
        .world
        .structures
        .iter()
        .filter(|structure| structure.kind == "house")
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(restored_houses, houses);
    for (_, rx) in &mut peers {
        assert_eq!(
            drain(&mut hub, rx, 7000)
                .iter()
                .filter(|m| m["t"] == "start")
                .count(),
            1
        );
    }
    hub.message("ABC", &host, &mut true, json!({"t":"resume"}), 7000);
    for (_, rx) in &mut peers {
        drain(&mut hub, rx, 7000);
    }
    for now in [7100, 7200, 7300] {
        hub.poll_native(now, now * 1_000_000);
        for (_, rx) in &mut peers {
            let frames = drain(&mut hub, rx, now);
            assert_eq!(frames.len(), 1);
            assert_eq!(frames[0]["t"], "s");
        }
        assert!(hub.native_status("ABC").unwrap().fault.is_none());
    }
    assert_eq!(hub.native_status("ABC").unwrap().tick, 12);
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn mode01_mode04_host_conquest_saves_and_result() {
    lifecycle("conquest", 2, false);
}
#[test]
fn mode01_mode04_host_classic_sudden_death_saves_and_result() {
    lifecycle("classic", 2, false);
}
#[test]
fn mode01_mode04_host_annihilation_elimination_saves_and_result() {
    lifecycle("annihilation", 2, false);
}
#[test]
fn mode01_mode04_host_assault_attacker_saves_and_result() {
    lifecycle("assault", 2, false);
}
#[test]
fn mode01_mode04_host_assault_defender_timeout_saves_and_result() {
    lifecycle("assault", 2, true);
}

#[test]
fn mode01_mode04_host_default_alias_restores_ticks_and_result() {
    lifecycle("default", 2, false);
}

#[test]
fn mode01_host_mode_guards_refuse_corrupt_pairs_and_missing_mode_owners() {
    use ww2_core::{economy::Faction, match_runtime::PlayerConfig};
    let map = crate::authoritative_maps::load("default").unwrap();
    for mode in [Mode::Classic, Mode::Annihilation, Mode::Assault] {
        let options = MatchOptions {
            seed: 3936341,
            mode,
            players: (0..2)
                .map(|slot| PlayerConfig {
                    name: format!("seat{slot}"),
                    team: slot,
                    faction: Faction::Usa,
                })
                .collect(),
            shuffle: true,
            army: ArmySize::Standard,
            weather: WeatherChoice::Map,
            weather_seed: None,
            living_weather: true,
            map_key: Some("default".into()),
            tech: true,
            supply: true,
            logistics: false,
            sandbox: false,
        };
        let make = || match mode {
            Mode::Classic => Match::new_classic(map, options.clone()).unwrap(),
            Mode::Annihilation => Match::new_annihilation(map, options.clone()).unwrap(),
            Mode::Assault => Match::new_assault(map, options.clone(), 0).unwrap(),
            _ => unreachable!(),
        };
        let mut corrupt = make();
        corrupt.world.game.mode = None;
        assert!(corrupt.validate_tick_state().is_err());
        assert!(corrupt.preflight_ordinary_snapshot().is_err());
        assert!(corrupt.ordinary_snapshot_raw_lists().is_err());
        assert!(corrupt.save_json().is_err());
        assert!(RoomRuntime::new(corrupt, 0).is_err());
        let mut missing = make();
        if mode == Mode::Assault {
            missing.assault = None;
        } else {
            missing.elimination = None;
        }
        assert!(missing.preflight_ordinary_snapshot().is_err());
        assert!(missing.save_json().is_err());
        assert!(RoomRuntime::new(missing, 0).is_err());
    }
}
