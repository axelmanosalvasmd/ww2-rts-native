//! Original World ground component replay. Constructor parity and full tick scheduling are separate.
//! Source startup rows restore the declared component boundary after real World construction.
use super::*;
use crate::{
    catalog::Catalog,
    combat::{Impact, WeaponDamage},
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::{LOCAL_DEFENDER_OWNER, Point, Section},
    grid::SpatialGrid,
    map::AuthoritativeMap,
    match_abilities::AbilityState,
    match_combat::{CombatState, ExplosionSource, FlightSource, RetirementProgress},
    match_commands::{Command, MatchCommands, UnitControl},
    match_recovery::RecoveryState,
    match_runtime::{MatchOptions, PlayerConfig},
    match_story::{DeathRecord, PlayerStory},
    random::{MatchRandom, StreamId},
    unit_publication::{PublicationParts, StagedUnit, prepare_world_publication},
    unit_spawn::initial_unit,
    weather::WeatherChoice,
    world::{Appearance, FailureMetadata, SectionMetadata, WorldStructure},
};
use serde_json::{Value as Json, json};
use std::collections::BTreeMap;

fn f(v: &Json) -> f64 {
    f64::from_bits(u64::from_str_radix(v.as_str().unwrap(), 16).unwrap())
}
fn b(v: f64) -> Json {
    json!(format!("{:016x}", v.to_bits()))
}
fn optional(v: Option<f64>) -> Json {
    v.map(b).unwrap_or(Json::Null)
}
fn point(v: &Json) -> Point {
    Point {
        x: f(&v["x"]),
        z: f(&v["z"]),
    }
}
fn at(v: Point) -> Json {
    json!({"x":b(v.x),"z":b(v.z)})
}
fn owner(v: &Json) -> usize {
    if v.as_i64() == Some(-1) {
        LOCAL_DEFENDER_OWNER
    } else {
        v.as_u64().unwrap() as usize
    }
}
fn source_owner(v: usize) -> i32 {
    if v == LOCAL_DEFENDER_OWNER {
        -1
    } else {
        v as i32
    }
}
fn kind(m: &Match, name: &str) -> usize {
    m.rules.kinds.iter().position(|k| k.name == name).unwrap()
}
fn equal(a: &Json, b: &Json) -> bool {
    match (a, b) {
        (Json::Number(a), Json::Number(b)) => {
            a.as_f64().unwrap().to_bits() == b.as_f64().unwrap().to_bits()
        }
        (Json::Array(a), Json::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| equal(a, b))
        }
        (Json::Object(a), Json::Object(b)) => {
            a.len() == b.len() && a.iter().all(|(k, a)| b.get(k).is_some_and(|b| equal(a, b)))
        }
        _ => a == b,
    }
}
fn difference(a: &Json, b: &Json, path: &str) -> String {
    match (a, b) {
        (Json::Object(a), Json::Object(b)) => {
            for k in a.keys().chain(b.keys()) {
                let av = a.get(k).unwrap_or(&Json::Null);
                let bv = b.get(k).unwrap_or(&Json::Null);
                if !equal(av, bv) {
                    return difference(av, bv, &format!("{path}.{k}"));
                }
            }
        }
        (Json::Array(a), Json::Array(b)) => {
            if a.len() != b.len() {
                return format!("{path}:length {} != {}", a.len(), b.len());
            }
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                if !equal(a, b) {
                    return difference(a, b, &format!("{path}[{i}]"));
                }
            }
        }
        _ => {}
    }
    format!("{path}:{a} != {b}")
}
fn fixture() -> Json {
    serde_json::from_str(include_str!("../tests/fixtures/world-ground-actors.json")).unwrap()
}

