//! Read-only original World images. Decoder output never restores a native owner.
use serde_json::{Value, json};
use std::{collections::BTreeMap, io::Read};
use ww2_core::{
    economy::{Faction, Mode, Technology},
    economy_state::ArmySize,
    game::{LOCAL_DEFENDER_OWNER, Point},
    map::AuthoritativeMap,
    match_end::{EndReason, MatchSample},
    match_runtime::{Match, MatchOptions, PlayerConfig},
    match_story::StoryKey,
    random::MatchRandom,
    weather::WeatherChoice,
};

/// JSON null cannot retain NaN or either infinity. Every Number is an IEEE image.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Exact {
    Null,
    Bool(bool),
    Number(u64),
    String(String),
    Array(Vec<Exact>),
    Object(BTreeMap<String, Exact>),
}
impl Exact {
    pub fn json(v: &Value) -> Self {
        match v {
            Value::Null => Self::Null,
            Value::Bool(v) => Self::Bool(*v),
            Value::Number(v) => Self::Number(v.as_f64().unwrap().to_bits()),
            Value::String(v) => Self::String(v.clone()),
            Value::Array(v) => Self::Array(v.iter().map(Self::json).collect()),
            Value::Object(v) => {
                Self::Object(v.iter().map(|(k, v)| (k.clone(), Self::json(v))).collect())
            }
        }
    }
    pub fn get(&self, key: &str) -> &Self {
        match self {
            Self::Object(v) => v
                .get(key)
                .unwrap_or_else(|| panic!("missing original field {key}")),
            Self::Array(v) => &v[key.parse::<usize>().unwrap()],
            _ => panic!("non-container at {key}"),
        }
    }
    pub fn optional(&self, key: &str) -> Option<&Self> {
        match self {
            Self::Object(v) => v.get(key),
            _ => None,
        }
    }
    pub fn array(&self) -> &[Self] {
        match self {
            Self::Array(v) => v,
            _ => panic!("expected array"),
        }
    }
    pub fn object(&self) -> &BTreeMap<String, Self> {
        match self {
            Self::Object(v) => v,
            _ => panic!("expected object"),
        }
    }
    pub fn number(&self) -> f64 {
        match self {
            Self::Number(v) => f64::from_bits(*v),
            _ => panic!("expected Number"),
        }
    }
    pub fn id(&self) -> u32 {
        let n = self.number();
        assert!(n >= 0. && n.fract() == 0. && n <= u32::MAX as f64);
        n as u32
    }
    fn at_mut(&mut self, path: &[String]) -> &mut Self {
        let Some((first, rest)) = path.split_first() else {
            return self;
        };
        match self {
            Self::Array(v) => v[first.parse::<usize>().unwrap()].at_mut(rest),
            Self::Object(v) => v.get_mut(first).unwrap().at_mut(rest),
            _ => panic!("invalid lossless patch parent"),
        }
    }
}
pub fn unpack(v: &Value) -> Exact {
    let mut image = Exact::json(&v["raw"]);
    for (path, bits) in v["numberBits"].as_object().unwrap() {
        let path = if path.is_empty() {
            Vec::new()
        } else {
            path.split('.').map(str::to_owned).collect()
        };
        *image.at_mut(&path) =
            Exact::Number(u64::from_str_radix(bits.as_str().unwrap(), 16).unwrap());
    }
    image
}
pub fn advance_image(image: &mut Exact, after: &Value) {
    assert_eq!(
        image.get("tick").number(),
        after["baseTick"].as_f64().unwrap()
    );
    for patch in after["patches"].as_array().unwrap() {
        let path: Vec<_> = patch["path"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect();
        if path.is_empty() {
            assert_eq!(patch["op"], "set");
            *image = unpack(&patch["value"]);
            continue;
        }
        let (key, parent) = path.split_last().unwrap();
        match image.at_mut(parent) {
            Exact::Object(v) => {
                if patch["op"] == "delete" {
                    assert!(v.remove(key).is_some());
                } else {
                    v.insert(key.clone(), unpack(&patch["value"]));
                }
            }
            Exact::Array(v) => {
                assert_eq!(patch["op"], "set", "Source deletes no array slots");
                v[key.parse::<usize>().unwrap()] = unpack(&patch["value"]);
            }
            _ => panic!("invalid lossless patch"),
        }
    }
    assert_eq!(image.get("tick").number(), after["tick"].as_f64().unwrap());
}
pub fn fixture() -> Value {
    let mut text = String::new();
    flate2::read::GzDecoder::new(include_bytes!("../fixtures/world-tick.json.gz").as_slice())
        .read_to_string(&mut text)
        .unwrap();
    serde_json::from_str(&text).unwrap()
}
pub fn options(case: &Value) -> MatchOptions {
    // Native seed is independently declared here. Source fixed Math.random is not equivalent.
    MatchOptions {
        seed: 9182,
        mode: Mode::World,
        players: case["setup"]["teams"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
            .map(|(slot, t)| PlayerConfig {
                name: ["A", "B"][slot].into(),
                team: t.as_i64().unwrap() as i32,
                faction: Faction::try_from(case["setup"]["factions"][slot].as_u64().unwrap() as u8)
                    .unwrap(),
            })
            .collect(),
        shuffle: false,
        army: ArmySize::Standard,
        weather: WeatherChoice::Map,
        weather_seed: None,
        living_weather: false,
        map_key: None,
        tech: true,
        supply: case["setup"]["options"]["supply"].as_bool().unwrap(),
        logistics: case["setup"]["options"]["logistics"].as_bool().unwrap(),
        sandbox: false,
    }
}
pub fn construct(f: &Value, case: &Value) -> Result<Match, String> {
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&f["map"]).unwrap())
        .map_err(|e| e.to_string())?;
    let o = options(case);
    let mut m = Match::new_world(&map, o.clone(), MatchRandom::new(o.seed))?;
    let controls = &case["setup"]["controls"]["allUnits"];
    for u in m.world.game.units.values_mut() {
        u.hold_pos = controls["holdPos"].as_bool().unwrap();
        let c = m.commands.units.get_mut(&u.id).unwrap();
        c.hold_fire = controls["holdFire"].as_bool().unwrap();
        c.auto_cast = controls["auto"].as_bool().unwrap();
    }
    for input in case["setup"]["authored"].as_array().unwrap() {
        if let Some(slot) = input["commander"].as_u64() {
            let p = m
                .start
                .economy
                .players
                .get_mut(slot as usize)
                .ok_or("authored commander absent")?;
            for (key, v) in input.as_object().unwrap() {
                match key.as_str() {
                    "commander" => {}
                    "mp" => p.mp = v.as_f64().unwrap(),
                    "mun" => p.mun = v.as_f64().unwrap(),
                    _ => return Err(format!("unknown authored wallet field {key}")),
                }
            }
        } else if let Some(id) = input["unit"].as_u64() {
            let id = id as u32;
            for (key, v) in input.as_object().unwrap() {
                let u = m
                    .world
                    .game
                    .units
                    .get_mut(&id)
                    .ok_or("authored World unit absent")?;
                match key.as_str() {
                    "unit" => {}
                    "x" => u.x = v.as_f64().unwrap(),
                    "z" => u.z = v.as_f64().unwrap(),
                    "hp" => u.hp = v.as_f64().unwrap(),
                    "lastHit" => {
                        m.combat.get_mut(id).unwrap().last_hit = Some(v.as_i64().unwrap() as i32)
                    }
                    "path" => {
                        u.path = v.as_array().unwrap().iter().map(point_input).collect();
                        u.path_gen += 1;
                    }
                    "cooldown" => m.firing.get_mut(id).unwrap().cooldown = v.as_f64().unwrap(),
                    "retarget" => m.firing.get_mut(id).unwrap().retarget = v.as_f64().unwrap(),
                    "repath" => u.repath = v.as_f64().unwrap(),
                    "sprint" => u.sprint = v.as_f64().unwrap(),
                    "cd" => m.abilities.get_mut(id).unwrap().cooldown = v.as_f64().unwrap(),
                    "buff" => m.abilities.get_mut(id).unwrap().buff = v.as_f64().unwrap(),
                    "react" => m.commands.units.get_mut(&id).unwrap().react = v.as_f64().unwrap(),
                    _ => return Err(format!("unrepresented authored World unit input {key}")),
                }
            }
        } else if let Some(id) = input["region"].as_i64() {
            let index = m
                .world
                .geography()
                .unwrap()
                .regions
                .iter()
                .position(|r| r.id == id as i32)
                .ok_or("authored World region absent")?;
            let r = &mut m.world_runtime.as_mut().unwrap().regions[index];
            for (key, v) in input.as_object().unwrap() {
                match key.as_str() {
                    "region" => {}
                    "team" => r.team = v.as_i64().unwrap() as i32,
                    "progress" => r.progress = v.as_f64().unwrap(),
                    "capper" => r.capper = Some(v.as_u64().unwrap() as usize),
                    _ => return Err(format!("unrepresented authored World region input {key}")),
                }
            }
        } else {
            return Err("unknown authored World setup input".into());
        }
    }
    m.start.environment_seed = case["setup"]["environmentSeed"].as_u64().unwrap() as u32;
    if let Some(wind) = case["setup"].get("wind") {
        m.start.wind = (wind["a"].as_f64().unwrap(), wind["s"].as_f64().unwrap());
        assert!(m.start.wind.0.is_finite() && m.start.wind.1.is_finite());
    }
    if !case["setup"]["finishInput"].is_null() {
        let input = &case["setup"]["finishInput"];
        assert_eq!(input["reason"], "nations");
        m.initialize_tick_scene_finish(
            input["winner"].as_i64().unwrap() as i32,
            EndReason::Nations,
            Some(point_input(&input["at"])),
        )?;
    }
    if let Some(commands) = case["setup"].get("commands") {
        for (sequence, input) in commands.as_array().unwrap().iter().enumerate() {
            use ww2_core::{match_commands::Command, orders::OrderKey};
            let raw = &input["command"];
            let command = match raw["t"].as_str().unwrap() {
                "tech" => {
                    assert_eq!(raw["kind"], "tier");
                    Command::Technology {
                        kind: Technology::Tier,
                    }
                }
                "buy" => Command::Buy {
                    kind: raw["unit"].as_str().unwrap().into(),
                    from: raw["from"].as_u64().map(|v| v as u32),
                },
                _ => return Err("unrepresented declared World boundary command".into()),
            };
            m.enqueue_command(
                OrderKey {
                    tick: input["tick"].as_u64().unwrap(),
                    issuer: input["slot"].as_u64().unwrap() as u32,
                    sequence: sequence as u64,
                },
                command,
            )
            .map_err(|e| format!("declared World boundary enqueue: {e:?}"))?;
        }
    }
    Ok(m)
}
fn point_input(v: &Value) -> Point {
    Point {
        x: v["x"].as_f64().unwrap(),
        z: v["z"].as_f64().unwrap(),
    }
}
fn at(p: Point) -> Value {
    json!({"x":p.x,"z":p.z})
}
fn owner(o: usize) -> i64 {
    if o == LOCAL_DEFENDER_OWNER {
        -1
    } else {
        o as i64
    }
}
pub fn same(expected: &Exact, actual: Value, path: &str) {
    assert_eq!(*expected, Exact::json(&actual), "{path}");
}

