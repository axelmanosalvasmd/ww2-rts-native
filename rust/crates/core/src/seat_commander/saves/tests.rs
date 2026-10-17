use super::*;
use crate::{match_commands::Command, orders::OrderKey, recipient_snapshot::DeliveryState};

fn ordinary() -> Match {
    let mut state = crate::save_state::tests::ordinary();
    for control in state.commands.units.values_mut() {
        control.hold_fire = true;
        control.auto_cast = false;
        control.auto_retreat = false;
    }
    state
}
fn step(
    state: &mut Match,
    seat: &mut SeatCommander,
    memory: &mut HumanObservationMemory,
) -> Option<Value> {
    let tick = state.world.game.tick as u64;
    let delivered = (tick.is_multiple_of(2) && seat.wants_view(tick as i64, true))
        .then(|| Delivered::Human(Box::new(state.human_commander_observation(memory).unwrap())));
    let Advance::Human(advance) = state
        .advance_seat_commander(seat, delivered, tick, &[])
        .unwrap()
    else {
        panic!("ordinary seat must use the human brain");
    };
    let command = advance.input.and_then(|input| input.command);
    if let Some(command) = &command {
        state
            .enqueue_command(
                OrderKey {
                    tick: tick + 1,
                    issuer: 0,
                    sequence: tick,
                },
                Command::from_json(command).unwrap(),
            )
            .unwrap();
    }
    state.advance_tick().unwrap();
    command
}
fn snapshot(state: &mut Match) -> Vec<Value> {
    let mut delivery = DeliveryState::new(state).unwrap();
    let frame = delivery
        .capture_with_lists(state, &[true; 2], &[false; 2], &[])
        .unwrap();
    (0..2)
        .map(|slot| frame.recipient(slot).unwrap().into_json())
        .collect()
}

#[test]
fn if27_save_continuation_keeps_planner_memory_and_resets_commander_attention() {
    const N: u64 = 160;
    let mut original = ordinary();
    let enemy = original
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 1 && original.rules.kinds[u.kind].name == "rifle")
        .unwrap()
        .id;
    {
        let unit = original.world.game.units.get_mut(&enemy).unwrap();
        unit.x = 36.0;
        unit.z = 20.0;
        original.grid.update(enemy, unit);
    }
    let mut seat = original.create_seat_commander(0, Level::Hard).unwrap();
    let mut memory = HumanObservationMemory::new(0);
    let fresh = original.save_ai_memories_json(&[(&seat, &memory)]).unwrap();
    let mut attempts = 0;
    for _ in 0..N {
        attempts += usize::from(step(&mut original, &mut seat, &mut memory).is_some());
    }
    assert!(
        !serde_json::to_value(&memory).unwrap()["sightings"]
            .as_array()
            .unwrap()
            .is_empty(),
        "the checkpoint must carry actual remembered enemy contacts"
    );
    let ai = original.save_ai_memories_json(&[(&seat, &memory)]).unwrap();
    assert_ne!(
        ai, fresh,
        "real observations and planning must change memory"
    );
    let Brain::Human {
        state,
        policy,
        delivered,
    } = &mut seat.brain
    else {
        panic!()
    };
    assert!(state.snapshot().is_some());
    assert!(delivered.is_some());
    let expected_policy = policy.clone();
    // Source deserializes g and ai, but its separate commander WeakMap has no entry.
    state.reset();
    *delivered = None;
    let bytes = serde_json::to_vec(&serde_json::json!({
        "version": 1, "g": original.save_json().unwrap(), "ai": ai,
    }))
    .unwrap();
    let envelope: Value = serde_json::from_slice(&bytes).unwrap();
    let mut loaded = Match::from_save_json(envelope["g"].clone()).unwrap();
    let mut restored = loaded
        .restore_ai_memories_json(&envelope["ai"], &[(0, Level::Hard)])
        .unwrap();
    let (restored_seat, restored_memory) = &mut restored[0];
    let Brain::Human {
        state,
        policy,
        delivered,
    } = &restored_seat.brain
    else {
        panic!()
    };
    assert_eq!(policy, &expected_policy);
    assert!(
        state.snapshot().is_none(),
        "camera and input queue restart on load"
    );
    assert!(
        delivered.is_none(),
        "load must obtain a fresh delivered view"
    );
    assert_eq!(
        loaded
            .save_ai_memories_json(&[(restored_seat, restored_memory)])
            .unwrap(),
        ai
    );
    for _ in N..2 * N {
        let a = step(&mut original, &mut seat, &mut memory);
        let b = step(&mut loaded, restored_seat, restored_memory);
        attempts += usize::from(a.is_some());
        assert_eq!(a, b);
        assert_eq!(snapshot(&mut original), snapshot(&mut loaded));
        assert_eq!(
            original.save_ai_memories_json(&[(&seat, &memory)]).unwrap(),
            loaded
                .save_ai_memories_json(&[(restored_seat, restored_memory)])
                .unwrap()
        );
    }
    assert_eq!(original.save_json().unwrap(), loaded.save_json().unwrap());
    assert!(
        attempts > 0,
        "continuation must exercise actual delayed commands"
    );
}

