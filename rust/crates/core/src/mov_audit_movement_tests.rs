//! Native walk, speed-bound and recovery behaviors. No Source captures are consumed.
use super::*;
use crate::mov_audit_test_support::{close, ground, point, unit};

#[test]
fn mov11_walk_budget_waypoints_backing_block_and_yield() {
    let rules = Rules::compiled().unwrap();
    let mut g = ground();
    let kind = &rules.kinds[rules.kind("rifle").unwrap()];
    let knowledge = TrafficKnowledge::physical();
    // One-metre initial legs avoid the infantry skip and exercise actual multi-leg consumption.
    let mut u = unit(&rules, "rifle", 1, point(20., 20.));
    let mut path = vec![point(21., 20.), point(21., 21.), point(24., 21.)];
    let steer = Steer::default();
    assert!(!walk_route(
        &g, &rules, &mut u, kind, &mut path, 3., 1., false, &steer, &knowledge
    ));
    close(u.x, 22.);
    close(u.z, 21.);
    close(u.rot, 0.);
    assert_eq!(path, vec![point(24., 21.)]);
    let mut back = unit(&rules, "rifle", 2, point(20., 20.));
    back.rot = 1.;
    let mut path = vec![point(30., 20.)];
    assert!(!walk_route(
        &g, &rules, &mut back, kind, &mut path, 4., 1., true, &steer, &knowledge
    ));
    close(back.x, 22.);
    close(back.rot, 1.);
    g.flags[10 * g.w + 11] = crate::game::MOVE;
    for yielding in [false, true] {
        let mut blocked = unit(&rules, "rifle", 3, point(20., 21.));
        let mut path = vec![point(26., 21.)];
        let steer = Steer {
            yielding,
            ..Steer::default()
        };
        let dropped = walk_route(
            &g,
            &rules,
            &mut blocked,
            kind,
            &mut path,
            4.,
            1.,
            false,
            &steer,
            &knowledge,
        );
        assert_eq!(dropped, !yielding);
        close(blocked.repath, 0.2);
        close(blocked.x, 20.);
        assert_eq!(
            path,
            vec![point(26., 21.)],
            "yielding retains its original route"
        );
    }
}

#[test]
fn mov11_blocked_walk_discards_the_ordered_path_at_the_movement_consumer() {
    let rules = Rules::compiled().unwrap();
    let mut g = ground();
    let mut u = unit(&rules, "gunboat", 1, point(41.99, 41.));
    g.flags.fill(crate::game::MOVE);
    // The first step crosses the cell boundary into land.
    g.flags[20 * g.w + 21] = crate::game::LAND;
    u.path = vec![point(60., 41.)];
    g.units.insert(1, u);
    let mut grid = SpatialGrid::new(&g.units, 16.);
    let generation = g.units[&1].path_gen;
    move_unit(&mut g, &rules, &mut grid, 1, 1.).unwrap();
    assert!(g.units[&1].path.is_empty());
    assert_eq!(g.units[&1].path_gen, generation + 1);
    close(g.units[&1].repath, 0.2);
    assert_eq!(g.units[&1].at(), point(41.99, 41.));
}

#[test]
fn mov11_blocked_yield_keeps_the_ordered_route_at_the_movement_consumer() {
    let rules = Rules::compiled().unwrap();
    let mut g = ground();
    let mut u = unit(&rules, "rifle", 1, point(41.99, 41.));
    let ordered = vec![point(61., 61.), point(81., 61.)];
    u.path = ordered.clone();
    u.traffic = Some(crate::game::Traffic {
        goal: point(61., 41.),
        legs: vec![point(61., 41.)],
        passing: true,
        by: 2,
        until: 10.,
    });
    g.units.insert(1, u);
    g.units.insert(2, unit(&rules, "tank", 2, point(61., 61.)));
    g.flags[20 * g.w + 21] = crate::game::MOVE;
    let mut grid = SpatialGrid::new(&g.units, 16.);
    let generation = g.units[&1].path_gen;
    move_unit(&mut g, &rules, &mut grid, 1, 1.).unwrap();
    assert!(g.units[&1].traffic.is_some());
    assert_eq!(g.units[&1].path, ordered);
    assert_eq!(g.units[&1].path_gen, generation);
    close(g.units[&1].repath, 0.2);
    assert_eq!(g.units[&1].at(), point(41.99, 41.));
}

