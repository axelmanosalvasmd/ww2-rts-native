use super::*;
use crate::logistics::Reserves;

pub(super) struct Harness {
    pub units: Vec<UnitView>,
    pub routes: Vec<(u32, Point, bool)>,
    pub failure: Option<RouteFailure>,
    pub handoff: bool,
    pub keep_route: bool,
    pub next: u32,
    pub rate: f64,
}
impl Default for Harness {
    fn default() -> Self {
        Self {
            units: vec![],
            routes: vec![],
            failure: None,
            handoff: true,
            keep_route: false,
            next: 100,
            rate: 0.,
        }
    }
}
impl World for Harness {
    fn units(&self) -> Vec<UnitView> {
        self.units.clone()
    }
    fn unit(&self, id: u32) -> Option<UnitView> {
        self.units.iter().find(|u| u.id == id).cloned()
    }
    fn route(&mut self, u: &UnitView, at: Point, safe: bool) -> Route {
        self.routes.push((u.id, at, safe));
        Route {
            route_end: Some(at),
            path: if self.failure.is_some() {
                vec![]
            } else {
                vec![at]
            },
            failure: self.failure,
        }
    }
    fn continue_route(&mut self, _: &UnitView, _: Point) -> bool {
        self.keep_route
    }
    fn handoff(&mut self, _: &UnitView, _: Point, _: Option<u32>, _: f64) -> bool {
        self.handoff
    }
    fn supply_rate(&mut self, _: usize, _: Point) -> Option<f64> {
        Some(self.rate)
    }
    fn spawn(&mut self, owner: usize, at: Point) -> u32 {
        let id = self.next;
        self.next += 1;
        self.units.push(unit(id, owner, UnitRole::Truck, at));
        id
    }
    fn retire(&mut self, id: u32) {
        self.units.retain(|u| u.id != id);
    }
    fn apply(&mut self, id: u32, operation: UnitOperation) {
        let u = self.units.iter_mut().find(|u| u.id == id).unwrap();
        match operation {
            UnitOperation::Route {
                path, world_goal, ..
            } => {
                u.path = path;
                u.world_goal = Some(world_goal);
            }
            UnitOperation::ClearRoute | UnitOperation::ResumeClear => {
                u.path.clear();
                u.world_goal = None;
                if operation == UnitOperation::ResumeClear {
                    u.orders.clear();
                    u.orders_pending = false;
                }
            }
            UnitOperation::RepairRoute(path) => {
                u.path = path;
                u.stuck = 0.;
            }
            UnitOperation::NewTruck => {}
        }
    }
}
pub(super) fn unit(id: u32, owner: usize, role: UnitRole, at: Point) -> UnitView {
    UnitView {
        projected: false,
        id,
        owner: Some(owner),
        role,
        hp: 100.,
        built: 1.,
        at,
        still: 3.,
        cargo_len: 0,
        region_cache: false,
        riding: None,
        path: vec![],
        world_goal: None,
        orders: vec![],
        orders_pending: false,
        amove: None,
        stuck: 0.,
    }
}
pub(super) fn player(slot: usize, team: i32, spawn: Point) -> Player {
    Player {
        slot,
        team,
        spawn,
        out: false,
        away: false,
    }
}
pub(super) fn context(players: &[Player], tick: u64) -> Context<'_> {
    Context {
        enabled: true,
        construction: false,
        tick,
        winner: false,
        army_pop: 1.,
        players,
        width: 1,
        chars: b".",
        aid: &[],
        points: &[],
    }
}
pub(super) fn reserve(provisions: f64) -> Reserves {
    Reserves {
        ammo: 100.,
        ammo_max: 100.,
        ammo_price: 0.01,
        ammo_credit: None,
        provisions,
        fuel: None,
        emergency: None,
        warning: None,
        forced: false,
        stranded: false,
        exhausted: false,
        supply_rate: Some(0.),
        cut_for: Some(10.),
    }
}
pub(super) fn store(id: StoreId, owner: usize, at: Point, source: bool) -> Store {
    Store {
        id,
        owner,
        at,
        source,
        entity: None,
        capacity: capacity(1., 32.),
        buckets: vec![],
        donations: vec![],
        active: true,
        support: false,
        point: None,
    }
}
pub(super) fn setup() -> (ConvoyState, LogisticsState, Harness) {
    (
        ConvoyState::setup(true).unwrap(),
        LogisticsState::default(),
        Harness::default(),
    )
}
fn add_troop(w: &mut Harness, r: &mut LogisticsState, id: u32, at: Point, provisions: f64) {
    w.units.push(unit(id, 0, UnitRole::Other, at));
    r.insert(id, reserve(provisions));
}

