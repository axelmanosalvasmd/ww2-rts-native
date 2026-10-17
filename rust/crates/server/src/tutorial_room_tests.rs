use super::*;
use ww2_core::{
    match_commands::{Command, MoveOrder},
    tutorial_steps::TutorialStep,
};

struct Peer {
    connection: Connection,
    rx: mpsc::Receiver<Output>,
}
impl Peer {
    fn new(id: u64) -> Self {
        let (tx, rx) = mpsc::channel(256);
        let (cancel, _) = watch::channel(false);
        Self {
            connection: Connection::new(id, tx, cancel),
            rx,
        }
    }
    fn send(&self, hub: &mut Hub, value: Value) {
        hub.message("TUT", &self.connection, &mut true, value, 0);
    }
    fn join(&self, hub: &mut Hub, listing: Value) {
        hub.message("TUT", &self.connection, &mut false,
            json!({"t":"hello", "token":self.connection.id.to_string(), "name":format!("Human {}", self.connection.id), "listing":listing}), 0);
    }
    fn join_operative(&self, hub: &mut Hub) {
        hub.message(
            "TUT",
            &self.connection,
            &mut false,
            json!({"t":"hello", "token":self.connection.id.to_string(),
                "name":"Operative", "role":"operative", "commander":0}),
            0,
        );
    }
    fn drain(&mut self, hub: &mut Hub) -> Vec<Value> {
        let mut rows = vec![];
        while let Ok(output) = self.rx.try_recv() {
            match output {
                Output::Json(row) => rows.push(row),
                Output::Snapshot(write) => {
                    rows.push(serde_json::from_str(&write.json).unwrap());
                    hub.complete_snapshot(&write, true, 0);
                }
                Output::Watcher(_) => panic!("unexpected spectator output for commander"),
                Output::Close(_, reason) => panic!("unexpected close: {reason}"),
            }
        }
        rows
    }
}
fn hub() -> Hub {
    let mut hub = Hub::new(32, String::new());
    hub.match_seed = 0x7050;
    hub
}
fn runtime(hub: &mut Hub) -> &mut RoomRuntime {
    &mut hub
        .rooms
        .get_mut("TUT")
        .unwrap()
        .native
        .as_mut()
        .unwrap()
        .runtime
}
fn tick_all(hub: &mut Hub) -> Vec<Value> {
    let rt = runtime(hub);
    let count = rt.state().world.game.players.len();
    rt.advance_tick(&vec![false; count]).unwrap();
    capture_all(hub)
}
fn capture_all(hub: &mut Hub) -> Vec<Value> {
    let rt = runtime(hub);
    let count = rt.state().world.game.players.len();
    let selected = (0..count)
        .map(|slot| SelectedSeat {
            slot,
            full: false,
            omit: false,
        })
        .collect::<Vec<_>>();
    let events = rt.frame_events(FrameKind::Scheduled).to_vec();
    let frames = rt.collect_frame(&selected, FrameKind::Scheduled).unwrap();
    assert!(rt.fault().is_none());
    assert_eq!(frames.len(), count);
    let mut frames = frames.into_iter().map(|f| f.message).collect::<Vec<_>>();
    let room = hub.rooms.get_mut("TUT").unwrap();
    let state = room.native.as_mut().unwrap().runtime.state_mut();
    for op in &mut room.operatives {
        let key = String::from_utf16(&op.person.token).unwrap();
        let mut frame = op
            .stream
            .as_mut()
            .unwrap()
            .capture(state, false, &events)
            .unwrap();
        frame["operative"] = state.companion_status(&key);
        assert_eq!(frame["tick"], state.world.game.tick);
        assert_eq!(frame["t"], "s");
        frames.push(frame);
    }
    frames
}

