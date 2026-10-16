//! Core AI room phase: deliver every requested seat view before any seat acts.
//! The host advances the match first and supplies its actual snapshot beat.

use std::collections::BTreeSet;

/// A brain owns private seat state. Observation must use the recipient path;
/// action submits through the same admission path as a person's orders.
pub trait Brain<C> {
    type View;
    type Output;
    type Error;

    fn wants_view(&self, context: &C, slot: usize) -> bool;
    fn observe(&mut self, context: &mut C, slot: usize) -> Result<Self::View, Self::Error>;
    fn act(
        &mut self,
        context: &mut C,
        slot: usize,
        delivered: &Self::View,
    ) -> Result<Self::Output, Self::Error>;
}

#[derive(Clone, Debug)]
struct Seat<B, V> {
    slot: usize,
    brain: B,
    delivered: Option<V>,
}

/// The supplied order is stable across both loops. A newly attached seat has
/// no delivered view, so it waits for a snapshot beat before it may act.
#[derive(Clone, Debug)]
pub struct Schedule<B, V> {
    seats: Vec<Seat<B, V>>,
}

impl<B, V> Schedule<B, V> {
    pub fn new(seats: impl IntoIterator<Item = (usize, B)>) -> Result<Self, &'static str> {
        let mut slots = BTreeSet::new();
        let mut ordered = Vec::new();
        for (slot, brain) in seats {
            if !slots.insert(slot) {
                return Err("duplicate AI seat");
            }
            ordered.push(Seat {
                slot,
                brain,
                delivered: None,
            });
        }
        Ok(Self { seats: ordered })
    }

    /// Call after the simulation step. An unsent tick retains the last view.
    /// Observation errors prevent the action phase; action errors return to
    /// the host, which owns recovery and must not retry committed actions.
    pub fn after_step<C>(
        &mut self,
        context: &mut C,
        sent: bool,
    ) -> Result<Vec<(usize, B::Output)>, B::Error>
    where
        B: Brain<C, View = V>,
    {
        let mut fresh = Vec::new();
        for (index, seat) in self.seats.iter_mut().enumerate() {
            if sent && seat.brain.wants_view(context, seat.slot) {
                fresh.push((index, seat.brain.observe(context, seat.slot)?));
            }
        }
        for (index, view) in fresh {
            self.seats[index].delivered = Some(view);
        }
        let mut outputs = Vec::new();
        for seat in &mut self.seats {
            if let Some(view) = &seat.delivered {
                outputs.push((seat.slot, seat.brain.act(context, seat.slot, view)?));
            }
        }
        Ok(outputs)
    }
}

#[cfg(test)]
#[path = "ai_schedule/tests.rs"]
mod tests;