#[test]
fn intent_log33_jobs_group_urgent_troops_reserve_members_and_fill_remote_stores() {
    let (mut s, mut r, mut w) = setup();
    let players = [
        player(0, 0, Point { x: 0., z: 0. }),
        player(1, 1, Point { x: 500., z: 0. }),
    ];
    // Equal provision ties keep caller insertion order; a group includes nearby healthy troops.
    for (id, x, p) in [
        (9, 100., 20.),
        (2, 105., 20.),
        (3, 110., 120.),
        (4, 140., 80.),
        (5, 160., 120.),
    ] {
        add_troop(&mut w, &mut r, id, Point { x, z: 0. }, p);
    }
    r.get_mut(4).unwrap().forced = true;
    r.get_mut(5).unwrap().cut_for = Some(9.99);
    let targets = s.targets(context(&players, 0), 0, &r, &mut w);
    assert_eq!(
        targets.iter().map(|t| t.id.clone()).collect::<Vec<_>>(),
        vec![TargetId::Unit(4), TargetId::Unit(9)]
    );
    assert_eq!(targets[0].priority, Some(-120.));
    assert_eq!(targets[1].members, vec![9, 2, 3]);
    assert_eq!(targets[1].need.provisions, 200.);
    // A collecting truck reserves all members before it carries cargo.
    let source = store(StoreId::Source(0), 0, players[0].spawn, true);
    let truck = s.add_truck(0, &source, &mut w);
    let c = s.trucks.get_mut(&truck).unwrap();
    c.state = TruckState::Collecting;
    c.target = Some(targets[1].clone());
    assert_eq!(s.targets(context(&players, 0), 0, &r, &mut w).len(), 1);
    s.trucks.get_mut(&truck).unwrap().state = TruckState::Loading;
    assert_eq!(s.targets(context(&players, 0), 0, &r, &mut w).len(), 1);
    for (id, x, support) in [(10, 180., true), (11, 190., false), (12, 340., false)] {
        let mut cache = store(StoreId::Cache(id), 0, Point { x, z: 0. }, false);
        cache.support = support;
        s.stores.push(cache);
    }
    s.stores.push(store(
        StoreId::Cache(13),
        1,
        Point { x: 180., z: 0. },
        false,
    ));
    let jobs = s.targets(context(&players, 0), 0, &r, &mut w);
    assert_eq!(
        jobs.iter().map(|t| t.id.clone()).collect::<Vec<_>>(),
        vec![
            TargetId::Unit(4),
            TargetId::Store(StoreId::Cache(10)),
            TargetId::Store(StoreId::Cache(11)),
            TargetId::Store(StoreId::Cache(12))
        ]
    );
    assert_eq!(jobs[1].priority, Some(170.));
    assert_eq!(jobs[2].priority, Some(180.));
    assert_eq!(jobs[1].need, capacity(1., 16.));
    // Store demand needs a nearby own troop, a shortage and no territorial service.
    for case in ["supplied", "distant", "half-full", "inactive", "source"] {
        let (mut s, mut r, mut w) = setup();
        add_troop(&mut w, &mut r, 1, Point { x: 100., z: 0. }, 120.);
        let mut cache = store(StoreId::Cache(10), 0, Point { x: 150., z: 0. }, false);
        match case {
            "supplied" => w.rate = 0.5,
            "distant" => cache.at.x = 300.,
            "half-full" => cache.buckets.push((0, capacity(1., 16.))),
            "inactive" => cache.active = false,
            "source" => cache.source = true,
            _ => unreachable!(),
        }
        s.stores.push(cache);
        assert!(
            s.targets(context(&players, 0), 0, &r, &mut w).is_empty(),
            "{case}"
        );
    }
    let (mut ordered, mut reserves, mut world) = setup();
    for (id, x, provisions) in [(8, 100., 70.), (6, 140., 10.), (4, 180., 40.)] {
        add_troop(
            &mut world,
            &mut reserves,
            id,
            Point { x, z: 0. },
            provisions,
        );
    }
    assert_eq!(
        ordered
            .targets(context(&players, 0), 0, &reserves, &mut world)
            .iter()
            .map(|t| t.unit.unwrap())
            .collect::<Vec<_>>(),
        vec![6, 4, 8]
    );
    // Every individual shortage threshold starts a job only after the cut grace period.
    for case in ["provisions", "ammo", "fuel", "forced", "healthy", "grace"] {
        let mut r = LogisticsState::default();
        let mut w = Harness::default();
        add_troop(&mut w, &mut r, 1, Point { x: 100., z: 0. }, 120.);
        let a = r.get_mut(1).unwrap();
        match case {
            "provisions" => a.provisions = 90.,
            "ammo" => a.ammo = 75.,
            "fuel" => a.fuel = Some(135.),
            "forced" => a.forced = true,
            "grace" => {
                a.provisions = 0.;
                a.cut_for = Some(9.99);
            }
            _ => {}
        }
        let mut s = ConvoyState::setup(true).unwrap();
        assert_eq!(
            s.targets(context(&players, 0), 0, &r, &mut w).len(),
            usize::from(!matches!(case, "healthy" | "grace")),
            "{case}"
        );
    }
}

