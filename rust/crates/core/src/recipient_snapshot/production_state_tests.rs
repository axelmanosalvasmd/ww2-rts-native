//! Pinned production wire shapes: sim.js:1034-1050, 5895-5934, 6018-6028.
use super::*;
use crate::{
    economy::{Faction, Technology},
    economy_state::ArmySize,
    game::Point,
    map::AuthoritativeMap,
    match_commands::{Command, Denial},
    match_runtime::{MatchOptions, PlayerConfig},
    orders::OrderKey,
    weather::WeatherChoice,
};

fn scene(tech: bool) -> Match {
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"Classic owner production", "w":70,"h":70,
            "rows":vec![".".repeat(70);70], "heights":vec!["0".repeat(70);70],
            "spawns":[{"x":7,"y":7},{"x":7,"y":62},{"x":62,"y":62}],
            "points":[{"x":35,"y":35,"mp":0,"vp":0}]
        }))
        .unwrap(),
    )
    .unwrap();
    Match::new_classic(
        &map,
        MatchOptions {
            seed: 719,
            mode: Mode::Classic,
            players: [0, 0, 1]
                .into_iter()
                .enumerate()
                .map(|(slot, team)| PlayerConfig {
                    name: format!("P{slot}"),
                    team,
                    faction: Faction::Usa,
                })
                .collect(),
            shuffle: false,
            army: ArmySize::Standard,
            weather: WeatherChoice::Map,
            weather_seed: Some(1),
            living_weather: false,
            map_key: None,
            tech,
            supply: false,
            logistics: false,
            sandbox: false,
        },
    )
    .unwrap()
}
fn hq(m: &Match, owner: usize) -> u32 {
    m.world
        .game
        .units
        .values()
        .find(|u| u.owner == owner && m.rules.kinds[u.kind].name == "hq")
        .unwrap()
        .id
}
fn snapshots(
    m: &mut Match,
    delivery: &mut DeliveryState,
    events: &[crate::match_tick::TickEvent],
) -> Vec<Value> {
    let selected: Vec<_> = (0..m.start.players.len())
        .map(|slot| SelectedSeat {
            slot,
            full: true,
            omit: false,
        })
        .collect();
    let frame = delivery
        .capture_selected_with_tick_events(m, &selected, events)
        .unwrap()
        .unwrap();
    selected
        .iter()
        .map(|seat| frame.recipient(seat.slot).unwrap().into_json())
        .collect()
}
fn command(
    m: &mut Match,
    delivery: &mut DeliveryState,
    owner: usize,
    value: Command,
) -> (Option<Denial>, Vec<Value>) {
    let tick = (m.world.game.tick + 1) as u64;
    m.enqueue_command(
        OrderKey {
            tick,
            issuer: owner as u32,
            sequence: tick,
        },
        value,
    )
    .unwrap();
    let report = m.advance_tick().unwrap();
    assert_eq!(report.commands.len(), 1);
    let denial = report.commands[0].denial;
    let rows = snapshots(m, delivery, &report.events);
    assert!(m.tick_state.fault().is_none());
    (denial, rows)
}

