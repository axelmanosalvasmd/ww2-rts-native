//! Ordinary R1-shaped matches keep their real constructor actors and full tick/snapshot paths.
use serde_json::json;
use ww2_core::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    map::AuthoritativeMap,
    match_commands::Command,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    match_tick::TickPathConfig,
    recipient_snapshot::DeliveryState,
    weather::WeatherChoice,
};

fn ordinary(supply: bool) -> Match {
    let map = AuthoritativeMap::from_json(include_bytes!(
        "../../server/src/authoritative-maps/el-alamein.json"
    ))
    .unwrap();
    let mut m = Match::new(
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
            weather_seed: Some(217),
            living_weather: false,
            map_key: Some("el-alamein".into()),
            tech: false,
            supply,
            logistics: false,
            sandbox: false,
        },
    )
    .unwrap();
    m.initialize_tick_scene(0, TickPathConfig::default())
        .unwrap();
    m
}

fn snapshots(m: &mut Match) {
    m.preflight_ordinary_snapshot().unwrap();
    let mp: Vec<_> = m
        .start
        .economy
        .players
        .iter()
        .map(|p| p.mp.floor())
        .collect();
    let mut delivery = DeliveryState::new(m).unwrap();
    let frame = delivery
        .capture_with_lists(m, &[true; 2], &[false; 2], &[])
        .unwrap();
    for (slot, expected) in mp.into_iter().enumerate() {
        let body = frame.recipient(slot).unwrap().into_json();
        assert_eq!(body["t"], "s");
        assert!(body["units"].is_array());
        assert_eq!(body["mp"], json!(expected));
        assert!(!serde_json::to_vec(&body).unwrap().is_empty());
    }
}

#[test]
fn base09_ordinary_retirement_pays_once_and_scuttle_keeps_ticking_and_snapshotting() {
    for supply in [false, true] {
        for victim_kind in ["rifle", "barracks"] {
            // Paired real matches isolate the bounty from ordinary income.
            let mut killed = ordinary(supply);
            let mut scuttled = ordinary(supply);
            snapshots(&mut killed);
            snapshots(&mut scuttled);
            let id = killed
                .world
                .game
                .units
                .values()
                .find(|u| u.owner == 1 && killed.rules.kinds[u.kind].name == victim_kind)
                .unwrap()
                .id;
            killed.world.game.units.get_mut(&id).unwrap().hp = 0.;
            killed.combat.initialize(id).last_hit = Some(0);
            assert_eq!(
                scuttled
                    .execute_command(
                        1,
                        &Command::Destroy {
                            ids: vec![],
                            id: Some(i64::from(id))
                        },
                        false
                    )
                    .unwrap(),
                None
            );
            for m in [&mut killed, &mut scuttled] {
                let report = m.advance_tick().unwrap();
                assert_eq!(
                    report
                        .retired
                        .iter()
                        .filter(|row| row.unit.id == id)
                        .count(),
                    1
                );
                assert!(!m.world.game.units.contains_key(&id));
                snapshots(m);
            }
            let expected = if victim_kind == "rifle" { 20. } else { 30. };
            assert!(
                (killed.start.economy.players[0].mp
                    - scuttled.start.economy.players[0].mp
                    - expected)
                    .abs()
                    < 1e-10
            );
            assert_eq!(
                killed.start.economy.players[1].mp,
                scuttled.start.economy.players[1].mp
            );
            for _ in 0..82 {
                killed.advance_tick().unwrap();
                scuttled.advance_tick().unwrap();
                snapshots(&mut killed);
                snapshots(&mut scuttled);
            }
            assert!(killed.tick_state.fault().is_none());
            assert!(scuttled.tick_state.fault().is_none());
            assert!(
                (killed.start.economy.players[0].mp
                    - scuttled.start.economy.players[0].mp
                    - expected)
                    .abs()
                    < 1e-10
            );
        }
    }
}

#[test]
fn base05_ordinary_point_cut_changes_only_on_supply_beats_and_suppresses_income() {
    for supply in [false, true] {
        let mut m = ordinary(supply);
        // A held point has no local enemy. Changing only its cut scalar tests
        // the scheduling boundary without replacing constructor terrain or actors.
        m.start.points[0].owner = Some(0);
        m.start.points[0].cut = true;
        m.advance_tick().unwrap();
        assert_eq!(m.start.points[0].cut, !supply);
        snapshots(&mut m);
        m.start.points[0].cut = true;
        for tick in 2..=40 {
            assert_eq!(m.advance_tick().unwrap().tick, tick);
            assert!(m.start.points[0].cut, "tick {tick}");
            snapshots(&mut m);
        }
        let before = m.start.economy.players[0].mp;
        m.advance_tick().unwrap();
        assert_eq!(m.start.points[0].cut, !supply);
        let inc = m.start.economy.players[0].inc;
        assert!(
            (m.start.economy.players[0].mp - before - inc * m.rules.movement.tick).abs() < 1e-10
        );
        assert_eq!(
            inc,
            if supply {
                4. + m.start.points[0].mp
            } else {
                4.
            }
        );
        snapshots(&mut m);
    }
}
