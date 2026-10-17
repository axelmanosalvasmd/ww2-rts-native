//! Retreat destinations and movement through canonical native state.
use super::*;
use crate::{
    game::PlaneState,
    match_commands::{Command, MoveOutcome, UnitControl},
    misc_retreat_support_tests::{delivered_tick, publish_rifle, scene, seats},
    recipient_snapshot::{DeliveryState, SelectedSeat, SnapshotCache, SnapshotError},
    supply_convoys::{ConvoyState, Store, StoreId, Truck, TruckState},
};

fn building(m: &mut Match, name: &str, owner: usize, at: Point) -> u32 {
    let id = m.next_unit_id;
    m.next_unit_id += 1;
    let mut u = crate::unit_spawn::initial_unit(
        &m.rules,
        Catalog::compiled().unwrap(),
        m.rules.kind(name).unwrap(),
        id,
        owner,
        at,
    )
    .unwrap();
    if m.rules.kinds[u.kind].building {
        u.built = Some(1.);
    }
    m.commands.insert(
        id,
        UnitControl::new_for_mode(&u, &m.rules, m.options.mode).unwrap(),
    );
    m.firing.initialize(id);
    m.combat.initialize(id);
    m.abilities.initialize(id);
    m.recovery.initialize(id);
    m.world.game.units.insert(id, u);
    m.grid.rebuild(&m.world.game.units);
    id
}

fn long_route_scene(mode: Mode) -> Match {
    let map = crate::map::AuthoritativeMap::from_json(
        &serde_json::to_vec(&serde_json::json!({
            "name":"Retreat through route legs", "w":160, "h":96,
            "rows":vec![".".repeat(160);96],
            "heights":vec!["0".repeat(160);96],
            "spawns":[{"x":8,"y":8},{"x":145,"y":80}],
            "points":[{"id":"neutral","x":80,"y":48,"mp":0,"vp":0}]
        }))
        .unwrap(),
    )
    .unwrap();
    let options = scene(mode).options;
    if mode == Mode::Classic {
        Match::new_classic(&map, options)
    } else {
        Match::new(&map, options)
    }
    .unwrap()
}

#[test]
fn base57_skirmish_nearest_finished_accessible_hq_and_spawn_fallback() {
    let mut m = scene(Mode::Conquest);
    let id = publish_rifle(&mut m, Point { x: 100., z: 100. });
    let spawn = m.start.players[0].spawn;
    for u in m
        .world
        .game
        .units
        .values_mut()
        .filter(|u| m.rules.kinds[u.kind].name == "hq")
    {
        u.built = Some(0.5);
    }
    let far = building(&mut m, "hq", 0, Point { x: 70., z: 100. });
    let near = building(&mut m, "hq", 0, Point { x: 90., z: 100. });
    let unfinished = building(&mut m, "hq", 0, Point { x: 99., z: 100. });
    m.world.game.units.get_mut(&unfinished).unwrap().built = Some(0.99);
    let dead = building(&mut m, "hq", 0, Point { x: 98., z: 100. });
    m.world.game.units.get_mut(&dead).unwrap().hp = 0.;
    building(&mut m, "hq", 1, Point { x: 97., z: 100. });
    building(&mut m, "barracks", 0, Point { x: 96., z: 100. });
    for mode in [
        Mode::Default,
        Mode::Conquest,
        Mode::Assault,
        Mode::Annihilation,
    ] {
        m.options.mode = mode;
        assert_eq!(m.reaction_home(id).unwrap(), m.world.game.units[&near].at());
    }
    m.options.mode = Mode::Conquest;
    m.world.game.units.get_mut(&near).unwrap().built = Some(0.);
    assert_eq!(m.reaction_home(id).unwrap(), m.world.game.units[&far].at());
    m.world.game.units.get_mut(&far).unwrap().hp = 0.;
    assert_eq!(m.reaction_home(id).unwrap(), spawn);
}

