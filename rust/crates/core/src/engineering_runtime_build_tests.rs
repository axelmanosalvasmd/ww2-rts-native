use super::*;
use crate::{
    environment::{Context, DamageContext},
    game::{MOVE, Pose, VBLOCK},
};
use std::collections::BTreeSet;

fn river_match() -> Match {
    let mut rows = vec!["..........WWWWW........................."; 40];
    // Keep the authored spawn and objective sites on dry ground.
    rows[0] = "........................................";
    make(&json!({
        "map": {"name":"Runtime bridge", "w":40, "h":40, "rows":rows,
            "spawns":[{"x":0,"y":0},{"x":39,"y":39}],"points":[{"x":20,"y":20}]},
        "units":[{"id":1,"type":"rifle","x":15.,"z":21.}]
    }))
}

fn command(m: &mut Match, kind: &str, at: Point, direction: Option<f64>) -> Option<Denial> {
    let raw = json!({"t":"dig","ids":[1],"kind":kind,"x":at.x,"z":at.z,"dir":direction});
    m.execute_command(0, &Command::from_json(&raw).unwrap(), false)
        .unwrap()
}

fn complete(m: &mut Match) {
    for _ in 0..5 {
        m.apply_field_work_unit_dt(1, 3.).unwrap();
    }
    assert!(!m.world.game.units[&1].dig);
}

fn destroy(m: &mut Match, c: usize) {
    let ids: Vec<_> = m.world.game.units.keys().copied().collect();
    let mut ctx = Context {
        tick: m.world.game.tick as u64,
        seed: &mut m.start.environment_seed,
        next_id: &mut m.next_unit_id,
        unit_order: &ids,
        grid: &m.grid,
        rain: 0.,
        wet: 0.,
        wind_angle: 0.,
        wind_strength: 0.,
    };
    m.environment
        .damage_section(
            &mut m.world,
            c,
            5000.,
            &mut ctx,
            &DamageContext::plain(&BTreeSet::new()),
        )
        .unwrap();
}

#[test]
fn eng07_paid_bridge_builds_five_cells_from_the_builders_bank() {
    let mut m = river_match();
    assert_eq!(
        command(&mut m, "bridge", Point { x: 25., z: 21. }, None),
        None
    );
    assert_eq!(m.start.economy.players[0].mp, 420.);
    let job = &m.field_works.jobs()[&1];
    assert_eq!(job.direction, 0.);
    assert_eq!(job.reach, Some(9.));
    assert_eq!(
        job.cells.iter().map(|c| c.cell).collect::<Vec<_>>(),
        (410..415).collect::<Vec<_>>()
    );
    let goal = m.current_dig_goal(1).unwrap();
    assert_eq!(goal, Point { x: 18., z: 21. });
    assert_eq!(
        m.world
            .game
            .physical(m.world.game.cell_of(goal.x, goal.z).unwrap()),
        b'.'
    );
    m.world.game.units.get_mut(&1).unwrap().x = goal.x;
    let unit = m.world.game.units[&1].clone();
    m.grid.update(1, &unit);
    m.apply_field_work_unit_dt(1, 2.99).unwrap();
    assert_eq!(m.world.game.chars[410], b'W');
    m.apply_field_work_unit_dt(1, 0.01).unwrap();
    assert_eq!(m.world.game.chars[410], b'=');
    for _ in 0..4 {
        m.apply_field_work_unit_dt(1, 3.).unwrap();
    }
    assert_eq!(&m.world.game.chars[410..415], b"=====");
    assert!(m.world.game.units[&1].x < 20.);
    assert_eq!(
        m.world
            .structures
            .iter()
            .filter(|s| s.kind == "bridge")
            .count(),
        1
    );
    assert_eq!(m.world.game.sections[&410].hp, 200.);
    assert!(!m.world.game.units[&1].dig);
}

#[test]
fn eng07_bridge_on_dry_ground_is_refused_without_payment_or_work() {
    let mut m = river_match();
    let before = m.world.game.chars.clone();
    assert_eq!(
        command(&mut m, "bridge", Point { x: 11., z: 21. }, Some(0.)),
        Some(Denial::Blocked)
    );
    assert_eq!(m.start.economy.players[0].mp, 500.);
    assert!(m.field_works.jobs().is_empty());
    assert!(!m.world.game.units[&1].dig);
    assert_eq!(m.world.game.chars, before);
}

#[test]
fn eng08_fill_is_paid_once_and_rechecks_each_changed_cell() {
    let fixture: Value =
        serde_json::from_str(include_str!("../tests/fixtures/match-engineering.json")).unwrap();
    let mut m = make(&fixture["traces"][1]);
    assert_eq!(
        command(&mut m, "fill", Point { x: 20., z: 20. }, Some(0.)),
        Some(Denial::Blocked)
    );
    assert_eq!(m.start.economy.players[0].mp, 500.);
    for c in 408..413 {
        m.world.set_cell(c, b'+').unwrap();
    }
    assert_eq!(
        command(&mut m, "fill", Point { x: 20., z: 20. }, Some(0.)),
        None
    );
    assert_eq!(m.start.economy.players[0].mp, 490.);
    m.world.set_cell(410, b'T').unwrap();
    complete(&mut m);
    assert_eq!(&m.world.game.chars[408..413], b"..T..");
    assert_eq!(m.start.economy.players[0].mp, 490.);
    for c in 408..413 {
        m.world.set_cell(c, b'+').unwrap();
    }
    let project = Command::from_json(
        &json!({"t":"entrench","ids":[1],"pattern":"line","fort":"fill","x":20.,"z":20.}),
    )
    .unwrap();
    assert_eq!(m.execute_command(0, &project, false).unwrap(), None);
    assert_eq!(
        m.start.economy.players[0].mp, 490.,
        "project admission does not pay yet"
    );
    m.apply_project_unit(1).unwrap();
    assert_eq!(m.start.economy.players[0].mp, 480.);
    complete(&mut m);
    assert_eq!(&m.world.game.chars[408..413], b".....");
}

