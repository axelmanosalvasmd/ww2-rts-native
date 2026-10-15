//! Read-only validation of authentic command/tick draw-prefix evidence.
use super::*;

pub(super) fn validate(prefixes: &[Value], commands: &[Value], consumed: usize, terminal_tick: i64) -> Result<(), String> {
    if prefixes.is_empty() { return Err("missing startup prefix".into()); }
    if terminal_tick < 0 { return Err("invalid completion tick".into()); }
    let mut tick = 0;
    let mut draws = 0;
    let mut command_index = 0;
    for (index,row) in prefixes.iter().enumerate() {
        if !row.as_object().is_some_and(|fields| fields.len()==3
            && ["stage","tick","drawCount"].iter().all(|key| fields.contains_key(*key)))
        { return Err("invalid prefix field set".into()); }
        let current_tick = row["tick"].as_i64().filter(|tick| *tick>=0).ok_or("invalid prefix tick")?;
        let current_draws = row["drawCount"].as_u64().ok_or("invalid prefix draw count")?;
        if current_draws < draws || current_draws > consumed as u64 {
            return Err("nonmonotone or excess prefix draw count".into());
        }
        match row["stage"].as_str() {
            Some("startup") if index==0 && current_tick==0 => {},
            Some("command") if index>0 && current_tick==tick => {
                let command = commands.get(command_index).ok_or("missing bound command receipt")?;
                if command["tick"].as_i64()!=Some(tick) { return Err("command prefix tick mismatch".into()); }
                command_index+=1;
            }
            Some("tick") if index>0 && tick.checked_add(1)==Some(current_tick) => {},
            _ => return Err("invalid startup/command/tick prefix order".into()),
        }
        tick=current_tick;
        draws=current_draws;
    }
    if command_index!=commands.len() { return Err("extra command receipt".into()); }
    if tick!=terminal_tick { return Err("prefix does not reach completion tick".into()); }
    if draws!=consumed as u64 { return Err("final prefix omits consumed draws".into()); }
    Ok(())
}

pub(super) fn counterfactuals(prefixes: &[Value],commands: &[Value],consumed: usize,terminal_tick: i64) -> Vec<&'static str> {
    validate(prefixes,commands,consumed,terminal_tick).expect("authentic prefix evidence must validate before controls");
    let command = prefixes.iter().position(|row| row["stage"]=="command").expect("real command prefix");
    let tick = prefixes.iter().position(|row| row["stage"]=="tick").expect("real ordinary tick prefix");
    let rollback = (1..prefixes.len()).find(|index|prefixes[index-1]["drawCount"].as_u64().is_some_and(|count|count>0))
        .expect("genuine consumed draw before a later boundary");
    assert!(consumed>0);
    let mut checked = Vec::new();
    let mut reject = |label,p:Vec<Value>,c:Vec<Value>,count| {
        assert!(validate(&p,&c,count,terminal_tick).is_err(),"prefix control must be decisive: {label}");
        checked.push(label);
    };
    let mut p=prefixes.to_vec();p.remove(0);reject("missing startup",p,commands.to_vec(),consumed);
    let mut p=prefixes.to_vec();p.insert(1,p[0].clone());reject("repeated startup",p,commands.to_vec(),consumed);
    let mut p=prefixes.to_vec();p[0]["tick"]=json!(1);reject("startup tick",p,commands.to_vec(),consumed);
    let mut p=prefixes.to_vec();p[tick]["tick"]=json!(-1);reject("negative tick",p,commands.to_vec(),consumed);
    let mut p=prefixes.to_vec();p[tick]["tick"]=json!(2);reject("missing ordinary tick",p,commands.to_vec(),consumed);
    let mut p=prefixes.to_vec();p[command]["tick"]=json!(1);reject("command advanced clock",p,commands.to_vec(),consumed);
    let mut p=prefixes.to_vec();p[command]["drawCount"]=Value::Null;reject("missing draw count",p,commands.to_vec(),consumed);
    let mut p=prefixes.to_vec();p[command]["drawCount"]=json!(-1);reject("negative draw count",p,commands.to_vec(),consumed);
    let mut p=prefixes.to_vec();p[rollback]["drawCount"]=json!(0);reject("draw count rollback",p,commands.to_vec(),consumed);
    let mut p=prefixes.to_vec();p[tick]["stage"]=json!("milestone");reject("wrong prefix stage",p,commands.to_vec(),consumed);
    let mut p=prefixes.to_vec();p[command]["drawCount"]=json!(consumed+1);reject("excess draw count",p,commands.to_vec(),consumed);
    let mut p=prefixes.to_vec();let last=p.len()-1;p[last]["drawCount"]=json!(consumed-1);reject("missing final consumed draw",p,commands.to_vec(),consumed);
    let mut c=commands.to_vec();c.remove(0);reject("missing command receipt",prefixes.to_vec(),c,consumed);
    let mut c=commands.to_vec();c[0]["tick"]=json!(1);reject("wrong command tick",prefixes.to_vec(),c,consumed);
    let mut c=commands.to_vec();c.push(commands[0].clone());reject("extra command receipt",prefixes.to_vec(),c,consumed);
    let mut p=prefixes.to_vec();p[tick]["tick"]=json!("1");reject("string tick",p,commands.to_vec(),consumed);
    let mut p=prefixes.to_vec();p[command]["drawCount"]=json!(true);reject("boolean draw count",p,commands.to_vec(),consumed);
    let mut p=prefixes.to_vec();p[command]["drawCount"]=json!({"absent":true});reject("absent draw count marker",p,commands.to_vec(),consumed);
    let mut p=prefixes.to_vec();p[command]["extra"]=json!(0);reject("extra prefix field",p,commands.to_vec(),consumed);
    let mut p=prefixes.to_vec();p[command].as_object_mut().unwrap().remove("tick");reject("missing prefix tick",p,commands.to_vec(),consumed);
    assert!(validate(prefixes,commands,consumed,terminal_tick.checked_add(1).unwrap()).is_err(),"completion tick must bind the last ordinary boundary");
    checked.push("completion beyond recorded boundary");
    assert!(validate(prefixes,commands,consumed,-1).is_err());
    checked.push("negative completion tick");
    checked
}

