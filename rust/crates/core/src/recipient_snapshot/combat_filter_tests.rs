use super::*;
use crate::{
    economy::Faction,
    economy_state::ArmySize,
    game::PlaneState,
    map::AuthoritativeMap,
    match_runtime::{MatchOptions, PlayerConfig},
    weather::WeatherChoice,
};

fn scene() -> Match {
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"Combat recipient filter", "w":80,"h":80,
            "rows":vec![".".repeat(80);80],
            "spawns":[{"x":8,"y":8},{"x":8,"y":70},{"x":70,"y":70}],
            "points":[{"x":40,"y":40}]
        }))
        .unwrap(),
    )
    .unwrap();
    let mut state = Match::new(
        &map,
        MatchOptions {
            seed: 1907,
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
        control.auto_cast = false;
        control.auto_retreat = false;
    }
    state
}

fn row(projection: &RecipientProjection, id: u32) -> Option<&Value> {
    projection.fields["units"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row[0] == id)
}

#[test]
fn cmb134_recipient_visibility_targets_cooldowns_and_flags_follow_each_seat() {
    let mut state = scene();
    let owner = state
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 0 && state.rules.kinds[u.kind].name == "rifle")
        .unwrap()
        .id;
    let target = state
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 2 && state.rules.kinds[u.kind].name == "rifle")
        .unwrap()
        .id;
    let hidden = state
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 2 && u.id != target && !state.rules.kinds[u.kind].structure)
        .unwrap()
        .id;
    let plane = state.spawn_aircraft(2, "fighter").unwrap();
    let unit = state.world.game.units.get_mut(&owner).unwrap();
    unit.hold_pos = true;
    unit.retreating = true;
    let control = state.commands.units.get_mut(&owner).unwrap();
    control.hold_fire = true;
    control.auto_retreat = true;
    control.auto_cast = true;
    let ability = state.abilities.get_mut(owner).unwrap();
    ability.cooldown = 4.2;
    ability.buff = 1.;
    ability.ap = true;
    state.firing.units.get_mut(&owner).unwrap().target_id = target;
    state.world.game.players[0].visible = vec![plane];
    state.world.game.players[1].visible = vec![target, plane];
    state.world.game.players[1].visible.sort_unstable();
    state.world.game.players[2].visible = vec![owner];
    for air in [
        PlaneState::Base,
        PlaneState::Out,
        PlaneState::Station,
        PlaneState::Home,
        PlaneState::Rearm,
    ] {
        state.world.game.units.get_mut(&plane).unwrap().air = Some(air);
        let cache = SnapshotCache::new(&state).unwrap();
        let snapshots = (0..3)
            .map(|slot| cache.recipient(slot).unwrap())
            .collect::<Vec<_>>();
        for (slot, snapshot) in snapshots.iter().enumerate() {
            assert_eq!(*snapshot, recipient_uncached(&state, slot).unwrap());
        }
        let own = row(&snapshots[0], owner).unwrap();
        assert_eq!(
            own[9], 0,
            "an owner's locked target is still hidden by its own view"
        );
        assert_eq!(own[11], 5.0);
        assert_eq!(
            own[12].as_u64().unwrap() & (2048 | 4096 | 8192 | 16384),
            2048 | 4096 | 8192 | 16384
        );
        for slot in [1, 2] {
            let other = row(&snapshots[slot], owner).unwrap();
            assert_eq!(other[9], target, "the recipient sees the locked target");
            assert_eq!(other[11], 0);
            assert_eq!(
                other[12], 7,
                "public retreat, buff and AP bits survive for allies and enemies"
            );
        }
        assert!(row(&snapshots[0], hidden).is_none());
        assert!(row(&snapshots[1], hidden).is_none());
        assert!(row(&snapshots[2], hidden).is_some());
        for snapshot in &snapshots[..2] {
            assert_eq!(
                row(snapshot, plane).is_some(),
                matches!(
                    air,
                    PlaneState::Out | PlaneState::Station | PlaneState::Home
                )
            );
        }
        assert!(row(&snapshots[2], plane).is_some());
    }
    state.world.game.players[0].visible.push(target);
    state.world.game.players[0].visible.sort_unstable();
    assert_eq!(
        row(
            &SnapshotCache::new(&state).unwrap().recipient(0).unwrap(),
            owner
        )
        .unwrap()[9],
        target
    );
    state.world.game.players[1]
        .visible
        .retain(|id| *id != target);
    assert_eq!(
        row(
            &SnapshotCache::new(&state).unwrap().recipient(1).unwrap(),
            owner
        )
        .unwrap()[9],
        0
    );
    state.world.game.reveal = true;
    let snapshot = SnapshotCache::new(&state).unwrap().recipient(1).unwrap();
    assert_eq!(
        snapshot.fields["units"].as_array().unwrap().len(),
        state.world.game.units.len()
    );
    assert_eq!(row(&snapshot, owner).unwrap()[9], target);
    assert_eq!(
        row(&snapshot, owner).unwrap()[11],
        0,
        "reveal does not disclose cooldowns"
    );
}

