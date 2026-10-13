use serde_json::{Value, json};
use ww2_core::{
    game::Wreck,
    map::AuthoritativeMap,
    world::{Appearance, CellChange, Recipient, World},
};
fn runs(values: impl IntoIterator<Item = Value>) -> Value {
    let mut out: Vec<[Value; 2]> = Vec::new();
    for v in values {
        if let Some(last) = out.last_mut().filter(|r| r[0] == v) {
            last[1] = json!(last[1].as_u64().unwrap() + 1);
        } else {
            out.push([v, json!(1)]);
        }
    }
    json!(out)
}
fn floats(v: &[f32]) -> Value {
    runs(v.iter().map(|v| json!(v.to_bits())))
}
fn map<T>(v: &std::collections::BTreeMap<usize, T>, f: impl Fn(&T) -> Value) -> Value {
    json!(v.iter().map(|(c, v)| json!([c, f(v)])).collect::<Vec<_>>())
}
fn row(r: &ww2_core::world::CellRow) -> Value {
    let d = &r.data;
    let mut data = json!({"ground":char::from(d.ground).to_string(),"object":char::from(d.object).to_string(),"material":d.material,"groundMaterial":d.ground_material,"groundWear":d.ground_wear});
    if let Some(front) = d.trench_front {
        data["trenchFront"] = json!(front);
    }
    if d.mine {
        data["mine"] = json!(true);
        if let Some(m) = &d.mine_material {
            data["mineMaterial"] = json!(m);
        }
    }
    if let Some(s) = &d.section {
        data["section"] = json!({"id":s.id,"structureId":s.structure_id,"state":s.state,"hp":s.hp,"maxHp":s.max_hp,"material":s.material,"anchor":s.anchor});
    }
    json!([
        r.cell,
        char::from(r.ch).to_string(),
        r.height,
        r.state,
        data
    ])
}
fn snapshot(w: &World) -> Value {
    let g = &w.game;
    let structures: Vec<_> = w
        .structures
        .iter()
        .map(|s| {
            json!([
                s.id,
                s.kind,
                s.entity_id,
                s.section_ids
                    .iter()
                    .zip(&s.cells)
                    .map(|(id, c)| {
                        let (m, h) = w.section_by_id(id).unwrap();
                        json!([
                            m.id,
                            m.structure_id,
                            c,
                            h.hp,
                            h.max_hp.unwrap(),
                            m.material,
                            m.anchor,
                            m.supports,
                            if h.failed {
                                "failed"
                            } else {
                                match m.appearance {
                                    Appearance::Intact => "intact",
                                    Appearance::Damaged => "damaged",
                                    Appearance::Burning => "burning",
                                }
                            },
                            m.rubble
                        ])
                    })
                    .collect::<Vec<_>>()
            ])
        })
        .collect();
    let pending: Vec<_> = g
        .players
        .iter()
        .enumerate()
        .map(|(i, _)| json!([i, w.pending(Recipient::Player(i)).unwrap()]))
        .collect();
    let initial = w.initial();
    json!({"chars":String::from_utf8(g.chars.clone()).unwrap(),"ground":String::from_utf8(g.ground.clone().unwrap()).unwrap(),"objects":String::from_utf8(g.objects.clone().unwrap()).unwrap(),
    "mines":runs(w.mine_layer.iter().map(|v|json!(v))),"flags":runs(g.flags.iter().map(|v|json!(v))),"height":g.height.as_ref().map(|v|runs(v.iter().map(|v|json!(v)))),
    "hp":floats(&g.cell_hp),"wear":floats(g.wear.as_ref().unwrap()),"surfaceWear":floats(&w.surface_wear),"front":g.trench_front.as_ref().map(|v|floats(v)),"burnt":runs(w.burnt.iter().map(|v|json!(v))),"scar":floats(&w.scar),"state":runs(w.cell_state.iter().map(|v|json!(v))),"tough":g.tough.as_ref().map(|v|runs(v.iter().map(|v|json!(v)))),
    "materials":map(&w.materials,|v|json!(v)),"baseMaterials":map(&w.base_materials,|v|json!(v)),"objectMaterials":map(&w.object_materials,|v|json!(v)),"mineMaterials":map(&w.mine_materials,|v|json!(v)),
    "mineOwners":map(&w.mines,|v|json!(v)),"mineSeen":map(&w.mine_seen,|v|json!(v)),"aid":map(&w.aid,|v|json!(v)),"fires":map(&w.fires,|v|json!(v)),"wrecks":g.wrecks.iter().map(|x|json!([x.x,x.z,x.cell,x.moving])).collect::<Vec<_>>(),
    "roads":w.roads,"shoal":w.shoal,"versions":[g.terrain_version as u64,w.versions.obstruction,w.versions.infantry,w.versions.vehicle,w.versions.naval],"soak":w.soak(),"journal":w.journal(),"log":w.cell_log().iter().map(row).collect::<Vec<_>>(),"newCells":w.new_cells().iter().map(row).collect::<Vec<_>>(),"pending":pending,"watch":w.pending(Recipient::Spectators),"structures":structures,"current":w.sections.iter().map(|(c,s)|json!([c,s.id])).collect::<Vec<_>>(),
    "initial":{"chars":String::from_utf8(initial.chars.clone()).unwrap(),"ground":String::from_utf8(initial.ground.clone()).unwrap(),"objects":String::from_utf8(initial.objects.clone()).unwrap(),"mines":initial.mines,"height":initial.height,"front":initial.trench_front.as_ref().map(|v|floats(v))}
    })
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
fn integer(v: &Value) -> usize {
    v.as_u64().map(|v| v as usize).unwrap_or(usize::MAX)
}
fn recipient(v: &Value) -> Recipient {
    if v == "watch" {
        Recipient::Spectators
    } else {
        Recipient::Player(integer(v))
    }
}
fn perform(w: &mut World, a: &Value) -> Value {
    let c = integer(&a["c"]);
    match a["op"].as_str().unwrap() {
        "mutate" => {
            let v = &a["change"];
            json!(
                w.mutate_cell(
                    c,
                    CellChange {
                        ground: v["ground"].as_str().map(|s| s.as_bytes()[0]),
                        object: v["object"].as_str().map(|s| s.as_bytes()[0]),
                        mine: v["mine"].as_bool(),
                        material: v["material"].as_str().map(str::to_owned),
                        height: v["height"].as_i64().map(|h| h as i8)
                    }
                )
                .is_ok()
            )
        }
        "set" => {
            w.set_cell(c, a["ch"].as_str().unwrap().as_bytes()[0])
                .unwrap();
            Value::Null
        }
        "level" => {
            w.set_level(c, a["height"].as_i64().unwrap() as i8).unwrap();
            Value::Null
        }
        "touch" => {
            w.touch(c).unwrap();
            Value::Null
        }
        "log" => {
            w.log_cell(c, a["look"].as_bool().unwrap()).unwrap();
            Value::Null
        }
        "register" => {
            let id = recipient(&a["slot"]);
            w.register_recipient(id);
            if let Recipient::Player(slot) = id {
                while w.game.players.len() <= slot {
                    w.game.players.push(ww2_core::game::Player {
                        slot: w.game.players.len(),
                        team: 0,
                        visible: Vec::new(),
                    });
                }
            }
            Value::Null
        }
        "ack" => {
            w.acknowledge(
                recipient(&a["slot"]),
                &a["indices"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(integer)
                    .collect::<Vec<_>>(),
            );
            Value::Null
        }
        "drain" => {
            w.take_new_cells();
            Value::Null
        }
        "drainSoak" => {
            w.take_soak();
            Value::Null
        }
        "prime" => {
            if let Some(v) = a["wear"].as_f64() {
                w.game.wear.as_mut().unwrap()[c] = v as f32;
            }
            if let Some(v) = a["scar"].as_f64() {
                w.scar[c] = v as f32;
            }
            if let Some(v) = a["burnt"].as_u64() {
                w.burnt[c] = v as u8;
            }
            if let Some(v) = a["hp"].as_f64() {
                w.game.cell_hp[c] = v as f32;
            }
            if let Some(v) = a["fire"].as_f64() {
                w.fires.insert(c, v);
            }
            if let Some(v) = a["aid"].as_u64() {
                w.aid.insert(c, v as usize);
            }
            if let Some(v) = a["owner"].as_i64() {
                w.mines.insert(c, v as i32);
            }
            if let Some(v) = a["seen"].as_u64() {
                w.mine_seen.insert(c, v as u32);
            }
            if let Some(v) = a["sectionHp"].as_f64() {
                w.game.sections.get_mut(&(c as u32)).unwrap().hp = v;
            }
            if let Some(v) = a["sectionState"].as_str() {
                w.game.sections.get_mut(&(c as u32)).unwrap().failed = v == "failed";
                w.sections.get_mut(&c).unwrap().appearance = match v {
                    "burning" => Appearance::Burning,
                    "damaged" => Appearance::Damaged,
                    _ => Appearance::Intact,
                };
            }
            if let Some(v) = a["wreck"].as_array() {
                w.game.wrecks.push(Wreck {
                    id: None,
                    x: v[0].as_f64().unwrap(),
                    z: v[1].as_f64().unwrap(),
                    cell: v[2].as_u64().map(|v| v as usize),
                    moving: v[3].as_bool().unwrap(),
                });
            }
            Value::Null
        }
        "building" => {
            w.register_building_structure(
                integer(&a["id"]) as u32,
                &a["cells"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(integer)
                    .collect::<Vec<_>>(),
                a["hp"].as_f64().unwrap(),
                a["maxHp"].as_f64().unwrap(),
            )
            .unwrap();
            Value::Null
        }
        _ => panic!(),
    }
}
#[test]
fn mutation_state_journals_and_pending_match_original_source() {
    let f: Value = serde_json::from_str(include_str!("fixtures/world.json")).unwrap();
    assert_eq!(f["reference"], "e4e579a838aabae3b1d9dadbc17a0a87147bc52d");
    let mut count = 0;
    for case in f["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let m = AuthoritativeMap::from_json(&serde_json::to_vec(&case["map"]).unwrap()).unwrap();
        let mut w = World::from_map(&m).unwrap();
        for i in 0..=case["actions"].as_array().unwrap().len() {
            if i > 0 {
                assert_eq!(
                    perform(&mut w, &case["actions"][i - 1]),
                    case["results"][i - 1],
                    "{name} action {i}"
                );
                count += 1;
            }
            let actual = snapshot(&w);
            assert!(
                equal(&actual["initial"], &case["snapshots"][0]["initial"]),
                "initial terrain changed"
            );
            let expected = &case["snapshots"][i];
            for (key, value) in expected.as_object().unwrap() {
                assert!(equal(&actual[key], value), "{name} snapshot {i}: {key}");
            }
        }
    }
    assert_eq!(count, 78);
}

#[test]
fn construction_moves_live_arrays_and_invalid_insertions_are_atomic() {
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"ownership", "w":20, "h":20, "rows":vec![".".repeat(20);20],
            "spawns":[{"x":0,"y":0},{"x":19,"y":19}],"points":[{"x":10,"y":10}]
        }))
        .unwrap(),
    )
    .unwrap();
    let input = map.initialize_structures().unwrap();
    let chars = input.terrain.chars.as_ptr();
    let ground = input.terrain.layers.ground.as_ptr();
    let flags = input.terrain.flags.as_ptr();
    let mut world = World::from_structured(&map, input).unwrap();
    assert_eq!(world.game.chars.as_ptr(), chars);
    assert_eq!(world.game.ground.as_ref().unwrap().as_ptr(), ground);
    assert_eq!(world.game.flags.as_ptr(), flags);
    assert_ne!(world.initial().chars.as_ptr(), chars);
    let before = snapshot(&world);
    for cells in [&[][..], &[1, 1], &[400]] {
        assert!(
            world
                .register_building_structure(1, cells, 10.0, 20.0)
                .is_err()
        );
        assert!(equal(&snapshot(&world), &before));
    }
    assert!(
        world
            .register_building_structure(1, &[1], f64::NAN, 20.0)
            .is_err()
    );
    assert!(equal(&snapshot(&world), &before));
    world.versions.infantry = u64::MAX;
    let exhausted = snapshot(&world);
    assert!(
        world
            .register_building_structure(7, &[1], 10.0, 20.0)
            .is_err()
    );
    assert!(equal(&snapshot(&world), &exhausted));
    world.versions.infantry = 0;
    world.set_cell(1, b'K').unwrap();
    world
        .register_building_structure(1, &[1], 10.0, 20.0)
        .unwrap();
    let after = snapshot(&world);
    assert!(
        world
            .register_building_structure(1, &[2], 10.0, 20.0)
            .is_err()
    );
    assert!(equal(&snapshot(&world), &after));
    assert_eq!(world.initial().chars[1], b'.');
}
