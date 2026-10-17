use super::*;
use crate::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    logistics::Stock,
    map::AuthoritativeMap,
    match_runtime::{MatchOptions, PlayerConfig},
    supply_convoys::{Store, StoreId},
    weather::WeatherChoice,
};
use serde_json::json;

pub(super) fn arena(enabled: bool) -> Match {
    arena_with_barrier(enabled, false)
}
pub(super) fn arena_with_barrier(enabled: bool, barrier: bool) -> Match {
    let mut row = ".".repeat(80);
    if barrier {
        row.replace_range(40..42, "WW");
    }
    let point_x = if barrier { 36 } else { 40 };
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name": "logistics runtime arena", "w": 80, "h": 80,
            "rows": vec![row; 80],
            "spawns": [{"x":12,"y":40},{"x":68,"y":40}],
            "points": [{"x":point_x,"y":8,"vp":0,"mp":0}], "defend":[0]
        }))
        .unwrap(),
    )
    .unwrap();
    Match::new(
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
    .unwrap()
}
pub(super) fn troop(m: &Match) -> u32 {
    m.world
        .game
        .units
        .values()
        .find(|u| u.owner == 0 && m.rules.kinds[u.kind].name == "rifle")
        .unwrap()
        .id
}
pub(super) fn place(m: &mut Match, id: u32, at: Point) {
    let u = m.world.game.units.get_mut(&id).unwrap();
    u.x = at.x;
    u.z = at.z;
    m.grid.update(id, u);
}
fn handoff(m: &mut Match, id: u32, at: Point, entity: Option<u32>, radius: f64) -> bool {
    physical_handoff(
        &m.world,
        &m.rules,
        &m.combat,
        &m.commands,
        &mut m.supply_routes,
        PhysicalHandoff {
            unit: id,
            at,
            entity,
            radius,
        },
    )
}
fn source(m: &mut Match, at: Point) {
    m.convoys.as_mut().unwrap().stores = vec![Store {
        id: StoreId::Source(0),
        owner: 0,
        at,
        source: true,
        entity: None,
        capacity: Stock::default(),
        buckets: vec![],
        donations: vec![],
        active: true,
        support: false,
        point: None,
    }];
    // Keep this explicit source in place while exercising local service.
    let s = m.convoys.as_mut().unwrap();
    s.next_schedule = 1000;
    s.schedule_queue.clear();
    s.jobs.clear();
}

#[test]
fn intent_log18_garrison_entry_fallback_carrier_and_real_service() {
    let mut m = arena(true);
    let id = troop(&m);
    let inside = Point { x: 81., z: 81. };
    let door = Point { x: 83., z: 81. };
    let store = Point { x: 89., z: 81. };
    place(&mut m, id, inside);
    let cell = m.world.game.cell_of(inside.x, inside.z).unwrap();
    m.world.game.flags[cell] = MOVE | crate::game::SIGHT;
    m.world.game.units.get_mut(&id).unwrap().garrison = Some(cell);
    m.commands.units.get_mut(&id).unwrap().garrison_entry = Some(door);
    source(&mut m, store);
    m.logistics.get_mut(id).unwrap().provisions = 40.;
    m.validate_supply().unwrap();
    assert!(handoff(&mut m, id, store, None, 15.));
    assert!(m.spend_support_provisions(id, 0, 5.).unwrap());
    assert_eq!(
        m.logistics.get(id).unwrap().provisions,
        40.,
        "reachable source pays support"
    );
    m.step_supply_convoys(false).unwrap();
    assert!(
        m.logistics.get(id).unwrap().provisions > 40.,
        "local service refills the garrison"
    );
    assert_eq!(m.world.game.units[&id].at(), inside);
    assert_eq!(m.world.game.units[&id].garrison, Some(cell));

    // With no remembered entry, the MOVE-only nearest-free tie goes right.
    m.commands.units.get_mut(&id).unwrap().garrison_entry = None;
    assert!(handoff(&mut m, id, door, None, 0.));
    assert!(!handoff(&mut m, id, Point { x: 79., z: 81. }, None, 0.));
    m.commands.units.get_mut(&id).unwrap().garrison_entry = Some(Point { x: 73., z: 81. });
    assert!(!handoff(&mut m, id, store, None, 15.));
    m.commands.units.get_mut(&id).unwrap().garrison_entry = Some(Point { x: 74., z: 81. });
    // An open line at exactly the radius still permits handoff.
    assert!(handoff(&mut m, id, Point { x: 74., z: 96. }, None, 15.));

    let carrier = m
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 0 && u.id != id)
        .unwrap()
        .id;
    place(&mut m, carrier, store);
    m.world.game.units.get_mut(&id).unwrap().riding = true;
    m.combat.get_mut(id).unwrap().carrier = Some(carrier);
    assert!(
        handoff(&mut m, id, store, None, 0.),
        "carrier wins over remembered entry"
    );
    m.world.game.units.get_mut(&id).unwrap().riding = false;
    m.combat.get_mut(id).unwrap().carrier = None;
    m.commands.units.get_mut(&id).unwrap().garrison_entry = Some(Point { x: f64::NAN, z: 0. });
    assert!(m.validate_supply().is_err());
    m.world.game.units.get_mut(&id).unwrap().garrison = None;
    place(&mut m, id, store);
    m.validate_supply().unwrap();
    assert!(
        handoff(&mut m, id, store, None, 0.),
        "stale entry is ignored outside the house"
    );
}

