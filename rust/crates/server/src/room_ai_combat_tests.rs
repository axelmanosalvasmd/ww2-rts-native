//! Actual hosted policy abilities retain projectiles across recipient deliveries.
use super::*;
use crate::room_runtime::{FrameKind, RoomRuntime};
use serde_json::json;
use std::collections::BTreeSet;
use ww2_core::match_commands::Command;
use ww2_core::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::Point,
    map::AuthoritativeMap,
    match_runtime::{MatchOptions, PlayerConfig},
    match_tick::TickPathConfig,
    recipient_snapshot::SelectedSeat,
    unit_publication::{PublicationParts, StagedUnit, prepare_publication},
    weather::WeatherChoice,
};

fn publish(state: &mut Match, owner: usize, kind: &str, at: Point) -> u32 {
    let id = state.next_unit_id;
    let draft = StagedUnit::ordinary(
        &state.world.game,
        &state.rules,
        owner,
        state.rules.kind(kind).unwrap(),
        id,
        0,
        state.start.players[owner].spawn,
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
        state.start.players.len(),
        vec![draft],
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

#[test]
fn hosted_horde_ai_grenade_survives_every_seat_delivery_until_natural_detonation() {
    // Prototype shared/ai.js:309,656-659,682:
    // the scripted director uses its delivered enemy sightings to throw at
    // infantry other than exposed rifles, through ordinary command admission.
    let public_map = json!({
        "name":"Hosted Horde grenade lifecycle", "w":80, "h":80,
        "rows":vec![".".repeat(80);80],
        "spawns":[{"x":40,"y":5},{"x":10,"y":74},{"x":70,"y":74}],
        "defend":[0], "points":[{"x":40,"y":30}]
    });
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&public_map).unwrap()).unwrap();
    let mut state = Match::new_horde(
        &map,
        MatchOptions {
            seed: 0x731e,
            mode: Mode::Horde,
            players: vec![PlayerConfig {
                name: "Defender AI".into(),
                team: 0,
                faction: Faction::Usa,
            }],
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
    let horde = state.horde.as_ref().unwrap().slot;
    let rifle = publish(&mut state, horde, "rifle", Point { x: 80.0, z: 80.0 });
    let mg = publish(&mut state, 0, "mg", Point { x: 100.0, z: 80.0 });
    // Direct firing calls must still reject malformed or non-aimed controls.
    state.commands.units.get_mut(&rifle).unwrap().nade = Some(Point {
        x: f64::NAN,
        z: 80.0,
    });
    assert_eq!(
        state.apply_unit_firing(rifle, false, 0.05).unwrap_err(),
        "invalid aimed ability point"
    );
    state.commands.units.get_mut(&rifle).unwrap().nade = None;
    state.commands.units.get_mut(&mg).unwrap().nade = Some(Point { x: 80.0, z: 80.0 });
    assert_eq!(
        state.apply_unit_firing(mg, false, 0.05).unwrap_err(),
        "pending firing aim requires an aimed ability"
    );
    state.commands.units.get_mut(&mg).unwrap().nade = None;
    // AI targets up to range + 4. At 20 m the rifle must approach its 18 m throw.
    // Both the initial reload and target scan expire during the approach. Without
    // this scan delay, the first tick acquires a target and the ability clears it
    // while the 0.5 s retarget timer lasts until the grenade reaches range.
    state.firing.get_mut(rifle).unwrap().cooldown = 0.3;
    state.firing.get_mut(rifle).unwrap().retarget = 0.3;
    // Declared initial scene: visible opposing infantry beyond grenade range. The
    // ordinary tick phase, policy and command queue own everything after setup.
    state.world.game.reveal = true;
    state.end_horde_break().unwrap();
    state
        .initialize_tick_scene(25, TickPathConfig::default())
        .unwrap();
    let mut runtime = RoomRuntime::new(state, 0).unwrap();
    runtime.activate_ai(&public_map, 0, "hard").unwrap();
    runtime.activate_ai(&public_map, horde, "normal").unwrap();
    runtime
        .collect_frame(&runtime.ai_selected(), FrameKind::Current)
        .unwrap();
    let mut aimed = BTreeSet::new();
    let mut accepted = BTreeSet::new();
    let mut flights = BTreeSet::new();
    let mut terminal = BTreeSet::new();
    let mut retained_ticks = 0;
    let mut delivered_throws = 0;
    let mut delivered_flights = 0;
    let mut after_expiration = 0;
    let mut approach_ticks = 0;
    let mut approach_shots = 0;
    let mut approach_trace = Vec::new();
    for _ in 0..180 {
        let report = runtime.advance_tick(&[false, false]).unwrap();
        for result in &report.commands {
            if aimed.contains(&result.key) {
                assert_eq!(result.denial, None, "scripted grenade command was refused");
                accepted.insert(result.key);
            }
        }
        if runtime
            .state()
            .commands
            .units
            .get(&rifle)
            .is_some_and(|control| control.nade.is_some())
        {
            approach_ticks += 1;
            let actor = &runtime.state().world.game.units[&rifle];
            assert!(
                actor.x > 80.0,
                "pending ability did not approach its target"
            );
            assert!(!actor.path.is_empty());
            assert!(runtime.state().abilities.get(rifle).unwrap().aimed_walked);
            let firing = runtime.state().firing.get(rifle).unwrap();
            approach_trace.push(json!({"tick":report.tick,"x":actor.x,"z":actor.z,
                "cooldown":firing.cooldown,"retarget":firing.retarget,"target":firing.target_id,
                "shotAt":actor.shot_at,"supp":actor.supp,"retreating":actor.retreating,
                "canShootTarget":runtime.state().can_shoot_target(rifle, mg).unwrap()}));
        }
        for grenade in &runtime.state().firing.grenades {
            if runtime
                .state()
                .firing
                .flights
                .get(&grenade.flight)
                .is_some_and(|flight| flight.shooter == rifle)
            {
                flights.insert(grenade.flight);
            }
        }
        let live = flights
            .iter()
            .any(|id| runtime.state().firing.flights.contains_key(id));
        if live {
            retained_ticks += 1;
        } else if !flights.is_empty() {
            after_expiration += 1;
        }
        let sent = runtime.should_snapshot(report.tick);
        let selected = (0..runtime.state().world.game.players.len())
            .map(|slot| SelectedSeat {
                slot,
                full: true,
                omit: false,
            })
            .collect::<Vec<_>>();
        let frames = runtime
            .collect_frame(
                &selected,
                if sent {
                    FrameKind::Scheduled
                } else {
                    FrameKind::Current
                },
            )
            .unwrap();
        runtime.tick_ai(sent).unwrap();
        for (key, command) in runtime
            .state()
            .commands
            .pending_tick(report.tick as u64 + 1)
        {
            if key.issuer as usize == horde
                && let Command::Ability {
                    ids,
                    at: Some(_),
                    automatic: false,
                    ..
                } = command
                && ids.contains(&i64::from(rifle))
            {
                aimed.insert(*key);
            }
        }
        assert_eq!(frames.len(), selected.len());
        for frame in frames {
            assert_eq!(frame.message["tick"], report.tick);
            assert_eq!(frame.message["mode"]["kind"], "horde");
            for shot in frame.message["shots"].as_array().unwrap() {
                if shot["k"] == "throw" && shot["f"].as_u64() == Some(u64::from(rifle)) {
                    delivered_throws += 1;
                }
                if runtime
                    .state()
                    .commands
                    .units
                    .get(&rifle)
                    .is_some_and(|control| control.nade.is_some())
                    && shot["f"].as_u64() == Some(u64::from(rifle))
                    && shot["k"] == "flight"
                {
                    approach_shots += 1;
                }
                if shot["k"] == "contact"
                    && shot["terminal"] == true
                    && shot["explosive"] == true
                    && let Some(id) = shot["flight"].as_u64()
                    && flights.contains(&id)
                {
                    terminal.insert(id);
                }
            }
            if !frame.message["flights"].as_array().unwrap().is_empty() {
                delivered_flights += 1;
            }
        }
        assert!(runtime.fault().is_none());
    }
    assert!(
        !aimed.is_empty(),
        "actual hosted policy never aimed its grenade"
    );
    assert!(
        approach_ticks > 1,
        "policy never retained an out-of-range aim"
    );
    assert!(
        approach_shots > 0,
        "ordinary firing stopped during the aimed approach: {approach_trace:?}"
    );
    assert_eq!(
        aimed, accepted,
        "scripted inputs did not reach command admission"
    );
    assert!(
        delivered_throws > 0,
        "throw never reached the recipient event path"
    );
    assert!(!flights.is_empty() && retained_ticks > 1 && delivered_flights > 0);
    assert!(
        flights.iter().all(|id| terminal.contains(id)),
        "grenade never detonated naturally"
    );
    assert!(
        flights
            .iter()
            .all(|id| !runtime.state().firing.flights.contains_key(id))
    );
    assert!(
        runtime
            .state()
            .firing
            .grenades
            .iter()
            .all(|ticket| !flights.contains(&ticket.flight))
    );
    assert!(
        after_expiration >= 20,
        "match did not continue after projectile cleanup"
    );
    assert!(
        runtime
            .state()
            .world
            .game
            .units
            .get(&mg)
            .is_none_or(|unit| unit.hp < 75.0)
    );
}
