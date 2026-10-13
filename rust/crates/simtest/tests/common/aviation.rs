//! Shared canonical scene factories and projections for aircraft component and runner evidence.
use serde_json::{Value, json};
use ww2_core::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::{PlaneState, Point},
    map::AuthoritativeMap,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    unit_publication::{PublicationParts, StagedUnit, prepare_publication},
    weather::WeatherChoice,
};
pub fn bits(value: f64) -> Value {
    json!(format!("{:016x}", value.to_bits()))
}
// JSON numbers have one source Number representation; state bit strings remain exact.
pub fn assert_json(actual: &Value, expected: &Value, path: &str) {
    match expected {
        Value::Number(_) => {
            let a = actual
                .as_f64()
                .unwrap_or_else(|| panic!("{path}: expected number"));
            let b = expected.as_f64().unwrap();
            if a == 0.0 && b == 0.0 {
                return;
            }
            assert_eq!(a.to_bits(), b.to_bits(), "{path}");
        }
        Value::Array(rows) => {
            let actual = actual.as_array().unwrap();
            assert_eq!(actual.len(), rows.len(), "{path}");
            for (i, (a, b)) in actual.iter().zip(rows).enumerate() {
                assert_json(a, b, &format!("{path}[{i}]"));
            }
        }
        Value::Object(rows) => {
            let actual = actual.as_object().unwrap();
            assert_eq!(actual.len(), rows.len(), "{path}");
            for (key, b) in rows {
                assert_json(
                    actual
                        .get(key)
                        .unwrap_or_else(|| panic!("{path}.{key}: missing")),
                    b,
                    &format!("{path}.{key}"),
                );
            }
        }
        _ => assert_eq!(actual, expected, "{path}"),
    }
}

pub fn make(map: &Value) -> Match {
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(map).unwrap()).unwrap();
    Match::new(
        &map,
        MatchOptions {
            seed: 811,
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
            shuffle: false,
            army: ArmySize::Standard,
            weather: WeatherChoice::Map,
            weather_seed: Some(1),
            living_weather: false,
            map_key: None,
            tech: false,
            supply: false,
            logistics: false,
            sandbox: false,
        },
    )
    .unwrap()
}

