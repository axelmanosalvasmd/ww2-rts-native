use super::*;

fn input(seq: i64, forward: i32) -> Value {
    json!({"t":"fps", "seq":seq, "forward":forward, "strafe":0, "yaw":0, "pitch":0})
}

fn commanders(hub: &Hub) -> Vec<Option<ww2_core::seat_commander::SeatCommander>> {
    (0..runtime(hub).state().world.game.players.len())
        .map(|slot| runtime(hub).ai_commander(slot).cloned())
        .collect()
}

#[test]
fn net79_host_pause_freezes_room_ai_armies_and_operatives_with_one_second_frames() {
    // Prototype e4e579a server.js:470-519,631,700-703,712,782:
    // pause gates simulation and brains; its one-second frame still reaches every role.
    for horde in [false, true] {
        let (mut hub, mut peers) = room(horde, "hard");
        peers[0].send(&mut hub, json!({"t":"start"}), 0);
        for peer in &mut peers {
            assert!(peer.drain(&mut hub, 0).iter().any(|m| m["t"] == "start"));
        }
        if horde {
            peers[0].send(&mut hub, json!({"t":"nextwave"}), 0);
        }
        let soldier = runtime(&hub).state().companions.get("op").unwrap().unit_id;
        let initial = &runtime(&hub).state().world.game.units[&soldier];
        let opening_position = (initial.x, initial.z);
        let opening_ai = commanders(&hub);
        let mut saw_ai_order = false;
        let mut saw_horde_order = false;
        for callback in 0..=240_u64 {
            let now = callback * 50;
            peers[1].send(&mut hub, input(callback as i64, 1), now + 20);
            hub.poll_native(now, now * 1_000_000);
            assert_eq!(hub.native_status("ABC").unwrap().fault, None);
            let state = runtime(&hub).state();
            let next_tick = u64::try_from(state.world.game.tick + 1).unwrap();
            saw_ai_order |= state
                .commands
                .pending_tick(next_tick)
                .any(|(key, _)| key.issuer == 1);
            saw_horde_order |= state
                .commands
                .pending_tick(next_tick)
                .any(|(key, _)| key.issuer == 2);
            for peer in &mut peers {
                peer.drain(&mut hub, now);
            }
        }
        assert!(runtime(&hub).state().finish.is_none());
        assert!(
            saw_ai_order,
            "the defender AI must issue real commands before pause"
        );
        assert_ne!(commanders(&hub), opening_ai);
        let moved = &runtime(&hub).state().world.game.units[&soldier];
        assert_ne!((moved.x, moved.z), opening_position);
        assert!(runtime(&hub).ai_delivered_view(1).unwrap().tick > 0);
        if horde {
            assert!(runtime(&hub).state().horde.as_ref().unwrap().active);
            assert!(
                saw_horde_order,
                "the internal Horde must issue real orders before pause"
            );
            assert!(runtime(&hub).ai_decoded_view(2).unwrap().tick > 0);
        }

        // Non-host roles cannot pause a running room.
        for peer in &mut peers[1..] {
            peer.send(&mut hub, json!({"t":"pause"}), 12_025);
        }
        assert!(!runtime(&hub).paused());
        let unit = runtime(&hub)
            .state()
            .world
            .game
            .units
            .values()
            .find(|u| {
                u.owner == 0
                    && u.id != soldier
                    && runtime(&hub).state().rules.kinds[u.kind].name == "rifle"
            })
            .unwrap()
            .id;
        let old_stance = runtime(&hub).state().world.game.units[&unit].hold_pos;
        let stance = |on| json!({"t":"stance", "ids":[unit], "key":"holdPos", "on":on});
        let pending_before = runtime(&hub).state().commands.pending_commands();
        peers[0].send(&mut hub, stance(!old_stance), 12_030);
        assert_eq!(
            runtime(&hub).state().commands.pending_commands(),
            pending_before + 1
        );
        // Materialize current recipient fog before freezing the full checkpoint.
        // Current frames leave initialized AI observations unchanged.
        let count = runtime(&hub).state().world.game.players.len();
        let selected = (0..count)
            .map(|slot| SelectedSeat {
                slot,
                full: false,
                omit: false,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            hub.rooms
                .get_mut("ABC")
                .unwrap()
                .native
                .as_mut()
                .unwrap()
                .runtime
                .collect_frame(&selected, FrameKind::Current)
                .unwrap()
                .len(),
            count
        );
        peers[0].send(&mut hub, json!({"t":"pause"}), 12_050);
        let pause = json!({"t":"pause", "paused":true, "by":"Host", "reason":"host", "left":0});
        for peer in &mut peers {
            assert_eq!(
                peer.drain(&mut hub, 12_050).as_slice(),
                std::slice::from_ref(&pause)
            );
        }
        let frozen = runtime(&hub).state().save_json().unwrap();
        let frozen_ai = commanders(&hub);
        let frozen_tick = runtime(&hub).state().world.game.tick;
        let frozen_view_tick = runtime(&hub).ai_delivered_view(1).unwrap().tick;
        let frozen_horde_tick = horde.then(|| runtime(&hub).ai_decoded_view(2).unwrap().tick);
        let horde_positions = runtime(&hub)
            .state()
            .world
            .game
            .units
            .values()
            .filter(|u| horde && u.owner == 2)
            .map(|u| (u.id, u.x, u.z))
            .collect::<Vec<_>>();
        let input_at = hub.rooms["ABC"].operatives[0].input_at;
        let seq = runtime(&hub).state().companions.get("op").unwrap().last_seq;
        let mut frames = [0; 3];
        for callback in 241..=302_u64 {
            let now = callback * 50;
            // Repeated host pauses cannot reset the original cadence. Resumes
            // from the operative and spectator cannot lift the host pause.
            peers[0].send(&mut hub, json!({"t":"pause"}), now);
            peers[0].send(&mut hub, stance(old_stance), now);
            peers[0].send(&mut hub, json!({"t":"stop", "ids":[unit]}), now);
            peers[0].send(&mut hub, json!({"t":"buy", "kind":"rifle"}), now);
            peers[1].send(&mut hub, json!({"t":"resume"}), now);
            peers[2].send(&mut hub, json!({"t":"resume"}), now);
            peers[1].send(&mut hub, input(10_000 + callback as i64, -1), now);
            hub.poll_native(now, now * 1_000_000);
            assert_eq!(hub.native_status("ABC").unwrap().fault, None);
            assert!(runtime(&hub).paused());
            assert_eq!(runtime(&hub).state().save_json().unwrap(), frozen);
            assert_eq!(commanders(&hub), frozen_ai);
            assert_eq!(
                runtime(&hub).ai_delivered_view(1).unwrap().tick,
                frozen_view_tick
            );
            if let Some(tick) = frozen_horde_tick {
                assert_eq!(runtime(&hub).ai_decoded_view(2).unwrap().tick, tick);
            }
            assert_eq!(hub.rooms["ABC"].operatives[0].input_at, input_at);
            assert_eq!(
                runtime(&hub).state().companions.get("op").unwrap().last_seq,
                seq
            );
            let due = now > 12_050 && (now - 12_050).is_multiple_of(1000);
            for (role, peer) in peers.iter_mut().enumerate() {
                let messages = peer.drain(&mut hub, now);
                if due {
                    assert_eq!(messages.len(), 2, "one pause plus one frame per role");
                    assert_eq!(messages[0], pause);
                    assert_eq!(messages[1]["t"], "s");
                    assert_eq!(messages[1]["tick"], frozen_tick);
                    frames[role] += 1;
                } else {
                    assert!(
                        messages.is_empty(),
                        "no output between one-second boundaries: {messages:?}"
                    );
                }
            }
        }
        assert_eq!(frames, [3; 3]);

        peers[0].send(&mut hub, json!({"t":"resume"}), 15_110);
        for peer in &mut peers {
            assert_eq!(
                peer.drain(&mut hub, 15_110),
                [json!({"t":"pause", "paused":false, "by":"", "reason":"host", "left":0})]
            );
        }
        assert!(!runtime(&hub).paused());
        peers[1].send(&mut hub, input(seq + 1, -1), 15_120);
        assert_eq!(
            runtime(&hub).state().companions.get("op").unwrap().last_seq,
            seq + 1
        );
        let paused_position = {
            let u = &runtime(&hub).state().world.game.units[&soldier];
            (u.x, u.z)
        };
        hub.poll_native(15_150, 15_150_000_000);
        assert_eq!(runtime(&hub).state().world.game.tick, frozen_tick + 1);
        assert_eq!(
            runtime(&hub).state().world.game.units[&unit].hold_pos,
            !old_stance,
            "the pre-pause order executes only after resume"
        );
        for peer in &mut peers {
            peer.drain(&mut hub, 15_150);
        }
        run_callbacks(&mut hub, &mut peers, 304, 343);
        let resumed = &runtime(&hub).state().world.game.units[&soldier];
        assert_ne!((resumed.x, resumed.z), paused_position);
        assert_ne!(commanders(&hub), frozen_ai);
        if horde {
            assert!(
                horde_positions.iter().any(|(id, x, z)| {
                    runtime(&hub)
                        .state()
                        .world
                        .game
                        .units
                        .get(id)
                        .is_some_and(|u| u.x != *x || u.z != *z)
                }),
                "the active Horde army continues its queued movement after resume"
            );
        }
        assert!(runtime(&hub).ai_delivered_view(1).unwrap().tick > frozen_view_tick);
    }
}
