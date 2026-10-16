//! Native feature intent checks. Prototype references are in movement.md.
//! Authored setup is input, never imported Source output. No capture is required.
use crate::{
    catalog::Catalog,
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::{MOVE, PlaneState, Player, Point},
    grid::SpatialGrid,
    map::AuthoritativeMap,
    match_commands::{Command, MatchCommands, MoveOrder, MoveOutcome, Stance, UnitControl},
    match_runtime::{Match, MatchOptions, PlayerConfig},
    match_tick::TickPhase,
    math, movement, separation, traffic,
    unit_spawn::{self, SpawnPlacer},
    weather::WeatherChoice,
    world::World,
};
use serde_json::json;

fn p(x: f64, z: f64) -> Point {
    Point { x, z }
}
fn distance(a: Point, b: Point) -> f64 {
    math::hypot(a.x - b.x, a.z - b.z)
}
fn map(rows: Vec<String>) -> AuthoritativeMap {
    AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"Native MOV intent", "w":80,"h":80,"rows":rows,
            "spawns":[{"x":8,"y":40},{"x":72,"y":40}],
            "points":[{"x":40,"y":40}]
        }))
        .unwrap(),
    )
    .unwrap()
}
fn options() -> MatchOptions {
    MatchOptions {
        seed: 701,
        mode: Mode::Conquest,
        players: vec![
            PlayerConfig {
                name: "A".into(),
                team: 0,
                faction: Faction::Usa,
            },
            PlayerConfig {
                name: "B".into(),
                team: 1,
                faction: Faction::Germany,
            },
        ],
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
    }
}
fn ordinary() -> Match {
    Match::new(&map(vec![".".repeat(80); 80]), options()).unwrap()
}
fn scene() -> Match {
    let map = map(vec![".".repeat(80); 80]);
    let mut m = Match::new(&map, options()).unwrap();
    for id in m.world.game.units.keys().copied().collect::<Vec<_>>() {
        m.production.remove_building(id);
    }
    m.world = World::from_map(&map).unwrap();
    m.world.game.players = (0..2)
        .map(|slot| Player {
            slot,
            team: slot as i32,
            visible: vec![],
        })
        .collect();
    m.commands = MatchCommands::new(4096);
    m.combat = Default::default();
    m.firing = Default::default();
    m.abilities = Default::default();
    m.recovery = Default::default();
    m.aviation = Default::default();
    m.field_works = Default::default();
    m.recruit_metadata.clear();
    m.building_cells.clear();
    m.building_rallies.clear();
    m.next_unit_id = 1000;
    m.world.game.tick = 1;
    m.world.game.weather = None;
    m.world.game.wet = Some(0.0);
    m.world.game.reveal = true;
    for player in &mut m.start.economy.players {
        player.mp = 10000.0;
    }
    m.grid.rebuild(&m.world.game.units);
    m.initialize_recipient_routing().unwrap();
    m
}
fn add(m: &mut Match, name: &str, owner: usize, at: Point) -> u32 {
    if m.rules.kinds[m.rules.kind(name).unwrap()].air {
        return m.spawn_aircraft(owner, name).unwrap();
    }
    let id = m.next_unit_id;
    m.next_unit_id += 1;
    let kind = m.rules.kind(name).unwrap();
    let mut u =
        unit_spawn::initial_unit(&m.rules, Catalog::compiled().unwrap(), kind, id, owner, at)
            .unwrap();
    u.still = Some(0.0);
    m.commands
        .insert(id, UnitControl::new(&u, &m.rules).unwrap());
    m.combat.initialize(id).hit_by = Some(0);
    m.firing.initialize(id);
    m.abilities.initialize(id);
    m.recovery.initialize(id);
    m.world.game.units.insert(id, u);
    m.grid.rebuild(&m.world.game.units);
    id
}
fn command(m: &mut Match, c: Command) {
    assert_eq!(m.execute_command(0, &c, false).unwrap(), None);
}
fn move_to(m: &mut Match, ids: &[u32], at: Point, queue: bool, attack_move: bool) {
    command(
        m,
        Command::Move {
            orders: ids
                .iter()
                .map(|id| MoveOrder {
                    unit: i64::from(*id),
                    at,
                })
                .collect(),
            control_ids: None,
            attack_move,
            queue,
            together: false,
            face: None,
        },
    );
}
fn walk(m: &mut Match, id: u32) -> movement::Moved {
    m.begin_path_tick(4096, 20).unwrap();
    m.move_unit_with_command_routes(id, 1.0).unwrap().unwrap()
}
fn route(m: &mut Match, id: u32, at: Point) {
    let u = m.world.game.units.get_mut(&id).unwrap();
    u.path = vec![at];
    u.path_gen += 1;
}
fn steer(m: &mut Match, id: u32) -> traffic::Steer {
    let mut u = m.world.game.units.remove(&id).unwrap();
    let goal = u.path.first().copied();
    let out = traffic::traffic_step(&m.world.game, &m.rules, &m.grid, &mut u, goal, 0.05);
    m.world.game.units.insert(id, u);
    out
}
fn show_all(m: &mut Match) {
    let ids = m.world.game.units.keys().copied().collect::<Vec<_>>();
    for player in &mut m.world.game.players {
        player.visible = ids.clone();
    }
}

