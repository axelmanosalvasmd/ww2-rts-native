use super::*;
use crate::{
    economy::Faction,
    economy_state::ArmySize,
    game::Point,
    logistics::{LogisticsState, Reserves},
    map::AuthoritativeMap,
    match_runtime::{MatchOptions, PlayerConfig},
    supply_convoys::{ConvoyState, Event, Store, StoreId, TruckState},
    weather::WeatherChoice,
};

fn scene(enabled: bool) -> Match {
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"Logistics recipients", "w":64, "h":64,
            "rows":vec![".".repeat(64);64],
            "spawns":[{"x":8,"y":8},{"x":8,"y":54},{"x":54,"y":54}],
            "points":[{"x":32,"y":32}]
        }))
        .unwrap(),
    )
    .unwrap();
    Match::new(
        &map,
        MatchOptions {
            seed: 7302,
            mode: Mode::Conquest,
            players: (0..3)
                .map(|slot| PlayerConfig {
                    name: format!("P{slot}"),
                    team: if slot == 2 { 1 } else { 0 },
                    faction: Faction::try_from(slot as u8).unwrap(),
                })
                .collect(),
            shuffle: false,
            army: ArmySize::Standard,
            weather: WeatherChoice::Map,
            weather_seed: Some(1),
            living_weather: false,
            map_key: None,
            tech: false,
            supply: true,
            logistics: enabled,
            sandbox: false,
        },
    )
    .unwrap()
}
fn inventory(ammo: f64) -> Stock {
    Stock {
        ammo,
        provisions: ammo * 2.0,
        fuel: ammo * 4.0,
    }
}
fn reserve() -> Reserves {
    Reserves {
        ammo: 1.,
        ammo_max: 3.,
        ammo_price: 0.01,
        ammo_credit: Some(0.123),
        provisions: 12.346,
        fuel: Some(90.126),
        emergency: Some(6.785),
        warning: Some(1.111),
        forced: true,
        stranded: true,
        exhausted: true,
        supply_rate: Some(0.456),
        cut_for: Some(2.25),
    }
}

#[test]
fn intent_log44_exact_fields_rounding_and_private_allied_stock_in_complete_frames() {
    let mut state = scene(true);
    let ids = (0..3)
        .map(|slot| {
            state
                .world
                .game
                .units
                .values()
                .find(|u| u.owner == slot && state.logistics.get(u.id).is_some())
                .unwrap()
                .id
        })
        .collect::<Vec<_>>();
    let truck = state.test_supply_truck(0);
    let ally_truck = state.test_supply_truck(1);
    state.logistics = LogisticsState::default();
    for id in &ids {
        state.logistics.insert(*id, reserve());
    }
    let store = |id, owner, active| Store {
        id: StoreId::Cache(id),
        owner,
        at: Point {
            x: 12.346,
            z: 5.123,
        },
        source: false,
        entity: None,
        capacity: inventory(20.125),
        buckets: vec![
            (0, inventory(4.125)),
            (1, inventory(99.)),
            (2, inventory(999.)),
        ],
        donations: vec![
            (0, vec![(1, inventory(1.))]),
            (1, vec![(0, inventory(7.25))]),
        ],
        active,
        support: false,
        point: None,
    };
    let convoys = state.convoys.as_mut().unwrap();
    convoys.stores = vec![store(81, 1, true), store(82, 0, false), store(83, 2, true)];
    let c = convoys.trucks.get_mut(&truck).unwrap();
    c.state = TruckState::WaitingCollect;
    c.manual = true;
    c.hold = true;
    c.cargo = inventory(1.236);
    c.destination = Some(Point {
        x: 12.34567,
        z: 23.45678,
    });
    c.route = vec![Point { x: 99., z: 99. }];
    state.world.game.units.get_mut(&truck).unwrap().path = vec![Point { x: 9.876, z: 7.654 }];
    convoys.events = vec![
        Event {
            id: 1,
            owner: 0,
            kind: "stock".into(),
            count: 3,
            at: Some(Point {
                x: 1.23456,
                z: 4.56789,
            }),
            message: "No stock".into(),
            seconds: 77.123,
        },
        Event {
            id: 2,
            owner: 0,
            kind: "fleet".into(),
            count: 1,
            at: None,
            message: "Fleet".into(),
            seconds: 78.,
        },
        Event {
            id: 3,
            owner: 1,
            kind: "private".into(),
            count: 1,
            at: None,
            message: "Ally only".into(),
            seconds: 79.,
        },
    ];
    let before = state.convoys.clone();
    state.world.game.reveal = true;
    for p in &mut state.world.game.players {
        p.visible = state.world.game.units.keys().copied().collect();
    }
    let mut delivery = DeliveryState::new(&state).unwrap();
    let frame = delivery
        .capture_with_lists(&mut state, &[true; 3], &[false; 3], &[])
        .unwrap();
    let own = frame.recipient(0).unwrap().into_json();
    assert_eq!(
        own["logistics"],
        json!({
            "enabled": true,
            "units": [{"id":ids[0], "ammo":0.33, "provisions":12.35, "fuel":90.13,
                "emergency":6.79, "warning":1.11, "forced":true, "stranded":true,
                "supply":0.46, "cut":false, "grace":8.}],
            "trucks": [{"id":truck, "state":"waitingCollect", "manual":true, "hold":true,
                "cargo":{"ammo":1.24,"provisions":2.47,"fuel":4.94},
                "destination":{"x":12.34567,"z":23.45678}, "route":[{"x":9.88,"z":7.65}]}],
            "stores": [{"id":"cache:81","owner":1,"x":12.35,"z":5.12,"source":false,
                "stock":{"ammo":10.375,"provisions":20.75,"fuel":41.5},
                "capacity":{"ammo":20.125,"provisions":40.25,"fuel":80.5}}],
            "events": [{"id":1,"type":"stock","count":3,"x":1.23456,"z":4.56789,"message":"No stock"},
                {"id":2,"type":"fleet","count":1,"message":"Fleet"}]
        })
    );
    let ally = frame.recipient(1).unwrap().into_json();
    let enemy = frame.recipient(2).unwrap().into_json();
    assert_eq!(ally["logistics"]["units"][0]["id"], ids[1]);
    assert_eq!(ally["logistics"]["units"].as_array().unwrap().len(), 1);
    assert_eq!(ally["logistics"]["trucks"][0]["id"], ally_truck);
    assert_eq!(ally["logistics"]["trucks"].as_array().unwrap().len(), 1);
    assert_eq!(
        ally["logistics"]["events"],
        json!([{"id":3,"type":"private","count":1,"message":"Ally only"}])
    );
    assert_eq!(
        ally["logistics"]["stores"][0]["stock"],
        json!({"ammo":92.75,"provisions":185.5,"fuel":371.})
    );
    assert_eq!(enemy["logistics"]["units"][0]["id"], ids[2]);
    assert_eq!(enemy["logistics"]["units"].as_array().unwrap().len(), 1);
    assert!(
        enemy["units"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row[0] == truck && row[2] == 0)
    );
    assert_eq!(enemy["logistics"]["stores"][0]["id"], "cache:83");
    assert_eq!(enemy["logistics"]["stores"].as_array().unwrap().len(), 1);
    assert_eq!(
        enemy["logistics"]["stores"][0]["stock"],
        json!({"ammo":999.,"provisions":1998.,"fuel":3996.})
    );
    assert_eq!(enemy["logistics"]["trucks"], json!([]));
    assert_eq!(enemy["logistics"]["events"], json!([]));
    assert!(own["logistics"].get("regions").is_none());
    drop(frame);
    assert_eq!(
        state.convoys, before,
        "projection never mutates payer buckets"
    );
}

