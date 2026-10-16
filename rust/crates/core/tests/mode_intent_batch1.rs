//! Main-supported mode checks using canonical constructors and retained exact data.
use ww2_core::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::{LOCAL_DEFENDER_OWNER, ModeKind, Point},
    map::{AuthoritativeMap, MapError},
    match_end::EndReason,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    match_world_end::WorldFinishUnitFacts,
    random::MatchRandom,
    weather::WeatherChoice,
    world_generation::{self, WorldSize, generate_world},
};

fn options(mode: Mode, count: usize) -> MatchOptions {
    MatchOptions {
        seed: 1821,
        mode,
        players: (0..count)
            .map(|i| PlayerConfig {
                name: format!("Commander {i}"),
                team: i as i32,
                faction: Faction::try_from((i % 3) as u8).unwrap(),
            })
            .collect(),
        shuffle: false,
        army: ArmySize::Standard,
        weather: WeatherChoice::Map,
        weather_seed: None,
        living_weather: false,
        map_key: None,
        tech: false,
        supply: false,
        logistics: false,
        sandbox: false,
    }
}

fn weather_match(choice: WeatherChoice, seed: Option<u32>, map_weather: &str) -> Match {
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&serde_json::json!({
            "name": "Weather runtime", "w":64, "h":64,
            "rows":vec![".".repeat(64);64],
            "spawns":[{"x":4,"y":4},{"x":59,"y":59}],
        "points":[{"x":32,"y":32}], "weather":map_weather
        }))
        .unwrap(),
    )
    .unwrap();
    let mut opts = options(Mode::Conquest, 2);
    opts.weather = choice;
    opts.weather_seed = seed;
    opts.living_weather = true;
    Match::new(&map, opts).unwrap()
}

#[test]
fn mode07_core_accepts_fresh_recorded_weather_seeds_and_replays_without_other_stream_changes() {
    use ww2_core::weather::{WeatherKind, WeatherPlan};
    // Existing exact weather fixture: random seed 0 starts fog and lifts at tick 4460.
    let a = weather_match(WeatherChoice::Random, Some(0), "clear");
    let replay = weather_match(WeatherChoice::Random, Some(0), "clear");
    let b = weather_match(WeatherChoice::Random, Some(1), "clear");
    assert_eq!(a.start.weather_seed, 0);
    assert_eq!(b.start.weather_seed, 1);
    assert_eq!(
        a.start.weather,
        WeatherPlan {
            now: WeatherKind::Fog,
            next: Some(WeatherKind::Clear),
            at: 4460
        }
    );
    assert_eq!(a.start.weather, replay.start.weather);
    assert_ne!(a.start.weather, b.start.weather);
    assert_eq!(a.start.environment_seed, b.start.environment_seed);
    assert_eq!(a.start.wind, b.start.wind);
    assert_eq!(a.start.random.snapshot(), b.start.random.snapshot());
    let units = |m: &Match| {
        m.world
            .game
            .units
            .values()
            .map(|u| (u.id, u.owner, u.kind, u.x.to_bits(), u.z.to_bits()))
            .collect::<Vec<_>>()
    };
    assert_eq!(units(&a), units(&b));
    // Omitting an explicit plan seed is deterministic, rather than fresh entropy.
    let derived = weather_match(WeatherChoice::Random, None, "clear");
    let derived_replay = weather_match(WeatherChoice::Random, None, "clear");
    assert_eq!(
        derived.start.weather_seed,
        derived_replay.start.weather_seed
    );
    assert_eq!(derived.start.weather, derived_replay.start.weather);
    assert_eq!(
        derived.start.random.snapshot(),
        derived_replay.start.random.snapshot()
    );
}

