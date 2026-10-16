//! Owned MAP behavior checks at the existing combat provider boundary.
//! Compiled only in the core's unit-test binary.
use super::*;
use crate::{
    catalog::Catalog,
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::Wreck,
    map::AuthoritativeMap,
    match_runtime::{MatchOptions, PlayerConfig},
    unit_spawn::initial_unit,
    weather::WeatherChoice,
};
use serde_json::json;

fn scene() -> Match {
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name":"MAP terrain combat", "w":40,"h":40,
            "rows":vec![".".repeat(40);40],"heights":vec!["0".repeat(40);40],
            "spawns":[{"x":2,"y":2},{"x":37,"y":37}],"points":[{"x":20,"y":20}]
        }))
        .unwrap(),
    )
    .unwrap();
    let mut m = Match::new(
        &map,
        MatchOptions {
            seed: 701,
            mode: Mode::Default,
            players: (0..2)
                .map(|team| PlayerConfig {
                    name: format!("p{team}"),
                    team,
                    faction: Faction::try_from(team as u8).unwrap(),
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
        },
    )
    .unwrap();
    m.world.game.units.clear();
    m.world.game.weather = None;
    m.world.game.wet = None;
    m.world.game.rain = 0.0;
    m
}
fn actor(m: &Match, kind: &str, id: u32, at: Point) -> Unit {
    initial_unit(
        &m.rules,
        Catalog::compiled().unwrap(),
        m.rules.kind(kind).unwrap(),
        id,
        0,
        at,
    )
    .unwrap()
}
fn exact(a: f64, b: f64) {
    assert_eq!(a.to_bits(), b.to_bits(), "{a} != {b}");
}

#[test]
fn map30_map51_trench_settle_facing_cover_wear_and_garrison() {
    let mut m = scene();
    let c = 20 * 40 + 20;
    let at = Point { x: 41.0, z: 41.0 };
    let from = Point { x: 61.0, z: 41.0 };
    let mut t = actor(&m, "rifle", 100, at);
    m.world.set_cell(c, b'T').unwrap();
    exact(
        crate::terrain::ground_mul(&m.world.game, &m.rules, c, false),
        1.2,
    );
    for (still, expected) in [(0.0, 0.5), (4.0, 0.4), (8.0, 0.3), (12.0, 0.3)] {
        t.still = Some(still);
        exact(m.cover_mul(&t, from).unwrap(), expected);
    }
    m.world.game.trench_front = Some(vec![f32::NAN; 1600]);
    m.world.game.trench_front.as_mut().unwrap()[c] = 0.0;
    exact(m.cover_mul(&t, from).unwrap(), 0.3);
    exact(m.cover_mul(&t, Point { x: 21.0, z: 41.0 }).unwrap(), 0.5);
    for (ch, hp, q) in [
        (b'H', 60.0, 1.0),
        (b'H', 30.0, 0.75),
        (b'#', 0.0, 0.5),
        (b'O', 200.0, 0.6),
    ] {
        m.world.set_cell(c, ch).unwrap();
        m.world.game.cell_hp[c] = hp;
        exact(m.cover_mul(&t, from).unwrap(), 1.0 - 0.5 * q);
    }
    m.world.set_cell(c, b'+').unwrap();
    for wear in [0.0f32, 0.5, 1.0] {
        m.world.game.wear.as_mut().unwrap()[c] = wear;
        exact(
            m.cover_mul(&t, from).unwrap(),
            1.0 - 0.5 * (0.55 + f64::from(wear)).min(1.4),
        );
    }
    m.world.set_cell(c, b'B').unwrap();
    t.garrison = Some(c);
    // A one-cell house is the compiled table's smallest house, with its own
    // exact intact/worn accuracy and sight multipliers.
    let house = &m
        .firing_config()
        .unwrap()
        .get("houses")
        .unwrap()
        .array()
        .unwrap()[usize::from(m.world.game.house[c])];
    let (hp, mul, worn) = (
        n(house, "hp", 0.0),
        n(house, "mul", 0.0),
        n(house, "worn", 0.0),
    );
    m.world.game.cell_hp[c] = hp as f32;
    exact(m.cover_mul(&t, from).unwrap(), mul);
    m.world.game.cell_hp[c] = 0.0;
    exact(m.cover_mul(&t, from).unwrap(), worn);
    let vehicle = actor(&m, "tank", 101, at);
    exact(m.cover_mul(&vehicle, from).unwrap(), 1.0);
}

#[test]
fn map51_directional_solids_corners_live_hulls_and_settled_wrecks() {
    let at = Point { x: 41.0, z: 41.0 };
    let east = Point { x: 61.0, z: 41.0 };
    for ch in b"B#RHKQ" {
        let mut m = scene();
        let t = actor(&m, "rifle", 100, at);
        m.world.set_cell(20 * 40 + 21, *ch).unwrap();
        exact(m.cover_behind(&t, east).unwrap(), 1.0);
        exact(m.cover_behind(&t, Point { x: 21.0, z: 41.0 }).unwrap(), 0.0);
    }
    let mut m = scene();
    let t = actor(&m, "rifle", 100, at);
    m.world.set_cell(21 * 40 + 21, b'B').unwrap();
    exact(m.cover_behind(&t, east).unwrap(), 1.0);
    exact(m.cover_behind(&t, Point { x: 21.0, z: 41.0 }).unwrap(), 0.0);
    m.world.set_cell(21 * 40 + 21, b'.').unwrap();
    let hull = actor(&m, "tank", 101, Point { x: 43.0, z: 41.0 });
    m.world.game.units.insert(101, hull);
    exact(m.cover_behind(&t, east).unwrap(), 1.0);
    m.world.game.units.get_mut(&101).unwrap().hp = 0.0;
    exact(m.cover_behind(&t, east).unwrap(), 0.0);
    m.world.game.wrecks.push(Wreck {
        id: Some(101),
        cell: None,
        x: 43.0,
        z: 41.0,
        moving: true,
    });
    exact(m.cover_behind(&t, east).unwrap(), 0.0);
    m.world.game.wrecks[0].moving = false;
    exact(m.cover_behind(&t, east).unwrap(), 1.0);
    m.world.game.wrecks[0].x = 45.0;
    exact(m.cover_behind(&t, east).unwrap(), 0.0);
}

#[test]
fn map52_high_ground_improves_native_volleys_and_sight_uses_level_and_house() {
    let mut counts = Vec::new();
    for level in [0, 2] {
        let mut m = scene();
        let shooter = actor(&m, "rifle", 100, Point { x: 5.0, z: 41.0 });
        let mut target = actor(&m, "rifle", 101, Point { x: 27.0, z: 41.0 });
        target.owner = 1;
        m.world.game.units.insert(100, shooter);
        m.world.game.units.insert(101, target);
        m.world.game.height.as_mut().unwrap()[20 * 40 + 2] = level;
        m.firing.initialize(100);
        m.firing.initialize(101);
        m.combat.initialize(100);
        m.combat.initialize(101);
        m.abilities.initialize(100);
        m.abilities.initialize(101);
        let mut accurate = 0;
        // The native RNG advances through 3,000 ordinary rifle volleys. Each
        // flight aimed at this same-row target has zero lateral velocity only
        // when the shipping accuracy draw succeeds. Misses have a nonzero offset.
        for _ in 0..3000 {
            m.fire_weapon(100, 101, false).unwrap();
            assert_eq!(m.firing.flights.len(), 5);
            accurate += m
                .firing
                .flights
                .values()
                .filter(|f| f.solution.velocity.z == 0.0)
                .count();
            m.firing.flights.clear();
            m.firing.suppressed.clear();
            m.firing.take_events();
        }
        let u = &m.world.game.units[&100];
        exact(
            vision::vision_range(&m.world.game, &m.rules, u),
            vision::vision_of(&m.world.game, &m.rules, u) * (1.0 + 0.1 * f64::from(level)),
        );
        counts.push(accurate);
    }
    assert!(
        counts[1] as f64 > counts[0] as f64 * 1.15,
        "high/flat {counts:?}"
    );
    let mut m = scene();
    let c = 20 * 40 + 20;
    m.world.set_cell(c, b'B').unwrap();
    let mut t = actor(&m, "rifle", 100, Point { x: 41.0, z: 41.0 });
    t.garrison = Some(c);
    let k = usize::from(m.world.game.house[c]);
    let house = m.rules.vision.house_vision[k].unwrap_or(1.25);
    exact(
        vision::vision_range(&m.world.game, &m.rules, &t),
        vision::vision_of(&m.world.game, &m.rules, &t) * house,
    );
}
