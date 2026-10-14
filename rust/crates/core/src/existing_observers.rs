//! Borrowed physical exits for current-unit sight replacement during a staged order.
use crate::{
    game::{CELL, Game, MOVE, Unit},
    grid::UnitId,
    match_commands::physical_navigation,
    rules::Rules,
    unit_spawn::SpawnPlacer,
};

/// A draft from the source nearest-free MOVE exit, with the original identity borrowed.
/// The borrow prevents physical publication until every staged route is admitted.
pub(crate) struct PreparedGroundExit<'a> {
    original: &'a Unit,
    draft: Unit,
}
impl<'a> PreparedGroundExit<'a> {
    pub(crate) fn prepare(game: &'a Game, rules: &Rules, id: UnitId) -> Result<Self, String> {
        let original = game.units.get(&id).ok_or("missing prepared exit actor")?;
        let kind = rules
            .kinds
            .get(original.kind)
            .ok_or("unknown prepared exit kind")?;
        if original.id != id
            || id == 0
            || original.owner >= game.players.len()
            || original.air.is_some()
            || original.riding
            || original.rail
            || kind.rail
            || kind.structure
            || ![original.x, original.z, original.rot, original.hp]
                .iter()
                .all(|v| v.is_finite())
            || game.cell_of(original.x, original.z).is_none()
            || original.garrison.is_some_and(|c| c >= game.chars.len())
        {
            return Err("invalid current ground exit identity".into());
        }
        let mut draft = original.clone();
        if original.garrison.is_some() {
            let terrain = physical_navigation(game);
            let cell = SpawnPlacer::new()
                .nearest_free(terrain, original.at(), MOVE, None)
                .map_err(|e| format!("prepared building exit: {e:?}"))?;
            draft.x = (cell % terrain.width) as f64 * CELL + CELL * 0.5;
            draft.z = (cell / terrain.width) as f64 * CELL + CELL * 0.5;
            draft.garrison = None;
        }
        Ok(Self { original, draft })
    }
    pub(crate) fn id(&self) -> UnitId {
        self.original.id
    }
    pub(crate) fn unit(&self) -> &Unit {
        &self.draft
    }
    pub(crate) fn into_unit(self) -> Unit {
        self.draft
    }
}

/// Existing rows are replaced in canonical ID order. They are never appended as births.
#[derive(Clone, Copy)]
pub(crate) struct ExistingObservers<'a, 'g> {
    exits: &'a [PreparedGroundExit<'g>],
}
impl<'a, 'g> ExistingObservers<'a, 'g> {
    pub(crate) fn new(exits: &'a [PreparedGroundExit<'g>]) -> Result<Self, String> {
        let mut ids = std::collections::BTreeSet::new();
        for exit in exits {
            if !ids.insert(exit.id()) {
                return Err("duplicate existing sight replacement".into());
            }
        }
        Ok(Self { exits })
    }
    pub(crate) fn validate_source(self, game: &Game, owner: usize) -> Result<(), String> {
        for exit in self.exits {
            if exit.original.owner != owner
                || !game
                    .units
                    .get(&exit.id())
                    .is_some_and(|unit| std::ptr::eq(unit, exit.original))
            {
                return Err("existing sight replacement source mismatch".into());
            }
        }
        Ok(())
    }
    pub(crate) fn replace<'b>(&'b self, original: &'b Unit) -> &'b Unit {
        self.exits
            .iter()
            .find(|exit| exit.id() == original.id)
            .map_or(original, |exit| exit.unit())
    }
    pub(crate) fn replacements(self) -> impl Iterator<Item = &'a Unit> {
        self.exits.iter().map(PreparedGroundExit::unit)
    }
    pub(crate) fn contains(self, id: UnitId) -> bool {
        self.exits.iter().any(|exit| exit.id() == id)
    }
}

/// One transient source projection, leaving the canonical actor collection untouched.
#[derive(Clone, Copy)]
pub(crate) enum ObserverOverlay<'a, 'g> {
    None,
    Added(&'a Unit),
    Existing(ExistingObservers<'a, 'g>),
}
impl ObserverOverlay<'_, '_> {
    pub(crate) fn replace<'a>(&'a self, original: &'a Unit) -> &'a Unit {
        match self {
            Self::Existing(exits) => exits.replace(original),
            _ => original,
        }
    }
    pub(crate) fn added(&self) -> Option<&Unit> {
        match self {
            Self::Added(unit) => Some(unit),
            _ => None,
        }
    }
    pub(crate) fn contains(&self, id: UnitId) -> bool {
        match self {
            Self::Existing(exits) => exits.contains(id),
            _ => false,
        }
    }
    pub(crate) fn replacements(&self) -> impl Iterator<Item = &Unit> {
        match self {
            Self::Existing(exits) => Some(*exits),
            _ => None,
        }
        .into_iter()
        .flat_map(ExistingObservers::replacements)
    }
    pub(crate) fn units<'a>(&'a self, game: &'a Game) -> impl Iterator<Item = &'a Unit> {
        game.units
            .values()
            .map(move |unit| self.replace(unit))
            .chain(self.added())
    }
}
