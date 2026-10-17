//! Prototype e4e579a sim.js:5895-5897,6017-6019; story.js:66-70; server.js:740-754.
use super::*;
use ww2_core::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::Point,
    map::AuthoritativeMap,
    match_runtime::{MatchOptions, PlayerConfig},
    recipient_snapshot::{SelectedSeat, SnapshotCache},
    weather::WeatherChoice,
};

fn setup(mode: Mode) -> (Hub, Vec<Connection>, Vec<mpsc::Receiver<Output>>) {
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"Results ordinary", "w":100,"h":100,
            "rows":vec![".".repeat(100);100], "heights":vec!["0".repeat(100);100],
            "spawns":[{"x":8,"y":8},{"x":91,"y":91}], "defend":[0], "assaultTime":300,
            "points":[{"x":50,"y":50,"mp":2,"vp":1}]
        }))
        .unwrap(),
    )
    .unwrap();
    let options = MatchOptions {
        seed: 701,
        mode,
        players: [0, 2]
            .into_iter()
            .enumerate()
            .map(|(slot, team)| PlayerConfig {
                name: format!("Core {slot}"),
                team,
                faction: Faction::Usa,
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
    };
    let mut state = match mode {
        Mode::Conquest => Match::new(&map, options),
        Mode::Assault => Match::new_assault(&map, options, 0),
        Mode::Annihilation => Match::new_annihilation(&map, options),
        Mode::Classic => Match::new_classic(&map, options),
        _ => unreachable!(),
    }
    .unwrap();
    // Real counters have non-integral values so publication must round each key.
    for (slot, key) in StoryKey::ALL.into_iter().enumerate() {
        state
            .record_story_tally(Some(slot % 2), key, slot as f64 + 0.6)
            .unwrap();
    }
    let mut hub = Hub::new(32, String::new());
    let mut connections = Vec::new();
    let mut receivers = Vec::new();
    for slot in 0..2 {
        let (tx, rx) = mpsc::channel(256);
        let (cancel, _) = watch::channel(false);
        let connection = Connection::new(slot + 1, tx, cancel);
        hub.message(
            "ABC",
            &connection,
            &mut false,
            json!({"t":"hello","token":format!("p{slot}"),"name":format!("Seat {slot}")}),
            0,
        );
        connections.push(connection);
        receivers.push(rx);
    }
    let mode_name = match mode {
        Mode::Conquest => "conquest",
        Mode::Assault => "assault",
        Mode::Annihilation => "annihilation",
        Mode::Classic => "classic",
        _ => unreachable!(),
    };
    let room = hub.rooms.get_mut("ABC").unwrap();
    room.mode = mode_name.into();
    for (slot, person) in room.players.iter_mut().enumerate() {
        person.team = slot * 2;
        person.faction = 0;
    }
    let prepared = hub
        .prepare_native_room(&hub.rooms["ABC"], state, 0, false, Some(map.source()))
        .unwrap();
    if mode == Mode::Classic {
        let cache = SnapshotCache::new(prepared.attached.runtime.state()).unwrap();
        for slot in 0..2 {
            assert_eq!(cache.recipient(slot).unwrap().fields["ghosts"], json!([]));
        }
    }
    hub.publish_native("ABC", prepared);
    for receiver in &mut receivers {
        drain(&mut hub, receiver, 0);
    }
    (hub, connections, receivers)
}

fn drain(hub: &mut Hub, receiver: &mut mpsc::Receiver<Output>, now: u64) -> Vec<Value> {
    let mut messages = Vec::new();
    while let Ok(output) = receiver.try_recv() {
        match output {
            Output::Json(v) => messages.push(v),
            Output::Snapshot(write) => {
                messages.push(serde_json::from_str(&write.json).unwrap());
                hub.complete_snapshot(&write, true, now);
            }
            other => panic!("unexpected output {other:?}"),
        }
    }
    messages
}

// JSON has one number type. serde_json distinguishes integer zero from 0.0.
fn assert_json(actual: &Value, expected: &Value) {
    match (actual, expected) {
        (Value::Number(a), Value::Number(b)) => assert_eq!(a.as_f64(), b.as_f64()),
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len());
            for (a, b) in a.iter().zip(b) {
                assert_json(a, b);
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(a.keys().collect::<Vec<_>>(), b.keys().collect::<Vec<_>>());
            for (key, b) in b {
                assert_json(&a[key], b);
            }
        }
        _ => assert_eq!(actual, expected),
    }
}

