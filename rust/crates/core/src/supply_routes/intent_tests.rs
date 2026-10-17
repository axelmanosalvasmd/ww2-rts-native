//! Component intent checks with explicit remembered terrain and visible threats.
use super::*;
use crate::rules::Rules;

struct Ground {
    width: usize,
    height: usize,
    flags: Vec<u16>,
    near: Vec<u8>,
    hazards: Vec<bool>,
    version: u64,
}

impl Ground {
    fn open(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            flags: vec![0; width * height],
            near: vec![0; width * height],
            hazards: vec![false; width * height],
            version: 0,
        }
    }

    fn terrain(&self) -> PathTerrain<'_> {
        PathTerrain {
            navigation: TerrainView {
                width: self.width,
                height: self.height,
                flags: &self.flags,
                heights: None,
                world_known: false,
                knowledge_id: 17,
                versions: Versions {
                    knowledge: self.version,
                    vehicle: self.version,
                    obstruction: self.version,
                    ..Versions::default()
                },
                changed: None,
                changes: &[],
            },
            near_walls: &self.near,
            known_mines: &self.hazards,
            fires: &self.hazards,
            bunkers: &[],
            wear: None,
            wetness: 0.0,
            roads: false,
        }
    }
}

fn p(x: f64, z: f64) -> Point {
    Point { x, z }
}

#[test]
fn if26_handoff_cache_groups_quarters_without_saturating_large_keys() {
    let rules = Rules::compiled().unwrap();
    let rifle = &rules.kinds[rules.kind("rifle").unwrap()];
    let mut ground = Ground::open(64, 64);
    for y in 39..=41 {
        ground.flags[y * 64 + 40] = MOVE | VBLOCK;
    }
    let mut routing = SupplyRouting {
        handoff_key: Some((0, 0, 0)),
        ..SupplyRouting::default()
    };
    let request = Handoff {
        tick: 0,
        start: p(75.0, 81.0),
        store: p(87.0, 81.0),
        radius: 15.01,
    };
    // This stored answer deliberately disagrees with the reachable detour.
    routing.handoffs.insert([300, 324, 348, 324, 60], false);
    assert!(
        !routing
            .handoff(request, ground.terrain(), rifle, &[])
            .unwrap()
    );
    assert!(
        !routing
            .handoff(
                Handoff {
                    radius: 15.124,
                    ..request
                },
                ground.terrain(),
                rifle,
                &[]
            )
            .unwrap()
    );
    assert!(
        routing
            .handoff(
                Handoff {
                    radius: 15.125,
                    ..request
                },
                ground.terrain(),
                rifle,
                &[]
            )
            .unwrap()
    );
    routing
        .handoffs
        .insert([300, 324, 348, 324, i64::MAX], false);
    let count = routing.handoffs.len();
    for radius in [2_f64.powi(61), 2_f64.powi(62), f64::MAX] {
        assert!(
            routing
                .handoff(Handoff { radius, ..request }, ground.terrain(), rifle, &[])
                .unwrap()
        );
    }
    assert_eq!(
        routing.handoffs.len(),
        count,
        "large keys do not reuse or overwrite a saturated legacy key"
    );
}

#[test]
fn log37_short_hops_skip_search_and_danger_views_refresh_per_owner_epoch_or_knowledge() {
    let rules = Rules::compiled().unwrap();
    let truck = rules.kinds.iter().find(|k| k.name == "truck").unwrap();
    let mut ground = Ground::open(64, 32);
    let mut routing = SupplyRouting::default();
    let threat = Threat {
        at: p(80.0, 32.0),
        range: 10.0,
    };
    let mut request = RouteRequest {
        owner: 0,
        tick: 0,
        from: p(72.0, 32.0),
        to: p(80.0, 32.0),
        safe: true,
    };
    let hop = routing
        .route(request, ground.terrain(), truck, &[threat])
        .unwrap();
    assert_eq!(hop.path, vec![request.to]);
    assert_eq!(hop.route_end, None);
    assert!(
        routing.owners.is_empty(),
        "short hops do not build search state"
    );

    request.from = p(16.0, 32.0);
    assert_eq!(
        routing
            .route(request, ground.terrain(), truck, &[threat])
            .unwrap()
            .failure,
        Some(RouteFailure::Dangerous)
    );
    request.tick = 39;
    assert_eq!(
        routing
            .route(request, ground.terrain(), truck, &[])
            .unwrap()
            .failure,
        Some(RouteFailure::Dangerous),
        "a threat remains cached within the dispatch epoch"
    );
    request.owner = 1;
    assert_eq!(
        routing
            .route(request, ground.terrain(), truck, &[])
            .unwrap()
            .failure,
        None
    );
    request.owner = 0;
    request.tick = 40;
    assert_eq!(
        routing
            .route(request, ground.terrain(), truck, &[])
            .unwrap()
            .failure,
        None
    );
    ground.version = 1;
    assert_eq!(
        routing
            .route(request, ground.terrain(), truck, &[threat])
            .unwrap()
            .failure,
        Some(RouteFailure::Dangerous),
        "new recipient knowledge refreshes threats before the next epoch"
    );
    request.safe = false;
    assert_eq!(
        routing
            .route(request, ground.terrain(), truck, &[threat])
            .unwrap()
            .failure,
        None
    );
}

