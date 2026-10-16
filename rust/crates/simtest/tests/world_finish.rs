use crate::shared::world_scene;
use serde_json::{Value, json};
use world_scene::{make_match, projection, reset, sorted_units, update_region};
use ww2_core::{
    game::Point,
    match_end::MatchSample,
    match_runtime::Match,
    match_story::{PlayerStory, StoryKey},
};
fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/world-finish.json")).unwrap()
}
fn encode(v: Value) -> Value {
    match v {
        Value::Number(n) => json!(format!("{:016x}", n.as_f64().unwrap().to_bits())),
        Value::Array(a) => Value::Array(a.into_iter().map(encode).collect()),
        Value::Object(o) => Value::Object(o.into_iter().map(|(k, v)| (k, encode(v))).collect()),
        v => v,
    }
}
fn samples(rows: &[MatchSample]) -> Vec<Value> {
    rows.iter()
        .map(|s| match s {
            MatchSample::Conquest { t, vp } => json!({"t":t,"vp":vp}),
            MatchSample::Territories {
                t,
                points,
                structures,
            } => json!({"t":t,"points":points,"structures":structures}),
        })
        .collect()
}
fn story(rows: &[PlayerStory]) -> Vec<Value> {
    rows.iter()
        .map(|s| {
            Value::Object(
                StoryKey::ALL
                    .into_iter()
                    .map(|k| (k.as_str().into(), json!(s.value(k))))
                    .collect(),
            )
        })
        .collect()
}
fn snapshot(m: &Match) -> Value {
    let base = projection(m);
    let finish=m.finish.map(|f|json!({"winner":f.winner,"reason":f.reason.as_str(),"endAt":{"x":f.end_at.x,"z":f.end_at.z},"endTick":f.end_tick}));
    let report=m.match_result().unwrap().map(|r|json!({"reason":r.reason.as_str(),"at":{"x":r.at.x,"z":r.at.z},"endTick":r.end_tick,"story":story(&r.story),"timeline":samples(&r.timeline),"winVp":r.win_vp,"mode":r.mode}));
    encode(
        json!({"regions":base["regions"],"players":base["players"],"units":base["units"],"story":story(&m.story),"finish":finish,"timeline":samples(&m.timeline),"reveal":m.world.game.reveal,"report":report}),
    )
}
#[test]
fn world_finish_uses_actual_constructed_match_and_original_phase_order() {
    let f = fixture();
    assert_eq!(f["reference"], "e4e579a838aabae3b1d9dadbc17a0a87147bc52d");
    assert_eq!(f["node"], "v24.21.0");
    assert_eq!(f["arch"], "x64");
    let mut phases = 0;
    for scene in [&f, &f["massive"]] {
        let mut m = make_match(
            scene,
            scene["cases"][0]["input"]["teams"].as_array().unwrap(),
        );
        for case in scene["cases"].as_array().unwrap() {
            reset(&mut m, scene, &case["input"]);
            for step in case["steps"].as_array().unwrap() {
                m.world.game.tick = step["tick"].as_i64().unwrap();
                for r in step["changeRegions"].as_array().into_iter().flatten() {
                    update_region(
                        &mut m.world_runtime.as_mut().unwrap().regions
                            [r["index"].as_u64().unwrap() as usize],
                        r,
                    );
                }
                let before = snapshot(&m);
                assert_eq!(before, step["before"], "{} before", case["name"]);
                if step["periodic"] == true {
                    m.sample_match_tick().unwrap();
                }
                let order: Vec<_> = step["order"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_u64().unwrap() as u32)
                    .collect();
                let result = m.apply_world_capture_income(&order, step["dt"].as_f64().unwrap());
                // The fixture's old native admission label predates fort capture support.
                let fort_capture =
                    case["name"] == "winning capture needing fort transfer remains refusal";
                if case["supported"] == true || fort_capture {
                    result.unwrap();
                    assert_eq!(snapshot(&m), step["after"], "{} after", case["name"]);
                    if let Some(report) = m.match_result().unwrap() {
                        assert_eq!(report.mode, "world");
                        assert_eq!(report.win_vp, None);
                        assert_eq!(report.names.len(), 3);
                        assert!(m.world.game.reveal);
                    }
                } else {
                    assert!(result.unwrap_err().starts_with("unsupported World"));
                    assert_eq!(snapshot(&m), before);
                    assert_ne!(step["before"], step["after"]);
                }
                assert!(m.apply_conquest_victory().is_err());
                assert!(m.sample_conquest_tick().is_err());
                assert!(m.conquest_result().is_err());
                phases += 1;
            }
        }
        // Keep shared base projection ordering independent of source Map insertion order.
        assert_eq!(sorted_units(projection(&m)), projection(&m));
    }
    assert_eq!(
        f["cases"].as_array().unwrap().len() + f["massive"]["cases"].as_array().unwrap().len(),
        15
    );
    assert_eq!(phases, 16);
}
fn protected(m: &Match) -> String {
    format!(
        "{}{:?}{:?}{:?}{:?}{:?}{}",
        projection(m),
        m.finish,
        m.timeline,
        m.story,
        m.start.economy,
        m.fallen,
        m.world.game.reveal
    )
}
#[test]
fn world_finish_preparation_errors_do_not_publish_region_income_or_story() {
    let f = fixture();
    let case = &f["cases"][0];
    let mut m = make_match(&f, case["input"]["teams"].as_array().unwrap());
    let mutations: [fn(&mut Match); 7] = [
        |m| m.options.players[2].team = 4,
        |m| m.world.game.players[1].slot = 2,
        |m| m.story[1].captures = f64::NAN,
        |m| m.world_runtime.as_mut().unwrap().regions[0].cap_tick = Some(i64::MAX),
        |m| {
            m.fallen.bunker = Some(Point {
                x: f64::INFINITY,
                z: 0.0,
            })
        },
        |m| {
            m.timeline.push(MatchSample::Conquest {
                t: 0.0,
                vp: vec![0.0; 6],
            })
        },
        |m| m.world_runtime.as_mut().unwrap().regions[0].capper = Some(9),
    ];
    for mutate in mutations {
        reset(&mut m, &f, &case["input"]);
        m.options.players[2].team = 5;
        m.world.game.players[1].slot = 1;
        mutate(&mut m);
        let before = protected(&m);
        assert!(m.apply_world_capture_income(&[], 0.05).is_err());
        assert_eq!(protected(&m), before);
    }
}
#[test]
fn canonical_world_result_stays_decided_and_source_story_stays_frozen() {
    let f = fixture();
    let case = &f["cases"][0];
    let mut m = make_match(&f, case["input"]["teams"].as_array().unwrap());
    reset(&mut m, &f, &case["input"]);
    m.world.game.tick = 200;
    m.apply_world_capture_income(&[], 0.0).unwrap();
    let result = m.match_result().unwrap().unwrap();
    let finish = m.finish;
    let timeline = m.timeline.clone();
    let story = m.story.clone();
    m.apply_world_capture_income(&[], 0.05).unwrap();
    assert!(!m.sample_match_tick().unwrap());
    m.record_spending(0, 100.0).unwrap();
    assert_eq!(m.finish, finish);
    assert_eq!(m.timeline, timeline);
    assert_eq!(m.story, story);
    assert!(m.world.game.reveal);
    assert_eq!(m.match_result().unwrap().unwrap(), result);
    // The explicit World phase continues; a complete closing tick is still caller-owned.
}

