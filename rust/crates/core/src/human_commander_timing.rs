//! The ordinary human commander's private timing stream and difficulty values.
//! Camera, alerts, hands, policy and Match scheduling remain separate providers.

use crate::math;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Skill {
    pub react: f64,
    pub notice: f64,
    pub input: f64,
    pub scan: f64,
    pub opening: f64,
    pub peak: usize,
    pub minute: usize,
}

impl Skill {
    pub fn for_level(level: &str) -> Self {
        match level {
            "easy" => Self {
                react: 0.8,
                notice: 3.4,
                input: 0.62,
                scan: 3.6,
                opening: 4.6,
                peak: 10,
                minute: 34,
            },
            "hard" => Self {
                react: 0.26,
                notice: 1.0,
                input: 0.14,
                scan: 0.45,
                opening: 2.1,
                peak: 50,
                minute: 160,
            },
            _ => Self {
                react: 0.5,
                notice: 1.7,
                input: 0.32,
                scan: 1.4,
                opening: 3.2,
                peak: 20,
                minute: 68,
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimingError {
    InvalidSample,
    InvalidTicks,
}

/// Owned by one seat, independently of the planner and simulation RNGs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Timing {
    value: u32,
    draws: u64,
}

impl Timing {
    pub fn new(seed: u32, slot: u32) -> Self {
        Self {
            value: seed ^ slot.wrapping_add(1).wrapping_mul(0x9E37_79B1),
            draws: 0,
        }
    }

    pub fn draws(&self) -> u64 {
        self.draws
    }

    fn fraction(&mut self) -> f64 {
        self.value = self.value.wrapping_add(0x6D2B_79F5);
        let mut t = self.value;
        t = (t ^ (t >> 15)).wrapping_mul(t | 1);
        t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 61));
        self.draws += 1;
        f64::from(t ^ (t >> 14)) / 4_294_967_296.0
    }

    /// Two private draws, in original Box-Muller order.
    pub fn sample(&mut self, median: f64, spread: f64) -> Result<f64, TimingError> {
        if !median.is_finite() || median < 0.0 || !spread.is_finite() || spread < 0.0 {
            return Err(TimingError::InvalidSample);
        }
        let mut staged = self.clone();
        let gauss = math::pow(-2.0 * math::log(1.0 - staged.fraction()), 0.5)
            * math::cos(2.0 * std::f64::consts::PI * staged.fraction());
        let result = median
            * math::exp(math::max(
                -4.0 * spread,
                math::min(4.0 * spread, gauss * spread),
            ));
        if !result.is_finite() {
            return Err(TimingError::InvalidSample);
        }
        *self = staged;
        Ok(result)
    }

    pub fn opening_delay(&mut self, skill: Skill) -> Result<u64, TimingError> {
        self.delay(skill.opening, 0.2, 1.0, 0.0)
    }

    pub fn reaction_delay(&mut self, skill: Skill, on_screen: bool) -> Result<u64, TimingError> {
        self.delay(
            if on_screen { skill.react } else { skill.notice },
            0.35,
            1.0,
            0.2,
        )
    }

    pub fn input_delay(&mut self, skill: Skill) -> Result<u64, TimingError> {
        self.delay(skill.input, 0.35, 1.0, 0.0)
    }

    pub fn scan_delay(&mut self, skill: Skill, load: f64) -> Result<u64, TimingError> {
        if !load.is_finite() || load < 0.0 {
            return Err(TimingError::InvalidSample);
        }
        self.delay(skill.scan, 0.5, 1.0 + 3.0 * load, 0.0)
    }

    fn delay(
        &mut self,
        median: f64,
        spread: f64,
        scale: f64,
        floor: f64,
    ) -> Result<u64, TimingError> {
        let mut staged = self.clone();
        let seconds = math::max(floor, staged.sample(median, spread)? * scale);
        let result = ticks(seconds)?;
        *self = staged;
        Ok(result)
    }
}

/// Source rounds seconds to the nearest tick and keeps at least one tick.
pub fn ticks(seconds: f64) -> Result<u64, TimingError> {
    let value = math::max(1.0, math::round(seconds * 20.0));
    if !seconds.is_finite() || seconds < 0.0 || value >= u64::MAX as f64 {
        return Err(TimingError::InvalidTicks);
    }
    Ok(value as u64)
}

/// Only caller-owned runtime facts, never authoritative hidden match state.
pub fn needs_view(initialized: bool, look_at: Option<u64>, front_is_plan: bool, now: u64) -> bool {
    !initialized || look_at.is_some_and(|at| at <= now.saturating_add(1)) || front_is_plan
}
