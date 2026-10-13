use serde_json::{Value, json};
use ww2_core::{
    catalog::Catalog,
    economy_state::PlayerEconomy,
    field_works::*,
    game::{Player, Point},
    map::AuthoritativeMap,
    rules::Rules,
    unit_spawn::initial_unit,
    world::World,
};
fn n(v: &Value) -> f64 {
    v.as_f64().unwrap()
}
fn point(v: &Value) -> Point {
    Point {
        x: n(&v["x"]),
        z: n(&v["z"]),
    }
}
fn runs(values: impl IntoIterator<Item = Value>) -> Value {
    let mut out: Vec<[Value; 2]> = Vec::new();
    for value in values {
        if let Some(last) = out.last_mut().filter(|last| last[0] == value) {
            last[1] = json!(last[1].as_u64().unwrap() + 1);
        } else {
            out.push([value, json!(1)]);
        }
    }
    json!(out)
}
fn floats(values: &[f32]) -> Value {
    runs(values.iter().map(|v| json!(v.to_bits())))
}
fn job(job: &Dig) -> Value {
    let mut out = json!({"x":job.at.x,"z":job.at.z,"cells":job.cells.iter().map(|c|json!([c.cell,char::from(c.ch).to_string()])).collect::<Vec<_>>(),"t":job.elapsed,"kind":job.kind.as_str(),"dir":job.direction});
    if let Some(n) = job.original_cells {
        out["n"] = json!(n);
    }
    if let Some(reach) = job.reach {
        out["reach"] = json!(reach);
    }
    if let Some(front) = job.front {
        out["front"] = json!(front);
    }
    if let Some(from) = job.from {
        out["from"] = json!({"x":from.x,"z":from.z});
    }
    out
}
fn snapshot(w: &World, r: &FieldWorksRuntime, wallets: &[PlayerEconomy]) -> Value {
    json!({"mp":wallets.iter().map(|p|p.mp).collect::<Vec<_>>(),"jobs":r.jobs().iter().map(|(id,v)|json!([id,job(v)])).collect::<Vec<_>>(),"progress":r.jobs().keys().map(|id|json!([id,r.progress(w,*id).unwrap()])).collect::<Vec<_>>(),"pits":r.pits().iter().copied().collect::<Vec<_>>(),"world":{
 "chars":String::from_utf8(w.game.chars.clone()).unwrap(),"ground":String::from_utf8(w.game.ground.clone().unwrap()).unwrap(),"objects":String::from_utf8(w.game.objects.clone().unwrap()).unwrap(),"flags":runs(w.game.flags.iter().map(|v|json!(v))),"mines":runs(w.mine_layer.iter().map(|v|json!(v))),"hp":floats(&w.game.cell_hp),"wear":floats(w.game.wear.as_ref().unwrap()),"front":w.game.trench_front.as_ref().map(|v|floats(v)),"mineOwners":w.mines.iter().map(|(c,o)|json!([c,o])).collect::<Vec<_>>(),"mineSeen":w.mine_seen.iter().map(|(c,o)|json!([c,o])).collect::<Vec<_>>(),"aid":w.aid.iter().map(|(c,o)|json!([c,o])).collect::<Vec<_>>(),"journal":w.journal(),"versions":[u64::from(w.game.terrain_version),w.versions.obstruction,w.versions.infantry,w.versions.vehicle,w.versions.naval]}})
}
fn equal(actual: &Value, expected: &Value, label: &str) {
    match actual {
        Value::Number(value) => assert_eq!(
            format!("{:016x}", value.as_f64().unwrap().to_bits()),
            expected.as_str().unwrap(),
            "{label}"
        ),
        Value::Array(items) => {
            assert_eq!(items.len(), expected.as_array().unwrap().len(), "{label}");
            for (i, value) in items.iter().enumerate() {
                equal(value, &expected[i], &format!("{label}[{i}]"));
            }
        }
        Value::Object(items) => {
            assert_eq!(items.len(), expected.as_object().unwrap().len(), "{label}");
            for (key, value) in items {
                equal(value, &expected[key], &format!("{label}.{key}"));
            }
        }
        _ => assert_eq!(actual, expected, "{label}"),
    }
}
fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/field-works.json")).unwrap()
}
fn world(trace: &Value) -> World {
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&trace["map"]).unwrap()).unwrap();
    let mut world = World::from_map(&map).unwrap();
    world.game.players = (0..2)
        .map(|slot| Player {
            slot,
            team: slot as i32,
            visible: Vec::new(),
        })
        .collect();
    let rules = Rules::compiled().unwrap();
    let catalog = Catalog::compiled().unwrap();
    for v in trace["units"].as_array().unwrap() {
        let id = v["id"].as_u64().unwrap() as u32;
        let mut u = initial_unit(
            &rules,
            catalog,
            rules.kind(v["type"].as_str().unwrap()).unwrap(),
            id,
            v["owner"].as_u64().unwrap() as usize,
            point(v),
        )
        .unwrap();
        u.repath = 0.;
        u.retreating = v
            .get("retreating")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        u.riding = v.get("riding").and_then(Value::as_u64).unwrap_or(0) > 0;
        if let Some(hp) = v.get("hp") {
            u.hp = n(hp);
        }
        world.game.units.insert(id, u);
    }
    world
}
fn hidden(action: &Value) -> Vec<usize> {
    action
        .get("hidden")
        .and_then(Value::as_array)
        .map_or(Vec::new(), |values| {
            values
                .iter()
                .map(|v| v.as_u64().unwrap() as usize)
                .collect()
        })
}
#[test]
fn paid_placement_and_timed_cells_match_original_source() {
    let fixture = fixture();
    assert_eq!(
        fixture["reference"],
        "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
    );
    for trace in fixture["traces"].as_array().unwrap() {
        let name = trace["name"].as_str().unwrap();
        let mut w = world(trace);
        let mut runtime = FieldWorksRuntime::default();
        let mut wallets: Vec<_> = (0..2)
            .map(|team| PlayerEconomy {
                team,
                mp: n(&trace["mp"]),
                ..Default::default()
            })
            .collect();
        equal(&snapshot(&w, &runtime, &wallets), &trace["initial"], name);
        for (i, row) in trace["rows"].as_array().unwrap().iter().enumerate() {
            let a = &row["action"];
            let label = format!("{name}:{i}");
            let mut result = Value::Null;
            let mut events = Vec::new();
            let mut routes = Vec::new();
            let id = a.get("unit").and_then(Value::as_u64).unwrap_or(0) as u32;
            match a["op"].as_str().unwrap() {
                "start" => {
                    let owner = a["owner"].as_u64().unwrap() as usize;
                    let concealed = hidden(a);
                    let request = StartRequest {
                        control_eligible: true,
                        owner,
                        unit: id,
                        kind: FortKind::parse(a["kind"].as_str().unwrap()).unwrap(),
                        at: point(a),
                        direction: a.get("dir").map(n),
                    };
                    match runtime.prepare_start(&w, &mut wallets[owner], request, |p| {
                        !concealed.contains(&w.game.cell_of(p.x, p.z).unwrap())
                    }) {
                        Ok(prepared) => {
                            let cost = prepared.cost();
                            runtime.cancel(id);
                            prepared.commit(&mut runtime);
                            events.push(json!({"owner":owner,"kind":"mpSpent","value":cost}));
                        }
                        Err(Error::Refused(reason)) => result = json!(reason.as_str()),
                        Err(e) => panic!("{label}: {e:?}"),
                    }
                }
                "step" => {
                    let outcome = runtime.step_unit(&mut w, id, n(&a["dt"])).unwrap();
                    if let Some(owner) = outcome.completed {
                        events.push(json!({"owner":owner,"kind":"built","value":1}));
                    }
                    if let Some(at) = outcome.route {
                        routes.push(json!({"unit":id,"at":{"x":at.x,"z":at.z}}));
                    }
                }
                "cancel" => {
                    runtime.cancel(id);
                }
                "move" => {
                    let u = w.game.units.get_mut(&id).unwrap();
                    let fields = &a["fields"];
                    if let Some(x) = fields.get("x") {
                        u.x = n(x);
                    }
                    if let Some(z) = fields.get("z") {
                        u.z = n(z);
                    }
                    if let Some(repath) = fields.get("repath") {
                        u.repath = n(repath);
                    }
                    if let Some(retreat) = fields.get("retreating") {
                        u.retreating = retreat.as_bool().unwrap();
                    }
                    if let Some(riding) = fields.get("riding") {
                        u.riding = riding.as_u64().unwrap() > 0;
                    }
                }
                "money" => wallets[a["owner"].as_u64().unwrap() as usize].mp = n(&a["mp"]),
                "mutate" => {
                    w.set_cell(
                        a["cell"].as_u64().unwrap() as usize,
                        a["ch"].as_str().unwrap().as_bytes()[0],
                    )
                    .unwrap();
                }
                "seen" => {
                    w.mine_seen.insert(
                        a["cell"].as_u64().unwrap() as usize,
                        a["mask"].as_u64().unwrap() as u32,
                    );
                }
                "preview" => {
                    let concealed = hidden(a);
                    result = match placement(
                        &w,
                        FortKind::parse(a["kind"].as_str().unwrap()).unwrap(),
                        point(a),
                        a.get("dir").map_or(0., n),
                        a.get("team").and_then(Value::as_i64).map(|v| v as i32),
                        |p| !concealed.contains(&w.game.cell_of(p.x, p.z).unwrap()),
                    ) {
                        Ok(cells) => {
                            json!({"ok":true,"reason":null,"cells":cells.iter().map(|v|json!([v.cell,char::from(v.ch).to_string()])).collect::<Vec<_>>()})
                        }
                        Err(Error::Refused(reason)) => {
                            json!({"ok":false,"reason":reason.as_str(),"cells":[]})
                        }
                        Err(e) => panic!("{label}: {e:?}"),
                    };
                }
                op => panic!("{op}"),
            }
            equal(&result, &row["result"], &format!("{label}:result"));
            equal(&json!(events), &row["events"], &format!("{label}:events"));
            equal(&json!(routes), &row["routes"], &format!("{label}:routes"));
            equal(
                &snapshot(&w, &runtime, &wallets),
                &row["snapshot"],
                &format!("{label}:snapshot"),
            );
        }
    }
}
#[test]
fn preparation_and_invalid_steps_preserve_canonical_state() {
    let fixture = fixture();
    let mut w = world(&fixture["traces"][0]);
    let mut runtime = FieldWorksRuntime::default();
    let mut wallet = PlayerEconomy {
        team: 0,
        mp: 100.,
        ..Default::default()
    };
    let request = StartRequest {
        control_eligible: true,
        owner: 0,
        unit: 1,
        kind: FortKind::Trench,
        at: Point { x: 20., z: 20. },
        direction: Some(0.),
    };
    {
        let prepared = runtime
            .prepare_start(&w, &mut wallet, request, |_| true)
            .unwrap();
        assert_eq!(prepared.cost(), 30.);
    }
    assert_eq!(wallet.mp, 100.);
    assert!(!runtime.is_active(1));
    runtime
        .prepare_start(&w, &mut wallet, request, |_| true)
        .unwrap()
        .commit(&mut runtime);
    let before = snapshot(&w, &runtime, &[wallet.clone()]);
    assert_eq!(
        runtime.step_unit(&mut w, 1, f64::NAN),
        Err(Error::InvalidInput)
    );
    assert_eq!(before, snapshot(&w, &runtime, &[wallet.clone()]));
    w.game.terrain_version = u32::MAX;
    let before = snapshot(&w, &runtime, &[wallet.clone()]);
    assert!(matches!(
        runtime.step_unit(&mut w, 1, 3.),
        Err(Error::World(_))
    ));
    assert_eq!(before, snapshot(&w, &runtime, &[wallet]));
    assert_eq!(FortKind::parse("bridge"), Err(Error::UnsupportedKind));
    assert_eq!(FortKind::parse("wall"), Err(Error::UnsupportedKind));
}
