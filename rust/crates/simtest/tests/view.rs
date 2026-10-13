//! Exercise the command-line replay reader with a complete gzip stream and failure handling.
use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn view_replay_accepts_gzip_and_stdin_and_rejects_bad_input() {
    let text =
        b"{\"t\":\"start\",\"map\":{\"w\":1,\"h\":1}}\n{\"t\":\"s\",\"units\":[],\"held\":[0,0]}\n";
    let path = std::env::temp_dir().join(format!("ww2-view-{}.jsonl.gz", std::process::id()));
    let mut encoded = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoded.write_all(text).unwrap();
    std::fs::write(&path, encoded.finish().unwrap()).unwrap();
    let gz = Command::new(env!("CARGO_BIN_EXE_simtest"))
        .arg("--view")
        .arg(&path)
        .output()
        .unwrap();
    std::fs::remove_file(path).unwrap();
    assert!(
        gz.status.success(),
        "{}",
        String::from_utf8_lossy(&gz.stderr)
    );
    assert_eq!(gz.stdout.iter().filter(|b| **b == b'\n').count(), 2);
    let mut stdin = Command::new(env!("CARGO_BIN_EXE_simtest"))
        .args(["--view", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    stdin.stdin.take().unwrap().write_all(text).unwrap();
    let plain = stdin.wait_with_output().unwrap();
    assert!(plain.status.success());
    assert_eq!(plain.stdout, gz.stdout);
    let mut bad = Command::new(env!("CARGO_BIN_EXE_simtest"))
        .args(["--view", "-"])
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    bad.stdin
        .take()
        .unwrap()
        .write_all(b"{\"t\":\"s\",\"units\":[]}\n")
        .unwrap();
    let bad = bad.wait_with_output().unwrap();
    assert!(!bad.status.success());
    assert!(String::from_utf8_lossy(&bad.stderr).contains(":1:"));
}
