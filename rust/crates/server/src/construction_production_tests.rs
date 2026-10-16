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

fn scene() -> Match {
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
            tech: false,
            supply: false,
            logistics: false,
            sandbox: false,
        },
    )
    .unwrap();
    state.start.economy.players[0].mp = 5000.;
    state
}

#[test]
fn classic_paid_training_and_nodes_survive_every_seat_room_frames_through_completion() {
    let mut state = scene();
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
    let mut state = scene();
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
