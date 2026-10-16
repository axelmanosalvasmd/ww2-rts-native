use crate::{
    bases_runtime_construction_tests::{build, scene, ticks},
    environment::{Blast, Context, DamageContext, Effect},
    game::{CELL, Point},
    match_commands::{Command, Denial},
    match_runtime::Match,
    orders::OrderKey,
};
use serde_json::json;
use std::collections::BTreeSet;

fn command(m: &mut Match, owner: usize, raw: serde_json::Value) -> Option<Denial> {
    m.execute_command(owner, &Command::from_json(&raw).unwrap(), false)
        .unwrap()
}

fn damage(m: &mut Match, at: Point, cell: Option<usize>) {
    let order = m.world.game.units.keys().copied().collect::<Vec<_>>();
    let rain = m.world.game.rain;
    let wet = m.world.game.wet.unwrap_or(0.0);
    let mut context = Context {
        tick: m.world.game.tick as u64,
        seed: &mut m.start.environment_seed,
        next_id: &mut m.next_unit_id,
        unit_order: &order,
        grid: &m.grid,
        rain,
        wet,
        wind_angle: m.start.wind.0,
        wind_strength: m.start.wind.1,
    };
    let skip = BTreeSet::new();
    let mut hit = DamageContext::plain(&skip);
    if let Some(cell) = cell {
        m.environment
            .damage_section(&mut m.world, cell, 1000.0, &mut context, &hit)
            .unwrap();
    } else {
        m.environment
            .damage_cells(
                &mut m.world,
                Blast {
                    at,
                    radius: 0.0,
                    damage: 1000.0,
                    scar: 0.0,
                },
                &mut context,
                &mut hit,
            )
            .unwrap();
    }
}

#[test]
fn intent_base31_cancel_refunds_once_removes_sections_and_prevents_ghost_rubble() {
    let (mut m, worker) = scene();
    let site = build(&mut m, worker, "barracks", Point { x: 43.0, z: 43.0 });
    ticks(&mut m, 10);
    let structure = m
        .world
        .structures
        .iter()
        .find(|s| s.entity_id == Some(site))
        .unwrap()
        .clone();
    let cells = structure.cells;
    let failed = cells[0];
    damage(&mut m, Point { x: 0.0, z: 0.0 }, Some(failed));
    assert!(!m.environment.falling_sections().is_empty());
    assert!(m.world.game.units[&site].built.unwrap() < 1.0);
    let mp = m.start.economy.players[0].mp;
    let spent = m.story[0].mp_spent;
    let losses = m.story[0].losses;
    assert_eq!(
        command(&mut m, 1, json!({"t":"cancel","id":site})),
        Some(Denial::Unseen)
    );
    assert_eq!(m.start.economy.players[0].mp, mp);
    assert_eq!(command(&mut m, 0, json!({"t":"cancel","id":site})), None);
    assert_eq!(m.start.economy.players[0].mp, mp + 112.5);
    assert_eq!(m.story[0].mp_spent, spent - 112.5);
    assert_eq!(m.story[0].losses, losses);
    assert!(!m.world.game.units.contains_key(&site));
    assert!(!m.commands.units.contains_key(&site));
    assert!(m.production.building(site).is_none());
    assert!(!m.building_rallies.contains_key(&site));
    assert!(
        m.environment
            .falling_sections()
            .iter()
            .all(|f| !structure.section_ids.contains(&f.section))
    );
    for &cell in &cells {
        assert_eq!(m.world.game.chars[cell], b'.');
        assert_eq!(m.world.game.cell_hp[cell], 0.0);
        assert!(!m.world.sections.contains_key(&cell));
        assert!(!m.world.game.sections.contains_key(&(cell as u32)));
        assert!(m.world.cell_row(cell).unwrap().data.section.is_none());
        assert!(
            m.world
                .cell_log()
                .iter()
                .find(|r| r.cell == cell)
                .unwrap()
                .data
                .section
                .is_none()
        );
        let at = Point {
            x: (cell % m.world.game.w) as f64 * CELL + 1.0,
            z: (cell / m.world.game.w) as f64 * CELL + 1.0,
        };
        damage(&mut m, at, None);
    }
    assert_eq!(
        command(&mut m, 0, json!({"t":"cancel","id":site})),
        Some(Denial::Unseen)
    );
    assert_eq!(m.start.economy.players[0].mp, mp + 112.5);
    for _ in 0..60 {
        let report = m.advance_tick().unwrap();
        assert!(
            !report
                .environment()
                .any(|e| matches!(e, Effect::BuildingCollapse { unit, .. } if *unit == site))
        );
    }
    assert!(cells.iter().all(|c| m.world.game.chars[*c] == b'.'));
    assert!(m.commands.units[&worker].build_target.is_none());
    let replacement = build(&mut m, worker, "barracks", Point { x: 43.0, z: 43.0 });
    assert_ne!(replacement, site);
}

