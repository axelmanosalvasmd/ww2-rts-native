//! Two genuine command histories. The old component candidate remains separate.
use super::*;

fn verify_choice(m: &Match, bindings: &BTreeMap<String,u32>, record: &Value,
    counts: &mut BTreeMap<String,usize>) -> Result<(),String> {
    let phase = record["phase"].as_str().ok_or("missing scene phase")?;
    let (shooter, first, second, expected, distances) = match phase {
        "sniper-select-mg" => ("sniper","rifle","mg","mg",[12.0,30.0]),
        "sniper-select-rifle" => ("sniper","rifle","mg","rifle",[12.0,40.0]),
        "at-select-tank" => ("at","rifle","tank","tank",[7.0,28.0]),
        _ => return Ok(()),
    };
    let id = |label| bindings.get(label).copied().ok_or_else(|| format!("missing real birth: {label}"));
    let shooter = id(shooter)?;
    let expected = id(expected)?;
    let u = &m.world.game.units[&shooter];
    for (label,distance) in [first,second].into_iter().zip(distances) {
        let target = id(label)?;
        let t = &m.world.game.units[&target];
        if t.hp <= 0.0 || !m.world.game.players[0].sees(target) || !m.can_shoot_target(shooter,target)?
            || (crate::math::hypot(t.x-u.x,t.z-u.z)-distance).abs() > 1e-6 {
            return Err(format!("genuine literal geometry/visibility/survival control: {phase}"));
        }
    }
    if m.commands.units[&shooter].attack_id != 0
        || m.firing.get(shooter).ok_or("missing real firing row")?.target_id != expected {
        return Err(format!("genuine automatic target choice: {phase}"));
    }
    if phase == "at-select-tank" {
        let tank = &m.world.game.units[&id("tank")?];
        if u.still.is_none_or(|v| v<2.0)
            || crate::math::cos(crate::math::atan2(u.z-tank.z,u.x-tank.x)-tank.rot) <= 0.5 {
            return Err("ordinary AT setup and tank front control failed".into());
        }
    }
    let view = SnapshotCache::new(m).map_err(|e| format!("cache: {e:?}"))?
        .recipient(0).map_err(|e| format!("recipient: {e:?}"))?;
    let row = view.fields["units"].as_array().ok_or("recipient units")?.iter()
        .find(|row| row[0].as_u64() == Some(u64::from(shooter))).ok_or("missing real owner row")?;
    if row[9].as_u64() != Some(u64::from(expected)) { return Err("actual owner recipient target differs".into()); }
    *counts.entry(phase.into()).or_default() += 1;
    Ok(())
}

