use crate::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::Point,
    map::AuthoritativeMap,
    match_commands::{Command, Denial, MoveOrder, Stance},
    match_runtime::{Match, MatchOptions, PlayerConfig},
    match_tick::TickPhase,
    recipient_snapshot::{DeliveryState, SnapshotCapability},
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
fn snapshot(m: &mut Match, d: &mut DeliveryState) {
    let tick = m.world.game.tick;
    let frame = d.capture(m, &[true, true], &[false, false]).unwrap();
    for owner in 0..2 {
        let projection = frame.recipient(owner).unwrap();
        assert_eq!(projection.fields["tick"], tick);
        assert!(
            projection.fields["units"]
                .as_array()
                .unwrap()
                .iter()
                .any(|r| r[2] == owner)
        );
        // The existing internal projection explicitly retains its missing logistics payload.
        assert!(projection.missing.contains(&SnapshotCapability::Logistics));
    }
}
#[test]
fn logistics_runtime_ordinary_matches_tick_and_project_with_logistics_off_and_on() {
    for enabled in [false, true] {
        let (mut m, id) = scene(enabled);
        let mut d = DeliveryState::new(&m).unwrap();
        snapshot(&mut m, &mut d);
        for _ in 0..45 {
            let report = m.advance_tick().unwrap();
            assert_eq!(report.phases.contains(&TickPhase::UnitReserves), enabled);
            assert_eq!(report.phases.contains(&TickPhase::Convoys), enabled);
            snapshot(&mut m, &mut d);
        }
        assert!(m.tick_state.fault().is_none());
        if enabled {
            assert!(m.logistics.get(id).unwrap().provisions > 0.);
            assert_eq!(m.supply_net.width, 80);
        }
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
    m.advance_tick().unwrap();
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
    let mut d = DeliveryState::new(&m).unwrap();
    snapshot(&mut m, &mut d);
    for _ in 0..20 {
        m.advance_tick().unwrap();
        snapshot(&mut m, &mut d);
    }
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
    m.advance_tick().unwrap();
    let ids = vec![i64::from(id)];
    for stance in [Stance::HoldFire, Stance::HoldPosition, Stance::AutoRetreat] {
        assert_eq!(
            m.execute_command(
                0,
                &Command::Stance {
                    ids: ids.clone(),
                    stance,
                    on: true
                },
                false
            )
            .unwrap(),
            None
        );
    }
    assert!(
        m.commands.units[&id].hold_fire
            && m.commands.units[&id].auto_retreat
            && m.world.game.units[&id].hold_pos
    );
    assert_eq!(
        m.execute_command(0, &Command::Stop { ids: ids.clone() }, false)
            .unwrap(),
        None
    );
    assert!(m.commands.units[&id].supply_hold);
    m.advance_tick().unwrap();
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
    let mut d = DeliveryState::new(&m).unwrap();
    m.advance_tick().unwrap();
    snapshot(&mut m, &mut d);
    m.execute_command(0, &Command::Stop { ids: ids.clone() }, false)
        .unwrap();
    m.execute_command(0, &Command::Retreat { ids }, false)
        .unwrap();
    assert!(!m.commands.units[&id].supply_hold);
    let r = m.logistics.get_mut(id).unwrap();
    r.provisions = 120.;
    r.ammo = r.ammo_max;
    m.advance_tick().unwrap();
    assert!(!m.logistics.get(id).unwrap().forced);
    let c = &m.commands.units[&id];
    assert!(!c.supply_started && !c.supply_hold && !c.supply_escape);
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
        m.advance_tick().unwrap();
        let u = &m.world.game.units[&id];
        assert!(!u.build && !u.entrench && !u.dig);
        assert!(m.commands.units[&id].build_target.is_none());
        assert!(m.field_works.membership(id).is_none() && !m.field_works.is_active(id));
        let mut d = DeliveryState::new(&m).unwrap();
        snapshot(&mut m, &mut d);
        m.advance_tick().unwrap();
        snapshot(&mut m, &mut d);
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
    m.advance_tick().unwrap();
    let u = &m.world.game.units[&id];
    assert_eq!(u.at(), at);
    assert!(u.path.is_empty() && !u.retreating && u.hp > 0.);
    assert!(m.logistics.get(id).unwrap().stranded);
    let mut d = DeliveryState::new(&m).unwrap();
    snapshot(&mut m, &mut d);
    m.advance_tick().unwrap();
    snapshot(&mut m, &mut d);
}
#[test]
fn logistics_runtime_log13_stranded_defense_cannot_chase_or_interrupt_escape() {
    let (mut m, id) = scene(true);
    force(&mut m, id, Point { x: 65., z: 45. });
    m.commands.units.get_mut(&id).unwrap().supply_hold = true;
    m.advance_tick().unwrap();
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
fn logistics_runtime_log12_forced_garrison_exits_before_ordinary_retreat_guard() {
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
    m.advance_tick().unwrap();
    assert!(m.world.game.units[&id].garrison.is_none());
    assert!(m.world.game.units[&id].hp > 0.);
    let mut d = DeliveryState::new(&m).unwrap();
    snapshot(&mut m, &mut d);
    m.advance_tick().unwrap();
    snapshot(&mut m, &mut d);
}
#[test]
fn logistics_runtime_late_convoy_birth_keeps_next_ticks_and_projections_valid() {
    let (mut m, id) = scene(true);
    force(&mut m, id, Point { x: 65., z: 45. });
    m.logistics.get_mut(id).unwrap().cut_for = Some(10.0);
    m.commands.units.get_mut(&id).unwrap().supply_hold = true;
    // Start a fresh scheduling cycle with an already cut-off troop. The real
    // convoy phase discovers sources and publishes the truck through its owner.
    m.convoys = crate::supply_convoys::ConvoyState::setup(true);
    m.supply_net.teams.clear();
    let first = m.next_unit_id;
    m.advance_tick().unwrap();
    assert!(m.next_unit_id > first);
    assert!(
        m.world
            .game
            .units
            .values()
            .any(|u| u.id >= first && m.rules.kinds[u.kind].name == "truck")
    );
    let mut d = DeliveryState::new(&m).unwrap();
    snapshot(&mut m, &mut d);
    for _ in 0..3 {
        m.advance_tick().unwrap();
        snapshot(&mut m, &mut d);
    }
    assert!(m.tick_state.fault().is_none());
}
