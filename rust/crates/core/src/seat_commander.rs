//! Core brain dispatch. Hosting, AI seat lifecycle and input admission stay with their owners.
use crate::{
    commander_ai::{Level, Observation},
    game::Point,
    horde_commander::{HordeCommander, View},
    human_commander::{HumanView, TickControl},
    human_commander_policy::{self, HumanPolicy},
    human_commander_state::{HumanAdvance, HumanState},
    match_commands::Command,
    match_human_observation::HumanShot,
    match_runtime::Match,
};

#[derive(Clone, Debug, PartialEq)]
pub enum Brain {
    Human {
        state: Box<HumanState>,
        policy: Box<HumanPolicy>,
        delivered: Option<Box<HumanView<Observation>>>,
    },
    Horde(HordeCommander),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Delivered {
    Human(Box<HumanView<Observation>>),
    Horde(View),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Advance {
    Human(HumanAdvance),
    Horde(Vec<Command>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct SeatCommander {
    owner: usize,
    level: Level,
    binding: (u64, usize, usize, usize, Point),
    brain: Brain,
}
impl SeatCommander {
    pub fn brain(&self) -> &Brain {
        &self.brain
    }
    pub fn wants_view(&self, tick: i64, sent: bool) -> bool {
        sent && tick >= 0
            && match &self.brain {
                Brain::Human { state, .. } => state.wants_view(tick as u64),
                Brain::Horde(state) => state.wants_view(sent),
            }
    }
}

impl Match {
    fn commander_binding(&self, owner: usize) -> Result<(u64, usize, usize, usize, Point), String> {
        let player = self
            .start
            .players
            .get(owner)
            .ok_or("invalid commander owner")?;
        if self.world.game.players.len() != self.start.players.len() {
            return Err("invalid commander players".into());
        }
        Ok((
            self.options.seed,
            self.world.game.w,
            self.world.game.h,
            self.start.players.len(),
            player.spawn,
        ))
    }
    /// Select the scripted director only for the internal Horde seat, regardless
    /// of a requested player difficulty. Ordinary seats retain the human policy.
    pub fn create_seat_commander(
        &self,
        owner: usize,
        level: Level,
    ) -> Result<SeatCommander, String> {
        let binding = self.commander_binding(owner)?;
        let brain = if self
            .horde_commander_live()
            .wave
            .is_some_and(|wave| wave.slot == owner)
        {
            Brain::Horde(HordeCommander::new(owner))
        } else {
            Brain::Human {
                state: Box::new(HumanState::new(
                    self.options.seed as u32,
                    owner as u32,
                    binding.4,
                )),
                policy: Box::new(HumanPolicy::new(owner as u32, self.options.seed)),
                delivered: None,
            }
        };
        Ok(SeatCommander {
            owner,
            level,
            binding,
            brain,
        })
    }
    /// Read live stop/wave controls every callback. A new packet is supplied only
    /// on a delivery beat; intervening ticks retain the previous packet.
    /// Results are attempts for the existing admission path, never direct mutation.
    pub fn advance_seat_commander(
        &self,
        seat: &mut SeatCommander,
        delivered: Option<Delivered>,
        generation: u64,
        shots: &[HumanShot],
    ) -> Result<Advance, String> {
        if seat.binding != self.commander_binding(seat.owner)? {
            return Err("commander belongs to another match".into());
        }
        let level = match seat.level {
            Level::Easy => "easy",
            Level::Normal => "normal",
            Level::Hard => "hard",
        };
        let control: TickControl<'_> = self.human_commander_control(seat.owner, level)?;
        let result = match &mut seat.brain {
            Brain::Human {
                state,
                policy,
                delivered: previous,
            } => {
                // Stop before accepting new observations or reading alerts.
                if control.live_owner_out || control.winner {
                    return Ok(Advance::Human(HumanAdvance::default()));
                }
                let next_view = if let Some(delivered) = delivered {
                    let Delivered::Human(view) = delivered else {
                        return Err("human commander requires a human observation".into());
                    };
                    if view.packet.input().slot != seat.owner || view.tick > control.now {
                        return Err(
                            "human observation belongs to another seat or future tick".into()
                        );
                    }
                    Some(view)
                } else {
                    None
                };
                let hurts = self.human_commander_hurts(seat.owner, generation, shots)?;
                let advance = human_commander_policy::advance(
                    state,
                    policy,
                    control,
                    next_view.as_deref().or(previous.as_deref()),
                    &hurts,
                )
                .map_err(|e| format!("human commander: {e:?}"))?;
                if let Some(view) = next_view {
                    *previous = Some(view);
                }
                Advance::Human(advance)
            }
            Brain::Horde(state) => {
                let live = self.horde_commander_live();
                let mut commands = Vec::new();
                if let Some(delivered) = delivered {
                    let Delivered::Horde(view) = delivered else {
                        return Err("Horde commander requires a Horde observation".into());
                    };
                    commands.extend(state.observe(live, view)?);
                }
                commands.extend(state.act(live)?);
                Advance::Horde(commands)
            }
        };
        Ok(result)
    }
}

mod saves;
