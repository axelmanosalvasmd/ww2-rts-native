//! Horde combat component checks. Hosted room admission remains separate.
use super::*;
use crate::{
    combat::{Impact, WeaponDamage},
    economy::{Faction, Mode, TechState},
    economy_state::ArmySize,
    game::ModeKind,
    map::AuthoritativeMap,
    match_combat::FlightSource,
    match_commands::{MatchCommands, UnitControl},
    match_runtime::{MatchOptions, PlayerConfig},
    recipient_snapshot::SnapshotCache,
    unit_spawn::initial_unit,
    weather::WeatherChoice,
    world::{CellChange, World},
};
use serde_json::json;

fn scene(mode: Mode, defenders: usize) -> Match {
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"Kaiju combat", "w":60,"h":60,"rows":vec![".".repeat(60);60],
            "spawns":[{"x":30,"y":5},{"x":10,"y":54},{"x":50,"y":54}],
            "defend":[0],"points":[{"x":30,"y":30}]
        }))
        .unwrap(),
    )
    .unwrap();
    let options = MatchOptions {
        seed: 701,
        mode,
        players: (0..defenders)
            .map(|slot| PlayerConfig {
                name: format!("P{slot}"),
                team: slot as i32,
                faction: Faction::Usa,
            })
            .collect(),
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
    };
    if mode == Mode::Horde {
        Match::new_horde(&map, options).unwrap()
    } else {
        Match::new(&map, options).unwrap()
    }
}
fn combat_scene(defenders: usize) -> Match {
    let mut m = scene(Mode::Horde, defenders);
    let players = m.world.game.players.clone();
    let source = json!({"name":"Beam ground","w":60,"h":60,"rows":vec![".".repeat(60);60],
        "spawns":[{"x":5,"y":5},{"x":54,"y":54}],"points":[{"x":30,"y":30}]});
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&source).unwrap()).unwrap();
    m.world = World::from_map(&map).unwrap();
    m.world.game.players = players;
    m.world.game.mode = Some(ModeKind::Horde);
    m.commands = MatchCommands::new(4096);
    m.combat = Default::default();
    m.firing = Default::default();
    m.logistics = Default::default();
    m.production = crate::production::ProductionState::new(vec![
        TechState::new(1, 0, 0, 0).unwrap();
        defenders + 1
    ])
    .unwrap();
    m.next_unit_id = 1;
    m.grid.rebuild(&m.world.game.units);
    m
}
fn add(m: &mut Match, name: &str, owner: usize, x: f64, z: f64) -> u32 {
    let id = m.next_unit_id;
    m.next_unit_id += 1;
    let mut u = initial_unit(
        &m.rules,
        Catalog::compiled().unwrap(),
        m.rules.kind(name).unwrap(),
        id,
        owner,
        Point { x, z },
    )
    .unwrap();
    u.still = Some(10.0);
    m.commands
        .insert(id, UnitControl::new(&u, &m.rules).unwrap());
    m.combat.initialize(id);
    m.firing.initialize(id);
    m.world.game.units.insert(id, u);
    m.grid.rebuild(&m.world.game.units);
    id
}

#[test]
fn horde_kaiju_shells_scale_per_defender_and_preserve_rear_armor() {
    for defenders in [1, 3] {
        for rear in [false, true] {
            let mut m = combat_scene(defenders);
            let boss = add(&mut m, "kaiju", defenders, 60.0, 60.0);
            let tank = add(&mut m, "tank", 0, 80.0, 60.0);
            assert_eq!(m.world.game.units[&boss].hp, 6000.0);
            m.world.game.units.get_mut(&boss).unwrap().rot =
                if rear { std::f64::consts::PI } else { 0.0 };
            let source = FlightSource {
                shooter: tank,
                kind: m.rules.kind("tank").unwrap(),
                owner: 0,
                airborne: false,
            };
            let weapon = WeaponDamage::from_catalog("tank").unwrap();
            let impact = Impact {
                shooter: tank,
                owner: 0,
                weapon,
                damage_scale: Some(1.0),
                energy: 1.0,
                armor_piercing: false,
                ap_naval_only: false,
                ap_multiplier: 1.0,
                launch_origin: Point { x: 80.0, z: 60.0 },
                at: Point { x: 60.0, z: 60.0 },
                direction: Point { x: -1.0, z: 0.0 },
            };
            let order = m.world.game.units.keys().copied().collect::<Vec<_>>();
            let (mut damage, sink) = m.firing_damage_view(&order);
            let actual = damage.direct_hit(source, boss, impact, sink).unwrap();
            assert_eq!(
                actual,
                weapon.vehicle * if rear { 2.0 } else { 1.0 } / defenders as f64
            );
            assert_eq!(m.world.game.units[&boss].hp, 6000.0 - actual);
        }
    }
}

