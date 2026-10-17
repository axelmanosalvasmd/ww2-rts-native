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
    record_at(state, kind, Point { x: 128.0, z: 128.0 });
}

fn record_at(state: &mut Match, kind: &str, at: Point) {
    state
        .enqueue_command(
            OrderKey {
                tick: 1,
                issuer: 0,
                sequence: 1,
            },
            Command::Support {
                kind: kind.into(),
                at,
                direction: Some(0.0),
            },
        )
        .unwrap();
}

#[test]
fn hosted_recorded_plane_support_pays_once_and_delivers_every_seat_through_retirement() {
    use crate::{match_support::SupportEvent, match_tick::TickEvent};

    for kind in ["recon", "strafe", "bombing", "dive", "para"] {
        let mut state = scene();
        let mut baseline = scene();
        for m in [&mut state, &mut baseline] {
            m.restrict_to_ordinary_snapshots().unwrap();
            m.initialize_tick_scene(0, TickPathConfig::default())
                .unwrap();
            for unit in m.world.game.units.values_mut() {
                unit.hold_pos = true;
            }
            for control in m.commands.units.values_mut() {
                control.hold_fire = true;
                control.auto_cast = false;
                control.auto_retreat = false;
            }
        }
        // A drop still requires actual allied sight and spare population.
        let at = if kind == "para" {
            state
                .world
                .game
                .units
                .values()
                .find(|u| u.owner == 0 && !state.rules.kinds[u.kind].structure)
                .unwrap()
                .at()
        } else {
            Point { x: 128.0, z: 128.0 }
        };
        let cost = economy::sup_cost(Mode::Conquest, kind).unwrap().cost;
        let spent = state.story[0].mp_spent;
        let calls = state.story[0].support_calls;
        let mun = state.start.economy.players[0].mun;
        record_at(&mut state, kind, at);
        let selected: Vec<_> = (0..2)
            .map(|slot| SelectedSeat {
                slot,
                full: true,
                omit: false,
            })
            .collect();
        let mut delivery = DeliveryState::new(&state).unwrap();
        let mut arrivals = 0;
        let mut delivered_arrivals = [0; 2];
        let mut retired_ticks = 0;
        for tick in 1..=500 {
            baseline.advance_tick().unwrap();
            let report = state.advance_tick().unwrap();
            if tick == 1 {
                assert_eq!(report.commands.len(), 1, "{kind}");
                assert_eq!(report.commands[0].denial, None, "{kind}");
                assert_eq!(state.world.game.strikes.len(), 1, "{kind}");
                assert_eq!(
                    state.world.game.strikes[0]
                        .runtime
                        .as_ref()
                        .unwrap()
                        .kind
                        .name(),
                    kind
                );
                assert!(state.start.players[0].support[kind] > 0.0);
                state
                    .enqueue_command(
                        OrderKey {
                            tick: 2,
                            issuer: 0,
                            sequence: 2,
                        },
                        Command::Support {
                            kind: kind.into(),
                            at,
                            direction: Some(0.0),
                        },
                    )
                    .unwrap();
            } else if tick == 2 {
                assert_eq!(report.commands.len(), 1);
                assert_eq!(report.commands[0].denial, Some(Denial::Cooldown), "{kind}");
            } else {
                assert!(report.commands.is_empty());
            }
            assert_eq!(
                state.start.economy.players[0].mp,
                baseline.start.economy.players[0].mp - cost,
                "{kind}, tick {tick}"
            );
            assert_eq!(state.start.economy.players[0].mun, mun);
            assert_eq!(state.story[0].mp_spent, spent + cost);
            assert_eq!(state.story[0].support_calls, calls + 1.0);
            arrivals += report.events.iter().filter(|event| {
                matches!(event, TickEvent::Support(SupportEvent::Plane { kind: actual, .. }) if actual.name() == kind)
            }).count();
            let active = !state.world.game.strikes.is_empty();
            let frame = delivery
                .capture_selected_with_tick_events(&mut state, &selected, &report.events)
                .unwrap()
                .unwrap();
            for (slot, count) in delivered_arrivals.iter_mut().enumerate() {
                let snapshot = frame.recipient(slot).unwrap().into_json();
                assert_eq!(snapshot["t"], "s");
                assert!(snapshot["units"].is_array());
                assert!(snapshot["cells"].is_array());
                assert!(snapshot["fog"]["v"].is_string());
                assert!(snapshot["fog"]["e"].is_string());
                assert_eq!(
                    snapshot["strikes"].as_array().unwrap().len(),
                    usize::from(active)
                );
                *count += snapshot["shots"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|shot| shot["k"] == kind && shot["pub"] == true)
                    .count();
            }
            assert!(state.tick_state.fault().is_none());
            if !active {
                retired_ticks += 1;
                if retired_ticks == 5 {
                    break;
                }
            }
        }
        assert_eq!(arrivals, 1, "{kind} announces once");
        assert_eq!(delivered_arrivals, [1; 2], "{kind} reaches both seats once");
        assert_eq!(retired_ticks, 5, "{kind} retires without ending delivery");
        assert_eq!(state.commands.pending_commands(), 0);
    }
}