fn kill(state: &mut Match, owner: usize, kind: &str) {
    let id = state
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == owner && state.rules.kinds[u.kind].name == kind)
        .unwrap()
        .id;
    state.world.game.units.get_mut(&id).unwrap().hp = 0.;
}

fn assert_mode(state: &Match, snapshot: &Value, slot: usize) {
    let expected = match state.options.mode {
        Mode::Conquest => None,
        Mode::Assault => {
            let a = state.assault.unwrap();
            Some(json!({"kind":"assault", "defenderTeam":0,
            "attackerTeam":2,"timeLeft":a.time_left.ceil().max(0.),"suddenDeath":false,"total":1}))
        }
        Mode::Annihilation => {
            Some(json!({"kind":"annihilation","suddenDeath":false,"bunkers":[1,0,1]}))
        }
        Mode::Classic => {
            let a = state.elimination.as_ref().unwrap();
            Some(json!({"kind":"classic",
            "timeLeft":a.time_left.unwrap().ceil().max(0.),"suddenDeath":a.sudden_death}))
        }
        _ => unreachable!(),
    };
    assert_eq!(snapshot.get("mode"), expected.as_ref());
    assert_eq!(
        snapshot["out"],
        json!(
            state
                .start
                .economy
                .players
                .iter()
                .map(|p| p.out)
                .collect::<Vec<_>>()
        )
    );
    if state.options.mode == Mode::Classic {
        let nodes = &state.elimination.as_ref().unwrap().nodes;
        assert!(!nodes.is_empty());
        for key in ["mun", "fuel"] {
            let p = &state.start.economy.players[slot];
            let value = if key == "mun" { p.mun } else { p.fuel };
            assert_eq!(snapshot[key].as_f64(), Some(value.floor()));
        }

        assert_eq!(
            snapshot["nodes"],
            json!(
                nodes
                    .iter()
                    .map(|n| [
                        ww2_core::math::round(n.at.x * 10.) / 10.,
                        ww2_core::math::round(n.at.z * 10.) / 10.,
                        n.rate,
                        f64::from(u8::from(n.fuel))
                    ])
                    .collect::<Vec<_>>()
            )
        );
    } else {
        assert!(snapshot.get("nodes").is_none());
    }
    assert!(snapshot.get("story").is_none());
    assert!(snapshot.get("timeline").is_none());
    if let Some(finish) = state.finish {
        if matches!(state.options.mode, Mode::Annihilation | Mode::Classic) {
            assert!(snapshot["out"][1].as_bool().unwrap());
            if finish.winner == -1 {
                assert_eq!(snapshot["out"], json!([true, true]));
            }
        } else {
            assert_eq!(snapshot["out"], json!([false, false]));
        }
        assert_eq!(
            snapshot["end"],
            json!({"reason":finish.reason.as_str(),"x":finish.end_at.x,"z":finish.end_at.z})
        );
    } else {
        assert!(snapshot.get("end").is_none());
    }
}

