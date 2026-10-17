//! Recipient focus decisions and ordinary human input scheduling.
use std::collections::{BTreeMap, BTreeSet};

use crate::{
    commander_ai::{Level, Observation},
    commander_attack_policy::{FocusFacts, FocusState, OwnFocusOrders, focus_plan},
    human_commander::{HumanView, OwnerHurtBatch, ScreenView, TickControl},
    human_commander_state::{Error, HumanAdvance, HumanState},
    match_commands::{Command, MoveOrder},
    match_human_observation::{HumanObservationMemory, human_screen_observation},
    match_runtime::Match,
    math, vision,
};
use serde_json::{Value, json};

#[derive(Clone, Debug, PartialEq)]
pub struct FocusPacket {
    observation: Observation,
    facts: FocusFacts,
    commanded_targets: BTreeMap<u32, u32>,
}
impl FocusPacket {
    pub fn observation(&self) -> &Observation {
        &self.observation
    }
    pub fn facts(&self) -> &FocusFacts {
        &self.facts
    }
}

impl Match {
    /// Own orders and received sight lines supplement the ordinary recipient packet.
    pub fn human_focus_observation(
        &mut self,
        memory: &mut HumanObservationMemory,
    ) -> Result<HumanView<FocusPacket>, String> {
        let mut staged = memory.clone();
        let view = self.human_commander_observation(&mut staged)?;
        let owner = view.packet.input().slot;
        let received = crate::commander_received_vision::received_game(self, &view.packet)?;
        let mut commanded_targets = BTreeMap::new();
        let mut facts = FocusFacts {
            mode: Some(self.options.mode),
            ..Default::default()
        };
        let team = view.packet.input().players[owner].team;
        let enemies: Vec<_> = view
            .packet
            .input()
            .units
            .iter()
            .filter(|u| view.packet.input().players[u.owner].team != team)
            .collect();
        for row in view
            .packet
            .input()
            .units
            .iter()
            .filter(|u| u.owner == owner)
        {
            let canonical = self
                .world
                .game
                .units
                .get(&row.id)
                .ok_or("missing own focus actor")?;
            let kind = self
                .rules
                .kinds
                .get(canonical.kind)
                .ok_or("missing own focus kind")?;
            if kind.structure {
                continue;
            }
            let control = self
                .commands
                .units
                .get(&row.id)
                .ok_or("missing own focus orders")?;
            if control.attack_id != 0 {
                commanded_targets.insert(row.id, control.attack_id);
            }
            let human = view
                .units
                .iter()
                .find(|u| u.motion.id == row.id)
                .ok_or("missing delivered own focus motion")?;
            let queued_amove = control.orders.iter().find_map(|command| match command {
                Command::Move {
                    orders,
                    attack_move: true,
                    ..
                } => orders
                    .iter()
                    .find(|order| order.unit == i64::from(row.id))
                    .map(|order| crate::game::Point {
                        x: math::round(order.at.x * 10.0) / 10.0,
                        z: math::round(order.at.z * 10.0) / 10.0,
                    }),
                _ => None,
            });
            facts.owned.insert(
                row.id,
                OwnFocusOrders {
                    attack_target: row.attack_id.then_some(control.attack_id),
                    amove: human.motion.amove,
                    queued_amove,
                },
            );
            for target in &enemies {
                if vision::los(
                    &received,
                    &self.rules,
                    (row.at.x, row.at.z),
                    (target.at.x, target.at.z),
                ) {
                    facts.clear_lines.insert((row.id, target.id));
                }
            }
        }
        *memory = staged;
        Ok(HumanView {
            tick: view.tick,
            spawn: view.spawn,
            units: view.units,
            visible: view.visible,
            sightings: view.sightings,
            packet: FocusPacket {
                observation: view.packet,
                facts,
                commanded_targets,
            },
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct FocusPolicy {
    owner: u32,
    tick: Option<u64>,
    noticed: BTreeMap<u32, (u64, u64)>,
    focus: FocusState,
}
impl FocusPolicy {
    pub fn new(owner: u32) -> Self {
        Self {
            owner,
            tick: None,
            noticed: BTreeMap::new(),
            focus: FocusState::default(),
        }
    }
    fn plan(
        &mut self,
        screen: ScreenView<'_, FocusPacket>,
        level: Level,
    ) -> Result<Vec<Value>, Error> {
        if screen.packet.observation.input().slot != self.owner as usize
            || self.tick.is_some_and(|tick| screen.tick < tick)
        {
            return Err(Error::Planner("focus policy seat or clock mismatch".into()));
        }
        let observation = human_screen_observation(&ScreenView {
            tick: screen.tick,
            spawn: screen.spawn,
            units: screen.units,
            visible: screen.visible,
            sightings: screen.sightings,
            packet: &screen.packet.observation,
        })
        .map_err(Error::Planner)?;
        let input = observation.input();
        let team = input.players[input.slot].team;
        let enemies: BTreeSet<_> = input
            .units
            .iter()
            .filter(|u| input.players[u.owner].team != team)
            .map(|u| u.id)
            .collect();
        let mut facts = screen.packet.facts.clone();
        facts.watched.clear();
        facts
            .clear_lines
            .retain(|(_, target)| enemies.contains(target));
        let mut next = self.clone();
        next.tick = Some(screen.tick);
        next.noticed
            .retain(|_, (_, last)| screen.tick.saturating_sub(*last) <= 1200);
        let notice = match level {
            Level::Easy => 50,
            Level::Normal => 25,
            Level::Hard => 10,
        };
        for id in &enemies {
            let seen = next
                .noticed
                .entry(*id)
                .or_insert((screen.tick, screen.tick));
            seen.1 = screen.tick;
            if screen.tick - seen.0 >= notice {
                facts.watched.insert(*id);
            }
        }
        // A masked attack ID cannot carry a hidden target into detached facts. A previously
        // issued focus can still be released using its own queued destination or current place.
        let mut lost = Vec::new();
        if level == Level::Hard {
            for (id, target) in &next.focus.targets {
                if enemies.contains(target) {
                    continue;
                }
                let Some(row) = input
                    .units
                    .iter()
                    .find(|u| u.id == *id && u.owner == input.slot)
                else {
                    continue;
                };
                // A recipient-visible target filtered by attention is handled once by focus_plan.
                if row.attack_id {
                    continue;
                }
                let Some(orders) = facts.owned.get(id) else {
                    continue;
                };
                if screen.packet.commanded_targets.get(id) == Some(target) {
                    lost.push(MoveOrder {
                        unit: i64::from(*id),
                        at: orders.queued_amove.unwrap_or(row.at),
                    });
                }
            }
        }
        let mut commands = Vec::new();
        if !lost.is_empty() {
            commands.push(Command::Move {
                orders: lost,
                control_ids: None,
                attack_move: true,
                queue: false,
                together: false,
                face: None,
            });
        }
        commands.extend(
            focus_plan(&observation, &facts, &mut next.focus, level).map_err(Error::Planner)?,
        );
        let raw = commands
            .iter()
            .map(raw_focus)
            .collect::<Result<Vec<_>, _>>()?;
        *self = next;
        Ok(raw)
    }
}

fn raw_focus(command: &Command) -> Result<Value, Error> {
    match command {
        Command::Attack {
            ids,
            target,
            queue,
            at: None,
        } => Ok(json!({"t":"attack","ids":ids,"target":target,"queue":queue})),
        Command::Move {
            orders,
            attack_move: true,
            queue,
            ..
        } => Ok(json!({"t":"amove",
            "orders":orders.iter().map(|o| json!([o.unit,o.at.x,o.at.z])).collect::<Vec<_>>(),"queue":queue})),
        _ => Err(Error::Planner("non-focus command in focus policy".into())),
    }
}

/// Motor state and private focus memory publish together after a valid advance.
pub fn advance_focus(
    state: &mut HumanState,
    policy: &mut FocusPolicy,
    control: TickControl<'_>,
    view: Option<&HumanView<FocusPacket>>,
    hurts: &OwnerHurtBatch,
) -> Result<HumanAdvance, Error> {
    let level = match control.level {
        "easy" => Level::Easy,
        "hard" => Level::Hard,
        _ => Level::Normal,
    };
    let mut next_state = state.clone();
    let mut next_policy = policy.clone();
    let result = next_state.advance(control, view, hurts, |screen| {
        next_policy.plan(screen, level)
    })?;
    *state = next_state;
    *policy = next_policy;
    Ok(result)
}

#[cfg(test)]
#[path = "commander_focus_runtime_tests.rs"]
mod tests;
