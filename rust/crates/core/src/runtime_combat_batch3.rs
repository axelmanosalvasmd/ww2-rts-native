//! Native feature tests for the owned combat window. No Source capture is used.
use super::*;
use crate::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    map::AuthoritativeMap,
    match_commands::UnitControl,
    match_runtime::{MatchOptions, PlayerConfig},
    random::StreamId,
    unit_spawn,
    weather::WeatherChoice,
    world::World,
};

fn arena() -> AuthoritativeMap {
    let map = serde_json::json!({
        "name": "Native combat flight arena", "w": 64, "h": 64,
        "rows": vec![".".repeat(64); 64], "heights": vec!["0".repeat(64); 64],
        "spawns": [{"x": 4, "y": 4}, {"x": 55, "y": 55}],
        "points": [{"x": 32, "y": 32}]
    });
    AuthoritativeMap::from_json(&serde_json::to_vec(&map).unwrap()).unwrap()
}
fn ordinary() -> Match {
    Match::new(
        &arena(),
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
    .unwrap()
}
fn scene(units: &[(&str, usize, f64, f64)]) -> Match {
    let mut m = ordinary();
    let players = m.world.game.players.clone();
    m.world = World::from_map(&arena()).unwrap();
    m.world.game.players = players;
    m.world.game.tick = 100;
    m.world.game.rain = 0.0;
    m.world.game.wet = Some(0.0);
    m.building_cells.clear();
    m.commands.units.clear();
    m.firing = Default::default();
    m.combat = Default::default();
    m.abilities = Default::default();
    m.recovery = Default::default();
    for (index, &(name, owner, x, z)) in units.iter().enumerate() {
        let id = index as u32 + 1;
        let kind = m.rules.kind(name).unwrap();
        let mut u = unit_spawn::initial_unit(
            &m.rules,
            Catalog::compiled().unwrap(),
            kind,
            id,
            owner,
            Point { x, z },
        )
        .unwrap();
        u.still = Some(10.0);
        if m.rules.kinds[kind].building {
            u.built = Some(1.0);
        }
        m.commands
            .insert(id, UnitControl::new(&u, &m.rules).unwrap());
        m.firing.initialize(id);
        m.combat.initialize(id);
        m.abilities.initialize(id);
        m.recovery.initialize(id);
        m.world.game.units.insert(id, u);
    }
    m.next_unit_id = 1000;
    m.grid.rebuild(&m.world.game.units);
    for owner in 0..2 {
        m.world
            .register_recipient(crate::world::Recipient::Player(owner));
    }
    m.initialize_recipient_routing().unwrap();
    sight(&mut m);
    m
}
fn sight(m: &mut Match) {
    m.grid.rebuild(&m.world.game.units);
    vision::update_vision(&mut m.world.game, &m.rules, &m.grid);
}
fn close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-9, "{actual} != {expected}");
}
fn launch(m: &mut Match, shooter: u32, target: u32) {
    m.commands.units.get_mut(&shooter).unwrap().attack_id = target;
    m.set_firing_target(shooter, target).unwrap();
    m.apply_unit_firing(shooter, false, 0.05).unwrap();
}
fn flights(m: &mut Match, ticks: usize) {
    for _ in 0..ticks {
        m.capture_firing_prior().unwrap();
        m.world.game.tick += 1;
        m.step_firing_flights().unwrap();
    }
}

#[test]
fn cmb43_paid_birth_has_full_health_clean_combat_and_ground_autoretreat() {
    let mut m = ordinary();
    m.start.economy.players[0].mp = 1000.0;
    let born = m.recorded_buy(0, "rifle", None).unwrap();
    let id = born.unit;
    let u = &m.world.game.units[&id];
    close(u.hp, 100.0);
    close(u.supp, 0.0);
    close(m.combat.get(id).unwrap().xp, 0.0);
    assert_eq!(m.firing.get(id).unwrap().target_id, 0);
    assert!(m.commands.units[&id].auto_retreat);
    assert!(!m.commands.units[&id].hold_fire && !u.hold_pos);
    assert!(m.commands.units[&id].face.is_none());
    assert!(m.commands.units[&id].auto_cast);
}

