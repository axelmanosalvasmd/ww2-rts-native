//! HUD availability from delivered recipient state. This never borrows a Match or World.
use crate::{
    catalog::{Catalog, Value as Rule},
    client_view::{ClientView, UnitRow},
    economy::{self, AbilityCost, Currency, Mode, TechState, Technology},
    math,
};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Availability {
    pub ok: bool,
    pub reason: String,
}
fn yes() -> Availability {
    Availability {
        ok: true,
        reason: String::new(),
    }
}
fn no(reason: impl Into<String>) -> Availability {
    Availability {
        ok: false,
        reason: reason.into(),
    }
}
fn num(v: &Value) -> f64 {
    v.as_f64().unwrap_or(f64::NAN)
}
fn number(rule: &Rule, field: &str, default: f64) -> f64 {
    rule.get(field).and_then(Rule::number).unwrap_or(default)
}
fn flag(rule: &Rule, field: &str) -> bool {
    rule.get(field).and_then(Rule::boolean) == Some(true)
}
fn strings(rule: Option<&Rule>) -> Vec<&str> {
    rule.and_then(Rule::array)
        .unwrap_or(&[])
        .iter()
        .filter_map(Rule::string)
        .collect()
}
fn rows(value: &Value) -> &[Value] {
    value.as_array().map_or(&[], Vec::as_slice)
}

/// Present the existing server denial code without inspecting a target or hidden state.
pub fn deny_sentence(code: &str, command: Option<&str>) -> Result<String, String> {
    let code = if code == "queueFull" && command.is_some_and(|cmd| !cmd.is_empty() && cmd != "buy")
    {
        "ordersFull"
    } else {
        code
    };
    let sentence = match code {
        "mp" => "Not enough manpower",
        "mun" => "Not enough munitions",
        "fuel" => "Not enough fuel",
        "ammo" => "Not enough carried ammunition",
        "pop" => "The army is at its limit",
        "cooldown" => "That order is still on cooldown",
        "unseen" => "That target is not visible",
        "notVisible" => "That spot is not visible",
        "blocked" => "That spot is blocked or uneven",
        "needs" => "The required unit or building is missing",
        "max" => "That unit or position is at its limit",
        "suddenDeath" => "Not during Sudden Death",
        "queueFull" => "The training queue is full",
        "ordersFull" => "This unit already has 8 queued orders",
        "retreating" => "That squad is retreating",
        "noBuilders" => "Select a builder squad",
        "noCover" => "No cover within reach",
        "notWaiting" => "That recruit is no longer waiting",
        "scenario" => "This scenario cannot start with the selected sides, factions or mode",
        "territory" => "Build inside territory your team owns",
        "coast" => "A Shipyard needs open water beside it",
        "tier" => "Needs a higher HQ tier",
        "rail" => "Too close to the railway or a station",
        "save" => "The match could not be saved or loaded",
        "station" => {
            let cfg = Catalog::compiled()
                .and_then(|c| c.table("CFG"))
                .map_err(|e| e.to_string())?;
            return Ok(format!(
                "Rail moves start within {} m of one of your stations and go to another station on your land",
                cfg.get("rail")
                    .map_or(0.0, |rail| number(rail, "reach", 0.0))
            ));
        }
        "railCut" => "No open line: every region on the way must be yours and its bridges standing",
        "airfield" => {
            let cfg = Catalog::compiled()
                .and_then(|c| c.table("CFG"))
                .map_err(|e| e.to_string())?;
            return Ok(format!(
                "Paratroopers fly from one of your finished Airfields, at most {} m away",
                number(cfg, "paraRange", 0.0)
            ));
        }
        "shore" => "Too far from the shore to land",
        _ => "That order cannot happen",
    };
    Ok(sentence.into())
}
fn resources(s: &Value, mp: f64, fuel: f64, mun: f64) -> Availability {
    if num(&s["mp"]) < mp {
        no(format!("Needs {mp} MP"))
    } else if fuel != 0.0
        && num(&s["fuel"])
            .partial_cmp(&fuel)
            .is_none_or(|order| order.is_lt())
    {
        no(format!("Needs {fuel} fuel"))
    } else if mun != 0.0
        && num(&s["mun"])
            .partial_cmp(&mun)
            .is_none_or(|order| order.is_lt())
    {
        no(format!("Needs {mun} munitions"))
    } else {
        yes()
    }
}
fn queued<'a>(s: &'a Value, unit: &UnitRow) -> &'a [Value] {
    rows(&s["queues"])
        .iter()
        .find(|q| num(&q[0]) == f64::from(unit.id))
        .map_or(&[], |q| rows(q).get(4..).unwrap_or(&[]))
}
fn production_access(s: &Value, action: &Value, owner: i32, slot: i32) -> bool {
    if owner == slot {
        return true;
    }
    if s["mode"]["kind"] != "horde"
        || num(&s["mode"]["slot"]) == f64::from(slot)
        || num(&s["mode"]["slot"]) == f64::from(owner)
        || owner < 0
        || slot < 0
    {
        return false;
    }
    let teams = rows(&action["teams"]);
    teams
        .get(owner as usize)
        .zip(teams.get(slot as usize))
        .is_some_and(|(a, b)| num(a) == num(b))
}

