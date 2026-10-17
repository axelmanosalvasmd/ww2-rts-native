//! Transactional asynchronous R1 match generation over the existing startup owner.
use super::*;

pub(super) struct PendingStart {
    token: Arc<()>,
    connections: Vec<Option<u64>>,
    operatives: Vec<(Vec<u16>, Option<u64>)>,
    spectators: Vec<(Vec<u16>, u64)>,
}

#[derive(Clone)]
pub(crate) struct StartRequest {
    token: Arc<()>,
    map: String,
    options: MatchOptions,
    restart: bool,
    defender: i32,
    raw_map: Option<Value>,
}

impl StartRequest {
    pub(crate) fn generate(&self) -> Result<Match, String> {
        let edited = self
            .raw_map
            .as_ref()
            .map(|raw| {
                ww2_core::map::AuthoritativeMap::from_json(
                    &serde_json::to_vec(raw).map_err(|e| e.to_string())?,
                )
                .map_err(|e| e.to_string())
            })
            .transpose()?;
        let map = if let Some(map) = &edited {
            map
        } else {
            crate::authoritative_maps::load(&self.map).map_err(|e| e.to_string())?
        };
        let mut options = self.options.clone();
        let mode = match options.mode {
            Mode::Default | Mode::Conquest => "conquest",
            Mode::Classic => "classic",
            Mode::Annihilation => "annihilation",
            Mode::Assault => "assault",
            Mode::Tutorial => "tutorial",
            _ => "",
        };
        options.logistics &= logistics_enabled_for(mode, map.source());
        let result = match options.mode {
            Mode::Default | Mode::Conquest => Match::new(map, options),
            Mode::Assault => Match::new_assault(map, options, self.defender),
            Mode::Annihilation => Match::new_annihilation(map, options),
            Mode::Classic => Match::new_classic(map, options),
            Mode::Tutorial => Match::new_tutorial(map, options),
            _ => Err("trusted asynchronous mode is not admitted".into()),
        };
        if self.raw_map.is_none() {
            result
                .map_err(crate::authoritative_maps::AuthoredMapError::Match)
                .map_err(|error| error.to_string())
        } else {
            result
        }
    }
}

impl Hub {
    pub(super) fn trusted_start_request(
        &self,
        code: &str,
        connection_id: u64,
        restart: bool,
    ) -> Result<StartRequest, String> {
        self.trusted_start_request_with_weather_source(
            code,
            connection_id,
            restart,
            fresh_weather_seed,
        )
    }

