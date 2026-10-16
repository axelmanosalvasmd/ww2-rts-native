use super::*;
use crate::{environment::Effect, recipient_lists::RecipientLists};

/// Ordered seat requests from the caller's readiness/backpressure selection.
/// Full is a reconnect read; omit suppresses only the browser fog mask.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SelectedSeat {
    pub slot: usize,
    pub full: bool,
    pub omit: bool,
}
pub type SelectedDeliveryFrame<'a> = DeliveryFrame<'a>;
pub struct SelectedGroundListFrame<'a> {
    frame: DeliveryFrame<'a>,
    lists: Vec<(usize, RecipientLists)>,
    _outside_events: &'a [Effect],
}
impl DeliveryState {
    /// Empty selection bypasses the cache and every producer, as the source
    /// room loop does when no recipient can receive this scheduled frame.
    pub fn capture_selected<'a>(
        &mut self,
        state: &'a mut Match,
        selected: &[SelectedSeat],
    ) -> Result<Option<SelectedDeliveryFrame<'a>>, SnapshotError> {
        if selected.is_empty() {
            return Ok(None);
        }
        self.preflight_selected(state, selected)?;
        Ok(Some(self.capture_admitted(state, selected)?))
    }
    /// Prepare only selected ordinary recipients. Every selected terrain and
    /// list producer admits before the sole remembered-list publication.
    /// Skipped seats retain their memory, outbox and delivery acknowledgments.
    pub fn capture_selected_with_lists<'a>(
        &mut self,
        state: &'a mut Match,
        selected: &[SelectedSeat],
        outside_events: &'a [Effect],
    ) -> Result<Option<SelectedGroundListFrame<'a>>, SnapshotError> {
        if selected.is_empty() {
            return Ok(None);
        }
        self.preflight_selected(state, selected)?;
        composition::admit_actors(state, outside_events)?;
        let owners = selected.iter().map(|seat| seat.slot).collect::<Vec<_>>();
        let prepared = state
            .prepare_ordinary_recipient_lists(&owners)
            .map_err(|_| SnapshotError::Rules)?;
        if prepared.results().len() != owners.len()
            || prepared
                .results()
                .iter()
                .zip(&owners)
                .any(|((slot, _), owner)| slot != owner)
        {
            return Err(SnapshotError::UnsupportedMode);
        }
        let (lists, commit) = prepared
            .prepare_publication_with_results(
                crate::match_routing::OrdinaryListPublication {
                    world: &state.world,
                    environment: &state.environment,
                    options: &state.options,
                },
                &mut state.recipients,
            )
            .map_err(|_| SnapshotError::Rules)?;
        commit.commit();
        Ok(Some(SelectedGroundListFrame {
            frame: self.capture_admitted(state, selected)?,
            lists,
            _outside_events: outside_events,
        }))
    }
}
impl SelectedGroundListFrame<'_> {
    pub fn slots(&self) -> impl Iterator<Item = usize> + '_ {
        self.frame.slots()
    }
    pub fn recipient(&self, slot: usize) -> Result<SupportedSeatSnapshot, SnapshotError> {
        let projection = self.frame.recipient(slot)?;
        let lists = self
            .lists
            .iter()
            .find(|(owner, _)| *owner == slot)
            .ok_or(SnapshotError::InvalidRecipient)?;
        Ok(composition::complete(projection, Some(&lists.1)))
    }
}

#[cfg(test)]
#[path = "selected_tests.rs"]
mod tests;
