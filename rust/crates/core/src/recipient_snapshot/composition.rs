use super::*;
use crate::environment::Effect;
use crate::recipient_lists::RecipientLists;
#[cfg(test)]
#[path = "composition/active_lists_tests.rs"]
mod active_list_tests;

/// Complete internal seat result for the explicitly admitted Conquest
/// frame. It does not implement a transport, spectator or unit delta policy.
#[derive(Clone, Debug, PartialEq)]
pub struct SupportedSeatSnapshot {
    fields: Map<String, Value>,
}
impl SupportedSeatSnapshot {
    pub fn into_json(self) -> Value {
        Value::Object(self.fields)
    }
}
pub struct QuiescentDeliveryFrame<'a> {
    frame: DeliveryFrame<'a>,
    _outside_events: &'a [Effect],
}
pub struct GroundListDeliveryFrame<'a> {
    frame: DeliveryFrame<'a>,
    lists: Vec<RecipientLists>,
    _outside_events: &'a [Effect],
}
impl DeliveryState {
    /// Collect real ordinary list results after every row, actor and lazy
    /// terrain producer has admitted. Canonical batch publication is the sole
    /// wreck-memory writer; network list versions/acknowledgments stay separate.
    pub fn capture_with_lists<'a>(
        &mut self,
        state: &'a mut Match,
        full: &[bool],
        omit: &[bool],
        outside_events: &'a [Effect],
    ) -> Result<GroundListDeliveryFrame<'a>, SnapshotError> {
        admit_actors(state, outside_events)?;
        self.preflight(state, full, omit)?;
        let count = state.world.game.players.len();
        let prepared = state
            .prepare_ordinary_recipient_lists(&(0..count).collect::<Vec<_>>())
            .map_err(|_| SnapshotError::Rules)?;
        let mut lists = Vec::with_capacity(count);
        if prepared.results().len() != count {
            return Err(SnapshotError::InvalidRecipient);
        }
        for (slot, (owner, list)) in prepared.results().iter().enumerate() {
            if slot != *owner || list.nodes.is_some() {
                return Err(SnapshotError::UnsupportedMode);
            }
            lists.push(list.clone());
        }
        state
            .publish_ordinary_recipient_lists(prepared)
            .map_err(|_| SnapshotError::Rules)?;
        Ok(GroundListDeliveryFrame {
            frame: self.capture(state, full, omit)?,
            lists,
            _outside_events: outside_events,
        })
    }
    /// `outside_events` is the caller's current environment effect batch, not
    /// an invented per-seat event store. No global owner collection is drained.
    /// Persistent actors and every recipient's remembered lists also admit
    /// before mutable terrain/fog delivery.
    pub fn capture_quiescent<'a>(
        &mut self,
        state: &'a mut Match,
        full: &[bool],
        omit: &[bool],
        outside_events: &'a [Effect],
    ) -> Result<QuiescentDeliveryFrame<'a>, SnapshotError> {
        admit(state, outside_events)?;
        Ok(QuiescentDeliveryFrame {
            frame: self.capture(state, full, omit)?,
            _outside_events: outside_events,
        })
    }
}
impl QuiescentDeliveryFrame<'_> {
    pub fn recipient(&self, slot: usize) -> Result<SupportedSeatSnapshot, SnapshotError> {
        Ok(complete(self.frame.recipient(slot)?, None))
    }
}
impl GroundListDeliveryFrame<'_> {
    pub fn recipient(&self, slot: usize) -> Result<SupportedSeatSnapshot, SnapshotError> {
        let projection = self.frame.recipient(slot)?;
        let lists = self
            .lists
            .get(slot)
            .ok_or(SnapshotError::InvalidRecipient)?;
        Ok(complete(projection, Some(lists)))
    }
}
fn complete(
    mut projection: RecipientProjection,
    lists: Option<&RecipientLists>,
) -> SupportedSeatSnapshot {
    projection.fields.insert("t".into(), json!("s"));
    // Admission inspected each actual canonical actor/list owner. Their
    // absent output is a represented source result for this bounded frame.
    for name in ["air", "works", "falling", "flights", "shots"] {
        projection.fields.insert(name.into(), json!([]));
    }
    projection.fields.insert(
        "wrecks".into(),
        lists.map_or_else(
            || json!([]),
            |lists| {
                json!(
                    lists
                        .wrecks
                        .iter()
                        .map(crate::recipient_lists::WreckRow::to_json)
                        .collect::<Vec<_>>()
                )
            },
        ),
    );
    projection.fields.insert(
        "foundMines".into(),
        lists.map_or_else(|| json!([]), |lists| json!(lists.found_mines)),
    );
    SupportedSeatSnapshot {
        fields: projection.fields,
    }
}
fn admit(state: &Match, outside_events: &[Effect]) -> Result<(), SnapshotError> {
    admit_actors(state, outside_events)?;
    let refused = || SnapshotError::Unsupported {
        unit: 0,
        capability: SnapshotCapability::WrecksAndFoundMines,
    };
    let g = &state.world.game;
    if !g.wrecks.is_empty() {
        return Err(refused());
    }
    for slot in 0..g.players.len() {
        let routing = state
            .recipient_routing(slot)
            .map_err(|_| SnapshotError::Rules)?;
        if !routing.remembered_wrecks().is_empty()
            || state
                .world
                .mine_seen
                .iter()
                .any(|(cell, bits)| bits >> g.players[slot].team & 1 != 0 && g.chars[*cell] == b'N')
        {
            return Err(refused());
        }
    }
    Ok(())
}
fn admit_actors(state: &Match, outside_events: &[Effect]) -> Result<(), SnapshotError> {
    let cache = SnapshotCache::new(state)?;
    for slot in 0..state.world.game.players.len() {
        cache.recipient(slot)?;
    }
    let refused = |capability| SnapshotError::Unsupported {
        unit: 0,
        capability,
    };
    if !outside_events.is_empty()
        || !state.firing.events.is_empty()
        || !state.firing.flights.is_empty()
        || !state.firing.salvos.is_empty()
        || !state.firing.grenades.is_empty()
        || !state.support.events.is_empty()
    {
        return Err(refused(SnapshotCapability::ShotsAndFlights));
    }
    if state
        .environment
        .falling_sections()
        .iter()
        .any(|s| !s.body.settled)
    {
        return Err(refused(SnapshotCapability::TeamEffects));
    }
    // This verified ancestry has no mass-project provider. Active dig/build/
    // entrench units already refuse in SnapshotCache; the sole paid job owner
    // must also be empty, including orphaned jobs. Future project integration
    // must inspect its canonical listed jobs before broadening this admission.
    if !state.field_works.jobs().is_empty() {
        return Err(refused(SnapshotCapability::Engineering));
    }
    if state.scenario.is_some() {
        return Err(refused(SnapshotCapability::Scenario));
    }
    if state.options.logistics || state.logistics.iter().next().is_some() || state.convoys.is_some()
    {
        return Err(refused(SnapshotCapability::Logistics));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn complete_quiet_seat_fields_match_original_source_without_filling_unsupported_actors() {
        let fixture: Value =
            serde_json::from_str(include_str!("../../tests/fixtures/recipient-snapshot.json"))
                .unwrap();
        for case in fixture["cases"].as_array().unwrap() {
            let mut state = super::super::tests::initialize(case);
            let mut delivery = DeliveryState::new(&state).unwrap();
            let frame = delivery
                .capture_quiescent(&mut state, &[true; 3], &[false; 3], &[])
                .unwrap();
            for slot in 0..3 {
                super::super::tests::equal(
                    &frame.recipient(slot).unwrap().into_json(),
                    &case["fullOutputs"][slot],
                    &format!("{} full recipient{slot}", case["name"]),
                );
            }
        }
    }
    #[test]
    fn unrepresented_event_or_persisted_lists_preserve_delivery_acknowledgments() {
        let fixture: Value =
            serde_json::from_str(include_str!("../../tests/fixtures/recipient-delivery.json"))
                .unwrap();
        let mut state = super::super::tests::initialize(&fixture["initial"]);
        let mut delivery = DeliveryState::new(&state).unwrap();
        let before = delivery.clone();
        let event = crate::environment::Effect::LastHit { unit: 1, owner: 0 };
        assert!(matches!(
            delivery.capture_quiescent(&mut state, &[false; 3], &[false; 3], &[event]),
            Err(SnapshotError::Unsupported {
                capability: SnapshotCapability::ShotsAndFlights,
                ..
            })
        ));
        assert_eq!(delivery, before);
        state.world.mine_seen.insert(5 * 40 + 25, 1);
        assert!(matches!(
            delivery.capture_quiescent(&mut state, &[false; 3], &[false; 3], &[]),
            Err(SnapshotError::Unsupported {
                capability: SnapshotCapability::WrecksAndFoundMines,
                ..
            })
        ));
        assert_eq!(delivery, before);
        state.world.mine_seen.clear();
        state
            .world
            .mine_seen
            .insert(state.world.game.chars.len(), 1);
        assert!(matches!(
            delivery.capture_quiescent(&mut state, &[false; 3], &[false; 3], &[]),
            Err(SnapshotError::Rules)
        ));
        assert_eq!(delivery, before);
        state.world.mine_seen.clear();
        let frame = delivery
            .capture_quiescent(&mut state, &[false; 3], &[false; 3], &[])
            .unwrap();
        assert!(
            !frame.recipient(0).unwrap().into_json()["cells"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }
}
