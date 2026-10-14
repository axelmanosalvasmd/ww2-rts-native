//! Produce gameplay terrain evidence with existing core APIs, outside the rules core.
use serde_json::{Value, json};
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

const COMPILED_COMMIT: &str = env!("WW2_TERRAIN_COMPILED_COMMIT");
const COMPILED_CLEAN: &str = env!("WW2_TERRAIN_COMPILED_CLEAN");
const REFERENCE: &str = "e4e579a838aabae3b1d9dadbc17a0a87147bc52d";

fn run() -> Result<(), String> {
    let mut input = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let mut output = None;
    let mut producing_commit = None;
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        let value = args
            .next()
            .ok_or_else(|| format!("missing value for {arg}"))?;
        match arg.as_str() {
            "--input-root" => input = PathBuf::from(value),
            "--out" => output = Some(PathBuf::from(value)),
            "--producing-commit" => producing_commit = Some(value),
            _ => return Err(format!("unknown argument {arg}")),
        }
    }
    let output = output.ok_or("usage: terrain-contract-export --out NEW_DIRECTORY --producing-commit SHA [--input-root REPOSITORY]")?;
    let commit = producing_commit.ok_or("missing --producing-commit")?;
    if commit.len() != 40 || !commit.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("producing commit must be a complete hexadecimal Git identity".into());
    }
    if COMPILED_COMMIT != commit || COMPILED_CLEAN != "true" {
        return Err("use an executable built from this exact clean producing commit".into());
    }
    if git(&input, &["rev-parse", "HEAD"])? != commit
        || !git(&input, &["status", "--porcelain", "--untracked-files=no"])?.is_empty()
        || !git(
            &input,
            &[
                "status",
                "--porcelain",
                "--untracked-files=all",
                "--",
                "rust/crates",
            ],
        )?
        .is_empty()
    {
        return Err("use the clean producing checkout and its exact HEAD".into());
    }
    if output.exists() {
        return Err("output directory must be new; existing evidence is preserved".into());
    }
    let fixture_root = input.join("docs/fixtures/terrain");
    let mut paths = fs::read_dir(fixture_root.join("source-authored"))
        .map_err(|e| e.to_string())?
        .map(|entry| entry.map(|e| e.path()).map_err(|e| e.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    paths.retain(|path| path.extension().is_some_and(|e| e == "json"));
    paths.sort();
    if paths.len() != 33 {
        return Err("expected all 33 authored Source inputs".into());
    }
    // Prepare every core result before writing an artifact. Source expected
    // heights and scouting rows never become inputs to the native producers.
    let mut authored = Vec::with_capacity(paths.len());
    for path in paths {
        let source = read(&path)?;
        let key = source["mapKey"].as_str().ok_or("missing map key")?;
        if source["schemaVersion"] != 1
            || source["producer"]["referenceSha"] != REFERENCE
            || source["producer"]["kind"] != "original-javascript"
            || source["points"]
                .as_array()
                .is_none_or(|points| points.len() != 1000)
            || path.file_name().and_then(|name| name.to_str())
                != Some(format!("{key}.json").as_str())
        {
            return Err("authored input does not match the maintained fixture corpus".into());
        }
        let points = source["points"]
            .as_array()
            .ok_or("missing sample points")?
            .iter()
            .map(|row| {
                Ok((
                    row[0].as_f64().ok_or("invalid sample x")?,
                    row[1].as_f64().ok_or("invalid sample z")?,
                ))
            })
            .collect::<Result<Vec<_>, String>>()?;
        let mut result = simtest::terrain_fixtures::produce(key, &source["map"], &points)?;
        result["producer"] = producer(&commit);
        authored.push((format!("{key}.json"), result));
    }
    let scout = read(&fixture_root.join("source-world-scout.json"))?;
    if scout["schema"] != 1
        || scout["source"]["pin"] != REFERENCE
        || scout["source"]["node"] != "v24.21.0"
        || scout["source"]["arch"] != "x64"
    {
        return Err("scouting input does not match the maintained Source provenance".into());
    }
    let mut scout_result = simtest::terrain_scout::replay(&scout)?;
    scout_result["producer"] = producer(&commit);
    fs::create_dir_all(&output).map_err(|e| e.to_string())?;
    for (name, result) in &authored {
        write(&output.join(name), result)?;
    }
    write(&output.join("world-scout.json"), &scout_result)?;
    write(
        &output.join("manifest.json"),
        &json!({
            "schemaVersion":1,
            "producer":producer(&commit),
            "authoredMaps":authored.iter().map(|(name, _)|name).collect::<Vec<_>>(),
            "pointsPerMap":1000,
            "worldScouting":"world-scout.json",
            "units":{"x":"metres east","z":"metres south","rawLevel":"integer level","levelMetres":"rawLevel * 2.5","heightAtMetres":"bilinear gameplay height"},
            "scope":"Gameplay queries and recorded recipient terrain rows. No rendered mesh, whole World tick, transport or ABI adoption."
        }),
    )?;
    println!(
        "exported 33 core-authored height files and canonical World scouting rows at {commit}"
    );
    Ok(())
}
fn producer(commit: &str) -> Value {
    json!({"kind":"native-rust-core","producingCommit":commit,"referenceSha":REFERENCE})
}
fn git(root: &Path, args: &[&str]) -> Result<String, String> {
    let result = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .map_err(|e| e.to_string())?;
    if !result.status.success() {
        return Err("cannot verify producing checkout".into());
    }
    String::from_utf8(result.stdout)
        .map(|s| s.trim().to_owned())
        .map_err(|e| e.to_string())
}
fn read(path: &Path) -> Result<Value, String> {
    serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}
fn write(path: &Path, value: &Value) -> Result<(), String> {
    let mut bytes = serde_json::to_vec(value).map_err(|e| e.to_string())?;
    bytes.push(b'\n');
    fs::write(path, bytes).map_err(|e| e.to_string())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
