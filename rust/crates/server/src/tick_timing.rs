//! Pure host timing inputs. Snapshot cadence never changes the recorded AI schedule.

pub const TICK_NS: u64 = 50_000_000;
pub const PHASES: [&str; 6] = [
    "tick",
    "step",
    "think",
    "snapshot",
    "snapshotBuild",
    "snapshotStringify",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimingError {
    InvalidDuration,
    Capacity,
}
impl std::fmt::Display for TimingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidDuration => "tick durations must be finite and nonnegative",
            Self::Capacity => "host timing capacity exhausted",
        })
    }
}
impl std::error::Error for TimingError {}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Percentiles {
    pub p50: f64,
    pub p95: f64,
}
#[derive(Clone, Debug, PartialEq)]
struct Ring {
    values: Vec<f64>,
    next: usize,
    count: usize,
}
impl Ring {
    fn new(size: usize) -> Self {
        Self {
            values: vec![0.0; size],
            next: 0,
            count: 0,
        }
    }
    fn add(&mut self, value: f64) {
        self.values[self.next] = value;
        self.next = (self.next + 1) % self.values.len();
        self.count = (self.count + 1).min(self.values.len());
    }
    fn percentiles(&self) -> Percentiles {
        if self.count == 0 {
            return Percentiles::default();
        }
        let mut values = self.values[..self.count].to_vec();
        values.sort_by(f64::total_cmp);
        Percentiles {
            p50: values[(self.count * 50).div_ceil(100) - 1],
            p95: values[(self.count * 95).div_ceil(100) - 1],
        }
    }
}

