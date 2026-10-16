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

fn pit_map() -> AuthoritativeMap {
    AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"Gun pit", "w":128, "h":128, "rows":vec![".".repeat(128);128],
            "spawns":[{"x":2,"y":2},{"x":125,"y":125}],
            "points":[{"x":64,"y":64}], "buildings":[]
        }))
        .unwrap(),
    )
    .unwrap()
}

fn ordinary_scene() -> Match {
    Match::new(
        &pit_map(),
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

fn scene() -> Match {
    let mut m = ordinary_scene();
    let players = m.world.game.players.clone();
    m.world = World::from_map(&pit_map()).unwrap();
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
        m.world
            .register_recipient(crate::world::Recipient::Player(owner));
    }
    m.initialize_recipient_routing().unwrap();
    m
}

fn soldier(m: &mut Match, name: &str, owner: usize, at: Point) -> u32 {
    let id = m.next_unit_id;
    m.next_unit_id += 1;
    let kind = m.rules.kind(name).unwrap();
    let mut unit =
        unit_spawn::initial_unit(&m.rules, Catalog::compiled().unwrap(), kind, id, owner, at)
            .unwrap();
    unit.still = Some(10.0);
    m.commands
        .insert(id, UnitControl::new(&unit, &m.rules).unwrap());
    m.firing.initialize(id);
    m.combat.initialize(id);
    m.abilities.initialize(id);
    m.recovery.initialize(id);
    m.world.game.units.insert(id, unit);
    m.grid.rebuild(&m.world.game.units);
    id
}

fn build_pit(m: &mut Match, ordinary_ticks: bool) {
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
        if ordinary_ticks {
            m.advance_tick().unwrap();
        } else {
            m.world.game.tick += 1;
            m.apply_field_work_unit(builder).unwrap();
        }
    }
    assert!(!m.world.game.units[&builder].dig, "paid pit completes");
    let center = m.world.game.cell_of(PIT.x, PIT.z).unwrap();
    assert_eq!(m.world.game.chars[center], b'T');
    assert_eq!(
        m.field_works.pits().iter().copied().collect::<Vec<_>>(),
        vec![center]
    );
    assert_eq!(
        m.world.game.chars.iter().filter(|&&ch| ch == b'T').count(),
        1
    );
    assert_eq!(
        m.world.game.chars.iter().filter(|&&ch| ch == b'#').count(),
        5
    );
    if !ordinary_ticks {
        assert_eq!(m.start.economy.players[0].mp, 940.0, "no per-cell recharge");
    }
}

#[test]
fn eng05_paid_pit_gives_only_five_weapon_teams_live_trench_range_and_firing() {
    for name in ["mg", "at", "mortar", "flak", "howitzer", "rifle", "tank"] {
        let mut m = scene();
        build_pit(&mut m, false);
        let shooter = soldier(&mut m, name, 0, PIT);
        let base = n(
            m.weapon(&m.world.game.units[&shooter]).unwrap().unwrap(),
            "range",
            0.0,
        );
        let target = soldier(
            &mut m,
            "rifle",
            1,
            Point {
                x: PIT.x,
                z: PIT.z + base * 1.125,
            },
        );
        // Spotting is an authored input, independent of the weapon team's vision radius.
        m.world.game.players[0].visible.push(target);
        let team = !matches!(name, "rifle" | "tank");
        assert_eq!(m.can_shoot_target(shooter, target).unwrap(), team, "{name}");
        assert_eq!(
            m.pick_firing_target(shooter).unwrap(),
            if team { target } else { 0 },
            "{name}"
        );
        if !team {
            continue;
        }
        m.set_firing_target(shooter, target).unwrap();
        m.apply_unit_firing(shooter, false, 0.05).unwrap();
        assert!(
            m.firing
                .flights
                .values()
                .any(|flight| flight.shooter == shooter),
            "{name} fires beyond base range"
        );
        assert!(m.firing.get(shooter).unwrap().cooldown > 0.0);

        // The multiplier ends at 1.25, and an ordinary trench is insufficient.
        m.world.game.units.get_mut(&target).unwrap().z = PIT.z + base * 1.25;
        m.grid.rebuild(&m.world.game.units);
        assert!(
            m.can_shoot_target(shooter, target).unwrap(),
            "{name} exact extended limit"
        );
        m.world.game.units.get_mut(&target).unwrap().z = PIT.z + base * 1.251;
        m.grid.rebuild(&m.world.game.units);
        assert!(
            !m.can_shoot_target(shooter, target).unwrap(),
            "{name} upper bound"
        );
        m.world.game.units.get_mut(&target).unwrap().z = PIT.z + base * 1.125;
        let plain = m.world.game.cell_of(PIT.x, PIT.z + 8.0).unwrap();
        m.world.set_cell(plain, b'T').unwrap();
        m.world.game.units.get_mut(&shooter).unwrap().z = PIT.z + 8.0;
        m.world.game.units.get_mut(&target).unwrap().z = PIT.z + 8.0 + base * 1.125;
        m.grid.rebuild(&m.world.game.units);
        assert!(
            !m.can_shoot_target(shooter, target).unwrap(),
            "{name} ordinary trench"
        );
        m.world.game.units.get_mut(&shooter).unwrap().z = PIT.z;
        m.world.game.units.get_mut(&target).unwrap().z = PIT.z + base * 1.125;
        let center = m.world.game.cell_of(PIT.x, PIT.z).unwrap();
        m.world.set_cell(center, b'.').unwrap();
        m.grid.rebuild(&m.world.game.units);
        assert!(
            !m.can_shoot_target(shooter, target).unwrap(),
            "{name} destroyed pit trench"
        );
    }
}

