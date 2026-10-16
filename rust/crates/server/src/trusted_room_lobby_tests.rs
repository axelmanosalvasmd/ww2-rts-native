use super::*;

fn room() -> (
    Hub,
    Connection,
    mpsc::Receiver<Output>,
    Connection,
    mpsc::Receiver<Output>,
) {
    let mut hub = Hub::new(32, String::new());
    hub.configure_trusted_room(Some(crate::TrustedRoomConfig::new("ABC", 3936341).unwrap()));
    let mut make = |id, token| {
        let (tx, rx) = mpsc::channel(64);
        let (cancel, _) = watch::channel(false);
        let c = Connection::new(id, tx, cancel);
        hub.message(
            "ABC",
            &c,
            &mut false,
            json!({"t":"hello","name":token,"token":token}),
            0,
        );
        (c, rx)
    };
    let (a, mut ra) = make(1, "a");
    let (b, mut rb) = make(2, "b");
    while ra.try_recv().is_ok() {}
    while rb.try_recv().is_ok() {}
    (hub, a, ra, b, rb)
}

type AccountSnapshot = (u64, u64, usize, SnapshotDelivery, SeatBackpressure, bool);
type RoomIdentity = (crate::NativeRoomStatus, String, Value, Vec<AccountSnapshot>);

fn identity(hub: &Hub) -> RoomIdentity {
    let native = hub.rooms["ABC"].native.as_ref().unwrap();
    let state = native.runtime.state();
    (
        hub.native_status("ABC").unwrap(),
        format!(
            "{:?}{:?}{:?}",
            state.world.game.units,
            state.commands.units,
            state
                .commands
                .pending_tick(state.world.game.tick as u64 + 1)
                .collect::<Vec<_>>()
        ),
        native.receipt.clone().unwrap(),
        hub.accounts
            .iter()
            .map(|(&id, a)| {
                (
                    id,
                    a.epoch,
                    a.slot,
                    a.delivery.clone(),
                    a.pressure.clone(),
                    a.full_terrain,
                )
            })
            .collect(),
    )
}

#[test]
fn trusted_rejected_and_faulted_restart_preserves_runtime_receipt_accounts_and_epoch() {
    let (mut hub, a, mut ra, _, mut rb) = room();
    assert_eq!(hub.start_trusted("ABC", a.id, false, 0).unwrap(), 1);
    while ra.try_recv().is_ok() {}
    while rb.try_recv().is_ok() {}
    hub.next_epoch = crate::native_startup::MAX_MATCH_ID;
    let before = identity(&hub);
    assert!(
        hub.start_trusted("ABC", a.id, true, 0)
            .unwrap_err()
            .contains("epoch exhausted")
    );
    assert_eq!(identity(&hub), before);
    assert!(ra.try_recv().is_err() && rb.try_recv().is_err());
    hub.next_epoch = 1;
    hub.rooms
        .get_mut("ABC")
        .unwrap()
        .native
        .as_mut()
        .unwrap()
        .runtime
        .fault_stop("internal deterministic latched-fault setup".into());
    let before = identity(&hub);
    assert!(
        hub.start_trusted("ABC", a.id, true, 0)
            .unwrap_err()
            .contains("faulted match restart")
    );
    hub.poll_native(1000, 1_000_000_000);
    assert_eq!(identity(&hub), before);
    assert_eq!(hub.next_epoch, 1);
    assert!(ra.try_recv().is_err() && rb.try_recv().is_err());
}

#[test]
fn trusted_restart_resets_input_and_delivery_but_keeps_old_socket_write_accounting() {
    let (mut hub, a, mut ra, _, mut rb) = room();
    hub.start_trusted("ABC", a.id, false, 0).unwrap();
    while ra.try_recv().is_ok() {}
    while rb.try_recv().is_ok() {}
    hub.poll_native(100, 100_000_000);
    let old = loop {
        if let Output::Snapshot(write) = ra.try_recv().unwrap() {
            break write;
        }
    };
    let pressure = hub.accounts[&a.id].pressure.clone();
    let owned = hub.rooms["ABC"]
        .native
        .as_ref()
        .unwrap()
        .runtime
        .state()
        .world
        .game
        .units
        .values()
        .find(|u| {
            u.owner == 0
                && !hub.rooms["ABC"]
                    .native
                    .as_ref()
                    .unwrap()
                    .runtime
                    .state()
                    .rules
                    .kinds[u.kind]
                    .structure
        })
        .unwrap()
        .id;
    hub.message("ABC", &a, &mut true, json!({"t":"stop","ids":[owned]}), 100);
    assert!(
        hub.rooms["ABC"]
            .native
            .as_ref()
            .unwrap()
            .runtime
            .state()
            .commands
            .pending_tick((hub.native_status("ABC").unwrap().tick + 1) as u64)
            .next()
            .is_some()
    );
    assert_eq!(hub.start_trusted("ABC", a.id, true, 100).unwrap(), 2);
    assert_eq!(hub.accounts[&a.id].pressure, pressure);
    let delivery = hub.accounts[&a.id].delivery.clone();
    hub.complete_snapshot(&old, true, 100);
    assert_eq!(hub.accounts[&a.id].epoch, 2);
    assert_eq!(hub.accounts[&a.id].delivery, delivery);
    let state = hub.rooms["ABC"].native.as_ref().unwrap().runtime.state();
    assert_eq!(state.world.game.tick, 0);
    assert!(state.commands.pending_tick(1).next().is_none());
}

#[test]
fn trusted_canonical_map_weather_matrix_records_options_before_publication() {
    let maps: Value = serde_json::from_str(include_str!("map-catalog.json")).unwrap();
    let mut admitted = 0;
    let mut refused = Vec::new();
    for map in maps.as_object().unwrap().keys() {
        for weather in ["map", "clear", "fog", "rain", "mud", "snow", "random"] {
            let (mut hub, a, _ra, _b, _rb) = room();
            let room = hub.rooms.get_mut("ABC").unwrap();
            room.map = map.clone();
            room.weather = weather.into();
            let result = hub.start_trusted("ABC", a.id, false, 0);
            if let Err(error) = result {
                assert_eq!(
                    error, "native startup authoring map binding changed",
                    "{map}/{weather}"
                );
                assert!(hub.rooms["ABC"].native.is_none() && hub.accounts.is_empty());
                assert_eq!(hub.next_epoch, 0);
                refused.push(format!("{map}/{weather}: {error}"));
                continue;
            }
            let native = hub.rooms["ABC"].native.as_ref().unwrap();
            let state = native.runtime.state();
            assert!(
                !state.options.logistics
                    && state.convoys.is_none()
                    && state.logistics.iter().next().is_none()
            );
            assert!(
                state.options.shuffle
                    && state.options.tech
                    && state.options.supply
                    && state.options.living_weather
            );
            assert_eq!(state.options.weather_seed, None);
            assert_eq!(
                native.receipt.as_ref().unwrap()["derivedWeatherSeed"],
                state.start.weather_seed
            );
            admitted += 1;
        }
    }
    eprintln!(
        "R1_CANONICAL_MATRIX admitted={admitted} refused={} gaps={refused:?}",
        refused.len()
    );
    assert_eq!(admitted + refused.len(), 231);
    assert_eq!(admitted, 224);
    assert!(refused.iter().all(|row| row.starts_with("no-mans-land/")));
    assert_eq!(refused.len(), 7);
}
