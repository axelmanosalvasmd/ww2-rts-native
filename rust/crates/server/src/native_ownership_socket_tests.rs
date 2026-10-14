//! Foreign socket selections preserve the canonical unit and command controls.
use super::*;
use ww2_core::match_commands::UnitControl;

#[derive(Debug, PartialEq)]
struct CommandState {
    id: u32,
    owner: usize,
    kind: usize,
    at: Point,
    rot: f64,
    hp: f64,
    path: Vec<Point>,
    path_gen: u64,
    world_goal: Option<Point>,
    amove: Option<Point>,
    reverse: Option<Point>,
    // Missing Source pace is NaN; retain its exact bits when comparing unchanged state.
    pace_bits: u64,
    attack: bool,
    retreating: bool,
    hold_pos: bool,
    build: bool,
    dig: bool,
    entrench: bool,
    board: u32,
    drift: bool,
    control: UnitControl,
}
impl CommandState {
    fn capture(state: &Match, id: u32) -> Self {
        let unit = &state.world.game.units[&id];
        let mut control = state.commands.units[&id].clone();
        // The reaction timer advances without an order. Compare every other control field.
        control.react = 0.;
        Self {
            id: unit.id,
            owner: unit.owner,
            kind: unit.kind,
            at: unit.at(),
            rot: unit.rot,
            hp: unit.hp,
            path: unit.path.clone(),
            path_gen: unit.path_gen,
            world_goal: unit.world_goal,
            amove: unit.amove,
            reverse: unit.reverse,
            pace_bits: unit.pace.to_bits(),
            attack: unit.attack,
            retreating: unit.retreating,
            hold_pos: unit.hold_pos,
            build: unit.build,
            dig: unit.dig,
            entrench: unit.entrench,
            board: unit.board,
            drift: unit.drift,
            control,
        }
    }
}

async fn inspect_command_state(control: &NativeRoomControl, id: u32) -> (i64, CommandState) {
    let hub = control.server.hub.lock().await;
    let native = hub.rooms["ABC"].native.as_ref().unwrap();
    assert!(native.runtime.fault().is_none());
    let state = native.runtime.state();
    assert_eq!(state.commands.pending_commands(), 0);
    assert!(state.commands.units[&id].orders.is_empty());
    (state.world.game.tick, CommandState::capture(state, id))
}

fn decoded_command_state(view: &ClientView, id: u32) -> Value {
    let unit = view.unit(id).unwrap();
    // Ignore timed ability/camouflage flags and cooldowns, retaining command flags and pose.
    let command_flags = 1 | 16 | 32 | 64 | 1024 | 2048 | 4096 | 8192 | 16384;
    json!([
        unit.id,
        unit.kind,
        unit.owner,
        unit.x,
        unit.z,
        unit.rot,
        unit.aim,
        unit.hp,
        unit.target,
        unit.flags & command_flags
    ])
}

