//! Combat shelter and recovery intent through the canonical native owners.
use crate::{
    catalog::Catalog,
    combat::{Impact, SuppressionInput, WeaponDamage},
    economy::{Faction, Mode},
    economy_state::ArmySize,
    environment::{Blast, DamageContext},
    game::Point,
    map::AuthoritativeMap,
    match_combat::{DamageWorld, ExplosionSource, FlightSource},
    match_commands::{Command, UnitControl},
    match_firing::FiringState,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    movement, terrain, unit_spawn, vision,
    weather::WeatherChoice,
};
use serde_json::json;
use std::collections::BTreeSet;

fn scene() -> Match {
    let mut rows = vec![".".repeat(64); 40];
    rows[20].replace_range(30..31, "B");
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"Shelter and recovery", "w":64, "h":40,
            "rows": rows, "heights": vec!["0".repeat(64);40],
            "spawns":[{"x":8,"y":10},{"x":52,"y":30}],
            "points":[{"x":4,"y":36,"vp":0,"mp":0}]
        }))
        .unwrap(),
    )
    .unwrap();
    let mut m = Match::new(
        &map,
        MatchOptions {
            seed: 740,
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
    for c in m.commands.units.values_mut() {
        c.auto_retreat = false;
        c.auto_cast = false;
        c.hold_fire = true;
    }
    m.world.game.rain = 0.;
    m.world.game.wet = Some(0.);
    m.world.game.weather = None;
    m
}

fn add(m: &mut Match, name: &str, owner: usize, at: Point) -> u32 {
    let id = m.next_unit_id;
    m.next_unit_id += 1;
    let kind = m.rules.kind(name).unwrap();
    let make = if m.rules.kinds[kind].air {
        unit_spawn::initial_aircraft
    } else {
        unit_spawn::initial_unit
    };
    let mut u = make(&m.rules, Catalog::compiled().unwrap(), kind, id, owner, at).unwrap();
    u.still = Some(10.);
    if m.rules.kinds[kind].building {
        u.built = Some(1.);
    }
    let mut control = UnitControl::new(&u, &m.rules).unwrap();
    control.auto_retreat = false;
    control.auto_cast = false;
    control.hold_fire = true;
    m.commands.insert(id, control);
    m.firing.initialize(id);
    m.combat.initialize(id);
    m.abilities.initialize(id);
    m.recovery.initialize(id);
    m.world.game.units.insert(id, u);
    m.grid.rebuild(&m.world.game.units);
    id
}

fn damage<'a>(m: &'a mut Match, order: &'a [u32]) -> (DamageWorld<'a>, &'a mut FiringState) {
    (
        DamageWorld {
            world: &mut m.world,
            combat: &mut m.combat,
            grid: &mut m.grid,
            rules: &m.rules,
            logistics: &m.logistics,
            production: &m.production,
            mode: m.options.mode,
            tech_enabled: false,
            environment: &mut m.environment,
            environment_seed: &mut m.start.environment_seed,
            next_id: &mut m.next_unit_id,
            unit_order: order,
            wind_angle: 0.,
            wind_strength: 0.,
        },
        &mut m.firing,
    )
}

fn profile(anti_garrison: Option<f64>) -> ExplosionSource {
    ExplosionSource {
        infantry: 20.,
        vehicle: 20.,
        terrain: None,
        suppression: 0.,
        scar: 0.,
        anti_garrison,
        weapon_kind: None,
        off_map_support: false,
    }
}

fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-8, "{a} != {b}");
}

fn world_scene() -> Match {
    // Only the authored map is reused. No captured action or expected state drives these checks.
    let authored: serde_json::Value =
        serde_json::from_str(include_str!("../../tests/fixtures/world-recovery.json")).unwrap();
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&authored["map"]).unwrap()).unwrap();
    let mut options = scene().options;
    options.mode = Mode::World;
    options.players[1].team = 0;
    Match::new_world(&map, options, crate::random::MatchRandom::new(740)).unwrap()
}

