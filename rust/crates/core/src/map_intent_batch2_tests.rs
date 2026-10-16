//! MAP behavior checks through the existing movement, firing and terrain owners.
use super::*;
use crate::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    environment::{Context, DamageContext},
    game::{COVER, LAND, SIGHT, TRENCH, WIRE},
    map::AuthoritativeMap,
    match_commands::Command,
    match_runtime::{MatchOptions, PlayerConfig},
    orders::OrderKey,
    random::{MatchRandom, StreamId},
    unit_publication::{PublicationParts, StagedUnit, prepare_publication},
    weather::WeatherChoice,
};
use serde_json::{Value as Json, json};
use std::collections::BTreeSet;

const WIDTH: usize = 60;
const CELL: usize = 25 * WIDTH + 25;

fn map_value() -> Json {
    json!({
        "name":"MAP second batch", "w":WIDTH, "h":WIDTH,
        "rows":vec![".".repeat(WIDTH);WIDTH],
        "heights":vec!["0".repeat(WIDTH);WIDTH],
        "spawns":[{"x":3,"y":3},{"x":56,"y":56}],
        "points":[{"x":20,"y":25}]
    })
}

fn scene(value: Json) -> Match {
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&value).unwrap()).unwrap();
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
            supply: false,
            logistics: false,
            sandbox: false,
        },
    )
    .unwrap()
}

fn publish(m: &mut Match, kind: &str, owner: usize, at: Point) -> u32 {
    let id = m.next_unit_id;
    let staged = StagedUnit::ordinary(
        &m.world.game,
        &m.rules,
        owner,
        m.rules.kind(kind).unwrap(),
        id,
        m.world.game.units.len() as u32,
        m.start.players[owner].spawn,
        Some(at),
    )
    .unwrap();
    prepare_publication(
        PublicationParts {
            units: &mut m.world.game.units,
            grid: &mut m.grid,
            controls: &mut m.commands.units,
            recruitment: &mut m.recruit_metadata,
            next_id: &mut m.next_unit_id,
        },
        &m.rules,
        2,
        vec![staged],
    )
    .unwrap()
    .publish_with(|u, _| {
        m.combat.initialize(u.id);
        m.firing.initialize(u.id);
        m.abilities.initialize(u.id);
        m.recovery.initialize(u.id);
    });
    m.commands.units.get_mut(&id).unwrap().auto_cast = false;
    m.commands.units.get_mut(&id).unwrap().auto_retreat = false;
    id
}

fn crush(ch: u8, into: u8, hp: f32, flags: u16) {
    let mut value = map_value();
    let mut row = vec![b'.'; WIDTH];
    row[20..=30].fill(ch);
    value["rows"][25] = json!(String::from_utf8(row).unwrap());
    let mut m = scene(value);
    assert_eq!(m.world.game.chars[CELL], ch);
    assert_eq!(m.world.game.cell_hp[CELL], hp);
    assert_eq!(m.world.game.flags[CELL], flags);
    let id = publish(&mut m, "tank", 0, Point { x: 51.0, z: 43.0 });
    m.world.game.units.get_mut(&id).unwrap().rot = std::f64::consts::FRAC_PI_2;

    // No command means no displacement and no crushing, even after a real tick.
    let idle = m.advance_tick().unwrap();
    assert_eq!(m.world.game.chars[CELL], ch);
    assert!(!idle.environment().any(|effect| matches!(effect,
        crate::environment::Effect::Collapse(c) if c.position == (Point { x:51.0, z:51.0 }))));
    let tick = m.commands.queue.next_tick().unwrap();
    m.enqueue_command(
        OrderKey {
            tick,
            issuer: 0,
            sequence: 1,
        },
        Command::from_json(&json!({"t":"move","orders":[[id,51,59]]})).unwrap(),
    )
    .unwrap();
    let mut crossed = false;
    for _ in 0..240 {
        let before = m.world.game.units[&id].at();
        let report = m.advance_tick().unwrap();
        if m.world.game.chars[CELL] == into {
            let actor = &m.world.game.units[&id];
            assert_ne!(actor.at(), before, "crushing follows real movement");
            assert_eq!(m.world.game.cell_of(actor.x, actor.z), Some(CELL));
            assert_eq!(m.world.game.cell_hp[CELL], 0.0);
            assert_eq!(m.world.game.flags[CELL], crate::map::flags(into));
            let effects: Vec<_> = report.environment().filter(|effect| matches!(effect,
                crate::environment::Effect::Collapse(c) if c.id == format!("collapse:{}:{CELL}", report.tick)
            )).collect();
            assert_eq!(
                effects.len(),
                1,
                "one moved-contact collapse reaches the tick report"
            );
            assert!(
                m.world
                    .cell_log()
                    .iter()
                    .any(|row| row.cell == CELL && row.ch == into)
            );
            crossed = true;
            break;
        }
    }
    assert!(
        crossed,
        "the ordered tank must cross and crush {}",
        char::from(ch)
    );

    // Damage uses the same replacement independently of the movement path.
    let mut m = scene(map_value());
    m.world.set_cell(CELL, ch).unwrap();
    let order: Vec<_> = m.world.game.units.keys().copied().collect();
    let mut ctx = Context {
        tick: 0,
        seed: &mut m.start.environment_seed,
        next_id: &mut m.next_unit_id,
        unit_order: &order,
        grid: &m.grid,
        rain: 0.0,
        wet: 0.0,
        wind_angle: 0.0,
        wind_strength: 0.5,
    };
    if ch == b'H' {
        m.environment.ignite(&mut m.world, CELL, &mut ctx).unwrap();
        assert_eq!(m.world.fires.get(&CELL), Some(&14.0));
    }
    let skip = BTreeSet::new();
    m.environment
        .damage_cells(
            &mut m.world,
            crate::environment::Blast {
                at: Point { x: 51.0, z: 51.0 },
                radius: 0.49,
                damage: if ch == b'T' {
                    401.0
                } else {
                    f64::from(hp) + 1.0
                },
                scar: 0.0,
            },
            &mut ctx,
            &mut DamageContext::plain(&skip),
        )
        .unwrap();
    assert_eq!(m.world.game.chars[CELL], into);
}

