//! Canonical four-seat core evidence. Live trusted rooms retain their two-seat guard.
//! Callback-only records and declared score facts are separate from actual tick traces.
use serde_json::{Value, json};
use ww2_core::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::Point,
    map::AuthoritativeMap,
    match_end::{EndReason, MatchSample},
    match_runtime::{Match, MatchOptions, PlayerConfig},
    match_story::{DeathRecord, PlayerStory, StoryKey},
    recipient_snapshot::SnapshotCache,
    weather::{WeatherChoice, WeatherKind},
};

fn encode(v: Value) -> Value {
    match v {
        Value::Number(n) => json!(format!("{:016x}", n.as_f64().unwrap().to_bits())),
        Value::Array(a) => Value::Array(a.into_iter().map(encode).collect()),
        Value::Object(o) => Value::Object(o.into_iter().map(|(k, v)| (k, encode(v))).collect()),
        v => v,
    }
}
fn teams(value: &Value) -> Vec<i32> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_i64().unwrap() as i32)
        .collect()
}
fn make(f: &Value, roster: &[i32]) -> Match {
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&f["map"]).unwrap()).unwrap();
    let mut m = Match::new(
        &map,
        MatchOptions {
            seed: f["seed"].as_u64().unwrap(),
            mode: Mode::Conquest,
            players: roster
                .iter()
                .enumerate()
                .map(|(i, &team)| PlayerConfig {
                    name: ["Host", "Guest", "Ally", "Opponent"][i].into(),
                    team,
                    faction: Faction::try_from(i as u8).unwrap(),
                })
                .collect(),
            shuffle: true,
            army: ArmySize::Standard,
            weather: WeatherChoice::Fixed(WeatherKind::Clear),
            weather_seed: None,
            living_weather: true,
            map_key: Some("pegasus-bridge".into()),
            tech: true,
            supply: true,
            logistics: false,
            sandbox: false,
        },
    )
    .unwrap();
    // Identical declared stance controls preserve constructor actor anchors.
    for u in m.world.game.units.values_mut() {
        u.hold_pos = true;
    }
    for control in m.commands.units.values_mut() {
        control.hold_fire = true;
    }
    m
}
fn story(row: PlayerStory) -> Value {
    json!(
        StoryKey::ALL
            .into_iter()
            .map(|key| (key.as_str().to_owned(), json!(row.value(key))))
            .collect::<serde_json::Map<_, _>>()
    )
}
fn sample(s: &MatchSample) -> Value {
    match s {
        MatchSample::Conquest { t, vp } => json!({"t": t, "vp": vp}),
        _ => panic!("Conquest sample"),
    }
}
fn state(m: &Match) -> Value {
    let result = m.conquest_result().unwrap().map(|r| json!({
        "winner": r.winner, "teams": r.teams, "names": r.names, "reason": r.reason.as_str(),
        "at": {"x": r.at.x, "z": r.at.z}, "endTick": r.end_tick,
        "story": r.story.into_iter().map(story).collect::<Vec<_>>(),
        "timeline": r.timeline.iter().map(sample).collect::<Vec<_>>(), "winVp": r.win_vp, "mode": r.mode,
    }));
    let all_revealed = m.finish.map(|_| {
        let cache = SnapshotCache::new(m).unwrap();
        (0..m.start.players.len()).all(|slot| {
            cache.recipient(slot).unwrap().fields["units"]
                .as_array()
                .unwrap()
                .len()
                == m.world.game.units.len()
        })
    });
    encode(json!({
        "tick": m.world.game.tick, "goal": m.start.win_vp,
        "teams": m.start.economy.players.iter().map(|p|p.team).collect::<Vec<_>>(),
        "vp": m.start.economy.players.iter().map(|p|p.vp).collect::<Vec<_>>(),
        "winner": m.finish.map(|v|v.winner), "reason": m.finish.map(|v|v.reason.as_str()),
        "at": m.finish.map(|v|json!({"x":v.end_at.x,"z":v.end_at.z})),
        "endTick": m.finish.map(|v|v.end_tick), "reveal": m.world.game.reveal,
        "story": m.story.iter().copied().map(story).collect::<Vec<_>>(),
        "timeline": m.timeline.iter().map(sample).collect::<Vec<_>>(), "result": result,
        "allRecipientsReveal": all_revealed,
    }))
}
fn anchors(m: &Match) -> Vec<(u32, usize, usize, u64, u64)> {
    m.world
        .game
        .units
        .values()
        .map(|u| (u.id, u.owner, u.kind, u.x.to_bits(), u.z.to_bits()))
        .collect()
}
fn finish(m: &mut Match, input: &Value) {
    let reason = match input["reason"].as_str().unwrap() {
        "timer" => EndReason::Timer,
        "hq" => EndReason::Headquarters,
        "vp" => EndReason::VictoryPoints,
        _ => panic!("Declared finish reason"),
    };
    let at = input["at"].as_object().map(|p| Point {
        x: p["x"].as_f64().unwrap(),
        z: p["z"].as_f64().unwrap(),
    });
    m.initialize_tick_scene_finish(input["winner"].as_i64().unwrap() as i32, reason, at)
        .unwrap();
}

