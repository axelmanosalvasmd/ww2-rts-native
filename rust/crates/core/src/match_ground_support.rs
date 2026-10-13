//! Ground-support phase preparation for the sole Match tick runner.
//! This module does not advance the clock or consume the recorded command queue.

use crate::{
    catalog::Catalog,
    match_commands::Command,
    match_runtime::Match,
    match_support::{SupportError, SupportKind},
};

impl Match {
    /// Check represented support state before any boundary command or clock mutation.
    /// Cooldown and wallet denials still belong to ordinary command admission.
    pub fn validate_ground_support_tick(&self, boundary: u64) -> Result<(), SupportError> {
        let tick = u64::try_from(self.world.game.tick)
            .map_err(|_| SupportError::Invalid("invalid support tick clock"))?;
        if tick.checked_add(1) != Some(boundary) {
            return Err(SupportError::Invalid(
                "support boundary does not follow clock",
            ));
        }
        self.validate_ground_support_state()?;
        for (_, command) in self.commands.queue.pending_tick(boundary) {
            if let Command::Support { kind, .. } = command
                && matches!(
                    kind.as_str(),
                    "recon" | "strafe" | "bombing" | "dive" | "para"
                )
            {
                return Err(SupportError::Unsupported(
                    "support aircraft tick phase is unavailable",
                ));
            }
        }
        Ok(())
    }

    fn validate_ground_support_state(&self) -> Result<(), SupportError> {
        self.validate_support()?;
        if self.world.game.strikes.iter().any(|strike| {
            strike.runtime.as_ref().is_none_or(|timing| {
                !matches!(
                    timing.kind,
                    SupportKind::Artillery | SupportKind::Smoke | SupportKind::Cover
                )
            })
        }) {
            return Err(SupportError::Unsupported(
                "support strikes require canonical ground metadata",
            ));
        }
        if self
            .world
            .game
            .units
            .values()
            .any(|u| u.air.is_some() || self.rules.kinds.get(u.kind).is_some_and(|kind| kind.air))
        {
            return Err(SupportError::Unsupported(
                "on-map aircraft support phase is unavailable",
            ));
        }
        let kinds = Catalog::compiled()
            .and_then(|catalog| catalog.table("SUPPORT"))
            .map_err(|e| SupportError::Actuation(e.to_string()))?
            .object()
            .ok_or(SupportError::Invalid("invalid compiled support table"))?;
        for (slot, start) in self.start.players.iter().enumerate() {
            if start.config.team != self.world.game.players[slot].team
                || start.support.len() != kinds.len()
                || kinds.keys().any(|kind| !start.support.contains_key(kind))
            {
                return Err(SupportError::Invalid("support player metadata mismatch"));
            }
        }
        if self
            .support
            .covers
            .iter()
            .any(|cover| cover.team != self.world.game.players[cover.owner].team)
        {
            return Err(SupportError::Invalid("support cover team mismatch"));
        }
        Ok(())
    }

    /// Run after projectiles with membership captured before separation, then decay
    /// covers and all support cooldowns before scenario and living weather.
    /// No on-map aircraft exist in this bounded context, so source anti-air is inert.
    pub fn step_ground_support_phase(
        &mut self,
        captured_late_ids: &[u32],
        dt: f64,
    ) -> Result<(), SupportError> {
        self.validate_ground_support_state()?;
        self.step_support_strikes(captured_late_ids, dt)?;
        self.advance_support_timers(dt)
    }
}
