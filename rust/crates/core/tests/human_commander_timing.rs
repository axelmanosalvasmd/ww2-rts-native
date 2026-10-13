use serde_json::Value;
use ww2_core::human_commander_timing::{Skill, Timing, TimingError, needs_view, ticks};

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/human-commander-timing.json")).unwrap()
}

#[test]
fn private_human_timings_match_original_source() {
    let fixture = fixture();
    for case in fixture["componentCases"].as_array().unwrap() {
        let skill = Skill::for_level(case["level"].as_str().unwrap());
        let seed = case["seed"].as_u64().unwrap() as u32;
        let slot = case["slot"].as_u64().unwrap() as u32;
        let mut sampled = Timing::new(seed, slot);
        let mut delayed = sampled.clone();
        for step in case["steps"].as_array().unwrap() {
            let kind = step["kind"].as_str().unwrap();
            let (median, spread, scale, floor) = match kind {
                "opening" => (skill.opening, 0.2, 1.0, 0.0),
                "reaction" => (
                    if step["onScreen"].as_bool().unwrap() {
                        skill.react
                    } else {
                        skill.notice
                    },
                    0.35,
                    1.0,
                    0.2,
                ),
                "input" => (skill.input, 0.35, 1.0, 0.0),
                "scan" => (
                    skill.scan,
                    0.5,
                    1.0 + 3.0 * step["load"].as_f64().unwrap(),
                    0.0,
                ),
                _ => panic!("unknown fixture kind"),
            };
            let seconds = sampled.sample(median, spread).unwrap() * scale;
            let seconds = if seconds < floor { floor } else { seconds };
            assert_eq!(
                format!("{:016x}", seconds.to_bits()),
                step["secondsBits"].as_str().unwrap(),
                "{case} {step}"
            );
            let expected = step["ticks"].as_u64().unwrap();
            assert_eq!(ticks(seconds).unwrap(), expected);
            let actual = match kind {
                "opening" => delayed.opening_delay(skill),
                "reaction" => delayed.reaction_delay(skill, step["onScreen"].as_bool().unwrap()),
                "input" => delayed.input_delay(skill),
                "scan" => delayed.scan_delay(skill, step["load"].as_f64().unwrap()),
                _ => unreachable!(),
            }
            .unwrap();
            assert_eq!(actual, expected);
        }
        assert_eq!(sampled, delayed);
        assert_eq!(sampled.draws(), 14);
    }
}

#[test]
fn invalid_timing_is_atomic_and_view_demand_preserves_source_boundary() {
    let mut timing = Timing::new(7, 1);
    let before = timing.clone();
    assert_eq!(
        timing.sample(f64::NAN, 0.35),
        Err(TimingError::InvalidSample)
    );
    assert_eq!(
        timing.scan_delay(Skill::for_level("normal"), f64::INFINITY),
        Err(TimingError::InvalidSample)
    );
    assert_eq!(timing, before);
    assert!(needs_view(false, None, false, 0));
    assert!(!needs_view(true, Some(51), false, 49));
    assert!(needs_view(true, Some(51), false, 50));
    assert!(needs_view(true, None, true, 0));
    assert!(!needs_view(true, None, false, 100));
    assert_eq!(ticks(0.025), Ok(1));
    assert_eq!(ticks(0.075), Ok(2));
    assert_eq!(Skill::for_level("unknown"), Skill::for_level("normal"));
}
