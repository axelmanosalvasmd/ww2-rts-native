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
                    team: if c["enemy"] == true { 1 } else { 0 },
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
        for unit in m.world.game.units.values_mut() {
            unit.hold_pos = true;
        }
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
        if let Some(changes) = input["changes"].as_array() {
            for row in changes {
                m.world
                    .set_cell(
                        row[0].as_u64().unwrap() as usize,
                        row[1].as_str().unwrap().as_bytes()[0],
                    )
                    .unwrap();
            }
        }
        if let Some(fires) = input["fires"].as_array() {
            for row in fires {
                m.world
                    .fires
                    .insert(row[0].as_u64().unwrap() as usize, row[1].as_f64().unwrap());
            }
        }
        if let Some(smokes) = input["smokes"].as_array() {
            for cloud in smokes {
                m.world.game.smokes.push(ww2_core::game::Smoke {
                    id: cloud["id"].as_u64().unwrap() as u32,
                    x: cloud["x"].as_f64().unwrap(),
                    z: cloud["z"].as_f64().unwrap(),
                    r: cloud["r"].as_f64().unwrap(),
                    remaining: Some(cloud["t"].as_f64().unwrap()),
                });
            }
        }
        if let Some(commands) = input["commands"].as_array() {
            for row in commands {
                m.enqueue_command(
                    ww2_core::orders::OrderKey {
                        tick: row["tick"].as_u64().unwrap(),
                        issuer: row["issuer"].as_u64().unwrap() as u32,
                        sequence: row["sequence"].as_u64().unwrap(),
                    },
                    ww2_core::match_commands::Command::from_json(&row["command"]).unwrap(),
                )
                .unwrap();
            }
        }
        let mut expected = c["checkpoints"].as_array().unwrap().iter().peekable();
        for _ in 0..c["ticks"].as_u64().unwrap_or(200) {
            let report = m.advance_tick().unwrap();
            assert!(!report.unavailable.is_empty());
            if expected
                .peek()
                .is_some_and(|e| e["tick"].as_i64() == Some(report.tick))
            {
                let mut state = snapshot(&m);
                if input["commands"].is_array() {
                    use ww2_core::match_commands::MoveOutcome;
                    state["control"] = json!(
                        m.world
                            .game
                            .units
                            .values()
                            .map(|u| {
                                let c = &m.commands.units[&u.id];
                                json!([
                                    u.id,
                                    c.outcome.map(|o| match o {
                                        MoveOutcome::Interrupted => "interrupted",
                                        MoveOutcome::Moving => "moving",
                                        MoveOutcome::Unreachable => "unreachable",
                                        MoveOutcome::Arrived => "completed",
                                    }),
                                    c.outcome_tick,
                                    c.orders.len(),
                                    u.world_goal.map(|p| [bits(p.x), bits(p.z)]),
                                    u.amove.map(|p| [bits(p.x), bits(p.z)])
                                ])
                            })
                            .collect::<Vec<_>>()
                    );
                }
                if input["fires"].is_array() {
                    state["environment"] = json!({
                        "fires":m.world.fires.iter().map(|(c,left)|json!([c,bits(*left)])).collect::<Vec<_>>(),
                        "smokes":m.world.game.smokes.iter().map(|s|json!([s.id,bits(s.x),bits(s.z),bits(s.r),bits(s.remaining.unwrap())])).collect::<Vec<_>>(),
                        "chars":String::from_utf8(m.world.game.chars.clone()).unwrap(),"burnt":m.world.burnt
                    });
                }
                assert_eq!(
                    state,
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
    assert!(m.advance_tick().is_err());
    assert_eq!(snapshot(&m), before);
    assert!(m.tick_state.fault().is_none());
}

#[test]
fn late_terrain_change_does_not_recompute_the_earlier_supply_beat() {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/match-tick.json")).unwrap();
    let mut c = fixture["ordering"].clone();
    c["enemy"] = json!(true);
    let mut m = make(&c);
    m.start.points[0].owner = Some(0);
    m.start.points[0].capper = Some(0);
    m.start.points[0].progress = 1.0;
    for step in c["steps"].as_array().unwrap() {
        m.world.game.tick = step["tick"].as_i64().unwrap();
        let ids: Vec<_> = m.world.game.units.keys().copied().collect();
        m.apply_point_supply(&ids).unwrap();
        for change in step["changes"].as_array().unwrap() {
            m.world
                .set_cell(
                    change[0].as_u64().unwrap() as usize,
                    change[1].as_str().unwrap().as_bytes()[0],
                )
                .unwrap();
        }
        m.apply_point_capture_income(&ids).unwrap();
        let state = snapshot(&m);
        assert_eq!(
            json!({"players":state["players"],"points":state["points"]}),
            step["expected"]
        );
    }
    assert!(m.start.points[0].cut);
}
