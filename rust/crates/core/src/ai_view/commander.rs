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
    stationary: BTreeMap<u32, (Point, f64)>,
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
            stationary: BTreeMap::new(),
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
        if !matches!(
            mode,
            Mode::Default
                | Mode::Conquest
                | Mode::Classic
                | Mode::Assault
                | Mode::Annihilation
                | Mode::Horde
                | Mode::Tutorial
        ) {
            return Err("delivered commander requires an ordinary mode".into());
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
        if view.ground.len() != self.width * self.height {
            return Err("invalid delivered commander ground dimensions".into());
        }
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
            defender: matches!(mode, Mode::Assault | Mode::Annihilation | Mode::Horde)
                && view.mode["defenderTeam"].as_i64() == Some(i64::from(team)),
            height: view.height.clone(),
            initial_chars: Some(view.initial_chars.clone()),
            initial_height: view.initial_height.clone(),
            mines: view.mines.iter().map(point).collect::<Result<_>>()?,
            found_mines: view.found_mines.iter().map(point).collect::<Result<_>>()?,
            ground_flights: true,
            ..Context::default()
        };
        for (name, cd) in me["sup"]
            .as_object()
            .ok_or("missing delivered commander support")?
        {
            context.support.insert(name.clone(), number(cd)?);
        }
        for strike in &view.strikes {
            let owner = unsigned(&strike["owner"])? as usize;
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
                    at: point(strike)?,
                    delay: number(&strike["t"])?,
                });
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
                        attack_target: if own { unsigned(&u["attackId"])? } else { 0 },
                        queued_amove: if own {
                            array(&u["orders"])?
                                .iter()
                                .find(|order| order["kind"] == 2)
                                .map(point)
                                .transpose()?
                        } else {
                            None
                        },
                        queued_orders: if own { array(&u["orders"])?.len() } else { 0 },
                        auto_retreat: own && truth(&u["autoRetreat"]),
                        convoy: own && trucks.contains(&id),
                        stock,
                        amove,
                        air_at_base: own && u["air"]["state"] == "base",
                        airborne: flag(definition, "air")
                            && matches!(
                                u["air"]["state"].as_str(),
                                Some("out" | "station" | "home")
                            ),
                        wall_work: own
                            && u["orderPlan"]["kind"] == 7
                            && view.works.iter().any(|work| {
                                work[5] == "wall"
                                    && work[2].as_f64().is_some_and(|x| {
                                        crate::math::round(x * 10.0) / 10.0
                                            == u["orderPlan"]["x"].as_f64().unwrap_or(f64::NAN)
                                    })
                                    && work[3].as_f64().is_some_and(|z| {
                                        crate::math::round(z * 10.0) / 10.0
                                            == u["orderPlan"]["z"].as_f64().unwrap_or(f64::NAN)
                                    })
                            }),
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
        context.nodes = view
            .nodes
            .iter()
            .map(|node| {
                Ok(NodeObservation {
                    at: point(node)?,
                    rate: number(&node["rate"])?,
                    fuel: truth(&node["fuel"]),
                    visible: false,
                })
            })
            .collect::<Result<_>>()?;
        context.site_nodes = view
            .nodes
            .iter()
            .enumerate()
            .filter(|(index, _)| !view.claimed_nodes.contains(index))
            .map(|(_, node)| point(node))
            .collect::<Result<_>>()?;
        context.known_depots = context
            .known_buildings
            .iter()
            .filter(|b| b.kind == "depot")
            .map(|b| b.at)
            .collect();
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
        let now = view.tick as f64 / 20.0;
        let mut stationary = self.stationary.clone();
        stationary.retain(|id, _| visible.binary_search(id).is_ok());
        for unit in units
            .iter()
            .filter(|u| visible.binary_search(&u.id).is_ok())
        {
            let previous = stationary.entry(unit.id).or_insert((unit.at, now));
            if crate::math::hypot(unit.at.x - previous.0.x, unit.at.z - previous.0.z) > 0.1 + 1e-9 {
                *previous = (unit.at, now);
            }
        }
        context.stationary_since = stationary
            .iter()
            .map(|(&id, &(_, since))| (id, since))
            .collect();
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
        self.stationary = stationary;
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
        for row in snap["units"].as_array_mut().unwrap() {
            row[11] = json!(7);
            row[12] = json!(2048 | 4096 | 8192 | 16384);
        }
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
        assert!(context.units[&1].hold_fire && context.units[&1].auto_cast);
        assert_eq!(context.units[&1].cooldown, 7.0);
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
    fn delivered_mode_roles_and_own_tactics_facts_follow_recipient_rows() {
        let (mut delivery, mut snap) = delivery();
        snap["units"][0][11] = json!(7.5);
        snap["units"][0][12] = json!(16 | 2048 | 4096 | 8192 | 16384);
        snap["plans"] = json!([[1, 7, 20.1, 22.2]]);
        snap["orders"] = json!([[1, 1, 2, 31.2, 32.3]]);
        snap["works"] = json!([[0, 0, 20.11, 22.21, 0, "wall"]]);
        snap["nodes"] = json!([[30, 31, 1, 0]]);
        snap["cells"] = json!([[5,"N",2,0,{"ground":"D","object":".","mineOwned":true}]]);
        for (kind, defender) in [
            ("conquest", false),
            ("classic", false),
            ("assault", true),
            ("annihilation", true),
            ("horde", true),
        ] {
            snap["mode"] = json!({"kind":kind,"slot":1,"defenderTeam":0});
            let view = delivery.decode(&snap).unwrap();
            let packet = CommanderMemory::new(0, 40, 40)
                .unwrap()
                .observe(&view)
                .unwrap();
            let context = packet.packet.input().runtime.as_ref().unwrap();
            assert_eq!(context.defender, defender, "{kind}");
            assert!(!context.horde);
            assert!(context.ground_flights);
            assert_eq!(context.initial_chars.as_ref().unwrap()[5], b'.');
            assert_eq!(context.mines, vec![Point { x: 11.0, z: 1.0 }]);
            assert_eq!(context.site_nodes, vec![Point { x: 30.0, z: 31.0 }]);
            let own = &context.units[&1];
            assert!(own.hold_fire && own.hold_position && own.auto_cast && own.auto_retreat);
            assert_eq!(own.cooldown, 7.5);
            assert_eq!(own.queued_amove, Some(Point { x: 31.2, z: 32.3 }));
            assert!(own.wall_work);
            assert!(!context.units[&3].hold_fire);
        }
        snap["mode"] = json!({"kind":"horde","slot":0,"defenderTeam":1});
        let packet = CommanderMemory::new(0, 40, 40)
            .unwrap()
            .observe(&delivery.decode(&snap).unwrap())
            .unwrap();
        let context = packet.packet.input().runtime.as_ref().unwrap();
        assert!(context.horde && !context.defender);
        let mut tutorial = delivery.decode(&snap).unwrap();
        tutorial.mode = json!({"kind":"tutorial"});
        let tutorial_packet = CommanderMemory::new(0, 40, 40)
            .unwrap()
            .observe(&tutorial)
            .unwrap();
        assert_eq!(
            tutorial_packet
                .packet
                .input()
                .runtime
                .as_ref()
                .unwrap()
                .mode,
            Mode::Tutorial
        );
        let view = delivery.decode(&snap).unwrap();
        for kind in ["world"] {
            let mut view = view.clone();
            view.mode = json!({"kind":kind});
            assert!(
                CommanderMemory::new(0, 40, 40)
                    .unwrap()
                    .observe(&view)
                    .is_err()
            );
        }
    }

    #[test]
    fn delivered_field_facts_keep_road_ground_and_crossings_across_detached_views() {
        let (mut delivery, mut snap) = delivery();
        let mut start = vec![b'.'; 1600];
        start[6] = b'=';
        delivery = delivery.with_terrain(start, None, false).unwrap();
        snap["cells"] = json!([[5,"R",0,0,{"ground":"D","object":"R"}]]);
        let first = delivery.decode(&snap).unwrap();
        let mut commander = CommanderMemory::new(0, 40, 40).unwrap();
        let packet = commander.observe(&first).unwrap();
        let field = packet
            .packet
            .input()
            .runtime
            .as_ref()
            .unwrap()
            .field
            .as_ref()
            .unwrap();
        assert_eq!(field.ground[5], b'D');
        assert_eq!(field.ground[6], b'W');
        assert_eq!(field.initial_chars[6], b'=');
        assert!(field.bridges.contains(&6));
        snap["tick"] = json!(22);
        snap["cells"] = json!([[5,"F",0,0,{"ground":"F","object":"."}]]);
        let flooded = delivery.decode(&snap).unwrap();
        let packet = commander.observe(&flooded).unwrap();
        assert_eq!(
            packet
                .packet
                .input()
                .runtime
                .as_ref()
                .unwrap()
                .field
                .as_ref()
                .unwrap()
                .ground[5],
            b'F'
        );
        assert_eq!(
            first.ground[5], b'D',
            "later delivery preserves the old detached ground"
        );
        let before = delivery.clone();
        snap["tick"] = json!(24);
        snap["cells"] = json!([[5,"R",0,0,{"ground":"DD","object":"R"}]]);
        assert!(delivery.decode(&snap).is_err());
        assert_eq!(
            delivery, before,
            "invalid delivered layers do not publish terrain"
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
    #[test]
    fn intent_ai34_stationary_gun_moves_or_disappears_between_attention_looks() {
        use crate::{
            commander_ai::Level,
            commander_tactics::{self, State},
            random::MatchRandom,
        };
        for disappeared in [false, true] {
            let (mut delivery, mut snap) = delivery();
            snap["units"][1][1] = json!("mg");
            snap["sup"] = json!({"artillery":0});
            snap["mp"] = json!(5000);
            snap["tech"] = Value::Null;
            let mut memory = CommanderMemory::new(0, 40, 40).unwrap();
            let first = memory.observe(&delivery.decode(&snap).unwrap()).unwrap();
            assert_eq!(
                first
                    .packet
                    .input()
                    .runtime
                    .as_ref()
                    .unwrap()
                    .stationary_since[&2],
                1.
            );
            let mut private = State::default();
            let mut random = MatchRandom::new(34);
            let commands = commander_tactics::plan(
                &first.packet,
                &[],
                &mut private,
                Level::Normal,
                &mut random,
            )
            .unwrap();
            assert!(commands.iter().all(|c| c["kind"] != "artillery"));
            // The recipient producer keeps observing even when attention does not plan.
            snap["tick"] = json!(40);
            if disappeared {
                snap["units"].as_array_mut().unwrap().remove(1);
            } else {
                snap["units"][1][3] = json!(26);
            }
            memory.observe(&delivery.decode(&snap).unwrap()).unwrap();
            snap["tick"] = json!(60);
            if disappeared {
                snap["units"].as_array_mut().unwrap().insert(
                    1,
                    json!([2, "mg", 1, 25, 25, 0, 0, 50, 0, 0, 0, 0, 0, 0, null]),
                );
            } else {
                snap["units"][1][3] = json!(25);
            }
            let returned = memory.observe(&delivery.decode(&snap).unwrap()).unwrap();
            assert_eq!(
                returned
                    .packet
                    .input()
                    .runtime
                    .as_ref()
                    .unwrap()
                    .stationary_since[&2],
                3.
            );
            snap["tick"] = json!(100);
            let next = memory.observe(&delivery.decode(&snap).unwrap()).unwrap();
            let commands = commander_tactics::plan(
                &next.packet,
                &[],
                &mut private,
                Level::Normal,
                &mut random,
            )
            .unwrap();
            assert!(
                commands.iter().all(|c| c["kind"] != "artillery"),
                "{commands:?}"
            );
            snap["tick"] = json!(140);
            let still = memory.observe(&delivery.decode(&snap).unwrap()).unwrap();
            let commands = commander_tactics::plan(
                &still.packet,
                &[],
                &mut private,
                Level::Normal,
                &mut random,
            )
            .unwrap();
            assert!(
                commands.iter().any(|c| c["kind"] == "artillery"),
                "a truly stationary watched gun becomes eligible: {commands:?}"
            );
        }
    }

    #[test]
    fn intent_ai34_invalid_delivery_cannot_reset_stationary_history() {
        let (mut delivery, mut snap) = delivery();
        let mut memory = CommanderMemory::new(0, 40, 40).unwrap();
        memory.observe(&delivery.decode(&snap).unwrap()).unwrap();
        let before = memory.clone();
        snap["tick"] = json!(40);
        snap["units"][1][3] = json!(26);
        let mut invalid = delivery.decode(&snap).unwrap();
        invalid.players[0]["mp"] = json!("invalid");
        assert!(memory.observe(&invalid).is_err());
        assert_eq!(memory, before);
    }

    #[test]
    fn intent_ai34_attention_filter_prunes_packet_clocks_and_preserves_delivery_history() {
        use crate::{
            commander_ai::Level,
            commander_tactics::{self, State},
            human_commander::ScreenView,
            match_human_observation::human_screen_observation,
            random::MatchRandom,
        };
        let (mut delivery, snap) = delivery();
        let mut memory = CommanderMemory::new(0, 40, 40).unwrap();
        let view = memory.observe(&delivery.decode(&snap).unwrap()).unwrap();
        assert!(
            view.packet
                .input()
                .runtime
                .as_ref()
                .unwrap()
                .stationary_since
                .contains_key(&2)
        );
        let screen = ScreenView {
            tick: view.tick,
            spawn: view.spawn,
            units: view.units.iter().filter(|u| u.motion.id != 2).collect(),
            visible: vec![],
            sightings: vec![],
            packet: &view.packet,
        };
        let packet = human_screen_observation(&screen).unwrap();
        assert!(
            !packet
                .input()
                .runtime
                .as_ref()
                .unwrap()
                .stationary_since
                .contains_key(&2)
        );
        commander_tactics::plan(
            &packet,
            &[],
            &mut State::default(),
            Level::Normal,
            &mut MatchRandom::new(34),
        )
        .unwrap();
        assert!(memory.stationary.contains_key(&2));
        assert!(
            view.packet
                .input()
                .runtime
                .as_ref()
                .unwrap()
                .stationary_since
                .contains_key(&2)
        );
    }
}
