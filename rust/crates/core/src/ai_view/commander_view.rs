//! Full commander packets built from delivered seat facts and public start terrain.

use super::{PublicMap, SeatView, integer, number, optional_rows, rows, truth};
use crate::{
    catalog::{Catalog, Value as Rule},
    commander_ai::{
        Input, Observation, PlayerObservation, PointObservation, ResearchObservation,
        UnitObservation,
    },
    commander_tactics::{Context, KnownBuilding, NodeObservation, Stock, UnitFacts},
    economy::{Faction, Mode, TechState, Technology},
    game::{Point, Smoke},
    human_commander::{HumanSighting, HumanUnit, HumanView},
    human_commander_inputs, math,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

type Result<T> = std::result::Result<T, String>;

/// Recreate for each match and seat handover. Initial terrain must be the public
/// client map, with authored mines hidden, never the authoritative terrain.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CommanderViewMemory {
    slot: usize,
    width: usize,
    height: usize,
    players: usize,
    chars: Vec<u8>,
    flags: Vec<u16>,
    elevation: Option<Vec<i8>>,
    naval: bool,
    tick: Option<i64>,
    sightings: BTreeMap<u32, HumanSighting>,
    #[serde(default)]
    stationary: BTreeMap<u32, (Point, f64)>,
}

fn at(value: &Value) -> Result<Point> {
    Ok(Point {
        x: number(&value["x"])?,
        z: number(&value["z"])?,
    })
}
fn optional_at(value: &Value) -> Result<Option<Point>> {
    if value.is_null() {
        Ok(None)
    } else {
        at(value).map(Some)
    }
}
fn optional_wallet(value: &Value) -> Result<f64> {
    // Ordinary snapshots omit these wallets. The source planner uses ?? 0.
    if value.is_null() {
        Ok(0.0)
    } else {
        number(value)
    }
}
fn byte(value: &Value) -> Result<u8> {
    value
        .as_str()
        .filter(|s| s.len() == 1 && s.is_ascii())
        .map(|s| s.as_bytes()[0])
        .ok_or_else(|| "invalid delivered commander terrain character".into())
}
fn flag(terrain: &Rule, ch: u8) -> Result<u16> {
    let value = terrain
        .get(&char::from(ch).to_string())
        .and_then(Rule::number)
        .ok_or("unknown delivered commander terrain")?;
    if !value.is_finite() || value < 0.0 || value > u16::MAX as f64 {
        return Err("invalid delivered commander terrain mask".into());
    }
    Ok(value as u16)
}
fn technology(value: &Value) -> Result<Technology> {
    match value.as_str() {
        Some("tier") => Ok(Technology::Tier),
        Some("inf") => Ok(Technology::Infantry),
        Some("guns") => Ok(Technology::Guns),
        Some("armor") => Ok(Technology::Armor),
        _ => Err("invalid delivered commander research kind".into()),
    }
}
fn tech_level(value: &Value) -> Result<u8> {
    u8::try_from(integer(value)?).map_err(|_| "invalid delivered commander research level".into())
}

impl CommanderViewMemory {
    pub fn planner_memory_json(&self) -> Result<Value> {
        crate::save_codec::to_value(self)
    }

    pub fn restore_planner_memory(&mut self, value: &Value) -> Result<()> {
        let restored: Self = crate::save_codec::from_value(value.clone())?;
        if (
            restored.slot,
            restored.width,
            restored.height,
            restored.players,
            restored.naval,
        ) != (self.slot, self.width, self.height, self.players, self.naval)
            || restored.chars.len() != self.chars.len()
            || restored.flags.len() != self.flags.len()
            || restored
                .elevation
                .as_ref()
                .is_some_and(|h| h.len() != self.chars.len())
        {
            return Err("saved commander memory belongs to another seat or map".into());
        }
        *self = restored;
        Ok(())
    }

