use super::*;
use crate::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::{CELL, Point},
    map::AuthoritativeMap,
    match_runtime::{MatchOptions, PlayerConfig},
    recipient_snapshot::{SelectedSeat, SnapshotCache, SnapshotCapability, recipient_uncached},
    weather::WeatherChoice,
};
use serde_json::json;

fn scene() -> (Match, u32) {
    let map = AuthoritativeMap::from_value(json!({
        "name":"Operative recipient", "w":64, "h":64,
        "rows":vec![".".repeat(64);64],
        "spawns":[{"x":8,"y":8},{"x":8,"y":54},{"x":54,"y":54}],
        "points":[{"x":32,"y":32}],
    }))
    .unwrap();
    let mut state = Match::new(
        &map,
        MatchOptions {
            seed: 7301,
            mode: Mode::Conquest,
            players: (0..3)
                .map(|slot| PlayerConfig {
                    name: format!("P{slot}"),
                    team: if slot == 2 { 1 } else { 0 },
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
    .unwrap();
    for unit in state.world.game.units.values_mut() {
        unit.hold_pos = true;
    }
    for control in state.commands.units.values_mut() {
        control.hold_fire = true;
        control.auto_cast = false;
        control.auto_retreat = false;
    }
    let id = state
        .join_companion("operative", 0, false)
        .unwrap()
        .unwrap();
    (state, id)
}

#[test]
fn operative_status_matches_alive_prediction_squad_and_hidden_attacker_fields() {
    let (mut state, id) = scene();
    let squad = state
        .world
        .game
        .units
        .values()
        .find(|u| u.id != id && u.owner == 0 && state.rules.kinds[u.kind].name == "rifle")
        .unwrap()
        .id;
    let companion = state.companions.get_mut("operative").unwrap();
    companion.squad_id = squad;
    companion.last_seq = 9;
    companion.shots = 4;
    companion.hits = 3;
    companion.kills = 1;
    let companion_id = companion.id;
    state.commands.units.get_mut(&squad).unwrap().follow = companion_id;
    state.world.game.units.get_mut(&squad).unwrap().hp = 40.1;
    let unit = state.world.game.units.get_mut(&id).unwrap();
    unit.hp = 17.1;
    unit.x = 10.126;
    unit.z = 20.124;
    unit.vx = Some(-1.125);
    unit.vz = Some(2.126);
    // No visible attacker is needed: the prototype discloses these coordinates.
    unit.hit_from = Some(Point { x: -1.5, z: 80.5 });
    state.world.game.tick = 20;
    state.combat.get_mut(id).unwrap().hit_at = 10.;
    let body = state.companions.soldiers.get_mut(&id).unwrap();
    body.jy = 0.126;
    body.crouch = true;
    body.weapon = 1;
    body.mags = [7, 19];
    body.reload = 1.25;
    body.nades = 2;
    assert_eq!(
        state.companion_status("operative"),
        json!({
            "id":id, "hp":18., "maxHp":20., "respawn":0., "cost":10.,
            "squad":{"id":squad,"name":"Rifle Squad","men":3.}, "squadSpawn":true,
            "waiting":false,"seq":9,"shots":4,"hits":3,"kills":1,
            "x":10.13,"z":20.12,"vx":-1.12,"vz":2.13,"jy":0.13,
            "crouch":true,"weapon":1,"mags":[7,19],"reload":1.3,"nades":2,
            "from":[-1.,81.],
        })
    );
    state.world.game.tick = 21;
    assert_eq!(state.companion_status("operative")["from"], Value::Null);
    state.commands.units.get_mut(&squad).unwrap().follow = 0;
    assert_eq!(state.companion_status("operative")["squad"], Value::Null);
    assert_eq!(state.companion_status("operative")["squadSpawn"], false);
}

#[test]
fn operative_status_dead_waiting_and_squad_respawn_use_exact_optional_shape() {
    let (mut state, id) = scene();
    state.world.game.units.get_mut(&id).unwrap().hp = 0.;
    state.advance_tick().unwrap();
    state.start.economy.players[0].mp = 0.;
    assert_eq!(
        state.companion_status("operative"),
        json!({
            "id":0,"hp":0.,"maxHp":20.,"respawn":8.,"cost":10.,"squad":null,
            "squadSpawn":false,"waiting":true,"seq":-1,"shots":0,"hits":0,"kills":0,
        })
    );
    let squad = state
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 0 && state.rules.kinds[u.kind].name == "rifle")
        .unwrap()
        .id;
    let op = state.companions.get_mut("operative").unwrap();
    op.squad_id = squad;
    state.commands.units.get_mut(&squad).unwrap().follow = op.id;
    state.world.game.units.get_mut(&squad).unwrap().hp = 20.1;
    let status = state.companion_status("operative");
    assert_eq!(
        status["squad"],
        json!({"id":squad,"name":"Rifle Squad","men":2.})
    );
    assert_eq!(status["squadSpawn"], true);
    assert_eq!(status["waiting"], false);
    state.world.game.units.get_mut(&squad).unwrap().riding = true;
    assert_eq!(state.companion_status("operative")["waiting"], true);
    state.start.economy.players[0].mp = 10.;
    assert_eq!(state.companion_status("operative")["waiting"], false);
    assert_eq!(
        state.companion_status("missing"),
        json!({
            "id":0,"hp":0.,"maxHp":20.,"respawn":0.,"cost":10.,"squad":null,
            "squadSpawn":false,"waiting":true,"seq":-1,"shots":0,"hits":0,"kills":0,
        })
    );
}

#[test]
fn operative_rows_use_cached_masks_and_reject_unbacked_runtime_state() {
    let (mut state, id) = scene();
    state.world.game.reveal = true;
    let cache = SnapshotCache::new(&state).unwrap();
    for slot in 0..3 {
        let projection = cache.recipient(slot).unwrap();
        assert_eq!(projection, recipient_uncached(&state, slot).unwrap());
        let fields = projection.fields;
        let row = fields["units"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row[0] == id)
            .unwrap();
        assert_eq!(row.as_array().unwrap().len(), 15);
        assert_eq!(
            row[12],
            if slot == 0 {
                json!(131072 | 2048 | 4096)
            } else {
                json!(0)
            }
        );
    }
    state.companions.soldiers.remove(&id);
    assert!(matches!(
        recipient_uncached(&state, 0),
        Err(SnapshotError::Unsupported {
            capability: SnapshotCapability::Operatives,
            ..
        })
    ));
    assert!(matches!(
        SnapshotCache::new(&state),
        Err(SnapshotError::Unsupported {
            capability: SnapshotCapability::Operatives,
            ..
        })
    ));
}

#[test]
fn operative_shots_keep_prototype_nonlocal_and_local_identity_rules() {
    use crate::{
        match_firing::{LaunchEvent, SmallArmsEvent, SmallArmsHit},
        match_tick::TickEvent,
        projectiles::Point3,
    };
    let (mut state, id) = scene();
    let enemy = state
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 2 && state.rules.kinds[u.kind].name == "rifle")
        .unwrap();
    let target = enemy.id;
    let at = enemy.at();
    for player in &mut state.world.game.players {
        player.visible.clear();
    }
    let events = [
        TickEvent::Firing(LaunchEvent::SmallArms(SmallArmsEvent {
            effect: "sniper",
            shooter: id,
            owner: 0,
            at: Point3::new(at.x, 1.2, at.z),
            hit: Some(SmallArmsHit {
                target,
                owner: 2,
                man: Some(3),
            }),
        })),
        TickEvent::Firing(LaunchEvent::Throw {
            shooter: id,
            owner: 0,
            at,
        }),
    ];
    let selected = (0..3)
        .map(|slot| SelectedSeat {
            slot,
            full: false,
            omit: false,
        })
        .collect::<Vec<_>>();
    let mut delivery = DeliveryState::new(&state).unwrap();
    let frame = delivery
        .capture_selected_with_tick_events(&mut state, &selected, &events)
        .unwrap()
        .unwrap();
    for slot in 0..2 {
        assert_eq!(
            frame.recipient(slot).unwrap().into_json()["shots"],
            json!([
                {"k":"sniper","f":id,"fo":0,"x":at.x,"y":1.2,"z":at.z,"to":2,"hit":true,"m":3}
            ])
        );
    }
    assert_eq!(
        frame.recipient(2).unwrap().into_json()["shots"],
        json!([
            {"k":"sniper","fo":0,"x":at.x,"y":1.2,"z":at.z,"t":target,"to":2,"hit":true,"m":3},
            {"k":"throw","x":at.x,"z":at.z,"physical":true,"local":true}
        ])
    );
}

#[test]
fn operative_grenade_flight_and_launch_rows_match_literal_shapes_and_fog_rules() {
    use crate::{
        companions::Input,
        match_firing::LaunchEvent,
        match_tick::TickEvent,
        projectiles::{Flight, Point3},
    };
    let (mut state, id) = scene();
    let flight = state
        .throw_companion(
            id,
            &Input {
                forward: 0.,
                strafe: 0.,
                yaw: 0.,
                pitch: 0.,
                fire: false,
                ads: false,
                crouch: false,
                sprint: false,
                weapon: 0,
            },
            false,
        )
        .unwrap();
    let live = state.firing.flights.get_mut(&flight).unwrap();
    live.solution = Flight {
        origin: Point3::new(50., 2., 50.),
        velocity: Point3::new(4., 3., 0.),
        gravity: 2.,
        stamp: 0.,
    };
    live.launched = 0.;
    live.expires = 1.;
    let kind = live.source_kind;
    let launch = TickEvent::Firing(LaunchEvent::Flight {
        id: flight,
        kind,
        effect: Some("boom"),
        shooter: id,
        owner: 0,
        at: Point3::new(50., 2., 50.),
        velocity: Point3::new(4., 3., 0.),
        gravity: 2.,
        time: 0.,
        expires: 1.,
    });
    state.firing.events.clear();
    state.world.game.reveal = true;
    let cache = SnapshotCache::new(&state).unwrap();
    for slot in 0..3 {
        assert_eq!(
            cache.recipient(slot).unwrap(),
            recipient_uncached(&state, slot).unwrap()
        );
    }
    let mut delivery = DeliveryState::new(&state).unwrap();
    let frame = delivery
        .capture_selected_with_tick_events(
            &mut state,
            &[SelectedSeat {
                slot: 0,
                full: false,
                omit: false,
            }],
            &[launch],
        )
        .unwrap()
        .unwrap();
    let snapshot = frame.recipient(0).unwrap().into_json();
    assert_eq!(
        snapshot["flights"],
        json!([{
            "flight":flight,"kind":"rifle","effect":"boom","seq":0,"x":50.,"y":2.,"z":50.,
            "v":[4.,3.,0],"gravity":2.,"time":0,"launched":0,"expires":1.,"until":0.15,"f":id,"fo":0,
        }])
    );
    assert_eq!(
        snapshot["shots"],
        json!([{
            "k":"flight","flight":flight,"kind":"rifle","effect":"boom","seq":0,
            "x":50.,"y":2.,"z":50.,"v":[4.,3.,0],"gravity":2.,"time":0,"expires":1.,
            "local":true,"until":0.15,"f":id,"fo":0,
        }])
    );
    state.world.game.reveal = false;
    for player in &mut state.world.game.players {
        player.visible.clear();
    }
    let at = state
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 2 && state.rules.kinds[u.kind].name == "rifle")
        .unwrap()
        .at();
    state
        .firing
        .flights
        .get_mut(&flight)
        .unwrap()
        .solution
        .origin = Point3::new(at.x, 8., at.z);
    let cache = SnapshotCache::new(&state).unwrap();
    assert_eq!(cache.recipient(0).unwrap().fields["flights"], json!([]));
    assert_eq!(
        cache.recipient(2).unwrap().fields["flights"],
        json!([{
            "flight":flight,"kind":"rifle","effect":"boom","seq":0,"x":at.x,"y":8.,"z":at.z,
            "v":[4.,3.,0],"gravity":2.,"time":0,"launched":0,"expires":1.,"until":0.15,
        }])
    );
}

#[test]
fn mixed_ordinary_and_operative_grenades_keep_each_recipient_live_and_refuse_corruption() {
    use crate::{companions::Input, match_firing::AbilityBlast, match_tick::TickEvent};
    let (mut state, operative_id) = scene();
    let ordinary = state
        .world
        .game
        .units
        .values()
        .find(|u| u.id != operative_id && u.owner == 0 && state.rules.kinds[u.kind].name == "rifle")
        .unwrap();
    let ordinary_id = ordinary.id;
    let at = ordinary.at();
    let blast = AbilityBlast::from_unit(ordinary.kind, &state.rules).unwrap();
    let ordinary_flight = state
        .launch_ability_flight(ordinary_id, at, blast, None, None)
        .unwrap();
    let operative_flight = state
        .throw_companion(
            operative_id,
            &Input {
                forward: 0.,
                strafe: 0.,
                yaw: 0.,
                pitch: 0.,
                fire: false,
                ads: false,
                crouch: false,
                sprint: false,
                weapon: 0,
            },
            false,
        )
        .unwrap();
    state.world.game.reveal = true;
    let events = std::mem::take(&mut state.firing.events)
        .into_iter()
        .map(TickEvent::Firing)
        .collect::<Vec<_>>();
    let selected = (0..3)
        .map(|slot| SelectedSeat {
            slot,
            full: true,
            omit: false,
        })
        .collect::<Vec<_>>();
    let mut delivery = DeliveryState::new(&state).unwrap();
    let frame = delivery
        .capture_selected_with_tick_events(&mut state, &selected, &events)
        .unwrap()
        .unwrap();
    let expected = frame.recipient(0).unwrap().into_json()["flights"].clone();
    assert_eq!(expected.as_array().unwrap().len(), 2);
    for (flight, shooter) in [
        (ordinary_flight, ordinary_id),
        (operative_flight, operative_id),
    ] {
        let row = expected
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["flight"] == flight)
            .unwrap();
        assert_eq!(row["f"], json!(shooter));
        assert_eq!(row["fo"], json!(0));
        assert_eq!(row["effect"], json!("boom"));
    }
    for slot in 0..3 {
        assert_eq!(
            frame.recipient(slot).unwrap().into_json()["flights"],
            expected
        );
    }
    let mut operative = OperativeDelivery::new(&state, 0).unwrap();
    assert_eq!(
        operative.capture(&mut state, true, &events).unwrap()["flights"],
        expected
    );
    state
        .firing
        .flights
        .get_mut(&operative_flight)
        .unwrap()
        .effect = Some("shell");
    let before = delivery.clone();
    let world = format!("{:?}", state.world);
    assert!(matches!(
        delivery.capture_selected_with_tick_events(&mut state, &selected, &[]),
        Err(SnapshotError::Unsupported {
            capability: SnapshotCapability::ShotsAndFlights,
            ..
        })
    ));
    assert_eq!(delivery, before);
    assert_eq!(format!("{:?}", state.world), world);
    assert!(state.tick_state.fault().is_none());
}

