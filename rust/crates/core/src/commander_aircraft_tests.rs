//! Aircraft policy intent over delivered facts and ordinary recipient observations.
use super::tests::{acts, fixture, run, unit};
use super::*;
use crate::game::PlaneState;

fn ready(input: &mut crate::commander_ai::Input, id: u32) {
    input
        .runtime
        .as_mut()
        .unwrap()
        .units
        .get_mut(&id)
        .unwrap()
        .air_at_base = true;
}

#[test]
fn intent_ai43_offmap_aircraft_are_finite_but_ground_and_paths_remain_bounded() {
    use crate::commander_ai::PolicyError;
    let mut input = fixture(vec![unit(1, "fighter", -28.3)]);
    assert!(Observation::new(input.clone()).is_ok());
    input.units[0].kind = "rifle".into();
    assert_eq!(Observation::new(input.clone()), Err(PolicyError::Position));
    input.units[0].kind = "fighter".into();
    input.units[0].at.x = f64::NAN;
    assert_eq!(Observation::new(input.clone()), Err(PolicyError::Position));
    input.units[0].at.x = f64::INFINITY;
    assert_eq!(Observation::new(input.clone()), Err(PolicyError::Position));
    input.units[0].at.x = -28.3;
    input.units[0].path.push(Point { x: -1.0, z: 20.0 });
    assert_eq!(Observation::new(input), Err(PolicyError::Position));
}

#[test]
fn intent_ai43_attackers_choose_nearest_visible_vehicle_or_gun_from_ground_army() {
    for kind in ["tank", "at", "flak", "mortar", "howitzer"] {
        let mut input = fixture(vec![
            unit(1, "attacker", 10.0),
            unit(2, "rifle", 100.0),
            unit(3, "rifle", 140.0),
            unit(4, "rifle", 120.0),
            unit(5, "hq", 121.0),
            unit(6, kind, 125.0),
            unit(7, "tank", 110.0),
            unit(8, "fighter", 122.0),
        ]);
        for u in &mut input.units[3..] {
            u.owner = 1;
        }
        ready(&mut input, 1);
        let commands = run(input, &mut State::default(), Level::Normal);
        assert!(
            commands.contains(&json!({"t":"attack","ids":[1],"target":6})),
            "{kind}: {commands:?}"
        );
    }
}

#[test]
fn intent_ai43_base_readiness_strict_range_and_spawn_fallback() {
    for (at_base, distance, expected) in [
        (true, 119.9, true),
        (true, 120.0, false),
        (true, 120.1, false),
        (false, 10.0, false),
    ] {
        // With no ground army the recipient's own spawn is the centre.
        let mut enemy = unit(2, "tank", 10.0 + distance);
        enemy.owner = 1;
        let mut input = fixture(vec![unit(1, "attacker", 150.0), enemy]);
        if at_base {
            ready(&mut input, 1);
        }
        let commands = run(input, &mut State::default(), Level::Normal);
        assert_eq!(
            acts(&commands, "attack", 1),
            expected,
            "base={at_base}, distance={distance}"
        );
        assert!(!acts(&commands, "move", 1));
    }
}

#[test]
fn intent_ai43_fighters_hunt_current_planes_and_ignore_hidden_sighting_memories() {
    let mut input = fixture(vec![
        unit(1, "fighter", 10.0),
        unit(2, "rifle", 100.0),
        unit(3, "rifle", 140.0),
    ]);
    ready(&mut input, 1);
    let hidden = Sighting {
        id: 99,
        kind: "fighter".into(),
        owner: 1,
        at: Point { x: 121.0, z: 20.0 },
        time: 10.0,
        worth: 200.0,
    };
    let mut state = State::default();
    let commands = plan(
        &Observation::new(input.clone()).unwrap(),
        &[hidden],
        &mut state,
        Level::Hard,
        &mut MatchRandom::new(7),
    )
    .unwrap();
    assert!(commands.contains(&json!({"t":"move","orders":[[1,120.0,20.0]]})));
    for (id, x) in [(4, 150.0), (5, 130.0), (6, 110.0)] {
        let mut enemy = unit(id, "fighter", x);
        enemy.owner = 1;
        input.units.push(enemy);
    }
    let commands = run(input.clone(), &mut State::default(), Level::Normal);
    assert!(
        commands.contains(&json!({"t":"move","orders":[[1,130.0,20.0]]})),
        "distance ties use unit ID"
    );
    input
        .runtime
        .as_mut()
        .unwrap()
        .units
        .get_mut(&1)
        .unwrap()
        .air_at_base = false;
    let commands = run(input, &mut State::default(), Level::Normal);
    assert!(!acts(&commands, "move", 1));
}