#[test]
fn cmb44_can_shoot_rejects_dead_allied_hidden_riding_air_and_outside_range() {
    let mut m = scene(&[("rifle", 0, 20.0, 20.0), ("rifle", 1, 30.0, 20.0)]);
    assert!(m.can_shoot_target(1, 2).unwrap());
    let original = m.world.game.units[&2].clone();
    for condition in 0..5 {
        *m.world.game.units.get_mut(&2).unwrap() = original.clone();
        let u = m.world.game.units.get_mut(&2).unwrap();
        match condition {
            0 => u.hp = 0.0,
            1 => u.owner = 0,
            2 => u.riding = true,
            3 => u.air = Some(crate::game::PlaneState::Station),
            _ => u.x = 100.0,
        }
        assert!(!m.can_shoot_target(1, 2).unwrap(), "condition {condition}");
    }
    *m.world.game.units.get_mut(&2).unwrap() = original;
    m.world.game.players[0].visible.clear();
    assert!(!m.can_shoot_target(1, 2).unwrap());
}

#[test]
fn cmb44_salvo_minimum_range_skips_walls_but_direct_fire_needs_sight() {
    let mut m = scene(&[("mortar", 0, 20.0, 20.0), ("rifle", 1, 40.0, 20.0)]);
    let wall = m.world.game.cell_of(30.0, 20.0).unwrap();
    m.world.set_cell(wall, b'B').unwrap();
    assert!(m.can_shoot_target(1, 2).unwrap());
    m.world.game.units.get_mut(&1).unwrap().kind = m.rules.kind("sniper").unwrap();
    assert!(!m.can_shoot_target(1, 2).unwrap());
    m.world.game.units.get_mut(&1).unwrap().kind = m.rules.kind("mortar").unwrap();
    m.world.game.units.get_mut(&2).unwrap().x = 31.9;
    assert!(!m.can_shoot_target(1, 2).unwrap());
    m.world.game.units.get_mut(&2).unwrap().x = 32.0;
    assert!(m.can_shoot_target(1, 2).unwrap());
    reserves(&mut m, 1, "mortar");
    m.logistics.get_mut(1).unwrap().ammo = 0.0;
    assert!(!m.can_shoot_target(1, 2).unwrap());
}

#[test]
fn cmb45_nearest_live_section_and_ship_bow_are_aimed_at() {
    let mut m = scene(&[
        ("tank", 0, 20.0, 30.0),
        ("pillbox", 1, 40.0, 30.0),
        ("destroyer", 1, 80.0, 30.0),
    ]);
    let width = m.world.game.w as u32;
    m.world.game.units.get_mut(&2).unwrap().cells = Some(vec![15 * width + 19, 15 * width + 20]);
    let point = m.aim_point(&m.world.game.units[&1], &m.world.game.units[&2]);
    assert_eq!(point, Point { x: 39.0, z: 31.0 });
    let ship = &m.world.game.units[&3];
    let bow = m.aim_point(&m.world.game.units[&1], ship);
    assert!(bow.x < ship.x && bow.z == ship.z);
    close(ship.x - bow.x, m.rules.kinds[ship.kind].hull);
}

#[test]
fn cmb46_setup_movefire_suppression_buff_and_area_order_gate_volley() {
    let mut mg = scene(&[("mg", 0, 20.0, 20.0), ("rifle", 1, 30.0, 20.0)]);
    mg.world.game.units.get_mut(&1).unwrap().still = Some(0.0);
    launch(&mut mg, 1, 2);
    assert!(mg.firing.flights.is_empty());
    mg.world.game.units.get_mut(&1).unwrap().still = Some(10.0);
    mg.apply_unit_firing(1, true, 0.05).unwrap();
    assert!(mg.firing.flights.is_empty());
    for (supp, buff, factor) in [
        (0.0, 0.0, 1.0),
        (50.0, 0.0, 1.5),
        (90.0, 0.0, 3.0),
        (50.0, 1.0, 0.75),
    ] {
        let mut m = scene(&[("mg", 0, 20.0, 20.0), ("rifle", 1, 30.0, 20.0)]);
        m.world.game.units.get_mut(&1).unwrap().supp = supp;
        m.abilities.get_mut(1).unwrap().buff = buff;
        launch(&mut m, 1, 2);
        assert!(!m.firing.flights.is_empty());
        close(m.firing.get(1).unwrap().cooldown, 0.3 * factor);
    }
    let mut rocket = scene(&[("rocket", 0, 20.0, 20.0), ("rifle", 1, 40.0, 20.0)]);
    rocket.apply_unit_firing(1, false, 0.05).unwrap();
    assert!(rocket.firing.flights.is_empty());
    launch(&mut rocket, 1, 2);
    assert!(!rocket.firing.flights.is_empty());
}

