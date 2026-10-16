use crate::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    field_works::{FortKind, Pattern, entrench_plan},
    game::Point,
    map::AuthoritativeMap,
    match_commands::{Command, Denial},
    match_runtime::{Match, MatchOptions, PlayerConfig},
    recipient_snapshot::DeliveryState,
    weather::WeatherChoice,
};
use serde_json::json;

fn scene() -> (Match, u32) {
    let map = AuthoritativeMap::from_value(json!({
        "name":"Concrete runtime", "w":60,"h":60,"rows":vec![".".repeat(60);60],
        "heights":vec!["0".repeat(60);60],
        "spawns":[{"x":4,"y":4},{"x":55,"y":55}],"points":[{"x":30,"y":30}]
    }))
    .unwrap();
    let mut m = Match::new(
        &map,
        MatchOptions {
            seed: 701,
            mode: Mode::Default,
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
    let id = m
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 0 && m.rules.kinds[u.kind].name == "rifle")
        .unwrap()
        .id;
    let u = m.world.game.units.get_mut(&id).unwrap();
    u.x = 30.;
    u.z = 20.;
    m.grid.update(id, u);
    m.start.economy.players[0].mp = 500.;
    (m, id)
}

fn command(m: &mut Match, id: u32, kind: &str) -> Option<Denial> {
    m.execute_command(
        0,
        &Command::from_json(&json!({
            "t":"dig","ids":[id],"kind":kind,"x":30.,"z":25.,"dir":0.
        }))
        .unwrap(),
        false,
    )
    .unwrap()
}

fn tick_and_capture(m: &mut Match, delivery: &mut DeliveryState) {
    m.advance_tick().unwrap();
    delivery
        .capture(m, &[false; 2], &[false; 2])
        .unwrap()
        .recipient(0)
        .unwrap();
    assert!(m.tick_state.fault().is_none());
}

#[test]
fn eng11_concrete_line_becomes_four_finished_buildings_and_survives_match_ticks() {
    let (mut m, id) = scene();
    m.options.tech = true;
    assert_eq!(command(&mut m, id, "wall"), Some(Denial::Tier));
    assert_eq!(m.start.economy.players[0].mp, 500.);
    m.production =
        crate::production::ProductionState::new(vec![
            crate::economy::TechState::new(2, 0, 0, 0)
                .unwrap();
            2
        ])
        .unwrap();
    for u in m
        .world
        .game
        .units
        .values()
        .filter(|u| m.rules.kinds[u.kind].building)
    {
        m.production.initialize_building(u.id).unwrap();
    }
    let before = m.next_unit_id;
    assert_eq!(
        m.execute_dig(
            0,
            &[i64::from(id)],
            FortKind::Wall,
            Point { x: 30., z: 25. },
            Some(0.25),
            false
        )
        .unwrap(),
        None
    );
    assert_eq!(m.start.economy.players[0].mp, 380.);
    let cells = m.field_works.jobs()[&id]
        .cells
        .iter()
        .map(|c| c.cell)
        .collect::<Vec<_>>();
    assert_eq!(cells.len(), 4);
    assert_eq!(m.field_works.jobs()[&id].reach, Some(6.));
    let mut delivery = DeliveryState::new(&m).unwrap();
    for _ in 0..3 {
        tick_and_capture(&mut m, &mut delivery);
    }
    assert!(m.world.game.units[&id].dig);
    assert_eq!(m.next_unit_id, before);
    for _ in 0..490 {
        m.advance_tick().unwrap();
    }
    assert!(!m.world.game.units[&id].dig);
    assert_eq!(m.next_unit_id, before + 4);
    for cell in cells {
        let wall = &m.world.game.units[&m.building_cells[&cell].unit];
        assert_eq!(m.rules.kinds[wall.kind].name, "wall");
        assert_eq!(wall.owner, 0);
        assert_eq!(wall.built, Some(1.));
        assert_eq!(wall.hp, 400.);
        assert_eq!(wall.rot, 0.25);
        assert_eq!(wall.cells.as_deref(), Some([cell as u32].as_slice()));
        assert_eq!(m.world.game.chars[cell], b'K');
        assert_eq!(
            m.world.sections[&cell].structure_id,
            format!("building:{}", wall.id)
        );
    }
    for _ in 0..3 {
        tick_and_capture(&mut m, &mut delivery);
    }
    assert_eq!(m.story[0].built, 1.);
}

#[test]
fn eng11_six_second_cells_and_permanent_occupancy_have_a_bounded_lifecycle() {
    let (mut m, id) = scene();
    assert_eq!(command(&mut m, id, "wall"), None);
    let first = m.field_works.jobs()[&id].cells[0].cell;
    m.apply_field_work_unit_dt(id, 5.99).unwrap();
    assert_eq!(m.world.game.chars[first], b'.');
    m.apply_field_work_unit_dt(id, 0.01).unwrap();
    assert_eq!(m.world.game.chars[first], b'K');
    let first_wall = m.world.game.units[&m.building_cells[&first].unit].clone();
    assert_eq!(first_wall.rot, 0.);

    let (mut m, id) = scene();
    // Only one buildable cell remains, permanently held by another grounded squad.
    for x in [14, 15, 16] {
        m.world.set_cell(12 * 60 + x, b'T').unwrap();
    }
    let blocker = m
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 0 && u.id != id && !m.rules.kinds[u.kind].structure)
        .unwrap()
        .id;
    let u = m.world.game.units.get_mut(&blocker).unwrap();
    u.x = 27.;
    u.z = 25.;
    u.hold_pos = true;
    m.grid.update(blocker, u);
    assert_eq!(command(&mut m, id, "wall"), None);
    assert_eq!(m.field_works.jobs()[&id].cells.len(), 1);
    m.apply_field_work_unit_dt(id, 6.).unwrap();
    assert_eq!(m.field_works.jobs()[&id].waits, 1);
    let mut delivery = DeliveryState::new(&m).unwrap();
    for _ in 0..3 {
        tick_and_capture(&mut m, &mut delivery);
    }
    for waits in 2..20 {
        m.apply_field_work_unit_dt(id, 6.).unwrap();
        assert_eq!(m.field_works.jobs()[&id].waits, waits);
    }
    m.apply_field_work_unit_dt(id, 6.).unwrap();
    assert!(!m.field_works.is_active(id));
    assert!(!m.world.game.units[&id].dig);
    assert_eq!(m.world.game.chars[12 * 60 + 13], b'.');
    assert!(
        !m.world
            .game
            .units
            .values()
            .any(|u| m.rules.kinds[u.kind].name == "wall")
    );
    for _ in 0..3 {
        tick_and_capture(&mut m, &mut delivery);
    }
}

