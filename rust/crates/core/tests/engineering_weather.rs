//! Named engineering predicates against existing native providers and pinned Source fixtures.
//! Source recorder and native execution are pending the coordinator's slot grant.

use serde_json::{Value, json};
use ww2_core::{
    catalog::Catalog,
    game::{Game, LAND, MUD, PlaneState, Point},
    map::AuthoritativeMap,
    movement::weather_speed,
    rules::Rules,
    terrain::{ground_mul, speed_mul},
    unit_spawn::{initial_aircraft, initial_unit},
    vision::{vision_of, vision_range},
    weather::{MapWeatherInfo, WeatherChoice, WeatherKind, WeatherPlan, WeatherRow, WeatherRules},
    world::World,
};

fn game() -> Game {
    let map = AuthoritativeMap::from_json(
        &serde_json::to_vec(&json!({
            "name": "Engineering weather predicates",
            "w": 20, "h": 20, "rows": vec!["...................."; 20],
            "spawns": [{"x": 2, "y": 2}, {"x": 17, "y": 17}],
            "points": [{"x": 10, "y": 10}]
        }))
        .unwrap(),
    )
    .unwrap();
    World::from_map(&map).unwrap().game
}

fn fixture() -> Value {
    let value: Value = serde_json::from_str(include_str!("fixtures/weather.json")).unwrap();
    assert_eq!(value["reference"], "e4e579a");
    value
}

fn same_bits(actual: f64, expected: f64, context: &str) {
    assert_eq!(actual.to_bits(), expected.to_bits(), "{context}");
}

#[test]
fn eng59_mud_depth_and_wetness_reach_the_unit_speed_provider() {
    // shared/sim.js:85,2363-2371; test.js "Roads and mud".
    let rules = Rules::compiled().unwrap();
    let mut g = game();
    g.flags.fill(LAND | MUD);
    g.chars.fill(b'M');
    let tank = initial_unit(
        &rules,
        Catalog::compiled().unwrap(),
        rules.kind("tank").unwrap(),
        1,
        0,
        Point { x: 19.0, z: 19.0 },
    )
    .unwrap();
    let rifle = initial_unit(
        &rules,
        Catalog::compiled().unwrap(),
        rules.kind("rifle").unwrap(),
        2,
        0,
        Point { x: 19.0, z: 19.0 },
    )
    .unwrap();
    let cell = g.cell_of(tank.x, tank.z).unwrap();
    let mut checked = 0;
    for (depth, dry_speed) in [
        (0.0, 0.7),
        (0.5, f64::from_bits(0x3fe0_cccc_cccc_cccc)),
        (1.0, 0.35),
    ] {
        g.wear.as_mut().unwrap().fill(depth);
        for (wet, wet_factor) in [
            (None, 1.0),
            (Some(0.0), 1.0),
            (Some(0.5), 0.9),
            (Some(1.0), 0.8),
        ] {
            g.wet = wet;
            // The named values are the Source's shallow, middle and deep mud rules.
            let expected = dry_speed * wet_factor;
            same_bits(
                ground_mul(&g, &rules, cell, true),
                expected,
                "mud vehicle cell",
            );
            same_bits(
                ground_mul(&g, &rules, cell, false),
                1.0,
                "mud infantry cell",
            );
            // Five identical samples still take the production footprint-averaging path.
            let average = (expected + expected + expected + expected + expected) / 5.0;
            same_bits(speed_mul(&g, &rules, &tank), average, "tank footprint");
            same_bits(speed_mul(&g, &rules, &rifle), 1.0, "rifle footprint");
            checked += 1;
        }
    }
    assert_eq!(checked, 12);
}

