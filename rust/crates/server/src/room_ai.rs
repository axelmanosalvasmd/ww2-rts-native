//! Room-owned private planners and delivered observation memory.
use serde_json::Value;
use ww2_core::{
    commander_ai::Level,
    match_commands::Command,
    match_human_observation::{HumanObservationMemory, HumanShot},
    match_runtime::Match,
    match_tick::TickEvent,
    orders::{OrderKey, QueueError},
    recipient_snapshot::SUPPORTED_SUPPORT_KINDS,
    seat_commander::{Advance, Delivered, SeatCommander},
};

#[derive(Clone, Debug, PartialEq)]
pub(super) struct RoomAi {
    pub(super) pairs: Vec<(SeatCommander, HumanObservationMemory)>,
    pub(super) levels: Vec<Option<Level>>,
}

impl RoomAi {
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

    pub(super) fn save(&self, state: &Match) -> Result<Value, String> {
        if self.levels.iter().all(Option::is_none) {
            return Ok(Value::Null);
        }
        let pairs = self
            .pairs
            .iter()
            .zip(&self.levels)
            .filter_map(|((seat, observation), level)| level.map(|_| (seat, observation)))
            .collect::<Vec<_>>();
        state.save_ai_memories_json(&pairs)
    }

    pub(super) fn restore(&mut self, state: &Match, value: &Value) -> Result<(), String> {
        let slots = self
            .levels
            .iter()
            .enumerate()
            .filter_map(|(slot, level)| level.map(|level| (slot, level)))
            .collect::<Vec<_>>();
        let restored = state.restore_ai_memories_json(value, &slots)?;
        let mut fresh = Self::new(state, &self.levels)?;
        for ((slot, _), pair) in slots.into_iter().zip(restored) {
            fresh.pairs[slot] = pair;
        }
        *self = fresh;
        Ok(())
    }

    pub(super) fn advance(
        &mut self,
        state: &mut Match,
        sent: bool,
        events: &[TickEvent],
    ) -> Result<(), String> {
        let tick = state.world.game.tick;
        let generation = u64::try_from(tick).map_err(|_| "invalid AI tick")?;
        if self.pairs.len() != state.start.players.len() || self.levels.len() != self.pairs.len() {
            return Err("AI roster differs from match seats".into());
        }
        if state.finish.is_some() {
            return Ok(());
        }
        let next_tick = generation.checked_add(1).ok_or("AI tick exhausted")?;
        state.validate_command_tick(next_tick)?;
        let mut delivered = vec![None; self.pairs.len()];
        // Every requested observation precedes every planner callback.
        for (slot, (seat, memory)) in self.pairs.iter_mut().enumerate() {
            if self.levels[slot].is_some() && seat.wants_view(tick, sent) {
                delivered[slot] = Some(Delivered::Human(Box::new(
                    state.human_commander_observation(memory)?,
                )));
            }
        }
        let shots = events.iter().map(HumanShot::tick).collect::<Vec<_>>();
        for (slot, (seat, _)) in self.pairs.iter_mut().enumerate() {
            if self.levels[slot].is_none() {
                continue;
            }
            let result =
                state.advance_seat_commander(seat, delivered[slot].take(), generation, &shots)?;
            let Advance::Human(result) = result else {
                return Err("ordinary room requires a human-style AI commander".into());
            };
            let Some(raw) = result.input.and_then(|input| input.command) else {
                continue;
            };
            let Ok(command) = Command::from_json(&raw) else {
                continue;
            };
            if let Command::Support { kind, .. } = &command
                && !SUPPORTED_SUPPORT_KINDS
                    .iter()
                    .any(|supported| supported.name() == kind)
            {
                continue;
            }
            let issuer = u32::try_from(slot).map_err(|_| "invalid AI issuer")?;
            let sequence = state
                .commands
                .pending_tick(next_tick)
                .filter(|(key, _)| key.issuer == issuer)
                .map(|(key, _)| key.sequence)
                .max()
                .unwrap_or(0)
                .checked_add(1)
                .ok_or("AI input sequence exhausted")?;
            // The canonical next-tick dispatcher decides affordability and eligibility.
            match state.enqueue_command(
                OrderKey {
                    tick: next_tick,
                    issuer,
                    sequence,
                },
                command,
            ) {
                Ok(()) | Err(QueueError::Full) => {}
                Err(error) => return Err(format!("AI command queue: {error:?}")),
            }
        }
        Ok(())
    }
}
