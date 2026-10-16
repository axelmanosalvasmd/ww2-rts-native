use super::*;
use crate::tick_timing::{TickMeter, TickSample};

#[test]
fn net97_browser_metrics_match_finite_clamp_and_missing_field_rules() {
    for invalid in [Value::Null, json!({}), json!("bad"), json!(7)] {
        assert_eq!(clean_client(&invalid), Value::Null);
    }
    assert_eq!(
        clean_client(&json!([-2, 2000000, 4.5, null, "5", true])),
        json!({
            "fps":0.0,"worst":1000000.0,"cpu":4.5,"snaps":null,"gap":null,"kbs":null,
            "ping":null,"jitter":null,"loss":null,"mem":null,"units":null,"fx":null
        })
    );
    let all = clean_client(&json!([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 999]));
    assert_eq!(all.as_object().unwrap().len(), 12);
    for (i, key) in CLIENT_KEYS.iter().enumerate() {
        assert_eq!(all[*key], json!((i + 1) as f64));
    }
}

#[test]
fn net97_windows_reset_after_pause_and_phase_logs_wait_thirty_seconds() {
    let mut room = RoomDiagnostics::default();
    assert_eq!(room.due(0, 0), None);
    assert_eq!(room.due(4999, 99), None);
    assert_eq!(room.due(5000, 100), Some(20.0));
    assert_eq!(room.due(13000, 101), None);
    assert_eq!(room.due(17999, 200), None);
    assert_eq!(room.due(18000, 201), Some(20.0));
    let mut other_room = RoomDiagnostics::default();
    assert_eq!(other_room.due(18000, 0), None);
    assert_eq!(room.due(23000, 301), Some(20.0));
    assert_eq!(other_room.due(23000, 75), Some(15.0));
    let mut meter = TickMeter::new(0).unwrap();
    meter
        .record(
            TickSample {
                durations: [12.0, 9.0, 0.0, 3.0, 2.0, 1.0],
                sent: true,
            },
            0,
        )
        .unwrap();
    let stats = meter.stats();
    assert!(!room.phase_due(0));
    assert!(!room.phase_due(29999));
    assert!(room.phase_due(30000));
    let line = phase_log("ABC", &stats, 2);
    assert!(line.contains("step 9.00/9.00 ms") && line.contains("snapshotStringify 1.00/1.00 ms"));
    assert!(line.ends_with("snapshots every 2 ticks"));
    assert!(!room.phase_due(30001));
    assert!(room.phase_due(60000));
    assert!(phase_log("ABC", &stats, 3).ends_with("snapshots every 3 ticks"));
}