#[test]
fn intent_log18_footprint_and_center_range_boundaries() {
    let mut m = arena(true);
    let id = troop(&m);
    place(&mut m, id, Point { x: 75., z: 81. });
    let building = m
        .world
        .game
        .units
        .values()
        .find(|u| u.id != id && u.owner == 0)
        .unwrap()
        .id;
    let at = Point { x: 81., z: 81. };
    let cell = m.world.game.cell_of(at.x, at.z).unwrap();
    m.world.game.flags[cell] = MOVE | VBLOCK;
    m.world.game.units.get_mut(&building).unwrap().cells = Some(vec![cell as u32]);
    assert!(
        !handoff(&mut m, id, at, None, 15.),
        "blocked store center needs its footprint"
    );
    assert!(handoff(&mut m, id, at, Some(building), 15.));
    place(&mut m, id, Point { x: 65., z: 81. });
    assert!(
        !handoff(&mut m, id, at, Some(building), 15.),
        "edge trim cannot extend center radius"
    );
}

fn barrier(m: &mut Match, from: usize, through: usize) {
    for y in 0..m.world.game.h {
        m.world.game.flags[y * m.world.game.w + 40] = if (from..=through).contains(&y) {
            MOVE | VBLOCK
        } else {
            0
        };
    }
}
#[test]
fn intent_log18_bounded_detours_and_epoch_terrain_cache() {
    let mut m = arena(true);
    let id = troop(&m);
    place(&mut m, id, Point { x: 75., z: 81. });
    let store = Point { x: 87., z: 81. };
    barrier(&mut m, 39, 41);
    assert!(
        handoff(&mut m, id, store, None, 15.),
        "short infantry detour is reachable"
    );
    // Use stable terrain and two radii in the same quarter-metre cache bucket.
    // The navigation route independently supplies the detour's measured length.
    barrier(&mut m, 29, 51);
    m.world.versions.infantry += 1;
    let n = m.world.game.flags.len();
    let near = vec![0; n];
    let empty = vec![false; n];
    let terrain = PathTerrain {
        navigation: view(&m.world.game),
        near_walls: &near,
        known_mines: &empty,
        fires: &empty,
        bunkers: &[],
        wear: m.world.game.wear.as_deref(),
        wetness: m.world.game.wet.unwrap_or(0.),
        roads: m.world.roads,
    };
    let path = crate::pathfinding::Pathfinding::new()
        .find_path(
            terrain,
            crate::pathfinding::PathRules::compiled().unwrap(),
            &m.rules.kinds[m.rules.kind("rifle").unwrap()],
            m.world.game.units[&id].at(),
            store,
            None,
        )
        .unwrap()
        .waypoints;
    assert!(!path.is_empty());
    let mut from = m.world.game.units[&id].at();
    let mut length = 0.;
    for next in path {
        length += math::hypot(next.x - from.x, next.z - from.z);
        from = next;
    }
    assert!(math::hypot(from.x - store.x, from.z - store.z) < 1.);
    let low = length / 2. - 0.00001;
    let high = length / 2. + 0.00001;
    assert!(
        low > math::hypot(store.x - 75., store.z - 81.),
        "measured detour length {length}"
    );
    assert_eq!(math::round(low * 4.), math::round(high * 4.));
    assert!(handoff(&mut m, id, store, None, high));
    assert!(
        handoff(&mut m, id, store, None, low),
        "same-epoch answer is cached"
    );
    m.world.game.tick = 40;
    assert!(
        !handoff(&mut m, id, store, None, low),
        "epoch expiry applies the stricter route budget"
    );
    m.world.versions.infantry += 1;
    assert!(handoff(&mut m, id, store, None, high));
    m.world.versions.obstruction += 1;
    assert!(!handoff(&mut m, id, store, None, low));
    m.world.versions.vehicle += 1;
    assert!(
        !handoff(&mut m, id, store, None, high),
        "vehicle-only changes preserve infantry cache"
    );
    m.world.game.tick = 80;
    assert!(handoff(&mut m, id, store, None, high));
    barrier(&mut m, 29, 51);
    m.world.versions.infantry += 1;
    assert!(
        !handoff(&mut m, id, store, None, 15.),
        "connected detour beyond twice radius is refused"
    );
    barrier(&mut m, 0, 79);
    m.world.versions.obstruction += 1;
    assert!(
        !handoff(&mut m, id, store, None, 15.),
        "disconnected handoff cannot transfer"
    );
    source(&mut m, store);
    m.logistics.get_mut(id).unwrap().provisions = 4.;
    assert!(!m.spend_support_provisions(id, 0, 5.).unwrap());
    assert_eq!(m.logistics.get(id).unwrap().provisions, 4.);
    m.logistics.get_mut(id).unwrap().provisions = 10.;
    assert!(m.spend_support_provisions(id, 0, 5.).unwrap());
    assert_eq!(
        m.logistics.get(id).unwrap().provisions,
        5.,
        "unreachable source cannot pay support"
    );
}

