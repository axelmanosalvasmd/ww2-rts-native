//! Trusted Classic attachment exercises ordinary room delivery, not public lobby admission.
use super::*;
use serde_json::json;
use ww2_core::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    map::AuthoritativeMap,
    match_commands::Command,
    match_runtime::{MatchOptions, PlayerConfig},
    orders::OrderKey,
    weather::WeatherChoice,
};

fn scene_with_logistics(logistics: bool) -> Match {
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"Hosted Classic production","w":70,"h":70,
            "rows":vec![".".repeat(70);70],"heights":vec!["0".repeat(70);70],
            "spawns":[{"x":7,"y":7},{"x":62,"y":62}],
            "points":[{"x":35,"y":35,"mp":0,"vp":0}]
        }))
        .unwrap(),
    )
    .unwrap();
    let mut state = Match::new_classic(
        &map,
        MatchOptions {
            seed: 719,
            mode: Mode::Classic,
            players: (0..2)
                .map(|slot| PlayerConfig {
                    name: format!("P{slot}"),
                    team: slot,
                    faction: Faction::Usa,
                })
                .collect(),
            shuffle: false,
            army: ArmySize::Standard,
            weather: WeatherChoice::Map,
            weather_seed: Some(1),
            living_weather: false,
            map_key: None,
            tech: logistics,
            supply: logistics,
            logistics,
            sandbox: false,
        },
    )
    .unwrap();
    state.start.economy.players[0].mp = 5000.;
    state
}

#[test]
fn classic_paid_training_and_nodes_survive_every_seat_room_frames_through_completion() {
    let mut state = scene_with_logistics(false);
    let hq = state
        .world
        .game
        .units
        .values()
        .find(|unit| unit.owner == 0 && state.rules.kinds[unit.kind].name == "hq")
        .unwrap()
        .id;
    state
        .enqueue_command(
            OrderKey {
                tick: 1,
                issuer: 0,
                sequence: 1,
            },
            Command::Buy {
                kind: "rifle".into(),
                from: Some(hq),
            },
        )
        .unwrap();
    let initial = state
        .world
        .game
        .units
        .values()
        .filter(|unit| unit.owner == 0 && state.rules.kinds[unit.kind].name == "rifle")
        .count();
    let mut runtime = RoomRuntime::new(state, 0).unwrap();
    let selected: Vec<_> = (0..2)
        .map(|slot| SelectedSeat {
            slot,
            full: true,
            omit: false,
        })
        .collect();
    let mut nodes = None;
    let mut completed = false;
    for tick in 1..=304 {
        let report = runtime.advance_tick(&[false; 2]).unwrap();
        if tick == 1 {
            assert_eq!(report.commands[0].denial, None);
        }
        let frames = runtime
            .collect_frame(&selected, FrameKind::Scheduled)
            .unwrap();
        assert_eq!(frames.len(), 2);
        for frame in frames {
            assert_eq!(frame.message["t"], "s");
            assert_eq!(frame.message["mode"]["kind"], "classic");
            let expected = nodes.get_or_insert_with(|| frame.message["nodes"].clone());
            assert_eq!(&frame.message["nodes"], expected);
            assert!(!expected.as_array().unwrap().is_empty());
            if frame.slot == 1 {
                assert!(
                    frame.message["productionJobs"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .all(|row| row[0] != hq)
                );
            }
        }
        completed |= runtime
            .state()
            .world
            .game
            .units
            .values()
            .filter(|unit| {
                unit.owner == 0 && runtime.state().rules.kinds[unit.kind].name == "rifle"
            })
            .count()
            > initial;
        assert!(runtime.fault().is_none());
        assert!(runtime.state().tick_state.fault().is_none());
    }
    assert!(completed);
    assert!(
        runtime
            .state()
            .production
            .building(hq)
            .unwrap()
            .jobs()
            .is_empty()
    );
}

#[test]
fn classic_paid_depot_claim_cancel_and_destruction_keep_the_room_running() {
    let mut state = scene_with_logistics(false);
    let at = state.elimination.as_ref().unwrap().nodes[0].at;
    let engineer = state
        .world
        .game
        .units
        .values()
        .find(|unit| unit.owner == 0 && state.rules.kinds[unit.kind].name == "engineer")
        .unwrap()
        .id;
    let unit = state.world.game.units.get_mut(&engineer).unwrap();
    unit.x = at.x;
    unit.z = at.z - 6.;
    state.grid.update(engineer, unit);
    let mut runtime = RoomRuntime::new(state, 0).unwrap();
    let selected: Vec<_> = (0..2)
        .map(|slot| SelectedSeat {
            slot,
            full: true,
            omit: false,
        })
        .collect();
    let mut depot = 0;
    for tick in 1..=10 {
        let command = match tick {
            1 | 3 => {
                depot = runtime.state().next_unit_id;
                Some(Command::Build {
                    ids: vec![i64::from(engineer)],
                    kind: "depot".into(),
                    at: Some(at),
                    direction: None,
                    queue: false,
                })
            }
            2 => Some(Command::Cancel {
                id: i64::from(depot),
            }),
            4 => Some(Command::Destroy {
                ids: vec![i64::from(depot)],
                id: None,
            }),
            _ => None,
        };
        if let Some(command) = command {
            runtime
                .state_mut()
                .enqueue_command(
                    OrderKey {
                        tick,
                        issuer: 0,
                        sequence: tick,
                    },
                    command,
                )
                .unwrap();
        }
        let report = runtime.advance_tick(&[false; 2]).unwrap();
        assert!(
            report
                .commands
                .iter()
                .all(|command| command.denial.is_none())
        );
        let frames = runtime
            .collect_frame(&selected, FrameKind::Scheduled)
            .unwrap();
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].message["nodes"], frames[1].message["nodes"]);
        assert!(
            frames[0].message["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .all(|row| row.as_array().unwrap().len() == 4)
        );
        assert_eq!(
            runtime.state().elimination.as_ref().unwrap().nodes[0].depot,
            if tick == 1 || tick == 3 {
                Some(depot)
            } else {
                None
            }
        );
        assert!(runtime.fault().is_none());
        assert!(runtime.state().tick_state.fault().is_none());
    }
}

