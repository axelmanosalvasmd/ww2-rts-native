use crate::shared::world_scene;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use ww2_core::{
    game::{LOCAL_DEFENDER_OWNER, Point},
    match_end::{EndReason, MatchFinish, MatchSample},
    match_runtime::Match,
    match_story::{PlayerStory, StoryKey},
    match_world_phases::WorldIncomeUnit,
};

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/world-epilogue.json")).unwrap()
}
fn ids(v: &Value) -> Vec<u32> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_u64().unwrap() as u32)
        .collect()
}
fn encode(v: Value) -> Value {
    match v {
        Value::Number(n) => json!(format!("{:016x}", n.as_f64().unwrap().to_bits())),
        Value::Array(a) => Value::Array(a.into_iter().map(encode).collect()),
        Value::Object(o) => Value::Object(o.into_iter().map(|(k, v)| (k, encode(v))).collect()),
        v => v,
    }
}
fn story(m: &Match) -> Value {
    json!(
        m.story
            .iter()
            .map(|s| Value::Object(
                StoryKey::ALL
                    .into_iter()
                    .map(|k| (k.as_str().into(), json!(s.value(k))))
                    .collect()
            ))
            .collect::<Vec<_>>()
    )
}
fn terminal(m: &Match) -> Value {
    let end = m.finish.unwrap();
    let timeline = m
        .timeline
        .iter()
        .map(|s| match s {
            MatchSample::Territories {
                t,
                points,
                structures,
            } => json!({"t":t,"points":points,"structures":structures}),
            MatchSample::Conquest { .. } => panic!("unexpected World sample"),
        })
        .collect::<Vec<_>>();
    encode(
        json!({"winner":end.winner,"reason":end.reason.as_str(),"endAt":{"x":end.end_at.x,"z":end.end_at.z},"endTick":end.end_tick,"timeline":timeline,"story":story(m)}),
    )
}
fn view(m: &Match, order: &[u32]) -> Value {
    let mut v = world_scene::projection(m);
    v["units"] = json!(
        order
            .iter()
            .filter_map(|id| m.world.game.units.get(id))
            .map(|u| [
                i64::from(u.id),
                if u.owner == LOCAL_DEFENDER_OWNER {
                    -1
                } else {
                    u.owner as i64
                }
            ])
            .collect::<Vec<_>>()
    );
    v["terminal"] = terminal(m);
    v["reveal"] = json!(m.world.game.reveal);
    v
}
fn load(f: &Value, case: &Value) -> Match {
    let mut m = world_scene::make_match(f, case["input"]["teams"].as_array().unwrap());
    world_scene::reset(&mut m, f, &case["input"]);
    // Restore canonical terminal facts recorded from original finish(). This is
    // explicit scene input, not another finish actuator or ending-hold runner.
    let t = &case["terminal"];
    m.finish = Some(MatchFinish {
        winner: t["winner"].as_i64().unwrap() as i32,
        reason: match t["reason"].as_str().unwrap() {
            "world" => EndReason::World,
            "nations" => EndReason::Nations,
            "draw" => EndReason::Draw,
            _ => panic!("unknown original World reason"),
        },
        end_at: Point {
            x: t["endAt"]["x"].as_f64().unwrap(),
            z: t["endAt"]["z"].as_f64().unwrap(),
        },
        end_tick: t["endTick"].as_i64().unwrap(),
    });
    m.world.game.reveal = true;
    m.timeline = t["timeline"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| MatchSample::Territories {
            t: s["t"].as_f64().unwrap(),
            points: s["points"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_f64().unwrap())
                .collect(),
            structures: s["structures"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_f64().unwrap())
                .collect(),
        })
        .collect();
    m.story = t["story"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| PlayerStory {
            kills: s["kills"].as_f64().unwrap(),
            losses: s["losses"].as_f64().unwrap(),
            built: s["built"].as_f64().unwrap(),
            captures: s["captures"].as_f64().unwrap(),
            mp_spent: s["mpSpent"].as_f64().unwrap(),
            support_calls: s["supportCalls"].as_f64().unwrap(),
            planes_downed: s["planesDowned"].as_f64().unwrap(),
        })
        .collect();
    m
}
fn stage(m: &mut Match, step: &Value) -> Vec<WorldIncomeUnit> {
    let spec = &step["spec"];
    m.world.game.tick = spec["tick"].as_i64().unwrap();
    for r in spec["regions"].as_array().into_iter().flatten() {
        world_scene::update_region(
            &mut m.world_runtime.as_mut().unwrap().regions[r["index"].as_u64().unwrap() as usize],
            r,
        );
    }
    for u in spec["units"].as_array().into_iter().flatten() {
        world_scene::update_unit(m, u, false);
    }
    let mut retired = BTreeMap::new();
    for a in step["actions"].as_array().unwrap() {
        match a["op"].as_str().unwrap() {
            "health" => {
                m.world
                    .game
                    .units
                    .get_mut(&(a["id"].as_u64().unwrap() as u32))
                    .unwrap()
                    .hp = a["hp"].as_f64().unwrap()
            }
            "remove" => {
                let id = a["id"].as_u64().unwrap() as u32;
                let u = m.world.game.units.remove(&id).unwrap();
                retired.insert(id, WorldIncomeUnit::from(&u));
                m.grid.remove_unit(id);
            }
            "birth" => world_scene::update_unit(m, &a["born"], true),
            _ => panic!("unknown original scene action"),
        }
    }
    ids(&step["capturedOrder"])
        .iter()
        .map(|id| {
            m.world
                .game
                .units
                .get(id)
                .map(WorldIncomeUnit::from)
                .unwrap_or_else(|| *retired.get(id).expect("explicit final removed object"))
        })
        .collect()
}
#[test]
fn original_decided_world_income_and_current_regions_continue_without_report_changes() {
    let f = fixture();
    let mut count = 0;
    for case in f["cases"].as_array().unwrap() {
        let mut m = load(&f, case);
        let report = m.match_result().unwrap().unwrap();
        let history = terminal(&m);
        for step in case["steps"].as_array().unwrap() {
            let facts = stage(&mut m, step);
            let order = ids(&step["order"]);
            let actual=facts.iter().map(|u|json!({"id":u.id,"owner":if u.owner==LOCAL_DEFENDER_OWNER{-1}else{u.owner as i64},"type":m.rules.kinds[u.kind].name,"hp":format!("{:016x}",u.hp.to_bits())})).collect::<Vec<_>>();
            assert_eq!(json!(actual), step["facts"], "{} final facts", case["name"]);
            assert_eq!(view(&m, &order), step["before"], "{} before", case["name"]);
            assert_eq!(
                m.sample_match_tick().unwrap(),
                step["periodic"].as_bool().unwrap()
            );
            m.apply_world_income(&facts, 0.05).unwrap();
            assert_eq!(
                view(&m, &order),
                step["afterIncome"],
                "{} income",
                case["name"]
            );
            let effects = m.apply_world_regions(&order, 0.05).unwrap();
            assert_eq!(
                json!(effects.removed.iter().map(|u| u.id).collect::<Vec<_>>()),
                step["removed"]
            );
            assert_eq!(
                view(&m, &order),
                step["afterRegions"],
                "{} regions",
                case["name"]
            );
            assert_eq!(terminal(&m), history);
            assert_eq!(m.match_result().unwrap().unwrap(), report);
            count += 1;
        }
    }
    assert_eq!(count, 12);
}
fn protected(m: &Match) -> String {
    format!(
        "{}{:?}{:?}{:?}{:?}{:?}{}",
        world_scene::projection(m),
        m.finish,
        m.timeline,
        m.story,
        m.start.economy,
        m.fallen,
        m.world.game.reveal
    )
}
#[test]
fn malformed_finished_world_and_zero_actor_identity_reject_before_publication() {
    let f = fixture();
    let case = &f["cases"][0];
    let mut m = load(&f, case);
    m.world.game.tick = 41;
    let end = m.finish;
    let history = m.timeline.clone();
    let story = m.story.clone();
    for change in 0..7 {
        match change {
            0 => m.finish.as_mut().unwrap().end_tick = 42,
            1 => m.world.game.reveal = false,
            2 => m.finish.as_mut().unwrap().end_at.x = f64::NAN,
            3 => m.finish.as_mut().unwrap().winner = 6,
            4 => m.story[0].captures = f64::NAN,
            5 => m.timeline.push(MatchSample::Conquest {
                t: 0.,
                vp: vec![0.; 2],
            }),
            _ => {
                let mut u = m.world.game.units.remove(&90).unwrap();
                u.id = 0;
                m.world.game.units.insert(0, u);
            }
        }
        let order = m.world.game.units.keys().copied().collect::<Vec<_>>();
        let before = protected(&m);
        assert!(m.apply_world_income(&[], 0.05).is_err());
        assert_eq!(protected(&m), before);
        assert!(m.apply_world_regions(&order, 0.05).is_err());
        assert_eq!(protected(&m), before);
        if change == 6 {
            let mut u = m.world.game.units.remove(&0).unwrap();
            u.id = 90;
            m.world.game.units.insert(90, u);
        }
        m.finish = end;
        m.world.game.reveal = true;
        m.timeline = history.clone();
        m.story = story.clone();
    }
    let mut bad_order = m.world.game.units.keys().copied().collect::<Vec<_>>();
    bad_order.pop();
    let before = protected(&m);
    assert!(m.apply_world_regions(&bad_order, 0.05).is_err());
    assert_eq!(protected(&m), before);
    m.options.logistics = true;
    assert!(m.apply_world_income(&[], 0.05).is_err());
    assert!(
        m.apply_world_regions(&ids(&case["steps"][0]["order"]), 0.05)
            .is_err()
    );
    assert_eq!(protected(&m), before);
}
#[test]
fn late_fort_refusal_preserves_decided_phase_and_the_original_terminal_owners() {
    let f = fixture();
    let case = &f["cases"][0];
    let mut m = load(&f, case);
    m.world.game.tick = 41;
    let facts = m
        .world
        .game
        .units
        .values()
        .map(WorldIncomeUnit::from)
        .collect::<Vec<_>>();
    m.apply_world_income(&facts, 0.05).unwrap();
    world_scene::update_unit(
        &mut m,
        &json!({"id":999,"owner":1,"type":"pillbox","x":193.,"z":65.,"hp":500.,"built":1.,"retreating":false,"riding":0}),
        true,
    );
    let order = m.world.game.units.keys().copied().collect::<Vec<_>>();
    let before = protected(&m);
    assert!(
        m.apply_world_regions(&order, 0.05)
            .unwrap_err()
            .contains("fort")
    );
    assert_eq!(protected(&m), before);
}
