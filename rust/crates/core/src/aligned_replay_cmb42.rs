//! Private reader for the two ordinary tagged CMB42 scenes. Expected state is read only.
#[path = "aligned_replay_combat_effects.rs"]
mod combat_effects;

use super::observe;
use super::super::{Replay, Tape};
use crate::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    map::AuthoritativeMap,
    match_commands::Command,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    match_tick::TickEvent,
    orders::OrderKey,
    recipient_snapshot::SnapshotCache,
    weather::WeatherChoice,
};
use serde_json::{Value, json};
use std::{collections::{BTreeMap, BTreeSet}, sync::{Arc, Mutex}};

const PIN: &str = "e4e579a838aabae3b1d9dadbc17a0a87147bc52d";
const MODE: &str = "cmb42Ordinary";

fn number(value: f64) -> Value {
    json!({"$f64":format!("{:016x}",value.to_bits())})
}
fn fraction(value: &Value) -> Result<f64, String> {
    let text = value.as_str().ok_or("draw is not a bit string")?;
    if text.len() != 16 || !text.bytes().all(|v| v.is_ascii_hexdigit()) {
        return Err("draw is not sixteen hexadecimal digits".into());
    }
    let value = f64::from_bits(u64::from_str_radix(text, 16).map_err(|e| e.to_string())?);
    if !value.is_finite() || !(0.0..1.0).contains(&value) {
        return Err("draw is outside [0,1)".into());
    }
    Ok(value)
}
fn numeric(value: &Value) -> Result<f64, String> {
    let bits = value.get("$f64").and_then(Value::as_str).ok_or("tagged input number required")?;
    if value.as_object().is_none_or(|v| v.len() != 1) || bits.len() != 16 {
        return Err("malformed input number".into());
    }
    let value = f64::from_bits(u64::from_str_radix(bits, 16).map_err(|e| e.to_string())?);
    value.is_finite().then_some(value).ok_or("nonfinite command input".into())
}
// Decode command data only. State, recipients and probes never pass through this function.
fn command_json(value: &Value) -> Result<Value, String> {
    if value.get("$f64").is_some() {
        return Ok(json!(numeric(value)?));
    }
    match value {
        Value::Array(rows) => Ok(Value::Array(rows.iter().map(command_json).collect::<Result<_, _>>()?)),
        Value::Object(rows) => {
            if rows.keys().any(|key| key.starts_with('$')) {
                return Err("non-command tagged container in command input".into());
            }
            Ok(Value::Object(rows.iter().map(|(key, value)| Ok((key.clone(), command_json(value)?))).collect::<Result<_, String>>()?))
        }
        Value::Number(_) => Err("untagged command number".into()),
        _ => Ok(value.clone()),
    }
}
fn tagged(value: &Value) -> Value {
    match value {
        Value::Number(value) => number(value.as_f64().unwrap()),
        Value::Array(rows) => Value::Array(rows.iter().map(tagged).collect()),
        Value::Object(rows) => Value::Object(rows.iter().map(|(key, value)| (key.clone(), tagged(value))).collect()),
        _ => value.clone(),
    }
}
// Only changes the existing observer's numeric wrapper. Absent/native-only and
// unmapped representation markers remain explicit differences, never defaults.
fn observer_tags(value: &Value) -> Value {
    if let Some(bits) = value.get("bits").and_then(Value::as_str) {
        return json!({"$f64":bits});
    }
    match value {
        Value::Array(rows) => Value::Array(rows.iter().map(observer_tags).collect()),
        Value::Object(rows) => Value::Object(rows.iter().map(|(key, value)| (key.clone(), observer_tags(value))).collect()),
        _ => value.clone(),
    }
}
fn difference(source: &Value, native: &Value, path: &str) -> Option<Value> {
    if source == native { return None; }
    match (source, native) {
        (Value::Array(a), Value::Array(b)) if a.len() == b.len() => a.iter().zip(b).enumerate()
            .find_map(|(i, (a, b))| difference(a, b, &format!("{path}[{i}]"))),
        (Value::Object(a), Value::Object(b)) => {
            let keys = a.keys().chain(b.keys()).collect::<BTreeSet<_>>();
            keys.into_iter().find_map(|key| match (a.get(key), b.get(key)) {
                (Some(a), Some(b)) => difference(a, b, &format!("{path}.{key}")),
                (a, b) => Some(json!({"path":format!("{path}.{key}"),"source":a,"native":b,"propertyPresenceMismatch":true})),
            })
        }
        _ => Some(json!({"path":path,"source":source,"native":native})),
    }
}
impl Replay {
    fn ordinary_cmb42(case: &Value) -> Result<Self, String> {
        if case["random"]["algorithm"] != "splitmix64-global-source-input"
            || case["random"]["seed"] != case["sharedInput"]["seed"] {
            return Err("global input algorithm/seed differs from its declaration".into());
        }
        let rows = case["random"]["draws"].as_array().ok_or("missing captured global draws")?;
        // These passive scenes have only the constructor seed draw. Besides
        // enforcing that predicate, this capacity cannot be made by Replay::new.
        if rows.len() != 1 {
            return Err("passive ordinary scene must consume only its constructor global draw".into());
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
            input: json!({"layout":case["sharedInput"]["layout"],"cmb42Ordinary":{"environment":[],"commands":[],"bindings":{}}}),
            values, used:Vec::new(), layout:None, startup:None, shot:None,
        }))))
    }
}
pub(crate) fn is_ordinary(replay: &Replay) -> bool {
    let tape = replay.0.lock().unwrap();
    tape.values.len() == 1 && tape.input.get(MODE).is_some()
}
pub(crate) fn environment_observation(replay: &Replay, before: u32, after: u32, value: f64, consumer: Value) {
    let mut tape = replay.0.lock().unwrap();
    let rows = tape.input[MODE]["environment"].as_array_mut().unwrap();
    let index = rows.len();
    rows.push(json!({"index":index,"beforeU32":before,"afterU32":after,
        "value":format!("{:016x}",value.to_bits()),"consumer":consumer}));
}
pub(crate) fn command_observation(m: &Match, command: &Command) -> Result<(), String> {
    let Some(replay) = m.start.random.replay.as_ref().filter(|r| is_ordinary(r)) else { return Ok(()); };
    let bindings = serde_json::from_value(replay.0.lock().unwrap().input[MODE]["bindings"].clone())
        .map_err(|e|format!("actual binding metadata: {e}"))?;
    let frame = native_frame(m, &bindings)?;
    let random = random_observation(replay);
    replay.0.lock().unwrap().input[MODE]["commands"].as_array_mut().unwrap()
        .push(json!({"command":format!("{command:?}"),"frame":frame,"random":random}));
    Ok(())
}
fn native_frame(m: &Match, bindings: &BTreeMap<String, u32>) -> Result<Value, String> {
    let raw = observer_tags(&observe(m));
    let mut state = serde_json::Map::new();
    for key in ["tick","visionTick","players","points","seed","wind","wx","winner","shots","flights","nades","salvos","strikes"] {
        state.insert(key.into(), raw[key].clone());
    }
    state.insert("units".into(), raw["units"]["$map"].clone());
    state.insert("story".into(), Value::Array(m.story.iter().map(|story| {
        Value::Object(crate::match_story::StoryKey::ALL.into_iter()
            .map(|key| (key.as_str().into(), number(story.value(key)))).collect())
    }).collect()));
    state.insert("terrain".into(), json!({"chars":raw["chars"],"cellHp":raw["cellHp"],
        "cellState":raw["cellState"],"structuralCells":raw["structuralCells"]}));
    let cache = SnapshotCache::new(m).map_err(|e|format!("recipient cache: {e:?}"))?;
    let projections = (0..m.start.players.len()).map(|owner| cache.recipient(owner))
        .collect::<Result<Vec<_>, _>>().map_err(|e|format!("recipient: {e:?}"))?;
    let recipients = projections.iter().map(|r| tagged(&Value::Object(r.fields.clone()))).collect::<Vec<_>>();
    let missing = projections.iter().map(|r|format!("{:?}",r.missing)).collect::<Vec<_>>();
    let mut probes = Vec::new();
    if let Some(shooter) = bindings.get("sniper").or_else(|| bindings.get("at")) {
        for label in ["rifle","mg","tank"] {
            if let Some(target) = bindings.get(label) {
                let owner = m.world.game.units[shooter].owner;
                probes.push(json!({"shooter":number(f64::from(*shooter)),"target":number(f64::from(*target)),
                    "visible":m.world.game.players[owner].sees(*target),"actualCanShoot":m.can_shoot_target(*shooter,*target)?}));
            }
        }
    }
    Ok(json!({"state":state,"recipients":recipients,"recipientMissingCapabilities":missing,
        "probes":probes,"completeNativeObservation":raw}))
}
fn random_observation(replay: &Replay) -> Value {
    let global = replay.receipt();
    let tape = replay.0.lock().unwrap();
    json!({"global":global,"environment":tape.input[MODE]["environment"]})
}
fn random_receipt(observation: Value, case: &Value, count: usize, environment_count: usize) -> Value {
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
            || row["consumer"][0] != "startup.environment.seed32") {
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
        if first.is_none() && (!context_matches || ["index","beforeU32","afterU32","value"].iter().any(|key| source[*key] != row[*key])) {
            first = Some(json!({"kind":"actual-environment-transition","source":source,"native":row}));
        }
    }
    json!({"aligned":first.is_none(),"firstDifference":first,"global":native,"environment":environment,
        "sourceGlobal":case["random"]["draws"],"sourceEnvironment":case["random"]["environment"],
        "environmentReturn":"actual original generator, never substituted","globalSourceSeed":case["sharedInput"]["seed"]})
}
fn options(input: &Value) -> Result<MatchOptions, String> {
    if input["names"] != json!(["A","B"]) || input["teams"] != json!([0,1])
        || input["factions"] != json!([0,1]) || input["layout"] != json!([0,1]) || input["shuffle"] != false
        || input["seed"] != 701 || input["options"] != json!({"mode":"conquest","army":"standard","weather":false,
            "weatherSeed":1,"tech":true,"supply":true,"logistics":false,"sandbox":false}) {
        return Err("packet constructor inputs differ from the bounded scene".into());
    }
    Ok(MatchOptions { seed:701, mode:Mode::Conquest,
        players:vec![PlayerConfig{name:"A".into(),team:0,faction:Faction::Usa},
            PlayerConfig{name:"B".into(),team:1,faction:Faction::Germany}],
        shuffle:false, army:ArmySize::Standard, weather:WeatherChoice::Map, weather_seed:Some(1),
        living_weather:false, map_key:None, tech:true, supply:true, logistics:false, sandbox:false })
}