#[test]
fn cmb47_living_models_launch_rounds_with_distinct_ids_and_scaled_suppression() {
    let mut m = scene(&[("rifle", 0, 20.0, 20.0), ("rifle", 1, 30.0, 20.0)]);
    m.world.game.units.get_mut(&1).unwrap().hp = 40.0;
    launch(&mut m, 1, 2);
    assert_eq!(m.firing.flights.len(), 2);
    let group = m.firing.flights.values().next().unwrap().volley_group;
    for (id, f) in &m.firing.flights {
        assert_eq!(*id, f.id);
        assert_eq!(f.volley_group, group);
        close(f.suppression_scale, 0.4);
        assert_eq!(f.intended, Some(2));
    }
}

#[test]
fn cmb48_round_launch_has_profile_height_lifetime_and_no_early_damage() {
    for (name, speed, gravity) in [
        ("rifle", 230.0, 0.0),
        ("sniper", 420.0, 0.0),
        ("at", 150.0, 9.81),
        ("flamer", 40.0, 0.0),
    ] {
        let distance = if speed > 200.0 { 20.0 } else { 5.0 };
        let mut m = scene(&[(name, 0, 20.0, 20.0), ("rifle", 1, 20.0 + distance, 20.0)]);
        let hp = m.world.game.units[&2].hp;
        m.fire_weapon(1, 2, false).unwrap();
        let f = m.firing.flights.values().next().unwrap();
        close(f.launch_origin.y, 1.2);
        close(f.solution.gravity, gravity);
        assert!(math::hypot(f.solution.velocity.x, f.solution.velocity.z) <= speed);
        assert!(math::hypot(f.solution.velocity.x, f.solution.velocity.z) > speed * 0.9);
        assert!(f.expires > f.launched);
        close(m.world.game.units[&2].hp, hp);
    }
}

