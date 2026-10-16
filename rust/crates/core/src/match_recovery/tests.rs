use crate::{
    catalog::Catalog,
    economy::{Faction, Mode},
    economy_state::ArmySize,
    environment::{Context, DamageContext, Effect},
    game::{Player, Point},
    grid::SpatialGrid,
    logistics::{LogisticsState, Reserves, Stock},
    map::AuthoritativeMap,
    match_combat::CombatState,
    match_commands::{MatchCommands, UnitControl},
    match_firing::FiringState,
    match_recovery::{RecoveryFrame, RecoveryState},
    match_runtime::{Match, MatchOptions, PlayerConfig},
    match_story::PlayerStory,
    objectives::Objective,
    supply_convoys::{ConvoyState, Store, StoreId},
    supply_network::GroundSupplyNetwork,
    unit_spawn::initial_unit,
    weather::WeatherChoice,
    world::{Appearance, CellChange, Recipient, World},
};
use serde_json::{Value, json};

fn point(v: &Value) -> Point {
    Point {
        x: v["x"].as_f64().unwrap(),
        z: v["z"].as_f64().unwrap(),
    }
}
fn runs(values: impl IntoIterator<Item = Value>) -> Value {
    let mut rows: Vec<[Value; 2]> = Vec::new();
    for value in values {
        if let Some(last) = rows.last_mut().filter(|r| r[0] == value) {
            last[1] = json!(last[1].as_u64().unwrap() + 1);
        } else {
            rows.push([value, json!(1)]);
        }
    }
    json!(rows)
}
fn bits(values: &[f32]) -> Value {
    runs(values.iter().map(|v| json!(v.to_bits())))
}
fn row(row: &crate::world::CellRow) -> Value {
    let d = &row.data;
    let mut data = json!({"ground":char::from(d.ground).to_string(),"object":char::from(d.object).to_string(),"material":d.material,"groundMaterial":d.ground_material,"groundWear":d.ground_wear});
    if let Some(v) = d.trench_front {
        data["trenchFront"] = json!(v);
    }
    if d.mine {
        data["mine"] = json!(true);
    }
    if let Some(v) = &d.mine_material {
        data["mineMaterial"] = json!(v);
    }
    if let Some(s) = &d.section {
        data["section"] = json!({"id":s.id,"structureId":s.structure_id,"state":s.state,"hp":s.hp,"maxHp":s.max_hp,"material":s.material,"anchor":s.anchor});
    }
    json!([
        row.cell,
        char::from(row.ch).to_string(),
        row.height,
        row.state,
        data
    ])
}
fn body(v: &crate::environment_debris::DebrisBody) -> Value {
    json!({"id":v.id,"kind":v.kind.as_str(),"x":v.x,"y":v.y,"z":v.z,"vx":v.vx,"vy":v.vy,"vz":v.vz,"dir":v.dir,"material":v.material,"mass":v.mass,"radius":v.radius,"origin":{"x":v.origin.x,"z":v.origin.z},"gravity":v.gravity,"drag":v.drag,"stamp":v.stamp,"born":v.born,"contacts":v.contacts,"settled":v.settled,"tilt":v.tilt})
}
fn projected_reserves(v: &Reserves) -> Value {
    json!({"ammo":v.ammo,"ammoMax":v.ammo_max,"provisions":v.provisions,"fuel":v.fuel,"emergency":v.emergency,"exhausted":v.exhausted,"forced":v.forced,"stranded":v.stranded})
}
fn snapshot(m: &Match, order: &[u32], frame: Option<&RecoveryFrame>, shots: &[Value]) -> Value {
    let w = &m.world;
    let g = &w.game;
    let units = order.iter().map(|id| {
        let u = &g.units[id];
        let recovery = m.recovery.get(*id).unwrap();
        let structure = w.structures.iter().find(|s| s.entity_id == Some(*id));
        json!({"id":id,"hp":u.hp,"reinf":recovery.reinforcement,"hpWas":recovery.hp_was,"woundAt":recovery.wound_at,"healAt":recovery.heal_at,"path":u.path.iter().map(|p|json!({"x":p.x,"z":p.z})).collect::<Vec<_>>(),"logistics":m.logistics.get(*id).map(projected_reserves),"structureId":structure.map(|s|&s.id)})
    }).collect::<Vec<_>>();
    let structures = w.structures.iter().map(|s| {
        let mut structure = json!({"id":s.id,"kind":s.kind,"sections":s.section_ids.iter().zip(&s.cells).map(|(id,c)| {
            let (meta,hp) = w.section_by_id(id).unwrap();
            let state = if hp.failed { "failed" } else { match meta.appearance { Appearance::Intact=>"intact",Appearance::Damaged=>"damaged",Appearance::Burning=>"burning" } };
            let mut section=json!({"id":id,"structureId":meta.structure_id,"c":c,"hp":hp.hp,"maxHp":hp.max_hp,"material":meta.material,"anchor":meta.anchor,"supports":meta.supports,"state":state,"rubble":meta.rubble});
            if let Some(entity)=s.entity_id {section["entityId"]=json!(entity);}
            if let Some(v)=meta.failure.impulse {section["impulse"]=json!(v);}
            if let Some(v)=meta.failure.failed_at {section["failedAt"]=json!(v);}
            if let Some(v)=meta.failure.direction {section["direction"]=json!({"x":v.x,"z":v.z});}
            if let Some(v)=meta.failure.settled_at {section["settledAt"]=json!(v);}
            section
        }).collect::<Vec<_>>()});
        if let Some(id)=s.entity_id {structure["entityId"]=json!(id);}
        structure
    }).collect::<Vec<_>>();
    let falling = m
        .environment
        .falling_sections()
        .iter()
        .map(|v| {
            let mut value = body(&v.body);
            for (key, v) in [
                ("section", json!(v.section)),
                ("c", json!(v.cell)),
                ("bridge", json!(v.bridge)),
                ("at", json!(v.at)),
                ("until", json!(v.until)),
                ("impulse", json!(v.impulse)),
            ] {
                value[key] = v;
            }
            value
        })
        .collect::<Vec<_>>();
    json!({"tick":g.tick,"units":units,"wallets":m.start.economy.players.iter().map(|p|json!({"mp":p.mp})).collect::<Vec<_>>(),"story":m.story.iter().map(|s|json!({"mpSpent":s.mp_spent})).collect::<Vec<_>>(),"structures":structures,"cellHp":bits(&g.cell_hp),"flags":runs(g.flags.iter().map(|v|json!(v))),"objects":String::from_utf8(g.objects.clone().unwrap()).unwrap(),"chars":String::from_utf8(g.chars.clone()).unwrap(),"wear":bits(g.wear.as_ref().unwrap()),"burnt":runs(w.burnt.iter().map(|v|json!(v))),"cellLog":w.cell_log().iter().map(row).collect::<Vec<_>>(),"newCells":w.new_cells().iter().map(row).collect::<Vec<_>>(),"journal":w.journal(),"pending":(0..g.players.len()).map(|slot|w.pending(Recipient::Player(slot)).unwrap().iter().copied().collect::<Vec<_>>()).collect::<Vec<_>>(),"watchPending":w.pending(Recipient::Spectators).unwrap().iter().copied().collect::<Vec<_>>(),"versions":[g.terrain_version,w.versions.obstruction,w.versions.infantry,w.versions.vehicle,w.versions.naval],"mineSeen":w.mine_seen.iter().map(|(c,b)|json!([c,b])).collect::<Vec<_>>(),"mines":w.mines.iter().map(|(c,b)|json!([c,b])).collect::<Vec<_>>(),"aid":w.aid.iter().map(|(c,b)|json!([c,b])).collect::<Vec<_>>(),"points":m.start.points.iter().map(|p|json!({"x":p.at.x,"z":p.at.z,"kind":p.kind,"owner":p.owner.map_or(-1,|owner|owner as i64),"cut":p.cut})).collect::<Vec<_>>(),"shots":shots,"fallingSections":falling,"seed":m.start.environment_seed,"frame":frame.map(|f|json!({"list":f.units()})),"stores":m.convoys.as_ref().map(|c|c.stores.iter().map(|s|json!({"id":s.id.key(),"owner":s.owner,"x":s.at.x,"z":s.at.z,"source":s.source,"active":s.active,"buckets":s.buckets.iter().map(|(owner,b)|json!([owner,{"ammo":b.ammo,"provisions":b.provisions,"fuel":b.fuel}])).collect::<Vec<_>>()})).collect::<Vec<_>>())})
}
fn normalize(mut value: Value) -> Value {
    // Reserve capacity/price fields absent from the input are outside this recorder's projection.
    for u in value["units"].as_array_mut().unwrap() {
        if let Some(r) = u["logistics"].as_object_mut() {
            r.retain(|k, _| {
                [
                    "ammo",
                    "ammoMax",
                    "provisions",
                    "fuel",
                    "emergency",
                    "exhausted",
                    "forced",
                    "stranded",
                ]
                .contains(&k.as_str())
            });
        }
    }
    if let Some(frame) = value["frame"].as_object_mut() {
        frame.retain(|k, _| k == "list");
    }
    for p in value["points"].as_array_mut().unwrap() {
        let mut projected =
            json!({"x":p["x"],"z":p["z"],"kind":p["kind"],"owner":p["owner"],"cut":p["cut"]});
        if projected["kind"].is_null() {
            projected["kind"] = Value::Null;
        }
        *p = projected;
    }
    for field in ["mineSeen", "mines", "aid"] {
        value[field]
            .as_array_mut()
            .unwrap()
            .sort_by_key(|v| v[0].as_u64().unwrap());
    }
    value
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
            a.len() == b.len() && a.iter().all(|(k, a)| b.get(k).is_some_and(|b| equal(a, b)))
        }
        _ => a == b,
    }
}
fn diff(a: &Value, b: &Value, path: &str) -> String {
    match (a, b) {
        (Value::Object(a), Value::Object(b)) => {
            for k in a.keys().chain(b.keys()) {
                let av = a.get(k).unwrap_or(&Value::Null);
                let bv = b.get(k).unwrap_or(&Value::Null);
                if !equal(av, bv) {
                    return diff(av, bv, &format!("{path}.{k}"));
                }
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            if a.len() != b.len() {
                return format!("{path}: lengths {} != {}", a.len(), b.len());
            }
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                if !equal(a, b) {
                    return diff(a, b, &format!("{path}[{i}]"));
                }
            }
        }
        _ => {}
    }
    format!("{path}: {a} != {b}")
}
fn reserves(v: &Value) -> Reserves {
    Reserves {
        ammo: v["ammo"].as_f64().unwrap(),
        ammo_max: v["ammoMax"].as_f64().unwrap(),
        ammo_price: v["ammoPrice"].as_f64().unwrap_or(0.01),
        ammo_credit: v["ammoCredit"].as_f64(),
        provisions: v["provisions"].as_f64().unwrap(),
        fuel: v["fuel"].as_f64(),
        emergency: v["emergency"].as_f64(),
        warning: v["warning"].as_f64(),
        forced: v["forced"].as_bool().unwrap_or(false),
        stranded: v["stranded"].as_bool().unwrap_or(false),
        exhausted: v["exhausted"].as_bool().unwrap_or(false),
        supply_rate: None,
        cut_for: None,
    }
}
fn insert(m: &mut Match, v: &Value, order: &mut Vec<u32>) {
    let id = v["id"].as_u64().unwrap() as u32;
    assert!(!m.world.game.units.contains_key(&id));
    let kind = m.rules.kind(v["type"].as_str().unwrap()).unwrap();
    let owner = v["owner"].as_u64().unwrap_or(0) as usize;
    let mut u = initial_unit(
        &m.rules,
        Catalog::compiled().unwrap(),
        kind,
        id,
        owner,
        point(v),
    )
    .unwrap();
    u.hp = v["hp"].as_f64().unwrap_or(u.hp);
    u.rot = v["rot"].as_f64().unwrap_or(0.);
    u.retreating = v["retreating"].as_bool().unwrap_or(false);
    u.still = Some(v["still"].as_f64().unwrap_or(0.));
    u.built = if m.rules.kinds[kind].building {
        Some(v["built"].as_f64().unwrap_or(1.))
    } else {
        None
    };
    u.riding = v["riding"].as_u64().unwrap_or(0) != 0;
    u.board = v["board"].as_u64().unwrap_or(0) as u32;
    u.garrison = v["garrison"]
        .as_i64()
        .filter(|v| *v >= 0)
        .map(|v| v as usize);
    u.amove = (!v["amove"].is_null()).then(|| point(&v["amove"]));
    u.path = v["path"]
        .as_array()
        .map_or_else(Vec::new, |rows| rows.iter().map(point).collect());
    let mut control = UnitControl::new(&u, &m.rules).unwrap();
    control.operative = v["operative"].as_u64().unwrap_or(0) != 0;
    control.enter = v["enter"].as_i64().filter(|v| *v >= 0).map(|v| v as usize);
    m.commands.units.insert(id, control);
    let recovery = m.recovery.initialize(id);
    recovery.reinforcement = v["reinf"].as_f64().unwrap_or(0.);
    recovery.hp_was = v["hpWas"].as_f64();
    recovery.wound_at = v["woundAt"].as_i64();
    recovery.heal_at = v["healAt"].as_i64();
    m.combat.initialize(id).carrier = v["riding"]
        .as_u64()
        .filter(|id| *id != 0)
        .map(|id| id as u32);
    m.firing.initialize(id);
    m.abilities.initialize(id);
    if !v["logistics"].is_null() {
        m.logistics.insert(id, reserves(&v["logistics"]));
    }
    m.world.game.units.insert(id, u);
    m.grid.update(id, &m.world.game.units[&id]);
    order.push(id);
}
fn make(case: &Value) -> (Match, Vec<u32>) {
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&case["map"]).unwrap()).unwrap();
    let configs = (0..3)
        .map(|slot| PlayerConfig {
            name: format!("p{slot}"),
            team: if slot == 2 { 0 } else { slot },
            faction: Faction::try_from((slot % 2) as u8).unwrap(),
        })
        .collect();
    let options = MatchOptions {
        seed: 1,
        mode: Mode::Conquest,
        players: configs,
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
    };
    let mut m = Match::new(&map, options).unwrap();
    m.world = World::from_map(&map).unwrap();
    m.world.game.players = (0..3)
        .map(|slot| Player {
            slot,
            team: if slot == 2 { 0 } else { slot as i32 },
            visible: Vec::new(),
        })
        .collect();
    for slot in 0..3 {
        m.world.register_recipient(Recipient::Player(slot));
        m.start.players[slot].spawn = Point {
            x: 3. + slot as f64 * 20.,
            z: 3.,
        };
        m.start.economy.players[slot].mp = 100.;
    }
    m.world.register_recipient(Recipient::Spectators);
    m.world.game.tick = 10;
    m.world.game.rain = 0.;
    m.world.game.wet = Some(0.);
    m.start.environment_seed = 123;
    m.start.wind = (0., 0.5);
    m.next_unit_id = 1;
    m.firing = FiringState::default();
    m.combat = CombatState::default();
    m.recovery = RecoveryState::default();
    m.abilities = crate::match_abilities::AbilityState::default();
    m.commands = MatchCommands::new(4096);
    m.logistics = LogisticsState::default();
    m.story = vec![PlayerStory::default(); 3];
    m.grid = SpatialGrid::new(&m.world.game.units, 16.);
    let mut order = Vec::new();
    for unit in case["units"].as_array().unwrap() {
        insert(&mut m, unit, &mut order);
    }
    for p in &mut m.world.game.players {
        p.visible = order.clone();
        p.visible.sort_unstable();
        p.visible.dedup();
    }
    m.options.logistics = case["logistics"].as_bool().unwrap();
    m.convoys = ConvoyState::setup(m.options.logistics);
    m.supply_net = GroundSupplyNetwork {
        width: m.world.game.w,
        height: m.world.game.h,
        teams: Vec::new(),
    };
    if m.options.logistics {
        m.next_unit_id = order.iter().copied().max().unwrap_or(0) + 1;
    }
    // Match::new initialized the old world. Bind fresh observation state to this
    // component fixture's actual initial terrain, without a public restore API.
    m.building_cells.clear();
    for slot in 0..m.world.game.players.len() {
        m.world
            .register_recipient(crate::world::Recipient::Player(slot));
    }
    m.initialize_recipient_routing().unwrap();
    (m, order)
}
fn prime(m: &mut Match, id: u32, changes: &Value) {
    let u = m.world.game.units.get_mut(&id).unwrap();
    for (k, v) in changes.as_object().unwrap() {
        match k.as_str() {
            "hp" => u.hp = v.as_f64().unwrap(),
            "x" => u.x = v.as_f64().unwrap(),
            "z" => u.z = v.as_f64().unwrap(),
            "still" => u.still = Some(v.as_f64().unwrap()),
            "retreating" => u.retreating = v.as_bool().unwrap(),
            "riding" => u.riding = v.as_u64().unwrap() != 0,
            "path" => u.path = v.as_array().unwrap().iter().map(point).collect(),
            "amove" => u.amove = (!v.is_null()).then(|| point(v)),
            "reinf" => m.recovery.get_mut(id).unwrap().reinforcement = v.as_f64().unwrap(),
            "hpWas" => m.recovery.get_mut(id).unwrap().hp_was = v.as_f64(),
            "woundAt" => m.recovery.get_mut(id).unwrap().wound_at = v.as_i64(),
            "healAt" => m.recovery.get_mut(id).unwrap().heal_at = v.as_i64(),
            "logistics" => {
                m.logistics.insert(id, reserves(v));
            }
            _ => panic!("unsupported recovery input {k}"),
        }
    }
}
#[test]
fn represented_recovery_phases_match_original_source() {
    let fixture: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/match-recovery.json")).unwrap();
    assert_eq!(
        fixture["reference"],
        "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
    );
    assert_eq!(fixture["node"], "v24.21.0");
    let mut count = 0;
    let mut cases = 0;
    let mut paid_halftrack_cases = 0;
    for case in fixture["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let paid_halftrack =
            name == "dismounted-squad-beside-halted-halftrack-paid-ten-second-window";
        cases += 1;
        let (mut m, mut order) = make(case);
        if paid_halftrack {
            paid_halftrack_cases += 1;
            assert_eq!(case["actions"].as_array().unwrap().len(), 401);
            assert_eq!(case["snapshots"].as_array().unwrap().len(), 402);
            assert_eq!(order, [1, 2]);
            let patient = &m.world.game.units[&1];
            let carrier = &m.world.game.units[&2];
            assert_eq!(m.rules.kinds[patient.kind].name, "rifle");
            assert_eq!(m.rules.kinds[carrier.kind].name, "halftrack");
            assert_eq!(patient.hp, 20.);
            assert!(!patient.riding);
            assert_eq!(m.combat.get(1).unwrap().carrier, None);
            assert_eq!(patient.owner, carrier.owner);
            assert_eq!(carrier.still, Some(2.));
            assert_eq!(patient.at(), Point { x: 17., z: 17. });
            assert_eq!(carrier.at(), Point { x: 15., z: 15. });
        }
        let mut frame = None;
        let initial = snapshot(&m, &order, None, &[]);
        let expected = normalize(case["snapshots"][0].clone());
        assert!(
            equal(&initial, &expected),
            "{name} initial {}",
            diff(&initial, &expected, "")
        );
        for (index, a) in case["actions"].as_array().unwrap().iter().enumerate() {
            count += 1;
            let paid_halftrack_before = if paid_halftrack {
                if index == 0 {
                    assert_eq!(a, &json!({"op":"player","slot":0,"changes":{"mp":500}}));
                    None
                } else if index % 2 == 1 {
                    assert_eq!(a, &json!({"op":"prepare","tick":11 + index / 2}));
                    None
                } else {
                    assert_eq!(a, &json!({"op":"reinforce","dt":0.05}));
                    Some((
                        m.world.game.units[&1].hp,
                        m.recovery.get(1).unwrap().reinforcement,
                        m.start.economy.players[0].mp,
                        m.story[0].mp_spent,
                    ))
                }
            } else {
                None
            };
            let mut result = Value::Null;
            let mut shots = Vec::new();
            if let Some(tick) = a["tick"].as_i64() {
                m.world.game.tick = tick;
            }
            match a["op"].as_str().unwrap() {
                "prepare" => frame = Some(m.prepare_recovery(&order).unwrap()),
                "cadence" => {
                    m.heal_and_sweep(frame.as_ref().unwrap(), a["dt"].as_f64().unwrap())
                        .unwrap();
                }
                "reinforce" => m
                    .reinforce(frame.as_ref().unwrap(), a["dt"].as_f64().unwrap())
                    .unwrap(),
                "medic" => {
                    let ids = a["ids"].as_array().map_or_else(
                        || order.clone(),
                        |rows| rows.iter().map(|v| v.as_u64().unwrap() as u32).collect(),
                    );
                    m.heal_wounded(&ids, a["dt"].as_f64().unwrap()).unwrap();
                }
                "sweep" => {
                    let ids = a["ids"].as_array().map_or_else(
                        || order.clone(),
                        |rows| rows.iter().map(|v| v.as_u64().unwrap() as u32).collect(),
                    );
                    m.sweep_mines(&ids).unwrap();
                }
                "repair" => {
                    result = json!(
                        m.repair_building_sections(
                            a["id"].as_u64().unwrap() as u32,
                            a["amount"].as_f64().unwrap()
                        )
                        .unwrap()
                    )
                }
                "building" => {
                    let id = a["id"].as_u64().unwrap() as u32;
                    let hp = a["hp"].as_f64().unwrap();
                    let cells = a["cells"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|v| v.as_u64().unwrap() as usize)
                        .collect::<Vec<_>>();
                    let u = m.world.game.units.get_mut(&id).unwrap();
                    u.hp = hp;
                    u.cells = Some(cells.iter().map(|c| *c as u32).collect());
                    for &c in &cells {
                        m.world
                            .mutate_cell(
                                c,
                                CellChange {
                                    object: Some(b'K'),
                                    ..CellChange::default()
                                },
                            )
                            .unwrap();
                    }
                    let full = Catalog::compiled()
                        .unwrap()
                        .unit("hq")
                        .unwrap()
                        .get("hpPer")
                        .unwrap()
                        .number()
                        .unwrap();
                    m.world
                        .register_building_structure(id, &cells, hp, full)
                        .unwrap();
                }
                "damage" => {
                    let contact = a["contact"]
                        .as_str()
                        .map(str::to_owned)
                        .unwrap_or_else(|| format!("recovery:{index}"));
                    let mut context = Context {
                        tick: m.world.game.tick as u64,
                        seed: &mut m.start.environment_seed,
                        next_id: &mut m.next_unit_id,
                        unit_order: &order,
                        grid: &m.grid,
                        rain: 0.,
                        wet: 0.,
                        wind_angle: 0.,
                        wind_strength: 0.5,
                    };
                    let skip = Default::default();
                    let hit = DamageContext {
                        owner: Some(a["owner"].as_i64().unwrap_or(1) as i32),
                        contact_id: Some(&contact),
                        direction: None,
                        actor_order: None,
                        debit_entity: true,
                        skip_buildings: &skip,
                        memory: None,
                    };
                    let (amount, effects) = m
                        .environment
                        .damage_section(
                            &mut m.world,
                            a["c"].as_u64().unwrap() as usize,
                            a["amount"].as_f64().unwrap(),
                            &mut context,
                            &hit,
                        )
                        .unwrap();
                    result = json!(amount);
                    shots = effects
                        .into_iter()
                        .filter_map(|e| match e {
                            Effect::LastHit { unit, owner } => {
                                m.combat.initialize(unit).last_hit = Some(owner);
                                None
                            }
                            _ => Some(e.to_json()),
                        })
                        .collect();
                }
                "mutate" => {
                    m.world
                        .mutate_cell(
                            a["c"].as_u64().unwrap() as usize,
                            CellChange {
                                object: a["change"]["object"].as_str().map(|v| v.as_bytes()[0]),
                                ..CellChange::default()
                            },
                        )
                        .unwrap();
                }
                "section" => {
                    let meta = m
                        .world
                        .sections
                        .get_mut(&(a["c"].as_u64().unwrap() as usize))
                        .unwrap();
                    for (k, v) in a["changes"].as_object().unwrap() {
                        match k.as_str() {
                            "anchor" => meta.anchor = v.as_bool().unwrap(),
                            _ => panic!("unsupported section input {k}"),
                        }
                    }
                }
                "prime" => prime(&mut m, a["id"].as_u64().unwrap() as u32, &a["changes"]),
                "player" => {
                    let slot = a["slot"].as_u64().unwrap() as usize;
                    for (k, v) in a["changes"].as_object().unwrap() {
                        match k.as_str() {
                            "mp" => m.start.economy.players[slot].mp = v.as_f64().unwrap(),
                            _ => panic!("unsupported player input {k}"),
                        }
                    }
                }
                "point" => {
                    if let Some(index) = a["index"].as_u64() {
                        for (k, v) in a["changes"].as_object().unwrap() {
                            match k.as_str() {
                                "cut" => m.start.points[index as usize].cut = v.as_bool().unwrap(),
                                _ => panic!("unsupported point input {k}"),
                            }
                        }
                    } else {
                        let v = &a["value"];
                        m.start.points.push(Objective {
                            id: Value::Null,
                            at: point(v),
                            vp: 0.,
                            mp: 0.,
                            kind: Some(v["kind"].as_str().unwrap().into()),
                            needs: None,
                            owner: v["owner"].as_u64().map(|v| v as usize),
                            capper: None,
                            progress: 0.,
                            cut: v["cut"].as_bool().unwrap(),
                            on_point: Vec::new(),
                            contested: false,
                        });
                        m.capture_ticks.push(None);
                    }
                }
                "add" => insert(&mut m, &a["unit"], &mut order),
                "aid" => {
                    m.world.aid.insert(
                        a["c"].as_u64().unwrap() as usize,
                        a["owner"].as_u64().unwrap() as usize,
                    );
                }
                "fire" => {
                    let c = a["c"].as_u64().unwrap() as usize;
                    if a["value"].is_null() {
                        m.world.fires.remove(&c);
                    } else {
                        m.world.fires.insert(c, a["value"].as_f64().unwrap());
                    }
                }
                "store" => {
                    let key = a["id"].as_str().unwrap();
                    let id = StoreId::Cache(key.strip_prefix("cache:").unwrap().parse().unwrap());
                    let v = &a["value"];
                    // The source injects a stock-only cache without a capacity field. No refill or
                    // storage-capacity operation runs in this trace; the typed default remains unused.
                    let store = Store {
                        id,
                        owner: v["owner"].as_u64().unwrap() as usize,
                        at: point(v),
                        source: v["source"].as_bool().unwrap(),
                        entity: None,
                        capacity: Stock::default(),
                        buckets: a["buckets"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|v| {
                                (
                                    v[0].as_u64().unwrap() as usize,
                                    Stock {
                                        ammo: v[1]["ammo"].as_f64().unwrap(),
                                        provisions: v[1]["provisions"].as_f64().unwrap(),
                                        fuel: v[1]["fuel"].as_f64().unwrap(),
                                    },
                                )
                            })
                            .collect(),
                        donations: Vec::new(),
                        active: v["active"].as_bool().unwrap(),
                        support: false,
                        point: None,
                    };
                    m.convoys.as_mut().unwrap().stores.push(store);
                }
                "mineOwner" => {
                    m.world.mines.insert(
                        a["c"].as_u64().unwrap() as usize,
                        a["owner"].as_i64().unwrap() as i32,
                    );
                }
                op => panic!("unsupported recovery action {op}"),
            }
            m.grid.rebuild(&m.world.game.units);
            assert!(
                equal(&result, &case["results"][index]),
                "{name} action{index} result {}",
                diff(&result, &case["results"][index], "")
            );
            let actual = snapshot(&m, &order, frame.as_ref(), &shots);
            let expected = normalize(case["snapshots"][index + 1].clone());
            assert!(
                equal(&actual, &expected),
                "{name} action{index} {}",
                diff(&actual, &expected, "")
            );
            if let Some((hp, timer, wallet, spent)) = paid_halftrack_before {
                let frame = frame.as_ref().unwrap();
                assert!(frame.bases.is_empty());
                assert!(frame.depots.is_empty());
                assert!(frame.tents.is_empty());
                assert_eq!(frame.tracks, [2]);
                let patient = &m.world.game.units[&1];
                let reinforcement = m.recovery.get(1).unwrap().reinforcement;
                if patient.hp > hp {
                    assert!(timer + 0.05 >= 4.);
                    assert_eq!(reinforcement, 0.);
                    assert_eq!(patient.hp, ((hp / 20.).ceil() + 1.) * 20.);
                    assert_eq!(m.start.economy.players[0].mp, wallet - 10.);
                    assert_eq!(m.story[0].mp_spent, spent + 10.);
                } else {
                    assert_eq!(patient.hp, hp);
                    assert_eq!(reinforcement, timer + 0.05);
                    assert_eq!(m.start.economy.players[0].mp, wallet);
                    assert_eq!(m.story[0].mp_spent, spent);
                }
                if index / 2 < 80 {
                    assert_eq!(patient.hp, 20.);
                    assert_eq!(m.start.economy.players[0].mp, 500.);
                }
            }
        }
        if paid_halftrack {
            let hp = m.world.game.units[&1].hp;
            let wallet = m.start.economy.players[0].mp;
            assert!(hp > 20. && hp < 100. && wallet < 500. + 200. * 4. / 20.);
            assert!(m.world.game.units[&2].hp > 0.);
            assert!(wallet < 500.);
            assert_eq!(500. - wallet, m.story[0].mp_spent);
            assert_eq!(m.story[0].mp_spent, (hp - 20.) / 20. * 10.);
            // Keep the historical inventory assertions independent of this appended witness.
            cases -= 1;
            count -= case["actions"].as_array().unwrap().len();
        }
    }
    assert_eq!(cases, 23);
    assert_eq!(count, 133);
    assert_eq!(paid_halftrack_cases, 1);
}

