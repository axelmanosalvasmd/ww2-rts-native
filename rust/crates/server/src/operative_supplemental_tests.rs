use super::*;

fn roster(message: &Value) -> Value {
    assert_eq!(message["t"], "lobby");
    json!({
        "players": message["players"],
        "spectators": message["spectators"],
        "operatives": message["operatives"]
    })
}

#[test]
fn net25_lobby_spectator_takeover_preserves_identity_and_retires_old_socket() {
    // Prototype server.js:596-603,620,719. Token identity precedes requested role.
    let (mut hub, _, mut commanders) = room();
    let (old, mut old_rx) = client(3);
    assert!(hello(
        &mut hub,
        &old,
        json!({"t":"hello","token":"watch","name":"Watcher","spectate":true}),
        0
    ));
    let person = &mut hub.rooms.get_mut("ABC").unwrap().spectators[0];
    person.denies = vec![1, 2, 3];
    person.rtt = Some(17.);
    messages(&mut old_rx);
    drain_commanders(&mut hub, &mut commanders, 0);
    let (current, mut current_rx) = client(4);
    assert!(hello(
        &mut hub,
        &current,
        json!({"t":"hello","token":"watch","name":" <Returned> ","role":"operative","commander":0}),
        1
    ));
    assert!(
        matches!(old_rx.try_recv(), Ok(Output::Json(value)) if value == json!({"t":"replaced"}))
    );
    assert!(matches!(old_rx.try_recv(), Ok(Output::Close(1000, ""))));
    assert!(old_rx.try_recv().is_err());
    let output = messages(&mut current_rx);
    assert_eq!(output.len(), 1);
    assert_eq!(output[0]["spectator"], true);
    assert_eq!(output[0]["you"], -1);
    assert_eq!(output[0]["spectators"], json!(["Returned"]));
    drain_commanders(&mut hub, &mut commanders, 1);
    let before = roster(&output[0]);
    for message in [
        json!({"t":"name","name":"Stale"}),
        json!({"t":"sit"}),
        json!({"t":"move","orders":[[1,2,3]]}),
    ] {
        command(&mut hub, &old, message, 2);
    }
    hub.disconnected("ABC", old.id, 3);
    assert_eq!(roster(&lobby(&hub, current.id)), before);
    assert!(messages(&mut current_rx).is_empty());
    assert!(old_rx.try_recv().is_err());
    assert!(
        drain_commanders(&mut hub, &mut commanders, 3)
            .iter()
            .all(Vec::is_empty)
    );
    let room = &hub.rooms["ABC"];
    assert_eq!(room.players.len(), 2);
    assert!(room.operatives.is_empty());
    assert_eq!(room.spectators.len(), 1);
    let person = &room.spectators[0];
    assert_eq!(person.name, "Returned");
    assert_eq!(person.denies, [1, 2, 3]);
    assert_eq!(person.rtt, Some(17.));
    assert_eq!(person.connection.as_ref().unwrap().id, current.id);
    assert_eq!(person.disconnected, None);
}

