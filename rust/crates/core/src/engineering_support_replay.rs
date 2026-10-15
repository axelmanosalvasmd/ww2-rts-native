//! Bounded original hedge entropy. This entire module belongs to core cfg(test).
use super::{Replay, Tape};
use crate::random::StreamId;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

#[derive(Debug)]
pub(super) struct Scope {
    pub(super) tick: i64,
    pub(super) closed: bool,
    pub(super) active: bool,
    pub(super) entries: Vec<Value>,
}

pub(super) fn role(file: &str, line: u32) -> Option<&'static str> {
    if !file.ends_with("match_support.rs") {
        return None;
    }
    let text = include_str!("match_support.rs")
        .lines()
        .nth(line.checked_sub(1)? as usize)?;
    if text.contains("let along = (self.start.random.next_fraction(StreamId::OffMap)") {
        Some("along")
    } else if text.contains("let side = (self.start.random.next_fraction(StreamId::OffMap)") {
        Some("side")
    } else {
        None
    }
}

impl Replay {
    fn engineering(entries: &Value) -> Result<Self, String> {
        let entries = entries.as_array().ok_or("missing original constant tape")?;
        if entries.len() != 20 {
            return Err("original artillery requires exactly twenty fractions".into());
        }
        let mut previous = 0;
        for (index, entry) in entries.iter().enumerate() {
            let tick = entry["tick"].as_i64().ok_or("invalid original draw tick")?;
            let axis = if index % 2 == 0 { "along" } else { "side" };
            if !(1..=200).contains(&tick)
                || tick < previous
                || (index % 2 == 1 && tick != previous)
                || (index > 0 && index % 2 == 0 && tick == previous)
                || entry["axis"] != axis
                || entry["index"].as_u64() != Some(index as u64)
                || entry["bits"] != "3fe0000000000000"
                || !entry["site"]
                    .as_str()
                    .is_some_and(|site| site.contains("shared/sim.js:5287:"))
            {
                return Err("original constant call identity/value/order differs".into());
            }
            previous = tick;
        }
        Ok(Self(Arc::new(Mutex::new(Tape {
            input: json!({"scene":"test.js:701:0","constantWindow":[1,200]}),
            values: entries
                .iter()
                .map(|entry| {
                    f64::from_bits(
                        u64::from_str_radix(entry["bits"].as_str().unwrap(), 16).unwrap(),
                    )
                })
                .collect(),
            used: Vec::new(),
            layout: None,
            startup: None,
            shot: None,
            engineering: Some(Scope {
                tick: 0,
                closed: false,
                active: false,
                entries: entries.clone(),
            }),
        }))))
    }

    fn engineering_begin(&self, tick: i64) -> Result<(), String> {
        let mut tape = self.0.lock().unwrap();
        let used = tape.used.len();
        let scope = tape
            .engineering
            .as_mut()
            .ok_or("not original hedge input")?;
        if scope.closed
            || scope.active
            || !(1..=200).contains(&tick)
            || tick != scope.tick + 1
            || scope
                .entries
                .get(used)
                .is_some_and(|v| v["tick"].as_i64().unwrap() < tick)
        {
            return Err("original constant tick skipped or outside window".into());
        }
        scope.tick = tick;
        scope.active = true;
        Ok(())
    }

    fn engineering_end(&self, tick: i64) -> Result<(), String> {
        let mut tape = self.0.lock().unwrap();
        let used = tape.used.len();
        let scope = tape
            .engineering
            .as_mut()
            .ok_or("not original hedge input")?;
        if scope.closed
            || !scope.active
            || scope.tick != tick
            || scope
                .entries
                .get(used)
                .is_some_and(|v| v["tick"].as_i64().unwrap() <= tick)
        {
            return Err("original constant tick has missing calls or no active interval".into());
        }
        scope.active = false;
        Ok(())
    }

    pub(super) fn engineering_draw(
        &self,
        purpose: StreamId,
        axis: Option<&str>,
    ) -> Result<f64, String> {
        let mut tape = self.0.lock().unwrap();
        let index = tape.used.len();
        let scope = tape
            .engineering
            .as_ref()
            .ok_or("not original hedge input")?;
        let entry = scope
            .entries
            .get(index)
            .ok_or("extra original constant call")?;
        if scope.closed
            || !scope.active
            || purpose != StreamId::OffMap
            || entry["tick"].as_i64() != Some(scope.tick)
            || axis.is_none()
            || entry["axis"].as_str() != axis
        {
            return Err("unexpected original constant purpose/site/axis/tick".into());
        }
        let tick = scope.tick;
        let entry = entry.clone();
        let value = tape.values[index];
        tape.used.push(
            json!({"index":index,"consumer":["hedge.artillery",axis,tick],
            "bits":format!("{:016x}",value.to_bits()),"sourceSite":entry["site"]}),
        );
        Ok(value)
    }

