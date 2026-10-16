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
                at: Point { x: 128.0, z: 128.0 },
                direction: Some(0.0),
            },
        )
        .unwrap();
}

#[test]
fn hosted_recorded_support_rejects_undeliverable_effects_before_any_payment_or_publication() {
    for kind in ["recon", "strafe", "bombing", "dive", "para"] {
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
        assert_eq!(results[0].denial, Some(Denial::Unsupported), "{kind}");
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
            "rejection must leave paid and published state unchanged: {kind}"
        );
        assert_eq!(state.commands.pending_commands(), 0);
        state.preflight_ordinary_snapshot().unwrap();
    }

    let mut state = scene();
    // The capability binding belongs to hosted matches, not the unrestricted component API.
    record(&mut state, "recon");
    assert_eq!(state.apply_commands(1, false).unwrap()[0].denial, None);
    assert_eq!(
        state.world.game.strikes[0].runtime.as_ref().unwrap().kind,
        crate::match_support::SupportKind::Recon
    );
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