#[test]
fn hosted_unknown_support_and_unsupported_command_reject_without_payment_or_publication() {
    for command in [
        Command::Support {
            kind: "unknown-support".into(),
            at: Point { x: 128.0, z: 128.0 },
            direction: Some(0.0),
        },
        Command::Unsupported,
    ] {
        let mut state = scene();
        state.restrict_to_ordinary_snapshots().unwrap();
        state
            .enqueue_command(
                OrderKey {
                    tick: 1,
                    issuer: 0,
                    sequence: 1,
                },
                command,
            )
            .unwrap();
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
            "rejection must leave paid and published state unchanged"
        );
        assert_eq!(state.commands.pending_commands(), 0);
        state.preflight_ordinary_snapshot().unwrap();
    }
}

#[test]
fn unrestricted_recorded_recon_retains_independent_component_admission() {
    let mut state = scene();
    let mp = state.start.economy.players[0].mp;
    let cost = economy::sup_cost(Mode::Conquest, "recon").unwrap().cost;
    let calls = state.story[0].support_calls;
    record(&mut state, "recon");
    assert_eq!(state.apply_commands(1, false).unwrap()[0].denial, None);
    assert_eq!(state.start.economy.players[0].mp, mp - cost);
    assert_eq!(state.story[0].support_calls, calls + 1.0);
    assert_eq!(
        state.world.game.strikes[0].runtime.as_ref().unwrap().kind,
        crate::match_support::SupportKind::Recon
    );
    assert_eq!(
        state
            .execute_command(
                0,
                &Command::Support {
                    kind: "unknown-support".into(),
                    at: Point { x: 128.0, z: 128.0 },
                    direction: Some(0.0),
                },
                false
            )
            .unwrap(),
        Some(Denial::Blocked)
    );
    assert_eq!(state.start.economy.players[0].mp, mp - cost);
    assert_eq!(state.story[0].support_calls, calls + 1.0);
    assert_eq!(state.world.game.strikes.len(), 1);
}

#[test]
fn hosted_recorded_artillery_still_pays_once_and_delivers_every_seat() {
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
    let cost = economy::sup_cost(Mode::Conquest, "artillery").unwrap().cost;
    let spent = state.story[0].mp_spent;
    let calls = state.story[0].support_calls;
    record(&mut state, "artillery");
    let selected = (0..2)
        .map(|slot| SelectedSeat {
            slot,
            full: false,
            omit: false,
        })
        .collect::<Vec<_>>();
    let mut delivery = DeliveryState::new(&state).unwrap();
    for tick in 1..=8 {
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
            assert_eq!(frame.recipient(slot).unwrap().into_json()["t"], "s");
        }
    }
    assert_eq!(state.story[0].support_calls, calls + 1.0);
}
