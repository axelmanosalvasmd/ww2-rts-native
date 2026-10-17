//! Sandbox commands use the real R1 start, queue, tick and recipient paths.
use super::*;
use crate::room_runtime::FrameKind;
use ww2_core::recipient_snapshot::SelectedSeat;

struct Sandbox {
    hub: Hub,
    peers: [Connection; 2],
    receivers: [mpsc::Receiver<Output>; 2],
}
impl Sandbox {
    fn lobby() -> Self {
        let mut hub = Hub::new(1, String::new());
        hub.configure_trusted_room(Some(
            crate::TrustedRoomConfig::new("BOX", 0x3c1055).unwrap(),
        ));
        let mut join = |id, name| {
            let (tx, rx) = mpsc::channel(64);
            let (cancel, _) = watch::channel(false);
            let peer = Connection::new(id, tx, cancel);
            hub.message(
                "BOX",
                &peer,
                &mut false,
                json!({"t":"hello","name":name,"token":name}),
                0,
            );
            (peer, rx)
        };
        let (a, ra) = join(1, "a");
        let (b, rb) = join(2, "b");
        let mut result = Self {
            hub,
            peers: [a, b],
            receivers: [ra, rb],
        };
        result.drain();
        result
    }
    fn send(&mut self, seat: usize, message: Value) {
        self.hub
            .message("BOX", &self.peers[seat], &mut true, message, 1000);
    }
    fn drain(&mut self) -> [Vec<Value>; 2] {
        self.receivers.each_mut().map(|rx| {
            let mut messages = Vec::new();
            while let Ok(output) = rx.try_recv() {
                let Output::Json(value) = output else {
                    panic!("unexpected output")
                };
                messages.push(value);
            }
            messages
        })
    }
    fn start(&mut self, sandbox: bool) -> [Value; 2] {
        self.send(0, json!({"t":"sandbox","v":sandbox}));
        self.drain();
        self.send(0, json!({"t":"start"}));
        self.drain()
            .map(|messages| messages.into_iter().find(|m| m["t"] == "start").unwrap())
    }
    fn runtime(&mut self) -> &mut RoomRuntime {
        &mut self
            .hub
            .rooms
            .get_mut("BOX")
            .unwrap()
            .native
            .as_mut()
            .unwrap()
            .runtime
    }
    fn tick(&mut self) -> (ww2_core::match_tick::TickReport, [Value; 2]) {
        let runtime = self.runtime();
        let report = runtime.advance_tick(&[false, false]).unwrap();
        let seats = [0, 1].map(|slot| SelectedSeat {
            slot,
            full: true,
            omit: false,
        });
        let frames = runtime.collect_frame(&seats, FrameKind::Scheduled).unwrap();
        assert!(runtime.fault().is_none());
        assert!(runtime.state().tick_state.fault().is_none());
        assert_eq!(frames.len(), 2);
        let frames: [Value; 2] = frames
            .into_iter()
            .enumerate()
            .map(|(slot, frame)| {
                assert_eq!(frame.slot, slot);
                assert_eq!(frame.message["t"], "s");
                assert_eq!(frame.message["tick"], report.tick);
                frame.message
            })
            .collect::<Vec<_>>()
            .try_into()
            .unwrap();
        (report, frames)
    }
}
fn row(frame: &Value, id: u32) -> Option<&Value> {
    frame["units"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row[0] == id)
}

