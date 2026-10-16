//! The remaining prototype command union uses seated authority and canonical owners.
use super::*;
use ww2_core::{
    economy::Mode,
    game::Point,
    match_commands::Command,
    orders::OrderKey,
    recipient_snapshot::SnapshotCache,
    unit_publication::{PublicationParts, StagedUnit, prepare_publication},
};

fn scene(mode: Mode) -> Match {
    let map = crate::authoritative_maps::load("default").unwrap();
    let mut options = native_tests::native_match().options;
    options.mode = mode;
    options.tech = false;
    options.map_key = Some("default".into());
    let mut state = if mode == Mode::Classic {
        Match::new_classic(map, options).unwrap()
    } else {
        Match::new(map, options).unwrap()
    };
    for player in &mut state.start.economy.players {
        player.mp = 10_000.;
        player.mun = 10_000.;
        player.fuel = 10_000.;
    }
    for control in state.commands.units.values_mut() {
        control.hold_fire = true;
        control.auto_cast = false;
        control.auto_retreat = false;
    }
    for unit in state.world.game.units.values_mut() {
        unit.hold_pos = true;
    }
    state
}

fn owned(state: &Match, slot: usize, name: &str) -> u32 {
    state
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == slot && state.rules.kinds[u.kind].name == name)
        .unwrap()
        .id
}

