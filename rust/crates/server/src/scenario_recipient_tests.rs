//! Ordinary room lifecycle proof for MODE-86. Expected wire rows are literal prototype shapes.
use super::*;
use serde_json::json;
use ww2_core::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    map::AuthoritativeMap,
    match_runtime::{MatchOptions, PlayerConfig},
    weather::WeatherChoice,
};

fn mission() -> (Match, Value) {
    let raw = json!({"name":"Objective recipients","w":64,"h":64,"rows":vec![".".repeat(64);64],
        "spawns":[{"x":4,"y":4},{"x":58,"y":58},{"x":4,"y":58}],
        "points":[{"id":"front","x":32,"y":32}],
        "scenario":{"version":1,"groups":[],"areas":[],"objectives":[
            {"id":"team","text":{"en":"Equipo","es":"Equipo"},"recipients":"team","side":2},
            {"id":"side","text":"Side","recipients":"side","side":0},
            {"id":"public","text":{"en":"Public","es":"Publico"}}],
            "triggers":[
                {"id":"intro","scope":"match","side":0,"condition":{"kind":"time","seconds":0.05},"actions":[
                    {"kind":"objectiveActivate","objective":"public"},
                    {"kind":"objectiveActivate","objective":"side"},
                    {"kind":"objectiveActivate","objective":"team"},
                    {"kind":"say","text":{"en":"Everyone","es":"Todos"}},
                    {"kind":"say","text":"One seat","recipients":"side","side":0},
                    {"kind":"say","text":{"en":"Allies","es":"Allies"},"recipients":"team","side":2}]},
                {"id":"done","scope":"match","condition":{"kind":"time","seconds":0.15},"actions":[
                    {"kind":"objectiveComplete","objective":"public"},
                    {"kind":"objectiveComplete","objective":"side"},
                    {"kind":"objectiveComplete","objective":"team"},
                    {"kind":"objectiveActivate","objective":"public"}]},
                {"id":"after","scope":"match","condition":{"kind":"time","seconds":0.5},"actions":[
                    {"kind":"say","text":"After victory"}]}]}});
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&raw).unwrap()).unwrap();
    let state = Match::new(
        &map,
        MatchOptions {
            seed: 86,
            mode: Mode::Conquest,
            players: [0, 1, 0]
                .into_iter()
                .enumerate()
                .map(|(slot, team)| PlayerConfig {
                    name: format!("p{slot}"),
                    team,
                    faction: Faction::Ussr,
                })
                .collect(),
            shuffle: false,
            army: ArmySize::Standard,
            weather: WeatherChoice::Map,
            weather_seed: Some(1),
            living_weather: false,
            map_key: Some("mission".into()),
            tech: false,
            supply: false,
            logistics: false,
            sandbox: false,
        },
    )
    .unwrap();
    (state, raw)
}
fn seats() -> Vec<SelectedSeat> {
    (0..3)
        .map(|slot| SelectedSeat {
            slot,
            full: true,
            omit: false,
        })
        .collect()
}
fn read(runtime: &mut RoomRuntime, kind: FrameKind) -> Vec<Value> {
    runtime
        .collect_frame(&seats(), kind)
        .unwrap()
        .into_iter()
        .map(|s| s.message)
        .collect()
}
fn objective(id: &str, state: &str, execution: &str, updated: f64) -> Value {
    let (text, recipients, side) = match id {
        "public" => (json!({"en":"Public","es":"Publico"}), "public", 0),
        "side" => (json!("Side"), "side", 0),
        "team" => (json!({"en":"Equipo","es":"Equipo"}), "team", 2),
        _ => unreachable!(),
    };
    json!({"id":id,"scope":"match","text":text,"state":state,"recipients":recipients,
        "side":side,"updated":updated,"execution":execution})
}
fn say(text: &str, localized: Value, recipients: &str, side: usize) -> Value {
    json!({"k":"say","text":text,"localized":localized,"scenarioRecipients":recipients,
        "scenarioSide":side,"pub":recipients=="public","execution":"scenario:1","atTick":1})
}

