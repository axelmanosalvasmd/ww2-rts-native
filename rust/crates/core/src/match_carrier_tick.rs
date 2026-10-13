//! Read-only admission for the existing ordinary carrier unit phases.
use crate::{match_runtime::Match, path_requests::RequestKind};

impl Match {
    /// Validate current transport facts before a tick consumes inputs or writes poses.
    /// Stale cargo and missing/dead Board targets retain their source phase outcomes.
    pub fn validate_carrier_tick(&self) -> Result<(), String> {
        self.validate_carrier_world()?;
        let dt = self.rules.movement.tick;
        if !dt.is_finite() || !(0.0..=2.5).contains(&dt) {
            return Err("invalid carrier tick interval".into());
        }
        for (&id, unit) in &self.world.game.units {
            let meta = self.combat.get(id).ok_or("missing carrier tick metadata")?;
            if !unit.riding && unit.board == 0 && meta.cargo.is_empty() {
                continue;
            }
            self.validate_carrier_unit(id)?;
            if unit.hp <= 0.0 {
                continue;
            }
            if unit.riding {
                if self.carrier_boundary(id)
                    || !self.rules.kinds[unit.kind].infantry
                    || self.commands.units[&id].operative
                {
                    return Err("unsupported riding tick actor".into());
                }
                let carrier = meta.carrier.ok_or("missing riding carrier identity")?;
                let parent = self
                    .world
                    .game
                    .units
                    .get(&carrier)
                    .filter(|parent| parent.hp > 0.0)
                    .ok_or("thrown carrier ejection tick phase is not connected")?;
                self.validate_carrier_unit(carrier)?;
                if self.carrier_boundary(carrier) || self.commands.units[&carrier].operative {
                    return Err("unsupported riding tick carrier".into());
                }
                if !self.combat.get(parent.id).unwrap().cargo.contains(&id) {
                    return Err("inconsistent reciprocal riding tick relation".into());
                }
                let cd = self
                    .abilities
                    .get(id)
                    .ok_or("missing rider ability timer")?
                    .cooldown;
                if !cd.is_finite() || !(cd - dt).is_finite() {
                    return Err("invalid rider ability timer".into());
                }
                continue;
            }
            // The OnFoot phase prunes missing and nonriding entries. Validate live
            // referenced rows without changing that first-source-turn membership.
            for &rider in &meta.cargo {
                if self.world.game.units.contains_key(&rider) {
                    self.validate_carrier_unit(rider)?;
                }
            }
            if unit.board != 0 {
                if self.carrier_boundary(id)
                    || !self.rules.kinds[unit.kind].infantry
                    || self.commands.units[&id].operative
                {
                    return Err("unsupported boarding tick actor".into());
                }
                if let Some(target) = self.world.game.units.get(&unit.board) {
                    self.validate_carrier_unit(target.id)?;
                    if self.carrier_boundary(target.id) {
                        return Err("unsupported boarding tick carrier".into());
                    }
                }
            }
        }
        for request in self.commands.requests.queued_requests() {
            if request.kind == RequestKind::Board {
                self.current_board_goal(request.unit)?;
            }
        }
        Ok(())
    }
}
