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

#[test]
fn every_recorded_message_matches_the_pinned_browser_view() {
    use std::io::{BufRead, BufReader};
    use ww2_core::client_view::ClientView;
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../core/tests/fixtures");
    for mode in ["conquest", "world"] {
        for slot in 0..2 {
            let stem = format!("protocol-{mode}-p{slot}");
            let open = |suffix: &str| {
                let file = std::fs::File::open(fixtures.join(format!("{stem}{suffix}"))).unwrap();
                BufReader::new(flate2::read::MultiGzDecoder::new(file)).lines()
            };
            let mut expected = open(".expected.jsonl.gz");
            let mut view = ClientView::new();
            let mut lines = 0;
            for (index, raw) in open(".jsonl.gz").enumerate() {
                let raw = raw.unwrap();
                view.apply(&raw)
                    .unwrap_or_else(|e| panic!("{stem}:{}: {e}", index + 1));
                let want = expected
                    .next()
                    .unwrap_or_else(|| panic!("{stem}: missing expected line {}", index + 1))
                    .unwrap();
                assert_eq!(simtest::view::report(&view), want, "{stem}:{}", index + 1);
                lines += 1;
            }
            assert!(lines > 0, "{stem}: empty recording");
            assert!(
                expected.next().is_none(),
                "{stem}: unconsumed expected lines"
            );
        }
    }
}
