//! Native behavior checks for the first fifty remaining NET rows.
//! Unsupported rows stay held in docs/testing/net-batch50.md.
use super::*;
use ww2_core::match_commands::{Command, Denial};
use ww2_core::recipient_snapshot::SnapshotCache;

struct Peer {
    connection: Connection,
    output: mpsc::Receiver<Output>,
    joined: bool,
}
impl Peer {
    fn new(id: u64) -> Self {
        let (tx, output) = mpsc::channel(256);
        let (cancel, _) = watch::channel(false);
        Self { connection: Connection { id, tx, cancel }, output, joined: false }
    }
    fn send(&mut self, hub: &mut Hub, code: &str, message: Value, now: u64) {
        hub.message(code, &self.connection, &mut self.joined, message, now);
    }
    fn hello(&mut self, hub: &mut Hub, code: &str, token: &str, name: &str, watch: bool) {
        self.send(hub, code, json!({"t":"hello","token":token,"name":name,"spectate":watch}), 0);
        assert!(self.joined);
    }
    fn messages(&mut self) -> Vec<Value> {
        let mut messages = Vec::new();
        while let Ok(output) = self.output.try_recv() {
            match output {
                Output::Json(message) => messages.push(message),
                other => panic!("expected JSON, received {other:?}"),
            }
        }
        messages
    }
    fn last_lobby(&mut self) -> Value {
        self.messages().into_iter().rfind(|m| m["t"] == "lobby").unwrap()
    }
}
fn room() -> (Hub, Peer, Peer) {
    let mut hub = Hub::new(32, "https://invite.example".into());
    let mut host = Peer::new(1);
    let mut guest = Peer::new(2);
    host.hello(&mut hub, "ABC", "host", "Host", false);
    guest.hello(&mut hub, "ABC", "guest", "Guest", false);
    host.messages();
    guest.messages();
    (hub, host, guest)
}
fn settings(hub: &Hub) -> Value {
    let room = &hub.rooms["ABC"];
    json!([room.mode, room.map, room.world_size, room.army, room.sandbox, room.weather, room.defender])
}
fn roster(hub: &Hub) -> Value {
    let room = &hub.rooms["ABC"];
    json!({"players":room.players.iter().map(|p| json!([p.name,p.team,p.faction,p.level,p.ai,p.token,p.connection.as_ref().map(|c| c.id)])).collect::<Vec<_>>(),
        "watchers":room.spectators.iter().map(|p| json!([p.name,p.token,p.connection.as_ref().map(|c| c.id)])).collect::<Vec<_>>()})
}

#[test]
fn creation_directory_capacity_and_invite_preserve_public_boundaries() {
    let mut hub = Hub::new(2, "https://invite.example".into());
    let mut host = Peer::new(1);
    host.send(&mut hub, "ABC", json!({"t":"hello","token":"private-token","name":"PrivateName","listing":{"public":true,"title":" <>&\"'\u{0} Public battle \n "}}), 0);
    let lobby = host.last_lobby();
    assert_eq!(lobby["publicUrl"], "https://invite.example");
    assert_eq!(lobby["mode"], "conquest");
    assert_eq!(lobby["mapName"], "default");
    assert_eq!(lobby["defenderTeam"], 0);
    assert_eq!(lobby["result"], Value::Null);
    assert!(hub.rooms["ABC"].native.is_none());
    let rows = hub.directory();
    assert_eq!(rows["rooms"].as_array().unwrap().len(), 1);
    let row = &rows["rooms"][0];
    assert_eq!(row["code"], "ABC");
    assert_eq!(row["title"], "Public battle");
    assert_eq!(row["humans"], 1);
    assert_eq!(row["ai"], 0);
    assert_eq!(row["occupied"], 1);
    assert_eq!(row["capacity"], hub.rooms["ABC"].capacity(&hub.maps));
    assert_eq!(row["joinable"], true);
    assert!(row.get("worldSize").is_none());
    assert_eq!(row.as_object().unwrap().len(), 10);
    assert!(!rows.to_string().contains("private-token"));
    assert!(!rows.to_string().contains("PrivateName"));
    host.send(&mut hub, "ABC", json!({"t":"mode","v":"world"}), 1);
    host.send(&mut hub, "ABC", json!({"t":"worldSize","size":"massive"}), 2);
    host.send(&mut hub, "ABC", json!({"t":"addAi"}), 3);
    let world = hub.directory();
    assert_eq!(world["rooms"][0]["map"], "world");
    assert_eq!(world["rooms"][0]["worldSize"], "massive");
    assert_eq!(world["rooms"][0]["ai"], 1);
    let mut hidden = Peer::new(2);
    hidden.hello(&mut hub, "DEF", "hidden", "Hidden", false);
    assert_eq!(hub.directory()["rooms"].as_array().unwrap().len(), 1);
    let mut excess = Peer::new(3);
    excess.send(&mut hub, "GHI", json!({"t":"hello","token":"excess","name":"Excess"}), 4);
    assert!(!excess.joined);
    assert!(matches!(excess.output.try_recv(), Ok(Output::Json(v)) if v == json!({"t":"full","reason":"capacity"})));
    assert!(matches!(excess.output.try_recv(), Ok(Output::Close(1000, ""))));
    let mut existing = Peer::new(4);
    existing.hello(&mut hub, "DEF", "existing", "Existing", false);
    assert_eq!(hub.rooms.len(), 2);
    hub.disconnected("ABC", host.connection.id, 5);
    assert!(hub.directory()["rooms"].as_array().unwrap().is_empty());
    // AI seats and watchers alone never make a room discoverable.
    let mut watcher = Peer::new(5);
    watcher.hello(&mut hub, "ABC", "watcher", "Watcher", true);
    assert!(hub.directory()["rooms"].as_array().unwrap().is_empty());
}

