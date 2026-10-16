use serde_json::json;
use ww2_core::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    map::AuthoritativeMap,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    recipient_snapshot::SelectedSeat,
    weather::WeatherChoice,
};
use ww2_server::{
    room_runtime::{FrameKind, RoomRuntime},
    tick_timing::TickSample,
};

fn room() -> RoomRuntime {
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"AI snapshot cadence", "w":80, "h":80,
            "rows":vec![".".repeat(80);80],
            "spawns":[{"x":5,"y":5},{"x":74,"y":74}],
            "points":[{"id":"front","x":40,"y":40}]
        }))
        .unwrap(),
    )
    .unwrap();
    let state = Match::new(
        &map,
        MatchOptions {
            seed: 811,
            mode: Mode::Conquest,
            players: vec![
                PlayerConfig {
                    name: "A".into(),
                    team: 0,
                    faction: Faction::Usa,
                },
                PlayerConfig {
                    name: "B".into(),
                    team: 1,
                    faction: Faction::Germany,
                },
            ],
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
    RoomRuntime::new(state, 0).unwrap()
}

fn sample(room: &mut RoomRuntime, now: u64, ms: f64, sent: bool) -> u8 {
    room.record_timing(
        TickSample {
            durations: [ms, 0., 0., 0., 0., 0.],
            sent,
        },
        now,
    )
    .unwrap()
}

fn fill(room: &mut RoomRuntime, now: u64, ms: f64, every: u8) {
    for _ in 0..50 {
        assert_eq!(sample(room, now, ms, true), every);
    }
}

fn delivered_ticks(room: &mut RoomRuntime) -> Vec<i64> {
    let mut ticks = Vec::new();
    for _ in 0..12 {
        let report = room.advance_tick(&[false, false]).unwrap();
        assert_eq!(report.tick, room.state().world.game.tick);
        if room.should_snapshot(report.tick) {
            let frames = room
                .collect_frame(
                    &[SelectedSeat {
                        slot: 0,
                        full: false,
                        omit: false,
                    }],
                    FrameKind::Scheduled,
                )
                .unwrap();
            assert_eq!(frames.len(), 1);
            assert_eq!(frames[0].message["tick"], report.tick);
            ticks.push(report.tick);
        }
    }
    ticks
}

// AI-15: shared server snapshot cadence, not the separate AI observe/act loop.
// Prototype: server/room-tick.js:8-21,45-46 and tickmeter.js:12-41 at e4e579a.
#[test]
fn intent_ai15_room_snapshot_cadence_adapts_and_winner_overrides_the_beat() {
    let mut room = room();
    assert_eq!(delivered_ticks(&mut room), [2, 4, 6, 8, 10, 12]);
    for _ in 0..9 {
        assert_eq!(sample(&mut room, 1000, 40., true), 2);
    }
    assert_eq!(sample(&mut room, 1000, 90., false), 2);
    assert_eq!(sample(&mut room, 1000, 40., true), 2);
    assert_eq!(sample(&mut room, 1999, 60., true), 2);
    assert_eq!(sample(&mut room, 2000, 60., true), 3);
    assert_eq!(delivered_ticks(&mut room), [15, 18, 21, 24]);
    fill(&mut room, 2001, 60., 3);
    assert_eq!(sample(&mut room, 3000, 60., true), 4);
    assert_eq!(sample(&mut room, 4000, 60., true), 4);
    assert_eq!(delivered_ticks(&mut room), [28, 32, 36]);

    // A single low tick does not replace the snapshot p95 history.
    assert_eq!(sample(&mut room, 5000, 0., true), 4);
    fill(&mut room, 5001, 24., 4);
    assert_eq!(sample(&mut room, 6000, 24., true), 4);
    assert_eq!(sample(&mut room, 16000, 24., true), 4);
    fill(&mut room, 16001, 23., 4);
    assert_eq!(sample(&mut room, 17000, 23., true), 4);
    fill(&mut room, 17001, 24., 4);
    assert_eq!(sample(&mut room, 18000, 24., true), 4);
    fill(&mut room, 18001, 23., 4);
    assert_eq!(sample(&mut room, 19000, 23., true), 4);
    assert_eq!(sample(&mut room, 28999, 23., true), 4);
    assert_eq!(sample(&mut room, 29999, 23., true), 3);
    assert_eq!(delivered_ticks(&mut room), [39, 42, 45, 48]);
    assert_eq!(sample(&mut room, 39999, 23., true), 2);
    assert_eq!(sample(&mut room, 49999, 23., true), 2);
    assert_eq!(delivered_ticks(&mut room), [50, 52, 54, 56, 58, 60]);

    fill(&mut room, 50000, 60., 2);
    assert_eq!(sample(&mut room, 50999, 60., true), 3);
    assert_eq!(sample(&mut room, 51999, 60., true), 4);
    assert!(!room.should_snapshot(61));
    let state = room.state_mut();
    state.start.economy.players[0].vp = state.start.win_vp;
    assert!(state.apply_conquest_victory().unwrap().is_some());
    for tick in 61..=65 {
        assert!(
            room.should_snapshot(tick),
            "winner must override tick {tick}"
        );
    }
    // Winner delivery does not rewrite the room's adaptive interval.
    assert_eq!(sample(&mut room, 52000, 60., false), 4);
}
