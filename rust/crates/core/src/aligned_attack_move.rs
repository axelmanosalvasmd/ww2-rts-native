//! One bounded genuine-constructor attack-move case and its plain-Move control.
//! This module and all observation calls exist only in cfg(test) builds.
use super::{Match, Replay, original};
use crate::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    game::Point,
    match_commands::Command,
    match_runtime::{MatchOptions, PlayerConfig},
    orders::OrderKey,
    weather::WeatherChoice,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

fn number(value: f64) -> Value {
    json!({"bits":format!("{:016x}",value.to_bits()),"value":if value.is_finite(){json!(value)}else{json!(if value.is_nan(){"NaN"}else if value.is_sign_positive(){"Infinity"}else{"-Infinity"})}})
}
fn point(value: Point) -> Value {
    json!({"x":number(value.x),"z":number(value.z)})
}
fn goal(value: Option<Point>) -> Value {
    value.map_or(Value::Null, point)
}
fn optional_number(value: Option<f64>) -> Value {
    value.map_or_else(|| json!({"absent":true}), number)
}
impl Replay {
    pub(crate) fn observe_attack_move(&self, m: &Match, id: u32, event: &str) {
        let u = &m.world.game.units[&id];
        let target = m.firing.units.get(&id).map_or(0, |f| f.target_id);
        self.0.lock().unwrap().scenario_branches.push(json!({"tick":m.world.game.tick,"id":id,"event":event,"targetId":target,
            "x":number(u.x),"z":number(u.z),"hp":number(u.hp),"goal":goal(u.amove),"path":u.path.iter().copied().map(point).collect::<Vec<_>>()}));
    }
    pub(crate) fn observe_moving(&self, id: u32, moving: bool) {
        self.0
            .lock()
            .unwrap()
            .scenario_motion
            .push(json!({"id":id,"moving":moving}));
    }
    fn take_observations(&self) -> (Vec<Value>, Vec<Value>) {
        let mut tape = self.0.lock().unwrap();
        (
            std::mem::take(&mut tape.scenario_branches),
            std::mem::take(&mut tape.scenario_motion),
        )
    }
}
fn snapshot(
    m: &Match,
    replay: &Replay,
    births: Vec<u32>,
    deaths: Vec<u32>,
    retired: &[crate::match_combat::RetiredUnit],
    actor: u32,
    foe: u32,
) -> Result<Value, String> {
    let units=m.world.game.units.values().map(|u| {
        let c=m.commands.units.get(&u.id).ok_or("missing unit control provider")?;
        let f=m.firing.units.get(&u.id).ok_or("missing unit firing provider")?;
        if !c.orders.is_empty() {
            return Err(format!("nonempty ordinary queued-order JSON provider needed for unit {}: {:?}",u.id,c.orders));
        }
        Ok(json!({"id":u.id,"owner":u.owner,"kind":m.rules.kinds[u.kind].name,"x":number(u.x),"z":number(u.z),"hp":number(u.hp),"supp":number(u.supp),
            "path":u.path.iter().copied().map(point).collect::<Vec<_>>(),"rot":number(u.rot),"aim":number(c.aim),"moveSpeed":number(u.move_speed),
            "vx":optional_number(u.vx),"vz":optional_number(u.vz),"travelDir":optional_number(u.travel_dir),"reverse":goal(u.reverse),
            "repath":number(u.repath),"stuck":number(u.stuck),"still":optional_number(u.still),"orders":[],
            "targetId":f.target_id,"attackId":c.attack_id,"amove":goal(u.amove),
            "face":c.face.map_or(Value::Null,|v|json!({"a":number(v.angle),"x":number(v.at.x),"z":number(v.at.z)})),
            "holdPos":u.hold_pos,"holdFire":c.hold_fire,"autoRetreat":c.auto_retreat,"retreating":u.retreating,
            "cooldown":number(f.cooldown),"retarget":number(f.retarget)}))
    }).collect::<Result<Vec<_>,String>>()?;
    let (branches, motion) = replay.take_observations();
    Ok(
        json!({"tick":m.world.game.tick,"nextId":m.next_unit_id,"births":births,"deaths":deaths,"units":units,
        "players":m.start.economy.players.iter().enumerate().map(|(slot,p)|json!({"slot":slot,"team":p.team,"mp":number(p.mp),"mun":number(p.mun),"fuel":number(p.fuel),"vp":number(p.vp),"inc":number(p.inc)})).collect::<Vec<_>>(),
        "score":{"winner":m.finish.map(|f|f.winner),"vp":m.start.economy.players.iter().map(|p|number(p.vp)).collect::<Vec<_>>()},
        "points":m.start.points.iter().map(|p|json!({"owner":p.owner.map_or(-1,|v|v as i64),"capper":p.capper.map_or(-1,|v|v as i64),"progress":number(p.progress),"cut":p.cut})).collect::<Vec<_>>(),
        "canShootFoe":m.can_shoot_target(actor,foe)?,"branchEvents":branches,"motion":motion,
        "retirements":retired.iter().map(|r|json!({"id":r.unit.id,"owner":r.unit.owner,"kind":m.rules.kinds[r.unit.kind].name,
            "x":number(r.unit.x),"z":number(r.unit.z),"hp":number(r.unit.hp),"lastHit":r.last_hit.map_or_else(||json!({"absent":true}),|v|number(f64::from(v)))})).collect::<Vec<_>>(),
        "drawCount":replay.receipt()["consumed"]}),
    )
}
// Comparison copies only: the four currency defaults and the separately
// authorized holdFire/holdPos absence-to-false fields. Every present value survives.
fn canonical(snapshot: &Value) -> (Value, Vec<String>, Vec<Value>) {
    let mut result = snapshot.clone();
    let mut mapped = Vec::new();
    if let Some(players) = result
        .get_mut("players")
        .and_then(Value::as_array_mut)
        .filter(|p| p.len() == 2)
    {
        for (slot, player) in players.iter_mut().enumerate() {
            if player["slot"].as_u64() != Some(slot as u64)
                || player["team"].as_u64() != Some(slot as u64)
            {
                continue;
            }
            if let Some(object) = player.as_object_mut() {
                for field in ["fuel", "mun"] {
                    if object
                        .get(field)
                        .is_none_or(|v| *v == json!({"absent":true}))
                    {
                        object.insert(field.into(), number(0.0));
                        mapped.push(format!("$.players[{slot}].{field}"));
                    }
                }
            }
        }
    }
    let mut booleans = Vec::new();
    if let Some(units) = result.get_mut("units").and_then(Value::as_array_mut) {
        for (index, unit) in units.iter_mut().enumerate() {
            if let Some(object) = unit.as_object_mut() {
                for field in ["holdFire", "holdPos"] {
                    if object
                        .get(field)
                        .is_none_or(|value| *value == json!({"absent":true}))
                    {
                        booleans.push(json!({"path":format!("$.units[{index}].{field}"),"actor":object.get("id"),"field":field}));
                        object.insert(field.into(), json!(false));
                    }
                }
            }
        }
    }
    (result, mapped, booleans)
}