fn add_truck(m: &mut Match, owner: usize, at: Point) -> u32 {
    m.with_supply_world(|_, state, _, world| {
        let mut s = state
            .stores
            .iter()
            .find(|s| s.source && s.owner == owner)
            .unwrap()
            .clone();
        s.at = at;
        Ok(state.add_truck(owner, &s, world))
    })
    .unwrap()
}
#[test]
fn intent_log31_real_truck_birth_has_no_combat_cost_reserves_or_auto_retreat() {
    let mut m = arena(true);
    let at = Point { x: 81., z: 81. };
    let id = add_truck(&mut m, 0, at);
    let u = &m.world.game.units[&id];
    let catalog = Catalog::compiled().unwrap();
    let truck = catalog.unit("truck").unwrap();
    for (key, value) in [
        ("hpPer", 120.),
        ("speed", 7.5),
        ("radius", 1.8),
        ("vision", 12.),
        ("cost", 0.),
        ("pop", 0.),
    ] {
        assert_eq!(truck.get(key).and_then(|v| v.number()), Some(value));
    }
    assert_eq!(u.hp, 120.);
    assert_eq!(m.rules.kinds[u.kind].name, "truck");
    assert_eq!(
        truck.get("w").unwrap().get("range").unwrap().number(),
        Some(0.)
    );
    assert!(!m.commands.units[&id].auto_retreat);
    assert!(m.firing.get(id).is_some() && m.combat.get(id).is_some());
    assert!(m.abilities.get(id).is_some() && m.recovery.get(id).is_some());
    assert!(m.logistics.get(id).is_none());
    let definition = Definition::from_catalog("truck").unwrap();
    assert!(
        m.logistics
            .initialize_unit(reserve_context(true), facts(u, &m.rules), Some(&definition))
            .is_none()
    );
    for faction in [Faction::Usa, Faction::Germany, Faction::Ussr, Faction::Uk] {
        assert!(!crate::economy::can_build("truck", faction).unwrap());
    }
    let c = &m.convoys.as_ref().unwrap().trucks[&id];
    assert_eq!(c.state, supply_convoys::TruckState::Idle);
    assert_eq!(c.cargo, Stock::default());
    assert!(!c.manual && !c.hold);
}