#[test]
fn eng73_five_weather_definitions_and_player_descriptions() {
    // shared/weather.js:10-19,122-129. Rain/Mud off-road effects are applied by wet ground.
    let catalog = Catalog::compiled().unwrap();
    let table = catalog.table("WEATHER").unwrap();
    let rules = WeatherRules::compiled().unwrap();
    let expected = [
        ("clear", 1.0, 1.0, 1.0, 1.0),
        ("fog", 0.7, 1.0, 1.0, 1.0),
        ("rain", 0.8, 1.0, 1.0, 0.8),
        ("mud", 1.0, 0.9, 1.0, 0.8),
        ("snow", 0.9, 0.9, 0.85, 1.0),
    ];
    assert_eq!(
        catalog
            .table("WEATHER_KINDS")
            .unwrap()
            .array()
            .unwrap()
            .len(),
        5
    );
    for (name, sight, infantry, vehicle, off_road) in expected {
        let row = table.get(name).unwrap();
        for (key, value) in [
            ("sight", sight),
            ("inf", infantry),
            ("veh", vehicle),
            ("offRoad", off_road),
        ] {
            same_bits(row.get(key).unwrap().number().unwrap(), value, name);
        }
    }
    let f = fixture();
    let descriptions = f["effects"].as_array().unwrap();
    assert_eq!(descriptions.len(), 6, "five kinds and missing weather");
    for row in descriptions {
        let kind = row["kind"]
            .as_str()
            .map(|name| WeatherKind::try_from(name).unwrap());
        assert_eq!(json!(rules.weather_effects(kind)), row["effects"]);
    }
    assert_eq!(
        rules.weather_effects(Some(WeatherKind::Rain)),
        ["sight -20%", "vehicles -20% off roads"]
    );
    assert_eq!(
        rules.weather_effects(Some(WeatherKind::Mud)),
        ["infantry -10%", "vehicles -20% off roads"]
    );
}

#[test]
fn eng74_weather_applies_to_ground_kinds_and_spares_aircraft() {
    // shared/weather.js:81-82,105-109; shared/sim.js visionOf and visionRange.
    let rules = Rules::compiled().unwrap();
    let catalog = Catalog::compiled().unwrap();
    let mut g = game();
    let mut checked = 0;
    for (name, sight, infantry, vehicle, rain) in [
        ("clear", 1.0, 1.0, 1.0, 0.0),
        ("fog", 0.7, 1.0, 1.0, 0.0),
        ("rain", 1.0, 1.0, 1.0, 1.0),
        ("mud", 1.0, 0.9, 1.0, 0.0),
        ("snow", 0.9, 0.9, 0.85, 0.0),
    ] {
        g.weather = Some(rules.weather_kind(name).unwrap());
        g.rain = rain;
        same_bits(g.sight_mul(&rules), sight, name);
        for (index, kind) in rules.kinds.iter().enumerate() {
            let at = Point { x: 19.0, z: 19.0 };
            let mut unit = if kind.air {
                initial_aircraft(&rules, catalog, index, 1, 0, at)
            } else {
                initial_unit(&rules, catalog, index, 1, 0, at)
            }
            .unwrap();
            // Completed buildings and airborne aircraft exercise the actual vision exemptions.
            if kind.building {
                unit.built = Some(1.0);
            }
            if kind.air {
                unit.air = Some(PlaneState::Station);
            }
            let expected_speed = if kind.air {
                1.0
            } else if kind.infantry {
                infantry
            } else {
                vehicle
            };
            same_bits(weather_speed(&g, &rules, kind), expected_speed, &kind.name);
            let expected_sight = kind.vision * if kind.air { 1.0 } else { sight };
            same_bits(vision_of(&g, &rules, &unit), expected_sight, &kind.name);
            if !kind.air {
                same_bits(
                    vision_range(&g, &rules, &unit),
                    expected_sight * (1.0 - 0.2 * rain),
                    &kind.name,
                );
            }
            checked += 1;
        }
    }
    assert_eq!(checked, 5 * 47);
    g.weather = None;
    same_bits(g.sight_mul(&rules), 1.0, "missing weather");
    for kind in &rules.kinds {
        same_bits(
            weather_speed(&g, &rules, kind),
            1.0,
            "missing weather speed",
        );
    }
}