/// Durations are milliseconds in PHASES order. Missing source phases use zero.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TickSample {
    pub durations: [f64; 6],
    /// A scheduled snapshot tick, including one with no eligible sockets.
    pub sent: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub struct TimingStats {
    pub phases: [Percentiles; 6],
    pub snapshot_tick: Percentiles,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TickMeter {
    phases: [Ring; 6],
    snapshot_ticks: Ring,
    snap_every: u8,
    next_check: u64,
    low_since: Option<u64>,
}
impl TickMeter {
    pub fn new(now_ms: u64) -> Result<Self, TimingError> {
        Ok(Self {
            phases: std::array::from_fn(|_| Ring::new(200)),
            snapshot_ticks: Ring::new(50),
            snap_every: 2,
            next_check: now_ms.checked_add(1000).ok_or(TimingError::Capacity)?,
            low_since: None,
        })
    }
    pub fn snap_every(&self) -> u8 {
        self.snap_every
    }
    pub fn next_check(&self) -> u64 {
        self.next_check
    }
    pub fn low_since(&self) -> Option<u64> {
        self.low_since
    }
    pub fn should_snapshot(&self, tick: u64, winner_decided: bool) -> bool {
        winner_decided || tick.is_multiple_of(u64::from(self.snap_every))
    }
    pub fn stats(&self) -> TimingStats {
        TimingStats {
            phases: std::array::from_fn(|i| self.phases[i].percentiles()),
            snapshot_tick: self.snapshot_ticks.percentiles(),
        }
    }
    /// Record all work and adapt only from the last 50 snapshot tick durations.
    /// Validation and deadline capacity checks precede any history changes.
    pub fn record(&mut self, sample: TickSample, now_ms: u64) -> Result<u8, TimingError> {
        if sample
            .durations
            .iter()
            .any(|value| !value.is_finite() || *value < 0.0)
        {
            return Err(TimingError::InvalidDuration);
        }
        let count = (self.snapshot_ticks.count + usize::from(sample.sent)).min(50);
        let ready = now_ms >= self.next_check && count >= 10;
        let next_check = if ready {
            now_ms.checked_add(1000).ok_or(TimingError::Capacity)?
        } else {
            self.next_check
        };
        let mut snap_every = self.snap_every;
        let mut low_since = self.low_since;
        if ready {
            let p95 = if sample.sent {
                let mut preview = self.snapshot_ticks.clone();
                preview.add(sample.durations[0]);
                preview.percentiles().p95
            } else {
                self.snapshot_ticks.percentiles().p95
            };
            if p95 > 40.0 {
                snap_every = (snap_every + 1).min(4);
                low_since = None;
            } else if p95 < 24.0 {
                let since = *low_since.get_or_insert(now_ms);
                if snap_every > 2 && now_ms.saturating_sub(since) >= 10_000 {
                    snap_every -= 1;
                    low_since = Some(now_ms);
                }
            } else {
                low_since = None;
            }
        }
        for (i, value) in sample.durations.into_iter().enumerate() {
            if i < 3 || sample.sent {
                self.phases[i].add(value);
            }
        }
        if sample.sent {
            self.snapshot_ticks.add(sample.durations[0]);
        }
        self.next_check = next_check;
        self.snap_every = snap_every;
        self.low_since = low_since;
        Ok(snap_every)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TickPoll {
    pub ticks: u8,
    pub due_ns: u64,
    pub dropped: bool,
    pub lateness_ns: Option<u64>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CatchUpClock {
    due_ns: u64,
}
impl CatchUpClock {
    pub fn new(now_ns: u64) -> Self {
        Self { due_ns: now_ns }
    }
    pub fn due_ns(&self) -> u64 {
        self.due_ns
    }
    /// A host poll runs at most four due ticks, then drops any remaining backlog.
    pub fn poll(&mut self, now_ns: u64) -> Result<TickPoll, TimingError> {
        if now_ns < self.due_ns {
            return Ok(TickPoll {
                ticks: 0,
                due_ns: self.due_ns,
                dropped: false,
                lateness_ns: None,
            });
        }
        let lateness = now_ns - self.due_ns;
        let ticks = (lateness / TICK_NS + 1).min(4) as u8;
        let mut due_ns = self
            .due_ns
            .checked_add(TICK_NS * u64::from(ticks))
            .ok_or(TimingError::Capacity)?;
        let dropped = due_ns <= now_ns;
        if dropped {
            due_ns = now_ns.checked_add(TICK_NS).ok_or(TimingError::Capacity)?;
        }
        self.due_ns = due_ns;
        Ok(TickPoll {
            ticks,
            due_ns,
            dropped,
            lateness_ns: Some(lateness),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn original_meter_windows_and_server_polling_loop() {
        let fixture: Value =
            serde_json::from_str(include_str!("../tests/fixtures/tick-timing.json")).unwrap();
        for trace in fixture["meters"].as_array().unwrap() {
            let mut meter = TickMeter::new(trace["initialNow"].as_u64().unwrap()).unwrap();
            for step in trace["steps"].as_array().unwrap() {
                let sample = TickSample {
                    durations: std::array::from_fn(|i| {
                        step["sample"][PHASES[i]].as_f64().unwrap_or(0.0)
                    }),
                    sent: step["sample"]["sent"].as_bool().unwrap(),
                };
                let expected = &step["expected"];
                assert_eq!(
                    meter.record(sample, step["now"].as_u64().unwrap()).unwrap(),
                    expected["snapEvery"].as_u64().unwrap() as u8,
                    "{}",
                    trace["name"]
                );
                assert_eq!(meter.next_check(), expected["nextCheck"].as_u64().unwrap());
                assert_eq!(meter.low_since(), expected["lowSince"].as_u64());
                let stats = meter.stats();
                for (i, name) in PHASES.into_iter().chain(["snapshotTick"]).enumerate() {
                    let actual = if i < 6 {
                        stats.phases[i]
                    } else {
                        stats.snapshot_tick
                    };
                    assert_eq!(actual.p50, expected["stats"][name]["p50"].as_f64().unwrap());
                    assert_eq!(actual.p95, expected["stats"][name]["p95"].as_f64().unwrap());
                }
            }
        }
        for trace in fixture["clocks"].as_array().unwrap() {
            let mut clock = CatchUpClock::new(trace["initialNow"].as_u64().unwrap());
            for step in trace["steps"].as_array().unwrap() {
                let poll = clock.poll(step["now"].as_u64().unwrap()).unwrap();
                let expected = &step["expected"];
                assert_eq!(poll.ticks, expected["ticks"].as_u64().unwrap() as u8);
                assert_eq!(poll.due_ns, expected["due"].as_u64().unwrap());
                assert_eq!(poll.dropped, expected["dropped"].as_bool().unwrap());
            }
        }
    }

    #[test]
    fn invalid_samples_and_capacity_preserve_histories() {
        let mut meter = TickMeter::new(0).unwrap();
        let sample = TickSample {
            durations: [20.0; 6],
            sent: true,
        };
        for _ in 0..10 {
            meter.record(sample, 0).unwrap();
        }
        let before = meter.clone();
        for duration in [-1.0, f64::INFINITY, f64::NAN] {
            let mut invalid = sample;
            invalid.durations[5] = duration;
            assert_eq!(
                meter.record(invalid, 1000),
                Err(TimingError::InvalidDuration)
            );
            assert_eq!(meter, before);
        }
        assert_eq!(meter.record(sample, u64::MAX), Err(TimingError::Capacity));
        assert_eq!(meter, before);
        let mut clock = CatchUpClock::new(u64::MAX - 1);
        let before = clock.clone();
        assert_eq!(clock.poll(u64::MAX), Err(TimingError::Capacity));
        assert_eq!(clock, before);
    }

    #[test]
    fn winner_always_sends_without_changing_cadence() {
        let meter = TickMeter::new(0).unwrap();
        assert!(!meter.should_snapshot(1, false));
        assert!(meter.should_snapshot(2, false));
        assert!(meter.should_snapshot(1, true));
        assert_eq!(meter.snap_every(), 2);
    }
}
