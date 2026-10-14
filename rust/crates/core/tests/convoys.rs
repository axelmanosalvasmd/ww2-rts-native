use serde_json::{Value, json};
use ww2_core::{
    game::Point,
    logistics::{LogisticsState, Reserves, Stock},
    supply_convoys::*,
};
fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/convoys.json")).unwrap()
}
fn point(v: &Value) -> Point {
    Point {
        x: v["x"].as_f64().unwrap(),
        z: v["z"].as_f64().unwrap(),
    }
}
fn pjson(p: Point) -> Value {
    json!({"x":p.x,"z":p.z})
}
fn points(v: &Value) -> Vec<Point> {
    v.as_array().unwrap().iter().map(point).collect()
}
fn sj(s: Stock) -> Value {
    json!({"ammo":s.ammo,"provisions":s.provisions,"fuel":s.fuel})
}
fn reserves(v: &Value) -> Reserves {
    let opt = |k: &str| v[k].as_f64();
    Reserves {
        ammo: opt("ammo").unwrap(),
        ammo_max: opt("ammoMax").unwrap(),
        ammo_price: opt("ammoPrice").unwrap(),
        ammo_credit: opt("ammoCredit"),
        provisions: opt("provisions").unwrap(),
        fuel: opt("fuel"),
        emergency: opt("emergency"),
        warning: opt("warning"),
        forced: v["forced"].as_bool().unwrap(),
        stranded: v["stranded"].as_bool().unwrap(),
        exhausted: v["exhausted"].as_bool().unwrap(),
        supply_rate: opt("supplyRate"),
        cut_for: opt("cutFor"),
    }
}
fn rjson(r: &Reserves) -> Value {
    let mut v = json!({"ammo":r.ammo,"ammoMax":r.ammo_max,"ammoPrice":r.ammo_price,"ammoCredit":r.ammo_credit,"provisions":r.provisions,"fuel":r.fuel,"emergency":r.emergency,"warning":r.warning,"forced":r.forced,"stranded":r.stranded,"exhausted":r.exhausted});
    if let Some(n) = r.supply_rate {
        v["supplyRate"] = json!(n);
    }
    if let Some(n) = r.cut_for {
        v["cutFor"] = json!(n);
    }
    v
}
fn role(v: &str) -> UnitRole {
    match v {
        "hq" => UnitRole::Hq,
        "supplycache" => UnitRole::Cache,
        "halftrack" => UnitRole::Halftrack,
        "truck" => UnitRole::Truck,
        _ => UnitRole::Other,
    }
}
#[derive(Clone)]
struct TestWorld {
    units: Vec<Value>,
    next: u32,
    rate: f64,
    route_mode: String,
    handoff: bool,
}
impl TestWorld {
    fn view(v: &Value) -> UnitView {
        UnitView {
            projected: false,
            id: v["id"].as_u64().unwrap() as u32,
            owner: v["owner"].as_u64().map(|v| v as usize),
            role: role(v["type"].as_str().unwrap()),
            hp: v["hp"].as_f64().unwrap(),
            built: v["built"].as_f64().unwrap(),
            at: point(v),
            still: v["still"].as_f64().unwrap(),
            cargo_len: v["cargo"].as_array().unwrap().len(),
            region_cache: !v["regionCache"].is_null(),
            riding: v["riding"].as_u64().map(|v| v as u32),
            path: points(&v["path"]),
            world_goal: (!v["worldGoal"].is_null()).then(|| point(&v["worldGoal"])),
            orders: points(&v["orders"]),
            orders_pending: !v["orders"].as_array().unwrap().is_empty(),
            amove: (!v["amove"].is_null()).then(|| point(&v["amove"])),
            stuck: v["stuck"].as_f64().unwrap(),
        }
    }
}
impl World for TestWorld {
    fn units(&self) -> Vec<UnitView> {
        self.units.iter().map(Self::view).collect()
    }
    fn unit(&self, id: u32) -> Option<UnitView> {
        self.units.iter().find(|u| u["id"] == id).map(Self::view)
    }
    fn route(&mut self, _u: &UnitView, at: Point, _safe: bool) -> Route {
        Route {
            route_end: None,
            path: if self.route_mode == "bad" {
                vec![]
            } else {
                vec![at]
            },
            failure: (self.route_mode == "bad").then_some(RouteFailure::Blocked),
        }
    }
    fn continue_route(&mut self, _u: &UnitView, _at: Point) -> bool {
        self.route_mode == "continue"
    }
    fn handoff(&mut self, _u: &UnitView, _at: Point, _entity: Option<u32>, _r: f64) -> bool {
        self.handoff
    }
    fn supply_rate(&mut self, _o: usize, _at: Point) -> Option<f64> {
        Some(self.rate)
    }
    fn spawn(&mut self, owner: usize, at: Point) -> u32 {
        let id = self.next;
        self.next += 1;
        self.units.push(json!({"id":id,"type":"truck","owner":owner,"x":at.x,"z":at.z,"hp":100,"built":1,"still":3,"cargo":[],"path":[],"worldGoal":null,"orders":[],"amove":null,"stuck":0,"attackId":0,"targetId":0,"retreating":false,"repath":0,"autoRetreat":true,"riding":null,"regionCache":null,"logistics":null,"convoy":null}));
        id
    }
    fn retire(&mut self, id: u32) {
        self.units.retain(|u| u["id"] != id);
    }
    fn apply(&mut self, id: u32, operation: UnitOperation) {
        let u = self.units.iter_mut().find(|u| u["id"] == id).unwrap();
        match operation {
            UnitOperation::Route {
                path,
                world_goal,
                repath,
            } => {
                u["path"] = json!(path.iter().map(|p| pjson(*p)).collect::<Vec<_>>());
                u["worldGoal"] = pjson(world_goal);
                u["repath"] = json!(repath);
                u["retreating"] = json!(false);
                u["attackId"] = json!(0);
                u["targetId"] = json!(0);
            }
            UnitOperation::ClearRoute => {
                u["path"] = json!([]);
                u["worldGoal"] = Value::Null;
            }
            UnitOperation::ResumeClear => {
                u["path"] = json!([]);
                u["worldGoal"] = Value::Null;
                u["orders"] = json!([]);
            }
            UnitOperation::RepairRoute(path) => {
                u["path"] = json!(path.iter().map(|p| pjson(*p)).collect::<Vec<_>>());
                u["stuck"] = json!(0);
            }
            UnitOperation::NewTruck => u["autoRetreat"] = json!(false),
        }
    }
}
fn tid(id: &TargetId) -> String {
    match id {
        TargetId::Unit(i) => format!("unit:{i}"),
        TargetId::Store(i) => i.key(),
    }
}
fn target(t: &Target) -> Value {
    let mut v = json!({"id":tid(&t.id),"owner":t.owner,"x":t.at.x,"z":t.at.z,"need":sj(t.need)});
    if let Some(id) = t.unit {
        v["unit"] = json!(id);
        v["members"] = json!(t.members);
    }
    if let Some(id) = &t.store {
        v["store"] = json!(id.key());
    }
    if let Some(relief) = t.relief {
        v["relief"] = json!(relief);
    }
    if let Some(priority) = t.priority {
        v["priority"] = json!(priority);
    }
    v
}
fn truck(c: &Truck) -> Value {
    json!({"state":c.state.name(),"cargo":sj(c.cargo),"target":c.target.as_ref().map(target),"origin":c.origin.key(),"timer":c.timer,"manual":c.manual,"hold":c.hold,"route":c.route.iter().map(|p|pjson(*p)).collect::<Vec<_>>(),"destination":c.destination.map(pjson),"routeFailure":c.route_failure.map(|r|match r{RouteFailure::Blocked=>"blocked",RouteFailure::Dangerous=>"dangerous",RouteFailure::Unreachable=>"unreachable"}),"retry":c.retry.as_ref().map(|r|json!({"key":format!("{},{}",r.key.0,r.key.1),"wait":r.wait,"at":r.at}))})
}
fn projection(s: &ConvoyState, r: &LogisticsState, w: &TestWorld) -> Value {
    let stores:Vec<_>=s.stores.iter().map(|v|json!({"id":v.id.key(),"owner":v.owner,"x":v.at.x,"z":v.at.z,"source":v.source,"entity":v.entity,"capacity":sj(v.capacity),"buckets":v.buckets.iter().map(|(p,b)|json!([p,sj(*b)])).collect::<Vec<_>>(),"donations":v.donations.iter().map(|(p,rs)|json!([p,rs.iter().map(|(q,b)|json!([q,sj(*b)])).collect::<Vec<_>>()])).collect::<Vec<_>>(),"active":v.active,"support":v.support,"point":v.point})).collect();
    let units: Vec<_> = w
        .units
        .iter()
        .map(|u| {
            let mut u = u.clone();
            let id = u["id"].as_u64().unwrap() as u32;
            u["logistics"] = r.get(id).map(rjson).unwrap_or(Value::Null);
            u["convoy"] = s.trucks.get(&id).map(truck).unwrap_or(Value::Null);
            u
        })
        .collect();
    json!({"snapshots":[],"stores":stores,"nextSpawn":s.next_spawn.iter().map(|(p,n)|json!([p,n])).collect::<Vec<_>>(),"losses":s.losses.iter().map(|(p,n)|json!([p,n])).collect::<Vec<_>>(),"events":s.events.iter().map(|e|json!({"id":e.id,"owner":e.owner,"type":e.kind,"count":e.count,"x":e.at.map(|p|p.x),"z":e.at.map(|p|p.z),"message":e.message,"at":e.seconds})).collect::<Vec<_>>(),"alerts":s.alerts.iter().map(|((p,k),n)|json!([format!("{p}:{k}"),n])).collect::<Vec<_>>(),"nextEvent":s.next_event,"nextSchedule":s.next_schedule,"refreshQueue":s.refresh_queue,"scheduleQueue":s.schedule_queue,"scheduled":s.scheduled,"assignments":s.assignments.iter().map(|(id,a)|json!([id,{"targets":a.targets.iter().map(tid).collect::<Vec<_>>(),"target":a.target,"origins":a.origins.as_ref().map(|rs|rs.iter().map(StoreId::key).collect::<Vec<_>>()),"origin":a.origin,"blocked":a.blocked,"noStock":a.no_stock}])).collect::<Vec<_>>(),"routed":s.routed,"jobs":s.jobs.iter().map(|(p,n)|json!([p,n])).collect::<Vec<_>>(),"units":units})
}
fn compare(a: &Value, b: &Value, path: &str) {
    if b.is_number() {
        let a = a
            .as_f64()
            .unwrap_or_else(|| panic!("{path} expected number got {a}"));
        let b = b.as_f64().unwrap();
        assert_eq!(a.to_bits(), b.to_bits(), "{path}");
        return;
    }
    match b {
        Value::Object(b) => {
            let a = a.as_object().unwrap();
            assert_eq!(a.len(), b.len(), "{path}: actual {a:?} expected {b:?}");
            for (k, v) in b {
                compare(
                    a.get(k)
                        .unwrap_or_else(|| panic!("{path}.{k}: missing; actual {a:?}")),
                    v,
                    &format!("{path}.{k}"),
                );
            }
        }
        Value::Array(b) => {
            let a = a.as_array().unwrap();
            assert_eq!(a.len(), b.len(), "{path}");
            for (i, v) in b.iter().enumerate() {
                compare(&a[i], v, &format!("{path}[{i}]"));
            }
        }
        _ => assert_eq!(a, b, "{path}"),
    }
}
#[test]
fn startup_and_delivery_scheduler_match_original_functions() {
    let f = fixture();
    for (i, scenario) in f["scenarios"].as_array().unwrap().iter().enumerate() {
        let mut state = ConvoyState::setup(true).unwrap();
        let mut world = TestWorld {
            units: scenario["initial"]["units"].as_array().unwrap().clone(),
            next: 20,
            rate: 1.0,
            route_mode: "good".into(),
            handoff: true,
        };
        let mut r = LogisticsState::default();
        for u in &world.units {
            if !u["logistics"].is_null() {
                r.insert(u["id"].as_u64().unwrap() as u32, reserves(&u["logistics"]));
            }
        }
        let mut players = [
            Player {
                slot: 0,
                team: 0,
                spawn: Point { x: 1.0, z: 1.0 },
                out: false,
                away: false,
            },
            Player {
                slot: 1,
                team: 0,
                spawn: Point { x: 21.0, z: 21.0 },
                out: false,
                away: false,
            },
        ];
        let mut chars = vec![b'.'; 144];
        chars[14] = b'A';
        let mut depots = [DepotPoint {
            at: Point { x: 9.0, z: 11.0 },
            owner: Some(0),
            contested: false,
            depot: true,
        }];
        let mut initial_projection = projection(&state, &r, &world);
        initial_projection["snapshots"] = json!([
            snapshot_json(
                state
                    .recipient_state(
                        Context {
                            enabled: true,
                            construction: scenario["mode"] != "conquest",
                            tick: 0,
                            winner: false,
                            army_pop: scenario["pop"].as_f64().unwrap(),
                            players: &players,
                            width: 12,
                            chars: &chars,
                            aid: &[(14, 0)],
                            points: &depots
                        },
                        0,
                        &r,
                        &world
                    )
                    .unwrap()
            ),
            snapshot_json(
                state
                    .recipient_state(
                        Context {
                            enabled: true,
                            construction: scenario["mode"] != "conquest",
                            tick: 0,
                            winner: false,
                            army_pop: scenario["pop"].as_f64().unwrap(),
                            players: &players,
                            width: 12,
                            chars: &chars,
                            aid: &[(14, 0)],
                            points: &depots
                        },
                        1,
                        &r,
                        &world
                    )
                    .unwrap()
            )
        ]);
        compare(&initial_projection, &scenario["initial"], "setup");
        let mut expected = scenario["initial"].clone();
        for (j, row) in scenario["steps"].as_array().unwrap().iter().enumerate() {
            let a = &row["action"];
            if a["restoreTeam"].as_bool().unwrap_or(false) {
                players[1].team = 0;
            }
            if a["teamChange"].as_bool().unwrap_or(false) {
                players[1].team = 1;
            }
            if let Some(rate) = a["rate"].as_f64() {
                world.rate = rate;
            }
            if let Some(mode) = a["route"].as_str() {
                world.route_mode = mode.into();
            }
            if a["stuckTrucks"].as_bool().unwrap_or(false) {
                for u in &mut world.units {
                    if u["type"] == "truck" && !u["path"].as_array().unwrap().is_empty() {
                        u["stuck"] = json!(1);
                    }
                }
            }
            if a["move"].as_bool().unwrap_or(false) {
                for u in &mut world.units {
                    if u["type"] == "truck" && !u["worldGoal"].is_null() {
                        u["x"] = u["worldGoal"]["x"].clone();
                        u["z"] = u["worldGoal"]["z"].clone();
                        u["path"] = json!([]);
                        u["worldGoal"] = Value::Null;
                    }
                }
            }
            if !a["reserve"].is_null() {
                let id = a["reserve"]["id"].as_u64().unwrap() as u32;
                let mut value = rjson(r.get(id).unwrap());
                for (k, v) in a["reserve"].as_object().unwrap() {
                    if k != "id" {
                        value[k] = v.clone();
                    }
                }
                *r.get_mut(id).unwrap() = reserves(&value);
            }
            if !a["patch"].is_null() {
                let id = &a["patch"]["id"];
                let u = world.units.iter_mut().find(|u| u["id"] == *id).unwrap();
                for (k, v) in a["patch"].as_object().unwrap() {
                    u[k] = v.clone();
                }
            }
            if !a["point"].is_null() {
                depots[0].contested = a["point"]["contested"].as_bool().unwrap();
                if let Some(owner) = a["point"]["owner"].as_u64() {
                    depots[0].owner = Some(owner as usize);
                }
            }
            let ctx = Context {
                enabled: true,
                construction: scenario["mode"] != "conquest",
                tick: a["tick"].as_u64().unwrap(),
                winner: false,
                army_pop: scenario["pop"].as_f64().unwrap(),
                players: &players,
                width: 12,
                chars: &chars,
                aid: &[(14, 0)],
                points: &depots,
            };
            if a["addTruck"].as_bool().unwrap_or(false) {
                let owner = a["owner"].as_u64().unwrap() as usize;
                let source = state
                    .stores
                    .iter()
                    .find(|s| s.source && s.owner == owner)
                    .unwrap()
                    .clone();
                state.add_truck(owner, &source, &mut world);
            }
            let truck_id = world
                .units
                .iter()
                .rfind(|u| u["type"] == "truck")
                .map(|u| u["id"].as_u64().unwrap() as u32);
            if let Some(id) = truck_id {
                if !a["seedCargo"].is_null() {
                    state.trucks.get_mut(&id).unwrap().cargo = Stock {
                        ammo: a["seedCargo"]["ammo"].as_f64().unwrap(),
                        provisions: a["seedCargo"]["provisions"].as_f64().unwrap(),
                        fuel: a["seedCargo"]["fuel"].as_f64().unwrap(),
                    };
                }
                if let Some(trip) = a["seedTrip"].as_str() {
                    let owner = world.unit(id).unwrap().owner.unwrap();
                    let origin = state
                        .stores
                        .iter()
                        .find(|s| s.source && s.owner == owner)
                        .unwrap()
                        .clone();
                    let c = state.trucks.get_mut(&id).unwrap();
                    c.state = TruckState::Returning;
                    c.origin = origin.id;
                    c.manual = false;
                    c.hold = false;
                    c.destination = Some(origin.at);
                    c.route = vec![Point { x: 70.0, z: 70.0 }, origin.at];
                    let u = world.units.iter_mut().find(|u| u["id"] == id).unwrap();
                    u["path"] = json!([pjson(Point { x: 70.0, z: 70.0 }), pjson(origin.at)]);
                    u["worldGoal"] = pjson(origin.at);
                    u["x"] = json!(100);
                    u["z"] = json!(100);
                    u["stuck"] = json!(if trip == "repair" { 1 } else { 0 });
                }
                if let Some(target) = a["supply"].as_u64() {
                    state
                        .supply(
                            ctx,
                            a["owner"].as_u64().unwrap() as usize,
                            &[id],
                            target as u32,
                            &r,
                            &mut world,
                        )
                        .unwrap();
                }
                if a["seedUnload"].as_bool().unwrap_or(false) {
                    let c = state.trucks.get_mut(&id).unwrap();
                    c.state = TruckState::Unloading;
                    c.timer = 8.0;
                    c.cargo = Stock {
                        ammo: 3.0,
                        provisions: 100.0,
                        fuel: 0.0,
                    };
                    let u = world.units.iter_mut().find(|u| u["id"] == id).unwrap();
                    u["x"] = json!(22);
                    u["z"] = json!(22);
                    u["path"] = json!([]);
                    u["worldGoal"] = Value::Null;
                    let member = r.get_mut(3).unwrap();
                    member.provisions = 0.0;
                    member.cut_for = Some(10.0);
                    assert_eq!(c.target.as_ref().unwrap().members, vec![3]);
                }
                if let Some(command) = a["command"].as_str() {
                    let command = match command {
                        "stop" => MovementCommand::Stop,
                        "retreat" => MovementCommand::Retreat { queue: false },
                        _ => MovementCommand::Move {
                            at: Some(Point {
                                x: a["x"].as_f64().unwrap(),
                                z: a["z"].as_f64().unwrap(),
                            }),
                            orders: vec![],
                            queue: a["queue"].as_bool().unwrap_or(false),
                        },
                    };
                    state
                        .prepare_movement(
                            ctx,
                            a["owner"].as_u64().unwrap() as usize,
                            &[id],
                            &command,
                            &mut world,
                        )
                        .unwrap();
                }
                if a["resume"].as_bool().unwrap_or(false) {
                    state.resume(
                        ctx,
                        a["owner"].as_u64().unwrap() as usize,
                        &[id],
                        &mut world,
                    );
                }
                if a["death"].as_bool().unwrap_or(false) {
                    state.convoy_death(ctx.tick, &world.unit(id).unwrap());
                    world.retire(id);
                }
            }
            if !a["support"].is_null() {
                let request = &a["support"];
                let result = state
                    .support_provision(
                        ctx,
                        request["id"].as_u64().unwrap() as u32,
                        request["owner"].as_u64().unwrap() as usize,
                        request["amount"].as_f64().unwrap(),
                        &mut r,
                        &mut world,
                    )
                    .unwrap();
                assert_eq!(result, row["supportResult"].as_bool().unwrap());
            }
            state
                .step(ctx, &mut r, &mut world, a["dt"].as_f64().unwrap())
                .unwrap();
            for delta in row["delta"].as_array().unwrap() {
                let mut destination = &mut expected;
                for key in delta[0].as_array().unwrap() {
                    destination = if let Some(i) = key.as_u64() {
                        &mut destination[i as usize]
                    } else {
                        &mut destination[key.as_str().unwrap()]
                    };
                }
                *destination = delta[1].clone();
            }
            let snapshots = json!([
                snapshot_json(state.recipient_state(ctx, 0, &r, &world).unwrap()),
                snapshot_json(state.recipient_state(ctx, 1, &r, &world).unwrap())
            ]);
            let mut projection = projection(&state, &r, &world);
            projection["snapshots"] = snapshots;
            compare(&projection, &expected, &format!("scenario{i}.step{j}"));
        }
    }
}
#[test]
fn finite_store_territory_restock_matches_original_javascript() {
    let f = fixture();
    let cases = f["finiteStores"].as_array().unwrap();
    assert_eq!(
        cases
            .iter()
            .map(|c| c["kind"].as_str().unwrap())
            .collect::<Vec<_>>(),
        [
            "full-rate",
            "half-rate",
            "zero-rate",
            "inactive",
            "source",
            "partially-occupied"
        ]
    );
    let stock = |v: &Value| Stock {
        ammo: v["ammo"].as_f64().unwrap(),
        provisions: v["provisions"].as_f64().unwrap(),
        fuel: v["fuel"].as_f64().unwrap(),
    };
    let players = [0, 1].map(|slot| Player {
        slot,
        team: 0,
        spawn: Point { x: 1.0, z: 1.0 },
        out: false,
        away: true,
    });
    let chars = *b"....";
    let mut ctx = Context {
        enabled: true,
        construction: false,
        tick: 0,
        winner: false,
        army_pop: 1.0,
        players: &players,
        width: 2,
        chars: &chars,
        aid: &[],
        points: &[],
    };
    for case in cases {
        let kind = case["kind"].as_str().unwrap();
        let mut world = TestWorld {
            units: vec![],
            next: 20,
            rate: case["rate"].as_f64().unwrap(),
            route_mode: "good".into(),
            handoff: true,
        };
        let mut r = LogisticsState::default();
        let mut state = ConvoyState::setup(true).unwrap();
        ctx.tick = 0;
        state.step(ctx, &mut r, &mut world, 0.0).unwrap();
        assert_eq!(state.next_schedule, 40);
        let initial = &case["initial"][0];
        state.stores.push(Store {
            id: StoreId::Cache(4),
            owner: 0,
            at: point(initial),
            source: initial["source"].as_bool().unwrap(),
            entity: None,
            capacity: stock(&initial["capacity"]),
            buckets: initial["buckets"]
                .as_array()
                .unwrap()
                .iter()
                .map(|b| (b[0].as_u64().unwrap() as usize, stock(&b[1])))
                .collect(),
            donations: vec![],
            active: initial["active"].as_bool().unwrap(),
            support: false,
            point: None,
        });
        let before = state.stores.clone();
        compare(
            &projection(&state, &r, &world)["stores"],
            &case["initial"],
            kind,
        );
        let steps = case["steps"].as_array().unwrap();
        assert_eq!(steps.len(), 5);
        for (i, row) in steps.iter().enumerate() {
            ctx.tick = row["tick"].as_u64().unwrap();
            assert_eq!(ctx.tick, i as u64 + 1);
            assert_eq!(
                row["dt"].as_f64().unwrap(),
                [10.0, 10.0, 20.0, 40.0, 60.0][i]
            );
            assert_eq!(row["elapsed"].as_u64().unwrap(), [10, 20, 40, 80, 140][i]);
            state
                .step(ctx, &mut r, &mut world, row["dt"].as_f64().unwrap())
                .unwrap();
            compare(
                &projection(&state, &r, &world)["stores"],
                &row["stores"],
                &format!("{kind}.step{i}"),
            );
            assert!(world.units.is_empty());
            assert!(state.trucks.is_empty() && state.jobs.is_empty() && state.events.is_empty());
            assert_eq!(state.next_schedule, 40);
            if matches!(kind, "zero-rate" | "inactive" | "source") {
                assert_eq!(state.stores, before);
            }
            let store = &state.stores[0];
            if kind == "full-rate" && i >= 2 || kind == "half-rate" && i >= 3 {
                compare(
                    &sj(store.total()),
                    &json!({"ammo":96,"provisions":3840,"fuel":5760}),
                    "full capacity",
                );
            }
            if kind == "half-rate" && i == 2 {
                compare(
                    &sj(store.total()),
                    &json!({"ammo":48,"provisions":1920,"fuel":2880}),
                    "half capacity at 40 seconds",
                );
            }
            if kind == "partially-occupied" {
                assert_eq!(store.buckets[0], before[0].buckets[0]);
                if i >= 2 {
                    compare(
                        &sj(store.total()),
                        &json!({"ammo":96,"provisions":3840,"fuel":5760}),
                        "occupied capacity",
                    );
                }
            }
        }
    }
}

