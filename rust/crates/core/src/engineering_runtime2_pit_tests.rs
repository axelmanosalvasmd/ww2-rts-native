//! Gun pit construction, weapon reach and launch-adjacent sandbag behavior.
use super::*;
use crate::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    map::AuthoritativeMap,
    match_commands::{Command, UnitControl},
    match_runtime::{MatchOptions, PlayerConfig},
    unit_spawn,
    weather::WeatherChoice,
    world::World,
};
use serde_json::json;

const PIT: Point = Point { x: 61.0, z: 61.0 };

fn scene() -> Match {
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"Gun pit", "w":128, "h":128, "rows":vec![".".repeat(128);128],
            "spawns":[{"x":2,"y":2},{"x":125,"y":125}],
            "points":[{"x":64,"y":64}], "buildings":[]
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
                PlayerConfig { name: "A".into(), team: 0, faction: Faction::Usa },
                PlayerConfig { name: "B".into(), team: 1, faction: Faction::Germany },
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
    let players = m.world.game.players.clone();
    m.world = World::from_map(&map).unwrap();
    m.world.game.players = players;
    m.building_cells.clear();
    m.commands.units.clear();
    m.firing = Default::default();
    m.combat = Default::default();
    m.abilities = Default::default();
    m.recovery = Default::default();
    m.next_unit_id = 1;
    m.grid.rebuild(&m.world.game.units);
    for owner in 0..2 {
        m.world.register_recipient(crate::world::Recipient::Player(owner));
    }
    m.initialize_recipient_routing().unwrap();
    m
}

fn soldier(m: &mut Match, name: &str, owner: usize, at: Point) -> u32 {
    let id = m.next_unit_id;
    m.next_unit_id += 1;
    let kind = m.rules.kind(name).unwrap();
    let mut unit = unit_spawn::initial_unit(
        &m.rules, Catalog::compiled().unwrap(), kind, id, owner, at,
    )
    .unwrap();
    unit.still = Some(10.0);
    m.commands.insert(id, UnitControl::new(&unit, &m.rules).unwrap());
    m.firing.initialize(id);
    m.combat.initialize(id);
    m.abilities.initialize(id);
    m.recovery.initialize(id);
    m.world.game.units.insert(id, unit);
    m.grid.rebuild(&m.world.game.units);
    id
}

fn build_pit(m: &mut Match) {
    let builder = soldier(m, "rifle", 0, PIT);
    vision::update_vision(&mut m.world.game, &m.rules, &m.grid);
    m.start.economy.players[0].mp = 1000.0;
    let command = Command::from_json(&json!({
        "t":"dig", "ids":[builder], "kind":"nest", "x":PIT.x, "z":PIT.z, "dir":0
    }))
    .unwrap();
    assert_eq!(m.execute_command(0, &command, false).unwrap(), None);
    assert_eq!(m.start.economy.players[0].mp, 940.0);
    assert_eq!(m.field_works.jobs()[&builder].cells.len(), 6);
    for _ in 0..2400 {
        if !m.world.game.units[&builder].dig {
            break;
        }
        m.world.game.tick += 1;
        m.apply_field_work_unit(builder).unwrap();
    }
    assert!(!m.world.game.units[&builder].dig, "paid pit completes");
    let center = m.world.game.cell_of(PIT.x, PIT.z).unwrap();
    assert_eq!(m.world.game.chars[center], b'T');
    assert_eq!(m.field_works.pits().iter().copied().collect::<Vec<_>>(), vec![center]);
    assert_eq!(m.world.game.chars.iter().filter(|&&ch| ch == b'T').count(), 1);
    assert_eq!(m.world.game.chars.iter().filter(|&&ch| ch == b'#').count(), 5);
    assert_eq!(m.start.economy.players[0].mp, 940.0, "no per-cell recharge");
}

