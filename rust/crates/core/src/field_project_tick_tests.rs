use super::*;
use crate::{
    economy::Faction,
    economy_state::ArmySize,
    field_works::{Pattern, ProjectMaterial, ProjectRequest, SegmentPreparation},
    game::{ModeKind, Point},
    map::AuthoritativeMap,
    match_runtime::{MatchOptions, PlayerConfig},
    weather::WeatherChoice,
};

fn scene() -> (Match, UnitId) {
    scene_mode(Mode::Conquest)
}
fn scene_mode(mode: Mode) -> (Match, UnitId) {
    let value = serde_json::json!({"name":"Project tick admission","w":40,"h":40,
        "rows":vec![".".repeat(40);40],"spawns":[{"x":0,"y":0},{"x":39,"y":39}],
        "points":[{"x":20,"y":20}]});
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&value).unwrap()).unwrap();
    let m = Match::new(
        &map,
        MatchOptions {
            seed: 701,
            mode,
            players: (0..2)
                .map(|team| PlayerConfig {
                    name: format!("P{team}"),
                    team,
                    faction: Faction::try_from(team as u8).unwrap(),
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
    (m, id)
}
fn order(m: &Match) -> Vec<UnitId> {
    m.world.game.units.keys().copied().collect()
}
fn state(m: &Match, id: UnitId) -> String {
    let u = &m.world.game.units[&id];
    format!(
        "{}:{:?}:{:?}:{:?}:{}:{}:{}",
        m.world.game.tick,
        m.field_works,
        m.commands.units[&id].orders,
        m.firing.units[&id],
        u.dig,
        u.entrench,
        m.start.economy.players[u.owner].mp.to_bits()
    )
}
fn paid(m: &mut Match, id: UnitId) {
    let u = &m.world.game.units[&id];
    let owner = u.owner;
    let prepared = m
        .field_works
        .prepare_project(
            &m.world,
            &m.start.economy.players[owner],
            ProjectRequest {
                owner,
                pattern: Pattern::Line,
                fort: FortKind::Trench,
                from: Point { x: 25.0, z: 25.0 },
                to: Point { x: 41.0, z: 25.0 },
                back: u.at(),
            },
            |_| true,
        )
        .unwrap();
    let project = prepared.commit(&mut m.field_works).unwrap();
    m.field_works.join_project(project, &[id]).unwrap();
    m.world.game.units.get_mut(&id).unwrap().entrench = true;
    let SegmentPreparation::Paid(prepared) = m
        .field_works
        .prepare_segment(&m.world, &mut m.start.economy.players[owner], id, |_| true)
        .unwrap()
    else {
        panic!("paid segment");
    };
    prepared.commit(&mut m.field_works).unwrap();
    m.world.game.units.get_mut(&id).unwrap().dig = true;
}

#[test]
fn project_tick_admission_reads_paid_and_dead_workers_without_recounting() {
    let (mut m, id) = scene();
    paid(&mut m, id);
    let current = order(&m);
    let before = state(&m, id);
    m.validate_field_project_tick(&current).unwrap();
    assert_eq!(state(&m, id), before);
    m.world.game.units.get_mut(&id).unwrap().hp = 0.0;
    let before = state(&m, id);
    m.validate_field_project_tick(&current).unwrap();
    assert_eq!(state(&m, id), before);
    // Only the actual early phase, after the clock changes, performs the source recount.
    assert_eq!(m.field_works.member_project(id).unwrap().active, 1);
    m.world.game.tick = 1;
    m.recount_field_projects().unwrap();
    assert_eq!(m.field_works.member_project(id).unwrap().active, 0);
}

#[test]
fn malformed_project_tick_facts_reject_before_any_clock_or_work_write() {
    for variant in 0..9 {
        let (mut m, id) = scene();
        paid(&mut m, id);
        let mut current = order(&m);
        match variant {
            0 => m.world.game.units.get_mut(&id).unwrap().dig = false,
            1 => m.field_works.jobs.get_mut(&id).unwrap().elapsed = f64::NAN,
            2 => m.world.game.units.get_mut(&id).unwrap().entrench = false,
            3 => m.world.game.tick = SAFE_INTEGER,
            4 => current.push(id),
            5 => m.options.mode = Mode::World,
            6 => m.world.game.mode = Some(ModeKind::Classic),
            7 => {
                let job = m.field_works.jobs.get_mut(&id).unwrap();
                while job.cells.len() < 7 {
                    job.cells.push(job.cells[0]);
                }
            }
            8 => {
                m.field_works.projects.records.get_mut(&1).unwrap().id = 2;
                m.field_works.projects.sequence = 2;
            }
            _ => unreachable!(),
        }
        let before = state(&m, id);
        assert!(
            m.validate_field_project_tick(&current).is_err(),
            "variant {variant}"
        );
        assert_eq!(state(&m, id), before, "variant {variant}");
    }
}

#[test]
fn stale_project_join_is_admitted_for_the_source_waiting_denial() {
    let (mut m, id) = scene();
    m.commands
        .units
        .get_mut(&id)
        .unwrap()
        .orders
        .push_back(Command::Entrench {
            ids: vec![i64::from(id)],
            queue: false,
            order: ProjectOrder::Join {
                project: 999,
                at: Some(Point { x: 25.0, z: 25.0 }),
            },
        });
    let current = order(&m);
    m.validate_field_project_tick(&current).unwrap();
    m.start_queued_orders(id, false).unwrap();
    assert!(m.commands.units[&id].orders.is_empty());
    assert_eq!(m.field_works.project_sequence(), 0);
    // A New intent is not the normalized Join reference stored by the queue writer.
    m.commands
        .units
        .get_mut(&id)
        .unwrap()
        .orders
        .push_back(Command::Entrench {
            ids: vec![i64::from(id)],
            queue: false,
            order: ProjectOrder::New {
                pattern: Some(Pattern::Line),
                material: ProjectMaterial::Fort(FortKind::Trench),
                from: Some(Point { x: 25.0, z: 25.0 }),
                to: Some(Point { x: 33.0, z: 25.0 }),
            },
        });
    let before = state(&m, id);
    assert!(m.validate_field_project_tick(&current).is_err());
    assert_eq!(state(&m, id), before);
}

#[test]
fn cached_crew_capacity_is_reserved_until_the_actual_twentieth_tick_sweep() {
    let (mut m, id) = scene();
    paid(&mut m, id);
    m.drop_field_work_job(id).unwrap();
    m.cancel_field_work(id).unwrap();
    m.field_works.projects.records.get_mut(&1).unwrap().crew = SAFE_INTEGER as usize;
    m.commands
        .units
        .get_mut(&id)
        .unwrap()
        .orders
        .push_back(Command::Entrench {
            ids: vec![i64::from(id)],
            queue: false,
            order: ProjectOrder::Join {
                project: 1,
                at: Some(Point { x: 25.0, z: 25.0 }),
            },
        });
    let current = order(&m);
    let before = state(&m, id);
    assert!(m.validate_field_project_tick(&current).is_err());
    assert_eq!(state(&m, id), before);
    // Tick twenty recounts the cached crew from live and waiting references before joining.
    m.world.game.tick = 19;
    m.validate_field_project_tick(&current).unwrap();
    m.world.game.tick = 20;
    m.recount_field_projects().unwrap();
    assert_eq!(m.field_works.project(1).unwrap().crew, 1);
}

fn alias_state(m: &Match) -> String {
    format!(
        "{:?}:{:?}:{:?}:{:?}:{:?}:{:?}:{:?}",
        m.world.game,
        m.field_works,
        m.commands.units,
        m.abilities,
        m.recovery,
        m.start.economy,
        m.story
    )
}

#[test]
fn ordinary_default_provider_admission_preserves_real_jobs_and_actuators() {
    let source: serde_json::Value =
        serde_json::from_slice(include_bytes!("../tests/fixtures/ordinary-mode-alias.json"))
            .unwrap();
    assert_eq!(source["cases"][0]["steps"], source["cases"][1]["steps"]);
    assert_eq!(source["cases"][0]["steps"][0]["expected"]["winner"], 0);
    assert_eq!(source["cases"][2]["steps"][0]["expected"]["tech"], false);
    assert_eq!(
        source["cases"][2]["steps"][0]["expected"]["logisticsEnabled"],
        false
    );
    let (mut default, first) = scene_mode(Mode::Default);
    let (mut conquest, other_first) = scene_mode(Mode::Conquest);
    assert_eq!(first, other_first);
    assert!(default.world.game.mode.is_none() && conquest.world.game.mode.is_none());
    assert_eq!(alias_state(&default), alias_state(&conquest));
    for m in [&mut default, &mut conquest] {
        let raw = &source["cases"][0]["command"]["command"];
        let stance = Command::from_json(raw).unwrap();
        assert_eq!(m.execute_command(0, &stance, false).unwrap(), None);
        paid(m, first);
        let current = order(m);
        let before = alias_state(m);
        m.validate_abilities().unwrap();
        m.prepare_recovery(&current).unwrap();
        m.validate_field_project_tick(&current).unwrap();
        assert_eq!(alias_state(m), before);
        let second = m
            .world
            .game
            .units
            .values()
            .find(|u| u.owner == 0 && u.id != first && m.rules.kinds[u.kind].name == "rifle")
            .unwrap()
            .id;
        assert_eq!(
            m.execute_entrench(
                0,
                &[i64::from(second)],
                ProjectOrder::Join {
                    project: 1,
                    at: None
                },
                false,
                false
            )
            .unwrap(),
            None
        );
        assert_eq!(m.field_works.membership(second), Some(1));
        assert!(m.field_works.is_active(first));
        m.advance_ability_timers(first, 0.05).unwrap();
        let frame = m.prepare_recovery(&current).unwrap();
        m.heal_and_sweep(&frame, 0.05).unwrap();
        m.reinforce(&frame, 0.05).unwrap();
        let hq = m
            .world
            .game
            .units
            .values()
            .find(|u| u.owner == 0 && m.rules.kinds[u.kind].name == "hq")
            .unwrap()
            .id;
        assert_eq!(m.repair_building_sections(hq, 1.0).unwrap(), 0.0);
        m.validate_field_project_tick(&current).unwrap();
    }
    assert_eq!(alias_state(&default), alias_state(&conquest));
    assert_eq!(
        default.start.random.snapshot(),
        conquest.start.random.snapshot()
    );
    // A mode alias never represents a construction context or a synthetic mode object.
    for (mode, game_mode) in [
        (Mode::World, None),
        (Mode::Classic, None),
        (Mode::Default, Some(ModeKind::Assault)),
        (Mode::Conquest, Some(ModeKind::Classic)),
    ] {
        default.options.mode = mode;
        default.world.game.mode = game_mode;
        let current = order(&default);
        let before = alias_state(&default);
        assert!(default.validate_abilities().is_err());
        assert!(default.prepare_recovery(&current).is_err());
        assert!(default.validate_field_project_tick(&current).is_err());
        assert!(
            default
                .execute_entrench(
                    0,
                    &[i64::from(first)],
                    ProjectOrder::Join {
                        project: 1,
                        at: None
                    },
                    false,
                    false
                )
                .unwrap()
                .is_some()
        );
        assert!(default.repair_building_sections(first, 1.0).is_err());
        assert_eq!(alias_state(&default), before);
    }
}