pub(super) fn command(raw: &Value, encoded: &Value) -> Option<Value> {
    original::compare_projection_for_test(&exact_command_numbers(raw),encoded)
}

fn change_number_bits(value: &mut Value) -> bool {
    match value {
        Value::Array(rows) => rows.iter_mut().any(change_number_bits),
        Value::Object(fields) => {
            if let Some(bits)=fields.get("bits").and_then(Value::as_str) {
                let changed=u64::from_str_radix(bits,16).unwrap()^1;
                fields.insert("bits".into(),json!(format!("{changed:016x}")));
                true
            } else { fields.values_mut().any(change_number_bits) }
        }
        _ => false,
    }
}

pub(super) fn command_counterfactuals(raw: &Value, encoded: &Value) -> usize {
    assert_eq!(command(raw,encoded),None,"authentic raw/encoded command must agree before controls");
    let mut changed=encoded.clone();changed["__extra_receipt_field"]=json!(true);
    assert!(command(raw,&changed).is_some(),"extra encoded command field must fail");
    let mut changed=encoded.clone();changed.as_object_mut().unwrap().remove("t");
    assert!(command(raw,&changed).is_some(),"missing encoded command kind must fail");
    let mut changed=encoded.clone();changed["t"]=json!(false);
    assert!(command(raw,&changed).is_some(),"present command type mismatch must fail");
    let mut changed=encoded.clone();
    if change_number_bits(&mut changed) {
        assert!(command(raw,&changed).is_some(),"one changed encoded numeric bit must fail");
        4
    } else { 3 }
}