fn number_bits(v: &Value) -> f64 {
    f64::from_bits(u64::from_str_radix(v.as_str().unwrap(), 16).unwrap())
}
#[test]
fn prepared_world_removal_sample_matches_exact_original_survivors_without_publication() {
    use ww2_core::match_world_end::WorldFinishUnitFacts;
    let f = fixture();
    let mut compared = 0;
    let mut removed_count = 0;
    for scene in [&f, &f["massive"]] {
        let mut m = make_match(
            scene,
            scene["cases"][0]["input"]["teams"].as_array().unwrap(),
        );
        for case in scene["cases"].as_array().unwrap() {
            reset(&mut m, scene, &case["input"]);
            for step in case["steps"].as_array().unwrap() {
                m.world.game.tick = step["tick"].as_i64().unwrap();
                let mut wallets = m.start.economy.clone();
                for (p, raw) in wallets
                    .players
                    .iter_mut()
                    .zip(step["after"]["players"].as_array().unwrap())
                {
                    p.out = raw[6].as_bool().unwrap();
                }
                let mut regions = m.world_runtime.as_ref().unwrap().regions.clone();
                for (r, raw) in regions
                    .iter_mut()
                    .zip(step["after"]["regions"].as_array().unwrap())
                {
                    r.team = number_bits(&raw[0]) as i32;
                    r.progress = number_bits(&raw[1]);
                    r.capper = if raw[2].is_null() {
                        None
                    } else {
                        Some(number_bits(&raw[2]) as usize)
                    };
                    r.cap_tick = if raw[5].is_null() {
                        None
                    } else {
                        Some(number_bits(&raw[5]) as i64)
                    };
                }
                let survivors = step["postWreckUnits"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|u| WorldFinishUnitFacts {
                        id: u["id"].as_u64().unwrap() as u32,
                        owner: if u["owner"].as_i64().unwrap() < 0 {
                            ww2_core::game::LOCAL_DEFENDER_OWNER
                        } else {
                            u["owner"].as_u64().unwrap() as usize
                        },
                        kind: m.rules.kind(u["type"].as_str().unwrap()).unwrap(),
                        hp: u["hp"].as_f64().unwrap(),
                    })
                    .collect::<Vec<_>>();
                let removed = step["removed"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_u64().unwrap() as u32)
                    .collect::<Vec<_>>();
                let before = protected(&m);
                let prepared =
                    m.prepare_world_finish_after_removal(&wallets, &regions, &survivors, &removed);
                if case["name"].as_str().unwrap().contains("fort transfer") {
                    assert!(prepared.is_err()); // Owner transfer remains outside this removal-only guard.
                } else {
                    let actual=prepared.unwrap().map(|(end,sample)|{
                        assert_eq!(encode(samples(&[sample])[0].clone()),*step["after"]["timeline"].as_array().unwrap().last().unwrap());
                        encode(json!({"winner":end.winner,"reason":end.reason.as_str(),"endAt":{"x":end.end_at.x,"z":end.end_at.z},"endTick":end.end_tick}))
                    }).unwrap_or(Value::Null);
                    assert_eq!(
                        actual, step["after"]["finish"],
                        "{} prepared finish",
                        case["name"]
                    );
                    compared += 1;
                    removed_count += removed.len();
                    if !survivors.is_empty() {
                        let mut bad = survivors.clone();
                        bad.push(survivors[0]);
                        assert!(
                            m.prepare_world_finish_after_removal(
                                &wallets, &regions, &bad, &removed
                            )
                            .is_err()
                        );
                        bad = survivors.clone();
                        bad[0].hp = f64::NAN;
                        assert!(
                            m.prepare_world_finish_after_removal(
                                &wallets, &regions, &bad, &removed
                            )
                            .is_err()
                        );
                        bad = survivors.clone();
                        bad[0].kind = m.rules.kinds.len();
                        assert!(
                            m.prepare_world_finish_after_removal(
                                &wallets, &regions, &bad, &removed
                            )
                            .is_err()
                        );
                    }
                    if !removed.is_empty() {
                        assert!(
                            m.prepare_world_finish_after_removal(
                                &wallets,
                                &regions,
                                &survivors,
                                &[]
                            )
                            .is_err()
                        );
                        let mut bad = removed.clone();
                        bad.push(removed[0]);
                        assert!(
                            m.prepare_world_finish_after_removal(
                                &wallets, &regions, &survivors, &bad
                            )
                            .is_err()
                        );
                        let mut active = wallets.clone();
                        active.players[m.world.game.units[&removed[0]].owner].out = false;
                        assert!(
                            m.prepare_world_finish_after_removal(
                                &active, &regions, &survivors, &removed
                            )
                            .is_err()
                        );
                    }
                }
                assert_eq!(
                    protected(&m),
                    before,
                    "preparation must not publish any staged phase"
                );
                if let Some(id) = m.world.game.units.keys().next().copied() {
                    for injected in [id + 10_000, 0] {
                        m.world.game.units.get_mut(&id).unwrap().id = injected;
                        let malformed = protected(&m);
                        let mut claimed_survivors = survivors.clone();
                        for u in &mut claimed_survivors {
                            if u.id == id {
                                u.id = injected;
                            }
                        }
                        let claimed_removed = removed
                            .iter()
                            .map(|removed| if *removed == id { injected } else { *removed })
                            .collect::<Vec<_>>();
                        // Match the injected value ID in the claims. The earlier coverage-only
                        // guard accepted this witness even though actual mutation uses the map key.
                        assert!(
                            m.prepare_world_finish_after_removal(
                                &wallets,
                                &regions,
                                &claimed_survivors,
                                &claimed_removed
                            )
                            .is_err()
                        );
                        assert_eq!(
                            protected(&m),
                            malformed,
                            "invalid canonical key/ID must remain unchanged"
                        );
                    }
                    m.world.game.units.get_mut(&id).unwrap().id = id;
                }
            }
        }
    }
    assert_eq!(compared, 15);
    assert_eq!(removed_count, 2);
}