pub fn plane_state(name: &str) -> PlaneState {
    match name {
        "base" => PlaneState::Base,
        "out" => PlaneState::Out,
        "station" => PlaneState::Station,
        "home" => PlaneState::Home,
        "rearm" => PlaneState::Rearm,
        _ => panic!("unknown aircraft state"),
    }
}
pub fn state_name(state: PlaneState) -> &'static str {
    match state {
        PlaneState::Base => "base",
        PlaneState::Out => "out",
        PlaneState::Station => "station",
        PlaneState::Home => "home",
        PlaneState::Rearm => "rearm",
    }
}
pub fn mission(value: &Value) -> ww2_core::match_aviation::PlaneMission {
    use ww2_core::match_aviation::MissionKind;
    ww2_core::match_aviation::PlaneMission {
        kind: match value["kind"].as_str().unwrap() {
            "patrol" => MissionKind::Patrol,
            "escort" => MissionKind::Escort,
            "attack" => MissionKind::Attack,
            "area" => MissionKind::Area,
            _ => panic!("unknown mission"),
        },
        target: value["id"]
            .as_u64()
            .filter(|id| *id != 0)
            .map(|id| id as u32),
        at: Point {
            x: value["x"].as_f64().unwrap(),
            z: value["z"].as_f64().unwrap(),
        },
    }
}
pub fn patch(m: &mut Match, id: u32, value: &Value) {
    let u = m.world.game.units.get_mut(&id).unwrap();
    for (name, field) in [
        ("x", &mut u.x),
        ("z", &mut u.z),
        ("hp", &mut u.hp),
        ("rot", &mut u.rot),
        ("supp", &mut u.supp),
        ("repath", &mut u.repath),
        ("sprint", &mut u.sprint),
    ] {
        if let Some(v) = value[name].as_f64() {
            *field = v;
        }
    }
    let ability = m.abilities.get_mut(id).unwrap();
    if let Some(v) = value["cd"].as_f64() {
        ability.cooldown = v;
    }
    if let Some(v) = value["buff"].as_f64() {
        ability.buff = v;
    }
    if let Some(v) = value["react"].as_f64() {
        m.commands.units.get_mut(&id).unwrap().react = v;
    }
    if let Some(v) = value["holdPos"].as_bool() {
        u.hold_pos = v;
    }
    if let Some(v) = value["auto"].as_bool() {
        m.commands.units.get_mut(&id).unwrap().auto_cast = v;
    }
    if let Some(v) = value["autoRetreat"].as_bool() {
        m.commands.units.get_mut(&id).unwrap().auto_retreat = v;
    }
    let firing = m.firing.get_mut(id).unwrap();
    if let Some(v) = value["cooldown"].as_f64() {
        firing.cooldown = v;
    }
    if let Some(v) = value["retarget"].as_f64() {
        firing.retarget = v;
    }
    if let Some(v) = value["still"].as_f64() {
        u.still = Some(v);
    }
    if let Some(v) = value["built"].as_f64() {
        u.built = Some(v);
    }
    if let Some(v) = value.get("operative") {
        m.commands.units.get_mut(&id).unwrap().operative =
            v.as_str().is_some_and(|s| !s.is_empty()) || v.as_bool() == Some(true);
    }
    if let Some(v) = value["riding"].as_u64() {
        u.riding = v != 0;
    }
    if let Some(v) = value["holdFire"].as_bool() {
        m.commands.units.get_mut(&id).unwrap().hold_fire = v;
    }
    if let Some(v) = value["xp"].as_f64() {
        m.combat.get_mut(id).unwrap().xp = v;
    }
    if let Some(v) = value["lastHit"].as_i64() {
        m.combat.get_mut(id).unwrap().last_hit = Some(v as i32);
    }
    if let Some(air) = value.get("air") {
        if let Some(v) = air["state"].as_str() {
            u.air = Some(plane_state(v));
        }
        let a = m.aviation.planes.get_mut(&id).unwrap();
        for (name, field) in [
            ("fuel", &mut a.fuel),
            ("ammo", &mut a.ammo),
            ("timer", &mut a.timer),
            ("ang", &mut a.orbit),
        ] {
            if let Some(v) = air[name].as_f64() {
                *field = v;
            }
        }
        if let Some(v) = air.get("mission") {
            a.mission = if v.is_null() { None } else { Some(mission(v)) };
        }
    }
    if let Some(v) = value.get("logistics") {
        m.logistics.insert(
            id,
            ww2_core::logistics::Reserves {
                ammo: v["ammo"].as_f64().unwrap(),
                ammo_max: v["ammoMax"].as_f64().unwrap(),
                ammo_price: v["ammoPrice"].as_f64().unwrap(),
                ammo_credit: v["ammoCredit"].as_f64(),
                provisions: v["provisions"].as_f64().unwrap(),
                fuel: v["fuel"].as_f64(),
                emergency: v["emergency"].as_f64(),
                warning: v["warning"].as_f64(),
                forced: v["forced"].as_bool().unwrap(),
                stranded: v["stranded"].as_bool().unwrap(),
                exhausted: v["exhausted"].as_bool().unwrap(),
                supply_rate: None,
                cut_for: None,
            },
        );
    }
    if value.get("aaShot").is_some() || value.get("aaReserve").is_some() {
        let aa = m.aviation.anti_air.entry(id).or_default();
        if let Some(v) = value["aaShot"].as_f64() {
            aa.shot = Some(v);
        }
        if let Some(v) = value["aaReserve"].as_f64() {
            aa.reserve = Some(v);
        }
    }
    m.grid.update(id, &m.world.game.units[&id]);
}
pub fn scene(trace: &Value) -> Match {
    let mut m = make(&trace["map"]);
    m.world.game.units.clear();
    m.commands.units.clear();
    m.recruit_metadata.clear();
    m.next_unit_id = 1000;
    m.grid.rebuild(&m.world.game.units);
    for spec in trace["specs"].as_array().unwrap() {
        let owner = spec["owner"].as_u64().unwrap() as usize;
        let name = spec["kind"].as_str().unwrap();
        let kind = m.rules.kind(name).unwrap();
        let id = if m.rules.kinds[kind].air {
            m.spawn_aircraft(owner, name).unwrap()
        } else {
            let id = m.next_unit_id;
            let draft = StagedUnit::ordinary(
                &m.world.game,
                &m.rules,
                owner,
                kind,
                id,
                m.world.game.units.len() as u32,
                m.start.players[owner].spawn,
                None,
            )
            .unwrap();
            prepare_publication(
                PublicationParts {
                    units: &mut m.world.game.units,
                    grid: &mut m.grid,
                    controls: &mut m.commands.units,
                    recruitment: &mut m.recruit_metadata,
                    next_id: &mut m.next_unit_id,
                },
                &m.rules,
                2,
                vec![draft],
            )
            .unwrap()
            .publish_with(|u, _| {
                m.combat.initialize(u.id).hit_by = Some(0);
                m.firing.initialize(u.id);
                m.abilities.initialize(u.id);
                m.recovery.initialize(u.id);
            });
            id
        };
        patch(&mut m, id, spec);
    }
    let visible: Vec<u32> = if trace["visible"].as_bool().unwrap() {
        m.world.game.units.keys().copied().collect()
    } else {
        Vec::new()
    };
    for p in &mut m.world.game.players {
        p.visible = visible.clone();
    }
    m
}
pub fn snapshot(m: &Match, events: &[ww2_core::match_aviation::AviationEvent]) -> Value {
    use ww2_core::match_aviation::AviationEvent;
    let units = m.world.game.units.values().map(|u| {
        let f = m.firing.get(u.id).unwrap();
        let c = m.combat.get(u.id).unwrap();
        let control = &m.commands.units[&u.id];
        let aa = m.aviation.anti_air.get(&u.id).copied().unwrap_or_default();
        let a = m.aviation.planes.get(&u.id);
        let logistics = m.logistics.get(u.id).map(|r|json!({"ammo":bits(r.ammo),"forced":r.forced,"stranded":r.stranded,"exhausted":r.exhausted}));
        let air = a.map(|a| json!({"state":state_name(u.air.unwrap()),"fuel":bits(a.fuel),"ammo":bits(a.ammo),"timer":bits(a.timer),"ang":bits(a.orbit),"mission":a.mission.map(|m|json!({"kind":m.kind.name(),"id":m.target.unwrap_or(0),"x":bits(m.at.x),"z":bits(m.at.z)}))}));
        json!({"id":u.id,"kind":m.rules.kinds[u.kind].name,"owner":u.owner,"x":bits(u.x),"z":bits(u.z),"rot":bits(u.rot),"hp":bits(u.hp),"cooldown":bits(f.cooldown),"retarget":bits(f.retarget),"targetId":f.target_id,"aim":bits(control.aim),"holdFire":control.hold_fire,"xp":bits(c.xp),"lastHit":c.last_hit,"downed":a.is_some_and(|a|a.downed),"aaShot":aa.shot.map(bits),"aaReserve":aa.reserve.map(bits),"logistics":logistics,"air":air})
    }).collect::<Vec<_>>();
    let events = events.iter().map(|e| match e {
        AviationEvent::AntiAir {shooter,target,owner,target_owner,at} => json!({"f":shooter,"t":target,"fo":owner,"to":target_owner,"x":at.x,"z":at.z,"k":"aa"}),
        AviationEvent::PlaneDown {target,owner,at,direction} => json!({"k":"planedown","t":target,"to":owner,"x":at.x,"z":at.z,"dir":direction,"kill":true}),
    }).collect::<Vec<_>>();
    json!({"units":units,"story":m.story.iter().map(|p|bits(p.planes_downed)).collect::<Vec<_>>(),"events":events})
}

// Component tests read the producer buffer; the runner uses detached report events.
#[allow(dead_code)]
pub fn aircraft_phase_snapshot(m: &Match) -> Value {
    aircraft_phase_snapshot_with_events(m, &m.aviation.events)
}

pub fn aircraft_phase_snapshot_with_events(
    m: &Match,
    events: &[ww2_core::match_aviation::AviationEvent],
) -> Value {
    let all = snapshot(m, events);
    let planes = all["units"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|u| !u["air"].is_null())
        .cloned()
        .collect::<Vec<_>>();
    let ground = all["units"].as_array().unwrap().iter()
        .filter(|u| u["air"].is_null() && matches!(u["kind"].as_str(), Some("flak" | "flaktrack" | "flakpos" | "mg")))
        .map(|u| json!({"id":u["id"],"hp":u["hp"],"xp":u["xp"],"lastHit":u["lastHit"],"aaShot":u["aaShot"],"aaReserve":u["aaReserve"]})).collect::<Vec<_>>();
    json!({"planes":planes,"ground":ground,"story":all["story"],"events":all["events"]})
}