// Validate actual Source input evidence before constructing a native Match.
// These checks read receipts only. They never supply state or draws to Match.
pub(super) fn input_receipt(
    receipt: &Value,
    input: &Value,
    terminal_draws: u64,
) -> Result<(), String> {
    let tape = input["tape"].as_array().ok_or("missing authored tape")?;
    if tape.len() != 16384 || receipt["capacity"].as_u64() != Some(16384) {
        return Err("invalid Source input capacity".into());
    }
    let draws = receipt["draws"].as_array().ok_or("missing Source draw rows")?;
    let consumed = draws.len() as u64;
    if consumed >= 16384 || receipt["exhausted"].as_bool() != Some(false) {
        return Err("Source input exhausted or exhaustion flag missing".into());
    }
    if receipt["consumed"].as_u64() != Some(consumed)
        || receipt["unconsumed"].as_u64() != Some(16384 - consumed)
        || terminal_draws != consumed
    {
        return Err("Source input counts do not bind completion".into());
    }
    if input["layout"] != json!([0, 1])
        || !receipt["startup"].as_object().is_some_and(|fields|
            ["spawns", "environmentSeed32", "weatherSeed", "wind", "shower", "rain", "wet", "weather", "nextId"]
                .iter().all(|key| fields.contains_key(*key)))
        || !receipt["layout"].is_object()
        || receipt["layout"]["input"] != input["layout"]
        || receipt["layout"]["boundary"] != "before player spawns, placement and actor allocation"
        || !receipt["layout"]["selectedBeforeInjection"].is_array()
    {
        return Err("Source startup/layout receipt missing or unbound".into());
    }
    for (index, row) in draws.iter().enumerate() {
        let fields = row.as_object().ok_or("invalid Source draw row")?;
        if fields.len() != 3
            || !["index", "consumer", "bits"].iter().all(|key| fields.contains_key(*key))
            || row["index"].as_u64() != Some(index as u64)
            || row["bits"].as_str().is_none()
            || row["bits"] != tape[index]
        {
            return Err("Source draw row does not bind the authored input".into());
        }
        let consumer = row["consumer"].as_array().ok_or("missing Source consumer")?;
        if consumer.first().and_then(Value::as_str)
            .is_none_or(|tag| tag.is_empty() || tag.starts_with("UNMAPPED"))
        {
            return Err("unmapped Source consumer".into());
        }
    }
    Ok(())
}

pub(super) fn input_counterfactuals(
    receipt: &Value,
    input: &Value,
    terminal_draws: u64,
) -> Vec<&'static str> {
    input_receipt(receipt, input, terminal_draws)
        .expect("authentic Source input must validate before controls");
    assert!(terminal_draws > 0, "controls require real consumed input");
    let mut checked = Vec::new();
    let mut reject = |label, changed: Value| {
        assert!(input_receipt(&changed, input, terminal_draws).is_err(),
            "input receipt control must be decisive: {label}");
        checked.push(label);
    };
    let mut r = receipt.clone(); r.as_object_mut().unwrap().remove("capacity");
    reject("missing capacity", r);
    let mut r = receipt.clone(); r["capacity"] = json!(16385);
    reject("wrong capacity", r);
    let mut r = receipt.clone(); r["consumed"] = json!(terminal_draws + 1);
    reject("wrong consumed count", r);
    let mut r = receipt.clone(); r["unconsumed"] = json!(0);
    reject("wrong unconsumed count", r);
    let mut r = receipt.clone(); r["exhausted"] = json!(true);
    reject("exhausted tape", r);
    let mut r = receipt.clone(); r.as_object_mut().unwrap().remove("exhausted");
    reject("missing exhaustion flag", r);
    let mut r = receipt.clone(); r["draws"] = Value::Null;
    reject("missing draw rows", r);
    let mut r = receipt.clone(); r["draws"][0]["index"] = json!(1);
    reject("wrong draw index", r);
    let mut r = receipt.clone(); r["draws"][0]["index"] = json!("0");
    reject("string draw index", r);
    let mut r = receipt.clone(); r["draws"][0]["bits"] = json!("not-authored-bits");
    reject("wrong draw bits", r);
    let mut r = receipt.clone(); r["draws"][0].as_object_mut().unwrap().remove("consumer");
    reject("missing consumer", r);
    let mut r = receipt.clone(); r["draws"][0]["consumer"] = json!([]);
    reject("empty consumer", r);
    let mut r = receipt.clone(); r["draws"][0]["consumer"][0] = json!("UNMAPPED.probe");
    reject("unmapped consumer", r);
    let mut r = receipt.clone(); r["draws"][0]["extra"] = json!(true);
    reject("extra draw field", r);
    let mut r = receipt.clone(); r["startup"] = Value::Null;
    reject("missing startup receipt", r);
    let mut r = receipt.clone(); r["layout"]["input"] = json!([1, 0]);
    reject("wrong injected layout", r);
    let mut r = receipt.clone(); r["layout"].as_object_mut().unwrap().remove("selectedBeforeInjection");
    reject("missing original layout selection", r);
    let mut r = receipt.clone(); r["layout"]["boundary"] = json!("after actor allocation");
    reject("wrong layout boundary", r);
    assert!(input_receipt(receipt, input, terminal_draws + 1).is_err());
    checked.push("completion draw count differs from input receipt");
    checked
}