    pub fn new(
        slot: usize,
        map: PublicMap,
        chars: Vec<u8>,
        height: Option<Vec<i8>>,
        naval: bool,
    ) -> Result<Self> {
        // Share the detached consumer's public descriptor and slot checks.
        super::SeatMemory::new(slot, map.clone())?;
        let cells = map
            .width
            .checked_mul(map.height)
            .ok_or("commander terrain dimensions overflow")?;
        if chars.len() != cells || height.as_ref().is_some_and(|h| h.len() != cells) {
            return Err("invalid public commander terrain dimensions".into());
        }
        let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
        let terrain = catalog.table("TERRAIN").map_err(|e| e.to_string())?;
        let flags = chars
            .iter()
            .map(|ch| flag(terrain, *ch))
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            slot,
            width: map.width,
            height: map.height,
            players: map.players.len(),
            chars,
            flags,
            elevation: height,
            naval,
            tick: None,
            sightings: BTreeMap::new(),
            stationary: BTreeMap::new(),
        })
    }

    /// The snapshot supplies only the delivered cell delta. Every actor, private
    /// order and wallet is read from the detached SeatMemory::decode result.
    /// A failed packet leaves terrain and sighting memory unchanged.
    pub fn observe(&mut self, view: &SeatView, snapshot: &Value) -> Result<HumanView<Observation>> {
        let mut next = self.clone();
        let result = next.observe_inner(view, snapshot)?;
        *self = next;
        Ok(result)
    }

    fn observe_inner(
        &mut self,
        view: &SeatView,
        snapshot: &Value,
    ) -> Result<HumanView<Observation>> {
        if view.tick < 0
            || view.tick > 9_007_199_254_740_991
            || integer(&snapshot["tick"])? != view.tick
            || self.tick.is_some_and(|tick| tick > view.tick)
            || view.players.len() != self.players
            || view.players[self.slot]["slot"].as_u64() != Some(self.slot as u64)
        {
            return Err("delivered commander seat or clock mismatch".into());
        }
        let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
        let terrain = catalog.table("TERRAIN").map_err(|e| e.to_string())?;
        for row in optional_rows(&snapshot["cells"])? {
            let row = rows(row)?;
            if row.len() < 5 {
                return Err("short delivered commander cell".into());
            }
            let cell = usize::try_from(integer(&row[0])?)
                .map_err(|_| "invalid delivered commander cell")?;
            if cell >= self.chars.len() {
                return Err("invalid delivered commander cell".into());
            }
            let ch = byte(&row[1])?;
            let surface = if ch == b'N' {
                let object = byte(&row[4]["object"])?;
                if object == b'.' {
                    byte(&row[4]["ground"])?
                } else {
                    object
                }
            } else {
                ch
            };
            self.chars[cell] = ch;
            self.flags[cell] = flag(terrain, surface)?;
            let elevation = i8::try_from(integer(&row[2])?)
                .map_err(|_| "invalid delivered commander height")?;
            self.elevation
                .get_or_insert_with(|| vec![0; self.chars.len()])[cell] = elevation;
        }
        let me = &view.players[self.slot];
        let faction = Faction::try_from(tech_level(&me["faction"])?).map_err(|e| e.to_string())?;
        let players = view
            .players
            .iter()
            .map(|p| {
                Ok(PlayerObservation {
                    team: i32::try_from(integer(&p["team"])?)
                        .map_err(|_| "invalid delivered commander team")?,
                    out: truth(&p["out"]),
                    spawn: at(&p["spawn"])?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let team = players[self.slot].team;
        let points = view
            .points
            .iter()
            .map(|p| {
                let owner = integer(&p["owner"])?;
                Ok(PointObservation {
                    at: at(p)?,
                    vp: number(&p["vp"])?,
                    mp: number(&p["mp"])?,
                    owner: if owner < 0 {
                        None
                    } else {
                        Some(owner as usize)
                    },
                    cut: truth(&p["cut"]),
                    locked: truth(&p["locked"]),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let mode = Mode::from_kind(view.mode["kind"].as_str());
        // World admission and its neutral owner are outside the ordinary room path.
        if mode == Mode::World {
            return Err("World commander delivery is not admitted".into());
        }
        let weather = catalog
            .table("WEATHER")
            .map_err(|e| e.to_string())?
            .get(view.weather["now"].as_str().unwrap_or("clear"))
            .ok_or("unknown delivered commander weather")?;
        let weather_number = |key| {
            weather
                .get(key)
                .and_then(Rule::number)
                .ok_or("invalid commander weather rule")
        };
        let sight = weather_number("sight")?;
        let caution = 1.0
            / (sight
                * math::pow(
                    math::min(1.0, weather_number("veh")? * weather_number("offRoad")?),
                    0.5,
                ));
        let standard_army = view.army.is_null() || view.army["pop"].as_f64() == Some(1.0);
        let mut context = Context {
            mode,
            naval: self.naval,
            caution,
            faction,
            munitions: optional_wallet(&me["mun"])?,
            fuel: optional_wallet(&me["fuel"])?,
            large_army: !standard_army,
            height: self.elevation.clone(),
            initial_chars: Some(view.initial_chars.clone()),
            initial_height: view.initial_height.clone(),
            mines: view.mines.iter().map(at).collect::<Result<_>>()?,
            found_mines: view.found_mines.iter().map(at).collect::<Result<_>>()?,
            bridge_cells: view
                .initial_chars
                .iter()
                .enumerate()
                .filter_map(|(i, &ch)| (ch == b'=').then_some(i))
                .collect(),
            sight: if weather
                .get("rains")
                .and_then(Rule::boolean)
                .unwrap_or(false)
            {
                1.0
            } else {
                sight
            },
            horde: mode == Mode::Horde && view.mode["slot"].as_u64() == Some(self.slot as u64),
            defender: matches!(mode, Mode::Horde | Mode::Assault)
                && view.mode["defenderTeam"].as_i64() == me["team"].as_i64(),
            ground_flights: true,
            ..Context::default()
        };
        if let Some(support) = me["sup"].as_object() {
            for (key, value) in support {
                context.support.insert(key.clone(), number(value)?);
            }
        }
        for strike in &view.strikes {
            let owner = usize::try_from(integer(&strike["owner"])?)
                .map_err(|_| "invalid delivered strike owner")?;
            if owner >= players.len() {
                return Err("invalid delivered strike owner".into());
            }
            context
                .strikes
                .push(crate::commander_tactics::AnnouncedStrike {
                    owner,
                    kind: strike["kind"]
                        .as_str()
                        .ok_or("missing delivered strike kind")?
                        .into(),
                    at: at(strike)?,
                    delay: number(&strike["t"])?,
                });
        }
        context.site_nodes = view.nodes.iter().map(at).collect::<Result<Vec<_>>>()?;
        context.nodes = view
            .nodes
            .iter()
            .map(|node| {
                Ok(NodeObservation {
                    at: at(node)?,
                    rate: number(&node["rate"])?,
                    fuel: truth(&node["fuel"]),
                    // A public resource position alone does not prove ground visibility.
                    visible: false,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        context.known_buildings = view
            .known_buildings
            .iter()
            .map(|b| {
                Ok(KnownBuilding {
                    id: super::id(&b["id"])?,
                    kind: b["type"]
                        .as_str()
                        .ok_or("invalid delivered building kind")?
                        .into(),
                    owner: usize::try_from(integer(&b["owner"])?)
                        .map_err(|_| "invalid delivered building owner")?,
                    at: at(b)?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        context.known_buildings.sort_by_key(|b| b.id);
        context.known_depots = context
            .known_buildings
            .iter()
            .filter(|b| b.kind == "depot")
            .map(|b| b.at)
            .collect();
        context.smokes = view
            .smokes
            .iter()
            .enumerate()
            .map(|(i, s)| {
                Ok(Smoke {
                    id: u32::try_from(i + 1).map_err(|_| "too many delivered commander smokes")?,
                    remaining: None,
                    x: number(&s["x"])?,
                    z: number(&s["z"])?,
                    r: number(&s["r"])?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let mut units = Vec::with_capacity(view.units.len());
        let mut human_units = Vec::with_capacity(view.units.len());
        let mut visible = Vec::new();
        let now = view.tick as f64 / 20.0;
        let mut own_structures = Vec::new();
        for (&id, u) in &view.units {
            if super::id(&u["id"])? != id {
                return Err("delivered commander unit identity mismatch".into());
            }
            let owner = usize::try_from(integer(&u["owner"])?)
                .map_err(|_| "invalid delivered commander owner")?;
            let player = players
                .get(owner)
                .ok_or("invalid delivered commander owner")?;
            let own = owner == self.slot;
            let kind = u["type"]
                .as_str()
                .ok_or("invalid delivered commander kind")?;
            let definition = catalog.unit(kind).map_err(|e| e.to_string())?;
            let is = |key| definition.get(key).and_then(Rule::boolean).unwrap_or(false);
            let private = own && !is("structure");
            let path = if private {
                optional_rows(&u["path"])?
                    .iter()
                    .map(at)
                    .collect::<Result<Vec<_>>>()?
            } else {
                vec![]
            };
            let row = UnitObservation {
                id,
                kind: kind.into(),
                owner,
                at: at(u)?,
                hp: number(&u["hp"])?,
                supp: number(&u["supp"])?,
                target_id: u32::try_from(integer(&u["targetId"])?)
                    .map_err(|_| "invalid delivered target")?,
                cover: tech_level(&u["cover"])?,
                path,
                attack_id: private && truth(&u["attackId"]),
                retreating: truth(&u["retreating"]),
                riding: false,
                build: truth(&u["build"]),
                dig: !u["dig"].is_null(),
                entrench: !u["entrench"].is_null(),
                garrison: integer(&u["garrison"])? >= 0,
                enter: private && integer(&u["enter"])? >= 0,
                fire_at: private && integer(&u["fireAt"])? >= 0,
                nade: private && !u["nade"].is_null(),
            };
            let motion_free = private
                && !(row.build
                    || row.retreating
                    || row.attack_id
                    || row.enter
                    || row.fire_at
                    || row.nade
                    || row.dig
                    || row.entrench);
            let amove = if motion_free {
                optional_at(&u["amove"])?
            } else {
                None
            };
            let world_goal = if motion_free && amove.is_none() {
                optional_at(&u["worldGoal"])?.or_else(|| row.path.last().copied())
            } else {
                None
            };
            human_units.push(HumanUnit {
                motion: human_commander_inputs::Unit {
                    id,
                    at: row.at,
                    world_goal,
                    amove,
                    target_id: row.target_id,
                    retreating: row.retreating,
                },
                owner: owner as u32,
                kind: kind.into(),
                hp: row.hp,
                air: is("air"),
                build: row.build,
                dig: row.dig,
                entrench: row.entrench,
                garrison: row.garrison,
            });
            if player.team == team {
                let stock = &u["logistics"];
                context.units.insert(
                    id,
                    UnitFacts {
                        built: if u["built"].is_null() {
                            1.0
                        } else {
                            number(&u["built"])?
                        },
                        hold_position: own && truth(&u["holdPos"]),
                        hold_fire: own && truth(&u["holdFire"]),
                        auto_cast: own && truth(&u["auto"]),
                        cooldown: if own { number(&u["cd"])? } else { 0.0 },
                        auto_retreat: own && truth(&u["autoRetreat"]),
                        queued_orders: if own {
                            optional_rows(&u["orders"])?.len()
                        } else {
                            0
                        },
                        amove,
                        attack_target: if own {
                            u32::try_from(integer(&u["attackId"])?)
                                .map_err(|_| "invalid delivered attack target")?
                        } else {
                            0
                        },
                        queued_amove: if own {
                            optional_rows(&u["orders"])?
                                .iter()
                                .find(|order| order["kind"] == 2)
                                .map(at)
                                .transpose()?
                        } else {
                            None
                        },
                        air_at_base: own && u["air"]["state"] == "base",
                        airborne: is("air")
                            && matches!(
                                u["air"]["state"].as_str(),
                                Some("out" | "station" | "home")
                            ),
                        convoy: own
                            && optional_rows(&view.logistics["trucks"])?
                                .iter()
                                .any(|t| t["id"].as_u64() == Some(id as u64)),
                        stock: if own && stock.is_object() {
                            Some(Stock {
                                ammo: stock["ammo"].as_f64(),
                                provisions: number(&stock["provisions"])?,
                                fuel: stock["fuel"].as_f64(),
                                forced: truth(&stock["forced"]),
                            })
                        } else {
                            None
                        },
                        wall_work: own
                            && u["orderPlan"]["kind"] == 7
                            && view.works.iter().any(|work| {
                                work[5] == "wall"
                                    && work[2].as_f64().is_some_and(|x| {
                                        math::round(x * 10.0) / 10.0
                                            == u["orderPlan"]["x"].as_f64().unwrap_or(f64::NAN)
                                    })
                                    && work[3].as_f64().is_some_and(|z| {
                                        math::round(z * 10.0) / 10.0
                                            == u["orderPlan"]["z"].as_f64().unwrap_or(f64::NAN)
                                    })
                            }),
                    },
                );
            } else {
                visible.push(id);
                if !is("structure") {
                    let rule_number = |key| {
                        definition
                            .get(key)
                            .and_then(Rule::number)
                            .ok_or("invalid commander sighting rule")
                    };
                    let worth = rule_number("cost")? * row.hp
                        / (rule_number("models")? * rule_number("hpPer")?);
                    self.sightings.insert(id, HumanSighting { id, payload: json!({"id":id,"type":kind,"owner":owner,"x":row.at.x,"z":row.at.z,"t":now,"val":worth}) });
                }
            }
            if own && is("structure") {
                own_structures.push((kind, row.hp, context.units[&id].built));
            }
            units.push(row);
        }
        self.sightings
            .retain(|_, s| s.payload["t"].as_f64().is_some_and(|t| now - t <= 60.0));
        if self.sightings.len() > 65_536 {
            return Err("commander sighting capacity exhausted".into());
        }
        self.stationary
            .retain(|id, _| visible.binary_search(id).is_ok());
        for unit in units
            .iter()
            .filter(|u| visible.binary_search(&u.id).is_ok())
        {
            let previous = self.stationary.entry(unit.id).or_insert((unit.at, now));
            if math::hypot(unit.at.x - previous.0.x, unit.at.z - previous.0.z) > 0.1 + 1e-9 {
                *previous = (unit.at, now);
            }
        }
        context.stationary_since = self
            .stationary
            .iter()
            .map(|(&id, &(_, since))| (id, since))
            .collect();
        let research = if me["tech"].is_null() {
            None
        } else {
            let tech = &me["tech"];
            let ready = |kind| {
                own_structures
                    .iter()
                    .filter(|(k, hp, built)| *k == kind && *hp > 0.0 && *built >= 1.0)
                    .count()
            };
            Some(ResearchObservation {
                faction,
                tech: TechState::new(
                    tech_level(&tech["tier"])?,
                    tech_level(&tech["armory"]["inf"])?,
                    tech_level(&tech["armory"]["guns"])?,
                    tech_level(&tech["armory"]["armor"])?,
                )
                .map_err(|e| e.to_string())?,
                busy: optional_rows(&tech["lab"])?
                    .iter()
                    .map(|job| technology(&job[0]))
                    .collect::<Result<Vec<_>>>()?,
                stock_base: own_structures.len() == 2 && ready("hq") == 1 && ready("barracks") == 1,
                armory_ready: ready("armory") > 0,
                standard_army,
            })
        };
        context.field = Some(crate::commander_field_policy::Facts {
            initial_chars: view.initial_chars.clone(),
            initial_height: view.initial_height.clone(),
            ground: view.ground.clone(),
            bridges: view
                .initial_chars
                .iter()
                .zip(&view.chars)
                .enumerate()
                .filter_map(|(cell, (initial, current))| {
                    (*initial == b'=' || *current == b'=').then_some(cell)
                })
                .collect(),
            mines: context.mines.clone(),
            found_mines: context.found_mines.clone(),
            free_nodes: context.site_nodes.clone(),
        });
        let mut input = Input {
            width: self.width,
            height: self.height,
            tick: view.tick,
            slot: self.slot,
            players,
            units,
            points,
            flags: self.flags.clone(),
            chars: self.chars.clone(),
            manpower: number(&me["mp"])?,
            research,
            runtime: Some(context),
        };
        if view.ground.len() != self.chars.len()
            || view.initial_chars.len() != self.chars.len()
            || view
                .initial_height
                .as_ref()
                .is_some_and(|height| height.len() != self.chars.len())
        {
            return Err("invalid delivered initial terrain dimensions".into());
        }
        crate::match_human_observation::complete_ordinary_facts(
            &mut input,
            &view.initial_chars,
            view.initial_height.as_deref(),
        );
        if let Some(field) = input
            .runtime
            .as_mut()
            .and_then(|context| context.field.as_mut())
        {
            field.ground.clone_from(&view.ground);
        }
        let packet = Observation::new(input).map_err(|e| e.to_string())?;
        self.tick = Some(view.tick);
        Ok(HumanView {
            tick: view.tick as u64,
            spawn: Some(packet.input().players[self.slot].spawn),
            units: human_units,
            visible,
            sightings: self.sightings.values().cloned().collect(),
            packet,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_view::SeatMemory;

    fn map() -> PublicMap {
        PublicMap {
            width: 40,
            height: 40,
            players: vec![
                json!({"name":"Own","team":0,"faction":0,"spawn":{"x":10,"z":10}}),
                json!({"name":"Enemy","team":1,"faction":1,"spawn":{"x":70,"z":70}}),
                json!({"name":"Ally","team":0,"faction":2,"spawn":{"x":10,"z":70}}),
            ],
            points: vec![json!({"x":30,"z":40,"vp":2,"mp":3})],
        }
    }
    fn snapshot() -> Value {
        json!({"tick":20,"units":[
            [9,"rifle",0,12,20,0,0,90,12,2,1,0,4096,0,1],
            [2,"rifle",1,30,20,0,0,70,0,9,0,0,0,0,1],
            [4,"rifle",2,20,20,0,0,80,0,2,0,0,0,0,1]],
            "points":[[1,0,0.4,1,0,1]],"out":[false,false,false],"vp":[1,2,3],
            "mp":123,
            "tech":{"tier":2,"armory":{"inf":1,"guns":0,"armor":0},"lab":[["guns",0.3]]},
            "sup":{"smoke":3,"para":0},"cells":[],"weather":["fog"],"army":{"pop":1,"income":1},
            "nodes":[],"foundMines":[],"smokes":[[12,20,3]],"strikes":[],"covers":[],"ghosts":[],
            "plans":[[9,2,40,40,20,20,40,40],[2,5,55,55],[4,1,60,60]],
            "orders":[[9,1,1,30,30],[2,1,1,50,50],[4,1,1,60,60]]})
    }
    fn memory() -> CommanderViewMemory {
        CommanderViewMemory::new(0, map(), vec![b'.'; 1600], Some(vec![0; 1600]), false).unwrap()
    }

    #[test]
    fn delivered_runtime_packet_keeps_own_orders_research_and_masks_foreign_intent() {
        let mut snap = snapshot();
        snap["mun"] = json!(45);
        snap["fuel"] = json!(12);
        for row in snap["units"].as_array_mut().unwrap() {
            row[11] = json!(2.5);
            row[12] = json!(2048 | 4096 | 16384);
        }
        let mut view = SeatMemory::new(0, map()).unwrap().decode(&snap).unwrap();
        for id in [2, 4] {
            assert_eq!(view.units[&id]["cd"], 0);
            assert_eq!(view.units[&id]["holdFire"], false);
            assert_eq!(view.units[&id]["auto"], false);
            // The commander adapter must also mask private facts on a foreign packet.
            let foreign = view.units.get_mut(&id).unwrap();
            foreign["holdFire"] = json!(true);
            foreign["auto"] = json!(true);
            foreign["cd"] = json!(99.0);
        }
        let detached = crate::ai_view::commander::CommanderMemory::new(0, 40, 40)
            .unwrap()
            .observe(&view)
            .unwrap();
        let controls = detached.packet.input().runtime.as_ref().unwrap();
        assert!(controls.units[&9].hold_fire && controls.units[&9].auto_cast);
        assert_eq!(controls.units[&9].cooldown, 2.5);
        assert!(!controls.units[&4].hold_fire && !controls.units[&4].auto_cast);
        assert_eq!(controls.units[&4].cooldown, 0.0);
        assert!(!controls.units.contains_key(&2));
        let human = memory().observe(&view, &snap).unwrap();
        let input = human.packet.input();
        assert_eq!(
            input.units.iter().map(|u| u.id).collect::<Vec<_>>(),
            vec![2, 4, 9]
        );
        assert_eq!(input.manpower, 123.0);
        let runtime = input.runtime.as_ref().unwrap();
        assert_eq!(runtime.munitions, 45.0);
        assert_eq!(runtime.support["smoke"], 3.0);
        assert_eq!(runtime.sight, 0.7);
        assert_eq!(runtime.units[&9].queued_orders, 1);
        assert!(runtime.units[&9].hold_position);
        assert!(runtime.units[&9].hold_fire && runtime.units[&9].auto_cast);
        assert_eq!(runtime.units[&9].cooldown, 2.5);
        assert!(!runtime.units[&4].hold_fire && !runtime.units[&4].auto_cast);
        assert_eq!(runtime.units[&4].cooldown, 0.0);
        assert_eq!(runtime.units[&9].amove, Some(Point { x: 40.0, z: 40.0 }));
        assert!(!runtime.units.contains_key(&2));
        assert_eq!(runtime.units[&4].queued_orders, 0);
        for unit in &input.units[..2] {
            assert!(unit.path.is_empty());
            assert!(!unit.attack_id && !unit.enter && !unit.fire_at && !unit.nade);
        }
        assert_eq!(
            input.research.as_ref().unwrap().busy,
            vec![Technology::Guns]
        );
        assert_eq!(human.visible, vec![2]);
        assert_eq!(human.sightings.len(), 1);
        let screen = crate::human_commander::ScreenView {
            tick: human.tick,
            spawn: human.spawn,
            units: human.units.iter().collect(),
            visible: human.visible.clone(),
            sightings: human.sightings.iter().collect(),
            packet: &human.packet,
        };
        assert!(crate::match_human_observation::human_screen_observation(&screen).is_ok());
    }

    #[test]
    fn delivered_ability_facts_keep_own_cooldown_attack_and_queue_without_foreign_intent() {
        let mut snap = snapshot();
        for index in 0..3 {
            snap["units"][index][11] = json!(2.5);
            snap["units"][index][12] = json!(2048 | 16384);
        }
        snap["plans"] = json!([[9, 4, 30, 20], [2, 4, 12, 20], [4, 4, 30, 20]]);
        snap["orders"] = json!([
            [9, 3, 1, 25, 25, 2, 30, 31, 2, 35, 36],
            [2, 1, 2, 50, 50],
            [4, 1, 2, 60, 60]
        ]);
        let view = SeatMemory::new(0, map()).unwrap().decode(&snap).unwrap();
        let mut memory = memory();
        let human = memory.observe(&view, &snap).unwrap();
        let runtime = human.packet.input().runtime.as_ref().unwrap();
        let own = &runtime.units[&9];
        assert!(own.hold_fire && own.auto_cast);
        assert_eq!(own.cooldown, 2.5);
        assert_eq!(own.attack_target, 2);
        assert_eq!(own.queued_orders, 3);
        assert_eq!(own.queued_amove, Some(Point { x: 30.0, z: 31.0 }));
        assert!(runtime.ground_flights);
        assert!(!runtime.units.contains_key(&2));
        let ally = &runtime.units[&4];
        assert!(!ally.hold_fire && !ally.auto_cast);
        assert_eq!(ally.cooldown, 0.0);
        assert_eq!(ally.attack_target, 0);
        assert_eq!(ally.queued_orders, 0);
        assert_eq!(ally.queued_amove, None);
        // A malformed own cooldown must fail rather than grant a ready ability.
        let mut invalid = view;
        invalid.units.get_mut(&9).unwrap()["cd"] = json!("bad");
        let before = memory.clone();
        assert!(memory.observe(&invalid, &snap).is_err());
        assert_eq!(memory, before);
    }

    #[test]
    fn ordinary_missing_wallets_default_to_zero_with_stock_and_disabled_research() {
        let mut snap = snapshot();
        snap["tech"] = json!({"tier":1,"armory":{"inf":0,"guns":0,"armor":0},"lab":[]});
        assert!(snap.get("mun").is_none() && snap.get("fuel").is_none());
        let mut decoder = SeatMemory::new(0, map()).unwrap();
        let mut memory = memory();
        let view = decoder.decode(&snap).unwrap();
        let human = memory.observe(&view, &snap).unwrap();
        let runtime = human.packet.input().runtime.as_ref().unwrap();
        assert_eq!((runtime.munitions, runtime.fuel), (0.0, 0.0));
        assert!(!runtime.large_army);
        let research = human.packet.input().research.as_ref().unwrap();
        assert_eq!(research.tech.tier(), 1);
        assert_eq!(research.tech.level(Technology::Infantry), 0);
        assert!(research.busy.is_empty() && research.standard_army);
        snap.as_object_mut().unwrap().remove("tech");
        snap["army"] = json!({"pop":2.5,"income":3});
        let view = decoder.decode(&snap).unwrap();
        let human = memory.observe(&view, &snap).unwrap();
        assert!(human.packet.input().research.is_none());
        assert!(human.packet.input().runtime.as_ref().unwrap().large_army);
        // A present malformed wallet must not become a fabricated zero.
        snap["mun"] = json!("bad");
        let view = decoder.decode(&snap).unwrap();
        let saved = memory.clone();
        assert!(memory.observe(&view, &snap).is_err());
        assert_eq!(memory, saved);
    }

    #[test]
    fn delivered_cell_height_allocates_memory_when_public_height_was_absent() {
        let mut snap = snapshot();
        snap["cells"] = json!([[3, "T", 2, 0, {}]]);
        let view = SeatMemory::new(0, map()).unwrap().decode(&snap).unwrap();
        let mut memory = CommanderViewMemory::new(0, map(), vec![b'.'; 1600], None, false).unwrap();
        let human = memory.observe(&view, &snap).unwrap();
        let heights = human
            .packet
            .input()
            .runtime
            .as_ref()
            .unwrap()
            .height
            .as_ref()
            .unwrap();
        assert_eq!(heights[3], 2);
        assert_eq!(heights[4], 0);
    }

    #[test]
    fn terrain_changes_only_from_delivered_cells_and_failed_updates_roll_back() {
        let mut snap = snapshot();
        snap["cells"] = json!([[3,"N",2,0,{"ground":".","object":"#","mineOwned":true}]]);
        let mut decoder = SeatMemory::new(0, map()).unwrap();
        let view = decoder.decode(&snap).unwrap();
        let mut memory = memory();
        let human = memory.observe(&view, &snap).unwrap();
        assert_eq!(human.packet.input().chars[3], b'N');
        assert_eq!(
            human.packet.input().flags[3],
            crate::game::LAND | crate::game::COVER
        );
        assert_eq!(
            human
                .packet
                .input()
                .runtime
                .as_ref()
                .unwrap()
                .height
                .as_ref()
                .unwrap()[3],
            2
        );
        let saved = memory.clone();
        snap["cells"] = json!([[3, "T", 0, 0, {}], [1600, "T", 0, 0, {}]]);
        assert!(memory.observe(&view, &snap).is_err());
        assert_eq!(memory, saved);
        snap["tick"] = json!(22);
        snap["cells"] = json!([]);
        let view = decoder.decode(&snap).unwrap();
        let human = memory.observe(&view, &snap).unwrap();
        assert_eq!(human.packet.input().chars[3], b'N');
        assert_eq!(human.packet.input().chars[4], b'.');
    }

    #[test]
    fn enemy_sightings_survive_missing_delivery_then_expire_and_clock_rewind_fails() {
        let mut snap = snapshot();
        let mut decoder = SeatMemory::new(0, map()).unwrap();
        let mut memory = memory();
        let view = decoder.decode(&snap).unwrap();
        memory.observe(&view, &snap).unwrap();
        snap["tick"] = json!(40);
        snap["units"] = json!([]);
        snap["plans"] = json!([]);
        snap["orders"] = json!([]);
        let view = decoder.decode(&snap).unwrap();
        let human = memory.observe(&view, &snap).unwrap();
        assert!(human.visible.is_empty());
        assert_eq!(human.sightings[0].id, 2);
        snap["tick"] = json!(1240);
        let view = decoder.decode(&snap).unwrap();
        assert!(memory.observe(&view, &snap).unwrap().sightings.is_empty());
        let saved = memory.clone();
        let mut rewind = view.clone();
        rewind.tick = 20;
        snap["tick"] = json!(20);
        assert!(memory.observe(&rewind, &snap).is_err());
        assert_eq!(memory, saved);
    }
}

// Preserve the earlier detached-consumer regression suite as comparison coverage.
#[cfg(test)]
mod delivered {
    //! Human commander packets built exclusively from detached recipient deliveries.

    use super::SeatView;
    use crate::{
        catalog::{Catalog, Value as Rule},
        commander_ai::{
            Input, Observation, PlayerObservation, PointObservation, ResearchObservation,
            UnitObservation,
        },
        commander_tactics::{Context, KnownBuilding, NodeObservation, Stock, UnitFacts},
        economy::{Faction, Mode, TechState, Technology},
        game::{Point, Smoke},
        human_commander::{HumanSighting, HumanUnit, HumanView},
        human_commander_inputs,
        weather::{WeatherKind, WeatherPlan, WeatherRules},
    };
    use serde_json::{Value, json};
    use std::collections::{BTreeMap, BTreeSet};

    type Result<T> = std::result::Result<T, String>;
    const SAFE_TICK: i64 = 9_007_199_254_740_991;

    /// Private sighting history for one seat in one match. Replace on restart or handover.
    #[derive(Clone, Debug, PartialEq)]
    pub struct CommanderMemory {
        slot: usize,
        width: usize,
        height: usize,
        tick: Option<i64>,
        sightings: Vec<HumanSighting>,
    }

    fn number(value: &Value) -> Result<f64> {
        value
            .as_f64()
            .filter(|n| n.is_finite())
            .ok_or_else(|| "invalid delivered commander number".into())
    }
    fn unsigned(value: &Value) -> Result<u32> {
        value
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .ok_or_else(|| "invalid delivered commander integer".into())
    }
    fn byte(value: &Value) -> Result<u8> {
        u8::try_from(unsigned(value)?).map_err(|_| "invalid delivered commander byte".into())
    }
    fn truth(value: &Value) -> bool {
        value
            .as_bool()
            .unwrap_or_else(|| value.as_f64().is_some_and(|n| n != 0.0))
    }
    fn point(value: &Value) -> Result<Point> {
        Ok(Point {
            x: number(&value["x"])?,
            z: number(&value["z"])?,
        })
    }
    fn optional_point(value: &Value) -> Result<Option<Point>> {
        if value.is_null() {
            Ok(None)
        } else {
            point(value).map(Some)
        }
    }
    fn array(value: &Value) -> Result<&[Value]> {
        value
            .as_array()
            .map(Vec::as_slice)
            .ok_or_else(|| "invalid delivered commander array".into())
    }
    fn optional_number(value: &Value) -> Result<Option<f64>> {
        if value.is_null() {
            Ok(None)
        } else {
            number(value).map(Some)
        }
    }
    fn flag(rule: &Rule, key: &str) -> bool {
        rule.get(key).and_then(Rule::boolean).unwrap_or(false)
    }
    fn rule_number(rule: &Rule, key: &str) -> Result<f64> {
        rule.get(key)
            .and_then(Rule::number)
            .filter(|n| n.is_finite())
            .ok_or_else(|| format!("missing commander rule {key}"))
    }

    impl CommanderMemory {
        pub fn new(slot: usize, width: usize, height: usize) -> Result<Self> {
            if slot >= 6 || !(20..=1024).contains(&width) || !(20..=1024).contains(&height) {
                return Err("invalid delivered commander seat or dimensions".into());
            }
            Ok(Self {
                slot,
                width,
                height,
                tick: None,
                sightings: Vec::new(),
            })
        }

        /// Validation and sighting updates commit together after the full packet succeeds.
        /// This accepts no authority handle, visibility oracle or other seat's economy.
        pub fn observe(&mut self, view: &SeatView) -> Result<HumanView<Observation>> {
            if !(0..=SAFE_TICK).contains(&view.tick) || self.tick.is_some_and(|old| view.tick < old)
            {
                return Err("delivered commander clock went backwards or is invalid".into());
            }
            let me = view
                .players
                .get(self.slot)
                .ok_or("missing delivered commander seat")?;
            let mode = Mode::from_kind(view.mode["kind"].as_str());
            if !matches!(mode, Mode::Default | Mode::Conquest | Mode::Horde) {
                return Err("delivered commander requires Conquest or Horde".into());
            }
            let faction = Faction::try_from(byte(&me["faction"])?).map_err(|e| e.to_string())?;
            let players = view
                .players
                .iter()
                .map(|p| {
                    Ok(PlayerObservation {
                        team: p["team"]
                            .as_i64()
                            .and_then(|n| i32::try_from(n).ok())
                            .ok_or("invalid delivered commander team")?,
                        out: truth(&p["out"]),
                        spawn: point(&p["spawn"])?,
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            let team = players[self.slot].team;
            let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
            let standard = catalog
                .table("CFG")
                .map_err(|e| e.to_string())?
                .get("armies")
                .and_then(|a| a.get("standard"))
                .ok_or("missing standard army rule")?;
            let large_army = if view.army.is_null() {
                false
            } else {
                number(&view.army["pop"])? != rule_number(standard, "pop")?
                    || number(&view.army["income"])? != rule_number(standard, "income")?
            };
            let weather = WeatherKind::try_from(view.weather["now"].as_str().unwrap_or("clear"))
                .map_err(|e| e.to_string())?;
            let weather_rule = catalog
                .table("WEATHER")
                .map_err(|e| e.to_string())?
                .get(weather.name())
                .ok_or("missing delivered weather rule")?;
            let mut context = Context {
                mode,
                naval: view.naval,
                faction,
                munitions: optional_number(&me["mun"])?.unwrap_or(0.0),
                fuel: optional_number(&me["fuel"])?.unwrap_or(0.0),
                large_army,
                caution: WeatherRules::compiled()
                    .map_err(|e| e.to_string())?
                    .ai_caution(Some(&WeatherPlan::fixed(weather))),
                sight: if flag(weather_rule, "rains") {
                    1.0
                } else {
                    rule_number(weather_rule, "sight")?
                },
                horde: mode == Mode::Horde && view.mode["slot"].as_u64() == Some(self.slot as u64),
                defender: mode == Mode::Horde
                    && view.mode["defenderTeam"].as_i64() == Some(i64::from(team)),
                height: view.height.clone(),
                ..Context::default()
            };
            for (name, cd) in me["sup"]
                .as_object()
                .ok_or("missing delivered commander support")?
            {
                context.support.insert(name.clone(), number(cd)?);
            }
            let mut units = Vec::with_capacity(view.units.len());
            let mut human_units = Vec::with_capacity(view.units.len());
            let mut visible = Vec::new();
            let trucks = view.logistics["trucks"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|row| unsigned(&row["id"]))
                .collect::<Result<BTreeSet<_>>>()?;
            for (&id, u) in &view.units {
                if id == 0 || unsigned(&u["id"])? != id {
                    return Err("inconsistent delivered commander unit id".into());
                }
                let owner = unsigned(&u["owner"])? as usize;
                let owner_player = players
                    .get(owner)
                    .ok_or("invalid delivered commander unit owner")?;
                let own = owner == self.slot;
                let kind = u["type"]
                    .as_str()
                    .ok_or("missing delivered commander unit type")?;
                let definition = catalog.unit(kind).map_err(|e| e.to_string())?;
                let structure = flag(definition, "structure");
                let flags = unsigned(&u["flags"])?;
                let row = UnitObservation {
                    id,
                    kind: kind.into(),
                    owner,
                    at: point(u)?,
                    hp: number(&u["hp"])?,
                    supp: number(&u["supp"])?,
                    target_id: unsigned(&u["targetId"])?,
                    cover: byte(&u["cover"])?,
                    path: if own && !structure {
                        array(&u["path"])?
                            .iter()
                            .map(point)
                            .collect::<Result<_>>()?
                    } else {
                        vec![]
                    },
                    attack_id: own && !structure && truth(&u["attackId"]),
                    retreating: truth(&u["retreating"]),
                    riding: flags & 262_144 != 0,
                    build: truth(&u["build"]),
                    dig: !u["dig"].is_null(),
                    entrench: !u["entrench"].is_null(),
                    garrison: u["garrison"].as_i64().is_some_and(|n| n >= 0),
                    enter: own && !structure && u["enter"].as_i64().is_some_and(|n| n >= 0),
                    fire_at: own && !structure && u["fireAt"].as_i64().is_some_and(|n| n >= 0),
                    nade: own && !structure && !u["nade"].is_null(),
                };
                let motion_allowed = own
                    && !structure
                    && !(row.build
                        || row.riding
                        || row.retreating
                        || row.attack_id
                        || row.fire_at
                        || row.nade
                        || row.dig
                        || row.entrench
                        || row.enter);
                let amove = if motion_allowed {
                    optional_point(&u["amove"])?
                } else {
                    None
                };
                let world_goal = if motion_allowed && amove.is_none() {
                    optional_point(&u["worldGoal"])?.or_else(|| row.path.last().copied())
                } else {
                    None
                };
                human_units.push(HumanUnit {
                    motion: human_commander_inputs::Unit {
                        id,
                        at: row.at,
                        world_goal,
                        amove,
                        target_id: row.target_id,
                        retreating: row.retreating,
                    },
                    owner: owner as u32,
                    kind: kind.into(),
                    hp: row.hp,
                    air: flag(definition, "air"),
                    build: row.build,
                    dig: row.dig,
                    entrench: row.entrench,
                    garrison: row.garrison,
                });
                if owner_player.team != team {
                    visible.push(id);
                }
                if owner_player.team == team {
                    let stock = if own && u["logistics"].is_object() {
                        let s = &u["logistics"];
                        Some(Stock {
                            ammo: optional_number(&s["ammo"])?,
                            provisions: number(&s["provisions"])?,
                            fuel: optional_number(&s["fuel"])?,
                            forced: truth(&s["forced"]),
                        })
                    } else {
                        None
                    };
                    context.units.insert(
                        id,
                        UnitFacts {
                            built: optional_number(&u["built"])?.unwrap_or(1.0),
                            hold_position: own && truth(&u["holdPos"]),
                            hold_fire: own && truth(&u["holdFire"]),
                            auto_cast: own && truth(&u["auto"]),
                            cooldown: if own { number(&u["cd"])? } else { 0.0 },
                            queued_orders: if own { array(&u["orders"])?.len() } else { 0 },
                            auto_retreat: own && truth(&u["autoRetreat"]),
                            convoy: own && trucks.contains(&id),
                            stock,
                            amove,
                            attack_target: if own { unsigned(&u["attackId"])? } else { 0 },
                            queued_amove: if own {
                                array(&u["orders"])?
                                    .iter()
                                    .find(|o| o["kind"] == 2)
                                    .map(point)
                                    .transpose()?
                            } else {
                                None
                            },
                            air_at_base: own && u["air"]["state"] == "base",
                            airborne: flag(definition, "air")
                                && matches!(
                                    u["air"]["state"].as_str(),
                                    Some("out" | "station" | "home")
                                ),
                            wall_work: own && u["dig"]["kind"] == "wall",
                        },
                    );
                }
                units.push(row);
            }
            for b in &view.known_buildings {
                context.known_buildings.push(KnownBuilding {
                    id: unsigned(&b["id"])?,
                    kind: b["type"]
                        .as_str()
                        .ok_or("invalid delivered building type")?
                        .into(),
                    owner: unsigned(&b["owner"])? as usize,
                    at: point(b)?,
                });
            }
            context.known_buildings.sort_by_key(|b| b.id);
            context.site_nodes = view.nodes.iter().map(point).collect::<Result<_>>()?;
            context.nodes = view
                .nodes
                .iter()
                .map(|node| {
                    Ok(NodeObservation {
                        at: point(node)?,
                        rate: number(&node["rate"])?,
                        fuel: truth(&node["fuel"]),
                        // A public node row does not establish current ground visibility.
                        visible: false,
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            context.known_depots = context
                .known_buildings
                .iter()
                .filter(|b| b.kind == "depot")
                .map(|b| b.at)
                .collect();
            context.smokes = view
                .smokes
                .iter()
                .enumerate()
                .map(|(i, s)| {
                    Ok(Smoke {
                        id: u32::try_from(i + 1).map_err(|_| "too many delivered smokes")?,
                        remaining: None,
                        x: number(&s["x"])?,
                        z: number(&s["z"])?,
                        r: number(&s["r"])?,
                    })
                })
                .collect::<Result<_>>()?;
            let points = view
                .points
                .iter()
                .map(|p| {
                    let owner = p["owner"]
                        .as_i64()
                        .ok_or("invalid delivered objective owner")?;
                    Ok(PointObservation {
                        at: point(p)?,
                        vp: number(&p["vp"])?,
                        mp: number(&p["mp"])?,
                        owner: if owner == -1 {
                            None
                        } else {
                            Some(
                                usize::try_from(owner)
                                    .map_err(|_| "invalid delivered objective owner")?,
                            )
                        },
                        cut: truth(&p["cut"]),
                        locked: truth(&p["locked"]),
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            let research = research(me, &units, &context, catalog)?;
            let packet = Observation::new(Input {
                width: self.width,
                height: self.height,
                tick: view.tick,
                slot: self.slot,
                players,
                units,
                points,
                flags: view.flags.clone(),
                chars: view.chars.clone(),
                manpower: number(&me["mp"])?,
                research,
                runtime: Some(context),
            })
            .map_err(|e| format!("delivered commander observation: {e}"))?;
            let now = view.tick as f64 / 20.0;
            let mut sightings = self.sightings.clone();
            let mut positions: BTreeMap<_, _> = sightings
                .iter()
                .enumerate()
                .map(|(i, s)| (s.id, i))
                .collect();
            for u in packet
                .input()
                .units
                .iter()
                .filter(|u| visible.binary_search(&u.id).is_ok())
            {
                let d = catalog.unit(&u.kind).map_err(|e| e.to_string())?;
                if flag(d, "structure") {
                    continue;
                }
                let worth = rule_number(d, "cost")? * u.hp
                    / (rule_number(d, "models")? * rule_number(d, "hpPer")?);
                if !worth.is_finite() {
                    return Err("invalid delivered sighting worth".into());
                }
                let sighting = HumanSighting {
                    id: u.id,
                    payload: json!({"id":u.id,"type":u.kind,"owner":u.owner,
                    "x":u.at.x,"z":u.at.z,"t":now,"val":worth}),
                };
                if let Some(&i) = positions.get(&u.id) {
                    sightings[i] = sighting;
                } else {
                    positions.insert(u.id, sightings.len());
                    sightings.push(sighting);
                }
            }
            sightings.retain(|s| s.payload["t"].as_f64().is_some_and(|t| now - t <= 60.0));
            if sightings.len() > 65_536 {
                return Err("delivered commander sighting capacity exhausted".into());
            }
            let result = HumanView {
                tick: view.tick as u64,
                spawn: Some(packet.input().players[self.slot].spawn),
                units: human_units,
                visible,
                sightings: sightings.clone(),
                packet,
            };
            self.tick = Some(view.tick);
            self.sightings = sightings;
            Ok(result)
        }
    }

    fn research(
        me: &Value,
        units: &[UnitObservation],
        context: &Context,
        catalog: &Catalog,
    ) -> Result<Option<ResearchObservation>> {
        let t = &me["tech"];
        if t.is_null() {
            return Ok(None);
        }
        let armory = &t["armory"];
        let tech = TechState::new(
            byte(&t["tier"])?,
            byte(&armory["inf"])?,
            byte(&armory["guns"])?,
            byte(&armory["armor"])?,
        )
        .map_err(|e| e.to_string())?;
        let mut busy = Vec::new();
        for row in array(&t["lab"])? {
            let row = array(row)?;
            busy.push(match row.first().and_then(Value::as_str) {
                Some("tier") => Technology::Tier,
                Some("inf") => Technology::Infantry,
                Some("guns") => Technology::Guns,
                Some("armor") => Technology::Armor,
                _ => return Err("invalid delivered research line".into()),
            });
        }
        let owner = unsigned(&me["slot"])? as usize;
        let own_structures = units
            .iter()
            .filter(|u| {
                u.owner == owner && catalog.unit(&u.kind).is_ok_and(|d| flag(d, "structure"))
            })
            .collect::<Vec<_>>();
        let ready = |kind: &str| {
            own_structures
                .iter()
                .filter(|u| {
                    u.kind == kind
                        && u.hp > 0.0
                        && context.units.get(&u.id).is_some_and(|f| f.built >= 1.0)
                })
                .count()
        };
        Ok(Some(ResearchObservation {
            faction: context.faction,
            tech,
            busy,
            stock_base: own_structures.len() == 2 && ready("hq") == 1 && ready("barracks") == 1,
            armory_ready: ready("armory") > 0,
            standard_army: !context.large_army,
        }))
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::ai_view::{PublicMap, SeatMemory};

        fn delivery() -> (SeatMemory, Value) {
            let public = PublicMap {
                width: 40,
                height: 40,
                players: vec![
                    json!({"name":"Defender","team":0,"faction":0,"spawn":{"x":10,"z":10}}),
                    json!({"name":"Horde","team":1,"faction":1,"spawn":{"x":70,"z":70}}),
                    json!({"name":"Ally","team":0,"faction":2,"spawn":{"x":10,"z":70}}),
                ],
                points: vec![json!({"x":30,"z":30,"vp":2,"mp":3})],
            };
            let memory = SeatMemory::new(0, public)
                .unwrap()
                .with_terrain(vec![b'.'; 1600], None, false)
                .unwrap();
            let snap = json!({"tick":20,"units":[
                [1,"rifle",0,10,12,0,0,100,0,0,0,0,0,0,null],
                [2,"rifle",1,25,25,0,0,50,0,0,0,0,0,0,null],
                [3,"rifle",2,12,12,0,0,100,0,0,0,0,0,0,null]],
                "out":[false,false,false],"vp":[0,0,0],"points":[[0,-1,0,0,0,0]],
                "mp":420,"mun":160,"fuel":12,"sup":{"smoke":0,"artillery":5},
                "tech":{"tier":2,"armory":{"inf":1,"guns":0,"armor":0},"lab":[["guns",0.2]]},
                "plans":[[1,2,20,22,15,16],[3,2,70,70]],"orders":[],"queues":[],"productionJobs":[],"air":[],"movement":[],
                "cells":[],"foundMines":[],"nodes":[],"smokes":[[18,18,3]],"strikes":[],"covers":[],
                "weather":["fog"],"mode":{"kind":"horde","slot":1,"defenderTeam":0},"ghosts":[]});
            (memory, snap)
        }

        #[test]
        fn delivered_defender_packet_keeps_own_intent_economy_and_received_terrain() {
            let (mut delivery, mut snap) = delivery();
            snap["cells"] = json!([[5,"N",2,0,{"ground":"D","object":".","mineOwned":true}]]);
            for row in snap["units"].as_array_mut().unwrap() {
                row[11] = json!(3.5);
                row[12] = json!(2048 | 16384);
            }
            let mut view = delivery.decode(&snap).unwrap();
            for id in [2, 3] {
                assert_eq!(view.units[&id]["cd"], 0);
                assert_eq!(view.units[&id]["holdFire"], false);
                assert_eq!(view.units[&id]["auto"], false);
                // Recheck the adapter's owner boundary independently of SeatMemory.
                let foreign = view.units.get_mut(&id).unwrap();
                foreign["holdFire"] = json!(true);
                foreign["auto"] = json!(true);
                foreign["cd"] = json!(99.0);
            }
            let result = CommanderMemory::new(0, 40, 40)
                .unwrap()
                .observe(&view)
                .unwrap();
            let input = result.packet.input();
            let context = input.runtime.as_ref().unwrap();
            assert_eq!(context.mode, Mode::Horde);
            assert!(context.defender);
            assert!(!context.horde);
            assert_eq!(input.manpower, 420.0);
            assert_eq!(context.munitions, 160.0);
            assert_eq!(context.support["artillery"], 5.0);
            assert!(context.units[&1].hold_fire && context.units[&1].auto_cast);
            assert_eq!(context.units[&1].cooldown, 3.5);
            assert!(!context.units[&3].hold_fire && !context.units[&3].auto_cast);
            assert_eq!(context.units[&3].cooldown, 0.0);
            assert!(!context.units.contains_key(&2));
            assert_eq!(
                input.research.as_ref().unwrap().busy,
                vec![Technology::Guns]
            );
            assert_eq!(
                result.units[0].motion.amove,
                Some(Point { x: 20.0, z: 22.0 })
            );
            assert!(result.units[2].motion.amove.is_none());
            assert!(input.units[2].path.is_empty());
            assert_eq!(result.visible, vec![2]);
            assert_eq!(input.chars[5], b'N');
            assert_eq!(
                input.flags[5],
                Catalog::compiled()
                    .unwrap()
                    .table("TERRAIN")
                    .unwrap()
                    .get("D")
                    .unwrap()
                    .number()
                    .unwrap() as u16
            );
            assert_eq!(context.height.as_ref().unwrap()[5], 2);
            snap["tick"] = json!(22);
            snap["cells"] = json!([[5,"W",0,0,{"ground":"W","object":"."}]]);
            let changed = delivery.decode(&snap).unwrap();
            assert_eq!(changed.chars[5], b'W');
            assert_eq!(
                view.chars[5], b'N',
                "a new delivery cannot mutate an old packet"
            );
        }

        #[test]
        fn delivered_sightings_survive_fog_expire_and_roll_back_on_invalid_frames() {
            let (mut delivery, mut snap) = delivery();
            let mut memory = CommanderMemory::new(0, 40, 40).unwrap();
            let first = memory.observe(&delivery.decode(&snap).unwrap()).unwrap();
            assert_eq!(first.sightings.len(), 1);
            snap["tick"] = json!(40);
            snap["units"].as_array_mut().unwrap().remove(1);
            let mut hidden = delivery.decode(&snap).unwrap();
            let old = memory.clone();
            hidden.players[0]["mun"] = json!("invalid");
            assert!(memory.observe(&hidden).is_err());
            assert_eq!(
                memory, old,
                "invalid packets do not publish partial history"
            );
            hidden.players[0]["mun"] = json!(160);
            let remembered = memory.observe(&hidden).unwrap();
            assert!(remembered.visible.is_empty());
            assert_eq!(remembered.sightings, first.sightings);
            snap["tick"] = json!(1222);
            assert!(
                memory
                    .observe(&delivery.decode(&snap).unwrap())
                    .unwrap()
                    .sightings
                    .is_empty()
            );
            assert!(
                memory.observe(&hidden).is_err(),
                "old delivered frames cannot rewind history"
            );
        }
    }
}