#[test]
fn listing_defaults_sanitizing_and_creation_only_fields() {
    for (index, title, expected) in [
        (0, json!(false), "Open skirmish".to_owned()),
        (1, json!(" <>&\"'\t\n "), "Open skirmish".to_owned()),
        (2, json!("x".repeat(60)), "x".repeat(48)),
    ] {
        let mut hub = Hub::new(32, String::new());
        let mut creator = Peer::new(1);
        creator.send(&mut hub, "ABC", json!({"t":"hello","token":"a","name":"A","listing":{"public":true,"title":title}}), 0);
        assert_eq!(hub.rooms["ABC"].title, expected, "title case {index}");
        let mut second = Peer::new(2);
        second.send(&mut hub, "ABC", json!({"t":"hello","token":"b","name":"B","listing":{"public":false,"title":"Replacement","tutorial":true}}), 0);
        assert_eq!(hub.rooms["ABC"].title, expected);
        assert!(hub.rooms["ABC"].listed);
        assert_eq!(hub.rooms["ABC"].mode, "conquest");
        assert_eq!(hub.rooms["ABC"].map, "default");
    }
}

#[test]
fn hello_rename_and_stale_membership_use_current_people() {
    let mut hub = Hub::new(32, String::new());
    let mut host = Peer::new(1);
    for invalid in [json!(null), json!([]), json!({}), json!({"t":9}), json!({"t":"name","name":"Before"}), json!({"t":"hello","name":"A","token":false})] {
        host.send(&mut hub, "ABC", invalid, 0);
        assert!(!host.joined && hub.rooms.is_empty());
        assert!(host.messages().is_empty());
    }
    host.hello(&mut hub, "ABC", &"t".repeat(50), "  <>&\"' Alice \u{feff}", false);
    assert_eq!(hub.rooms["ABC"].players[0].token, vec![u16::from(b't'); 40]);
    assert_eq!(hub.rooms["ABC"].players[0].name, "Alice");
    let mut guest = Peer::new(2);
    guest.hello(&mut hub, "ABC", "guest", "Guest", false);
    host.messages();
    guest.messages();
    host.send(&mut hub, "ABC", json!({"t":"name","name":" <>&\"' "}), 1);
    assert_eq!(host.last_lobby()["players"][0]["name"], "Soldier");
    assert_eq!(guest.last_lobby()["players"][0]["name"], "Soldier");
    for name in [json!(null), json!(7), json!([]), json!({}), json!(false)] {
        host.send(&mut hub, "ABC", json!({"t":"name","name":name}), 2);
        assert!(host.messages().is_empty() && guest.messages().is_empty());
        assert_eq!(hub.rooms["ABC"].players[0].name, "Soldier");
    }
    let mut replacement = Peer::new(3);
    replacement.hello(&mut hub, "ABC", &"t".repeat(40), "Current", false);
    assert!(matches!(host.output.try_recv(), Ok(Output::Json(v)) if v == json!({"t":"replaced"})));
    assert!(matches!(host.output.try_recv(), Ok(Output::Close(1000, ""))));
    replacement.messages();
    guest.messages();
    let before = roster(&hub);
    let before_settings = settings(&hub);
    host.send(&mut hub, "ABC", json!({"t":"name","name":"Stale"}), 3);
    host.send(&mut hub, "ABC", json!({"t":"kick","slot":1}), 3);
    host.send(&mut hub, "ABC", json!({"t":"mode","v":"world"}), 3);
    host.send(&mut hub, "ABC", json!({"t":"map","name":"default"}), 3);
    hub.disconnected("ABC", host.connection.id, 3);
    assert_eq!(roster(&hub), before);
    assert_eq!(settings(&hub), before_settings);
    assert!(replacement.messages().is_empty() && guest.messages().is_empty());
    hub.disconnected("ABC", guest.connection.id, 4);
    replacement.messages();
    replacement.send(&mut hub, "ABC", json!({"t":"kick","slot":1}), 5);
    replacement.messages();
    guest.messages();
    let before = roster(&hub);
    guest.send(&mut hub, "ABC", json!({"t":"name","name":"Freed"}), 6);
    guest.send(&mut hub, "ABC", json!({"t":"sit"}), 6);
    assert_eq!(roster(&hub), before);
    assert!(guest.messages().is_empty() && replacement.messages().is_empty());
}

