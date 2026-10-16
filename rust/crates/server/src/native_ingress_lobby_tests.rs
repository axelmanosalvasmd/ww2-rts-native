//! Stable-connection admission under an explicit host clock.
use super::*;
use ww2_core::orders::OrderKey;

#[test]
fn replaced_connection_cannot_reserve_input_and_denials_stay_with_the_person() {
    let client = |id| {
        let (tx, rx) = mpsc::channel(64);
        let (cancel, _) = watch::channel(false);
        (Connection::new(id, tx, cancel), rx)
    };
    let (old, mut old_rx) = client(1);
    let (peer, mut peer_rx) = client(2);
    let (current, mut current_rx) = client(3);
    let mut hub = Hub::new(32, String::new());
    for (connection, token) in [(&old, "a"), (&peer, "b")] {
        hub.message(
            "ABC",
            connection,
            &mut false,
            json!({"t":"hello","token":token,"name":token}),
            0,
        );
    }
    hub.attach_native("ABC", native_tests::native_match(), 0)
        .unwrap();
    while old_rx.try_recv().is_ok() {}
    while peer_rx.try_recv().is_ok() {}
    hub.message(
        "ABC",
        &current,
        &mut false,
        json!({"t":"hello","token":"a","name":"a"}),
        1,
    );
    while current_rx.try_recv().is_ok() {}
    let before = format!("{:?}", hub.rooms["ABC"].native.as_ref().unwrap().ingress);
    hub.message("ABC", &old, &mut true, json!({"t":"stop","ids":[1]}), 2);
    hub.disconnected("ABC", old.id, 3);
    let room = &hub.rooms["ABC"];
    assert_eq!(room.players[0].connection.as_ref().unwrap().id, current.id);
    assert_eq!(
        format!("{:?}", room.native.as_ref().unwrap().ingress),
        before
    );
    assert_eq!(
        room.native
            .as_ref()
            .unwrap()
            .runtime
            .state()
            .commands
            .pending_commands(),
        0
    );
    hub.message(
        "ABC",
        &current,
        &mut true,
        json!({"t":"stop","ids":[1],"slot":1,"issuer":1,"tick":999,"sequence":99}),
        4,
    );
    let state = hub.rooms["ABC"].native.as_ref().unwrap().runtime.state();
    assert_eq!(
        state
            .commands
            .pending_tick(1)
            .map(|(key, _)| *key)
            .collect::<Vec<_>>(),
        vec![OrderKey {
            tick: 1,
            issuer: 0,
            sequence: 1
        }]
    );
    // Parser refusals share the Source four-per-second window across takeover.
    for _ in 0..6 {
        hub.message(
            "ABC",
            &current,
            &mut true,
            json!({"t":"move","orders":[[1,2,"bad"]]}),
            5,
        );
    }
    let mut denials = Vec::new();
    while let Ok(Output::Json(value)) = current_rx.try_recv() {
        denials.push(value);
    }
    assert_eq!(denials.len(), 4);
    assert!(
        denials
            .iter()
            .all(|m| *m == json!({"t":"deny","cmd":"move","reason":"blocked"}))
    );
    let (replacement, mut replacement_rx) = client(4);
    hub.message(
        "ABC",
        &replacement,
        &mut false,
        json!({"t":"hello","token":"a","name":"a"}),
        6,
    );
    while replacement_rx.try_recv().is_ok() {}
    hub.message(
        "ABC",
        &replacement,
        &mut true,
        json!({"t":"move","orders":[[1,2,"bad"]]}),
        7,
    );
    assert!(replacement_rx.try_recv().is_err());
    hub.message(
        "ABC",
        &replacement,
        &mut true,
        json!({"t":"move","orders":[[1,2,"bad"]]}),
        1006,
    );
    assert!(
        matches!(replacement_rx.try_recv(),Ok(Output::Json(m)) if m["t"]=="deny" && m["reason"]=="blocked")
    );
    assert_eq!(
        hub.rooms["ABC"]
            .native
            .as_ref()
            .unwrap()
            .runtime
            .state()
            .commands
            .pending_commands(),
        1
    );
}
