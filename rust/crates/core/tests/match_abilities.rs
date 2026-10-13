// Compare bounded canonical ability phases, not raw JavaScript save objects.
// Object-valued control fields compare their represented presence or boolean state.
// React has no canonical owner here. Raw tables, suppression sets and duplicate launch
// duration/stamp fields are omitted; blast values, events, geometry and timing are checked.
use serde_json::{Value, json};
use std::collections::VecDeque;
use ww2_core::{
    catalog::Catalog,
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::{Player, Point, TRENCH, Traffic},
    grid::SpatialGrid,
    logistics::{self, LogisticsState},
    map::AuthoritativeMap,
    match_abilities::{AbilityState, AutocastTarget},
    match_combat::CombatState,
    match_commands::{Command, Facing, MatchCommands, MoveOrder, MoveOutcome, UnitControl},
    match_firing::FiringState,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    math,
    random::{MatchRandom, StreamId},
    unit_spawn::initial_unit,
    weather::WeatherChoice,
    world::World,
};
fn point(v: &Value) -> Point {
    Point {
        x: v["x"].as_f64().unwrap(),
        z: v["z"].as_f64().unwrap(),
    }
}
fn at(p: Point) -> Value {
    json!({"x":p.x,"z":p.z})
}
fn truth(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::Number(v) => v.as_f64() != Some(0.),
        _ => true,
    }
}
fn order(v: &Value, id: u32) -> Command {
    match v["t"].as_str().unwrap() {
        "move" => Command::Move {
            orders: vec![MoveOrder {
                unit: i64::from(id),
                at: point(v),
            }],
            control_ids: None,
            attack_move: false,
            queue: false,
            together: false,
            face: None,
        },
        "ability" => Command::Ability {
            ids: vec![i64::from(id)],
            at: Some(point(v)),
            queue: false,
            automatic: false,
        },
        kind => panic!("unrepresented waiting order {kind}"),
    }
}
fn command(v: &Value, automatic: bool) -> Command {
    assert_eq!(v["t"], "ability");
    Command::Ability {
        ids: v["ids"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_i64().unwrap())
            .collect(),
        at: if v["x"].is_number() && v["z"].is_number() {
            Some(point(v))
        } else {
            None
        },
        queue: v["queue"].as_bool().unwrap_or(false),
        automatic,
    }
}
fn orders(v: &VecDeque<Command>) -> Value {
    json!(
        v.iter()
            .map(|c| match c {
                Command::Move { orders, .. } => {
                    assert_eq!(orders.len(), 1);
                    json!({"t":"move","x":orders[0].at.x,"z":orders[0].at.z})
                }
                Command::Ability {
                    ids, at: Some(p), ..
                } => json!({"t":"ability","ids":ids,"x":p.x,"z":p.z}),
                _ => panic!("unrepresented queued control"),
            })
            .collect::<Vec<_>>()
    )
}
fn apply_inputs(m: &mut Match, id: u32, v: &Value) {
    let u = m.world.game.units.get_mut(&id).unwrap();
    for (k, value) in v.as_object().unwrap() {
        match k.as_str() {
            "id" | "type" | "owner" => {}
            "x" => u.x = value.as_f64().unwrap(),
            "z" => u.z = value.as_f64().unwrap(),
            "rot" => u.rot = value.as_f64().unwrap(),
            "hp" => u.hp = value.as_f64().unwrap(),
            "supp" => u.supp = value.as_f64().unwrap(),
            "sprint" => u.sprint = value.as_f64().unwrap(),
            "repath" => u.repath = value.as_f64().unwrap(),
            "stuck" => u.stuck = value.as_f64().unwrap(),
            "riding" => u.riding = value.as_u64().unwrap() != 0,
            "retreating" => u.retreating = value.as_bool().unwrap(),
            "garrison" => u.garrison = value.as_i64().filter(|v| *v >= 0).map(|v| v as usize),
            "board" => u.board = value.as_u64().unwrap() as u32,
            "path" => u.path = value.as_array().unwrap().iter().map(point).collect(),
            "amove" => u.amove = (!value.is_null()).then(|| point(value)),
            "reverse" => u.reverse = (!value.is_null()).then(|| point(value)),
            "worldGoal" => u.world_goal = (!value.is_null()).then(|| point(value)),
            "dig" => u.dig = truth(value),
            "build" => u.build = truth(value),
            "entrench" => u.entrench = truth(value),
            "drift" => u.drift = truth(value),
            "holdPos" => u.hold_pos = value.as_bool().unwrap(),
            "traffic" => {
                u.traffic = truth(value).then_some(Traffic {
                    goal: u.at(),
                    legs: Vec::new(),
                    passing: false,
                    by: value["id"].as_u64().unwrap_or(0) as u32,
                    until: 0.,
                })
            }
            "trafficWait" => u.traffic_wait = value.as_f64().unwrap(),
            "follow" => {
                m.commands.units.get_mut(&id).unwrap().follow = value.as_u64().unwrap() as u32
            }
            "auto" => m.commands.units.get_mut(&id).unwrap().auto_cast = value.as_bool().unwrap(),
            "autoRetreat" => {
                m.commands.units.get_mut(&id).unwrap().auto_retreat = value.as_bool().unwrap()
            }
            "holdFire" => {
                m.commands.units.get_mut(&id).unwrap().hold_fire = value.as_bool().unwrap()
            }
            "operative" => m.commands.units.get_mut(&id).unwrap().operative = truth(value),
            "attackId" => {
                let target = value.as_u64().unwrap() as u32;
                m.commands.units.get_mut(&id).unwrap().attack_id = target;
                u.attack = target != 0;
            }
            "enter" => {
                m.commands.units.get_mut(&id).unwrap().enter =
                    value.as_i64().filter(|v| *v >= 0).map(|v| v as usize)
            }
            "fireAt" => {
                m.commands.units.get_mut(&id).unwrap().fire_at =
                    value.as_i64().filter(|v| *v >= 0).map(|v| v as usize)
            }
            "face" => {
                m.commands.units.get_mut(&id).unwrap().face = truth(value).then_some(Facing {
                    angle: 0.,
                    at: Point {
                        x: value["x"].as_f64().unwrap_or(0.),
                        z: value["z"].as_f64().unwrap_or(0.),
                    },
                })
            }
            "orders" => {
                m.commands.units.get_mut(&id).unwrap().orders = value
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| order(v, id))
                    .collect()
            }
            "nade" => {
                m.commands.units.get_mut(&id).unwrap().nade =
                    (!value.is_null()).then(|| point(value));
                let a = m.abilities.get_mut(id).unwrap();
                a.aimed_auto = value["auto"].as_bool().unwrap_or(false);
                a.aimed_walked = value["walked"].as_bool().unwrap_or(false);
            }
            "cd" => m.abilities.get_mut(id).unwrap().cooldown = value.as_f64().unwrap(),
            "buff" => m.abilities.get_mut(id).unwrap().buff = value.as_f64().unwrap(),
            "ap" => m.abilities.get_mut(id).unwrap().ap = value.as_bool().unwrap(),
            "cooldown" => m.firing.get_mut(id).unwrap().cooldown = value.as_f64().unwrap(),
            "retarget" => m.firing.get_mut(id).unwrap().retarget = value.as_f64().unwrap(),
            "targetId" => m.firing.get_mut(id).unwrap().target_id = value.as_u64().unwrap() as u32,
            "atHit" => m.combat.get_mut(id).unwrap().at_hit = value.as_f64(),
            "logistics" => {
                let definition =
                    logistics::Definition::from_catalog(&m.rules.kinds[u.kind].name).unwrap();
                m.logistics
                    .initialize_unit(
                        logistics::Context {
                            enabled_override: Some(true),
                            scenario: false,
                            mode: logistics::Mode::Conquest,
                        },
                        logistics::UnitFacts {
                            id,
                            kind: &m.rules.kinds[u.kind].name,
                            owner: Some(u.owner),
                            owner_exists: true,
                            guard_home: false,
                            riding: u.riding,
                        },
                        Some(&definition),
                    )
                    .unwrap();
                let reserves = m.logistics.get_mut(id).unwrap();
                if let Some(v) = value["ammo"].as_f64() {
                    reserves.ammo = v;
                }
                if let Some(v) = value["forced"].as_bool() {
                    reserves.forced = v;
                }
                if let Some(v) = value["stranded"].as_bool() {
                    reserves.stranded = v;
                }
            }
            key => panic!("unrepresented ability actor input {key}"),
        }
    }
}
fn make(case: &Value) -> (Match, Vec<u32>) {
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&case["map"]).unwrap()).unwrap();
    let options = MatchOptions {
        seed: 701,
        mode: Mode::Conquest,
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
    m.world.game.tick = 10;
    m.world.game.rain = 0.;
    m.world.game.wet = Some(0.);
    m.world.game.players = (0..2)
        .map(|slot| Player {
            slot,
            team: slot as i32,
            visible: Vec::new(),
        })
        .collect();
    m.start.environment_seed = 123;
    m.start.wind = (0., 0.5);
    m.start.random = MatchRandom::new(701);
    m.next_unit_id = 1000;
    for p in &mut m.start.economy.players {
        p.mp = 100.;
        p.mun = 0.;
        p.fuel = 0.;
    }
    m.commands = MatchCommands::default();
    m.abilities = AbilityState::default();
    m.firing = FiringState::default();
    m.firing.next_flight = 2000;
    m.combat = CombatState::default();
    m.logistics = LogisticsState::default();
    m.grid = SpatialGrid::new(&m.world.game.units, 16.);
    let mut order = Vec::new();
    for v in case["units"].as_array().unwrap() {
        let id = v["id"].as_u64().unwrap() as u32;
        let kind = m.rules.kind(v["type"].as_str().unwrap()).unwrap();
        let owner = v["owner"].as_u64().unwrap_or(0) as usize;
        let u = initial_unit(
            &m.rules,
            Catalog::compiled().unwrap(),
            kind,
            id,
            owner,
            point(v),
        )
        .unwrap();
        let mut control = UnitControl::new(&u, &m.rules).unwrap();
        control.follow = 77;
        control.auto_cast = false;
        control.auto_retreat = false;
        m.world.game.units.insert(id, u);
        m.commands.units.insert(id, control);
        m.abilities.initialize(id);
        m.firing.initialize(id);
        m.combat.initialize(id);
        apply_inputs(&mut m, id, v);
        m.grid.update(id, &m.world.game.units[&id]);
        order.push(id);
    }
    for p in &mut m.world.game.players {
        p.visible = order.clone();
    }
    (m, order)
}
fn combat_random(m: &Match) -> (String, Vec<f64>) {
    let snapshot = m.start.random.snapshot();
    assert_eq!(snapshot.seed, 701);
    let mut paired = MatchRandom::new(snapshot.seed);
    paired.next_fraction(StreamId::Combat);
    let first = paired
        .snapshot()
        .streams
        .iter()
        .find(|(id, _)| *id == StreamId::Combat)
        .unwrap()
        .1;
    let initial = first.wrapping_sub(0x9e37_79b9_7f4a_7c15);
    let current = snapshot
        .streams
        .iter()
        .find(|(id, _)| *id == StreamId::Combat)
        .map_or(initial, |(_, s)| *s);
    let mut paired = MatchRandom::new(snapshot.seed);
    let mut state = initial;
    let mut draws = Vec::new();
    while state != current {
        assert!(draws.len() < 4096, "unexpected Combat RNG consumption");
        draws.push(paired.next_fraction(StreamId::Combat));
        state = paired.snapshot().streams[0].1;
    }
    (format!("{current:016x}"), draws)
}
fn blast(source: &ww2_core::match_combat::ExplosionSource) -> Value {
    json!({"inf":source.infantry,"veh":source.vehicle,"terrain":source.terrain,"supp":source.suppression,"scar":source.scar,"antiGarrison":source.anti_garrison})
}
fn snapshot(m: &Match, order: &[u32], shots: &[Value]) -> Value {
    let g = &m.world.game;
    let (state, draws) = combat_random(m);
    let units=order.iter().map(|id|{
        let u=&g.units[id];let c=&m.commands.units[id];let a=m.abilities.get(*id).unwrap();let f=m.firing.get(*id).unwrap();
        let outcome=c.outcome.map(|o|match o{MoveOutcome::Interrupted=>"interrupted",MoveOutcome::Moving=>"moving",MoveOutcome::Unreachable=>"unreachable",MoveOutcome::Arrived=>"completed"});
        let mut nade=c.nade.map(at).unwrap_or(Value::Null);if !nade.is_null(){if a.aimed_auto{nade["auto"]=json!(true);}if a.aimed_walked{nade["walked"]=json!(true);}}
        json!({"id":id,"hp":u.hp,"x":u.x,"z":u.z,"cd":a.cooldown,"buff":a.buff,"ap":a.ap,"sprint":u.sprint,"supp":u.supp,"nade":nade,"path":u.path.iter().copied().map(at).collect::<Vec<_>>(),"retreating":u.retreating,"logistics":m.logistics.get(*id).map(|r|json!({"ammo":r.ammo,"forced":r.forced,"stranded":r.stranded})),"control":{"follow":c.follow,"auto":c.auto_cast,"autoRetreat":c.auto_retreat,"holdFire":c.hold_fire,"holdPos":u.hold_pos,"attackId":c.attack_id,"targetId":f.target_id,"orders":orders(&c.orders),"face":c.face.is_some(),"entrench":u.entrench,"dig":u.dig,"build":u.build,"enter":c.enter.map_or(-1,|v|v as i64),"board":u.board,"amove":u.amove.map(at),"fireAt":c.fire_at.map_or(-1,|v|v as i64),"stuck":u.stuck,"drift":u.drift,"reverse":u.reverse.map(at),"repath":u.repath,"cooldown":f.cooldown,"retarget":f.retarget,"traffic":u.traffic.is_some(),"trafficWait":u.traffic_wait,"moveOutcome":outcome,"moveOutcomeTick":c.outcome_tick,"moveResult":if c.outcome==Some(MoveOutcome::Unreachable){json!(["unreachable",c.outcome_tick])}else{Value::Null},"worldGoal":u.world_goal.map(at),"garrison":u.garrison.map_or(-1,|v|v as i64),"shotAt":u.shot_at}})
    }).collect::<Vec<_>>();
    let flights=m.firing.flights.values().map(|p|json!({"id":p.id,"owner":p.owner,"shooter":p.shooter,"kind":m.rules.kinds[p.source_kind].name,"x":p.solution.origin.x,"y":p.solution.origin.y,"z":p.solution.origin.z,"vx":p.solution.velocity.x,"vy":p.solution.velocity.y,"vz":p.solution.velocity.z,"gravity":p.solution.gravity,"launched":p.launched,"last":p.last,"expires":p.expires,"energy":p.energy,"sequence":p.sequence,"maxContacts":p.max_contacts,"near":p.near,"launchOrigin":{"x":p.launch_origin.x,"y":p.launch_origin.y,"z":p.launch_origin.z},"startCell":p.start_cell,"explosive":p.explosive,"blastRadius":p.blast_radius,"blastSource":p.blast_source.as_ref().map(blast),"effect":p.effect,"sent":p.sent})).collect::<Vec<_>>();
    json!({"tick":g.tick,"units":units,"wallets":m.start.economy.players.iter().map(|p|json!({"mp":p.mp,"mun":p.mun,"fuel":p.fuel})).collect::<Vec<_>>(),"smokes":g.smokes.iter().map(|s|json!({"id":s.id,"x":s.x,"z":s.z,"r":s.r,"t":s.remaining})).collect::<Vec<_>>(),"shots":shots,"flights":flights,"salvos":m.firing.salvos.iter().map(|s|json!({"x":s.at.x,"z":s.at.z,"owner":s.owner,"left":s.left,"next":s.next,"spread":s.spread,"every":s.every})).collect::<Vec<_>>(),"nades":m.firing.grenades.iter().map(|s|json!({"x":s.at.x,"z":s.at.z,"t":s.remaining,"owner":s.owner,"flight":s.flight})).collect::<Vec<_>>(),"nextId":m.next_unit_id,"nextFlight":m.firing.next_flight,"combatSeed":701,"combatRandomState":state,"randomCalls":draws})
}
fn normalize(mut v: Value) -> Value {
    for u in v["units"].as_array_mut().unwrap() {
        if let Some(r) = u["logistics"].as_object_mut() {
            r.retain(|k, _| ["ammo", "forced", "stranded"].contains(&k.as_str()));
        }
        let c = u["control"].as_object_mut().unwrap();
        c.remove("react");
        for key in ["face", "entrench", "dig", "build", "drift", "traffic"] {
            c.insert(key.into(), json!(truth(c.get(key).unwrap_or(&Value::Null))));
        }
    }
    for p in v["flights"].as_array_mut().unwrap() {
        let sent = p["sent"].as_bool().unwrap_or(false);
        p["sent"] = json!(sent);
        let b = &p["blastSource"];
        let blast = if b.is_null() {
            Value::Null
        } else {
            json!({"inf":b["inf"],"veh":b["veh"],"terrain":b["terrain"],"supp":b["supp"],"scar":b["scar"].as_f64().unwrap_or(0.),"antiGarrison":b["antiGarrison"]})
        };
        p["blastSource"] = blast;
        p.as_object_mut()
            .unwrap()
            .retain(|k, _| !["duration", "stamp", "suppressed", "delay"].contains(&k.as_str()));
    }
    for salvo in v["salvos"].as_array_mut().unwrap() {
        *salvo = json!({"x":salvo["x"],"z":salvo["z"],"owner":salvo["owner"],"left":salvo["left"],"next":salvo["next"],"spread":salvo["spread"],"every":salvo["w"]["every"]});
    }
    for ticket in v["nades"].as_array_mut().unwrap() {
        ticket.as_object_mut().unwrap().remove("ab");
    }
    normalize_zero(&mut v);
    v
}
fn normalize_zero(v: &mut Value) {
    match v {
        Value::Number(n) if n.as_f64() == Some(0.) => *v = json!(0),
        Value::Array(rows) => {
            for v in rows {
                normalize_zero(v)
            }
        }
        Value::Object(rows) => {
            for v in rows.values_mut() {
                normalize_zero(v)
            }
        }
        _ => {}
    }
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
#[test]
fn conquest_abilities_match_original_source_phases() {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/match-ability.json")).unwrap();
    assert_eq!(
        fixture["reference"],
        "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
    );
    assert_eq!(fixture["node"], "v24.21.0");
    let (mut cases, mut actions, mut unsupported) = (0, 0, 0);
    for case in fixture["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let (mut m, order) = make(case);
        if case["mode"] == "classic" {
            m.options.mode = Mode::Classic;
            let before = snapshot(&m, &order, &[]);
            let first = &case["actions"][0];
            let error = m
                .execute_command(
                    first["slot"].as_u64().unwrap() as usize,
                    &command(&first["cmd"], false),
                    false,
                )
                .unwrap_err();
            assert!(error.contains("Conquest"), "{name} mode rejection: {error}");
            assert_eq!(snapshot(&m, &order, &[]), before);
            unsupported += 1;
            continue;
        }
        cases += 1;
        let mut initial = snapshot(&m, &order, &[]);
        normalize_zero(&mut initial);
        let expected = normalize(case["snapshots"][0].clone());
        assert!(
            equal(&initial, &expected),
            "{name} initial {}",
            diff(&initial, &expected, "")
        );
        for (index, a) in case["actions"].as_array().unwrap().iter().enumerate() {
            actions += 1;
            m.firing.take_events();
            let mut result = Value::Null;
            if let Some(tick) = a["tick"].as_i64() {
                m.world.game.tick = tick;
            }
            match a["op"].as_str().unwrap() {
                "command" => {
                    result = m
                        .execute_command(
                            a["slot"].as_u64().unwrap() as usize,
                            &command(&a["cmd"], a["auto"].as_bool().unwrap_or(false)),
                            false,
                        )
                        .unwrap()
                        .map_or(Value::Null, |d| json!(d.name()));
                }
                "target" => {
                    result = match m.autocast_target(a["id"].as_u64().unwrap() as u32).unwrap() {
                        None => Value::Null,
                        Some(AutocastTarget::Instant) => json!({}),
                        Some(AutocastTarget::At(p)) => at(p),
                    }
                }
                "prime" => apply_inputs(&mut m, a["id"].as_u64().unwrap() as u32, &a["changes"]),
                "player" => {
                    let slot = a["slot"].as_u64().unwrap() as usize;
                    for (k, v) in a["changes"].as_object().unwrap() {
                        match k.as_str() {
                            "mun" => m.start.economy.players[slot].mun = v.as_f64().unwrap(),
                            _ => panic!("unrepresented ability player input {k}"),
                        }
                    }
                }
                "visibility" => {
                    m.world.game.players[a["owner"].as_u64().unwrap() as usize].visible = a["ids"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|v| v.as_u64().unwrap() as u32)
                        .collect()
                }
                "phase" | "aimed" => {
                    m.begin_path_tick(4096, 1).unwrap();
                    for id in a["ids"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|v| v.as_u64().unwrap() as u32)
                    {
                        let u = &m.world.game.units[&id];
                        if u.hp <= 0. || u.riding || m.commands.units[&id].operative {
                            continue;
                        }
                        if a["op"] == "phase" {
                            let dt = a["dt"].as_f64().unwrap();
                            m.advance_firing_timers(id, dt).unwrap();
                            // Game owns suppression, sprint and route readiness; this bounded pass skips movement.
                            let u = &m.world.game.units[&id];
                            let infantry = m.rules.kinds[u.kind].infantry;
                            let rate = if m.world.game.flags_at(u.x, u.z) & TRENCH != 0 {
                                Catalog::compiled()
                                    .unwrap()
                                    .table("CFG")
                                    .unwrap()
                                    .get("trench")
                                    .unwrap()
                                    .get("rally")
                                    .unwrap()
                                    .number()
                                    .unwrap()
                            } else {
                                8.
                            };
                            let u = m.world.game.units.get_mut(&id).unwrap();
                            if infantry {
                                u.supp = math::max(0., u.supp - rate * dt);
                            }
                            u.repath -= dt;
                            u.sprint -= dt;
                            m.advance_ability_timers(id, dt).unwrap();
                            m.apply_autocast(id).unwrap();
                        }
                        m.step_aimed_ability(id).unwrap();
                    }
                }
                op => panic!("unrepresented ability action {op}"),
            }
            m.grid.rebuild(&m.world.game.units);
            let shots = m
                .firing
                .take_events()
                .into_iter()
                .map(|e| e.to_json(&m.rules))
                .collect::<Vec<_>>();
            let mut actual = snapshot(&m, &order, &shots);
            normalize_zero(&mut actual);
            assert!(
                equal(&result, &case["results"][index]),
                "{name} action{index} result {}",
                diff(&result, &case["results"][index], "")
            );
            let expected = normalize(case["snapshots"][index + 1].clone());
            assert!(
                equal(&actual, &expected),
                "{name} action{index} {}",
                diff(&actual, &expected, "")
            );
        }
    }
    assert_eq!(cases, 28);
    assert_eq!(unsupported, 2);
    assert_eq!(actions, 132);
}