/// These gaps are printed and asserted explicitly. No unrecognized field is dropped.
/// Wind starts from unrelated constructor RNG unless a scene explicitly authors it.
pub const GAPS: &[(&str, &str)] = &[(
    "wind",
    "Source fixed constructor random and native Weather stream have different provenance",
)];

pub fn compare_units(m: &Match, expected: &Exact, context: &str) {
    let rows = expected.array();
    assert_eq!(
        m.world.game.units.len(),
        rows.len(),
        "{context}: live membership"
    );
    for (u, row) in m.world.game.units.values().zip(rows) {
        let c = &m.commands.units[&u.id];
        let f = m.firing.get(u.id).unwrap();
        let a = m.abilities.get(u.id).unwrap();
        let h = m.combat.get(u.id).unwrap();
        let r = m.recovery.get(u.id).unwrap();
        assert_eq!(row.get("id").id(), u.id, "{context}: ordered live identity");
        for (key, e) in row.object() {
            let value = match key.as_str() {
                "id" => json!(u.id),
                "type" => json!(m.rules.kinds[u.kind].name),
                "owner" => json!(owner(u.owner)),
                "x" => json!(u.x),
                "z" => json!(u.z),
                "rot" => json!(u.rot),
                "aim" => json!(c.aim),
                "hp" => json!(u.hp),
                "supp" => json!(u.supp),
                "moveSpeed" => json!(u.move_speed),
                "vx" => json!(u.vx),
                "vz" => json!(u.vz),
                "travelDir" => json!(u.travel_dir),
                "path" => json!(u.path.iter().copied().map(at).collect::<Vec<_>>()),
                "reverse" => json!(u.reverse.map(at)),
                "orders" => {
                    assert!(c.orders.is_empty());
                    json!([])
                }
                "attackId" => json!(c.attack_id),
                "targetId" => json!(f.target_id),
                "cooldown" => json!(f.cooldown),
                "retarget" => json!(f.retarget),
                "repath" => json!(u.repath),
                "still" => json!(u.still),
                "stuck" => json!(u.stuck),
                "cd" => json!(a.cooldown),
                "buff" => json!(a.buff),
                "ap" => json!(a.ap),
                "sprint" => json!(u.sprint),
                "react" => json!(c.react),
                "retreating" => json!(u.retreating),
                "reinf" => json!(r.reinforcement),
                "garrison" => json!(u.garrison.map_or(-1, |v| v as i64)),
                "riding" => json!(h.carrier.unwrap_or(0)),
                "cargo" => json!(h.cargo),
                "board" => json!(u.board),
                "amove" => json!(u.amove.map(at)),
                "hitBy" => json!(h.hit_by.unwrap_or(0)),
                "hitAt" => json!(h.hit_at),
                "hitFrom" => json!(u.hit_from.map(at)),
                "lastHit" => json!(h.last_hit),
                "xp" => json!(h.xp),
                "holdPos" => json!(u.hold_pos),
                "holdFire" => json!(c.hold_fire),
                "auto" => json!(c.auto_cast),
                "autoRetreat" => json!(c.auto_retreat),
                "dig" => {
                    assert!(!u.dig);
                    Value::Null
                }
                "nade" => {
                    assert!(c.nade.is_none());
                    Value::Null
                }
                "enter" => json!(c.enter.map_or(-1, |v| v as i64)),
                "fireAt" => json!(c.fire_at.map_or(-1, |v| v as i64)),
                "face" => {
                    assert!(c.face.is_none());
                    Value::Null
                }
                "drift" => json!(u.drift),
                "cells" => json!(u.cells),
                "built" => json!(u.built),
                "queue" => json!(m.production.building(u.id).unwrap().queue().collect::<Vec<_>>()),
                "productionJobs" => json!(m.production.building(u.id).unwrap().rows().iter().map(|r|json!({"id":r.job.id,"type":r.job.kind,"status":match r.status{ww2_core::production::JobStatus::Active=>"active",ww2_core::production::JobStatus::Waiting=>"waiting"},"mp":r.job.charge.mp,"fuel":r.job.charge.fuel})).collect::<Vec<_>>()),
                "prog" => json!(m.production.building(u.id).unwrap().progress()),
                "rally" => json!(m.building_rallies[&u.id].map(at)),
                "structureId" => json!(
                    m.world
                        .structures
                        .iter()
                        .find(|s| s.entity_id == Some(u.id))
                        .map(|s| s.id.clone())
                ),
                "guardHome" => json!(c.guard_home.map(at)),
                "region" => json!(
                    m.world.geography().unwrap().regions
                        [m.world_runtime.as_ref().unwrap().guard_bases[&u.id]]
                        .id
                ),
                "motionBefore" => json!(
                    u.motion_before
                        .map(|p| json!({"x":p.x,"z":p.z,"rot":p.rot}))
                ),
                "traffic" => {
                    assert!(u.traffic.is_none());
                    Value::Null
                }
                "trafficWait" => json!(u.traffic_wait),
                "queuedBehind" => json!(u.queued_behind),
                "hpWas" => json!(r.hp_was),
                "woundAt" => json!(r.wound_at),
                "healAt" => json!(r.heal_at),
                "shotAt" => json!(u.shot_at),
                _ => panic!("{context}: unclassified unit {} Source field {key}", u.id),
            };
            same(e, value, &format!("{context}: unit {}.{key}", u.id));
        }
    }
}
fn compare_commanders(m: &Match, expected: &Exact, context: &str) {
    assert_eq!(expected.array().len(), m.start.players.len());
    for (slot, row) in expected.array().iter().enumerate() {
        let p = &m.start.players[slot];
        let w = &m.start.economy.players[slot];
        let g = &m.world.game.players[slot];
        let research = m.production.player(slot).unwrap();
        for (key, e) in row.object() {
            let v = match key.as_str() {
                "slot" => json!(g.slot),
                "name" => json!(p.config.name),
                "team" => json!(g.team),
                "faction" => json!(p.config.faction as u8),
                "vp" => json!(w.vp),
                "mp" => json!(w.mp),
                "inc" => json!(w.inc),
                "upkeep" => json!(w.upkeep),
                "mun" => json!(w.mun),
                "fuel" => json!(w.fuel),
                "fuelInc" => json!(w.fuel_inc),
                "away" => json!(w.away),
                "out" => json!(w.out),
                "spawn" => at(p.spawn),
                "visible" => json!(g.visible),
                "sup" => json!(p.support),
                "tier" => json!(research.tech().tier()),
                "armory" => {
                    json!({"inf":research.tech().level(Technology::Infantry),"guns":research.tech().level(Technology::Guns),"armor":research.tech().level(Technology::Armor)})
                }
                "lab" => json!(research.jobs().iter().map(|j|json!({"kind":match j.kind{Technology::Tier=>"tier",Technology::Infantry=>"inf",Technology::Guns=>"guns",Technology::Armor=>"armor"},"prog":j.progress,"time":j.time})).collect::<Vec<_>>()),
                _ => panic!("{context}: unclassified commander field {key}"),
            };
            same(e, v, &format!("{context}: commander {slot}.{key}"));
        }
    }
}
fn rle(values: impl IntoIterator<Item = Value>) -> Value {
    let mut rows: Vec<(Value, usize)> = Vec::new();
    for v in values {
        if let Some(last) = rows.last_mut().filter(|r| r.0 == v) {
            last.1 += 1;
        } else {
            rows.push((v, 1));
        }
    }
    json!(rows)
}
fn cell_row(row: &ww2_core::world::CellRow) -> Value {
    let d = &row.data;
    let mut data = json!({"ground":char::from(d.ground).to_string(),"object":char::from(d.object).to_string(),"material":d.material,
        "groundMaterial":d.ground_material,"groundWear":f64::from(d.ground_wear)});
    let map = data.as_object_mut().unwrap();
    if let Some(v) = d.trench_front {
        map.insert("trenchFront".into(), json!(f64::from(v)));
    }
    if d.mine {
        map.insert("mine".into(), json!(true));
    }
    if let Some(v) = &d.mine_material {
        map.insert("mineMaterial".into(), json!(v));
    }
    if let Some(v) = d.rail {
        map.insert("rail".into(), json!(v));
    }
    if let Some(v) = d.station {
        map.insert("station".into(), json!(v));
    }
    if let Some(s) = &d.section {
        map.insert("section".into(),json!({"id":s.id,"structureId":s.structure_id,"state":s.state,"hp":s.hp,"maxHp":s.max_hp,"material":s.material,"anchor":s.anchor}));
    }
    json!([
        row.cell,
        char::from(row.ch).to_string(),
        row.height,
        row.state,
        data
    ])
}
fn section(m: &Match, id: &str, cell: usize, entity: Option<u32>) -> Value {
    let (meta, hp) = m.world.section_by_id(id).unwrap();
    let mut row = json!({"id":meta.id,"structureId":meta.structure_id,"c":cell,"hp":hp.hp,"maxHp":hp.max_hp,
        "material":meta.material,"anchor":meta.anchor,"supports":meta.supports,"state":if hp.failed{"failed"}else{"intact"},"rubble":meta.rubble});
    let object = row.as_object_mut().unwrap();
    if let Some(v) = entity {
        object.insert("entityId".into(), json!(v));
    }
    if let Some(v) = meta.failure.impulse {
        object.insert("impulse".into(), json!(v));
    }
    if let Some(v) = meta.failure.failed_at {
        object.insert("failedAt".into(), json!(v));
    }
    if let Some(v) = meta.failure.direction {
        object.insert("direction".into(), at(v));
    }
    if let Some(v) = meta.failure.settled_at {
        object.insert("settledAt".into(), json!(v));
    }
    row
}
fn structures(m: &Match) -> (Value, Value) {
    let mut cells = Vec::new();
    let rows = m
        .world
        .structures
        .iter()
        .map(|s| {
            let sections = s
                .section_ids
                .iter()
                .map(|id| {
                    let cell = m
                        .world
                        .sections
                        .iter()
                        .find_map(|(cell, meta)| (meta.id == *id).then_some(*cell))
                        .or_else(|| m.world.detached_sections.get(id).map(|v| v.cell))
                        .unwrap();
                    let row = section(m, id, cell, s.entity_id);
                    cells.push(json!([cell, row.clone()]));
                    row
                })
                .collect::<Vec<_>>();
            let mut row = json!({"id":s.id,"kind":s.kind,"sections":sections});
            if let Some(id) = s.entity_id {
                row["entityId"] = json!(id);
            }
            json!([s.id, row])
        })
        .collect::<Vec<_>>();
    (json!(rows), json!(cells))
}
fn compare_terrain(m: &Match, expected: &Exact, context: &str) {
    let g = &m.world.game;
    for (key, e) in expected.object() {
        let v = match key.as_str() {
            "chars" => rle(g.chars.iter().map(|v| json!(char::from(*v).to_string()))),
            "flags" => rle(g.flags.iter().map(|v| json!(v))),
            "height" => g
                .height
                .as_ref()
                .map_or(Value::Null, |v| rle(v.iter().map(|v| json!(v)))),
            "hp" => rle(g.cell_hp.iter().map(|v| json!(f64::from(*v)))),
            "wear" => g
                .wear
                .as_ref()
                .map_or(Value::Null, |v| rle(v.iter().map(|v| json!(f64::from(*v))))),
            "scar" => rle(m.world.scar.iter().map(|v| json!(f64::from(*v)))),
            "burnt" => rle(m.world.burnt.iter().map(|v| json!(v))),
            "state" => rle(m.world.cell_state.iter().map(|v| json!(v))),
            "versions" => json!([
                u64::from(g.terrain_version),
                m.world.versions.obstruction,
                m.world.versions.infantry,
                m.world.versions.vehicle,
                m.world.versions.naval
            ]),
            "journal" => json!(m.world.journal()),
            "journalBase" => json!(m.world.journal_base()),
            "newCells" => json!(m.world.new_cells().iter().map(cell_row).collect::<Vec<_>>()),
            "cellLog" => json!(m.world.cell_log().iter().map(cell_row).collect::<Vec<_>>()),
            _ => panic!("{context}: unclassified terrain field {key}"),
        };
        same(e, v, &format!("{context}: terrain.{key}"));
    }
}
pub fn compare_state(m: &Match, image: &Exact, shots: &[Value], context: &str, compare_wind: bool) {
    let runtime = m.world_runtime.as_ref().unwrap();
    if image.optional("nextProductionJob").is_none() {
        assert_eq!(
            m.production.next_job_id(),
            0,
            "{context}: no paid job before Source counter exists"
        );
    }
    for (key, e) in image.object() {
        let v=match key.as_str(){
        "units"=>{compare_units(m,e,context);continue;},"commanders"=>{compare_commanders(m,e,context);continue;},"terrain"=>{compare_terrain(m,e,context);continue;},
        "tick"=>json!(m.world.game.tick),"visionTick"=>json!(m.world.game.vision_tick),"nextId"=>json!(m.next_unit_id),"nextFlight"=>json!(m.firing.next_flight),
        "seed"=>json!(m.start.environment_seed as i32),"wind"=>{if !compare_wind{continue;}json!({"a":m.start.wind.0,"v":m.start.wind.1})},"wx"=>{assert!(m.start.living.is_none());Value::Null},
        "nextProductionJob"=>json!(m.production.next_job_id()),
        "mode"=>json!({"kind":"world","teams":m.start.players.iter().map(|p|p.config.team).collect::<std::collections::BTreeSet<_>>().len(),"total":runtime.regions.len()}),
        "army"=>json!({"pop":1,"income":1}),"pointSupply"=>json!(m.options.supply && m.options.mode!=Mode::World),"logisticsEnabled"=>json!(m.options.logistics),
        "localDefender"=>json!({"slot":-1,"name":"Local defenders","team":-1,"faction":runtime.local_defender.faction as u8,"spawn":at(runtime.local_defender.spawn),"visible":runtime.local_defender.visible,"mp":0,"mun":0,"fuel":0}),
        "regions"=>{
            assert_eq!(e.array().len(),runtime.regions.len());
            for ((raw,r),def) in e.array().iter().zip(&runtime.regions).zip(&m.world.geography().unwrap().regions){for (key,value) in raw.object(){let actual=match key.as_str(){
                "id"=>json!(def.id),"x"=>json!((def.x as f64+0.5)*2.),"y"=>json!(def.y),"z"=>json!((def.y as f64+0.5)*2.),
                "kind"=>json!(def.kind),"bounds"=>json!(def.bounds.map(|v|v*2)),"home"=>json!(def.home),"team"=>json!(r.team),"progress"=>json!(r.progress),
                "capper"=>json!(r.capper.map_or(-1,|v|v as i64)),"capTick"=>json!(r.cap_tick),"contested"=>json!(r.contested),"locked"=>json!(r.locked),_=>panic!("unclassified region field {key}"),
            };same(value,actual,&format!("{context}: region {}.{key}",def.id));}}continue;
        },
        "nodes"=>json!(runtime.nodes.iter().map(|n|json!({"c":n.cell,"x":n.at.x,"z":n.at.z,"depot":n.depot.unwrap_or(0),"rate":n.rate,"fuel":n.fuel})).collect::<Vec<_>>()),
        "points"=>{assert!(m.start.points.is_empty());json!([])},
        "story"=>if m.story.iter().all(|s|StoryKey::ALL.into_iter().all(|k|s.value(k)==0.)){Value::Null}else{json!(m.story.iter().map(|s|StoryKey::ALL.into_iter().map(|k|(k.as_str().to_owned(),json!(s.value(k)))).collect::<serde_json::Map<_,_>>()).collect::<Vec<_>>())},
        "timeline"=>if m.timeline.is_empty(){Value::Null}else{json!(m.timeline.iter().map(|s|match s{MatchSample::Territories{t,points,structures}=>json!({"t":t,"points":points,"structures":structures}),MatchSample::Conquest{..}=>panic!("unexpected World Conquest sample")}).collect::<Vec<_>>())},
        "fallen"=>if m.fallen.structure.is_none()&&m.fallen.bunker.is_none()&&m.fallen.hq.is_none(){Value::Null}else{
            let mut v=json!({});
            if let Some(p)=m.fallen.structure{v["structure"]=at(p);}
            if let Some(p)=m.fallen.bunker{v["bunker"]=at(p);}
            if let Some(p)=m.fallen.hq{v["hq"]=at(p);}
            v
        },
        "finish"=>json!({"winner":m.finish.map(|f|f.winner),"reason":m.finish.map(|f|f.reason.as_str()),"at":m.finish.map(|f|at(f.end_at)),"tick":m.finish.map(|f|f.end_tick),"reveal":m.world.game.reveal}),
        "shots"=>json!(shots),"flights"=>{assert!(m.firing.flights.is_empty());json!([])},"nades"=>{assert!(m.firing.grenades.is_empty());json!([])},"salvos"=>{assert!(m.firing.salvos.is_empty());json!([])},
        "strikes"=>{assert!(m.world.game.strikes.is_empty());json!([])},"covers"=>{assert!(m.support.covers.is_empty());json!([])},"convoys"=>{assert!(m.convoys.is_none());Value::Null},
        "fires"=>json!(m.world.fires.iter().map(|(c,t)|json!([c,t])).collect::<Vec<_>>()),"soak"=>json!(m.world.soak()),
        "smokes"=>{assert!(m.world.game.smokes.is_empty());json!([])},"wrecks"=>{assert!(m.world.game.wrecks.is_empty(),"World building removals do not fabricate vehicle wrecks");json!([])},
        "structures"=>structures(m).0,"structuralCells"=>structures(m).1,
        _=>panic!("{context}: unclassified canonical Source field {key}"),
    };
        same(e, v, &format!("{context}: {key}"));
    }
}
