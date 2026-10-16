//! Native feature checks use explicit component inputs, not recorded match state.
use ww2_core::{
    game::Point,
    logistics::{self, Definition, LogisticsState, Reserves, Stock, UnitFacts},
    supply_convoys::*,
};

fn at(x: f64) -> Point {
    Point { x, z: 1.0 }
}
fn facts(id: u32, kind: &str) -> UnitFacts<'_> {
    UnitFacts {
        id,
        kind,
        owner: Some(0),
        owner_exists: true,
        guard_home: false,
        riding: false,
    }
}
fn reserve_context() -> logistics::Context {
    logistics::Context {
        enabled_override: Some(true),
        scenario: false,
        mode: logistics::Mode::Conquest,
    }
}
fn equipped(kind: &str) -> LogisticsState {
    let mut state = LogisticsState::default();
    state
        .initialize_unit(
            reserve_context(),
            facts(1, kind),
            Some(&Definition::from_catalog(kind).unwrap()),
        )
        .unwrap();
    state
}
#[test]
fn feature_log02_eligible_units_keep_spent_reserves_on_reinitialization() {
    let mut state = equipped("rifle");
    state.consume_ammo(1, 7.0);
    let before = state.get(1).unwrap().clone();
    state.initialize_unit(
        reserve_context(),
        facts(1, "rifle"),
        Some(&Definition::from_catalog("rifle").unwrap()),
    );
    assert_eq!(state.get(1), Some(&before));
    for kind in ["hq", "tank", "rifle", "fighter", "gunboat", "truck"] {
        let def = Definition::from_catalog(kind).unwrap();
        for variant in 0..3 {
            let mut unit = facts(2, kind);
            match variant {
                0 => unit.owner = None,
                1 => unit.owner_exists = false,
                _ => unit.guard_home = true,
            }
            assert!(
                state
                    .initialize_unit(reserve_context(), unit, Some(&def))
                    .is_none()
            );
        }
    }
    for kind in ["hq", "fighter", "gunboat", "truck"] {
        assert!(
            state
                .initialize_unit(
                    reserve_context(),
                    facts(2, kind),
                    Some(&Definition::from_catalog(kind).unwrap())
                )
                .is_none()
        );
    }
    let mut definition = Definition::from_catalog("rifle").unwrap();
    definition.rail = true;
    assert!(
        state
            .initialize_unit(reserve_context(), facts(2, "rifle"), Some(&definition))
            .is_none()
    );
    definition.rail = false;
    definition.speed = 0.0;
    assert!(
        state
            .initialize_unit(reserve_context(), facts(2, "rifle"), Some(&definition))
            .is_none()
    );
    assert!(
        state
            .initialize_unit(
                reserve_context(),
                facts(2, "tank"),
                Some(&Definition::from_catalog("tank").unwrap())
            )
            .is_some()
    );
}
#[test]
fn feature_log06_exhaustion_counts_only_time_without_provisions() {
    for riding in [false, true] {
        let mut state = equipped("rifle");
        let mut unit = facts(1, "rifle");
        unit.riding = riding;
        state.tick_reserves(reserve_context(), unit, 119.5, 0.0);
        assert_eq!(state.get(1).unwrap().provisions, 0.5);
        assert_eq!(state.get(1).unwrap().warning, None);
        state.tick_reserves(reserve_context(), unit, 1.0, 0.0);
        assert_eq!(state.get(1).unwrap().warning, Some(19.5));
        assert!(state.get(1).unwrap().exhausted);
        state.tick_reserves(reserve_context(), unit, 19.0, 0.0);
        assert!(!state.get(1).unwrap().forced);
        state.tick_reserves(reserve_context(), unit, 0.5, 0.0);
        assert!(state.get(1).unwrap().forced);
    }
}
#[test]
fn feature_log07_only_powered_travel_uses_fuel_then_emergency() {
    let mut state = equipped("tank");
    state.tick_reserves(reserve_context(), facts(1, "tank"), 5.0, 0.0);
    assert_eq!(state.get(1).unwrap().fuel, Some(180.0));
    let mut riding = facts(1, "tank");
    riding.riding = true;
    state.tick_reserves(reserve_context(), riding, 5.0, 5.0);
    assert_eq!(state.get(1).unwrap().fuel, Some(180.0));
    state.tick_reserves(reserve_context(), facts(1, "tank"), 1.0, 100.0);
    assert_eq!(state.get(1).unwrap().fuel, Some(179.0));
    state.consume_driving_fuel(facts(1, "tank"), 180.0);
    let r = state.get(1).unwrap();
    assert_eq!(
        (r.fuel, r.emergency, r.forced, r.stranded),
        (Some(0.0), Some(59.0), true, false)
    );
    state.consume_driving_fuel(facts(1, "tank"), 59.0);
    assert!(state.get(1).unwrap().stranded);
}
#[test]
fn feature_log08_shortage_penalties_do_not_stack() {
    let mut state = equipped("rifle");
    assert_eq!(state.shortage_rate(1), 1.0);
    let r = state.get_mut(1).unwrap();
    r.ammo = r.ammo_max * 0.25;
    assert_eq!(state.shortage_rate(1), 1.0);
    state.get_mut(1).unwrap().ammo -= 1.0;
    assert_eq!(state.shortage_rate(1), 2.0);
    state.get_mut(1).unwrap().exhausted = true;
    assert_eq!(state.shortage_rate(1), 2.0);
    state.transfer_stock(
        1,
        Some(&mut Stock {
            ammo: 100.0,
            provisions: 0.0,
            fuel: 0.0,
        }),
        8.0,
    );
    // Partial relief does not release the exhaustion latch below half provisions.
    state.get_mut(1).unwrap().provisions = 20.0;
    state.get_mut(1).unwrap().exhausted = true;
    assert_eq!(state.shortage_rate(1), 2.0);
}
#[test]
fn feature_log09_recovery_thresholds_and_latch() {
    let mut state = equipped("rifle");
    for (provisions, exhausted, expected) in [
        (30.0, false, 1.0),
        (29.5, false, 0.5),
        (0.0, false, 0.0),
        (90.0, true, 0.0),
    ] {
        let r = state.get_mut(1).unwrap();
        r.provisions = provisions;
        r.exhausted = exhausted;
        assert_eq!(state.recovery_rate(1), expected);
    }
    assert_eq!(state.recovery_rate(99), 1.0);
}