#[test]
fn host_settings_validate_all_choices_and_serialize_replacement_requests() {
    let (mut hub, mut host, mut guest) = room();
    for message in [json!({"t":"mode","v":"world"}), json!({"t":"army","v":"large"}), json!({"t":"sandbox","v":true}), json!({"t":"weather","v":"snow"}), json!({"t":"defender","v":4})] {
        let before = settings(&hub);
        guest.send(&mut hub, "ABC", message, 0);
        assert_eq!(settings(&hub), before);
    }
    for mode in ["conquest", "assault", "annihilation", "classic", "horde", "tutorial", "world"] {
        host.send(&mut hub, "ABC", json!({"t":"mode","v":mode}), 0);
        assert_eq!(hub.rooms["ABC"].mode, mode);
        if mode == "tutorial" {
            assert_eq!(hub.rooms["ABC"].map, "tutorial");
            host.send(&mut hub, "ABC", json!({"t":"map","name":"default"}), 0);
            assert_eq!(hub.rooms["ABC"].map, "tutorial");
        }
    }
    for (field, values) in [
        ("worldSize", vec!["huge", "massive"]),
        ("army", vec!["standard", "large", "massive", "endless"]),
        ("weather", vec!["map", "clear", "fog", "rain", "mud", "snow", "random"]),
    ] {
        for value in values {
            host.send(&mut hub, "ABC", json!({"t":field,"v":value}), 0);
            let actual = match field { "worldSize" => &hub.rooms["ABC"].world_size, "army" => &hub.rooms["ABC"].army, _ => &hub.rooms["ABC"].weather };
            assert_eq!(actual, value);
        }
        let before = settings(&hub);
        host.send(&mut hub, "ABC", json!({"t":field,"v":"invalid"}), 0);
        assert_eq!(settings(&hub), before);
    }
    host.send(&mut hub, "ABC", json!({"t":"mode","v":"horde"}), 0);
    assert_eq!(hub.rooms["ABC"].army, "standard");
    assert_eq!(hub.maps[&hub.rooms["ABC"].map]["horde"], true);
    host.send(&mut hub, "ABC", json!({"t":"army","v":"endless"}), 0);
    assert_eq!(hub.rooms["ABC"].army, "standard");
    let catalog = hub.maps.as_object().unwrap().clone();
    for (name, entry) in catalog {
        let before = hub.rooms["ABC"].map.clone();
        host.send(&mut hub, "ABC", json!({"t":"map","name":name}), 0);
        if entry["horde"] == true {
            assert_eq!(hub.rooms["ABC"].map, name);
        } else {
            assert_eq!(hub.rooms["ABC"].map, before);
        }
    }
    let before = settings(&hub);
    host.send(&mut hub, "ABC", json!({"t":"map","name":"not-present"}), 0);
    assert_eq!(settings(&hub), before);
    for value in [json!(false), json!(0), json!(""), Value::Null] {
        host.send(&mut hub, "ABC", json!({"t":"sandbox","v":value}), 0);
        assert!(!hub.rooms["ABC"].sandbox);
    }
    for value in [json!(true), json!(1), json!("yes"), json!([]), json!({})] {
        host.send(&mut hub, "ABC", json!({"t":"sandbox","v":value}), 0);
        assert!(hub.rooms["ABC"].sandbox);
    }
    for value in 0..6 {
        host.send(&mut hub, "ABC", json!({"t":"defender","v":value}), 0);
        assert_eq!(hub.rooms["ABC"].defender, value);
    }
    let before = settings(&hub);
    for value in [json!(-1), json!(6), json!(1.5), json!("2"), json!([])] {
        host.send(&mut hub, "ABC", json!({"t":"defender","v":value}), 0);
        assert_eq!(settings(&hub), before);
    }
    // Native catalogue selection is synchronous under the Hub lock: later requests win.
    host.send(&mut hub, "ABC", json!({"t":"mode","v":"world"}), 0);
    host.send(&mut hub, "ABC", json!({"t":"mode","v":"conquest"}), 0);
    assert_eq!(hub.rooms["ABC"].mode, "conquest");
    host.send(&mut hub, "ABC", json!({"t":"map","name":"default"}), 0);
    assert_eq!(hub.rooms["ABC"].map, "default");
}