#[test]
fn net29_all_room_roles_clean_names_and_broadcast_in_arrival_order() {
    // Prototype server.js:194,338-350,635,638. Each valid rename broadcasts once.
    let (mut hub, seats, mut commanders) = room();
    let (watcher, mut watcher_rx) = client(3);
    assert!(hello(
        &mut hub,
        &watcher,
        json!({"t":"hello","token":"watch","name":"Watcher","spectate":true}),
        0
    ));
    let (operative, mut operative_rx) = client(4);
    join_operative(&mut hub, &operative, "friend", 0);
    messages(&mut watcher_rx);
    messages(&mut operative_rx);
    drain_commanders(&mut hub, &mut commanders, 0);
    let mut expected = roster(&lobby(&hub, seats[0].id));
    let mut revisions = Vec::new();
    for (connection, path) in [
        (&seats[0], "/players/0/name"),
        (&seats[1], "/players/1/name"),
        (&watcher, "/spectators/0"),
        (&operative, "/operatives/0/name"),
    ] {
        for (name, cleaned) in [
            (" <>&\"' ", "Soldier"),
            (" abcdefghijklmnopqrstuvwxyz ", "abcdefghijklmnop"),
            (" <Final> ", "Final"),
        ] {
            command(&mut hub, connection, json!({"t":"name","name":name}), 1);
            *expected.pointer_mut(path).unwrap() = json!(cleaned);
            revisions.push(expected.clone());
        }
        for bad in [Value::Null, json!(false), json!(123), json!([]), json!({})] {
            command(&mut hub, connection, json!({"t":"name","name":bad}), 2);
        }
    }
    let [first, second] = drain_commanders(&mut hub, &mut commanders, 2);
    let watched = messages(&mut watcher_rx);
    let operated = messages(&mut operative_rx);
    for output in [&first, &second, &watched, &operated] {
        assert_eq!(output.len(), 12);
        assert_eq!(output.iter().map(roster).collect::<Vec<_>>(), revisions);
    }
    for message in &watched {
        assert_eq!(message["spectator"], true);
        assert_eq!(message["you"], -1);
    }
    for message in &operated {
        assert_eq!(message["role"], "operative");
        assert_eq!(message["spectator"], false);
        assert_eq!(message["commander"], 0);
    }
}

#[test]
fn net82_lobby_spectator_close_is_immediate_while_seat_cleanup_waits_ten_seconds() {
    // Prototype server.js:568-575,718-733. Spectators hold no reconnect reservation.
    let (mut hub, seats, mut commanders) = room();
    let (watcher, mut watcher_rx) = client(3);
    assert!(hello(
        &mut hub,
        &watcher,
        json!({"t":"hello","token":"watch","name":"Watcher","spectate":true}),
        0
    ));
    messages(&mut watcher_rx);
    drain_commanders(&mut hub, &mut commanders, 0);
    hub.disconnected("ABC", watcher.id, 100);
    assert!(hub.rooms["ABC"].spectators.is_empty());
    for output in drain_commanders(&mut hub, &mut commanders, 100) {
        assert_eq!(output.len(), 1);
        assert_eq!(output[0]["spectators"], json!([]));
        assert_eq!(output[0]["players"][0]["connected"], true);
        assert_eq!(output[0]["players"][1]["connected"], true);
    }
    let (returned, mut returned_rx) = client(4);
    assert!(hello(
        &mut hub,
        &returned,
        json!({"t":"hello","token":"watch","name":"Back"}),
        101
    ));
    let output = messages(&mut returned_rx);
    assert_eq!(output.len(), 1);
    assert_eq!(output[0]["you"], 2);
    assert_eq!(output[0]["spectator"], false);
    assert_eq!(output[0]["players"][2]["name"], "Back");
    drain_commanders(&mut hub, &mut commanders, 101);
    hub.disconnected("ABC", seats[1].id, 200);
    let output = drain_commanders(&mut hub, &mut commanders, 200);
    assert_eq!(output[0].len(), 1);
    assert_eq!(
        output[0][0]["players"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| (p["name"].clone(), p["connected"].clone()))
            .collect::<Vec<_>>(),
        vec![
            (json!("a"), json!(true)),
            (json!("b"), json!(false)),
            (json!("Back"), json!(true))
        ]
    );
    messages(&mut returned_rx);
    hub.expire(10_199);
    assert_eq!(hub.rooms["ABC"].players.len(), 3);
    assert!(messages(&mut commanders[0]).is_empty());
    assert!(messages(&mut returned_rx).is_empty());
    hub.expire(10_200);
    let output = messages(&mut commanders[0]);
    let returned_output = messages(&mut returned_rx);
    assert_eq!(output.len(), 1);
    assert_eq!(returned_output.len(), 1);
    assert_eq!(roster(&output[0]), roster(&returned_output[0]));
    assert_eq!(returned_output[0]["you"], 1);
    assert_eq!(output[0]["spectators"], json!([]));
    assert_eq!(
        output[0]["players"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| (p["name"].clone(), p["connected"].clone()))
            .collect::<Vec<_>>(),
        vec![(json!("a"), json!(true)), (json!("Back"), json!(true))]
    );
}
