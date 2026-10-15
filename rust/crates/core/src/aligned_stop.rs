//! CMB-140 diagnostic using real constructors, commands and ordinary ticks only.
use super::*;

fn registry() -> Value {
    serde_json::from_str(include_str!(
        "../../../../docs/testing/aligned-stop-proposed-defaults.json"
    ))
    .unwrap()
}
fn queued_order(command: &Command) -> Result<Value, String> {
    match command {
        Command::Move {
            orders,
            attack_move,
            face,
            control_ids,
            queue,
            together,
        } => {
            if control_ids.is_some() || *queue || *together {
                return Err("unrepresented internal waiting Move flags".into());
            }
            if orders.len() != 1 {
                return Err("waiting Move must have one actual typed actor".into());
            }
            let at = orders[0].at;
            let mut value = json!({"t":if *attack_move {"amove"}else{"move"},"orders":[[number(orders[0].unit as f64),number(at.x),number(at.z)]],"x":number(at.x),"z":number(at.z)});
            if let Some(angle) = face {
                value["face"] = number(*angle);
            }
            Ok(value)
        }
        Command::Attack {
            ids,
            target,
            at: Some(at),
            queue: false,
        } => Ok(
            json!({"t":"attack","ids":ids.iter().map(|id|number(*id as f64)).collect::<Vec<_>>(),"target":number(*target as f64),"x":number(at.x),"z":number(at.z)}),
        ),
        other => Err(format!(
            "unrepresented queued command, never replace with empty output: {other:?}"
        )),
    }
}
fn traffic(value: Option<&crate::game::Traffic>) -> Value {
    value.map_or(Value::Null, |t| {
        let mut value =
            json!({"goal":point(t.goal),"by":number(f64::from(t.by)),"until":number(t.until)});
        if t.passing {
            value["passing"] = json!(true);
            value["legs"] = json!(t.legs.iter().copied().map(point).collect::<Vec<_>>());
        }
        value
    })
}
fn stop_snapshot(
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
        if u.dig || u.entrench || u.build != c.build_target.is_some() {
            return Err(format!("active field-work projection outside authored ground Stop scene: {}", u.id));
        }
        let orders = c.orders.iter().map(queued_order).collect::<Result<Vec<_>, _>>()?;
        Ok(json!({"id":u.id,"owner":u.owner,"kind":m.rules.kinds[u.kind].name,"x":number(u.x),"z":number(u.z),"hp":number(u.hp),"supp":number(u.supp),
            "path":u.path.iter().copied().map(point).collect::<Vec<_>>(),"rot":number(u.rot),"aim":number(c.aim),"moveSpeed":number(u.move_speed),
            "vx":optional_number(u.vx),"vz":optional_number(u.vz),"travelDir":optional_number(u.travel_dir),"reverse":goal(u.reverse),
            "repath":number(u.repath),"stuck":number(u.stuck),"still":optional_number(u.still),"orders":orders,"worldGoal":goal(u.world_goal),"follow":number(f64::from(c.follow)),
            "build":number(f64::from(c.build_target.unwrap_or(0))),"entrench":Value::Null,"dig":Value::Null,"nade":goal(c.nade),
            "enter":c.enter.map_or(-1, |v| v as i64),"board":u.board,"fireAt":c.fire_at.map_or(-1, |v| v as i64),
            "traffic":traffic(u.traffic.as_ref()),"trafficWait":number(u.traffic_wait),
            "moveOutcome":c.outcome.map_or_else(||json!({"absent":true}), |v|json!(v.name())),
            "moveOutcomeTick":c.outcome_tick.map_or_else(||json!({"absent":true}), |v|number(v as f64)),
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
        "drawCount":replay.0.lock().unwrap().used.len()}),
    )
}
fn diagnostic(frame: &Value) -> (Value, Vec<String>, Vec<Value>) {
    let (mut copy, currencies, mut mappings) = proposed_defaults(frame);
    for (index, unit) in copy["units"].as_array_mut().unwrap().iter_mut().enumerate() {
        for entry in registry()["fields"].as_array().unwrap() {
            let field = entry["field"].as_str().unwrap();
            let object = unit.as_object_mut().unwrap();
            if object
                .get(field)
                .is_none_or(|v| *v == json!({"absent":true}))
            {
                let default = if entry["numberBits"].is_string() {
                    number(0.0)
                } else {
                    entry["default"].clone()
                };
                mappings.push(json!({"path":format!("$.units[{index}].{field}"),"actor":object.get("id"),"field":field,"default":default}));
                object.insert(field.into(), default);
            }
        }
    }
    // Parent boolean mapping records already identify false explicitly by registry.
    for row in &mut mappings {
        if row.get("default").is_none() {
            row["default"] = json!(false);
        }
    }
    (copy, currencies, mappings)
}
#[test]
fn stop_registry_preserves_present_values_and_unknown_fields() {
    assert_eq!(registry()["ownerApproval"], "PENDING");
    assert_eq!(registry()["parityEligible"], false);
    assert_eq!(registry()["fields"].as_array().unwrap().len(), 6);
    for entry in registry()["fields"].as_array().unwrap() {
        let field = entry["field"].as_str().unwrap();
        for present in [
            Value::Null,
            json!(false),
            json!(true),
            number(0.0),
            number(-0.0),
            number(f64::NAN),
            number(f64::INFINITY),
            json!(0),
            json!("0"),
            json!([]),
            json!({}),
            json!({"absent":true,"extra":0}),
        ] {
            let frame = json!({"units":[{"id":1,(field):present}]});
            let (result, _, mapping) = diagnostic(&frame);
            assert_eq!(result["units"][0][field], frame["units"][0][field]);
            assert!(!mapping.iter().any(|row| row["field"] == field));
        }
        for missing in [
            json!({"units":[{"id":1}]}),
            json!({"units":[{"id":1,(field):{"absent":true}}]}),
        ] {
            let original = missing.clone();
            let (copy, _, mapping) = diagnostic(&missing);
            assert_eq!(missing, original);
            let expected = if entry["numberBits"].is_string() {
                number(0.0)
            } else {
                entry["default"].clone()
            };
            assert_eq!(copy["units"][0][field], expected);
            assert_eq!(
                mapping.iter().filter(|row| row["field"] == field).count(),
                1
            );
        }
    }
    let unknown = json!({"units":[{"id":1,"notListed":{"absent":true},"holdFire":false,"holdPos":false,"orders":[1,2]}]});
    let (copy, _, _) = diagnostic(&unknown);
    assert_eq!(
        copy["units"][0]["notListed"],
        unknown["units"][0]["notListed"]
    );
    assert_eq!(copy["units"][0]["orders"], unknown["units"][0]["orders"]);
}
fn cleared(unit: &Value) -> bool {
    unit["amove"].is_null()
        && unit["attackId"] == 0
        && unit["orders"] == json!([])
        && unit["path"] == json!([])
        && unit["face"].is_null()
        && unit["nade"].is_null()
        && unit["dig"].is_null()
        && unit["entrench"].is_null()
        && unit["build"]["bits"] == "0000000000000000"
        && unit["enter"] == -1
        && unit["board"] == 0
        && unit["fireAt"] == -1
        && unit["retreating"] == false
        && unit["worldGoal"].is_null()
        && unit["traffic"].is_null()
}
fn qualification(observations: &[Value]) -> Value {
    let Some(stop) = observations.iter().find(|o| o["label"] == "batch-stop") else {
        return json!({"complete":false,"missing":"actual batch Stop"});
    };
    let a = actor(&stop["before"], 1).unwrap();
    let b = actor(&stop["before"], 2).unwrap();
    let c = actor(&stop["before"], 3).unwrap();
    let preconditions = json!({"actorAAmove":!a["amove"].is_null(),"actorAPath":!a["path"].as_array().unwrap().is_empty(),"actorCPath":!c["path"].as_array().unwrap().is_empty(),"actorBAttack":b["attackId"]==4,
        "actorCQueue":c["orders"].as_array().is_some_and(|q|q.len()==2&&q[0]["t"]=="move"&&q[1]["t"]=="attack"),"foreignMoving":!actor(&stop["before"],4).unwrap()["path"].as_array().unwrap().is_empty()});
    let postconditions = (1..=3)
        .map(|id| json!({"id":id,"cleared":cleared(actor(&stop["after"],id).unwrap())}))
        .collect::<Vec<_>>();
    let controls=observations.iter().filter(|o|["foreign-only-denied","empty-denied","no-own-denied"].iter().any(|label|o["label"]==*label))
        .map(|o|json!({"label":o["label"],"needs":o["denial"]=="needs","unchanged":o["before"]==o["after"]})).collect::<Vec<_>>();
    let mixed = observations.iter().find(|o| o["label"] == "mixed-stop");
    let foreign_preserved = actor(&stop["before"], 4) == actor(&stop["after"], 4)
        && mixed.is_some_and(|o| actor(&o["before"], 4) == actor(&o["after"], 4));
    let complete = preconditions
        .as_object()
        .unwrap()
        .values()
        .all(|v| v == true)
        && postconditions.iter().all(|r| r["cleared"] == true)
        && controls.len() == 3
        && controls
            .iter()
            .all(|r| r["needs"] == true && r["unchanged"] == true)
        && foreign_preserved;
    json!({"preconditions":preconditions,"postconditions":postconditions,"controls":controls,"foreignPreserved":foreign_preserved,"complete":complete,"simultaneousSingleActorClaim":false})
}
#[test]
#[ignore = "requires new authentic private Stop capture and Root S3 grant"]
fn three_actor_stop_clears_real_intents_and_preserves_foreign_controls() {
    let trace_bytes =
        std::fs::read(std::env::var_os("WW2_STOP_TRACE").expect("private new Stop capture"))
            .unwrap();
    let trace: Value = serde_json::from_slice(&trace_bytes).unwrap();
    let binding: Value = serde_json::from_slice(
        &std::fs::read(std::env::var_os("WW2_STOP_BINDING").expect("authentic input binding"))
            .unwrap(),
    )
    .unwrap();
    let scene: Value = serde_json::from_str(include_str!(
        "../../../../harness/scenarios/aligned-stop.json"
    ))
    .unwrap();
    let input: Value = serde_json::from_slice(
        &std::fs::read(std::env::var_os("WW2_STOP_INPUT").expect("authored shared tape")).unwrap(),
    )
    .unwrap();
    assert_eq!(trace["format"], "aligned-stop-trace-v1");
    assert_eq!(trace["reference"], registry()["reference"]);
    assert_eq!(trace["sourceHead"], binding["sourceHead"]);
    assert_eq!(binding["bytes"], trace_bytes.len());
    assert_eq!(trace["scene"], scene);
    assert_eq!(trace["sharedInput"], input);
    assert_eq!(trace["failure"], Value::Null);
    assert_eq!(trace["proposedRegistry"], registry());
    assert_eq!(trace["instrumentation"]["erasesExactly"], true);
    assert_eq!(trace["instrumentation"]["zeroEngineEffects"], true);
    let source = &trace["cases"][0];
    assert_eq!(source["qualification"]["complete"], true);
    let map = crate::map::AuthoritativeMap::from_json(&serde_json::to_vec(&scene["map"]).unwrap())
        .unwrap();
    let replay = Replay::new(&input).unwrap();
    let mut m = Match::new_aligned(
        &map,
        MatchOptions {
            seed: input["seed"].as_str().unwrap().parse().unwrap(),
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
    )
    .unwrap();
    let ticks = scene["ticks"].as_u64().unwrap() as usize;
    let mut native = Vec::new();
    let mut observations = Vec::new();
    let mut checked = 0;
    let mut first_raw = None;
    let mut first_approved = None;
    let mut first_proposed = None;
    let mut failure = None;
    let mut checkpoint_count = 0;
    let mut matched = 0;
    let mut raw_matched = 0;
    let mut approved_matched = 0;
    let mut source_currency = BTreeMap::<String, usize>::new();
    let mut native_currency = BTreeMap::<String, usize>::new();
    let mut mappings = BTreeMap::<String, usize>::new();
    let mut observation_index = 0;
    'ticks: for tick in 0..=ticks {
        let (before_ids, retired) = if tick == 0 {
            (BTreeSet::new(), Vec::new())
        } else {
            for row in scene["orders"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|row| row["tick"] == tick)
            {
                let before = stop_snapshot(&m, &replay, vec![], vec![], &[], 1, 4).unwrap();
                let command = Command::from_json(&row["command"]).unwrap();
                let winner_decided = m.finish.is_some();
                let denial = match m.execute_command(
                    row["issuer"].as_u64().unwrap() as usize,
                    &command,
                    winner_decided,
                ) {
                    Ok(d) => d.map_or(Value::Null, |d| json!(d.name())),
                    Err(e) => {
                        failure = Some(
                            json!({"tick":tick,"kind":"admission","label":row["label"],"reason":e}),
                        );
                        break 'ticks;
                    }
                };
                let after = stop_snapshot(&m, &replay, vec![], vec![], &[], 1, 4).unwrap();
                let observation = json!({"tick":tick,"label":row["label"],"issuer":row["issuer"],"denial":denial,"before":before,"after":after});
                let expected = &source["observations"][observation_index];
                observation_index += 1;
                if denial != row["expectedDenial"] {
                    failure = Some(
                        json!({"tick":tick,"kind":"commandDenial","label":row["label"],"source":row["expectedDenial"],"native":denial}),
                    );
                }
                for phase in ["before", "after"] {
                    let (a, _, _) = diagnostic(&expected[phase]);
                    let (b, _, _) = diagnostic(&observation[phase]);
                    if let Some(delta) = original::compare_projection_for_test(&a, &b) {
                        first_proposed = Some(
                            json!({"tick":tick,"label":row["label"],"phase":phase,"difference":delta}),
                        );
                        break;
                    }
                }
                observations.push(observation);
                if first_proposed.is_some() || failure.is_some() {
                    break 'ticks;
                }
            }
            let before = m.world.game.units.keys().copied().collect::<BTreeSet<_>>();
            let report = match m.advance_tick() {
                Ok(r) => r,
                Err(e) => {
                    failure = Some(json!({"tick":tick,"kind":"nativeTick","reason":e.reason}));
                    break;
                }
            };
            (before, report.retired)
        };
        let after = m.world.game.units.keys().copied().collect::<BTreeSet<_>>();
        let (frame_births, frame_deaths) = if tick == 0 {
            (after.iter().copied().collect(), vec![])
        } else {
            (
                after.difference(&before_ids).copied().collect(),
                before_ids.difference(&after).copied().collect(),
            )
        };
        let frame = match stop_snapshot(&m, &replay, frame_births, frame_deaths, &retired, 1, 4) {
            Ok(v) => v,
            Err(e) => {
                failure = Some(json!({"tick":tick,"kind":"provider","reason":e}));
                break;
            }
        };
        let expected = &source["frames"][tick];
        if let Err(delta) = prefix_alignment(source, expected, &replay, &mut checked) {
            failure = Some(json!({"tick":tick,"kind":"inputAlignment","difference":delta}));
        }
        if tick == 0 && replay.receipt()["startup"] != source["startup"] {
            failure = Some(json!({"tick":0,"kind":"startupAlignment"}));
        }
        let raw = original::compare_projection_for_test(expected, &frame);
        if first_raw.is_none() {
            first_raw = raw.as_ref().map(|d| json!({"tick":tick,"difference":d}));
        }
        let (a, _) = approved_currency_only(expected);
        let (b, _) = approved_currency_only(&frame);
        let approved = original::compare_projection_for_test(&a, &b);
        if first_approved.is_none() {
            first_approved = approved
                .as_ref()
                .map(|d| json!({"tick":tick,"difference":d}));
        }
        let (a, sc, sm) = diagnostic(expected);
        let (b, nc, nm) = diagnostic(&frame);
        for p in sc {
            *source_currency.entry(p).or_default() += 1;
        }
        for p in nc {
            *native_currency.entry(p).or_default() += 1;
        }
        for (side, rows) in [("source", sm), ("native", nm)] {
            for row in rows {
                let key = json!({"side":side,"mapping":row}).to_string();
                *mappings.entry(key).or_default() += 1;
            }
        }
        let proposed = original::compare_projection_for_test(&a, &b);
        if let Some(delta) = proposed.as_ref() {
            first_proposed = Some(json!({"tick":tick,"difference":delta}));
        }
        if tick % 10 == 0 {
            checkpoint_count += 1;
            if raw.is_none() {
                raw_matched += 1;
            }
            if approved.is_none() {
                approved_matched += 1;
            }
            if proposed.is_none() {
                matched += 1;
            }
        }
        native.push(frame);
        if first_proposed.is_some() || failure.is_some() {
            break;
        }
    }
    let receipt = replay.receipt();
    let qualified = qualification(&observations);
    let full_input = receipt["draws"] == source["inputReceipt"]["draws"]
        && receipt["consumed"] == source["inputReceipt"]["consumed"]
        && receipt["unconsumed"] == source["inputReceipt"]["unconsumed"]
        && receipt["startup"] == source["startup"]
        && receipt["layout"]["input"] == source["inputReceipt"]["layout"]["input"];
    let complete = native.len() == ticks + 1
        && first_proposed.is_none()
        && failure.is_none()
        && full_input
        && qualified["complete"] == true;
    let mapped = mappings
        .into_iter()
        .map(|(key, count)| {
            let mut row: Value = serde_json::from_str(&key).unwrap();
            row["count"] = json!(count);
            row
        })
        .collect::<Vec<_>>();
    let report = json!({"format":"aligned-stop-report-v1","mode":"DIAGNOSTIC_PROPOSED_DEFAULTS","ownerApproval":"PENDING","parityEligible":false,"nativeHead":std::env::var("WW2_STOP_HEAD").unwrap(),"sourceHead":trace["sourceHead"],"binding":binding,
        "rawStrict":{"firstDifference":first_raw,"checkpointsMatched":raw_matched},"approvedCurrencyOnly":{"firstDifference":first_approved,"checkpointsMatched":approved_matched},
        "proposedDiagnostic":{"firstDifference":first_proposed,"checkpointsMatched":matched,"checkpointsCompared":checkpoint_count},"failure":failure,"framesCompared":native.len(),"nativeTicksAdvanced":m.world.game.tick,"prefixDrawsCompared":checked,"fullInputAlignment":full_input,
        "currencyMappings":{"source":source_currency,"native":native_currency},"proposedMappings":mapped,"parentRegistry":proposed_registry(),"stopRegistry":registry(),"sourceQualification":source["qualification"],"nativeQualification":qualified,"nativeInputReceipt":receipt,"sourceInputReceipt":source["inputReceipt"],"nativeObservations":observations,"sourceObservations":source["observations"],"nativeFrames":native,"sourceRawFrames":source["frames"],"diagnosticMatched":complete,"rowStatusChanged":false});
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(std::env::var_os("WW2_STOP_REPORT").unwrap())
        .unwrap();
    serde_json::to_writer(std::io::BufWriter::new(file), &report).unwrap();
    println!(
        "{}",
        json!({"diagnosticMatched":complete,"framesCompared":report["framesCompared"],"firstProposedDifference":report["proposedDiagnostic"]["firstDifference"],"failure":report["failure"],"prefixDrawsCompared":checked,"qualification":report["nativeQualification"]})
    );
    assert!(
        complete,
        "Stop diagnostic incomplete or differing; full raw report retained"
    );
}
