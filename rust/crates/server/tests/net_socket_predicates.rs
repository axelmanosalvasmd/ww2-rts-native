//! Prospective NET-05/34/68 original-Source predicates. Recording and execution pending.
#[allow(dead_code)]
mod common;
use common::{BinaryServer, Client};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    io::Write,
    net::{Shutdown, SocketAddr},
};

const SEED: u64 = 3936341;

fn text(client: &mut Client, text: &str) {
    let bytes = text.as_bytes();
    let mask = [13_u8, 17, 19, 23];
    let mut frame = vec![0x81];
    if bytes.len() < 126 {
        frame.push(0x80 | bytes.len() as u8);
    } else {
        frame.push(0x80 | 126);
        frame.extend(u16::try_from(bytes.len()).unwrap().to_be_bytes());
    }
    frame.extend(mask);
    frame.extend(bytes.iter().enumerate().map(|(i, b)| b ^ mask[i % 4]));
    client.socket.write_all(&frame).unwrap();
}
fn units(text: &Value) -> Value {
    json!(text.as_str().unwrap().encode_utf16().collect::<Vec<_>>())
}
fn join(address: SocketAddr, code: &str, token: &str, name: &str) -> Client {
    Client::open(
        address,
        code,
        json!({"t":"hello","token":token,"name":name}),
    )
}
// The following play helper is byte-exact frozen 0f93; see the coverage document.
fn play(fixture: &Value) {
    let server = BinaryServer::start(true, SEED);
    let mut host = Client::join(server.address, "a");
    let mut guest = Client::join(server.address, "b");
    host.send(json!({"t":"start"}));
    let start = host.until(|m| m["t"] == "start");
    assert_eq!(
        guest.until(|m| m["t"] == "start")["matchId"],
        start["matchId"]
    );
    assert_eq!(start["you"], 0);
    let before = host.until(|m| m["t"] == "s")["tick"].as_u64().unwrap();
    assert!(host.view.units().next().is_some());
    for input in fixture["inputs"].as_array().unwrap() {
        text(&mut host, input.as_str().unwrap());
    }
    host.send(json!({"t":"ping","c":fixture["pong"]["c"]}));
    let pong = host.until(|m| m["t"] == "pong");
    assert_eq!(json!({"t":pong["t"],"c":pong["c"]}), fixture["pong"]);
    host.until(|m| m["t"] == "s" && m["tick"].as_u64().is_some_and(|t| t > before));
    assert_eq!(fixture["advanced"], true);
    assert!(host.view.units().next().is_some());
    assert_eq!(fixture["unitsPresent"], true);
    assert!(!server.log().contains("NATIVE_FAULT "));
    guest.socket.shutdown(Shutdown::Both).unwrap();
    drop(guest);
    host.until(|m| m["t"] == "pause" && m["paused"] == true && m["reason"] == "drop");
    let mut replacement = join(server.address, "ABC", "b", "b");
    let resumed = replacement.until(|m| m["t"] == "start");
    assert_eq!(resumed["matchId"], start["matchId"]);
    assert_eq!(resumed["you"], fixture["you"]);
    assert_eq!(fixture["sameMatchAfterReconnect"], true);
    replacement.until(|m| m["t"] == "pause" && m["paused"] == false);
    replacement.until(|m| m["t"] == "s");
    assert!(replacement.view.units().next().is_some());
    assert_eq!(server.receipts().len(), 1);
    for receipt in server.receipts() {
        assert_eq!(receipt["options"]["logistics"], false);
        assert_eq!(receipt["options"]["sandbox"], false);
        assert_eq!(receipt["options"]["seed"], SEED);
    }
    println!(
        "PASS NET-05 actual R1 binary: {} malformed in-play inputs, ordered Pong, later decoded snapshot and same-seat reconnect",
        fixture["inputs"].as_array().unwrap().len()
    );
}
fn roster(message: &Value) -> Value {
    Value::Array(
        message["players"]
            .as_array()
            .unwrap()
            .iter()
            .map(|player| {
                json!({
                    "nameUnits":units(&player["name"]),"team":player["team"],
                    "faction":player["faction"],"connected":player["connected"],
                    "ai":player["ai"],"level":player["level"]
                })
            })
            .collect(),
    )
}

fn malformed(fixture: &Value) {
    let server = BinaryServer::start(false, SEED);
    let mut client = Client::open(server.address, "Malformed", json!({}));
    for input in fixture["invalid"].as_array().unwrap() {
        text(&mut client, input.as_str().unwrap());
    }
    client.send(json!({"t":"hello","token":"malformed","name":"malformed"}));
    let first = client.receive();
    assert_eq!(
        first["t"], "lobby",
        "NET-05 prehello malformed text is silent"
    );
    assert_eq!(roster(&first), fixture["first"]);
    for input in fixture["invalid"].as_array().unwrap() {
        text(&mut client, input.as_str().unwrap());
    }
    client.send(json!({"t":"ping","c":fixture["pong"]["c"]}));
    assert_eq!(
        client.receive(),
        fixture["pong"],
        "NET-05 posthello malformed text is silent"
    );
}