#[test]
fn eng05_paid_pit_gives_only_five_weapon_teams_live_trench_range_and_firing() {
    for name in ["mg", "at", "mortar", "flak", "howitzer", "rifle", "tank"] {
        let mut m = scene();
        build_pit(&mut m);
        let shooter = soldier(&mut m, name, 0, PIT);
        let base = n(m.weapon(&m.world.game.units[&shooter]).unwrap().unwrap(), "range", 0.0);
        let target = soldier(&mut m, "rifle", 1, Point { x: PIT.x, z: PIT.z + base * 1.125 });
        // Spotting is an authored input, independent of the weapon team's vision radius.
        m.world.game.players[0].visible.push(target);
        let team = !matches!(name, "rifle" | "tank");
        assert_eq!(m.can_shoot_target(shooter, target).unwrap(), team, "{name}");
        assert_eq!(m.pick_firing_target(shooter).unwrap(), if team { target } else { 0 }, "{name}");
        if !team {
            continue;
        }
        m.set_firing_target(shooter, target).unwrap();
        m.apply_unit_firing(shooter, false, 0.05).unwrap();
        assert!(m.firing.flights.values().any(|flight| flight.shooter == shooter), "{name} fires beyond base range");
        assert!(m.firing.get(shooter).unwrap().cooldown > 0.0);

        // The multiplier ends at 1.25, and an ordinary trench is insufficient.
        m.world.game.units.get_mut(&target).unwrap().z = PIT.z + base * 1.25;
        m.grid.rebuild(&m.world.game.units);
        assert!(m.can_shoot_target(shooter, target).unwrap(), "{name} exact extended limit");
        m.world.game.units.get_mut(&target).unwrap().z = PIT.z + base * 1.251;
        m.grid.rebuild(&m.world.game.units);
        assert!(!m.can_shoot_target(shooter, target).unwrap(), "{name} upper bound");
        m.world.game.units.get_mut(&target).unwrap().z = PIT.z + base * 1.125;
        let plain = m.world.game.cell_of(PIT.x, PIT.z + 8.0).unwrap();
        m.world.set_cell(plain, b'T').unwrap();
        m.world.game.units.get_mut(&shooter).unwrap().z = PIT.z + 8.0;
        m.world.game.units.get_mut(&target).unwrap().z = PIT.z + 8.0 + base * 1.125;
        m.grid.rebuild(&m.world.game.units);
        assert!(!m.can_shoot_target(shooter, target).unwrap(), "{name} ordinary trench");
        m.world.game.units.get_mut(&shooter).unwrap().z = PIT.z;
        m.world.game.units.get_mut(&target).unwrap().z = PIT.z + base * 1.125;
        let center = m.world.game.cell_of(PIT.x, PIT.z).unwrap();
        m.world.set_cell(center, b'.').unwrap();
        m.grid.rebuild(&m.world.game.units);
        assert!(!m.can_shoot_target(shooter, target).unwrap(), "{name} destroyed pit trench");
    }
}

#[test]
fn eng05_real_first_flight_segment_clears_adjacent_pit_sandbag_but_later_segment_hits() {
    let mut m = scene();
    build_pit(&mut m);
    let shooter = soldier(&mut m, "mg", 0, PIT);
    let target = soldier(&mut m, "rifle", 1, Point { x: PIT.x, z: PIT.z - 20.0 });
    m.fire_weapon(shooter, target, false).unwrap();
    let mut flight = m.firing.flights.values().find(|flight| flight.shooter == shooter).unwrap().clone();
    let sandbag = m.world.game.cell_of(PIT.x, PIT.z - 2.0).unwrap();
    assert_eq!(m.world.game.chars[sandbag], b'#');
    let a = projectiles::Point3::new(PIT.x, 1.0, PIT.z);
    let b = projectiles::Point3::new(PIT.x, 1.0, PIT.z - 3.5);
    assert_eq!(flight.sequence, 0);
    assert!(m.flight_contact(&flight, a, b, 0.0, 1.0).unwrap().is_none());
    flight.sequence = 1;
    let hit = m.flight_contact(&flight, a, b, 0.0, 1.0).unwrap().unwrap();
    assert_eq!(hit.surface, FlightSurface::Cell(sandbag));
}
