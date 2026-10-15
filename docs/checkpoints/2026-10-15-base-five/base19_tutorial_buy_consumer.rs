//! Pending owner union: install as a crate-local test module only after MODE's provider handoff.
//! This file is not registered or compiled on the f7 BASE candidate.
use crate::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::Point,
    map::AuthoritativeMap,
    match_commands::Command,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    weather::WeatherChoice,
};
use serde_json::{Value, json};

fn bits(n: f64) -> String {
    format!("{:016x}", n.to_bits())
}
fn point(p: Option<Point>) -> Value {
    p.map_or(Value::Null, |p| json!([bits(p.x), bits(p.z)]))
}
fn project(m: &Match) -> Value {
    json!({"tick":m.world.game.tick,"seed":m.start.environment_seed,"next":m.next_unit_id,
        "money":m.start.economy.players.iter().map(|p|
            json!([bits(p.mp),bits(p.mun),bits(p.fuel)])).collect::<Vec<_>>(),
        "spent":m.story.iter().map(|s| bits(s.mp_spent)).collect::<Vec<_>>(),
        "rally":point(m.player_rallies[0]),
        "units":m.world.game.units.values().map(|u| json!({
            "id":u.id,"owner":u.owner,"type":m.rules.kinds[u.kind].name,
            "hp":bits(u.hp),"pose":[bits(u.x),bits(u.z),bits(u.rot),
                bits(m.commands.units[&u.id].aim),bits(u.ghost)],
            "path":u.path.iter().map(|p| point(Some(*p))).collect::<Vec<_>>(),
            "goal":point(u.world_goal),"drift":u.drift,"cells":u.cells,
        })).collect::<Vec<_>>()})
}
fn make(map: &AuthoritativeMap) -> Match {
    Match::new_tutorial(map, MatchOptions {
        seed: 701, mode: Mode::Tutorial,
        players: vec![PlayerConfig { name: "me".into(), team: 0, faction: Faction::Germany }],
        shuffle: false, army: ArmySize::Standard, weather: WeatherChoice::Map,
        weather_seed: Some(1), living_weather: false, map_key: None,
        tech: false, supply: false, logistics: false, sandbox: false,
    }).unwrap()
}

#[test]
fn no_maker_buy_faces_home_rally_walks_and_preserves_state_on_mp_refusal() {
    let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../simtest/tests/fixtures/base19-tutorial-buy.json");
    let f: Value = serde_json::from_str(&std::fs::read_to_string(file)
        .expect("Root must authorize original BASE19 recording before native execution")).unwrap();
    assert_eq!(f["reference"], "e4e579a838aabae3b1d9dadbc17a0a87147bc52d");
    assert_eq!(f["seed"], 701);
    assert_eq!(f["input"]["names"], json!(["me"]));
    assert_eq!(f["input"]["teams"], json!([0]));
    assert_eq!(f["input"]["factions"], json!([1]));
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&f["input"]["map"]).unwrap()).unwrap();
    let cases = f["cases"].as_array().unwrap();
    assert_eq!(cases.iter().map(|c| c["name"].as_str().unwrap()).collect::<Vec<_>>(),
        ["map-centre-facing", "personal-rally"]);
    let mut commands = 0;
    for case in cases {
        let mut m = make(&map);
        assert_eq!(project(&m), case["initial"]);
        assert_eq!(m.world.game.units.len(), 3);
        assert!(m.building_cells.is_empty());
        let random = m.start.random.snapshot();
        for row in case["records"].as_array().unwrap() {
            match row["op"].as_str().unwrap() {
                "command" => {
                    assert_eq!(project(&m), row["before"]);
                    let cmd = Command::from_json(&row["cmd"]).unwrap();
                    let before = (m.world.game.units.clone(), m.commands.units.clone(),
                        m.production.clone(), m.story.clone(), m.next_unit_id,
                        m.start.economy.players.iter().map(|p|
                            [p.mp.to_bits(),p.mun.to_bits(),p.fuel.to_bits()]).collect::<Vec<_>>(),
                        m.building_cells.clone(), m.recruit_metadata.clone());
                    let result = m.execute_command(0, &cmd, false).unwrap();
                    assert_eq!(json!(result.map(|d| d.name())), row["result"]);
                    assert_eq!(project(&m), row["after"]);
                    if result.is_some() {
                        assert_eq!(row["cmd"], json!({"t":"buy","unit":"at"}));
                        assert_eq!(row["result"], "mp");
                        let after = (m.world.game.units.clone(), m.commands.units.clone(),
                            m.production.clone(), m.story.clone(), m.next_unit_id,
                            m.start.economy.players.iter().map(|p|
                                [p.mp.to_bits(),p.mun.to_bits(),p.fuel.to_bits()]).collect::<Vec<_>>(),
                            m.building_cells.clone(), m.recruit_metadata.clone());
                        assert_eq!(before, after, "MP denial must preserve canonical actor/control/production/story/wallet/ID/footprint/recruit state");
                    }
                    if let Command::Buy { kind, from } = &cmd {
                        if result.is_none() {
                            assert_eq!(kind, "rifle");
                            assert_eq!(*from, Some(u32::MAX));
                            let id = before.4;
                            assert_eq!(json!(id), case["bought"]);
                            assert_eq!(m.next_unit_id, id + 1);
                            assert_eq!(m.world.game.units.len(), before.0.len() + 1);
                            assert_eq!(m.recruit_metadata[&id].building, 0);
                            assert!(m.building_cells.is_empty());
                            assert_eq!(m.start.economy.players[0].mp, 50.0);
                            assert_eq!(m.story[0].mp_spent, 100.0);
                            let u = &m.world.game.units[&id];
                            let home = m.start.players[0].spawn;
                            let to = m.player_rallies[0].unwrap_or(Point {
                                x: m.world.game.w as f64 * crate::game::CELL / 2.0,
                                z: m.world.game.h as f64 * crate::game::CELL / 2.0,
                            });
                            assert_eq!(u.rot.to_bits(), crate::math::atan2(to.z-home.z,to.x-home.x).to_bits());
                            assert_eq!(u.world_goal, m.player_rallies[0]);
                            assert_eq!(u.drift, m.player_rallies[0].is_some());
                            assert!(u.ghost > 0.0);
                        }
                    }
                    commands += 1;
                }
                "walk" => {
                    assert_eq!(case["name"], "personal-rally");
                    assert_eq!(row["limit"], 300);
                    let id = case["bought"].as_u64().unwrap() as u32;
                    let before = m.world.game.units[&id].at();
                    let frames = row["frames"].as_array().unwrap();
                    assert!(!frames.is_empty() && frames.len() <= 300);
                    for frame in frames {
                        m.advance_tick().unwrap();
                        assert_eq!(project(&m), *frame);
                        assert_eq!(m.start.random.snapshot(), random);
                    }
                    let u = &m.world.game.units[&id];
                    assert_ne!(u.at(), before);
                    assert!(u.path.is_empty());
                    let rally = m.player_rallies[0].unwrap();
                    assert!(crate::math::hypot(u.x-rally.x,u.z-rally.z) < 3.0);
                }
                _ => panic!("unexpected BASE19 operation"),
            }
            assert_eq!(m.start.random.snapshot(), random);
        }
        assert_eq!(project(&m), case["final"]);
    }
    assert_eq!(commands, 5);
}