fn run(mode: Mode, timer: bool, draw: bool) {
    let (mut hub, _, mut receivers) = setup(mode);
    if timer {
        let state = hub
            .rooms
            .get_mut("ABC")
            .unwrap()
            .native
            .as_mut()
            .unwrap()
            .runtime
            .state_mut();
        if mode == Mode::Assault {
            state.assault.as_mut().unwrap().time_left = 0.13;
        } else {
            state.elimination.as_mut().unwrap().time_left = Some(0.13);
        }
    }
    let selected = (0..2)
        .map(|slot| SelectedSeat {
            slot,
            full: true,
            omit: false,
        })
        .collect::<Vec<_>>();
    let mut final_report = None;
    let mut observed_sudden = false;
    let mut expected = None;
    for callback in 0..160_u64 {
        let now = callback * 50;
        if callback == 5 {
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
                Mode::Conquest => {
                    state.start.points[0].owner = Some(0);
                    state.start.win_vp = 0.01;
                }
                Mode::Assault if !timer => kill(state, 0, "bunker"),
                Mode::Annihilation => kill(state, 1, "bunker"),
                Mode::Classic => {
                    kill(state, 1, "hq");
                    if draw {
                        kill(state, 0, "hq");
                    }
                }
                _ => {}
            }
        }
        hub.poll_native(now, now * 1_000_000);
        let status = hub.native_status("ABC").unwrap();
        assert!(status.fault.is_none(), "{mode:?}: {status:?}");
        if let Some(native) = hub.rooms.get_mut("ABC").unwrap().native.as_mut() {
            let frames = native
                .runtime
                .collect_frame(&selected, FrameKind::Current)
                .unwrap();
            assert_eq!(frames.len(), 2);
            for frame in frames {
                assert_mode(native.runtime.state(), &frame.message, frame.slot);
            }
            observed_sudden |= native
                .runtime
                .state()
                .elimination
                .as_ref()
                .is_some_and(|s| s.sudden_death);
            if expected.is_none() && native.runtime.state().finish.is_some() {
                let report = native.runtime.state().match_result().unwrap().unwrap();
                let timeline = report
                    .timeline
                    .iter()
                    .map(|sample| match sample {
                        MatchSample::Conquest { t, vp } => json!({"t":t,"vp":vp}),
                        MatchSample::Territories {
                            t,
                            points,
                            structures,
                        } => json!({"t":t,"points":points,"structures":structures}),
                    })
                    .collect::<Vec<_>>();
                let story = report
                    .story
                    .iter()
                    .map(|row| {
                        StoryKey::ALL
                            .into_iter()
                            .map(|key| (key.as_str().to_owned(), json!(row.value(key))))
                            .collect::<serde_json::Map<_, _>>()
                    })
                    .collect::<Vec<_>>();
                expected = Some(
                    json!({"winner":report.winner,"teams":[0,2],"names":["Seat 0","Seat 1"],
                    "reason":report.reason.as_str(),"at":{"x":report.at.x,"z":report.at.z},"endTick":report.end_tick,
                    "story":story,"timeline":timeline,"winVp":report.win_vp,"mode":report.mode}),
                );
            }
        }
        for (slot, rx) in receivers.iter_mut().enumerate() {
            for msg in drain(&mut hub, rx, now) {
                if msg["t"] == "lobby" && msg["state"] == "lobby" {
                    assert_eq!(status.held, 120);
                    let mut report = msg["result"].clone();
                    let you = report.as_object_mut().unwrap().remove("you").unwrap();
                    assert_json(&report, expected.as_ref().unwrap());
                    let winner = report["winner"].as_i64().unwrap();
                    assert_eq!(
                        you,
                        json!({"team":slot*2,"outcome":if winner==-1 {"draw"} else if winner==slot as i64*2 {"victory"} else {"defeat"}})
                    );
                    final_report = Some(report);
                }
            }
        }
        if status.ending_complete {
            break;
        }
    }
    let report = final_report.expect("natural result published after complete hold");
    assert_eq!(
        report["mode"],
        match mode {
            Mode::Conquest => "conquest",
            Mode::Assault => "assault",
            Mode::Annihilation => "annihilation",
            Mode::Classic => "classic",
            _ => unreachable!(),
        }
    );
    if timer && mode == Mode::Classic {
        assert!(observed_sudden);
    }
    if draw {
        assert_eq!(report["winner"], -1);
        assert_eq!(report["reason"], "draw");
    }
    assert!(hub.rooms["ABC"].native.is_none());
}

