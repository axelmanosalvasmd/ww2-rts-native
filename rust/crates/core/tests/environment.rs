use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use ww2_core::{
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
fn world_snapshot(w: &World) -> Value {
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
    "mineOwners":map(&w.mines,|v|json!(v)),"mineSeen":map(&w.mine_seen,|v|json!(v)),"aid":map(&w.aid,|v|json!(v)),"fires":map(&w.fires.iter().map(|(c,t)|(*c,*t)).collect(),|v|json!(v)),"wrecks":g.wrecks.iter().map(|x|json!([x.x,x.z,x.cell,x.moving])).collect::<Vec<_>>(),
    "roads":w.roads,"shoal":w.shoal,"versions":[g.terrain_version as u64,w.versions.obstruction,w.versions.infantry,w.versions.vehicle,w.versions.naval],"soak":w.soak(),"journal":w.journal(),"log":w.cell_log().iter().map(row).collect::<Vec<_>>(),"newCells":w.new_cells().iter().map(row).collect::<Vec<_>>(),"pending":pending,"watch":w.pending(Recipient::Spectators),"structures":structures,"current":w.sections.iter().map(|(c,s)|json!([c,s.id])).collect::<Vec<_>>(),
    "initial":{"chars":String::from_utf8(initial.chars.clone()).unwrap(),"ground":String::from_utf8(initial.ground.clone()).unwrap(),"objects":String::from_utf8(initial.objects.clone()).unwrap(),"mines":initial.mines,"height":initial.height,"front":initial.trench_front.as_ref().map(|v|floats(v))}
    })
}
use ww2_core::{
    catalog::Catalog,
    environment::{Blast, Context, DamageContext, DamageMemory, Effect, EnvironmentRuntime},
    game::Point,
    grid::SpatialGrid,
    rules::Rules,
    unit_spawn::initial_unit,
};
fn point(v: &Value) -> Point {
    Point {
        x: v["x"].as_f64().unwrap(),
        z: v["z"].as_f64().unwrap(),
    }
}
fn falling(v: &ww2_core::environment::FallingSection) -> Value {
    let b = &v.body;
    json!({"id":b.id,"kind":b.kind.as_str(),"x":b.x,"y":b.y,"z":b.z,"vx":b.vx,"vy":b.vy,"vz":b.vz,"dir":b.dir,"material":b.material,"mass":b.mass,"radius":b.radius,"origin":{"x":b.origin.x,"z":b.origin.z},"gravity":b.gravity,"drag":b.drag,"stamp":b.stamp,"born":b.born,"contacts":b.contacts,"settled":b.settled,"tilt":b.tilt,"section":v.section,"c":v.cell,"bridge":v.bridge,"at":v.at,"until":v.until,"impulse":v.impulse})
}
fn snapshot(
    w: &World,
    r: &EnvironmentRuntime,
    seed: u32,
    next_id: u32,
    order: &[u32],
    last_hit: &BTreeMap<u32, i32>,
) -> Value {
    let mut s = world_snapshot(w);
    for key in [
        "initial", "front", "tough", "roads", "shoal", "aid", "wrecks", "pending", "watch",
        "current",
    ] {
        s.as_object_mut().unwrap().remove(key);
    }
    s["fires"] = json!(
        w.fires
            .iter()
            .map(|(c, t)| json!([c, t]))
            .collect::<Vec<_>>()
    );
    s["structures"]=json!(w.structures.iter().map(|structure|json!([structure.id,structure.kind,structure.entity_id,structure.section_ids.iter().zip(&structure.cells).map(|(id,c)|{
        let (m,h)=w.section_by_id(id).unwrap();let mut s=json!({"id":m.id,"structureId":m.structure_id,"c":c,"hp":h.hp,"maxHp":h.max_hp.unwrap(),"material":m.material,"anchor":m.anchor,"supports":m.supports,"state":if h.failed{"failed"}else{match m.appearance{Appearance::Intact=>"intact",Appearance::Damaged=>"damaged",Appearance::Burning=>"burning"}},"rubble":m.rubble});
        let f=&m.failure;if let Some(v)=f.impulse{s["impulse"]=json!(v);}
if let Some(v)=f.failed_at{s["failedAt"]=json!(v);}
if let Some(v)=f.direction{s["direction"]=json!({"x":v.x,"z":v.z});}
if let Some(v)=f.settled_at{s["settledAt"]=json!(v);}s
    }).collect::<Vec<_>>()])).collect::<Vec<_>>());
    s["units"] = json!(
        order
            .iter()
            .map(|id| {
                let u = &w.game.units[id];
                json!([
                    id,
                    u.hp,
                    u.supp,
                    u.garrison.map_or(-1, |c| c as i64),
                    u.path
                        .iter()
                        .map(|p| json!({"x":p.x,"z":p.z}))
                        .collect::<Vec<_>>(),
                    last_hit.get(id)
                ])
            })
            .collect::<Vec<_>>()
    );
    s["smokes"] = json!(
        w.game
            .smokes
            .iter()
            .map(|s| json!({"id":s.id,"x":s.x,"z":s.z,"r":s.r,"t":s.remaining.unwrap()}))
            .collect::<Vec<_>>()
    );
    s["falling"] = json!(r.falling_sections().iter().map(falling).collect::<Vec<_>>());
    s["contacts"] = json!(r.contacts());
    s["seed"] = json!(seed);
    s["nextId"] = json!(next_id);
    s
}
fn equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => {
            a.as_f64().unwrap().to_bits() == b.as_f64().unwrap().to_bits()
        }
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| equal(a, b))
        }
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len()
                && a.iter()
                    .all(|(key, v)| b.get(key).is_some_and(|b| equal(v, b)))
        }
        _ => a == b,
    }
}
fn difference(a: &Value, b: &Value, path: &str) -> Option<String> {
    if equal(a, b) {
        return None;
    }
    match (a, b) {
        (Value::Object(a), Value::Object(b)) => {
            for key in a.keys().chain(b.keys()) {
                if let Some(d) = difference(
                    a.get(key).unwrap_or(&Value::Null),
                    b.get(key).unwrap_or(&Value::Null),
                    &format!("{path}.{key}"),
                ) {
                    return Some(d);
                }
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            if a.len() != b.len() {
                return Some(format!("{path}: length {} != {}", a.len(), b.len()));
            }
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                if let Some(d) = difference(a, b, &format!("{path}[{i}]")) {
                    return Some(d);
                }
            }
        }
        _ => {}
    }
    Some(format!("{path}: {a} != {b}"))
}
fn apply_patches(state: &mut Value, patches: &Value) {
    for patch in patches.as_array().unwrap() {
        let path = patch[0].as_array().unwrap();
        let mut parent = &mut *state;
        for key in &path[..path.len() - 1] {
            parent = if let Some(i) = key.as_u64() {
                &mut parent[i as usize]
            } else {
                &mut parent[key.as_str().unwrap()]
            };
        }
        let key = path.last().unwrap();
        if patch.as_array().unwrap().len() == 1 {
            parent
                .as_object_mut()
                .unwrap()
                .remove(key.as_str().unwrap());
        } else if let Some(i) = key.as_u64() {
            parent[i as usize] = patch[1].clone();
        } else {
            parent[key.as_str().unwrap()] = patch[1].clone();
        }
    }
}
#[test]
fn authoritative_environment_matches_original_actions_and_order() {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/environment.json")).unwrap();
    let rules = Rules::compiled().unwrap();
    let catalog = Catalog::compiled().unwrap();
    assert_eq!(
        fixture["reference"],
        "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
    );
    assert_eq!(fixture["arch"], "x64");
    assert_eq!(fixture["node"], "v24.21.0");
    let mut count = 0;
    for case in fixture["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let map = AuthoritativeMap::from_json(&serde_json::to_vec(&case["map"]).unwrap()).unwrap();
        let mut w = World::from_map(&map).unwrap();
        let mut r = EnvironmentRuntime::default();
        let mut order = Vec::new();
        let mut last_hit = BTreeMap::new();
        for v in case["units"].as_array().unwrap() {
            let id = v["id"].as_u64().unwrap() as u32;
            let kind = rules
                .kinds
                .iter()
                .position(|k| k.name == v["type"].as_str().unwrap())
                .unwrap();
            let mut u = initial_unit(&rules, catalog, kind, id, 0, point(v)).unwrap();
            if let Some(h) = v["hp"].as_f64() {
                u.hp = h;
            }
            if let Some(s) = v["supp"].as_f64() {
                u.supp = s;
            }
            u.garrison = v["garrison"].as_u64().map(|v| v as usize);
            u.riding = v["riding"].as_bool().unwrap_or(false);
            if let Some(path) = v["path"].as_array() {
                u.path = path.iter().map(point).collect();
            }
            w.game.units.insert(id, u);
            order.push(id);
        }
        let grid = SpatialGrid::new(&w.game.units, 16.);
        let (mut seed, mut next_id, mut tick, mut rain, mut wet, mut wind_angle, mut wind_strength) =
            (123456789, 1000, 10, 0., 0., 0.7, 0.6);
        let mut memory = DamageMemory::default();
        let mut expected = case["initial"].clone();
        let actual = snapshot(&w, &r, seed, next_id, &order, &last_hit);
        assert!(
            equal(&actual, &expected),
            "{name} initial {}",
            difference(&actual, &expected, "").unwrap_or_default()
        );
        for (index, a) in case["actions"].as_array().unwrap().iter().enumerate() {
            count += 1;
            tick = a["tick"].as_u64().unwrap_or(tick);
            let mut ctx = Context {
                tick,
                seed: &mut seed,
                next_id: &mut next_id,
                unit_order: &order,
                grid: &grid,
                rain,
                wet,
                wind_angle,
                wind_strength,
            };
            let c = a["c"].as_u64().unwrap_or(0) as usize;
            let mut effects = Vec::new();
            let mut result = Value::Null;
            let skip = a["skip"].as_array().map_or_else(BTreeSet::new, |v| {
                v.iter().map(|v| v.as_u64().unwrap() as u32).collect()
            });
            let mut hit = DamageContext::plain(&skip);
            hit.owner = a["owner"].as_i64().map(|v| v as i32);
            hit.contact_id = a["contact"].as_str();
            hit.direction = (!a["direction"].is_null()).then(|| point(&a["direction"]));
            hit.debit_entity = a["debit"].as_bool().unwrap_or(true);
            if a["memory"] == true {
                hit.memory = Some(&mut memory);
            }
            match a["op"].as_str().unwrap() {
                "ignite" => r.ignite(&mut w, c, &mut ctx).unwrap(),
                "burn" => effects = r.burn(&mut w, a["dt"].as_f64().unwrap(), &mut ctx).unwrap(),
                "flood" => r.flood(&mut w).unwrap(),
                "debris" => r
                    .step_sections(&mut w, a["dt"].as_f64().unwrap(), &mut ctx)
                    .unwrap(),
                "contact_loop" => {
                    for i in 0..a["count"].as_u64().unwrap() {
                        let contact = format!("fifo:{i}");
                        let mut hit = DamageContext::plain(&skip);
                        hit.contact_id = Some(&contact);
                        let (_, e) = r
                            .damage_section(
                                &mut w,
                                c,
                                a["damage"].as_f64().unwrap(),
                                &mut ctx,
                                &hit,
                            )
                            .unwrap();
                        effects.extend(e);
                    }
                }
                "section" => {
                    let damage = a["damage"].as_f64().unwrap_or(f64::INFINITY);
                    let (hit, e) = r.damage_section(&mut w, c, damage, &mut ctx, &hit).unwrap();
                    result = json!(hit);
                    effects = e;
                }
                "blast" => {
                    effects = r
                        .damage_cells(
                            &mut w,
                            Blast {
                                at: point(&a["at"]),
                                radius: a["radius"].as_f64().unwrap(),
                                damage: a["damage"].as_f64().unwrap(),
                                scar: a["scar"].as_f64().unwrap_or(0.),
                            },
                            &mut ctx,
                            &mut hit,
                        )
                        .unwrap()
                }
                "wreck" => {
                    effects = r
                        .wreck_cell(
                            &mut w,
                            c,
                            a["into"].as_str().map(|s| s.as_bytes()[0]),
                            &mut ctx,
                        )
                        .unwrap()
                }
                "mutate" => {
                    let change = &a["change"];
                    w.mutate_cell(
                        c,
                        CellChange {
                            object: change["object"].as_str().map(|s| s.as_bytes()[0]),
                            ground: change["ground"].as_str().map(|s| s.as_bytes()[0]),
                            height: change["height"].as_i64().map(|v| v as i8),
                            ..Default::default()
                        },
                    )
                    .unwrap();
                }
                "building" => {
                    let id = a["id"].as_u64().unwrap() as u32;
                    let cells = a["cells"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|c| c.as_u64().unwrap() as usize)
                        .collect::<Vec<_>>();
                    let hp = a["hp"].as_f64().unwrap();
                    w.game.units.get_mut(&id).unwrap().hp = hp;
                    w.register_building_structure(
                        id,
                        &cells,
                        hp,
                        catalog
                            .unit("hq")
                            .unwrap()
                            .get("hpPer")
                            .unwrap()
                            .number()
                            .unwrap(),
                    )
                    .unwrap();
                }
                "prime" => {
                    if let Some(v) = a["rain"].as_f64() {
                        rain = v;
                    }
                    if let Some(v) = a["wet"].as_f64() {
                        wet = v;
                    }
                    if !a["wind"].is_null() {
                        wind_angle = a["wind"]["a"].as_f64().unwrap();
                        wind_strength = a["wind"]["v"].as_f64().unwrap();
                    }
                    if let Some(v) = a["fire"].as_array() {
                        let c = v[0].as_u64().unwrap() as usize;
                        if let Some(t) = v[1].as_f64() {
                            w.fires.insert(c, t);
                        } else {
                            w.fires.remove(&c);
                        }
                    }
                    if let Some(v) = a["scar"].as_array() {
                        w.scar[v[0].as_u64().unwrap() as usize] = v[1].as_f64().unwrap() as f32;
                    }
                    if let Some(v) = a["hp"].as_array() {
                        w.game.cell_hp[v[0].as_u64().unwrap() as usize] =
                            v[1].as_f64().unwrap() as f32;
                    }
                    if let Some(v) = a["wear"].as_array() {
                        w.game.wear.as_mut().unwrap()[v[0].as_u64().unwrap() as usize] =
                            v[1].as_f64().unwrap() as f32;
                    }
                }
                _ => panic!("unknown action"),
            }
            let mut shots = Vec::new();
            for effect in effects {
                if let Effect::LastHit { unit, owner } = effect {
                    last_hit.insert(unit, owner);
                } else {
                    shots.push(effect.to_json());
                }
            }
            let actual_result = json!({"value":result,"shots":shots});
            assert!(
                equal(&actual_result, &case["results"][index]),
                "{name} action {index} result {}",
                difference(&actual_result, &case["results"][index], "").unwrap_or_default()
            );
            apply_patches(&mut expected, &case["patches"][index]);
            let actual = snapshot(&w, &r, seed, next_id, &order, &last_hit);
            assert!(
                equal(&actual, &expected),
                "{name} action {index} {}",
                difference(&actual, &expected, "").unwrap_or_default()
            );
        }
    }
    assert!(count > 200);
}