// Observation-only references expand before strict comparison. No observed
// value passes to Match construction, commands, runtime fields or random input.
fn expand(value: &Value, pool: &[Value], depth: usize) -> Result<Value,String> {
    if depth > 64 { return Err("observation reference nesting exceeds bound".into()); }
    if let Some(index) = value.get("$observation") {
        if value.as_object().is_none_or(|v|v.len()!=1) { return Err("malformed observation reference".into()); }
        let index = index.as_u64().ok_or("observation index")? as usize;
        return expand(pool.get(index).ok_or("observation reference outside pool")?,pool,depth+1);
    }
    match value {
        Value::Array(rows) => Ok(Value::Array(rows.iter().map(|v|expand(v,pool,depth+1)).collect::<Result<_,_>>()?)),
        Value::Object(rows) => Ok(Value::Object(rows.iter().map(|(k,v)|Ok((k.clone(),expand(v,pool,depth+1)?)))
            .collect::<Result<_,String>>()?)),
        _ => Ok(value.clone()),
    }
}
struct GenuineDriver {
    frames: Vec<Value>,
    first_failure: Option<Value>,
    current: Value,
    observations: Vec<Value>,
    observation_ids: BTreeMap<String,usize>,
}
impl GenuineDriver {
    fn compact(&mut self,value: &Value) -> Result<Value,String> {
        match value {
            Value::Array(rows) => {
                let packed = Value::Array(rows.iter().map(|v|self.compact(v)).collect::<Result<_,_>>()?);
                if rows.len()<64 { return Ok(packed); }
                let key = serde_json::to_string(&packed).map_err(|e|e.to_string())?;
                let index = if let Some(index) = self.observation_ids.get(&key) { *index } else {
                    let index=self.observations.len();
                    self.observations.push(packed);
                    self.observation_ids.insert(key,index);
                    index
                };
                Ok(json!({"$observation":index}))
            }
            Value::Object(rows) => Ok(Value::Object(rows.iter().map(|(k,v)|Ok((k.clone(),self.compact(v)?)))
                .collect::<Result<_,String>>()?)),
            _ => Ok(value.clone()),
        }
    }
    fn compare(&mut self, source: &Value, native: Value, random: Value) -> Result<(), String> {
        let expected = json!({"state":source["state"],"recipients":source["recipients"],"probes":source["probes"]});
        let actual = json!({"state":native["state"],"recipients":native["recipients"],"probes":native["probes"]});
        let first = if source["tick"].as_f64() != Some(numeric(&native["state"]["tick"])?) {
            Some(json!({"path":"$.tick","source":source["tick"],"native":native["state"]["tick"]}))
        } else { difference(&expected, &actual, "$") };
        let frame = json!({"scene":self.current["scene"],"index":source["index"],"event":source["event"],"phase":source["phase"],
            "tick":source["tick"],"source":source,"native":native,"random":random,"rawFirstDifference":first});
        let packed = self.compact(&frame)?;
        self.frames.push(packed);
        if first.is_some() || random["aligned"] != true {
            self.first_failure = Some(json!({"record":source["index"],"raw":first,"random":random["firstDifference"]}));
            return Err("first raw state/input difference; no repair or retry".into());
        }
        Ok(())
    }
}

