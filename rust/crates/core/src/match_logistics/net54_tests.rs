//! Owner lists in running ordinary recipient frames, including physical supply.
use super::*;
use crate::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    map::AuthoritativeMap,
    match_commands::{Command, MoveOrder},
    match_runtime::{MatchOptions, PlayerConfig},
    recipient_snapshot::{DeliveryState, SelectedSeat, SnapshotCache},
    weather::WeatherChoice,
};
use serde_json::{Value, json};

fn scene(mode: Mode, logistics: bool) -> Match {
    let map = AuthoritativeMap::from_value(json!({
        "name":"Owner-only ordinary lists", "w":96,"h":96,
        "rows":vec![".".repeat(96);96],
        "spawns":[{"x":10,"y":10},{"x":10,"y":80},{"x":80,"y":80}],
        "defend":[0],"points":[{"x":48,"y":48,"vp":0,"mp":1},{"x":60,"y":60,"vp":0,"mp":1}]
    }))
    .unwrap();
    let options = MatchOptions {
        seed: 5401,
        mode,
        players: (0..3)
            .map(|slot| PlayerConfig {
                name: format!("P{slot}"),
                team: if slot == 2 { 1 } else { 0 },
                faction: Faction::Usa,
            })
            .collect(),
        shuffle: false,
        army: ArmySize::Standard,
        weather: WeatherChoice::Map,
        weather_seed: Some(1),
        living_weather: false,
        map_key: None,
        tech: true,
        supply: false,
        logistics,
        sandbox: false,
    };
    let mut m = match mode {
        Mode::Classic => Match::new_classic(&map, options),
        Mode::Annihilation => Match::new_annihilation(&map, options),
        Mode::Assault => Match::new_assault(&map, options, 0),
        Mode::Tutorial => Match::new_tutorial(&map, options),
        Mode::Horde => Match::new_horde(&map, options),
        _ => Match::new(&map, options),
    }
    .unwrap();
    for c in m.commands.units.values_mut() {
        c.hold_fire = true;
        c.auto_cast = false;
        c.auto_retreat = false;
    }
    m
}
fn tick(m: &mut Match, delivery: &mut DeliveryState) -> Vec<Value> {
    let report = m.advance_tick().unwrap();
    if m.convoys.is_some() {
        use crate::match_tick::TickPhase;
        assert_eq!(
            report
                .phases
                .iter()
                .filter(|p| **p == TickPhase::UnitReserves)
                .count(),
            1
        );
        let beat = crate::supply_network::SupplyRules::compiled()
            .unwrap()
            .network_ticks;
        assert_eq!(
            report.phases.contains(&TickPhase::SupplyNetwork),
            beat != 0 && (report.tick as u64).is_multiple_of(beat)
        );
        assert_eq!(
            report
                .phases
                .iter()
                .filter(|p| **p == TickPhase::Convoys)
                .count(),
            1
        );
        assert!(report.unavailable.iter().all(|p| !matches!(
            p,
            TickPhase::UnitReserves | TickPhase::SupplyNetwork | TickPhase::Convoys
        )));
    }
    let selected = (0..m.start.players.len())
        .map(|slot| SelectedSeat {
            slot,
            full: false,
            omit: false,
        })
        .collect::<Vec<_>>();
    let frame = delivery
        .capture_selected_with_tick_events(m, &selected, &report.events)
        .unwrap()
        .unwrap();
    let rows = selected
        .iter()
        .map(|s| frame.recipient(s.slot).unwrap().into_json())
        .collect();
    assert!(m.tick_state.fault().is_none());
    rows
}
fn own_ids(m: &Match, slot: usize, rows: &Value) {
    for row in rows.as_array().unwrap() {
        let id = row[0].as_u64().unwrap() as u32;
        assert_eq!(m.world.game.units[&id].owner, slot);
    }
}
#[test]
fn net54_running_ordinary_modes_keep_populated_lists_and_wallets_with_the_owner() {
    for mode in [
        Mode::Default,
        Mode::Conquest,
        Mode::Classic,
        Mode::Annihilation,
        Mode::Assault,
        Mode::Tutorial,
        Mode::Horde,
    ] {
        let mut m = scene(mode, false);
        let slots = m.start.players.len();
        let mut actors = Vec::new();
        for slot in 0..slots {
            m.start.economy.players[slot].mp = 1000.75 + slot as f64 * 100.0;
            for value in m.start.players[slot].support.values_mut() {
                *value = 10.25 + slot as f64 * 10.0;
            }
            m.player_rallies[slot] = Some(Point {
                x: 30.25 + slot as f64,
                z: 35.75,
            });
            let Some(id) = m
                .world
                .game
                .units
                .values()
                .find(|u| u.owner == slot && !m.rules.kinds[u.kind].structure && u.air.is_none())
                .map(|u| u.id)
            else {
                actors.push(None);
                continue;
            };
            let at = m.world.game.units[&id].at();
            for (distance, queue) in [(12.0, false), (20.0, true)] {
                assert_eq!(
                    m.execute_command(
                        slot,
                        &Command::Move {
                            orders: vec![MoveOrder {
                                unit: i64::from(id),
                                at: Point {
                                    x: at.x + distance,
                                    z: at.z
                                }
                            }],
                            control_ids: None,
                            attack_move: false,
                            queue,
                            together: false,
                            face: None,
                        },
                        false
                    )
                    .unwrap(),
                    None,
                    "{mode:?} slot {slot}"
                );
            }
            actors.push(Some(id));
        }
        // Plane rows have a different owner from the first ground actor.
        let plane = m.spawn_aircraft(1, "fighter").unwrap();
        let mut delivery = DeliveryState::new(&m).unwrap();
        for _ in 0..4 {
            let rows = tick(&mut m, &mut delivery);
            for (slot, row) in rows.iter().enumerate() {
                for key in ["plans", "orders", "movement", "productionJobs", "air"] {
                    own_ids(&m, slot, &row[key]);
                }
                assert_eq!(row["mp"], m.start.economy.players[slot].mp.floor());
                assert_eq!(
                    row["inc"],
                    math::round(m.start.economy.players[slot].inc * 10.) / 10.
                );
                assert_eq!(
                    row["rally"],
                    json!([math::round((30.25 + slot as f64) * 10.) / 10., 35.8])
                );
                assert_eq!(
                    row["sup"]["recon"],
                    m.start.players[slot].support["recon"].ceil()
                );
                assert_eq!(row["air"].as_array().unwrap().len(), usize::from(slot == 1));
                if slot == 1 {
                    assert_eq!(row["air"][0][0], plane);
                }
                if let Some(id) = actors[slot] {
                    assert!(row["plans"].as_array().unwrap().iter().any(|r| r[0] == id));
                    assert!(row["orders"].as_array().unwrap().iter().any(|r| r[0] == id));
                    assert!(
                        row["movement"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .any(|r| r[0] == id)
                    );
                }
                if mode == Mode::Classic {
                    assert_eq!(row["mun"], m.start.economy.players[slot].mun.floor());
                    assert_eq!(row["fuel"], m.start.economy.players[slot].fuel.floor());
                    assert!(row.get("fuelInc").is_some());
                    assert!(row.get("upkeep").is_some());
                }
            }
        }
    }
}
#[test]
fn net54_physical_supply_survives_ticks_with_private_reserves_trucks_stores_and_alerts() {
    for mode in [
        Mode::Default,
        Mode::Conquest,
        Mode::Classic,
        Mode::Annihilation,
    ] {
        let mut m = scene(mode, true);
        let mut trucks = Vec::new();
        for slot in 0..3 {
            let truck = m.test_supply_truck(slot);
            // An idle surplus fleet member is deliberately retired at its source.
            // A real manual hold keeps this selected truck active for privacy reads.
            assert_eq!(
                m.execute_command(
                    slot,
                    &Command::Stop {
                        ids: vec![i64::from(truck)],
                    },
                    false
                )
                .unwrap(),
                None
            );
            assert_eq!(
                m.convoys.as_ref().unwrap().trucks[&truck].state,
                crate::supply_convoys::TruckState::Held
            );
            trucks.push(truck);
            m.convoys
                .as_mut()
                .unwrap()
                .events
                .push(supply_convoys::Event {
                    id: slot as u64,
                    owner: slot,
                    kind: "blocked".into(),
                    count: slot + 1,
                    at: Some(Point {
                        x: 20. + slot as f64,
                        z: 25.,
                    }),
                    message: format!("owner {slot}"),
                    seconds: 0.,
                });
        }
        let original_stores = m.convoys.as_ref().unwrap().stores.clone();
        {
            let cache = SnapshotCache::new(&m).unwrap();
            let a = cache.recipient(0).unwrap();
            cache.recipient(1).unwrap();
            assert_eq!(a, cache.recipient(0).unwrap());
        }
        assert_eq!(m.convoys.as_ref().unwrap().stores, original_stores);
        let mut delivery = DeliveryState::new(&m).unwrap();
        for _ in 0..24 {
            let rows = tick(&mut m, &mut delivery);
            for (slot, row) in rows.iter().enumerate() {
                let supply = &row["logistics"];
                assert_eq!(supply["enabled"], true);
                assert!(supply.get("regions").is_none());
                assert!(!supply["units"].as_array().unwrap().is_empty());
                for reserve in supply["units"].as_array().unwrap() {
                    let id = reserve["id"].as_u64().unwrap() as u32;
                    assert_eq!(m.world.game.units[&id].owner, slot);
                    assert_eq!(reserve.as_object().unwrap().len(), 11);
                }
                assert!(
                    supply["trucks"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|r| r["id"] == trucks[slot])
                );
                for truck in supply["trucks"].as_array().unwrap() {
                    assert_eq!(
                        m.world.game.units[&(truck["id"].as_u64().unwrap() as u32)].owner,
                        slot
                    );
                    assert!(truck["cargo"].get("provisions").is_some());
                    assert!(truck["route"].is_array());
                }
                let stores = supply["stores"].as_array().unwrap();
                assert!(!stores.is_empty());
                for store in stores {
                    let owner = store["owner"].as_u64().unwrap() as usize;
                    assert!(m.world.game.allied(owner, slot));
                }
                let alerts = supply["events"].as_array().unwrap();
                assert!(
                    alerts
                        .iter()
                        .any(|e| e["message"] == format!("owner {slot}"))
                );
                for e in alerts {
                    assert!(e.get("owner").is_none());
                    assert!(e.get("at").is_none());
                    assert!(e.get("seconds").is_none());
                    assert_eq!(e["type"], "blocked");
                    assert_eq!(e["message"], format!("owner {slot}"));
                }
            }
        }
        // Retirement removes reserve and convoy links before the next recipient read.
        m.world.game.units.get_mut(&trucks[0]).unwrap().hp = 0.;
        let rows = tick(&mut m, &mut delivery);
        assert!(!m.world.game.units.contains_key(&trucks[0]));
        assert!(!m.convoys.as_ref().unwrap().trucks.contains_key(&trucks[0]));
        assert!(
            rows[0]["logistics"]["trucks"]
                .as_array()
                .unwrap()
                .iter()
                .all(|r| r["id"] != trucks[0])
        );
        // Each mode's actual victory actuator keeps every seat serializable after finish.
        if matches!(mode, Mode::Default | Mode::Conquest) {
            m.start.economy.players[0].vp = m.start.win_vp;
        } else {
            // Classic requires production buildings; Annihilation requires bunkers.
            // Bunkers are structures without the building flag at the prototype pin.
            for u in m.world.game.units.values_mut() {
                let kind = &m.rules.kinds[u.kind];
                let required = if mode == Mode::Annihilation {
                    kind.name == "bunker"
                } else {
                    kind.building
                };
                if u.owner == 2 && required {
                    u.hp = 0.;
                }
            }
        }
        tick(&mut m, &mut delivery);
        assert!(
            m.finish.is_some(),
            "{mode:?} must finish after its opposing objective is retired"
        );
        let reason = if mode == Mode::Annihilation {
            crate::match_end::EndReason::Bunkers
        } else if mode == Mode::Classic {
            crate::match_end::EndReason::Headquarters
        } else {
            crate::match_end::EndReason::VictoryPoints
        };
        assert_eq!(m.finish.unwrap().reason, reason, "{mode:?}");
        let frozen = m.convoys.clone();
        for _ in 0..4 {
            let rows = tick(&mut m, &mut delivery);
            assert!(rows.iter().all(|row| row["winner"] == 0));
            assert_eq!(m.convoys, frozen);
        }
    }
}
#[test]
fn net54_inconsistent_logistics_is_refused_before_clock_or_delivery_changes() {
    let mut m = scene(Mode::Default, false);
    let mut delivery = DeliveryState::new(&m).unwrap();
    let before = delivery.clone();
    m.options.logistics = true;
    let tick = m.world.game.tick;
    let units = format!("{:?}", m.world.game.units);
    assert!(m.advance_tick().is_err());
    assert_eq!(m.world.game.tick, tick);
    assert_eq!(format!("{:?}", m.world.game.units), units);
    assert!(
        delivery
            .capture_selected(
                &mut m,
                &[SelectedSeat {
                    slot: 0,
                    full: false,
                    omit: false
                }]
            )
            .is_err()
    );
    assert_eq!(delivery, before);
    let mut m = scene(Mode::Conquest, true);
    let mut delivery = DeliveryState::new(&m).unwrap();
    let before = delivery.clone();
    let id = m.logistics.iter().next().unwrap().0;
    m.logistics.remove(id);
    assert!(m.advance_tick().is_err());
    assert_eq!(m.world.game.tick, 0);
    assert!(
        delivery
            .capture_selected(
                &mut m,
                &[SelectedSeat {
                    slot: 0,
                    full: false,
                    omit: false
                }]
            )
            .is_err()
    );
    assert_eq!(delivery, before);
}

#[test]
fn net54_paid_classic_jobs_refunds_progress_and_research_stay_private() {
    let mut m = scene(Mode::Classic, false);
    let buildings = (0..3)
        .map(|slot| {
            m.start.economy.players[slot].mp = 5000.0;
            m.start.economy.players[slot].mun = 500.0 + slot as f64;
            m.start.economy.players[slot].fuel = 300.0 + slot as f64;
            let id = m.next_unit_id;
            m.place_starting_building(slot, "hq", (25 + slot * 12) * m.world.game.w + 30)
                .unwrap();
            for kind in ["rifle", "engineer"] {
                assert_eq!(
                    m.execute_command(
                        slot,
                        &Command::Buy {
                            kind: kind.into(),
                            from: Some(id)
                        },
                        false
                    )
                    .unwrap(),
                    None
                );
            }
            id
        })
        .collect::<Vec<_>>();
    m.request_research(0, crate::economy::Technology::Tier)
        .unwrap();
    let mut delivery = DeliveryState::new(&m).unwrap();
    for _ in 0..4 {
        let rows = tick(&mut m, &mut delivery);
        for (slot, row) in rows.iter().enumerate() {
            own_ids(&m, slot, &row["queues"]);
            own_ids(&m, slot, &row["productionJobs"]);
            let queue = row["queues"]
                .as_array()
                .unwrap()
                .iter()
                .find(|r| r[0] == buildings[slot])
                .unwrap();
            assert_eq!(queue[4], "rifle");
            assert_eq!(queue[5], "engineer");
            let building = m.production.building(buildings[slot]).unwrap();
            let catalog = Catalog::compiled().unwrap();
            let train = catalog
                .unit("rifle")
                .unwrap()
                .get("train")
                .unwrap()
                .number()
                .unwrap();
            assert_eq!(
                queue[1],
                math::round(building.progress() / train * 10.) / 10.
            );
            let jobs = row["productionJobs"]
                .as_array()
                .unwrap()
                .iter()
                .find(|r| r[0] == buildings[slot])
                .unwrap();
            let canonical = m.production.building(buildings[slot]).unwrap().jobs();
            assert_eq!(jobs[1].as_array().unwrap().len(), 2);
            for (i, job) in canonical.iter().enumerate() {
                assert_eq!(
                    jobs[1][i],
                    json!({"id":job.id,"type":job.kind,"mp":job.charge.mp,
                    "fuel":job.charge.fuel,"status":if i==0 {"active"} else {"waiting"}})
                );
            }
            assert_eq!(
                row["tech"]["lab"].as_array().unwrap().len(),
                usize::from(slot == 0)
            );
        }
    }
    let own_job = m.production.building(buildings[0]).unwrap().jobs()[1].id;
    let other_job = m.production.building(buildings[1]).unwrap().jobs()[1].id;
    assert_ne!(own_job, other_job);
    let wallet = m.start.economy.players[0].mp;
    let refund = m.production.building(buildings[0]).unwrap().jobs()[1]
        .charge
        .mp;
    assert!(m.cancel_training(1, buildings[0], own_job).is_err());
    m.cancel_training(0, buildings[0], own_job).unwrap();
    assert_eq!(m.start.economy.players[0].mp, wallet + refund);
    let rows = tick(&mut m, &mut delivery);
    let own = rows[0]["productionJobs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r[0] == buildings[0])
        .unwrap();
    assert_eq!(own[1].as_array().unwrap().len(), 1);
    assert_eq!(
        m.production.building(buildings[1]).unwrap().jobs()[1].id,
        other_job
    );
}

#[test]
fn net54_exact_logistics_json_matches_prototype_rounding_nulls_grace_and_private_stock() {
    // Source inspected: supply-convoys.js:646-661 rounds reserves/cargo/route,
    // retains raw destination/stock/capacity, and strips event owner plus at.
    use crate::supply_convoys::{Event, Store, StoreId, TruckState};
    let mut m = scene(Mode::Conquest, true);
    let own = m
        .world
        .game
        .units
        .values()
        .filter(|u| u.owner == 0 && !m.rules.kinds[u.kind].structure)
        .take(2)
        .map(|u| u.id)
        .collect::<Vec<_>>();
    let ally = m
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 1 && !m.rules.kinds[u.kind].structure)
        .unwrap()
        .id;
    let mut reserve = m.logistics.get(own[0]).unwrap().clone();
    m.logistics = logistics::LogisticsState::default();
    reserve.ammo = 2.;
    reserve.ammo_max = 3.;
    reserve.provisions = 12.345;
    reserve.fuel = Some(4.567);
    reserve.emergency = None;
    reserve.warning = Some(1.234);
    reserve.forced = false;
    reserve.stranded = true;
    reserve.supply_rate = Some(0.456);
    reserve.cut_for = Some(2.25);
    m.logistics.insert(own[0], reserve.clone());
    reserve.ammo = 0.;
    reserve.ammo_max = 0.;
    reserve.provisions = 1.236;
    reserve.fuel = None;
    reserve.emergency = Some(2.345);
    reserve.warning = None;
    reserve.forced = true;
    reserve.stranded = false;
    reserve.cut_for = Some(10.);
    m.logistics.insert(own[1], reserve.clone());
    m.logistics.insert(ally, reserve);
    let truck = m.test_supply_truck(0);
    let ally_truck = m.test_supply_truck(1);
    // A small trusted serializer fixture retains every eligible unit's reserve.
    // Running ordinary rosters are exercised separately above.
    m.world
        .game
        .units
        .retain(|id, _| own.contains(id) || *id == ally || *id == truck || *id == ally_truck);
    m.grid.rebuild(&m.world.game.units);
    {
        let u = m.world.game.units.get_mut(&truck).unwrap();
        u.path = vec![
            Point {
                x: 21.234,
                z: 22.786,
            },
            Point {
                x: 30.126,
                z: 31.234,
            },
        ];
    }
    let state = m.convoys.as_mut().unwrap();
    state.trucks.get_mut(&truck).unwrap().state = TruckState::Delivering;
    state.trucks.get_mut(&truck).unwrap().manual = true;
    state.trucks.get_mut(&truck).unwrap().hold = false;
    state.trucks.get_mut(&truck).unwrap().cargo = logistics::Stock {
        ammo: 1.234,
        provisions: 2.346,
        fuel: 3.456,
    };
    state.trucks.get_mut(&truck).unwrap().destination = Some(Point {
        x: 41.2345,
        z: 42.3456,
    });
    let quantity = |n| logistics::Stock {
        ammo: n,
        provisions: n + 1.,
        fuel: n + 2.,
    };
    let store = |id, owner, active, buckets, donations| Store {
        id,
        owner,
        at: Point {
            x: 10.234,
            z: 11.786,
        },
        source: owner == 0,
        entity: None,
        capacity: quantity(300.125),
        buckets,
        donations,
        active,
        support: false,
        point: None,
    };
    state.stores = vec![
        store(
            StoreId::Source(0),
            0,
            true,
            vec![
                (0, quantity(100.125)),
                (1, quantity(40.125)),
                (2, quantity(777.)),
            ],
            vec![(0, vec![(1, quantity(10.0625))])],
        ),
        store(
            StoreId::Hospital(7),
            1,
            true,
            vec![(0, quantity(7.125)), (1, quantity(888.))],
            vec![],
        ),
        store(
            StoreId::Source(2),
            2,
            true,
            vec![(0, quantity(999.))],
            vec![],
        ),
        store(
            StoreId::Cache(999),
            0,
            false,
            vec![(0, quantity(999.))],
            vec![],
        ),
    ];
    state.events = vec![
        Event {
            id: 7,
            owner: 0,
            kind: "blocked".into(),
            count: 2,
            at: Some(Point {
                x: 55.2345,
                z: 56.3456,
            }),
            message: "Own alert".into(),
            seconds: 1.25,
        },
        Event {
            id: 8,
            owner: 0,
            kind: "lost".into(),
            count: 1,
            at: None,
            message: "Without position".into(),
            seconds: 1.5,
        },
        Event {
            id: 9,
            owner: 1,
            kind: "blocked".into(),
            count: 1,
            at: None,
            message: "Allied private alert".into(),
            seconds: 1.75,
        },
    ];
    let before = format!("{:?}", m.convoys);
    let cache = SnapshotCache::new(&m).unwrap();
    let expected = json!({"enabled":true,
        "units":[
            {"id":own[0],"ammo":0.67,"provisions":12.35,"fuel":4.57,"emergency":null,"warning":1.23,
                "forced":false,"stranded":true,"supply":0.46,"cut":false,"grace":8.},
            {"id":own[1],"ammo":null,"provisions":1.24,"fuel":null,"emergency":2.35,"warning":null,
                "forced":true,"stranded":false,"supply":0.,"cut":true,"grace":null}],
        "trucks":[{"id":truck,"state":"delivering","manual":true,"hold":false,
            "cargo":{"ammo":1.23,"provisions":2.35,"fuel":3.46},
            "destination":{"x":41.2345,"z":42.3456},"route":[{"x":21.23,"z":22.79},{"x":30.13,"z":31.23}]}],
        "stores":[
            {"id":"source:0","owner":0,"x":10.23,"z":11.79,"source":true,
                "stock":{"ammo":90.0625,"provisions":90.0625,"fuel":90.0625},
                "capacity":{"ammo":300.125,"provisions":301.125,"fuel":302.125}},
            {"id":"hospital:7","owner":1,"x":10.23,"z":11.79,"source":false,
                "stock":{"ammo":7.125,"provisions":8.125,"fuel":9.125},
                "capacity":{"ammo":300.125,"provisions":301.125,"fuel":302.125}}],
        "events":[{"id":7,"type":"blocked","count":2,"x":55.2345,"z":56.3456,"message":"Own alert"},
            {"id":8,"type":"lost","count":1,"message":"Without position"}]
    });
    assert_eq!(cache.recipient(0).unwrap().fields["logistics"], expected);
    let allied = cache.recipient(1).unwrap().fields["logistics"].clone();
    assert_eq!(
        allied["stores"][0]["stock"],
        json!({"ammo":50.1875,"provisions":52.1875,"fuel":54.1875})
    );
    assert_eq!(
        allied["events"],
        json!([{"id":9,"type":"blocked","count":1,"message":"Allied private alert"}])
    );
    assert_eq!(allied["trucks"][0]["id"], ally_truck);
    assert_eq!(cache.recipient(0).unwrap().fields["logistics"], expected);
    assert_eq!(format!("{:?}", m.convoys), before);
}

#[test]
fn net54_automatic_convoys_keep_scheduler_routes_while_manual_trucks_replan() {
    use crate::supply_convoys::TruckState;
    for empty in [true, false] {
        let mut m = scene(Mode::Conquest, true);
        let trucks = [m.test_supply_truck(0), m.test_supply_truck(1)];
        for (index, &id) in trucks.iter().enumerate() {
            let at = Point {
                x: 60.,
                z: 60. + index as f64 * 20.,
            };
            let goal = Point { x: 100., z: at.z };
            let u = m.world.game.units.get_mut(&id).unwrap();
            u.x = at.x;
            u.z = at.z;
            u.world_goal = Some(goal);
            u.repath = 0.;
            u.path = if empty { vec![] } else { vec![goal] };
            u.stuck = if empty { 0. } else { 2. };
            m.grid.update(id, u);
            let convoy = m.convoys.as_mut().unwrap().trucks.get_mut(&id).unwrap();
            convoy.manual = index == 1;
            convoy.state = if index == 1 {
                TruckState::Manual
            } else {
                TruckState::Idle
            };
            convoy.destination = None;
            if !empty {
                // The receiver already sees this blocked next route cell.
                let cell = m.world.game.cell_of(at.x + 2., at.z).unwrap();
                m.world.set_cell(cell, b'B').unwrap();
            }
        }
        let convoys = m.convoys.as_mut().unwrap();
        convoys.next_schedule = 10000;
        convoys.schedule_queue.clear();
        convoys.refresh_queue.clear();
        for slot in 0..3 {
            convoys.next_spawn.insert(slot, 10000.);
        }
        let mut delivery = DeliveryState::new(&m).unwrap();
        let auto_path_gen = m.world.game.units[&trucks[0]].path_gen;
        let manual_path_gen = m.world.game.units[&trucks[1]].path_gen;
        for _ in 0..4 {
            tick(&mut m, &mut delivery);
        }
        if empty {
            assert_eq!(m.world.game.units[&trucks[0]].path_gen, auto_path_gen);
            assert!(m.world.game.units[&trucks[0]].path.is_empty());
        }
        assert!(m.world.game.units[&trucks[1]].path_gen > manual_path_gen);
        let mut expected = 0.;
        for _ in 0..4 {
            expected -= m.rules.movement.tick;
        }
        assert_eq!(m.world.game.units[&trucks[0]].repath, expected);
        assert!(m.world.game.units[&trucks[1]].repath > 0.);
    }
}

#[test]
fn net54_damaged_stationary_convoys_never_enter_generic_combat_reactions() {
    for manual in [false, true] {
        let mut m = scene(Mode::Conquest, true);
        let truck = m.test_supply_truck(0);
        let at = Point { x: 60., z: 60. };
        let u = m.world.game.units.get_mut(&truck).unwrap();
        u.x = at.x;
        u.z = at.z;
        u.hp = 1.;
        u.path.clear();
        u.world_goal = None;
        u.hit_from = Some(Point { x: 50., z: 60. });
        m.grid.update(truck, u);
        m.combat.get_mut(truck).unwrap().hit_at = 0.;
        m.commands.units.get_mut(&truck).unwrap().react = 0.;
        let convoys = m.convoys.as_mut().unwrap();
        convoys.trucks.get_mut(&truck).unwrap().manual = manual;
        convoys.trucks.get_mut(&truck).unwrap().state = crate::supply_convoys::TruckState::Idle;
        convoys.next_schedule = 10000;
        convoys.schedule_queue.clear();
        convoys.refresh_queue.clear();
        for slot in 0..3 {
            convoys.next_spawn.insert(slot, 10000.);
        }
        let mut delivery = DeliveryState::new(&m).unwrap();
        let mut expected = 0.;
        for _ in 0..4 {
            expected -= m.rules.movement.tick;
            tick(&mut m, &mut delivery);
            let u = &m.world.game.units[&truck];
            assert_eq!(u.at(), at);
            assert!(u.path.is_empty());
            assert_eq!(m.commands.units[&truck].react, expected);
        }
    }
}

#[test]
fn net54_fractional_medic_and_building_repair_depletion_remains_nonnegative_through_delivery() {
    for repair in [false, true] {
        let mut m = scene(
            if repair {
                Mode::Classic
            } else {
                Mode::Conquest
            },
            true,
        );
        m.start.economy.players[0].mp = 10000.;
        let worker;
        let target;
        if repair {
            worker = m
                .world
                .game
                .units
                .values()
                .find(|u| u.owner == 0 && m.rules.kinds[u.kind].name == "engineer")
                .unwrap()
                .id;
            let unit = m.world.game.units.get_mut(&worker).unwrap();
            unit.x = 28.;
            unit.z = 24.;
            unit.path.clear();
            unit.world_goal = None;
            m.grid.update(worker, unit);
            target = m.next_unit_id;
            assert_eq!(
                m.execute_build(
                    0,
                    &[i64::from(worker)],
                    "flakpos",
                    Some(Point { x: 32., z: 24. }),
                    None,
                    false
                )
                .unwrap(),
                None
            );
            // The paid site's real damaged sections provide the repair deficit.
            m.world.game.units.get_mut(&target).unwrap().built = Some(1.);
            assert_eq!(
                m.execute_assist(0, &[i64::from(worker)], i64::from(target), false)
                    .unwrap(),
                None
            );
        } else {
            worker = m.next_unit_id;
            assert_eq!(
                m.execute_command(
                    0,
                    &Command::Buy {
                        kind: "medic".into(),
                        from: None,
                    },
                    false
                )
                .unwrap(),
                None
            );
            target = m
                .world
                .game
                .units
                .values()
                .find(|u| u.owner == 0 && m.rules.kinds[u.kind].name == "rifle")
                .unwrap()
                .id;
            for (id, x) in [(worker, 60.), (target, 62.)] {
                let unit = m.world.game.units.get_mut(&id).unwrap();
                unit.x = x;
                unit.z = 60.;
                unit.path.clear();
                unit.world_goal = None;
                m.grid.update(id, unit);
            }
            m.world.game.units.get_mut(&target).unwrap().hp = 20.;
        }
        let reserves = m.logistics.get_mut(worker).unwrap();
        reserves.provisions = 0.007;
        reserves.exhausted = false;
        reserves.warning = None;
        reserves.forced = false;
        let hp = m.world.game.units[&target].hp;
        if repair {
            let mut frame = m.begin_construction_tick().unwrap();
            m.step_construction_worker(worker, &mut frame).unwrap();
            m.finish_construction_tick(frame).unwrap();
        } else {
            let ids = m.world.game.units.keys().copied().collect::<Vec<_>>();
            m.heal_wounded(&ids, 1.).unwrap();
        }
        assert!(m.world.game.units[&target].hp > hp, "repair={repair}");
        assert_eq!(
            m.logistics.get(worker).unwrap().provisions,
            0.,
            "repair={repair}"
        );
        let cache = SnapshotCache::new(&m).unwrap();
        let row = cache.recipient(0).unwrap().fields["logistics"]["units"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["id"] == worker)
            .unwrap()
            .clone();
        assert_eq!(row["provisions"], 0.);
        let mut delivery = DeliveryState::new(&m).unwrap();
        for _ in 0..4 {
            let rows = tick(&mut m, &mut delivery);
            assert!(rows.iter().all(|row| row["t"] == "s"));
            assert!(m.logistics.get(worker).unwrap().provisions >= 0.);
        }
    }
}
