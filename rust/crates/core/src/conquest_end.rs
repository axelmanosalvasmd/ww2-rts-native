//! Ordinary Conquest victory and finish bookkeeping on the authoritative Match.
//! Capture/income and the simulation clock are separate phases. No other mode is supported.

use crate::{
    economy::Mode,
    game::{CELL, Point},
    match_runtime::Match,
};

pub use crate::match_end::{EndReason as ConquestEndReason, MatchFinish as ConquestFinish};

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
        self.sample_match_tick()
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
        let prepared = self.prepare_match_finish(winner, ConquestEndReason::VictoryPoints, Some(at))?;
        Ok(Some(self.publish_match_finish(prepared)))
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
        self.validate_match_finish_state()
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
}