#[test]
fn cmb74_trench_front_settling_blast_concealment_and_speed() {
    let mut m = scene();
    let at = Point { x: 61., z: 41. };
    let id = add(&mut m, "rifle", 0, at);
    let c = m.world.game.cell_of(at.x, at.z).unwrap();
    m.world.set_cell(c, b'T').unwrap();
    m.world.game.trench_front = Some(vec![f32::NAN; m.world.game.chars.len()]);
    m.world.game.trench_front.as_mut().unwrap()[c] = 0.;
    m.world.game.units.get_mut(&id).unwrap().still = Some(0.);
    let front = Point { x: 81., z: 41. };
    let rear = Point { x: 41., z: 41. };
    close(m.cover_mul(&m.world.game.units[&id], front).unwrap(), 0.5);
    m.world.game.units.get_mut(&id).unwrap().still = Some(8.);
    close(m.cover_mul(&m.world.game.units[&id], front).unwrap(), 0.3);
    assert!(m.cover_mul(&m.world.game.units[&id], rear).unwrap() > 0.3);
    m.world.game.trench_front.as_mut().unwrap()[c] = f32::NAN;
    close(m.cover_mul(&m.world.game.units[&id], rear).unwrap(), 0.3);
    close(terrain::ground_mul(&m.world.game, &m.rules, c, false), 1.2);
    let order = m.world.game.units.keys().copied().collect::<Vec<_>>();
    let (mut view, sink) = damage(&mut m, &order);
    view.hurt(id, profile(None), 1., 1, sink).unwrap();
    close(m.world.game.units[&id].hp, 90.);

    // Keep only the named observer as a sight source for the enemy side.
    for u in m.world.game.units.values_mut().filter(|u| u.owner == 1) {
        u.hp = 0.;
    }
    let observer = add(&mut m, "rifle", 1, front);
    m.world.game.tick = 100;
    m.grid.rebuild(&m.world.game.units);
    vision::update_vision(&mut m.world.game, &m.rules, &m.grid);
    assert!(!m.world.game.players[1].sees(id));
    m.world.game.units.get_mut(&observer).unwrap().x = 78.;
    m.grid.rebuild(&m.world.game.units);
    vision::update_vision(&mut m.world.game, &m.rules, &m.grid);
    assert!(m.world.game.players[1].sees(id));
    m.world.game.units.get_mut(&observer).unwrap().x = 81.;
    m.world.game.units.get_mut(&id).unwrap().shot_at = Some(100.);
    m.grid.rebuild(&m.world.game.units);
    vision::update_vision(&mut m.world.game, &m.rules, &m.grid);
    assert!(m.world.game.players[1].sees(id));
}

