use crate::commander_ingress::CommanderIngress;
use crate::room_runtime::{FrameKind, RoomRuntime};
use crate::snapshot_backpressure::{CompletionContext, SeatBackpressure, WriteTicket};
use crate::snapshot_lists::{ListScope, SnapshotDelivery};
use crate::tick_timing::{CatchUpClock, TickSample};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::Write,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};
use tokio::sync::{mpsc, watch};
use ww2_core::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    match_end::MatchSample,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    match_story::{MatchResult, StoryKey},
    recipient_snapshot::SelectedSeat,
    weather::WeatherChoice,
};

const MAX_PLAYERS: usize = 6;
const MAX_SPECTATORS: usize = 8;

#[derive(Clone)]
pub(crate) struct Connection {
    pub id: u64,
    pub tx: mpsc::Sender<Output>,
    pub cancel: watch::Sender<bool>,
}

#[derive(Debug)]
pub(crate) struct SnapshotWrite {
    pub room: String,
    pub epoch: u64,
    pub slot: usize,
    pub connection_id: u64,
    pub ticket: WriteTicket,
    pub json: String,
}

#[derive(Debug)]
pub(crate) enum Output {
    Json(Value),
    Snapshot(SnapshotWrite),
    Close(u16, &'static str),
}

impl Connection {
    fn ready(&self) -> bool {
        !self.tx.is_closed() && !*self.cancel.borrow()
    }
    pub fn send(&self, msg: Value) {
        self.output(Output::Json(msg));
    }
    pub fn close(&self, code: u16, reason: &'static str) {
        self.output(Output::Close(code, reason));
    }
    fn snapshot(&self, write: SnapshotWrite) -> Result<(), SnapshotWrite> {
        match self.tx.try_send(Output::Snapshot(write)) {
            Ok(()) => Ok(()),
            Err(error) => {
                let _ = self.cancel.send(true);
                let Output::Snapshot(write) = error.into_inner() else {
                    unreachable!()
                };
                Err(write)
            }
        }
    }
    fn output(&self, msg: Output) {
        if self.tx.try_send(msg).is_err() {
            // A stalled client cannot make every other room wait or grow an unbounded queue.
            let _ = self.cancel.send(true);
        }
    }
}

struct SnapshotAccount {
    room: String,
    epoch: u64,
    slot: usize,
    delivery: SnapshotDelivery,
    pressure: SeatBackpressure,
    full_terrain: bool,
}

impl SnapshotAccount {
    fn start_stream(
        accounts: &mut BTreeMap<u64, Self>,
        id: u64,
        room: &str,
        epoch: u64,
        slot: usize,
    ) {
        let account = accounts.entry(id).or_insert_with(|| Self {
            room: room.into(),
            epoch,
            slot,
            delivery: SnapshotDelivery::default(),
            pressure: SeatBackpressure::default(),
            full_terrain: true,
        });
        // Outstanding writes stay with the socket when its match stream resets.
        account.room = room.into();
        account.epoch = epoch;
        account.slot = slot;
        account.delivery.reset();
        account.full_terrain = true;
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
    rtt: Option<f64>,
    last_match: Option<Value>,
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

struct AttachedRuntime {
    epoch: u64,
    ingress: CommanderIngress,
    runtime: RoomRuntime,
    startup: crate::native_startup::NativeStartup,
    // Complete R1 options/seeds are a host receipt, never a client envelope.
    receipt: Option<Value>,
}

struct PreparedNative {
    attached: AttachedRuntime,
    starts: Vec<(usize, Value)>,
    resume: Value,
    pause: Option<Value>,
}

struct RoomResult {
    report: Value,
    status: crate::NativeRoomStatus,
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
    native: Option<AttachedRuntime>,
    result: Option<RoomResult>,
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
        if let Some(result) = &mut self.result {
            for (key, empty) in [
                ("teams", Value::Null),
                ("names", json!("")),
                ("story", Value::Null),
            ] {
                if let Some(list) = result.report.get_mut(key).and_then(Value::as_array_mut) {
                    list.insert(self.players.len(), empty);
                }
            }
        }
        self.players.push(p);
    }
    fn retain_seats(&mut self, keep: impl Fn(usize, &Person) -> bool) -> Vec<Person> {
        let count = self.players.len();
        let retained = self
            .players
            .iter()
            .enumerate()
            .map(|(slot, person)| keep(slot, person))
            .collect::<Vec<_>>();
        if retained.iter().any(|keep| !keep)
            && let Some(result) = &mut self.result
        {
            let order = retained
                .iter()
                .enumerate()
                .filter(|(_, keep)| **keep)
                .map(|(slot, _)| slot)
                .chain(
                    retained
                        .iter()
                        .enumerate()
                        .filter(|(_, keep)| !**keep)
                        .map(|(slot, _)| slot),
                )
                .collect::<Vec<_>>();
            for key in ["teams", "names", "story"] {
                if let Some(list) = result.report.get_mut(key).and_then(Value::as_array_mut) {
                    let old = std::mem::take(list);
                    let mut rows = order
                        .iter()
                        .map(|slot| old.get(*slot).cloned().unwrap_or(Value::Null))
                        .collect::<Vec<_>>();
                    rows.extend(old.into_iter().skip(count));
                    *list = rows;
                }
            }
        }
        let mut removed = Vec::new();
        for (person, keep) in std::mem::take(&mut self.players).into_iter().zip(retained) {
            if keep {
                self.players.push(person);
            } else {
                removed.push(person);
            }
        }
        removed
    }
    fn finish_native(&mut self, now: u64) -> Result<(), String> {
        let native = self.native.as_mut().ok_or("native runtime absent")?;
        if native.runtime.held() != 120 {
            return Err("native result precedes final hold callback".into());
        }
        let result = native.runtime.finished_result()?;
        if result.teams.len() != self.players.len()
            || result.names.len() != self.players.len()
            || result.story.len() != self.players.len()
        {
            return Err("native result seat rows disagree with the stable roster".into());
        }
        let outcomes = result.teams.iter().map(|team| json!({"team":team,
            "outcome":if result.winner == -1 { "draw" } else if result.winner == *team { "victory" } else { "defeat" }})).collect::<Vec<_>>();
        let report = result_message(
            result,
            self.players.iter().map(|p| p.name.clone()).collect(),
        )?;
        let status = crate::NativeRoomStatus {
            epoch: native.epoch,
            tick: native.runtime.state().world.game.tick,
            held: native.runtime.held(),
            ending_complete: true,
            paused: false,
            fault: None,
        };
        for (person, outcome) in self.players.iter_mut().zip(outcomes) {
            person.last_match = Some(outcome);
        }
        let resume = native.runtime.resume(now);
        for person in &self.players {
            if let Some(connection) = &person.connection {
                connection.send(resume.clone());
            }
        }
        self.result = Some(RoomResult { report, status });
        self.native = None;
        self.retain_seats(|_, person| {
            person.ai || person.connection.as_ref().is_some_and(Connection::ready)
        });
        if self.empty_since.is_none()
            && !self
                .people()
                .any(|person| person.connection.as_ref().is_some_and(Connection::ready))
        {
            self.empty_since = Some(now);
        }
        Ok(())
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
            "t":"lobby", "proto":0, "code": self.code, "state":if self.native.is_some() { "play" } else { "lobby" },
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
        if let Some(last) = &self.result {
            let mut report = last.report.clone();
            report["you"] = you
                .and_then(|slot| self.players[slot].last_match.clone())
                .unwrap_or(Value::Null);
            result["result"] = report;
        }
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
            "map":if self.mode == "world" { "world" } else { &self.map },"state":if self.native.is_some() { "play" } else { "lobby" },
            "humans":self.players.iter().filter(|p| !p.ai && p.connection.is_some()).count(),
            "ai":self.players.iter().filter(|p| p.ai).count(),"occupied":self.players.len(),
            "capacity":self.capacity(maps),"joinable":self.native.is_none() && self.players.len() < self.capacity(maps)
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
        let attached = self.native.is_some();
        let me = if let Some(i) = seated {
            &mut self.players[i]
        } else {
            &mut self.spectators[watching.unwrap()]
        };
        if t == "ping" {
            if msg.get("x").is_none() && msg.get("z").is_none() {
                if let Some(rtt) = msg["rtt"].as_f64().filter(|v| v.is_finite()) {
                    me.rtt = Some(rtt.round().clamp(0.0, 9999.0));
                }
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
        if CommanderIngress::recognizes(t)
            && let Some(native) = &mut self.native
        {
            // The current connection was resolved from the stable roster above.
            // Paused rooms ignore ordinary orders just like the Source callback.
            if native.runtime.paused() || !me.connection.as_ref().is_some_and(Connection::ready) {
                return false;
            }
            let Some(slot) = seated else {
                return false;
            };
            let denial = if me.ai || native.runtime.fault().is_some() {
                Some(ww2_core::match_commands::Denial::Blocked)
            } else {
                native
                    .ingress
                    .enqueue(native.runtime.state_mut(), slot, msg)
                    .err()
            };
            if let Some(denial) = denial {
                me.deny(t, denial.name(), now);
            }
            return false;
        }
        // Other gameplay and public setup admission remain unavailable.
        if UNAVAILABLE.contains(&t) {
            let reason = match t {
                "start" | "restart" => "scenario",
                "save" | "loadSave" => "save",
                _ => "blocked",
            };
            me.deny(t, reason, now);
            return false;
        }
        if attached
            && matches!(
                t,
                "addAi"
                    | "level"
                    | "kick"
                    | "team"
                    | "faction"
                    | "map"
                    | "mode"
                    | "worldSize"
                    | "army"
                    | "sandbox"
                    | "weather"
                    | "defender"
                    | "spectate"
                    | "sit"
            )
        {
            me.deny(t, "blocked", now);
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
                    rtt: None,
                    last_match: None,
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
                self.retain_seats(|slot, _| slot != i);
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
                let mut removed = self.retain_seats(|slot, _| Some(slot) != seated);
                self.spectators.push(removed.remove(0));
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
fn display_space(c: char) -> bool {
    // ECMAScript TrimString uses WhiteSpace plus LineTerminator, not Rust's
    // Unicode White_Space property. In particular FEFF trims and 0085 stays.
    matches!(
        c,
        '\u{0009}'..='\u{000d}'
            | '\u{0020}'
            | '\u{00a0}'
            | '\u{1680}'
            | '\u{2000}'..='\u{200a}'
            | '\u{2028}'
            | '\u{2029}'
            | '\u{202f}'
            | '\u{205f}'
            | '\u{3000}'
            | '\u{feff}'
    )
}

fn clean(s: &str, length: usize, fallback: &str, controls: bool) -> String {
    let filtered: String = s
        .chars()
        .filter(|c| !"<>&\"'".contains(*c) && !(controls && (*c as u32) < 32))
        .collect();
    // JavaScript slices UTF-16 code units, including a possible lone surrogate at the boundary.
    let units: Vec<_> = filtered
        .trim_matches(display_space)
        .encode_utf16()
        .take(length)
        .collect();
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
    accounts: BTreeMap<u64, SnapshotAccount>,
    next_epoch: u64,
    clock: CatchUpClock,
    stopped: Arc<AtomicBool>,
    trusted: Option<crate::TrustedRoomConfig>,
}
impl Hub {
    pub fn new(max_rooms: usize, public_url: String) -> Self {
        Self {
            rooms: BTreeMap::new(),
            room_order: Vec::new(),
            maps: serde_json::from_str(include_str!("map-catalog.json")).unwrap(),
            max_rooms,
            public_url,
            accounts: BTreeMap::new(),
            next_epoch: 0,
            clock: CatchUpClock::new(0),
            stopped: Arc::new(AtomicBool::new(false)),
            trusted: None,
        }
    }
    pub fn attach_native(&mut self, code: &str, state: Match, now: u64) -> Result<u64, String> {
        let prepared = self.prepare_native(code, state, now, false)?;
        Ok(self.publish_native(code, prepared))
    }

    pub(crate) fn configure_trusted_room(&mut self, trusted: Option<crate::TrustedRoomConfig>) {
        self.trusted = trusted;
    }

    fn prepare_native(
        &self,
        code: &str,
        state: Match,
        now: u64,
        replace: bool,
    ) -> Result<PreparedNative, String> {
        if self.is_stopped() {
            return Err("server stopped".into());
        }
        let room = self.rooms.get(code).ok_or("room does not exist")?;
        if room.native.is_some() && !replace {
            return Err("room already has a native runtime".into());
        }
        if !room.spectators.is_empty()
            || room.players.is_empty()
            || room.players.iter().any(|p| p.ai)
        {
            return Err("native attachment needs commander human seats without spectators".into());
        }
        if state.start.players.len() != room.players.len()
            || state.options.players.len() != room.players.len()
            || state.world.game.players.len() != room.players.len()
            || room.players.iter().enumerate().any(|(slot, person)| {
                let start = &state.start.players[slot].config;
                let config = &state.options.players[slot];
                let live = &state.world.game.players[slot];
                start.team != person.team as i32
                    || config.team != start.team
                    || live.slot != slot
                    || live.team != start.team
                    || start.faction as usize != person.faction
                    || config.faction != start.faction
            })
        {
            return Err("native match seats disagree with the room roster".into());
        }
        let epoch = self
            .next_epoch
            .checked_add(1)
            .filter(|epoch| *epoch <= crate::native_startup::MAX_MATCH_ID)
            .ok_or("native runtime epoch exhausted")?;
        let startup = crate::native_startup::NativeStartup::prepare(
            &state,
            crate::native_startup::StartupSettings {
                map: &room.map,
                mode: &room.mode,
                army: &room.army,
                weather: &room.weather,
                sandbox: room.sandbox,
            },
        )?;
        // Canonical runtime admission completes before changing the room or accounts.
        let mut runtime = RoomRuntime::new(state, now)?;
        let resume = runtime.resume(now);
        for (slot, person) in room.players.iter().enumerate() {
            if person
                .connection
                .as_ref()
                .is_none_or(|connection| !connection.ready())
            {
                runtime.auto_pause(slot, &person.name, now)?;
            }
        }
        let slots = (0..room.players.len()).collect::<Vec<_>>();
        let names = room
            .players
            .iter()
            .map(|p| p.name.clone())
            .collect::<Vec<_>>();
        let starts = runtime.collect_starts(&slots, &startup, &names, epoch)?;
        let pause = runtime.paused().then(|| runtime.pause_message(now));
        Ok(PreparedNative {
            attached: AttachedRuntime {
                epoch,
                ingress: CommanderIngress::default(),
                runtime,
                startup,
                receipt: None,
            },
            starts,
            resume,
            pause,
        })
    }

    fn publish_native(&mut self, code: &str, prepared: PreparedNative) -> u64 {
        let PreparedNative {
            attached,
            starts,
            resume,
            pause,
        } = prepared;
        let epoch = attached.epoch;
        let room = self.rooms.get_mut(code).unwrap();
        room.native = Some(attached);
        room.result = None;
        self.next_epoch = epoch;
        for (slot, person) in room.players.iter().enumerate() {
            if let Some(connection) = &person.connection {
                SnapshotAccount::start_stream(&mut self.accounts, connection.id, code, epoch, slot);
            }
        }
        for person in room.people() {
            if let Some(connection) = &person.connection {
                connection.send(resume.clone());
            }
        }
        room.broadcast(&self.maps, &self.public_url);
        for (slot, start) in starts {
            if let Some(connection) = &room.players[slot].connection {
                connection.send(start);
                if let Some(pause) = &pause {
                    connection.send(pause.clone());
                }
                self.accounts.get_mut(&connection.id).unwrap().full_terrain = false;
            }
        }
        if let Some(receipt) = &room.native.as_ref().unwrap().receipt {
            // The installed runtime retains the receipt even if the log sink fails.
            let _ = writeln!(std::io::stderr().lock(), "NATIVE_MATCH {receipt}");
        }
        epoch
    }
    pub fn native_status(&self, code: &str) -> Result<crate::NativeRoomStatus, String> {
        if self.is_stopped() {
            return Err("server stopped".into());
        }
        let room = self.rooms.get(code).ok_or("native room does not exist")?;
        let Some(native) = room.native.as_ref() else {
            return room
                .result
                .as_ref()
                .map(|result| result.status.clone())
                .ok_or("native room does not exist".into());
        };
        Ok(crate::NativeRoomStatus {
            epoch: native.epoch,
            tick: native.runtime.state().world.game.tick,
            held: native.runtime.held(),
            ending_complete: false,
            paused: native.runtime.paused(),
            fault: native.runtime.fault().map(str::to_owned),
        })
    }
    fn is_stopped(&self) -> bool {
        self.stopped.load(Ordering::Acquire)
    }
    pub(crate) fn stop_authority(&self) -> Arc<AtomicBool> {
        self.stopped.clone()
    }
    pub fn stop(&mut self) {
        self.stopped.store(true, Ordering::Release);
    }
    pub fn retire_connection(&mut self, id: u64) {
        let Some(account) = self.accounts.remove(&id) else {
            return;
        };
        let current = self.rooms.get(&account.room).is_some_and(|room| {
            room.native
                .as_ref()
                .is_some_and(|native| native.epoch == account.epoch)
                && room
                    .players
                    .get(account.slot)
                    .and_then(|p| p.connection.as_ref())
                    .is_some_and(|c| c.id == id)
        });
        if current
            && account.pressure.inflight() != 0
            && let Some(native) = self
                .rooms
                .get_mut(&account.room)
                .and_then(|r| r.native.as_mut())
        {
            native
                .runtime
                .fault_stop("terminal socket retained snapshot reservations".into());
        }
    }
    pub fn complete_snapshot(&mut self, write: &SnapshotWrite, success: bool, now: u64) {
        let current = !self.is_stopped()
            && self.rooms.get(&write.room).is_some_and(|room| {
                room.native
                    .as_ref()
                    .is_some_and(|native| native.epoch == write.epoch)
                    && room
                        .players
                        .get(write.slot)
                        .and_then(|p| p.connection.as_ref())
                        .is_some_and(|c| c.id == write.connection_id)
            });
        let ready = self
            .rooms
            .get(&write.room)
            .and_then(|room| room.players.get(write.slot))
            .and_then(|p| p.connection.as_ref())
            .is_some_and(Connection::ready);
        let playing = self
            .rooms
            .get(&write.room)
            .and_then(|room| room.native.as_ref())
            .is_some_and(|native| {
                native.epoch == write.epoch
                    && native.runtime.fault().is_none()
                    && native.runtime.held() < 120
            });
        // Old-epoch tickets still belong to this socket's accounting. The current
        // runtime/slot checks above prevent them from producing a new catch-up.
        let Some(account) = self.accounts.get_mut(&write.connection_id) else {
            return;
        };
        let catch_up = match account.pressure.complete_write(
            write.ticket,
            CompletionContext {
                current_socket: current && success && ready,
                seat_present: current,
                playing,
                has_game: playing,
            },
        ) {
            Ok(catch_up) => catch_up,
            Err(error) => {
                if current
                    && let Some(native) = self
                        .rooms
                        .get_mut(&write.room)
                        .and_then(|room| room.native.as_mut())
                {
                    native
                        .runtime
                        .fault_stop(format!("snapshot completion: {error}"));
                }
                return;
            }
        };
        if !success {
            // A failed network write loses this socket, not the canonical match.
            // Its new stream on reconnect starts from full terrain/fog and an empty ledger.
            if current {
                self.disconnected(&write.room, write.connection_id, now);
            }
            return;
        }
        if catch_up {
            let Some(room) = self.rooms.get_mut(&write.room) else {
                return;
            };
            let full = self.accounts[&write.connection_id].full_terrain;
            let selected = [SelectedSeat {
                slot: write.slot,
                full,
                omit: false,
            }];
            let mut dropped = Vec::new();
            Self::send_native_frame(
                room,
                &mut self.accounts,
                &selected,
                FrameKind::Current,
                &mut dropped,
            );
            for (code, id) in dropped {
                self.disconnected(&code, id, now);
            }
        }
    }
    fn select_native(
        room: &Room,
        accounts: &mut BTreeMap<u64, SnapshotAccount>,
        backed_up: bool,
    ) -> Result<Vec<SelectedSeat>, String> {
        let native = room.native.as_ref().ok_or("native runtime absent")?;
        let mut selected = Vec::new();
        for (slot, person) in room.players.iter().enumerate() {
            let Some(connection) = person.connection.as_ref().filter(|c| c.ready()) else {
                continue;
            };
            let account = accounts
                .get_mut(&connection.id)
                .filter(|account| {
                    account.room == room.code
                        && account.epoch == native.epoch
                        && account.slot == slot
                })
                .ok_or("native snapshot account absent")?;
            if backed_up && account.pressure.skip_snapshot() {
                continue;
            }
            selected.push(SelectedSeat {
                slot,
                full: account.full_terrain,
                omit: false,
            });
        }
        Ok(selected)
    }
    fn send_native_frame(
        room: &mut Room,
        accounts: &mut BTreeMap<u64, SnapshotAccount>,
        selected: &[SelectedSeat],
        kind: FrameKind,
        dropped: &mut Vec<(String, u64)>,
    ) -> Option<(f64, f64)> {
        let online = room
            .players
            .iter()
            .map(|p| p.connection.as_ref().is_some_and(Connection::ready))
            .collect::<Vec<_>>();
        let ping = room.players.iter().map(|p| p.rtt).collect::<Vec<_>>();
        let native = room.native.as_mut()?;
        let began = Instant::now();
        let captured = match native.runtime.collect_frame(selected, kind) {
            Ok(captured) => captured,
            Err(error) => {
                native
                    .runtime
                    .fault_stop(format!("snapshot collection: {error}"));
                return None;
            }
        };
        let mut build = began.elapsed().as_secs_f64() * 1000.0;
        let mut stringify = 0.0;
        if captured.len() != selected.len()
            || captured
                .iter()
                .zip(selected)
                .any(|(frame, seat)| frame.slot != seat.slot)
        {
            native
                .runtime
                .fault_stop("snapshot selection changed during capture".into());
            return None;
        }
        for mut frame in captured {
            let build_at = Instant::now();
            let scope = match native.runtime.list_scope(frame.batch) {
                Ok(scope) if scope == ListScope::Shared(frame.list_versions) => scope,
                _ => {
                    native
                        .runtime
                        .fault_stop("snapshot list batch disagrees with canonical cache".into());
                    return None;
                }
            };
            let Some(connection) = room
                .players
                .get(frame.slot)
                .and_then(|p| p.connection.as_ref())
            else {
                native
                    .runtime
                    .fault_stop("snapshot seat disappeared after capture".into());
                return None;
            };
            let Some(account) = accounts.get_mut(&connection.id).filter(|account| {
                account.room == room.code
                    && account.epoch == native.epoch
                    && account.slot == frame.slot
            }) else {
                native
                    .runtime
                    .fault_stop("snapshot account disappeared after capture".into());
                return None;
            };
            frame.message["online"] = json!(online);
            frame.message["ping"] = json!(ping);
            let message = match account.delivery.trim(scope, frame.message) {
                Ok(message) => message,
                Err(error) => {
                    native.runtime.fault_stop(format!("snapshot trim: {error}"));
                    return None;
                }
            };
            build += build_at.elapsed().as_secs_f64() * 1000.0;
            let stringify_at = Instant::now();
            let json = match serde_json::to_string(&message) {
                Ok(json) => json,
                Err(error) => {
                    native
                        .runtime
                        .fault_stop(format!("snapshot serialization: {error}"));
                    return None;
                }
            };
            stringify += stringify_at.elapsed().as_secs_f64() * 1000.0;
            let ticket = match account.pressure.begin_write(&json) {
                Ok(ticket) => ticket,
                Err(error) => {
                    native
                        .runtime
                        .fault_stop(format!("snapshot reservation: {error}"));
                    return None;
                }
            };
            account.full_terrain = false;
            let write = SnapshotWrite {
                room: room.code.clone(),
                epoch: native.epoch,
                slot: frame.slot,
                connection_id: connection.id,
                ticket,
                json,
            };
            if let Err(write) = connection.snapshot(write) {
                let drained = account.pressure.complete_write(
                    write.ticket,
                    CompletionContext {
                        current_socket: false,
                        seat_present: false,
                        playing: false,
                        has_game: false,
                    },
                );
                match drained {
                    Ok(_) => dropped.push((write.room, write.connection_id)),
                    Err(error) => {
                        native
                            .runtime
                            .fault_stop(format!("snapshot enqueue accounting: {error}"));
                        return None;
                    }
                }
            }
        }
        Some((build, stringify))
    }
    pub fn poll_native(&mut self, now_ms: u64, now_ns: u64) {
        if self.is_stopped() {
            return;
        }
        let poll = match self.clock.poll(now_ns) {
            Ok(poll) => poll,
            Err(error) => {
                for room in self.rooms.values_mut() {
                    if let Some(native) = &mut room.native {
                        native.runtime.fault_stop(format!("host clock: {error}"));
                    }
                }
                return;
            }
        };
        for _ in 0..poll.ticks {
            if self.is_stopped() {
                return;
            }
            self.expire(now_ms);
            let mut dropped = Vec::new();
            for code in &self.room_order {
                if self.stopped.load(Ordering::Acquire) {
                    return;
                }
                let Some(room) = self.rooms.get_mut(code) else {
                    continue;
                };
                let Some(native) = &mut room.native else {
                    continue;
                };
                if native.runtime.fault().is_some() || native.runtime.held() >= 120 {
                    native.ingress.discard_feedback();
                    continue;
                }
                let connected = room
                    .players
                    .iter()
                    .map(|p| p.connection.as_ref().is_some_and(Connection::ready))
                    .collect::<Vec<_>>();
                let pause = match native.runtime.poll_pause(&connected, now_ms) {
                    Ok(pause) => pause,
                    Err(error) => {
                        native.runtime.fault_stop(format!("native pause: {error}"));
                        continue;
                    }
                };
                if let Some(message) = pause.message {
                    for person in &room.players {
                        if let Some(connection) = &person.connection {
                            connection.send(message.clone());
                        }
                    }
                }
                if pause.blocked {
                    if pause.send_frame {
                        match Self::select_native(room, &mut self.accounts, false) {
                            Ok(selected) => {
                                Self::send_native_frame(
                                    room,
                                    &mut self.accounts,
                                    &selected,
                                    FrameKind::Scheduled,
                                    &mut dropped,
                                );
                            }
                            Err(error) => room.native.as_mut().unwrap().runtime.fault_stop(error),
                        }
                    }
                    continue;
                }
                let away = room
                    .players
                    .iter()
                    .map(|person| person.connection.is_none())
                    .collect::<Vec<_>>();
                let native = room.native.as_mut().unwrap();
                if let Err(error) = native.runtime.set_away(&away) {
                    native.runtime.fault_stop(error);
                    continue;
                }
                let ending = match native.runtime.hold_callback() {
                    Ok(ending) => ending,
                    Err(error) => {
                        native.runtime.fault_stop(error);
                        continue;
                    }
                };
                if let Some(ending) = ending {
                    if ending.advance {
                        if let Err(error) = native.runtime.advance_tick(&away) {
                            native.runtime.fault_stop(error);
                            continue;
                        }
                        match Self::select_native(room, &mut self.accounts, false) {
                            Ok(selected) => {
                                Self::send_native_frame(
                                    room,
                                    &mut self.accounts,
                                    &selected,
                                    FrameKind::Scheduled,
                                    &mut dropped,
                                );
                            }
                            Err(error) => room.native.as_mut().unwrap().runtime.fault_stop(error),
                        }
                    }
                    if ending.complete
                        && room
                            .native
                            .as_ref()
                            .is_some_and(|native| native.runtime.fault().is_none())
                    {
                        match room.finish_native(now_ms) {
                            Ok(()) => room.broadcast(&self.maps, &self.public_url),
                            Err(error) => room.native.as_mut().unwrap().runtime.fault_stop(error),
                        }
                    }
                    continue;
                }
                let began = Instant::now();
                let report = match native.runtime.advance_tick(&away) {
                    Ok(report) => report,
                    Err(error) => {
                        native.ingress.discard_feedback();
                        if native.receipt.is_some() {
                            let _ = writeln!(
                                std::io::stderr().lock(),
                                "NATIVE_FAULT {}",
                                json!({
                                    "room": room.code, "matchId": native.epoch,
                                    "tick": native.runtime.state().world.game.tick, "error": error
                                })
                            );
                        }
                        native.runtime.fault_stop(error);
                        continue;
                    }
                };
                let step = began.elapsed().as_secs_f64() * 1000.0;
                for (slot, command, denial) in native.ingress.complete(&report.commands) {
                    if let Some(person) = room.players.get_mut(slot) {
                        person.deny(command, denial.name(), now_ms);
                    }
                }
                let sent = native.runtime.should_snapshot(report.tick);
                let snapshot_at = Instant::now();
                let mut costs = (0.0, 0.0);
                if sent {
                    match Self::select_native(room, &mut self.accounts, true) {
                        Ok(selected) => {
                            let Some(value) = Self::send_native_frame(
                                room,
                                &mut self.accounts,
                                &selected,
                                FrameKind::Scheduled,
                                &mut dropped,
                            ) else {
                                continue;
                            };
                            costs = value;
                        }
                        Err(error) => {
                            room.native.as_mut().unwrap().runtime.fault_stop(error);
                            continue;
                        }
                    }
                }
                let sample = TickSample {
                    sent,
                    durations: [
                        began.elapsed().as_secs_f64() * 1000.0,
                        step,
                        0.0,
                        snapshot_at.elapsed().as_secs_f64() * 1000.0,
                        costs.0,
                        costs.1,
                    ],
                };
                let native = room.native.as_mut().unwrap();
                if let Err(error) = native.runtime.record_timing(sample, now_ms) {
                    native.runtime.fault_stop(error);
                }
            }
            // Expected socket losses are applied after every due callback and
            // before another tick, without interrupting healthy peer output.
            for (code, id) in dropped {
                self.disconnected(&code, id, now_ms);
            }
        }
    }
    pub fn directory(&self) -> Value {
        json!({"rooms":self.room_order.iter().filter_map(|code| self.rooms.get(code)).filter(|r| r.listed && r.players.iter().any(|p| !p.ai && p.connection.is_some()))
            .map(|r| r.directory(&self.maps)).collect::<Vec<_>>()})
    }

    fn start_trusted(
        &mut self,
        code: &str,
        connection_id: u64,
        restart: bool,
        now: u64,
    ) -> Result<u64, String> {
        let trusted = self.trusted.as_ref().ok_or("trusted hosting is disabled")?;
        if trusted.code != code || self.is_stopped() {
            return Err("trusted room unavailable".into());
        }
        let room = self.rooms.get(code).ok_or("trusted room absent")?;
        if !room.is_host(connection_id)
            || !room.players.iter().any(|p| {
                p.connection
                    .as_ref()
                    .is_some_and(|c| c.id == connection_id && c.ready())
            })
        {
            return Err("only the current commander host may start or restart".into());
        }
        if restart {
            let native = room
                .native
                .as_ref()
                .ok_or("restart requires an attached match")?;
            if let Some(fault) = native.runtime.fault() {
                return Err(format!(
                    "faulted match restart awaits a recovery policy: {fault}"
                ));
            }
        } else if room.native.is_some() {
            return Err("start requires lobby state".into());
        }
        if room.listed || room.mode != "conquest" || room.army != "standard" || room.sandbox {
            return Err("R1 requires private Conquest, standard army and sandbox off".into());
        }
        if room.players.len() != 2
            || !room.spectators.is_empty()
            || room.players.iter().any(|p| p.ai || p.token.is_empty())
            || room.players[0].team == room.players[1].team
            || room.players[0].token == room.players[1].token
        {
            return Err("R1 requires exactly two human commander tokens on opposing teams".into());
        }
        if !matches!(
            room.weather.as_str(),
            "map" | "clear" | "fog" | "rain" | "mud" | "snow" | "random"
        ) {
            return Err("R1 weather choice is unsupported".into());
        }
        let players = room
            .players
            .iter()
            .map(|p| {
                Ok(PlayerConfig {
                    name: p.name.clone(),
                    team: i32::try_from(p.team).map_err(|_| "invalid R1 team")?,
                    faction: Faction::try_from(
                        u8::try_from(p.faction).map_err(|_| "invalid R1 faction")?,
                    )
                    .map_err(|e| e.to_string())?,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let state = crate::authoritative_maps::create_match(
            &room.map,
            MatchOptions {
                seed: trusted.seed,
                mode: Mode::Conquest,
                players,
                shuffle: true,
                army: ArmySize::Standard,
                weather: WeatherChoice::from_reference(Some(&room.weather)),
                weather_seed: None,
                living_weather: true,
                map_key: Some(room.map.clone()),
                tech: true,
                supply: true,
                logistics: false,
                sandbox: false,
            },
        )
        .map_err(|e| e.to_string())?;
        let mut prepared = self.prepare_native(code, state, now, restart)?;
        prepared.attached.receipt =
            Some(trusted.receipt(prepared.attached.runtime.state(), prepared.attached.epoch));
        Ok(self.publish_native(code, prepared))
    }

    fn trusted_command(&mut self, code: &str, connection_id: u64, msg: &Value, now: u64) -> bool {
        if self.trusted.as_ref().is_none_or(|p| p.code != code) {
            return false;
        }
        let Some(command) = msg["t"].as_str() else {
            return false;
        };
        let result = match command {
            "start" | "restart" => self
                .start_trusted(code, connection_id, command == "restart", now)
                .map(|_| ()),
            "addAi" | "level" | "spectate" | "sit" => Err("R1 admits human commanders only".into()),
            _ => return false,
        };
        if let Err(error) = result {
            if let Some(person) = self.rooms.get_mut(code).and_then(|room| {
                room.players
                    .iter_mut()
                    .find(|p| p.connection.as_ref().is_some_and(|c| c.id == connection_id))
            }) {
                person.deny(
                    command,
                    if matches!(command, "start" | "restart") {
                        "scenario"
                    } else {
                        "blocked"
                    },
                    now,
                );
            }
            let _ = writeln!(
                std::io::stderr().lock(),
                "NATIVE_REFUSED {}",
                json!({"room":code,"command":command,"error":error})
            );
        }
        true
    }

    fn admit_trusted_hello(&self, code: &str, msg: &Value) -> Result<(), &'static str> {
        let Some(trusted) = &self.trusted else {
            return Ok(());
        };
        if code != trusted.code
            || msg["listing"]["public"] == true
            || msg["listing"]["tutorial"] == true
        {
            return Err("trusted room is private and code-bound");
        }
        let token = msg["token"]
            .as_str()
            .unwrap()
            .encode_utf16()
            .take(40)
            .collect::<Vec<_>>();
        if token.is_empty()
            || msg["spectate"] == true
            || msg.get("role").is_some_and(|role| role != "commander")
        {
            return Err("trusted room requires a commander seat token");
        }
        if let Some(room) = self.rooms.get(code)
            && !room.players.iter().any(|p| p.token == token)
            && (room.native.is_some() || room.players.len() >= 2)
        {
            return Err("trusted commander roster is fixed");
        }
        Ok(())
    }
    pub fn message(
        &mut self,
        code: &str,
        connection: &Connection,
        joined: &mut bool,
        msg: Value,
        now: u64,
    ) {
        if self.is_stopped() || !msg.is_object() {
            return;
        }
        if *joined {
            if self.trusted_command(code, connection.id, &msg, now) {
                return;
            }
            if msg["t"] == "resync"
                && let Some(room) = self.rooms.get(code)
                && let Some(native) = &room.native
                && let Some(slot) = room
                    .players
                    .iter()
                    .position(|p| p.connection.as_ref().is_some_and(|c| c.id == connection.id))
                && let Some(account) = self
                    .accounts
                    .get_mut(&connection.id)
                    .filter(|a| a.room == code && a.epoch == native.epoch && a.slot == slot)
            {
                account.delivery.request_full();
                return;
            }
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
        if let Err(reason) = self.admit_trusted_hello(code, &msg) {
            connection.send(json!({"t":"deny","cmd":"join","reason":"blocked"}));
            connection.close(1008, reason);
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
                native: None,
                result: None,
            }
        });
        let token: Vec<_> = msg["token"]
            .as_str()
            .unwrap()
            .encode_utf16()
            .take(40)
            .collect();
        let name = clean(msg["name"].as_str().unwrap(), 16, "Soldier", false);
        // Prepare the returning seat before replacing its socket or transport ledgers.
        let returning = room
            .players
            .iter()
            .position(|p| !token.is_empty() && p.token == token);
        let mut prepared_start = None;
        if let Some(slot) = returning
            && let Some(native) = &mut room.native
        {
            let mut names = room
                .players
                .iter()
                .map(|p| p.name.clone())
                .collect::<Vec<_>>();
            names[slot] = name.clone();
            match native
                .runtime
                .collect_starts(&[slot], &native.startup, &names, native.epoch)
            {
                Ok(mut starts) => {
                    let (_, start) = starts.pop().expect("one admitted returning seat");
                    prepared_start = Some((
                        start,
                        native
                            .runtime
                            .paused()
                            .then(|| native.runtime.pause_message(now)),
                    ));
                }
                Err(error) => {
                    native.runtime.fault_stop(error);
                    connection.send(json!({"t":"deny","cmd":"join","reason":"blocked"}));
                    connection.close(1008, "native start unavailable");
                    return;
                }
            }
        }
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
            if room.native.is_some() {
                connection.send(json!({"t":"deny","cmd":"join","reason":"blocked"}));
                connection.close(1008, "native roster fixed");
                return;
            }
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
                rtt: None,
                last_match: None,
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
        if let Some(native) = &mut room.native {
            let slot = room
                .players
                .iter()
                .position(|p| p.connection.as_ref().is_some_and(|c| c.id == connection.id))
                .unwrap();
            SnapshotAccount::start_stream(
                &mut self.accounts,
                connection.id,
                code,
                native.epoch,
                slot,
            );
        }
        *joined = true;
        room.empty_since = None;
        if let Some((start, pause)) = prepared_start {
            connection.send(start);
            if let Some(pause) = pause {
                connection.send(pause);
            }
            self.accounts.get_mut(&connection.id).unwrap().full_terrain = false;
            let slot = returning.expect("admitted returning commander");
            let native = room.native.as_mut().unwrap();
            match native.runtime.resume_drop(slot, now) {
                Ok(Some(resume)) => {
                    for person in &room.players {
                        if let Some(connection) = &person.connection {
                            connection.send(resume.clone());
                        }
                    }
                }
                Ok(None) => {}
                Err(error) => native.runtime.fault_stop(error),
            }
        }
        // Source hello does not await lobby's map-list read, so reconnect start
        // and drop-resume are emitted before the later lobby broadcast.
        room.broadcast(&self.maps, &self.public_url);
        if msg["listing"]["tutorial"] == true && room.mode == "tutorial" {
            connection.send(json!({"t":"deny","cmd":"start","reason":"scenario"}));
        }
    }
    pub fn disconnected(&mut self, code: &str, id: u64, now: u64) {
        let active = !self.is_stopped();
        let Some(room) = self.rooms.get_mut(code) else {
            return;
        };
        let mut changed = false;
        let mut pauses = Vec::new();
        for (slot, p) in room.players.iter_mut().enumerate() {
            if p.connection.as_ref().is_some_and(|c| c.id == id) {
                p.connection = None;
                p.disconnected = Some(now);
                changed = true;
                if active
                    && let Some(native) = &mut room.native
                    && native.runtime.fault().is_none()
                    && native.runtime.held() < 120
                {
                    match native.runtime.auto_pause(slot, &p.name, now) {
                        Ok(Some(message)) => pauses.push(message),
                        Ok(None) => {}
                        Err(error) => native.runtime.fault_stop(error),
                    }
                }
            }
        }
        let size = room.spectators.len();
        room.spectators
            .retain(|p| !p.connection.as_ref().is_some_and(|c| c.id == id));
        changed |= size != room.spectators.len();
        if changed {
            for message in pauses {
                for person in room.people() {
                    if let Some(connection) = &person.connection {
                        connection.send(message.clone());
                    }
                }
            }
            if !room.people().any(|p| p.connection.is_some()) {
                room.empty_since = Some(now);
            }
            room.broadcast(&self.maps, &self.public_url);
        }
    }
    pub fn expire(&mut self, now: u64) {
        if self.is_stopped() {
            return;
        }
        self.rooms.retain(|_, room| {
            if room
                .empty_since
                .is_some_and(|at| now.saturating_sub(at) > 60_000)
            {
                return false;
            }
            if room.native.is_some() {
                return true;
            }
            let size = room.players.len();
            room.retain_seats(|_, p| {
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

fn result_number(value: f64) -> Value {
    if value == 0.0 { json!(0) } else { json!(value) }
}
fn result_message(result: MatchResult, names: Vec<String>) -> Result<Value, String> {
    if result.mode != "conquest" {
        return Err("result serializer requires represented ordinary mode".into());
    }
    let timeline = result.timeline.into_iter().map(|sample| match sample {
        MatchSample::Conquest { t, vp } => Ok(json!({"t":result_number(t), "vp":vp.into_iter().map(result_number).collect::<Vec<_>>()})),
        MatchSample::Territories { .. } => Err("ordinary result cannot serialize a territory timeline".to_owned()),
    }).collect::<Result<Vec<_>, _>>()?;
    let story = result
        .story
        .into_iter()
        .map(|row| {
            StoryKey::ALL
                .into_iter()
                .map(|key| (key.as_str().to_owned(), result_number(row.value(key))))
                .collect::<serde_json::Map<String, Value>>()
        })
        .collect::<Vec<_>>();
    Ok(
        json!({"winner":result.winner,"teams":result.teams,"names":names,
        "reason":result.reason.as_str(),"at":{"x":result_number(result.at.x),"z":result_number(result.at.z)},
        "endTick":result.end_tick,"story":story,"timeline":timeline,
        "winVp":result.win_vp.map(result_number),"mode":result.mode}),
    )
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

#[cfg(test)]
#[path = "native_lobby_tests.rs"]
pub(crate) mod native_tests;

#[cfg(test)]
#[path = "native_startup_tests.rs"]
mod native_startup_tests;

#[cfg(test)]
#[path = "native_ingress_lobby_tests.rs"]
mod native_ingress_tests;

#[cfg(test)]
#[path = "trusted_room_lobby_tests.rs"]
mod trusted_room_tests;

#[cfg(test)]
#[path = "lobby_socket_tests.rs"]
mod lobby_socket_tests;

#[cfg(test)]
#[path = "net_batch50_lobby_tests.rs"]
mod net_batch50;