#[test]
fn checked_inputs_fail_before_world_seed_ids_and_contact_memory_change() {
    use ww2_core::environment::EnvironmentError;
    let fixture: Value = serde_json::from_str(include_str!("fixtures/environment.json")).unwrap();
    let map =
        AuthoritativeMap::from_json(&serde_json::to_vec(&fixture["cases"][0]["map"]).unwrap())
            .unwrap();
    let mut w = World::from_map(&map).unwrap();
    let mut runtime = EnvironmentRuntime::default();
    let order = [];
    let grid = SpatialGrid::new(&w.game.units, 16.);
    let mut seed = 123;
    let mut next_id = 1000;
    let owners = BTreeMap::new();
    let before = snapshot(&w, &runtime, seed, next_id, &order, &owners);
    let mut ctx = Context {
        tick: 10,
        seed: &mut seed,
        next_id: &mut next_id,
        unit_order: &order,
        grid: &grid,
        rain: 0.,
        wet: 0.,
        wind_angle: 0.,
        wind_strength: 0.5,
    };
    let skip = BTreeSet::new();
    let mut memory = DamageMemory::default();
    let mut hit = DamageContext::plain(&skip);
    hit.memory = Some(&mut memory);
    for blast in [
        Blast {
            at: Point {
                x: f64::INFINITY,
                z: 1.,
            },
            radius: 1.,
            damage: 100.,
            scar: 1.,
        },
        Blast {
            at: Point { x: 1., z: 1. },
            radius: 4097.,
            damage: 100.,
            scar: 1.,
        },
        Blast {
            at: Point { x: 1e308, z: 1. },
            radius: 1.,
            damage: 100.,
            scar: 1.,
        },
    ] {
        assert!(matches!(
            runtime.damage_cells(&mut w, blast, &mut ctx, &mut hit),
            Err(EnvironmentError::InvalidInput(_))
        ));
    }
    assert!(runtime.burn(&mut w, f64::NAN, &mut ctx).is_err());
    assert!(runtime.ignite(&mut w, usize::MAX, &mut ctx).is_err());
    assert!(
        runtime
            .damage_section(&mut w, 0, f64::NAN, &mut ctx, &hit)
            .is_err()
    );
    assert!(runtime.step_sections(&mut w, 3., &mut ctx).is_err());
    assert!(hit.memory.as_ref().unwrap().cells.is_empty());
    assert_eq!(
        snapshot(&w, &runtime, *ctx.seed, *ctx.next_id, &order, &owners),
        before
    );
    w.versions.infantry = u64::MAX;
    let before = snapshot(&w, &runtime, *ctx.seed, *ctx.next_id, &order, &owners);
    assert!(matches!(
        runtime.ignite(&mut w, 0, &mut ctx),
        Err(EnvironmentError::World(_))
    ));
    assert_eq!(
        snapshot(&w, &runtime, *ctx.seed, *ctx.next_id, &order, &owners),
        before
    );
}

