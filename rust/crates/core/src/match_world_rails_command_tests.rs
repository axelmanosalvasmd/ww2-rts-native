//! Rail command intent checks use the existing World core component, without room admission.

use super::*;
use crate::match_commands::Command;

const ORIGIN: Point = Point { x: 65.0, z: 65.0 };
const DESTINATION: Point = Point { x: 193.0, z: 65.0 };

fn actor(m: &mut Match, owner: usize, at: Point) -> UnitId {
    let id = m.next_unit_id;
    let draft = StagedUnit::ordinary_with_actor(
        &m.world.game,
        &m.rules,
        SpawnActor::Commander {
            owner,
            spawn: m.start.players[owner].spawn,
        },
        m.rules.kind("conscript").unwrap(),
        id,
        0,
        Some(at),
    )
    .unwrap();
    prepare_world_publication(
        PublicationParts {
            units: &mut m.world.game.units,
            grid: &mut m.grid,
            controls: &mut m.commands.units,
            recruitment: &mut m.recruit_metadata,
            next_id: &mut m.next_unit_id,
        },
        &m.rules,
        m.start.players.len(),
        Some(ModeKind::World),
        vec![draft],
    )
    .unwrap()
    .publish_with(|unit, _| {
        m.combat.initialize(unit.id);
        m.firing.initialize(unit.id);
        m.abilities.initialize(unit.id);
        m.recovery.initialize(unit.id);
    });
    id
}

fn rail(m: &mut Match, owner: usize, ids: &[UnitId], at: Point) -> Option<Denial> {
    m.execute_command(
        owner,
        &Command::Rail {
            ids: ids.iter().map(|&id| i64::from(id)).collect(),
            at,
        },
        false,
    )
    .unwrap()
}

#[test]
fn mov81_command_boards_eight_nearest_and_clears_old_orders() {
    assert_eq!(Denial::Station.name(), "station");
    assert_eq!(Denial::RailCut.name(), "railCut");
    let mut m = super::intent_tests::world_for_rail_orders();
    let ids: Vec<_> = [30.001, 30.0, 8.0, 7.0, 6.0, 5.0, 4.0, 3.0, 2.0, 1.0]
        .into_iter()
        .map(|distance| {
            actor(
                &mut m,
                0,
                Point {
                    x: 65.0 + distance,
                    z: 65.0,
                },
            )
        })
        .collect();
    for &id in &ids {
        let u = m.world.game.units.get_mut(&id).unwrap();
        u.path = vec![DESTINATION];
        u.world_goal = Some(DESTINATION);
        u.board = ids[0];
        let c = m.commands.units.get_mut(&id).unwrap();
        c.attack_id = ids[0];
        c.nade = Some(DESTINATION);
        c.fire_at = Some(1);
        c.follow = ids[0];
    }
    assert_eq!(rail(&mut m, 0, &ids, DESTINATION), None);
    let train = m.next_unit_id - 1;
    let expected: Vec<_> = ids[2..].iter().rev().copied().collect();
    assert_eq!(m.combat.get(train).unwrap().cargo, expected);
    assert_eq!(m.rail_runs[&train].destination, 1);
    for &id in &expected {
        let u = &m.world.game.units[&id];
        let c = &m.commands.units[&id];
        assert!(u.riding);
        assert_eq!(m.combat.get(id).unwrap().carrier, Some(train));
        assert!(u.path.is_empty());
        assert_eq!(u.world_goal, None);
        assert_eq!(u.board, 0);
        assert_eq!(c.attack_id, 0);
        assert_eq!(c.nade, None);
        assert_eq!(c.fire_at, None);
        assert_eq!(c.follow, 0);
        assert_eq!(c.outcome, Some(MoveOutcome::Interrupted));
    }
    for &id in &ids[..2] {
        assert!(!m.world.game.units[&id].riding);
        assert_eq!(m.combat.get(id).unwrap().carrier, None);
        assert_eq!(m.commands.units[&id].follow, 0);
    }
}