fn receive_checked(client: &mut Client) -> Value {
    let message = client.receive();
    if message["t"] == "s" {
        assert!(
            !client.view.needs_resync(),
            "actual decoded held fingerprint"
        );
    }
    message
}

fn until_checked(client: &mut Client, predicate: impl Fn(&Value) -> bool) -> Value {
    for _ in 0..512 {
        let message = receive_checked(client);
        if predicate(&message) {
            return message;
        }
    }
    panic!("expected packet not received within 512 actual messages");
}

fn cleared(client: &Client) {
    assert_eq!(
        client.view.unit_count(),
        0,
        "start clears actual warmed rows"
    );
    assert!(client.view.state().is_null());
    assert!(!client.view.needs_resync());
}

fn full(client: &Client, snapshot: &Value, expected: &Value) {
    let rows = snapshot["units"].as_array().unwrap();
    assert!(!rows.is_empty());
    let ids = rows
        .iter()
        .map(|row| row[0].as_u64().unwrap())
        .collect::<BTreeSet<_>>();
    assert_eq!(ids.len(), rows.len());
    assert!(snapshot.get("gone").is_none());
    assert_eq!(
        snapshot["held"],
        json!([ids.len(), ids.iter().fold(0_u64, |xor, id| xor ^ *id)])
    );
    assert_eq!(client.view.unit_count(), rows.len());
    assert!(!client.view.needs_resync());
    assert_eq!(
        json!({"nonempty":true,"unique":true,"noGone":true,"completeHeld":true}),
        *expected,
        "actual full initial rows; Source does not require all:true"
    );
}

fn resets(fixture: &Value) {
    let server = BinaryServer::start(true, SEED);
    let mut host = Client::join(server.address, "a");
    let mut guest = Client::join(server.address, "b");
    host.send(json!({"t":"start"}));
    let start = until_checked(&mut host, |m| m["t"] == "start");
    cleared(&host);
    let initial = until_checked(&mut host, |m| m["t"] == "s");
    full(&host, &initial, &fixture["initial"]);
    let guest_start = until_checked(&mut guest, |m| m["t"] == "start");
    assert_eq!(guest_start["matchId"], start["matchId"]);
    cleared(&guest);
    let guest_first = until_checked(&mut guest, |m| m["t"] == "s");
    full(&guest, &guest_first, &fixture["guestInitial"]);
    host.send(json!({"t":"team","slot":0,"v":5}));
    host.send(json!({"t":"faction","slot":0,"v":3}));
    host.send(json!({"t":"ping","c":"settings-refused-in-play"}));
    until_checked(&mut host, |m| {
        m["t"] == "pong" && m["c"] == "settings-refused-in-play"
    });
    assert!(host.view.unit_count() > 0);
    let mut replacement = join(server.address, "ABC", "a", "Replacement");
    // This is actual prior decoder history, not injected expected rows.
    replacement.view = host.view.clone();
    until_checked(&mut host, |m| m["t"] == "replaced");
    assert_eq!(host.frame().0, 8, "actual replaced connection closes");
    let incoming = until_checked(&mut replacement, |m| m["t"] == "start");
    assert_eq!(incoming["matchId"], start["matchId"]);
    assert_eq!(incoming["you"], start["you"]);
    assert_eq!(
        incoming["teams"], start["teams"],
        "NET-34 team refused in play"
    );
    assert_eq!(
        incoming["factions"], start["factions"],
        "NET-34 faction refused in play"
    );
    cleared(&replacement);
    let first = until_checked(&mut replacement, |m| m["t"] == "s");
    full(&replacement, &first, &fixture["rejoined"]);
    guest.send(json!({"t":"ping","c":"after-replace"}));
    let mut saw_barrier = false;
    for _ in 0..512 {
        let message = receive_checked(&mut guest);
        assert!(
            message["t"] != "pause" || message["reason"] != "drop" || message["paused"] != true
        );
        if message["t"] == "pong" && message["c"] == "after-replace" {
            saw_barrier = true;
            break;
        }
    }
    assert!(
        saw_barrier,
        "replacement continuity barrier was actually received"
    );
    let history = guest.view.clone();
    assert!(history.unit_count() > 0);
    guest.socket.shutdown(Shutdown::Both).unwrap();
    drop(guest);
    until_checked(&mut replacement, |m| {
        m["t"] == "pause" && m["paused"] == true && m["reason"] == "drop"
    });
    let mut returned = join(server.address, "ABC", "b", "b");
    returned.view = history;
    let drop_start = until_checked(&mut returned, |m| m["t"] == "start");
    assert_eq!(drop_start["matchId"], start["matchId"]);
    assert_eq!(drop_start["you"], 1);
    cleared(&returned);
    until_checked(&mut returned, |m| m["t"] == "pause" && m["paused"] == false);
    let drop_full = until_checked(&mut returned, |m| m["t"] == "s");
    full(&returned, &drop_full, &fixture["dropFull"]);
    assert!(replacement.view.unit_count() > 0);
    replacement.send(json!({"t":"restart"}));
    let restart = until_checked(&mut replacement, |m| {
        m["t"] == "start" && m["matchId"] != start["matchId"]
    });
    assert_eq!(
        restart["matchId"].as_u64().unwrap(),
        start["matchId"].as_u64().unwrap() + 1
    );
    cleared(&replacement);
    let restarted = until_checked(&mut replacement, |m| m["t"] == "s");
    full(&replacement, &restarted, &fixture["restarted"]);
    let guest_restart = until_checked(&mut returned, |m| m["t"] == "start");
    assert_eq!(guest_restart["matchId"], restart["matchId"]);
    assert_eq!(guest_restart["you"], 1);
    cleared(&returned);
    let guest_restarted = until_checked(&mut returned, |m| m["t"] == "s");
    full(&returned, &guest_restarted, &fixture["guestRestarted"]);
    assert_eq!(server.receipts().len(), 2);
    assert!(!server.log().contains("NATIVE_FAULT "));
}

