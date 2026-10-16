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
use serde_json::Value as Json;

fn arena() -> AuthoritativeMap {
    let f: Json = serde_json::from_str(include_str!(
        "../../simtest/tests/fixtures/base-pending-paratroopers.json"
    ))
    .unwrap();
    AuthoritativeMap::from_json(&serde_json::to_vec(&f["map"]).unwrap()).unwrap()
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
fn cmb01_faction_locks_preserve_all_four_buy_sets() {
    // sim.js:393-394; test.js:1018-1040. Exact catalog comparisons remain in economy.rs.
    for (name, owner) in [
        ("ranger", 0),
        ("tiger", 1),
        ("conscript", 2),
        ("commando", 3),
        ("churchill", 3),
    ] {
        for faction in 0..4 {
            assert_eq!(
                crate::economy::can_build(name, Faction::try_from(faction).unwrap()).unwrap(),
                faction == owner
            );
        }
    }
    for faction in 0..4 {
        let faction = Faction::try_from(faction).unwrap();
        assert!(crate::economy::can_build("rifle", faction).unwrap());
        assert!(!crate::economy::can_build("kaiju", faction).unwrap());
    }
}

#[test]
fn cmb03_uk_support_admission_creates_fifteen_shells() {
    let mut m = ordinary();
    for (owner, faction, count) in [(0, Faction::Uk, 15.0), (1, Faction::Germany, 10.0)] {
        m.start.players[owner].config.faction = faction;
        m.start.economy.players[owner].mp = 1000.0;
        assert_eq!(
            m.admit_support(owner, "artillery", Point { x: 60.0, z: 60.0 }, Some(0.0))
                .unwrap(),
            None
        );
        close(
            m.world
                .game
                .strikes
                .last()
                .unwrap()
                .runtime
                .as_ref()
                .unwrap()
                .left,
            count,
        );
        close(m.start.economy.players[owner].mp, 850.0);
    }
}

#[test]
fn cmb38_unfinished_pillbox_cannot_fire_finished_one_launches() {
    let mut m = scene(&[("pillbox", 0, 30.0, 30.0), ("rifle", 1, 45.0, 30.0)]);
    m.world.game.units.get_mut(&1).unwrap().built = Some(0.99);
    launch(&mut m, 1, 2);
    assert!(m.firing.flights.is_empty());
    m.world.game.units.get_mut(&1).unwrap().built = Some(1.0);
    launch(&mut m, 1, 2);
    assert_eq!(m.firing.flights.len(), 1);
}

#[test]
fn cmb42_sniper_and_at_choose_their_jobs_over_nearer_rifles() {
    for (name, other, distance) in [("sniper", "mg", 30.0), ("at", "tank", 28.0)] {
        let mut m = scene(&[
            (name, 0, 5.0, 21.0),
            ("rifle", 1, 12.0, 21.0),
            (other, 1, 5.0 + distance, 21.0),
        ]);
        m.begin_firing_tick(&[1, 2, 3]).unwrap();
        m.apply_unit_firing(1, false, 0.05).unwrap();
        assert_eq!(
            m.firing.get(1).unwrap().target_id,
            3,
            "{name} prefers its role"
        );
    }
    for (a, t, value) in [
        ("at", "tank", 3.0),
        ("tankdestroyer", "tank", 3.0),
        ("flamer", "rifle", 2.0),
        ("sniper", "howitzer", 3.0),
        ("engineer", "rifle", 2.0),
        ("rifle", "tank", 1.0),
    ] {
        close(crate::combat::role_mul(a, t).unwrap(), value);
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
fn cmb51_52_arrival_uses_current_armor_facing_and_credits_damage_once() {
    for (kind, rotation, expected) in [
        ("tank", 0.0, 240.0),
        ("churchill", std::f64::consts::PI, 72.0),
    ] {
        let mut m = scene(&[("at", 0, 20.0, 20.0), (kind, 1, 30.0, 20.0)]);
        m.abilities.get_mut(1).unwrap().ap = true;
        m.fire_weapon(1, 2, false).unwrap();
        m.world.game.units.get_mut(&2).unwrap().rot = rotation;
        let before = m.world.game.units[&2].hp;
        // AP is not part of these armor assertions.
        for f in m.firing.flights.values_mut() {
            f.armor_piercing = false;
        }
        flights(&mut m, 8);
        close(before - m.world.game.units[&2].hp, expected);
        let xp = m.combat.get(1).unwrap().xp;
        close(xp, expected);
        flights(&mut m, 8);
        close(m.combat.get(1).unwrap().xp, xp);
        assert_eq!(m.combat.get(2).unwrap().last_hit, Some(0));
    }
}

#[test]
fn cmb54_travel_suppresses_once_per_volley_and_retreat_and_sprint_are_immune() {
    for (retreat, sprint) in [(false, 0.0), (true, 0.0), (false, 1.0)] {
        let mut m = scene(&[("mg", 0, 20.0, 20.0), ("rifle", 1, 35.0, 20.0)]);
        let target = m.world.game.units.get_mut(&2).unwrap();
        target.retreating = retreat;
        target.sprint = sprint;
        m.fire_weapon(1, 2, false).unwrap();
        let flight = m.firing.flights.values().next().unwrap().clone();
        let a = projectiles::Point3::new(20.0, 1.2, 20.0);
        let b = projectiles::Point3::new(36.0, 1.2, 20.0);
        m.suppress_flight_segment(&flight, a, b).unwrap();
        let supp = m.world.game.units[&2].supp;
        m.suppress_flight_segment(&flight, a, b).unwrap();
        close(m.world.game.units[&2].supp, supp);
        if retreat || sprint > 0.0 {
            close(supp, 0.0);
        } else {
            assert!(supp > 0.0);
            assert_eq!(m.combat.get(2).unwrap().hit_by, Some(1));
        }
    }
}

#[test]
fn cmb55_suppression_speed_thresholds_and_trench_rally() {
    for (supp, speed) in [(49.9, 1.0), (50.0, 0.6), (89.9, 0.6), (90.0, 0.3)] {
        close(crate::movement::supp_speed(supp), speed);
    }
    let mut m = ordinary();
    let ids = m
        .world
        .game
        .units
        .values()
        .filter(|u| m.rules.kinds[u.kind].name == "rifle" && u.owner == 0)
        .map(|u| u.id)
        .collect::<Vec<_>>();
    assert!(ids.len() >= 2);
    let c = m
        .world
        .game
        .cell_of(m.world.game.units[&ids[0]].x, m.world.game.units[&ids[0]].z)
        .unwrap();
    m.world.set_cell(c, b'T').unwrap();
    for id in &ids {
        m.world.game.units.get_mut(id).unwrap().supp = 60.0;
    }
    m.advance_tick().unwrap();
    close(m.world.game.units[&ids[0]].supp, 59.3);
    close(m.world.game.units[&ids[1]].supp, 59.6);
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
fn cmb58_salvo_timer_consumes_two_draws_only_when_shell_is_due() {
    let mut m = scene(&[("rocket", 0, 20.0, 20.0), ("rifle", 1, 40.0, 20.0)]);
    launch(&mut m, 1, 2);
    let mut expected = m.start.random.clone();
    m.advance_firing_salvos(0.01).unwrap();
    assert_eq!(m.start.random, expected);
    let due = m.firing.salvos[0].next;
    expected.next_fraction(StreamId::Combat);
    expected.next_fraction(StreamId::Combat);
    m.advance_firing_salvos(due).unwrap();
    assert_eq!(m.start.random, expected);
    assert_eq!(m.firing.salvos[0].left, 7);
}

#[test]
fn cmb71_74_trench_settles_and_front_arc_controls_cover() {
    let mut m = scene(&[("rifle", 0, 21.0, 21.0)]);
    let c = m.world.game.cell_of(21.0, 21.0).unwrap();
    m.world.set_cell(c, b'T').unwrap();
    let front = Point { x: 21.0, z: 0.0 };
    let rear = Point { x: 21.0, z: 40.0 };
    m.world.game.units.get_mut(&1).unwrap().still = Some(0.0);
    close(m.cover_mul(&m.world.game.units[&1], front).unwrap(), 0.5);
    m.world.game.units.get_mut(&1).unwrap().still = Some(8.0);
    close(m.cover_mul(&m.world.game.units[&1], front).unwrap(), 0.3);
    close(m.cover_mul(&m.world.game.units[&1], rear).unwrap(), 0.3);
    m.world.game.trench_front = Some(vec![f32::NAN; m.world.game.chars.len()]);
    m.world.game.trench_front.as_mut().unwrap()[c] = -std::f32::consts::FRAC_PI_2;
    close(m.cover_mul(&m.world.game.units[&1], front).unwrap(), 0.3);
    close(m.cover_mul(&m.world.game.units[&1], rear).unwrap(), 0.5);
}

#[test]
fn cmb72_quality_tracks_woods_crater_wear_and_wall_health() {
    let mut m = scene(&[("rifle", 0, 21.0, 21.0)]);
    let c = m.world.game.cell_of(21.0, 21.0).unwrap();
    for (ch, expected) in [(b'O', 0.6), (b'#', 1.0)] {
        m.world.set_cell(c, ch).unwrap();
        close(m.cover_quality(c).unwrap(), expected);
    }
    m.world.game.cell_hp[c] *= 0.5;
    close(m.cover_quality(c).unwrap(), 0.75);
    m.world.set_cell(c, b'+').unwrap();
    m.world.game.wear.as_mut().unwrap()[c] = 0.0;
    close(m.cover_quality(c).unwrap(), 0.55);
    m.world.game.wear.as_mut().unwrap()[c] = 1.0;
    close(m.cover_quality(c).unwrap(), 1.4);
}

#[test]
fn cmb73_wall_distance_flank_corner_and_live_hull_cover() {
    let mut m = scene(&[("rifle", 0, 19.0, 21.0)]);
    let c = 10 * m.world.game.w + 10;
    m.world.set_cell(c, b'#').unwrap();
    let from = Point { x: 39.0, z: 21.0 };
    close(m.cover_behind(&m.world.game.units[&1], from).unwrap(), 1.0);
    m.world.game.units.get_mut(&1).unwrap().x = 17.4;
    let partial = m.cover_behind(&m.world.game.units[&1], from).unwrap();
    assert!(partial > 0.1 && partial < 0.9);
    m.world.game.units.get_mut(&1).unwrap().x = 15.0;
    close(m.cover_behind(&m.world.game.units[&1], from).unwrap(), 0.0);
    m.world.game.units.get_mut(&1).unwrap().x = 19.0;
    close(
        m.cover_behind(&m.world.game.units[&1], Point { x: 19.0, z: 40.0 })
            .unwrap(),
        0.0,
    );
    m.world.set_cell(c, b'B').unwrap();
    close(m.cover_behind(&m.world.game.units[&1], from).unwrap(), 1.0);
    let hull = scene(&[("rifle", 0, 20.0, 20.0), ("tank", 0, 22.0, 20.0)]);
    close(
        hull.cover_behind(&hull.world.game.units[&1], Point { x: 40.0, z: 20.0 })
            .unwrap(),
        1.0,
    );
}

#[test]
fn cmb75_all_house_types_lose_accuracy_protection_as_the_cell_is_damaged() {
    let mut m = scene(&[("rifle", 0, 21.0, 21.0)]);
    let c = m.world.game.cell_of(21.0, 21.0).unwrap();
    m.world.set_cell(c, b'B').unwrap();
    m.world.game.units.get_mut(&1).unwrap().garrison = Some(c);
    for (kind, hp, mul, worn) in [
        (0, 250.0, 0.55, 0.85),
        (1, 400.0, 0.35, 0.7),
        (2, 650.0, 0.25, 0.55),
        (3, 700.0, 0.25, 0.55),
        (4, 1000.0, 0.2, 0.45),
        (5, 800.0, 0.25, 0.5),
    ] {
        m.world.game.house[c] = kind;
        m.world.game.cell_hp[c] = hp;
        close(
            m.cover_mul(&m.world.game.units[&1], Point { x: 0.0, z: 21.0 })
                .unwrap(),
            mul,
        );
        m.world.game.cell_hp[c] = 0.0;
        close(
            m.cover_mul(&m.world.game.units[&1], Point { x: 0.0, z: 21.0 })
                .unwrap(),
            worn,
        );
    }
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

#[test]
fn cmb78_retreat_clears_orders_paths_home_and_stops_firing() {
    let mut m = scene(&[("rifle", 0, 20.0, 20.0), ("tank", 1, 30.0, 20.0)]);
    m.commands.units.get_mut(&1).unwrap().attack_id = 2;
    m.set_firing_target(1, 2).unwrap();
    m.commands
        .units
        .get_mut(&1)
        .unwrap()
        .orders
        .push_back(crate::match_commands::Command::Stop { ids: vec![1] });
    m.retreat_unit(1).unwrap();
    let u = &m.world.game.units[&1];
    assert!(u.retreating && !u.path.is_empty());
    assert!(m.commands.units[&1].orders.is_empty());
    assert_eq!(m.commands.units[&1].attack_id, 0);
    m.apply_unit_firing(1, false, 0.05).unwrap();
    assert_eq!(m.firing.get(1).unwrap().target_id, 0);
    assert!(m.firing.flights.is_empty());
}

#[test]
fn cmb79_autoretreat_obeys_health_home_distance_and_switch() {
    let mut m = scene(&[("rifle", 0, 80.0, 80.0)]);
    m.world.game.units.get_mut(&1).unwrap().hp = 35.0;
    assert!(!m.automatic_retreat_candidate(1).unwrap());
    m.world.game.units.get_mut(&1).unwrap().hp = 34.0;
    assert!(m.automatic_retreat_candidate(1).unwrap());
    m.commands.units.get_mut(&1).unwrap().auto_retreat = false;
    assert!(!m.automatic_retreat_candidate(1).unwrap());
    m.commands.units.get_mut(&1).unwrap().auto_retreat = true;
    let home = m.reaction_home(1).unwrap();
    m.world.game.units.get_mut(&1).unwrap().x = home.x;
    m.world.game.units.get_mut(&1).unwrap().z = home.z;
    assert!(!m.automatic_retreat_candidate(1).unwrap());
}

#[test]
fn cmb80_home_reinforcement_restores_two_models_and_repairs_vehicle_for_mp() {
    let mut m = scene(&[
        ("rifle", 0, 21.0, 21.0),
        ("hq", 0, 15.0, 20.0),
        ("tank", 0, 20.0, 25.0),
    ]);
    m.world.game.units.get_mut(&1).unwrap().hp = 40.0;
    m.world.game.units.get_mut(&3).unwrap().hp = 200.0;
    m.start.economy.players[0].mp = 1000.0;
    for _ in 0..100 {
        let frame = m.prepare_recovery(&[1, 2, 3]).unwrap();
        m.reinforce(&frame, 0.05).unwrap();
        m.world.game.tick += 1;
    }
    close(m.world.game.units[&1].hp, 80.0);
    close(m.world.game.units[&3].hp, 290.0);
    close(m.start.economy.players[0].mp, 935.0);
}

#[test]
fn cmb50_shallow_hard_contact_continues_same_round_then_contact_limit_stops_it() {
    for explosive in [false, true] {
        let mut m = scene(&[("at", 0, 20.0, 20.0), ("tank", 1, 30.0, 20.0)]);
        m.fire_weapon(1, 2, false).unwrap();
        let id = *m.firing.flights.keys().next().unwrap();
        let mut f = m.firing.flights.remove(&id).unwrap();
        f.explosive = explosive;
        let contact = Some(FlightContact {
            surface: FlightSurface::Unit(2),
            hit: projectiles::ContactHit {
                fraction: 0.5,
                normal: projectiles::Point3::new(0.0, 0.0, -1.0),
            },
        });
        let velocity = projectiles::Point3::new(150.0, 0.0, 10.0);
        let at = projectiles::Point3::new(28.0, 1.2, 20.0);
        let terminal = m
            .resolve_live_flight(&mut f, contact, at, 5.1, velocity)
            .unwrap();
        assert_eq!(terminal, explosive);
        assert_eq!(f.id, id);
        assert_eq!(f.sequence, 1);
        if !explosive {
            assert!(f.energy < 1.0 && f.energy > 0.0);
            assert!(f.solution.velocity.z < 0.0);
            let v = f.solution.velocity;
            assert!(m.resolve_live_flight(&mut f, contact, at, 5.2, v).unwrap());
            assert_eq!(f.sequence, 2);
        }
    }
}

#[test]
fn cmb53_direct_terrain_contact_wears_the_contacted_solid() {
    let mut m = scene(&[("tank", 0, 20.0, 20.0), ("rifle", 1, 35.0, 20.0)]);
    let cell = 10 * m.world.game.w + 14;
    m.world.set_cell(cell, b'#').unwrap();
    m.fire_weapon(1, 2, false).unwrap();
    let id = *m.firing.flights.keys().next().unwrap();
    let mut f = m.firing.flights.remove(&id).unwrap();
    let before = m.world.game.cell_hp[cell];
    let at = projectiles::Point3::new(29.0, 1.0, 21.0);
    let contact = Some(FlightContact {
        surface: FlightSurface::Cell(cell),
        hit: projectiles::ContactHit {
            fraction: 0.5,
            normal: projectiles::Point3::new(-1.0, 0.0, 0.0),
        },
    });
    assert!(
        m.resolve_live_flight(
            &mut f,
            contact,
            at,
            5.1,
            projectiles::Point3::new(150.0, 0.0, 0.0)
        )
        .unwrap()
    );
    assert!(m.world.game.cell_hp[cell] < before);
    assert!(m.world.game.wear.as_ref().unwrap()[cell] > 0.0);
}

#[test]
fn cmb56_blast_friendly_fire_falloff_trench_and_retreat_multipliers() {
    let mut m = scene(&[
        ("rifle", 0, 40.0, 40.0),
        ("rifle", 1, 42.0, 40.0),
        ("rifle", 1, 40.0, 42.0),
        ("rifle", 1, 38.0, 40.0),
    ]);
    let c = m.world.game.cell_of(40.0, 42.0).unwrap();
    m.world.set_cell(c, b'T').unwrap();
    m.world.game.units.get_mut(&4).unwrap().retreating = true;
    let profile = ExplosionSource {
        infantry: 40.0,
        vehicle: 15.0,
        terrain: None,
        suppression: 50.0,
        scar: 0.0,
        anti_garrison: None,
        weapon_kind: None,
        off_map_support: false,
    };
    let (mut d, t) = m.firing_damage_view(&[1, 2, 3, 4]);
    d.explosion(
        Point { x: 40.0, z: 40.0 },
        4.0,
        profile,
        0,
        None,
        None,
        None,
        t,
    )
    .unwrap();
    close(m.world.game.units[&1].hp, 60.0);
    close(m.world.game.units[&2].hp, 70.0);
    close(m.world.game.units[&3].hp, 85.0);
    close(m.world.game.units[&4].hp, 92.5);
    assert!(m.world.game.units[&2].supp > 0.0);
}

#[test]
fn cmb60_bunker_direct_and_offmap_damage_have_separate_scaling() {
    let mut m = scene(&[("tank", 0, 20.0, 20.0), ("bunker", 1, 30.0, 20.0)]);
    let before = m.world.game.units[&2].hp;
    let source = crate::match_combat::FlightSource {
        shooter: 1,
        kind: m.world.game.units[&1].kind,
        owner: 0,
        airborne: false,
    };
    let impact = crate::combat::Impact {
        owner: 0,
        shooter: 1,
        weapon: crate::combat::WeaponDamage::from_catalog("tank").unwrap(),
        damage_scale: None,
        energy: 1.0,
        armor_piercing: false,
        ap_naval_only: false,
        ap_multiplier: 1.5,
        launch_origin: Point { x: 20.0, z: 20.0 },
        at: Point { x: 30.0, z: 20.0 },
        direction: Point { x: 1.0, z: 0.0 },
    };
    let (mut d, t) = m.firing_damage_view(&[1, 2]);
    d.direct_hit(source, 2, impact, t).unwrap();
    close(before - m.world.game.units[&2].hp, 11.25);
    let before = m.world.game.units[&2].hp;
    let (mut d, t) = m.firing_damage_view(&[1, 2]);
    d.hurt(2, ExplosionSource::support("dive").unwrap(), 1.0, 0, t)
        .unwrap();
    close(before - m.world.game.units[&2].hp, 25.0);
    for defenders in [1, 2, 4] {
        close(
            crate::combat::bunker_mul("bunker", Mode::Horde, defenders).unwrap(),
            1.0 / f64::from(defenders),
        );
        close(
            crate::combat::bunker_mul("kaiju", Mode::Horde, defenders).unwrap(),
            1.0 / f64::from(defenders),
        );
    }
}

#[test]
fn cmb63_star_thresholds_reduce_actual_damage_and_suppression() {
    for (xp, stars) in [(99.0, 0), (100.0, 1), (249.0, 1), (250.0, 2), (500.0, 3)] {
        assert_eq!(crate::combat::veterancy("rifle", xp).unwrap(), stars);
        let mut m = scene(&[("tank", 0, 20.0, 20.0), ("rifle", 1, 30.0, 20.0)]);
        m.combat.get_mut(2).unwrap().xp = xp;
        let profile = ExplosionSource {
            infantry: 40.0,
            vehicle: 15.0,
            terrain: None,
            suppression: 50.0,
            scar: 0.0,
            anti_garrison: None,
            weapon_kind: None,
            off_map_support: false,
        };
        let (mut d, t) = m.firing_damage_view(&[1, 2]);
        d.hurt(2, profile, 1.0, 0, t).unwrap();
        close(
            m.world.game.units[&2].hp,
            100.0 - 40.0 * (1.0 - 0.08 * f64::from(stars)),
        );
        close(
            m.world.game.units[&2].supp,
            50.0 * (1.0 - 0.15 * f64::from(stars)),
        );
    }
    assert_eq!(crate::combat::veterancy("bunker", 10000.0).unwrap(), 0);
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
fn cmb66_support_interception_checks_empty_ammo_setup_and_holdfire() {
    for (ammo, still, hold, attempts) in [
        (0.0, 10.0, false, 0),
        (1.0, 0.0, false, 0),
        (1.0, 10.0, true, 0),
        (1.0, 10.0, false, 1),
    ] {
        let mut m = scene(&[("flak", 0, 40.0, 40.0)]);
        reserves(&mut m, 1, "flak");
        m.logistics.get_mut(1).unwrap().ammo = ammo;
        m.world.game.units.get_mut(&1).unwrap().still = Some(still);
        m.commands.units.get_mut(&1).unwrap().hold_fire = hold;
        m.start.economy.players[1].mp = 1000.0;
        assert_eq!(
            m.admit_support(1, "recon", Point { x: 40.0, z: 40.0 }, Some(0.0))
                .unwrap(),
            None
        );
        for _ in 0..61 {
            m.step_support_strikes(&[1], 0.05).unwrap();
        }
        let actual = m
            .support
            .events
            .iter()
            .filter(|e| matches!(e, crate::match_support::SupportEvent::Flak { unit: 1, .. }))
            .count();
        assert_eq!(actual, attempts);
        close(m.logistics.get(1).unwrap().ammo, ammo - attempts as f64);
    }
}

#[test]
fn cmb68_enemy_carrier_hit_ejects_deck_riders_with_shock() {
    let mut m = scene(&[
        ("rifle", 1, 20.0, 20.0),
        ("tank", 0, 30.0, 20.0),
        ("rifle", 0, 30.0, 20.0),
    ]);
    m.combat.get_mut(2).unwrap().cargo = vec![3];
    m.combat.get_mut(3).unwrap().carrier = Some(2);
    m.world.game.units.get_mut(&3).unwrap().riding = true;
    let kind = m.world.game.units[&1].kind;
    let impact = crate::combat::Impact {
        owner: 1,
        shooter: 1,
        weapon: crate::combat::WeaponDamage::from_catalog("rifle").unwrap(),
        damage_scale: None,
        energy: 1.0,
        armor_piercing: false,
        ap_naval_only: false,
        ap_multiplier: 1.5,
        launch_origin: Point { x: 20.0, z: 20.0 },
        at: Point { x: 30.0, z: 20.0 },
        direction: Point { x: 1.0, z: 0.0 },
    };
    let (mut d, t) = m.firing_damage_view(&[1, 2, 3]);
    d.direct_hit(
        crate::match_combat::FlightSource {
            shooter: 1,
            kind,
            owner: 1,
            airborne: false,
        },
        2,
        impact,
        t,
    )
    .unwrap();
    assert!(!m.world.game.units[&3].riding);
    assert!(m.world.game.units[&3].supp >= 60.0);
    assert_eq!(m.combat.get(3).unwrap().carrier, None);
    assert!(m.combat.get(2).unwrap().cargo.is_empty());
}

#[test]
fn cmb70_dead_vehicle_settles_into_rule_wreck_infantry_does_not() {
    let mut m = scene(&[("tank", 0, 40.0, 40.0), ("rifle", 1, 60.0, 40.0)]);
    m.world.game.units.get_mut(&1).unwrap().hp = 0.0;
    m.world.game.units.get_mut(&2).unwrap().hp = 0.0;
    m.retire_dead(&[1, 2]).unwrap();
    assert_eq!(m.world.game.wrecks.len(), 1);
    let id = m.world.game.wrecks[0].id;
    for _ in 0..400 {
        m.world.game.tick += 1;
        let mut context = crate::environment::Context {
            tick: m.world.game.tick as u64,
            seed: &mut m.start.environment_seed,
            next_id: &mut m.next_unit_id,
            unit_order: &[],
            grid: &m.grid,
            rain: 0.0,
            wet: 0.0,
            wind_angle: 0.0,
            wind_strength: 0.5,
        };
        m.environment
            .step_wrecks(&mut m.world, 0.05, &mut context)
            .unwrap();
    }
    assert_eq!(m.world.game.wrecks[0].id, id);
    assert!(!m.world.game.wrecks[0].moving);
    let c = m.world.game.wrecks[0].cell.unwrap();
    assert_eq!(m.world.game.physical(c), b'Q');
    assert_ne!(m.world.game.flags[c] & crate::game::VBLOCK, 0);
    assert_ne!(m.world.game.flags[c] & crate::game::COVER, 0);
}