fn log09_scene(units: Value) -> (Match, Vec<u32>) {
    let (mut m, order) = make(&json!({
        "map": {
            "name":"Recovery rates", "w":30, "h":20,
            "rows":vec![".".repeat(30);20],
            "spawns":[{"x":0,"y":0},{"x":29,"y":19}],
            "points":[{"x":15,"y":10}]
        },
        "logistics":true,
        "units":units
    }));
    // A reachable unlimited source isolates base recovery from support affordability.
    m.convoys.as_mut().unwrap().stores.push(Store {
        id: StoreId::Source(0),
        owner: 0,
        at: Point { x: 15., z: 15. },
        source: true,
        entity: None,
        capacity: Stock::default(),
        buckets: Vec::new(),
        donations: Vec::new(),
        active: true,
        support: false,
        point: None,
    });
    (m, order)
}

#[test]
fn log09_reinforcement_timer_scales_with_provisions_and_exhaustion() {
    for (provisions, exhausted, rate) in [
        (120., false, 1.),
        (30., false, 1.),
        (29., false, 0.5),
        (0., false, 0.),
        (120., true, 0.),
    ] {
        let (mut m, order) = log09_scene(json!([
            {"id":1,"type":"hq","x":15,"z":15},
            {"id":2,"type":"rifle","x":17,"z":15,"hp":20,
             "logistics":{"ammo":285,"ammoMax":285,"provisions":provisions,"exhausted":exhausted}}
        ]));
        assert!(m.spend_support_provisions(2, 0, 5.).unwrap());
        assert_eq!(m.logistics.get(2).unwrap().provisions, provisions);
        let frame = m.prepare_recovery(&order).unwrap();
        assert_eq!(frame.bases, [1]);
        m.reinforce(&frame, 1.).unwrap();
        assert_eq!(
            m.recovery.get(2).unwrap().reinforcement,
            rate,
            "provisions={provisions}, exhausted={exhausted}"
        );
        assert_eq!(m.world.game.units[&2].hp, 20.);
        assert_eq!(m.start.economy.players[0].mp, 100.);
        if rate > 0. {
            m.reinforce(&frame, 2. / rate - 1.).unwrap();
            assert_eq!(m.world.game.units[&2].hp, 40.);
            assert_eq!(m.recovery.get(2).unwrap().reinforcement, 0.);
            assert_eq!(m.start.economy.players[0].mp, 90.);
            assert_eq!(m.story[0].mp_spent, 10.);
        }
        assert_eq!(m.logistics.get(2).unwrap().provisions, provisions);
    }
}

