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
            sight: if weather
                .get("rains")
                .and_then(Rule::boolean)
                .unwrap_or(false)
            {
                1.0
            } else {
                sight
            },
            horde: mode == Mode::Horde,
            defender: matches!(mode, Mode::Horde | Mode::Assault)
                && view.mode["defenderTeam"].as_i64() == me["team"].as_i64(),
            ground_flights: false,
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
                        auto_retreat: own && truth(&u["autoRetreat"]),
                        queued_orders: if own {
                            optional_rows(&u["orders"])?.len()
                        } else {
                            0
                        },
                        amove,
                        air_at_base: own && is("air") && u["air"]["state"] == "base",
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
                        wall_work: false,
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
        let packet = Observation::new(Input {
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
        })
        .map_err(|e| e.to_string())?;
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
        let view = SeatMemory::new(0, map()).unwrap().decode(&snap).unwrap();
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
        assert!(!runtime.ground_flights);
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