#[test]
fn cmb75_house_wear_vision_blast_flame_collapse_and_attack_exit() {
    let mut m = scene();
    let at = Point { x: 61., z: 41. };
    let id = add(&mut m, "rifle", 0, at);
    let c = m.world.game.cell_of(at.x, at.z).unwrap();
    m.world.game.units.get_mut(&id).unwrap().garrison = Some(c);
    let houses = [
        (250., 0.55, 0.85, 1.25),
        (400., 0.35, 0.7, 1.25),
        (650., 0.25, 0.55, 1.25),
        (700., 0.25, 0.55, 1.6),
        (1000., 0.2, 0.45, 1.4),
        (800., 0.25, 0.5, 1.5),
    ];
    for (kind, (hp, intact, worn, sight)) in houses.into_iter().enumerate() {
        m.world.game.house[c] = kind as u8;
        for (health, expected) in [(hp, intact), (hp * 0.5, (intact + worn) * 0.5), (0., worn)] {
            m.world.game.cell_hp[c] = health as f32;
            close(
                m.cover_mul(&m.world.game.units[&id], Point { x: 81., z: 41. })
                    .unwrap(),
                expected,
            );
        }
        let u = &m.world.game.units[&id];
        let mut outside = u.clone();
        outside.garrison = None;
        close(
            vision::vision_range(&m.world.game, &m.rules, u)
                / vision::vision_range(&m.world.game, &m.rules, &outside),
            sight,
        );
    }
    m.world.game.house[c] = 0;
    m.world.game.cell_hp[c] = 250.;
    let order = m.world.game.units.keys().copied().collect::<Vec<_>>();
    for (anti, hp) in [(None, 90.), (Some(0.8), 84.)] {
        m.world.game.units.get_mut(&id).unwrap().hp = 100.;
        let (mut view, sink) = damage(&mut m, &order);
        view.hurt(id, profile(anti), 1., 1, sink).unwrap();
        close(m.world.game.units[&id].hp, hp);
    }
    let flamer = add(&mut m, "flamer", 1, Point { x: 69., z: 41. });
    let kind = m.world.game.units[&flamer].kind;
    let weapon = WeaponDamage::from_catalog("flamer").unwrap();
    let order = m.world.game.units.keys().copied().collect::<Vec<_>>();
    m.world.game.units.get_mut(&id).unwrap().hp = 100.;
    let (mut view, sink) = damage(&mut m, &order);
    let hit = view
        .direct_hit(
            FlightSource {
                shooter: flamer,
                kind,
                owner: 1,
                airborne: false,
            },
            id,
            Impact {
                owner: 1,
                shooter: flamer,
                weapon,
                damage_scale: None,
                energy: 1.,
                armor_piercing: false,
                ap_naval_only: false,
                ap_multiplier: 1.,
                launch_origin: Point { x: 69., z: 41. },
                at,
                direction: Point { x: -1., z: 0. },
            },
            sink,
        )
        .unwrap();
    close(hit, weapon.infantry * 1.5);

    m.world.game.units.get_mut(&id).unwrap().hp = 100.;
    let skip = BTreeSet::new();
    let (mut view, _) = damage(&mut m, &order);
    view.terrain_blast(
        Blast {
            at,
            radius: 0.,
            damage: 100_000.,
            scar: 0.,
        },
        &mut DamageContext::plain(&skip),
    )
    .unwrap();
    assert!(m.world.game.units[&id].garrison.is_none());
    close(m.world.game.units[&id].hp, 70.);

    // An explicit attack retains a usable house, and exits when the weapon cannot answer.
    let mut m = scene();
    let id = add(&mut m, "rifle", 0, at);
    m.world.game.units.get_mut(&id).unwrap().garrison = Some(c);
    let enemy = add(&mut m, "rifle", 1, Point { x: 75., z: 41. });
    m.world.game.players[0].visible.push(enemy);
    assert_eq!(
        m.ground_attack_command(0, &[i64::from(id)], i64::from(enemy), false)
            .unwrap(),
        None
    );
    assert_eq!(m.world.game.units[&id].garrison, Some(c));
    m.world.game.units.get_mut(&enemy).unwrap().x = 115.;
    assert_eq!(
        m.ground_attack_command(0, &[i64::from(id)], i64::from(enemy), false)
            .unwrap(),
        None
    );
    assert!(m.world.game.units[&id].garrison.is_none());
    assert_eq!(m.commands.units[&id].attack_id, enemy);
}

