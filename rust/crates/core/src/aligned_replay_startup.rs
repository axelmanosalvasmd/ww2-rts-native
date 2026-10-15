//! Complete-startup candidate. Included only by the existing cfg(test) replay module.
//! Every observed field has an exact, source-backed sentinel/default or checked inactive disposition.
use super::{Match, Replay};
use crate::{
    economy::{Faction, Mode, Technology},
    economy_state::ArmySize,
    match_runtime::{MatchOptions, PlayerConfig},
    weather::WeatherChoice,
};
use serde_json::{Map, Value, json};

fn n(value: f64) -> Value {
    json!({"bits":format!("{:016x}",value.to_bits()),
        "value":if value.is_finite(){json!(value)}else{json!(value.to_string())}})
}
fn none() -> Value {
    json!({"$none":true})
}
fn option<T>(value: Option<T>, encode: impl FnOnce(T) -> Value) -> Value {
    value.map_or_else(none, encode)
}
fn raw(value: &impl std::fmt::Debug) -> Value {
    json!({"$unmappedNativeRepresentation":format!("{value:?}")})
}
fn array_or_raw<T: std::fmt::Debug>(values: &[T]) -> Value {
    if values.is_empty() {
        json!([])
    } else {
        raw(&values)
    }
}
fn catalog(value: &crate::catalog::Value) -> Value {
    use crate::catalog::Value as C;
    match value {
        C::Null => Value::Null,
        C::Bool(v) => json!(v),
        C::Number(v) => n(*v),
        C::String(v) => json!(v),
        C::Array(v) => Value::Array(v.iter().map(catalog).collect()),
        C::Object(v) => Value::Object(v.iter().map(|(k, v)| (k.clone(), catalog(v))).collect()),
    }
}
fn point(p: crate::game::Point) -> Value {
    json!({"x":n(p.x),"z":n(p.z)})
}
fn numbers(values: impl IntoIterator<Item = f64>) -> Value {
    Value::Array(values.into_iter().map(n).collect())
}
fn chars(values: &[u8]) -> Value {
    Value::Array(
        values
            .iter()
            .map(|c| json!(char::from(*c).to_string()))
            .collect(),
    )
}
fn typed(kind: &str, values: impl IntoIterator<Item = f64>) -> Value {
    json!({"$typed":kind,"values":numbers(values)})
}
fn f32s(values: &[f32]) -> Value {
    let mut result = typed("Float32Array", values.iter().map(|v| f64::from(*v)));
    result["storageBits"] = json!(
        values
            .iter()
            .map(|v| format!("{:08x}", v.to_bits()))
            .collect::<Vec<_>>()
    );
    result
}
fn map(values: impl IntoIterator<Item = (Value, Value)>) -> Value {
    json!({"$map":values.into_iter().map(|(k,v)|json!([k,v])).collect::<Vec<_>>()})
}
// Only buildingCells has cell-membership semantics. Other observed maps remain ordered.
// Source sim.js:1202,5865 reads by cell; native match_routing.rs reads buildings.get(&c).
fn building_cells_equal(source: &Value, native: &Value) -> bool {
    fn number(value: &Value) -> Option<f64> {
        let object = value.as_object()?;
        if object.len() != 2 {
            return None;
        }
        let text = object.get("bits")?.as_str()?;
        if text.len() != 16 || !text.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let bits = u64::from_str_radix(text, 16).ok()?;
        let number = f64::from_bits(bits);
        let display = object.get("value")?.as_f64()?;
        // JSON.stringify(-0) displays 0. Recorded bits still distinguish the two zeros.
        let consistent = display.to_bits() == bits || display == 0.0 && number == 0.0;
        (number.is_finite() && consistent).then_some(number)
    }
    fn entries(value: &Value) -> Option<std::collections::BTreeMap<String, &Value>> {
        let object = value.as_object()?;
        if object.len() != 1 {
            return None;
        }
        let rows = object.get("$map")?.as_array()?;
        let mut entries = std::collections::BTreeMap::new();
        for row in rows {
            let row = row.as_array()?;
            if row.len() != 2 {
                return None;
            }
            let cell = number(&row[0])?;
            if cell < 0.0 || cell.fract() != 0.0 {
                return None;
            }
            let record = row[1].as_object()?;
            if record.len() != 4 {
                return None;
            }
            for field in ["id", "owner", "x", "z"] {
                number(record.get(field)?)?;
            }
            let key = row[0]["bits"].as_str()?.to_owned();
            if entries.insert(key, &row[1]).is_some() {
                return None;
            }
        }
        Some(entries)
    }
    let (Some(source), Some(native)) = (entries(source), entries(native)) else {
        return false;
    };
    source.len() == native.len()
        && source
            .iter()
            .all(|(cell, record)| native.get(cell).is_some_and(|other| exact(record, other)))
}

