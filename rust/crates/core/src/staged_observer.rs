//! A prepared unpublished sight source, borrowed only while routing is staged.
use crate::{
    game::{Game, LOCAL_DEFENDER_OWNER, ModeKind, Unit},
    rules::Rules,
};

/// An admitted draft with its canonical future identity and source sight fields.
/// This does not insert a unit or retain a reference in fog or routing caches.
#[derive(Clone, Copy)]
pub struct StagedObserver<'a> {
    unit: &'a Unit,
}
impl<'a> StagedObserver<'a> {
    pub fn new(
        game: &Game,
        rules: &Rules,
        future_id: u32,
        owner: usize,
        unit: &'a Unit,
    ) -> Result<Self, String> {
        let valid_owner = owner < game.players.len()
            || (game.mode == Some(ModeKind::World) && owner == LOCAL_DEFENDER_OWNER);
        if !valid_owner
            || unit.owner != owner
            || future_id == 0
            || unit.id != future_id
            || game
                .units
                .last_key_value()
                .is_some_and(|(id, _)| *id >= future_id)
            || unit.kind >= rules.kinds.len()
            || !unit.x.is_finite()
            || !unit.z.is_finite()
            || !unit.rot.is_finite()
            || !unit.hp.is_finite()
            || unit.hp < 0.0
            || unit
                .built
                .is_some_and(|v| !v.is_finite() || !(0.0..=1.0).contains(&v))
            || unit.garrison.is_some_and(|c| c >= game.house.len())
            || unit
                .cells
                .as_ref()
                .is_some_and(|cs| cs.iter().any(|c| *c as usize >= game.chars.len()))
        {
            return Err("invalid staged sight observer".into());
        }
        Ok(Self { unit })
    }
    pub fn unit(self) -> &'a Unit {
        self.unit
    }
}