#[test]
fn log09_base_vehicle_repair_scales_with_provisions_and_exhaustion() {
    for (provisions, exhausted, repaired) in [
        (120., false, 18.),
        (30., false, 18.),
        (29., false, 9.),
        (0., false, 0.),
        (120., true, 0.),
    ] {
        let (mut m, order) = log09_scene(json!([
            {"id":1,"type":"hq","x":15,"z":15},
            {"id":2,"type":"tank","x":17,"z":15,"hp":180,
             "logistics":{"ammo":30,"ammoMax":30,"provisions":provisions,
                          "fuel":180,"emergency":60,"exhausted":exhausted}}
        ]));
        assert!(m.spend_support_provisions(2, 0, 5.).unwrap());
        assert_eq!(m.logistics.get(2).unwrap().provisions, provisions);
        let frame = m.prepare_recovery(&order).unwrap();
        assert_eq!(frame.bases, [1]);
        m.reinforce(&frame, 1.).unwrap();
        assert_eq!(
            m.world.game.units[&2].hp,
            180. + repaired,
            "provisions={provisions}, exhausted={exhausted}"
        );
        assert_eq!(m.start.economy.players[0].mp, 100. - repaired * 0.5);
        assert_eq!(m.story[0].mp_spent, repaired * 0.5);
        assert_eq!(m.logistics.get(2).unwrap().provisions, provisions);
    }
}

