//! Room-owned commanders consume only the snapshots selected by room delivery.
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use ww2_core::{
    ai_schedule::{Beat, Brain, DeliveredSeat, Schedule, Seat},
    ai_view::{CommanderViewMemory, PublicMap, SeatMemory},
    commander_ai::Level,
    match_commands::Command,
    match_runtime::Match,
    orders::{OrderKey, QueueError},
    recipient_snapshot::SelectedSeat,
    seat_commander::{Advance, Delivered, SeatCommander},
};

#[derive(Clone)]
struct Consumer {
    memory: SeatMemory,
    policy_view: CommanderViewMemory,
    commander: SeatCommander,
    full: bool,
    shots: Vec<ww2_core::match_human_observation::HumanShot>,
    generation: u64,
    cells: BTreeMap<u64, Value>,
    #[cfg(test)]
    decoded: Option<ww2_core::ai_view::SeatView>,
}
#[cfg(test)]
#[derive(Clone, Debug)]
pub(crate) struct DecisionTrace {
    pub tick: i64,
    pub slot: usize,
    pub accepted: Vec<(OrderKey, Command)>,
}
#[derive(Default)]
pub(super) struct RoomAi {
    #[cfg(test)]
    trace: Vec<DecisionTrace>,
    schedule: Schedule,
    consumers: BTreeMap<usize, Consumer>,
    delivered: Vec<DeliveredSeat<Delivered>>,
    fresh: BTreeMap<usize, Delivered>,
}
impl RoomAi {
    #[cfg(test)]
    pub fn take_trace(&mut self) -> Vec<DecisionTrace> {
        std::mem::take(&mut self.trace)
    }

    fn save_memories_with(
        &self,
        seats: usize,
        mut encode: impl FnMut(&Consumer) -> Result<Value, String>,
    ) -> Value {
        if self.consumers.is_empty() {
            return Value::Null;
        }
        // The prototype retries the whole save without AI memory if any clone fails.
        (0..seats)
            .map(|slot| self.consumers.get(&slot).map(&mut encode).transpose())
            .collect::<Result<Vec<_>, _>>()
            .map_or(Value::Null, |memories| json!(memories))
    }

    pub fn save_memories(&self, seats: usize) -> Value {
        self.save_memories_with(seats, |consumer| {
            Ok(json!({
                "recipient":consumer.memory.planner_memory_json()?,
                "view":consumer.policy_view.planner_memory_json()?,
                "planner":consumer.commander.planner_memory_json()?,
                "cells":consumer.cells,
            }))
        })
    }

    pub fn restore_memories(&mut self, saved: &Value, seats: usize) -> Result<(), String> {
        if saved.is_null() {
            return Ok(());
        }
        let memories = saved
            .as_array()
            .filter(|m| m.len() == seats)
            .ok_or("invalid saved planner seat count")?;
        let mut staged = self.consumers.clone();
        for (&slot, consumer) in &mut staged {
            let memory = &memories[slot];
            if memory.is_null() {
                continue;
            }
            consumer
                .memory
                .restore_planner_memory(&memory["recipient"])?;
            consumer
                .policy_view
                .restore_planner_memory(&memory["view"])?;
            consumer
                .commander
                .restore_planner_memory(&memory["planner"])?;
            consumer.cells = serde_json::from_value(memory["cells"].clone())
                .map_err(|e| format!("saved planner terrain deltas: {e}"))?;
            #[cfg(test)]
            {
                consumer.decoded = None;
            }
        }
        self.consumers = staged;
        // Packets and attention state restart with an opening look after load.
        self.fresh.clear();
        for entry in &mut self.delivered {
            entry.view = None;
        }
        Ok(())
    }

    #[cfg(test)]
    pub fn level(&self, slot: usize) -> Option<Level> {
        self.delivered
            .iter()
            .find(|entry| entry.seat.slot == slot)
            .map(|entry| entry.seat.level)
    }

    #[cfg(test)]
    pub fn commander(&self, slot: usize) -> Option<&SeatCommander> {
        self.consumers.get(&slot).map(|c| &c.commander)
    }

    #[cfg(test)]
    pub fn clone_failure(&self, seats: usize) -> Value {
        let mut attempts = 0;
        let result = self.save_memories_with(seats, |consumer| {
            attempts += 1;
            if attempts == 2 {
                return Err("uncloneable seat memory".into());
            }
            consumer.commander.planner_memory_json()
        });
        assert_eq!(attempts, 2);
        result
    }

