use crate::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::Point,
    map::AuthoritativeMap,
    match_commands::{Command, Denial, MoveOrder, Stance},
    match_runtime::{Match, MatchOptions, PlayerConfig},
    match_tick::TickPhase,
    recipient_snapshot::DeliveryState,
    weather::WeatherChoice,
};
use serde_json::json;
fn scene(enabled: bool) -> (Match, u32) {
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"Logistics runtime","w":80,"h":80,"rows":vec![".".repeat(80);80],
            "heights":vec!["0".repeat(80);80],"spawns":[{"x":5,"y":5},{"x":74,"y":74}],
            "points":[{"x":40,"y":40}]
        }))
        .unwrap(),
    )
    .unwrap();
    let mut m = Match::new(
        &map,
        MatchOptions {
            seed: 701,
            mode: Mode::Conquest,
            players: vec![
                PlayerConfig {
                    name: "a".into(),
                    team: 0,
                    faction: Faction::Usa,
                },
                PlayerConfig {
                    name: "b".into(),
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
            supply: true,
            logistics: enabled,
            sandbox: false,
        },
    )
    .unwrap();
    for c in m.commands.units.values_mut() {
        c.auto_cast = false;
        c.auto_retreat = false;
        c.hold_fire = true;
    }
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
fn force(m: &mut Match, id: u32, at: Point) {
    let u = m.world.game.units.get_mut(&id).unwrap();
    u.x = at.x;
    u.z = at.z;
    u.path.clear();
    u.world_goal = None;
    m.grid.rebuild(&m.world.game.units);
    let r = m.logistics.get_mut(id).unwrap();
    r.provisions = 0.;
    r.exhausted = true;
    r.warning = Some(0.);
    r.forced = true;
}
// Exercise the reserve and withdrawal adapters without entering the full tick runner.
fn withdrawal_turn(m: &mut Match, id: u32) {
    m.begin_path_tick(4096, 20).unwrap();
    m.advance_unit_reserves(id).unwrap();
    m.advance_supply_withdrawal(id).unwrap();
}
fn assert_deliverable(m: &mut Match) {
    m.validate_tick_state().unwrap();
    let mut delivery = DeliveryState::new(m).unwrap();
    let frame = delivery
        .capture_with_lists(m, &[true, true], &[false, false], &[])
        .unwrap();
    for owner in 0..2 {
        frame.recipient(owner).unwrap();
    }
    assert!(m.tick_state.fault().is_none());
}
#[test]
fn logistics_runtime_full_tick_requires_matching_convoy_state() {
    let (mut enabled, _) = scene(true);
    let report = enabled.advance_tick().unwrap();
    assert!(report.phases.contains(&TickPhase::UnitReserves));
    assert!(report.phases.contains(&TickPhase::Convoys));
    assert_deliverable(&mut enabled);
    for (enabled, convoys) in [(true, false), (false, true)] {
        let (mut m, id) = scene(true);
        m.options.logistics = enabled;
        if !convoys {
            m.convoys = None;
        }
        let tick = m.world.game.tick;
        let reserves = m.logistics.get(id).unwrap().clone();
        let control = m.commands.units[&id].clone();
        let error = m.advance_tick().unwrap_err();
        assert!(
            error
                .reason
                .contains("logistics option and convoy state disagree")
        );
        assert!(error.events.is_empty());
        assert_eq!(m.world.game.tick, tick);
        assert_eq!(m.logistics.get(id).unwrap(), &reserves);
        assert_eq!(m.commands.units[&id], control);
        assert!(m.tick_state.fault().is_none());
    }
}
#[test]
fn logistics_runtime_ordinary_logistics_off_ticks_and_complete_snapshots_still_work() {
    let (mut m, _) = scene(false);
    let mut delivery = DeliveryState::new(&m).unwrap();
    for _ in 0..45 {
        let report = m.advance_tick().unwrap();
        assert!(!report.phases.contains(&TickPhase::UnitReserves));
        assert!(!report.phases.contains(&TickPhase::Convoys));
        for phase in [
            TickPhase::UnitReserves,
            TickPhase::SupplyNetwork,
            TickPhase::Convoys,
        ] {
            assert!(!report.unavailable.contains(&phase));
            assert!(!report.phases.contains(&phase));
        }
        assert!(m.convoys.is_none());
        assert!(m.logistics.iter().next().is_none());
        let frame = delivery
            .capture_with_lists(&mut m, &[true, true], &[false, false], &[])
            .unwrap();
        for owner in 0..2 {
            frame.recipient(owner).unwrap();
        }
        assert!(m.tick_state.fault().is_none());
    }
}
#[test]
fn logistics_runtime_log12_withdrawal_cancels_orders_and_routes_without_teleport() {
    let (mut m, id) = scene(true);
    let start = Point { x: 65., z: 45. };
    force(&mut m, id, start);
    let carrier = add_actor(&mut m, "halftrack", Point { x: 75., z: 45. });
    let c = m.commands.units.get_mut(&id).unwrap();
    c.nade = Some(start);
    c.fire_at = Some(0);
    c.attack_id = 99;
    c.enter = Some(0);
    c.orders.push_back(Command::Stop {
        ids: vec![i64::from(id)],
    });
    m.world.game.units.get_mut(&id).unwrap().board = carrier;
    withdrawal_turn(&mut m, id);
    let c = &m.commands.units[&id];
    assert!(c.supply_started && c.orders.is_empty());
    assert!(c.nade.is_none() && c.fire_at.is_none() && c.enter.is_none());
    assert_eq!(c.attack_id, 0);
    assert_eq!(m.world.game.units[&id].board, 0);
    let u = &m.world.game.units[&id];
    assert!(u.retreating && !u.path.is_empty());
    assert!(!m.logistics.get(id).unwrap().stranded);
    assert!(crate::math::hypot(u.x - start.x, u.z - start.z) < 2.);
    let goal = u.world_goal.unwrap();
    let home = m
        .convoys
        .as_ref()
        .unwrap()
        .stores
        .iter()
        .filter(|s| s.owner == 0 && s.source)
        .min_by(|a, b| {
            crate::math::hypot(a.at.x - start.x, a.at.z - start.z)
                .total_cmp(&crate::math::hypot(b.at.x - start.x, b.at.z - start.z))
        })
        .unwrap()
        .at;
    assert!((crate::math::hypot(goal.x - home.x, goal.z - home.z) - 10.).abs() < 1e-9);
    assert_deliverable(&mut m);
}
#[test]
fn logistics_runtime_log12_stranded_retries_and_nearby_handoff_needs_no_path() {
    let (mut m, id) = scene(true);
    force(&mut m, id, Point { x: 65., z: 45. });
    let stores = std::mem::take(&mut m.convoys.as_mut().unwrap().stores);
    m.begin_path_tick(4096, 20).unwrap();
    m.advance_supply_withdrawal(id).unwrap();
    assert!(m.logistics.get(id).unwrap().stranded);
    assert!(!m.world.game.units[&id].retreating);
    assert_eq!(m.commands.units[&id].supply_retry, Some(40));
    m.convoys.as_mut().unwrap().stores = stores;
    m.world.game.tick = 39;
    m.advance_supply_withdrawal(id).unwrap();
    assert!(m.world.game.units[&id].path.is_empty());
    m.world.game.tick = 40;
    m.begin_path_tick(4096, 20).unwrap();
    m.advance_supply_withdrawal(id).unwrap();
    assert!(!m.world.game.units[&id].path.is_empty());
    let (mut near, id) = scene(true);
    let home = near
        .convoys
        .as_ref()
        .unwrap()
        .stores
        .iter()
        .find(|s| s.owner == 0 && s.source)
        .unwrap()
        .at;
    force(
        &mut near,
        id,
        Point {
            x: home.x + 12.,
            z: home.z,
        },
    );
    near.begin_path_tick(4096, 20).unwrap();
    near.advance_supply_withdrawal(id).unwrap();
    assert!(near.world.game.units[&id].path.is_empty());
    assert!(!near.logistics.get(id).unwrap().stranded);
}
#[test]
fn logistics_runtime_log13_player_hold_escape_stances_and_recovery() {
    let (mut m, id) = scene(true);
    force(&mut m, id, Point { x: 65., z: 45. });
    withdrawal_turn(&mut m, id);
    let ids = vec![i64::from(id)];
    for stranded in [false, true] {
        m.logistics.get_mut(id).unwrap().stranded = stranded;
        for on in [true, false] {
            for stance in [Stance::HoldFire, Stance::HoldPosition, Stance::AutoRetreat] {
                assert_eq!(
                    m.execute_command(
                        0,
                        &Command::Stance {
                            ids: ids.clone(),
                            stance,
                            on
                        },
                        false
                    )
                    .unwrap(),
                    None
                );
                match stance {
                    Stance::HoldFire => assert_eq!(m.commands.units[&id].hold_fire, on),
                    Stance::HoldPosition => assert_eq!(m.world.game.units[&id].hold_pos, on),
                    Stance::AutoRetreat => assert_eq!(m.commands.units[&id].auto_retreat, on),
                }
            }
            m.advance_tick().unwrap();
            assert_deliverable(&mut m);
        }
    }
    m.commands.units.get_mut(&id).unwrap().hold_fire = true;
    assert_eq!(
        m.execute_command(0, &Command::Stop { ids: ids.clone() }, false)
            .unwrap(),
        None
    );
    assert!(m.commands.units[&id].supply_hold);
    withdrawal_turn(&mut m, id);
    assert!(m.logistics.get(id).unwrap().stranded);
    assert!(m.world.game.units[&id].path.is_empty());
    let go = |attack_move| Command::Move {
        orders: vec![MoveOrder {
            unit: i64::from(id),
            at: Point { x: 85., z: 45. },
        }],
        control_ids: None,
        attack_move,
        queue: false,
        together: false,
        face: None,
    };
    assert_eq!(m.execute_command(0, &go(false), false).unwrap(), None);
    assert!(!m.commands.units[&id].supply_hold && m.commands.units[&id].supply_escape);
    assert!(m.world.game.units[&id].retreating);
    assert_eq!(
        m.execute_command(0, &go(true), false).unwrap(),
        Some(Denial::Blocked)
    );
    for _ in 0..3 {
        m.advance_tick().unwrap();
        assert_deliverable(&mut m);
    }
    m.execute_command(0, &Command::Stop { ids: ids.clone() }, false)
        .unwrap();
    m.execute_command(0, &Command::Retreat { ids }, false)
        .unwrap();
    assert!(!m.commands.units[&id].supply_hold);
    let r = m.logistics.get_mut(id).unwrap();
    r.provisions = 120.;
    r.ammo = r.ammo_max;
    withdrawal_turn(&mut m, id);
    assert!(!m.logistics.get(id).unwrap().forced);
    let c = &m.commands.units[&id];
    assert!(!c.supply_started && !c.supply_hold && !c.supply_escape);
    for _ in 0..3 {
        m.advance_tick().unwrap();
        assert_deliverable(&mut m);
    }
}
fn add_actor(m: &mut Match, kind: &str, at: Point) -> u32 {
    let id = m.next_unit_id;
    let u = crate::unit_spawn::initial_unit(
        &m.rules,
        crate::catalog::Catalog::compiled().unwrap(),
        m.rules.kind(kind).unwrap(),
        id,
        0,
        at,
    )
    .unwrap();
    let r = m.prepare_new_unit_reserves(&u).unwrap();
    let mut c = crate::match_commands::UnitControl::new(&u, &m.rules).unwrap();
    c.auto_cast = false;
    c.auto_retreat = false;
    c.hold_fire = true;
    m.commands.units.insert(id, c);
    m.combat.initialize(id);
    m.firing.initialize(id);
    m.abilities.initialize(id);
    m.recovery.initialize(id);
    if let Some(r) = r {
        m.logistics.insert(id, r);
    }
    m.world.game.units.insert(id, u);
    m.next_unit_id += 1;
    m.grid.rebuild(&m.world.game.units);
    id
}
#[test]
fn logistics_runtime_log12_withdrawal_cancels_canonical_build_and_shared_project() {
    use crate::field_works::{FortKind, Pattern, ProjectMaterial, ProjectOrder};
    for project in [false, true] {
        let (mut m, _) = scene(true);
        m.start.economy.players[0].mp = 5000.;
        let at = Point { x: 28., z: 24. };
        let id = add_actor(&mut m, "engineer", at);
        let order = if project {
            Command::Entrench {
                ids: vec![i64::from(id)],
                queue: false,
                order: ProjectOrder::New {
                    pattern: Some(Pattern::Line),
                    material: ProjectMaterial::Fort(FortKind::Trench),
                    from: Some(Point { x: 24., z: 30. }),
                    to: Some(Point { x: 36., z: 30. }),
                },
            }
        } else {
            Command::Build {
                ids: vec![i64::from(id)],
                kind: "flakpos".into(),
                at: Some(Point { x: 32., z: 24. }),
                direction: None,
                queue: false,
            }
        };
        assert_eq!(m.execute_command(0, &order, false).unwrap(), None);
        assert!(m.world.game.units[&id].build || m.world.game.units[&id].entrench);
        // The command owner pays the waiting site and normalizes it to Assist.
        assert_eq!(
            m.execute_command(
                0,
                &Command::Build {
                    ids: vec![i64::from(id)],
                    kind: "flakpos".into(),
                    at: Some(Point { x: 40., z: 24. }),
                    direction: None,
                    queue: true,
                },
                false
            )
            .unwrap(),
            None
        );
        force(&mut m, id, at);
        withdrawal_turn(&mut m, id);
        let u = &m.world.game.units[&id];
        assert!(!u.build && !u.entrench && !u.dig);
        assert!(m.commands.units[&id].build_target.is_none());
        assert!(m.field_works.membership(id).is_none() && !m.field_works.is_active(id));
        assert_deliverable(&mut m);
        withdrawal_turn(&mut m, id);
        assert_deliverable(&mut m);
    }
}
#[test]
fn logistics_runtime_log12_empty_fuel_and_emergency_strands_without_removal() {
    let (mut m, _) = scene(true);
    let at = Point { x: 65., z: 45. };
    let id = add_actor(&mut m, "tank", at);
    force(&mut m, id, at);
    let r = m.logistics.get_mut(id).unwrap();
    r.fuel = Some(0.);
    r.emergency = Some(0.);
    withdrawal_turn(&mut m, id);
    let u = &m.world.game.units[&id];
    assert_eq!(u.at(), at);
    assert!(u.path.is_empty() && !u.retreating && u.hp > 0.);
    assert!(m.logistics.get(id).unwrap().stranded);
    assert_deliverable(&mut m);
    withdrawal_turn(&mut m, id);
    assert_deliverable(&mut m);
}
#[test]
fn logistics_runtime_log13_stranded_defense_cannot_chase_or_interrupt_escape() {
    let (mut m, id) = scene(true);
    force(&mut m, id, Point { x: 65., z: 45. });
    m.commands.units.get_mut(&id).unwrap().supply_hold = true;
    withdrawal_turn(&mut m, id);
    let enemy = m
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 1 && m.rules.kinds[u.kind].name == "rifle")
        .unwrap()
        .id;
    let e = m.world.game.units.get_mut(&enemy).unwrap();
    e.x = 125.;
    e.z = 45.;
    m.grid.rebuild(&m.world.game.units);
    m.world.game.players[0].visible = vec![enemy];
    let attack = Command::Attack {
        ids: vec![i64::from(id)],
        target: i64::from(enemy),
        queue: false,
        at: None,
    };
    assert_eq!(m.execute_command(0, &attack, false).unwrap(), None);
    assert_eq!(m.commands.units[&id].attack_id, enemy);
    m.apply_ground_attack(id).unwrap();
    assert!(m.world.game.units[&id].path.is_empty());
    let ability = Command::Ability {
        ids: vec![i64::from(id)],
        at: Some(Point { x: 70., z: 45. }),
        queue: false,
        automatic: false,
    };
    assert_eq!(m.execute_command(0, &ability, false).unwrap(), None);
    assert!(m.commands.units[&id].nade.is_some());
    m.logistics.get_mut(id).unwrap().stranded = false;
    assert!(m.execute_command(0, &ability, false).unwrap().is_some());
    let before = m.commands.units[&id].attack_id;
    assert!(m.execute_command(0, &attack, false).unwrap().is_some());
    assert_eq!(m.commands.units[&id].attack_id, before);
}
#[test]
fn logistics_runtime_log12_forced_garrison_exits_through_withdrawal_adapter() {
    let (mut m, id) = scene(true);
    let at = Point { x: 65., z: 45. };
    let cell = 22 * 80 + 32;
    m.world.set_cell(cell, b'B').unwrap();
    force(&mut m, id, at);
    m.logistics.get_mut(id).unwrap().forced = false;
    let u = m.world.game.units.get_mut(&id).unwrap();
    u.garrison = Some(cell);
    u.hp = 1.;
    m.commands.units.get_mut(&id).unwrap().auto_retreat = true;
    withdrawal_turn(&mut m, id);
    assert!(m.world.game.units[&id].garrison.is_none());
    assert!(m.world.game.units[&id].hp > 0.);
    assert_deliverable(&mut m);
    withdrawal_turn(&mut m, id);
    assert_deliverable(&mut m);
}
#[test]
fn logistics_runtime_convoy_adapter_keeps_full_ticks_and_snapshots_supported() {
    let (mut m, id) = scene(true);
    force(&mut m, id, Point { x: 65., z: 45. });
    m.logistics.get_mut(id).unwrap().cut_for = Some(10.0);
    m.commands.units.get_mut(&id).unwrap().supply_hold = true;
    // The convoy adapter discovers sources and publishes the truck through its owner.
    m.convoys = crate::supply_convoys::ConvoyState::setup(true);
    m.supply_net.teams.clear();
    let first = m.next_unit_id;
    m.step_supply_convoys(false).unwrap();
    assert!(m.next_unit_id > first);
    assert!(
        m.world
            .game
            .units
            .values()
            .any(|u| u.id >= first && m.rules.kinds[u.kind].name == "truck")
    );
    assert_deliverable(&mut m);
    for _ in 0..3 {
        m.step_supply_convoys(false).unwrap();
        assert_deliverable(&mut m);
    }
    assert!(m.tick_state.fault().is_none());
}

#[test]
fn log13_forced_command_matrix_keeps_only_escape_controls_and_stranded_defense() {
    let (mut m, id) = scene(true);
    force(&mut m, id, Point { x: 65., z: 45. });
    withdrawal_turn(&mut m, id);
    let ids = vec![i64::from(id)];
    let at = Point { x: 85., z: 45. };
    let go = |attack_move| Command::Move {
        orders: vec![MoveOrder {
            unit: i64::from(id),
            at,
        }],
        control_ids: None,
        attack_move,
        queue: false,
        together: false,
        face: None,
    };
    let permitted = [
        go(false),
        Command::Stop { ids: ids.clone() },
        Command::Retreat { ids: ids.clone() },
        Command::Stance {
            ids: ids.clone(),
            stance: Stance::HoldFire,
            on: true,
        },
        Command::Stance {
            ids: ids.clone(),
            stance: Stance::HoldPosition,
            on: true,
        },
        Command::Stance {
            ids: ids.clone(),
            stance: Stance::AutoRetreat,
            on: true,
        },
    ];
    let defense = [
        Command::Attack {
            ids: ids.clone(),
            target: 1,
            queue: false,
            at: None,
        },
        Command::Ability {
            ids: ids.clone(),
            at: Some(at),
            queue: false,
            automatic: false,
        },
    ];
    let denied = [
        go(true),
        Command::Build {
            ids: ids.clone(),
            kind: "cache".into(),
            at: Some(at),
            direction: None,
            queue: false,
        },
        Command::Assist {
            ids: ids.clone(),
            target: 1,
            at: None,
            queue: false,
        },
        Command::Board {
            ids: ids.clone(),
            target: Some(1),
        },
        Command::Unload { ids: ids.clone() },
        Command::Rail {
            ids: ids.clone(),
            at,
        },
        Command::Enter {
            ids: ids.clone(),
            at,
            queue: false,
        },
        Command::FireAt {
            ids: ids.clone(),
            at,
            queue: false,
        },
        Command::Dig {
            ids: ids.clone(),
            kind: crate::field_works::FortKind::Trench,
            at,
            direction: None,
            queue: false,
        },
        Command::Cover {
            ids: ids.clone(),
            queue: false,
            at: None,
        },
        Command::Escort {
            ids: ids.clone(),
            target: 1,
        },
        Command::Autocast { ids, on: true },
    ];
    for stranded in [false, true] {
        m.logistics.get_mut(id).unwrap().stranded = stranded;
        for command in &permitted {
            assert_eq!(
                m.eligible_command_unit(i64::from(id), 0, command),
                Some(id),
                "{command:?}"
            );
            assert_eq!(m.eligible_command_unit(i64::from(id), 1, command), None);
        }
        for command in &defense {
            assert_eq!(
                m.eligible_command_unit(i64::from(id), 0, command),
                stranded.then_some(id),
                "{command:?}"
            );
        }
        for command in &denied {
            assert_eq!(
                m.eligible_command_unit(i64::from(id), 0, command),
                None,
                "{command:?}"
            );
        }
    }
}

#[test]
fn log13_unroutable_direct_escape_stays_stranded_and_stop_holds_across_full_ticks() {
    let (mut m, id) = scene(true);
    force(&mut m, id, Point { x: 65., z: 45. });
    withdrawal_turn(&mut m, id);
    // A destination at the current cell needs no path, even on open terrain.
    let at = m.world.game.units[&id].at();
    assert_eq!(
        m.execute_command(
            0,
            &Command::Move {
                orders: vec![MoveOrder {
                    unit: i64::from(id),
                    at
                }],
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
    assert!(m.commands.units[&id].supply_escape);
    assert!(m.logistics.get(id).unwrap().stranded);
    assert!(!m.world.game.units[&id].retreating);
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
    let start = m.world.game.units[&id].at();
    for _ in 0..4 {
        m.advance_tick().unwrap();
        assert!(m.commands.units[&id].supply_hold);
        assert!(m.logistics.get(id).unwrap().stranded);
        assert!(m.world.game.units[&id].path.is_empty());
        assert_eq!(m.world.game.units[&id].at(), start);
        assert_deliverable(&mut m);
    }
}
