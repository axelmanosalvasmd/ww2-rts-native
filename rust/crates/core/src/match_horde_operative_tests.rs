//! Horde Operatives share the ordinary tick, projectile and recipient owners.
use crate::{
    companions::{Input, OPERATIVE, WEAPONS},
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::Point,
    map::AuthoritativeMap,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    match_tick::TickPhase,
    recipient_snapshot::{DeliveryState, OperativeDelivery, SelectedSeat},
    weather::WeatherChoice,
};
use serde_json::json;
use std::collections::BTreeSet;

fn scene() -> (Match, u32) {
    let map = AuthoritativeMap::from_value(json!({
        "name":"Horde Operative delivery", "w":80, "h":80,
        "rows":vec![".".repeat(80);80],
        "spawns":[{"x":10,"y":8},{"x":70,"y":8},{"x":40,"y":74}],
        "defend":[0,1], "points":[{"x":40,"y":40}]
    }))
    .unwrap();
    let mut state = Match::new_horde(
        &map,
        MatchOptions {
            seed: 0x417413,
            mode: Mode::Horde,
            players: vec![
                PlayerConfig {
                    name: "Operative commander".into(),
                    team: 0,
                    faction: Faction::Usa,
                },
                PlayerConfig {
                    name: "Allied defender".into(),
                    team: 0,
                    faction: Faction::Germany,
                },
            ],
            shuffle: false,
            army: ArmySize::Standard,
            weather: WeatherChoice::Map,
            weather_seed: Some(1),
            living_weather: false,
            map_key: None,
            tech: true,
            supply: false,
            logistics: false,
            sandbox: false,
        },
    )
    .unwrap();
    for unit in state.world.game.units.values_mut() {
        unit.hold_pos = true;
    }
    for control in state.commands.units.values_mut() {
        control.hold_fire = true;
        control.auto_cast = false;
        control.auto_retreat = false;
    }
    let operative = state
        .join_companion("operative", 0, false)
        .unwrap()
        .unwrap();
    // Declared initial scene: an open grenade lane away from either army and
    // full visibility, so every canonical seat must deliver the actual effects.
    let lane = Point { x: 80.0, z: 70.0 };
    assert!(
        state
            .world
            .game
            .units
            .values()
            .filter(|u| u.id != operative)
            .all(|u| { crate::math::hypot(u.x - lane.x, u.z - lane.z) > 30.0 })
    );
    let unit = state.world.game.units.get_mut(&operative).unwrap();
    unit.x = lane.x;
    unit.z = lane.z;
    state.grid.update(operative, unit);
    state.world.game.reveal = true;
    state.end_horde_break().unwrap();
    (state, operative)
}

