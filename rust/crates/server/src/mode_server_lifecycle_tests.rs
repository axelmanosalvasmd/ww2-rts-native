//! Ordinary host lifecycle checks. Public mode admission and transports are unchanged.
use super::*;
use ww2_core::{match_commands::Denial, match_end::EndReason};

fn drain(hub: &mut Hub, receiver: &mut mpsc::Receiver<Output>, now: u64) {
    while let Ok(output) = receiver.try_recv() {
        match output {
            Output::Snapshot(frame) => hub.complete_snapshot(&frame, true, now),
            Output::Json(_) => {}
            Output::Close(_, _) => panic!("unexpected connection close"),
        }
    }
}

#[test]
fn net50_nextwave_messages_leave_ordinary_rooms_running() {
    let (mut hub, a, mut ra, b, mut rb) = room();
    hub.attach_native("ABC", native_match(), 0).unwrap();
    initial_start(&mut ra);
    initial_start(&mut rb);
    for callback in 0..6_u64 {
        let now = callback * 50;
        for connection in [&a, &b] {
            hub.message("ABC", connection, &mut true, json!({"t":"nextwave"}), now);
        }
        hub.poll_native(now, now * 1_000_000);
        drain(&mut hub, &mut ra, now);
        drain(&mut hub, &mut rb, now);
        let status = hub.native_status("ABC").unwrap();
        assert_eq!(status.tick, callback as i64 + 1);
        assert!(status.fault.is_none());
        assert!(
            hub.rooms["ABC"]
                .native
                .as_ref()
                .unwrap()
                .runtime
                .state()
                .horde
                .is_none()
        );
    }
}

#[test]
fn mode89_host_holds_for_120_callbacks_with_60_frames_and_team_or_draw_outcomes() {
    for draw in [false, true] {
        let (mut hub, a, mut ra, b, mut rb) = room();
        let mut options = native_match().options;
        // Team IDs deliberately differ from both seat IDs.
        for (slot, team) in [4, 2].into_iter().enumerate() {
            options.players[slot].team = team;
            hub.rooms.get_mut("ABC").unwrap().players[slot].team = team as usize;
        }
        let mut state = crate::authoritative_maps::create_match("default", options).unwrap();
        let finish = if draw {
            // The server receives an already decided outcome. Draw adjudication
            // belongs to the mode, while this test exercises its entire hold.
            state
                .initialize_tick_scene_finish(-1, EndReason::Draw, None)
                .unwrap()
        } else {
            state.start.economy.players[0].vp = state.start.win_vp;
            state.apply_conquest_victory().unwrap().unwrap()
        };
        let epoch = hub.attach_native("ABC", state, 0).unwrap();
        initial_start(&mut ra);
        initial_start(&mut rb);
        let mut frames = [0; 2];
        for callback in 1..=120_u64 {
            let now = (callback - 1) * 50;
            hub.poll_native(now, now * 1_000_000);
            let status = hub.native_status("ABC").unwrap();
            assert_eq!(status.epoch, epoch);
            assert_eq!(status.held, callback as u16);
            assert_eq!(status.tick, (callback / 2) as i64);
            assert_eq!(status.ending_complete, callback == 120);
            assert!(status.fault.is_none());
            for (slot, receiver) in [&mut ra, &mut rb].into_iter().enumerate() {
                if callback % 2 == 0 {
                    let Output::Snapshot(frame) = receiver.try_recv().unwrap() else {
                        panic!("each advancing hold callback must send a frame first")
                    };
                    assert_eq!(frame.epoch, epoch);
                    let snapshot: Value = serde_json::from_str(&frame.json).unwrap();
                    assert_eq!(snapshot["tick"], callback / 2);
                    assert_eq!(snapshot["winner"], finish.winner);
                    assert_eq!(snapshot["end"]["reason"], finish.reason.as_str());
                    hub.complete_snapshot(&frame, true, now);
                    frames[slot] += 1;
                }
                if callback == 120 {
                    let Output::Json(resume) = receiver.try_recv().unwrap() else {
                        panic!("resume must follow the final hold frame")
                    };
                    assert_eq!(resume["t"], "pause");
                    assert_eq!(resume["paused"], false);
                    let Output::Json(lobby) = receiver.try_recv().unwrap() else {
                        panic!("result lobby must follow the final frame and resume")
                    };
                    assert_eq!(lobby["t"], "lobby");
                    assert_eq!(lobby["state"], "lobby");
                    assert_eq!(lobby["result"]["winner"], finish.winner);
                    assert_eq!(lobby["result"]["teams"], json!([4, 2]));
                    assert_eq!(lobby["result"]["endTick"], finish.end_tick);
                    assert_eq!(
                        lobby["result"]["you"],
                        json!({"team":([4,2][slot]), "outcome":if draw {
                            "draw"
                        } else if slot == 0 {
                            "victory"
                        } else {
                            "defeat"
                        }})
                    );
                } else {
                    assert!(hub.rooms["ABC"].native.is_some());
                    assert!(hub.rooms["ABC"].result.is_none());
                }
                assert!(receiver.try_recv().is_err(), "unexpected hold output");
            }
            // No ordinary input can enter the queue after a winner is decided.
            if callback == 1 {
                for connection in [&a, &b] {
                    hub.message(
                        "ABC",
                        connection,
                        &mut true,
                        json!({"t":"stop","ids":[]}),
                        now,
                    );
                }
                let runtime = &hub.rooms["ABC"].native.as_ref().unwrap().runtime;
                assert!(runtime.state().commands.pending_tick(1).next().is_none());
                for receiver in [&mut ra, &mut rb] {
                    let Output::Json(denial) = receiver.try_recv().unwrap() else {
                        panic!("finished ordinary commands must be denied")
                    };
                    assert_eq!(denial, json!({"t":"deny","cmd":"stop","reason":"blocked"}));
                    assert!(receiver.try_recv().is_err());
                }
            }
        }
        assert_eq!(frames, [60, 60]);
        assert!(hub.rooms["ABC"].native.is_none());
        hub.poll_native(6000, 6_000_000_000);
        assert_eq!(hub.native_status("ABC").unwrap().tick, 60);
        assert!(ra.try_recv().is_err() && rb.try_recv().is_err());
    }
}