    #[cfg(test)]
    pub fn delivered_view(
        &self,
        slot: usize,
    ) -> Option<&ww2_core::human_commander::HumanView<ww2_core::commander_ai::Observation>> {
        match self
            .delivered
            .iter()
            .find(|entry| entry.seat.slot == slot)?
            .view
            .as_ref()?
        {
            Delivered::Human(view) => Some(view),
            Delivered::Horde(_) | Delivered::HordePolicy { .. } => None,
        }
    }
    #[cfg(test)]
    pub fn decoded_view(&self, slot: usize) -> Option<&ww2_core::ai_view::SeatView> {
        self.consumers.get(&slot)?.decoded.as_ref()
    }

    pub fn activate(
        &mut self,
        state: &Match,
        map: &Value,
        slot: usize,
        level: Level,
    ) -> Result<(), String> {
        let horde = state.horde.as_ref().is_some_and(|horde| horde.slot == slot);
        let level = if horde { Level::Normal } else { level };
        let public = PublicMap {
            width: state.world.game.w,
            height: state.world.game.h,
            players: state
                .start
                .players
                .iter()
                .map(|p| {
                    json!({
                        "name":p.config.name,"team":p.config.team,"faction":p.config.faction as u8,
                        "spawn":{"x":p.spawn.x,"z":p.spawn.z}
                    })
                })
                .collect(),
            points: state
                .start
                .points
                .iter()
                .map(|p| json!({"x":p.at.x,"z":p.at.z,"vp":p.vp,"mp":p.mp}))
                .collect(),
        };
        let chars = map["rows"]
            .as_array()
            .ok_or("AI public terrain missing")?
            .iter()
            .map(|row| row.as_str().ok_or("AI public terrain row"))
            .collect::<Result<Vec<_>, _>>()?
            .concat()
            .into_bytes();
        let memory = SeatMemory::new(slot, public.clone())?;
        let elevation = map
            .get("heights")
            .map(|heights| {
                heights
                    .as_array()
                    .ok_or("AI public height grid")?
                    .iter()
                    .map(|row| row.as_str().ok_or("AI public height row"))
                    .collect::<Result<Vec<_>, _>>()?
                    .concat()
                    .bytes()
                    .map(|ch| ww2_core::map::level_of(ch).ok_or("AI public height character"))
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()?;
        let memory = memory.with_terrain(chars.clone(), elevation.clone(), map["naval"] == true)?;
        let policy_view =
            CommanderViewMemory::new(slot, public, chars, elevation, map["naval"] == true)?;
        let commander = state.create_seat_commander(slot, level)?;
        self.consumers.insert(
            slot,
            Consumer {
                memory,
                policy_view,
                commander,
                full: true,
                shots: Vec::new(),
                generation: 0,
                cells: BTreeMap::new(),
                #[cfg(test)]
                decoded: None,
            },
        );
        self.delivered.retain(|entry| entry.seat.slot != slot);
        self.delivered.push(DeliveredSeat {
            seat: Seat {
                slot,
                level,
                brain: if horde {
                    Brain::Horde
                } else {
                    Brain::Commander
                },
            },
            view: None,
        });
        self.delivered.sort_by_key(|entry| entry.seat.slot);
        self.fresh.remove(&slot);
        Ok(())
    }
    pub fn reset(&mut self, slot: usize) {
        self.consumers.remove(&slot);
        self.delivered.retain(|entry| entry.seat.slot != slot);
        self.fresh.remove(&slot);
    }
    pub fn selected(&self, tick: i64) -> Vec<SelectedSeat> {
        self.consumers
            .iter()
            .filter(|_| tick >= 0)
            .map(|(&slot, consumer)| SelectedSeat {
                slot,
                full: consumer.full,
                omit: true,
            })
            .collect()
    }
    pub fn observe(
        &mut self,
        slot: usize,
        snapshot: &Value,
        scheduled: bool,
    ) -> Result<(), String> {
        let Some(consumer) = self.consumers.get_mut(&slot) else {
            return Ok(());
        };
        // Current/catch-up frames may seed a newly activated seat, but later
        // observations belong to the room's actual scheduled delivery beat.
        if !scheduled && consumer.generation > 0 {
            return Ok(());
        }
        let tick = snapshot["tick"]
            .as_i64()
            .ok_or("AI snapshot clock missing")?;
        for row in snapshot["cells"].as_array().into_iter().flatten() {
            let cell = row[0].as_u64().ok_or("AI delivered cell identity")?;
            consumer.cells.insert(cell, row.clone());
        }
        consumer.shots = snapshot["shots"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|shot| {
                if shot["k"] == "hurt"
                    && let (Some(owner), Some(unit), Some(x), Some(z)) = (
                        shot["to"].as_u64(),
                        shot["t"].as_u64(),
                        shot["x"].as_f64(),
                        shot["z"].as_f64(),
                    )
                {
                    return ww2_core::match_human_observation::HumanShot::Hurt {
                        owner: owner as usize,
                        unit: unit as u32,
                        at: ww2_core::game::Point { x, z },
                    };
                }
                ww2_core::match_human_observation::HumanShot::Other
            })
            .collect();
        consumer.generation = consumer
            .generation
            .checked_add(1)
            .ok_or("AI delivered frame generation exhausted")?;
        let entry = self
            .delivered
            .iter_mut()
            .find(|entry| entry.seat.slot == slot)
            .expect("AI seat registered");
        if consumer.commander.wants_view(tick, true) || entry.view.is_none() {
            // Keep delivered terrain deltas between looks, without learning
            // transient enemy sightings that the commander never observed.
            let mut observation = snapshot.clone();
            observation["cells"] = json!(consumer.cells.values().collect::<Vec<_>>());
            let view = consumer.memory.decode(&observation)?;
            let delivered = if entry.seat.brain == Brain::Horde {
                let (view, facts) = view.horde_packet(slot)?;
                Delivered::HordePolicy {
                    view,
                    facts: Box::new(facts),
                }
            } else {
                Delivered::Human(Box::new(consumer.policy_view.observe(&view, &observation)?))
            };
            #[cfg(test)]
            {
                consumer.decoded = Some(view);
            }
            consumer.cells.clear();
            entry.view = Some(delivered.clone());
            self.fresh.insert(slot, delivered);
        }
        consumer.full = false;
        Ok(())
    }
    pub fn tick(&mut self, state: &mut Match, sent: bool) -> Result<(), String> {
        let tick = state.world.game.tick;
        let wants = self
            .consumers
            .iter()
            .filter(|(_, c)| c.commander.wants_view(tick, sent))
            .map(|(&slot, _)| slot)
            .collect::<BTreeSet<_>>();
        let fresh = self.fresh.keys().copied().collect::<BTreeSet<_>>();
        let mut active = BTreeSet::new();
        self.schedule
            .tick(
                Beat {
                    tick,
                    sent,
                    finished: state.finish.is_some(),
                },
                &mut self.delivered,
                |seat| wants.contains(&seat.slot),
                |seat| {
                    self.fresh
                        .remove(&seat.slot)
                        .expect("selected AI snapshot delivered before planning")
                },
                |seat, _| {
                    active.insert(seat.slot);
                },
            )
            .map_err(|e| format!("room AI schedule: {e:?}"))?;
        // Every Horde delivery drains pending ground orders, including deliveries
        // between the director's Normal think ticks.
        if sent && state.finish.is_none() {
            active.extend(
                self.delivered
                    .iter()
                    .filter(|entry| {
                        entry.seat.brain == Brain::Horde && fresh.contains(&entry.seat.slot)
                    })
                    .map(|entry| entry.seat.slot),
            );
        }
        for slot in active {
            let packet = fresh
                .contains(&slot)
                .then(|| {
                    self.delivered
                        .iter()
                        .find(|entry| entry.seat.slot == slot)
                        .unwrap()
                        .view
                        .clone()
                })
                .flatten();
            let consumer = self.consumers.get_mut(&slot).expect("active commander");
            let advance = state.advance_seat_commander(
                &mut consumer.commander,
                packet,
                consumer.generation,
                &consumer.shots,
            )?;
            let commands = match advance {
                Advance::Human(advance) => advance
                    .input
                    .and_then(|input| input.command)
                    .into_iter()
                    .filter_map(|raw| Command::from_json(&raw).ok())
                    .collect::<Vec<_>>(),
                Advance::Horde(commands) => commands,
            };
            #[cfg(test)]
            let mut trace = DecisionTrace {
                tick,
                slot,
                accepted: Vec::new(),
            };
            for command in commands {
                let next = (tick as u64).checked_add(1).ok_or("AI tick exhausted")?;
                let sequence = state
                    .commands
                    .pending_tick(next)
                    .filter(|(key, _)| key.issuer == slot as u32)
                    .map(|(key, _)| key.sequence)
                    .max()
                    .unwrap_or(0)
                    .checked_add(1)
                    .ok_or("AI sequence exhausted")?;
                let key = OrderKey {
                    tick: next,
                    issuer: slot as u32,
                    sequence,
                };
                #[cfg(test)]
                let recorded = command.clone();
                match state.enqueue_command(key, command) {
                    Ok(()) => {
                        #[cfg(test)]
                        trace.accepted.push((key, recorded));
                    }
                    Err(QueueError::Full) => {}
                    Err(error) => return Err(format!("room AI admission: {error:?}")),
                }
            }
            #[cfg(test)]
            self.trace.push(trace);
        }
        self.fresh.clear();
        Ok(())
    }
}
