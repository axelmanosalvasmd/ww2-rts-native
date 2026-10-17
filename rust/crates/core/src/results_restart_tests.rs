//! Results intent checks against prototype e4e579a, shared/story.js:28-63 and sim.js:5318,5756-5814.
use crate::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::Point,
    map::AuthoritativeMap,
    match_end::{EndReason, MatchSample},
    match_runtime::{Match, MatchOptions, PlayerConfig},
    recipient_snapshot::SnapshotCache,
    weather::WeatherChoice,
};
use serde_json::json;

fn scene(mode: Mode) -> Match {
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"Results intent", "w":100,"h":100,
            "rows":vec![".".repeat(100);100],"heights":vec!["0".repeat(100);100],
            "spawns":[{"x":8,"y":8},{"x":91,"y":91},{"x":8,"y":91},{"x":91,"y":8}],
            "defend":[0,1], "assaultTime":300,
            "points":[{"x":50,"y":50,"mp":2,"vp":1}]
        }))
        .unwrap(),
    )
    .unwrap();
    let options = MatchOptions {
        seed: 701,
        mode,
        players: [0, 0, 2, 2]
            .into_iter()
            .enumerate()
            .map(|(slot, team)| PlayerConfig {
                name: format!("p{slot}"),
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
    };
    match mode {
        Mode::Conquest => Match::new(&map, options),
        Mode::Assault => Match::new_assault(&map, options, 0),
        Mode::Annihilation => Match::new_annihilation(&map, options),
        Mode::Classic => Match::new_classic(&map, options),
        _ => unreachable!(),
    }
    .unwrap()
}

fn actor(m: &Match, owner: usize, kind: &str) -> u32 {
    m.world
        .game
        .units
        .values()
        .find(|u| u.owner == owner && m.rules.kinds[u.kind].name == kind)
        .unwrap()
        .id
}

fn kill(m: &mut Match, id: u32) -> Point {
    let unit = m.world.game.units.get_mut(&id).unwrap();
    unit.hp = 0.0;
    unit.at()
}

#[test]
fn mode92_actual_death_cleanup_selects_last_bunker_or_production_building() {
    // The caller never supplies a finish position or writes the fallen cache.
    // Prototype sim.js:5774,5781,5765 selects the actual retirement callback's position.
    for mode in [Mode::Assault, Mode::Annihilation, Mode::Classic] {
        let mut m = scene(mode);
        let (owners, kind, reason) = match mode {
            Mode::Assault => ([0, 1], "bunker", EndReason::Structures),
            Mode::Annihilation => ([2, 3], "bunker", EndReason::Bunkers),
            Mode::Classic => ([2, 3], "hq", EndReason::Headquarters),
            _ => unreachable!(),
        };
        let first = actor(&m, owners[0], kind);
        let second = actor(&m, owners[1], kind);
        let first_at = kill(&mut m, first);
        m.advance_tick().unwrap();
        assert!(
            m.finish.is_none(),
            "{mode:?}: another production actor survives"
        );
        assert!(!m.world.game.units.contains_key(&first));
        // Classic must wait for a surviving production building, even after its HQ falls.
        let last = if mode == Mode::Classic {
            m.place_starting_building(owners[1], "barracks", 50 * 100 + 70)
                .unwrap();
            kill(&mut m, second);
            m.advance_tick().unwrap();
            assert!(m.finish.is_none());
            actor(&m, owners[1], "barracks")
        } else {
            second
        };
        let last_at = kill(&mut m, last);
        assert_ne!(first_at, last_at);
        m.advance_tick().unwrap();
        let finish = m.finish.unwrap();
        assert_eq!(finish.reason, reason);
        let expected = Point {
            x: crate::math::round(last_at.x * 10.0) / 10.0,
            z: crate::math::round(last_at.z * 10.0) / 10.0,
        };
        assert_eq!(finish.end_at, expected, "{mode:?}");
        assert_eq!(m.match_result().unwrap().unwrap().at, expected);
        assert!(!m.world.game.units.contains_key(&last));
        let timeline = m.timeline.clone();
        for _ in 0..4 {
            m.advance_tick().unwrap();
            assert_eq!(m.finish, Some(finish));
            assert_eq!(m.timeline, timeline);
        }
    }
}

#[test]
fn mode93_conquest_runner_samples_team_floor_at_tick_one_200_and_finish() {
    let mut m = scene(Mode::Conquest);
    for (player, vp) in m.start.economy.players.iter_mut().zip([1.6, 1.7, 2.9, 2.9]) {
        player.vp = vp;
    }
    for tick in 1..=200 {
        m.advance_tick().unwrap();
        assert_eq!(m.world.game.tick, tick);
        assert_eq!(m.timeline.len(), if tick < 200 { 1 } else { 2 });
        let cache = SnapshotCache::new(&m).unwrap();
        for slot in 0..4 {
            let snapshot = cache.recipient(slot).unwrap();
            assert!(!snapshot.fields.contains_key("end"));
            assert_eq!(snapshot.fields["out"], json!([false, false, false, false]));
        }
    }
    assert_eq!(
        m.timeline,
        vec![
            MatchSample::Conquest {
                t: 0.0,
                vp: vec![3.0, 0.0, 5.0]
            },
            MatchSample::Conquest {
                t: 10.0,
                vp: vec![3.0, 0.0, 5.0]
            },
        ]
    );
    m.start.win_vp = 3.0;
    m.advance_tick().unwrap();
    assert_eq!(m.timeline.len(), 3);
    assert_eq!(m.timeline[2], m.timeline[1]);
    let finish = m.finish.unwrap();
    assert_eq!(finish.reason, EndReason::VictoryPoints);
    assert_eq!(finish.end_tick, 201);
    for _ in 0..200 {
        m.advance_tick().unwrap();
        let cache = SnapshotCache::new(&m).unwrap();
        for slot in 0..4 {
            let snapshot = cache.recipient(slot).unwrap();
            assert_eq!(
                snapshot.fields["end"],
                json!({"reason":"vp","x":finish.end_at.x,"z":finish.end_at.z})
            );
        }
    }
    assert_eq!(m.timeline.len(), 3);
}

#[test]
fn mode93_ordinary_runner_counts_held_points_and_mode_specific_living_structures() {
    for mode in [Mode::Assault, Mode::Annihilation, Mode::Classic] {
        let mut m = scene(mode);
        m.start.points[0].owner = Some(1);
        // A building other than a bunker counts in Classic, but not Annihilation.
        m.place_starting_building(0, "flakpos", 50 * 100 + 30)
            .unwrap();
        let structures = match mode {
            Mode::Classic => vec![3.0, 0.0, 2.0],
            Mode::Assault => vec![2.0, 0.0, 0.0],
            _ => vec![2.0, 0.0, 2.0],
        };
        for tick in 1..=200 {
            m.advance_tick().unwrap();
            assert_eq!(m.timeline.len(), if tick < 200 { 1 } else { 2 });
        }
        for (sample, t) in m.timeline.iter().zip([0.0, 10.0]) {
            assert_eq!(
                sample,
                &MatchSample::Territories {
                    t,
                    points: vec![1.0, 0.0, 0.0],
                    structures: structures.clone(),
                }
            );
        }
        let kind = if mode == Mode::Classic {
            "hq"
        } else {
            "bunker"
        };
        let losing = if mode == Mode::Assault {
            [0, 1]
        } else {
            [2, 3]
        };
        for owner in losing {
            let id = actor(&m, owner, kind);
            kill(&mut m, id);
        }
        m.advance_tick().unwrap();
        assert!(m.finish.is_some());
        let mut remaining = structures;
        remaining[if mode == Mode::Assault { 0 } else { 2 }] = 0.0;
        assert_eq!(
            m.timeline.last().unwrap(),
            &MatchSample::Territories {
                t: 10.0,
                points: vec![1.0, 0.0, 0.0],
                structures: remaining,
            }
        );
        assert_eq!(m.timeline.len(), 3);
        m.advance_tick().unwrap();
        assert_eq!(m.timeline.len(), 3);
    }
}