#[test]
fn map27_queued_tank_crushes_hedge_and_damage_clears_burning_cover() {
    crush(b'H', b'.', 60.0, LAND | SIGHT | COVER);
}

#[test]
fn map28_queued_tank_crushes_wall_without_sight_or_movement_blocking() {
    crush(b'#', b'+', 150.0, LAND | COVER);
}

#[test]
fn map30_queued_tank_crushes_trench_with_directional_settled_cover() {
    let mut m = scene(map_value());
    m.world.set_cell(CELL, b'T').unwrap();
    let id = publish(&mut m, "rifle", 0, Point { x: 51.0, z: 51.0 });
    let from = Point { x: 71.0, z: 51.0 };
    assert_eq!(
        crate::terrain::ground_mul(&m.world.game, &m.rules, CELL, false),
        1.2
    );
    for (still, expected) in [(0.0, 0.5), (4.0, 0.4), (8.0, 0.3), (12.0, 0.3)] {
        m.world.game.units.get_mut(&id).unwrap().still = Some(still);
        assert_eq!(
            m.cover_mul(&m.world.game.units[&id], from).unwrap(),
            expected
        );
    }
    m.world
        .game
        .trench_front
        .get_or_insert_with(|| vec![f32::NAN; WIDTH * WIDTH])[CELL] = 0.0;
    assert_eq!(m.cover_mul(&m.world.game.units[&id], from).unwrap(), 0.3);
    assert_eq!(
        m.cover_mul(&m.world.game.units[&id], Point { x: 31.0, z: 51.0 })
            .unwrap(),
        0.5
    );
    crush(b'T', b'+', 0.0, LAND | COVER | TRENCH);
}

#[test]
fn map35_queued_tank_crushes_wire_that_slows_infantry() {
    let mut m = scene(map_value());
    m.world.set_cell(CELL, b'X').unwrap();
    assert_eq!(
        crate::terrain::ground_mul(&m.world.game, &m.rules, CELL, false),
        0.35
    );
    crush(b'X', b'.', 40.0, LAND | WIRE);
}

#[test]
fn map52_firing_uses_elevation_accuracy_and_both_clamps() {
    let mut m = scene(map_value());
    let shooter = publish(&mut m, "mg", 0, Point { x: 51.0, z: 51.0 });
    let target = publish(&mut m, "rifle", 1, Point { x: 73.0, z: 51.0 });
    assert_eq!(
        n(
            m.weapon(&m.world.game.units[&shooter]).unwrap().unwrap(),
            "accInf",
            0.0
        ),
        0.5
    );
    let target_cell = m.world.game.cell_of(73.0, 51.0).unwrap();
    // Literal factors from the prototype: 1 + 0.15 * difference, clamped to 0.7..1.45.
    for (shooter_level, target_level, factor) in [
        (-2, 4, 0.7),
        (0, 4, 0.7),
        (0, 2, 0.7),
        (0, 1, 0.85),
        (0, 0, 1.0),
        (1, 0, 1.15),
        (2, 0, 1.3),
        (3, 0, 1.45),
        (4, 0, 1.45),
        (4, -2, 1.45),
    ] {
        m.world.set_level(CELL, shooter_level).unwrap();
        m.world.set_level(target_cell, target_level).unwrap();
        let mut hits = 0;
        for seed in 0..128 {
            m.start.random = MatchRandom::new(seed);
            let mut expected = m.start.random.clone();
            let accurate = expected.next_fraction(StreamId::Combat) < 0.5 * factor;
            if !accurate {
                expected.next_fraction(StreamId::Combat);
                expected.next_fraction(StreamId::Combat);
            }
            m.firing.flights.clear();
            m.firing.events.clear();
            m.fire_weapon(shooter, target, false).unwrap();
            assert_eq!(m.firing.flights.len(), 1);
            let flight = m.firing.flights.values().next().unwrap();
            assert_eq!(
                flight.solution.velocity.z == 0.0,
                accurate,
                "levels {shooter_level}/{target_level}, seed {seed}"
            );
            assert_eq!(m.start.random.snapshot(), expected.snapshot());
            hits += usize::from(accurate);
        }
        assert!(
            (1..128).contains(&hits),
            "both sides of the accuracy threshold are exercised"
        );
    }
}