fn set(values: impl IntoIterator<Item = Value>) -> Value {
    json!({"$set":values.into_iter().collect::<Vec<_>>()})
}
macro_rules! obj {
    ($($key:literal => $value:expr),* $(,)?) => {{
        let mut object = Map::new();
        $(object.insert($key.into(), $value);)*
        Value::Object(object)
    }};
}
fn cell_row(r: &crate::world::CellRow) -> Value {
    let d = &r.data;
    let mut data = obj! {"ground"=>json!(char::from(d.ground).to_string()),
    "object"=>json!(char::from(d.object).to_string()),"material"=>json!(d.material),
    "groundMaterial"=>json!(d.ground_material),"groundWear"=>n(f64::from(d.ground_wear))};
    if let Some(v) = d.trench_front {
        data["trenchFront"] = n(f64::from(v));
    }
    if d.mine {
        data["mine"] = json!(true);
    }
    if let Some(v) = &d.mine_material {
        data["mineMaterial"] = json!(v);
    }
    if let Some(v) = d.rail {
        data["rail"] = n(f64::from(v));
    }
    if let Some(v) = d.station {
        data["station"] = n(v as f64);
    }
    if let Some(v) = &d.section {
        data["section"] = obj! {"id"=>json!(v.id),"structureId"=>json!(v.structure_id),
        "state"=>json!(v.state),"hp"=>n(v.hp),"maxHp"=>n(v.max_hp),"material"=>json!(v.material),"anchor"=>json!(v.anchor)};
    }
    json!([
        n(r.cell as f64),
        char::from(r.ch).to_string(),
        n(f64::from(r.height)),
        n(f64::from(r.state)),
        data
    ])
}
fn section(m: &Match, id: &str, cell: usize, entity: Option<u32>) -> Value {
    let (meta, hp) = m
        .world
        .section_by_id(id)
        .expect("actual structure section provider");
    let mut value = obj! {"id"=>json!(meta.id),"structureId"=>json!(meta.structure_id),"c"=>n(cell as f64),
    "hp"=>n(hp.hp),"maxHp"=>option(hp.max_hp,n),"material"=>json!(meta.material),
    "anchor"=>json!(meta.anchor),"supports"=>json!(meta.supports),"rubble"=>numbers(meta.rubble.iter().map(|v|*v as f64)),
    "state"=>json!(if hp.failed {"failed"} else { match meta.appearance {
        crate::world::Appearance::Intact=>"intact",crate::world::Appearance::Damaged=>"damaged",crate::world::Appearance::Burning=>"burning"} })};
    if let Some(v) = entity {
        value["entityId"] = n(f64::from(v));
    }
    value
}
fn unit(m: &Match, u: &crate::game::Unit) -> Value {
    let c = m
        .commands
        .units
        .get(&u.id)
        .expect("actual control provider");
    let f = m.firing.units.get(&u.id).expect("actual firing provider");
    let a = m.abilities.get(u.id).expect("actual ability provider");
    let h = m.combat.get(u.id).expect("actual combat provider");
    let r = m.recovery.get(u.id).expect("actual recovery provider");
    let mut value = obj! {
        "id"=>n(f64::from(u.id)),
        "type"=>json!(m.rules.kinds[u.kind].name),
        "owner"=>n(u.owner as f64),
        "x"=>n(u.x),
        "z"=>n(u.z),
        "rot"=>n(u.rot),
        "aim"=>n(c.aim),
        "moveSpeed"=>n(u.move_speed),
        "vx"=>option(u.vx,n),
        "vz"=>option(u.vz,n),
        "travelDir"=>option(u.travel_dir,n),
        "hp"=>n(u.hp),
        "supp"=>n(u.supp),
        "path"=>json!(u.path.iter().copied().map(point).collect::<Vec<_>>()),
        "orders"=>if c.orders.is_empty(){json!([])}else{raw(&c.orders)},
        "attackId"=>n(f64::from(c.attack_id)),
        "targetId"=>n(f64::from(f.target_id)),
        "cooldown"=>n(f.cooldown),
        "still"=>option(u.still,n),
        "retarget"=>n(f.retarget),
        "repath"=>n(u.repath),
        "stuck"=>n(u.stuck),
        "cd"=>n(a.cooldown),
        "buff"=>n(a.buff),
        "ap"=>json!(a.ap),
        "nade"=>option(c.nade,point),
        "retreating"=>json!(u.retreating),
        "reinf"=>n(r.reinforcement),
        "dig"=>json!(u.dig),
        "garrison"=>option(u.garrison,|v|n(v as f64)),
        "enter"=>option(c.enter,|v|n(v as f64)),
        "board"=>n(f64::from(u.board)),
        "amove"=>option(u.amove,point),
        "face"=>raw(&c.face),
        "xp"=>n(h.xp),
        "fireAt"=>option(c.fire_at,|v|n(v as f64)),
        "sprint"=>n(u.sprint),
        "riding"=>option(h.carrier,|v|n(f64::from(v))),
        "cargo"=>numbers(h.cargo.iter().map(|v|f64::from(*v))),
        "hitBy"=>option(h.hit_by,|v|n(f64::from(v))),
        "hitAt"=>n(h.hit_at),
        "hitFrom"=>option(u.hit_from,point),
        "drift"=>json!(u.drift),
        "reverse"=>option(u.reverse,point),
        "react"=>n(c.react),
        "auto"=>json!(c.auto_cast),
        "autoRetreat"=>json!(c.auto_retreat),
        "motionBefore"=>raw(&u.motion_before),
        "motionClearance"=>raw(&u.clearance),
        "traffic"=>raw(&u.traffic),
        "trafficWait"=>n(u.traffic_wait),
        "trafficTrail"=>raw(&u.traffic_trail),
        "trafficEntered"=>json!(u.traffic_entered),
        "queuedBehind"=>n(f64::from(u.queued_behind)),
        "laneScan"=>raw(&u.lane_scan),
        "follow"=>n(f64::from(c.follow)),
        "routeEnd"=>option(c.route_end,point),
        "moveOutcome"=>raw(&c.outcome),
        "moveOutcomeTick"=>option(c.outcome_tick,|v|n(v as f64)),
        "moveResult"=>raw(&c.move_result),
        "worldGoal"=>option(u.world_goal,point),
        "pace"=>n(u.pace),
        "ghost"=>n(u.ghost),
        "build"=>json!(u.build),
        "entrench"=>json!(u.entrench),
        "holdFire"=>json!(c.hold_fire),
        "holdPos"=>json!(u.hold_pos),
        "guardHome"=>option(c.guard_home,point),
        "shotAt"=>option(u.shot_at,n),
        "lastHit"=>option(h.last_hit,|v|n(f64::from(v))),
        "deathImpulse"=>raw(&h.death_impulse),
        "hpWas"=>option(r.hp_was,n),
        "woundAt"=>option(r.wound_at,|v|n(v as f64)),
        "healAt"=>option(r.heal_at,|v|n(v as f64)),
        "atHit"=>option(h.at_hit,n),
        "dust"=>option(u.dust,n),
        "cells"=>option(u.cells.as_ref(),|v|numbers(v.iter().map(|v|f64::from(*v)))),
        "built"=>option(u.built,n),
        "logistics"=>raw(&m.logistics.get(u.id)),
        "supplyStarted"=>json!(c.supply_started),
        "supplyEscape"=>json!(c.supply_escape),
        "supplyHold"=>json!(c.supply_hold),
        "supplyRetry"=>option(c.supply_retry,|v|n(v as f64)),
        "air"=>raw(&u.air)
    };
    // Optional building records are observed only when the actual sidecar exists.
    if let Some(p) = m.production.building(u.id) {
        value["queue"] = json!(p.queue().collect::<Vec<_>>());
        value["productionJobs"] = if p.jobs().is_empty() {
            json!([])
        } else {
            raw(&p.jobs())
        };
        value["prog"] = n(p.progress());
    }
    if let Some(p) = m.building_rallies.get(&u.id) {
        value["rally"] = option(*p, point);
    }
    if let Some(s) = m
        .world
        .structures
        .iter()
        .find(|s| s.entity_id == Some(u.id))
    {
        value["structureId"] = json!(s.id);
    }
    value
}
fn observe(m: &Match) -> Value {
    let army_key = match m.options.army {
        ArmySize::Standard => "standard",
        ArmySize::Large => "large",
        ArmySize::Massive => "massive",
        ArmySize::Endless => "endless",
    };
    let army = catalog(
        crate::catalog::Catalog::compiled()
            .unwrap()
            .table("CFG")
            .unwrap()
            .get("armies")
            .unwrap()
            .get(army_key)
            .unwrap(),
    );
    let w = &m.world;
    let g = &w.game;
    let initial = w.initial();
    let players=m.start.players.iter().enumerate().map(|(slot,p)| {
        let e=&m.start.economy.players[slot];
        let research=m.production.player(slot).expect("actual research provider");
        let tech=research.tech();
        obj!{"slot"=>n(slot as f64),"name"=>json!(p.config.name),"team"=>n(f64::from(p.config.team)),
            "faction"=>n(f64::from(p.config.faction as u8)),"vp"=>n(e.vp),"mp"=>n(e.mp),"mun"=>n(e.mun),"fuel"=>n(e.fuel),"inc"=>n(e.inc),
            "sup"=>Value::Object(p.support.iter().map(|(k,v)|(k.clone(),n(*v))).collect()),"spawn"=>point(p.spawn),
            "visible"=>set(g.players[slot].visible.iter().map(|id|n(f64::from(*id)))),
            "tier"=>n(f64::from(tech.tier())),"armory"=>obj!{"inf"=>n(f64::from(tech.level(Technology::Infantry))),
                "guns"=>n(f64::from(tech.level(Technology::Guns))),"armor"=>n(f64::from(tech.level(Technology::Armor)))},
            "lab"=>if research.jobs().is_empty(){json!([])}else{raw(&research.jobs())}}
    }).collect::<Vec<_>>();
    let points=m.start.points.iter().map(|p|obj!{"scenarioId"=>p.id.clone(),"x"=>n(p.at.x),"z"=>n(p.at.z),
        "vp"=>n(p.vp),"mp"=>n(p.mp),"owner"=>option(p.owner,|v|n(v as f64)),"capper"=>option(p.capper,|v|n(v as f64)),
        "progress"=>n(p.progress),"cut"=>json!(p.cut),"kind"=>option(p.kind.as_ref(),|v|json!(v)),"needs"=>option(p.needs,|v|n(v as f64))}).collect::<Vec<_>>();
    let structures = map(w.structures.iter().map(|s| {
        let sections = s
            .section_ids
            .iter()
            .zip(&s.cells)
            .map(|(id, c)| section(m, id, *c, s.entity_id))
            .collect::<Vec<_>>();
        let mut v = obj! {"id"=>json!(s.id),"kind"=>json!(s.kind),"sections"=>json!(sections)};
        if let Some(id) = s.entity_id {
            v["entityId"] = n(f64::from(id));
        }
        (json!(s.id), v)
    }));
    obj! {
        "w"=>n(g.w as f64),"h"=>n(g.h as f64),"flags"=>typed("Uint16Array",g.flags.iter().map(|v|f64::from(*v))),
        "tick"=>n(g.tick as f64),"nextId"=>n(f64::from(m.next_unit_id)),"winVp"=>n(m.start.win_vp),
        "units"=>map(g.units.iter().map(|(id,u)|(n(f64::from(*id)),unit(m,u)))),"players"=>json!(players),"points"=>json!(points),
        "shots"=>array_or_raw(&m.firing.events),"flights"=>if m.firing.flights.is_empty(){map([])}else{raw(&m.firing.flights)},
        "nextFlight"=>n(m.firing.next_flight as f64),"nades"=>array_or_raw(&m.firing.grenades),"salvos"=>array_or_raw(&m.firing.salvos),
        "smokes"=>array_or_raw(&g.smokes),"strikes"=>array_or_raw(&g.strikes),"wrecks"=>array_or_raw(&g.wrecks),
        "winner"=>m.finish.as_ref().map_or(Value::Null,|v|n(f64::from(v.winner))),
        "cellLog"=>json!(w.cell_log().iter().map(cell_row).collect::<Vec<_>>()),
        "newCells"=>json!(w.new_cells().iter().map(cell_row).collect::<Vec<_>>()),
        "claims"=>map(m.firing.target_claims().map(|((target,team),amount)|(n(f64::from(target)*8.0+f64::from(team)),n(amount)))),
        "visionTick"=>option(g.vision_tick,|v|n(v as f64)),"story"=>raw(&m.story),"fallen"=>raw(&m.fallen),
        "timeline"=>array_or_raw(&m.timeline),"navalRegionVersion"=>n(w.versions.naval as f64),
        "nextProductionJob"=>n(m.production.next_job_id() as f64),"covers"=>array_or_raw(&m.support.covers),
        "convoys"=>raw(&m.convoys),"scenario"=>raw(&m.scenario),"world"=>raw(&m.world_runtime.as_ref().map(|_|"present")),
        "trenchFront"=>option(g.trench_front.as_ref(),|v|f32s(v)),
        "tough"=>option(g.tough.as_ref(),|v|typed("RustVecF64",v.iter().copied())),
        "shoal"=>option(w.shoal.as_ref(),|v|typed("Uint8Array",v.iter().map(|v|f64::from(*v)))),
        "logisticsEnabled"=>json!(m.options.logistics),"supply"=>json!(m.options.supply),"sandbox"=>json!(m.options.sandbox),
        "tech"=>json!(m.options.tech),"army"=>army,
        "chars"=>chars(&g.chars),"ground"=>option(g.ground.as_ref(),|v|chars(v)),"objects"=>option(g.objects.as_ref(),|v|chars(v)),
        "mineLayer"=>typed("Uint8Array",w.mine_layer.iter().map(|v|f64::from(*v))),
        "materials"=>map(w.materials.iter().map(|(k,v)|(n(*k as f64),json!(v)))),
        "baseMaterials"=>map(w.base_materials.iter().map(|(k,v)|(n(*k as f64),json!(v)))),
        "objectMaterials"=>map(w.object_materials.iter().map(|(k,v)|(n(*k as f64),json!(v)))),
        "mineMaterials"=>map(w.mine_materials.iter().map(|(k,v)|(n(*k as f64),json!(v)))),
        "roads"=>json!(w.roads),"naval"=>json!(g.naval),
        "mines"=>map(w.mines.iter().map(|(k,v)|(n(*k as f64),n(f64::from(*v))))),
        "mineSeen"=>map(w.mine_seen.iter().map(|(k,v)|(n(*k as f64),n(f64::from(*v))))),
        "aid"=>map(w.aid.iter().map(|(k,v)|(n(*k as f64),n(*v as f64)))),
        "house"=>numbers(g.house.iter().map(|v|f64::from(*v))),
        "cellHp"=>f32s(&g.cell_hp),
        "surfaceWear"=>f32s(&w.surface_wear),
        "wear"=>option(g.wear.as_ref(),|v|f32s(v)),
        "burnt"=>typed("Uint8Array",w.burnt.iter().map(|v|f64::from(*v))),
        "cellState"=>typed("Uint8Array",w.cell_state.iter().map(|v|f64::from(*v))),
        "fires"=>map(w.fires.iter().map(|(k,v)|(n(*k as f64),n(*v)))),
        "scar"=>f32s(&w.scar),"soak"=>set(w.soak().iter().map(|v|n(*v as f64))),
        "weather"=>obj!{"now"=>json!(m.start.weather.now.name()),"next"=>m.start.weather.next.map_or(Value::Null,|v|json!(v.name())),"at"=>n(f64::from(m.start.weather.at))},
        "seed"=>n(f64::from(m.start.environment_seed)),"wind"=>obj!{"a"=>n(m.start.wind.0),"v"=>n(m.start.wind.1)},
        "wx"=>m.start.living.map_or(Value::Null,|v|obj!{"rain"=>n(g.rain),"raining"=>json!(v.raining),"wet"=>option(g.wet,n),"next"=>n(v.next)}),
        "height"=>g.height.as_ref().map_or(Value::Null,|v|typed("Int8Array",v.iter().map(|v|f64::from(*v)))),
        "initialTerrain"=>obj!{"chars"=>chars(&initial.chars),"ground"=>chars(&initial.ground),"objects"=>chars(&initial.objects),
            "mineLayer"=>numbers(initial.mines.iter().map(|v|f64::from(*v))),
            "height"=>initial.height.as_ref().map_or(Value::Null,|v|numbers(v.iter().map(|v|f64::from(*v)))),
            "trenchFront"=>initial.trench_front.as_ref().map_or(Value::Null,|v|numbers(v.iter().map(|v|f64::from(*v))))},
        "structures"=>structures,
        "structuralCells"=>map(w.structures.iter().flat_map(|s|s.section_ids.iter().zip(&s.cells).map(move |(id,c)|(n(*c as f64),section(m,id,*c,s.entity_id))))),
        "fallingSections"=>array_or_raw(m.environment.falling_sections()),"worldContacts"=>set(m.environment.contacts().iter().map(|v|json!(v))),
        "buildingCells"=>map(m.building_cells.iter().map(|(c,b)|(n(*c as f64),obj!{"id"=>n(f64::from(b.unit)),"owner"=>n(b.owner as f64),"x"=>n(b.at.x),"z"=>n(b.at.z)}))),
        "obstructionVersion"=>n(w.versions.obstruction as f64),"infantryRegionVersion"=>n(w.versions.infantry as f64),"vehicleRegionVersion"=>n(w.versions.vehicle as f64),
        "projectSeq"=>n(m.field_works.project_sequence() as f64),"projects"=>raw(&m.field_works.projects().collect::<Vec<_>>()),
        "pits"=>set(m.field_works.pits().iter().map(|v|n(*v as f64))),
        "terrainVersion"=>n(f64::from(g.terrain_version)),"cellJournal"=>numbers(w.journal().iter().map(|v|*v as f64)),
        "journalBase"=>n(w.journal_base() as f64)
    }
}