#[test]
fn mode96_ordinary_modes_deliver_exact_rows_through_victory_and_full_hold() {
    for mode in [Mode::Assault, Mode::Annihilation, Mode::Classic] {
        run(mode, false, false);
    }
    run(Mode::Assault, true, false);
    run(Mode::Classic, true, false);
}

#[test]
fn mode90_complete_ordinary_results_publish_story_timeline_and_each_seat_outcome() {
    for mode in [
        Mode::Conquest,
        Mode::Assault,
        Mode::Annihilation,
        Mode::Classic,
    ] {
        run(mode, false, false);
    }
    run(Mode::Classic, false, true);
}

#[test]
fn mode90_host_end_publishes_only_ended_and_clears_pause_after_live_snapshots() {
    let (mut hub, connections, mut receivers) = setup(Mode::Conquest);
    for tick in 0..5 {
        hub.poll_native(tick * 50, tick * 50_000_000);
        for rx in &mut receivers {
            drain(&mut hub, rx, tick * 50);
        }
        assert!(hub.native_status("ABC").unwrap().fault.is_none());
    }
    hub.message("ABC", &connections[1], &mut true, json!({"t":"end"}), 300);
    assert!(hub.rooms["ABC"].native.is_some());
    assert!(
        drain(&mut hub, &mut receivers[1], 300)
            .iter()
            .any(|v| *v == json!({"t":"deny","cmd":"end","reason":"blocked"}))
    );
    hub.rooms
        .get_mut("ABC")
        .unwrap()
        .native
        .as_mut()
        .unwrap()
        .runtime
        .pause(0, "Seat 0", crate::room_runtime::PauseReason::Host, 301)
        .unwrap();
    hub.message("ABC", &connections[0], &mut true, json!({"t":"end"}), 302);
    for rx in &mut receivers {
        let messages = drain(&mut hub, rx, 302);
        let lobby = messages.iter().find(|v| v["t"] == "lobby").unwrap();
        assert_eq!(lobby["result"], json!({"ended":true,"you":null}));
    }
    assert!(!hub.native_status("ABC").unwrap().paused);
    hub.poll_native(400, 400_000_000);
    assert!(hub.rooms["ABC"].native.is_none());
}

#[test]
fn mode96_closed_modes_and_mismatched_runtime_stay_refused() {
    let (mut hub, _, _) = setup(Mode::Conquest);
    let state = hub
        .rooms
        .get_mut("ABC")
        .unwrap()
        .native
        .as_mut()
        .unwrap()
        .runtime
        .state_mut();
    for mode in [Mode::Horde, Mode::World, Mode::Tutorial] {
        state.options.mode = mode;
        assert!(SnapshotCache::new(state).is_err());
    }
    state.options.mode = Mode::Classic;
    assert!(SnapshotCache::new(state).is_err());
    state.options.mode = Mode::Conquest;
    state.fallen.structure = Some(Point { x: 1., z: 1. });
    assert!(SnapshotCache::new(state).is_ok());
}

#[test]
fn mode96_assault_attachment_refuses_lobby_defender_mismatch() {
    let (mut hub, _, _) = setup(Mode::Assault);
    let native = hub.rooms["ABC"].native.as_ref().unwrap();
    let raw = native.startup.raw_map().clone();
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&raw).unwrap()).unwrap();
    let state = Match::new_assault(&map, native.runtime.state().options.clone(), 0).unwrap();
    hub.rooms.get_mut("ABC").unwrap().defender = 2;
    let error = hub
        .prepare_native_room(&hub.rooms["ABC"], state, 0, true, Some(&raw))
        .err()
        .unwrap();
    assert!(error.contains("defender disagrees"), "{error}");
    assert_eq!(hub.native_status("ABC").unwrap().epoch, 1);
    assert!(hub.native_status("ABC").unwrap().fault.is_none());
}