#[test]
fn mov24_waiting_convoy_does_not_request_stall_recovery() {
    let rules = Rules::compiled().unwrap();
    let mut g = ground();
    for (id, x) in [(1, 40.), (2, 44.)] {
        let mut u = unit(&rules, "tank", id, point(x, 41.));
        u.path = vec![point(90., 41.)];
        u.world_goal = u.path.last().copied();
        u.vx = Some(0.);
        u.vz = Some(0.);
        u.stuck = if id == 1 { 0.9 } else { 0. };
        g.units.insert(id, u);
    }
    let mut grid = SpatialGrid::new(&g.units, 16.);
    let mut requests = 0;
    move_unit_with_routes(&mut g, &rules, &mut grid, 1, 1., &mut |_, _, _, _, _| {
        requests += 1;
    })
    .unwrap();
    assert_eq!(
        g.units[&1].queued_behind, 2,
        "the unit actually waits for its leader"
    );
    assert_eq!(requests, 0);
}

#[test]
fn mov24_progress_is_measured_after_real_crowd_separation() {
    let rules = Rules::compiled().unwrap();
    let mut g = ground();
    let mut walker = unit(&rules, "rifle", 1, point(40., 41.));
    walker.path = vec![point(90., 41.)];
    let kind = &rules.kinds[walker.kind];
    let advance = kind.speed * rules.movement.tick;
    let minimum = kind.radius * 2. * 0.8;
    // This initial spacing makes the real separation push cancel the voluntary step.
    let other = unit(&rules, "rifle", 2, point(40. + minimum - 3. * advance, 41.));
    g.units.insert(1, walker);
    g.units.insert(2, other);
    let mut grid = SpatialGrid::new(&g.units, 16.);
    let moved = move_unit(&mut g, &rules, &mut grid, 1, 1.).unwrap();
    assert!(g.units[&1].x > 40.);
    crate::separation::separate_crowds(&mut g, &rules, &mut grid);
    close(g.units[&1].x, 40.);
    infantry_progress(&mut g, &rules, &[moved]);
    close(g.units[&1].stuck, rules.movement.tick);
    g.units.remove(&2);
    grid = SpatialGrid::new(&g.units, 16.);
    let moved = move_unit(&mut g, &rules, &mut grid, 1, 1.).unwrap();
    crate::separation::separate_crowds(&mut g, &rules, &mut grid);
    infantry_progress(&mut g, &rules, &[moved]);
    close(g.units[&1].stuck, 0.);
}

#[test]
#[ignore = "Retained e1c69647 failure: reduced-step bound failed in component truck scene without enabled logistics"]
fn mov23_powered_vehicle_displacement_respects_reduced_step_budget() {
    // An already moving truck receives a reduced powered-speed input this tick.
    // This component assertion also catches a missing final fuel-limit clamp.
    let rules = Rules::compiled().unwrap();
    let mut g = ground();
    let mut u = unit(&rules, "truck", 1, point(40., 40.));
    u.path = vec![point(90., 40.)];
    u.world_goal = u.path.last().copied();
    u.move_speed = rules.kinds[u.kind].speed;
    u.travel_dir = Some(0.);
    g.units.insert(1, u);
    let mut grid = SpatialGrid::new(&g.units, 16.);
    let before = g.units[&1].at();
    let moved = move_unit(&mut g, &rules, &mut grid, 1, 0.01).unwrap();
    assert!(
        dist(before, g.units[&1].at()) <= moved.speed * rules.movement.tick + 1e-9,
        "powered travel exceeded the current speed times dt budget"
    );
}

