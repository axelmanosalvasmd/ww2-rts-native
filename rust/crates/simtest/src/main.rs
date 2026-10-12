//! `simtest <state.json> [--tables tables.json] [--threads] [--bench N]`
//!
//! Prints the lines `node harness/vision-check.mjs <state.json>` prints, from the Rust core; the two must be
//! identical. The rules come from the state file's `tables` if it has them, else from `--tables`, else from
//! `tables.json` beside the state file (`harness/tables.mjs` writes it to `harness/out/`). `--threads` works
//! the fog out one team per thread. `--bench N` adds timings, as `vision-check.mjs --bench N` does.

use std::path::Path;
use std::process::ExitCode;

use simtest::check::{bench, check};
use simtest::load::{load_game, load_rules};

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let opt = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
    };
    let file = args
        .first()
        .filter(|a| !a.starts_with("--"))
        .ok_or("usage: simtest <state.json> [--tables tables.json] [--threads] [--bench N]")?;
    let read =
        |path: &Path| std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()));
    let state = simtest::parse(&read(Path::new(file))?)?;
    let tables = match (state.get("tables"), opt("--tables")) {
        (Some(t), None) => t.clone(),
        (_, Some(path)) => simtest::parse(&read(Path::new(path))?)?,
        (None, None) => simtest::parse(&read(&Path::new(file).with_file_name("tables.json"))?)?,
    };
    let rules = load_rules(&tables)?;
    let mut loaded = load_game(
        state.get("game").ok_or("the state file has no game")?,
        &rules,
    )?;
    drop(state);
    for line in check(
        &mut loaded.game,
        &mut loaded.fog,
        &rules,
        args.iter().any(|a| a == "--threads"),
    ) {
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
