use super::*;
use std::path::PathBuf;
use ww2_core::client_view::ClientView;

struct TestDir(PathBuf);
impl TestDir {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        Self(std::env::temp_dir().join(format!(
            "ww2-net97-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )))
    }
}
impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn drain(
    hub: &mut Hub,
    receiver: &mut mpsc::Receiver<Output>,
    now: u64,
    connection: &Connection,
    view: &mut ClientView,
) -> usize {
    let mut frames = 0;
    while let Ok(output) = receiver.try_recv() {
        connection.complete_bytes(output.diagnostic_bytes());
        match output {
            Output::Snapshot(write) => {
                let message: Value = serde_json::from_str(&write.json).unwrap();
                assert!(message.get("diagnostics").is_none() && message.get("client").is_none());
                view.apply(&message.to_string()).unwrap();
                hub.complete_snapshot(&write, true, now);
                frames += 1;
            }
            Output::Json(message) if message["t"] == "start" => {
                view.apply(&message.to_string()).unwrap();
            }
            Output::Json(_) | Output::Watcher(_) => {}
            Output::Close(code, reason) => panic!("unexpected close {code}: {reason}"),
        }
    }
    frames
}

#[test]
fn net97_r1_ping_diagnostics_write_jsonl_and_keep_both_snapshot_streams_delivering() {
    let dir = TestDir::new();
    let mut hub = Hub::new(32, String::new());
    hub.configure_trusted_room(Some(crate::TrustedRoomConfig::new("ABC", 3936341).unwrap()));
    hub.configure_diagnostics(Some(dir.0.clone()), true)
        .unwrap();
    let mut join = |id, token| {
        let (tx, rx) = mpsc::channel(64);
        let (cancel, _) = watch::channel(false);
        let mut c = Connection::new(id, tx, cancel);
        c.enable_diagnostics();
        hub.message(
            "ABC",
            &c,
            &mut false,
            json!({"t":"hello","name":token,"token":token}),
            0,
        );
        (c, rx)
    };
    let (a, mut ra) = join(1, "a");
    let (b, mut rb) = join(2, "b");
    hub.start_trusted("ABC", a.id, false, 0).unwrap();
    let mut va = ClientView::new();
    let mut vb = ClientView::new();
    drain(&mut hub, &mut ra, 0, &a, &mut va);
    drain(&mut hub, &mut rb, 0, &b, &mut vb);
    hub.message(
        "ABC",
        &a,
        &mut true,
        json!({"t":"ping","c":7,"rtt":22.6,"d":[-5,2000000,4.5]}),
        0,
    );
    let mut counts = [0, 0];
    for now in (0..=5050).step_by(10) {
        hub.poll_native(now, now * 1_000_000);
        counts[0] += drain(&mut hub, &mut ra, now, &a, &mut va);
        counts[1] += drain(&mut hub, &mut rb, now, &b, &mut vb);
    }
    assert!(counts.into_iter().all(|n| n >= 40));
    assert_eq!(hub.native_status("ABC").unwrap().fault, None);
    hub.diagnostics.as_ref().unwrap().flush();
    let data = std::fs::read_to_string(dir.0.join("diag-ABC.jsonl")).unwrap();
    let lines = data
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(lines.len(), 1);
    let line = &lines[0];
    assert_eq!(line["tps"], 20.0);
    assert!(line["tick"].as_i64().unwrap() >= 100);
    assert!(line["units"].as_u64().unwrap() > 0);
    assert!(line["tickP95"].as_f64().unwrap() >= line["tickP50"].as_f64().unwrap());
    assert_eq!(line["snapEvery"], 2);
    assert_eq!(line["loopP99"], 0.0);
    assert_eq!(line["lateMax"], 0.0);
    assert_eq!(line["players"][0]["rtt"], 23.0);
    assert_eq!(line["players"][0]["quiet"], 5000);
    assert_eq!(line["players"][0]["client"]["fps"], 0.0);
    assert_eq!(line["players"][0]["client"]["worst"], 1000000.0);
    assert_eq!(line["players"][0]["client"]["cpu"], 4.5);
    assert!(line["players"][0]["bufMax"].as_u64().unwrap() > 0);
    assert_eq!(line["players"][1]["client"], Value::Null);
    assert_eq!(line["players"][1]["quiet"], Value::Null);
    assert!(line["t"].as_u64().unwrap() > 0);
    assert!(!data.contains("3936341") && !data.contains("token"));
}

#[test]
fn net97_writer_records_scheduler_delay_and_confines_paths_without_faulting_on_io_error() {
    let dir = TestDir::new();
    let mut recorder = crate::diagnostics::Recorder::new(dir.0.clone()).unwrap();
    recorder.observe_poll(0, Some(0));
    recorder.observe_poll(40_000_000, Some(30_000_000));
    recorder.record("ABC", json!({"tick":1}));
    recorder.record("../escape", json!({"tick":2}));
    recorder.flush();
    let line: Value = serde_json::from_str(
        std::fs::read_to_string(dir.0.join("diag-ABC.jsonl"))
            .unwrap()
            .trim(),
    )
    .unwrap();
    assert_eq!(line["lateMax"], 30.0);
    assert_eq!(line["loopP99"], 30.0);
    assert_eq!(line["loopMax"], 30.0);
    assert_eq!(std::fs::read_dir(&dir.0).unwrap().count(), 1);
    let blocked = dir.0.join("file");
    std::fs::write(&blocked, b"not a directory").unwrap();
    let mut recorder = crate::diagnostics::Recorder::new(blocked).unwrap();
    recorder.record("ABC", json!({"tick":1}));
    recorder.flush();
}

#[test]
fn net97_optional_socket_gauge_counts_utf8_for_all_outputs_and_rolls_back_failed_enqueues() {
    let (tx, mut rx) = mpsc::channel(3);
    let (cancel, _) = watch::channel(false);
    let mut connection = Connection::new(1, tx, cancel);
    connection.enable_diagnostics();
    let clone = connection.clone();
    for message in [
        json!({"t":"start","name":"雪é"}),
        json!({"t":"lobby"}),
        json!({"t":"pong","c":1}),
    ] {
        connection.send(message);
    }
    let queued = clone.buffered_bytes();
    let expected = [
        json!({"t":"start","name":"雪é"}),
        json!({"t":"lobby"}),
        json!({"t":"pong","c":1}),
    ]
    .iter()
    .map(|v| v.to_string().len() as u64)
    .sum::<u64>();
    assert_eq!(queued, expected);
    assert!(
        json!({"t":"start","name":"雪é"}).to_string().len()
            > json!({"t":"start","name":"雪é"})
                .to_string()
                .encode_utf16()
                .count()
    );
    connection.send(json!({"t":"overflow"}));
    assert_eq!(clone.buffered_bytes(), queued);
    let mut exact = 0;
    while let Ok(output) = rx.try_recv() {
        let bytes = output.diagnostic_bytes();
        exact += bytes;
        clone.complete_bytes(bytes);
    }
    assert_eq!(exact, queued);
    assert_eq!(connection.buffered_bytes(), 0);
    let (tx, mut rx) = mpsc::channel(1);
    let (cancel, _) = watch::channel(false);
    let disabled = Connection::new(2, tx, cancel);
    disabled.send(json!({"t":"pong"}));
    assert_eq!(disabled.buffered_bytes(), 0);
    assert!(matches!(rx.try_recv(), Ok(Output::Json(_))));
}