#[test]
fn intent_log35_dispatch_carries_origin_search_and_later_players_with_shared_budget() {
    let (mut s, mut r, mut w) = setup();
    let players = [
        player(0, 0, Point { x: 0., z: 0. }),
        player(1, 0, Point { x: 0., z: 40. }),
    ];
    for i in 0..5 {
        s.stores.push(store(
            StoreId::Source(i),
            0,
            Point {
                x: f64::from(i) * 20.,
                z: 0.,
            },
            true,
        ));
    }
    add_troop(&mut w, &mut r, 1, Point { x: 300., z: 0. }, 20.);
    w.units
        .push(unit(2, 1, UnitRole::Other, Point { x: 300., z: 40. }));
    r.insert(2, reserve(20.));
    // Keep every origin outside arrival reach, including the nearest one.
    let mut source = s.stores[0].clone();
    source.at.x = -100.;
    let first = s.add_truck(0, &source, &mut w);
    let second = s.add_truck(1, &source, &mut w);
    s.schedule_queue = VecDeque::from([0, 1]);
    w.failure = Some(RouteFailure::Blocked);
    s.schedule(context(&players, 0), &r, &mut w);
    assert_eq!(s.routed, 3);
    assert_eq!(s.assignments[&first].origin, 3);
    assert_eq!(s.schedule_queue, VecDeque::from([0, 1]));
    // The next tick resumes at the fourth origin, rather than trying the first three again.
    s.routed = 0;
    s.schedule(context(&players, 1), &r, &mut w);
    assert!(s.scheduled.contains(&first));
    assert_eq!(s.routed, 3);
    assert_eq!(s.assignments[&second].origin, 1);
    assert_eq!(s.schedule_queue, VecDeque::from([1]));
    for tick in 2..6 {
        s.routed = 0;
        s.schedule(context(&players, tick), &r, &mut w);
        assert!(s.routed <= 3);
    }
    assert!(s.schedule_queue.is_empty());
    assert_eq!(w.routes.len(), 10);
    // Refresh consumes the same counter as assignment.
    s.scheduled.clear();
    s.assignments.clear();
    s.schedule_queue.push_back(0);
    s.routed = 2;
    s.schedule(context(&players, 40), &r, &mut w);
    assert_eq!(s.routed, 3);
    assert_eq!(s.assignments[&first].origin, 1);
}