/// The action has the browser's local selection/seat/team fields, not an authoritative command.
/// Unknown catalog names return an error instead of guessing an enable state.
pub fn availability(view: Option<&ClientView>, action: &Value) -> Result<Availability, String> {
    let Some(view) = view.filter(|v| !v.state().is_null()) else {
        return Ok(no("Waiting for the army"));
    };
    let s = view.state();
    if action["watching"] == true {
        return Ok(no("You are spectating"));
    }
    let slot = action["slot"]
        .as_f64()
        .filter(|v| v.fract() == 0.0 && *v >= f64::from(i32::MIN) && *v <= f64::from(i32::MAX))
        .map(|v| v as i32);
    if slot.is_some_and(|slot| slot >= 0 && s["out"][slot as usize] == true) {
        return Ok(no("You are spectating"));
    }
    let catalog = Catalog::compiled().map_err(|e| e.to_string())?;
    let cfg = catalog.table("CFG").map_err(|e| e.to_string())?;
    let unit = |kind: &str| catalog.unit(kind).map_err(|e| e.to_string());
    let mode = Mode::from_kind(s["mode"]["kind"].as_str());
    let classic = mode.is_construction();
    let skirmish = mode.is_skirmish();
    let sudden = classic && s["mode"]["suddenDeath"] == true;
    let own: Vec<_> = view.units().filter(|u| Some(u.owner) == slot).collect();
    let selected: Vec<_> = own
        .iter()
        .copied()
        .filter(|u| {
            rows(&action["ids"])
                .iter()
                .any(|id| num(id) == f64::from(u.id))
        })
        .collect();
    let queue: Vec<_> = own
        .iter()
        .flat_map(|u| queued(s, u))
        .filter_map(Value::as_str)
        .collect();
    let facilities: Vec<_> = view
        .units()
        .filter(|u| slot.is_some_and(|slot| production_access(s, action, u.owner, slot)))
        .collect();
    let population = |need: f64| -> Result<Availability, String> {
        let mut used = 0.0;
        for u in &own {
            if !flag(unit(&u.kind)?, "structure") {
                used += economy::pop_use(&u.kind).map_err(|e| e.to_string())?;
            }
        }
        for kind in &queue {
            used += economy::pop_use(kind).map_err(|e| e.to_string())?;
        }
        let base = if classic {
            cfg.get("classic").unwrap_or(cfg)
        } else {
            cfg
        };
        let cap = s["world"]["cap"].as_f64().unwrap_or_else(|| {
            math::round(number(base, "popCap", 0.0) * s["army"]["pop"].as_f64().unwrap_or(1.0))
        });
        Ok(if used + need > cap {
            no(format!("Army at its limit ({used}/{cap})"))
        } else {
            yes()
        })
    };
    let tier_name = |tier: u8| -> Result<&str, String> {
        catalog
            .table("TIER_NAMES")
            .map_err(|e| e.to_string())?
            .array()
            .and_then(|a| a.get(usize::from(tier)))
            .and_then(Rule::string)
            .ok_or("invalid HQ tier".into())
    };
    let required_tier = |kind: &str| -> Result<Option<Availability>, String> {
        let required = economy::tier_of(kind).map_err(|e| e.to_string())?;
        (!s["tech"].is_null() && f64::from(required) > num(&s["tech"]["tier"]))
            .then(|| tier_name(required).map(|name| no(format!("Needs {name}"))))
            .transpose()
    };
    let selected_crew = |builders: &[&str]| -> Availability {
        let crew: Vec<_> = selected
            .iter()
            .filter(|u| builders.contains(&u.kind.as_str()))
            .collect();
        if crew.is_empty() {
            no("Select a builder squad")
        } else if crew.iter().all(|u| u.flags & 1 != 0) {
            no("That squad is retreating")
        } else {
            yes()
        }
    };
    let result = match action["t"].as_str().unwrap_or("") {
        "cancelProduction" => {
            if sudden {
                return Ok(no("Not during Sudden Death"));
            }
            let jobs = rows(&s["productionJobs"])
                .iter()
                .find(|r| num(&r[0]) == num(&action["id"]))
                .map(|r| rows(&r[1]))
                .unwrap_or(&[]);
            if own
                .iter()
                .any(|u| f64::from(u.id) == num(&action["id"]) && u.built >= 1.0)
                && jobs
                    .iter()
                    .any(|j| num(&j["id"]) == num(&action["job"]) && j["status"] == "waiting")
            {
                yes()
            } else {
                no("That recruit is no longer waiting")
            }
        }
        "buy" => {
            let kind = action["unit"].as_str().ok_or("buy needs a unit type")?;
            let Ok(def) = unit(kind) else {
                return Ok(no("Unavailable in this mode"));
            };
            if flag(def, "structure") || (flag(def, "classic") && !classic) {
                return Ok(no("Unavailable in this mode"));
            }
            if flag(def, "naval") && action["naval"] != true {
                return Ok(no("Needs a map with a sea"));
            }
            if let Some(denial) = required_tier(kind)? {
                return Ok(denial);
            }
            let price = economy::price_of(mode, kind).map_err(|e| e.to_string())?;
            let money = resources(s, price.mp, price.fuel, 0.0);
            if !money.ok {
                return Ok(money);
            }
            let pop = population(economy::pop_use(kind).map_err(|e| e.to_string())?)?;
            if !pop.ok {
                return Ok(pop);
            }
            let have = own.iter().filter(|u| u.kind == kind).count()
                + queue.iter().filter(|t| **t == kind).count();
            if have as f64 >= number(def, "max", f64::INFINITY) {
                return Ok(no("Maximum of this unit reached"));
            }
            if classic || skirmish {
                if sudden {
                    return Ok(no("Not during Sudden Death"));
                }
                let mut makers = Vec::new();
                for u in if skirmish { &facilities } else { &own } {
                    let makes = strings(unit(&u.kind)?.get("makes"));
                    if u.built >= 1.0 && makes.contains(&kind) && (!skirmish || u.hp > 0.0)
                        || skirmish
                            && u.built >= 1.0
                            && u.hp > 0.0
                            && u.kind == "barracks"
                            && kind == "rifle"
                    {
                        makers.push(*u);
                    }
                }
                if makers.is_empty() {
                    let mut name = "Production Building";
                    for t in strings(Some(
                        catalog.table("UNIT_TYPES").map_err(|e| e.to_string())?,
                    )) {
                        let maker = unit(t)?;
                        if strings(maker.get("makes")).contains(&kind) {
                            name = maker.get("name").and_then(Rule::string).unwrap_or(name);
                            break;
                        }
                    }
                    return Ok(no(format!("Needs a {name}")));
                }
                if !makers.iter().any(|u| {
                    (skirmish || queued(s, u).len() < 5)
                        && (action.get("from").is_none() || num(&action["from"]) == f64::from(u.id))
                }) {
                    return Ok(no(if skirmish {
                        "Selected building cannot produce this unit"
                    } else {
                        "The training queue is full"
                    }));
                }
            }
            yes()
        }
        "support" => {
            let kind = action["kind"].as_str().ok_or("support needs a kind")?;
            let cd = s["sup"][kind].as_f64().unwrap_or(0.0);
            if cd > 0.0 {
                return Ok(no(format!("Cooldown {} s", math::ceil(cd))));
            }
            let cost = economy::sup_cost(mode, kind).map_err(|e| e.to_string())?;
            let money = resources(
                s,
                if cost.currency == Currency::Manpower {
                    cost.cost
                } else {
                    0.0
                },
                0.0,
                if cost.currency == Currency::Munitions {
                    cost.cost
                } else {
                    0.0
                },
            );
            if !money.ok {
                return Ok(money);
            }
            if kind == "para"
                && mode == Mode::World
                && !own.iter().any(|u| u.kind == "airfield" && u.built >= 1.0)
            {
                return Ok(no(deny_sentence("airfield", None)?));
            }
            if kind == "para" {
                population(economy::drop_pop(kind).map_err(|e| e.to_string())?)?
            } else {
                yes()
            }
        }
        "build" => {
            let kind = action["kind"].as_str().ok_or("build needs a kind")?;
            if !economy::build_kinds(mode)
                .map_err(|e| e.to_string())?
                .contains(&kind)
                || kind == "armory" && s["tech"].is_null()
            {
                return Ok(no("Unavailable in this mode"));
            }
            if let Some(denial) = required_tier(kind)? {
                return Ok(denial);
            }
            if sudden {
                return Ok(no("Not during Sudden Death"));
            }
            let crew = selected_crew(&economy::builder_types(mode).map_err(|e| e.to_string())?);
            if !crew.ok {
                return Ok(crew);
            }
            let def = unit(kind)?;
            let money = resources(s, number(def, "cost", 0.0), 0.0, 0.0);
            if !money.ok {
                return Ok(money);
            }
            if let Some(needs) = def.get("needs").and_then(Rule::string)
                && !facilities
                    .iter()
                    .any(|u| u.kind == needs && u.built >= 1.0 && u.hp > 0.0)
            {
                return Ok(no(format!(
                    "Needs a {}",
                    unit(needs)?
                        .get("name")
                        .and_then(Rule::string)
                        .unwrap_or(needs)
                )));
            }
            yes()
        }
        "tech" => {
            let t = &s["tech"];
            if t.is_null() {
                return Ok(no("Unavailable in this mode"));
            }
            if sudden {
                return Ok(no("Not during Sudden Death"));
            }
            let kind = action["kind"].as_str().ok_or("tech needs a kind")?;
            let technology = match kind {
                "tier" => Technology::Tier,
                "inf" => Technology::Infantry,
                "guns" => Technology::Guns,
                "armor" => Technology::Armor,
                _ => return Err("unknown research kind".into()),
            };
            let byte = |v: &Value| {
                v.as_f64()
                    .filter(|n| n.fract() == 0.0 && (0.0..=255.0).contains(n))
                    .map(|n| n as u8)
                    .ok_or("invalid recipient research state")
            };
            let state = TechState::new(
                byte(&t["tier"])?,
                byte(&t["armory"]["inf"])?,
                byte(&t["armory"]["guns"])?,
                byte(&t["armory"]["armor"])?,
            )
            .map_err(|e| e.to_string())?;
            let Some(cost) =
                economy::tech_cost(state, technology, mode).map_err(|e| e.to_string())?
            else {
                return Ok(no("Fully researched"));
            };
            if rows(&t["lab"]).iter().any(|r| r[0] == kind) {
                return Ok(no("Already researching"));
            }
            let armory = kind != "tier";
            if armory && num(&t["armory"][kind]) >= num(&t["tier"]) {
                return Ok(no(format!("Needs {}", tier_name(state.tier() + 1)?)));
            }
            let labs = facilities
                .iter()
                .filter(|u| {
                    u.kind == if armory { "armory" } else { "hq" } && u.built >= 1.0 && u.hp > 0.0
                })
                .count();
            if labs == 0 {
                return Ok(no(format!(
                    "Needs a finished {}",
                    if armory { "Armory" } else { "HQ" }
                )));
            }
            if armory && rows(&t["lab"]).iter().filter(|r| r[0] != "tier").count() >= labs {
                return Ok(no("Each Armory researches one line at a time"));
            }
            resources(s, cost.mp, 0.0, cost.mun)
        }
        "dig" | "entrench" => {
            let entrench = action["t"] == "entrench";
            let fort = if entrench {
                None
            } else {
                Some(
                    catalog
                        .table("FORTS")
                        .map_err(|e| e.to_string())?
                        .get(action["kind"].as_str().ok_or("dig needs a kind")?)
                        .ok_or("unknown fort kind")?,
                )
            };
            if let Some(fort) = fort {
                let tier = number(fort, "tier", 0.0);
                if !s["tech"].is_null() && tier > num(&s["tech"]["tier"]) {
                    return Ok(no(format!("Needs {}", tier_name(tier as u8)?)));
                }
            }
            let builders = if fort.is_some_and(|f| f.get("unit").is_some()) {
                economy::builder_types(mode).map_err(|e| e.to_string())?
            } else {
                strings(cfg.get("fortBuilders"))
            };
            let crew = selected_crew(&builders);
            if !crew.ok {
                return Ok(crew);
            }
            if entrench || action["queue"] == true {
                yes()
            } else {
                resources(s, number(fort.unwrap(), "cost", 0.0), 0.0, 0.0)
            }
        }
        "stance" => {
            let mut active = false;
            for u in &selected {
                active |= !flag(unit(&u.kind)?, "structure");
            }
            if active { yes() } else { no("Select a unit") }
        }
        "cover" => {
            let mut infantry = Vec::new();
            for u in &selected {
                if flag(unit(&u.kind)?, "infantry") {
                    infantry.push(u);
                }
            }
            if infantry.is_empty() {
                no("Select an infantry squad")
            } else if infantry.iter().all(|u| u.flags & 1 != 0) {
                no("That squad is retreating")
            } else {
                yes()
            }
        }
        "ability" => {
            let kind = action["unit"].as_str().ok_or("ability needs a unit")?;
            let crew: Vec<_> = selected.iter().filter(|u| u.kind == kind).collect();
            if crew.is_empty() {
                return Ok(no("Select a squad"));
            }
            let active: Vec<_> = crew.iter().filter(|u| u.flags & 1 == 0).collect();
            if active.is_empty() {
                return Ok(no("That squad is retreating"));
            }
            let ab = unit(kind)?.get("ab").ok_or("missing unit ability")?;
            let id = ab
                .get("id")
                .and_then(Rule::string)
                .ok_or("missing ability id")?;
            if action["queue"] == true && ["grenade", "barrage", "satchel"].contains(&id) {
                return Ok(yes());
            }
            if active.iter().all(|u| u.cd > 0.0) {
                return Ok(no(format!(
                    "Cooldown {} s",
                    math::ceil(
                        active
                            .iter()
                            .fold(f64::INFINITY, |cd, u| math::min(cd, u.cd))
                    )
                )));
            }
            let cost = economy::ab_cost(
                mode,
                AbilityCost {
                    id,
                    munitions: ab.get("mun").and_then(Rule::number),
                },
            )
            .map_err(|e| e.to_string())?;
            resources(s, 0.0, 0.0, cost)
        }
        _ => yes(),
    };
    Ok(result)
}
