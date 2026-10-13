use super::*;
use crate::{
    environment::WreckMetadata,
    environment_debris::{DebrisBody, DebrisKind, Vec3},
    game::Wreck,
    grid::SpatialGrid,
    world::CellChange,
};

fn number(v: &Value, key: &str) -> f64 {
    v[key].as_f64().unwrap()
}
fn wreck(state: &mut Match, v: &Value) {
    let id = v["id"].as_u64().unwrap() as u32;
    let (x, z) = (number(v, "x"), number(v, "z"));
    let moving = v["moving"].as_bool().unwrap_or(false);
    let hull = Wreck {
        id: Some(id),
        cell: None,
        x,
        z,
        moving,
    };
    if let Some(old) = state
        .world
        .game
        .wrecks
        .iter_mut()
        .find(|w| w.id == Some(id))
    {
        *old = hull;
    } else {
        state.world.game.wrecks.push(hull);
    }
    state.environment.wreck_metadata.insert(
        id,
        WreckMetadata {
            kind: state.rules.kind(v["type"].as_str().unwrap()).unwrap(),
            owner: v["owner"].as_u64().unwrap() as usize,
            rotation: number(v, "rot"),
            motion: moving.then(|| DebrisBody {
                id: format!("wreck:{id}"),
                kind: DebrisKind::Wreck,
                x,
                y: 0.08,
                z,
                vx: 0.13,
                vy: -0.02,
                vz: 0.27,
                dir: 0.35,
                material: "steel".into(),
                mass: 8500.,
                radius: 1.,
                origin: Vec3 { x, y: 0., z },
                gravity: 9.81,
                drag: 3.2,
                stamp: 1.25,
                born: 0.5,
                contacts: 0,
                settled: false,
                tilt: 0.007,
            }),
        },
    );
}
#[test]
fn full_snapshots_consume_canonical_observed_lists_across_visibility_and_rediscovery() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/recipient-snapshot.json"
    ))
    .unwrap();
    for trace in fixture["listTraces"].as_array().unwrap() {
        let mut state = super::super::tests::initialize(&trace["initial"]);
        let mut delivery = DeliveryState::new(&state).unwrap();
        for action in trace["actions"].as_array().unwrap() {
            if let Some(tick) = action["tick"].as_i64() {
                state.world.game.tick = tick;
            }
            if let Some(moves) = action["move"].as_array() {
                for movement in moves {
                    let unit = state
                        .world
                        .game
                        .units
                        .get_mut(&(movement["id"].as_u64().unwrap() as u32))
                        .unwrap();
                    unit.x = number(movement, "x");
                    unit.z = number(movement, "z");
                }
            }
            if let Some(wrecks) = action["wrecks"].as_array() {
                for row in wrecks {
                    wreck(&mut state, row);
                }
            }
            if let Some(remove) = action["remove"].as_array() {
                for id in remove {
                    let id = id.as_u64().unwrap() as u32;
                    state.world.game.wrecks.retain(|w| w.id != Some(id));
                    state.environment.wreck_metadata.remove(&id);
                }
            }
            if let Some(mines) = action["mines"].as_array() {
                for mine in mines {
                    state
                        .world
                        .mutate_cell(
                            mine["cell"].as_u64().unwrap() as usize,
                            CellChange {
                                mine: Some(mine["value"].as_bool().unwrap()),
                                ..Default::default()
                            },
                        )
                        .unwrap();
                }
            }
            if let Some(forget) = action["forget"].as_array() {
                for cell in forget {
                    state
                        .world
                        .mine_seen
                        .remove(&(cell.as_u64().unwrap() as usize));
                }
            }
            if let Some(seen) = action["seen"].as_array() {
                for mine in seen {
                    state.world.mine_seen.insert(
                        mine["cell"].as_u64().unwrap() as usize,
                        mine["bits"].as_u64().unwrap() as u32,
                    );
                }
            }
            if let Some(reveal) = action["reveal"].as_bool() {
                state.world.game.reveal = reveal;
            }
            if action["vision"] == true {
                let grid = SpatialGrid::new(&state.world.game.units, 16.);
                vision::update_vision(&mut state.world.game, &state.rules, &grid);
            }
            let frame = delivery
                .capture_with_lists(&mut state, &[true; 3], &[false; 3], &[])
                .unwrap();
            for slot in 0..3 {
                super::super::tests::equal(
                    &frame.recipient(slot).unwrap().into_json(),
                    &action["outputs"][slot],
                    &format!("{} recipient{slot}", action["name"]),
                );
            }
        }
    }
}