#[test]
fn ai_level_kick_team_and_faction_enforce_authority_and_integer_types() {
    let (mut hub, mut host, mut guest) = room();
    host.send(&mut hub, "ABC", json!({"t":"addAi"}), 0);
    for value in ["easy", "normal", "hard"] {
        host.send(&mut hub, "ABC", json!({"t":"level","slot":2,"v":value}), 0);
        assert_eq!(hub.rooms["ABC"].players[2].level, value);
    }
    for message in [json!({"t":"level","slot":0,"v":"easy"}), json!({"t":"level","slot":"2","v":"easy"}), json!({"t":"level","slot":2,"v":"impossible"})] {
        let before = roster(&hub);
        host.send(&mut hub, "ABC", message, 0);
        assert_eq!(roster(&hub), before);
    }
    let before = roster(&hub);
    guest.send(&mut hub, "ABC", json!({"t":"level","slot":2,"v":"easy"}), 0);
    guest.send(&mut hub, "ABC", json!({"t":"kick","slot":2}), 0);
    host.send(&mut hub, "ABC", json!({"t":"kick","slot":1}), 0);
    assert_eq!(roster(&hub), before);
    for value in 0..6 {
        host.send(&mut hub, "ABC", json!({"t":"team","slot":1,"v":value}), 0);
        assert_eq!(hub.rooms["ABC"].players[1].team, value);
    }
    for value in 0..4 {
        guest.send(&mut hub, "ABC", json!({"t":"faction","slot":1,"v":value}), 0);
        host.send(&mut hub, "ABC", json!({"t":"faction","slot":2,"v":value}), 0);
        host.send(&mut hub, "ABC", json!({"t":"faction","slot":0,"v":value}), 0);
        assert_eq!(hub.rooms["ABC"].players[1].faction, value);
    }
    let before = roster(&hub);
    for message in [json!({"t":"team","slot":0,"v":2}), json!({"t":"faction","slot":0,"v":2})] {
        guest.send(&mut hub, "ABC", message, 0);
        assert_eq!(roster(&hub), before);
    }
    host.send(&mut hub, "ABC", json!({"t":"faction","slot":1,"v":0}), 0);
    assert_eq!(roster(&hub), before);
    for field in ["team", "faction"] {
        for bad in [json!("1"), json!(1.5), json!([]), json!(-1), json!(99)] {
            host.send(&mut hub, "ABC", json!({"t":field,"slot":1,"v":bad}), 0);
            host.send(&mut hub, "ABC", json!({"t":field,"slot":bad,"v":0}), 0);
            assert_eq!(roster(&hub), before);
        }
    }
    for slot in [json!("2"), json!(2.5), json!(-1), json!([]), json!(99)] {
        host.send(&mut hub, "ABC", json!({"t":"kick","slot":slot}), 0);
        assert_eq!(roster(&hub), before);
    }
    host.send(&mut hub, "ABC", json!({"t":"kick","slot":2}), 0);
    assert_eq!(hub.rooms["ABC"].players.len(), 2);
    hub.disconnected("ABC", guest.connection.id, 1);
    host.send(&mut hub, "ABC", json!({"t":"kick","slot":1}), 2);
    assert_eq!(hub.rooms["ABC"].players.len(), 1);
}