#[test]
fn intent_log32_real_fleet_cap_spawn_beat_ring_retirement_and_loss_delay() {
    let mut m = arena(true);
    let at = Point { x: 81., z: 81. };
    source(&mut m, at);
    m.convoys.as_mut().unwrap().jobs.insert(0, 8);
    for (tick, count) in [(1, 1), (1, 1), (20, 1), (21, 2), (41, 3), (61, 4), (81, 4)] {
        m.world.game.tick = tick;
        m.step_supply_convoys_with_dt(false, 0.).unwrap();
        assert_eq!(
            m.convoys.as_ref().unwrap().trucks.len(),
            count,
            "tick {tick}"
        );
    }
    let ids = m
        .convoys
        .as_ref()
        .unwrap()
        .trucks
        .keys()
        .copied()
        .collect::<Vec<_>>();
    assert_eq!(m.world.game.units[&ids[0]].at(), Point { x: 91., z: 81. });
    let expected_second = Point {
        x: ((at.x + math::cos(2.4) * 10.) / CELL).floor() * CELL + 1.,
        z: ((at.z + math::sin(2.4) * 10.) / CELL).floor() * CELL + 1.,
    };
    assert_eq!(m.world.game.units[&ids[1]].at(), expected_second);
    for (i, id) in ids.iter().enumerate() {
        assert_eq!(
            m.convoys.as_ref().unwrap().trucks[id].origin,
            StoreId::Source(0)
        );
        assert!(
            math::hypot(
                m.world.game.units[id].x - at.x,
                m.world.game.units[id].z - at.z
            ) <= 15.
        );
        for other in &ids[..i] {
            assert!(
                math::hypot(
                    m.world.game.units[id].x - m.world.game.units[other].x,
                    m.world.game.units[id].z - m.world.game.units[other].z
                ) > 3.88
            );
        }
    }
    // With two open jobs, newest eligible trucks retire first.
    m.convoys.as_mut().unwrap().jobs.insert(0, 2);
    m.step_supply_convoys_with_dt(false, 0.).unwrap();
    assert_eq!(
        m.convoys
            .as_ref()
            .unwrap()
            .trucks
            .keys()
            .copied()
            .collect::<Vec<_>>(),
        ids[..2]
    );
    for id in &ids[2..] {
        assert!(!m.world.game.units.contains_key(id));
        assert!(!m.commands.units.contains_key(id));
        assert!(m.firing.get(*id).is_none() && m.combat.get(*id).is_none());
        assert!(m.abilities.get(*id).is_none() && m.recovery.get(*id).is_none());
    }
    m.convoys.as_mut().unwrap().jobs.insert(0, 0);
    m.convoys
        .as_mut()
        .unwrap()
        .trucks
        .get_mut(&ids[0])
        .unwrap()
        .cargo
        .provisions = 1.;
    m.convoys
        .as_mut()
        .unwrap()
        .trucks
        .get_mut(&ids[1])
        .unwrap()
        .hold = true;
    m.step_supply_convoys_with_dt(false, 0.).unwrap();
    assert_eq!(
        m.convoys.as_ref().unwrap().trucks.len(),
        2,
        "cargo and held trucks survive shrink"
    );
    m.convoys
        .as_mut()
        .unwrap()
        .trucks
        .get_mut(&ids[0])
        .unwrap()
        .cargo
        .provisions = 0.;
    m.convoys
        .as_mut()
        .unwrap()
        .trucks
        .get_mut(&ids[1])
        .unwrap()
        .hold = false;
    // Orderless is required, even for an otherwise empty truck at its source.
    m.world.game.units.get_mut(&ids[0]).unwrap().path = vec![Point { x: 95., z: 81. }];
    m.step_supply_convoys_with_dt(false, 0.).unwrap();
    assert_eq!(m.convoys.as_ref().unwrap().trucks.len(), 1);
    assert!(m.world.game.units.contains_key(&ids[0]));
    m.world.game.units.get_mut(&ids[0]).unwrap().path.clear();
    let dead = m.world.game.units[&ids[0]].clone();
    m.record_convoy_death(&dead).unwrap();
    m.with_supply_world(|_, _, _, world| {
        supply_convoys::World::retire(world, dead.id);
        Ok(())
    })
    .unwrap();
    m.convoys.as_mut().unwrap().jobs.insert(0, 1);
    for (tick, count) in [(680, 0), (681, 1)] {
        m.world.game.tick = tick;
        m.step_supply_convoys_with_dt(false, 0.).unwrap();
        assert_eq!(m.convoys.as_ref().unwrap().trucks.len(), count);
    }
    // Absent players do not grow a fleet despite queued demand.
    for (out, away) in [(true, false), (false, true)] {
        m.start.economy.players[0].out = out;
        m.start.economy.players[0].away = away;
        m.convoys.as_mut().unwrap().jobs.insert(0, 8);
        m.world.game.tick += 20;
        m.step_supply_convoys_with_dt(false, 0.).unwrap();
        assert_eq!(m.convoys.as_ref().unwrap().trucks.len(), 1);
    }
}