#[test]
fn log37_safe_routes_close_threat_cells_but_leave_four_metres_to_escape_and_report_disconnection() {
    let rules = Rules::compiled().unwrap();
    let truck = rules.kinds.iter().find(|k| k.name == "truck").unwrap();
    let mut ground = Ground::open(64, 32);
    let mut routing = SupplyRouting::default();
    let request = RouteRequest {
        owner: 0,
        tick: 0,
        from: p(17.0, 31.0),
        to: p(105.0, 31.0),
        safe: true,
    };
    let threat = Threat {
        at: p(20.0, 31.0),
        range: 1.0,
    };
    let route = routing
        .route(request, ground.terrain(), truck, &[threat])
        .unwrap();
    let view = &routing.owners[&0];
    let cell = |x: usize, z: usize| z * ground.width + x;
    assert_eq!(view.flags[cell(8, 15)], 0, "truck centre stays open");
    assert_eq!(
        view.flags[cell(9, 15)],
        0,
        "ground within four metres stays open"
    );
    assert_ne!(
        view.flags[cell(11, 15)] & (MOVE | VBLOCK),
        0,
        "danger beyond escape radius closes"
    );
    assert_eq!(route.failure, None, "a truck can leave a small danger disc");
    assert!(distance(*route.path.last().unwrap(), request.to) <= 3.0);

    for z in 0..ground.height {
        for x in 30..34 {
            ground.flags[cell(x, z)] = MOVE | VBLOCK;
        }
    }
    ground.version += 1;
    let route = routing
        .route(request, ground.terrain(), truck, &[])
        .unwrap();
    assert!(route.path.is_empty());
    assert_eq!(route.failure, Some(RouteFailure::Unreachable));
}

#[test]
fn log37_long_convoy_routes_join_bounded_fine_legs_to_the_destination() {
    let rules = Rules::compiled().unwrap();
    let truck = rules.kinds.iter().find(|k| k.name == "truck").unwrap();
    let ground = Ground::open(2048, 16);
    let from = p(9.0, 15.0);
    let to = p(4079.0, 15.0);
    let first = Pathfinding::new()
        .find_path(
            ground.terrain(),
            PathRules::compiled().unwrap(),
            truck,
            from,
            to,
            None,
        )
        .unwrap();
    assert!(
        first.limited,
        "the fixture must require more than one fine leg"
    );
    assert!(distance(*first.waypoints.last().unwrap(), to) > 3.0);
    let route = SupplyRouting::default()
        .route(
            RouteRequest {
                owner: 0,
                tick: 0,
                from,
                to,
                safe: true,
            },
            ground.terrain(),
            truck,
            &[],
        )
        .unwrap();
    assert_eq!(route.failure, None);
    assert!(route.path.len() > first.waypoints.len());
    assert!(distance(*route.path.last().unwrap(), to) <= 3.0);
}

#[test]
fn log38_continuation_checks_every_remaining_leg_for_known_obstacles_and_visible_weapon_radius() {
    let rules = Rules::compiled().unwrap();
    let truck = rules.kinds.iter().find(|k| k.name == "truck").unwrap();
    let mut ground = Ground::open(64, 32);
    let from = p(9.0, 15.0);
    let path = [p(41.0, 15.0), p(81.0, 15.0)];
    let continues = |g: &Ground, stuck, threats: &[Threat]| {
        SupplyRouting::continue_route(g.terrain().navigation, truck, from, &path, stuck, threats)
    };
    assert!(continues(&ground, 0.5, &[]));
    assert!(!continues(&ground, 0.501, &[]));
    assert!(!SupplyRouting::continue_route(
        ground.terrain().navigation,
        truck,
        from,
        &[],
        0.0,
        &[]
    ));
    let threat = Threat {
        at: p(61.0, 21.0),
        range: 3.0,
    };
    assert!(
        !continues(&ground, 0.0, &[threat]),
        "second leg touches range plus three metres"
    );
    assert!(continues(
        &ground,
        0.0,
        &[Threat {
            at: p(61.0, 21.01),
            ..threat
        }]
    ));
    ground.flags[7 * ground.width + 30] = MOVE | VBLOCK;
    assert!(
        !continues(&ground, 0.0, &[]),
        "a newly remembered obstacle blocks a later leg"
    );
}