#[test]
fn mode07_fixed_and_map_weather_hold_through_ticks_with_rain_and_mud_initialization() {
    use ww2_core::weather::WeatherKind;
    for kind in [
        WeatherKind::Clear,
        WeatherKind::Fog,
        WeatherKind::Rain,
        WeatherKind::Mud,
        WeatherKind::Snow,
    ] {
        for choice in [WeatherChoice::Fixed(kind), WeatherChoice::Map] {
            let mut m = weather_match(choice, Some(71), kind.name());
            assert_eq!(m.start.weather.now, kind);
            assert_eq!(m.world.game.weather, Some(kind as usize));
            assert_eq!(
                m.world.game.rain,
                if kind == WeatherKind::Rain { 1.0 } else { 0.0 }
            );
            assert_eq!(
                m.world.game.wet,
                Some(if matches!(kind, WeatherKind::Rain | WeatherKind::Mud) {
                    1.0
                } else {
                    0.0
                })
            );
            assert_eq!(m.start.living.unwrap().raining, kind == WeatherKind::Rain);
            // Pass the latest random transition time, without a wall-clock wait.
            m.initialize_tick_scene(8_400, Default::default()).unwrap();
            for _ in 0..3 {
                m.advance_tick().unwrap();
                assert_eq!(m.start.weather.now, kind);
                assert_eq!(m.start.weather.next, None);
                assert_eq!(m.world.game.weather, Some(kind as usize));
                if kind == WeatherKind::Rain {
                    assert_eq!(m.world.game.rain, 1.0);
                }
                if kind == WeatherKind::Mud {
                    assert_eq!(m.world.game.wet, Some(1.0));
                }
            }
        }
    }
}

#[test]
fn mode07_random_weather_transitions_once_at_the_announced_core_tick() {
    use ww2_core::weather::WeatherKind;
    for (seed, before, after, earliest, latest) in [
        (0, WeatherKind::Fog, WeatherKind::Clear, 210 * 20, 270 * 20),
        (6, WeatherKind::Rain, WeatherKind::Mud, 300 * 20, 420 * 20),
    ] {
        let mut m = weather_match(WeatherChoice::Random, Some(seed), "clear");
        let plan = m.start.weather;
        assert_eq!(plan.now, before);
        assert_eq!(plan.next, Some(after));
        assert!((earliest..latest).contains(&plan.at));
        m.initialize_tick_scene(u64::from(plan.at - 2), Default::default())
            .unwrap();
        m.advance_tick().unwrap();
        assert_eq!(m.start.weather, plan);
        m.advance_tick().unwrap();
        assert_eq!(m.start.weather.now, after);
        assert_eq!(m.start.weather.next, None);
        assert_eq!(m.start.weather.at, 0);
        assert_eq!(m.world.game.weather, Some(after as usize));
        m.advance_tick().unwrap();
        assert_eq!(m.start.weather.now, after);
        assert_eq!(m.start.weather.next, None);
    }
}

#[test]
fn mode32_assault_authoring_checks_ranges_sets_flags_and_shared_spawns() {
    use serde_json::json;
    let original: serde_json::Value = serde_json::from_slice(include_bytes!(
        "../../server/src/authoritative-maps/hill-112.json"
    ))
    .unwrap();
    assert_eq!(original["spawns"].as_array().unwrap().len(), 4);
    let load = |source: &serde_json::Value| {
        AuthoritativeMap::from_json(&serde_json::to_vec(source).unwrap())
    };
    for value in [
        json!(299),
        json!(3601),
        json!(900.5),
        json!(-1),
        json!(null),
        json!(true),
        json!("900"),
    ] {
        let mut source = original.clone();
        source["assaultTime"] = value.clone();
        let error = load(&source).unwrap_err();
        assert_eq!(
            error.to_string(),
            "assaultTime must be 300-3600 seconds",
            "{value}"
        );
    }
    for value in [
        json!([]),
        json!([0, 0]),
        json!([0, 1, 2, 3]),
        json!([4]),
        json!([-1]),
        json!([0.5]),
        json!([true]),
        json!(["0"]),
        json!(null),
    ] {
        let mut source = original.clone();
        source["defend"] = value.clone();
        let error = load(&source).unwrap_err();
        assert_eq!(
            error.to_string(),
            "defend must list some (not all) spawn numbers",
            "{value}"
        );
    }
    for time in [300, 900, 3600] {
        for defenders in [vec![0], vec![3], vec![2, 0, 1]] {
            let mut source = original.clone();
            source["assaultTime"] = json!(time);
            source["defend"] = json!(defenders);
            let map = load(&source).unwrap();
            assert_eq!(map.assault_time(), Some(time));
            assert_eq!(map.defend(), Some(defenders.as_slice()));
        }
    }
    for value in [json!(1), json!("true"), json!(null), json!([])] {
        let mut source = original.clone();
        source["spawns"][0]["assault"] = value;
        assert!(
            matches!(load(&source), Err(MapError::Invalid { field, .. }) if field == "assault")
        );
    }
    for restricted in 0..=4 {
        let mut source = original.clone();
        for i in 0..4 {
            source["spawns"][i]["assault"] = json!(i < restricted);
        }
        let result = load(&source);
        if restricted <= 2 {
            let map = result.unwrap();
            assert_eq!(
                map.spawns().iter().filter(|s| !s.assault).count(),
                4 - restricted
            );
        } else {
            assert!(matches!(result, Err(MapError::Invalid { field, .. }) if field == "spawns"));
        }
    }
    let mut source = original;
    source.as_object_mut().unwrap().remove("assaultTime");
    source.as_object_mut().unwrap().remove("defend");
    for spawn in source["spawns"].as_array_mut().unwrap() {
        spawn.as_object_mut().unwrap().remove("assault");
    }
    let map = load(&source).unwrap();
    assert_eq!(map.assault_time(), None);
    assert_eq!(map.defend(), None);
    assert!(map.spawns().iter().all(|s| !s.assault));
}

