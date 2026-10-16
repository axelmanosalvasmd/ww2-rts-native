// Compare bounded canonical ability phases, not raw JavaScript save objects.
// Object-valued control fields compare their represented presence or boolean state.
// React has no canonical owner here. Raw tables, suppression sets and duplicate launch
// duration/stamp fields are omitted; blast values, events, geometry and timing are checked.
use crate::{
    catalog::Catalog,
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::{Player, Point, Smoke, TRENCH, Traffic},
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
use serde_json::{Value, json};
use std::collections::VecDeque;
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
    if v["t"] == "move" {
        assert!(!automatic);
        return Command::Move {
            orders: v["orders"]
                .as_array()
                .unwrap()
                .iter()
                .map(|o| MoveOrder {
                    unit: o[0].as_i64().unwrap(),
                    at: Point {
                        x: o[1].as_f64().unwrap(),
                        z: o[2].as_f64().unwrap(),
                    },
                })
                .collect(),
            control_ids: None,
            attack_move: false,
            queue: false,
            together: false,
            face: None,
        };
    }
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
            "still" => u.still = Some(value.as_f64().unwrap()),
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
            "build" => {
                let c = m
                    .commands
                    .units
                    .get_mut(&id)
                    .expect("restored ability control");
                c.build_target = value
                    .as_u64()
                    .filter(|id| *id != 0)
                    .map(|id| u32::try_from(id).unwrap());
                u.build = c.build_target.is_some();
            }
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
    m.commands = MatchCommands::new(4096);
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
        p.visible.sort_unstable();
        p.visible.dedup();
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
fn blast(source: &crate::match_combat::ExplosionSource) -> Value {
    json!({"inf":source.infantry,"veh":source.vehicle,"terrain":source.terrain,"supp":source.suppression,"scar":source.scar,"antiGarrison":source.anti_garrison})
}
fn snapshot(m: &Match, order: &[u32], shots: &[Value]) -> Value {
    let g = &m.world.game;
    let (state, draws) = combat_random(m);
    let units=order.iter().map(|id|{
        let u=&g.units[id];let c=&m.commands.units[id];let a=m.abilities.get(*id).unwrap();let f=m.firing.get(*id).unwrap();
        let outcome=c.outcome.map(|o|match o{MoveOutcome::Interrupted=>"interrupted",MoveOutcome::Moving=>"moving",MoveOutcome::Unreachable=>"unreachable",MoveOutcome::Arrived=>"completed"});
        let mut nade=c.nade.map(at).unwrap_or(Value::Null);if !nade.is_null(){if a.aimed_auto{nade["auto"]=json!(true);}
        if a.aimed_walked{nade["walked"]=json!(true);}}
        let control = json!({"follow":c.follow,"auto":c.auto_cast,"autoRetreat":c.auto_retreat,"holdFire":c.hold_fire,"holdPos":u.hold_pos,"attackId":c.attack_id,"targetId":f.target_id,"orders":orders(&c.orders),"face":c.face.is_some(),"entrench":u.entrench,"dig":u.dig,"build":u.build,"enter":c.enter.map_or(-1,|v|v as i64),"board":u.board,"amove":u.amove.map(at),"fireAt":c.fire_at.map_or(-1,|v|v as i64),"stuck":u.stuck,"drift":u.drift,"reverse":u.reverse.map(at),"repath":u.repath,"cooldown":f.cooldown,"retarget":f.retarget,"traffic":u.traffic.is_some(),"trafficWait":u.traffic_wait,"moveOutcome":outcome,"moveOutcomeTick":c.outcome_tick,"moveResult":if c.outcome==Some(MoveOutcome::Unreachable){json!(["unreachable",c.outcome_tick])}else{Value::Null},"worldGoal":u.world_goal.map(at),"garrison":u.garrison.map_or(-1,|v|v as i64),"shotAt":u.shot_at});
        json!({"id":id,"hp":u.hp,"x":u.x,"z":u.z,"cd":a.cooldown,"buff":a.buff,"ap":a.ap,"sprint":u.sprint,"supp":u.supp,"nade":nade,"path":u.path.iter().copied().map(at).collect::<Vec<_>>(),"retreating":u.retreating,"logistics":m.logistics.get(*id).map(|r|json!({"ammo":r.ammo,"forced":r.forced,"stranded":r.stranded})),"control":control})
    }).collect::<Vec<_>>();
    let flights=m.firing.flights.values().map(|p|json!({"id":p.id,"owner":p.owner,"shooter":p.shooter,"kind":m.rules.kinds[p.source_kind].name,"x":p.solution.origin.x,"y":p.solution.origin.y,"z":p.solution.origin.z,"vx":p.solution.velocity.x,"vy":p.solution.velocity.y,"vz":p.solution.velocity.z,"gravity":p.solution.gravity,"launched":p.launched,"last":p.last,"expires":p.expires,"energy":p.energy,"sequence":p.sequence,"maxContacts":p.max_contacts,"near":p.near,"launchOrigin":{"x":p.launch_origin.x,"y":p.launch_origin.y,"z":p.launch_origin.z},"startCell":p.start_cell,"explosive":p.explosive,"blastRadius":p.blast_radius,"blastSource":p.blast_source.as_ref().map(blast),"effect":p.effect,"sent":p.sent,"intended":p.intended,"damageScale":p.damage_scale,"ap":p.armor_piercing,"apMultiplier":p.ap_multiplier,"apNaval":p.ap_naval,"suppression":p.suppression,"suppressionScale":p.suppression_scale,"terrain":p.terrain,"scar":p.scar})).collect::<Vec<_>>();
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
        for (key, default) in [
            ("intended", Value::Null),
            ("damageScale", json!(1.)),
            ("ap", json!(false)),
            ("apMultiplier", json!(1.5)),
            ("apNaval", json!(false)),
            ("suppression", json!(0.)),
            ("suppressionScale", json!(1.)),
            ("terrain", json!(0.)),
            ("scar", json!(0.)),
            ("blastRadius", Value::Null),
            ("effect", Value::Null),
        ] {
            if p.get(key).is_none() {
                p[key] = default;
            }
        }
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
    let fixture: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/match-ability.json")).unwrap();
    assert_eq!(
        fixture["reference"],
        "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
    );
    assert_eq!(fixture["node"], "v24.21.0");
    let (mut cases, mut actions, mut unsupported, mut unsupported_work) = (0, 0, 0, 0);
    let mut appended_cases = std::collections::BTreeSet::new();
    let mut appended_actions = 0;
    for case in fixture["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let appended = name.starts_with("cmb-");
        let mut activated = false;
        let mut phase_count = 0;
        let mut prior_tick = 10;
        if appended {
            assert!(appended_cases.insert(name));
            assert_eq!(case["units"][0]["auto"], true);
            assert_eq!(case["units"][0].get("cd"), None);
        }
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
        if name == "manual-aimed-unrepresented-project-boundary" {
            let invalid = Command::Ability {
                ids: vec![1],
                at: Some(Point {
                    x: f64::NAN,
                    z: 15.,
                }),
                queue: false,
                automatic: false,
            };
            let work = (
                m.world.game.units[&1].dig,
                m.world.game.units[&1].build,
                m.world.game.units[&1].entrench,
            );
            assert_eq!(
                m.execute_command(0, &invalid, false).unwrap(),
                Some(crate::match_commands::Denial::Blocked)
            );
            assert_eq!(
                (
                    m.world.game.units[&1].dig,
                    m.world.game.units[&1].build,
                    m.world.game.units[&1].entrench
                ),
                work
            );
            let before = snapshot(&m, &order, &[]);
            let action = &case["actions"][0];
            assert_eq!(
                m.execute_command(0, &command(&action["cmd"], false), false)
                    .unwrap(),
                Some(crate::match_commands::Denial::Unsupported)
            );
            assert_eq!(snapshot(&m, &order, &[]), before);
            assert_eq!(
                m.execute_command(
                    0,
                    &Command::Ability {
                        ids: vec![1],
                        at: Some(Point {
                            x: f64::NAN,
                            z: 15.
                        }),
                        queue: false,
                        automatic: false,
                    },
                    false,
                )
                .unwrap(),
                Some(crate::match_commands::Denial::Blocked),
                "invalid typed aim retains coordinate refusal precedence",
            );
            // Source mine releases follow before refusing a non-finite aimed command.
            let mut invalid_expected = before.clone();
            invalid_expected["units"][0]["control"]["follow"] = json!(0);
            assert_eq!(snapshot(&m, &order, &[]), invalid_expected);
            unsupported_work += 1;
            continue;
        }
        if !appended {
            cases += 1;
        }
        let mut initial = snapshot(&m, &order, &[]);
        normalize_zero(&mut initial);
        let expected = normalize(case["snapshots"][0].clone());
        assert!(
            equal(&initial, &expected),
            "{name} initial {}",
            diff(&initial, &expected, "")
        );
        for (index, a) in case["actions"].as_array().unwrap().iter().enumerate() {
            if appended {
                appended_actions += 1;
            } else {
                actions += 1;
            }
            m.firing.take_events();
            let mut result = Value::Null;
            if let Some(tick) = a["tick"].as_i64() {
                m.world.game.tick = tick;
            }
            match a["op"].as_str().unwrap() {
                "command" => {
                    if a["cmd"]["t"] == "move" {
                        m.begin_path_tick(4096, 1).unwrap();
                    }
                    result = m
                        .execute_command(
                            a["slot"].as_u64().unwrap() as usize,
                            &command(&a["cmd"], a["auto"].as_bool().unwrap_or(false)),
                            false,
                        )
                        .unwrap()
                        .map_or(Value::Null, |d| json!(d.name()));
                }
                "weapon" => {
                    m.begin_firing_tick(&order).unwrap();
                    m.apply_unit_firing(a["id"].as_u64().unwrap() as u32, false, 0.05)
                        .unwrap();
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
            if a["op"] != "prime" {
                crate::unit_placement::tests::assert_current(&m.world.game, &m.grid);
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
            if appended && a["op"] == "phase" {
                phase_count += 1;
                assert_eq!(
                    m.world.game.tick,
                    prior_tick + 1,
                    "{name} chronological phase"
                );
                prior_tick = m.world.game.tick;
                let u = &m.world.game.units[&1];
                let ability = m.abilities.get(1).unwrap();
                assert!(
                    m.commands.units[&1].auto_cast,
                    "{name} enabled automatic actor"
                );
                let negative = matches!(
                    name,
                    "cmb-105-away-phase-window"
                        | "cmb-118-infantry-phase-window"
                        | "cmb-119-healthy-phase-window"
                        | "cmb-119-no-hit-history-phase-window"
                        | "cmb-120-idle-phase-window"
                );
                if negative {
                    assert!(
                        ability.cooldown <= 0.,
                        "{name} stays ready without activating"
                    );
                    assert!(ability.buff <= 0.);
                    assert!(!ability.ap);
                    assert!(u.sprint <= 0.);
                    assert!(m.world.game.smokes.is_empty());
                }
                if !activated && !negative && ability.cooldown > 0. {
                    assert_eq!(m.world.game.tick, 19, "{name} first scheduled opportunity");
                    let cooldown = match name {
                        "cmb-105-approaching-phase-window" => 40.,
                        "cmb-118-vehicle-phase-window" | "cmb-119-recent-hit-phase-window" => 45.,
                        "cmb-120-moving-phase-window" => 35.,
                        _ => panic!("unexpected positive CMB case {name}"),
                    };
                    assert_eq!(ability.cooldown, cooldown);
                    if name == "cmb-105-approaching-phase-window" {
                        assert_eq!(ability.buff, 10.);
                    }
                    if name == "cmb-120-moving-phase-window" {
                        assert_eq!(u.sprint, 6.);
                    }
                }
                match name {
                    "cmb-105-approaching-phase-window" | "cmb-105-away-phase-window" => {
                        let enemy = &m.world.game.units[&2];
                        let goal = enemy.path.last().expect("commanded enemy movement path");
                        if name == "cmb-105-away-phase-window" {
                            assert!(goal.x > enemy.x && enemy.x > u.x);
                            assert_eq!(enemy.rot, 0.);
                        } else {
                            assert!(goal.x < enemy.x);
                            activated |= ability.cooldown > 0. && ability.buff > 0.;
                        }
                    }
                    "cmb-118-infantry-phase-window" => {
                        assert_eq!(m.firing.get(1).unwrap().target_id, 2);
                        assert!(m.rules.kinds[m.world.game.units[&2].kind].infantry);
                    }
                    "cmb-118-vehicle-phase-window" => {
                        activated |= ability.cooldown > 0. && ability.ap;
                    }
                    "cmb-119-recent-hit-phase-window" => {
                        let hit = m.combat.get(1).unwrap().at_hit.unwrap();
                        assert!(hit <= m.world.game.tick as f64);
                        if !m.world.game.smokes.is_empty() {
                            assert!(m.world.game.tick as f64 - hit <= 60.);
                            assert_eq!(m.world.game.smokes.len(), 1);
                            assert_eq!(m.world.game.smokes[0].r, 9.);
                            assert_eq!(m.world.game.smokes[0].remaining, Some(14.));
                            activated |= ability.cooldown > 0.;
                        }
                    }
                    "cmb-119-no-hit-history-phase-window" => {
                        assert_eq!(m.combat.get(1).unwrap().at_hit, None);
                    }
                    "cmb-119-healthy-phase-window" => assert_eq!(u.hp, 300.),
                    "cmb-120-moving-phase-window" => {
                        activated |= ability.cooldown > 0. && u.sprint > 0. && u.supp == 0.;
                    }
                    "cmb-120-idle-phase-window" => {
                        assert!(u.path.is_empty());
                        assert!(u.supp >= 50.);
                    }
                    _ => panic!("unexpected appended CMB case {name}"),
                }
            }
            let expected = normalize(case["snapshots"][index + 1].clone());
            assert!(
                equal(&actual, &expected),
                "{name} action{index} {}",
                diff(&actual, &expected, "")
            );
        }
        if appended {
            let expected_phases = match name {
                "cmb-105-approaching-phase-window"
                | "cmb-118-vehicle-phase-window"
                | "cmb-118-infantry-phase-window" => 40,
                "cmb-105-away-phase-window" => 60,
                "cmb-119-recent-hit-phase-window"
                | "cmb-119-healthy-phase-window"
                | "cmb-119-no-hit-history-phase-window"
                | "cmb-120-moving-phase-window" => 20,
                "cmb-120-idle-phase-window" => 30,
                _ => panic!("unexpected appended CMB case {name}"),
            };
            assert_eq!(
                phase_count, expected_phases,
                "{name} full acceptance window"
            );
            if matches!(
                name,
                "cmb-105-approaching-phase-window"
                    | "cmb-118-vehicle-phase-window"
                    | "cmb-119-recent-hit-phase-window"
                    | "cmb-120-moving-phase-window"
            ) {
                assert!(
                    activated,
                    "{name} activates inside its named acceptance window"
                );
            }
        }
    }
    assert_eq!(appended_cases.len(), 9);
    assert_eq!(appended_actions, 323);
    assert_eq!(cases, 33);
    assert_eq!(unsupported, 2);
    assert_eq!(unsupported_work, 1);
    assert_eq!(actions, 147);
}

#[test]
fn smoke_identity_collision_rejects_before_selected_unit_writes() {
    let fixture: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/match-ability.json")).unwrap();
    for live_unit_collision in [false, true] {
        let (mut m, order) = make(&fixture["cases"][0]);
        if live_unit_collision {
            m.next_unit_id = 3;
        } else {
            m.world.game.smokes.push(Smoke {
                id: m.next_unit_id,
                x: 15.,
                z: 15.,
                r: 9.,
                remaining: Some(14.),
            });
        }
        let before = snapshot(&m, &order, &[]);
        let command = Command::Ability {
            ids: vec![1, 3],
            at: None,
            queue: false,
            automatic: false,
        };
        let error = m.execute_command(0, &command, false).unwrap_err();
        assert!(error.contains("smoke identity"), "{error}");
        assert_eq!(snapshot(&m, &order, &[]), before);
    }
}

#[test]
fn manual_aimed_ability_cancels_real_paid_work_without_refund() {
    use crate::field_works::{FortKind, StartRequest};
    let fixture: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/match-ability.json")).unwrap();
    let case = fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["name"] == "manual-aimed-clears-and-clamps")
        .unwrap();
    let (mut m, _) = make(case);
    let initial_mp = m.start.economy.players[0].mp;
    let prepared = m
        .field_works
        .prepare_start(
            &m.world,
            &mut m.start.economy.players[0],
            StartRequest {
                control_eligible: true,
                owner: 0,
                unit: 1,
                kind: FortKind::Trench,
                at: Point { x: 25., z: 15. },
                direction: Some(0.),
            },
            |_| true,
        )
        .unwrap();
    let cost = prepared.cost();
    prepared.commit(&mut m.field_works);
    m.world.game.units.get_mut(&1).unwrap().dig = true;
    let terrain = m.world.game.chars.clone();
    assert_eq!(
        m.execute_command(
            0,
            &Command::Ability {
                ids: vec![1],
                at: Some(Point { x: 25., z: 15. }),
                queue: false,
                automatic: false
            },
            false
        )
        .unwrap(),
        None
    );
    assert!(!m.field_works.is_active(1));
    assert!(!m.world.game.units[&1].dig);
    assert_eq!(m.start.economy.players[0].mp, initial_mp - cost);
    assert_eq!(m.world.game.chars, terrain);
}

#[test]
fn rejected_ap_launch_preserves_loaded_round_ammunition_and_random_stream() {
    let fixture: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/match-ability.json")).unwrap();
    let case = fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["name"] == "loaded-ap-consumed-after-qualifying-fire")
        .unwrap();
    let (mut m, order) = make(case);
    let command = command(&case["actions"][0]["cmd"], false);
    assert_eq!(m.execute_command(0, &command, false).unwrap(), None);
    assert!(m.abilities.get(1).unwrap().ap);
    m.begin_firing_tick(&order).unwrap();
    m.firing.next_flight = 9_007_199_254_740_991;
    let ability = *m.abilities.get(1).unwrap();
    let ammo = m.logistics.get(1).cloned();
    let random = m.start.random.snapshot();
    let error = m.apply_unit_firing(1, false, 0.05).unwrap_err();
    assert!(error.contains("flight ID exhausted"), "{error}");
    assert_eq!(m.abilities.get(1), Some(&ability));
    assert_eq!(m.logistics.get(1), ammo.as_ref());
    assert_eq!(m.start.random.snapshot(), random);
    assert_eq!(m.firing.next_flight, 9_007_199_254_740_991);
    assert!(m.firing.flights.is_empty());
    assert!(m.firing.take_events().is_empty());
}
