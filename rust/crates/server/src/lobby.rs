use serde_json::{Value, json};
use std::collections::BTreeMap;
use tokio::sync::{mpsc, watch};

const MAX_PLAYERS: usize = 6;
const MAX_SPECTATORS: usize = 8;

#[derive(Clone)]
pub(crate) struct Connection {
    pub id: u64,
    pub tx: mpsc::Sender<Output>,
    pub cancel: watch::Sender<bool>,
}

#[derive(Debug)]
pub(crate) enum Output {
    Json(Value),
    Close(u16, &'static str),
}

impl Connection {
    pub fn send(&self, msg: Value) {
        self.output(Output::Json(msg));
    }
    pub fn close(&self, code: u16, reason: &'static str) {
        self.output(Output::Close(code, reason));
    }
    fn output(&self, msg: Output) {
        if self.tx.try_send(msg).is_err() {
            // A stalled client cannot make every other room wait or grow an unbounded queue.
            let _ = self.cancel.send(true);
        }
    }
}

struct Person {
    token: Vec<u16>,
    name: String,
    connection: Option<Connection>,
    ai: bool,
    team: usize,
    faction: usize,
    level: &'static str,
    disconnected: Option<u64>,
    denies: Vec<u64>,
}

impl Person {
    fn deny(&mut self, command: &str, reason: &str, now: u64) {
        self.denies.retain(|at| now.saturating_sub(*at) < 1_000);
        if self.denies.len() < 4 {
            self.denies.push(now);
            if let Some(c) = &self.connection {
                c.send(json!({"t":"deny","cmd":command,"reason":reason}));
            }
        }
    }
}

struct Room {
    code: String,
    listed: bool,
    title: String,
    players: Vec<Person>,
    spectators: Vec<Person>,
    mode: String,
    map: String,
    world_size: String,
    army: String,
    weather: String,
    sandbox: bool,
    defender: usize,
    empty_since: Option<u64>,
}

impl Room {
    fn people(&self) -> impl Iterator<Item = &Person> {
        self.players.iter().chain(self.spectators.iter())
    }
    fn host(&self) -> Option<usize> {
        self.players
            .iter()
            .position(|p| !p.ai && p.connection.is_some())
            .or_else(|| self.players.iter().position(|p| !p.ai))
    }
    fn is_host(&self, id: u64) -> bool {
        let person = self
            .host()
            .map(|i| &self.players[i])
            .or_else(|| self.spectators.iter().find(|p| p.connection.is_some()));
        person.is_some_and(|p| p.connection.as_ref().is_some_and(|c| c.id == id))
    }
    fn capacity(&self, maps: &Value) -> usize {
        match self.mode.as_str() {
            "world" => MAX_PLAYERS,
            "horde" | "tutorial" => MAX_PLAYERS - 1,
            mode => maps[&self.map][if mode == "assault" {
                "assault"
            } else {
                "normal"
            }]
            .as_u64()
            .unwrap_or(0) as usize,
        }
    }
    fn has_seat(&self, maps: &Value) -> bool {
        self.players.len() < MAX_PLAYERS
            && (!self.listed || self.players.len() < self.capacity(maps))
    }
    fn add_seat(&mut self, mut p: Person) {
        p.team = (0..MAX_PLAYERS)
            .find(|team| !self.players.iter().any(|p| p.team == *team))
            .unwrap_or(0);
        p.faction = self.players.len() % 3;
        self.players.push(p);
    }
    fn broadcast(&self, maps: &Value, public_url: &str) {
        for p in self.people() {
            if let Some(c) = &p.connection {
                c.send(self.lobby(c.id, maps, public_url));
            }
        }
    }
    fn lobby(&self, id: u64, maps: &Value, public_url: &str) -> Value {
        let you = self
            .players
            .iter()
            .position(|p| p.connection.as_ref().is_some_and(|c| c.id == id));
        let mut result = json!({
            "t":"lobby", "proto":0, "code": self.code, "state":"lobby",
            "role":"commander", "operatives":[], "spectator":you.is_none(),
            "amHost":self.is_host(id), "spectators":self.spectators.iter().map(|p| &p.name).collect::<Vec<_>>(),
            "listed":self.listed, "saves":[], "hordeBest":null,
            "you":you.map_or(-1, |i| i as i64), "host":self.host().map_or(-1, |i| i as i64),
            "maps":maps.as_object().unwrap().keys().collect::<Vec<_>>(),
            "mapName":if self.mode == "world" { "world" } else { &self.map },
            "spawns":self.capacity(maps), "publicUrl":public_url,
            "mode":self.mode,"worldSize":self.world_size,"defenderTeam":self.defender,
            "army":self.army,"weather":self.weather,"sandbox":self.sandbox,"result":null,
            "players":self.players.iter().map(|p| json!({
                "name":p.name,"connected":p.connection.is_some() || p.ai,"ai":p.ai,
                "team":p.team,"faction":p.faction,"level":if p.ai { json!(p.level) } else { Value::Null }
            })).collect::<Vec<_>>()
        });
        if self.mode == "horde" {
            result["hordeMaps"] = json!(
                maps.as_object()
                    .unwrap()
                    .iter()
                    .filter(|(_, m)| m["horde"] == true)
                    .map(|(name, _)| name)
                    .collect::<Vec<_>>()
            );
        }
        result
    }
    fn directory(&self, maps: &Value) -> Value {
        let mut v = json!({
            "code":self.code,"title":self.title,"mode":self.mode,
            "map":if self.mode == "world" { "world" } else { &self.map },"state":"lobby",
            "humans":self.players.iter().filter(|p| !p.ai && p.connection.is_some()).count(),
            "ai":self.players.iter().filter(|p| p.ai).count(),"occupied":self.players.len(),
            "capacity":self.capacity(maps),"joinable":self.players.len() < self.capacity(maps)
        });
        if self.mode == "world" {
            v["worldSize"] = json!(self.world_size);
        }
        v
    }
    fn command(&mut self, id: u64, msg: &Value, maps: &Value, now: u64) -> bool {
        let Some(t) = msg["t"].as_str() else {
            return false;
        };
        let seated = self
            .players
            .iter()
            .position(|p| p.connection.as_ref().is_some_and(|c| c.id == id));
        let watching = self
            .spectators
            .iter()
            .position(|p| p.connection.as_ref().is_some_and(|c| c.id == id));
        if seated.is_none() && watching.is_none() {
            return false;
        }
        let host = self.is_host(id);
        let me = if let Some(i) = seated {
            &mut self.players[i]
        } else {
            &mut self.spectators[watching.unwrap()]
        };
        if t == "ping" {
            if msg.get("x").is_none() && msg.get("z").is_none() {
                let mut pong = json!({"t":"pong"});
                if let Some(c) = msg.get("c") {
                    pong["c"] = c.clone();
                }
                me.connection.as_ref().unwrap().send(pong);
            }
            return false;
        }
        if t == "name" {
            if let Some(name) = msg["name"].as_str() {
                me.name = clean(name, 16, "Soldier", false);
                return true;
            }
            return false;
        }
        // This slice never enters play. Known, unavailable commands fail explicitly.
        if UNAVAILABLE.contains(&t) {
            let reason = match t {
                "start" | "restart" => "scenario",
                "save" | "loadSave" => "save",
                _ => "blocked",
            };
            me.deny(t, reason, now);
            return false;
        }
        let slot = integer(&msg["slot"]).and_then(|v| usize::try_from(v).ok());
        let val = msg["v"].as_str();
        match t {
            "addAi" if host && self.has_seat(maps) => {
                let name = format!("AI {}", self.players.iter().filter(|p| p.ai).count() + 1);
                self.add_seat(Person {
                    token: Vec::new(),
                    name,
                    connection: None,
                    ai: true,
                    team: 0,
                    faction: 0,
                    level: "normal",
                    disconnected: None,
                    denies: Vec::new(),
                });
            }
            "level" if host && matches!(val, Some("easy" | "normal" | "hard")) => {
                let Some(p) = slot.and_then(|i| self.players.get_mut(i)).filter(|p| p.ai) else {
                    return false;
                };
                p.level = match val.unwrap() {
                    "easy" => "easy",
                    "hard" => "hard",
                    _ => "normal",
                };
            }
            "kick" if host => {
                let Some(i) = slot.filter(|i| {
                    self.players
                        .get(*i)
                        .is_some_and(|p| p.ai || p.connection.is_none())
                }) else {
                    return false;
                };
                self.players.remove(i);
            }
            "team" | "faction" => {
                let Some(i) = slot.filter(|i| *i < self.players.len()) else {
                    return false;
                };
                let Some(v) = integer(&msg["v"]).filter(|v| *v < if t == "team" { 6 } else { 4 })
                else {
                    return false;
                };
                let p = &mut self.players[i];
                if !(if host {
                    t == "team" || p.ai || Some(i) == seated
                } else {
                    t == "faction" && Some(i) == seated
                }) {
                    return false;
                }
                if t == "team" {
                    p.team = v as usize;
                } else {
                    p.faction = v as usize;
                }
            }
            "map" if host && self.mode != "tutorial" => {
                let Some(name) = msg["name"].as_str().filter(|n| maps.get(*n).is_some()) else {
                    return false;
                };
                if self.mode == "horde" && maps[name]["horde"] != true {
                    return false;
                }
                self.map = name.into();
            }
            "mode"
                if host
                    && matches!(
                        val,
                        Some(
                            "conquest"
                                | "assault"
                                | "annihilation"
                                | "classic"
                                | "horde"
                                | "tutorial"
                                | "world"
                        )
                    ) =>
            {
                let mode = val.unwrap();
                if mode == "tutorial" {
                    self.map = "tutorial".into();
                }
                if mode == "horde" {
                    if maps[&self.map]["horde"] != true {
                        let Some(name) = maps
                            .as_object()
                            .unwrap()
                            .iter()
                            .find(|(_, m)| m["horde"] == true)
                            .map(|(n, _)| n)
                        else {
                            return false;
                        };
                        self.map = name.clone();
                    }
                    if self.army == "endless" {
                        self.army = "standard".into();
                    }
                }
                self.mode = mode.into();
            }
            "worldSize" if host => {
                let value = msg
                    .get("v")
                    .filter(|v| !v.is_null())
                    .unwrap_or(&msg["size"]);
                let Some(v) = value.as_str().filter(|v| matches!(*v, "huge" | "massive")) else {
                    return false;
                };
                self.world_size = v.into();
            }
            "army" if host && matches!(val, Some("standard" | "large" | "massive" | "endless")) => {
                if self.mode == "horde" && val == Some("endless") {
                    return false;
                }
                self.army = val.unwrap().into();
            }
            "sandbox" if host => self.sandbox = truthy(&msg["v"]),
            "weather"
                if host
                    && matches!(
                        val,
                        Some("map" | "clear" | "fog" | "rain" | "mud" | "snow" | "random")
                    ) =>
            {
                self.weather = val.unwrap().into()
            }
            "defender" if host && integer(&msg["v"]).is_some_and(|v| v < 6) => {
                self.defender = integer(&msg["v"]).unwrap() as usize
            }
            "spectate" if seated.is_some() && self.spectators.len() < MAX_SPECTATORS => {
                self.spectators.push(self.players.remove(seated.unwrap()));
            }
            "sit" if watching.is_some() && self.has_seat(maps) => {
                let p = self.spectators.remove(watching.unwrap());
                self.add_seat(p);
            }
            _ => return false,
        }
        true
    }
}

const UNAVAILABLE: &[&str] = &[
    "start",
    "restart",
    "save",
    "loadSave",
    "resync",
    "fps",
    "end",
    "nextwave",
    "pause",
    "resume",
    "handAi",
    "leave",
    "move",
    "amove",
    "attack",
    "fireat",
    "garrison",
    "stop",
    "escort",
    "board",
    "rail",
    "unload",
    "retreat",
    "stance",
    "cover",
    "ability",
    "autocast",
    "dig",
    "entrench",
    "support",
    "build",
    "assist",
    "tech",
    "buy",
    "cancel",
    "rally",
    "cancelProduction",
    "destroy",
    "recover",
    "orders",
    "spawn",
    "supply",
    "logisticsResume",
];

fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64() != Some(0.0),
        Value::String(s) => !s.is_empty(),
        _ => true,
    }
}
fn clean(s: &str, length: usize, fallback: &str, controls: bool) -> String {
    let filtered: String = s
        .chars()
        .filter(|c| !"<>&\"'".contains(*c) && !(controls && (*c as u32) < 32))
        .collect();
    // JavaScript slices UTF-16 code units, including a possible lone surrogate at the boundary.
    let units: Vec<_> = filtered.trim().encode_utf16().take(length).collect();
    let name = String::from_utf16_lossy(&units);
    if name.is_empty() {
        fallback.into()
    } else {
        name
    }
}

