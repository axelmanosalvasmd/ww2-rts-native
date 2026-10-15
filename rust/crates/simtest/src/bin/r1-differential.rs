//! Diagnostic whole-match prefix comparison, never a runtime state restorer.
use serde_json::{Value, json};
use std::{collections::BTreeSet, process::ExitCode};
use ww2_core::{
    economy::{Faction, Mode},
    economy_state::ArmySize,
    map::AuthoritativeMap,
    match_commands::Command,
    match_runtime::{Match, MatchOptions, PlayerConfig},
    orders::OrderKey,
    weather::WeatherChoice,
};

const PIN: &str = "e4e579a838aabae3b1d9dadbc17a0a87147bc52d";

fn number(value: f64) -> Value {
    json!({"bits":format!("{:016x}",value.to_bits()),"value":value})
}

fn snapshot(m: &Match) -> Value {
    json!({
        "tick":m.world.game.tick,
        "units":m.world.game.units.values().map(|u|json!({"id":u.id,"owner":u.owner,"kind":m.rules.kinds[u.kind].name,"x":number(u.x),"z":number(u.z),"hp":number(u.hp)})).collect::<Vec<_>>(),
        "players":m.start.economy.players.iter().enumerate().map(|(slot,p)|json!({"slot":slot,"team":p.team,"mp":number(p.mp),"mun":number(p.mun),"fuel":number(p.fuel),"vp":number(p.vp),"inc":number(p.inc)})).collect::<Vec<_>>(),
        "mode":{"requested":"conquest","runtime":m.world.game.mode.map(|mode|format!("{mode:?}"))},
        "score":{"winner":m.finish.map(|f|f.winner),"vp":m.start.economy.players.iter().map(|p|number(p.vp)).collect::<Vec<_>>()},
        "points":m.start.points.iter().map(|p|json!({"owner":p.owner.map_or(-1,|v|v as i64),"capper":p.capper.map_or(-1,|v|v as i64),"progress":number(p.progress),"cut":p.cut})).collect::<Vec<_>>()
    })
}

// Bits are the comparison value; decimal JSON is a readable rendering. JSON's
// integer/float storage tag and its loss of the sign on rendered zero are not
// additional simulation fields. Reject inconsistent or malformed wrappers.
fn numeric_bits(value: &Value) -> Option<u64> {
    let object = value.as_object()?;
    if object.len() != 2 {
        return None;
    }
    let text = object.get("bits")?.as_str()?;
    if text.len() != 16 || !text.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let bits = u64::from_str_radix(text, 16).ok()?;
    let number = f64::from_bits(bits);
    let display = object.get("value")?;
    let consistent = if number.is_finite() {
        display
            .as_f64()
            .is_some_and(|v| v.to_bits() == bits || v == 0.0 && number == 0.0)
    } else {
        display.as_str()
            == Some(if number.is_nan() {
                "NaN"
            } else if number.is_sign_positive() {
                "Infinity"
            } else {
                "-Infinity"
            })
    };
    consistent.then_some(bits)
}

fn difference(source: &Value, native: &Value, field: &str) -> Option<Value> {
    if source.get("bits").is_some() || native.get("bits").is_some() {
        if numeric_bits(source).is_some_and(|bits| Some(bits) == numeric_bits(native)) {
            return None;
        }
        return Some(json!({"field":field,"source":source,"native":native}));
    }
    if source == native {
        return None;
    }
    match (source, native) {
        (Value::Array(a), Value::Array(b)) if a.len() == b.len() => a
            .iter()
            .zip(b)
            .enumerate()
            .find_map(|(i, (a, b))| difference(a, b, &format!("{field}[{i}]"))),
        (Value::Object(a), Value::Object(b)) if a.keys().eq(b.keys()) => {
            let priority = ["units", "players", "mode", "score", "points", "tick"];
            priority
                .iter()
                .filter_map(|key| a.get(*key).map(|value| (*key, value)))
                .chain(
                    a.iter()
                        .filter(|(key, _)| !priority.contains(&key.as_str()))
                        .map(|(key, value)| (key.as_str(), value)),
                )
                .find_map(|(key, value)| difference(value, &b[key], &format!("{field}.{key}")))
        }
        _ => Some(json!({"field":field,"source":source,"native":native})),
    }
}

// Additional comparison-only access for the private core test. The raw comparator
// and its standalone diagnostic path retain their original behavior.
#[cfg(test)]
pub(crate) fn compare_projection_for_test(source: &Value, native: &Value) -> Option<Value> {
    difference(source, native, "$")
}

