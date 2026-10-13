//! Fixture setup and projections for actual ground-support Match ticks.
use flate2::read::GzDecoder;
use serde_json::{Value, json};
use std::io::Read;
use ww2_core::{
    catalog::Catalog,
    companions::Companions,
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::Point,
    grid::SpatialGrid,
    map::AuthoritativeMap,
    match_abilities::AbilityState,
    match_combat::CombatState,
    match_commands::{MatchCommands, UnitControl},
    match_end::MatchSample,
    match_firing::FiringState,
    match_recovery::RecoveryState,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    match_support::SupportEvent,
    random::{MatchRandom, RandomSnapshot, StreamId},
    unit_spawn,
    weather::{WeatherChoice, WeatherKind},
    world::CellRow,
};

fn n(v: &Value) -> f64 {
    v.as_f64().expect("source number")
}
fn point_json(p: Point) -> Value {
    json!({"x":p.x,"z":p.z})
}

pub fn fixture() -> Value {
    let mut text = String::new();
    GzDecoder::new(include_bytes!("../fixtures/ground-support-tick.json.gz").as_slice())
        .read_to_string(&mut text)
        .unwrap();
    serde_json::from_str(&text).unwrap()
}

pub fn make(t: &Value) -> Match {
    let setup = &t["setup"];
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&setup["map"]).unwrap()).unwrap();
    let living = setup["options"]["weather"] == "rain";
    let mut m = Match::new(
        &map,
        MatchOptions {
            seed: 701,
            mode: match setup["nativeMode"].as_str().unwrap() {
                "Default" => Mode::Default,
                "Conquest" => Mode::Conquest,
                _ => panic!("unsupported fixture mode"),
            },
            players: setup["teams"]
                .as_array()
                .unwrap()
                .iter()
                .enumerate()
                .map(|(i, team)| PlayerConfig {
                    name: if i == 0 { "A" } else { "B" }.into(),
                    team: n(team) as i32,
                    faction: Faction::try_from(n(&setup["factions"][i]) as u8).unwrap(),
                })
                .collect(),
            shuffle: false,
            army: ArmySize::Standard,
            weather: if living {
                WeatherChoice::Fixed(WeatherKind::Rain)
            } else {
                WeatherChoice::Map
            },
            weather_seed: Some(1),
            living_weather: living,
            map_key: None,
            tech: false,
            supply: false,
            logistics: false,
            sandbox: false,
        },
    )
    .unwrap();
    // Source clears actors while keeping the constructor's terrain and building footprints.
    let old: Vec<_> = m.world.game.units.keys().copied().collect();
    for id in old {
        m.production.remove_building(id);
    }
    m.world.game.units.clear();
    m.commands = MatchCommands::new(4096);
    m.firing = FiringState::default();
    m.combat = CombatState::default();
    m.abilities = AbilityState::default();
    m.recovery = RecoveryState::default();
    m.companions = Companions::default();
    m.recruit_metadata.clear();
    m.building_rallies.clear();
    let actor = &setup["unit"];
    let owner = n(&actor["owner"]) as usize;
    let kind = m.rules.kind(actor["kind"].as_str().unwrap()).unwrap();
    let mut u = unit_spawn::initial_unit(
        &m.rules,
        Catalog::compiled().unwrap(),
        kind,
        n(&actor["id"]) as u32,
        owner,
        m.start.players[owner].spawn,
    )
    .unwrap();
    u.x = n(&actor["x"]);
    u.z = n(&actor["z"]);
    u.hold_pos = actor["holdPos"].as_bool().unwrap();
    let mut control = UnitControl::new(&u, &m.rules).unwrap();
    control.hold_fire = actor["holdFire"].as_bool().unwrap();
    control.auto_cast = actor["auto"].as_bool().unwrap();
    control.auto_retreat = actor["autoRetreat"].as_bool().unwrap();
    m.commands.insert(u.id, control);
    m.firing.initialize(u.id);
    m.combat.initialize(u.id);
    m.abilities.initialize(u.id);
    m.recovery.initialize(u.id);
    m.world.game.units.insert(u.id, u);
    m.grid = SpatialGrid::new(&m.world.game.units, 16.0);
    m.next_unit_id = n(&setup["nextId"]) as u32;
    m.start.environment_seed = n(&setup["environmentSeed"]) as u32;
    m.start.wind = (n(&setup["wind"]["a"]), n(&setup["wind"]["v"]));
    for p in &mut m.start.economy.players {
        p.mp = 500.0;
    }
    let spec = &t["spec"];
    m.start.economy.players[0].mp = spec["mp"].as_f64().unwrap_or(500.0);
    m.start.economy.players[0].away = spec["away"].as_bool().unwrap_or(false);
    m.start.economy.players[0].out = spec["out"].as_bool().unwrap_or(false);
    if let Some(cd) = spec["cooldown"].as_f64() {
        m.start.players[0]
            .support
            .insert(spec["kind"].as_str().unwrap().into(), cd);
    }
    if spec["radio"] == true {
        m.start.points[0].owner = Some(0);
        m.start.points[0].cut = spec["cut"] == true;
    }
    // This is authored setup. Runtime outputs never write back into Match state.
    assert_eq!(m.start.random.snapshot().seed, 701);
    assert!(
        m.start
            .random
            .snapshot()
            .streams
            .iter()
            .all(|(kind, _)| !matches!(
                kind,
                StreamId::Combat | StreamId::OffMap | StreamId::Ai(_)
            ))
    );
    m
}

