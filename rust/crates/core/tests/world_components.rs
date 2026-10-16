//! Pure World setup/query comparisons and a detached regional-base intent check.
use serde_json::{Value, json};
use ww2_core::{
    catalog::Catalog,
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::{CELL, LOCAL_DEFENDER_OWNER, Point},
    map::AuthoritativeMap,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    match_world_rails::WorldRails,
    random::MatchRandom,
    weather::WeatherChoice,
    world_generation::{WorldSize, generate_world_from_terrain_seed},
    world_memory::KnownRegion,
    world_placement::{PlacementRegion, footprint_exclusion},
};

fn fixture() -> Value {
    let value: Value =
        serde_json::from_str(include_str!("fixtures/world-components.json")).unwrap();
    assert_eq!(
        value["reference"],
        "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
    );
    value
}
fn indices(value: &Value) -> Vec<usize> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_u64().unwrap() as usize)
        .collect()
}

#[test]
fn map90_setup_bits_stations_bridges_and_exclusion_queries_equal_prototype() {
    // Original setupRails and railYard, including all eight directions, repeated
    // cells, nonadjacent hops, overlapping stations and signed edge yards.
    for case in fixture()["rails"].as_array().unwrap() {
        let rails = WorldRails::from_metadata(
            (!case["metadata"].is_null()).then_some(&case["metadata"]),
            case["chars"].as_str().unwrap().as_bytes(),
            20,
            20,
            3,
        )
        .unwrap();
        assert_eq!(
            rails.stations.len(),
            case["stations"].as_array().unwrap().len()
        );
        for (actual, expected) in rails
            .stations
            .iter()
            .zip(case["stations"].as_array().unwrap())
        {
            assert_eq!(actual.region, expected["region"].as_u64().unwrap() as usize);
            assert_eq!(actual.cell, expected["c"].as_u64().unwrap() as usize);
            assert_eq!(
                actual.x.to_bits(),
                expected["x"].as_f64().unwrap().to_bits()
            );
            assert_eq!(
                actual.z.to_bits(),
                expected["z"].as_f64().unwrap().to_bits()
            );
            assert_eq!(json!(actual.next), expected["next"]);
        }
        let lines = rails.lines.iter().map(|l|json!({"a":l.a,"b":l.b,"cells":l.cells,"regions":l.regions,"bridges":l.bridges})).collect::<Vec<_>>();
        assert_eq!(json!(lines), case["lines"]);
        assert_eq!(json!(rails.rail_at), case["railAt"]);
        assert_eq!(json!(rails.station_at), case["stationAt"]);
        assert_eq!(json!(rails.yard), case["yard"]);
        for c in 0..400 {
            let expected = case["excludes"][c].as_bool().unwrap();
            assert_eq!(rails.excludes_footprint(&[c]), expected, "cell {c}");
            // A multi-cell footprint is excluded when any member is reserved.
            assert_eq!(
                rails.excludes_footprint(&[c, (c + 1) % 400]),
                expected || case["excludes"][(c + 1) % 400].as_bool().unwrap()
            );
        }
    }
}

#[test]
fn map59_known_owned_visible_footprints_equal_prototype_placement_check() {
    // Actual placementCheck results using the test-world-territories sparse
    // explored-run fixture, adjacent friendly/enemy regions and legacy bounds.
    for case in fixture()["placements"].as_array().unwrap() {
        let regions = case["regions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| KnownRegion {
                id: r["id"].as_i64().unwrap() as i32,
                team: r["team"].as_i64().unwrap() as i32,
                name: None,
                kind: None,
                at: Point { x: 0., z: 0. },
                bounds: r["bounds"]
                    .as_array()
                    .map(|a| std::array::from_fn(|i| a[i].as_f64().unwrap())),
                runs: r["runs"].as_array().map_or_else(Vec::new, |runs| {
                    runs.iter()
                        .map(|run| std::array::from_fn(|i| run[i].as_u64().unwrap() as usize))
                        .collect()
                }),
                progress: 0.,
                capper: None,
                locked: false,
                contested: false,
            })
            .collect::<Vec<_>>();
        let at = Point {
            x: case["x"].as_f64().unwrap(),
            z: case["z"].as_f64().unwrap(),
        };
        let x = (at.x / CELL - 1.).round() as usize;
        let y = (at.z / CELL - 1.).round() as usize;
        let cells = [
            y * 16 + x,
            y * 16 + x + 1,
            (y + 1) * 16 + x,
            (y + 1) * 16 + x + 1,
        ];
        let raw_regions = case["regions"].as_array().unwrap();
        let views = regions
            .iter()
            .zip(raw_regions)
            .map(|(r, raw)| PlacementRegion {
                team: r.team,
                bounds: r.bounds.as_ref(),
                runs: raw.get("runs").map(|_| r.runs.as_slice()),
            })
            .collect::<Vec<_>>();
        let hidden = indices(&case["hidden"]);
        let rails = WorldRails {
            yard: indices(&case["yard"])
                .into_iter()
                .map(|c| c as i64)
                .collect(),
            ..WorldRails::default()
        };
        let actual = footprint_exclusion(
            at,
            16,
            &cells,
            case["team"].as_i64().unwrap() as i32,
            &views,
            |p| {
                !(case["hiddenRaw"] == true && p == at)
                    && !hidden.contains(
                        &((p.z / CELL).floor() as usize * 16 + (p.x / CELL).floor() as usize),
                    )
            },
            &rails,
        );
        assert_eq!(
            actual.map(|v| v.reason()),
            case["reason"].as_str(),
            "{case}"
        );
    }
}

