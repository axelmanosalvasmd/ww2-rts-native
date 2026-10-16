//! Supported native damage, section contact and veterancy behavior checks.
use super::*;
use crate::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    map::AuthoritativeMap,
    match_commands::UnitControl,
    match_runtime::{MatchOptions, PlayerConfig},
    unit_spawn,
    weather::WeatherChoice,
    world::World,
};

fn arena() -> AuthoritativeMap {
    let map = serde_json::json!({
        "name": "Native combat damage arena", "w": 64, "h": 64,
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

fn impact(m: &Match, shooter: u32, target: u32) -> crate::combat::Impact {
    let u = &m.world.game.units[&shooter];
    crate::combat::Impact {
        owner: u.owner as i32,
        shooter,
        weapon: crate::combat::WeaponDamage::from_catalog(&m.rules.kinds[u.kind].name).unwrap(),
        damage_scale: None,
        energy: 1.0,
        armor_piercing: false,
        ap_naval_only: false,
        ap_multiplier: 1.5,
        launch_origin: u.at(),
        at: m.world.game.units[&target].at(),
        direction: Point { x: 1.0, z: 0.0 },
    }
}
fn source(m: &Match, shooter: u32) -> crate::match_combat::FlightSource {
    let u = &m.world.game.units[&shooter];
    crate::match_combat::FlightSource {
        shooter,
        kind: u.kind,
        owner: u.owner as i32,
        airborne: false,
    }
}
fn direct(m: &mut Match, target: u32, p: crate::combat::Impact) -> f64 {
    let s = source(m, p.shooter);
    let order = m.world.game.units.keys().copied().collect::<Vec<_>>();
    let (mut d, t) = m.firing_damage_view(&order);
    d.direct_hit(s, target, p, t).unwrap()
}
fn profile() -> ExplosionSource {
    ExplosionSource {
        infantry: 40.0,
        vehicle: 60.0,
        terrain: None,
        suppression: 50.0,
        scar: 0.0,
        anti_garrison: None,
        weapon_kind: None,
        off_map_support: false,
    }
}
fn blast(m: &mut Match, at: Point, radius: f64, p: ExplosionSource) {
    let order = m.world.game.units.keys().copied().collect::<Vec<_>>();
    let (mut d, t) = m.firing_damage_view(&order);
    d.explosion(at, radius, p, 0, None, None, None, t).unwrap();
}
fn building(m: &mut Match, id: u32, cells: &[usize]) {
    let u = m.world.game.units.get_mut(&id).unwrap();
    u.cells = Some(cells.iter().map(|c| *c as u32).collect());
    let hp = u.hp;
    m.world
        .register_building_structure(id, cells, hp, hp)
        .unwrap();
}

#[test]
fn cmb38_finished_pillbox_fires_and_sections_obey_slit_facing() {
    let mut m = scene(&[("pillbox", 0, 30.0, 30.0), ("rifle", 1, 45.0, 30.0)]);
    m.world.game.units.get_mut(&1).unwrap().built = Some(0.99);
    launch(&mut m, 1, 2);
    assert!(m.firing.flights.is_empty());
    m.world.game.units.get_mut(&1).unwrap().built = Some(1.0);
    launch(&mut m, 1, 2);
    assert_eq!(m.firing.flights.len(), 1);
    // Actual structural contact, with the same small-arms and rear tests for each weapon.
    for (weapon, front, rear) in [("rifle", 0.15, 0.3), ("at", 120.0, 240.0)] {
        for (direction, expected) in [(-1.0, front), (1.0, rear)] {
            let mut m = scene(&[(weapon, 0, 20.0, 30.0), ("pillbox", 1, 30.0, 30.0)]);
            let c = m.world.game.cell_of(30.0, 30.0).unwrap();
            let cells = [c, c + 1, c + m.world.game.w, c + m.world.game.w + 1];
            building(&mut m, 2, &cells);
            m.world.game.units.get_mut(&2).unwrap().rot = 0.0;
            let before = m.world.game.units[&2].hp;
            let mut p = impact(&m, 1, 2);
            p.direction.x = direction;
            let s = source(&m, 1);
            let (mut d, t) = m.firing_damage_view(&[1, 2]);
            d.local_contact(
                s,
                crate::match_combat::ContactTarget::Cell(c),
                p,
                "pillbox:1",
                None,
                t,
            )
            .unwrap();
            close(before - m.world.game.units[&2].hp, expected);
            close(m.combat.get(1).unwrap().xp, expected);
        }
    }
}

#[test]
fn cmb51_arrival_uses_current_hull_facing_and_cannot_credit_twice() {
    for (kind, rotation, expected) in [
        ("tank", 0.0, 240.0),
        ("churchill", std::f64::consts::PI, 72.0),
    ] {
        let mut m = scene(&[("at", 0, 20.0, 20.0), (kind, 1, 30.0, 20.0)]);
        m.world.game.units.get_mut(&2).unwrap().rot = 1.5;
        let seed = (0..1000)
            .find(|seed| {
                crate::random::MatchRandom::new(*seed)
                    .next_fraction(crate::random::StreamId::Combat)
                    < 0.2
            })
            .unwrap();
        m.start.random = crate::random::MatchRandom::new(seed);
        m.fire_weapon(1, 2, false).unwrap();
        m.world.game.units.get_mut(&2).unwrap().rot = rotation;
        let before = m.world.game.units[&2].hp;
        flights(&mut m, 8);
        close(before - m.world.game.units[&2].hp, expected);
        close(m.combat.get(1).unwrap().xp, expected);
        assert_eq!(m.combat.get(2).unwrap().last_hit, Some(0));
        assert_eq!(m.combat.get(2).unwrap().hit_by, Some(1));
        assert!(m.combat.get(2).unwrap().at_hit.is_some());
        assert_eq!(
            m.world.game.units[&2].hit_from,
            Some(Point { x: 20.0, z: 20.0 })
        );
        assert!(m.combat.get(2).unwrap().death_impulse.is_some());
        flights(&mut m, 8);
        close(m.combat.get(1).unwrap().xp, expected);
    }
}

#[test]
fn cmb51_direct_damage_ap_flame_retreat_building_and_tech_rules() {
    // Scale and energy precede AP and target armor. Naval-only AP ignores ground armor.
    for (naval_only, expected) in [(false, 180.0), (true, 120.0)] {
        let mut m = scene(&[("at", 0, 20.0, 20.0), ("tank", 1, 30.0, 20.0)]);
        m.world.game.units.get_mut(&2).unwrap().rot = std::f64::consts::FRAC_PI_2;
        let mut p = impact(&m, 1, 2);
        p.damage_scale = Some(2.0);
        p.energy = 0.5;
        p.armor_piercing = true;
        p.ap_naval_only = naval_only;
        close(direct(&mut m, 2, p), expected);
    }
    for (trench, garrison, retreat, expected) in [
        (false, false, false, 9.0),
        (true, false, false, 13.5),
        (false, true, false, 13.5),
        (true, false, true, 3.375),
    ] {
        let mut m = scene(&[("flamer", 0, 20.0, 20.0), ("rifle", 1, 30.0, 20.0)]);
        let c = m.world.game.cell_of(30.0, 20.0).unwrap();
        if trench {
            m.world.set_cell(c, b'T').unwrap();
        }
        let u = m.world.game.units.get_mut(&2).unwrap();
        u.garrison = garrison.then_some(c);
        u.retreating = retreat;
        let p = impact(&m, 1, 2);
        close(direct(&mut m, 2, p), expected);
    }
    let mut m = scene(&[("at", 0, 20.0, 20.0), ("tank", 1, 30.0, 20.0)]);
    m.options.tech = true;
    m.production = crate::production::ProductionState::new(vec![
        crate::economy::TechState::new(1, 2, 0, 0).unwrap(),
        crate::economy::TechState::new(1, 0, 0, 1).unwrap(),
    ])
    .unwrap();
    m.world.game.units.get_mut(&2).unwrap().rot = std::f64::consts::FRAC_PI_2;
    let p = impact(&m, 1, 2);
    close(direct(&mut m, 2, p), 120.0 * 1.2 * 0.9);
    // Ordinary buildings use the anti-building budget even if their entity is hit directly.
    for (weapon, expected) in [("rifle", 0.75), ("at", 120.0)] {
        let mut m = scene(&[(weapon, 0, 20.0, 20.0), ("hq", 1, 30.0, 20.0)]);
        let p = impact(&m, 1, 2);
        close(direct(&mut m, 2, p), expected);
    }
}

#[test]
fn cmb53_solid_contact_and_terminal_shell_damage_terrain_and_credit_building_xp() {
    // A local contact applies only quarter-strength contact wear.
    let mut m = scene(&[("tank", 0, 20.0, 20.0), ("rifle", 1, 35.0, 20.0)]);
    let cell = m.world.game.cell_of(29.0, 21.0).unwrap();
    m.world.set_cell(cell, b'#').unwrap();
    let before = m.world.game.cell_hp[cell];
    let p = impact(&m, 1, 2);
    let s = source(&m, 1);
    let (mut d, t) = m.firing_damage_view(&[1, 2]);
    d.local_contact(
        s,
        crate::match_combat::ContactTarget::Cell(cell),
        p,
        "wear:1",
        None,
        t,
    )
    .unwrap();
    close(f64::from(before - m.world.game.cell_hp[cell]), 22.5);
    close(m.combat.get(1).unwrap().xp, 0.0);

    // An actual terminal flight adds its 1.5 m shell demolition pass and building XP.
    let mut m = scene(&[("tank", 0, 20.0, 20.0), ("pillbox", 1, 29.0, 21.0)]);
    let cell = m.world.game.cell_of(29.0, 21.0).unwrap();
    building(&mut m, 2, &[cell]);
    m.world.game.units.get_mut(&2).unwrap().rot = std::f64::consts::PI;
    m.fire_weapon(1, 2, false).unwrap();
    let id = *m.firing.flights.keys().next().unwrap();
    let mut f = m.firing.flights.remove(&id).unwrap();
    let before = m.world.game.units[&2].hp;
    let contact = FlightContact {
        surface: FlightSurface::Cell(cell),
        hit: projectiles::ContactHit {
            fraction: 0.5,
            normal: projectiles::Point3::new(-1.0, 0.0, 0.0),
        },
    };
    assert!(
        m.resolve_live_flight(
            &mut f,
            Some(contact),
            projectiles::Point3::new(29.0, 1.0, 21.0),
            5.1,
            projectiles::Point3::new(150.0, 0.0, 0.0)
        )
        .unwrap()
    );
    close(before - m.world.game.units[&2].hp, 45.0 + 90.0);
    close(m.combat.get(1).unwrap().xp, 135.0);
}

#[test]
fn cmb56_blast_friendly_fire_cover_veterancy_and_retreat_apply_at_arrival() {
    let mut m = scene(&[
        ("rifle", 0, 40.0, 40.0),
        ("rifle", 1, 42.0, 40.0),
        ("rifle", 1, 40.0, 42.0),
        ("rifle", 1, 38.0, 40.0),
        ("rifle", 1, 40.0, 38.0),
        ("rifle", 1, 39.0, 40.0),
    ]);
    let trench = m.world.game.cell_of(40.0, 42.0).unwrap();
    m.world.set_cell(trench, b'T').unwrap();
    m.world.game.units.get_mut(&4).unwrap().retreating = true;
    m.world.game.units.get_mut(&5).unwrap().garrison = Some(trench);
    m.combat.get_mut(6).unwrap().xp = 250.0;
    let mut p = profile();
    p.anti_garrison = Some(1.5);
    blast(&mut m, Point { x: 40.0, z: 40.0 }, 4.0, p);
    for (id, damage) in [
        (1, 40.0),
        (2, 30.0),
        (3, 15.0),
        (4, 7.5),
        (5, 45.0),
        (6, 35.0 * 0.84),
    ] {
        close(m.world.game.units[&id].hp, 100.0 - damage);
        assert_eq!(m.combat.get(id).unwrap().last_hit, Some(0));
        assert!(m.combat.get(id).unwrap().death_impulse.is_some());
    }
    close(m.world.game.units[&6].supp, 35.0);
    // Suppression uses veterancy without distance or retreat reduction.
    close(m.world.game.units[&4].supp, 50.0);
    close(m.world.game.units[&1].supp, 50.0);
    close(m.combat.get(1).unwrap().xp, 0.0);
}

#[test]
fn cmb56_blast_skips_air_and_carried_squads_but_hits_deck_riders() {
    let mut m = scene(&[
        ("rifle", 0, 40.0, 40.0),
        ("rifle", 0, 40.0, 40.0),
        ("rifle", 0, 40.0, 40.0),
        ("tank", 0, 50.0, 40.0),
        ("halftrack", 0, 50.0, 40.0),
    ]);
    // An airborne physical row is excluded before its weapon or sortie state matters.
    m.world.game.units.get_mut(&1).unwrap().air = Some(crate::game::PlaneState::Out);
    for (rider, carrier) in [(2, 4), (3, 5)] {
        m.world.game.units.get_mut(&rider).unwrap().riding = true;
        m.combat.get_mut(rider).unwrap().carrier = Some(carrier);
        m.combat.get_mut(carrier).unwrap().cargo.push(rider);
    }
    blast(&mut m, Point { x: 40.0, z: 40.0 }, 4.0, profile());
    close(m.world.game.units[&1].hp, 100.0);
    close(m.world.game.units[&2].hp, 60.0);
    close(m.world.game.units[&3].hp, 100.0);
    close(m.world.game.units[&3].supp, 0.0);
}

#[test]
fn cmb56_building_blast_selects_nearest_section_and_terrain_is_not_double_charged() {
    for terrain in [None, Some(60.0)] {
        let mut m = scene(&[("rifle", 0, 20.0, 20.0), ("pillbox", 1, 31.0, 31.0)]);
        let cell = m.world.game.cell_of(29.0, 31.0).unwrap();
        let cells = [cell, cell + 1];
        building(&mut m, 2, &cells);
        let before = m.world.game.units[&2].hp;
        let section_before = m.world.game.sections[&(cell as u32)].hp;
        let other_before = m.world.game.sections[&((cell + 1) as u32)].hp;
        let mut p = profile();
        p.terrain = terrain;
        blast(&mut m, Point { x: 29.0, z: 31.0 }, 1.5, p);
        // Entity center is outside this radius, but terrain still reaches the near section.
        let expected = if terrain.is_some() { 60.0 } else { 0.0 };
        close(before - m.world.game.units[&2].hp, expected);
        close(
            section_before - m.world.game.sections[&(cell as u32)].hp,
            expected,
        );
        close(m.world.game.sections[&((cell + 1) as u32)].hp, other_before);
    }
    let mut m = scene(&[("rifle", 0, 20.0, 20.0), ("pillbox", 1, 31.0, 31.0)]);
    let c = m.world.game.cell_of(29.0, 31.0).unwrap();
    building(&mut m, 2, &[c, c + 1]);
    let near = m.world.game.sections[&(c as u32)].hp;
    let far = m.world.game.sections[&((c + 1) as u32)].hp;
    blast(&mut m, Point { x: 29.0, z: 31.0 }, 4.0, profile());
    close(near - m.world.game.sections[&(c as u32)].hp, 45.0);
    close(m.world.game.sections[&((c + 1) as u32)].hp, far);
}

#[test]
fn cmb60_bunker_separates_direct_offmap_and_ordinary_blast_damage() {
    let mut m = scene(&[("tank", 0, 20.0, 20.0), ("bunker", 1, 30.0, 20.0)]);
    let p = impact(&m, 1, 2);
    close(direct(&mut m, 2, p), 11.25);
    let before = m.world.game.units[&2].hp;
    let support = ExplosionSource::support("dive").unwrap();
    let (mut d, t) = m.firing_damage_view(&[1, 2]);
    d.hurt(2, support, 1.0, 0, t).unwrap();
    close(before - m.world.game.units[&2].hp, 25.0);
    let before = m.world.game.units[&2].hp;
    let weapon = ExplosionSource::weapon(m.rules.kind("howitzer").unwrap(), &m.rules).unwrap();
    let (mut d, t) = m.firing_damage_view(&[1, 2]);
    d.hurt(2, weapon, 1.0, 0, t).unwrap();
    close(before - m.world.game.units[&2].hp, 150.0);
}

#[test]
fn cmb63_damage_caps_xp_at_remaining_health_and_pays_kill_bonus_once() {
    let mut m = scene(&[("at", 0, 20.0, 20.0), ("rifle", 1, 30.0, 20.0)]);
    m.world.game.units.get_mut(&2).unwrap().hp = 1.0;
    let p = impact(&m, 1, 2);
    assert!(direct(&mut m, 2, p) > 1.0);
    close(m.combat.get(1).unwrap().xp, 21.0);
    close(direct(&mut m, 2, p), 0.0);
    close(m.combat.get(1).unwrap().xp, 21.0);
    // XP thresholds use the recipient's own cost, with inclusive boundary values.
    for (xp, stars) in [
        (99.0, 0),
        (100.0, 1),
        (249.0, 1),
        (250.0, 2),
        (499.0, 2),
        (500.0, 3),
    ] {
        assert_eq!(crate::combat::veterancy("rifle", xp).unwrap(), stars);
        let mut m = scene(&[("tank", 0, 20.0, 20.0), ("rifle", 1, 30.0, 20.0)]);
        m.combat.get_mut(2).unwrap().xp = xp;
        let p = impact(&m, 1, 2);
        close(direct(&mut m, 2, p), 30.0 * (1.0 - 0.08 * f64::from(stars)));
        let (mut d, t) = m.firing_damage_view(&[1, 2]);
        d.hurt(2, profile(), 1.0, 0, t).unwrap();
        close(
            m.world.game.units[&2].supp,
            50.0 * (1.0 - 0.15 * f64::from(stars)),
        );
    }
    assert_eq!(crate::combat::veterancy("bunker", 10_000.0).unwrap(), 0);
}

#[test]
fn cmb63_star_accuracy_changes_actual_launch_with_the_same_random_draw() {
    // MG accuracy is 0.5. Choose a fixed seed whose first draw lies between the
    // zero-star and one-star thresholds, then compare actual launch trajectories.
    let seed = (0..1000)
        .find(|seed| {
            let draw = crate::random::MatchRandom::new(*seed)
                .next_fraction(crate::random::StreamId::Combat);
            (0.5..0.55).contains(&draw)
        })
        .unwrap();
    let mut velocities = Vec::new();
    for xp in [0.0, 150.0] {
        let mut m = scene(&[("mg", 0, 20.0, 20.0), ("rifle", 1, 30.0, 20.0)]);
        m.start.random = crate::random::MatchRandom::new(seed);
        m.combat.get_mut(1).unwrap().xp = xp;
        m.fire_weapon(1, 2, false).unwrap();
        velocities.push(m.firing.flights.values().next().unwrap().solution.velocity);
    }
    assert_ne!(velocities[0].z, 0.0, "zero-star round misses sideways");
    close(velocities[1].z, 0.0);
}

#[test]
fn cmb38_build_command_defaults_slit_away_from_spawn_and_preserves_ordered_direction() {
    for direction in [None, Some(1.25)] {
        let mut m = scene(&[("engineer", 0, 26.0, 30.0)]);
        m.start.economy.players[0].mp = 5000.0;
        let id = m.next_unit_id;
        assert_eq!(
            m.execute_build(
                0,
                &[1],
                "pillbox",
                Some(Point { x: 32.0, z: 30.0 }),
                direction,
                true
            )
            .unwrap(),
            None
        );
        let u = &m.world.game.units[&id];
        let spawn = m.start.players[0].spawn;
        close(
            u.rot,
            direction.unwrap_or_else(|| math::atan2(u.z - spawn.z, u.x - spawn.x)),
        );
        assert_eq!(u.cells.as_ref().unwrap().len(), 4);
        assert_eq!(u.built, Some(0.0));
        close(u.hp, 375.0);
    }
}

#[test]
fn cmb51_torpedo_ap_applies_only_to_naval_targets_and_air_hits_do_not_set_vehicle_memory() {
    for (target, naval, expected) in [("tank", false, 12.0), ("gunboat", true, 300.0)] {
        let mut m = scene(&[("gunboat", 0, 20.0, 20.0), (target, 1, 30.0, 20.0)]);
        m.world.game.units.get_mut(&2).unwrap().rot = std::f64::consts::FRAC_PI_2;
        let mut p = impact(&m, 1, 2);
        p.armor_piercing = true;
        p.ap_naval_only = true;
        p.ap_multiplier = 25.0;
        close(direct(&mut m, 2, p), expected);
        assert_eq!(m.rules.kinds[m.world.game.units[&2].kind].naval, naval);
    }
    for (target, airborne, remembers) in [
        ("tank", true, false),
        ("tank", false, true),
        ("rifle", true, true),
    ] {
        let mut m = scene(&[("at", 0, 20.0, 20.0), (target, 1, 30.0, 20.0)]);
        let p = impact(&m, 1, 2);
        let mut s = source(&m, 1);
        s.airborne = airborne;
        let (mut d, t) = m.firing_damage_view(&[1, 2]);
        d.direct_hit(s, 2, p, t).unwrap();
        assert_eq!(m.combat.get(2).unwrap().hit_by == Some(1), remembers);
        assert_eq!(m.combat.get(2).unwrap().at_hit.is_some(), target == "tank");
    }
}
