//! Detached AI facts decoded exclusively from a delivered human seat snapshot.
//! This consumer does not widen snapshot admission or activate a room's AI seats.

use crate::{catalog::Catalog, math, recipient_snapshot::SupportedSeatSnapshot};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

mod horde;

/// Metadata already available to the human client from match start.
#[derive(Clone, Debug, PartialEq)]
pub struct PublicMap {
    pub width: usize,
    pub height: usize,
    pub players: Vec<Value>,
    pub points: Vec<Value>,
}

/// All values are owned. Unit entries use the prototype's decoded field names.
#[derive(Clone, Debug, PartialEq)]
pub struct SeatView {
    pub initial_chars: Vec<u8>,
    pub initial_height: Option<Vec<i8>>,
    pub works: Vec<Value>,
    pub chars: Vec<u8>,
    pub flags: Vec<u16>,
    pub height: Option<Vec<i8>>,
    pub naval: bool,
    pub local_defenders: Option<Value>,
    pub tick: i64,
    pub units: BTreeMap<u32, Value>,
    pub players: Vec<Value>,
    pub points: Vec<Value>,
    pub nodes: Vec<Value>,
    pub mines: Vec<Value>,
    pub found_mines: Vec<Value>,
    pub weather: Value,
    pub mode: Value,
    pub army: Value,
    pub smokes: Vec<Value>,
    pub strikes: Vec<Value>,
    pub covers: Vec<Value>,
    pub known_buildings: Vec<Value>,
    pub claimed_nodes: BTreeSet<usize>,
    pub logistics: Value,
}

/// Private to one seat and match. Recreate it for a new match or seat handover.
#[derive(Clone, Debug, PartialEq)]
pub struct SeatMemory {
    slot: usize,
    binding: Option<(u64, usize)>,
    map: PublicMap,
    tick: Option<i64>,
    building_seen: BTreeMap<u32, f64>,
    mines: BTreeSet<usize>,
    initial_chars: Vec<u8>,
    initial_height: Option<Vec<i8>>,
    chars: Vec<u8>,
    flags: Vec<u16>,
    height: Option<Vec<i8>>,
    naval: bool,
}

type Result<T> = std::result::Result<T, String>;
fn rows(value: &Value) -> Result<&[Value]> {
    value
        .as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| "invalid AI snapshot row array".into())
}
fn optional_rows(value: &Value) -> Result<&[Value]> {
    if value.is_null() {
        Ok(&[])
    } else {
        rows(value)
    }
}
fn integer(value: &Value) -> Result<i64> {
    value
        .as_i64()
        .ok_or_else(|| "invalid AI snapshot integer".into())
}
fn id(value: &Value) -> Result<u32> {
    u32::try_from(integer(value)?)
        .ok()
        .filter(|id| *id > 0)
        .ok_or_else(|| "invalid AI snapshot unit id".into())
}
fn number(value: &Value) -> Result<f64> {
    value
        .as_f64()
        .filter(|v| v.is_finite())
        .ok_or_else(|| "invalid AI snapshot number".into())
}
fn truth(value: &Value) -> bool {
    value
        .as_bool()
        .unwrap_or_else(|| value.as_f64().is_some_and(|v| v != 0.0))
}
fn terrain_flag(terrain: &crate::catalog::Value, ch: u8) -> Result<u16> {
    let value = terrain
        .get(&char::from(ch).to_string())
        .and_then(crate::catalog::Value::number)
        .unwrap_or(0.0);
    if !value.is_finite() || value < 0.0 || value > f64::from(u16::MAX) || value.fract() != 0.0 {
        return Err("invalid AI terrain flags".into());
    }
    Ok(value as u16)
}
fn point(x: &Value, z: &Value) -> Result<Value> {
    Ok(json!({"x":number(x)?,"z":number(z)?}))
}
fn cell(width: usize, x: &Value, z: &Value) -> Result<i64> {
    Ok((number(z)? / 2.0).floor() as i64 * width as i64 + (number(x)? / 2.0).floor() as i64)
}
fn mapped(rows: &[Value], names: &[&str]) -> Result<Vec<Value>> {
    rows.iter()
        .map(|row| {
            let row = self::rows(row)?;
            if row.len() < names.len() {
                return Err("short AI snapshot row".into());
            }
            Ok(Value::Object(
                names
                    .iter()
                    .zip(row)
                    .map(|(name, v)| ((*name).into(), v.clone()))
                    .collect(),
            ))
        })
        .collect()
}

