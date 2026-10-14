//! Host output ownership exercised through actual ordinary firing and section reconciliation.
use super::*;
use serde_json::json;
use ww2_core::{
    catalog::Catalog,
    economy::{Faction, Mode},
    economy_state::ArmySize,
    environment::Effect,
    game::Point,
    map::AuthoritativeMap,
    match_commands::Command,
    match_firing::LaunchEvent,
    match_runtime::{MatchOptions, PlayerConfig},
    match_tick::TickPathConfig,
    orders::{OrderKey, QueueError},
    unit_publication::{PublicationParts, StagedUnit, prepare_publication},
    weather::WeatherChoice,
};

// The scene uses the same actual publication boundary as the tick-event provider's
// native failure arena. It supplies inputs only, never an expected event buffer.
fn arena(specs: &[(&str, usize, Point)]) -> Match {
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"Host event ownership arena", "w":40, "h":40,
            "rows":vec![".".repeat(40);40],
            "spawns":[{"x":2,"y":2},{"x":37,"y":37}],
            "points":[{"id":"front","x":30,"y":15}]
        }))
        .unwrap(),
    )
    .unwrap();
    let mut m = Match::new(
        &map,
        MatchOptions {
            seed: 811,
            mode: Mode::Conquest,
            players: vec![
                PlayerConfig {
                    name: "A".into(),
                    team: 0,
                    faction: Faction::Usa,
                },
                PlayerConfig {
                    name: "B".into(),
                    team: 1,
                    faction: Faction::Germany,
                },
            ],
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
    m.world.game.units.clear();
    m.commands.units.clear();
    m.recruit_metadata.clear();
    m.next_unit_id = 1000;
    m.grid.rebuild(&m.world.game.units);
    for &(name, owner, at) in specs {
        let id = m.next_unit_id;
        let kind = m.rules.kind(name).unwrap();
        let mut draft = StagedUnit::ordinary(
            &m.world.game,
            &m.rules,
            owner,
            kind,
            id,
            m.world.game.units.len() as u32,
            m.start.players[owner].spawn,
            Some(at),
        )
        .unwrap();
        draft.unit.hold_pos = true;
        draft.unit.still = Some(20.);
        draft.control.hold_fire = true;
        draft.control.auto_cast = false;
        draft.control.auto_retreat = false;
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
            vec![draft],
        )
        .unwrap()
        .publish_with(|u, _| {
            m.combat.initialize(u.id).hit_by = Some(0);
            m.firing.initialize(u.id);
            m.abilities.initialize(u.id);
            m.recovery.initialize(u.id);
        });
    }
    let visible = m.world.game.units.keys().copied().collect::<Vec<_>>();
    for player in &mut m.world.game.players {
        player.visible = visible.clone();
    }
    m.initialize_tick_scene(0, TickPathConfig::default())
        .unwrap();
    m
}

fn protected(runtime: &RoomRuntime) -> Vec<String> {
    let m = runtime.state();
    vec![
        format!("{:?}", m.world),
        format!("{:?}", m.environment),
        format!("{:?}", m.firing),
        format!("{:?}", m.combat),
        format!("{:?}", m.commands.units),
        format!("{:?}", m.commands.path_requests()),
        format!("{:?}", m.start.random.snapshot()),
        format!("{:?}", m.story),
        format!("{:?}", m.tick_state),
        format!("{:?}", runtime.pending_events),
        format!("{:?}", runtime.tick_failure()),
        format!(
            "{:?}",
            (
                m.commands.pending_commands(),
                m.commands.next_tick(),
                m.next_unit_id
            )
        ),
    ]
}