#[test]
fn cmb78_ground_retreat_cleans_orders_protects_and_ends_at_home() {
    let mut m = scene();
    let id = add(&mut m, "rifle", 0, Point { x: 61., z: 45. });
    // Keep the arrival pose stable when retreat ends and idle reactions resume.
    // Hold Position still permits the explicit retreat and its movement.
    m.world.game.units.get_mut(&id).unwrap().hold_pos = true;
    let shooter = add(&mut m, "rifle", 1, Point { x: 71., z: 45. });
    m.start.points[0].at = Point { x: 61., z: 45. };
    let c = m.commands.units.get_mut(&id).unwrap();
    c.orders.push_back(Command::Stop {
        ids: vec![i64::from(id)],
    });
    c.attack_id = shooter;
    c.nade = Some(Point { x: 70., z: 45. });
    c.enter = Some(1);
    c.fire_at = Some(1);
    m.firing.initialize(id).target_id = shooter;
    m.retreat_unit(id).unwrap();
    let planned_end = m.commands.units[&id].route_end.unwrap();
    let home = m.reaction_home(id).unwrap();
    let hq_half_extent = Catalog::compiled()
        .unwrap()
        .unit("hq")
        .unwrap()
        .get("size")
        .and_then(|v| v.number())
        .unwrap()
        * crate::game::CELL
        / 2.;
    // The HQ is solid. Its route ends in the first free cell beside its footprint.
    close(
        crate::math::hypot(planned_end.x - home.x, planned_end.z - home.z),
        hq_half_extent + crate::game::CELL / 2.,
    );
    let u = &m.world.game.units[&id];
    assert!(u.retreating && !u.path.is_empty());
    assert_eq!(u.world_goal, Some(m.reaction_home(id).unwrap()));
    let c = &m.commands.units[&id];
    assert!(c.orders.is_empty() && c.nade.is_none() && c.enter.is_none() && c.fire_at.is_none());
    assert_eq!(c.attack_id, 0);
    m.apply_unit_firing(id, false, 0.05).unwrap();
    assert_eq!(m.firing.get(id).unwrap().target_id, 0);
    assert!(m.firing.flights.is_empty());
    let order = m.world.game.units.keys().copied().collect::<Vec<_>>();
    let (mut view, sink) = damage(&mut m, &order);
    view.hurt(id, profile(None), 1., 1, sink).unwrap();
    close(m.world.game.units[&id].hp, 95.);
    let (mut view, sink) = damage(&mut m, &order);
    view.suppress(
        id,
        SuppressionInput {
            owner: 1,
            shooter,
            amount: 30.,
            scale: None,
            flame: false,
            cover_mul: 1.,
            cover_behind: 0.,
            launch_origin: Point { x: 71., z: 45. },
            already_suppressed: false,
        },
        sink,
        &mut BTreeSet::new(),
    )
    .unwrap();
    close(m.world.game.units[&id].supp, 0.);
    let speed = movement::move_unit(&mut m.world.game, &m.rules, &mut m.grid, id, 1.)
        .unwrap()
        .speed;
    close(
        speed,
        m.rules.kinds[m.world.game.units[&id].kind].speed * 1.5,
    );
    m.world.game.units.get_mut(&shooter).unwrap().x = 105.;
    m.world.game.units.get_mut(&shooter).unwrap().z = 65.;
    m.advance_tick().unwrap();
    close(m.start.points[0].progress, 0.);
    assert!(!m.start.points[0].on_point.contains(&id));
    for _ in 0..1000 {
        if !m.world.game.units[&id].retreating {
            break;
        }
        m.advance_tick().unwrap();
    }
    assert!(!m.world.game.units[&id].retreating);
    let u = &m.world.game.units[&id];
    assert_eq!(m.commands.units[&id].route_end, Some(planned_end));
    assert_eq!(
        m.commands.units[&id].outcome,
        Some(crate::match_commands::MoveOutcome::Arrived)
    );
    assert!(u.world_goal.is_none() && u.path.is_empty());
    assert!(
        crate::math::hypot(u.x - planned_end.x, u.z - planned_end.z) <= 0.25,
        "arrival {:?}, planned end {planned_end:?}, home {home:?}",
        u.at()
    );
}

#[test]
fn cmb78_forced_withdrawal_protection_requires_moving_and_not_stranded() {
    let mut m = scene();
    let id = add(&mut m, "rifle", 0, Point { x: 61., z: 45. });
    m.retreat_unit(id).unwrap();
    m.logistics.insert(
        id,
        crate::logistics::Reserves {
            ammo: 20.,
            ammo_max: 20.,
            ammo_price: 1.,
            ammo_credit: Some(0.),
            provisions: 50.,
            fuel: None,
            emergency: None,
            warning: None,
            forced: true,
            stranded: false,
            exhausted: false,
            supply_rate: None,
            cut_for: None,
        },
    );
    let order = m.world.game.units.keys().copied().collect::<Vec<_>>();
    for (speed, stranded, hp) in [(0., false, 80.), (4., false, 95.), (4., true, 80.)] {
        let u = m.world.game.units.get_mut(&id).unwrap();
        u.hp = 100.;
        u.move_speed = speed;
        u.vx = Some(0.);
        u.vz = Some(0.);
        m.logistics.get_mut(id).unwrap().stranded = stranded;
        let (mut view, sink) = damage(&mut m, &order);
        view.hurt(id, profile(None), 1., 1, sink).unwrap();
        close(m.world.game.units[&id].hp, hp);
    }
}

