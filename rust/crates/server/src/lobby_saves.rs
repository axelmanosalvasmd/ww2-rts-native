//! Save admission and transactional restoration for ordinary human rooms.
use super::*;

impl Hub {
    pub(super) fn save_command(
        &mut self,
        code: &str,
        connection_id: u64,
        message: &Value,
        now: u64,
    ) -> bool {
        if message["t"] == "resume"
            && let Some(room) = self.rooms.get_mut(code)
            && room
                .native
                .as_ref()
                .is_some_and(|native| native.loaded_save)
        {
            if room.is_host(connection_id) {
                let resumed = room.native.as_mut().unwrap().runtime.resume(now);
                for person in room.people() {
                    if let Some(connection) = &person.connection {
                        connection.send(resumed.clone());
                    }
                }
            } else if let Some(person) = room
                .players
                .iter_mut()
                .find(|p| p.connection.as_ref().is_some_and(|c| c.id == connection_id))
            {
                person.deny("resume", "blocked", now);
            }
            return true;
        }
        let Some(command @ ("save" | "loadSave")) = message["t"].as_str() else {
            return false;
        };
        // Non-host spectators fall through silently in the prototype callback.
        if let Some(room) = self.rooms.get(code)
            && !room.is_host(connection_id)
            && room.spectators.iter().any(|person| {
                person
                    .connection
                    .as_ref()
                    .is_some_and(|c| c.id == connection_id)
            })
        {
            return true;
        }
        let result = if command == "save" {
            self.save_room(code, connection_id).map(|id| {
                if let Some(room) = self.rooms.get(code) {
                    if let Some(connection) = room
                        .people()
                        .filter_map(|p| p.connection.as_ref())
                        .find(|c| c.id == connection_id)
                    {
                        connection.send(json!({"t":"saved","id":id}));
                    }
                    room.broadcast(&self.maps, &self.public_url);
                }
            })
        } else {
            self.load_room(
                code,
                connection_id,
                message["id"].as_str().unwrap_or(""),
                now,
            )
            .map(|_| ())
        };
        if result.is_err()
            && let Some(room) = self.rooms.get_mut(code)
            && let Some(person) = room
                .players
                .iter_mut()
                .chain(&mut room.spectators)
                .find(|p| p.connection.as_ref().is_some_and(|c| c.id == connection_id))
        {
            person.deny(command, "save", now);
        }
        true
    }
    pub(super) fn save_room(&self, code: &str, connection_id: u64) -> Result<String, String> {
        let room = self.rooms.get(code).ok_or("room absent")?;
        if !room.is_host(connection_id) {
            return Err("host required".into());
        }
        let native = room.native.as_ref().ok_or("running match required")?;
        if native.runtime.fault().is_some() {
            return Err("faulted match".into());
        }
        let state = native.runtime.state();
        if !matches!(state.options.mode, Mode::Default | Mode::Conquest)
            || state.options.logistics
            || state.options.sandbox
            || state.world_runtime.is_some()
            || room.players.iter().any(|p| p.ai)
            || !room.spectators.is_empty()
        {
            return Err("save needs an unavailable room capability".into());
        }
        let seats = room.players.iter().map(|p| {
            let token = match String::from_utf16(&p.token) {
                Ok(token) => json!(token),
                Err(_) => json!({"utf16":p.token}),
            };
            json!({"name":p.name.strip_suffix(" (AI)").unwrap_or(&p.name),"token":token,"ai":p.ai,"level":if p.ai {json!(p.level)} else {Value::Null},"team":p.team,"faction":p.faction})
        }).collect::<Vec<_>>();
        let settings = json!({"mode":room.mode,"worldSize":room.world_size,"mapName":room.map,
            "mapSpawns":native.startup.raw_map()["spawns"],"army":room.army,"weather":room.weather,
            "defenderTeam":room.defender,"logistics":state.options.logistics,"tech":state.options.tech});
        let save = json!({"version":crate::saves::VERSION,"g":native.runtime.save_json()?,"guard":null,
            "map":native.startup.raw_map(),"seats":seats,"settings":settings,"ai":null});
        room.saves.write(
            code,
            "manual",
            &save,
            json!({"mode":room.mode,"minutes":state.world.game.tick/1200,
            "names":seats.iter().map(|p|p["name"].clone()).collect::<Vec<_>>()}),
            crate::saves::unix_millis()?,
        )
    }
    pub(super) fn load_room(
        &mut self,
        code: &str,
        connection_id: u64,
        id: &str,
        now: u64,
    ) -> Result<u64, String> {
        if self.is_stopped() {
            return Err("server stopped".into());
        }
        let room = self.rooms.get(code).ok_or("room absent")?;
        if room.native.is_some() || !room.is_host(connection_id) {
            return Err("host in lobby required".into());
        }
        let saved = room.saves.read(code, id)?;
        // Closed room capabilities remain closed even when a file is valid JSON.
        if saved["settings"]["mode"] != "conquest"
            || !saved["guard"].is_null()
            || !saved["ai"].is_null()
        {
            return Err("save needs an unavailable room capability".into());
        }
        let state = Match::from_save_json(saved["g"]["state"].clone())?;
        if !matches!(state.options.mode, Mode::Default | Mode::Conquest)
            || state.options.logistics
            || state.options.sandbox
            || state.world_runtime.is_some()
        {
            return Err("save needs an unavailable room capability".into());
        }
        let seats = saved["seats"]
            .as_array()
            .filter(|s| !s.is_empty() && s.len() <= MAX_PLAYERS)
            .ok_or("invalid saved roster")?;
        let present = room
            .players
            .iter()
            .chain(&room.spectators)
            .filter(|p| !p.ai)
            .collect::<Vec<_>>();
        let tokens = seats
            .iter()
            .map(|seat| {
                if seat["ai"] != false {
                    return Err("AI room loading unavailable".to_owned());
                }
                if let Some(text) = seat["token"].as_str() {
                    return Ok(text.encode_utf16().collect::<Vec<_>>());
                }
                seat["token"]["utf16"]
                    .as_array()
                    .ok_or("invalid saved token".to_owned())?
                    .iter()
                    .map(|v| {
                        v.as_u64()
                            .and_then(|v| u16::try_from(v).ok())
                            .ok_or("invalid token code unit".to_owned())
                    })
                    .collect()
            })
            .collect::<Result<Vec<_>, String>>()?;
        let mut owners = vec![None; seats.len()];
        let mut used = std::collections::BTreeSet::new();
        // Finish every token match before assigning any connected fallback.
        for (slot, token) in tokens.iter().enumerate() {
            if let Some((index, _)) = present
                .iter()
                .enumerate()
                .find(|(index, p)| !token.is_empty() && p.token == *token && !used.contains(index))
            {
                owners[slot] = Some(index);
                used.insert(index);
            }
        }
        for owner in &mut owners {
            if owner.is_none()
                && let Some((index, _)) = present.iter().enumerate().find(|(index, p)| {
                    p.connection.as_ref().is_some_and(Connection::ready) && !used.contains(index)
                })
            {
                *owner = Some(index);
                used.insert(index);
            }
        }
        if present.iter().enumerate().any(|(index, p)| {
            !used.contains(&index) && p.connection.as_ref().is_some_and(Connection::ready)
        }) {
            return Err("spectator runtime admission unavailable".into());
        }
        let mut players = Vec::new();
        for (slot, seat) in seats.iter().enumerate() {
            let team = seat["team"]
                .as_u64()
                .and_then(|v| usize::try_from(v).ok())
                .filter(|v| *v < MAX_PLAYERS)
                .ok_or("invalid saved team")?;
            let faction = seat["faction"]
                .as_u64()
                .and_then(|v| usize::try_from(v).ok())
                .filter(|v| *v < 4)
                .ok_or("invalid saved faction")?;
            let mut person = if let Some(owner) = owners[slot] {
                present[owner].clone()
            } else {
                Person {
                    token: tokens[slot].clone(),
                    name: seat["name"].as_str().ok_or("invalid saved name")?.into(),
                    connection: None,
                    ai: false,
                    team,
                    faction,
                    level: "normal",
                    disconnected: None,
                    denies: Vec::new(),
                    rtt: None,
                    diagnostics: Default::default(),
                    last_match: None,
                }
            };
            person.team = team;
            person.faction = faction;
            person.disconnected = None;
            person.last_match = None;
            players.push(person);
        }
        let settings = &saved["settings"];
        let text = |key: &str| {
            settings[key]
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| format!("invalid saved setting {key}"))
        };
        if settings["logistics"] != state.options.logistics
            || settings["tech"] != state.options.tech
            || settings["mapSpawns"] != saved["map"]["spawns"]
        {
            return Err("saved settings disagree with the match".into());
        }
        let staged = Room {
            code: room.code.clone(),
            saves: room.saves.clone(),
            listed: room.listed,
            title: room.title.clone(),
            players,
            spectators: Vec::new(),
            mode: text("mode")?,
            map: text("mapName")?,
            loaded_spawns: Some(settings["mapSpawns"].clone()),
            world_size: text("worldSize")?,
            army: text("army")?,
            weather: text("weather")?,
            sandbox: state.options.sandbox,
            defender: settings["defenderTeam"]
                .as_u64()
                .and_then(|v| usize::try_from(v).ok())
                .filter(|v| *v < MAX_PLAYERS)
                .ok_or("invalid saved defender")?,
            empty_since: None,
            native: None,
            result: None,
        };
        let mut prepared =
            self.prepare_native_room(&staged, state, now, false, Some(&saved["map"]))?;
        prepared
            .attached
            .runtime
            .restore_saved_hold(&saved["g"]["held"])?;
        prepared
            .attached
            .runtime
            .restore_saved_events(saved["g"]["pendingEvents"].clone())?;
        let host = staged
            .players
            .iter()
            .position(|p| p.connection.as_ref().is_some_and(|c| c.id == connection_id))
            .ok_or("host did not receive a saved seat")?;
        // Replace a missing-seat drop pause with the prototype's indefinite host pause.
        prepared
            .attached
            .runtime
            .open_loaded_pause(host, &staged.players[host].name, now)?;
        prepared.pause = Some(prepared.attached.runtime.pause_message(now));
        prepared.attached.loaded_save = true;
        *self.rooms.get_mut(code).unwrap() = staged;
        Ok(self.publish_native(code, prepared))
    }
}