fn add(state: &mut Match, slot: usize, name: &str, at: Point) -> u32 {
    let id = state.next_unit_id;
    let mut draft = StagedUnit::ordinary(
        &state.world.game,
        &state.rules,
        slot,
        state.rules.kind(name).unwrap(),
        id,
        state.world.game.units.len() as u32,
        state.start.players[slot].spawn,
        Some(at),
    )
    .unwrap();
    draft.unit.hold_pos = true;
    draft.control.hold_fire = true;
    draft.control.auto_cast = false;
    draft.control.auto_retreat = false;
    prepare_publication(
        PublicationParts {
            units: &mut state.world.game.units,
            grid: &mut state.grid,
            controls: &mut state.commands.units,
            recruitment: &mut state.recruit_metadata,
            next_id: &mut state.next_unit_id,
        },
        &state.rules,
        2,
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

struct Room {
    hub: Hub,
    peers: Vec<Connection>,
    receivers: Vec<mpsc::Receiver<Output>>,
    now: u64,
    nanos: u64,
}
impl Room {
    fn new(state: Match) -> Self {
        let mut hub = Hub::new(32, String::new());
        let mut peers = Vec::new();
        let mut receivers = Vec::new();
        for slot in 0..2 {
            let (tx, rx) = mpsc::channel(128);
            let (cancel, _) = watch::channel(false);
            let peer = Connection::new(slot + 1, tx, cancel);
            hub.message(
                "ABC",
                &peer,
                &mut false,
                json!({"t":"hello","token":format!("seat{slot}"),"name":"seat"}),
                0,
            );
            peers.push(peer);
            receivers.push(rx);
        }
        if state.options.mode == Mode::Classic {
            hub.rooms.get_mut("ABC").unwrap().mode = "classic".into();
        }
        hub.attach_native("ABC", state, 0).unwrap();
        let mut room = Self {
            hub,
            peers,
            receivers,
            now: 0,
            nanos: 0,
        };
        room.drain();
        room
    }
    fn state(&self) -> &Match {
        self.hub.rooms["ABC"]
            .native
            .as_ref()
            .unwrap()
            .runtime
            .state()
    }
    fn drain(&mut self) -> Vec<Vec<Value>> {
        self.receivers
            .iter_mut()
            .map(|rx| {
                let mut messages = Vec::new();
                while let Ok(output) = rx.try_recv() {
                    match output {
                        Output::Json(value) => messages.push(value),
                        Output::Snapshot(write) => {
                            messages.push(serde_json::from_str(&write.json).unwrap());
                            self.hub.complete_snapshot(&write, true, self.now);
                        }
                        Output::Watcher(_) => panic!("unexpected watcher"),
                        Output::Close(..) => panic!("unexpected close"),
                    }
                }
                messages
            })
            .collect()
    }
    fn message(&mut self, slot: usize, raw: Value) {
        self.hub
            .message("ABC", &self.peers[slot], &mut true, raw, self.now);
    }
    fn step(&mut self) -> Vec<Vec<Value>> {
        self.hub.poll_native(self.now, self.nanos);
        self.nanos += 50_000_000;
        self.now += 50;
        assert!(self.hub.native_status("ABC").unwrap().fault.is_none());
        if let Some(native) = self.hub.rooms["ABC"].native.as_ref() {
            let cache = SnapshotCache::new(native.runtime.state()).unwrap();
            for slot in 0..2 {
                cache.recipient(slot).unwrap();
            }
        }
        self.drain()
    }
    fn issue(&mut self, slot: usize, raw: Value) -> Vec<Vec<Value>> {
        self.now += 1_000;
        self.message(slot, raw);
        self.step()
    }
    fn accepted(&mut self, slot: usize, raw: Value) {
        let messages = self.issue(slot, raw);
        assert!(
            !messages.iter().flatten().any(|v| v["t"] == "deny"),
            "{messages:?}"
        );
    }
    fn healthy(&mut self) {
        let mut frames = [false; 2];
        for _ in 0..6 {
            let messages = self.step();
            for (slot, messages) in messages.iter().enumerate() {
                assert!(!messages.iter().any(|v| v["t"] == "deny"));
                frames[slot] |= messages.iter().any(|v| v["t"] == "s");
            }
        }
        assert_eq!(frames, [true, true]);
    }
}

#[test]
fn net43_union_guest_pause_stamps_malformed_and_exact_recipient_local_denials() {
    for (raw, reason) in [
        (json!({"t":"attack","ids":[],"target":-1}), "unseen"),
        (json!({"t":"ability","ids":[]}), "needs"),
        (json!({"t":"autocast","ids":[],"on":true}), "needs"),
        (
            json!({"t":"build","kind":"flakpos","ids":[],"x":20,"z":20}),
            "noBuilders",
        ),
        (json!({"t":"board","ids":[],"target":-1}), "unseen"),
        (json!({"t":"unload","ids":[]}), "needs"),
        (json!({"t":"escort","ids":[],"target":-1}), "unseen"),
        (json!({"t":"rail","ids":[],"x":20,"z":20}), "blocked"),
        (
            json!({"t":"orders","commands":[{"t":"attack","ids":[],"target":-1}]}),
            "unseen",
        ),
        (json!({"t":"cancelProduction","id":-1,"job":1}), "needs"),
    ] {
        let mut room = Room::new(scene(Mode::Conquest));
        let mut forged = raw.clone();
        forged["issuer"] = json!(0);
        forged["slot"] = json!(0);
        forged["tick"] = json!(999);
        forged["sequence"] = json!(999);
        room.message(0, json!({"t":"pause"}));
        room.message(1, forged.clone());
        assert_eq!(room.state().commands.pending_commands(), 0);
        room.message(0, json!({"t":"resume"}));
        room.drain();
        let mut malformed = forged.clone();
        malformed["orders"] = json!([[1, 0, "bad"]]);
        room.message(1, malformed);
        assert_eq!(room.state().commands.pending_commands(), 0);
        let denied = room.drain();
        assert!(!denied[0].iter().any(|v| v["t"] == "deny"));
        assert_eq!(
            denied[1]
                .iter()
                .filter(|v| v["t"] == "deny")
                .collect::<Vec<_>>(),
            vec![&json!({"t":"deny","cmd":raw["t"],"reason":"blocked"})]
        );
        room.now += 1_000;
        room.message(1, forged);
        let queued = room.state().commands.pending_tick(1).collect::<Vec<_>>();
        assert_eq!(queued.len(), 1);
        assert_eq!(
            *queued[0].0,
            OrderKey {
                tick: 1,
                issuer: 1,
                sequence: 1
            }
        );
        assert_eq!(queued[0].1, &Command::from_json(&raw).unwrap());
        let denied = room.step();
        assert!(!denied[0].iter().any(|v| v["t"] == "deny"));
        assert_eq!(
            denied[1]
                .iter()
                .filter(|v| v["t"] == "deny")
                .collect::<Vec<_>>(),
            vec![&json!({"t":"deny","cmd":raw["t"],"reason":reason})]
        );
        assert_eq!(room.state().commands.pending_commands(), 0);
        room.healthy();
    }
}

#[test]
fn net43_attack_and_grouped_orders_keep_selection_and_parent_queue_semantics() {
    let mut state = scene(Mode::Conquest);
    let actor = owned(&state, 1, "rifle");
    let at = state.world.game.units[&actor].at();
    let target = add(
        &mut state,
        0,
        "rifle",
        Point {
            x: at.x + 12.,
            z: at.z,
        },
    );
    let foreign = owned(&state, 0, "rifle");
    for player in &mut state.world.game.players {
        player.visible = state.world.game.units.keys().copied().collect();
    }
    let mut room = Room::new(state);
    room.accepted(
        1,
        json!({"t":"attack","ids":[-1,foreign,actor],"target":target}),
    );
    assert_eq!(room.state().commands.units[&actor].attack_id, target);
    assert_eq!(room.state().commands.units[&foreign].attack_id, 0);
    room.accepted(
        1,
        json!({"t":"orders","queue":true,"commands":[
            {"t":"attack","ids":[actor],"target":target,"queue":false},
            {"t":"rally","x":110,"z":110},
            {"t":"destroy","ids":[actor]}
        ]}),
    );
    assert!(
        matches!(room.state().commands.units[&actor].orders.back(), Some(Command::Attack { target: id, .. }) if *id == i64::from(target))
    );
    assert_eq!(
        room.state().player_rallies[1],
        Some(Point { x: 110., z: 110. })
    );
    assert!(room.state().world.game.units[&actor].hp > 0.);
    room.healthy();
}

#[test]
fn net43_ability_autocast_and_waiting_kind_six_survive_room_delivery() {
    let state = scene(Mode::Conquest);
    let actor = owned(&state, 1, "rifle");
    let at = state.world.game.units[&actor].at();
    let foreign = owned(&state, 0, "rifle");
    let mut room = Room::new(state);
    room.accepted(
        1,
        json!({"t":"autocast","ids":[-1,foreign,actor],"on":true}),
    );
    assert!(room.state().commands.units[&actor].auto_cast);
    assert!(!room.state().commands.units[&foreign].auto_cast);
    room.accepted(1, json!({"t":"autocast","ids":[actor],"on":false}));
    room.accepted(1, json!({"t":"move","orders":[[actor,at.x-40.,at.z]]}));
    let aim = Point {
        x: at.x - 20.,
        z: at.z,
    };
    room.accepted(
        1,
        json!({"t":"ability","ids":[actor],"x":aim.x,"z":aim.z,"queue":true}),
    );
    assert!(matches!(
        room.state().commands.units[&actor].orders.back(),
        Some(Command::Ability { .. })
    ));
    let cache = SnapshotCache::new(room.state()).unwrap();
    let owner = cache.recipient(1).unwrap().into_json();
    assert!(
        owner["orders"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row == &json!([actor, 1, 6, aim.x, aim.z]))
    );
    assert!(
        !cache.recipient(0).unwrap().into_json()["orders"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row[0] == actor)
    );
    room.healthy();
    room.accepted(1, json!({"t":"stop","ids":[actor]}));
    room.accepted(1, json!({"t":"ability","ids":[actor],"x":aim.x,"z":aim.z}));
    assert!(
        room.state().commands.units[&actor].nade.is_some()
            || room.state().abilities.get(actor).unwrap().cooldown > 0.
    );
    room.healthy();
}

#[test]
fn net43_board_unload_and_aircraft_escort_run_through_ordinary_rooms() {
    let mut state = scene(Mode::Conquest);
    let actor = owned(&state, 1, "rifle");
    let at = state.world.game.units[&actor].at();
    let carrier = add(
        &mut state,
        1,
        "halftrack",
        Point {
            x: at.x + 1.,
            z: at.z,
        },
    );
    let plane = state.spawn_aircraft(1, "fighter").unwrap();
    let mut room = Room::new(state);
    room.accepted(1, json!({"t":"board","ids":[-1,actor],"target":carrier}));
    for _ in 0..20 {
        if room.state().world.game.units[&actor].riding {
            break;
        }
        room.step();
    }
    assert!(room.state().world.game.units[&actor].riding);
    assert_eq!(
        room.state().combat.get(actor).unwrap().carrier,
        Some(carrier)
    );
    assert!(
        room.state()
            .combat
            .get(carrier)
            .unwrap()
            .cargo
            .contains(&actor)
    );
    room.healthy();
    room.accepted(
        1,
        json!({"t":"escort","ids":[actor,plane],"target":carrier}),
    );
    assert_eq!(
        room.state().aviation.planes[&plane].mission.unwrap().kind,
        ww2_core::match_aviation::MissionKind::Escort
    );
    room.healthy();
    room.accepted(1, json!({"t":"unload","ids":[carrier]}));
    assert!(!room.state().world.game.units[&actor].riding);
    assert!(room.state().combat.get(carrier).unwrap().cargo.is_empty());
    room.healthy();
    room.accepted(1, json!({"t":"move","orders":[[plane,at.x-10.,at.z]]}));
    room.accepted(1, json!({"t":"stop","ids":[plane]}));
    room.healthy();
}

#[test]
fn net43_build_and_worker_replacement_remain_live_after_admission() {
    let state = scene(Mode::Conquest);
    let worker = owned(&state, 1, "rifle");
    let origin = state.world.game.units[&worker].at();
    // Discover a valid visible placement against a detached fixture. The live
    // room receives the payment and site only through the public queued Build.
    let mut place = None;
    let mut probe = scene(Mode::Conquest);
    'find: for dz in -8..=8 {
        for dx in -8..=8 {
            let at = Point {
                x: origin.x + f64::from(dx) * 2.,
                z: origin.z + f64::from(dz) * 2.,
            };
            if probe
                .execute_build(
                    1,
                    &[i64::from(worker)],
                    "flakpos",
                    Some(at),
                    Some(0.),
                    false,
                )
                .unwrap()
                .is_none()
            {
                place = Some(at);
                break 'find;
            }
        }
    }
    let at = place.expect("canonical map supports a visible Flak site");
    let next = state.next_unit_id;
    let mut room = Room::new(state);
    room.accepted(
        1,
        json!({"t":"build","ids":[worker],"kind":"flakpos","x":at.x,"z":at.z,"dir":0}),
    );
    assert!(room.state().world.game.units.contains_key(&next));
    assert_eq!(
        room.state().commands.units[&worker].build_target,
        Some(next)
    );
    room.healthy();
    room.accepted(
        1,
        json!({"t":"stance","ids":[worker],"key":"autoRetreat","on":true}),
    );
    room.accepted(1, json!({"t":"stop","ids":[worker]}));
    assert!(!room.state().world.game.units[&worker].build);
    assert_eq!(room.state().commands.units[&worker].build_target, None);
    room.healthy();
}

#[test]
fn net43_classic_waiting_production_cancels_refunds_and_delivers_every_seat() {
    let state = scene(Mode::Classic);
    let hq = owned(&state, 1, "hq");
    let mut room = Room::new(state);
    room.accepted(1, json!({"t":"buy","unit":"engineer","from":hq}));
    room.accepted(1, json!({"t":"buy","unit":"engineer","from":hq}));
    let jobs = room
        .state()
        .production
        .building(hq)
        .unwrap()
        .jobs()
        .to_vec();
    assert_eq!(jobs.len(), 2);
    let before = room.state().start.economy.players[1].mp;
    let spent = room.state().story[1].mp_spent;
    let denied = room.issue(0, json!({"t":"cancelProduction","id":hq,"job":jobs[1].id}));
    assert_eq!(
        denied[0]
            .iter()
            .filter(|v| v["t"] == "deny")
            .collect::<Vec<_>>(),
        vec![&json!({"t":"deny","cmd":"cancelProduction","reason":"unseen"})]
    );
    assert!(!denied[1].iter().any(|v| v["t"] == "deny"));
    room.accepted(1, json!({"t":"cancelProduction","id":hq,"job":jobs[1].id}));
    assert_eq!(
        room.state().production.building(hq).unwrap().jobs(),
        &jobs[..1]
    );
    assert!(room.state().start.economy.players[1].mp >= before + jobs[1].charge.mp);
    assert_eq!(room.state().story[1].mp_spent, spent - jobs[1].charge.mp);
    let denied = room.issue(1, json!({"t":"cancelProduction","id":hq,"job":jobs[0].id}));
    assert_eq!(
        denied[1]
            .iter()
            .filter(|v| v["t"] == "deny")
            .collect::<Vec<_>>(),
        vec![&json!({"t":"deny","cmd":"cancelProduction","reason":"notWaiting"})]
    );
    let cache = SnapshotCache::new(room.state()).unwrap();
    let owner = cache.recipient(1).unwrap().into_json();
    assert!(owner["mun"].is_number() && owner["fuel"].is_number());
    assert_eq!(owner["mode"]["kind"], "classic");
    assert_eq!(
        owner["productionJobs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r[0] == hq)
            .unwrap()[1]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(
        !cache.recipient(0).unwrap().into_json()["productionJobs"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r[0] == hq)
    );
    let expected_nodes = room
        .state()
        .elimination
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .map(|node| {
            json!([
                node.at.x,
                node.at.z,
                node.rate,
                if node.fuel { 1. } else { 0. }
            ])
        })
        .collect::<Vec<_>>();
    assert_eq!(owner["nodes"], json!(expected_nodes));
    room.healthy();
    // The losing commander scuttles its only production building through the
    // ordinary command queue. Elimination and finish remain normal tick work.
    let losing_hq = owned(room.state(), 0, "hq");
    room.accepted(0, json!({"t":"destroy","ids":[losing_hq]}));
    let finish = room
        .state()
        .finish
        .expect("losing the last producer ends Classic");
    assert_eq!(finish.winner, 1);
    assert_eq!(finish.reason, ww2_core::match_end::EndReason::Headquarters);
    assert!(room.state().start.economy.players[0].out);
    assert!(!room.state().world.game.units.contains_key(&losing_hq));
    assert_eq!(room.hub.native_status("ABC").unwrap().held, 0);
    let final_tick = room.state().world.game.tick;
    let mut frame_counts = [0; 2];
    for callback in 1..=120_u16 {
        let messages = room.step();
        let status = room.hub.native_status("ABC").unwrap();
        assert_eq!(status.held, callback);
        assert_eq!(status.ending_complete, callback == 120);
        assert_eq!(status.tick, final_tick + i64::from(callback / 2));
        assert!(status.fault.is_none());
        for (slot, messages) in messages.iter().enumerate() {
            let frames = messages
                .iter()
                .filter(|message| message["t"] == "s")
                .collect::<Vec<_>>();
            assert_eq!(frames.len(), usize::from(callback % 2 == 0));
            for frame in frames {
                assert_eq!(frame["tick"], status.tick);
                assert_eq!(frame["winner"], 1);
                assert_eq!(frame["mode"]["kind"], "classic");
                assert_eq!(frame["end"]["reason"], "hq");
                assert_eq!(frame["out"], json!([true, false]));
                frame_counts[slot] += 1;
            }
            let lobbies = messages
                .iter()
                .filter(|message| message["t"] == "lobby")
                .collect::<Vec<_>>();
            if callback < 120 {
                assert!(lobbies.is_empty());
                assert!(room.hub.rooms["ABC"].native.is_some());
                assert!(room.hub.rooms["ABC"].result.is_none());
                continue;
            }
            assert_eq!(lobbies.len(), 1);
            let lobby = lobbies[0];
            assert_eq!(lobby["state"], "lobby");
            let result = &lobby["result"];
            let keys = result
                .as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect::<std::collections::BTreeSet<_>>();
            assert_eq!(
                keys,
                [
                    "winner", "teams", "names", "reason", "at", "endTick", "story", "timeline",
                    "winVp", "mode", "you"
                ]
                .into()
            );
            assert_eq!(result["winner"], 1);
            assert_eq!(result["teams"], json!([0, 1]));
            assert_eq!(result["names"], json!(["seat", "seat"]));
            assert_eq!(result["mode"], "classic");
            assert_eq!(result["reason"], "hq");
            assert_eq!(
                result["at"],
                json!({"x":finish.end_at.x,"z":finish.end_at.z})
            );
            assert_eq!(result["endTick"], finish.end_tick);
            assert!(result["winVp"].is_null());
            assert_eq!(
                result["you"],
                json!({"team":slot,"outcome":if slot == 0 {"defeat"} else {"victory"}})
            );
            assert_eq!(
                result["story"][1]["mpSpent"],
                (spent - jobs[1].charge.mp).round()
            );
            let timeline = result["timeline"].as_array().unwrap();
            assert!(!timeline.is_empty());
            for sample in timeline {
                let keys = sample
                    .as_object()
                    .unwrap()
                    .keys()
                    .map(String::as_str)
                    .collect::<std::collections::BTreeSet<_>>();
                assert_eq!(keys, ["t", "points", "structures"].into());
                assert!(sample["t"].is_number());
                assert_eq!(sample["points"], json!([0, 0]));
                assert_eq!(sample["structures"].as_array().unwrap().len(), 2);
            }
            assert_eq!(timeline.first().unwrap()["structures"], json!([1., 1.]));
            assert_eq!(timeline.last().unwrap()["structures"], json!([0, 1.]));
            assert!(
                messages
                    .iter()
                    .any(|message| message["t"] == "pause" && message["paused"] == false)
            );
        }
    }
    assert_eq!(frame_counts, [60, 60]);
    assert!(room.hub.rooms["ABC"].native.is_none());
    let complete = room.hub.native_status("ABC").unwrap();
    for _ in 0..3 {
        assert!(room.step().iter().all(Vec::is_empty));
        let after = room.hub.native_status("ABC").unwrap();
        assert_eq!(after.tick, complete.tick);
        assert_eq!(after.held, 120);
        assert!(after.ending_complete && after.fault.is_none());
    }
}

#[test]
fn net43_classic_attachment_keeps_mode_map_binding_and_refuses_node_rounding_overflow() {
    let mut state = scene(Mode::Classic);
    let settings = || crate::native_startup::StartupSettings {
        map: "default",
        mode: "classic",
        army: "standard",
        weather: "map",
        sandbox: false,
    };
    assert!(crate::native_startup::NativeStartup::prepare(&state, settings()).is_ok());
    state.options.map_key = None;
    assert!(crate::native_startup::NativeStartup::prepare(&state, settings()).is_err());
    state.options.map_key = Some("default".into());
    let mut wrong_mode = settings();
    wrong_mode.mode = "conquest";
    assert!(crate::native_startup::NativeStartup::prepare(&state, wrong_mode).is_err());
    state.elimination.as_mut().unwrap().nodes[0].at.x = f64::MAX;
    let before = format!(
        "{:?}",
        (&state.elimination, &state.production, &state.commands)
    );
    assert!(state.preflight_ordinary_snapshot().is_err());
    assert!(state.ordinary_snapshot_raw_lists().is_err());
    assert_eq!(
        format!(
            "{:?}",
            (&state.elimination, &state.production, &state.commands)
        ),
        before
    );
    state.options.mode = Mode::World;
    state.world.game.mode = Some(ww2_core::game::ModeKind::World);
    assert!(SnapshotCache::new(&state).is_err());
}