fn decoded_movement(view: &ClientView, id: u32) -> Option<Value> {
    view.state()["movement"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row[0] == id)
        .cloned()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn foreign_socket_orders_preserve_control_in_both_seats_and_mixed_selections() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (stop, stopped) = oneshot::channel();
    let (ready, receipt) = oneshot::channel();
    let server = tokio::spawn(serve_with_native_control(
        listener,
        Config::default(),
        async move {
            let _ = stopped.await;
        },
        Some(ready),
    ));
    let control = receipt.await.unwrap();
    let host = tokio::runtime::Handle::current();
    tokio::task::spawn_blocking(move || {
        let mut clients = [
            Receiver::connect(address, "a"),
            Receiver::connect(address, "b"),
        ];
        let mut state = setup("default");
        // Declared quiet initial controls. All later mutations enter through the sockets.
        for unit in state.world.game.units.values_mut() {
            unit.hold_pos = true;
        }
        for control in state.commands.units.values_mut() {
            control.hold_fire = true;
            control.auto_retreat = false;
            control.auto_cast = false;
        }
        let ids: [u32; 2] = std::array::from_fn(|slot| {
            state
                .world
                .game
                .units
                .values()
                .find(|unit| unit.owner == slot && state.rules.kinds[unit.kind].name == "rifle")
                .unwrap()
                .id
        });
        let goals = ids.map(|id| {
            let origin = state.world.game.units[&id].at();
            Point {
                x: ww2_core::math::round(origin.x),
                z: ww2_core::math::round(origin.z) + 12.,
            }
        });
        let epoch = host.block_on(control.attach("ABC", state)).unwrap();
        for (slot, client) in clients.iter_mut().enumerate() {
            let start = client.until(|message| message["t"] == "start");
            assert_eq!(start["matchId"], epoch);
            assert_eq!(start["you"], slot);
            client.until(|message| message["t"] == "s");
            assert_ne!(client.view.unit(ids[slot]).unwrap().flags & 2048, 0);
        }

        for seat in 0..2 {
            let other = 1 - seat;
            let foreign = ids[other];
            for (name, reason) in [
                ("move", "blocked"),
                ("amove", "blocked"),
                ("stop", "needs"),
                ("stance", "needs"),
            ] {
                let (before_tick, before) = host.block_on(inspect_command_state(&control, foreign));
                clients[other].until(|message| {
                    message["t"] == "s" && message["tick"].as_i64().unwrap() >= before_tick
                });
                let decoded_before = decoded_command_state(&clients[other].view, foreign);
                let movement_before = decoded_movement(&clients[other].view, foreign);
                let mut command = match name {
                    "move" | "amove" => json!({
                        "t":name,"orders":[[foreign,goals[other].x,goals[other].z]]
                    }),
                    "stop" => json!({"t":name,"ids":[foreign]}),
                    "stance" => {
                        json!({"t":name,"ids":[foreign],"key":"holdFire","on":false})
                    }
                    _ => unreachable!(),
                };
                for (key, value) in [
                    ("slot", other),
                    ("issuer", other),
                    ("tick", 999),
                    ("sequence", 0),
                ] {
                    command[key] = json!(value);
                }
                clients[seat].send(command);
                let denial = clients[seat].until(|message| message["t"] == "deny");
                assert_eq!(denial, json!({"t":"deny","cmd":name,"reason":reason}));
                assert_eq!(clients[seat].view.last_denial(), Some(&denial));
                let (processed_tick, after) =
                    host.block_on(inspect_command_state(&control, foreign));
                assert!(processed_tick > before_tick);
                assert_eq!(
                    after, before,
                    "seat {seat} foreign {name} changed canonical control"
                );
                clients[other].until(|message| {
                    message["t"] == "s" && message["tick"].as_i64().unwrap() >= processed_tick
                });
                assert_eq!(
                    decoded_command_state(&clients[other].view, foreign),
                    decoded_before
                );
                assert_eq!(
                    decoded_movement(&clients[other].view, foreign),
                    movement_before
                );
                let (_, later) = host.block_on(inspect_command_state(&control, foreign));
                assert_eq!(later, before, "foreign {name} left a later mutation");
            }
        }

        for seat in 0..2 {
            let other = 1 - seat;
            let owned = ids[seat];
            let foreign = ids[other];
            let (before_tick, before) = host.block_on(inspect_command_state(&control, foreign));
            clients[other].until(|message| {
                message["t"] == "s" && message["tick"].as_i64().unwrap() >= before_tick
            });
            let decoded_before = decoded_command_state(&clients[other].view, foreign);
            let movement_before = decoded_movement(&clients[other].view, foreign);
            clients[seat].send(json!({
                "t":"stance","ids":[foreign,owned],"key":"holdFire","on":false,
                "slot":other,"issuer":other,"tick":999,"sequence":0
            }));
            clients[seat].until(|message| {
                message["t"] == "s"
                    && message["units"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|row| row[0] == owned && row[12].as_u64().unwrap() & 2048 == 0)
            });
            assert_eq!(clients[seat].view.unit(owned).unwrap().flags & 2048, 0);
            let (_, owned_after) = host.block_on(inspect_command_state(&control, owned));
            assert_eq!(owned_after.owner, seat);
            assert!(!owned_after.control.hold_fire);
            let (processed_tick, after) = host.block_on(inspect_command_state(&control, foreign));
            assert_eq!(after, before, "mixed selection changed the foreign control");
            clients[other].until(|message| {
                message["t"] == "s" && message["tick"].as_i64().unwrap() >= processed_tick
            });
            assert_eq!(
                decoded_command_state(&clients[other].view, foreign),
                decoded_before
            );
            assert_eq!(
                decoded_movement(&clients[other].view, foreign),
                movement_before
            );
            let (_, later) = host.block_on(inspect_command_state(&control, foreign));
            assert_eq!(later, before);
            // Restore through the owning socket so the reverse mixed selection also changes a flag.
            clients[seat].send(json!({"t":"stance","ids":[owned],"key":"holdFire","on":true}));
            clients[seat].until(|message| {
                message["t"] == "s"
                    && message["units"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|row| row[0] == owned && row[12].as_u64().unwrap() & 2048 != 0)
            });
            let (_, restored) = host.block_on(inspect_command_state(&control, owned));
            assert!(restored.control.hold_fire);
        }
        let status = host.block_on(control.inspect("ABC")).unwrap();
        assert_eq!(status.epoch, epoch);
        assert!(status.tick > 0 && status.fault.is_none());
        assert!(clients.iter().all(|client| !client.view.needs_resync()));
    })
    .await
    .unwrap();
    let _ = stop.send(());
    server.await.unwrap().unwrap();
}
