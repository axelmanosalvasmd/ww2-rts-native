//! A seat's persistent AI map and player view, built from public startup facts
//! and delivered recipient rows. Canonical terrain is never consulted on update.
use std::{collections::BTreeSet, sync::Arc};

use serde_json::Value;

use crate::{
    catalog::Catalog,
    economy::Mode,
    game::Point,
    match_runtime::Match,
    recipient_snapshot::{DeliveryState, SelectedSeat, SnapshotError},
};

#[derive(Clone, Debug, PartialEq)]
pub struct PublicPlayer {
    pub name: String,
    pub team: i32,
    pub faction: u8,
    pub spawn: Option<Point>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SeatPlayer {
    pub slot: i32,
    pub name: String,
    pub team: i32,
    pub faction: u8,
    pub spawn: Option<Point>,
    pub out: bool,
    pub vp: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TerrainFrame {
    pub chars: Arc<[u8]>,
    pub flags: Arc<[u16]>,
    pub height: Option<Arc<[i8]>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SeatView {
    pub players: Vec<SeatPlayer>,
    pub local_defender: Option<SeatPlayer>,
    pub terrain: TerrainFrame,
    pub map_chars: Arc<[u8]>,
    pub map_height: Option<Arc<[i8]>>,
    pub bridge_cells: BTreeSet<usize>,
    pub own_mines: BTreeSet<usize>,
}

/// A public starting map, with authored mines already replaced by their
/// underlying surface. `from_match` performs that masking before any AI read.
#[derive(Clone, Debug)]
pub struct PublicTerrain {
    pub width: usize,
    pub height: usize,
    pub chars: Vec<u8>,
    pub levels: Option<Vec<i8>>,
}

#[derive(Clone, Debug)]
pub struct SeatMemory {
    slot: usize,
    world: bool,
    players: Vec<PublicPlayer>,
    width: usize,
    current: TerrainFrame,
    map_chars: Arc<[u8]>,
    map_height: Option<Arc<[i8]>>,
    bridges: BTreeSet<usize>,
    mines: BTreeSet<usize>,
    binding: Option<(usize, u64)>,
    last_tick: Option<i64>,
}

impl PublicTerrain {
    pub(crate) fn from_match(state: &Match) -> Self {
        let initial = state.world.initial();
        let chars = initial
            .chars
            .iter()
            .enumerate()
            .map(|(cell, &ch)| {
                if initial.mines[cell] == 0 {
                    ch
                } else if initial.objects[cell] != b'.' {
                    initial.objects[cell]
                } else {
                    initial.ground[cell]
                }
            })
            .collect();
        Self {
            width: state.world.game.w,
            height: state.world.game.h,
            chars,
            levels: initial.height.clone(),
        }
    }
}

fn terrain_flag(ch: u8) -> Result<u16, SnapshotError> {
    let catalog = Catalog::compiled().map_err(|_| SnapshotError::Rules)?;
    let table = catalog.table("TERRAIN").map_err(|_| SnapshotError::Rules)?;
    Ok(table
        .get(&char::from(ch).to_string())
        .and_then(|value| value.number())
        .filter(|value| value.is_finite() && *value >= 0. && *value <= u16::MAX as f64)
        .map(|value| value as u16)
        .unwrap_or(0))
}

impl SeatMemory {
    pub fn new(
        slot: usize,
        mut players: Vec<PublicPlayer>,
        map: PublicTerrain,
        world: bool,
    ) -> Result<Self, SnapshotError> {
        let len = map
            .width
            .checked_mul(map.height)
            .ok_or(SnapshotError::Rules)?;
        if slot >= players.len()
            || map.chars.len() != len
            || map
                .levels
                .as_ref()
                .is_some_and(|levels| levels.len() != len)
        {
            return Err(SnapshotError::InvalidRecipient);
        }
        if world {
            let team = players[slot].team;
            for player in &mut players {
                if player.team != team {
                    player.spawn = None;
                }
            }
        }
        let chars: Vec<u8> = if world { vec![b'?'; len] } else { map.chars };
        let flags = if world {
            vec![0; len]
        } else {
            chars
                .iter()
                .map(|&ch| terrain_flag(ch))
                .collect::<Result<Vec<_>, _>>()?
        };
        let levels = if world {
            Some(vec![0; len])
        } else {
            map.levels
        };
        let bridges = chars
            .iter()
            .enumerate()
            .filter_map(|(cell, ch)| (*ch == b'=').then_some(cell))
            .collect();
        let current = TerrainFrame {
            chars: Arc::from(chars),
            flags: Arc::from(flags),
            height: levels.map(Arc::from),
        };
        Ok(Self {
            slot,
            world,
            players,
            width: map.width,
            map_chars: Arc::clone(&current.chars),
            map_height: current.height.clone(),
            current,
            bridges,
            mines: BTreeSet::new(),
            binding: None,
            last_tick: None,
        })
    }

    pub fn from_match(state: &Match, slot: usize) -> Result<Self, SnapshotError> {
        if !matches!(state.options.mode, Mode::Default | Mode::Conquest)
            || state.world.game.mode.is_some()
            || state.world_runtime.is_some()
        {
            return Err(SnapshotError::UnsupportedMode);
        }
        let players = state
            .start
            .players
            .iter()
            .map(|player| PublicPlayer {
                name: player.config.name.clone(),
                team: player.config.team,
                faction: player.config.faction as u8,
                spawn: Some(player.spawn),
            })
            .collect();
        let mut memory = Self::new(slot, players, PublicTerrain::from_match(state), false)?;
        memory.binding = Some((
            state.world.initial().chars.as_ptr() as usize,
            state.options.seed,
        ));
        Ok(memory)
    }

    /// Build the next view before publishing memory. An invalid frame retains
    /// the previous terrain and mine knowledge.
    pub fn accept(&mut self, snapshot: &Value) -> Result<SeatView, SnapshotError> {
        let mut next = self.clone();
        let out = snapshot
            .get("out")
            .and_then(Value::as_array)
            .ok_or(SnapshotError::Rules)?;
        let vp = snapshot
            .get("vp")
            .and_then(Value::as_array)
            .ok_or(SnapshotError::Rules)?;
        if out.len() != self.players.len() || vp.len() != self.players.len() {
            return Err(SnapshotError::Rules);
        }
        let mut players = Vec::with_capacity(self.players.len());
        for (slot, player) in self.players.iter().enumerate() {
            let visible_spawn = if !self.world || player.team == self.players[self.slot].team {
                player.spawn
            } else {
                None
            };
            let spawn = if self.world && slot == self.slot {
                snapshot
                    .get("home")
                    .and_then(Value::as_array)
                    .map(|home| {
                        if home.len() != 2 {
                            return Err(SnapshotError::Rules);
                        }
                        Ok(Point {
                            x: home[0].as_f64().ok_or(SnapshotError::Rules)?,
                            z: home[1].as_f64().ok_or(SnapshotError::Rules)?,
                        })
                    })
                    .transpose()?
                    .or(visible_spawn)
            } else {
                visible_spawn
            };
            let score = vp[slot].as_f64().ok_or(SnapshotError::Rules)?;
            if !score.is_finite() {
                return Err(SnapshotError::InvalidNumber);
            }
            players.push(SeatPlayer {
                slot: slot as i32,
                name: player.name.clone(),
                team: player.team,
                faction: player.faction,
                spawn,
                out: out[slot].as_bool().ok_or(SnapshotError::Rules)?,
                vp: score,
            });
        }
        let cells = snapshot
            .get("cells")
            .and_then(Value::as_array)
            .ok_or(SnapshotError::Rules)?;
        let len = next.current.chars.len();
        for row in cells {
            let row = row
                .as_array()
                .filter(|row| row.len() >= 5)
                .ok_or(SnapshotError::Rules)?;
            let cell = row[0]
                .as_u64()
                .and_then(|cell| usize::try_from(cell).ok())
                .filter(|cell| *cell < len)
                .ok_or(SnapshotError::Rules)?;
            let ch = row[1]
                .as_str()
                .filter(|ch| ch.len() == 1)
                .ok_or(SnapshotError::Rules)?
                .as_bytes()[0];
            let data = row[4].as_object().ok_or(SnapshotError::Rules)?;
            let surface = data
                .get("object")
                .and_then(Value::as_str)
                .filter(|object| *object != ".")
                .or_else(|| data.get("ground").and_then(Value::as_str))
                .ok_or(SnapshotError::Rules)?;
            let surface = surface
                .as_bytes()
                .first()
                .copied()
                .filter(|_| surface.len() == 1)
                .ok_or(SnapshotError::Rules)?;
            let flag = terrain_flag(if ch == b'N' { surface } else { ch })?;
            if next.current.chars[cell] != ch {
                Arc::make_mut(&mut next.current.chars)[cell] = ch;
            }
            if next.current.flags[cell] != flag {
                Arc::make_mut(&mut next.current.flags)[cell] = flag;
            }
            if ch == b'N' && data.get("mineOwned").and_then(Value::as_bool) == Some(true) {
                next.mines.insert(cell);
            } else {
                next.mines.remove(&cell);
            }
            if self.world {
                if ch == b'=' {
                    next.bridges.insert(cell);
                } else {
                    next.bridges.remove(&cell);
                }
            }
            if !row[2].is_null() {
                let level = row[2]
                    .as_i64()
                    .and_then(|level| i8::try_from(level).ok())
                    .ok_or(SnapshotError::Rules)?;
                if next
                    .current
                    .height
                    .as_ref()
                    .is_none_or(|height| height[cell] != level)
                {
                    let height = next
                        .current
                        .height
                        .get_or_insert_with(|| Arc::from(vec![0; len]));
                    Arc::make_mut(height)[cell] = level;
                }
            }
        }
        if self.world {
            next.map_chars = Arc::clone(&next.current.chars);
            next.map_height = next.current.height.clone();
        }
        let view = SeatView {
            players,
            local_defender: self.world.then(|| SeatPlayer {
                slot: -1,
                name: "Local defenders".into(),
                team: -1,
                faction: 0,
                spawn: None,
                out: false,
                vp: 0.,
            }),
            terrain: next.current.clone(),
            map_chars: Arc::clone(&next.map_chars),
            map_height: next.map_height.clone(),
            bridge_cells: next.bridges.clone(),
            own_mines: next.mines.clone(),
        };
        *self = next;
        Ok(view)
    }

    /// Read the seat's complete remembered terrain without draining the browser
    /// outbox. This also seeds a human handover after a browser delta was sent.
    pub fn observe_match(
        &mut self,
        state: &mut Match,
        delivery: &mut DeliveryState,
    ) -> Result<SeatView, SnapshotError> {
        if self.world
            || self.slot >= state.world.game.players.len()
            || !matches!(state.options.mode, Mode::Default | Mode::Conquest)
            || self.binding
                != Some((
                    state.world.initial().chars.as_ptr() as usize,
                    state.options.seed,
                ))
            || self.width != state.world.game.w
            || self.current.chars.len() != state.world.game.chars.len()
        {
            return Err(SnapshotError::UnsupportedMode);
        }
        let tick = state.world.game.tick;
        if self.last_tick.is_some_and(|last| tick <= last) {
            return Err(SnapshotError::Rules);
        }
        let selected = [SelectedSeat {
            slot: self.slot,
            full: true,
            omit: true,
        }];
        let frame = delivery
            .capture_selected_with_lists(state, &selected, &[])?
            .ok_or(SnapshotError::Rules)?;
        let snapshot = frame.recipient(self.slot)?.into_json();
        if snapshot["tick"].as_i64() != Some(tick) {
            return Err(SnapshotError::Rules);
        }
        let view = self.accept(&snapshot)?;
        self.last_tick = Some(tick);
        Ok(view)
    }

    pub fn width(&self) -> usize {
        self.width
    }
}

#[cfg(test)]
#[path = "ai_seat_view/tests.rs"]
mod tests;
