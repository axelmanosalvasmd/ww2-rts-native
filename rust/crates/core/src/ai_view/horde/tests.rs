use super::*;

fn snapshot() -> Value {
    json!({"tick":20,"mode":{"kind":"horde","slot":1,"wave":3,"active":true},
        "units":[
            [1,"rifle",1,12.1,20.3,0,0,73,12,2,2,0.7,8192,1,1],
            [2,"bunker",0,50.2,60.4,0,0,500,0,0,0,99,8192,0,1],
            [3,"fighter",0,30,40,0,0,100,0,0,0,77,512,0,1]],
        "points":[[0,-1,0,0,0,0]],"out":[false,false],"vp":[0,0],
        "mp":321.5,"mun":0,"fuel":0,"sup":{"recon":1.2,"smoke":0},
        "plans":[[1,7,18.1,19.2,14.3,15.4],[2,4,99,99]],
        "orders":[[1,2,4,30.1,31.2,7,40.3,41.4],[2,1,1,99,99]],
        "movement":[[1,"completed",19]],"strikes":[["smoke",17.1,18.2,0,1.3,0]],
        "cells":[],"foundMines":[],"ghosts":[]})
}

fn memory() -> SeatMemory {
    SeatMemory::new(
        1,
        PublicMap {
            width: 40,
            height: 40,
            players: vec![
                json!({"name":"Defender","team":0,"faction":0,"spawn":{"x":10,"z":10}}),
                json!({"name":"Horde","team":1,"faction":2,"spawn":{"x":70.1,"z":70.2}}),
            ],
            points: vec![json!({"x":30.1,"z":40.2,"vp":1,"mp":0})],
        },
    )
    .unwrap()
}

#[test]
fn delivered_horde_packet_preserves_typed_facts_and_exact_owner_plan_tuples() {
    let mut snap = snapshot();
    snap["units"][0][12] = json!(8192 | 16 | 262144);
    let seat = memory().decode(&snap).unwrap();
    let (view, facts) = seat.horde_packet(1).unwrap();
    assert_eq!(facts.spawn, Point { x: 70.1, z: 70.2 });
    assert_eq!(facts.manpower, 321.5);
    assert_eq!(facts.support["recon"], 1.2);
    assert_eq!(facts.units[&1].cooldown, 0.7);
    assert!(facts.units[&1].auto_retreat && facts.units[&1].in_cover && facts.units[&1].riding);
    assert_eq!(facts.units[&2].cooldown, 0.0);
    assert!(!facts.units[&2].auto_retreat);
    assert_eq!(facts.units[&3].plane, Some(PlaneState::Base));
    assert_eq!(facts.units[&3].cooldown, 0.0);
    assert_eq!(
        facts.points,
        vec![PointFact {
            at: Point { x: 30.1, z: 40.2 },
            owner: Some(0),
            visible: true
        }]
    );
    assert_eq!(
        facts.strikes,
        vec![Strike {
            at: Point { x: 17.1, z: 18.2 },
            owner: 0,
            kind: "smoke".into(),
            delay: 1.3
        }]
    );
    assert_eq!(view.bunker, Some(Point { x: 50.2, z: 60.4 }));
    assert_eq!(view.units.keys().copied().collect::<Vec<_>>(), vec![1]);
    let intent = &view.units[&1].plan;
    assert_eq!(intent.path, vec![Point { x: 14.3, z: 15.4 }]);
    assert_eq!(intent.outcome, Some(MoveOutcome::Arrived));
    assert_eq!(intent.outcome_tick, Some(19));
    assert!(intent.is_digging());
    assert_eq!(
        intent.delivered,
        Some(DeliveredIntent {
            plan: Some(DeliveredPlan {
                kind: 7,
                at: Point { x: 18.1, z: 19.2 }
            }),
            orders: vec![
                DeliveredPlan {
                    kind: 4,
                    at: Point { x: 30.1, z: 31.2 }
                },
                DeliveredPlan {
                    kind: 7,
                    at: Point { x: 40.3, z: 41.4 }
                },
            ],
            digging: true,
        })
    );
    let mut busy = view.clone();
    busy.units.get_mut(&1).unwrap().plan.path.clear();
    busy.units.get_mut(&1).unwrap().plan.target = 0;
    let mut walking = facts.clone();
    walking.units.get_mut(&1).unwrap().riding = false;
    let mut policy = crate::horde_policy::HordePolicy::default();
    policy.observe(&busy, &walking).unwrap();
    assert!(
        policy
            .plan(&busy, &walking)
            .unwrap()
            .iter()
            .all(|command| { !matches!(command, crate::match_commands::Command::Move { .. }) }),
        "a delivered Dig remains busy without inventing a paid job"
    );
    let mut commander = crate::horde_commander::HordeCommander::new(1);
    let live = crate::horde_commander::Live {
        tick: view.tick,
        winner: false,
        wave: Some(view.wave),
    };
    commander.observe(live, view.clone()).unwrap();
    let command = crate::match_commands::Command::Move {
        orders: vec![crate::match_commands::MoveOrder {
            unit: 1,
            at: facts.spawn,
        }],
        control_ids: None,
        attack_move: true,
        queue: false,
        together: false,
        face: None,
    };
    for _ in 0..4 {
        assert_eq!(commander.submit(live, command.clone()).unwrap().len(), 1);
    }
    assert!(commander.submit(live, command).unwrap().is_empty());
    assert_eq!(commander.pending(), 1);
    // A changed display target matters even when no authoritative actor id is delivered.
    snap["plans"][0][2] = json!(18.2);
    let changed = memory().decode(&snap).unwrap().horde_packet(1).unwrap();
    assert_ne!(view.units[&1].plan, changed.0.units[&1].plan);
    assert_eq!(facts, changed.1);
    assert!(commander.observe(live, changed.0).unwrap().is_empty());
    assert_eq!(
        commander.pending(),
        0,
        "changed delivered intent drops pending movement"
    );
    assert!(seat.horde_packet(0).is_err());
    let mut wrong = seat.clone();
    wrong.mode["kind"] = json!("world");
    assert!(wrong.horde_packet(1).is_err());
}