    fn engineering_finish(&self) -> Result<(), String> {
        let mut tape = self.0.lock().unwrap();
        if tape.used.len() != 20 {
            return Err("missing original constant calls".into());
        }
        let scope = tape
            .engineering
            .as_mut()
            .ok_or("not original hedge input")?;
        if scope.closed || scope.active || scope.tick != 200 {
            return Err("original constant window incomplete".into());
        }
        scope.closed = true;
        Ok(())
    }
}

#[test]
fn original_constant_scope_rejects_wrong_inputs_consumers_and_incomplete_ticks() {
    // Schema controls only. These are not recorded Source output or match fixtures.
    let entries = json!(
        (0..20)
            .map(|index| json!({"index":index,"tick":1+index/2,
        "axis":if index%2==0 {"along"} else {"side"},"bits":"3fe0000000000000",
        "site":"stepStrikes (reference/ww2-rts/shared/sim.js:5287:40)"}))
            .collect::<Vec<_>>()
    );
    assert!(Replay::engineering(&Value::Null).is_err());
    for kind in [
        "missing",
        "extra",
        "value",
        "axis",
        "site",
        "order",
        "type",
        "negative",
        "zero",
        "late",
        "pair",
        "repeated",
        "descending",
    ] {
        let mut invalid = entries.clone();
        match kind {
            "missing" => {
                invalid.as_array_mut().unwrap().pop();
            }
            "extra" => {
                invalid.as_array_mut().unwrap().push(entries[0].clone());
            }
            "value" => invalid[0]["bits"] = json!("3fd0000000000000"),
            "axis" => invalid[0]["axis"] = json!("side"),
            "site" => invalid[0]["site"] = json!("unrelated"),
            "order" => invalid[0]["index"] = json!(1),
            "type" => invalid[0]["tick"] = json!("1"),
            "negative" => invalid[0]["tick"] = json!(-1),
            "zero" => invalid[0]["tick"] = json!(0),
            "late" => invalid[0]["tick"] = json!(201),
            "pair" => invalid[1]["tick"] = json!(2),
            "repeated" => {
                invalid[2]["tick"] = json!(1);
                invalid[3]["tick"] = json!(1);
            }
            "descending" => {
                invalid[0]["tick"] = json!(2);
                invalid[1]["tick"] = json!(2);
                invalid[2]["tick"] = json!(1);
                invalid[3]["tick"] = json!(1);
            }
            _ => unreachable!(),
        }
        assert!(Replay::engineering(&invalid).is_err(), "{kind}");
    }
    for bits in [
        "7ff8000000000000",
        "7ff0000000000000",
        "fff0000000000000",
        "8000000000000000",
        "3ff0000000000000",
        "not hexadecimal",
        "",
    ] {
        let mut invalid = entries.clone();
        invalid[0]["bits"] = json!(bits);
        assert!(
            Replay::engineering(&invalid).is_err(),
            "nonconstant/nonfinite bits: {bits}"
        );
    }
    let replay = Replay::engineering(&entries).unwrap();
    // Math.random does not replace the independent seeded environment generator.
    let before = replay.receipt();
    let mut actual_seed = 739;
    let mut ordinary_seed = actual_seed;
    let ordinary = crate::weather_runtime::environment_random(&mut ordinary_seed);
    let actual = super::environment_draw(
        &mut actual_seed,
        Some(&replay),
        json!(["weather.wind.angle", 1]),
    );
    assert_eq!(actual.to_bits(), ordinary.to_bits());
    assert_eq!(actual_seed, ordinary_seed);
    assert_eq!(replay.receipt(), before);
    for tick in [-1, 0, 2, 201] {
        assert!(replay.engineering_begin(tick).is_err());
    }
    for tick in [0, 1] {
        assert!(replay.engineering_end(tick).is_err());
    }
    assert!(
        replay
            .engineering_draw(StreamId::OffMap, Some("along"))
            .is_err()
    );
    assert!(replay.engineering_finish().is_err());
    assert_eq!(
        replay.receipt(),
        before,
        "out-of-window calls do not mutate the scope"
    );
    for tick in 1..=200 {
        replay.engineering_begin(tick).unwrap();
        let before = replay.receipt();
        assert!(replay.engineering_begin(tick).is_err(), "duplicate begin");
        assert!(
            replay.engineering_begin(tick + 1).is_err(),
            "begin requires previous end even without draws"
        );
        assert!(
            replay.engineering_end(tick + 1).is_err(),
            "end must match its active tick"
        );
        if tick <= 10 {
            for purpose in [StreamId::Terrain, StreamId::Combat, StreamId::Weather] {
                assert!(replay.engineering_draw(purpose, Some("along")).is_err());
            }
            for axis in [Some("side"), Some("unrelated"), None] {
                assert!(replay.engineering_draw(StreamId::OffMap, axis).is_err());
            }
            assert!(
                replay.engineering_end(tick).is_err(),
                "missing complete shell"
            );
            assert_eq!(replay.receipt(), before, "invalid calls consume nothing");
            // Exercise the same dispatch entry point as MatchRandom with a wrong caller.
            for purpose in [StreamId::Combat, StreamId::OffMap] {
                let refused = std::panic::catch_unwind(|| {
                    replay.fraction(purpose, std::panic::Location::caller())
                });
                assert!(
                    refused.is_err(),
                    "unmapped real fraction entry point must refuse"
                );
                assert_eq!(
                    replay.receipt(),
                    before,
                    "wrapper rejection preserves the tape"
                );
            }
            assert_eq!(
                replay
                    .engineering_draw(StreamId::OffMap, Some("along"))
                    .unwrap()
                    .to_bits(),
                0.5_f64.to_bits()
            );
            let half = replay.receipt();
            assert!(
                replay
                    .engineering_draw(StreamId::OffMap, Some("along"))
                    .is_err(),
                "reversed/duplicate axis"
            );
            assert!(replay.engineering_end(tick).is_err(), "missing side");
            assert_eq!(replay.receipt(), half);
            assert_eq!(
                replay
                    .engineering_draw(StreamId::OffMap, Some("side"))
                    .unwrap()
                    .to_bits(),
                0.5_f64.to_bits()
            );
        }
        let consumed = replay.receipt();
        let ordinary = crate::weather_runtime::environment_random(&mut ordinary_seed);
        let actual = super::environment_draw(
            &mut actual_seed,
            Some(&replay),
            json!(["weather.wind.angle", tick]),
        );
        assert_eq!(actual.to_bits(), ordinary.to_bits());
        assert_eq!(actual_seed, ordinary_seed);
        assert_eq!(
            replay.receipt(),
            consumed,
            "active engineering intervals retain independent seeded weather"
        );
        assert!(
            replay
                .engineering_draw(StreamId::OffMap, Some("along"))
                .is_err(),
            "future shell or exhausted tape"
        );
        assert!(
            replay.engineering_finish().is_err(),
            "all200 intervals must end, including final active tick"
        );
        assert_eq!(replay.receipt(), consumed);
        replay.engineering_end(tick).unwrap();
        let ended = replay.receipt();
        assert!(replay.engineering_end(tick).is_err(), "duplicate end");
        assert!(
            replay
                .engineering_draw(StreamId::OffMap, Some("along"))
                .is_err(),
            "draw after interval end"
        );
        if tick < 200 {
            assert!(
                replay.engineering_finish().is_err(),
                "premature finish after exhaustion"
            );
        }
        assert_eq!(replay.receipt(), ended);
    }
    assert_eq!(replay.receipt()["consumed"], 20);
    assert_eq!(replay.receipt()["unconsumed"], 0);
    assert_eq!(replay.receipt()["exhausted"], true);
    replay.engineering_finish().unwrap();
    let closed = replay.receipt();
    assert!(replay.engineering_begin(201).is_err());
    assert!(replay.engineering_end(200).is_err());
    assert!(replay.engineering_finish().is_err(), "duplicate finish");
    assert!(
        replay
            .engineering_draw(StreamId::OffMap, Some("along"))
            .is_err()
    );
    assert_eq!(replay.receipt(), closed);
    assert_eq!(closed["engineering"]["closed"], true);
    assert_eq!(closed["engineering"]["active"], false);
    for (index, draw) in closed["draws"].as_array().unwrap().iter().enumerate() {
        assert_eq!(draw["index"], index);
        assert_eq!(draw["bits"], entries[index]["bits"]);
        assert_eq!(draw["sourceSite"], entries[index]["site"]);
        assert_eq!(
            draw["consumer"],
            json!([
                "hedge.artillery",
                entries[index]["axis"],
                entries[index]["tick"]
            ])
        );
    }
    for (file, line) in [
        ("unexpected.rs", 1),
        ("match_support.rs", 0),
        ("match_support.rs", 1),
        ("match_support.rs", u32::MAX),
    ] {
        assert_eq!(role(file, line), None);
    }
    let source = include_str!("match_support.rs");
    for axis in ["along", "side"] {
        let line = source
            .lines()
            .position(|line| {
                line.contains(&format!(
                    "let {axis} = (self.start.random.next_fraction(StreamId::OffMap)"
                ))
            })
            .unwrap();
        assert_eq!(role("match_support.rs", line as u32 + 1), Some(axis));
    }
}

#[path = "engineering_hedge_consumer.rs"]
mod consumer;