#[test]
fn mode72_home_choices_match_original_seeds_for_one_through_six_players() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/world-generation.json")).unwrap();
    assert_eq!(
        fixture["reference"],
        "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
    );
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 10);
    for case in cases {
        let size = match case["size"].as_str().unwrap() {
            "huge" => WorldSize::Huge,
            "massive" => WorldSize::Massive,
            other => panic!("unexpected size {other}"),
        };
        let seed = case["seed"].as_u64().unwrap() as u32;
        let all_teams = case["teams"].as_array().unwrap();
        for count in 1..=6 {
            let teams = all_teams[..count]
                .iter()
                .map(|t| t.as_i64().unwrap() as i32)
                .collect::<Vec<_>>();
            let generated =
                world_generation::generate_from_terrain_seed(seed, size, &teams).unwrap();
            assert_eq!(generated.spawns.len(), count);
            let expected = case["regions"].as_array().unwrap();
            assert_eq!(generated.regions.len(), expected.len());
            for (region, source) in generated.regions.iter().zip(expected) {
                let home = source["home"]
                    .as_u64()
                    .map(|v| v as usize)
                    .filter(|&v| v < count);
                assert_eq!(
                    region.home, home,
                    "{size:?} seed {seed} count {count} region {}",
                    region.id
                );
                assert_eq!(region.team, home.map(|slot| teams[slot]));
            }
            for (spawn, source) in generated
                .spawns
                .iter()
                .zip(case["spawns"].as_array().unwrap())
            {
                assert_eq!(
                    (spawn.x, spawn.y),
                    (
                        source["x"].as_u64().unwrap() as usize,
                        source["y"].as_u64().unwrap() as usize
                    )
                );
            }
        }
    }
}