impl SeatMemory {
    pub fn new(slot: usize, map: PublicMap) -> Result<Self> {
        if map.width == 0
            || map.height == 0
            || slot >= map.players.len()
            || map.width.checked_mul(map.height).is_none()
            || map
                .players
                .iter()
                .any(|p| !p.is_object() || p["team"].as_i64().is_none())
        {
            return Err("invalid AI public map or seat".into());
        }
        let mut map = map;
        // A match-start descriptor carries only public player metadata.
        for player in &mut map.players {
            *player = json!({"name":player["name"],"team":player["team"],"faction":player["faction"],"spawn":player["spawn"]});
        }
        let cells = map.width * map.height;
        Ok(Self {
            slot,
            binding: None,
            map,
            tick: None,
            building_seen: BTreeMap::new(),
            mines: BTreeSet::new(),
            initial_chars: vec![b'?'; cells],
            initial_height: None,
            chars: vec![b'?'; cells],
            flags: vec![0; cells],
            height: None,
            naval: false,
        })
    }

    /// Supply the public starting map, with authored mines already hidden.
    pub fn with_terrain(
        mut self,
        chars: Vec<u8>,
        height: Option<Vec<i8>>,
        naval: bool,
    ) -> Result<Self> {
        let cells = self.map.width * self.map.height;
        if self.tick.is_some()
            || chars.len() != cells
            || height.as_ref().is_some_and(|h| h.len() != cells)
        {
            return Err("invalid AI public terrain dimensions or initialization phase".into());
        }
        let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
        let terrain = catalog.table("TERRAIN").map_err(|e| e.to_string())?;
        self.flags = chars
            .iter()
            .map(|ch| terrain_flag(terrain, *ch))
            .collect::<Result<_>>()?;
        self.initial_chars = chars.clone();
        self.initial_height = height.clone();
        self.chars = chars;
        self.height = height;
        self.naval = naval;
        Ok(self)
    }

    /// Consume the same complete internal result the human recipient receives.
    pub fn observe(&mut self, snapshot: SupportedSeatSnapshot) -> Result<SeatView> {
        self.decode(&snapshot.into_json())
    }

    /// Decode an already delivered snapshot, including snapshots supplied by a host adapter.
    /// Failure leaves private memory unchanged. No authoritative Match is accepted here.
    pub fn decode(&mut self, snapshot: &Value) -> Result<SeatView> {
        let mut next = self.clone();
        let view = next.decode_inner(snapshot)?;
        *self = next;
        Ok(view)
    }

