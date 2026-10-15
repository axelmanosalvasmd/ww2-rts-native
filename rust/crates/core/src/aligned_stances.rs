//! Genuine-constructor stance scenario. Only test compilation registers this module.
use super::*;

fn recipient_flags(m: &Match) -> Result<Value, String> {
    let project = |cache: &crate::recipient_snapshot::SnapshotCache<'_>, slot| {
        let view = cache
            .recipient(slot)
            .map_err(|e| format!("recipient provider: {e:?}"))?;
        let rows = view.fields["units"]
            .as_array()
            .ok_or("recipient units absent")?;
        Ok::<_, String>(
            json!({"slot":slot,"units":rows.iter().map(|r|json!({"id":r[0],"owner":r[2],"flags":r[12]})).collect::<Vec<_>>()}),
        )
    };
    let mut direct = Vec::new();
    for slot in 0..2 {
        let cache = crate::recipient_snapshot::SnapshotCache::new(m)
            .map_err(|e| format!("fresh recipient cache: {e:?}"))?;
        direct.push(project(&cache, slot)?);
    }
    let cache = crate::recipient_snapshot::SnapshotCache::new(m)
        .map_err(|e| format!("shared recipient cache: {e:?}"))?;
    let cached = (0..2)
        .map(|slot| project(&cache, slot))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(json!({"direct":direct,"cached":cached}))
}
fn dispatch(m: &mut Match, authored: &Value, tick: usize) -> Result<Vec<Value>, String> {
    let mut receipts = Vec::new();
    for row in authored["orders"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["tick"] == tick)
    {
        let before = recipient_flags(m)?;
        // This actual decoder can faithfully reject malformed JSON calls. No invented Command variant.
        let denial = match Command::from_json(&row["command"]) {
            Ok(command) => {
                m.execute_command(row["issuer"].as_u64().unwrap() as usize, &command, false)?
            }
            Err(reason) => Some(reason),
        };
        receipts.push(json!({"tick":tick,"issuer":row["issuer"],"sequence":row["sequence"],"label":row["label"],"command":row["command"],
            "denial":denial.map(|r|r.name()),"before":before,"after":recipient_flags(m)?}));
    }
    Ok(receipts)
}
#[allow(clippy::too_many_arguments)]
fn stance_snapshot(
    m: &Match,
    replay: &Replay,
    births: Vec<u32>,
    deaths: Vec<u32>,
    retired: &[crate::match_combat::RetiredUnit],
    actor: u32,
    foe: u32,
    commands: Vec<Value>,
) -> Result<Value, String> {
    let mut frame = snapshot(m, replay, births, deaths, retired, actor, foe)?;
    frame["recipientFlags"] = recipient_flags(m)?;
    frame["commands"] = json!(commands);
    Ok(frame)
}
fn branches(frames: &[Value], scene: &Value) -> Value {
    let unit = |tick: usize, id| frames.get(tick).and_then(|f| actor(f, id));
    let n = |tick, id, key| unit(tick, id).map(|u| value(u, key));
    let same = |a: Option<&Value>, b: Option<&Value>| {
        a.zip(b).is_some_and(|(a, b)| {
            a["x"]["bits"] == b["x"]["bits"] && a["z"]["bits"] == b["z"]["bits"]
        })
    };
    let commands = frames
        .iter()
        .flat_map(|f| f["commands"].as_array().unwrap())
        .collect::<Vec<_>>();
    let malformed = commands
        .iter()
        .filter(|c| {
            c["label"].as_str().unwrap().starts_with("malformed-") || c["label"] == "missing-on"
        })
        .collect::<Vec<_>>();
    let foreign = commands
        .iter()
        .filter(|c| c["label"].as_str().unwrap().starts_with("foreign-"))
        .collect::<Vec<_>>();
    let authored = scene["cases"][0]["orders"].as_array().unwrap();
    let dispositions = commands.len() == authored.len()
        && commands
            .iter()
            .zip(authored)
            .all(|(a, b)| a["denial"] == b["expectedDenial"]);
    let in_range = frames
        .iter()
        .skip(500)
        .take(101)
        .any(|f| f["canShootFoe"] == true);
    let held_fire = in_range && n(500, 4, "hp") == Some(100.0) && n(600, 4, "hp") == Some(100.0);
    let incoming = n(600, 1, "hp")
        .zip(n(520, 1, "hp"))
        .is_some_and(|(a, b)| a < b);
    let held_position =
        incoming && frames.len() > 600 && (520..=600).all(|tick| same(unit(tick, 1), unit(520, 1)));
    let explicit = n(700, 4, "hp")
        .zip(n(619, 4, "hp"))
        .is_some_and(|(a, b)| a < b);
    let resumed = n(779, 4, "hp")
        .zip(n(701, 4, "hp"))
        .is_some_and(|(a, b)| a < b);
    let released = (821..frames.len())
        .find(|&tick| unit(tick, 1).is_some() && !same(unit(tick, 1), unit(820, 1)));
    let toggles = [
        (16, "holdPos", false),
        (17, "holdPos", true),
        (20, "autoRetreat", true),
        (21, "autoRetreat", false),
        (702, "holdFire", false),
        (780, "holdFire", true),
    ]
    .iter()
    .all(|&(tick, key, on)| unit(tick, 1).is_some_and(|u| u[key] == on));
    let mut owner_bits = 0;
    let mut visible_foreign = 0;
    let mut leaked = false;
    for f in frames {
        for mode in ["direct", "cached"] {
            for view in f["recipientFlags"][mode].as_array().unwrap() {
                for row in view["units"].as_array().unwrap() {
                    let flags = row["flags"].as_u64().unwrap();
                    if row["owner"] == view["slot"] {
                        owner_bits |= flags & 30720;
                    } else {
                        leaked |= flags & 30720 != 0;
                        let owner = row["owner"].as_u64().unwrap() as usize;
                        if f["recipientFlags"][mode][owner]["units"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .any(|u| {
                                u["id"] == row["id"] && u["flags"].as_u64().unwrap() & 30720 != 0
                            })
                        {
                            visible_foreign += 1;
                        }
                    }
                }
            }
        }
    }
    let privacy = owner_bits == 30720 && visible_foreign > 0 && !leaked;
    let malformed_blocked =
        malformed.len() == 5 && malformed.iter().all(|c| c["denial"] == "blocked");
    let foreign_needs = foreign.len() == 2 && foreign.iter().all(|c| c["denial"] == "needs");
    json!({"allDispositions":dispositions,"malformedBlocked":malformed_blocked,"foreignNeeds":foreign_needs,"inRange":in_range,"holdFirePreventsDamage":held_fire,
        "naturalIncoming":incoming,"holdPositionUnderFire":held_position,"explicitAttackOverridesHoldFire":explicit,"holdFireOffResumesDamage":resumed,"releasedMoveTick":released,
        "actualBooleanToggles":toggles,"ownerBits":owner_bits,"visibleForeignPrivateChecks":visible_foreign,"ownerOnlyRecipientBits":privacy,
        "complete":dispositions&&malformed_blocked&&foreign_needs&&held_fire&&held_position&&explicit&&resumed&&released.is_some()&&toggles&&privacy})
}
fn run_case(
    trace: &Value,
    authored: &Value,
    source: &Value,
    writer: &mut impl Write,
) -> Result<Value, String> {
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
            map_key: Some("aligned-stances".into()),
            tech: true,
            supply: true,
            logistics: false,
            sandbox: false,
        },
        replay.clone(),
    )?;
    let initial_ids = m.world.game.units.keys().copied().collect::<Vec<_>>();
    let id = scene["actor"].as_u64().unwrap() as u32;
    let foe = scene["foe"].as_u64().unwrap() as u32;
    let mut native = Vec::new();
    let mut first_raw = None;
    let mut first_approved = None;
    let mut raw_checkpoints_matched = 0;
    let mut approved_checkpoints_matched = 0;
    let mut checked_draws = 0;
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
        let commands = if tick == 0 {
            Vec::new()
        } else {
            dispatch(&mut m, authored, tick)?
        };
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
        let frame = match stance_snapshot(&m, &replay, births, deaths, &retired, id, foe, commands)
        {
            Ok(f) => f,
            Err(e) => {
                failure = Some(json!({"kind":"provider","tick":tick,"reason":e}));
                break;
            }
        };
        native.push(frame);
        let observed = &native[tick];
        let expected = &source_frames[tick];
        let receipt = if tick == 0 {
            replay.receipt()
        } else {
            Value::Null
        };
        if tick == 0
            && (source["startup"] != receipt["startup"]
                || source["inputReceipt"]["layout"]["input"] != receipt["layout"]["input"])
        {
            failure = Some(
                json!({"kind":"startupAlignment","tick":0,"source":source["startup"],"native":receipt["startup"]}),
            );
            break;
        }
        if let Err(delta) = prefix_alignment(source, expected, &replay, &mut checked_draws) {
            failure = Some(json!({"kind":"inputAlignment","tick":tick,"difference":delta}));
            break;
        }
        if let Some(delta) = original::compare_projection_for_test(expected, observed) {
            if first_raw.is_none() {
                first_raw = Some(json!({"tick":tick,"difference":delta}));
            }
        } else if tick % 10 == 0 {
            raw_checkpoints_matched += 1;
        }
        let (approved_source, _) = approved_currency_only(expected);
        let (approved_native, _) = approved_currency_only(observed);
        if let Some(delta) =
            original::compare_projection_for_test(&approved_source, &approved_native)
        {
            if first_approved.is_none() {
                first_approved = Some(json!({"tick":tick,"difference":delta}));
            }
        } else if tick % 10 == 0 {
            approved_checkpoints_matched += 1;
        }
        let (a, source_currency, source_boolean) = proposed_defaults(expected);
        let (b, native_currency, native_boolean) = proposed_defaults(observed);
        count_mappings(
            source_currency,
            source_boolean,
            &mut source_currency_counts,
            &mut source_boolean_counts,
        );
        count_mappings(
            native_currency,
            native_boolean,
            &mut native_currency_counts,
            &mut native_boolean_counts,
        );
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
    let source_branch = branches(source_frames, scene);
    let native_branch = branches(&native, scene);
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
    let metadata = json!({"name":authored["name"],"mode":"DIAGNOSTIC_PROPOSED_DEFAULTS","ownerApproval":"PENDING","parityEligible":false,
        "diagnosticVerdict":if passed{"MATCH"}else if first.is_some(){"MISMATCH"}else if failure.is_some(){"HARNESS_FAILURE"}else{"BRANCH_INCOMPLETE"},
        "rawStrict":{"firstDifference":first_raw,"checkpointsMatched":raw_checkpoints_matched},
        "approvedCurrencyOnly":{"firstDifference":first_approved,"checkpointsMatched":approved_checkpoints_matched},
        "prefixDrawsCompared":checked_draws,"nativeTicksAdvanced":m.world.game.tick,
        "harnessCompleted":failure.is_none(),"failure":failure,"firstRawDifference":first_raw,"firstProposedDifference":first,"framesCompared":native.len(),"actualTicksMatched":ticks_matched,
        "checkpointsCompared":checkpoints,"checkpointsMatched":matched,"requestedCheckpoints":ticks/10+1,
        "currencyMappings":{"source":source_currency_counts,"native":native_currency_counts},
        "booleanMappings":{"fields":["holdFire","holdPos"],"default":false,"source":boolean_counts(&source_boolean_counts),"native":boolean_counts(&native_boolean_counts)},
        "nativeInputReceipt":receipt,"sourceInputReceipt":source["inputReceipt"],"fullInputAlignment":full_alignment,
        "sourceBranch":source_branch,"nativeBranch":native_branch,"denials":denials,"unavailable":unavailable,"diagnosticMatched":passed,"rowStatusChanged":false});
    stream_case(writer, &metadata, &native, source)?;
    Ok(metadata)
}
#[test]
#[ignore = "requires authentic private aligned scenario capture and explicit S1 grant"]
fn stances_match_with_complete_behavior_and_recipient_branches() {
    let path = std::env::var_os("WW2_STANCES_TRACE").expect("private trace path required");
    let bytes = std::fs::read(path).unwrap();
    let trace: Value = serde_json::from_slice(&bytes).unwrap();
    let scene: Value = serde_json::from_str(include_str!(
        "../../../../harness/scenarios/aligned-stances.json"
    ))
    .unwrap();
    assert_eq!(trace["format"], "aligned-stances-trace-v1");
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
    let output = std::env::var_os("WW2_STANCES_REPORT").expect("private report path required");
    let mut writer = BufWriter::new(std::fs::File::create(output).unwrap());
    let header = json!({"format":"aligned-stances-report-v1","mode":"DIAGNOSTIC_PROPOSED_DEFAULTS","ownerApproval":"PENDING","parityEligible":false,
        "sourceHead":trace["sourceHead"],"nativeHead":std::env::var("WW2_STANCES_NATIVE_HEAD").expect("frozen head required"),
        "sourceFixtureSha256":std::env::var("WW2_STANCES_TRACE_SHA256").expect("private fixture binding required"),
        "currencyWhitelist":["$.players[0].fuel","$.players[0].mun","$.players[1].fuel","$.players[1].mun"],
        "proposedDefaultRegistry":proposed_registry(),"rowStatusChanged":false});
    let mut header = serde_json::to_vec(&header).unwrap();
    assert_eq!(header.pop(), Some(b'}'));
    writer.write_all(&header).unwrap();
    writer.write_all(b",\"cases\":[").unwrap();
    let mut results = Vec::new();
    let mut failure = None;
    for (authored, source) in scene["cases"]
        .as_array()
        .unwrap()
        .iter()
        .zip(trace["cases"].as_array().unwrap())
    {
        assert_eq!(authored["name"], source["name"]);
        if !results.is_empty() {
            writer.write_all(b",").unwrap();
        }
        match run_case(&trace, authored, source, &mut writer) {
            Ok(result) => {
                let matched = result["diagnosticMatched"] == true;
                println!(
                    "{}",
                    json!({"mode":"DIAGNOSTIC_PROPOSED_DEFAULTS","name":result["name"],"diagnosticVerdict":result["diagnosticVerdict"],
                    "firstProposedDifference":result["firstProposedDifference"],"rawStrict":result["rawStrict"],"approvedCurrencyOnly":result["approvedCurrencyOnly"],
                    "framesCompared":result["framesCompared"],"checkpointsCompared":result["checkpointsCompared"],"checkpointsMatched":result["checkpointsMatched"],
                    "actualTicksMatched":result["actualTicksMatched"],"prefixDrawsCompared":result["prefixDrawsCompared"],"fullInputAlignment":result["fullInputAlignment"],
                    "sourceBranch":result["sourceBranch"],"nativeBranch":result["nativeBranch"],"booleanMappings":result["booleanMappings"]})
                );
                results.push(result);
                writer.flush().unwrap();
                if !matched {
                    break;
                }
            }
            Err(error) => {
                serde_json::to_writer(&mut writer, &json!({"name":authored["name"],"mode":"DIAGNOSTIC_PROPOSED_DEFAULTS","admissionFailure":error,"parityEligible":false})).unwrap();
                failure = Some(error);
                break;
            }
        }
    }
    let matched = results.len() == 1
        && results.iter().all(|r| r["diagnosticMatched"] == true)
        && failure.is_none();
    writer.write_all(b"],\"diagnosticMatched\":").unwrap();
    serde_json::to_writer(&mut writer, &matched).unwrap();
    writer.write_all(b",\"failure\":").unwrap();
    serde_json::to_writer(&mut writer, &failure).unwrap();
    writer.write_all(b"}").unwrap();
    writer.flush().unwrap();
    println!(
        "{}",
        json!({"mode":"DIAGNOSTIC_PROPOSED_DEFAULTS","diagnosticMatched":matched,"ownerApproval":"PENDING","parityEligible":false,"failure":failure})
    );
    assert!(
        matched,
        "diagnostic comparison or complete branch failed; raw report retained, no parity eligibility"
    );
}