#[test]
fn mode76_world_finish_checks_threshold_rivals_draw_and_capture_location() {
    for size in [WorldSize::Huge, WorldSize::Massive] {
        for single_team in [false, true] {
            let mut options = options(Mode::World, 3);
            let teams = if single_team { [4, 4, 4] } else { [4, 1, 4] };
            for (player, team) in options.players.iter_mut().zip(teams) {
                player.team = team;
            }
            let mut random = MatchRandom::new(options.seed);
            let generated = generate_world(&mut random, size, &teams, true).unwrap();
            let map = AuthoritativeMap::from_world(generated.into_world_map_input()).unwrap();
            let m = Match::new_world(&map, options, random).unwrap();
            let total = size.region_count();
            let goal = match size {
                WorldSize::Huge => 39,
                WorldSize::Massive => 77,
            };
            let cases = if single_team {
                vec![
                    (goal, false, None),
                    (total - 1, false, None),
                    (total, false, Some((4, EndReason::World))),
                ]
            } else {
                vec![
                    (goal - 1, false, None),
                    (goal, false, Some((4, EndReason::World))),
                    (total, true, Some((4, EndReason::World))),
                    (1, true, Some((4, EndReason::Nations))),
                    (0, true, Some((-1, EndReason::Draw))),
                ]
            };
            for (owned, eliminate_rival, expected) in cases {
                // These are staged post-capture/elimination inputs to the real finish owner.
                let mut regions = m.world_runtime.as_ref().unwrap().regions.clone();
                for (i, region) in regions.iter_mut().enumerate() {
                    region.team = if i < owned {
                        4
                    } else if !single_team && !eliminate_rival {
                        1
                    } else {
                        -1
                    };
                    region.progress = if region.team >= 0 { 1.0 } else { 0.0 };
                    region.cap_tick = None;
                }
                // The first current-tick capture supplies the end spot, even for a rival.
                let captured = if owned == 1 { 0 } else { total - 2 };
                if owned > 0 {
                    regions[captured].cap_tick = Some(m.world.game.tick);
                    if owned > 1 {
                        regions[total - 1].cap_tick = Some(m.world.game.tick);
                        regions[0].cap_tick = Some(m.world.game.tick - 1);
                    }
                }
                let mut wallets = m.start.economy.clone();
                for wallet in &mut wallets.players {
                    wallet.out = !regions.iter().any(|r| r.team == wallet.team);
                }
                let mut survivors = Vec::new();
                let mut removed = Vec::new();
                for unit in m.world.game.units.values() {
                    if wallets.players.get(unit.owner).is_some_and(|p| p.out) {
                        removed.push(unit.id);
                    } else {
                        survivors.push(WorldFinishUnitFacts {
                            id: unit.id,
                            owner: unit.owner,
                            kind: unit.kind,
                            hp: unit.hp,
                        });
                    }
                }
                let decision = m
                    .prepare_world_finish_after_removal(&wallets, &regions, &survivors, &removed)
                    .unwrap();
                assert_eq!(
                    decision.as_ref().map(|(f, _)| (f.winner, f.reason)),
                    expected,
                    "{size:?} single={single_team} owned={owned}"
                );
                if let Some((finish, _sample)) = decision {
                    let region = &m.world.geography().unwrap().regions[captured];
                    let at = if finish.winner == -1 {
                        Point {
                            x: m.world.game.w as f64,
                            z: m.world.game.h as f64,
                        }
                    } else {
                        Point {
                            x: (region.x as f64 + 0.5) * 2.0,
                            z: (region.y as f64 + 0.5) * 2.0,
                        }
                    };
                    assert_eq!(finish.end_at, at);
                    assert_eq!(finish.end_tick, m.world.game.tick);
                    if !single_team && owned == goal {
                        for region in &mut regions {
                            region.cap_tick = None;
                        }
                        let (fallback, _) = m
                            .prepare_world_finish_after_removal(
                                &wallets, &regions, &survivors, &removed,
                            )
                            .unwrap()
                            .unwrap();
                        assert_eq!(fallback.winner, finish.winner);
                        assert_eq!(fallback.reason, finish.reason);
                        assert_eq!(
                            fallback.end_at,
                            Point {
                                x: m.world.game.w as f64,
                                z: m.world.game.h as f64
                            }
                        );
                    }
                }
                assert!(m.finish.is_none());
                assert!(m.timeline.is_empty());
            }
        }
    }
}