#[test]
fn intent_log35_cycle_counts_jobs_before_fleet_and_keeps_unfinished_queues() {
    let (mut s, mut r, mut w) = setup();
    let players = [player(0, 0, Point { x: 0., z: 0. })];
    add_troop(&mut w, &mut r, 1, Point { x: 100., z: 0. }, 10.);
    s.step(context(&players, 0), &mut r, &mut w, 0.).unwrap();
    assert_eq!(
        s.trucks.len(),
        1,
        "fresh demand sizes a fleet during this cycle"
    );
    assert_eq!(s.next_schedule, 40);
    assert_eq!(s.trucks.values().next().unwrap().state, TruckState::Loading);
    s.step(context(&players, 39), &mut r, &mut w, 0.).unwrap();
    assert_eq!(s.next_schedule, 40);
    s.step(context(&players, 40), &mut r, &mut w, 0.).unwrap();
    assert_eq!(s.next_schedule, 80);
    // An old reverse-order refresh queue must survive a dispatch boundary.
    let (mut s, mut r, mut w) = setup();
    let source = store(StoreId::Source(0), 0, players[0].spawn, true);
    s.stores.push(source.clone());
    add_troop(&mut w, &mut r, 1, Point { x: 300., z: 0. }, 10.);
    let mut ids = vec![];
    for i in 0..5 {
        let id = s.add_truck(0, &source, &mut w);
        let c = s.trucks.get_mut(&id).unwrap();
        c.state = TruckState::Returning;
        c.destination = None;
        w.units.iter_mut().find(|u| u.id == id).unwrap().at = Point {
            x: 100. + f64::from(i),
            z: 0.,
        };
        ids.push(id);
    }
    let idle = s.add_truck(0, &source, &mut w);
    w.units.iter_mut().find(|u| u.id == idle).unwrap().at.x = 120.;
    s.next_spawn.insert(0, 10000.);
    s.refresh_queue = ids.iter().rev().copied().collect();
    s.schedule_queue = VecDeque::from([0]);
    s.next_schedule = 40;
    s.step(context(&players, 40), &mut r, &mut w, 0.).unwrap();
    assert_eq!(
        w.routes.iter().map(|v| v.0).collect::<Vec<_>>(),
        vec![ids[4], ids[3], ids[2]]
    );
    assert_eq!(s.refresh_queue, VecDeque::from([ids[1], ids[0]]));
    assert_eq!(s.schedule_queue, VecDeque::from([0]));
    assert_eq!(s.routed, 3);
    s.step(context(&players, 41), &mut r, &mut w, 0.).unwrap();
    assert_eq!(
        w.routes.iter().map(|v| v.0).collect::<Vec<_>>(),
        vec![ids[4], ids[3], ids[2], ids[1], ids[0], idle]
    );
    assert_eq!(s.routed, 3, "two refreshes leave only one scheduling plan");
    assert!(s.refresh_queue.is_empty());
    assert_eq!(s.trucks[&idle].state, TruckState::Collecting);
}