// Process receipts and successful recheck copies are required before Match exists.
// The raw copies bind bytes directly; their SHA256 values remain available to Root.
pub(super) fn checked_capture(path: &std::path::Path, name: &str, bytes: &[u8]) -> Value {
    let sibling = |suffix: &str| std::path::PathBuf::from(format!("{}{suffix}", path.display()));
    let read_json = |suffix: &str| -> Value {
        serde_json::from_slice(&std::fs::read(sibling(suffix))
            .expect("Source process result required before native construction")).unwrap()
    };
    let generated = read_json(".generate.result.json");
    let checked = read_json(".check.result.json");
    let head = std::env::var("WW2_GROUND_SOURCE_HEAD").expect("exact Source-producing head");
    let input_path = std::path::PathBuf::from(std::env::var_os("WW2_GROUND_INPUT").unwrap());
    let input_bytes = std::fs::read(&input_path).unwrap();
    let rows = std::io::Cursor::new(bytes).lines().map(|line|
        serde_json::from_str::<Value>(&line.unwrap()).unwrap()).collect::<Vec<_>>();
    let last = rows.last().expect("Source terminal input receipt");
    assert_eq!(last["evidence"]["stage"], "inputReceipt");
    for (result, phase) in [(&generated, "generate"), (&checked, "check")] {
        assert_eq!(result["schema"], "ground-source-process-v1");
        assert_eq!(result["phase"], phase);
        assert_eq!(result["result"], "PASS");
        assert_eq!(result["sourceHead"], head);
        assert_eq!(result["reference"], "e4e579a838aabae3b1d9dadbc17a0a87147bc52d");
        assert_eq!(result["case"], name);
        assert_eq!(result["output"].as_str(), path.to_str());
        let process = result["process"].as_object().expect("actual Source process result");
        assert_eq!(process.len(), 5);
        assert!(process.get("pid").and_then(Value::as_u64).is_some_and(|pid| pid > 0));
        assert_eq!(process.get("exitCode").and_then(Value::as_i64), Some(0));
        assert_eq!(process.get("signal"), Some(&Value::Null));
        assert_eq!(process.get("error"), Some(&Value::Null));
        assert_eq!(process.get("terminal").and_then(Value::as_bool), Some(true));
        assert_eq!(result.get("error"), Some(&Value::Null));
        assert_eq!(result["capture"]["path"].as_str(), path.to_str());
        assert_eq!(result["capture"]["bytes"].as_u64(), Some(bytes.len() as u64));
        assert_eq!(result["input"]["path"].as_str(), input_path.to_str());
        assert_eq!(result["input"]["bytes"].as_u64(), Some(input_bytes.len() as u64));
        for key in ["capture", "input"] {
            let sha = result[key]["sha256"].as_str().expect("actual artifact SHA256");
            assert!(sha.len() == 64 && sha.bytes().all(|ch| ch.is_ascii_digit() || (b'a'..=b'f').contains(&ch)));
        }
        assert_eq!(result["terminal"]["rows"].as_u64(), Some(rows.len() as u64));
        assert!(last["state"]["tick"].as_i64().is_some_and(|tick| tick >= 0));
        assert!(last["state"]["drawCount"].as_u64().is_some());
        assert_eq!(result["terminal"]["tick"], last["state"]["tick"]);
        assert_eq!(result["terminal"]["drawCount"], last["state"]["drawCount"]);
    }
    assert_eq!(generated["capture"], checked["capture"]);
    assert_eq!(generated["input"], checked["input"]);
    assert_eq!(generated["terminal"], checked["terminal"]);
    assert_eq!(std::fs::read(sibling(".checked.jsonl")).expect("successful real Source recheck copy"), bytes);
    assert_eq!(std::fs::read(sibling(".checked.input")).expect("successful Source input byte binding"), input_bytes);
    json!({"generate":generated,"check":checked,"captureBytesEqualToCheckedCopy":true,
        "authoredInputBytesEqualToCheckedCopy":true})
}
