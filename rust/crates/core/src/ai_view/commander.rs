//! Human commander packets built exclusively from detached recipient deliveries.

use super::SeatView;
use crate::{
    catalog::{Catalog, Value as Rule},
    commander_ai::{
        Input, Observation, PlayerObservation, PointObservation, ResearchObservation,
        UnitObservation,
    },
    commander_tactics::{Context, KnownBuilding, Stock, UnitFacts},
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
        if !(0..=SAFE_TICK).contains(&view.tick) || self.tick.is_some_and(|old| view.tick < old) {
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
                        queued_orders: if own { array(&u["orders"])?.len() } else { 0 },
                        auto_retreat: own && truth(&u["autoRetreat"]),
                        convoy: own && trucks.contains(&id),
                        stock,
                        amove,
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
        context.nodes = view.nodes.iter().map(point).collect::<Result<_>>()?;
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
        .filter(|u| u.owner == owner && catalog.unit(&u.kind).is_ok_and(|d| flag(d, "structure")))
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
        let view = delivery.decode(&snap).unwrap();
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
