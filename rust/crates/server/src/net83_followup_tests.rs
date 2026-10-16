use super::*;
use ww2_core::{match_end::EndReason, match_story::StoryKey};

fn connection(id: u64) -> (Connection, mpsc::Receiver<Output>) {
    let (tx, rx) = mpsc::channel(64);
    let (cancel, _) = watch::channel(false);
    (Connection { id, tx, cancel }, rx)
}

fn drain(hub: &mut Hub, rx: &mut mpsc::Receiver<Output>, now: u64) -> Vec<Value> {
    let mut messages = Vec::new();
    while let Ok(output) = rx.try_recv() {
        match output {
            Output::Json(message) => messages.push(message),
            Output::Snapshot(write) => {
                hub.complete_snapshot(&write, true, now);
            }
            other => panic!("unexpected output {other:?}"),
        }
    }
    messages
}

#[test]
fn net83_non_final_offline_seat_moves_survivor_result_rows_before_new_seat() {
    // Prototype e4e579a server.js:533-566 keeps survivors first, then removed
    // report rows, and inserts empty result rows where a new seat is added.
    let mut hub = Hub::new(32, String::new());
    let (offline, mut offline_rx) = connection(1);
    let (survivor, mut survivor_rx) = connection(2);
    for (client, name) in [(&offline, "Offline"), (&survivor, "Survivor")] {
        hub.message(
            "ABC",
            client,
            &mut false,
            json!({"t":"hello","token":name,"name":name}),
            0,
        );
    }
    let mut state = native_tests::native_match();
    state
        .record_story_tally(Some(0), StoryKey::Kills, 7.0)
        .unwrap();
    state
        .record_story_tally(Some(1), StoryKey::Losses, 9.0)
        .unwrap();
    state
        .initialize_tick_scene_finish(1, EndReason::Headquarters, None)
        .unwrap();
    hub.attach_native("ABC", state, 0).unwrap();
    drain(&mut hub, &mut offline_rx, 0);
    drain(&mut hub, &mut survivor_rx, 0);

    hub.disconnected("ABC", offline.id, 0);
    hub.rooms
        .get_mut("ABC")
        .unwrap()
        .native
        .as_mut()
        .unwrap()
        .runtime
        .resume(0);
    drain(&mut hub, &mut survivor_rx, 0);
    let mut finished = Vec::new();
    for step in 0..120 {
        hub.poll_native(step * 50, step * 50_000_000);
        finished.extend(
            drain(&mut hub, &mut survivor_rx, step * 50)
                .into_iter()
                .filter(|message| message["t"] == "lobby" && message["state"] == "lobby"),
        );
        if step < 119 {
            assert_eq!(hub.rooms["ABC"].players.len(), 2);
            assert!(finished.is_empty());
        }
    }
    assert!(hub.rooms["ABC"].native.is_none());
    assert_eq!(hub.native_status("ABC").unwrap().held, 120);
    assert_eq!(hub.rooms["ABC"].players.len(), 1);
    assert_eq!(hub.rooms["ABC"].players[0].name, "Survivor");
    assert_eq!(finished.len(), 1);
    let lobby = &finished[0];
    assert_eq!(lobby["you"], 0);
    assert_eq!(lobby["host"], 0);
    assert_eq!(lobby["players"][0]["name"], "Survivor");
    let result = &lobby["result"];
    assert_eq!(result["names"], json!(["Survivor", "Offline"]));
    assert_eq!(result["teams"], json!([1, 0]));
    assert_eq!(result["story"].as_array().unwrap().len(), 2);
    assert_eq!(result["story"][0]["kills"].as_f64(), Some(0.0));
    assert_eq!(result["story"][0]["losses"].as_f64(), Some(9.0));
    assert_eq!(result["story"][1]["kills"].as_f64(), Some(7.0));
    assert_eq!(result["story"][1]["losses"].as_f64(), Some(0.0));
    let outcome = json!({"team":1,"outcome":"victory"});
    assert_eq!(result["you"], outcome);
    assert_eq!(
        hub.rooms["ABC"].result.as_ref().unwrap().report["names"],
        result["names"]
    );

    hub.message("ABC", &survivor, &mut true, json!({"t":"addAi"}), 6000);
    let messages = drain(&mut hub, &mut survivor_rx, 6000);
    let lobby = messages
        .iter()
        .find(|message| message["t"] == "lobby")
        .unwrap();
    assert_eq!(lobby["players"].as_array().unwrap().len(), 2);
    assert_eq!(lobby["players"][0]["name"], "Survivor");
    assert_eq!(lobby["players"][1]["ai"], true);
    assert_eq!(lobby["result"]["names"], json!(["Survivor", "", "Offline"]));
    assert_eq!(lobby["result"]["teams"], json!([1, null, 0]));
    assert_eq!(
        lobby["result"]["story"],
        json!([result["story"][0], null, result["story"][1]])
    );
    assert_eq!(lobby["result"]["you"], outcome);
}