#[test]
fn logistics_off_keeps_reserves_and_convoys_absent() {
    let mut m = arena(false);
    let before = m.world.game.units.len();
    m.initialize_logistics().unwrap();
    m.step_supply_convoys(false).unwrap();
    assert!(m.convoys.is_none());
    assert!(
        m.world
            .game
            .units
            .keys()
            .all(|id| m.logistics.get(*id).is_none())
    );
    assert_eq!(m.world.game.units.len(), before);
}

#[test]
fn intent_log32_busy_demand_own_source_and_obstructed_ring() {
    let mut m = arena(true);
    let at = Point { x: 81., z: 81. };
    source(&mut m, at);
    // An allied source is listed first, but this player's own source wins.
    m.world.game.players[1].team = 0;
    m.start.economy.players[1].team = 0;
    m.start.economy.players[1].away = true;
    let mut ally = m.convoys.as_ref().unwrap().stores[0].clone();
    ally.id = StoreId::Source(1);
    ally.owner = 1;
    ally.at = Point { x: 41., z: 41. };
    m.convoys.as_mut().unwrap().stores.insert(0, ally);
    let blocker = troop(&m);
    place(&mut m, blocker, Point { x: 91., z: 81. });
    m.convoys.as_mut().unwrap().jobs.insert(0, 1);
    m.world.game.tick = 1;
    m.step_supply_convoys_with_dt(false, 0.).unwrap();
    let id = *m.convoys.as_ref().unwrap().trucks.keys().next().unwrap();
    assert_eq!(
        m.convoys.as_ref().unwrap().trucks[&id].origin,
        StoreId::Source(0)
    );
    let expected = Point {
        x: ((at.x + math::cos(2.4) * 10.) / CELL).floor() * CELL + 1.,
        z: ((at.z + math::sin(2.4) * 10.) / CELL).floor() * CELL + 1.,
    };
    assert_eq!(
        m.world.game.units[&id].at(),
        expected,
        "occupied first ring candidate must be skipped"
    );
    m.convoys
        .as_mut()
        .unwrap()
        .trucks
        .get_mut(&id)
        .unwrap()
        .hold = true;
    m.world.game.tick = 21;
    m.step_supply_convoys_with_dt(false, 0.).unwrap();
    assert_eq!(
        m.convoys.as_ref().unwrap().trucks.len(),
        2,
        "one busy truck plus one open job needs two trucks"
    );
}