#[test]
fn if27_v1_null_memory_and_human_resumption_reset_without_changing_game_schema() {
    let original = ordinary();
    let legacy = original.save_json().unwrap();
    let loaded = Match::from_save_json(legacy.clone()).unwrap();
    assert_eq!(loaded.save_json().unwrap(), legacy);
    let fresh = loaded
        .restore_ai_memories_json(&Value::Null, &[(0, Level::Normal)])
        .unwrap();
    assert_eq!(
        fresh[0].0,
        loaded.create_seat_commander(0, Level::Normal).unwrap()
    );
    assert_eq!(fresh[0].1, HumanObservationMemory::new(0));
    let ai = loaded
        .save_ai_memories_json(&[(&fresh[0].0, &fresh[0].1)])
        .unwrap();
    assert!(
        loaded
            .restore_ai_memories_json(&ai, &[])
            .unwrap()
            .is_empty(),
        "a resumed human must not recover the old AI memory"
    );
    assert!(
        loaded
            .restore_ai_memories_json(&ai, &[(0, Level::Hard), (0, Level::Hard)])
            .is_err()
    );
    assert!(
        loaded
            .restore_ai_memories_json(&ai, &[(2, Level::Hard)])
            .is_err()
    );
    assert!(
        loaded
            .restore_ai_memories_json(&serde_json::json!({}), &[])
            .is_err()
    );
    assert!(
        loaded
            .save_ai_memories_json(&[(&fresh[0].0, &HumanObservationMemory::new(1))])
            .is_err()
    );
    let mut other = ordinary();
    let mut foreign_memory = HumanObservationMemory::new(0);
    other
        .human_commander_observation(&mut foreign_memory)
        .unwrap();
    assert!(
        loaded
            .save_ai_memories_json(&[(&fresh[0].0, &foreign_memory)])
            .is_err(),
        "another allocation's remembered contacts must not be adopted"
    );
    other.options.seed += 1;
    assert!(
        other
            .restore_ai_memories_json(&ai, &[(0, Level::Normal)])
            .is_err()
    );
}