#[test]
fn base57_horde_shared_defender_hq_excludes_attacker_and_enemy() {
    let mut m = scene(Mode::Conquest);
    let id = publish_rifle(&mut m, Point { x: 100., z: 100. });
    m.world.game.players[1].team = m.world.game.players[0].team;
    let ally = building(&mut m, "hq", 1, Point { x: 95., z: 100. });
    let unit = &m.world.game.units[&id];
    let select = |attacker| {
        ground_home(
            Mode::Horde,
            unit,
            &m.world.game.units,
            &m.rules,
            m.start.players[0].spawn,
            &m.world.game.players,
            Some(attacker),
        )
        .unwrap()
    };
    assert_eq!(select(2), m.world.game.units[&ally].at());
    assert_ne!(select(1), m.world.game.units[&ally].at());
    assert_ne!(select(0), m.world.game.units[&ally].at());
    m.world.game.players[1].team = 1;
    assert_ne!(
        ground_home(
            Mode::Horde,
            &m.world.game.units[&id],
            &m.world.game.units,
            &m.rules,
            m.start.players[0].spawn,
            &m.world.game.players,
            Some(2)
        )
        .unwrap(),
        m.world.game.units[&ally].at()
    );
}

#[test]
fn base57_classic_nearest_own_finished_production_and_spawn_fallback() {
    let mut m = scene(Mode::Classic);
    let id = publish_rifle(&mut m, Point { x: 100., z: 100. });
    let spawn = m.start.players[0].spawn;
    for u in m
        .world
        .game
        .units
        .values_mut()
        .filter(|u| m.rules.kinds[u.kind].building)
    {
        u.built = Some(0.);
    }
    let far = building(&mut m, "motorpool", 0, Point { x: 70., z: 100. });
    let near = building(&mut m, "barracks", 0, Point { x: 90., z: 100. });
    building(&mut m, "armory", 0, Point { x: 99., z: 100. });
    building(&mut m, "barracks", 1, Point { x: 98., z: 100. });
    m.world.game.players[1].team = 0;
    let unfinished = building(&mut m, "airfield", 0, Point { x: 97., z: 100. });
    m.world.game.units.get_mut(&unfinished).unwrap().built = Some(0.99);
    let dead = building(&mut m, "barracks", 0, Point { x: 96., z: 100. });
    m.world.game.units.get_mut(&dead).unwrap().hp = 0.;
    assert_eq!(m.reaction_home(id).unwrap(), m.world.game.units[&near].at());
    m.world.game.units.get_mut(&near).unwrap().hp = 0.;
    assert_eq!(m.reaction_home(id).unwrap(), m.world.game.units[&far].at());
    m.world.game.units.get_mut(&far).unwrap().hp = 0.;
    assert_eq!(m.reaction_home(id).unwrap(), spawn);
}

fn store(id: u32, owner: usize, x: f64, active: bool, source: bool) -> Store {
    Store {
        id: StoreId::Source(id),
        owner,
        at: Point { x, z: 100. },
        active,
        source,
        entity: None,
        capacity: Default::default(),
        buckets: vec![],
        donations: vec![],
        support: false,
        point: None,
    }
}

#[test]
fn base57_convoy_nearest_active_allied_source_precedes_mode_home_with_fallback() {
    let mut m = scene(Mode::Conquest);
    let id = publish_rifle(&mut m, Point { x: 100., z: 100. });
    let ordinary = m.reaction_home(id).unwrap();
    let mut state = ConvoyState::default();
    state.trucks.insert(
        id,
        Truck {
            state: TruckState::Idle,
            cargo: Default::default(),
            target: None,
            origin: StoreId::Source(0),
            timer: 0.,
            manual: false,
            hold: false,
            route: vec![],
            destination: None,
            route_failure: None,
            retry: None,
        },
    );
    state.stores = vec![
        store(1, 0, 50., true, true),
        store(2, 1, 90., true, true),
        store(3, 0, 99., false, true),
        store(4, 0, 98., true, false),
    ];
    m.convoys = Some(state);
    assert_eq!(m.reaction_home(id).unwrap(), Point { x: 50., z: 100. });
    m.world.game.players[1].team = 0;
    assert_eq!(m.reaction_home(id).unwrap(), Point { x: 90., z: 100. });
    // The source's stored origin does not override the nearest active allied source.
    m.convoys.as_mut().unwrap().stores[0].at.x = 110.;
    assert_eq!(m.reaction_home(id).unwrap(), Point { x: 110., z: 100. });
    m.convoys.as_mut().unwrap().stores[0].active = false;
    m.convoys.as_mut().unwrap().stores[1].active = false;
    assert_eq!(m.reaction_home(id).unwrap(), ordinary);
}

