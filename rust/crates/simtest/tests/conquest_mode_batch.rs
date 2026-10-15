//! One original-Source component fixture covers capture, score, goal, victory and terminal rules.
//! Separate native random streams are retained; this never compares whole-match trajectories.
use serde_json::{Value, json};
use ww2_core::{
    deployment,
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::Point,
    map::AuthoritativeMap,
    match_commands::{Command, Denial},
    match_end::MatchSample,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    match_story::StoryKey,
    objectives::{self, Objective, ObjectivePlayer, ObjectiveUnit},
    weather::{WeatherChoice, WeatherKind},
};
fn bits(v: f64) -> Value {
    json!(format!("{:016x}", v.to_bits()))
}
fn encode(v: Value) -> Value {
    match v {
        Value::Number(n) => bits(n.as_f64().unwrap()),
        Value::Array(a) => Value::Array(a.into_iter().map(encode).collect()),
        Value::Object(o) => Value::Object(o.into_iter().map(|(k, v)| (k, encode(v))).collect()),
        v => v,
    }
}
fn slot(v: &Value) -> Option<usize> {
    v.as_i64().and_then(|n| (n >= 0).then_some(n as usize))
}
fn index(v: Option<usize>) -> Value {
    json!(v.map_or(-1, |n| n as i64))
}
fn make(f: &Value) -> Match {
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&f["map"]).unwrap()).unwrap();
    Match::new(
        &map,
        MatchOptions {
            seed: f["seed"].as_u64().unwrap(),
            mode: Mode::Conquest,
            players: [Faction::Usa, Faction::Germany]
                .into_iter()
                .enumerate()
                .map(|(i, faction)| PlayerConfig {
                    name: if i == 0 { "Host" } else { "Guest" }.into(),
                    team: i as i32,
                    faction,
                })
                .collect(),
            shuffle: true,
            army: ArmySize::Standard,
            weather: WeatherChoice::Fixed(WeatherKind::Clear),
            weather_seed: None,
            living_weather: true,
            map_key: Some("default".into()),
            tech: true,
            supply: true,
            logistics: false,
            sandbox: false,
        },
    )
    .unwrap()
}
fn state(m: &Match) -> Value {
    encode(json!({
        "tick":m.world.game.tick,"goal":m.start.win_vp,"winner":m.finish.map(|f|f.winner),"reason":m.finish.map(|f|f.reason.as_str()),"at":m.finish.map(|f|json!({"x":f.end_at.x,"z":f.end_at.z})),"endTick":m.finish.map(|f|f.end_tick),"reveal":m.world.game.reveal,
        "players":m.start.economy.players.iter().map(|p|json!({"mp":p.mp,"vp":p.vp,"inc":p.inc})).collect::<Vec<_>>(),
        "points":m.start.points.iter().map(|p|json!({"owner":index(p.owner),"capper":index(p.capper),"progress":p.progress,"cut":p.cut})).collect::<Vec<_>>(),
        "story":m.story.iter().map(|p|StoryKey::ALL.into_iter().map(|key|(key.as_str().to_owned(),json!(p.value(key)))).collect::<serde_json::Map<String,Value>>()).collect::<Vec<_>>(),
        "timeline":m.timeline.iter().map(|s|match s {MatchSample::Conquest {t,vp}=>json!({"t":t,"vp":vp}),_=>panic!("Conquest sample")}).collect::<Vec<_>>()
    }))
}
#[test]
fn conquest_capture_score_and_terminal_rules_match_pinned_source() {
    let f: Value = serde_json::from_str(include_str!("fixtures/conquest-mode-batch.json")).unwrap();
    assert_eq!(f["reference"], "e4e579a838aabae3b1d9dadbc17a0a87147bc52d");
    assert_eq!(f["node"], "v24.21.0");
    assert_eq!(f["arch"], "x64");
    assert_eq!(f["kind"], "isolated-components-not-whole-match");
    let mut checkpoints = 0;
    for c in f["captures"].as_array().unwrap() {
        let players = c["players"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| ObjectivePlayer {
                team: p["team"].as_i64().unwrap() as i32,
                out: p["out"].as_bool().unwrap(),
                spawn: Point { x: 0., z: 0. },
            })
            .collect::<Vec<_>>();
        let units = c["units"]
            .as_array()
            .unwrap()
            .iter()
            .map(|u| ObjectiveUnit {
                id: u["id"].as_u64().unwrap() as u32,
                owner: u["owner"].as_u64().unwrap() as usize,
                kind: u["type"].as_str().unwrap(),
                at: Point {
                    x: u["x"].as_f64().unwrap(),
                    z: u["z"].as_f64().unwrap(),
                },
                hp: u["hp"].as_f64().unwrap(),
                airborne: false,
                riding: u["riding"].as_u64().unwrap() != 0,
                retreating: u["retreating"].as_bool().unwrap(),
                forced_reserve: u["logistics"]["forced"] == true,
            })
            .collect::<Vec<_>>();
        let mut points = c["points"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| Objective {
                id: p["id"].clone(),
                at: Point {
                    x: p["x"].as_f64().unwrap(),
                    z: p["z"].as_f64().unwrap(),
                },
                vp: p["vp"].as_f64().unwrap(),
                mp: p["mp"].as_f64().unwrap(),
                kind: None,
                needs: slot(&p["needs"]),
                owner: slot(&p["owner"]),
                capper: slot(&p["capper"]),
                progress: p["progress"].as_f64().unwrap(),
                cut: false,
                on_point: vec![],
                contested: false,
            })
            .collect::<Vec<_>>();
        let mut captures = vec![0.; players.len()];
        for expected in c["expected"].as_array().unwrap() {
            for event in objectives::capture_points(&mut points, &units, &players, 0.05).unwrap() {
                captures[event.owner] += 1.;
            }
            let actual = encode(
                json!({"points":points.iter().map(|p|json!({"owner":index(p.owner),"capper":index(p.capper),"progress":p.progress,"onPoint":p.on_point,"contested":p.contested})).collect::<Vec<_>>(),"captures":captures}),
            );
            assert_eq!(
                actual, *expected,
                "capture {} checkpoint{checkpoints}",
                c["name"]
            );
            checkpoints += 1;
        }
    }
    for c in f["goals"].as_array().unwrap() {
        let teams = c["teams"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_i64().unwrap() as i32)
            .collect::<Vec<_>>();
        assert_eq!(
            bits(deployment::win_vp(&teams).unwrap()),
            c["goal"],
            "goal {teams:?}"
        );
    }
    for c in f["phases"].as_array().unwrap() {
        let mut m = make(&f);
        assert_eq!(m.start.win_vp, 1200.);
        let actor_count = m.world.game.units.len();
        for u in m.world.game.units.values_mut() {
            if !m.rules.kinds[u.kind].structure {
                u.x = 2.;
                u.z = 2.;
            }
        }
        for step in c["steps"].as_array().unwrap() {
            let a = &step["action"];
            m.world.game.tick = a["tick"].as_i64().unwrap();
            for p in a["players"].as_array().into_iter().flatten() {
                let row = &mut m.start.economy.players[p[0].as_u64().unwrap() as usize];
                match p[1].as_str().unwrap() {
                    "vp" => row.vp = p[2].as_f64().unwrap(),
                    "away" => row.away = p[2].as_bool().unwrap(),
                    _ => panic!("declared player fact"),
                }
            }
            for p in a["points"].as_array().into_iter().flatten() {
                let i = p[0].as_u64().unwrap() as usize;
                let row = &mut m.start.points[i];
                for (key, v) in p[1].as_object().unwrap() {
                    match key.as_str() {
                        "owner" => row.owner = slot(v),
                        "capper" => row.capper = slot(v),
                        "progress" => row.progress = v.as_f64().unwrap(),
                        "cut" => row.cut = v.as_bool().unwrap(),
                        "capTick" => m.capture_ticks[i] = Some(v.as_i64().unwrap()),
                        _ => panic!("declared point fact"),
                    }
                }
            }
            if a["periodic"] == true {
                m.sample_conquest_tick().unwrap();
            }
            if a["pointsPhase"] == true {
                let order = m.world.game.units.keys().copied().collect::<Vec<_>>();
                m.apply_point_capture_income(&order).unwrap();
            }
            m.apply_conquest_victory().unwrap();
            assert_eq!(
                m.world.game.units.len(),
                actor_count,
                "canonical actors retained"
            );
            assert_eq!(
                state(&m),
                step["expected"],
                "phase {} tick{}",
                c["name"],
                a["tick"]
            );
        }
    }
    let mut scoring = make(&f);
    let scorer = scoring
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 0 && scoring.rules.kinds[u.kind].name == "rifle")
        .unwrap()
        .id;
    assert_eq!(scorer, f["scoring"]["actor"]["id"].as_u64().unwrap() as u32);
    for u in scoring.world.game.units.values_mut() {
        u.hold_pos = true;
    }
    for control in scoring.commands.units.values_mut() {
        control.hold_fire = true;
    }
    let at = scoring.start.points[0].at;
    let u = scoring.world.game.units.get_mut(&scorer).unwrap();
    u.x = at.x;
    u.z = at.z;
    for step in f["scoring"]["steps"].as_array().unwrap() {
        scoring.start.economy.players[0].away = step["away"].as_bool().unwrap();
        while scoring.world.game.tick < step["tick"].as_i64().unwrap() {
            scoring.advance_tick().unwrap();
        }
        assert_eq!(
            state(&scoring),
            step["expected"],
            "actual scoring tick{}",
            step["tick"]
        );
        assert_eq!(scoring.start.points[0].owner, Some(0));
        assert!(scoring.finish.is_none());
    }
    let actual = f["scoring"]["steps"].as_array().unwrap();
    assert_eq!(
        actual[0]["expected"]["players"][0]["vp"],
        actual[1]["expected"]["players"][0]["vp"]
    );
    assert_eq!(
        actual[0]["expected"]["players"][0]["mp"],
        actual[1]["expected"]["players"][0]["mp"]
    );
    let mut m = make(&f);
    m.start.economy.players[0].vp = m.start.win_vp;
    m.apply_conquest_victory().unwrap();
    assert_eq!(state(&m), f["terminal"]["before"]);
    let frozen = m.finish;
    let before = state(&m);
    let actor_count = m.world.game.units.len();
    let victim = m
        .world
        .game
        .units
        .values()
        .find(|u| u.owner == 0 && m.rules.kinds[u.kind].name == "rifle")
        .unwrap()
        .id;
    assert_eq!(
        victim,
        f["terminal"]["deadActor"]["id"].as_u64().unwrap() as u32
    );
    m.world.game.units.get_mut(&victim).unwrap().hp = 0.;
    for step in f["terminal"]["steps"].as_array().unwrap() {
        let command = Command::from_json(&step["command"]).unwrap();
        assert_eq!(
            m.execute_command(0, &command, false).unwrap(),
            Some(Denial::Blocked)
        );
        assert_eq!(step["denial"], "blocked");
        m.record_story_tally(Some(0), StoryKey::Built, 1.).unwrap();
        while m.world.game.tick < step["tick"].as_i64().unwrap() {
            m.advance_tick().unwrap();
        }
        assert_eq!(m.world.game.tick, step["tick"].as_i64().unwrap());
        assert_eq!(m.finish, frozen);
        assert_eq!(state(&m)["story"], step["story"]);
        assert_eq!(state(&m)["timeline"], step["timeline"]);
        assert_eq!(state(&m)["story"], before["story"]);
        assert_eq!(state(&m)["timeline"], before["timeline"]);
        assert!(!m.world.game.units.contains_key(&victim));
        assert_eq!(step["retired"], true);
        assert_eq!(m.world.game.units.len(), actor_count - 1);
        assert_eq!(
            m.world.game.units.len() as u64,
            step["actorCount"].as_u64().unwrap()
        );
    }
    eprintln!(
        "MODE_BATCH capture_cases={} capture_checkpoints={checkpoints} goals={} phases={} terminal_checkpoints={} terminal_actual_ticks=220 scoring_actual_ticks=420",
        f["captures"].as_array().unwrap().len(),
        f["goals"].as_array().unwrap().len(),
        f["phases"].as_array().unwrap().len(),
        f["terminal"]["steps"].as_array().unwrap().len()
    );
}
