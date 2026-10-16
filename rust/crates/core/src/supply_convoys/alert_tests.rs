use super::*;
use crate::logistics::Reserves;

const PLAYERS: [Player; 2] = [
    Player {
        slot: 0,
        team: 0,
        spawn: Point { x: 0.0, z: 0.0 },
        out: false,
        away: false,
    },
    Player {
        slot: 1,
        team: 1,
        spawn: Point { x: 500.0, z: 0.0 },
        out: false,
        away: false,
    },
];

fn context(tick: u64) -> Context<'static> {
    Context {
        enabled: true,
        construction: false,
        tick,
        winner: false,
        army_pop: 1.0,
        players: &PLAYERS,
        width: 1,
        chars: b".",
        aid: &[],
        points: &[],
    }
}
fn unit(id: UnitId, owner: usize, role: UnitRole, x: f64) -> UnitView {
    UnitView {
        projected: false,
        id,
        owner: Some(owner),
        role,
        hp: 100.0,
        built: 1.0,
        at: Point { x, z: 0.0 },
        still: 0.0,
        cargo_len: 0,
        region_cache: false,
        riding: None,
        path: vec![],
        world_goal: None,
        orders: vec![],
        orders_pending: false,
        amove: None,
        stuck: 0.0,
    }
}
fn reserve(provisions: f64, ammo: f64) -> Reserves {
    Reserves {
        ammo,
        ammo_max: 100.0,
        ammo_price: 1.0,
        ammo_credit: None,
        provisions,
        fuel: None,
        emergency: None,
        warning: None,
        forced: false,
        stranded: false,
        exhausted: false,
        supply_rate: None,
        cut_for: Some(10.0),
    }
}
fn truck() -> Truck {
    Truck {
        state: TruckState::Idle,
        cargo: empty(),
        target: None,
        origin: StoreId::Source(0),
        timer: 0.0,
        manual: false,
        hold: false,
        route: vec![],
        destination: None,
        route_failure: None,
        retry: None,
    }
}
fn target(id: UnitId, members: Vec<UnitId>) -> Target {
    Target {
        id: TargetId::Unit(id),
        unit: Some(id),
        store: None,
        owner: 0,
        at: Point { x: 200.0, z: 0.0 },
        members,
        need: Stock {
            provisions: 100.0,
            ..empty()
        },
        priority: None,
        relief: None,
    }
}
#[derive(Default)]
struct AlertWorld {
    units: Vec<UnitView>,
    failure: Option<RouteFailure>,
    routes: usize,
    rates: BTreeMap<UnitId, f64>,
}
impl World for AlertWorld {
    fn units(&self) -> Vec<UnitView> {
        self.units.clone()
    }
    fn unit(&self, id: UnitId) -> Option<UnitView> {
        self.units.iter().find(|u| u.id == id).cloned()
    }
    fn route(&mut self, _: &UnitView, at: Point, _: bool) -> Route {
        self.routes += 1;
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
    fn handoff(&mut self, _: &UnitView, _: Point, _: Option<UnitId>, _: f64) -> bool {
        true
    }
    fn supply_rate(&mut self, _: usize, at: Point) -> Option<f64> {
        self.units
            .iter()
            .find(|u| u.at == at)
            .and_then(|u| self.rates.get(&u.id))
            .copied()
    }
    fn spawn(&mut self, _: usize, _: Point) -> UnitId {
        panic!("component must not spawn")
    }
    fn retire(&mut self, _: UnitId) {
        panic!("component must not retire")
    }
    fn apply(&mut self, _: UnitId, _: UnitOperation) {}
}
fn kinds(state: &ConvoyState) -> Vec<&str> {
    state.events.iter().map(|e| e.kind.as_str()).collect()
}

#[test]
fn alert_ids_order_and_cooldown_are_per_owner_and_type() {
    let mut state = ConvoyState::setup(true).unwrap();
    for (owner, kind) in [(0, "cut"), (1, "cut"), (0, "restored")] {
        state.alert(context(0), owner, kind, kind.into(), None, 1);
    }
    state.alert(context(399), 0, "cut", "suppressed".into(), None, 9);
    assert_eq!(state.next_event, 4);
    state.alert(context(400), 0, "cut", "boundary".into(), None, 2);
    assert_eq!(
        state
            .events
            .iter()
            .map(|e| (e.id, e.owner, e.kind.as_str(), e.count, e.seconds))
            .collect::<Vec<_>>(),
        vec![
            (1, 0, "cut", 1, 0.0),
            (2, 1, "cut", 1, 0.0),
            (3, 0, "restored", 1, 0.0),
            (4, 0, "cut", 2, 20.0)
        ]
    );
}

#[test]
fn alert_retention_keeps_thirty_second_boundary_and_latest_128() {
    let mut state = ConvoyState::setup(true).unwrap();
    state.alert(context(0), 0, "cut", String::new(), None, 1);
    state.alert(context(600), 0, "restored", String::new(), None, 1);
    assert_eq!(
        state.events.iter().map(|e| e.id).collect::<Vec<_>>(),
        vec![1, 2]
    );
    state.alert(context(601), 0, "low", String::new(), None, 1);
    assert_eq!(
        state.events.iter().map(|e| e.id).collect::<Vec<_>>(),
        vec![2, 3]
    );
    for owner in 0..129 {
        state.alert(context(601), owner, "stock", String::new(), None, 1);
    }
    assert_eq!(state.events.len(), 128);
    assert_eq!(state.events.first().unwrap().id, 5);
    assert_eq!(state.events.last().unwrap().id, 132);
}

#[test]
fn territory_alerts_group_cut_counts_and_emit_restored_before_cut() {
    let mut state = ConvoyState::setup(true).unwrap();
    let mut world = AlertWorld {
        units: vec![
            unit(1, 0, UnitRole::Other, 10.0),
            unit(2, 0, UnitRole::Other, 20.0),
            unit(3, 1, UnitRole::Other, 30.0),
        ],
        ..Default::default()
    };
    let mut reserves = LogisticsState::default();
    for id in 1..=3 {
        let mut r = reserve(120.0, 100.0);
        r.cut_for = Some(9.0);
        reserves.insert(id, r);
        world.rates.insert(id, 0.0);
    }
    state.territory_service(context(0), &mut reserves, &mut world, 0.5);
    assert!(state.events.is_empty());
    state.territory_service(context(10), &mut reserves, &mut world, 0.5);
    assert_eq!(
        state
            .events
            .iter()
            .map(|e| (e.owner, e.kind.as_str(), e.count, e.at))
            .collect::<Vec<_>>(),
        vec![
            (0, "cut", 2, Some(world.units[0].at)),
            (1, "cut", 1, Some(world.units[2].at))
        ]
    );
    assert_eq!(state.events[0].message, "2 units cut off from supply");
    world.rates.insert(1, 1.0);
    world.rates.insert(2, 1.0);
    let mut new_cut = reserve(120.0, 100.0);
    new_cut.cut_for = Some(9.5);
    reserves.insert(4, new_cut);
    world.units.push(unit(4, 1, UnitRole::Other, 40.0));
    world.rates.insert(4, 0.0);
    state.territory_service(context(410), &mut reserves, &mut world, 0.5);
    assert_eq!(kinds(&state), vec!["cut", "cut", "restored", "cut"]);
    assert_eq!(state.events[2].count, 1);
    assert_eq!(state.events[2].at, Some(world.units[0].at));
    assert_eq!(state.events[3].message, "1 unit cut off from supply");
}

#[test]
fn low_alert_uses_strict_thresholds_and_excludes_dead_and_other_owner_units() {
    let mut state = ConvoyState::setup(true).unwrap();
    let mut world = AlertWorld::default();
    let mut reserves = LogisticsState::default();
    for (id, provisions, ammo) in [
        (1, 30.0, 25.0),
        (2, 29.999, 100.0),
        (3, 120.0, 24.999),
        (4, 120.0, 0.0),
    ] {
        world.units.push(unit(id, 0, UnitRole::Other, id as f64));
        let mut r = reserve(provisions, ammo);
        if id == 4 {
            r.ammo_max = 0.0;
        }
        reserves.insert(id, r);
    }
    world.units.push(unit(5, 1, UnitRole::Other, 5.0));
    reserves.insert(5, reserve(0.0, 0.0));
    let mut dead = unit(6, 0, UnitRole::Other, 6.0);
    dead.hp = 0.0;
    world.units.push(dead);
    reserves.insert(6, reserve(0.0, 0.0));
    assert!(state.schedule_player(context(0), PLAYERS[0], &reserves, &mut world));
    assert_eq!(kinds(&state), vec!["low"]);
    assert_eq!(
        (state.events[0].count, state.events[0].at),
        (2, Some(world.units[1].at))
    );
    assert_eq!(state.events[0].message, "2 units running low on supplies");
}

#[test]
fn schedule_defers_grouped_shortage_alerts_until_real_route_budget_pass_finishes() {
    let mut state = ConvoyState::setup(true).unwrap();
    let mut world = AlertWorld {
        failure: Some(RouteFailure::Blocked),
        units: vec![
            unit(1, 0, UnitRole::Other, 200.0),
            unit(2, 0, UnitRole::Other, 300.0),
            unit(3, 0, UnitRole::Other, 205.0),
            unit(10, 0, UnitRole::Truck, 0.0),
            unit(11, 0, UnitRole::Truck, 10.0),
        ],
        ..Default::default()
    };
    let mut reserves = LogisticsState::default();
    let mut critical = reserve(5.0, 0.0);
    critical.forced = true;
    critical.stranded = true;
    reserves.insert(1, critical.clone());
    critical.stranded = false;
    reserves.insert(3, critical);
    reserves.insert(2, reserve(60.0, 100.0));
    state.trucks.insert(10, truck());
    let mut incoming = truck();
    incoming.state = TruckState::Delivering;
    incoming.target = Some(target(1, vec![1, 3, 1]));
    incoming.cargo.provisions = 10.0;
    state.trucks.insert(11, incoming);
    for id in 0..4 {
        state.ensure(
            context(0),
            StoreId::Source(id),
            0,
            Point {
                x: 50.0 + id as f64 * 40.0,
                z: 0.0,
            },
            true,
            None,
            32.0,
        );
    }
    state.schedule_queue.push_back(0);
    state.schedule(context(0), &reserves, &mut world);
    assert_eq!((world.routes, state.routed), (3, ROUTES));
    assert_eq!(state.schedule_queue.front(), Some(&0));
    assert!(state.events.is_empty());
    state.routed = 0;
    state.schedule(context(1), &reserves, &mut world);
    assert_eq!(world.routes, 4);
    assert!(state.schedule_queue.is_empty());
    assert_eq!(
        kinds(&state),
        vec!["blocked", "low", "withdrawal", "stranded", "transit"]
    );
    assert_eq!(
        state.events.iter().map(|e| e.count).collect::<Vec<_>>(),
        vec![1, 2, 2, 1, 2]
    );
    assert_eq!(
        state.events.iter().map(|e| e.id).collect::<Vec<_>>(),
        vec![1, 2, 3, 4, 5]
    );
}

#[test]
fn automatic_dispatch_alerts_stock_and_each_route_failure_reason() {
    for (failure, kind, message) in [
        (
            None,
            "stock",
            "No available supply stock or source for delivery",
        ),
        (
            Some(RouteFailure::Dangerous),
            "dangerous",
            "Supply delivery is waiting for a safe route",
        ),
        (
            Some(RouteFailure::Blocked),
            "blocked",
            "Supply route is blocked",
        ),
        (
            Some(RouteFailure::Unreachable),
            "unreachable",
            "No reachable ground route for supplies",
        ),
    ] {
        let mut state = ConvoyState::setup(true).unwrap();
        let mut world = AlertWorld {
            failure,
            units: vec![
                unit(1, 0, UnitRole::Other, 200.0),
                unit(10, 0, UnitRole::Truck, 0.0),
            ],
            ..Default::default()
        };
        let mut reserves = LogisticsState::default();
        reserves.insert(1, reserve(60.0, 100.0));
        state.trucks.insert(10, truck());
        if failure.is_some() {
            state.ensure(
                context(0),
                StoreId::Source(0),
                0,
                Point { x: 0.0, z: 0.0 },
                true,
                None,
                32.0,
            );
        }
        assert!(state.schedule_player(context(0), PLAYERS[0], &reserves, &mut world));
        assert_eq!(kinds(&state), vec![kind]);
        let event = &state.events[0];
        assert_eq!(
            (event.id, event.owner, event.count, event.at),
            (1, 0, 1, Some(world.units[0].at))
        );
        assert_eq!(event.message, message);
        state.scheduled.clear();
        assert!(state.schedule_player(context(1), PLAYERS[0], &reserves, &mut world));
        assert_eq!(state.events.len(), 1);
    }
}

#[test]
fn transit_counts_only_critical_members_of_live_loaded_delivery_or_unloading_trucks() {
    let mut state = ConvoyState::setup(true).unwrap();
    let mut world = AlertWorld::default();
    let mut reserves = LogisticsState::default();
    for id in 1..=6 {
        world.units.push(unit(id, 0, UnitRole::Other, id as f64));
        reserves.insert(id, reserve(if id == 6 { 60.0 } else { 5.0 }, 100.0));
    }
    for (id, status, members, loaded, owner, hp) in [
        (10, TruckState::Delivering, vec![1, 1, 6], true, 0, 100.0),
        (11, TruckState::Unloading, vec![2], true, 0, 100.0),
        (12, TruckState::Delivering, vec![3], false, 0, 100.0),
        (13, TruckState::Returning, vec![4], true, 0, 100.0),
        (14, TruckState::Delivering, vec![5], true, 1, 100.0),
        (15, TruckState::Delivering, vec![5], true, 0, 0.0),
    ] {
        let mut view = unit(id, owner, UnitRole::Truck, 100.0 + id as f64);
        view.hp = hp;
        world.units.push(view);
        let mut c = truck();
        c.state = status;
        c.target = Some(target(members[0], members));
        c.cargo.provisions = if loaded { 1.0 } else { 0.0 };
        state.trucks.insert(id, c);
    }
    assert!(state.schedule_player(context(0), PLAYERS[0], &reserves, &mut world));
    assert_eq!(kinds(&state), vec!["low", "transit"]);
    assert_eq!(state.events[0].count, 5);
    assert_eq!(state.events[1].count, 2);
    assert_eq!(
        state.events[1].message,
        "Supplies are in transit to 2 units"
    );
    assert_eq!(state.events[1].at, Some(world.units[0].at));
}
