use serde_json::{Value, json};
use std::io::Read;
use ww2_core::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::{PlaneState, Point},
    map::AuthoritativeMap,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    unit_publication::{PublicationParts, StagedUnit, prepare_publication},
    weather::WeatherChoice,
};
fn fixture() -> Value {
    let mut text = String::new();
    flate2::read::GzDecoder::new(&include_bytes!("fixtures/match-aviation.json.gz")[..])
        .read_to_string(&mut text)
        .unwrap();
    serde_json::from_str(&text).unwrap()
}
fn bits(value: f64) -> Value {
    json!(format!("{:016x}", value.to_bits()))
}
// JSON numbers have one source Number representation; state bit strings remain exact.
fn assert_json(actual: &Value, expected: &Value, path: &str) {
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
fn make(map: &Value) -> Match {
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
#[test]
fn explicit_aircraft_factory_matches_original_spawn_and_preserves_ordinary_policy() {
    let f = fixture();
    for row in f["constructors"].as_array().unwrap() {
        let mut m = make(&row["map"]);
        m.world.game.units.clear();
        m.commands.units.clear();
        m.recruit_metadata.clear();
        m.next_unit_id = 1000;
        m.grid.rebuild(&m.world.game.units);
        let owner = row["owner"].as_u64().unwrap() as usize;
        let kind = m.rules.kind(row["kind"].as_str().unwrap()).unwrap();
        let index = row["index"].as_u64().unwrap() as u32;
        let spawn = Point {
            x: row["spawn"]["x"].as_f64().unwrap(),
            z: row["spawn"]["z"].as_f64().unwrap(),
        };
        let base = Point {
            x: row["base"]["x"].as_f64().unwrap(),
            z: row["base"]["z"].as_f64().unwrap(),
        };
        assert!(
            StagedUnit::ordinary(
                &m.world.game,
                &m.rules,
                owner,
                kind,
                1000,
                index,
                spawn,
                None
            )
            .is_err()
        );
        let draft = StagedUnit::aircraft(
            &m.world.game,
            &m.rules,
            owner,
            kind,
            1000,
            index,
            spawn,
            base,
        )
        .unwrap();
        let u = &draft.unit;
        let c = &draft.control;
        assert!(c.orders.is_empty());
        let got = json!({"id":u.id,"owner":u.owner,"kind":m.rules.kinds[u.kind].name,"x":bits(u.x),"z":bits(u.z),"rot":bits(u.rot),"hp":bits(u.hp),"supp":bits(u.supp),"air":if u.air==Some(PlaneState::Base){"base"}else{"unexpected"},"still":bits(u.still.unwrap()),"vx":bits(u.vx.unwrap()),"vz":bits(u.vz.unwrap()),"moveSpeed":bits(u.move_speed),"travelDir":bits(u.travel_dir.unwrap()),"path":u.path.iter().map(|p|json!({"x":p.x,"z":p.z})).collect::<Vec<_>>(),"orders":Vec::<Value>::new(),"auto":c.auto_cast,"autoRetreat":c.auto_retreat,"aim":bits(c.aim),"cooldown":bits(0.0),"retarget":bits(0.0),"targetId":0,"hitBy":0,"hitAt":bits(-1e9)});
        assert_eq!(got, row["expected"], "{}", row["kind"]);
        let ids = prepare_publication(
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
        });
        assert_eq!(ids, [1000]);
        assert_eq!(m.next_unit_id, 1001);
        assert_eq!(m.world.game.units[&1000].at(), base);
        let firing = m.firing.get(1000).unwrap();
        let combat = m.combat.get(1000).unwrap();
        for (key, actual) in [
            ("cooldown", bits(firing.cooldown)),
            ("retarget", bits(firing.retarget)),
            ("targetId", json!(firing.target_id)),
            ("hitBy", json!(combat.hit_by)),
            ("hitAt", bits(combat.hit_at)),
        ] {
            assert_eq!(actual, row["expected"][key], "published {key}");
        }
    }
    assert_eq!(f["constructors"].as_array().unwrap().len(), 18);
}

fn plane_state(name: &str) -> PlaneState {
    match name {
        "base" => PlaneState::Base,
        "out" => PlaneState::Out,
        "station" => PlaneState::Station,
        "home" => PlaneState::Home,
        "rearm" => PlaneState::Rearm,
        _ => panic!("unknown aircraft state"),
    }
}
fn state_name(state: PlaneState) -> &'static str {
    match state {
        PlaneState::Base => "base",
        PlaneState::Out => "out",
        PlaneState::Station => "station",
        PlaneState::Home => "home",
        PlaneState::Rearm => "rearm",
    }
}
fn mission(value: &Value) -> ww2_core::match_aviation::PlaneMission {
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
fn patch(m: &mut Match, id: u32, value: &Value) {
    let u = m.world.game.units.get_mut(&id).unwrap();
    for (name, field) in [
        ("x", &mut u.x),
        ("z", &mut u.z),
        ("hp", &mut u.hp),
        ("rot", &mut u.rot),
    ] {
        if let Some(v) = value[name].as_f64() {
            *field = v;
        }
    }
    if let Some(v) = value["still"].as_f64() {
        u.still = Some(v);
    }
    if let Some(v) = value["built"].as_f64() {
        u.built = Some(v);
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
fn scene(trace: &Value) -> Match {
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
fn snapshot(m: &Match, events: &[ww2_core::match_aviation::AviationEvent]) -> Value {
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
#[test]
fn held_flights_rearming_and_continuous_aa_match_every_original_call() {
    let f = fixture();
    let mut checkpoints = 0;
    for trace in f["traces"].as_array().unwrap() {
        let mut m = scene(trace);
        assert_json(
            &snapshot(&m, &[]),
            &trace["initial"],
            &format!("{} initial", trace["name"]),
        );
        checkpoints += 1;
        for (index, frame) in trace["frames"].as_array().unwrap().iter().enumerate() {
            let a = &frame["action"];
            let id = a["id"].as_u64().unwrap_or(0) as u32;
            let admitted = match a["op"].as_str().unwrap() {
                "step" => {
                    m.step_aircraft(id, a["dt"].as_f64().unwrap()).unwrap();
                    None
                }
                "aa" => {
                    let ids = a["actors"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|v| v.as_u64().unwrap() as u32)
                        .collect::<Vec<_>>();
                    m.step_aircraft_anti_air(&ids, a["dt"].as_f64().unwrap())
                        .unwrap();
                    None
                }
                "mission" => Some(
                    m.send_aircraft_mission(
                        m.world.game.units[&id].owner,
                        id,
                        mission(&a["mission"]),
                    )
                    .unwrap(),
                ),
                "home" => Some(
                    m.return_aircraft_home(m.world.game.units[&id].owner, id)
                        .unwrap(),
                ),
                "patch" => {
                    patch(&mut m, id, &a["value"]);
                    None
                }
                "remove" => {
                    m.world.game.units.remove(&id);
                    m.grid.rebuild(&m.world.game.units);
                    None
                }
                "visible" => {
                    m.world.game.players[a["owner"].as_u64().unwrap() as usize].visible = a["ids"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|v| v.as_u64().unwrap() as u32)
                        .collect();
                    None
                }
                "rebuild" => {
                    m.grid.rebuild(&m.world.game.units);
                    None
                }
                "reregister" => {
                    m.grid.remove_unit(id);
                    m.grid.update(id, &m.world.game.units[&id]);
                    None
                }
                _ => panic!("unknown fixture action"),
            };
            assert_eq!(
                json!(admitted),
                frame["admitted"],
                "{} action {index}",
                trace["name"]
            );
            let events = m.aviation.take_events();
            assert_json(
                &snapshot(&m, &events),
                &frame["expected"],
                &format!("{} action {index}", trace["name"]),
            );
            assert!(m.firing.events.is_empty());
            checkpoints += 1;
        }
    }
    assert_eq!(f["traces"].as_array().unwrap().len(), 42);
    assert_eq!(checkpoints, f["checkpoints"].as_u64().unwrap());
    assert_eq!(checkpoints, 3410);
}
#[test]
fn unavailable_weapon_and_invalid_late_metadata_refuse_before_mutation() {
    use ww2_core::match_aviation::{AviationError, MissionKind, PlaneMission};
    let f = fixture();
    let mut m = scene(&f["traces"][0]);
    let before = snapshot(&m, &[]);
    let area = PlaneMission {
        kind: MissionKind::Area,
        target: None,
        at: Point { x: 40.0, z: 40.0 },
    };
    assert!(matches!(
        m.send_aircraft_mission(0, 1000, area),
        Err(AviationError::Unsupported(_))
    ));
    assert_eq!(snapshot(&m, &[]), before);
    m.commands.units.get_mut(&1000).unwrap().hold_fire = false;
    let before = snapshot(&m, &[]);
    let patrol = PlaneMission {
        kind: MissionKind::Patrol,
        ..area
    };
    assert!(matches!(
        m.send_aircraft_mission(0, 1000, patrol),
        Err(AviationError::Unsupported(_))
    ));
    assert_eq!(snapshot(&m, &[]), before);
    let trace = f["traces"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "aa-killed-later-fighter-must-skip")
        .unwrap();
    let mut m = scene(trace);
    m.combat.get_mut(1001).unwrap().xp = -1.0;
    let before = snapshot(&m, &[]);
    assert!(matches!(m.step_aircraft_anti_air(&[1000,1001],.05),Err(AviationError::Invalid(_))));
    assert_eq!(snapshot(&m, &[]), before);
    m.combat.get_mut(1001).unwrap().xp = 0.0;
    m.aviation.planes.remove(&1001);
    let before = format!("{:?}", m.world.game.units);
    assert!(
        matches!(m.step_aircraft_anti_air(&[1000,1001],.05),Err(AviationError::Unsupported(_)))
    );
    assert_eq!(format!("{:?}", m.world.game.units), before);
    assert!(m.aviation.events.is_empty());
}