#[test]
fn payer_grants_conserve_private_and_relief_stock() {
    let f = fixture();
    for row in f["pools"].as_array().unwrap() {
        let mut s = Store {
            id: StoreId::Cache(1),
            owner: 1,
            at: Point { x: 0.0, z: 0.0 },
            source: false,
            entity: Some(1),
            capacity: Stock {
                ammo: 20.0,
                provisions: 200.0,
                fuel: 300.0,
            },
            buckets: vec![
                (
                    0,
                    Stock {
                        ammo: 10.0,
                        provisions: 100.0,
                        fuel: 90.0,
                    },
                ),
                (
                    1,
                    Stock {
                        ammo: 2.0,
                        provisions: 10.0,
                        fuel: 20.0,
                    },
                ),
                (
                    2,
                    Stock {
                        ammo: 5.0,
                        provisions: 50.0,
                        fuel: 30.0,
                    },
                ),
            ],
            donations: vec![
                (
                    0,
                    vec![
                        (
                            1,
                            Stock {
                                ammo: 4.0,
                                provisions: 40.0,
                                fuel: 45.0,
                            },
                        ),
                        (
                            2,
                            Stock {
                                ammo: 2.0,
                                provisions: 20.0,
                                fuel: 15.0,
                            },
                        ),
                    ],
                ),
                (
                    2,
                    vec![(
                        1,
                        Stock {
                            ammo: 3.0,
                            provisions: 30.0,
                            fuel: 10.0,
                        },
                    )],
                ),
            ],
            active: true,
            support: false,
            point: None,
        };
        let payer = row["payer"].as_u64().unwrap() as usize;
        let relief = row["relief"].as_bool().unwrap();
        compare(&sj(s.available(payer, relief)), &row["before"], "before");
        s.consume(
            payer,
            Stock {
                ammo: 7.0,
                provisions: 65.0,
                fuel: 80.0,
            },
            relief,
        );
        compare(&sj(s.available(payer, relief)), &row["after"], "after");
        compare(
            &json!(
                s.buckets
                    .iter()
                    .map(|(p, s)| json!([p, sj(*s)]))
                    .collect::<Vec<_>>()
            ),
            &row["buckets"],
            "buckets",
        );
        compare(
            &json!(
                s.donations
                    .iter()
                    .map(|(p, rs)| json!([
                        p,
                        rs.iter()
                            .map(|(q, s)| json!([q, sj(*s)]))
                            .collect::<Vec<_>>()
                    ]))
                    .collect::<Vec<_>>()
            ),
            &row["donations"],
            "grants",
        );
        compare(&sj(s.total()), &row["total"], "total");
        compare(&sj(s.space()), &row["space"], "space");
    }
}

