//! `simtest <state.json> [--tables tables.json] [--threads] [--bench N]`
//!
//! Prints the lines `node harness/vision-check.mjs <state.json>` prints, from the Rust core; the two must be
//! identical. The rules come from the state file's `tables` if it has them, else from `--tables`, else from
//! `tables.json` beside the state file (`harness/tables.mjs` writes it to `harness/out/`). `--threads` works
//! the fog out one team per thread. `--bench N` adds timings, as `vision-check.mjs --bench N` does.
//!
//! `simtest --replay <trace.json>`
//!
//! Replays a recording from `harness/movement-trace.mjs` call by call against the movement port and prints
//! the first mismatch, or how much matched (`replay.rs`). Fails on a mismatch.
//!
//! `simtest --workload <state.json> [--ticks N] [--threads] [--tables tables.json]`
//!
//! Runs the Rust side of the frozen workload (`workload.rs`) and prints a RESULT line shaped like
//! `harness/sim-workload.mjs`'s: mean and p95 milliseconds per tick for the step, movement, separation, the
//! vision pass and the fog pass (the last two on vision ticks). `--threads` works the vision and fog passes
//! out one team per thread.

use std::path::Path;
use std::process::ExitCode;

use serde_json::{Value, json};
use simtest::check::{bench, check};
use simtest::load::{load_game, load_rules};
use simtest::replay::replay;
use simtest::workload::{run as run_workload, stats};

fn read(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
}

/// The rules for a state file: its own `tables`, `--tables`, or `tables.json` beside it.
fn tables_for(state: &Value, file: &str, tables: Option<&String>) -> Result<Value, String> {
    match (state.get("tables"), tables) {
        (Some(t), None) => Ok(t.clone()),
        (_, Some(path)) => simtest::parse(&read(Path::new(path))?),
        (None, None) => simtest::parse(&read(&Path::new(file).with_file_name("tables.json"))?),
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let opt = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
    };
    let threads = args.iter().any(|a| a == "--threads");
    if let Some(file) = opt("--replay") {
        let trace = simtest::parse(&read(Path::new(file))?)?;
        let report = replay(&trace)?;
        if !report.outside.is_empty() {
            let fields: Vec<String> = report
                .outside
                .iter()
                .map(|(k, n)| format!("{k} {n}"))
                .collect();
            println!(
                "changed between a unit's movement block and separation by code not ported: {}",
                fields.join(", ")
            );
        }
        println!(
            "replayed {} ticks: {} movement calls, {} unit states after separation and progress",
            report.ticks, report.calls, report.units
        );
        return match report.mismatch {
            Some(m) => Err(format!("first mismatch: {m}")),
            None => {
                println!("no mismatch");
                Ok(())
            }
        };
    }
    if let Some(file) = opt("--workload") {
        let state = simtest::parse(&read(Path::new(file))?)?;
        let rules = load_rules(&tables_for(&state, file, opt("--tables"))?)?;
        let mut loaded = load_game(
            state.get("game").ok_or("the state file has no game")?,
            &rules,
        )?;
        drop(state);
        let ticks: usize = opt("--ticks")
            .map_or(Ok(200), |t| t.parse())
            .map_err(|_| "--ticks takes a number")?;
        let units = loaded.game.units.len();
        let w = run_workload(&mut loaded.game, &mut loaded.fog, &rules, ticks, threads);
        let part = |values: &[f64]| {
            let (mean, p95) = stats(values);
            json!({ "mean": (mean * 1000.0).round() / 1000.0, "p95": (p95 * 1000.0).round() / 1000.0 })
        };
        let result = json!({
            "state": file, "units": units, "ticks": ticks, "rust": "1.99.0", "threads": threads,
            "cores": std::thread::available_parallelism().map_or(1, |n| n.get()),
            "ms": {
                "step": part(&w.step), "move": part(&w.movement), "separate": part(&w.separate),
                "vision": part(&w.vision), "fog": part(&w.fog),
            },
            "hashes": { "units": w.units_hash, "fog": w.fog_hash },
        });
        println!("RESULT {result}");
        return Ok(());
    }
    let file = args
        .first()
        .filter(|a| !a.starts_with("--"))
        .ok_or("usage: simtest <state.json> [--tables tables.json] [--threads] [--bench N] | --replay <trace.json> | --workload <state.json> [--ticks N] [--threads]")?;
    let state = simtest::parse(&read(Path::new(file))?)?;
    let rules = load_rules(&tables_for(&state, file, opt("--tables"))?)?;
    let mut loaded = load_game(
        state.get("game").ok_or("the state file has no game")?,
        &rules,
    )?;
    drop(state);
    for line in check(&mut loaded.game, &mut loaded.fog, &rules, threads) {
        println!("{line}");
    }
    if let Some(n) = opt("--bench") {
        let n: usize = n.parse().map_err(|_| "--bench takes a number of passes")?;
        for line in bench(&mut loaded.game, &mut loaded.fog, &rules, n) {
            println!("{line}");
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("simtest: {e}");
            ExitCode::FAILURE
        }
    }
}
