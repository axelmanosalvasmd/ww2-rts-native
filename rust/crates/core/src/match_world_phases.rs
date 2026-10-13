//! Explicit World income over final captured objects, separate from current region actors.
//! The caller owns late-list membership, final retirement facts and phase ordering.
//! This module neither advances the clock nor runs a World controller or convoy phase.

use crate::{
    economy::Mode,
    economy_state as economy,
    game::{LOCAL_DEFENDER_OWNER, ModeKind, Unit},
    match_runtime::Match,
};
use std::collections::BTreeSet;

/// Final values of one real object in the source's captured late list.
/// A removed object remains explicit input. No missing ID is dropped or assigned HP.
/// The caller preserves captured order and supplies final current or retired values
/// after late effects and ordinary retirement, immediately before World income.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldIncomeUnit {
    pub id: u32,
    pub owner: usize,
    pub kind: usize,
    pub hp: f64,
}
impl From<&Unit> for WorldIncomeUnit {
    fn from(unit: &Unit) -> Self {
        Self {
            id: unit.id,
            owner: unit.owner,
            kind: unit.kind,
            hp: unit.hp,
        }
    }
}
impl Match {
    /// Publish only income. Regions, homes, story and armies are not advanced here.
    /// Current depots are read separately from the captured upkeep facts.
    /// This does not admit World logistics, a closing hold or a complete World tick.
    pub fn apply_world_income(
        &mut self,
        captured: &[WorldIncomeUnit],
        dt: f64,
    ) -> Result<(), String> {
        let wallets = self.prepare_world_income(captured, dt)?;
        self.start.economy = wallets;
        Ok(())
    }

    pub(crate) fn validate_world_phase_context(
        &self,
        dt: f64,
    ) -> Result<Vec<economy::Region>, String> {
        if self.options.mode != Mode::World
            || self.world.game.mode != Some(ModeKind::World)
            || self.options.logistics
            || self.finish.is_some()
        {
            return Err(
                "World region phase requires undecided World with logistics disabled".into(),
            );
        }
        if !dt.is_finite() || dt < 0.0 {
            return Err("invalid World phase dt".into());
        }
        let regions = self.world_economy_regions()?;
        self.world.geography().ok_or("missing World geography")?;
        let game = &self.world.game;
        let players = &self.start.economy.players;
        if !(1..=6).contains(&players.len())
            || players.len() != game.players.len()
            || players.len() != self.start.players.len()
            || players.len() != self.story.len()
            || players.len() != self.options.players.len()
            || game.tick < 0
            || game.tick > 9_007_199_254_740_991
        {
            return Err("invalid World phase shape".into());
        }
        for (slot, p) in players.iter().enumerate() {
            if [p.mp, p.mun, p.fuel, p.vp, p.inc]
                .into_iter()
                .chain(p.fuel_inc)
                .chain(p.upkeep)
                .any(|value| !value.is_finite())
            {
                return Err("invalid World wallet facts".into());
            }
            if !(0..6).contains(&p.team)
                || game.players[slot].slot != slot
                || p.team != game.players[slot].team
                || p.team != self.start.players[slot].config.team
                || p.team != self.options.players[slot].team
                || !self.start.players[slot].spawn.x.is_finite()
                || !self.start.players[slot].spawn.z.is_finite()
            {
                return Err("invalid World commander identity".into());
            }
        }
        // These are current authoritative actors, not a reconstruction of the captured list.
        for (&id, u) in &game.units {
            if u.id != id
                || u.kind >= self.rules.kinds.len()
                || !u.hp.is_finite()
                || u.built.is_some_and(|value| !value.is_finite())
                || !u.x.is_finite()
                || !u.z.is_finite()
                || (u.owner != LOCAL_DEFENDER_OWNER && u.owner >= players.len())
            {
                return Err("invalid World phase unit".into());
            }
        }
        Ok(regions)
    }

    pub(crate) fn current_world_income_units(
        &self,
        order: &[u32],
    ) -> Result<Vec<WorldIncomeUnit>, String> {
        let mut seen = BTreeSet::new();
        order
            .iter()
            .map(|id| {
                if !seen.insert(*id) {
                    return Err("duplicate World tick unit".into());
                }
                self.world
                    .game
                    .units
                    .get(id)
                    .map(WorldIncomeUnit::from)
                    .ok_or_else(|| "unsupported World income: removed tick unit facts".into())
            })
            .collect()
    }

    pub(crate) fn prepare_world_income(
        &self,
        captured: &[WorldIncomeUnit],
        dt: f64,
    ) -> Result<economy::EconomyState, String> {
        let regions = self.validate_world_phase_context(dt)?;
        let game = &self.world.game;
        let mut seen = BTreeSet::new();
        for unit in captured {
            if unit.id == 0
                || !seen.insert(unit.id)
                || unit.kind >= self.rules.kinds.len()
                || !unit.hp.is_finite()
                || (unit.owner != LOCAL_DEFENDER_OWNER
                    && unit.owner >= self.start.economy.players.len())
            {
                return Err("invalid captured World income unit".into());
            }
            if let Some(current) = game.units.get(&unit.id)
                && (current.owner != unit.owner
                    || current.kind != unit.kind
                    || current.hp != unit.hp)
            {
                return Err("captured World income facts differ from current object".into());
            }
        }
        // World income reads only owner, kind and HP from the captured objects.
        // The generic economy adapter's built/queue fields are unused by this calculation.
        let income_units = captured
            .iter()
            .map(|u| economy::Unit {
                id: u64::from(u.id),
                owner: (u.owner != LOCAL_DEFENDER_OWNER).then_some(u.owner),
                kind: self.rules.kinds[u.kind].name.as_str(),
                hp: u.hp,
                built: 0.0,
                queue: &[],
            })
            .collect::<Vec<_>>();
        let current_depots = game
            .units
            .values()
            .map(|u| economy::DepotFacts {
                id: u64::from(u.id),
                owner: (u.owner != LOCAL_DEFENDER_OWNER).then_some(u.owner),
                hp: u.hp,
                built: u.built.unwrap_or(0.0),
            })
            .collect::<Vec<_>>();
        let nodes = self
            .world_runtime
            .as_ref()
            .ok_or("missing World runtime")?
            .nodes
            .iter()
            .map(|node| {
                Ok(economy::Node {
                    depot: node.depot.map(u64::from),
                    rate: node.rate,
                    fuel: node.fuel,
                    region: self.world_region_index(node.at)?,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let mut wallets = self.start.economy.clone();
        wallets
            .advance_income_with_depots(
                &economy::Context {
                    mode: economy::ModeContext {
                        kind: Mode::World,
                        defender_team: None,
                        income_excluded_slot: None,
                    },
                    army: self.options.army,
                    units: &income_units,
                    drops: &[],
                    points: &[],
                    nodes: &nodes,
                    world: Some(economy::WorldContext { regions: &regions }),
                },
                &current_depots,
                dt,
                false,
            )
            .map_err(|e| e.to_string())?;
        if wallets.players.iter().any(|p| {
            [p.mp, p.mun, p.fuel, p.inc]
                .into_iter()
                .chain(p.fuel_inc)
                .chain(p.upkeep)
                .any(|v| !v.is_finite())
        }) {
            return Err("nonfinite World income result".into());
        }
        Ok(wallets)
    }
}
