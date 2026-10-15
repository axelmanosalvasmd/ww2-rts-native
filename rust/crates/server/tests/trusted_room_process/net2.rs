//! Ordinary executable witness, separate from explicit-clock Hub tests.
use super::*;
use ww2_core::client_view::ClientView;

fn replay_held(client: &Client) {
    let mut view = ClientView::new();
    for message in &client.messages {
        view.apply(&message.to_string()).unwrap();
        if message["t"] == "s" {
            assert!(message.get("fog").is_none_or(Value::is_string));
            let ids = view.units().map(|unit| unit.id).collect::<Vec<_>>();
            assert_eq!(message["held"][0].as_u64(), Some(ids.len() as u64));
            assert_eq!(
                message["held"][1].as_i64(),
                Some(i64::from(ids.iter().fold(0_i32, |xor, id| xor ^ *id as i32)))
            );
            assert!(!view.needs_resync());
        }
    }
}

#[test]
fn ordinary_r1_socket_metadata_resync_and_drop_pause_use_live_receivers() {
    let server = BinaryServer::start(true, SEED);
    let mut host = Client::join(server.address, "a");
    let mut guest = Client::join(server.address, "b");
    let (start, _) = begin(&mut host, &mut guest);
    let identity = start["matchId"].as_u64().unwrap();
    for client in [&mut host, &mut guest] {
        let frame = client.until(|m| m["t"] == "s");
        assert_eq!(frame["online"], json!([true, true]));
        assert_eq!(frame["ping"], json!([null, null]));
    }
    host.send(json!({"t":"ping","c":"net2-a","rtt":10000}));
    assert_eq!(host.until(|m| m["t"] == "pong")["c"], "net2-a");
    guest.send(json!({"t":"ping","c":"net2-b","rtt":-10}));
    assert_eq!(guest.until(|m| m["t"] == "pong")["c"], "net2-b");
    for client in [&mut host, &mut guest] {
        let frame = client.until(|m| m["t"] == "s" && m["ping"] == json!([9999.0, 0.0]));
        assert_eq!(frame["online"], json!([true, true]));
    }
    host.send(json!({"t":"resync"}));
    let full = host.until(|m| m["t"] == "s" && m["all"] == true);
    let tick = full["tick"].as_i64().unwrap();
    let peer = guest.until(|m| m["t"] == "s" && m["tick"].as_i64().is_some_and(|t| t >= tick));
    assert!(peer.get("all").is_none(), "resync is socket-local");
    replay_held(&guest);
    guest.socket.shutdown(Shutdown::Both).unwrap();
    drop(guest);
    let pause = host.until(|m| m["t"] == "pause" && m["paused"] == true);
    assert_eq!(pause["reason"], "drop");
    assert_eq!(pause["by"], "b");
    let paused = host.until(|m| m["t"] == "s" && m["online"] == json!([true, false]));
    host.until(|m| m["t"] == "pause" && m["paused"] == true);
    let repeated = host.until(|m| m["t"] == "s");
    assert_eq!(repeated["tick"], paused["tick"], "periodic paused frames do not step the match");
    assert_eq!(repeated["online"], json!([true, false]));
    assert_eq!(repeated["ping"], json!([9999.0, 0.0]));

    let mut returned = Client::open(
        server.address,
        "ABC",
        json!({"t":"hello","name":"b","token":"b","proto":0}),
    );
    let resumed_start = returned.until(|m| m["t"] == "start");
    assert_eq!(resumed_start["matchId"], identity);
    assert_eq!(resumed_start["you"], 1);
    let still_paused = returned.receive();
    assert_eq!(still_paused["t"], "pause");
    assert_eq!(still_paused["paused"], true);
    let resume = returned.receive();
    assert_eq!(resume["t"], "pause");
    assert_eq!(resume["paused"], false);
    assert_eq!(returned.receive()["t"], "lobby");
    host.until(|m| m["t"] == "pause" && m["paused"] == false);
    let restored = returned.until(|m| m["t"] == "s" && m["online"] == json!([true, true]));
    assert_eq!(restored["ping"], json!([9999.0, 0.0]));
    assert!(restored["tick"].as_i64().unwrap() > paused["tick"].as_i64().unwrap());
    replay_held(&host);
    replay_held(&returned);
    assert_eq!(server.receipts().len(), 1);
    assert_eq!(server.directory(), json!({"rooms":[]}));
}
