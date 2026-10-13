//! Actual reference steps over friendly matches, avoiding unavailable combat actuators.
use serde_json::{Value, json};
use ww2_core::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::Point,
    map::AuthoritativeMap,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    weather::WeatherChoice,
};
fn bits(n: f64) -> Value {
    json!(format!("{:016x}", n.to_bits()))
}
fn make(c: &Value) -> Match {
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&c["map"]).unwrap()).unwrap();
    Match::new(
        &map,
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
                    team: 0,
                    faction: Faction::Germany,
                },
            ],
            shuffle: false,
            army: match c["army"].as_str().unwrap() {
                "standard" => ArmySize::Standard,
                "large" => ArmySize::Large,
                "massive" => ArmySize::Massive,
                "endless" => ArmySize::Endless,
                _ => unreachable!(),
            },
            weather: WeatherChoice::Map,
            weather_seed: Some(1),
            living_weather: false,
            map_key: None,
            tech: true,
            supply: true,
            logistics: false,
            sandbox: false,
        },
    )
    .unwrap()
}
fn snapshot(m: &Match) -> Value {
    let g = &m.world.game;
    json!({"tick":g.tick,"visionTick":g.vision_tick,"seed":m.start.environment_seed,"wind":[bits(m.start.wind.0),bits(m.start.wind.1)],
        "units":g.units.values().map(|u|json!([u.id,bits(u.x),bits(u.z),bits(u.rot),bits(u.hp),bits(u.supp),bits(u.stuck),bits(u.repath),bits(u.sprint),u.path.iter().map(|p|json!([bits(p.x),bits(p.z)])).collect::<Vec<_>>()])).collect::<Vec<_>>(),
        "players":m.start.economy.players.iter().map(|p|json!([bits(p.mp),bits(p.vp),bits(p.inc)])).collect::<Vec<_>>(),
        "points":m.start.points.iter().enumerate().map(|(i,p)|json!([p.owner.map_or(-1,|v|v as i64),p.capper.map_or(-1,|v|v as i64),bits(p.progress),p.cut,p.contested,p.on_point,m.capture_ticks[i]])).collect::<Vec<_>>()})
}
#[test]
fn connected_tick_phases_match_actual_friendly_reference_steps() {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/match-tick.json")).unwrap();
    for c in fixture["cases"].as_array().unwrap() {
        let mut m = make(c);
        let input = &c["input"];
        m.start.environment_seed = input["seed"].as_u64().unwrap() as u32;
        m.start.wind = (
            input["wind"][0].as_f64().unwrap(),
            input["wind"][1].as_f64().unwrap(),
        );
        for row in input["positions"].as_array().unwrap() {
            let u = m
                .world
                .game
                .units
                .get_mut(&(row[0].as_u64().unwrap() as u32))
                .unwrap();
            u.x = row[1].as_f64().unwrap();
            u.z = row[2].as_f64().unwrap();
        }
        for row in input["paths"].as_array().unwrap() {
            let u = m
                .world
                .game
                .units
                .get_mut(&(row[0].as_u64().unwrap() as u32))
                .unwrap();
            u.path = vec![Point {
                x: row[1].as_f64().unwrap(),
                z: row[2].as_f64().unwrap(),
            }];
            u.path_gen += 1;
        }
        let mut expected = c["checkpoints"].as_array().unwrap().iter().peekable();
        for _ in 0..200 {
            let report = m.advance_tick(false).unwrap();
            assert!(!report.unavailable.is_empty());
            if expected
                .peek()
                .is_some_and(|e| e["tick"].as_i64() == Some(report.tick))
            {
                assert_eq!(
                    snapshot(&m),
                    *expected.next().unwrap(),
                    "army {} tick {}",
                    c["army"],
                    report.tick
                );
            }
        }
        assert!(expected.next().is_none());
    }
}
#[test]
fn rejected_clock_does_not_consume_a_tick() {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/match-tick.json")).unwrap();
    let mut m = make(&fixture["cases"][0]);
    m.world.game.tick = i64::MAX;
    let before = snapshot(&m);
    assert!(m.advance_tick(false).is_err());
    assert_eq!(snapshot(&m), before);
    assert!(m.tick_state.fault().is_none());
}