#[test]
fn intent_log36_collect_load_unload_retarget_and_return_conserve_cargo() {
    let (mut s, mut r, mut w) = setup();
    let players = [player(0, 0, Point { x: 0., z: 0. })];
    let source = store(StoreId::Source(0), 0, players[0].spawn, true);
    s.stores.push(source.clone());
    add_troop(&mut w, &mut r, 1, Point { x: 100., z: 0. }, 0.);
    let id = s.add_truck(0, &source, &mut w);
    s.next_schedule = 10000;
    s.next_spawn.insert(0, 10000.);
    let target = s.targets(context(&players, 0), 0, &r, &mut w).remove(0);
    let mut c = s.trucks.remove(&id).unwrap();
    let u = w.unit(id).unwrap();
    assert!(s.assign(
        context(&players, 0),
        &u,
        &mut c,
        target,
        source,
        &mut w,
        false
    ));
    assert_eq!(c.state, TruckState::Loading);
    assert!(w.unit(id).unwrap().path.is_empty());
    s.trucks.insert(id, c);
    for tick in 1..80 {
        s.step(context(&players, tick), &mut r, &mut w, 0.05)
            .unwrap();
        assert_eq!(s.trucks[&id].state, TruckState::Loading);
        assert_eq!(s.trucks[&id].cargo, empty());
    }
    s.step(context(&players, 80), &mut r, &mut w, 0.051)
        .unwrap();
    assert_eq!(s.trucks[&id].state, TruckState::Delivering);
    assert_eq!(s.trucks[&id].cargo.provisions, 120.);
    let truck = w.units.iter_mut().find(|u| u.id == id).unwrap();
    truck.at = Point { x: 100., z: 0. };
    truck.path.clear();
    truck.world_goal = None;
    s.step(context(&players, 81), &mut r, &mut w, 0.).unwrap();
    assert_eq!(s.trucks[&id].state, TruckState::Unloading);
    s.step(context(&players, 82), &mut r, &mut w, 1.).unwrap();
    assert_eq!(r.get(1).unwrap().provisions, 15.);
    assert_eq!(s.trucks[&id].cargo.provisions, 105.);
    // A recipient that moves out of reach is followed after the remaining unload timer expires.
    w.units.iter_mut().find(|u| u.id == 1).unwrap().at.x = 200.;
    s.step(context(&players, 83), &mut r, &mut w, 7.).unwrap();
    assert_eq!(s.trucks[&id].state, TruckState::Delivering);
    assert_eq!(s.trucks[&id].target.as_ref().unwrap().at.x, 200.);
    assert_eq!(s.trucks[&id].cargo.provisions, 105.);
    // Deposit only available space and retain surplus, without duplicating the payer's cargo.
    let origin = s.stores.first_mut().unwrap();
    origin.source = false;
    origin.capacity.provisions = 10.;
    let c = s.trucks.get_mut(&id).unwrap();
    c.state = TruckState::Returning;
    c.destination = Some(players[0].spawn);
    let truck = w.units.iter_mut().find(|u| u.id == id).unwrap();
    truck.at = players[0].spawn;
    truck.path.clear();
    truck.world_goal = None;
    s.step(context(&players, 84), &mut r, &mut w, 0.).unwrap();
    assert_eq!(s.trucks[&id].state, TruckState::Idle);
    assert_eq!(s.trucks[&id].cargo.provisions, 95.);
    assert_eq!(s.stores[0].total().provisions, 10.);
    s.step(context(&players, 85), &mut r, &mut w, 0.).unwrap();
    assert_eq!(s.stores[0].total().provisions, 10.);
    assert_eq!(s.trucks[&id].cargo.provisions, 95.);
}

