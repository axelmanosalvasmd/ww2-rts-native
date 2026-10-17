//! Recorded support orders must remain deliverable before creating paid effects.
use super::{Command, Denial};
use crate::{
    economy::{self, Faction, Mode},
    economy_state::ArmySize,
    game::Point,
    map::AuthoritativeMap,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    match_tick::TickPathConfig,
    orders::OrderKey,
    recipient_snapshot::{DeliveryState, SelectedSeat},
    weather::WeatherChoice,
};
use serde_json::json;

fn scene() -> Match {
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"Recorded support", "w":128,"h":128,"rows":vec![".".repeat(128);128],
            "heights":vec!["0".repeat(128);128],
            "spawns":[{"x":8,"y":8},{"x":112,"y":112}],
            "points":[{"id":"neutral","x":64,"y":64,"mp":0,"vp":0}]
        }))
        .unwrap(),
    )
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
            map_key: None,
            tech: false,
            supply: false,
            logistics: false,
            sandbox: false,
        },
    )
    .unwrap();
    state.start.economy.players[0].mp = 10_000.0;
    state.start.economy.players[0].mun = 10_000.0;
    for cooldown in state.start.players[0].support.values_mut() {
        *cooldown = 0.0;
    }
    state
}

fn record(state: &mut Match, kind: &str) {
    state
        .enqueue_command(
            OrderKey {
                tick: 1,
                issuer: 0,
                sequence: 1,
            },
            Command::Support {
                kind: kind.into(),
                at: if kind == "para" {
                    state
                        .world
                        .game
                        .units
                        .values()
                        .find(|unit| unit.owner == 0)
                        .unwrap()
                        .at()
                } else {
                    Point { x: 128.0, z: 128.0 }
                },
                direction: Some(0.0),
            },
        )
        .unwrap();
}

#[test]
fn hosted_recorded_plane_support_pays_once_and_invalid_kind_preserves_all_state() {
    for kind in [
        "recon",
        "strafe",
        "bombing",
        "dive",
        "para",
        "invalid-support",
    ] {
        let mut state = scene();
        state.restrict_to_ordinary_snapshots().unwrap();
        record(&mut state, kind);
        let before = (
            state.start.economy.clone(),
            state.start.players[0].support.clone(),
            state.world.game.strikes.clone(),
            state.support.clone(),
            state.story.clone(),
            state.start.random.clone(),
            state.next_unit_id,
        );
        let results = state.apply_commands(1, false).unwrap();
        assert_eq!(results.len(), 1);
        if kind == "invalid-support" {
            assert_eq!(results[0].denial, Some(Denial::Unsupported));
            assert_eq!(
                (
                    state.start.economy.clone(),
                    state.start.players[0].support.clone(),
                    state.world.game.strikes.clone(),
                    state.support.clone(),
                    state.story.clone(),
                    state.start.random.clone(),
                    state.next_unit_id,
                ),
                before,
                "invalid kind must not change paid or published state"
            );
        } else {
            assert_eq!(results[0].denial, None, "{kind}");
            let cost = economy::sup_cost(Mode::Conquest, kind).unwrap().cost;
            let mut expected_economy = before.0.clone();
            expected_economy.players[0].mp -= cost;
            assert_eq!(state.start.economy, expected_economy);
            let definition = crate::catalog::Catalog::compiled()
                .unwrap()
                .support(kind)
                .unwrap();
            let cooldown = definition
                .get("cd")
                .and_then(crate::catalog::Value::number)
                .unwrap();
            let mut expected_cooldowns = before.1.clone();
            expected_cooldowns.insert(kind.into(), cooldown);
            assert_eq!(state.start.players[0].support, expected_cooldowns);
            let mut expected_story = before.4.clone();
            expected_story[0].mp_spent += cost;
            expected_story[0].support_calls += 1.0;
            assert_eq!(state.story, expected_story);
            assert!(before.2.is_empty());
            assert_eq!(state.world.game.strikes.len(), 1);
            let strike = &state.world.game.strikes[0];
            assert_eq!(strike.owner, 0);
            assert!(!strike.live);
            assert_eq!(strike.dir, 0.0);
            let timing = strike.runtime.as_ref().unwrap();
            assert_eq!(timing.kind.name(), kind);
            let delay = definition
                .get("delay")
                .and_then(crate::catalog::Value::number)
                .unwrap();
            assert_eq!(timing.delay, delay);
            assert_eq!(state.support, before.3);
            assert_eq!(state.start.random, before.5);
            assert_eq!(state.next_unit_id, before.6);
            let expected_strikes = json!([[
                kind,
                crate::math::round(strike.x * 10.) / 10.,
                crate::math::round(strike.z * 10.) / 10.,
                0.,
                delay,
                0
            ]]);
            let cache = crate::recipient_snapshot::SnapshotCache::new(&state).unwrap();
            for slot in 0..2 {
                let fields = cache.recipient(slot).unwrap().fields;
                assert_eq!(fields["strikes"], expected_strikes);
                assert_eq!(
                    fields["sup"][kind],
                    json!(state.start.players[slot].support[kind].ceil())
                );
            }
        }
        assert_eq!(state.commands.pending_commands(), 0);
        state.preflight_ordinary_snapshot().unwrap();
    }

    let mut state = scene();
    record(&mut state, "recon");
    assert_eq!(state.apply_commands(1, false).unwrap()[0].denial, None);
    assert_eq!(
        state.world.game.strikes[0].runtime.as_ref().unwrap().kind,
        crate::match_support::SupportKind::Recon
    );
}

