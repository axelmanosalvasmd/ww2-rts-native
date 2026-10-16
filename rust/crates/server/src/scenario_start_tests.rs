//! MODE-81 startup admission and room publication boundaries.
use super::*;
use ww2_core::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    map::AuthoritativeMap,
    match_runtime::{MatchOptions, PlayerConfig},
    weather::WeatherChoice,
};

fn authored() -> Value {
    json!({"name":"Scenario start","w":64,"h":64,"rows":vec![".".repeat(64);64],
        "spawns":[{"x":4,"y":4},{"x":58,"y":58},{"x":4,"y":58}],
        "points":[{"id":"front","x":32,"y":32}],
        "scenario":{"version":1,"areas":[],"objectives":[],
            "groups":[{"id":"initial","units":[{"id":"leader","type":"rifle","side":0,"x":20,"y":20}]}],
            "triggers":[{"id":"reinforce","scope":"match","condition":{"kind":"time","seconds":0.05},
                "actions":[{"kind":"reinforce","group":"initial","side":0,"at":[24,24],
                    "roster":[{"type":"rifle","count":1}],"order":{"kind":"hold"},"expires":5}]}]}})
}
fn options() -> MatchOptions {
    MatchOptions {
        seed: 81,
        mode: Mode::Conquest,
        players: (0..2)
            .map(|slot| PlayerConfig {
                name: format!("p{slot}"),
                team: slot,
                faction: Faction::try_from(slot as u8).unwrap(),
            })
            .collect(),
        shuffle: false,
        army: ArmySize::Standard,
        weather: WeatherChoice::Map,
        weather_seed: Some(1),
        living_weather: false,
        map_key: Some("default".into()),
        tech: false,
        supply: false,
        logistics: false,
        sandbox: false,
    }
}
fn construct(raw: &Value) -> Result<Match, String> {
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(raw).unwrap())
        .map_err(|e| e.to_string())?;
    Match::new(&map, options())
}
fn room() -> (
    Hub,
    Connection,
    mpsc::Receiver<Output>,
    mpsc::Receiver<Output>,
) {
    let mut hub = Hub::new(32, String::new());
    hub.configure_trusted_room(Some(crate::TrustedRoomConfig::new("ABC", 81).unwrap()));
    let mut peers = Vec::new();
    for id in 1..=2 {
        let (tx, rx) = mpsc::channel(64);
        let (cancel, _) = watch::channel(false);
        let peer = Connection::new(id, tx, cancel);
        hub.message(
            "ABC",
            &peer,
            &mut false,
            json!({"t":"hello","name":format!("p{id}"),"token":format!("token{id}")}),
            0,
        );
        peers.push((peer, rx));
    }
    let (_, mut guest_rx) = peers.pop().unwrap();
    let (host, mut host_rx) = peers.pop().unwrap();
    while host_rx.try_recv().is_ok() {}
    while guest_rx.try_recv().is_ok() {}
    (hub, host, host_rx, guest_rx)
}
fn identity(hub: &Hub, host: u64) -> (Value, u64, String, Option<Value>) {
    let room = &hub.rooms["ABC"];
    (
        room.lobby(host, &hub.maps, &hub.public_url),
        hub.next_epoch,
        format!(
            "{:?}",
            hub.accounts
                .iter()
                .map(|(id, a)| (
                    id,
                    &a.room,
                    a.epoch,
                    a.slot,
                    &a.delivery,
                    &a.pressure,
                    a.full_terrain
                ))
                .collect::<Vec<_>>()
        ),
        room.native.as_ref().map(|native| {
            json!({
                "state":native.runtime.state().save_json().unwrap(),
                "epoch":native.epoch,"receipt":native.receipt,
                "map":native.startup.raw_map(),"fault":native.runtime.fault(),
            })
        }),
    )
}
fn invalid_candidates() -> Vec<(&'static str, Value)> {
    let mut cases = Vec::new();
    for (label, path) in [
        ("authored faction", "authored"),
        ("reinforcement faction", "reinforcement"),
    ] {
        let mut raw = authored();
        if path == "authored" {
            raw["scenario"]["groups"][0]["units"][0]["type"] = json!("tiger");
        } else {
            raw["scenario"]["triggers"][0]["actions"][0]["roster"][0]["type"] = json!("tiger");
        }
        cases.push((label, raw));
    }
    for (label, kind) in [
        ("construction type in Conquest", "engineer"),
        ("naval type on land map", "gunboat"),
    ] {
        for initial in [true, false] {
            let mut raw = authored();
            if initial {
                raw["scenario"]["groups"][0]["units"][0]["type"] = json!(kind);
            } else {
                raw["scenario"]["triggers"][0]["actions"][0]["roster"][0]["type"] = json!(kind);
            }
            cases.push((label, raw));
        }
    }
    let mut raw = authored();
    let mut rows = vec![".".repeat(64); 64];
    rows[20].replace_range(20..21, "B");
    raw["rows"] = json!(rows);
    cases.push(("authored unit on blocked entry", raw));
    let mut raw = authored();
    raw["scenario"]["objectives"] = json!([{"id":"absent","text":"Absent","side":2}]);
    cases.push(("objective side absent from selected match", raw));
    let mut raw = authored();
    raw["scenario"]["triggers"][0]["side"] = json!(2);
    cases.push(("trigger side absent from selected match", raw));
    let mut raw = authored();
    raw["scenario"]["triggers"][0]["condition"] =
        json!({"kind":"ownership","target":"front","side":2});
    cases.push(("condition side absent from selected match", raw));
    let mut raw = authored();
    raw["scenario"]["groups"][0]["units"][0]["side"] = json!(2);
    cases.push(("authored side absent from selected match", raw));
    let mut raw = authored();
    raw["scenario"]["triggers"][0]["actions"][0]["side"] = json!(2);
    cases.push(("reinforcement side absent from selected match", raw));
    let mut raw = authored();
    raw["scenario"]["triggers"][0]["condition"] =
        json!({"kind":"ownership","target":"missing","side":0});
    cases.push(("missing capture target rejected by map validation", raw));
    let mut raw = authored();
    raw["scenario"]["groups"][0]["units"][0]["type"] = json!("missing");
    cases.push(("unknown type rejected by map validation", raw));
    let mut raw = authored();
    raw["worldVersion"] = json!(2);
    raw["scenario"]["triggers"][0]["actions"][0]["roster"][0]["type"] = json!("armoredcar");
    let mut objects = vec![".".repeat(64); 64];
    let mut mines = objects.clone();
    let mut rows = objects.clone();
    objects[24].replace_range(24..25, "R");
    mines[24].replace_range(24..25, "N");
    rows[24].replace_range(24..25, "N");
    raw["rows"] = json!(rows);
    raw["layers"] = json!({"ground":vec![".".repeat(64);64],"objects":objects,"mines":mines});
    cases.push(("rubble entry hidden under a mine", raw));
    cases
}