fn unit(id: u32, owner: usize, role: UnitRole, x: f64) -> UnitView {
    UnitView {
        projected: false,
        id,
        owner: Some(owner),
        role,
        hp: 100.0,
        built: 1.0,
        at: at(x),
        still: 3.0,
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
struct ComponentWorld {
    units: Vec<UnitView>,
    rate: f64,
    handoff: bool,
    safe: bool,
    reuse: bool,
    plans: usize,
    rate_queries: Vec<Point>,
}
impl ComponentWorld {
    fn new(units: Vec<UnitView>) -> Self {
        Self {
            units,
            rate: 0.0,
            handoff: true,
            safe: true,
            reuse: false,
            plans: 0,
            rate_queries: vec![],
        }
    }
}
impl World for ComponentWorld {
    fn units(&self) -> Vec<UnitView> {
        self.units.clone()
    }
    fn unit(&self, id: u32) -> Option<UnitView> {
        self.units.iter().find(|u| u.id == id).cloned()
    }
    fn route(&mut self, _u: &UnitView, target: Point, _safe: bool) -> Route {
        self.plans += 1;
        Route {
            path: if self.safe { vec![target] } else { vec![] },
            route_end: None,
            failure: (!self.safe).then_some(RouteFailure::Blocked),
        }
    }
    fn continue_route(&mut self, _u: &UnitView, _target: Point) -> bool {
        self.reuse
    }
    fn handoff(&mut self, _u: &UnitView, _at: Point, _entity: Option<u32>, _radius: f64) -> bool {
        self.handoff
    }
    fn supply_rate(&mut self, _owner: usize, at: Point) -> Option<f64> {
        self.rate_queries.push(at);
        Some(self.rate)
    }
    fn spawn(&mut self, owner: usize, at: Point) -> u32 {
        let id = self.units.iter().map(|u| u.id).max().unwrap_or(0) + 1;
        let mut u = unit(id, owner, UnitRole::Truck, at.x);
        u.at = at;
        self.units.push(u);
        id
    }
    fn retire(&mut self, id: u32) {
        self.units.retain(|u| u.id != id);
    }
    fn apply(&mut self, id: u32, op: UnitOperation) {
        let u = self.units.iter_mut().find(|u| u.id == id).unwrap();
        match op {
            UnitOperation::Route {
                path, world_goal, ..
            } => {
                u.path = path;
                u.world_goal = Some(world_goal);
            }
            UnitOperation::ClearRoute => {
                u.path.clear();
                u.world_goal = None;
            }
            UnitOperation::ResumeClear => {
                u.path.clear();
                u.world_goal = None;
                u.orders.clear();
                u.orders_pending = false;
            }
            UnitOperation::RepairRoute(path) => {
                u.path = path;
                u.stuck = 0.0;
            }
            UnitOperation::NewTruck => {}
        }
    }
}
fn players() -> [Player; 3] {
    std::array::from_fn(|slot| Player {
        slot,
        team: if slot == 2 { 1 } else { 0 },
        spawn: at(1.0 + slot as f64 * 100.0),
        out: false,
        away: false,
    })
}
fn context(players: &[Player]) -> Context<'_> {
    Context {
        enabled: true,
        construction: false,
        tick: 1,
        winner: false,
        army_pop: 1.0,
        players,
        width: 1,
        chars: b".",
        aid: &[],
        points: &[],
    }
}
fn source(state: &ConvoyState, owner: usize) -> Store {
    state
        .stores
        .iter()
        .find(|s| s.source && s.owner == owner)
        .unwrap()
        .clone()
}
fn empty_reserves() -> Reserves {
    let mut r = equipped("rifle").get(1).unwrap().clone();
    r.provisions = 0.0;
    r.ammo = 0.0;
    r.cut_for = Some(10.0);
    r
}
#[test]
fn feature_log11_support_pays_local_stock_or_reserves_once() {
    let ps = players();
    let ctx = context(&ps);
    let mut world = ComponentWorld::new(vec![unit(1, 0, UnitRole::Other, 1.0)]);
    let mut state = ConvoyState::setup(true).unwrap();
    let mut r = equipped("rifle");
    assert!(
        state
            .support_provision(ctx, 1, 0, 5.0, &mut r, &mut world)
            .unwrap()
    );
    assert_eq!(r.get(1).unwrap().provisions, 115.0);
    state.sync_stores(ctx, &world).unwrap();
    assert!(
        state
            .support_provision(ctx, 1, 0, 5.0, &mut r, &mut world)
            .unwrap()
    );
    assert_eq!(r.get(1).unwrap().provisions, 115.0);
    world.handoff = false;
    assert!(
        state
            .support_provision(ctx, 1, 0, 5.0, &mut r, &mut world)
            .unwrap()
    );
    assert_eq!(r.get(1).unwrap().provisions, 110.0);
    assert!(
        !state
            .support_provision(ctx, 1, 0, 111.0, &mut r, &mut world)
            .unwrap()
    );
    assert_eq!(r.get(1).unwrap().provisions, 110.0);
    state.stores.clear();
    world.handoff = true;
    world.units.push(unit(2, 0, UnitRole::Halftrack, 1.0));
    state.sync_stores(ctx, &world).unwrap();
    state.stores.retain(|s| !s.source);
    let before = state.stores[0].total().provisions;
    assert!(
        state
            .support_provision(ctx, 1, 0, 5.0, &mut r, &mut world)
            .unwrap()
    );
    assert_eq!(state.stores[0].total().provisions, before - 5.0);
    assert_eq!(r.get(1).unwrap().provisions, 110.0);
}
#[test]
fn feature_log16_sources_follow_live_completed_hqs_and_spawn_activity() {
    let mut ps = players();
    ps[1].away = true;
    ps[2].out = true;
    let mut world = ComponentWorld::new(vec![]);
    let mut state = ConvoyState::setup(true).unwrap();
    state.sync_stores(context(&ps), &world).unwrap();
    assert_eq!(state.stores.len(), 1);
    world.units = vec![
        unit(8, 0, UnitRole::Hq, 20.0),
        unit(9, 1, UnitRole::Hq, 80.0),
    ];
    state.sync_stores(context(&ps), &world).unwrap();
    assert_eq!(state.stores.len(), 2);
    assert_eq!(source(&state, 0).at, at(20.0));
    world.units[0].built = 0.5;
    state.sync_stores(context(&ps), &world).unwrap();
    assert_eq!(state.stores.len(), 1);
    world.units[1].hp = 0.0;
    state.sync_stores(context(&ps), &world).unwrap();
    assert!(state.stores.is_empty());
    world.units[0].built = 1.0;
    state.sync_stores(context(&ps), &world).unwrap();
    assert_eq!(source(&state, 0).entity, Some(8));
    let s = state.stores.iter_mut().find(|s| s.source).unwrap();
    let need = Stock {
        ammo: 900.0,
        provisions: 900.0,
        fuel: 900.0,
    };
    for _ in 0..3 {
        assert_eq!(s.fill_source(0, need, need), need);
    }
}
#[test]
fn feature_log19_store_lifecycle_preserves_contested_stock_and_scales_capacity() {
    let ps = players();
    let mut ctx = context(&ps);
    ctx.army_pop = 0.5;
    ctx.chars = b"A";
    ctx.aid = &[(0, 0)];
    let points = [DepotPoint {
        at: at(50.0),
        owner: Some(0),
        contested: false,
        depot: true,
    }];
    ctx.points = &points;
    let mut world = ComponentWorld::new(vec![
        unit(8, 0, UnitRole::Cache, 30.0),
        unit(9, 0, UnitRole::Halftrack, 70.0),
    ]);
    let mut state = ConvoyState::setup(true).unwrap();
    state.sync_stores(ctx, &world).unwrap();
    let cache = state.store_mut(&StoreId::Cache(8)).unwrap();
    assert_eq!(
        cache.capacity,
        Stock {
            ammo: 48.0,
            provisions: 1920.0,
            fuel: 2880.0
        }
    );
    assert_eq!(cache.total(), Stock::default());
    let support = state.store_mut(&StoreId::Support(9)).unwrap();
    assert_eq!(
        support.capacity,
        Stock {
            ammo: 6.0,
            provisions: 240.0,
            fuel: 360.0
        }
    );
    assert_eq!(
        support.total(),
        Stock {
            ammo: 6.0,
            provisions: 240.0,
            fuel: 0.0
        }
    );
    assert_eq!(
        state.store_mut(&StoreId::Hospital(0)).unwrap().total().ammo,
        6.0
    );
    state.store_mut(&StoreId::Point(0)).unwrap().buckets.push((
        0,
        Stock {
            provisions: 17.0,
            ..Stock::default()
        },
    ));
    let mut contested_points = points;
    contested_points[0].contested = true;
    ctx.points = &contested_points;
    state.sync_stores(ctx, &world).unwrap();
    let point = state.store_mut(&StoreId::Point(0)).unwrap();
    assert!(!point.active);
    assert_eq!(point.total().provisions, 17.0);
    let mut captured_points = contested_points;
    captured_points[0].contested = false;
    captured_points[0].owner = Some(1);
    ctx.points = &captured_points;
    state.sync_stores(ctx, &world).unwrap();
    assert_eq!(
        state.store_mut(&StoreId::Point(0)).unwrap().total(),
        Stock::default()
    );
    world.units[1].still = 1.0;
    world.units[1].cargo_len = 1;
    state.sync_stores(ctx, &world).unwrap();
    assert!(state.store_mut(&StoreId::Support(9)).is_some());
    world.units[1].cargo_len = 0;
    state.sync_stores(ctx, &world).unwrap();
    assert!(state.store_mut(&StoreId::Support(9)).is_none());
    world.units[0].built = 0.5;
    state.sync_stores(ctx, &world).unwrap();
    assert!(state.store_mut(&StoreId::Cache(8)).is_none());
}
#[test]
fn feature_log28_territory_grace_refills_then_cuts_and_reconnects() {
    let mut ps = players();
    for p in &mut ps {
        p.away = true;
    }
    let ctx = context(&ps);
    let mut world = ComponentWorld::new(vec![unit(1, 0, UnitRole::Other, 50.0)]);
    world.rate = 1.0;
    let mut state = ConvoyState::setup(true).unwrap();
    let mut r = LogisticsState::default();
    r.insert(1, empty_reserves());
    state.step(ctx, &mut r, &mut world, 1.0).unwrap();
    assert_eq!(r.get(1).unwrap().provisions, 3.0);
    assert_eq!(r.get(1).unwrap().cut_for, Some(0.0));
    world.rate = 0.0;
    state.step(ctx, &mut r, &mut world, 9.0).unwrap();
    assert_eq!(r.get(1).unwrap().provisions, 30.0);
    state.step(ctx, &mut r, &mut world, 1.0).unwrap();
    assert_eq!(r.get(1).unwrap().provisions, 30.0);
    assert_eq!(r.get(1).unwrap().cut_for, Some(10.0));
    assert_eq!(state.events.iter().filter(|e| e.kind == "cut").count(), 1);
    world.rate = 0.5;
    state.step(ctx, &mut r, &mut world, 2.0).unwrap();
    assert_eq!(r.get(1).unwrap().provisions, 33.0);
    assert_eq!(r.get(1).unwrap().cut_for, Some(0.0));
    assert_eq!(
        state.events.iter().filter(|e| e.kind == "restored").count(),
        1
    );
    world.units[0].riding = Some(2);
    world.units.push(unit(2, 0, UnitRole::Other, 80.0));
    world.rate_queries.clear();
    state.step(ctx, &mut r, &mut world, 1000.0).unwrap();
    assert_eq!(world.rate_queries, vec![at(80.0)]);
    assert_eq!(r.get(1).unwrap().provisions, 120.0);
    assert_eq!(r.get(1).unwrap().ammo, r.get(1).unwrap().ammo_max);
}
#[test]
fn feature_log30_source_and_territory_refills_need_no_wallet() {
    let mut ps = players();
    for p in &mut ps {
        p.away = true;
    }
    let mut world = ComponentWorld::new(vec![unit(1, 0, UnitRole::Other, 50.0)]);
    world.rate = 1.0;
    let mut state = ConvoyState::setup(true).unwrap();
    let mut r = LogisticsState::default();
    r.insert(1, empty_reserves());
    state.step(context(&ps), &mut r, &mut world, 40.0).unwrap();
    assert_eq!(r.get(1).unwrap().provisions, 120.0);
    assert_eq!(r.get(1).unwrap().ammo, r.get(1).unwrap().ammo_max);
    // Both mode families use these service APIs without a wallet or currency provider.
    for construction in [false, true] {
        let ps = players();
        let mut ctx = context(&ps);
        ctx.construction = construction;
        let mut world = ComponentWorld::new(vec![unit(1, 0, UnitRole::Other, 50.0)]);
        if construction {
            world.units.push(unit(10, 0, UnitRole::Hq, 1.0));
        }
        let mut state = ConvoyState::setup(true).unwrap();
        let mut r = LogisticsState::default();
        r.insert(1, empty_reserves());
        state.step(ctx, &mut r, &mut world, 0.0).unwrap();
        let id = *state.trucks.keys().next().unwrap();
        ctx.tick = 2;
        state.step(ctx, &mut r, &mut world, 4.0).unwrap();
        assert_eq!(state.trucks[&id].cargo.provisions, 120.0);
        let truck = world.units.iter_mut().find(|u| u.id == id).unwrap();
        truck.at = at(50.0);
        truck.path.clear();
        truck.world_goal = None;
        ctx.tick = 3;
        state.step(ctx, &mut r, &mut world, 0.0).unwrap();
        ctx.tick = 4;
        state.step(ctx, &mut r, &mut world, 8.0).unwrap();
        assert_eq!(r.get(1).unwrap().provisions, 120.0);
        assert_eq!(r.get(1).unwrap().ammo, r.get(1).unwrap().ammo_max);
        assert_eq!(state.trucks[&id].cargo, Stock::default());
        let root = source(&state, 0);
        let store = state.store_mut(&root.id).unwrap();
        let mut returned = Stock {
            provisions: 7.0,
            ..Stock::default()
        };
        let limit = returned;
        store.deposit(0, None, &mut returned, limit);
        assert_eq!(returned, Stock::default());
        assert_eq!(store.total().provisions, 7.0);
        assert_eq!(store.fill_source(0, limit, limit), limit);
        assert_eq!(store.total(), Stock::default());
        assert_eq!(store.fill_source(0, limit, limit), limit);
        assert_eq!(store.total(), Stock::default());
    }
}
#[test]
fn feature_log33_grouped_demand_is_reserved_before_loading() {
    let ps = players();
    let mut world = ComponentWorld::new(vec![
        unit(1, 0, UnitRole::Other, 51.0),
        unit(2, 0, UnitRole::Other, 56.0),
    ]);
    let mut r = LogisticsState::default();
    r.insert(1, empty_reserves());
    r.insert(2, empty_reserves());
    let mut state = ConvoyState::setup(true).unwrap();
    state.step(context(&ps), &mut r, &mut world, 0.0).unwrap();
    assert_eq!(state.jobs[&0], 0);
    assert_eq!(state.trucks.len(), 1);
    let c = state.trucks.values().next().unwrap();
    assert_eq!(c.state, TruckState::Loading);
    assert_eq!(c.cargo, Stock::default());
    assert_eq!(c.target.as_ref().unwrap().members, vec![1, 2]);
    let mut ctx = context(&ps);
    ctx.tick = 41;
    state.step(ctx, &mut r, &mut world, 0.0).unwrap();
    assert_eq!(state.jobs[&0], 0);
    assert_eq!(state.trucks.len(), 1);
}
#[test]
fn feature_log35_route_refresh_budget_carries_unfinished_work() {
    let mut ps = players();
    for p in &mut ps {
        p.away = true;
    }
    let mut world = ComponentWorld::new(vec![]);
    let mut state = ConvoyState::setup(true).unwrap();
    let mut active = ps;
    active[0].away = false;
    state.sync_stores(context(&active), &world).unwrap();
    let root = source(&state, 0);
    for _ in 0..5 {
        let id = state.add_truck(0, &root, &mut world);
        let c = state.trucks.get_mut(&id).unwrap();
        c.state = TruckState::Returning;
    }
    state.next_schedule = 1000;
    state.refresh_queue = state.trucks.keys().copied().collect();
    let mut r = LogisticsState::default();
    state.step(context(&ps), &mut r, &mut world, 0.0).unwrap();
    assert_eq!(world.plans, 3);
    assert_eq!(state.refresh_queue.len(), 2);
    state.step(context(&ps), &mut r, &mut world, 0.0).unwrap();
    assert_eq!(world.plans, 5);
    assert!(state.refresh_queue.is_empty());
}
#[test]
fn feature_log38_safe_reuse_skips_planning_and_stuck_routes_repair() {
    let mut ps = players();
    ps[1].away = true;
    ps[2].away = true;
    let mut world = ComponentWorld::new(vec![]);
    let mut state = ConvoyState::setup(true).unwrap();
    state.sync_stores(context(&ps), &world).unwrap();
    let root = source(&state, 0);
    let id = state.add_truck(0, &root, &mut world);
    world.units[0].at = at(100.0);
    world.units[0].path = vec![at(60.0), at(1.0)];
    world.units[0].world_goal = Some(at(1.0));
    let c = state.trucks.get_mut(&id).unwrap();
    c.state = TruckState::Returning;
    c.destination = Some(at(1.0));
    c.route = world.units[0].path.clone();
    world.reuse = true;
    state.next_schedule = 1000;
    state.refresh_queue.push_back(id);
    let mut r = LogisticsState::default();
    state.step(context(&ps), &mut r, &mut world, 0.0).unwrap();
    assert_eq!(world.plans, 0);
    world.units[0].stuck = 0.6;
    state.refresh_queue.push_back(id);
    state.step(context(&ps), &mut r, &mut world, 0.0).unwrap();
    assert_eq!(world.plans, 1);
    assert_eq!(world.units[0].stuck, 0.0);
    assert_eq!(world.units[0].path, vec![at(60.0), at(1.0)]);
}
#[test]
fn feature_log39_hold_resume_and_invalid_manual_route_preserve_cargo() {
    let ps = players();
    let mut world = ComponentWorld::new(vec![unit(1, 0, UnitRole::Other, 50.0)]);
    let mut state = ConvoyState::setup(true).unwrap();
    state.sync_stores(context(&ps), &world).unwrap();
    let root = source(&state, 0);
    let id = state.add_truck(0, &root, &mut world);
    let cargo = Stock {
        provisions: 60.0,
        ..Stock::default()
    };
    state.trucks.get_mut(&id).unwrap().cargo = cargo;
    let r = equipped("rifle");
    assert!(
        state
            .supply(context(&ps), 0, &[id], 1, &r, &mut world)
            .unwrap()
    );
    let target = state.trucks[&id].target.clone();
    assert!(
        state
            .prepare_movement(context(&ps), 0, &[id], &MovementCommand::Stop, &mut world)
            .unwrap()
    );
    assert!(state.trucks[&id].hold);
    assert_eq!(state.trucks[&id].cargo, cargo);
    assert!(state.resume(context(&ps), 0, &[id], &mut world));
    assert_eq!(state.trucks[&id].state, TruckState::Delivering);
    assert_eq!(state.trucks[&id].target, target);
    assert_eq!(state.trucks[&id].cargo, cargo);
    let before = state.trucks[&id].clone();
    world.safe = false;
    assert!(
        !state
            .prepare_movement(
                context(&ps),
                0,
                &[id],
                &MovementCommand::Move {
                    at: Some(at(80.0)),
                    orders: vec![],
                    queue: true
                },
                &mut world
            )
            .unwrap()
    );
    assert_eq!(state.trucks[&id], before);
}
#[test]
fn feature_log40_relief_preserves_payer_and_recipient_permissions() {
    let mut s = Store {
        id: StoreId::Cache(1),
        owner: 1,
        at: at(1.0),
        source: false,
        entity: Some(1),
        capacity: Stock {
            ammo: 100.0,
            provisions: 100.0,
            fuel: 100.0,
        },
        buckets: vec![],
        donations: vec![],
        active: true,
        support: false,
        point: None,
    };
    for payer in [0, 2] {
        let mut cargo = Stock {
            provisions: 20.0,
            ..Stock::default()
        };
        let limit = cargo;
        s.deposit(payer, Some(1), &mut cargo, limit);
        assert_eq!(cargo, Stock::default());
    }
    assert_eq!(s.total().provisions, 40.0);
    assert_eq!(s.available(0, false).provisions, 0.0);
    assert_eq!(s.available(2, false).provisions, 0.0);
    assert_eq!(s.available(1, true).provisions, 40.0);
    assert_eq!(s.available(1, false).provisions, 0.0);
    s.consume(
        1,
        Stock {
            provisions: 25.0,
            ..Stock::default()
        },
        true,
    );
    assert_eq!(s.total().provisions, 15.0);
    assert_eq!(s.buckets[0].1.provisions, 0.0);
    assert_eq!(
        s.buckets
            .iter()
            .find(|(payer, _)| *payer == 2)
            .unwrap()
            .1
            .provisions,
        15.0
    );
    assert_eq!(s.available(1, true).provisions, 15.0);
}
#[test]
fn feature_log41_trucks_start_at_own_source_and_ally_service_excludes_enemy() {
    let ps = players();
    let mut world = ComponentWorld::new(vec![]);
    let mut state = ConvoyState::setup(true).unwrap();
    state.sync_stores(context(&ps), &world).unwrap();
    let root = source(&state, 1);
    state.next_schedule = 10000;
    state.jobs.insert(1, 1);
    let mut r = LogisticsState::default();
    state.step(context(&ps), &mut r, &mut world, 0.0).unwrap();
    let id = *state.trucks.keys().next().unwrap();
    assert_eq!(world.unit(id).unwrap().at, ps[1].spawn);
    assert_eq!(state.trucks[&id].origin, root.id);
    world
        .units
        .push(unit(99, 0, UnitRole::Other, ps[1].spawn.x));
    r = equipped("rifle");
    let value = r.remove(1).unwrap();
    r.insert(99, value);
    state.stores.retain(|s| s.owner == 1);
    assert!(
        state
            .support_provision(context(&ps), 99, 0, 5.0, &mut r, &mut world)
            .unwrap()
    );
    assert_eq!(r.get(99).unwrap().provisions, 120.0);
    state.stores[0].owner = 2;
    assert!(
        state
            .support_provision(context(&ps), 99, 0, 5.0, &mut r, &mut world)
            .unwrap()
    );
    assert_eq!(r.get(99).unwrap().provisions, 115.0);
    world.units.extend([
        unit(100, 0, UnitRole::Cache, 60.0),
        unit(101, 2, UnitRole::Cache, 60.0),
    ]);
    state.sync_stores(context(&ps), &world).unwrap();
    state.stores.retain(|s| !s.source || s.owner == 0);
    assert!(
        state
            .supply(context(&ps), 1, &[id], 99, &r, &mut world)
            .unwrap()
    );
    assert_eq!(state.trucks[&id].origin, source(&state, 0).id);
    state.trucks.get_mut(&id).unwrap().cargo.provisions = 20.0;
    assert!(
        state
            .supply(context(&ps), 1, &[id], 100, &r, &mut world)
            .unwrap()
    );
    assert_eq!(
        state.trucks[&id].target.as_ref().unwrap().relief,
        Some(true)
    );
    assert!(
        !state
            .supply(context(&ps), 1, &[id], 101, &r, &mut world)
            .unwrap()
    );
}
#[test]
fn feature_log42_death_discards_cargo_and_delays_replacement() {
    let ps = players();
    let mut world = ComponentWorld::new(vec![unit(8, 0, UnitRole::Cache, 30.0)]);
    let mut state = ConvoyState::setup(true).unwrap();
    state.sync_stores(context(&ps), &world).unwrap();
    let root = source(&state, 0);
    let id = state.add_truck(0, &root, &mut world);
    state.trucks.get_mut(&id).unwrap().cargo.provisions = 60.0;
    let truck = world.unit(id).unwrap();
    state.convoy_death(100, &truck);
    world.retire(id);
    assert!(!state.trucks.contains_key(&id));
    assert_eq!(state.losses[&0], vec![35.0]);
    state.retire_unit(100, 8, Some(0), false);
    assert!(state.store_mut(&StoreId::Cache(8)).is_none());
    assert_eq!(state.losses[&0], vec![35.0]);
}
#[test]
fn feature_log44_component_recipient_rows_are_owner_private() {
    let ps = players();
    let world = ComponentWorld::new(vec![
        unit(1, 0, UnitRole::Other, 50.0),
        unit(2, 2, UnitRole::Other, 80.0),
    ]);
    let mut state = ConvoyState::setup(true).unwrap();
    state.sync_stores(context(&ps), &world).unwrap();
    let mut r = equipped("rifle");
    r.insert(2, empty_reserves());
    let own = state.recipient_state(context(&ps), 0, &r, &world).unwrap();
    assert_eq!(own.units.iter().map(|u| u.id).collect::<Vec<_>>(), vec![1]);
    assert!(own.stores.iter().all(|s| s.owner != 2));
    let enemy = state.recipient_state(context(&ps), 2, &r, &world).unwrap();
    assert_eq!(
        enemy.units.iter().map(|u| u.id).collect::<Vec<_>>(),
        vec![2]
    );
    assert!(enemy.stores.iter().all(|s| s.owner == 2));
    assert!(state.recipient_state(context(&ps), 3, &r, &world).is_err());
}
#[test]
fn feature_log01_mode_and_scenario_component_defaults() {
    for (mode, enabled) in [
        (logistics::Mode::Conquest, true),
        (logistics::Mode::Classic, true),
        (logistics::Mode::World, true),
        (logistics::Mode::Annihilation, true),
        (logistics::Mode::Assault, false),
        (logistics::Mode::Horde, false),
        (logistics::Mode::Tutorial, false),
    ] {
        let c = logistics::Context {
            mode,
            scenario: false,
            enabled_override: None,
        };
        assert_eq!(logistics::enabled_for(c), enabled);
        assert!(!logistics::enabled_for(logistics::Context {
            scenario: true,
            ..c
        }));
        assert!(logistics::enabled_for(logistics::Context {
            scenario: true,
            enabled_override: Some(true),
            ..c
        }));
        assert!(!logistics::enabled_for(logistics::Context {
            enabled_override: Some(false),
            ..c
        }));
    }
}
#[test]
fn feature_log05_projectiles_cannot_spend_fractional_or_unfunded_rounds() {
    let mut r = equipped("rifle");
    r.get_mut(1).unwrap().ammo = 2.75;
    assert!(!r.can_fire(1, 3.0));
    assert!(r.can_fire(1, 2.0));
    assert_eq!(r.consume_ammo(1, 5.0), 2.0);
    assert_eq!(r.get(1).unwrap().ammo, 0.75);
    assert!(!r.can_fire(1, 1.0));
    assert_eq!(r.consume_ammo(1, 1.0), 0.0);
    assert_eq!(r.get(1).unwrap().ammo, 0.75);
}
#[test]
fn feature_log22_23_ground_network_bridge_damage_and_connected_depot() {
    use ww2_core::{
        game::MOVE,
        navigation::{TerrainView, Versions},
        objectives::{Objective, ObjectivePlayer},
        supply_network::{GroundSupplyNetwork, SupplyInput},
        unit_spawn::SpawnPlacer,
    };
    let players = [ObjectivePlayer {
        team: 0,
        out: false,
        spawn: at(1.0),
    }];
    let mut flags = vec![0; 32];
    let mut chars = vec![b'.'; 32];
    let initial = chars.clone();
    let mut points = vec![Objective {
        id: serde_json::json!(1),
        at: at(55.0),
        vp: 1.0,
        mp: 1.0,
        kind: Some("depot".into()),
        needs: None,
        owner: Some(0),
        capper: None,
        progress: 0.0,
        cut: false,
        on_point: vec![],
        contested: false,
    }];
    let compute = |flags: &[u16], chars: &[u8], points: &[Objective]| {
        GroundSupplyNetwork::compute(
            SupplyInput {
                view: TerrainView {
                    width: 32,
                    height: 1,
                    flags,
                    heights: None,
                    world_known: false,
                    knowledge_id: 0,
                    versions: Versions::default(),
                    changed: None,
                    changes: &[],
                },
                chars,
                initial_chars: &initial,
                initial_heights: None,
                points,
                units: &[],
                players: &players,
            },
            &mut SpawnPlacer::new(),
        )
        .unwrap()
    };
    let clean = compute(&flags, &chars, &[]);
    assert_eq!(clean.teams[0].steps[31], 31);
    chars[16] = b'+';
    let damaged = compute(&flags, &chars, &[]);
    assert_eq!(damaged.teams[0].steps[31], 36);
    flags[16] = MOVE;
    let broken = compute(&flags, &chars, &[]);
    assert_eq!(broken.rate_at(Some(0), at(61.0)).unwrap(), 0.0);
    let with_depot = compute(&flags, &chars, &points);
    assert_eq!(with_depot.rate_at(Some(0), at(61.0)).unwrap(), 1.0);
    points[0].cut = true;
    assert_eq!(
        compute(&flags, &chars, &points)
            .rate_at(Some(0), at(61.0))
            .unwrap(),
        0.0
    );
    points[0].cut = false;
    points[0].contested = true;
    assert_eq!(
        compute(&flags, &chars, &points)
            .rate_at(Some(0), at(61.0))
            .unwrap(),
        0.0
    );
    flags[16] = 0;
    chars[16] = b'.';
    assert_eq!(compute(&flags, &chars, &[]).teams[0].steps[31], 31);
    let long_flags = vec![0; 65540];
    let long_chars = vec![b'.'; long_flags.len()];
    let saturated = GroundSupplyNetwork::compute(
        SupplyInput {
            view: TerrainView {
                width: long_flags.len(),
                height: 1,
                flags: &long_flags,
                heights: None,
                world_known: false,
                knowledge_id: 0,
                versions: Versions::default(),
                changed: None,
                changes: &[],
            },
            chars: &long_chars,
            initial_chars: &long_chars,
            initial_heights: None,
            points: &[],
            units: &[],
            players: &players,
        },
        &mut SpawnPlacer::new(),
    )
    .unwrap();
    assert_eq!(saturated.teams[0].steps[65533], 65533);
    assert!(
        saturated.teams[0].steps[65534..]
            .iter()
            .all(|s| *s == 65534)
    );
}
#[test]
fn feature_log25_ground_rate_near_linear_floor_and_closed_cells() {
    use ww2_core::supply_network::{GroundSupplyNetwork, TeamNetwork};
    for (steps, expected) in [
        (75, 1.0),
        (150, 0.75),
        (300, 0.25),
        (500, 0.25),
        (65535, 0.0),
    ] {
        let mut net = GroundSupplyNetwork {
            width: 1,
            height: 1,
            teams: vec![TeamNetwork {
                team: 0,
                steps: vec![steps],
                closed: vec![0],
            }],
        };
        assert_eq!(net.rate_at(Some(0), at(1.0)).unwrap(), expected);
        net.teams[0].closed[0] = 1;
        assert_eq!(net.rate_at(Some(0), at(1.0)).unwrap(), 0.0);
    }
    let mut net = GroundSupplyNetwork {
        width: 9,
        height: 9,
        teams: vec![TeamNetwork {
            team: 0,
            steps: vec![65535; 81],
            closed: vec![0; 81],
        }],
    };
    let center = Point { x: 9.0, z: 9.0 };
    net.teams[0].steps[4 * 9 + 8] = 0;
    assert_eq!(net.rate_at(Some(0), center).unwrap(), 0.0);
    net.teams[0].steps[7 * 9 + 7] = 150;
    assert_eq!(net.rate_at(Some(0), center).unwrap(), 0.75);
    net.teams[0].steps[10] = 75;
    assert_eq!(net.rate_at(Some(0), center).unwrap(), 1.0);
}
#[test]
fn feature_log32_fleet_limits_spawning_retires_newest_and_obeys_loss_delay() {
    let mut ps = players();
    ps[1].away = true;
    ps[2].away = true;
    let mut ctx = context(&ps);
    let mut state = ConvoyState::setup(true).unwrap();
    let mut world = ComponentWorld::new(vec![]);
    state.sync_stores(ctx, &world).unwrap();
    state.next_schedule = 10000;
    state.jobs.insert(0, 8);
    let mut r = LogisticsState::default();
    for tick in [1, 21, 41, 61] {
        ctx.tick = tick;
        state.step(ctx, &mut r, &mut world, 0.0).unwrap();
    }
    assert_eq!(state.trucks.len(), 4);
    ctx.tick = 81;
    state.step(ctx, &mut r, &mut world, 0.0).unwrap();
    assert_eq!(state.trucks.len(), 4);
    let oldest = *state.trucks.keys().next().unwrap();
    let newest = *state.trucks.keys().next_back().unwrap();
    state.jobs.insert(0, 0);
    state.trucks.get_mut(&oldest).unwrap().cargo.provisions = 1.0;
    state.step(ctx, &mut r, &mut world, 0.0).unwrap();
    assert_eq!(state.trucks.len(), 1);
    assert!(state.trucks.contains_key(&oldest));
    assert!(!state.trucks.contains_key(&newest));
    let dead = world.unit(oldest).unwrap();
    state.convoy_death(ctx.tick, &dead);
    world.retire(oldest);
    state.jobs.insert(0, 1);
    ctx.tick = 680;
    state.step(ctx, &mut r, &mut world, 0.0).unwrap();
    assert!(state.trucks.is_empty());
    ctx.tick = 681;
    state.step(ctx, &mut r, &mut world, 0.0).unwrap();
    assert_eq!(state.trucks.len(), 1);
}
#[test]
fn feature_log34_manual_supply_selects_nearest_own_stock_excluding_grants() {
    let ps = players();
    let ctx = context(&ps);
    let mut world = ComponentWorld::new(vec![
        unit(8, 0, UnitRole::Cache, 30.0),
        unit(9, 1, UnitRole::Cache, 45.0),
        unit(1, 0, UnitRole::Other, 50.0),
    ]);
    let mut state = ConvoyState::setup(true).unwrap();
    state.sync_stores(ctx, &world).unwrap();
    state.store_mut(&StoreId::Cache(8)).unwrap().buckets.push((
        0,
        Stock {
            provisions: 40.0,
            ..Stock::default()
        },
    ));
    let donated = state.store_mut(&StoreId::Cache(9)).unwrap();
    donated.buckets.push((
        1,
        Stock {
            provisions: 50.0,
            ..Stock::default()
        },
    ));
    donated.donations.push((
        1,
        vec![(
            0,
            Stock {
                provisions: 50.0,
                ..Stock::default()
            },
        )],
    ));
    let root = source(&state, 0);
    let id = state.add_truck(0, &root, &mut world);
    let r = equipped("rifle");
    assert!(state.supply(ctx, 0, &[id], 1, &r, &mut world).unwrap());
    assert_eq!(state.trucks[&id].origin, StoreId::Cache(8));
    assert_eq!(state.trucks[&id].state, TruckState::Collecting);
}
#[test]
fn feature_log36_loading_waits_four_seconds_and_delivery_is_incremental() {
    let mut ps = players();
    ps[1].away = true;
    ps[2].away = true;
    let mut ctx = context(&ps);
    let mut world = ComponentWorld::new(vec![unit(1, 0, UnitRole::Other, 50.0)]);
    let mut state = ConvoyState::setup(true).unwrap();
    let mut r = LogisticsState::default();
    r.insert(1, empty_reserves());
    state.step(ctx, &mut r, &mut world, 0.0).unwrap();
    let id = *state.trucks.keys().next().unwrap();
    assert_eq!(state.trucks[&id].timer, 4.0);
    ctx.tick = 2;
    state.step(ctx, &mut r, &mut world, 3.0).unwrap();
    assert_eq!(state.trucks[&id].state, TruckState::Loading);
    assert_eq!(state.trucks[&id].cargo, Stock::default());
    ctx.tick = 3;
    state.step(ctx, &mut r, &mut world, 1.0).unwrap();
    assert_eq!(state.trucks[&id].state, TruckState::Delivering);
    assert_eq!(state.trucks[&id].cargo.provisions, 120.0);
    let u = world.units.iter_mut().find(|u| u.id == id).unwrap();
    u.at = at(50.0);
    u.path.clear();
    u.world_goal = None;
    ctx.tick = 4;
    state.step(ctx, &mut r, &mut world, 0.0).unwrap();
    assert_eq!(state.trucks[&id].state, TruckState::Unloading);
    ctx.tick = 5;
    state.step(ctx, &mut r, &mut world, 1.0).unwrap();
    assert_eq!(r.get(1).unwrap().provisions, 15.0);
    assert_eq!(state.trucks[&id].cargo.provisions, 105.0);
}
#[test]
fn feature_log43_grouped_alerts_cool_down_and_expire() {
    let mut ps = players();
    for p in &mut ps {
        p.away = true;
    }
    let mut world = ComponentWorld::new(vec![
        unit(1, 0, UnitRole::Other, 50.0),
        unit(2, 0, UnitRole::Other, 60.0),
    ]);
    let mut state = ConvoyState::setup(true).unwrap();
    let mut r = LogisticsState::default();
    r.insert(1, empty_reserves());
    r.insert(2, empty_reserves());
    let mut ctx = context(&ps);
    for id in [1, 2] {
        r.get_mut(id).unwrap().cut_for = Some(0.0);
    }
    state.step(ctx, &mut r, &mut world, 10.0).unwrap();
    assert_eq!(state.events.len(), 1);
    assert_eq!(
        (state.events[0].kind.as_str(), state.events[0].count),
        ("cut", 2)
    );
    world.rate = 1.0;
    ctx.tick = 2;
    state.step(ctx, &mut r, &mut world, 0.0).unwrap();
    assert_eq!(state.events.len(), 2);
    world.rate = 0.0;
    ctx.tick = 3;
    state.step(ctx, &mut r, &mut world, 10.0).unwrap();
    assert_eq!(state.events.len(), 2);
    world.rate = 1.0;
    ctx.tick = 802;
    state.step(ctx, &mut r, &mut world, 0.0).unwrap();
    assert_eq!(state.events.len(), 1);
    assert_eq!(state.events[0].kind, "restored");
}

#[test]
fn feature_log48_upkeep_ignores_structures_dead_foreign_and_free_trucks() {
    use ww2_core::{
        economy::{self, Mode},
        economy_state::{
            self as es, ArmySize, EconomyState, ModeContext, PlayerEconomy, WorldContext,
        },
    };
    for mode in [Mode::Classic, Mode::World] {
        let mut w = EconomyState {
            players: vec![
                PlayerEconomy {
                    team: 0,
                    ..Default::default()
                },
                PlayerEconomy {
                    team: 0,
                    ..Default::default()
                },
                PlayerEconomy {
                    team: 1,
                    ..Default::default()
                },
            ],
        };
        let mut c = es::Context {
            mode: ModeContext {
                kind: mode,
                ..Default::default()
            },
            army: ArmySize::Standard,
            units: &[],
            drops: &[],
            points: &[],
            nodes: &[],
            world: None,
        };
        c.world = Some(WorldContext { regions: &[] });
        let make = |id, owner, kind, hp| es::Unit {
            id,
            owner: Some(owner),
            kind,
            hp,
            built: 1.0,
            queue: &[],
        };
        let units = [
            make(1, 0, "rifle", 100.0),
            make(2, 0, "hq", 100.0),
            make(3, 0, "tank", 0.0),
            make(4, 2, "tank", 100.0),
            make(5, 0, "truck", 120.0),
        ];
        c.units = &units;
        w.advance_income(&c, 1.0, false).unwrap();
        let upkeep = economy::price_of(Mode::Default, "rifle").unwrap().mp * 0.0008;
        assert_eq!(w.players[0].upkeep, Some(upkeep));
        assert_eq!(w.players[0].inc, 3.0 - upkeep);
        c.army = ArmySize::Large;
        w.advance_income(&c, 1.0, false).unwrap();
        assert_eq!(w.players[0].upkeep, Some(upkeep));
        assert_eq!(w.players[0].inc, 9.0 - upkeep);
        let expensive: Vec<_> = (0..100).map(|id| make(id, 0, "tank", 100.0)).collect();
        c.units = &expensive;
        w.advance_income(&c, 1.0, false).unwrap();
        assert_eq!(w.players[0].inc, 0.5);
    }
}
#[test]
fn feature_log20_31_cache_and_truck_catalog_fields_and_reserve_exclusion() {
    use ww2_core::catalog::Catalog;
    let catalog = Catalog::compiled().unwrap();
    let truck = catalog.unit("truck").unwrap();
    let cache = catalog.unit("supplycache").unwrap();
    for (key, value) in [
        ("hpPer", 120.0),
        ("speed", 7.5),
        ("radius", 1.8),
        ("vision", 12.0),
        ("cost", 0.0),
        ("pop", 0.0),
    ] {
        assert_eq!(truck.get(key).and_then(|v| v.number()), Some(value));
    }
    for (key, value) in [
        ("cost", 60.0),
        ("buildTime", 12.0),
        ("hpPer", 400.0),
        ("size", 2.0),
        ("vision", 12.0),
    ] {
        assert_eq!(cache.get(key).and_then(|v| v.number()), Some(value));
    }
    assert_eq!(
        truck.get("w").unwrap().get("range").unwrap().number(),
        Some(0.0)
    );
    for faction in [
        ww2_core::economy::Faction::Usa,
        ww2_core::economy::Faction::Germany,
        ww2_core::economy::Faction::Ussr,
        ww2_core::economy::Faction::Uk,
    ] {
        assert!(!ww2_core::economy::can_build("truck", faction).unwrap());
    }
    let mut r = LogisticsState::default();
    assert!(
        r.initialize_unit(
            reserve_context(),
            facts(1, "truck"),
            Some(&Definition::from_value(truck))
        )
        .is_none()
    );
}