#[test]
fn sandbox_start_flag_host_authority_restart_and_every_seat_snapshots() {
    let mut room = Sandbox::lobby();
    room.send(1, json!({"t":"sandbox","v":true}));
    assert!(!room.hub.rooms["BOX"].sandbox);
    room.send(0, json!({"t":"sandbox","v":[]})); // JS truthy.
    assert!(room.hub.rooms["BOX"].sandbox);
    room.drain();
    room.send(1, json!({"t":"start"}));
    assert!(room.hub.rooms["BOX"].native.is_none());
    room.drain();
    let starts = room.start(true);
    for (seat, start) in starts.iter().enumerate() {
        assert_eq!(
            start
                .as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            [
                "cells", "factions", "fog", "map", "matchId", "names", "sandbox", "spawn",
                "spawns", "t", "teams", "weather", "you"
            ]
        );
        assert_eq!(start["sandbox"], true);
        assert_eq!(start["you"], seat);
        assert_eq!(start["teams"], json!([0, 1]));
        assert_eq!(start["names"], json!(["a", "b"]));
    }
    assert!(
        room.hub.rooms["BOX"]
            .native
            .as_ref()
            .unwrap()
            .receipt
            .as_ref()
            .unwrap()["options"]["sandbox"]
            .as_bool()
            .unwrap()
    );
    for _ in 0..6 {
        room.tick();
    }
    room.send(0, json!({"t":"sandbox","v":false}));
    assert!(room.hub.rooms["BOX"].sandbox);
    room.drain();
    room.send(0, json!({"t":"restart"}));
    for messages in room.drain() {
        let start = messages.iter().find(|m| m["t"] == "start").unwrap();
        assert_eq!(start["sandbox"], true);
        assert_eq!(start["matchId"], 2);
    }
    for _ in 0..6 {
        room.tick();
    }
    {
        let mode = "world";
        let mut guarded = Sandbox::lobby();
        guarded.hub.rooms.get_mut("BOX").unwrap().mode = mode.into();
        guarded.send(0, json!({"t":"sandbox","v":true}));
        guarded.send(0, json!({"t":"start"}));
        assert!(guarded.hub.rooms["BOX"].native.is_none(), "{mode}");
    }
}

#[test]
fn sandbox_guest_spawn_dispatch_is_free_and_retains_recipient_fog_and_owner_fields() {
    let mut room = Sandbox::lobby();
    room.start(true);
    let before = room.runtime().state().world.game.units.len();
    let own = room.runtime().state().start.players[1].spawn;
    let enemy = room.runtime().state().start.players[0].spawn;
    let id = room.runtime().state().next_unit_id;
    room.send(
        1,
        json!({"t":"spawn","unit":"tiger","x":own.x,"z":own.z,
        "slot":0,"issuer":0,"tick":999,"sequence":99}),
    );
    room.send(
        1,
        json!({"t":"spawn","unit":"kaiju","enemy":[],"x":enemy.x,"z":enemy.z}),
    );
    let state = room.runtime().state();
    assert_eq!(state.world.game.units.len(), before);
    assert!(
        state
            .commands
            .pending_tick(1)
            .all(|(key, _)| key.issuer == 1)
    );
    let spent = state
        .story
        .iter()
        .map(|s| format!("{s:?}"))
        .collect::<Vec<_>>();
    let (report, frames) = room.tick();
    assert_eq!(report.commands.len(), 2);
    assert!(report.commands.iter().all(|r| r.denial.is_none()));
    let state = room.runtime().state();
    assert_eq!(state.world.game.units[&id].owner, 1);
    assert_eq!(state.world.game.units[&(id + 1)].owner, 0);
    assert_eq!(
        state
            .story
            .iter()
            .map(|s| format!("{s:?}"))
            .collect::<Vec<_>>(),
        spent
    );
    assert!(row(&frames[1], id).is_some());
    assert!(
        row(&frames[0], id).is_none(),
        "sandbox does not reveal hidden enemy units"
    );
    assert!(row(&frames[0], id + 1).is_some());
    assert!(row(&frames[1], id + 1).is_none());
    for _ in 0..5 {
        room.tick();
    }
    // Exercise victory and reveal with the newly published metadata still alive.
    let win_vp = room.runtime().state().start.win_vp;
    room.runtime().state_mut().start.economy.players[0].vp = win_vp;
    for _ in 0..6 {
        room.tick();
    }
    assert!(room.runtime().state().finish.is_some());
    let (_, frames) = room.tick();
    let public = row(&frames[0], id).unwrap();
    let owner = row(&frames[1], id).unwrap();
    assert_eq!(public[11], 0);
    assert_eq!(public[12].as_u64().unwrap() & (8192 | 16384), 0);
    assert_ne!(owner[12].as_u64().unwrap() & 8192, 0);
    assert_eq!(
        public.as_array().unwrap().len(),
        owner.as_array().unwrap().len()
    );
    for held in 1..=120 {
        let ending = room.runtime().hold_callback().unwrap().unwrap();
        assert_eq!(ending.complete, held == 120);
        if ending.advance {
            room.tick();
        }
    }
    assert_eq!(room.runtime().finished_result().unwrap().winner, 0);
}

