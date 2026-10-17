//! AI-50 engineering orders survive actual ordinary mode constructors and ticks.
use crate::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    field_works::FortKind,
    game::Point,
    map::AuthoritativeMap,
    match_commands::Command,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    orders::OrderKey,
    recipient_snapshot::{DeliveryState, SelectedSeat, SnapshotCache, SnapshotError},
    weather::WeatherChoice,
};
use serde_json::{Value, json};

fn scene(mode: Mode) -> (Match, u32) {
    let map = AuthoritativeMap::from_value(json!({
        "name":"HQ engineering in ordinary modes", "w":120,"h":100,
        "rows":vec![".".repeat(120);100],
        "heights":vec!["0".repeat(120);100],
        "spawns":[{"x":18,"y":30},{"x":100,"y":70}],
        "defend":[0], "assaultTime":300,
        "points":[{"x":60,"y":50,"vp":1,"mp":1}]
    }))
    .unwrap();
    let options = MatchOptions {
        seed: 701,
        mode,
        players: (0..2)
            .map(|slot| PlayerConfig {
                name: format!("p{slot}"),
                team: slot,
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
    };
    let mut state = match mode {
        Mode::Default | Mode::Conquest => Match::new(&map, options),
        Mode::Assault => Match::new_assault(&map, options, 0),
        Mode::Annihilation => Match::new_annihilation(&map, options),
        Mode::Classic => Match::new_classic(&map, options),
        _ => unreachable!(),
    }
    .unwrap();
    assert_eq!(state.options.mode, mode);
    assert!(crate::match_actor::ordinary_ground_mode(
        &state.world.game,
        mode
    ));
    let kind = if mode.is_construction() {
        "engineer"
    } else {
        "rifle"
    };
    let builder = state
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 0 && state.rules.kinds[u.kind].name == kind)
        .unwrap()
        .id;
    relocate(&mut state, builder, Point { x: 80.0, z: 40.0 });
    for control in state.commands.units.values_mut() {
        control.auto_cast = false;
        control.auto_retreat = false;
        control.hold_fire = true;
    }
    state.start.economy.players[0].mp = 2000.0;
    (state, builder)
}
fn relocate(state: &mut Match, id: u32, at: Point) {
    let unit = state.world.game.units.get_mut(&id).unwrap();
    unit.x = at.x;
    unit.z = at.z;
    state.grid.update(id, unit);
}
fn enqueue(state: &mut Match, sequence: u64, command: Value) {
    state
        .enqueue_command(
            OrderKey {
                tick: state.world.game.tick as u64 + 1,
                issuer: 0,
                sequence,
            },
            Command::from_json(&command).unwrap(),
        )
        .unwrap();
}
fn collect_classic_complete(state: &mut Match, delivery: &mut DeliveryState) {
    let expected = crate::recipient_snapshot::mode_row(state).unwrap();
    assert_eq!(expected["kind"], "classic");
    let count = state.start.players.len();
    let selected = (0..count)
        .map(|slot| SelectedSeat {
            slot,
            full: false,
            omit: false,
        })
        .collect::<Vec<_>>();
    let expected_units: Vec<_> = (0..count)
        .map(|slot| {
            let own: Vec<_> = state
                .world
                .game
                .units
                .values()
                .filter(|unit| unit.owner == slot)
                .map(|unit| u64::from(unit.id))
                .collect();
            let hidden: Vec<_> = state
                .world
                .game
                .units
                .values()
                .filter(|unit| {
                    !state.world.game.reveal
                        && !state.world.game.allied(unit.owner, slot)
                        && !state.world.game.players[slot].sees(unit.id)
                })
                .map(|unit| u64::from(unit.id))
                .collect();
            (own, hidden)
        })
        .collect();
    let jobs = state.field_works.jobs().len();
    let frame = delivery
        .capture_selected_with_tick_events(state, &selected, &[])
        .unwrap()
        .unwrap();
    assert_eq!(
        frame.slots().collect::<Vec<_>>(),
        (0..count).collect::<Vec<_>>()
    );
    for (slot, (own, hidden)) in expected_units.iter().enumerate() {
        let snapshot = frame.recipient(slot).unwrap().into_json();
        assert_eq!(snapshot["t"], "s");
        assert_eq!(snapshot["mode"], expected, "Classic seat {slot}");
        assert!(snapshot.get("missing").is_none());
        for field in [
            "units",
            "works",
            "wrecks",
            "foundMines",
            "shots",
            "flights",
            "smokes",
            "strikes",
        ] {
            assert!(snapshot[field].is_array(), "Classic seat {slot}: {field}");
        }
        let rows = snapshot["units"].as_array().unwrap();
        for id in own {
            assert!(rows.iter().any(|row| row[0].as_u64() == Some(*id)));
        }
        for id in hidden {
            assert!(rows.iter().all(|row| row[0].as_u64() != Some(*id)));
        }
        assert_eq!(
            snapshot["works"].as_array().unwrap().len(),
            if slot == 0 { jobs } else { 0 }
        );
        assert!(snapshot["cells"].is_array());
        assert!(snapshot["units"].is_array());
        assert!(snapshot["works"].is_array());
    }
}

fn tick_and_collect(state: &mut Match, delivery: Option<&mut DeliveryState>) {
    let report = state.advance_tick().unwrap();
    assert!(
        report.commands.iter().all(|result| result.denial.is_none()),
        "{:?} tick {}: {:?}",
        state.options.mode,
        state.world.game.tick,
        report.commands
    );
    for &id in state.world.game.units.keys() {
        state.validate_gate_actor(id).unwrap();
    }
    let Some(delivery) = delivery else {
        // Unsupported mode snapshot bounds remain independent of Gate admission.
        assert!(matches!(
            SnapshotCache::new(state),
            Err(SnapshotError::UnsupportedMode)
        ));
        return;
    };
    let snapshot_mode = (!matches!(state.options.mode, Mode::Default | Mode::Conquest))
        .then(|| crate::recipient_snapshot::mode_row(state).unwrap());
    let selected: Vec<_> = (0..state.start.players.len())
        .map(|slot| SelectedSeat {
            slot,
            full: false,
            omit: false,
        })
        .collect();
    let frame = delivery
        .capture_selected_with_tick_events(state, &selected, &report.events)
        .unwrap()
        .unwrap();
    assert_eq!(
        frame.slots().collect::<Vec<_>>(),
        selected.iter().map(|seat| seat.slot).collect::<Vec<_>>()
    );
    for seat in selected {
        let snapshot = frame.recipient(seat.slot).unwrap().into_json();
        assert_eq!(snapshot["t"], "s");
        assert!(snapshot.get("missing").is_none());
        for field in ["wrecks", "foundMines", "shots", "flights"] {
            assert!(snapshot[field].is_array(), "seat {}: {field}", seat.slot);
        }
        assert!(snapshot["units"].is_array());
        assert!(snapshot["works"].is_array());
        if let Some(expected) = &snapshot_mode {
            assert_eq!(snapshot.get("mode"), Some(expected));
        } else {
            assert!(snapshot.get("mode").is_none());
        }
    }
}
fn count(state: &Match, kind: &str) -> usize {
    state
        .world
        .game
        .units
        .values()
        .filter(|unit| {
            unit.owner == 0 && state.rules.kinds[unit.kind].name == kind && unit.built == Some(1.0)
        })
        .count()
}

#[test]
fn intent_ai50_gate_order_completes_in_each_real_ordinary_mode_with_supported_seat_snapshots() {
    for mode in [
        Mode::Default,
        Mode::Conquest,
        Mode::Assault,
        Mode::Annihilation,
        Mode::Classic,
    ] {
        let (mut state, builder) = scene(mode);
        if mode == Mode::Classic {
            assert_eq!(
                crate::recipient_snapshot::mode_row(&state).unwrap(),
                json!({"kind":"classic","timeLeft":1500.0,"suddenDeath":false})
            );
        }
        let before = count(&state, "gate");
        let first_gate = state.next_unit_id;
        let spent = state.story[0].mp_spent;
        enqueue(
            &mut state,
            1,
            json!({"t":"dig","kind":"gate","ids":[builder],
            "x":80.0,"z":45.0,"dir":0.0}),
        );
        let mut delivery = if matches!(
            mode,
            Mode::Default | Mode::Conquest | Mode::Classic | Mode::Assault | Mode::Annihilation
        ) {
            Some(DeliveryState::new(&state).unwrap())
        } else {
            assert!(matches!(
                SnapshotCache::new(&state),
                Err(SnapshotError::UnsupportedMode)
            ));
            None
        };
        if mode == Mode::Classic {
            collect_classic_complete(&mut state, delivery.as_mut().unwrap());
        }
        tick_and_collect(&mut state, delivery.as_mut());
        let job = state.field_works.jobs().get(&builder).unwrap();
        assert_eq!(job.kind, FortKind::Gate);
        assert_eq!(job.cells.len(), 3);
        assert_eq!(state.story[0].mp_spent, spent + 150.0);
        let cells: Vec<_> = job.cells.iter().map(|cell| cell.cell).collect();
        let mut completed = false;
        for _ in 0..1200 {
            tick_and_collect(&mut state, delivery.as_mut());
            if !state.field_works.is_active(builder) && count(&state, "gate") == before + 3 {
                completed = true;
                break;
            }
        }
        assert!(completed, "{mode:?}: paid Gate work did not complete");
        for cell in cells {
            let gate = &state.world.game.units[&state.building_cells[&cell].unit];
            assert!(gate.id >= first_gate);
            assert_eq!(state.rules.kinds[gate.kind].name, "gate");
            assert_eq!(gate.built, Some(1.0));
            assert!(state.commands.units[&gate.id].gate.is_some());
            assert_eq!(gate.cells.as_deref(), Some([cell as u32].as_slice()));
        }
        for _ in 0..12 {
            tick_and_collect(&mut state, delivery.as_mut());
        }
        assert_eq!(count(&state, "gate"), before + 3);
        assert!(!state.world.game.units[&builder].dig);
        assert!(
            state
                .world
                .game
                .units
                .values()
                .filter(|unit| { unit.owner == 0 && state.rules.kinds[unit.kind].name == "gate" })
                .all(|gate| state.commands.units[&gate.id].gate.unwrap().open.is_some())
        );
    }
}

#[test]
fn intent_ai50_wall_fronts_queue_and_complete_through_ordinary_conquest_snapshots() {
    let (mut state, builder) = scene(Mode::Conquest);
    relocate(&mut state, builder, Point { x: 50.0, z: 50.0 });
    let other = state
        .world
        .game
        .units
        .values()
        .find(|unit| {
            unit.owner == 0 && unit.id != builder && state.rules.kinds[unit.kind].name == "rifle"
        })
        .unwrap()
        .id;
    relocate(&mut state, other, Point { x: 52.0, z: 50.0 });
    let before = count(&state, "wall");
    let spent = state.story[0].mp_spent;
    // The same two existing Entrench shapes as the planner's step 2.
    enqueue(
        &mut state,
        1,
        json!({"t":"entrench","ids":[builder,other],"fort":"wall","pattern":"line",
        "x":55.0,"z":54.0,"x2":55.0,"z2":64.0,"queue":false}),
    );
    enqueue(
        &mut state,
        2,
        json!({"t":"entrench","ids":[builder,other],"fort":"wall","pattern":"line",
        "x":55.0,"z":46.0,"x2":55.0,"z2":36.0,"queue":true}),
    );
    let mut delivery = DeliveryState::new(&state).unwrap();
    tick_and_collect(&mut state, Some(&mut delivery));
    assert!(state.field_works.membership(builder).is_some());
    assert!(state.field_works.membership(other).is_some());
    assert!(
        state.commands.units[&builder]
            .orders
            .iter()
            .any(|command| matches!(command, Command::Entrench { .. }))
    );
    let mut saw_paid = false;
    let mut completed = false;
    for _ in 0..2200 {
        tick_and_collect(&mut state, Some(&mut delivery));
        saw_paid |= state
            .field_works
            .jobs()
            .values()
            .any(|job| job.kind == FortKind::Wall);
        if [builder, other].iter().all(|id| {
            let unit = &state.world.game.units[id];
            !unit.entrench && !unit.dig && state.commands.units[id].orders.is_empty()
        }) && count(&state, "wall") >= before + 8
        {
            completed = true;
            break;
        }
    }
    assert!(saw_paid);
    assert!(completed, "both queued wall fronts must finish");
    assert!(state.story[0].mp_spent > spent);
    let walls: Vec<_> = state
        .world
        .game
        .units
        .values()
        .filter(|unit| unit.owner == 0 && state.rules.kinds[unit.kind].name == "wall")
        .collect();
    assert!(walls.iter().any(|wall| wall.z > 54.0));
    assert!(walls.iter().any(|wall| wall.z < 46.0));
    assert!(walls.iter().all(|wall| wall.z <= 47.0 || wall.z >= 53.0));
    for _ in 0..12 {
        tick_and_collect(&mut state, Some(&mut delivery));
    }
}
