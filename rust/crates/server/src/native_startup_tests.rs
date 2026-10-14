//! Real websocket receiver regressions for the trusted ordinary startup path.
//! Both authoring data and simulation inputs remain canonical. No packet is forged.
use super::*;
use crate::{Config, NativeRoomControl, serve_with_native_control};
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
};
use tokio::{net::TcpListener, sync::oneshot};
use ww2_core::{
    client_view::{Applied, ClientView, ViewError},
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::Point,
    match_runtime::{MatchOptions, PlayerConfig},
    weather::WeatherChoice,
};

struct Receiver {
    socket: TcpStream,
    view: ClientView,
    messages: Vec<Value>,
}
impl Receiver {
    fn open(address: SocketAddr, token: &str) -> Self {
        let mut socket = TcpStream::connect(address).unwrap();
        socket
            .set_read_timeout(Some(std::time::Duration::from_secs(10)))
            .unwrap();
        socket
            .set_write_timeout(Some(std::time::Duration::from_secs(10)))
            .unwrap();
        write!(socket, "GET /ws?room=ABC HTTP/1.1\r\nHost: {address}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n").unwrap();
        let mut headers = Vec::new();
        while !headers.ends_with(b"\r\n\r\n") {
            let mut byte = [0];
            socket.read_exact(&mut byte).unwrap();
            headers.push(byte[0]);
            assert!(headers.len() < 8192);
        }
        assert!(
            String::from_utf8(headers)
                .unwrap()
                .starts_with("HTTP/1.1 101")
        );
        let mut receiver = Self {
            socket,
            view: ClientView::new(),
            messages: Vec::new(),
        };
        receiver.send(json!({"t":"hello","token":token,"name":token}));
        receiver
    }
    fn connect(address: SocketAddr, token: &str) -> Self {
        let mut receiver = Self::open(address, token);
        receiver.until(|message| message["t"] == "lobby");
        receiver
    }
    fn send(&mut self, message: Value) {
        let bytes = message.to_string().into_bytes();
        assert!(bytes.len() < 126);
        let mask = [13_u8, 17, 19, 23];
        let mut frame = vec![0x81, 0x80 | bytes.len() as u8];
        frame.extend(mask);
        frame.extend(bytes.iter().enumerate().map(|(i, byte)| byte ^ mask[i % 4]));
        self.socket.write_all(&frame).unwrap();
    }
    fn receive(&mut self) -> Value {
        let mut header = [0; 2];
        self.socket.read_exact(&mut header).unwrap();
        assert_eq!(header[0], 0x81, "expected a complete JSON text frame");
        assert_eq!(header[1] & 0x80, 0, "server frames are unmasked");
        let size = match header[1] & 127 {
            126 => {
                let mut n = [0; 2];
                self.socket.read_exact(&mut n).unwrap();
                usize::from(u16::from_be_bytes(n))
            }
            127 => {
                let mut n = [0; 8];
                self.socket.read_exact(&mut n).unwrap();
                usize::try_from(u64::from_be_bytes(n)).unwrap()
            }
            size => usize::from(size),
        };
        assert!(size < 16 * 1024 * 1024);
        let mut bytes = vec![0; size];
        self.socket.read_exact(&mut bytes).unwrap();
        let text = String::from_utf8(bytes).unwrap();
        let message: Value = serde_json::from_str(&text).unwrap();
        // Every received message goes through the actual decoder. In particular,
        // an s before start or an object fog baseline in s fails here.
        let applied = self
            .view
            .apply(&text)
            .unwrap_or_else(|error| panic!("{} decoder rejected packet: {error}", message["t"]));
        if message["t"] == "start" {
            assert_eq!(applied, Applied::Start);
        }
        if message["t"] == "s" {
            assert_eq!(applied, Applied::Snapshot);
            assert!(message.get("fog").is_none_or(Value::is_string));
            assert!(
                !self.view.needs_resync(),
                "complete primary receiver must retain the held set"
            );
            let ids = self.view.units().map(|unit| unit.id).collect::<Vec<_>>();
            assert_eq!(message["held"][0].as_u64(), Some(ids.len() as u64));
            assert_eq!(
                message["held"][1].as_i64(),
                Some(i64::from(
                    ids.iter().fold(0_i32, |xor, id| xor ^ *id as i32)
                ))
            );
        }
        self.messages.push(message.clone());
        message
    }
    fn until(&mut self, wanted: impl Fn(&Value) -> bool) -> Value {
        for _ in 0..512 {
            let message = self.receive();
            if wanted(&message) {
                return message;
            }
        }
        panic!("expected lifecycle packet did not arrive");
    }
}