#[test]
fn intent_log32_inactive_allied_source_blocks_retirement_until_activated() {
    let mut m = arena(true);
    let at = Point { x: 81., z: 81. };
    source(&mut m, at);
    let id = add_truck(&mut m, 0, at);
    m.world.game.players[1].team = 0;
    m.start.economy.players[1].team = 0;
    {
        let state = m.convoys.as_mut().unwrap();
        state.stores[0].id = StoreId::Source(1);
        state.stores[0].owner = 1;
        state.trucks.get_mut(&id).unwrap().origin = StoreId::Source(1);
    }

    for active in [false, true] {
        // Only source activation changes between the two fleet passes.
        m.convoys.as_mut().unwrap().stores[0].active = active;
        let state = m.convoys.as_ref().unwrap();
        assert_eq!(state.stores.len(), 1);
        let store = &state.stores[0];
        assert_eq!(store.id, StoreId::Source(1));
        assert_eq!(store.owner, 1);
        assert!(store.source);
        assert_eq!(store.at, at);
        assert_eq!(m.world.game.players[store.owner].team, 0);
        assert!(!m.start.economy.players[0].out && !m.start.economy.players[0].away);
        assert!(state.jobs.is_empty());
        assert_eq!(state.trucks.len(), 1);
        let truck = &state.trucks[&id];
        assert_eq!(truck.state, supply_convoys::TruckState::Idle);
        assert_eq!(truck.cargo, Stock::default());
        assert!(!truck.manual && !truck.hold);
        let unit = &m.world.game.units[&id];
        assert_eq!(unit.owner, 0);
        assert_eq!(m.rules.kinds[unit.kind].name, "truck");
        assert!(unit.hp > 0.);
        assert!(unit.path.is_empty() && unit.world_goal.is_none());
        assert!(m.commands.units[&id].orders.is_empty());
        assert!(math::hypot(unit.x - at.x, unit.z - at.z) <= 15.);
        assert!(handoff(&mut m, id, at, None, 15.));

        m.step_supply_convoys_with_dt(false, 0.).unwrap();
        assert_eq!(
            m.convoys.as_ref().unwrap().trucks.contains_key(&id),
            !active,
            "retirement requires the allied source to be active"
        );
        assert_eq!(m.world.game.units.contains_key(&id), !active);
        assert_eq!(m.commands.units.contains_key(&id), !active);
    }
}

#[test]
fn intent_log32_retirement_requires_idle_empty_orderless_reachable_source() {
    use crate::match_commands::{Command, MoveOrder};
    use supply_convoys::TruckState;
    for reason in ["manual", "busy", "goal", "order", "distant", "blocked"] {
        let mut m = arena(true);
        let at = Point { x: 81., z: 81. };
        source(&mut m, at);
        let first = add_truck(&mut m, 0, at);
        let second = add_truck(&mut m, 0, at);
        // Busy/manual trucks must be examined before the eligible surplus.
        let (protected, surplus) = if matches!(reason, "manual" | "busy") {
            (second, first)
        } else {
            (first, second)
        };
        match reason {
            "manual" => {
                m.convoys
                    .as_mut()
                    .unwrap()
                    .trucks
                    .get_mut(&protected)
                    .unwrap()
                    .manual = true
            }
            "busy" => {
                let c = m
                    .convoys
                    .as_mut()
                    .unwrap()
                    .trucks
                    .get_mut(&protected)
                    .unwrap();
                c.state = TruckState::Loading;
                c.timer = 100.;
            }
            "goal" => {
                m.world.game.units.get_mut(&protected).unwrap().world_goal =
                    Some(Point { x: 95., z: 81. })
            }
            "order" => m
                .commands
                .units
                .get_mut(&protected)
                .unwrap()
                .orders
                .push_back(Command::Move {
                    control_ids: None,
                    orders: vec![MoveOrder {
                        unit: i64::from(protected),
                        at: Point { x: 95., z: 81. },
                    }],
                    attack_move: false,
                    queue: false,
                    together: false,
                    face: None,
                }),
            "distant" => place(&mut m, protected, Point { x: 121., z: 81. }),
            "blocked" => {
                // The first truck is east of this barrier, the second and source west.
                for y in 0..80 {
                    m.world.game.flags[y * 80 + 43] = MOVE | VBLOCK;
                }
                m.world.versions.infantry += 1;
                assert!(!handoff(&mut m, protected, at, None, 15.));
                assert!(handoff(&mut m, surplus, at, None, 15.));
            }
            _ => unreachable!(),
        }
        m.step_supply_convoys_with_dt(false, 0.).unwrap();
        assert!(
            m.convoys.as_ref().unwrap().trucks.contains_key(&protected),
            "{reason}"
        );
        assert!(
            !m.convoys.as_ref().unwrap().trucks.contains_key(&surplus),
            "eligible surplus must retire: {reason}"
        );
    }
}