#[test]
fn exhausted_escape_path_identity_rejects_before_fire_damage() {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/environment.json")).unwrap();
    let case = fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "fire-order-rain-expiry-and-path")
        .unwrap();
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&case["map"]).unwrap()).unwrap();
    let mut w = World::from_map(&map).unwrap();
    let rules = Rules::compiled().unwrap();
    let kind = rules.kinds.iter().position(|k| k.name == "rifle").unwrap();
    let mut unit = initial_unit(
        &rules,
        Catalog::compiled().unwrap(),
        kind,
        7,
        0,
        Point { x: 5., z: 5. },
    )
    .unwrap();
    unit.path_gen = u64::MAX;
    w.game.units.insert(7, unit);
    w.fires.insert(42, 10.);
    let mut r = EnvironmentRuntime::default();
    let grid = SpatialGrid::new(&w.game.units, 16.);
    let order = [7];
    let mut seed = 123;
    let mut next_id = 1000;
    let owners = BTreeMap::new();
    let before = snapshot(&w, &r, seed, next_id, &order, &owners);
    let mut ctx = Context {
        tick: 10,
        seed: &mut seed,
        next_id: &mut next_id,
        unit_order: &order,
        grid: &grid,
        rain: 0.,
        wet: 0.,
        wind_angle: 0.,
        wind_strength: 0.5,
    };
    assert!(r.burn(&mut w, 0.5, &mut ctx).is_err());
    assert_eq!(snapshot(&w, &r, seed, next_id, &order, &owners), before);
}

#[test]
fn fire_registry_preserves_updates_and_remove_reinsert_order() {
    use ww2_core::world::OrderedFires;
    let mut fires = OrderedFires::default();
    fires.insert(9, 1.);
    fires.insert(2, 2.);
    fires.insert(9, 3.);
    assert_eq!(
        fires.iter().map(|(c, t)| (*c, *t)).collect::<Vec<_>>(),
        [(9, 3.), (2, 2.)]
    );
    assert_eq!(fires.remove(&9), Some(3.));
    fires.insert(9, 4.);
    assert_eq!(
        fires.iter().map(|(c, t)| (*c, *t)).collect::<Vec<_>>(),
        [(2, 2.), (9, 4.)]
    );
    assert_eq!(fires.get(&9), Some(&4.));
    assert!(fires.contains_key(&2));
    assert_eq!(fires.len(), 2);
    fires.clear();
    assert!(fires.is_empty());
}