#[test]
fn operative_delivery_keeps_commander_pending_memory_and_fog_independent_on_success_and_error() {
    let (mut state, id) = scene();
    let mut commander = DeliveryState::new(&state).unwrap();
    let selected = [SelectedSeat {
        slot: 0,
        full: false,
        omit: false,
    }];
    commander
        .capture_selected_with_tick_events(&mut state, &selected, &[])
        .unwrap()
        .unwrap();
    let mut operative = OperativeDelivery::new(&state, 0).unwrap();
    let mut second = OperativeDelivery::new(&state, 0).unwrap();
    let at = state.world.game.units[&id].at();
    let cell = (at.z / CELL) as usize * state.world.game.w + (at.x / CELL) as usize;
    state.world.set_cell(cell, b'T').unwrap();
    let pending = state.world.pending(Recipient::Player(0)).cloned();
    let remembered = state.recipients[0].remembered().to_vec();
    let fog = commander.seat_fog_for_test(0);
    let first = operative.capture(&mut state, true, &[]).unwrap();
    assert!(
        first["cells"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row[0] == cell)
    );
    assert!(first["fog"].is_object());
    assert_eq!(state.world.pending(Recipient::Player(0)), pending.as_ref());
    assert_eq!(state.recipients[0].remembered(), remembered);
    assert_eq!(commander.seat_fog_for_test(0), fog);
    let delivered = commander
        .capture_selected_with_tick_events(&mut state, &selected, &[])
        .unwrap()
        .unwrap()
        .recipient(0)
        .unwrap()
        .into_json();
    assert!(
        delivered["cells"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row[0] == cell)
    );
    // This peer starts after the commander's acknowledgment consumed the log.
    let peer = second.capture(&mut state, true, &[]).unwrap();
    assert_eq!(peer["cells"], first["cells"]);
    assert_eq!(peer["fog"], first["fog"]);
    let pending = state.world.pending(Recipient::Player(0)).cloned();
    let remembered = state.recipients[0].remembered().to_vec();
    state.companions.soldiers.get_mut(&id).unwrap().weapon = 2;
    assert!(operative.capture(&mut state, false, &[]).is_err());
    assert_eq!(state.world.pending(Recipient::Player(0)), pending.as_ref());
    assert_eq!(state.recipients[0].remembered(), remembered);
}