    fn decode_inner(&mut self, snap: &Value) -> Result<SeatView> {
        let tick = integer(&snap["tick"])?;
        if tick < 0 || self.tick.is_some_and(|old| tick < old) {
            return Err("AI snapshot clock went backwards".into());
        }
        let world = snap["mode"]["kind"] == "world";
        let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
        let mut units = BTreeMap::new();
        for row in rows(&snap["units"])? {
            let row = rows(row)?;
            if row.len() < 15 {
                return Err("AI unit requires at least fifteen delivered fields".into());
            }
            let uid = id(&row[0])?;
            let kind = row[1].as_str().ok_or("invalid AI unit kind")?;
            let definition = catalog.unit(kind).map_err(|e| e.to_string())?;
            let owner = integer(&row[2])?;
            if !(0..self.map.players.len() as i64).contains(&owner) && !(world && owner == -1) {
                return Err("invalid AI unit owner".into());
            }
            let own = owner == self.slot as i64;
            let mut flags = integer(&row[12])?;
            if flags < 0 {
                return Err("invalid AI unit flags".into());
            }
            if !own {
                flags &= !(2048 | 4096 | 8192 | 16384);
            }
            let mut u = json!({"id":uid,"type":kind,"owner":owner,
                "x":row[3],"z":row[4],"rot":row[5],"aim":row[6],"hp":row[7],"supp":row[8],
                "targetId":row[9],"cover":row[10],"cd":if own {row[11].clone()} else {json!(0)},"flags":flags,"veterancy":row[13],"built":row[14],
                "path":[],"orders":[],"attackId":0,"retreating":flags & 1 != 0,"buff":if flags & 2 != 0 {1} else {0},
                "ap":flags & 4 != 0,"reinf":if flags & 8 != 0 {1} else {0},"dig":if flags & 16 != 0 {json!({})} else {Value::Null},
                "garrison":if flags & 32 != 0 {0} else {-1},"amove":null,"build":if flags & 128 != 0 {1} else {0},
                "enter":-1,"fireAt":-1,"nade":null,"entrench":if flags & 1024 != 0 {json!({})} else {Value::Null},
                "holdFire":flags & 2048 != 0,"holdPos":flags & 4096 != 0,"autoRetreat":flags & 8192 != 0,"auto":flags & 16384 != 0});
            if definition
                .get("air")
                .and_then(crate::catalog::Value::boolean)
                .unwrap_or(false)
            {
                u["air"] = json!({"state":if flags & 512 != 0 {"base"} else {"out"}});
            }
            if units.insert(uid, u).is_some() {
                return Err("duplicate AI unit id".into());
            }
        }
        self.decode_own(snap, &mut units)?;
        let team = self.map.players[self.slot]["team"].clone();
        let mut players = self.map.players.clone();
        for (slot, player) in players.iter_mut().enumerate() {
            player["slot"] = json!(slot);
            player["out"] = snap["out"][slot].clone();
            player["vp"] = snap["vp"][slot].clone();
            if world && player["team"] != team {
                player["spawn"] = Value::Null;
            }
        }
        if world && snap["home"].is_array() {
            players[self.slot]["spawn"] = point(&snap["home"][0], &snap["home"][1])?;
        }
        for name in [
            "mp", "mun", "fuel", "tech", "inc", "fuelInc", "upkeep", "sup",
        ] {
            players[self.slot][name] = snap[name].clone();
        }
        players[self.slot]["rally"] = if snap["rally"].is_array() {
            point(&snap["rally"][0], &snap["rally"][1])?
        } else {
            Value::Null
        };
        let points = if world {
            optional_rows(&snap["world"]["regions"] )?.iter().map(|r| {
                let owner = players.iter().position(|p| p["team"] == r["team"]);
                json!({"id":r["id"],"x":r["x"],"z":r["z"],"vp":1,"mp":1,"owner":owner.map_or(-1,|n|n as i64),
                    "capper":r["capper"],"progress":r["progress"],"contested":truth(&r["contested"]),"locked":r["locked"],"cut":false})
            }).collect()
        } else {
            let delivered = rows(&snap["points"])?;
            if delivered.len() != self.map.points.len() {
                return Err("AI objective count mismatch".into());
            }
            self.map.points.iter().zip(delivered).map(|(p,r)| {
                let r = rows(r)?;
                if r.len() < 6 { return Err("short AI objective row".into()); }
                Ok(json!({"x":p["x"],"z":p["z"],"vp":p["vp"],"mp":p["mp"],"owner":r[0],"capper":r[1],"progress":r[2],
                    "contested":truth(&r[3]),"cut":truth(&r[4]),"locked":truth(&r[5])}))
            }).collect::<Result<Vec<_>>>()?
        };
        let nodes = mapped(optional_rows(&snap["nodes"])?, &["x", "z", "rate", "fuel"])?
            .into_iter()
            .map(|mut n| {
                n["fuel"] = json!(truth(&n["fuel"]));
                n
            })
            .collect::<Vec<_>>();
        for row in optional_rows(&snap["cells"])? {
            let row = rows(row)?;
            if row.len() < 2 {
                return Err("short AI terrain row".into());
            }
            let c = usize::try_from(integer(&row[0])?).map_err(|_| "invalid AI mine cell")?;
            if c >= self.map.width * self.map.height {
                return Err("invalid AI mine cell".into());
            }
            let ch = row[1]
                .as_str()
                .filter(|s| s.len() == 1 && s.is_ascii())
                .ok_or("invalid AI terrain character")?
                .as_bytes()[0];
            let data = row.get(4).unwrap_or(&Value::Null);
            let surface = data["object"]
                .as_str()
                .filter(|s| *s != ".")
                .or_else(|| data["ground"].as_str())
                .unwrap_or(".");
            if surface.len() != 1 || !surface.is_ascii() {
                return Err("invalid AI terrain surface".into());
            }
            let terrain = catalog.table("TERRAIN").map_err(|e| e.to_string())?;
            self.chars[c] = ch;
            self.flags[c] = terrain_flag(
                terrain,
                if ch == b'N' {
                    surface.as_bytes()[0]
                } else {
                    ch
                },
            )?;
            if let Some(level) = row.get(2).filter(|v| !v.is_null()) {
                let level =
                    i8::try_from(integer(level)?).map_err(|_| "invalid AI terrain height")?;
                self.height
                    .get_or_insert_with(|| vec![0; self.map.width * self.map.height])[c] = level;
            }
            if ch == b'N' && truth(&data["mineOwned"]) {
                self.mines.insert(c);
            } else {
                self.mines.remove(&c);
            }
        }
        let found = optional_rows(&snap["foundMines"])?
            .iter()
            .map(|v| {
                let c = usize::try_from(integer(v)?).map_err(|_| "invalid AI mine cell")?;
                if c >= self.map.width * self.map.height {
                    return Err("invalid AI mine cell".into());
                }
                Ok(c)
            })
            .collect::<Result<BTreeSet<_>>>()?;
        let at = |c: &usize| json!({"x":(c % self.map.width) as f64 * 2.0 + 1.0,"z":(c / self.map.width) as f64 * 2.0 + 1.0});
        let mines = self.mines.difference(&found).map(at).collect();
        let found_mines = found.iter().map(at).collect();
        let smokes = mapped(optional_rows(&snap["smokes"])?, &["x", "z", "r"])?;
        let strikes = mapped(
            optional_rows(&snap["strikes"])?,
            &["kind", "x", "z", "dir", "t", "owner"],
        )?
        .into_iter()
        .map(|mut s| {
            s["live"] = json!(s["t"].as_f64().is_some_and(|t| t <= 0.0));
            s
        })
        .collect();
        let covers = mapped(optional_rows(&snap["covers"])?, &["x", "z", "r", "t"])?;
        let mut known = Vec::new();
        let mut positions = BTreeMap::new();
        for b in mapped(
            optional_rows(&snap["ghosts"])?,
            &["id", "type", "owner", "x", "z", "built"],
        )? {
            let uid = id(&b["id"])?;
            positions.insert(uid, known.len());
            known.push(b);
        }
        for (uid, u) in &units {
            let owner = u["owner"].as_i64().unwrap();
            let owner_team = if owner == -1 {
                json!(-1)
            } else {
                players[owner as usize]["team"].clone()
            };
            if owner_team == team
                || !catalog
                    .unit(u["type"].as_str().unwrap())
                    .map_err(|e| e.to_string())?
                    .get("building")
                    .and_then(crate::catalog::Value::boolean)
                    .unwrap_or(false)
            {
                continue;
            }
            let b = json!({"id":uid,"type":u["type"],"owner":owner,"x":u["x"],"z":u["z"],"built":u["built"]});
            if let Some(index) = positions.get(uid) {
                known[*index] = b;
            } else {
                positions.insert(*uid, known.len());
                known.push(b);
            }
            self.building_seen.insert(*uid, tick as f64 / 20.0);
        }
        self.building_seen
            .retain(|uid, _| positions.contains_key(uid));
        for b in &mut known {
            b["t"] = self
                .building_seen
                .get(&id(&b["id"])?)
                .map_or(Value::Null, |t| json!(t));
        }
        let claims = units
            .values()
            .filter(|u| {
                u["type"] == "depot"
                    && u["owner"]
                        .as_u64()
                        .is_some_and(|owner| players[owner as usize]["team"] == team)
            })
            .chain(known.iter().filter(|b| b["type"] == "depot"))
            .collect::<Vec<_>>();
        let mut claimed_nodes = BTreeSet::new();
        for (index, n) in nodes.iter().enumerate() {
            for b in &claims {
                if math::hypot(
                    number(&b["x"])? - number(&n["x"])?,
                    number(&b["z"])? - number(&n["z"])?,
                ) < 8.0
                {
                    claimed_nodes.insert(index);
                    break;
                }
            }
        }
        let mut logistics = Value::Null;
        if truth(&snap["logistics"]["enabled"]) {
            logistics = snap["logistics"].clone();
            for name in ["units", "trucks"] {
                logistics[name] = json!(
                    optional_rows(&snap["logistics"][name])?
                        .iter()
                        .filter(|r| r["id"]
                            .as_u64()
                            .and_then(|n| u32::try_from(n).ok())
                            .and_then(|uid| units.get(&uid))
                            .is_some_and(|u| u["owner"].as_u64() == Some(self.slot as u64)))
                        .cloned()
                        .collect::<Vec<_>>()
                );
            }
            for row in optional_rows(&logistics["units"])? {
                if let Some(u) = units.get_mut(&id(&row["id"])?) {
                    u["logistics"] = row.clone();
                }
            }
        }
        self.tick = Some(tick);
        Ok(SeatView { initial_chars:self.initial_chars.clone(), initial_height:self.initial_height.clone(), works:optional_rows(&snap["works"])?.to_vec(), chars:self.chars.clone(), flags:self.flags.clone(), height:self.height.clone(), naval:self.naval,
            local_defenders: world.then(||json!({"slot":-1,"name":"Local defenders","team":-1,"faction":0,"spawn":null,"out":false})), tick, units, players, points, nodes, mines, found_mines,
            weather:json!({"now":snap["weather"][0].as_str().unwrap_or("clear"),"next":snap["weather"][1],"left":snap["weather"][2]}),
            mode:snap["mode"].clone(),army:snap["army"].clone(),smokes,strikes,covers,known_buildings:known,claimed_nodes,logistics })
    }