fn differences(source: &Value, native: &Value, path: &str, out: &mut Vec<Value>) {
    // Delegate numeric-wrapper equality to the existing exact-bits comparator.
    if super::original::compare_projection_for_test(source, native).is_none() {
        return;
    }
    match (source, native) {
        (Value::Object(a), Value::Object(b))
            if !a.contains_key("bits") && !b.contains_key("bits") =>
        {
            let keys = a
                .keys()
                .chain(b.keys())
                .collect::<std::collections::BTreeSet<_>>();
            for key in keys {
                let missing = json!({"$missingKey":true});
                differences(
                    a.get(key).unwrap_or(&missing),
                    b.get(key).unwrap_or(&missing),
                    &format!("{path}.{key}"),
                    out,
                );
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            for i in 0..a.len().max(b.len()) {
                let missing = json!({"$missingIndex":true});
                differences(
                    a.get(i).unwrap_or(&missing),
                    b.get(i).unwrap_or(&missing),
                    &format!("{path}[{i}]"),
                    out,
                );
            }
        }
        _ => out.push(json!({"path":path,"source":source,"native":native})),
    }
}
fn currency_only(value: &Value) -> (Value, Vec<String>) {
    let mut result = value.clone();
    let mut mapped = Vec::new();
    if let Some(players) = result
        .get_mut("players")
        .and_then(Value::as_array_mut)
        .filter(|v| v.len() == 2)
    {
        for (slot, player) in players.iter_mut().enumerate() {
            if player["slot"]["bits"] != n(slot as f64)["bits"]
                || player["team"]["bits"] != n(slot as f64)["bits"]
            {
                continue;
            }
            for field in ["fuel", "mun"] {
                let object = player.as_object_mut().expect("observed player object");
                if object
                    .get(field)
                    .is_none_or(|v| *v == json!({"absent":true}))
                {
                    object.insert(field.into(), n(0.0));
                    mapped.push(format!("$.players[{slot}].{field}"));
                }
            }
        }
    }
    (result, mapped)
}
fn inventory_missing(source: &Value, fields: &Value, scope: &str, out: &mut Vec<String>) {
    for key in source.as_object().expect("observed plain object").keys() {
        if fields.get(key).is_none() {
            out.push(format!("{scope}.{key}"));
        }
    }
}
fn coverage(source: &Value, matrix: &Value) -> Vec<String> {
    let mut unknown = Vec::new();
    inventory_missing(source, &matrix["game"], "$", &mut unknown);
    for field in ["initialTerrain", "wind", "wx", "weather", "army"] {
        if let Some(value) = source.get(field).filter(|v| v.is_object()) {
            inventory_missing(value, &matrix[field], &format!("$.{field}"), &mut unknown);
        }
    }
    for (i, p) in source["players"].as_array().unwrap().iter().enumerate() {
        inventory_missing(
            p,
            &matrix["player"],
            &format!("$.players[{i}]"),
            &mut unknown,
        );
        for field in ["armory", "spawn"] {
            if let Some(value) = p.get(field) {
                inventory_missing(
                    value,
                    &matrix[field],
                    &format!("$.players[{i}].{field}"),
                    &mut unknown,
                );
            }
        }
        // Support is a dynamic keyed record. All keys/values participate in differences().
    }
    for (i, p) in source["points"].as_array().unwrap().iter().enumerate() {
        inventory_missing(p, &matrix["point"], &format!("$.points[{i}]"), &mut unknown);
    }
    for row in source["units"]["$map"].as_array().unwrap() {
        inventory_missing(
            &row[1],
            &matrix["unit"],
            &format!("$.units[{}]", row[0]["value"]),
            &mut unknown,
        );
    }
    for row in source["structures"]["$map"].as_array().unwrap() {
        inventory_missing(
            &row[1],
            &matrix["structure"],
            "$.structures[*]",
            &mut unknown,
        );
        for section in row[1]["sections"].as_array().unwrap() {
            inventory_missing(
                section,
                &matrix["section"],
                "$.structures[*].sections[*]",
                &mut unknown,
            );
        }
    }
    for field in ["cellLog", "newCells"] {
        for row in source[field].as_array().unwrap() {
            inventory_missing(
                &row[4],
                &matrix["cellData"],
                &format!("$.{field}[*][4]"),
                &mut unknown,
            );
            if let Some(section) = row[4].get("section") {
                inventory_missing(
                    section,
                    &matrix["cellSection"],
                    &format!("$.{field}[*][4].section"),
                    &mut unknown,
                );
            }
        }
    }
    if let Some(footprints) = source
        .get("buildingCells")
        .and_then(|v| v["$map"].as_array())
    {
        for row in footprints {
            inventory_missing(
                &row[1],
                &matrix["footprint"],
                "$.buildingCells[*]",
                &mut unknown,
            );
        }
    }
    unknown
}

fn inactive_game(m: &Match, key: &str) -> bool {
    let g = &m.world.game;
    let no_logistics = !m.options.logistics
        && m.convoys.is_none()
        && m.logistics.iter().next().is_none()
        && m.supply_net.teams.is_empty()
        && m.supply_net.width == 0
        && m.supply_net.height == 0;
    let no_visibility = g.vision_tick.is_none()
        && g.players.iter().all(|p| p.visible.is_empty())
        && m.routing_fog.terrain.is_none()
        && m.routing_fog.kept.is_empty()
        && m.routing_fog.teams.is_empty()
        && m.recipients
            .iter()
            .all(|r| r.remembered().is_empty() && r.remembered_wrecks().is_empty());
    match key {
        "claims" => m.firing.target_claims().next().is_none(),
        "fog" | "fogTerrain" | "fogSources" | "ghosts" => no_visibility,
        "pathStats" => {
            m.commands.pending_commands() == 0
                && m.commands.path_requests().pending_len() == 0
                && m.commands.path_requests().queue_len() == 0
                && m.commands.path_requests().used() == 0
                && g.units
                    .values()
                    .all(|u| u.path.is_empty() && m.commands.units[&u.id].orders.is_empty())
        }
        "worldFootprintRadius" | "world" => {
            m.world_runtime.is_none() && m.options.mode == Mode::Conquest
        }
        "operatives" => m.companions.records.is_empty() && m.companions.soldiers.is_empty(),
        "supplyGraph" | "supplyNet" | "convoys" => no_logistics,
        "scenario" => m.scenario.is_none(),
        "story" => m
            .story
            .iter()
            .all(|s| *s == crate::match_story::PlayerStory::default()),
        "fallen" => {
            m.fallen.structure.is_none() && m.fallen.bunker.is_none() && m.fallen.hq.is_none()
        }
        "timeline" => m.timeline.is_empty(),
        "projects" => {
            m.field_works.projects().next().is_none()
                && m.field_works.retained_projects().next().is_none()
        }
        "covers" => m.support.covers.is_empty(),
        "pits" => m.field_works.pits().is_empty(),
        _ => false,
    }
}
fn inactive_unit(m: &Match, id: u32, key: &str) -> bool {
    let u = &m.world.game.units[&id];
    let c = &m.commands.units[&id];
    let h = m.combat.get(id).unwrap();
    match key {
        "face" => c.face.is_none(),
        "motionBefore" => u.motion_before.is_none(),
        "motionClearance" => u.clearance.is_none(),
        "traffic" => u.traffic.is_none(),
        "trafficTrail" => u.traffic_trail.is_empty(),
        "laneScan" => u.lane_scan.is_none(),
        "moveOutcome" => c.outcome.is_none(),
        "moveResult" => c.move_result.is_none(),
        "deathImpulse" => h.death_impulse.is_none(),
        "garrisonEntry" => u.garrison.is_none() && c.enter.is_none(),
        "attackPick" | "aaShot" | "aaReserve" => {
            !m.rules.kinds[u.kind].air
                && u.air.is_none()
                && m.firing.flights.is_empty()
                && m.firing.events.is_empty()
                && m.firing.suppressed.is_empty()
                && m.firing.target_claims().next().is_none()
        }
        "region" | "gateT" | "open" => {
            m.world_runtime.is_none()
                && matches!(
                    m.rules.kinds[u.kind].name.as_str(),
                    "rifle" | "mg" | "hq" | "barracks"
                )
        }
        "convoy" | "supplyRouteReason" | "logistics" => {
            !m.options.logistics && m.convoys.is_none() && m.logistics.get(id).is_none()
        }
        "air" => !m.rules.kinds[u.kind].air && u.air.is_none(),
        "queue" | "productionJobs" | "prog" => m.production.building(id).is_none(),
        "structureId" => !m.world.structures.iter().any(|s| s.entity_id == Some(id)),
        _ => false,
    }
}
fn undefined(value: &Value) -> bool {
    *value == json!({"$undefined":true}) || *value == json!({"$missingKey":true})
}
fn absent_native(value: &Value) -> bool {
    *value == none() || *value == json!({"$missingKey":true})
}
fn exact(a: &Value, b: &Value) -> bool {
    super::original::compare_projection_for_test(a, b).is_none()
}
fn default_value(kind: &str) -> Value {
    if kind == "default-zero" {
        n(0.0)
    } else {
        json!(false)
    }
}
struct Semantic<'a> {
    m: &'a Match,
    matrix: &'a Value,
    checked: Vec<Value>,
    differences: Vec<Value>,
    mappings: Vec<Value>,
    representations: Vec<Value>,
}
impl Semantic<'_> {
    fn mismatch(&mut self, path: &str, a: &Value, b: &Value, rule: &Value) {
        let mut leaves = Vec::new();
        differences(a, b, path, &mut leaves);
        if leaves.is_empty() {
            leaves.push(json!({"path":path,"source":a,"native":b,"predicateFailed":true}));
        }
        for mut leaf in leaves {
            leaf["disposition"] = rule.clone();
            self.differences.push(leaf);
        }
    }
    fn object(&mut self, scope: &str, a: &Value, b: &Value, path: &str, id: Option<u32>) {
        let (Some(a), Some(b)) = (a.as_object(), b.as_object()) else {
            self.mismatch(path, a, b, &json!({"disposition":"expected object"}));
            return;
        };
        let keys = a
            .keys()
            .chain(b.keys())
            .collect::<std::collections::BTreeSet<_>>();
        for key in keys {
            let missing = json!({"$missingKey":true});
            let av = a.get(key).unwrap_or(&missing);
            let bv = b.get(key).unwrap_or(&missing);
            let rule = self.matrix[scope].get(key).cloned().unwrap_or(Value::Null);
            self.field(scope, key, (av, bv), &format!("{path}.{key}"), id, &rule);
        }
    }
    fn array(&mut self, scope: &str, a: &Value, b: &Value, path: &str) {
        let (Some(a), Some(b)) = (a.as_array(), b.as_array()) else {
            self.mismatch(path, a, b, &json!({"disposition":"expected array"}));
            return;
        };
        if a.len() != b.len() {
            self.mismatch(
                path,
                &json!(a.len()),
                &json!(b.len()),
                &json!({"disposition":"array length"}),
            );
        }
        for (i, (a, b)) in a.iter().zip(b).enumerate() {
            self.object(scope, a, b, &format!("{path}[{i}]"), None);
        }
    }
    fn entries(&mut self, scope: &str, a: &Value, b: &Value, path: &str) {
        let (Some(a), Some(b)) = (a["$map"].as_array(), b["$map"].as_array()) else {
            self.mismatch(path, a, b, &json!({"disposition":"expected ordered map"}));
            return;
        };
        if a.len() != b.len() {
            self.mismatch(
                path,
                &json!(a.len()),
                &json!(b.len()),
                &json!({"disposition":"map length"}),
            );
        }
        for (i, (a, b)) in a.iter().zip(b).enumerate() {
            if !exact(&a[0], &b[0]) {
                self.mismatch(
                    &format!("{path}.$map[{i}].key"),
                    &a[0],
                    &b[0],
                    &json!({"disposition":"ordered identity"}),
                );
            }
            let id = if scope == "unit" {
                a[0]["value"].as_u64().and_then(|v| u32::try_from(v).ok())
            } else {
                None
            };
            if scope == "unit" && id.is_none_or(|v| !self.m.world.game.units.contains_key(&v)) {
                self.mismatch(
                    path,
                    &a[0],
                    &b[0],
                    &json!({"disposition":"actual unit identity"}),
                );
                continue;
            }
            self.object(scope, &a[1], &b[1], &format!("{path}.$map[{i}].value"), id);
        }
    }
    fn field(
        &mut self,
        scope: &str,
        key: &str,
        values: (&Value, &Value),
        path: &str,
        id: Option<u32>,
        rule: &Value,
    ) {
        let (a, b) = values;
        let Some(kind) = rule["disposition"].as_str() else {
            self.mismatch(path, a, b, &json!({"disposition":"UNKNOWN_OR_UNDISPOSED"}));
            return;
        };
        self.checked.push(json!({"path":path,"scope":scope,"field":key,"disposition":kind,"sourceAnchor":rule["source"],"expression":rule["consumingExpression"]}));
        // Recurse through every record, retaining exact ordered identity and key-set checks.
        match (scope, key) {
            ("game", "units") => {
                self.entries("unit", a, b, path);
                return;
            }
            ("game", "players") => {
                self.array("player", a, b, path);
                return;
            }
            ("game", "points") => {
                self.array("point", a, b, path);
                return;
            }
            ("game", "structures") => {
                self.entries("structure", a, b, path);
                return;
            }
            ("structure", "sections") => {
                self.array("section", a, b, path);
                return;
            }
            ("game", "wind" | "weather" | "army" | "initialTerrain") => {
                self.object(key, a, b, path, None);
                return;
            }
            ("game", "wx") if a.is_object() && a.get("$undefined").is_none() && b.is_object() => {
                self.object("wx", a, b, path, None);
                return;
            }
            ("player", "spawn" | "armory") => {
                self.object(key, a, b, path, None);
                return;
            }
            _ => {}
        }
        let same = exact(a, b);
        let accepted = match kind {
            "exact" => same,
            "building-cells-membership" => {
                scope == "game" && key == "buildingCells" && building_cells_equal(a, b)
            }
            "empty" => {
                same && (a.as_array().is_some_and(Vec::is_empty)
                    || a["$map"].as_array().is_some_and(Vec::is_empty))
            }
            "default-zero" | "default-false" => {
                if undefined(a) {
                    let default = default_value(kind);
                    if exact(b, &default) {
                        self.mappings.push(json!({"path":path,"side":"source","expression":rule["consumingExpression"],"sourceAnchor":rule["source"],"default":default,"count":1,"approval":"PENDING_AXEL","mode":"DIAGNOSTIC_PROPOSED_DEFAULTS"}));
                        true
                    } else {
                        false
                    }
                } else {
                    same
                }
            }
            "none" | "none-null" | "none-minus-one" | "none-zero" => {
                let sentinel = match kind {
                    "none" => undefined(a),
                    "none-null" => a.is_null() || (key == "rally" && undefined(a)),
                    "none-minus-one" => exact(a, &n(-1.0)),
                    "none-zero" => exact(a, &n(0.0)),
                    _ => false,
                };
                same || (sentinel
                    && absent_native(b)
                    && (key != "riding" || !self.m.world.game.units[&id.unwrap()].riding))
            }
            "inactive" => {
                let source_ok = undefined(a) || (scope == "unit" && key == "face" && a.is_null());
                source_ok
                    && if scope == "game" {
                        inactive_game(self.m, key)
                    } else {
                        id.is_some_and(|v| inactive_unit(self.m, v, key))
                    }
            }
            "optional-building" => {
                same || (undefined(a)
                    && absent_native(b)
                    && id.is_some_and(|v| inactive_unit(self.m, v, key)))
            }
            "no-dig" => {
                same || (a.is_null()
                    && *b == json!(false)
                    && id.is_some_and(|v| {
                        !self.m.world.game.units[&v].dig
                            && self.m.field_works.projects().next().is_none()
                    }))
            }
            "no-pace" => {
                same || (undefined(a)
                    && id.is_some_and(|v| {
                        let u = &self.m.world.game.units[&v];
                        u.pace.is_nan() && u.path.is_empty() && u.move_speed.to_bits() == 0
                    }))
            }
            "seed-bits" => a["value"]
                .as_f64()
                .zip(b["value"].as_f64())
                .is_some_and(|(s, n)| {
                    s.is_finite()
                        && n.is_finite()
                        && s.fract() == 0.0
                        && n.fract() == 0.0
                        && (-2147483648.0..=4294967295.0).contains(&s)
                        && (0.0..=4294967295.0).contains(&n)
                        && ((s as i64) as u32) == n as u32
                }),
            "internal-journal" => {
                undefined(a)
                    && self.m.world_runtime.is_none()
                    && self.m.world.journal_base() == 0
                    && self
                        .m
                        .world
                        .journal()
                        .iter()
                        .all(|c| *c < self.m.world.game.w * self.m.world.game.h)
            }
            "derived-index" => {
                let log = &self.m.world.cell_log();
                let unique = log
                    .iter()
                    .map(|r| r.cell)
                    .collect::<std::collections::BTreeSet<_>>()
                    .len()
                    == log.len();
                let expected = map(log
                    .iter()
                    .enumerate()
                    .map(|(i, r)| (n(r.cell as f64), n(i as f64))));
                unique && exact(a, &expected)
            }
            _ => false,
        };
        if accepted && !same {
            self.representations.push(json!({"path":path,"source":a,"native":b,"disposition":kind,"sourceAnchor":rule["source"],"meaning":rule["note"]}));
        }
        if !accepted {
            self.mismatch(path, a, b, rule);
        }
    }
}