#[test]
fn log09_medic_healing_multiplies_patient_and_medic_recovery_rates() {
    for (patient, patient_exhausted, medic, medic_exhausted, healed) in [
        (120., false, 120., false, 5.),
        (30., false, 30., false, 5.),
        (29., false, 120., false, 2.5),
        (120., false, 29., false, 2.5),
        (29., false, 29., false, 1.25),
        (0., false, 120., false, 0.),
        (120., false, 0., false, 0.),
        (120., true, 120., false, 0.),
        (120., false, 120., true, 0.),
    ] {
        let (mut m, order) = log09_scene(json!([
            {"id":1,"type":"medic","x":15,"z":15,
             "logistics":{"ammo":0,"ammoMax":0,"provisions":medic,"exhausted":medic_exhausted}},
            {"id":2,"type":"rifle","x":17,"z":15,"hp":20,
             "logistics":{"ammo":285,"ammoMax":285,"provisions":patient,"exhausted":patient_exhausted}}
        ]));
        // No recent wound, 80 missing HP, and positive medic stocks can fund more than 5 HP.
        // In particular, the two 29-second reserves must multiply to quarter-rate healing.
        assert_eq!(m.recovery.get(2).unwrap().wound_at, None);
        assert!(medic == 0. || medic * 5. > 5.);
        assert!(m.heal_wounded(&order, 1.).unwrap().is_empty());
        assert_eq!(
            m.world.game.units[&2].hp,
            20. + healed,
            "patient=({patient}, {patient_exhausted}), medic=({medic}, {medic_exhausted})"
        );
        assert_eq!(m.logistics.get(2).unwrap().provisions, patient);
        assert_eq!(m.logistics.get(1).unwrap().provisions, medic - healed / 5.);
    }
}

