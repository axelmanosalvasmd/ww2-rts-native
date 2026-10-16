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
            let kind = self
                .rules
                .kinds
                .get(unit.kind)
                .ok_or("unknown carrier tick kind")?;
            let reaches_riding = unit.hp > 0.0
                && !self
                    .commands
                    .units
                    .get(&id)
                    .is_some_and(|control| control.operative)
                && !(kind.building
                    && (!kind.weapon || unit.built.is_some_and(|built| built < 1.0)))
                && !kind.air
                && !kind.rail;
            // OnFoot also validates its actor before pruning cargo. Admit those
            // exact facts now, even when no transport relation is currently active.
            if reaches_riding {
                self.validate_carrier_unit(id)?;
            }
            if !unit.riding && meta.carrier.is_none() && unit.board == 0 && meta.cargo.is_empty() {
                continue;
            }
            if !reaches_riding {
                self.validate_carrier_unit(id)?;
            }
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
                let Some(parent) = self
                    .world
                    .game
                    .units
                    .get(&carrier)
                    .filter(|parent| parent.hp > 0.0)
                else {
                    if self.combat.get(carrier).is_none() {
                        self.prepare_carrier_ejection(id)?;
                        continue;
                    }
                    if self
                        .combat
                        .get(carrier)
                        .is_none_or(|m| !m.cargo.contains(&id))
                    {
                        return Err("inconsistent wrecked carrier relation".into());
                    }
                    self.prepare_carrier_ejections(carrier)?;
                    continue;
                };
                self.validate_carrier_unit(carrier)?;
                if self.carrier_boundary(carrier) || self.commands.units[&carrier].operative {
                    return Err("unsupported riding tick carrier".into());
                }
                if !self.combat.get(parent.id).unwrap().cargo.contains(&id) {
                    return Err("inconsistent reciprocal riding tick relation".into());
                }
                // A command or an earlier combat phase may wreck a currently live
                // carrier. Admit its passenger's eventual physical exit as well.
                self.prepare_carrier_ejection(id)?;
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