#[test]
fn intent_ai43_recipient_readiness_and_emitted_patrol_survive_ordinary_ticks_all_seats() {
    use crate::{
        economy_state::ArmySize,
        map::AuthoritativeMap,
        match_commands::Command,
        match_runtime::{Match, MatchOptions, PlayerConfig},
        recipient_snapshot::{DeliveryState, SelectedSeat},
        weather::WeatherChoice,
    };
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"AI aircraft", "w":64,"h":64,"rows":vec![".".repeat(64);64],
            "spawns":[{"x":8,"y":8},{"x":54,"y":54}],"points":[{"x":32,"y":32}]
        }))
        .unwrap(),
    )
    .unwrap();
    let mut m = Match::new(
        &map,
        MatchOptions {
            seed: 743,
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
    let fighter = m.spawn_aircraft(0, "fighter").unwrap();
    let enemy = m.spawn_aircraft(1, "fighter").unwrap();
    assert!(m.world.game.units[&fighter].x < 0.0 || m.world.game.units[&fighter].z < 0.0);
    for c in m.commands.units.values_mut() {
        c.hold_fire = true;
        c.auto_cast = false;
        c.auto_retreat = false;
    }
    for _ in 0..2 {
        m.advance_tick().unwrap();
    }
    let observation = m.opening_commander_observation(0).unwrap();
    assert!(!observation.input().units.iter().any(|u| u.id == enemy));
    assert!(observation.input().runtime.as_ref().unwrap().units[&fighter].air_at_base);
    m.world.game.units.get_mut(&fighter).unwrap().air = Some(PlaneState::Rearm);
    let refuelling = m.opening_commander_observation(0).unwrap();
    assert!(!refuelling.input().runtime.as_ref().unwrap().units[&fighter].air_at_base);
    let commands = plan(
        &refuelling,
        &[],
        &mut State::default(),
        Level::Normal,
        &mut MatchRandom::new(3),
    )
    .unwrap();
    assert!(!acts(&commands, "move", fighter));
    m.world.game.units.get_mut(&fighter).unwrap().air = Some(PlaneState::Base);
    let commands = plan(
        &observation,
        &[],
        &mut State::default(),
        Level::Normal,
        &mut MatchRandom::new(3),
    )
    .unwrap();
    let mission = commands
        .iter()
        .find(|c| acts(std::slice::from_ref(c), "move", fighter))
        .unwrap();
    assert_eq!(
        m.execute_command(0, &Command::from_json(mission).unwrap(), false)
            .unwrap(),
        None
    );
    assert_eq!(m.world.game.units[&fighter].air, Some(PlaneState::Out));
    let seats: Vec<_> = (0..2)
        .map(|slot| SelectedSeat {
            slot,
            full: false,
            omit: false,
        })
        .collect();
    let mut delivery = DeliveryState::new(&m).unwrap();
    let mut snapshots = 0;
    for _ in 0..8 {
        let report = m.advance_tick().unwrap();
        let frame = delivery
            .capture_selected_with_tick_events(&mut m, &seats, &report.events)
            .unwrap();
        if let Some(frame) = frame {
            snapshots += 1;
            for slot in 0..2 {
                assert!(frame.recipient(slot).is_ok());
            }
        }
        assert!(m.tick_state.fault().is_none());
    }
    assert_eq!(snapshots, 8);

    // Publish a visible ground vehicle through the ordinary canonical initializer.
    let attacker = m.spawn_aircraft(0, "attacker").unwrap();
    let target = m.next_unit_id;
    let draft = crate::unit_publication::StagedUnit::ordinary(
        &m.world.game,
        &m.rules,
        1,
        m.rules.kind("tank").unwrap(),
        target,
        0,
        m.start.players[1].spawn,
        Some(Point { x: 42.0, z: 16.0 }),
    )
    .unwrap();
    m.publish_starting_unit(draft).unwrap();
    m.commands.units.get_mut(&target).unwrap().hold_fire = true;
    m.commands.units.get_mut(&target).unwrap().auto_cast = false;
    m.commands.units.get_mut(&target).unwrap().auto_retreat = false;
    m.commands.units.get_mut(&attacker).unwrap().hold_fire = false;
    for _ in 0..4 {
        m.advance_tick().unwrap();
    }
    let observation = m.opening_commander_observation(0).unwrap();
    assert!(observation.input().units.iter().any(|u| u.id == target));
    assert!(m.world.game.players[0].sees(target));
    let commands = plan(
        &observation,
        &[],
        &mut State::default(),
        Level::Normal,
        &mut MatchRandom::new(3),
    )
    .unwrap();
    let attack = commands
        .iter()
        .find(|command| command["t"] == "attack" && command["ids"] == json!([attacker]))
        .unwrap();
    assert_eq!(attack["target"], target);
    assert_eq!(
        m.execute_command(0, &Command::from_json(attack).unwrap(), false)
            .unwrap(),
        None
    );
    assert_eq!(m.world.game.units[&attacker].air, Some(PlaneState::Out));
    assert!(!m.commands.units[&attacker].hold_fire);
    let ammo_before = m.aviation.planes[&attacker].ammo;
    let full_seats: Vec<_> = (0..2)
        .map(|slot| SelectedSeat {
            slot,
            full: true,
            omit: false,
        })
        .collect();
    let mut retained_flight = false;
    let mut full_snapshots = 0;
    // The off-map home is about 60 m from the target; allow the real ingress flight.
    for _ in 0..120 {
        let report = m.advance_tick().unwrap();
        retained_flight |= m
            .firing
            .flights
            .values()
            .any(|flight| flight.shooter == attacker);
        let frame = delivery
            .capture_selected_with_tick_events(&mut m, &full_seats, &report.events)
            .unwrap();
        if let Some(frame) = frame {
            full_snapshots += 1;
            for slot in 0..2 {
                frame.recipient(slot).unwrap().into_json();
            }
        }
        assert!(m.tick_state.fault().is_none());
    }
    assert!(
        retained_flight,
        "the policy-issued attacker mission must fire a retained flight"
    );
    assert!(m.aviation.planes[&attacker].ammo < ammo_before);
    assert_eq!(full_snapshots, 120);
}
