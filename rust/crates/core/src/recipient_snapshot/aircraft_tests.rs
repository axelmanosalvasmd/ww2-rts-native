use super::*;
use crate::{
    economy::Faction,
    economy_state::ArmySize,
    map::AuthoritativeMap,
    match_aviation::{AviationEvent, PlaneMission},
    match_runtime::{MatchOptions, PlayerConfig},
    match_tick::TickEvent,
    weather::WeatherChoice,
};

fn scene() -> Match {
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"Aircraft snapshots", "w":64, "h":64,
            "rows":vec![".".repeat(64);64],
            "spawns":[{"x":8,"y":8},{"x":8,"y":54},{"x":54,"y":54}],
            "points":[{"x":32,"y":32}]
        }))
        .unwrap(),
    )
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
    // Exercise the state-only slice. Automatic weapon actuators and aviation
    // shot encodings remain outside its admitted runtime/contract scope.
    for control in state.commands.units.values_mut() {
        control.hold_fire = true;
        control.auto_cast = false;
        control.auto_retreat = false;
    }
    state
}

fn selected() -> Vec<SelectedSeat> {
    (0..3)
        .map(|slot| SelectedSeat {
            slot,
            full: false,
            omit: false,
        })
        .collect()
}

fn capture(state: &mut Match, delivery: &mut DeliveryState, events: &[TickEvent]) -> Vec<Value> {
    let frame = delivery
        .capture_selected_with_tick_events(state, &selected(), events)
        .unwrap()
        .unwrap();
    (0..3)
        .map(|slot| frame.recipient(slot).unwrap().into_json())
        .collect()
}

#[test]
fn aircraft_owner_rows_and_plans_match_wire_codes_without_leaking_to_allies_or_enemies() {
    let mut state = scene();
    // The projection matrix includes Area, which requires a bomber salvo weapon.
    let id = state.spawn_aircraft(0, "bomber").unwrap();
    let point = Point { x: 41.25, z: 53.26 };
    for (air, code, hidden) in [
        (PlaneState::Base, 0, true),
        (PlaneState::Out, 1, false),
        (PlaneState::Station, 2, false),
        (PlaneState::Home, 3, false),
        (PlaneState::Rearm, 4, true),
    ] {
        state.world.game.units.get_mut(&id).unwrap().air = Some(air);
        let runtime = state.aviation.planes.get_mut(&id).unwrap();
        runtime.fuel = 12.01;
        runtime.ammo = 2.;
        runtime.timer = 3.01;
        for (mission, plan_code) in [
            (MissionKind::Patrol, 1),
            (MissionKind::Escort, 1),
            (MissionKind::Attack, 4),
            (MissionKind::Area, 5),
        ] {
            state.aviation.planes.get_mut(&id).unwrap().mission = Some(PlaneMission {
                kind: mission,
                target: None,
                at: point,
            });
            // Deliberately stale visibility cannot expose a grounded enemy.
            for player in &mut state.world.game.players {
                player.visible = vec![id];
            }
            let cache = SnapshotCache::new(&state).unwrap();
            for slot in 0..3 {
                let fields = cache.recipient(slot).unwrap().fields;
                assert_eq!(
                    fields["air"],
                    if slot == 0 {
                        json!([[id, code, 13.0, 2.0, 4.0]])
                    } else {
                        json!([])
                    }
                );
                let plan = fields["plans"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|row| row[0] == id);
                if slot == 0 {
                    assert_eq!(
                        plan.unwrap(),
                        &if hidden {
                            json!([id, 0, 0.0, 0.0])
                        } else {
                            json!([id, plan_code, 41.3, 53.3])
                        }
                    );
                } else {
                    assert!(plan.is_none());
                }
                let row = fields["units"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|row| row[0] == id);
                assert_eq!(row.is_some(), slot != 2 || !hidden);
                if let Some(row) = row {
                    assert_eq!(row.as_array().unwrap().len(), 15);
                    assert_eq!(row[12].as_u64().unwrap() & 512 != 0, hidden);
                    if slot != 0 {
                        assert_eq!(row[11].as_f64(), Some(0.));
                        assert_eq!(row[12].as_u64().unwrap() & (2048 | 4096 | 8192 | 16384), 0);
                    }
                }
            }
        }
        state.world.game.players[2].visible.clear();
        assert!(
            !SnapshotCache::new(&state)
                .unwrap()
                .recipient(2)
                .unwrap()
                .fields["units"]
                .as_array()
                .unwrap()
                .iter()
                .any(|row| row[0] == id)
        );
    }
    state.world.game.reveal = true;
    assert!(
        SnapshotCache::new(&state)
            .unwrap()
            .recipient(2)
            .unwrap()
            .fields["units"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row[0] == id)
    );
}