#[test]
fn operative_death_respawn_and_full_match_finish_keep_every_recipient_live() {
    let (mut state, first) = scene();
    // Grenades collide with allied bodies too. Use an open firing lane so the
    // deliberate death happens while this soldier's grenade is still airborne.
    let firing_lane = Point { x: 60., z: 40. };
    assert!(
        state
            .world
            .game
            .units
            .values()
            .filter(|u| u.id != first)
            .all(|u| { crate::math::hypot(u.x - firing_lane.x, u.z - firing_lane.z) > 30. })
    );
    let unit = state.world.game.units.get_mut(&first).unwrap();
    unit.x = firing_lane.x;
    unit.z = firing_lane.z;
    state.grid.update(first, unit);
    state.start.points[0].owner = Some(0);
    state.start.points[0].progress = 1.;
    state.start.points[0].vp = 1.;
    state.start.win_vp = 12.;
    let mut commander = DeliveryState::new(&state).unwrap();
    let mut operative = OperativeDelivery::new(&state, 0).unwrap();
    let selected = (0..3)
        .map(|slot| SelectedSeat {
            slot,
            full: false,
            omit: false,
        })
        .collect::<Vec<_>>();
    let mut dead = false;
    let mut respawned = false;
    let mut saw_fire = false;
    let mut saw_throw = false;
    let mut saw_flight = false;
    let mut finish_tick = None;
    for _ in 0..400 {
        if state.world.game.tick < 2 {
            let tick = state.world.game.tick;
            assert!(state.companion_input(
                "operative",
                &json!({
                    "seq":tick+1,"forward":0.,"strafe":0.,"yaw":0.,"pitch":0.,
                    "fire":tick==0,"nade":tick,"weapon":0,
                }),
                false
            ));
        }
        if state.world.game.tick == 3 {
            assert!(
                state
                    .firing
                    .flights
                    .values()
                    .any(|flight| flight.shooter == first)
            );
            state.world.game.units.get_mut(&first).unwrap().hp = 0.;
        }
        let report = state.advance_tick().unwrap();
        let id = state.companions.get("operative").unwrap().unit_id;
        dead |= id == 0;
        respawned |= id != 0 && id != first;
        let frame = commander
            .capture_selected_with_tick_events(&mut state, &selected, &report.events)
            .unwrap()
            .unwrap();
        for slot in 0..3 {
            let snapshot = frame.recipient(slot).unwrap().into_json();
            saw_fire |= snapshot["shots"]
                .as_array()
                .unwrap()
                .iter()
                .any(|shot| shot["k"] == "sniper");
            saw_throw |= snapshot["shots"]
                .as_array()
                .unwrap()
                .iter()
                .any(|shot| shot["k"] == "throw");
            saw_flight |= !snapshot["flights"].as_array().unwrap().is_empty();
        }
        let snapshot = operative
            .capture(&mut state, false, &report.events)
            .unwrap();
        if id != 0 {
            assert!(
                snapshot["units"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|row| row[0] == id)
            );
        }
        assert!(state.tick_state.fault().is_none());
        if state.finish.is_some() {
            let start = *finish_tick.get_or_insert(state.world.game.tick);
            if state.world.game.tick >= start + 8 {
                break;
            }
        }
    }
    assert!(dead && respawned && saw_fire && saw_throw && saw_flight);
    assert_eq!(state.finish.as_ref().unwrap().winner, 0);
    assert!(state.world.game.tick >= finish_tick.unwrap() + 8);
}
