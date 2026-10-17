use super::*;
use crate::{
    catalog::Value as RuleValue,
    game::{PlaneState, Point},
    horde_commander::{DeliveredIntent, DeliveredPlan, OwnPlan, OwnUnit, View, Wave},
    horde_policy::{Facts, PointFact, Strike, Unit},
    match_commands::MoveOutcome,
};

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

fn owner(value: &Value, count: usize) -> Result<usize> {
    usize::try_from(integer(value)?)
        .ok()
        .filter(|slot| *slot < count)
        .ok_or_else(|| "invalid delivered Horde owner".into())
}

fn plan(value: &Value) -> Result<DeliveredPlan> {
    let kind = u8::try_from(integer(&value["kind"])?)
        .ok()
        .filter(|kind| *kind <= 9)
        .ok_or("invalid delivered Horde plan kind")?;
    Ok(DeliveredPlan {
        kind,
        at: at(value)?,
    })
}

fn optional_cell(value: &Value) -> Result<Option<usize>> {
    match integer(value)? {
        -1 => Ok(None),
        cell => usize::try_from(cell)
            .map(Some)
            .map_err(|_| "invalid delivered Horde cell".into()),
    }
}

impl SeatView {
    /// Adapt only this detached recipient delivery and the public rules catalog.
    pub fn horde_packet(&self, slot: usize) -> Result<(View, Facts)> {
        if self.mode["kind"] != "horde"
            || self.mode["slot"].as_u64() != Some(slot as u64)
            || slot >= self.players.len()
        {
            return Err("Horde packet requires its scripted recipient seat".into());
        }
        let wave = Wave {
            slot,
            number: u32::try_from(integer(&self.mode["wave"])?)
                .map_err(|_| "invalid delivered Horde wave")?,
            active: self.mode["active"]
                .as_bool()
                .ok_or("invalid delivered Horde activity")?,
        };
        let me = &self.players[slot];
        let spawn = at(&me["spawn"])?;
        let teams = self
            .players
            .iter()
            .map(|p| {
                i32::try_from(integer(&p["team"])?)
                    .map_err(|_| "invalid delivered Horde team".into())
            })
            .collect::<Result<Vec<_>>>()?;
        let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
        let mut facts = Facts {
            tick: self.tick,
            slot,
            spawn,
            teams,
            manpower: number(&me["mp"])?,
            support: me["sup"]
                .as_object()
                .ok_or("missing delivered Horde support cooldowns")?
                .iter()
                .map(|(kind, cooldown)| Ok((kind.clone(), number(cooldown)?)))
                .collect::<Result<_>>()?,
            units: BTreeMap::new(),
            points: Vec::new(),
            strikes: Vec::new(),
        };
        let mut own = BTreeMap::new();
        for (&uid, u) in &self.units {
            if id(&u["id"])? != uid {
                return Err("mismatched delivered Horde unit id".into());
            }
            let kind = u["type"].as_str().ok_or("invalid Horde unit kind")?;
            let rule = catalog.unit(kind).map_err(|e| e.to_string())?;
            let unit_owner = owner(&u["owner"], self.players.len())?;
            let air = rule
                .get("air")
                .and_then(RuleValue::boolean)
                .unwrap_or(false);
            let plane = if air {
                Some(match u["air"]["state"].as_str() {
                    Some("base") => PlaneState::Base,
                    Some("out") => PlaneState::Out,
                    Some("station") => PlaneState::Station,
                    Some("home") => PlaneState::Home,
                    Some("rearm") => PlaneState::Rearm,
                    _ => return Err("invalid delivered Horde aircraft state".into()),
                })
            } else {
                None
            };
            let unit = Unit {
                id: uid,
                owner: unit_owner,
                kind: kind.into(),
                at: at(u)?,
                hp: number(&u["hp"])?,
                suppression: number(&u["supp"])?,
                plane,
                cooldown: if unit_owner == slot {
                    number(&u["cd"])?
                } else {
                    0.0
                },
                auto_retreat: unit_owner == slot && truth(&u["autoRetreat"]),
                garrisoned: integer(&u["garrison"])? >= 0,
                in_cover: number(&u["cover"])? != 0.0,
                riding: integer(&u["flags"])? & 262144 != 0,
            };
            if unit_owner == slot {
                let delivered = DeliveredIntent {
                    plan: u.get("orderPlan").map(plan).transpose()?,
                    orders: rows(&u["orders"])?
                        .iter()
                        .map(plan)
                        .collect::<Result<_>>()?,
                    digging: !u["dig"].is_null(),
                };
                let outcome = match u.get("moveOutcome").and_then(Value::as_str) {
                    None => None,
                    Some("interrupted") => Some(MoveOutcome::Interrupted),
                    Some("moving") => Some(MoveOutcome::Moving),
                    Some("unreachable") => Some(MoveOutcome::Unreachable),
                    Some("completed") => Some(MoveOutcome::Arrived),
                    _ => return Err("invalid delivered Horde movement outcome".into()),
                };
                // Private generations, route ends, command targets and paid Dig
                // internals are absent from delivery. Raw tuples retain equality.
                let intent = OwnPlan {
                    delivered: Some(delivered),
                    path_generation: 0,
                    goal: optional_at(&u["worldGoal"])?,
                    route_end: None,
                    outcome,
                    outcome_tick: u.get("moveOutcomeTick").map(integer).transpose()?,
                    path: rows(&u["path"])?.iter().map(at).collect::<Result<_>>()?,
                    amove: optional_at(&u["amove"])?,
                    attack: u32::try_from(integer(&u["attackId"])?)
                        .map_err(|_| "invalid delivered Horde attack id")?,
                    target: u32::try_from(integer(&u["targetId"])?)
                        .map_err(|_| "invalid delivered Horde target id")?,
                    retreating: truth(&u["retreating"]),
                    orders: Default::default(),
                    dig: None,
                    entrench: !u["entrench"].is_null(),
                    build: truth(&u["build"]).then_some(1),
                    enter: optional_cell(&u["enter"])?,
                    fire_at: optional_cell(&u["fireAt"])?,
                    nade: optional_at(&u["nade"])?,
                    hold: truth(&u["holdPos"]),
                };
                own.insert(
                    uid,
                    OwnUnit {
                        id: uid,
                        owner: slot,
                        hp: unit.hp,
                        air,
                        structure: rule
                            .get("structure")
                            .and_then(RuleValue::boolean)
                            .unwrap_or(false),
                        plan: intent,
                    },
                );
            }
            facts.units.insert(uid, unit);
        }
        for p in &self.points {
            facts.points.push(PointFact {
                at: at(p)?,
                owner: if integer(&p["owner"])? == -1 {
                    None
                } else {
                    Some(owner(&p["owner"], self.players.len())?)
                },
                visible: true,
            });
        }
        for strike in &self.strikes {
            facts.strikes.push(Strike {
                at: at(strike)?,
                owner: owner(&strike["owner"], self.players.len())?,
                kind: strike["kind"]
                    .as_str()
                    .ok_or("invalid Horde strike kind")?
                    .into(),
                delay: number(&strike["t"])?,
            });
        }
        let bunker = facts
            .units
            .values()
            .filter(|unit| {
                unit.hp > 0.0
                    && unit.kind == "bunker"
                    && facts.teams[unit.owner] != facts.teams[slot]
            })
            .min_by(|a, b| {
                math::hypot(a.at.x - spawn.x, a.at.z - spawn.z)
                    .total_cmp(&math::hypot(b.at.x - spawn.x, b.at.z - spawn.z))
                    .then(a.id.cmp(&b.id))
            })
            .map(|unit| unit.at);
        let view = View {
            tick: self.tick,
            wave,
            units: own,
            bunker,
        };
        facts.validate(&view)?;
        Ok((view, facts))
    }
}

#[cfg(test)]
mod tests;