type BooleanCounts = BTreeMap<(String, String, String), usize>;
fn count_mappings(
    currencies: Vec<String>,
    booleans: Vec<Value>,
    currency_counts: &mut BTreeMap<String, usize>,
    boolean_counts: &mut BooleanCounts,
) {
    for path in currencies {
        *currency_counts.entry(path).or_default() += 1;
    }
    for row in booleans {
        let key = (
            row["path"].as_str().unwrap().to_owned(),
            row["actor"].to_string(),
            row["field"].as_str().unwrap().to_owned(),
        );
        *boolean_counts.entry(key).or_default() += 1;
    }
}
fn boolean_counts(counts: &BooleanCounts) -> Vec<Value> {
    counts.iter().map(|((path, actor, field), count)| {
        let actor: Value = serde_json::from_str(actor).unwrap();
        json!({"path":path,"actor":actor,"field":field,"count":count,"default":false})
    }).collect()
}

#[test]
fn boolean_defaults_are_symmetric_and_preserve_every_present_value() {
    let frame = |field: &str, value: Option<Value>| {
        let mut frame = json!({"units":[{"id":7,"holdFire":false,"holdPos":false}]});
        let object = frame["units"][0].as_object_mut().unwrap();
        if let Some(value) = value {
            object.insert(field.into(), value);
        } else {
            object.remove(field);
        }
        frame
    };
    let equal = |a: &Value, b: &Value| {
        let originals = (a.clone(), b.clone());
        let (a_copy, _, _) = canonical(a);
        let (b_copy, _, _) = canonical(b);
        assert_eq!((a, b), (&originals.0, &originals.1));
        original::compare_projection_for_test(&a_copy, &b_copy).is_none()
    };
    for field in ["holdFire", "holdPos"] {
        let missing = frame(field, None);
        let marker = frame(field, Some(json!({"absent":true})));
        let false_value = frame(field, Some(json!(false)));
        for a in [&missing, &marker, &false_value] {
            for b in [&missing, &marker, &false_value] {
                assert!(equal(a, b));
            }
        }
        let (_, currencies, mappings) = canonical(&missing);
        assert!(currencies.is_empty());
        assert_eq!(mappings, vec![json!({"path":format!("$.units[0].{field}"),"actor":7,"field":field})]);
        for present in [json!(true), json!(false), json!(0), number(0.0), Value::Null,
            json!("false"), json!({"absent":true,"extra":0})] {
            let original = frame(field, Some(present.clone()));
            let (copy, _, mapped) = canonical(&original);
            assert_eq!(copy["units"][0][field], present);
            assert!(mapped.is_empty());
            if present != json!(false) {
                assert!(!equal(&original, &false_value));
                assert!(!equal(&false_value, &original));
                assert!(!equal(&original, &missing));
                assert!(!equal(&missing, &original));
            }
        }
        assert!(!equal(&frame(field, Some(json!(true))), &false_value));
        assert!(!equal(&false_value, &frame(field, Some(json!(true)))));
    }
    let unlisted_missing = frame("autoRetreat", None);
    let unlisted_false = frame("autoRetreat", Some(json!(false)));
    assert!(!equal(&unlisted_missing, &unlisted_false));
    assert!(!equal(&unlisted_false, &unlisted_missing));
}
fn actor(frame: &Value, id: u32) -> Option<&Value> {
    frame["units"].as_array()?.iter().find(|u| u["id"] == id)
}
fn value(unit: &Value, field: &str) -> f64 {
    unit[field]["value"].as_f64().unwrap()
}
fn distance(unit: &Value, destination: &Value) -> f64 {
    crate::math::hypot(
        value(unit, "x") - destination["x"].as_f64().unwrap(),
        value(unit, "z") - destination["z"].as_f64().unwrap(),
    )
}
fn event(frame: &Value, id: u32, kind: &str) -> bool {
    frame["branchEvents"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["id"] == id && e["event"] == kind)
}
fn moving(frame: &Value, id: u32) -> bool {
    frame["motion"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["id"] == id && e["moving"] == true)
}
fn branch_predicates(frames: &[Value], scene: &Value, name: &str) -> Value {
    let id = scene["actor"].as_u64().unwrap() as u32;
    let foe = scene["foe"].as_u64().unwrap() as u32;
    let start = scene["startTick"].as_u64().unwrap() as usize;
    let staged = frames.get(start - 1).is_some_and(|f| {
        actor(f, id).zip(actor(f, foe)).is_some_and(|(a, t)| {
            distance(a, &scene["actorStage"]) < 2.5
                && distance(t, &scene["foeStage"]) < 2.5
                && actor(&frames[0], id).is_some_and(|u| u["hp"] == a["hp"])
                && actor(&frames[0], foe).is_some_and(|u| u["hp"] == t["hp"])
        })
    });
    let halt = frames
        .iter()
        .enumerate()
        .skip(start)
        .find(|(_, f)| {
            f["branchEvents"].as_array().unwrap().iter().any(|e| {
                e["id"] == id
                    && e["event"] == "halt"
                    && e["targetId"] == foe
                    && !e["path"].as_array().unwrap().is_empty()
            }) && actor(f, id).is_some_and(|u| u["path"].as_array().unwrap().is_empty())
                && actor(f, foe).is_some_and(|u| value(u, "hp") > 0.0)
        })
        .map(|(i, _)| i);
    let stopped = halt.is_some_and(|i| {
        frames
            .get(i + 1)
            .zip(actor(&frames[i], id))
            .is_some_and(|(f, a)| {
                actor(f, id).is_some_and(|b| {
                    crate::math::hypot(value(a, "x") - value(b, "x"), value(a, "z") - value(b, "z"))
                        <= 0.001
                })
            })
    });
    let death = frames
        .iter()
        .enumerate()
        .skip(start)
        .find(|(_, f)| {
            actor(f, foe).is_none()
                && f["retirements"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|r| r["id"] == foe && value(r, "hp") <= 0.0)
        })
        .map(|(i, _)| i);
    let resume = death.and_then(|d| {
        frames
            .iter()
            .enumerate()
            .skip(d + 1)
            .find(|(_, f)| event(f, id, "resumeRequest"))
            .map(|(i, _)| i)
    });
    let progress = resume.and_then(|r| {
        actor(&frames[r], id).and_then(|a| {
            frames
                .iter()
                .enumerate()
                .skip(r + 1)
                .find(|(_, f)| {
                    actor(f, id).is_some_and(|u| {
                        moving(f, id)
                            && distance(u, &scene["goal"]) + 0.1 < distance(a, &scene["goal"])
                    })
                })
                .map(|(i, _)| i)
        })
    });
    let arrival = progress.and_then(|p| {
        frames
            .iter()
            .enumerate()
            .skip(p)
            .find(|(_, f)| {
                actor(f, id).is_some_and(|u| {
                    u["amove"].is_null()
                        && u["path"].as_array().unwrap().is_empty()
                        && distance(u, &scene["goal"]) < 2.5
                })
            })
            .map(|(i, _)| i)
    });
    let arrival_branch = arrival.is_some_and(|a| {
        frames
            .iter()
            .take(a + 1)
            .skip(start)
            .any(|f| event(f, id, "arrival"))
    });
    let pass = frames
        .iter()
        .enumerate()
        .skip(start)
        .find(|(_, f)| {
            f["canShootFoe"] == true
                && moving(f, id)
                && actor(f, id).zip(actor(f, foe)).is_some_and(|(a, t)| {
                    value(a, "x") > value(t, "x") + 2.5
                        && value(t, "hp") > 0.0
                        && a["amove"].is_null()
                        && !a["path"].as_array().unwrap().is_empty()
                })
        })
        .map(|(i, _)| i);
    let no_halt = frames.iter().skip(start).all(|f| !event(f, id, "halt"));
    let plain_arrival = pass.and_then(|p| {
        frames
            .iter()
            .enumerate()
            .skip(p)
            .find(|(_, f)| {
                actor(f, id).is_some_and(|u| {
                    u["amove"].is_null()
                        && u["path"].as_array().unwrap().is_empty()
                        && distance(u, &scene["goal"]) < 2.5
                })
            })
            .map(|(i, _)| i)
    });
    let complete = if name == "amove" {
        staged
            && stopped
            && halt.zip(death).is_some_and(|(h, d)| h < d)
            && resume.is_some()
            && progress.is_some()
            && arrival.is_some()
            && arrival_branch
    } else {
        staged && pass.is_some() && no_halt && plain_arrival.is_some()
    };
    json!({"stagedFullHealth":staged,"haltTick":halt,"stationaryAfterHalt":stopped,"deathTick":death,"resumeRequestTick":resume,"postKillProgressTick":progress,
        "arrivalTick":arrival,"arrivalBranch":arrival_branch,"plainPassShootableLivingFoeTick":pass,"plainNoAttackMoveHalt":no_halt,"plainArrivalTick":plain_arrival,"complete":complete})
}
fn prefix_alignment(source: &Value, frame: &Value, receipt: &Value) -> Result<(), Value> {
    let draws = receipt["draws"].as_array().unwrap();
    let expected = source["inputReceipt"]["draws"].as_array().unwrap();
    let count = frame["drawCount"].as_u64().unwrap() as usize;
    if draws.len() != count {
        return Err(json!({"kind":"drawCount","source":count,"native":draws.len()}));
    }
    for (i, (a, b)) in expected.iter().take(count).zip(draws).enumerate() {
        if a != b {
            return Err(json!({"kind":"draw","index":i,"source":a,"native":b}));
        }
        if a["consumer"][0]
            .as_str()
            .is_none_or(|v| v.starts_with("UNMAPPED"))
        {
            return Err(json!({"kind":"unmapped","index":i,"source":a,"native":b}));
        }
    }
    if count > expected.len() {
        return Err(json!({"kind":"sourceDrawsTruncated","source":expected.len(),"native":count}));
    }
    Ok(())
}
fn run_case(trace: &Value, authored: &Value, source: &Value) -> Result<Value, String> {
    let scene = &trace["scene"];
    let ticks = scene["ticks"].as_u64().unwrap() as usize;
    if source["completed"] != ticks
        || source["frames"]
            .as_array()
            .is_none_or(|v| v.len() != ticks + 1)
    {
        return Err("Source capture incomplete".into());
    }
    let map = crate::map::AuthoritativeMap::from_json(&serde_json::to_vec(&scene["map"]).unwrap())
        .map_err(|e| e.to_string())?;
    let replay = Replay::new(&trace["sharedInput"])?;
    let mut m = Match::new_aligned(
        &map,
        MatchOptions {
            seed: trace["sharedInput"]["seed"]
                .as_str()
                .ok_or("missing private seed")?
                .parse()
                .map_err(|_| "invalid seed")?,
            mode: Mode::Conquest,
            players: vec![
                PlayerConfig {
                    name: "A".into(),
                    team: 0,
                    faction: Faction::Usa,
                },
                PlayerConfig {
                    name: "B".into(),
                    team: 1,
                    faction: Faction::Germany,
                },
            ],
            shuffle: true,
            army: ArmySize::Standard,
            weather: WeatherChoice::from_reference(Some("clear")),
            weather_seed: None,
            living_weather: true,
            map_key: Some("aligned-amove".into()),
            tech: true,
            supply: true,
            logistics: false,
            sandbox: false,
        },
        replay.clone(),
    )?;
    for row in authored["orders"].as_array().unwrap() {
        m.enqueue_command(
            OrderKey {
                tick: row["tick"].as_u64().unwrap(),
                issuer: row["issuer"].as_u64().unwrap() as u32,
                sequence: row["sequence"].as_u64().unwrap(),
            },
            Command::from_json(&row["command"]).map_err(|e| format!("command decode: {e:?}"))?,
        )
        .map_err(|e| format!("enqueue: {e:?}"))?;
    }
    let initial_ids = m.world.game.units.keys().copied().collect::<Vec<_>>();
    let id = scene["actor"].as_u64().unwrap() as u32;
    let foe = scene["foe"].as_u64().unwrap() as u32;
    let mut native = Vec::new();
    let mut first_raw = None;
    let mut first = None;
    let mut failure = None;
    let mut checkpoints = 0;
    let mut matched = 0;
    let mut ticks_matched = 0;
    let mut source_currency_counts = BTreeMap::new();
    let mut native_currency_counts = BTreeMap::new();
    let mut source_boolean_counts = BooleanCounts::new();
    let mut native_boolean_counts = BooleanCounts::new();
    let mut denials = Vec::new();
    let mut unavailable = BTreeSet::new();
    let source_frames = source["frames"].as_array().unwrap();
    for tick in 0..=ticks {
        let (births, deaths, retired) = if tick == 0 {
            (initial_ids.clone(), Vec::new(), Vec::new())
        } else {
            let before = m.world.game.units.keys().copied().collect::<BTreeSet<_>>();
            let report = match m.advance_tick() {
                Ok(r) => r,
                Err(e) => {
                    failure = Some(
                        json!({"kind":"nativeTick","tick":tick,"reason":e.reason,"events":format!("{:?}",e.events),"retirement":format!("{:?}",e.retirement)}),
                    );
                    break;
                }
            };
            unavailable.extend(report.unavailable.iter().map(|v| format!("{v:?}")));
            for command in report.commands {
                if let Some(reason) = command.denial {
                    denials.push(json!({"tick":tick,"issuer":command.key.issuer,"sequence":command.key.sequence,"reason":format!("{reason:?}")}));
                }
            }
            let after = m.world.game.units.keys().copied().collect::<BTreeSet<_>>();
            (
                after.difference(&before).copied().collect(),
                before.difference(&after).copied().collect(),
                report.retired,
            )
        };
        let frame = match snapshot(&m, &replay, births, deaths, &retired, id, foe) {
            Ok(f) => f,
            Err(e) => {
                failure = Some(json!({"kind":"provider","tick":tick,"reason":e}));
                break;
            }
        };
        native.push(frame);
        let observed = &native[tick];
        let expected = &source_frames[tick];
        let receipt = replay.receipt();
        if tick == 0
            && (source["startup"] != receipt["startup"]
                || source["inputReceipt"]["layout"]["input"] != receipt["layout"]["input"])
        {
            failure = Some(
                json!({"kind":"startupAlignment","tick":0,"source":source["startup"],"native":receipt["startup"]}),
            );
            break;
        }
        if let Err(delta) = prefix_alignment(source, expected, &receipt) {
            failure = Some(json!({"kind":"inputAlignment","tick":tick,"difference":delta}));
            break;
        }
        if let Some(delta) = original::compare_projection_for_test(expected, observed) {
            if first_raw.is_none() {
                first_raw = Some(json!({"tick":tick,"difference":delta}));
            }
        }
        let (a, source_currency, source_boolean) = canonical(expected);
        let (b, native_currency, native_boolean) = canonical(observed);
        count_mappings(source_currency, source_boolean, &mut source_currency_counts, &mut source_boolean_counts);
        count_mappings(native_currency, native_boolean, &mut native_currency_counts, &mut native_boolean_counts);
        if tick % 10 == 0 {
            checkpoints += 1;
        }
        if let Some(delta) = original::compare_projection_for_test(&a, &b) {
            first = Some(json!({"tick":tick,"difference":delta}));
            break;
        }
        if !denials.is_empty() {
            failure = Some(json!({"kind":"commandAdmission","tick":tick,"denials":denials}));
            break;
        }
        if tick % 10 == 0 {
            matched += 1;
        }
        if tick > 0 {
            ticks_matched += 1;
        }
    }
    let source_branch = branch_predicates(source_frames, scene, authored["name"].as_str().unwrap());
    let native_branch = branch_predicates(&native, scene, authored["name"].as_str().unwrap());
    let receipt = replay.receipt();
    let full_alignment = receipt["draws"] == source["inputReceipt"]["draws"]
        && receipt["consumed"] == source["inputReceipt"]["consumed"]
        && receipt["unconsumed"] == source["inputReceipt"]["unconsumed"]
        && receipt["exhausted"] == source["inputReceipt"]["exhausted"];
    let passed = failure.is_none()
        && first.is_none()
        && native.len() == ticks + 1
        && full_alignment
        && source_branch["complete"] == true
        && native_branch["complete"] == true;
    Ok(
        json!({"name":authored["name"],"semanticVerdict":if passed{"PASS"}else if first.is_some(){"MISMATCH"}else if failure.is_some(){"HARNESS_FAILURE"}else{"BRANCH_INCOMPLETE"},
        "harnessCompleted":failure.is_none(),"failure":failure,"firstRawDifference":first_raw,"firstCanonicalDifference":first,"framesCompared":native.len(),"actualTicksMatched":ticks_matched,
        "checkpointsCompared":checkpoints,"checkpointsMatched":matched,"requestedCheckpoints":ticks/10+1,"nativeFrames":native,"sourceRawFrames":source["frames"],
        "currencyMappings":{"source":source_currency_counts,"native":native_currency_counts},
        "booleanMappings":{"fields":["holdFire","holdPos"],"default":false,"source":boolean_counts(&source_boolean_counts),"native":boolean_counts(&native_boolean_counts)},
        "nativeInputReceipt":receipt,"sourceInputReceipt":source["inputReceipt"],"fullInputAlignment":full_alignment,
        "sourceBranch":source_branch,"nativeBranch":native_branch,"denials":denials,"unavailable":unavailable,"pass":passed,"rowStatusChanged":false}),
    )
}
#[test]
#[ignore = "requires authentic private aligned scenario capture and explicit S1 grant"]
fn attack_move_and_plain_move_match_with_complete_branches() {
    let path = std::env::var_os("WW2_AMOVE_TRACE").expect("private trace path required");
    let bytes = std::fs::read(path).unwrap();
    let trace: Value = serde_json::from_slice(&bytes).unwrap();
    let scene: Value = serde_json::from_str(include_str!(
        "../../../../harness/scenarios/aligned-amove.json"
    ))
    .unwrap();
    assert_eq!(trace["format"], "aligned-amove-trace-v1");
    assert_eq!(
        trace["reference"],
        "e4e579a838aabae3b1d9dadbc17a0a87147bc52d"
    );
    assert_eq!(trace["scene"], scene);
    assert_eq!(trace["failure"], Value::Null);
    assert_eq!(trace["instrumentation"]["erasesExactly"], true);
    assert_eq!(trace["sharedInput"]["layout"], scene["layout"]);
    assert_eq!(
        trace["sharedInput"]["tape"].as_array().unwrap().len(),
        16384
    );
    let mut results = Vec::new();
    let mut failure = None;
    for (authored, source) in scene["cases"]
        .as_array()
        .unwrap()
        .iter()
        .zip(trace["cases"].as_array().unwrap())
    {
        assert_eq!(authored["name"], source["name"]);
        match run_case(&trace, authored, source) {
            Ok(r) => {
                let pass = r["pass"] == true;
                results.push(r);
                if !pass {
                    break;
                }
            }
            Err(e) => {
                failure = Some(e);
                break;
            }
        }
    }
    let passed =
        results.len() == 2 && results.iter().all(|r| r["pass"] == true) && failure.is_none();
    let report = json!({"format":"aligned-amove-report-v1","sourceHead":trace["sourceHead"],"nativeHead":std::env::var("WW2_AMOVE_NATIVE_HEAD").expect("frozen head required"),
        "pass":passed,"cases":results,"failure":failure,"sourceFixtureSha256":std::env::var("WW2_AMOVE_TRACE_SHA256").expect("private fixture binding required"),
        "currencyWhitelist":["$.players[0].fuel","$.players[0].mun","$.players[1].fuel","$.players[1].mun"],
        "booleanWhitelist":["$.units[*].holdFire","$.units[*].holdPos"],"booleanDefault":false,
        "booleanAuthority":"User follow-up after diagnostic6196700421: missing key or exact absence marker only; every present value retained; comparison copies only",
        "rowStatusChanged":false});
    std::fs::write(
        std::env::var_os("WW2_AMOVE_REPORT").expect("private report path required"),
        serde_json::to_vec(&report).unwrap(),
    )
    .unwrap();
    println!(
        "{}",
        json!({"pass":passed,"failure":failure,"cases":results.iter().map(|r|json!({"name":r["name"],"semanticVerdict":r["semanticVerdict"],"firstCanonicalDifference":r["firstCanonicalDifference"],"framesCompared":r["framesCompared"],"checkpointsMatched":r["checkpointsMatched"],"checkpointsCompared":r["checkpointsCompared"],"sourceBranch":r["sourceBranch"],"nativeBranch":r["nativeBranch"]})).collect::<Vec<_>>()})
    );
    assert!(
        passed,
        "actual comparison or complete branch failed; full private report retained"
    );
}
