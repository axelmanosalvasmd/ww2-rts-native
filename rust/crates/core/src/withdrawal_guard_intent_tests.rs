//! LOG-14 guards over canonical reserves and ordinary logistics delivery. World stays guarded.
use crate::{
    catalog::Catalog,
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::{CELL, LOCAL_DEFENDER_OWNER, PlaneState, Point, Strike, StrikeKind},
    logistics::Reserves,
    map::AuthoritativeMap,
    match_commands::UnitControl,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    match_support::{SupportEvent, SupportKind, SupportTiming},
    random::MatchRandom,
    recipient_snapshot::{DeliveryState, SelectedSeat, SnapshotCache, SnapshotCapability},
    unit_spawn::initial_unit,
    weather::WeatherChoice,
    world_generation::{WorldSize, generate_world},
};
use serde_json::json;

fn point(x: f64, z: f64) -> Point {
    Point { x, z }
}
fn options(mode: Mode) -> MatchOptions {
    MatchOptions {
        seed: 1821,
        mode,
        players: [0, 1]
            .into_iter()
            .map(|team| PlayerConfig {
                name: format!("Commander {team}"),
                team,
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
    }
}
fn scene() -> Match {
    scene_with_logistics(false)
}
fn scene_with_logistics(enabled: bool) -> Match {
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"withdrawal guards", "w":80, "h":80,
            "rows":vec![".".repeat(80);80],
            "spawns":[{"x":8,"y":8},{"x":70,"y":70}],
            "points":[{"x":40,"y":40,"vp":0,"mp":0}]
        }))
        .unwrap(),
    )
    .unwrap();
    let mut opts = options(Mode::Conquest);
    opts.logistics = enabled;
    let mut m = Match::new(&map, opts).unwrap();
    for control in m.commands.units.values_mut() {
        control.auto_cast = false;
        control.auto_retreat = false;
        control.hold_fire = true;
    }
    m
}
fn add(m: &mut Match, name: &str, owner: usize, at: Point) -> u32 {
    let id = m.next_unit_id;
    m.next_unit_id += 1;
    let mut unit = initial_unit(
        &m.rules,
        Catalog::compiled().unwrap(),
        m.rules.kind(name).unwrap(),
        id,
        owner,
        at,
    )
    .unwrap();
    unit.still = Some(10.0);
    let mut control = UnitControl::new(&unit, &m.rules).unwrap();
    control.auto_cast = false;
    control.auto_retreat = false;
    m.commands.insert(id, control);
    m.combat.initialize(id);
    m.firing.initialize(id);
    m.abilities.initialize(id);
    m.recovery.initialize(id);
    if m.options.logistics
        && let Some(reserves) = m.prepare_new_unit_reserves(&unit).unwrap()
    {
        m.logistics.insert(id, reserves);
    }
    m.world.game.units.insert(id, unit);
    m.grid.rebuild(&m.world.game.units);
    id
}
fn force(m: &mut Match, id: u32, stranded: bool) {
    m.logistics.insert(
        id,
        Reserves {
            ammo: 100.0,
            ammo_max: 100.0,
            ammo_price: 1.0,
            ammo_credit: None,
            provisions: 0.0,
            fuel: None,
            emergency: None,
            warning: None,
            forced: true,
            stranded,
            exhausted: true,
            supply_rate: None,
            cut_for: None,
        },
    );
}
fn order(m: &Match) -> Vec<u32> {
    m.world.game.units.keys().copied().collect()
}

#[test]
fn log14_forced_progress_resync_requires_path_and_no_supply_hold() {
    let mut m = scene();
    let id = add(&mut m, "rifle", 0, point(70.0, 70.0));
    force(&mut m, id, false);
    m.world.game.units.get_mut(&id).unwrap().path = vec![point(100.0, 70.0)];
    for (stuck, expected) in [(1.0, false), (1.001, true), (0.0, false)] {
        m.world.game.units.get_mut(&id).unwrap().stuck = stuck;
        m.sync_supply_withdrawal_progress(id);
        assert_eq!(m.logistics.get(id).unwrap().stranded, expected);
        assert_eq!(m.world.game.units[&id].retreating, !expected);
    }
    for guard in ["hold", "empty path", "not forced"] {
        let mut guarded = scene();
        let id = add(&mut guarded, "rifle", 0, point(70.0, 70.0));
        force(&mut guarded, id, false);
        let unit = guarded.world.game.units.get_mut(&id).unwrap();
        unit.stuck = 2.0;
        unit.retreating = true;
        unit.path = vec![point(100.0, 70.0)];
        match guard {
            "hold" => guarded.commands.units.get_mut(&id).unwrap().supply_hold = true,
            "empty path" => guarded.world.game.units.get_mut(&id).unwrap().path.clear(),
            "not forced" => guarded.logistics.get_mut(id).unwrap().forced = false,
            _ => unreachable!(),
        }
        guarded.sync_supply_withdrawal_progress(id);
        assert!(!guarded.logistics.get(id).unwrap().stranded, "{guard}");
        assert!(guarded.world.game.units[&id].retreating, "{guard}");
    }
}

