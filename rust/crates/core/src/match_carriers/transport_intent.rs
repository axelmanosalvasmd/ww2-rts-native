//! Transport intent against sim.js:2422-2433 and 3520-3570 at e4e579a.
use super::*;
use crate::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    map::AuthoritativeMap,
    match_runtime::{MatchOptions, PlayerConfig},
    recipient_snapshot::{DeliveryState, SelectedSeat},
    unit_publication::StagedUnit,
    weather::WeatherChoice,
};
use serde_json::json;

fn scene() -> Match {
    let mut rows = vec![".".repeat(96); 64];
    for row in &mut rows {
        row.replace_range(40..60, &"W".repeat(20));
    }
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"Transport intent", "w":96,"h":64,"rows":rows,
            "heights":vec!["0".repeat(96);64], "naval":true,
            "spawns":[{"x":8,"y":8},{"x":88,"y":56}],
            "points":[{"x":20,"y":56,"mp":0,"vp":0}]
        }))
        .unwrap(),
    )
    .unwrap();
    let mut m = Match::new(
        &map,
        MatchOptions {
            seed: 14675,
            mode: Mode::Conquest,
            players: [Faction::Usa, Faction::Germany]
                .into_iter()
                .enumerate()
                .map(|(slot, faction)| PlayerConfig {
                    name: format!("P{slot}"),
                    team: slot as i32,
                    faction,
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
    for c in m.commands.units.values_mut() {
        c.hold_fire = true;
        c.auto_retreat = false;
        c.auto_cast = false;
    }
    m
}

fn add(m: &mut Match, kind: &str, at: Point) -> u32 {
    let id = m.next_unit_id;
    let draft = StagedUnit::ordinary(
        &m.world.game,
        &m.rules,
        0,
        m.rules.kind(kind).unwrap(),
        id,
        m.world.game.units.len() as u32,
        m.start.players[0].spawn,
        Some(at),
    )
    .unwrap();
    m.publish_starting_unit(draft).unwrap();
    let c = m.commands.units.get_mut(&id).unwrap();
    c.hold_fire = true;
    c.auto_retreat = false;
    c.auto_cast = false;
    id
}

fn loaded(m: &mut Match, kind: &str, at: Point) -> (u32, Vec<u32>) {
    let carrier = add(m, kind, at);
    let seats = if kind == "lcvp" { 1 } else { 2 };
    let riders = (0..seats).map(|_| add(m, "rifle", at)).collect::<Vec<_>>();
    for &id in &riders {
        m.world.game.units.get_mut(&id).unwrap().riding = true;
        m.combat.get_mut(id).unwrap().carrier = Some(carrier);
    }
    m.combat.get_mut(carrier).unwrap().cargo = riders.clone();
    (carrier, riders)
}

fn unload(m: &mut Match, ids: &[u32]) -> Option<Denial> {
    let command = Command::from_json(&json!({"t":"unload","ids":ids})).unwrap();
    m.execute_command(0, &command, false).unwrap()
}

#[test]
fn mov75_unload_uses_nearest_free_cells_and_inclusive_shore_reach_per_carrier() {
    // Last dry cell is centred at x=79. The five-metre boundary is inclusive.
    for (x, expected) in [
        (84.0, None),
        (84.001, Some(Denial::Shore)),
        (101.0, Some(Denial::Shore)),
    ] {
        let mut m = scene();
        let (boat, riders) = loaded(&mut m, "lcvp", Point { x, z: 65. });
        let hp: Vec<_> = riders.iter().map(|id| m.world.game.units[id].hp).collect();
        m.commands.units.get_mut(&boat).unwrap().follow = riders[0];
        assert_eq!(unload(&mut m, &[boat]), expected);
        assert_eq!(m.commands.units[&boat].follow, 0);
        assert_eq!(Denial::Shore.name(), "shore");
        for (i, id) in riders.iter().enumerate() {
            let u = &m.world.game.units[id];
            assert_eq!(u.hp, hp[i], "manual unload never applies wreck damage");
            assert_eq!(u.riding, expected.is_some());
            assert_eq!(m.combat.get(*id).unwrap().carrier, expected.map(|_| boat));
            if expected.is_none() {
                assert_eq!(u.at(), Point { x: 79., z: 65. });
                assert_eq!(
                    m.world.game.flags[m.world.game.cell_of(u.x, u.z).unwrap()] & MOVE,
                    0
                );
            }
        }
        assert_eq!(
            m.combat.get(boat).unwrap().cargo.is_empty(),
            expected.is_none()
        );
        crate::unit_placement::tests::assert_current(&m.world.game, &m.grid);
    }
    let mut m = scene();
    let (deep, deep_riders) = loaded(&mut m, "lcvp", Point { x: 101., z: 65. });
    let (shore, shore_riders) = loaded(&mut m, "lcvp", Point { x: 83., z: 65. });
    let (ground, ground_riders) = loaded(&mut m, "halftrack", Point { x: 73., z: 65. });
    // A blocked carrier cell searches right first, then left, matching nearestFree.
    let blocked = m.world.game.cell_of(73., 65.).unwrap();
    m.world.set_cell(blocked, b'B').unwrap();
    assert_eq!(unload(&mut m, &[deep, ground, shore, ground]), None);
    for id in deep_riders {
        assert!(m.world.game.units[&id].riding);
    }
    for id in ground_riders {
        assert_eq!(m.world.game.units[&id].at(), Point { x: 75., z: 65. });
    }
    for id in shore_riders {
        assert_eq!(m.world.game.units[&id].at(), Point { x: 79., z: 65. });
    }
    assert_eq!(unload(&mut m, &[ground, shore]), Some(Denial::Needs));

    // Wadeable surf is a valid landing even when dry ground is far away.
    let ford = m.world.game.cell_of(103., 65.).unwrap();
    m.world.set_cell(ford, b'F').unwrap();
    assert_eq!(unload(&mut m, &[deep]), None);
    assert!(m.combat.get(deep).unwrap().cargo.is_empty());
    crate::unit_placement::tests::assert_current(&m.world.game, &m.grid);
}

fn tick(m: &mut Match, delivery: &mut DeliveryState) {
    let report = m.advance_tick().unwrap();
    let seats = (0..m.start.players.len())
        .map(|slot| SelectedSeat {
            slot,
            full: false,
            omit: false,
        })
        .collect::<Vec<_>>();
    let frame = delivery
        .capture_selected_with_tick_events(m, &seats, &report.events)
        .unwrap()
        .unwrap();
    for seat in seats {
        assert_eq!(frame.recipient(seat.slot).unwrap().into_json()["t"], "s");
    }
    assert!(m.tick_state.fault().is_none());
}

fn sail(m: &mut Match, delivery: &mut DeliveryState, boat: u32, at: Point) {
    let command = Command::from_json(&json!({"t":"move","orders":[[boat,at.x,at.z]]})).unwrap();
    assert_eq!(m.execute_command(0, &command, false).unwrap(), None);
    for _ in 0..400 {
        tick(m, delivery);
        let u = &m.world.game.units[&boat];
        if math::hypot(u.x - at.x, u.z - at.z) < 0.5 {
            return;
        }
    }
    panic!("boat did not reach its ordered destination");
}

fn command(m: &mut Match, value: serde_json::Value) -> Option<Denial> {
    m.execute_command(0, &Command::from_json(&value).unwrap(), false)
        .unwrap()
}

fn finish_board_and_deliver(m: &mut Match, delivery: &mut DeliveryState, id: u32, carrier: u32) {
    assert_eq!(m.world.game.units[&id].board, carrier);
    for _ in 0..400 {
        tick(m, delivery);
        if m.world.game.units[&id].riding {
            break;
        }
    }
    assert!(m.world.game.units[&id].riding);
    assert_eq!(m.combat.get(id).unwrap().carrier, Some(carrier));
    assert!(m.combat.get(carrier).unwrap().cargo.contains(&id));
    for _ in 0..4 {
        tick(m, delivery);
    }
    assert_eq!(unload(m, &[carrier]), None);
    for _ in 0..4 {
        tick(m, delivery);
    }
    assert!(!m.world.game.units[&id].riding);
    assert_eq!(m.combat.get(id).unwrap().carrier, None);
    crate::unit_placement::tests::assert_current(&m.world.game, &m.grid);
}

#[test]
fn cmb146_board_exits_garrison_before_routing_and_delivers_board_and_unload() {
    let mut m = scene();
    let at = Point { x: 61., z: 61. };
    let id = add(&mut m, "rifle", at);
    let cell = m.world.game.cell_of(at.x, at.z).unwrap();
    m.world.set_cell(cell, b'B').unwrap();
    let entry = Point { x: 59., z: 61. };
    let u = m.world.game.units.get_mut(&id).unwrap();
    crate::unit_placement::place_unit(&mut m.grid, u, |u| {
        u.x = entry.x;
        u.z = entry.z;
    });
    crate::vision::update_vision(&mut m.world.game, &m.rules, &m.grid);
    assert_eq!(
        command(&mut m, json!({"t":"garrison","ids":[id],"x":at.x,"z":at.z})),
        None
    );
    m.apply_unit_entry_with_routes(id).unwrap();
    assert_eq!(m.world.game.units[&id].garrison, Some(cell));
    let carrier = add(&mut m, "halftrack", Point { x: 73., z: 61. });
    let mut delivery = DeliveryState::new(&m).unwrap();
    assert_eq!(
        command(&mut m, json!({"t":"board","ids":[id],"target":carrier})),
        None
    );
    let u = &m.world.game.units[&id];
    assert_eq!(u.garrison, None);
    assert_ne!(u.at(), at);
    assert_eq!(
        m.world.game.flags[m.world.game.cell_of(u.x, u.z).unwrap()] & MOVE,
        0
    );
    assert!(!u.path.is_empty(), "route starts at the free exit cell");
    finish_board_and_deliver(&mut m, &mut delivery, id, carrier);
}

#[test]
fn cmb146_board_stops_construction_without_removing_or_refunding_the_site() {
    let mut m = scene();
    m.start.economy.players[0].mp = 1000.;
    let id = add(&mut m, "engineer", Point { x: 53., z: 61. });
    let carrier = add(&mut m, "halftrack", Point { x: 71., z: 61. });
    crate::vision::update_vision(&mut m.world.game, &m.rules, &m.grid);
    let site = m.next_unit_id;
    assert_eq!(
        command(
            &mut m,
            json!({"t":"build","ids":[id],"kind":"flakpos","x":57,"z":61})
        ),
        None
    );
    assert!(m.world.game.units[&id].build);
    assert_eq!(m.commands.units[&id].build_target, Some(site));
    let mut delivery = DeliveryState::new(&m).unwrap();
    for _ in 0..40 {
        tick(&mut m, &mut delivery);
        if m.world.game.units[&site].built.unwrap() > 0. {
            break;
        }
    }
    let progress = m.world.game.units[&site].built;
    assert!(progress.unwrap() > 0. && progress.unwrap() < 1.);
    let mp = m.start.economy.players[0].mp;
    assert_eq!(
        command(&mut m, json!({"t":"board","ids":[id],"target":carrier})),
        None
    );
    assert!(!m.world.game.units[&id].build);
    assert_eq!(m.commands.units[&id].build_target, None);
    assert_eq!(m.start.economy.players[0].mp, mp);
    finish_board_and_deliver(&mut m, &mut delivery, id, carrier);
    assert_eq!(
        m.world.game.units[&site].built, progress,
        "unattended site remains paid and unfinished"
    );
}

#[test]
fn cmb146_board_cancels_paid_dig_without_refund_and_survives_ticks() {
    let mut m = scene();
    m.start.economy.players[0].mp = 1000.;
    let id = add(&mut m, "rifle", Point { x: 53., z: 65. });
    let carrier = add(&mut m, "halftrack", Point { x: 73., z: 65. });
    crate::vision::update_vision(&mut m.world.game, &m.rules, &m.grid);
    assert_eq!(
        command(
            &mut m,
            json!({"t":"dig","ids":[id],"kind":"trench","x":53,"z":65,"dir":0})
        ),
        None
    );
    assert!(m.world.game.units[&id].dig);
    let mut delivery = DeliveryState::new(&m).unwrap();
    tick(&mut m, &mut delivery);
    assert!(m.field_works.jobs()[&id].elapsed > 0.);
    let job_cells = m.field_works.jobs()[&id].cells.clone();
    let mp = m.start.economy.players[0].mp;
    assert_eq!(
        command(&mut m, json!({"t":"board","ids":[id],"target":carrier})),
        None
    );
    assert!(!m.world.game.units[&id].dig);
    assert!(!m.field_works.is_active(id));
    assert_eq!(m.start.economy.players[0].mp, mp);
    finish_board_and_deliver(&mut m, &mut delivery, id, carrier);
    assert!(
        job_cells.iter().all(|c| m.world.game.chars[c.cell] != c.ch),
        "cancelled work never stamps terrain"
    );
}

#[test]
fn cmb146_board_detaches_only_assigned_mass_entrenchment_workers() {
    use crate::field_works::{FortKind, Pattern, ProjectMaterial, ProjectOrder};
    let mut m = scene();
    m.start.economy.players[0].mp = 1000.;
    let worker = add(&mut m, "rifle", Point { x: 53., z: 65. });
    let waiting = add(&mut m, "rifle", Point { x: 55., z: 65. });
    let carrier = add(&mut m, "lcvp", Point { x: 81., z: 65. });
    crate::vision::update_vision(&mut m.world.game, &m.rules, &m.grid);
    let order = Command::Entrench {
        ids: vec![i64::from(worker), i64::from(waiting)],
        order: ProjectOrder::New {
            pattern: Some(Pattern::Line),
            material: ProjectMaterial::Fort(FortKind::Trench),
            from: Some(Point { x: 53., z: 65. }),
            to: Some(Point { x: 69., z: 65. }),
        },
        queue: false,
    };
    assert_eq!(m.execute_command(0, &order, false).unwrap(), None);
    let project = m.field_works.membership(worker).unwrap();
    assert_eq!(m.field_works.membership(waiting), Some(project));
    let mut delivery = DeliveryState::new(&m).unwrap();
    tick(&mut m, &mut delivery);
    assert!(m.world.game.units[&worker].dig || m.world.game.units[&waiting].dig);
    // The nearer squad gets the boat's sole seat; the other keeps its paid job.
    let before = m.field_works.jobs().get(&worker).cloned();
    let mp = m.start.economy.players[0].mp;
    assert_eq!(
        command(
            &mut m,
            json!({"t":"board","ids":[worker,waiting],"target":carrier})
        ),
        None
    );
    assert_eq!(m.world.game.units[&waiting].board, carrier);
    assert!(!m.world.game.units[&waiting].entrench);
    assert!(!m.world.game.units[&waiting].dig);
    assert_eq!(m.field_works.membership(waiting), None);
    assert_eq!(m.field_works.membership(worker), Some(project));
    assert_eq!(m.field_works.jobs().get(&worker), before.as_ref());
    assert_eq!(m.start.economy.players[0].mp, mp);
    for _ in 0..400 {
        tick(&mut m, &mut delivery);
        if m.world.game.units[&waiting].riding {
            break;
        }
    }
    assert!(m.world.game.units[&waiting].riding);
    for _ in 0..4 {
        tick(&mut m, &mut delivery);
    }
    assert_eq!(unload(&mut m, &[carrier]), None);
    for _ in 0..4 {
        tick(&mut m, &mut delivery);
    }
    assert!(!m.world.game.units[&waiting].riding);
}

#[test]
fn mov75_board_sail_refuse_deep_unload_then_land_with_every_seat_snapshots() {
    let mut m = scene();
    let boat = add(&mut m, "lcvp", Point { x: 81., z: 65. });
    let riders = [add(&mut m, "rifle", Point { x: 79., z: 65. })];
    let board = Command::from_json(&json!({"t":"board","ids":riders,"target":boat})).unwrap();
    assert_eq!(m.execute_command(0, &board, false).unwrap(), None);
    let mut delivery = DeliveryState::new(&m).unwrap();
    for _ in 0..40 {
        tick(&mut m, &mut delivery);
        if riders.iter().all(|id| m.world.game.units[id].riding) {
            break;
        }
    }
    assert_eq!(m.combat.get(boat).unwrap().cargo.len(), 1);
    sail(&mut m, &mut delivery, boat, Point { x: 101., z: 65. });
    assert_eq!(unload(&mut m, &[boat]), Some(Denial::Shore));
    for _ in 0..4 {
        tick(&mut m, &mut delivery);
    }
    assert!(riders.iter().all(|id| m.world.game.units[id].riding));
    sail(&mut m, &mut delivery, boat, Point { x: 119., z: 65. });
    assert_eq!(unload(&mut m, &[boat]), None);
    for id in riders {
        assert!(!m.world.game.units[&id].riding);
        assert_eq!(m.combat.get(id).unwrap().carrier, None);
        assert!(m.world.game.units[&id].x >= 120.);
    }
    assert!(m.combat.get(boat).unwrap().cargo.is_empty());
    for _ in 0..6 {
        tick(&mut m, &mut delivery);
    }
    crate::unit_placement::tests::assert_current(&m.world.game, &m.grid);
}
