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
    fn drain(&mut self, hub: &mut Hub) -> Vec<Value> {
        let mut rows = vec![];
        while let Ok(output) = self.rx.try_recv() {
            match output {
                Output::Json(row) => rows.push(row),
                Output::Snapshot(write) => {
                    rows.push(serde_json::from_str(&write.json).unwrap());
                    hub.complete_snapshot(&write, true, 0);
                }
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
    let selected = (0..count)
        .map(|slot| SelectedSeat {
            slot,
            full: false,
            omit: false,
        })
        .collect::<Vec<_>>();
    let frames = rt.collect_frame(&selected, FrameKind::Scheduled).unwrap();
    assert!(rt.fault().is_none());
    assert_eq!(frames.len(), count);
    frames.into_iter().map(|f| f.message).collect()
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
        .filter(|u| u.owner == 0 && u.hp > 0.)
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
    peer.join(&mut hub, json!({"tutorial":true}));
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
    let ids = runtime(&mut hub)
        .state()
        .world
        .game
        .units
        .keys()
        .map(|id| i64::from(*id))
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
    let blast = ww2_core::match_firing::AbilityBlast::from_unit(
        state.world.game.units[&1].kind,
        &state.rules,
    )
    .unwrap();
    let target = state.tutorial_state().unwrap().spawned()[0];
    let at = state.world.game.units[&target].at();
    state
        .launch_ability_flight(1, at, blast, Some(1.), None)
        .unwrap();
    assert!(!state.firing.grenades.is_empty());
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
    for callback in 0..120 {
        let now = (callback + 1) * 50;
        hub.poll_native(now, now * 1_000_000);
        peer.drain(&mut hub);
        assert_eq!(hub.native_status("TUT").unwrap().fault, None);
    }
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
    peers[0].send(&mut hub, json!({"t":"start"}));
    for p in &mut peers {
        p.drain(&mut hub);
    }
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
    for callback in 0..120 {
        let now = (callback + 1) * 50;
        hub.poll_native(now, now * 1_000_000);
        for p in &mut peers {
            p.drain(&mut hub);
        }
        assert_eq!(hub.native_status("TUT").unwrap().fault, None);
    }
    assert!(hub.native_status("TUT").unwrap().ending_complete);
    let result = &hub.rooms["TUT"].result.as_ref().unwrap().report;
    assert_eq!(result["mode"], "assault");
    assert_eq!(result["reason"], "timer");
    assert_eq!(result["winner"], 0);
    assert_eq!(result["names"], json!(["Human 1", "Human 2"]));
}
