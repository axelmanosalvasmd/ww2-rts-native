use super::*;
use crate::{
    economy::Faction,
    economy_state::ArmySize,
    game::{ModeKind, Point},
    map::AuthoritativeMap,
    match_runtime::{MatchOptions, PlayerConfig},
    match_world::ResourceNode,
    weather::WeatherChoice,
};

fn scene(mode: Mode) -> Match {
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"Elimination recipients", "w":96, "h":96,
            "rows":vec![".".repeat(96);96],
            "spawns":[{"x":8,"y":8},{"x":8,"y":87},{"x":87,"y":87}],
            "points":[{"x":48,"y":48,"mp":2,"vp":1}]
        }))
        .unwrap(),
    )
    .unwrap();
    let options = MatchOptions {
        seed: 7302,
        mode,
        players: (0..3)
            .map(|slot| PlayerConfig {
                name: format!("P{slot}"),
                team: if slot == 2 { 1 } else { 0 },
                faction: Faction::try_from(0).unwrap(),
            })
            .collect(),
        shuffle: false,
        army: ArmySize::Standard,
        weather: WeatherChoice::Map,
        weather_seed: Some(1),
        living_weather: false,
        map_key: None,
        tech: true,
        supply: false,
        logistics: true,
        sandbox: false,
    };
    match mode {
        Mode::Classic => Match::new_classic(&map, options),
        Mode::Annihilation => Match::new_annihilation(&map, options),
        _ => unreachable!(),
    }
    .unwrap()
}

#[test]
fn classic_complete_snapshots_keep_private_currency_and_public_prototype_nodes() {
    let mut state = scene(Mode::Classic);
    for (slot, player) in state.start.economy.players.iter_mut().enumerate() {
        player.mun = 11.99 + slot as f64 * 10.;
        player.fuel = 41.99 + slot as f64 * 10.;
        player.fuel_inc = Some(2.26 + slot as f64);
        player.upkeep = Some(3.12 + slot as f64);
    }
    let mode = state.elimination.as_mut().unwrap();
    mode.time_left = Some(12.01);
    mode.sudden_death = true;
    mode.nodes = vec![ResourceNode {
        cell: 80 * 96 + 80,
        at: Point {
            x: 320.125,
            z: 320.165,
        },
        depot: None,
        rate: 2.345,
        fuel: true,
    }];
    let expected_nodes = json!([[320.1, 320.2, 2.345, 1.]]);
    let enemy_hq = state
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 2 && state.rules.kinds[u.kind].name == "hq")
        .unwrap();
    let ghost_id = enemy_hq.id;
    state.classic_known_buildings = vec![(
        0,
        vec![crate::vision::KnownBuilding {
            id: ghost_id,
            kind: enemy_hq.kind,
            owner: 2,
            at: Point {
                x: 301.26,
                z: 302.14,
            },
            built: Some(0.36),
        }],
    )];
    let raw = state.ordinary_snapshot_raw_lists().unwrap();
    assert_eq!(raw.nodes, Some(expected_nodes.clone()));
    for slot in 0..3 {
        assert_eq!(
            state.recipient_snapshot_lists(slot).unwrap().to_json()["nodes"],
            expected_nodes
        );
    }
    let owners = state
        .world
        .game
        .units
        .iter()
        .map(|(&id, unit)| (id, unit.owner))
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut delivery = DeliveryState::new(&state).unwrap();
    let frame = delivery
        .capture_with_lists(&mut state, &[true; 3], &[false; 3], &[])
        .unwrap();
    for slot in 0..3 {
        let msg = frame.recipient(slot).unwrap().into_json();
        assert_eq!(
            msg["mode"],
            json!({"kind":"classic","timeLeft":13.,"suddenDeath":true})
        );
        assert_eq!(msg["mun"], json!(11. + slot as f64 * 10.));
        assert_eq!(msg["fuel"], json!(41. + slot as f64 * 10.));
        assert_eq!(msg["fuelInc"], json!([2.3, 3.3, 4.3][slot]));
        assert_eq!(msg["upkeep"], json!([3.1, 4.1, 5.1][slot]));
        assert_eq!(msg["nodes"], json!([[320.1, 320.2, 2.345, 1]]));
        assert_eq!(
            msg["ghosts"],
            if slot == 2 {
                json!([])
            } else {
                json!([[ghost_id, "hq", 2, 301.3, 302.1, 0.4]])
            }
        );
        assert_eq!(msg["logistics"]["enabled"], true);
        assert!(
            msg["logistics"]["units"]
                .as_array()
                .unwrap()
                .iter()
                .all(|row| { owners[&(row["id"].as_u64().unwrap() as u32)] == slot })
        );
        let rows = msg["units"].as_array().unwrap();
        assert!(rows.iter().all(|row| {
            let owner = row[2].as_u64().unwrap() as usize;
            if slot == 2 { owner == 2 } else { owner != 2 }
        }));
        assert!(rows.iter().any(|row| row[2] == json!(slot)));
    }
    drop(frame);
    let selected = [SelectedSeat {
        slot: 2,
        full: true,
        omit: false,
    }];
    let frame = delivery
        .capture_selected_with_lists(&mut state, &selected, &[])
        .unwrap()
        .unwrap();
    assert_eq!(
        frame.recipient(2).unwrap().into_json()["nodes"],
        json!([[320.1, 320.2, 2.345, 1]])
    );
    drop(frame);
    state.elimination.as_mut().unwrap().nodes.clear();
    state.elimination.as_mut().unwrap().time_left = Some(-0.25);
    assert_eq!(
        state.ordinary_snapshot_raw_lists().unwrap().nodes,
        Some(json!([]))
    );
    let projection = SnapshotCache::new(&state).unwrap().recipient(0).unwrap();
    assert_eq!(projection.fields["nodes"], json!([]));
    assert_eq!(projection.fields["mode"]["timeLeft"], json!(0.));
}