#[test]
fn mov_01_connected_tick_phase_order() {
    let mut m = ordinary();
    let r = m.advance_tick().unwrap();
    assert_eq!(r.tick, 1);
    let indices = [
        TickPhase::Clock,
        TickPhase::Paths,
        TickPhase::UnitOrders,
        TickPhase::Movement,
        TickPhase::Separation,
        TickPhase::Projectiles,
        TickPhase::Support,
        TickPhase::AntiAir,
        TickPhase::SupportTimers,
        TickPhase::LivingWeather,
        TickPhase::Death,
    ]
    .map(|phase| {
        r.phases
            .iter()
            .position(|p| *p == phase)
            .expect("required movement phase")
    });
    assert!(indices.windows(2).all(|pair| pair[0] < pair[1]));
}
#[test]
fn mov_02_dead_units_skip_their_live_turn() {
    let mut m = ordinary();
    let id = m
        .world
        .game
        .units
        .values()
        .find(|u| m.rules.kinds[u.kind].infantry)
        .unwrap()
        .id;
    let u = m.world.game.units.get_mut(&id).unwrap();
    u.hp = 0.0;
    u.path = vec![p(90.0, 90.0)];
    let before = u.at();
    let r = m.advance_tick().unwrap();
    assert_eq!(r.prior[&id].x, before.x);
    assert!(!m.world.game.units.contains_key(&id));
    assert!(r.retired.iter().any(|u| u.unit.id == id));
    assert!(r.unit_order.windows(2).all(|pair| pair[0] < pair[1]));
}
#[test]
fn mov_03_route_movement_updates_velocity_grid_and_stillness() {
    let mut m = scene();
    let id = add(&mut m, "rifle", 0, p(40.0, 40.0));
    move_to(&mut m, &[id], p(70.0, 40.0), false, false);
    let before = m.world.game.units[&id].at();
    walk(&mut m, id);
    let u = &m.world.game.units[&id];
    assert!(u.x > before.x);
    assert_eq!(u.still, Some(0.0));
    assert!((u.vx.unwrap() - (u.x - before.x) / 0.05).abs() < 1e-9);
    assert_eq!(
        m.grid
            .in_order(&m.world.game.units, (u.x, u.z), 0.1, |_, _| true),
        vec![id]
    );
}
#[test]
fn mov_04_speed_order_factors_and_pace() {
    for (supp, retreat, sprint, factor) in [
        (0.0, false, false, 1.0),
        (50.0, false, false, 0.6),
        (90.0, false, false, 0.3),
        (90.0, true, false, 1.5),
        (90.0, false, true, 1.6),
    ] {
        let mut m = scene();
        let id = add(
            &mut m,
            if sprint { "conscript" } else { "rifle" },
            0,
            p(40.0, 40.0),
        );
        route(&mut m, id, p(100.0, 40.0));
        let u = m.world.game.units.get_mut(&id).unwrap();
        u.supp = supp;
        u.retreating = retreat;
        u.sprint = if sprint { 1.0 } else { 0.0 };
        let base = m.rules.kinds[m.world.game.units[&id].kind].speed;
        let moved = movement::move_unit(
            &mut m.world.game,
            &m.rules,
            &mut m.grid,
            id,
            movement::supp_speed(supp),
        )
        .unwrap();
        assert!(
            (moved.speed - base * factor).abs() < 1e-9,
            "{supp}/{retreat}/{sprint}"
        );
    }
}
#[test]
fn mov_15_hulls_classify_and_reject_map_edges() {
    let mut m = scene();
    for name in ["tank", "train", "kaiju", "truck"] {
        assert!(m.rules.kinds[m.rules.kind(name).unwrap()].ground_vehicle());
    }
    for name in ["rifle", "hq", "fighter", "gunboat"] {
        assert!(!m.rules.kinds[m.rules.kind(name).unwrap()].ground_vehicle());
    }
    let id = add(&mut m, "tank", 0, p(40.0, 40.0));
    let u = &m.world.game.units[&id];
    let k = &m.rules.kinds[u.kind];
    assert!(crate::vehicle::swept_vehicle_clear(
        &m.world.game,
        u.pose(),
        u.pose(),
        k,
        k.block()
    ));
    let edge = crate::game::Pose {
        x: 0.0,
        z: 40.0,
        rot: 0.0,
    };
    assert!(!crate::vehicle::swept_vehicle_clear(
        &m.world.game,
        u.pose(),
        edge,
        k,
        k.block()
    ));
}
#[test]
fn mov_20_short_engaged_retreat_keeps_front_toward_enemy() {
    let mut m = scene();
    let id = add(&mut m, "tank", 0, p(60.0, 60.0));
    let foe = add(&mut m, "tank", 1, p(60.0, 90.0));
    m.world.game.units.get_mut(&id).unwrap().rot = std::f64::consts::FRAC_PI_2;
    m.firing.get_mut(id).unwrap().target_id = foe;
    show_all(&mut m);
    move_to(&mut m, &[id], p(60.0, 50.0), false, false);
    assert!(m.world.game.units[&id].reverse.is_some());
    let before = m.world.game.units[&id].pose();
    for _ in 0..10 {
        walk(&mut m, id);
    }
    let u = &m.world.game.units[&id];
    assert!(u.z < before.z);
    assert!(crate::vehicle::angle_delta(before.rot, u.rot).abs() < 0.1);
}
#[test]
fn mov_41_retry_cap_drops_goal_and_new_command_refreshes_allowance() {
    let mut m = scene();
    let id = add(&mut m, "rifle", 0, p(30.0, 30.0));
    for z in 0..80 {
        m.world.set_cell(z * 80 + 40, b'W').unwrap();
    }
    m.initialize_recipient_routing().unwrap();
    let goal = p(110.0, 30.0);
    move_to(&mut m, &[id], goal, false, false);
    for _ in 0..20 {
        m.world.game.tick += 1;
        m.begin_path_tick(4096, 20).unwrap();
        m.request_command_path(id, goal, crate::path_requests::RequestKind::World)
            .unwrap();
    }
    assert_eq!(
        m.commands.units[&id].outcome,
        Some(MoveOutcome::Unreachable)
    );
    assert!(m.world.game.units[&id].world_goal.is_none());
    move_to(&mut m, &[id], goal, false, false);
    assert_ne!(
        m.commands.units[&id].outcome,
        Some(MoveOutcome::Unreachable)
    );
    assert!(m.world.game.units[&id].world_goal.is_some());
}
#[test]
fn mov_42_completed_long_goal_publishes_terminal_result() {
    let mut m = scene();
    let id = add(&mut m, "rifle", 0, p(40.0, 40.0));
    command(
        &mut m,
        Command::Stance {
            ids: vec![id.into()],
            stance: Stance::HoldPosition,
            on: true,
        },
    );
    let goal = p(44.0, 40.0);
    move_to(&mut m, &[id], goal, false, false);
    for _ in 0..80 {
        m.world.game.tick += 1;
        walk(&mut m, id);
        m.prepare_unit_commands(id, false).unwrap();
    }
    assert!(distance(m.world.game.units[&id].at(), goal) < 0.25);
    assert_eq!(m.commands.units[&id].outcome, Some(MoveOutcome::Arrived));
    assert!(m.commands.units[&id].move_result.is_some());
}
#[test]
fn mov_44_nearest_free_stays_in_origins_region() {
    let mut m = scene();
    for z in 0..80 {
        m.world.set_cell(z * 80 + 40, b'W').unwrap();
    }
    let g = &m.world.game;
    let view = crate::match_runtime::terrain_view(g);
    let mut placer = SpawnPlacer::new();
    let free = placer
        .nearest_free(view, p(110.0, 30.0), MOVE, Some(p(30.0, 30.0)))
        .unwrap();
    assert!(free % 80 < 40);
    assert_eq!(g.flags[free] & MOVE, 0);
    let unclipped = placer
        .nearest_free(view, p(-20.0, 10.0), MOVE, None)
        .unwrap();
    assert_eq!(unclipped % 80, 0);
}
#[test]
fn mov_45_replacement_and_eight_waiting_moves() {
    let mut m = scene();
    let id = add(&mut m, "rifle", 0, p(40.0, 40.0));
    move_to(&mut m, &[id], p(80.0, 40.0), false, false);
    for n in 0..8 {
        move_to(&mut m, &[id], p(85.0 + n as f64, 40.0), true, false);
    }
    assert_eq!(m.commands.units[&id].orders.len(), 8);
    move_to(&mut m, &[id], p(50.0, 70.0), false, true);
    assert!(m.commands.units[&id].orders.is_empty());
    assert!(m.world.game.units[&id].amove.is_some());
    assert!(m.world.game.units[&id].world_goal.is_none());
    assert_eq!(m.world.game.units[&id].traffic_wait, 0.0);
}
#[test]
fn mov_46_group_endpoints_spread_and_capture_stays_inside() {
    for capture in [false, true] {
        let mut m = scene();
        let ids = (0..9)
            .map(|i| add(&mut m, "rifle", 0, p(30.0, 20.0 + i as f64 * 4.0)))
            .collect::<Vec<_>>();
        let goal = if capture {
            p(80.0, 80.0)
        } else {
            p(100.0, 50.0)
        };
        move_to(&mut m, &ids, goal, false, false);
        let ends = ids
            .iter()
            .map(|id| {
                *m.world.game.units[id]
                    .path
                    .last()
                    .expect("every squad route")
            })
            .collect::<Vec<_>>();
        assert!(
            ends.iter()
                .all(|end| distance(*end, goal) <= if capture { 8.0 } else { 16.0 })
        );
        for (i, a) in ends.iter().enumerate() {
            for b in &ends[i + 1..] {
                assert!(distance(*a, *b) >= 2.75);
            }
        }
    }
}
#[test]
fn mov_47_idle_spacing_requests_equal_or_better_cover() {
    let mut m = scene();
    let a = add(&mut m, "rifle", 0, p(60.0, 60.0));
    let b = add(&mut m, "rifle", 0, p(60.5, 60.0));
    m.commands.units.get_mut(&b).unwrap().react = 0.0;
    m.begin_path_tick(4096, 20).unwrap();
    m.react_unit(b).unwrap();
    assert!(m.world.game.units[&a].path.is_empty());
    let end = *m.world.game.units[&b]
        .path
        .last()
        .expect("younger idle squad seeks spacing");
    assert!(distance(end, m.world.game.units[&a].at()) >= 2.75);
    assert!(distance(end, p(60.5, 60.0)) <= 4.0);
}
#[test]
fn mov_48_retreat_uses_home_and_speed_bonus() {
    let mut m = scene();
    let id = add(&mut m, "rifle", 0, p(60.0, 60.0));
    let home = m.reaction_home(id).unwrap();
    command(
        &mut m,
        Command::Retreat {
            ids: vec![id.into()],
        },
    );
    assert!(m.world.game.units[&id].retreating);
    assert!(distance(m.world.game.units[&id].world_goal.unwrap(), home) < 0.01);
    let base = m.rules.kinds[m.world.game.units[&id].kind].speed;
    assert!((walk(&mut m, id).speed - base * 1.5).abs() < 1e-9);
}

