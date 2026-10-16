//! Prototype operative membership and its private recipient stream.
use super::*;
use ww2_core::recipient_snapshot::OperativeDelivery;

pub(super) struct Operative {
    pub(super) person: Person,
    key: String,
    pub(super) commander: Option<usize>,
    pub(super) input_at: u64,
    pub(super) stream: Option<OperativeDelivery>,
    pub(super) net: SnapshotDelivery,
}
impl Operative {
    fn key(&self) -> String {
        self.key.clone()
    }
}
impl Room {
    pub(super) fn operative_lobby(&self, id: u64, message: &mut Value) {
        message["operatives"] = json!(
            self.operatives
                .iter()
                .map(|op| json!({
                    "name":op.person.name,"commander":op.commander.map_or(-1, |i| i as i64),
                    "online":op.person.connection.as_ref().is_some_and(Connection::ready)
                }))
                .collect::<Vec<_>>()
        );
        if let Some(op) = self
            .operatives
            .iter()
            .find(|op| op.person.connection.as_ref().is_some_and(|c| c.id == id))
        {
            message["role"] = json!("operative");
            message["commander"] = json!(op.commander.map_or(-1, |i| i as i64));
            message["spectator"] = json!(false);
            message["amHost"] = json!(false);
        }
    }
    pub(super) fn operative_hello(
        &mut self,
        connection: &Connection,
        message: &Value,
        token: &[u16],
        name: &str,
        joined: &mut bool,
        now: u64,
    ) -> bool {
        let returning = self
            .operatives
            .iter()
            .position(|op| !token.is_empty() && op.person.token == token);
        // Token identity takes precedence over the requested role.
        if returning.is_none()
            && (message["role"] != "operative"
                || self
                    .players
                    .iter()
                    .chain(&self.spectators)
                    .any(|p| !token.is_empty() && p.token == token))
        {
            return false;
        }
        let index = if let Some(index) = returning {
            let person = &mut self.operatives[index].person;
            if let Some(old) = person.connection.replace(connection.clone()) {
                old.send(json!({"t":"replaced"}));
                old.close(1000, "");
            }
            person.name = name.into();
            person.disconnected = None;
            index
        } else {
            let commander = integer(&message["commander"])
                .and_then(|slot| usize::try_from(slot).ok())
                .filter(|slot| self.players.get(*slot).is_some_and(|p| !p.ai));
            if commander.is_none()
                || token.is_empty()
                || self.operatives.len() >= 4
                || self.mode == "world"
            {
                connection.send(json!({"t":"deny","cmd":"join","reason":"commander"}));
                connection.close(
                    1008,
                    "operative needs human commander; world mode unsupported",
                );
                return true;
            }
            // Truncation may retain an unmatched UTF-16 surrogate. Distinct
            // person tokens must keep distinct core records after UTF-8 conversion.
            let mut key = String::from_utf16_lossy(token);
            while self.operatives.iter().any(|operative| operative.key == key) {
                key.push('\0');
            }
            self.operatives.push(Operative {
                key,
                person: Person {
                    token: token.to_vec(),
                    name: name.into(),
                    connection: Some(connection.clone()),
                    ai: false,
                    team: 0,
                    faction: 0,
                    level: "normal",
                    disconnected: None,
                    denies: Vec::new(),
                    rtt: None,
                    map_pings: Vec::new(),
                    diagnostics: crate::diagnostics::PlayerDiagnostics::default(),
                    last_match: None,
                },
                commander,
                input_at: 0,
                stream: None,
                net: SnapshotDelivery::default(),
            });
            self.operatives.len() - 1
        };
        *joined = true;
        self.empty_since = None;
        if self.native.is_some()
            && let Err(error) = self.start_operative(index, now)
        {
            self.native.as_mut().unwrap().runtime.fault_stop(error);
        }
        true
    }
    pub(super) fn start_operative(&mut self, index: usize, now: u64) -> Result<(), String> {
        let op = &mut self.operatives[index];
        let Some(slot) = op.commander.filter(|_| self.mode != "world") else {
            op.stream = None;
            return Ok(());
        };
        let native = self
            .native
            .as_mut()
            .ok_or("operative start without match")?;
        let key = op.key();
        let state = native.runtime.state_mut();
        let finished = state.finish.is_some();
        if state.join_companion(&key, slot, finished)?.is_none() {
            op.stream = None;
            return Ok(());
        }
        state
            .companions
            .get_mut(&key)
            .unwrap()
            .reconnect(op.person.connection.as_ref().is_some_and(Connection::ready));
        op.input_at = 0;
        op.net.reset();
        op.net.request_full();
        let mut stream = OperativeDelivery::new(state, slot)
            .map_err(|error| format!("operative delivery admission: {error:?}"))?;
        let snapshot = stream
            .capture(state, true, &[])
            .map_err(|error| format!("operative start delivery: {error:?}"))?;
        let names = self
            .players
            .iter()
            .map(|person| person.name.clone())
            .collect::<Vec<_>>();
        let envelope = native.startup.envelope(state, &names, native.epoch)?;
        let mut start = envelope.message(slot, snapshot);
        start.as_object_mut().unwrap().remove("sandbox");
        start["role"] = json!("operative");
        start["operative"] = state.companion_status(&key);
        op.stream = Some(stream);
        if let Some(connection) = &op.person.connection {
            connection.send(start);
            if native.runtime.paused() {
                connection.send(native.runtime.pause_message(now));
            }
        }
        Ok(())
    }
    /// A current operative socket consumes its entire whitelist here.
    pub(super) fn operative_message(&mut self, id: u64, msg: &Value, now: u64) -> Option<bool> {
        let op = self.operatives.iter_mut().find(|op| {
            op.person
                .connection
                .as_ref()
                .is_some_and(|connection| connection.id == id)
        })?;
        let connection = op.person.connection.as_ref().unwrap();
        if !connection.ready() {
            return Some(false);
        }
        match msg["t"].as_str() {
            Some("fps") => {
                if let Some(native) = &mut self.native
                    && !native.runtime.paused()
                    && native.runtime.fault().is_none()
                    && now.saturating_sub(op.input_at) >= 20
                {
                    op.input_at = now;
                    let key = op.key();
                    let state = native.runtime.state_mut();
                    state.companion_input(&key, msg, state.finish.is_some());
                }
            }
            Some("resync") if op.stream.is_some() => op.net.request_full(),
            Some("name") => {
                if let Some(name) = msg["name"].as_str() {
                    op.person.name = clean(name, 16, "Soldier", false);
                    return Some(true);
                }
            }
            Some("ping") if msg.get("x").is_none() && msg.get("z").is_none() => {
                if let Some(rtt) = msg["rtt"].as_f64().filter(|n| n.is_finite()) {
                    op.person.rtt = Some(rtt.round().clamp(0.0, 9999.0));
                }
                let mut pong = json!({"t":"pong"});
                if let Some(value) = msg.get("c") {
                    pong["c"] = value.clone();
                }
                if let Some(native) = &self.native {
                    let stats = native.runtime.timing_stats();
                    pong["srv"] = json!([
                        (stats.snapshot_tick.p95 * 10.0).round() / 10.0,
                        native.runtime.snapshot_cadence()
                    ]);
                }
                connection.send(pong);
            }
            _ => {}
        }
        Some(false)
    }
    pub(super) fn disconnect_operative(&mut self, id: u64) -> bool {
        let Some(op) = self.operatives.iter_mut().find(|op| {
            op.person
                .connection
                .as_ref()
                .is_some_and(|connection| connection.id == id)
        }) else {
            return false;
        };
        op.person.connection = None;
        if let Some(native) = &mut self.native
            && let Some(companion) = native.runtime.state_mut().companions.get_mut(&op.key())
        {
            companion.disconnect();
        }
        true
    }
    pub(super) fn send_operatives(
        &mut self,
        events: &[ww2_core::match_tick::TickEvent],
        online: &[bool],
        ping: &[Option<f64>],
    ) -> Result<(), String> {
        if !self
            .operatives
            .iter()
            .any(|op| op.stream.is_some() && op.person.online())
        {
            return Ok(());
        }
        let native = self
            .native
            .as_mut()
            .ok_or("operative frame without match")?;
        let scope = native.runtime.operative_list_scope()?;
        for op in &mut self.operatives {
            let key = op.key();
            let Some(connection) = op.person.connection.as_ref().filter(|c| c.ready()) else {
                continue;
            };
            let Some(stream) = &mut op.stream else {
                continue;
            };
            let state = native.runtime.state_mut();
            let mut message = stream
                .capture(state, false, events)
                .map_err(|error| format!("operative frame: {error:?}"))?;
            message["operative"] = state.companion_status(&key);
            message["online"] = json!(online);
            message["ping"] = json!(ping);
            for key in [
                "movement",
                "queues",
                "productionJobs",
                "plans",
                "orders",
                "air",
                "rally",
                "works",
                "covers",
                "sup",
            ] {
                message.as_object_mut().unwrap().remove(key);
            }
            let message = op
                .net
                .trim(scope, message)
                .map_err(|e| format!("operative delta: {e}"))?;
            connection.send(message);
        }
        Ok(())
    }
}
