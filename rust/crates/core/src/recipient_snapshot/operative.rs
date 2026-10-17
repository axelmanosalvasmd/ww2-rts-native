use super::{DeliveryState, Match, SelectedSeat, SnapshotError, Value};
use crate::{match_routing::RecipientRouting, match_tick::TickEvent, world::Recipient};

/// A companion's independent terrain memory, outbox and fog acknowledgments.
/// Visibility still comes from the attached commander's ordinary recipient view.
#[derive(Clone, Debug)]
pub struct OperativeDelivery {
    slot: usize,
    routing: RecipientRouting,
    delivery: DeliveryState,
}
impl OperativeDelivery {
    pub fn new(state: &Match, slot: usize) -> Result<Self, SnapshotError> {
        let routing = state
            .recipients
            .get(slot)
            .ok_or(SnapshotError::InvalidRecipient)?
            .independent_delivery();
        Ok(Self {
            slot,
            routing,
            delivery: DeliveryState::new(state)?,
        })
    }

    pub fn capture(
        &mut self,
        state: &mut Match,
        full: bool,
        events: &[TickEvent],
    ) -> Result<Value, SnapshotError> {
        if self.slot >= state.recipients.len() {
            return Err(SnapshotError::InvalidRecipient);
        }
        // operativeView revisits every logged index on each send. Its terrain
        // acknowledgments must never consume the commander's pending changes.
        let recipient = Recipient::Player(self.slot);
        let pending = Some((0..state.world.cell_log().len()).collect());
        let commander_pending = state.world.replace_pending(recipient, pending);
        std::mem::swap(&mut state.recipients[self.slot], &mut self.routing);
        let result = (|| {
            let frame = self
                .delivery
                .capture_selected_with_tick_events(
                    state,
                    &[SelectedSeat {
                        slot: self.slot,
                        full,
                        omit: false,
                    }],
                    events,
                )?
                .ok_or(SnapshotError::InvalidRecipient)?;
            Ok(frame.recipient(self.slot)?.into_json())
        })();
        std::mem::swap(&mut state.recipients[self.slot], &mut self.routing);
        let _ = state.world.replace_pending(recipient, commander_pending);
        result
    }
}

#[cfg(test)]
#[path = "operative_tests.rs"]
mod tests;
