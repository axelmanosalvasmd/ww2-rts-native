mod common;
use common::{BinaryServer, Client};
use serde_json::{Value, json};
use std::{net::Shutdown, process::Command};

const SEED: u64 = 3936341;

#[test]
fn net14_public_url_environment_reaches_live_lobby_invites() {
    for configured in [None, Some("https://invite.example/game")] {
        let server = BinaryServer::start_with_public_url(false, SEED, configured);
        let mut client = Client::open(
            server.address,
            "ABC",
            json!({"t":"hello","token":"host","name":"Host"}),
        );
        let lobby = client.until(|m| m["t"] == "lobby");
        assert_eq!(lobby["publicUrl"], configured.unwrap_or_default());
        assert_eq!(lobby["code"], "ABC");
        client.send(json!({"t":"name","name":"Renamed"}));
        assert_eq!(
            client.until(|m| m["t"] == "lobby")["publicUrl"],
            configured.unwrap_or_default()
        );
    }
}

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
fn ordinary_binary_opt_in_admits_spectators_and_retains_other_refusals() {
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
    assert_eq!(ordinary.until(|m| m["t"] == "start")["you"], 0);
    assert_eq!(ordinary.until(|m| m["t"] == "s")["online"], json!([true]));
    ordinary.send(json!({"t":"restart"}));
    assert_eq!(ordinary.deny("restart")["reason"], "scenario");
    assert!(default.receipts().is_empty());
    let server = BinaryServer::start(true, SEED);
    for (code, extra) in [
        ("XYZ", json!({})),
        ("ABC", json!({"listing":{"public":true}})),
        ("ABC", json!({"listing":{"tutorial":true}})),
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
    let mut watcher = Client::open(
        server.address,
        "ABC",
        json!({"t":"hello","token":"watcher","name":"Watcher","spectate":true}),
    );
    let watching = watcher.until(|m| m["t"] == "lobby");
    assert_eq!(watching["spectator"], true);
    assert_eq!(watching["you"], -1);
    host.send(json!({"t":"addAi"}));
    let lobby =
        host.until(|m| m["t"] == "lobby" && m["players"].as_array().is_some_and(|p| p.len() == 3));
    assert_eq!(lobby["players"][2]["ai"], true);
    host.send(json!({"t":"kick","slot":2}));
    host.until(|m| m["t"] == "lobby" && m["players"].as_array().is_some_and(|p| p.len() == 2));
    host.send(json!({"t":"spectate"}));
    let released = host.until(|m| m["t"] == "lobby" && m["spectator"] == true);
    assert_eq!(released["players"].as_array().unwrap().len(), 1);
    assert_eq!(released["spectators"].as_array().unwrap().len(), 2);
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
    let guest_fresh = replacement.until(|m| m["t"] == "start");
    assert_eq!(guest_fresh["matchId"], fresh["matchId"]);
    assert_eq!(guest_fresh["spawns"], fresh["spawns"]);
    assert_eq!(guest_fresh["weather"], fresh["weather"]);
    clients[0].barrier();
    let receipts = server.receipts();
    assert_eq!(receipts.len(), 2);
    let mut fresh_settings = receipts[1]["options"].clone();
    fresh_settings["seed"] = receipts[0]["options"]["seed"].clone();
    fresh_settings["weatherSeed"] = receipts[0]["options"]["weatherSeed"].clone();
    assert_eq!(receipts[0]["options"], fresh_settings);
    assert_ne!(
        receipts[0]["environmentSeed"],
        receipts[1]["environmentSeed"]
    );
    for (index, receipt) in receipts.into_iter().enumerate() {
        let options = &receipt["options"];
        assert_eq!(
            options["seed"].as_u64(),
            Some(u64::MAX.wrapping_add(index as u64))
        );
        assert!(options["weatherSeed"].as_u64().unwrap() < (1 << 31));
        assert_eq!(options["weatherSeed"], receipt["derivedWeatherSeed"]);
        for key in ["shuffle", "tech", "supply", "livingWeather", "logistics"] {
            assert_eq!(options[key], true);
        }
        assert_eq!(options["sandbox"], false);
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
    assert_eq!(
        host.until(|m| m["t"] == "pause" && m["paused"] == true)["reason"],
        "drop"
    );
    let first = host.until(|m| m["t"] == "start");
    assert!(
        host.barrier()
            .iter()
            .filter(|m| m["t"] == "s")
            .all(|m| m["tick"] == 0)
    );
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
    host.until(|m| m["t"] == "pause" && m["paused"] == true);
    let second = host.until(|m| m["t"] == "start");
    assert!(second["matchId"].as_u64().unwrap() > first["matchId"].as_u64().unwrap());
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
        let receipts = server.receipts();
        assert_eq!(receipts[0]["options"]["weather"], weather);
        assert!(receipts[0]["options"]["weatherSeed"].as_u64().unwrap() < (1 << 31));
        assert_eq!(
            receipts[0]["options"]["weatherSeed"],
            receipts[0]["derivedWeatherSeed"]
        );
    }
}

#[test]
fn ordinary_binary_genuine_mine_contact_detonates_and_keeps_delivering_snapshots() {
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
    let mine_cell = 39 * authored["w"].as_u64().unwrap() + 47;
    let blast = clients[west].until(|m| {
        m["t"] == "s"
            && m["cells"]
                .as_array()
                .is_some_and(|rows| rows.iter().any(|row| row[0] == mine_cell && row[1] == "+"))
    });
    let survivor = clients[west]
        .view
        .units()
        .find(|u| u.id == unit.id)
        .unwrap();
    assert!(
        survivor.hp < unit.hp,
        "genuine contact must damage the mover"
    );
    assert!(survivor.supp > 0., "the blast must suppress infantry");
    let after_blast = blast["tick"].as_u64().unwrap() + 10;
    for client in &mut clients {
        client.until(|m| m["t"] == "s" && m["tick"].as_u64().unwrap() >= after_blast);
        assert_eq!(client.view.start().unwrap()["matchId"], identity);
    }
    assert!(!server.log().contains("NATIVE_FAULT"));
    // Only the host may restart, even after a mine detonates.
    clients[1].send(json!({"t":"restart"}));
    clients[1].deny("restart");
    clients[0].send(json!({"t":"restart"}));
    let fresh = clients[0].until(|m| m["t"] == "start");
    assert!(fresh["matchId"].as_u64().unwrap() > identity.as_u64().unwrap());
    assert_eq!(
        clients[1].until(|m| m["t"] == "start")["matchId"],
        fresh["matchId"]
    );
    for client in &mut clients {
        client.until(|m| m["t"] == "s");
        client.barrier();
    }
    assert_eq!(server.receipts().len(), 2);
    assert!(!server.log().contains("NATIVE_FAULT"));
}

#[path = "trusted_room_process/net2.rs"]
mod net2;

#[test]
fn net97_ordinary_binary_environment_writes_private_diagnostics_while_r1_snapshots_continue() {
    let dir = std::env::temp_dir().join(format!("ww2-net97-process-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let server = BinaryServer::start_with_diagnostics(true, SEED, Some(&dir));
    let mut host = Client::join(server.address, "a");
    let mut guest = Client::join(server.address, "b");
    begin(&mut host, &mut guest);
    host.send(json!({"t":"ping","c":1,"d":[60,17,3],"rtt":30}));
    let file = dir.join("logs/diag-ABC.jsonl");
    let began = std::time::Instant::now();
    let line = loop {
        // Ordered frames prove the room keeps running while waiting for actual recorder output.
        host.until(|m| m["t"] == "s");
        guest.until(|m| m["t"] == "s");
        if let Ok(data) = std::fs::read_to_string(&file)
            && let Some(line) = data.lines().next()
            && let Ok(value) = serde_json::from_str::<Value>(line)
        {
            break value;
        }
        assert!(
            began.elapsed() < std::time::Duration::from_secs(20),
            "diagnostic output deadline: {}",
            server.log()
        );
    };
    assert_eq!(line["players"][0]["client"]["fps"], 60.0);
    assert_eq!(line["players"][1]["client"], Value::Null);
    assert!(line["tick"].as_u64().unwrap() >= 100);
    assert!(line["tickP95"].as_f64().unwrap() >= 0.0);
    for client in [&mut host, &mut guest] {
        let snapshot = client.until(|m| m["t"] == "s");
        assert!(snapshot.get("client").is_none() && snapshot.get("loopP99").is_none());
        assert!(client.view.unit_count() > 0);
    }
    assert!(!server.log().contains("NATIVE_FAULT"));
    drop(server);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn mode06_binary_named_armies_start_and_deliver_to_both_sockets() {
    for (key, income, pop) in [
        ("standard", 1.0, 1.0),
        ("large", 3.0, 2.5),
        ("massive", 6.0, 5.0),
        ("endless", 20.0, 5.0),
    ] {
        let server = BinaryServer::start(true, SEED);
        let mut host = Client::join(server.address, "a");
        let mut guest = Client::join(server.address, "b");
        host.send(json!({"t":"army","v":key}));
        host.barrier();
        begin(&mut host, &mut guest);
        for client in [&mut host, &mut guest] {
            for _ in 0..3 {
                let snapshot = client.until(|m| m["t"] == "s");
                assert_eq!(snapshot["army"], json!({"pop":pop,"income":income}));
            }
        }
        host.barrier();
        assert_eq!(server.receipts()[0]["options"]["army"], key);
        assert!(!server.log().contains("NATIVE_FAULT"));
    }
}

#[test]
fn room_ai_binary_starts_selected_difficulty_and_delivers_recipient_snapshots() {
    let server = BinaryServer::start(true, SEED);
    let mut host = Client::join(server.address, "a");
    host.send(json!({"t":"addAi"}));
    host.until(|m| m["t"] == "lobby" && m["players"].as_array().is_some_and(|p| p.len() == 2));
    host.send(json!({"t":"level","slot":1,"v":"hard"}));
    let lobby = host.until(|m| m["t"] == "lobby" && m["players"][1]["level"] == "hard");
    assert_eq!(
        lobby["players"][1],
        json!({"name":"AI 1","connected":true,"ai":true,"team":1,"faction":1,"level":"hard"})
    );
    host.send(json!({"t":"start"}));
    let start = host.until(|m| m["t"] == "start");
    assert_eq!(start["names"], json!(["a", "AI 1"]));
    for _ in 0..4 {
        let snapshot = host.until(|m| m["t"] == "s");
        assert_eq!(snapshot["online"], json!([true, true]));
        assert_eq!(snapshot["ping"][1].as_f64(), Some(-1.0));
    }
    assert_eq!(server.receipts().len(), 1);
}