#[test]
fn watch_and_sit_obey_capacity_and_assign_fresh_seat_defaults() {
    let (mut hub, mut host, mut guest) = room();
    host.send(&mut hub, "ABC", json!({"t":"team","slot":1,"v":5}), 0);
    guest.send(&mut hub, "ABC", json!({"t":"faction","slot":1,"v":3}), 0);
    guest.send(&mut hub, "ABC", json!({"t":"spectate"}), 0);
    assert_eq!(hub.rooms["ABC"].players.len(), 1);
    assert_eq!(hub.rooms["ABC"].spectators.len(), 1);
    guest.send(&mut hub, "ABC", json!({"t":"sit"}), 0);
    assert_eq!(hub.rooms["ABC"].players[1].team, 1);
    assert_eq!(hub.rooms["ABC"].players[1].faction, 1);
    assert!(hub.rooms["ABC"].spectators.is_empty());
    let mut watchers = Vec::new();
    for id in 3..11 {
        let mut watcher = Peer::new(id);
        watcher.hello(&mut hub, "ABC", &format!("w{id}"), "Watcher", true);
        watchers.push(watcher);
    }
    assert_eq!(hub.rooms["ABC"].spectators.len(), 8);
    guest.send(&mut hub, "ABC", json!({"t":"spectate"}), 0);
    assert_eq!(hub.rooms["ABC"].players.len(), 2);
    assert_eq!(hub.rooms["ABC"].spectators.len(), 8);
    for _ in 0..4 {
        host.send(&mut hub, "ABC", json!({"t":"addAi"}), 0);
    }
    assert_eq!(hub.rooms["ABC"].players.len(), 6);
    watchers[0].send(&mut hub, "ABC", json!({"t":"sit"}), 0);
    assert_eq!(hub.rooms["ABC"].players.len(), 6);
    host.send(&mut hub, "ABC", json!({"t":"kick","slot":5}), 0);
    watchers[0].send(&mut hub, "ABC", json!({"t":"sit"}), 0);
    assert_eq!(hub.rooms["ABC"].players.len(), 6);
    assert_eq!(hub.rooms["ABC"].spectators.len(), 7);
    assert_eq!(hub.rooms["ABC"].players[5].team, 5);
    assert_eq!(hub.rooms["ABC"].players[5].faction, 2);
    let mut listed = Hub::new(32, String::new());
    let mut leader = Peer::new(20);
    leader.send(&mut listed, "DEF", json!({"t":"hello","token":"leader","name":"Leader","listing":{"public":true}}), 0);
    let mut second = Peer::new(21);
    second.hello(&mut listed, "DEF", "second", "Second", false);
    let mut third = Peer::new(22);
    third.hello(&mut listed, "DEF", "third", "Third", false);
    assert_eq!(listed.rooms["DEF"].capacity(&listed.maps), 3);
    let mut overflow = Peer::new(23);
    overflow.hello(&mut listed, "DEF", "overflow", "Overflow", false);
    assert_eq!(listed.rooms["DEF"].players.len(), 3);
    assert_eq!(listed.rooms["DEF"].spectators.len(), 1);
    overflow.send(&mut listed, "DEF", json!({"t":"sit"}), 0);
    assert_eq!(listed.rooms["DEF"].players.len(), 3);
    second.send(&mut listed, "DEF", json!({"t":"spectate"}), 0);
    overflow.send(&mut listed, "DEF", json!({"t":"sit"}), 0);
    assert_eq!(listed.rooms["DEF"].players[2].name, "Overflow");
    assert_eq!(listed.rooms["DEF"].players[2].team, 1);
    assert_eq!(listed.rooms["DEF"].players[2].faction, 2);
}

#[test]
fn attached_match_blocks_lobby_mutations_and_preserves_command_window_on_rejoin() {
    let (mut hub, mut host, mut guest) = room();
    hub.attach_native("ABC", native_tests::native_match(), 0).unwrap();
    host.messages();
    guest.messages();
    let before = (settings(&hub), roster(&hub));
    for message in [json!({"t":"level","slot":0,"v":"hard"}), json!({"t":"kick","slot":1}), json!({"t":"team","slot":0,"v":5}), json!({"t":"faction","slot":0,"v":3}), json!({"t":"map","name":"default"}), json!({"t":"mode","v":"world"}), json!({"t":"worldSize","v":"massive"}), json!({"t":"army","v":"large"}), json!({"t":"sandbox","v":true}), json!({"t":"weather","v":"snow"}), json!({"t":"defender","v":3}), json!({"t":"spectate"}), json!({"t":"sit"})] {
        host.send(&mut hub, "ABC", message, 1);
        assert_eq!((settings(&hub), roster(&hub)), before);
    }
    host.messages();
    // Start a distinct window after the lobby-control refusals.
    for index in 0..6 {
        guest.send(&mut hub, "ABC", json!({"t":"move","orders":[[1,2,"bad"]]}), 2000 + index);
    }
    let denials = guest.messages();
    assert_eq!(denials.len(), 4);
    assert!(denials.iter().all(|v| *v == json!({"t":"deny","cmd":"move","reason":"blocked"})));
    let budget = hub.rooms["ABC"].players[1].denies.clone();
    for index in 0..8 {
        guest.send(&mut hub, "ABC", json!({"t":"ping","c":index}), 2006 + index);
        assert_eq!(guest.messages(), vec![json!({"t":"pong","c":index})]);
    }
    assert_eq!(hub.rooms["ABC"].players[1].denies, budget);
    let mut returned = Peer::new(3);
    returned.hello(&mut hub, "ABC", "guest", "Returned", false);
    let kinds = returned.messages().iter().map(|v| v["t"].as_str().unwrap().to_owned()).collect::<Vec<_>>();
    assert_eq!(kinds, ["start", "lobby"]);
    assert_eq!(hub.rooms["ABC"].players[1].denies, budget);
    returned.send(&mut hub, "ABC", json!({"t":"move","orders":[[1,2,"bad"]]}), 2999);
    assert!(returned.messages().is_empty());
    returned.send(&mut hub, "ABC", json!({"t":"move","orders":[[1,2,"bad"]]}), 3000);
    assert_eq!(returned.messages(), vec![json!({"t":"deny","cmd":"move","reason":"blocked"})]);
}