#[test]
fn map52_named_house_sight_overrides_combine_with_height_and_rain() {
    let mut value = map_value();
    let mut rows = vec![vec![b'.'; WIDTH]; WIDTH];
    for (x, y, width, height) in [
        (10, 15, 1, 1),
        (15, 15, 11, 1),
        (30, 15, 6, 4),
        (10, 30, 1, 1),
        (20, 30, 1, 1),
        (30, 30, 1, 1),
    ] {
        for row in &mut rows[y..y + height] {
            row[x..x + width].fill(b'B');
        }
    }
    value["rows"] = json!(
        rows.into_iter()
            .map(|r| String::from_utf8(r).unwrap())
            .collect::<Vec<_>>()
    );
    value["buildings"] = json!([
        {"x":10,"y":30,"kind":"church"}, {"x":20,"y":30,"kind":"factory"}, {"x":30,"y":30,"kind":"mine head"}
    ]);
    let mut m = scene(value);
    let id = publish(&mut m, "rifle", 0, Point { x: 51.0, z: 51.0 });
    for (kind, x, y, mul) in [
        (0, 10, 15, 1.25),
        (1, 15, 15, 1.25),
        (2, 30, 15, 1.25),
        (3, 10, 30, 1.6),
        (4, 20, 30, 1.4),
        (5, 30, 30, 1.5),
    ] {
        let cell = y * WIDTH + x;
        assert_eq!(m.world.game.house[cell], kind);
        for level in [-2, 0, 4] {
            m.world.set_level(cell, level).unwrap();
            let unit = m.world.game.units.get_mut(&id).unwrap();
            unit.x = (x as f64 + 0.5) * 2.0;
            unit.z = (y as f64 + 0.5) * 2.0;
            unit.garrison = Some(cell);
            for rain in [0.0, 0.5, 1.0] {
                m.world.game.rain = rain;
                let expected = 36.0 * (1.0 + 0.1 * f64::from(level)) * mul * (1.0 - 0.2 * rain);
                let actual =
                    vision::vision_range(&m.world.game, &m.rules, &m.world.game.units[&id]);
                assert_eq!(
                    actual.to_bits(),
                    expected.to_bits(),
                    "house {kind}, level {level}, rain {rain}"
                );
            }
        }
    }
}

#[test]
fn map33_timed_bridge_collapse_after_setup_kills_occupants_and_logs_the_span() {
    let mut value = map_value();
    let rows: Vec<_> = (0..WIDTH)
        .map(|y| {
            let mut row = vec![b'.'; WIDTH];
            row[24..=26].fill(if y == 25 {
                b'='
            } else if y == 40 {
                b'F'
            } else {
                b'W'
            });
            String::from_utf8(row).unwrap()
        })
        .collect();
    value["rows"] = json!(rows);
    value["buildings"] = json!([{"x":25,"y":25,"kind":"stone bridge"}]);
    value["triggers"] = json!([{"at":0.05,"blow":[24,25,26,25]}]);
    let mut m = scene(value);
    let cells: Vec<_> = (24..=26).map(|x| 25 * WIDTH + x).collect();
    for &cell in &cells {
        assert_eq!(m.world.game.chars[cell], b'=');
        assert_eq!(m.world.game.cell_hp[cell], 2400.0);
        assert_eq!(m.world.game.physical(cell), b'=');
    }
    let rifle = publish(&mut m, "rifle", 1, Point { x: 49.0, z: 51.0 });
    let tank = publish(&mut m, "tank", 1, Point { x: 53.0, z: 51.0 });
    let report = m.advance_tick().unwrap();
    assert_eq!(report.tick, 1);
    for &cell in &cells {
        assert_eq!(m.world.game.chars[cell], b'W');
        assert!(m.world.game.sections[&(cell as u32)].failed);
        assert!(
            m.world
                .cell_log()
                .iter()
                .any(|row| row.cell == cell && row.ch == b'W')
        );
    }
    for id in [rifle, tank] {
        assert!(
            !m.world.game.units.contains_key(&id),
            "bridge casualties retire through the normal tick"
        );
        assert!(report.retired.iter().any(|unit| unit.unit.id == id));
    }
    let collapses: Vec<_> = report
        .environment()
        .filter_map(|effect| match effect {
            crate::environment::Effect::Collapse(c) => c.section.as_ref(),
            _ => None,
        })
        .collect();
    assert_eq!(collapses.len(), 3);
}