fn row_metadata(tick: usize, field: &str) -> Value {
    if tick == 0
        && field.starts_with("$.units[")
        && (field.ends_with(".x") || field.ends_with(".z"))
    {
        json!({"owningRows":["MODE-22"],"relatedInvestigationRows":["MODE-21","IF-20","IF-21"],"cause":"unassigned; constructor position-list mismatch, without RNG-cause attribution","rowStatusChanged":false})
    } else {
        json!({"owningRows":[],"cause":"unassigned; requires source investigation before assigning a parity row","rowStatusChanged":false})
    }
}

fn run(trace: &Value) -> Result<Value, String> {
    run_with_match(trace, Match::new)
}

pub(crate) fn run_with_match(
    trace: &Value,
    constructor: impl FnOnce(&AuthoritativeMap, MatchOptions) -> Result<Match, String>,
) -> Result<Value, String> {
    if trace["format"] != "r1-whole-match-differential-v1"
        || trace["reference"] != PIN
        || trace["requestedTicks"] != 600
        || trace["checkpointEvery"] != 10
    {
        return Err("unsupported trace identity or cadence".into());
    }
    let profile = json!({"mode":"conquest","army":"standard","weather":"clear","shuffle":true,"tech":true,"supply":true,"livingWeather":true,"logistics":false,"sandbox":false,"weatherSeed":null});
    if trace["profile"] != profile {
        return Err("trace does not select the explicit bounded R1 profile".into());
    }
    let committed: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../server/src/authoritative-maps/default.json"
    )))
    .map_err(|e| e.to_string())?;
    if trace["map"] != committed {
        return Err("trace map differs from the committed Default authoring".into());
    }
    let seed = trace["seed"]
        .as_str()
        .ok_or("missing private seed")?
        .parse::<u64>()
        .map_err(|_| "invalid private seed")?;
    let map = AuthoritativeMap::from_json(&serde_json::to_vec(&committed).unwrap())
        .map_err(|e| e.to_string())?;
    let mut m = constructor(
        &map,
        MatchOptions {
            seed,
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
            map_key: Some("default".into()),
            tech: true,
            supply: true,
            logistics: false,
            sandbox: false,
        },
    )?;
    m.validate_tick_state()?;
    let orders = trace["orders"]
        .as_array()
        .ok_or("missing recorded orders")?;
    let mut keys = BTreeSet::new();
    for row in orders {
        let tick = row["tick"].as_u64().ok_or("invalid order tick")?;
        let issuer = row["issuer"]
            .as_u64()
            .filter(|v| *v < 2)
            .ok_or("invalid issuer")? as u32;
        let sequence = row["sequence"].as_u64().ok_or("invalid order sequence")?;
        if !(1..=600).contains(&tick)
            || !keys.insert((tick, issuer, sequence))
            || !matches!(
                row["command"]["t"].as_str(),
                Some("move" | "amove" | "stop" | "stance")
            )
        {
            return Err("invalid recorded order stream".into());
        }
        m.enqueue_command(
            OrderKey {
                tick,
                issuer,
                sequence,
            },
            Command::from_json(&row["command"]).map_err(|e| format!("recorded command: {e:?}"))?,
        )
        .map_err(|e| format!("recorded queue: {e:?}"))?;
    }
    let frames = trace["frames"].as_array().ok_or("missing Source frames")?;
    let completed_source = trace["completed"]
        .as_u64()
        .ok_or("missing Source tick count")?;
    if frames.len() != completed_source as usize + 1
        || frames
            .iter()
            .enumerate()
            .any(|(i, f)| f["tick"].as_u64() != Some(i as u64))
    {
        return Err("Source frames are truncated or out of order".into());
    }
    let mut native_frames = vec![snapshot(&m)];
    let mut failure = None;
    let mut moved_ticks = 0;
    let mut events = 0;
    let mut denials = Vec::new();
    let mut unavailable = BTreeSet::new();
    // A state mismatch never ends either engine. Only an actual runner fault does.
    for tick in 1..=600 {
        let before = m
            .world
            .game
            .units
            .values()
            .map(|u| (u.id, u.x, u.z))
            .collect::<Vec<_>>();
        match m.advance_tick() {
            Ok(report) => {
                events += report.events.len();
                unavailable.extend(report.unavailable.iter().map(|phase| format!("{phase:?}")));
                for command in report.commands {
                    if let Some(reason) = command.denial {
                        denials.push(json!({"tick":tick,"issuer":command.key.issuer,"sequence":command.key.sequence,"reason":format!("{reason:?}")}));
                    }
                }
                if before.iter().any(|(id, x, z)| {
                    m.world
                        .game
                        .units
                        .get(id)
                        .is_some_and(|u| u.x != *x || u.z != *z)
                }) {
                    moved_ticks += 1;
                }
                native_frames.push(snapshot(&m));
            }
            Err(error) => {
                failure =
                    Some(json!({"nextTick":tick,"internalTick":error.tick,"reason":error.reason}));
                break;
            }
        }
    }
    let mut first = None;
    let mut checkpoints_matched = 0;
    let mut ticks_matched = 0;
    let mut prefix = 0;
    for (tick, (source, native)) in frames.iter().zip(&native_frames).enumerate() {
        if let Some(delta) = difference(source, native, "$") {
            if first.is_none() {
                let rows = row_metadata(tick, delta["field"].as_str().unwrap_or(""));
                first = Some(json!({"tick":tick,"difference":delta,"rows":rows}));
            }
        } else {
            if tick > 0 {
                ticks_matched += 1;
            }
            if tick % 10 == 0 {
                checkpoints_matched += 1;
            }
            if first.is_none() {
                prefix = tick;
            }
        }
    }
    let native_ticks = native_frames.len() - 1;
    let complete = completed_source == 600
        && native_ticks == 600
        && trace["failure"].is_null()
        && failure.is_none()
        && trace["movedTicks"].as_u64().is_some_and(|v| v > 0)
        && moved_ticks > 0
        && !orders.is_empty();
    #[cfg(test)]
    if let Some(path) = std::env::var_os("WW2_ALIGNED_NATIVE_FRAMES") {
        std::fs::write(path, serde_json::to_vec(&native_frames).unwrap())
            .map_err(|e| e.to_string())?;
    }
    Ok(json!({
        "harnessCompleted":complete,"differentialVerdict":if !complete {"INCOMPLETE"} else if first.is_some() {"MISMATCH"} else {"MATCH"},
        "sourceTicks":completed_source,"nativeTicks":native_ticks,"actualTicksMatched":ticks_matched,"matchedTickPrefix":prefix,
        "checkpointsCompared":frames.len().min(native_frames.len()).div_ceil(10),"checkpointsMatched":checkpoints_matched,
        "firstMismatch":first,"sourceFailure":trace["failure"],"nativeFailure":failure,
        "sourceMovedTicks":trace["movedTicks"],"nativeMovedTicks":moved_ticks,"nativeEvents":events,
        "sourceDenials":trace["denials"],"nativeDenials":denials,"nativeUnavailablePhases":unavailable,
        "limits":"600-tick prefix; no tolerance, RNG synchronization, state priming, network or completed-victory claim; owner parity criterion pending"
    }))
}

