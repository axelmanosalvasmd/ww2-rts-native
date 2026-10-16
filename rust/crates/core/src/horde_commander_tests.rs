use super::*;
use crate::{
    commander_ai::Level,
    economy::{Faction, Mode},
    economy_state::ArmySize,
    human_commander_timing::Timing,
    map::AuthoritativeMap,
    match_human_observation::{HumanObservationMemory, HumanShot},
    match_runtime::{MatchOptions, PlayerConfig},
    orders::OrderKey,
    recipient_snapshot::{DeliveryState, SelectedSeat, SnapshotCache, SnapshotError},
    seat_commander::{Advance, Brain, Delivered},
    unit_publication::StagedUnit,
    weather::WeatherChoice,
};
use serde_json::json;

fn scene(horde: bool) -> Match {
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"Horde commander intent", "w":80, "h":80,
            "rows":vec![".".repeat(80);80],
            "spawns":[{"x":40,"y":5},{"x":10,"y":74},{"x":70,"y":74}],
            "defend":[0], "points":[{"x":40,"y":5,"vp":1}]
        }))
        .unwrap(),
    )
    .unwrap();
    let options = MatchOptions {
        seed: 1701,
        mode: if horde { Mode::Horde } else { Mode::Conquest },
        players: (0..if horde { 1 } else { 2 })
            .map(|i| PlayerConfig {
                name: format!("P{i}"),
                team: i,
                faction: if i == 0 {
                    Faction::Usa
                } else {
                    Faction::Germany
                },
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
        logistics: false,
        sandbox: false,
    };
    if horde {
        Match::new_horde(&map, options).unwrap()
    } else {
        Match::new(&map, options).unwrap()
    }
}
fn add_rifles(m: &mut Match, owner: usize, count: usize) -> Vec<u32> {
    (0..count)
        .map(|i| {
            let id = m.next_unit_id;
            let draft = StagedUnit::ordinary(
                &m.world.game,
                &m.rules,
                owner,
                m.rules.kind("rifle").unwrap(),
                id,
                i as u32,
                m.start.players[owner].spawn,
                Some(Point {
                    x: 15.0 + (i % 25) as f64 * 3.0,
                    z: 50.0 + (i / 25) as f64 * 3.0,
                }),
            )
            .unwrap();
            m.publish_starting_unit(draft).unwrap();
            id
        })
        .collect()
}
fn movement(ids: &[u32], attack_move: bool, x: f64) -> Command {
    Command::Move {
        orders: ids
            .iter()
            .map(|id| MoveOrder {
                unit: i64::from(*id),
                at: Point { x, z: 90.0 },
            })
            .collect(),
        control_ids: None,
        attack_move,
        queue: false,
        together: false,
        face: None,
    }
}
fn ids(commands: &[Command]) -> Vec<u32> {
    commands
        .iter()
        .flat_map(|c| match c {
            Command::Move { orders, .. } => orders.iter().map(|o| o.unit as u32).collect(),
            _ => Vec::new(),
        })
        .collect()
}
fn wave_view(m: &Match, owner: usize, tick: i64) -> View {
    View {
        tick,
        wave: Wave {
            slot: owner,
            number: 1,
            active: true,
        },
        units: m
            .world
            .game
            .units
            .values()
            .filter(|u| u.owner == owner)
            .map(|u| (u.id, m.horde_own_unit(u.id, owner).unwrap()))
            .collect(),
        bunker: Some(Point { x: 90.0, z: 90.0 }),
    }
}
fn live(view: &View) -> Live {
    Live {
        tick: view.tick,
        winner: false,
        wave: Some(view.wave),
    }
}

#[test]
fn intent_ai10_live_owner_alerts_stop_seed_and_spawn_ignore_view_age() {
    let mut m = scene(false);
    let mut memory = HumanObservationMemory::new(0);
    let delivered = m.human_commander_observation(&mut memory).unwrap();
    let mut seat = m.create_seat_commander(0, Level::Normal).unwrap();
    let spawn = m.start.players[0].spawn;
    let victim = delivered
        .units
        .iter()
        .find(|u| u.owner == 0)
        .unwrap()
        .motion
        .id;
    let mut shots = vec![
        HumanShot::Other,
        HumanShot::Hurt {
            owner: 1,
            unit: victim,
            at: spawn,
        },
        HumanShot::Hurt {
            owner: 0,
            unit: victim,
            at: spawn,
        },
    ];
    let Advance::Human(first) = m
        .advance_seat_commander(
            &mut seat,
            Some(Delivered::Human(Box::new(delivered))),
            8,
            &shots,
        )
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(first.new_alerts.len(), 1);
    let Brain::Human { state, .. } = seat.brain() else {
        panic!()
    };
    assert_eq!(state.camera(), Some(spawn));
    let mut expected = Timing::new(m.options.seed as u32, 0);
    expected
        .opening_delay(crate::human_commander_timing::Skill::for_level("normal"))
        .unwrap();
    // Initialization and one noticed hurt each consume their own deterministic sample.
    expected
        .reaction_delay(
            crate::human_commander_timing::Skill::for_level("normal"),
            true,
        )
        .unwrap();
    assert_eq!(state.snapshot().unwrap().timing, expected);
    m.world.game.tick = 1;
    shots.push(HumanShot::Hurt {
        owner: 0,
        unit: victim,
        at: Point { x: 140.0, z: 130.0 },
    });
    let Advance::Human(next) = m
        .advance_seat_commander(&mut seat, None, 8, &shots)
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(next.new_alerts.len(), 1, "new alerts reach an unsent tick");
    assert_eq!(next.new_alerts[0].tick, 1);
    let frozen = seat.clone();
    m.start.economy.players[0].out = true;
    assert_eq!(
        m.advance_seat_commander(&mut seat, None, 9, &shots)
            .unwrap(),
        Advance::Human(Default::default())
    );
    assert_eq!(seat, frozen);
    m.start.economy.players[0].out = false;
    m.finish = Some(crate::match_end::MatchFinish {
        winner: 0,
        reason: crate::match_end::EndReason::VictoryPoints,
        end_at: spawn,
        end_tick: 1,
    });
    assert_eq!(
        m.advance_seat_commander(&mut seat, None, 9, &shots)
            .unwrap(),
        Advance::Human(Default::default())
    );
    assert_eq!(seat, frozen);
}

#[test]
fn intent_ai16_real_dispatch_uses_human_seats_and_scripted_horde() {
    let ordinary = scene(false);
    for owner in 0..2 {
        assert!(matches!(
            ordinary
                .create_seat_commander(owner, Level::Hard)
                .unwrap()
                .brain(),
            Brain::Human { .. }
        ));
    }
    let mut m = scene(true);
    m.step_horde(45.0).unwrap();
    let slot = m.horde.as_ref().unwrap().slot;
    let bunker = m.horde.as_ref().unwrap().bunker;
    m.world.game.players[slot].visible.push(bunker);
    add_rifles(&mut m, slot, 12);
    let mut defender = m.create_seat_commander(0, Level::Easy).unwrap();
    assert!(matches!(
        m.advance_seat_commander(&mut defender, None, 1, &[])
            .unwrap(),
        Advance::Human(_)
    ));
    let Brain::Human { state, .. } = defender.brain() else {
        panic!()
    };
    assert!(
        state.snapshot().is_some(),
        "dispatch actually initializes the human runtime"
    );
    let mut director = m.create_seat_commander(slot, Level::Hard).unwrap();
    assert!(matches!(director.brain(), Brain::Horde(_)));
    m.world.game.tick = 26;
    let view = m.horde_commander_observation(slot).unwrap();
    assert_eq!(
        m.advance_seat_commander(&mut director, Some(Delivered::Horde(view)), 1, &[])
            .unwrap(),
        Advance::Horde(vec![])
    );
    m.world.game.tick = 27;
    let Advance::Horde(commands) = m
        .advance_seat_commander(&mut director, None, 1, &[])
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(
        commands.len(),
        ORDER_BATCH,
        "scripted dispatch uses Normal's 40-tick beat"
    );
    let Brain::Horde(state) = director.brain() else {
        panic!()
    };
    assert_eq!(
        state.delivered().unwrap().tick,
        26,
        "unsent tick retains delivered packet"
    );
    assert!(state.pending() > 0);
    assert_eq!(
        m.validate_tick_state(),
        Err("command tick is not the next queued tick".into())
    );
    let cache = SnapshotCache::new(&m).unwrap();
    for recipient in 0..m.world.game.players.len() {
        cache.recipient(recipient).unwrap();
    }
    drop(cache);
    assert_eq!(
        m.restrict_to_ordinary_snapshots(),
        Err(SnapshotError::Unsupported {
            unit: 0,
            capability: crate::recipient_snapshot::SnapshotCapability::Air,
        })
    );
    assert!(m.opening_commander_observation(slot).is_err());
    for mutation in 0..3 {
        let mut stale = director.clone();
        m.horde.as_mut().unwrap().active = true;
        m.horde.as_mut().unwrap().wave = 1;
        m.finish = None;
        m.world.game.tick = 67;
        match mutation {
            0 => m.horde.as_mut().unwrap().active = false,
            1 => m.horde.as_mut().unwrap().wave = 2,
            _ => {
                m.finish = Some(crate::match_end::MatchFinish {
                    winner: 0,
                    reason: crate::match_end::EndReason::Structures,
                    end_at: m.start.players[0].spawn,
                    end_tick: 67,
                })
            }
        }
        assert_eq!(
            m.advance_seat_commander(&mut stale, None, 2, &[]).unwrap(),
            Advance::Horde(vec![])
        );
        let Brain::Horde(state) = stale.brain() else {
            panic!()
        };
        assert_eq!(
            state.pending(),
            0,
            "dispatcher reads live Match state before a stale-view think"
        );
    }
}

#[test]
fn intent_ai17_normal_stagger_observes_every_sent_beat_and_keeps_unsent_view() {
    for slot in 0..6 {
        let state = HordeCommander::new(slot);
        for tick in 0..160 {
            assert_eq!(state.due(tick), (tick + slot as i64 * 13) % 40 == 0);
            assert_eq!(
                state.planner_wants_view(tick),
                (40 - (tick + slot as i64 * 13) % 40) % 40 < 4
            );
            assert!(state.wants_view(true));
            assert!(!state.wants_view(false));
        }
    }
    let mut m = scene(true);
    m.step_horde(45.0).unwrap();
    let slot = m.horde.as_ref().unwrap().slot;
    let mut seat = m.create_seat_commander(slot, Level::Easy).unwrap();
    for tick in 1..81 {
        m.world.game.tick = tick;
        let sent = tick % 2 == 0;
        assert_eq!(seat.wants_view(tick, sent), sent);
        let view = sent.then(|| Delivered::Horde(m.horde_commander_observation(slot).unwrap()));
        m.advance_seat_commander(&mut seat, view, 0, &[]).unwrap();
        let Brain::Horde(state) = seat.brain() else {
            panic!()
        };
        assert_eq!(
            state.delivered().map(|v| v.tick),
            (tick >= 2).then_some(tick - tick % 2)
        );
    }
}

#[test]
fn intent_ai10_stale_wave_break_replacement_and_winner_stop_before_planning() {
    let mut m = scene(true);
    m.step_horde(45.0).unwrap();
    let slot = m.horde.as_ref().unwrap().slot;
    let all = add_rifles(&mut m, slot, 10);
    let mut state = HordeCommander::new(slot);
    let view = m.horde_commander_observation(slot).unwrap();
    state.observe(m.horde_commander_live(), view).unwrap();
    assert_eq!(
        state
            .submit(m.horde_commander_live(), movement(&all, true, 90.0))
            .unwrap()
            .len(),
        4
    );
    assert_eq!(state.pending(), 6);
    for mutation in 0..3 {
        let mut stale = state.clone();
        m.world.game.tick = 27;
        let mut control = m.horde_commander_live();
        match mutation {
            0 => control.wave.as_mut().unwrap().active = false,
            1 => control.wave.as_mut().unwrap().number += 1,
            _ => control.winner = true,
        }
        assert!(stale.act(control).unwrap().is_empty());
        assert_eq!(stale.pending(), 0);
        assert!(
            stale
                .submit(control, movement(&all, true, 90.0))
                .unwrap()
                .is_empty()
        );
    }
}

#[test]
fn intent_ai18_fifo_capacity_replacement_and_one_budget_per_observed_tick() {
    let mut m = scene(false);
    let all = add_rifles(&mut m, 0, 250);
    let view = wave_view(&m, 0, 0);
    let mut state = HordeCommander::new(0);
    state.observe(live(&view), view.clone()).unwrap();
    assert_eq!(
        ids(&state
            .submit(live(&view), movement(&all, true, 90.0))
            .unwrap()),
        all[..4]
    );
    assert_eq!(state.pending(), 236);
    assert!(
        state
            .submit(live(&view), movement(&all, true, 90.0))
            .unwrap()
            .is_empty()
    );
    assert_eq!(state.pending(), 240);
    assert!(state.observe(live(&view), view.clone()).unwrap().is_empty());
    assert!(
        state
            .submit(live(&view), movement(&[all[6]], false, 100.0))
            .unwrap()
            .is_empty()
    );
    let mut next = view.clone();
    next.tick = 2;
    let emitted = state.observe(live(&next), next).unwrap();
    assert_eq!(ids(&emitted), all[4..8]);
    assert!(
        matches!(&emitted[2], Command::Move { orders, attack_move: false, .. } if orders[0].at.x == 100.0)
    );
    // New ids were dropped at capacity; replacement did not move id 6 to the tail.
    assert!(state.pending.iter().all(|p| !all[240..].contains(&p.id)));
}

#[test]
fn intent_ai18_observe_discards_dead_foreign_air_retreat_and_complete_changed_intent() {
    let mut m = scene(false);
    let all = add_rifles(&mut m, 0, 20);
    let mut view = wave_view(&m, 0, 0);
    let mut state = HordeCommander::new(0);
    state.observe(live(&view), view.clone()).unwrap();
    state
        .submit(live(&view), movement(&all, true, 90.0))
        .unwrap();
    view.tick = 2;
    view.units.remove(&all[4]);
    view.units.get_mut(&all[5]).unwrap().hp = 0.0;
    view.units.get_mut(&all[6]).unwrap().owner = 1;
    view.units.get_mut(&all[7]).unwrap().air = true;
    view.units.get_mut(&all[8]).unwrap().plan.retreating = true;
    // Actual ordinary admission changes route identity even for same-cell Move and idle Stop.
    let same = all[9];
    let stop = all[10];
    let at = m.world.game.units[&same].at();
    m.execute_command(
        0,
        &Command::Move {
            orders: vec![MoveOrder {
                unit: i64::from(same),
                at,
            }],
            control_ids: None,
            attack_move: false,
            queue: false,
            together: false,
            face: None,
        },
        false,
    )
    .unwrap();
    m.execute_command(
        0,
        &Command::Stop {
            ids: vec![i64::from(stop)],
        },
        false,
    )
    .unwrap();
    for id in [same, stop] {
        view.units.insert(id, m.horde_own_unit(id, 0).unwrap());
    }
    view.units.get_mut(&all[11]).unwrap().plan.hold = true;
    view.units
        .get_mut(&all[12])
        .unwrap()
        .plan
        .orders
        .push_back(movement(&[all[12]], false, 100.0));
    // Health and position do not invalidate an otherwise unchanged live plan.
    view.units.get_mut(&all[13]).unwrap().hp -= 1.0;
    let commands = state.observe(live(&view), view).unwrap();
    assert_eq!(ids(&commands), all[13..17]);
    assert!(state.pending.iter().all(|p| !all[4..13].contains(&p.id)));
}

#[test]
fn intent_ai10_horde_observation_ignores_hidden_enemy_state_and_private_rng() {
    let mut m = scene(true);
    m.step_horde(45.0).unwrap();
    let slot = m.horde.as_ref().unwrap().slot;
    let bunker = m.horde.as_ref().unwrap().bunker;
    let random = m.start.random.snapshot();
    let first = m.horde_commander_observation(slot).unwrap();
    assert_eq!(first.bunker, None);
    let own = *first.units.keys().next().unwrap();
    m.firing.units.get_mut(&own).unwrap().target_id = bunker;
    m.commands.units.get_mut(&own).unwrap().attack_id = bunker;
    m.world.game.units.get_mut(&bunker).unwrap().x += 20.0;
    m.start.economy.players[0].mp = 99999.0;
    assert_eq!(m.horde_commander_observation(slot).unwrap(), first);
    m.world.game.players[slot].visible.push(bunker);
    assert!(
        m.horde_commander_observation(slot)
            .unwrap()
            .bunker
            .is_some()
    );
    assert_eq!(m.start.random.snapshot(), random);
    assert!(m.horde_commander_observation(0).is_err());
}

#[test]
fn new_private_queue_survives_full_ordinary_match_and_final_recipient_snapshots() {
    let mut m = scene(false);
    let all = add_rifles(&mut m, 0, 20);
    m.start.win_vp = 6.0;
    m.start.points[0].owner = Some(0);
    let mut delivery = DeliveryState::new(&m).unwrap();
    let mut state = HordeCommander::new(0);
    let mut human = m.create_seat_commander(1, Level::Hard).unwrap();
    let mut human_memory = HumanObservationMemory::new(1);
    let mut human_planned = false;
    let mut sequence = 0;
    let mut received = Vec::new();
    let mut finish = None;
    let mut had_pending = false;
    for _ in 0..600 {
        let tick = m.world.game.tick;
        let human_view = human.wants_view(tick, tick % 2 == 0).then(|| {
            Delivered::Human(Box::new(
                m.human_commander_observation(&mut human_memory).unwrap(),
            ))
        });
        let Advance::Human(result) = m
            .advance_seat_commander(&mut human, human_view, tick as u64, &[])
            .unwrap()
        else {
            panic!()
        };
        human_planned |= result.input.as_ref().is_some_and(|i| i.command.is_some());
        let view = wave_view(&m, 0, tick);
        let mut control = live(&view);
        control.winner = m.finish.is_some();
        let mut emitted = if tick % 2 == 0 {
            state.observe(control, view).unwrap()
        } else {
            vec![]
        };
        if tick == 0 {
            emitted.extend(state.submit(control, movement(&all, true, 100.0)).unwrap());
            had_pending = state.pending() > 0;
        }
        emitted.extend(state.act(control).unwrap());
        received.extend(ids(&emitted));
        for command in emitted {
            m.enqueue_command(
                OrderKey {
                    tick: tick as u64 + 1,
                    issuer: 0,
                    sequence,
                },
                command,
            )
            .unwrap();
            sequence += 1;
        }
        // Private queued work never becomes an unsupported UnitControl.orders kind.
        assert!(m.commands.units.values().all(|u| u.orders.is_empty()));
        let report = m.advance_tick().unwrap();
        assert!(report.commands.iter().all(|c| c.denial.is_none()));
        assert_eq!(m.tick_state.fault(), None);
        let selected = [0, 1].map(|slot| SelectedSeat {
            slot,
            full: false,
            omit: false,
        });
        let frame = delivery
            .capture_selected_with_tick_events(&mut m, &selected, &report.events)
            .unwrap()
            .unwrap();
        for slot in 0..2 {
            let snapshot = frame.recipient(slot).unwrap().into_json();
            assert_eq!(snapshot["t"], "s");
        }
        if m.finish.is_some() {
            let ended = *finish.get_or_insert(m.world.game.tick);
            if m.world.game.tick >= ended + 8 {
                break;
            }
        }
    }
    assert!(had_pending);
    assert!(
        human_planned,
        "ordinary brain dispatch reaches the human strategic policy and timed input"
    );
    assert!(all.iter().all(|id| received.contains(id)));
    assert!(
        m.finish.is_some(),
        "ordinary point economy must complete the match"
    );
    assert!(m.world.game.tick >= finish.unwrap() + 8);
    assert_eq!(state.pending(), 0);
    assert_eq!(m.tick_state.fault(), None);
}
