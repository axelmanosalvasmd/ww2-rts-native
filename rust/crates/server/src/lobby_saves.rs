//! Save admission and transactional restoration for ordinary AI and Horde human rooms.
use super::*;

fn saved_mode_supported(mode: &str, state: &Match, seats: usize) -> bool {
    match mode {
        "conquest" => matches!(state.options.mode, Mode::Default | Mode::Conquest),
        "horde" => {
            state.options.mode == Mode::Horde
                && (1..=5).contains(&seats)
                && state.horde.as_ref().is_some_and(|h| h.slot == seats)
        }
        _ => false,
    }
}

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
        if room.starting.is_some() || !room.is_host(connection_id) {
            return Err("host required".into());
        }
        let native = room.native.as_ref().ok_or("running match required")?;
        if native.runtime.fault().is_some() {
            return Err("faulted match".into());
        }
        let state = native.runtime.state();
        if !saved_mode_supported(&room.mode, state, room.players.len())
            || (state.options.sandbox && state.options.mode != Mode::Horde)
            || state.world_runtime.is_some()
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
            "map":native.startup.raw_map(),"seats":seats,"settings":settings,"ai":native.runtime.save_ai_memories()});
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
        if room.native.is_some() || room.starting.is_some() || !room.is_host(connection_id) {
            return Err("host in lobby required".into());
        }
        let saved = room.saves.read(code, id)?;
        // Closed room capabilities remain closed even when a file is valid JSON.
        if !matches!(
            saved["settings"]["mode"].as_str(),
            Some("conquest" | "horde")
        ) || !saved["guard"].is_null()
        {
            return Err("save needs an unavailable room capability".into());
        }
        let state = Match::from_save_json(saved["g"]["state"].clone())?;
        if state.options.sandbox && state.options.mode != Mode::Horde
            || state.world_runtime.is_some()
        {
            return Err("save needs an unavailable room capability".into());
        }
        let seats = saved["seats"]
            .as_array()
            .filter(|s| !s.is_empty() && s.len() <= MAX_PLAYERS)
            .ok_or("invalid saved roster")?;
        if !saved_mode_supported(
            saved["settings"]["mode"].as_str().unwrap(),
            &state,
            seats.len(),
        ) {
            return Err("saved mode disagrees with the match".into());
        }
        let present = room
            .players
            .iter()
            .chain(&room.spectators)
            .filter(|p| !p.ai)
            .collect::<Vec<_>>();
        let tokens = seats
            .iter()
            .map(|seat| {
                if !seat["ai"].is_boolean() {
                    return Err("invalid saved seat role".to_owned());
                }
                if seat["ai"] == true {
                    return Ok(Vec::new());
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
            if seats[slot]["ai"] == true {
                continue;
            }
            if let Some((index, _)) = present
                .iter()
                .enumerate()
                .find(|(index, p)| !token.is_empty() && p.token == *token && !used.contains(index))
            {
                owners[slot] = Some(index);
                used.insert(index);
            }
        }
        for (slot, owner) in owners.iter_mut().enumerate() {
            if seats[slot]["ai"] == true {
                continue;
            }
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
                    ai: seat["ai"] == true,
                    team,
                    faction,
                    level: if seat["ai"] == true {
                        match &seat["level"] {
                            Value::Null => "normal",
                            value => match value.as_str() {
                                Some("normal") => "normal",
                                Some("easy") => "easy",
                                Some("hard") => "hard",
                                _ => return Err("invalid saved AI level".into()),
                            },
                        }
                    } else {
                        "normal"
                    },
                    disconnected: None,
                    denies: Vec::new(),
                    rtt: None,
                    map_pings: Vec::new(),
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
        // The prototype retains each operative's commander object across load.
        // Resolve that identity through the same present-person assignment used
        // for saved seats; a departed commander must stay unassigned.
        let operative_commanders = room
            .operatives
            .iter()
            .map(|op| {
                let commander = room.players.get(op.commander?)?;
                let present_index = present.iter().position(|p| std::ptr::eq(*p, commander))?;
                owners
                    .iter()
                    .position(|owner| *owner == Some(present_index))
            })
            .collect::<Vec<_>>();
        let mut staged = Room {
            code: room.code.clone(),
            saves: room.saves.clone(),
            records: room.records.clone(),
            listed: room.listed,
            title: room.title.clone(),
            players,
            spectators: Vec::new(),
            operatives: Vec::new(),
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
            starting: None,
            result: None,
        };
        // prepare_native_room rebuilds the internal Horde brain from the loaded
        // recipient view. The canonical wave, RNG and recorded inputs stay saved.
        // Prototype saves.js:20-21 also permits omitted AI memories to start afresh.
        let mut prepared =
            self.prepare_native_room(&staged, state, now, false, Some(&saved["map"]))?;
        prepared
            .attached
            .runtime
            .restore_ai_memories(&saved["ai"])?;
        let slots = (0..staged.players.len()).collect::<Vec<_>>();
        let names = prepared
            .attached
            .runtime
            .state()
            .start
            .players
            .iter()
            .enumerate()
            .map(|(slot, player)| {
                staged
                    .players
                    .get(slot)
                    .map_or_else(|| player.config.name.clone(), |person| person.name.clone())
            })
            .collect::<Vec<_>>();
        prepared.starts = prepared.attached.runtime.collect_starts(
            &slots,
            &prepared.attached.startup,
            &names,
            prepared.attached.epoch,
        )?;
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
        // Move membership only after every fallible restoration step succeeds.
        staged.operatives = std::mem::take(&mut self.rooms.get_mut(code).unwrap().operatives);
        for (operative, commander) in staged.operatives.iter_mut().zip(operative_commanders) {
            operative.commander = commander;
        }
        *self.rooms.get_mut(code).unwrap() = staged;
        Ok(self.publish_native(code, prepared))
    }
}

#[cfg(test)]
#[path = "horde_save_tests.rs"]
mod horde_tests;