#[test]
fn intent_log36_group_priority_handoff_capacity_and_staggered_waiting() {
    let (mut s, mut r, mut w) = setup();
    let players = [player(0, 0, Point { x: 0., z: 0. })];
    let source = store(StoreId::Source(0), 0, players[0].spawn, true);
    s.stores.push(source.clone());
    add_troop(&mut w, &mut r, 1, Point { x: 100., z: 0. }, 60.);
    add_troop(&mut w, &mut r, 2, Point { x: 112., z: 0. }, 0.);
    let id = s.add_truck(0, &source, &mut w);
    let mut c = s.trucks.remove(&id).unwrap();
    c.target = Some(Target {
        id: TargetId::Unit(1),
        unit: Some(1),
        store: None,
        owner: 0,
        at: Point { x: 100., z: 0. },
        members: vec![1, 2],
        need: capacity(1., 100.),
        priority: None,
        relief: None,
    });
    let mut ctx = context(&players, 0);
    ctx.army_pop = 0.5;
    s.load_job(ctx, &w.unit(id).unwrap(), &mut c, &mut w);
    assert_eq!(
        c.cargo,
        capacity(0.5, 8.),
        "cargo scales with army population"
    );
    let target = c.target.clone().unwrap();
    c.cargo = Stock {
        provisions: 10.,
        ..empty()
    };
    c.state = TruckState::Unloading;
    c.timer = 8.;
    w.units.iter_mut().find(|u| u.id == id).unwrap().at.x = 100.;
    let truck = w.unit(id).unwrap();
    s.deliver(ctx, &truck, &mut c, &mut r, &mut w, 1.);
    assert_eq!(
        r.get(2).unwrap().provisions,
        10.,
        "lowest provisions receives scarce cargo at the inclusive 12 m boundary"
    );
    assert_eq!(r.get(1).unwrap().provisions, 60.);
    c.cargo.provisions = 10.;
    c.target = Some(target.clone());
    c.state = TruckState::Unloading;
    c.timer = 8.;
    w.handoff = false;
    s.deliver(ctx, &truck, &mut c, &mut r, &mut w, 1.);
    assert_eq!(
        c.cargo.provisions, 10.,
        "nearby disconnected recipients cannot receive cargo"
    );
    w.handoff = true;
    // Each waiting state retries only on its own modulo-40 beat.
    s.next_schedule = 10000;
    s.next_spawn.insert(0, 10000.);
    for (waiting, resumed) in [
        (TruckState::Waiting, TruckState::Delivering),
        (TruckState::WaitingCollect, TruckState::Collecting),
        (TruckState::WaitingReturn, TruckState::Returning),
    ] {
        c.state = waiting;
        c.target = Some(target.clone());
        c.retry = None;
        c.hold = false;
        c.destination = None;
        s.trucks.insert(id, c.clone());
        w.routes.clear();
        let beat = 200 + u64::from(id) % 40;
        s.step(context(&players, beat - 1), &mut r, &mut w, 0.)
            .unwrap();
        assert_eq!(s.trucks[&id].state, waiting);
        assert!(w.routes.is_empty());
        s.step(context(&players, beat), &mut r, &mut w, 0.).unwrap();
        assert_eq!(s.trucks[&id].state, resumed);
        assert_eq!(w.routes.len(), 1);
    }
    c.state = TruckState::Manual;
    c.manual = true;
    s.trucks.insert(id, c.clone());
    let truck = w.units.iter_mut().find(|u| u.id == id).unwrap();
    truck.path.clear();
    truck.world_goal = None;
    truck.orders_pending = true;
    s.step(context(&players, 300), &mut r, &mut w, 0.).unwrap();
    assert_eq!(s.trucks[&id].state, TruckState::Manual);
    w.units
        .iter_mut()
        .find(|u| u.id == id)
        .unwrap()
        .orders_pending = false;
    s.step(context(&players, 301), &mut r, &mut w, 0.).unwrap();
    assert_eq!(s.trucks[&id].state, TruckState::Idle);
    assert!(!s.trucks[&id].manual);
    c.state = TruckState::Loading;
    c.hold = true;
    c.timer = 4.;
    s.trucks.insert(id, c.clone());
    s.step(context(&players, 302), &mut r, &mut w, 1.).unwrap();
    assert_eq!(
        s.trucks[&id], c,
        "held trucks do not advance timers or cargo"
    );
}
