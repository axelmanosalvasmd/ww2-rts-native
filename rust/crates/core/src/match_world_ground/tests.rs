use super::*;
use crate::{
    economy::Faction,
    economy_state::ArmySize,
    fog::TeamFog,
    grid::SpatialGrid,
    map::AuthoritativeMap,
    match_commands::{MoveResult, UnitControl},
    match_runtime::{MatchOptions, PlayerConfig},
    random::MatchRandom,
    route_knowledge::{CellMetadata, CoverBasis, RememberedCell, RouteKnowledge},
    traffic_knowledge::TrafficKnowledge,
    weather::WeatherChoice,
};
use serde_json::{Value, json};
fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../../simtest/tests/fixtures/world-ground-routing.json"
    ))
    .unwrap()
}
fn point(v: &Value) -> Point {
    Point {
        x: v["x"].as_f64().unwrap(),
        z: v["z"].as_f64().unwrap(),
    }
}
fn bits(v: f64) -> Value {
    json!(format!("{:016x}", v.to_bits()))
}
fn path(v: &[Point]) -> Value {
    json!(v.iter().map(|p| [bits(p.x), bits(p.z)]).collect::<Vec<_>>())
}
fn slot(v: &Value) -> usize {
    let n = v.as_i64().unwrap();
    if n < 0 {
        LOCAL_DEFENDER_OWNER
    } else {
        n as usize
    }
}
fn make(f: &Value) -> Match {
    let defs = f["definitions"].as_array().unwrap();
    let regions = defs
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let mut r = r.clone();
            r["bounds"] = json!([i % 8 * 64, i / 8 * 64, i % 8 * 64 + 64, i / 8 * 64 + 64]);
            if let Some(home) = [0, 63, 1].iter().position(|v| *v == i) {
                r["home"] = json!(home);
            }
            r
        })
        .collect::<Vec<_>>();
    let map=AuthoritativeMap::from_json(&serde_json::to_vec(&json!({"name":"World ground oracle","w":512,"h":512,
        "rows":vec![".".repeat(512);512],"spawns":[{"x":32,"y":32},{"x":480,"y":480},{"x":96,"y":32}],"points":[],
        "world":{"size":"huge","total":64,"regionCells":64,"regions":regions}})).unwrap()).unwrap();
    let options = MatchOptions {
        seed: 9182,
        mode: Mode::World,
        players: [5, 1, 5]
            .iter()
            .enumerate()
            .map(|(i, &team)| PlayerConfig {
                name: format!("P{i}"),
                team,
                faction: Faction::Usa,
            })
            .collect(),
        shuffle: false,
        army: ArmySize::Standard,
        weather: WeatherChoice::Map,
        weather_seed: None,
        living_weather: false,
        map_key: None,
        tech: false,
        supply: true,
        logistics: false,
        sandbox: false,
    };
    Match::new_world(&map, options, MatchRandom::new(9182)).unwrap()
}
fn unit(m: &mut Match, raw: &Value) {
    let id = raw["id"].as_u64().unwrap() as u32;
    let kind = m.rules.kind(raw["type"].as_str().unwrap()).unwrap();
    let make = if m.rules.kinds[kind].air {
        crate::unit_spawn::initial_aircraft
    } else {
        crate::unit_spawn::initial_unit
    };
    let mut u = make(
        &m.rules,
        Catalog::compiled().unwrap(),
        kind,
        id,
        slot(&raw["owner"]),
        point(raw),
    )
    .unwrap();
    u.hp = raw["hp"].as_f64().unwrap_or(u.hp);
    u.built = raw["built"].as_f64();
    u.shot_at = raw["shotAt"].as_f64();
    u.path = raw["path"]
        .as_array()
        .map(|a| a.iter().map(point).collect())
        .unwrap_or_default();
    u.traffic_wait = raw["trafficWait"].as_f64().unwrap_or(0.0);
    u.stuck = raw["stuck"].as_f64().unwrap_or(0.0);
    u.world_goal = raw["worldGoal"]
        .as_object()
        .map(|_| point(&raw["worldGoal"]));
    if let Some(s) = raw["air"]["state"].as_str() {
        u.air = Some(match s {
            "base" => crate::game::PlaneState::Base,
            "out" => crate::game::PlaneState::Out,
            "station" => crate::game::PlaneState::Station,
            "home" => crate::game::PlaneState::Home,
            _ => panic!("air"),
        });
    }
    let mut c = UnitControl::new_for_mode(&u, &m.rules, Mode::World).unwrap();
    c.attack_id = raw["attackId"].as_u64().unwrap_or(0) as u32;
    c.guard_home = raw["guardHome"]
        .as_object()
        .map(|_| point(&raw["guardHome"]));
    m.commands.insert(id, c);
    m.combat.initialize(id).hit_at = raw["hitAt"].as_f64().unwrap_or(-1e9);
    m.firing.initialize(id).target_id = raw["targetId"].as_u64().unwrap_or(0) as u32;
    m.world.game.units.insert(id, u);
}
fn setup(m: &mut Match, input: &Value) {
    m.world.game.units.clear();
    m.commands.units.clear();
    // Each recorder case constructs a fresh source Game and its derived caches.
    m.commands.paths = crate::pathfinding::Pathfinding::new();
    m.commands.requests = Default::default();
    assert_eq!(
        m.commands.inspected_path_tick(),
        None,
        "path-service scenes must start before any path tick"
    );
    m.building_cells.clear();
    m.world.game.tick = input["tick"].as_i64().unwrap_or(400);
    m.world.game.vision_tick = Some(0);
    m.world.game.reveal = input["reveal"].as_bool().unwrap_or(false);
    let runtime = m.world_runtime.as_mut().unwrap();
    runtime.memory = Default::default();
    runtime.known_buildings.clear();
    runtime.local_defender.spawn = input["neutralSpawn"]
        .as_object()
        .map(|_| point(&input["neutralSpawn"]))
        .unwrap_or(Point { x: 11., z: 13. });
    runtime.local_defender.visible.clear();
    for (i, r) in runtime.regions.iter_mut().enumerate() {
        r.team = input["regionTeams"][i.to_string()]
            .as_i64()
            .map(|v| v as i32)
            .unwrap_or(if i == 0 || i == 20 {
                5
            } else if i == 63 {
                1
            } else {
                -1
            });
    }
    for p in &mut m.world.game.players {
        p.visible.clear();
    }
    if let Some(units) = input["units"].as_array() {
        for u in units {
            unit(m, u);
        }
    }
    if input["unit"].is_object() {
        unit(m, &input["unit"]);
    }
    m.grid = SpatialGrid::new(&m.world.game.units, 16.0);
}
fn visibility(m: &mut Match, owner: usize, rows: &Value) {
    let ids = rows
        .as_array()
        .map(|a| a.iter().map(|v| v.as_u64().unwrap() as u32).collect())
        .unwrap_or_default();
    if owner == LOCAL_DEFENDER_OWNER {
        m.world_runtime.as_mut().unwrap().local_defender.visible = ids;
    } else {
        m.world.game.players[owner].visible = ids;
    }
}
#[test]
fn original_world_march_home_visibility_cover_and_first_contact() {
    let f = fixture();
    let mut m = make(&f);
    for c in f["march"].as_array().unwrap() {
        setup(&mut m, &c["input"]);
        let u = &m.world.game.units[&7];
        assert_eq!(
            bits(m.world_march_multiplier(u).unwrap()),
            c["expectedBits"],
            "{}",
            c["name"]
        );
    }
    for c in f["home"].as_array().unwrap() {
        setup(&mut m, &c["input"]);
        let mut order = c["input"]["units"]
            .as_array()
            .unwrap()
            .iter()
            .map(|u| u["id"].as_u64().unwrap() as u32)
            .collect::<Vec<_>>();
        if !order.contains(&7) {
            order.push(7);
        }
        let home = m
            .world_ground_home_in_order(&m.world.game.units[&7], &order)
            .unwrap();
        assert_eq!(
            json!([bits(home.x), bits(home.z)]),
            json!([c["expected"]["xBits"], c["expected"]["zBits"]]),
            "{}",
            c["name"]
        );
    }
    for c in f["visibility"].as_array().unwrap() {
        if !c["input"]["unit"].is_object() {
            continue;
        }
        setup(&mut m, &c["input"]);
        let owner = slot(&c["input"]["slot"]);
        visibility(&mut m, owner, &c["input"]["visible"]);
        let knowledge = RouteKnowledge::world(512, 512, 5, m.rules.clone()).unwrap();
        let query = knowledge.query();
        let actor = m.actor_policy(owner).unwrap();
        let terrain = TrafficKnowledge::observed(query.navigation(), &knowledge, &[])
            .unwrap()
            .with_actor(owner, actor.visible);
        assert_eq!(
            terrain.seen_by(&m.world.game, owner, &m.world.game.units[&7]),
            c["expected"].as_bool().unwrap(),
            "{}",
            c["name"]
        );
    }
    for c in f["cover"].as_array().unwrap() {
        let mut knowledge = RouteKnowledge::world(512, 512, 5, m.rules.clone()).unwrap();
        let rows = c["input"]["rows"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
            .map(|(i, r)| RememberedCell {
                cell: r[0].as_u64().unwrap() as usize,
                revision: i as u64 + 1,
                character: r[1].as_str().unwrap().as_bytes()[0],
                height: 0,
                state: 0,
                metadata: CellMetadata {
                    trench_front: r[4]["trenchFront"].as_f64(),
                    ..Default::default()
                },
                cover_basis: Some(CoverBasis {
                    house_kind: Some(0),
                    toughness: 1.0,
                }),
            })
            .collect::<Vec<_>>();
        knowledge.apply_rows(rows.len() as u64, &rows).unwrap();
        let wrecks = c["input"]["wrecks"]
            .as_array()
            .map(|a| {
                a.iter()
                    .map(|r| crate::game::Wreck {
                        id: Some(r["id"].as_u64().unwrap() as u32),
                        cell: Some(
                            (r["z"].as_f64().unwrap() / 2.).floor() as usize * 512
                                + (r["x"].as_f64().unwrap() / 2.).floor() as usize,
                        ),
                        x: r["x"].as_f64().unwrap(),
                        z: r["z"].as_f64().unwrap(),
                        moving: false,
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let from = c["input"]["from"]
            .as_object()
            .map(|_| point(&c["input"]["from"]));
        assert_eq!(
            knowledge
                .cover_rank(point(&c["input"]["at"]), from, &wrecks)
                .unwrap() as u64,
            c["expected"]["rank"].as_u64().unwrap(),
            "{}",
            c["name"]
        );
    }
    for c in f["traffic"].as_array().unwrap() {
        setup(&mut m, &c["input"]);
        unit(&mut m, &c["input"]["other"]);
        m.grid = SpatialGrid::new(&m.world.game.units, 16.);
        let mut u = m.world.game.units[&7].clone();
        visibility(&mut m, u.owner, &c["input"]["visible"]);
        let knowledge = RouteKnowledge::world(512, 512, 5, m.rules.clone()).unwrap();
        let query = knowledge.query();
        let actor = m.actor_policy(u.owner).unwrap();
        let read = std::cell::Cell::new(false);
        let traffic = TrafficKnowledge::observed(query.navigation(), &knowledge, &[])
            .unwrap()
            .with_actor(u.owner, actor.visible)
            .track_reads(&read);
        let goal = u.path.first().copied();
        let steer = crate::traffic::traffic_step_with_knowledge(
            &m.world.game,
            &m.rules,
            &m.grid,
            &mut u,
            goal,
            m.rules.movement.tick,
            &traffic,
        )
        .unwrap();
        assert_eq!(
            json!({"goal":steer.goal.map(|p|json!([bits(p.x),bits(p.z)])),"blocked":steer.blocked,"waiting":steer.waiting,"queuedBehind":u.queued_behind,"waitBits":bits(u.traffic_wait)}),
            c["expected"],
            "{}",
            c["name"]
        );
        assert!(
            !read.get(),
            "{}: source first contact does not read terrain",
            c["name"]
        );
    }
}

#[test]
fn mov08_world_march_moves_calm_owned_squad_faster_than_combat_squad() {
    let f = fixture();
    let cases = f["march"].as_array().unwrap();
    let mut checked = 0;
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let mut m = make(&f);
        setup(&mut m, &case["input"]);
        let unit = &m.world.game.units[&7];
        let march = m.world_march_multiplier(unit).unwrap();
        let expected = u64::from_str_radix(case["expectedBits"].as_str().unwrap(), 16).unwrap();
        assert_eq!(march.to_bits(), expected, "{name}");
        if m.rules.kinds[unit.kind].air {
            assert!(!crate::movement::reaches_block(&m.rules, unit), "{name}");
            continue;
        }
        if unit.x < 0.0 {
            continue;
        }
        let start = unit.at();
        let kind = unit.kind;
        let mut actual = m.world.game.clone();
        let mut baseline = m.world.game.clone();
        if m.rules.kinds[kind].naval {
            actual.flags.fill(0);
            baseline.flags.fill(0);
        }
        for game in [&mut actual, &mut baseline] {
            let u = game.units.get_mut(&7).unwrap();
            u.path = vec![Point {
                x: start.x + 20.0,
                z: start.z,
            }];
            u.path_gen += 1;
        }
        let mut actual_grid = SpatialGrid::new(&actual.units, 16.0);
        let mut baseline_grid = SpatialGrid::new(&baseline.units, 16.0);
        let mut routes =
            |_: &crate::game::Game,
             _: &crate::rules::Rules,
             _: &mut crate::game::Unit,
             _: Point,
             _: crate::movement::RouteRequest| panic!("no route request: {name}");
        let moved = crate::movement::move_unit_with_march(
            &mut actual,
            &m.rules,
            &mut actual_grid,
            7,
            1.0,
            &mut routes,
            &TrafficKnowledge::physical(),
            march,
        )
        .unwrap()
        .unwrap();
        let base = crate::movement::move_unit_with_march(
            &mut baseline,
            &m.rules,
            &mut baseline_grid,
            7,
            1.0,
            &mut routes,
            &TrafficKnowledge::physical(),
            1.0,
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            moved.speed.to_bits(),
            (base.speed * march).to_bits(),
            "{name} speed"
        );
        if m.rules.kinds[kind].infantry || m.rules.kinds[kind].naval {
            assert!(
                ((actual.units[&7].x - start.x) - (baseline.units[&7].x - start.x) * march).abs()
                    < 1e-10,
                "{name} distance",
            );
        }
        checked += 1;
    }
    assert_eq!(checked, 16);
}
fn vision_mask(m: &mut Match, owner: usize, cells: &Value) {
    let team = m.actor_policy(owner).unwrap().team;
    let mut fog = TeamFog::new(512 * 512);
    fog.key = Some(m.world.game.vision_tick.unwrap());
    for c in cells.as_array().unwrap() {
        fog.vis[c.as_u64().unwrap() as usize] = 1;
    }
    m.world_runtime
        .as_mut()
        .unwrap()
        .memory
        .fog
        .teams
        .insert(team, fog);
}
fn mutation(m: &mut Match, a: &Value) {
    let c = a["c"].as_u64().unwrap() as usize;
    m.world
        .mutate_cell(
            c,
            crate::world::CellChange {
                object: a["change"]["object"].as_str().map(|v| v.as_bytes()[0]),
                ..Default::default()
            },
        )
        .unwrap();
}
fn route_state(m: &Match, owner: usize, result: &crate::pathfinding::PathResult) -> Value {
    let team = m.actor_policy(owner).unwrap().team;
    let memory = m.world_runtime.as_ref().unwrap().memory.team(team).unwrap();
    json!({"path":path(&result.waypoints),"routeEnd":[bits(result.route_end.x),bits(result.route_end.z)],
        "memoryVersion":memory.version(),"navigationVersion":memory.query().navigation().versions.knowledge})
}
#[test]
fn original_world_route_histories_and_full_memory_retry_version() {
    let f = fixture();
    for case in f["routing"].as_array().unwrap() {
        let mut m = make(&f);
        setup(&mut m, &case["input"]);
        let owner = slot(&case["input"]["slot"]);
        vision_mask(&mut m, owner, &case["input"]["visibleCells"]);
        for (i, a) in case["actions"].as_array().unwrap().iter().enumerate() {
            match a["op"].as_str().unwrap() {
                "read" => {}
                "mutate" => mutation(&mut m, a),
                "visibility" => {
                    m.world.game.vision_tick = Some(m.world.game.vision_tick.unwrap() + 1);
                    vision_mask(&mut m, owner, &a["cells"]);
                }
                "wear" => {
                    let c = a["c"].as_u64().unwrap() as usize;
                    m.world.game.wear.as_mut().unwrap()[c] = a["value"].as_f64().unwrap() as f32;
                    m.world.touch(c).unwrap();
                    m.world.game.vision_tick = Some(m.world.game.vision_tick.unwrap() + 1);
                    vision_mask(&mut m, owner, &a["cells"]);
                }
                _ => panic!("action"),
            }
            let u = m.world.game.units[&7].clone();
            let result = m
                .search_unit_route(&u, point(&case["input"]["goal"]))
                .unwrap()
                .result;
            let expected = &case["captures"][i];
            let expected = json!({"path":expected["path"],"routeEnd":expected["routeEnd"],"memoryVersion":expected["memoryVersion"],"navigationVersion":expected["navigationVersion"]});
            assert_eq!(
                route_state(&m, owner, &result),
                expected,
                "{}:{i}",
                case["name"]
            );
        }
    }
    let case = &f["scheduler"][0];
    let mut m = make(&f);
    setup(&mut m, &case["input"]);
    for a in case["input"]["initialEdits"].as_array().unwrap() {
        mutation(&mut m, a);
    }
    vision_mask(&mut m, 0, &case["input"]["visibleCells"]);
    for (i, capture) in case["captures"].as_array().unwrap().iter().enumerate() {
        if capture["mutation"].is_object() {
            let c = capture["mutation"]["c"].as_u64().unwrap() as usize;
            m.world.game.wear.as_mut().unwrap()[c] =
                capture["mutation"]["wear"].as_f64().unwrap() as f32;
            m.world.touch(c).unwrap();
            m.world.game.vision_tick = Some(m.world.game.vision_tick.unwrap() + 1);
            vision_mask(&mut m, 0, &case["input"]["visibleCells"]);
        }
        m.begin_path_tick(4096, 3).unwrap();
        m.request_command_path(
            7,
            point(&case["input"]["goal"]),
            crate::path_requests::RequestKind::World,
        )
        .unwrap();
        let u = &m.world.game.units[&7];
        let memory = m.world_runtime.as_ref().unwrap().memory.team(5).unwrap();
        assert_eq!(
            json!({"path":path(&u.path),"worldGoal":u.world_goal.map(|p|json!([bits(p.x),bits(p.z)])),"memoryVersion":memory.version(),"navigationVersion":memory.query().navigation().versions.knowledge}),
            json!({"path":capture["path"],"worldGoal":capture["worldGoal"],"memoryVersion":capture["memoryVersion"],"navigationVersion":capture["navigationVersion"]}),
            "retry {i}"
        );
        assert_eq!(
            matches!(
                m.commands.units[&7].move_result,
                Some(MoveResult::Unreachable { .. })
            ),
            capture["moveOutcome"] == "unreachable"
        );
        m.world.game.tick += 1;
    }
}
fn same_unit(actual: &crate::game::Unit, expected: &crate::game::Unit) {
    assert_eq!(actual.pace.to_bits(), expected.pace.to_bits());
    let mut actual = actual.clone();
    let mut expected = expected.clone();
    // Compare the absent NaN pace by bits before normalizing detached copies only.
    actual.pace = 0.0;
    expected.pace = 0.0;
    assert_eq!(actual, expected);
}
#[test]
fn remembered_terrain_read_discards_world_trial_and_matches_prepared_move() {
    let f = fixture();
    let case = &f["movement"][1];
    assert_eq!(
        case["name"],
        "neutral near waypoint uses remembered clear skip"
    );
    let mut optimized = make(&f);
    let mut prepared = make(&f);
    for m in [&mut optimized, &mut prepared] {
        setup(m, &case["input"]);
        vision_mask(m, LOCAL_DEFENDER_OWNER, &case["input"]["visibleCells"]);
        m.begin_path_tick(4096, 3).unwrap();
    }

    let id = 7;
    let original = optimized.world.game.units[&id].clone();
    let original_grid =
        optimized
            .grid
            .unordered(&optimized.world.game.units, (17., 17.), 50., |_, _| true);
    let original_random = optimized.start.random.snapshot();
    let march = optimized.world_march_multiplier(&original).unwrap();
    let visible = optimized
        .actor_policy(original.owner)
        .unwrap()
        .visible
        .to_vec();
    let terrain_read = std::cell::Cell::new(false);
    let physical = TrafficKnowledge::physical()
        .with_actor(original.owner, &visible)
        .track_reads(&terrain_read);
    let trial = crate::movement::trial_move_with_march(
        &mut optimized.world.game,
        &optimized.rules,
        &optimized.grid,
        id,
        1.,
        &mut |_, _, _, _, _| panic!("this case must read terrain without requesting a route"),
        &physical,
        march,
    )
    .unwrap()
    .unwrap();
    assert!(trial.succeeded());
    assert!(
        terrain_read.get(),
        "the real movement trial must read terrain"
    );
    assert!(
        optimized
            .move_world_unit_without_memory(id, 1., march, original.owner, &visible)
            .unwrap()
            .is_none(),
        "a terrain-reading trial must be discarded"
    );
    same_unit(&optimized.world.game.units[&id], &original);
    assert_eq!(
        optimized
            .grid
            .unordered(&optimized.world.game.units, (17., 17.), 50., |_, _| true),
        original_grid
    );
    assert!(
        optimized
            .world_runtime
            .as_ref()
            .unwrap()
            .memory
            .team(-1)
            .is_none()
    );
    assert_eq!(optimized.start.random.snapshot(), original_random);

    let moved = optimized
        .move_unit_with_command_routes(id, 1.)
        .unwrap()
        .unwrap();

    // Run the original prepared branch directly on a separately constructed
    // Match. The route callback must stay unused in this terrain-read case.
    let runtime = prepared.world_runtime.as_mut().unwrap();
    let preview = crate::match_world_memory::prepare_world_routing(
        crate::match_world_memory::WorldRoutingContext {
            world: &prepared.world,
            environment: &prepared.environment,
            rules: &prepared.rules,
            start: &prepared.start,
            rails: &runtime.rails,
            regions: &runtime.regions,
            known_buildings: &runtime.known_buildings,
            local_defender: &runtime.local_defender,
        },
        &mut runtime.memory,
        original.owner,
        None,
    )
    .unwrap();
    let paths = preview.fork_paths(&prepared.commands.paths);
    let (game, mut commit) = preview
        .prepare_publication(&mut prepared.world, &prepared.environment)
        .unwrap();
    let read = std::cell::Cell::new(false);
    let query = commit.query();
    query.path_terrain(commit.live()).unwrap();
    let knowledge =
        TrafficKnowledge::observed(query.navigation(), commit.knowledge(), commit.wrecks())
            .unwrap()
            .with_actor(original.owner, &visible)
            .track_reads(&read);
    let expected = crate::movement::move_unit_with_march(
        game,
        &prepared.rules,
        &mut prepared.grid,
        id,
        1.,
        &mut |_, _, _, _, _| panic!("prepared move must not request a route"),
        &knowledge,
        march,
    )
    .unwrap()
    .unwrap();
    assert!(read.get());
    drop(query);
    commit.sync_moved_object(&game.units[&id], &prepared.rules, true);
    commit.commit();
    prepared.commands.paths = paths;

    assert_eq!(
        (moved.id, moved.infantry, moved.speed.to_bits()),
        (expected.id, expected.infantry, expected.speed.to_bits())
    );
    same_unit(
        &optimized.world.game.units[&id],
        &prepared.world.game.units[&id],
    );
    for center in [(17., 17.), (21., 17.), (27., 17.)] {
        assert_eq!(
            optimized
                .grid
                .unordered(&optimized.world.game.units, center, 8., |_, _| true),
            prepared
                .grid
                .unordered(&prepared.world.game.units, center, 8., |_, _| true)
        );
    }
    let actual_memory = &optimized.world_runtime.as_ref().unwrap().memory;
    let expected_memory = &prepared.world_runtime.as_ref().unwrap().memory;
    let actual_team = actual_memory.team(-1).unwrap();
    let expected_team = expected_memory.team(-1).unwrap();
    assert_eq!(actual_team.version(), expected_team.version());
    assert_eq!(
        actual_team.query().navigation().versions.knowledge,
        expected_team.query().navigation().versions.knowledge
    );
    let actual_bound = actual_memory
        .live_for(original.owner)
        .unwrap()
        .units
        .iter()
        .find(|u| u.id == id)
        .unwrap();
    let expected_bound = expected_memory
        .live_for(original.owner)
        .unwrap()
        .units
        .iter()
        .find(|u| u.id == id)
        .unwrap();
    assert_eq!(
        (actual_bound.at.x.to_bits(), actual_bound.at.z.to_bits()),
        (expected_bound.at.x.to_bits(), expected_bound.at.z.to_bits())
    );
    assert_eq!(
        optimized.start.random.snapshot(),
        prepared.start.random.snapshot()
    );
}
#[test]
fn lazy_world_movement_and_refusal_preserve_memory_ledger_and_history() {
    let f = fixture();
    let mut m = make(&f);
    for case in f["movement"].as_array().unwrap() {
        let mut m = make(&f);
        setup(&mut m, &case["input"]);
        let owner = slot(&case["input"]["slot"]);
        vision_mask(&mut m, owner, &case["input"]["visibleCells"]);
        m.begin_path_tick(4096, 3).unwrap();
        let moved = m.move_unit_with_command_routes(7, 1.0).unwrap().unwrap();
        let u = &m.world.game.units[&7];
        let expected = &case["expected"];
        assert_eq!(
            json!({"speedBits":bits(moved.speed),"xBits":bits(u.x),"zBits":bits(u.z),"rotBits":bits(u.rot),"path":path(&u.path)}),
            json!({"speedBits":expected["speedBits"],"xBits":expected["xBits"],"zBits":expected["zBits"],"rotBits":expected["rotBits"],"path":expected["path"]}),
            "{}",
            case["name"]
        );
        let runtime = m.world_runtime.as_ref().unwrap();
        let team = m.actor_policy(owner).unwrap().team;
        if expected["readCount"] == 0 {
            assert!(runtime.memory.team(team).is_none());
        } else {
            let memory = runtime.memory.team(team).unwrap();
            assert_eq!(
                memory.version(),
                expected["memoryVersion"].as_u64().unwrap()
            );
            assert_eq!(
                memory.query().navigation().versions.knowledge,
                expected["navigationVersion"].as_u64().unwrap()
            );
            let bound = runtime
                .memory
                .live_for(owner)
                .unwrap()
                .units
                .iter()
                .find(|u| u.id == 7)
                .unwrap();
            assert_eq!(
                json!([bits(bound.at.x), bits(bound.at.z)]),
                json!([
                    expected["cachedUnitPose"]["xBits"],
                    expected["cachedUnitPose"]["zBits"]
                ])
            );
        }
    }
    let c = &f["traffic"][0];
    setup(&mut m, &c["input"]);
    unit(&mut m, &c["input"]["other"]);
    m.grid = SpatialGrid::new(&m.world.game.units, 16.);
    m.begin_path_tick(4096, 3).unwrap();
    assert!(m.world_runtime.as_ref().unwrap().memory.teams.is_empty());
    let pending = format!("{:?}", m.world.pending(crate::world::Recipient::Player(0)));
    let journal = (m.world.journal_base(), m.world.journal().to_vec());
    m.move_unit_with_command_routes(7, 1.).unwrap();
    assert!(m.world_runtime.as_ref().unwrap().memory.teams.is_empty());
    assert!(
        m.world_runtime
            .as_ref()
            .unwrap()
            .memory
            .fog
            .teams
            .is_empty()
    );
    assert_eq!(
        format!("{:?}", m.world.pending(crate::world::Recipient::Player(0))),
        pending
    );
    assert_eq!(
        (m.world.journal_base(), m.world.journal().to_vec()),
        journal
    );
    // Admission refusal precedes queue, memory and maintained-grid mutation.
    let queue = format!("{:?}", m.commands.requests);
    let unit = m.world.game.units[&7].clone();
    let grid = m
        .grid
        .unordered(&m.world.game.units, (17., 17.), 50., |_, _| true);
    let mut foreign = unit.clone();
    foreign.owner = 0;
    assert!(m.world_ground_home_for(&foreign).is_err());
    foreign = unit.clone();
    foreign.kind = m.rules.kind("tank").unwrap();
    assert!(m.world_ground_home_for(&foreign).is_err());
    same_unit(&m.world.game.units[&7], &unit);
    assert_eq!(format!("{:?}", m.commands.requests), queue);
    assert!(m.world_runtime.as_ref().unwrap().memory.teams.is_empty());
    m.options.logistics = true;
    assert!(m.begin_path_tick(4, 3).is_err());
    assert_eq!(format!("{:?}", m.commands.requests), queue);
    same_unit(&m.world.game.units[&7], &unit);
    assert_eq!(
        m.grid
            .unordered(&m.world.game.units, (17., 17.), 50., |_, _| true),
        grid
    );
    assert!(m.world_runtime.as_ref().unwrap().memory.teams.is_empty());
    m.options.logistics = false;
    m.commands.units.get_mut(&7).unwrap().attack_id = 90;
    assert!(m.begin_path_tick(4, 3).is_err());
    assert_eq!(format!("{:?}", m.commands.requests), queue);
    assert!(m.world_ground_home_in_order(&unit, &[7]).is_err());
}
