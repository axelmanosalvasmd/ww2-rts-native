use super::*;
use crate::{
    economy::{Faction, Mode},
    match_commands::{Command, MoveOutcome},
    math,
    economy_state::ArmySize,
    map::AuthoritativeMap,
    match_firing::LaunchEvent,
    match_runtime::{MatchOptions, PlayerConfig},
    match_tick::TickEvent,
    recipient_snapshot::{DeliveryState, SelectedSeat},
    unit_publication::{PublicationParts, StagedUnit, prepare_publication},
    weather::WeatherChoice,
};
use serde_json::json;

fn scene(kind: &str, cells: &[(usize, usize, u8)]) -> (Match, UnitId) {
    let mut rows = vec![vec![b'.'; 64]; 64];
    for &(x, z, ch) in cells {
        rows[z][x] = ch;
    }
    let rows = rows
        .into_iter()
        .map(|r| String::from_utf8(r).unwrap())
        .collect::<Vec<_>>();
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"Terrain order routing", "w":64, "h":64, "rows":rows,
            "spawns":[{"x":5,"y":5},{"x":58,"y":58}],
            "points":[{"x":32,"y":32}]
        }))
        .unwrap(),
    )
    .unwrap();
    let mut m = Match::new(
        &map,
        MatchOptions {
            seed: 701,
            mode: Mode::Conquest,
            players: (0..2)
                .map(|slot| PlayerConfig {
                    name: format!("p{slot}"),
                    team: slot,
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
            supply: false,
            logistics: false,
            sandbox: false,
        },
    )
    .unwrap();
    let id = m.next_unit_id;
    let staged = StagedUnit::ordinary(
        &m.world.game,
        &m.rules,
        0,
        m.rules.kind(kind).unwrap(),
        id,
        m.world.game.units.len() as u32,
        m.start.players[0].spawn,
        Some(Point { x: 51., z: 51. }),
    )
    .unwrap();
    prepare_publication(
        PublicationParts {
            units: &mut m.world.game.units,
            grid: &mut m.grid,
            controls: &mut m.commands.units,
            recruitment: &mut m.recruit_metadata,
            next_id: &mut m.next_unit_id,
        },
        &m.rules,
        2,
        vec![staged],
    )
    .unwrap()
    .publish_with(|u, _| {
        m.combat.initialize(u.id);
        m.firing.initialize(u.id);
        m.abilities.initialize(u.id);
        m.recovery.initialize(u.id);
    });
    for c in m.commands.units.values_mut() {
        c.hold_fire = true;
        c.auto_retreat = false;
        c.auto_cast = false;
    }
    m.world.game.units.get_mut(&id).unwrap().still = Some(10.);
    (m, id)
}

fn fire(m: &mut Match, id: UnitId, x: f64, z: f64) -> usize {
    assert_eq!(
        m.execute_command(
            0,
            &Command::FireAt {
                ids: vec![i64::from(id)],
                at: Point { x, z },
                queue: false,
            },
            false
        )
        .unwrap(),
        None
    );
    m.world.game.cell_of(x, z).unwrap()
}

fn tick(m: &mut Match, delivery: &mut DeliveryState) -> Vec<TickEvent> {
    let report = m.advance_tick().unwrap();
    let seats = (0..2)
        .map(|slot| SelectedSeat {
            slot,
            full: true,
            omit: false,
        })
        .collect::<Vec<_>>();
    let frame = delivery
        .capture_selected_with_tick_events(m, &seats, &report.events)
        .unwrap()
        .unwrap();
    for slot in 0..2 {
        assert_eq!(frame.recipient(slot).unwrap().into_json()["t"], "s");
    }
    drop(frame);
    assert!(m.tick_state.fault().is_none());
    report.events
}

#[test]
fn terrain_fire_out_of_range_routes_moves_and_stops_to_launch_during_ordinary_ticks() {
    let (mut m, id) = scene("tank", &[(52, 25, b'H')]);
    let start = m.world.game.units[&id].at();
    let cell = fire(&mut m, id, 105., 51.);
    let goal = center(m.world.game.w, cell);
    assert_eq!(m.current_fire_at_goal(id).unwrap(), Some(goal));
    assert!(math::hypot(goal.x - start.x, goal.z - start.z) > 35.);
    let mut delivery = DeliveryState::new(&m).unwrap();
    let mut routed = false;
    let mut launched = false;
    for _ in 0..300 {
        let events = tick(&mut m, &mut delivery);
        routed |= m.commands.requests.used() > 0 && !m.world.game.units[&id].path.is_empty();
        if events.iter().any(|event| {
            matches!(event,
            TickEvent::Firing(LaunchEvent::Flight { shooter, .. }) if *shooter == id)
        }) {
            let u = &m.world.game.units[&id];
            assert!(routed, "the FireAt phase must request its own route");
            assert!(math::hypot(u.x - start.x, u.z - start.z) > 1.);
            assert!(math::hypot(u.x - goal.x, u.z - goal.z) <= 35.);
            assert!(u.path.is_empty(), "the gun stops before a terrain shot");
            launched = true;
            break;
        }
    }
    assert!(launched, "the distant terrain order must eventually fire");
}

#[test]
fn terrain_fire_blocked_sight_prevents_launch_and_requests_a_route() {
    let (mut m, id) = scene("tank", &[(32, 25, b'H'), (40, 25, b'H')]);
    let cell = fire(&mut m, id, 81., 51.);
    assert!(!m.can_shell_cell(id, cell).unwrap());
    m.apply_unit_firing(id, false, 0.05).unwrap();
    assert!(
        m.firing.flights.is_empty(),
        "range alone does not permit a direct terrain shot"
    );
    assert_eq!(
        m.current_fire_at_goal(id).unwrap(),
        Some(Point { x: 81., z: 51. })
    );
    m.begin_path_tick(4096, 20).unwrap();
    m.apply_fire_at_order(id).unwrap();
    assert_eq!(m.commands.requests.used(), 1);
    assert!(!m.world.game.units[&id].path.is_empty());
}

#[test]
fn terrain_fire_can_shell_through_the_first_wall_of_its_own_structure_only() {
    let cells = [(40, 25, b'B'), (41, 25, b'B'), (42, 25, b'B')];
    let (mut m, id) = scene("tank", &cells);
    let cell = fire(&mut m, id, 85., 51.);
    let wall = m.world.game.cell_of(81., 51.).unwrap();
    assert_eq!(
        m.world.sections[&cell].structure_id,
        m.world.sections[&wall].structure_id
    );
    assert!(!crate::vision::los(
        &m.world.game,
        &m.rules,
        (51., 51.),
        (85., 51.)
    ));
    assert!(
        m.can_shell_cell(id, cell).unwrap(),
        "the first visible wall belongs to the selected house"
    );
    m.apply_unit_firing(id, false, 0.05).unwrap();
    assert_eq!(m.firing.flights.len(), 1);

    let (mut other, id) = scene(
        "tank",
        &[
            (35, 25, b'B'),
            (40, 25, b'B'),
            (41, 25, b'B'),
            (42, 25, b'B'),
        ],
    );
    let cell = fire(&mut other, id, 85., 51.);
    let blocker = other.world.game.cell_of(71., 51.).unwrap();
    assert_ne!(
        other.world.sections[&cell].structure_id,
        other.world.sections[&blocker].structure_id
    );
    assert!(
        !other.can_shell_cell(id, cell).unwrap(),
        "an unrelated house still blocks the shot"
    );
    other.apply_unit_firing(id, false, 0.05).unwrap();
    assert!(other.firing.flights.is_empty());
}

#[test]
fn terrain_fire_destroyed_target_cancels_direct_fire_but_salvo_keeps_shelling_open_ground() {
    for kind in ["tank", "mortar"] {
        let (mut m, id) = scene(kind, &[(40, 25, b'H')]);
        let cell = fire(&mut m, id, 81., 51.);
        m.world.set_cell(cell, b'.').unwrap();
        assert_eq!(m.world.game.cell_hp[cell], 0.);
        let mut delivery = DeliveryState::new(&m).unwrap();
        let events = tick(&mut m, &mut delivery);
        if kind == "tank" {
            assert_eq!(m.commands.units[&id].fire_at, None);
            assert!(m.firing.flights.is_empty());
        } else {
            assert_eq!(m.commands.units[&id].fire_at, Some(cell));
            assert!(events.iter().any(|event| matches!(event,
                TickEvent::Firing(LaunchEvent::Flight { shooter,.. }) if *shooter == id)));
            for _ in 0..3 {
                tick(&mut m, &mut delivery);
            }
            assert_eq!(m.commands.units[&id].fire_at, Some(cell));
        }
    }
}

#[test]
fn terrain_fire_path_fifo_discards_replaced_goal_and_services_current_goal() {
    let (mut m, id) = scene("tank", &[(52, 25, b'H'), (51, 27, b'H')]);
    fire(&mut m, id, 105., 51.);
    m.begin_path_tick(0, 20).unwrap();
    m.apply_fire_at_order(id).unwrap();
    assert_eq!(m.commands.requests.pending_len(), 1);
    assert_eq!(m.commands.requests.queue_len(), 1);
    let cell = fire(&mut m, id, 103., 55.);
    m.apply_fire_at_order(id).unwrap();
    assert_eq!(m.commands.requests.pending_len(), 1);
    assert_eq!(m.commands.requests.queue_len(), 2);
    let report = m.begin_path_tick(1, 20).unwrap();
    assert_eq!(report.discarded, 1);
    assert_eq!(report.searched, 1);
    assert_eq!(m.commands.requests.pending_len(), 0);
    assert_eq!(m.commands.requests.queue_len(), 0);
    assert_eq!(m.commands.units[&id].fire_at, Some(cell));
    assert!(!m.world.game.units[&id].path.is_empty());
    assert_eq!(
        m.current_fire_at_goal(id).unwrap(),
        None,
        "a route in progress consumes no new request"
    );
}

#[test]
fn terrain_fire_path_fifo_discards_stopped_orders_without_spending_budget() {
    let (mut m, id) = scene("tank", &[(52, 25, b'H')]);
    fire(&mut m, id, 105., 51.);
    m.begin_path_tick(0, 20).unwrap();
    m.apply_fire_at_order(id).unwrap();
    assert_eq!(m.commands.requests.pending_len(), 1);
    assert_eq!(
        m.execute_command(
            0,
            &Command::Stop {
                ids: vec![i64::from(id)]
            },
            false
        )
        .unwrap(),
        None
    );
    let report = m.begin_path_tick(0, 20).unwrap();
    assert_eq!(report.discarded, 1);
    assert_eq!(report.searched, 0);
    assert_eq!(m.commands.requests.pending_len(), 0);
    assert_eq!(m.commands.units[&id].fire_at, None);
    assert!(m.world.game.units[&id].path.is_empty());
}

#[test]
fn terrain_fire_unreachable_path_drops_fire_order_at_the_retry_limit() {
    let mut cells = (0..64)
        .flat_map(|z| [(35, z, b'W'), (36, z, b'W'), (37, z, b'W')])
        .collect::<Vec<_>>();
    cells.push((52, 25, b'H'));
    let (mut m, id) = scene("tank", &cells);
    fire(&mut m, id, 105., 51.);
    m.begin_path_tick(1, 1).unwrap();
    m.apply_fire_at_order(id).unwrap();
    assert_eq!(m.commands.requests.stats().dropped, 1);
    let c = &m.commands.units[&id];
    assert_eq!(c.fire_at, None);
    assert_eq!(c.outcome, Some(MoveOutcome::Unreachable));
    assert_eq!(
        c.move_result,
        Some(crate::match_commands::MoveResult::Unreachable {
            tick: m.world.game.tick
        })
    );
    assert!(m.world.game.units[&id].path.is_empty());
}

#[test]
fn terrain_fire_mixed_selection_queues_ground_but_dispatches_bomber_immediately() {
    let (mut m, gun) = scene("tank", &[(40, 25, b'H')]);
    let bomber = m.spawn_aircraft(0, "bomber").unwrap();
    assert_eq!(
        m.execute_command(
            0,
            &Command::FireAt {
                ids: vec![i64::from(gun), i64::from(bomber)],
                at: Point { x: 81.8, z: 51.8 },
                queue: true,
            },
            false
        )
        .unwrap(),
        None
    );
    let at = Point { x: 81.0, z: 51.0 };
    assert_eq!(
        m.commands.units[&gun].orders.front(),
        Some(&Command::FireAt {
            ids: vec![i64::from(gun)],
            at,
            queue: false,
        })
    );
    assert!(m.commands.units[&bomber].orders.is_empty());
    let mission = m.aviation.planes[&bomber].mission.unwrap();
    assert_eq!(mission.kind, crate::match_aviation::MissionKind::Area);
    assert_eq!(mission.at, at);
}