#[test]
fn eng11_construction_mode_component_requires_engineers_for_concrete() {
    for mode in [crate::game::ModeKind::Classic, crate::game::ModeKind::World] {
        let (mut m, id) = scene();
        m.world.game.mode = Some(mode);
        let request = crate::field_works::StartRequest {
            control_eligible: true,
            owner: 0,
            unit: id,
            kind: FortKind::Wall,
            at: Point { x: 30., z: 25. },
            direction: Some(0.),
        };
        assert!(matches!(
            m.field_works
                .prepare_start(&m.world, &mut m.start.economy.players[0], request, |_| true),
            Err(crate::field_works::Error::Refused(
                crate::field_works::Reason::NoBuilders
            ))
        ));
        m.world.game.units.get_mut(&id).unwrap().kind = m.rules.kind("engineer").unwrap();
        assert!(
            m.field_works
                .prepare_start(&m.world, &mut m.start.economy.players[0], request, |_| true)
                .is_ok()
        );
        // Room admission remains guarded even though the component knows its builder rule.
        assert!(
            m.execute_dig(
                0,
                &[i64::from(id)],
                FortKind::Wall,
                request.at,
                Some(0.),
                false
            )
            .is_err()
        );
    }
}

#[test]
fn eng11_occupied_concrete_retries_then_steps_the_crew_four_metres_back() {
    let (mut m, id) = scene();
    let u = m.world.game.units.get_mut(&id).unwrap();
    u.x = 27.;
    u.z = 25.;
    m.grid.update(id, u);
    assert_eq!(command(&mut m, id, "wall"), None);
    m.begin_path_tick(0, 20).unwrap();
    m.apply_field_work_unit_dt(id, 6.).unwrap();
    let job = &m.field_works.jobs()[&id];
    assert_eq!(job.waits, 1);
    assert_eq!(job.cells.len(), 4);
    assert_eq!(job.step_back, Some(Point { x: 27., z: 29. }));
    assert!(m.world.game.units[&id].path.is_empty());
    assert_eq!(m.next_unit_id, m.world.game.units.keys().max().unwrap() + 1);
    let mut delivery = DeliveryState::new(&m).unwrap();
    delivery
        .capture(&mut m, &[false; 2], &[false; 2])
        .unwrap()
        .recipient(0)
        .unwrap();
    tick_and_capture(&mut m, &mut delivery);
    assert!(!m.world.game.units[&id].path.is_empty());
    for _ in 0..3 {
        tick_and_capture(&mut m, &mut delivery);
    }
    for _ in 0..650 {
        m.advance_tick().unwrap();
    }
    assert!(!m.world.game.units[&id].dig);
    assert_eq!(
        m.world
            .game
            .units
            .values()
            .filter(|u| m.rules.kinds[u.kind].name == "wall")
            .count(),
        4
    );
    tick_and_capture(&mut m, &mut delivery);
}

