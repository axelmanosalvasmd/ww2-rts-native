//! MAP-79 home selection and usable construction space after actual World startup.
use serde_json::{Value, json};
use ww2_core::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::{CELL, Point},
    map::AuthoritativeMap,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    match_world::world_can_stamp,
    math,
    random::MatchRandom,
    weather::WeatherChoice,
    world_generation::{self, WorldSize},
};

#[test]
fn map79_home_selection_matches_pinned_pure_generator_values() {
    // Reuse the existing pure generator comparison, without recording behavior fixtures.
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/world-generation.json")).unwrap();
    assert_eq!(
        fixture["reference"],
        "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
    );
    for case in fixture["cases"].as_array().unwrap() {
        let size = if case["size"] == "huge" {
            WorldSize::Huge
        } else {
            WorldSize::Massive
        };
        let seed = case["seed"].as_u64().unwrap() as u32;
        let teams: Vec<_> = case["teams"]
            .as_array()
            .unwrap()
            .iter()
            .map(|team| team.as_i64().unwrap() as i32)
            .collect();
        let generated = world_generation::generate_from_terrain_seed(seed, size, &teams).unwrap();
        for (slot, team) in teams.iter().enumerate() {
            let expected = case["regions"]
                .as_array()
                .unwrap()
                .iter()
                .find(|region| region["home"] == json!(slot))
                .unwrap();
            let home = generated
                .regions
                .iter()
                .find(|r| r.home == Some(slot))
                .unwrap();
            assert_eq!(
                json!(home.id),
                expected["id"],
                "{size:?} seed {seed} slot {slot}"
            );
            assert_eq!(
                json!([home.x, home.y]),
                json!([expected["x"], expected["y"]])
            );
            assert_eq!(home.team, Some(*team));
            assert_eq!(json!(home.team), expected["team"]);
            assert_eq!(
                json!({"x":generated.spawns[slot].x,"y":generated.spawns[slot].y}),
                case["spawns"][slot]
            );
        }
    }
}

#[test]
fn map79_generated_homes_have_twenty_real_barracks_sites_and_fair_spread() {
    // The six settings from test-world-acceptance.js, with ordinary HQ and troop placement.
    for size in [WorldSize::Huge, WorldSize::Massive] {
        for seed in [1_u32, 0x3141_5926, u32::MAX] {
            let count = if seed == 1 { 2 } else { 6 };
            let teams: Vec<_> = (0..count).map(|slot| slot as i32).collect();
            let generated =
                world_generation::generate_world_from_terrain_seed(seed, size, &teams, true)
                    .unwrap();
            let homes: Vec<_> = (0..count)
                .map(|slot| {
                    let home = generated
                        .geography
                        .regions
                        .iter()
                        .find(|r| r.home == Some(slot))
                        .unwrap();
                    (
                        home.id,
                        Point {
                            x: (home.x as f64 + 0.5) * CELL,
                            z: (home.y as f64 + 0.5) * CELL,
                        },
                    )
                })
                .collect();
            let map = AuthoritativeMap::from_world(generated.into_world_map_input()).unwrap();
            let options = MatchOptions {
                seed: u64::from(seed),
                mode: Mode::World,
                players: (0..count)
                    .map(|slot| PlayerConfig {
                        name: format!("seat{slot}"),
                        team: slot as i32,
                        faction: Faction::try_from((slot % 3) as u8).unwrap(),
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
            let m = Match::new_world(&map, options, MatchRandom::new(u64::from(seed))).unwrap();
            let runtime = m.world_runtime.as_ref().unwrap();
            let g = &m.world.game;
            let mut usable = Vec::new();
            for (slot, &(home_id, home)) in homes.iter().enumerate() {
                assert_eq!(runtime.regions[home_id].team, teams[slot]);
                let mut package: Vec<_> = g
                    .units
                    .values()
                    .filter(|unit| unit.owner == slot)
                    .map(|unit| m.rules.kinds[unit.kind].name.as_str())
                    .collect();
                package.sort_unstable();
                assert_eq!(package, ["engineer", "hq", "rifle"]);
                let nearest = map
                    .geography()
                    .unwrap()
                    .regions
                    .iter()
                    .enumerate()
                    .filter(|(id, _)| runtime.regions[*id].team < 0)
                    .map(|(_, r)| {
                        math::hypot(
                            home.x - (r.x as f64 + 0.5) * CELL,
                            home.z - (r.y as f64 + 0.5) * CELL,
                        )
                    })
                    .fold(f64::INFINITY, f64::min);
                assert!(
                    (100.0..=140.0).contains(&nearest),
                    "{size:?} seed {seed} slot {slot}: neutral distance {nearest}"
                );
                let mut sites = 0;
                for dz in (-10..=10).step_by(2) {
                    for dx in (-10..=10).step_by(2) {
                        // Barracks size 3: use the public preview's Math.round snap.
                        let x = math::round((home.x + f64::from(dx)) / CELL - 1.5) as usize;
                        let y = math::round((home.z + f64::from(dz)) / CELL - 1.5) as usize;
                        let cells: Vec<_> = (y..y + 3)
                            .flat_map(|yy| (x..x + 3).map(move |xx| yy * g.w + xx))
                            .collect();
                        let owned = cells.iter().all(|&cell| {
                            let at = Point {
                                x: (cell % g.w) as f64 * CELL + CELL / 2.0,
                                z: (cell / g.w) as f64 * CELL + CELL / 2.0,
                            };
                            m.world_region_index(at)
                                .unwrap()
                                .is_some_and(|id| runtime.regions[id].team == teams[slot])
                        });
                        let reserved_node = runtime.nodes.iter().any(|node| {
                            let nx = node.cell % g.w;
                            let ny = node.cell / g.w;
                            cells.iter().any(|&cell| {
                                (nx..nx + 2).contains(&(cell % g.w))
                                    && (ny..ny + 2).contains(&(cell / g.w))
                            })
                        });
                        if owned
                            && !reserved_node
                            && !runtime.rails.excludes_footprint(&cells)
                            && world_can_stamp(g, &m.rules, &cells)
                        {
                            sites += 1;
                        }
                    }
                }
                assert!(
                    sites >= 20,
                    "{size:?} seed {seed} slot {slot}: {sites} Barracks sites"
                );
                usable.push(sites);
            }
            assert!(
                usable.iter().max().unwrap() - usable.iter().min().unwrap() <= 5,
                "{size:?} seed {seed}: Barracks counts {usable:?}"
            );
        }
    }
}
