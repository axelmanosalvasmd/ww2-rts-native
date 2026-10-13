use serde_json::{Value, json};
use ww2_core::{
    catalog::Catalog,
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::Point,
    map::AuthoritativeMap,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    unit_spawn,
    weather::WeatherChoice,
};
fn bits(n: f64) -> Value {
    json!(format!("{:016x}", n.to_bits()))
}
fn snapshot(m: &Match) -> Value {
    json!({"players":m.start.economy.players.iter().map(|p|json!([bits(p.mp),bits(p.vp),bits(p.inc),p.away,p.out])).collect::<Vec<_>>(),
        "points":m.start.points.iter().enumerate().map(|(i,p)|json!([p.owner.map_or(-1,|v|v as i64),p.capper.map_or(-1,|v|v as i64),bits(p.progress),p.cut,p.contested,p.on_point,m.capture_ticks[i]])).collect::<Vec<_>>()})
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
                    faction: Faction::try_from(0).unwrap(),
                },
                PlayerConfig {
                    name: "B".into(),
                    team: if c["allied"].as_bool().unwrap() { 0 } else { 1 },
                    faction: Faction::try_from(1).unwrap(),
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
#[test]
fn point_phases_match_original_block_on_assembled_matches() {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/match-economy.json")).unwrap();
    assert_eq!(
        fixture["reference"],
        "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
    );
    assert_eq!(fixture["node"], "v24.21.0");
    assert_eq!(fixture["arch"], "x64");
    let mut count = 0;
    for c in fixture["cases"].as_array().unwrap() {
        let mut m = make(c);
        for step in c["steps"].as_array().unwrap() {
            let a = &step["action"];
            m.world.game.tick = a["tick"].as_i64().unwrap();
            for change in a["moves"].as_array().into_iter().flatten() {
                let u = m
                    .world
                    .game
                    .units
                    .get_mut(&(change[0].as_u64().unwrap() as u32))
                    .unwrap();
                u.x = change[1].as_f64().unwrap();
                u.z = change[2].as_f64().unwrap();
            }
            for change in a["points"].as_array().into_iter().flatten() {
                let p = &mut m.start.points[change[0].as_u64().unwrap() as usize];
                p.owner = usize::try_from(change[1].as_i64().unwrap()).ok();
                p.capper = usize::try_from(change[2].as_i64().unwrap()).ok();
                p.progress = change[3].as_f64().unwrap();
            }
            for change in a["away"].as_array().into_iter().flatten() {
                m.start.economy.players[change[0].as_u64().unwrap() as usize].away =
                    change[1].as_bool().unwrap();
            }
            for change in a["out"].as_array().into_iter().flatten() {
                m.start.economy.players[change[0].as_u64().unwrap() as usize].out =
                    change[1].as_bool().unwrap();
            }
            for change in a["cells"].as_array().into_iter().flatten() {
                m.world
                    .set_cell(
                        change[0].as_u64().unwrap() as usize,
                        change[1].as_str().unwrap().as_bytes()[0],
                    )
                    .unwrap();
            }
            let mut ids: Vec<_> = m.world.game.units.keys().copied().collect();
            if !a["spawn"].is_null() {
                let s = &a["spawn"];
                let kind = m.rules.kind(s["kind"].as_str().unwrap()).unwrap();
                let id = m.next_unit_id;
                let u = unit_spawn::initial_unit(
                    &m.rules,
                    Catalog::compiled().unwrap(),
                    kind,
                    id,
                    s["owner"].as_u64().unwrap() as usize,
                    Point {
                        x: s["x"].as_f64().unwrap(),
                        z: s["z"].as_f64().unwrap(),
                    },
                )
                .unwrap();
                m.world.game.units.insert(id, u);
                m.next_unit_id += 1;
                if a["beforeSpawn"] != true {
                    ids = m.world.game.units.keys().copied().collect();
                }
            }
            m.apply_point_economy(&ids, a["winner"] == true).unwrap();
            assert_eq!(
                snapshot(&m),
                step["expected"],
                "army {} allied {} tick {}",
                c["army"],
                c["allied"],
                a["tick"]
            );
            count += 1;
        }
    }
    assert_eq!(count, 72);
}
#[test]
fn invalid_late_phase_keeps_objectives_and_wallets_unchanged() {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/match-economy.json")).unwrap();
    let mut m = make(&fixture["cases"][0]);
    let ids: Vec<_> = m.world.game.units.keys().copied().collect();
    let before = snapshot(&m);
    assert!(m.apply_point_economy(&[ids[0], ids[0]], false).is_err());
    assert_eq!(snapshot(&m), before);
    m.world.game.tick = 1;
    m.start.points[0].progress = 0.995;
    m.start.points[0].capper = Some(0);
    m.world.game.units.get_mut(&ids[0]).unwrap().x = m.start.points[0].at.x;
    m.world.game.units.get_mut(&ids[0]).unwrap().z = m.start.points[0].at.z;
    m.start.economy.players[0].mp = f64::NAN;
    let before = snapshot(&m);
    assert!(m.apply_point_economy(&ids, false).is_err());
    assert_eq!(snapshot(&m), before);
}
