//! Original private scheduler bodies, with explicit eligibility/search adapters.
use serde_json::{Value, json};
use std::collections::BTreeMap;
use ww2_core::game::Point;
use ww2_core::path_requests::{
    ClearedValue, PathRequestContext, PathRequests, PathUpdate, Request, RequestKind,
    SchedulerError, SearchOutcome, TickConfig,
};

fn point(v: &Value) -> Point {
    Point {
        x: v["x"].as_f64().unwrap(),
        z: v["z"].as_f64().unwrap(),
    }
}
fn json_point(p: Point) -> Value {
    json!({"x":p.x,"z":p.z})
}
fn kind(s: &str) -> RequestKind {
    match s {
        "recovery" => RequestKind::Recovery,
        "world" => RequestKind::World,
        "build" => RequestKind::Build,
        "dig" => RequestKind::Dig,
        "nade" => RequestKind::Nade,
        "enter" => RequestKind::Enter,
        "board" => RequestKind::Board,
        "amove" => RequestKind::Amove,
        "fireAt" => RequestKind::FireAt,
        "attackId" => RequestKind::AttackId,
        _ => panic!("unknown kind {s}"),
    }
}
const KINDS: [&str; 10] = [
    "recovery", "world", "build", "dig", "nade", "enter", "board", "amove", "fireAt", "attackId",
];
fn order(kind: RequestKind, at: Point) -> Value {
    match kind {
        RequestKind::Build
        | RequestKind::Board
        | RequestKind::AttackId
        | RequestKind::Enter
        | RequestKind::FireAt => json!(10),
        _ => json_point(at),
    }
}
struct Unit {
    incarnation: u64,
    hp: f64,
    eligible: bool,
    result: String,
    goals: BTreeMap<String, Point>,
    orders: BTreeMap<String, Value>,
    path: Vec<Point>,
    repath: f64,
    stuck: f64,
    outcome: Option<&'static str>,
    outcome_tick: Option<u64>,
    move_result: Option<Value>,
}
#[derive(Default)]
struct Context {
    units: BTreeMap<u32, Unit>,
    versions: BTreeMap<u32, u64>,
    searches: Vec<Value>,
    calls: u64,
    failed: u64,
    deferred: u64,
}
impl PathRequestContext for Context {
    type Error = &'static str;
    fn current_goal(&mut self, id: u32, k: RequestKind) -> Option<Point> {
        let u = self.units.get(&id)?;
        if u.hp <= 0.0 || !u.eligible {
            return None;
        }
        let active = match k {
            RequestKind::Recovery => true,
            RequestKind::Build | RequestKind::Board | RequestKind::AttackId => {
                u.orders[k.order_field()] != json!(0)
            }
            RequestKind::Enter | RequestKind::FireAt => u.orders[k.order_field()] != json!(-1),
            _ => !u.orders[k.order_field()].is_null(),
        };
        active.then(|| u.goals[k.name()])
    }
    fn unit_incarnation(&self, id: u32) -> Option<u64> {
        self.units.get(&id).map(|u| u.incarnation)
    }
    fn knowledge_version(&self, id: u32) -> u64 {
        *self.versions.get(&id).unwrap_or(&0)
    }
    fn search(&mut self, r: Request) -> Result<SearchOutcome, Self::Error> {
        let outcome = &self.units[&r.unit].result;
        if outcome == "error" {
            return Err("search rejected");
        }
        self.searches
            .push(json!([r.unit, r.goal.x, r.goal.z, outcome]));
        self.calls += 1;
        self.failed += u64::from(outcome == "failed");
        self.deferred += u64::from(outcome == "deferred");
        Ok(SearchOutcome {
            path: if outcome == "invalid" {
                vec![Point {
                    x: f64::NAN,
                    z: 0.0,
                }]
            } else if outcome == "success" {
                vec![r.goal]
            } else {
                Vec::new()
            },
            failed: outcome == "failed",
        })
    }
    fn apply(&mut self, update: PathUpdate) {
        let u = self.units.get_mut(&update.request.unit).unwrap();
        if let Some(path) = update.path {
            u.path = path;
        }
        u.repath = update.repath;
        if let Some(drop) = update.dropped {
            u.orders.insert(
                update.request.kind.order_field().to_owned(),
                match drop.value {
                    ClearedValue::Zero => json!(0),
                    ClearedValue::MinusOne => json!(-1),
                    ClearedValue::Null => Value::Null,
                },
            );
            u.path.clear();
            u.stuck = 0.0;
            u.outcome = Some("unreachable");
            u.outcome_tick = Some(drop.tick);
            u.move_result = Some(json!(["unreachable", drop.tick]));
        }
    }
}
fn add_unit(context: &mut Context, id: u32, incarnation: u64) {
    let mut unit = Unit {
        incarnation,
        hp: 1.0,
        eligible: true,
        result: "success".into(),
        goals: BTreeMap::new(),
        orders: BTreeMap::new(),
        path: vec![Point { x: -1.0, z: -1.0 }],
        repath: 0.0,
        stuck: 3.0,
        outcome: None,
        outcome_tick: None,
        move_result: None,
    };
    for name in KINDS {
        let k = kind(name);
        let at = Point {
            x: f64::from(id) + 10.0,
            z: f64::from(id) + 20.0,
        };
        unit.goals.insert(name.into(), at);
        unit.orders.insert(k.order_field().into(), order(k, at));
    }
    context.units.insert(id, unit);
}
fn snapshot(scheduler: &PathRequests, context: &Context) -> Value {
    json!({"searches":context.searches,"used":scheduler.used(),"queue":scheduler.queue_len(),"pending":scheduler.pending_len(),
        "stats":{"calls":context.calls,"failed":context.failed,"deferred":context.deferred+scheduler.stats().deferred,"dropped":scheduler.stats().dropped},
        "units":context.units.iter().map(|(&id,u)|{
            let orders:BTreeMap<_,_>=u.orders.iter().filter(|(name,_)|name.as_str()!="recovery").collect();
            json!({"id":id,"path":u.path.iter().copied().map(json_point).collect::<Vec<_>>(),"repath":u.repath,"stuck":u.stuck,"orders":orders,
                "moveOutcome":u.outcome,"moveOutcomeTick":u.outcome_tick,"moveResult":u.move_result})
        }).collect::<Vec<_>>()})
}