#[test]
fn sandbox_every_non_structure_kind_survives_ticks_and_both_recipient_snapshots() {
    let mut probe = Sandbox::lobby();
    probe.start(true);
    let kinds = probe
        .runtime()
        .state()
        .rules
        .kinds
        .iter()
        .filter(|kind| !kind.structure)
        .map(|kind| (kind.name.clone(), kind.air, kind.rail))
        .collect::<Vec<_>>();
    assert!(kinds.iter().any(|(_, air, _)| *air));
    assert!(kinds.iter().any(|(_, _, rail)| *rail));
    for (name, air, rail) in kinds {
        let mut room = Sandbox::lobby();
        room.start(true);
        let state = room.runtime().state();
        let id = state.next_unit_id;
        let at = state.start.players[1].spawn;
        let mp = state.start.economy.players[1].mp;
        room.send(1, json!({"t":"spawn","unit":name,"x":at.x,"z":at.z}));
        let (report, _) = room.tick();
        assert_eq!(report.commands.len(), 1, "{name}");
        assert_eq!(report.commands[0].denial, None, "{name}");
        let state = room.runtime().state();
        let unit = &state.world.game.units[&id];
        assert_eq!(state.rules.kinds[unit.kind].name, name);
        assert_eq!(unit.owner, 1);
        assert!(
            state.start.economy.players[1].mp >= mp,
            "spawn charges no manpower: {name}"
        );
        if air {
            assert!(unit.air.is_some());
            assert_ne!(unit.at(), at, "planes retain their normal air base");
        }
        if rail {
            assert!(!unit.rail && !state.rail_runs.contains_key(&id));
        }
        for _ in 0..5 {
            room.tick();
        }
    }
}

#[test]
fn sandbox_spawn_refusals_and_pause_never_publish_partial_state() {
    let mut room = Sandbox::lobby();
    room.start(false);
    room.send(1, json!({"t":"spawn","unit":"rifle","x":20,"z":20}));
    let (report, _) = room.tick();
    assert_eq!(
        report.commands[0].denial,
        Some(ww2_core::match_commands::Denial::Blocked)
    );
    let mut room = Sandbox::lobby();
    room.start(true);
    let count = room.runtime().state().world.game.units.len();
    for message in [
        json!({"t":"spawn","unit":"bunker","x":20,"z":20}),
        json!({"t":"spawn","unit":"__proto__","x":20,"z":20}),
        json!({"t":"spawn","unit":"rifle","x":"20","z":20}),
        json!({"t":"spawn","unit":"rifle","x":20}),
    ] {
        room.send(1, message);
        let (report, _) = room.tick();
        let messages = room.drain();
        if report.commands.is_empty() {
            assert_eq!(
                messages[1],
                vec![json!({"t":"deny","cmd":"spawn","reason":"blocked"})]
            );
        }
        assert!(
            report
                .commands
                .iter()
                .all(|r| r.denial == Some(ww2_core::match_commands::Denial::Blocked))
        );
        assert_eq!(room.runtime().state().world.game.units.len(), count);
    }
    room.runtime()
        .pause(0, "a", crate::room_runtime::PauseReason::Host, 1000)
        .unwrap();
    assert!(room.runtime().paused());
    room.send(1, json!({"t":"spawn","unit":"rifle","x":20,"z":20}));
    assert_eq!(room.runtime().state().commands.pending_commands(), 0);
    room.runtime().resume(1000);
    for _ in 0..6 {
        room.tick();
    }
    assert_eq!(room.runtime().state().world.game.units.len(), count);
}

#[test]
fn sandbox_kaiju_beam_damages_enemy_and_delivers_events_without_horde_state() {
    let mut room = Sandbox::lobby();
    room.start(true);
    // Use the open road away from the starting headquarters and their units.
    let at = ww2_core::game::Point { x: 71.0, z: 21.0 };
    // The recorded boundary sorts issuer 0 before issuer 1, independent of arrival.
    let victim = room.runtime().state().next_unit_id;
    let boss = victim + 1;
    room.send(1, json!({"t":"spawn","unit":"kaiju","x":at.x,"z":at.z}));
    room.send(
        0,
        json!({"t":"spawn","unit":"tiger","x":at.x+16.0,"z":at.z}),
    );
    let mut emitted = false;
    for _ in 0..80 {
        let (_, frames) = room.tick();
        emitted |= frames.iter().any(|frame| {
            frame["shots"]
                .as_array()
                .unwrap()
                .iter()
                .any(|shot| shot["k"] == "kaiju" && shot["f"] == boss)
        });
        if emitted {
            break;
        }
    }
    assert!(
        emitted,
        "the ordinary sandbox Kaiju must fire its actual beam"
    );
    let state = room.runtime().state();
    let definition = ww2_core::catalog::Catalog::compiled()
        .unwrap()
        .unit("tiger")
        .unwrap();
    let full_hp = definition.get("models").unwrap().number().unwrap()
        * definition.get("hpPer").unwrap().number().unwrap();
    assert!(
        state
            .world
            .game
            .units
            .get(&victim)
            .is_none_or(|victim| victim.hp < full_hp)
    );
    assert!(state.horde.is_none());
    for _ in 0..6 {
        room.tick();
    }
}