pub(crate) struct Hub {
    rooms: BTreeMap<String, Room>,
    room_order: Vec<String>,
    maps: Value,
    max_rooms: usize,
    public_url: String,
}
impl Hub {
    pub fn new(max_rooms: usize, public_url: String) -> Self {
        Self {
            rooms: BTreeMap::new(),
            room_order: Vec::new(),
            maps: serde_json::from_str(include_str!("map-catalog.json")).unwrap(),
            max_rooms,
            public_url,
        }
    }
    pub fn maps(&self) -> Value {
        json!(self.maps.as_object().unwrap().keys().collect::<Vec<_>>())
    }
    pub fn directory(&self) -> Value {
        json!({"rooms":self.room_order.iter().filter_map(|code| self.rooms.get(code)).filter(|r| r.listed && r.players.iter().any(|p| !p.ai && p.connection.is_some()))
            .map(|r| r.directory(&self.maps)).collect::<Vec<_>>()})
    }
    pub fn message(
        &mut self,
        code: &str,
        connection: &Connection,
        joined: &mut bool,
        msg: Value,
        now: u64,
    ) {
        if !msg.is_object() {
            return;
        }
        if *joined {
            if let Some(room) = self.rooms.get_mut(code)
                && room.command(connection.id, &msg, &self.maps, now)
            {
                room.broadcast(&self.maps, &self.public_url);
            }
            return;
        }
        if msg["t"] != "hello" || !msg["token"].is_string() || !msg["name"].is_string() {
            return;
        }
        if msg.get("proto").is_some_and(|v| integer(v) != Some(0)) {
            connection.send(json!({"t":"deny","cmd":"join","reason":"version"}));
            connection.close(1008, "unsupported protocol");
            return;
        }
        if msg["role"] == "operative" {
            connection.send(json!({"t":"deny","cmd":"join","reason":"commander"}));
            connection.close(1008, "operative unavailable");
            return;
        }
        if !self.rooms.contains_key(code) && self.rooms.len() >= self.max_rooms {
            connection.send(json!({"t":"full","reason":"capacity"}));
            connection.close(1000, "");
            return;
        }
        if !self.rooms.contains_key(code) {
            self.room_order.push(code.into());
        }
        let room = self.rooms.entry(code.into()).or_insert_with(|| {
            let tutorial = msg["listing"]["tutorial"] == true;
            Room {
                code: code.into(),
                listed: msg["listing"]["public"] == true && !tutorial,
                title: clean(
                    msg["listing"]["title"].as_str().unwrap_or("Open skirmish"),
                    48,
                    "Open skirmish",
                    true,
                ),
                players: Vec::new(),
                spectators: Vec::new(),
                mode: if tutorial { "tutorial" } else { "conquest" }.into(),
                map: if tutorial { "tutorial" } else { "default" }.into(),
                world_size: "huge".into(),
                army: "standard".into(),
                weather: "map".into(),
                sandbox: false,
                defender: 0,
                empty_since: None,
            }
        });
        let token: Vec<_> = msg["token"]
            .as_str()
            .unwrap()
            .encode_utf16()
            .take(40)
            .collect();
        let name = clean(msg["name"].as_str().unwrap(), 16, "Soldier", false);
        if let Some(p) = room
            .players
            .iter_mut()
            .chain(room.spectators.iter_mut())
            .find(|p| !token.is_empty() && p.token == token)
        {
            if let Some(old) = p.connection.replace(connection.clone()) {
                old.send(json!({"t":"replaced"}));
                old.close(1000, "");
            }
            p.name = name;
            p.disconnected = None;
        } else {
            let p = Person {
                token,
                name,
                connection: Some(connection.clone()),
                ai: false,
                team: 0,
                faction: 0,
                level: "normal",
                disconnected: None,
                denies: Vec::new(),
            };
            if room.has_seat(&self.maps) && msg["spectate"] != true {
                room.add_seat(p);
            } else if room.spectators.len() < MAX_SPECTATORS {
                room.spectators.push(p);
            } else {
                connection.send(json!({"t":"full","reason":"seats"}));
                connection.close(1000, "");
                return;
            }
        }
        *joined = true;
        room.empty_since = None;
        room.broadcast(&self.maps, &self.public_url);
        if msg["listing"]["tutorial"] == true && room.mode == "tutorial" {
            connection.send(json!({"t":"deny","cmd":"start","reason":"scenario"}));
        }
    }
    pub fn disconnected(&mut self, code: &str, id: u64, now: u64) {
        let Some(room) = self.rooms.get_mut(code) else {
            return;
        };
        let mut changed = false;
        for p in &mut room.players {
            if p.connection.as_ref().is_some_and(|c| c.id == id) {
                p.connection = None;
                p.disconnected = Some(now);
                changed = true;
            }
        }
        let size = room.spectators.len();
        room.spectators
            .retain(|p| !p.connection.as_ref().is_some_and(|c| c.id == id));
        changed |= size != room.spectators.len();
        if changed {
            if !room.people().any(|p| p.connection.is_some()) {
                room.empty_since = Some(now);
            }
            room.broadcast(&self.maps, &self.public_url);
        }
    }
    pub fn expire(&mut self, now: u64) {
        self.rooms.retain(|_, room| {
            if room
                .empty_since
                .is_some_and(|at| now.saturating_sub(at) > 60_000)
            {
                return false;
            }
            let size = room.players.len();
            room.players.retain(|p| {
                p.ai || p.connection.is_some()
                    || !p
                        .disconnected
                        .is_some_and(|at| now.saturating_sub(at) >= 10_000)
            });
            if size != room.players.len() {
                room.broadcast(&self.maps, &self.public_url);
            }
            true
        });
        self.room_order.retain(|code| self.rooms.contains_key(code));
    }
}