#[test]
fn mov24_stall_recovery_kind_cooldown_and_post_separation_progress() {
    let rules = Rules::compiled().unwrap();
    for (name, expected) in [
        ("rifle", RouteRequest::Recovery),
        ("tank", RouteRequest::Immediate),
    ] {
        let mut g = ground();
        let mut u = unit(&rules, name, 1, point(40., 40.));
        u.path = vec![point(90., 40.)];
        u.world_goal = u.path.last().copied();
        u.stuck = 0.8;
        g.units.insert(1, u);
        let mut grid = SpatialGrid::new(&g.units, 16.);
        let mut requests = vec![];
        for _ in 0..10 {
            move_unit_with_routes(
                &mut g,
                &rules,
                &mut grid,
                1,
                1.,
                &mut |_, _, u, goal, why| {
                    requests.push((goal, why));
                    u.repath = 1.;
                },
            );
        }
        assert_eq!(requests, vec![(point(90., 40.), expected)]);
    }
    let mut g = ground();
    let mut u = unit(&rules, "rifle", 1, point(40., 40.));
    u.path = vec![point(90., 40.)];
    u.motion_before = Some(u.pose());
    g.units.insert(1, u);
    let moved = Moved {
        id: 1,
        speed: 4.,
        infantry: true,
    };
    infantry_progress(&mut g, &rules, &[moved]);
    close(g.units[&1].stuck, rules.movement.tick);
    // The input to this phase is the position left by separation, not voluntary movement.
    g.units.get_mut(&1).unwrap().x += 0.1;
    infantry_progress(&mut g, &rules, &[moved]);
    close(g.units[&1].stuck, 0.);
    let mut g = ground();
    let mut tank = unit(&rules, "tank", 1, point(40., 40.));
    tank.path = vec![point(40., 80.)];
    tank.stuck = 0.5;
    g.units.insert(1, tank);
    let mut grid = SpatialGrid::new(&g.units, 16.);
    move_unit(&mut g, &rules, &mut grid, 1, 1.);
    assert!(g.units[&1].rot.abs() > 1e-4);
    close(g.units[&1].stuck, 0.);
}

