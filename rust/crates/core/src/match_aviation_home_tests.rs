//! MOV-84: production access, nearest-home selection and initial plane placement.
use super::*;
use crate::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    map::AuthoritativeMap,
    match_runtime::{MatchOptions, PlayerConfig},
    recipient_snapshot::{DeliveryState, SelectedSeat},
    weather::WeatherChoice,
};
use serde_json::json;

fn scene(mode: Mode) -> Match {
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"Aircraft homes", "w":96,"h":96,
            "rows":vec![".".repeat(96);96],
            "spawns":[{"x":10,"y":10},{"x":10,"y":80},{"x":80,"y":80}],
            "defend":[0], "points":[{"x":48,"y":48,"vp":0,"mp":1},{"x":60,"y":60,"vp":0,"mp":1}]
        }))
        .unwrap(),
    )
    .unwrap();
    let options = MatchOptions {
        seed: 8401,
        mode,
        players: (0..if mode == Mode::Horde { 2 } else { 3 })
            .map(|slot| PlayerConfig {
                name: format!("P{slot}"),
                team: if slot == 2 { 1 } else { 0 },
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
    };
    match mode {
        Mode::Horde => Match::new_horde(&map, options),
        Mode::Classic => Match::new_classic(&map, options),
        Mode::Annihilation => Match::new_annihilation(&map, options),
        Mode::Assault => Match::new_assault(&map, options, 0),
        Mode::Tutorial => Match::new_tutorial(&map, options),
        _ => Match::new(&map, options),
    }
    .unwrap()
}
fn field(state: &mut Match, owner: usize, x: usize, z: usize) -> (UnitId, Point) {
    let id = state.next_unit_id;
    let at = state
        .place_starting_building(owner, "airfield", z * state.world.game.w + x)
        .unwrap();
    (id, at)
}
fn offmap(state: &Match, owner: usize) -> Point {
    // sim.js:2188-2189 and CFG.air.offmap: 40 m outward from the map centre.
    let spawn = state.start.players[owner].spawn;
    let dx = spawn.x - state.world.game.w as f64 * crate::game::CELL / 2.;
    let dz = spawn.z - state.world.game.h as f64 * crate::game::CELL / 2.;
    let length = math::hypot(dx, dz);
    Point {
        x: spawn.x + dx / length * 40.,
        z: spawn.z + dz / length * 40.,
    }
}

#[test]
fn mov84_nearest_finished_accessible_airfield_and_initial_home_survive_ordinary_delivery() {
    // Airfield access is ownership outside Horde, including allied players.
    for mode in [
        Mode::Default,
        Mode::Conquest,
        Mode::Classic,
        Mode::Assault,
        Mode::Annihilation,
        Mode::Tutorial,
    ] {
        let mut state = scene(mode);
        let fallback = offmap(&state, 0);
        assert_eq!(
            state.aircraft_base(0, Point { x: 40., z: 40. }).unwrap(),
            fallback
        );
        let initial = state.spawn_aircraft(0, "fighter").unwrap();
        assert_eq!(state.world.game.units[&initial].at(), fallback);
        assert_eq!(state.world.game.units[&initial].air, Some(PlaneState::Base));
        let (_, allied) = field(&mut state, 1, 20, 20);
        let enemy_owner = state.start.players.len() - 1;
        let (_, enemy) = field(&mut state, enemy_owner, 28, 20);
        let (unfinished, _) = field(&mut state, 0, 36, 20);
        let (dead, _) = field(&mut state, 0, 44, 20);
        state.world.game.units.get_mut(&unfinished).unwrap().built = Some(0.99);
        state.world.game.units.get_mut(&dead).unwrap().hp = 0.;
        for at in [allied, enemy] {
            assert_eq!(state.aircraft_base(0, at).unwrap(), fallback);
        }
        let (first, first_at) = field(&mut state, 0, 52, 20);
        let (_, second_at) = field(&mut state, 0, 68, 20);
        assert_eq!(state.aircraft_base(0, second_at).unwrap(), second_at);
        assert_eq!(state.aircraft_base(0, allied).unwrap(), first_at);
        let middle = Point {
            x: (first_at.x + second_at.x) / 2.,
            z: first_at.z,
        };
        assert_eq!(
            state.aircraft_base(0, middle).unwrap(),
            first_at,
            "ties use the lower unit ID"
        );
        assert!(first > dead);
        // Only first_at remains eligible for a new plane's scattered starting point.
        state.world.game.units.values_mut().for_each(|u| {
            if state.rules.kinds[u.kind].name == "airfield" && u.id != first {
                u.built = Some(0.5);
            }
        });
        for name in ["fighter", "attacker", "bomber"] {
            let id = state.spawn_aircraft(0, name).unwrap();
            assert_eq!(
                state.world.game.units[&id].at(),
                first_at,
                "{mode:?} {name}"
            );
            assert_eq!(state.world.game.units[&id].air, Some(PlaneState::Base));
        }
        // Restore synthetic completion/death probes before the real tick lifecycle.
        for u in state.world.game.units.values_mut() {
            if state.rules.kinds[u.kind].name == "airfield" {
                u.built = Some(1.);
                u.hp = 1800.;
            }
        }
        if !matches!(mode, Mode::Default | Mode::Conquest | Mode::Classic) {
            // These mode runners have the same home primitive, but their full
            // recipient admission remains outside this delivery batch.
            for _ in 0..4 {
                state.advance_tick().unwrap();
            }
            assert!(state.tick_state.fault().is_none());
            continue;
        }
        let selected: Vec<_> = (0..state.start.players.len())
            .map(|slot| SelectedSeat {
                slot,
                full: false,
                omit: false,
            })
            .collect();
        let mut delivery = DeliveryState::new(&state).unwrap();
        for _ in 0..4 {
            let report = state.advance_tick().unwrap();
            let frame = delivery
                .capture_selected_with_tick_events(&mut state, &selected, &report.events)
                .unwrap()
                .unwrap();
            for seat in &selected {
                let snapshot = frame.recipient(seat.slot).unwrap().into_json();
                assert_eq!(snapshot["t"], "s");
                let air = snapshot["air"].as_array().unwrap();
                assert_eq!(air.len(), if seat.slot == 0 { 4 } else { 0 });
            }
        }
        assert!(state.tick_state.fault().is_none());
    }
}

#[test]
fn mov84_horde_defenders_share_airfields_but_the_horde_and_other_modes_do_not() {
    // productionAccess at sim.js:387 is also the predicate used by airBase.
    let mut state = scene(Mode::Horde);
    let (_, shared) = field(&mut state, 0, 30, 25);
    assert_eq!(state.aircraft_base(1, shared).unwrap(), shared);
    let id = state.spawn_aircraft(1, "fighter").unwrap();
    assert_eq!(state.world.game.units[&id].at(), shared);
    let horde = state.horde.as_ref().unwrap().slot;
    assert_eq!(
        state.aircraft_base(horde, shared).unwrap(),
        offmap(&state, horde)
    );
    let (_, enemy) = field(&mut state, horde, 45, 25);
    assert_eq!(state.aircraft_base(horde, enemy).unwrap(), enemy);
    assert_eq!(state.aircraft_base(1, enemy).unwrap(), shared);
    // Canonical Horde now uses the same admitted aviation lifecycle.
    state.validate_aviation().unwrap();
}

fn delivered_tick(state: &mut Match, delivery: &mut DeliveryState) -> Vec<serde_json::Value> {
    let report = state.advance_tick().unwrap();
    let selected = (0..state.start.players.len())
        .map(|slot| SelectedSeat {
            slot,
            full: false,
            omit: false,
        })
        .collect::<Vec<_>>();
    let classic = (state.options.mode == Mode::Classic).then(|| {
        let public = state.elimination.as_ref().unwrap();
        (
            json!({"kind":"classic", "timeLeft": public.time_left.unwrap().max(0.).ceil(), "suddenDeath": public.sudden_death}),
            public.nodes.iter().map(|n| json!([(n.at.x * 10.).round() / 10., (n.at.z * 10.).round() / 10., n.rate, u8::from(n.fuel)])).collect::<Vec<_>>(),
            state.start.economy.players.iter().map(|p| (p.mun.floor(), p.fuel.floor())).collect::<Vec<_>>()
        )
    });
    let frame = delivery
        .capture_selected_with_tick_events(state, &selected, &report.events)
        .unwrap()
        .unwrap();
    let snapshots: Vec<_> = selected
        .iter()
        .map(|seat| frame.recipient(seat.slot).unwrap().into_json())
        .collect();
    if let Some((mode, nodes, wallets)) = classic {
        for (slot, snapshot) in snapshots.iter().enumerate() {
            assert_eq!(snapshot["mode"], mode);
            assert_eq!(snapshot["nodes"], json!(nodes));
            assert_eq!(snapshot["mun"], wallets[slot].0);
            assert_eq!(snapshot["fuel"], wallets[slot].1);
            assert!(snapshot["mode"].get("teams").is_none());
        }
    }
    assert!(state.tick_state.fault().is_none());
    snapshots
}

#[test]
fn mov84_classic_constructs_two_airfields_and_trains_each_plane_at_its_paid_producer() {
    use crate::match_commands::Command;
    let mut state = scene(Mode::Classic);
    {
        let initial = crate::recipient_snapshot::SnapshotCache::new(&state).unwrap();
        for slot in 0..state.start.players.len() {
            let fields = initial.recipient(slot).unwrap().fields;
            assert_eq!(fields["ghosts"], json!([]));
            assert_eq!(
                fields["mode"],
                json!({"kind":"classic", "timeLeft":1500., "suddenDeath":false})
            );
            assert_eq!(fields["mun"], 0.);
            assert_eq!(fields["fuel"], 0.);
        }
    }
    for control in state.commands.units.values_mut() {
        control.hold_fire = true;
        control.auto_cast = false;
        control.auto_retreat = false;
    }
    state.start.economy.players[0].mp = 10000.;
    state.start.economy.players[0].fuel = 10000.;
    let engineer = state
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 0 && state.rules.kinds[u.kind].name == "engineer")
        .unwrap()
        .id;
    // Isolate the Airfield prerequisite from the selected construction/training journey.
    state
        .place_starting_building(0, "motorpool", 35 * state.world.game.w + 20)
        .unwrap();
    let mut delivery = DeliveryState::new(&state).unwrap();
    let mut born = Vec::new();
    for at in [Point { x: 63., z: 63. }, Point { x: 103., z: 63. }] {
        crate::unit_placement::place_unit(
            &mut state.grid,
            state.world.game.units.get_mut(&engineer).unwrap(),
            |u| {
                u.x = at.x;
                u.z = at.z + 8.;
                u.path.clear();
                u.world_goal = None;
            },
        );
        crate::vision::update_vision(&mut state.world.game, &state.rules, &state.grid);
        let field = state.next_unit_id;
        let mp = state.start.economy.players[0].mp;
        assert_eq!(
            state
                .execute_command(
                    0,
                    &Command::Build {
                        ids: vec![i64::from(engineer)],
                        kind: "airfield".into(),
                        at: Some(at),
                        direction: None,
                        queue: false,
                    },
                    false
                )
                .unwrap(),
            None
        );
        assert_eq!(state.start.economy.players[0].mp, mp - 250.);
        assert_eq!(state.world.game.units[&field].at(), at);
        assert!(state.world.game.units[&field].built.unwrap() < 1.);
        for _ in 0..2000 {
            delivered_tick(&mut state, &mut delivery);
            if state.world.game.units[&field].built.unwrap() >= 1. {
                break;
            }
        }
        assert_eq!(state.world.game.units[&field].built, Some(1.));
        for (name, price_mp, price_fuel) in [
            ("fighter", 200., 40.),
            ("attacker", 240., 70.),
            ("bomber", 300., 100.),
        ] {
            let before_mp = state.start.economy.players[0].mp;
            let before_fuel = state.start.economy.players[0].fuel;
            assert_eq!(
                state
                    .execute_command(
                        0,
                        &Command::Buy {
                            kind: name.into(),
                            from: Some(field)
                        },
                        false
                    )
                    .unwrap(),
                None
            );
            let jobs = state.production.building(field).unwrap().jobs();
            assert_eq!(jobs.len(), 1);
            assert_eq!(jobs[0].kind, name);
            assert_eq!(state.start.economy.players[0].mp, before_mp - price_mp);
            assert_eq!(
                state.start.economy.players[0].fuel,
                before_fuel - price_fuel
            );
            let id = state.next_unit_id;
            let mut completed = false;
            for _ in 0..2000 {
                let snapshots = delivered_tick(&mut state, &mut delivery);
                if state.world.game.units.contains_key(&id) {
                    let u = &state.world.game.units[&id];
                    assert_eq!(state.rules.kinds[u.kind].name, name);
                    assert_eq!(u.air, Some(PlaneState::Base));
                    assert_eq!(
                        u.at(),
                        at,
                        "the distant producer overrides the initial home scatter"
                    );
                    let runtime = state.aviation.planes[&id];
                    assert!(runtime.ammo > 0. && runtime.fuel > 0.);
                    assert!(runtime.mission.is_none());
                    assert!(state.production.building(field).unwrap().jobs().is_empty());
                    assert!(
                        snapshots[0]["air"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .any(|row| row[0] == id)
                    );
                    for snapshot in &snapshots[1..] {
                        assert!(
                            !snapshot["air"]
                                .as_array()
                                .unwrap()
                                .iter()
                                .any(|row| row[0] == id)
                        );
                    }
                    born.push(id);
                    completed = true;
                    break;
                }
            }
            assert!(completed, "{name} must finish its real queue");
            for _ in 0..4 {
                delivered_tick(&mut state, &mut delivery);
            }
        }
    }
    assert_eq!(born.len(), 6);
    assert!(state.tick_state.fault().is_none());
}

#[test]
fn base18_aviation_purchase_rejects_invalid_makers_atomically_and_places_each_plane_at_selected_field()
 {
    use crate::match_commands::Command;
    for mode in [
        Mode::Default,
        Mode::Conquest,
        Mode::Assault,
        Mode::Annihilation,
    ] {
        let mut state = scene(mode);
        state.start.economy.players[0].mp = 10000.;
        state.start.economy.players[0].fuel = 777.;
        let (allied, _) = field(&mut state, 1, 12, 30);
        let (enemy, _) = field(&mut state, 2, 24, 30);
        let (unfinished, _) = field(&mut state, 0, 36, 30);
        let (dead, _) = field(&mut state, 0, 48, 30);
        let (first, first_at) = field(&mut state, 0, 60, 30);
        let (second, second_at) = field(&mut state, 0, 72, 30);
        state.world.game.units.get_mut(&unfinished).unwrap().built = Some(0.99);
        state.world.game.units.get_mut(&dead).unwrap().hp = 0.;
        state.player_rallies[0] = Some(Point { x: 10., z: 10. });
        state
            .building_rallies
            .insert(second, Some(Point { x: 20., z: 20. }));
        let buy = |name: &str, from| Command::Buy {
            kind: name.into(),
            from,
        };
        for from in [allied, enemy, unfinished, dead, u32::MAX] {
            let mp = state.start.economy.players[0].mp;
            let next = state.next_unit_id;
            assert_eq!(
                state
                    .execute_command(0, &buy("fighter", Some(from)), false)
                    .unwrap(),
                Some(crate::match_commands::Denial::Needs)
            );
            assert_eq!(state.start.economy.players[0].mp, mp);
            assert_eq!(state.next_unit_id, next);
            assert!(state.aviation.planes.is_empty());
        }
        for (name, price, from, at) in [
            ("fighter", 280., None, first_at),
            ("attacker", 340., Some(second), second_at),
            ("bomber", 420., Some(first), first_at),
        ] {
            let mp = state.start.economy.players[0].mp;
            let id = state.next_unit_id;
            assert_eq!(
                state.execute_command(0, &buy(name, from), false).unwrap(),
                None
            );
            assert_eq!(state.start.economy.players[0].mp, mp - price);
            assert_eq!(state.start.economy.players[0].fuel, 777.);
            let u = &state.world.game.units[&id];
            assert_eq!(u.owner, 0);
            assert_eq!(state.rules.kinds[u.kind].name, name);
            assert_eq!(u.at(), at);
            assert_eq!(u.air, Some(PlaneState::Base));
            assert!(u.path.is_empty());
            assert!(state.aviation.planes[&id].mission.is_none());
            assert_eq!(state.recruit_metadata[&id].building, from.unwrap_or(first));
            assert!(state.production.building(first).unwrap().jobs().is_empty());
            assert!(state.production.building(second).unwrap().jobs().is_empty());
        }
        // Restore synthetic invalid-maker probes before entering actual ticks.
        state.world.game.units.get_mut(&unfinished).unwrap().built = Some(1.);
        state.world.game.units.get_mut(&dead).unwrap().hp = 1800.;
        if matches!(mode, Mode::Default | Mode::Conquest) {
            let mut delivery = DeliveryState::new(&state).unwrap();
            for _ in 0..5 {
                let snapshots = delivered_tick(&mut state, &mut delivery);
                assert_eq!(snapshots[0]["air"].as_array().unwrap().len(), 3);
                assert_eq!(snapshots[1]["air"], json!([]));
                assert_eq!(snapshots[2]["air"], json!([]));
            }
        } else {
            for _ in 0..5 {
                state.advance_tick().unwrap();
            }
        }
        assert!(state.tick_state.fault().is_none());
    }
}

#[test]
fn base18_horde_shared_airfields_charge_only_buyer_spawn_all_aircraft_and_deliver_every_seat() {
    use crate::match_commands::{Command, Denial};
    let mut state = scene(Mode::Horde);
    let horde = state.horde.as_ref().unwrap().slot;
    state.start.economy.players[1].mp = 10000.;
    let (shared, at) = field(&mut state, 0, 30, 25);
    let (enemy, _) = field(&mut state, horde, 50, 25);
    let (unfinished, _) = field(&mut state, 0, 40, 25);
    state.world.game.units.get_mut(&unfinished).unwrap().built = Some(0.5);
    for from in [enemy, unfinished, u32::MAX] {
        let before = state.start.economy.players.clone();
        let next = state.next_unit_id;
        assert_eq!(
            state
                .execute_command(
                    1,
                    &Command::Buy {
                        kind: "fighter".into(),
                        from: Some(from)
                    },
                    false
                )
                .unwrap(),
            Some(Denial::Needs)
        );
        assert_eq!(state.start.economy.players, before);
        assert_eq!(state.next_unit_id, next);
    }
    state.world.game.units.get_mut(&unfinished).unwrap().built = Some(1.);
    let original_owner_mp = state.start.economy.players[0].mp;
    let mut ids = Vec::new();
    for (kind, price, from) in [
        ("fighter", 280., None),
        ("attacker", 340., Some(shared)),
        ("bomber", 420., Some(shared)),
    ] {
        let mp = state.start.economy.players[1].mp;
        let id = state.next_unit_id;
        assert_eq!(
            state
                .execute_command(
                    1,
                    &Command::Buy {
                        kind: kind.into(),
                        from
                    },
                    false
                )
                .unwrap(),
            None
        );
        assert_eq!(state.start.economy.players[1].mp, mp - price);
        assert_eq!(state.start.economy.players[0].mp, original_owner_mp);
        assert_eq!(state.world.game.units[&id].owner, 1);
        assert_eq!(state.world.game.units[&id].at(), at);
        assert_eq!(state.world.game.units[&id].air, Some(PlaneState::Base));
        assert!(state.world.game.units[&id].path.is_empty());
        assert_eq!(state.recruit_metadata[&id].building, shared);
        assert!(state.production.building(shared).unwrap().jobs().is_empty());
        ids.push(id);
    }
    let mut delivery = DeliveryState::new(&state).unwrap();
    for _ in 0..12 {
        let snapshots = delivered_tick(&mut state, &mut delivery);
        assert_eq!(
            snapshots[1]["air"]
                .as_array()
                .unwrap()
                .iter()
                .map(|r| r[0].as_u64().unwrap() as u32)
                .collect::<Vec<_>>(),
            ids
        );
        for slot in [0, horde] {
            assert_eq!(snapshots[slot]["air"], json!([]));
        }
        for slot in [0, 1] {
            assert!(
                snapshots[slot]["queues"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|r| r[0] == shared)
            );
        }
        assert!(
            !snapshots[horde]["queues"]
                .as_array()
                .unwrap()
                .iter()
                .any(|r| r[0] == shared)
        );
        assert!(
            !snapshots[1]["productionJobs"]
                .as_array()
                .unwrap()
                .iter()
                .any(|r| r[0] == shared)
        );
    }
    assert!(state.tick_state.fault().is_none());
}

#[test]
fn base38_airfield_values_prerequisite_tier_and_paid_construction_match_prototype() {
    use crate::{
        catalog::{Catalog, Value},
        economy::TechState,
        match_commands::{Command, Denial},
    };
    let catalog = Catalog::compiled().unwrap();
    let definition = catalog.unit("airfield").unwrap();
    for (key, expected) in [
        ("cost", 250.),
        ("buildTime", 40.),
        ("hpPer", 1800.),
        ("size", 3.),
        ("vision", 30.),
    ] {
        assert_eq!(
            definition.get(key).and_then(Value::number),
            Some(expected),
            "{key}"
        );
    }
    assert_eq!(
        definition.get("needs").and_then(Value::string),
        Some("motorpool")
    );
    assert_eq!(
        definition
            .get("makes")
            .and_then(Value::array)
            .unwrap()
            .iter()
            .map(|v| v.string().unwrap())
            .collect::<Vec<_>>(),
        vec!["fighter", "attacker", "bomber"]
    );
    assert_eq!(crate::economy::tier_of("airfield").unwrap(), 3);
    let mut state = scene(Mode::Classic);
    state.options.tech = true;
    state.start.economy.players[0].mp = 10000.;
    let engineer = state
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 0 && state.rules.kinds[u.kind].name == "engineer")
        .unwrap()
        .id;
    let at = Point { x: 63., z: 63. };
    crate::unit_placement::place_unit(
        &mut state.grid,
        state.world.game.units.get_mut(&engineer).unwrap(),
        |u| {
            u.x = at.x;
            u.z = at.z + 8.;
            u.path.clear();
            u.world_goal = None;
        },
    );
    crate::vision::update_vision(&mut state.world.game, &state.rules, &state.grid);
    let build = Command::Build {
        ids: vec![i64::from(engineer)],
        kind: "airfield".into(),
        at: Some(at),
        direction: None,
        queue: false,
    };
    let mp = state.start.economy.players[0].mp;
    let id = state.next_unit_id;
    assert_eq!(
        state.execute_command(0, &build, false).unwrap(),
        Some(Denial::Needs)
    );
    let maker = state.next_unit_id;
    state
        .place_starting_building(0, "motorpool", 35 * state.world.game.w + 20)
        .unwrap();
    for (hp, built) in [(1900., 0.9), (0., 1.)] {
        let u = state.world.game.units.get_mut(&maker).unwrap();
        u.hp = hp;
        u.built = Some(built);
        assert_eq!(
            state.execute_command(0, &build, false).unwrap(),
            Some(Denial::Needs)
        );
    }
    let u = state.world.game.units.get_mut(&maker).unwrap();
    u.hp = 1900.;
    u.built = Some(1.);
    for tier in [1, 2, 3] {
        let mut production = serde_json::to_value(&state.production).unwrap();
        production["players"][0]["tech"] =
            serde_json::to_value(TechState::new(tier, 0, 0, 0).unwrap()).unwrap();
        state.production = serde_json::from_value(production).unwrap();
        assert_eq!(
            state.execute_command(0, &build, false).unwrap(),
            if tier == 3 { None } else { Some(Denial::Tier) }
        );
        assert_eq!(
            state.start.economy.players[0].mp,
            mp - if tier == 3 { 250. } else { 0. }
        );
    }
    let field = id + 1;
    assert_eq!(
        state.world.game.units[&field].cells.as_ref().unwrap().len(),
        9
    );
    let mut delivery = DeliveryState::new(&state).unwrap();
    for _ in 0..2000 {
        delivered_tick(&mut state, &mut delivery);
        if state.world.game.units[&field].built == Some(1.) {
            break;
        }
    }
    assert_eq!(state.world.game.units[&field].built, Some(1.));
    assert_eq!(state.world.game.units[&field].hp, 1800.);
    assert!(state.tick_state.fault().is_none());
}
