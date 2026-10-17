//! Bounded retreat and support checks, including an explicit Classic core constructor.
use crate::{
    catalog::{Catalog, Value},
    economy::{self, Currency, Faction, Mode},
    economy_state::ArmySize,
    game::Point,
    map::AuthoritativeMap,
    match_commands::{Command, Denial, MoveOutcome, MoveResult},
    match_runtime::{Match, MatchOptions, PlayerConfig},
    match_support::{SupportDenial, SupportKind},
    recipient_snapshot::{DeliveryState, SelectedSeat},
    unit_publication::{PublicationParts, StagedUnit, prepare_publication},
    weather::WeatherChoice,
};
use serde_json::json;

pub(crate) fn scene(mode: Mode) -> Match {
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"Retreat and support", "w":128,"h":128,"rows":vec![".".repeat(128);128],
            "heights":vec!["0".repeat(128);128],
            "spawns":[{"x":8,"y":8},{"x":112,"y":112}],
            "points":[{"id":"neutral","x":64,"y":64,"mp":0,"vp":0}]
        }))
        .unwrap(),
    )
    .unwrap();
    let constructor = if mode == Mode::Classic {
        Match::new_classic
    } else {
        Match::new
    };
    constructor(
        &map,
        MatchOptions {
            seed: 701,
            mode,
            players: (0..2)
                .map(|slot| PlayerConfig {
                    name: format!("p{slot}"),
                    team: slot,
                    faction: Faction::try_from(slot as u8).unwrap(),
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
pub(crate) fn publish_rifle(state: &mut Match, at: Point) -> u32 {
    let id = state.next_unit_id;
    let staged = StagedUnit::ordinary(
        &state.world.game,
        &state.rules,
        0,
        state.rules.kind("rifle").unwrap(),
        id,
        state.world.game.units.len() as u32,
        state.start.players[0].spawn,
        Some(at),
    )
    .unwrap();
    prepare_publication(
        PublicationParts {
            units: &mut state.world.game.units,
            grid: &mut state.grid,
            controls: &mut state.commands.units,
            recruitment: &mut state.recruit_metadata,
            next_id: &mut state.next_unit_id,
        },
        &state.rules,
        2,
        vec![staged],
    )
    .unwrap()
    .publish_with(|unit, _| {
        state.combat.initialize(unit.id);
        state.firing.initialize(unit.id);
        state.abilities.initialize(unit.id);
        state.recovery.initialize(unit.id);
    });
    id
}
pub(crate) fn seats(state: &Match) -> Vec<SelectedSeat> {
    (0..state.world.game.players.len())
        .map(|slot| SelectedSeat {
            slot,
            full: false,
            omit: false,
        })
        .collect()
}
pub(crate) fn delivered_tick(
    state: &mut Match,
    delivery: &mut DeliveryState,
    selected: &[SelectedSeat],
) {
    let report = state.advance_tick().unwrap();
    let frame = delivery
        .capture_selected_with_tick_events(state, selected, &report.events)
        .unwrap()
        .unwrap();
    for seat in selected {
        assert_eq!(frame.recipient(seat.slot).unwrap().into_json()["t"], "s");
    }
}

#[test]
fn mov48_ordinary_retreat_reaches_owned_home_through_ticks_and_every_seat_delivery() {
    for mode in [Mode::Default, Mode::Conquest] {
        let mut state = scene(mode);
        let id = publish_rifle(&mut state, Point { x: 50., z: 50. });
        let home = state.reaction_home(id).unwrap();
        let selected = seats(&state);
        let mut delivery = DeliveryState::new(&state).unwrap();
        assert_eq!(
            state
                .execute_command(
                    0,
                    &Command::Retreat {
                        ids: vec![i64::from(id)]
                    },
                    false
                )
                .unwrap(),
            None
        );
        assert!(state.world.game.units[&id].retreating);
        assert_eq!(state.world.game.units[&id].world_goal, Some(home));
        assert_eq!(state.rules.movement.retreat_speed, 1.5);
        let home_radius = Catalog::compiled()
            .unwrap()
            .table("CFG")
            .unwrap()
            .get("reinforceRadius")
            .and_then(Value::number)
            .unwrap();
        let mut reached = false;
        for _ in 0..600 {
            delivered_tick(&mut state, &mut delivery, &selected);
            let unit = &state.world.game.units[&id];
            assert_eq!(state.firing.get(id).unwrap().target_id, 0);
            if matches!(
                state.commands.units[&id].move_result,
                Some(MoveResult::Completed { .. })
            ) {
                assert!(!unit.retreating);
                assert!(unit.world_goal.is_none());
                // Completion runs before idle spacing, which may assign a new path this tick.
                // The retained result proves the retreat route completed before that movement.
                assert!(crate::math::hypot(unit.x - home.x, unit.z - home.z) <= home_radius);
                assert_eq!(
                    state.commands.units[&id].outcome,
                    Some(MoveOutcome::Arrived)
                );
                reached = true;
                break;
            }
        }
        assert!(reached, "retreat must complete through ordinary ticks");
        assert!(state.tick_state.fault().is_none());
    }
}

#[test]
fn mov91_eight_support_prices_timers_and_mode_currencies_match_pinned_values() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../tests/fixtures/misc-home-support.json")).unwrap();
    for row in fixture["support"].as_array().unwrap() {
        let name = row["name"].as_str().unwrap();
        let mp = row["mp"].as_f64().unwrap();
        let mun = row["mun"].as_f64().unwrap();
        let cd = row["cd"].as_f64().unwrap();
        let delay = row["delay"].as_f64().unwrap();
        for mode in [
            Mode::Default,
            Mode::Conquest,
            Mode::Assault,
            Mode::Annihilation,
            Mode::Horde,
            Mode::Tutorial,
            Mode::Other,
            Mode::Classic,
            Mode::World,
        ] {
            let cost = economy::sup_cost(mode, name).unwrap();
            assert_eq!(
                cost.currency,
                if mode.is_construction() {
                    Currency::Munitions
                } else {
                    Currency::Manpower
                }
            );
            assert_eq!(cost.cost, if mode.is_construction() { mun } else { mp });
        }
        let definition = Catalog::compiled().unwrap().support(name).unwrap();
        assert_eq!(definition.get("cd").and_then(Value::number), Some(cd));
        assert_eq!(definition.get("delay").and_then(Value::number), Some(delay));
    }
}

#[test]
fn mov91_own_string_keys_all_eight_admissions_denials_and_default_direction() {
    let mut state = scene(Mode::Conquest);
    state.start.economy.players[0].mp = 10000.;
    for kind in [
        json!({"toString":null,"valueOf":null}),
        json!(["dive"]),
        json!(null),
        json!(1),
    ] {
        assert_eq!(
            Command::from_json(&json!({"t":"support","kind":kind,"x":20,"z":20})),
            Err(Denial::Blocked)
        );
    }
    for name in ["toString", "__proto__", "constructor", "nuke", ""] {
        let before = state.start.economy.players[0].mp;
        assert_eq!(
            state
                .admit_support(0, name, Point { x: 20., z: 20. }, None)
                .unwrap(),
            Some(SupportDenial::Blocked)
        );
        assert_eq!(state.start.economy.players[0].mp, before);
        assert!(state.world.game.strikes.is_empty());
        assert!(economy::sup_cost(Mode::Classic, name).is_err());
    }
    let at = state.start.players[0].spawn;
    let at = Point {
        x: at.x + 8.,
        z: at.z,
    };
    for name in [
        "recon",
        "artillery",
        "strafe",
        "smoke",
        "bombing",
        "dive",
        "para",
        "cover",
    ] {
        let cost = economy::sup_cost(Mode::Conquest, name).unwrap().cost;
        let before = state.start.economy.players[0].mp;
        state.start.economy.players[0].mp = cost - 1.;
        assert_eq!(
            state.admit_support(0, name, at, None).unwrap(),
            Some(SupportDenial::Manpower)
        );
        assert_eq!(state.start.economy.players[0].mp, cost - 1.);
        state.start.economy.players[0].mp = before;
        assert_eq!(state.admit_support(0, name, at, None).unwrap(), None);
        assert_eq!(state.start.economy.players[0].mp, before - cost);
        let strike = state.world.game.strikes.last().unwrap();
        assert_eq!(strike.dir, 0.);
        assert_eq!(strike.runtime.as_ref().unwrap().kind.name(), name);
        let count = state.world.game.strikes.len();
        assert_eq!(
            state.admit_support(0, name, at, Some(1.25)).unwrap(),
            Some(SupportDenial::Cooldown)
        );
        assert_eq!(state.world.game.strikes.len(), count);
        assert_eq!(state.start.economy.players[0].mp, before - cost);
    }
    state.start.players[0].support.insert("cover".into(), 0.);
    assert_eq!(
        state.admit_support(0, "cover", at, Some(1.25)).unwrap(),
        None
    );
    assert_eq!(state.world.game.strikes.last().unwrap().dir, 1.25);
}

#[test]
fn mov91_supported_support_lifetimes_survive_ticks_and_every_seat_delivery() {
    let mut state = scene(Mode::Conquest);
    state.start.economy.players[0].mp = 1000.;
    let at = Point { x: 128., z: 128. };
    let selected = seats(&state);
    let mut delivery = DeliveryState::new(&state).unwrap();
    for name in ["artillery", "smoke", "cover"] {
        assert_eq!(state.admit_support(0, name, at, None).unwrap(), None);
    }
    assert_eq!(state.story[0].support_calls, 3.);
    assert_eq!(state.story[0].mp_spent, 320.);
    let mut cover_active = false;
    let mut smoke_active = false;
    for _ in 0..1260 {
        delivered_tick(&mut state, &mut delivery, &selected);
        cover_active |= !state.support.covers.is_empty();
        smoke_active |= !state.world.game.smokes.is_empty();
    }
    assert!(cover_active && smoke_active);
    assert!(state.world.game.strikes.is_empty());
    assert!(state.world.game.smokes.is_empty());
    assert!(state.support.covers.is_empty());
    assert!(state.support.events.is_empty());
    assert!(state.tick_state.fault().is_none());
    assert!(state.start.players[0].support[SupportKind::Smoke.name()] <= 0.);
}

#[test]
fn mov48_aircraft_retreat_component_returns_only_airborne_states_home() {
    use crate::game::PlaneState;
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../tests/fixtures/misc-home-support.json")).unwrap();
    let plane_state = |name: &str| match name {
        "base" => PlaneState::Base,
        "rearm" => PlaneState::Rearm,
        "out" => PlaneState::Out,
        "station" => PlaneState::Station,
        "home" => PlaneState::Home,
        _ => panic!("unknown Source aircraft state"),
    };
    for row in fixture["aircraft"].as_array().unwrap() {
        let prior = plane_state(row["prior"].as_str().unwrap());
        let after = plane_state(row["after"].as_str().unwrap());
        let mut state = scene(Mode::Conquest);
        let id = state.spawn_aircraft(0, "fighter").unwrap();
        state.world.game.units.get_mut(&id).unwrap().air = Some(prior);
        assert_eq!(
            state
                .execute_command(
                    0,
                    &Command::Retreat {
                        ids: vec![i64::from(id)]
                    },
                    false
                )
                .unwrap(),
            None
        );
        assert_eq!(state.world.game.units[&id].air, Some(after));
        assert!(!state.world.game.units[&id].retreating);
        assert_eq!(
            state.commands.units[&id].outcome,
            Some(crate::match_commands::MoveOutcome::Interrupted)
        );
        assert!(state.commands.units[&id].orders.is_empty());
    }
}
