use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use ww2_core::{
    catalog::Catalog,
    combat::{Impact, SuppressionInput, WeaponDamage},
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::{Player, Point},
    grid::SpatialGrid,
    map::AuthoritativeMap,
    match_combat::{
        CombatState, ContactTarget, DamageWorld, ExplosionSource, FlightSource, RetargetSink,
    },
    match_runtime::{Match, MatchOptions, PlayerConfig},
    unit_spawn::initial_unit,
    weather::WeatherChoice,
    world::{CellChange, World},
};
#[derive(Default)]
struct Timers(BTreeMap<u32, f64>);
impl RetargetSink for Timers {
    fn reset_retarget(&mut self, id: u32) {
        self.0.insert(id, 0.);
    }
}
fn point(v: &Value) -> Point {
    Point {
        x: v["x"].as_f64().unwrap(),
        z: v["z"].as_f64().unwrap(),
    }
}
fn runs(values: impl IntoIterator<Item = Value>) -> Value {
    let mut r: Vec<[Value; 2]> = Vec::new();
    for v in values {
        if let Some(last) = r.last_mut().filter(|r| r[0] == v) {
            last[1] = json!(last[1].as_u64().unwrap() + 1);
        } else {
            r.push([v, json!(1)]);
        }
    }
    json!(r)
}
fn bits(v: &[f32]) -> Value {
    runs(v.iter().map(|v| json!(v.to_bits())))
}
fn body(v: &ww2_core::environment_debris::DebrisBody) -> Value {
    json!({"id":v.id,"kind":v.kind.as_str(),"x":v.x,"y":v.y,"z":v.z,"vx":v.vx,"vy":v.vy,"vz":v.vz,"dir":v.dir,"material":v.material,"mass":v.mass,"radius":v.radius,"origin":{"x":v.origin.x,"z":v.origin.z},"gravity":v.gravity,"drag":v.drag,"stamp":v.stamp,"born":v.born,"contacts":v.contacts,"settled":v.settled,"tilt":v.tilt})
}
fn row(v: &ww2_core::world::CellRow) -> Value {
    let d = &v.data;
    let mut data = json!({"ground":char::from(d.ground).to_string(),"object":char::from(d.object).to_string(),"material":d.material,"groundMaterial":d.ground_material,"groundWear":d.ground_wear});
    if let Some(f) = d.trench_front {
        data["trenchFront"] = json!(f);
    }
    if d.mine {
        data["mine"] = json!(true);
    }
    if let Some(s) = &d.section {
        data["section"] = json!({"id":s.id,"structureId":s.structure_id,"state":s.state,"hp":s.hp,"maxHp":s.max_hp,"material":s.material,"anchor":s.anchor});
    }
    json!([
        v.cell,
        char::from(v.ch).to_string(),
        v.height,
        v.state,
        data
    ])
}
fn snapshot(m: &Match, timers: &Timers, groups: &BTreeMap<u32, BTreeSet<u32>>) -> Value {
    let w = &m.world;
    let g = &w.game;
    let structures=w.structures.iter().map(|s|json!([s.id,s.kind,s.section_ids.iter().map(|id|{let (meta,h)=w.section_by_id(id).unwrap();let mut v=json!({"id":id,"hp":h.hp,"state":if h.failed{"failed"}else{match meta.appearance{ww2_core::world::Appearance::Intact=>"intact",ww2_core::world::Appearance::Damaged=>"damaged",ww2_core::world::Appearance::Burning=>"burning"}},"rubble":meta.rubble});if let Some(i)=meta.failure.impulse{v["impulse"]=json!(i);}
if let Some(t)=meta.failure.failed_at{v["failedAt"]=json!(t);}
if let Some(d)=meta.failure.direction{v["direction"]=json!({"x":d.x,"z":d.z});}
if let Some(t)=meta.failure.settled_at{v["settledAt"]=json!(t);}v}).collect::<Vec<_>>()])).collect::<Vec<_>>();
    let wrecks = g
        .wrecks
        .iter()
        .map(|w| {
            let meta = &m.environment.wreck_metadata()[&w.id.unwrap()];
            json!([
                w.id.unwrap(),
                m.rules.kinds[meta.kind].name,
                meta.owner,
                w.x,
                w.z,
                meta.rotation,
                w.cell.map_or(-1, |c| c as i64),
                meta.motion.as_ref().map(body)
            ])
        })
        .collect::<Vec<_>>();
    let falling = m
        .environment
        .falling_sections()
        .iter()
        .map(|v| {
            let mut b = body(&v.body);
            for (key, value) in [
                ("section", json!(v.section)),
                ("c", json!(v.cell)),
                ("bridge", json!(v.bridge)),
                ("at", json!(v.at)),
                ("until", json!(v.until)),
                ("impulse", json!(v.impulse)),
            ] {
                b[key] = value;
            }
            b
        })
        .collect::<Vec<_>>();
    json!({"units":g.units.iter().map(|(id,u)|{let c=m.combat.get(*id).unwrap();json!([id,u.hp,u.supp,u.x,u.z,c.carrier.unwrap_or(0),c.cargo,c.xp,c.last_hit,c.death_impulse.map(|v|json!({"dir":v.direction,"impulse":v.magnitude})),c.hit_by.unwrap_or(0),c.hit_at,u.hit_from.map(|v|json!({"x":v.x,"z":v.z})),c.at_hit,timers.0[id],u.sprint])}).collect::<Vec<_>>(),"retained":m.combat.iter().map(|(id,m)|json!([id,m.xp])).collect::<Vec<_>>(),"suppressed":groups.iter().map(|(id,s)|json!([id,s])).collect::<Vec<_>>(),"wallets":m.start.economy.players.iter().map(|p|p.mp).collect::<Vec<_>>(),"chars":String::from_utf8(g.chars.clone()).unwrap(),"hp":bits(&g.cell_hp),"flags":runs(g.flags.iter().map(|v|json!(v))),"height":g.height.as_ref().map(|v|runs(v.iter().map(|v|json!(v)))),"wear":bits(g.wear.as_ref().unwrap()),"burnt":runs(w.burnt.iter().map(|v|json!(v))),"structures":structures,"wrecks":wrecks,"falling":falling,"contacts":m.environment.contacts(),"fires":w.fires.iter().map(|(c,t)|json!([c,t])).collect::<Vec<_>>(),"smokes":g.smokes.iter().map(|s|json!({"id":s.id,"x":s.x,"z":s.z,"r":s.r,"t":s.remaining})).collect::<Vec<_>>(),"log":w.cell_log().iter().map(row).collect::<Vec<_>>(),"journal":w.journal(),"versions":[g.terrain_version as u64,w.versions.obstruction,w.versions.infantry,w.versions.vehicle,w.versions.naval],"seed":m.start.environment_seed,"nextId":m.next_unit_id})
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
fn make(case: &Value) -> Match {
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&case["map"]).unwrap()).unwrap();
    let options = MatchOptions {
        seed: 1,
        mode: Mode::Default,
        players: (0..2)
            .map(|slot| PlayerConfig {
                name: format!("p{slot}"),
                team: slot,
                faction: Faction::try_from(slot as u8).unwrap(),
            })
            .collect(),
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
    m.world.game.players = (0..2)
        .map(|slot| Player {
            slot,
            team: slot as i32,
            visible: Vec::new(),
        })
        .collect();
    m.world.game.tick = 10;
    m.world.game.rain = 0.;
    m.world.game.wet = Some(0.);
    m.combat = CombatState::default();
    m.start.environment_seed = 123;
    m.start.wind = (0., 0.5);
    m.next_unit_id = 1000;
    for p in &mut m.start.economy.players {
        p.mp = 100.;
    }
    for v in case["units"].as_array().unwrap() {
        let id = v["id"].as_u64().unwrap() as u32;
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
        u.supp = v["supp"].as_f64().unwrap_or(0.);
        u.retreating = v["retreating"].as_bool().unwrap_or(false);
        u.sprint = v["sprint"].as_f64().unwrap_or(0.);
        u.garrison = v["garrison"].as_u64().map(|v| v as usize);
        u.riding = v["carrier"].is_u64();
        if let Some(path) = v["path"].as_array() {
            u.path = path.iter().map(point).collect();
        }
        let c = m.combat.initialize(id);
        c.xp = v["xp"].as_f64().unwrap_or(0.);
        c.last_hit = v["lastHit"].as_i64().map(|v| v as i32);
        c.carrier = v["carrier"].as_u64().map(|v| v as u32);
        c.cargo = v["cargo"].as_array().map_or_else(Vec::new, |v| {
            v.iter().map(|v| v.as_u64().unwrap() as u32).collect()
        });
        m.world.game.units.insert(id, u);
    }
    m.grid = SpatialGrid::new(&m.world.game.units, 16.);
    m
}
#[test]
fn ordered_damage_riders_terrain_and_death_match_original_source() {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/match-combat.json")).unwrap();
    assert_eq!(
        fixture["reference"],
        "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
    );
    assert_eq!(fixture["node"], "v24.21.0");
    let mut count = 0;
    for case in fixture["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let mut m = make(case);
        let mut timers = Timers(
            case["units"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| {
                    (
                        v["id"].as_u64().unwrap() as u32,
                        v["retarget"].as_f64().unwrap_or(7.),
                    )
                })
                .collect(),
        );
        let mut groups = BTreeMap::<u32, BTreeSet<u32>>::new();
        let actual = snapshot(&m, &timers, &groups);
        assert!(
            equal(&actual, &case["snapshots"][0]),
            "{name} initial {}",
            diff(&actual, &case["snapshots"][0], "")
        );
        let sources = case["units"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| {
                (
                    v["id"].as_u64().unwrap() as u32,
                    FlightSource {
                        shooter: v["id"].as_u64().unwrap() as u32,
                        kind: m.rules.kind(v["type"].as_str().unwrap()).unwrap(),
                        owner: v["owner"].as_i64().unwrap_or(0) as i32,
                        airborne: false,
                    },
                )
            })
            .collect::<BTreeMap<_, _>>();
        for (index, a) in case["actions"].as_array().unwrap().iter().enumerate() {
            count += 1;
            let order = m.world.game.units.keys().copied().collect::<Vec<_>>();
            let mut value = Value::Null;
            let mut shots = Vec::new();
            if a["op"] == "retire" {
                let order = a["order"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_u64().unwrap() as u32)
                    .collect::<Vec<_>>();
                let result = m.retire_dead(&order).unwrap();
                shots = result.shots;
            } else if a["op"] == "debris" {
                let mut ctx = ww2_core::environment::Context {
                    tick: 10,
                    seed: &mut m.start.environment_seed,
                    next_id: &mut m.next_unit_id,
                    unit_order: &order,
                    grid: &m.grid,
                    rain: 0.,
                    wet: 0.,
                    wind_angle: 0.,
                    wind_strength: 0.5,
                };
                m.environment
                    .step_sections(&mut m.world, a["dt"].as_f64().unwrap(), &mut ctx)
                    .unwrap();
                m.environment
                    .step_wrecks(&mut m.world, a["dt"].as_f64().unwrap(), &mut ctx)
                    .unwrap();
            } else if a["op"] == "mutate" {
                let c = a["c"].as_u64().unwrap() as usize;
                m.world
                    .mutate_cell(
                        c,
                        CellChange {
                            object: a["change"]["object"].as_str().map(|s| s.as_bytes()[0]),
                            ..Default::default()
                        },
                    )
                    .unwrap();
            } else if a["op"] == "building" {
                let id = a["id"].as_u64().unwrap() as u32;
                let hp = a["hp"].as_f64().unwrap();
                let cells = a["cells"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_u64().unwrap() as usize)
                    .collect::<Vec<_>>();
                m.world.game.units.get_mut(&id).unwrap().hp = hp;
                m.world.game.units.get_mut(&id).unwrap().cells =
                    Some(cells.iter().map(|v| *v as u32).collect());
                m.world
                    .register_building_structure(
                        id,
                        &cells,
                        hp,
                        Catalog::compiled()
                            .unwrap()
                            .unit("hq")
                            .unwrap()
                            .get("hpPer")
                            .unwrap()
                            .number()
                            .unwrap(),
                    )
                    .unwrap();
            } else if a["op"] == "prime" {
                let id = a["id"].as_u64().unwrap() as u32;
                let u = m.world.game.units.get_mut(&id).unwrap();
                if let Some(h) = a["hp"].as_f64() {
                    u.hp = h;
                }
                if let Some(h) = a["xp"].as_f64() {
                    m.combat.get_mut(id).unwrap().xp = h;
                }
                if !a["hitFrom"].is_null() {
                    u.hit_from = Some(point(&a["hitFrom"]));
                }
                if let Some(out) = a["out"].as_bool() {
                    m.start.economy.players[id as usize].out = out;
                }
                if let Some(h) = a["lastHit"].as_i64() {
                    m.combat.get_mut(id).unwrap().last_hit = Some(h as i32);
                }
            } else if a["op"] == "sync" {
                let id = a["id"].as_u64().unwrap() as u32;
                let mut ctx = ww2_core::environment::Context {
                    tick: 10,
                    seed: &mut m.start.environment_seed,
                    next_id: &mut m.next_unit_id,
                    unit_order: &order,
                    grid: &m.grid,
                    rain: 0.,
                    wet: 0.,
                    wind_angle: 0.,
                    wind_strength: 0.5,
                };
                shots = m
                    .environment
                    .sync_building_sections(
                        &mut m.world,
                        id,
                        m.combat.get(id).unwrap().last_hit,
                        &mut ctx,
                    )
                    .unwrap();
            } else {
                let shooter = a["shooter"].as_u64().map(|v| v as u32);
                let mut damage = DamageWorld {
                    world: &mut m.world,
                    combat: &mut m.combat,
                    grid: &mut m.grid,
                    rules: &m.rules,
                    logistics: &m.logistics,
                    production: &m.production,
                    mode: Mode::Default,
                    tech_enabled: false,
                    environment: &mut m.environment,
                    environment_seed: &mut m.start.environment_seed,
                    next_id: &mut m.next_unit_id,
                    unit_order: &order,
                    wind_angle: 0.,
                    wind_strength: 0.5,
                };
                if a["op"] == "blast" {
                    let profile = if let Some(support) = a["support"].as_str() {
                        ExplosionSource::support(support).unwrap()
                    } else {
                        ExplosionSource::weapon(
                            m.rules.kind(a["kind"].as_str().unwrap()).unwrap(),
                            &m.rules,
                        )
                        .unwrap()
                    };
                    shots = damage
                        .explosion(
                            point(&a["at"]),
                            a["radius"].as_f64().unwrap(),
                            profile,
                            a["owner"].as_i64().unwrap_or(1) as i32,
                            a["contact"].as_str(),
                            (!a["direction"].is_null()).then(|| point(&a["direction"])),
                            None,
                            &mut timers,
                        )
                        .unwrap();
                } else {
                    let source = sources[&shooter.unwrap()];
                    let kind = &m.rules.kinds[source.kind].name;
                    let from = if !a["from"].is_null() {
                        point(&a["from"])
                    } else {
                        let v = case["units"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .find(|v| v["id"].as_u64() == Some(u64::from(source.shooter)))
                            .unwrap();
                        point(v)
                    };
                    if a["op"] == "suppress" {
                        let group = a["group"].as_u64().unwrap_or(0) as u32;
                        damage
                            .suppress(
                                a["target"].as_u64().unwrap() as u32,
                                SuppressionInput {
                                    owner: source.owner,
                                    shooter: source.shooter,
                                    amount: a["amount"].as_f64().unwrap_or(10.),
                                    scale: a["suppScale"].as_f64(),
                                    flame: WeaponDamage::from_catalog(kind)
                                        .unwrap()
                                        .flame
                                        .is_some(),
                                    launch_origin: from,
                                    already_suppressed: false,
                                    cover_mul: 1.,
                                    cover_behind: 0.,
                                },
                                &mut timers,
                                groups.entry(group).or_default(),
                            )
                            .unwrap();
                    } else {
                        let target = a["target"].as_u64().map(|v| v as u32);
                        let at = if !a["at"].is_null() {
                            point(&a["at"])
                        } else if let Some(id) = target {
                            damage.world.game.units[&id].at()
                        } else {
                            Point {
                                x: (a["c"].as_u64().unwrap() % 20) as f64 * 2. + 1.,
                                z: (a["c"].as_u64().unwrap() / 20) as f64 * 2. + 1.,
                            }
                        };
                        let impact = Impact {
                            owner: source.owner,
                            shooter: source.shooter,
                            weapon: WeaponDamage::from_catalog(kind).unwrap(),
                            damage_scale: a["scale"].as_f64(),
                            energy: a["energy"].as_f64().unwrap_or(1.),
                            armor_piercing: false,
                            ap_naval_only: false,
                            ap_multiplier: 1.5,
                            launch_origin: from,
                            at,
                            direction: if !a["direction"].is_null() {
                                point(&a["direction"])
                            } else {
                                Point { x: 1., z: 0. }
                            },
                        };
                        if a["op"] == "damage" {
                            value = json!(
                                damage
                                    .direct_hit(source, target.unwrap(), impact, &mut timers)
                                    .unwrap()
                            );
                        } else {
                            let (hit, e) = damage
                                .local_contact(
                                    source,
                                    ContactTarget::Cell(a["c"].as_u64().unwrap() as usize),
                                    impact,
                                    a["contact"].as_str().unwrap(),
                                    None,
                                    &mut timers,
                                )
                                .unwrap();
                            value = json!(hit);
                            shots = e;
                        }
                    }
                }
            }
            shots.retain(|e| {
                if let ww2_core::environment::Effect::LastHit { unit, owner } = e {
                    m.combat.initialize(*unit).last_hit = Some(*owner);
                    false
                } else {
                    true
                }
            });
            let result =
                json!({"value":value,"shots":shots.iter().map(|s|s.to_json()).collect::<Vec<_>>()});
            assert!(
                equal(&result, &case["results"][index]),
                "{name} action{index} result {}",
                diff(&result, &case["results"][index], "")
            );
            let actual = snapshot(&m, &timers, &groups);
            assert!(
                equal(&actual, &case["snapshots"][index + 1]),
                "{name} action{index} {}",
                diff(&actual, &case["snapshots"][index + 1], "")
            );
        }
    }
    assert_eq!(count, 35);
}

#[test]
fn event_and_death_preflight_preserve_canonical_state_on_invalid_input() {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/match-combat.json")).unwrap();
    let mut m = make(&fixture["cases"][0]);
    let mut timers = Timers(m.world.game.units.keys().map(|id| (*id, 7.)).collect());
    let groups = BTreeMap::new();
    let before = snapshot(&m, &timers, &groups);
    let order = m.world.game.units.keys().copied().collect::<Vec<_>>();
    let mut view = DamageWorld {
        world: &mut m.world,
        combat: &mut m.combat,
        grid: &mut m.grid,
        rules: &m.rules,
        logistics: &m.logistics,
        production: &m.production,
        mode: Mode::Default,
        tech_enabled: false,
        environment: &mut m.environment,
        environment_seed: &mut m.start.environment_seed,
        next_id: &mut m.next_unit_id,
        unit_order: &order,
        wind_angle: 0.,
        wind_strength: 0.5,
    };
    let events = [
        ww2_core::combat::CombatEvent::Health { unit: 2, value: 0. },
        ww2_core::combat::CombatEvent::Health {
            unit: 999,
            value: 1.,
        },
    ];
    assert!(
        view.apply_events(&events, &mut timers, &mut BTreeSet::new())
            .is_err()
    );
    for invalid in [
        ww2_core::combat::CombatEvent::Suppression {
            unit: 2,
            value: -1.,
        },
        ww2_core::combat::CombatEvent::LastHit {
            unit: 2,
            owner: 999,
        },
    ] {
        let batch = [
            ww2_core::combat::CombatEvent::Health { unit: 2, value: 0. },
            invalid,
        ];
        assert!(
            view.apply_events(&batch, &mut timers, &mut BTreeSet::new())
                .is_err()
        );
    }
    assert_eq!(snapshot(&m, &timers, &groups), before);
    m.world.game.units.get_mut(&1).unwrap().hp = 0.;
    m.world.game.units.get_mut(&2).unwrap().hp = 0.;
    m.combat.get_mut(2).unwrap().last_hit = Some(999);
    let before = snapshot(&m, &timers, &groups);
    assert!(m.retire_dead(&[1, 2, 3]).is_err());
    assert_eq!(snapshot(&m, &timers, &groups), before);
}

#[test]
fn mid_tick_removal_keeps_surviving_spatial_bucket_order() {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/match-combat.json")).unwrap();
    let mut m = make(&fixture["cases"][0]);
    // A moved older unit appends after a newer one in its destination bucket.
    m.world.game.units.get_mut(&1).unwrap().x = 19.;
    m.grid.update(1, &m.world.game.units[&1]);
    m.world.game.units.get_mut(&1).unwrap().x = 5.;
    m.grid.update(1, &m.world.game.units[&1]);
    let before = m
        .grid
        .unordered(&m.world.game.units, (7., 5.), 8., |_, _| true);
    assert_eq!(before, [2, 3, 1]);
    m.world.game.units.remove(&2);
    m.grid.remove_unit(2);
    let after = m
        .grid
        .unordered(&m.world.game.units, (7., 5.), 8., |_, _| true);
    assert_eq!(after, [3, 1]);
    let owned = m.grid.owned_by(&m.world.game.units, 1);
    assert_eq!(owned, [3]);
}