#[test]
fn base57_world_own_territory_production_then_nearest_own_region_component() {
    let authored: serde_json::Value =
        serde_json::from_str(include_str!("../../tests/fixtures/world-recovery.json")).unwrap();
    let map =
        crate::map::AuthoritativeMap::from_json(&serde_json::to_vec(&authored["map"]).unwrap())
            .unwrap();
    let mut options = scene(Mode::Conquest).options;
    options.mode = Mode::World;
    let mut m = Match::new_world(&map, options, crate::random::MatchRandom::new(701)).unwrap();
    for unit in m.world.game.units.values_mut() {
        if m.rules.kinds[unit.kind].building {
            unit.built = Some(0.);
        }
    }
    let regions = &m.world.geography().unwrap().regions;
    let sites = regions[..3]
        .iter()
        .map(|r| Point {
            x: (r.x as f64 + 0.5) * 2.,
            z: (r.y as f64 + 0.5) * 2.,
        })
        .collect::<Vec<_>>();
    for region in &mut m.world_runtime.as_mut().unwrap().regions {
        region.team = 1;
    }
    m.world_runtime.as_mut().unwrap().regions[0].team = 0;
    m.world_runtime.as_mut().unwrap().regions[1].team = 0;
    let id = building(&mut m, "rifle", 0, sites[2]);
    let far = building(&mut m, "barracks", 0, sites[0]);
    let near = building(&mut m, "motorpool", 0, sites[1]);
    building(&mut m, "barracks", 0, sites[2]);
    m.world.game.players[1].team = 0;
    m.start.players[1].config.team = 0;
    building(&mut m, "barracks", 1, sites[2]);
    assert_eq!(m.world_region_at(sites[2]).unwrap().unwrap().team, 1);
    assert_eq!(m.reaction_home(id).unwrap(), sites[1]);
    m.world.game.units.get_mut(&near).unwrap().built = Some(0.99);
    assert_eq!(m.reaction_home(id).unwrap(), sites[0]);
    m.world.game.units.get_mut(&far).unwrap().hp = 0.;
    assert_eq!(m.reaction_home(id).unwrap(), sites[1]);
    m.world_runtime.as_mut().unwrap().regions[1].team = 1;
    assert_eq!(m.reaction_home(id).unwrap(), sites[0]);
    // This calls only the home component. World room admission stays closed.
    assert!(matches!(
        SnapshotCache::new(&m),
        Err(SnapshotError::UnsupportedMode)
    ));
}

#[test]
fn mov48_retreat_actual_ground_displacement_is_one_and_a_half_normal_speed() {
    let mut retreat = scene(Mode::Conquest);
    let mut normal = scene(Mode::Conquest);
    let at = Point { x: 100., z: 100. };
    let home = Point { x: 40., z: 100. };
    let id = publish_rifle(&mut retreat, at);
    assert_eq!(publish_rifle(&mut normal, at), id);
    for m in [&mut retreat, &mut normal] {
        let unit = m.world.game.units.get_mut(&id).unwrap();
        unit.path = vec![home];
        unit.rot = std::f64::consts::PI;
    }
    retreat.world.game.units.get_mut(&id).unwrap().retreating = true;
    crate::movement::move_unit(
        &mut retreat.world.game,
        &retreat.rules,
        &mut retreat.grid,
        id,
        1.0,
    )
    .unwrap();
    crate::movement::move_unit(
        &mut normal.world.game,
        &normal.rules,
        &mut normal.grid,
        id,
        1.0,
    )
    .unwrap();
    let ordinary_moved = distance(at, normal.world.game.units[&id].at());
    let retreat_moved = distance(at, retreat.world.game.units[&id].at());
    assert!(ordinary_moved > 0.);
    assert!((retreat_moved / ordinary_moved - 1.5).abs() < 1e-10);
}