fn main() -> ExitCode {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let report_only = args.iter().any(|a| a == "--report-only");
    let result = args
        .first()
        .ok_or_else(|| "usage: r1-differential PRIVATE_TRACE.json [--report-only]".into())
        .and_then(|path| std::fs::read_to_string(path).map_err(|e| format!("trace: {e}")))
        .and_then(|text| serde_json::from_str(&text).map_err(|e| format!("trace JSON: {e}")))
        .and_then(|trace| run(&trace));
    match result {
        Ok(mut report) => {
            report["reportOnly"] = json!(report_only);
            let completed = report["harnessCompleted"] == true;
            let matched = report["differentialVerdict"] == "MATCH";
            println!("{report}");
            if !completed {
                ExitCode::from(2)
            } else if matched || report_only {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_comparator_retains_first_unit_field_and_signed_zero() {
        let integer: Value =
            serde_json::from_str(r#"{"bits":"4059000000000000","value":100}"#).unwrap();
        let float: Value =
            serde_json::from_str(r#"{"bits":"4059000000000000","value":100.0}"#).unwrap();
        assert!(difference(&integer, &float, "$.hp").is_none());
        assert!(
            difference(
                &json!({"bits":"4059000000000000","value":99}),
                &float,
                "$.hp"
            )
            .is_some()
        );
        let source =
            json!({"units":[{"hp":number(100.0),"x":number(-0.0)}],"score":{"winner":null}});
        let native = json!({"units":[{"hp":number(100.0),"x":number(0.0)}],"score":{"winner":1}});
        let mismatch = difference(&source, &native, "$").unwrap();
        assert_eq!(mismatch["field"], "$.units[0].x");
        assert_eq!(mismatch["source"]["bits"], "8000000000000000");
        assert_eq!(mismatch["native"]["bits"], "0000000000000000");
        assert!(difference(&source, &source, "$").is_none());
        assert!(difference(&json!([1]), &json!([]), "$").is_some());
    }
}
