//! Shared-enemy startup checks against createGame at reference e4e579a.
use serde_json::json;
use ww2_core::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::Point,
    map::AuthoritativeMap,
    match_end::EndReason,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    recipient_snapshot::{DeliveryState, SelectedSeat},
    weather::WeatherChoice,
};

fn map(defend: &[usize], assault_first: bool) -> AuthoritativeMap {
    AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"Mode startup", "w":80, "h":80,
            "rows":vec![".".repeat(80);80],
            "spawns":[
                {"x":10,"y":10,"assault":assault_first},
                {"x":69,"y":69,"assault":assault_first},
                {"x":10,"y":69}, {"x":69,"y":10},
                {"x":40,"y":10}, {"x":40,"y":69}
            ],
            "defend":defend,
            "points":[{"x":30,"y":40},{"x":50,"y":40}]
        }))
        .unwrap(),
    )
    .unwrap()
}

fn options(mode: Mode, factions: &[Faction], shuffle: bool) -> MatchOptions {
    MatchOptions {
        seed: 0x4d04,
        mode,
        players: factions
            .iter()
            .enumerate()
            .map(|(slot, &faction)| PlayerConfig {
                name: format!("Player {slot}"),
                team: (slot as i32 + 2) % 6,
                faction,
            })
            .collect(),
        shuffle,
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

fn construct(map: &AuthoritativeMap, options: MatchOptions) -> Match {
    match options.mode {
        Mode::Horde => Match::new_horde(map, options).unwrap(),
        Mode::Tutorial => Match::new_tutorial(map, options).unwrap(),
        _ => unreachable!(),
    }
}

#[test]
fn mode16_shared_enemy_modes_use_authored_spawn_indices_without_assault_remapping() {
    // createGame: Horde uses defend[len >> 1] and the first non-defend index.
    // Tutorial uses authored indices 0 and 1, even when they are Assault-only.
    let authored = [
        Point { x: 21.0, z: 21.0 },
        Point { x: 139.0, z: 139.0 },
        Point { x: 21.0, z: 139.0 },
        Point { x: 139.0, z: 21.0 },
        Point { x: 81.0, z: 21.0 },
        Point { x: 81.0, z: 139.0 },
    ];
    for (defend, home, gate) in [
        (vec![4], 4, 0),
        (vec![2, 0], 0, 1),
        (vec![4, 0, 2], 0, 1),
        (vec![0, 1, 4, 2], 4, 3),
    ] {
        for shuffle in [false, true] {
            for count in [1, 5] {
                for mode in [Mode::Horde, Mode::Tutorial] {
                    let m = construct(
                        &map(&defend, true),
                        options(mode, &vec![Faction::Usa; count], shuffle),
                    );
                    let (home, gate) = if mode == Mode::Tutorial {
                        (0, 1)
                    } else {
                        (home, gate)
                    };
                    assert_eq!(m.start.players.len(), count + 1);
                    for player in &m.start.players[..count] {
                        assert_eq!(player.spawn, authored[home], "{mode:?}, {defend:?}");
                    }
                    assert_eq!(m.start.players[count].spawn, authored[gate]);
                }
            }
        }
    }
}

#[test]
fn mode17_shared_enemy_rosters_choose_first_unused_faction_then_germany() {
    // Exact priority from createGame: Germany, USSR, USA, then Germany.
    for (picked, expected) in [
        (vec![Faction::Usa], Faction::Germany),
        (vec![Faction::Germany], Faction::Ussr),
        (vec![Faction::Ussr], Faction::Germany),
        (vec![Faction::Uk], Faction::Germany),
        (vec![Faction::Germany, Faction::Ussr], Faction::Usa),
        (vec![Faction::Usa, Faction::Germany], Faction::Ussr),
        (vec![Faction::Ussr, Faction::Usa], Faction::Germany),
        (
            vec![
                Faction::Uk,
                Faction::Usa,
                Faction::Germany,
                Faction::Ussr,
                Faction::Usa,
            ],
            Faction::Germany,
        ),
    ] {
        for mode in [Mode::Horde, Mode::Tutorial] {
            let input = options(mode, &picked, true);
            let original = input.players.clone();
            let m = construct(&map(&[2, 0, 4], false), input);
            let enemy = picked.len();
            let name = if mode == Mode::Horde {
                "Horde"
            } else {
                "Enemy"
            };
            assert_eq!(m.options.players.len(), enemy + 1);
            assert_eq!(m.start.players.len(), enemy + 1);
            assert_eq!(m.world.game.players.len(), enemy + 1);
            assert_eq!(m.start.economy.players.len(), enemy + 1);
            for slot in 0..=enemy {
                let config = &m.options.players[slot];
                let started = &m.start.players[slot].config;
                let team = i32::from(slot == enemy);
                assert_eq!(config.team, team);
                assert_eq!(started.team, team);
                assert_eq!(m.world.game.players[slot].slot, slot);
                assert_eq!(m.world.game.players[slot].team, team);
                assert_eq!(m.start.economy.players[slot].team, team);
                if slot == enemy {
                    assert_eq!(config.name, name);
                    assert_eq!(started.name, name);
                    assert_eq!(config.faction, expected);
                    assert_eq!(started.faction, expected);
                } else {
                    assert_eq!(config.name, original[slot].name);
                    assert_eq!(started.name, original[slot].name);
                    assert_eq!(config.faction, picked[slot]);
                    assert_eq!(started.faction, picked[slot]);
                }
            }
        }
    }
}

#[test]
fn ordinary_spawn_filtering_still_ticks_to_victory_and_delivers_every_seat() {
    let mut m = Match::new(
        &map(&[2], true),
        options(Mode::Conquest, &[Faction::Usa, Faction::Germany], false),
    )
    .unwrap();
    for player in &m.start.players {
        assert!(
            ![Point { x: 21.0, z: 21.0 }, Point { x: 139.0, z: 139.0 },].contains(&player.spawn)
        );
    }
    // A small fixture goal exercises the ordinary victory phase after two sample beats.
    m.start.win_vp = 1.0;
    m.start.points[0].owner = Some(0);
    m.start.points[0].vp = 0.05;
    let seats = (0..m.world.game.players.len())
        .map(|slot| SelectedSeat {
            slot,
            full: false,
            omit: false,
        })
        .collect::<Vec<_>>();
    let mut delivery = DeliveryState::new(&m).unwrap();
    for _ in 0..410 {
        let report = m.advance_tick().unwrap();
        let frame = delivery
            .capture_selected_with_tick_events(&mut m, &seats, &report.events)
            .unwrap()
            .unwrap();
        for seat in &seats {
            assert_eq!(frame.recipient(seat.slot).unwrap().into_json()["t"], "s");
        }
        if m.finish.is_some() {
            break;
        }
    }
    let finish = m
        .finish
        .expect("ordinary point income must finish the fixture");
    assert_eq!(finish.reason, EndReason::VictoryPoints);
    assert_eq!(finish.winner, 2);
    assert!((400..=410).contains(&finish.end_tick));
    assert!(m.tick_state.fault().is_none());
    let result = m.conquest_result().unwrap().unwrap();
    assert_eq!(result.end_tick, finish.end_tick);
    assert_eq!(result.timeline, m.timeline);
}