fn setup(map: &str) -> Match {
    crate::authoritative_maps::create_match(
        map,
        MatchOptions {
            seed: 0x3c1055,
            mode: Mode::Conquest,
            players: (0..2)
                .map(|slot| PlayerConfig {
                    name: ["a", "b"][slot].into(),
                    team: slot as i32,
                    faction: Faction::try_from(slot as u8).unwrap(),
                })
                .collect(),
            shuffle: false,
            army: ArmySize::Standard,
            weather: WeatherChoice::Map,
            weather_seed: Some(1),
            living_weather: false,
            map_key: Some(map.into()),
            tech: true,
            supply: false,
            logistics: false,
            sandbox: false,
        },
    )
    .unwrap()
}
struct SetupExpected {
    spawns: Vec<Point>,
    weather: &'static str,
    public_map: Value,
}
impl SetupExpected {
    fn new(state: &Match) -> Self {
        Self {
            spawns: state
                .start
                .players
                .iter()
                .map(|player| player.spawn)
                .collect(),
            weather: state.start.weather.now.name(),
            public_map: serde_json::from_str(include_str!("maps/default.json")).unwrap(),
        }
    }
    fn check(&self, start: &Value, slot: usize, identity: u64) {
        assert_eq!(start["t"], "start");
        assert_eq!(start["matchId"].as_u64(), Some(identity));
        assert!(identity <= 9_007_199_254_740_991);
        assert_eq!(
            start["map"], self.public_map,
            "complete pinned mapForClient projection"
        );
        assert_eq!(start["you"], slot);
        assert_eq!(start["names"], json!(["a", "b"]));
        assert_eq!(start["teams"], json!([0, 1]));
        assert_eq!(start["factions"], json!([0, 1]));
        assert_eq!(start["weather"], json!([self.weather]));
        assert_eq!(start["sandbox"], false);
        assert_eq!(
            start.as_object().unwrap().len(),
            13,
            "existing ordinary Source setup fields"
        );
        for (i, at) in self.spawns.iter().enumerate() {
            assert_eq!(start["spawns"][i]["x"].as_f64(), Some(at.x));
            assert_eq!(start["spawns"][i]["z"].as_f64(), Some(at.z));
        }
        assert_eq!(start["spawn"], start["spawns"][slot]);
        assert!(start["cells"].is_array());
        assert!(start["fog"]["v"].is_string() && start["fog"]["e"].is_string());
        check_public_map(&start["map"]);
    }
}
fn check_public_map(map: &Value) {
    assert!(map.get("triggers").is_none());
    assert!(map.get("scenario").is_none());
    assert!(
        map["rows"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| !row.as_str().unwrap().contains('N'))
    );
    for key in ["objects", "mines"] {
        assert!(
            map["layers"][key]
                .as_array()
                .unwrap()
                .iter()
                .all(|row| !row.as_str().unwrap().contains('N'))
        );
    }
    if let Some(materials) = map["layers"].get("mineMaterials") {
        assert_eq!(materials, &json!([]));
    }
}
async fn native<T>(control: &NativeRoomControl, change: impl FnOnce(&mut RoomRuntime) -> T) -> T {
    let mut hub = control.server.hub.lock().await;
    change(
        &mut hub
            .rooms
            .get_mut("ABC")
            .unwrap()
            .native
            .as_mut()
            .unwrap()
            .runtime,
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn actual_socket_start_decode_mutations_resync_reconnect_and_new_match() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (stop, stopped) = oneshot::channel();
    let (ready, receipt) = oneshot::channel();
    let server = tokio::spawn(serve_with_native_control(
        listener,
        Config::default(),
        async move {
            let _ = stopped.await;
        },
        Some(ready),
    ));
    let control = receipt.await.unwrap();
    let host = tokio::runtime::Handle::current();
    tokio::task::spawn_blocking(move || {
        let mut a = Receiver::connect(address, "a");
        let mut b = Receiver::connect(address, "b");
        a.send(json!({"t":"map","name":"el-alamein"}));
        a.until(|message| message["t"] == "lobby" && message["mapName"] == "el-alamein");
        a.send(json!({"t":"start"}));
        assert_eq!(a.until(|message| message["t"] == "deny")["cmd"], "start");
        b.until(|message| message["t"] == "lobby" && message["mapName"] == "el-alamein");
        let scripted = setup("el-alamein");
        assert!(scripted.scenario.is_some());
        let authored = crate::authoritative_maps::load("el-alamein").unwrap();
        assert!(
            authored.source()["rows"]
                .as_array()
                .unwrap()
                .iter()
                .any(|row| row.as_str().unwrap().contains('N'))
        );
        assert!(!authored.triggers().is_empty());
        let public = crate::maps::public_map("el-alamein").unwrap();
        assert_eq!(
            public,
            serde_json::from_str::<Value>(include_str!("maps/el-alamein.json")).unwrap(),
            "complete pinned private-mine mapForClient projection"
        );
        check_public_map(&public);
        let width = authored.dimensions().0;
        let hidden = authored.source()["rows"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
            .flat_map(|(z, row)| {
                row.as_str()
                    .unwrap()
                    .bytes()
                    .enumerate()
                    .filter_map(move |(x, cell)| (cell == b'N').then_some(z * width + x))
            })
            .collect::<std::collections::BTreeSet<_>>();
        for key in ["materials", "objectMaterials"] {
            if let Some(materials) = public["layers"][key].as_array() {
                assert!(
                    materials
                        .iter()
                        .all(|row| { !hidden.contains(&(row["c"].as_u64().unwrap() as usize)) })
                );
            }
        }
        let lobby_before = host.block_on(async {
            let hub = control.server.hub.lock().await;
            let room = &hub.rooms["ABC"];
            room.lobby(
                room.players[0].connection.as_ref().unwrap().id,
                &hub.maps,
                &hub.public_url,
            )
        });
        let refusal = host.block_on(control.attach("ABC", scripted)).unwrap_err();
        assert!(refusal.contains("capability: Scenario"), "{refusal}");
        host.block_on(async {
            let hub = control.server.hub.lock().await;
            assert!(hub.rooms["ABC"].native.is_none());
            assert!(hub.accounts.is_empty());
            assert_eq!(hub.next_epoch, 0);
            let room = &hub.rooms["ABC"];
            assert_eq!(
                room.lobby(
                    room.players[0].connection.as_ref().unwrap().id,
                    &hub.maps,
                    &hub.public_url,
                ),
                lobby_before,
                "refused attachment preserves actual lobby roster, settings and result"
            );
        });
        // The next actual packet is the requested default lobby, not a partial
        // start or snapshot from the refused scripted attachment.
        a.send(json!({"t":"map","name":"default"}));
        for receiver in [&mut a, &mut b] {
            let message = receiver.receive();
            assert_eq!(message["t"], "lobby");
            assert_eq!(message["mapName"], "default");
        }
        let state = setup("default");
        assert!(state.scenario.is_none());
        let expected = SetupExpected::new(&state);
        let victim = state
            .world
            .game
            .units
            .values()
            .find(|unit| unit.owner == 0 && state.rules.kinds[unit.kind].name == "rifle")
            .unwrap()
            .id;
        let identity = host.block_on(control.attach("ABC", state)).unwrap();
        let start_a = a.until(|message| message["t"] == "start");
        expected.check(&start_a, 0, identity);
        let at_start = a.messages.len() - 1;
        assert_eq!(a.messages[at_start - 1]["t"], "lobby");
        assert_eq!(a.messages[at_start - 1]["state"], "play");
        assert_eq!(a.messages[at_start - 2]["t"], "pause");
        assert_eq!(a.messages[at_start - 2]["paused"], false);
        let start_b = b.until(|message| message["t"] == "start");
        expected.check(&start_b, 1, identity);
        let first = a.until(|message| message["t"] == "s");
        assert!(a.view.unit(victim).is_some());
        assert!(
            first.get("all").is_none(),
            "Source sendStart resets sent rows without requesting resync"
        );
        // Retain the original missing-start failure with an actual complete host
        // snapshot, rather than adopting the old hello/lobby/s stream as valid.
        let mut missing_start = ClientView::new();
        assert!(matches!(
            missing_start.apply(&first.to_string()),
            Err(ViewError::WrongState(_))
        ));
        assert_eq!(missing_start.unit_count(), 0);
        let accepted_start = a.view.start().unwrap().clone();
        let held = a.view.unit_count();
        a.send(json!({"t":"name","name":"a"}));
        a.until(|message| message["t"] == "lobby" && message["state"] == "play");
        assert_eq!(a.view.start(), Some(&accepted_start));
        assert_eq!(
            a.view.unit_count(),
            held,
            "later lobby broadcast cannot reset accepted state"
        );
        let before_fog = a.view.seen().to_vec();
        // A real recorded move advances the visibility frontier through the
        // native tick owner. It does not write a decoder mask or replace RNG.
        host.block_on(native(&control, |runtime| {
            let state = runtime.state_mut();
            let at = state.world.game.units[&victim].pose();
            let goal = Point {
                x: (at.x + 40.).min(state.world.game.w as f64 * 2. - 3.),
                z: at.z,
            };
            state
                .enqueue_command(
                    ww2_core::orders::OrderKey {
                        tick: state.world.game.tick as u64 + 1,
                        issuer: 0,
                        sequence: 1,
                    },
                    ww2_core::match_commands::Command::Move {
                        orders: vec![ww2_core::match_commands::MoveOrder {
                            unit: i64::from(victim),
                            at: goal,
                        }],
                        control_ids: None,
                        attack_move: false,
                        queue: false,
                        together: false,
                        face: None,
                    },
                )
                .unwrap();
        }));
        for _ in 0..128 {
            if before_fog != a.view.seen() {
                break;
            }
            a.receive();
        }
        assert_ne!(
            before_fog,
            a.view.seen(),
            "actual movement must change the full-start fog baseline"
        );
        // Canonical terrain mutation and actual dead retirement, not a replacement packet.
        let cell = host.block_on(native(&control, |runtime| {
            let state = runtime.state_mut();
            let at = state.world.game.units[&victim].pose();
            let origin = state.world.game.cell_of(at.x, at.z).unwrap();
            let cell = (origin.saturating_sub(1)
                ..=(origin + 1).min(state.world.game.chars.len() - 1))
                .find(|&cell| {
                    state.world.game.objects.as_ref().unwrap()[cell] == b'.'
                        && state.world.game.chars[cell] != b'D'
                })
                .unwrap();
            state.world.set_cell(cell, b'D').unwrap();
            cell
        }));
        for _ in 0..32 {
            if a.view
                .cells()
                .get(&(cell as u32))
                .is_some_and(|row| row[1] == "D")
            {
                break;
            }
            a.receive();
        }
        assert_eq!(a.view.cells()[&(cell as u32)][1], "D");
        host.block_on(native(&control, |runtime| {
            runtime
                .state_mut()
                .world
                .game
                .units
                .get_mut(&victim)
                .unwrap()
                .hp = 0.;
        }));
        a.until(|message| {
            message["t"] == "s"
                && message["gone"]
                    .as_array()
                    .is_some_and(|ids| ids.iter().any(|id| id.as_u64() == Some(u64::from(victim))))
        });
        assert!(a.view.unit(victim).is_none());
        let mut lagged = a.view.clone();
        let published = host.block_on(native(&control, |runtime| {
            let state = runtime.state_mut();
            state.start.economy.players[0].mp = 1000.;
            state.recorded_buy(0, "rifle", None).unwrap().unit
        }));
        let mut lost = false;
        for _ in 0..64 {
            let message = a.receive();
            if !lost
                && message["t"] == "s"
                && message["units"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|row| row[0].as_u64() == Some(u64::from(published)))
            {
                // Only this secondary consumer loses one actual application.
                // The primary above applied every received packet successfully.
                lost = true;
            } else {
                lagged.apply(&message.to_string()).unwrap();
            }
            if lost && lagged.needs_resync() {
                break;
            }
        }
        assert!(lost && lagged.needs_resync());
        assert!(a.view.unit(published).is_some());
        a.send(json!({"t":"resync"}));
        for _ in 0..32 {
            let message = a.receive();
            lagged.apply(&message.to_string()).unwrap();
            if message["t"] == "s" && message["all"] == true {
                break;
            }
        }
        assert_eq!(a.messages.last().unwrap()["all"], true);
        assert!(!lagged.needs_resync());
        assert_eq!(lagged, a.view);
        let resync_tick = a.messages.last().unwrap()["tick"].as_u64().unwrap();
        let peer = b.until(|message| {
            message["t"] == "s"
                && message["tick"]
                    .as_u64()
                    .is_some_and(|tick| tick >= resync_tick)
        });
        assert!(
            peer.get("all").is_none(),
            "one seat's resync cannot reset another socket ledger"
        );
        // Real drop pauses the current stable seat. Replacing another seat must
        // deliver its start and paused state without resuming the dropped seat.
        drop(a);
        b.until(|message| message["t"] == "pause" && message["paused"] == true);
        let mut replacement_b = Receiver::open(address, "b");
        let b_start = replacement_b.receive();
        expected.check(&b_start, 1, identity);
        let paused = replacement_b.receive();
        assert_eq!(paused["t"], "pause");
        assert_eq!(paused["paused"], true);
        assert_eq!(paused["reason"], "drop");
        let later_lobby = replacement_b.receive();
        assert_eq!(later_lobby["t"], "lobby");
        assert_eq!(later_lobby["state"], "play");
        assert_eq!(replacement_b.view.start(), Some(&b_start));
        assert_eq!(replacement_b.view.unit_count(), 0);
        assert!(host.block_on(control.inspect("ABC")).unwrap().paused);
        let mut a = Receiver::open(address, "a");
        let reconnect = a.receive();
        expected.check(&reconnect, 0, identity);
        let paused = a.receive();
        assert_eq!(paused["t"], "pause");
        assert_eq!(paused["paused"], true);
        assert_eq!(paused["reason"], "drop");
        let resumed = a.receive();
        assert_eq!(resumed["t"], "pause");
        assert_eq!(resumed["paused"], false);
        let later_lobby = a.receive();
        assert_eq!(later_lobby["t"], "lobby");
        assert_eq!(later_lobby["state"], "play");
        assert_eq!(a.view.start(), Some(&reconnect));
        assert_eq!(a.view.unit_count(), 0);
        assert_eq!(a.view.pause(), Some(&resumed));
        a.until(|message| message["t"] == "s");
        assert!(a.view.unit(victim).is_none());
        assert!(a.view.unit(published).is_some());
        assert_eq!(a.view.cells()[&(cell as u32)][1], "D");
        host.block_on(native(&control, |runtime| {
            let state = runtime.state_mut();
            state.start.economy.players[0].vp = state.start.win_vp;
            state.apply_conquest_victory().unwrap().unwrap();
        }));
        a.until(|message| message["t"] == "lobby" && message["result"].is_object());
        assert!(
            host.block_on(control.inspect("ABC"))
                .unwrap()
                .ending_complete
        );
        let next = setup("default");
        let next_expected = SetupExpected::new(&next);
        let new_identity = host.block_on(control.attach("ABC", next)).unwrap();
        assert!(new_identity > identity);
        let fresh = a.until(|message| message["t"] == "start");
        next_expected.check(&fresh, 0, new_identity);
        assert_eq!(a.view.unit_count(), 0);
        assert!(!a.view.needs_resync());
        assert!(
            a.view
                .cells()
                .get(&(cell as u32))
                .is_none_or(|row| row[1] != "D"),
            "new start cannot retain prior match terrain edits"
        );
        a.until(|message| message["t"] == "s");
        assert!(a.view.unit(published).is_none());
        assert!(a.view.unit(victim).is_some());
    })
    .await
    .unwrap();
    let _ = stop.send(());
    server.await.unwrap().unwrap();
}

#[path = "native_ingress_socket_tests.rs"]
mod ingress;

#[path = "native_two_client_socket_tests.rs"]
mod native_two_client_socket_tests;
