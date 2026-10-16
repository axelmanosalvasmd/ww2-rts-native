//! Source World finish after capture, home relocation and army elimination preparation.
//! The region actuator owns those prerequisites and refuses unrepresented removal effects.
use crate::{
    catalog::{Catalog, Value},
    economy::Mode,
    economy_state::EconomyState,
    game::{CELL, ModeKind, Point},
    match_end::{EndReason, MatchFinish, MatchSample},
    match_runtime::Match,
    match_world::RegionState,
};
use std::collections::{BTreeMap, BTreeSet};

/// Ephemeral source unit values after staged World elimination and building wrecks.
/// These facts are not another unit store; the actuator owns and immediately commits the
/// corresponding World/environment/grid changes after this finish preparation succeeds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldFinishUnitFacts {
    pub id: u32,
    pub owner: usize,
    pub kind: usize,
    pub hp: f64,
}

fn validate_transition(
    current: &[WorldFinishUnitFacts],
    survivors: &[WorldFinishUnitFacts],
    removed: &[u32],
    removable_owners: &BTreeSet<usize>,
) -> Result<(), String> {
    let by_id = current
        .iter()
        .map(|u| (u.id, u))
        .collect::<BTreeMap<_, _>>();
    if by_id.len() != current.len() {
        return Err("duplicate canonical World unit identity".into());
    }
    let mut seen = BTreeSet::new();
    for after in survivors {
        let before = by_id
            .get(&after.id)
            .ok_or("unknown staged World survivor")?;
        if !seen.insert(after.id)
            || after.owner != before.owner
            || after.kind != before.kind
            || !after.hp.is_finite()
        {
            return Err("invalid staged World survivor".into());
        }
    }
    for id in removed {
        let before = by_id.get(id).ok_or("unknown staged World removal")?;
        if !seen.insert(*id) || !removable_owners.contains(&before.owner) {
            return Err("invalid staged World removal".into());
        }
    }
    if seen.len() != current.len() {
        return Err("incomplete staged World unit coverage".into());
    }
    Ok(())
}
impl Match {
    fn world_finish_unit_facts(&self) -> Result<Vec<WorldFinishUnitFacts>, String> {
        self.world
            .game
            .units
            .iter()
            .map(|(id, u)| {
                if *id == 0 || *id != u.id {
                    return Err("invalid canonical World finish unit identity".into());
                }
                Ok(WorldFinishUnitFacts {
                    id: *id,
                    owner: u.owner,
                    kind: u.kind,
                    hp: u.hp,
                })
            })
            .collect()
    }
    /// Owned preparation retains no canonical borrow across region/wallet/home/story commits.
    /// This is internal to the region actuator, not a shortcut past its side-effect preflight.
    pub(crate) fn prepare_world_finish(
        &self,
        wallets: &EconomyState,
        regions: &[RegionState],
    ) -> Result<Option<(MatchFinish, MatchSample)>, String> {
        let units = self.world_finish_unit_facts()?;
        self.prepare_world_finish_with_units(wallets, regions, &units)
    }
    /// Prepare against actual post-wreck survivors, before any canonical removal or terrain
    /// publication. Every current ID must occur exactly once as a survivor or ordered removal.
    /// Owners and kinds cannot change; finite surviving HP may change through real wreck effects.
    pub fn prepare_world_finish_after_removal(
        &self,
        wallets: &EconomyState,
        regions: &[RegionState],
        survivors: &[WorldFinishUnitFacts],
        removed: &[u32],
    ) -> Result<Option<(MatchFinish, MatchSample)>, String> {
        self.prepare_world_finish_after_region_changes(wallets, regions, survivors, removed, &[])
    }
    pub(crate) fn prepare_world_finish_after_region_changes(
        &self,
        wallets: &EconomyState,
        regions: &[RegionState],
        survivors: &[WorldFinishUnitFacts],
        removed: &[u32],
        transfers: &[crate::match_world_regions::WorldFortTransfer],
    ) -> Result<Option<(MatchFinish, MatchSample)>, String> {
        if self.options.mode != Mode::World || self.world.game.mode != Some(ModeKind::World) {
            return Err("World finish requires explicit canonical World mode".into());
        }
        if self.finish.is_some() {
            return Ok(None);
        }
        self.validate_match_finish_state()?;
        self.validate_sample_units()?;
        let mut current = self.world_finish_unit_facts()?;
        for transfer in transfers {
            let unit = current
                .iter_mut()
                .find(|u| u.id == transfer.id)
                .ok_or("unknown World finish fort transfer")?;
            unit.owner = transfer.owner;
        }
        let removable = wallets
            .players
            .iter()
            .enumerate()
            .filter(|(_, p)| p.out && !regions.iter().any(|r| r.team == p.team))
            .map(|(slot, _)| slot)
            .collect();
        validate_transition(&current, survivors, removed, &removable)?;
        self.prepare_world_finish_with_units(wallets, regions, survivors)
    }
    fn prepare_world_finish_with_units(
        &self,
        wallets: &EconomyState,
        regions: &[RegionState],
        units: &[WorldFinishUnitFacts],
    ) -> Result<Option<(MatchFinish, MatchSample)>, String> {
        if self.options.mode != Mode::World || self.world.game.mode != Some(ModeKind::World) {
            return Err("World finish requires explicit canonical World mode".into());
        }
        if self.finish.is_some() {
            return Ok(None);
        }
        self.validate_match_finish_state()?;
        self.validate_sample_units()?;
        let geography = self.world.geography().ok_or("missing World geography")?;
        let total = geography.size.region_count();
        if wallets.players.len() != self.start.economy.players.len() || regions.len() != total {
            return Err("invalid staged World finish shape".into());
        }
        for (slot, p) in wallets.players.iter().enumerate() {
            if p.team != self.start.economy.players[slot].team
                || [p.mp, p.mun, p.fuel, p.inc]
                    .into_iter()
                    .chain(p.fuel_inc)
                    .chain(p.upkeep)
                    .any(|v| !v.is_finite())
            {
                return Err("invalid staged World finish commander".into());
            }
            if !regions.iter().any(|r| r.team == p.team)
                && (!p.out || units.iter().any(|u| u.owner == slot))
            {
                return Err("World finish requires completed army elimination".into());
            }
        }
        for r in regions {
            if !(-1..6).contains(&r.team)
                || !r.progress.is_finite()
                || !(0.0..=1.0).contains(&r.progress)
                || r.capper.is_some_and(|slot| slot >= wallets.players.len())
                || r.cap_tick.is_some_and(|tick| {
                    !(-9_007_199_254_740_991..=9_007_199_254_740_991).contains(&tick)
                })
            {
                return Err("invalid staged World finish region".into());
            }
        }
        // Set insertion order is first player appearance, not sorted team IDs.
        let mut alive = Vec::new();
        let mut all = Vec::new();
        for p in &wallets.players {
            if !all.contains(&p.team) {
                all.push(p.team);
            }
            if !p.out && !alive.contains(&p.team) {
                alive.push(p.team);
            }
        }
        let tuning = Catalog::compiled()
            .map_err(|e| e.to_string())?
            .table("WORLD_TUNING")
            .map_err(|e| e.to_string())?;
        let share = tuning
            .get("winShare")
            .and_then(Value::number)
            .filter(|v| v.is_finite())
            .ok_or("invalid World win share")?;
        let goal = (total as f64 * share).ceil();
        if !goal.is_finite() || goal < 0.0 {
            return Err("invalid World goal".into());
        }
        let rivals = all.len() > 1;
        let mut decision = if alive.is_empty() {
            Some((-1, EndReason::Draw))
        } else {
            None
        };
        if decision.is_none() {
            for team in &alive {
                let owned = regions.iter().filter(|r| r.team == *team).count();
                if owned == total || (rivals && owned as f64 >= goal) {
                    decision = Some((*team, EndReason::World));
                    break;
                }
            }
            if decision.is_none() && rivals && alive.len() == 1 {
                decision = Some((alive[0], EndReason::Nations));
            }
        }
        let Some((winner, reason)) = decision else {
            return Ok(None);
        };
        // The first region captured this tick supplies the location, even for another team.
        // Draw uses the source map-center fallback regardless of capture timestamps.
        let at = if winner == -1 {
            None
        } else {
            regions
                .iter()
                .position(|r| r.cap_tick == Some(self.world.game.tick))
                .map(|i| Point {
                    x: (geography.regions[i].x as f64 + 0.5) * CELL,
                    z: (geography.regions[i].y as f64 + 0.5) * CELL,
                })
        };
        Ok(Some(self.prepare_match_finish_using(
            winner,
            reason,
            at,
            || self.match_sample_with_units(units.iter().map(|u| (u.owner, u.kind, u.hp))),
        )?))
    }
}