#[test]
fn eng24_fill_and_wall_line_patterns_keep_material_and_middle_first_order() {
    for kind in [
        FortKind::Trench,
        FortKind::Sandbags,
        FortKind::Wire,
        FortKind::Traps,
        FortKind::Mines,
        FortKind::Demine,
        FortKind::Fill,
        FortKind::Wall,
    ] {
        for pattern in [Pattern::Line, Pattern::Zigzag, Pattern::Double] {
            let plan = entrench_plan(
                pattern,
                Point { x: 20., z: 25. },
                Point { x: 44., z: 25. },
                Some(Point { x: 30., z: 20. }),
                kind,
            )
            .unwrap();
            assert!(plan.iter().all(|s| s.kind == kind));
        }
        for pattern in [Pattern::Arc, Pattern::Ring, Pattern::Strongpoint] {
            assert_eq!(
                entrench_plan(
                    pattern,
                    Point { x: 20., z: 25. },
                    Point { x: 44., z: 25. },
                    None,
                    kind
                )
                .is_ok(),
                kind == FortKind::Trench
            );
        }
    }
    for (kind, xs) in [
        (FortKind::Wall, vec![32., 24., 40.]),
        (FortKind::Fill, vec![27., 37.]),
    ] {
        let plan = entrench_plan(
            Pattern::Line,
            Point { x: 20., z: 25. },
            Point { x: 44., z: 25. },
            None,
            kind,
        )
        .unwrap();
        assert_eq!(plan.iter().map(|s| s.at.x).collect::<Vec<_>>(), xs);
    }
    for kind in ["wall", "fill"] {
        let (mut m, id) = scene();
        if kind == "fill" {
            for cell in 12 * 60 + 12..12 * 60 + 18 {
                m.world.set_cell(cell, b'+').unwrap();
            }
        }
        let order = Command::from_json(
            &json!({"t":"entrench","ids":[id],"pattern":"line","fort":kind,"x":30.,"z":25.}),
        )
        .unwrap();
        assert_eq!(m.execute_command(0, &order, false).unwrap(), None);
        assert_eq!(m.start.economy.players[0].mp, 500.);
        let mut delivery = DeliveryState::new(&m).unwrap();
        tick_and_capture(&mut m, &mut delivery);
        let job = &m.field_works.jobs()[&id];
        assert_eq!(job.kind.as_str(), kind);
        assert_eq!(job.cells.len(), if kind == "wall" { 4 } else { 5 });
        let cells = job.cells.iter().map(|c| c.cell).collect::<Vec<_>>();
        for _ in 0..510 {
            m.advance_tick().unwrap();
        }
        tick_and_capture(&mut m, &mut delivery);
        assert!(!m.world.game.units[&id].dig);
        for cell in cells {
            assert_eq!(
                m.world.game.chars[cell],
                if kind == "wall" { b'K' } else { b'.' }
            );
        }
    }
}