#[test]
fn horde_operative_fires_and_grenade_detonates_with_every_seat_and_independent_delivery() {
    let (mut state, operative) = scene();
    let horde = state.horde.as_ref().unwrap().slot;
    assert_eq!(horde, 2);
    assert_eq!(state.start.players.len(), 3);
    assert_eq!(state.companions.get("operative").unwrap().owner, 0);
    let seats = (0..state.start.players.len())
        .map(|slot| SelectedSeat {
            slot,
            full: true,
            omit: false,
        })
        .collect::<Vec<_>>();
    let mut commanders = DeliveryState::new(&state).unwrap();
    let mut companion = OperativeDelivery::new(&state, 0).unwrap();
    let mut launched = BTreeSet::new();
    let mut terminal = BTreeSet::new();
    let mut saw_fire = vec![false; seats.len() + 1];
    let mut saw_throw = vec![false; seats.len() + 1];
    let mut saw_flight = vec![false; seats.len() + 1];
    let mut retained_ticks = 0;
    let mut after_detonation = 0;
    for step in 0..100 {
        if step < 2 {
            assert!(state.companion_input(
                "operative",
                &json!({"seq":step+1,"forward":0.0,"strafe":0.0,"yaw":0.0,
                    "pitch":0.0,"weapon":0,"fire":step==0,"nade":step}),
                false,
            ));
        }
        let report = state.advance_tick().unwrap();
        assert!(report.phases.contains(&TickPhase::Horde));
        assert!(state.finish.is_none());
        for ticket in &state.firing.grenades {
            if state
                .firing
                .flights
                .get(&ticket.flight)
                .is_some_and(|f| f.shooter == operative)
            {
                launched.insert(ticket.flight);
            }
        }
        if launched
            .iter()
            .any(|id| state.firing.flights.contains_key(id))
        {
            retained_ticks += 1;
        } else if !launched.is_empty() {
            after_detonation += 1;
        }
        let mut rows = {
            let frame = commanders
                .capture_selected_with_tick_events(&mut state, &seats, &report.events)
                .unwrap()
                .unwrap();
            (0..seats.len())
                .map(|slot| frame.recipient(slot).unwrap().into_json())
                .collect::<Vec<_>>()
        };
        rows.push(companion.capture(&mut state, true, &report.events).unwrap());
        for (recipient, row) in rows.iter().enumerate() {
            assert_eq!(row["tick"], report.tick);
            assert_eq!(row["mode"]["kind"], "horde");
            assert_eq!(row["mode"]["slot"], horde);
            assert_eq!(row["mode"]["wave"], 1);
            for shot in row["shots"].as_array().unwrap() {
                if shot["f"].as_u64() == Some(u64::from(operative)) {
                    saw_fire[recipient] |= shot["k"] == "sniper";
                    saw_throw[recipient] |= shot["k"] == "throw";
                }
                if shot["k"] == "contact"
                    && shot["terminal"] == true
                    && shot["explosive"] == true
                    && let Some(flight) = shot["flight"].as_u64()
                    && launched.contains(&flight)
                {
                    terminal.insert(flight);
                }
            }
            saw_flight[recipient] |= row["flights"].as_array().unwrap().iter().any(|flight| {
                flight["flight"]
                    .as_u64()
                    .is_some_and(|id| launched.contains(&id))
            });
        }
        assert_eq!(rows.last().unwrap()["mode"], rows[0]["mode"]);
        assert_eq!(state.companion_status("operative")["id"], operative);
        assert!(state.tick_state.fault().is_none());
    }
    assert_eq!(state.companions.get("operative").unwrap().shots, 1);
    assert_eq!(
        state.companions.soldiers[&operative].mags[0],
        WEAPONS[0].mag - 1
    );
    assert_eq!(
        state.companions.soldiers[&operative].nades,
        OPERATIVE.nades - 1
    );
    assert_eq!(launched.len(), 1);
    assert!(
        retained_ticks > 2,
        "grenade must survive multiple ordinary ticks"
    );
    assert_eq!(
        terminal, launched,
        "the retained grenade must detonate naturally"
    );
    assert!(
        launched
            .iter()
            .all(|id| !state.firing.flights.contains_key(id))
    );
    assert!(
        state
            .firing
            .grenades
            .iter()
            .all(|ticket| !launched.contains(&ticket.flight))
    );
    assert!(
        after_detonation >= 40,
        "continue ordinary delivery after projectile cleanup"
    );
    assert!(
        saw_fire.iter().all(|seen| *seen),
        "every seat and Operative must receive fire"
    );
    assert!(
        saw_throw.iter().all(|seen| *seen),
        "every seat and Operative must receive the throw"
    );
    assert!(
        saw_flight.iter().all(|seen| *seen),
        "every seat and Operative must receive the retained flight"
    );
}

fn unchanged_state(state: &Match) -> String {
    format!(
        "{:?}",
        (
            &state.world,
            &state.companions,
            &state.start.economy,
            &state.horde,
            &state.firing,
            &state.combat,
            &state.abilities,
            state.commands.queue.snapshot(),
            state.start.random.snapshot(),
            state.next_unit_id,
        )
    )
}

fn ready_input() -> Input {
    Input {
        forward: 0.0,
        strafe: 0.0,
        yaw: 0.0,
        pitch: 0.0,
        fire: true,
        ads: false,
        crouch: false,
        sprint: false,
        weapon: 0,
    }
}

#[test]
fn malformed_horde_operative_context_refuses_tick_and_effects_before_mutation() {
    let (mut state, operative) = scene();
    state.horde.as_mut().unwrap().slot = 0;
    let before = unchanged_state(&state);
    assert!(state.prepare_companion_tick().is_err());
    assert!(
        state
            .fire_companion(operative, &ready_input(), false)
            .is_err()
    );
    assert!(
        state
            .throw_companion(operative, &ready_input(), false)
            .is_err()
    );
    let failure = state.advance_tick().unwrap_err();
    assert!(failure.events.is_empty());
    assert_eq!(unchanged_state(&state), before);
    assert!(state.tick_state.fault().is_none());
}

#[test]
fn horde_operative_extension_keeps_world_and_tutorial_combat_guards() {
    for mode in [Mode::World, Mode::Tutorial] {
        let (mut state, operative) = scene();
        // Deliberately request an unsupported mode at the companion provider's
        // boundary. This does not claim either mode's full constructor is valid.
        state.options.mode = mode;
        let before = unchanged_state(&state);
        assert!(state.prepare_companion_tick().is_err(), "{mode:?}");
        assert!(
            state
                .fire_companion(operative, &ready_input(), false)
                .is_err(),
            "{mode:?}"
        );
        assert!(
            state
                .throw_companion(operative, &ready_input(), false)
                .is_err(),
            "{mode:?}"
        );
        assert_eq!(unchanged_state(&state), before, "{mode:?}");
    }
}