#[test]
fn net63_cached_and_uncached_reads_equal_without_mutating_shared_or_returned_rows() {
    let mut state = scene();
    let plane = state.spawn_aircraft(2, "fighter").unwrap();
    state.commands.units.get_mut(&plane).unwrap().hold_fire = true;
    let owner = state
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 0)
        .unwrap()
        .id;
    state.abilities.get_mut(owner).unwrap().cooldown = 5.1;
    state.commands.units.get_mut(&owner).unwrap().auto_cast = true;
    let mut delivery = DeliveryState::new(&state).unwrap();
    for _ in 0..6 {
        let report = state.advance_tick().unwrap();
        assert!(state.tick_state.fault().is_none());
        let selected = (0..3)
            .map(|slot| SelectedSeat {
                slot,
                full: false,
                omit: false,
            })
            .collect::<Vec<_>>();
        let builds = CACHE_BUILDS.with(|count| count.get());
        let frame = delivery
            .capture_selected_with_tick_events(&mut state, &selected, &report.events)
            .unwrap()
            .unwrap();
        for slot in 0..3 {
            assert_eq!(
                frame.recipient(slot).unwrap().into_json()["tick"],
                report.tick
            );
        }
        drop(frame);
        assert_eq!(
            CACHE_BUILDS.with(|count| count.get()) - builds,
            1,
            "one cache for the complete three-seat send"
        );
        let cache = SnapshotCache::new(&state).unwrap();
        let original = (
            cache.rows.clone(),
            cache.owners.clone(),
            cache.public.clone(),
        );
        let expected = (0..3)
            .map(|slot| recipient_uncached(&state, slot).unwrap())
            .collect::<Vec<_>>();
        for order in [[2, 1, 0], [0, 0, 2], [1, 2, 1]] {
            for slot in order {
                let mut snapshot = cache.recipient(slot).unwrap();
                assert_eq!(snapshot, expected[slot]);
                snapshot.fields["units"][0][12] = json!(u64::MAX);
                snapshot.fields["units"].as_array_mut().unwrap().clear();
                snapshot.fields["plans"].as_array_mut().unwrap().clear();
                snapshot.fields["mp"] = json!(-999);
                assert_eq!(cache.recipient(slot).unwrap(), expected[slot]);
            }
        }
        assert_eq!(
            (
                cache.rows.clone(),
                cache.owners.clone(),
                cache.public.clone()
            ),
            original
        );
        assert_eq!(cache.recipient(3), Err(SnapshotError::InvalidRecipient));
        assert_eq!(
            recipient_uncached(&state, 3),
            Err(SnapshotError::InvalidRecipient)
        );
    }
}

#[test]
fn cmb130_classic_ghost_rows_keep_the_exact_six_field_shape() {
    let mut state = scene();
    let mut options = state.options.clone();
    options.mode = Mode::Classic;
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"Classic ghost fields", "w":80,"h":80,
            "rows":vec![".".repeat(80);80],
            "spawns":[{"x":8,"y":8},{"x":8,"y":70},{"x":70,"y":70}],
            "points":[{"x":40,"y":40}]
        }))
        .unwrap(),
    )
    .unwrap();
    state = Match::new_classic(&map, options).unwrap();
    state.classic_known_buildings.push((
        0,
        vec![crate::vision::KnownBuilding {
            id: 9001,
            kind: state.rules.kind("hq").unwrap(),
            owner: 2,
            at: crate::game::Point { x: 80.04, z: 70.06 },
            built: Some(0.234),
        }],
    ));
    let cache = SnapshotCache::new(&state).unwrap();
    for slot in [0, 1] {
        let snapshot = cache.recipient(slot).unwrap();
        assert_eq!(
            snapshot.fields["ghosts"],
            json!([[9001, "hq", 2, 80.0, 70.1, 0.2]])
        );
        assert_eq!(snapshot, recipient_uncached(&state, slot).unwrap());
    }
    assert_eq!(cache.recipient(2).unwrap().fields["ghosts"], json!([]));
}

#[test]
fn net63_hidden_invalid_units_have_the_same_cached_and_uncached_refusal() {
    let mut state = scene();
    let hidden = state
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 2)
        .unwrap()
        .id;
    assert!(row(&recipient_uncached(&state, 0).unwrap(), hidden).is_none());
    state.abilities.get_mut(hidden).unwrap().cooldown = f64::NAN;
    assert_eq!(
        SnapshotCache::new(&state).err(),
        Some(SnapshotError::InvalidNumber)
    );
    assert_eq!(
        recipient_uncached(&state, 0),
        Err(SnapshotError::InvalidNumber)
    );
    state.abilities.get_mut(hidden).unwrap().cooldown = 0.;
    state.commands.units.get_mut(&hidden).unwrap().operative = true;
    let expected = SnapshotError::Unsupported {
        unit: hidden,
        capability: SnapshotCapability::Operatives,
    };
    assert_eq!(SnapshotCache::new(&state).err(), Some(expected.clone()));
    assert_eq!(recipient_uncached(&state, 0), Err(expected));
}