#[test]
fn mov_21_hull_escape_is_bounded_and_cancels_on_stop() {
    let mut m = scene();
    let id = add(&mut m, "tank", 0, p(60.0, 60.0));
    route(&mut m, id, p(90.0, 60.0));
    let u = m.world.game.units.get_mut(&id).unwrap();
    u.stuck = 0.5;
    u.clearance = Some(crate::game::Clearance {
        goal: crate::game::Pose {
            x: 60.0,
            z: 58.0,
            rot: 0.0,
        },
        reverse: true,
        path: u.path_gen,
        until: 81.0,
    });
    let before = u.at();
    walk(&mut m, id);
    let u = &m.world.game.units[&id];
    assert!(distance(u.at(), before) <= 1.2 * 0.05 + 1e-9);
    command(
        &mut m,
        Command::Stop {
            ids: vec![id.into()],
        },
    );
    walk(&mut m, id);
    assert!(m.world.game.units[&id].clearance.is_none());
}
#[test]
fn mov_51_new_recruit_receives_players_rally() {
    let mut m = ordinary();
    m.start.economy.players[0].mp = 10000.0;
    let hq = m
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 0 && m.rules.kinds[u.kind].name == "hq")
        .unwrap()
        .id;
    let rally = p(60.0, 60.0);
    m.player_rallies[0] = Some(rally);
    let first = m.next_unit_id;
    command(
        &mut m,
        Command::Buy {
            kind: "rifle".into(),
            from: Some(hq),
        },
    );
    let id = *m
        .world
        .game
        .units
        .range(first..)
        .next()
        .expect("purchased rifle")
        .0;
    let u = &m.world.game.units[&id];
    assert!(u.drift);
    assert!(!u.path.is_empty());
    assert!(distance(*u.path.last().unwrap(), rally) < 4.0);
}
#[test]
fn mov_52_priority_ages_and_breaks_ties_deterministically() {
    let tr = &crate::rules::Rules::compiled().unwrap().movement.traffic;
    assert_eq!(
        [
            tr.horizon,
            tr.wait,
            tr.replan,
            tr.age,
            tr.max_yield,
            tr.ghost,
            tr.ghost_time
        ],
        [0.7, 0.35, 3.0, 2.0, 5.0, 3.0, 2.0]
    );
    let mut m = scene();
    let a = add(&mut m, "tank", 0, p(40.0, 40.0));
    let b = add(&mut m, "tank", 0, p(50.0, 40.0));
    let mut a = m.world.game.units[&a].clone();
    let mut b = m.world.game.units[&b].clone();
    assert!(traffic::traffic_wins(&m.rules, &a, &b));
    b.traffic_entered = true;
    assert!(traffic::traffic_wins(&m.rules, &b, &a));
    a.retreating = true;
    assert!(traffic::traffic_wins(&m.rules, &a, &b));
    a.retreating = false;
    a.path = vec![p(100.0, 40.0)];
    assert!(!traffic::traffic_wins(&m.rules, &a, &b));
    a.traffic_wait = 4.0;
    assert!(!traffic::traffic_wins(&m.rules, &a, &b));
    a.traffic_wait = 6.0;
    assert!(traffic::traffic_wins(&m.rules, &a, &b));
    a.traffic_wait = 100.0;
    b.retreating = true;
    assert!(traffic::traffic_wins(&m.rules, &a, &b));
}
#[test]
fn mov_53_protected_and_infantry_peers_do_not_steer() {
    for held in [false, true] {
        let mut m = scene();
        let a = add(&mut m, "rifle", 0, p(40.0, 40.0));
        let b = add(&mut m, "rifle", 0, p(41.0, 40.0));
        route(&mut m, a, p(80.0, 40.0));
        route(&mut m, b, p(10.0, 40.0));
        m.world.game.units.get_mut(&a).unwrap().build = held;
        let out = steer(&mut m, a);
        assert!(!out.blocked && !out.yielding);
        assert_eq!(out.goal, Some(p(80.0, 40.0)));
    }
}
#[test]
fn mov_54_head_on_conflict_blocks_and_receding_contact_does_not() {
    let mut m = scene();
    let a = add(&mut m, "tank", 0, p(60.0, 60.0));
    let b = add(&mut m, "tank", 0, p(60.0, 65.0));
    route(&mut m, a, p(60.0, 100.0));
    route(&mut m, b, p(60.0, 20.0));
    let out = steer(&mut m, b);
    assert!(out.blocked || out.yielding || out.waiting);
    route(&mut m, b, p(60.0, 100.0));
    m.world.game.units.get_mut(&b).unwrap().z = 80.0;
    m.grid.rebuild(&m.world.game.units);
    let out = steer(&mut m, a);
    assert!(!out.blocked);
}
#[test]
fn mov_55_same_direction_queues_without_priority_age() {
    let mut m = scene();
    let lead = add(&mut m, "tank", 0, p(60.0, 66.0));
    let rear = add(&mut m, "tank", 0, p(60.0, 60.0));
    for id in [lead, rear] {
        route(&mut m, id, p(60.0, 110.0));
    }
    let before = m.world.game.units[&rear].traffic_wait;
    let out = steer(&mut m, rear);
    assert!(out.waiting);
    assert_eq!(m.world.game.units[&rear].queued_behind, lead);
    assert_eq!(m.world.game.units[&rear].traffic_wait, before);
}
#[test]
fn mov_56_one_lane_oncoming_vehicles_hold_short() {
    let mut m = scene();
    for z in 30..50 {
        for x in 0..80 {
            if !(38..42).contains(&x) {
                m.world.set_cell(z * 80 + x, b'B').unwrap();
            }
        }
    }
    let a = add(&mut m, "tank", 0, p(80.0, 50.0));
    let b = add(&mut m, "tank", 0, p(80.0, 80.0));
    route(&mut m, a, p(80.0, 120.0));
    route(&mut m, b, p(80.0, 40.0));
    let out = steer(&mut m, a);
    assert!(out.waiting);
    assert!(m.world.game.units[&a].lane_scan.is_some());
}
#[test]
fn mov_57_yield_preserves_route_and_remembers_bounded_trail() {
    let mut m = scene();
    let a = add(&mut m, "tank", 0, p(60.0, 60.0));
    let b = add(&mut m, "tank", 0, p(60.0, 65.0));
    route(&mut m, a, p(60.0, 100.0));
    route(&mut m, b, p(60.0, 20.0));
    let retained = m.world.game.units[&b].path.clone();
    m.world.game.units.get_mut(&b).unwrap().traffic_wait = 0.4;
    let out = steer(&mut m, b);
    assert!(out.yielding);
    assert_ne!(out.goal, retained.first().copied());
    assert_eq!(m.world.game.units[&b].path, retained);
    traffic::remember_position(m.world.game.units.get_mut(&b).unwrap());
    assert!(
        m.world.game.units[&b].traffic_trail.is_empty(),
        "yielding does not record trail"
    );
    move_to(&mut m, &[b], p(140.0, 65.0), false, false);
    assert!(m.world.game.units[&b].traffic.is_none());
    for _ in 0..400 {
        walk(&mut m, b);
    }
    assert_eq!(m.world.game.units[&b].traffic_trail.len(), 20);
}
#[test]
fn mov_58_pass_stopped_unit_and_retain_destination() {
    let mut m = scene();
    let holder = add(&mut m, "tank", 0, p(60.0, 67.0));
    let mover = add(&mut m, "tank", 0, p(60.0, 60.0));
    m.world.game.units.get_mut(&holder).unwrap().hold_pos = true;
    route(&mut m, mover, p(60.0, 100.0));
    m.world.game.units.get_mut(&mover).unwrap().traffic_wait = 0.4;
    let mut passing = false;
    for _ in 0..40 {
        let out = steer(&mut m, mover);
        passing |= out.yielding
            && m.world.game.units[&mover]
                .traffic
                .as_ref()
                .is_some_and(|t| t.passing);
    }
    assert!(passing);
    assert_eq!(
        m.world.game.units[&mover].path.last(),
        Some(&p(60.0, 100.0))
    );
}
#[test]
fn mov_59_boxed_conflict_replans_after_wait() {
    let mut m = scene();
    let a = add(&mut m, "tank", 0, p(60.0, 60.0));
    let b = add(&mut m, "tank", 1, p(60.0, 65.0));
    route(&mut m, a, p(60.0, 100.0));
    show_all(&mut m);
    for z in 25..40 {
        for x in [27, 28, 31, 32] {
            m.world.set_cell(z * 80 + x, b'B').unwrap();
        }
    }
    m.world.game.units.get_mut(&a).unwrap().traffic_wait = 3.1;
    let out = steer(&mut m, a);
    assert!(out.blocked);
    assert!(out.replan);
    assert!(m.world.game.units[&b].hp > 0.0);
}
#[test]
fn mov_60_ghost_only_allows_allied_pairs() {
    let mut m = scene();
    let a = add(&mut m, "tank", 0, p(60.0, 60.0));
    let b = add(&mut m, "tank", 0, p(61.0, 60.0));
    let mut a = m.world.game.units[&a].clone();
    let b = &m.world.game.units[&b];
    traffic::start_ghost(&m.world.game, &mut a, 2.0, 0.05);
    assert_eq!(a.ghost, m.world.game.tick as f64 + 40.0);
    assert!(traffic::ghost_pair(&m.world.game, &a, b, true));
    assert!(!traffic::ghost_pair(&m.world.game, &a, b, false));
}
#[test]
fn mov_61_overlapping_vehicles_separate_and_stop() {
    let mut m = scene();
    let a = add(&mut m, "tank", 0, p(60.0, 60.0));
    let b = add(&mut m, "tank", 0, p(60.5, 60.0));
    for id in [a, b] {
        let u = m.world.game.units.get_mut(&id).unwrap();
        u.move_speed = 4.0;
        u.vx = Some(4.0);
    }
    separation::separate_crowds(&mut m.world.game, &m.rules, &mut m.grid);
    assert!(distance(m.world.game.units[&a].at(), m.world.game.units[&b].at()) > 0.5);
    for id in [a, b] {
        let u = &m.world.game.units[&id];
        assert_eq!(u.move_speed, 0.0);
        assert_eq!(u.vx, Some(0.0));
    }
}
#[test]
fn mov_63_bridge_peers_keep_forward_progress() {
    let mut m = scene();
    m.world.set_cell(30 * 80 + 30, b'=').unwrap();
    let a = add(&mut m, "rifle", 0, p(60.5, 60.5));
    let b = add(&mut m, "rifle", 0, p(61.0, 61.0));
    for id in [a, b] {
        route(&mut m, id, p(60.5, 100.0));
        let u = m.world.game.units.get_mut(&id).unwrap();
        u.vx = Some(0.0);
        u.vz = Some(2.0);
    }
    let before = [m.world.game.units[&a].z, m.world.game.units[&b].z];
    separation::separate_crowds(&mut m.world.game, &m.rules, &mut m.grid);
    assert!(m.world.game.units[&a].z >= before[0]);
    assert!(m.world.game.units[&b].z >= before[1]);
    assert!((m.world.game.units[&a].x - m.world.game.units[&b].x).abs() > 0.5);
}
#[test]
fn mov_64_held_workers_do_not_push_routed_friends() {
    for protected in 0..4 {
        let mut m = scene();
        let a = add(&mut m, "rifle", 0, p(60.0, 60.0));
        let b = add(&mut m, "rifle", 0, p(60.5, 60.0));
        let u = m.world.game.units.get_mut(&a).unwrap();
        match protected {
            0 => u.hold_pos = true,
            1 => u.build = true,
            2 => u.dig = true,
            _ => u.entrench = true,
        }
        route(&mut m, b, p(100.0, 60.0));
        let positions = [m.world.game.units[&a].at(), m.world.game.units[&b].at()];
        separation::separate_crowds(&mut m.world.game, &m.rules, &mut m.grid);
        assert_eq!(m.world.game.units[&a].at(), positions[0]);
        assert_eq!(m.world.game.units[&b].at(), positions[1]);
    }
}
#[test]
fn mov_67_placement_updates_live_grid_like_rebuild() {
    let mut m = scene();
    let id = add(&mut m, "rifle", 0, p(30.0, 30.0));
    for at in [p(33.0, 31.0), p(80.0, 80.0), p(15.0, 120.0)] {
        let u = m.world.game.units.get_mut(&id).unwrap();
        u.x = at.x;
        u.z = at.z;
        m.grid.update(id, u);
        let fresh = SpatialGrid::new(&m.world.game.units, 16.0);
        for center in [(30.0, 30.0), (80.0, 80.0), (15.0, 120.0)] {
            assert_eq!(
                m.grid
                    .in_order(&m.world.game.units, center, 10.0, |_, _| true),
                fresh.in_order(&m.world.game.units, center, 10.0, |_, _| true)
            );
        }
    }
}
#[test]
fn mov_68_spawn_uses_monotonic_id_scatter_and_sidecars() {
    let mut m = ordinary();
    m.start.economy.players[0].mp = 10000.0;
    let hq = m
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 0 && m.rules.kinds[u.kind].name == "hq")
        .unwrap()
        .id;
    let first = m.next_unit_id;
    for n in 0..2 {
        command(
            &mut m,
            Command::Buy {
                kind: "rifle".into(),
                from: Some(hq),
            },
        );
        let id = first + n;
        let u = &m.world.game.units[&id];
        assert_eq!(u.owner, 0);
        assert_eq!(u.id, id);
        assert_eq!(m.world.game.flags_at(u.x, u.z) & MOVE, 0);
        assert!(
            m.combat.get(id).is_some()
                && m.firing.get(id).is_some()
                && m.recovery.get(id).is_some()
        );
    }
    assert_eq!(m.next_unit_id, first + 2);
}

