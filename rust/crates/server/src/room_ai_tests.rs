//! Hosted brains use the recipient boundary and the room's delivered snapshot beat.
use super::*;
use crate::{
    room_runtime::{FrameKind, RoomRuntime},
    tick_timing::TickSample,
};
use serde_json::json;
use ww2_core::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::Point,
    map::AuthoritativeMap,
    match_runtime::{MatchOptions, PlayerConfig},
    recipient_snapshot::SelectedSeat,
    seat_commander::Brain as CommanderBrain,
    unit_publication::{PublicationParts, StagedUnit, prepare_publication},
    weather::WeatherChoice,
};

fn horde_scene(reveal: bool) -> Match {
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"Hosted delivered Horde", "w":80, "h":80,
            "rows":vec![".".repeat(80);80],
            "spawns":[{"x":40,"y":5},{"x":10,"y":74},{"x":70,"y":74}],
            "defend":[0], "points":[{"x":40,"y":30}]
        }))
        .unwrap(),
    )
    .unwrap();
    let mut state = Match::new_horde(
        &map,
        MatchOptions {
            seed: 1701,
            mode: Mode::Horde,
            players: vec![PlayerConfig {
                name: "Defender AI".into(),
                team: 4,
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
    .unwrap();
    state.world.game.reveal = reveal;
    state
}

fn rifle(state: &mut Match, owner: usize, at: Point) -> u32 {
    let id = state.next_unit_id;
    let draft = StagedUnit::ordinary(
        &state.world.game,
        &state.rules,
        owner,
        state.rules.kind("rifle").unwrap(),
        id,
        0,
        state.start.players[owner].spawn,
        Some(at),
    )
    .unwrap();
    prepare_publication(
        PublicationParts {
            units: &mut state.world.game.units,
            grid: &mut state.grid,
            controls: &mut state.commands.units,
            recruitment: &mut state.recruit_metadata,
            next_id: &mut state.next_unit_id,
        },
        &state.rules,
        state.start.players.len(),
        vec![draft],
    )
    .unwrap()
    .publish_with(|unit, _| {
        state.combat.initialize(unit.id);
        state.firing.initialize(unit.id);
        state.abilities.initialize(unit.id);
        state.recovery.initialize(unit.id);
    });
    id
}

fn snapshots(runtime: &mut RoomRuntime, sent: bool) {
    let selected = (0..runtime.state().world.game.players.len())
        .map(|slot| SelectedSeat {
            slot,
            full: true,
            omit: false,
        })
        .collect::<Vec<_>>();
    let tick = runtime.state().world.game.tick;
    let frames = runtime
        .collect_frame(
            &selected,
            if sent {
                FrameKind::Scheduled
            } else {
                FrameKind::Current
            },
        )
        .unwrap();
    assert_eq!(frames.len(), selected.len());
    for frame in frames {
        assert_eq!(frame.message["tick"], tick);
        assert_eq!(frame.message["mode"]["kind"], "horde");
    }
    assert!(runtime.fault().is_none());
}

#[test]
fn net73_hosted_horde_and_defender_keep_delivered_views_on_adaptive_room_beats() {
    // Prototype server/room-tick.js:12-21 and shared/ai-schedule.js:23-40:
    // step, refresh all requested recipient views on the actual send beat, act.
    let mut state = horde_scene(true);
    let horde = state.horde.as_ref().unwrap().slot;
    for i in 0..12 {
        let id = rifle(
            &mut state,
            horde,
            Point {
                x: 40.0 + f64::from(i) * 5.0,
                z: 130.0,
            },
        );
        if i == 0 {
            state.world.game.units.get_mut(&id).unwrap().hp = 1.0;
        }
    }
    let bunker_at = state.world.game.units[&state.horde.as_ref().unwrap().bunker].at();
    state.end_horde_break().unwrap();
    let mut runtime = RoomRuntime::new(state, 0).unwrap();
    runtime.configure_ai(&[(0, Level::Hard)]).unwrap();
    assert_eq!(runtime.ai.as_ref().unwrap().seats.len(), 2);
    assert!(matches!(
        runtime.ai.as_ref().unwrap().commander(0).unwrap().brain(),
        CommanderBrain::Human { .. }
    ));
    assert!(matches!(
        runtime
            .ai
            .as_ref()
            .unwrap()
            .commander(horde)
            .unwrap()
            .brain(),
        CommanderBrain::Horde(_)
    ));
    let mut cadences = BTreeSet::new();
    let mut human_inputs = 0;
    let mut horde_movements = 0;
    let mut bunker_attack = false;
    let mut accepted_human = 0;
    let mut accepted_horde = 0;
    let mut retained = 0;
    let mut human_refreshes = 0;
    let mut horde_refreshes = 0;
    for _ in 0..400 {
        let report = runtime.advance_tick(&[false, false]).unwrap();
        for result in &report.commands {
            if result.denial.is_none() {
                if result.key.issuer == 0 {
                    accepted_human += 1;
                } else if result.key.issuer as usize == horde {
                    accepted_horde += 1;
                }
            }
        }
        assert!(runtime.state().finish.is_none());
        let tick = report.tick;
        let sent = runtime.should_snapshot(tick);
        let ai = runtime.ai.as_ref().unwrap();
        let before = [
            ai.delivered(0).unwrap().clone(),
            ai.delivered(horde).unwrap().clone(),
        ];
        let wants_human = ai.commander(0).unwrap().wants_view(tick, sent);
        runtime.think(sent).unwrap();
        let ai = runtime.ai.as_ref().unwrap();
        for (index, slot) in [0, horde].into_iter().enumerate() {
            let view = ai.delivered(slot).unwrap();
            if sent && (slot == horde || wants_human) {
                assert_eq!(view.tick, tick, "requested seat {slot} must refresh first");
                if slot == horde {
                    horde_refreshes += 1;
                } else {
                    human_refreshes += 1;
                }
            } else {
                assert_eq!(view, &before[index], "seat {slot} read between deliveries");
                retained += 1;
            }
        }
        // Both persistent brains must consume the packets produced by the shared
        // schedule, rather than independently reading the current Match.
        match ai.commander(0).unwrap().brain() {
            CommanderBrain::Human {
                state, delivered, ..
            } => {
                let packet = delivered.as_ref().expect("human has its seeded packet");
                assert_eq!(packet.tick as i64, ai.delivered(0).unwrap().tick);
                assert!(state.snapshot().is_some());
            }
            _ => panic!("defender lost its human commander"),
        }
        match ai.commander(horde).unwrap().brain() {
            CommanderBrain::Horde(brain) => {
                if let Some(packet) = brain.delivered() {
                    assert_eq!(packet.tick, ai.delivered(horde).unwrap().tick);
                } else {
                    assert_eq!(tick, 1, "Horde missed its first delivered packet");
                }
            }
            _ => panic!("internal seat lost its scripted director"),
        }
        let mut this_horde_batch = 0;
        for (key, command) in runtime.state().commands.pending_tick(tick as u64 + 1) {
            if key.issuer == 0 {
                human_inputs += 1;
            } else if key.issuer as usize == horde {
                assert!(
                    !matches!(command, Command::Buy { .. } | Command::Retreat { .. }),
                    "the wave director must not shop or retreat"
                );
                if let Command::Move {
                    orders,
                    attack_move,
                    ..
                } = command
                {
                    this_horde_batch += orders.len();
                    horde_movements += orders.len();
                    bunker_attack |=
                        *attack_move && orders.iter().any(|order| order.at == bunker_at);
                }
            }
        }
        assert!(this_horde_batch <= ww2_core::horde_commander::ORDER_BATCH);
        snapshots(&mut runtime, sent);
        cadences.insert(runtime.snapshot_cadence());
        // Real meter inputs increase cadence from 2 through 3 to its maximum 4.
        runtime
            .record_timing(
                TickSample {
                    durations: [45.0, 5.0, 5.0, 35.0, 30.0, 5.0],
                    sent,
                },
                tick as u64 * 50,
            )
            .unwrap();
    }
    assert_eq!(cadences, BTreeSet::from([2, 3, 4]));
    assert!(retained > 0 && human_refreshes > 1 && horde_refreshes > human_refreshes);
    assert!(
        human_inputs > 0 && accepted_human > 0,
        "human brain never actuated"
    );
    assert!(
        bunker_attack,
        "the scripted director must attack-move on its delivered bunker"
    );
    assert!(
        horde_movements > 4 && accepted_horde > 0,
        "Horde queue never drained"
    );
    // The actual shared Schedule rejects a repeated callback at the same tick.
    assert!(runtime.think(false).unwrap_err().contains("InvalidTick"));
}

#[test]
fn net73_hidden_enemy_changes_do_not_change_delivered_defender_brain_or_orders() {
    // Prototype shared/ai-schedule.js:30-40 retains the last delivered view;
    // shared/ai.js:256 plans without a reference to authoritative state.
    fn setup() -> (RoomRuntime, u32) {
        let mut state = horde_scene(false);
        let slot = state.horde.as_ref().unwrap().slot;
        let hidden = rifle(&mut state, slot, Point { x: 140.0, z: 145.0 });
        state.update_horde_vision().unwrap();
        let mut runtime = RoomRuntime::new(state, 0).unwrap();
        runtime.configure_ai(&[(0, Level::Hard)]).unwrap();
        assert!(
            !runtime
                .ai
                .as_ref()
                .unwrap()
                .delivered(0)
                .unwrap()
                .units
                .contains_key(&hidden)
        );
        (runtime, hidden)
    }
    let (mut baseline, hidden) = setup();
    let (mut changed, changed_hidden) = setup();
    assert_eq!(hidden, changed_hidden);
    let mut retained = 0;
    for tick in 1..=160 {
        baseline.advance_tick(&[false, false]).unwrap();
        changed.advance_tick(&[false, false]).unwrap();
        // Alter a real canonical enemy after the simulation phase. Restore it
        // before the next phase so only observation/planning can see this change.
        let original = {
            let unit = changed
                .state_mut()
                .world
                .game
                .units
                .get_mut(&hidden)
                .unwrap();
            let original = (unit.x, unit.hp);
            unit.x += 8.0;
            unit.hp -= 1.0;
            original
        };
        let sent = baseline.should_snapshot(tick);
        let previous = baseline.ai.as_ref().unwrap().delivered(0).unwrap().clone();
        baseline.think(sent).unwrap();
        changed.think(sent).unwrap();
        let left = baseline.ai.as_ref().unwrap();
        let right = changed.ai.as_ref().unwrap();
        assert!(!left.delivered(0).unwrap().units.contains_key(&hidden));
        assert_eq!(left.delivered(0), right.delivered(0));
        assert_eq!(
            left.commander(0),
            right.commander(0),
            "hidden facts changed human policy"
        );
        if !sent {
            assert_eq!(left.delivered(0).unwrap(), &previous);
            retained += 1;
        }
        let pending = |runtime: &RoomRuntime| {
            runtime
                .state()
                .commands
                .pending_tick(tick as u64 + 1)
                .filter(|(key, _)| key.issuer == 0)
                .map(|(key, command)| (*key, command.clone()))
                .collect::<Vec<_>>()
        };
        assert_eq!(pending(&baseline), pending(&changed));
        {
            let unit = changed
                .state_mut()
                .world
                .game
                .units
                .get_mut(&hidden)
                .unwrap();
            unit.x = original.0;
            unit.hp = original.1;
        }
        snapshots(&mut baseline, sent);
        snapshots(&mut changed, sent);
    }
    assert!(retained > 0);
}
