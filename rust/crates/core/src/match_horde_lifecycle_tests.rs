use super::*;
use crate::{
    combat::{Impact, WeaponDamage},
    match_combat::{ExplosionSource, FlightSource},
    match_commands::UnitControl,
    unit_spawn::initial_unit,
    weather::WeatherChoice,
};
use serde_json::json;

fn damage_scene(mode: Mode, defenders: usize) -> Match {
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name": "Bunker damage", "w": 60, "h": 60,
            "rows": vec![".".repeat(60); 60],
            "spawns": [{"x":30,"y":5},{"x":10,"y":54},{"x":50,"y":54}],
            "defend": [0], "points": [{"x":30,"y":30}]
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
    match mode {
        Mode::Assault => Match::new_assault(&map, options, 1).unwrap(),
        Mode::Annihilation => Match::new_annihilation(&map, options).unwrap(),
        Mode::Horde => Match::new_horde(&map, options).unwrap(),
        _ => unreachable!(),
    }
}

fn tank_hit(m: &mut Match, bunker: u32) -> f64 {
    let at = m.world.game.units[&bunker].at();
    let owner = if m.world.game.units[&bunker].owner == 0 {
        m.world.game.players.len() - 1
    } else {
        0
    };
    let kind = m.rules.kind("tank").unwrap();
    let id = m.next_unit_id;
    m.next_unit_id += 1;
    let unit = initial_unit(
        &m.rules,
        Catalog::compiled().unwrap(),
        kind,
        id,
        owner,
        Point {
            x: at.x + 25.0,
            z: at.z,
        },
    )
    .unwrap();
    m.commands
        .insert(id, UnitControl::new(&unit, &m.rules).unwrap());
    m.combat.initialize(id);
    m.firing.initialize(id);
    m.world.game.units.insert(id, unit);
    m.grid.rebuild(&m.world.game.units);
    let source = FlightSource {
        shooter: id,
        kind,
        owner: owner as i32,
        airborne: false,
    };
    let weapon = WeaponDamage::from_catalog("tank").unwrap();
    let impact = Impact {
        shooter: id,
        owner: source.owner,
        weapon,
        damage_scale: Some(1.0),
        energy: 1.0,
        armor_piercing: false,
        ap_naval_only: false,
        ap_multiplier: 1.0,
        launch_origin: Point {
            x: at.x + 25.0,
            z: at.z,
        },
        at,
        direction: Point { x: -1.0, z: 0.0 },
    };
    let before = m.world.game.units[&bunker].hp;
    let order = m.world.game.units.keys().copied().collect::<Vec<_>>();
    let (mut damage, sink) = m.firing_damage_view(&order);
    let dealt = damage.direct_hit(source, bunker, impact, sink).unwrap();
    assert!(((before - m.world.game.units[&bunker].hp) - dealt).abs() < 1e-8);
    dealt
}

fn blast_hit(m: &mut Match, bunker: u32, source: ExplosionSource) -> f64 {
    let before = m.world.game.units[&bunker].hp;
    let owner = if m.world.game.units[&bunker].owner == 0 {
        m.world.game.players.len() - 1
    } else {
        0
    };
    let order = m.world.game.units.keys().copied().collect::<Vec<_>>();
    let (mut damage, sink) = m.firing_damage_view(&order);
    damage
        .hurt(bunker, source, 1.0, owner as i32, sink)
        .unwrap();
    before - m.world.game.units[&bunker].hp
}

fn close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 1e-8,
        "expected {expected} damage, got {actual}"
    );
}

#[test]
fn mode30_bunker_direct_support_and_howitzer_damage_respect_mode_and_defender_count() {
    for (mode, defenders, expected_scale) in [
        (Mode::Assault, 2, 1.0),
        (Mode::Annihilation, 2, 1.0),
        (Mode::Horde, 1, 1.0),
        (Mode::Horde, 3, 1.0 / 3.0),
    ] {
        let mut direct = damage_scene(mode, defenders);
        let bunker = direct
            .world
            .game
            .units
            .values()
            .find(|unit| direct.rules.kinds[unit.kind].name == "bunker")
            .unwrap()
            .id;
        let weapon = WeaponDamage::from_catalog("tank").unwrap();
        close(
            tank_hit(&mut direct, bunker),
            weapon.vehicle * 0.25 * expected_scale,
        );

        let mut support = damage_scene(mode, defenders);
        let bunker = support
            .world
            .game
            .units
            .values()
            .find(|unit| support.rules.kinds[unit.kind].name == "bunker")
            .unwrap()
            .id;
        let artillery = ExplosionSource::support("artillery").unwrap();
        let base = artillery.terrain.unwrap_or(artillery.vehicle);
        close(
            blast_hit(&mut support, bunker, artillery),
            base * 0.05 * expected_scale,
        );

        let mut shell = damage_scene(mode, defenders);
        let bunker = shell
            .world
            .game
            .units
            .values()
            .find(|unit| shell.rules.kinds[unit.kind].name == "bunker")
            .unwrap()
            .id;
        let howitzer =
            ExplosionSource::weapon(shell.rules.kind("howitzer").unwrap(), &shell.rules).unwrap();
        let base = howitzer.terrain.unwrap_or(howitzer.vehicle);
        let annihilation = if mode == Mode::Annihilation { 0.1 } else { 1.0 };
        close(
            blast_hit(&mut shell, bunker, howitzer),
            base * expected_scale * annihilation,
        );
    }
}
