//! Native movement intent checks mapped in docs/testing/mov-impl-native-intent.md.
//! Authored setup is input, never imported Source output. No capture is required.
use crate::{
    catalog::Catalog,
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::{MOVE, PlaneState, Player, Point},
    map::AuthoritativeMap,
    match_commands::{Command, MatchCommands, MoveOrder, MoveOutcome, UnitControl},
    match_runtime::{Match, MatchOptions, PlayerConfig},
    math, separation, traffic,
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
fn mov_41_retry_cap_resets_on_new_command_or_observed_knowledge() {
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
    // A new command resets all 20 attempts. Change observed terrain after 19
    // failures, while keeping the same impossible goal and a closed barrier.
    for attempt in 1..=19 {
        m.world.game.tick += 1;
        m.begin_path_tick(4096, 20).unwrap();
        m.request_command_path(id, goal, crate::path_requests::RequestKind::World)
            .unwrap();
        assert_ne!(
            m.commands.units[&id].outcome,
            Some(MoveOutcome::Unreachable),
            "new command attempt {attempt}"
        );
    }
    let version = m.route_failure_version(0);
    m.world.set_cell(15 * 80 + 39, b'W').unwrap();
    m.world.game.tick += 1;
    m.refresh_recipient_routing(0).unwrap();
    assert_ne!(
        m.route_failure_version(0),
        version,
        "changed observed knowledge"
    );
    m.begin_path_tick(4096, 20).unwrap();
    m.request_command_path(id, goal, crate::path_requests::RequestKind::World)
        .unwrap();
    assert_ne!(
        m.commands.units[&id].outcome,
        Some(MoveOutcome::Unreachable)
    );
    assert_eq!(m.world.game.units[&id].world_goal, Some(goal));
    for _ in 0..19 {
        m.world.game.tick += 1;
        m.begin_path_tick(4096, 20).unwrap();
        m.request_command_path(id, goal, crate::path_requests::RequestKind::World)
            .unwrap();
    }
    assert_eq!(
        m.commands.units[&id].outcome,
        Some(MoveOutcome::Unreachable)
    );
}
#[test]
fn mov_44_nearest_free_masks_bfs_clamping_and_origin_region() {
    use crate::game::{LAND, SHOAL, VBLOCK};
    // These are the four blockOf masks at the pin. Exact cells below follow
    // Source's east, west, south, north breadth-first visitation.
    for mask in [MOVE, MOVE | VBLOCK, LAND, LAND | SHOAL] {
        let mut m = scene();
        let mut placer = SpawnPlacer::new();
        m.world.game.flags.fill(mask);
        for (free, expected) in [
            (
                vec![30 * 80 + 31, 30 * 80 + 29, 31 * 80 + 30, 29 * 80 + 30],
                30 * 80 + 31,
            ),
            (vec![30 * 80 + 29, 31 * 80 + 30, 29 * 80 + 30], 30 * 80 + 29),
            (vec![31 * 80 + 30, 29 * 80 + 30], 31 * 80 + 30),
            (vec![29 * 80 + 30], 29 * 80 + 30),
            (vec![30 * 80 + 32, 30 * 80 + 28], 30 * 80 + 32),
        ] {
            m.world.game.flags.fill(mask);
            for cell in free {
                m.world.game.flags[cell] = (MOVE | VBLOCK | LAND | SHOAL) & !mask;
            }
            assert_eq!(
                placer
                    .nearest_free(
                        crate::match_runtime::terrain_view(&m.world.game),
                        p(61.0, 61.0),
                        mask,
                        None
                    )
                    .unwrap(),
                expected
            );
        }
        m.world.game.flags.fill(0);
        let view = crate::match_runtime::terrain_view(&m.world.game);
        assert_eq!(
            placer
                .nearest_free(view, p(-20.0, -20.0), mask, None)
                .unwrap(),
            0
        );
        assert_eq!(
            placer
                .nearest_free(view, p(999.0, 999.0), mask, None)
                .unwrap(),
            6399
        );
        for z in 0..80 {
            m.world.game.flags[z * 80 + 40] = mask;
        }
        let view = crate::match_runtime::terrain_view(&m.world.game);
        assert_eq!(
            placer
                .nearest_free(view, p(110.0, 31.0), mask, None)
                .unwrap(),
            15 * 80 + 55
        );
        assert_eq!(
            placer
                .nearest_free(view, p(110.0, 31.0), mask, Some(p(31.0, 31.0)))
                .unwrap(),
            15 * 80 + 39
        );
        m.world.game.flags.fill(mask);
        assert_eq!(
            placer
                .nearest_free(
                    crate::match_runtime::terrain_view(&m.world.game),
                    p(61.0, 61.0),
                    mask,
                    None
                )
                .unwrap(),
            30 * 80 + 30
        );
    }
}
#[test]
fn mov_47_idle_spacing_preserves_cover_and_respects_reaction_and_target() {
    for case in ["open", "cover", "isolated-cover", "react", "target"] {
        let mut m = scene();
        if case == "cover" {
            for z in 27..34 {
                for x in 27..34 {
                    m.world.set_cell(z * 80 + x, b'O').unwrap();
                }
            }
        }
        if case == "isolated-cover" {
            m.world.set_cell(30 * 80 + 30, b'O').unwrap();
        }
        let a = add(&mut m, "rifle", 0, p(60.0, 60.0));
        let b = add(&mut m, "rifle", 0, p(60.5, 60.0));
        let before = m.world.game.units[&b].at();
        let rank = crate::terrain::cover_rank(&m.world.game, &m.rules, before, None);
        m.commands.units.get_mut(&b).unwrap().react = if case == "react" { 0.1 } else { 0.0 };
        if case == "target" {
            m.firing.get_mut(b).unwrap().target_id = a;
        }
        m.initialize_recipient_routing().unwrap();
        m.begin_path_tick(4096, 20).unwrap();
        m.react_unit(a).unwrap();
        assert!(m.world.game.units[&a].path.is_empty(), "older squad stays");
        m.react_unit(b).unwrap();
        if ["react", "target", "isolated-cover"].contains(&case) {
            assert!(m.world.game.units[&b].path.is_empty(), "{case}");
        } else {
            let end = *m.world.game.units[&b]
                .path
                .last()
                .expect("newer squad spaces itself");
            assert!(distance(end, m.world.game.units[&a].at()) >= 2.75, "{case}");
            assert!(distance(end, before) <= 4.0, "{case}");
            assert!(
                crate::terrain::cover_rank(&m.world.game, &m.rules, end, None) <= rank,
                "{case}"
            );
        }
    }
}

#[test]
fn mov_52_priority_tiers_age_cap_ties_and_traffic_durations() {
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
    a.retreating = false;
    a.drift = true;
    a.path.clear();
    a.traffic_wait = 0.0;
    b.retreating = false;
    b.traffic_entered = false;
    b.path = vec![p(100.0, 40.0)];
    assert!(
        !traffic::traffic_wins(&m.rules, &a, &b),
        "drift yields to a moving unit"
    );
    b.path.clear();
    b.traffic_wait = 0.0;
    a.traffic_wait = 9.999;
    assert!(
        traffic::traffic_wins(&m.rules, &a, &b),
        "equal priority breaks on id"
    );
    a.traffic_entered = false;
    b.traffic_entered = true;
    assert!(
        !traffic::traffic_wins(&m.rules, &a, &b),
        "occupied lane wins equal aged priority"
    );
    a.traffic_wait = 10.0;
    assert!(
        traffic::traffic_wins(&m.rules, &a, &b),
        "fifth age increment"
    );
    b.retreating = true;
    a.traffic_wait = 1e6;
    assert!(
        !traffic::traffic_wins(&m.rules, &a, &b),
        "age caps at 25 below retreat priority"
    );

    // A short endpoint limits predicted velocity to 4 m/s. Both contacts are
    // inside the spatial query and straddle the 0.7 s closest-approach horizon.
    for (offset, blocked) in [(-0.01, true), (0.01, false)] {
        let mut m = scene();
        let a = add(&mut m, "tank", 0, p(60.0, 60.0));
        let radius = m.rules.kinds[m.world.game.units[&a].kind].body_radius();
        let b = add(
            &mut m,
            "tank",
            1,
            p(60.0, 60.0 + 2.0 * radius + 0.15 + 4.0 * 0.7 + offset),
        );
        route(&mut m, a, p(60.0, 60.2));
        show_all(&mut m);
        assert_eq!(
            steer(&mut m, a).blocked,
            blocked,
            "horizon offset {offset}, blocker {b}"
        );
    }
    let mut m = scene();
    let a = add(&mut m, "tank", 0, p(60.0, 60.0));
    let b = add(&mut m, "tank", 0, p(60.0, 65.0));
    route(&mut m, a, p(60.0, 100.0));
    route(&mut m, b, p(60.0, 20.0));
    for step in 1..=7 {
        m.world.game.tick = step;
        let out = steer(&mut m, b);
        assert_eq!(out.yielding, step == 7);
    }
    let until = m.world.game.units[&b].traffic.as_ref().unwrap().until;
    assert!((until - (7.0 * 0.05 + 5.0)).abs() < 1e-9);
    // Hold the actor short of its sidestep. At expiry the old maneuver must
    // retire even though the conflict still exists.
    m.world.game.tick = 106;
    assert!(steer(&mut m, b).yielding);
    assert_eq!(
        m.world.game.units[&b].traffic.as_ref().unwrap().until,
        until
    );
    m.world.game.tick = 107;
    steer(&mut m, b);
    assert!(
        m.world.game.units[&b]
            .traffic
            .as_ref()
            .is_none_or(|t| t.until > until)
    );
    // With every candidate blocked, enemies request a replan after three
    // seconds. Friends get a two-second ghost on the following call instead.
    for friend in [false, true] {
        let mut m = scene();
        let a = add(&mut m, "tank", 0, p(60.0, 60.0));
        let b = add(&mut m, "tank", if friend { 0 } else { 1 }, p(60.0, 65.0));
        route(&mut m, a, p(60.0, 100.0));
        route(&mut m, b, p(60.0, 20.0));
        m.world.game.units.get_mut(&a).unwrap().retreating = true;
        for z in 0..80 {
            for x in 0..80 {
                if !(29..=30).contains(&x) {
                    m.world.game.flags[z * 80 + x] |= MOVE;
                }
            }
        }
        show_all(&mut m);
        for step in 1..=61 {
            m.world.game.tick = step;
            let out = steer(&mut m, b);
            assert_eq!(out.replan, step == 61, "replan {friend}/{step}");
        }
        m.world.game.tick = 62;
        let out = steer(&mut m, b);
        if friend {
            assert!(!out.blocked);
            let u = &m.world.game.units[&b];
            assert_eq!(u.ghost, 102.0);
            assert!(traffic::ghost_pair(
                &m.world.game,
                u,
                &m.world.game.units[&a],
                true
            ));
            m.world.game.tick = 101;
            assert!(traffic::ghost_pair(
                &m.world.game,
                &m.world.game.units[&b],
                &m.world.game.units[&a],
                true
            ));
            m.world.game.tick = 102;
            assert!(!traffic::ghost_pair(
                &m.world.game,
                &m.world.game.units[&b],
                &m.world.game.units[&a],
                true
            ));
        } else {
            assert!(out.replan);
            assert_eq!(m.world.game.units[&b].ghost, 0.0);
        }
    }
}

#[test]
fn mov_63_bridge_and_ford_peers_keep_forward_progress() {
    for crossing in *b"=F." {
        let mut m = scene();
        m.world.set_cell(30 * 80 + 30, crossing).unwrap();
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
        if crossing == b'.' {
            assert!(
                m.world.game.units[&a].z < before[0],
                "ordinary separation may push backwards"
            );
        } else {
            assert!(m.world.game.units[&a].z >= before[0]);
            assert!(m.world.game.units[&b].z >= before[1]);
        }
        assert!((m.world.game.units[&a].x - m.world.game.units[&b].x).abs() > 0.5);
    }
}
#[test]
fn mov_64_held_workers_do_not_push_routed_friends() {
    for protected in 0..5 {
        let mut m = scene();
        let a = add(&mut m, "rifle", 0, p(60.0, 60.0));
        let b = add(&mut m, "rifle", 0, p(60.5, 60.0));
        let u = m.world.game.units.get_mut(&a).unwrap();
        match protected {
            0 => u.hold_pos = true,
            1 => u.build = true,
            2 => u.dig = true,
            3 => u.entrench = true,
            _ => {
                m.world.set_cell(30 * 80 + 30, b'O').unwrap();
            }
        }
        if protected == 4 {
            assert!(crate::terrain::in_cover(
                &m.world.game,
                &m.rules,
                &m.world.game.units[&a]
            ));
        }
        route(&mut m, b, p(100.0, 60.0));
        let positions = [m.world.game.units[&a].at(), m.world.game.units[&b].at()];
        separation::separate_crowds(&mut m.world.game, &m.rules, &mut m.grid);
        assert_eq!(m.world.game.units[&a].at(), positions[0]);
        assert_eq!(m.world.game.units[&b].at(), positions[1]);
    }
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
    let player = m
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 0 && m.rules.kinds[u.kind].infantry)
        .unwrap()
        .id;
    let before = m.world.game.units[&player].at();
    m.apply_world_guard_home(player).unwrap();
    assert_eq!(m.world.game.units[&player].at(), before);
    {
        let u = m.world.game.units.get_mut(&id).unwrap();
        u.x = home.x + 16.0;
        u.z = home.z;
    }
    m.apply_world_guard_home(id).unwrap();
    assert_eq!(m.world.game.units[&id].at(), p(home.x + 16.0, home.z));

    let u = m.world.game.units.get_mut(&id).unwrap();
    u.x = home.x + 20.0;
    u.path = vec![p(home.x + 30.0, home.z)];
    u.attack = true;
    m.commands.units.get_mut(&id).unwrap().attack_id = player;
    m.apply_world_guard_home(id).unwrap();
    let u = &m.world.game.units[&id];
    assert_eq!(u.at(), home);
    assert!(u.path.is_empty());
    assert!(!u.attack);
    assert_eq!(m.commands.units[&id].attack_id, 0);
}
#[test]
fn mov_72_carrier_tables_and_boarding_enforce_capacity() {
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
    for (name, riders, reinforces) in [
        ("halftrack", false, true),
        ("lorry", false, false),
        ("tank", true, false),
        ("medium", true, false),
        ("tankdestroyer", true, false),
        ("tiger", true, false),
        ("churchill", true, false),
        ("lcvp", false, false),
        ("train", false, false),
    ] {
        let def = catalog.unit(name).unwrap();
        assert_eq!(
            def.get("riders").and_then(|v| v.boolean()).unwrap_or(false),
            riders,
            "{name}"
        );
        assert_eq!(
            def.get("reinforces")
                .and_then(|v| v.boolean())
                .unwrap_or(false),
            reinforces,
            "{name}"
        );
    }
    for (name, seats) in [("tank", 2), ("tiger", 3), ("halftrack", 1), ("lorry", 3)] {
        let mut m = scene();
        let carrier = add(&mut m, name, 0, p(60.0, 60.0));
        let ids = (0..seats + 1)
            .map(|i| add(&mut m, "rifle", 0, p(61.0 + i as f64 * 0.5, 60.0)))
            .collect::<Vec<_>>();
        command(
            &mut m,
            Command::Board {
                ids: ids.iter().rev().map(|id| i64::from(*id)).collect(),
                target: Some(carrier.into()),
            },
        );
        for &id in &ids[..seats] {
            assert_eq!(m.world.game.units[&id].board, carrier);
            m.apply_unit_boarding(id).unwrap();
            assert!(m.world.game.units[&id].riding);
        }
        assert_eq!(m.combat.get(carrier).unwrap().cargo.len(), seats);
        let extra = ids[seats];
        assert_eq!(m.world.game.units[&extra].board, 0);
        assert_eq!(
            m.execute_command(
                0,
                &Command::Board {
                    ids: vec![extra.into()],
                    target: Some(carrier.into())
                },
                false
            )
            .unwrap(),
            Some(crate::match_commands::Denial::Maximum)
        );
    }
}

#[path = "mov_intent_batch_tail.rs"]
mod tail;
