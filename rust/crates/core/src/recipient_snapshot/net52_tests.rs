use super::*;
use crate::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::{PlaneState, Point},
    map::AuthoritativeMap,
    match_aviation::{MissionKind, PlaneMission},
    match_runtime::{MatchOptions, PlayerConfig},
    weather::WeatherChoice,
};
use std::collections::BTreeSet;

fn scene() -> Match {
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"NET52", "w":80,"h":80,"rows":vec![".".repeat(80);80],
            "spawns":[{"x":8,"y":40},{"x":20,"y":40},{"x":72,"y":40}],
            "points":[{"x":40,"y":40}]
        }))
        .unwrap(),
    )
    .unwrap();
    let mut state = Match::new(
        &map,
        MatchOptions {
            seed: 701,
            mode: Mode::Conquest,
            players: [0, 0, 1]
                .into_iter()
                .enumerate()
                .map(|(slot, team)| PlayerConfig {
                    name: format!("P{slot}"),
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
        },
    )
    .unwrap();
    for control in state.commands.units.values_mut() {
        control.hold_fire = true;
    }
    state
}

fn ids(projection: &RecipientProjection) -> BTreeSet<u32> {
    projection.fields["units"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row[0].as_u64().unwrap() as u32)
        .collect()
}

#[test]
fn net52_units_include_allies_visible_enemies_airborne_planes_and_end_reveal() {
    let mut state = scene();
    let plane = state.spawn_aircraft(2, "fighter").unwrap();
    let own_plane = state.spawn_aircraft(0, "fighter").unwrap();
    let visible_enemy = state
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 2 && u.air.is_none())
        .unwrap()
        .id;
    let hidden_enemy = state
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 2 && u.air.is_none() && u.id != visible_enemy)
        .unwrap()
        .id;
    let friendly = state
        .world
        .game
        .units
        .values()
        .filter(|u| u.owner != 2)
        .map(|u| u.id)
        .collect::<BTreeSet<_>>();
    state.world.game.players[0].visible = vec![visible_enemy, plane];
    state.world.game.players[0].visible.sort_unstable();
    state.world.game.players[1].visible = state.world.game.players[0].visible.clone();
    state.world.game.players[2].visible.clear();
    for (air, flies) in [
        (PlaneState::Base, false),
        (PlaneState::Out, true),
        (PlaneState::Station, true),
        (PlaneState::Home, true),
        (PlaneState::Rearm, false),
    ] {
        state.world.game.units.get_mut(&plane).unwrap().air = Some(air);
        let cache = SnapshotCache::new(&state).unwrap();
        let mut expected = friendly.clone();
        expected.insert(visible_enemy);
        if flies {
            expected.insert(plane);
        }
        for slot in [0, 1] {
            let projection = cache.recipient(slot).unwrap();
            assert_eq!(ids(&projection), expected, "seat {slot}, {air:?}");
            assert!(!ids(&projection).contains(&hidden_enemy));
            // A friendly plane at base is still visible to both teammates.
            assert!(ids(&projection).contains(&own_plane));
        }
        let own = cache.recipient(2).unwrap();
        let row = own.fields["units"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row[0] == plane)
            .unwrap();
        assert_eq!(row.as_array().unwrap().len(), 15);
        assert_eq!(row[12].as_u64().unwrap() & 512, if flies { 0 } else { 512 });
        assert!(ids(&own).contains(&hidden_enemy));
        assert!(!ids(&own).contains(&own_plane));
    }
    // Airborne state alone cannot bypass team vision.
    state.world.game.units.get_mut(&plane).unwrap().air = Some(PlaneState::Station);
    state.world.game.players[0].visible.clear();
    assert_eq!(
        ids(&SnapshotCache::new(&state).unwrap().recipient(0).unwrap()),
        friendly
    );
    // The finish reveal includes hidden ground and grounded aircraft.
    state.world.game.units.get_mut(&plane).unwrap().air = Some(PlaneState::Rearm);
    state.world.game.reveal = true;
    let cache = SnapshotCache::new(&state).unwrap();
    let all = state
        .world
        .game
        .units
        .keys()
        .copied()
        .collect::<BTreeSet<_>>();
    for slot in 0..3 {
        assert_eq!(ids(&cache.recipient(slot).unwrap()), all);
    }
}

#[test]
fn net52_aircraft_ticks_and_recipient_delivery_preserve_room_admission_guard() {
    let mut state = scene();
    let plane = state.spawn_aircraft(0, "fighter").unwrap();
    state.commands.units.get_mut(&plane).unwrap().hold_fire = true;
    let at = Point { x: 40., z: 80. };
    assert!(
        state
            .send_aircraft_mission(
                0,
                plane,
                PlaneMission {
                    kind: MissionKind::Patrol,
                    target: None,
                    at,
                }
            )
            .unwrap()
    );
    let mut delivery = DeliveryState::new(&state).unwrap();
    // Complete room snapshots still refuse aircraft, before attachment.
    assert!(matches!(
        state.preflight_ordinary_snapshot(),
        Err(SnapshotError::Unsupported {
            capability: SnapshotCapability::Air,
            ..
        })
    ));
    for _ in 0..12 {
        state.advance_tick().unwrap();
        assert!(state.tick_state.fault().is_none());
        let frame = delivery
            .capture(&mut state, &[false; 3], &[false; 3])
            .unwrap();
        for slot in 0..3 {
            let projection = frame.recipient(slot).unwrap();
            assert!(projection.missing.contains(&SnapshotCapability::Air));
            if slot < 2 {
                assert!(ids(&projection).contains(&plane));
            }
            if slot == 0 {
                assert!(
                    projection.fields["plans"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|row| row == &json!([plane, 1, at.x, at.z]))
                );
            }
        }
    }
    assert!(matches!(
        state.preflight_ordinary_snapshot(),
        Err(SnapshotError::Unsupported {
            capability: SnapshotCapability::Air,
            ..
        })
    ));
    state.aviation.planes.remove(&plane);
    assert!(matches!(
        SnapshotCache::new(&state),
        Err(SnapshotError::Rules)
    ));
}
