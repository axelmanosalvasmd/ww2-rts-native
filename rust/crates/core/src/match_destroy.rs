//! Source scuttling keeps work/control state until the next actual retirement phase.
use crate::{match_commands::Denial, match_runtime::Match};

impl Match {
    pub(crate) fn execute_destroy(
        &mut self,
        owner: usize,
        ids: &[i64],
        scalar: Option<i64>,
    ) -> Result<Option<Denial>, String> {
        let game = &self.world.game;
        let limit = 50.max(game.units.len());
        let selected = ids
            .iter()
            .copied()
            .take(limit)
            .chain(scalar)
            .filter_map(|id| u32::try_from(id).ok())
            .filter(|id| {
                game.units
                    .get(id)
                    .is_some_and(|unit| unit.owner == owner && unit.hp > 0.0 && !unit.riding)
            })
            .collect::<Vec<_>>();
        if selected.is_empty() {
            return Ok(Some(Denial::Unseen));
        }
        for &id in &selected {
            if id == 0 || game.units[&id].id != id || self.combat.get(id).is_none() {
                return Err("invalid scuttle unit or combat identity".into());
            }
        }
        let last_hit = i32::try_from(owner).map_err(|_| "invalid scuttle owner")?;
        // Source list construction precedes mutation, including repeated IDs.
        for id in selected {
            self.world.game.units.get_mut(&id).unwrap().hp = 0.0;
            self.combat.get_mut(id).unwrap().last_hit = Some(last_hit);
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::match_commands::Command;
    use serde_json::{Value, json};

    #[test]
    fn scuttle_preserves_controls_and_refuses_atomically_without_a_combat_owner() {
        let fixture: Value =
            serde_json::from_str(include_str!("../tests/fixtures/recipient-snapshot.json"))
                .unwrap();
        let scene = fixture["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["name"] == "untouched-startup")
            .unwrap();
        let mut state = crate::recipient_snapshot::tests::initialize(scene);
        let owned = state
            .world
            .game
            .units
            .values()
            .filter(|unit| unit.owner == 0 && unit.hp > 0.0)
            .map(|unit| unit.id)
            .take(2)
            .collect::<Vec<_>>();
        assert_eq!(owned.len(), 2);
        let command =
            Command::from_json(&json!({"t":"destroy","ids":[owned[0]],"id":owned[1]})).unwrap();
        let controls = state.commands.units.clone();
        let works = state.field_works.clone();
        let wallets = format!("{:?}", state.start.economy.players);
        let hp = owned
            .iter()
            .map(|id| state.world.game.units[id].hp.to_bits())
            .collect::<Vec<_>>();
        let prior = state.combat.get(owned[0]).unwrap().clone();
        state.combat.remove(owned[1]);
        assert!(state.execute_command(0, &command, false).is_err());
        assert_eq!(state.combat.get(owned[0]), Some(&prior));
        assert_eq!(
            owned
                .iter()
                .map(|id| state.world.game.units[id].hp.to_bits())
                .collect::<Vec<_>>(),
            hp
        );
        state.combat.initialize(owned[1]);
        assert_eq!(state.execute_command(0, &command, false).unwrap(), None);
        for id in owned {
            assert_eq!(state.world.game.units[&id].hp, 0.0);
            assert_eq!(state.combat.get(id).unwrap().last_hit, Some(0));
        }
        assert_eq!(state.commands.units, controls);
        assert_eq!(state.field_works, works);
        assert_eq!(format!("{:?}", state.start.economy.players), wallets);
        assert_eq!(
            state.execute_command(0, &command, false).unwrap(),
            Some(Denial::Unseen)
        );
    }
}