#[test]
fn mode81_custom_candidate_validation_and_preparation_leave_lobby_and_live_room_unchanged() {
    let (mut hub, host, mut host_rx, mut guest_rx) = room();
    let raw = authored();
    let valid = construct(&raw).unwrap();
    let facts = serde_json::to_value(valid.authored_scenario.as_ref().unwrap()).unwrap();
    let leader = facts["facts"]["authored_units"]["leader"].as_u64().unwrap() as u32;
    assert!(valid.world.game.units.contains_key(&leader));
    let candidates = invalid_candidates();
    // The third authored spawn makes side 2 valid map data, but it is absent from this match.
    for (label, raw) in &candidates {
        if !label.contains("map validation") {
            assert!(
                AuthoritativeMap::from_json(&serde_json::to_vec(raw).unwrap()).is_ok(),
                "{label}"
            );
        }
    }
    for live in [false, true] {
        if live {
            let prepared = hub
                .prepare_native_room(
                    &hub.rooms["ABC"],
                    construct(&raw).unwrap(),
                    0,
                    false,
                    Some(&raw),
                )
                .unwrap();
            assert!(
                prepared
                    .attached
                    .runtime
                    .state()
                    .world
                    .game
                    .units
                    .values()
                    .any(|u| u.owner == 0 && u.x == 41.0 && u.z == 41.0)
            );
            assert_eq!(prepared.starts.len(), 2);
            for (_, start) in &prepared.starts {
                assert!(start["map"].get("scenario").is_none());
                assert!(
                    start.get("scenario").is_none(),
                    "scenario rows arrive in snapshots"
                );
            }
            hub.publish_native("ABC", prepared);
            while host_rx.try_recv().is_ok() {}
            while guest_rx.try_recv().is_ok() {}
        }
        for (label, invalid) in &candidates {
            let before = identity(&hub, host.id);
            let error = match construct(invalid) {
                Ok(state) => {
                    match hub.prepare_native_room(&hub.rooms["ABC"], state, 0, live, Some(invalid))
                    {
                        Ok(_) => panic!("{label} unexpectedly prepared"),
                        Err(error) => error,
                    }
                }
                Err(error) => error,
            };
            assert!(
                error.contains("scenario") || error == "invalid authored deferred arrival",
                "{label}: {error}"
            );
            assert_eq!(identity(&hub, host.id), before, "{label}, live={live}");
            assert!(host_rx.try_recv().is_err() && guest_rx.try_recv().is_err());
        }
        // Exercise preparation refusal after a valid candidate already exists.
        let mut wrong_binding = raw.clone();
        wrong_binding["rows"][30] = json!(format!("{}T{}", ".".repeat(30), ".".repeat(33)));
        let before = identity(&hub, host.id);
        assert!(
            hub.prepare_native_room(
                &hub.rooms["ABC"],
                construct(&raw).unwrap(),
                0,
                live,
                Some(&wrong_binding)
            )
            .is_err()
        );
        assert_eq!(identity(&hub, host.id), before);
        assert!(host_rx.try_recv().is_err() && guest_rx.try_recv().is_err());
    }
    let runtime = &mut hub
        .rooms
        .get_mut("ABC")
        .unwrap()
        .native
        .as_mut()
        .unwrap()
        .runtime;
    let tick = runtime.state().world.game.tick;
    for _ in 0..4 {
        runtime.advance_tick(&[false; 2]).unwrap();
        let selected = (0..2)
            .map(|slot| SelectedSeat {
                slot,
                full: true,
                omit: false,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            runtime
                .collect_frame(&selected, FrameKind::Scheduled)
                .unwrap()
                .len(),
            2
        );
        assert!(runtime.fault().is_none());
    }
    assert_eq!(runtime.state().world.game.tick, tick + 4);
    let selected = (0..2)
        .map(|slot| SelectedSeat {
            slot,
            full: true,
            omit: false,
        })
        .collect::<Vec<_>>();
    let frames = runtime
        .collect_frame(&selected, FrameKind::Current)
        .unwrap();
    assert_eq!(frames.len(), 2);
    assert!(runtime.fault().is_none());
    for frame in frames {
        assert_eq!(frame.message["t"], "s");
        assert_eq!(
            frame.message["scenario"],
            json!({"version":1,"objectives":[]})
        );
    }
}

#[test]
fn mode81_trusted_catalog_refusal_denies_start_and_restart_without_publication() {
    let (mut hub, host, mut host_rx, mut guest_rx) = room();
    for restart in [false, true] {
        if restart {
            hub.rooms.get_mut("ABC").unwrap().map = "default".into();
            hub.start_trusted("ABC", host.id, false, 0).unwrap();
            while host_rx.try_recv().is_ok() {}
            while guest_rx.try_recv().is_ok() {}
        }
        hub.rooms.get_mut("ABC").unwrap().map = "absent-authored-map".into();
        let before = identity(&hub, host.id);
        let command = if restart { "restart" } else { "start" };
        assert!(
            hub.start_trusted("ABC", host.id, restart, 10_000)
                .unwrap_err()
                .contains("unknown authored map")
        );
        hub.message("ABC", &host, &mut true, json!({"t":command}), 10_000);
        assert_eq!(identity(&hub, host.id), before);
        let Output::Json(denial) = host_rx.try_recv().unwrap() else {
            panic!("expected deny")
        };
        assert_eq!(
            denial,
            json!({"t":"deny","cmd":command,"reason":"scenario"})
        );
        assert!(host_rx.try_recv().is_err() && guest_rx.try_recv().is_err());
    }
    let runtime = &mut hub
        .rooms
        .get_mut("ABC")
        .unwrap()
        .native
        .as_mut()
        .unwrap()
        .runtime;
    let tick = runtime.state().world.game.tick;
    for _ in 0..4 {
        runtime.advance_tick(&[false; 2]).unwrap();
        let selected = (0..2)
            .map(|slot| SelectedSeat {
                slot,
                full: true,
                omit: false,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            runtime
                .collect_frame(&selected, FrameKind::Scheduled)
                .unwrap()
                .len(),
            2
        );
        assert!(runtime.fault().is_none());
    }
    assert_eq!(runtime.state().world.game.tick, tick + 4);
    let selected = (0..2)
        .map(|slot| SelectedSeat {
            slot,
            full: true,
            omit: false,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        runtime
            .collect_frame(&selected, FrameKind::Current)
            .unwrap()
            .len(),
        2
    );
    assert!(runtime.fault().is_none());
}
