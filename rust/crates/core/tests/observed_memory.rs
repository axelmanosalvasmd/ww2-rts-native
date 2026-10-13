use serde_json::{Value, json};
use std::collections::BTreeMap;
use ww2_core::{
    map::AuthoritativeMap,
    observed_memory::{
        FootprintFact, ObservationFacts, ObservationFrame, ObservedMemory, ObservedRow,
    },
    route_knowledge::{InitialTerrain, RouteKnowledge},
    rules::Rules,
    world::{CellChange, Recipient, World},
};
fn idx(v: &Value) -> usize {
    v.as_u64().unwrap() as usize
}
fn cells(v: &Value) -> Vec<usize> {
    v.as_array().unwrap().iter().map(idx).collect()
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
#[derive(Clone)]
struct Footprint {
    identity: u64,
    owner: usize,
    live: bool,
    visible: Vec<usize>,
    origin: usize,
}
#[derive(Default)]
struct Facts {
    visible: [Vec<usize>; 2],
    teams: [i32; 2],
    mines: BTreeMap<usize, i32>,
    seen: BTreeMap<usize, u32>,
    footprints: BTreeMap<usize, Footprint>,
}
impl ObservationFacts for Facts {
    fn cell_visible(&mut self, c: usize, team: i32) -> bool {
        self.visible[team as usize].contains(&c)
    }
    fn mine_known(&mut self, c: usize, team: i32) -> bool {
        self.mine_owned(c, team) || ((self.seen.get(&c).copied().unwrap_or(0) >> team) & 1) == 1
    }
    fn mine_owned(&mut self, c: usize, team: i32) -> bool {
        self.mines
            .get(&c)
            .is_some_and(|&owner| owner >= 0 && self.teams[owner as usize] == team)
    }
    fn footprint(&mut self, c: usize, slot: usize, team: i32) -> Option<FootprintFact> {
        self.footprints.get(&c).map(|f| FootprintFact {
            identity: f.identity,
            allied: self.teams[f.owner] == team,
            live: f.live,
            visible: f.visible.contains(&slot),
            ruin_visible: self.visible[team as usize].contains(&f.origin),
        })
    }
}
fn route(w: &World, id: u64) -> RouteKnowledge {
    let i = w.initial();
    let shoal = w
        .shoal
        .as_ref()
        .map(|v| v.iter().map(|&v| v != 0).collect::<Vec<_>>());
    RouteKnowledge::observed(
        InitialTerrain {
            width: w.game.w,
            height: w.game.h,
            chars: &i.chars,
            ground: Some(&i.ground),
            objects: Some(&i.objects),
            heights: i.height.as_deref(),
            shoals: shoal.as_deref(),
            house_kinds: Some(&w.game.house),
            toughness: w.game.tough.as_deref(),
            trench_front: i.trench_front.as_deref(),
        },
        id,
        Rules::compiled().unwrap(),
    )
    .unwrap()
}
#[test]
fn original_collectors_preserve_filtered_memory_outbox_and_route_order() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/observed-memory.json")).unwrap();
    assert_eq!(
        fixture["reference"],
        "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
    );
    assert_eq!(fixture["node"], "v24.21.0");
    assert_eq!(fixture["arch"], "x64");
    let mut total = 0;
    for case in fixture["cases"].as_array().unwrap() {
        let map = AuthoritativeMap::from_json(&serde_json::to_vec(&case["map"]).unwrap()).unwrap();
        let mut w = World::from_map(&map).unwrap();
        w.game.vision_tick = Some(0);
        let mut memory: [ObservedMemory; 2] =
            std::array::from_fn(|slot| ObservedMemory::new(&w, slot, slot as u64).unwrap());
        let mut routes: [RouteKnowledge; 2] = std::array::from_fn(|slot| route(&w, slot as u64));
        let mut keys = [None; 2];
        let mut facts = Facts {
            teams: [0, 1],
            ..Facts::default()
        };
        let mut skip_fog = false;
        for (at, (a, s)) in case["actions"]
            .as_array()
            .unwrap()
            .iter()
            .zip(case["snapshots"].as_array().unwrap())
            .enumerate()
        {
            let mut result = Value::Null;
            match a["op"].as_str().unwrap() {
                "set" => {
                    w.set_cell(idx(&a["c"]), a["ch"].as_str().unwrap().as_bytes()[0])
                        .unwrap();
                }
                "mutate" => {
                    let v = &a["change"];
                    w.mutate_cell(
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
                "vision" => {
                    w.game.vision_tick = Some(a["tick"].as_i64().unwrap());
                    facts.visible[idx(&a["team"])] = cells(&a["cells"]);
                }
                "tick" => w.game.tick = a["tick"].as_i64().unwrap(),
                "log" => {
                    w.log_cell(idx(&a["c"]), a["look"].as_bool().unwrap_or(false))
                        .unwrap();
                }
                "mine" => {
                    let c = idx(&a["c"]);
                    if let Some(v) = a["owner"].as_i64() {
                        w.mines.insert(c, v as i32);
                        facts.mines.insert(c, v as i32);
                    }
                    if let Some(v) = a["seen"].as_u64() {
                        w.mine_seen.insert(c, v as u32);
                        facts.seen.insert(c, v as u32);
                    }
                }
                "footprint" => {
                    let cs = cells(&a["cells"]);
                    let f = Footprint {
                        identity: a["identity"].as_u64().unwrap(),
                        owner: idx(&a["owner"]),
                        live: a["live"].as_bool().unwrap(),
                        visible: cells(&a["visibleSlots"]),
                        origin: cs[0],
                    };
                    for c in cs {
                        facts.footprints.insert(c, f.clone());
                    }
                }
                "clearFootprint" => {
                    for c in cells(&a["cells"]) {
                        facts.footprints.remove(&c);
                    }
                }
                "reveal" => w.game.reveal = a["value"].as_bool().unwrap(),
                "skipFog" => skip_fog = a["value"].as_bool().unwrap(),
                "team" => facts.teams[idx(&a["slot"])] = a["team"].as_i64().unwrap() as i32,
                "route" | "read" | "reconnect" => {
                    let slot = idx(&a["slot"]);
                    let op = a["op"].as_str().unwrap();
                    let key = (w.game.tick, w.game.terrain_version, w.game.vision_tick);
                    if op != "route" || keys[slot] != Some(key) {
                        memory[slot]
                            .refresh(
                                &mut w,
                                ObservationFrame {
                                    team: facts.teams[slot],
                                    skip_fog,
                                },
                                &mut facts,
                            )
                            .unwrap();
                        if op == "route" {
                            memory[slot].apply_routes(&mut routes[slot]).unwrap();
                            keys[slot] = Some(key);
                        }
                    }
                    if op == "route" {
                        let r = &routes[slot];
                        let q = r.query();
                        let n = q.navigation();
                        let cs: Vec<_> = (0..w.game.w * w.game.h)
                            .map(|c| r.cell(c).unwrap())
                            .collect();
                        result = json!({"chars":cs.iter().map(|v|char::from(v.character)).collect::<String>(),"flags":n.flags,"height":n.heights.unwrap(),"mines":cs.iter().enumerate().filter_map(|(c,v)|v.known_mine.then_some(c)).collect::<Vec<_>>(),"version":n.versions.knowledge,"changes":n.changes.iter().map(|v|json!([v.from,v.to,v.cells])).collect::<Vec<_>>(),"hp":cs.iter().map(|v|format!("{:016x}",f64::from(v.cell_hp.unwrap()).to_bits())).collect::<Vec<_>>()});
                    } else if op == "reconnect" || a["full"].as_bool().unwrap() {
                        result =
                            json!(memory[slot].full_rows().iter().map(row).collect::<Vec<_>>());
                    } else {
                        result = json!(
                            memory[slot]
                                .take_changes()
                                .iter()
                                .map(row)
                                .collect::<Vec<_>>()
                        );
                    }
                }
                op => panic!("unknown {op}"),
            }
            let label = format!("{} action {at} {}", case["name"], a["op"]);
            assert!(
                equal(&result, &s["result"]),
                "{label} result\nactual {result}\nexpected {}",
                s["result"]
            );
            for (slot, m) in memory.iter().enumerate() {
                let expected = &s["memories"][slot];
                let rows = json!(
                    m.full_rows()
                        .iter()
                        .map(|r| json!({"row":row(r),"revision":r.revision}))
                        .collect::<Vec<_>>()
                );
                assert!(
                    equal(&rows, &expected["rows"]),
                    "{label} slot{slot} memory\nactual {rows}\nexpected {}",
                    expected["rows"]
                );
                let p = json!(
                    w.pending(Recipient::Player(slot))
                        .map(|v| v.iter().copied().collect::<Vec<_>>())
                        .unwrap_or_default()
                );
                assert_eq!(p, expected["pending"], "{label} slot{slot} pending");
            }
            assert_eq!(
                json!([
                    w.game.tick,
                    i64::from(w.game.terrain_version),
                    w.game.vision_tick.unwrap()
                ]),
                s["versions"],
                "{label} versions"
            );
            total += 1;
        }
    }
    assert_eq!(total, 117);
}

#[test]
fn route_forwarding_rejects_different_recipient_and_mode() {
    use ww2_core::observed_memory::ObservedError;
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/observed-memory.json")).unwrap();
    let map =
        AuthoritativeMap::from_json(&serde_json::to_vec(&fixture["cases"][0]["map"]).unwrap())
            .unwrap();
    let world = World::from_map(&map).unwrap();
    let memory = ObservedMemory::new(&world, 0, 7).unwrap();
    let mut other = route(&world, 8);
    assert_eq!(
        memory.apply_routes(&mut other),
        Err(ObservedError::RecipientMismatch)
    );
    let mut wrong_mode = RouteKnowledge::world(20, 20, 7, Rules::compiled().unwrap()).unwrap();
    assert_eq!(
        memory.apply_routes(&mut wrong_mode),
        Err(ObservedError::RecipientMismatch)
    );
    let small = [b'.'; 9];
    let mut wrong_shape = RouteKnowledge::observed(
        InitialTerrain {
            width: 3,
            height: 3,
            chars: &small,
            ground: None,
            objects: None,
            heights: None,
            shoals: None,
            house_kinds: None,
            toughness: None,
            trench_front: None,
        },
        7,
        Rules::compiled().unwrap(),
    )
    .unwrap();
    assert_eq!(
        memory.apply_routes(&mut wrong_shape),
        Err(ObservedError::RecipientMismatch)
    );
    let mut own = route(&world, 7);
    memory.apply_routes(&mut own).unwrap();
    assert_eq!(own.query().navigation().versions.knowledge, 0);
}