#[test]
fn mode95_disconnect_propagates_each_tick_freezes_income_and_vp_and_refuses_commands() {
    let (mut hub, a, mut ra, b, mut rb) = room();
    let mut state = native_match();
    assert!(state.start.points.len() >= 2);
    for slot in 0..2 {
        state.start.points[slot].owner = Some(slot);
        state.start.points[slot].cut = false;
        assert!(state.start.points[slot].vp > 0.0);
        state.start.economy.players[slot].mp = 100.0;
    }
    let units = (0..2)
        .map(|slot| {
            state
                .world
                .game
                .units
                .values()
                .find(|unit| unit.owner == slot && state.rules.kinds[unit.kind].name == "rifle")
                .unwrap()
                .id
        })
        .collect::<Vec<_>>();
    for id in &units {
        state.commands.units.get_mut(id).unwrap().hold_fire = false;
    }
    hub.attach_native("ABC", state, 0).unwrap();
    initial_start(&mut ra);
    initial_start(&mut rb);
    hub.poll_native(0, 0);
    let before = hub.rooms["ABC"]
        .native
        .as_ref()
        .unwrap()
        .runtime
        .state()
        .start
        .economy
        .players
        .clone();
    assert!(
        before
            .iter()
            .all(|player| !player.away && player.mp > 100.0 && player.vp > 0.0)
    );
    // Both inputs enter while connected. The first seat drops before execution.
    for (connection, id) in [(&a, units[0]), (&b, units[1])] {
        hub.message(
            "ABC",
            connection,
            &mut true,
            json!({"t":"stance","ids":[id],"key":"holdFire","on":true}),
            1,
        );
    }
    assert_eq!(
        hub.rooms["ABC"]
            .native
            .as_ref()
            .unwrap()
            .runtime
            .state()
            .commands
            .pending_tick(2)
            .count(),
        2
    );
    hub.disconnected("ABC", a.id, 2);
    assert!(hub.native_status("ABC").unwrap().paused);
    hub.poll_native(50, 50_000_000);
    assert_eq!(hub.native_status("ABC").unwrap().tick, 1);
    drain(&mut hub, &mut ra, 50);
    drain(&mut hub, &mut rb, 50);
    // Expire the real drop pause without simulating a thirty-second backlog.
    hub.poll_native(30_002, 100_000_000);
    assert_eq!(hub.native_status("ABC").unwrap().tick, 2);
    assert!(!hub.native_status("ABC").unwrap().paused);
    for beat in 3..=12_u64 {
        // The expiry poll consumed the callback at 100 ms. Each later beat
        // must advance the monotonic clock by another complete 50 ms.
        hub.poll_native(30_002 + (beat - 2) * 50, beat * 50_000_000);
        drain(&mut hub, &mut rb, 30_002 + (beat - 2) * 50);
        let runtime = &hub.rooms["ABC"].native.as_ref().unwrap().runtime;
        let state = runtime.state();
        assert_eq!(state.world.game.tick, beat as i64);
        assert!(state.start.economy.players[0].away);
        assert!(!state.start.economy.players[1].away);
        assert_eq!(state.start.economy.players[0].mp, before[0].mp);
        assert_eq!(state.start.economy.players[0].vp, before[0].vp);
        assert_eq!(state.start.economy.players[0].inc, 0.0);
        assert_eq!(state.start.economy.players[0].mun, before[0].mun);
        assert_eq!(state.start.economy.players[0].fuel, before[0].fuel);
        assert!(runtime.fault().is_none());
        assert_eq!(state.start.points[0].owner, Some(0));
        assert_eq!(state.start.points[1].owner, Some(1));
        assert!(!state.start.points[0].contested && !state.start.points[1].contested);
        assert!(state.start.economy.players[1].mp > before[1].mp);
        assert!(state.start.economy.players[1].vp > before[1].vp);
        assert!(!state.commands.units[&units[0]].hold_fire);
        assert!(state.commands.units[&units[1]].hold_fire);
        assert!(state.commands.pending_tick(2).next().is_none());
    }
    // The old socket cannot stamp new input after its roster connection is gone.
    hub.message(
        "ABC",
        &a,
        &mut true,
        json!({"t":"stance","ids":[units[0]],"key":"holdFire","on":true}),
        30_503,
    );
    let native = hub.rooms.get_mut("ABC").unwrap().native.as_mut().unwrap();
    assert!(
        native
            .runtime
            .state()
            .commands
            .pending_tick(13)
            .next()
            .is_none()
    );
    for command in [
        json!({"t":"move","orders":[[units[0],5,5]]}),
        json!({"t":"amove","orders":[[units[0],5,5]]}),
        json!({"t":"stop","ids":[units[0]]}),
        json!({"t":"stance","ids":[units[0]],"key":"holdFire","on":true}),
    ] {
        assert_eq!(
            native
                .ingress
                .enqueue(native.runtime.state_mut(), 0, &command),
            Err(Denial::Blocked)
        );
    }
    assert!(
        native
            .runtime
            .state()
            .commands
            .pending_tick(13)
            .next()
            .is_none()
    );
    let (replacement, mut replacement_rx) = client(3);
    join(&mut hub, &replacement, "a", 30_504);
    hub.poll_native(30_552, 650_000_000);
    let state = hub.rooms["ABC"].native.as_ref().unwrap().runtime.state();
    assert_eq!(state.world.game.tick, 13);
    assert!(!state.start.economy.players[0].away);
    assert!(state.start.economy.players[0].mp > before[0].mp);
    assert!(state.start.economy.players[0].vp > before[0].vp);
    hub.message(
        "ABC",
        &replacement,
        &mut true,
        json!({"t":"stance","ids":[units[0]],"key":"holdFire","on":true}),
        30_553,
    );
    hub.poll_native(30_602, 700_000_000);
    assert!(
        hub.rooms["ABC"]
            .native
            .as_ref()
            .unwrap()
            .runtime
            .state()
            .commands
            .units[&units[0]]
            .hold_fire
    );
    assert!(hub.native_status("ABC").unwrap().fault.is_none());
    drain(&mut hub, &mut replacement_rx, 30_602);
    drain(&mut hub, &mut rb, 30_602);
}