#[test]
fn canonical_conquest_teams_finish_and_report_match_pinned_source() {
    let f: Value =
        serde_json::from_str(include_str!("fixtures/conquest-team-result.json")).unwrap();
    assert_eq!(f["reference"], "e4e579a838aabae3b1d9dadbc17a0a87147bc52d");
    assert_eq!(f["node"], "v24.21.0");
    assert_eq!(f["arch"], "x64");
    assert_eq!(f["kind"], "isolated-components-not-whole-match");
    let committed: Value = serde_json::from_str(include_str!(
        "../../server/src/authoritative-maps/pegasus-bridge.json"
    ))
    .unwrap();
    assert_eq!(f["map"], committed);
    assert_eq!(f["map"]["spawns"].as_array().unwrap().len(), 4);
    assert!(f["map"].get("scenario").is_none());
    for c in f["constructors"].as_array().unwrap() {
        let m = make(&f, &teams(&c["teams"]));
        assert_eq!(m.start.win_vp, 2400.);
        let mut inventory = m
            .world
            .game
            .units
            .values()
            .map(|u| format!("{}:{}", u.owner, m.rules.kinds[u.kind].name))
            .collect::<Vec<_>>();
        inventory.sort();
        let actual = encode(
            json!({"goal":m.start.win_vp,"teams":m.start.economy.players.iter().map(|p|p.team).collect::<Vec<_>>(),"names":m.start.players.iter().map(|p|p.config.name.clone()).collect::<Vec<_>>(),"inventory":inventory}),
        );
        assert_eq!(actual, c["expected"], "canonical constructor");
    }
    let mut victory_checkpoints = 0;
    for c in f["victories"].as_array().unwrap() {
        let mut m = make(&f, &teams(&c["teams"]));
        let before = anchors(&m);
        for step in c["steps"].as_array().unwrap() {
            let a = &step["action"];
            let tick = a["tick"].as_i64().unwrap_or(m.world.game.tick + 1);
            while m.world.game.tick < tick - 1 {
                m.advance_tick().unwrap();
            }
            for p in a["vp"].as_array().unwrap() {
                m.start.economy.players[p[0].as_u64().unwrap() as usize].vp =
                    p[1].as_f64().unwrap();
            }
            for p in a["points"].as_array().into_iter().flatten() {
                let i = p[0].as_u64().unwrap() as usize;
                let owner = p[1].as_u64().unwrap() as usize;
                m.start.points[i].owner = Some(owner);
                m.start.points[i].capper = Some(owner);
                m.start.points[i].progress = 1.;
                m.capture_ticks[i] = Some(p[2].as_i64().unwrap());
            }
            m.advance_tick().unwrap();
            assert_eq!(anchors(&m), before, "{} canonical anchors", c["name"]);
            assert_eq!(
                state(&m),
                step["expected"],
                "{} checkpoint{victory_checkpoints}",
                c["name"]
            );
            victory_checkpoints += 1;
        }
    }
    for c in f["freshFinishes"].as_array().unwrap() {
        let t = c["input"]["teams"]
            .as_array()
            .map(|_| teams(&c["input"]["teams"]))
            .unwrap_or_else(|| vec![0, 1, 0, 1]);
        let mut m = make(&f, &t);
        let before = anchors(&m);
        finish(&mut m, &c["input"]);
        assert_eq!(
            state(&m),
            c["expected"],
            "fresh finish {}",
            c["input"]["name"]
        );
        // A second setup initialization is guarded; this is not Source finish()'s return behavior.
        assert!(
            m.initialize_tick_scene_finish(-1, EndReason::Draw, None)
                .is_err()
        );
        assert_eq!(m.apply_conquest_victory().unwrap(), None);
        assert_eq!(state(&m), c["expected"]);
        assert_eq!(anchors(&m), before);
    }
    let mut m = make(&f, &[2, 5, 2, 5]);
    let before = anchors(&m);
    for (p, vp) in m
        .start
        .economy
        .players
        .iter_mut()
        .zip([1.75, 2.5, 3.5, 4.75])
    {
        p.vp = vp;
    }
    for step in f["timeline"].as_array().unwrap() {
        let tick = step["tick"].as_i64().unwrap();
        if tick == 202 {
            m.start.economy.players[0].vp = 1000.25;
            m.start.economy.players[2].vp = 1400.25;
        }
        while m.world.game.tick < tick {
            m.advance_tick().unwrap();
        }
        assert_eq!(state(&m), step["expected"], "timeline tick{tick}");
        assert_eq!(anchors(&m), before);
    }
    let mut m = make(&f, &[0, 1, 0, 1]);
    let before = anchors(&m);
    let mut fallen = serde_json::Map::new();
    for step in f["callbacks"].as_array().unwrap() {
        let a = &step["action"];
        if a["kind"] == "tally" {
            let key = StoryKey::ALL
                .into_iter()
                .find(|k| k.as_str() == a["key"].as_str().unwrap())
                .unwrap();
            let slot = a["slot"].as_i64().unwrap();
            m.record_story_tally(
                (slot >= 0).then_some(slot as usize),
                key,
                a["amount"].as_f64().unwrap(),
            )
            .unwrap();
        } else {
            let owner = a["owner"].as_u64().unwrap() as usize;
            let kind = a["unit"].as_str().unwrap();
            let at = m
                .world
                .game
                .units
                .values()
                .find(|u| u.owner == owner && m.rules.kinds[u.kind].name == kind)
                .unwrap()
                .at();
            m.record_death(
                DeathRecord {
                    owner: Some(owner),
                    kind,
                    at,
                },
                Some(a["by"].as_u64().unwrap() as usize),
            )
            .unwrap();
            fallen.insert("structure".into(), json!(m.fallen.structure == Some(at)));
            fallen.insert("hq".into(), json!(m.fallen.hq == Some(at)));
        }
        let actual = encode(
            json!({"story":m.story.iter().copied().map(story).collect::<Vec<_>>(),"fallenAtCanonicalActors":fallen,"actorCount":m.world.game.units.len()}),
        );
        assert_eq!(actual, step["expected"], "canonical callback {}", a);
        assert_eq!(anchors(&m), before);
    }
    finish(&mut m, &json!({"winner":0,"reason":"vp","at":null}));
    assert_eq!(state(&m), f["callbackResult"]);
    for key in StoryKey::ALL {
        m.record_story_tally(Some(1), key, 100.).unwrap();
    }
    assert_eq!(state(&m), f["callbackResult"]);
    let mut m = make(&f, &[0, 1, 0, 1]);
    let initial_count = m.world.game.units.len();
    for (i, step) in f["deaths"].as_array().unwrap().iter().enumerate() {
        let a = &step["action"];
        let owner = a["owner"].as_u64().unwrap() as usize;
        let victim = m
            .world
            .game
            .units
            .values()
            .find(|u| u.owner == owner && m.rules.kinds[u.kind].name == "rifle")
            .unwrap()
            .id;
        m.world.game.units.get_mut(&victim).unwrap().hp = 0.;
        m.combat.get_mut(victim).unwrap().last_hit = a["by"].as_i64().map(|v| v as i32);
        m.advance_tick().unwrap();
        assert!(!m.world.game.units.contains_key(&victim));
        assert_eq!(m.world.game.units.len(), initial_count - i - 1);
        assert_eq!(
            m.world.game.units.len() as u64,
            step["actorCount"].as_u64().unwrap()
        );
        assert_eq!(state(&m), step["expected"], "actual death{i}");
    }
    m.start.economy.players[0].vp = 1100.;
    m.start.economy.players[2].vp = 1300.;
    m.advance_tick().unwrap();
    assert_eq!(state(&m), f["deathResult"]);
    assert_eq!(f["constructors"].as_array().unwrap().len(), 2);
    assert_eq!(victory_checkpoints, 5);
    assert_eq!(f["freshFinishes"].as_array().unwrap().len(), 4);
    assert_eq!(f["timeline"].as_array().unwrap().len(), 5);
    assert_eq!(f["callbacks"].as_array().unwrap().len(), 10);
    assert_eq!(f["deaths"].as_array().unwrap().len(), 3);
    eprintln!(
        "CONQUEST_TEAM_RESULT constructors=2 victory_traces=4 victory_checkpoints=5 fresh_finishes=4 timeline_checkpoints=5 callback_actions=10 actual_deaths=3 actual_tick_steps=361"
    );
}
