use super::*;
use crate::{game::Point, projectiles::{self as geometry, Point3}};
use crate::{
    economy::Faction,
    economy_state::ArmySize,
    match_commands::Command,
    match_firing::{AbilityBlast, LaunchEvent},
    match_runtime::{MatchOptions, PlayerConfig},
    match_tick::TickEvent,
    unit_publication::{PublicationParts, StagedUnit, prepare_publication},
    weather::WeatherChoice,
};

fn scene() -> Match {
    let map = crate::map::AuthoritativeMap::from_json(&serde_json::to_vec(&json!({
        "name":"Delivered Horde ground projectiles","w":80,"h":80,"rows":vec![".".repeat(80);80],
        "spawns":[{"x":40,"y":5},{"x":10,"y":74},{"x":70,"y":74}],
        "defend":[0],"points":[{"x":40,"y":30}]
    })).unwrap()).unwrap();
    Match::new_horde(
        &map,
        MatchOptions {
            seed: 701,
            mode: Mode::Horde,
            players: vec![PlayerConfig {
                name: "Defender".into(),
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
    .unwrap()
}

fn actor(state: &mut Match, kind: &str, owner: usize, at: Point) -> u32 {
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

fn flight(state: &mut Match, shooter: u32) -> u64 {
    let kind = state.world.game.units[&shooter].kind;
    let blast = AbilityBlast::from_unit(kind, &state.rules).unwrap();
    let id = state
        .launch_ability_flight(shooter, Point { x: 30.5, z: 30.5 }, blast, None, None)
        .unwrap();
    let p = state.firing.flights.get_mut(&id).unwrap();
    // A declared launch trajectory isolates the recipient geometry from ballistics.
    p.solution = geometry::Flight {
        origin: Point3::new(30.5, 10.0, 30.5),
        velocity: Point3::new(20.0, 0.0, 0.0),
        gravity: 0.0,
        stamp: 0.0,
    };
    p.launched = 0.0;
    p.last = 0.0;
    p.expires = 5.0;
    state.firing.events.clear();
    id
}

#[test]
fn retained_flight_shape_clips_at_ground_fog_and_redacts_unseen_shooter() {
    let mut state = scene();
    let shooter = actor(&mut state, "rifle", 1, Point { x: 130.0, z: 130.0 });
    let id = flight(&mut state, shooter);
    state.world.game.players[0].visible.clear();
    let g = &state.world.game;
    let key = Some(g.vision_tick.unwrap_or(-1));
    for team in [0, 1] {
        let mut fog = crate::fog::TeamFog::new(g.w * g.h);
        fog.key = key;
        if team == 0 {
            fog.vis[15 * g.w + 15] = 1;
        }
        state.routing_fog.teams.insert(team, fog);
    }
    validate(&state).unwrap();
    // Prototype sim.js:5986-5994: full precision, no target or private blast fields.
    assert_eq!(
        for_seat(&state, 0).unwrap(),
        json!([{
            "flight":id,"kind":"rifle","effect":"boom","seq":0,
            "x":30.5,"y":10.0,"z":30.5,"v":[20.0,0,0],"gravity":0,
            "time":0,"launched":0,"expires":5.0,"until":0.075
        }])
    );
    assert_eq!(
        for_seat(&state, 1).unwrap(),
        json!([]),
        "ownership does not bypass ground fog"
    );
    let event = LaunchEvent::Throw {
        shooter,
        owner: 1,
        at: Point { x: 30.5, z: 30.5 },
    };
    let selected = [
        SelectedSeat {
            slot: 0,
            full: true,
            omit: false,
        },
        SelectedSeat {
            slot: 1,
            full: true,
            omit: false,
        },
    ];
    let mut delivery = DeliveryState::new(&state).unwrap();
    let frame = delivery
        .capture_selected_with_shots(&mut state, &selected, &[ShotRef::Firing(&event)])
        .unwrap()
        .unwrap();
    assert_eq!(
        frame.recipient(0).unwrap().into_json()["shots"],
        json!([{"k":"throw","x":30.5,"z":30.5,"physical":true,"local":true}])
    );
    assert_eq!(frame.recipient(1).unwrap().into_json()["shots"], json!([]));
    drop(frame);
    state.world.game.reveal = true;
    let row = for_seat(&state, 0).unwrap();
    assert_eq!(row[0]["f"], shooter);
    assert_eq!(row[0]["fo"], 1);
    assert_eq!(row[0]["until"], 0.15);
    assert!(row[0].get("t").is_none() && row[0].get("to").is_none());
    let p = state.firing.flights.get_mut(&id).unwrap();
    p.launched = 0.1;
    p.solution.stamp = 0.1;
    assert_eq!(
        for_seat(&state, 0).unwrap(),
        json!([]),
        "delayed rounds stay absent until launch"
    );
}

#[test]
fn static_contact_extrapolation_uses_only_seen_bodies() {
    let mut state = scene();
    let shooter = actor(&mut state, "rifle", 1, Point { x: 130.0, z: 130.0 });
    let target = actor(&mut state, "rifle", 1, Point { x: 40.0, z: 30.5 });
    let id = flight(&mut state, shooter);
    let p = state.firing.flights.get_mut(&id).unwrap();
    p.solution.origin.y = 1.2;
    p.solution.velocity.x = 100.0;
    let mut fog = crate::fog::TeamFog::new(80 * 80);
    fog.key = Some(state.world.game.vision_tick.unwrap_or(-1));
    fog.vis.fill(1);
    state.routing_fog.teams.insert(0, fog);
    state.world.game.players[0].visible.clear();
    assert_eq!(for_seat(&state, 0).unwrap()[0]["until"], 0.15);
    state.world.game.players[0].visible.push(target);
    let radius = state.rules.kinds[state.world.game.units[&target].kind].radius * 0.8;
    let expected = (40.0 - radius - 30.5) / 100.0;
    let until = for_seat(&state, 0).unwrap()[0]["until"].as_f64().unwrap();
    assert!((until - expected).abs() < 1e-12);
    assert!(until < 0.15);
}

#[test]
fn ordinary_horde_grenade_and_barrage_deliver_every_seat_until_retained_state_expires() {
    let mut state = scene();
    let rifle = actor(&mut state, "rifle", 1, Point { x: 60.0, z: 70.0 });
    let mortar = actor(&mut state, "mortar", 1, Point { x: 50.0, z: 80.0 });
    actor(&mut state, "rifle", 0, Point { x: 72.0, z: 70.0 });
    for control in state.commands.units.values_mut() {
        control.hold_fire = true;
        control.auto_cast = false;
        control.auto_retreat = false;
    }
    state.world.game.reveal = true;
    state.tick_state.restrict_aircraft_weapons_for_delivery();
    state.restrict_to_ordinary_snapshots().unwrap();
    for id in [rifle, mortar] {
        assert_eq!(
            state.execute_command(
                1,
                &Command::Ability {
                    ids: vec![i64::from(id)],
                    at: Some(Point { x: 72.0, z: 70.0 }),
                    queue: false,
                    automatic: false,
                },
                false
            ),
            Ok(None)
        );
    }
    let selected = (0..2)
        .map(|slot| SelectedSeat {
            slot,
            full: false,
            omit: false,
        })
        .collect::<Vec<_>>();
    let mut delivery = DeliveryState::new(&state).unwrap();
    let mut thrown = false;
    let mut salvo = false;
    let mut live = false;
    let mut resolved = false;
    let mut ending = 0;
    for _ in 0..300 {
        let report = state.advance_tick().unwrap();
        thrown |= report.events.iter().any(
            |e| matches!(e,TickEvent::Firing(LaunchEvent::Throw{shooter,..}) if *shooter==rifle),
        );
        salvo |= report.events.iter().any(
            |e| matches!(e,TickEvent::Firing(LaunchEvent::Salvo{shooter,..}) if *shooter==mortar),
        );
        resolved |= report.events.iter().any(|e| {
            matches!(
                e,
                TickEvent::Firing(LaunchEvent::Contact(_) | LaunchEvent::Expiry { .. })
            )
        });
        let frame = delivery
            .capture_selected_with_tick_events(&mut state, &selected, &report.events)
            .unwrap()
            .unwrap();
        for slot in 0..2 {
            let snap = frame.recipient(slot).unwrap().into_json();
            live |= !snap["flights"].as_array().unwrap().is_empty();
            assert_eq!(snap["tick"], report.tick);
            assert_eq!(snap["mode"]["kind"], "horde");
        }
        drop(frame);
        if thrown
            && salvo
            && state.firing.flights.is_empty()
            && state.firing.grenades.is_empty()
            && state.firing.salvos.is_empty()
        {
            ending += 1;
            if ending == 8 {
                break;
            }
        }
    }
    assert!(thrown && salvo && live && resolved);
    assert_eq!(
        ending, 8,
        "all retained state expires before eight further ordinary ticks"
    );
    assert!(state.tick_state.fault().is_none());
}

#[test]
fn malformed_retained_payload_refuses_before_recipient_publication() {
    let mut state = scene();
    let shooter = actor(&mut state, "rifle", 1, Point { x: 130.0, z: 130.0 });
    let id = flight(&mut state, shooter);
    let mut delivery = DeliveryState::new(&state).unwrap();
    let before = delivery.clone();
    state
        .firing
        .flights
        .get_mut(&id)
        .unwrap()
        .solution
        .velocity
        .x = f64::NAN;
    assert!(
        delivery
            .capture_selected_with_lists(
                &mut state,
                &[SelectedSeat {
                    slot: 0,
                    full: true,
                    omit: false
                }],
                &[]
            )
            .is_err()
    );
    assert_eq!(delivery, before);
}
