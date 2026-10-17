//! AI memory lives beside the game checkpoint, as in the prototype's save body.
use super::*;
use crate::match_human_observation::HumanObservationMemory;
use serde_json::Value;

#[derive(serde::Serialize, serde::Deserialize)]
struct SavedSeat {
    owner: usize,
    binding: (u64, usize, usize, usize, Point),
    memory: Memory,
}
#[derive(serde::Serialize, serde::Deserialize)]
enum Memory {
    Human {
        policy: Box<HumanPolicy>,
        observation: HumanObservationMemory,
    },
    Horde(HordeCommander),
}

impl Match {
    /// Encode the private AI owner separately from `g`, without human camera/input state.
    /// The caller supplies only AI seats; resumed human seats must omit their old memory.
    pub fn save_ai_memories_json(
        &self,
        seats: &[(&SeatCommander, &HumanObservationMemory)],
    ) -> Result<Value, String> {
        let mut memories = vec![None; self.start.players.len()];
        for &(seat, observation) in seats {
            let binding = self.commander_binding(seat.owner)?;
            observation.validate_save_binding(self)?;
            if seat.binding != binding || observation.owner() != seat.owner {
                return Err("AI memory belongs to another match or seat".into());
            }
            if memories[seat.owner].is_some() {
                return Err("duplicate AI memory seat".into());
            }
            let memory = match &seat.brain {
                Brain::Human { policy, .. } => Memory::Human {
                    policy: policy.clone(),
                    observation: observation.clone(),
                },
                Brain::Horde(state) => Memory::Horde(state.clone()),
            };
            memories[seat.owner] = Some(crate::save_codec::to_value(&SavedSeat {
                owner: seat.owner,
                binding,
                memory,
            })?);
        }
        Ok(Value::Array(
            memories
                .into_iter()
                .map(|v| v.unwrap_or(Value::Null))
                .collect(),
        ))
    }

    /// Restore planner memory for the saved AI roster. Null v1 memory starts fresh.
    /// Human attention, pending inputs and delivered packets restart, matching
    /// `ai-human.js:35-65` and `server.js:447-448`; native route caches stay in `g`.
    pub fn restore_ai_memories_json(
        &self,
        value: &Value,
        slots: &[(usize, Level)],
    ) -> Result<Vec<(SeatCommander, HumanObservationMemory)>, String> {
        let memories = if value.is_null() {
            None
        } else {
            Some(
                value
                    .as_array()
                    .filter(|v| v.len() <= self.start.players.len())
                    .ok_or("invalid AI memory array")?,
            )
        };
        let mut restored = Vec::new();
        let mut owners = std::collections::BTreeSet::new();
        for &(owner, level) in slots {
            if !owners.insert(owner) {
                return Err("duplicate AI roster seat".into());
            }
            let mut seat = self.create_seat_commander(owner, level)?;
            let mut observation = HumanObservationMemory::new(owner);
            if let Some(saved) = memories.and_then(|v| v.get(owner)).filter(|v| !v.is_null()) {
                let saved: SavedSeat = crate::save_codec::from_value(saved.clone())?;
                if saved.owner != owner || saved.binding != seat.binding {
                    return Err("saved AI belongs to another match or seat".into());
                }
                match (saved.memory, &mut seat.brain) {
                    (
                        Memory::Human {
                            policy,
                            observation: memory,
                        },
                        Brain::Human { policy: next, .. },
                    ) if policy.save_owner() as usize == owner && memory.owner() == owner => {
                        memory.validate_save_binding(self)?;
                        *next = policy;
                        observation = memory;
                    }
                    (Memory::Horde(mut state), Brain::Horde(next))
                        if state.save_owner() == owner =>
                    {
                        state.reset_saved_delivery();
                        *next = state;
                    }
                    _ => return Err("saved AI brain or owner differs from roster".into()),
                }
            }
            restored.push((seat, observation));
        }
        Ok(restored)
    }
}

#[cfg(test)]
mod tests;