#[test]
fn eng05_real_first_flight_segment_clears_adjacent_pit_sandbag_but_later_segment_hits() {
    let mut m = scene();
    build_pit(&mut m, false);
    let shooter = soldier(&mut m, "mg", 0, PIT);
    let target = soldier(
        &mut m,
        "rifle",
        1,
        Point {
            x: PIT.x,
            z: PIT.z - 20.0,
        },
    );
    m.fire_weapon(shooter, target, false).unwrap();
    let mut flight = m
        .firing
        .flights
        .values()
        .find(|flight| flight.shooter == shooter)
        .unwrap()
        .clone();
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

#[test]
fn occupied_gun_pit_and_mine_detonation_survive_ordinary_ticks_and_every_seat_snapshot() {
    use crate::{
        environment::Effect,
        match_commands::MoveOrder,
        match_tick::TickEvent,
        recipient_snapshot::{DeliveryState, SelectedSeat},
    };

    // Retain the ordinary startup roster, production and elimination owners.
    for killed in [false, true] {
        let mut m = ordinary_scene();
        let builder = m.next_unit_id;
        build_pit(&mut m, true);
        // The completed builder starts clear of the occupant's cover cell.
        let unit = m.world.game.units.get_mut(&builder).unwrap();
        unit.x = PIT.x + 10.0;
        unit.z = PIT.z + 10.0;
        m.grid.update(builder, unit);
        let gun = soldier(&mut m, "mg", 0, PIT);
        let mover = soldier(&mut m, "tank", 1, Point { x: 91.0, z: 91.0 });
        if killed {
            m.world.game.units.get_mut(&mover).unwrap().hp = 1.0;
        }
        for control in m.commands.units.values_mut() {
            control.hold_fire = true;
            control.auto_retreat = false;
        }
        let pit = m.world.game.cell_of(PIT.x, PIT.z).unwrap();
        let mine = m.world.game.cell_of(93.0, 91.0).unwrap();
        m.world.set_cell(mine, b'N').unwrap();
        m.world.mines.insert(mine, 0);
        m.preflight_ordinary_snapshot().unwrap();
        let mut delivery = DeliveryState::new(&m).unwrap();
        assert_eq!(
            m.execute_command(
                1,
                &Command::Move {
                    orders: vec![MoveOrder {
                        unit: i64::from(mover),
                        at: Point { x: 97.0, z: 91.0 },
                    }],
                    control_ids: None,
                    attack_move: false,
                    queue: false,
                    together: false,
                    face: None,
                },
                false,
            )
            .unwrap(),
            None,
        );
        let seats: Vec<_> = (0..m.start.players.len())
            .map(|slot| SelectedSeat {
                slot,
                full: true,
                omit: false,
            })
            .collect();
        let mut hits = 0;
        let mut ticks_after_blast = 0;
        for tick in 0..200 {
            let report = m.advance_tick().unwrap();
            assert!(m.tick_state.fault().is_none());
            hits += report
                .events
                .iter()
                .filter(|event| {
                    matches!(event,
                        TickEvent::Environment(Effect::Hurt(hit)) if hit.unit == mover
                    )
                })
                .count();
            if hits > 0 {
                ticks_after_blast += 1;
            }
            assert_eq!(m.world.game.chars[pit], b'T');
            assert!(m.field_works.pits().contains(&pit));
            let at = m.world.game.units[&gun].at();
            assert_eq!(m.world.game.cell_of(at.x, at.z), Some(pit));
            assert!(m.world.game.units[&gun].garrison.is_none());
            let weapon = m.weapon(&m.world.game.units[&gun]).unwrap().unwrap();
            assert_eq!(
                m.weapon_range(&m.world.game.units[&gun], weapon).unwrap(),
                n(weapon, "range", 0.0) * 1.25
            );
            let frame = delivery
                .capture_selected_with_tick_events(&mut m, &seats, &report.events)
                .unwrap()
                .unwrap();
            for seat in &seats {
                let snapshot = frame.recipient(seat.slot).unwrap().into_json();
                assert_eq!(snapshot["t"], "s", "seat {} at tick {tick}", seat.slot);
                assert!(snapshot["units"].is_array());
            }
        }
        assert_eq!(hits, 1, "the live movement detonates exactly once");
        assert!(
            ticks_after_blast >= 20,
            "continue after damage and retirement"
        );
        assert_eq!(m.world.game.chars[mine], b'+');
        assert!(!m.world.mines.contains_key(&mine));
        assert_eq!(m.world.game.units.contains_key(&mover), !killed);
        assert!(m.tick_state.fault().is_none());
    }
}