#[test]
fn net47_away_classic_seat_freezes_munitions_and_fuel_until_present_again() {
    use ww2_core::economy_state::{Context, EconomyState, ModeContext, PlayerEconomy};
    let mut economy = EconomyState {
        players: (0..2)
            .map(|slot| PlayerEconomy {
                team: slot,
                away: slot == 0,
                mp: 50.0,
                mun: 20.0,
                fuel: 10.0,
                ..PlayerEconomy::default()
            })
            .collect(),
    };
    let context = Context {
        mode: ModeContext {
            kind: ww2_core::economy::Mode::Classic,
            ..ModeContext::default()
        },
        army: ww2_core::economy_state::ArmySize::Standard,
        units: &[],
        drops: &[],
        points: &[],
        nodes: &[],
        world: None,
    };
    for _ in 0..6 {
        economy.advance_income(&context, 0.05, false).unwrap();
        let away = &economy.players[0];
        assert_eq!(
            (away.mp, away.mun, away.fuel, away.inc),
            (50.0, 20.0, 10.0, 0.0)
        );
        let present = &economy.players[1];
        assert!(present.mp > 50.0 && present.mun > 20.0 && present.fuel > 10.0);
    }
    economy.players[0].away = false;
    economy.advance_income(&context, 0.05, false).unwrap();
    let returned = &economy.players[0];
    assert!(returned.mp > 50.0 && returned.mun > 20.0 && returned.fuel > 10.0);
}
