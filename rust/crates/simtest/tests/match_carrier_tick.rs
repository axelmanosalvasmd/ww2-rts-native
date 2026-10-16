//! Full source carrier steps replayed through the single canonical Match runner.
//!
//! This consumer compares initial and post-step actors, cargo/boarding links,
//! paths and waiting-order counts, paid work, represented controls/timers,
//! wallets/income/story, passive grid candidates and public scheduler counters.
//! Command snapshots are oracle-only because native admission happens inside
//! advance_tick. Source patches rebuild a read-only JSON oracle, never Game.
//!
//! Source moveResult has no canonical owner and is excluded in full, including
//! its retained outcome/clock facts. The separate moveOutcome/moveOutcomeTick
//! fields are compared directly. Auxiliary lane/clearance objects, source path
//! calls and failures, private scheduler queues, detailed capture-point state,
//! raw terrain flags and complete terrain logs are outside this projection.
//! Quiet ground/surface wear is checked
//! against native constructor state. No ground effects, mines, crushing, combat,
//! ejection, naval, rail or garrison tick completion is claimed.
use serde_json::{Value, json};
use ww2_core::{
    catalog::Catalog,
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::Point,
    map::AuthoritativeMap,
    match_commands::{Command, MoveOutcome, UnitControl},
    match_end::MatchSample,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    match_tick::{TickPathConfig, TickPhase},
    orders::OrderKey,
    random::RandomSnapshot,
    unit_spawn,
    weather::WeatherChoice,
};