fn unit_row(m: &Match, id: u32) -> Value {
    let u = &m.world.game.units[&id];
    let c = &m.commands.units[&id];
    let f = m.firing.get(id).unwrap();
    let combat = m.combat.get(id).unwrap();
    let a = m.abilities.get(id).unwrap();
    let recovery = m.recovery.get(id).unwrap();
    assert!(c.orders.is_empty() && !u.dig && !u.build && !u.entrench);
    assert_eq!(u.riding, combat.carrier.is_some());
    json!({"id":u.id,"type":m.rules.kinds[u.kind].name,"owner":u.owner,"x":u.x,"z":u.z,"rot":u.rot,"aim":c.aim,
        "moveSpeed":u.move_speed,"travelDir":u.travel_dir,"vx":u.vx,"vz":u.vz,"hp":u.hp,"supp":u.supp,
        "path":u.path.iter().copied().map(point_json).collect::<Vec<_>>(),"orders":[],"attackId":c.attack_id,"targetId":f.target_id,
        "cooldown":f.cooldown,"still":u.still,"retarget":f.retarget,"repath":u.repath,"stuck":u.stuck,"cd":a.cooldown,
        "buff":a.buff,"ap":a.ap,"nade":c.nade.map(point_json),"retreating":u.retreating,"reinf":recovery.reinforcement,"dig":null,
        "garrison":u.garrison.map_or(-1,|v|v as i64),"enter":c.enter.map_or(-1,|v|v as i64),"board":u.board,"amove":u.amove.map(point_json),
        "face":c.face.map(|f|json!({"a":f.angle,"x":f.at.x,"z":f.at.z})),"xp":combat.xp,"fireAt":c.fire_at.map_or(-1,|v|v as i64),
        "sprint":u.sprint,"riding":combat.carrier.unwrap_or(0),"cargo":combat.cargo,"hitBy":combat.hit_by.unwrap_or(0),"hitAt":combat.hit_at,
        "hitFrom":u.hit_from.map(point_json),"drift":u.drift,"reverse":u.reverse.map(point_json),"react":c.react,
        "auto":c.auto_cast,"autoRetreat":c.auto_retreat,"holdFire":c.hold_fire,"holdPos":u.hold_pos,
        "traffic":u.traffic.as_ref().map(|t|json!({"goal":point_json(t.goal),"legs":t.legs.iter().copied().map(point_json).collect::<Vec<_>>(),"passing":t.passing,"by":t.by,"until":t.until})),
        "trafficWait":u.traffic_wait,"queuedBehind":u.queued_behind,"lastHit":combat.last_hit,
        "deathImpulse":combat.death_impulse.map(|d|json!({"dir":d.direction,"impulse":d.magnitude})),
        "motionBefore":u.motion_before.map(|p|json!({"x":p.x,"z":p.z,"rot":p.rot}))})
}
fn runs(items: impl IntoIterator<Item = Value>) -> Value {
    let mut rows: Vec<Value> = Vec::new();
    for value in items {
        if rows.last().is_some_and(|row| row[0] == value) {
            let row = rows.last_mut().unwrap();
            row[1] = json!(row[1].as_u64().unwrap() + 1);
        } else {
            rows.push(json!([value, 1]));
        }
    }
    json!(rows)
}
fn cell_row(row: &CellRow) -> Value {
    let d = &row.data;
    let mut data = json!({"ground":char::from(d.ground).to_string(),"object":char::from(d.object).to_string(),"material":d.material,"groundMaterial":d.ground_material,"groundWear":f64::from(d.ground_wear)});
    if let Some(s) = &d.section {
        data["section"] = json!({"id":s.id,"structureId":s.structure_id,"state":s.state,"hp":s.hp,"maxHp":s.max_hp,"material":s.material,"anchor":s.anchor});
    }
    assert!(d.trench_front.is_none() && !d.mine && d.rail.is_none() && d.station.is_none());
    json!([
        row.cell,
        char::from(row.ch).to_string(),
        row.height,
        row.state,
        data
    ])
}

