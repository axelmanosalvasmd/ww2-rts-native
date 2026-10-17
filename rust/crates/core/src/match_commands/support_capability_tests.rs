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
        // The shared wire projector canonicalizes numeric zero to integer JSON zero.
        let expected_arrival = json!({"k":kind,"x":at.x,"z":at.z,"dir":0,"fo":0,"pub":true});
        record(&mut state, kind);
        let selected = (0..2)
            .map(|slot| SelectedSeat {
                slot,
                full: false,
                omit: false,
            })
            .collect::<Vec<_>>();
        let mut delivery = DeliveryState::new(&state).unwrap();
        let mut arrived = [0_usize; 2];
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
            for (slot, arrived_at_seat) in arrived.iter_mut().enumerate() {
                let row = frame.recipient(slot).unwrap().into_json();
                assert_eq!(row["t"], "s");
                assert!(
                    row["strikes"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .all(|strike| strike[0] == kind && strike[5] == 0)
                );
                for arrival in row["shots"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|shot| shot["k"] == kind)
                {
                    assert_eq!(arrival, &expected_arrival, "{kind} arrival at seat {slot}");
                    *arrived_at_seat += 1;
                }
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
                arrived.into_iter().all(|value| value == 1),
                "{kind} arrival must reach both seats exactly once"
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
fn hosted_recorded_plane_support_pays_once_and_keeps_every_seat_deliverable() {
    for kind in ["recon", "strafe", "bombing", "dive", "para"] {
        let mut state = scene();
        state.world.game.reveal = true;
        state.restrict_to_ordinary_snapshots().unwrap();
        state
            .initialize_tick_scene(0, TickPathConfig::default())
            .unwrap();
        let cost = economy::sup_cost(Mode::Conquest, kind).unwrap().cost;
        let spent = state.story[0].mp_spent;
        let calls = state.story[0].support_calls;
        let at = state.start.players[0].spawn;
        record_at(&mut state, kind, at);
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
                assert_eq!(report.commands[0].denial, None, "{kind}");
                assert_eq!(state.world.game.strikes.len(), 1);
                assert_eq!(
                    state.world.game.strikes[0]
                        .runtime
                        .as_ref()
                        .unwrap()
                        .kind
                        .name(),
                    kind
                );
            }
            assert_eq!(state.story[0].mp_spent, spent + cost);
            assert_eq!(state.story[0].support_calls, calls + 1.0);
            assert_eq!(state.commands.pending_commands(), 0);
            let frame = delivery
                .capture_selected_with_tick_events(&mut state, &selected, &report.events)
                .unwrap()
                .unwrap();
            for slot in 0..2 {
                let snapshot = frame.recipient(slot).unwrap().into_json();
                assert_eq!(snapshot["t"], "s");
                assert_eq!(snapshot["strikes"][0][0], kind);
            }
            assert!(state.tick_state.fault().is_none());
        }
    }
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
    record_at(&mut state, "artillery", Point { x: 128.0, z: 128.0 });
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

#[test]
fn hosted_recorded_plane_support_pays_once_and_delivers_every_seat_through_retirement() {
    for kind in ["recon", "strafe", "bombing", "dive", "para"] {
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
        // Para requires a visible landing site. The owner's spawn is visible.
        let at = state.start.players[0].spawn;
        record_at(&mut state, kind, at);
        let spent = state.story[0].mp_spent;
        let calls = state.story[0].support_calls;
        let next_id = state.next_unit_id;
        let price = economy::sup_cost(Mode::Conquest, kind).unwrap();
        assert_eq!(price.currency, economy::Currency::Manpower);
        let definition = crate::catalog::Catalog::compiled()
            .unwrap()
            .support(kind)
            .unwrap();
        let cooldown = definition.get("cd").unwrap().number().unwrap();
        let delay = definition.get("delay").unwrap().number().unwrap();
        let mut first_report = Some(state.advance_tick().unwrap());
        let results = &first_report.as_ref().unwrap().commands;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].denial, None, "{kind}");
        assert_eq!(state.commands.pending_commands(), 0);
        assert_eq!(
            state.start.economy.players[0].mp,
            baseline.start.economy.players[0].mp - price.cost
        );
        assert_eq!(
            state.start.economy.players[0].mun,
            baseline.start.economy.players[0].mun
        );
        // Commands publish before the same tick's strike and cooldown phases.
        let dt = state.rules.movement.tick;
        assert!(
            state
                .start
                .points
                .iter()
                .all(|point| point.kind.as_deref() != Some("radio"))
        );
        assert_eq!(state.start.players[0].support[kind], cooldown - dt);
        assert_eq!(state.story[0].mp_spent, spent + price.cost);
        assert_eq!(state.story[0].support_calls, calls + 1.0);
        assert_eq!(state.next_unit_id, next_id);
        assert_eq!(state.world.game.strikes.len(), 1);
        let strike = &state.world.game.strikes[0];
        assert_eq!(
            (strike.owner, strike.x, strike.z, strike.dir, strike.live),
            (0, at.x, at.z, 0.0, false)
        );
        assert_eq!(strike.runtime.as_ref().unwrap().kind.name(), kind);
        assert_eq!(strike.runtime.as_ref().unwrap().delay, delay - dt);
        let paid = (
            state.start.economy.clone(),
            state.start.players[0].support.clone(),
            state.world.game.strikes.clone(),
            state.support.clone(),
            state.story.clone(),
            state.start.random.clone(),
            state.next_unit_id,
        );
        assert_eq!(
            state
                .execute_command(
                    0,
                    &Command::Support {
                        kind: kind.into(),
                        at,
                        direction: Some(0.0),
                    },
                    false
                )
                .unwrap(),
            Some(Denial::Cooldown)
        );
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
            paid,
            "cooldown rejection leaves paid and published state unchanged: {kind}"
        );
        state.preflight_ordinary_snapshot().unwrap();
        let selected = (0..2)
            .map(|slot| SelectedSeat {
                slot,
                full: false,
                omit: false,
            })
            .collect::<Vec<_>>();
        let mut delivery = DeliveryState::new(&state).unwrap();
        let mut arrived = false;
        for tick in 1..=2000 {
            let report = if tick == 1 {
                first_report.take().unwrap()
            } else {
                let report = state.advance_tick().unwrap();
                assert!(report.commands.is_empty());
                report
            };
            let plane = report.events.iter().any(|event| {
                matches!(event,
                crate::match_tick::TickEvent::Support(crate::match_support::SupportEvent::Plane {
                    kind: observed, ..
                }) if observed.name() == kind)
            });
            arrived |= plane;
            let retired = state.world.game.strikes.is_empty();
            let frame = delivery
                .capture_selected_with_tick_events(&mut state, &selected, &report.events)
                .unwrap()
                .unwrap();
            for slot in 0..2 {
                let snapshot = frame.recipient(slot).unwrap().into_json();
                assert_eq!(snapshot["t"], "s");
                if plane {
                    let arrivals = snapshot["shots"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .filter(|shot| shot["k"] == kind)
                        .cloned()
                        .collect::<Vec<_>>();
                    assert_eq!(
                        arrivals,
                        vec![json!({
                            "k":kind,"x":at.x,"z":at.z,"dir":0,"fo":0,"pub":true
                        })]
                    );
                }
            }
            if retired {
                break;
            }
        }
        assert!(arrived, "{kind} must arrive through ordinary tick events");
        assert!(state.world.game.strikes.is_empty(), "{kind} must retire");
        assert_eq!(state.story[0].support_calls, calls + 1.0);
        assert_eq!(state.story[0].mp_spent, spent + price.cost);
        assert!(state.tick_state.fault().is_none());
    }

    let mut state = scene();
    // Unrestricted component admission keeps the same canonical publication.
    record_at(&mut state, "recon");
    assert_eq!(state.apply_commands(1, false).unwrap()[0].denial, None);
    assert_eq!(
        state.world.game.strikes[0].runtime.as_ref().unwrap().kind,
        crate::match_support::SupportKind::Recon
    );
}