#[test]
fn log14_forced_progress_survives_ordinary_ticks_and_each_seat_snapshot() {
    let mut m = scene_with_logistics(true);
    let id = add(&mut m, "rifle", 0, point(70.0, 70.0));
    force(&mut m, id, false);
    let unit = m.world.game.units.get_mut(&id).unwrap();
    unit.path = vec![point(100.0, 70.0)];
    unit.world_goal = Some(point(100.0, 70.0));
    unit.stuck = 1.01;
    unit.retreating = true;
    m.commands.units.get_mut(&id).unwrap().supply_started = true;
    m.commands.units.get_mut(&id).unwrap().route_end = Some(point(100.0, 70.0));
    let before = m.world.game.units[&id].x;
    let selected = (0..m.start.players.len())
        .map(|slot| SelectedSeat {
            slot,
            full: false,
            omit: false,
        })
        .collect::<Vec<_>>();
    let mut delivery = DeliveryState::new(&m).unwrap();
    for tick in 1..=4 {
        let report = m.advance_tick().unwrap();
        assert_eq!(m.tick_state.fault(), None);
        assert_eq!(m.logistics.get(id).unwrap().stranded, tick == 1);
        assert_eq!(m.world.game.units[&id].retreating, tick != 1);
        {
            let cache = SnapshotCache::new(&m).unwrap();
            for seat in 0..m.start.players.len() {
                let projection = cache.recipient(seat).unwrap();
                assert!(!projection.missing.contains(&SnapshotCapability::Logistics));
            }
        }
        let reserves_before_delivery = m.logistics.get(id).unwrap().clone();
        let frame = delivery
            .capture_selected_with_tick_events(&mut m, &selected, &report.events)
            .unwrap()
            .unwrap();
        for seat in &selected {
            let snapshot = frame.recipient(seat.slot).unwrap().into_json();
            assert_eq!(snapshot["t"], "s");
            let reserves = snapshot["logistics"]["units"].as_array().unwrap();
            let own = reserves.iter().find(|row| row["id"] == id);
            if seat.slot == 0 {
                let reserve = own.expect("owner receives its forced withdrawal reserve");
                assert_eq!(reserve["forced"], true);
                assert_eq!(reserve["stranded"], tick == 1);
                let unit = snapshot["units"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|row| row[0] == id)
                    .unwrap();
                assert_eq!(unit[12].as_u64().unwrap() & 1 != 0, tick != 1);
            } else {
                assert!(own.is_none(), "nonowners never receive the forced reserve");
            }
        }
        assert_eq!(m.logistics.get(id).unwrap(), &reserves_before_delivery);
    }
    assert!(m.world.game.units[&id].x > before);
    // A mismatched option remains a read-only refusal after supported ticks.
    m.options.logistics = false;
    let tick = m.world.game.tick;
    assert!(
        m.validate_tick_state()
            .unwrap_err()
            .contains("logistics option and convoy state disagree")
    );
    assert_eq!(m.world.game.tick, tick);
}

#[test]
fn log14_forced_point_capturers_do_not_capture_or_contest_mixed_groups() {
    let mut m = scene();
    let at = m.start.points[0].at;
    let forced = add(&mut m, "rifle", 0, at);
    force(&mut m, forced, true);
    let ids = order(&m);
    m.apply_point_capture_income(&ids).unwrap();
    assert_eq!(m.start.points[0].progress, 0.0);
    assert!(m.start.points[0].on_point.is_empty());
    let rival = add(&mut m, "rifle", 1, at);
    let ids = order(&m);
    m.apply_point_capture_income(&ids).unwrap();
    assert_eq!(m.start.points[0].on_point, vec![rival]);
    assert!(!m.start.points[0].contested);
    assert!(m.start.points[0].progress > 0.0);
    m.logistics.get_mut(forced).unwrap().forced = false;
    m.apply_point_capture_income(&ids).unwrap();
    assert!(m.start.points[0].contested);
    assert_eq!(m.start.points[0].on_point, vec![forced, rival]);
}

#[test]
fn log14_forced_units_ignore_enemy_reactions_and_retreating_units_do_not_fire() {
    let mut m = scene();
    let id = add(&mut m, "rifle", 0, point(70.0, 70.0));
    let enemy = add(&mut m, "rifle", 1, point(80.0, 70.0));
    m.world.game.players[0].visible = vec![enemy];
    m.world.game.units.get_mut(&id).unwrap().hit_from = Some(point(80.0, 70.0));
    m.combat.get_mut(id).unwrap().hit_at = 0.0;
    m.combat.get_mut(id).unwrap().hit_by = Some(enemy);
    force(&mut m, id, true);
    m.react_unit(id).unwrap();
    assert_eq!(m.commands.units[&id].react, 0.0);
    m.logistics.get_mut(id).unwrap().forced = false;
    m.react_unit(id).unwrap();
    assert_eq!(m.commands.units[&id].react, 2.0);
    m.logistics.get_mut(id).unwrap().forced = true;
    m.logistics.get_mut(id).unwrap().stranded = false;
    m.world.game.units.get_mut(&id).unwrap().retreating = true;
    let ids = order(&m);
    m.begin_firing_tick(&ids).unwrap();
    m.apply_unit_firing(id, false, 0.05).unwrap();
    assert_eq!(m.firing.get(id).unwrap().target_id, 0);
    assert!(m.firing.take_events().is_empty());
    m.world.game.units.get_mut(&id).unwrap().retreating = false;
    m.logistics.get_mut(id).unwrap().stranded = true;
    m.apply_unit_firing(id, false, 0.05).unwrap();
    assert_eq!(m.firing.get(id).unwrap().target_id, enemy);
    assert!(!m.firing.take_events().is_empty());
}