#[test]
fn eng16_bridge_preview_masks_sight_and_reorders_a_reversed_span_from_the_bank() {
    let mut m = river_match();
    let at = Point { x: 25., z: 21. };
    assert_eq!(
        crate::field_works::placement(&m.world, FortKind::Bridge, at, 0., Some(0), |_| false),
        Err(crate::field_works::Error::Refused(
            crate::field_works::Reason::NotVisible
        ))
    );
    let reverse = crate::field_works::placement(
        &m.world,
        FortKind::Bridge,
        at,
        std::f64::consts::PI,
        Some(0),
        |_| true,
    )
    .unwrap();
    // Prototype fortCells emits right to left at PI. supportedBridgePlan keeps
    // that order when the right bank supports the first cell (sim.js:2538-2551).
    assert_eq!(
        reverse.iter().map(|c| (c.cell, c.ch)).collect::<Vec<_>>(),
        vec![
            (414, b'='),
            (413, b'='),
            (412, b'='),
            (411, b'='),
            (410, b'=')
        ]
    );
    // Removing only the right bank makes the same directed plan wait for the
    // leftmost cell, then accept one more cell on each pass through pending.
    m.world.set_cell(415, b'W').unwrap();
    let reverse = crate::field_works::placement(
        &m.world,
        FortKind::Bridge,
        at,
        std::f64::consts::PI,
        Some(0),
        |_| true,
    )
    .unwrap();
    assert_eq!(
        reverse.iter().map(|c| (c.cell, c.ch)).collect::<Vec<_>>(),
        vec![
            (410, b'='),
            (411, b'='),
            (412, b'='),
            (413, b'='),
            (414, b'=')
        ]
    );
    assert_eq!(
        crate::field_works::placement(
            &m.world,
            FortKind::Bridge,
            at,
            std::f64::consts::FRAC_PI_2,
            Some(0),
            |_| true
        ),
        Err(crate::field_works::Error::Refused(
            crate::field_works::Reason::Blocked
        ))
    );
}

#[test]
fn eng56_bridge_repair_joins_supported_deck_and_vehicle_admission_requires_hull_width() {
    let mut m = river_match();
    m.world.game.units.get_mut(&1).unwrap().x = 19.;
    assert_eq!(
        command(&mut m, "bridge", Point { x: 25., z: 21. }, Some(0.)),
        None
    );
    complete(&mut m);
    let tank = m.rules.kinds[m.rules.kind("tank").unwrap()].clone();
    let pose = Pose {
        x: 25.,
        z: 21.,
        rot: 0.,
    };
    assert!(!crate::vehicle::vehicle_position_clear(
        &m.world.game,
        pose,
        &tank,
        MOVE | VBLOCK
    ));
    let original = m.world.sections[&412].structure_id.clone();
    destroy(&mut m, 412);
    assert_eq!(m.world.game.chars[412], b'W');
    assert!(m.world.game.sections[&412].failed);
    assert_eq!(
        command(&mut m, "bridge", Point { x: 25., z: 21. }, Some(0.)),
        None
    );
    m.apply_field_work_unit_dt(1, 3.).unwrap();
    assert_eq!(m.world.sections[&412].structure_id, original);
    assert!(!m.world.game.sections[&412].failed);
    assert_eq!(m.world.game.sections[&412].hp, 200.);
    for z in [19., 23.] {
        assert_eq!(
            command(&mut m, "bridge", Point { x: 25., z }, Some(0.)),
            None
        );
        complete(&mut m);
    }
    assert!(crate::vehicle::vehicle_position_clear(
        &m.world.game,
        pose,
        &tank,
        MOVE | VBLOCK
    ));
}

#[test]
fn eng57_paid_replacement_of_a_stone_bridge_cell_uses_plank_health_and_material() {
    let mut m = river_match();
    m.world.game.units.get_mut(&1).unwrap().x = 19.;
    assert_eq!(
        command(&mut m, "bridge", Point { x: 25., z: 21. }, Some(0.)),
        None
    );
    complete(&mut m);
    for c in 410..415 {
        m.world.sections.get_mut(&c).unwrap().material = "stone".into();
        let s = m.world.game.sections.get_mut(&(c as u32)).unwrap();
        s.hp = 2400.;
        s.max_hp = Some(2400.);
        m.world.game.cell_hp[c] = 2400.;
        m.world.game.tough.get_or_insert_with(|| vec![1.; 1600])[c] = 12.;
    }
    destroy(&mut m, 412);
    let before = m.start.economy.players[0].mp;
    assert_eq!(
        command(&mut m, "bridge", Point { x: 25., z: 21. }, Some(0.)),
        None
    );
    assert_eq!(m.start.economy.players[0].mp, before - 80.);
    m.apply_field_work_unit_dt(1, 3.).unwrap();
    assert_eq!(m.world.sections[&412].material, "wood");
    assert_eq!(m.world.game.sections[&412].max_hp, Some(200.));
    assert_eq!(m.world.game.tough.as_ref().unwrap()[412], 1.);
    assert_eq!(m.world.sections[&411].material, "stone");
    assert_eq!(m.world.game.sections[&411].hp, 2400.);
}