fn operative_starts(hub: &mut Hub, operatives: &mut [Peer]) {
    for peer in operatives {
        let rows = peer.drain(hub);
        let starts = rows
            .iter()
            .filter(|row| row["t"] == "start")
            .collect::<Vec<_>>();
        assert_eq!(starts.len(), 1);
        let start = starts[0];
        assert_eq!(start["role"], "operative");
        assert_eq!(start["you"], 0);
        let room = &hub.rooms["TUT"];
        let op = room
            .operatives
            .iter()
            .find(|op| op.person.connection.as_ref().unwrap().id == peer.connection.id)
            .unwrap();
        let state = room.native.as_ref().unwrap().runtime.state();
        let companion = state
            .companions
            .get(&peer.connection.id.to_string())
            .unwrap();
        assert_eq!(start["operative"]["id"], companion.unit_id);
        assert_eq!(state.world.game.units[&companion.unit_id].owner, 0);
        assert!(state.commands.units[&companion.unit_id].operative);
        assert!(op.stream.is_some());
    }
    assert!(hub.native_status("TUT").unwrap().fault.is_none());
}

fn exercise_operatives(hub: &mut Hub, operatives: &[Peer]) {
    let ids = runtime(hub)
        .state()
        .companions
        .records
        .values()
        .map(|op| op.unit_id)
        .collect::<Vec<_>>();
    let before = ids
        .iter()
        .map(|id| runtime(hub).state().world.game.units[id].at())
        .collect::<Vec<_>>();
    let tutorial = runtime(hub).state().tutorial_state().is_some();
    let mut fired = vec![false; ids.len()];
    let mut threw = fired.clone();
    let mut flew = false;
    for sequence in 0..200 {
        for (peer, id) in operatives.iter().zip(&ids) {
            let at = runtime(hub).state().world.game.units[id].at();
            let moving = if tutorial { at.x < 32. } else { sequence < 8 };
            // Cross the authored hedgerow through the road, using FPS movement.
            let yaw = if tutorial && at.x < 30. && at.z < 43. {
                std::f64::consts::FRAC_PI_2
            } else if tutorial && at.x < 30. && at.z > 45. {
                -std::f64::consts::FRAC_PI_2
            } else {
                0.
            };
            let forward = if moving { 1. } else { 0. };
            hub.message(
                "TUT",
                &peer.connection,
                &mut true,
                json!({"t":"fps", "seq":sequence, "forward":forward, "strafe":0,
                    "yaw":yaw, "pitch":0, "fire":sequence < 6, "nade":if sequence > 0 { 1 } else { 0 }}),
                20 + sequence as u64 * 50,
            );
        }
        let frames = tick_all(hub);
        for (index, id) in ids.iter().enumerate() {
            for frame in &frames {
                for event in frame["shots"].as_array().unwrap() {
                    if event["f"] == *id {
                        threw[index] |= event["k"] == "throw";
                        fired[index] |= event["k"] != "throw";
                    }
                }
                flew |= frame["flights"]
                    .as_array()
                    .is_some_and(|rows| !rows.is_empty());
            }
        }
    }
    let state = runtime(hub).state();
    for (index, id) in ids.iter().enumerate() {
        let op = &state.companions.records[&state.companions.soldiers[id].companion];
        assert_eq!(op.last_seq, 199);
        assert!(op.shots > 0 && fired[index] && threw[index]);
        assert_eq!(state.companions.soldiers[id].nades, 2);
        let at = state.world.game.units[id].at();
        assert!((at.x - before[index].x).abs() + (at.z - before[index].z).abs() > 0.01);
        if tutorial {
            assert!(
                at.x >= 32.,
                "operative must cross the Tutorial hedgerow: {at:?}"
            );
        }
    }
    assert!(flew);
    assert!(state.firing.flights.is_empty());
}

