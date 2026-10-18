use super::*;
use crate::{
    economy_state::ArmySize,
    map::AuthoritativeMap,
    match_commands::Command,
    match_human_observation::HumanObservationMemory,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    recipient_snapshot::DeliveryState,
    weather::WeatherChoice,
};

fn tick_and_deliver(state: &mut Match, delivery: &mut DeliveryState) {
    let report = state.advance_tick().unwrap();
    assert!(state.tick_state.fault().is_none());
    let effects = report.environment().cloned().collect::<Vec<_>>();
    let frame = delivery
        .capture_with_lists(state, &[true; 2], &[false; 2], &effects)
        .unwrap();
    for seat in 0..2 {
        assert_eq!(frame.recipient(seat).unwrap().into_json()["t"], "s");
    }
}

fn ordinary() -> (Match, u32) {
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"AI ordinary consumer lifecycle", "w":80,"h":80,
            "rows":vec![".".repeat(80);80],
            "spawns":[{"x":4,"y":4},{"x":75,"y":75}],
            "points":[{"x":30,"y":30}]
        }))
        .unwrap(),
    )
    .unwrap();
    let mut state = Match::new(
        &map,
        MatchOptions {
            seed: 701,
            mode: Mode::Default,
            players: (0..2)
                .map(|slot| PlayerConfig {
                    name: format!("p{slot}"),
                    team: slot,
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
        },
    )
    .unwrap();
    let worker = state
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 0 && state.rules.kinds[u.kind].name == "rifle")
        .unwrap()
        .id;
    relocate(&mut state, worker, Point { x: 60.0, z: 60.0 });
    (state, worker)
}

fn relocate(state: &mut Match, id: u32, at: Point) {
    crate::unit_placement::place_unit(
        &mut state.grid,
        state.world.game.units.get_mut(&id).unwrap(),
        |unit| {
            unit.x = at.x;
            unit.z = at.z;
        },
    );
}

fn add_tank(state: &mut Match) -> u32 {
    let id = state.next_unit_id;
    let draft = crate::unit_publication::StagedUnit::ordinary(
        &state.world.game,
        &state.rules,
        1,
        state.rules.kind("tank").unwrap(),
        id,
        state.world.game.units.len() as u32,
        state.start.players[1].spawn,
        Some(Point { x: 80.0, z: 60.0 }),
    )
    .unwrap();
    let reserves = state.prepare_new_unit_reserves(&draft.unit).unwrap();
    let publication = crate::unit_publication::prepare_publication(
        crate::unit_publication::PublicationParts {
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
    .unwrap();
    publication.publish_with(|unit, _| {
        state.combat.initialize(unit.id);
        state.firing.initialize(unit.id);
        state.abilities.initialize(unit.id);
        state.recovery.initialize(unit.id);
        if let Some(reserves) = reserves.clone() {
            state.logistics.insert(unit.id, reserves);
        }
    });
    id
}

fn prime(state: &mut Match) {
    for control in state.commands.units.values_mut() {
        control.hold_fire = true;
        control.auto_cast = false;
        control.auto_retreat = false;
    }
}

#[test]
fn if25_ai_ready_aircraft_producer_planner_commands_survive_ordinary_ticks_and_all_seat_snapshots()
{
    let (mut state, _) = ordinary();
    let tank = add_tank(&mut state);
    let attacker = state.spawn_aircraft(0, "attacker").unwrap();
    let fighter = state.spawn_aircraft(0, "fighter").unwrap();
    prime(&mut state);
    let mut delivery = DeliveryState::new(&state).unwrap();
    for _ in 0..2 {
        tick_and_deliver(&mut state, &mut delivery);
    }
    for id in [attacker, fighter] {
        let unit = &state.world.game.units[&id];
        assert_eq!(unit.air, Some(crate::game::PlaneState::Base));
        assert!(unit.x.is_finite() && unit.z.is_finite());
        assert!(
            unit.x < 0.0
                || unit.z < 0.0
                || unit.x > state.world.game.w as f64 * crate::game::CELL
                || unit.z > state.world.game.h as f64 * crate::game::CELL
        );
    }
    let delivered = state
        .human_commander_observation(&mut HumanObservationMemory::new(0))
        .unwrap();
    let observation = &delivered.packet;
    let input = observation.input();
    let context = input.runtime.as_ref().unwrap();
    assert!(context.units[&attacker].air_at_base);
    assert!(context.units[&fighter].air_at_base);
    let mine = input
        .units
        .iter()
        .filter(|u| u.owner == 0 && !catalog_structure(u))
        .collect::<Vec<_>>();
    let enemies = input
        .units
        .iter()
        .filter(|u| u.owner == 1)
        .collect::<Vec<_>>();
    let commands = aircraft(observation, context, &mine, &enemies).unwrap();
    assert_eq!(commands.len(), 2);
    assert!(
        commands
            .iter()
            .any(|c| c["t"] == "attack" && c["ids"] == json!([attacker]) && c["target"] == tank)
    );
    assert!(
        commands
            .iter()
            .any(|c| c["t"] == "move" && c["orders"][0][0] == fighter)
    );
    for command in commands {
        assert_eq!(
            state
                .execute_command(0, &Command::from_json(&command).unwrap(), false)
                .unwrap(),
            None
        );
    }
    assert_eq!(
        state.aviation.planes[&attacker].mission.unwrap().target,
        Some(tank)
    );
    assert!(state.aviation.planes[&fighter].mission.is_some());
    for _ in 0..8 {
        tick_and_deliver(&mut state, &mut delivery);
    }
    assert!(
        !state
            .human_commander_observation(&mut HumanObservationMemory::new(0))
            .unwrap()
            .packet
            .input()
            .runtime
            .as_ref()
            .unwrap()
            .units[&attacker]
            .air_at_base
    );
}

fn catalog_structure(unit: &UnitObservation) -> bool {
    boolean(
        Catalog::compiled().unwrap().unit(&unit.kind).unwrap(),
        "structure",
    )
}

#[test]
fn if25_ai_holder_work_from_delivered_point_survives_ordinary_ticks_and_all_seat_snapshots() {
    let (mut state, worker) = ordinary();
    state.start.points[0].at = Point { x: 60.0, z: 60.0 };
    state.start.points[0].owner = Some(0);
    state.start.economy.players[0].mp = 1000.0;
    prime(&mut state);
    let mut delivery = DeliveryState::new(&state).unwrap();
    for _ in 0..2 {
        tick_and_deliver(&mut state, &mut delivery);
    }
    let delivered = state
        .human_commander_observation(&mut HumanObservationMemory::new(0))
        .unwrap();
    let observation = &delivered.packet;
    let input = observation.input();
    let unit = input.units.iter().find(|u| u.id == worker).unwrap();
    assert!(input.runtime.as_ref().unwrap().field.is_some());
    let mut commands = Vec::new();
    crate::commander_field_policy::holder(observation, unit, 0, false, |command| {
        commands.push(command);
        true
    })
    .unwrap();
    assert_eq!(commands.len(), 1);
    let command = &commands[0];
    assert_eq!(command["kind"], "mines");
    assert_eq!(
        state
            .execute_command(0, &Command::from_json(command).unwrap(), false)
            .unwrap(),
        None
    );
    assert!(state.world.game.units[&worker].dig);
    assert!(state.field_works.jobs().contains_key(&worker));
    for _ in 0..8 {
        tick_and_deliver(&mut state, &mut delivery);
    }
    assert!(state.world.game.units[&worker].dig || !state.world.mines.is_empty());
}