#[test]
fn mov81_command_excludes_ineligible_actors_and_accepts_thirty_meter_boundary() {
    for case in [
        "enemy",
        "air",
        "naval",
        "structure",
        "operative",
        "riding",
        "rail",
        "forced",
    ] {
        let mut m = super::intent_tests::world_for_rail_orders();
        let id = actor(&mut m, 0, ORIGIN);
        match case {
            "enemy" => m.world.game.units.get_mut(&id).unwrap().owner = 1,
            "air" | "naval" | "structure" | "rail" => {
                let kind = m
                    .rules
                    .kinds
                    .iter()
                    .position(|kind| match case {
                        "air" => kind.air,
                        "naval" => kind.naval,
                        "structure" => kind.structure,
                        "rail" => kind.rail,
                        _ => unreachable!(),
                    })
                    .unwrap();
                m.world.game.units.get_mut(&id).unwrap().kind = kind;
            }
            "operative" => m.commands.units.get_mut(&id).unwrap().operative = true,
            "riding" => m.world.game.units.get_mut(&id).unwrap().riding = true,
            "forced" => {
                m.logistics.insert(
                    id,
                    crate::logistics::Reserves {
                        ammo: 10.0,
                        ammo_max: 10.0,
                        ammo_price: 1.0,
                        ammo_credit: None,
                        provisions: 0.0,
                        fuel: None,
                        emergency: None,
                        warning: None,
                        forced: true,
                        stranded: true,
                        exhausted: true,
                        supply_rate: None,
                        cut_for: None,
                    },
                );
            }
            _ => unreachable!(),
        }
        let next_id = m.next_unit_id;
        m.commands.units.get_mut(&id).unwrap().follow = 123;
        assert_eq!(
            rail(&mut m, 0, &[id], DESTINATION),
            Some(Denial::Needs),
            "{case}"
        );
        assert_eq!(
            m.commands.units[&id].follow,
            if matches!(case, "air" | "naval") {
                0
            } else {
                123
            },
            "{case}"
        );
        assert_eq!(m.next_unit_id, next_id, "{case}");
        assert!(m.rail_runs.is_empty(), "{case}");
    }
    for (distance, expected) in [(30.0, None), (30.001, Some(Denial::Station))] {
        let mut m = super::intent_tests::world_for_rail_orders();
        let id = actor(
            &mut m,
            0,
            Point {
                x: ORIGIN.x + distance,
                z: ORIGIN.z,
            },
        );
        assert_eq!(rail(&mut m, 0, &[id], DESTINATION), expected);
        assert_eq!(m.world.game.units[&id].riding, expected.is_none());
    }
}

#[test]
fn mov81_command_selects_owned_stations_and_checks_live_regions_and_bridges() {
    for case in [
        "no stations",
        "no origin",
        "no destination",
        "enemy region",
        "broken bridge",
    ] {
        let mut m = super::intent_tests::world_for_rail_orders();
        let id = actor(&mut m, 0, ORIGIN);
        let runtime = m.world_runtime.as_mut().unwrap();
        let expected = match case {
            "no stations" => {
                runtime.rails.stations.clear();
                Denial::Blocked
            }
            "no origin" => {
                runtime.regions[0].team = 1;
                Denial::Station
            }
            "no destination" => {
                runtime.regions[1].team = 1;
                Denial::Station
            }
            "enemy region" => {
                runtime.rails.lines[0].regions.push(2);
                runtime.regions[2].team = 1;
                Denial::RailCut
            }
            "broken bridge" => {
                let cell = runtime.rails.lines[0].cells[10];
                runtime.rails.lines[0].bridges.push(cell);
                m.world.game.chars[cell] = b'W';
                Denial::RailCut
            }
            _ => unreachable!(),
        };
        let next_id = m.next_unit_id;
        assert_eq!(
            rail(&mut m, 0, &[id], DESTINATION),
            Some(expected),
            "{case}"
        );
        assert_eq!(m.next_unit_id, next_id);
        assert!(
            m.world_runtime
                .as_ref()
                .unwrap()
                .rails
                .stations
                .iter()
                .all(|station| station.next.is_empty())
        );
    }
    let mut m = super::intent_tests::world_for_rail_orders();
    let id = actor(&mut m, 0, DESTINATION);
    assert_eq!(rail(&mut m, 0, &[id], ORIGIN), None);
    assert_eq!(m.rail_runs[&(m.next_unit_id - 1)].destination, 0);
    assert_eq!(
        m.world_runtime.as_ref().unwrap().rails.stations[1].next[&0],
        400
    );
    assert!(
        m.world_runtime.as_ref().unwrap().rails.stations[0]
            .next
            .is_empty()
    );

    let mut m = super::intent_tests::world_for_rail_orders();
    let id = actor(&mut m, 0, ORIGIN);
    let runtime = m.world_runtime.as_mut().unwrap();
    runtime.regions[2].team = 0;
    runtime.regions[3].team = 1;
    runtime.rails.stations.extend([
        WorldStation {
            region: 2,
            cell: 32 * 512 + 160,
            x: 321.0,
            z: 65.0,
            next: BTreeMap::new(),
        },
        WorldStation {
            region: 3,
            cell: 32 * 512 + 161,
            x: 323.0,
            z: 65.0,
            next: BTreeMap::new(),
        },
    ]);
    runtime.rails.lines.push(WorldRailLine {
        a: 1,
        b: 2,
        cells: (96..=160).map(|x| 32 * 512 + x).collect(),
        regions: vec![1, 2],
        bridges: Vec::new(),
    });
    assert_eq!(rail(&mut m, 0, &[id], Point { x: 323.0, z: 65.0 }), None);
    let run = &m.rail_runs[&(m.next_unit_id - 1)];
    assert_eq!(run.destination, 2);
    assert_eq!(run.path.last().unwrap().cell, 32 * 512 + 160);
}