fn slots(fixture: &Value) {
    let server = BinaryServer::start(false, SEED);
    for case in fixture.as_array().unwrap() {
        let code = case["code"].as_str().unwrap();
        let mut host = join(server.address, code, "Host", "Host");
        assert_eq!(host.receive()["t"], "lobby");
        let mut guest = if case["guest"] == true {
            let mut client = join(server.address, code, "Guest", "Guest");
            client.until(|m| m["t"] == "lobby");
            host.until(|m| m["t"] == "lobby" && m["players"].as_array().unwrap().len() == 2);
            Some(client)
        } else {
            None
        };
        host.send(json!({"t":"addAi"}));
        let initial = host.until(|m| {
            m["t"] == "lobby"
                && m["players"].as_array().unwrap().len()
                    == case["initial"].as_array().unwrap().len()
        });
        assert_eq!(roster(&initial), case["initial"]);
        if let Some(client) = guest.as_mut() {
            assert_eq!(
                roster(&client.until(|m| m["t"] == "lobby")),
                case["initial"]
            );
        }
        for step in case["steps"].as_array().unwrap() {
            let is_guest = step["actor"] == "guest";
            if is_guest {
                guest.as_mut().unwrap().send(step["message"].clone());
            } else {
                host.send(step["message"].clone());
            }
            if step["broadcast"] == true {
                host.until(|m| m["t"] == "lobby");
                if let Some(client) = guest.as_mut() {
                    client.until(|m| m["t"] == "lobby");
                }
            }
            let actor = if is_guest {
                guest.as_mut().unwrap()
            } else {
                &mut host
            };
            actor.send(json!({"t":"ping","c":step["barrier"]["c"]}));
            assert_eq!(
                actor.receive(),
                step["barrier"],
                "NET-34 named slot guard is silent"
            );
            host.send(step["probe"].clone());
            let actual = host.until(|m| m["t"] == "lobby");
            assert_eq!(
                roster(&actual),
                step["roster"],
                "NET-34 complete fresh roster after action and no-op probe"
            );
            if let Some(client) = guest.as_mut() {
                assert_eq!(roster(&client.until(|m| m["t"] == "lobby")), step["roster"]);
            }
        }
    }
}

#[test]
fn source_recorded_socket_predicates_use_actual_binary_tcp() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/net-socket-predicates.json");
    let bytes = std::fs::read(path).expect(
        "record original Source predicates under a central-ledger grant before native execution",
    );
    let fixture: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        fixture["reference"],
        "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
    );
    assert_eq!(fixture["recorded"], true);
    assert_eq!(fixture["provenance"]["node"], "v24.21.0");
    assert_eq!(fixture["provenance"]["arch"], "x64");
    assert_eq!(fixture["rows"], json!(["NET-05", "NET-34", "NET-68"]));
    play(&fixture["play"]);
    malformed(&fixture["malformed"]);
    resets(&fixture["replacement"]);
    slots(&fixture["slots"]);
}
