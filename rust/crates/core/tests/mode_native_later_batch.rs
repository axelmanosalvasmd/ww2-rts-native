//! Later MODE window: native startup and durable state checks.
use ww2_core::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::{LOCAL_DEFENDER_OWNER, ModeKind, Point},
    map::AuthoritativeMap,
    match_end::MatchSample,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    random::MatchRandom,
    weather::WeatherChoice,
    world_generation::{WorldSize, generate_world},
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
fn tutorial() -> Match {
    let map = AuthoritativeMap::from_json(include_bytes!(
        "../../server/src/authoritative-maps/tutorial.json"
    ))
    .unwrap();
    Match::new_tutorial(&map, options(Mode::Tutorial, 1)).unwrap()
}
#[test]
fn mode70_tutorial_map_keeps_both_zero_vp_objectives() {
    let m = tutorial();
    assert_eq!((m.world.game.w, m.world.game.h), (72, 44));
    assert_eq!(m.start.players[0].spawn, Point { x: 11.0, z: 43.0 });
    assert_eq!(m.start.players[1].spawn, Point { x: 137.0, z: 77.0 });
    assert_eq!(m.start.points.len(), 2);
    for (point, at) in m
        .start
        .points
        .iter()
        .zip([Point { x: 69.0, z: 45.0 }, Point { x: 125.0, z: 45.0 }])
    {
        assert_eq!(point.at, at);
        assert_eq!(point.vp, 0.0);
        assert_eq!(point.mp, 2.0);
        assert_eq!(point.owner, None);
    }
}
#[test]
fn mode71_73_generated_world_starts_homes_and_unattended_guards() {
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
                assert_eq!(runtime.guard_bases.values().filter(|&&r| r == i).count(), 1);
            }
        }
        for owner in 0..count {
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
                teams[owner]
            );
        }
        let local = m.world_actor_view(LOCAL_DEFENDER_OWNER).unwrap();
        assert_eq!(local.team, -1);
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
    }
}
#[test]
fn mode86_93_reading_durable_state_does_not_step_or_resample() {
    let mut m = tutorial();
    for _ in 0..201 {
        m.advance_tick().unwrap();
    }
    assert_eq!(m.world.game.tick, 201);
    assert_eq!(m.timeline.len(), 2);
    for (sample, expected_t) in m.timeline.iter().zip([0.0, 10.0]) {
        assert_eq!(
            sample,
            &MatchSample::Territories {
                t: expected_t,
                points: vec![0.0, 0.0],
                structures: vec![0.0, 0.0],
            }
        );
    }
    let before = (m.world.game.tick, m.timeline.clone(), m.story.clone());
    for _ in 0..20 {
        assert!(m.match_result().unwrap().is_none());
        assert!(m.tutorial_state().is_some());
    }
    assert_eq!(
        (m.world.game.tick, m.timeline.clone(), m.story.clone()),
        before
    );
}