#[test]
fn eng75_map_fallbacks_and_random_change_branches_are_present() {
    // The existing replay checks all 158 metadata cases and 5,682 complete plans.
    // This check binds named acceptance predicates to that unchanged Source corpus.
    let f = fixture();
    assert_eq!(f["metadata"].as_array().unwrap().len(), 158);
    assert_eq!(f["plans"].as_array().unwrap().len(), 5682);
    let rules = WeatherRules::compiled().unwrap();
    let winter = MapWeatherInfo {
        name: Some("Winter crossing"),
        ..Default::default()
    };
    let dawn = MapWeatherInfo {
        name: Some("River crossing"),
        ..Default::default()
    };
    assert_eq!(
        rules.map_weather(winter, None),
        WeatherPlan::fixed(WeatherKind::Snow)
    );
    assert_eq!(
        rules.map_weather(dawn, None),
        WeatherPlan {
            now: WeatherKind::Fog,
            next: Some(WeatherKind::Clear),
            at: 4800
        }
    );
    assert_eq!(
        rules.plan_weather(
            WeatherChoice::from_reference(Some("made-up")),
            dawn,
            None,
            17
        ),
        rules.map_weather(dawn, None)
    );
    let explicit = MapWeatherInfo {
        weather: Some("rain"),
        ..winter
    };
    assert_eq!(
        rules.map_weather(explicit, None),
        WeatherPlan::fixed(WeatherKind::Rain)
    );
    let mut starts = std::collections::BTreeSet::new();
    let (mut fog_changes, mut rain_changes, mut held) = (0, 0, 0);
    for row in f["plans"].as_array().unwrap().iter().take(4096) {
        assert_eq!(row[0], "random");
        let seed = row[2].as_u64().unwrap() as u32;
        let map = &f["metadata"][0];
        let info = MapWeatherInfo {
            name: map["map"]["name"].as_str(),
            mood: map["map"]["mood"].as_str(),
            weather: map["map"]["weather"].as_str(),
        };
        let plan = rules.plan_weather(WeatherChoice::Random, info, map["key"].as_str(), seed);
        assert_eq!(
            plan,
            rules.plan_weather(WeatherChoice::Random, info, map["key"].as_str(), seed)
        );
        starts.insert(plan.now.name());
        match (plan.now, plan.next) {
            (WeatherKind::Fog, Some(WeatherKind::Clear)) => {
                assert!((4200..5400).contains(&plan.at));
                fog_changes += 1;
            }
            (WeatherKind::Rain, Some(WeatherKind::Mud)) => {
                assert!((6000..8400).contains(&plan.at));
                rain_changes += 1;
            }
            (_, None) => {
                assert_eq!(plan.at, 0);
                held += 1;
            }
            _ => panic!("unexpected weather transition: {plan:?}"),
        }
    }
    assert_eq!(starts.len(), 5);
    assert!(fog_changes > 0 && rain_changes > 0 && held > 0);
    assert_eq!(fog_changes + rain_changes + held, 4096);
}

#[test]
fn eng76_warning_boundary_and_single_change_are_explicit() {
    // shared/weather.js:73-77,113-119; ordinary snapshot transport is separate.
    assert_eq!(fixture()["transitions"].as_array().unwrap().len(), 590);
    let rules = WeatherRules::compiled().unwrap();
    let mut plan = WeatherPlan {
        now: WeatherKind::Fog,
        next: Some(WeatherKind::Clear),
        at: 4800,
    };
    assert_eq!(
        rules.weather_row(Some(&plan), 4599),
        WeatherRow::Current(WeatherKind::Fog)
    );
    for (tick, seconds) in [
        (4600, 10),
        (4601, 10),
        (4619, 10),
        (4620, 9),
        (4799, 1),
        (4800, 0),
    ] {
        assert_eq!(
            rules.weather_row(Some(&plan), tick),
            WeatherRow::Changing {
                now: WeatherKind::Fog,
                next: WeatherKind::Clear,
                seconds
            }
        );
    }
    assert!(!plan.step(4799));
    assert!(plan.step(4800));
    for tick in [4800, 4801, 5000, u64::MAX] {
        assert!(!plan.step(tick));
        assert_eq!(plan, WeatherPlan::fixed(WeatherKind::Clear));
        assert_eq!(
            rules.weather_row(Some(&plan), tick),
            WeatherRow::Current(WeatherKind::Clear)
        );
    }
}
