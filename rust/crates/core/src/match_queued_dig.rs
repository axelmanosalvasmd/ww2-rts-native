//! Waiting Dig admission over the canonical Match state.
use super::{Error, FortKind, Reason, StartRequest};
use crate::{
    game::Point,
    match_commands::{Command, Denial},
    match_runtime::Match,
};
use std::collections::BTreeMap;

fn denial(reason: Reason) -> Denial {
    match reason {
        Reason::Blocked => Denial::Blocked,
        Reason::NoBuilders => Denial::NoBuilders,
        Reason::Retreating => Denial::Retreating,
        Reason::Mp => Denial::Manpower,
        Reason::NotVisible => Denial::NotVisible,
        Reason::Max => Denial::Maximum,
    }
}

impl Match {
    /// Checks the present terrain and crew, but defers payment until ordinary Dig activation.
    pub(crate) fn queue_dig(
        &mut self,
        owner: usize,
        ids: &[i64],
        kind: FortKind,
        at: Point,
        direction: Option<f64>,
        winner_decided: bool,
    ) -> Result<Option<Denial>, String> {
        let Some(player) = self.start.economy.players.get(owner) else {
            return Ok(Some(Denial::Blocked));
        };
        if winner_decided || player.away || player.out {
            return Ok(Some(Denial::Blocked));
        }
        self.validate_field_work_state()?;
        let tier_denial = self.dig_tier_denial(owner, kind)?;
        let Some(id) = ids.first().copied().and_then(|id| u32::try_from(id).ok()) else {
            return Ok(tier_denial.or(Some(Denial::NoBuilders)));
        };
        let Some(unit) = self.world.game.units.get(&id) else {
            return Ok(tier_denial.or(Some(Denial::NoBuilders)));
        };
        let profile = self
            .rules
            .kinds
            .get(unit.kind)
            .ok_or("unknown dig unit kind")?;
        let control = &self.commands.units[&id];
        let eligible = unit.owner == owner
            && !profile.structure
            && !profile.rail
            && !unit.rail
            && !unit.riding
            && !control.operative
            && !self.logistics.get(id).is_some_and(|r| r.forced);
        if !eligible {
            return Ok(tier_denial.or(Some(Denial::NoBuilders)));
        }
        // Source mine(ids[0]) releases the squad from its leader even if the
        // later builder, placement or queue-capacity check refuses the order.
        self.commands.units.get_mut(&id).unwrap().follow = 0;
        if let Some(reason) = tier_denial {
            return Ok(Some(reason));
        }
        crate::match_construction::validate_build_activity(unit, &self.commands.units[&id])?;
        let request = StartRequest {
            control_eligible: true,
            owner,
            unit: id,
            kind,
            at,
            direction,
        };
        let mut wallet = player.clone();
        // Source bypasses only the payment check while waiting. The shared placement
        // validator still checks the actual builder, terrain, sight, and single-use cap.
        wallet.mp = f64::MAX;
        let mut sight = BTreeMap::<usize, bool>::new();
        loop {
            let mut missing = None;
            let result =
                self.field_works
                    .prepare_start(&self.world, &mut wallet, request, |point| {
                        let cell = self
                            .world
                            .game
                            .cell_of(point.x, point.z)
                            .expect("in-map placement query");
                        sight.get(&cell).copied().unwrap_or_else(|| {
                            missing = Some(point);
                            false
                        })
                    });
            match result {
                Err(Error::Refused(Reason::NotVisible)) if missing.is_some() => {
                    let point = missing.unwrap();
                    let visible = self.recipient_sees(owner, point)?;
                    sight.insert(self.world.game.cell_of(point.x, point.z).unwrap(), visible);
                }
                Err(Error::Refused(reason)) => return Ok(Some(denial(reason))),
                Err(error) => return Err(format!("queued dig admission: {error}")),
                Ok(prepared) => {
                    let direction = prepared.job().direction;
                    let control = self
                        .commands
                        .units
                        .get_mut(&id)
                        .expect("validated dig control");
                    if control.orders.len() >= 8 {
                        return Ok(Some(Denial::QueueFull));
                    }
                    control.orders.push_back(Command::Dig {
                        ids: vec![i64::from(id)],
                        kind,
                        at: prepared.job().at,
                        direction: Some(direction),
                        queue: false,
                    });
                    return Ok(None);
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "match_queued_dig_tests.rs"]
mod tests;