#[test]
fn sandbox_spawned_rifle_grenade_survives_throw_flight_and_expiry_for_every_seat() {
    use ww2_core::{game::Point, match_commands::Command};
    let mut room = Sandbox::lobby();
    room.start(true);
    let at = room.runtime().state().start.players[1].spawn;
    let rifle = room.runtime().state().next_unit_id;
    room.send(1, json!({"t":"spawn","unit":"rifle","x":at.x,"z":at.z}));
    room.tick();
    let at = room.runtime().state().world.game.units[&rifle].at();
    assert_eq!(
        room.runtime()
            .state_mut()
            .execute_command(
                1,
                &Command::Ability {
                    ids: vec![i64::from(rifle)],
                    at: Some(Point {
                        x: at.x - 8.0,
                        z: at.z
                    }),
                    queue: false,
                    automatic: false,
                },
                false
            )
            .unwrap(),
        None
    );
    let (mut threw, mut flew) = (false, false);
    for _ in 0..80 {
        let (_, frames) = room.tick();
        threw |= frames[1]["shots"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["k"] == "throw" && s["f"] == rifle);
        flew |= !frames[1]["flights"].as_array().unwrap().is_empty();
    }
    assert!(
        threw && flew,
        "the spawned rifle must publish its real throw and retained flight"
    );
    assert!(room.runtime().state().firing.grenades.is_empty());
    assert!(room.runtime().state().firing.flights.is_empty());
    for _ in 0..6 {
        room.tick();
    }
}

#[test]
fn sandbox_horde_start_and_spawn_keep_every_canonical_seat_live() {
    let mut room = Sandbox::lobby();
    room.send(0, json!({"t":"mode","v":"horde"}));
    room.send(0, json!({"t":"sandbox","v":true}));
    room.drain();
    room.send(0, json!({"t":"start"}));
    for messages in room.drain() {
        let start = messages.iter().find(|m| m["t"] == "start").unwrap();
        assert_eq!(start["sandbox"], true);
        assert_eq!(start["names"].as_array().unwrap().len(), 3);
    }
    let state = room.runtime().state();
    let slot = state.horde.as_ref().unwrap().slot;
    let id = state.next_unit_id;
    let at = state.start.players[0].spawn;
    room.send(
        1,
        json!({"t":"spawn","unit":"rifle","enemy":true,"x":at.x+20.,"z":at.z}),
    );
    let selected = (0..3)
        .map(|slot| SelectedSeat {
            slot,
            full: true,
            omit: false,
        })
        .collect::<Vec<_>>();
    for tick in 0..6 {
        let runtime = room.runtime();
        let report = runtime.advance_tick(&[false; 3]).unwrap();
        if tick == 0 {
            assert_eq!(report.commands.len(), 1);
            assert_eq!(report.commands[0].denial, None);
        }
        assert_eq!(runtime.state().world.game.units[&id].owner, slot);
        assert!(runtime.state().horde.as_ref().unwrap().active);
        let frames = runtime
            .collect_frame(&selected, FrameKind::Scheduled)
            .unwrap();
        assert_eq!(frames.len(), 3);
        assert!(
            frames
                .iter()
                .all(|frame| frame.message["mode"]["kind"] == "horde")
        );
        assert!(runtime.fault().is_none());
    }
}

#[test]
fn cmb150_nonworld_recovery_and_unknown_orders_keep_exact_blocked_denial() {
    let mut room = Sandbox::lobby();
    room.start(true);
    for command in ["recover", "unknown"] {
        room.send(1, json!({"t":command}));
    }
    assert_eq!(room.runtime().state().commands.pending_commands(), 0);
    let messages = room.drain();
    assert!(messages[0].is_empty());
    assert_eq!(
        messages[1],
        vec![
            json!({"t":"deny","cmd":"recover","reason":"blocked"}),
            json!({"t":"deny","cmd":"unknown","reason":"blocked"}),
        ]
    );
    for _ in 0..6 {
        room.tick();
    }
}