#[test]
fn building_cells_membership_preserves_complete_entries() {
    let record = |id, owner, x, z| obj! {"id"=>n(id),"owner"=>n(owner),"x"=>n(x),"z"=>n(z)};
    let source = map([
        (n(8.0), record(7.0, 0.0, 10.0, 20.0)),
        (n(3.0), record(8.0, 1.0, 30.0, 40.0)),
    ]);
    let mut reordered = source.clone();
    reordered["$map"].as_array_mut().unwrap().reverse();
    assert!(building_cells_equal(&source, &reordered));
    assert!(!exact(&source, &reordered)); // Raw order still differs.
    let mut negative_zero = source.clone();
    negative_zero["$map"][0][1]["x"] = json!({"bits":"8000000000000000","value":0});
    assert!(building_cells_equal(&negative_zero, &negative_zero));
    let mut native_negative_zero = negative_zero.clone();
    native_negative_zero["$map"][0][1]["x"] = n(-0.0);
    assert!(building_cells_equal(&negative_zero, &native_negative_zero));
    let mut positive_zero = negative_zero.clone();
    positive_zero["$map"][0][1]["x"] = n(0.0);
    assert!(!building_cells_equal(&positive_zero, &negative_zero));
    assert!(!building_cells_equal(&negative_zero, &positive_zero));
    for source_side in [false, true] {
        let rejected = |changed: Value| {
            if source_side {
                assert!(!building_cells_equal(&changed, &source));
            } else {
                assert!(!building_cells_equal(&source, &changed));
            }
        };
        let mut missing = source.clone();
        missing["$map"].as_array_mut().unwrap().pop();
        rejected(missing);
        let mut extra = source.clone();
        extra["$map"]
            .as_array_mut()
            .unwrap()
            .push(json!([n(9.0), record(9.0, 0.0, 10.0, 20.0)]));
        rejected(extra);
        let mut duplicate = source.clone();
        duplicate["$map"][1] = duplicate["$map"][0].clone();
        rejected(duplicate);
        let mut wrong_key = source.clone();
        wrong_key["$map"][0][0] = n(9.0);
        rejected(wrong_key);
        for field in ["id", "owner", "x", "z"] {
            let mut wrong_value = source.clone();
            wrong_value["$map"][0][1][field] = n(99.0);
            rejected(wrong_value);
            let mut missing_field = source.clone();
            missing_field["$map"][0][1]
                .as_object_mut()
                .unwrap()
                .remove(field);
            rejected(missing_field);
        }
        let mut signed_zero = source.clone();
        signed_zero["$map"][0][1]["owner"] = n(-0.0);
        rejected(signed_zero);
        let mut unknown_field = source.clone();
        unknown_field["$map"][0][1]["unregisteredField"] = n(0.0);
        rejected(unknown_field);
        let mut unknown_wrapper = source.clone();
        unknown_wrapper["unregisteredField"] = json!(true);
        rejected(unknown_wrapper);
        let mut wrong_wrapper = source.clone();
        let rows = wrong_wrapper
            .as_object_mut()
            .unwrap()
            .remove("$map")
            .unwrap();
        wrong_wrapper["$set"] = rows;
        rejected(wrong_wrapper);
        let mut wrong_type = source.clone();
        wrong_type["$map"][0][1]["owner"] = json!(false);
        rejected(wrong_type);
        let mut wrong_association = source.clone();
        let first = wrong_association["$map"][0][1].clone();
        wrong_association["$map"][0][1] = wrong_association["$map"][1][1].clone();
        wrong_association["$map"][1][1] = first;
        rejected(wrong_association);
        let mut inconsistent_bits = source.clone();
        inconsistent_bits["$map"][0][1]["x"]["bits"] = json!("4000000000000000");
        rejected(inconsistent_bits);
        let mut malformed_bits = source.clone();
        malformed_bits["$map"][0][1]["x"]["bits"] = json!("not sixteen hex");
        rejected(malformed_bits);
        let mut nonfinite = source.clone();
        nonfinite["$map"][0][1]["x"] = n(f64::NAN);
        rejected(nonfinite);
    }
    assert!(!building_cells_equal(&source, &source["$map"]));
}