#[test]
fn command_preamble_blocks_finished_out_away_and_whole_malformed_orders() {
    let mut state = native_tests::native_match();
    let id = state.world.game.units.values().find(|u| u.owner == 0).unwrap().id;
    let stop = Command::from_json(&json!({"t":"stop","ids":[id]})).unwrap();
    assert_eq!(state.execute_command(0, &stop, true).unwrap(), Some(Denial::Blocked));
    assert_eq!(state.execute_command(99, &stop, false).unwrap(), Some(Denial::Blocked));
    state.start.economy.players[0].out = true;
    assert_eq!(state.execute_command(0, &stop, false).unwrap(), Some(Denial::Blocked));
    state.start.economy.players[0].out = false;
    state.start.economy.players[0].away = true;
    assert_eq!(state.execute_command(0, &stop, false).unwrap(), Some(Denial::Blocked));
    state.start.economy.players[0].away = false;
    let before = state.commands.units.clone();
    for value in [json!(null), json!([]), json!(2), json!({"t":"move","orders":[[id,1,2],[id,3,"bad"]]}), json!({"t":"move","orders":{}})] {
        assert!(matches!(Command::from_json(&value), Err(Denial::Blocked)));
        assert_eq!(state.commands.units, before);
    }
    assert_eq!(state.execute_command(0, &stop, false).unwrap(), None);
}

#[test]
fn recipient_rows_hide_other_owners_cooldowns_stances_and_unseen_target_ids() {
    let mut state = native_tests::native_match();
    let own = state.world.game.units.values().find(|u| u.owner == 0 && !state.rules.kinds[u.kind].structure).unwrap().id;
    let enemy = state.world.game.units.values().find(|u| u.owner == 1 && !state.rules.kinds[u.kind].structure).unwrap().id;
    let hidden = state.world.game.units.values().find(|u| u.owner == 1 && u.id != enemy).unwrap().id;
    state.world.game.players[0].visible = vec![enemy];
    for id in [own, enemy] {
        state.abilities.initialize(id).cooldown = 3.2;
        state.commands.units.get_mut(&id).unwrap().hold_fire = true;
        state.commands.units.get_mut(&id).unwrap().auto_cast = true;
        state.commands.units.get_mut(&id).unwrap().auto_retreat = true;
        state.world.game.units.get_mut(&id).unwrap().hold_pos = true;
        state.firing.initialize(id).target_id = hidden;
    }
    let cache = SnapshotCache::new(&state).unwrap();
    let fields = cache.recipient(0).unwrap().fields;
    let rows = fields["units"].as_array().unwrap();
    assert!(!rows.iter().any(|r| r[0] == hidden));
    let mine = rows.iter().find(|r| r[0] == own).unwrap();
    let theirs = rows.iter().find(|r| r[0] == enemy).unwrap();
    assert_eq!(mine[11].as_f64(), Some(4.0));
    assert_eq!(theirs[11].as_f64(), Some(0.0));
    assert_eq!(mine[9], 0);
    assert_eq!(theirs[9], 0);
    let private = 2048 | 4096 | 8192 | 16384;
    assert_eq!(mine[12].as_u64().unwrap() & private, private);
    assert_eq!(theirs[12].as_u64().unwrap() & private, 0);
    let public_mask = 2047 | 32768 | (7 << 20) | 262144;
    assert_eq!(theirs[12].as_u64().unwrap() & !public_mask, 0);
    drop(cache);
    state.world.game.players[0].visible = vec![enemy, hidden];
    state.world.game.players[0].visible.sort_unstable();
    let fields = SnapshotCache::new(&state).unwrap().recipient(0).unwrap().fields;
    assert_eq!(fields["units"].as_array().unwrap().iter().find(|r| r[0] == enemy).unwrap()[9], hidden);
    state.world.game.reveal = true;
    let revealed = SnapshotCache::new(&state).unwrap().recipient(0).unwrap().fields;
    assert_eq!(revealed["units"].as_array().unwrap().len(), state.world.game.units.len());
}