#[test]
fn mov48_manual_retreat_exits_house_moves_home_and_delivers_every_seat() {
    let mut m = scene(Mode::Conquest);
    let cell = 50 * m.world.game.w + 50;
    m.world.set_cell(cell, b'B').unwrap();
    let at = Point { x: 101., z: 101. };
    let id = publish_rifle(&mut m, at);
    m.world.game.units.get_mut(&id).unwrap().garrison = Some(cell);
    m.world.game.units.get_mut(&id).unwrap().hold_pos = true;
    m.commands.units.get_mut(&id).unwrap().hold_fire = true;
    let home = m.reaction_home(id).unwrap();
    let selected = seats(&m);
    let mut delivery = DeliveryState::new(&m).unwrap();
    assert_eq!(
        m.execute_command(
            0,
            &Command::Retreat {
                ids: vec![i64::from(id)]
            },
            false
        )
        .unwrap(),
        None
    );
    let exited = m.world.game.units[&id].at();
    assert!(m.world.game.units[&id].garrison.is_none());
    assert_ne!(exited, at);
    assert!(m.world.game.units[&id].retreating);
    let mut moved = false;
    for _ in 0..1000 {
        delivered_tick(&mut m, &mut delivery, &selected);
        moved |= m.world.game.units[&id].at() != exited;
        if !m.world.game.units[&id].retreating {
            break;
        }
    }
    let u = &m.world.game.units[&id];
    assert!(moved && !u.retreating && u.world_goal.is_none());
    assert!(distance(u.at(), home) <= 15.);
    assert_eq!(m.commands.units[&id].outcome, Some(MoveOutcome::Arrived));
    assert!(m.tick_state.fault().is_none());
}

fn classic_retreat_complete_tick(m: &mut Match, delivery: &mut DeliveryState, id: u32) {
    let report = m.advance_tick().unwrap();
    let mode = m.elimination.as_ref().unwrap();
    let expected = serde_json::json!({
        "kind":"classic",
        "timeLeft":crate::math::max(0.0, mode.time_left.unwrap().ceil()),
        "suddenDeath":mode.sudden_death
    });
    let count = m.start.players.len();
    let retreating = m.world.game.units[&id].retreating;
    let selected = seats(m);
    let frame = delivery
        .capture_selected_with_tick_events(m, &selected, &report.events)
        .unwrap()
        .unwrap();
    assert_eq!(
        frame.slots().collect::<Vec<_>>(),
        (0..count).collect::<Vec<_>>()
    );
    for slot in 0..count {
        let snapshot = frame.recipient(slot).unwrap().into_json();
        assert_eq!(snapshot["t"], "s");
        assert_eq!(snapshot["mode"], expected, "Classic seat {slot}");
        assert!(snapshot.get("missing").is_none());
        for field in ["wrecks", "foundMines", "shots", "flights"] {
            assert!(snapshot[field].is_array(), "Classic seat {slot}: {field}");
        }
    }
    let snapshot = frame.recipient(0).unwrap().into_json();
    let row = snapshot["units"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row[0] == serde_json::json!(id))
        .unwrap()
        .as_array()
        .unwrap();
    assert_eq!(
        &row[..3],
        &[
            serde_json::json!(id),
            serde_json::json!("rifle"),
            serde_json::json!(0),
        ]
    );
    assert_eq!(row[12].as_u64().unwrap() & 1, u64::from(retreating));
}

