//! Hosted Horde admission uses the canonical core and the existing JSON v0 lifecycle.
use super::*;
use ww2_core::map::AuthoritativeMap;

const LOST_DEFEND: &str = "Horde map lost its defender spawns";

impl Hub {
    pub(crate) fn configure_authored_maps(&mut self, dir: Option<std::path::PathBuf>) {
        self.authored_maps_dir = dir;
    }

    fn horde_map(&self, key: &str) -> Result<AuthoritativeMap, String> {
        // Catalogue membership bounds the path. Files are private host input, never HTTP data.
        crate::authoritative_maps::load(key).map_err(|e| e.to_string())?;
        if let Some(dir) = &self.authored_maps_dir {
            let bytes = std::fs::read(dir.join(format!("{key}.json")))
                .map_err(|e| format!("Horde map reread: {e}"))?;
            AuthoritativeMap::from_json(&bytes).map_err(|e| e.to_string())
        } else {
            Ok(crate::authoritative_maps::load(key)
                .map_err(|e| e.to_string())?
                .clone())
        }
    }

    fn start_horde(&mut self, code: &str, id: u64, restart: bool, now: u64) -> Result<u64, String> {
        if self.is_stopped() {
            return Err("server stopped".into());
        }
        let room = self.rooms.get(code).ok_or("room absent")?;
        if room.mode != "horde"
            || !room.is_host(id)
            || !room.players.iter().any(|p| {
                !p.ai
                    && p.connection
                        .as_ref()
                        .is_some_and(|c| c.id == id && c.ready())
            })
        {
            return Err("Horde start needs the current commander host".into());
        }
        if restart {
            let native = room
                .native
                .as_ref()
                .ok_or("restart requires an attached match")?;
            if native.runtime.fault().is_some() {
                return Err("faulted match restart awaits a recovery policy".into());
            }
        } else if room.native.is_some() {
            return Err("start requires lobby state".into());
        }
        if !(1..=5).contains(&room.players.len()) || !room.spectators.is_empty() {
            return Err("Horde needs one through five commanders without spectators".into());
        }
        let map = self.horde_map(&room.map)?;
        if map.defend().is_none_or(|spawns| spawns.is_empty()) {
            return Err(LOST_DEFEND.into());
        }
        let players = room
            .players
            .iter()
            .map(|p| {
                Ok(PlayerConfig {
                    name: p.name.clone(),
                    team: 0,
                    faction: Faction::try_from(
                        u8::try_from(p.faction).map_err(|_| "invalid Horde faction")?,
                    )
                    .map_err(|e| e.to_string())?,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let army = match room.army.as_str() {
            "standard" => ArmySize::Standard,
            "large" => ArmySize::Large,
            "massive" => ArmySize::Massive,
            _ => return Err("unsupported Horde army".into()),
        };
        // The host records one seed on construction. Replays never consult a clock.
        let seed = self
            .trusted
            .as_ref()
            .filter(|t| t.code == code)
            .map_or_else(
                || {
                    code.bytes()
                        .fold(now ^ self.next_epoch.wrapping_add(1), |v, c| {
                            v.wrapping_mul(1099511628211) ^ u64::from(c)
                        })
                },
                |t| t.seed,
            );
        let state = Match::new_horde(
            &map,
            MatchOptions {
                seed,
                mode: Mode::Horde,
                players,
                shuffle: true,
                army,
                weather: WeatherChoice::from_reference(Some(&room.weather)),
                weather_seed: None,
                living_weather: true,
                map_key: Some(room.map.clone()),
                tech: true,
                supply: false,
                logistics: false,
                sandbox: room.sandbox,
            },
        )?;
        let prepared = self.prepare_native_room(room, state, now, restart, Some(map.source()))?;
        Ok(self.publish_native(code, prepared))
    }

    pub(super) fn horde_command(&mut self, code: &str, id: u64, msg: &Value, now: u64) -> bool {
        let Some(room) = self.rooms.get(code).filter(|r| r.mode == "horde") else {
            return false;
        };
        match msg["t"].as_str() {
            Some(command @ ("start" | "restart")) => {
                if !room.is_host(id) || (command == "start") == room.native.is_some() {
                    return true;
                }
                if let Err(error) = self.start_horde(code, id, command == "restart", now) {
                    // The prototype reports cmd:start for either startup path's failure.
                    // Lost defender markers alone restore the room without a denial.
                    if error != LOST_DEFEND {
                        for person in &self.rooms[code].players {
                            if let Some(connection) = &person.connection {
                                connection
                                    .send(json!({"t":"deny","cmd":"start","reason":"scenario"}));
                            }
                        }
                    }
                    self.rooms[code].broadcast(&self.maps, &self.public_url);
                }
                true
            }
            Some("pause" | "resume") => {
                let host = room.is_host(id);
                let slot = room
                    .players
                    .iter()
                    .position(|p| p.connection.as_ref().is_some_and(|c| c.id == id));
                let room = self.rooms.get_mut(code).unwrap();
                if host && let (Some(slot), Some(native)) = (slot, &mut room.native) {
                    let message = if msg["t"] == "resume" {
                        Some(native.runtime.resume(now))
                    } else {
                        match native.runtime.pause(
                            slot,
                            &room.players[slot].name,
                            crate::room_runtime::PauseReason::Host,
                            now,
                        ) {
                            Ok(message) => message,
                            Err(error) => {
                                native.runtime.fault_stop(error);
                                None
                            }
                        }
                    };
                    if let Some(message) = message {
                        for person in room.people() {
                            if let Some(connection) = &person.connection {
                                connection.send(message.clone());
                            }
                        }
                    }
                }
                true
            }
            _ => false,
        }
    }
}

#[cfg(test)]
#[path = "horde_host_tests.rs"]
mod tests;