#[test]
fn trusted_lifecycle_publishes_start_before_snapshots_and_rejoins_same_match() {
    use ww2_core::client_view::{Applied, ClientView};
    let (mut hub, mut host, mut guest) = room();
    hub.configure_trusted_room(Some(crate::TrustedRoomConfig::new("ABC", 3936341).unwrap()));
    guest.send(&mut hub, "ABC", json!({"t":"start"}), 0);
    assert!(hub.rooms["ABC"].native.is_none());
    assert_eq!(guest.messages(), vec![json!({"t":"deny","cmd":"start","reason":"scenario"})]);
    host.send(&mut hub, "ABC", json!({"t":"start"}), 1);
    let messages = host.messages();
    assert_eq!(messages.iter().map(|m| m["t"].as_str().unwrap()).collect::<Vec<_>>(), ["pause", "lobby", "start"]);
    let start = &messages[2];
    assert_eq!(start["matchId"], 1);
    assert_eq!(start["you"], 0);
    assert_eq!(start["names"], json!(["Host", "Guest"]));
    assert_eq!(start["teams"], json!([0, 1]));
    assert_eq!(start["factions"], json!([0, 1]));
    assert_eq!(start["spawn"], start["spawns"][0]);
    assert!(start["cells"].is_array());
    assert!(start["fog"]["v"].is_string() && start["fog"]["e"].is_string());
    assert!(start["map"].get("scenario").is_none());
    let mut view = ClientView::default();
    assert_eq!(view.apply(&start.to_string()).unwrap(), Applied::Start);
    let other = guest.messages();
    assert_eq!(other[2]["you"], 1);
    assert_eq!(other[2]["matchId"], 1);
    hub.disconnected("ABC", guest.connection.id, 50);
    assert!(hub.native_status("ABC").unwrap().paused);
    host.messages();
    let mut returned = Peer::new(3);
    returned.send(&mut hub, "ABC", json!({"t":"hello","token":"guest","name":"Returned"}), 60);
    assert!(returned.joined);
    let messages = returned.messages();
    assert_eq!(messages.iter().map(|m| m["t"].as_str().unwrap()).collect::<Vec<_>>(), ["start", "pause", "pause", "lobby"]);
    assert_eq!(messages[0]["matchId"], 1);
    assert_eq!(messages[0]["you"], 1);
    assert_eq!(messages[1]["paused"], true);
    assert_eq!(messages[2]["paused"], false);
    assert!(!hub.native_status("ABC").unwrap().paused);
    host.messages();
    host.send(&mut hub, "ABC", json!({"t":"restart"}), 70);
    let messages = host.messages();
    assert_eq!(messages.iter().map(|m| m["t"].as_str().unwrap()).collect::<Vec<_>>(), ["pause", "lobby", "start"]);
    assert_eq!(messages[2]["matchId"], 2);
    assert_eq!(hub.native_status("ABC").unwrap().tick, 0);
    assert!(!hub.native_status("ABC").unwrap().paused);
}

#[test]
fn away_seat_stops_ordinary_wallet_income_and_cannot_command() {
    let state = native_tests::native_match();
    let mut runtime = RoomRuntime::new(state, 0).unwrap();
    let balances = runtime.state().start.economy.players.iter().map(|p| (p.mp, p.mun, p.vp)).collect::<Vec<_>>();
    for _ in 0..20 {
        runtime.advance_tick(&[true, false]).unwrap();
    }
    let economy = &runtime.state().start.economy;
    assert!(economy.players[0].away);
    assert!(!economy.players[1].away);
    assert_eq!((economy.players[0].mp, economy.players[0].mun, economy.players[0].vp), balances[0]);
    assert!(economy.players[1].mp > balances[1].0, "a connected control seat actually earns income");
    let stop = Command::from_json(&json!({"t":"stop","ids":[]})).unwrap();
    assert_eq!(runtime.state_mut().execute_command(0, &stop, false).unwrap(), Some(Denial::Blocked));
}

#[test]
fn unimplemented_roles_and_public_lifecycle_remain_explicit_refusals() {
    let (mut hub, mut host, _guest) = room();
    let mut operative = Peer::new(3);
    operative.send(&mut hub, "ABC", json!({"t":"hello","token":"operative","name":"Operative","role":"operative","commander":0}), 0);
    assert!(!operative.joined);
    assert!(matches!(operative.output.try_recv(), Ok(Output::Json(v)) if v == json!({"t":"deny","cmd":"join","reason":"commander"})));
    assert!(matches!(operative.output.try_recv(), Ok(Output::Close(1008, "operative unavailable"))));
    for (index, name) in ["start", "restart", "end", "leave", "handAi", "nextwave"].iter().enumerate() {
        host.send(&mut hub, "ABC", json!({"t":name}), (index as u64 + 1) * 1001);
        let messages = host.messages();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0]["t"], "deny");
        assert_eq!(messages[0]["cmd"], *name);
        assert!(hub.rooms["ABC"].native.is_none());
    }
}