impl Replay {
    fn genuine_cmb42(case: &Value) -> Result<Self, String> {
        if case["random"]["algorithm"] != "splitmix64-global-source-input"
            || case["random"]["seed"] != case["sharedInput"]["seed"] {
            return Err("global input algorithm/seed differs from its declaration".into());
        }
        let rows = case["random"]["draws"].as_array().ok_or("missing captured global draws")?;
        // An explicit successor marker and non-default capacity keep the
        // unchanged Replay::new schema out of this private captured-input mode.
        if rows.is_empty() || rows.len() == 16384 {
            return Err("genuine capture must have a nonempty non-default capacity".into());
        }
        let mut state = case["sharedInput"]["seed"].as_u64().ok_or("missing global seed")?;
        let mut values = Vec::new();
        for (index, row) in rows.iter().enumerate() {
            if row["index"].as_u64() != Some(index as u64)
                || row["before"] != format!("{state:016x}") {
                return Err("global tape index/state is discontinuous".into());
            }
            let mut generator = crate::random::SplitMix64::new(state);
            let expected = (generator.next_u64() >> 11) as f64 / 9_007_199_254_740_992.0;
            state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
            let actual = fraction(&row["value"])?;
            if actual.to_bits() != expected.to_bits() || row["after"] != format!("{state:016x}") {
                return Err("captured global draw disagrees with declared SplitMix64 input".into());
            }
            values.push(actual);
        }
        if case["random"]["finalState"] != format!("{state:016x}") {
            return Err("global final state disagrees with continuous input".into());
        }
        Ok(Self(Arc::new(Mutex::new(Tape {
            input: json!({"layout":case["sharedInput"]["layout"],"cmb42Ordinary":{"environment":[],"commands":[],"bindings":{},"genuineInputV2":true}}),
            values, used:Vec::new(), layout:None, startup:None, shot:None,
        }))))
    }
}
fn prefix_counts(record: &Value, case: &Value) -> Result<(usize,usize),String> {
    let count = record["random"]["count"].as_u64().ok_or("global count missing")? as usize;
    let environment_count = record["random"]["environmentCount"].as_u64().ok_or("environment count missing")? as usize;
    let draws = case["random"]["draws"].as_array().ok_or("missing global draws")?;
    let environment = case["random"]["environment"].as_array().ok_or("missing environment draws")?;
    if count > draws.len() || environment_count > environment.len() {
        return Err("observation consumed prefix exceeds captured input".into());
    }
    let state = if count == 0 { format!("{:016x}",case["sharedInput"]["seed"].as_u64().ok_or("global seed")?) }
        else { draws[count-1]["after"].as_str().ok_or("global prefix state missing")?.into() };
    if record["random"]["state"] != state {
        return Err("observation global state is not its consumed prefix".into());
    }
    Ok((count,environment_count))
}
fn genuine_random_receipt(observation: Value, case: &Value, count: usize, environment_count: usize) -> Value {
    let native = &observation["global"];
    let environment = &observation["environment"];
    let global = &native["draws"];
    let mut first = None;
    if global.as_array().unwrap().len() != count || environment.as_array().unwrap().len() != environment_count {
        first = Some(json!({"kind":"count","sourceGlobal":count,"nativeGlobal":global.as_array().unwrap().len(),
            "sourceEnvironment":environment_count,"nativeEnvironment":environment.as_array().unwrap().len()}));
    }
    for (index, row) in global.as_array().unwrap().iter().enumerate() {
        let source = &case["random"]["draws"][index];
        if first.is_none() && (source["index"] != row["index"] || source["value"] != row["bits"]
            || source["consumer"] != row["consumer"]) {
            first = Some(json!({"kind":"global-consumer/index/bits","source":source,"native":row}));
        }
    }
    for (index, row) in environment.as_array().unwrap().iter().enumerate() {
        let source = &case["random"]["environment"][index];
        let role = row["consumer"][0].as_str().unwrap_or("");
        let context_matches = if index < 2 {
            source["phase"] == "constructor" && source["tick"] == 0
                && role == if index == 0 { "startup.wind.angle" } else { "startup.wind.strength" }
        } else {
            matches!(role,"weather.wind.angle"|"weather.wind.strength")
                && source["phase"].as_str().is_some_and(|v|v.starts_with("step:"))
                && source["tick"] == row["consumer"][1]
        };
        let source_seeds_match = ["before","after"].into_iter().all(|key| {
            source[key].as_i64().is_some_and(|v| source[format!("{key}U32")].as_u64() == Some(u64::from(v as u32)))
        });
        if first.is_none() && (!context_matches || !source_seeds_match
            || ["consumer","index","beforeU32","afterU32","value"].iter().any(|key| source[*key] != row[*key])) {
            first = Some(json!({"kind":"actual-environment-transition","source":source,"native":row}));
        }
    }
    json!({"aligned":first.is_none(),"firstDifference":first,"global":native,"environment":environment,
        "sourceGlobal":case["random"]["draws"],"sourceEnvironment":case["random"]["environment"],
        "environmentReturn":"actual original generator, never substituted","globalSourceSeed":case["sharedInput"]["seed"]})
}
impl GenuineDriver {
    fn genuine_scene(&mut self, case: &Value, pool: &[Value]) -> Result<(), String> {
        let input = &case["sharedInput"];
        let map = AuthoritativeMap::from_json(&serde_json::to_vec(&input["map"]).map_err(|e| e.to_string())?).map_err(|e|e.to_string())?;
        let replay = Replay::genuine_cmb42(case)?;
        let mut m = Match::new_aligned(&map, options(input)?, replay.clone())?;
        if m.start.economy.players.len() != 2 || m.start.economy.players.iter().any(|p| p.mp != 150.0)
            || m.world.game.units.len() != 10 {
            return Err("ordinary constructor wallet/army differs from the bounded inputs".into());
        }
        let initial_receipt = replay.receipt();
        if initial_receipt["layout"]["selectedBeforeInjection"] != input["layout"]
            || case["observedLayout"] != input["layout"] {
            return Err("computed layouts differ; declared layout cannot hide an injection".into());
        }
        let records = case["records"].as_array().ok_or("missing raw records")?;
        let inputs = input["inputs"].as_array().ok_or("missing declared input list")?;
        let mut cursor = 0;
        let mut bindings = BTreeMap::new();
        let mut pending = Vec::<Value>::new();
        let mut births = Vec::new();
        let mut choice_counts = BTreeMap::<String, usize>::new();
        let mut recipient_controls = 0;
        let mut verified_queued = BTreeSet::new();
        for (index, packed) in records.iter().enumerate() {
            let expanded = expand(packed,pool,0)?;
            let record = &expanded;
            self.current = json!({"scene":case["name"],"record":index,"source":record,
                "nativeBefore":observer_tags(&observe(&m)),"randomBefore":random_observation(&replay)});
            if record["index"].as_u64() != Some(index as u64) { return Err("record index gap".into()); }
            if !matches!(record["event"].as_str(),Some("constructor"|"command-enqueued"
                |"command-consumed"|"purchase-provider"|"tick-consumed"|"recipient-control"|"scene-complete")) {
                return Err("unknown packet observation event".into());
            }
            let mut queued_frames = Vec::new();
            let mut consumed_commands = Vec::new();
            if inputs.get(cursor).is_some_and(|v| v["record"].as_u64() == Some(index as u64)) {
                let action = &inputs[cursor];
                if action["index"].as_u64() != Some(cursor as u64) { return Err("input index gap".into()); }
                cursor += 1;
                match action["event"].as_str().ok_or("input event required")? {
                    "queued-command" => {
                        let key = OrderKey { tick:action["tick"].as_u64().ok_or("queued tick required")?,
                            issuer:u32::try_from(action["owner"].as_u64().ok_or("queued owner required")?).map_err(|e|e.to_string())?,
                            sequence:action["sequence"].as_u64().ok_or("queued sequence required")? };
                        let command = Command::from_json(&command_json(&action["command"])?)
                            .map_err(|e|format!("command parse: {e:?}"))?;
                        m.enqueue_command(key,command).map_err(|e|format!("queue: {e:?}"))?;
                        pending.push(action.clone());
                    }
                    "immediate-command" => {
                        let owner = action["owner"].as_u64().ok_or("command owner required")? as usize;
                        let command = Command::from_json(&command_json(&action["command"])?)
                            .map_err(|e|format!("command parse: {e:?}"))?;
                        let before = m.world.game.units.keys().copied().collect::<BTreeSet<_>>();
                        let result = m.execute_command(owner,&command,false)?;
                        if result.is_some() { return Err(format!("immediate command denial: {result:?}")); }
                        births = m.world.game.units.keys().filter(|id| !before.contains(id)).copied().collect();
                    }
                    "step" => {
                        let before = m.world.game.tick;
                        let tick = action["tick"].as_i64().ok_or("step clock required")?;
                        if tick != before + 1 { return Err("non-contiguous step input".into()); }
                        let report = m.advance_tick().map_err(|e| {
                            self.current["nativeAfterFailure"] = observer_tags(&observe(&m));
                            self.current["randomAfterFailure"] = random_observation(&replay);
                            self.current["tickFailure"] = json!(format!("{e:?}"));
                            e.to_string()
                        })?;
                        if report.tick != tick || report.commands.len() != pending.len()
                            || report.commands.iter().any(|v|v.denial.is_some()) || !report.retired.is_empty() {
                            return Err("actual tick/command invariant failed".into());
                        }
                        pending.sort_by_key(|v|(v["owner"].as_u64(),v["sequence"].as_u64()));
                        for (command, result) in pending.iter().zip(&report.commands) {
                            if command["owner"].as_u64() != Some(u64::from(result.key.issuer))
                                || command["sequence"].as_u64() != Some(result.key.sequence)
                                || command["tick"].as_u64() != Some(result.key.tick) {
                                return Err("native command receipt changed owner/sequence/tick".into());
                            }
                        }
                        let mut tape = replay.0.lock().unwrap();
                        queued_frames = std::mem::take(tape.input[MODE]["commands"].as_array_mut().unwrap());
                        if queued_frames.len() != pending.len() { return Err("missing actual command-boundary snapshots".into()); }
                        consumed_commands = std::mem::take(&mut pending);
                    }
                    _ => return Err("unknown declared input".into()),
                }
            }
            // A step consumes its queued command records first, while their native
            // frames were saved inside that same actual advance_tick call.
            if !queued_frames.is_empty() {
                for (offset, saved) in queued_frames.iter().enumerate() {
                    let source = expand(records.get(index+offset).ok_or("missing queued source receipt")?,pool,0)?;
                    if source["event"] != "command-consumed" { return Err("expected actual queued dispatch receipt".into()); }
                    let command = &consumed_commands[offset];
                    let detail = &source["detail"];
                    let expected_command = command_json(&command["command"])?;
                    let parsed = Command::from_json(&expected_command).map_err(|e|format!("queued identity: {e:?}"))?;
                    if Some(numeric(&detail["owner"])?) != command["owner"].as_f64()
                        || Some(numeric(&detail["sequence"])?) != command["sequence"].as_f64()
                        || command_json(&detail["command"])? != expected_command
                        || source["tick"].as_i64() != command["tick"].as_i64().map(|v|v-1)
                        || saved["command"] != format!("{parsed:?}")
                        || !verified_queued.insert(index+offset) {
                        return Err("actual queued snapshot command/key/clock identity differs".into());
                    }
                    let (count,environment_count) = prefix_counts(&source,case)?;
                    let random = genuine_random_receipt(saved["random"].clone(),case,count,environment_count);
                    self.compare(&source,saved["frame"].clone(),random)?;
                }
            }
            if record["event"] == "command-consumed" && !record["detail"]["sequence"].is_null() {
                // Queued records were observed in advance_tick, never re-executed.
                if !verified_queued.remove(&index) { return Err("unbound queued command snapshot".into()); }
                continue;
            }
            if record["event"] == "purchase-provider" {
                let detail = command_json(&record["detail"])?;
                let label = detail["label"].as_str().ok_or("purchase label missing")?;
                if births.len() != 1 { return Err("purchase did not publish one real actor".into()); }
                let id = births[0];
                let producer = m.recruit_metadata.get(&id).ok_or("missing actual purchase metadata")?.building;
                let actor = &m.world.game.units[&id];
                if detail["id"].as_f64() != Some(f64::from(id))
                    || detail["producer"].as_f64() != Some(f64::from(producer))
                    || detail["kind"].as_str() != Some(m.rules.kinds[actor.kind].name.as_str())
                    || detail["owner"].as_f64() != Some(actor.owner as f64) {
                    return Err("actual purchase provider/birth differs".into());
                }
                bindings.insert(label.into(),id);
                replay.0.lock().unwrap().input[MODE]["bindings"] = json!(bindings);
            }
            if record["event"] == "tick-consumed" {
                verify_choice(&m, &bindings, record, &mut choice_counts)?;
            }
            if record["event"] == "recipient-control" {
                let shooter = *bindings.get("sniper").ok_or("missing real sniper")?;
                let mg = *bindings.get("mg").ok_or("missing real MG")?;
                if m.world.game.players[0].sees(mg) || m.can_shoot_target(shooter, mg)?
                    || m.firing.get(shooter).ok_or("missing firing actor")?.target_id != 0 {
                    return Err("hidden-MG target/visibility control failed".into());
                }
                let view = SnapshotCache::new(&m).map_err(|e| format!("cache: {e:?}"))?
                    .recipient(0).map_err(|e| format!("recipient: {e:?}"))?;
                let rows = view.fields["units"].as_array().ok_or("recipient units")?;
                let shooter_row = rows.iter().find(|row|row[0].as_u64() == Some(u64::from(shooter)))
                    .ok_or("hidden control missing actual owner shooter row")?;
                if shooter_row[9].as_u64() != Some(0) {
                    return Err("hidden control actual owner target is not zero".into());
                }
                if rows.iter().any(|row| row[0].as_u64() == Some(u64::from(mg))) {
                    return Err("actual owner recipient leaks hidden MG".into());
                }
                recipient_controls += 1;
            }
            let (count,environment_count) = prefix_counts(record,case)?;
            self.compare(record,native_frame(&m,&bindings)?,genuine_random_receipt(random_observation(&replay),case,count,environment_count))?;
        }
        if cursor != inputs.len() || !pending.is_empty() || !verified_queued.is_empty() || m.commands.pending_commands() != 0
            || records.last().is_none_or(|r|r["event"]!="scene-complete") {
            return Err("incomplete scene input or command consumption".into());
        }
        let expected_counts = if case["name"] == "genuine-sniper-12-30-distance-and-recipient" {
            BTreeMap::from([("sniper-select-mg".into(),20), ("sniper-select-rifle".into(),20)])
        } else { BTreeMap::from([("at-select-tank".into(),20)]) };
        if choice_counts != expected_counts || recipient_controls != usize::from(bindings.contains_key("sniper")) {
            return Err("complete named choice windows or recipient control missing".into());
        }
        let receipt = random_observation(&replay);
        if receipt["global"]["unconsumed"] != 0
            || receipt["environment"].as_array().unwrap().len() != case["random"]["environment"].as_array().ok_or("missing Source environment receipt")?.len() {
            return Err("scene left an unconsumed captured random suffix".into());
        }
        Ok(())
    }
}
#[test]
#[ignore = "requires a new frozen ordinary CMB42 packet and a separate Root exact-head grant"]
fn genuine_cmb42_packet_matches_actual_native_boundaries() {
    let packet = std::env::var_os("WW2_CMB42_PACKET").expect("private captured scenes.json required");
    let output = std::env::var_os("WW2_CMB42_NATIVE_RECEIPT").expect("new private receipt path required");
    let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(output).unwrap();
    let mut driver = GenuineDriver { frames:Vec::new(), first_failure:None, current:Value::Null,
        observations:Vec::new(), observation_ids:BTreeMap::new() };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<(),String> {
        let trace: Value = serde_json::from_slice(&std::fs::read(packet).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
        if trace["format"] != "cmb42-genuine-tagged-v2" || trace["reference"] != PIN {
            return Err("packet format/reference mismatch".into());
        }
        let cases = trace["cases"].as_array().ok_or("missing two scenes")?;
        if cases.len() != 2 || cases[0]["name"] != "genuine-sniper-12-30-distance-and-recipient"
            || cases[1]["name"] != "genuine-at-7-28-researched-built-producers" {
            return Err("packet scene selection changed".into());
        }
        let pool = trace["observations"].as_array().ok_or("missing lossless observation pool")?;
        for case in cases { driver.genuine_scene(case,pool)?; }
        Ok(())
    }));
    let error = match result {
        Ok(Ok(())) => None,
        Ok(Err(error)) => Some(error),
        Err(payload) => Some(payload.downcast_ref::<String>().cloned()
            .or_else(||payload.downcast_ref::<&str>().map(|v|v.to_string())).unwrap_or("native panic".into())),
    };
    if error.is_some() && driver.first_failure.is_none() {
        driver.first_failure = Some(json!({"error":error,"context":driver.current}));
    }
    use std::io::Write;
    serde_json::to_writer(&mut file,&json!({"format":"cmb42-genuine-native-receipt-v2","frames":driver.frames,"observations":driver.observations,
        "firstFailure":driver.first_failure,"error":error,"comparisonDefaultsApplied":[],"complete":error.is_none()})).unwrap();
    file.write_all(b"\n").unwrap();
    assert!(error.is_none(),"{}",error.unwrap_or_default());
}