#[test]
fn if27_horde_pending_memory_round_trips_order_budget_and_supersession() {
    use crate::game::Point;
    use crate::horde_commander::{HordeCommander, Live, OwnPlan, OwnUnit, View, Wave};
    let plan = OwnPlan {
        delivered: None,
        path_generation: 2,
        goal: None,
        route_end: None,
        outcome: None,
        outcome_tick: None,
        path: vec![],
        amove: None,
        attack: 0,
        target: 0,
        retreating: false,
        orders: Default::default(),
        dig: None,
        entrench: false,
        build: None,
        enter: None,
        fire_at: None,
        nade: None,
        hold: false,
    };
    let mut view = View {
        tick: 20,
        wave: Wave {
            slot: 1,
            number: 2,
            active: true,
        },
        units: (1..=7)
            .map(|id| {
                (
                    id,
                    OwnUnit {
                        id,
                        owner: 1,
                        hp: 100.0,
                        air: false,
                        structure: false,
                        plan: plan.clone(),
                    },
                )
            })
            .collect(),
        bunker: Some(Point { x: 90.0, z: 90.0 }),
    };
    let mut live = Live {
        tick: 20,
        winner: false,
        wave: Some(view.wave),
    };
    let mut state = HordeCommander::new(1);
    let facts = crate::horde_policy::Facts {
        tick: view.tick,
        slot: 1,
        spawn: Point { x: 40.0, z: 40.0 },
        teams: vec![0, 1],
        manpower: 150.0,
        support: Default::default(),
        units: view
            .units
            .iter()
            .map(|(&id, own)| {
                (
                    id,
                    crate::horde_policy::Unit {
                        id,
                        owner: own.owner,
                        kind: "rifle".into(),
                        at: Point { x: 40.0, z: 40.0 },
                        hp: own.hp,
                        suppression: 0.0,
                        plane: None,
                        cooldown: 0.0,
                        auto_retreat: false,
                        garrisoned: false,
                        in_cover: false,
                        riding: false,
                    },
                )
            })
            .collect(),
        points: vec![],
        strikes: vec![],
    };
    state.observe_policy(live, view.clone(), facts).unwrap();
    let encoded = crate::save_codec::to_value(&state).unwrap();
    let decoded: HordeCommander = crate::save_codec::from_value(encoded).unwrap();
    assert_eq!(decoded, state);
    let mut legacy = serde_json::to_value(&state).unwrap();
    assert!(!legacy["policy"]["still"].as_object().unwrap().is_empty());
    assert!(legacy["policy_facts"].is_object());
    legacy.as_object_mut().unwrap().remove("policy");
    legacy.as_object_mut().unwrap().remove("policy_facts");
    let legacy: HordeCommander = serde_json::from_value(legacy).unwrap();
    let legacy = serde_json::to_value(legacy).unwrap();
    assert_eq!(
        legacy["policy"],
        serde_json::to_value(crate::horde_policy::HordePolicy::default()).unwrap()
    );
    assert!(legacy["policy_facts"].is_null());
    let command = Command::from_json(&serde_json::json!({
        "t":"move", "orders":(1..=7).map(|id| serde_json::json!([id, 90.0, 90.0])).collect::<Vec<_>>()
    })).unwrap();
    assert_eq!(state.submit(live, command).unwrap().len(), 4);
    assert_eq!(state.pending(), 3);
    let map = crate::map::AuthoritativeMap::from_json(
        &serde_json::to_vec(&serde_json::json!({
            "name":"Horde memory checkpoint", "w":80, "h":80,
            "rows":vec![".".repeat(80);80], "defend":[0],
            "spawns":[{"x":40,"y":5},{"x":10,"y":74},{"x":70,"y":74}],
            "points":[{"x":40,"y":5,"vp":1}]
        }))
        .unwrap(),
    )
    .unwrap();
    let mut options = ordinary().options;
    options.mode = crate::economy::Mode::Horde;
    options.army = crate::economy_state::ArmySize::Standard;
    options.players.truncate(1);
    let game = Match::new_horde(&map, options).unwrap();
    let mut seat = game.create_seat_commander(1, Level::Normal).unwrap();
    seat.brain = Brain::Horde(state.clone());
    let memory = HumanObservationMemory::new(1);
    let saved = game.save_ai_memories_json(&[(&seat, &memory)]).unwrap();
    let saved = serde_json::from_slice(&serde_json::to_vec(&saved).unwrap()).unwrap();
    let mut restored = game
        .restore_ai_memories_json(&saved, &[(1, Level::Hard)])
        .unwrap();
    let Brain::Horde(ref mut loaded) = restored[0].0.brain else {
        panic!()
    };
    let mut expected = state.clone();
    expected.reset_saved_delivery();
    assert_eq!(*loaded, expected);
    assert!(
        loaded.act(live).unwrap().is_empty(),
        "load waits for a fresh observation"
    );
    assert!(
        loaded.observe(live, view.clone()).unwrap().is_empty(),
        "load cannot replenish this tick's budget"
    );
    live.tick += 1;
    view.tick += 1;
    view.units.get_mut(&6).unwrap().plan.hold = true;
    let a = state.observe(live, view.clone()).unwrap();
    let b = loaded.observe(live, view).unwrap();
    assert_eq!(a, b);
    assert_eq!(
        a.len(),
        2,
        "superseded unit six must be discarded after load"
    );
    assert_eq!(loaded.pending(), 0);
}