#[test]
fn semantic_registry_preserves_present_values_and_unknown_fields() {
    assert!(undefined(&json!({"$undefined":true})));
    assert!(!undefined(&Value::Null));
    assert!(!undefined(&json!(false)));
    assert!(!undefined(&n(-0.0)));
    assert!(!exact(&n(-0.0), &default_value("default-zero")));
    assert!(!exact(&n(1.0), &default_value("default-zero")));
    assert!(!exact(&Value::Null, &default_value("default-false")));
    let matrix: Value = serde_json::from_str(include_str!(
        "../../../../docs/testing/r1-mode21-startup-fields.json"
    ))
    .unwrap();
    assert_eq!(
        matrix
            .as_object()
            .unwrap()
            .values()
            .map(|v| v.as_object().unwrap().len())
            .sum::<usize>(),
        275
    );
    for fields in matrix.as_object().unwrap().values() {
        for entry in fields.as_object().unwrap().values() {
            assert!(matches!(
                entry["disposition"].as_str().unwrap(),
                "exact"
                    | "building-cells-membership"
                    | "empty"
                    | "inactive"
                    | "none"
                    | "none-null"
                    | "none-minus-one"
                    | "none-zero"
                    | "default-zero"
                    | "default-false"
                    | "optional-building"
                    | "no-dig"
                    | "no-pace"
                    | "seed-bits"
                    | "internal-journal"
                    | "derived-index"
            ));
        }
    }
    assert!(matrix["unit"].get("unregisteredField").is_none());
}