// JavaScript JSON has one numeric type; compare integer-looking values and
// floats as the same f64 while preserving every represented numeric bit.
fn numbers_as_bits(value: Value) -> Value {
    match value {
        Value::Number(n) => json!(format!("{:016x}", n.as_f64().unwrap().to_bits())),
        Value::Array(values) => Value::Array(values.into_iter().map(numbers_as_bits).collect()),
        Value::Object(values) => Value::Object(
            values
                .into_iter()
                .map(|(key, value)| (key, numbers_as_bits(value)))
                .collect(),
        ),
        other => other,
    }
}

#[test]
fn fifo_budget_retries_and_recovery_match_original_scheduler() {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/path-requests.json")).unwrap();
    assert_eq!(
        fixture["reference"],
        "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
    );
    assert_eq!(fixture["node"], "v24.21.0");
    assert_eq!(fixture["arch"], "x64");
    let mut actions = 0;
    let mut searches = 0;
    let mut dropped = 0;
    for scenario in fixture["scenarios"].as_array().unwrap() {
        let mut scheduler = PathRequests::new();
        let mut context = Context::default();
        let expected = scenario["expected"].as_array().unwrap();
        for (i, action) in scenario["actions"].as_array().unwrap().iter().enumerate() {
            context.searches.clear();
            let id = action["id"].as_u64().unwrap_or(0) as u32;
            match action["op"].as_str().unwrap() {
                "unit" => add_unit(
                    &mut context,
                    id,
                    action["incarnation"].as_u64().unwrap_or(1),
                ),
                "goal" => {
                    let k = kind(action["kind"].as_str().unwrap());
                    let at = point(&action["goal"]);
                    let u = context.units.get_mut(&id).unwrap();
                    u.goals.insert(k.name().into(), at);
                    u.orders.insert(k.order_field().into(), order(k, at));
                }
                "hp" => context.units.get_mut(&id).unwrap().hp = action["hp"].as_f64().unwrap(),
                "eligible" => {
                    context.units.get_mut(&id).unwrap().eligible =
                        action["value"].as_bool().unwrap()
                }
                "remove" => {
                    context.units.remove(&id);
                }
                "result" => {
                    context.units.get_mut(&id).unwrap().result =
                        action["value"].as_str().unwrap().into()
                }
                "knowledge" => {
                    context
                        .versions
                        .insert(id, action["version"].as_u64().unwrap());
                }
                "direct" => {
                    scheduler.clear_failures(id);
                    context
                        .search(Request {
                            unit: id,
                            kind: RequestKind::World,
                            goal: point(&action["goal"]),
                        })
                        .unwrap();
                }
                "begin" => {
                    scheduler
                        .begin_tick(
                            TickConfig {
                                budget: action["budget"].as_u64().unwrap_or(4096) as usize,
                                retry_limit: action["retry"].as_u64().unwrap_or(20) as u32,
                                tick: action["tick"].as_u64().unwrap(),
                                tick_seconds: fixture["tickSeconds"].as_f64().unwrap(),
                            },
                            &mut context,
                        )
                        .unwrap();
                }
                "request" => {
                    scheduler
                        .request(
                            Request {
                                unit: id,
                                kind: kind(action["kind"].as_str().unwrap()),
                                goal: point(&action["goal"]),
                            },
                            &mut context,
                        )
                        .unwrap();
                }
                other => panic!("unknown op {other}"),
            }
            assert_eq!(
                numbers_as_bits(snapshot(&scheduler, &context)),
                numbers_as_bits(expected[i].clone()),
                "{}, action {i}: {action}",
                scenario["name"]
            );
            actions += 1;
        }
        searches += context.calls;
        dropped += scheduler.stats().dropped;
    }
    assert_eq!(
        (actions, searches, dropped),
        (594, 201, 20),
        "reference coverage changed"
    );
}