#[test]
fn if27_checkpoint_continues_nonempty_path_fifo_retries_grid_and_navigation_caches() {
    use crate::{game::Point, map::AuthoritativeMap, path_requests::RequestKind};
    let rows = vec![format!("{}W{}", ".".repeat(32), ".".repeat(31)); 40];
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&serde_json::json!({
            "name":"Checkpoint disconnected banks", "w":64, "h":40, "rows":rows,
            "spawns":[{"x":8,"y":10},{"x":40,"y":32}],
            "points":[{"x":4,"y":36,"vp":0,"mp":0}]
        }))
        .unwrap(),
    )
    .unwrap();
    let mut original = Match::new(&map, ordinary().options).unwrap();
    for control in original.commands.units.values_mut() {
        control.hold_fire = true;
        control.auto_cast = false;
        control.auto_retreat = false;
    }
    let unit = original
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 0 && original.rules.kinds[u.kind].name == "rifle")
        .unwrap()
        .id;
    let goal = Point { x: 90.0, z: 20.0 };
    original
        .execute_command(
            0,
            &Command::from_json(&serde_json::json!({
                "t":"move", "orders":[[unit,goal.x,goal.z]]
            }))
            .unwrap(),
            false,
        )
        .unwrap();
    original.world.game.units.get_mut(&unit).unwrap().world_goal = Some(goal);
    original.begin_path_tick(1, 20).unwrap();
    original
        .request_command_path(unit, goal, RequestKind::World)
        .unwrap();
    original
        .request_command_path(unit, goal, RequestKind::World)
        .unwrap();
    assert_eq!(original.commands.path_requests().queue_len(), 1);
    assert_eq!(original.commands.path_requests().pending_len(), 1);
    fn field<'a>(map: &'a Value, name: &str) -> &'a Value {
        &map[1]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry[0] == serde_json::json!(["string", name]))
            .unwrap()[1]
    }
    let requests = crate::save_codec::to_value(&original.commands.requests).unwrap();
    assert!(
        !field(&requests, "failures")[1]
            .as_array()
            .unwrap()
            .is_empty(),
        "the disconnected native search must retain a nonzero retry count"
    );
    let saved = original.save_json().unwrap();
    let mut loaded = Match::from_save_json(
        serde_json::from_slice(&serde_json::to_vec(&saved).unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(loaded.commands.path_requests().queue_len(), 1);
    assert_eq!(
        crate::save_codec::to_value(&loaded.commands.requests).unwrap(),
        requests
    );
    for _ in 0..6 {
        original.advance_tick().unwrap();
        loaded.advance_tick().unwrap();
        assert_eq!(snapshot(&mut original), snapshot(&mut loaded));
        assert_eq!(original.save_json().unwrap(), loaded.save_json().unwrap());
    }
    assert_eq!(loaded.commands.path_requests().pending_len(), 0);
    assert_eq!(loaded.tick_state.fault(), None);
}

#[test]
fn if27_field_and_world_policy_memory_round_trip_and_legacy_defaults() {
    let game = ordinary();
    let mut seat = game.create_seat_commander(0, Level::Hard).unwrap();
    let Brain::Human { policy, .. } = &mut seat.brain else {
        panic!("ordinary seat must use the human brain");
    };
    let mut encoded = serde_json::to_value(&**policy).unwrap();
    encoded["tactics"]["field"] = serde_json::json!({
        "bridges":[12], "bridge_crew":{"4":{"tick":9,"dug":true}},
        "fill_crew":{"5":{"tick":10,"dug":false}},
        "demine_crew":{"6":{"tick":11,"dug":true}}, "armor_seen":true
    });
    encoded["tactics"]["world"] = serde_json::json!({
        "rail_at":{"7":23}, "fort_steps":{"8":3},
        "field": encoded["tactics"]["field"].clone()
    });
    encoded["tactics"]["engineering"] = serde_json::json!({
        "bridges":[17], "start_seen":true, "crew":{}
    });
    encoded["tactics"]["armor_ever_seen"] = serde_json::json!(true);
    **policy = serde_json::from_value(encoded.clone()).unwrap();
    let expected = policy.clone();
    let memory = HumanObservationMemory::new(0);
    let saved = game.save_ai_memories_json(&[(&seat, &memory)]).unwrap();
    let saved = serde_json::from_slice(&serde_json::to_vec(&saved).unwrap()).unwrap();
    let restored = game
        .restore_ai_memories_json(&saved, &[(0, Level::Hard)])
        .unwrap();
    let Brain::Human { policy, .. } = &restored[0].0.brain else {
        panic!("restored seat must use the human brain");
    };
    assert_eq!(policy, &expected);
    let tactics = encoded["tactics"].as_object_mut().unwrap();
    tactics.remove("field");
    tactics.remove("world");
    tactics.remove("engineering");
    tactics.remove("armor_ever_seen");
    let legacy: HumanPolicy = serde_json::from_value(encoded).unwrap();
    let legacy = serde_json::to_value(legacy).unwrap();
    assert_eq!(
        legacy["tactics"]["field"],
        serde_json::to_value(crate::commander_field_policy::State::default()).unwrap()
    );
    assert_eq!(
        legacy["tactics"]["world"],
        serde_json::to_value(crate::commander_world_policy::State::default()).unwrap()
    );
    assert_eq!(
        legacy["tactics"]["engineering"],
        serde_json::to_value(crate::commander_engineering::State::default()).unwrap()
    );
    assert_eq!(legacy["tactics"]["armor_ever_seen"], false);
}