#[test]
fn cancellation_refuses_finished_sites_and_stages_wallet_and_terrain_atomically() {
    let (mut m, worker) = scene();
    let hq = m
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 0 && m.rules.kinds[u.kind].name == "hq")
        .unwrap()
        .id;
    assert_eq!(
        command(&mut m, 0, json!({"t":"cancel","id":hq})),
        Some(Denial::Unseen)
    );
    let site = build(&mut m, worker, "barracks", Point { x: 43.0, z: 43.0 });
    let before = format!(
        "{:?}",
        (
            &m.world,
            &m.start.economy,
            &m.production,
            &m.story,
            &m.commands.units
        )
    );
    let old_spent = m.story[0].mp_spent;
    // Force a late transaction guard after the detached physical draft is prepared.
    m.story[1].kills = f64::NAN;
    assert!(
        m.execute_command(
            0,
            &Command::CancelSite {
                id: i64::from(site)
            },
            false
        )
        .is_err()
    );
    assert_eq!(m.story[0].mp_spent, old_spent);
    m.story[1].kills = 0.0;
    assert_eq!(
        format!(
            "{:?}",
            (
                &m.world,
                &m.start.economy,
                &m.production,
                &m.story,
                &m.commands.units
            )
        ),
        before
    );
}

#[test]
fn cancel_rebuild_cycles_keep_latest_terrain_rows_and_immutable_emissions() {
    let (mut m, worker) = scene();
    let mut first = Vec::new();
    let mut frozen = Vec::new();
    let mut cells = Vec::new();
    for cycle in 0..20 {
        let site = build(&mut m, worker, "barracks", Point { x: 43.0, z: 43.0 });
        cells = m
            .world
            .structures
            .iter()
            .find(|s| s.entity_id == Some(site))
            .unwrap()
            .cells
            .clone();
        if cycle == 0 {
            first = m.world.take_new_cells();
            frozen = first.clone();
        }
        assert_eq!(command(&mut m, 0, json!({"t":"cancel","id":site})), None);
    }
    assert_eq!(first, frozen);
    let log = m.world.cell_log();
    assert_eq!(
        log.len(),
        log.iter().map(|r| r.cell).collect::<BTreeSet<_>>().len()
    );
    for cell in cells {
        assert_eq!(log.iter().find(|r| r.cell == cell).unwrap().ch, b'.');
        assert!(
            log.iter()
                .find(|r| r.cell == cell)
                .unwrap()
                .data
                .section
                .is_none()
        );
    }
}

#[test]
fn intent_base23_recorded_rallies_route_real_recruits_with_building_precedence() {
    let (mut m, _) = scene();
    let hq = m
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 0 && m.rules.kinds[u.kind].name == "hq")
        .unwrap()
        .id;
    let personal = Point { x: 31.0, z: 33.0 };
    let building = Point { x: 49.0, z: 43.0 };
    for (sequence, raw) in [
        json!({"t":"rally","x":personal.x,"z":personal.z}),
        json!({"t":"buy","unit":"rifle","from":hq}),
        json!({"t":"rally","id":hq,"x":building.x,"z":building.z}),
        json!({"t":"buy","unit":"rifle","from":hq}),
    ]
    .into_iter()
    .enumerate()
    {
        m.enqueue_command(
            OrderKey {
                tick: 1,
                issuer: 0,
                sequence: sequence as u64,
            },
            Command::from_json(&raw).unwrap(),
        )
        .unwrap();
    }
    let next = m.next_unit_id;
    let report = m.advance_tick().unwrap();
    assert!(report.commands.iter().all(|r| r.denial.is_none()));
    assert_eq!(m.player_rallies[0], Some(personal));
    assert_eq!(m.building_rallies[&hq], Some(building));
    assert_eq!(m.world.game.units[&next].world_goal, Some(personal));
    assert_eq!(m.world.game.units[&(next + 1)].world_goal, Some(building));
    assert!(m.commands.units[&next].rally_drift);
    assert!(m.commands.units[&(next + 1)].rally_drift);
    let mut arrivals = BTreeSet::new();
    for _ in 0..500 {
        m.advance_tick().unwrap();
        for (id, at) in [(next, personal), (next + 1, building)] {
            if matches!(
                m.commands.units[&id].move_result,
                Some(crate::match_commands::MoveResult::Completed { .. })
            ) && arrivals.insert(id)
            {
                let unit = &m.world.game.units[&id];
                assert!(
                    crate::math::hypot(unit.x - at.x, unit.z - at.z) < 2.0,
                    "first arrival for {id}: {:?}, expected {at:?}",
                    unit.at()
                );
                assert_eq!(m.commands.units[&id].route_end, Some(at));
            }
        }
        if arrivals.len() == 2 {
            break;
        }
    }
    assert_eq!(arrivals, BTreeSet::from([next, next + 1]));
}

