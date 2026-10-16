use serde_json::{Value, json};
use std::collections::BTreeMap;
use ww2_core::{
    catalog::Catalog,
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::{LOCAL_DEFENDER_OWNER, Point, Wreck},
    map::AuthoritativeMap,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    random::MatchRandom,
    unit_spawn,
    weather::WeatherChoice,
    world::CellChange,
    world_memory::{KnownRegion, WorldRow},
};
fn bits(n: f64) -> Value {
    json!(format!("{:016x}", n.to_bits()))
}
fn hash(v: &Value) -> Value {
    let mut h = 0xcbf29ce484222325u64;
    for b in serde_json::to_vec(v).unwrap() {
        h = (h ^ u64::from(b)).wrapping_mul(0x100000001b3);
    }
    json!(format!("{h:016x}"))
}
fn idx(v: &Value) -> usize {
    v.as_u64().unwrap() as usize
}
fn owner(v: &Value) -> usize {
    if v.as_i64() == Some(-1) {
        LOCAL_DEFENDER_OWNER
    } else {
        idx(v)
    }
}
fn projected(r: &WorldRow) -> Value {
    let x = &r.observed.row;
    let d = &x.data;
    json!([
        x.cell,
        char::from(x.ch).to_string(),
        x.height,
        x.state,
        char::from(d.ground).to_string(),
        char::from(d.object).to_string(),
        d.material,
        d.ground_material,
        bits(f64::from(d.ground_wear)),
        d.trench_front.map(|v| bits(f64::from(v))),
        d.mine,
        d.mine_material,
        d.section.as_ref().map(|s| json!([
            s.id,
            s.structure_id,
            s.state,
            bits(s.hp),
            bits(s.max_hp),
            s.material,
            s.anchor
        ])),
        r.observed.mine_owned,
        [char::from(r.initial.0).to_string(), r.initial.1],
        r.biome,
        r.landmark,
        d.rail,
        d.station,
        r.observed.revision
    ])
}
fn rows(rs: &[WorldRow]) -> Value {
    json!({"count":rs.len(),"hash":hash(&json!(rs.iter().map(projected).collect::<Vec<_>>()))})
}
fn wrecks(rs: &[Wreck]) -> Value {
    json!(
        rs.iter()
            .map(|w| json!([w.id, bits(w.x), bits(w.z), w.moving]))
            .collect::<Vec<_>>()
    )
}
fn regions(rs: &[KnownRegion]) -> Value {
    json!(
        rs.iter()
            .map(|r| json!([
                r.id,
                r.name,
                r.kind,
                r.team,
                bits(r.at.x),
                bits(r.at.z),
                r.bounds,
                r.runs,
                bits(r.progress),
                r.capper.map_or(-1, |c| c as i64),
                r.locked,
                r.contested
            ]))
            .collect::<Vec<_>>()
    )
}
fn equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => a.as_f64() == b.as_f64(),
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| equal(a, b))
        }
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len() && a.iter().all(|(k, a)| b.get(k).is_some_and(|b| equal(a, b)))
        }
        _ => a == b,
    }
}
fn options(c: &Value) -> MatchOptions {
    MatchOptions {
        seed: c["seed"].as_u64().unwrap(),
        mode: Mode::World,
        players: c["players"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| PlayerConfig {
                name: p["name"].as_str().unwrap().into(),
                team: p["team"].as_i64().unwrap() as i32,
                faction: Faction::try_from(p["faction"].as_u64().unwrap() as u8).unwrap(),
            })
            .collect(),
        shuffle: c["settings"]["shuffle"].as_bool().unwrap(),
        army: match c["settings"]["army"].as_str().unwrap() {
            "standard" => ArmySize::Standard,
            "large" => ArmySize::Large,
            "massive" => ArmySize::Massive,
            "endless" => ArmySize::Endless,
            _ => unreachable!(),
        },
        weather: WeatherChoice::from_reference(c["settings"]["weather"].as_str()),
        weather_seed: c["weatherSeed"].as_u64().map(|v| v as u32),
        living_weather: c["settings"]["living"].as_bool().unwrap(),
        map_key: None,
        tech: c["settings"]["tech"].as_bool().unwrap(),
        supply: true,
        logistics: false,
        sandbox: false,
    }
}
fn map(c: &Value) -> AuthoritativeMap {
    let mut m = c["map"].clone();
    if let Some(runs) = m["world"]["regionMapRuns"].as_array() {
        let values: Vec<_> = runs
            .iter()
            .flat_map(|r| std::iter::repeat_n(r[0].clone(), r[1].as_u64().unwrap() as usize))
            .collect();
        m["world"].as_object_mut().unwrap().remove("regionMapRuns");
        m["world"]["regionMap"] = json!(values);
    }
    AuthoritativeMap::from_json(&serde_json::to_vec(&m).unwrap()).unwrap()
}

