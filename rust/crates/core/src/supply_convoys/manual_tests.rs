use super::*;
use crate::logistics::Reserves;

fn point(x: f64) -> Point {
    Point { x, z: 40.0 }
}

fn context() -> Context<'static> {
    static PLAYERS: [Player; 3] = [
        Player {
            slot: 0,
            team: 1,
            spawn: Point { x: 0.0, z: 40.0 },
            out: false,
            away: false,
        },
        Player {
            slot: 1,
            team: 1,
            spawn: Point { x: 200.0, z: 40.0 },
            out: false,
            away: false,
        },
        Player {
            slot: 2,
            team: 2,
            spawn: Point { x: 400.0, z: 40.0 },
            out: false,
            away: false,
        },
    ];
    Context {
        enabled: true,
        construction: true,
        tick: 1,
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
        at: point(x),
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

#[derive(Default)]
struct ManualWorld {
    units: Vec<UnitView>,
    routes: Vec<(Point, Point, bool)>,
    operations: Vec<(UnitId, UnitOperation)>,
    blocked: Option<Point>,
}
impl World for ManualWorld {
    fn units(&self) -> Vec<UnitView> {
        self.units.clone()
    }
    fn unit(&self, id: UnitId) -> Option<UnitView> {
        self.units.iter().find(|u| u.id == id).cloned()
    }
    fn route(&mut self, u: &UnitView, at: Point, safe: bool) -> Route {
        self.routes.push((u.at, at, safe));
        let blocked = self.blocked == Some(at);
        Route {
            route_end: Some(at),
            path: if blocked { vec![] } else { vec![at] },
            failure: blocked.then_some(RouteFailure::Blocked),
        }
    }
    fn handoff(&mut self, _: &UnitView, _: Point, _: Option<UnitId>, _: f64) -> bool {
        true
    }
    fn spawn(&mut self, _: usize, _: Point) -> UnitId {
        panic!("manual order tests must not spawn")
    }
    fn retire(&mut self, _: UnitId) {
        panic!("manual order tests must not retire")
    }
    fn apply(&mut self, id: UnitId, op: UnitOperation) {
        let u = self.units.iter_mut().find(|u| u.id == id).unwrap();
        match &op {
            UnitOperation::Route {
                path, world_goal, ..
            } => {
                u.path = path.clone();
                u.world_goal = Some(*world_goal);
            }
            UnitOperation::ClearRoute | UnitOperation::ResumeClear => {
                u.path.clear();
                u.world_goal = None;
                if op == UnitOperation::ResumeClear {
                    u.orders.clear();
                    u.orders_pending = false;
                }
            }
            _ => panic!("unexpected manual order operation"),
        }
        self.operations.push((id, op));
    }
}

fn setup() -> (ConvoyState, LogisticsState, ManualWorld) {
    let mut state = ConvoyState::setup(true).unwrap();
    state.ensure(
        context(),
        StoreId::Source(10),
        0,
        point(0.0),
        true,
        None,
        32.0,
    );
    state.ensure(
        context(),
        StoreId::Source(11),
        0,
        point(200.0),
        true,
        None,
        32.0,
    );
    state.trucks.insert(
        1,
        Truck {
            state: TruckState::Idle,
            cargo: empty(),
            target: None,
            origin: StoreId::Source(10),
            timer: 0.0,
            manual: false,
            hold: false,
            route: vec![],
            destination: None,
            route_failure: None,
            retry: None,
        },
    );
    let mut reserves = LogisticsState::default();
    reserves.insert(
        2,
        Reserves {
            ammo: 0.0,
            ammo_max: 10.0,
            ammo_price: 0.1,
            ammo_credit: None,
            provisions: 0.0,
            fuel: None,
            emergency: None,
            warning: None,
            forced: false,
            stranded: false,
            exhausted: false,
            supply_rate: None,
            cut_for: None,
        },
    );
    let world = ManualWorld {
        units: vec![
            unit(1, 0, UnitRole::Truck, 40.0),
            unit(2, 1, UnitRole::Other, 170.0),
        ],
        ..ManualWorld::default()
    };
    (state, reserves, world)
}

#[test]
fn manual_supply_loaded_drives_direct_and_empty_collects_nearest_origin() {
    for loaded in [false, true] {
        let (mut state, reserves, mut world) = setup();
        let cargo = Stock {
            provisions: if loaded { 25.0 } else { 0.0 },
            ..empty()
        };
        state.trucks.get_mut(&1).unwrap().cargo = cargo;
        assert_eq!(
            state.supply(context(), 0, &[1], 2, &reserves, &mut world),
            Ok(true)
        );
        let truck = &state.trucks[&1];
        assert!(truck.manual);
        assert!(!truck.hold);
        assert_eq!(truck.cargo, cargo);
        assert_eq!(truck.target.as_ref().unwrap().id, TargetId::Unit(2));
        assert_eq!(truck.target.as_ref().unwrap().owner, 1);
        if loaded {
            assert_eq!(truck.state, TruckState::Delivering);
            assert_eq!(truck.destination, Some(point(170.0)));
        } else {
            assert_eq!(truck.state, TruckState::Collecting);
            assert_eq!(truck.origin, StoreId::Source(11));
            assert_eq!(truck.destination, Some(point(200.0)));
        }
        assert_eq!(world.routes.len(), 1);
        assert!(!world.routes[0].2);
    }
}

#[test]
fn manual_supply_completed_allied_cache_is_available_before_inventory_sync() {
    let (mut state, mut reserves, mut world) = setup();
    reserves.remove(2);
    world.units[1].role = UnitRole::Cache;
    assert!(state.store(&StoreId::Cache(2)).is_none());
    state.trucks.get_mut(&1).unwrap().cargo.provisions = 25.0;
    assert_eq!(
        state.supply(context(), 0, &[1], 2, &reserves, &mut world),
        Ok(true)
    );
    let target = state.trucks[&1].target.as_ref().unwrap();
    assert_eq!(target.id, TargetId::Store(StoreId::Cache(2)));
    assert_eq!(target.relief, Some(true));
    let store = state.store(&StoreId::Cache(2)).unwrap();
    assert_eq!(store.owner, 1);
    assert!(store.active);
    assert_eq!(store.at, point(170.0));
    assert_eq!(target.need, store.space());
}

#[test]
fn manual_supply_refuses_unfinished_enemy_unreserved_and_region_cache_targets() {
    for case in 0..4 {
        let (mut state, mut reserves, mut world) = setup();
        match case {
            0 => {
                world.units[1].role = UnitRole::Cache;
                world.units[1].built = 0.5;
            }
            1 => world.units[1].owner = Some(2),
            2 => {
                reserves.remove(2);
            }
            3 => {
                world.units[1].role = UnitRole::Cache;
                world.units[1].region_cache = true;
            }
            _ => unreachable!(),
        }
        let before = state.clone();
        assert_eq!(
            state.supply(context(), 0, &[1], 2, &reserves, &mut world),
            Ok(false)
        );
        assert_eq!(state, before);
        assert!(world.routes.is_empty());
        assert!(world.operations.is_empty());
    }
}

#[test]
fn manual_supply_refuses_no_stock_and_unreachable_routes_without_losing_job() {
    let (mut state, reserves, mut world) = setup();
    state.stores.clear();
    state.ensure(
        context(),
        StoreId::Cache(9),
        0,
        point(100.0),
        false,
        Some(9),
        32.0,
    );
    let before = state.trucks[&1].clone();
    assert_eq!(
        state.supply(context(), 0, &[1], 2, &reserves, &mut world),
        Ok(false)
    );
    assert_eq!(state.trucks[&1], before);
    assert!(world.routes.is_empty());
    state.ensure(
        context(),
        StoreId::Source(11),
        0,
        point(200.0),
        true,
        None,
        32.0,
    );
    world.blocked = Some(point(191.0));
    assert_eq!(
        state.supply(context(), 0, &[1], 2, &reserves, &mut world),
        Ok(false)
    );
    assert_eq!(state.trucks[&1].state, TruckState::Idle);
    assert!(state.trucks[&1].target.is_none());
    assert_eq!(state.trucks[&1].origin, before.origin);
    assert!(world.operations.is_empty());
    state.trucks.get_mut(&1).unwrap().cargo.provisions = 25.0;
    world.blocked = Some(point(161.0));
    let cargo = state.trucks[&1].cargo;
    assert_eq!(
        state.supply(context(), 0, &[1], 2, &reserves, &mut world),
        Ok(false)
    );
    assert_eq!(state.trucks[&1].cargo, cargo);
    assert_eq!(state.trucks[&1].state, TruckState::Idle);
    assert!(state.trucks[&1].target.is_none());
    assert_eq!(state.trucks[&1].route_failure, Some(RouteFailure::Blocked));
    assert!(world.operations.is_empty());
}

#[test]
fn manual_move_amove_and_retreat_preflight_routes_before_changing_convoys() {
    for command in [
        MovementCommand::Move {
            at: Some(point(300.0)),
            orders: vec![],
            queue: false,
        },
        MovementCommand::AttackMove {
            at: Some(point(300.0)),
            orders: vec![],
            queue: false,
        },
        MovementCommand::Retreat { queue: false },
    ] {
        let (mut state, _, mut world) = setup();
        let destination = if matches!(command, MovementCommand::Retreat { .. }) {
            point(0.0)
        } else {
            point(300.0)
        };
        world.blocked = Some(destination);
        let before = state.clone();
        assert_eq!(
            state.prepare_movement(context(), 0, &[1], &command, &mut world),
            Ok(false)
        );
        assert_eq!(state, before);
        assert_eq!(world.routes, vec![(point(40.0), destination, false)]);
        world.blocked = None;
        assert_eq!(
            state.prepare_movement(context(), 0, &[1], &command, &mut world),
            Ok(true)
        );
        let truck = &state.trucks[&1];
        assert_eq!(truck.state, TruckState::Manual);
        assert!(truck.manual);
        assert!(!truck.hold);
        assert!(world.operations.is_empty());
    }
}

#[test]
fn manual_queued_waypoints_preflight_from_each_leg_and_keep_manual_state() {
    let (mut state, _, mut world) = setup();
    world.units[0].world_goal = Some(point(70.0));
    world.units[0].orders = vec![point(90.0), point(110.0)];
    state.trucks.get_mut(&1).unwrap().cargo.provisions = 25.0;
    let command = MovementCommand::Move {
        at: None,
        orders: vec![(1, point(140.0))],
        queue: true,
    };
    world.blocked = Some(point(70.0));
    let before = state.clone();
    assert_eq!(
        state.prepare_movement(context(), 0, &[1], &command, &mut world),
        Ok(false)
    );
    assert_eq!(state, before);
    assert_eq!(world.routes, vec![(point(40.0), point(70.0), false)]);
    world.routes.clear();
    world.blocked = None;
    assert_eq!(
        state.prepare_movement(context(), 0, &[1], &command, &mut world),
        Ok(true)
    );
    assert_eq!(
        world.routes,
        vec![
            (point(40.0), point(70.0), false),
            (point(70.0), point(90.0), false),
            (point(90.0), point(110.0), false),
            (point(110.0), point(140.0), false),
        ]
    );
    state.next_schedule = u64::MAX;
    state
        .step(context(), &mut LogisticsState::default(), &mut world, 0.05)
        .unwrap();
    assert_eq!(state.trucks[&1].state, TruckState::Manual);
    assert!(state.trucks[&1].manual);
    assert_eq!(state.trucks[&1].cargo.provisions, 25.0);
    assert_eq!(world.units[0].world_goal, Some(point(70.0)));
    assert_eq!(world.units[0].orders, vec![point(90.0), point(110.0)]);
}

#[test]
fn manual_stop_holds_paid_cargo_and_original_target_then_resume_continues_delivery() {
    let (mut state, reserves, mut world) = setup();
    state.trucks.get_mut(&1).unwrap().cargo.provisions = 25.0;
    assert_eq!(
        state.supply(context(), 0, &[1], 2, &reserves, &mut world),
        Ok(true)
    );
    let target = state.trucks[&1].target.clone();
    assert_eq!(
        state.prepare_movement(context(), 0, &[1], &MovementCommand::Stop, &mut world),
        Ok(true)
    );
    state.next_schedule = u64::MAX;
    state
        .step(context(), &mut LogisticsState::default(), &mut world, 20.0)
        .unwrap();
    assert_eq!(state.trucks[&1].state, TruckState::Held);
    assert!(state.trucks[&1].hold);
    assert_eq!(state.trucks[&1].cargo.provisions, 25.0);
    assert_eq!(state.trucks[&1].target, target);
    world.units[0].orders.push(point(250.0));
    assert!(state.resume(context(), 0, &[1], &mut world));
    let truck = &state.trucks[&1];
    assert!(!truck.manual);
    assert!(!truck.hold);
    assert_eq!(truck.state, TruckState::Delivering);
    assert_eq!(truck.target, target);
    assert_eq!(truck.cargo.provisions, 25.0);
    assert_eq!(truck.destination, Some(point(170.0)));
    assert!(world.units[0].orders.is_empty());
    assert!(
        world
            .operations
            .iter()
            .any(|(_, op)| *op == UnitOperation::ResumeClear)
    );
    assert!(world.routes.last().unwrap().2);
}

#[test]
fn manual_malformed_numeric_orders_are_refused_before_any_route_or_state_mutation() {
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        for command in [
            MovementCommand::Move {
                at: Some(Point { x: bad, z: 0.0 }),
                orders: vec![],
                queue: false,
            },
            MovementCommand::AttackMove {
                at: Some(point(100.0)),
                orders: vec![(1, Point { x: 20.0, z: bad })],
                queue: true,
            },
        ] {
            let (mut state, _, mut world) = setup();
            let before = state.clone();
            assert_eq!(
                state.prepare_movement(context(), 0, &[1], &command, &mut world),
                Err(ConvoyError::InvalidContext)
            );
            assert_eq!(state, before);
            assert!(world.routes.is_empty());
            assert!(world.operations.is_empty());
        }
    }
}