#[test]
fn medic_path_identity_capacity_rejects_before_health_or_wound_writes() {
    let fixture: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/match-recovery.json")).unwrap();
    let case = fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "medic-idle-pathseek-and-exclusions")
        .unwrap();
    let (mut m, order) = make(case);
    m.world.game.units.get_mut(&1).unwrap().path_gen = u64::MAX;
    let before = snapshot(&m, &order, None, &[]);
    let error = m.heal_wounded(&order, 0.5).unwrap_err();
    assert!(error.contains("medic path identity"), "{error}");
    assert_eq!(snapshot(&m, &order, None, &[]), before);
    assert_eq!(m.world.game.units[&1].path_gen, u64::MAX);
}

fn world_recovery_bits(n: f64) -> Value {
    json!(format!("{:016x}", n.to_bits()))
}
fn world_recovery_scene(fixture: &Value) -> Match {
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&fixture["map"]).unwrap()).unwrap();
    Match::new_world(
        &map,
        MatchOptions {
            seed: 701,
            mode: Mode::World,
            players: vec![
                PlayerConfig {
                    name: "Recovery".into(),
                    team: 0,
                    faction: Faction::Usa,
                },
                PlayerConfig {
                    name: "Ally".into(),
                    team: 0,
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
        crate::random::MatchRandom::new(701),
    )
    .unwrap()
}
fn compare_world_recovery(
    m: &Match,
    expected: &Value,
    frame: Option<&RecoveryFrame>,
    context: &str,
) {
    let rows=m.world.game.units.iter().map(|(&id,u)|{let actor=m.actor_policy(u.owner).unwrap();let r=m.recovery.get(id).unwrap();json!({"id":id,"type":m.rules.kinds[u.kind].name,"owner":actor.source_owner,"team":actor.team,"x":world_recovery_bits(u.x),"z":world_recovery_bits(u.z),"hp":world_recovery_bits(u.hp),"reinf":world_recovery_bits(r.reinforcement),"hpWas":r.hp_was.map(world_recovery_bits),"woundAt":r.wound_at,"healAt":r.heal_at,"path":u.path.iter().map(|p|json!({"x":world_recovery_bits(p.x),"z":world_recovery_bits(p.z)})).collect::<Vec<_>>()})}).collect::<Vec<_>>();
    assert_eq!(
        json!(rows),
        expected["actors"],
        "{context}: all authoritative actors"
    );
    assert_eq!(
        json!(m.world.game.units.keys().copied().collect::<Vec<_>>()),
        expected["actorOrder"],
        "{context}: source actor order"
    );
    assert_eq!(json!(m.world.game.tick), expected["tick"]);
    assert_eq!(json!(m.start.environment_seed), expected["seed"]);
    assert_eq!(
        json!(
            m.start
                .economy
                .players
                .iter()
                .map(|p| world_recovery_bits(p.mp))
                .collect::<Vec<_>>()
        ),
        expected["wallets"],
        "{context}: commander wallets"
    );
    assert_eq!(m.start.economy.players.len(), 2);
    assert_eq!(m.world.game.players.len(), 2);
    assert_eq!(expected["localWallet"], world_recovery_bits(0.));
    if expected["story"].is_null() {
        assert!(m.story.iter().all(|s| s.mp_spent == 0.));
    } else {
        assert_eq!(
            json!(
                m.story
                    .iter()
                    .map(|s| json!({"mpSpent":world_recovery_bits(s.mp_spent)}))
                    .collect::<Vec<_>>()
            ),
            expected["story"],
            "{context}: spending"
        );
    }
    assert_eq!(
        json!(
            m.world
                .mine_seen
                .iter()
                .map(|(c, b)| json!([c, *b as i32]))
                .collect::<Vec<_>>()
        ),
        expected["mineSeen"],
        "{context}: signed Source mask, identical bit31"
    );
    assert_eq!(
        json!(
            m.world
                .mines
                .iter()
                .map(|(c, b)| json!([c, b]))
                .collect::<Vec<_>>()
        ),
        expected["mines"]
    );
    if let Some(frame) = frame {
        assert_eq!(json!(frame.units), expected["frame"]["list"]);
        assert_eq!(
            json!(frame.bases),
            expected["frame"]["bases"],
            "{context}: captured production bases"
        );
        assert_eq!(json!(frame.tracks), expected["frame"]["tracks"]);
        let home = frame
            .units
            .iter()
            .copied()
            .filter(|id| m.recovery_home(frame, *id).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            json!(home),
            expected["frame"]["home"],
            "{context}: live Source home predicate"
        );
    } else {
        assert!(expected["frame"].is_null());
    }
}
#[test]
fn captured_world_recovery_matches_original_source_without_local_seat() {
    let fixture: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/world-recovery.json")).unwrap();
    let mut actions = 0;
    for case in fixture["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let mut m = world_recovery_scene(&fixture);
        let random = m.start.random.snapshot();
        let aliases = &case["aliases"];
        let mut frame = None;
        compare_world_recovery(&m, &case["initial"], None, name);
        for (index, a) in case["actions"].as_array().unwrap().iter().enumerate() {
            actions += 1;
            match a["op"].as_str().unwrap() {
                "clock" => m.world.game.tick = a["tick"].as_i64().unwrap(),
                "prime" => {
                    let id = aliases[a["unit"].as_str().unwrap()].as_u64().unwrap() as u32;
                    for (k, v) in a["fields"].as_object().unwrap() {
                        match k.as_str() {
                            "hp" => {
                                m.world.game.units.get_mut(&id).unwrap().hp = v.as_f64().unwrap()
                            }
                            "reinf" => {
                                m.recovery.get_mut(id).unwrap().reinforcement = v.as_f64().unwrap()
                            }
                            "still" => {
                                m.world.game.units.get_mut(&id).unwrap().still =
                                    Some(v.as_f64().unwrap())
                            }
                            k => panic!("unrepresented recovery scene field{k}"),
                        }
                    }
                }
                "nearBase" => {
                    let id = aliases[a["unit"].as_str().unwrap()].as_u64().unwrap() as u32;
                    let base = aliases[a["base"].as_str().unwrap()].as_u64().unwrap() as u32;
                    let at = m.world.game.units[&base].at();
                    let u = m.world.game.units.get_mut(&id).unwrap();
                    u.x = at.x;
                    u.z = at.z;
                }
                "prepare" => {
                    frame = Some(
                        m.prepare_recovery(&m.world.game.units.keys().copied().collect::<Vec<_>>())
                            .unwrap(),
                    )
                }
                "reinforce" => m
                    .reinforce(frame.as_ref().unwrap(), a["dt"].as_f64().unwrap())
                    .unwrap(),
                "heal" => {
                    m.heal_wounded(frame.as_ref().unwrap().units(), a["dt"].as_f64().unwrap())
                        .unwrap();
                }
                "sweep" => m.sweep_mines(frame.as_ref().unwrap().units()).unwrap(),
                "mine" => {
                    let c = aliases["mine"].as_u64().unwrap() as usize;
                    m.world
                        .mutate_cell(
                            c,
                            CellChange {
                                mine: Some(true),
                                ..CellChange::default()
                            },
                        )
                        .unwrap();
                }
                "medic" => {
                    let row = &case["factory"][0];
                    let id = row["id"].as_u64().unwrap() as u32;
                    assert_eq!(m.next_unit_id, id);
                    assert!(!m.world.game.units.contains_key(&id));
                    let kind = m.rules.kind("medic").unwrap();
                    let owner = crate::game::LOCAL_DEFENDER_OWNER;
                    let u = initial_unit(
                        &m.rules,
                        Catalog::compiled().unwrap(),
                        kind,
                        id,
                        owner,
                        Point {
                            x: row["x"].as_f64().unwrap(),
                            z: row["z"].as_f64().unwrap(),
                        },
                    )
                    .unwrap();
                    m.commands.insert(
                        id,
                        UnitControl::new_for_mode(&u, &m.rules, Mode::World).unwrap(),
                    );
                    m.combat.initialize(id);
                    m.firing.initialize(id);
                    m.abilities.initialize(id);
                    m.recovery.initialize(id);
                    m.world.game.units.insert(id, u);
                    m.next_unit_id = id.checked_add(1).unwrap();
                }
                op => panic!("unrepresented World recovery action{op}"),
            }
            m.grid.rebuild(&m.world.game.units);
            compare_world_recovery(
                &m,
                &case["snapshots"][index],
                frame.as_ref(),
                &format!("{name} action{index}"),
            );
            assert_eq!(
                m.start.random.snapshot(),
                random,
                "recovery must preserve recorded streams"
            );
        }
    }
    assert_eq!(actions, 30);
}
#[test]
fn invalid_world_recovery_actor_rejects_before_health_wallet_or_metadata() {
    let fixture: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/world-recovery.json")).unwrap();
    let mut m = world_recovery_scene(&fixture);
    let id = *m.world.game.units.keys().next().unwrap();
    m.world.game.units.get_mut(&id).unwrap().owner = m.world.game.players.len() + 1;
    let health = m
        .world
        .game
        .units
        .iter()
        .map(|(id, u)| (*id, u.hp.to_bits()))
        .collect::<Vec<_>>();
    let wallets = m
        .start
        .economy
        .players
        .iter()
        .map(|p| p.mp.to_bits())
        .collect::<Vec<_>>();
    let metadata = m.recovery.clone();
    assert!(
        m.prepare_recovery(&m.world.game.units.keys().copied().collect::<Vec<_>>())
            .is_err()
    );
    assert_eq!(
        m.world
            .game
            .units
            .iter()
            .map(|(id, u)| (*id, u.hp.to_bits()))
            .collect::<Vec<_>>(),
        health
    );
    assert_eq!(
        m.start
            .economy
            .players
            .iter()
            .map(|p| p.mp.to_bits())
            .collect::<Vec<_>>(),
        wallets
    );
    assert_eq!(m.recovery, metadata);
}