#[test]
fn base22_classic_paid_jobs_are_owner_only_and_waiting_cancellation_preserves_the_head() {
    let mut m = scene(false);
    m.start.economy.players[0].mp = 5000.;
    m.start.economy.players[0].fuel = 1000.;
    let producer = m.next_unit_id;
    m.place_starting_building(0, "motorpool", 14 * 70 + 15)
        .unwrap();
    let rally = Point { x: 44.34, z: 38.36 };
    m.building_rallies.insert(producer, Some(rally));
    let mut delivery = DeliveryState::new(&m).unwrap();
    for _ in 0..2 {
        let (denial, _) = command(
            &mut m,
            &mut delivery,
            0,
            Command::Buy {
                kind: "halftrack".into(),
                from: Some(producer),
            },
        );
        assert_eq!(denial, None);
    }
    let building = m.production.building(producer).unwrap();
    let jobs = building.jobs().to_vec();
    assert_eq!(jobs.len(), 2);
    assert_ne!(jobs[0].id, jobs[1].id);
    assert!(jobs[1].charge.mp > 0. && jobs[1].charge.fuel > 0.);
    let before = building.progress();
    let rows = snapshots(&mut m, &mut delivery, &[]);
    assert_eq!(
        rows[0]["productionJobs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row[0] == producer)
            .unwrap(),
        &json!([producer,[
            {"id":jobs[0].id,"type":"halftrack","mp":jobs[0].charge.mp,"fuel":jobs[0].charge.fuel,"status":"active"},
            {"id":jobs[1].id,"type":"halftrack","mp":jobs[1].charge.mp,"fuel":jobs[1].charge.fuel,"status":"waiting"}
        ]])
    );
    assert_eq!(
        rows[0]["queues"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row[0] == producer)
            .unwrap(),
        &json!([producer, 0., 44.3, 38.4, "halftrack", "halftrack"])
    );
    for row in &rows[1..] {
        for field in ["productionJobs", "queues"] {
            assert!(
                row[field]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|row| row[0] != producer)
            );
        }
    }
    // Allied players can see the building, but cannot cancel its owner's jobs.
    let cancellation = |job| Command::CancelProduction {
        id: i64::from(producer),
        job: Some(job as i64),
    };
    assert_eq!(
        command(&mut m, &mut delivery, 1, cancellation(jobs[1].id)).0,
        Some(Denial::Unseen)
    );
    assert_eq!(
        command(&mut m, &mut delivery, 0, cancellation(jobs[0].id)).0,
        Some(Denial::NotWaiting)
    );
    let wallet = m.start.economy.players[0].clone();
    let progress = m.production.building(producer).unwrap().progress();
    let (denial, rows) = command(&mut m, &mut delivery, 0, cancellation(jobs[1].id));
    assert_eq!(denial, None);
    assert!(m.start.economy.players[0].mp >= wallet.mp + jobs[1].charge.mp);
    assert!(m.start.economy.players[0].fuel >= wallet.fuel + jobs[1].charge.fuel);
    let building = m.production.building(producer).unwrap();
    assert_eq!(building.jobs(), &jobs[..1]);
    assert!(building.progress() > progress && building.progress() > before);
    assert_eq!(
        rows[0]["productionJobs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row[0] == producer)
            .unwrap()[1]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    // A direct cancellation isolates the exact refund from concurrent income.
    let waiting = m
        .recorded_world_buy(0, "halftrack", Some(producer))
        .unwrap();
    let charged = m.production.building(producer).unwrap().jobs()[1].charge;
    let before = m.production.building(producer).unwrap().progress();
    let mp = m.start.economy.players[0].mp;
    let fuel = m.start.economy.players[0].fuel;
    assert_eq!(m.cancel_training(0, producer, waiting).unwrap(), charged);
    assert_eq!(m.start.economy.players[0].mp, mp + charged.mp);
    assert_eq!(m.start.economy.players[0].fuel, fuel + charged.fuel);
    assert_eq!(m.production.building(producer).unwrap().progress(), before);
    snapshots(&mut m, &mut delivery, &[]);
    m.elimination.as_mut().unwrap().sudden_death = true;
    assert_eq!(
        command(&mut m, &mut delivery, 0, cancellation(jobs[0].id)).0,
        Some(Denial::SuddenDeath)
    );
}

#[test]
fn base59_classic_snapshots_publish_exact_nodes_and_private_research_rallies_through_ticks() {
    let mut m = scene(true);
    m.start.economy.players[0].mp = 5000.;
    m.start.economy.players[0].mun = 500.;
    m.start.economy.players[0].fuel = 54.9;
    let producer = hq(&m, 0);
    m.recorded_world_buy(0, "rifle", Some(producer)).unwrap();
    m.request_research(0, Technology::Tier).unwrap();
    m.start.economy.players[0].mun = 31.9;
    m.player_rallies[0] = Some(Point { x: 38.34, z: 48.36 });
    m.building_rallies
        .insert(producer, Some(Point { x: 30.36, z: 44.34 }));
    m.elimination.as_mut().unwrap().time_left = Some(123.4);
    let expected_nodes = json!(
        m.elimination
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .map(|node| [
                r(node.at.x),
                r(node.at.z),
                node.rate,
                if node.fuel { 1. } else { 0. }
            ])
            .collect::<Vec<_>>()
    );
    assert!(!expected_nodes.as_array().unwrap().is_empty());
    let mut delivery = DeliveryState::new(&m).unwrap();
    for step in 0..6 {
        let events = if step == 0 {
            vec![]
        } else {
            m.advance_tick().unwrap().events
        };
        let rows = snapshots(&mut m, &mut delivery, &events);
        for (slot, row) in rows.iter().enumerate() {
            assert_eq!(row["nodes"], expected_nodes);
            assert_eq!(
                row["mode"],
                json!({"kind":"classic","timeLeft":124.,"suddenDeath":false})
            );
            assert!(row["ghosts"].is_array());
            assert_eq!(row["tech"]["tier"], 1);
            if slot == 0 {
                assert_eq!(row["rally"], json!([38.3, 48.4]));
                assert_eq!(row["mun"], json!(m.start.economy.players[0].mun.floor()));
                assert!(!row["tech"]["lab"].as_array().unwrap().is_empty());
                assert!(
                    row["productionJobs"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|row| row[0] == producer)
                );
            } else {
                assert_eq!(row["rally"], Value::Null);
                assert_eq!(row["tech"]["lab"], json!([]));
                assert!(
                    row["productionJobs"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .all(|row| row[0] != producer)
                );
            }
        }
        assert_eq!(
            m.ordinary_snapshot_raw_lists().unwrap().nodes,
            Some(expected_nodes.clone())
        );
        assert!(m.tick_state.fault().is_none());
    }
    // Classic node locations are public even when outside this team's vision.
    assert!(
        m.elimination
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .any(|node| !crate::vision::team_sees(&m.world.game, &m.rules, &m.grid, 0, node.at))
    );
    m.elimination.as_mut().unwrap().nodes[0].rate = f64::NAN;
    assert!(DeliveryState::new(&m).is_err());
    assert!(m.ordinary_snapshot_raw_lists().is_err());
}

#[test]
fn queued_aircraft_bind_a_room_with_integrated_aviation_delivery() {
    let mut m = scene(false);
    m.start.economy.players[0].mp = 5000.;
    m.start.economy.players[0].fuel = 5000.;
    let field = m.next_unit_id;
    m.place_starting_building(0, "airfield", 14 * 70 + 15)
        .unwrap();
    m.recorded_world_buy(0, "fighter", Some(field)).unwrap();
    let jobs = m.production.building(field).unwrap().jobs().to_vec();
    assert_eq!(m.restrict_to_ordinary_snapshots(), Ok(()));
    assert!(m.ordinary_snapshot_only);
    assert_eq!(m.production.building(field).unwrap().jobs(), jobs);
    let mut delivery = DeliveryState::new(&m).unwrap();
    for _ in 0..4 {
        let report = m.advance_tick().unwrap();
        let rows = snapshots(&mut m, &mut delivery, &report.events);
        assert_eq!(rows.len(), 3);
        assert!(rows.iter().all(|row| row["tick"] == report.tick));
    }
    assert!(m.tick_state.fault().is_none());
}