fn retreat_through_legs(mode: Mode) {
    let mut m = long_route_scene(mode);
    let at = Point { x: 281., z: 41. };
    let id = publish_rifle(&mut m, at);
    m.world.game.units.get_mut(&id).unwrap().hold_pos = true;
    m.commands.units.get_mut(&id).unwrap().hold_fire = true;
    let home = m.reaction_home(id).unwrap();
    let selected = seats(&m);
    if mode == Mode::Classic {
        assert_eq!(
            crate::recipient_snapshot::mode_row(&m).unwrap(),
            serde_json::json!({"kind":"classic","timeLeft":1500.0,"suddenDeath":false})
        );
    }
    let mut delivery = DeliveryState::new(&m).unwrap();
    assert_eq!(
        m.execute_command(
            0,
            &Command::Retreat {
                ids: vec![i64::from(id)]
            },
            false
        )
        .unwrap(),
        None
    );
    let first_leg = *m.world.game.units[&id].path.last().unwrap();
    assert!(distance(first_leg, home) > 64.);
    assert!(distance(at, first_leg) <= 65.);
    let mut reached_first_leg = false;
    let mut leg_ends = BTreeSet::new();
    for _ in 0..2500 {
        if mode == Mode::Classic {
            classic_retreat_complete_tick(&mut m, &mut delivery, id);
        } else {
            delivered_tick(&mut m, &mut delivery, &selected);
        }
        let u = &m.world.game.units[&id];
        if let Some(end) = u.path.last() {
            leg_ends.insert((end.x.to_bits(), end.z.to_bits()));
        }
        reached_first_leg |= distance(u.at(), first_leg) < 2.;
        if u.retreating {
            assert_eq!(u.world_goal, Some(home));
            assert_eq!(m.firing.get(id).unwrap().target_id, 0);
        }
        if !m.world.game.units[&id].retreating {
            break;
        }
    }
    let u = &m.world.game.units[&id];
    assert!(reached_first_leg, "the first bounded leg must be traversed");
    assert!(leg_ends.len() >= 3, "retreat must continue onto later legs");
    assert!(!u.retreating && u.world_goal.is_none());
    assert!(distance(u.at(), home) <= 15.);
    assert_eq!(m.commands.units[&id].outcome, Some(MoveOutcome::Arrived));
    assert!(m.tick_state.fault().is_none());
    for _ in 0..3 {
        if mode == Mode::Classic {
            classic_retreat_complete_tick(&mut m, &mut delivery, id);
        } else {
            delivered_tick(&mut m, &mut delivery, &selected);
        }
    }
    assert!(m.tick_state.fault().is_none());
}

#[test]
fn mov48_ordinary_retreat_traverses_multiple_legs_and_delivers_every_seat() {
    for mode in [Mode::Default, Mode::Conquest] {
        retreat_through_legs(mode);
    }
}

#[test]
fn mov48_classic_retreat_traverses_multiple_legs_with_every_seat_complete_delivery() {
    retreat_through_legs(Mode::Classic);
}

#[test]
fn mov48_aircraft_retreat_flies_home_and_rearms_through_ticks_and_all_seats() {
    let mut m = scene(Mode::Conquest);
    let id = m.spawn_aircraft(0, "fighter").unwrap();
    let at = Point { x: 100., z: 100. };
    let u = m.world.game.units.get_mut(&id).unwrap();
    u.x = at.x;
    u.z = at.z;
    u.air = Some(PlaneState::Out);
    m.grid.rebuild(&m.world.game.units);
    let home = m.aircraft_base(0, at).unwrap();
    let selected = seats(&m);
    let mut delivery = DeliveryState::new(&m).unwrap();
    assert_eq!(
        m.execute_command(
            0,
            &Command::Retreat {
                ids: vec![i64::from(id)]
            },
            false
        )
        .unwrap(),
        None
    );
    assert_eq!(m.world.game.units[&id].air, Some(PlaneState::Home));
    let mut prior = distance(at, home);
    let mut moved = false;
    for _ in 0..500 {
        delivered_tick(&mut m, &mut delivery, &selected);
        let u = &m.world.game.units[&id];
        let remaining = distance(u.at(), home);
        assert!(remaining <= prior);
        moved |= remaining < prior;
        prior = remaining;
        if u.air == Some(PlaneState::Rearm) {
            break;
        }
    }
    assert!(moved);
    assert_eq!(m.world.game.units[&id].air, Some(PlaneState::Rearm));
    assert!(prior < 6.);
    assert!(m.aviation.planes[&id].mission.is_none());
    assert!(m.aviation.planes[&id].timer > 0.);
    for _ in 0..3 {
        delivered_tick(&mut m, &mut delivery, &selected);
    }
    assert!(m.tick_state.fault().is_none());
}
