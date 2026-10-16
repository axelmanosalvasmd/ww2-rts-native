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
    for id in m.world.game.units.keys().copied().collect::<Vec<_>>() {
        m.production.remove_building(id);
        m.building_rallies.remove(&id);
    }
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
            if m.rules.kinds[u.kind].building {
                m.production.initialize_building(u.id).unwrap();
                m.building_rallies.insert(u.id, None);
            }
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

#[test]
fn supported_support_lifecycles_deliver_every_r1_seat_through_match_finish() {
    use ww2_core::recipient_snapshot::SUPPORTED_SUPPORT_KINDS;
    for kind in SUPPORTED_SUPPORT_KINDS {
        let mut state = crate::lobby::native_tests::native_match();
        for control in state.commands.units.values_mut() {
            control.hold_fire = true;
            control.auto_cast = false;
            control.auto_retreat = false;
        }
        state.start.economy.players[0].mp = 10000.;
        state.start.players[0]
            .support
            .insert(kind.name().into(), 0.);
        let at = Point { x: 100., z: 100. };
        state
            .enqueue_command(
                OrderKey {
                    tick: 1,
                    issuer: 0,
                    sequence: 1,
                },
                Command::Support {
                    kind: kind.name().into(),
                    at,
                    direction: Some(0.),
                },
            )
            .unwrap();
        let mut runtime = RoomRuntime::new(state, 0).unwrap();
        let selected = (0..2)
            .map(|slot| SelectedSeat {
                slot,
                full: false,
                omit: false,
            })
            .collect::<Vec<_>>();
        let mut observed = false;
        let mut retired = false;
        for _ in 0..2400 {
            runtime.advance_tick(&[false; 2]).unwrap();
            let frames = runtime
                .collect_frame(&selected, FrameKind::Scheduled)
                .unwrap();
            assert_eq!(frames.len(), 2);
            assert_eq!(frames[0].message["strikes"], frames[1].message["strikes"]);
            assert!(
                frames[0].message["strikes"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|row| row[0] == kind.name() && row[5] == 0)
            );
            assert!(frames.iter().all(|frame| frame.message["t"] == "s"));
            observed |= !runtime.state().world.game.strikes.is_empty();
            if observed
                && runtime.state().world.game.strikes.is_empty()
                && runtime.state().support.covers.is_empty()
                && runtime.state().world.game.smokes.is_empty()
            {
                retired = true;
                break;
            }
        }
        assert!(observed && retired, "{} did not finish", kind.name());
        // End through the actual VP phase after the support lifetime, retaining
        // ordinary tick and delivery admission through the decided match.
        runtime.state_mut().start.economy.players[0].vp = 10000.;
        for _ in 0..10 {
            runtime.advance_tick(&[false; 2]).unwrap();
            let frames = runtime
                .collect_frame(&selected, FrameKind::Scheduled)
                .unwrap();
            if runtime.state().finish.is_some() {
                assert!(frames.iter().all(|frame| frame.message["winner"] == 0));
            }
        }
        assert!(runtime.state().finish.is_some());
        assert!(runtime.fault().is_none());
        assert!(runtime.tick_failure().is_none());
    }
}