#[test]
fn aircraft_sorties_survive_ordinary_ticks_snapshots_rearm_and_match_finish() {
    for kind in ["fighter", "attacker", "bomber"] {
        let mut state = scene();
        // Purchase/production currently rejects aircraft. Use the existing
        // trusted aircraft publisher without widening that admission boundary.
        let id = state.spawn_aircraft(0, kind).unwrap();
        state.commands.units.get_mut(&id).unwrap().hold_fire = true;
        state.preflight_ordinary_snapshot().unwrap();
        state.validate_tick_state().unwrap();
        let mut delivery = DeliveryState::new(&state).unwrap();
        assert_eq!(capture(&mut state, &mut delivery, &[])[0]["air"][0][1], 0);
        for _ in 0..3 {
            let tick = state.advance_tick().unwrap();
            assert_eq!(
                capture(&mut state, &mut delivery, &tick.events)[0]["air"][0][1],
                0
            );
        }
        let mission = PlaneMission {
            kind: MissionKind::Patrol,
            target: None,
            at: Point { x: 60., z: 45. },
        };
        assert!(state.send_aircraft_mission(0, id, mission).unwrap());
        let mut saw = [false; 5];
        for _ in 0..1200 {
            let tick = state.advance_tick().unwrap();
            let snapshots = capture(&mut state, &mut delivery, &tick.events);
            let code = snapshots[0]["air"][0][1].as_u64().unwrap() as usize;
            saw[code] = true;
            for snapshot in &snapshots[1..] {
                assert_eq!(snapshot["air"], json!([]));
            }
            if code == 2 {
                state.return_aircraft_home(0, id).unwrap();
            }
            if saw[4] && code == 0 {
                break;
            }
        }
        assert_eq!(saw, [true; 5], "{kind} completes each sortie state");
        // End through the ordinary victory phase, then collect ending frames.
        state.start.economy.players[0].vp = state.start.win_vp;
        for _ in 0..3 {
            let tick = state.advance_tick().unwrap();
            let snapshots = capture(&mut state, &mut delivery, &tick.events);
            assert_eq!(snapshots[0]["winner"], 0);
            assert_eq!(snapshots[0]["air"][0][1], 0);
        }
        assert!(state.finish.is_some());
        assert!(state.tick_state.fault().is_none());
    }
}

#[test]
fn invalid_aircraft_metadata_and_uncontracted_events_preserve_delivery_state() {
    let mut state = scene();
    let id = state.spawn_aircraft(0, "fighter").unwrap();
    let mut delivery = DeliveryState::new(&state).unwrap();
    let original = state.aviation.planes[&id];
    for bad in ["missing", "nonfinite", "orphan", "state"] {
        match bad {
            "missing" => {
                state.aviation.planes.remove(&id);
            }
            "nonfinite" => {
                state.aviation.planes.get_mut(&id).unwrap().fuel = f64::NAN;
            }
            "orphan" => {
                state.aviation.planes.insert(9999, original);
            }
            "state" => {
                state.world.game.units.get_mut(&id).unwrap().air = None;
            }
            _ => unreachable!(),
        }
        let acknowledgments = delivery.clone();
        let world = format!("{:?}", state.world);
        let memory = format!("{:?}", state.recipients);
        assert!(
            delivery
                .capture_selected_with_lists(&mut state, &selected(), &[])
                .is_err(),
            "{bad}"
        );
        assert_eq!(delivery, acknowledgments);
        assert_eq!(format!("{:?}", state.world), world);
        assert_eq!(format!("{:?}", state.recipients), memory);
        state.aviation.planes.clear();
        state.aviation.planes.insert(id, original);
        state.world.game.units.get_mut(&id).unwrap().air = Some(PlaneState::Base);
    }
    for event in [
        AviationEvent::AntiAir {
            shooter: id,
            target: id,
            owner: 0,
            target_owner: 0,
            at: Point { x: 10., z: 10. },
        },
        AviationEvent::PlaneDown {
            target: id,
            owner: 0,
            at: Point { x: 10., z: 10. },
            direction: 0.,
        },
    ] {
        let before = delivery.clone();
        assert!(matches!(
            delivery.capture_selected_with_tick_events(
                &mut state,
                &selected(),
                &[TickEvent::Aviation(event.clone())]
            ),
            Err(SnapshotError::Unsupported {
                capability: SnapshotCapability::Air,
                ..
            })
        ));
        assert_eq!(delivery, before);
        state.aviation.events.push(event);
        assert!(matches!(
            state.preflight_ordinary_snapshot(),
            Err(SnapshotError::Unsupported {
                capability: SnapshotCapability::Air,
                ..
            })
        ));
        state.aviation.events.clear();
    }
}