#[test]
fn base21_base59_logistics_training_publishes_reserves_private_jobs_and_terminal_room_frames() {
    let mut state = scene_with_logistics(true);
    state.start.economy.players[0].fuel = 5000.;
    state.start.economy.players[0].mun = 500.;
    let first_birth = state.next_unit_id;
    let hq = state
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 0 && state.rules.kinds[u.kind].name == "hq")
        .unwrap()
        .id;
    let producer = hq;
    assert!(state.convoys.is_some());
    // Sources follow the construction-mode HQ identity rather than a fallback spawn.
    assert!(
        state
            .convoys
            .as_ref()
            .unwrap()
            .stores
            .iter()
            .any(|s| s.source && s.entity == Some(hq))
    );
    for u in state.world.game.units.values_mut() {
        u.hold_pos = true;
    }
    for c in state.commands.units.values_mut() {
        c.hold_fire = true;
        c.auto_cast = false;
        c.auto_retreat = false;
    }
    state
        .building_rallies
        .insert(producer, Some(ww2_core::game::Point { x: 55.34, z: 45.36 }));
    state.player_rallies[0] = Some(ww2_core::game::Point { x: 50.36, z: 46.34 });
    state
        .request_research(0, ww2_core::economy::Technology::Tier)
        .unwrap();
    for (sequence, (from, kind)) in [(producer, "rifle"), (producer, "rifle")]
        .into_iter()
        .enumerate()
    {
        state
            .enqueue_command(
                OrderKey {
                    tick: 1,
                    issuer: 0,
                    sequence: sequence as u64,
                },
                Command::Buy {
                    kind: kind.into(),
                    from: Some(from),
                },
            )
            .unwrap();
    }
    let mut runtime = RoomRuntime::new(state, 0).unwrap();
    let selected: Vec<_> = (0..2)
        .map(|slot| SelectedSeat {
            slot,
            full: true,
            omit: false,
        })
        .collect();
    let mut births = std::collections::BTreeSet::new();
    let mut last_nodes = None;
    // Complete both FIFO jobs through the real clock and publish every seat on every tick.
    for tick in 1..=604 {
        let report = runtime.advance_tick(&[false; 2]).unwrap();
        if tick == 1 {
            assert_eq!(report.commands.len(), 2);
            assert!(report.commands.iter().all(|c| c.denial.is_none()));
        }
        let frames = runtime
            .collect_frame(&selected, FrameKind::Scheduled)
            .unwrap();
        assert_eq!(frames.len(), 2);
        for frame in frames {
            let value = frame.message;
            assert_eq!(value["logistics"]["enabled"], true);
            assert_eq!(value["mode"]["kind"], "classic");
            assert_eq!(
                value["nodes"],
                *last_nodes.get_or_insert_with(|| value["nodes"].clone())
            );
            let jobs = value["productionJobs"].as_array().unwrap();
            let queues = value["queues"].as_array().unwrap();
            let reserves = value["logistics"]["units"].as_array().unwrap();
            assert!(reserves.iter().all(|r| {
                runtime.state().world.game.units[&(r["id"].as_u64().unwrap() as u32)].owner
                    == frame.slot
            }));
            if frame.slot == 0 {
                assert_eq!(value["rally"], json!([50.4, 46.3]));
                if tick == 1 {
                    assert_eq!(value["tech"]["tier"], 1);
                    assert_eq!(value["tech"]["lab"][0], json!(["tier", 0.]));
                    let job = jobs.iter().find(|row| row[0] == producer).unwrap();
                    assert_eq!(job[1].as_array().unwrap().len(), 2);
                    assert_eq!(job[1][0]["status"], "active");
                    assert_eq!(job[1][1]["status"], "waiting");
                    assert_eq!(job[1][0]["mp"], 100.);
                    assert_eq!(job[1][0]["fuel"], 0.);
                    assert_eq!(
                        *queues.iter().find(|row| row[0] == producer).unwrap(),
                        json!([producer, 0., 55.3, 45.4, "rifle", "rifle"])
                    );
                }
                for r in reserves {
                    let id = r["id"].as_u64().unwrap() as u32;
                    let u = &runtime.state().world.game.units[&id];
                    if id >= first_birth && runtime.state().rules.kinds[u.kind].name == "rifle" {
                        births.insert(id);
                        assert!(r["ammo"].as_f64().unwrap() > 0.);
                        assert!(r["provisions"].as_f64().unwrap() > 0.);
                        assert!(r["fuel"].is_null());
                    }
                }
            } else {
                assert!(jobs.iter().all(|row| row[0] != producer && row[0] != hq));
                assert!(queues.iter().all(|row| row[0] != producer && row[0] != hq));
                assert_eq!(value["rally"], serde_json::Value::Null);
                assert_eq!(value["tech"]["lab"], json!([]));
            }
        }
        assert!(runtime.fault().is_none());
        assert!(runtime.state().tick_state.fault().is_none());
    }
    assert_eq!(births.len(), 2);
    assert!(
        runtime
            .state()
            .production
            .building(producer)
            .unwrap()
            .jobs()
            .is_empty()
    );
    assert!(
        runtime
            .state()
            .production
            .building(hq)
            .unwrap()
            .jobs()
            .is_empty()
    );
    for id in &births {
        let u = &runtime.state().world.game.units[id];
        assert_ne!(u.at(), runtime.state().world.game.units[&hq].at());
        assert!(runtime.state().logistics.get(*id).is_some());
    }
    // Eliminate the opposing producer, then continue the ordinary terminal ticks and delivery.
    let enemy_producers: Vec<_> = runtime
        .state()
        .world
        .game
        .units
        .values()
        .filter(|u| u.owner == 1 && runtime.state().rules.kinds[u.kind].building)
        .map(|u| u.id)
        .collect();
    assert!(!enemy_producers.is_empty());
    for id in enemy_producers {
        runtime
            .state_mut()
            .world
            .game
            .units
            .get_mut(&id)
            .unwrap()
            .hp = 0.;
    }
    for _ in 0..5 {
        runtime.advance_tick(&[false; 2]).unwrap();
        let frames = runtime
            .collect_frame(&selected, FrameKind::Scheduled)
            .unwrap();
        assert_eq!(frames.len(), 2);
        assert!(runtime.fault().is_none());
        assert!(runtime.state().tick_state.fault().is_none());
    }
    assert!(runtime.state().finish.is_some());
}
