use serde_json::{Value, json};
use ww2_core::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::Point,
    map::AuthoritativeMap,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    match_story::{self, DeathRecord, PlayerStory, StoryChange, StoryKey},
    weather::WeatherChoice,
};
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
fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/match-story.json")).unwrap()
}
fn encode(v: Value) -> Value {
    match v {
        Value::Number(n) => json!(format!("{:016x}", n.as_f64().unwrap().to_bits())),
        Value::Array(a) => Value::Array(a.into_iter().map(encode).collect()),
        Value::Object(o) => Value::Object(o.into_iter().map(|(k, v)| (k, encode(v))).collect()),
        v => v,
    }
}
fn story_row(v: PlayerStory) -> Value {
    Value::Object(
        StoryKey::ALL
            .into_iter()
            .map(|key| (key.as_str().to_owned(), json!(v.value(key))))
            .collect(),
    )
}
fn key(v: &Value) -> StoryKey {
    StoryKey::ALL
        .into_iter()
        .find(|k| k.as_str() == v.as_str().unwrap())
        .unwrap()
}
fn point(v: &Value) -> Point {
    Point {
        x: v["x"].as_f64().unwrap(),
        z: v["z"].as_f64().unwrap(),
    }
}
fn slot(v: &Value) -> Option<usize> {
    v.as_i64().and_then(|v| (v >= 0).then_some(v as usize))
}
fn snapshot(m: &Match) -> Value {
    let mut fallen = serde_json::Map::new();
    for (name, p) in [
        ("structure", m.fallen.structure),
        ("bunker", m.fallen.bunker),
        ("hq", m.fallen.hq),
    ] {
        if let Some(p) = p {
            fallen.insert(name.into(), json!({"x":p.x,"z":p.z}));
        }
    }
    let report=m.conquest_result().unwrap().map(|r|json!({"winner":r.winner,"teams":r.teams,"names":r.names,"reason":r.reason.as_str(),"at":{"x":r.at.x,"z":r.at.z},"endTick":r.end_tick,"story":r.story.into_iter().map(story_row).collect::<Vec<_>>(),"timeline":r.timeline.into_iter().map(|s|match s { ww2_core::match_end::MatchSample::Conquest {t,vp} => json!({"t":t,"vp":vp}), _ => panic!("Conquest report must retain source shape") }).collect::<Vec<_>>(),"winVp":r.win_vp,"mode":r.mode}));
    encode(
        json!({"story":m.story.iter().copied().map(story_row).collect::<Vec<_>>(),"fallen":fallen,"captureTicks":m.capture_ticks,"report":report}),
    )
}
#[test]
fn canonical_story_and_result_match_original_callbacks() {
    let f = fixture();
    assert_eq!(f["reference"], "e4e579a838aabae3b1d9dadbc17a0a87147bc52d");
    let mut count = 0;
    for c in f["cases"].as_array().unwrap() {
        let mut m = make(c);
        for step in c["steps"].as_array().unwrap() {
            let a = &step["action"];
            m.world.game.tick = a["tick"].as_i64().unwrap_or(1);
            match a["kind"].as_str().unwrap() {
                "tally" => m
                    .record_story_tally(
                        slot(&a["slot"]),
                        key(&a["key"]),
                        a["amount"].as_f64().unwrap(),
                    )
                    .unwrap(),
                "support" => m
                    .record_support_call(
                        a["slot"].as_u64().unwrap() as usize,
                        a["mpCharge"].as_f64(),
                    )
                    .unwrap(),
                "death" => m
                    .record_death(
                        DeathRecord {
                            owner: slot(&a["owner"]),
                            kind: a["unit"].as_str().unwrap(),
                            at: point(&a["at"]),
                        },
                        slot(&a["lastHit"]),
                    )
                    .unwrap(),
                "points" => {
                    for p in a["points"].as_array().into_iter().flatten() {
                        let p0 = &mut m.start.points[p["index"].as_u64().unwrap() as usize];
                        p0.owner = slot(&p["owner"]);
                        p0.capper = slot(&p["capper"]);
                        p0.progress = p["progress"].as_f64().unwrap();
                    }
                    for c in a["moves"].as_array().into_iter().flatten() {
                        let u = m
                            .world
                            .game
                            .units
                            .get_mut(&(c[0].as_u64().unwrap() as u32))
                            .unwrap();
                        u.x = c[1].as_f64().unwrap();
                        u.z = c[2].as_f64().unwrap();
                    }
                    let ids = m.world.game.units.keys().copied().collect::<Vec<_>>();
                    m.apply_conquest_point_economy(&ids).unwrap();
                }
                "out" => {
                    m.start.economy.players[a["slot"].as_u64().unwrap() as usize].out =
                        a["value"].as_bool().unwrap();
                }
                "sample" => {
                    m.sample_conquest_tick().unwrap();
                }
                "finish" => {
                    let team = a["winner"].as_i64().unwrap() as i32;
                    let owner = m
                        .start
                        .economy
                        .players
                        .iter()
                        .position(|p| p.team == team)
                        .unwrap();
                    m.start.economy.players[owner].vp = m.start.win_vp;
                    if let Some(at) = a.get("at") {
                        m.start.points[0].at = point(at);
                        m.start.points[0].owner = Some(owner);
                        m.capture_ticks[0] = Some(m.world.game.tick);
                    }
                    m.apply_conquest_victory().unwrap();
                }
                _ => unreachable!(),
            }
            assert_eq!(snapshot(&m), step["expected"], "{} action{}", c["name"], a);
            count += 1;
        }
    }
    assert_eq!(count, 42);
}
#[test]
fn prepared_story_guard_is_atomic_and_supports_disjoint_wallet_commit() {
    let f = fixture();
    let mut m = make(&f["cases"][0]);
    let before = m.story.clone();
    let changes = [
        StoryChange {
            slot: Some(0),
            key: StoryKey::MpSpent,
            amount: 20.5,
        },
        StoryChange {
            slot: Some(0),
            key: StoryKey::MpSpent,
            amount: -0.5,
        },
    ];
    {
        let guard = match_story::prepare_story_updates(&mut m.story, false, &changes).unwrap();
        drop(guard);
    }
    assert_eq!(m.story, before);
    let old_mp = m.start.economy.players[0].mp;
    let guard = match_story::prepare_story_updates(&mut m.story, false, &changes).unwrap();
    m.start.economy.players[0].mp -= 20.0;
    guard.commit();
    assert_eq!(m.start.economy.players[0].mp, old_mp - 20.0);
    assert_eq!(m.story[0].mp_spent, 20.0);
    let before = m.story.clone();
    let invalid = [
        changes[0],
        StoryChange {
            slot: Some(99),
            key: StoryKey::Kills,
            amount: 1.0,
        },
    ];
    assert!(match_story::prepare_story_updates(&mut m.story, false, &invalid).is_err());
    assert_eq!(m.story, before);
    assert!(
        m.record_death(
            DeathRecord {
                owner: Some(0),
                kind: "missing",
                at: Point { x: 1.0, z: 2.0 }
            },
            Some(1)
        )
        .is_err()
    );
    assert_eq!(m.story, before);
    assert_eq!(m.fallen.structure, None);
    assert!(m.record_support_call(0, Some(f64::NAN)).is_err());
    assert_eq!(m.story, before);
    m.story[1].kills = f64::MAX;
    let before = m.story.clone();
    assert!(
        match_story::prepare_story_updates(
            &mut m.story,
            false,
            &[
                StoryChange {
                    slot: Some(0),
                    key: StoryKey::Losses,
                    amount: 1.0
                },
                StoryChange {
                    slot: Some(1),
                    key: StoryKey::Kills,
                    amount: f64::MAX
                }
            ]
        )
        .is_err()
    );
    assert_eq!(m.story, before);
}
#[test]
fn raw_counters_round_only_in_the_finished_report() {
    let f = fixture();
    let mut m = make(&f["cases"][0]);
    assert_eq!(m.conquest_result().unwrap(), None);
    m.record_spending(0, -0.5).unwrap();
    m.record_plane_downed(Some(0)).unwrap();
    m.record_built(0).unwrap();
    m.world.game.tick = 1;
    m.start.economy.players[0].vp = m.start.win_vp;
    m.apply_conquest_victory().unwrap();
    let r = m.conquest_result().unwrap().unwrap();
    assert_eq!(r.story[0].mp_spent.to_bits(), (-0.0f64).to_bits());
    assert_eq!(m.story[0].mp_spent, -0.5);
    assert_eq!(r.story[0].built, 1.0);
    assert_eq!(r.story[0].planes_downed, 1.0);
    let ww2_core::match_end::MatchSample::Conquest { vp, .. } = &mut m.timeline[0] else {
        panic!("Conquest sample")
    };
    vp[0] = f64::NAN;
    assert!(m.conquest_result().is_err());
}