#[test]
fn mode73_generated_world_keeps_local_defenders_out_of_player_seats() {
    for (size, count) in [(WorldSize::Huge, 1), (WorldSize::Massive, 6)] {
        let options = options(Mode::World, count);
        let teams = options.players.iter().map(|p| p.team).collect::<Vec<_>>();
        let mut random = MatchRandom::new(options.seed);
        let generated = generate_world(&mut random, size, &teams, true).unwrap();
        let map = AuthoritativeMap::from_world(generated.into_world_map_input()).unwrap();
        let m = Match::new_world(&map, options, random).unwrap();
        assert_eq!(m.world.game.mode, Some(ModeKind::World));
        assert_eq!(m.options.players.len(), count);
        let geography = m.world.geography().unwrap();
        let runtime = m.world_runtime.as_ref().unwrap();
        assert_eq!(runtime.regions.len(), size.region_count());
        assert_eq!(runtime.guard_bases.len(), size.region_count() - count);
        for (i, region) in geography.regions.iter().enumerate() {
            if let Some(owner) = region.home {
                assert_eq!(runtime.regions[i].team, teams[owner]);
                assert_eq!(runtime.regions[i].progress, 1.0);
            } else {
                assert_eq!(runtime.regions[i].team, -1);
                assert_eq!(runtime.regions[i].progress, 0.0);
                let bases = runtime
                    .guard_bases
                    .iter()
                    .filter(|(_, r)| **r == i)
                    .collect::<Vec<_>>();
                assert_eq!(bases.len(), 1);
                let base = &m.world.game.units[bases[0].0];
                assert_eq!(base.owner, LOCAL_DEFENDER_OWNER);
                assert_eq!(m.rules.kinds[base.kind].name, "worldbase");
                assert!(base.hp > 0.0);
            }
        }
        for (owner, &team) in teams.iter().enumerate() {
            let names = m
                .world
                .game
                .units
                .values()
                .filter(|u| u.owner == owner)
                .map(|u| m.rules.kinds[u.kind].name.as_str())
                .collect::<Vec<_>>();
            assert!(names.contains(&"hq"));
            assert!(names.contains(&"engineer"));
            assert!(names.contains(&"rifle"));
            assert_eq!(
                m.world_region_at(m.start.players[owner].spawn)
                    .unwrap()
                    .unwrap()
                    .team,
                team
            );
        }
        let local = m.world_actor_view(LOCAL_DEFENDER_OWNER).unwrap();
        assert_eq!(local.team, -1);
        assert_eq!(local.owner, LOCAL_DEFENDER_OWNER);
        assert_eq!(local.faction, Faction::Usa);
        assert_eq!(m.world.game.players.len(), count);
        assert!(
            m.world
                .game
                .allied(LOCAL_DEFENDER_OWNER, LOCAL_DEFENDER_OWNER)
        );
        for owner in 0..count {
            assert!(!m.world.game.allied(owner, LOCAL_DEFENDER_OWNER));
            assert!(!m.world.game.allied(LOCAL_DEFENDER_OWNER, owner));
        }
        let mut guards = 0;
        let mut observed_kinds = std::collections::BTreeSet::new();
        let mut guards_by_region = std::collections::BTreeMap::<usize, Vec<&str>>::new();
        for u in m
            .world
            .game
            .units
            .values()
            .filter(|u| u.owner == LOCAL_DEFENDER_OWNER)
        {
            let kind = &m.rules.kinds[u.kind];
            if !kind.structure {
                guards += 1;
                assert!(u.hold_pos);
                assert!(!m.commands.units[&u.id].auto_retreat);
                let region = m.world_region_index(u.at()).unwrap().unwrap();
                assert!(geography.regions[region].home.is_none());
                let allowed: &[&str] = match geography.regions[region].kind.as_deref() {
                    Some("city") => &["rifle", "mg", "at", "mortar"],
                    Some("industrial") => &["rifle", "at", "armoredcar"],
                    Some("resource") => &["rifle", "mg", "mortar"],
                    _ => &["rifle", "mg"],
                };
                assert!(
                    allowed.contains(&kind.name.as_str()),
                    "unexpected guard {}",
                    kind.name
                );
                guards_by_region
                    .entry(region)
                    .or_default()
                    .push(kind.name.as_str());
            }
        }
        assert!(guards >= 3 * (size.region_count() - count));
        for (i, region) in geography
            .regions
            .iter()
            .enumerate()
            .filter(|(_, r)| r.home.is_none())
        {
            observed_kinds.insert(region.kind.as_deref().unwrap());
            let mut expected = match region.kind.as_deref() {
                Some("city") => vec!["rifle", "rifle", "mg", "at", "mortar"],
                Some("industrial") => vec!["rifle", "at", "armoredcar"],
                Some("resource") => vec!["rifle", "mg", "mortar"],
                _ => vec!["rifle", "rifle", "mg"],
            };
            expected.sort();
            let actual = guards_by_region.get_mut(&i).unwrap();
            actual.sort();
            assert_eq!(actual, &expected, "guard roster in region {i}");
        }
        assert_eq!(
            observed_kinds,
            ["city", "industrial", "resource", "rural"]
                .into_iter()
                .collect()
        );
    }
}