fn snapshot_json(s: RecipientState) -> Value {
    json!({"enabled":true,"units":s.units.iter().map(|r|json!({"id":r.id,"ammo":r.ammo,"provisions":r.provisions,"fuel":r.fuel,"emergency":r.emergency,"warning":r.warning,"forced":r.forced,"stranded":r.stranded,"supply":r.supply,"cut":r.cut,"grace":r.grace})).collect::<Vec<_>>(),"trucks":s.trucks.iter().map(|r|json!({"id":r.id,"state":r.state.name(),"manual":r.manual,"hold":r.hold,"cargo":sj(r.cargo),"destination":r.destination.map(pjson),"route":r.route.iter().map(|p|pjson(*p)).collect::<Vec<_>>()})).collect::<Vec<_>>(),"stores":s.stores.iter().map(|r|json!({"id":r.id.key(),"owner":r.owner,"x":r.at.x,"z":r.at.z,"source":r.source,"stock":sj(r.stock),"capacity":sj(r.capacity)})).collect::<Vec<_>>(),"events":s.events.iter().map(|e|json!({"id":e.id,"type":e.kind,"count":e.count,"x":e.at.map(|p|p.x),"z":e.at.map(|p|p.z),"message":e.message})).collect::<Vec<_>>()})
}
#[test]
fn invalid_tick_inputs_leave_every_owned_state_unchanged() {
    let f = fixture();
    let scenario = &f["scenarios"][0];
    let mut state = ConvoyState::setup(true).unwrap();
    let mut world = TestWorld {
        units: scenario["initial"]["units"].as_array().unwrap().clone(),
        next: 20,
        rate: 1.0,
        route_mode: "good".into(),
        handoff: true,
    };
    let mut reserves_state = LogisticsState::default();
    for u in &world.units {
        if !u["logistics"].is_null() {
            reserves_state.insert(u["id"].as_u64().unwrap() as u32, reserves(&u["logistics"]));
        }
    }
    let players = [
        Player {
            slot: 0,
            team: 0,
            spawn: Point { x: 1.0, z: 1.0 },
            out: false,
            away: false,
        },
        Player {
            slot: 1,
            team: 0,
            spawn: Point { x: 21.0, z: 21.0 },
            out: false,
            away: false,
        },
    ];
    let chars = [b'.'; 4];
    let ctx = Context {
        enabled: true,
        construction: false,
        tick: 0,
        winner: false,
        army_pop: 1.0,
        players: &players,
        width: 2,
        chars: &chars,
        aid: &[],
        points: &[],
    };
    let before = projection(&state, &reserves_state, &world);
    for dt in [-1.0, f64::NAN, f64::INFINITY] {
        assert_eq!(
            state.step(ctx, &mut reserves_state, &mut world, dt),
            Err(ConvoyError::InvalidTime)
        );
        compare(
            &projection(&state, &reserves_state, &world),
            &before,
            "atomic",
        );
    }
    let mut invalid = ctx;
    invalid.tick = u64::MAX;
    assert_eq!(
        state.step(invalid, &mut reserves_state, &mut world, 0.0),
        Err(ConvoyError::InvalidContext)
    );
    compare(
        &projection(&state, &reserves_state, &world),
        &before,
        "tick overflow",
    );
    for amount in [-1.0, f64::NAN, f64::INFINITY] {
        assert_eq!(
            state.support_provision(ctx, 1, 0, amount, &mut reserves_state, &mut world),
            Err(ConvoyError::InvalidQuantity)
        );
        compare(
            &projection(&state, &reserves_state, &world),
            &before,
            "support atomic",
        );
    }
    assert!(
        !state
            .supply(ctx, 0, &[], 4, &reserves_state, &mut world)
            .unwrap()
    );
    assert!(
        !state
            .supply(ctx, 0, &[1], 4, &reserves_state, &mut world)
            .unwrap()
    );
    compare(
        &projection(&state, &reserves_state, &world),
        &before,
        "no actionable trucks",
    );
    invalid = ctx;
    invalid.width = 0;
    assert_eq!(
        state.step(invalid, &mut reserves_state, &mut world, 0.0),
        Err(ConvoyError::InvalidContext)
    );
    compare(
        &projection(&state, &reserves_state, &world),
        &before,
        "shape",
    );
    assert!(ConvoyState::setup(false).is_none());
}