#[test]
fn horde_bunker_and_kaiju_generic_hurt_scale_per_defender() {
    for defenders in [1, 3] {
        let mut m = combat_scene(defenders);
        let boss = add(&mut m, "kaiju", defenders, 60.0, 60.0);
        let bunker = add(&mut m, "bunker", 0, 80.0, 60.0);
        let profile = ExplosionSource::weapon(m.rules.kind("kaiju").unwrap(), &m.rules).unwrap();
        let before = [m.world.game.units[&boss].hp, m.world.game.units[&bunker].hp];
        let order = m.world.game.units.keys().copied().collect::<Vec<_>>();
        let (mut damage, sink) = m.firing_damage_view(&order);
        damage.hurt(boss, profile, 1.0, 0, sink).unwrap();
        damage
            .hurt(bunker, profile, 1.0, defenders as i32, sink)
            .unwrap();
        assert_eq!(
            m.world.game.units[&boss].hp,
            before[0] - profile.vehicle / defenders as f64
        );
        assert_eq!(
            m.world.game.units[&bunker].hp,
            before[1] - profile.terrain.unwrap() / defenders as f64
        );
    }
}

#[test]
fn horde_breath_hits_whole_line_once_and_overlapping_trench_cells_once() {
    let mut m = combat_scene(1);
    let boss = add(&mut m, "kaiju", 1, 61.0, 61.0);
    let near = add(&mut m, "rifle", 0, 71.0, 61.0);
    let far = add(&mut m, "rifle", 0, 91.0, 61.0);
    let aside = add(&mut m, "rifle", 0, 81.0, 81.0);
    let ally = add(&mut m, "rifle", 1, 81.0, 61.0);
    let behind = add(&mut m, "rifle", 0, 59.0, 61.0);
    let outside = add(&mut m, "rifle", 0, 97.0, 61.0);
    let trench = 30 * m.world.game.w + 40;
    m.world
        .mutate_cell(
            trench,
            CellChange {
                object: Some(b'T'),
                ..Default::default()
            },
        )
        .unwrap();
    let before = m
        .world
        .game
        .units
        .iter()
        .map(|(&id, u)| (id, u.hp))
        .collect::<BTreeMap<_, _>>();
    // The real targeting/firing block must dispatch the numeric beam weapon.
    m.world.game.players[1].visible = vec![near, far, aside, behind, outside];
    m.commands.units.get_mut(&boss).unwrap().attack_id = near;
    m.firing.initialize(boss).target_id = near;
    m.apply_unit_firing(boss, false, 0.05).unwrap();
    for id in [near, far] {
        assert_eq!(before[&id] - m.world.game.units[&id].hp, 40.0);
    }
    for id in [aside, ally, behind, outside] {
        assert_eq!(before[&id], m.world.game.units[&id].hp);
    }
    assert_eq!(m.world.game.physical(trench), b'T');
    let wear = m.world.game.wear.as_ref().unwrap()[trench];
    assert!(wear > 0.0 && wear <= 250.0 / 400.0);
    let rows = m
        .firing
        .events
        .iter()
        .map(|e| e.to_json(&m.rules))
        .collect::<Vec<_>>();
    for id in [near, far] {
        assert_eq!(
            rows.iter()
                .filter(|v| v["k"] == "hurt" && v["t"] == id)
                .count(),
            1
        );
    }
    assert!(rows.contains(&json!({"f":boss,"t":near,"fo":1,"to":0,"x":95.0,"z":61.0,"hit":true,"k":"kaiju","pub":true})));
    assert_eq!(
        rows.iter()
            .filter(|v| v["k"] == "shell" && v["pub"] == true)
            .count(),
        4
    );
    assert_eq!(m.firing.get(boss).unwrap().cooldown, 6.0);
    assert_eq!(m.world.game.units[&boss].shot_at, Some(0.0));
    assert!(m.firing.flights.is_empty());
    assert!(
        m.advance_tick().is_err(),
        "detached combat scene lacks tick metadata"
    );
    assert!(
        matches!(
            SnapshotCache::new(&m),
            Err(crate::recipient_snapshot::SnapshotError::Rules)
        ),
        "detached combat world lacks full Match metadata"
    );
}

#[test]
fn horde_breath_building_has_one_direct_hit_and_one_shared_terrain_debit() {
    let mut m = combat_scene(1);
    let boss = add(&mut m, "kaiju", 1, 61.0, 61.0);
    let building = add(&mut m, "hq", 0, 81.0, 61.0);
    let cell = 30 * m.world.game.w + 40;
    m.world
        .mutate_cell(
            cell,
            CellChange {
                object: Some(b'B'),
                ..Default::default()
            },
        )
        .unwrap();
    m.world.game.units.get_mut(&building).unwrap().hp = 2000.0;
    m.world.game.units.get_mut(&building).unwrap().cells = Some(vec![cell as u32]);
    m.world
        .register_building_structure(building, &[cell], 2000.0, 2000.0)
        .unwrap();
    m.fire_breath(boss, building).unwrap();
    // The first overlapping sample is 2m before the cell center: 250*(1-2/4.5*0.5).
    let terrain = 250.0 * (1.0 - 2.0 / 4.5 * 0.5);
    assert!((2000.0 - m.world.game.units[&building].hp - 250.0 - terrain).abs() < 1e-9);
    assert!((2000.0 - m.world.game.sections[&(cell as u32)].hp - terrain).abs() < 1e-9);
    assert_eq!(m.firing.events.iter().filter(|e|matches!(e,LaunchEvent::Environment(crate::environment::Effect::Hurt(h)) if h.unit==building)).count(),2);
}