#[test]
fn rally_selection_denials_and_bounds_preserve_personal_and_producer_state() {
    let (mut m, worker) = scene();
    let site = build(&mut m, worker, "barracks", Point { x: 43.0, z: 43.0 });
    assert_eq!(
        command(&mut m, 0, json!({"t":"rally","ids":[site],"x":-8,"z":9999})),
        None
    );
    assert_eq!(
        m.building_rallies[&site],
        Some(Point {
            x: 0.0,
            z: m.world.game.h as f64 * CELL
        })
    );
    let before = (m.player_rallies.clone(), m.building_rallies.clone());
    for (owner, raw, expected) in [
        (
            1,
            json!({"t":"rally","id":site,"x":20,"z":20}),
            Denial::Unseen,
        ),
        (
            0,
            json!({"t":"rally","id":worker,"x":20,"z":20}),
            Denial::Needs,
        ),
        (
            0,
            json!({"t":"rally","id":null,"x":20,"z":20}),
            Denial::Unseen,
        ),
        (0, json!({"t":"rally","x":-1,"z":20}), Denial::Blocked),
        (0, json!({"t":"rally","x":9999,"z":20}), Denial::Blocked),
        (0, json!({"t":"rally","x":43,"z":43}), Denial::Blocked),
    ] {
        assert_eq!(command(&mut m, owner, raw), Some(expected));
        assert_eq!(
            (m.player_rallies.clone(), m.building_rallies.clone()),
            before
        );
    }
    assert_eq!(
        Command::from_json(&json!({"t":"rally","x":"bad","z":1})),
        Err(Denial::Blocked)
    );
    assert_eq!(
        m.execute_command(
            0,
            &Command::Rally {
                ids: vec![],
                id: None,
                at: Point {
                    x: f64::NAN,
                    z: 1.0
                }
            },
            false
        )
        .unwrap(),
        Some(Denial::Blocked)
    );
}

#[test]
fn cancel_accepts_a_site_after_an_admitted_replacement_takes_a_failed_corner() {
    let (mut m, worker) = scene();
    let site = build(&mut m, worker, "barracks", Point { x: 43.0, z: 43.0 });
    let original = m
        .world
        .structures
        .iter()
        .find(|s| s.entity_id == Some(site))
        .unwrap()
        .clone();
    let cell = original.cells[0];
    damage(&mut m, Point { x: 0.0, z: 0.0 }, Some(cell));
    assert_eq!(m.world.game.objects.as_ref().unwrap()[cell], b'.');
    let at = Point {
        x: (cell % m.world.game.w) as f64 * CELL + 1.0,
        z: (cell / m.world.game.w) as f64 * CELL + 1.0,
    };
    let tower = build(&mut m, worker, "tower", at);
    let replacement = m.world.sections[&cell].id.clone();
    assert!(
        m.world
            .detached_sections
            .contains_key(&original.section_ids[0])
    );
    m.enqueue_command(
        OrderKey {
            tick: m.world.game.tick as u64 + 1,
            issuer: 0,
            sequence: 1,
        },
        Command::CancelSite {
            id: i64::from(site),
        },
    )
    .unwrap();
    let report = m.advance_tick().unwrap();
    assert_eq!(report.commands.len(), 1);
    assert_eq!(report.commands[0].denial, None);
    assert!(!m.world.game.units.contains_key(&site));
    assert!(m.world.game.units.contains_key(&tower));
    assert_eq!(m.world.sections[&cell].id, replacement);
    assert!(
        original
            .section_ids
            .iter()
            .all(|id| m.world.section_by_id(id).is_none())
    );
    // Source clears the original footprint but retains a replacement's section graph.
    // This accepted construction history must not fault the subsequent runner.
    ticks(&mut m, 2);
}
