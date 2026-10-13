use super::*;
use crate::{
    economy::Faction,
    economy_state::ArmySize,
    game::Point,
    map::AuthoritativeMap,
    match_commands::Command,
    match_runtime::{MatchOptions, PlayerConfig},
    unit_publication::{PublicationParts, StagedUnit, prepare_publication},
    weather::WeatherChoice,
};
use serde_json::json;
fn scene() -> (Match, u32) {
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"Construction guards", "w":60,"h":60,"rows":vec![".".repeat(60);60],
            "heights":vec!["0".repeat(60);60],
            "spawns":[{"x":4,"y":4},{"x":55,"y":55}],"points":[{"x":30,"y":30}]
        }))
        .unwrap(),
    )
    .unwrap();
    let mut m = Match::new(
        &map,
        MatchOptions {
            seed: 701,
            mode: Mode::Default,
            players: (0..2)
                .map(|slot| PlayerConfig {
                    name: format!("p{slot}"),
                    team: slot,
                    faction: Faction::try_from(slot as u8).unwrap(),
                })
                .collect(),
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
        },
    )
    .unwrap();
    m.start.economy.players[0].mp = 5000.0;
    let id = add_worker(&mut m, Point { x: 28.0, z: 24.0 });
    (m, id)
}
fn add_worker(m: &mut Match, at: Point) -> u32 {
    let id = m.next_unit_id;
    let staged = StagedUnit::ordinary(
        &m.world.game,
        &m.rules,
        0,
        m.rules.kind("engineer").unwrap(),
        id,
        m.world.game.units.len() as u32,
        m.start.players[0].spawn,
        Some(at),
    )
    .unwrap();
    prepare_publication(
        PublicationParts {
            units: &mut m.world.game.units,
            grid: &mut m.grid,
            controls: &mut m.commands.units,
            recruitment: &mut m.recruit_metadata,
            next_id: &mut m.next_unit_id,
        },
        &m.rules,
        2,
        vec![staged],
    )
    .unwrap()
    .publish_with(|u, _| {
        m.combat.initialize(u.id);
        m.firing.initialize(u.id);
        m.abilities.initialize(u.id);
        m.recovery.initialize(u.id);
    });
    id
}
// Use the actual scheduler API to create a prior failed retry. This is a typed
// capacity/commit boundary setup, separate from the original gameplay replays.
fn seed_retry(m: &mut Match, id: u32) {
    use crate::path_requests::{
        PathRequestContext, PathUpdate, Request, RequestKind, SearchOutcome, TickConfig,
    };
    struct FailedRoute;
    impl PathRequestContext for FailedRoute {
        type Error = String;
        fn current_goal(&mut self, _: u32, _: RequestKind) -> Option<Point> {
            Some(Point { x: 32.0, z: 24.0 })
        }
        fn unit_incarnation(&self, id: u32) -> Option<u64> {
            Some(u64::from(id))
        }
        fn knowledge_version(&self, _: u32) -> u64 {
            0
        }
        fn search(&mut self, _: Request) -> Result<SearchOutcome, String> {
            Ok(SearchOutcome {
                path: Vec::new(),
                failed: true,
            })
        }
        fn apply(&mut self, _: PathUpdate) {}
    }
    let mut context = FailedRoute;
    m.commands
        .requests
        .begin_tick(
            TickConfig {
                retry_limit: 3,
                ..TickConfig::default()
            },
            &mut context,
        )
        .unwrap();
    m.commands
        .requests
        .request(
            Request {
                unit: id,
                kind: RequestKind::Build,
                goal: Point { x: 32.0, z: 24.0 },
            },
            &mut context,
        )
        .unwrap();
    assert!(format!("{:?}", m.commands.requests).contains("count: 1"));
}
fn build(m: &mut Match, id: u32) -> u32 {
    let site = m.next_unit_id;
    assert_eq!(
        m.execute_command(
            0,
            &Command::Build {
                ids: vec![i64::from(id)],
                kind: "flakpos".into(),
                at: Some(Point { x: 32.0, z: 24.0 }),
                direction: None,
                queue: false
            },
            false
        )
        .unwrap(),
        None
    );
    site
}
#[test]
fn rare_placement_rejects_after_draft_routes_without_publishing_or_warming_live_fog() {
    // Restored orphan AIR rows cannot be reused by a paid static site.
    for plane in [false, true] {
        let (mut m, worker) = scene();
        let site = m.next_unit_id;
        if plane {
            m.aviation.planes.insert(
                site,
                crate::match_aviation::AircraftRuntime {
                    fuel: 1.0,
                    ammo: 1.0,
                    timer: 0.0,
                    orbit: 0.0,
                    mission: None,
                    downed: false,
                },
            );
        } else {
            m.aviation
                .anti_air
                .insert(site, crate::match_aviation::AntiAirRuntime::default());
        }
        let snapshot = |m: &Match| {
            format!(
                "{:?}",
                (
                    (&m.world, &m.grid),
                    (&m.commands.units, &m.commands.paths, &m.commands.requests),
                    (&m.recipients, &m.routing_fog),
                    (&m.story, &m.start.economy.players),
                    (&m.combat, &m.firing, &m.abilities, &m.recovery),
                    (
                        &m.production,
                        &m.building_cells,
                        &m.building_rallies,
                        &m.recruit_metadata,
                        &m.field_works
                    ),
                    (
                        &m.aviation,
                        m.next_unit_id,
                        m.start.random.snapshot(),
                        m.start.environment_seed,
                        m.start.wind
                    ),
                )
            )
        };
        let before = snapshot(&m);
        let command = Command::Build {
            ids: vec![i64::from(worker)],
            kind: "flakpos".into(),
            at: Some(Point { x: 32.0, z: 24.0 }),
            direction: None,
            queue: false,
        };
        assert!(m.execute_command(0, &command, false).is_err());
        assert_eq!(snapshot(&m), before);
    }
    let (mut m, id) = scene();
    let unit = m.world.game.units[&id].clone();
    m.command_route(0, &unit, Point { x: 30.0, z: 30.0 })
        .unwrap();
    assert!(m.routing_fog.teams.is_empty()); // Source empty fire filtering is lazy.
    let second = add_worker(&mut m, Point { x: 27.0, z: 24.0 });
    m.world.game.units.get_mut(&second).unwrap().path_gen = u64::MAX;
    seed_retry(&mut m, id);
    let requests = format!("{:?}", m.commands.requests);
    let world = format!("{:?}", m.world);
    let grid = format!("{:?}", m.grid);
    let controls = m.commands.units.clone();
    let routing = format!("{:?}", m.recipients);
    let paths = format!("{:?}", m.commands.paths);
    let fog = m.routing_fog.clone();
    let story = m.story.clone();
    let allocator = m.next_unit_id;
    let mp = m.start.economy.players[0].mp;
    let rng = m.start.random.snapshot();
    assert!(
        m.execute_build(
            0,
            &[i64::from(id), i64::from(second)],
            "flakpos",
            Some(Point { x: 32.0, z: 24.0 }),
            None,
            false
        )
        .is_err()
    );
    assert_eq!(format!("{:?}", m.commands.requests), requests);
    assert_eq!(format!("{:?}", m.world), world);
    assert_eq!(format!("{:?}", m.grid), grid);
    assert_eq!(m.commands.units, controls);
    assert_eq!(format!("{:?}", m.recipients), routing);
    assert_eq!(format!("{:?}", m.commands.paths), paths);
    assert_eq!(m.routing_fog, fog);
    assert_eq!(m.story, story);
    assert_eq!(m.next_unit_id, allocator);
    assert_eq!(m.start.economy.players[0].mp, mp);
    assert_eq!(m.start.random.snapshot(), rng);
    m.world.game.units.get_mut(&second).unwrap().path_gen = 0;
    let initial = m.world.initial().chars.as_ptr();
    let flags = m.world.game.flags.as_ptr();
    let chars = m.world.game.chars.as_ptr();
    let height = m.world.game.height.as_ref().unwrap().as_ptr();
    let site = build(&mut m, id);
    assert_eq!(m.world.initial().chars.as_ptr(), initial);
    assert_eq!(m.world.game.flags.as_ptr(), flags);
    assert_eq!(m.world.game.chars.as_ptr(), chars);
    assert_eq!(m.world.game.height.as_ref().unwrap().as_ptr(), height);
    assert_eq!(m.commands.units[&id].build_target, Some(site));
    assert!(m.world.game.units[&id].build);
    assert_eq!(m.world.game.units[&site].built, Some(0.0));
    assert!(m.routing_fog.teams.is_empty());
    assert_eq!(m.start.random.snapshot(), rng);
    assert!(!format!("{:?}", m.commands.requests).contains("count: 1"));
}
#[test]
fn captured_worker_health_and_pose_changes_do_not_recompute_crew_membership() {
    let (mut m, id) = scene();
    let site = build(&mut m, id);
    let mut frame = m.begin_construction_tick().unwrap();
    m.step_construction_worker(id, &mut frame).unwrap();
    let u = m.world.game.units.get_mut(&id).unwrap();
    u.hp = 0.0;
    u.x = 90.0;
    u.z = 90.0;
    m.finish_construction_tick(frame).unwrap();
    let definition = Catalog::compiled().unwrap().unit("flakpos").unwrap();
    let rate = 1.0 / number(definition, "buildTime").unwrap() * m.rules.movement.tick;
    assert_eq!(
        m.world.game.units[&site].built.unwrap().to_bits(),
        rate.to_bits()
    );
    assert_eq!(m.commands.units[&id].build_target, Some(site));
}
#[test]
fn crew_capacity_and_layer_admission_precede_progress_story_and_section_health() {
    let (mut m, id) = scene();
    let site = build(&mut m, id);
    let mut frame = m.begin_construction_tick().unwrap();
    m.step_construction_worker(id, &mut frame).unwrap();
    m.world.versions.vehicle = u64::MAX;
    let before = format!("{:?}", m.world);
    let story = m.story.clone();
    assert!(m.finish_construction_tick(frame).is_err());
    assert_eq!(format!("{:?}", m.world), before);
    assert_eq!(m.story, story);
    m.world.versions.vehicle = 0;
    let mut frame = m.begin_construction_tick().unwrap();
    m.step_construction_worker(id, &mut frame).unwrap();
    m.world.game.ground = None;
    let before = format!("{:?}", m.world);
    assert!(m.finish_construction_tick(frame).is_err());
    assert_eq!(format!("{:?}", m.world), before);
    assert_eq!(m.story, story);
    assert_eq!(m.world.game.units[&site].built, Some(0.0));
}

