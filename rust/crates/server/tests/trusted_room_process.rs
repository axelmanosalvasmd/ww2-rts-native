mod common;
use common::{BinaryServer, Client};
use serde_json::{Value, json};
use std::{net::Shutdown, process::Command};

const SEED: u64 = 3936341;

fn begin(host: &mut Client, guest: &mut Client) -> (Value, Value) {
    guest.send(json!({"t":"start","slot":0,"host":true}));
    assert_eq!(guest.deny("start")["reason"], "scenario");
    assert!(guest.barrier().iter().all(|m| m["t"] != "start"));
    let offset = host.messages.len();
    host.send(json!({"t":"start"}));
    let a = host.until(|m| m["t"] == "start");
    let b = guest.until(|m| m["t"] == "start");
    assert_eq!(a["matchId"], b["matchId"]);
    assert_eq!(a["you"], 0);
    assert_eq!(b["you"], 1);
    let emitted = host.messages[offset..]
        .iter()
        .map(|m| m["t"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(&emitted[emitted.len() - 3..], &["pause", "lobby", "start"]);
    assert!(a["cells"].is_array() && a["fog"].is_object());
    (a, b)
}

#[test]
fn ordinary_binary_opt_in_rejects_public_roles_and_retains_default_refusal() {
    for args in [
        vec!["--trusted-room", "ABC"],
        vec!["--seed", "1"],
        vec!["--trusted-room", "ab", "--seed", "1"],
        vec!["--trusted-room", "ABC", "--seed", "-1"],
        vec!["--trusted-room", "ABC", "--seed", "18446744073709551616"],
    ] {
        let result = Command::new(env!("CARGO_BIN_EXE_ww2-server"))
            .args(args)
            .output()
            .unwrap();
        assert!(!result.status.success());
        assert!(!String::from_utf8_lossy(&result.stdout).contains("LISTENING"));
    }
    let default = BinaryServer::start(false, SEED);
    let mut ordinary = Client::join(default.address, "a");
    ordinary.send(json!({"t":"start"}));
    assert_eq!(ordinary.deny("start")["reason"], "scenario");
    ordinary.send(json!({"t":"restart"}));
    assert_eq!(ordinary.deny("restart")["reason"], "scenario");
    assert!(default.receipts().is_empty());
    let server = BinaryServer::start(true, SEED);
    for (code, extra) in [
        ("XYZ", json!({})),
        ("ABC", json!({"listing":{"public":true}})),
        ("ABC", json!({"listing":{"tutorial":true}})),
        ("ABC", json!({"spectate":true})),
        ("ABC", json!({"role":"operative"})),
        ("ABC", json!({"role":"spectator"})),
        ("ABC", json!({"token":""})),
    ] {
        let mut hello = json!({"t":"hello","token":"probe","name":"probe"});
        hello
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        let mut probe = Client::open(server.address, code, hello);
        probe.deny("join");
        let (kind, body) = probe.frame();
        assert_eq!(kind, 8);
        assert_eq!(u16::from_be_bytes([body[0], body[1]]), 1008);
        assert_eq!(server.directory(), json!({"rooms":[]}));
        assert!(server.receipts().is_empty());
    }
    let mut host = Client::join(server.address, "a");
    host.send(json!({"t":"start"}));
    host.deny("start");
    assert!(server.receipts().is_empty());
    let _guest = Client::join(server.address, "b");
    let mut third = Client::open(
        server.address,
        "ABC",
        json!({"t":"hello","token":"c","name":"c"}),
    );
    third.deny("join");
    for command in ["addAi", "spectate"] {
        host.send(json!({"t":command}));
        host.deny(command);
    }
    assert!(host.barrier().iter().all(|m| m["t"] != "start"));
    assert!(server.receipts().is_empty());
}

#[test]
fn ordinary_binary_two_seats_move_stop_takeover_and_restart_with_private_seed() {
    let server = BinaryServer::start(true, u64::MAX);
    let mut host = Client::join(server.address, "a");
    let mut guest = Client::join(server.address, "b");
    let (start, _) = begin(&mut host, &mut guest);
    let identity = start["matchId"].as_u64().unwrap();
    let mut clients = [host, guest];
    for (slot, client) in clients.iter_mut().enumerate() {
        client.until(|m| m["t"] == "s");
        let unit = client
            .view
            .units()
            .find(|u| u.owner == slot as i32 && u.kind == "rifle")
            .unwrap()
            .clone();
        client.send(json!({"t":"stance","ids":[unit.id],"key":"holdFire","on":true}));
        client.send(json!({"t":"move","orders":[[unit.id,unit.x,unit.z+if slot==0 {12.} else {-12.}]],"slot":1-slot,"tick":777}));
        client.until(|m| {
            m["t"] == "s"
                && m["units"].as_array().unwrap().iter().any(|row| {
                    row[0] == unit.id
                        && (row[3].as_f64().unwrap() - unit.x).abs()
                            + (row[4].as_f64().unwrap() - unit.z).abs()
                            > 0.1
                })
        });
        client.send(json!({"t":"stop","ids":[unit.id]}));
        client.until(|m| {
            m["t"] == "s"
                && m["movement"].as_array().is_some_and(|rows| {
                    rows.iter()
                        .any(|row| row[0] == unit.id && row[1] == "interrupted")
                })
        });
    }
    let foreign = clients[1]
        .view
        .units()
        .find(|u| u.owner == 1 && u.kind == "rifle")
        .unwrap()
        .id;
    clients[0].send(json!({"t":"stop","ids":[foreign],"slot":1,"issuer":1}));
    assert_eq!(clients[0].deny("stop")["reason"], "needs");
    clients[1].send(json!({"t":"restart","slot":0}));
    clients[1].deny("restart");
    assert_eq!(server.receipts().len(), 1);
    let mut replacement = Client::open(
        server.address,
        "ABC",
        json!({"t":"hello","token":"b","name":"b"}),
    );
    let resumed = replacement.until(|m| m["t"] == "start");
    assert_eq!(resumed["matchId"], identity);
    clients[1].until(|m| m["t"] == "replaced");
    replacement.until(|m| m["t"] == "s");
    clients[0].send(json!({"t":"restart"}));
    let fresh = clients[0].until(|m| m["t"] == "start");
    assert!(fresh["matchId"].as_u64().unwrap() > identity);
    assert_eq!(
        replacement.until(|m| m["t"] == "start")["matchId"],
        fresh["matchId"]
    );
    assert_eq!(fresh["spawns"], start["spawns"]);
    assert_eq!(fresh["weather"], start["weather"]);
    clients[0].barrier();
    let receipts = server.receipts();
    assert_eq!(receipts.len(), 2);
    assert_eq!(receipts[0]["options"], receipts[1]["options"]);
    for receipt in receipts {
        let options = &receipt["options"];
        assert_eq!(options["seed"].as_u64(), Some(u64::MAX));
        assert_eq!(options["weatherSeed"], Value::Null);
        for key in ["shuffle", "tech", "supply", "livingWeather"] {
            assert_eq!(options[key], true);
        }
        for key in ["logistics", "sandbox"] {
            assert_eq!(options[key], false);
        }
        assert_eq!(options.as_object().unwrap().len(), 13);
    }
    assert_eq!(server.directory(), json!({"rooms":[]}));
}

#[test]
fn ordinary_binary_absent_seat_start_and_restart_auto_pause_then_same_token_resume() {
    let server = BinaryServer::start(true, SEED);
    let mut host = Client::join(server.address, "a");
    let guest = Client::join(server.address, "b");
    guest.socket.shutdown(Shutdown::Both).unwrap();
    drop(guest);
    let lobby = host.until(|m| m["t"] == "lobby" && m["players"][1]["connected"] == false);
    assert_eq!(lobby["players"].as_array().unwrap().len(), 2);
    host.send(json!({"t":"start"}));
    let first = host.until(|m| m["t"] == "start");
    assert_eq!(
        host.until(|m| m["t"] == "pause" && m["paused"] == true)["reason"],
        "drop"
    );
    assert!(host.barrier().iter().all(|m| m["t"] != "s"));
    let mut guest = Client::open(
        server.address,
        "ABC",
        json!({"t":"hello","token":"b","name":"b"}),
    );
    assert_eq!(
        guest.until(|m| m["t"] == "start")["matchId"],
        first["matchId"]
    );
    guest.until(|m| m["t"] == "pause" && m["paused"] == true);
    guest.until(|m| m["t"] == "pause" && m["paused"] == false);
    guest.until(|m| m["t"] == "s");
    host.until(|m| m["t"] == "s");
    guest.socket.shutdown(Shutdown::Both).unwrap();
    drop(guest);
    host.until(|m| m["t"] == "lobby" && m["players"][1]["connected"] == false);
    host.send(json!({"t":"restart"}));
    let second = host.until(|m| m["t"] == "start");
    assert!(second["matchId"].as_u64().unwrap() > first["matchId"].as_u64().unwrap());
    host.until(|m| m["t"] == "pause" && m["paused"] == true);
    let mut guest = Client::open(
        server.address,
        "ABC",
        json!({"t":"hello","token":"b","name":"b"}),
    );
    assert_eq!(
        guest.until(|m| m["t"] == "start")["matchId"],
        second["matchId"]
    );
    guest.until(|m| m["t"] == "pause" && m["paused"] == false);
    guest.until(|m| m["t"] == "s");
    host.barrier();
    assert_eq!(server.receipts().len(), 2);
}

#[test]
fn ordinary_binary_refuses_unsupported_settings_before_any_start_publication() {
    let cases = [
        json!({"t":"mode","v":"world"}),
        json!({"t":"mode","v":"assault"}),
        json!({"t":"mode","v":"annihilation"}),
        json!({"t":"mode","v":"classic"}),
        json!({"t":"mode","v":"horde"}),
        json!({"t":"mode","v":"tutorial"}),
        json!({"t":"army","v":"large"}),
        json!({"t":"army","v":"massive"}),
        json!({"t":"army","v":"endless"}),
        json!({"t":"sandbox","v":true}),
        json!({"t":"team","slot":1,"v":0}),
    ];
    for setting in cases {
        let server = BinaryServer::start(true, SEED);
        let mut host = Client::join(server.address, "a");
        let _guest = Client::join(server.address, "b");
        host.send(setting.clone());
        host.barrier();
        host.send(json!({"t":"start"}));
        assert_eq!(host.deny("start")["reason"], "scenario", "{setting}");
        assert!(
            host.barrier()
                .iter()
                .all(|m| m["t"] != "start" && m["t"] != "s" && m["state"] != "play")
        );
        assert!(server.receipts().is_empty());
        assert_eq!(server.directory(), json!({"rooms":[]}));
    }
    for weather in ["map", "clear", "fog", "rain", "mud", "snow", "random"] {
        let server = BinaryServer::start(true, SEED);
        let mut host = Client::join(server.address, "a");
        let mut guest = Client::join(server.address, "b");
        host.send(json!({"t":"weather","v":weather}));
        host.barrier();
        begin(&mut host, &mut guest);
        host.until(|m| m["t"] == "s");
        assert_eq!(server.receipts()[0]["options"]["weather"], weather);
    }
}

#[test]
fn ordinary_binary_genuine_mine_contact_latches_fault_and_refuses_restart() {
    let server = BinaryServer::start(true, SEED);
    let mut host = Client::join(server.address, "a");
    let mut guest = Client::join(server.address, "b");
    host.send(json!({"t":"map","name":"el-alamein"}));
    host.barrier();
    host.send(json!({"t":"weather","v":"clear"}));
    host.barrier();
    let (a, b) = begin(&mut host, &mut guest);
    let identity = a["matchId"].clone();
    let west = if a["spawn"]["x"].as_f64().unwrap() < b["spawn"]["x"].as_f64().unwrap() {
        0
    } else {
        1
    };
    let mut clients = [host, guest];
    clients[west].until(|m| m["t"] == "s");
    let unit = clients[west]
        .view
        .units()
        .find(|u| u.owner == west as i32 && u.kind == "rifle")
        .unwrap()
        .clone();
    clients[west].send(json!({"t":"stance","ids":[unit.id],"key":"holdPos","on":true}));
    clients[west].until(|m| {
        m["t"] == "s"
            && m["units"]
                .as_array()
                .unwrap()
                .iter()
                .any(|row| row[0] == unit.id && row[12].as_u64().unwrap() & 4096 != 0)
    });
    // Reach genuine clear cell (46,39) before entering adjacent authored mine (47,39).
    let authored: Value =
        serde_json::from_str(include_str!("../src/authoritative-maps/el-alamein.json")).unwrap();
    assert_eq!(authored["rows"][39].as_str().unwrap().as_bytes()[47], b'N');
    assert_ne!(authored["rows"][39].as_str().unwrap().as_bytes()[46], b'N');
    assert_ne!(a["map"]["rows"][39].as_str().unwrap().as_bytes()[47], b'N');
    clients[west].send(json!({"t":"move","orders":[[unit.id,93.,79.]]}));
    clients[west].until(|m| {
        m["t"] == "s"
            && m["units"].as_array().unwrap().iter().any(|row| {
                row[0] == unit.id
                    && (row[3].as_f64().unwrap() - 93.).abs() < 0.6
                    && (row[4].as_f64().unwrap() - 79.).abs() < 0.6
            })
    });
    assert!(
        !server.log().contains("NATIVE_FAULT"),
        "staging route must complete without a fault"
    );
    clients[west].send(json!({"t":"move","orders":[[unit.id,95.,79.]]}));
    let fault = server.wait_fault();
    eprintln!("R1_AUTHORED_MINE_FAULT {fault}");
    assert_eq!(fault["matchId"], identity);
    assert!(
        fault["error"]
            .as_str()
            .unwrap()
            .contains("moved hostile mine detonation is not connected")
    );
    let until_fault = clients[0].barrier();
    assert!(
        until_fault
            .iter()
            .filter(|m| m["t"] == "s")
            .all(|m| m["tick"].as_i64().unwrap() < fault["tick"].as_i64().unwrap())
    );
    clients[0].send(json!({"t":"restart"}));
    assert_eq!(clients[0].deny("restart")["reason"], "scenario");
    clients[1].send(json!({"t":"restart"}));
    clients[1].deny("restart");
    for client in &mut clients {
        assert!(
            client
                .barrier()
                .iter()
                .all(|m| m["t"] != "start" && m["t"] != "s")
        );
    }
    assert_eq!(server.receipts().len(), 1);
    assert_eq!(
        server
            .log()
            .lines()
            .filter(|l| l.starts_with("NATIVE_FAULT "))
            .count(),
        1
    );
    assert!(
        server
            .log()
            .contains("faulted match restart awaits a recovery policy")
    );
}

#[path = "trusted_room_process/net2.rs"]
mod net2;