fn ending_hold(hub: &mut Hub, peers: &mut [Peer], operatives: &mut [Peer]) {
    let mut frames = vec![0; operatives.len()];
    let mut commander_frames = vec![0; peers.len()];
    let tick = runtime(hub).state().world.game.tick;
    let statuses = operatives
        .iter()
        .map(|peer| {
            let key = peer.connection.id.to_string();
            let status = runtime(hub).state().companion_status(&key);
            hub.message(
                "TUT",
                &peer.connection,
                &mut true,
                json!({"t":"fps", "seq":200, "forward":1, "strafe":0,
                    "yaw":0, "pitch":0, "fire":true, "nade":2}),
                10_020,
            );
            assert_eq!(runtime(hub).state().companion_status(&key), status);
            status
        })
        .collect::<Vec<_>>();
    for callback in 0..120 {
        // Include unattended game seats in the ordinary ending snapshots.
        capture_all(hub);
        let now = (callback + 1) * 50;
        hub.poll_native(now, now * 1_000_000);
        for (index, peer) in peers.iter_mut().enumerate() {
            commander_frames[index] += peer.drain(hub).iter().filter(|row| row["t"] == "s").count();
        }
        for (index, peer) in operatives.iter_mut().enumerate() {
            for row in peer.drain(hub) {
                if row["t"] == "s" {
                    assert!(row["operative"].is_object());
                    assert!(row["winner"].as_i64().is_some());
                    for key in ["id", "seq", "shots", "nades"] {
                        assert_eq!(row["operative"][key], statuses[index][key]);
                    }
                    for key in [
                        "movement",
                        "queues",
                        "productionJobs",
                        "plans",
                        "orders",
                        "air",
                        "rally",
                        "works",
                        "covers",
                        "sup",
                    ] {
                        assert!(row.get(key).is_none(), "operative leaked {key}");
                    }
                    frames[index] += 1;
                }
            }
        }
        assert_eq!(hub.native_status("TUT").unwrap().fault, None);
    }
    let status = hub.native_status("TUT").unwrap();
    assert!(status.ending_complete);
    assert_eq!(status.held, 120);
    assert_eq!(status.tick, tick + 60);
    assert!(hub.rooms["TUT"].native.is_none());
    assert!(frames.iter().all(|count| *count == 60));
    assert!(commander_frames.iter().all(|count| *count == 60));
}