#[test]
fn ordinary_ticks_and_recipient_snapshot_keep_horde_breath_unreachable() {
    let mut m = scene(Mode::Conquest, 2);
    for control in m.commands.units.values_mut() {
        control.hold_fire = true;
    }
    for _ in 0..3 {
        let report = m.advance_tick().unwrap();
        assert!(!report.events.iter().any(|e| matches!(
            e,
            crate::match_tick::TickEvent::Firing(
                LaunchEvent::Breath { .. } | LaunchEvent::BreathShell { .. }
            )
        )));
    }
    assert!(m.tick_state.fault.is_none());
    SnapshotCache::new(&m).unwrap().recipient(0).unwrap();
    let shooter = *m.world.game.units.keys().next().unwrap();
    let target = *m.world.game.units.keys().last().unwrap();
    let before = m.firing.events.clone();
    assert!(m.fire_breath(shooter, target).is_err());
    assert_eq!(m.firing.events, before);
    assert!(m.horde.is_none());
}

#[test]
fn cmb59_breath_full_range_and_radius_boundaries_hit_once_without_falloff() {
    let mut m = combat_scene(1);
    let boss = add(&mut m, "kaiju", 1, 61.0, 61.0);
    let target = add(&mut m, "rifle", 0, 71.0, 61.0);
    let mut cases = Vec::new();
    // sim.js: along > 0 && along <= 34 && across <= 2.5 + radius (1.5).
    for (x, z, hit) in [
        (95.0, 61.0, true),
        (95.000001, 61.0, false),
        (61.0, 65.0, false),
        (61.000001, 65.0, true),
        (81.0, 65.0, true),
        (81.0, 65.000001, false),
        (81.0, 57.0, true),
        (81.0, 56.999999, false),
    ] {
        let id = add(&mut m, "rifle", 0, x, z);
        cases.push((id, hit, m.world.game.units[&id].hp));
    }
    m.fire_breath(boss, target).unwrap();
    for (id, hit, hp) in cases {
        assert_eq!(
            hp - m.world.game.units[&id].hp,
            if hit { 40.0 } else { 0.0 }
        );
        assert_eq!(
            m.firing
                .events
                .iter()
                .filter(|e| matches!(e,
            LaunchEvent::Environment(crate::environment::Effect::Hurt(h)) if h.unit == id))
                .count(),
            usize::from(hit)
        );
    }
}

#[test]
fn cmb59_breath_terrain_samples_scar_once_and_obey_seeded_ignition_predicates() {
    for (wet, burnt, wear, existing_fire, can_ignite) in [
        (0.0, 0, 0.0, false, true),
        (0.35, 0, 0.0, false, false),
        (0.349999, 0, 0.0, false, true),
        (0.0, 1, 0.0, false, false),
        (0.0, 0, 0.5, false, false),
        (0.0, 0, 0.0, true, true),
    ] {
        let mut m = combat_scene(1);
        let boss = add(&mut m, "kaiju", 1, 61.0, 61.0);
        let target = add(&mut m, "rifle", 0, 71.0, 61.0);
        m.start.environment_seed = 1;
        m.world.game.wet = Some(wet);
        m.world.burnt.fill(burnt);
        m.world.game.wear.as_mut().unwrap().fill(wear);
        if existing_fire {
            m.world.fires.insert(1772, 2.5);
        }
        m.fire_breath(boss, target).unwrap();
        // Pinned damageCells: 4..=34 by 2m, radius 2.5, cell reach 3.5,
        // terrain 250, scar 0.25, scar.hit 600. Overlap retains the first sample.
        assert_eq!(m.world.scar.iter().filter(|v| **v != 0.0).count(), 54);
        for (cell, expected) in [
            (1772, f32::from_bits(0x3da5_ed09)),
            (1831, f32::from_bits(0x3da5_ed09)),
            (1832, f32::from_bits(0x3dd5_5555)),
            (1833, f32::from_bits(0x3da5_ed09)),
            (1892, f32::from_bits(0x3da5_ed09)),
            (1830, 0.0),
            (1849, 0.0),
        ] {
            assert_eq!(
                m.world.scar[cell].to_bits(),
                expected.to_bits(),
                "cell {cell}"
            );
        }
        // Seed 1 consumes one draw per unique grass cell. Exactly cell 1772
        // draws below CFG.fire.ignite['.'] (0.03), before flammability checks.
        assert_eq!(m.start.environment_seed, 120306095);
        assert_eq!(
            m.world.fires.iter().map(|(c, _)| *c).collect::<Vec<_>>(),
            if can_ignite { vec![1772] } else { vec![] }
        );
        if can_ignite {
            assert_eq!(
                *m.world.fires.get(&1772).unwrap(),
                if existing_fire { 2.5 } else { 5.0 }
            );
        }
    }
}