    pub(super) fn trusted_start_request_with_weather_source(
        &self,
        code: &str,
        connection_id: u64,
        restart: bool,
        weather_seed: impl FnOnce() -> Result<u32, String>,
    ) -> Result<StartRequest, String> {
        let trusted = self.trusted.as_ref().ok_or("trusted hosting is disabled")?;
        if trusted.code != code || self.is_stopped() {
            return Err("trusted room unavailable".into());
        }
        let room = self.rooms.get(code).ok_or("trusted room absent")?;
        if !room.is_host(connection_id)
            || !room.people().any(|p| {
                p.connection
                    .as_ref()
                    .is_some_and(|c| c.id == connection_id && c.ready())
            })
        {
            return Err("only the current commander host may start or restart".into());
        }
        if room.starting.is_some() {
            return Err("match generation is already pending".into());
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
        if !room.start_ready(&self.maps) {
            return Err(
                "start requires one through capacity players and both Assault sides".into(),
            );
        }
        if room.listed
            || !matches!(
                room.mode.as_str(),
                "conquest" | "assault" | "annihilation" | "classic" | "tutorial"
            )
        {
            return Err("R1 requires a private represented ordinary mode".into());
        }
        if !matches!(
            room.army.as_str(),
            "standard" | "large" | "massive" | "endless"
        ) {
            return Err("R1 army choice is unsupported".into());
        }
        if room.players.len() != 2
            || room.players.iter().any(|p| !p.ai && p.token.is_empty())
            || room.players[0].team == room.players[1].team
            || (!room.players[0].ai
                && !room.players[1].ai
                && room.players[0].token == room.players[1].token)
        {
            return Err("R1 requires exactly two commander seats on opposing teams".into());
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
        let raw_map = self
            .map_editor
            .as_ref()
            .and_then(|editor| editor.raw(&room.map));
        // Preserve asynchronous creation failure: raw map loading belongs to generate.
        let mut options = room.native_match_options(
            trusted.seed.wrapping_add(self.next_epoch),
            players,
            raw_map.as_ref().unwrap_or(&Value::Null),
        )?;
        options.weather_seed = Some(weather_seed()?);
        Ok(StartRequest {
            token: Arc::new(()),
            map: room.map.clone(),
            restart,
            defender: room.defender as i32,
            raw_map,
            options,
        })
    }

    pub(crate) fn claim_trusted_start(
        &mut self,
        code: &str,
        connection_id: u64,
        restart: bool,
    ) -> Result<StartRequest, String> {
        let request = self.trusted_start_request(code, connection_id, restart)?;
        let room = self.rooms.get_mut(code).expect("validated room");
        room.starting = Some(PendingStart {
            token: request.token.clone(),
            connections: room
                .players
                .iter()
                .map(|person| person.connection.as_ref().map(|c| c.id))
                .collect(),
            operatives: room
                .operatives
                .iter()
                .map(|op| {
                    (
                        op.person.token.clone(),
                        op.person.connection.as_ref().map(|c| c.id),
                    )
                })
                .collect(),
            spectators: room
                .spectators
                .iter()
                .filter_map(|person| {
                    person
                        .connection
                        .as_ref()
                        .map(|connection| (person.token.clone(), connection.id))
                })
                .collect(),
        });
        Ok(request)
    }

    pub(super) fn prepare_trusted_start(
        &self,
        code: &str,
        state: Match,
        now: u64,
        restart: bool,
    ) -> Result<PreparedNative, String> {
        let mut prepared = self.prepare_native(code, state, now, restart)?;
        let trusted = self.trusted.as_ref().ok_or("trusted hosting is disabled")?;
        prepared.attached.receipt =
            Some(trusted.receipt(prepared.attached.runtime.state(), prepared.attached.epoch));
        Ok(prepared)
    }

    pub(crate) fn complete_trusted_start(
        &mut self,
        code: &str,
        request: StartRequest,
        generated: Result<Match, String>,
        now: u64,
    ) -> Result<Option<u64>, String> {
        if self.is_stopped()
            || self
                .rooms
                .get(code)
                .and_then(|room| room.starting.as_ref())
                .is_none_or(|pending| !Arc::ptr_eq(&pending.token, &request.token))
        {
            return Ok(None);
        }
        let prepared = generated
            .and_then(|state| self.prepare_trusted_start(code, state, now, request.restart));
        let pending = self.rooms.get_mut(code).unwrap().starting.take().unwrap();
        match prepared {
            Ok(prepared) => Ok(Some(self.publish_native(code, prepared))),
            Err(error) => {
                let room = self.rooms.get_mut(code).unwrap();
                let connections = room
                    .people()
                    .filter_map(|person| person.connection.clone())
                    .collect::<Vec<_>>();
                // Existing sockets retain their delivery ledgers and outstanding writes.
                // A socket replaced while loading needs the restored game's handshake.
                if let Some(native) = &mut room.native {
                    let slots = room
                        .players
                        .iter()
                        .enumerate()
                        .filter(|(slot, person)| {
                            person.connection.as_ref().is_some_and(|connection| {
                                pending.connections[*slot] != Some(connection.id)
                            })
                        })
                        .map(|(slot, _)| slot)
                        .collect::<Vec<_>>();
                    let names = room
                        .players
                        .iter()
                        .map(|person| person.name.clone())
                        .collect::<Vec<_>>();
                    if !slots.is_empty() {
                        match native.runtime.collect_starts(
                            &slots,
                            &native.startup,
                            &names,
                            native.epoch,
                        ) {
                            Ok(starts) => {
                                for (slot, start) in starts {
                                    let connection =
                                        room.players[slot].connection.as_ref().unwrap();
                                    SnapshotAccount::start_stream(
                                        &mut self.accounts,
                                        connection.id,
                                        code,
                                        native.epoch,
                                        slot,
                                    );
                                    connection.send(start);
                                    self.accounts.get_mut(&connection.id).unwrap().full_terrain =
                                        false;
                                }
                            }
                            Err(fault) => native.runtime.fault_stop(fault),
                        }
                    }
                    for (slot, person) in room.players.iter().enumerate() {
                        let message = if person.online() {
                            native.runtime.resume_drop(slot, now)
                        } else {
                            native.runtime.auto_pause(slot, &person.name, now)
                        };
                        match message {
                            Ok(Some(message)) => {
                                for connection in &connections {
                                    connection.send(message.clone());
                                }
                            }
                            Ok(None) => {}
                            Err(fault) => {
                                native.runtime.fault_stop(fault);
                                break;
                            }
                        }
                    }
                    let watchers = room
                        .spectators
                        .iter()
                        .filter_map(|person| {
                            person.connection.as_ref().filter(|connection| {
                                connection.ready()
                                    && !pending.spectators.iter().any(|(token, id)| {
                                        *token == person.token && *id == connection.id
                                    })
                            })
                        })
                        .collect::<Vec<_>>();
                    if !watchers.is_empty() {
                        match native.runtime.collect_watcher_start(
                            &native.startup,
                            &names,
                            native.epoch,
                        ) {
                            Ok(start) => {
                                // Watchers share a ledger. A reconnect requests one
                                // full next frame without resetting that ledger.
                                if native.watch_started {
                                    native.watch_delivery.request_full();
                                }
                                for connection in watchers {
                                    connection.send(start.clone());
                                    if native.runtime.paused() {
                                        connection.send(native.runtime.pause_message(now));
                                    }
                                }
                            }
                            Err(fault) => native.runtime.fault_stop(fault),
                        }
                    }
                }
                if room.native.is_some()
                    && let Err(fault) = room.restore_pending_operatives(&pending.operatives, now)
                {
                    room.native.as_mut().unwrap().runtime.fault_stop(fault);
                }
                for person in &room.players {
                    if let Some(connection) = &person.connection {
                        connection.send(json!({"t":"deny", "cmd":"start", "reason":"scenario"}));
                    }
                }
                room.broadcast(&self.maps, &self.public_url);
                Err(error)
            }
        }
    }

    pub(crate) fn refuse_trusted_start(&mut self, code: &str, id: u64, command: &str, now: u64) {
        if let Some(person) = self.rooms.get_mut(code).and_then(|room| {
            room.players.iter_mut().find(|person| {
                person
                    .connection
                    .as_ref()
                    .is_some_and(|connection| connection.id == id)
            })
        }) {
            person.deny(command, "scenario", now);
        }
    }

    pub(crate) fn uses_trusted_start(&self, code: &str) -> bool {
        self.trusted
            .as_ref()
            .is_some_and(|trusted| trusted.code == code)
            && self.rooms.get(code).is_none_or(|room| room.mode != "horde")
    }

    pub(super) fn pending_start_command(
        &mut self,
        code: &str,
        id: u64,
        msg: &Value,
        now: u64,
    ) -> bool {
        let Some(room) = self
            .rooms
            .get_mut(code)
            .filter(|room| room.starting.is_some())
        else {
            return false;
        };
        if msg["t"] != "end" {
            return false;
        }
        let host = room.is_host(id)
            && room.players.iter().any(|person| {
                person
                    .connection
                    .as_ref()
                    .is_some_and(|connection| connection.id == id && connection.ready())
            });
        if host {
            if let Err(error) = room.end_native(now)
                && let Some(native) = &mut room.native
            {
                native.runtime.fault_stop(error);
            }
            room.broadcast(&self.maps, &self.public_url);
        } else if let Some(person) = room.players.iter_mut().find(|person| {
            person
                .connection
                .as_ref()
                .is_some_and(|connection| connection.id == id)
        }) {
            person.deny("end", "blocked", now);
        }
        true
    }
}
