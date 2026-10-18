//! Room-owned private planners and delivered observation memory.
use ww2_core::{
    commander_ai::Level, match_human_observation::HumanObservationMemory, match_runtime::Match,
    seat_commander::SeatCommander,
};

#[derive(Clone, Debug, PartialEq)]
pub(super) struct RoomHelpers {
    pub(super) pairs: Vec<(SeatCommander, HumanObservationMemory)>,
    pub(super) levels: Vec<Option<Level>>,
}

impl RoomHelpers {
    pub(super) fn new(state: &Match, levels: &[Option<Level>]) -> Result<Self, String> {
        if levels.len() != state.start.players.len() {
            return Err("AI roster differs from match seats".into());
        }
        let pairs = levels
            .iter()
            .enumerate()
            .map(|(slot, level)| {
                Ok((
                    state.create_seat_commander(slot, level.unwrap_or_default())?,
                    HumanObservationMemory::new(slot),
                ))
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(Self {
            pairs,
            levels: levels.to_vec(),
        })
    }

    pub(super) fn reset_human(&mut self, state: &Match, slot: usize) -> Result<(), String> {
        if self.levels.get(slot) != Some(&None) {
            return Err("helper reset requires a human seat".into());
        }
        let fresh = (
            state.create_seat_commander(slot, Level::default())?,
            HumanObservationMemory::new(slot),
        );
        *self.pairs.get_mut(slot).ok_or("missing human helper")? = fresh;
        Ok(())
    }
}