fn bits(n: f64) -> Value {
    json!(format!("{:016x}", n.to_bits()))
}
fn number(v: &Value) -> f64 {
    let text = v
        .as_str()
        .or_else(|| v["f64"].as_str())
        .expect("source IEEE 754 value");
    f64::from_bits(u64::from_str_radix(text, 16).unwrap())
}
fn point(p: Point) -> Value {
    json!({"x":bits(p.x),"z":bits(p.z)})
}
fn tagged(v: Value) -> Value {
    match v {
        Value::Number(n) => json!({"f64":bits(n.as_f64().unwrap())}),
        Value::Array(a) => Value::Array(a.into_iter().map(tagged).collect()),
        Value::Object(o) => Value::Object(o.into_iter().map(|(k, v)| (k, tagged(v))).collect()),
        v => v,
    }
}
fn tagged_point(p: Option<Point>) -> Value {
    p.map_or(Value::Null, |p| tagged(json!({"x":p.x,"z":p.z})))
}
fn alias(c: &Value, name: &str) -> u32 {
    c["setup"]["aliases"][name]
        .as_u64()
        .expect("declared source actor alias") as u32
}
fn actor(c: &Value, v: &Value) -> u32 {
    if let Some(name) = v.as_str() {
        alias(c, name)
    } else {
        u32::try_from(v.as_u64().expect("source actor ID")).unwrap()
    }
}
fn refresh(m: &mut Match) {
    m.grid.rebuild(&m.world.game.units);
    ww2_core::vision::update_vision(&mut m.world.game, &m.rules, &m.grid);
    for owner in 0..m.world.game.players.len() {
        m.refresh_recipient_routing(owner).unwrap();
    }
}
fn prime_units(m: &mut Match, c: &Value, rows: &Value) {
    for (name, fields) in rows.as_object().expect("declared prime unit fields") {
        let id = alias(c, name);
        for (key, v) in fields.as_object().unwrap() {
            match key.as_str() {
                "cd" => m.abilities.get_mut(id).unwrap().cooldown = v.as_f64().unwrap(),
                "buff" => m.abilities.get_mut(id).unwrap().buff = v.as_f64().unwrap(),
                "cooldown" => m.firing.get_mut(id).unwrap().cooldown = v.as_f64().unwrap(),
                "retarget" => m.firing.get_mut(id).unwrap().retarget = v.as_f64().unwrap(),
                "react" => m.commands.units.get_mut(&id).unwrap().react = v.as_f64().unwrap(),
                "cargo" => {
                    m.combat.get_mut(id).unwrap().cargo =
                        v.as_array().unwrap().iter().map(|v| actor(c, v)).collect()
                }
                "riding" => {
                    let carrier = actor(c, v);
                    m.combat.get_mut(id).unwrap().carrier = (carrier != 0).then_some(carrier);
                    m.world.game.units.get_mut(&id).unwrap().riding = carrier != 0;
                }
                "board" => m.world.game.units.get_mut(&id).unwrap().board = actor(c, v),
                "x" => m.world.game.units.get_mut(&id).unwrap().x = v.as_f64().unwrap(),
                "z" => m.world.game.units.get_mut(&id).unwrap().z = v.as_f64().unwrap(),
                "rot" => m.world.game.units.get_mut(&id).unwrap().rot = v.as_f64().unwrap(),
                "repath" => m.world.game.units.get_mut(&id).unwrap().repath = v.as_f64().unwrap(),
                "sprint" => m.world.game.units.get_mut(&id).unwrap().sprint = v.as_f64().unwrap(),
                "still" => {
                    m.world.game.units.get_mut(&id).unwrap().still = Some(v.as_f64().unwrap())
                }
                "supp" => m.world.game.units.get_mut(&id).unwrap().supp = v.as_f64().unwrap(),
                "moveSpeed" => {
                    m.world.game.units.get_mut(&id).unwrap().move_speed = v.as_f64().unwrap()
                }
                "worldGoal" => {
                    assert!(
                        v.is_null(),
                        "only declared null worldGoal prime is represented"
                    );
                    m.world.game.units.get_mut(&id).unwrap().world_goal = None;
                }
                "path" => {
                    assert!(
                        v.as_array().unwrap().is_empty(),
                        "only declared empty path prime is represented"
                    );
                    let u = m.world.game.units.get_mut(&id).unwrap();
                    u.path = Vec::new();
                    u.path_gen = u.path_gen.checked_add(1).unwrap();
                }
                other => panic!("undeclared/unrepresented carrier prime {name}/{other}"),
            }
        }
    }
}
fn make(c: &Value) -> Match {
    let s = &c["setup"];
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&s["map"]).unwrap()).unwrap();
    let players = s["names"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .map(|(owner, name)| PlayerConfig {
            name: name.as_str().unwrap().into(),
            team: s["teams"][owner].as_i64().unwrap() as i32,
            faction: Faction::try_from(s["factions"][owner].as_u64().unwrap() as u8).unwrap(),
        })
        .collect();
    let mut m = Match::new(
        &map,
        MatchOptions {
            seed: 701,
            mode: Mode::Default,
            players,
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
    assert_eq!(
        json!(m.world.game.units.keys().copied().collect::<Vec<_>>()),
        s["initialActors"],
        "all source constructor actors retained"
    );
    assert_eq!(
        m.next_unit_id,
        s["creation"]["nextId"].as_u64().unwrap() as u32
    );
    for spec in s["spawns"].as_array().unwrap() {
        let id = m.next_unit_id;
        assert_eq!(id, alias(c, spec["alias"].as_str().unwrap()));
        assert!(!m.world.game.units.contains_key(&id) && !m.commands.units.contains_key(&id));
        assert!(
            m.combat.get(id).is_none()
                && m.firing.get(id).is_none()
                && m.abilities.get(id).is_none()
                && m.recovery.get(id).is_none()
        );
        let kind = m.rules.kind(spec["type"].as_str().unwrap()).unwrap();
        let u = unit_spawn::initial_unit(
            &m.rules,
            Catalog::compiled().unwrap(),
            kind,
            id,
            spec["owner"].as_u64().unwrap() as usize,
            Point {
                x: spec["pose"]["x"].as_f64().unwrap(),
                z: spec["pose"]["z"].as_f64().unwrap(),
            },
        )
        .unwrap();
        let control = UnitControl::new(&u, &m.rules).unwrap();
        m.commands.insert(id, control);
        m.combat.initialize(id);
        m.firing.initialize(id);
        m.abilities.initialize(id);
        m.recovery.initialize(id);
        m.world.game.units.insert(id, u);
        m.next_unit_id = id.checked_add(1).unwrap();
    }
    let quiet = &s["prime"]["allUnits"];
    assert_eq!(
        *quiet,
        json!({"holdFire":true,"holdPos":true,"auto":false,"autoRetreat":false})
    );
    for u in m.world.game.units.values_mut() {
        u.hold_pos = true;
    }
    for control in m.commands.units.values_mut() {
        control.hold_fire = true;
        control.auto_cast = false;
        control.auto_retreat = false;
    }
    prime_units(&mut m, c, &s["prime"]["units"]);
    refresh(&mut m);
    m.initialize_tick_scene(
        0,
        TickPathConfig {
            budget: s["prime"]["pathBudget"].as_u64().unwrap() as usize,
            retry_limit: s["prime"]["pathRetryLimit"].as_u64().unwrap() as u32,
        },
    )
    .unwrap();
    m
}
// This is an oracle-only JSON update. It cannot borrow or change Native Match.
fn oracle_patch(state: &mut Value, patch: &Value) {
    for change in patch.as_array().unwrap() {
        let path = change[0].as_array().unwrap();
        if path.is_empty() {
            *state = change[1].clone();
            continue;
        }
        let mut at = &mut *state;
        for key in &path[..path.len() - 1] {
            at = if let Some(i) = key.as_u64() {
                &mut at[i as usize]
            } else {
                &mut at[key.as_str().unwrap()]
            };
        }
        let key = path.last().unwrap();
        if let Some(i) = key.as_u64() {
            at[i as usize] = change[1].clone();
        } else if change.as_array().unwrap().len() == 1 {
            at.as_object_mut().unwrap().remove(key.as_str().unwrap());
        } else {
            at[key.as_str().unwrap()] = change[1].clone();
        }
    }
}
fn compare_paid(m: &Match, id: u32, source: &Value, context: &str) {
    let Some(job) = m.field_works.jobs().get(&id) else {
        assert!(
            source.is_null(),
            "{context}: missing native paid job for {id}"
        );
        return;
    };
    assert!(!source.is_null(), "{context}: extra native paid job {id}");
    for (key, value) in [
        ("x", tagged(json!(job.at.x))),
        ("z", tagged(json!(job.at.z))),
        ("dir", tagged(json!(job.direction))),
        ("t", tagged(json!(job.elapsed))),
        ("kind", json!(job.kind.as_str())),
        ("n", tagged(json!(job.original_cells))),
        ("reach", tagged(json!(job.reach))),
        (
            "cells",
            tagged(json!(
                job.cells
                    .iter()
                    .map(|c| json!([c.cell, char::from(c.ch).to_string()]))
                    .collect::<Vec<_>>()
            )),
        ),
    ] {
        assert_eq!(value, source[key], "{context}: unit {id} paid {key}");
    }
    for key in source.as_object().unwrap().keys() {
        assert!(
            ["x", "z", "dir", "t", "kind", "n", "reach", "cells"].contains(&key.as_str()),
            "{context}: unrepresented paid-job source field {key}"
        );
    }
}
fn compare(
    m: &Match,
    expected: &Value,
    rng: &RandomSnapshot,
    env_start: u32,
    source_start: u32,
    radius: f64,
    context: &str,
) {
    macro_rules! same {
        ($a:expr,$s:expr,$name:expr) => {
            assert_eq!($a, $s, "{context}: {}", $name);
        };
    }
    same!(
        m.world.game.tick,
        expected["tick"].as_i64().unwrap(),
        "tick"
    );
    same!(
        json!(m.world.game.units.keys().copied().collect::<Vec<_>>()),
        expected["actorOrder"],
        "actor order"
    );
    same!(
        m.next_unit_id,
        expected["nextId"].as_u64().unwrap() as u32,
        "next ID"
    );
    same!(
        m.start.random.snapshot(),
        *rng,
        "quiet native random streams"
    );
    same!(
        m.start.environment_seed.wrapping_sub(env_start),
        (expected["seed"].as_u64().unwrap() as u32).wrapping_sub(source_start),
        "environment advancement from own constructor states"
    );
    for source in expected["units"].as_array().unwrap() {
        let id = source["id"].as_u64().unwrap() as u32;
        let u = &m.world.game.units[&id];
        let c = &m.commands.units[&id];
        let combat = m.combat.get(id).unwrap();
        let fire = m.firing.get(id).unwrap();
        let ability = m.abilities.get(id).unwrap();
        same!(json!(u.owner), source["owner"], "owner");
        same!(json!(m.rules.kinds[u.kind].name), source["type"], "kind");
        for (k, n) in [
            ("x", u.x),
            ("z", u.z),
            ("rot", u.rot),
            ("hp", u.hp),
            ("aim", c.aim),
        ] {
            same!(bits(n), source[k], format!("actor {id} {k}"));
        }
        same!(
            json!(combat.cargo),
            source["cargo"],
            format!("actor {id} cargo order")
        );
        same!(
            u.riding,
            combat.carrier.is_some(),
            format!("actor {id} riding query flag")
        );
        if let Some(carrier) = combat.carrier {
            assert!(m.world.game.units.contains_key(&carrier));
            assert!(
                m.combat.get(carrier).unwrap().cargo.contains(&id),
                "{context}: missing reciprocal live carrier link"
            );
        }
        for cargo in &combat.cargo {
            if let Some(row) = m.combat.get(*cargo) {
                assert_eq!(row.carrier, Some(id), "{context}: cargo reciprocal link");
                assert!(m.world.game.units[cargo].riding);
            }
        }
        same!(
            json!(u.path.iter().copied().map(point).collect::<Vec<_>>()),
            source["path"],
            format!("actor {id} path")
        );
        same!(
            c.orders.len(),
            source["orders"].as_array().unwrap().len(),
            format!("actor {id} waiting order count")
        );
        // The fixture currently contains no waiting rows. Fail on an expanded shape.
        assert!(
            source["orders"].as_array().unwrap().is_empty(),
            "{context}: waiting command body projection needed"
        );
        compare_paid(m, id, &source["dig"], context);
        same!(
            u.dig,
            m.field_works.is_active(id),
            "Dig owner/query projection"
        );
        same!(
            tagged(json!(m.field_works.membership(id))),
            source["entrench"],
            "Entrench owner projection"
        );
        same!(
            u.entrench,
            m.field_works.membership(id).is_some(),
            "Entrench query projection"
        );
        let controls = &source["controls"];
        let fields = [
            ("board", tagged(json!(u.board))),
            ("riding", tagged(json!(combat.carrier.unwrap_or(0)))),
            (
                "garrison",
                tagged(json!(u.garrison.map_or(-1, |v| v as i64))),
            ),
            ("attackId", tagged(json!(c.attack_id))),
            ("targetId", tagged(json!(fire.target_id))),
            ("enter", tagged(json!(c.enter.map_or(-1, |v| v as i64)))),
            ("fireAt", tagged(json!(c.fire_at.map_or(-1, |v| v as i64)))),
            ("retreating", json!(u.retreating)),
            ("holdFire", json!(c.hold_fire)),
            ("holdPos", json!(u.hold_pos)),
            ("auto", json!(c.auto_cast)),
            ("autoRetreat", json!(c.auto_retreat)),
            ("worldGoal", tagged_point(u.world_goal)),
            ("amove", tagged_point(u.amove)),
            ("reverse", tagged_point(u.reverse)),
            ("routeEnd", tagged_point(c.route_end)),
            ("drift", json!(u.drift)),
        ];
        for (k, v) in fields {
            same!(v, controls[k], format!("actor {id} control {k}"));
        }
        if controls["follow"].is_null() {
            same!(c.follow, 0, "absent source follow");
        } else {
            same!(tagged(json!(c.follow)), controls["follow"], "follow");
        }
        if controls["pace"].is_null() {
            assert!(u.pace.is_nan(), "{context}: absent source pace sentinel");
        } else {
            same!(tagged(json!(u.pace)), controls["pace"], "pace");
        }
        if controls["trafficWait"].is_null() {
            same!(u.traffic_wait, 0., "absent source trafficWait");
        } else {
            same!(
                tagged(json!(u.traffic_wait)),
                controls["trafficWait"],
                "trafficWait"
            );
        }
        same!(
            c.face.is_some(),
            !controls["face"].is_null(),
            "facing presence"
        );
        assert!(
            controls["face"].is_null(),
            "{context}: facing body outside carrier fixture"
        );
        same!(
            u.traffic.is_some(),
            !controls["traffic"].is_null(),
            "traffic presence"
        );
        assert!(
            controls["traffic"].is_null(),
            "{context}: traffic body outside carrier fixture"
        );
        assert!(controls["build"].is_null() || controls["build"] == tagged(json!(0)));
        same!(u.build, false, "quiet construction");
        let outcome = c.outcome.map(|o| match o {
            MoveOutcome::Interrupted => "interrupted",
            MoveOutcome::Moving => "moving",
            MoveOutcome::Unreachable => "unreachable",
            MoveOutcome::Arrived => "completed",
        });
        same!(json!(outcome), controls["moveOutcome"], "move outcome");
        same!(
            tagged(json!(c.outcome_tick)),
            controls["moveOutcomeTick"],
            "move outcome clock"
        );
        // Auxiliary source motionClearance/lane object shape has no receipt here.
        let actual_before = u
            .motion_before
            .map(|p| tagged(json!({"x":p.x,"z":p.z,"rot":p.rot})));
        same!(
            actual_before.unwrap_or(Value::Null),
            controls["motionBefore"],
            "motionBefore"
        );
        for (k, n) in [
            ("cd", ability.cooldown),
            ("cooldown", fire.cooldown),
            ("retarget", fire.retarget),
            ("repath", u.repath),
            ("buff", ability.buff),
            ("sprint", u.sprint),
            ("react", c.react),
            ("still", u.still.expect("canonical still timer")),
            ("stuck", u.stuck),
            ("supp", u.supp),
            ("reinf", m.recovery.get(id).unwrap().reinforcement),
            ("moveSpeed", u.move_speed),
            ("vx", u.vx.expect("canonical vx")),
            ("vz", u.vz.expect("canonical vz")),
        ] {
            same!(
                tagged(json!(n)),
                source["timers"][k],
                format!("actor {id} timer {k}")
            );
        }
        let travel = u
            .travel_dir
            .expect("source factory travelDir is represented");
        same!(
            tagged(json!(travel)),
            source["timers"]["travelDir"],
            "travelDir"
        );
        // JS candidates defaults to ordered bucket candidates, not exact-radius filtering.
        same!(
            json!(
                m.grid
                    .in_order(&m.world.game.units, (u.x, u.z), radius, |_, _| true)
            ),
            source["grid"],
            format!("actor {id} passive candidates")
        );
    }
    for (slot, source) in expected["players"].as_array().unwrap().iter().enumerate() {
        let p = &m.start.economy.players[slot];
        same!(json!(slot), source["slot"], "player slot");
        same!(json!(p.team), source["team"], "player team");
        for (k, n) in [("mp", p.mp), ("inc", p.inc), ("vp", p.vp)] {
            same!(bits(n), source[k], format!("wallet {slot}/{k}"));
        }
        for (k, n) in [("mun", p.mun), ("fuel", p.fuel)] {
            if source[k].is_null() {
                same!(n, 0., format!("absent source {k}"));
            } else {
                same!(bits(n), source[k], k);
            }
        }
        for (k, v) in [("out", p.out), ("away", p.away)] {
            if source[k].is_null() {
                same!(v, false, format!("absent source {k}"));
            } else {
                same!(json!(v), source[k], k);
            }
        }
        same!(
            tagged(json!(m.start.players[slot].support)),
            source["sup"],
            "support cooldowns"
        );
    }
    for (slot, s) in m.story.iter().enumerate() {
        for (k, n) in [
            ("kills", s.kills),
            ("losses", s.losses),
            ("built", s.built),
            ("captures", s.captures),
            ("mpSpent", s.mp_spent),
            ("supportCalls", s.support_calls),
            ("planesDowned", s.planes_downed),
        ] {
            if expected["story"].is_null() {
                same!(n, 0., "lazy source story default");
            } else {
                same!(tagged(json!(n)), expected["story"][slot][k], k);
            }
        }
    }
    if expected["timeline"].is_null() {
        assert!(m.timeline.is_empty());
    } else {
        same!(
            tagged(json!(
                m.timeline
                    .iter()
                    .map(|s| match s {
                        MatchSample::Conquest { t, vp } => json!({"t":t,"vp":vp}),
                        MatchSample::Territories { .. } =>
                            panic!("carrier fixture cannot represent a Territories timeline"),
                    })
                    .collect::<Vec<_>>()
            )),
            expected["timeline"],
            "story sample"
        );
    }
    let stats = m.commands.path_requests().stats();
    if !expected["pathStats"].is_null() {
        same!(
            stats.deferred as f64,
            number(&expected["pathStats"]["deferred"]),
            "public scheduler deferred"
        );
        same!(
            stats.dropped as f64,
            number(&expected["pathStats"]["dropped"]),
            "public scheduler dropped"
        );
    } else {
        same!(stats.deferred, 0, "absent source deferred counter");
        same!(stats.dropped, 0, "absent source dropped counter");
    }
    same!(expected["shots"], json!([]), "quiet source shots");
    assert!(m.firing.events.is_empty() && m.support.events.is_empty());
    assert!(expected["winner"].is_null() && m.finish.is_none());
}
fn order_key(action: &Value) -> OrderKey {
    OrderKey {
        tick: action["key"]["tick"].as_u64().unwrap(),
        issuer: action["key"]["issuer"].as_u64().unwrap() as u32,
        sequence: action["key"]["sequence"].as_u64().unwrap(),
    }
}
#[test]
fn canonical_carriers_match_original_full_steps_and_arrival_order() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/match-carrier-tick.json")).unwrap();
    assert_eq!(
        fixture["reference"],
        "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
    );
    assert_eq!(fixture["node"], "v24.21.0");
    assert_eq!(fixture["arch"], "x64");
    let mut steps = 0usize;
    let mut commands = 0usize;
    let mut comparisons = 0usize;
    for reverse_arrival in [false, true] {
        for c in fixture["cases"].as_array().unwrap() {
            let name = c["name"].as_str().unwrap();
            let mut m = make(c);
            let rng = m.start.random.snapshot();
            let env = m.start.environment_seed;
            let source_env = c["initial"]["seed"].as_u64().unwrap() as u32;
            let wear = m.world.game.wear.clone();
            let surface_wear = m.world.surface_wear.clone();
            let mut expected = c["initial"].clone();
            let radius = c["setup"]["gridRadius"].as_f64().unwrap();
            let mut pending: Vec<(OrderKey, Command, Value)> = Vec::new();
            compare(&m, &expected, &rng, env, source_env, radius, name);
            comparisons += 1;
            for (index, record) in c["records"].as_array().unwrap().iter().enumerate() {
                let action = &record["action"];
                match action["op"].as_str().unwrap() {
                    "prime" => {
                        assert!(
                            pending.is_empty(),
                            "{name}: source prime after boundary command"
                        );
                        prime_units(&mut m, c, &action["units"]);
                        refresh(&mut m);
                    }
                    "command" => {
                        let key = order_key(action);
                        assert_eq!(key.tick, m.world.game.tick as u64 + 1);
                        let command = Command::from_json(&action["cmd"])
                            .expect("captured source command must parse");
                        pending.push((key, command, record["result"].clone()));
                    }
                    "step" => {
                        pending.sort_by_key(|r| r.0);
                        let oracle = pending
                            .iter()
                            .map(|(k, _, d)| (*k, d.clone()))
                            .collect::<Vec<_>>();
                        if reverse_arrival {
                            pending.reverse();
                        }
                        for (key, command, _) in pending.drain(..) {
                            m.enqueue_command(key, command).unwrap();
                        }
                        // Exactly one actual runner invocation for this one source sim.step.
                        let report = m.advance_tick().unwrap_or_else(|error| {
                            panic!("{name}: actual full carrier tick failed: {error}")
                        });
                        assert!(
                            !report.unavailable.contains(&TickPhase::Riding),
                            "{name}: real riding/boarding tick dependency is unavailable"
                        );
                        assert!(
                            report.phases.contains(&TickPhase::Riding),
                            "{name}: missing actual Riding receipt"
                        );
                        assert!(
                            !report.unavailable.contains(&TickPhase::GroundEffects),
                            "{name}: ground-effects phase must be available"
                        );
                        assert_eq!(
                            report.commands.len(),
                            oracle.len(),
                            "{name}: command receipt count"
                        );
                        for (actual, (key, source)) in report.commands.iter().zip(&oracle) {
                            assert_eq!(actual.key, *key, "{name}: source command order");
                            let denial = actual.denial.map_or(Value::Null, |d| json!(d.name()));
                            assert_eq!(denial, *source, "{name}: actual boundary denial {:?}", key);
                        }
                        commands += oracle.len();
                        steps += 1;
                    }
                    other => panic!("unknown source carrier operation {other}"),
                }
                oracle_patch(&mut expected, &c["patches"][index]);
                assert_eq!(
                    record["lastSnapshot"].as_u64().unwrap(),
                    index as u64 + 1,
                    "source patch receipt"
                );
                if action["op"] == "step" {
                    compare(
                        &m,
                        &expected,
                        &rng,
                        env,
                        source_env,
                        radius,
                        &format!("{name} post-step {index} reverse={reverse_arrival}"),
                    );
                    assert_eq!(m.world.game.wear, wear, "{name}: quiet native ground wear");
                    assert_eq!(
                        m.world.surface_wear, surface_wear,
                        "{name}: quiet native surface wear"
                    );
                    comparisons += 1;
                }
            }
            assert!(
                pending.is_empty(),
                "{name}: unconsumed source command boundary"
            );
            assert_eq!(
                c["patches"].as_array().unwrap().len() + 1,
                c["snapshots"].as_u64().unwrap() as usize
            );
        }
    }
    assert_eq!(steps, fixture["steps"].as_u64().unwrap() as usize * 2);
    assert_eq!(commands, fixture["commands"].as_u64().unwrap() as usize * 2);
    assert_eq!(
        comparisons,
        (fixture["cases"].as_array().unwrap().len() + fixture["steps"].as_u64().unwrap() as usize)
            * 2
    );
}