    fn decode_own(&self, snap: &Value, units: &mut BTreeMap<u32, Value>) -> Result<()> {
        for name in [
            "plans",
            "queues",
            "productionJobs",
            "air",
            "orders",
            "movement",
        ] {
            for row in optional_rows(&snap[name])? {
                let row = rows(row)?;
                let uid = id(row.first().ok_or("empty AI private row")?)?;
                let Some(u) = units
                    .get_mut(&uid)
                    .filter(|u| u["owner"].as_u64() == Some(self.slot as u64))
                else {
                    continue;
                };
                match name {
                    "plans" => {
                        if row.len() < 4 || !(row.len() - 4).is_multiple_of(2) {
                            return Err("invalid AI plan row".into());
                        }
                        let at = point(&row[2], &row[3])?;
                        u["path"] = json!(
                            row[4..]
                                .as_chunks::<2>()
                                .0
                                .iter()
                                .map(|p| point(&p[0], &p[1]))
                                .collect::<Result<Vec<_>>>()?
                        );
                        let kind = integer(&row[1])?;
                        u["orderPlan"] = json!({"kind":kind,"x":row[2],"z":row[3]});
                        match kind {
                            1 => u["worldGoal"] = at.clone(),
                            2 => u["amove"] = at.clone(),
                            4 => {
                                u["attackId"] = if truth(&u["targetId"]) {
                                    u["targetId"].clone()
                                } else {
                                    json!(1)
                                }
                            }
                            5 => u["fireAt"] = json!(cell(self.map.width, &row[2], &row[3])?),
                            6 => u["nade"] = at.clone(),
                            7 if !u["dig"].is_null() => u["dig"] = at.clone(),
                            8 => u["build"] = json!(1),
                            9 => u["enter"] = json!(cell(self.map.width, &row[2], &row[3])?),
                            _ => {}
                        }
                        if u["air"].is_object() && kind != 0 {
                            u["air"]["mission"] = json!({"kind":if kind == 4 {"attack"} else {"move"},"x":at["x"],"z":at["z"]});
                        }
                    }
                    "queues" => {
                        if row.len() < 4 {
                            return Err("invalid AI queue row".into());
                        }
                        u["queue"] = json!(row[4..]);
                        u["prog"] = row[1].clone();
                        u["rally"] = if number(&row[2])? < 0.0 {
                            Value::Null
                        } else {
                            point(&row[2], &row[3])?
                        };
                    }
                    "productionJobs" => {
                        if row.len() < 2 {
                            return Err("invalid AI production jobs".into());
                        }
                        u["productionJobs"] = json!(rows(&row[1])?);
                    }
                    "air" => {
                        if row.len() < 5 || !u["air"].is_object() {
                            return Err("invalid AI aircraft row".into());
                        }
                        let state = usize::try_from(integer(&row[1])?)
                            .ok()
                            .and_then(|i| ["base", "out", "station", "home", "rearm"].get(i))
                            .ok_or("invalid AI air state")?;
                        for (name, value) in ["fuel", "ammo", "timer"].iter().zip(&row[2..]) {
                            u["air"][*name] = value.clone();
                        }
                        u["air"]["state"] = json!(state);
                    }
                    "orders" => {
                        if row.len() < 2 {
                            return Err("invalid AI orders".into());
                        }
                        let count = usize::try_from(integer(&row[1])?)
                            .map_err(|_| "invalid AI order count")?;
                        if count > (row.len() - 2) / 3 {
                            return Err("invalid AI order count".into());
                        }
                        u["orders"] = json!(
                            row[2..2 + count * 3]
                                .as_chunks::<3>()
                                .0
                                .iter()
                                .map(|o| json!({"kind":o[0],"x":o[1],"z":o[2]}))
                                .collect::<Vec<_>>()
                        );
                    }
                    "movement" => {
                        if row.len() < 3 {
                            return Err("invalid AI movement row".into());
                        }
                        u["moveOutcome"] = row[1].clone();
                        u["moveOutcomeTick"] = row[2].clone();
                    }
                    _ => unreachable!(),
                }
            }
        }
        Ok(())
    }
}

