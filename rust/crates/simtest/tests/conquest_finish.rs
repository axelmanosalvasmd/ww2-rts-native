use serde_json::{Value, json};
use ww2_core::{
    conquest_end::ConquestEndReason,
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
            players: c["teams"]
                .as_array()
                .unwrap()
                .iter()
                .enumerate()
                .map(|(i, t)| PlayerConfig {
                    name: format!("P{i}"),
                    team: t.as_i64().unwrap() as i32,
                    faction: Faction::try_from((i % 4) as u8).unwrap(),
                })
                .collect(),
            shuffle: false,
            army: ArmySize::Standard,
            weather: WeatherChoice::Map,
            weather_seed: Some(1),
            living_weather: false,
            map_key: None,
            tech: true,
            supply: false,
            logistics: false,
            sandbox: false,
        },
    )
    .unwrap()
}
fn snapshot(m: &Match) -> Value {
    json!({"winner":m.finish.map(|f|f.winner),"reason":m.finish.map(|f|f.reason.as_str()),"endAt":m.finish.map(|f|json!({"x":bits(f.end_at.x),"z":bits(f.end_at.z)})),"endTick":m.finish.map(|f|f.end_tick),"reveal":m.world.game.reveal,"timeline":m.timeline.iter().map(|s|json!({"t":bits(s.t),"vp":s.vp.iter().map(|v|bits(*v)).collect::<Vec<_>>()})).collect::<Vec<_>>(),"captures":m.story.iter().map(|v|bits(v.captures)).collect::<Vec<_>>(),"players":m.start.economy.players.iter().map(|p|json!([bits(p.mp),bits(p.vp),bits(p.inc)])).collect::<Vec<_>>(),"captureTicks":m.capture_ticks})
}
fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/conquest-finish.json")).unwrap()
}
#[test]
fn assembled_match_finish_matches_original_conquest_and_story_functions() {
    let f = fixture();
    assert_eq!(f["reference"], "e4e579a838aabae3b1d9dadbc17a0a87147bc52d");
    assert_eq!(f["node"], "v24.21.0");
    assert_eq!(f["arch"], "x64");
    let mut count = 0;
    for c in f["cases"].as_array().unwrap() {
        let mut m = make(c);
        for step in c["steps"].as_array().unwrap() {
            let a = &step["action"];
            m.world.game.tick = a["tick"].as_i64().unwrap();
            for change in a["vp"].as_array().into_iter().flatten() {
                m.start.economy.players[change[0].as_u64().unwrap() as usize].vp =
                    change[1].as_f64().unwrap();
            }
            for change in a["out"].as_array().into_iter().flatten() {
                m.start.economy.players[change[0].as_u64().unwrap() as usize].out =
                    change[1].as_bool().unwrap();
            }
            for change in a["away"].as_array().into_iter().flatten() {
                m.start.economy.players[change[0].as_u64().unwrap() as usize].away =
                    change[1].as_bool().unwrap();
            }
            for p in a["points"].as_array().into_iter().flatten() {
                let i = p["index"].as_u64().unwrap() as usize;
                let owner = p["owner"].as_i64().unwrap();
                m.start.points[i].owner = (owner >= 0).then_some(owner as usize);
                let capper = p["capper"].as_i64().unwrap_or(owner);
                m.start.points[i].capper = (capper >= 0).then_some(capper as usize);
                m.start.points[i].progress =
                    p["progress"]
                        .as_f64()
                        .unwrap_or(if owner >= 0 { 1.0 } else { 0.0 });
                if let Some(tick) = p.get("capTick") {
                    m.capture_ticks[i] = Some(tick.as_i64().unwrap());
                }
                if let Some(at) = p.get("at") {
                    m.start.points[i].at = Point {
                        x: at["x"].as_f64().unwrap(),
                        z: at["z"].as_f64().unwrap(),
                    };
                }
            }
            if a["kill"].as_bool().unwrap_or(false) {
                for u in m.world.game.units.values_mut() {
                    u.hp = 0.0;
                }
            }
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
            if a["periodic"].as_bool().unwrap_or(false) {
                m.sample_conquest_tick().unwrap();
            }
            if a["pointsPhase"].as_bool().unwrap_or(false) {
                let list = m.world.game.units.keys().copied().collect::<Vec<_>>();
                m.apply_conquest_point_economy(&list).unwrap();
            }
            m.apply_conquest_victory().unwrap();
            assert_eq!(
                snapshot(&m),
                step["expected"],
                "{} tick{}",
                c["name"],
                a["tick"]
            );
            count += 1;
        }
    }
    assert_eq!(count, 24);
}
#[test]
fn invalid_state_never_partially_finishes_or_samples() {
    let f = fixture();
    let case = &f["cases"][0];
    let mut m = make(case);
    m.world.game.tick = 200;
    m.start.economy.players[0].vp = 1200.0;
    let before = snapshot(&m);
    m.start.economy.players[1].vp = f64::NAN;
    assert!(m.apply_conquest_victory().is_err());
    assert!(m.finish.is_none());
    assert!(m.timeline.is_empty());
    assert!(!m.world.game.reveal);
    m.start.economy.players[1].vp = 0.0;
    assert_eq!(snapshot(&m), before);
    m.world.game.players[1].team = 0;
    assert!(m.apply_conquest_victory().is_err());
    assert!(m.sample_conquest_tick().is_err());
    assert!(m.apply_conquest_point_economy(&[]).is_err());
    assert_eq!(snapshot(&m), before);
    m.world.game.players[1].team = 1;
    m.start.points[2].owner = Some(90);
    assert!(m.apply_conquest_victory().is_err());
    assert_eq!(snapshot(&m), before);
    m.start.points[2].owner = None;
    m.start.points[2].at.x = f64::INFINITY;
    assert!(m.apply_conquest_victory().is_err());
    assert_eq!(snapshot(&m), before);
    m.start.points[2].at.x = 17.0;
    m.start.win_vp = 0.0;
    assert!(m.apply_conquest_victory().is_err());
    assert_eq!(snapshot(&m), before);
    m.start.win_vp = 1200.0;
    m.capture_ticks[1] = Some(i64::MAX);
    assert!(m.apply_conquest_victory().is_err());
    assert!(m.finish.is_none());
    assert!(m.timeline.is_empty());
    m.capture_ticks[1] = None;
    m.start.points[0].owner = Some(0);
    m.start.points[0].at.x = f64::MAX;
    assert!(m.apply_conquest_victory().is_err());
    assert!(m.finish.is_none());
    assert!(m.timeline.is_empty());
    assert!(!m.world.game.reveal);
    m.start.points[0].owner = None;
    m.start.points[0].at.x = 17.0;
    assert_eq!(snapshot(&m), before);
    m.options.mode = Mode::Assault;
    assert!(m.apply_conquest_victory().is_err());
    assert!(m.sample_conquest_tick().is_err());
    assert!(m.apply_conquest_point_economy(&[]).is_err());
    assert_eq!(snapshot(&m), before);
}
#[test]
fn first_finish_stays_canonical_without_eliminating_losers() {
    let f = fixture();
    let mut m = make(&f["cases"][0]);
    m.world.game.tick = 19;
    m.start.economy.players[0].vp = 1200.0;
    let first = m.apply_conquest_victory().unwrap().unwrap();
    assert_eq!(first.reason, ConquestEndReason::VictoryPoints);
    let sample_count = m.timeline.len();
    let units = m.world.game.units.len();
    m.world.game.tick = 200;
    m.start.economy.players[1].vp = 9999.0;
    assert_eq!(m.apply_conquest_victory().unwrap(), None);
    assert!(!m.sample_conquest_tick().unwrap());
    assert_eq!(m.finish, Some(first));
    assert_eq!(m.timeline.len(), sample_count);
    assert_eq!(m.world.game.units.len(), units);
    assert!(m.start.economy.players.iter().all(|p| !p.out));
}
