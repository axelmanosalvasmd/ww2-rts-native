use super::*;
use crate::{environment::WreckMetadata, game::Wreck, grid::SpatialGrid, world::Recipient};

fn fixture() -> Value {
    serde_json::from_str(include_str!("../../tests/fixtures/recipient-selected.json")).unwrap()
}
fn seat(slot: usize) -> SelectedSeat {
    SelectedSeat {
        slot,
        full: false,
        omit: false,
    }
}
fn initial() -> Match {
    super::super::tests::initialize(&fixture()["initial"])
}
fn apply(state: &mut Match, action: &Value) {
    state.world.game.tick = action["tick"].as_i64().unwrap();
    if let Some(cell) = action["cell"].as_u64() {
        state
            .world
            .set_cell(cell as usize, action["ch"].as_str().unwrap().as_bytes()[0])
            .unwrap();
    }
    if let Some(moves) = action["move"].as_array() {
        for movement in moves {
            let u = state
                .world
                .game
                .units
                .get_mut(&(movement["id"].as_u64().unwrap() as u32))
                .unwrap();
            u.x = movement["x"].as_f64().unwrap();
            u.z = movement["z"].as_f64().unwrap();
        }
    }
    if let Some(wrecks) = action["wrecks"].as_array() {
        for w in wrecks {
            let id = w["id"].as_u64().unwrap() as u32;
            state.world.game.wrecks.push(Wreck {
                id: Some(id),
                cell: None,
                x: w["x"].as_f64().unwrap(),
                z: w["z"].as_f64().unwrap(),
                moving: false,
            });
            state.environment.wreck_metadata.insert(
                id,
                WreckMetadata {
                    kind: state.rules.kind(w["type"].as_str().unwrap()).unwrap(),
                    owner: w["owner"].as_u64().unwrap() as usize,
                    rotation: w["rot"].as_f64().unwrap(),
                    motion: None,
                },
            );
        }
    }
    if action["vision"] == true {
        let grid = SpatialGrid::new(&state.world.game.units, 16.);
        vision::update_vision(&mut state.world.game, &state.rules, &grid);
    }
}
fn skipped(state: &Match, slot: usize) -> (String, String) {
    // Inspect canonical state directly. A public collector would refresh it.
    (
        format!("{:?}", state.recipients[slot]),
        format!("{:?}", state.world.pending(Recipient::Player(slot))),
    )
}
#[test]
fn selected_delivery_matches_actual_room_selection_and_preserves_skipped_seats() {
    let fixture = fixture();
    let mut state = super::super::tests::initialize(&fixture["initial"]);
    let mut delivery = DeliveryState::new(&state).unwrap();
    for action in fixture["actions"].as_array().unwrap() {
        apply(&mut state, action);
        let selected = action["selected"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| SelectedSeat {
                slot: s["slot"].as_u64().unwrap() as usize,
                full: s["full"].as_bool().unwrap(),
                omit: s["omit"].as_bool().unwrap(),
            })
            .collect::<Vec<_>>();
        let skipped_before = (0..3)
            .filter(|slot| !selected.iter().any(|s| s.slot == *slot))
            .map(|slot| {
                (
                    slot,
                    skipped(&state, slot),
                    delivery.seat_fog_for_test(slot),
                )
            })
            .collect::<Vec<_>>();
        if action["manual"].is_array() {
            let frame = delivery
                .capture_selected(&mut state, &selected)
                .unwrap()
                .unwrap();
            assert_eq!(
                frame.slots().collect::<Vec<_>>(),
                selected.iter().map(|s| s.slot).collect::<Vec<_>>()
            );
            for output in action["outputs"].as_array().unwrap() {
                let slot = output["slot"].as_u64().unwrap() as usize;
                let actual = Value::Object(frame.recipient(slot).unwrap().fields);
                let mut fields = Map::new();
                fields.insert("cells".into(), actual["cells"].clone());
                if let Some(fog) = actual.get("fog") {
                    fields.insert("fog".into(), fog.clone());
                }
                super::super::tests::equal(
                    &Value::Object(fields),
                    &output["fields"],
                    action["name"].as_str().unwrap(),
                );
            }
            for (slot, _, _) in &skipped_before {
                assert!(matches!(
                    frame.recipient(*slot),
                    Err(SnapshotError::InvalidRecipient)
                ));
            }
        } else {
            let frame = delivery
                .capture_selected_with_lists(&mut state, &selected, &[])
                .unwrap();
            if selected.is_empty() {
                assert!(frame.is_none());
            } else {
                let frame = frame.unwrap();
                assert_eq!(
                    frame.slots().collect::<Vec<_>>(),
                    selected.iter().map(|s| s.slot).collect::<Vec<_>>()
                );
                for output in action["outputs"].as_array().unwrap() {
                    let slot = output["slot"].as_u64().unwrap() as usize;
                    super::super::tests::equal(
                        &frame.recipient(slot).unwrap().into_json(),
                        &output["fields"],
                        action["name"].as_str().unwrap(),
                    );
                }
                for (slot, _, _) in &skipped_before {
                    assert!(matches!(
                        frame.recipient(*slot),
                        Err(SnapshotError::InvalidRecipient)
                    ));
                }
            }
        }
        for (slot, memory, fog) in skipped_before {
            assert_eq!(skipped(&state, slot), memory);
            assert_eq!(delivery.seat_fog_for_test(slot), fog);
        }
    }
}
#[test]
fn empty_and_invalid_selection_have_no_producer_or_acknowledgment_effects() {
    let mut state = initial();
    let mut delivery = DeliveryState::new(&state).unwrap();
    let before = delivery.clone();
    let memories_before_empty = format!("{:?}", state.recipients);
    let world_before_empty = format!("{:?}", state.world);
    let fog_before_empty = format!("{:?}", state.routing_fog);
    // Invalid canonical actors prove that empty selection never builds a cache.
    let id = *state.world.game.units.keys().next().unwrap();
    let kind = state.world.game.units[&id].kind;
    state.world.game.units.get_mut(&id).unwrap().kind = usize::MAX;
    assert!(
        delivery
            .capture_selected(&mut state, &[])
            .unwrap()
            .is_none()
    );
    assert!(
        delivery
            .capture_selected_with_lists(&mut state, &[], &[])
            .unwrap()
            .is_none()
    );
    state.world.game.units.get_mut(&id).unwrap().kind = kind;
    assert_eq!(delivery, before);
    assert_eq!(format!("{:?}", state.recipients), memories_before_empty);
    assert_eq!(format!("{:?}", state.world), world_before_empty);
    assert_eq!(format!("{:?}", state.routing_fog), fog_before_empty);
    let memories = format!("{:?}", state.recipients);
    let world = format!("{:?}", state.world);
    let fog = format!("{:?}", state.routing_fog);
    for selected in [vec![seat(0), seat(3)], vec![seat(0), seat(0)]] {
        assert!(matches!(
            delivery.capture_selected_with_lists(&mut state, &selected, &[]),
            Err(SnapshotError::InvalidRecipient)
        ));
        assert_eq!(delivery, before);
        assert_eq!(format!("{:?}", state.recipients), memories);
        assert_eq!(format!("{:?}", state.world), world);
        assert_eq!(format!("{:?}", state.routing_fog), fog);
    }
}
#[test]
fn later_selected_terrain_or_list_refusal_is_whole_admission_atomic() {
    let mut state = initial();
    let mut delivery = DeliveryState::new(&state).unwrap();
    let bad = 2020;
    let good = bad + 1;
    state.world.set_cell(bad, b'T').unwrap();
    for slot in 0..2 {
        state.observed_terrain(slot, false).unwrap();
    }
    state.world.set_cell(good, b'T').unwrap();
    state.world.materials.insert(bad, "invalid".into());
    let assert_unchanged =
        |state: &mut Match, delivery: &mut DeliveryState, selected: &[SelectedSeat]| {
            let memories = format!("{:?}", state.recipients);
            let world = format!("{:?}", state.world);
            let fog = format!("{:?}", state.routing_fog);
            let before = delivery.clone();
            assert!(matches!(
                delivery.capture_selected_with_lists(state, selected, &[]),
                Err(SnapshotError::Rules)
            ));
            assert_eq!(format!("{:?}", state.recipients), memories);
            assert_eq!(format!("{:?}", state.world), world);
            assert_eq!(format!("{:?}", state.routing_fog), fog);
            assert_eq!(*delivery, before);
        };
    state.preflight_observed_terrain(0).unwrap();
    assert!(state.preflight_observed_terrain(2).is_err());
    assert_unchanged(&mut state, &mut delivery, &[seat(0), seat(2)]);
    state.world.materials.remove(&bad);
    // A hull seen only by the later selected owner lacks display metadata.
    state.world.game.units.get_mut(&2).unwrap().x = 41.;
    state.world.game.units.get_mut(&2).unwrap().z = 41.;
    state.world.game.wrecks.push(Wreck {
        id: Some(9001),
        cell: None,
        x: 183.,
        z: 101.,
        moving: false,
    });
    assert_unchanged(&mut state, &mut delivery, &[seat(0), seat(2)]);
    // The skipped malformed list owner must not block selected lazy producers.
    let frame = delivery
        .capture_selected_with_lists(&mut state, &[seat(0)], &[])
        .unwrap()
        .unwrap();
    assert!(frame.recipient(0).is_ok());
    assert!(matches!(
        frame.recipient(2),
        Err(SnapshotError::InvalidRecipient)
    ));
}
