use super::*;
use ww2_core::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    match_runtime::{MatchOptions, PlayerConfig},
    weather::WeatherChoice,
};

pub(crate) fn native_match() -> Match {
    crate::authoritative_maps::create_match(
        "default",
        MatchOptions {
            seed: 0x3c1055,
            mode: Mode::Conquest,
            players: (0..2)
                .map(|slot| PlayerConfig {
                    name: format!("Commander {slot}"),
                    team: slot,
                    faction: Faction::try_from(slot as u8).unwrap(),
                })
                .collect(),
            shuffle: false,
            army: ArmySize::Standard,
            weather: WeatherChoice::Clear,
            weather_seed: Some(1),
            living_weather: false,
            map_key: Some("default".into()),
            tech: true,
            supply: false,
            logistics: false,
            sandbox: false,
        },
    )
    .unwrap()
}
fn client(id: u64) -> (Connection, mpsc::Receiver<Output>) {
    let (tx, rx) = mpsc::channel(64);
    let (cancel, _) = watch::channel(false);
    (Connection { id, tx, cancel }, rx)
}
fn join(h: &mut Hub, c: &Connection, token: &str, now: u64) {
    h.message(
        "ABC",
        c,
        &mut false,
        json!({"t":"hello","token":token,"name":token}),
        now,
    );
}
fn room() -> (
    Hub,
    Connection,
    mpsc::Receiver<Output>,
    Connection,
    mpsc::Receiver<Output>,
) {
    let mut hub = Hub::new(32, String::new());
    let (a, mut ra) = client(1);
    let (b, mut rb) = client(2);
    join(&mut hub, &a, "a", 0);
    join(&mut hub, &b, "b", 0);
    while ra.try_recv().is_ok() {}
    while rb.try_recv().is_ok() {}
    (hub, a, ra, b, rb)
}
fn write(rx: &mut mpsc::Receiver<Output>) -> SnapshotWrite {
    loop {
        match rx.try_recv().unwrap() {
            Output::Snapshot(write) => return write,
            Output::Json(_) => {}
            Output::Close(_, _) => panic!("unexpected close"),
        }
    }
}
#[test]
fn native_attachment_admits_before_roster_or_account_publication() {
    let (mut h, _, _, _, _) = room();
    let mut invalid = native_match();
    invalid.world.game.units.values_mut().next().unwrap().kind = usize::MAX;
    assert!(h.attach_native("ABC", invalid, 0).is_err());
    assert!(h.rooms["ABC"].native.is_none());
    assert!(h.accounts.is_empty());
    assert_eq!(h.next_epoch, 0);
    let mut mismatch = native_match();
    mismatch.options.players[1].faction = Faction::Uk;
    assert!(h.attach_native("ABC", mismatch, 0).is_err());
    assert!(h.accounts.is_empty());
    assert_eq!(h.attach_native("ABC", native_match(), 0).unwrap(), 1);
    assert!(h.attach_native("ABC", native_match(), 0).is_err());
    assert_eq!(h.accounts.len(), 2);
    h.stop();
    let tick = h.rooms["ABC"]
        .native
        .as_ref()
        .unwrap()
        .runtime
        .state()
        .world
        .game
        .tick;
    h.poll_native(10_000, 10_000_000_000);
    assert_eq!(
        h.rooms["ABC"]
            .native
            .as_ref()
            .unwrap()
            .runtime
            .state()
            .world
            .game
            .tick,
        tick
    );
    assert!(h.native_status("ABC").is_err());
}
#[test]
fn attached_roster_reconnects_in_place_and_stale_writes_only_drain_old_accounts() {
    let (mut h, a, mut ra, _, mut rb) = room();
    h.attach_native("ABC", native_match(), 0).unwrap();
    for command in [
        json!({"t":"spectate"}),
        json!({"t":"addAi"}),
        json!({"t":"team","slot":1,"v":0}),
        json!({"t":"faction","slot":0,"v":3}),
    ] {
        h.message("ABC", &a, &mut true, command, 0);
    }
    let (newcomer, _) = client(3);
    join(&mut h, &newcomer, "new", 0);
    assert_eq!(h.rooms["ABC"].players.len(), 2);
    assert!(h.rooms["ABC"].spectators.is_empty());
    assert_eq!(h.rooms["ABC"].players[1].team, 1);
    assert_eq!(h.rooms["ABC"].players[0].faction, 0);
    h.poll_native(0, 0);
    h.poll_native(50, 50_000_000);
    let old = write(&mut ra);
    let other = write(&mut rb);
    assert!(h.accounts[&1].pressure.inflight() > 0);
    let (replacement, mut rr) = client(4);
    join(&mut h, &replacement, "a", 51);
    let new_history = h.accounts[&4].delivery.clone();
    h.complete_snapshot(&old, true, 52);
    assert_eq!(h.accounts[&1].pressure.inflight(), 0);
    assert_eq!(h.accounts[&4].delivery, new_history);
    assert_eq!(h.accounts[&4].pressure.inflight(), 0);
    assert!(h.accounts[&4].full_terrain);
    assert!(!matches!(rr.try_recv(), Ok(Output::Snapshot(_))));
    h.complete_snapshot(&other, true, 52);
    h.disconnected("ABC", 1, 53);
    assert_eq!(h.rooms["ABC"].players[0].connection.as_ref().unwrap().id, 4);
    h.disconnected("ABC", 4, 54);
    h.expire(10_054);
    assert_eq!(h.rooms["ABC"].players.len(), 2);
    assert!(h.rooms["ABC"].players[0].connection.is_none());
    h.retire_connection(1);
    assert!(!h.accounts.contains_key(&1));
}
#[test]
fn refused_snapshot_enqueue_and_terminal_drain_fault_only_originating_runtime() {
    let (mut h, a, mut ra, _, mut rb) = room();
    h.attach_native("ABC", native_match(), 0).unwrap();
    for _ in 0..64 {
        a.send(json!({"t":"pong"}));
    }
    h.poll_native(0, 0);
    h.poll_native(50, 50_000_000);
    assert!(h.native_status("ABC").unwrap().fault.is_some());
    assert_eq!(h.accounts[&1].pressure.inflight(), 0);
    while let Ok(output) = ra.try_recv() {
        assert!(!matches!(output, Output::Snapshot(_)));
    }
    while let Ok(output) = rb.try_recv() {
        if let Output::Snapshot(write) = output {
            h.complete_snapshot(&write, false, 51);
        }
    }
    assert_eq!(h.accounts[&2].pressure.inflight(), 0);

    let (mut h, _, mut ra, _, mut rb) = room();
    let epoch = h.attach_native("ABC", native_match(), 0).unwrap();
    h.poll_native(0, 0);
    h.poll_native(50, 50_000_000);
    let old = write(&mut ra);
    let other = write(&mut rb);
    h.rooms.remove("ABC");
    let (new, _) = client(9);
    let (new2, _) = client(10);
    join(&mut h, &new, "new", 60);
    join(&mut h, &new2, "new2", 60);
    assert!(h.attach_native("ABC", native_match(), 60).unwrap() > epoch);
    h.complete_snapshot(&old, false, 61);
    h.complete_snapshot(&other, false, 61);
    assert!(h.native_status("ABC").unwrap().fault.is_none());
    assert_eq!(h.accounts[&1].pressure.inflight(), 0);
    assert_eq!(h.accounts[&2].pressure.inflight(), 0);
    assert_eq!(h.accounts[&9].pressure.inflight(), 0);
}

#[test]
fn absent_and_unready_connections_are_distinct_and_expiry_precedes_callbacks() {
    let (mut h, _, mut ra, _, mut rb) = room();
    h.attach_native("ABC", native_match(), 0).unwrap();
    let before = h.accounts[&1].delivery.clone();
    ra.close(); rb.close();
    h.poll_native(0, 0);
    h.poll_native(50, 50_000_000);
    let native = &h.rooms["ABC"].native.as_ref().unwrap().runtime;
    assert_eq!(native.state().world.game.tick, 2);
    assert!(native.state().start.economy.players.iter().all(|player| !player.away));
    assert_eq!(h.accounts[&1].delivery, before);
    assert!(h.accounts[&1].full_terrain);
    assert_eq!(h.accounts[&1].pressure.inflight(), 0);
    // An expired native room must not run a pause callback or any due tick.
    h.rooms.get_mut("ABC").unwrap().empty_since = Some(0);
    h.poll_native(60_001, 60_001_000_000);
    assert!(!h.rooms.contains_key("ABC"));
    assert!(h.room_order.is_empty());
    assert_eq!(h.accounts[&1].delivery, before);
}