#[test]
fn invalid_requests_and_callback_errors_are_not_successes() {
    let mut scheduler = PathRequests::new();
    let mut context = Context::default();
    add_unit(&mut context, 1, 1);
    let request = Request {
        unit: 1,
        kind: RequestKind::World,
        goal: Point { x: 10.0, z: 20.0 },
    };
    assert_eq!(
        scheduler.request(request, &mut context),
        Err(SchedulerError::NotStarted)
    );
    assert_eq!(
        scheduler.begin_tick(
            TickConfig {
                tick_seconds: f64::NAN,
                ..TickConfig::default()
            },
            &mut context
        ),
        Err(SchedulerError::InvalidTickDuration)
    );
    scheduler
        .begin_tick(TickConfig::default(), &mut context)
        .unwrap();
    assert_eq!(
        scheduler.request(
            Request {
                goal: Point {
                    x: f64::INFINITY,
                    z: 0.0
                },
                ..request
            },
            &mut context
        ),
        Err(SchedulerError::InvalidGoal)
    );
    assert_eq!(
        scheduler.request(Request { unit: 2, ..request }, &mut context),
        Err(SchedulerError::MissingUnit)
    );
    context.units.get_mut(&1).unwrap().result = "error".into();
    assert_eq!(
        scheduler.request(request, &mut context),
        Err(SchedulerError::Search("search rejected"))
    );
    assert_eq!(context.units[&1].path, vec![Point { x: -1.0, z: -1.0 }]);
    assert_eq!(scheduler.used(), 1);
    context.units.get_mut(&1).unwrap().result = "invalid".into();
    assert_eq!(
        scheduler.request(request, &mut context),
        Err(SchedulerError::InvalidSearchResult)
    );
    assert_eq!(context.units[&1].path, vec![Point { x: -1.0, z: -1.0 }]);
}
