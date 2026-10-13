//! Borrow current actor facts without adding a World local defender to commander seats.
use crate::{
    economy::{Faction, Mode},
    game::{Game, LOCAL_DEFENDER_OWNER, ModeKind, Point},
    match_runtime::Match,
};

/// Resources remain in the existing commander economy. The local defender has
/// Source owner -1 and no economy slot; its Source MP, Munitions and fuel are zero.
pub struct ActorPolicy<'a> {
    pub owner: usize,
    pub source_owner: i32,
    pub economy_slot: Option<usize>,
    pub team: i32,
    pub faction: Faction,
    pub spawn: Point,
    pub visible: &'a [u32],
}
impl ActorPolicy<'_> {
    pub fn sees(&self, id: u32) -> bool {
        self.visible.contains(&id)
    }
}

/// Validate only the represented actor modes, not their complete tick capabilities.
pub(crate) fn checked_game_mode(game: &Game, mode: Mode) -> Result<(), String> {
    match (mode, game.mode) {
        (Mode::Default | Mode::Conquest, None) | (Mode::World, Some(ModeKind::World)) => Ok(()),
        _ => Err("actor mode does not match canonical Game mode".into()),
    }
}
pub(crate) fn checked_game_owner(game: &Game, owner: usize) -> Result<i32, String> {
    if game.mode == Some(ModeKind::World) && owner == LOCAL_DEFENDER_OWNER {
        Ok(-1)
    } else if game.players.get(owner).is_some_and(|p| p.slot == owner) {
        i32::try_from(owner).map_err(|_| "actor owner exceeds source range".into())
    } else {
        Err("actor owner has no canonical identity".into())
    }
}
pub(crate) fn checked_game_team(game: &Game, owner: usize) -> Result<i32, String> {
    checked_game_owner(game, owner)?;
    Ok(if owner == LOCAL_DEFENDER_OWNER {
        -1
    } else {
        game.players[owner].team
    })
}

impl Match {
    /// This is an actor lookup gate. The sole tick owner validates phase capabilities separately.
    pub(crate) fn validate_actor_mode(&self) -> Result<(), String> {
        checked_game_mode(&self.world.game, self.options.mode)?;
        if self.options.mode == Mode::World
            && (self.world_runtime.is_none() || self.world.geography().is_none())
        {
            return Err("World actor policy requires canonical runtime and geography".into());
        }
        Ok(())
    }
    pub fn actor_policy(&self, owner: usize) -> Result<ActorPolicy<'_>, String> {
        self.validate_actor_mode()?;
        let source_owner = checked_game_owner(&self.world.game, owner)?;
        if self.options.mode == Mode::World {
            let actor = self.world_actor_view(owner)?;
            Ok(ActorPolicy {
                owner,
                source_owner,
                economy_slot: (source_owner >= 0).then_some(owner),
                team: actor.team,
                faction: actor.faction,
                spawn: actor.spawn,
                visible: actor.visible,
            })
        } else {
            let actor = self
                .start
                .players
                .get(owner)
                .ok_or("missing canonical commander actor")?;
            let player = &self.world.game.players[owner];
            Ok(ActorPolicy {
                owner,
                source_owner,
                economy_slot: Some(owner),
                team: player.team,
                faction: actor.config.faction,
                spawn: actor.spawn,
                visible: &player.visible,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn local_actor_visibility_keeps_maintained_grid_order() {
        let visible = [9, 2, 7];
        let actor = ActorPolicy {
            owner: LOCAL_DEFENDER_OWNER,
            source_owner: -1,
            economy_slot: None,
            team: -1,
            faction: Faction::try_from(0).unwrap(),
            spawn: Point { x: 0., z: 0. },
            visible: &visible,
        };
        assert!(visible.iter().all(|&id| actor.sees(id)));
        assert!(!actor.sees(3));
        assert_eq!(actor.visible, &[9, 2, 7]);
    }
}
