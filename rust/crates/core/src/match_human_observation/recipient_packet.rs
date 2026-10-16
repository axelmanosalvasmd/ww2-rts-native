//! Strategic fields decoded from the recipient projection and public match-start map.
use super::*;
use crate::{
    ai_view::{PublicMap, SeatMemory, SeatView},
    commander_ai::{PlayerObservation, PointObservation, ResearchObservation, UnitObservation},
    commander_tactics::{Context, KnownBuilding, UnitFacts},
    economy::{Faction, Mode, TechState, Technology},
    economy_state::ArmySize,
};
use serde_json::Value;

pub(super) struct PublicFacts {
    map: PublicMap,
    owner: usize,
    faction: Faction,
    army: ArmySize,
    naval: bool,
    chars: Vec<u8>,
    height: Option<Vec<i8>>,
}
fn number(v: &Value) -> Result<f64, String> {
    v.as_f64()
        .filter(|v| v.is_finite())
        .ok_or_else(|| "invalid delivered commander number".into())
}
fn at(v: &Value) -> Result<Point, String> {
    Ok(Point {
        x: number(&v["x"])?,
        z: number(&v["z"])?,
    })
}
fn truth(v: &Value) -> bool {
    v.as_bool()
        .unwrap_or_else(|| v.as_f64().is_some_and(|v| v != 0.0))
}
impl PublicFacts {
    pub(super) fn capture(state: &Match, owner: usize) -> Result<Self, String> {
        let initial = state.world.initial();
        // The downloaded public map hides authored mines, as ai-view.js:104-113 does.
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
        Ok(Self {
            map: PublicMap {
                width: state.world.game.w,
                height: state.world.game.h,
                players: state.start.players.iter().map(|p| json!({
                    "name":p.config.name,"team":p.config.team,"faction":p.config.faction as u8,
                    "spawn":{"x":p.spawn.x,"z":p.spawn.z}
                })).collect(),
                points: state.start.points.iter().map(|p| json!({"x":p.at.x,"z":p.at.z,"vp":p.vp,"mp":p.mp})).collect(),
            },
            owner,
            faction: state.options.players.get(owner).ok_or("missing commander public seat")?.faction,
            army: state.options.army,
            naval: state.world.game.naval,
            chars,
            height: initial.height.clone(),
        })
    }
    fn view(&self, snapshot: &Value) -> Result<SeatView, String> {
        SeatMemory::new(self.owner, self.map.clone())?.decode(snapshot)
    }
    pub(super) fn decode(&self, snapshot: &Value) -> Result<(Input, Vec<HumanUnit>), String> {
        let view = self.view(snapshot)?;
        let mut units = Vec::new();
        let mut human = Vec::new();
        let mut facts = BTreeMap::new();
        let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
        for (&id, u) in &view.units {
            let owner = u["owner"].as_u64().ok_or("invalid delivered unit owner")? as usize;
            let own = owner == self.owner;
            let kind = u["type"]
                .as_str()
                .ok_or("invalid delivered kind")?
                .to_owned();
            let definition = catalog.unit(&kind).map_err(|e| e.to_string())?;
            let path = u["path"]
                .as_array()
                .ok_or("missing delivered path")?
                .iter()
                .map(at)
                .collect::<Result<Vec<_>, _>>()?;
            let target_id = u["targetId"].as_u64().ok_or("invalid delivered target")? as u32;
            let row = UnitObservation {
                id,
                kind: kind.clone(),
                owner,
                at: at(u)?,
                hp: number(&u["hp"])?,
                supp: number(&u["supp"])?,
                target_id,
                cover: u["cover"].as_u64().ok_or("invalid delivered cover")? as u8,
                path,
                attack_id: truth(&u["attackId"]),
                retreating: truth(&u["retreating"]),
                riding: u["flags"].as_u64().unwrap_or(0) & 262144 != 0,
                build: truth(&u["build"]),
                dig: !u["dig"].is_null(),
                entrench: !u["entrench"].is_null(),
                garrison: u["garrison"].as_i64().is_some_and(|v| v >= 0),
                enter: u["enter"].as_i64().is_some_and(|v| v >= 0),
                fire_at: u["fireAt"].as_i64().is_some_and(|v| v >= 0),
                nade: !u["nade"].is_null(),
            };
            let world_goal = u
                .get("worldGoal")
                .filter(|v| !v.is_null())
                .map(at)
                .transpose()?;
            let amove = u
                .get("amove")
                .filter(|v| !v.is_null())
                .map(at)
                .transpose()?;
            let air = definition
                .get("air")
                .and_then(RuleValue::boolean)
                .unwrap_or(false);
            human.push(HumanUnit {
                motion: human_commander_inputs::Unit {
                    id,
                    at: row.at,
                    world_goal,
                    amove,
                    target_id,
                    retreating: row.retreating,
                },
                owner: owner as u32,
                kind,
                hp: row.hp,
                air,
                build: row.build,
                dig: row.dig,
                entrench: row.entrench,
                garrison: row.garrison,
            });
            facts.insert(
                id,
                UnitFacts {
                    built: number(&u["built"])?,
                    hold_position: truth(&u["holdPos"]),
                    hold_fire: truth(&u["holdFire"]),
                    auto_cast: truth(&u["auto"]),
                    cooldown: number(&u["cd"])?,
                    queued_orders: u["orders"].as_array().map_or(0, Vec::len),
                    queued_amove: u["orders"]
                        .as_array()
                        .and_then(|orders| orders.iter().find(|o| o["kind"] == 2))
                        .map(at)
                        .transpose()?,
                    auto_retreat: truth(&u["autoRetreat"]),
                    // Enabled or retained logistics is refused before this partial projection.
                    convoy: false,
                    stock: None,
                    amove,
                    attack_target: if own {
                        u["attackId"].as_u64().unwrap_or(0) as u32
                    } else {
                        0
                    },
                    airborne: air && u["air"]["state"] != "base" && u["air"]["state"] != "rearm",
                    wall_work: own
                        && u["orderPlan"]["kind"] == 7
                        && snapshot["works"].as_array().is_some_and(|works| {
                            works.iter().any(|w| {
                                w[5] == "wall"
                                    && w[2].as_f64().is_some_and(|x| {
                                        math::round(x * 10.0) / 10.0
                                            == u["orderPlan"]["x"].as_f64().unwrap_or(f64::NAN)
                                    })
                                    && w[3].as_f64().is_some_and(|z| {
                                        math::round(z * 10.0) / 10.0
                                            == u["orderPlan"]["z"].as_f64().unwrap_or(f64::NAN)
                                    })
                            })
                        }),
                },
            );
            units.push(row);
        }
        let me = &view.players[self.owner];
        let ready = |kind: &str| {
            view.units
                .values()
                .filter(|u| {
                    u["owner"].as_u64() == Some(self.owner as u64)
                        && u["type"] == kind
                        && u["hp"].as_f64().is_some_and(|v| v > 0.0)
                        && u["built"].as_f64().is_some_and(|v| v >= 1.0)
                })
                .count()
        };
        let own_structures = units
            .iter()
            .filter(|u| {
                u.owner == self.owner
                    && catalog
                        .unit(&u.kind)
                        .ok()
                        .and_then(|d| d.get("structure"))
                        .and_then(RuleValue::boolean)
                        .unwrap_or(false)
            })
            .count();
        let tech = &me["tech"];
        let research = if tech.is_null() {
            None
        } else {
            let level = |v: &Value| {
                v.as_u64()
                    .and_then(|v| u8::try_from(v).ok())
                    .ok_or("invalid delivered research level")
            };
            Some(ResearchObservation {
                faction: self.faction,
                tech: TechState::new(
                    level(&tech["tier"])?,
                    level(&tech["armory"]["inf"])?,
                    level(&tech["armory"]["guns"])?,
                    level(&tech["armory"]["armor"])?,
                )
                .map_err(|e| format!("delivered research: {e:?}"))?,
                busy: tech["lab"]
                    .as_array()
                    .ok_or("missing delivered lab")?
                    .iter()
                    .map(|row| match row[0].as_str() {
                        Some("tier") => Ok(Technology::Tier),
                        Some("inf") => Ok(Technology::Infantry),
                        Some("guns") => Ok(Technology::Guns),
                        Some("armor") => Ok(Technology::Armor),
                        _ => Err("invalid delivered technology".into()),
                    })
                    .collect::<Result<Vec<_>, String>>()?,
                stock_base: own_structures == 2 && ready("hq") == 1 && ready("barracks") == 1,
                armory_ready: ready("armory") > 0,
                standard_army: self.army == ArmySize::Standard,
            })
        };
        let now = crate::weather::WeatherKind::try_from(
            view.weather["now"]
                .as_str()
                .ok_or("missing delivered weather")?,
        )
        .map_err(|e| format!("delivered weather: {e:?}"))?;
        let weather = crate::weather::WeatherRules::compiled().map_err(|e| e.to_string())?;
        let rules = crate::rules::Rules::compiled().map_err(|e| e.to_string())?;
        let current = &rules.weather[rules
            .weather_kind(now.name())
            .ok_or("missing weather rule")?];
        let terrain = catalog.table("TERRAIN").map_err(|e| e.to_string())?;
        let flags = self
            .chars
            .iter()
            .map(|ch| {
                terrain
                    .get(&char::from(*ch).to_string())
                    .and_then(RuleValue::number)
                    .unwrap_or(0.0) as u16
            })
            .collect();
        let context = Context {
            mode: Mode::from_kind(view.mode["kind"].as_str()),
            horde: view.mode["kind"] == "horde"
                && view.mode["slot"].as_u64() == Some(self.owner as u64),
            defender: matches!(
                view.mode["kind"].as_str(),
                Some("assault" | "annihilation" | "horde")
            ) && view.mode["defenderTeam"].as_i64() == me["team"].as_i64(),
            ground_flights: true,
            naval: self.naval,
            faction: self.faction,
            caution: weather.ai_caution(Some(&crate::weather::WeatherPlan::fixed(now))),
            munitions: me["mun"].as_f64().unwrap_or(0.0),
            fuel: me["fuel"].as_f64().unwrap_or(0.0),
            large_army: self.army != ArmySize::Standard,
            height: self.height.clone(),
            initial_chars: Some(self.chars.clone()),
            initial_height: self.height.clone(),
            sight: if current.rains { 1.0 } else { current.sight },
            support: me["sup"]
                .as_object()
                .ok_or("missing delivered support")?
                .iter()
                .map(|(k, v)| Ok((k.clone(), number(v)?)))
                .collect::<Result<_, String>>()?,
            smokes: view
                .smokes
                .iter()
                .enumerate()
                .map(|(i, s)| {
                    Ok(crate::game::Smoke {
                        id: snapshot["smokes"][i][3].as_u64().unwrap_or(0) as u32,
                        remaining: None,
                        x: number(&s["x"])?,
                        z: number(&s["z"])?,
                        r: number(&s["r"])?,
                    })
                })
                .collect::<Result<_, String>>()?,
            known_buildings: view
                .known_buildings
                .iter()
                .map(|b| {
                    Ok(KnownBuilding {
                        id: b["id"].as_u64().ok_or("invalid delivered building")? as u32,
                        kind: b["type"]
                            .as_str()
                            .ok_or("invalid delivered building type")?
                            .into(),
                        owner: b["owner"]
                            .as_u64()
                            .ok_or("invalid delivered building owner")?
                            as usize,
                        at: at(b)?,
                    })
                })
                .collect::<Result<_, String>>()?,
            nodes: view.nodes.iter().map(at).collect::<Result<_, _>>()?,
            site_nodes: view
                .nodes
                .iter()
                .enumerate()
                .filter(|(i, _)| !view.claimed_nodes.contains(i))
                .map(|(_, n)| at(n))
                .collect::<Result<_, _>>()?,
            known_depots: view
                .known_buildings
                .iter()
                .filter(|b| b["type"] == "depot")
                .map(at)
                .collect::<Result<_, _>>()?,
            units: facts,
            ..Context::default()
        };
        Ok((
            Input {
                width: self.map.width,
                height: self.map.height,
                tick: view.tick,
                slot: self.owner,
                players: view
                    .players
                    .iter()
                    .map(|p| {
                        Ok(PlayerObservation {
                            team: p["team"].as_i64().ok_or("invalid delivered team")? as i32,
                            out: truth(&p["out"]),
                            spawn: at(&p["spawn"])?,
                        })
                    })
                    .collect::<Result<_, String>>()?,
                points: view
                    .points
                    .iter()
                    .map(|p| {
                        Ok(PointObservation {
                            at: at(p)?,
                            vp: number(&p["vp"])?,
                            mp: number(&p["mp"])?,
                            owner: p["owner"].as_u64().map(|v| v as usize),
                            cut: truth(&p["cut"]),
                            locked: truth(&p["locked"]),
                        })
                    })
                    .collect::<Result<_, String>>()?,
                units,
                chars: self.chars.clone(),
                flags,
                manpower: number(&me["mp"])?,
                research,
                runtime: Some(context),
            },
            human,
        ))
    }
    pub(super) fn populate_mines(&self, input: &mut Input, snapshot: &Value) -> Result<(), String> {
        // Decode the actual delivered cell ownership and found-mine list together.
        let view = self.view(snapshot)?;
        if let Some(context) = &mut input.runtime {
            context.mines = view.mines.iter().map(at).collect::<Result<_, _>>()?;
            context.found_mines = view.found_mines.iter().map(at).collect::<Result<_, _>>()?;
        }
        Ok(())
    }
}
