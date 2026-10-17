use crate::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::Point,
    map::AuthoritativeMap,
    match_commands::{Command, MoveOrder},
    match_runtime::{Match, MatchOptions, PlayerConfig},
    recipient_snapshot::{DeliveryState, SelectedSeat},
    unit_publication::{PublicationParts, StagedUnit, prepare_publication},
    weather::WeatherChoice,
};
use crate::{environment::WreckMetadata, game::Wreck, world::CellChange};
use serde_json::{Value, json};

fn scene(cover: bool) -> Match {
    let mut rows = vec![".".repeat(96); 96];
    if cover {
        rows[26] = "H".repeat(96);
        rows[28] = "H".repeat(96);
    } else {
        rows[29] = "H".repeat(96);
    }
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"Traffic privacy intent", "w":96,"h":96,
            "rows":rows,
            "spawns":[{"x":5,"y":5},{"x":87,"y":87}],
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
    for control in m.commands.units.values_mut() {
        control.hold_fire = true;
        control.auto_retreat = false;
    }
    m
}

fn publish(m: &mut Match, name: &str, owner: usize, at: Point) -> u32 {
    let id = m.next_unit_id;
    let staged = StagedUnit::ordinary(
        &m.world.game,
        &m.rules,
        owner,
        m.rules.kind(name).unwrap(),
        id,
        m.world.game.units.len() as u32,
        m.start.players[owner].spawn,
        Some(at),
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
    let control = m.commands.units.get_mut(&id).unwrap();
    control.hold_fire = true;
    control.auto_retreat = false;
    let unit = m.world.game.units.get_mut(&id).unwrap();
    unit.hold_pos = true;
    unit.still = Some(5.0);
    id
}

fn orders(m: &mut Match, orders: Vec<(u32, Point)>) {
    assert_eq!(
        m.execute_command(
            0,
            &Command::Move {
                orders: orders
                    .into_iter()
                    .map(|(unit, at)| MoveOrder {
                        unit: i64::from(unit),
                        at
                    })
                    .collect(),
                control_ids: None,
                attack_move: false,
                queue: false,
                together: false,
                face: None,
            },
            false
        )
        .unwrap(),
        None
    );
}

fn tick(m: &mut Match, delivery: &mut DeliveryState) -> Vec<Value> {
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
    let snapshots = (0..2)
        .map(|slot| {
            let value = frame.recipient(slot).unwrap().into_json();
            assert_eq!(value["t"], "s");
            value
        })
        .collect();
    assert!(m.tick_state.fault().is_none());
    snapshots
}

fn hidden(m: &mut Match, cell: usize) {
    assert_eq!(
        m.routing_fog.team(&m.world.game, &m.rules, 0).vis[cell],
        0,
        "the candidate terrain must stay outside owner sight"
    );
}

// Adapt the two ordinary scenes in test-engine-traffic-privacy.js:95-187.
// The controls reveal truth through normal recipient observation. They do not
// replace a traffic reader or inject omniscient knowledge into a hidden run.
fn trajectory(cover: bool, changed: bool, reveal: bool) -> Vec<[u64; 3]> {
    let mut m = scene(cover);
    let (actor, blocker) = if cover {
        let tank = publish(&mut m, "tank", 0, Point { x: 46.0, z: 55.0 });
        let rifle = publish(&mut m, "rifle", 0, Point { x: 40.0, z: 55.0 });
        m.world.game.units.get_mut(&tank).unwrap().rot = std::f64::consts::PI;
        m.world.game.units.get_mut(&rifle).unwrap().hit_from = Some(Point { x: 40.0, z: 0.0 });
        (rifle, tank)
    } else {
        let tank = publish(&mut m, "tank", 0, Point { x: 40.0, z: 55.0 });
        let rifle = publish(&mut m, "rifle", 0, Point { x: 46.0, z: 55.0 });
        (tank, rifle)
    };
    let cell = if cover { 23 * 96 + 19 } else { 30 * 96 + 20 };
    let mut delivery = DeliveryState::new(&m).unwrap();
    for _ in 0..8 {
        tick(&mut m, &mut delivery);
    }
    hidden(&mut m, cell);
    let flags = m
        .recipient_routing(0)
        .unwrap()
        .query()
        .navigation()
        .flags
        .to_vec();
    let wreck_id = m.next_unit_id;
    if changed {
        if cover {
            m.next_unit_id += 1;
            m.world.game.wrecks.push(Wreck {
                id: Some(wreck_id),
                cell: None,
                x: 39.2,
                z: 47.8,
                moving: false,
            });
            m.environment.wreck_metadata.insert(
                wreck_id,
                WreckMetadata {
                    kind: m.rules.kind("tank").unwrap(),
                    owner: 1,
                    rotation: 0.0,
                    motion: None,
                },
            );
        } else {
            m.world
                .mutate_cell(
                    cell,
                    CellChange {
                        object: Some(b'R'),
                        ..CellChange::default()
                    },
                )
                .unwrap();
        }
    }
    m.world.game.reveal = reveal;
    for _ in 0..2 {
        tick(&mut m, &mut delivery);
    }
    if !reveal {
        hidden(&mut m, cell);
        assert_eq!(
            m.recipient_routing(0).unwrap().query().navigation().flags,
            flags
        );
        assert!(
            !m.observed_wrecks(0)
                .unwrap()
                .iter()
                .any(|w| w.id == Some(wreck_id))
        );
    }
    let mut moves = vec![(actor, Point { x: 80.0, z: 55.0 })];
    if cover {
        moves.push((blocker, Point { x: 20.0, z: 55.0 }));
    }
    orders(&mut m, moves);
    let mut response = Vec::new();
    let mut turned = false;
    for _ in 0..24 {
        let snapshots = tick(&mut m, &mut delivery);
        let actor_unit = &m.world.game.units[&actor];
        let blocker_unit = &m.world.game.units[&blocker];
        let distance =
            crate::math::hypot(actor_unit.x - blocker_unit.x, actor_unit.z - blocker_unit.z);
        if cover {
            assert!(distance > 3.7, "yield must precede physical actor contact");
            assert!(
                crate::math::hypot(actor_unit.x - 39.2, actor_unit.z - 47.8) > 6.0,
                "yield must precede observation or contact with the wreck"
            );
        } else {
            let clearance = m.rules.kinds[actor_unit.kind].radius * 0.8
                + m.rules.kinds[blocker_unit.kind].radius * 0.8
                + 0.5;
            assert!(distance > clearance, "turn must precede actor contact");
            assert!(
                actor_unit.z < 58.0,
                "turn must precede hidden terrain contact"
            );
        }
        turned = if cover {
            (actor_unit.z - 55.0).abs() > 0.01
        } else {
            actor_unit.rot.abs() > 0.01
        };
        let row = snapshots[0]["units"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row[0].as_u64() == Some(u64::from(actor)))
            .unwrap();
        let pose = [
            row[3].as_f64().unwrap(),
            row[4].as_f64().unwrap(),
            row[5].as_f64().unwrap(),
        ];
        assert!(pose.iter().all(|v| v.is_finite()));
        response.push(pose.map(f64::to_bits));
        if !reveal {
            hidden(&mut m, cell);
            assert_eq!(
                m.recipient_routing(0).unwrap().query().navigation().flags,
                flags
            );
            assert!(
                !m.observed_wrecks(0)
                    .unwrap()
                    .iter()
                    .any(|w| w.id == Some(wreck_id))
            );
            assert!(
                !snapshots[0]["wrecks"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|row| row[0].as_u64() == Some(u64::from(wreck_id)))
            );
        }
        if turned {
            break;
        }
    }
    assert!(
        turned,
        "the ordinary movement must deliver a voluntary response"
    );
    let unit = &m.world.game.units[&actor];
    let direction = if cover { unit.z - 55.0 } else { unit.rot };
    assert!(
        if changed && reveal {
            direction < 0.0
        } else {
            direction > 0.0
        },
        "observing the changed candidate must reverse the voluntary choice: cover={cover} changed={changed} reveal={reveal} direction={direction}"
    );
    response
}

#[test]
fn mov61_ordinary_ticks_keep_hidden_clearance_and_cover_out_of_delivered_trajectories() {
    for cover in [false, true] {
        let open = trajectory(cover, false, false);
        let hidden = trajectory(cover, true, false);
        assert_eq!(
            hidden, open,
            "unobserved terrain or wreck metadata changed the delivered trajectory"
        );
        let visible_open = trajectory(cover, false, true);
        let visible_changed = trajectory(cover, true, true);
        assert_ne!(
            visible_open, visible_changed,
            "the scene must detect reading physical truth"
        );
    }
}

#[test]
fn mov61_traffic_filters_hidden_enemies_but_includes_visible_enemies_and_allies() {
    let mut m = scene(false);
    let actor = publish(&mut m, "tank", 0, Point { x: 40.0, z: 55.0 });
    let blocker = publish(&mut m, "rifle", 1, Point { x: 46.0, z: 55.0 });
    let mut active = m.world.game.units[&actor].clone();
    active.hold_pos = false;
    active.path = vec![Point { x: 80.0, z: 55.0 }];
    let query = |m: &Match| {
        let mut game = m.world.game.clone();
        game.units.remove(&actor);
        let mut unit = active.clone();
        let goal = unit.path[0];
        crate::traffic::traffic_step(
            &game,
            &m.rules,
            &m.grid,
            &mut unit,
            Some(goal),
            m.rules.movement.tick,
        )
    };
    m.world.game.players[0].visible.clear();
    let hidden = query(&m);
    assert_eq!(hidden.goal, active.path.first().copied());
    assert!(!hidden.blocked && !hidden.yielding);
    m.world.game.players[0].visible.push(blocker);
    let visible = query(&m);
    assert_ne!(
        visible.goal, hidden.goal,
        "visible enemy must change voluntary traffic"
    );
    m.world.game.players[0].visible.clear();
    m.world.game.players[1].team = m.world.game.players[0].team;
    let allied = query(&m);
    assert_ne!(
        allied.goal, hidden.goal,
        "allied traffic is included without enemy visibility membership"
    );
}