#[test]
fn net18_tutorial_hello_autostarts_once_and_delivers_every_seat() {
    let mut hub = hub();
    let mut peer = Peer::new(1);
    peer.join(&mut hub, json!({"tutorial":true,"public":true}));
    let rows = peer.drain(&mut hub);
    assert_eq!(
        rows.iter()
            .map(|r| r["t"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["lobby", "pause", "lobby", "start"]
    );
    assert_eq!(rows[0]["listed"], false);
    assert_eq!(rows[0]["state"], "lobby");
    assert_eq!(rows[2]["state"], "play");
    assert_eq!(hub.directory(), json!({"rooms":[]}));
    let start = &rows[3];
    assert_eq!(
        start
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        [
            "cells", "factions", "fog", "map", "matchId", "names", "sandbox", "spawn", "spawns",
            "t", "teams", "weather", "you"
        ]
    );
    assert_eq!(start["names"], json!(["Human 1", "Enemy"]));
    assert_eq!(start["teams"], json!([0, 1]));
    assert_eq!(start["factions"], json!([0, 1]));
    assert_eq!(start["you"], 0);
    assert_eq!(start["spawns"].as_array().unwrap().len(), 2);
    assert_eq!(start["spawn"], start["spawns"][0]);
    assert!(start["fog"].is_object());
    let epoch = start["matchId"].clone();
    // Exercise the actual maintenance presence arrays, then every core recipient.
    hub.poll_native(0, 0);
    hub.poll_native(100, 100_000_000);
    assert_eq!(hub.native_status("TUT").unwrap().fault, None);
    for row in peer.drain(&mut hub).into_iter().filter(|r| r["t"] == "s") {
        assert_eq!(row["online"], json!([true]));
        assert_eq!(row["ping"], json!([null]));
    }
    for _ in 0..6 {
        tick_all(&mut hub);
    }
    let mut replacement = Peer::new(2);
    hub.message(
        "TUT",
        &replacement.connection,
        &mut false,
        json!({"t":"hello","token":"1","name":"Returned","listing":{"tutorial":true}}),
        100,
    );
    let returned = replacement.drain(&mut hub);
    assert_eq!(
        returned.iter().find(|r| r["t"] == "start").unwrap()["matchId"],
        epoch
    );
    assert_eq!(returned.iter().filter(|r| r["t"] == "start").count(), 1);
    assert_eq!(hub.native_status("TUT").unwrap().fault, None);
    for _ in 0..6 {
        tick_all(&mut hub);
    }
}

#[test]
fn mode57_host_selected_tutorial_starts_one_to_five_humans_with_snapshots() {
    for count in 1..=5 {
        let mut hub = hub();
        let mut peers = (1..=count).map(Peer::new).collect::<Vec<_>>();
        for p in &peers {
            p.join(&mut hub, json!({}));
        }
        peers[0].send(&mut hub, json!({"t":"mode","v":"tutorial"}));
        peers[0].send(&mut hub, json!({"t":"map","name":"default"}));
        assert_eq!(hub.rooms["TUT"].map, "tutorial");
        for p in &mut peers {
            p.drain(&mut hub);
        }
        peers[0].send(&mut hub, json!({"t":"start"}));
        for (slot, p) in peers.iter_mut().enumerate() {
            let rows = p.drain(&mut hub);
            let start = rows.iter().find(|r| r["t"] == "start").unwrap();
            assert_eq!(start["you"], slot);
            assert_eq!(start["names"].as_array().unwrap().len(), count as usize + 1);
            assert_eq!(start["names"][count as usize], "Enemy");
            assert_eq!(start["spawns"][slot], start["spawns"][0]);
        }
        for _ in 0..6 {
            for row in tick_all(&mut hub) {
                assert_eq!(
                    row["mode"],
                    json!({"kind":"tutorial","suddenDeath":false,"slot":count,
                    "goal":"Move your squads up to the hedgerow","step":0,"steps":9})
                );
            }
        }
    }
}

#[test]
fn net36_public_start_checks_host_capacity_and_assault_readiness() {
    for (mode, map) in [
        ("conquest", "default"),
        ("assault", "default"),
        ("conquest", "king-of-the-hill"),
        ("assault", "king-of-the-hill"),
    ] {
        for count in 1..=6 {
            let mut hub = hub();
            let mut peers = (1..=count).map(Peer::new).collect::<Vec<_>>();
            for p in &peers {
                p.join(&mut hub, json!({"public":false}));
            }
            peers[0].send(&mut hub, json!({"t":"mode","v":mode}));
            peers[0].send(&mut hub, json!({"t":"map","name":map}));
            let capacity = hub.rooms["TUT"].capacity(&hub.maps);
            if count > 1 {
                peers[1].send(&mut hub, json!({"t":"start"}));
                assert!(hub.rooms["TUT"].native.is_none());
            }
            for p in &mut peers {
                p.drain(&mut hub);
            }
            peers[0].send(&mut hub, json!({"t":"start"}));
            if count as usize > capacity || (mode == "assault" && count == 1) {
                assert!(hub.rooms["TUT"].native.is_none());
                continue;
            }
            for p in &mut peers {
                assert!(p.drain(&mut hub).iter().any(|r| r["t"] == "start"));
            }
            for _ in 0..6 {
                for row in tick_all(&mut hub) {
                    if mode == "assault" {
                        assert_eq!(
                            row["mode"]
                                .as_object()
                                .unwrap()
                                .keys()
                                .map(String::as_str)
                                .collect::<Vec<_>>(),
                            [
                                "attackerTeam",
                                "defenderTeam",
                                "kind",
                                "suddenDeath",
                                "timeLeft",
                                "total"
                            ]
                        );
                        assert_eq!(row["mode"]["kind"], "assault");
                    }
                }
            }
        }
    }
    let mut hub = hub();
    let peers = [Peer::new(1), Peer::new(2)];
    for p in &peers {
        p.join(&mut hub, json!({}));
    }
    peers[0].send(&mut hub, json!({"t":"mode","v":"assault"}));
    peers[0].send(&mut hub, json!({"t":"team","slot":1,"v":0}));
    peers[0].send(&mut hub, json!({"t":"start"}));
    assert!(hub.rooms["TUT"].native.is_none(), "no attacker");
    peers[0].send(&mut hub, json!({"t":"defender","v":5}));
    peers[0].send(&mut hub, json!({"t":"start"}));
    assert!(hub.rooms["TUT"].native.is_none(), "no defender");
    peers[0].send(&mut hub, json!({"t":"team","slot":1,"v":5}));
    peers[0].send(&mut hub, json!({"t":"start"}));
    for _ in 0..6 {
        tick_all(&mut hub);
    }
}

fn execute(hub: &mut Hub, command: Command) {
    assert_eq!(
        runtime(hub)
            .state_mut()
            .execute_command(0, &command, false)
            .unwrap(),
        None
    );
}
fn go(hub: &mut Hub, at: ww2_core::game::Point) {
    let state = runtime(hub).state();
    let orders = state
        .world
        .game
        .units
        .values()
        .filter(|u| u.owner == 0 && u.hp > 0. && !state.commands.units[&u.id].operative)
        .map(|u| MoveOrder {
            unit: i64::from(u.id),
            at,
        })
        .collect();
    execute(
        hub,
        Command::Move {
            orders,
            control_ids: None,
            attack_move: false,
            queue: false,
            together: false,
            face: None,
        },
    );
}
fn kill_spawned(hub: &mut Hub) {
    let state = runtime(hub).state_mut();
    for id in state.tutorial_state().unwrap().spawned().to_vec() {
        if let Some(u) = state.world.game.units.get_mut(&id) {
            u.hp = 0.;
        }
    }
}
fn reach(hub: &mut Hub, step: TutorialStep, lines: &mut Vec<Value>) {
    for _ in 0..2000 {
        if runtime(hub).state().tutorial_state().unwrap().step() == step {
            return;
        }
        let frames = tick_all(hub);
        let said = |row: &Value| {
            row["shots"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|r| r["k"] == "say")
                .cloned()
                .collect::<Vec<_>>()
        };
        assert_eq!(
            said(&frames[0]),
            said(&frames[1]),
            "Tutorial dialogue is public"
        );
        lines.extend(
            frames[0]["shots"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|r| r["k"] == "say")
                .cloned(),
        );
    }
    panic!("did not reach {step:?}");
}

#[test]
fn tutorial_room_runs_all_steps_snapshots_dialogue_and_ending_hold() {
    use ww2_core::{game::Point, match_commands::Stance, match_end::EndReason};
    let mut hub = hub();
    let mut peer = Peer::new(1);
    peer.join(&mut hub, json!({}));
    peer.send(&mut hub, json!({"t":"mode","v":"tutorial"}));
    let mut operatives = [Peer::new(10), Peer::new(11)];
    operatives[0].join_operative(&mut hub);
    peer.send(&mut hub, json!({"t":"start"}));
    operatives[1].join_operative(&mut hub);
    operative_starts(&mut hub, &mut operatives);
    peer.drain(&mut hub);
    let mut lines = vec![];
    for tick in 1..=40 {
        let frames = tick_all(&mut hub);
        assert_eq!(frames[0]["shots"], frames[1]["shots"]);
        if tick < 40 {
            assert!(frames[0]["shots"].as_array().unwrap().is_empty());
        }
        lines.extend(frames[0]["shots"].as_array().unwrap().iter().cloned());
    }
    assert_eq!(lines.len(), 2);
    assert_eq!(
        lines[0],
        json!({"k":"say","text":"Sgt. Hollis: Morning, Lieutenant. The glider pilot missed the field by two miles, so we walk.","x":27.,"z":45.,"pub":true})
    );
    exercise_operatives(&mut hub, &operatives);
    peer.drain(&mut hub);
    let state = runtime(&mut hub).state();
    let ids = state
        .world
        .game
        .units
        .values()
        .filter(|unit| !state.commands.units[&unit.id].operative)
        .map(|unit| i64::from(unit.id))
        .collect();
    execute(
        &mut hub,
        Command::Stance {
            ids,
            stance: Stance::HoldFire,
            on: true,
        },
    );
    go(&mut hub, Point { x: 31., z: 45. });
    reach(&mut hub, TutorialStep::Orchard, &mut lines);
    // Resolve the scripted fights explicitly, as prototype test-tutorial.js does.
    kill_spawned(&mut hub);
    reach(&mut hub, TutorialStep::Crossroads, &mut lines);
    let at = runtime(&mut hub).state().start.points[0].at;
    go(&mut hub, at);
    reach(&mut hub, TutorialStep::Recruit, &mut lines);
    execute(
        &mut hub,
        Command::Buy {
            kind: "at".into(),
            from: None,
        },
    );
    reach(&mut hub, TutorialStep::Tank, &mut lines);
    kill_spawned(&mut hub);
    reach(&mut hub, TutorialStep::Trench, &mut lines);
    // Keep a real grenade ticket and its linked flight through room delivery.
    let state = runtime(&mut hub).state_mut();
    let mut at = state.world.game.units[&1].at();
    at.x += 12.;
    state
        .enqueue_command(
            ww2_core::orders::OrderKey {
                tick: state.world.game.tick as u64 + 1,
                issuer: 0,
                sequence: 1,
            },
            Command::Ability {
                ids: vec![1],
                at: Some(at),
                queue: false,
                automatic: false,
            },
        )
        .unwrap();
    let frames = tick_all(&mut hub);
    lines.extend(
        frames[0]["shots"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["k"] == "say")
            .cloned(),
    );
    assert!(!runtime(&mut hub).state().firing.grenades.is_empty());
    for _ in 0..25 {
        let frames = tick_all(&mut hub);
        lines.extend(
            frames[0]["shots"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|r| r["k"] == "say")
                .cloned(),
        );
    }
    assert!(runtime(&mut hub).state().firing.grenades.is_empty());
    kill_spawned(&mut hub);
    reach(&mut hub, TutorialStep::Retreat, &mut lines);
    execute(&mut hub, Command::Retreat { ids: vec![1] });
    reach(&mut hub, TutorialStep::Artillery, &mut lines);
    let defenders = runtime(&mut hub)
        .state()
        .tutorial_state()
        .unwrap()
        .spawned()
        .to_vec();
    let mut garrisoned = false;
    for _ in 0..400 {
        garrisoned = defenders.iter().all(|id| {
            runtime(&mut hub).state().world.game.units[id]
                .garrison
                .is_some()
        });
        if garrisoned {
            break;
        }
        let frames = tick_all(&mut hub);
        lines.extend(
            frames[0]["shots"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|r| r["k"] == "say")
                .cloned(),
        );
    }
    assert!(garrisoned);
    runtime(&mut hub).state_mut().world.game.reveal = true;
    execute(
        &mut hub,
        Command::Support {
            kind: "artillery".into(),
            at: Point { x: 127., z: 35. },
            direction: Some(0.),
        },
    );
    reach(&mut hub, TutorialStep::Bridgehead, &mut lines);
    // Deliver the artillery lifecycle rather than discarding the active strike.
    for id in defenders {
        if let Some(unit) = runtime(&mut hub).state_mut().world.game.units.get_mut(&id) {
            unit.hp = 0.;
        }
    }
    let at = runtime(&mut hub).state().start.points[1].at;
    go(&mut hub, at);
    for _ in 0..3000 {
        if runtime(&mut hub).state().finish.is_some() {
            break;
        }
        let frames = tick_all(&mut hub);
        lines.extend(
            frames[0]["shots"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|r| r["k"] == "say")
                .cloned(),
        );
    }
    let state = runtime(&mut hub).state();
    assert!(
        state.finish.is_some(),
        "Tutorial stuck: step={:?} units={:?} points={:?}",
        state.tutorial_state().unwrap().step(),
        state
            .world
            .game
            .units
            .values()
            .map(|u| (u.id, u.hp, u.x, u.z))
            .collect::<Vec<_>>(),
        state
            .start
            .points
            .iter()
            .map(|p| (p.owner, p.progress))
            .collect::<Vec<_>>()
    );
    assert_eq!(state.finish.unwrap().reason, EndReason::Tutorial);
    assert_eq!(lines.len(), 18);
    // The room itself must serialize its full Enemy result and release the runtime.
    ending_hold(&mut hub, std::slice::from_mut(&mut peer), &mut operatives);
    let status = hub.native_status("TUT").unwrap();
    assert!(status.ending_complete);
    assert!(hub.rooms["TUT"].native.is_none());
    let result = &hub.rooms["TUT"].result.as_ref().unwrap().report;
    assert_eq!(result["mode"], "tutorial");
    assert_eq!(result["names"], json!(["Human 1", "Enemy"]));
    assert_eq!(result["teams"], json!([0, 1]));
    assert!(
        result["timeline"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row.get("points").is_some() && row.get("structures").is_some())
    );
}

#[test]
fn assault_room_timer_victory_survives_every_seat_snapshot_and_result_delivery() {
    let mut hub = hub();
    let mut peers = [Peer::new(1), Peer::new(2)];
    for p in &peers {
        p.join(&mut hub, json!({"public":true}));
    }
    peers[0].send(&mut hub, json!({"t":"mode","v":"assault"}));
    let mut operatives = [Peer::new(10), Peer::new(11)];
    operatives[0].join_operative(&mut hub);
    peers[0].send(&mut hub, json!({"t":"start"}));
    operatives[1].join_operative(&mut hub);
    operative_starts(&mut hub, &mut operatives);
    for p in &mut peers {
        p.drain(&mut hub);
    }
    exercise_operatives(&mut hub, &operatives);
    runtime(&mut hub)
        .state_mut()
        .assault
        .as_mut()
        .unwrap()
        .time_left = 0.15;
    for _ in 0..6 {
        tick_all(&mut hub);
    }
    assert_eq!(
        runtime(&mut hub).state().finish.unwrap().reason,
        ww2_core::match_end::EndReason::Timer
    );
    ending_hold(&mut hub, &mut peers, &mut operatives);
    assert!(hub.native_status("TUT").unwrap().ending_complete);
    let result = &hub.rooms["TUT"].result.as_ref().unwrap().report;
    assert_eq!(result["mode"], "assault");
    assert_eq!(result["reason"], "timer");
    assert_eq!(result["winner"], 0);
    assert_eq!(result["names"], json!(["Human 1", "Human 2"]));
}

#[test]
fn attached_elimination_rooms_run_lobby_and_live_operatives_through_victory_and_hold() {
    use ww2_core::{economy::Mode, match_end::EndReason};
    for (mode, name, target, reason) in [
        (
            Mode::Annihilation,
            "annihilation",
            "bunker",
            EndReason::Bunkers,
        ),
        (Mode::Classic, "classic", "hq", EndReason::Headquarters),
    ] {
        let mut hub = hub();
        let mut peers = [Peer::new(1), Peer::new(2)];
        for peer in &peers {
            peer.join(&mut hub, json!({}));
        }
        peers[0].send(&mut hub, json!({"t":"mode","v":name}));
        let mut operatives = [Peer::new(10), Peer::new(11)];
        operatives[0].join_operative(&mut hub);
        let mut options = native_tests::native_match().options;
        options.mode = mode;
        options.tech = false;
        let map = crate::authoritative_maps::load("default").unwrap();
        let state = match mode {
            Mode::Annihilation => Match::new_annihilation(map, options),
            Mode::Classic => Match::new_classic(map, options),
            _ => unreachable!(),
        }
        .unwrap();
        // Exercise existing constructor attachment without changing public Start admission.
        hub.attach_native("TUT", state, 0).unwrap();
        operatives[1].join_operative(&mut hub);
        operative_starts(&mut hub, &mut operatives);
        for peer in &mut peers {
            peer.drain(&mut hub);
        }
        exercise_operatives(&mut hub, &operatives);
        let state = runtime(&mut hub).state_mut();
        let mut removed = 0;
        for unit in state.world.game.units.values_mut() {
            if unit.owner == 1 && state.rules.kinds[unit.kind].name == target {
                unit.hp = 0.;
                removed += 1;
            }
        }
        assert!(removed > 0);
        for _ in 0..200 {
            if runtime(&mut hub).state().finish.is_some() {
                break;
            }
            tick_all(&mut hub);
        }
        let finish = runtime(&mut hub).state().finish.unwrap();
        assert_eq!(finish.winner, 0);
        assert_eq!(finish.reason, reason);
        ending_hold(&mut hub, &mut peers, &mut operatives);
        let result = &hub.rooms["TUT"].result.as_ref().unwrap().report;
        assert_eq!(result["mode"], name);
        assert_eq!(result["winner"], 0);
        assert_eq!(result["reason"], reason.as_str());
    }
}