fn restore_actor(m: &mut Match, row: &Json) {
    let id = row["id"].as_u64().unwrap() as u32;
    let mut u = initial_unit(
        &m.rules,
        Catalog::compiled().unwrap(),
        kind(m, row["type"].as_str().unwrap()),
        id,
        owner(&row["owner"]),
        point(row),
    )
    .unwrap();
    u.rot = f(&row["rot"]);
    u.hp = f(&row["hp"]);
    u.supp = f(&row["supp"]);
    u.sprint = f(&row["sprint"]);
    u.repath = f(&row["repath"]);
    u.still = (!row["still"].is_null()).then(|| f(&row["still"]));
    u.built = (!row["built"].is_null()).then(|| f(&row["built"]));
    u.cells = row["cells"]
        .as_array()
        .map(|a| a.iter().map(|v| v.as_u64().unwrap() as u32).collect());
    u.garrison = row["garrison"]
        .as_i64()
        .filter(|v| *v >= 0)
        .map(|v| v as usize);
    u.riding = row["riding"].as_u64().unwrap() != 0;
    u.hold_pos = row["holdPos"].as_bool().unwrap();
    u.retreating = row["retreating"].as_bool().unwrap();
    u.hit_from = (!row["hitFrom"].is_null()).then(|| point(&row["hitFrom"]));
    u.path = row["path"].as_array().unwrap().iter().map(point).collect();
    u.shot_at = row["shotAt"].as_f64();
    let mut c = UnitControl::new_for_mode(&u, &m.rules, Mode::World).unwrap();
    c.aim = f(&row["aim"]);
    c.react = f(&row["react"]);
    c.follow = row["follow"].as_u64().unwrap() as u32;
    c.attack_id = row["attackId"].as_u64().unwrap() as u32;
    c.hold_fire = row["holdFire"].as_bool().unwrap();
    c.auto_cast = row["auto"].as_bool().unwrap();
    assert!(row["orders"].as_array().unwrap().is_empty());
    assert!(row["nade"].is_null());
    assert!(row["anchor"].is_null());
    m.commands.units.insert(id, c);
    m.world.game.units.insert(id, u);
    let combat = m.combat.initialize(id);
    combat.xp = f(&row["xp"]);
    combat.last_hit = row["lastHit"].as_i64().map(|v| v as i32);
    combat.hit_by = row["hitBy"].as_u64().filter(|id| *id > 0).map(|v| v as u32);
    combat.hit_at = f(&row["hitAt"]);
    let firing = m.firing.initialize(id);
    firing.cooldown = f(&row["cooldown"]);
    firing.retarget = f(&row["retarget"]);
    firing.target_id = row["targetId"].as_u64().unwrap() as u32;
    let a = m.abilities.initialize(id);
    a.cooldown = f(&row["cd"]);
    a.buff = f(&row["buff"]);
    a.ap = row["ap"].as_bool().unwrap();
    let r = m.recovery.initialize(id);
    r.reinforcement = f(&row["reinf"]);
    r.hp_was = (!row["hpWas"].is_null()).then(|| f(&row["hpWas"]));
    r.wound_at = row["woundAt"].as_i64();
    r.heal_at = row["healAt"].as_i64();
}
fn run_values(v: &Json) -> Vec<Json> {
    v.as_array()
        .unwrap()
        .iter()
        .flat_map(|row| std::iter::repeat_n(row[0].clone(), row[1].as_u64().unwrap() as usize))
        .collect()
}
fn f32_values(v: &Json) -> Vec<f32> {
    run_values(v)
        .iter()
        .map(|v| f32::from_bits(v.as_u64().unwrap() as u32))
        .collect()
}
fn make(input: &Json) -> Match {
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&input["map"]).unwrap()).unwrap();
    let options = MatchOptions {
        seed: 701,
        mode: Mode::World,
        players: (0..2)
            .map(|slot| PlayerConfig {
                name: input["constructorInputs"]["names"][slot]
                    .as_str()
                    .unwrap()
                    .into(),
                team: input["constructorInputs"]["teams"][slot].as_i64().unwrap() as i32,
                faction: Faction::try_from(
                    input["constructorInputs"]["factions"][slot]
                        .as_u64()
                        .unwrap() as u8,
                )
                .unwrap(),
            })
            .collect(),
        shuffle: false,
        army: ArmySize::Standard,
        weather: WeatherChoice::Map,
        weather_seed: Some(123),
        living_weather: false,
        map_key: None,
        tech: false,
        supply: false,
        logistics: false,
        sandbox: false,
    };
    let mut m = Match::new_world(&map, options, MatchRandom::new(701)).unwrap();
    let start = &input["startup"];
    let baseline = &start["initial"];
    m.world.game.units.clear();
    m.commands = MatchCommands::new(4096);
    m.combat = CombatState::default();
    m.firing = FiringState::default();
    m.abilities = AbilityState::default();
    m.recovery = RecoveryState::default();
    m.recruit_metadata.clear();
    // Boundary restoration uses only original constructor inputs, never later expected states.
    let chars: Vec<u8> = start["charRows"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|v| v.as_str().unwrap().bytes())
        .collect();
    m.world.game.chars.copy_from_slice(&chars);
    let ground: Vec<u8> = start["ground"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().as_bytes()[0])
        .collect();
    m.world
        .game
        .ground
        .as_mut()
        .unwrap()
        .copy_from_slice(&ground);
    let objects: Vec<u8> = start["objects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().as_bytes()[0])
        .collect();
    m.world
        .game
        .objects
        .as_mut()
        .unwrap()
        .copy_from_slice(&objects);
    let height: Vec<i8> = start["height"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_i64().unwrap() as i8)
        .collect();
    m.world
        .game
        .height
        .as_mut()
        .unwrap()
        .copy_from_slice(&height);
    // Physical arrays are recorded from the original post-startup boundary.
    // Preserve existing Game allocation bindings where their arrays already exist.
    let flags: Vec<u16> = run_values(&start["flagRuns"])
        .iter()
        .map(|v| v.as_u64().unwrap() as u16)
        .collect();
    m.world.game.flags.copy_from_slice(&flags);
    m.world
        .game
        .cell_hp
        .copy_from_slice(&f32_values(&start["cellHpBits"]));
    m.world.game.house.copy_from_slice(
        &run_values(&start["houseRuns"])
            .iter()
            .map(|v| v.as_u64().unwrap() as u8)
            .collect::<Vec<_>>(),
    );
    m.world
        .game
        .wear
        .as_mut()
        .unwrap()
        .copy_from_slice(&f32_values(&start["wearBits"]));
    m.world
        .surface_wear
        .copy_from_slice(&f32_values(&start["surfaceWearBits"]));
    m.world
        .scar
        .copy_from_slice(&f32_values(&start["scarBits"]));
    m.world.burnt.copy_from_slice(
        &run_values(&start["burntRuns"])
            .iter()
            .map(|v| v.as_u64().unwrap() as u8)
            .collect::<Vec<_>>(),
    );
    m.world.cell_state.copy_from_slice(
        &run_values(&start["cellStateRuns"])
            .iter()
            .map(|v| v.as_u64().unwrap() as u8)
            .collect::<Vec<_>>(),
    );
    m.world.game.trench_front =
        (!start["trenchFrontBits"].is_null()).then(|| f32_values(&start["trenchFrontBits"]));
    m.world.game.tough = (!start["toughBits"].is_null())
        .then(|| run_values(&start["toughBits"]).iter().map(f).collect());
    let versions = start["terrainVersions"].as_array().unwrap();
    m.world.game.terrain_version = versions[0].as_u64().unwrap() as u32;
    m.world.versions.obstruction = versions[1].as_u64().unwrap();
    m.world.versions.infantry = versions[2].as_u64().unwrap();
    m.world.versions.vehicle = versions[3].as_u64().unwrap();
    m.world.versions.naval = versions[4].as_u64().unwrap();
    m.world.structures.clear();
    m.world.sections.clear();
    m.world.game.sections.clear();
    for s in start["structures"].as_array().unwrap() {
        let sections = s["sections"].as_array().unwrap();
        m.world.structures.push(WorldStructure {
            id: s["id"].as_str().unwrap().into(),
            kind: match s["kind"].as_str().unwrap() {
                "house" => "house",
                "bridge" => "bridge",
                "building" => "building",
                v => panic!("unsupported structure {v}"),
            },
            cells: sections
                .iter()
                .map(|v| v["c"].as_u64().unwrap() as usize)
                .collect(),
            section_ids: sections
                .iter()
                .map(|v| v["id"].as_str().unwrap().into())
                .collect(),
            entity_id: s["entityId"].as_u64().map(|v| v as u32),
        });
        for row in sections {
            let c = row["c"].as_u64().unwrap() as usize;
            let hp = row["hp"].as_f64().unwrap();
            m.world.game.sections.insert(
                c as u32,
                Section {
                    failed: false,
                    hp,
                    max_hp: row["maxHp"].as_f64(),
                },
            );
            m.world.game.cell_hp[c] = hp as f32;
            m.world.sections.insert(
                c,
                SectionMetadata {
                    id: row["id"].as_str().unwrap().into(),
                    structure_id: row["structureId"].as_str().unwrap().into(),
                    material: row["material"].as_str().unwrap().into(),
                    anchor: row["anchor"].as_bool().unwrap(),
                    supports: row["supports"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|v| v.as_str().unwrap().into())
                        .collect(),
                    rubble: Vec::new(),
                    appearance: Appearance::Intact,
                    failure: FailureMetadata::default(),
                },
            );
            assert_eq!(row["state"], "intact");
            assert!(row["rubble"].as_array().unwrap().is_empty());
        }
    }
    for row in baseline["actors"].as_array().unwrap() {
        restore_actor(&mut m, row);
    }
    m.grid = SpatialGrid::new(&m.world.game.units, 16.);
    m.next_unit_id = baseline["nextId"].as_u64().unwrap() as u32;
    m.world.game.tick = 10;
    m.world.game.rain = 0.;
    m.world.game.wet = Some(0.);
    m.start.environment_seed = 123;
    m.start.wind = (0., 0.5);
    m.story = vec![PlayerStory::default(); 2];
    for p in baseline["players"].as_array().unwrap() {
        if p["slot"].as_i64().unwrap() >= 0 {
            let slot = p["slot"].as_u64().unwrap() as usize;
            m.start.economy.players[slot].mp = f(&p["mp"]);
            m.start.economy.players[slot].mun = f(&p["mun"]);
            m.start.economy.players[slot].fuel = f(&p["fuel"]);
            m.world.game.players[slot].visible = Vec::new();
        } else {
            m.world_runtime.as_mut().unwrap().local_defender.visible = Vec::new();
        }
    }
    m.building_cells.clear();
    for u in m.world.game.units.values() {
        if m.rules.kinds[u.kind].building {
            for &cell in u.cells.as_ref().expect("restored building footprint") {
                let old = m.building_cells.insert(
                    cell as usize,
                    crate::match_runtime::BuildingCell {
                        unit: u.id,
                        owner: u.owner,
                        at: u.at(),
                    },
                );
                assert!(old.is_none(), "overlapping restored building footprint");
            }
        }
    }
    // Keep new_world's real World memory binding. These in-range components
    // never enter the separate ordinary recipient routing provider.
    m
}
fn actor(m: &Match, id: u32) -> Json {
    let u = &m.world.game.units[&id];
    let c = &m.commands.units[&id];
    let combat = m.combat.get(id).unwrap();
    let f = m.firing.get(id).unwrap();
    let a = m.abilities.get(id).unwrap();
    let r = m.recovery.get(id).unwrap();
    assert!(
        c.orders.is_empty(),
        "recorder does not include queued orders"
    );
    json!({"id":id,"type":m.rules.kinds[u.kind].name,"owner":source_owner(u.owner),"x":b(u.x),"z":b(u.z),"rot":b(u.rot),"aim":b(c.aim),"hp":b(u.hp),"xp":b(combat.xp),"supp":b(u.supp),"hitAt":b(combat.hit_at),"cd":b(a.cooldown),"buff":b(a.buff),"sprint":b(u.sprint),"cooldown":b(f.cooldown),"retarget":b(f.retarget),"repath":b(u.repath),"react":b(c.react),"still":optional(u.still),"reinf":b(r.reinforcement),"built":optional(u.built),"structureId":m.world.structures.iter().find(|s|s.entity_id==Some(id)).map(|s|s.id.clone()),"cells":u.cells,"anchor":Json::Null,"targetId":f.target_id,"attackId":c.attack_id,"shotAt":u.shot_at,"hitBy":combat.hit_by.unwrap_or(0),"lastHit":combat.last_hit,"hitFrom":u.hit_from.map(at),"deathImpulse":combat.death_impulse.map(|v|json!({"dir":b(v.direction),"impulse":b(v.magnitude)})),"hpWas":r.hp_was.map(b),"woundAt":r.wound_at,"healAt":r.heal_at,"ap":a.ap,"auto":c.auto_cast,"holdFire":c.hold_fire,"holdPos":u.hold_pos,"retreating":u.retreating,"riding":if u.riding {combat.carrier.unwrap()}else{0},"garrison":u.garrison.map_or(-1,|v|v as i64),"follow":c.follow,"orders":[],"path":u.path.iter().copied().map(at).collect::<Vec<_>>(),"nade":c.nade.map(|v|json!({"x":b(v.x),"z":b(v.z),"auto":a.aimed_auto,"walked":a.aimed_walked}))})
}
fn catalog_object(v: Option<&crate::catalog::Value>) -> Json {
    v.and_then(|v| v.object())
        .map(|v| {
            Json::Object(
                v.iter()
                    .filter_map(|(k, v)| {
                        let value = if let Some(n) = v.number() {
                            b(n)
                        } else if let Some(s) = v.string() {
                            json!(s)
                        } else {
                            json!(v.boolean()?)
                        };
                        Some((k.clone(), value))
                    })
                    .collect(),
            )
        })
        .unwrap_or(Json::Null)
}
fn blast_source(s: ExplosionSource, ab: &crate::catalog::Value) -> Json {
    // Immutable ability identity/description stays in the catalog. Actual stored damage
    // fields are projected from the flight, rather than replaced with catalog defaults.
    let mut value = catalog_object(Some(ab));
    value["inf"] = b(s.infantry);
    value["veh"] = b(s.vehicle);
    value["supp"] = b(s.suppression);
    if ab.get("terrain").is_some() {
        value["terrain"] = optional(s.terrain)
    } else {
        assert!(s.terrain.is_none())
    }
    if ab.get("antiGarrison").is_some() {
        value["antiGarrison"] = optional(s.anti_garrison)
    } else {
        assert!(s.anti_garrison.is_none())
    }
    if ab.get("scar").is_some() {
        value["scar"] = b(s.scar)
    } else {
        assert_eq!(s.scar, 0.)
    }
    value
}
fn flight(m: &Match, p: &LiveFlight) -> Json {
    let def = Catalog::compiled()
        .unwrap()
        .unit(&m.rules.kinds[p.source_kind].name)
        .unwrap();
    let blast = if let Some(source) = p.blast_source {
        blast_source(source, def.get("ab").unwrap())
    } else {
        Json::Null
    };
    json!({"id":p.id,"shooter":p.shooter,"owner":source_owner(p.owner),"kind":m.rules.kinds[p.source_kind].name,"at":[b(p.solution.origin.x),b(p.solution.origin.y),b(p.solution.origin.z)],"velocity":[b(p.solution.velocity.x),b(p.solution.velocity.y),b(p.solution.velocity.z)],"launchOrigin":[b(p.launch_origin.x),b(p.launch_origin.y),b(p.launch_origin.z)],"gravity":b(p.solution.gravity),"stamp":b(p.solution.stamp),"launched":b(p.launched),"last":b(p.last),"expires":b(p.expires),"energy":b(p.energy),"sequence":p.sequence,"maxContacts":p.max_contacts,"startCell":p.start_cell,"intended":p.intended,"damageScale":b(p.damage_scale),"near":b(p.near),"suppression":b(p.suppression),"suppressionScale":b(p.suppression_scale),"ap":p.armor_piercing,"apMultiplier":b(p.ap_multiplier),"apNaval":p.ap_naval,"blastRadius":p.blast_radius.map(b),"blastSource":blast,"weapon":catalog_object(def.get("w")),"effect":p.effect,"sent":p.sent,"sourceXp":b(m.combat.get(p.shooter).unwrap().xp)})
}
fn rng(m: &Match) -> (String, Vec<Json>) {
    let snapshot = m.start.random.snapshot();
    assert_eq!(snapshot.seed, 701);
    let mut paired = MatchRandom::new(701);
    paired.next_fraction(StreamId::Combat);
    let initial = paired.snapshot().streams[0]
        .1
        .wrapping_sub(0x9e37_79b9_7f4a_7c15);
    let current = snapshot
        .streams
        .iter()
        .find(|(id, _)| *id == StreamId::Combat)
        .map_or(initial, |(_, s)| *s);
    let mut paired = MatchRandom::new(701);
    let mut state = initial;
    let mut draws = Vec::new();
    while state != current {
        assert!(draws.len() < 4096);
        draws.push(b(paired.next_fraction(StreamId::Combat)));
        state = paired.snapshot().streams[0].1;
    }
    (format!("{current:016x}"), draws)
}
fn snapshot(m: &Match, order: &[u32], baseline: &Json, shots: &[Json]) -> Json {
    let initial: BTreeMap<u32, &Json> = baseline["actors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|u| (u["id"].as_u64().unwrap() as u32, u))
        .collect();
    let actors = order
        .iter()
        .filter(|id| m.world.game.units.contains_key(id))
        .map(|id| actor(m, *id))
        .filter(|v| {
            initial
                .get(&(v["id"].as_u64().unwrap() as u32))
                .is_none_or(|base| **base != *v)
        })
        .collect::<Vec<_>>();
    let order = order
        .iter()
        .filter(|id| m.world.game.units.contains_key(id))
        .copied()
        .collect::<Vec<_>>();
    let players=(0..2).map(|slot|{let p=&m.start.economy.players[slot];json!({"slot":slot,"team":m.world.game.players[slot].team,"mp":b(p.mp),"mun":b(p.mun),"fuel":b(p.fuel),"visible":m.world.game.players[slot].visible})}).chain(std::iter::once(json!({"slot":-1,"team":-1,"mp":b(0.),"mun":b(0.),"fuel":b(0.),"visible":m.world_runtime.as_ref().unwrap().local_defender.visible}))).collect::<Vec<_>>();
    let claims=m.firing.claims.iter().map(|(&(target,team),&v)|json!({"target":target,"team":team,"value":b(v),"rawKey":i64::from(target)*8+i64::from(team)})).collect::<Vec<_>>();
    let (state, draws) = rng(m);
    let story=m.story.iter().map(|s|json!({"kills":s.kills,"losses":s.losses,"built":s.built,"captures":s.captures,"mpSpent":s.mp_spent,"supportCalls":s.support_calls,"planesDowned":s.planes_downed})).collect::<Vec<_>>();
    let story = if m.story.iter().all(|s| *s == PlayerStory::default()) {
        Json::Null
    } else {
        json!(story)
    };
    let nades=m.firing.grenades.iter().map(|n|{let p=&m.firing.flights[&n.flight];let def=Catalog::compiled().unwrap().unit(&m.rules.kinds[p.source_kind].name).unwrap();json!({"x":b(n.at.x),"z":b(n.at.z),"remaining":b(n.remaining),"owner":source_owner(n.owner),"flight":n.flight,"ab":blast_source(p.blast_source.unwrap(),def.get("ab").unwrap())})}).collect::<Vec<_>>();
    json!({"tick":m.world.game.tick,"nextId":m.next_unit_id,"actorOrder":order,"actors":actors,"players":players,"logicalClaims":claims,"flights":m.firing.flights.values().map(|p|flight(m,p)).collect::<Vec<_>>(),"shots":shots,"nades":nades,"smokes":m.world.game.smokes.iter().map(|s|json!({"id":s.id,"x":b(s.x),"z":b(s.z),"radius":b(s.r),"remaining":optional(s.remaining)})).collect::<Vec<_>>(),"story":story,"combatSeed":701,"combatRandomState":state,"randomCalls":draws})
}
fn prime(m: &mut Match, id: u32, fields: &Json) {
    for (k, v) in fields.as_object().unwrap() {
        match k.as_str() {
            "x" => m.world.game.units.get_mut(&id).unwrap().x = f(v),
            "z" => m.world.game.units.get_mut(&id).unwrap().z = f(v),
            "hp" => m.world.game.units.get_mut(&id).unwrap().hp = f(v),
            "rot" => m.world.game.units.get_mut(&id).unwrap().rot = f(v),
            "aim" => m.commands.units.get_mut(&id).unwrap().aim = f(v),
            "still" => m.world.game.units.get_mut(&id).unwrap().still = Some(f(v)),
            "cd" => m.abilities.get_mut(id).unwrap().cooldown = f(v),
            "buff" => m.abilities.get_mut(id).unwrap().buff = f(v),
            "sprint" => m.world.game.units.get_mut(&id).unwrap().sprint = f(v),
            "cooldown" => m.firing.get_mut(id).unwrap().cooldown = f(v),
            "retarget" => m.firing.get_mut(id).unwrap().retarget = f(v),
            "repath" => m.world.game.units.get_mut(&id).unwrap().repath = f(v),
            "react" => m.commands.units.get_mut(&id).unwrap().react = f(v),
            v => panic!("unrepresented prime {v}"),
        }
    }
}
fn collect(m: &mut Match, shots: &mut Vec<Json>) {
    shots.extend(
        m.firing
            .take_events()
            .into_iter()
            .map(|e| e.to_json(&m.rules)),
    );
}
#[test]
fn original_world_ground_actor_components() {
    let input = fixture();
    assert_eq!(
        input["reference"],
        "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
    );
    assert_eq!(input["cases"].as_array().unwrap().len(), 10);
    let baseline = &input["startup"]["initial"];
    for case in input["cases"].as_array().unwrap() {
        let mut m = make(&input);
        let mut order = baseline["actorOrder"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap() as u32)
            .collect::<Vec<_>>();
        let mut shots = Vec::new();
        let mut aliases = case["aliases"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), v.as_u64().unwrap() as u32))
            .collect::<BTreeMap<_, _>>();
        // Alias for a later birth cannot refer to an actor before its real publication.
        aliases.retain(|_, id| m.world.game.units.contains_key(id));
        for (index, a) in case["actions"].as_array().unwrap().iter().enumerate() {
            let op = a["op"].as_str().unwrap();
            let id = a["unit"].as_str().map(|s| aliases[s]);
            let mut result = Json::Null;
            match op {
                "prime" => prime(&mut m, id.unwrap(), &a["fields"]),
                "resource" => {
                    let slot = a["slot"].as_u64().unwrap() as usize;
                    for (k, v) in a["fields"].as_object().unwrap() {
                        match k.as_str() {
                            "mp" => m.start.economy.players[slot].mp = f(v),
                            "mun" => m.start.economy.players[slot].mun = f(v),
                            "fuel" => m.start.economy.players[slot].fuel = f(v),
                            _ => panic!("unsupported wallet input"),
                        }
                    }
                }
                "visible" => {
                    let ids = a["units"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|v| aliases[v.as_str().unwrap()])
                        .collect();
                    if a["slot"].as_i64() == Some(-1) {
                        m.world_runtime.as_mut().unwrap().local_defender.visible = ids
                    } else {
                        m.world.game.players[a["slot"].as_u64().unwrap() as usize].visible = ids
                    }
                }
                "scene" => {
                    let at = point(&a["at"]);
                    for (name, dx, dz, rot) in [
                        ("commander", 0., 0., 0.),
                        ("local", 10., 0., f64::from_bits(0x4009_21fb_5444_2d18)),
                        ("otherLocal", 12., 2., 0.),
                    ] {
                        let id = aliases[name];
                        let u = m.world.game.units.get_mut(&id).unwrap();
                        u.x = at.x + dx;
                        u.z = at.z + dz;
                        u.path.clear();
                        u.garrison = None;
                        u.still = Some(20.);
                        let c = m.commands.units.get_mut(&id).unwrap();
                        c.auto_cast = false;
                        if name != "otherLocal" {
                            u.rot = rot;
                            c.aim = rot;
                            c.hold_fire = false;
                            c.attack_id = 0;
                            let f = m.firing.get_mut(id).unwrap();
                            f.target_id = 0;
                            f.cooldown = 0.;
                            f.retarget = 0.;
                        }
                    }
                }
                "claims" => {
                    let ids = m.world.game.units.keys().copied().collect::<Vec<_>>();
                    m.begin_firing_tick(&ids).unwrap();
                }
                "pick" => result = json!(m.pick_firing_target(id.unwrap()).unwrap()),
                "firing" => m
                    .apply_unit_firing(id.unwrap(), false, f(&a["dt"]))
                    .unwrap(),
                "flights" => {
                    m.capture_firing_prior().unwrap();
                    for _ in 0..a["steps"].as_u64().unwrap() {
                        m.world.game.tick += 1;
                        m.step_firing_flights().unwrap();
                    }
                }
                "hurt" => {
                    let order = m.world.game.units.keys().copied().collect::<Vec<_>>();
                    let profile =
                        ExplosionSource::weapon(kind(&m, a["weapon"].as_str().unwrap()), &m.rules)
                            .unwrap();
                    let (damage, sink) = m.firing_damage_view(&order);
                    let mut damage = damage;
                    let hurt = damage
                        .hurt(
                            id.unwrap(),
                            profile,
                            f(&a["fall"]),
                            a["owner"].as_i64().unwrap() as i32,
                            sink,
                        )
                        .unwrap();
                    shots.push(crate::environment::Effect::Hurt(hurt).to_json());
                }
                "contact" => {
                    let p = m
                        .firing
                        .flights
                        .values()
                        .find(|p| p.shooter == aliases[a["shooter"].as_str().unwrap()])
                        .unwrap()
                        .clone();
                    let target = m.world.game.units[&id.unwrap()].at();
                    let origin = Point {
                        x: p.launch_origin.x,
                        z: p.launch_origin.z,
                    };
                    let source = FlightSource {
                        shooter: p.shooter,
                        kind: p.source_kind,
                        owner: source_owner(p.owner),
                        airborne: false,
                    };
                    let impact = Impact {
                        owner: source.owner,
                        shooter: source.shooter,
                        weapon: WeaponDamage::from_catalog(&m.rules.kinds[p.source_kind].name)
                            .unwrap(),
                        damage_scale: Some(p.damage_scale),
                        energy: p.energy,
                        armor_piercing: p.armor_piercing,
                        ap_naval_only: p.ap_naval,
                        ap_multiplier: p.ap_multiplier,
                        launch_origin: origin,
                        at: target,
                        direction: Point {
                            x: target.x - origin.x,
                            z: target.z - origin.z,
                        },
                    };
                    let order = m.world.game.units.keys().copied().collect::<Vec<_>>();
                    let (mut damage, sink) = m.firing_damage_view(&order);
                    result = b(damage
                        .direct_hit(source, id.unwrap(), impact, sink)
                        .unwrap());
                }
                "ability" => {
                    let t = aliases[a["target"].as_str().unwrap_or(a["unit"].as_str().unwrap())];
                    let at = m.world.game.units[&t].at();
                    let command = Command::from_json(
                        &json!({"t":"ability","ids":[id.unwrap()],"x":at.x,"z":at.z}),
                    )
                    .unwrap();
                    result = m
                        .execute_command(a["slot"].as_u64().unwrap() as usize, &command, false)
                        .unwrap()
                        .map(|e| json!(e.name()))
                        .unwrap_or(Json::Null);
                }
                "aimed" => m.step_aimed_ability(id.unwrap()).unwrap(),
                // Source Game/UnitControl timer ownership has no aggregate provider here.
                // Their three represented decays accompany the actual firing/ability providers.
                "timers" => {
                    let id = id.unwrap();
                    let dt = f(&a["dt"]);
                    m.advance_firing_timers(id, dt).unwrap();
                    m.advance_ability_timers(id, dt).unwrap();
                    m.world.game.units.get_mut(&id).unwrap().repath -= dt;
                    m.world.game.units.get_mut(&id).unwrap().sprint -= dt;
                    m.commands.units.get_mut(&id).unwrap().react -= dt;
                }
                "ordinaryDeath" => {
                    let ids = m.world.game.units.keys().copied().collect::<Vec<_>>();
                    let mut progress = RetirementProgress::default();
                    m.retire_dead_emitting(&ids, &mut progress, &mut |e| shots.push(e.to_json()))
                        .unwrap();
                    for retired in progress.removed {
                        let name = m.rules.kinds[retired.unit.kind].name.clone();
                        m.record_death(
                            DeathRecord {
                                owner: (retired.unit.owner != LOCAL_DEFENDER_OWNER)
                                    .then_some(retired.unit.owner),
                                kind: &name,
                                at: retired.unit.at(),
                            },
                            retired.enemy_killer,
                        )
                        .unwrap();
                    }
                }
                "adminRemove" => {
                    m.world.game.units.remove(&id.unwrap());
                    m.grid.remove_unit(id.unwrap());
                }
                "spawn" => {
                    let owner = owner(&a["owner"]);
                    let id = m.next_unit_id;
                    let at = m.world.game.units[&aliases["commander"]].at();
                    let draft = StagedUnit::ordinary(
                        &m.world.game,
                        &m.rules,
                        owner,
                        kind(&m, a["type"].as_str().unwrap()),
                        id,
                        m.world.game.units.len() as u32,
                        m.start.players[owner].spawn,
                        Some(at),
                    )
                    .unwrap();
                    prepare_world_publication(
                        PublicationParts {
                            units: &mut m.world.game.units,
                            grid: &mut m.grid,
                            controls: &mut m.commands.units,
                            recruitment: &mut m.recruit_metadata,
                            next_id: &mut m.next_unit_id,
                        },
                        &m.rules,
                        2,
                        m.world.game.mode,
                        vec![draft],
                    )
                    .unwrap()
                    .publish_with(|u, _| {
                        m.combat.initialize(u.id);
                        m.firing.initialize(u.id);
                        m.abilities.initialize(u.id);
                        m.recovery.initialize(u.id);
                    });
                    assert_eq!(id, a["id"].as_u64().unwrap() as u32);
                    aliases.insert(a["alias"].as_str().unwrap().into(), id);
                    order.push(id);
                }
                _ => panic!("unrepresented action {op}"),
            }
            m.grid = SpatialGrid::new(&m.world.game.units, 16.);
            collect(&mut m, &mut shots);
            assert_eq!(
                result, case["results"][index],
                "{} action{index} result",
                case["name"]
            );
            let actual = snapshot(&m, &order, baseline, &shots);
            let mut expected = case["snapshots"][index].clone();
            expected.as_object_mut().unwrap().remove("claims");
            assert!(
                equal(&actual, &expected),
                "{} action{index} {op}: {}",
                case["name"],
                difference(&actual, &expected, "snapshot")
            );
        }
    }
}
