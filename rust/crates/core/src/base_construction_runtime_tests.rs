//! BASE-12 and BASE-26 against pinned sim.js:3281-3302, 3689-3709, 3759.
use super::*;
use crate::{
    economy::{Faction, Mode, Technology},
    economy_state::ArmySize,
    game::Point,
    map::AuthoritativeMap,
    match_commands::{Command, Denial},
    match_runtime::{MatchOptions, PlayerConfig},
    match_world::ResourceNode,
    recipient_snapshot::{DeliveryState, SelectedSeat},
    weather::WeatherChoice,
};
use serde_json::json;

fn scene(tech: bool) -> (Match, u32) {
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"Paid Classic construction", "w":100,"h":100,
            "rows":vec![".".repeat(100);100],"heights":vec!["0".repeat(100);100],
            "spawns":[{"x":8,"y":8},{"x":91,"y":91}],
            "points":[{"x":50,"y":50,"mp":0,"vp":0}]
        }))
        .unwrap(),
    )
    .unwrap();
    let mut m = Match::new_classic(
        &map,
        MatchOptions {
            seed: 701,
            mode: Mode::Classic,
            players: (0..2)
                .map(|slot| PlayerConfig {
                    name: format!("p{slot}"),
                    team: slot,
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
    .unwrap();
    m.start.economy.players[0].mp = 5000.0;
    m.start.economy.players[0].mun = 1000.0;
    m.start.economy.players[1].mp = 5000.0;
    // Two legal, separated node footprints make both candidates fall within one click.
    m.elimination.as_mut().unwrap().nodes = [80.0, 92.0]
        .into_iter()
        .map(|x| ResourceNode {
            cell: 29 * 100 + (x as usize / 2 - 1),
            at: Point { x, z: 60.0 },
            depot: None,
            rate: 2.0,
            fuel: false,
        })
        .collect();
    let worker = own(&m, 0, "engineer");
    relocate(&mut m, worker, Point { x: 80.0, z: 52.0 });
    (m, worker)
}

fn own(m: &Match, owner: usize, kind: &str) -> u32 {
    m.world
        .game
        .units
        .values()
        .find(|u| u.owner == owner && m.rules.kinds[u.kind].name == kind)
        .unwrap()
        .id
}
fn relocate(m: &mut Match, id: u32, at: Point) {
    crate::unit_placement::place_unit(&mut m.grid, m.world.game.units.get_mut(&id).unwrap(), |u| {
        u.x = at.x;
        u.z = at.z;
    });
}
fn build(
    m: &mut Match,
    owner: usize,
    worker: u32,
    kind: &str,
    x: f64,
    z: f64,
    queue: bool,
) -> Option<Denial> {
    m.execute_command(
        owner,
        &Command::Build {
            ids: vec![i64::from(worker)],
            kind: kind.into(),
            at: Some(Point { x, z }),
            direction: None,
            queue,
        },
        false,
    )
    .unwrap()
}
fn cancel(m: &mut Match, owner: usize, id: u32) {
    assert_eq!(
        m.execute_command(owner, &Command::Cancel { id: i64::from(id) }, false)
            .unwrap(),
        None
    );
}
fn snapshot(m: &mut Match, delivery: &mut DeliveryState, events: &[crate::match_tick::TickEvent]) {
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
    for seat in selected {
        assert_eq!(frame.recipient(seat.slot).unwrap().into_json()["t"], "s");
    }
}
fn ticks(m: &mut Match, delivery: &mut DeliveryState, count: usize) {
    for _ in 0..count {
        let report = m.advance_tick().unwrap();
        snapshot(m, delivery, &report.events);
        assert!(m.tick_state.fault().is_none());
    }
}

#[test]
fn base12_depots_snap_to_nearest_free_node_and_release_only_their_claim_on_cancel_or_death() {
    let (mut m, worker) = scene(false);
    let mut delivery = DeliveryState::new(&m).unwrap();
    // The farther first node must lose to the nearer second node.
    let second = m.next_unit_id;
    assert_eq!(build(&mut m, 0, worker, "depot", 87.0, 60.0, false), None);
    assert_eq!(m.world.game.units[&second].at(), Point { x: 92.0, z: 60.0 });
    assert_eq!(m.elimination.as_ref().unwrap().nodes[1].depot, Some(second));
    assert_eq!(m.elimination.as_ref().unwrap().nodes[0].depot, None);
    let first = m.next_unit_id;
    assert_eq!(build(&mut m, 0, worker, "depot", 87.0, 60.0, true), None);
    assert_eq!(m.world.game.units[&first].at(), Point { x: 80.0, z: 60.0 });
    assert_eq!(m.elimination.as_ref().unwrap().nodes[0].depot, Some(first));
    let wallet = m.start.economy.players[0].mp;
    let next_id = m.next_unit_id;
    assert_eq!(
        build(&mut m, 0, worker, "depot", 87.0, 60.0, false),
        Some(Denial::Blocked)
    );
    assert_eq!(m.next_unit_id, next_id);
    assert_eq!(m.start.economy.players[0].mp, wallet);
    ticks(&mut m, &mut delivery, 3);
    let before_refund = m.start.economy.players[0].mp;
    cancel(&mut m, 0, second);
    assert_eq!(m.elimination.as_ref().unwrap().nodes[1].depot, None);
    assert_eq!(m.elimination.as_ref().unwrap().nodes[0].depot, Some(first));
    assert_eq!(m.start.economy.players[0].mp, before_refund + 45.0);
    let rebuilt = m.next_unit_id;
    assert_eq!(build(&mut m, 0, worker, "depot", 92.0, 60.0, false), None);
    assert_eq!(
        m.elimination.as_ref().unwrap().nodes[1].depot,
        Some(rebuilt)
    );
    assert_eq!(
        m.execute_command(
            0,
            &Command::Destroy {
                ids: vec![i64::from(first)],
                id: None
            },
            false
        )
        .unwrap(),
        None
    );
    ticks(&mut m, &mut delivery, 3);
    assert!(!m.world.game.units.contains_key(&first));
    assert_eq!(m.elimination.as_ref().unwrap().nodes[0].depot, None);
    assert_eq!(
        m.elimination.as_ref().unwrap().nodes[1].depot,
        Some(rebuilt)
    );
    let replacement = m.next_unit_id;
    assert_eq!(build(&mut m, 0, worker, "depot", 80.0, 60.0, false), None);
    assert_eq!(
        m.elimination.as_ref().unwrap().nodes[0].depot,
        Some(replacement)
    );
    ticks(&mut m, &mut delivery, 3);
}

#[test]
fn base12_node_radius_visibility_terrain_and_other_building_reservations_are_enforced() {
    for (x, expected) in [(72.0, None), (71.999, Some(Denial::Blocked))] {
        let (mut m, worker) = scene(false);
        m.start.economy.players[0].mp = 60.0;
        assert_eq!(build(&mut m, 0, worker, "depot", x, 60.0, false), expected);
        assert_eq!(
            m.start.economy.players[0].mp,
            if expected.is_none() { 0.0 } else { 60.0 }
        );
    }
    let (mut tied, worker) = scene(false);
    let site = tied.next_unit_id;
    assert_eq!(
        build(&mut tied, 0, worker, "depot", 86.0, 60.0, false),
        None
    );
    assert_eq!(
        tied.elimination.as_ref().unwrap().nodes[0].depot,
        Some(site)
    );
    let (mut m, worker) = scene(false);
    let before = m.start.economy.players[0].mp;
    assert_eq!(
        build(&mut m, 0, worker, "barracks", 80.0, 60.0, false),
        Some(Denial::Blocked)
    );
    assert_eq!(m.start.economy.players[0].mp, before);
    // A blocked nearest node cannot be bypassed by choosing the farther free node.
    let cell = m.elimination.as_ref().unwrap().nodes[1].cell;
    m.world.set_cell(cell, b'W').unwrap();
    assert_eq!(
        build(&mut m, 0, worker, "depot", 87.0, 60.0, false),
        Some(Denial::Blocked)
    );
    assert!(
        m.elimination
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .all(|n| n.depot.is_none())
    );
    let (mut m, worker) = scene(false);
    relocate(&mut m, worker, Point { x: 16.0, z: 24.0 });
    assert_eq!(
        build(&mut m, 0, worker, "depot", 80.0, 60.0, false),
        Some(Denial::NotVisible)
    );
    assert_eq!(
        build(&mut m, 0, worker, "depot", 120.0, 60.0, false),
        Some(Denial::Blocked)
    );
    // Enemy depots contribute no sight to the requesting owner. Their unseen claim
    // must produce notVisible rather than reveal that the node is already taken.
    let enemy = own(&m, 1, "engineer");
    relocate(&mut m, enemy, Point { x: 80.0, z: 52.0 });
    assert_eq!(build(&mut m, 1, enemy, "depot", 80.0, 60.0, false), None);
    assert_eq!(
        build(&mut m, 0, worker, "depot", 80.0, 60.0, false),
        Some(Denial::NotVisible)
    );
    // Observe the node from its center so its opaque walls cannot hide the claim.
    relocate(&mut m, worker, Point { x: 80.0, z: 60.0 });
    assert_eq!(
        build(&mut m, 0, worker, "depot", 80.0, 60.0, false),
        Some(Denial::Blocked)
    );
}

#[test]
fn base26_paid_building_sites_assign_routes_or_waiting_assist_without_orphaning_full_queues() {
    for queue in [false, true] {
        let (mut m, worker) = scene(false);
        let site = m.next_unit_id;
        let before = m.start.economy.players[0].mp;
        let old_path = vec![Point { x: 80.0, z: 48.0 }];
        m.world.game.units.get_mut(&worker).unwrap().path = old_path.clone();
        assert_eq!(build(&mut m, 0, worker, "depot", 80.0, 60.0, queue), None);
        assert_eq!(m.start.economy.players[0].mp, before - 60.0);
        assert_eq!(m.world.game.units[&site].built, Some(0.0));
        assert_eq!(m.world.game.units[&site].hp, 150.0);
        assert_eq!(m.elimination.as_ref().unwrap().nodes[0].depot, Some(site));
        let control = &m.commands.units[&worker];
        if queue {
            assert!(!m.world.game.units[&worker].build);
            assert_eq!(m.world.game.units[&worker].path, old_path);
            assert_eq!(control.build_target, None);
            assert_eq!(
                control.orders.front(),
                Some(&Command::Assist {
                    ids: vec![i64::from(worker)],
                    target: i64::from(site),
                    at: Some(Point { x: 80.0, z: 60.0 }),
                    queue: false,
                })
            );
        } else {
            assert!(m.world.game.units[&worker].build);
            assert!(!m.world.game.units[&worker].path.is_empty());
            assert_eq!(control.build_target, Some(site));
            assert!(control.orders.is_empty());
        }
        let mut delivery = DeliveryState::new(&m).unwrap();
        for _ in 0..600 {
            ticks(&mut m, &mut delivery, 1);
            if m.world.game.units[&site].built == Some(1.0) {
                break;
            }
        }
        assert_eq!(m.world.game.units[&site].built, Some(1.0));
        ticks(&mut m, &mut delivery, 2);
        assert_eq!(m.commands.units[&worker].build_target, None);
    }
    let (mut m, worker) = scene(false);
    for _ in 0..8 {
        m.commands
            .units
            .get_mut(&worker)
            .unwrap()
            .orders
            .push_back(Command::Stop {
                ids: vec![i64::from(worker)],
            });
    }
    let before = m.start.economy.players[0].mp;
    let next = m.next_unit_id;
    assert_eq!(
        build(&mut m, 0, worker, "depot", 80.0, 60.0, true),
        Some(Denial::QueueFull)
    );
    assert_eq!(m.start.economy.players[0].mp, before);
    assert_eq!(m.next_unit_id, next);
    assert_eq!(m.elimination.as_ref().unwrap().nodes[0].depot, None);
    // A nonqueued command replaces a full queue and still assigns its paid site.
    assert_eq!(build(&mut m, 0, worker, "depot", 80.0, 60.0, false), None);
    assert_eq!(m.commands.units[&worker].build_target, Some(next));
    assert!(m.commands.units[&worker].orders.is_empty());
    // A route preparation failure leaves no paid site or node claim behind.
    let (mut m, worker) = scene(false);
    let before = m.start.economy.players[0].mp;
    let next = m.next_unit_id;
    m.world.game.units.get_mut(&worker).unwrap().path_gen = u64::MAX;
    assert!(
        m.execute_build(
            0,
            &[i64::from(worker)],
            "depot",
            Some(Point { x: 80.0, z: 60.0 }),
            None,
            false
        )
        .is_err()
    );
    assert_eq!(m.start.economy.players[0].mp, before);
    assert_eq!(m.next_unit_id, next);
    assert!(!m.world.game.units.contains_key(&next));
    assert_eq!(m.elimination.as_ref().unwrap().nodes[0].depot, None);
}

#[test]
fn base26_builder_money_prerequisite_tech_tier_placement_and_sudden_death_admission() {
    let (mut m, worker) = scene(false);
    let rifle = own(&m, 0, "rifle");
    assert_eq!(
        build(&mut m, 0, rifle, "depot", 80.0, 60.0, false),
        Some(Denial::NoBuilders)
    );
    m.world.game.units.get_mut(&worker).unwrap().retreating = true;
    assert_eq!(
        build(&mut m, 0, worker, "depot", 80.0, 60.0, false),
        Some(Denial::Retreating)
    );
    m.world.game.units.get_mut(&worker).unwrap().retreating = false;
    m.start.economy.players[0].mp = 59.0;
    let next = m.next_unit_id;
    assert_eq!(
        build(&mut m, 0, worker, "depot", 80.0, 60.0, false),
        Some(Denial::Manpower)
    );
    assert_eq!(m.next_unit_id, next);
    m.start.economy.players[0].mp = 5000.0;
    assert_eq!(
        build(&mut m, 0, worker, "armory", 80.0, 72.0, false),
        Some(Denial::Blocked)
    );
    assert_eq!(
        build(&mut m, 0, worker, "motorpool", 80.0, 72.0, false),
        Some(Denial::Needs)
    );
    let barracks = m.next_unit_id;
    assert_eq!(
        build(&mut m, 0, worker, "barracks", 80.0, 72.0, false),
        None
    );
    assert_eq!(m.world.game.units[&barracks].hp, 375.0);
    assert_eq!(
        build(&mut m, 0, worker, "motorpool", 92.0, 72.0, false),
        Some(Denial::Needs)
    );
    let mut delivery = DeliveryState::new(&m).unwrap();
    for _ in 0..900 {
        ticks(&mut m, &mut delivery, 1);
        if m.world.game.units[&barracks].built == Some(1.0) {
            break;
        }
    }
    assert_eq!(m.world.game.units[&barracks].built, Some(1.0));
    let motorpool = m.next_unit_id;
    assert_eq!(
        build(&mut m, 0, worker, "motorpool", 92.0, 72.0, false),
        None
    );
    cancel(&mut m, 0, motorpool);
    // An enemy producer is inaccessible in Classic, as is a dead own one.
    crate::unit_placement::place_unit(
        &mut m.grid,
        m.world.game.units.get_mut(&barracks).unwrap(),
        |u| u.owner = 1,
    );
    assert_eq!(
        build(&mut m, 0, worker, "motorpool", 92.0, 72.0, false),
        Some(Denial::Needs)
    );
    crate::unit_placement::place_unit(
        &mut m.grid,
        m.world.game.units.get_mut(&barracks).unwrap(),
        |u| u.owner = 0,
    );
    m.world.game.units.get_mut(&barracks).unwrap().hp = 0.0;
    assert_eq!(
        build(&mut m, 0, worker, "motorpool", 92.0, 72.0, false),
        Some(Denial::Needs)
    );
    let (mut m, worker) = scene(true);
    assert_eq!(
        build(&mut m, 0, worker, "pillbox", 80.0, 72.0, false),
        Some(Denial::Tier)
    );
    assert_eq!(
        build(&mut m, 0, worker, "armory", 80.0, 72.0, false),
        Some(Denial::Tier)
    );
    m.request_research(0, Technology::Tier).unwrap();
    m.step_research(60.0).unwrap();
    assert_eq!(m.production.player(0).unwrap().tech().tier(), 2);
    assert_eq!(build(&mut m, 0, worker, "armory", 80.0, 72.0, false), None);
    let pillbox = m.next_unit_id;
    assert_eq!(build(&mut m, 0, worker, "pillbox", 92.0, 72.0, false), None);
    assert_eq!(
        m.world.game.units[&pillbox].rot,
        math::atan2(
            72.0 - m.start.players[0].spawn.z,
            92.0 - m.start.players[0].spawn.x
        )
    );
    cancel(&mut m, 0, pillbox);
    let directed = m.next_unit_id;
    assert_eq!(
        m.execute_build(
            0,
            &[i64::from(worker)],
            "pillbox",
            Some(Point { x: 92.0, z: 72.0 }),
            Some(-1.25),
            false
        )
        .unwrap(),
        None
    );
    assert_eq!(m.world.game.units[&directed].rot, -1.25);
    let before = m.start.economy.players[0].mp;
    let next = m.next_unit_id;
    m.elimination.as_mut().unwrap().sudden_death = true;
    assert_eq!(
        build(&mut m, 0, worker, "depot", 80.0, 60.0, false),
        Some(Denial::SuddenDeath)
    );
    assert_eq!(m.start.economy.players[0].mp, before);
    assert_eq!(m.next_unit_id, next);
}