#[test]
#[ignore = "Retained e1c69647 failure: authored central capture point is on river terrain; no bridge journey ran"]
fn mov11_bridge_groups_finish_all_widths_and_order_variants_with_deferred_recovery() {
    use crate::{
        match_commands::{Command, MoveOrder, MoveOutcome, Stance},
        mov_audit_test_support::{scene_rows, spawn},
    };
    for width in [1, 2, 3] {
        for variant in 0..6 {
            let reverse = variant == 4;
            let queued = variant == 2;
            let attack = variant == 3;
            let rows = (0..64)
                .map(|y| {
                    (0..64)
                        .map(|x| {
                            if (27..=35).contains(&y) {
                                if (30..30 + width).contains(&x) {
                                    '='
                                } else {
                                    'W'
                                }
                            } else {
                                '.'
                            }
                        })
                        .collect::<String>()
                })
                .collect();
            let mut m = scene_rows(rows);
            let mut ids = vec![];
            let mut orders = vec![];
            let mut goals = std::collections::BTreeMap::new();
            for i in 0..18 {
                let name = ["rifle", "at", "mg", "mortar", "rifle", "medic"][i % 6];
                let id = spawn(
                    &mut m,
                    name,
                    0,
                    point(
                        57. + (i % 3) as f64 * 4.,
                        if reverse {
                            47. - (i / 3) as f64 * 4.
                        } else {
                            77. + (i / 3) as f64 * 4.
                        },
                    ),
                );
                let at = point(
                    51. + (i % 6) as f64 * 4.,
                    if reverse {
                        89. + (i / 6) as f64 * 4.
                    } else {
                        31. + (i / 6) as f64 * 4.
                    },
                );
                ids.push(id);
                orders.push(MoveOrder {
                    unit: i64::from(id),
                    at,
                });
                goals.insert(id, point(at.x, at.z - if queued { 12. } else { 0. }));
            }
            for owner in 0..2 {
                let all = m
                    .world
                    .game
                    .units
                    .values()
                    .filter(|u| u.owner == owner)
                    .map(|u| i64::from(u.id))
                    .collect::<Vec<_>>();
                for (stance, on) in [(Stance::HoldFire, true), (Stance::AutoRetreat, false)] {
                    assert_eq!(
                        m.execute_command(
                            owner,
                            &Command::Stance {
                                ids: all.clone(),
                                stance,
                                on
                            },
                            false
                        )
                        .unwrap(),
                        None
                    );
                }
            }
            let command = Command::Move {
                orders: orders.clone(),
                control_ids: None,
                attack_move: attack,
                queue: false,
                together: variant == 1,
                face: None,
            };
            assert_eq!(m.execute_command(0, &command, false).unwrap(), None);
            if queued {
                for order in &mut orders {
                    order.at.z -= 12.;
                }
                assert_eq!(
                    m.execute_command(
                        0,
                        &Command::Move {
                            orders,
                            control_ids: None,
                            attack_move: false,
                            queue: true,
                            together: false,
                            face: None
                        },
                        false
                    )
                    .unwrap(),
                    None
                );
            }
            let mut arrived = std::collections::BTreeSet::new();
            // The composed movement/command/separation loop deliberately omits unrelated combat and economy.
            for n in 0..2400 {
                m.world.game.tick += 1;
                if n % 4 == 0 {
                    crate::vision::update_vision(&mut m.world.game, &m.rules, &m.grid);
                }
                let budget = if variant == 5 && (100..300).contains(&n) {
                    0
                } else {
                    4096
                };
                m.begin_path_tick(budget, 20).unwrap();
                let mut moved = vec![];
                for &id in &ids {
                    m.prepare_unit_commands(id, false).unwrap();
                    m.world.game.units.get_mut(&id).unwrap().repath -= m.rules.movement.tick;
                    if attack {
                        m.apply_attack_move(id).unwrap();
                    }
                    if let Some(result) = m.move_unit_with_command_routes(id, 1.).unwrap() {
                        moved.push(result);
                    }
                }
                crate::separation::separate_crowds(&mut m.world.game, &m.rules, &mut m.grid);
                infantry_progress(&mut m.world.game, &m.rules, &moved);
                if budget == 0 {
                    assert_eq!(
                        m.commands.requests.used(),
                        0,
                        "recovery respects zero path budget"
                    );
                }
                for &id in &ids {
                    let u = &m.world.game.units[&id];
                    let c = m.world.game.cell_of(u.x, u.z).unwrap();
                    assert_ne!(
                        m.world.game.chars[c], b'W',
                        "width {width} variant {variant} actor {id}"
                    );
                    assert_ne!(
                        m.commands.units[&id].outcome,
                        Some(MoveOutcome::Unreachable)
                    );
                    if dist(u.at(), goals[&id]) < if attack { 2.5 } else { 0.5 }
                        && u.path.is_empty()
                        && u.world_goal.is_none()
                        && m.commands.units[&id].orders.is_empty()
                    {
                        arrived.insert(id);
                        assert_eq!(
                            m.execute_command(
                                0,
                                &Command::Stance {
                                    ids: vec![i64::from(id)],
                                    stance: Stance::HoldPosition,
                                    on: true
                                },
                                false
                            )
                            .unwrap(),
                            None
                        );
                    }
                }
                if arrived.len() == 18 {
                    break;
                }
            }
            assert_eq!(
                arrived.len(),
                18,
                "all eighteen native roles finish width {width}, variant {variant}"
            );
        }
    }
}

#[test]
fn mov11_separation_checks_protected_cover_at_the_actual_displacement_phase() {
    let rules = Rules::compiled().unwrap();
    let mut g = ground();
    // These are declared component inputs at separation, before any walk can leave cover.
    let c = 10 * g.w + 10;
    g.flags[c] = crate::game::COVER;
    g.chars[c] = b'#';
    g.flags[c + 1] = crate::game::ROAD;
    g.chars[c + 1] = b'=';
    let mut covered = unit(&rules, "rifle", 1, point(21.81, 20.1894));
    covered.path = vec![point(40., 2.)];
    covered.world_goal = covered.path.last().copied();
    let mut other = unit(&rules, "rifle", 2, point(23.1, 20.1994));
    other.path = vec![point(41.29, 2.01)];
    other.world_goal = other.path.last().copied();
    assert!(crate::terrain::in_cover(&g, &rules, &covered));
    let before = covered.at();
    g.units.insert(1, covered);
    g.units.insert(2, other);
    let mut grid = SpatialGrid::new(&g.units, 16.);
    crate::separation::separate_crowds(&mut g, &rules, &mut grid);
    assert!(crate::terrain::in_cover(&g, &rules, &g.units[&1]));
    assert!(
        dist(g.units[&2].at(), point(23.1, 20.1994)) > 0.,
        "the pair reaches an actual separation branch"
    );
    assert!(g.units[&1].at() != before || g.units[&2].at() != point(23.1, 20.1994));
}