fn detached_world() -> Match {
    let seed = 7;
    let generated =
        generate_world_from_terrain_seed(seed, WorldSize::Huge, &[0, 1], false).unwrap();
    let map = AuthoritativeMap::from_world(generated.into_world_map_input()).unwrap();
    Match::new_world(
        &map,
        MatchOptions {
            seed: u64::from(seed),
            mode: Mode::World,
            players: [0, 1]
                .into_iter()
                .enumerate()
                .map(|(slot, team)| PlayerConfig {
                    name: format!("P{slot}"),
                    team,
                    faction: Faction::Germany,
                })
                .collect(),
            shuffle: false,
            army: ArmySize::Standard,
            weather: WeatherChoice::Map,
            weather_seed: Some(31),
            living_weather: false,
            map_key: None,
            tech: false,
            supply: false,
            logistics: false,
            sandbox: false,
        },
        MatchRandom::new(u64::from(seed)),
    )
    .unwrap()
}

#[test]
fn base46_detached_regional_bases_are_neutral_and_release_capture_locks_on_death() {
    let catalog = Catalog::compiled().unwrap();
    let def = catalog.unit("worldbase").unwrap();
    let expected = &fixture()["worldbase"];
    for field in ["cost", "hpPer", "size", "vision"] {
        assert_eq!(def.get(field).unwrap().number(), expected[field].as_f64());
    }
    assert_eq!(
        def.get("produces").unwrap().boolean(),
        expected["produces"].as_bool()
    );
    assert!(def.get("makes").is_none());
    assert_eq!(expected["buyable"], false);
    for table in ["BUILDABLE", "WORLD_BUILDS"] {
        assert!(
            !catalog
                .table(table)
                .unwrap()
                .array()
                .unwrap()
                .iter()
                .any(|v| v.string() == Some("worldbase"))
        );
    }
    let mut m = detached_world();
    let runtime = m.world_runtime.as_ref().unwrap();
    let neutral = runtime.regions.iter().filter(|r| r.team < 0).count();
    assert_eq!(runtime.guard_bases.len(), neutral);
    let (&base, &region) = runtime.guard_bases.iter().next().unwrap();
    for (&id, &index) in &runtime.guard_bases {
        let unit = &m.world.game.units[&id];
        assert_eq!(unit.owner, LOCAL_DEFENDER_OWNER);
        assert_eq!(m.world.game.team_of(unit), -1);
        assert_eq!(unit.hp, 600.);
        assert_eq!(unit.cells.as_ref().unwrap().len(), 9);
        assert_eq!(unit.built, Some(1.));
        assert_eq!(runtime.regions[index].team, -1);
        assert_eq!(m.rules.kinds[unit.kind].name, "worldbase");
    }
    let r = &m.world.geography().unwrap().regions[region];
    let at = Point {
        x: (r.x as f64 + 0.5) * CELL,
        z: (r.y as f64 + 0.5) * CELL,
    };
    let rifle = m
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 0 && m.rules.kinds[u.kind].name == "rifle")
        .unwrap()
        .id;
    // Isolate capture from incidental combat. Keep every constructor actor and
    // all production records; place noncapturing troops away from this objective.
    for unit in m.world.game.units.values_mut() {
        if !m.rules.kinds[unit.kind].building {
            unit.x = at.x + 50.;
            unit.z = at.z + 50.;
        }
    }
    let unit = m.world.game.units.get_mut(&rifle).unwrap();
    unit.x = at.x;
    unit.z = at.z;
    m.grid.rebuild(&m.world.game.units);
    let order = m.world.game.units.keys().copied().collect::<Vec<_>>();
    m.apply_world_capture_income(&order, 0.).unwrap();
    assert_eq!(
        m.world_runtime.as_ref().unwrap().regions[region].locked,
        Some(true)
    );
    assert_eq!(m.world_runtime.as_ref().unwrap().regions[region].team, -1);
    // Dead production buildings stop locking immediately, before retirement.
    m.world.game.units.get_mut(&base).unwrap().hp = 0.;
    let dt = catalog
        .table("CFG")
        .unwrap()
        .get("captureTime")
        .unwrap()
        .number()
        .unwrap();
    let captures = m.apply_world_capture_income(&order, dt).unwrap();
    assert!(
        captures
            .iter()
            .any(|capture| capture.region_index == region && capture.owner == 0)
    );
    let state = &m.world_runtime.as_ref().unwrap().regions[region];
    assert_eq!(state.locked, Some(false));
    assert_eq!(state.team, 0);
    assert_eq!(state.progress, 1.);
}