pub fn events(m: &mut Match) -> Value {
    json!(
        m.support
            .take_events()
            .into_iter()
            .map(|event| match event {
                SupportEvent::Shell(p) => json!({"x":p.x,"z":p.z,"k":"shell","pub":true}),
                SupportEvent::SmokeShells(p) =>
                    json!({"x":p.x,"z":p.z,"k":"smokeshells","pub":true}),
                SupportEvent::Environment(effect) => effect.to_json(),
            })
            .collect::<Vec<_>>()
    )
}

pub fn signature(m: &Match) -> Value {
    json!([
        m.support
            .strikes
            .iter()
            .map(|s| json!([s.kind.name(), s.live, s.left]))
            .collect::<Vec<_>>(),
        m.support.covers.len(),
        m.world.game.smokes.len(),
        m.world.game.units.len()
    ])
}

pub fn compare(
    m: &Match,
    encoded: &Value,
    shots: &Value,
    initial_rng: &RandomSnapshot,
    context: &str,
) {
    let g = &m.world.game;
    let story=json!(m.story.iter().map(|s|json!({"kills":s.kills,"losses":s.losses,"built":s.built,"captures":s.captures,"mpSpent":s.mp_spent,"supportCalls":s.support_calls,"planesDowned":s.planes_downed})).collect::<Vec<_>>());
    let wx = m.start.living.as_ref().map(
        |living| json!({"raining":living.raining,"next":living.next,"rain":g.rain,"wet":g.wet}),
    );
    assert!(
        g.wrecks.is_empty()
            && m.firing.flights.is_empty()
            && m.firing.grenades.is_empty()
            && m.firing.salvos.is_empty()
    );
    assert!(m.fallen.structure.is_none() && m.fallen.bunker.is_none() && m.fallen.hq.is_none());
    let actual = json!({"tick":g.tick,"visionTick":g.vision_tick,"nextId":m.next_unit_id,"seed":f64::from(m.start.environment_seed as i32),
        "wind":{"a":m.start.wind.0,"v":m.start.wind.1},"wx":wx,"winner":m.finish.map(|f|f.winner),
        "players":m.start.players.iter().enumerate().map(|(i,p)|{let e=&m.start.economy.players[i];json!({"slot":i,"name":p.config.name,"team":g.players[i].team,"faction":p.config.faction as u8,"vp":e.vp,"mp":e.mp,"inc":e.inc,"sup":p.support,"spawn":point_json(p.spawn),"visible":g.players[i].visible,"away":e.away,"out":e.out})}).collect::<Vec<_>>(),
        "points":m.start.points.iter().enumerate().map(|(i,p)|json!({"scenarioId":p.id,"x":p.at.x,"z":p.at.z,"vp":p.vp,"mp":p.mp,"owner":p.owner.map_or(-1,|v|v as i64),"capper":p.capper.map_or(-1,|v|v as i64),"progress":p.progress,"cut":p.cut,"kind":p.kind,"needs":p.needs.map_or(-1,|v|v as i64),"onPoint":p.on_point,"contested":p.contested,"capTick":m.capture_ticks[i]})).collect::<Vec<_>>(),
        "units":g.units.keys().map(|id|unit_row(m,*id)).collect::<Vec<_>>(),"unitOrder":g.units.keys().collect::<Vec<_>>(),
        "strikes":m.support.strikes.iter().map(|s|json!({"kind":s.kind.name(),"owner":s.owner,"x":s.at.x,"z":s.at.z,"dir":s.direction,"t":s.delay,"left":s.left,"next":s.next,"live":s.live})).collect::<Vec<_>>(),
        "covers":m.support.covers.iter().map(|c|json!({"team":c.team,"owner":c.owner,"x":c.at.x,"z":c.at.z,"r":c.radius,"t":c.remaining})).collect::<Vec<_>>(),
        "smokes":g.smokes.iter().map(|s|json!({"id":s.id,"x":s.x,"z":s.z,"r":s.r,"t":s.remaining})).collect::<Vec<_>>(),"shots":shots,
        "timeline":m.timeline.iter().map(|s|match s { MatchSample::Conquest {t,vp}=>json!({"t":t,"vp":vp}), MatchSample::Territories {..}=>panic!("ground-support fixture cannot represent a Territories timeline") }).collect::<Vec<_>>(),"flights":[],"nades":[],"salvos":[],
        "environment":{"fires":m.world.fires.iter().map(|(c,t)|json!([c,t])).collect::<Vec<_>>(),"soak":m.world.soak(),"mines":m.world.mines.iter().map(|(c,o)|json!([c,o])).collect::<Vec<_>>(),"aid":m.world.aid.iter().map(|(c,o)|json!([c,o])).collect::<Vec<_>>(),"claims":m.firing.target_claims().map(|((target,team),v)|json!([u64::from(target)*8+team as u64,v])).collect::<Vec<_>>(),"wrecks":[],"newCells":m.world.new_cells().iter().map(cell_row).collect::<Vec<_>>(),"cellLog":m.world.cell_log().iter().map(cell_row).collect::<Vec<_>>()},
        "terrain":{"chars":runs(g.chars.iter().map(|c|json!(char::from(*c).to_string()))),"flags":runs(g.flags.iter().map(|c|json!(c))),"height":g.height.as_ref().map(|h|runs(h.iter().map(|n|json!(n)))),"cellHp":runs(g.cell_hp.iter().map(|n|json!(f64::from(*n)))),"wear":runs(g.wear.as_ref().unwrap().iter().map(|n|json!(f64::from(*n)))),"scar":runs(m.world.scar.iter().map(|n|json!(f64::from(*n)))),"burnt":runs(m.world.burnt.iter().map(|n|json!(n))),"cellState":runs(m.world.cell_state.iter().map(|n|json!(n))),"versions":[g.terrain_version,m.world.versions.obstruction,m.world.versions.infantry,m.world.versions.vehicle,m.world.versions.naval]}});
    for (key, value) in encoded["raw"].as_object().unwrap() {
        match key.as_str() {
            "random" => compare_random(m, value, initial_rng, context),
            "story" => {
                if value.is_null() {
                    for row in story.as_array().unwrap() {
                        for n in row.as_object().unwrap().values() {
                            assert_eq!(
                                n.as_f64().unwrap().to_bits(),
                                0f64.to_bits(),
                                "{context}: absent story"
                            );
                        }
                    }
                } else {
                    assert_source(&story, value, &encoded["floatBits"], key, context);
                }
            }
            "fallen" => assert!(value.is_null(), "fixture excludes fallen actors"),
            "timeline" if value.is_null() => assert!(m.timeline.is_empty()),
            _ => assert_source(
                actual
                    .get(key)
                    .unwrap_or_else(|| panic!("missing projection {key}")),
                value,
                &encoded["floatBits"],
                key,
                context,
            ),
        }
    }
}