#[test]
fn mode86_ordinary_room_filters_scenario_reads_without_execution_and_keeps_normal_victory() {
    let (state, raw) = mission();
    // Loaded ordinary rooms use the real authoring/start binding and strip private mission data.
    let startup = crate::native_startup::NativeStartup::prepare_loaded(
        &state,
        crate::native_startup::StartupSettings {
            map: "mission",
            mode: "conquest",
            army: "standard",
            weather: "map",
            sandbox: false,
        },
        &raw,
    )
    .unwrap();
    let mut runtime = RoomRuntime::new(state, 0).unwrap();
    let initial = read(&mut runtime, FrameKind::Current);
    for (slot, message) in initial.iter().enumerate() {
        assert_eq!(message["scenario"], json!({"version":1,"objectives":[]}));
        let start = startup
            .envelope(runtime.state(), &["p0".into(), "p1".into(), "p2".into()], 1)
            .unwrap()
            .message(slot, message.clone());
        assert!(start["map"].get("scenario").is_none());
    }
    runtime.advance_tick(&[false; 3]).unwrap();
    // Pending private and public announcements survive the checkpoint codec before delivery.
    assert_eq!(crate::saves::version_for(runtime.state()), 2);
    let saved = runtime.state().save_json().unwrap();
    let pending = Match::save_events_json(&runtime.pending_events).unwrap();
    let mut restored = Match::from_save_json(saved).unwrap();
    let events = Match::events_from_save_json(pending).unwrap();
    let mut delivery = DeliveryState::new(&restored).unwrap();
    let frame = delivery
        .capture_selected_with_tick_events(&mut restored, &seats(), &events)
        .unwrap()
        .unwrap();
    let checkpoint_rows = (0..3)
        .map(|slot| frame.recipient(slot).unwrap().into_json())
        .collect::<Vec<_>>();
    drop(frame);
    let first = read(&mut runtime, FrameKind::Current);
    let public = say(
        "Everyone",
        json!({"en":"Everyone","es":"Todos"}),
        "public",
        0,
    );
    for slot in 0..3 {
        let mut shots = vec![public.clone()];
        let mut objectives = vec![objective("public", "active", "scenario:1", 0.05)];
        if slot == 0 {
            shots.push(say("One seat", json!("One seat"), "side", 0));
            objectives.push(objective("side", "active", "scenario:1", 0.05));
        }
        if slot != 1 {
            shots.push(say(
                "Allies",
                json!({"en":"Allies","es":"Allies"}),
                "team",
                2,
            ));
            objectives.push(objective("team", "active", "scenario:1", 0.05));
        }
        assert_eq!(first[slot]["shots"], json!([]));
        assert_eq!(checkpoint_rows[slot]["shots"], json!(shots));
        assert_eq!(
            first[slot]["scenario"],
            json!({"version":1,"objectives":objectives})
        );
        assert_eq!(checkpoint_rows[slot]["scenario"], first[slot]["scenario"]);
    }
    // Reveal cannot bypass a scenario's private recipient policy.
    runtime.state_mut().world.game.reveal = true;
    let before = runtime.state().authored_scenario.clone();
    let pending_before = Match::save_events_json(&runtime.pending_events).unwrap();
    for _ in 0..3 {
        let reconnect = read(&mut runtime, FrameKind::Current);
        for slot in 0..3 {
            assert_eq!(reconnect[slot]["scenario"], first[slot]["scenario"]);
            assert_eq!(reconnect[slot]["shots"], first[slot]["shots"]);
        }
        assert_eq!(runtime.state().authored_scenario, before);
        assert_eq!(
            Match::save_events_json(&runtime.pending_events).unwrap(),
            pending_before
        );
        assert_eq!(runtime.state().world.game.tick, 1);
    }
    let scheduled = read(&mut runtime, FrameKind::Scheduled);
    for slot in 0..3 {
        assert_eq!(scheduled[slot]["shots"], checkpoint_rows[slot]["shots"]);
        assert_eq!(scheduled[slot]["scenario"], first[slot]["scenario"]);
    }
    for tick in 2..=8 {
        runtime.advance_tick(&[false; 3]).unwrap();
        let messages = read(&mut runtime, FrameKind::Scheduled);
        assert!(
            runtime.state().finish.is_none(),
            "objectives must never decide victory"
        );
        assert!(runtime.fault().is_none());
        if tick >= 3 {
            for (slot, message) in messages.iter().enumerate() {
                let mut objectives =
                    vec![objective("public", "complete", "scenario:2", 3.0 * 0.05)];
                if slot == 0 {
                    objectives.push(objective("side", "complete", "scenario:2", 3.0 * 0.05));
                }
                if slot != 1 {
                    objectives.push(objective("team", "complete", "scenario:2", 3.0 * 0.05));
                }
                assert_eq!(
                    message["scenario"],
                    json!({"version":1,"objectives":objectives})
                );
                assert_eq!(message["shots"], json!([]));
            }
        }
    }
    let completed = runtime.state().authored_scenario.clone();
    let mut restored = Match::from_save_json(runtime.state().save_json().unwrap()).unwrap();
    assert_eq!(restored.authored_scenario, completed);
    // A normal opposing-team VP win is still authoritative after every objective completes.
    runtime.state_mut().start.economy.players[1].vp = runtime.state().start.win_vp;
    restored.start.economy.players[1].vp = restored.start.win_vp;
    runtime.advance_tick(&[false; 3]).unwrap();
    restored.advance_tick().unwrap();
    assert_eq!(restored.finish, runtime.state().finish);
    let finish = runtime.state().finish.unwrap();
    assert_eq!(finish.winner, 1);
    assert_eq!(finish.reason, ww2_core::match_end::EndReason::VictoryPoints);
    read(&mut runtime, FrameKind::Scheduled);
    let completed_at_victory = runtime.state().authored_scenario.clone();
    for _ in 0..120 {
        let ending = runtime.hold_callback().unwrap().unwrap();
        if ending.advance {
            runtime.advance_tick(&[false; 3]).unwrap();
            for message in read(&mut runtime, FrameKind::Scheduled) {
                assert_eq!(message["shots"], json!([]));
            }
        }
        assert!(runtime.fault().is_none());
        assert_eq!(runtime.state().authored_scenario, completed_at_victory);
    }
    assert_eq!(runtime.held(), 120);
}
