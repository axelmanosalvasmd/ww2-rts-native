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

#[test]
fn recorded_queued_cover_inherits_attack_site_and_project_anchors_without_starting_work() {
    use crate::field_works::{FortKind, Pattern, ProjectMaterial, ProjectOrder};
    let (mut m, id) = scene();
    let target = m
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 1 && m.rules.kinds[u.kind].infantry)
        .unwrap()
        .id;
    let attack_at = Point { x: 36.0, z: 20.0 };
    let enemy = m.world.game.units.get_mut(&target).unwrap();
    enemy.x = attack_at.x;
    enemy.z = attack_at.z;
    m.grid.update(target, enemy);
    crate::vision::update_vision(&mut m.world.game, &m.rules, &m.grid);
    assert!(m.world.game.players[0].sees(target));
    let worker = m.world.game.units[&id].clone();
    let paths = format!("{:?}", m.commands.paths);
    let requests = format!("{:?}", m.commands.requests);
    let rng = m.start.random.snapshot();
    let environment_seed = m.start.environment_seed;
    let mp = m.start.economy.players[0].mp;
    let spent = m.story[0].mp_spent;
    let site = m.next_unit_id;
    let build_at = Point { x: 32.0, z: 24.0 };
    let cover = || Command::Cover {
        ids: vec![i64::from(id)],
        queue: true,
        at: None,
    };
    let commands = [
        Command::Attack {
            ids: vec![i64::from(id)],
            target: i64::from(target),
            queue: true,
            at: None,
        },
        cover(),
        Command::Build {
            ids: vec![i64::from(id)],
            kind: "flakpos".into(),
            at: Some(build_at),
            direction: None,
            queue: true,
        },
        cover(),
        Command::Entrench {
            ids: vec![i64::from(id)],
            order: ProjectOrder::New {
                pattern: Some(Pattern::Line),
                material: ProjectMaterial::Fort(FortKind::Trench),
                from: Some(Point { x: 24.0, z: 30.0 }),
                to: Some(Point { x: 36.0, z: 30.0 }),
            },
            queue: true,
        },
        cover(),
    ];
    for (sequence, command) in commands.into_iter().enumerate() {
        m.enqueue_command(
            crate::orders::OrderKey {
                tick: 1,
                issuer: 0,
                sequence: sequence as u64,
            },
            command,
        )
        .unwrap();
    }
    let results = m.apply_commands(1, false).unwrap();
    assert_eq!(results.len(), 6);
    assert!(results.iter().all(|r| r.denial.is_none()));
    let project = m.field_works.projects().next().unwrap();
    assert_eq!(m.field_works.projects().count(), 1);
    assert_eq!((project.crew, project.active), (0, 0));
    assert!(!project.jobs.is_empty());
    let project_at = project.jobs[0].at;
    assert_eq!(m.world.game.units[&site].at(), build_at);
    let orders = &m.commands.units[&id].orders;
    assert_eq!(orders.len(), 6);
    assert_eq!(
        orders[0],
        Command::Attack {
            ids: vec![i64::from(id)],
            target: i64::from(target),
            queue: false,
            at: Some(attack_at),
        }
    );
    assert_eq!(
        orders[2],
        Command::Assist {
            ids: vec![i64::from(id)],
            target: i64::from(site),
            at: Some(build_at),
            queue: false,
        }
    );
    assert_eq!(
        orders[4],
        Command::Entrench {
            ids: vec![i64::from(id)],
            order: ProjectOrder::Join {
                project: project.id,
                at: Some(project_at),
            },
            queue: false,
        }
    );
    for (index, at) in [(1, attack_at), (3, build_at), (5, project_at)] {
        assert_eq!(
            orders[index],
            Command::Cover {
                ids: vec![i64::from(id)],
                queue: false,
                at: Some(at),
            }
        );
    }
    let mut live_worker = m.world.game.units[&id].clone();
    let mut expected_worker = worker;
    assert_eq!(live_worker.pace.to_bits(), expected_worker.pace.to_bits());
    // Source pace starts as NaN. Normalize detached copies for the remaining fields.
    live_worker.pace = 0.0;
    expected_worker.pace = 0.0;
    assert_eq!(live_worker, expected_worker);
    assert_eq!(m.commands.units[&id].build_target, None);
    assert_eq!(m.field_works.membership(id), None);
    assert!(m.field_works.jobs().is_empty());
    assert_eq!(m.world.game.units[&site].built, Some(0.0));
    let cost = number(
        Catalog::compiled().unwrap().unit("flakpos").unwrap(),
        "cost",
    )
    .unwrap();
    assert_eq!(m.start.economy.players[0].mp, mp - cost);
    assert_eq!(m.story[0].mp_spent, spent + cost);
    assert_eq!(format!("{:?}", m.commands.paths), paths);
    assert_eq!(format!("{:?}", m.commands.requests), requests);
    assert_eq!(m.start.random.snapshot(), rng);
    assert_eq!(m.start.environment_seed, environment_seed);
}