fn assert_source(actual: &Value, expected: &Value, bits: &Value, path: &str, context: &str) {
    match expected {
        Value::Number(_) => {
            let number = actual
                .as_f64()
                .unwrap_or_else(|| panic!("{context}:{path}: missing numeric field"));
            assert_eq!(
                format!("{:016x}", number.to_bits()),
                bits[path].as_str().unwrap(),
                "{context}:{path}"
            );
        }
        Value::Array(rows) => {
            let a = actual
                .as_array()
                .unwrap_or_else(|| panic!("{context}:{path}: expected array"));
            assert_eq!(a.len(), rows.len(), "{context}:{path}");
            for (i, (a, e)) in a.iter().zip(rows).enumerate() {
                assert_source(a, e, bits, &format!("{path}.{i}"), context);
            }
        }
        Value::Object(rows) => {
            for (k, e) in rows {
                assert_source(
                    actual
                        .get(k)
                        .unwrap_or_else(|| panic!("{context}:{path}.{k}: missing field")),
                    e,
                    bits,
                    &format!("{path}.{k}"),
                    context,
                );
            }
        }
        _ => assert_eq!(actual, expected, "{context}:{path}"),
    }
}

fn compare_random(m: &Match, source: &Value, initial: &RandomSnapshot, context: &str) {
    let current = m.start.random.snapshot();
    assert_eq!(current.seed, 701);
    assert_eq!(
        current
            .streams
            .iter()
            .filter(|(k, _)| *k != StreamId::OffMap)
            .collect::<Vec<_>>(),
        initial
            .streams
            .iter()
            .filter(|(k, _)| *k != StreamId::OffMap)
            .collect::<Vec<_>>(),
        "{context}: unrelated random purpose changed"
    );
    let count = n(&source["count"]) as usize;
    let actual = current
        .streams
        .iter()
        .find_map(|(k, s)| (*k == StreamId::OffMap).then_some(*s));
    if count == 0 {
        assert!(actual.is_none(), "{context}: premature OffMap draw");
    } else {
        let mut predictor = MatchRandom::new(701);
        for _ in 0..count {
            predictor.next_fraction(StreamId::OffMap);
        }
        assert_eq!(
            actual,
            predictor
                .snapshot()
                .streams
                .iter()
                .find_map(|(k, s)| (*k == StreamId::OffMap).then_some(*s)),
            "{context}: draw count"
        );
        assert_eq!(
            format!("{:016x}", actual.unwrap()),
            source["state"].as_str().unwrap(),
            "{context}: Source OffMap state"
        );
    }
}