#[test]
fn recipient_wallets_support_and_unit_lists_stay_with_the_owning_seat() {
    let mut state = native_tests::native_match();
    for slot in 0..2 {
        let player = &mut state.start.economy.players[slot];
        player.mp = 101.0 + slot as f64 * 100.0;
        player.mun = 11.0 + slot as f64 * 10.0;
        player.fuel = 21.0 + slot as f64 * 10.0;
        player.inc = 3.0 + slot as f64;
        player.fuel_inc = Some(5.0 + slot as f64);
        player.upkeep = Some(7.0 + slot as f64);
        for value in state.start.players[slot].support.values_mut() {
            *value = 13.0 + slot as f64;
        }
    }
    let cache = SnapshotCache::new(&state).unwrap();
    for slot in 0..2 {
        let fields = cache.recipient(slot).unwrap().fields;
        assert_eq!(fields["mp"].as_f64(), Some(101.0 + slot as f64 * 100.0));
        // Ordinary Conquest has no mun/fuel wire fields, despite internal wallets.
        assert!(fields.get("mun").is_none() && fields.get("fuel").is_none());
        assert_eq!(fields["inc"], 3.0 + slot as f64);
        assert_eq!(fields["fuelInc"], 5.0 + slot as f64);
        assert_eq!(fields["upkeep"], 7.0 + slot as f64);
        assert!(fields["sup"].as_object().unwrap().values().all(|v| v.as_f64() == Some(13.0 + slot as f64)));
        for name in ["plans", "orders", "queues", "productionJobs", "movement"] {
            let rows = fields[name].as_array().unwrap();
            if name == "plans" {
                assert!(!rows.is_empty(), "positive owner-list control");
            }
            for row in rows {
                let id = row[0].as_u64().unwrap() as u32;
                assert_eq!(state.world.game.units[&id].owner, slot, "{name}");
            }
        }
    }
}

#[test]
fn terrain_and_fog_start_baselines_do_not_repeat_in_unchanged_snapshots() {
    use ww2_core::recipient_snapshot::DeliveryState;
    let mut state = native_tests::native_match();
    let mut delivery = DeliveryState::new(&state).unwrap();
    let mut baseline = Vec::new();
    {
        let frame = delivery.capture(&mut state, &[true, true], &[false, false]).unwrap();
        for slot in 0..2 {
            let fields = frame.recipient(slot).unwrap().fields;
            assert!(fields["fog"]["v"].is_string());
            assert!(fields["fog"]["e"].is_string());
            assert!(!fields["cells"].as_array().unwrap().is_empty());
            baseline.push(fields["cells"].clone());
        }
    }
    {
        // A full start read preserves the outbox. The first delta drains it.
        let frame = delivery.capture(&mut state, &[false, false], &[false, false]).unwrap();
        for (slot, expected) in baseline.iter().enumerate() {
            let fields = frame.recipient(slot).unwrap().fields;
            assert!(fields.get("fog").is_none());
            assert_eq!(&fields["cells"], expected);
        }
    }
    let frame = delivery.capture(&mut state, &[false, false], &[false, false]).unwrap();
    for slot in 0..2 {
        let fields = frame.recipient(slot).unwrap().fields;
        assert!(fields.get("fog").is_none());
        assert!(fields["cells"].as_array().unwrap().is_empty());
    }
}

#[test]
fn directory_marks_playing_rooms_unjoinable_without_publishing_personal_fields() {
    let mut hub = Hub::new(32, String::new());
    let mut host = Peer::new(1);
    host.send(&mut hub, "ABC", json!({"t":"hello","token":"private-a","name":"Private A","listing":{"public":true}}), 0);
    let mut guest = Peer::new(2);
    guest.hello(&mut hub, "ABC", "private-b", "Private B", false);
    hub.attach_native("ABC", native_tests::native_match(), 0).unwrap();
    let rows = hub.directory();
    let row = &rows["rooms"][0];
    assert_eq!(row["state"], "play");
    assert_eq!(row["joinable"], false);
    assert_eq!(row["humans"], 2);
    assert_eq!(row["ai"], 0);
    assert_eq!(row["occupied"], 2);
    assert_eq!(row.as_object().unwrap().len(), 10);
    assert!(!rows.to_string().contains("private-a"));
    assert!(!rows.to_string().contains("Private A"));
}