#[test]
fn mov81_command_cooldown_is_twenty_seconds_per_station_and_team() {
    let mut m = super::intent_tests::world_for_rail_orders();
    let first = actor(&mut m, 0, ORIGIN);
    let waiting = actor(&mut m, 0, ORIGIN);
    assert_eq!(rail(&mut m, 0, &[first], DESTINATION), None);
    assert_eq!(
        m.world_runtime.as_ref().unwrap().rails.stations[0].next[&0],
        400
    );
    m.world.game.tick = 399;
    assert_eq!(
        rail(&mut m, 0, &[waiting], DESTINATION),
        Some(Denial::Cooldown)
    );
    let other_origin = actor(&mut m, 0, DESTINATION);
    assert_eq!(rail(&mut m, 0, &[other_origin], ORIGIN), None);
    m.world.game.tick = 400;
    assert_eq!(rail(&mut m, 0, &[waiting], DESTINATION), None);
    assert_eq!(
        m.world_runtime.as_ref().unwrap().rails.stations[0].next[&0],
        800
    );
    let other_team = actor(&mut m, 1, ORIGIN);
    for region in &mut m.world_runtime.as_mut().unwrap().regions {
        region.team = 1;
    }
    assert_eq!(rail(&mut m, 1, &[other_team], DESTINATION), None);
    let next = &m.world_runtime.as_ref().unwrap().rails.stations[0].next;
    assert_eq!(next[&0], 800);
    assert_eq!(next[&1], 800);
}

#[test]
fn mov81_command_semantic_refusals_clear_follow_but_validation_errors_preserve_it() {
    for (case, expected) in [
        ("station", Some(Denial::Station)),
        ("cooldown", Some(Denial::Cooldown)),
        ("cut", Some(Denial::RailCut)),
        ("success", None),
    ] {
        let mut m = super::intent_tests::world_for_rail_orders();
        let ground = actor(&mut m, 0, ORIGIN);
        let air = actor(&mut m, 0, ORIGIN);
        let naval = actor(&mut m, 0, ORIGIN);
        let foreign = actor(&mut m, 1, ORIGIN);
        m.world.game.units.get_mut(&air).unwrap().kind = m
            .rules
            .kinds
            .iter()
            .position(|kind| kind.air && !kind.structure)
            .unwrap();
        m.world.game.units.get_mut(&naval).unwrap().kind = m
            .rules
            .kinds
            .iter()
            .position(|kind| kind.naval && !kind.structure)
            .unwrap();
        for id in [ground, air, naval, foreign] {
            m.commands.units.get_mut(&id).unwrap().follow = 123;
        }
        let runtime = m.world_runtime.as_mut().unwrap();
        match case {
            "station" => runtime.regions[1].team = 1,
            "cooldown" => {
                runtime.rails.stations[0].next.insert(0, 1);
            }
            "cut" => runtime.rails.lines.clear(),
            "success" => {}
            _ => unreachable!(),
        }
        assert_eq!(
            rail(&mut m, 0, &[ground, air, naval, foreign], DESTINATION),
            expected,
            "{case}"
        );
        for id in [ground, air, naval] {
            assert_eq!(m.commands.units[&id].follow, 0, "{case}: {id}");
        }
        assert_eq!(m.commands.units[&foreign].follow, 123);
    }

    let mut m = super::intent_tests::world_for_rail_orders();
    let id = actor(&mut m, 0, ORIGIN);
    m.commands.units.get_mut(&id).unwrap().follow = 123;
    m.world.game.units.get_mut(&id).unwrap().path_gen = u64::MAX;
    let next_id = m.next_unit_id;
    assert!(
        m.execute_command(
            0,
            &Command::Rail {
                ids: vec![i64::from(id)],
                at: DESTINATION
            },
            false
        )
        .is_err()
    );
    assert_eq!(m.commands.units[&id].follow, 123);
    assert_eq!(m.next_unit_id, next_id);
    assert!(!m.world.game.units[&id].riding);
    assert!(m.rail_runs.is_empty());
    assert!(
        m.world_runtime.as_ref().unwrap().rails.stations[0]
            .next
            .is_empty()
    );
}