#[test]
fn cmb79_auto_retreat_threshold_distance_switch_and_garrison_tick_exit() {
    let mut m = scene();
    let at = Point { x: 61., z: 41. };
    let id = add(&mut m, "rifle", 0, at);
    m.commands.units.get_mut(&id).unwrap().auto_retreat = true;
    m.world.game.units.get_mut(&id).unwrap().hp = 34.;
    m.world.game.units.get_mut(&id).unwrap().riding = true;
    assert!(!m.automatic_retreat_candidate(id).unwrap());
    m.world.game.units.get_mut(&id).unwrap().riding = false;
    let mut convoys = crate::supply_convoys::ConvoyState::default();
    convoys.trucks.insert(
        id,
        crate::supply_convoys::Truck {
            state: crate::supply_convoys::TruckState::Idle,
            cargo: Default::default(),
            target: None,
            origin: crate::supply_convoys::StoreId::Source(0),
            timer: 0.,
            manual: false,
            hold: false,
            route: vec![],
            destination: None,
            route_failure: None,
            retry: None,
        },
    );
    m.convoys = Some(convoys);
    assert!(!m.automatic_retreat_candidate(id).unwrap());
    m.convoys = None;
    let immobile = m
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 0 && m.rules.kinds[u.kind].name == "hq")
        .unwrap()
        .id;
    let hp = m.world.game.units[&immobile].hp;
    m.world.game.units.get_mut(&immobile).unwrap().hp = 1.;
    m.commands.units.get_mut(&immobile).unwrap().auto_retreat = true;
    assert!(!m.automatic_retreat_candidate(immobile).unwrap());
    m.world.game.units.get_mut(&immobile).unwrap().hp = hp;
    m.commands.units.get_mut(&immobile).unwrap().auto_retreat = false;
    m.world.game.units.get_mut(&id).unwrap().hp = 35.;
    assert!(!m.automatic_retreat_candidate(id).unwrap());
    m.world.game.units.get_mut(&id).unwrap().hp = 34.;
    assert!(m.automatic_retreat_candidate(id).unwrap());
    m.commands.units.get_mut(&id).unwrap().auto_retreat = false;
    assert!(!m.automatic_retreat_candidate(id).unwrap());
    m.commands.units.get_mut(&id).unwrap().auto_retreat = true;
    let home = m.reaction_home(id).unwrap();
    for (distance, expected) in [(15., false), (15.01, true)] {
        m.world.game.units.get_mut(&id).unwrap().x = home.x + distance;
        m.world.game.units.get_mut(&id).unwrap().z = home.z;
        assert_eq!(m.automatic_retreat_candidate(id).unwrap(), expected);
    }
    let cell = m.world.game.cell_of(at.x, at.z).unwrap();
    let u = m.world.game.units.get_mut(&id).unwrap();
    u.x = at.x;
    u.z = at.z;
    u.garrison = Some(cell);
    m.grid.rebuild(&m.world.game.units);
    m.advance_tick().unwrap();
    let u = &m.world.game.units[&id];
    assert!(u.garrison.is_none() && u.retreating && !u.path.is_empty());
    assert!(m.tick_state.fault().is_none());
}