#[test]
fn cmb49_intervening_enemy_intercepts_but_ally_is_transparent() {
    for allied in [false, true] {
        let mut m = scene(&[
            ("sniper", 0, 20.0, 20.0),
            ("rifle", 1, 40.0, 20.0),
            ("rifle", usize::from(!allied), 30.0, 20.0),
        ]);
        // A physical round with an explicit aim is a component input. It still
        // runs the real sweep, contact, damage and event path.
        m.abilities.get_mut(1).unwrap().ap = true;
        m.fire_weapon(1, 2, false).unwrap();
        let f = m.firing.flights.values_mut().next().unwrap();
        f.solution.velocity = projectiles::Point3::new(420.0, 0.0, 0.0);
        f.solution.origin.y = 1.2;
        let before = m.world.game.units[&3].hp;
        flights(&mut m, 2);
        if allied {
            close(m.world.game.units[&3].hp, before);
        } else {
            assert!(m.world.game.units[&3].hp < before);
        }
        let contacts = m
            .firing
            .events
            .iter()
            .filter_map(|e| {
                if let LaunchEvent::Contact(c) = e {
                    c.target
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(contacts.first().copied(), Some(if allied { 2 } else { 3 }));
    }
}

#[test]
fn cmb57_salvo_scatter_staggers_physical_shells_and_cooldown() {
    let mut m = scene(&[("rocket", 0, 20.0, 20.0), ("rifle", 1, 40.0, 20.0)]);
    launch(&mut m, 1, 2);
    assert_eq!(m.firing.flights.len(), 8);
    let first = m.firing.flights.values().next().unwrap().launched;
    for (i, f) in m.firing.flights.values().enumerate() {
        close(f.launched - first, i as f64 * 0.15);
        assert_eq!(f.blast_radius, Some(3.5));
        close(f.solution.gravity, 25.0);
    }
    assert_eq!(m.firing.salvos[0].left, 8);
}

#[test]
fn cmb77_sniper_camouflage_requires_stillness_quiet_and_distance() {
    let mut m = scene(&[("rifle", 0, 20.0, 20.0), ("sniper", 1, 40.0, 20.0)]);
    assert!(!m.world.game.players[0].sees(2));
    m.world.game.units.get_mut(&2).unwrap().shot_at = Some(100.0);
    sight(&mut m);
    assert!(m.world.game.players[0].sees(2));
    m.world.game.units.get_mut(&2).unwrap().shot_at = None;
    m.world.game.units.get_mut(&2).unwrap().still = Some(0.0);
    sight(&mut m);
    assert!(m.world.game.players[0].sees(2));
    m.world.game.units.get_mut(&2).unwrap().still = Some(3.0);
    m.world.game.units.get_mut(&2).unwrap().x = 31.0;
    sight(&mut m);
    assert!(m.world.game.players[0].sees(2));
}

fn reserves(m: &mut Match, id: u32, name: &str) {
    m.logistics
        .initialize_unit(
            crate::logistics::Context {
                enabled_override: Some(true),
                scenario: false,
                mode: crate::logistics::Mode::Conquest,
            },
            crate::logistics::UnitFacts {
                id,
                kind: name,
                owner: Some(m.world.game.units[&id].owner),
                owner_exists: true,
                guard_home: false,
                riding: false,
            },
            Some(&crate::logistics::Definition::from_catalog(name).unwrap()),
        )
        .unwrap();
}

#[test]
fn cmb43_mode_costs_and_air_defaults_are_published_cleanly() {
    let rules = ordinary().rules;
    for mode in [Mode::Default, Mode::Conquest, Mode::Assault, Mode::World] {
        for name in [
            "rifle", "mg", "rocket", "sniper", "fighter", "attacker", "bomber",
        ] {
            let kind = rules.kind(name).unwrap();
            let catalog = Catalog::compiled().unwrap();
            let at = Point { x: 20.0, z: 20.0 };
            let u = if rules.kinds[kind].air {
                unit_spawn::initial_aircraft(&rules, catalog, kind, 1, 0, at)
            } else {
                unit_spawn::initial_unit(&rules, catalog, kind, 1, 0, at)
            }
            .unwrap();
            let c = UnitControl::new_for_mode(&u, &rules, mode).unwrap();
            close(u.supp, 0.0);
            let definition = catalog.unit(name).unwrap();
            close(
                u.hp,
                n(definition, "hpPer", 0.0) * n(definition, "models", 0.0),
            );
            assert_eq!(c.auto_retreat, !rules.kinds[kind].air);
            assert!(!c.hold_fire && !u.hold_pos && c.face.is_none());
            let expected_auto = match name {
                "rifle" => !matches!(mode, Mode::World),
                "mg" => mode != Mode::World,
                "rocket" => true,
                _ => false,
            };
            assert_eq!(c.auto_cast, expected_auto, "{mode:?} {name}");
        }
    }
}

#[test]
fn cmb45_failed_sections_are_skipped_and_naval_hulls_extend_search() {
    let mut m = scene(&[("tank", 0, 20.0, 30.0), ("pillbox", 1, 40.0, 30.0)]);
    let near = 15 * m.world.game.w as u32 + 19;
    let far = near + 1;
    m.world.game.units.get_mut(&2).unwrap().cells = Some(vec![near, far]);
    for (failed, hp) in [(true, 100.0), (false, 0.0)] {
        m.world.game.sections.insert(
            near,
            crate::game::Section {
                failed,
                hp,
                max_hp: Some(100.0),
            },
        );
        assert_eq!(
            m.aim_point(&m.world.game.units[&1], &m.world.game.units[&2]),
            Point { x: 41.0, z: 31.0 }
        );
    }
    let mut naval = scene(&[
        ("tank", 0, 20.0, 30.0),
        ("destroyer", 1, 80.0, 30.0),
        ("rifle", 0, 80.0, 40.0),
    ]);
    naval.world.game.naval = true;
    assert!(naval.can_shoot_target(1, 2).unwrap());
    assert_eq!(naval.pick_firing_target(1).unwrap(), 2);
    naval.world.game.naval = false;
    assert_eq!(naval.pick_firing_target(1).unwrap(), 0);
}

#[test]
fn cmb46_shortage_slows_reload_and_salvos_ignore_suppression() {
    for name in ["mg", "rocket"] {
        for (low, supp, expected) in [
            (false, 0.0, 1.0),
            (true, 0.0, 2.0),
            (true, 90.0, if name == "mg" { 6.0 } else { 2.0 }),
        ] {
            let mut m = scene(&[(name, 0, 20.0, 20.0), ("rifle", 1, 40.0, 20.0)]);
            reserves(&mut m, 1, name);
            if low {
                m.logistics.get_mut(1).unwrap().ammo = 2.0;
            }
            m.world.game.units.get_mut(&1).unwrap().supp = supp;
            launch(&mut m, 1, 2);
            assert!(!m.firing.flights.is_empty());
            close(
                m.firing.get(1).unwrap().cooldown,
                if name == "mg" {
                    0.3 * expected
                } else {
                    20.0 * expected
                },
            );
        }
    }
}

fn first_draw_seed(low: f64, high: f64) -> u64 {
    (0..100_000)
        .find(|seed| {
            let mut rng = crate::random::MatchRandom::new(*seed);
            let v = rng.next_fraction(StreamId::Combat);
            v > low && v < high
        })
        .unwrap()
}

fn round_endpoint(m: &Match) -> projectiles::Point3 {
    let p = m.firing.flights.values().next().unwrap();
    let target_x = m.world.game.units[&2].x;
    let duration = (target_x - p.solution.origin.x) / p.solution.velocity.x;
    projectiles::flight_at(p.solution, p.launched + duration).unwrap()
}

#[test]
fn cmb47_accuracy_modifiers_select_dispersion_and_veterancy_improves_hits() {
    // A single living model makes each comparison use the same first combat roll.
    for (supp, moving, trench, level, xp, accuracy) in [
        (0.0, false, false, 0, 0.0, 0.7),
        (50.0, false, false, 0, 0.0, 0.49),
        (90.0, false, false, 0, 0.0, 0.35),
        (0.0, true, false, 0, 0.0, 0.35),
        (0.0, false, true, 0, 0.0, 0.21),
        (0.0, false, false, 1, 0.0, 0.805),
        (0.0, false, false, 4, 0.0, 1.015),
        (0.0, false, false, -2, 0.0, 0.49),
        (0.0, false, false, 0, 100.0, 0.77),
        (50.0, true, true, 1, 500.0, 0.1098825),
    ] {
        for accurate in [false, true] {
            if !accurate && accuracy >= 1.0 {
                continue;
            }
            let mut m = scene(&[("rifle", 0, 20.0, 20.0), ("rifle", 1, 30.0, 20.0)]);
            let u = m.world.game.units.get_mut(&1).unwrap();
            u.hp = 20.0;
            u.supp = supp;
            m.combat.get_mut(1).unwrap().xp = xp;
            let cell = m.world.game.cell_of(20.0, 20.0).unwrap();
            m.world
                .game
                .height
                .get_or_insert_with(|| vec![0; m.world.game.w * m.world.game.h])[cell] = level;
            if trench {
                let cell = m.world.game.cell_of(30.0, 20.0).unwrap();
                m.world.set_cell(cell, b'T').unwrap();
            }
            let seed = if accurate {
                first_draw_seed(accuracy * 0.8, accuracy.min(1.0))
            } else {
                first_draw_seed(accuracy, (accuracy + 0.1).min(1.0))
            };
            m.start.random = crate::random::MatchRandom::new(seed);
            m.fire_weapon(1, 2, moving).unwrap();
            let end = round_endpoint(&m);
            if accurate {
                close(end.z, 20.0);
                close(end.y, 0.8);
            } else {
                close(end.y, 0.08);
                assert!((1.625..=2.325).contains(&(end.z - 20.0).abs()));
            }
        }
    }
}

#[test]
fn cmb48_garrison_height_travel_and_expiry_are_physical() {
    let mut m = scene(&[("sniper", 0, 20.0, 20.0), ("rifle", 1, 40.0, 20.0)]);
    m.world.game.units.get_mut(&1).unwrap().garrison = m.world.game.cell_of(20.0, 20.0);
    m.fire_weapon(1, 2, false).unwrap();
    close(
        m.firing.flights.values().next().unwrap().launch_origin.y,
        3.5,
    );
    let p = m.firing.flights.values_mut().next().unwrap();
    p.solution.origin.y = 50.0;
    p.solution.velocity.y = 0.0;
    p.solution.velocity.z = 0.0;
    let id = p.id;
    let end = p.expires;
    let before = m.world.game.units[&2].hp;
    flights(&mut m, 1);
    assert!(m.firing.flights.contains_key(&id));
    close(m.world.game.units[&2].hp, before);
    flights(&mut m, 30);
    assert!(!m.firing.flights.contains_key(&id));
    assert!(m.firing.events.iter().any(|e| matches!(e, LaunchEvent::Expiry { id: event_id, time, .. } if *event_id == id && *time == end)));
}

#[test]
fn cmb49_terrain_start_cell_and_moving_bodies_take_first_contact() {
    let mut m = scene(&[
        ("sniper", 0, 20.0, 20.0),
        ("rifle", 1, 40.0, 20.0),
        ("rifle", 1, 30.0, 24.0),
    ]);
    m.fire_weapon(1, 2, false).unwrap();
    let mut p = m.firing.flights.values().next().unwrap().clone();
    let a = projectiles::Point3::new(20.0, 1.0, 20.0);
    let b = projectiles::Point3::new(42.0, 1.0, 20.0);
    let start = m.world.game.cell_of(20.0, 20.0).unwrap();
    let beside = start + 1;
    let wall = m.world.game.cell_of(28.0, 20.0).unwrap();
    for cell in [start, beside, wall] {
        m.world.set_cell(cell, b'#').unwrap();
    }
    let contact = m.flight_contact(&p, a, b, 0.0, 1.0).unwrap().unwrap();
    assert_eq!(contact.surface, FlightSurface::Cell(wall));
    m.world.set_cell(wall, b'.').unwrap();
    p.sequence = 1;
    assert_eq!(
        m.flight_contact(&p, a, b, 0.0, 1.0)
            .unwrap()
            .unwrap()
            .surface,
        FlightSurface::Cell(start)
    );
    for cell in [start, beside] {
        m.world.set_cell(cell, b'.').unwrap();
    }
    p.sequence = 0;
    m.firing.prior.insert(
        3,
        crate::game::Pose {
            x: 30.0,
            z: 16.0,
            rot: 0.0,
        },
    );
    m.grid.rebuild(&m.world.game.units);
    assert_eq!(
        m.flight_contact(&p, a, b, 0.0, 1.0)
            .unwrap()
            .unwrap()
            .surface,
        FlightSurface::Unit(3)
    );
    m.firing.prior.clear();
    assert_eq!(
        m.flight_contact(&p, a, b, 0.0, 1.0)
            .unwrap()
            .unwrap()
            .surface,
        FlightSurface::Unit(2)
    );
}

#[test]
fn cmb49_off_map_aircraft_rounds_hit_border_houses_on_the_true_segment() {
    for distance in [20.0, 1000.0] {
        for (from, target, border) in [
            (
                Point {
                    x: -distance,
                    z: 21.0,
                },
                Point { x: 4.0, z: 21.0 },
                Point { x: 1.0, z: 21.0 },
            ),
            (
                Point {
                    x: 128.0 + distance,
                    z: 21.0,
                },
                Point { x: 124.0, z: 21.0 },
                Point { x: 127.0, z: 21.0 },
            ),
            (
                Point {
                    x: 21.0,
                    z: -distance,
                },
                Point { x: 21.0, z: 4.0 },
                Point { x: 21.0, z: 1.0 },
            ),
            (
                Point {
                    x: 21.0,
                    z: 128.0 + distance,
                },
                Point { x: 21.0, z: 124.0 },
                Point { x: 21.0, z: 127.0 },
            ),
        ] {
            let mut m = scene(&[("rifle", 0, 20.0, 20.0), ("tank", 1, target.x, target.z)]);
            let kind = m.rules.kind("attacker").unwrap();
            let mut plane = unit_spawn::initial_aircraft(
                &m.rules,
                Catalog::compiled().unwrap(),
                kind,
                1,
                0,
                from,
            )
            .unwrap();
            plane.air = Some(crate::game::PlaneState::Out);
            m.world.game.units.insert(1, plane);
            m.aviation.planes.insert(
                1,
                crate::match_aviation::AircraftRuntime {
                    fuel: 50.0,
                    ammo: 8.0,
                    timer: 0.0,
                    orbit: 0.0,
                    mission: Some(crate::match_aviation::PlaneMission {
                        kind: crate::match_aviation::MissionKind::Patrol,
                        target: None,
                        at: target,
                    }),
                    attack_pick: 0,
                    downed: false,
                },
            );
            m.grid.rebuild(&m.world.game.units);
            let cell = m.world.game.cell_of(border.x, border.z).unwrap();
            m.world.set_cell(cell, b'B').unwrap();
            m.fire_weapon(1, 2, false).unwrap();
            m.validate_firing_tick_state().unwrap();
            let p = m.firing.flights.values().next().unwrap();
            assert_eq!(p.start_cell, usize::MAX);
            let a = p.launch_origin;
            close(a.x, from.x);
            close(a.z, from.z);
            close(a.y, 20.0);
            // An explicit aim isolates terrain geometry from weapon dispersion.
            let b = projectiles::Point3::new(target.x, 1.2, target.z);
            let contact = m.flight_contact(p, a, b, 0.0, 1.0).unwrap().unwrap();
            assert_eq!(contact.surface, FlightSurface::Cell(cell));
            close(contact.hit.fraction, distance / (distance + 4.0));
        }
    }
}

#[test]
fn cmb49_off_map_cell_traversal_rejects_stationary_and_parallel_misses() {
    let m = scene(&[("rifle", 0, 20.0, 20.0)]);
    for (a, b) in [
        ((-1000.0, 21.0), (-1000.0, 21.0)),
        ((-1000.0, 21.0), (-900.0, 21.0)),
        ((-1000.0, -1.0), (1000.0, -1.0)),
        ((128.0, -1000.0), (128.0, 1000.0)),
    ] {
        assert!(
            m.flight_crossed_cells(
                projectiles::Point3::new(a.0, 4.0, a.1),
                projectiles::Point3::new(b.0, 4.0, b.1),
            )
            .unwrap()
            .is_empty()
        );
    }
    let at = projectiles::Point3::new(21.0, 4.0, 21.0);
    assert_eq!(
        m.flight_crossed_cells(at, at).unwrap(),
        vec![m.world.game.cell_of(at.x, at.z).unwrap()]
    );
}

#[test]
fn cmb49_terrain_contact_respects_each_solid_height() {
    for (ch, height) in [
        (b'B', 5.0),
        (b'K', 5.0),
        (b'H', 1.8),
        (b'#', 1.2),
        (b'Y', 1.2),
        (b'Q', 1.6),
        (b'R', 0.8),
        (b'O', 0.8),
        (b'=', 0.25),
    ] {
        let mut m = scene(&[("sniper", 0, 20.0, 20.0), ("rifle", 1, 60.0, 20.0)]);
        m.fire_weapon(1, 2, false).unwrap();
        let p = m.firing.flights.values().next().unwrap();
        let p = p.clone();
        let cell = m.world.game.cell_of(30.0, 20.0).unwrap();
        m.world.set_cell(cell, ch).unwrap();
        let contact = |y| {
            m.flight_contact(
                &p,
                projectiles::Point3::new(28.0, y, 21.0),
                projectiles::Point3::new(34.0, y, 21.0),
                0.0,
                1.0,
            )
            .unwrap()
        };
        assert_eq!(
            contact(height - 0.01).unwrap().surface,
            FlightSurface::Cell(cell),
            "{}",
            char::from(ch)
        );
        assert!(contact(height + 0.01).is_none(), "{}", char::from(ch));
    }
}

#[test]
fn cmb57_range_scatter_ammo_and_arrival_blast_use_the_same_shell() {
    for (distance, spread) in [(30.0, 3.0), (62.5, 5.5), (95.0, 8.0)] {
        let mut m = scene(&[
            ("howitzer", 0, 20.0, 20.0),
            ("rifle", 1, 20.0 + distance, 20.0),
        ]);
        reserves(&mut m, 1, "howitzer");
        m.logistics.get_mut(1).unwrap().ammo = 1.0;
        let at = m.world.game.units[&2].at();
        let before = m.world.game.units[&2].hp;
        let mut rng = m.start.random.clone();
        let angle = rng.next_fraction(StreamId::Combat) * std::f64::consts::TAU;
        let radius = rng.next_fraction(StreamId::Combat).sqrt() * spread;
        let ids = m.launch_weapon_salvo(1, at, Some(4)).unwrap();
        assert_eq!(ids.len(), 1);
        close(m.logistics.get(1).unwrap().ammo, 0.0);
        close(m.firing.salvos[0].spread, spread);
        let p = &m.firing.flights[&ids[0]];
        let end = projectiles::flight_at(p.solution, p.expires).unwrap();
        close(end.x, at.x + math::cos(angle) * radius);
        close(end.z, at.z + math::sin(angle) * radius);
        close(m.world.game.units[&2].hp, before);
        // Place the target at the committed landing spot, then run actual flight arrival.
        m.world.game.units.get_mut(&2).unwrap().x = end.x;
        m.world.game.units.get_mut(&2).unwrap().z = end.z;
        m.grid.rebuild(&m.world.game.units);
        flights(&mut m, 70);
        assert!(m.world.game.units[&2].hp < before);
        let after = m.world.game.units[&2].hp;
        flights(&mut m, 10);
        close(m.world.game.units[&2].hp, after);
    }
}

#[test]
fn cmb76_completed_gun_pit_extends_only_weapon_teams_on_live_trench() {
    let mut m = scene(&[
        ("rifle", 0, 20.0, 20.0),
        ("at", 0, 21.0, 21.0),
        ("tank", 1, 77.0, 21.0),
    ]);
    m.start.economy.players[0].mp = 1000.0;
    let request = crate::field_works::StartRequest {
        control_eligible: true,
        owner: 0,
        unit: 1,
        kind: crate::field_works::FortKind::Nest,
        at: Point { x: 21.0, z: 21.0 },
        direction: Some(0.0),
    };
    m.field_works
        .prepare_start(&m.world, &mut m.start.economy.players[0], request, |_| true)
        .unwrap()
        .commit(&mut m.field_works);
    for _ in 0..10 {
        m.field_works.step_unit(&mut m.world, 1, 10.0).unwrap();
    }
    let cell = m.world.game.cell_of(21.0, 21.0).unwrap();
    assert!(m.field_works.pits().contains(&cell));
    let weapon = m.weapon(&m.world.game.units[&2]).unwrap().unwrap();
    close(
        m.weapon_range(&m.world.game.units[&2], weapon).unwrap(),
        56.25,
    );
    m.world.game.players[0].visible = vec![3];
    assert!(m.can_shoot_target(2, 3).unwrap());
    m.set_firing_target(2, 3).unwrap();
    m.commands.units.get_mut(&2).unwrap().attack_id = 3;
    m.apply_unit_firing(2, false, 0.05).unwrap();
    assert!(!m.firing.flights.is_empty());
    for name in ["mg", "at", "mortar", "flak", "howitzer", "rifle", "tank"] {
        let mut u = m.world.game.units[&2].clone();
        u.kind = m.rules.kind(name).unwrap();
        let w = m.weapon(&u).unwrap().unwrap();
        close(
            m.weapon_range(&u, w).unwrap(),
            n(w, "range", 0.0)
                * if name == "rifle" || name == "tank" {
                    1.0
                } else {
                    1.25
                },
        );
    }
    m.world.set_cell(cell, b'+').unwrap();
    close(
        m.weapon_range(&m.world.game.units[&2], weapon).unwrap(),
        45.0,
    );
    assert!(!m.can_shoot_target(2, 3).unwrap());
}

#[test]
fn cmb77_planes_recon_and_owner_flag_follow_camouflage() {
    let mut m = scene(&[("rifle", 0, 20.0, 20.0), ("sniper", 1, 40.0, 20.0)]);
    let own_flags = |m: &Match| {
        let cache = crate::recipient_snapshot::SnapshotCache::new(m).unwrap();
        let rows = cache.recipient(1).unwrap();
        rows.fields["units"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row[0] == 2)
            .unwrap()[12]
            .as_u64()
            .unwrap()
    };
    assert_ne!(own_flags(&m) & 256, 0);
    let kind = m.rules.kind("fighter").unwrap();
    let mut plane = unit_spawn::initial_aircraft(
        &m.rules,
        Catalog::compiled().unwrap(),
        kind,
        3,
        0,
        Point { x: 16.0, z: 20.0 },
    )
    .unwrap();
    plane.air = Some(crate::game::PlaneState::Station);
    m.world.game.units.insert(3, plane);
    sight(&mut m);
    assert!(!m.world.game.players[0].sees(2));
    m.world.game.units.get_mut(&3).unwrap().x = 17.0;
    sight(&mut m);
    assert!(m.world.game.players[0].sees(2));
    m.world.game.units.remove(&3);
    m.world.game.strikes.push(crate::game::Strike {
        live: true,
        kind: crate::game::StrikeKind::Recon,
        owner: 0,
        x: 40.0,
        z: 20.0,
        dir: 0.0,
        runtime: None,
    });
    sight(&mut m);
    assert!(m.world.game.players[0].sees(2));
    m.world.game.strikes.clear();
    m.world.game.units.get_mut(&2).unwrap().still = Some(0.0);
    sight(&mut m);
    assert_eq!(own_flags(&m) & 256, 0);
}