struct Driver {
    frames: Vec<Value>,
    first_failure: Option<Value>,
    current: Value,
}
impl Driver {
    fn compare(&mut self, source: &Value, native: Value, random: Value) -> Result<(), String> {
        let expected = json!({"state":source["state"],"recipients":source["recipients"],"probes":source["probes"]});
        let actual = json!({"state":native["state"],"recipients":native["recipients"],"probes":native["probes"]});
        let first = if source["tick"].as_f64() != Some(numeric(&native["state"]["tick"])?) {
            Some(json!({"path":"$.tick","source":source["tick"],"native":native["state"]["tick"]}))
        } else { difference(&expected, &actual, "$") };
        self.frames.push(json!({"scene":self.current["scene"],"index":source["index"],"event":source["event"],"phase":source["phase"],
            "tick":source["tick"],"source":source,"native":native,"random":random,"rawFirstDifference":first}));
        if first.is_some() || random["aligned"] != true {
            self.first_failure = Some(json!({"record":source["index"],"raw":first,"random":random["firstDifference"]}));
            return Err("first raw state/input difference; no repair or retry".into());
        }
        Ok(())
    }
    fn scene(&mut self, case: &Value) -> Result<(), String> {
        let input = &case["sharedInput"];
        let map = AuthoritativeMap::from_json(&serde_json::to_vec(&input["map"]).map_err(|e| e.to_string())?).map_err(|e|e.to_string())?;
        let replay = Replay::ordinary_cmb42(case)?;
        let mut m = Match::new_aligned(&map, options(input)?, replay.clone())?;
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
        let mut funded = BTreeSet::new();
        let mut cooled = BTreeSet::new();
        for (index, record) in records.iter().enumerate() {
            self.current = json!({"scene":case["name"],"record":index,"source":record,
                "nativeBefore":observer_tags(&observe(&m)),"randomBefore":random_observation(&replay)});
            if record["index"].as_u64() != Some(index as u64) { return Err("record index gap".into()); }
            if !matches!(record["event"].as_str(),Some("constructor"|"declared-input"|"command-enqueued"
                |"command-consumed"|"purchase-provider"|"tick-consumed"|"recipient-control"|"scene-complete")) {
                return Err("unknown packet observation event".into());
            }
            let mut queued_frames = Vec::new();
            if inputs.get(cursor).is_some_and(|v| v["record"].as_u64() == Some(index as u64)) {
                let action = &inputs[cursor];
                if action["index"].as_u64() != Some(cursor as u64) { return Err("input index gap".into()); }
                cursor += 1;
                match action["event"].as_str().ok_or("input event required")? {
                    "initial-funding" => {
                        let owner = action["owner"].as_u64().ok_or("funding owner required")? as usize;
                        if owner >= 2 || !funded.insert(owner) || numeric(&action["value"])? != 5000.0 || m.world.game.tick != 0 {
                            return Err("only one declared initial funding input per seat".into());
                        }
                        m.start.economy.players[owner].mp = numeric(&action["value"])?;
                    }
                    "initial-cooldown" => {
                        let label = action["label"].as_str().ok_or("shooter label required")?;
                        let id = *bindings.get(label).ok_or("shooter must be an observed real birth")?;
                        if !matches!(label,"at"|"sniper") || !cooled.insert(id) || numeric(&action["value"])? != 1e9 {
                            return Err("only the original one-time shooter cooldown input is permitted".into());
                        }
                        m.firing.get_mut(id).ok_or("missing actual firing row")?.cooldown = numeric(&action["value"])?;
                    }
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
                            || report.commands.iter().any(|v|v.denial.is_some()) || !report.retired.is_empty()
                            || report.events.iter().any(|e|matches!(e,TickEvent::Firing(_))) {
                            return Err("actual passive tick/command invariant failed".into());
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
                        pending.clear();
                    }
                    _ => return Err("unknown declared input".into()),
                }
            }
            // A step consumes its queued command records first, while their native
            // frames were saved inside that same actual advance_tick call.
            if !queued_frames.is_empty() {
                for (offset, saved) in queued_frames.iter().enumerate() {
                    let source = records.get(index+offset).ok_or("missing queued source receipt")?;
                    if source["event"] != "command-consumed" { return Err("expected actual queued dispatch receipt".into()); }
                    let count = source["random"]["count"].as_u64().ok_or("global count missing")? as usize;
                    let environment_count = source["random"]["environmentCount"].as_u64().ok_or("environment count missing")? as usize;
                    let random = random_receipt(saved["random"].clone(),case,count,environment_count);
                    self.compare(source,saved["frame"].clone(),random)?;
                }
            }
            if record["event"] == "command-consumed" && !record["detail"]["sequence"].is_null() {
                // Queued records were observed in advance_tick, never re-executed.
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
            let count = record["random"]["count"].as_u64().ok_or("global count missing")? as usize;
            let environment_count = record["random"]["environmentCount"].as_u64().ok_or("environment count missing")? as usize;
            let state = if count == 0 { format!("{:016x}",input["seed"].as_u64().unwrap()) }
                else { case["random"]["draws"][count-1]["after"].as_str().ok_or("global prefix exceeds captured input")?.into() };
            if record["random"]["state"] != state { return Err("observation global state is not its consumed prefix".into()); }
            self.compare(record,native_frame(&m,&bindings)?,random_receipt(random_observation(&replay),case,count,environment_count))?;
        }
        if cursor != inputs.len() || !pending.is_empty() || m.commands.pending_commands() != 0
            || funded.len() != 2 || cooled.len() != 1 || records.last().is_none_or(|r|r["event"]!="scene-complete") {
            return Err("incomplete scene input or command consumption".into());
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
fn ordinary_cmb42_packet_matches_actual_native_boundaries() {
    let packet = std::env::var_os("WW2_CMB42_PACKET").expect("private captured scenes.json required");
    let output = std::env::var_os("WW2_CMB42_NATIVE_RECEIPT").expect("new private receipt path required");
    let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(output).unwrap();
    let mut driver = Driver { frames:Vec::new(), first_failure:None, current:Value::Null };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<(),String> {
        let trace: Value = serde_json::from_slice(&std::fs::read(packet).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
        if trace["format"] != "cmb42-ordinary-tagged-v1" || trace["reference"] != PIN {
            return Err("packet format/reference mismatch".into());
        }
        let cases = trace["cases"].as_array().ok_or("missing two scenes")?;
        if cases.len() != 2 || cases[0]["name"] != "ordinary-sniper-12-30-distance-and-recipient"
            || cases[1]["name"] != "ordinary-at-7-28-researched-built-producers" {
            return Err("packet scene selection changed".into());
        }
        for case in cases { driver.scene(case)?; }
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
    serde_json::to_writer(&mut file,&json!({"format":"cmb42-native-receipt-v1","frames":driver.frames,
        "firstFailure":driver.first_failure,"error":error,"comparisonDefaultsApplied":[],"complete":error.is_none()})).unwrap();
    file.write_all(b"\n").unwrap();
    assert!(error.is_none(),"{}",error.unwrap_or_default());
}