#[test]
fn log14_forced_aa_and_flak_fire_only_when_stranded() {
    for stranded in [false, true] {
        let mut m = scene();
        let gun = add(&mut m, "flak", 0, point(70.0, 70.0));
        force(&mut m, gun, stranded);
        let target = m.spawn_aircraft(1, "attacker").unwrap();
        let plane = m.world.game.units.get_mut(&target).unwrap();
        plane.air = Some(PlaneState::Station);
        plane.x = 80.0;
        plane.z = 70.0;
        m.world.game.players[0].visible = vec![target];
        m.grid.rebuild(&m.world.game.units);
        let hp = m.world.game.units[&target].hp;
        m.step_aircraft_anti_air(&[gun], 0.1).unwrap();
        assert_eq!(m.world.game.units[&target].hp < hp, stranded);
        assert_eq!(m.aviation.take_events().is_empty(), !stranded);

        m.world.game.strikes.push(Strike {
            kind: StrikeKind::Recon,
            owner: 1,
            x: 70.0,
            z: 70.0,
            dir: 0.0,
            live: false,
            runtime: Some(SupportTiming {
                kind: SupportKind::Recon,
                delay: 0.0,
                left: 1.0,
                next: 0.0,
            }),
        });
        m.step_support_strikes(&[gun], 0.05).unwrap();
        assert_eq!(
            m.support
                .take_events()
                .iter()
                .any(|event| matches!(event, SupportEvent::Flak { unit, .. } if *unit == gun)),
            stranded
        );
    }
}

#[test]
fn log14_withdrawal_entry_drops_pending_grenades_and_forced_chases_are_cancelled() {
    let mut m = scene();
    let id = add(&mut m, "rifle", 0, point(70.0, 70.0));
    force(&mut m, id, false);
    m.commands.units.get_mut(&id).unwrap().nade = Some(point(120.0, 70.0));
    m.step_aimed_ability(id).unwrap();
    assert_eq!(m.commands.units[&id].nade, None);
    m.commands.units.get_mut(&id).unwrap().nade = Some(point(72.0, 70.0));
    m.commands.units.get_mut(&id).unwrap().supply_hold = true;
    m.advance_supply_withdrawal(id).unwrap();
    assert!(m.commands.units[&id].supply_started);
    assert_eq!(m.commands.units[&id].nade, None);
    assert!(m.logistics.get(id).unwrap().stranded);
    assert!(m.firing.take_events().is_empty());
}

#[test]
fn log14_forced_world_region_capturers_do_not_capture_or_contest_mixed_groups() {
    let options = options(Mode::World);
    let mut random = MatchRandom::new(options.seed);
    let generated = generate_world(&mut random, WorldSize::Huge, &[0, 1], true).unwrap();
    let map = AuthoritativeMap::from_world(generated.into_world_map_input()).unwrap();
    let mut m = Match::new_world(&map, options, random).unwrap();
    let region = m
        .world_runtime
        .as_ref()
        .unwrap()
        .regions
        .iter()
        .position(|r| r.team < 0)
        .unwrap();
    let definition = &m.world.geography().unwrap().regions[region];
    let at = point(
        (definition.x as f64 + 0.5) * CELL,
        (definition.y as f64 + 0.5) * CELL,
    );
    for unit in m
        .world
        .game
        .units
        .values_mut()
        .filter(|unit| unit.owner == LOCAL_DEFENDER_OWNER)
    {
        unit.hp = 0.0;
    }
    let forced = add(&mut m, "rifle", 0, at);
    force(&mut m, forced, true);
    let ids = order(&m);
    m.apply_world_capture_income(&ids, 0.1).unwrap();
    assert_eq!(
        m.world_runtime.as_ref().unwrap().regions[region].progress,
        0.0
    );
    let rival = add(&mut m, "rifle", 1, at);
    let ids = order(&m);
    m.apply_world_capture_income(&ids, 0.1).unwrap();
    let state = &m.world_runtime.as_ref().unwrap().regions[region];
    assert!(!state.contested);
    assert!(state.progress > 0.0);
    assert_eq!(state.capper, Some(1));
    assert_eq!(m.world.game.units[&rival].owner, 1);
    m.logistics.get_mut(forced).unwrap().forced = false;
    m.apply_world_capture_income(&ids, 0.1).unwrap();
    assert!(m.world_runtime.as_ref().unwrap().regions[region].contested);
}