fn load(index: usize, shape: Option<&Value>) -> Match {
    let starts: Value = serde_json::from_str(include_str!("fixtures/world-start.json")).unwrap();
    let mut c = starts["cases"][index].clone();
    if let Some(shape) = shape.filter(|s| !s.is_null()) {
        c["map"]["w"] = shape["w"].clone();
        c["map"]["h"] = shape["h"].clone();
        c["map"]["rows"] = json!(vec![".".repeat(idx(&shape["w"])); idx(&shape["h"])]);
        c["map"]["heights"] = json!(vec!["0".repeat(idx(&shape["w"])); idx(&shape["h"])]);
        c["map"]["world"]["regionMapRuns"] = shape["regionMapRuns"].clone();
        if let Some(mines) = shape["authoredMines"].as_array() {
            let w = idx(&shape["w"]);
            for mine in mines {
                let cell = idx(mine);
                let mut row = c["map"]["rows"][cell / w]
                    .as_str()
                    .unwrap()
                    .as_bytes()
                    .to_vec();
                row[cell % w] = b'N';
                c["map"]["rows"][cell / w] = String::from_utf8(row).unwrap().into();
            }
        }
    }
    let m = map(&c);
    let o = options(&c);
    Match::new_world(&m, o.clone(), MatchRandom::new(o.seed)).unwrap()
}
#[test]
fn original_world_memory_producers() {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/world-memory.json")).unwrap();
    assert_eq!(
        fixture["reference"],
        "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
    );
    assert_eq!(fixture["node"], "v24.21.0");
    assert_eq!(fixture["arch"], "x64");
    let mut count = 0;
    for c in fixture["cases"].as_array().unwrap() {
        let mut m = load(idx(&c["startIndex"]), Some(&c["mapShape"]));
        let mut aliases: BTreeMap<String, u32> = BTreeMap::new();
        for (at, (a, s)) in c["actions"]
            .as_array()
            .unwrap()
            .iter()
            .zip(c["snapshots"].as_array().unwrap())
            .enumerate()
        {
            let mut result = Value::Null;
            match a["op"].as_str().unwrap() {
                "clearUnits" => m.world.game.units.clear(),
                "spawn" => {
                    let id = idx(&a["id"]) as u32;
                    assert_eq!(id, m.next_unit_id);
                    m.next_unit_id += 1;
                    let kind = m.rules.kind(a["kind"].as_str().unwrap()).unwrap();
                    let p = Point {
                        x: a["x"].as_f64().unwrap(),
                        z: a["z"].as_f64().unwrap(),
                    };
                    let u = unit_spawn::initial_unit(
                        &m.rules,
                        Catalog::compiled().unwrap(),
                        kind,
                        id,
                        owner(&a["owner"]),
                        p,
                    )
                    .unwrap();
                    m.world.game.units.insert(id, u);
                    aliases.insert(a["alias"].as_str().unwrap().into(), id);
                }
                "move" => {
                    let u = m
                        .world
                        .game
                        .units
                        .get_mut(&aliases[a["alias"].as_str().unwrap()])
                        .unwrap();
                    u.x = a["x"].as_f64().unwrap();
                    u.z = a["z"].as_f64().unwrap();
                    if let Some(hp) = a["fields"]["hp"].as_f64() {
                        u.hp = hp;
                    }
                }
                "remove" => {
                    let u = m
                        .world
                        .game
                        .units
                        .remove(&aliases[a["alias"].as_str().unwrap()])
                        .unwrap();
                    m.retain_removed_world_route_unit(&u).unwrap();
                }
                "vision" => {
                    m.world.game.tick = a["tick"].as_i64().unwrap();
                    m.refresh_world_visibility().unwrap();
                }
                "tick" => m.world.game.tick = a["tick"].as_i64().unwrap(),
                "set" => {
                    m.world
                        .set_cell(idx(&a["c"]), a["ch"].as_str().unwrap().as_bytes()[0])
                        .unwrap();
                }
                "mutate" => {
                    m.world
                        .mutate_cell(
                            idx(&a["c"]),
                            CellChange {
                                mine: a["change"]["mine"].as_bool(),
                                ..CellChange::default()
                            },
                        )
                        .unwrap();
                }
                "mine" => {
                    if let Some(v) = a["owner"].as_i64() {
                        m.world.mines.insert(idx(&a["c"]), v as i32);
                    }
                    if let Some(v) = a["seen"].as_u64() {
                        m.world.mine_seen.insert(idx(&a["c"]), v as u32);
                    }
                }
                "log" => {
                    m.world
                        .log_cell(idx(&a["c"]), a["look"].as_bool().unwrap_or(false))
                        .unwrap();
                }
                "cycleChanges" => {
                    for i in 0..idx(&a["count"]) {
                        m.world
                            .set_cell(idx(&a["c"]), if i % 2 == 0 { b'T' } else { b'M' })
                            .unwrap();
                        m.refresh_world_memory(owner(&a["slot"])).unwrap();
                    }
                }
                "touchMany" => {
                    for _ in 0..idx(&a["count"]) {
                        m.world.touch(idx(&a["c"])).unwrap();
                    }
                }
                "reveal" => m.world.game.reveal = a["value"].as_bool().unwrap(),
                "wet" => m.world.game.wet = a["value"].as_f64(),
                "fire" => {
                    if a["value"].as_bool().unwrap() {
                        m.world.fires.insert(idx(&a["c"]), 20.);
                    } else {
                        m.world.fires.remove(&idx(&a["c"]));
                    }
                }
                "region" => {
                    let r = &mut m.world_runtime.as_mut().unwrap().regions[idx(&a["index"])];
                    let f = &a["fields"];
                    if let Some(v) = f["team"].as_i64() {
                        r.team = v as i32;
                    }
                    if let Some(v) = f["progress"].as_f64() {
                        r.progress = v;
                    }
                    if let Some(v) = f["capper"].as_i64() {
                        r.capper = (v >= 0).then_some(v as usize);
                    }
                    if let Some(v) = f["contested"].as_bool() {
                        r.contested = v;
                    }
                }
                "visible" => {
                    m.world.game.players[idx(&a["slot"])].visible = a["aliases"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|v| aliases[v.as_str().unwrap()])
                        .collect()
                }
                "wreck" => {
                    let w = Wreck {
                        id: Some(idx(&a["id"]) as u32),
                        cell: None,
                        x: a["x"].as_f64().unwrap(),
                        z: a["z"].as_f64().unwrap(),
                        moving: a["moving"].as_bool().unwrap_or(false),
                    };
                    if let Some(old) = m.world.game.wrecks.iter_mut().find(|old| old.id == w.id) {
                        *old = w;
                    } else {
                        m.world.game.wrecks.push(w);
                    }
                }
                "removeWreck" => m
                    .world
                    .game
                    .wrecks
                    .retain(|w| w.id != Some(idx(&a["id"]) as u32)),
                "read" => {
                    result = rows(
                        &m.world_terrain(owner(&a["slot"]), a["full"].as_bool().unwrap())
                            .unwrap(),
                    )
                }
                "regions" => result = regions(&m.world_regions(owner(&a["slot"])).unwrap()),
                "wreckRead" => result = wrecks(&m.world_wrecks(owner(&a["slot"])).unwrap()),
                "route" => {
                    let slot = owner(&a["slot"]);
                    m.refresh_world_routing(slot).unwrap();
                    let r = m.world_routing(slot).unwrap();
                    let query = r.query();
                    let nav = query.navigation();
                    let team = m.world_actor_view(slot).unwrap().team;
                    let known = m
                        .world_runtime
                        .as_ref()
                        .unwrap()
                        .memory
                        .team(team)
                        .unwrap()
                        .rows()
                        .iter()
                        .map(|r| {
                            let c = r.observed.row.cell;
                            query_cell(m.world_routing(slot).unwrap().knowledge(), c)
                        })
                        .collect::<Vec<_>>();
                    result = json!({"version":nav.versions.knowledge,"changes":nav.changes.iter().map(|c|json!([c.from,c.to,c.cells])).collect::<Vec<_>>(),"knownHash":hash(&json!(known)),"knownCount":known.len(),"fires":r.live().fires.iter().enumerate().filter_map(|(c,v)|v.then_some(c)).collect::<Vec<_>>(),"bunkers":r.live().bunkers.iter().map(|p|[bits(p.x),bits(p.z)]).collect::<Vec<_>>(),"wrecks":wrecks(r.wrecks()),"wet":r.live().wetness});
                }
                op => panic!("unknown {op}"),
            }
            m.grid.rebuild(&m.world.game.units);
            assert!(
                equal(&result, &s["result"]),
                "start{} action{at} {a}: result mismatch: {result} != {}",
                c["startIndex"],
                s["result"]
            );
            let teams = s["teams"]
                .as_array()
                .unwrap()
                .iter()
                .map(|t| {
                    let team = t[0].as_i64().unwrap() as i32;
                    let memory = m.world_runtime.as_ref().unwrap().memory.team(team).unwrap();
                    json!([
                        team,
                        memory.version(),
                        rows(memory.rows()),
                        memory.regions().len(),
                        wrecks(memory.remembered_wrecks())
                    ])
                })
                .collect::<Vec<_>>();
            assert!(
                equal(&json!(teams), &s["teams"]),
                "start{} action{at} {a}: memory mismatch {teams:?} != {}",
                c["startIndex"],
                s["teams"]
            );
            assert_eq!(
                json!([m.world.journal_base(), m.world.journal().len()]),
                s["journal"],
                "action{at} journal"
            );
            assert_eq!(
                json!([
                    m.world.game.tick,
                    m.world.game.terrain_version,
                    m.world.game.vision_tick
                ]),
                s["versions"]
            );
            assert_eq!(
                json!(
                    m.world
                        .game
                        .players
                        .iter()
                        .map(|p| &p.visible)
                        .collect::<Vec<_>>()
                ),
                s["visible"],
                "action{at} visibility"
            );
            assert_eq!(
                json!(m.world_runtime.as_ref().unwrap().local_defender.visible),
                s["localVisible"],
                "action{at} local vision"
            );
            count += 1;
        }
    }
    assert_eq!(count, 120);
}
fn query_cell(knowledge: &ww2_core::route_knowledge::RouteKnowledge, c: usize) -> Value {
    let v = knowledge.cell(c).unwrap();
    json!([
        c,
        char::from(v.character).to_string(),
        v.flags,
        v.height,
        bits(f64::from(v.cell_hp.unwrap())),
        bits(f64::from(v.trench_front))
    ])
}
#[test]
fn malformed_context_and_hidden_knowledge_are_atomic() {
    let mut m = load(1, None);
    m.world.game.units.clear();
    m.grid.rebuild(&m.world.game.units);
    m.refresh_world_visibility().unwrap();
    m.refresh_world_routing(0).unwrap();
    let team = m.world.game.players[0].team;
    assert!(
        m.world_runtime
            .as_ref()
            .unwrap()
            .memory
            .team(team)
            .unwrap()
            .rows()
            .is_empty()
    );
    let query = m.world_routing(0).unwrap();
    assert!(query.query().navigation().flags.iter().all(|f| *f == 0));
    assert!(!query.knowledge().cell(0).unwrap().known);
    assert!(m.world_terrain(LOCAL_DEFENDER_OWNER, false).is_err());
    assert!(m.world_regions(LOCAL_DEFENDER_OWNER).is_err());
    let journal = m.world.journal().to_vec();
    let base = m.world.journal_base();
    assert!(
        m.world
            .trim_journal_before(m.world.journal_end().unwrap() + 1)
            .is_err()
    );
    assert_eq!(m.world.journal(), journal);
    assert_eq!(m.world.journal_base(), base);
    m.world.game.cell_hp.pop();
    assert!(m.refresh_world_routing(0).is_err());
    assert!(m.world_terrain(0, false).is_err());
    assert_eq!(
        m.world_runtime
            .as_ref()
            .unwrap()
            .memory
            .team(team)
            .unwrap()
            .version(),
        0
    );
    assert_eq!(m.world.journal(), journal);
}

