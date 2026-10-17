//! Caller-fed AI delivery scheduling, independent of Match authority and transport.
//! Views belong to the caller. This module neither constructs observations nor admits seats.
use crate::commander_ai::Level;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Brain {
    Commander,
    Scripted,
    Horde,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Seat {
    pub slot: usize,
    pub level: Level,
    pub brain: Brain,
}

pub struct DeliveredSeat<V> {
    pub seat: Seat,
    pub view: Option<V>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScheduleError {
    InvalidTick,
    DuplicateSeat,
}

/// Host-private replay input. Record the beat selected by the host, then feed
/// that same value back during replay rather than measuring host load again.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Beat {
    pub tick: i64,
    pub sent: bool,
    pub finished: bool,
}

#[derive(Default)]
pub struct Schedule {
    last_tick: Option<i64>,
}

impl Schedule {
    /// Refresh every requested view before invoking any brain. Commanders act every
    /// tick; scripted and Horde brains use the shared difficulty/seat stagger.
    /// `sent` is the caller's delivery beat, not a newly calculated cadence.
    pub fn tick<V>(
        &mut self,
        beat: Beat,
        seats: &mut [DeliveredSeat<V>],
        mut commander_wants_view: impl FnMut(Seat) -> bool,
        mut observe: impl FnMut(Seat) -> V,
        mut act: impl FnMut(Seat, &V),
    ) -> Result<(), ScheduleError> {
        let Beat {
            tick,
            sent,
            finished,
        } = beat;
        if !(0..=9_007_199_254_740_991).contains(&tick)
            || self.last_tick.is_some_and(|last| tick <= last)
        {
            return Err(ScheduleError::InvalidTick);
        }
        for (index, entry) in seats.iter().enumerate() {
            if seats[..index]
                .iter()
                .any(|old| old.seat.slot == entry.seat.slot)
            {
                return Err(ScheduleError::DuplicateSeat);
            }
        }
        self.last_tick = Some(tick);
        if finished {
            return Ok(());
        }
        if sent {
            for entry in seats.iter_mut() {
                let seat = entry.seat;
                let wants = match seat.brain {
                    Brain::Commander => commander_wants_view(seat),
                    Brain::Scripted => seat.level.wants_view(tick, seat.slot),
                    Brain::Horde => true,
                };
                if wants {
                    entry.view = Some(observe(seat));
                }
            }
        }
        for entry in seats {
            if let Some(view) = &entry.view
                && (entry.seat.brain == Brain::Commander
                    || entry.seat.level.due(tick, entry.seat.slot))
            {
                act(entry.seat, view);
            }
        }
        Ok(())
    }

    /// Clear the prior match's delivery receipts and clock before reusing seats.
    pub fn reset<V>(&mut self, seats: &mut [DeliveredSeat<V>]) {
        self.last_tick = None;
        for entry in seats {
            entry.view = None;
        }
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod replay_tests;