#[test]
fn intent_log44_nulls_grace_cut_boundaries_and_disabled_omission_match_prototype() {
    let mut state = scene(false);
    let mut delivery = DeliveryState::new(&state).unwrap();
    let frame = delivery
        .capture_with_lists(&mut state, &[true; 3], &[false; 3], &[])
        .unwrap();
    assert!(
        frame
            .recipient(0)
            .unwrap()
            .into_json()
            .get("logistics")
            .is_none()
    );
    drop(frame);
    state.convoys = ConvoyState::setup(true);
    let id = state
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 0)
        .unwrap()
        .id;
    let mut r = reserve();
    r.ammo_max = 0.;
    r.fuel = None;
    r.emergency = None;
    r.warning = None;
    for (cut_for, cut, supply, grace) in [
        (None, false, 0.46, Value::Null),
        (Some(0.), false, 0.46, Value::Null),
        (Some(9.99), false, 0.46, json!(1.)),
        (Some(10.), true, 0., Value::Null),
        (Some(11.), true, 0., Value::Null),
    ] {
        r.cut_for = cut_for;
        state.logistics.insert(id, r.clone());
        let view = project(&state, 0).unwrap().unwrap();
        let row = &view["units"][0];
        for name in ["ammo", "fuel", "emergency", "warning"] {
            assert_eq!(row[name], Value::Null);
        }
        assert_eq!(row["cut"], cut);
        assert_eq!(row["supply"], supply);
        assert_eq!(row["grace"], grace);
    }
    state.logistics.get_mut(id).unwrap().supply_rate = None;
    state.logistics.get_mut(id).unwrap().cut_for = None;
    assert_eq!(
        project(&state, 0).unwrap().unwrap()["units"][0]["supply"],
        0.
    );
    state.logistics.get_mut(id).unwrap().provisions = f64::NAN;
    assert_eq!(project(&state, 0), Err(SnapshotError::InvalidNumber));
    assert!(
        project(&state, 1).is_ok(),
        "another seat does not inspect hidden reserves"
    );
}

#[test]
fn logistics_cannot_be_requested_with_a_nonseat_recipient() {
    let mut state = scene(true);
    let count = state.world.game.players.len();
    for slot in [count, usize::MAX] {
        let cache = SnapshotCache::new(&state).unwrap();
        assert_eq!(cache.recipient(slot), Err(SnapshotError::InvalidRecipient));
        let mut delivery = DeliveryState::new(&state).unwrap();
        assert!(matches!(
            delivery.capture_selected_with_lists(
                &mut state,
                &[SelectedSeat {
                    slot,
                    full: true,
                    omit: false
                }],
                &[],
            ),
            Err(SnapshotError::InvalidRecipient)
        ));
    }
}