fn world_scene() -> Match {
    let regions = (0..64)
        .map(|i| {
            let x = i % 8 * 64;
            let y = i / 8 * 64;
            let mut r = json!({"id":i,"x":x+32,"y":y+32,"kind":"rural",
            "name":format!("R{i}"),"bounds":[x,y,x+64,y+64]});
            if i == 0 {
                r["home"] = json!(0);
            }
            if i == 63 {
                r["home"] = json!(1);
            }
            r
        })
        .collect::<Vec<_>>();
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({"name":"Native World MOV",
        "w":512,"h":512,"rows":vec![".".repeat(512);512],
        "spawns":[{"x":32,"y":32},{"x":480,"y":480}],"points":[],
        "world":{"size":"huge","total":64,"regionCells":64,"regions":regions}}))
        .unwrap(),
    )
    .unwrap();
    let mut o = options();
    o.mode = Mode::World;
    Match::new_world(&map, o, crate::random::MatchRandom::new(701)).unwrap()
}
#[test]
fn mov_08_world_march_requires_own_region_and_calm() {
    let mut m = world_scene();
    let id = m
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 0 && m.rules.kinds[u.kind].infantry)
        .unwrap()
        .id;
    m.world.game.tick = 300;
    m.firing.get_mut(id).unwrap().target_id = 0;
    m.combat.get_mut(id).unwrap().hit_at = -1e9;
    m.world.game.units.get_mut(&id).unwrap().shot_at = Some(-1e9);
    assert_eq!(
        m.world_march_multiplier(&m.world.game.units[&id]).unwrap(),
        1.5
    );
    m.combat.get_mut(id).unwrap().hit_at = 299.0;
    assert_eq!(
        m.world_march_multiplier(&m.world.game.units[&id]).unwrap(),
        1.0
    );
    m.combat.get_mut(id).unwrap().hit_at = -1e9;
    m.firing.get_mut(id).unwrap().target_id = id;
    assert_eq!(
        m.world_march_multiplier(&m.world.game.units[&id]).unwrap(),
        1.0
    );
}
#[test]
fn mov_50_displaced_world_guard_returns_and_drops_route() {
    let mut m = world_scene();
    let id = *m
        .commands
        .units
        .iter()
        .find(|(_, c)| c.guard_home.is_some())
        .expect("native local guard")
        .0;
    let home = m.commands.units[&id].guard_home.unwrap();
    let u = m.world.game.units.get_mut(&id).unwrap();
    u.x = home.x + 20.0;
    u.path = vec![p(home.x + 30.0, home.z)];
    m.apply_world_guard_home(id).unwrap();
    let u = &m.world.game.units[&id];
    assert_eq!(u.at(), home);
    assert!(u.path.is_empty());
    assert_eq!(m.commands.units[&id].attack_id, 0);
}
#[test]
fn mov_69_step_out_fallback_and_ghost_window() {
    let mut m = scene();
    let id = add(&mut m, "rifle", 0, p(60.0, 60.0));
    m.world.set_cell(30 * 80 + 30, b'B').unwrap();
    let at = m.world.game.units[&id].at();
    let cell = SpawnPlacer::new()
        .nearest_free(
            crate::match_runtime::terrain_view(&m.world.game),
            at,
            MOVE,
            Some(p(60.0, 58.0)),
        )
        .unwrap();
    assert_eq!(m.world.game.flags[cell] & MOVE, 0);
    let u = m.world.game.units.get_mut(&id).unwrap();
    // The component verifies placement and ghost policy. The ring-search caller is absent.
    let mut u = u.clone();
    traffic::start_ghost(&m.world.game, &mut u, 4.0, 0.05);
    assert_eq!(u.ghost, 81.0);
}
#[test]
#[ignore = "MOV-70: native queued house-entry actuator is unavailable"]
fn mov_70_house_entry_precedes_queued_exit() {
    let mut m = scene();
    let id = add(&mut m, "rifle", 0, p(60.0, 60.0));
    m.world.set_cell(30 * 80 + 32, b'H').unwrap();
    let enter = Command::from_json(&json!({"t":"enter","ids":[id],"cell":30*80+32})).unwrap();
    command(&mut m, enter);
    move_to(&mut m, &[id], p(80.0, 60.0), true, false);
    for _ in 0..100 {
        m.prepare_unit_commands(id, false).unwrap();
        walk(&mut m, id);
    }
    assert!(m.world.game.units[&id].garrison.is_some());
}
#[test]
fn mov_71_real_movement_wears_ground_and_raises_dust() {
    let mut m = scene();
    let id = add(&mut m, "tank", 0, p(60.0, 60.0));
    route(&mut m, id, p(100.0, 60.0));
    let before = m.world.game.units[&id].pose();
    walk(&mut m, id);
    m.apply_moved_ground_contact(id, before, None).unwrap();
    let u = &m.world.game.units[&id];
    let cell = m.world.game.cell_of(u.x, u.z).unwrap();
    assert!(m.world.game.wear.as_ref().unwrap()[cell] > 0.0);
    assert_eq!(u.dust, Some(m.world.game.tick as f64));
}
#[test]
fn mov_72_all_carrier_seat_values_match_pinned_table() {
    let catalog = Catalog::compiled().unwrap();
    for (kind, seats) in [
        ("halftrack", 1.0),
        ("lorry", 3.0),
        ("tank", 2.0),
        ("medium", 2.0),
        ("tankdestroyer", 2.0),
        ("tiger", 3.0),
        ("churchill", 3.0),
        ("lcvp", 1.0),
        ("train", 8.0),
    ] {
        assert_eq!(
            catalog.unit(kind).unwrap().get("carries").unwrap().number(),
            Some(seats),
            "{kind}"
        );
    }
}
fn mounted() -> (Match, u32, u32) {
    let mut m = scene();
    let tank = add(&mut m, "tank", 0, p(60.0, 60.0));
    let rider = add(&mut m, "rifle", 0, p(62.0, 60.0));
    command(
        &mut m,
        Command::Board {
            ids: vec![rider.into()],
            target: Some(tank.into()),
        },
    );
    assert_eq!(
        m.apply_unit_boarding(rider).unwrap(),
        crate::match_carriers::BoardingOutcome::Boarded { carrier: tank }
    );
    (m, tank, rider)
}
#[test]
fn mov_73_board_reserves_seats_for_nearest_squads() {
    let mut m = scene();
    let tank = add(&mut m, "tank", 0, p(60.0, 60.0));
    let a = add(&mut m, "rifle", 0, p(61.0, 60.0));
    let b = add(&mut m, "rifle", 0, p(62.0, 60.0));
    let c = add(&mut m, "rifle", 0, p(63.0, 60.0));
    command(
        &mut m,
        Command::Board {
            ids: vec![c.into(), b.into(), a.into()],
            target: Some(tank.into()),
        },
    );
    assert_eq!(m.world.game.units[&a].board, tank);
    assert_eq!(m.world.game.units[&b].board, tank);
    assert_eq!(m.world.game.units[&c].board, 0);
    for id in [a, b] {
        m.apply_unit_boarding(id).unwrap();
    }
    assert_eq!(m.combat.get(tank).unwrap().cargo.len(), 2);
}
#[test]
fn mov_74_riders_follow_carrier_and_ignore_move_orders() {
    let (mut m, tank, rider) = mounted();
    let u = m.world.game.units.get_mut(&tank).unwrap();
    u.x = 80.0;
    u.z = 70.0;
    u.rot = 1.0;
    m.world.game.units.get_mut(&rider).unwrap().supp = 99.0;
    m.firing.get_mut(rider).unwrap().target_id = tank;
    assert_eq!(
        m.apply_unit_riding(rider, 0.05).unwrap(),
        crate::match_carriers::RidingOutcome::Riding
    );
    assert_eq!(
        m.world.game.units[&rider].pose(),
        m.world.game.units[&tank].pose()
    );
    assert_eq!(m.world.game.units[&rider].supp, 0.0);
    assert_eq!(m.firing.get(rider).unwrap().target_id, 0);
    assert_eq!(
        m.execute_command(
            0,
            &Command::Move {
                orders: vec![MoveOrder {
                    unit: rider.into(),
                    at: p(100.0, 100.0)
                }],
                control_ids: None,
                attack_move: false,
                queue: false,
                together: false,
                face: None
            },
            false
        )
        .unwrap(),
        Some(crate::match_commands::Denial::Blocked)
    );
    assert!(m.world.game.units[&rider].path.is_empty());
}
#[test]
fn mov_75_unload_places_all_ground_riders_on_free_cells() {
    let (mut m, tank, rider) = mounted();
    command(
        &mut m,
        Command::Unload {
            ids: vec![tank.into()],
        },
    );
    assert!(!m.world.game.units[&rider].riding);
    assert!(m.combat.get(tank).unwrap().cargo.is_empty());
    assert_eq!(m.combat.get(rider).unwrap().carrier, None);
    let u = &m.world.game.units[&rider];
    assert_eq!(m.world.game.flags_at(u.x, u.z) & MOVE, 0);
}
#[test]
#[ignore = "MOV-76: wrecked-carrier ejection and drowning actuator is unavailable"]
fn mov_76_wrecked_carrier_ejects_and_hurts_rider() {
    let (mut m, tank, rider) = mounted();
    let hp = m.world.game.units[&rider].hp;
    m.world.game.units.get_mut(&tank).unwrap().hp = 0.0;
    assert_eq!(
        m.apply_unit_riding(rider, 0.05).unwrap(),
        crate::match_carriers::RidingOutcome::OnFoot
    );
    assert!(!m.world.game.units[&rider].riding);
    assert!(m.world.game.units[&rider].hp < hp);
}
#[test]
#[ignore = "MOV-77: combat-triggered tank-rider shock/ejection is unavailable"]
fn mov_77_enemy_hit_ejects_tank_riders_with_shock() {
    let (mut m, tank, rider) = mounted();
    let foe = add(&mut m, "rifle", 1, p(80.0, 60.0));
    show_all(&mut m);
    m.world.game.units.get_mut(&tank).unwrap().hp -= 1.0;
    m.combat.get_mut(tank).unwrap().hit_by = Some(foe);
    m.apply_unit_riding(rider, 0.05).unwrap();
    assert!(!m.world.game.units[&rider].riding);
    assert!(m.world.game.units[&rider].supp >= 60.0);
}
#[test]
fn mov_78_stationary_halftrack_reinforces_and_charges() {
    let mut m = scene();
    let carrier = add(&mut m, "halftrack", 0, p(60.0, 60.0));
    let id = add(&mut m, "rifle", 0, p(65.0, 60.0));
    m.world.game.units.get_mut(&carrier).unwrap().still = Some(2.1);
    m.world.game.units.get_mut(&id).unwrap().hp -= 80.0;
    let hp = m.world.game.units[&id].hp;
    let mp = m.start.economy.players[0].mp;
    let frame = m.prepare_recovery(&[carrier, id]).unwrap();
    for _ in 0..120 {
        m.reinforce(&frame, 0.05).unwrap();
    }
    assert!(m.world.game.units[&id].hp > hp);
    assert!(m.start.economy.players[0].mp < mp);
}
#[test]
#[ignore = "MOV-81: World rail command and operational train boarding are unavailable"]
fn mov_81_rail_order_boards_units_from_station() {
    let mut m = world_scene();
    let id = m
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 0 && m.rules.kinds[u.kind].infantry)
        .unwrap()
        .id;
    let c = Command::from_json(&json!({"t":"rail","ids":[id],"x":900,"z":900})).unwrap();
    command(&mut m, c);
    assert!(
        m.world
            .game
            .units
            .values()
            .any(|u| m.rules.kinds[u.kind].rail)
    );
}
#[test]
#[ignore = "MOV-82: train loading, track travel, interruption and unloading are unavailable"]
fn mov_82_train_moves_along_track_and_leaves_after_unload() {
    let mut m = scene();
    let id = add(&mut m, "train", 0, p(60.0, 60.0));
    route(&mut m, id, p(60.0, 100.0));
    walk(&mut m, id);
    assert!(m.world.game.units[&id].z > 60.0);
}
fn plane(m: &mut Match, owner: usize) -> u32 {
    let id = m.spawn_aircraft(owner, "fighter").unwrap();
    m.commands.units.get_mut(&id).unwrap().hold_fire = true;
    id
}
fn patrol(m: &mut Match, id: u32, at: Point) {
    assert!(
        m.send_aircraft_mission(
            0,
            id,
            crate::match_aviation::PlaneMission {
                kind: crate::match_aviation::MissionKind::Patrol,
                target: None,
                at
            }
        )
        .unwrap()
    );
}
#[test]
fn mov_84_plane_starts_at_offmap_or_nearest_finished_airfield() {
    let mut m = scene();
    let base = m.aircraft_base(0, p(60.0, 60.0)).unwrap();
    assert!((distance(base, m.start.players[0].spawn) - 40.0).abs() < 1e-9);
    let id = plane(&mut m, 0);
    assert_eq!(m.world.game.units[&id].at(), base);
    let field = add(&mut m, "airfield", 0, p(60.0, 60.0));
    m.world.game.units.get_mut(&field).unwrap().built = Some(1.0);
    assert_eq!(m.aircraft_base(0, p(65.0, 60.0)).unwrap(), p(60.0, 60.0));
    m.world.game.units.get_mut(&field).unwrap().built = Some(0.5);
    assert_eq!(m.aircraft_base(0, p(65.0, 60.0)).unwrap(), base);
}
#[test]
fn mov_85_sortie_station_return_and_rearm() {
    let mut m = scene();
    let id = plane(&mut m, 0);
    patrol(&mut m, id, p(80.0, 80.0));
    for _ in 0..300 {
        m.step_aircraft(id, 0.05).unwrap();
    }
    assert_eq!(m.world.game.units[&id].air, Some(PlaneState::Station));
    let fuel = m.aviation.planes[&id].fuel;
    m.step_aircraft(id, 0.05).unwrap();
    assert!(m.aviation.planes[&id].fuel < fuel);
    m.aviation.planes.get_mut(&id).unwrap().fuel = 0.0;
    for _ in 0..400 {
        m.step_aircraft(id, 0.05).unwrap();
        if m.world.game.units[&id].air == Some(PlaneState::Rearm) {
            break;
        }
    }
    assert_eq!(m.world.game.units[&id].air, Some(PlaneState::Rearm));
    for _ in 0..601 {
        m.step_aircraft(id, 0.05).unwrap();
    }
    assert_eq!(m.world.game.units[&id].air, Some(PlaneState::Base));
    assert_eq!(m.aviation.planes[&id].fuel, 50.0);
    assert_eq!(m.aviation.planes[&id].ammo, 60.0);
}
#[test]
fn mov_86_plane_move_stop_retreat_and_rearm_refusal() {
    let mut m = scene();
    let id = plane(&mut m, 0);
    move_to(&mut m, &[id], p(80.0, 80.0), false, false);
    assert_eq!(
        m.aviation.planes[&id].mission.unwrap().kind,
        crate::match_aviation::MissionKind::Patrol
    );
    command(
        &mut m,
        Command::Stop {
            ids: vec![id.into()],
        },
    );
    assert_eq!(
        m.aviation.planes[&id].mission.unwrap().at,
        m.world.game.units[&id].at()
    );
    command(
        &mut m,
        Command::Retreat {
            ids: vec![id.into()],
        },
    );
    assert_eq!(m.world.game.units[&id].air, Some(PlaneState::Home));
    m.world.game.units.get_mut(&id).unwrap().air = Some(PlaneState::Rearm);
    let before = m.aviation.planes[&id].mission;
    move_to(&mut m, &[id], p(100.0, 100.0), false, false);
    assert_eq!(m.aviation.planes[&id].mission, before);
}
#[test]
#[ignore = "MOV-87: automatic fighter seek target selection is not connected"]
fn mov_87_fighter_seeks_nearest_visible_airborne_enemy() {
    let mut m = scene();
    let a = plane(&mut m, 0);
    let b = plane(&mut m, 1);
    for (id, at) in [(a, p(60.0, 60.0)), (b, p(70.0, 60.0))] {
        let u = m.world.game.units.get_mut(&id).unwrap();
        u.x = at.x;
        u.z = at.z;
        u.air = Some(PlaneState::Station);
    }
    show_all(&mut m);
    patrol(&mut m, a, p(60.0, 60.0));
    m.step_aircraft(a, 0.05).unwrap();
    assert_eq!(m.firing.get(a).unwrap().target_id, b);
}
#[test]
#[ignore = "MOV-88: automatic aircraft ground fire and area salvos are unavailable"]
fn mov_88_ground_attack_plane_damages_ground_target() {
    let mut m = scene();
    let a = m.spawn_aircraft(0, "attacker").unwrap();
    let b = add(&mut m, "tank", 1, p(80.0, 80.0));
    let hp = m.world.game.units[&b].hp;
    show_all(&mut m);
    assert!(
        m.send_aircraft_mission(
            0,
            a,
            crate::match_aviation::PlaneMission {
                kind: crate::match_aviation::MissionKind::Attack,
                target: Some(b),
                at: p(80.0, 80.0)
            }
        )
        .unwrap()
    );
    for _ in 0..200 {
        m.step_aircraft(a, 0.05).unwrap();
    }
    assert!(m.world.game.units[&b].hp < hp);
}
#[test]
fn mov_89_aa_damages_visible_airborne_enemy_and_ignores_base() {
    let mut m = scene();
    let a = plane(&mut m, 1);
    let flak = add(&mut m, "flak", 0, p(60.0, 60.0));
    let u = m.world.game.units.get_mut(&a).unwrap();
    u.x = 65.0;
    u.z = 60.0;
    u.air = Some(PlaneState::Out);
    show_all(&mut m);
    m.world.game.units.get_mut(&flak).unwrap().still = Some(3.0);
    m.grid.rebuild(&m.world.game.units);
    let hp = m.world.game.units[&a].hp;
    m.step_aircraft_anti_air(&[flak], 0.05).unwrap();
    assert!(m.world.game.units[&a].hp < hp);
    m.world.game.units.get_mut(&a).unwrap().air = Some(PlaneState::Base);
    let hp = m.world.game.units[&a].hp;
    m.step_aircraft_anti_air(&[flak], 0.05).unwrap();
    assert_eq!(m.world.game.units[&a].hp, hp);
}
#[test]
fn mov_90_aircraft_ignore_terrain_and_crowd_separation() {
    let mut m = scene();
    let a = plane(&mut m, 0);
    patrol(&mut m, a, p(80.0, 80.0));
    for z in 0..80 {
        m.world.set_cell(z * 80 + 40, b'W').unwrap();
    }
    let before = m.world.game.units[&a].at();
    m.step_aircraft(a, 0.05).unwrap();
    assert!(distance(before, m.world.game.units[&a].at()) > 0.0);
    let at = m.world.game.units[&a].at();
    add(&mut m, "tank", 0, at);
    separation::separate_crowds(&mut m.world.game, &m.rules, &mut m.grid);
    assert_eq!(m.world.game.units[&a].at(), at);
}

#[path = "mov_intent_batch_tail.rs"]
mod tail;