fn integer(value: &Value) -> Option<u64> {
    let n = value.as_f64()?;
    (n >= 0.0 && n.fract() == 0.0 && n <= 9_007_199_254_740_991.0).then_some(n as u64)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn client(id: u64) -> (Connection, mpsc::Receiver<Output>) {
        let (tx, rx) = mpsc::channel(64);
        let (cancel, _) = watch::channel(false);
        (Connection { id, tx, cancel }, rx)
    }
    fn join(h: &mut Hub, c: &Connection, token: &str, spectate: bool, now: u64) {
        h.message("ABC",c,&mut false,json!({"t":"hello","token":token,"name":token,"spectate":spectate,"listing":{"public":true}}),now);
    }
    #[test]
    fn duplicate_takeover_cannot_clear_or_control_the_new_connection() {
        let mut h = Hub::new(32, String::new());
        let (old, mut rx) = client(1);
        let (new, _) = client(2);
        join(&mut h, &old, "secret", false, 0);
        join(&mut h, &new, "secret", false, 2);
        assert!(matches!(rx.try_recv(), Ok(Output::Json(_))));
        assert!(matches!(rx.try_recv(),Ok(Output::Json(v)) if v["t"] == "replaced"));
        h.disconnected("ABC", old.id, 3);
        h.message("ABC", &old, &mut true, json!({"t":"addAi"}), 4);
        let room = &h.rooms["ABC"];
        assert_eq!(room.players.len(), 1);
        assert_eq!(room.players[0].connection.as_ref().unwrap().id, new.id);
        assert!(
            !room
                .lobby(new.id, &h.maps, "")
                .to_string()
                .contains("token")
        );
    }
    #[test]
    fn observer_hosts_ai_room_and_cannot_change_another_humans_faction() {
        let mut h = Hub::new(32, String::new());
        let (a, _) = client(1);
        let (b, _) = client(2);
        join(&mut h, &a, "a", false, 0);
        join(&mut h, &b, "b", false, 0);
        h.message(
            "ABC",
            &a,
            &mut true,
            json!({"t":"faction","slot":1,"v":3}),
            0,
        );
        assert_eq!(h.rooms["ABC"].players[1].faction, 1);
        h.message("ABC", &a, &mut true, json!({"t":"addAi"}), 0);
        h.message("ABC", &a, &mut true, json!({"t":"spectate"}), 0);
        h.message("ABC", &b, &mut true, json!({"t":"spectate"}), 0);
        let v = h.rooms["ABC"].lobby(a.id, &h.maps, "");
        assert_eq!(v["host"], -1);
        assert_eq!(v["amHost"], true);
        assert_eq!(h.directory()["rooms"], json!([]));
    }
    #[test]
    fn cleanup_uses_explicit_clock_and_reconnect_cancels_expiry() {
        let mut h = Hub::new(32, String::new());
        let (a, _) = client(1);
        let (b, _) = client(2);
        join(&mut h, &a, "a", false, 0);
        h.disconnected("ABC", 1, 100);
        h.expire(10_099);
        assert_eq!(h.rooms["ABC"].players.len(), 1);
        join(&mut h, &b, "a", false, 10_099);
        h.expire(10_100);
        assert_eq!(h.rooms["ABC"].players.len(), 1);
        h.disconnected("ABC", 2, 11_000);
        h.expire(21_000);
        assert_eq!(h.rooms["ABC"].players.len(), 0);
        h.expire(71_000);
        assert_eq!(h.rooms.len(), 1);
        h.expire(71_001);
        assert_eq!(h.rooms.len(), 0);
    }
    #[test]
    fn public_capacity_and_private_overflow_follow_reference() {
        let mut h = Hub::new(1, String::new());
        let (a, _) = client(1);
        join(&mut h, &a, "a", false, 0);
        let room = h.rooms.get_mut("ABC").unwrap();
        assert_eq!(
            room.capacity(&h.maps),
            h.maps["default"]["normal"].as_u64().unwrap() as usize
        );
        room.listed = false;
        for id in 2..=6 {
            let (c, _) = client(id);
            join(&mut h, &c, &id.to_string(), false, 0);
        }
        let (c, _) = client(7);
        join(&mut h, &c, "spectator", false, 0);
        assert_eq!(h.rooms["ABC"].players.len(), 6);
        assert_eq!(h.rooms["ABC"].spectators.len(), 1);
        let (c, mut rx) = client(8);
        h.message(
            "XYZ",
            &c,
            &mut false,
            json!({"t":"hello","token":"x","name":"x"}),
            0,
        );
        assert!(
            matches!(rx.try_recv(),Ok(Output::Json(v)) if v == json!({"t":"full","reason":"capacity"}))
        );
    }
    #[test]
    fn truncated_unicode_tokens_remain_distinct() {
        let mut h = Hub::new(32, String::new());
        let (a, _) = client(1);
        let (b, _) = client(2);
        join(
            &mut h,
            &a,
            &format!("{}\u{10000}", "x".repeat(39)),
            false,
            0,
        );
        join(
            &mut h,
            &b,
            &format!("{}\u{10400}", "x".repeat(39)),
            false,
            0,
        );
        assert_eq!(h.rooms["ABC"].players.len(), 2);
    }
    #[test]
    fn settings_type_guards_and_horde_transition() {
        let mut h = Hub::new(32, String::new());
        let (a, _) = client(1);
        join(&mut h, &a, "a", false, 0);
        for v in [json!("2"), json!([2]), json!(1.5), json!(-1), Value::Null] {
            h.message("ABC", &a, &mut true, json!({"t":"team","slot":0,"v":v}), 0);
            assert_eq!(h.rooms["ABC"].players[0].team, 0);
        }
        h.message(
            "ABC",
            &a,
            &mut true,
            json!({"t":"team","slot":0.0,"v":2.0}),
            0,
        );
        assert_eq!(h.rooms["ABC"].players[0].team, 2);
        h.message("ABC", &a, &mut true, json!({"t":"army","v":"endless"}), 0);
        h.message("ABC", &a, &mut true, json!({"t":"mode","v":"horde"}), 0);
        let room = &h.rooms["ABC"];
        assert_eq!(room.army, "standard");
        assert_eq!(room.capacity(&h.maps), 5);
        assert_eq!(h.maps[&room.map]["horde"], true);
    }
    #[test]
    fn denied_commands_are_throttled_and_never_start_a_match() {
        let mut h = Hub::new(32, String::new());
        let (a, mut rx) = client(1);
        join(&mut h, &a, "a", false, 0);
        let _ = rx.try_recv();
        for _ in 0..6 {
            h.message("ABC", &a, &mut true, json!({"t":"start"}), 10);
        }
        for _ in 0..4 {
            assert!(
                matches!(rx.try_recv(),Ok(Output::Json(v)) if v["t"]=="deny" && v["reason"]=="scenario")
            );
        }
        assert!(rx.try_recv().is_err());
        h.message("ABC", &a, &mut true, json!({"t":"start"}), 1_010);
        assert!(rx.try_recv().is_ok());
        assert_eq!(h.rooms["ABC"].lobby(a.id, &h.maps, "")["state"], "lobby");
    }
}
