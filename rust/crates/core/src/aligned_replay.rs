//! Private, per-match test input. No hook, storage or entry point exists in a shipping build.
use crate::{match_runtime::Match, random::StreamId};
use serde_json::{Value, json};
use std::{
    panic::Location,
    sync::{Arc, Mutex},
};

#[allow(dead_code)]
#[path = "../../simtest/src/bin/r1-differential.rs"]
mod original;

#[path = "aligned_replay_comparison.rs"]
mod canonical;

#[path = "aligned_replay_startup.rs"]
mod startup_state;

#[derive(Clone, Debug)]
pub struct Replay(Arc<Mutex<Tape>>);
impl PartialEq for Replay {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for Replay {}
#[derive(Debug)]
struct Tape {
    input: Value,
    values: Vec<f64>,
    used: Vec<Value>,
    layout: Option<Value>,
    startup: Option<Value>,
    shot: Option<(i64, u32, u32, u32)>,
}
impl Replay {
    fn new(input: &Value) -> Result<Self, String> {
        let values = input["tape"]
            .as_array()
            .ok_or("missing shared random tape")?
            .iter()
            .map(|v| {
                let bits = v.as_str().ok_or("invalid draw bits")?;
                if bits.len() != 16 {
                    return Err("invalid draw bit width");
                }
                let value =
                    f64::from_bits(u64::from_str_radix(bits, 16).map_err(|_| "invalid draw bits")?);
                if !value.is_finite() || !(0.0..1.0).contains(&value) {
                    return Err("invalid fraction");
                }
                Ok(value)
            })
            .collect::<Result<Vec<_>, _>>()?;
        if values.len() != 16384 {
            return Err("unexpected tape capacity".into());
        }
        Ok(Self(Arc::new(Mutex::new(Tape {
            input: input.clone(),
            values,
            used: Vec::new(),
            layout: None,
            startup: None,
            shot: None,
        }))))
    }
    pub(crate) fn draw(&self, consumer: Value) -> f64 {
        let mut tape = self.0.lock().unwrap();
        let index = tape.used.len();
        let value = *tape
            .values
            .get(index)
            .expect("shared random tape exhausted; no fallback");
        tape.used.push(json!({"index": index, "consumer": consumer, "bits": format!("{:016x}", value.to_bits())}));
        value
    }
    pub(crate) fn integer(&self, purpose: StreamId, caller: &Location<'_>) -> u64 {
        let consumer =
            if purpose == StreamId::Weather && caller.file().ends_with("match_runtime.rs") {
                json!(["startup.environment.seed32"])
            } else {
                json!([
                    "UNMAPPED.u64",
                    format!("{purpose:?}"),
                    caller.file(),
                    caller.line()
                ])
            };
        // The declared consumer is Source's floor(random * 2**32), before Mulberry32.
        (self.draw(consumer) * 4_294_967_296.0) as u64
    }
    pub(crate) fn fraction(&self, purpose: StreamId, caller: &Location<'_>) -> f64 {
        let consumer =
            if purpose == StreamId::Terrain && caller.file().ends_with("match_runtime.rs") {
                json!(["startup.layout.shuffle"])
            } else if purpose == StreamId::Combat && caller.file().ends_with("match_firing.rs") {
                let line = include_str!("match_firing.rs")
                    .lines()
                    .nth(caller.line() as usize - 1)
                    .unwrap_or("");
                let role = if line.contains("< accuracy") {
                    "combat.accuracy"
                } else if line.contains("* 0.7") {
                    "combat.offset.magnitude"
                } else if line.contains("< 0.5") {
                    "combat.offset.sign"
                } else {
                    "UNMAPPED.combat"
                };
                let tape = self.0.lock().unwrap();
                match tape.shot {
                    Some((tick, actor, target, round)) => json!([role, tick, actor, target, round]),
                    None => json!(["UNMAPPED.combat.context", caller.line()]),
                }
            } else {
                json!([
                    "UNMAPPED.fraction",
                    format!("{purpose:?}"),
                    caller.file(),
                    caller.line()
                ])
            };
        self.draw(consumer)
    }
    pub(crate) fn shot(&self, tick: i64, actor: u32, target: u32, round: u32) {
        self.0.lock().unwrap().shot = Some((tick, actor, target, round));
    }
    pub(crate) fn layout(
        &self,
        selected: &[usize],
        available: &[usize],
        players: usize,
    ) -> Result<Vec<usize>, String> {
        let mut tape = self.0.lock().unwrap();
        let slots = tape.input["layout"]
            .as_array()
            .ok_or("missing authored test layout")?
            .iter()
            .map(|v| {
                v.as_u64()
                    .and_then(|n| usize::try_from(n).ok())
                    .ok_or("invalid test layout")
            })
            .collect::<Result<Vec<_>, _>>()?;
        if slots.len() != players
            || slots.iter().any(|n| !available.contains(n))
            || slots[0] == slots[1]
        {
            return Err("test layout does not bind two opposing authored spawns".into());
        }
        tape.layout = Some(
            json!({"input":slots,"selectedBeforeInjection":selected,"boundary":"before player spawns, placement and actor allocation"}),
        );
        Ok(slots)
    }
    pub(crate) fn startup(&self, m: &Match) {
        let bits = |v: f64| format!("{:016x}", v.to_bits());
        self.0.lock().unwrap().startup = Some(json!({
            "spawns":m.start.players.iter().map(|p|json!([bits(p.spawn.x),bits(p.spawn.z)])).collect::<Vec<_>>(),
            "environmentSeed32":m.start.environment_seed,"weatherSeed":m.start.weather_seed,
            "wind":[bits(m.start.wind.0),bits(m.start.wind.1)],
            "shower":m.start.living.map(|v|json!({"raining":v.raining,"next":bits(v.next)})),
            "rain":bits(m.world.game.rain),"wet":m.world.game.wet.map(bits),
            "weather":{"now":m.start.weather.now.name(),"next":m.start.weather.next.map(|v|v.name()),"at":m.start.weather.at},
            "nextId":m.next_unit_id
        }));
    }
    fn receipt(&self) -> Value {
        let tape = self.0.lock().unwrap();
        json!({"capacity":tape.values.len(),"consumed":tape.used.len(),"unconsumed":tape.values.len()-tape.used.len(),
            "exhausted":tape.used.len()==tape.values.len(),"draws":tape.used,"layout":tape.layout,"startup":tape.startup})
    }
}

pub(crate) fn environment_draw(seed: &mut u32, replay: Option<&Replay>, consumer: Value) -> f64 {
    let before = *seed;
    // Execute the original generator, including its seed advance, at the real consumer.
    let original = crate::weather_runtime::environment_random(seed);
    if let Some(replay) = replay.filter(|r| startup_state::cmb42::is_ordinary(r)) {
        startup_state::cmb42::environment_observation(replay, before, *seed, original, consumer);
        return original;
    }
    replay.map_or(original, |r| r.draw(consumer))
}

pub(crate) fn observe_cmb42_command(m: &Match, command: &crate::match_commands::Command) -> Result<(), String> {
    startup_state::cmb42::command_observation(m, command)
}

#[test]
fn tape_rejects_malformed_inputs_and_records_every_consumption() {
    let input = json!({"tape":vec!["3fe0000000000000";16384],"layout":[0,1]});
    let replay = Replay::new(&input).unwrap();
    assert_eq!(replay.draw(json!(["probe", 0])), 0.5);
    let cloned = replay.clone();
    assert_eq!(cloned.draw(json!(["probe", 1])), 0.5);
    let receipt = replay.receipt();
    assert_eq!(receipt["consumed"], 2);
    assert_eq!(receipt["unconsumed"], 16382);
    assert_eq!(receipt["draws"][1]["index"], 1);
    assert_eq!(receipt["draws"][1]["consumer"], json!(["probe", 1]));
    assert!(Replay::new(&json!({"tape":["3ff0000000000000"]})).is_err());
    assert!(replay.layout(&[1, 0], &[0, 1], 2).is_ok());
    assert!(replay.layout(&[1, 0], &[0], 2).is_err());
}

#[test]
#[ignore = "requires the private frozen Source fixture and explicit S1 grant"]
fn original_r1_aligned_61_checkpoints() {
    let path =
        std::env::var_os("WW2_ALIGNED_TRACE").expect("private frozen Source fixture required");
    let bytes = std::fs::read(path).unwrap();
    let trace: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(trace["alignedFormat"], "r1-aligned-inputs-v1");
    let replay = Replay::new(&trace["sharedInput"]).unwrap();
    let result = original::run_with_match(&trace, |map, options| {
        Match::new_aligned(map, options, replay.clone())
    });
    let receipt = replay.receipt();
    let mut report = match result {
        Ok(report) => report,
        Err(error) => json!({"harnessCompleted":false,"harnessFailure":error}),
    };
    let source = &trace["inputReceipt"];
    let first_draw = source["draws"]
        .as_array()
        .unwrap()
        .iter()
        .zip(receipt["draws"].as_array().unwrap())
        .position(|(a, b)| a != b);
    let startup_equal = source["startup"] == receipt["startup"];
    let aligned = first_draw.is_none()
        && source["consumed"] == receipt["consumed"]
        && source["unconsumed"] == receipt["unconsumed"]
        && source["layout"]["input"] == receipt["layout"]["input"]
        && startup_equal;
    report["alignedInputs"] = json!(aligned);
    report["alignmentFirstDrawDifference"] = first_draw.map_or(
        Value::Null,
        |i| json!({"index":i,"source":source["draws"][i],"native":receipt["draws"][i]}),
    );
    report["startupInputsAgree"] = json!(startup_equal);
    report["nativeInputReceipt"] = receipt;
    report["limits"] = json!(
        "test-only 600-tick R1 prefix, exact original projection; no completed row, victory, socket or Unreal claim"
    );
    // Read only the original runner's raw frame artifact. Engine state and input
    // receipts are never changed by the additional comparison layer.
    let canonical_result = if report["harnessCompleted"] == true {
        std::env::var_os("WW2_ALIGNED_NATIVE_FRAMES")
            .ok_or_else(|| "raw native frames path required".to_owned())
            .and_then(|path| std::fs::read(path).map_err(|e| e.to_string()))
            .and_then(|bytes| serde_json::from_slice(&bytes).map_err(|e| e.to_string()))
            .and_then(|native| canonical::compare_frames(&trace, &native))
    } else {
        Err("raw runner did not complete".into())
    };
    let canonical_complete = canonical_result.is_ok();
    report["canonicalComparison"] =
        canonical_result.unwrap_or_else(|error| json!({"failure":error}));
    let output = std::env::var_os("WW2_ALIGNED_REPORT").expect("private report path required");
    std::fs::write(output, serde_json::to_vec(&report).unwrap()).unwrap();
    println!(
        "{}",
        json!({"alignedInputs":aligned,"differentialVerdict":report["differentialVerdict"],"checkpointsMatched":report["checkpointsMatched"],"checkpointsCompared":report["checkpointsCompared"],"firstMismatch":report["firstMismatch"],"alignmentFirstDrawDifference":report["alignmentFirstDrawDifference"],"sourceDraws":source["consumed"],"nativeDraws":report["nativeInputReceipt"]["consumed"],"canonicalComparison":report["canonicalComparison"]})
    );
    assert_eq!(
        report["harnessCompleted"], true,
        "full failure and trace retained"
    );
    assert_eq!(report["checkpointsCompared"], 61);
    assert!(aligned, "alignment failure retained, never a parity pass");
    assert!(canonical_complete, "canonical comparison failure retained");
    assert_eq!(report["canonicalComparison"]["checkpointsCompared"], 61);
    // MISMATCH is a completed diagnostic result, never an equivalence assertion.
}