#[test]
fn cmb80_home_recovery_pays_for_models_repairs_and_obeys_provisions() {
    let mut m = scene();
    let home = m.start.players[0].spawn;
    let id = add(
        &mut m,
        "rifle",
        0,
        Point {
            x: home.x + 8.,
            z: home.z,
        },
    );
    let tank = add(
        &mut m,
        "tank",
        0,
        Point {
            x: home.x + 8.,
            z: home.z + 4.,
        },
    );
    let hq = m
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 0 && m.rules.kinds[u.kind].name == "hq")
        .unwrap()
        .id;
    let structure_hp = m.world.game.units[&hq].hp - 20.;
    m.world.game.units.get_mut(&hq).unwrap().hp = structure_hp;
    let plane = add(
        &mut m,
        "fighter",
        0,
        Point {
            x: home.x + 9.,
            z: home.z + 4.,
        },
    );
    m.world.game.units.get_mut(&plane).unwrap().hp = 10.;
    m.world.game.units.get_mut(&id).unwrap().hp = 40.;
    m.world.game.units.get_mut(&tank).unwrap().hp = 200.;
    m.start.economy.players[0].mp = 1000.;
    let order = m.world.game.units.keys().copied().collect::<Vec<_>>();
    for _ in 0..100 {
        let frame = m.prepare_recovery(&order).unwrap();
        m.reinforce(&frame, 0.05).unwrap();
        m.world.game.tick += 1;
    }
    close(m.world.game.units[&id].hp, 80.);
    close(m.world.game.units[&tank].hp, 290.);
    close(m.start.economy.players[0].mp, 935.);
    close(m.world.game.units[&hq].hp, structure_hp);
    close(m.world.game.units[&plane].hp, 10.);
    let frame = m.prepare_recovery(&order).unwrap();
    m.start.economy.players[0].mp = 0.;
    m.reinforce(&frame, 2.).unwrap();
    close(m.world.game.units[&id].hp, 80.);
    close(m.world.game.units[&tank].hp, 290.);

    m.logistics.insert(
        id,
        crate::logistics::Reserves {
            ammo: 20.,
            ammo_max: 20.,
            ammo_price: 1.,
            ammo_credit: Some(0.),
            provisions: 10.,
            fuel: None,
            emergency: None,
            warning: None,
            forced: false,
            stranded: false,
            exhausted: false,
            supply_rate: None,
            cut_for: None,
        },
    );
    m.options.logistics = true;
    m.convoys = Some(Default::default());
    m.supply_net = crate::supply_network::GroundSupplyNetwork {
        width: m.world.game.w,
        height: m.world.game.h,
        teams: vec![],
    };
    m.start.economy.players[0].mp = 1000.;
    m.recovery.get_mut(id).unwrap().reinforcement = 0.;
    m.logistics.get_mut(id).unwrap().provisions = 10.;
    m.reinforce(&frame, 2.).unwrap();
    close(m.world.game.units[&id].hp, 80.);
    close(m.logistics.get(id).unwrap().provisions, 10.);
    m.reinforce(&frame, 2.).unwrap();
    close(m.world.game.units[&id].hp, 100.);
    close(m.logistics.get(id).unwrap().provisions, 5.);
    m.world.game.units.get_mut(&id).unwrap().hp = 80.;
    m.logistics.get_mut(id).unwrap().provisions = 0.;
    m.reinforce(&frame, 10.).unwrap();
    close(m.world.game.units[&id].hp, 80.);
}

#[test]
fn cmb80_home_recovery_requires_own_finished_base_or_held_uncut_depot() {
    let mut m = scene();
    let at = Point { x: 77., z: 45. };
    let id = add(&mut m, "rifle", 0, at);
    let base = add(&mut m, "hq", 1, Point { x: 77., z: 49. });
    m.world.game.units.get_mut(&id).unwrap().hp = 40.;
    m.start.economy.players[0].mp = 1000.;
    let order = m.world.game.units.keys().copied().collect::<Vec<_>>();
    let frame = m.prepare_recovery(&order).unwrap();
    m.reinforce(&frame, 2.).unwrap();
    close(m.world.game.units[&id].hp, 40.);
    m.world.game.units.get_mut(&base).unwrap().owner = 0;
    m.world.game.units.get_mut(&base).unwrap().built = Some(0.99);
    let frame = m.prepare_recovery(&order).unwrap();
    m.reinforce(&frame, 2.).unwrap();
    close(m.world.game.units[&id].hp, 40.);
    m.world.game.units.get_mut(&base).unwrap().built = Some(1.);
    let frame = m.prepare_recovery(&order).unwrap();
    m.reinforce(&frame, 2.).unwrap();
    close(m.world.game.units[&id].hp, 60.);
    m.world.game.units.get_mut(&base).unwrap().hp = 0.;
    let depot = &mut m.start.points[0];
    depot.at = at;
    depot.owner = Some(0);
    depot.kind = Some("depot".into());
    depot.cut = true;
    let frame = m.prepare_recovery(&order).unwrap();
    m.reinforce(&frame, 2.).unwrap();
    close(m.world.game.units[&id].hp, 60.);
    m.start.points[0].cut = false;
    let frame = m.prepare_recovery(&order).unwrap();
    m.reinforce(&frame, 2.).unwrap();
    close(m.world.game.units[&id].hp, 80.);
}