#[test]
fn map99_map100_map102_hidden_changes_memory_reconnect_and_node_station_filter() {
    let mut m=load(1,None);
    m.world.game.units.clear();m.grid.rebuild(&m.world.game.units);
    let at=Point{x:41.0,z:41.0};
    let id=m.next_unit_id;
    let eye=unit_spawn::initial_unit(&m.rules,Catalog::compiled().unwrap(),
        m.rules.kind("rifle").unwrap(),id,0,at).unwrap();
    m.next_unit_id+=1;m.world.game.units.insert(id,eye);m.grid.rebuild(&m.world.game.units);
    m.world.game.tick=1;m.refresh_world_visibility().unwrap();
    let c=m.world.game.cell_of(at.x,at.z).unwrap();
    let before=m.world_recipient_metadata(0,true).unwrap();
    let before_json=before.to_json();
    let remembered=before.geography.as_ref().unwrap().rows[c/m.world.game.w].as_bytes()[c%m.world.game.w];
    assert_ne!(remembered,b'B');
    // Leave the explored cell, then change its current terrain in the fog.
    let eye=m.world.game.units.get_mut(&id).unwrap();eye.x=201.0;eye.z=201.0;
    m.grid.rebuild(&m.world.game.units);m.world.game.tick=2;m.refresh_world_visibility().unwrap();
    let left=m.world_recipient_metadata(0,true).unwrap();
    m.world.set_cell(c,b'B').unwrap();
    let hidden=m.world_recipient_metadata(0,true).unwrap();
    assert_eq!(left.to_json(),hidden.to_json(),"a hidden change alters no recipient metadata");
    let known=hidden.geography.as_ref().unwrap();
    assert_eq!(known.rows[c/m.world.game.w].as_bytes()[c%m.world.game.w],remembered);
    for (slot,spawn) in known.spawns.iter().enumerate() {
        assert_eq!(spawn.is_some(),m.world.game.allied(slot,0));
    }
    let team=m.world.game.players[0].team;
    let memory=m.world_runtime.as_ref().unwrap().memory.team(team).unwrap();
    let runtime=m.world_runtime.as_ref().unwrap();
    assert_eq!(hidden.lists.nodes.as_ref().unwrap().len(),runtime.nodes.iter().filter(|n|memory.contains_cell(n.cell)).count());
    assert_eq!(hidden.world.stations.as_ref().map_or(0,Vec::len),runtime.rails.stations.iter().filter(|s|memory.contains_cell(s.cell)).count());
    let unseen=(0..m.world.game.chars.len()).find(|&cell|!memory.contains_cell(cell)).unwrap();
    assert_eq!(known.rows[unseen/m.world.game.w].as_bytes()[unseen%m.world.game.w],b'.');
    assert_eq!(known.heights[unseen/m.world.game.w].as_bytes()[unseen%m.world.game.w],b'0');
    let json=hidden.to_json();
    assert!(json["map"]["world"].get("seed").is_none());
    assert!(json["map"]["points"].as_array().unwrap().is_empty());
    // Reconnect receives the complete remembered map with the old cell. Returning
    // the observer accepts the current terrain rather than an injected answer.
    let eye=m.world.game.units.get_mut(&id).unwrap();eye.x=at.x;eye.z=at.z;
    m.grid.rebuild(&m.world.game.units);m.world.game.tick=3;m.refresh_world_visibility().unwrap();
    let seen=m.world_recipient_metadata(0,true).unwrap();
    assert_eq!(seen.geography.as_ref().unwrap().rows[c/m.world.game.w].as_bytes()[c%m.world.game.w],b'B');
    assert_ne!(before_json,seen.to_json());
}
