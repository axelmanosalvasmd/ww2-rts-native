//! Ordinary Conquest victory and finish bookkeeping on the authoritative Match.
//! Capture/income and the simulation clock are separate phases. No other mode is supported.

use crate::{
    economy::Mode,
    game::{CELL, Point},
    match_runtime::Match,
    math,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConquestEndReason {
    VictoryPoints,
}
impl ConquestEndReason {
    pub const fn as_str(self) -> &'static str {
        "vp"
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ConquestFinish {
    pub winner: i32,
    pub reason: ConquestEndReason,
    pub end_at: Point,
    pub end_tick: i64,
}
/// The exact ordinary-Conquest sample fields in `shared/story.js`: simulation seconds and
/// floor victory points indexed by team ID. Gaps in team IDs contain zero.
#[derive(Clone, Debug, PartialEq)]
pub struct ConquestSample {
    pub t: f64,
    pub vp: Vec<f64>,
}

impl Match {
    /// Canonical point phase. Decided state freezes VP and capture story bookkeeping while
    /// source capture ownership and manpower income can continue during the ending hold.
    pub fn apply_conquest_point_economy(
        &mut self,
        unit_order: &[u32],
    ) -> Result<Vec<crate::objectives::Captured>, String> {
        self.validate_conquest_finish_state()?;
        self.apply_point_economy(unit_order, self.finish.is_some())
    }

    /// Source early-tick story sample, at tick 1 and every 200 ticks while undecided.
    /// Call once at that phase of each tick. Does not advance the clock or evaluate victory.
    pub fn sample_conquest_tick(&mut self) -> Result<bool, String> {
        self.validate_conquest_mode()?;
        if self.finish.is_some() {
            return Ok(false);
        }
        self.validate_conquest_finish_state()?;
        let tick = self.world.game.tick;
        if tick == 1 || tick % 200 == 0 {
            let sample = self.conquest_sample()?;
            self.timeline.push(sample);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Source final Conquest check in player order. Losing armies are not eliminated here:
    /// Conquest ends by team VP even when a team has lost all units or buildings.
    /// The first result remains canonical while later simulation phases may continue.
    pub fn apply_conquest_victory(&mut self) -> Result<Option<ConquestFinish>, String> {
        self.validate_conquest_mode()?;
        if self.finish.is_some() {
            return Ok(None);
        }
        self.validate_conquest_finish_state()?;
        let mut winner = None;
        for player in &self.start.economy.players {
            if self.team_vp(player.team)? >= self.start.win_vp {
                winner = Some(player.team);
                break;
            }
        }
        let Some(winner) = winner else {
            return Ok(None);
        };
        let mut last = None;
        let mut latest = -1;
        for (index, point) in self.start.points.iter().enumerate() {
            if point
                .owner
                .is_some_and(|owner| self.start.economy.players[owner].team == winner)
            {
                let tick = self.capture_ticks[index].unwrap_or(-1);
                if last.is_none() || tick > latest {
                    last = Some(point.at);
                    latest = tick;
                }
            }
        }
        let at = last.unwrap_or(Point {
            x: self.world.game.w as f64 * CELL / 2.0,
            z: self.world.game.h as f64 * CELL / 2.0,
        });
        let end_at = Point {
            x: math::round(at.x * 10.0) / 10.0,
            z: math::round(at.z * 10.0) / 10.0,
        };
        if !end_at.x.is_finite() || !end_at.z.is_finite() {
            return Err("invalid Conquest end position".into());
        }
        let finish = ConquestFinish {
            winner,
            reason: ConquestEndReason::VictoryPoints,
            end_at,
            end_tick: self.world.game.tick,
        };
        let sample = self.conquest_sample()?;
        self.timeline.push(sample);
        self.finish = Some(finish);
        self.world.game.reveal = true;
        Ok(Some(finish))
    }

    pub(crate) fn validate_conquest_mode(&self) -> Result<(), String> {
        if !matches!(self.options.mode, Mode::Default | Mode::Conquest)
            || self.world.game.mode.is_some()
        {
            Err("victory phase supports ordinary Conquest only".into())
        } else {
            Ok(())
        }
    }
    pub(crate) fn validate_conquest_finish_state(&self) -> Result<(), String> {
        self.validate_conquest_mode()?;
        let players = &self.start.economy.players;
        if !(1..=6).contains(&players.len())
            || self.start.players.len() != players.len()
            || self.options.players.len() != players.len()
            || self.world.game.players.len() != players.len()
            || self.capture_counts.len() != players.len()
            || self.capture_ticks.len() != self.start.points.len()
            || self.world.game.tick < 0
            || self.world.game.tick > 9_007_199_254_740_991
            || self.world.game.w == 0
            || self.world.game.h == 0
            || self.start.dimensions != (self.world.game.w, self.world.game.h)
            || !self.start.win_vp.is_finite()
            || self.start.win_vp <= 0.0
        {
            return Err("invalid Conquest finish state".into());
        }
        for (index, p) in players.iter().enumerate() {
            if !(0..6).contains(&p.team)
                || p.team != self.start.players[index].config.team
                || p.team != self.world.game.players[index].team
                || p.team != self.options.players[index].team
                || !p.vp.is_finite()
                || !self.capture_counts[index].is_finite()
                || self.capture_counts[index] < 0.0
            {
                return Err("invalid Conquest player identity or score".into());
            }
        }
        if self
            .capture_ticks
            .iter()
            .flatten()
            .any(|tick| *tick < -9_007_199_254_740_991 || *tick > 9_007_199_254_740_991)
        {
            return Err("invalid Conquest capture tick".into());
        }
        for point in &self.start.points {
            if point.owner.is_some_and(|owner| owner >= players.len())
                || !point.at.x.is_finite()
                || !point.at.z.is_finite()
            {
                return Err("invalid Conquest objective owner or position".into());
            }
        }
        // Check every team sum before a phase can mutate canonical state, even a later player
        // that would never be consulted once an earlier player's team has won.
        for player in players {
            self.team_vp(player.team)?;
        }
        Ok(())
    }
    fn team_vp(&self, team: i32) -> Result<f64, String> {
        let mut total = 0.0;
        for player in &self.start.economy.players {
            total += if player.team == team { player.vp } else { 0.0 };
        }
        if total.is_finite() {
            Ok(total)
        } else {
            Err("nonfinite Conquest team score".into())
        }
    }
    fn conquest_sample(&self) -> Result<ConquestSample, String> {
        let max_team = self
            .start
            .economy
            .players
            .iter()
            .map(|p| p.team)
            .max()
            .ok_or("Conquest sample needs players")?;
        let mut vp = vec![0.0; (max_team + 1) as usize];
        for p in &self.start.economy.players {
            let total = vp[p.team as usize] + p.vp;
            if !total.is_finite() {
                return Err("nonfinite Conquest timeline score".into());
            }
            vp[p.team as usize] = total;
        }
        for value in &mut vp {
            *value = value.floor();
        }
        Ok(ConquestSample {
            t: math::round(self.world.game.tick as f64 / 20.0),
            vp,
        })
    }
}