#[test]
fn cmb78_cmb79_world_auto_retreat_uses_nearest_owned_production_and_home_radius() {
    let mut m = world_scene();
    let hq = m
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 0 && m.rules.kinds[u.kind].name == "hq")
        .unwrap()
        .at();
    let nearer = Point {
        x: hq.x + 24.,
        z: hq.z,
    };
    let base = add(&mut m, "worldbase", 0, nearer);
    let id = add(
        &mut m,
        "rifle",
        0,
        Point {
            x: hq.x + 50.,
            z: hq.z,
        },
    );
    m.world.game.units.get_mut(&id).unwrap().hp = 34.;
    m.commands.units.get_mut(&id).unwrap().auto_retreat = true;
    assert_eq!(m.reaction_home(id).unwrap(), nearer);
    m.world.game.units.get_mut(&base).unwrap().built = Some(0.99);
    assert_eq!(m.reaction_home(id).unwrap(), hq);
    m.world.game.units.get_mut(&base).unwrap().built = Some(1.);
    assert!(m.automatic_retreat_unit(id).unwrap());
    assert!(m.world.game.units[&id].retreating);
    assert_eq!(m.world.game.units[&id].world_goal, Some(nearer));
    assert!(!m.world.game.units[&id].path.is_empty());
    let u = m.world.game.units.get_mut(&id).unwrap();
    u.x = nearer.x + 14.;
    u.z = nearer.z;
    m.prepare_unit_commands(id, false).unwrap();
    assert!(!m.world.game.units[&id].retreating);
    assert!(m.world.game.units[&id].world_goal.is_none());
}

#[test]
fn cmb80_world_allied_production_recovers_commanders_without_funding_local_defenders() {
    let mut m = world_scene();
    let base = m
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 1 && m.rules.kinds[u.kind].name == "hq")
        .unwrap()
        .at();
    let id = add(&mut m, "rifle", 0, base);
    let local = add(&mut m, "rifle", crate::game::LOCAL_DEFENDER_OWNER, base);
    let tank = add(&mut m, "tank", crate::game::LOCAL_DEFENDER_OWNER, base);
    m.world.game.units.get_mut(&id).unwrap().hp = 40.;
    m.world.game.units.get_mut(&local).unwrap().hp = 40.;
    m.world.game.units.get_mut(&tank).unwrap().hp = 100.;
    m.start.economy.players[0].mp = 1000.;
    // The local defender needs its own friendly production base to be eligible.
    add(&mut m, "worldbase", crate::game::LOCAL_DEFENDER_OWNER, base);
    let order = m.world.game.units.keys().copied().collect::<Vec<_>>();
    let frame = m.prepare_recovery(&order).unwrap();
    m.reinforce(&frame, 2.).unwrap();
    close(m.world.game.units[&id].hp, 60.);
    close(m.start.economy.players[0].mp, 990.);
    close(m.world.game.units[&local].hp, 40.);
    close(m.world.game.units[&tank].hp, 100.);
    assert_eq!(
        m.actor_policy(crate::game::LOCAL_DEFENDER_OWNER)
            .unwrap()
            .economy_slot,
        None
    );
    assert_eq!(m.start.economy.players.len(), 2);
}