#[test]
fn support_spending_matches_original_source_and_canonical_unit_fallback() {
    let f = fixture();
    for row in f["support"].as_array().unwrap() {
        let kind = row["kind"].as_str().unwrap();
        let mut unit = f["scenarios"][0]["initial"]["units"][1].clone();
        unit["owner"] = json!(if kind == "captured" { 1 } else { 0 });
        if kind == "dead" {
            unit["hp"] = json!(0);
        }
        let mut world = TestWorld {
            units: vec![unit.clone()],
            next: 20,
            rate: 0.0,
            route_mode: "good".into(),
            handoff: true,
        };
        let mut r = LogisticsState::default();
        r.insert(2, reserves(&unit["logistics"]));
        let players = [
            Player {
                slot: 0,
                team: 0,
                spawn: Point { x: 1.0, z: 1.0 },
                out: false,
                away: false,
            },
            Player {
                slot: 1,
                team: 0,
                spawn: Point { x: 21.0, z: 21.0 },
                out: false,
                away: false,
            },
        ];
        let chars = *b".";
        let ctx = Context {
            enabled: true,
            construction: false,
            tick: 0,
            winner: false,
            army_pop: 1.0,
            players: &players,
            width: 1,
            chars: &chars,
            aid: &[],
            points: &[],
        };
        let mut state = ConvoyState::setup(true).unwrap();
        let store = Store {
            id: StoreId::Cache(4),
            owner: 1,
            at: Point { x: 90.0, z: 90.0 },
            source: kind == "source",
            entity: Some(4),
            capacity: Stock {
                ammo: 20.0,
                provisions: 200.0,
                fuel: 300.0,
            },
            buckets: vec![(
                0,
                Stock {
                    ammo: 0.0,
                    provisions: 30.0,
                    fuel: 0.0,
                },
            )],
            donations: vec![(
                0,
                vec![(
                    1,
                    Stock {
                        ammo: 0.0,
                        provisions: 20.0,
                        fuel: 0.0,
                    },
                )],
            )],
            active: true,
            support: false,
            point: None,
        };
        if kind == "source" || kind == "grant" {
            state.stores.push(store);
        }
        assert_eq!(
            state
                .support_provision(
                    ctx,
                    2,
                    row["owner"].as_u64().unwrap() as usize,
                    5.0,
                    &mut r,
                    &mut world
                )
                .unwrap(),
            row["result"].as_bool().unwrap()
        );
        assert_eq!(
            r.get(2).unwrap().provisions.to_bits(),
            row["provisions"].as_f64().unwrap().to_bits()
        );
        if let Some(store) = state.stores.first() {
            compare(
                &json!(
                    store
                        .buckets
                        .iter()
                        .map(|(p, b)| json!([p, sj(*b)]))
                        .collect::<Vec<_>>()
                ),
                &row["buckets"],
                "support buckets",
            );
            compare(
                &json!(
                    store
                        .donations
                        .iter()
                        .map(|(p, rs)| json!([
                            p,
                            rs.iter()
                                .map(|(q, b)| json!([q, sj(*b)]))
                                .collect::<Vec<_>>()
                        ]))
                        .collect::<Vec<_>>()
                ),
                &row["donations"],
                "support grants",
            );
        }
    }
}