#[test]
fn all_producers_admit_before_any_recipient_memory_or_delivery_changes() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/recipient-snapshot.json"
    ))
    .unwrap();
    let mut state = super::super::tests::initialize(&fixture["listTraces"][0]["initial"]);
    // Only the last seat can see the hull with missing display metadata.
    let ally = state.world.game.units.get_mut(&2).unwrap();
    ally.x = 41.;
    ally.z = 41.;
    wreck(
        &mut state,
        &json!({"id":9002,"type":"tank","owner":2,"x":43.14,"z":41.26,"rot":-0.26}),
    );
    state.world.game.wrecks.push(Wreck {
        id: Some(9001),
        cell: None,
        x: 183.,
        z: 101.,
        moving: false,
    });
    let mut delivery = DeliveryState::new(&state).unwrap();
    let before = delivery.clone();
    let remembered = (0..3)
        .map(|slot| state.recipient_routing(slot).unwrap().remembered().to_vec())
        .collect::<Vec<_>>();
    let wrecks = (0..3)
        .map(|slot| {
            state
                .recipient_routing(slot)
                .unwrap()
                .remembered_wreck_rows()
                .unwrap()
        })
        .collect::<Vec<_>>();
    let pending = (0..3)
        .map(|slot| {
            state
                .world
                .pending(crate::world::Recipient::Player(slot))
                .unwrap()
                .clone()
        })
        .collect::<Vec<_>>();
    assert!(matches!(
        delivery.capture_with_lists(&mut state, &[true; 3], &[false; 3], &[]),
        Err(SnapshotError::Rules)
    ));
    assert_eq!(delivery, before);
    for slot in 0..3 {
        assert_eq!(
            state.recipient_routing(slot).unwrap().remembered(),
            remembered[slot]
        );
        assert_eq!(
            state
                .recipient_routing(slot)
                .unwrap()
                .remembered_wreck_rows()
                .unwrap(),
            wrecks[slot]
        );
        assert_eq!(
            state
                .world
                .pending(crate::world::Recipient::Player(slot))
                .unwrap(),
            &pending[slot]
        );
    }
    state.world.game.wrecks.retain(|w| w.id != Some(9001));
    state.world.set_cell(421, b'T').unwrap();
    state.world.materials.insert(421, "invalid".into());
    assert!(matches!(
        delivery.capture_with_lists(&mut state, &[true; 3], &[false; 3], &[]),
        Err(SnapshotError::Rules)
    ));
    assert_eq!(delivery, before);
    for slot in 0..3 {
        assert_eq!(
            state.recipient_routing(slot).unwrap().remembered(),
            remembered[slot]
        );
        assert_eq!(
            state
                .recipient_routing(slot)
                .unwrap()
                .remembered_wreck_rows()
                .unwrap(),
            wrecks[slot]
        );
    }
    state.world.materials.remove(&421);
    assert!(matches!(
        delivery.capture_with_lists(&mut state, &[true; 2], &[false; 3], &[]),
        Err(SnapshotError::InvalidRecipient)
    ));
    assert_eq!(delivery, before);
    for (slot, remembered) in wrecks.iter().enumerate() {
        assert_eq!(
            state
                .recipient_routing(slot)
                .unwrap()
                .remembered_wreck_rows()
                .unwrap(),
            *remembered
        );
    }
    let frame = delivery
        .capture_with_lists(&mut state, &[true; 3], &[false; 3], &[])
        .unwrap();
    assert_eq!(
        frame.recipient(0).unwrap().into_json()["wrecks"][0][0],
        9002
    );
}