#[test]
fn r4_first_reached_guard_refuses_three_independent_faults_atomically() {
    use crate::{
        match_runtime::ProductionStepError,
        match_tick::{TickPathConfig, TickState},
        orders::{OrderKey, OrderSnapshot},
        production::Error as ProductionError,
        random::RandomSnapshot,
    };

    const CONSTRUCTION: &str = "waiting Build must already own a paid site and normalized Assist";
    const SCENARIO: &str = "legacy scenario point state does not match this match";
    const PRODUCTION_ADMISSION: &str =
        "production admission: Production(InvalidInput(\"production building missing\"))";

    #[derive(Debug, PartialEq)]
    struct Readback {
        world: String,
        owners: Vec<String>,
        queue: OrderSnapshot<Command>,
        random: RandomSnapshot,
        pace: Vec<(u32, u64)>,
    }

    fn readback(state: &Match) -> Readback {
        Readback {
            // Includes World's private pending, journal and terrain publication state.
            world: format!("{:?}", state.world),
            owners: vec![
                format!("{:?}", state.grid),
                format!("{:?}", state.commands.units),
                format!("{:?}", state.commands.paths),
                format!("{:?}", state.commands.requests),
                format!("{:?}", state.production),
                format!("{:?}", state.start.economy),
                format!("{:?}", state.start.points),
                format!("{:?}", state.start.players),
                format!(
                    "{:?}",
                    (
                        state.start.dimensions,
                        state.start.win_vp.to_bits(),
                        state.start.weather,
                        state.start.weather_seed,
                        state.start.environment_seed,
                        state.start.living,
                        (state.start.wind.0.to_bits(), state.start.wind.1.to_bits()),
                        state.start.initial_rain.to_bits(),
                        state.start.initial_wet.map(f64::to_bits),
                        &state.start.starting_kinds,
                    )
                ),
                format!("{:?}", state.environment),
                format!("{:?}", state.firing),
                format!("{:?}", state.support),
                format!("{:?}", state.aviation),
                format!("{:?}", state.combat),
                format!("{:?}", state.abilities),
                format!("{:?}", state.recovery),
                format!("{:?}", state.field_works),
                format!("{:?}", state.companions),
                format!("{:?}", state.logistics),
                format!("{:?}", state.supply_net),
                format!("{:?}", state.scenario.as_ref().map(|value| value.state())),
                format!("{:?}", state.recipients),
                format!("{:?}", state.routing_fog),
                format!("{:?}", state.building_cells),
                format!("{:?}", state.building_rallies),
                format!("{:?}", state.player_rallies),
                format!("{:?}", state.recruit_metadata),
                format!("{:?}", state.capture_ticks),
                format!("{:?}", state.story),
                format!("{:?}", state.fallen),
                format!("{:?}", state.finish),
                format!("{:?}", state.timeline),
                format!("{:?}", state.next_unit_id),
                format!("{:?}", state.options),
                format!("{:?}", state.rules),
                // Derived Debug includes the private started bit, fault and paths.
                format!("{:?}", state.tick_state),
            ],
            queue: state.commands.queue.snapshot(),
            random: state.start.random.snapshot(),
            // Debug represents NaN symbolically. Preserve the actual pace bits too.
            pace: state
                .world
                .game
                .units
                .iter()
                .map(|(&id, unit)| (id, unit.pace.to_bits()))
                .collect(),
        }
    }

    fn make() -> Match {
        let map = AuthoritativeMap::from_json(include_bytes!(
            "../../server/src/authoritative-maps/el-alamein.json"
        ))
        .unwrap();
        let mut state = Match::new(
            &map,
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
                map_key: Some("el-alamein".into()),
                tech: false,
                supply: false,
                logistics: false,
                sandbox: false,
            },
        )
        .unwrap();
        state
            .initialize_tick_scene(0, TickPathConfig::default())
            .unwrap();
        assert!(state.scenario.is_some());
        assert!(!state.start.points.is_empty());
        for key in [
            OrderKey {
                tick: 1,
                issuer: 0,
                sequence: 1,
            },
            OrderKey {
                tick: 3,
                issuer: 1,
                sequence: 7,
            },
        ] {
            state.enqueue_command(key, Command::Unsupported).unwrap();
        }
        assert_eq!(state.commands.pending_commands(), 2);
        assert_eq!(state.commands.next_tick(), Some(1));
        assert_eq!(
            format!("{:?}", state.tick_state),
            format!("{:?}", TickState::default())
        );
        let before = readback(&state);
        state.validate_tick_state().unwrap();
        state.validate_construction_tick().unwrap();
        state
            .validate_production_phases(state.rules.movement.tick)
            .unwrap();
        state.preflight_legacy_scenario().unwrap();
        assert_eq!(readback(&state), before, "healthy admission mutated state");
        state
    }

    fn earlier_inputs_are_healthy(state: &Match) {
        assert_eq!(state.world.game.tick, 0);
        assert_eq!(state.options.mode, Mode::Conquest);
        assert!(state.world.game.mode.is_none());
        assert!(!state.options.logistics);
        assert!(state.convoys.is_none());
        assert!(state.world_runtime.is_none());
        assert!(state.assault.is_none());
        assert!(!state.options.living_weather);
        assert!(state.start.living.is_none());
        assert_eq!(state.start.players.len(), state.world.game.players.len());
        assert_eq!(
            state.start.economy.players.len(),
            state.world.game.players.len()
        );
        assert_eq!(state.capture_ticks.len(), state.start.points.len());
        assert!(state.rules.movement.tick.is_finite());
        assert!(state.rules.movement.tick > 0.0);
        assert_eq!(state.tick_state.path_config(), TickPathConfig::default());
        assert!(state.firing.events.is_empty());
        assert!(state.support.events.is_empty());
        assert!(state.aviation.events.is_empty());
        state.validate_command_tick(1).unwrap();
        state.validate_abilities().unwrap();
        state.prepare_companion_tick().unwrap();
        state.validate_aircraft_runner_boundary(1).unwrap();
        state.validate_support_metadata().unwrap();
        let actors = state.world.game.units.keys().copied().collect::<Vec<_>>();
        state
            .validate_support_tick_phases(&actors, state.rules.movement.tick, 1)
            .unwrap();
        state.validate_carrier_tick().unwrap();
    }

    fn guard(state: &Match, which: usize) -> Result<(), String> {
        match which {
            0 => state.validate_construction_tick(),
            1 => state
                .validate_production_phases(state.rules.movement.tick)
                .map_err(|error| format!("{error:?}")),
            2 => state.preflight_legacy_scenario(),
            _ => unreachable!(),
        }
    }

    let production_error = ProductionStepError::Production(ProductionError::InvalidInput(
        "production building missing",
    ));
    let direct_reasons = [
        CONSTRUCTION.to_owned(),
        format!("{production_error:?}"),
        SCENARIO.to_owned(),
    ];
    for defects in [0_u8, 1, 2, 4, 7] {
        let mut state = make();
        if defects & 1 != 0 {
            let actor = state
                .world
                .game
                .units
                .values()
                .find(|unit| state.rules.kinds[unit.kind].infantry)
                .unwrap();
            let id = actor.id;
            let at = actor.at();
            assert!(!actor.build);
            let control = state.commands.units.get_mut(&id).unwrap();
            assert!(control.build_target.is_none());
            assert!(control.orders.is_empty());
            control.orders.push_back(Command::Build {
                ids: vec![i64::from(id)],
                kind: "flakpos".into(),
                at: Some(at),
                direction: None,
                queue: true,
            });
        }
        if defects & 2 != 0 {
            let absent = u32::MAX - 1;
            assert!(!state.world.game.units.contains_key(&absent));
            assert!(state.production.building(absent).is_none());
            state.production.initialize_building(absent).unwrap();
        }
        if defects & 4 != 0 {
            state.start.points[0].owner = Some(state.world.game.players.len());
        }
        let before = readback(&state);
        earlier_inputs_are_healthy(&state);
        for (which, reason) in direct_reasons.iter().enumerate() {
            let expected = if defects & (1 << which) != 0 {
                Err(reason.clone())
            } else {
                Ok(())
            };
            assert_eq!(guard(&state, which), expected, "defects={defects}");
        }
        if defects & 2 != 0 {
            assert_eq!(
                state.validate_production_phases(state.rules.movement.tick),
                Err(ProductionStepError::Production(
                    ProductionError::InvalidInput("production building missing")
                )),
                "the component refusal must precede the later Match wrapper refusal"
            );
        }
        assert_eq!(
            readback(&state),
            before,
            "individual guards, defects={defects}"
        );
        if defects == 7 {
            for order in [
                [0, 1, 2],
                [0, 2, 1],
                [1, 0, 2],
                [1, 2, 0],
                [2, 0, 1],
                [2, 1, 0],
            ] {
                let result = order.iter().try_for_each(|&which| guard(&state, which));
                assert_eq!(result, Err(direct_reasons[order[0]].clone()));
                assert_eq!(readback(&state), before, "direct guard order={order:?}");
            }
        }
        let expected = match defects {
            0 => {
                state.validate_tick_state().unwrap();
                assert_eq!(readback(&state), before, "healthy whole admission");
                continue;
            }
            1 => CONSTRUCTION,
            2 => PRODUCTION_ADMISSION,
            4 | 7 => SCENARIO,
            _ => unreachable!(),
        };
        assert_eq!(state.validate_tick_state(), Err(expected.into()));
        assert_eq!(
            readback(&state),
            before,
            "whole admission, defects={defects}"
        );
        // Scenario admission is reached before production/construction and started=true.
        // This checks core admission output, not a socket or post-command rollback.
        let failure = state.advance_tick().unwrap_err();
        assert_eq!(failure.reason, expected, "defects={defects}");
        assert_eq!(failure.tick, 0);
        assert!(failure.events.is_empty());
        assert_eq!(failure.retirement, Default::default());
        assert!(state.tick_state.fault().is_none());
        assert_eq!(
            format!("{:?}", state.tick_state),
            format!("{:?}", TickState::default())
        );
        assert_eq!(state.world.game.tick, 0);
        assert_eq!(state.commands.pending_commands(), 2);
        assert_eq!(state.commands.next_tick(), Some(1));
        assert_eq!(
            readback(&state),
            before,
            "failed advance, defects={defects}"
        );
    }
}