#[test]
#[ignore = "requires additive authentic startup capture and explicit resource grant"]
fn complete_r1_create_game_state() {
    let bytes = std::fs::read(
        std::env::var_os("WW2_MODE21_STARTUP").expect("private startup artifact required"),
    )
    .unwrap();
    let fixture: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(fixture["format"], "r1-mode21-startup-v1");
    assert_eq!(fixture["baselinePreserved"], true);
    assert_eq!(
        fixture["observation"]["format"],
        "r1-complete-startup-observation-v1"
    );
    let trace = &fixture["trace"];
    assert_eq!(
        trace["reference"],
        "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
    );
    assert_eq!(trace["alignedFormat"], "r1-aligned-inputs-v1");
    assert_eq!(trace["format"], "r1-whole-match-differential-v1");
    assert_eq!(trace["requestedTicks"], 600);
    assert_eq!(trace["checkpointEvery"], 10);
    assert_eq!(trace["completed"], 600);
    assert_eq!(trace["failure"], Value::Null);
    assert_eq!(trace["frames"].as_array().unwrap().len(), 601);
    assert_eq!(
        trace["profile"],
        json!({"mode":"conquest","army":"standard","weather":"clear","shuffle":true,"tech":true,"supply":true,"livingWeather":true,"logistics":false,"sandbox":false,"weatherSeed":null})
    );
    let committed: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../server/src/authoritative-maps/default.json"
    )))
    .unwrap();
    assert_eq!(
        trace["map"], committed,
        "constructor authoring must be independent of observed state"
    );
    assert!(
        committed.get("scenario").is_none() && committed.get("world").is_none(),
        "R1 inactive branches must remain absent"
    );
    assert_eq!(trace["sharedInput"]["seed"], trace["seed"]);
    let seed = trace["seed"].as_str().unwrap().parse::<u64>().unwrap();
    let authored =
        crate::map::AuthoritativeMap::from_json(&serde_json::to_vec(&committed).unwrap()).unwrap();
    let replay = Replay::new(&trace["sharedInput"]).unwrap();
    let m = Match::new_aligned(
        &authored,
        MatchOptions {
            seed,
            mode: Mode::Conquest,
            players: vec![
                PlayerConfig {
                    name: "A".into(),
                    team: 0,
                    faction: Faction::Usa,
                },
                PlayerConfig {
                    name: "B".into(),
                    team: 1,
                    faction: Faction::Germany,
                },
            ],
            shuffle: true,
            army: ArmySize::Standard,
            weather: WeatherChoice::from_reference(Some("clear")),
            weather_seed: None,
            living_weather: true,
            map_key: Some("default".into()),
            tech: true,
            supply: true,
            logistics: false,
            sandbox: false,
        },
        replay.clone(),
    )
    .unwrap();
    let before = replay.receipt();
    let native = observe(&m);
    assert_eq!(before, replay.receipt(), "observer consumed a draw");
    assert_eq!(native, observe(&m), "observer is not stable");
    let source = &fixture["observation"]["state"];
    let matrix: Value = serde_json::from_str(include_str!(
        "../../../../docs/testing/r1-mode21-startup-fields.json"
    ))
    .unwrap();
    let unknown = coverage(source, &matrix);
    let mut raw_differences = Vec::new();
    differences(source, &native, "$", &mut raw_differences);
    let (canonical_source, source_mapped) = currency_only(source);
    let (canonical_native, native_mapped) = currency_only(&native);
    let mut canonical_differences = Vec::new();
    differences(
        &canonical_source,
        &canonical_native,
        "$",
        &mut canonical_differences,
    );
    let source_receipt = &fixture["startupReceipt"];
    assert_eq!(source_receipt["consumed"], 5);
    assert_eq!(
        source_receipt["draws"],
        json!(trace["inputReceipt"]["draws"].as_array().unwrap()[..5])
    );
    let aligned = source_receipt["consumed"] == before["consumed"]
        && source_receipt["unconsumed"] == before["unconsumed"]
        && source_receipt["draws"] == before["draws"]
        && source_receipt["startup"] == before["startup"]
        && source_receipt["layout"]["input"] == before["layout"]["input"]
        && source_receipt["capacity"] == before["capacity"]
        && source_receipt["exhausted"] == before["exhausted"];
    let mut semantic = Semantic {
        m: &m,
        matrix: &matrix,
        checked: Vec::new(),
        differences: Vec::new(),
        mappings: Vec::new(),
        representations: Vec::new(),
    };
    semantic.object("game", &canonical_source, &canonical_native, "$", None);
    let mut unknown = unknown;
    unknown.extend(coverage(&native, &matrix));
    let complete = unknown.is_empty();
    let matches = aligned && complete && semantic.differences.is_empty();
    // Raw observations and differences are never replaced by the semantic dispositions.
    let report = json!({"format":"r1-mode21-startup-report-v2","mode":"DIAGNOSTIC_PROPOSED_DEFAULTS","ownerApproval":"PENDING_AXEL_6196930542",
        "alignedInputs":aligned,"nativeInputReceipt":before,"sourceInputReceipt":source_receipt,
        "sourceObservation":fixture["observation"],"nativeObservation":native,"fieldProviderMatrix":matrix,
        "unknownObservedFields":unknown,"semanticDispositions":semantic.checked,"semanticDifferences":semantic.differences,
        "verifiedRepresentationDispositions":semantic.representations,"proposedDefaultMappings":semantic.mappings,
        "rawDifferences":raw_differences,"canonicalDifferences":canonical_differences,
        "currencyMappings":{"source":source_mapped,"native":native_mapped},
        "canonicalFirstDifference":canonical_differences.first(),"rawFirstDifference":raw_differences.first(),
        "firstSemanticDifference":semantic.differences.first(),"fullSemanticCheckMatches":matches,
        "mode21Eligible":false,"rowStatusChanged":false,
        "sourceHead":fixture["sourceHead"],"baselineSourceHead":fixture["baselineSourceHead"],
        "nativeHead":std::env::var("WW2_MODE21_NATIVE_HEAD").expect("frozen producing head required"),
        "nativeTicksAdvanced":0,"constructors":1,"startupDraws":5,"startupUnused":16379,
        "rawSidecars":{"firing":raw(&m.firing),"commands":raw(&m.commands.units),"world":raw(&m.world),"production":raw(&m.production),
            "combat":raw(&m.combat),"abilities":raw(&m.abilities),"recovery":raw(&m.recovery),"grid":raw(&m.grid),"economy":raw(&m.start.economy)},
        "limits":"All proposed defaults remain diagnostic pending Axel. Semantic sentinels/inactive/internal storage are separately justified. No raw graph equality, shipping getter, extra mode or runtime change is required."});
    std::fs::write(
        std::env::var_os("WW2_MODE21_REPORT").expect("private report required"),
        serde_json::to_vec(&report).unwrap(),
    )
    .unwrap();
    println!(
        "{}",
        json!({"alignedInputs":aligned,"nativeTicksAdvanced":0,"startupDraws":5,"startupUnused":16379,
        "rawDifferenceCount":raw_differences.len(),"currencyOnlyDifferenceCount":canonical_differences.len(),
        "semanticDifferenceCount":semantic.differences.len(),"firstSemanticDifferencePath":semantic.differences.first().map(|v| &v["path"]),
        "rawFirstDifferencePath":raw_differences.first().map(|v| &v["path"]),"proposedMappingCount":semantic.mappings.len(),
        "unknownObservedFields":unknown,"fullSemanticCheckMatches":matches,"mode21Eligible":false})
    );
    assert!(aligned, "startup draw alignment failure retained");
    assert!(complete, "unknown or undisposed observed fields retained");
    assert!(
        semantic.differences.is_empty(),
        "complete semantic startup differences retained"
    );
}