impl crate::match_runtime::Match {
    pub(super) fn require_ai_recipient_mode(&self) -> Result<()> {
        use crate::{economy::Mode, game::ModeKind};
        if !matches!(
            (self.options.mode, self.world.game.mode),
            (Mode::Default | Mode::Conquest, None)
                | (Mode::Classic, Some(ModeKind::Classic))
                | (Mode::Assault, Some(ModeKind::Assault))
                | (Mode::Annihilation, Some(ModeKind::Annihilation))
                | (Mode::Horde, Some(ModeKind::Horde))
        ) {
            return Err("AI delivery requires a canonical ordinary mode pair".into());
        }
        Ok(())
    }

    /// Build a consumer from the same public start metadata given to ordinary clients.
    pub fn ordinary_ai_view_memory(&self, slot: usize) -> Result<SeatMemory> {
        self.require_ai_recipient_mode()?;
        let map = PublicMap {
            width: self.world.game.w,
            height: self.world.game.h,
            players: self
                .start
                .players
                .iter()
                .map(|p| {
                    json!({"name":p.config.name,"team":p.config.team,
                "faction":p.config.faction as u8,"spawn":{"x":p.spawn.x,"z":p.spawn.z}})
                })
                .collect(),
            points: self
                .start
                .points
                .iter()
                .map(|p| json!({"x":p.at.x,"z":p.at.z,"vp":p.vp,"mp":p.mp}))
                .collect(),
        };
        let initial = self.world.initial();
        let chars = initial
            .chars
            .iter()
            .enumerate()
            .map(|(c, &ch)| {
                if initial.mines[c] == 0 {
                    ch
                } else if initial.objects[c] != b'.' {
                    initial.objects[c]
                } else {
                    initial.ground[c]
                }
            })
            .collect();
        let mut memory = SeatMemory::new(slot, map)?.with_terrain(
            chars,
            initial.height.clone(),
            self.world.game.naval,
        )?;
        memory.binding = Some((
            self.options.seed,
            self.world.initial().chars.as_ptr() as usize,
        ));
        Ok(memory)
    }

    /// Deliver through the existing selected recipient pipeline, then decode its owned result.
    /// Existing mode, actor and snapshot admission guards apply without alteration.
    pub fn ordinary_ai_view(
        &mut self,
        memory: &mut SeatMemory,
        delivery: &mut crate::recipient_snapshot::DeliveryState,
        full: bool,
    ) -> Result<SeatView> {
        self.require_ai_recipient_mode()?;
        if memory.binding
            != Some((
                self.options.seed,
                self.world.initial().chars.as_ptr() as usize,
            ))
        {
            return Err("AI observation memory belongs to another match".into());
        }
        let selected = [crate::recipient_snapshot::SelectedSeat {
            slot: memory.slot,
            full,
            omit: true,
        }];
        let frame = delivery
            .capture_selected_with_lists(self, &selected, &[])
            .map_err(|e| format!("AI recipient delivery: {e:?}"))?
            .ok_or("missing selected AI recipient")?;
        memory.observe(
            frame
                .recipient(memory.slot)
                .map_err(|e| format!("AI recipient snapshot: {e:?}"))?,
        )
    }
}

#[cfg(test)]
mod tests;

pub mod commander;