#[test]
fn real_launch_prefix_survives_host_fault_and_retry_refusal() {
    let mut m = arena(&[
        ("rifle", 0, Point { x: 25., z: 25. }),
        ("rifle", 0, Point { x: 25., z: 45. }),
        ("rifle", 1, Point { x: 37., z: 25. }),
    ]);
    m.commands.units.get_mut(&1000).unwrap().hold_fire = false;
    m.world.game.units.get_mut(&1001).unwrap().path_gen = u64::MAX - 2;
    m.enqueue_command(
        OrderKey {
            tick: 1,
            issuer: 0,
            sequence: 1,
        },
        Command::from_json(&json!({"t":"move","orders":[[1001,35,45]]})).unwrap(),
    )
    .unwrap();
    let future_key = OrderKey {
        tick: 2,
        issuer: 0,
        sequence: 2,
    };
    let future = Command::from_json(&json!({"t":"stop","ids":[1001]})).unwrap();
    m.enqueue_command(future_key, future.clone()).unwrap();
    let initial_random = m.start.random.snapshot();
    let mut runtime = RoomRuntime::new(m, 0).unwrap();
    assert_eq!(
        runtime.advance_tick(&[false, false]).unwrap_err(),
        "unit path identity exhausted"
    );
    let failure = runtime.tick_failure().unwrap();
    assert_eq!(failure.tick, 1);
    assert_eq!(failure.reason, "unit path identity exhausted");
    assert_eq!(failure.retirement, Default::default());
    assert!(!failure.events.is_empty());
    let mut flights = BTreeSet::new();
    for event in &failure.events {
        let TickEvent::Firing(LaunchEvent::Flight {
            id, shooter, owner, ..
        }) = event
        else {
            panic!("expected earlier rifle's actual launch, received {event:?}");
        };
        assert_eq!((*shooter, *owner), (1000, 0));
        assert!(flights.insert(*id), "launch prefix duplicated");
        assert_eq!(runtime.state().firing.flights[id].shooter, 1000);
    }
    assert_eq!(flights.len(), runtime.state().firing.flights.len());
    assert_ne!(runtime.state().start.random.snapshot(), initial_random);
    assert_eq!(
        runtime.state().world.game.units[&1001].path_gen,
        u64::MAX - 1
    );
    assert!(runtime.state().firing.events.is_empty());
    assert!(runtime.state().support.events.is_empty());
    assert!(runtime.state().aviation.events.is_empty());
    assert!(
        runtime.pending_events.is_empty(),
        "failed prefix has one diagnostic owner"
    );
    let before = protected(&runtime);
    assert!(runtime.advance_tick(&[false, false]).is_err());
    for kind in [FrameKind::Current, FrameKind::Scheduled] {
        assert!(
            runtime
                .collect_frame(
                    &[SelectedSeat {
                        slot: 0,
                        full: false,
                        omit: false
                    }],
                    kind
                )
                .is_err()
        );
        assert!(runtime.collect_frame(&[], kind).is_err());
    }
    assert_eq!(protected(&runtime), before);
    assert_eq!(
        runtime.state_mut().enqueue_command(future_key, future),
        Err(QueueError::DuplicateKey)
    );
    assert_eq!(protected(&runtime), before);
}

fn section_runtime() -> RoomRuntime {
    let mut m = arena(&[
        ("hq", 0, Point { x: 25., z: 25. }),
        ("hq", 1, Point { x: 45., z: 45. }),
    ]);
    let maximum = Catalog::compiled()
        .unwrap()
        .unit("hq")
        .unwrap()
        .get("hpPer")
        .unwrap()
        .number()
        .unwrap();
    for (id, cell) in [(1000, 12 * 40 + 12), (1001, 22 * 40 + 22)] {
        m.world.set_cell(cell, b'B').unwrap();
        m.world.game.units.get_mut(&id).unwrap().cells = Some(vec![cell as u32]);
        m.world
            .register_building_structure(id, &[cell], 100., maximum)
            .unwrap();
        m.world.game.units.get_mut(&id).unwrap().hp = if id == 1000 { 75. } else { 100. };
    }
    let mut runtime = RoomRuntime::new(m, 0).unwrap();
    let report = runtime.advance_tick(&[false, false]).unwrap();
    assert!(
        report.events.is_empty(),
        "successful events move into the host batch"
    );
    assert_eq!(runtime.pending_events.len(), 1);
    let TickEvent::Environment(Effect::Hurt(hurt)) = &runtime.pending_events[0] else {
        panic!("expected actual section reconciliation hurt");
    };
    assert_eq!(
        (hurt.unit, hurt.owner, hurt.at, hurt.killed),
        (1000, 0, Point { x: 25., z: 25. }, false)
    );
    assert_eq!(runtime.state().world.game.sections[&(12 * 40 + 12)].hp, 75.);
    runtime
}

#[test]
fn real_section_hurt_waits_for_scheduled_frame_and_empty_schedule_discards_it() {
    let selected = [SelectedSeat {
        slot: 0,
        full: false,
        omit: false,
    }];
    let mut runtime = section_runtime();
    let TickEvent::Environment(effect) = &runtime.pending_events[0] else {
        unreachable!()
    };
    let actual_hurt = effect.to_json();
    for seats in [selected.as_slice(), &[]] {
        let frames = runtime.collect_frame(seats, FrameKind::Current).unwrap();
        for frame in frames {
            assert_eq!(frame.message["shots"], json!([]));
        }
        assert_eq!(runtime.pending_events.len(), 1);
    }
    let frame = runtime
        .collect_frame(&selected, FrameKind::Scheduled)
        .unwrap();
    assert_eq!(frame[0].message["shots"], json!([actual_hurt]));
    assert!(runtime.pending_events.is_empty());
    let frame = runtime
        .collect_frame(&selected, FrameKind::Scheduled)
        .unwrap();
    assert_eq!(frame[0].message["shots"], json!([]));

    let mut unobserved = section_runtime();
    assert!(
        unobserved
            .collect_frame(&[], FrameKind::Current)
            .unwrap()
            .is_empty()
    );
    assert_eq!(unobserved.pending_events.len(), 1);
    assert!(
        unobserved
            .collect_frame(&[], FrameKind::Scheduled)
            .unwrap()
            .is_empty()
    );
    assert!(unobserved.pending_events.is_empty());
    let frame = unobserved
        .collect_frame(&selected, FrameKind::Current)
        .unwrap();
    assert_eq!(frame[0].message["shots"], json!([]));
}
