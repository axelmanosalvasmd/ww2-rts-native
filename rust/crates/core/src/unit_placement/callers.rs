//! Immediate caller checks run before the ordinary tick can rebuild the grid.
use super::tests::assert_current;
use crate::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::{PlaneState, Point},
    map::AuthoritativeMap,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    recipient_snapshot::{DeliveryState, SelectedSeat},
    weather::WeatherChoice,
};
use serde_json::json;

fn scene() -> Match {
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"Placement callers", "w":64,"h":64,"rows":vec![".".repeat(64);64],
            "spawns":[{"x":6,"y":6},{"x":56,"y":56}],"points":[{"x":32,"y":32}]
        }))
        .unwrap(),
    )
    .unwrap();
    Match::new(
        &map,
        MatchOptions {
            seed: 6701,
            mode: Mode::Conquest,
            players: (0..2)
                .map(|slot| PlayerConfig {
                    name: format!("seat{slot}"),
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
    .unwrap()
}

fn continue_every_seat(m: &mut Match) {
    for control in m.commands.units.values_mut() {
        control.hold_fire = true;
        control.auto_cast = false;
    }
    let selected = (0..m.world.game.players.len())
        .map(|slot| SelectedSeat {
            slot,
            full: false,
            omit: false,
        })
        .collect::<Vec<_>>();
    let mut delivery = DeliveryState::new(m).unwrap();
    for _ in 0..6 {
        let report = m.advance_tick().unwrap();
        assert_current(&m.world.game, &m.grid);
        let frame = delivery
            .capture_selected_with_tick_events(m, &selected, &report.events)
            .unwrap()
            .unwrap();
        for seat in &selected {
            assert_eq!(frame.recipient(seat.slot).unwrap().into_json()["t"], "s");
        }
    }
    assert!(m.tick_state.fault().is_none());
}

#[test]
fn mov67_bunker_and_building_publication_are_indexed_before_the_next_tick() {
    let mut m = scene();
    let id = m.next_unit_id;
    m.fortify_assault(0).unwrap();
    let bunker = &m.world.game.units[&id];
    assert_eq!(m.rules.kinds[bunker.kind].name, "bunker");
    assert_eq!(bunker.owner, 0);
    let spawn = m.start.players[0].spawn;
    let facing = crate::math::atan2(64.0 - spawn.z, 64.0 - spawn.x);
    assert_eq!(bunker.rot, facing);
    assert_eq!(m.commands.units[&id].aim, facing);
    assert_ne!(bunker.at(), spawn);
    assert_current(&m.world.game, &m.grid);
    for (name, cell) in [("barracks", 40 * 64 + 40), ("airfield", 40 * 64 + 50)] {
        let id = m.next_unit_id;
        let at = m.place_starting_building(0, name, cell).unwrap();
        let building = &m.world.game.units[&id];
        assert_eq!(building.at(), at);
        assert_eq!(building.built, Some(1.0));
        assert!(!building.cells.as_ref().unwrap().is_empty());
        assert_current(&m.world.game, &m.grid);
    }
    continue_every_seat(&mut m);
}

#[test]
fn mov67_base_and_rearming_planes_reindex_both_offmap_and_airfield_poses() {
    let mut m = scene();
    let id = m.spawn_aircraft(0, "fighter").unwrap();
    assert_current(&m.world.game, &m.grid);
    for airfield in [false, true] {
        if airfield {
            m.place_starting_building(0, "airfield", 40 * 64 + 40)
                .unwrap();
            assert_current(&m.world.game, &m.grid);
        }
        for state in [PlaneState::Base, PlaneState::Rearm] {
            let old = Point { x: 25.0, z: 25.0 };
            let plane = m.world.game.units.get_mut(&id).unwrap();
            plane.x = old.x;
            plane.z = old.z;
            plane.air = Some(state);
            m.grid.update(id, plane);
            m.aviation.planes.get_mut(&id).unwrap().timer = 10.0;
            let base = m.aircraft_base(0, old).unwrap();
            assert_ne!((base.x / 16.0).floor(), (old.x / 16.0).floor());
            m.step_aircraft(id, 0.05).unwrap();
            assert_eq!(m.world.game.units[&id].at(), base);
            assert_eq!(m.world.game.units[&id].air, Some(state));
            assert_current(&m.world.game, &m.grid);
        }
    }
    continue_every_seat(&mut m);
}

#[test]
fn mov67_operative_squad_respawn_publishes_the_squad_pose_without_a_tick_rebuild() {
    let mut m = scene();
    let body = m.join_companion("placement", 0, false).unwrap().unwrap();
    assert_current(&m.world.game, &m.grid);
    let squad = m
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 0 && m.rules.kinds[u.kind].name == "rifle" && u.id != body)
        .unwrap()
        .id;
    for id in [squad, body] {
        let unit = m.world.game.units.get_mut(&id).unwrap();
        unit.x = 81.0;
        unit.z = if id == squad { 65.0 } else { 67.0 };
        unit.path.clear();
        m.grid.update(id, unit);
    }
    m.pick_companion_squad(body).unwrap();
    assert_eq!(m.companions.get("placement").unwrap().squad_id, squad);
    m.world.game.units.get_mut(&body).unwrap().hp = 0.0;
    m.advance_tick().unwrap();
    let op = m.companions.get("placement").unwrap();
    assert_eq!(op.unit_id, 0);
    m.world.game.tick = op.respawn_at;
    m.start.economy.players[0].mp = 0.0;
    let at = m.world.game.units[&squad].at();
    let hp = m.world.game.units[&squad].hp;
    let born = m.respawn_companions(false).unwrap();
    assert_eq!(born.len(), 1);
    assert_eq!(m.world.game.units[&born[0]].at(), at);
    assert_eq!(m.world.game.units[&born[0]].owner, 0);
    assert_eq!(
        m.world.game.units[&squad].hp,
        hp - crate::companions::OPERATIVE.hp
    );
    assert_eq!(m.start.economy.players[0].mp, 0.0);
    assert_current(&m.world.game, &m.grid);
}
