use super::*;
use crate::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::ModeKind,
    map::AuthoritativeMap,
    match_end::{EndReason, MatchSample},
    match_runtime::{MatchOptions, PlayerConfig},
    weather::WeatherChoice,
};
use serde_json::json;

fn scene() -> Match {
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"Story intent", "w":60,"h":60,"rows":vec![".".repeat(60);60],
            "spawns":[{"x":5,"y":5},{"x":54,"y":54},{"x":5,"y":54}],
            "points":[{"x":30,"y":30}]
        }))
        .unwrap(),
    )
    .unwrap();
    Match::new(
        &map,
        MatchOptions {
            seed: 701,
            mode: Mode::Conquest,
            players: [0, 0, 2]
                .into_iter()
                .enumerate()
                .map(|(i, team)| PlayerConfig {
                    name: format!("P{i}"),
                    team,
                    faction: Faction::Usa,
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
    .unwrap()
}

#[test]
fn seven_story_counters_attribute_real_slots_and_stop_after_finish() {
    let mut m = scene();
    // The death callback receives the actual cleanup owner's enemy attribution.
    m.record_death(
        DeathRecord {
            owner: Some(2),
            kind: "rifle",
            at: Point { x: 20.0, z: 20.0 },
        },
        Some(0),
    )
    .unwrap();
    m.record_death(
        DeathRecord {
            owner: Some(1),
            kind: "rifle",
            at: Point { x: 21.0, z: 20.0 },
        },
        Some(0),
    )
    .unwrap();
    m.record_death(
        DeathRecord {
            owner: None,
            kind: "rifle",
            at: Point { x: 22.0, z: 20.0 },
        },
        None,
    )
    .unwrap();
    m.record_built(0).unwrap();
    m.record_spending(0, 120.5).unwrap();
    m.record_spending(0, -20.5).unwrap();
    m.record_support_call(0, Some(100.0)).unwrap();
    m.record_support_call(1, None).unwrap();
    m.record_plane_downed(Some(0)).unwrap();
    m.record_plane_downed(None).unwrap();
    for key in StoryKey::ALL {
        m.record_story_tally(None, key, 1000.0).unwrap();
    }
    // Capture uses the canonical point actuator, including its capture timestamp.
    let capturer = m
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 0 && !m.rules.kinds[u.kind].structure)
        .unwrap()
        .id;
    for unit in m.world.game.units.values_mut() {
        unit.x = if unit.id == capturer { 60.0 } else { 10.0 };
        unit.z = if unit.id == capturer { 60.0 } else { 10.0 };
    }
    m.start.points[0].capper = Some(0);
    m.start.points[0].progress = 0.999999;
    m.world.game.tick = 1;
    let order = m.world.game.units.keys().copied().collect::<Vec<_>>();
    let captured = m.apply_conquest_point_economy(&order).unwrap();
    assert_eq!(captured.len(), 1);
    assert_eq!(m.capture_ticks[0], Some(1));
    assert_eq!(
        m.story[0],
        PlayerStory {
            kills: 1.0,
            losses: 0.0,
            built: 1.0,
            captures: 1.0,
            mp_spent: 200.0,
            support_calls: 1.0,
            planes_downed: 1.0
        }
    );
    assert_eq!(
        m.story[1],
        PlayerStory {
            losses: 1.0,
            support_calls: 1.0,
            ..PlayerStory::default()
        }
    );
    assert_eq!(
        m.story[2],
        PlayerStory {
            losses: 1.0,
            ..PlayerStory::default()
        }
    );
    let before = m.story.clone();
    assert!(m.record_story_tally(Some(3), StoryKey::Kills, 1.0).is_err());
    assert_eq!(m.story, before);
    m.start.economy.players[0].vp = m.start.win_vp;
    m.apply_conquest_victory().unwrap().unwrap();
    m.record_built(0).unwrap();
    m.record_spending(0, 50.0).unwrap();
    m.record_support_call(0, Some(50.0)).unwrap();
    m.record_plane_downed(Some(0)).unwrap();
    m.record_death(
        DeathRecord {
            owner: Some(2),
            kind: "rifle",
            at: Point { x: 30.0, z: 30.0 },
        },
        Some(0),
    )
    .unwrap();
    for key in StoryKey::ALL {
        m.record_story_tally(Some(0), key, 50.0).unwrap();
    }
    assert_eq!(m.story, before);
    assert_eq!(m.match_result().unwrap().unwrap().story, before);
}

#[test]
fn fallen_locations_remain_separate_and_explicit_finish_spots_override_the_centre() {
    for (reason, expected) in [
        (EndReason::Structures, Point { x: 20.0, z: 30.0 }),
        (EndReason::Bunkers, Point { x: 20.0, z: 30.0 }),
        (EndReason::Headquarters, Point { x: 30.0, z: 40.0 }),
        (EndReason::Timer, Point { x: 60.0, z: 60.0 }),
    ] {
        let mut m = scene();
        for (kind, at) in [
            ("hq", Point { x: 10.0, z: 20.0 }),
            ("bunker", Point { x: 20.0, z: 30.0 }),
            ("barracks", Point { x: 30.0, z: 40.0 }),
            ("rifle", Point { x: 90.0, z: 90.0 }),
        ] {
            m.record_death(
                DeathRecord {
                    owner: Some(2),
                    kind,
                    at,
                },
                Some(0),
            )
            .unwrap();
        }
        assert_eq!(m.fallen.structure, Some(Point { x: 30.0, z: 40.0 }));
        assert_eq!(m.fallen.bunker, Some(Point { x: 20.0, z: 30.0 }));
        assert_eq!(m.fallen.hq, Some(Point { x: 30.0, z: 40.0 }));
        let at = match reason {
            EndReason::Headquarters => m.fallen.hq,
            EndReason::Bunkers | EndReason::Structures => m.fallen.bunker,
            _ => None,
        };
        let f = m.initialize_tick_scene_finish(0, reason, at).unwrap();
        assert_eq!(f.end_at, expected);
        let fallen = m.fallen;
        m.record_death(
            DeathRecord {
                owner: Some(2),
                kind: "hq",
                at: Point { x: 99.0, z: 99.0 },
            },
            Some(0),
        )
        .unwrap();
        assert_eq!(m.fallen, fallen);
        assert_eq!(m.match_result().unwrap().unwrap().at, expected);
    }
    let mut m = scene();
    m.fallen.bunker = Some(Point { x: 20.0, z: 30.0 });
    assert_eq!(
        m.initialize_tick_scene_finish(0, EndReason::Bunkers, Some(Point { x: 9.25, z: 8.75 }))
            .unwrap()
            .end_at,
        Point { x: 9.3, z: 8.8 }
    );
    let mut m = scene();
    m.fallen.structure = Some(Point { x: 30.0, z: 40.0 });
    assert_eq!(
        m.initialize_tick_scene_finish(0, EndReason::Structures, None)
            .unwrap()
            .end_at,
        Point { x: 60.0, z: 60.0 }
    );
}

#[test]
fn story_sampler_counts_classic_buildings_and_other_mode_bunkers() {
    let mut m = scene();
    m.start.points[0].owner = Some(1);
    let bunker = m.rules.kind("bunker").unwrap();
    let hq = m.rules.kind("hq").unwrap();
    let rifle = m.rules.kind("rifle").unwrap();
    let facts = [
        (0, bunker, 100.0),
        (1, hq, 100.0),
        (2, bunker, 0.0),
        (2, hq, 100.0),
        (2, rifle, 100.0),
    ];
    m.world.game.tick = 200;
    for (mode, tag, name, expected) in [
        (
            Mode::Classic,
            ModeKind::Classic,
            "classic",
            vec![2.0, 0.0, 1.0],
        ),
        (
            Mode::Annihilation,
            ModeKind::Annihilation,
            "annihilation",
            vec![1.0, 0.0, 0.0],
        ),
        (Mode::Horde, ModeKind::Horde, "horde", vec![1.0, 0.0, 0.0]),
        (
            Mode::Tutorial,
            ModeKind::Tutorial,
            "tutorial",
            vec![1.0, 0.0, 0.0],
        ),
        (Mode::World, ModeKind::World, "world", vec![2.0, 0.0, 1.0]),
    ] {
        m.options.mode = mode;
        m.world.game.mode = Some(tag);
        m.world.game.tick = 200;
        let sample = m.match_sample_with_units(facts).unwrap();
        assert_eq!(
            sample,
            MatchSample::Territories {
                t: 10.0,
                points: vec![1.0, 0.0, 0.0],
                structures: expected
            }
        );
        if mode == Mode::World {
            continue;
        }
        m.timeline.clear();
        for tick in [1, 199, 200, 201, 400] {
            m.world.game.tick = tick;
            assert_eq!(
                m.sample_match_tick().unwrap(),
                matches!(tick, 1 | 200 | 400)
            );
        }
        let prepared = m
            .prepare_match_finish(0, EndReason::Structures, None)
            .unwrap();
        m.publish_match_finish(prepared);
        assert_eq!(m.timeline.len(), 4);
        m.world.game.tick = 600;
        assert!(!m.sample_match_tick().unwrap());
        assert_eq!(m.timeline.len(), 4);
        let report = m.match_result().unwrap().unwrap();
        assert_eq!(report.mode, name);
        assert_eq!(report.win_vp, None);
        m.finish = None;
    }
}
