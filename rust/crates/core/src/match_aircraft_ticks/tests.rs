use super::*;
use crate::{
    catalog::Catalog,
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::Point,
    map::AuthoritativeMap,
    match_aviation::{AircraftRuntime, MissionKind, PlaneMission},
    match_commands::UnitControl,
    match_runtime::{MatchOptions, PlayerConfig},
    unit_spawn, vision,
    weather::WeatherChoice,
    world::World,
};
use serde_json::Value as Json;

fn arena() -> AuthoritativeMap {
    let f: Json = serde_json::from_str(include_str!(
        "../../../simtest/tests/fixtures/base-pending-paratroopers.json"
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
pub(crate) fn scene(units: &[(&str, usize, f64, f64)]) -> Match {
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

fn add_plane(m: &mut Match, name: &str, mission: PlaneMission) -> UnitId {
    let id = m.next_unit_id;
    m.next_unit_id += 1;
    let kind = m.rules.kind(name).unwrap();
    let mut u = unit_spawn::initial_aircraft(
        &m.rules,
        Catalog::compiled().unwrap(),
        kind,
        id,
        0,
        Point { x: 30.0, z: 40.0 },
    )
    .unwrap();
    u.air = Some(PlaneState::Out);
    m.commands
        .insert(id, UnitControl::new(&u, &m.rules).unwrap());
    m.commands.units.get_mut(&id).unwrap().hold_fire = false;
    m.firing.initialize(id);
    m.combat.initialize(id);
    m.abilities.initialize(id);
    m.recovery.initialize(id);
    let ammo = Catalog::compiled()
        .unwrap()
        .unit(name)
        .unwrap()
        .get("ammo")
        .unwrap()
        .number()
        .unwrap();
    m.aviation.planes.insert(
        id,
        AircraftRuntime {
            fuel: 50.0,
            ammo,
            timer: 0.0,
            orbit: 0.0,
            mission: Some(mission),
            attack_pick: 0,
            downed: false,
        },
    );
    m.world.game.units.insert(id, u);
    sight(m);
    id
}

#[test]
fn cmb67_aircraft_fire_after_moving_prioritize_orders_and_obey_hold_fire() {
    let mut m = scene(&[("rifle", 1, 40.0, 40.0), ("tank", 1, 43.0, 40.0)]);
    let patrol = PlaneMission {
        kind: MissionKind::Patrol,
        target: None,
        at: Point { x: 40.0, z: 40.0 },
    };
    let id = add_plane(&mut m, "attacker", patrol);
    let before = m.world.game.units[&id].at();
    assert!(m.step_aircraft_turn(id, 0.05).unwrap());
    assert_ne!(m.world.game.units[&id].at(), before);
    assert_eq!(m.aviation.planes[&id].ammo, 7.0);
    assert!(!m.firing.flights.is_empty());
    assert!(
        m.firing
            .flights
            .values()
            .all(|p| p.solution.origin.y == 20.0)
    );
    assert_eq!(m.firing.get(id).unwrap().retarget, 0.5);
    let actors = m.world.game.units.keys().copied().collect::<Vec<_>>();
    m.begin_firing_tick(&actors).unwrap();
    let claims = m.firing.target_claims().collect::<Vec<_>>();
    assert!(!claims.is_empty());
    m.step_aircraft_turn(id, 0.05).unwrap();
    assert_eq!(
        m.firing.target_claims().collect::<Vec<_>>(),
        claims,
        "aircraft target assignment must not add a second ground volley claim"
    );

    let hold = crate::match_commands::Command::from_json(
        &serde_json::json!({"t":"stance", "ids":[id], "key":"holdFire", "on":true}),
    )
    .unwrap();
    assert_eq!(m.execute_command(0, &hold, false).unwrap(), None);
    let attack = crate::match_commands::Command::from_json(
        &serde_json::json!({"t":"attack", "ids":[id], "target":1}),
    )
    .unwrap();
    assert_eq!(m.execute_command(0, &attack, false).unwrap(), None);
    m.firing.get_mut(id).unwrap().cooldown = 0.0;
    m.step_aircraft_turn(id, 0.05).unwrap();
    assert_eq!(m.firing.get(id).unwrap().target_id, 1);
    assert_eq!(m.aviation.planes[&id].ammo, 6.0);
    let flights = m.firing.flights.len();

    assert!(m.send_aircraft_mission(0, id, patrol).unwrap());
    m.firing.get_mut(id).unwrap().cooldown = 0.0;
    m.step_aircraft_turn(id, 0.05).unwrap();
    assert_eq!(m.aviation.planes[&id].ammo, 6.0);
    assert_eq!(m.firing.flights.len(), flights);
    assert_eq!(m.firing.get(id).unwrap().target_id, 0);
    let resume = crate::match_commands::Command::from_json(
        &serde_json::json!({"t":"stance", "ids":[id], "key":"holdFire", "on":false}),
    )
    .unwrap();
    assert_eq!(m.execute_command(0, &resume, false).unwrap(), None);
    m.firing.get_mut(id).unwrap().retarget = 0.0;
    m.step_aircraft_turn(id, 0.05).unwrap();
    assert_eq!(m.aviation.planes[&id].ammo, 5.0);
    // A cached automatic target leaving range does not trigger a fresh pick until the half-second beat.
    let cached = m.aviation.planes[&id].attack_pick;
    m.world.game.units.get_mut(&cached).unwrap().x = 130.0;
    sight(&mut m);
    m.firing.get_mut(id).unwrap().cooldown = 0.0;
    m.step_aircraft_turn(id, 0.05).unwrap();
    assert_eq!(m.aviation.planes[&id].ammo, 5.0);
    assert_eq!(m.firing.get(id).unwrap().target_id, 0);
    assert_eq!(m.aviation.planes[&id].attack_pick, cached);
    assert!(m.firing.get(id).unwrap().retarget > 0.0);
    for _ in 0..10 {
        m.step_aircraft_turn(id, 0.05).unwrap();
    }
    assert_eq!(m.aviation.planes[&id].ammo, 4.0);
    assert_ne!(m.firing.get(id).unwrap().target_id, cached);
}

#[test]
fn cmb67_area_bomber_spends_one_sortie_round_for_each_physical_stick() {
    let mut m = scene(&[("rifle", 1, 40.0, 40.0)]);
    let at = Point { x: 40.0, z: 40.0 };
    let mission = PlaneMission {
        kind: MissionKind::Area,
        target: None,
        at,
    };
    let id = add_plane(&mut m, "bomber", mission);
    m.commands.units.get_mut(&id).unwrap().hold_fire = true;
    // The ordered point authorizes bombing even with Hold Fire and without an entity target.
    assert!(m.send_aircraft_mission(0, id, mission).unwrap());
    m.step_aircraft_turn(id, 0.05).unwrap();
    assert_eq!(m.world.game.units[&id].air, Some(PlaneState::Station));
    assert_eq!(m.aviation.planes[&id].ammo, 1.0);
    assert_eq!(m.firing.flights.len(), 4);
    assert!(
        m.firing
            .flights
            .values()
            .all(|p| p.solution.origin.y == 20.0)
    );
    assert_eq!(m.firing.salvos.last().unwrap().at, at);
    m.step_aircraft_turn(id, 0.05).unwrap();
    assert_eq!(
        m.firing.flights.len(),
        4,
        "cooldown prevents a second stick"
    );
    m.firing.get_mut(id).unwrap().cooldown = 0.0;
    m.step_aircraft_turn(id, 0.05).unwrap();
    assert_eq!(m.aviation.planes[&id].ammo, 0.0);
    assert_eq!(m.firing.flights.len(), 8);
    m.step_aircraft_turn(id, 0.05).unwrap();
    assert_eq!(m.world.game.units[&id].air, Some(PlaneState::Home));
    assert_eq!(m.firing.flights.len(), 8);
}

#[test]
fn aircraft_launch_events_detach_before_later_ground_queued_order() {
    for name in ["attacker", "bomber"] {
        let mut m = ordinary();
        for control in m.commands.units.values_mut() {
            control.hold_fire = true;
            control.auto_cast = false;
            control.auto_retreat = false;
        }
        let target = m
            .world
            .game
            .units
            .values()
            .find(|unit| unit.owner == 1 && m.rules.kinds[unit.kind].name == "rifle")
            .unwrap()
            .id;
        let enemy = m.world.game.units.get_mut(&target).unwrap();
        enemy.x = 40.0;
        enemy.z = 40.0;
        let plane = add_plane(
            &mut m,
            name,
            PlaneMission {
                kind: if name == "bomber" {
                    MissionKind::Area
                } else {
                    MissionKind::Attack
                },
                target: (name == "attacker").then_some(target),
                at: Point { x: 40.0, z: 40.0 },
            },
        );
        let ground = m.next_unit_id;
        m.next_unit_id += 1;
        let unit = unit_spawn::initial_unit(
            &m.rules,
            Catalog::compiled().unwrap(),
            m.rules.kind("rifle").unwrap(),
            ground,
            0,
            Point { x: 70.0, z: 70.0 },
        )
        .unwrap();
        m.commands
            .insert(ground, UnitControl::new(&unit, &m.rules).unwrap());
        m.firing.initialize(ground);
        m.combat.initialize(ground);
        m.abilities.initialize(ground);
        m.recovery.initialize(ground);
        m.world.game.units.insert(ground, unit);
        sight(&mut m);
        m.initialize_tick_scene(100, Default::default()).unwrap();
        let queued = crate::match_commands::Command::from_json(&serde_json::json!({
            "t": "move", "orders": [[ground, 80.0, 70.0]],
            "queue": true
        }))
        .unwrap();
        assert_eq!(m.execute_command(0, &queued, false).unwrap(), None);
        assert_eq!(m.commands.units[&ground].orders.len(), 1);
        let report = m.advance_tick().unwrap();
        assert!(report.events.iter().any(|event| matches!(
            event,
            crate::match_tick::TickEvent::Firing(crate::match_firing::LaunchEvent::Flight { shooter, .. })
                if *shooter == plane
        )));
        assert!(m.commands.units[&ground].orders.is_empty());
        assert!(
            m.world.game.units[&ground]
                .world_goal
                .is_some_and(|goal| goal.x > 70.0)
        );
        for expected in 102..=104 {
            assert_eq!(m.advance_tick().unwrap().tick, expected);
        }
        assert_eq!(m.tick_state.fault(), None);
    }
}

#[test]
fn cmb68_enemy_hit_on_a_tank_ejects_all_deck_riders_with_shock() {
    let mut m = scene(&[
        ("rifle", 1, 20.0, 20.0),
        ("tank", 0, 30.0, 20.0),
        ("rifle", 0, 30.0, 20.0),
        ("rifle", 0, 30.0, 20.0),
    ]);
    m.combat.get_mut(2).unwrap().cargo = vec![3, 4];
    for id in [3, 4] {
        m.combat.get_mut(id).unwrap().carrier = Some(2);
        m.world.game.units.get_mut(&id).unwrap().riding = true;
    }
    sight(&mut m);
    for rider in [3, 4] {
        assert!(!m.world.game.players[1].sees(rider));
        assert!(!m.can_shoot_target(1, rider).unwrap());
    }
    let kind = m.world.game.units[&1].kind;
    let from = Point { x: 20.0, z: 20.0 };
    let at = Point { x: 30.0, z: 20.0 };
    let impact = crate::combat::Impact {
        owner: 1,
        shooter: 1,
        weapon: crate::combat::WeaponDamage::from_catalog("rifle").unwrap(),
        damage_scale: None,
        energy: 1.0,
        armor_piercing: false,
        ap_naval_only: false,
        ap_multiplier: 1.5,
        launch_origin: from,
        at,
        direction: Point { x: 1.0, z: 0.0 },
    };
    let mut damage = crate::match_combat::DamageWorld {
        world: &mut m.world,
        combat: &mut m.combat,
        grid: &mut m.grid,
        rules: &m.rules,
        logistics: &m.logistics,
        production: &m.production,
        mode: m.options.mode,
        tech_enabled: m.options.tech,
        environment: &mut m.environment,
        environment_seed: &mut m.start.environment_seed,
        next_id: &mut m.next_unit_id,
        unit_order: &[1, 2, 3, 4],
        wind_angle: m.start.wind.0,
        wind_strength: m.start.wind.1,
    };
    damage
        .direct_hit(
            crate::match_combat::FlightSource {
                shooter: 1,
                kind,
                owner: 1,
                airborne: false,
            },
            2,
            impact,
            &mut m.firing,
        )
        .unwrap();
    for id in [3, 4] {
        assert!(!m.world.game.units[&id].riding);
        assert!(m.world.game.units[&id].supp >= 60.0);
        assert_eq!(m.combat.get(id).unwrap().carrier, None);
    }
    assert!(m.combat.get(2).unwrap().cargo.is_empty());
}

#[test]
fn cmb68_wrecked_carriers_hurt_survivors_and_drown_boat_passengers_far_from_land() {
    for (name, water, survives) in [
        ("halftrack", 0, true),
        ("lcvp", 1, true),
        ("lcvp", 4, false),
    ] {
        let mut m = scene(&[(name, 0, 41.0, 41.0), ("rifle", 0, 41.0, 41.0)]);
        // Author the water around the passengers. The narrow bank is within five metres.
        if water > 0 {
            for z in (20 - water)..=(20 + water) {
                for x in (20 - water)..=(20 + water) {
                    let cell = z * m.world.game.w + x;
                    m.world.set_cell(cell, b'W').unwrap();
                }
            }
        }
        m.combat.get_mut(1).unwrap().cargo.push(2);
        m.combat.get_mut(2).unwrap().carrier = Some(1);
        m.world.game.units.get_mut(&2).unwrap().riding = true;
        let hp = m.world.game.units[&2].hp;
        m.world.game.units.get_mut(&1).unwrap().hp = 0.0;
        m.validate_carrier_tick().unwrap();
        let retired = m.retire_dead(&[1, 2]).unwrap();
        assert_eq!(m.combat.get(2).unwrap().carrier, None);
        assert!(m.combat.get(1).unwrap().cargo.is_empty());
        if survives {
            assert_eq!(retired.retired.len(), 1);
            let u = &m.world.game.units[&2];
            assert!(!u.riding);
            assert!((u.hp - hp * 0.7).abs() < 1e-9);
            assert_eq!(m.world.game.flags_at(u.x, u.z) & crate::game::MOVE, 0);
        } else {
            assert_eq!(retired.retired.len(), 2);
            assert!(!m.world.game.units.contains_key(&2));
        }
        assert!(retired.shots.iter().any(|effect| matches!(effect, crate::environment::Effect::Hurt(h) if h.unit == 2 && h.killed != survives)));
    }
}

#[test]
fn cmb63_aircraft_aa_experience_caps_overkill_without_a_kill_cost_bonus() {
    let mut m = scene(&[("flak", 1, 30.0, 40.0)]);
    let id = add_plane(
        &mut m,
        "attacker",
        PlaneMission {
            kind: MissionKind::Patrol,
            target: None,
            at: Point { x: 40.0, z: 40.0 },
        },
    );
    m.world.game.units.get_mut(&id).unwrap().hp = 0.25;
    m.commands.units.get_mut(&1).unwrap().hold_fire = false;
    m.step_aircraft_anti_air(&[1, id], 0.05).unwrap();
    assert!(m.world.game.units[&id].hp <= 0.0);
    assert_eq!(m.combat.get(1).unwrap().xp, 0.25);
    assert_eq!(m.story[1].planes_downed, 1.0);
}

#[test]
fn cmb44_aircraft_ground_weapons_skip_ground_line_of_sight() {
    let mut m = scene(&[("rifle", 1, 40.0, 40.0)]);
    let id = add_plane(
        &mut m,
        "attacker",
        PlaneMission {
            kind: MissionKind::Attack,
            target: Some(1),
            at: Point { x: 40.0, z: 40.0 },
        },
    );
    for z in 0..m.world.game.h {
        m.world.set_cell(z * m.world.game.w + 17, b'H').unwrap();
    }
    sight(&mut m);
    assert!(!vision::los(
        &m.world.game,
        &m.rules,
        (30.0, 40.0),
        (40.0, 40.0)
    ));
    assert!(m.can_shoot_target(id, 1).unwrap());
    m.step_aircraft_turn(id, 0.05).unwrap();
    assert_eq!(m.firing.get(id).unwrap().target_id, 1);
    assert_eq!(m.aviation.planes[&id].ammo, 7.0);
}

#[test]
fn cmb78_recalled_aircraft_fly_home_and_start_rearming() {
    let mut m = scene(&[]);
    let id = add_plane(
        &mut m,
        "attacker",
        PlaneMission {
            kind: MissionKind::Patrol,
            target: None,
            at: Point { x: 40.0, z: 40.0 },
        },
    );
    let home = m.aircraft_base(0, m.world.game.units[&id].at()).unwrap();
    assert!(m.return_aircraft_home(0, id).unwrap());
    assert_eq!(m.world.game.units[&id].air, Some(PlaneState::Home));
    m.step_aircraft(id, 30.0).unwrap();
    let u = &m.world.game.units[&id];
    assert!((u.x - home.x).abs() < 1e-9 && (u.z - home.z).abs() < 1e-9);
    assert_eq!(u.air, Some(PlaneState::Rearm));
    assert_eq!(m.aviation.planes[&id].mission, None);
    assert_eq!(m.aviation.planes[&id].timer, 30.0);
}