#[test]
fn plane_support_state_cannot_attach_to_an_r1_room_before_its_event_contract() {
    for name in ["recon", "strafe", "bombing", "dive", "para"] {
        let mut state = crate::lobby::native_tests::native_match();
        state.start.economy.players[0].mp = 10000.;
        state.start.players[0].support.insert(name.into(), 0.);
        let at = state.start.players[0].spawn;
        assert_eq!(state.admit_support(0, name, at, Some(0.)).unwrap(), None);
        assert!(
            matches!(RoomRuntime::new(state, 0), Err(error) if error == "snapshot admission: Unsupported { unit: 0, capability: TeamEffects }")
        );
    }
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
fn r1_aircraft_purchase_is_denied_without_charge_and_every_seat_keeps_snapshots() {
    let purchase_ready = || {
        let mut m = crate::lobby::native_tests::native_match();
        m.production = ww2_core::production::ProductionState::new(vec![
            ww2_core::economy::TechState::new(3, 0, 0, 0).unwrap();
            2
        ])
        .unwrap();
        for unit in m.world.game.units.values() {
            if m.rules.kinds[unit.kind].building {
                m.production.initialize_building(unit.id).unwrap();
            }
        }
        m.start.economy.players[0].mp = 50_000.;
        m.start.economy.players[0].fuel = 50_000.;
        let field = m.next_unit_id;
        let kind = m.rules.kind("airfield").unwrap();
        let definition = Catalog::compiled().unwrap().unit("airfield").unwrap();
        let size = definition.get("size").unwrap().number().unwrap() as usize;
        let hp = definition.get("hpPer").unwrap().number().unwrap();
        let g = &m.world.game;
        let cells = (1..g.chars.len())
            .filter(|cell| cell % g.w + size < g.w && cell / g.w + size < g.h)
            .map(|cell| {
                (0..size)
                    .flat_map(|y| (0..size).map(move |x| cell + y * g.w + x))
                    .collect::<Vec<_>>()
            })
            .find(|cells| {
                let at = Point {
                    x: (cells[0] % g.w) as f64 * ww2_core::game::CELL
                        + size as f64 * ww2_core::game::CELL / 2.,
                    z: (cells[0] / g.w) as f64 * ww2_core::game::CELL
                        + size as f64 * ww2_core::game::CELL / 2.,
                };
                cells.iter().all(|&cell| {
                    g.chars[cell] == b'.'
                        && g.height.as_ref().is_none_or(|h| h[cell] == h[cells[0]])
                }) && g
                    .units
                    .values()
                    .all(|u| ww2_core::math::hypot(u.x - at.x, u.z - at.z) > 10.)
            })
            .unwrap();
        let at = Point {
            x: (cells[0] % g.w) as f64 * ww2_core::game::CELL
                + size as f64 * ww2_core::game::CELL / 2.,
            z: (cells[0] / g.w) as f64 * ww2_core::game::CELL
                + size as f64 * ww2_core::game::CELL / 2.,
        };
        for &cell in &cells {
            m.world.set_cell(cell, b'K').unwrap();
        }
        let mut draft = StagedUnit::ordinary(
            &m.world.game,
            &m.rules,
            0,
            kind,
            field,
            m.world.game.units.len() as u32,
            m.start.players[0].spawn,
            Some(at),
        )
        .unwrap();
        draft.unit.built = Some(1.);
        draft.unit.hp = hp;
        draft.unit.cells = Some(cells.iter().map(|&cell| cell as u32).collect());
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
        .publish_with(|unit, _| {
            m.combat.initialize(unit.id).hit_by = Some(0);
            m.firing.initialize(unit.id);
            m.abilities.initialize(unit.id);
            m.recovery.initialize(unit.id);
            m.production.initialize_building(unit.id).unwrap();
            m.building_rallies.insert(unit.id, None);
        });
        for &cell in &cells {
            m.building_cells.insert(
                cell,
                ww2_core::match_runtime::BuildingCell {
                    unit: field,
                    owner: 0,
                    at,
                },
            );
        }
        m.world
            .register_building_structure(field, &cells, hp, hp)
            .unwrap();
        (m, field)
    };

    // These purchases pass ordinary economy/maker admission without a host binding.
    let (mut standalone, field) = purchase_ready();
    for kind in ["fighter", "attacker", "bomber"] {
        standalone.recorded_buy(0, kind, Some(field)).unwrap();
    }

    let (mut m, field) = purchase_ready();
    let buy = |kind: &str| Command::Buy {
        kind: kind.into(),
        from: Some(field),
    };
    // A command recorded before attachment must obey the host's later binding too.
    m.enqueue_command(
        OrderKey {
            tick: 1,
            issuer: 0,
            sequence: 1,
        },
        buy("fighter"),
    )
    .unwrap();
    let mut runtime = RoomRuntime::new(m, 0).unwrap();
    let initial_id = runtime.state().next_unit_id;
    let initial_count = runtime.state().world.game.units.len();
    let selected = (0..2)
        .map(|slot| SelectedSeat {
            slot,
            full: true,
            omit: false,
        })
        .collect::<Vec<_>>();
    for tick in 1..=9 {
        if tick == 4 || tick == 8 {
            runtime
                .state_mut()
                .enqueue_command(
                    OrderKey {
                        tick,
                        issuer: 0,
                        sequence: tick,
                    },
                    buy(if tick == 4 { "attacker" } else { "bomber" }),
                )
                .unwrap();
        }
        let before = runtime.state().start.economy.players[0].clone();
        let story = format!("{:?}", runtime.state().story);
        assert!(matches!(
            runtime.state_mut().recorded_buy(0, "fighter", Some(field)),
            Err(ww2_core::match_runtime::RecruitError::Placement(
                ww2_core::produced_units::Error::UnsupportedAir
            ))
        ));
        assert_eq!(runtime.state().start.economy.players[0], before);
        assert_eq!(format!("{:?}", runtime.state().story), story);
        let report = runtime.advance_tick(&[false; 2]).unwrap();
        assert_eq!(report.tick, tick as i64);
        if [1, 4, 8].contains(&tick) {
            assert_eq!(report.commands.len(), 1);
            assert_eq!(
                report.commands[0].denial,
                Some(ww2_core::match_commands::Denial::Unsupported)
            );
        }
        for kind in [FrameKind::Current, FrameKind::Scheduled] {
            let frames = runtime.collect_frame(&selected, kind).unwrap();
            assert_eq!(
                frames.iter().map(|frame| frame.slot).collect::<Vec<_>>(),
                vec![0, 1]
            );
        }
        assert!(runtime.fault().is_none());
        assert!(runtime.state().tick_state.fault().is_none());
        assert!(runtime.state().aviation.planes.is_empty());
        assert_eq!(runtime.state().next_unit_id, initial_id);
        assert_eq!(runtime.state().world.game.units.len(), initial_count);
    }
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

#[test]
fn aircraft_weapons_wait_for_contract_while_every_seat_receives_ticks_through_victory() {
    use ww2_core::{
        game::PlaneState,
        match_aviation::{MissionKind, PlaneMission},
        match_end::EndReason,
    };
    for name in ["attacker", "bomber"] {
        let map = AuthoritativeMap::from_json(
            &serde_json::to_vec(&json!({
                "name":"Aircraft delivery boundary", "w":80, "h":80,
                "rows":vec![".".repeat(80);80],
                "spawns":[{"x":8,"y":8},{"x":70,"y":70}],
                "points":[{"x":12,"y":12,"vp":5,"mp":0}]
            }))
            .unwrap(),
        )
        .unwrap();
        let mut state = Match::new(
            &map,
            MatchOptions {
                seed: 701,
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
        for control in state.commands.units.values_mut() {
            control.hold_fire = true;
            control.auto_cast = false;
            control.auto_retreat = false;
        }
        let target = state
            .world
            .game
            .units
            .values()
            .find(|u| u.owner == 1 && state.rules.kinds[u.kind].name == "rifle")
            .unwrap()
            .id;
        state.world.game.units.get_mut(&target).unwrap().x = 40.;
        state.world.game.units.get_mut(&target).unwrap().z = 40.;
        state.grid.rebuild(&state.world.game.units);
        let plane = state.spawn_aircraft(0, name).unwrap();
        state.commands.units.get_mut(&plane).unwrap().hold_fire = false;
        ww2_core::vision::update_vision(&mut state.world.game, &state.rules, &state.grid);
        let initial_ammo = state.aviation.planes[&plane].ammo;
        assert!(
            state
                .send_aircraft_mission(
                    0,
                    plane,
                    PlaneMission {
                        kind: if name == "bomber" {
                            MissionKind::Area
                        } else {
                            MissionKind::Attack
                        },
                        target: (name == "attacker").then_some(target),
                        at: Point { x: 40., z: 40. },
                    }
                )
                .unwrap()
        );
        let capturer = state
            .world
            .game
            .units
            .values()
            .find(|u| u.owner == 0 && state.rules.kinds[u.kind].name == "rifle")
            .unwrap()
            .id;
        state
            .enqueue_command(
                OrderKey {
                    tick: 1,
                    issuer: 0,
                    sequence: 1,
                },
                Command::from_json(&json!({"t":"move", "orders":[[capturer,24.,24.]]})).unwrap(),
            )
            .unwrap();
        let mut room = RoomRuntime::new(state, 0).unwrap();
        let seats = (0..2)
            .map(|slot| SelectedSeat {
                slot,
                full: false,
                omit: false,
            })
            .collect::<Vec<_>>();
        let mut snapshots = 0;
        let mut ready_to_fire = false;
        assert!(room.state().finish.is_none());
        assert_eq!(room.state().start.economy.players[0].vp, 0.);
        for _ in 0..10000 {
            let report = room.advance_tick(&[false; 2]).unwrap();
            let m = room.state();
            assert!(m.firing.flights.is_empty());
            assert!(m.firing.salvos.is_empty());
            assert_eq!(m.aviation.planes[&plane].ammo, initial_ammo);
            ready_to_fire |= if name == "bomber" {
                m.world.game.units[&plane].air == Some(PlaneState::Station)
            } else {
                m.can_shoot_target(plane, target).unwrap()
            };
            let frames = room.collect_frame(&seats, FrameKind::Scheduled).unwrap();
            assert_eq!(frames.len(), seats.len());
            snapshots += frames.len();
            assert!(frames.iter().all(|frame| frame.message["t"] == "s"));
            if room.state().finish.is_some() {
                assert!(frames.iter().all(|frame| frame.message["winner"] == 0));
                assert_eq!(snapshots, report.tick as usize * seats.len());
                break;
            }
        }
        assert!(
            ready_to_fire,
            "{name} must reach a genuine firing opportunity"
        );
        let end = room
            .state()
            .finish
            .as_ref()
            .expect("ordinary income must finish the match");
        assert_eq!(end.reason, EndReason::VictoryPoints);
        assert_eq!(end.winner, 0);
        assert!(room.fault().is_none());
        assert!(room.tick_failure().is_none());
        assert!(room.state().tick_state.fault().is_none());
    }
}