fn scene() -> crate::match_runtime::Match {
    use crate::{
        economy::{Faction, Mode},
        economy_state::ArmySize,
        map::AuthoritativeMap,
        match_runtime::{Match, MatchOptions, PlayerConfig},
        weather::WeatherChoice,
    };
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"Delivered Horde policy", "w":60,"h":60,"rows":vec![".".repeat(60);60],
            "spawns":[{"x":30,"y":5},{"x":10,"y":54},{"x":50,"y":54}],
            "defend":[0],"points":[{"x":30,"y":30}]
        }))
        .unwrap(),
    )
    .unwrap();
    Match::new_horde(
        &map,
        MatchOptions {
            seed: 701,
            mode: Mode::Horde,
            players: vec![PlayerConfig {
                name: "Defender".into(),
                team: 0,
                faction: Faction::Usa,
            }],
            shuffle: false,
            army: ArmySize::Standard,
            weather: WeatherChoice::Map,
            weather_seed: Some(1),
            living_weather: false,
            map_key: None,
            tech: true,
            supply: false,
            logistics: false,
            sandbox: false,
        },
    )
    .unwrap()
}

#[test]
fn real_horde_ticks_deliver_policy_packets_without_hidden_enemy_inputs() {
    use crate::{horde_policy::HordePolicy, recipient_snapshot::DeliveryState};
    let mut state = scene();
    let slot = state.horde.as_ref().unwrap().slot;
    let mut memory = state.ordinary_ai_view_memory(slot).unwrap();
    let mut defender = state.ordinary_ai_view_memory(0).unwrap();
    let mut delivery = DeliveryState::new(&state).unwrap();
    state.end_horde_break().unwrap();
    for _ in 0..6 {
        state.advance_tick().unwrap();
        let seat = state
            .ordinary_ai_view(&mut memory, &mut delivery, true)
            .unwrap();
        let (view, facts) = seat.horde_packet(slot).unwrap();
        assert_eq!(view.wave.number, 1);
        assert_eq!(view.tick, state.world.game.tick);
        assert!(!view.units.is_empty());
        let mut policy = HordePolicy::default();
        policy.observe(&view, &facts).unwrap();
        policy.plan(&view, &facts).unwrap();
        let human = state
            .ordinary_ai_view(&mut defender, &mut delivery, true)
            .unwrap();
        assert!(
            human.horde_packet(slot).is_err(),
            "foreign seat has no scripted wallet"
        );
    }
    let hidden = state
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 0)
        .unwrap()
        .id;
    state.world.game.players[slot]
        .visible
        .retain(|id| *id != hidden);
    let before = state
        .ordinary_ai_view(&mut memory, &mut delivery, true)
        .unwrap()
        .horde_packet(slot)
        .unwrap();
    assert!(!before.1.units.contains_key(&hidden));
    let rng = state.start.random.snapshot();
    let actor = state.world.game.units.get_mut(&hidden).unwrap();
    actor.hp *= 0.5;
    actor.supp = 83.0;
    state.abilities.get_mut(hidden).unwrap().cooldown = 75.0;
    let after = state
        .ordinary_ai_view(&mut memory, &mut delivery, true)
        .unwrap()
        .horde_packet(slot)
        .unwrap();
    assert_eq!(
        before, after,
        "unseen authoritative changes never enter the packet"
    );
    assert_eq!(rng, state.start.random.snapshot());
    assert!(state.tick_state.fault().is_none());
}