#[test]
fn ordered_death_preflight_checks_every_record_without_publishing() {
    let f = fixture();
    let mut m = make(&f["cases"][0]);
    let one = DeathRecord {
        owner: Some(0),
        kind: "hq",
        at: Point { x: 3.0, z: 4.0 },
    };
    let two = DeathRecord {
        owner: Some(1),
        kind: "tank",
        at: Point { x: 7.0, z: 8.0 },
    };
    let before = m.story.clone();
    let fallen = m.fallen;
    m.validate_story_for_deaths(&[(one, Some(1)), (two, Some(0))])
        .unwrap();
    assert_eq!(m.story, before);
    assert_eq!(m.fallen, fallen);
    assert!(
        m.validate_story_for_deaths(&[
            (one, Some(1)),
            (
                DeathRecord {
                    kind: "unknown",
                    ..two
                },
                Some(0)
            )
        ])
        .is_err()
    );
    assert_eq!(m.story, before);
    assert_eq!(m.fallen, fallen);
    m.record_death(one, Some(1)).unwrap();
    m.record_death(two, Some(0)).unwrap();
    assert_eq!(m.story[0].kills, 1.0);
    assert_eq!(m.story[0].losses, 1.0);
    assert_eq!(m.story[1].kills, 1.0);
    assert_eq!(m.story[1].losses, 1.0);
    assert_eq!(m.fallen.hq, Some(one.at));
}