#[test]
fn if25_match_convoy_retreat_equal_distance_keeps_active_source_insertion_order() {
    for reverse in [false, true] {
        let mut m = arena(true);
        let truck = m.test_supply_truck(0);
        place(&mut m, truck, Point { x: 81.0, z: 81.0 });
        let points = [Point { x: 61.0, z: 81.0 }, Point { x: 101.0, z: 81.0 }];
        source(&mut m, points[0]);
        let mut other = m.convoys.as_ref().unwrap().stores[0].clone();
        other.id = StoreId::Source(900);
        other.at = points[1];
        m.convoys.as_mut().unwrap().stores.push(other);
        if reverse {
            m.convoys.as_mut().unwrap().stores.reverse();
        }
        let expected = m.convoys.as_ref().unwrap().stores[0].at;
        let mut inactive = m.convoys.as_ref().unwrap().stores[0].clone();
        inactive.id = StoreId::Source(901);
        inactive.at = Point { x: 81.0, z: 81.0 };
        inactive.active = false;
        m.convoys.as_mut().unwrap().stores.insert(0, inactive);
        assert_eq!(
            m.execute_command(
                0,
                &crate::match_commands::Command::Retreat {
                    ids: vec![i64::from(truck)],
                },
                false
            )
            .unwrap(),
            None
        );
        assert_eq!(m.world.game.units[&truck].world_goal, Some(expected));
        assert!(m.world.game.units[&truck].retreating);
        let row = &m.convoys.as_ref().unwrap().trucks[&truck];
        assert!(row.manual && !row.hold);
    }
}

#[test]
fn if25_match_withdrawal_equal_distance_keeps_store_insertion_order() {
    for reverse in [false, true] {
        let mut m = arena(true);
        let id = troop(&m);
        place(&mut m, id, Point { x: 81.0, z: 81.0 });
        let points = [Point { x: 61.0, z: 81.0 }, Point { x: 101.0, z: 81.0 }];
        source(&mut m, points[0]);
        let mut other = m.convoys.as_ref().unwrap().stores[0].clone();
        other.id = StoreId::Source(900);
        other.at = points[1];
        m.convoys.as_mut().unwrap().stores.push(other);
        if reverse {
            m.convoys.as_mut().unwrap().stores.reverse();
        }
        let expected = m.convoys.as_ref().unwrap().stores[0].at;
        m.logistics.get_mut(id).unwrap().forced = true;
        m.advance_supply_withdrawal(id).unwrap();
        let expected = Point {
            x: 81.0 + (expected.x - 81.0) * 0.5,
            z: 81.0,
        };
        assert_eq!(m.world.game.units[&id].world_goal, Some(expected));
        assert!(m.world.game.units[&id].retreating);
        assert!(m.commands.units[&id].supply_started);
        assert!(!m.logistics.get(id).unwrap().stranded);
    }
}