#[test]
fn queued_site_does_not_query_routes_and_actual_queries_preserve_same_key_fog() {
    let (mut m, id) = scene();
    let unit = m.world.game.units[&id].clone();
    m.command_route(0, &unit, Point { x: 30.0, z: 30.0 })
        .unwrap();
    seed_retry(&mut m, id);
    let requests = format!("{:?}", m.commands.requests);
    let routing = format!("{:?}", m.recipients);
    let paths = format!("{:?}", m.commands.paths);
    let fog = m.routing_fog.clone();
    let site = m.next_unit_id;
    assert_eq!(
        m.execute_build(
            0,
            &[i64::from(id)],
            "flakpos",
            Some(Point { x: 32.0, z: 24.0 }),
            None,
            true
        )
        .unwrap(),
        None
    );
    assert_eq!(format!("{:?}", m.commands.requests), requests);
    assert_eq!(format!("{:?}", m.recipients), routing);
    assert_eq!(format!("{:?}", m.commands.paths), paths);
    assert_eq!(m.routing_fog, fog);
    assert_eq!(m.commands.units[&id].build_target, None);
    assert_eq!(
        m.commands.units[&id].orders.front(),
        Some(&Command::Assist {
            ids: vec![i64::from(id)],
            target: i64::from(site),
            at: Some(Point { x: 32.0, z: 24.0 }),
            queue: false,
        })
    );
    for warm in [false, true] {
        let (mut m, id) = scene();
        m.world.fires.insert(1230, 10.0);
        if warm {
            m.routing_fog.team(&m.world.game, &m.rules, 0);
        }
        let prior = m.routing_fog.clone();
        build(&mut m, id);
        if warm {
            assert_eq!(m.routing_fog, prior);
        } else {
            assert!(m.routing_fog.teams.contains_key(&0));
        }
    }
}

#[test]
fn target_level_denials_and_unlisted_builds_retain_follow_and_slits_keep_finite_angles() {
    let (mut m, id) = scene();
    m.commands.units.get_mut(&id).unwrap().follow = 77;
    assert_eq!(
        m.execute_assist(0, &[i64::from(id)], 0, false).unwrap(),
        Some(crate::match_commands::Denial::Unseen)
    );
    assert_eq!(m.commands.units[&id].follow, 77);
    assert_eq!(
        m.execute_build(0, &[i64::from(id)], "unknown", None, None, false)
            .unwrap(),
        Some(crate::match_commands::Denial::Blocked)
    );
    assert_eq!(m.commands.units[&id].follow, 77);
    let site = m.next_unit_id;
    let command =
        Command::from_json(&json!({"t":"build","kind":"pillbox","ids":[id],"x":32,"z":24,"dir":7}))
            .unwrap();
    assert_eq!(m.execute_command(0, &command, false).unwrap(), None);
    assert_eq!(m.world.game.units[&site].rot, 7.0);
    assert_eq!(m.commands.units[&site].aim, 0.0);
    assert_eq!(m.commands.units[&id].follow, 0);
}