#[test]
fn annihilation_complete_snapshots_match_mode_shape_and_omit_classic_fields() {
    let mut state = scene(Mode::Annihilation);
    assert_eq!(state.elimination.as_ref().unwrap().bunkers, vec![2, 1]);
    assert!(state.ordinary_snapshot_raw_lists().unwrap().nodes.is_none());
    let mut delivery = DeliveryState::new(&state).unwrap();
    let frame = delivery
        .capture_with_lists(&mut state, &[true; 3], &[false; 3], &[])
        .unwrap();
    for slot in 0..3 {
        let msg = frame.recipient(slot).unwrap().into_json();
        assert_eq!(
            msg["mode"],
            json!({"kind":"annihilation","suddenDeath":false,"bunkers":[2,1]})
        );
        for key in ["nodes", "mun", "fuel"] {
            assert!(msg.get(key).is_none(), "unexpected {key}");
        }
        assert_eq!(msg["logistics"]["enabled"], true);
    }
}

#[test]
fn elimination_snapshot_admission_rejects_missing_state_and_other_modes() {
    let mut state = scene(Mode::Classic);
    for (mode, kind) in [
        (Mode::World, ModeKind::World),
        (Mode::Assault, ModeKind::Assault),
        (Mode::Tutorial, ModeKind::Tutorial),
        (Mode::Annihilation, ModeKind::Classic),
    ] {
        state.options.mode = mode;
        state.world.game.mode = Some(kind);
        let expected = if mode == Mode::Horde {
            SnapshotError::Rules
        } else {
            SnapshotError::UnsupportedMode
        };
        assert_eq!(SnapshotCache::new(&state).err(), Some(expected), "{mode:?}");
    }
    // Horde is represented, but changing a Classic tag does not create its owner.
    state.options.mode = Mode::Horde;
    state.world.game.mode = Some(ModeKind::Horde);
    assert_eq!(
        SnapshotCache::new(&state).err(),
        Some(SnapshotError::Unsupported {
            unit: 0,
            capability: SnapshotCapability::Logistics,
        })
    );
    // Without the incompatible retained Classic logistics, the missing Horde
    // owner itself still refuses. Restore all logistics before later cases.
    let convoys = state.convoys.take();
    let reserves = std::mem::take(&mut state.logistics);
    state.options.logistics = false;
    assert_eq!(SnapshotCache::new(&state).err(), Some(SnapshotError::Rules));
    state.options.logistics = true;
    state.convoys = convoys;
    state.logistics = reserves;
    state.options.mode = Mode::Classic;
    state.world.game.mode = Some(ModeKind::Classic);
    state.elimination.as_mut().unwrap().time_left = Some(f64::NAN);
    assert!(SnapshotCache::new(&state).is_err());
    state.elimination = None;
    assert!(SnapshotCache::new(&state).is_err());
}