#[test]
fn hosted_recorded_support_pays_once_and_delivers_every_seat_through_cleanup_and_ending() {
    for kind in ["recon", "strafe", "bombing", "dive", "para", "artillery"] {
        let mut state = scene();
        state.restrict_to_ordinary_snapshots().unwrap();
        state
            .initialize_tick_scene(0, TickPathConfig::default())
            .unwrap();
        let mut baseline = scene();
        baseline.restrict_to_ordinary_snapshots().unwrap();
        baseline
            .initialize_tick_scene(0, TickPathConfig::default())
            .unwrap();
        baseline.advance_tick().unwrap();
        let cost = economy::sup_cost(Mode::Conquest, kind).unwrap().cost;
        let spent = state.story[0].mp_spent;
        let calls = state.story[0].support_calls;
        let next_unit = state.next_unit_id;
        let at = if kind == "para" {
            state
                .world
                .game
                .units
                .values()
                .find(|unit| unit.owner == 0)
                .unwrap()
                .at()
        } else {
            Point { x: 128., z: 128. }
        };
        let expected_arrival = json!({"k":kind,"x":at.x,"z":at.z,"dir":0.,"fo":0,"pub":true});
        record(&mut state, kind);
        let selected = (0..2)
            .map(|slot| SelectedSeat {
                slot,
                full: false,
                omit: false,
            })
            .collect::<Vec<_>>();
        let mut delivery = DeliveryState::new(&state).unwrap();
        let mut arrived = [false; 2];
        let mut cleaned = 0;
        for tick in 1..=600 {
            let report = state.advance_tick().unwrap();
            if tick == 1 {
                assert_eq!(report.commands.len(), 1);
                assert_eq!(report.commands[0].denial, None);
                assert_eq!(
                    state.start.economy.players[0].mp,
                    baseline.start.economy.players[0].mp - cost
                );
                assert_eq!(state.story[0].mp_spent, spent + cost);
                assert_eq!(state.story[0].support_calls, calls + 1.0);
                assert_eq!(state.world.game.strikes.len(), 1);
            }
            let frame = delivery
                .capture_selected_with_tick_events(&mut state, &selected, &report.events)
                .unwrap()
                .unwrap();
            for slot in 0..2 {
                let row = frame.recipient(slot).unwrap().into_json();
                assert_eq!(row["t"], "s");
                assert!(
                    row["strikes"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .all(|strike| strike[0] == kind && strike[5] == 0)
                );
                arrived[slot] |= row["shots"].as_array().unwrap().contains(&expected_arrival);
            }
            drop(frame);
            assert!(state.tick_state.fault().is_none());
            if state.world.game.strikes.is_empty() {
                cleaned += 1;
                if cleaned == 8 {
                    break;
                }
            }
        }
        assert_eq!(cleaned, 8, "{kind} must naturally retire before ending");
        if kind != "artillery" {
            assert!(
                arrived.into_iter().all(|value| value),
                "{kind} arrival must reach both seats"
            );
        }
        assert_eq!(state.story[0].support_calls, calls + 1.0);
        if kind == "para" {
            assert_eq!(state.next_unit_id, next_unit + 3);
            for (offset, kind) in ["rifle", "rifle", "mg"].into_iter().enumerate() {
                let unit = &state.world.game.units[&(next_unit + offset as u32)];
                assert_eq!(unit.owner, 0);
                assert_eq!(state.rules.kinds[unit.kind].name, kind);
                assert!(state.commands.units.contains_key(&unit.id));
                assert!(state.combat.get(unit.id).is_some());
                assert!(state.firing.get(unit.id).is_some());
                assert!(state.abilities.get(unit.id).is_some());
                assert!(state.recovery.get(unit.id).is_some());
            }
        }
        state.start.economy.players[0].vp = 10_000.0;
        for _ in 0..10 {
            let report = state.advance_tick().unwrap();
            let frame = delivery
                .capture_selected_with_tick_events(&mut state, &selected, &report.events)
                .unwrap()
                .unwrap();
            for slot in 0..2 {
                assert_eq!(frame.recipient(slot).unwrap().into_json()["winner"], 0);
            }
        }
        assert!(state.finish.is_some());
        assert!(state.tick_state.fault().is_none());
    }
}
