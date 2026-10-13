use serde_json::{Value, json};
use std::collections::BTreeMap;
use ww2_core::{
    catalog::Catalog,
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::{PlaneState, Point, Smoke, Wreck},
    map::AuthoritativeMap,
    match_runtime::{BuildingCell, Match, MatchOptions, PlayerConfig},
    observed_memory::ObservedRow,
    pathfinding::{PathRules, Pathfinding},
    unit_spawn, vision,
    weather::WeatherChoice,
    world::{CellChange, Recipient},
};
fn idx(v: &Value) -> usize {
    v.as_u64().unwrap() as usize
}
fn options() -> MatchOptions {
    MatchOptions {
        seed: 17,
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
        weather_seed: None,
        living_weather: true,
        map_key: None,
        tech: false,
        supply: false,
        logistics: false,
        sandbox: false,
    }
}
fn bits(v: f64) -> String {
    format!("{:016x}", v.to_bits())
}
fn runs(values: impl IntoIterator<Item = Value>) -> Value {
    let mut out: Vec<[Value; 2]> = Vec::new();
    for v in values {
        if let Some(last) = out.last_mut().filter(|r| r[0] == v) {
            last[1] = json!(last[1].as_u64().unwrap() + 1)
        } else {
            out.push([v, json!(1)])
        }
    }
    json!(out)
}
fn row(r: &ObservedRow) -> Value {
    let x = &r.row;
    let d = &x.data;
    let mut data = json!({"ground":char::from(d.ground).to_string(),"object":char::from(d.object).to_string(),"material":d.material,"groundMaterial":d.ground_material,"groundWear":d.ground_wear});
    if let Some(v) = d.trench_front {
        data["trenchFront"] = json!(v);
    }
    if d.mine {
        data["mine"] = json!(true);
        if let Some(v) = &d.mine_material {
            data["mineMaterial"] = json!(v);
        }
    }
    if let Some(v) = r.mine_owned {
        data["mineOwned"] = json!(v);
    }
    if let Some(s) = &d.section {
        data["section"] = json!({"id":s.id,"structureId":s.structure_id,"state":s.state,"hp":s.hp,"maxHp":s.max_hp,"material":s.material,"anchor":s.anchor});
    }
    json!([
        x.cell,
        char::from(x.ch).to_string(),
        x.height,
        x.state,
        data
    ])
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

fn wrecks(rows: &[Wreck]) -> Value {
    json!(
        rows.iter()
            .map(|w| json!([w.id.unwrap(), bits(w.x), bits(w.z), w.moving]))
            .collect::<Vec<_>>()
    )
}
fn output(instance: &Match, slot: usize) -> Value {
    let state = instance.recipient_routing(slot).unwrap();
    let r = state.knowledge();
    let q = state.query();
    let nav = q.navigation();
    let live = state.live();
    let n = instance.world.game.w * instance.world.game.h;
    json!({"wetness":live.wetness,"wrecks":wrecks(state.wrecks()),"chars":(0..n).map(|c|char::from(r.cell(c).unwrap().character)).collect::<String>(),"flags":runs(nav.flags.iter().map(|v|json!(v))),"height":runs(nav.heights.unwrap().iter().map(|v|json!(v))),"hp":runs((0..n).map(|c|json!(bits(f64::from(r.cell(c).unwrap().cell_hp.unwrap()))))),"version":nav.versions.knowledge,"changes":nav.changes.iter().map(|c|json!([c.from,c.to,c.cells])).collect::<Vec<_>>(),"fires":live.fires.iter().enumerate().filter_map(|(c,v)|v.then_some(c)).collect::<Vec<_>>(),"bunkers":live.bunkers.iter().map(|p|[bits(p.x),bits(p.z)]).collect::<Vec<_>>()})
}
#[test]
fn actual_match_visibility_memory_and_owned_routing_match_source() {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/match-routing.json")).unwrap();
    assert_eq!(
        fixture["reference"],
        "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
    );
    assert_eq!(fixture["node"], "v24.21.0");
    assert_eq!(fixture["arch"], "x64");
    let mut count = 0;
    for case in fixture["cases"].as_array().unwrap() {
        let map = AuthoritativeMap::from_json(&serde_json::to_vec(&case["map"]).unwrap()).unwrap();
        let mut instance = Match::new(&map, options()).unwrap();
        let mut aliases = BTreeMap::new();
        let mut finders: [Pathfinding; 2] = std::array::from_fn(|_| Pathfinding::new());
        for (at, (a, s)) in case["actions"]
            .as_array()
            .unwrap()
            .iter()
            .zip(case["snapshots"].as_array().unwrap())
            .enumerate()
        {
            let mut result = Value::Null;
            match a["op"].as_str().unwrap() {
                "clearUnits" => instance.world.game.units.clear(),
                "spawn" => {
                    let id = a["id"].as_u64().unwrap() as u32;
                    assert_eq!(id, instance.next_unit_id);
                    instance.next_unit_id += 1;
                    let owner = idx(&a["owner"]);
                    let kind = instance.rules.kind(a["kind"].as_str().unwrap()).unwrap();
                    let base = if instance.rules.kinds[kind].air {
                        instance.rules.kind("rifle").unwrap()
                    } else {
                        kind
                    };
                    let point = Point {
                        x: a["x"].as_f64().unwrap(),
                        z: a["z"].as_f64().unwrap(),
                    };
                    let mut u = unit_spawn::initial_unit(
                        &instance.rules,
                        Catalog::compiled().unwrap(),
                        base,
                        id,
                        owner,
                        point,
                    )
                    .unwrap();
                    u.kind = kind;
                    if let Some(v) = a["air"].as_str() {
                        u.air = Some(match v {
                            "base" => PlaneState::Base,
                            "station" => PlaneState::Station,
                            _ => panic!("unknown air"),
                        });
                    }
                    if let Some(v) = a["fields"]["built"].as_f64() {
                        u.built = Some(v);
                    }
                    if let Some(v) = a["fields"]["hp"].as_f64() {
                        u.hp = v;
                    }
                    instance.world.game.units.insert(id, u);
                    aliases.insert(a["alias"].as_str().unwrap().to_owned(), id);
                }
                "move" => {
                    let id = aliases[a["alias"].as_str().unwrap()];
                    let u = instance.world.game.units.get_mut(&id).unwrap();
                    u.x = a["x"].as_f64().unwrap();
                    u.z = a["z"].as_f64().unwrap();
                    if let Some(v) = a["fields"]["built"].as_f64() {
                        u.built = Some(v);
                    }
                    if let Some(v) = a["fields"]["hp"].as_f64() {
                        u.hp = v;
                    }
                }
                "remove" => {
                    let unit = instance
                        .world
                        .game
                        .units
                        .remove(&aliases[a["alias"].as_str().unwrap()])
                        .unwrap();
                    instance.retain_removed_route_unit(&unit).unwrap();
                }
                "set" => {
                    instance
                        .world
                        .set_cell(idx(&a["c"]), a["ch"].as_str().unwrap().as_bytes()[0])
                        .unwrap();
                }
                "mutate" => {
                    let v = &a["change"];
                    instance
                        .world
                        .mutate_cell(
                            idx(&a["c"]),
                            CellChange {
                                ground: v["ground"].as_str().map(|v| v.as_bytes()[0]),
                                object: v["object"].as_str().map(|v| v.as_bytes()[0]),
                                mine: v["mine"].as_bool(),
                                material: v["material"].as_str().map(str::to_owned),
                                height: v["height"].as_i64().map(|v| v as i8),
                            },
                        )
                        .unwrap();
                }
                "log" => {
                    instance
                        .world
                        .log_cell(idx(&a["c"]), a["look"].as_bool().unwrap_or(false))
                        .unwrap();
                }
                "vision" => {
                    instance.world.game.tick = a["tick"].as_i64().unwrap();
                    instance.grid.rebuild(&instance.world.game.units);
                    vision::update_vision(
                        &mut instance.world.game,
                        &instance.rules,
                        &instance.grid,
                    );
                }
                "tick" => instance.world.game.tick = a["tick"].as_i64().unwrap(),
                "fire" => {
                    let c = idx(&a["c"]);
                    if a["value"].as_bool().unwrap() {
                        instance.world.fires.insert(c, 20.0);
                    } else {
                        instance.world.fires.remove(&c);
                    }
                }
                "smoke" => {
                    instance.world.game.smokes = a["values"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|v| Smoke {
                            id: 0,
                            x: v["x"].as_f64().unwrap(),
                            z: v["z"].as_f64().unwrap(),
                            r: v["r"].as_f64().unwrap(),
                        })
                        .collect()
                }
                "weather" => {
                    instance.world.game.weather = instance
                        .rules
                        .weather
                        .iter()
                        .position(|v| Some(v.name.as_str()) == a["name"].as_str())
                }
                "wet" => instance.world.game.wet = Some(a["value"].as_f64().unwrap()),
                "mine" => {
                    let c = idx(&a["c"]);
                    if let Some(v) = a["owner"].as_i64() {
                        instance.world.mines.insert(c, v as i32);
                    }
                    if let Some(v) = a["seen"].as_u64() {
                        instance.world.mine_seen.insert(c, v as u32);
                    }
                }
                "footprint" => {
                    let id = aliases[a["alias"].as_str().unwrap()];
                    let u = &instance.world.game.units[&id];
                    for c in a["cells"].as_array().unwrap().iter().map(idx) {
                        instance.building_cells.insert(
                            c,
                            BuildingCell {
                                unit: id,
                                owner: u.owner,
                                at: u.at(),
                            },
                        );
                    }
                }
                "clearFootprint" => {
                    for c in a["cells"].as_array().unwrap().iter().map(idx) {
                        instance.building_cells.remove(&c);
                    }
                }
                "visible" => {
                    let mut ids: Vec<_> = a["aliases"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|k| aliases[k.as_str().unwrap()])
                        .collect();
                    ids.sort_unstable();
                    instance.world.game.players[idx(&a["slot"])].visible = ids;
                }
                "team" => {
                    instance.world.game.players[idx(&a["slot"])].team =
                        a["team"].as_i64().unwrap() as i32
                }
                "reveal" => instance.world.game.reveal = a["value"].as_bool().unwrap(),
                "wreck" => {
                    let w = Wreck {
                        id: Some(a["id"].as_u64().unwrap() as u32),
                        cell: None,
                        x: a["x"].as_f64().unwrap(),
                        z: a["z"].as_f64().unwrap(),
                        moving: a["moving"].as_bool().unwrap(),
                    };
                    if let Some(old) = instance.world.game.wrecks.iter_mut().find(|v| v.id == w.id)
                    {
                        *old = w;
                    } else {
                        instance.world.game.wrecks.push(w);
                    }
                }
                "removeWreck" => instance
                    .world
                    .game
                    .wrecks
                    .retain(|w| w.id != Some(a["id"].as_u64().unwrap() as u32)),
                "wreckRead" => result = wrecks(&instance.observed_wrecks(idx(&a["slot"])).unwrap()),
                "read" => {
                    result = json!(
                        instance
                            .observed_terrain(idx(&a["slot"]), a["full"].as_bool().unwrap())
                            .unwrap()
                            .iter()
                            .map(row)
                            .collect::<Vec<_>>()
                    )
                }
                "route" => {
                    let slot = idx(&a["slot"]);
                    instance.refresh_recipient_routing(slot).unwrap();
                    result = output(&instance, slot);
                    if !a["path"].is_null() {
                        let p = &a["path"];
                        let unit =
                            &instance.world.game.units[&aliases[p["alias"].as_str().unwrap()]];
                        let state = instance.recipient_routing(slot).unwrap();
                        let query = state.query();
                        let path = finders[slot]
                            .find_path(
                                query.path_terrain(state.live()).unwrap(),
                                PathRules::compiled().unwrap(),
                                &instance.rules.kinds[unit.kind],
                                unit.at(),
                                Point {
                                    x: p["goal"]["x"].as_f64().unwrap(),
                                    z: p["goal"]["z"].as_f64().unwrap(),
                                },
                                None,
                            )
                            .unwrap();
                        result["expansions"] = json!(path.expansions);
                        result["path"] = json!(
                            path.waypoints
                                .iter()
                                .map(|p| [bits(p.x), bits(p.z)])
                                .collect::<Vec<_>>()
                        );
                        result["end"] = json!([bits(path.route_end.x), bits(path.route_end.z)]);
                    }

                    if !a["cover"].is_null() {
                        let at = &a["cover"]["at"];
                        let from = &a["cover"]["from"];
                        let state = instance.recipient_routing(slot).unwrap();
                        result["cover"] = json!(
                            state
                                .knowledge()
                                .cover_rank(
                                    Point {
                                        x: at["x"].as_f64().unwrap(),
                                        z: at["z"].as_f64().unwrap()
                                    },
                                    Some(Point {
                                        x: from["x"].as_f64().unwrap(),
                                        z: from["z"].as_f64().unwrap()
                                    }),
                                    state.wrecks()
                                )
                                .unwrap()
                        );
                    }
                    if case["name"] == "current-position-discovery-and-cached-fire-fog" && at == 7 {
                        assert!(
                            !instance
                                .recipient_sees(0, Point { x: 15.0, z: 11.0 })
                                .unwrap()
                        );
                        let state = instance.recipient_routing(0).unwrap();
                        assert_eq!(state.knowledge().cell(207).unwrap().character, b'.');
                        assert!(state.live().fires[207]);
                    }
                }
                v => panic!("unknown {v}"),
            }
            let label = format!("{} action{at} {}", case["name"], a["op"]);
            assert!(
                equal(&result, &s["result"]),
                "{label} result\nactual {result}\nexpected {}",
                s["result"]
            );
            for slot in 0..2 {
                let state = instance.recipient_routing(slot).unwrap();
                let rows = json!(
                    state
                        .remembered()
                        .iter()
                        .map(|r| json!({"row":row(r),"revision":r.revision}))
                        .collect::<Vec<_>>()
                );
                assert!(
                    equal(&rows, &s["memories"][slot]["rows"]),
                    "{label} slot{slot} memory\nactual {rows}\nexpected {}",
                    s["memories"][slot]["rows"]
                );
                assert_eq!(
                    wrecks(state.remembered_wrecks()),
                    s["memories"][slot]["wrecks"],
                    "{label} slot{slot} wreck memory"
                );
                assert_eq!(
                    json!(instance.world.pending(Recipient::Player(slot)).unwrap()),
                    s["memories"][slot]["pending"],
                    "{label} slot{slot} pending"
                );
            }
            assert_eq!(
                json!(
                    instance
                        .world
                        .game
                        .players
                        .iter()
                        .map(|p| &p.visible)
                        .collect::<Vec<_>>()
                ),
                s["visible"],
                "{label} visible"
            );
            assert_eq!(
                json!([
                    Some(instance.world.game.tick),
                    Some(i64::from(instance.world.game.terrain_version)),
                    instance.world.game.vision_tick
                ]),
                s["versions"],
                "{label} versions"
            );
            count += 1;
        }
    }
    assert_eq!(count, 162);
}

#[test]
fn malformed_context_cannot_change_owned_memory_routes_or_pending() {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/match-routing.json")).unwrap();
    let map =
        AuthoritativeMap::from_json(&serde_json::to_vec(&fixture["cases"][0]["map"]).unwrap())
            .unwrap();
    let mut m = Match::new(&map, options()).unwrap();
    m.refresh_recipient_routing(0).unwrap();
    m.world.set_cell(20, b'T').unwrap();
    let before = output(&m, 0);
    let rows = m.recipient_routing(0).unwrap().remembered().to_vec();
    let pending = m.world.pending(Recipient::Player(0)).unwrap().clone();
    assert!(m.refresh_recipient_routing(99).is_err());
    let flag = m.world.game.flags.pop().unwrap();
    assert!(m.refresh_recipient_routing(0).is_err());
    m.world.game.flags.push(flag);
    m.world.game.players[0].team = -1;
    assert!(m.refresh_recipient_routing(0).is_err());
    m.world.game.players[0].team = 0;
    let kind = m.world.game.units.first_entry().unwrap().get().kind;
    m.world.game.units.first_entry().unwrap().get_mut().kind = usize::MAX;
    assert!(m.refresh_recipient_routing(0).is_err());
    m.world.game.units.first_entry().unwrap().get_mut().kind = kind;
    m.world.game.players[0].slot = 1;
    assert!(m.refresh_recipient_routing(0).is_err());
    m.world.game.players[0].slot = 0;
    m.world.game.wrecks.push(Wreck {
        id: None,
        cell: None,
        x: 5.0,
        z: 5.0,
        moving: false,
    });
    assert!(m.observed_wrecks(0).is_err());
    assert!(m.refresh_recipient_routing(0).is_err());
    m.world.game.wrecks[0].id = Some(77);
    m.world.game.wrecks.push(m.world.game.wrecks[0].clone());
    assert!(m.observed_wrecks(0).is_err());
    m.world.game.wrecks.clear();
    m.world.game.mode = Some(ww2_core::game::ModeKind::World);
    assert!(m.refresh_recipient_routing(0).is_err());
    m.world.game.mode = None;
    let units = std::mem::take(&mut m.world.game.units);
    let kinds = std::mem::take(&mut m.rules.kinds);
    assert!(m.refresh_recipient_routing(0).is_err());
    m.rules.kinds = kinds;
    m.world.game.units = units;
    assert_eq!(output(&m, 0), before);
    assert_eq!(m.recipient_routing(0).unwrap().remembered(), rows);
    assert_eq!(m.world.pending(Recipient::Player(0)), Some(&pending));
}
