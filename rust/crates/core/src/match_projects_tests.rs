// Match replay of the original shared-project phases. The fixture's observer-only
// orphan references are filtered; operational listed and unit/job references remain exact.
use crate::{
    catalog::Catalog,
    economy::{Faction, Mode},
    economy_state::ArmySize,
    field_works::{self, FieldWorksRuntime, Project, ProjectOrder, Segment},
    game::{Player, Point},
    grid::SpatialGrid,
    logistics::LogisticsState,
    map::AuthoritativeMap,
    match_abilities::AbilityState,
    match_combat::CombatState,
    match_commands::{Command, MatchCommands, MoveOrder, MoveOutcome, UnitControl},
    match_firing::FiringState,
    match_recovery::RecoveryState,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    match_story::PlayerStory,
    unit_spawn::initial_unit,
    weather::WeatherChoice,
    world::World,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
fn number(v: &Value) -> f64 {
    f64::from_bits(u64::from_str_radix(v.as_str().unwrap(), 16).unwrap())
}
fn bits(v: f64) -> Value {
    json!(format!("{:016x}", v.to_bits()))
}
fn point(v: &Value) -> Point {
    Point {
        x: number(&v["x"]),
        z: number(&v["z"]),
    }
}
fn at(p: Point) -> Value {
    json!({"x":bits(p.x),"z":bits(p.z)})
}
fn decode_command(v: &Value) -> Value {
    let mut v = v.clone();
    for key in ["x", "z", "x2", "z2", "dir"] {
        if let Some(value) = v.get_mut(key) {
            *value = json!(number(value));
        }
    }
    v
}
fn input_order(v: &Value, id: u32) -> Command {
    if v["t"] == "move" {
        Command::Move {
            orders: vec![MoveOrder {
                unit: i64::from(id),
                at: point(v),
            }],
            control_ids: Some(vec![i64::from(id)]),
            attack_move: false,
            queue: false,
            together: false,
            face: None,
        }
    } else {
        Command::from_json(&decode_command(v)).unwrap()
    }
}
fn order(c: &Command) -> Value {
    match c {
        Command::Move {
            orders,
            control_ids,
            ..
        } => {
            assert_eq!(orders.len(), 1);
            json!({"t":"move","ids":control_ids.as_ref().unwrap(),"x":bits(orders[0].at.x),"z":bits(orders[0].at.z)})
        }
        Command::Entrench {
            ids,
            order:
                ProjectOrder::Join {
                    project,
                    at: Some(p),
                },
            ..
        } => json!({"t":"entrench","ids":ids,"join":project,"x":bits(p.x),"z":bits(p.z)}),
        _ => panic!("unrepresented waiting order"),
    }
}
fn segment(s: &Segment) -> Value {
    json!({"kind":s.kind.as_str(),"x":bits(s.at.x),"z":bits(s.at.z),"dir":bits(s.direction),"front":bits(s.front)})
}
fn project(p: &Project) -> Value {
    json!({"id":p.id,"owner":p.owner,"fort":p.fort.as_str(),"jobs":p.jobs.iter().map(segment).collect::<Vec<_>>(),"crew":p.crew,"active":p.active})
}
fn dig(m: &Match, id: u32) -> Value {
    let Some(d) = m.field_works.jobs().get(&id) else {
        return Value::Null;
    };
    let mut v = json!({"x":bits(d.at.x),"z":bits(d.at.z),"cells":d.cells.iter().map(|c|json!([c.cell,char::from(c.ch).to_string()])).collect::<Vec<_>>(),"t":bits(d.elapsed),"kind":d.kind.as_str(),"dir":bits(d.direction),"project":d.project});
    if let Some(front) = d.front {
        v["front"] = bits(front);
    }
    if let Some(reach) = d.reach {
        v["reach"] = bits(reach);
    }
    if let Some(n) = d.original_cells {
        v["n"] = json!(n);
    }
    v
}
fn snapshot(m: &Match) -> Value {
    let g = &m.world.game;
    let referenced = m
        .field_works
        .projects()
        .map(|p| p.id)
        .chain(
            g.units
                .keys()
                .filter_map(|id| m.field_works.membership(*id)),
        )
        .chain(m.field_works.jobs().values().filter_map(|d| d.project))
        .collect::<BTreeSet<_>>();
    let units=g.units.values().map(|u|{
        let c=&m.commands.units[&u.id];let f=m.firing.get(u.id).unwrap();
        json!({"id":u.id,"hp":bits(u.hp),"x":bits(u.x),"z":bits(u.z),"entrench":m.field_works.membership(u.id),"dig":dig(m,u.id),"orders":c.orders.iter().map(order).collect::<Vec<_>>(),"path":u.path.iter().copied().map(at).collect::<Vec<_>>(),"follow":c.follow,"garrison":u.garrison.map_or(-1,|v|v as i64),"retreating":u.retreating,"control":{"attackId":c.attack_id,"targetId":f.target_id,"nade":c.nade.map(at),"enter":c.enter.map_or(-1,|v|v as i64),"board":u.board,"amove":u.amove.map(at),"fireAt":c.fire_at.map_or(-1,|v|v as i64),"build":if u.build{json!(true)}else{json!(0)},"repath":bits(u.repath),"worldGoal":u.world_goal.map(at),"face":c.face.map(|f|json!({"angle":bits(f.angle),"at":at(f.at)})),"traffic":u.traffic.as_ref().map(|t|json!({"goal":at(t.goal),"legs":t.legs.iter().copied().map(at).collect::<Vec<_>>(),"passing":t.passing,"by":t.by,"until":bits(t.until)})),"trafficWait":bits(u.traffic_wait),"moveOutcome":c.outcome.map(|v|match v{MoveOutcome::Interrupted=>"interrupted",MoveOutcome::Moving=>"moving",MoveOutcome::Unreachable=>"unreachable",MoveOutcome::Arrived=>"completed"}),"moveOutcomeTick":c.outcome_tick}})
    }).collect::<Vec<_>>();
    let fronts = g.trench_front.as_ref().map_or_else(Vec::new, |values| {
        values
            .iter()
            .enumerate()
            .filter_map(|(c, v)| (!v.is_nan()).then(|| json!([c, v.to_bits()])))
            .collect()
    });
    json!({"tick":g.tick,"projectSeq":m.field_works.project_sequence(),"projects":m.field_works.projects().map(project).collect::<Vec<_>>(),"retainedProjects":m.field_works.retained_projects().filter(|p|referenced.contains(&p.id)).map(project).collect::<Vec<_>>(),"units":units,"wallets":m.start.economy.players.iter().map(|p|bits(p.mp)).collect::<Vec<_>>(),"story":m.story.iter().map(|s|json!({"mpSpent":bits(s.mp_spent),"built":bits(s.built)})).collect::<Vec<_>>(),"chars":String::from_utf8(g.chars.clone()).unwrap(),"journal":m.world.journal(),"versions":[g.terrain_version,m.world.versions.obstruction,m.world.versions.infantry,m.world.versions.vehicle,m.world.versions.naval],"frontBits":fronts,"mines":m.world.mines.iter().map(|(c,p)|json!([c,p])).collect::<Vec<_>>(),"mineSeen":m.world.mine_seen.iter().map(|(c,p)|json!([c,p])).collect::<Vec<_>>(),"aid":m.world.aid.iter().map(|(c,p)|json!([c,p])).collect::<Vec<_>>()})
}
fn normalize(mut expected: Value) -> Value {
    let mut references = expected["projects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["id"].as_u64().unwrap())
        .collect::<BTreeSet<_>>();
    for u in expected["units"].as_array().unwrap() {
        if let Some(id) = u["entrench"].as_u64() {
            references.insert(id);
        }
        if let Some(id) = u["dig"]["project"].as_u64() {
            references.insert(id);
        }
    }
    expected["retainedProjects"]
        .as_array_mut()
        .unwrap()
        .retain(|p| references.contains(&p["id"].as_u64().unwrap()));
    for key in ["mines", "mineSeen", "aid"] {
        expected[key]
            .as_array_mut()
            .unwrap()
            .sort_by_key(|v| v[0].as_u64().unwrap());
    }
    expected
}
fn assert_snapshot(m: &Match, expected: &Value, label: &str) {
    let actual = snapshot(m);
    let expected = normalize(expected.clone());
    assert_eq!(
        actual,
        expected,
        "{label}: {}",
        diff(&actual, &expected, "")
    );
}
fn diff(a: &Value, b: &Value, path: &str) -> String {
    match (a, b) {
        (Value::Object(a), Value::Object(b)) => {
            for key in a.keys().chain(b.keys()) {
                let a = a.get(key).unwrap_or(&Value::Null);
                let b = b.get(key).unwrap_or(&Value::Null);
                if a != b {
                    return diff(a, b, &format!("{path}.{key}"));
                }
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            if a.len() != b.len() {
                return format!("{path}: lengths {} != {}", a.len(), b.len());
            }
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                if a != b {
                    return diff(a, b, &format!("{path}[{i}]"));
                }
            }
        }
        _ => {}
    }
    format!("{path}: {a} != {b}")
}
fn prime(m: &mut Match, id: u32, fields: &Value) {
    let u = m.world.game.units.get_mut(&id).unwrap();
    for (key, v) in fields.as_object().unwrap() {
        match key.as_str() {
            "id" | "type" | "owner" => {}
            "x" => u.x = number(v),
            "z" => u.z = number(v),
            "hp" => u.hp = number(v),
            "supp" => u.supp = number(v),
            "repath" => u.repath = number(v),
            "riding" => {
                u.riding = v.as_u64().unwrap() != 0;
                m.combat.get_mut(id).unwrap().carrier =
                    v.as_u64().filter(|v| *v != 0).map(|v| v as u32);
            }
            "retreating" => u.retreating = v.as_bool().unwrap(),
            "operative" => {
                m.commands.units.get_mut(&id).unwrap().operative = v.as_u64().unwrap() != 0
            }
            "attackId" => {
                let target = v.as_u64().unwrap() as u32;
                m.commands.units.get_mut(&id).unwrap().attack_id = target;
                u.attack = target != 0;
            }
            "path" => u.path = v.as_array().unwrap().iter().map(point).collect(),
            "orders" => {
                m.commands.units.get_mut(&id).unwrap().orders = v
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| input_order(v, id))
                    .collect()
            }
            key => panic!("unrepresented project actor input {key}"),
        }
    }
    m.grid.update(id, u);
}
fn make(case: &Value) -> Match {
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&case["map"]).unwrap()).unwrap();
    let mut m = Match::new(
        &map,
        MatchOptions {
            seed: 701,
            mode: Mode::Conquest,
            players: (0..3)
                .map(|slot| PlayerConfig {
                    name: format!("p{slot}"),
                    team: if slot == 2 { 0 } else { slot as i32 },
                    faction: Faction::try_from((slot % 2) as u8).unwrap(),
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
        },
    )
    .unwrap();
    m.world = World::from_map(&map).unwrap();
    m.world.game.tick = 10;
    m.world.game.rain = 0.;
    m.world.game.wet = Some(0.);
    m.world.game.players = (0..3)
        .map(|slot| Player {
            slot,
            team: if slot == 2 { 0 } else { slot as i32 },
            visible: Vec::new(),
        })
        .collect();
    m.options.tech = case["tech"].as_bool().unwrap();
    m.start.environment_seed = 123;
    m.start.wind = (0., 0.5);
    m.next_unit_id = 1000;
    m.commands = MatchCommands::new(4096);
    m.abilities = AbilityState::default();
    m.firing = FiringState::default();
    m.combat = CombatState::default();
    m.recovery = RecoveryState::default();
    m.logistics = LogisticsState::default();
    m.field_works = FieldWorksRuntime::default();
    m.story = vec![PlayerStory::default(); 3];
    m.grid = SpatialGrid::new(&m.world.game.units, 16.);
    for (slot, p) in m.start.economy.players.iter_mut().enumerate() {
        p.mp = number(&case["mp"][slot]);
        p.mun = 0.;
        p.fuel = 0.;
        m.start.players[slot].spawn = Point {
            x: 3. + 20. * slot as f64,
            z: 3.,
        };
    }
    for v in case["units"].as_array().unwrap() {
        let id = v["id"].as_u64().unwrap() as u32;
        let kind = m.rules.kind(v["type"].as_str().unwrap()).unwrap();
        let owner = v["owner"].as_u64().unwrap() as usize;
        let u = initial_unit(
            &m.rules,
            Catalog::compiled().unwrap(),
            kind,
            id,
            owner,
            point(v),
        )
        .unwrap();
        let mut c = UnitControl::new(&u, &m.rules).unwrap();
        c.follow = 77;
        c.auto_cast = false;
        c.auto_retreat = false;
        m.world.game.units.insert(id, u);
        m.commands.units.insert(id, c);
        m.abilities.initialize(id);
        m.firing.initialize(id);
        m.combat.initialize(id);
        m.recovery.initialize(id);
        prime(&mut m, id, v);
    }
    let ids = m.world.game.units.keys().copied().collect::<Vec<_>>();
    for p in &mut m.world.game.players {
        p.visible = ids.clone();
    }
    m.building_cells.clear();
    for slot in 0..3 {
        m.world
            .register_recipient(crate::world::Recipient::Player(slot));
    }
    m.initialize_recipient_routing().unwrap();
    m
}
// Resolve callback borrowing through the real owner-bound view. This probes placement
// without mutating it and obtains every sight answer from the production recipient API.
fn visible_cells(m: &mut Match, owner: usize, job: Segment) -> Vec<field_works::WorkCell> {
    let mut sight = BTreeMap::new();
    loop {
        let mut missing = None;
        let result = field_works::placement(
            &m.world,
            job.kind,
            job.at,
            job.direction,
            Some(m.world.game.players[owner].team),
            |p| {
                let key = (p.x.to_bits(), p.z.to_bits());
                sight.get(&key).copied().unwrap_or_else(|| {
                    missing = Some(p);
                    false
                })
            },
        );
        if let Some(p) = missing {
            let seen = m.recipient_sees(owner, p).unwrap();
            sight.insert((p.x.to_bits(), p.z.to_bits()), seen);
        } else {
            return result.unwrap();
        }
    }
}
#[test]
fn actual_match_project_phases_match_original_source() {
    let fixture: Value =
        serde_json::from_str(include_str!("../tests/fixtures/mass-entrenchment.json")).unwrap();
    assert_eq!(
        fixture["reference"],
        "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
    );
    assert_eq!(fixture["node"], "v24.21.0");
    let (mut cases, mut actions, mut component_only, mut unsupported) = (0, 0, 0, 0);
    for case in fixture["traces"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        if name == "actual-join-take-helpers-with-source-preconditions" {
            component_only += 1;
            continue;
        }
        let mut m = make(case);
        if name == "source-wall-tier-admission-boundary" {
            let before = snapshot(&m);
            let a = &case["actions"][0];
            let cmd = Command::from_json(&decode_command(&a["cmd"])).unwrap();
            assert_eq!(
                m.execute_command(0, &cmd, false).unwrap(),
                Some(crate::match_commands::Denial::Unsupported)
            );
            assert_eq!(snapshot(&m), before);
            unsupported += 1;
            continue;
        }
        cases += 1;
        let mut path_budget = 4096usize;
        let mut path_retry_limit = 20u32;
        assert_snapshot(&m, &case["snapshots"][0], &format!("{name} initial"));
        for (index, a) in case["actions"].as_array().unwrap().iter().enumerate() {
            actions += 1;
            let mut result = Value::Null;
            if let Some(tick) = a["tick"].as_i64() {
                m.world.game.tick = tick;
            }
            match a["op"].as_str().unwrap() {
                "command" => {
                    let cmd = Command::from_json(&decode_command(&a["cmd"])).unwrap();
                    result = m
                        .execute_command(a["slot"].as_u64().unwrap() as usize, &cmd, false)
                        .unwrap()
                        .map_or(Value::Null, |r| json!(r.name()));
                }
                "mass" => {
                    for id in a["ids"].as_array().unwrap() {
                        m.apply_project_unit(id.as_u64().unwrap() as u32).unwrap();
                    }
                }
                "work" => {
                    m.begin_path_tick(path_budget, path_retry_limit).unwrap();
                    m.apply_field_work_unit_dt(a["id"].as_u64().unwrap() as u32, number(&a["dt"]))
                        .unwrap();
                }
                "pathBegin" => {
                    path_budget = a["budget"].as_u64().unwrap() as usize;
                    path_retry_limit = a["retryLimit"].as_u64().unwrap() as u32;
                    m.begin_path_tick(path_budget, path_retry_limit).unwrap();
                }
                "queued" => m
                    .start_queued_orders(a["id"].as_u64().unwrap() as u32, false)
                    .unwrap(),
                "recount" => m.recount_field_projects().unwrap(),
                "prime" => prime(&mut m, a["id"].as_u64().unwrap() as u32, &a["changes"]),
                "player" => {
                    for (k, v) in a["changes"].as_object().unwrap() {
                        assert_eq!(k, "mp");
                        m.start.economy.players[a["slot"].as_u64().unwrap() as usize].mp =
                            number(v);
                    }
                }
                "atDig" => {
                    let id = a["id"].as_u64().unwrap() as u32;
                    let p = m.field_works.jobs()[&id].at;
                    let u = m.world.game.units.get_mut(&id).unwrap();
                    u.x = p.x;
                    u.z = p.z;
                    u.path.clear();
                    u.repath = 0.;
                    m.grid.update(id, u);
                }
                "invalidate" => {
                    let p = m
                        .field_works
                        .project(a["project"].as_u64().unwrap())
                        .unwrap();
                    let owner = p.owner;
                    let job = p.jobs[a["index"].as_u64().unwrap_or(0) as usize];
                    let cells = visible_cells(&mut m, owner, job);
                    for cell in cells {
                        m.world
                            .set_cell(cell.cell, a["ch"].as_str().unwrap().as_bytes()[0])
                            .unwrap();
                    }
                }
                "mutate" => {
                    m.world
                        .set_cell(
                            a["c"].as_u64().unwrap() as usize,
                            a["ch"].as_str().unwrap().as_bytes()[0],
                        )
                        .unwrap();
                }
                "remove" => {
                    let id = a["id"].as_u64().unwrap() as u32;
                    m.cancel_field_work(id).unwrap();
                    m.world.game.units.remove(&id);
                    m.grid.remove_unit(id);
                    m.commands.units.remove(&id);
                    m.firing.remove(id);
                    m.abilities.remove(id);
                    m.recovery.remove(id);
                }
                op => panic!("unrepresented Match project action {op}"),
            }
            m.grid.rebuild(&m.world.game.units);
            assert_eq!(
                result, case["results"][index],
                "{name} action{index} result"
            );
            assert_snapshot(
                &m,
                &case["snapshots"][index + 1],
                &format!("{name} action{index}"),
            );
        }
    }
    assert_eq!(cases, 23);
    assert_eq!(actions, 131);
    assert_eq!(component_only, 1);
    assert_eq!(unsupported, 1);
}
#[test]
fn malformed_membership_rejects_before_project_payment_or_control_writes() {
    let fixture: Value =
        serde_json::from_str(include_str!("../tests/fixtures/mass-entrenchment.json")).unwrap();
    let case = &fixture["traces"][0];
    let mut m = make(case);
    let first = &case["actions"][0];
    let cmd = Command::from_json(&decode_command(&first["cmd"])).unwrap();
    assert_eq!(m.execute_command(0, &cmd, false).unwrap(), None);
    assert_eq!(m.field_works.membership(1), Some(1));
    m.world.game.units.get_mut(&1).unwrap().entrench = false;
    let before = snapshot(&m);
    let error = m.apply_project_unit(1).unwrap_err();
    assert!(error.contains("canonical shared field-work"), "{error}");
    assert_eq!(snapshot(&m), before);
    assert!(!m.world.game.units[&1].entrench);
}

#[test]
fn defensive_project_failures_preserve_follow_wallet_and_project_identity() {
    let fixture: Value =
        serde_json::from_str(include_str!("../tests/fixtures/mass-entrenchment.json")).unwrap();
    let case = &fixture["traces"][0];
    for boundary in ["wallet", "sequence", "zero-id", "path"] {
        let mut m = make(case);
        let mut cmd = Command::from_json(&decode_command(&case["actions"][0]["cmd"])).unwrap();
        match boundary {
            "wallet" => m.start.economy.players[0].mp = f64::NAN,
            "sequence" => m.field_works.projects.sequence = 9_007_199_254_740_991,
            "zero-id" => {
                let mut unit = m.world.game.units.remove(&1).unwrap();
                unit.id = 0;
                m.world.game.units.insert(0, unit);
                let control = m.commands.units.remove(&1).unwrap();
                m.commands.units.insert(0, control);
                m.firing.initialize(0);
                m.abilities.initialize(0);
                m.recovery.initialize(0);
                m.combat.initialize(0);
                if let Command::Entrench { ids, .. } = &mut cmd {
                    ids[0] = 0;
                }
            }
            "path" => m.world.game.units.get_mut(&1).unwrap().path_gen = u64::MAX,
            _ => unreachable!(),
        }
        let before = snapshot(&m);
        assert!(m.execute_command(0, &cmd, false).is_err(), "{boundary}");
        assert_eq!(snapshot(&m), before, "{boundary}");
        assert_eq!(m.field_works.projects().count(), 0, "{boundary}");
    }
}
