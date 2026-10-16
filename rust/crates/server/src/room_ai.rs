//! Private brains retain recipient packets between the room's actual delivery beats.
use std::collections::{BTreeMap, BTreeSet};
use ww2_core::{
    ai_schedule::{Beat, Brain, DeliveredSeat, Schedule, Seat},
    ai_view::{SeatMemory, SeatView, commander::CommanderMemory},
    commander_ai::Level,
    match_commands::Command,
    match_human_observation::HumanShot,
    match_runtime::Match,
    match_tick::TickEvent,
    orders::{OrderKey, QueueError},
    recipient_snapshot::DeliveryState,
    seat_commander::{Advance, Delivered, SeatCommander},
};

pub(crate) struct RoomAi {
    schedule: Schedule,
    seats: Vec<DeliveredSeat<Result<SeatView, String>>>,
    memories: BTreeMap<usize, SeatMemory>,
    human: BTreeMap<usize, CommanderMemory>,
    commanders: BTreeMap<usize, SeatCommander>,
    consumed: BTreeMap<usize, i64>,
    #[cfg(test)]
    inputs: Vec<(usize, ww2_core::human_commander_state::InputReceipt)>,
}
impl RoomAi {
    pub(crate) fn new(
        state: &mut Match,
        delivery: &mut DeliveryState,
        configured: &[(usize, Level)],
    ) -> Result<Self, String> {
        let mut result = Self {
            schedule: Schedule::default(),
            seats: Vec::new(),
            memories: BTreeMap::new(),
            human: BTreeMap::new(),
            commanders: BTreeMap::new(),
            consumed: BTreeMap::new(),
            #[cfg(test)]
            inputs: Vec::new(),
        };
        let mut configured = configured.to_vec();
        if let Some(horde) = &state.horde {
            configured.push((horde.slot, Level::Normal));
        }
        configured.sort_by_key(|(slot, _)| *slot);
        if configured.windows(2).any(|pair| pair[0].0 == pair[1].0) {
            return Err("duplicate hosted AI seat".into());
        }
        for (slot, level) in configured {
            let horde = state.horde.as_ref().is_some_and(|h| h.slot == slot);
            let mut memory = state.ordinary_ai_view_memory(slot)?;
            // Prototype beginMatch seeds AI views from the same initial recipient boundary.
            let initial = state.ordinary_ai_view(&mut memory, delivery, true)?;
            result.seats.push(DeliveredSeat {
                seat: Seat {
                    slot,
                    level,
                    brain: if horde {
                        Brain::Horde
                    } else {
                        Brain::Commander
                    },
                },
                view: Some(Ok(initial)),
            });
            result.memories.insert(slot, memory);
            if !horde {
                result.human.insert(
                    slot,
                    CommanderMemory::new(slot, state.world.game.w, state.world.game.h)?,
                );
            }
            result
                .commanders
                .insert(slot, state.create_seat_commander(slot, level)?);
        }
        Ok(result)
    }
    pub(crate) fn tick(
        &mut self,
        state: &mut Match,
        delivery: &mut DeliveryState,
        sent: bool,
        generation: u64,
        events: &[TickEvent],
    ) -> Result<(), String> {
        let tick = state.world.game.tick;
        let finished = state.finish.is_some();
        let wants = self
            .commanders
            .iter()
            .filter(|(_, c)| c.wants_view(tick, sent))
            .map(|(&slot, _)| slot)
            .collect::<BTreeSet<_>>();
        let mut acted = BTreeSet::new();
        let mut observed = BTreeSet::new();
        let memories = &mut self.memories;
        self.schedule
            .tick(
                Beat {
                    tick,
                    sent,
                    finished,
                },
                &mut self.seats,
                |seat| wants.contains(&seat.slot),
                |seat| {
                    observed.insert(seat.slot);
                    state.ordinary_ai_view(
                        memories.get_mut(&seat.slot).expect("AI memory"),
                        delivery,
                        false,
                    )
                },
                |seat, _| {
                    acted.insert(seat.slot);
                },
            )
            .map_err(|e| format!("shared AI schedule: {e:?}"))?;
        let shots = events.iter().map(HumanShot::tick).collect::<Vec<_>>();
        for entry in &self.seats {
            let slot = entry.seat.slot;
            // Horde observations flush bounded queued work even outside a think tick.
            if !finished && !acted.contains(&slot) && !observed.contains(&slot) {
                continue;
            }
            let commander = self.commanders.get_mut(&slot).expect("AI brain");
            let packet = if !finished && let Some(view) = &entry.view {
                let view = view.as_ref().map_err(Clone::clone)?;
                if self.consumed.get(&slot) != Some(&view.tick) {
                    let packet = if entry.seat.brain == Brain::Horde {
                        let (view, facts) = view.horde_packet(slot)?;
                        Delivered::HordePolicy {
                            view,
                            facts: Box::new(facts),
                        }
                    } else {
                        Delivered::Human(Box::new(
                            self.human
                                .get_mut(&slot)
                                .expect("human memory")
                                .observe(view)?,
                        ))
                    };
                    self.consumed.insert(slot, view.tick);
                    Some(packet)
                } else {
                    None
                }
            } else {
                None
            };
            let advance = state.advance_seat_commander(commander, packet, generation, &shots)?;
            #[cfg(test)]
            if let Advance::Human(ref advance) = advance
                && let Some(input) = &advance.input
            {
                self.inputs.push((slot, input.clone()));
            }
            let commands = match advance {
                Advance::Horde(commands) => commands,
                Advance::Human(advance) => advance
                    .input
                    .and_then(|input| input.command)
                    .and_then(|raw| Command::from_json(&raw).ok())
                    .into_iter()
                    .collect(),
            };
            for command in commands {
                let next = (tick as u64).checked_add(1).ok_or("AI tick exhausted")?;
                let sequence = state
                    .commands
                    .pending_tick(next)
                    .filter(|(key, _)| key.issuer as usize == slot)
                    .map(|(key, _)| key.sequence)
                    .max()
                    .unwrap_or(0)
                    .checked_add(1)
                    .ok_or("AI sequence exhausted")?;
                match state.enqueue_command(
                    OrderKey {
                        tick: next,
                        issuer: slot as u32,
                        sequence,
                    },
                    command,
                ) {
                    Ok(()) | Err(QueueError::Full) => {}
                    Err(error) => return Err(format!("AI recorded input: {error:?}")),
                }
            }
        }
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn delivered(&self, slot: usize) -> Option<&SeatView> {
        self.seats
            .iter()
            .find(|s| s.seat.slot == slot)?
            .view
            .as_ref()?
            .as_ref()
            .ok()
    }
    #[cfg(test)]
    pub(crate) fn commander(&self, slot: usize) -> Option<&SeatCommander> {
        self.commanders.get(&slot)
    }
}

#[cfg(test)]
#[path = "room_ai_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "room_ai_combat_tests.rs"]
mod combat_tests;

#[cfg(test)]
#[path = "room_ai_seeded_tests.rs"]
mod seeded_tests;
